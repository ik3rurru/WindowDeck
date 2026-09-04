use std::env;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windowdeck_protocol::{ConnectionEvent, ConnectionState, Message, read_message, write_message};

const WIDTH: u16 = 32;
const HEIGHT: u16 = 12;
const FPS: u16 = 10;

fn main() -> Result<(), Box<dyn Error>> {
    let address = env::args().nth(1).unwrap_or_else(|| "0.0.0.0:48150".into());
    let listener = TcpListener::bind(&address)?;
    println!("WindowDeck Host escuchando en {address}");

    loop {
        let (stream, peer) = listener.accept()?;
        println!("Cliente conectado: {peer}");
        if let Err(error) = serve(stream) {
            eprintln!("Sesión cerrada: {error}");
        }
    }
}

fn serve(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut state = ConnectionState::AwaitingHello;

    match read_message(&mut stream)? {
        Message::Hello { app_version } => {
            println!("Cliente WindowDeck {app_version}");
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
                pixels: pattern(number),
            },
        )?;
        number += 1;
        if number.is_multiple_of(u64::from(FPS)) {
            println!(
                "frames enviados: {number}, {:.1} FPS",
                number as f64 / started.elapsed().as_secs_f64()
            );
        }
        thread::sleep(
            Duration::from_secs_f64(1.0 / f64::from(FPS)).saturating_sub(frame_started.elapsed()),
        );
    }
}

fn pattern(frame: u64) -> Vec<u8> {
    (0..usize::from(WIDTH) * usize::from(HEIGHT))
        .map(|index| (((index % usize::from(WIDTH)) as u64 + frame) % 10) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_has_expected_size_and_moves() {
        assert_eq!(pattern(0).len(), usize::from(WIDTH) * usize::from(HEIGHT));
        assert_ne!(pattern(0), pattern(1));
    }
}
