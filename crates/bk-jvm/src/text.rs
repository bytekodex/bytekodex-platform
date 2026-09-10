//! Highlighting `javap` output that a user pasted in.
//!
//! This is the only path that needs a lexer. When we read a class file ourselves the tokens
//! are generated with their meaning already known, so there is nothing to recognize. Here the
//! meaning has to be recovered from text, which is what `logos` is for: it compiles the
//! patterns below into a single DFA, so classification is one pass with no backtracking.

use bk_core::{DocumentBuilder, Result, Stats, TokenKind};
use logos::Logos;

use crate::opcodes::OPCODES;

#[derive(Logos, Debug, PartialEq)]
enum Lexeme {
    #[regex(r"[ \t]+")]
    Space,

    // Greedy by design: input is lexed one line at a time, so "to end of input" is "to end
    // of line" and cannot run away.
    #[regex(r"//[^\n\r]*", allow_greedy = true)]
    Comment,

    #[regex(r#""([^"\\\n]|\\.)*""#)]
    StringLiteral,

    #[regex(r"#[0-9]+")]
    PoolIndex,

    // `12:` is an instruction offset when it opens a line and an ordinary number anywhere
    // else. logos cannot express the line-start condition — that needs a lookbehind — so the
    // distinction is made by the caller, which knows whether anything preceded it.
    #[regex(r"[0-9]+:")]
    NumberColon,

    #[regex(r"ACC_[A-Z_]+")]
    AccessFlag,

    #[regex(r"REF_[a-zA-Z]+")]
    MethodHandleRef,

    #[regex(r"\([^)\n]*\)(\[*([BCDFIJSZV]|L[^;\n]*;))")]
    Descriptor,

    #[regex(r"-?[0-9]+\.[0-9]+([eE][-+]?[0-9]+)?[dDfF]?")]
    #[regex(r"-?0[xX][0-9a-fA-F]+")]
    #[regex(r"-?[0-9]+[lLdDfF]?")]
    Number,

    #[token("class")]
    #[token("interface")]
    #[token("extends")]
    #[token("implements")]
    #[token("public")]
    #[token("private")]
    #[token("protected")]
    #[token("static")]
    #[token("final")]
    #[token("abstract")]
    #[token("synchronized")]
    #[token("native")]
    #[token("transient")]
    #[token("volatile")]
    #[token("strictfp")]
    #[token("sealed")]
    #[token("permits")]
    #[token("record")]
    Keyword,

    #[token("byte")]
    #[token("short")]
    #[token("int")]
    #[token("long")]
    #[token("char")]
    #[token("float")]
    #[token("double")]
    #[token("boolean")]
    #[token("void")]
    Primitive,

    #[token("null")]
    #[token("true")]
    #[token("false")]
    Literal,

    // Constant pool tag names, as printed in the `Constant pool:` section.
    #[token("Utf8")]
    #[token("Integer")]
    #[token("Float")]
    #[token("Long")]
    #[token("Double")]
    #[token("Class")]
    #[token("String")]
    #[token("Fieldref")]
    #[token("Methodref")]
    #[token("InterfaceMethodref")]
    #[token("NameAndType")]
    #[token("MethodHandle")]
    #[token("MethodType")]
    #[token("Dynamic")]
    #[token("InvokeDynamic")]
    #[token("Module")]
    #[token("Package")]
    PoolTag,

    // A single word. Whether it is a mnemonic, an attribute name or just an identifier is
    // decided after the match, by looking it up — encoding 202 mnemonics as patterns would
    // bloat the DFA for no gain.
    #[regex(r"[A-Za-z_$][A-Za-z0-9_$]*(/[A-Za-z_$][A-Za-z0-9_$]*)+")]
    QualifiedName,

    #[regex(r"[A-Za-z_$][A-Za-z0-9_$]*")]
    Word,

    // Identifier characters are spelled out rather than using `\w`, which excludes `$` and so
    // would let this pattern overlap `Word`.
    #[regex(r"[^ \t\n\rA-Za-z0-9_$]")]
    Punctuation,
}

/// Attribute names `javap` prints as section headers.
const ATTRIBUTE_NAMES: &[&str] = &[
    "Code",
    "LineNumberTable",
    "LocalVariableTable",
    "LocalVariableTypeTable",
    "StackMapTable",
    "Exceptions",
    "InnerClasses",
    "EnclosingMethod",
    "Synthetic",
    "Signature",
    "SourceFile",
    "SourceDebugExtension",
    "Deprecated",
    "RuntimeVisibleAnnotations",
    "RuntimeInvisibleAnnotations",
    "RuntimeVisibleParameterAnnotations",
    "RuntimeInvisibleParameterAnnotations",
    "RuntimeVisibleTypeAnnotations",
    "RuntimeInvisibleTypeAnnotations",
    "AnnotationDefault",
    "BootstrapMethods",
    "MethodParameters",
    "Module",
    "ModulePackages",
    "ModuleMainClass",
    "NestHost",
    "NestMembers",
    "Record",
    "PermittedSubclasses",
    "ConstantValue",
    // JDK 28 preview, from JEP 401.
    "LoadableDescriptors",
];

fn is_mnemonic(word: &str) -> bool {
    OPCODES.iter().flatten().any(|info| info.name == word)
}

/// Highlights pasted disassembly. Opcode counting works here too — every recognized mnemonic
/// is one instruction — but it is an estimate, since a mnemonic can also appear inside a
/// comment or an identifier.
pub fn emit_text(input: &str, out: &mut DocumentBuilder) -> Result<Stats> {
    let mut stats = Stats {
        classes: 1,
        ..Stats::default()
    };

    for line in input.lines() {
        let mut lexer = Lexeme::lexer(line);
        let mut at_line_start = true;
        while let Some(result) = lexer.next() {
            let text = lexer.slice();
            let opens_the_line = at_line_start;
            if result != Ok(Lexeme::Space) {
                at_line_start = false;
            }

            let kind = match result {
                Ok(Lexeme::Space) | Ok(Lexeme::Punctuation) | Err(_) => TokenKind::Plain,
                Ok(Lexeme::Comment) => TokenKind::Comment,
                Ok(Lexeme::StringLiteral) => TokenKind::StringLiteral,
                Ok(Lexeme::PoolIndex) => TokenKind::ConstPoolIndex,
                Ok(Lexeme::NumberColon) => {
                    if opens_the_line {
                        TokenKind::InstructionOffset
                    } else {
                        let digits = text.len() - 1;
                        out.push(TokenKind::Number, &text[..digits]);
                        out.push(TokenKind::Plain, ":");
                        continue;
                    }
                }
                Ok(Lexeme::AccessFlag) => TokenKind::AccessFlag,
                Ok(Lexeme::MethodHandleRef) => TokenKind::MethodHandleRef,
                Ok(Lexeme::Descriptor) => TokenKind::Descriptor,
                Ok(Lexeme::Number) => TokenKind::Number,
                Ok(Lexeme::Keyword) => TokenKind::Keyword,
                Ok(Lexeme::Primitive) => TokenKind::Primitive,
                Ok(Lexeme::Literal) => TokenKind::Literal,
                Ok(Lexeme::PoolTag) => TokenKind::ConstPoolTag,
                Ok(Lexeme::QualifiedName) => TokenKind::TypeName,
                Ok(Lexeme::Word) => {
                    if is_mnemonic(text) {
                        stats.opcodes_total += 1;
                        TokenKind::Instruction
                    } else if ATTRIBUTE_NAMES.contains(&text) {
                        TokenKind::AttributeName
                    } else {
                        TokenKind::Plain
                    }
                }
            };
            out.push(kind, text);
        }
        out.newline();
    }

    bk_core::stats::record_opcodes(stats.opcodes_total);
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bk_core::TokenKind;

    fn kinds_of(source: &str) -> Vec<(TokenKind, String)> {
        let mut builder = DocumentBuilder::new();
        emit_text(source, &mut builder).unwrap();
        let document = builder.finish();
        document
            .tokens()
            .iter()
            .map(|t| (t.kind, document.slice(*t).to_string()))
            .filter(|(kind, text)| *kind != TokenKind::Plain || !text.trim().is_empty())
            .collect()
    }

    #[test]
    fn classifies_an_instruction_line() {
        let tokens = kinds_of("     4: invokevirtual #12   // java/io/PrintStream.println");
        assert!(tokens.contains(&(TokenKind::Instruction, "invokevirtual".into())));
        assert!(tokens.contains(&(TokenKind::ConstPoolIndex, "#12".into())));
        assert!(
            tokens
                .iter()
                .any(|(k, _)| *k == TokenKind::InstructionOffset)
        );
        assert!(
            tokens
                .iter()
                .any(|(k, t)| *k == TokenKind::Comment && t.starts_with("//"))
        );
    }

    #[test]
    fn counts_mnemonics() {
        let mut builder = DocumentBuilder::new();
        let stats = emit_text("0: iconst_0\n1: ireturn\n", &mut builder).unwrap();
        assert_eq!(stats.opcodes_total, 2);
    }

    #[test]
    fn descriptors_are_not_mistaken_for_punctuation() {
        let tokens = kinds_of("  main(Ljava/lang/String;)V;");
        assert!(
            tokens
                .iter()
                .any(|(k, t)| *k == TokenKind::Descriptor && t.contains("Ljava/lang/String;"))
        );
    }
}
