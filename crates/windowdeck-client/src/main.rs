use softbuffer::{Context, Surface};
use std::env;
use std::error::Error;
use std::net::{Shutdown, TcpStream};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windowdeck_diagnostics::{Level, emit};
use windowdeck_protocol::{ConnectionEvent, ConnectionState, Message, read_message, write_message};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle};
use winit::window::{Window, WindowId};

fn main() {
    if let Err(error) = run() {
        emit(
            Level::Error,
            "client_failed",
            &[("error", &error.to_string())],
        );
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:48150".into());
    let (stream, width, height) = connect(&address)?;
    let shutdown = stream.try_clone()?;
    let latest = Arc::new(Mutex::new(None));
    let event_loop = EventLoop::<ClientEvent>::with_user_event().build()?;
    let worker = receive_frames(stream, Arc::clone(&latest), event_loop.create_proxy());
    let context = Context::new(event_loop.owned_display_handle())?;
    let mut app = App::new(context, latest, shutdown, worker, width, height);

    event_loop.run_app(&mut app)?;
    app.stop()?;
    Ok(())
}

fn connect(address: &str) -> Result<(TcpStream, u16, u16), Box<dyn Error>> {
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    emit(Level::Info, "host_connected", &[("address", address)]);

    write_message(
        &mut stream,
        &Message::Hello {
            app_version: env!("CARGO_PKG_VERSION").into(),
        },
    )?;
    let mut state = ConnectionState::AwaitingHello;
    match read_message(&mut stream)? {
        Message::Hello { app_version } => {
            emit(Level::Info, "host_hello", &[("version", &app_version)]);
            state = state.apply(ConnectionEvent::HelloReceived)?;
        }
        _ => return Err("se esperaba Hello".into()),
    }

    write_message(
        &mut stream,
        &Message::Capabilities {
            max_width: 1280,
            max_height: 800,
            max_fps: 60,
        },
    )?;
    let (width, height) = match read_message(&mut stream)? {
        Message::SessionConfig {
            width, height, fps, ..
        } if width > 0 && height > 0 && fps > 0 => {
            emit(
                Level::Info,
                "session_configured",
                &[
                    ("width", &width.to_string()),
                    ("height", &height.to_string()),
                    ("fps", &fps.to_string()),
                ],
            );
            state = state.apply(ConnectionEvent::Negotiated)?;
            (width, height)
        }
        Message::SessionConfig { .. } => return Err("configuración de sesión inválida".into()),
        _ => return Err("se esperaba SessionConfig".into()),
    };
    match read_message(&mut stream)? {
        Message::Start => {
            state.apply(ConnectionEvent::Started)?;
        }
        _ => return Err("se esperaba Start".into()),
    }
    Ok((stream, width, height))
}

#[derive(Debug)]
struct Frame {
    number: u64,
    width: u16,
    height: u16,
    pixels: Vec<u8>,
}

enum ClientEvent {
    FrameReady,
    Disconnected(String),
    Stopped,
}

fn receive_frames(
    mut stream: TcpStream,
    latest: Arc<Mutex<Option<Frame>>>,
    proxy: EventLoopProxy<ClientEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let started = Instant::now();
        let mut received = 0_u64;
        let mut dropped = 0_u64;
        let mut lost = 0_u64;
        let mut previous = None;

        loop {
            match read_message(&mut stream) {
                Ok(Message::Frame {
                    number,
                    width,
                    height,
                    pixels,
                    ..
                }) => {
                    received += 1;
                    if let Some(previous) = previous {
                        lost += number.saturating_sub(previous + 1);
                    }
                    previous = Some(number);

                    let notify = match latest.lock() {
                        Ok(mut latest) => {
                            if latest
                                .replace(Frame {
                                    number,
                                    width,
                                    height,
                                    pixels,
                                })
                                .is_some()
                            {
                                dropped += 1;
                                false
                            } else {
                                true
                            }
                        }
                        Err(error) => {
                            let _ = proxy.send_event(ClientEvent::Disconnected(error.to_string()));
                            break;
                        }
                    };
                    if notify && proxy.send_event(ClientEvent::FrameReady).is_err() {
                        break;
                    }
                    if received.is_multiple_of(60) {
                        emit(
                            Level::Info,
                            "video_metrics",
                            &[
                                ("frames_received", &received.to_string()),
                                ("frames_lost", &lost.to_string()),
                                ("frames_dropped", &dropped.to_string()),
                                (
                                    "fps",
                                    &format!(
                                        "{:.1}",
                                        received as f64 / started.elapsed().as_secs_f64()
                                    ),
                                ),
                            ],
                        );
                    }
                }
                Ok(Message::Ping { nonce }) => {
                    if let Err(error) = write_message(&mut stream, &Message::Pong { nonce }) {
                        let _ = proxy.send_event(ClientEvent::Disconnected(error.to_string()));
                        break;
                    }
                }
                Ok(Message::Stop) => {
                    let _ = proxy.send_event(ClientEvent::Stopped);
                    break;
                }
                Ok(Message::Error { code, message }) => {
                    let _ = proxy.send_event(ClientEvent::Disconnected(format!(
                        "host error {code}: {message}"
                    )));
                    break;
                }
                Ok(_) => {
                    let _ = proxy.send_event(ClientEvent::Disconnected(
                        "mensaje inesperado durante streaming".into(),
                    ));
                    break;
                }
                Err(error) => {
                    let _ = proxy.send_event(ClientEvent::Disconnected(error.to_string()));
                    break;
                }
            }
        }
    })
}

struct App {
    context: Context<OwnedDisplayHandle>,
    surface: Option<Surface<OwnedDisplayHandle, Rc<Window>>>,
    latest: Arc<Mutex<Option<Frame>>>,
    frame: Option<Frame>,
    shutdown: TcpStream,
    worker: Option<JoinHandle<()>>,
    initial_size: (u16, u16),
}

impl App {
    fn new(
        context: Context<OwnedDisplayHandle>,
        latest: Arc<Mutex<Option<Frame>>>,
        shutdown: TcpStream,
        worker: JoinHandle<()>,
        width: u16,
        height: u16,
    ) -> Self {
        Self {
            context,
            surface: None,
            latest,
            frame: None,
            shutdown,
            worker: Some(worker),
            initial_size: (width, height),
        }
    }

    fn stop(&mut self) -> Result<(), Box<dyn Error>> {
        let _ = self.shutdown.shutdown(Shutdown::Both);
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            return Err("el hilo de red terminó inesperadamente".into());
        }
        Ok(())
    }

    fn fail(event_loop: &ActiveEventLoop, error: &dyn std::fmt::Display) {
        emit(
            Level::Error,
            "render_failed",
            &[("error", &error.to_string())],
        );
        event_loop.exit();
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let Some(frame) = self.frame.as_ref() else {
            return;
        };
        let size = surface.window().inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        if let Err(error) = surface.resize(width, height) {
            Self::fail(event_loop, &error);
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(error) => {
                Self::fail(event_loop, &error);
                return;
            }
        };
        if let Err(error) = draw_frame(frame, size.width, size.height, &mut buffer) {
            Self::fail(event_loop, &error);
            return;
        }
        if let Err(error) = buffer.present() {
            Self::fail(event_loop, &error);
        }
    }
}

impl ApplicationHandler<ClientEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_some() {
            return;
        }
        let scale =
            (960.0 / f64::from(self.initial_size.0)).min(600.0 / f64::from(self.initial_size.1));
        let attributes = Window::default_attributes()
            .with_title("WindowDeck")
            .with_inner_size(LogicalSize::new(
                f64::from(self.initial_size.0) * scale,
                f64::from(self.initial_size.1) * scale,
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(error) => {
                Self::fail(event_loop, &error);
                return;
            }
        };
        match Surface::new(&self.context, window) {
            Ok(surface) => self.surface = Some(surface),
            Err(error) => Self::fail(event_loop, &error),
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ClientEvent) {
        match event {
            ClientEvent::FrameReady => match self.latest.lock() {
                Ok(mut latest) => {
                    if let Some(frame) = latest.take() {
                        if let Some(surface) = &self.surface {
                            surface
                                .window()
                                .set_title(&format!("WindowDeck — frame {}", frame.number));
                            surface.window().request_redraw();
                        }
                        self.frame = Some(frame);
                    }
                }
                Err(error) => Self::fail(event_loop, &error),
            },
            ClientEvent::Disconnected(error) => {
                emit(Level::Warn, "host_disconnected", &[("error", &error)]);
                event_loop.exit();
            }
            ClientEvent::Stopped => {
                emit(Level::Info, "session_stopped", &[]);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .surface
            .as_ref()
            .is_none_or(|surface| surface.window().id() != window_id)
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                if let Some(surface) = &self.surface {
                    surface.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }
}

fn draw_frame(
    frame: &Frame,
    destination_width: u32,
    destination_height: u32,
    output: &mut [u32],
) -> Result<(), &'static str> {
    let source_width = usize::from(frame.width);
    let source_height = usize::from(frame.height);
    let destination_width = usize::try_from(destination_width).map_err(|_| "ancho inválido")?;
    let destination_height = usize::try_from(destination_height).map_err(|_| "alto inválido")?;
    if source_width == 0 || source_height == 0 {
        return Err("frame vacío");
    }
    if frame.pixels.len() != source_width * source_height {
        return Err("frame con dimensiones inválidas");
    }
    if output.len() != destination_width * destination_height {
        return Err("buffer con dimensiones inválidas");
    }

    output.fill(0x0000_0000);
    let (draw_width, draw_height) =
        if destination_width * source_height <= destination_height * source_width {
            (
                destination_width,
                destination_width * source_height / source_width,
            )
        } else {
            (
                destination_height * source_width / source_height,
                destination_height,
            )
        };
    let offset_x = (destination_width - draw_width) / 2;
    let offset_y = (destination_height - draw_height) / 2;

    for y in 0..draw_height {
        let source_y = y * source_height / draw_height;
        for x in 0..draw_width {
            let source_x = x * source_width / draw_width;
            let value = frame.pixels[source_y * source_width + source_x];
            output[(offset_y + y) * destination_width + offset_x + x] = color(value);
        }
    }
    Ok(())
}

fn color(value: u8) -> u32 {
    let intensity = u32::from(value.min(9)) * 25;
    (intensity << 16) | ((255 - intensity) << 8) | 0x40
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_scaled_with_letterboxing() {
        let frame = Frame {
            number: 0,
            width: 2,
            height: 1,
            pixels: vec![0, 9],
        };
        let mut output = vec![1; 16];

        draw_frame(&frame, 4, 4, &mut output).expect("valid frame");

        assert_eq!(&output[0..4], &[0; 4]);
        assert_eq!(&output[12..16], &[0; 4]);
        assert_eq!(&output[4..8], &[color(0), color(0), color(9), color(9)]);
    }

    #[test]
    fn invalid_frame_is_rejected() {
        let frame = Frame {
            number: 0,
            width: 2,
            height: 2,
            pixels: vec![0, 1, 2],
        };
        assert!(draw_frame(&frame, 2, 2, &mut [0; 4]).is_err());
    }
}
