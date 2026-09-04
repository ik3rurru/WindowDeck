use std::env;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windowdeck_diagnostics::{Level, emit};
use windowdeck_protocol::{
    ConnectionEvent, ConnectionState, Message, VideoCodec, read_message, write_message,
};

#[cfg(windows)]
mod capture;

type AnyError = Box<dyn Error + Send + Sync>;
const DEFAULT_ADDRESS: &str = "0.0.0.0:48150";
const WIDTH: u16 = 128;
const HEIGHT: u16 = 80;
const FPS: u16 = 10;
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
    match parse_mode(env::args().skip(1))? {
        Mode::Serve {
            address,
            monitor,
            codec,
        } => run_server(address, monitor, codec),
        Mode::CaptureTest(index) => capture_test(index),
        Mode::EncodeTest(index) => encode_test(index),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Serve {
        address: String,
        monitor: Option<usize>,
        codec: VideoCodec,
    },
    CaptureTest(usize),
    EncodeTest(usize),
}

fn parse_mode(args: impl IntoIterator<Item = String>) -> Result<Mode, &'static str> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some(command @ ("--capture-test" | "--encode-test")) => {
            let index = args
                .next()
                .map(|value| value.parse().map_err(|_| "índice de monitor inválido"))
                .transpose()?
                .unwrap_or(1);
            if index == 0 || args.next().is_some() {
                return Err("usa --capture-test [N] o --encode-test [N], con N mayor que cero");
            }
            Ok(if command == "--capture-test" {
                Mode::CaptureTest(index)
            } else {
                Mode::EncodeTest(index)
            })
        }
        Some(command @ ("--capture" | "--h264")) => {
            let index = args
                .next()
                .ok_or("falta el índice de monitor")?
                .parse()
                .map_err(|_| "índice de monitor inválido")?;
            let address = args.next().unwrap_or_else(|| DEFAULT_ADDRESS.into());
            if index == 0 || args.next().is_some() {
                return Err("usa --capture N [DIRECCIÓN] o --h264 N [DIRECCIÓN]");
            }
            Ok(Mode::Serve {
                address,
                monitor: Some(index),
                codec: if command == "--h264" {
                    VideoCodec::H264
                } else {
                    VideoCodec::Rgb332
                },
            })
        }
        Some(value) if value.starts_with('-') => Err("opción desconocida"),
        address => {
            if args.next().is_some() {
                return Err("solo se admite una dirección");
            }
            Ok(Mode::Serve {
                address: address.unwrap_or(DEFAULT_ADDRESS).into(),
                monitor: None,
                codec: VideoCodec::Rgb332,
            })
        }
    }
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

fn run_server(address: String, monitor: Option<usize>, codec: VideoCodec) -> Result<(), AnyError> {
    #[cfg(not(windows))]
    if monitor.is_some() {
        return Err("la captura de pantalla solo está disponible en Windows".into());
    }
    let listener = TcpListener::bind(&address)?;
    emit(Level::Info, "host_listening", &[("address", &address)]);

    loop {
        let (stream, peer) = listener.accept()?;
        emit(
            Level::Info,
            "client_connected",
            &[("peer", &peer.to_string())],
        );
        if let Err(error) = serve(stream, monitor, codec) {
            emit(
                Level::Warn,
                "session_closed",
                &[("error", &error.to_string())],
            );
        }
    }
}

fn serve(mut stream: TcpStream, monitor: Option<usize>, codec: VideoCodec) -> Result<(), AnyError> {
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
    match read_message(&mut stream)? {
        Message::Capabilities {
            max_width,
            max_height,
            max_fps,
            codecs,
        } if codecs & codec.capability() != 0
            && width <= max_width
            && height <= max_height
            && fps <= max_fps =>
        {
            state = state.apply(ConnectionEvent::Negotiated)?;
        }
        Message::Capabilities { .. } => {
            return Err("el cliente no admite la configuración solicitada".into());
        }
        _ => return Err("se esperaba Capabilities".into()),
    }

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
    write_message(&mut stream, &Message::Start)?;
    state.apply(ConnectionEvent::Started)?;

    if let Some(index) = monitor {
        return capture_stream(stream, index, session_id, codec);
    }

    let started = Instant::now();
    let mut number = 0_u64;
    loop {
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

#[cfg(windows)]
fn capture_stream(
    stream: TcpStream,
    index: usize,
    session_id: u64,
    codec: VideoCodec,
) -> Result<(), AnyError> {
    match codec {
        VideoCodec::Rgb332 => capture::stream(stream, index, session_id),
        VideoCodec::H264 => capture::stream_h264(stream, index, session_id),
    }
}

#[cfg(not(windows))]
fn capture_stream(
    _stream: TcpStream,
    _index: usize,
    _session_id: u64,
    _codec: VideoCodec,
) -> Result<(), AnyError> {
    Err("la captura de pantalla solo está disponible en Windows".into())
}

fn stream_format(codec: VideoCodec, monitor: Option<usize>) -> Result<(u16, u16, u16), AnyError> {
    match codec {
        VideoCodec::Rgb332 => Ok((WIDTH, HEIGHT, FPS)),
        VideoCodec::H264 => h264_format(monitor.ok_or("H.264 requiere un monitor")?),
    }
}

#[cfg(windows)]
fn h264_format(index: usize) -> Result<(u16, u16, u16), AnyError> {
    let (width, height) = capture::size(index)?;
    Ok((width, height, capture::ENCODE_FPS as u16))
}

#[cfg(not(windows))]
fn h264_format(_index: usize) -> Result<(u16, u16, u16), AnyError> {
    Err("la codificación H.264 solo está disponible en Windows".into())
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
    fn mode_accepts_server_address_or_capture_monitor() {
        assert_eq!(
            parse_mode(Vec::new()),
            Ok(Mode::Serve {
                address: DEFAULT_ADDRESS.into(),
                monitor: None,
                codec: VideoCodec::Rgb332,
            })
        );
        assert_eq!(
            parse_mode(["--capture-test".into(), "2".into()]),
            Ok(Mode::CaptureTest(2))
        );
        assert_eq!(
            parse_mode(["--encode-test".into()]),
            Ok(Mode::EncodeTest(1))
        );
        assert_eq!(
            parse_mode(["--capture".into(), "2".into(), "127.0.0.1:9".into()]),
            Ok(Mode::Serve {
                address: "127.0.0.1:9".into(),
                monitor: Some(2),
                codec: VideoCodec::Rgb332,
            })
        );
        assert_eq!(
            parse_mode(["--h264".into(), "1".into()]),
            Ok(Mode::Serve {
                address: DEFAULT_ADDRESS.into(),
                monitor: Some(1),
                codec: VideoCodec::H264,
            })
        );
        assert!(parse_mode(["--capture-test".into(), "0".into()]).is_err());
        assert!(parse_mode(["--unknown".into()]).is_err());
    }
}
