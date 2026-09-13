use command_group::{CommandGroup, GroupChild};
use std::{
    fs::File,
    io,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const CREATE_NO_WINDOW: u32 = 0x08000000;
pub const POLL: Duration = Duration::from_millis(100);
pub const STOP_GRACE: Duration = Duration::from_secs(8);

pub struct OwnedProcess(GroupChild);

impl OwnedProcess {
    pub fn spawn(command: &mut Command, stdout: &Path, stderr: &Path) -> io::Result<Self> {
        command
            .stdin(Stdio::null())
            .stdout(File::create(stdout)?)
            .stderr(File::create(stderr)?);
        // The kernel closes the job even if the launcher is terminated, including
        // the host's encoder and frame-reader descendants. No lookup by PID/name.
        command
            .group()
            .kill_on_drop(true)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(Self)
    }

    pub fn status(&mut self) -> io::Result<Option<ExitStatus>> {
        self.0.inner().try_wait()
    }

    pub fn wait_until(
        &mut self,
        timeout: Duration,
        mut cancel: impl FnMut() -> bool,
    ) -> io::Result<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.status()? {
                return Ok(status);
            }
            if cancel() {
                return Err(super::error("Operación cancelada"));
            }
            if Instant::now() >= deadline {
                return Err(super::error("Tiempo de espera agotado"));
            }
            thread::sleep(POLL);
        }
    }

    pub fn stop(&mut self, stop_file: &Path) -> io::Result<()> {
        let signal = std::fs::write(stop_file, b"stop\n");
        if signal.is_ok() && self.wait_until(STOP_GRACE, || false).is_ok() {
            return Ok(());
        }
        self.0.kill()?;
        self.0.inner().wait()?;
        signal
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        // kill() targets the whole job even when its original process has exited.
        let _ = self.0.kill();
        let _ = self.0.inner().wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "subprocess fixture, invoked by owns_children_without_touching_unrelated_processes"]
    fn child_fixture() {
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn owns_children_without_touching_unrelated_processes() {
        let directory = std::env::temp_dir().join(format!(
            "windowdeck-process-{}",
            super::super::identifier().unwrap()
        ));
        std::fs::create_dir(&directory).unwrap();
        let fixture = || {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args([
                "--exact",
                "platform::process::tests::child_fixture",
                "--ignored",
            ]);
            command
        };
        let mut owned = OwnedProcess::spawn(
            &mut fixture(),
            &directory.join("owned.out"),
            &directory.join("owned.err"),
        )
        .unwrap();
        let mut unrelated = OwnedProcess::spawn(
            &mut fixture(),
            &directory.join("other.out"),
            &directory.join("other.err"),
        )
        .unwrap();
        assert!(owned.status().unwrap().is_none());
        assert!(owned.wait_until(Duration::from_secs(2), || true).is_err());
        owned.0.kill().unwrap();
        assert!(!owned.0.inner().wait().unwrap().success());
        assert!(unrelated.status().unwrap().is_none());
        drop(owned);
        drop(unrelated);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
