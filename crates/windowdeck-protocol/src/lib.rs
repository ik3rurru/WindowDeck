use std::fmt;
use std::io::{self, Cursor, Read, Write};

pub const PROTOCOL_VERSION: u16 = 3;
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024;
const VIDEO_CHUNK_OVERHEAD: usize = 32;
pub const MAX_VIDEO_PAYLOAD: usize = MAX_MESSAGE_SIZE - VIDEO_CHUNK_OVERHEAD;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VideoCodec {
    Rgb332 = 1,
    H264 = 2,
}

impl VideoCodec {
    pub const fn capability(self) -> u8 {
        1_u8 << (self as u8 - 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Hello {
        app_version: String,
    },
    Capabilities {
        max_width: u16,
        max_height: u16,
        max_fps: u16,
        codecs: u8,
    },
    SessionConfig {
        session_id: u64,
        width: u16,
        height: u16,
        fps: u16,
        codec: VideoCodec,
    },
    Start,
    Stop,
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    Error {
        code: u16,
        message: String,
    },
    Frame {
        session_id: u64,
        number: u64,
        captured_micros: u64,
        width: u16,
        height: u16,
        pixels: Vec<u8>,
    },
    VideoChunk {
        session_id: u64,
        frame_number: u64,
        captured_micros: u64,
        fragment_index: u16,
        fragment_count: u16,
        keyframe: bool,
        payload: Vec<u8>,
    },
}

#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    Oversized(usize),
    UnsupportedVersion(u16),
    Invalid(&'static str),
    InvalidUtf8,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O: {error}"),
            Self::Oversized(size) => write!(f, "message size {size} exceeds {MAX_MESSAGE_SIZE}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "protocol version {version} is not supported")
            }
            Self::Invalid(reason) => write!(f, "invalid message: {reason}"),
            Self::InvalidUtf8 => write!(f, "invalid UTF-8 string"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<io::Error> for ProtocolError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn write_message(mut writer: impl Write, message: &Message) -> Result<(), ProtocolError> {
    let payload = encode(message)?;
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    Ok(())
}

pub fn read_message(mut reader: impl Read) -> Result<Message, ProtocolError> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::Oversized(length));
    }
    if length < 3 {
        return Err(ProtocolError::Invalid("header is truncated"));
    }

    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    decode(&payload)
}

fn encode(message: &Message) -> Result<Vec<u8>, ProtocolError> {
    let mut output = Vec::new();
    output.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    match message {
        Message::Hello { app_version } => {
            output.push(1);
            put_string(&mut output, app_version)?;
        }
        Message::Capabilities {
            max_width,
            max_height,
            max_fps,
            codecs,
        } => {
            output.push(2);
            put_u16(&mut output, *max_width);
            put_u16(&mut output, *max_height);
            put_u16(&mut output, *max_fps);
            output.push(*codecs);
        }
        Message::SessionConfig {
            session_id,
            width,
            height,
            fps,
            codec,
        } => {
            output.push(3);
            put_u64(&mut output, *session_id);
            put_u16(&mut output, *width);
            put_u16(&mut output, *height);
            put_u16(&mut output, *fps);
            output.push(*codec as u8);
        }
        Message::Start => output.push(4),
        Message::Stop => output.push(5),
        Message::Ping { nonce } => {
            output.push(6);
            put_u64(&mut output, *nonce);
        }
        Message::Pong { nonce } => {
            output.push(7);
            put_u64(&mut output, *nonce);
        }
        Message::Error { code, message } => {
            output.push(8);
            put_u16(&mut output, *code);
            put_string(&mut output, message)?;
        }
        Message::Frame {
            session_id,
            number,
            captured_micros,
            width,
            height,
            pixels,
        } => {
            output.push(9);
            let expected = usize::from(*width) * usize::from(*height);
            if pixels.len() != expected {
                return Err(ProtocolError::Invalid(
                    "pixel count does not match dimensions",
                ));
            }
            put_u64(&mut output, *session_id);
            put_u64(&mut output, *number);
            put_u64(&mut output, *captured_micros);
            put_u16(&mut output, *width);
            put_u16(&mut output, *height);
            output.extend_from_slice(pixels);
        }
        Message::VideoChunk {
            session_id,
            frame_number,
            captured_micros,
            fragment_index,
            fragment_count,
            keyframe,
            payload,
        } => {
            validate_video_chunk(*fragment_index, *fragment_count, payload.len())?;
            output.push(10);
            put_u64(&mut output, *session_id);
            put_u64(&mut output, *frame_number);
            put_u64(&mut output, *captured_micros);
            put_u16(&mut output, *fragment_index);
            put_u16(&mut output, *fragment_count);
            output.push(u8::from(*keyframe));
            output.extend_from_slice(payload);
        }
    }
    if output.len() > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::Oversized(output.len()));
    }
    Ok(output)
}

fn decode(payload: &[u8]) -> Result<Message, ProtocolError> {
    let mut input = Cursor::new(payload);
    let version = take_u16(&mut input)?;
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(version));
    }
    let kind = take_u8(&mut input)?;
    let message = match kind {
        1 => Message::Hello {
            app_version: take_string(&mut input)?,
        },
        2 => Message::Capabilities {
            max_width: take_u16(&mut input)?,
            max_height: take_u16(&mut input)?,
            max_fps: take_u16(&mut input)?,
            codecs: take_u8(&mut input)?,
        },
        3 => Message::SessionConfig {
            session_id: take_u64(&mut input)?,
            width: take_u16(&mut input)?,
            height: take_u16(&mut input)?,
            fps: take_u16(&mut input)?,
            codec: take_codec(&mut input)?,
        },
        4 => Message::Start,
        5 => Message::Stop,
        6 => Message::Ping {
            nonce: take_u64(&mut input)?,
        },
        7 => Message::Pong {
            nonce: take_u64(&mut input)?,
        },
        8 => Message::Error {
            code: take_u16(&mut input)?,
            message: take_string(&mut input)?,
        },
        9 => {
            let session_id = take_u64(&mut input)?;
            let number = take_u64(&mut input)?;
            let captured_micros = take_u64(&mut input)?;
            let width = take_u16(&mut input)?;
            let height = take_u16(&mut input)?;
            let pixel_count = usize::from(width)
                .checked_mul(usize::from(height))
                .ok_or(ProtocolError::Invalid("frame dimensions overflow"))?;
            if pixel_count > MAX_MESSAGE_SIZE {
                return Err(ProtocolError::Oversized(pixel_count));
            }
            let mut pixels = vec![0; pixel_count];
            input.read_exact(&mut pixels)?;
            Message::Frame {
                session_id,
                number,
                captured_micros,
                width,
                height,
                pixels,
            }
        }
        10 => {
            let session_id = take_u64(&mut input)?;
            let frame_number = take_u64(&mut input)?;
            let captured_micros = take_u64(&mut input)?;
            let fragment_index = take_u16(&mut input)?;
            let fragment_count = take_u16(&mut input)?;
            let keyframe = match take_u8(&mut input)? {
                0 => false,
                1 => true,
                _ => return Err(ProtocolError::Invalid("invalid keyframe flag")),
            };
            let mut payload = Vec::new();
            input.read_to_end(&mut payload)?;
            validate_video_chunk(fragment_index, fragment_count, payload.len())?;
            Message::VideoChunk {
                session_id,
                frame_number,
                captured_micros,
                fragment_index,
                fragment_count,
                keyframe,
                payload,
            }
        }
        _ => return Err(ProtocolError::Invalid("unknown message kind")),
    };
    if input.position() != payload.len() as u64 {
        return Err(ProtocolError::Invalid("trailing bytes"));
    }
    Ok(message)
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_string(output: &mut Vec<u8>, value: &str) -> Result<(), ProtocolError> {
    let length = u16::try_from(value.len()).map_err(|_| ProtocolError::Oversized(value.len()))?;
    put_u16(output, length);
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn take_u8(input: &mut Cursor<&[u8]>) -> Result<u8, ProtocolError> {
    let mut bytes = [0; 1];
    input.read_exact(&mut bytes)?;
    Ok(bytes[0])
}

fn take_u16(input: &mut Cursor<&[u8]>) -> Result<u16, ProtocolError> {
    let mut bytes = [0; 2];
    input.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

fn take_codec(input: &mut Cursor<&[u8]>) -> Result<VideoCodec, ProtocolError> {
    match take_u8(input)? {
        1 => Ok(VideoCodec::Rgb332),
        2 => Ok(VideoCodec::H264),
        _ => Err(ProtocolError::Invalid("unknown video codec")),
    }
}

fn take_u64(input: &mut Cursor<&[u8]>) -> Result<u64, ProtocolError> {
    let mut bytes = [0; 8];
    input.read_exact(&mut bytes)?;
    Ok(u64::from_be_bytes(bytes))
}

fn take_string(input: &mut Cursor<&[u8]>) -> Result<String, ProtocolError> {
    let length = usize::from(take_u16(input)?);
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| ProtocolError::InvalidUtf8)
}

fn validate_video_chunk(
    fragment_index: u16,
    fragment_count: u16,
    payload_len: usize,
) -> Result<(), ProtocolError> {
    if fragment_count == 0 || fragment_index >= fragment_count {
        return Err(ProtocolError::Invalid("invalid video fragment"));
    }
    if payload_len == 0 {
        return Err(ProtocolError::Invalid("empty video payload"));
    }
    if payload_len > MAX_VIDEO_PAYLOAD {
        return Err(ProtocolError::Oversized(payload_len + VIDEO_CHUNK_OVERHEAD));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    AwaitingHello,
    Negotiating,
    Ready,
    Streaming,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    HelloReceived,
    Negotiated,
    Started,
    Stopped,
}

impl ConnectionState {
    pub fn apply(self, event: ConnectionEvent) -> Result<Self, ProtocolError> {
        match (self, event) {
            (Self::AwaitingHello, ConnectionEvent::HelloReceived) => Ok(Self::Negotiating),
            (Self::Negotiating, ConnectionEvent::Negotiated) => Ok(Self::Ready),
            (Self::Ready, ConnectionEvent::Started) => Ok(Self::Streaming),
            (Self::Ready | Self::Streaming, ConnectionEvent::Stopped) => Ok(Self::Closed),
            _ => Err(ProtocolError::Invalid("unexpected connection event")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_messages_round_trip() {
        let messages = [
            Message::Hello {
                app_version: "0.1.0".into(),
            },
            Message::Capabilities {
                max_width: 1280,
                max_height: 800,
                max_fps: 60,
                codecs: VideoCodec::Rgb332.capability() | VideoCodec::H264.capability(),
            },
            Message::SessionConfig {
                session_id: 42,
                width: 32,
                height: 12,
                fps: 10,
                codec: VideoCodec::Rgb332,
            },
            Message::Start,
            Message::Stop,
            Message::Ping { nonce: 7 },
            Message::Pong { nonce: 7 },
            Message::Error {
                code: 1,
                message: "test".into(),
            },
            Message::Frame {
                session_id: 42,
                number: 3,
                captured_micros: 9,
                width: 2,
                height: 2,
                pixels: vec![0, 1, 2, 3],
            },
            Message::VideoChunk {
                session_id: 42,
                frame_number: 4,
                captured_micros: 10,
                fragment_index: 1,
                fragment_count: 2,
                keyframe: true,
                payload: vec![0, 0, 0, 1, 0x65],
            },
        ];
        for message in messages {
            let mut bytes = Vec::new();
            write_message(&mut bytes, &message).expect("test message must encode");
            assert_eq!(
                read_message(bytes.as_slice()).expect("message must decode"),
                message
            );
        }
    }

    #[test]
    fn rejects_truncated_oversized_and_incompatible_messages() {
        assert!(read_message([0, 0, 0, 3, 0].as_slice()).is_err());

        let oversized = (MAX_MESSAGE_SIZE as u32 + 1).to_be_bytes();
        assert!(matches!(
            read_message(oversized.as_slice()),
            Err(ProtocolError::Oversized(_))
        ));

        let incompatible = [0, 0, 0, 3, 0, 4, 4];
        assert!(matches!(
            read_message(incompatible.as_slice()),
            Err(ProtocolError::UnsupportedVersion(4))
        ));

        assert!(
            write_message(
                Vec::new(),
                &Message::VideoChunk {
                    session_id: 1,
                    frame_number: 1,
                    captured_micros: 1,
                    fragment_index: 1,
                    fragment_count: 1,
                    keyframe: false,
                    payload: vec![1],
                }
            )
            .is_err()
        );
    }

    #[test]
    fn connection_state_rejects_out_of_order_events() {
        let state = ConnectionState::AwaitingHello
            .apply(ConnectionEvent::HelloReceived)
            .and_then(|state| state.apply(ConnectionEvent::Negotiated))
            .and_then(|state| state.apply(ConnectionEvent::Started))
            .expect("valid session flow");
        assert_eq!(state, ConnectionState::Streaming);
        assert!(
            ConnectionState::AwaitingHello
                .apply(ConnectionEvent::Started)
                .is_err()
        );
    }
}
