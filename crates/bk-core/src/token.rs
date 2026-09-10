/// What a run of characters means, independent of which bytecode format produced it.
///
/// Kinds are shared across platforms on purpose: JVM `invokevirtual` and IL `callvirt` are
/// both [`TokenKind::Instruction`], so the theme and the renderer stay format-agnostic.
/// Anything a frontend cannot classify becomes [`TokenKind::Plain`] rather than an error —
/// an unknown attribute should still be readable, just uncolored.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TokenKind {
    /// Unclassified text, punctuation, and whitespace.
    Plain = 0,
    Comment,
    /// Path of the class or assembly the dump came from.
    FilePath,
    /// Source-language keyword echoed by the disassembler (`class`, `extends`, `final`).
    Keyword,
    /// `ACC_PUBLIC`, `ACC_IDENTITY`, `ACC_STRICT_INIT`, and friends.
    AccessFlag,
    /// A primitive type name (`int`, `long`, `void`).
    Primitive,
    /// `null`, `true`, `false`.
    Literal,
    StringLiteral,
    Number,
    /// Fully qualified type name (`java/util/ArrayList`, `System.Collections.List`).
    TypeName,
    /// Method or field descriptor (`(Ljava/lang/String;)V`).
    Descriptor,
    /// Generic signature, which is a different grammar from a descriptor.
    Signature,
    /// The mnemonic itself.
    Instruction,
    /// Byte offset printed in front of an instruction. Deliberately its own kind so it can
    /// recede visually instead of competing with real numbers.
    InstructionOffset,
    /// Branch target or switch case label.
    Label,
    /// A `MethodHandle` reference kind (`REF_invokeStatic`).
    MethodHandleRef,
    /// Constant pool entry type (`Utf8`, `MethodHandle`, `Dynamic`).
    ConstPoolTag,
    /// A `#N` reference into the constant pool. Separate from [`TokenKind::Number`] so a
    /// pool index never reads as an operand value.
    ConstPoolIndex,
    /// Name of a class file attribute (`StackMapTable`, `LoadableDescriptors`).
    AttributeName,
    /// Local variable name from `LocalVariableTable`.
    LocalName,
    /// Something the frontend could parse but considers malformed.
    Malformed,
}

impl TokenKind {
    /// Total number of kinds. Themes index a palette array by `kind as usize`, so this is
    /// the array length they must provide.
    pub const COUNT: usize = TokenKind::Malformed as usize + 1;

    /// Every kind, in discriminant order, so a palette can be built by iterating rather than
    /// by casting an integer back into the enum.
    pub const ALL: [TokenKind; Self::COUNT] = [
        TokenKind::Plain,
        TokenKind::Comment,
        TokenKind::FilePath,
        TokenKind::Keyword,
        TokenKind::AccessFlag,
        TokenKind::Primitive,
        TokenKind::Literal,
        TokenKind::StringLiteral,
        TokenKind::Number,
        TokenKind::TypeName,
        TokenKind::Descriptor,
        TokenKind::Signature,
        TokenKind::Instruction,
        TokenKind::InstructionOffset,
        TokenKind::Label,
        TokenKind::MethodHandleRef,
        TokenKind::ConstPoolTag,
        TokenKind::ConstPoolIndex,
        TokenKind::AttributeName,
        TokenKind::LocalName,
        TokenKind::Malformed,
    ];
}

// `ALL` is hand-written, so it is checked rather than trusted: a kind added above without a
// matching entry here would silently shift the palette.
const _: () = {
    let mut i = 0;
    while i < TokenKind::COUNT {
        assert!(TokenKind::ALL[i] as usize == i);
        i += 1;
    }
};

/// One classified run of text inside a [`crate::Document`].
///
/// Eight bytes, `Copy`, and owning nothing: the text lives in the document's single buffer
/// and is addressed by `start`/`len`. A million-token dump costs 8 MB of tokens and one
/// copy of the text, with no per-token allocation to chase.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: TokenKind,
    /// Reserved so the struct stays 8 bytes when a flag byte is eventually needed.
    pub reserved: u8,
    /// Length in bytes. Frontends split runs longer than this, which never happens in
    /// practice for a single lexical token.
    pub len: u16,
    /// Byte offset into the document text.
    pub start: u32,
}

impl Token {
    pub const MAX_LEN: usize = u16::MAX as usize;
}

const _: () = assert!(size_of::<Token>() == 8);
