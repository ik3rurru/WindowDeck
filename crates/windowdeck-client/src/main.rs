use softbuffer::{Context, Surface};
use std::env;
use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::num::NonZeroU32;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windowdeck_diagnostics::{Level, emit};
use windowdeck_protocol::{
    ConnectionEvent, ConnectionState, Message, VideoCodec, read_message, write_message,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

const DEFAULT_ADDRESS: &str = "127.0.0.1:48150";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const RECONNECT_DELAY: Duration = Duration::from_secs(1);

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
    let options = parse_options(env::args().skip(1))?;
    if options.h264_test {
        return receive_h264_test(&options.address, options.fullscreen);
    }
    let (stream, session_id, width, height) = connect(&options.address, VideoCodec::Rgb332)?;
    let shutdown = stream.try_clone()?;
    let latest = Arc::new(Mutex::new(None));
    let stopping = Arc::new(AtomicBool::new(false));
    let event_loop = EventLoop::<ClientEvent>::with_user_event().build()?;
    let worker = receive_frames(
        stream,
        session_id,
        options.address,
        Arc::clone(&latest),
        Arc::clone(&stopping),
        event_loop.create_proxy(),
    );
    let context = Context::new(event_loop.owned_display_handle())?;
    let mut app = App {
        context,
        surface: None,
        latest,
        frame: None,
        stopping,
        shutdown,
        worker: Some(worker),
        initial_size: (width, height),
        start_fullscreen: options.fullscreen,
    };

    event_loop.run_app(&mut app)?;
    app.stop()?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    address: String,
    fullscreen: bool,
    h264_test: bool,
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Options, &'static str> {
    let mut address = None;
    let mut fullscreen = false;
    let mut h264_test = false;
    for argument in args {
        match argument.as_str() {
            "--fullscreen" => fullscreen = true,
            "--h264-test" => h264_test = true,
            _ if argument.starts_with('-') => return Err("opción desconocida"),
            _ if address.is_none() => address = Some(argument),
            _ => return Err("solo se admite una dirección"),
        }
    }
    Ok(Options {
        address: address.unwrap_or_else(|| DEFAULT_ADDRESS.into()),
        fullscreen,
        h264_test,
    })
}

fn connect(address: &str, codec: VideoCodec) -> Result<(TcpStream, u64, u16, u16), Box<dyn Error>> {
    let socket: SocketAddr = address
        .parse()
        .map_err(|_| "dirección inválida; usa IP:puerto")?;
    let mut stream = TcpStream::connect_timeout(&socket, CONNECT_TIMEOUT)?;
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
            max_width: if codec == VideoCodec::H264 {
                u16::MAX
            } else {
                1280
            },
            max_height: if codec == VideoCodec::H264 {
                u16::MAX
            } else {
                800
            },
            max_fps: 60,
            codecs: codec.capability(),
        },
    )?;
    let (session_id, width, height) = match read_message(&mut stream)? {
        Message::SessionConfig {
            session_id,
            width,
            height,
            fps,
            codec: configured_codec,
        } if width > 0 && height > 0 && fps > 0 && configured_codec == codec => {
            emit(
                Level::Info,
                "session_configured",
                &[
                    ("width", &width.to_string()),
                    ("height", &height.to_string()),
                    ("fps", &fps.to_string()),
                    (
                        "codec",
                        if codec == VideoCodec::H264 {
                            "h264"
                        } else {
                            "rgb332"
                        },
                    ),
                ],
            );
            state = state.apply(ConnectionEvent::Negotiated)?;
            (session_id, width, height)
        }
        Message::SessionConfig { .. } => {
            return Err("configuración de sesión o códec no compatible".into());
        }
        _ => return Err("se esperaba SessionConfig".into()),
    };
    match read_message(&mut stream)? {
        Message::Start => {
            state.apply(ConnectionEvent::Started)?;
        }
        _ => return Err("se esperaba Start".into()),
    }
    Ok((stream, session_id, width, height))
}

fn receive_h264_test(address: &str, fullscreen: bool) -> Result<(), Box<dyn Error>> {
    let (stream, session_id, ..) = connect(address, VideoCodec::H264)?;
    let mut command = Command::new("ffplay");
    command.args([
        "-loglevel",
        "error",
        "-autoexit",
        "-an",
        "-fflags",
        "nobuffer",
        "-flags",
        "low_delay",
        "-framedrop",
        "-probesize",
        "32k",
        "-analyzeduration",
        "0",
        "-f",
        "mpegts",
        "-window_title",
        "WindowDeck H.264",
    ]);
    if fullscreen {
        command.arg("-fs");
    }
    let mut player = command
        .args(["-i", "pipe:0"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("no se pudo iniciar ffplay; instala FFmpeg: {error}"),
            )
        })?;
    let input = player.stdin.take().ok_or("ffplay no abrió su entrada")?;
    let shutdown = stream.try_clone()?;
    let stopping = Arc::new(AtomicBool::new(false));
    let worker_stopping = Arc::clone(&stopping);
    let worker = thread::spawn(move || forward_h264(stream, input, session_id, &worker_stopping));
    let player_closed = loop {
        if worker.is_finished() {
            break false;
        }
        if player.try_wait()?.is_some() {
            break true;
        }
        thread::sleep(Duration::from_millis(10));
    };
    if player_closed {
        stopping.store(true, Ordering::Relaxed);
        let _ = shutdown.shutdown(Shutdown::Both);
    }
    let forwarded = worker
        .join()
        .map_err(|_| "el receptor H.264 terminó inesperadamente")?;
    let status = match player.try_wait()? {
        Some(status) => status,
        None => player.wait()?,
    };
    if !status.success() {
        return Err(format!("ffplay terminó con {status}").into());
    }
    let (bytes, chunks) = forwarded?;
    emit(
        Level::Info,
        "h264_stream_stopped",
        &[
            ("bytes", &bytes.to_string()),
            ("chunks", &chunks.to_string()),
        ],
    );
    Ok(())
}

fn forward_h264(
    mut reader: impl Read,
    mut output: impl Write,
    session_id: u64,
    stopping: &AtomicBool,
) -> io::Result<(u64, u64)> {
    let mut bytes = 0_u64;
    let mut chunks = 0_u64;
    loop {
        let message = match read_message(&mut reader) {
            Ok(message) => message,
            Err(_) if stopping.load(Ordering::Relaxed) => break,
            Err(error) => return Err(io::Error::other(error)),
        };
        match message {
            Message::VideoChunk {
                session_id: chunk_session_id,
                frame_number,
                fragment_index,
                fragment_count,
                keyframe,
                payload,
                ..
            } if chunk_session_id == session_id
                && frame_number == chunks
                && fragment_index == 0
                && fragment_count == 1
                && (chunks != 0 || keyframe) =>
            {
                if let Err(error) = output.write_all(&payload) {
                    if error.kind() == io::ErrorKind::BrokenPipe {
                        break;
                    }
                    return Err(error);
                }
                bytes = bytes
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| io::Error::other("contador H.264 desbordado"))?;
                chunks += 1;
            }
            Message::VideoChunk { .. } => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "paquete H.264 desordenado o de otra sesión",
                ));
            }
            Message::Stop => break,
            Message::Error { code, message } => {
                return Err(io::Error::other(format!("host error {code}: {message}")));
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "mensaje inesperado durante streaming H.264",
                ));
            }
        }
    }
    Ok((bytes, chunks))
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
    Failed(String),
    Reconnected(TcpStream),
    Stopped,
}

fn receive_frames(
    mut stream: TcpStream,
    mut session_id: u64,
    address: String,
    latest: Arc<Mutex<Option<Frame>>>,
    stopping: Arc<AtomicBool>,
    proxy: EventLoopProxy<ClientEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        'sessions: loop {
            let started = Instant::now();
            let mut received = 0_u64;
            let mut dropped = 0_u64;
            let mut lost = 0_u64;
            let mut previous = None;

            loop {
                match read_message(&mut stream) {
                    Ok(Message::Frame {
                        session_id: frame_session_id,
                        number,
                        width,
                        height,
                        pixels,
                        ..
                    }) => {
                        if frame_session_id != session_id {
                            let _ = proxy.send_event(ClientEvent::Failed(
                                "frame recibido de otra sesión".into(),
                            ));
                            return;
                        }
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
                                let _ = proxy.send_event(ClientEvent::Failed(error.to_string()));
                                return;
                            }
                        };
                        if notify && proxy.send_event(ClientEvent::FrameReady).is_err() {
                            return;
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
                        return;
                    }
                    Ok(Message::Error { code, message }) => {
                        let _ = proxy.send_event(ClientEvent::Failed(format!(
                            "host error {code}: {message}"
                        )));
                        return;
                    }
                    Ok(_) => {
                        let _ = proxy.send_event(ClientEvent::Failed(
                            "mensaje inesperado durante streaming".into(),
                        ));
                        return;
                    }
                    Err(error) => {
                        if proxy
                            .send_event(ClientEvent::Disconnected(error.to_string()))
                            .is_err()
                        {
                            return;
                        }
                        break;
                    }
                }
            }

            loop {
                thread::sleep(RECONNECT_DELAY);
                if stopping.load(Ordering::Relaxed) {
                    return;
                }
                match connect(&address, VideoCodec::Rgb332) {
                    Ok((new_stream, new_session_id, ..)) => {
                        let Ok(shutdown) = new_stream.try_clone() else {
                            continue;
                        };
                        if proxy
                            .send_event(ClientEvent::Reconnected(shutdown))
                            .is_err()
                        {
                            return;
                        }
                        stream = new_stream;
                        session_id = new_session_id;
                        continue 'sessions;
                    }
                    Err(error) => emit(
                        Level::Warn,
                        "reconnect_failed",
                        &[("error", &error.to_string())],
                    ),
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
    stopping: Arc<AtomicBool>,
    shutdown: TcpStream,
    worker: Option<JoinHandle<()>>,
    initial_size: (u16, u16),
    start_fullscreen: bool,
}

impl App {
    fn stop(&mut self) -> Result<(), Box<dyn Error>> {
        self.stopping.store(true, Ordering::Relaxed);
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

    fn toggle_fullscreen(&self) {
        let Some(surface) = &self.surface else {
            return;
        };
        let window = surface.window();
        let enabled = window.fullscreen().is_none();
        window.set_fullscreen(enabled.then_some(Fullscreen::Borderless(None)));
        emit(
            Level::Info,
            "fullscreen_changed",
            &[("enabled", &enabled.to_string())],
        );
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
            .with_fullscreen(
                self.start_fullscreen
                    .then_some(Fullscreen::Borderless(None)),
            )
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
            Ok(surface) => {
                self.surface = Some(surface);
                emit(
                    Level::Info,
                    "window_opened",
                    &[("fullscreen", &self.start_fullscreen.to_string())],
                );
            }
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
                if let Some(surface) = &self.surface {
                    surface.window().set_title("WindowDeck — reconectando…");
                }
            }
            ClientEvent::Failed(error) => {
                emit(Level::Error, "session_failed", &[("error", &error)]);
                event_loop.exit();
            }
            ClientEvent::Reconnected(shutdown) => {
                self.shutdown = shutdown;
                emit(Level::Info, "host_reconnected", &[]);
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
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                match event.logical_key {
                    Key::Named(NamedKey::F11) => self.toggle_fullscreen(),
                    Key::Named(NamedKey::Escape)
                        if self
                            .surface
                            .as_ref()
                            .is_some_and(|surface| surface.window().fullscreen().is_some()) =>
                    {
                        self.toggle_fullscreen();
                    }
                    _ => {}
                }
            }
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
    let red = u32::from(value >> 5) * 255 / 7;
    let green = u32::from((value >> 2) & 7) * 255 / 7;
    let blue = u32::from(value & 3) * 255 / 3;
    (red << 16) | (green << 8) | blue
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

    #[test]
    fn rgb332_expands_to_display_color() {
        assert_eq!(color(0xe0), 0x00ff_0000);
        assert_eq!(color(0x1c), 0x0000_ff00);
        assert_eq!(color(0x03), 0x0000_00ff);
        assert_eq!(color(0xff), 0x00ff_ffff);
    }

    #[test]
    fn options_accept_address_and_fullscreen_in_any_order() {
        assert_eq!(
            parse_options(["--fullscreen".into(), "192.0.2.1:48150".into()]),
            Ok(Options {
                address: "192.0.2.1:48150".into(),
                fullscreen: true,
                h264_test: false,
            })
        );
        assert_eq!(
            parse_options(Vec::new()),
            Ok(Options {
                address: DEFAULT_ADDRESS.into(),
                fullscreen: false,
                h264_test: false,
            })
        );
        assert_eq!(
            parse_options(["--h264-test".into()]),
            Ok(Options {
                address: DEFAULT_ADDRESS.into(),
                fullscreen: false,
                h264_test: true,
            })
        );
        assert_eq!(
            parse_options(["--h264-test".into(), "--fullscreen".into()]),
            Ok(Options {
                address: DEFAULT_ADDRESS.into(),
                fullscreen: true,
                h264_test: true,
            })
        );
        assert!(parse_options(["--unknown".into()]).is_err());
    }

    #[test]
    fn h264_stream_forwards_ordered_chunks() {
        let mut stream = Vec::new();
        for (frame_number, payload) in [(0, b"first".as_slice()), (1, b"second".as_slice())] {
            write_message(
                &mut stream,
                &Message::VideoChunk {
                    session_id: 42,
                    frame_number,
                    captured_micros: 0,
                    fragment_index: 0,
                    fragment_count: 1,
                    keyframe: frame_number == 0,
                    payload: payload.to_vec(),
                },
            )
            .expect("valid chunk");
        }
        write_message(&mut stream, &Message::Stop).expect("valid stop");

        let mut output = Vec::new();
        assert_eq!(
            forward_h264(stream.as_slice(), &mut output, 42, &AtomicBool::new(false))
                .expect("valid stream"),
            (11, 2)
        );
        assert_eq!(output, b"firstsecond");
    }
}
