//! End-to-end admission policy of the real host binary on its synthetic route
//! (`diag pattern`), which needs no capture, driver or Windows.
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use windowdeck_protocol::{Message, VideoCodec, connection, read_message, write_message};
use windowdeck_transport::pairing::{self, Outcome, Prompt, Request};
use windowdeck_transport::secure::{self, Established};
use windowdeck_transport::{Intent, Reject, Role, SecureError, Trust};

const WAIT: Duration = Duration::from_secs(10);

fn temporary_dir(name: &str) -> PathBuf {
    let mut random = [0; 8];
    getrandom::fill(&mut random).unwrap();
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("windowdeck-host-e2e-{name}-{suffix}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Host {
    child: Child,
    address: SocketAddr,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
    dir: PathBuf,
}

impl Host {
    fn start(name: &str, panel: bool) -> Self {
        let dir = temporary_dir(name);
        let mut command = Command::new(env!("CARGO_BIN_EXE_windowdeck-host"));
        command
            .args(["diag", "pattern", "127.0.0.1:0"])
            .env("WINDOWDECK_DATA_DIR", dir.join("host"))
            .env("WINDOWDECK_STOP_FILE", dir.join("stop"))
            .env_remove("WINDOWDECK_PAIRING_FILE")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if panel {
            command.env("WINDOWDECK_PAIRING_FILE", dir.join("pairing-decision"));
        }
        let mut child = command.spawn().unwrap();
        let stdout = lines(child.stdout.take().unwrap());
        let stderr = lines(child.stderr.take().unwrap());
        let listening = wait_for(&stderr, |line| line.contains("event=host_listening"));
        let address = listening
            .split("address=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap()
            .parse()
            .unwrap();
        Self {
            child,
            address,
            stdout,
            stderr,
            dir,
        }
    }

    fn trust(&self) -> Trust {
        Trust::open(self.dir.join("host"), Role::Host).unwrap()
    }

    fn client(&self, name: &str) -> Trust {
        Trust::open(self.dir.join(name), Role::Client).unwrap()
    }

    /// A client that already trusts this host; `authorized` also registers it.
    fn known_client(&self, name: &str, authorized: bool) -> Trust {
        let client = self.client(name);
        client
            .add(self.trust().identity().unwrap().public(), "host")
            .unwrap();
        if authorized {
            self.trust()
                .add(client.identity().unwrap().public(), name)
                .unwrap();
        }
        client
    }

    fn connect(&self, client: &Trust, intent: Intent) -> Result<Established, SecureError> {
        let identity = client.identity().unwrap();
        secure::connect(
            TcpStream::connect(self.address).unwrap(),
            &identity,
            "steamdeck",
            intent,
            |key| client.is_paired(key),
        )
    }

    fn devices(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_windowdeck-host"))
            .arg("devices")
            .args(args)
            .env("WINDOWDECK_DATA_DIR", self.dir.join("host"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::write(self.dir.join("stop"), "");
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline && matches!(self.child.try_wait(), Ok(None)) {
            thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn lines(output: impl std::io::Read + Send + 'static) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

fn wait_for(lines: &Receiver<String>, matches: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = lines
            .recv_timeout(remaining)
            .expect("expected host output did not appear");
        if matches(&line) {
            return line;
        }
    }
}

/// Runs the protocol up to the first synthetic frame inside the channel.
fn stream_one_frame(mut stream: windowdeck_transport::SecureStream) {
    stream.set_read_timeout(Some(WAIT)).unwrap();
    write_message(
        &mut stream,
        &Message::Hello {
            app_version: "test".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        read_message(&mut stream).unwrap(),
        Message::Hello { .. }
    ));
    write_message(
        &mut stream,
        &Message::Capabilities {
            max_width: 128,
            max_height: 80,
            max_fps: 10,
            codecs: VideoCodec::Rgb332.capability() | connection::CAPABILITY,
        },
    )
    .unwrap();
    let Message::SessionConfig { session_id, .. } = read_message(&mut stream).unwrap() else {
        panic!("configuration expected");
    };
    let mut probes = connection::ProbeReceiver::new(session_id);
    loop {
        match read_message(&mut stream).unwrap() {
            Message::Start => break,
            message => {
                if let Some(reply) = probes.receive(message).unwrap() {
                    write_message(&mut stream, &reply).unwrap();
                }
            }
        }
    }
    assert!(matches!(
        read_message(&mut stream).unwrap(),
        Message::Frame { number: 0, .. }
    ));
}

#[test]
fn only_paired_clients_reach_the_protocol() {
    let host = Host::start("admission", false);
    let paired = host.known_client("paired", true);
    let Established::Session(stream, peer) = host.connect(&paired, Intent::Session).unwrap() else {
        panic!("paired client must get a session");
    };
    assert!(!peer.name.is_empty());
    stream_one_frame(stream);
    wait_for(&host.stdout, |line| line == "windowdeck_state=streaming");
    wait_for(&host.stdout, |line| line == "windowdeck_state=listening");

    // The host is known to this client, but the client is not authorized.
    let stranger = host.known_client("stranger", false);
    assert!(matches!(
        host.connect(&stranger, Intent::Session),
        Err(SecureError::Rejected(Reject::NotPaired))
    ));
    wait_for(&host.stderr, |line| {
        line.contains("event=unpaired_client_rejected")
    });

    // Without the panel or a terminal nobody can approve a request.
    assert!(matches!(
        host.connect(&stranger, Intent::Pair),
        Err(SecureError::Rejected(Reject::PairingUnavailable))
    ));

    let mut legacy = TcpStream::connect(host.address).unwrap();
    legacy.set_read_timeout(Some(WAIT)).unwrap();
    write_message(
        &mut legacy,
        &Message::Hello {
            app_version: "0.3.1".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        read_message(&mut legacy).unwrap(),
        Message::Error { code: 3, .. }
    ));
    wait_for(&host.stdout, |line| {
        line == "windowdeck_state=legacy_client"
    });
    // None of the refused connections negotiated a session.
    while let Ok(line) = host.stdout.recv_timeout(Duration::from_millis(200)) {
        assert_ne!(line, "windowdeck_state=negotiating");
    }
}

struct Accepting {
    shown: mpsc::Sender<String>,
}

impl Prompt for Accepting {
    fn show(&mut self, request: &Request) -> std::io::Result<()> {
        let _ = self.shown.send(request.code.clone());
        Ok(())
    }
    fn decision(&mut self) -> std::io::Result<Option<bool>> {
        Ok(Some(true))
    }
}

/// Starts a ceremony from a client thread; returns the code it shows.
fn request_pairing(host: &Host, client: &Trust) -> (Receiver<String>, thread::JoinHandle<Outcome>) {
    let Established::PairingRequired(pairing) = host.connect(client, Intent::Pair).unwrap() else {
        panic!("pairing required");
    };
    let client = client.clone();
    let (shown, codes) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut pairing = *pairing;
        let outcome = pairing::pair_as_client(
            &mut pairing.stream,
            &pairing.transcript,
            &pairing.peer,
            &mut Accepting { shown },
            pairing::TIMEOUT,
        )
        .unwrap();
        if outcome == Outcome::Paired {
            client.add(pairing.peer.key, &pairing.peer.name).unwrap();
        }
        outcome
    });
    (codes, handle)
}

fn answer(host: &Host, request: &str, decision: &str) {
    let id = request.split(' ').next().unwrap();
    let temporary = host.dir.join("pairing-decision.tmp");
    std::fs::write(&temporary, format!("{id} {decision}\n")).unwrap();
    std::fs::rename(temporary, host.dir.join("pairing-decision")).unwrap();
}

fn wait_request(host: &Host) -> String {
    wait_for(&host.stdout, |line| line.starts_with("windowdeck_pairing="))
        .trim_start_matches("windowdeck_pairing=")
        .to_string()
}

#[test]
fn the_panel_approves_pairing_and_revocation_takes_effect_while_running() {
    let host = Host::start("panel", true);
    let client = host.client("deck");
    let (codes, ceremony) = request_pairing(&host, &client);
    let request = wait_request(&host);
    let fields: Vec<_> = request.splitn(4, ' ').collect();
    assert_eq!(
        fields[1],
        codes.recv_timeout(WAIT).unwrap().replace(' ', "")
    );
    assert_eq!(fields[2], client.identity().unwrap().public().fingerprint());
    assert_eq!(fields[3], "steamdeck");
    answer(&host, &request, "accept");
    assert_eq!(ceremony.join().unwrap(), Outcome::Paired);
    wait_for(&host.stdout, |line| {
        line.starts_with("windowdeck_pairing_result=") && line.ends_with(" paired")
    });
    let listed = host.devices(&["list"]);
    let fingerprint = client.identity().unwrap().public().fingerprint();
    assert!(
        listed.contains(&fingerprint) && listed.contains("steamdeck"),
        "{listed}"
    );

    let Established::Session(stream, _) = host.connect(&client, Intent::Session).unwrap() else {
        panic!("a paired device gets a session");
    };
    stream_one_frame(stream);
    wait_for(&host.stdout, |line| line == "windowdeck_state=listening");

    assert!(
        host.devices(&["remove", &fingerprint[..4]])
            .contains("Revocado")
    );
    assert!(matches!(
        host.connect(&client, Intent::Session),
        Err(SecureError::Rejected(Reject::NotPaired))
    ));
}

#[test]
fn repeated_refusals_block_pairing_until_restart() {
    let host = Host::start("attempts", true);
    for attempt in 0..3 {
        let client = host.client(&format!("deck-{attempt}"));
        let (_codes, ceremony) = request_pairing(&host, &client);
        let request = wait_request(&host);
        answer(&host, &request, "reject");
        assert_eq!(ceremony.join().unwrap(), Outcome::RejectedByPeer);
        assert!(client.devices().unwrap().is_empty());
    }
    assert!(host.trust().devices().unwrap().is_empty());
    let late = host.client("late");
    assert!(matches!(
        host.connect(&late, Intent::Pair),
        Err(SecureError::Rejected(Reject::TooManyAttempts))
    ));
    assert!(!Path::new(&host.dir.join("pairing-decision")).exists());
}
