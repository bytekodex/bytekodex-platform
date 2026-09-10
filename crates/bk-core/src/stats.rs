use core::sync::atomic::{AtomicU64, Ordering};

/// Opcode count for one method.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MethodStat {
    pub name: String,
    pub descriptor: String,
    pub opcodes: u32,
    /// Bytes of the `Code` attribute's instruction array, which is not the same number —
    /// most instructions carry operands.
    pub code_len: u32,
}

/// Counts gathered while parsing. Cheap to produce because the parser has to walk every
/// instruction anyway to find the next one.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Stats {
    pub classes: u32,
    pub fields: u32,
    pub methods: u32,
    pub opcodes_total: u64,
    pub per_method: Vec<MethodStat>,
}

impl Stats {
    /// Folds a worker's result into an accumulator.
    ///
    /// This is the reduction step for `rayon`, and it is why per-method counting uses a plain
    /// `u32` local in the parser rather than an atomic. An atomic increment costs a
    /// read-modify-write on every single instruction; over a hundred thousand opcodes that
    /// becomes the dominant cost, and it buys nothing since each worker owns its own class.
    pub fn merge(mut self, other: Self) -> Self {
        self.classes += other.classes;
        self.fields += other.fields;
        self.methods += other.methods;
        self.opcodes_total += other.opcodes_total;
        self.per_method.extend(other.per_method);
        self
    }
}

/// Opcodes this process has decoded since it started.
///
/// The one place an atomic is genuinely the right tool: a single counter, updated once per
/// request rather than once per instruction, read only by whoever scrapes metrics. Nothing
/// orders against it, so `Relaxed` is correct rather than merely cheap.
static TOTAL_OPCODES: AtomicU64 = AtomicU64::new(0);

pub fn record_opcodes(count: u64) {
    TOTAL_OPCODES.fetch_add(count, Ordering::Relaxed);
}

pub fn total_opcodes() -> u64 {
    TOTAL_OPCODES.load(Ordering::Relaxed)
}
