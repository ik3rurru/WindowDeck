use std::env;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windowdeck_diagnostics::{Level, emit};
use windowdeck_protocol::{ConnectionEvent, ConnectionState, Message, read_message, write_message};

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
        Mode::Serve(address) => run_server(address),
        Mode::Capture(index) => capture_test(index),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Serve(String),
    Capture(usize),
}

fn parse_mode(args: impl IntoIterator<Item = String>) -> Result<Mode, &'static str> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("--capture-test") => {
            let index = args
                .next()
                .map(|value| value.parse().map_err(|_| "índice de monitor inválido"))
                .transpose()?
                .unwrap_or(1);
            if index == 0 || args.next().is_some() {
                return Err("usa --capture-test [N], con N mayor que cero");
            }
            Ok(Mode::Capture(index))
        }
        Some(value) if value.starts_with('-') => Err("opción desconocida"),
        address => {
            if args.next().is_some() {
                return Err("solo se admite una dirección");
            }
            Ok(Mode::Serve(address.unwrap_or(DEFAULT_ADDRESS).into()))
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

fn run_server(address: String) -> Result<(), AnyError> {
    let listener = TcpListener::bind(&address)?;
    emit(Level::Info, "host_listening", &[("address", &address)]);

    loop {
        let (stream, peer) = listener.accept()?;
        emit(
            Level::Info,
            "client_connected",
            &[("peer", &peer.to_string())],
        );
        if let Err(error) = serve(stream) {
            emit(
                Level::Warn,
                "session_closed",
                &[("error", &error.to_string())],
            );
        }
    }
}

fn serve(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
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

    match read_message(&mut stream)? {
        Message::Capabilities { .. } => state = state.apply(ConnectionEvent::Negotiated)?,
        _ => return Err("se esperaba Capabilities".into()),
    }

    let session_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros() as u64;
    write_message(
        &mut stream,
        &Message::SessionConfig {
            session_id,
            width: WIDTH,
            height: HEIGHT,
            fps: FPS,
        },
    )?;
    write_message(&mut stream, &Message::Start)?;
    state.apply(ConnectionEvent::Started)?;

    let started = Instant::now();
    let mut number = 0_u64;
    loop {
        let frame_started = Instant::now();
        let captured_micros = started.elapsed().as_micros() as u64;
        write_message(
            &mut stream,
            &Message::Frame {
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

fn pattern(frame: u64, captured_micros: u64) -> Vec<u8> {
    let mut pixels = (0..usize::from(WIDTH) * usize::from(HEIGHT))
        .map(|index| (((index % usize::from(WIDTH)) as u64 + frame) % 9) as u8)
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
                        pixels[pixel_y * usize::from(WIDTH) + pixel_x] = 9;
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
        assert!(first.contains(&9));
        assert_ne!(first, pattern(1, 1_000));
    }

    #[test]
    fn mode_accepts_server_address_or_capture_monitor() {
        assert_eq!(
            parse_mode(Vec::new()),
            Ok(Mode::Serve(DEFAULT_ADDRESS.into()))
        );
        assert_eq!(
            parse_mode(["--capture-test".into(), "2".into()]),
            Ok(Mode::Capture(2))
        );
        assert!(parse_mode(["--capture-test".into(), "0".into()]).is_err());
        assert!(parse_mode(["--unknown".into()]).is_err());
    }
}
