use crate::document::DocumentBuilder;
use crate::error::Result;
use crate::stats::Stats;
use crate::view::{InputKind, Platform, ViewOptions};

/// Turns one compiled unit into tokens.
///
/// The whole point of this trait is that it is the only thing that knows about a bytecode
/// format. Frontends append into a caller-supplied [`DocumentBuilder`], so several units —
/// the many class files one Kotlin file compiles to — land in a single document without an
/// intermediate merge step.
pub trait Frontend {
    fn platform(&self) -> Platform;

    /// Input kinds this frontend accepts. A JVM frontend handles both raw class bytes and
    /// pasted `javap` output; a CIL frontend may start with only one.
    fn accepts(&self, kind: InputKind) -> bool;

    /// A short label for the unit, used in captions and page headers.
    fn unit_name(&self, input: &[u8]) -> Option<String>;

    fn emit(
        &self,
        input: &[u8],
        kind: InputKind,
        options: &ViewOptions,
        out: &mut DocumentBuilder,
    ) -> Result<Stats>;
}
