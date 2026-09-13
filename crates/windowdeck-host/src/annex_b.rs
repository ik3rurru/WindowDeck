//! Frame the CPU encoder's Annex B output without buffering a playback timeline.
//! FFmpeg must insert an AUD before every access unit and repeat SPS/PPS on IDRs.
use std::io::{self, Read, Write};
use std::time::Instant;
use windowdeck_diagnostics::{Level, Timings, emit};
use windowdeck_protocol::video::{AccessUnit, MAX_ACCESS_UNIT, fragments};
use windowdeck_protocol::{MAX_VIDEO_PAYLOAD, Message, queue, write_message};

pub struct Units<R> {
    input: R,
    pending: Vec<u8>,
    scan: usize,
    started: bool,
    picture: bool,
    keyframe: bool,
    sps: bool,
    pps: bool,
    first: bool,
    eof: bool,
}

impl<R: Read> Units<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            pending: Vec::new(),
            scan: 0,
            started: false,
            picture: false,
            keyframe: false,
            sps: false,
            pps: false,
            first: true,
            eof: false,
        }
    }

    pub fn next_unit(&mut self) -> io::Result<Option<(Vec<u8>, bool)>> {
        loop {
            while self.scan + 3 < self.pending.len() {
                let rest = &self.pending[self.scan..];
                let prefix = if rest.starts_with(&[0, 0, 0, 1]) {
                    4
                } else if rest.starts_with(&[0, 0, 1]) {
                    3
                } else {
                    self.scan += 1;
                    continue;
                };
                if self.scan + prefix == self.pending.len() {
                    break; // NAL header arrives in the next pipe read.
                }
                let header = self.pending[self.scan + prefix];
                if header & 0x80 != 0 || header & 0x1f == 0 {
                    return Err(invalid("invalid Annex B NAL header"));
                }
                let kind = header & 0x1f;
                if kind == 9 {
                    if self.started {
                        return self.take_unit(self.scan).map(Some);
                    }
                    if self.pending[..self.scan].iter().any(|&byte| byte != 0) {
                        return Err(invalid("data before the first access unit delimiter"));
                    }
                    self.started = true;
                } else if !self.started {
                    return Err(invalid("CPU H.264 output has no access unit delimiter"));
                }
                self.picture |= matches!(kind, 1..=5);
                self.keyframe |= kind == 5;
                self.sps |= kind == 7;
                self.pps |= kind == 8;
                self.scan += prefix + 1;
            }
            if self.pending.len() > MAX_ACCESS_UNIT {
                return Err(invalid("CPU H.264 access unit exceeds the size limit"));
            }
            if self.eof {
                if self.pending.is_empty() {
                    return Ok(None);
                }
                if self.pending.ends_with(&[0, 0, 1]) {
                    return Err(invalid("truncated Annex B NAL header"));
                }
                return self.take_unit(self.pending.len()).map(Some);
            }
            let mut bytes = [0; MAX_VIDEO_PAYLOAD];
            let count = match self.input.read(&mut bytes) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            self.eof = count == 0;
            self.pending.extend_from_slice(&bytes[..count]);
        }
    }

    fn take_unit(&mut self, end: usize) -> io::Result<(Vec<u8>, bool)> {
        if !self.started || !self.picture || end == 0 || end > MAX_ACCESS_UNIT {
            return Err(invalid("empty, incomplete or oversized CPU access unit"));
        }
        if self.first && !(self.keyframe && self.sps && self.pps) {
            return Err(invalid("CPU stream must start with SPS, PPS and an IDR"));
        }
        let remaining = self.pending.split_off(end);
        let payload = std::mem::replace(&mut self.pending, remaining);
        let keyframe = self.keyframe;
        self.scan = 0;
        self.started = false;
        self.picture = false;
        self.keyframe = false;
        self.sps = false;
        self.pps = false;
        self.first = false;
        Ok((payload, keyframe))
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn send(
    input: impl Read,
    mut output: impl Write,
    session: u64,
    stopping: impl Fn() -> bool,
) -> io::Result<(u64, u64)> {
    let mut units = Units::new(input);
    let mut number = 0;
    let mut bytes = 0;
    let mut timings = Timings::default();
    let started = Instant::now();
    let mut report = Instant::now();
    loop {
        if stopping() {
            break;
        }
        let next = units.next_unit();
        // The watchdog can kill FFmpeg halfway through its final pipe write.
        // Stop ends the session without passing that incomplete unit to a decoder.
        if stopping() {
            break;
        }
        let Some((payload, keyframe)) = next? else {
            break;
        };
        let sent = Instant::now();
        bytes += payload.len() as u64;
        // The BGRA pipe does not carry the driver's QPC timestamp through FFmpeg.
        // Zero means unavailable, never a fabricated capture time.
        let unit = AccessUnit {
            number,
            captured_micros: 0,
            keyframe,
            payload,
        };
        for message in fragments(session, unit).map_err(io::Error::other)? {
            if sent.elapsed() >= queue::STALL_TIMEOUT {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "CPU encoded frame expired during send",
                ));
            }
            write_message(&mut output, &message).map_err(io::Error::other)?;
        }
        timings.record(sent.elapsed());
        if number == 0 {
            super::publish_state("streaming");
            emit(
                Level::Info,
                "h264_first_access_unit_sent",
                &[("elapsed_ms", &started.elapsed().as_millis().to_string())],
            );
        }
        number = number
            .checked_add(1)
            .ok_or_else(|| invalid("CPU access unit counter overflow"))?;
        if report.elapsed().as_secs() >= 1 {
            timings.report("cpu_access_unit_send_metrics");
            report = Instant::now();
        }
    }
    write_message(&mut output, &Message::Stop).map_err(io::Error::other)?;
    Ok((bytes, number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windowdeck_protocol::{read_message, video::Assembler};

    fn keyframe() -> Vec<u8> {
        vec![
            0, 0, 0, 1, 9, 0xf0, 0, 0, 1, 0x67, 0x80, 0, 0, 0, 1, 0x68, 0x80, 0, 0, 1, 0x65, 0x80,
        ]
    }

    #[test]
    fn a_brief_stall_mid_frame_preserves_all_fragments_and_the_session() {
        struct PausedWriter {
            bytes: Vec<u8>,
            paused: bool,
        }
        impl Write for PausedWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.paused {
                    self.paused = true;
                    std::thread::sleep(std::time::Duration::from_millis(350));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut encoded = keyframe();
        encoded.extend(vec![0x55; MAX_VIDEO_PAYLOAD * 3]);
        let mut output = PausedWriter {
            bytes: Vec::new(),
            paused: false,
        };
        assert_eq!(
            send(encoded.as_slice(), &mut output, 91, || false).unwrap(),
            (encoded.len() as u64, 1)
        );
        let mut wire = output.bytes.as_slice();
        let mut assembler = Assembler::new(91);
        let unit = loop {
            if let Some(unit) = assembler.push(read_message(&mut wire).unwrap()).unwrap() {
                break unit;
            }
        };
        assert_eq!(unit.payload, encoded);
        assert_eq!(unit.number, 0);
        assert_eq!(read_message(&mut wire).unwrap(), Message::Stop);
        assert!(wire.is_empty());
    }

    #[test]
    fn preserves_access_units_with_start_codes_split_at_every_boundary() {
        struct Chunks<'a> {
            bytes: &'a [u8],
            maximum: usize,
        }
        impl Read for Chunks<'_> {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                let count = output.len().min(self.maximum).min(self.bytes.len());
                output[..count].copy_from_slice(&self.bytes[..count]);
                self.bytes = &self.bytes[count..];
                Ok(count)
            }
        }
        let first = keyframe();
        let second = [0, 0, 1, 9, 0xf0, 0, 0, 0, 1, 0x61, 0x80, 0, 0, 3, 1];
        let input = [first.as_slice(), &second].concat();
        for maximum in 1..=input.len() {
            let mut units = Units::new(Chunks {
                bytes: &input,
                maximum,
            });
            assert_eq!(units.next_unit().unwrap(), Some((first.clone(), true)));
            assert_eq!(units.next_unit().unwrap(), Some((second.to_vec(), false)));
            assert_eq!(units.next_unit().unwrap(), None);
        }
    }

    #[test]
    fn rejects_missing_headers_empty_units_truncation_and_excessive_size() {
        for bytes in [
            vec![0, 0, 1, 0x65, 0x80],
            vec![0, 0, 1, 9, 0xf0],
            vec![0, 0, 1, 9, 0xf0, 0, 0, 1, 0x61, 0x80],
            vec![0, 0, 1, 9, 0xf0, 0, 0, 1, 0xe5, 0x80],
            [keyframe(), vec![0, 0, 1]].concat(),
            [keyframe(), vec![0; MAX_ACCESS_UNIT]].concat(),
        ] {
            assert_eq!(
                Units::new(bytes.as_slice()).next_unit().unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn native_receiver_reconstructs_every_fragment_and_stop() {
        let mut first = keyframe();
        first.extend(vec![0x80; MAX_VIDEO_PAYLOAD + 5]);
        let second = [0, 0, 1, 9, 0xf0, 0, 0, 1, 0x61, 0x80];
        let input = [first.as_slice(), &second].concat();
        let mut wire = Vec::new();
        assert_eq!(
            send(input.as_slice(), &mut wire, 42, || false).unwrap(),
            (input.len() as u64, 2)
        );
        let mut wire = wire.as_slice();
        let mut assembler = Assembler::new(42);
        let mut output = Vec::new();
        loop {
            let message = read_message(&mut wire).unwrap();
            if message == Message::Stop {
                break;
            }
            if let Some(unit) = assembler.push(message).unwrap() {
                assert_eq!(unit.number, output.len() as u64);
                assert_eq!(unit.captured_micros, 0);
                output.push(unit.payload);
            }
        }
        assert_eq!(output, [first, second.to_vec()]);
        assert!(!assembler.is_partial());
        assert!(wire.is_empty());
    }

    #[test]
    fn stop_during_the_last_pipe_write_never_sends_an_incomplete_unit() {
        use std::cell::Cell;
        struct StopAtEof<'a> {
            input: &'a [u8],
            stopped: &'a Cell<bool>,
        }
        impl Read for StopAtEof<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                let count = self.input.read(bytes)?;
                if count == 0 {
                    self.stopped.set(true);
                }
                Ok(count)
            }
        }
        let first = keyframe();
        let input = [first.as_slice(), &[0, 0, 1, 9, 0xf0, 0, 0, 1]].concat();
        let stopped = Cell::new(false);
        let mut wire = Vec::new();
        let reader = StopAtEof {
            input: &input,
            stopped: &stopped,
        };
        assert_eq!(
            send(reader, &mut wire, 42, || stopped.get()).unwrap(),
            (first.len() as u64, 1)
        );
        let mut wire = wire.as_slice();
        let unit = Assembler::new(42)
            .push(read_message(&mut wire).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(unit.payload, first);
        assert_eq!(read_message(&mut wire).unwrap(), Message::Stop);
        assert!(wire.is_empty());
    }

    #[test]
    #[ignore = "requires FFmpeg with libx264 and h264_metadata in PATH"]
    fn cpu_encoder_output_decodes_after_fragmented_transport() {
        use std::process::{Command, Stdio};
        fn ffmpeg() -> Command {
            let mut command = Command::new("ffmpeg");
            command.args(["-hide_banner", "-loglevel", "error"]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            command
        }
        let encoded = ffmpeg()
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x800:rate=60",
                "-frames:v",
                "120",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-tune",
                "zerolatency",
                "-x264-params",
                "sync-lookahead=0:rc-lookahead=0:ref=1:scenecut=0:repeat-headers=1",
                "-bf",
                "0",
                "-g",
                "60",
                "-b:v",
                "16000000",
                "-pix_fmt",
                "yuv420p",
                "-bsf:v",
                "h264_metadata=aud=insert",
                "-f",
                "h264",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let mut wire = Vec::new();
        assert_eq!(
            send(encoded.stdout.as_slice(), &mut wire, 51, || false).unwrap(),
            (encoded.stdout.len() as u64, 120)
        );
        let mut wire = wire.as_slice();
        let mut assembler = Assembler::new(51);
        let mut decoded_input = Vec::new();
        let mut frames = 0;
        loop {
            let message = read_message(&mut wire).unwrap();
            if message == Message::Stop {
                break;
            }
            if let Some(unit) = assembler.push(message).unwrap() {
                assert_eq!(unit.keyframe, frames % 60 == 0);
                decoded_input.extend(unit.payload);
                frames += 1;
            }
        }
        assert_eq!(frames, 120);
        assert_eq!(decoded_input, encoded.stdout);
        assert!(wire.is_empty());
        assert!(!assembler.is_partial());
        let mut decoder = ffmpeg()
            .args([
                "-xerror", "-f", "h264", "-i", "pipe:0", "-f", "framemd5", "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = decoder.stdin.take().unwrap();
        let writer = std::thread::spawn(move || input.write_all(&decoded_input));
        let decoded = decoder.wait_with_output().unwrap();
        writer.join().unwrap().unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        let checksums = String::from_utf8(decoded.stdout).unwrap();
        let frames: Vec<_> = checksums
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect();
        assert_eq!(frames.len(), 120);
        let unique: std::collections::HashSet<_> = frames
            .iter()
            .map(|line| line.rsplit(',').next().unwrap())
            .collect();
        assert!(
            unique.len() > 100,
            "decoded moving pixels must not freeze or repeat"
        );
    }
}
