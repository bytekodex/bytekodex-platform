//! Access flag decoding, per context.
//!
//! Flag bits are not global: the same bit means different things on a class, a field, a method
//! and a method parameter. `0x0020` is `ACC_SYNCHRONIZED` on a method but `ACC_SUPER` on a
//! class — and as of JDK 28 preview it is `ACC_IDENTITY` on a class, since JEP 401 repurposed
//! it to mark identity classes. `0x0800` is `ACC_STRICT` (that is, `strictfp`) on a method but
//! `ACC_STRICT_INIT` on a field under JEP 539.
//!
//! Decoding these from one shared table, which is what the old ANTLR grammar did, is wrong for
//! any input that exercises the overlap.

/// Where the flags were read from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlagContext {
    Class,
    Field,
    Method,
    /// `MethodParameters` attribute entries.
    Parameter,
    /// `InnerClasses` entries, which use the class table minus `ACC_SUPER`.
    InnerClass,
    Module,
}

/// Names of the set bits, low to high, in the order `javap` prints them.
pub fn decode(context: FlagContext, flags: u16, preview: bool) -> Vec<&'static str> {
    let table: &[(u16, &'static str)] = match context {
        FlagContext::Class | FlagContext::InnerClass => &[
            (0x0001, "ACC_PUBLIC"),
            (0x0002, "ACC_PRIVATE"),
            (0x0004, "ACC_PROTECTED"),
            (0x0008, "ACC_STATIC"),
            (0x0010, "ACC_FINAL"),
            (0x0020, "ACC_SUPER"),
            (0x0200, "ACC_INTERFACE"),
            (0x0400, "ACC_ABSTRACT"),
            (0x1000, "ACC_SYNTHETIC"),
            (0x2000, "ACC_ANNOTATION"),
            (0x4000, "ACC_ENUM"),
            (0x8000, "ACC_MODULE"),
        ],
        FlagContext::Field => &[
            (0x0001, "ACC_PUBLIC"),
            (0x0002, "ACC_PRIVATE"),
            (0x0004, "ACC_PROTECTED"),
            (0x0008, "ACC_STATIC"),
            (0x0010, "ACC_FINAL"),
            (0x0040, "ACC_VOLATILE"),
            (0x0080, "ACC_TRANSIENT"),
            (0x0800, "ACC_STRICT_INIT"),
            (0x1000, "ACC_SYNTHETIC"),
            (0x4000, "ACC_ENUM"),
        ],
        FlagContext::Method => &[
            (0x0001, "ACC_PUBLIC"),
            (0x0002, "ACC_PRIVATE"),
            (0x0004, "ACC_PROTECTED"),
            (0x0008, "ACC_STATIC"),
            (0x0010, "ACC_FINAL"),
            (0x0020, "ACC_SYNCHRONIZED"),
            (0x0040, "ACC_BRIDGE"),
            (0x0080, "ACC_VARARGS"),
            (0x0100, "ACC_NATIVE"),
            (0x0400, "ACC_ABSTRACT"),
            (0x0800, "ACC_STRICT"),
            (0x1000, "ACC_SYNTHETIC"),
        ],
        FlagContext::Parameter => &[
            (0x0010, "ACC_FINAL"),
            (0x1000, "ACC_SYNTHETIC"),
            (0x8000, "ACC_MANDATED"),
        ],
        FlagContext::Module => &[
            (0x0020, "ACC_TRANSITIVE"),
            (0x0040, "ACC_STATIC_PHASE"),
            (0x1000, "ACC_SYNTHETIC"),
            (0x8000, "ACC_MANDATED"),
        ],
    };

    let mut names: Vec<&'static str> = table
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(bit, name)| {
            // JEP 401 renames this bit on classes, but only for preview class files; in a
            // non-preview file the same bit still means `ACC_SUPER`.
            if preview && context == FlagContext::Class && *bit == 0x0020 {
                "ACC_IDENTITY"
            } else {
                *name
            }
        })
        .collect();

    // A field with no `ACC_STRICT_INIT` in a non-preview file cannot have the bit set at all,
    // so an unexpected bit is worth surfacing rather than silently dropping.
    let known: u16 = table.iter().map(|(bit, _)| bit).fold(0, |a, b| a | b);
    if flags & !known != 0 {
        names.push("ACC_UNKNOWN");
    }
    names
}

/// Source-level keywords `javap` prints in a declaration, as opposed to the raw flag names.
pub fn declaration_keywords(context: FlagContext, flags: u16) -> Vec<&'static str> {
    let mut words = Vec::new();
    if flags & 0x0001 != 0 {
        words.push("public");
    }
    if flags & 0x0002 != 0 {
        words.push("private");
    }
    if flags & 0x0004 != 0 {
        words.push("protected");
    }
    if flags & 0x0008 != 0 {
        words.push("static");
    }
    if flags & 0x0010 != 0 {
        words.push("final");
    }
    if context == FlagContext::Method && flags & 0x0020 != 0 {
        words.push("synchronized");
    }
    if context == FlagContext::Field && flags & 0x0040 != 0 {
        words.push("volatile");
    }
    if context == FlagContext::Field && flags & 0x0080 != 0 {
        words.push("transient");
    }
    if context == FlagContext::Method && flags & 0x0100 != 0 {
        words.push("native");
    }
    if flags & 0x0400 != 0 {
        words.push("abstract");
    }
    if context == FlagContext::Method && flags & 0x0800 != 0 {
        words.push("strictfp");
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_0x0800_depends_on_context() {
        assert_eq!(
            decode(FlagContext::Method, 0x0800, false),
            vec!["ACC_STRICT"]
        );
        assert_eq!(
            decode(FlagContext::Field, 0x0800, false),
            vec!["ACC_STRICT_INIT"]
        );
    }

    #[test]
    fn bit_0x0020_depends_on_preview() {
        assert_eq!(decode(FlagContext::Class, 0x0020, false), vec!["ACC_SUPER"]);
        assert_eq!(
            decode(FlagContext::Class, 0x0020, true),
            vec!["ACC_IDENTITY"]
        );
        assert_eq!(
            decode(FlagContext::Method, 0x0020, true),
            vec!["ACC_SYNCHRONIZED"]
        );
    }
}
