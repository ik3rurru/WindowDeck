//! Admission of connections: only paired devices reach the WindowDeck protocol.
//! See docs/adr/0022-pairing-and-encryption.md.
use super::{AnyError, publish_state, stop_requested};
use std::io::{self, BufRead, IsTerminal};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc;
use windowdeck_diagnostics::{Level, emit};
use windowdeck_transport::pairing::{self, Outcome, Prompt, Request};
use windowdeck_transport::secure::{self, Accepted, Control, Incoming};
use windowdeck_transport::{
    Identity, Intent, Peer, Reject, Role, SecureError, SecureStream, Trust,
};

/// Failed ceremonies tolerated before pairing is refused until the host restarts.
const MAX_FAILED_PAIRINGS: u8 = 3;
pub const LEGACY_CLIENT_ERROR: u16 = 3;
const LEGACY_MESSAGE: &str = "Este PC exige emparejamiento y conexión cifrada. Actualiza WindowDeck en la Steam Deck a la misma versión que el PC.";

pub struct Security {
    trust: Trust,
    identity: Identity,
    name: String,
    approval: Approval,
    failed_pairings: u8,
}

/// Where a person confirms pairing requests.
enum Approval {
    /// The panel answers through this file (WINDOWDECK_PAIRING_FILE).
    Panel(PathBuf),
    Console(Option<mpsc::Receiver<String>>),
    Unavailable,
}

pub enum Admission {
    Session(SecureStream, Peer),
    Closed,
}

impl Security {
    pub fn load() -> Result<Self, AnyError> {
        let trust = Trust::open_default(Role::Host)?;
        let identity = trust.identity()?;
        let approval = match std::env::var_os("WINDOWDECK_PAIRING_FILE") {
            Some(path) => Approval::Panel(PathBuf::from(path)),
            None if io::stdin().is_terminal() => Approval::Console(None),
            None => Approval::Unavailable,
        };
        emit(
            Level::Info,
            "host_identity",
            &[
                ("fingerprint", &identity.public().fingerprint()),
                ("paired_devices", &trust.devices()?.len().to_string()),
                (
                    "pairing",
                    match approval {
                        Approval::Panel(_) => "panel",
                        Approval::Console(_) => "console",
                        Approval::Unavailable => "unavailable",
                    },
                ),
            ],
        );
        Ok(Self {
            trust,
            identity,
            name: windowdeck_transport::trust::local_name(),
            approval,
            failed_pairings: 0,
        })
    }

    /// Completes the encrypted handshake and applies the pairing policy. Nothing
    /// in the WindowDeck protocol, and no display or capture, starts before this.
    pub fn admit(&mut self, socket: TcpStream) -> Admission {
        let accepted = match secure::accept(socket, &self.identity, &self.name) {
            Ok(Incoming::Secure(accepted)) => *accepted,
            Ok(Incoming::Legacy(legacy)) => {
                emit(
                    Level::Warn,
                    "legacy_client_rejected",
                    &[(
                        "peer",
                        &legacy
                            .peer_addr()
                            .map_or_else(|_| "?".into(), |a| a.to_string()),
                    )],
                );
                publish_state("legacy_client");
                if let Err(error) = legacy.reject(LEGACY_CLIENT_ERROR, LEGACY_MESSAGE) {
                    emit(
                        Level::Warn,
                        "legacy_client_reply_failed",
                        &[("error", &error.to_string())],
                    );
                }
                return Admission::Closed;
            }
            // A client that does not recognise this PC leaves after learning its
            // key and then asks to pair on a new connection.
            Err(SecureError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset
                ) =>
            {
                emit(
                    Level::Info,
                    "secure_handshake_abandoned",
                    &[("error", &error.to_string())],
                );
                return Admission::Closed;
            }
            Err(error) => {
                emit(
                    Level::Warn,
                    "secure_handshake_failed",
                    &[("error", &error.to_string())],
                );
                return Admission::Closed;
            }
        };
        match self.decide(accepted) {
            Ok(admission) => admission,
            Err(error) => {
                emit(
                    Level::Warn,
                    "admission_failed",
                    &[("error", &error.to_string())],
                );
                Admission::Closed
            }
        }
    }

    fn decide(&mut self, accepted: Accepted) -> Result<Admission, AnyError> {
        let Accepted {
            mut stream,
            peer,
            intent,
            knows_host,
            transcript,
        } = accepted;
        let fingerprint = peer.key.fingerprint();
        let paired = self.trust.is_paired(&peer.key)?;
        match intent {
            Intent::Session if paired => {
                stream.send_control(&Control::Accepted)?;
                emit(
                    Level::Info,
                    "client_authenticated",
                    &[("name", &peer.name), ("fingerprint", &fingerprint)],
                );
                Ok(Admission::Session(stream, peer))
            }
            Intent::Session => {
                emit(
                    Level::Warn,
                    "unpaired_client_rejected",
                    &[("name", &peer.name), ("fingerprint", &fingerprint)],
                );
                stream.send_control(&Control::Rejected(Reject::NotPaired))?;
                Ok(Admission::Closed)
            }
            Intent::Pair if paired && knows_host => {
                stream.send_control(&Control::Accepted)?;
                Ok(Admission::Closed)
            }
            Intent::Pair => {
                self.pair(stream, &transcript, &peer)?;
                Ok(Admission::Closed)
            }
        }
    }

    fn pair(
        &mut self,
        mut stream: SecureStream,
        transcript: &[u8; secure::TRANSCRIPT_LEN],
        peer: &Peer,
    ) -> Result<(), AnyError> {
        let fingerprint = peer.key.fingerprint();
        let refusal = if self.failed_pairings >= MAX_FAILED_PAIRINGS {
            Some(Reject::TooManyAttempts)
        } else if matches!(self.approval, Approval::Unavailable) {
            Some(Reject::PairingUnavailable)
        } else {
            None
        };
        if let Some(reason) = refusal {
            emit(
                Level::Warn,
                "pairing_refused",
                &[
                    ("name", &peer.name),
                    ("fingerprint", &fingerprint),
                    ("reason", reason.message()),
                ],
            );
            stream.send_control(&Control::Rejected(reason))?;
            return Ok(());
        }
        stream.send_control(&Control::PairingRequired)?;
        emit(
            Level::Info,
            "pairing_requested",
            &[("name", &peer.name), ("fingerprint", &fingerprint)],
        );
        let mut prompt: Box<dyn HostPrompt> = match &mut self.approval {
            Approval::Panel(path) => Box::new(PanelPrompt::new(path.clone())?),
            Approval::Console(lines) => Box::new(ConsolePrompt::new(lines)),
            Approval::Unavailable => unreachable!("refused above"),
        };
        let result = pairing::pair_as_host(
            &mut stream,
            transcript,
            peer,
            prompt.as_prompt(),
            pairing::TIMEOUT,
        );
        let outcome = match &result {
            Ok(Outcome::Paired) => {
                self.trust.add(peer.key, &peer.name)?;
                self.failed_pairings = 0;
                "paired"
            }
            Ok(Outcome::RejectedHere) => "rejected",
            Ok(Outcome::RejectedByPeer) => "rejected_by_device",
            Ok(Outcome::Expired) => "expired",
            Ok(Outcome::Cancelled) => "cancelled",
            Err(_) => "failed",
        };
        if outcome != "paired" && outcome != "cancelled" {
            self.failed_pairings = self.failed_pairings.saturating_add(1);
        }
        prompt.finish(outcome);
        let mut fields = vec![
            ("name", peer.name.clone()),
            ("fingerprint", fingerprint),
            ("outcome", outcome.into()),
        ];
        if let Err(error) = &result {
            fields.push(("error", error.to_string()));
        }
        let fields: Vec<_> = fields
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        emit(
            if outcome == "paired" {
                Level::Info
            } else {
                Level::Warn
            },
            "pairing_finished",
            &fields,
        );
        publish_state("listening");
        Ok(())
    }
}

trait HostPrompt {
    fn as_prompt(&mut self) -> &mut dyn Prompt;
    fn finish(&mut self, outcome: &str);
}

/// Shows the request in the panel. The panel replaces the decision file with
/// `ID accept` or `ID reject`; the random ID ignores stale answers.
struct PanelPrompt {
    file: PathBuf,
    id: String,
}

impl PanelPrompt {
    fn new(file: PathBuf) -> io::Result<Self> {
        let mut id = [0_u8; 6];
        getrandom::fill(&mut id).map_err(|error| io::Error::other(error.to_string()))?;
        let _ = std::fs::remove_file(&file);
        Ok(Self {
            file,
            id: id.iter().map(|byte| format!("{byte:02x}")).collect(),
        })
    }
}

impl Prompt for PanelPrompt {
    fn show(&mut self, request: &Request) -> io::Result<()> {
        publish_line(&pairing_line(&self.id, request));
        Ok(())
    }

    fn decision(&mut self) -> io::Result<Option<bool>> {
        let answer = match std::fs::read_to_string(&self.file) {
            Ok(answer) => answer,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let decision = match answer.trim().split_once(' ') {
            Some((id, "accept")) if id == self.id => Some(true),
            Some((id, "reject")) if id == self.id => Some(false),
            _ => None,
        };
        if decision.is_some() {
            let _ = std::fs::remove_file(&self.file);
        }
        Ok(decision)
    }

    fn peer_decided(&mut self, accepted: bool) {
        publish_line(&format!(
            "windowdeck_pairing_peer={} {}",
            self.id,
            if accepted { "accepted" } else { "rejected" }
        ));
    }

    fn cancelled(&mut self) -> bool {
        stop_requested()
    }
}

impl HostPrompt for PanelPrompt {
    fn as_prompt(&mut self) -> &mut dyn Prompt {
        self
    }

    fn finish(&mut self, outcome: &str) {
        let _ = std::fs::remove_file(&self.file);
        publish_line(&format!("windowdeck_pairing_result={} {outcome}", self.id));
    }
}

/// `windowdeck_pairing=ID CODE FINGERPRINT NAME`; the sanitized name is last
/// because it may contain spaces.
fn pairing_line(id: &str, request: &Request) -> String {
    format!(
        "windowdeck_pairing={id} {} {} {}",
        request.code.replace(' ', ""),
        request.peer.key.fingerprint(),
        request.peer.name
    )
}

/// For a host started from a terminal without the panel.
struct ConsolePrompt<'a> {
    lines: &'a mut Option<mpsc::Receiver<String>>,
}

impl<'a> ConsolePrompt<'a> {
    fn new(lines: &'a mut Option<mpsc::Receiver<String>>) -> Self {
        // A single reader thread outlives each request; stale lines are discarded.
        let receiver = lines.get_or_insert_with(|| {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                for line in io::stdin().lock().lines() {
                    let Ok(line) = line else { break };
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            receiver
        });
        while receiver.try_recv().is_ok() {}
        Self { lines }
    }
}

impl Prompt for ConsolePrompt<'_> {
    fn show(&mut self, request: &Request) -> io::Result<()> {
        eprintln!(
            "\nSolicitud de emparejamiento de «{}» (huella {}).\nCódigo: {}\nComprueba que la Deck muestra el mismo código. Escribe s y pulsa Intro para aceptar, o n para rechazar:",
            request.peer.name,
            request.peer.key.fingerprint(),
            request.code
        );
        Ok(())
    }

    fn decision(&mut self) -> io::Result<Option<bool>> {
        let Some(lines) = self.lines.as_ref() else {
            return Ok(None);
        };
        Ok(match lines.try_recv() {
            Ok(line) => Some(matches!(
                line.trim().to_lowercase().as_str(),
                "s" | "si" | "sí" | "y" | "yes"
            )),
            Err(_) => None,
        })
    }

    fn peer_decided(&mut self, accepted: bool) {
        if accepted {
            eprintln!("La Deck ha confirmado el código.");
        }
    }

    fn cancelled(&mut self) -> bool {
        stop_requested()
    }
}

impl HostPrompt for ConsolePrompt<'_> {
    fn as_prompt(&mut self) -> &mut dyn Prompt {
        self
    }

    fn finish(&mut self, outcome: &str) {
        eprintln!(
            "{}",
            match outcome {
                "paired" => "Dispositivo emparejado.",
                "expired" => "La solicitud caducó.",
                _ => "Emparejamiento no completado.",
            }
        );
    }
}

fn publish_line(line: &str) {
    use std::io::Write;
    let mut output = io::stdout().lock();
    let _ = writeln!(output, "{line}");
    let _ = output.flush();
}

/// `windowdeck-host devices ...`: list or revoke paired devices.
pub fn devices(command: &super::DevicesCommand) -> Result<(), AnyError> {
    let trust = Trust::open_default(Role::Host)?;
    match command {
        super::DevicesCommand::List => {
            let devices = trust.devices()?;
            if devices.is_empty() {
                println!("No hay dispositivos emparejados.");
            }
            for device in devices {
                println!(
                    "{}  {}  emparejado el {}",
                    device.key.fingerprint(),
                    device.name,
                    device.paired_on()
                );
            }
            println!(
                "Huella de este PC: {}",
                trust.identity()?.public().fingerprint()
            );
        }
        super::DevicesCommand::Remove(selector) => {
            let removed = trust.remove(selector)?;
            println!(
                "Revocado {} ({}). Deberá emparejarse de nuevo.",
                removed.name,
                removed.key.fingerprint()
            );
        }
        super::DevicesCommand::Clear => {
            let count = trust.clear()?;
            println!("Revocados {count} dispositivos.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windowdeck_transport::PublicKey;

    #[test]
    fn pairing_lines_keep_spaced_names_last_and_codes_compact() {
        let request = Request {
            code: "123 456".into(),
            peer: Peer {
                key: PublicKey::from_slice(&[3; 32]).unwrap(),
                name: "Steam Deck de Ana".into(),
            },
        };
        let line = pairing_line("abc123", &request);
        let fields: Vec<_> = line
            .strip_prefix("windowdeck_pairing=")
            .unwrap()
            .splitn(4, ' ')
            .collect();
        assert_eq!(fields[0], "abc123");
        assert_eq!(fields[1], "123456");
        assert_eq!(fields[2], request.peer.key.fingerprint());
        assert_eq!(fields[3], "Steam Deck de Ana");
    }

    #[test]
    fn panel_answers_must_match_the_current_request() {
        let dir = std::env::temp_dir().join(format!("windowdeck-panel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pairing-decision");
        std::fs::write(&file, "stale accept").unwrap();
        let mut prompt = PanelPrompt::new(file.clone()).unwrap();
        assert!(!file.exists(), "stale answers are discarded");
        assert_eq!(prompt.decision().unwrap(), None);
        std::fs::write(&file, "otro accept\n").unwrap();
        assert_eq!(prompt.decision().unwrap(), None);
        std::fs::write(&file, format!("{} accept\n", prompt.id)).unwrap();
        assert_eq!(prompt.decision().unwrap(), Some(true));
        assert!(!file.exists());
        std::fs::write(&file, format!("{} reject", prompt.id)).unwrap();
        assert_eq!(prompt.decision().unwrap(), Some(false));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
