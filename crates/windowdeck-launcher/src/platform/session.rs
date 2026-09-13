use super::{
    Layout,
    broker::Connection,
    error, identifier, log_root,
    process::{OwnedProcess, POLL},
};
use crate::model::State;
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub enum Event {
    Session(PathBuf),
    State(State),
    Finished(Result<(), String>),
}

pub struct Worker {
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Worker {
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn finished(&self) -> bool {
        self.thread.is_finished()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start(native: bool) -> Worker {
    let (sender, events) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let thread = thread::spawn(move || {
        let result = run(native, &worker_cancel, &sender).map_err(|e| e.to_string());
        let _ = sender.send(Event::Finished(result));
    });
    Worker {
        events,
        cancel,
        thread,
    }
}

fn run(native: bool, cancel: &Arc<AtomicBool>, events: &Sender<Event>) -> io::Result<()> {
    let session = log_root()?.join(format!("launcher-{}", identifier()?));
    fs::create_dir_all(&session)?;
    let _ = events.send(Event::Session(session.clone()));
    let layout = Layout::discover()?;
    let stop = session.join("stop");
    let mut connection = None;
    let mut host = None;
    let outcome = (|| {
        preflight(&layout, &session, native, cancel)?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        connection = Connection::start(&layout, &session, native, cancel)?;
        let Some(connection) = connection.as_mut() else {
            return Ok(());
        };
        if !connection.wait_ready(cancel, &session)? {
            return Ok(());
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let _ = events.send(Event::State(State::HostStarting));
        let mut command = layout.command(&layout.host)?;
        command
            .args([
                if native {
                    "--driver-native-h264"
                } else {
                    "--driver-h264"
                },
                "0.0.0.0:48150",
            ])
            .env("WINDOWDECK_DISPLAY_EXE", &layout.display)
            .env("WINDOWDECK_STOP_FILE", &stop);
        host = Some(OwnedProcess::spawn(
            &mut command,
            &session.join("host.out"),
            &session.join("host.log"),
        )?);
        let host = host.as_mut().expect("host just started");
        let mut output = File::open(session.join("host.out"))?;
        let mut tail = StateTail::default();
        let started = Instant::now();
        let mut seen_state = false;
        while !cancel.load(Ordering::Relaxed) {
            if let Some(status) = host.status()? {
                return Err(error(format!(
                    "El host se ha cerrado ({status}). Consulta host.log."
                )));
            }
            if connection.read()?.is_some() {
                return Err(error("El broker falló. Consulta los registros."));
            }
            let mut chunk = [0; 4096];
            let count = output.read(&mut chunk)?;
            for state in tail.push(&chunk[..count]) {
                seen_state = true;
                let _ = events.send(Event::State(state));
            }
            if !seen_state && started.elapsed() > Duration::from_secs(15) {
                return Err(error("El host no confirmó su arranque. Consulta host.log."));
            }
            thread::sleep(POLL);
        }
        Ok(())
    })();
    // Always release the host before the elevated brokers, including startup errors.
    let cleanup = host.as_mut().map_or(Ok(()), |host| host.stop(&stop));
    drop(host);
    let broker_cleanup = connection.map_or(Ok(()), |connection| connection.stop(&session));
    let outcome = if cancel.load(Ordering::Relaxed) {
        Ok(())
    } else {
        outcome
    };
    let result = outcome.and(cleanup).and(broker_cleanup);
    if let Err(e) = &result {
        let _ = fs::write(session.join("launcher-error.txt"), e.to_string());
    }
    result
}

fn preflight(layout: &Layout, session: &Path, native: bool, cancel: &AtomicBool) -> io::Result<()> {
    if is_elevated::is_elevated() {
        return Err(error(
            "Abre WindowDeck sin ejecutar como administrador. Solo el broker necesita elevación.",
        ));
    }
    for file in [
        &layout.host,
        &layout.display,
        &layout.bin.join("ffmpeg.exe"),
    ] {
        if !file.is_file() {
            return Err(error(format!(
                "Falta {}. Usa el paquete completo de WindowDeck o compila release.",
                file.display()
            )));
        }
    }
    let mut command = layout.command(&layout.host)?;
    command.arg("--version");
    let mut probe = OwnedProcess::spawn(
        &mut command,
        &session.join("host-version.txt"),
        &session.join("host-version.err"),
    )?;
    let status = probe.wait_until(Duration::from_secs(5), || cancel.load(Ordering::Relaxed))?;
    if !status.success() {
        return Err(error(
            "No se pudo abrir el host. Comprueba que sus DLL están junto al ejecutable.",
        ));
    }
    let version = fs::read_to_string(session.join("host-version.txt"))?;
    if native
        && !version
            .lines()
            .any(|line| line.trim() == "native_media=true")
    {
        return Err(error(
            "La ruta GPU experimental requiere compilar con native-media.",
        ));
    }
    Ok(())
}

#[derive(Default)]
struct StateTail {
    line: Vec<u8>,
    overlong: bool,
}

impl StateTail {
    fn push(&mut self, bytes: &[u8]) -> Vec<State> {
        let mut states = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                if !self.overlong
                    && let Ok(line) = std::str::from_utf8(&self.line)
                    && let Some(state) = State::from_host(line)
                {
                    states.push(state);
                }
                self.line.clear();
                self.overlong = false;
            } else if self.line.len() < 4096 {
                self.line.push(byte);
            } else {
                self.overlong = true;
            }
        }
        states
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_stdout_records_are_preserved_and_oversized_lines_ignored() {
        let mut tail = StateTail::default();
        assert!(tail.push(b"windowdeck_state=stream").is_empty());
        assert_eq!(tail.push(b"ing\r\n"), vec![State::Streaming]);
        assert!(tail.push(&vec![b'x'; 8192]).is_empty());
        assert_eq!(
            tail.push(b"\nwindowdeck_state=listening\n"),
            vec![State::Listening]
        );
        assert_eq!(
            tail.push(b"noise\nwindowdeck_state=negotiating\n"),
            vec![State::Negotiating]
        );
    }
}
