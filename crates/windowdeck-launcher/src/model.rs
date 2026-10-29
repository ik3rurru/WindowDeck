use crate::cli::SessionMode;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Stopped,
    Starting,
    HostStarting,
    Listening,
    Negotiating,
    Validating,
    ValidationFailed,
    Activating,
    Unstable,
    LegacyClient,
    Streaming,
    Stopping,
}

impl State {
    pub fn from_host(line: &str) -> Option<Self> {
        match line.trim_end() {
            "windowdeck_state=listening" => Some(Self::Listening),
            "windowdeck_state=negotiating" => Some(Self::Negotiating),
            "windowdeck_state=validating" => Some(Self::Validating),
            "windowdeck_state=validation_failed" => Some(Self::ValidationFailed),
            "windowdeck_state=activating" => Some(Self::Activating),
            "windowdeck_state=unstable" => Some(Self::Unstable),
            "windowdeck_state=legacy_client" => Some(Self::LegacyClient),
            "windowdeck_state=streaming" => Some(Self::Streaming),
            _ => None,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Stopped => "Detenido. Pulsa Iniciar y abre WindowDeck en la Steam Deck.",
            Self::Starting => "Iniciando. Acepta el permiso del broker para continuar...",
            Self::HostStarting => "Iniciando el host...",
            Self::Listening => "Esperando Deck. Abre WindowDeck en la Steam Deck.",
            Self::Negotiating => "Preparando la conexión con la Deck...",
            Self::Validating => "Comprobando la estabilidad de la conexión...",
            Self::ValidationFailed => {
                "La conexión no superó la prueba de estabilidad. Revisa la red y vuelve a abrir el cliente."
            }
            Self::Activating => "Conexión preparada. Iniciando la pantalla y el vídeo...",
            Self::Unstable => {
                "Se pausaron los reintentos tras tres fallos seguidos. Revisa la red; pulsa Detener e Iniciar para volver a probar."
            }
            Self::LegacyClient => {
                "Se rechazó una Deck con una versión anterior de WindowDeck. Actualízala para usar la conexión cifrada."
            }
            Self::Streaming => "Deck conectada.",
            Self::Stopping => "Deteniendo y recuperando las ventanas...",
        }
    }
}

/// A pairing request announced by the host:
/// `windowdeck_pairing=ID CODE FINGERPRINT NAME`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingRequest {
    pub id: String,
    /// Grouped as shown on the Deck: `123 456`.
    pub code: String,
    pub fingerprint: String,
    pub name: String,
    pub peer_accepted: bool,
    pub answered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingResult {
    Paired,
    Rejected,
    RejectedByDevice,
    Expired,
    Cancelled,
    Failed,
}

/// Lines of the host's standard output that the panel acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostLine {
    State(State),
    Pairing(PairingRequest),
    PairingPeer { id: String, accepted: bool },
    PairingResult { id: String, result: PairingResult },
}

impl HostLine {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end();
        if let Some(state) = State::from_host(line) {
            return Some(Self::State(state));
        }
        if let Some(rest) = line.strip_prefix("windowdeck_pairing=") {
            let mut fields = rest.splitn(4, ' ');
            let (id, code, fingerprint, name) = (
                fields.next()?,
                fields.next()?,
                fields.next()?,
                fields.next()?,
            );
            let fingerprint_ok = fingerprint.len() == 19
                && fingerprint.chars().enumerate().all(|(index, c)| {
                    if index % 5 == 4 {
                        c == '-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                });
            if !valid_id(id)
                || code.len() != 6
                || !code.chars().all(|c| c.is_ascii_digit())
                || !fingerprint_ok
                || name.is_empty()
                || name.chars().any(char::is_control)
            {
                return None;
            }
            return Some(Self::Pairing(PairingRequest {
                id: id.into(),
                code: format!("{} {}", &code[..3], &code[3..]),
                fingerprint: fingerprint.into(),
                name: name.chars().take(40).collect(),
                peer_accepted: false,
                answered: false,
            }));
        }
        if let Some(rest) = line.strip_prefix("windowdeck_pairing_peer=") {
            let (id, verdict) = rest.split_once(' ')?;
            let accepted = match verdict {
                "accepted" => true,
                "rejected" => false,
                _ => return None,
            };
            return valid_id(id).then(|| Self::PairingPeer {
                id: id.into(),
                accepted,
            });
        }
        if let Some(rest) = line.strip_prefix("windowdeck_pairing_result=") {
            let (id, result) = rest.split_once(' ')?;
            let result = match result {
                "paired" => PairingResult::Paired,
                "rejected" => PairingResult::Rejected,
                "rejected_by_device" => PairingResult::RejectedByDevice,
                "expired" => PairingResult::Expired,
                "cancelled" => PairingResult::Cancelled,
                "failed" => PairingResult::Failed,
                _ => return None,
            };
            return valid_id(id).then(|| Self::PairingResult {
                id: id.into(),
                result,
            });
        }
        None
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 12 && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// The host polls this file for the panel's answer (WINDOWDECK_PAIRING_FILE).
pub fn pairing_file(session: &Path) -> PathBuf {
    session.join("pairing-decision")
}

/// Replaces the answer atomically so the host never reads half a line.
pub fn answer_pairing(session: &Path, id: &str, accept: bool) -> io::Result<()> {
    let target = pairing_file(session);
    let temporary = target.with_extension("tmp");
    std::fs::write(
        &temporary,
        format!("{id} {}\n", if accept { "accept" } else { "reject" }),
    )?;
    std::fs::rename(temporary, target)
}

#[derive(Default)]
pub struct Panel {
    pub state: State,
    pub mode: SessionMode,
    pub failure: Option<String>,
    pub pairing: Option<PairingRequest>,
    /// Outcome of the last pairing, shown until the connection moves on.
    pub notice: Option<String>,
}

impl Panel {
    pub fn start(&mut self) {
        self.state = State::Starting;
        self.failure = None;
        self.notice = None;
    }

    pub fn stop(&mut self) {
        self.state = State::Stopping;
        self.pairing = None;
    }

    pub fn update(&mut self, state: State) {
        // A late stdout event cannot undo cancellation or a completed session.
        if self.state != State::Stopping && self.state != State::Stopped {
            self.state = state;
            if state != State::Listening {
                self.notice = None;
            }
        }
    }

    /// Applies a pairing line; returns true when a new device was paired.
    pub fn pairing_event(&mut self, line: HostLine) -> bool {
        if matches!(self.state, State::Stopping | State::Stopped) {
            return false;
        }
        match line {
            HostLine::State(state) => self.update(state),
            HostLine::Pairing(request) => {
                self.notice = None;
                self.pairing = Some(request);
            }
            HostLine::PairingPeer { id, accepted } => {
                if let Some(request) = self.pairing.as_mut().filter(|request| request.id == id) {
                    request.peer_accepted = accepted;
                }
            }
            HostLine::PairingResult { id, result } => {
                let Some(request) = self.pairing.take_if(|request| request.id == id) else {
                    return false;
                };
                self.notice = Some(match result {
                    PairingResult::Paired => {
                        format!("«{}» está emparejada. Ya puede conectarse.", request.name)
                    }
                    PairingResult::Rejected => "Emparejamiento rechazado en el PC.".into(),
                    PairingResult::RejectedByDevice => {
                        "La Deck indicó que el código no coincide. No se ha emparejado.".into()
                    }
                    PairingResult::Expired => {
                        "La solicitud de emparejamiento caducó. Vuelve a abrir WindowDeck en la Deck.".into()
                    }
                    PairingResult::Cancelled => "Emparejamiento cancelado.".into(),
                    PairingResult::Failed => {
                        "El emparejamiento se interrumpió. Consulta los registros.".into()
                    }
                });
                return result == PairingResult::Paired;
            }
        }
        false
    }

    /// Records the person's answer; the request stays until the host reports.
    pub fn answer(&mut self) -> Option<PairingRequest> {
        let request = self.pairing.as_mut().filter(|request| !request.answered)?;
        request.answered = true;
        Some(request.clone())
    }

    pub fn finish(&mut self, result: Result<(), String>) {
        self.state = State::Stopped;
        self.failure = result.err();
        self.pairing = None;
    }

    pub fn text(&self) -> String {
        if let Some(failure) = &self.failure {
            return failure.clone();
        }
        if let Some(request) = &self.pairing {
            return if request.answered {
                format!(
                    "Código {} aceptado. Esperando a que «{}» lo confirme...",
                    request.code, request.name
                )
            } else {
                format!(
                    "«{}» quiere emparejarse. Comprueba que muestra el código {}{} y pulsa Aceptar.",
                    request.name,
                    request.code,
                    if request.peer_accepted {
                        " (ya confirmado en la Deck)"
                    } else {
                        ""
                    }
                )
            };
        }
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        self.mode_text().into()
    }

    fn mode_text(&self) -> &str {
        if self.mode == SessionMode::Mirror {
            match self.state {
                State::Starting => {
                    return "Iniciando. Acepta el permiso para preparar el acceso de red...";
                }
                State::Activating => {
                    return "Conexión preparada. Iniciando la captura de la pantalla principal...";
                }
                State::Streaming => return "Deck conectada. Duplicando la pantalla principal.",
                State::Stopping => return "Deteniendo la transmisión...",
                _ => {}
            }
        }
        self.state.text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopping_cannot_be_reversed_by_late_host_events() {
        let mut panel = Panel::default();
        panel.start();
        panel.update(State::Streaming);
        panel.stop();
        panel.update(State::Listening);
        assert_eq!(panel.state, State::Stopping);
        panel.finish(Err("broker falló".into()));
        panel.update(State::Streaming);
        assert_eq!(panel.state, State::Stopped);
        assert_eq!(panel.text(), "broker falló");
        panel.start();
        assert!(panel.failure.is_none());
    }
    #[test]
    fn mirror_status_describes_capture_without_display_lifecycle() {
        let mut panel = Panel {
            mode: SessionMode::Mirror,
            ..Panel::default()
        };
        panel.start();
        assert!(panel.text().contains("red"));
        panel.update(State::Activating);
        assert!(panel.text().contains("captura"));
        panel.stop();
        assert!(!panel.text().contains("ventanas"));
        panel.finish(Err("captura interrumpida".into()));
        assert_eq!(panel.text(), "captura interrumpida");
    }
    #[test]
    fn only_explicit_host_events_change_connection_state() {
        for state in [
            State::Stopped,
            State::Starting,
            State::HostStarting,
            State::Listening,
            State::Negotiating,
            State::Validating,
            State::ValidationFailed,
            State::Activating,
            State::Unstable,
            State::LegacyClient,
            State::Streaming,
            State::Stopping,
        ] {
            assert!(!state.text().is_empty());
        }
        assert_eq!(
            State::from_host("windowdeck_state=streaming\r\n"),
            Some(State::Streaming)
        );
        assert_eq!(
            State::from_host("windowdeck_state=negotiating"),
            Some(State::Negotiating)
        );
        for (name, state) in [
            ("validating", State::Validating),
            ("validation_failed", State::ValidationFailed),
            ("activating", State::Activating),
            ("unstable", State::Unstable),
        ] {
            assert_eq!(
                State::from_host(&format!("windowdeck_state={name}")),
                Some(state)
            );
        }
        for line in [
            "connected",
            "log windowdeck_state=streaming",
            "windowdeck_state=failed",
        ] {
            assert!(State::from_host(line).is_none());
        }
    }

    const FINGERPRINT: &str = "1a2b-3c4d-5e6f-7a8b";

    fn request_line(id: &str) -> String {
        format!("windowdeck_pairing={id} 123456 {FINGERPRINT} Steam Deck de Ana")
    }

    #[test]
    fn pairing_lines_are_parsed_strictly() {
        let Some(HostLine::Pairing(request)) = HostLine::parse(&request_line("a1b2c3d4e5f6"))
        else {
            panic!("request expected");
        };
        assert_eq!(request.code, "123 456");
        assert_eq!(request.fingerprint, FINGERPRINT);
        assert_eq!(request.name, "Steam Deck de Ana");
        assert_eq!(
            HostLine::parse("windowdeck_pairing_peer=a1b2c3d4e5f6 accepted\r\n"),
            Some(HostLine::PairingPeer {
                id: "a1b2c3d4e5f6".into(),
                accepted: true
            })
        );
        assert_eq!(
            HostLine::parse("windowdeck_pairing_result=a1b2c3d4e5f6 expired"),
            Some(HostLine::PairingResult {
                id: "a1b2c3d4e5f6".into(),
                result: PairingResult::Expired
            })
        );
        assert_eq!(
            HostLine::parse("windowdeck_state=legacy_client"),
            Some(HostLine::State(State::LegacyClient))
        );
        for line in [
            "windowdeck_pairing=a1b2c3d4e5f6 123456 1a2b-3c4d-5e6f-7a8b".to_string(),
            "windowdeck_pairing=xyz 123456 1a2b-3c4d-5e6f-7a8b Deck".into(),
            "windowdeck_pairing=a1b2c3d4e5f6 12345 1a2b-3c4d-5e6f-7a8b Deck".into(),
            "windowdeck_pairing=a1b2c3d4e5f6 12345a 1a2b-3c4d-5e6f-7a8b Deck".into(),
            "windowdeck_pairing=a1b2c3d4e5f6 123456 1a2b3c4d5e6f7a8b Deck".into(),
            "windowdeck_pairing=a1b2c3d4e5f6 123456 1a2b-3c4d-5e6f-7a8b De\u{7}ck".into(),
            "windowdeck_pairing_result=a1b2c3d4e5f6 maybe".into(),
            "windowdeck_pairing_peer=a1b2c3d4e5f6".into(),
            "log windowdeck_pairing=a1b2c3d4e5f6 123456 1a2b-3c4d-5e6f-7a8b Deck".into(),
        ] {
            assert!(HostLine::parse(&line).is_none(), "{line}");
        }
    }

    #[test]
    fn a_pairing_request_takes_over_the_status_until_its_result() {
        let mut panel = Panel::default();
        panel.start();
        panel.update(State::Listening);
        assert!(!panel.pairing_event(HostLine::parse(&request_line("a1b2c3d4e5f6")).unwrap()));
        assert!(panel.text().contains("123 456"));
        assert!(panel.text().contains("Steam Deck de Ana"));
        panel.pairing_event(HostLine::PairingPeer {
            id: "a1b2c3d4e5f6".into(),
            accepted: true,
        });
        assert!(panel.text().contains("ya confirmado"));
        let answered = panel.answer().unwrap();
        assert_eq!(answered.id, "a1b2c3d4e5f6");
        assert!(panel.answer().is_none(), "one answer per request");
        assert!(panel.text().contains("Esperando"));
        // A result for another request is ignored.
        assert!(!panel.pairing_event(HostLine::PairingResult {
            id: "000000000000".into(),
            result: PairingResult::Paired
        }));
        assert!(panel.pairing.is_some());
        assert!(panel.pairing_event(HostLine::PairingResult {
            id: "a1b2c3d4e5f6".into(),
            result: PairingResult::Paired
        }));
        assert!(panel.text().contains("emparejada"));
        // The notice survives the host returning to listening, not a new session.
        panel.update(State::Listening);
        assert!(panel.text().contains("emparejada"));
        panel.update(State::Negotiating);
        assert_eq!(panel.text(), State::Negotiating.text());
    }

    #[test]
    fn stopping_discards_pending_requests_and_late_lines() {
        let mut panel = Panel::default();
        panel.start();
        panel.pairing_event(HostLine::parse(&request_line("a1b2c3d4e5f6")).unwrap());
        panel.stop();
        assert!(panel.pairing.is_none());
        assert!(panel.answer().is_none());
        panel.pairing_event(HostLine::parse(&request_line("a1b2c3d4e5f6")).unwrap());
        assert!(panel.pairing.is_none());
        panel.finish(Ok(()));
        assert_eq!(panel.state, State::Stopped);
    }

    #[test]
    fn answers_replace_the_decision_file_atomically() {
        let dir = std::env::temp_dir().join(format!("windowdeck-launcher-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        answer_pairing(&dir, "a1b2c3d4e5f6", true).unwrap();
        assert_eq!(
            std::fs::read_to_string(pairing_file(&dir)).unwrap(),
            "a1b2c3d4e5f6 accept\n"
        );
        answer_pairing(&dir, "a1b2c3d4e5f6", false).unwrap();
        assert_eq!(
            std::fs::read_to_string(pairing_file(&dir)).unwrap(),
            "a1b2c3d4e5f6 reject\n"
        );
        assert!(!pairing_file(&dir).with_extension("tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
