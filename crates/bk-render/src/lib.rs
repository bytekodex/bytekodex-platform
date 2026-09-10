//! Token document to PNG.
//!
//! Bytecode is monospace, so layout is arithmetic: a character's position is its column times
//! the advance, and its line times the line height. There is no shaping, no line breaking and
//! no measuring pass — which is exactly the work a general text engine spends its time on.

pub mod atlas;
pub mod canvas;

use bk_core::{Document, Error, Result};
use bk_theme::Theme;

use crate::atlas::Atlas;
use crate::canvas::{Canvas, IDX_TRANSPARENT};

/// Telegram rejects a photo whose width plus height exceeds 10000. Staying under it with room
/// to spare is cheaper than discovering the limit at send time.
pub const DEFAULT_MAX_DIMENSION_SUM: u32 = 9600;

pub struct RenderOptions {
    pub font_size: f32,
    pub margin: u32,
    pub corner_radius: u32,
    /// Zero-based page.
    pub page: u32,
    pub page_rows: u32,
    pub max_dimension_sum: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            font_size: 40.0,
            margin: 32,
            corner_radius: 20,
            page: 0,
            page_rows: bk_core::ViewOptions::DEFAULT_PAGE_ROWS,
            max_dimension_sum: DEFAULT_MAX_DIMENSION_SUM,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rendered {
    pub width: u32,
    pub height: u32,
    pub pages_total: u32,
}

pub struct Renderer {
    atlas: Atlas,
    theme: &'static Theme,
}

impl Renderer {
    pub fn new(font_bytes: &[u8], font_size: f32, theme: &'static Theme) -> Result<Self> {
        Ok(Self {
            atlas: Atlas::new(font_bytes, font_size)?,
            theme,
        })
    }

    pub fn is_monospace(&self) -> bool {
        self.atlas.is_monospace()
    }

    pub fn pages(document: &Document, page_rows: u32) -> u32 {
        let rows = document.rows().max(1);
        let per_page = page_rows.max(1);
        rows.div_ceil(per_page)
    }

    /// Renders one page and appends the PNG to `out`.
    ///
    /// Appending into a caller-owned `Vec` rather than returning a fresh one is what lets the
    /// FFI layer hand us a buffer that Go allocated and pooled, so no image bytes are ever
    /// allocated on this side of the boundary.
    pub fn render_page(
        &mut self,
        document: &Document,
        options: &RenderOptions,
        out: &mut Vec<u8>,
    ) -> Result<Rendered> {
        let pages_total = Self::pages(document, options.page_rows);
        if options.page >= pages_total {
            return Err(Error::PageOutOfRange {
                requested: options.page,
                total: pages_total,
            });
        }

        let first_row = options.page * options.page_rows;
        let last_row = (first_row + options.page_rows).min(document.rows());
        let rows = last_row - first_row;

        let advance = self.atlas.advance;
        let line_height = self.atlas.line_height;
        let columns = page_columns(document, first_row, last_row);

        let width = (columns as f32 * advance).ceil() as u32 + options.margin * 2;
        let height = (rows as f32 * line_height).ceil() as u32 + options.margin * 2;
        if width + height > options.max_dimension_sum {
            return Err(Error::ImageTooLarge { width, height });
        }

        let mut canvas = Canvas::new(width.max(1), height.max(1));

        for row in first_row..last_row {
            let baseline =
                options.margin as f32 + (row - first_row) as f32 * line_height + self.atlas.ascent;
            let mut column = 0u32;

            for token in document.line_tokens(row) {
                for ch in document.slice(*token).chars() {
                    if !ch.is_whitespace() {
                        let pen_x = options.margin as f32 + column as f32 * advance;
                        let glyph = self.atlas.glyph(ch);
                        let left = (pen_x + glyph.metrics.xmin as f32).round() as i32;
                        let top = (baseline - glyph.metrics.ymin as f32).round() as i32
                            - glyph.metrics.height as i32;

                        for gy in 0..glyph.metrics.height {
                            for gx in 0..glyph.metrics.width {
                                let coverage = glyph.coverage[gy * glyph.metrics.width + gx];
                                canvas.blend(
                                    left + gx as i32,
                                    top + gy as i32,
                                    token.kind,
                                    coverage,
                                );
                            }
                        }
                    }
                    column += 1;
                }
            }
        }

        canvas.round_corners(options.corner_radius);
        encode(&canvas, self.theme, out)?;

        Ok(Rendered {
            width: canvas.width,
            height: canvas.height,
            pages_total,
        })
    }
}

fn page_columns(document: &Document, first_row: u32, last_row: u32) -> u32 {
    (first_row..last_row)
        .map(|row| {
            document
                .line_tokens(row)
                .iter()
                .map(|t| document.slice(*t).chars().count() as u32)
                .sum::<u32>()
        })
        .max()
        .unwrap_or(0)
        .max(1)
}

fn encode(canvas: &Canvas, theme: &Theme, out: &mut Vec<u8>) -> Result<()> {
    let (palette, alpha) = Canvas::palette(theme);

    let mut encoder = png::Encoder::new(out, canvas.width, canvas.height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(palette);
    // Only the rounded corners are transparent, but the alpha table has to cover the whole
    // palette, so it is emitted only when a corner was actually punched out.
    if canvas.pixels().contains(&IDX_TRANSPARENT) {
        encoder.set_trns(alpha);
    }
    // Screenshots of code compress well at a moderate effort; the top setting costs noticeably
    // more time for a fraction of a percent of size.
    encoder.set_compression(png::Compression::Balanced);

    let mut writer = encoder.write_header().map_err(|_| Error::EncodeFailure)?;
    writer
        .write_image_data(canvas.pixels())
        .map_err(|_| Error::EncodeFailure)?;
    writer.finish().map_err(|_| Error::EncodeFailure)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use bk_core::{DocumentBuilder, TokenKind};

    use super::*;

    /// A monospace TrueType font from the system, if one is installed in a form fontdue can
    /// read. Font collections (`.ttc`) are skipped because they are not single fonts.
    fn system_mono_font() -> Option<Vec<u8>> {
        [
            "/System/Library/Fonts/SFNSMono.ttf",
            "/Library/Fonts/JetBrainsMono-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "C:/Windows/Fonts/consola.ttf",
        ]
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    }

    fn sample() -> Document {
        let mut builder = DocumentBuilder::new();
        builder.push(TokenKind::InstructionOffset, "   0:");
        builder.pad(4);
        builder.push(TokenKind::Instruction, "invokevirtual");
        builder.push(TokenKind::Plain, " ");
        builder.push(TokenKind::ConstPoolIndex, "#12");
        builder.newline();
        builder.push(TokenKind::InstructionOffset, "   3:");
        builder.pad(4);
        builder.push(TokenKind::Instruction, "return");
        builder.finish()
    }

    #[test]
    fn pagination_covers_every_row_exactly_once() {
        let document = sample();
        assert_eq!(Renderer::pages(&document, 1), 2);
        assert_eq!(Renderer::pages(&document, 2), 1);
        assert_eq!(Renderer::pages(&document, 100), 1);
    }

    #[test]
    fn renders_a_page_to_a_png() {
        let Some(font) = system_mono_font() else {
            eprintln!("no usable system monospace font, skipping");
            return;
        };
        let mut renderer = Renderer::new(&font, 24.0, &bk_theme::DARK).unwrap();
        let document = sample();

        let mut png_bytes = Vec::new();
        let rendered = renderer
            .render_page(&document, &RenderOptions::default(), &mut png_bytes)
            .unwrap();

        assert_eq!(rendered.pages_total, 1);
        assert!(rendered.width > 0 && rendered.height > 0);
        assert_eq!(&png_bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn refuses_a_page_beyond_the_last() {
        let Some(font) = system_mono_font() else {
            return;
        };
        let mut renderer = Renderer::new(&font, 24.0, &bk_theme::DARK).unwrap();
        let options = RenderOptions {
            page: 9,
            ..RenderOptions::default()
        };

        let error = renderer
            .render_page(&sample(), &options, &mut Vec::new())
            .unwrap_err();
        assert!(matches!(
            error,
            Error::PageOutOfRange {
                requested: 9,
                total: 1
            }
        ));
    }
}
