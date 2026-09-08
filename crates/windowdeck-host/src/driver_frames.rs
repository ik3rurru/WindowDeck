use super::AnyError;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use windowdeck_diagnostics::{Level, emit};

const WIDTH: usize = 1280;
const HEIGHT: usize = 800;
const BYTES: usize = WIDTH * HEIGHT * 4;
const FRAMES: u64 = 120;

struct Source {
    child: Arc<Mutex<Child>>,
    cancel: mpsc::Sender<()>,
    watcher: Option<thread::JoinHandle<()>>,
}
impl Drop for Source {
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

fn header(bytes: &[u8; 64], previous: u64) -> Result<(u64, f64, u64), AnyError> {
    let word = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    let wide = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
    if word(0) != 0x31464457
        || word(4) != WIDTH as u32
        || word(8) != HEIGHT as u32
        || word(12) != BYTES as u32
    {
        return Err("cabecera de frame del driver inválida".into());
    }
    let (sequence, acquired, published, frequency) = (wide(16), wide(32), wide(40), wide(48));
    if sequence <= previous || published < acquired || frequency == 0 {
        return Err("secuencia o timestamps del driver inválidos".into());
    }
    Ok((
        sequence,
        (published - acquired) as f64 * 1000.0 / frequency as f64,
        wide(56),
    ))
}

fn pattern_phase(pixels: &[u8]) -> Option<usize> {
    if pixels.len() != BYTES {
        return None;
    }
    let pixel = |x, y| &pixels[(y * WIDTH + x) * 4..(y * WIDTH + x) * 4 + 3];
    if !pixel(16, 16).iter().all(|v| *v >= 251) || !pixel(1264, 16).iter().all(|v| *v <= 4) {
        return None;
    }
    let colors = [[40u8, 40, 220], [40, 220, 40], [220, 40, 40]];
    colors.iter().position(|color| {
        [
            (100, 100),
            (640, 100),
            (1100, 100),
            (100, 700),
            (640, 400),
            (1100, 700),
        ]
        .iter()
        .all(|&(x, y)| {
            pixel(x, y)
                .iter()
                .zip(color)
                .all(|(actual, expected)| actual.abs_diff(*expected) <= 4)
        })
    })
}

pub(super) fn run(tool: PathBuf, gpu: bool) -> Result<(), AnyError> {
    let mut child = Command::new(tool)
        .arg(if gpu {
            "--gpu-frame-source"
        } else {
            "--frame-source"
        })
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or("falta la salida del prototipo de frames")?;
    let child = Arc::new(Mutex::new(child));
    let watched = Arc::clone(&child);
    let (cancel, cancelled) = mpsc::channel();
    let watcher = thread::spawn(move || {
        if matches!(
            cancelled.recv_timeout(Duration::from_secs(25)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ) && let Ok(mut child) = watched.lock()
        {
            let _ = child.kill();
        }
    });
    let source = Source {
        child,
        cancel,
        watcher: Some(watcher),
    };
    let mut reader = BufReader::new(output);
    let mut pixels = vec![0; BYTES];
    let mut last = 0;
    let mut phases = [0u64; 3];
    let mut unmatched = Vec::new();
    let mut readback_ms = 0.0;
    let mut copy_micros = 0u64;
    let mut first = None;
    for frame in 0..FRAMES {
        let mut bytes = [0; 64];
        reader.read_exact(&mut bytes)?;
        let (sequence, delay, copy) = header(&bytes, last)?;
        reader.read_exact(&mut pixels)?;
        first.get_or_insert_with(Instant::now);
        if let Some(phase) = pattern_phase(&pixels) {
            phases[phase] += 1;
        } else {
            unmatched.push(frame);
        }
        last = sequence;
        readback_ms += delay;
        copy_micros += copy;
    }
    let elapsed = first.unwrap().elapsed();
    if reader.read(&mut [0u8])? != 0 {
        return Err("el prototipo emitió frames adicionales".into());
    }
    let status = loop {
        if let Some(status) = source
            .child
            .lock()
            .map_err(|_| "proceso de frames bloqueado")?
            .try_wait()?
        {
            break status;
        }
        thread::sleep(Duration::from_millis(20));
    };
    if !status.success() {
        return Err(format!("el auxiliar de frames terminó con {status}").into());
    }
    let matched: u64 = phases.iter().sum();
    // The first captures may precede the pattern window. Once it is visible,
    // every subsequent frame must match; do not hide intermittent corruption
    // behind a percentage threshold.
    let only_warmup = unmatched
        .iter()
        .enumerate()
        .all(|(index, frame)| *frame == index as u64);
    if matched < 90 || phases.iter().any(|count| *count < 5) || !only_warmup {
        return Err(format!(
            "no se verificó el patrón del escritorio virtual: {matched}/{FRAMES}, fases {phases:?}"
        )
        .into());
    }
    emit(
        Level::Info,
        "driver_frame_probe_passed",
        &[
            ("transfer", if gpu { "d3d11" } else { "cpu" }),
            ("frames", &FRAMES.to_string()),
            ("matched_pattern", &matched.to_string()),
            ("unmatched_frames", &format!("{unmatched:?}")),
            ("bytes", &(FRAMES * BYTES as u64).to_string()),
            ("last_sequence", &last.to_string()),
            (
                "receive_fps",
                &format!("{:.2}", (FRAMES - 1) as f64 / elapsed.as_secs_f64()),
            ),
            (
                "acquire_to_publish_mean_ms",
                &format!("{:.3}", readback_ms / FRAMES as f64),
            ),
            ("shared_copy_mean_us", &(copy_micros / FRAMES).to_string()),
        ],
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_header_rejects_wrong_size_order_and_clock() {
        let mut bytes = [0; 64];
        for (offset, value) in [(0, 0x31464457u32), (4, 1280), (8, 800), (12, BYTES as u32)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (offset, value) in [(16, 1u64), (32, 100), (40, 200), (48, 1000)] {
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        assert_eq!(header(&bytes, 0).unwrap(), (1, 100.0, 0));
        assert!(header(&bytes, 1).is_err());
        bytes[48..56].fill(0);
        assert!(header(&bytes, 0).is_err());
        bytes[48..56].copy_from_slice(&1000u64.to_le_bytes());
        bytes[40..48].copy_from_slice(&99u64.to_le_bytes());
        assert!(header(&bytes, 0).is_err());
        bytes[40..48].copy_from_slice(&200u64.to_le_bytes());
        bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(header(&bytes, 0).is_err());
    }
    #[test]
    fn pattern_requires_markers_and_consistent_bgra_samples() {
        let mut pixels = [40, 40, 220, 255].repeat(WIDTH * HEIGHT);
        assert_eq!(pattern_phase(&pixels), None);
        pixels[(16 * WIDTH + 16) * 4..(16 * WIDTH + 16) * 4 + 3].fill(255);
        pixels[(16 * WIDTH + 1264) * 4..(16 * WIDTH + 1264) * 4 + 3].fill(0);
        assert_eq!(pattern_phase(&pixels), Some(0));
        pixels[(400 * WIDTH + 640) * 4] = 200;
        assert_eq!(pattern_phase(&pixels), None);
    }
}
