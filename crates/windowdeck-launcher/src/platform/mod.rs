mod broker;
mod process;
mod session;

pub use broker::run as broker;
pub use session::{Event, Worker, start};

use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::Command,
};

pub fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

#[derive(Clone)]
struct Layout {
    launcher: PathBuf,
    host: PathBuf,
    display: PathBuf,
    bin: PathBuf,
}

impl Layout {
    fn at(launcher: PathBuf) -> io::Result<Self> {
        let directory = launcher
            .parent()
            .ok_or_else(|| error("Ruta del lanzador inválida"))?;
        let bin = if directory.join("bin").is_dir() {
            directory.join("bin")
        } else {
            directory.to_owned()
        };
        let mut display = bin.join("windowdeck-display.exe");
        // The developer helper is built by MSBuild outside Cargo's profile folder.
        if !display.is_file()
            && let Some(target) = bin
                .parent()
                .filter(|p| p.file_name().is_some_and(|n| n == "target"))
        {
            display = target.join("windows-idd/windowdeck-display.exe");
        }
        Ok(Self {
            host: bin.join("windowdeck-host.exe"),
            launcher,
            bin,
            display,
        })
    }

    fn discover() -> io::Result<Self> {
        Self::at(std::env::current_exe()?)
    }

    fn command(&self, program: &Path) -> io::Result<Command> {
        let mut command = Command::new(program);
        let mut paths = vec![self.bin.clone()];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }
        command
            .env(
                "PATH",
                std::env::join_paths(paths).map_err(|e| error(e.to_string()))?,
            )
            .current_dir(&self.bin);
        Ok(command)
    }
}

fn identifier() -> io::Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| error(e.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn log_root() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("WINDOWDECK_LOG_DIR") {
        return std::path::absolute(path);
    }
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| error("No se encuentra la carpeta de datos del usuario"))?;
    Ok(PathBuf::from(local).join("WindowDeck/logs"))
}

fn system_program(name: &str) -> io::Result<PathBuf> {
    let windows = std::env::var_os("SystemRoot").ok_or_else(|| error("No se encuentra Windows"))?;
    Ok(PathBuf::from(windows).join("System32").join(name))
}

pub fn open_logs(path: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    let windows = std::env::var_os("SystemRoot").ok_or_else(|| error("No se encuentra Windows"))?;
    Command::new(PathBuf::from(windows).join("explorer.exe"))
        .arg(path)
        .creation_flags(process::CREATE_NO_WINDOW)
        .spawn()?;
    Ok(())
}

pub fn addresses() -> String {
    let mut addresses: Vec<_> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_link_local() && !ip.is_unspecified() => {
                Some(format!("{ip}:48150"))
            }
            _ => None,
        })
        .collect();
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty() {
        "Dirección del PC: sin red local disponible.".into()
    } else {
        format!("Dirección del PC: {}", addresses.join(", "))
    }
}

fn prefixed_arg(prefix: &str, path: &Path) -> OsString {
    let mut arg = OsString::from(prefix);
    arg.push(path);
    arg
}
