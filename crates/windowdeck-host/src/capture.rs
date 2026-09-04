use super::AnyError;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use windowdeck_diagnostics::{Level, emit};
use windowdeck_protocol::{MAX_VIDEO_PAYLOAD, Message, write_message};
use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};
use windows::core::Interface;
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
    VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

pub(super) const ENCODE_FPS: u32 = 30;
const ENCODE_BITRATE: u32 = 4_000_000;
const ENCODE_FRAMES: u64 = 60;

struct CaptureProbe;

impl GraphicsCaptureApiHandler for CaptureProbe {
    type Flags = (String, usize);
    type Error = AnyError;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        emit(
            Level::Info,
            "capture_started",
            &[
                ("monitor", &context.flags.1.to_string()),
                ("name", &context.flags.0),
            ],
        );
        Ok(Self)
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let _texture = frame.as_raw_texture();
        emit(
            Level::Info,
            "frame_captured",
            &[
                ("width", &frame.width().to_string()),
                ("height", &frame.height().to_string()),
                ("surface", "d3d11_texture"),
            ],
        );
        capture_control.stop();
        Ok(())
    }
}

pub fn run(index: usize) -> Result<(), AnyError> {
    let monitor = Monitor::from_index(index)?;
    let name = monitor.name()?;
    CaptureProbe::start(settings(
        monitor,
        MinimumUpdateIntervalSettings::Default,
        (name, index),
    ))?;
    Ok(())
}

struct EncodeFlags {
    name: String,
    index: usize,
    width: u32,
    height: u32,
    network: Option<(TcpStream, u64)>,
}

struct EncodeProbe {
    encoder: Option<VideoEncoder>,
    stream: InMemoryRandomAccessStream,
    started: Instant,
    frames: u64,
    width: u32,
    height: u32,
    network: Option<(TcpStream, u64)>,
}

impl GraphicsCaptureApiHandler for EncodeProbe {
    type Flags = EncodeFlags;
    type Error = AnyError;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = context.flags;
        let stream = InMemoryRandomAccessStream::new()?;
        let encoder = VideoEncoder::new_from_stream(
            VideoSettingsBuilder::new(flags.width, flags.height)
                .sub_type(VideoSettingsSubType::H264)
                .bitrate(ENCODE_BITRATE)
                .frame_rate(ENCODE_FPS),
            AudioSettingsBuilder::new().disabled(true),
            ContainerSettingsBuilder::new(),
            stream.cast()?,
        )?;
        emit(
            Level::Info,
            "h264_encode_started",
            &[
                ("monitor", &flags.index.to_string()),
                ("name", &flags.name),
                ("width", &flags.width.to_string()),
                ("height", &flags.height.to_string()),
                ("fps", &ENCODE_FPS.to_string()),
                ("bitrate", &ENCODE_BITRATE.to_string()),
            ],
        );
        Ok(Self {
            encoder: Some(encoder),
            stream,
            started: Instant::now(),
            frames: 0,
            width: flags.width,
            height: flags.height,
            network: flags.network,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        self.encoder
            .as_mut()
            .ok_or("codificador H.264 no disponible")?
            .send_frame(frame)?;
        self.frames += 1;
        if self.frames < ENCODE_FRAMES {
            return Ok(());
        }

        self.encoder
            .take()
            .ok_or("codificador H.264 no disponible")?
            .finish()?;
        let bytes = self.stream.Size()?;
        if bytes == 0 {
            return Err("el codificador H.264 no produjo datos".into());
        }
        if let Some((stream, session_id)) = &mut self.network {
            let encoded = read_stream(&self.stream, bytes)?;
            let fragments = send_h264(stream, *session_id, &encoded)?;
            emit(
                Level::Info,
                "h264_sent",
                &[
                    ("bytes", &bytes.to_string()),
                    ("fragments", &fragments.to_string()),
                ],
            );
        }
        emit(
            Level::Info,
            "h264_encoded",
            &[
                ("frames", &self.frames.to_string()),
                ("bytes", &bytes.to_string()),
                (
                    "duration_ms",
                    &self.started.elapsed().as_millis().to_string(),
                ),
                ("width", &self.width.to_string()),
                ("height", &self.height.to_string()),
            ],
        );
        capture_control.stop();
        Ok(())
    }
}

pub fn encode(index: usize) -> Result<(), AnyError> {
    let monitor = Monitor::from_index(index)?;
    let flags = EncodeFlags {
        name: monitor.name()?,
        index,
        width: monitor.width()?,
        height: monitor.height()?,
        network: None,
    };
    EncodeProbe::start(settings(
        monitor,
        MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(1.0 / f64::from(ENCODE_FPS))),
        flags,
    ))?;
    Ok(())
}

pub fn stream_h264(stream: TcpStream, index: usize, session_id: u64) -> Result<(), AnyError> {
    let monitor = Monitor::from_index(index)?;
    let flags = EncodeFlags {
        name: monitor.name()?,
        index,
        width: monitor.width()?,
        height: monitor.height()?,
        network: Some((stream, session_id)),
    };
    EncodeProbe::start(settings(
        monitor,
        MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(1.0 / f64::from(ENCODE_FPS))),
        flags,
    ))?;
    Ok(())
}

pub fn size(index: usize) -> Result<(u16, u16), AnyError> {
    let monitor = Monitor::from_index(index)?;
    Ok((monitor.width()?.try_into()?, monitor.height()?.try_into()?))
}

fn read_stream(stream: &InMemoryRandomAccessStream, size: u64) -> Result<Vec<u8>, AnyError> {
    let size = u32::try_from(size).map_err(|_| "salida H.264 demasiado grande")?;
    let input = stream.GetInputStreamAt(0)?;
    let reader = DataReader::CreateDataReader(&input)?;
    reader.LoadAsync(size)?.join()?;
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

fn send_h264(stream: &mut TcpStream, session_id: u64, bytes: &[u8]) -> Result<u16, AnyError> {
    let fragment_count = u16::try_from(bytes.len().div_ceil(MAX_VIDEO_PAYLOAD))
        .map_err(|_| "salida H.264 demasiado grande")?;
    for (fragment_index, payload) in bytes.chunks(MAX_VIDEO_PAYLOAD).enumerate() {
        write_message(
            &mut *stream,
            &Message::VideoChunk {
                session_id,
                frame_number: 0,
                captured_micros: 0,
                fragment_index: fragment_index.try_into()?,
                fragment_count,
                keyframe: true,
                payload: payload.to_vec(),
            },
        )?;
    }
    write_message(stream, &Message::Stop)?;
    Ok(fragment_count)
}

struct StreamFlags {
    stream: TcpStream,
    session_id: u64,
    name: String,
    index: usize,
}

struct CaptureStream {
    stream: TcpStream,
    session_id: u64,
    started: Instant,
    number: u64,
    scratch: Vec<u8>,
    pixels: Vec<u8>,
}

impl GraphicsCaptureApiHandler for CaptureStream {
    type Flags = StreamFlags;
    type Error = AnyError;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = context.flags;
        emit(
            Level::Info,
            "capture_stream_started",
            &[("monitor", &flags.index.to_string()), ("name", &flags.name)],
        );
        Ok(Self {
            stream: flags.stream,
            session_id: flags.session_id,
            started: Instant::now(),
            number: 0,
            scratch: Vec::new(),
            pixels: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let (source_width, source_height) = (frame.width() as usize, frame.height() as usize);
        let buffer = frame.buffer()?;
        let source = buffer.as_nopadding_buffer(&mut self.scratch);
        // ponytail: CPU scaling proves the path; replace it with GPU scaling plus H.264 in Hito 3.
        downscale_bgra(source, source_width, source_height, &mut self.pixels)?;
        write_message(
            &mut self.stream,
            &Message::Frame {
                session_id: self.session_id,
                number: self.number,
                captured_micros: self.started.elapsed().as_micros() as u64,
                width: super::WIDTH,
                height: super::HEIGHT,
                pixels: self.pixels.clone(),
            },
        )?;
        self.number += 1;
        if self.number.is_multiple_of(u64::from(super::FPS)) {
            emit(
                Level::Info,
                "capture_metrics",
                &[
                    ("frames_sent", &self.number.to_string()),
                    (
                        "fps",
                        &format!(
                            "{:.1}",
                            self.number as f64 / self.started.elapsed().as_secs_f64()
                        ),
                    ),
                ],
            );
        }
        Ok(())
    }
}

pub fn stream(stream: TcpStream, index: usize, session_id: u64) -> Result<(), AnyError> {
    let monitor = Monitor::from_index(index)?;
    let flags = StreamFlags {
        stream,
        session_id,
        name: monitor.name()?,
        index,
    };
    CaptureStream::start(settings(
        monitor,
        MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(1.0 / f64::from(super::FPS))),
        flags,
    ))?;
    Ok(())
}

fn settings<Flags>(
    monitor: Monitor,
    interval: MinimumUpdateIntervalSettings,
    flags: Flags,
) -> Settings<Flags, Monitor> {
    Settings::new(
        monitor,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        interval,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        flags,
    )
}

fn downscale_bgra(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    output: &mut Vec<u8>,
) -> Result<(), &'static str> {
    let expected = source_width
        .checked_mul(source_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("dimensiones de captura desbordadas")?;
    if source_width == 0 || source_height == 0 || source.len() != expected {
        return Err("buffer de captura inválido");
    }
    output.clear();
    output.reserve(usize::from(super::WIDTH) * usize::from(super::HEIGHT));
    for y in 0..usize::from(super::HEIGHT) {
        for x in 0..usize::from(super::WIDTH) {
            let source_x = x * source_width / usize::from(super::WIDTH);
            let source_y = y * source_height / usize::from(super::HEIGHT);
            let offset = (source_y * source_width + source_x) * 4;
            let (blue, green, red) = (source[offset], source[offset + 1], source[offset + 2]);
            output.push((red & 0xe0) | ((green & 0xe0) >> 3) | (blue >> 6));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_is_downscaled_to_rgb332() {
        let mut output = Vec::new();
        downscale_bgra(&[0, 0, 255, 255], 1, 1, &mut output).expect("valid pixel");
        assert_eq!(
            output.len(),
            usize::from(super::super::WIDTH) * usize::from(super::super::HEIGHT)
        );
        assert!(output.iter().all(|pixel| *pixel == 0xe0));
        assert!(downscale_bgra(&[], 1, 1, &mut output).is_err());
    }
}
