//! Rendering what a compiler said when it refused.
//!
//! A compile error is the second most common thing a user sees, after bytecode, and dumping it
//! into a chat message as plain text loses everything that makes it readable: which line, which
//! column, what the caret was pointing at. So it goes through the same renderer, with the path
//! and position receding and the message itself in front.
//!
//! The classification is deliberately shallow. javac, kotlinc and groovyc agree on roughly
//! `path:line:col: severity: message`, and the parts that do not fit that shape are still worth
//! showing — they are just uncolored rather than dropped.

use crate::document::DocumentBuilder;
use crate::token::TokenKind;

/// How bad one line is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    fn kind(self) -> TokenKind {
        match self {
            // Malformed is the palette's red, which is what an error should be.
            Severity::Error => TokenKind::Malformed,
            Severity::Warning => TokenKind::AccessFlag,
            Severity::Note => TokenKind::Comment,
        }
    }

    fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "error" | "error:" | "e" | "fatal error" => Some(Severity::Error),
            "warning" | "warning:" | "w" | "warn" => Some(Severity::Warning),
            "note" | "info" => Some(Severity::Note),
            _ => None,
        }
    }
}

/// Emits a compiler's output as a document. `heading` is shown first, in the colour of the worst
/// severity found, so the reader knows what happened before reading anything else.
pub fn emit(heading: &str, output: &str, out: &mut DocumentBuilder) {
    let worst = output
        .lines()
        .filter_map(severity_of)
        .fold(Severity::Note, |worst, found| match (worst, found) {
            (Severity::Error, _) | (_, Severity::Error) => Severity::Error,
            (Severity::Warning, _) | (_, Severity::Warning) => Severity::Warning,
            _ => Severity::Note,
        });

    if !heading.is_empty() {
        out.push(worst.kind(), heading);
        out.newline();
        out.newline();
    }

    for line in output.lines() {
        emit_line(line, out);
        out.newline();
    }
}

fn severity_of(line: &str) -> Option<Severity> {
    split(line).map(|(_, _, severity, _)| severity)
}

/// Emits one line, split into its parts when it has them.
fn emit_line(line: &str, out: &mut DocumentBuilder) {
    let Some((path, position, severity, message)) = split(line) else {
        // A caret line is mostly spaces and one `^`, and it only means anything directly under
        // the line it refers to, so it keeps its indentation exactly.
        if line.trim_start().starts_with('^') {
            out.push(TokenKind::InstructionOffset, line);
        } else {
            out.push(TokenKind::Plain, line);
        }
        return;
    };

    out.push(TokenKind::FilePath, path);
    out.push(TokenKind::Plain, ":");
    out.push(TokenKind::Number, position);
    out.push(TokenKind::Plain, ": ");
    out.push(severity.kind(), severity_word(severity));
    out.push(TokenKind::Plain, ": ");
    out.push(TokenKind::Plain, message);
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}

/// Splits `path:line:col: severity: message`, returning the position as one already-joined run
/// because nothing downstream needs the line and the column apart.
///
/// Returns `None` for anything that is not shaped like a diagnostic, which includes the caret
/// lines, blank lines, and summary lines such as "2 errors".
fn split(line: &str) -> Option<(&str, &str, Severity, &str)> {
    // Work from the severity inwards: it is the only part with a fixed vocabulary. Searching for
    // it first avoids being confused by the colons inside a Windows path or a generic type.
    let (head, tail) = find_severity(line)?;
    let severity = Severity::parse(head.rsplit(':').next()?.trim())?;

    // Everything before the severity is the locator: `path:line` or `path:line:col`. javac prints
    // a column for some diagnostics and not others, and a Windows path contains a colon of its
    // own, so the numeric tail is taken from the right rather than the path from the left.
    let locator = head[..head.rfind(':')?].trim_end_matches(':');
    let fields: Vec<&str> = locator.split(':').collect();

    let mut taken = 0;
    while taken < 2 && taken + 1 < fields.len() {
        let candidate = fields[fields.len() - 1 - taken];
        if candidate.is_empty() || !candidate.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        taken += 1;
    }
    if taken == 0 {
        return None;
    }

    let tail_len: usize = fields[fields.len() - taken..]
        .iter()
        .map(|f| f.len())
        .sum::<usize>()
        + taken;
    let path = &locator[..locator.len() - tail_len];
    if path.is_empty() {
        return None;
    }

    Some((
        path,
        &locator[locator.len() - tail_len + 1..],
        severity,
        tail.trim_start(),
    ))
}

/// Finds the `: severity:` marker, returning the text up to and including the severity word and
/// the message after it.
fn find_severity(line: &str) -> Option<(&str, &str)> {
    for word in ["error:", "warning:", "note:", "info:"] {
        if let Some(at) = line.find(word) {
            let head = &line[..at + word.len() - 1];
            let tail = &line[at + word.len()..];
            // Require something that looks like a locator in front, otherwise a message that
            // merely contains the word "error:" would be mistaken for a diagnostic header.
            if head.matches(':').count() >= 2 {
                return Some((head, tail));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentBuilder;

    fn rendered(output: &str) -> (String, Vec<TokenKind>) {
        let mut builder = DocumentBuilder::new();
        emit("Compilation failed", output, &mut builder);
        let document = builder.finish();
        let kinds = document.tokens().iter().map(|t| t.kind).collect();
        (document.text().to_string(), kinds)
    }

    #[test]
    fn javac_diagnostic_is_split_into_parts() {
        let line = "Main.java:4: error: cannot find symbol";
        let (text, kinds) = rendered(line);

        assert!(text.contains("Main.java"), "{text}");
        assert!(text.contains("cannot find symbol"), "{text}");
        assert!(
            kinds.contains(&TokenKind::FilePath),
            "the path should be its own kind: {kinds:?}"
        );
        assert!(
            kinds.contains(&TokenKind::Malformed),
            "an error should be red: {kinds:?}"
        );
    }

    #[test]
    fn kotlinc_diagnostic_with_a_column_is_split_too() {
        let (text, kinds) = rendered("Snippet.kt:7:15: error: unresolved reference: foo");
        assert!(text.contains("7:15"), "{text}");
        assert!(text.contains("unresolved reference: foo"), "{text}");
        assert!(kinds.contains(&TokenKind::Number));
    }

    #[test]
    fn a_warning_is_not_coloured_like_an_error() {
        let (_, kinds) = rendered("A.java:1: warning: deprecated API");
        assert!(kinds.contains(&TokenKind::AccessFlag), "{kinds:?}");
        assert!(
            !kinds.contains(&TokenKind::Malformed),
            "nothing here is an error: {kinds:?}"
        );
    }

    // A caret line only means anything directly under the code it points at, so its leading
    // spaces have to survive.
    #[test]
    fn caret_lines_keep_their_indentation() {
        let (text, _) = rendered("        ^");
        assert!(text.contains("        ^"), "{text:?}");
    }

    // Lines that are not diagnostics are still shown, because a compiler's last line is often
    // the only useful one.
    #[test]
    fn unrecognized_lines_survive_uncoloured() {
        let (text, _) = rendered("2 errors\nnote: some messages have been simplified");
        assert!(text.contains("2 errors"), "{text}");
        assert!(text.contains("simplified"), "{text}");
    }

    // A message that merely mentions "error:" must not be mistaken for a diagnostic header.
    #[test]
    fn prose_containing_the_word_error_is_not_split() {
        let (text, _) = rendered("Note: recompile with -Xlint for details error: no");
        assert!(text.contains("recompile with -Xlint"), "{text}");
    }

    #[test]
    fn the_heading_takes_the_worst_severity_present() {
        let mut builder = DocumentBuilder::new();
        emit(
            "Compilation failed",
            "A.java:1: warning: deprecated\nA.java:2: error: broken",
            &mut builder,
        );
        let document = builder.finish();

        let heading = document.tokens().first().expect("no tokens");
        assert_eq!(heading.kind, TokenKind::Malformed);
    }
}
