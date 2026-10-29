//! Pairing by comparing a six-digit code on both screens.
//!
//! The code is derived from the Noise handshake transcript and two random
//! values. The client commits to its value before seeing the host's, so an
//! attacker relaying two separate handshakes cannot choose values that make the
//! codes match: each attempt succeeds with probability 1 in 1,000,000, and every
//! attempt needs a person to accept it on both devices.
use crate::SecureError;
use crate::secure::{Control, Peer, SecureStream, TRANSCRIPT_LEN};
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::time::{Duration, Instant};

/// How long people have to compare and confirm the code.
pub const TIMEOUT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(100);
const COMMIT_LABEL: &[u8] = b"WindowDeck pairing commitment 1";
const CODE_LABEL: &[u8] = b"WindowDeck pairing code 1";

/// What both screens show: the same code and the other device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Six digits grouped as `123 456`.
    pub code: String,
    pub peer: Peer,
}

/// The user-facing side of a ceremony. Implementations must not block.
pub trait Prompt {
    fn show(&mut self, request: &Request) -> io::Result<()>;
    /// The local person's answer, once given.
    fn decision(&mut self) -> io::Result<Option<bool>>;
    /// The other device's answer, so the interface can update.
    fn peer_decided(&mut self, _accepted: bool) {}
    /// Abandons the ceremony, for example because the host is stopping.
    fn cancelled(&mut self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Both people accepted: store the peer's key.
    Paired,
    RejectedHere,
    RejectedByPeer,
    Expired,
    Cancelled,
}

/// Client role: commits first, reveals after receiving the host's value.
pub fn pair_as_client(
    stream: &mut SecureStream,
    transcript: &[u8; TRANSCRIPT_LEN],
    peer: &Peer,
    prompt: &mut dyn Prompt,
    timeout: Duration,
) -> Result<Outcome, SecureError> {
    let own = random()?;
    stream.send_control(&Control::PairCommit(commitment(transcript, &own)))?;
    let Control::PairNonce(host) = stream.read_control()? else {
        return Err(SecureError::Protocol("se esperaba el valor del PC"));
    };
    stream.send_control(&Control::PairReveal(own))?;
    confirm(stream, code(transcript, &own, &host), peer, prompt, timeout)
}

/// Host role: answers the commitment and verifies the revealed value.
pub fn pair_as_host(
    stream: &mut SecureStream,
    transcript: &[u8; TRANSCRIPT_LEN],
    peer: &Peer,
    prompt: &mut dyn Prompt,
    timeout: Duration,
) -> Result<Outcome, SecureError> {
    let Control::PairCommit(committed) = stream.read_control()? else {
        return Err(SecureError::Protocol(
            "se esperaba el compromiso del cliente",
        ));
    };
    let own = random()?;
    stream.send_control(&Control::PairNonce(own))?;
    let Control::PairReveal(client) = stream.read_control()? else {
        return Err(SecureError::Protocol("se esperaba el valor del cliente"));
    };
    if commitment(transcript, &client) != committed {
        return Err(SecureError::Protocol(
            "el valor de emparejamiento no coincide con su compromiso",
        ));
    }
    confirm(
        stream,
        code(transcript, &client, &own),
        peer,
        prompt,
        timeout,
    )
}

fn confirm(
    stream: &mut SecureStream,
    code: String,
    peer: &Peer,
    prompt: &mut dyn Prompt,
    timeout: Duration,
) -> Result<Outcome, SecureError> {
    let previous = stream.read_timeout()?;
    stream.set_read_timeout(Some(POLL))?;
    let result = (|| {
        prompt.show(&Request {
            code,
            peer: peer.clone(),
        })?;
        let deadline = Instant::now() + timeout;
        let mut local = None;
        let mut remote = None;
        loop {
            if prompt.cancelled() {
                refuse(stream);
                return Ok(Outcome::Cancelled);
            }
            if local.is_none()
                && let Some(accepted) = prompt.decision()?
            {
                local = Some(accepted);
                if !accepted {
                    refuse(stream);
                    return Ok(Outcome::RejectedHere);
                }
                stream.send_control(&Control::PairVerdict(true))?;
            }
            if remote.is_none() {
                match stream.read_control() {
                    Ok(Control::PairVerdict(accepted)) => {
                        remote = Some(accepted);
                        prompt.peer_decided(accepted);
                        if !accepted {
                            return Ok(Outcome::RejectedByPeer);
                        }
                    }
                    Ok(_) => return Err(SecureError::Protocol("se esperaba una confirmación")),
                    Err(SecureError::Io(error))
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => return Err(error),
                }
            } else {
                std::thread::sleep(POLL);
            }
            if local == Some(true) && remote == Some(true) {
                return Ok(Outcome::Paired);
            }
            if Instant::now() >= deadline {
                refuse(stream);
                return Ok(Outcome::Expired);
            }
        }
    })();
    let restored = stream.set_read_timeout(previous);
    let outcome = result?;
    restored?;
    Ok(outcome)
}

/// Sends a negative verdict and lingers until the peer closes, so closing with
/// its unread verdict pending cannot reset the connection before ours arrives.
fn refuse(stream: &mut SecureStream) {
    if stream.send_control(&Control::PairVerdict(false)).is_err() {
        return;
    }
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut sink = [0; 64];
    while Instant::now() < deadline {
        match stream.read(&mut sink) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
}

fn random() -> Result<[u8; 32], SecureError> {
    let mut value = [0; 32];
    getrandom::fill(&mut value).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(value)
}

fn commitment(transcript: &[u8; TRANSCRIPT_LEN], value: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(COMMIT_LABEL)
        .chain_update(transcript)
        .chain_update(value)
        .finalize()
        .into()
}

fn code(transcript: &[u8; TRANSCRIPT_LEN], client: &[u8; 32], host: &[u8; 32]) -> String {
    let digest = Sha256::new()
        .chain_update(CODE_LABEL)
        .chain_update(transcript)
        .chain_update(client)
        .chain_update(host)
        .finalize();
    let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000;
    format!("{:03} {:03}", number / 1000, number % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_depends_on_every_input_and_has_six_digits() {
        let transcript = [1; TRANSCRIPT_LEN];
        let (client, host) = ([2; 32], [3; 32]);
        let reference = code(&transcript, &client, &host);
        assert_eq!(reference.len(), 7);
        assert!(
            reference
                .chars()
                .enumerate()
                .all(|(index, c)| if index == 3 {
                    c == ' '
                } else {
                    c.is_ascii_digit()
                })
        );
        assert_eq!(reference, code(&transcript, &client, &host));
        assert_ne!(reference, code(&[9; TRANSCRIPT_LEN], &client, &host));
        assert_ne!(reference, code(&transcript, &[9; 32], &host));
        assert_ne!(reference, code(&transcript, &client, &[9; 32]));
        assert_ne!(reference, code(&transcript, &host, &client));
    }

    #[test]
    fn commitment_binds_value_and_transcript() {
        let transcript = [4; TRANSCRIPT_LEN];
        let value = [5; 32];
        assert_eq!(
            commitment(&transcript, &value),
            commitment(&transcript, &value)
        );
        assert_ne!(
            commitment(&transcript, &value),
            commitment(&transcript, &[6; 32])
        );
        assert_ne!(
            commitment(&transcript, &value),
            commitment(&[7; TRANSCRIPT_LEN], &value)
        );
        assert_ne!(commitment(&transcript, &value), value);
    }
}
