//! Encrypted sessions and interactive pairing with a host.
//! See docs/adr/0022-pairing-and-encryption.md.
use super::*;
use crate::canvas::{self, Canvas, Rect};
use std::sync::mpsc;
use windowdeck_transport::pairing::{self as ceremony, Outcome, Prompt, Request};
use windowdeck_transport::secure::{self, Established};
use windowdeck_transport::{Identity, Intent, Role, SecureError, SecureStream, Trust};
use winit::event::{MouseButton, TouchPhase};

/// This device's identity and the hosts it trusts.
pub struct ClientSecurity {
    pub trust: Trust,
    identity: Identity,
    name: String,
}

impl ClientSecurity {
    pub fn load() -> Result<Self, Box<dyn Error>> {
        Self::open(Trust::open_default(Role::Client)?)
    }

    pub fn open(trust: Trust) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            identity: trust.identity()?,
            trust,
            name: windowdeck_transport::trust::local_name(),
        })
    }
}

/// Opens an authenticated session. Errors keep their I/O kind so reconnect
/// loops still retry transport failures; pairing errors are never retried.
pub fn secure_session(target: &Target, socket: TcpStream) -> Result<SecureStream, Box<dyn Error>> {
    let security = &target.security;
    match secure::connect(
        socket,
        &security.identity,
        &security.name,
        Intent::Session,
        |key| security.trust.is_paired(key),
    ) {
        Ok(Established::Session(stream, host)) => {
            emit(
                Level::Info,
                "host_authenticated",
                &[
                    ("name", &host.name),
                    ("fingerprint", &host.key.fingerprint()),
                ],
            );
            Ok(stream)
        }
        Ok(_) => Err("respuesta inesperada del PC".into()),
        Err(SecureError::Io(error)) => Err(error.into()),
        Err(error) => Err(error.into()),
    }
}

pub fn needs_pairing(error: &(dyn Error + 'static)) -> bool {
    error
        .downcast_ref::<SecureError>()
        .is_some_and(SecureError::needs_pairing)
}

/// The network side of pairing; `prompt` asks the person.
pub fn pair_with(
    target: &Target,
    codec: VideoCodec,
    prompt: &mut dyn Prompt,
) -> Result<Outcome, Box<dyn Error>> {
    let security = &target.security;
    let socket = open_socket(target, codec)?;
    socket.set_nodelay(true)?;
    match secure::connect(
        socket,
        &security.identity,
        &security.name,
        Intent::Pair,
        |key| security.trust.is_paired(key),
    )? {
        Established::AlreadyPaired(_) => Ok(Outcome::Paired),
        Established::PairingRequired(pairing) => {
            let mut pairing = *pairing;
            let outcome = ceremony::pair_as_client(
                &mut pairing.stream,
                &pairing.transcript,
                &pairing.peer,
                prompt,
                ceremony::TIMEOUT,
            )?;
            if outcome == Outcome::Paired {
                security.trust.add(pairing.peer.key, &pairing.peer.name)?;
                emit(
                    Level::Info,
                    "host_paired",
                    &[
                        ("name", &pairing.peer.name),
                        ("fingerprint", &pairing.peer.key.fingerprint()),
                    ],
                );
            }
            Ok(outcome)
        }
        Established::Session(..) => Err("respuesta inesperada del PC".into()),
    }
}

fn outcome_message(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Paired => "Emparejado.",
        Outcome::RejectedHere => "Has indicado que los códigos no coinciden. No se ha emparejado.",
        Outcome::RejectedByPeer => "El PC rechazó el emparejamiento.",
        Outcome::Expired => "La solicitud caducó. Vuelve a abrir WindowDeck para intentarlo.",
        Outcome::Cancelled => "Emparejamiento cancelado.",
    }
}

/// Lists or forgets trusted hosts: `--paired-hosts` and `--forget-host`.
pub fn manage(args: &[String]) -> Result<(), Box<dyn Error>> {
    let trust = Trust::open_default(Role::Client)?;
    match args {
        [command] if command == "--paired-hosts" => {
            let hosts = trust.devices()?;
            if hosts.is_empty() {
                println!("No hay PC emparejados.");
            }
            for host in hosts {
                println!(
                    "{}  {}  emparejado el {}",
                    host.key.fingerprint(),
                    host.name,
                    host.paired_on()
                );
            }
            println!(
                "Huella de esta Deck: {}",
                trust.identity()?.public().fingerprint()
            );
            Ok(())
        }
        [command, selector] if command == "--forget-host" => {
            if selector == "all" {
                println!("Olvidados {} PC.", trust.clear()?);
            } else {
                let host = trust.remove(selector)?;
                println!("Olvidado {} ({}).", host.name, host.key.fingerprint());
            }
            Ok(())
        }
        _ => Err("uso: --paired-hosts | --forget-host HUELLA|all".into()),
    }
}

/// Messages from the ceremony thread to the window.
pub enum Update {
    Show(Request),
    PeerAccepted,
    Finished(Result<Outcome, String>),
}

struct ThreadPrompt {
    proxy: EventLoopProxy<ClientEvent>,
    decisions: mpsc::Receiver<bool>,
    cancelled: Arc<AtomicBool>,
}

impl Prompt for ThreadPrompt {
    fn show(&mut self, request: &Request) -> io::Result<()> {
        self.proxy
            .send_event(ClientEvent::Pairing(Update::Show(request.clone())))
            .map_err(|_| io::Error::other("la ventana de emparejamiento se cerró"))
    }
    fn decision(&mut self) -> io::Result<Option<bool>> {
        Ok(self.decisions.try_recv().ok())
    }
    fn peer_decided(&mut self, accepted: bool) {
        if accepted {
            let _ = self
                .proxy
                .send_event(ClientEvent::Pairing(Update::PeerAccepted));
        }
    }
    fn cancelled(&mut self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Shows the code and asks the person to compare it with the PC. Returns once
/// both devices trust each other, or the reason they do not.
pub fn pair_interactively(
    target: &Target,
    codec: VideoCodec,
    ui: &mut Ui,
    reason: &dyn Error,
) -> Result<(), Box<dyn Error>> {
    emit(
        Level::Info,
        "pairing_needed",
        &[("reason", &reason.to_string())],
    );
    let event_loop = ui.event_loop()?;
    let proxy = event_loop.create_proxy();
    let (decide, decisions) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker = {
        let (target, cancelled) = (target.clone(), Arc::clone(&cancelled));
        thread::spawn(move || {
            let mut prompt = ThreadPrompt {
                proxy: proxy.clone(),
                decisions,
                cancelled,
            };
            let result = pair_with(&target, codec, &mut prompt).map_err(|e| e.to_string());
            let _ = proxy.send_event(ClientEvent::Pairing(Update::Finished(result.clone())));
            result
        })
    };
    let mut window = PairingWindow {
        context: Context::new(event_loop.owned_display_handle())?,
        surface: None,
        window: None,
        screen: Screen::Connecting,
        peer_accepted: false,
        decide,
        buttons: Vec::new(),
        cursor: (0.0, 0.0),
    };
    let run = event_loop.run_app_on_demand(&mut window);
    // Closing the window abandons a pending ceremony; the worker notices quickly.
    cancelled.store(true, Ordering::Relaxed);
    drop(window);
    let result = worker
        .join()
        .map_err(|_| "el emparejamiento terminó inesperadamente")?;
    run?;
    match result {
        Ok(Outcome::Paired) => Ok(()),
        Ok(outcome) => Err(outcome_message(outcome).into()),
        Err(error) => Err(error.into()),
    }
}

enum Screen {
    Connecting,
    Code(Request),
    Waiting(Request),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Accept,
    Reject,
    Close,
}

struct PairingWindow {
    context: Context<OwnedDisplayHandle>,
    surface: Option<Surface<OwnedDisplayHandle, Rc<Window>>>,
    window: Option<Rc<Window>>,
    screen: Screen,
    peer_accepted: bool,
    decide: mpsc::Sender<bool>,
    buttons: Vec<(Rect, Action)>,
    cursor: (f64, f64),
}

impl PairingWindow {
    fn act(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        match (&self.screen, action) {
            (Screen::Code(request), Action::Accept) => {
                let _ = self.decide.send(true);
                self.screen = Screen::Waiting(request.clone());
            }
            (Screen::Code(_) | Screen::Waiting(_), Action::Reject) => {
                let _ = self.decide.send(false);
            }
            (Screen::Failed(_), _) | (Screen::Connecting, Action::Reject | Action::Close) => {
                event_loop.exit();
            }
            _ => return,
        }
        self.redraw();
    }

    fn click(&mut self, x: f64, y: f64, event_loop: &ActiveEventLoop) {
        if let Some((_, action)) = self.buttons.iter().find(|(rect, _)| rect.contains(x, y)) {
            self.act(*action, event_loop);
        }
    }

    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn draw(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return Ok(());
        };
        let surface = self.surface.as_mut().ok_or("No hay superficie")?;
        surface.resize(w, h)?;
        let mut pixels = surface.buffer_mut()?;
        pixels.fill(canvas::BACKGROUND);
        let (width, height) = (size.width as usize, size.height as usize);
        let mut canvas = Canvas::new(&mut pixels, width, height);
        self.buttons.clear();
        let half = width / 2;
        let row = |y: usize| y.min(height.saturating_sub(80));
        let left = Rect {
            x: half.saturating_sub(300),
            y: row(360),
            width: 280,
            height: 64,
        };
        let right = Rect {
            x: half + 20,
            ..left
        };
        match &self.screen {
            Screen::Connecting => {
                canvas.centered(80, 3, canvas::WHITE, "Emparejar con el PC");
                canvas.centered(180, 2, canvas::MUTED, "Conectando con el PC...");
                canvas.centered(
                    220,
                    2,
                    canvas::MUTED,
                    "Comprueba que WindowDeck está iniciado en Windows.",
                );
                let cancel = Rect {
                    x: half.saturating_sub(140),
                    ..left
                };
                canvas.button(cancel, canvas::NEUTRAL, "Cancelar");
                self.buttons.push((cancel, Action::Close));
            }
            Screen::Code(request) | Screen::Waiting(request) => {
                canvas.centered(40, 3, canvas::WHITE, "Emparejar con el PC");
                canvas.centered(100, 2, canvas::MUTED, &format!("«{}»", request.peer.name));
                canvas.centered(
                    140,
                    2,
                    canvas::MUTED,
                    "Comprueba que el PC muestra este código:",
                );
                canvas.centered(190, 8, canvas::WHITE, &request.code);
                canvas.centered(
                    290,
                    2,
                    canvas::MUTED,
                    &format!("Huella del PC: {}", request.peer.key.fingerprint()),
                );
                if matches!(self.screen, Screen::Code(_)) {
                    canvas.button(left, canvas::ACCEPT, "Coincide");
                    canvas.button(right, canvas::REJECT, "No coincide");
                    self.buttons.push((left, Action::Accept));
                    self.buttons.push((right, Action::Reject));
                    if self.peer_accepted {
                        canvas.centered(
                            row(450),
                            2,
                            canvas::MUTED,
                            "El PC ya ha confirmado el código.",
                        );
                    }
                    canvas.centered(
                        row(490),
                        2,
                        canvas::MUTED,
                        "Intro: coincide   Esc: no coincide",
                    );
                } else {
                    canvas.centered(row(380), 2, canvas::WHITE, "Confirma el código en el PC...");
                    let cancel = Rect {
                        x: half.saturating_sub(140),
                        y: row(440),
                        ..left
                    };
                    canvas.button(cancel, canvas::NEUTRAL, "Cancelar");
                    self.buttons.push((cancel, Action::Reject));
                }
            }
            Screen::Failed(message) => {
                canvas.centered(80, 3, canvas::WHITE, "No se ha emparejado");
                // Wrap long messages at word boundaries for the 8x8 font.
                let columns = (width.saturating_sub(40) / 16).max(10);
                let mut line = String::new();
                let mut y = 180;
                for word in message.split_whitespace() {
                    if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > columns
                    {
                        canvas.centered(y, 2, canvas::MUTED, &line);
                        line.clear();
                        y += 32;
                    }
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(word);
                }
                canvas.centered(y, 2, canvas::MUTED, &line);
                let close = Rect {
                    x: half.saturating_sub(140),
                    ..left
                };
                canvas.button(close, canvas::NEUTRAL, "Cerrar");
                self.buttons.push((close, Action::Close));
            }
        }
        pixels.present()?;
        Ok(())
    }
}

impl ApplicationHandler<ClientEvent> for PairingWindow {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<(), Box<dyn Error>> {
            let window = Rc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("WindowDeck - Emparejar con el PC")
                        .with_inner_size(LogicalSize::new(800, 600)),
                )?,
            );
            self.surface = Some(Surface::new(&self.context, Rc::clone(&window))?);
            window.request_redraw();
            self.window = Some(window);
            Ok(())
        })();
        if let Err(error) = result {
            emit(
                Level::Error,
                "pairing_window_failed",
                &[("error", &error.to_string())],
            );
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ClientEvent) {
        let ClientEvent::Pairing(update) = event else {
            return;
        };
        match update {
            Update::Show(request) => self.screen = Screen::Code(request),
            Update::PeerAccepted => self.peer_accepted = true,
            Update::Finished(Ok(Outcome::Paired)) => {
                event_loop.exit();
                return;
            }
            Update::Finished(Ok(outcome)) => {
                self.screen = Screen::Failed(outcome_message(outcome).into())
            }
            Update::Finished(Err(error)) => self.screen = Screen::Failed(error),
        }
        self.redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => self.cursor = (position.x, position.y),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => self.click(self.cursor.0, self.cursor.1, event_loop),
            WindowEvent::Touch(touch) if touch.phase == TouchPhase::Ended => {
                self.click(touch.location.x, touch.location.y, event_loop)
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Enter) => self.act(Action::Accept, event_loop),
                    Key::Named(NamedKey::Escape) => {
                        let action = if matches!(self.screen, Screen::Failed(_)) {
                            Action::Close
                        } else {
                            Action::Reject
                        };
                        self.act(action, event_loop)
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw() {
                    emit(
                        Level::Error,
                        "pairing_window_failed",
                        &[("error", &error.to_string())],
                    );
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(_) => self.redraw(),
            _ => {}
        }
    }
}
