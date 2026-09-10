//! The seam between bytecode formats and pixels.
//!
//! A frontend (JVM class files, .NET IL, or plain `javap` text) turns its input into a
//! [`Document`]: a flat run of [`Token`]s over one text buffer. The renderer consumes a
//! `Document` and knows nothing about opcodes. Adding a bytecode format means adding a
//! frontend, never touching the drawing code.

pub mod document;
pub mod error;
pub mod frontend;
pub mod stats;
pub mod token;
pub mod view;

pub use document::{Document, DocumentBuilder, Span};
pub use error::{Error, Result};
pub use frontend::Frontend;
pub use stats::{MethodStat, Stats};
pub use token::{Token, TokenKind};
pub use view::{InputKind, Platform, ViewFlags, ViewOptions};
