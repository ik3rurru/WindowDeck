//! Access-unit framing for the integrated codec. This is distinct from legacy
//! MPEG-TS chunks; a partial frame can only be discarded by ending its session.
use crate::{MAX_VIDEO_PAYLOAD, Message, ProtocolError};
pub const MAX_ACCESS_UNIT: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub struct AccessUnit {
    pub number: u64,
    pub captured_micros: u64,
    pub keyframe: bool,
    pub payload: Vec<u8>,
}

pub fn fragments(session_id: u64, unit: AccessUnit) -> Result<Vec<Message>, ProtocolError> {
    if unit.payload.is_empty() || unit.payload.len() > MAX_ACCESS_UNIT {
        return Err(ProtocolError::Invalid("invalid access unit size"));
    }
    let count = unit.payload.len().div_ceil(MAX_VIDEO_PAYLOAD) as u16;
    Ok(unit
        .payload
        .chunks(MAX_VIDEO_PAYLOAD)
        .enumerate()
        .map(|(i, payload)| Message::VideoChunk {
            session_id,
            frame_number: unit.number,
            captured_micros: unit.captured_micros,
            fragment_index: i as u16,
            fragment_count: count,
            keyframe: unit.keyframe,
            payload: payload.to_vec(),
        })
        .collect())
}

pub struct Assembler {
    session: u64,
    next_number: u64,
    next_fragment: u16,
    fragment_count: u16,
    unit: Option<AccessUnit>,
}

impl Assembler {
    pub fn new(session: u64) -> Self {
        Self {
            session,
            next_number: 0,
            next_fragment: 0,
            fragment_count: 0,
            unit: None,
        }
    }
    pub fn push(&mut self, message: Message) -> Result<Option<AccessUnit>, ProtocolError> {
        let Message::VideoChunk {
            session_id,
            frame_number,
            captured_micros,
            fragment_index,
            fragment_count,
            keyframe,
            payload,
        } = message
        else {
            return Err(ProtocolError::Invalid("expected an access unit fragment"));
        };
        if session_id != self.session
            || frame_number != self.next_number
            || fragment_index != self.next_fragment
            || fragment_count == 0
            || fragment_count as usize > MAX_ACCESS_UNIT.div_ceil(MAX_VIDEO_PAYLOAD)
            || fragment_index >= fragment_count
            || payload.is_empty()
            || payload.len() > MAX_VIDEO_PAYLOAD
            || (self.next_number == 0 && !keyframe)
        {
            return Err(ProtocolError::Invalid(
                "invalid access unit order or session",
            ));
        }
        let unit = self.unit.get_or_insert_with(|| {
            self.fragment_count = fragment_count;
            AccessUnit {
                number: frame_number,
                captured_micros,
                keyframe,
                payload: Vec::new(),
            }
        });
        if self.fragment_count != fragment_count
            || unit.captured_micros != captured_micros
            || unit.keyframe != keyframe
            || unit.payload.len() + payload.len() > MAX_ACCESS_UNIT
        {
            return Err(ProtocolError::Invalid("inconsistent access unit fragments"));
        }
        unit.payload.extend_from_slice(&payload);
        self.next_fragment += 1;
        if self.next_fragment != fragment_count {
            return Ok(None);
        }
        self.next_number = self
            .next_number
            .checked_add(1)
            .ok_or(ProtocolError::Invalid("frame counter overflow"))?;
        self.next_fragment = 0;
        Ok(self.unit.take())
    }
    pub fn is_partial(&self) -> bool {
        self.unit.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn unit() -> AccessUnit {
        AccessUnit {
            number: 0,
            captured_micros: 123,
            keyframe: true,
            payload: vec![7; MAX_VIDEO_PAYLOAD + 10],
        }
    }
    #[test]
    fn reconstructs_complete_units_and_preserves_capture_time() {
        let packets = fragments(42, unit()).unwrap();
        let mut assembler = Assembler::new(42);
        assert!(assembler.push(packets[0].clone()).unwrap().is_none());
        assert!(assembler.is_partial());
        let result = assembler.push(packets[1].clone()).unwrap().unwrap();
        assert_eq!(result.payload, unit().payload);
        assert_eq!(result.captured_micros, 123);
        assert!(!assembler.is_partial());
        assert!(assembler.push(packets[1].clone()).is_err());
    }
    #[test]
    fn rejects_lost_fragments_cross_session_and_changed_metadata() {
        let packets = fragments(42, unit()).unwrap();
        assert!(Assembler::new(42).push(packets[1].clone()).is_err());
        assert!(Assembler::new(43).push(packets[0].clone()).is_err());
        let mut assembler = Assembler::new(42);
        assembler.push(packets[0].clone()).unwrap();
        let mut changed = packets[1].clone();
        if let Message::VideoChunk {
            captured_micros, ..
        } = &mut changed
        {
            *captured_micros += 1;
        }
        assert!(assembler.push(changed).is_err());
        let mut missing_keyframe = unit();
        missing_keyframe.keyframe = false;
        assert!(
            Assembler::new(42)
                .push(fragments(42, missing_keyframe).unwrap().remove(0))
                .is_err()
        );
    }
}
