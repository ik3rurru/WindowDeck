#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Stopped,
    Starting,
    HostStarting,
    Listening,
    Negotiating,
    Streaming,
    Stopping,
}

impl State {
    pub fn from_host(line: &str) -> Option<Self> {
        match line.trim_end() {
            "windowdeck_state=listening" => Some(Self::Listening),
            "windowdeck_state=negotiating" => Some(Self::Negotiating),
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
            Self::Streaming => "Deck conectada.",
            Self::Stopping => "Deteniendo y recuperando las ventanas...",
        }
    }
}

#[derive(Default)]
pub struct Panel {
    pub state: State,
    pub failure: Option<String>,
}

impl Panel {
    pub fn start(&mut self) {
        self.state = State::Starting;
        self.failure = None;
    }

    pub fn stop(&mut self) {
        self.state = State::Stopping;
    }

    pub fn update(&mut self, state: State) {
        // A late stdout event cannot undo cancellation or a completed session.
        if self.state != State::Stopping && self.state != State::Stopped {
            self.state = state;
        }
    }

    pub fn finish(&mut self, result: Result<(), String>) {
        self.state = State::Stopped;
        self.failure = result.err();
    }

    pub fn text(&self) -> &str {
        self.failure.as_deref().unwrap_or(self.state.text())
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
    fn only_explicit_host_events_change_connection_state() {
        for state in [
            State::Stopped,
            State::Starting,
            State::HostStarting,
            State::Listening,
            State::Negotiating,
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
        for line in [
            "connected",
            "log windowdeck_state=streaming",
            "windowdeck_state=failed",
        ] {
            assert!(State::from_host(line).is_none());
        }
    }
}
