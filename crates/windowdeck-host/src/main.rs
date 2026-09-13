use std::env;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windowdeck_diagnostics::{Level, emit};
use windowdeck_protocol::{
    ConnectionEvent, ConnectionState, Message, VideoCodec, connection, read_message, write_message,
};

#[cfg(any(windows, test))]
mod annex_b;
#[cfg(windows)]
mod capture;
mod cli;

type AnyError = Box<dyn Error + Send + Sync>;
const DEFAULT_ADDRESS: &str = "0.0.0.0:48150";
const WIDTH: u16 = 128;
const HEIGHT: u16 = 80;
const FPS: u16 = 10;
#[cfg(windows)]
const H264_WIDTH: u16 = 1280;
#[cfg(windows)]
const H264_HEIGHT: u16 = 800;
const DIGITS: [u64; 10] = [
    0b11111_10001_10001_10001_10001_10001_11111,
    0b00100_01100_00100_00100_00100_00100_01110,
    0b11111_00001_00001_11111_10000_10000_11111,
    0b11111_00001_00001_11111_00001_00001_11111,
    0b10001_10001_10001_11111_00001_00001_00001,
    0b11111_10000_10000_11111_00001_00001_11111,
    0b11111_10000_10000_11111_10001_10001_11111,
    0b11111_00001_00010_00100_01000_01000_01000,
    0b11111_10001_10001_11111_10001_10001_11111,
    0b11111_10001_10001_11111_00001_00001_11111,
];

fn main() {
    if let Err(error) = run() {
        emit(
            Level::Error,
            "host_failed",
            &[("error", &error.to_string())],
        );
        std::process::exit(1);
    }
}

fn run() -> Result<(), AnyError> {
    let mode = cli::parse_mode(env::args().skip(1))?;
    if matches!(mode, Mode::Help | Mode::DiagHelp) {
        println!(
            "{}",
            if mode == Mode::Help {
                cli::HELP
            } else {
                cli::DIAG_HELP
            }
        );
        return Ok(());
    }
    if mode == Mode::Version {
        println!(
            "windowdeck-host {} protocol={} profile={}",
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
    emit(
        Level::Info,
        "host_build",
        &[
            ("version", env!("CARGO_PKG_VERSION")),
            (
                "profile",
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
            ),
        ],
    );
    match mode {
        Mode::Serve {
            address,
            monitor,
            codec,
        } => run_server(address, monitor, codec),
        Mode::CaptureTest(index) => capture_test(index),
        Mode::EncodeTest(index) => encode_test(index),
        Mode::DriverFrameTest => driver_frame_test(false),
        Mode::GpuFrameTest => driver_frame_test(true),
        Mode::GpuEncodeTest(encoder) => gpu_encode_test(&encoder),
        Mode::Help | Mode::DiagHelp | Mode::Version => unreachable!(),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Help,
    DiagHelp,
    Version,
    Serve {
        address: String,
        monitor: Option<CaptureTarget>,
        codec: VideoCodec,
    },
    CaptureTest(usize),
    EncodeTest(usize),
    DriverFrameTest,
    GpuFrameTest,
    GpuEncodeTest(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptureTarget {
    Monitor(usize),
    WindowDeck,
    WindowDeckAuto,
    WindowDeckCpu,
    WindowDeckNative,
}

#[cfg(windows)]
fn driver_frame_test(gpu: bool) -> Result<(), AnyError> {
    capture::probe_driver_frames(gpu)
}

#[cfg(all(windows, feature = "native-media"))]
fn gpu_encode_test(encoder: &str) -> Result<(), AnyError> {
    windowdeck_media::gpu_self_test(encoder)?;
    Ok(())
}

#[cfg(not(all(windows, feature = "native-media")))]
fn gpu_encode_test(_encoder: &str) -> Result<(), AnyError> {
    Err("diag gpu-encode requiere Windows y una compilacion con native-media".into())
}

#[cfg(not(windows))]
fn driver_frame_test(_gpu: bool) -> Result<(), AnyError> {
    Err("el prototipo del driver requiere Windows".into())
}

#[cfg(windows)]
fn capture_test(index: usize) -> Result<(), AnyError> {
    capture::run(index)
}

#[cfg(not(windows))]
fn capture_test(_index: usize) -> Result<(), AnyError> {
    Err("la captura de pantalla solo está disponible en Windows".into())
}

#[cfg(windows)]
fn encode_test(index: usize) -> Result<(), AnyError> {
    capture::encode(index)
}

#[cfg(not(windows))]
fn encode_test(_index: usize) -> Result<(), AnyError> {
    Err("la codificación de pantalla solo está disponible en Windows".into())
}

fn run_server(
    address: String,
    monitor: Option<CaptureTarget>,
    codec: VideoCodec,
) -> Result<(), AnyError> {
    #[cfg(not(feature = "native-media"))]
    if monitor == Some(CaptureTarget::WindowDeckNative) {
        return Err("recompila con --features native-media para usar --driver-native-h264".into());
    }
    #[cfg(not(windows))]
    if monitor.is_some() {
        return Err("la captura de pantalla solo está disponible en Windows".into());
    }
    let listener = TcpListener::bind(&address)?;
    let _announcement = match windowdeck_protocol::discovery::advertise(
        listener.local_addr()?,
        if codec != VideoCodec::Rgb332 {
            "h264"
        } else {
            "rgb332"
        },
    ) {
        Ok(service) => Some(service),
        Err(error) => {
            emit(
                Level::Warn,
                "discovery_unavailable",
                &[("error", &error.to_string())],
            );
            None
        }
    };
    emit(Level::Info, "host_listening", &[("address", &address)]);
    publish_state("listening");

    listener.set_nonblocking(true)?;
    let mut health = SessionHealth::default();
    loop {
        if stop_requested() {
            publish_state("stopped");
            return Ok(());
        }
        let (stream, peer) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        stream.set_nonblocking(false)?;
        publish_state("negotiating");
        emit(
            Level::Info,
            "client_connected",
            &[("peer", &peer.to_string())],
        );
        let result = serve(stream, monitor, codec, &mut health);
        if let Err(error) = &result {
            emit(
                Level::Warn,
                "session_closed",
                &[("error", &error.to_string())],
            );
        }
        if health.session_ended(
            result
                .as_ref()
                .err()
                .is_some_and(|error| startup_failure(error.as_ref())),
        ) {
            emit(
                Level::Error,
                "connection_unstable",
                &[("reason", "three_short_sessions_failed")],
            );
            publish_state("unstable");
            // Stop the activation loop, including retries by old clients. The panel
            // remains available so the user can stop and start a fresh attempt.
            drop(listener);
            drop(_announcement);
            while !stop_requested() {
                thread::sleep(Duration::from_millis(50));
            }
            publish_state("stopped");
            return Ok(());
        }
        if !health.validation_failed {
            publish_state("listening");
        }
    }
}

fn serve(
    mut stream: TcpStream,
    mut monitor: Option<CaptureTarget>,
    mut codec: VideoCodec,
    health: &mut SessionHealth,
) -> Result<(), AnyError> {
    health.validation_failed = false;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut state = ConnectionState::AwaitingHello;

    match read_message(&mut stream)? {
        Message::Hello { app_version } => {
            emit(Level::Info, "client_hello", &[("version", &app_version)]);
            state = state.apply(ConnectionEvent::HelloReceived)?;
        }
        _ => return Err("se esperaba Hello".into()),
    }
    write_message(
        &mut stream,
        &Message::Hello {
            app_version: env!("CARGO_PKG_VERSION").into(),
        },
    )?;

    let (width, height, fps) = stream_format(codec, monitor)?;
    let validate_connection = match read_message(&mut stream)? {
        Message::Capabilities {
            max_width,
            max_height,
            max_fps,
            codecs,
        } if (codecs & codec.capability() != 0
            || (codec == VideoCodec::H264Frames
                && codecs & VideoCodec::H264.capability() != 0))
            && width <= max_width
            && height <= max_height
            && fps <= max_fps =>
        {
            if codec == VideoCodec::H264Frames && codecs & codec.capability() == 0 {
                codec = VideoCodec::H264;
                monitor = Some(CaptureTarget::WindowDeckCpu);
                emit(
                    Level::Info,
                    "native_host_fallback",
                    &[("reason", "legacy_client")],
                );
            }
            state = state.apply(ConnectionEvent::Negotiated)?;
            codecs & connection::CAPABILITY != 0
        }
        Message::Capabilities { .. } => {
            return Err("el cliente no admite la configuración solicitada".into());
        }
        _ => return Err("se esperaba Capabilities".into()),
    };
    let session_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros() as u64;
    write_message(
        &mut stream,
        &Message::SessionConfig {
            session_id,
            width,
            height,
            fps,
            codec,
        },
    )?;
    if validate_connection {
        publish_state("validating");
        emit(Level::Info, "connection_validation_started", &[]);
        match connection::validate(&mut stream, session_id, stop_requested) {
            Ok(validation) => emit(
                Level::Info,
                "connection_validated",
                &[
                    ("elapsed_ms", &validation.elapsed.as_millis().to_string()),
                    (
                        "slowest_round_ms",
                        &validation.slowest_round.as_millis().to_string(),
                    ),
                ],
            ),
            Err(error) => {
                health.validation_failed = true;
                publish_state("validation_failed");
                emit(
                    Level::Warn,
                    "connection_validation_failed",
                    &[("error", &error.to_string())],
                );
                stream.set_write_timeout(Some(Duration::from_millis(250)))?;
                let _ = write_message(&mut stream, &Message::Error {
                    code: 2,
                    message: "La conexión no superó la prueba de estabilidad; no se activó la pantalla. Revisa la red y vuelve a intentarlo.".into(),
                });
                return Err(error.into());
            }
        }
    } else {
        emit(
            Level::Info,
            "connection_validation_skipped",
            &[("reason", "legacy_client")],
        );
    }
    state = state.apply(ConnectionEvent::Validated)?;
    if stop_requested() {
        return Ok(());
    }
    // No capture helper, encoder or display lease may start before this point.
    health.activation_started = Some(Instant::now());
    publish_state("activating");
    // The lease outlives capture_stream: reap its encoder before monitor removal.
    #[cfg(windows)]
    let _display_lease = if monitor == Some(CaptureTarget::WindowDeckAuto) {
        Some(capture::DisplayLease::acquire()?)
    } else {
        None
    };
    write_message(&mut stream, &Message::Start)?;
    state.apply(ConnectionEvent::Started)?;

    if let Some(index) = monitor {
        return capture_stream(stream, index, session_id, codec);
    }

    publish_state("streaming");

    let started = Instant::now();
    let mut number = 0_u64;
    loop {
        if stop_requested() {
            write_message(&mut stream, &Message::Stop)?;
            return Ok(());
        }
        let frame_started = Instant::now();
        let captured_micros = started.elapsed().as_micros() as u64;
        write_message(
            &mut stream,
            &Message::Frame {
                session_id,
                number,
                captured_micros,
                width: WIDTH,
                height: HEIGHT,
                pixels: pattern(number, captured_micros),
            },
        )?;
        number += 1;
        if number.is_multiple_of(u64::from(FPS)) {
            emit(
                Level::Info,
                "video_metrics",
                &[
                    ("frames_sent", &number.to_string()),
                    (
                        "fps",
                        &format!("{:.1}", number as f64 / started.elapsed().as_secs_f64()),
                    ),
                ],
            );
        }
        thread::sleep(
            Duration::from_secs_f64(1.0 / f64::from(FPS)).saturating_sub(frame_started.elapsed()),
        );
    }
}

#[derive(Default)]
struct SessionHealth {
    activation_started: Option<Instant>,
    short_failures: u8,
    validation_failed: bool,
}

fn startup_failure(error: &(dyn Error + 'static)) -> bool {
    if let Some(windowdeck_protocol::ProtocolError::Io(error)) = error.downcast_ref() {
        return startup_failure(error);
    }
    if let Some(error) = error.downcast_ref::<std::io::Error>() {
        // Closing the client (including older clients that only close TCP) is not
        // an unstable startup. Nested I/O errors retain the transport's cause.
        return match error.kind() {
            std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted => false,
            _ => error.get_ref().is_none_or(|cause| startup_failure(cause)),
        };
    }
    true
}

impl SessionHealth {
    fn session_ended(&mut self, failed: bool) -> bool {
        if let Some(started) = self.activation_started.take() {
            if failed && started.elapsed() < connection::STABLE_SESSION {
                self.short_failures = self.short_failures.saturating_add(1);
            } else {
                self.short_failures = 0;
            }
        }
        self.short_failures >= 3
    }
}

#[cfg(windows)]
fn capture_stream(
    stream: TcpStream,
    target: CaptureTarget,
    session_id: u64,
    codec: VideoCodec,
) -> Result<(), AnyError> {
    let index = match target {
        CaptureTarget::WindowDeckNative => return capture::stream_native_h264(stream, session_id),
        CaptureTarget::WindowDeckCpu
            if matches!(codec, VideoCodec::H264 | VideoCodec::H264Frames) =>
        {
            return capture::stream_driver_h264(
                stream,
                session_id,
                codec == VideoCodec::H264Frames,
            );
        }
        CaptureTarget::WindowDeckCpu => return Err("WindowDeck requiere H.264".into()),
        CaptureTarget::WindowDeck | CaptureTarget::WindowDeckAuto if codec == VideoCodec::H264 => {
            return capture::stream_virtual_h264(stream, session_id);
        }
        CaptureTarget::WindowDeck | CaptureTarget::WindowDeckAuto => {
            return Err("WindowDeck requiere H.264".into());
        }
        CaptureTarget::Monitor(index) => index,
    };
    match codec {
        VideoCodec::Rgb332 => capture::stream(stream, index, session_id),
        VideoCodec::H264 => capture::stream_h264(stream, index, session_id),
        VideoCodec::H264Frames => Err("H.264 integrado requiere la captura del driver".into()),
    }
}

#[cfg(not(windows))]
fn capture_stream(
    _stream: TcpStream,
    _target: CaptureTarget,
    _session_id: u64,
    _codec: VideoCodec,
) -> Result<(), AnyError> {
    Err("la captura de pantalla solo está disponible en Windows".into())
}

fn stream_format(
    codec: VideoCodec,
    monitor: Option<CaptureTarget>,
) -> Result<(u16, u16, u16), AnyError> {
    match codec {
        VideoCodec::Rgb332 => Ok((WIDTH, HEIGHT, FPS)),
        VideoCodec::H264 | VideoCodec::H264Frames => {
            h264_format(monitor.ok_or("H.264 requiere un monitor")?)
        }
    }
}

#[cfg(windows)]
fn h264_format(target: CaptureTarget) -> Result<(u16, u16, u16), AnyError> {
    match target {
        CaptureTarget::Monitor(index) => {
            capture::size(index)?;
        }
        CaptureTarget::WindowDeck => {
            capture::virtual_monitor()?;
        }
        CaptureTarget::WindowDeckAuto
        | CaptureTarget::WindowDeckCpu
        | CaptureTarget::WindowDeckNative => {}
    }
    Ok((H264_WIDTH, H264_HEIGHT, capture::ENCODE_FPS as u16))
}

#[cfg(not(windows))]
fn h264_format(_target: CaptureTarget) -> Result<(u16, u16, u16), AnyError> {
    Err("la codificación H.264 solo está disponible en Windows".into())
}

fn publish_state(state: &str) {
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    let _ = writeln!(output, "windowdeck_state={state}");
    let _ = output.flush();
}

fn stop_requested() -> bool {
    std::env::var_os("WINDOWDECK_STOP_FILE")
        .is_some_and(|path| std::path::Path::new(&path).is_file())
}

fn pattern(frame: u64, captured_micros: u64) -> Vec<u8> {
    let mut pixels = (0..usize::from(WIDTH) * usize::from(HEIGHT))
        .map(|index| {
            let x = index % usize::from(WIDTH);
            let y = index / usize::from(WIDTH);
            ((((x as u64 + frame) / 8 % 8) as u8) << 5)
                | ((((y as u64 + frame / 2) / 5 % 8) as u8) << 2)
                | ((((x + y) as u64 + frame) / 16 % 4) as u8)
        })
        .collect::<Vec<_>>();
    draw_number(&mut pixels, 4, 8, frame);
    draw_number(&mut pixels, 4, 40, captured_micros / 1_000);
    pixels
}

fn draw_number(pixels: &mut [u8], x: usize, y: usize, number: u64) {
    let text = number.to_string();
    for (position, digit) in text.bytes().skip(text.len().saturating_sub(10)).enumerate() {
        let glyph = DIGITS[usize::from(digit - b'0')];
        for bit in 0..35 {
            if glyph & (1 << (34 - bit)) == 0 {
                continue;
            }
            for offset_y in 0..2 {
                for offset_x in 0..2 {
                    let pixel_x = x + position * 12 + (bit % 5) * 2 + offset_x;
                    let pixel_y = y + (bit / 5) * 2 + offset_y;
                    if pixel_x < usize::from(WIDTH) && pixel_y < usize::from(HEIGHT) {
                        pixels[pixel_y * usize::from(WIDTH) + pixel_x] = u8::MAX;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_has_expected_size_moves_and_contains_counters() {
        let first = pattern(0, 0);
        assert_eq!(first.len(), usize::from(WIDTH) * usize::from(HEIGHT));
        assert!(first.contains(&u8::MAX));
        assert_ne!(first, pattern(1, 1_000));
    }

    #[test]
    fn repeated_startup_failures_pause_activation_until_a_new_host_is_started() {
        let mut health = SessionHealth::default();
        for attempt in 1..=3 {
            // Rejected connections do not count as display activation attempts.
            assert!(!health.session_ended(true));
            health.activation_started = Some(Instant::now());
            assert_eq!(health.session_ended(true), attempt == 3);
        }
        health.activation_started = Some(Instant::now() - connection::STABLE_SESSION);
        assert!(!health.session_ended(true));
        assert_eq!(health.short_failures, 0);
        health.activation_started = Some(Instant::now());
        assert!(!health.session_ended(true));
        health.activation_started = Some(Instant::now());
        assert!(!health.session_ended(false));
        assert_eq!(health.short_failures, 0);
    }

    #[test]
    fn closing_a_client_does_not_trip_the_startup_failure_limit() {
        for kind in [
            std::io::ErrorKind::UnexpectedEof,
            std::io::ErrorKind::BrokenPipe,
            std::io::ErrorKind::ConnectionReset,
        ] {
            let error = std::io::Error::other(windowdeck_protocol::ProtocolError::Io(kind.into()));
            assert!(!startup_failure(&error));
        }
        let timeout = std::io::Error::other(windowdeck_protocol::ProtocolError::Io(
            std::io::ErrorKind::TimedOut.into(),
        ));
        assert!(startup_failure(&timeout));
    }

    #[cfg(windows)]
    #[test]
    fn failed_validation_never_reaches_any_automatic_display_route() {
        for target in [
            CaptureTarget::WindowDeckAuto,
            CaptureTarget::WindowDeckCpu,
            CaptureTarget::WindowDeckNative,
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
                let (socket, _) = listener.accept().unwrap();
                let mut health = SessionHealth::default();
                let error = serve(socket, Some(target), VideoCodec::H264, &mut health).unwrap_err();
                assert!(health.activation_started.is_none());
                assert!(health.validation_failed);
                assert!(!health.session_ended(true));
                error.to_string()
            });
            let mut client = TcpStream::connect(address).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write_message(
                &mut client,
                &Message::Hello {
                    app_version: "test".into(),
                },
            )
            .unwrap();
            assert!(matches!(
                read_message(&mut client).unwrap(),
                Message::Hello { .. }
            ));
            write_message(
                &mut client,
                &Message::Capabilities {
                    max_width: 1280,
                    max_height: 800,
                    max_fps: 60,
                    codecs: VideoCodec::H264.capability() | connection::CAPABILITY,
                },
            )
            .unwrap();
            // Even WGC must deliver configuration before acquiring its display lease.
            let Message::SessionConfig { session_id, .. } = read_message(&mut client).unwrap()
            else {
                panic!("configuration must precede display activation");
            };
            for _ in 0..connection::BURST_PACKETS {
                assert!(matches!(
                    read_message(&mut client).unwrap(),
                    Message::ConnectionProbe { .. }
                ));
            }
            assert_eq!(
                read_message(&mut client).unwrap(),
                Message::Ping { nonce: session_id }
            );
            write_message(
                &mut client,
                &Message::Pong {
                    nonce: session_id.wrapping_add(1),
                },
            )
            .unwrap();
            assert!(matches!(
                read_message(&mut client).unwrap(),
                Message::Error { code: 2, .. }
            ));
            assert!(server.join().unwrap().contains("acknowledgement"));
        }
    }

    #[cfg(windows)]
    #[test]
    fn automatic_display_requires_compatible_negotiation() {
        for (target, codecs) in [
            (CaptureTarget::WindowDeckAuto, 0),
            (
                CaptureTarget::WindowDeckAuto,
                VideoCodec::Rgb332.capability(),
            ),
            (CaptureTarget::WindowDeckCpu, 0),
            (
                CaptureTarget::WindowDeckCpu,
                VideoCodec::Rgb332.capability(),
            ),
            (
                CaptureTarget::WindowDeckNative,
                VideoCodec::Rgb332.capability(),
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
                let (socket, _) = listener.accept().unwrap();
                serve(
                    socket,
                    Some(target),
                    VideoCodec::H264,
                    &mut SessionHealth::default(),
                )
                .unwrap_err()
                .to_string()
            });
            let mut client = TcpStream::connect(address).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            write_message(
                &mut client,
                &Message::Hello {
                    app_version: "test".into(),
                },
            )
            .unwrap();
            assert!(matches!(
                read_message(&mut client).unwrap(),
                Message::Hello { .. }
            ));
            write_message(
                &mut client,
                &Message::Capabilities {
                    max_width: 1280,
                    max_height: 800,
                    max_fps: 60,
                    codecs,
                },
            )
            .unwrap();
            let error = server.join().unwrap();
            assert_eq!(error, "el cliente no admite la configuración solicitada");
        }
    }
}
