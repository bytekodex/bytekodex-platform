use crate::error::Error;

/// Which bytecode family the input belongs to.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Jvm = 1,
    /// ECMA-335 CIL. Not implemented yet; the id is reserved so the ABI does not shift later.
    Cil = 2,
}

impl Platform {
    pub fn from_raw(value: u32) -> Result<Self, Error> {
        match value {
            1 => Ok(Platform::Jvm),
            2 => Ok(Platform::Cil),
            other => Err(Error::UnsupportedPlatform(other)),
        }
    }
}

/// What the caller handed us.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputKind {
    /// Raw bytes of one compiled unit — a `.class` file, or a `.dll` for CIL.
    Binary = 1,
    /// Text a disassembler already produced, pasted in by a user. This is the path the
    /// `logos` lexer serves; the binary path never needs a lexer.
    DisassemblyText = 2,
}

impl InputKind {
    pub fn from_raw(value: u32) -> Result<Self, Error> {
        match value {
            1 => Ok(InputKind::Binary),
            2 => Ok(InputKind::DisassemblyText),
            other => Err(Error::UnsupportedInputKind(other)),
        }
    }
}

/// What to include in the dump. The bot exposes these as one menu row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ViewFlags(pub u32);

impl ViewFlags {
    /// Class declaration, fields, method signatures and their code. The default view.
    pub const METHODS: Self = Self(1 << 0);
    pub const CONSTANT_POOL: Self = Self(1 << 1);
    /// `LocalVariableTable` and `LocalVariableTypeTable`.
    pub const LOCALS: Self = Self(1 << 2);
    pub const STACK_MAP: Self = Self(1 << 3);
    pub const LINE_NUMBERS: Self = Self(1 << 4);
    /// Everything else: annotations, `BootstrapMethods`, `NestMembers`, `LoadableDescriptors`.
    pub const ATTRIBUTES: Self = Self(1 << 5);

    pub const ALL: Self = Self(0b11_1111);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Default for ViewFlags {
    fn default() -> Self {
        Self::METHODS
    }
}

/// How the caller wants the dump produced and cut into pages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ViewOptions {
    pub flags: ViewFlags,
    /// Zero-based page to render.
    pub page: u32,
    /// Lines per page. Pagination exists because Telegram rejects a photo whose width plus
    /// height exceeds 10000, which a long dump reaches quickly.
    pub page_rows: u32,
}

impl ViewOptions {
    pub const DEFAULT_PAGE_ROWS: u32 = 90;
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            flags: ViewFlags::default(),
            page: 0,
            page_rows: Self::DEFAULT_PAGE_ROWS,
        }
    }
}
