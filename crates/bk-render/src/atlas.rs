use std::collections::HashMap;

use bk_core::{Error, Result};
use fontdue::{Font, FontSettings, Metrics};

/// One rasterized glyph: 8-bit coverage plus where to put it relative to the pen.
pub struct Glyph {
    pub metrics: Metrics,
    pub coverage: Vec<u8>,
}

/// Glyphs rasterized once and reused for every character of that codepoint.
///
/// This is where the memory budget is won. A page of bytecode is a few thousand characters
/// drawn from maybe a hundred distinct codepoints, so rasterizing per codepoint instead of per
/// character turns thousands of outline evaluations into a hundred, and the cache itself is
/// well under a megabyte.
pub struct Atlas {
    font: Font,
    size: f32,
    glyphs: HashMap<char, Glyph>,
    /// Horizontal advance, identical for every glyph because the font is monospace. The whole
    /// layout reduces to `col * advance`, so no shaping and no measuring pass.
    pub advance: f32,
    pub ascent: f32,
    pub line_height: f32,
}

impl Atlas {
    pub fn new(font_bytes: &[u8], size: f32) -> Result<Self> {
        if !(4.0..=400.0).contains(&size) {
            return Err(Error::InvalidArgument("font size out of range"));
        }
        let font = Font::from_bytes(
            font_bytes,
            FontSettings {
                scale: size,
                ..Default::default()
            },
        )
        .map_err(|_| Error::FontFailure("font could not be parsed"))?;

        let line = font
            .horizontal_line_metrics(size)
            .ok_or(Error::FontFailure("font has no horizontal line metrics"))?;

        // Measured on a glyph that exists in every font and is full width in a monospace one.
        let advance = font.metrics('M', size).advance_width;
        if advance <= 0.0 {
            return Err(Error::FontFailure("font reports a non-positive advance"));
        }

        let mut atlas = Self {
            font,
            size,
            glyphs: HashMap::with_capacity(128),
            advance,
            ascent: line.ascent,
            line_height: (line.ascent - line.descent + line.line_gap).ceil(),
        };
        for byte in 0x20u8..0x7F {
            atlas.glyph(byte as char);
        }
        Ok(atlas)
    }

    /// Whether the font actually is monospace. A proportional font still renders, but every
    /// column lands slightly wrong, so the caller deserves to know.
    pub fn is_monospace(&self) -> bool {
        ['i', 'M', 'w', '.', '0']
            .iter()
            .all(|&c| (self.font.metrics(c, self.size).advance_width - self.advance).abs() < 0.01)
    }

    pub fn glyph(&mut self, ch: char) -> &Glyph {
        if !self.glyphs.contains_key(&ch) {
            let (metrics, coverage) = self.font.rasterize(ch, self.size);
            self.glyphs.insert(ch, Glyph { metrics, coverage });
        }
        &self.glyphs[&ch]
    }
}
