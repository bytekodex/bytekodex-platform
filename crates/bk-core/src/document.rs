use core::fmt;

use crate::token::{Token, TokenKind};

/// A byte range inside a [`Document`]'s text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

/// Rendered text plus its classification, laid out as a grid of lines.
///
/// Frontends never hand out borrowed slices of their input. A class file has no text to
/// borrow in the first place — the mnemonics are synthesized — and for `javap` text the
/// input is a few kilobytes, so copying it into one buffer costs a single `memcpy` and
/// removes lifetimes from every downstream signature.
pub struct Document {
    text: String,
    tokens: Vec<Token>,
    /// Byte offset of the first character of each line, one entry per line.
    line_starts: Vec<u32>,
    /// Widest line, counted in characters, which for a monospace grid is columns.
    columns: u32,
}

impl Document {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn slice(&self, token: Token) -> &str {
        let start = token.start as usize;
        &self.text[start..start + token.len as usize]
    }

    /// Number of lines. Always at least one.
    pub fn rows(&self) -> u32 {
        self.line_starts.len().max(1) as u32
    }

    /// Width of the widest line in characters.
    pub fn columns(&self) -> u32 {
        self.columns
    }

    /// Tokens of a single line, for paged rendering.
    pub fn line_tokens(&self, row: u32) -> &[Token] {
        let Some(&start) = self.line_starts.get(row as usize) else {
            return &[];
        };
        let end = self
            .line_starts
            .get(row as usize + 1)
            .copied()
            .unwrap_or(self.text.len() as u32);
        let lo = self.tokens.partition_point(|t| t.start < start);
        let hi = self.tokens.partition_point(|t| t.start < end);
        &self.tokens[lo..hi]
    }
}

/// Builds a [`Document`] by appending classified text.
///
/// The builder owns line bookkeeping so no token ever straddles a newline, which is what
/// lets the renderer treat the result as a grid and lets pagination cut on line boundaries.
pub struct DocumentBuilder {
    text: String,
    tokens: Vec<Token>,
    line_starts: Vec<u32>,
    columns: u32,
    current_columns: u32,
}

impl DocumentBuilder {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            tokens: Vec::new(),
            line_starts: vec![0],
            columns: 0,
            current_columns: 0,
        }
    }

    /// Pre-sizes the buffers. Worth calling: a frontend can estimate output from input size
    /// and skip the growth reallocations entirely.
    pub fn with_capacity(bytes: usize, tokens: usize) -> Self {
        let mut builder = Self::new();
        builder.text.reserve(bytes);
        builder.tokens.reserve(tokens);
        builder
    }

    /// Appends `text` as one token. Embedded newlines are not allowed and are replaced with
    /// spaces, because a token spanning lines would break both layout and pagination.
    pub fn push(&mut self, kind: TokenKind, text: &str) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len() as u32;
        if text.contains(['\n', '\r']) {
            let sanitized: String = text
                .chars()
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                .collect();
            self.text.push_str(&sanitized);
        } else {
            self.text.push_str(text);
        }
        let len = self.text.len() as u32 - start;
        self.current_columns += self.text[start as usize..].chars().count() as u32;

        // Split anything longer than a token can address. Real tokens never hit this.
        let mut offset = 0u32;
        while offset < len {
            let chunk = (len - offset).min(Token::MAX_LEN as u32);
            self.tokens.push(Token {
                kind,
                reserved: 0,
                len: chunk as u16,
                start: start + offset,
            });
            offset += chunk;
        }
    }

    /// Appends formatted text as one token, without an intermediate `String` allocation.
    pub fn push_fmt(&mut self, kind: TokenKind, args: fmt::Arguments<'_>) {
        use fmt::Write as _;

        let start = self.text.len();
        let _ = self.text.write_fmt(args);
        let appended = self.text.len() - start;
        if appended == 0 {
            return;
        }
        self.current_columns += self.text[start..].chars().count() as u32;
        let mut offset = 0usize;
        while offset < appended {
            let chunk = (appended - offset).min(Token::MAX_LEN);
            self.tokens.push(Token {
                kind,
                reserved: 0,
                len: chunk as u16,
                start: (start + offset) as u32,
            });
            offset += chunk;
        }
    }

    /// Appends `count` spaces as plain text. Common enough in a fixed-column dump to be
    /// worth its own entry point.
    pub fn pad(&mut self, count: usize) {
        const SPACES: &str = "                                                                ";
        let mut left = count;
        while left > 0 {
            let take = left.min(SPACES.len());
            self.push(TokenKind::Plain, &SPACES[..take]);
            left -= take;
        }
    }

    /// Pads with spaces until the current line is at least `column` characters wide.
    pub fn pad_to(&mut self, column: u32) {
        if self.current_columns < column {
            self.pad((column - self.current_columns) as usize);
        }
    }

    pub fn newline(&mut self) {
        self.text.push('\n');
        self.columns = self.columns.max(self.current_columns);
        self.current_columns = 0;
        self.line_starts.push(self.text.len() as u32);
    }

    pub fn current_column(&self) -> u32 {
        self.current_columns
    }

    pub fn finish(mut self) -> Document {
        self.columns = self.columns.max(self.current_columns);
        // A trailing newline would otherwise register as an extra empty line.
        if self.line_starts.last() == Some(&(self.text.len() as u32)) && self.text.ends_with('\n') {
            self.line_starts.pop();
        }
        Document {
            text: self.text,
            tokens: self.tokens,
            line_starts: self.line_starts,
            columns: self.columns,
        }
    }
}

impl Default for DocumentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_grid_geometry() {
        let mut builder = DocumentBuilder::new();
        builder.push(TokenKind::Instruction, "invokevirtual");
        builder.newline();
        builder.push(TokenKind::Plain, "ok");
        let document = builder.finish();

        assert_eq!(document.rows(), 2);
        assert_eq!(document.columns(), 13);
        assert_eq!(document.line_tokens(0).len(), 1);
        assert_eq!(document.slice(document.line_tokens(1)[0]), "ok");
    }

    #[test]
    fn newlines_inside_a_token_do_not_create_lines() {
        let mut builder = DocumentBuilder::new();
        builder.push(TokenKind::StringLiteral, "a\nb");
        let document = builder.finish();

        assert_eq!(document.rows(), 1);
        assert_eq!(document.text(), "a b");
    }
}
