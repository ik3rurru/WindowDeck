//! Optional protocol-3 connection validation. The high capability bit is not a codec.
//! Probe the video direction under load before allowing any display activation.
use crate::{Message, ProtocolError, read_message, write_message};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub const CAPABILITY: u8 = 0x80;
pub const PROBE_BYTES: usize = 62_500;
pub const BURST_PACKETS: usize = 8;
pub const PROBE_ROUNDS: u64 = 8;
const INTERVAL: Duration = Duration::from_millis(250);
const ROUND_TIMEOUT: Duration = Duration::from_millis(750);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(4);
pub const STABLE_SESSION: Duration = Duration::from_secs(30);

pub struct Validation {
    pub elapsed: Duration,
    pub slowest_round: Duration,
}

/// Eight 500 kB bursts at a nominal 16 Mbps, each acknowledged after receipt.
/// Timeouts bound the whole operation, including partial TCP reads and writes.
pub fn validate(
    stream: &mut TcpStream,
    session: u64,
    cancelled: impl Fn() -> bool,
) -> Result<Validation, ProtocolError> {
    let old_read = stream.read_timeout()?;
    let old_write = stream.write_timeout()?;
    let result = (|| {
        let started = Instant::now();
        let mut slowest_round = Duration::ZERO;
        for round in 0..PROBE_ROUNDS {
            let round_started = Instant::now();
            let mut socket = DeadlineSocket {
                stream,
                deadline: (round_started + ROUND_TIMEOUT).min(started + TOTAL_TIMEOUT),
                cancelled: &cancelled,
            };
            let nonce = session.wrapping_add(round);
            let probe = Message::ConnectionProbe {
                nonce,
                payload: vec![0x5a; PROBE_BYTES],
            };
            for _ in 0..BURST_PACKETS {
                write_message(&mut socket, &probe)?;
            }
            write_message(&mut socket, &Message::Ping { nonce })?;
            match read_message(&mut socket)? {
                Message::Pong { nonce: reply } if reply == nonce => {}
                Message::Stop => return Err(io::Error::from(io::ErrorKind::Interrupted).into()),
                _ => {
                    return Err(ProtocolError::Invalid(
                        "invalid connection probe acknowledgement",
                    ));
                }
            }
            socket.remaining()?;
            slowest_round = slowest_round.max(round_started.elapsed());
            while round_started.elapsed() < INTERVAL {
                if cancelled() {
                    return Err(io::Error::from(io::ErrorKind::Interrupted).into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if started.elapsed() > TOTAL_TIMEOUT {
            return Err(
                io::Error::new(io::ErrorKind::TimedOut, "connection validation timed out").into(),
            );
        }
        Ok(Validation {
            elapsed: started.elapsed(),
            slowest_round,
        })
    })();
    // Restore both socket options even when validation fails.
    let restored_read = stream.set_read_timeout(old_read);
    let restored_write = stream.set_write_timeout(old_write);
    let validation = result?;
    restored_read?;
    restored_write?;
    Ok(validation)
}

struct DeadlineSocket<'a, F> {
    stream: &'a mut TcpStream,
    deadline: Instant,
    cancelled: &'a F,
}

impl<F: Fn() -> bool> DeadlineSocket<'_, F> {
    fn remaining(&self) -> io::Result<Duration> {
        if (self.cancelled)() {
            // read_exact/write_all retry Interrupted forever; cancellation is final.
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "connection validation timed out")
            })
    }
}

impl<F: Fn() -> bool> Read for DeadlineSocket<'_, F> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(bytes)
    }
}

impl<F: Fn() -> bool> Write for DeadlineSocket<'_, F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

/// A Pong confirms a complete burst, never merely an open TCP connection.
pub struct ProbeReceiver {
    session: u64,
    round: u64,
    packets: usize,
}

impl ProbeReceiver {
    pub fn new(session: u64) -> Self {
        Self {
            session,
            round: 0,
            packets: 0,
        }
    }

    pub fn receive(&mut self, message: Message) -> Result<Option<Message>, ProtocolError> {
        if self.round >= PROBE_ROUNDS {
            return Err(ProtocolError::Invalid("too many connection probes"));
        }
        let expected = self.session.wrapping_add(self.round);
        match message {
            Message::ConnectionProbe { nonce, payload }
                if nonce == expected
                    && self.packets < BURST_PACKETS
                    && payload.len() == PROBE_BYTES
                    && payload.iter().all(|&byte| byte == 0x5a) =>
            {
                self.packets += 1;
                Ok(None)
            }
            Message::Ping { nonce } if nonce == expected && self.packets == BURST_PACKETS => {
                self.round += 1;
                self.packets = 0;
                Ok(Some(Message::Pong { nonce }))
            }
            _ => Err(ProtocolError::Invalid(
                "incomplete or invalid connection probe",
            )),
        }
    }

    pub fn finish(&self) -> Result<(), ProtocolError> {
        // Hosts predating the capability send Start directly after SessionConfig.
        if self.packets == 0 && matches!(self.round, 0 | PROBE_ROUNDS) {
            Ok(())
        } else {
            Err(ProtocolError::Invalid(
                "Start before connection validation completed",
            ))
        }
    }
}

/// Do not reset on a successful handshake: short-lived sessions are still failures.
#[derive(Default)]
pub struct RetryBackoff {
    attempts: u32,
}

impl RetryBackoff {
    pub fn session_ended(&mut self, duration: Duration) {
        if duration >= STABLE_SESSION {
            self.attempts = 0;
        }
    }
    pub fn next_delay(&mut self) -> Duration {
        let seconds = (1_u64 << self.attempts.min(5)).min(30);
        self.attempts = self.attempts.saturating_add(1);
        Duration::from_secs(seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn burst(receiver: &mut ProbeReceiver, nonce: u64) {
        for _ in 0..BURST_PACKETS {
            assert!(
                receiver
                    .receive(Message::ConnectionProbe {
                        nonce,
                        payload: vec![0x5a; PROBE_BYTES],
                    })
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn probes_require_complete_ordered_bursts_before_start() {
        let mut receiver = ProbeReceiver::new(42);
        assert!(receiver.finish().is_ok()); // Legacy host.
        assert!(receiver.receive(Message::Ping { nonce: 42 }).is_err());
        assert!(
            receiver
                .receive(Message::ConnectionProbe {
                    nonce: 43,
                    payload: vec![0x5a; PROBE_BYTES],
                })
                .is_err()
        );
        for round in 0..PROBE_ROUNDS {
            burst(&mut receiver, 42 + round);
            assert!(receiver.finish().is_err());
            assert!(receiver.receive(Message::Ping { nonce: 900 }).is_err());
            assert_eq!(
                receiver
                    .receive(Message::Ping { nonce: 42 + round })
                    .unwrap(),
                Some(Message::Pong { nonce: 42 + round })
            );
            assert_eq!(receiver.finish().is_ok(), round + 1 == PROBE_ROUNDS);
        }
        assert!(receiver.receive(Message::Ping { nonce: 50 }).is_err());
    }

    #[test]
    fn validation_exchanges_load_and_restores_timeouts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let mut socket = TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut receiver = ProbeReceiver::new(8);
            for _ in 0..PROBE_ROUNDS * (BURST_PACKETS as u64 + 1) {
                if let Some(reply) = receiver
                    .receive(read_message(&mut socket).unwrap())
                    .unwrap()
                {
                    write_message(&mut socket, &reply).unwrap();
                }
            }
            receiver.finish().unwrap();
        });
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_nodelay(true).unwrap();
        let stats = validate(&mut socket, 8, || false).unwrap();
        assert!(stats.elapsed >= INTERVAL * PROBE_ROUNDS as u32);
        assert!(stats.elapsed < TOTAL_TIMEOUT);
        assert_eq!(socket.read_timeout().unwrap(), None);
        assert_eq!(socket.write_timeout().unwrap(), None);
        peer.join().unwrap();
    }

    #[test]
    fn silent_and_partial_acknowledgements_time_out_without_start() {
        for partial in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let start = Instant::now();
                let result = validate(&mut socket, 2, || false);
                assert!(result.is_err());
                assert!(start.elapsed() < Duration::from_secs(2));
                assert_eq!(socket.read_timeout().unwrap(), None);
                assert_eq!(socket.write_timeout().unwrap(), None);
            });
            for _ in 0..BURST_PACKETS + 1 {
                read_message(&mut peer).unwrap();
            }
            if partial {
                peer.write_all(&[0, 0, 0, 11, 0]).unwrap();
            }
            server.join().unwrap();
        }
    }

    #[test]
    fn cancellation_does_not_wait_for_a_probe() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut socket, _) = listener.accept().unwrap();
        assert!(matches!(validate(&mut socket, 1, || true),
            Err(ProtocolError::Io(error)) if error.kind() == io::ErrorKind::ConnectionAborted));
    }

    #[test]
    fn only_a_stable_session_resets_retry_delay() {
        let mut retry = RetryBackoff::default();
        for seconds in [1, 2, 4, 8, 16, 30, 30] {
            retry.session_ended(Duration::from_secs(2));
            assert_eq!(retry.next_delay(), Duration::from_secs(seconds));
        }
        retry.session_ended(STABLE_SESSION);
        assert_eq!(retry.next_delay(), Duration::from_secs(1));
    }
}
