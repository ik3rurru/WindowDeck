#[cfg(any(feature = "native-media", test))]
use windowdeck_protocol::quality::{DECK, FULL_HD, MAX_NATIVE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    #[default]
    Auto,
    Deck,
    FullHd,
    Qhd,
}

impl Quality {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "auto" => Ok(Self::Auto),
            "deck" => Ok(Self::Deck),
            "1080p" => Ok(Self::FullHd),
            "1440p" => Ok(Self::Qhd),
            _ => Err("--quality admite auto, deck, 1080p o 1440p"),
        }
    }

    #[cfg(any(feature = "native-media", test))]
    pub fn bounds(self, output: (u32, u32)) -> (u16, u16) {
        match self {
            // Keep legacy fixed-size extension usable even in a small window.
            Self::Auto => (
                output.0.clamp(u32::from(DECK.0), u32::from(MAX_NATIVE.0)) as u16 & !1,
                output.1.clamp(u32::from(DECK.1), u32::from(MAX_NATIVE.1)) as u16 & !1,
            ),
            Self::Deck => DECK,
            Self::FullHd => FULL_HD,
            Self::Qhd => MAX_NATIVE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_bound_tv_and_handheld_sizes() {
        assert_eq!(Quality::Auto.bounds((1280, 800)), DECK);
        assert_eq!(Quality::Auto.bounds((3840, 2160)), MAX_NATIVE);
        assert_eq!(Quality::Auto.bounds((1920, 1080)), FULL_HD);
        assert_eq!(Quality::Auto.bounds((640, 480)), DECK);
        assert_eq!(Quality::Deck.bounds((3840, 2160)), DECK);
        assert_eq!(Quality::FullHd.bounds((3840, 2160)), FULL_HD);
        assert_eq!(Quality::Qhd.bounds((1280, 800)), MAX_NATIVE);
    }
}
