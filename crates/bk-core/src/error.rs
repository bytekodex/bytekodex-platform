use core::fmt;

pub type Result<T> = core::result::Result<T, Error>;

/// Every failure the platform can report.
///
/// The discriminants are part of the C ABI and are frozen: `bk-ffi` returns them directly,
/// so a value may be added but never renumbered. Zero is reserved for success, which is why
/// this enum starts at -1.
#[repr(i32)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Error {
    /// Caller was built against a different `bk_request` layout.
    AbiMismatch {
        expected: u32,
        got: u32,
    } = -1,
    /// A null pointer, a zero-length input, or a nonsensical option combination.
    InvalidArgument(&'static str) = -2,
    UnsupportedPlatform(u32) = -3,
    UnsupportedInputKind(u32) = -4,
    /// Input is the right shape but does not parse.
    Malformed {
        at: usize,
        what: &'static str,
    } = -5,
    /// Class file major version we refuse to guess at.
    UnsupportedVersion {
        major: u16,
        minor: u16,
    } = -6,
    /// Output buffer was too small; the caller should retry with `required` bytes.
    BufferTooSmall {
        required: usize,
    } = -7,
    /// Rendered page would exceed what the delivery channel accepts.
    ImageTooLarge {
        width: u32,
        height: u32,
    } = -8,
    EncodeFailure = -9,
    /// No usable font, or the font has no glyph metrics we can lay out with.
    FontFailure(&'static str) = -10,
    PageOutOfRange {
        requested: u32,
        total: u32,
    } = -11,
    /// A panic crossed a boundary and was caught. Always a bug in the platform.
    Internal = -99,
}

impl Error {
    pub fn code(&self) -> i32 {
        // Reading the discriminant of a `#[repr(i32)]` enum with fields needs a pointer read;
        // matching is uglier but keeps the crate free of `unsafe`.
        match self {
            Error::AbiMismatch { .. } => -1,
            Error::InvalidArgument(_) => -2,
            Error::UnsupportedPlatform(_) => -3,
            Error::UnsupportedInputKind(_) => -4,
            Error::Malformed { .. } => -5,
            Error::UnsupportedVersion { .. } => -6,
            Error::BufferTooSmall { .. } => -7,
            Error::ImageTooLarge { .. } => -8,
            Error::EncodeFailure => -9,
            Error::FontFailure(_) => -10,
            Error::PageOutOfRange { .. } => -11,
            Error::Internal => -99,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AbiMismatch { expected, got } => {
                write!(
                    f,
                    "abi version mismatch: library speaks {expected}, caller sent {got}"
                )
            }
            Error::InvalidArgument(what) => write!(f, "invalid argument: {what}"),
            Error::UnsupportedPlatform(value) => write!(f, "unknown platform id {value}"),
            Error::UnsupportedInputKind(value) => write!(f, "unknown input kind {value}"),
            Error::Malformed { at, what } => write!(f, "malformed input at byte {at}: {what}"),
            Error::UnsupportedVersion { major, minor } => {
                write!(f, "unsupported class file version {major}.{minor}")
            }
            Error::BufferTooSmall { required } => {
                write!(f, "output buffer too small, need {required} bytes")
            }
            Error::ImageTooLarge { width, height } => {
                write!(
                    f,
                    "rendered page is {width}x{height}, which exceeds the delivery limit"
                )
            }
            Error::EncodeFailure => write!(f, "image encoding failed"),
            Error::FontFailure(what) => write!(f, "font problem: {what}"),
            Error::PageOutOfRange { requested, total } => {
                write!(f, "page {requested} requested but only {total} exist")
            }
            Error::Internal => write!(f, "internal error"),
        }
    }
}

impl core::error::Error for Error {}
