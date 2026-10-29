//! Test helper: lets unencrypted test peers (the Python harnesses) talk to the
//! real client or host through the encrypted transport. Loopback use only.
//!
//!   secure_relay provision HOST_DIR CLIENT_DIR
//!   secure_relay host HOST_DIR LISTEN UPSTREAM     encrypted in, plain out
//!   secure_relay client CLIENT_DIR LISTEN UPSTREAM plain in, encrypted out
//!
//! Prints `secure_relay_listening=ADDRESS` once it accepts connections.
use std::error::Error;
use std::io::{self, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;
use windowdeck_transport::secure::{self, Control, Established, Incoming};
use windowdeck_transport::{Intent, Reject, Role, SecureStream, Trust};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["provision", host, client] => {
            let host = Trust::open(host, Role::Host)?;
            let client = Trust::open(client, Role::Client)?;
            host.add(client.identity()?.public(), "secure-relay-client")?;
            client.add(host.identity()?.public(), "secure-relay-host")?;
            Ok(())
        }
        [role @ ("host" | "client"), dir, listen, upstream] => {
            let role = if *role == "host" {
                Role::Host
            } else {
                Role::Client
            };
            let trust = Trust::open(dir, role)?;
            let listener = TcpListener::bind(listen)?;
            println!("secure_relay_listening={}", listener.local_addr()?);
            io::stdout().flush()?;
            for socket in listener.incoming() {
                let socket = socket?;
                let (trust, upstream) = (trust.clone(), upstream.to_string());
                thread::spawn(move || {
                    let result = match role {
                        Role::Host => serve_host(socket, &trust, &upstream),
                        Role::Client => serve_client(socket, &trust, &upstream),
                    };
                    if let Err(error) = result {
                        eprintln!("secure_relay_connection_failed error={error}");
                    }
                });
            }
            Ok(())
        }
        _ => Err("uso: secure_relay provision|host|client ...".into()),
    }
}

fn serve_host(socket: TcpStream, trust: &Trust, upstream: &str) -> Result<(), Box<dyn Error>> {
    let identity = trust.identity()?;
    let mut accepted = match secure::accept(socket, &identity, "secure-relay-host")? {
        Incoming::Secure(accepted) => accepted,
        Incoming::Legacy(legacy) => {
            legacy.reject(3, "secure_relay requiere el transporte cifrado")?;
            return Ok(());
        }
    };
    let paired = trust.is_paired(&accepted.peer.key)?;
    match accepted.intent {
        Intent::Session if paired => {
            accepted.stream.send_control(&Control::Accepted)?;
            pump(accepted.stream, TcpStream::connect(upstream)?)
        }
        Intent::Pair if paired && accepted.knows_host => {
            accepted.stream.send_control(&Control::Accepted)?;
            Ok(())
        }
        _ => {
            accepted
                .stream
                .send_control(&Control::Rejected(Reject::NotPaired))?;
            Ok(())
        }
    }
}

fn serve_client(socket: TcpStream, trust: &Trust, upstream: &str) -> Result<(), Box<dyn Error>> {
    let identity = trust.identity()?;
    let remote = TcpStream::connect(upstream)?;
    match secure::connect(
        remote,
        &identity,
        "secure-relay-client",
        Intent::Session,
        |key| trust.is_paired(key),
    )? {
        Established::Session(stream, _) => pump(stream, socket),
        _ => Err("respuesta inesperada del host".into()),
    }
}

/// Copies both directions and propagates each end-of-stream separately, so a
/// peer closing mid-message is seen as such on the other side.
fn pump(secure: SecureStream, plain: TcpStream) -> Result<(), Box<dyn Error>> {
    secure.set_read_timeout(None)?;
    secure.set_write_timeout(None)?;
    let (mut secure_in, mut plain_out) = (secure.try_clone()?, plain.try_clone()?);
    let downstream = thread::spawn(move || {
        let result = io::copy(&mut secure_in, &mut plain_out);
        // A failed channel must also stop the other direction.
        let how = if result.is_ok() {
            Shutdown::Write
        } else {
            Shutdown::Both
        };
        let _ = plain_out.shutdown(how);
        if result.is_err() {
            let _ = secure_in.shutdown(Shutdown::Both);
        }
        result
    });
    let (mut plain_in, mut secure_out) = (plain, secure);
    let upstream = io::copy(&mut plain_in, &mut secure_out);
    let _ = secure_out.shutdown(Shutdown::Write);
    if upstream.is_err() {
        let _ = plain_in.shutdown(Shutdown::Both);
        let _ = secure_out.shutdown(Shutdown::Both);
    }
    let downstream = downstream.join().map_err(|_| "relay thread panicked")?;
    upstream?;
    downstream?;
    Ok(())
}
