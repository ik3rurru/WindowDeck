#![cfg(feature = "native")]
//! Owned, single-threaded handles around FFmpeg/SDL. No native pointer or borrowed
//! AVPacket escapes this module; compressed packets are copied before the next call.
#[cfg(windows)]
use std::ffi::CString;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::io;
use std::ptr::NonNull;

unsafe extern "C" {
    fn wd_version() -> *const c_char;
    fn wd_error() -> *const c_char;
    fn wd_player_open(fullscreen: c_int) -> *mut c_void;
    fn wd_player_close(player: *mut c_void);
    fn wd_player_packet(player: *mut c_void, bytes: *const u8, len: c_int, pts: i64) -> c_int;
    fn wd_player_poll(player: *mut c_void) -> c_int;
    fn wd_player_reset(player: *mut c_void) -> c_int;
    fn wd_self_test() -> c_int;
    #[cfg(windows)]
    fn wd_encoder_open(mapping: *const c_char, codec: *const c_char) -> *mut c_void;
    #[cfg(windows)]
    fn wd_encoder_close(encoder: *mut c_void);
    #[cfg(windows)]
    fn wd_encoder_next(encoder: *mut c_void, packet: *mut RawPacket) -> c_int;
    #[cfg(windows)]
    fn wd_gpu_self_test(codec: *const c_char) -> c_int;
}

fn error() -> io::Error {
    // SAFETY: the bridge returns a thread-local, NUL-terminated error string.
    io::Error::other(
        unsafe { CStr::from_ptr(wd_error()) }
            .to_string_lossy()
            .into_owned(),
    )
}

pub fn version() -> String {
    // SAFETY: FFmpeg owns this immutable static string for the process lifetime.
    unsafe { CStr::from_ptr(wd_version()) }
        .to_string_lossy()
        .into_owned()
}

/// Run only as a standalone diagnostic process; SDL uses its dummy video driver.
pub fn self_test() -> io::Result<()> {
    // SAFETY: diagnostic owns all resources for the duration of the call.
    if unsafe { wd_self_test() } < 0 {
        return Err(error());
    }
    Ok(())
}

#[cfg(windows)]
pub fn gpu_self_test(codec: &str) -> io::Result<()> {
    let codec = CString::new(codec).map_err(io::Error::other)?;
    // SAFETY: the diagnostic owns offscreen textures, encoders and decoders. It
    // never creates a display, reads the desktop or changes the installed driver.
    if unsafe { wd_gpu_self_test(codec.as_ptr()) } < 0 {
        return Err(error());
    }
    Ok(())
}

pub struct Player(NonNull<c_void>);
impl Player {
    pub fn new(fullscreen: bool) -> io::Result<Self> {
        // SAFETY: constructor transfers sole ownership or returns null on failure.
        NonNull::new(unsafe { wd_player_open(i32::from(fullscreen)) })
            .map(Self)
            .ok_or_else(error)
    }
    pub fn packet(&mut self, bytes: &[u8], pts: u64) -> io::Result<()> {
        let len = i32::try_from(bytes.len()).map_err(io::Error::other)?;
        let pts = i64::try_from(pts).map_err(io::Error::other)?;
        // SAFETY: owned handle, valid slice during the call. The bridge copies input.
        if unsafe { wd_player_packet(self.0.as_ptr(), bytes.as_ptr(), len, pts) } < 0 {
            return Err(error());
        }
        Ok(())
    }
    pub fn poll(&mut self) -> io::Result<bool> {
        // SAFETY: all SDL calls remain on the thread that constructed this handle.
        match unsafe { wd_player_poll(self.0.as_ptr()) } {
            n if n < 0 => Err(error()),
            0 => Ok(false),
            _ => Ok(true),
        }
    }
    pub fn reset(&mut self) -> io::Result<()> {
        // SAFETY: unique mutable access; flushes decoder reference pictures on reconnect.
        if unsafe { wd_player_reset(self.0.as_ptr()) } < 0 {
            return Err(error());
        }
        Ok(())
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: exactly one destruction per successful constructor.
        unsafe { wd_player_close(self.0.as_ptr()) }
    }
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct RawPacket {
    bytes: *const u8,
    len: c_int,
    keyframe: c_int,
    captured_micros: u64,
}

#[cfg(windows)]
pub struct Packet {
    pub bytes: Vec<u8>,
    pub keyframe: bool,
    pub captured_micros: u64,
}

#[cfg(windows)]
pub struct Encoder(NonNull<c_void>);
#[cfg(windows)]
impl Encoder {
    pub fn new(mapping: &str, codec: &str) -> io::Result<Self> {
        let mapping = CString::new(mapping).map_err(io::Error::other)?;
        let codec = CString::new(codec).map_err(io::Error::other)?;
        // SAFETY: both strings are valid through construction; native code copies them.
        NonNull::new(unsafe { wd_encoder_open(mapping.as_ptr(), codec.as_ptr()) })
            .map(Self)
            .ok_or_else(error)
    }
    pub fn next_packet(&mut self) -> io::Result<Option<Packet>> {
        let mut packet = RawPacket::default();
        // SAFETY: unique owned encoder and a correctly laid-out writable output struct.
        match unsafe { wd_encoder_next(self.0.as_ptr(), &mut packet) } {
            n if n < 0 => Err(error()),
            0 => Ok(None),
            _ => {
                if packet.len <= 0 || packet.len > 4 * 1024 * 1024 || packet.bytes.is_null() {
                    return Err(io::Error::other("invalid native packet"));
                }
                // SAFETY: bridge owns len bytes until the next call; copy them now.
                let bytes =
                    unsafe { std::slice::from_raw_parts(packet.bytes, packet.len as usize) }
                        .to_vec();
                Ok(Some(Packet {
                    bytes,
                    keyframe: packet.keyframe != 0,
                    captured_micros: packet.captured_micros,
                }))
            }
        }
    }
}
#[cfg(windows)]
impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: sole ownership, no in-flight calls on another thread.
        unsafe { wd_encoder_close(self.0.as_ptr()) }
    }
}
