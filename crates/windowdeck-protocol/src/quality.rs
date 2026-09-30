//! Bounded video sizes shared by capture and the native client's negotiation.
//! These are local limits; the version 3 wire format remains unchanged.
pub const DECK: (u16, u16) = (1280, 800);
pub const FULL_HD: (u16, u16) = (1920, 1080);
pub const MAX_NATIVE: (u16, u16) = (2560, 1440);

/// Fit without stretching or upscaling; H.264 4:2:0 requires even dimensions.
pub fn fit(source: (u32, u32), bounds: (u16, u16)) -> Result<(u16, u16), &'static str> {
    let (width, height) = (u64::from(source.0), u64::from(source.1));
    let max_width = u64::from(bounds.0.min(MAX_NATIVE.0));
    let max_height = u64::from(bounds.1.min(MAX_NATIVE.1));
    if width < 2 || height < 2 || max_width < 2 || max_height < 2 {
        return Err("resolución de vídeo inválida");
    }
    let (width, height) = if width <= max_width && height <= max_height {
        (width, height)
    } else if width * max_height >= height * max_width {
        (max_width, max_width * height / width)
    } else {
        (max_height * width / height, max_height)
    };
    let size = ((width & !1) as u16, (height & !1) as u16);
    if size.0 < 2 || size.1 < 2 {
        return Err("proporción de pantalla no compatible");
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_source_and_receiver_without_upscaling() {
        assert_eq!(fit((2560, 1440), DECK), Ok((1280, 720)));
        assert_eq!(fit((3840, 2160), FULL_HD), Ok(FULL_HD));
        assert_eq!(fit((3840, 2160), MAX_NATIVE), Ok(MAX_NATIVE));
        assert_eq!(fit((1920, 1200), DECK), Ok(DECK));
        assert_eq!(fit((1280, 800), MAX_NATIVE), Ok(DECK));
        assert_eq!(fit((640, 481), DECK), Ok((640, 480)));
        assert_eq!(fit((1080, 1920), DECK), Ok((450, 800)));
        assert_eq!(fit((5120, 1440), MAX_NATIVE), Ok((2560, 720)));
        assert_eq!(fit((3840, 2160), (u16::MAX, u16::MAX)), Ok(MAX_NATIVE));
        assert!(fit((0, 1080), DECK).is_err());
        assert!(fit((1920, 1080), (1, 1)).is_err());
        assert!(fit((2, u32::MAX), MAX_NATIVE).is_err());
    }
}
