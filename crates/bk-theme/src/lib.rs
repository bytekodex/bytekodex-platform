//! Token kind to color.
//!
//! The palette is carried over unchanged from the original painter so existing screenshots
//! stay recognizable. Only the three kinds the painter had no concept of — instruction
//! offsets, branch labels and constant pool indices — get new colors.

use bk_core::TokenKind;

/// Opaque 8-bit-per-channel color. Alpha exists because the renderer composites glyph
/// coverage, not because anything is drawn translucent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xFF }
    }

    /// Packs to the `RGBA8` byte order both `tiny-skia` and the PNG encoder expect.
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

pub struct Theme {
    pub background: Rgba,
    /// Indexed by `TokenKind as usize`.
    colors: [Rgba; TokenKind::COUNT],
}

/// The palette shipped by the original painter, extended.
///
/// A `static` rather than a `const` so `&DARK` is genuinely `'static` and a renderer can hold
/// the reference for its whole life without relying on const promotion.
pub static DARK: Theme = {
    let plain = Rgba::rgb(0xDC, 0xDC, 0xDC);
    let blue = Rgba::rgb(0x56, 0x9C, 0xD6);
    let sand = Rgba::rgb(0xD6, 0x9D, 0x85);
    let violet = Rgba::rgb(0xB3, 0x89, 0xC5);

    let mut colors = [plain; TokenKind::COUNT];
    colors[TokenKind::Comment as usize] = Rgba::rgb(0x41, 0xA5, 0x3F);
    colors[TokenKind::FilePath as usize] = Rgba::rgb(0xA9, 0xA9, 0xA9);
    colors[TokenKind::Keyword as usize] = blue;
    colors[TokenKind::AccessFlag as usize] = Rgba::rgb(0xBB, 0xB5, 0x29);
    colors[TokenKind::Primitive as usize] = blue;
    colors[TokenKind::Literal as usize] = blue;
    colors[TokenKind::StringLiteral as usize] = sand;
    colors[TokenKind::Number as usize] = Rgba::rgb(0xB5, 0xCE, 0xA8);
    colors[TokenKind::TypeName as usize] = Rgba::rgb(0x4E, 0xC9, 0xB0);
    colors[TokenKind::Descriptor as usize] = sand;
    colors[TokenKind::Signature as usize] = sand;
    colors[TokenKind::Instruction as usize] = violet;
    colors[TokenKind::MethodHandleRef as usize] = violet;
    colors[TokenKind::ConstPoolTag as usize] = Rgba::rgb(0x9C, 0xDC, 0xFE);

    // New: an offset is structural, so it recedes rather than reading as an operand.
    colors[TokenKind::InstructionOffset as usize] = Rgba::rgb(0x6A, 0x6A, 0x6A);
    // New: branch targets and switch cases, so control flow is followable.
    colors[TokenKind::Label as usize] = Rgba::rgb(0xC5, 0x86, 0xC0);
    // New: `#12` is a pointer into the pool, not the number twelve.
    colors[TokenKind::ConstPoolIndex as usize] = Rgba::rgb(0x7E, 0x9C, 0xBD);

    colors[TokenKind::AttributeName as usize] = Rgba::rgb(0x9C, 0xDC, 0xFE);
    colors[TokenKind::LocalName as usize] = Rgba::rgb(0x9C, 0xDC, 0xFE);
    colors[TokenKind::Malformed as usize] = Rgba::rgb(0xF4, 0x47, 0x47);

    Theme {
        background: Rgba::rgb(0x1E, 0x1E, 0x1E),
        colors,
    }
};

impl Theme {
    pub fn color(&self, kind: TokenKind) -> Rgba {
        self.colors[kind as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_the_original_painter() {
        assert_eq!(
            DARK.color(TokenKind::Instruction),
            Rgba::rgb(0xB3, 0x89, 0xC5)
        );
        assert_eq!(DARK.color(TokenKind::Plain), Rgba::rgb(0xDC, 0xDC, 0xDC));
        assert_eq!(DARK.background, Rgba::rgb(0x1E, 0x1E, 0x1E));
    }

    #[test]
    fn offsets_and_pool_indices_differ_from_numbers() {
        assert_ne!(
            DARK.color(TokenKind::InstructionOffset),
            DARK.color(TokenKind::Number)
        );
        assert_ne!(
            DARK.color(TokenKind::ConstPoolIndex),
            DARK.color(TokenKind::Number)
        );
    }
}
