use bk_core::TokenKind;
use bk_theme::Theme;

/// Coverage levels each color is quantized to, on top of fully transparent and pure
/// background. Ten steps is enough that antialiased text looks smooth and few enough that the
/// whole palette fits in 256 entries.
pub const LEVELS: usize = 10;

pub const IDX_TRANSPARENT: u8 = 0;
pub const IDX_BACKGROUND: u8 = 1;
const FIRST_COLOR_INDEX: usize = 2;

const _: () = assert!(FIRST_COLOR_INDEX + TokenKind::COUNT * LEVELS <= 256);

/// An 8-bit indexed pixel buffer.
///
/// One byte per pixel rather than four. Bytecode uses about twenty colors, so instead of
/// storing blended RGBA the buffer stores "which color, how covered" and the PNG palette does
/// the blending. Peak memory drops fourfold and the PNG comes out several times smaller,
/// which matters because Telegram caps a photo at 10 MB.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![IDX_BACKGROUND; (width * height) as usize],
        }
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Blends one glyph pixel. Coverage is quantized on the way in, and a lighter pixel never
    /// overwrites a darker one at the same spot so overlapping glyph boxes do not erase ink.
    pub fn blend(&mut self, x: i32, y: i32, kind: TokenKind, coverage: u8) {
        if coverage == 0 || x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let level = (u32::from(coverage) * LEVELS as u32 + 127) / 255;
        if level == 0 {
            return;
        }
        let index = (FIRST_COLOR_INDEX + kind as usize * LEVELS + (level as usize - 1)) as u8;

        let offset = (y as u32 * self.width + x as u32) as usize;
        let current = self.pixels[offset];
        if current == IDX_BACKGROUND || index > current {
            self.pixels[offset] = index;
        }
    }

    /// Punches transparent quarter-circles out of the four corners.
    ///
    /// The only non-text drawing the renderer does, and the reason no 2D library is needed:
    /// a rounded rectangle is one distance test per corner pixel.
    pub fn round_corners(&mut self, radius: u32) {
        if radius == 0 {
            return;
        }
        let radius = radius.min(self.width / 2).min(self.height / 2);
        let r = radius as f32 - 0.5;

        for corner_y in 0..radius {
            for corner_x in 0..radius {
                let dx = r - corner_x as f32;
                let dy = r - corner_y as f32;
                if dx * dx + dy * dy <= r * r {
                    continue;
                }
                for (x, y) in [
                    (corner_x, corner_y),
                    (self.width - 1 - corner_x, corner_y),
                    (corner_x, self.height - 1 - corner_y),
                    (self.width - 1 - corner_x, self.height - 1 - corner_y),
                ] {
                    self.pixels[(y * self.width + x) as usize] = IDX_TRANSPARENT;
                }
            }
        }
    }

    /// Builds the PNG palette this buffer's indices refer to: `(RGB triples, alpha values)`.
    pub fn palette(theme: &Theme) -> (Vec<u8>, Vec<u8>) {
        let background = theme.background;
        let mut rgb = Vec::with_capacity(256 * 3);
        let mut alpha = Vec::with_capacity(256);

        rgb.extend_from_slice(&[background.r, background.g, background.b]);
        alpha.push(0x00);
        rgb.extend_from_slice(&[background.r, background.g, background.b]);
        alpha.push(0xFF);

        for kind in TokenKind::ALL {
            let color = theme.color(kind);
            for level in 1..=LEVELS {
                let t = level as f32 / LEVELS as f32;
                let mix = |fg: u8, bg: u8| (fg as f32 * t + bg as f32 * (1.0 - t)).round() as u8;
                rgb.extend_from_slice(&[
                    mix(color.r, background.r),
                    mix(color.g, background.g),
                    mix(color.b, background.b),
                ]);
                alpha.push(0xFF);
            }
        }

        (rgb, alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_fits_in_one_byte_of_indices() {
        let (rgb, alpha) = Canvas::palette(&bk_theme::DARK);
        assert_eq!(rgb.len() / 3, alpha.len());
        assert!(alpha.len() <= 256, "palette has {} entries", alpha.len());
    }

    #[test]
    fn full_coverage_reaches_the_pure_color() {
        let (rgb, _) = Canvas::palette(&bk_theme::DARK);
        let index = FIRST_COLOR_INDEX + TokenKind::Instruction as usize * LEVELS + (LEVELS - 1);
        let color = bk_theme::DARK.color(TokenKind::Instruction);
        assert_eq!(&rgb[index * 3..index * 3 + 3], &[color.r, color.g, color.b]);
    }

    #[test]
    fn corners_become_transparent() {
        let mut canvas = Canvas::new(20, 20);
        canvas.round_corners(6);
        assert_eq!(canvas.pixels()[0], IDX_TRANSPARENT);
        assert_eq!(canvas.pixels()[(10 * 20 + 10) as usize], IDX_BACKGROUND);
    }
}
