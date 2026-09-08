use super::AnyError;
#[path = "driver_frames.rs"]
mod driver_frames;
use std::io::{self, BufRead, BufReader, Read};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use windowdeck_diagnostics::{Level, emit};
use windowdeck_protocol::{MAX_VIDEO_PAYLOAD, Message, write_message};
use windows::Storage::Streams::InMemoryRandomAccessStream;
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

pub(super) const ENCODE_FPS: u32 = 60;
const ENCODE_BITRATE: u32 = 16_000_000;
const ENCODE_FRAMES: u64 = 60;

pub(super) fn probe_driver_frames(gpu: bool) -> Result<(), AnyError> {
    driver_frames::run(display_tool()?, gpu)
}

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
}

struct EncodeProbe {
    encoder: Option<VideoEncoder>,
    stream: InMemoryRandomAccessStream,
    started: Instant,
    frames: u64,
    width: u32,
    height: u32,
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
    };
    EncodeProbe::start(settings(
        monitor,
        MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(1.0 / f64::from(ENCODE_FPS))),
        flags,
    ))?;
    Ok(())
}

pub fn stream_h264(stream: TcpStream, index: usize, session_id: u64) -> Result<(), AnyError> {
    let input = format!(
        "ddagrab=output_idx={}:framerate={ENCODE_FPS}:draw_mouse=1",
        index - 1
    );
    stream_h264_input(stream, session_id, input, index.to_string(), None, None)
}

fn display_tool() -> Result<PathBuf, AnyError> {
    if let Some(path) = std::env::var_os("WINDOWDECK_DISPLAY_EXE") {
        return Ok(path.into());
    }
    let exe = std::env::current_exe()?;
    let directory = exe.parent().ok_or("no se encuentra la carpeta del host")?;
    for path in [
        directory.join("windowdeck-display.exe"),
        directory.join("../windows-idd/windowdeck-display.exe"),
    ] {
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("falta windowdeck-display.exe; compila driver/windows-idd/build.ps1 o configura WINDOWDECK_DISPLAY_EXE".into())
}

pub(super) struct DisplayLease {
    child: std::process::Child,
}

impl DisplayLease {
    pub(super) fn acquire() -> Result<Self, AnyError> {
        Self::start(Command::new(display_tool()?).arg("--lease"))
    }

    fn start(command: &mut Command) -> Result<Self, AnyError> {
        let mut lease = Self {
            child: command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?,
        };
        let output = lease
            .child
            .stdout
            .take()
            .ok_or("falta la salida de --lease")?;
        let (ready, response) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(output).read_line(&mut line).map(|_| line);
            let _ = ready.send(result);
        });
        let result = response.recv_timeout(Duration::from_secs(9));
        if !matches!(&result, Ok(Ok(line)) if line.trim() == "READY") {
            let _ = lease.child.kill();
            let _ = lease.child.wait();
            let _ = reader.join();
            return Err("no se pudo activar el monitor automático; inicia windowdeck-display --broker como administrador y consulta su registro".into());
        }
        reader.join().map_err(|_| "falló el lector de --lease")?;
        emit(Level::Info, "virtual_display_acquired", &[]);
        Ok(lease)
    }
}

impl Drop for DisplayLease {
    fn drop(&mut self) {
        // EOF also occurs when the host exits unexpectedly. The native helper releases
        // the broker connection and waits for PnP removal before exiting.
        drop(self.child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(7);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        emit(Level::Info, "virtual_display_released", &[]);
    }
}

fn virtual_device_name() -> Result<String, AnyError> {
    let output = Command::new(display_tool()?).arg("--source").output()?;
    if !output.status.success() {
        return Err(format!("monitor WindowDeck no disponible: {}; abre windowdeck-display --run como administrador",
            String::from_utf8_lossy(&output.stderr).trim()).into());
    }
    parse_device_name(&output.stdout)
}

fn parse_device_name(bytes: &[u8]) -> Result<String, AnyError> {
    let name = std::str::from_utf8(bytes)?.trim();
    let suffix = name
        .strip_prefix(r"\\.\DISPLAY")
        .ok_or("respuesta --source inválida; recompila la utilidad WindowDeck")?;
    if suffix.is_empty()
        || !suffix.bytes().all(|byte| byte.is_ascii_digit())
        || suffix.parse::<u32>()? == 0
    {
        return Err("nombre de dispositivo WindowDeck inválido".into());
    }
    Ok(name.to_owned())
}

pub fn virtual_monitor() -> Result<Monitor, AnyError> {
    // Resolve the driver's device path through DisplayConfig, never an assumed monitor index or friendly name.
    let name = virtual_device_name()?;
    for monitor in Monitor::enumerate()? {
        if monitor.device_name()? == name {
            if monitor.width()? != u32::from(super::H264_WIDTH)
                || monitor.height()? != u32::from(super::H264_HEIGHT)
            {
                return Err("el modo de WindowDeck cambió durante la consulta".into());
            }
            return Ok(monitor);
        }
    }
    Err("el monitor WindowDeck desapareció durante la consulta".into())
}

pub fn stream_virtual_h264(stream: TcpStream, session_id: u64) -> Result<(), AnyError> {
    let monitor = virtual_monitor()?;
    let name = monitor.device_name()?;
    let handle = monitor.as_raw_hmonitor() as usize;
    let input = format!(
        "gfxcapture=hmonitor={handle}:max_framerate={ENCODE_FPS}:capture_cursor=1:output_fmt=bgra"
    );
    emit(
        Level::Info,
        "virtual_capture_selected",
        &[
            ("device", &name),
            ("name", &monitor.name()?),
            ("capture", "windows_graphics_capture"),
        ],
    );
    stream_h264_input(stream, session_id, input, name.clone(), Some(name), None)
}

struct CpuFrameSource(std::process::Child);

impl Drop for CpuFrameSource {
    fn drop(&mut self) {
        // Closing this private pipe also handles unexpected host termination.
        drop(self.0.stdin.take());
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn stream_driver_h264(stream: TcpStream, session_id: u64) -> Result<(), AnyError> {
    let mut source = CpuFrameSource(
        Command::new(display_tool()?)
            .arg("--cpu-frame-stream")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(9);
    let name = loop {
        if let Some(status) = source.0.try_wait()? {
            return Err(format!(
                "el auxiliar CPU terminó con {status}; inicia --frame-broker como administrador"
            )
            .into());
        }
        if let Ok(name) = virtual_device_name() {
            break name;
        }
        if Instant::now() >= deadline {
            return Err("el monitor CPU no se activó a tiempo".into());
        }
        thread::sleep(Duration::from_millis(100));
    };
    let input = source.0.stdout.take().ok_or("falta la salida CPU")?;
    emit(
        Level::Info,
        "driver_capture_selected",
        &[("capture", "driver_cpu"), ("device", &name)],
    );
    // Source outlives the encoder: drop FFmpeg before releasing the virtual display.
    stream_h264_input(
        stream,
        session_id,
        String::new(),
        name.clone(),
        Some(name),
        Some(input),
    )
}

// Own the encoder so all error paths reap it. The virtual route also watches removal,
// client closure and stalled output; a capture must never fall back to the physical screen.
struct EncoderProcess {
    child: Arc<Mutex<std::process::Child>>,
    cancel: mpsc::Sender<()>,
    watcher: Option<thread::JoinHandle<()>>,
}

impl Drop for EncoderProcess {
    fn drop(&mut self) {
        let _ = self.cancel.send(());
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

struct EncoderOutput {
    pipe: std::process::ChildStdout,
    last_output: Arc<Mutex<Instant>>,
}

impl Read for EncoderOutput {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.pipe.read(buffer)?;
        if count != 0 {
            *self
                .last_output
                .lock()
                .map_err(|_| io::Error::other("encoder progress lock poisoned"))? = Instant::now();
        }
        Ok(count)
    }
}

fn stream_h264_input(
    stream: TcpStream,
    session_id: u64,
    input: String,
    monitor: String,
    virtual_device: Option<String>,
    raw_input: Option<std::process::ChildStdout>,
) -> Result<(), AnyError> {
    let encoder_name =
        std::env::var("WINDOWDECK_H264_ENCODER").unwrap_or_else(|_| "libx264".into());
    let encoder_name = match encoder_name.as_str() {
        "libx264" | "h264_amf" | "h264_nvenc" | "h264_mf" => encoder_name,
        _ => {
            return Err(
                "WINDOWDECK_H264_ENCODER inválido; usa libx264, h264_amf, h264_nvenc o h264_mf"
                    .into(),
            );
        }
    };
    let peer = stream.try_clone()?;
    // Only the watchdog reads after negotiation; the existing write timeout is preserved.
    peer.set_read_timeout(Some(Duration::from_millis(200)))?;
    let filter = format!(
        "hwdownload,format=bgra,scale={}:{}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
        super::H264_WIDTH,
        super::H264_HEIGHT,
        super::H264_WIDTH,
        super::H264_HEIGHT,
    );
    let filter = if virtual_device.is_some() {
        format!("{filter},fps={ENCODE_FPS}")
    } else {
        filter
    };
    let mut command = Command::new("ffmpeg");
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostats",
        "-stats_period",
        "1",
        "-progress",
        "pipe:2",
        "-nostdin",
    ]);
    if let Some(raw_input) = raw_input {
        command
            .args([
                "-f",
                "rawvideo",
                "-pixel_format",
                "bgra",
                "-video_size",
                "1280x800",
                "-framerate",
                &ENCODE_FPS.to_string(),
                "-i",
                "pipe:0",
            ])
            .stdin(Stdio::from(raw_input));
    } else {
        command.args(["-f", "lavfi", "-i", &input, "-vf", &filter]);
    }
    command.args(["-an", "-c:v", &encoder_name]);
    match encoder_name.as_str() {
        "libx264" => {
            command.args(["-preset", "ultrafast", "-tune", "zerolatency"]);
        }
        "h264_amf" => {
            command.args(["-usage", "ultralowlatency", "-quality", "speed"]);
        }
        "h264_nvenc" => {
            command.args(["-preset", "p1", "-tune", "ull"]);
        }
        "h264_mf" => {}
        _ => unreachable!(),
    }
    let output_format = if encoder_name == "libx264" || encoder_name == "h264_mf" {
        "yuv420p"
    } else {
        "nv12"
    };
    let mut ffmpeg = command
        .args([
            "-bf",
            "0",
            "-g",
            &ENCODE_FPS.to_string(),
            "-b:v",
            &ENCODE_BITRATE.to_string(),
            "-pix_fmt",
            output_format,
            "-f",
            "mpegts",
            "-mpegts_flags",
            "+resend_headers",
            "-muxdelay",
            "0",
            "-muxpreload",
            "0",
            "-flush_packets",
            "1",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("no se pudo iniciar ffmpeg; instala FFmpeg: {error}"),
            )
        })?;
    let last_output = Arc::new(Mutex::new(Instant::now()));
    let mut output = EncoderOutput {
        pipe: ffmpeg.stdout.take().ok_or("ffmpeg no abrió su salida")?,
        last_output: Arc::clone(&last_output),
    };
    let diagnostics = ffmpeg.stderr.take().ok_or("ffmpeg no abrió sus métricas")?;
    let (cancel, cancelled) = mpsc::channel();
    let child = Arc::new(Mutex::new(ffmpeg));
    let mut encoder = EncoderProcess {
        child: Arc::clone(&child),
        cancel,
        watcher: None,
    };
    if let Some(expected) = virtual_device {
        encoder.watcher = Some(thread::spawn(move || {
            while matches!(
                cancelled.recv_timeout(Duration::from_millis(500)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                let mut byte = [0];
                let disconnected = match peer.peek(&mut byte) {
                    Ok(0) => true,
                    Ok(_) => false,
                    Err(error) => !matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ),
                };
                let active = virtual_device_name().is_ok_and(|name| name == expected);
                let stalled = last_output
                    .lock()
                    .map_or(true, |last| last.elapsed() > Duration::from_secs(10));
                if disconnected || !active || stalled {
                    emit(
                        Level::Info,
                        "virtual_capture_stopped",
                        &[(
                            "reason",
                            if disconnected {
                                "client_disconnected"
                            } else if !active {
                                "monitor_removed_or_changed"
                            } else {
                                "encoder_stalled"
                            },
                        )],
                    );
                    if let Ok(mut child) = child.lock() {
                        let _ = child.kill();
                    }
                    break;
                }
            }
        }));
    }
    let diagnostics = thread::spawn(move || log_ffmpeg_progress(diagnostics));
    emit(
        Level::Info,
        "h264_stream_started",
        &[
            ("monitor", &monitor),
            ("width", &super::H264_WIDTH.to_string()),
            ("height", &super::H264_HEIGHT.to_string()),
            ("fps", &ENCODE_FPS.to_string()),
            ("encoder", &encoder_name),
        ],
    );

    let result = send_h264_stream(&mut output, stream, session_id);
    let status = {
        let mut child = encoder
            .child
            .lock()
            .map_err(|_| "encoder process lock poisoned")?;
        let _ = child.kill();
        child.wait()?
    };
    diagnostics
        .join()
        .map_err(|_| "el lector de métricas de ffmpeg terminó inesperadamente")??;
    let (bytes, chunks) = result?;
    if !status.success() {
        return Err(format!("ffmpeg terminó con {status}").into());
    }
    emit(
        Level::Info,
        "h264_stream_stopped",
        &[
            ("bytes", &bytes.to_string()),
            ("chunks", &chunks.to_string()),
        ],
    );
    Ok(())
}

pub fn size(index: usize) -> Result<(u16, u16), AnyError> {
    let monitor = Monitor::from_index(index)?;
    Ok((monitor.width()?.try_into()?, monitor.height()?.try_into()?))
}

fn send_h264_stream(
    mut input: impl Read,
    mut stream: impl io::Write,
    session_id: u64,
) -> Result<(u64, u64), AnyError> {
    let started = Instant::now();
    let mut buffer = vec![0; MAX_VIDEO_PAYLOAD];
    let mut bytes = 0_u64;
    let mut chunk = 0_u64;
    let mut last_report = Instant::now();
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        // ponytail: this probe sequences transport chunks; parse H.264 access units when recovery needs exact keyframes.
        write_message(
            &mut stream,
            &Message::VideoChunk {
                session_id,
                frame_number: chunk,
                captured_micros: started.elapsed().as_micros() as u64,
                fragment_index: 0,
                fragment_count: 1,
                keyframe: chunk == 0,
                payload: buffer[..read].to_vec(),
            },
        )?;
        bytes += read as u64;
        chunk += 1;
        if chunk == 1 {
            emit(
                Level::Info,
                "h264_first_packet_sent",
                &[("elapsed_ms", &started.elapsed().as_millis().to_string())],
            );
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            emit_stream_metrics("h264_send_metrics", started, bytes, chunk);
            last_report = Instant::now();
        }
    }
    write_message(&mut stream, &Message::Stop)?;
    Ok((bytes, chunk))
}

fn emit_stream_metrics(event: &str, started: Instant, bytes: u64, chunks: u64) {
    let elapsed = started.elapsed();
    let mbps = bytes as f64 * 8.0 / elapsed.as_secs_f64().max(f64::EPSILON) / 1_000_000.0;
    emit(
        Level::Info,
        event,
        &[
            ("elapsed_ms", &elapsed.as_millis().to_string()),
            ("bytes", &bytes.to_string()),
            ("chunks", &chunks.to_string()),
            ("mbps", &format!("{mbps:.2}")),
        ],
    );
}

fn log_ffmpeg_progress(input: impl Read) -> io::Result<[String; 4]> {
    let mut metrics = ["0", "0", "N/A", "0x"].map(str::to_owned);
    for line in BufReader::new(input).lines() {
        let line = line?;
        let Some((key, value)) = line.split_once('=') else {
            if !line.is_empty() {
                emit(Level::Warn, "ffmpeg_output", &[("message", &line)]);
            }
            continue;
        };
        match key {
            "frame" => metrics[0] = value.into(),
            "fps" => metrics[1] = value.into(),
            "bitrate" => metrics[2] = value.into(),
            "speed" => metrics[3] = value.into(),
            "progress" => emit(
                Level::Info,
                "h264_encoder_metrics",
                &[
                    ("frames", &metrics[0]),
                    ("fps", &metrics[1]),
                    ("bitrate", &metrics[2]),
                    ("speed", &metrics[3]),
                ],
            ),
            _ => {}
        }
    }
    Ok(metrics)
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
    use windowdeck_protocol::read_message;

    #[test]
    fn virtual_source_rejects_unexpected_helper_output() {
        assert_eq!(
            parse_device_name(b"\\\\.\\DISPLAY12\r\n").expect("GDI source"),
            r"\\.\DISPLAY12"
        );
        for invalid in [
            "",
            "WindowDeck",
            r"\\.\DISPLAY",
            r"\\.\DISPLAY0",
            r"\\.\DISPLAY-1",
            r"\\.\DISPLAY1:monitor_idx=0",
            "\\\\.\\DISPLAY1\n\\\\.\\DISPLAY2",
        ] {
            assert!(parse_device_name(invalid.as_bytes()).is_err(), "{invalid}");
        }
        assert!(parse_device_name(&[255]).is_err());
    }

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

    #[test]
    fn h264_stream_is_split_into_bounded_chunks() {
        let input = vec![7; MAX_VIDEO_PAYLOAD + 1];
        let mut output = Vec::new();
        assert_eq!(
            send_h264_stream(input.as_slice(), &mut output, 42).expect("valid stream"),
            (input.len() as u64, 2)
        );
        let mut messages = output.as_slice();
        for (number, size) in [(0, MAX_VIDEO_PAYLOAD), (1, 1)] {
            match read_message(&mut messages).expect("valid chunk") {
                Message::VideoChunk {
                    session_id,
                    frame_number,
                    fragment_index,
                    fragment_count,
                    keyframe,
                    payload,
                    ..
                } => {
                    assert_eq!(session_id, 42);
                    assert_eq!(frame_number, number);
                    assert_eq!((fragment_index, fragment_count), (0, 1));
                    assert_eq!(keyframe, number == 0);
                    assert_eq!(payload.len(), size);
                }
                message => panic!("unexpected message: {message:?}"),
            }
        }
        assert_eq!(
            read_message(&mut messages).expect("valid stop"),
            Message::Stop
        );

        assert_eq!(
            log_ffmpeg_progress(
                b"frame=30\nfps=29.9\nbitrate=4000.0kbits/s\nspeed=0.99x\nprogress=continue\n"
                    .as_slice()
            )
            .expect("valid progress"),
            ["30", "29.9", "4000.0kbits/s", "0.99x"].map(str::to_owned)
        );
    }
}
