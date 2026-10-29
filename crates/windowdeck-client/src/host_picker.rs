use super::*;
use crate::canvas::{self, Canvas};
use windowdeck_protocol::discovery::{Browser, Host};
use winit::event::{MouseButton, TouchPhase};

pub fn choose(browser: &mut Browser, ui: &mut Ui) -> Result<Option<Host>, Box<dyn Error>> {
    let event_loop = ui.event_loop()?;
    let context = Context::new(event_loop.owned_display_handle())?;
    let mut picker = Picker {
        context,
        surface: None,
        window: None,
        browser,
        hosts: Vec::new(),
        selected: None,
        cursor_y: 0.0,
    };
    event_loop.run_app_on_demand(&mut picker)?;
    Ok(picker.selected)
}
struct Picker<'a> {
    context: Context<OwnedDisplayHandle>,
    surface: Option<Surface<OwnedDisplayHandle, Rc<Window>>>,
    window: Option<Rc<Window>>,
    browser: &'a mut Browser,
    hosts: Vec<Host>,
    selected: Option<Host>,
    cursor_y: f64,
}
impl Picker<'_> {
    fn select(&mut self, y: f64, event_loop: &ActiveEventLoop) {
        let index = ((y - 80.0) / 48.0).floor() as isize;
        if y >= 80.0 && index >= 0 && (index as usize) < self.hosts.len() {
            self.selected = Some(self.hosts[index as usize].clone());
            event_loop.exit();
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
        let mut canvas = Canvas::new(&mut pixels, size.width as usize, size.height as usize);
        let mut text = |value: &str, y: usize| canvas.text(20, y, 2, canvas::WHITE, value);
        if self.hosts.is_empty() {
            text("Buscando equipos en la red...", 24);
            text("Pulsa Iniciar en Windows.", 64);
            text("Comprueba que comparten la misma red.", 104);
        } else {
            text("Elige el PC para conectar (toca su nombre)", 24);
            for (i, host) in self.hosts.iter().enumerate() {
                text(&format!("{}: {}", i + 1, host.name), 96 + i * 48);
            }
        }
        pixels.present()?;
        Ok(())
    }
}
impl ApplicationHandler<ClientEvent> for Picker<'_> {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let hosts = self.browser.snapshot();
        if hosts.len() == 1 && self.hosts.is_empty() {
            self.selected = hosts.into_iter().next();
            event_loop.exit();
            return;
        }
        if self.hosts != hosts {
            self.hosts = hosts;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<(), Box<dyn Error>> {
            let window = Rc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("WindowDeck - Elegir PC")
                        .with_inner_size(LogicalSize::new(800, 600)),
                )?,
            );
            self.surface = Some(Surface::new(&self.context, Rc::clone(&window))?);
            window.request_redraw();
            self.window = Some(window);
            Ok(())
        })();
        if result.is_err() {
            event_loop.exit();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => self.cursor_y = position.y,
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => self.select(self.cursor_y, event_loop),
            WindowEvent::Touch(touch) if touch.phase == TouchPhase::Ended => {
                self.select(touch.location.y, event_loop)
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if event.logical_key == Key::Named(NamedKey::Escape) {
                    event_loop.exit();
                }
                if let Key::Character(c) = event.logical_key
                    && let Ok(n) = c.parse::<usize>()
                    && n > 0
                    && n <= self.hosts.len()
                {
                    self.selected = Some(self.hosts[n - 1].clone());
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                if self.draw().is_err() {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(_) => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}
