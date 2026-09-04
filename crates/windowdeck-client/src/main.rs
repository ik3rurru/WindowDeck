use std::env;
use std::error::Error;
use std::io::{self, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use windowdeck_protocol::{ConnectionEvent, ConnectionState, Message, read_message, write_message};

fn main() -> Result<(), Box<dyn Error>> {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:48150".into());
    let mut stream = TcpStream::connect(&address)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    println!("Conectado a {address}");

    write_message(
        &mut stream,
        &Message::Hello {
            app_version: env!("CARGO_PKG_VERSION").into(),
        },
    )?;
    let mut state = ConnectionState::AwaitingHello;
    match read_message(&mut stream)? {
        Message::Hello { app_version } => {
            println!("Host WindowDeck {app_version}");
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
        } => {
            println!("Sesión: {width}x{height} a {fps} FPS");
            state = state.apply(ConnectionEvent::Negotiated)?;
            (width, height)
        }
        _ => return Err("se esperaba SessionConfig".into()),
    };
    match read_message(&mut stream)? {
        Message::Start => state = state.apply(ConnectionEvent::Started)?,
        _ => return Err("se esperaba Start".into()),
    }

    let started = Instant::now();
    let mut received = 0_u64;
    while state == ConnectionState::Streaming {
        match read_message(&mut stream)? {
            Message::Frame { number, pixels, .. } => {
                received += 1;
                render(width, height, number, &pixels)?;
                if received.is_multiple_of(10) {
                    eprintln!(
                        "frames recibidos: {received}, {:.1} FPS",
                        received as f64 / started.elapsed().as_secs_f64()
                    );
                }
            }
            Message::Ping { nonce } => write_message(&mut stream, &Message::Pong { nonce })?,
            Message::Stop => state = state.apply(ConnectionEvent::Stopped)?,
            Message::Error { code, message } => {
                return Err(format!("host error {code}: {message}").into());
            }
            _ => return Err("mensaje inesperado durante streaming".into()),
        }
    }
    Ok(())
}

fn render(width: u16, height: u16, frame: u64, pixels: &[u8]) -> Result<(), Box<dyn Error>> {
    if pixels.len() != usize::from(width) * usize::from(height) {
        return Err("frame con dimensiones inválidas".into());
    }
    let mut output = format!("\x1b[2J\x1b[HWindowDeck frame {frame}\n");
    for row in pixels.chunks_exact(usize::from(width)) {
        for pixel in row {
            output.push(char::from(b'0' + *pixel));
        }
        output.push('\n');
    }
    print!("{output}");
    io::stdout().flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_rejects_wrong_pixel_count() {
        assert!(render(2, 2, 0, &[0, 1, 2]).is_err());
    }
}
