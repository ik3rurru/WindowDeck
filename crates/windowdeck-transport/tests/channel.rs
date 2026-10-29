//! Loopback tests of the encrypted channel, pairing and revocation.
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use windowdeck_protocol::{Message, read_message, write_message};
use windowdeck_transport::pairing::{self, Outcome, Prompt, Request};
use windowdeck_transport::secure::{self, Accepted, Control, Established, Incoming};
use windowdeck_transport::{Intent, Reject, Role, SecureError, SecureStream, Trust};

const HOST_NAME: &str = "PC de prueba";
const CLIENT_NAME: &str = "steamdeck";

fn temporary_dir(name: &str) -> PathBuf {
    let mut random = [0; 8];
    getrandom::fill(&mut random).unwrap();
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("windowdeck-channel-{name}-{suffix}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Devices {
    host: Trust,
    client: Trust,
}

impl Devices {
    fn new(name: &str, paired: bool) -> Self {
        let devices = Self {
            host: Trust::open(temporary_dir(&format!("{name}-host")), Role::Host).unwrap(),
            client: Trust::open(temporary_dir(&format!("{name}-client")), Role::Client).unwrap(),
        };
        if paired {
            devices.pair();
        }
        devices
    }

    fn pair(&self) {
        let host = self.host.identity().unwrap().public();
        let client = self.client.identity().unwrap().public();
        self.host.add(client, CLIENT_NAME).unwrap();
        self.client.add(host, HOST_NAME).unwrap();
    }

    fn connect(&self, address: SocketAddr, intent: Intent) -> Result<Established, SecureError> {
        let identity = self.client.identity().unwrap();
        let socket = TcpStream::connect(address).unwrap();
        secure::connect(socket, &identity, CLIENT_NAME, intent, |key| {
            self.client.is_paired(key)
        })
    }
}

impl Drop for Devices {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.host.dir());
        let _ = std::fs::remove_dir_all(self.client.dir());
    }
}

/// Accepts one connection and applies the host policy used by windowdeck-host.
fn host_once(
    trust: Trust,
    host_prompt: Option<Scripted>,
) -> (SocketAddr, thread::JoinHandle<HostResult>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        let identity = trust.identity().unwrap();
        let accepted = match secure::accept(socket, &identity, HOST_NAME) {
            Ok(Incoming::Secure(accepted)) => accepted,
            Ok(Incoming::Legacy(legacy)) => {
                legacy.reject(3, "actualiza la Deck").unwrap();
                return HostResult::Legacy;
            }
            Err(error) => return HostResult::Failed(error.to_string()),
        };
        let Accepted {
            mut stream,
            peer,
            intent,
            knows_host,
            transcript,
        } = *accepted;
        let authorized = trust.is_paired(&peer.key).unwrap();
        match intent {
            Intent::Session if authorized => {
                stream.send_control(&Control::Accepted).unwrap();
                HostResult::Session(stream, peer.name)
            }
            Intent::Session => {
                stream
                    .send_control(&Control::Rejected(Reject::NotPaired))
                    .unwrap();
                HostResult::Refused
            }
            Intent::Pair if authorized && knows_host => {
                stream.send_control(&Control::Accepted).unwrap();
                HostResult::AlreadyPaired
            }
            Intent::Pair => {
                stream.send_control(&Control::PairingRequired).unwrap();
                let mut prompt = host_prompt.expect("pairing prompt");
                let timeout = prompt.timeout;
                let outcome =
                    pairing::pair_as_host(&mut stream, &transcript, &peer, &mut prompt, timeout);
                match outcome {
                    Ok(Outcome::Paired) => {
                        trust.add(peer.key, &peer.name).unwrap();
                        HostResult::Paired(Outcome::Paired, prompt.shown)
                    }
                    Ok(outcome) => HostResult::Paired(outcome, prompt.shown),
                    Err(error) => HostResult::Failed(error.to_string()),
                }
            }
        }
    });
    (address, handle)
}

enum HostResult {
    Session(SecureStream, String),
    Refused,
    AlreadyPaired,
    Paired(Outcome, Option<Request>),
    Legacy,
    Failed(String),
}

struct Scripted {
    answer: Option<bool>,
    after: Duration,
    shown: Option<Request>,
    shown_at: Option<Instant>,
    timeout: Duration,
}

impl Scripted {
    fn answer(answer: Option<bool>) -> Self {
        Self {
            answer,
            after: Duration::from_millis(50),
            shown: None,
            shown_at: None,
            timeout: Duration::from_secs(5),
        }
    }
}

impl Prompt for Scripted {
    fn show(&mut self, request: &Request) -> io::Result<()> {
        self.shown = Some(request.clone());
        self.shown_at = Some(Instant::now());
        Ok(())
    }
    fn decision(&mut self) -> io::Result<Option<bool>> {
        Ok(self
            .shown_at
            .filter(|shown| shown.elapsed() >= self.after)
            .and(self.answer))
    }
}

fn pair_client(devices: &Devices, address: SocketAddr, prompt: &mut Scripted) -> Outcome {
    let Established::PairingRequired(pairing) = devices.connect(address, Intent::Pair).unwrap()
    else {
        panic!("pairing must be required");
    };
    let mut pairing = *pairing;
    let timeout = prompt.timeout;
    let outcome = pairing::pair_as_client(
        &mut pairing.stream,
        &pairing.transcript,
        &pairing.peer,
        prompt,
        timeout,
    )
    .unwrap();
    if outcome == Outcome::Paired {
        devices
            .client
            .add(pairing.peer.key, &pairing.peer.name)
            .unwrap();
    }
    outcome
}

#[test]
fn paired_devices_exchange_protocol_messages_in_both_directions() {
    let devices = Devices::new("session", true);
    let (address, host) = host_once(devices.host.clone(), None);
    let Established::Session(mut client, peer) = devices.connect(address, Intent::Session).unwrap()
    else {
        panic!("session expected");
    };
    assert_eq!(peer.name, HOST_NAME);
    assert_eq!(peer.key, devices.host.identity().unwrap().public());
    let HostResult::Session(mut server, name) = host.join().unwrap() else {
        panic!("host must accept a paired client");
    };
    assert_eq!(name, CLIENT_NAME);

    // A reader thread on a clone while this thread writes: separate nonces.
    let mut reader = server.try_clone().unwrap();
    let echo = thread::spawn(move || {
        let mut seen = Vec::new();
        loop {
            match read_message(&mut reader) {
                Ok(Message::Stop) => return seen,
                Ok(message) => {
                    write_message(&mut reader, &message).unwrap();
                    seen.push(message);
                }
                Err(error) => panic!("{error}"),
            }
        }
    });
    let mut sent = Vec::new();
    for number in 0..200_u64 {
        let message = Message::VideoChunk {
            session_id: 9,
            frame_number: number,
            captured_micros: number * 16_667,
            fragment_index: 0,
            fragment_count: 1,
            keyframe: number == 0,
            payload: vec![(number % 251) as u8; if number % 2 == 0 { 7 } else { 65_000 }],
        };
        write_message(&mut client, &message).unwrap();
        assert_eq!(read_message(&mut client).unwrap(), message);
        sent.push(message);
    }
    write_message(&mut client, &Message::Stop).unwrap();
    assert_eq!(echo.join().unwrap(), sent);

    // One write larger than a record and than the per-call bound still arrives whole.
    let large: Vec<u8> = (0..3_000_000_u32).map(|value| value as u8).collect();
    let expected = large.clone();
    let receiver = thread::spawn(move || {
        let mut received = vec![0; expected.len()];
        server.read_exact(&mut received).unwrap();
        assert_eq!(received, expected);
        let mut byte = [0];
        assert_eq!(
            server.read(&mut byte).unwrap(),
            0,
            "clean EOF between records"
        );
    });
    client.write_all(&large).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    receiver.join().unwrap();
}

#[test]
fn an_unknown_host_never_receives_a_session_request() {
    let devices = Devices::new("unknown-host", false);
    let client_key = devices.client.identity().unwrap().public();
    devices.host.add(client_key, CLIENT_NAME).unwrap();
    let (address, host) = host_once(devices.host.clone(), None);
    let error = devices
        .connect(address, Intent::Session)
        .err()
        .expect("unknown host");
    assert!(error.needs_pairing());
    let SecureError::HostNotPaired(peer) = &error else {
        panic!("{error}");
    };
    assert_eq!(peer.key, devices.host.identity().unwrap().public());
    assert!(error.to_string().contains(&peer.key.fingerprint()));
    // The client stops before sending its identity or intent.
    assert!(matches!(host.join().unwrap(), HostResult::Failed(_)));
}

#[test]
fn an_unpaired_client_is_refused_before_any_session_starts() {
    let devices = Devices::new("unpaired-client", false);
    let host_key = devices.host.identity().unwrap().public();
    devices.client.add(host_key, HOST_NAME).unwrap();
    let (address, host) = host_once(devices.host.clone(), None);
    let error = devices
        .connect(address, Intent::Session)
        .err()
        .expect("refused");
    assert!(matches!(error, SecureError::Rejected(Reject::NotPaired)));
    assert!(error.needs_pairing());
    assert!(matches!(host.join().unwrap(), HostResult::Refused));
}

#[test]
fn pairing_shows_one_code_and_stores_both_keys_then_revocation_applies() {
    let devices = Devices::new("pairing", false);
    let (address, host) = host_once(devices.host.clone(), Some(Scripted::answer(Some(true))));
    let mut client_prompt = Scripted::answer(Some(true));
    assert_eq!(
        pair_client(&devices, address, &mut client_prompt),
        Outcome::Paired
    );
    let HostResult::Paired(Outcome::Paired, Some(host_request)) = host.join().unwrap() else {
        panic!("host must pair");
    };
    let client_request = client_prompt.shown.unwrap();
    assert_eq!(host_request.code, client_request.code);
    assert_eq!(host_request.peer.name, CLIENT_NAME);
    assert_eq!(client_request.peer.name, HOST_NAME);
    assert_eq!(
        host_request.peer.key,
        devices.client.identity().unwrap().public()
    );

    // Pairing again is a no-op once both ends trust each other.
    let (address, host) = host_once(devices.host.clone(), None);
    assert!(matches!(
        devices.connect(address, Intent::Pair).unwrap(),
        Established::AlreadyPaired(_)
    ));
    assert!(matches!(host.join().unwrap(), HostResult::AlreadyPaired));

    let (address, host) = host_once(devices.host.clone(), None);
    assert!(matches!(
        devices.connect(address, Intent::Session).unwrap(),
        Established::Session(..)
    ));
    assert!(matches!(host.join().unwrap(), HostResult::Session(..)));

    // Revoking on the host refuses the next session of that device.
    let fingerprint = devices.client.identity().unwrap().public().fingerprint();
    devices.host.remove(&fingerprint).unwrap();
    let (address, host) = host_once(devices.host.clone(), None);
    assert!(matches!(
        devices.connect(address, Intent::Session),
        Err(SecureError::Rejected(Reject::NotPaired))
    ));
    assert!(matches!(host.join().unwrap(), HostResult::Refused));
}

#[test]
fn either_person_can_reject_and_nothing_is_stored() {
    for host_accepts in [false, true] {
        let devices = Devices::new("reject", false);
        let (address, host) = host_once(
            devices.host.clone(),
            Some(Scripted::answer(Some(host_accepts))),
        );
        let mut client_prompt = Scripted::answer(Some(!host_accepts));
        let client = pair_client(&devices, address, &mut client_prompt);
        let HostResult::Paired(host_outcome, _) = host.join().unwrap() else {
            panic!("ceremony expected");
        };
        if host_accepts {
            assert_eq!(client, Outcome::RejectedHere);
            assert_eq!(host_outcome, Outcome::RejectedByPeer);
        } else {
            assert_eq!(host_outcome, Outcome::RejectedHere);
            assert_eq!(client, Outcome::RejectedByPeer);
        }
        assert!(devices.host.devices().unwrap().is_empty());
        assert!(devices.client.devices().unwrap().is_empty());
    }
}

#[test]
fn an_unanswered_request_expires() {
    let devices = Devices::new("expire", false);
    let mut host_prompt = Scripted::answer(None);
    host_prompt.timeout = Duration::from_millis(400);
    let (address, host) = host_once(devices.host.clone(), Some(host_prompt));
    let mut client_prompt = Scripted::answer(Some(true));
    client_prompt.timeout = Duration::from_secs(5);
    let started = Instant::now();
    assert_eq!(
        pair_client(&devices, address, &mut client_prompt),
        Outcome::RejectedByPeer
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(matches!(
        host.join().unwrap(),
        HostResult::Paired(Outcome::Expired, _)
    ));
    assert!(devices.host.devices().unwrap().is_empty());
}

#[test]
fn a_revealed_value_must_match_its_commitment() {
    let devices = Devices::new("commitment", false);
    let (address, host) = host_once(devices.host.clone(), Some(Scripted::answer(Some(true))));
    let Established::PairingRequired(pairing) = devices.connect(address, Intent::Pair).unwrap()
    else {
        panic!("pairing expected");
    };
    let mut stream = pairing.stream;
    // Commit to one value, reveal another after seeing the host's value.
    stream.send_control(&Control::PairCommit([1; 32])).unwrap();
    assert!(matches!(
        stream.read_control().unwrap(),
        Control::PairNonce(_)
    ));
    stream.send_control(&Control::PairReveal([2; 32])).unwrap();
    let HostResult::Failed(error) = host.join().unwrap() else {
        panic!("host must abort");
    };
    assert!(error.contains("compromiso"), "{error}");
    assert!(devices.host.devices().unwrap().is_empty());
}

#[test]
fn a_legacy_client_receives_a_readable_refusal() {
    let devices = Devices::new("legacy-client", true);
    let (address, host) = host_once(devices.host.clone(), None);
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write_message(
        &mut socket,
        &Message::Hello {
            app_version: "0.3.1".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        read_message(&mut socket).unwrap(),
        Message::Error { code: 3, .. }
    ));
    assert!(matches!(host.join().unwrap(), HostResult::Legacy));
}

#[test]
fn a_legacy_or_foreign_host_is_reported_without_falling_back() {
    let devices = Devices::new("legacy-host", true);
    for reply in [
        None,
        Some(b"HTTP/1.1".to_vec()),
        Some(b"WDKS\x00\x09\x00\x00".to_vec()),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let expected = reply.clone();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut preamble = [0; 8];
            socket.read_exact(&mut preamble).unwrap();
            // Consume the first handshake message too: closing with unread
            // bytes would reset the connection instead of ending it.
            let mut first = [0; 2 + 32];
            socket.read_exact(&mut first).unwrap();
            // An older host fails to parse the preamble as a frame and closes.
            if let Some(reply) = expected {
                socket.write_all(&reply).unwrap();
            }
            preamble
        });
        let error = devices
            .connect(address, Intent::Session)
            .err()
            .expect("no session");
        assert_eq!(&server.join().unwrap()[..4], b"WDKS");
        match reply.as_deref() {
            None => assert!(error.to_string().contains("versión anterior"), "{error}"),
            Some(b"HTTP/1.1") => assert!(matches!(error, SecureError::Protocol(_))),
            Some(_) => assert!(matches!(error, SecureError::Incompatible(9))),
        }
    }
}

#[test]
fn a_newer_client_version_is_answered_and_refused() {
    let devices = Devices::new("newer-client", true);
    let (address, host) = host_once(devices.host.clone(), None);
    let mut socket = TcpStream::connect(address).unwrap();
    socket.write_all(b"WDKS\x00\x02\x00\x00").unwrap();
    let mut reply = [0; 8];
    socket.read_exact(&mut reply).unwrap();
    assert_eq!(&reply, b"WDKS\x00\x01\x00\x00");
    let HostResult::Failed(error) = host.join().unwrap() else {
        panic!("host must refuse");
    };
    assert!(error.contains("versión 2"), "{error}");
}

#[test]
fn malformed_handshake_frames_are_rejected() {
    let devices = Devices::new("handshake", true);
    for frame in [vec![0, 0], vec![0x08, 0x00], vec![0, 32, 1, 2, 3]] {
        let (address, host) = host_once(devices.host.clone(), None);
        let mut socket = TcpStream::connect(address).unwrap();
        let mut bytes = b"WDKS\x00\x01\x00\x00".to_vec();
        bytes.extend_from_slice(&frame);
        socket.write_all(&bytes).unwrap();
        if frame == [0, 32, 1, 2, 3] {
            // Truncated: the host waits for the rest until the peer closes.
            socket.shutdown(Shutdown::Write).unwrap();
        }
        assert!(matches!(host.join().unwrap(), HostResult::Failed(_)));
    }
}

/// Relays host-to-client bytes and injects a fault after the first
/// application record offset.
#[derive(Clone)]
enum Fault {
    Pause(usize, Duration),
    Flip(usize),
    CloseAt(usize),
    Inject(usize, Vec<u8>),
}

fn faulty_proxy(host: SocketAddr, fault: Fault) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (client, _) = listener.accept().unwrap();
        let upstream = TcpStream::connect(host).unwrap();
        let (mut from_client, mut to_host) =
            (client.try_clone().unwrap(), upstream.try_clone().unwrap());
        thread::spawn(move || {
            let _ = io::copy(&mut from_client, &mut to_host);
            let _ = to_host.shutdown(Shutdown::Write);
        });
        let (mut from_host, mut to_client) = (upstream, client);
        let mut offset = 0;
        let mut buffer = [0; 4096];
        let mut faulted = false;
        loop {
            let count = match from_host.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            let mut chunk = buffer[..count].to_vec();
            let start = offset;
            offset += count;
            let position = match &fault {
                Fault::Pause(at, _)
                | Fault::Flip(at)
                | Fault::CloseAt(at)
                | Fault::Inject(at, _) => *at,
            };
            if faulted || position >= offset {
                let _ = to_client.write_all(&chunk);
                continue;
            }
            faulted = true;
            let split = position - start;
            match &fault {
                Fault::Pause(_, duration) => {
                    let _ = to_client.write_all(&chunk[..split]);
                    thread::sleep(*duration);
                    let _ = to_client.write_all(&chunk[split..]);
                }
                Fault::Flip(_) => {
                    chunk[split] ^= 0x40;
                    let _ = to_client.write_all(&chunk);
                }
                Fault::CloseAt(_) => {
                    let _ = to_client.write_all(&chunk[..split]);
                    break;
                }
                Fault::Inject(_, bytes) => {
                    let _ = to_client.write_all(&chunk[..split]);
                    let _ = to_client.write_all(bytes);
                    break;
                }
            }
        }
        let _ = to_client.shutdown(Shutdown::Both);
    });
    address
}

/// Bytes the host sends before the first application record: preamble, the
/// second handshake message and the one-record Accepted control frame.
fn application_offset() -> usize {
    let name = HOST_NAME.len() + 1;
    let handshake = 8 + 2 + 32 + (32 + 16) + name + 16;
    handshake + 2 + 3 + 16
}

fn faulty_session(name: &str, fault: Fault) -> (Devices, SecureStream, SecureStream) {
    let devices = Devices::new(name, true);
    let (host_address, host) = host_once(devices.host.clone(), None);
    let address = faulty_proxy(host_address, fault);
    let Established::Session(client, _) = devices.connect(address, Intent::Session).unwrap() else {
        panic!("session expected");
    };
    let HostResult::Session(server, _) = host.join().unwrap() else {
        panic!("host session expected");
    };
    (devices, client, server)
}

fn probe() -> Message {
    Message::Ping { nonce: 0x0123_4567 }
}

#[test]
fn a_partial_record_survives_read_timeouts_and_peek() {
    let offset = application_offset();
    let (_devices, mut client, mut server) = faulty_session(
        "pause",
        Fault::Pause(offset + 5, Duration::from_millis(400)),
    );
    write_message(&mut server, &probe()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let error = read_message(&mut client).unwrap_err();
    assert!(error.to_string().contains("I/O"), "{error}");
    let mut first = [0; 4];
    assert!(client.peek(&mut first).is_err());
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    assert_eq!(client.peek(&mut first).unwrap(), 4);
    assert_eq!(read_message(&mut client).unwrap(), probe());
}

#[test]
fn tampered_records_are_rejected_permanently() {
    let offset = application_offset();
    let (_devices, mut client, mut server) = faulty_session("flip", Fault::Flip(offset + 9));
    write_message(&mut server, &probe()).unwrap();
    write_message(&mut server, &probe()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    for _ in 0..2 {
        let error = read_message(&mut client).unwrap_err();
        assert!(error.to_string().contains("cifrado"), "{error}");
    }
}

#[test]
fn truncated_short_and_reordered_records_are_rejected() {
    let offset = application_offset();
    let cases = [
        (
            "truncated",
            Fault::CloseAt(offset + 10),
            "mitad de un registro",
        ),
        (
            "short",
            Fault::Inject(offset, vec![0, 5, 1, 2, 3, 4, 5]),
            "demasiado corto",
        ),
        // A well-formed record from another position fails authentication.
        (
            "reordered",
            Fault::Inject(offset, vec![0, 16].into_iter().chain([0; 16]).collect()),
            "manipulado",
        ),
    ];
    for (name, fault, expected) in cases {
        let (_devices, mut client, mut server) = faulty_session(name, fault);
        write_message(&mut server, &probe()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let error = read_message(&mut client).unwrap_err();
        assert!(error.to_string().contains(expected), "{name}: {error}");
    }
}

#[test]
fn shutdown_handles_end_the_session_of_a_waiting_reader() {
    let devices = Devices::new("shutdown", true);
    let (address, host) = host_once(devices.host.clone(), None);
    let Established::Session(mut client, _) = devices.connect(address, Intent::Session).unwrap()
    else {
        panic!("session expected");
    };
    let HostResult::Session(mut server, _) = host.join().unwrap() else {
        panic!("host session expected");
    };
    // Like the host, the peer closes once it reads the end of the stream.
    let peer = thread::spawn(move || {
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut byte = [0];
        let ended = matches!(server.read(&mut byte), Ok(0));
        drop(server);
        ended
    });
    client.set_read_timeout(None).unwrap();
    let handle = client.shutdown_handle().unwrap();
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let _ = done.send(read_message(&mut client).is_err());
    });
    thread::sleep(Duration::from_millis(100));
    // Linux wakes the blocked reader at once; Windows only when the peer closes.
    handle.shutdown(Shutdown::Both).unwrap();
    assert!(
        peer.join().unwrap(),
        "the peer must see a clean end of stream"
    );
    assert!(finished.recv_timeout(Duration::from_secs(3)).unwrap());
}

/// Reproducible measurement of the record layer: `cargo test --release -p
/// windowdeck-transport throughput -- --ignored --nocapture`.
#[test]
#[ignore = "measurement; run explicitly in release"]
fn throughput() {
    let devices = Devices::new("throughput", true);
    let (address, host) = host_once(devices.host.clone(), None);
    let Established::Session(mut client, _) = devices.connect(address, Intent::Session).unwrap()
    else {
        panic!("session expected");
    };
    let HostResult::Session(mut server, _) = host.join().unwrap() else {
        panic!("host session expected");
    };
    let message = Message::VideoChunk {
        session_id: 1,
        frame_number: 0,
        captured_micros: 0,
        fragment_index: 0,
        fragment_count: 1,
        keyframe: true,
        payload: vec![0x42; 60_000],
    };
    let count = 4_000;
    let reader = thread::spawn(move || {
        for _ in 0..count {
            read_message(&mut client).unwrap();
        }
    });
    let started = Instant::now();
    for _ in 0..count {
        write_message(&mut server, &message).unwrap();
    }
    reader.join().unwrap();
    let elapsed = started.elapsed();
    let megabits = (count * 60_000 * 8) as f64 / 1_000_000.0;
    println!(
        "secure_throughput mbps={:.0} messages={count} elapsed_ms={}",
        megabits / elapsed.as_secs_f64(),
        elapsed.as_millis()
    );
}
