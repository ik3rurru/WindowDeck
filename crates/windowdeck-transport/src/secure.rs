//! Authenticated and encrypted TCP channel (Noise XX) for the WindowDeck protocol.
//!
//! Wire format after TCP connect:
//! 1. Client preamble `WDKS` + version (u16) + reserved (u16), then each Noise
//!    handshake message prefixed by its length (u16). The host answers with its
//!    own preamble so a version mismatch is reported instead of guessed.
//! 2. After `-> e`, `<- e, ee, s, es` and `-> s, se`, both ends know each
//!    other's static key. Every following byte travels in records: length (u16)
//!    and ChaCha20-Poly1305 ciphertext with an implicit, increasing nonce.
//! 3. The host's first record is a control frame that accepts the session,
//!    starts pairing or rejects the device. Only then does the unchanged
//!    WindowDeck protocol (Hello, Capabilities...) start inside the channel.
use crate::SecureError;
use crate::trust::{Identity, PublicKey, sanitize_name};
use snow::{Builder, HandshakeState, StatelessTransportState};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use windowdeck_protocol::connection::TimedStream;
use windowdeck_protocol::{MAX_MESSAGE_SIZE, Message, write_message};

pub const MAGIC: [u8; 4] = *b"WDKS";
pub const VERSION: u16 = 1;
const PREAMBLE_LEN: usize = 8;
const PARAMS: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const PROLOGUE: &[u8] = b"WindowDeck secure transport 1";
const TAG: usize = 16;
/// Largest ciphertext the Noise specification allows in one message.
pub const MAX_RECORD: usize = u16::MAX as usize;
pub const MAX_RECORD_PLAINTEXT: usize = MAX_RECORD - TAG;
/// Bounds the ciphertext prepared by a single `write` call.
const MAX_WRITE: usize = 1 << 20;
const MAX_HANDSHAKE: usize = 1024;
const MAX_CONTROL: usize = 64;
const MAX_NAME_BYTES: usize = 255;
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
pub const TRANSCRIPT_LEN: usize = 32;

fn preamble() -> [u8; PREAMBLE_LEN] {
    let [high, low] = VERSION.to_be_bytes();
    [MAGIC[0], MAGIC[1], MAGIC[2], MAGIC[3], high, low, 0, 0]
}

/// Why the client connects. Pairing never starts from a session connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Session,
    Pair,
}

impl Intent {
    fn byte(self) -> u8 {
        match self {
            Self::Session => 1,
            Self::Pair => 2,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Session),
            2 => Some(Self::Pair),
            _ => None,
        }
    }
}

/// The authenticated key of the other end and the name it announced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub key: PublicKey,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    NotPaired,
    PairingUnavailable,
    TooManyAttempts,
    Busy,
    Other(u8),
}

impl Reject {
    fn code(self) -> u8 {
        match self {
            Self::NotPaired => 1,
            Self::PairingUnavailable => 2,
            Self::TooManyAttempts => 3,
            Self::Busy => 4,
            Self::Other(code) => code,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::NotPaired,
            2 => Self::PairingUnavailable,
            3 => Self::TooManyAttempts,
            4 => Self::Busy,
            other => Self::Other(other),
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::NotPaired => "el PC no reconoce este dispositivo; hay que emparejarlo",
            Self::PairingUnavailable => {
                "el PC no puede mostrar la solicitud de emparejamiento; inicia WindowDeck desde su panel"
            }
            Self::TooManyAttempts => {
                "el PC bloqueó el emparejamiento tras varios intentos; pulsa Detener e Iniciar en Windows"
            }
            Self::Busy => "el PC está atendiendo otra solicitud de emparejamiento",
            Self::Other(_) => "el PC rechazó la conexión",
        }
    }
}

/// Frames exchanged before the WindowDeck protocol starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Accepted,
    PairingRequired,
    Rejected(Reject),
    PairCommit([u8; 32]),
    PairNonce([u8; 32]),
    PairReveal([u8; 32]),
    PairVerdict(bool),
}

impl Control {
    fn encode(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(33);
        match self {
            Self::Accepted => body.push(1),
            Self::PairingRequired => body.push(2),
            Self::Rejected(reason) => body.extend_from_slice(&[3, reason.code()]),
            Self::PairCommit(value) => {
                body.push(10);
                body.extend_from_slice(value);
            }
            Self::PairNonce(value) => {
                body.push(11);
                body.extend_from_slice(value);
            }
            Self::PairReveal(value) => {
                body.push(12);
                body.extend_from_slice(value);
            }
            Self::PairVerdict(accepted) => body.extend_from_slice(&[13, u8::from(*accepted)]),
        }
        let mut frame = (body.len() as u16).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        frame
    }

    fn decode(body: &[u8]) -> Result<Self, SecureError> {
        let invalid = SecureError::Protocol("mensaje de control inválido");
        let value = |bytes: &[u8]| -> Result<[u8; 32], SecureError> {
            bytes
                .try_into()
                .map_err(|_| SecureError::Protocol("mensaje de control inválido"))
        };
        match body {
            [1] => Ok(Self::Accepted),
            [2] => Ok(Self::PairingRequired),
            [3, reason] => Ok(Self::Rejected(Reject::from_code(*reason))),
            [10, rest @ ..] => Ok(Self::PairCommit(value(rest)?)),
            [11, rest @ ..] => Ok(Self::PairNonce(value(rest)?)),
            [12, rest @ ..] => Ok(Self::PairReveal(value(rest)?)),
            [13, 0] => Ok(Self::PairVerdict(false)),
            [13, 1] => Ok(Self::PairVerdict(true)),
            _ => Err(invalid),
        }
    }
}

/// A connection whose handshake completed on the host. The caller decides,
/// from its trust store, which control frame to send first.
pub struct Accepted {
    pub stream: SecureStream,
    pub peer: Peer,
    pub intent: Intent,
    /// Whether the client already trusts this host's key.
    pub knows_host: bool,
    pub transcript: [u8; TRANSCRIPT_LEN],
}

pub enum Incoming {
    Secure(Box<Accepted>),
    /// A client predating the encrypted transport. It is never served.
    Legacy(LegacyClient),
}

pub struct LegacyClient {
    socket: TcpStream,
    prefix: [u8; 4],
}

impl LegacyClient {
    /// Answers in the old unencrypted format so the client can log a reason, then
    /// closes. Reads its pending Hello first: closing with unread data would reset
    /// the connection and discard the reply.
    pub fn reject(mut self, code: u16, message: &str) -> io::Result<()> {
        let announced = u32::from_be_bytes(self.prefix) as usize;
        if announced <= MAX_MESSAGE_SIZE {
            let mut hello = vec![0; announced];
            let _ = self.socket.read_exact(&mut hello);
        }
        write_message(
            &mut self.socket,
            &Message::Error {
                code,
                message: message.into(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?;
        let _ = self.socket.shutdown(Shutdown::Write);
        let deadline = Instant::now() + Duration::from_secs(1);
        self.socket
            .set_read_timeout(Some(Duration::from_millis(200)))?;
        let mut sink = [0; 512];
        while Instant::now() < deadline && matches!(self.socket.read(&mut sink), Ok(1..)) {}
        Ok(())
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.socket.peer_addr()
    }
}

/// Host side: reads the client preamble and completes the Noise handshake.
pub fn accept(
    mut socket: TcpStream,
    identity: &Identity,
    name: &str,
) -> Result<Incoming, SecureError> {
    socket.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    socket.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    let mut magic = [0; 4];
    socket.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Ok(Incoming::Legacy(LegacyClient {
            socket,
            prefix: magic,
        }));
    }
    let mut version = [0; 4];
    socket.read_exact(&mut version)?;
    // Answer before checking, so either end can explain a version mismatch.
    socket.write_all(&preamble())?;
    let version = u16::from_be_bytes([version[0], version[1]]);
    if version != VERSION {
        return Err(SecureError::Incompatible(version));
    }
    let mut noise = handshake(identity, false)?;
    let mut payload = [0; MAX_HANDSHAKE];
    if noise.read_message(&read_frame(&mut socket)?, &mut payload)? != 0 {
        return Err(SecureError::Protocol("datos inesperados en la negociación"));
    }
    let mut message = [0; MAX_HANDSHAKE];
    let length = noise.write_message(&encode_name(name), &mut message)?;
    write_frame(&mut socket, &message[..length])?;
    let length = noise.read_message(&read_frame(&mut socket)?, &mut payload)?;
    let (intent, knows_host, peer_name) = match &payload[..length] {
        [intent, knows @ (0 | 1), name @ ..] => (
            Intent::from_byte(*intent).ok_or(SecureError::Protocol("intención desconocida"))?,
            *knows == 1,
            decode_name(name)?,
        ),
        _ => return Err(SecureError::Protocol("presentación del cliente inválida")),
    };
    let peer = Peer {
        key: remote_key(&noise)?,
        name: peer_name,
    };
    let transcript = transcript(&noise)?;
    let stream = SecureStream::new(socket, noise.into_stateless_transport_mode()?);
    Ok(Incoming::Secure(Box::new(Accepted {
        stream,
        peer,
        intent,
        knows_host,
        transcript,
    })))
}

/// Client side result.
pub enum Established {
    /// The host accepted a session; the WindowDeck protocol starts now.
    Session(SecureStream, Peer),
    /// Both ends already trust each other; nothing to pair.
    AlreadyPaired(Peer),
    /// The host started a pairing ceremony on this connection.
    PairingRequired(Box<Pairing>),
}

pub struct Pairing {
    pub stream: SecureStream,
    pub peer: Peer,
    pub transcript: [u8; TRANSCRIPT_LEN],
}

/// Client side: completes the handshake and reads the host decision. A session
/// is only requested from a host whose key `is_paired` already accepts.
pub fn connect(
    mut socket: TcpStream,
    identity: &Identity,
    name: &str,
    intent: Intent,
    is_paired: impl FnOnce(&PublicKey) -> io::Result<bool>,
) -> Result<Established, SecureError> {
    socket.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    socket.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    let mut noise = handshake(identity, true)?;
    let mut message = [0; MAX_HANDSHAKE];
    let length = noise.write_message(&[], &mut message)?;
    let mut first = preamble().to_vec();
    first.extend_from_slice(&(length as u16).to_be_bytes());
    first.extend_from_slice(&message[..length]);
    socket.write_all(&first)?;
    let mut reply = [0; PREAMBLE_LEN];
    if let Err(error) = socket.read_exact(&mut reply) {
        return Err(match error.kind() {
            io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted => io::Error::new(
                error.kind(),
                "el PC cerró la conexión al iniciar el cifrado; si usa una versión anterior de WindowDeck, actualízala",
            )
            .into(),
            _ => error.into(),
        });
    }
    if reply[..4] != MAGIC {
        return Err(SecureError::Protocol(
            "el PC no respondió con el cifrado de WindowDeck",
        ));
    }
    let version = u16::from_be_bytes([reply[4], reply[5]]);
    if version != VERSION {
        return Err(SecureError::Incompatible(version));
    }
    let mut payload = [0; MAX_HANDSHAKE];
    let length = noise.read_message(&read_frame(&mut socket)?, &mut payload)?;
    let peer = Peer {
        key: remote_key(&noise)?,
        name: decode_name(&payload[..length])?,
    };
    let known = is_paired(&peer.key)?;
    if intent == Intent::Session && !known {
        // Never authenticate to, or receive video from, an unknown host.
        let _ = socket.shutdown(Shutdown::Both);
        return Err(SecureError::HostNotPaired(peer));
    }
    let mut info = vec![intent.byte(), u8::from(known)];
    info.extend_from_slice(&encode_name(name));
    let length = noise.write_message(&info, &mut message)?;
    write_frame(&mut socket, &message[..length])?;
    let transcript = transcript(&noise)?;
    let mut stream = SecureStream::new(socket, noise.into_stateless_transport_mode()?);
    match (stream.read_control()?, intent) {
        (Control::Accepted, Intent::Session) => Ok(Established::Session(stream, peer)),
        (Control::Accepted, Intent::Pair) if known => Ok(Established::AlreadyPaired(peer)),
        (Control::PairingRequired, Intent::Pair) => {
            Ok(Established::PairingRequired(Box::new(Pairing {
                stream,
                peer,
                transcript,
            })))
        }
        (Control::Rejected(reason), _) => Err(SecureError::Rejected(reason)),
        _ => Err(SecureError::Protocol("respuesta inesperada del PC")),
    }
}

fn handshake(identity: &Identity, initiator: bool) -> Result<HandshakeState, SecureError> {
    let params = PARAMS
        .parse()
        .map_err(|_| SecureError::Protocol("parámetros Noise no admitidos"))?;
    let builder = Builder::new(params)
        .local_private_key(identity.private())?
        .prologue(PROLOGUE)?;
    Ok(if initiator {
        builder.build_initiator()?
    } else {
        builder.build_responder()?
    })
}

fn remote_key(noise: &HandshakeState) -> Result<PublicKey, SecureError> {
    noise
        .get_remote_static()
        .and_then(PublicKey::from_slice)
        .ok_or(SecureError::Protocol("falta la clave del otro equipo"))
}

fn transcript(noise: &HandshakeState) -> Result<[u8; TRANSCRIPT_LEN], SecureError> {
    noise
        .get_handshake_hash()
        .try_into()
        .map_err(|_| SecureError::Protocol("resumen de negociación inesperado"))
}

fn encode_name(name: &str) -> Vec<u8> {
    let name = sanitize_name(name);
    let mut end = name.len().min(MAX_NAME_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    let mut bytes = vec![end as u8];
    bytes.extend_from_slice(&name.as_bytes()[..end]);
    bytes
}

fn decode_name(bytes: &[u8]) -> Result<String, SecureError> {
    match bytes {
        [length, name @ ..] if usize::from(*length) == name.len() => std::str::from_utf8(name)
            .map(sanitize_name)
            .map_err(|_| SecureError::Protocol("nombre inválido")),
        _ => Err(SecureError::Protocol("nombre inválido")),
    }
}

fn write_frame(socket: &mut TcpStream, frame: &[u8]) -> io::Result<()> {
    let mut bytes = (frame.len() as u16).to_be_bytes().to_vec();
    bytes.extend_from_slice(frame);
    socket.write_all(&bytes)
}

fn read_frame(socket: &mut TcpStream) -> Result<Vec<u8>, SecureError> {
    let mut length = [0; 2];
    socket.read_exact(&mut length)?;
    let length = usize::from(u16::from_be_bytes(length));
    if length == 0 || length > MAX_HANDSHAKE {
        return Err(SecureError::Protocol("mensaje de negociación inválido"));
    }
    let mut frame = vec![0; length];
    socket.read_exact(&mut frame)?;
    Ok(frame)
}

/// Encrypted byte stream with the socket API the host and client already use.
/// Clones share the cipher state; each direction keeps its own nonce, so one
/// thread may read while another writes. A partially received record survives
/// read timeouts. Any authentication failure or interrupted record write makes
/// that direction fail permanently: the stream can no longer be trusted.
pub struct SecureStream {
    socket: TcpStream,
    shared: Arc<Shared>,
}

struct Shared {
    cipher: StatelessTransportState,
    sender: Mutex<Sender>,
    receiver: Mutex<Receiver>,
}

#[derive(Default)]
struct Sender {
    nonce: u64,
    failed: bool,
    buffer: Vec<u8>,
}

#[derive(Default)]
struct Receiver {
    nonce: u64,
    failed: bool,
    record: Vec<u8>,
    filled: usize,
    plain: Vec<u8>,
    offset: usize,
}

impl Receiver {
    /// Reads one complete record and decrypts it. Returns `false` on a clean EOF
    /// between records. I/O errors, including timeouts, keep the partial record.
    fn next_record(
        &mut self,
        socket: &TcpStream,
        cipher: &StatelessTransportState,
    ) -> io::Result<bool> {
        if self.record.len() < 2 + MAX_RECORD {
            self.record.resize(2 + MAX_RECORD, 0);
        }
        loop {
            let needed = if self.filled < 2 {
                2
            } else {
                2 + usize::from(u16::from_be_bytes([self.record[0], self.record[1]]))
            };
            if self.filled >= 2 && needed < 2 + TAG {
                self.failed = true;
                return Err(invalid("registro cifrado demasiado corto"));
            }
            if self.filled == needed && needed > 2 {
                self.plain.resize(needed - 2 - TAG, 0);
                let opened = cipher
                    .read_message(self.nonce, &self.record[2..needed], &mut self.plain)
                    .map_err(|_| {
                        self.failed = true;
                        invalid("registro cifrado manipulado o fuera de orden")
                    })?;
                self.plain.truncate(opened);
                self.offset = 0;
                self.filled = 0;
                self.nonce = self.nonce.checked_add(1).ok_or_else(|| {
                    self.failed = true;
                    invalid("contador del canal cifrado agotado")
                })?;
                return Ok(true);
            }
            let count = (&*socket).read(&mut self.record[self.filled..needed])?;
            if count == 0 {
                return if self.filled == 0 {
                    Ok(false)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "conexión cerrada a mitad de un registro cifrado",
                    ))
                };
            }
            self.filled += count;
        }
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn lock<T>(mutex: &Mutex<T>) -> io::Result<MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| io::Error::other("estado del canal cifrado bloqueado"))
}

impl SecureStream {
    fn new(socket: TcpStream, cipher: StatelessTransportState) -> Self {
        Self {
            socket,
            shared: Arc::new(Shared {
                cipher,
                sender: Mutex::default(),
                receiver: Mutex::default(),
            }),
        }
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            socket: self.socket.try_clone()?,
            shared: Arc::clone(&self.shared),
        })
    }

    /// A handle that can only close the connection, for threads that must
    /// interrupt a blocked reader without touching the encrypted stream.
    pub fn shutdown_handle(&self) -> io::Result<ShutdownHandle> {
        Ok(ShutdownHandle(self.socket.try_clone()?))
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.socket.shutdown(how)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.socket.set_nodelay(nodelay)
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.socket.set_read_timeout(timeout)
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.socket.set_write_timeout(timeout)
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        self.socket.read_timeout()
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        self.socket.write_timeout()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.socket.peer_addr()
    }

    /// Like `TcpStream::peek`, but over decrypted bytes: it may wait (up to the
    /// read timeout) for a whole record, and leaves the bytes to the next read.
    pub fn peek(&self, output: &mut [u8]) -> io::Result<usize> {
        self.read_plain(output, false)
    }

    pub fn send_control(&mut self, control: &Control) -> Result<(), SecureError> {
        // One write call seals the whole frame in a single record.
        self.write_all(&control.encode())?;
        Ok(())
    }

    pub fn read_control(&mut self) -> Result<Control, SecureError> {
        let mut length = [0; 2];
        self.read_exact(&mut length)?;
        let length = usize::from(u16::from_be_bytes(length));
        if length == 0 || length > MAX_CONTROL {
            return Err(SecureError::Protocol("mensaje de control inválido"));
        }
        let mut body = vec![0; length];
        self.read_exact(&mut body)?;
        Control::decode(&body)
    }

    fn read_plain(&self, output: &mut [u8], consume: bool) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let mut receiver = lock(&self.shared.receiver)?;
        if receiver.failed {
            return Err(invalid("el canal cifrado ya no es fiable"));
        }
        while receiver.offset == receiver.plain.len() {
            if !receiver.next_record(&self.socket, &self.shared.cipher)? {
                return Ok(0);
            }
        }
        let available = &receiver.plain[receiver.offset..];
        let count = available.len().min(output.len());
        output[..count].copy_from_slice(&available[..count]);
        if consume {
            receiver.offset += count;
        }
        Ok(count)
    }

    fn write_sealed(&self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        let data = &data[..data.len().min(MAX_WRITE)];
        let mut sender = lock(&self.shared.sender)?;
        if sender.failed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "el canal cifrado se interrumpió durante un envío",
            ));
        }
        let Sender {
            nonce,
            failed,
            buffer,
        } = &mut *sender;
        buffer.clear();
        for chunk in data.chunks(MAX_RECORD_PLAINTEXT) {
            let start = buffer.len();
            buffer.resize(start + 2 + chunk.len() + TAG, 0);
            let sealed = self
                .shared
                .cipher
                .write_message(*nonce, chunk, &mut buffer[start + 2..])
                .map_err(|error| io::Error::other(error.to_string()))?;
            buffer[start..start + 2].copy_from_slice(&(sealed as u16).to_be_bytes());
            *nonce = nonce
                .checked_add(1)
                .ok_or_else(|| io::Error::other("contador del canal cifrado agotado"))?;
        }
        if let Err(error) = (&self.socket).write_all(buffer) {
            // Part of a record may be on the wire: later records would not decrypt.
            *failed = true;
            return Err(error);
        }
        Ok(data.len())
    }
}

impl std::fmt::Debug for SecureStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureStream")
            .field("peer", &self.socket.peer_addr().ok())
            .finish_non_exhaustive()
    }
}

impl Read for SecureStream {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.read_plain(output, true)
    }
}

impl Read for &SecureStream {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.read_plain(output, true)
    }
}

impl Write for SecureStream {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.write_sealed(data)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Write for &SecureStream {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.write_sealed(data)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl TimedStream for SecureStream {
    fn read_timeout(&self) -> io::Result<Option<Duration>> {
        SecureStream::read_timeout(self)
    }
    fn write_timeout(&self) -> io::Result<Option<Duration>> {
        SecureStream::write_timeout(self)
    }
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        SecureStream::set_read_timeout(self, timeout)
    }
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        SecureStream::set_write_timeout(self, timeout)
    }
}

/// Closes a connection from another thread; cannot read or write.
#[derive(Debug)]
pub struct ShutdownHandle(TcpStream);

impl ShutdownHandle {
    /// For sockets that are still being negotiated.
    pub fn new(socket: &TcpStream) -> io::Result<Self> {
        Ok(Self(socket.try_clone()?))
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(self.0.try_clone()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_frames_round_trip_and_reject_malformed_bodies() {
        for control in [
            Control::Accepted,
            Control::PairingRequired,
            Control::Rejected(Reject::NotPaired),
            Control::Rejected(Reject::Other(77)),
            Control::PairCommit([1; 32]),
            Control::PairNonce([2; 32]),
            Control::PairReveal([3; 32]),
            Control::PairVerdict(true),
            Control::PairVerdict(false),
        ] {
            let frame = control.encode();
            assert_eq!(
                usize::from(u16::from_be_bytes([frame[0], frame[1]])),
                frame.len() - 2
            );
            assert!(frame.len() - 2 <= MAX_CONTROL);
            assert_eq!(Control::decode(&frame[2..]).unwrap(), control);
        }
        for body in [
            &[][..],
            &[0],
            &[1, 0],
            &[3],
            &[10; 32],
            &[11; 34],
            &[13, 2],
            &[13],
            &[99],
        ] {
            assert!(Control::decode(body).is_err(), "{body:?}");
        }
    }

    #[test]
    fn names_are_sanitized_and_length_checked() {
        let encoded = encode_name("Mi\nPC");
        assert_eq!(decode_name(&encoded).unwrap(), "Mi PC");
        assert_eq!(
            decode_name(&encode_name(&"é".repeat(300)))
                .unwrap()
                .chars()
                .count(),
            40
        );
        assert!(decode_name(&[]).is_err());
        assert!(decode_name(&[3, b'a']).is_err());
        assert!(decode_name(&[1, 0xff]).is_err());
        assert_eq!(decode_name(&[0]).unwrap(), "Dispositivo sin nombre");
    }
}
