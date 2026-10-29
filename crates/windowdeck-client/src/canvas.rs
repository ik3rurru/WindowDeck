//! Minimal software drawing for the host picker and pairing windows.
use font8x8::UnicodeFonts;

pub const BACKGROUND: u32 = 0x18202c;
pub const WHITE: u32 = 0xffffff;
pub const MUTED: u32 = 0xb8c2d0;
pub const ACCEPT: u32 = 0x2e7d32;
pub const REJECT: u32 = 0xb3261e;
pub const NEUTRAL: u32 = 0x3a4656;

pub struct Canvas<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && y >= self.y as f64
            && x < (self.x + self.width) as f64
            && y < (self.y + self.height) as f64
    }
}

impl<'a> Canvas<'a> {
    pub fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self {
        Self {
            pixels,
            width,
            height,
        }
    }

    pub fn fill(&mut self, rect: Rect, color: u32) {
        for y in rect.y..(rect.y + rect.height).min(self.height) {
            let row = y * self.width;
            for x in rect.x..(rect.x + rect.width).min(self.width) {
                self.pixels[row + x] = color;
            }
        }
    }

    /// Draws one line of 8x8 glyphs at `scale`, clipped to the canvas. Covers
    /// ASCII and Latin-1, enough for Spanish text; other characters become `?`.
    pub fn text(&mut self, x: usize, y: usize, scale: usize, color: u32, value: &str) {
        let advance = 8 * scale;
        let fits = self.width.saturating_sub(x) / advance.max(1);
        for (index, c) in value.chars().take(fits).enumerate() {
            let glyph = font8x8::BASIC_FONTS
                .get(c)
                .or_else(|| font8x8::LATIN_FONTS.get(c))
                .or_else(|| font8x8::BASIC_FONTS.get('?'))
                .unwrap_or([0; 8]);
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..8 {
                    if bits & (1 << column) != 0 {
                        self.fill(
                            Rect {
                                x: x + index * advance + column * scale,
                                y: y + row * scale,
                                width: scale,
                                height: scale,
                            },
                            color,
                        );
                    }
                }
            }
        }
    }

    /// Text horizontally centred on the canvas.
    pub fn centered(&mut self, y: usize, scale: usize, color: u32, value: &str) {
        let width = value.chars().count() * 8 * scale;
        self.text(self.width.saturating_sub(width) / 2, y, scale, color, value);
    }

    pub fn button(&mut self, rect: Rect, color: u32, label: &str) {
        self.fill(rect, color);
        let scale = 2;
        let width = label.chars().count() * 8 * scale;
        self.text(
            rect.x + rect.width.saturating_sub(width) / 2,
            rect.y + rect.height.saturating_sub(8 * scale) / 2,
            scale,
            WHITE,
            label,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_clipped_and_supports_spanish_characters() {
        let mut pixels = vec![0; 40 * 20];
        let mut canvas = Canvas::new(&mut pixels, 40, 20);
        canvas.text(0, 0, 1, WHITE, "Sí, código ñ…");
        canvas.centered(10, 1, WHITE, "«ok»");
        canvas.text(100, 100, 3, WHITE, "fuera");
        assert!(pixels.contains(&WHITE));
        let mut latin = vec![0; 8 * 8];
        Canvas::new(&mut latin, 8, 8).text(0, 0, 1, WHITE, "ñ");
        assert!(latin.contains(&WHITE), "Latin-1 glyphs are drawn");
    }

    #[test]
    fn buttons_report_hits_inside_their_area() {
        let rect = Rect {
            x: 10,
            y: 20,
            width: 100,
            height: 40,
        };
        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(109.9, 59.9));
        assert!(!rect.contains(110.0, 30.0));
        assert!(!rect.contains(50.0, 19.9));
        let mut pixels = vec![0; 120 * 70];
        Canvas::new(&mut pixels, 120, 70).button(rect, ACCEPT, "Sí");
        assert_eq!(pixels[25 * 120 + 12], ACCEPT);
    }
}
