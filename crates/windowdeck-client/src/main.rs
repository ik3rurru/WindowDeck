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
    ConnectionEvent, ConnectionState, Message, ProtocolError, VideoCodec, read_message,
    write_message,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

mod host_picker;
#[cfg(feature = "native-media")]
mod native;
const DEFAULT_ADDRESS: &str = "auto";
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
    #[cfg(feature = "native-media")]
    if env::args().nth(1).as_deref() == Some("--media-self-test") {
        windowdeck_media::self_test()?;
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("--version") {
        println!(
            "windowdeck-client {} protocol={} profile={}",
            env!("CARGO_PKG_VERSION"),
            windowdeck_protocol::PROTOCOL_VERSION,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        );
        println!("native_media={}", cfg!(feature = "native-media"));
        #[cfg(feature = "native-media")]
        println!("ffmpeg={}", windowdeck_media::version());
        return Ok(());
    }
    let options = parse_options(env::args().skip(1))?;
    let native = env::args().any(|arg| arg == "--native")
        || (cfg!(feature = "native-media")
            && options.address == "auto"
            && !env::args().any(|arg| arg == "--ffplay")
            && !options.ffplay_baseline);
    if native && !cfg!(feature = "native-media") {
        return Err("recompila con --features native-media para usar --native".into());
    }
    let mut target = Target::from(options.address.as_str());
    if options.address == "auto" {
        let codec = if options.h264_test { "h264" } else { "rgb332" };
        let mut browser = windowdeck_protocol::discovery::Browser::new(codec)?;
        let Some(host) = host_picker::choose(&mut browser)? else {
            return Ok(());
        };
        target.address = format!("mdns:{}", host.identity);
        target.browser = Some(Arc::new(Mutex::new(browser)));
    }
    if options.h264_test {
        #[cfg(feature = "native-media")]
        if native {
            return native::run(target, options.fullscreen);
        }
        return receive_h264_test(&target, options.fullscreen, options.ffplay_baseline);
    }
    let (stream, session_id, width, height, _) = connect(&target, VideoCodec::Rgb332)?;
    let shutdown = stream.try_clone()?;
    let latest = Arc::new(Mutex::new(None));
    let stopping = Arc::new(AtomicBool::new(false));
    let event_loop = EventLoop::<ClientEvent>::with_user_event().build()?;
    let worker = receive_frames(
        stream,
        session_id,
        target,
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
    ffplay_baseline: bool,
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Options, &'static str> {
    let mut address = None;
    let mut fullscreen = false;
    let mut h264_test = false;
    let mut ffplay_baseline = false;
    for argument in args {
        match argument.as_str() {
            "--fullscreen" => fullscreen = true,
            "--h264-test" => h264_test = true,
            "--native" | "--ffplay" => h264_test = true,
            "--ffplay-baseline" => ffplay_baseline = true,
            _ if argument.starts_with('-') => return Err("opción desconocida"),
            _ if address.is_none() => address = Some(argument),
            _ => return Err("solo se admite una dirección"),
        }
    }
    if ffplay_baseline && !h264_test {
        return Err("--ffplay-baseline requiere --h264-test");
    }
    if address.as_deref().is_none_or(|value| value == "auto") {
        h264_test = true;
    }
    Ok(Options {
        address: address.unwrap_or_else(|| DEFAULT_ADDRESS.into()),
        fullscreen,
        h264_test,
        ffplay_baseline,
    })
}

#[derive(Clone)]
struct Target {
    address: String,
    browser: Option<Arc<Mutex<windowdeck_protocol::discovery::Browser>>>,
}

impl From<&str> for Target {
    fn from(address: &str) -> Self {
        Self {
            address: address.into(),
            browser: None,
        }
    }
}

type Connected = (TcpStream, u64, u16, u16, VideoCodec);
fn connect(target: &Target, codec: VideoCodec) -> Result<Connected, Box<dyn Error>> {
    connect_observed(target, codec, None)
}

fn connect_observed(
    target: &Target,
    codec: VideoCodec,
    observer: Option<(&AtomicBool, &Mutex<TcpStream>)>,
) -> Result<Connected, Box<dyn Error>> {
    let address = target.address.as_str();
    let mut stream = if let Some(identity) = address.strip_prefix("mdns:") {
        let codec = if codec != VideoCodec::Rgb332 {
            "h264"
        } else {
            "rgb332"
        };
        let hosts = if let Some(browser) = &target.browser {
            browser
                .lock()
                .map_err(|_| io::Error::other("mDNS lock poisoned"))?
                .wait(Some(identity), Duration::from_millis(700))?
        } else {
            windowdeck_protocol::discovery::browse(codec, Some(identity))?
        };
        let host = hosts.first().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "El PC no esta disponible en la red",
            )
        })?;
        windowdeck_protocol::discovery::connect_host(host)?
    } else {
        let socket: SocketAddr = address
            .parse()
            .map_err(|_| "dirección inválida; usa IP:puerto")?;
        TcpStream::connect_timeout(&socket, CONNECT_TIMEOUT)?
    };
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    if let Some((stopping, shutdown)) = observer {
        let mut watched = shutdown
            .lock()
            .map_err(|_| io::Error::other("socket lock poisoned"))?;
        if stopping.load(Ordering::Relaxed) {
            return Err(io::Error::from(io::ErrorKind::Interrupted).into());
        }
        *watched = stream.try_clone()?;
    }
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
            codecs: codec.capability()
                | if codec == VideoCodec::H264Frames {
                    VideoCodec::H264.capability()
                } else {
                    0
                },
        },
    )?;
    let (session_id, width, height, actual_codec) = match read_message(&mut stream)? {
        Message::SessionConfig {
            session_id,
            width,
            height,
            fps,
            codec: configured_codec,
        } if width > 0
            && height > 0
            && fps > 0
            && fps <= 60
            && (configured_codec == codec
                || (codec == VideoCodec::H264Frames && configured_codec == VideoCodec::H264))
            && (configured_codec != VideoCodec::H264Frames || (width <= 1280 && height <= 800)) =>
        {
            emit(
                Level::Info,
                "session_configured",
                &[
                    ("width", &width.to_string()),
                    ("height", &height.to_string()),
                    ("fps", &fps.to_string()),
                    (
                        "codec",
                        match configured_codec {
                            VideoCodec::H264 => "h264_mpegts",
                            VideoCodec::H264Frames => "h264_access_units",
                            VideoCodec::Rgb332 => "rgb332",
                        },
                    ),
                ],
            );
            state = state.apply(ConnectionEvent::Negotiated)?;
            (session_id, width, height, configured_codec)
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
    Ok((stream, session_id, width, height, actual_codec))
}

fn ffplay_command(fullscreen: bool, baseline: bool) -> Command {
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
        "32",
        "-analyzeduration",
        "0",
        "-f",
        "mpegts",
        "-window_title",
        "WindowDeck H.264",
    ]);
    if !baseline {
        // AVIO direct triggers missing-PPS errors on this MPEG-TS pipe; see docs/testing.md.
        command.args(["-max_delay", "0", "-sync", "ext"]);
    }
    if fullscreen {
        command.arg("-fs");
    }
    command.args(["-i", "pipe:0"]).stdin(Stdio::piped());
    command
}

fn receive_h264_test(
    address: &Target,
    fullscreen: bool,
    baseline: bool,
) -> Result<(), Box<dyn Error>> {
    let (stream, session_id, ..) = connect(address, VideoCodec::H264)?;
    play_legacy(stream, session_id, address, fullscreen, baseline)
}

fn play_legacy(
    stream: TcpStream,
    session_id: u64,
    address: &Target,
    fullscreen: bool,
    baseline: bool,
) -> Result<(), Box<dyn Error>> {
    let mut player = ffplay_command(fullscreen, baseline)
        .spawn()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("no se pudo iniciar ffplay; instala FFmpeg: {error}"),
            )
        })?;
    emit(
        Level::Info,
        "h264_player_started",
        &[("buffering", if baseline { "baseline" } else { "reduced" })],
    );
    let input = player.stdin.take().ok_or("ffplay no abrió su entrada")?;
    let shutdown = Arc::new(Mutex::new(stream.try_clone()?));
    let stopping = Arc::new(AtomicBool::new(false));
    let worker_stopping = Arc::clone(&stopping);
    let worker_shutdown = Arc::clone(&shutdown);
    let address = address.clone();
    let worker = thread::spawn(move || {
        reconnect_h264(
            stream,
            session_id,
            &address,
            input,
            &worker_stopping,
            &worker_shutdown,
        )
    });
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
        if let Ok(socket) = shutdown.lock() {
            let _ = socket.shutdown(Shutdown::Both);
        }
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
    forwarded?;
    emit(Level::Info, "h264_stream_stopped", &[]);
    Ok(())
}

fn transport_error(error: &(dyn Error + 'static)) -> bool {
    let io =
        error
            .downcast_ref::<io::Error>()
            .or_else(|| match error.downcast_ref::<ProtocolError>() {
                Some(ProtocolError::Io(error)) => Some(error),
                _ => None,
            });
    io.is_some_and(|error| {
        matches!(
            error.kind(),
            io::ErrorKind::UnexpectedEof
                | io::ErrorKind::TimedOut
                | io::ErrorKind::WouldBlock
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionRefused
                | io::ErrorKind::NotConnected
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::NetworkUnreachable
                | io::ErrorKind::HostUnreachable
                | io::ErrorKind::Interrupted
        )
    })
}

fn reconnect_h264(
    mut stream: TcpStream,
    mut session_id: u64,
    address: &Target,
    mut input: impl Write,
    stopping: &AtomicBool,
    shutdown: &Mutex<TcpStream>,
) -> io::Result<()> {
    loop {
        // Keep the same player and its stdin alive across transport failures. This
        // leaves the window's X available while offline and avoids an EOF/autoexit race.
        let result = forward_h264(&mut stream, &mut input, session_id, stopping);
        if stopping.load(Ordering::Relaxed) {
            return Ok(());
        }
        match result {
            Ok((bytes, chunks)) => {
                emit(
                    Level::Info,
                    "h264_session_stopped",
                    &[
                        ("bytes", &bytes.to_string()),
                        ("chunks", &chunks.to_string()),
                    ],
                );
                return Ok(()); // Explicit Stop or the player closed its pipe.
            }
            Err(error) if transport_error(&error) => {
                emit(
                    Level::Warn,
                    "h264_connection_lost",
                    &[("error", &error.to_string())],
                );
            }
            Err(error) => return Err(error),
        }
        let _ = stream.shutdown(Shutdown::Both);
        loop {
            let until = Instant::now() + RECONNECT_DELAY;
            while Instant::now() < until {
                if stopping.load(Ordering::Relaxed) {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(20));
            }
            match connect_observed(address, VideoCodec::H264, Some((stopping, shutdown))) {
                Ok((next, id, ..)) => {
                    let mut watched = shutdown
                        .lock()
                        .map_err(|_| io::Error::other("socket lock poisoned"))?;
                    if stopping.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    *watched = next.try_clone()?;
                    stream = next;
                    session_id = id;
                    emit(
                        Level::Info,
                        "h264_reconnected",
                        &[("session_id", &id.to_string())],
                    );
                    break;
                }
                Err(error) if transport_error(error.as_ref()) => {
                    emit(
                        Level::Info,
                        "h264_reconnect_wait",
                        &[("error", &error.to_string())],
                    );
                }
                Err(error) => return Err(io::Error::other(error.to_string())),
            }
        }
    }
}

fn forward_h264(
    mut reader: impl Read,
    mut output: impl Write,
    session_id: u64,
    stopping: &AtomicBool,
) -> io::Result<(u64, u64)> {
    let started = Instant::now();
    let mut last_report = Instant::now();
    let mut bytes = 0_u64;
    let mut chunks = 0_u64;
    let mut chunks_at_report = 0_u64;
    let mut read_times = windowdeck_diagnostics::Timings::default();
    let mut write_times = windowdeck_diagnostics::Timings::default();
    loop {
        let read_started = Instant::now();
        let message = match read_message(&mut reader) {
            Ok(message) => message,
            Err(_) if stopping.load(Ordering::Relaxed) => break,
            Err(ProtocolError::Io(error)) => return Err(error),
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        };
        read_times.record(read_started.elapsed());
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
                let write_started = Instant::now();
                if let Err(error) = output.write_all(&payload) {
                    if error.kind() == io::ErrorKind::BrokenPipe {
                        break;
                    }
                    return Err(error);
                }
                write_times.record(write_started.elapsed());
                bytes = bytes
                    .checked_add(payload.len() as u64)
                    .ok_or_else(|| io::Error::other("contador H.264 desbordado"))?;
                chunks += 1;
                if chunks == 1 {
                    emit(
                        Level::Info,
                        "h264_first_packet_received",
                        &[("elapsed_ms", &started.elapsed().as_millis().to_string())],
                    );
                }
                if last_report.elapsed() >= Duration::from_secs(1) {
                    read_times.report("h264_tcp_read_metrics");
                    write_times.report("h264_player_write_metrics");
                    let elapsed = started.elapsed();
                    let report_elapsed = last_report.elapsed();
                    let report_chunks = chunks.saturating_sub(chunks_at_report);
                    let mbps =
                        bytes as f64 * 8.0 / elapsed.as_secs_f64().max(f64::EPSILON) / 1_000_000.0;
                    emit(
                        Level::Info,
                        "h264_receive_metrics",
                        &[
                            ("elapsed_ms", &elapsed.as_millis().to_string()),
                            ("bytes", &bytes.to_string()),
                            ("chunks", &chunks.to_string()),
                            (
                                "chunks_per_sec",
                                &format!(
                                    "{:.1}",
                                    report_chunks as f64
                                        / report_elapsed.as_secs_f64().max(f64::EPSILON)
                                ),
                            ),
                            ("mbps", &format!("{mbps:.2}")),
                        ],
                    );
                    chunks_at_report = chunks;
                    last_report = Instant::now();
                }
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
    address: Target,
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
    fn h264_reconnects_with_new_session_and_resets_chunk_sequence() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let client = TcpStream::connect(&address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let packet = |id, value| Message::VideoChunk {
                session_id: id,
                frame_number: 0,
                captured_micros: 0,
                fragment_index: 0,
                fragment_count: 1,
                keyframe: true,
                payload: vec![value],
            };
            write_message(&mut first, &packet(10, 1)).unwrap();
            drop(first); // EOF without Stop is a transport failure.
            let (mut second, _) = listener.accept().unwrap();
            second
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            assert!(matches!(
                read_message(&mut second).unwrap(),
                Message::Hello { .. }
            ));
            write_message(
                &mut second,
                &Message::Hello {
                    app_version: "test".into(),
                },
            )
            .unwrap();
            assert!(matches!(
                read_message(&mut second).unwrap(),
                Message::Capabilities { .. }
            ));
            write_message(
                &mut second,
                &Message::SessionConfig {
                    session_id: 20,
                    width: 1280,
                    height: 800,
                    fps: 60,
                    codec: VideoCodec::H264,
                },
            )
            .unwrap();
            write_message(&mut second, &Message::Start).unwrap();
            write_message(&mut second, &packet(20, 2)).unwrap();
            write_message(&mut second, &Message::Stop).unwrap();
        });
        let shutdown = Mutex::new(client.try_clone().unwrap());
        let mut output = Vec::new();
        reconnect_h264(
            client,
            10,
            &Target::from(address.as_str()),
            &mut output,
            &AtomicBool::new(false),
            &shutdown,
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(output, [1, 2]);
    }

    #[test]
    fn closing_player_during_reconnect_wait_prevents_new_connection() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let client = TcpStream::connect(&address).unwrap();
        let (server, _) = listener.accept().unwrap();
        drop(server);
        let stopping = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&stopping);
        let shutdown = Mutex::new(client.try_clone().unwrap());
        let worker = thread::spawn(move || {
            reconnect_h264(
                client,
                1,
                &Target::from(address.as_str()),
                Vec::new(),
                &cancel,
                &shutdown,
            )
        });
        thread::sleep(Duration::from_millis(100));
        stopping.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap();
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn reconnect_retries_transport_but_rejects_protocol_errors() {
        assert!(transport_error(&ProtocolError::Io(io::Error::from(
            io::ErrorKind::WouldBlock
        ))));
        assert!(transport_error(&io::Error::from(
            io::ErrorKind::ConnectionRefused
        )));
        assert!(!transport_error(&ProtocolError::UnsupportedVersion(99)));
        assert!(!transport_error(&io::Error::from(
            io::ErrorKind::InvalidData
        )));
        let mut bytes = Vec::new();
        write_message(
            &mut bytes,
            &Message::Hello {
                app_version: "wrong".into(),
            },
        )
        .unwrap();
        let error =
            forward_h264(bytes.as_slice(), Vec::new(), 1, &AtomicBool::new(false)).unwrap_err();
        assert!(!transport_error(&error));
    }

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
                ffplay_baseline: false,
            })
        );
        assert_eq!(
            parse_options(Vec::new()),
            Ok(Options {
                address: DEFAULT_ADDRESS.into(),
                fullscreen: false,
                h264_test: true,
                ffplay_baseline: false,
            })
        );
        assert_eq!(
            parse_options(["--h264-test".into()]),
            Ok(Options {
                address: DEFAULT_ADDRESS.into(),
                fullscreen: false,
                h264_test: true,
                ffplay_baseline: false,
            })
        );
        assert_eq!(
            parse_options(["--h264-test".into(), "--fullscreen".into()]),
            Ok(Options {
                address: DEFAULT_ADDRESS.into(),
                fullscreen: true,
                h264_test: true,
                ffplay_baseline: false,
            })
        );
        assert!(parse_options(["--unknown".into()]).is_err());
        assert!(parse_options(["--ffplay-baseline".into()]).is_err());
        let baseline = parse_options([
            "--ffplay-baseline".into(),
            "--fullscreen".into(),
            "--h264-test".into(),
        ])
        .expect("valid H.264 comparison");
        assert!(baseline.ffplay_baseline && baseline.h264_test && baseline.fullscreen);
    }

    #[test]
    fn ffplay_comparison_only_changes_buffering_options() {
        let baseline = ffplay_command(true, true);
        let baseline_args: Vec<_> = baseline.get_args().collect();
        assert_eq!(
            baseline_args,
            [
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
                "32",
                "-analyzeduration",
                "0",
                "-f",
                "mpegts",
                "-window_title",
                "WindowDeck H.264",
                "-fs",
                "-i",
                "pipe:0",
            ]
        );
        let reduced = ffplay_command(true, false);
        let reduced_args: Vec<_> = reduced.get_args().collect();
        assert_eq!(&reduced_args[..17], &baseline_args[..17]);
        assert_eq!(&reduced_args[17..21], ["-max_delay", "0", "-sync", "ext"]);
        assert_eq!(&reduced_args[21..], &baseline_args[17..]);
        assert!(
            !ffplay_command(false, false)
                .get_args()
                .any(|arg| arg == "-fs")
        );
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
