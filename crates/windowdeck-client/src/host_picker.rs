use super::*;
use font8x8::UnicodeFonts;
use windowdeck_protocol::discovery::Host;
use winit::event::{MouseButton, TouchPhase};

pub fn choose(hosts: &[Host]) -> Result<Option<usize>, Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let context = Context::new(event_loop.owned_display_handle())?;
    let mut picker = Picker {
        context,
        surface: None,
        window: None,
        hosts,
        selected: None,
        cursor_y: 0.0,
    };
    event_loop.run_app(&mut picker)?;
    Ok(picker.selected)
}
struct Picker<'a> {
    context: Context<OwnedDisplayHandle>,
    surface: Option<Surface<OwnedDisplayHandle, Rc<Window>>>,
    window: Option<Rc<Window>>,
    hosts: &'a [Host],
    selected: Option<usize>,
    cursor_y: f64,
}
impl Picker<'_> {
    fn select(&mut self, y: f64, event_loop: &ActiveEventLoop) {
        let index = ((y - 80.0) / 48.0).floor() as isize;
        if y >= 80.0 && index >= 0 && (index as usize) < self.hosts.len() {
            self.selected = Some(index as usize);
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
        pixels.fill(0x18202c);
        let mut text = |value: &str, y: usize| {
            for (i, c) in value
                .chars()
                .take(size.width.saturating_sub(40) as usize / 16)
                .enumerate()
            {
                let glyph = font8x8::BASIC_FONTS
                    .get(c)
                    .or_else(|| font8x8::BASIC_FONTS.get('?'))
                    .unwrap();
                for (row, bits) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << col) == 0 {
                            continue;
                        }
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let x = 20 + i * 16 + col * 2 + dx;
                                let py = y + row * 2 + dy;
                                if x < size.width as usize && py < size.height as usize {
                                    pixels[py * size.width as usize + x] = 0xffffff;
                                }
                            }
                        }
                    }
                }
            }
        };
        if self.hosts.is_empty() {
            text("No se encontro ningun PC.", 24);
            text("Pulsa Iniciar en Windows y vuelve a abrir.", 64);
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
impl ApplicationHandler for Picker<'_> {
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
                    self.selected = Some(n - 1);
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
