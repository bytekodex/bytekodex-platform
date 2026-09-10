//! The JVM frontend.
//!
//! Two ways in, one way out. Raw class bytes go through [`class::ClassFile`], which reads the
//! binary format directly — no `javap`, no subprocess, no JDK at runtime. Pasted disassembly
//! goes through [`text`], which is where the `logos` lexer lives. Both append to the same
//! [`bk_core::DocumentBuilder`], so the renderer cannot tell them apart.

pub mod class;
pub mod emit;
pub mod flags;
pub mod opcodes;
pub mod pool;
pub mod reader;
pub mod stackmap;
pub mod text;

use bk_core::{DocumentBuilder, Error, Frontend, InputKind, Platform, Result, Stats, ViewOptions};

use crate::class::ClassFile;

pub struct JvmFrontend;

impl Frontend for JvmFrontend {
    fn platform(&self) -> Platform {
        Platform::Jvm
    }

    fn accepts(&self, kind: InputKind) -> bool {
        matches!(kind, InputKind::Binary | InputKind::DisassemblyText)
    }

    fn unit_name(&self, input: &[u8]) -> Option<String> {
        ClassFile::parse(input).ok()?.name().map(|n| n.to_string())
    }

    fn emit(
        &self,
        input: &[u8],
        kind: InputKind,
        options: &ViewOptions,
        out: &mut DocumentBuilder,
    ) -> Result<Stats> {
        match kind {
            InputKind::Binary => {
                let class = ClassFile::parse(input)?;
                emit::emit_class(&class, options, out)
            }
            InputKind::DisassemblyText => {
                let source = std::str::from_utf8(input).map_err(|e| Error::Malformed {
                    at: e.valid_up_to(),
                    what: "input is not UTF-8",
                })?;
                text::emit_text(source, out)
            }
            // Compiler output is not bytecode and is not this frontend's business; the renderer
            // handles it directly, without asking a platform.
            InputKind::Diagnostic => Err(Error::UnsupportedInputKind(kind as u32)),
        }
    }
}

/// Cheap sniff for which input kind a buffer holds, so callers do not have to guess.
pub fn detect_input_kind(input: &[u8]) -> InputKind {
    let is_class = input
        .first_chunk::<4>()
        .is_some_and(|magic| u32::from_be_bytes(*magic) == class::MAGIC);
    if is_class {
        InputKind::Binary
    } else {
        InputKind::DisassemblyText
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_class_magic() {
        assert_eq!(
            detect_input_kind(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0]),
            InputKind::Binary
        );
        assert_eq!(
            detect_input_kind(b"  0: return"),
            InputKind::DisassemblyText
        );
        assert_eq!(detect_input_kind(b""), InputKind::DisassemblyText);
    }
}
