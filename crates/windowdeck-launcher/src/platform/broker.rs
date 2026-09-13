use super::{
    Layout, error, identifier, prefixed_arg,
    process::{OwnedProcess, POLL, STOP_GRACE},
    system_program,
};
use crate::cli::BrokerOptions;
use std::{
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::Path,
    process::ExitStatus,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

type Completion = Receiver<io::Result<ExitStatus>>;

pub(super) struct Connection {
    stream: TcpStream,
    finished: Completion,
}

fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

impl Connection {
    pub fn start(
        layout: &Layout,
        session: &Path,
        native: bool,
        cancel: &Arc<AtomicBool>,
    ) -> io::Result<Option<Self>> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let token = identifier()?;
        let args = [
            OsString::from("--broker"),
            listener.local_addr()?.to_string().into(),
            token.clone().into(),
            session.as_os_str().to_owned(),
            OsString::from(if native { "gpu" } else { "cpu" }),
        ];
        let executable = layout.launcher.clone();
        let (sender, finished) = mpsc::channel();
        thread::spawn(move || {
            let result = runas::Command::new(executable)
                .args(&args)
                .show(false)
                .gui(true)
                .status();
            let _ = sender.send(result);
        });
        // UAC can remain on the secure desktop while the user reads the request.
        // Cancellation closes this listener: a late approval cannot start a broker.
        let deadline = Instant::now() + Duration::from_secs(120);
        while !cancelled(cancel) {
            if let Ok(result) = finished.try_recv() {
                return Err(elevation_error(result));
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
                    stream.set_write_timeout(Some(Duration::from_millis(250)))?;
                    let mut supplied = [0; 32];
                    if stream.read_exact(&mut supplied).is_ok()
                        && supplied == token.as_bytes()
                        && !cancelled(cancel)
                    {
                        stream.write_all(b"A")?;
                        stream.set_nonblocking(true)?;
                        return Ok(Some(Self { stream, finished }));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL),
                Err(e) => return Err(e),
            }
            if Instant::now() >= deadline {
                return Err(error(
                    "Tiempo de espera agotado al solicitar el permiso del broker",
                ));
            }
        }
        Ok(None)
    }

    pub fn wait_ready(&mut self, cancel: &AtomicBool, session: &Path) -> io::Result<bool> {
        let deadline = Instant::now() + Duration::from_secs(45);
        while !cancelled(cancel) {
            match self.read()? {
                Some(b'R') => return Ok(true),
                Some(_) => return Err(broker_error(session)),
                None => {}
            }
            if Instant::now() >= deadline {
                return Err(error("Tiempo de espera agotado al iniciar el broker"));
            }
            thread::sleep(POLL);
        }
        Ok(false)
    }

    pub fn read(&mut self) -> io::Result<Option<u8>> {
        let mut byte = [0];
        match self.stream.read(&mut byte) {
            Ok(0) => Err(error("El broker se ha cerrado. Consulta los registros.")),
            Ok(_) => Ok(Some(byte[0])),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn stop(mut self, session: &Path) -> io::Result<()> {
        // Called only after the host has released its display lease (or its job).
        let _ = self.stream.write_all(b"S");
        let _ = self.stream.shutdown(Shutdown::Both);
        match self
            .finished
            .recv_timeout(STOP_GRACE + Duration::from_secs(5))
        {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(_) => Err(broker_error(session)),
            Err(_) => Err(error(
                "El gestor del broker no confirmó su cierre. Consulta los registros.",
            )),
        }
    }
}

fn elevation_error(result: io::Result<ExitStatus>) -> io::Error {
    match result {
        Err(e) if e.raw_os_error() == Some(1223) => {
            error("Inicio cancelado: no se concedió el permiso del broker.")
        }
        Err(e) => error(format!("No se pudo solicitar el permiso del broker: {e}")),
        Ok(_) => error("El gestor del broker terminó antes de iniciar. Consulta los registros."),
    }
}

fn broker_error(session: &Path) -> io::Error {
    error(
        fs::read_to_string(session.join("error.txt"))
            .unwrap_or_else(|_| "El broker falló. Consulta broker.err y firewall.log.".into()),
    )
}

pub fn run(options: BrokerOptions) -> io::Result<()> {
    if !is_elevated::is_elevated() {
        return Err(error("El gestor del broker requiere elevación"));
    }
    let mut stream = TcpStream::connect_timeout(&options.address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(options.token.as_bytes())?;
    let mut accepted = [0];
    stream.read_exact(&mut accepted)?;
    if accepted != *b"A" {
        return Err(error("El panel ya no acepta esta sesión"));
    }
    stream.set_nonblocking(true)?;
    let result = manage(&options, &mut stream);
    if let Err(e) = &result {
        let _ = fs::write(options.session.join("error.txt"), e.to_string());
        let _ = stream.write_all(b"E");
    }
    result
}

fn owner_stopped(stream: &mut TcpStream) -> bool {
    let mut byte = [0];
    !matches!(stream.read(&mut byte), Err(e) if e.kind() == io::ErrorKind::WouldBlock)
}

fn manage(options: &BrokerOptions, stream: &mut TcpStream) -> io::Result<()> {
    let layout = Layout::discover()?;
    if !configure_firewall(&layout, &options.session, stream)? || owner_stopped(stream) {
        return Ok(());
    }
    let mut brokers = Vec::new();
    let routes: &[(&str, &str)] = if options.native {
        &[
            ("--gpu-frame-broker", "broker"),
            ("--frame-broker", "cpu-broker"),
        ]
    } else {
        &[("--frame-broker", "broker")]
    };
    for (route, log) in routes {
        let mut command = layout.command(&layout.display)?;
        command.arg(route);
        brokers.push(OwnedProcess::spawn(
            &mut command,
            &options.session.join(format!("{log}.log")),
            &options.session.join(format!("{log}.err")),
        )?);
    }
    let started = Instant::now();
    let mut ready = false;
    loop {
        for child in &mut brokers {
            if child.status()?.is_some() {
                return Err(error(
                    "No se pudo mantener el broker. Consulta broker.err o cpu-broker.err.",
                ));
            }
        }
        if !ready && started.elapsed() >= Duration::from_millis(500) {
            stream.write_all(b"R")?;
            ready = true;
        }
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(1) if byte == *b"S" => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            // Loss of the panel closes the authenticated local connection.
            // Keep the historical grace period before dropping the broker jobs.
            _ => {
                thread::sleep(STOP_GRACE);
                return Ok(());
            }
        }
        thread::sleep(POLL);
    }
}

fn firewall_args(
    name: &str,
    protocol: &str,
    port: &str,
    host: &Path,
    update: bool,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "advfirewall",
        "firewall",
        if update { "set" } else { "add" },
        "rule",
    ]
    .map(OsString::from)
    .into();
    args.push(format!("name={name}").into());
    if update {
        args.push("new".into());
    }
    args.extend(
        [
            "dir=in",
            "action=allow",
            "enable=yes",
            "profile=private",
            "remoteip=localsubnet",
        ]
        .map(OsString::from),
    );
    args.extend([
        format!("protocol={protocol}").into(),
        format!("localport={port}").into(),
        prefixed_arg("program=", host),
    ]);
    args
}

fn configure_firewall(layout: &Layout, session: &Path, stream: &mut TcpStream) -> io::Result<bool> {
    for (name, protocol, port) in [
        ("WindowDeck-mDNS (private LAN)", "udp", "5353"),
        ("WindowDeck-Video (private LAN)", "tcp", "48150"),
    ] {
        let mut configured = false;
        for update in [true, false] {
            if owner_stopped(stream) {
                return Ok(false);
            }
            let mut command = std::process::Command::new(system_program("netsh.exe")?);
            command.args(firewall_args(name, protocol, port, &layout.host, update));
            let log = format!("firewall-{port}-{}", if update { "set" } else { "add" });
            let mut child = OwnedProcess::spawn(
                &mut command,
                &session.join(format!("{log}.log")),
                &session.join(format!("{log}.err")),
            )?;
            let mut stopped = false;
            let status = child.wait_until(Duration::from_secs(10), || {
                stopped = owner_stopped(stream);
                stopped
            });
            if stopped {
                return Ok(false);
            }
            let status = status?;
            if status.success() {
                configured = true;
                break;
            }
        }
        if !configured {
            return Err(error(format!(
                "No se pudo configurar {name}. Consulta los registros firewall-*.log."
            )));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firewall_rules_always_restrict_profile_subnet_and_executable() {
        let host = Path::new("C:\\una carpeta\\WindowDeck\\bin\\windowdeck-host.exe");
        for update in [true, false] {
            let args = firewall_args("WindowDeck-Video", "tcp", "48150", host, update);
            for restriction in [
                "dir=in",
                "action=allow",
                "profile=private",
                "remoteip=localsubnet",
                "protocol=tcp",
                "localport=48150",
            ] {
                assert!(args.contains(&restriction.into()));
            }
            assert_eq!(args.last().unwrap(), &prefixed_arg("program=", host));
        }
    }
    #[test]
    fn losing_the_owner_connection_requests_cleanup() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut owner, _) = listener.accept().unwrap();
        owner.set_nonblocking(true).unwrap();
        assert!(!owner_stopped(&mut owner));
        drop(peer);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !owner_stopped(&mut owner) {
            assert!(Instant::now() < deadline);
            thread::sleep(POLL);
        }
    }
}
