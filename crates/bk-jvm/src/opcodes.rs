//! The JVM instruction set.
//!
//! The set has been frozen since Java 7 added `invokedynamic` (`0xBA`), the last opcode ever
//! added. `0x00`–`0xC9` are the 202 real instructions; `0xCA`, `0xFE` and `0xFF` are reserved
//! for debuggers and never appear in a class file; `0xCB`–`0xFD` are unused.
//!
//! Project Valhalla did briefly propose `aconst_init` and `withfield` in early-access builds,
//! and those were dropped — the final JEP 401 in JDK 28 adds no instructions at all. Anything
//! naming them is out of date.

/// Shape of the bytes following an opcode. Determines both how to print the instruction and
/// where the next one starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operands {
    None,
    /// Unsigned one-byte local variable index.
    LocalU1,
    /// Signed one-byte immediate.
    ImmI1,
    /// Signed two-byte immediate.
    ImmI2,
    /// One-byte constant pool index, only `ldc`.
    Cp1,
    /// Two-byte constant pool index.
    Cp2,
    /// Signed two-byte branch offset relative to the opcode.
    Branch2,
    /// Signed four-byte branch offset.
    Branch4,
    /// Local index plus signed one-byte delta, only `iinc`.
    LocalConst,
    /// `newarray` element type code.
    ArrayType,
    /// Two-byte pool index plus dimension count.
    MultiANewArray,
    /// Two-byte pool index, an argument count, and one byte that must be zero.
    InvokeInterface,
    /// Two-byte pool index and two bytes that must be zero.
    InvokeDynamic,
    /// Widens the following instruction's index operand to two bytes.
    Wide,
    /// Padded to a four-byte boundary, then a default offset, low, high, and a jump table.
    TableSwitch,
    /// Padded to a four-byte boundary, then a default offset, a pair count, and match pairs.
    LookupSwitch,
}

impl Operands {
    /// Operand bytes following the opcode, or `None` when the length depends on the
    /// instruction's position or contents.
    pub const fn fixed_len(self) -> Option<usize> {
        match self {
            Operands::None => Some(0),
            Operands::LocalU1 | Operands::Cp1 | Operands::ImmI1 | Operands::ArrayType => Some(1),
            Operands::ImmI2 | Operands::Cp2 | Operands::Branch2 | Operands::LocalConst => Some(2),
            Operands::MultiANewArray => Some(3),
            Operands::Branch4 | Operands::InvokeInterface | Operands::InvokeDynamic => Some(4),
            Operands::Wide | Operands::TableSwitch | Operands::LookupSwitch => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OpInfo {
    pub name: &'static str,
    pub operands: Operands,
    /// Reserved opcodes are legal to name but must never appear in a class file.
    pub reserved: bool,
}

const fn op(name: &'static str, operands: Operands) -> Option<OpInfo> {
    Some(OpInfo {
        name,
        operands,
        reserved: false,
    })
}

const fn reserved(name: &'static str) -> Option<OpInfo> {
    Some(OpInfo {
        name,
        operands: Operands::None,
        reserved: true,
    })
}

/// Lookup table indexed by opcode byte.
pub static OPCODES: [Option<OpInfo>; 256] = {
    use Operands::*;

    // `Option::None` spelled out because the glob import above shadows the prelude's `None`.
    let mut t: [Option<OpInfo>; 256] = [Option::None; 256];

    t[0x00] = op("nop", None);
    t[0x01] = op("aconst_null", None);
    t[0x02] = op("iconst_m1", None);
    t[0x03] = op("iconst_0", None);
    t[0x04] = op("iconst_1", None);
    t[0x05] = op("iconst_2", None);
    t[0x06] = op("iconst_3", None);
    t[0x07] = op("iconst_4", None);
    t[0x08] = op("iconst_5", None);
    t[0x09] = op("lconst_0", None);
    t[0x0A] = op("lconst_1", None);
    t[0x0B] = op("fconst_0", None);
    t[0x0C] = op("fconst_1", None);
    t[0x0D] = op("fconst_2", None);
    t[0x0E] = op("dconst_0", None);
    t[0x0F] = op("dconst_1", None);
    t[0x10] = op("bipush", ImmI1);
    t[0x11] = op("sipush", ImmI2);
    t[0x12] = op("ldc", Cp1);
    t[0x13] = op("ldc_w", Cp2);
    t[0x14] = op("ldc2_w", Cp2);
    t[0x15] = op("iload", LocalU1);
    t[0x16] = op("lload", LocalU1);
    t[0x17] = op("fload", LocalU1);
    t[0x18] = op("dload", LocalU1);
    t[0x19] = op("aload", LocalU1);
    t[0x1A] = op("iload_0", None);
    t[0x1B] = op("iload_1", None);
    t[0x1C] = op("iload_2", None);
    t[0x1D] = op("iload_3", None);
    t[0x1E] = op("lload_0", None);
    t[0x1F] = op("lload_1", None);
    t[0x20] = op("lload_2", None);
    t[0x21] = op("lload_3", None);
    t[0x22] = op("fload_0", None);
    t[0x23] = op("fload_1", None);
    t[0x24] = op("fload_2", None);
    t[0x25] = op("fload_3", None);
    t[0x26] = op("dload_0", None);
    t[0x27] = op("dload_1", None);
    t[0x28] = op("dload_2", None);
    t[0x29] = op("dload_3", None);
    t[0x2A] = op("aload_0", None);
    t[0x2B] = op("aload_1", None);
    t[0x2C] = op("aload_2", None);
    t[0x2D] = op("aload_3", None);
    t[0x2E] = op("iaload", None);
    t[0x2F] = op("laload", None);
    t[0x30] = op("faload", None);
    t[0x31] = op("daload", None);
    t[0x32] = op("aaload", None);
    t[0x33] = op("baload", None);
    t[0x34] = op("caload", None);
    t[0x35] = op("saload", None);
    t[0x36] = op("istore", LocalU1);
    t[0x37] = op("lstore", LocalU1);
    t[0x38] = op("fstore", LocalU1);
    t[0x39] = op("dstore", LocalU1);
    t[0x3A] = op("astore", LocalU1);
    t[0x3B] = op("istore_0", None);
    t[0x3C] = op("istore_1", None);
    t[0x3D] = op("istore_2", None);
    t[0x3E] = op("istore_3", None);
    t[0x3F] = op("lstore_0", None);
    t[0x40] = op("lstore_1", None);
    t[0x41] = op("lstore_2", None);
    t[0x42] = op("lstore_3", None);
    t[0x43] = op("fstore_0", None);
    t[0x44] = op("fstore_1", None);
    t[0x45] = op("fstore_2", None);
    t[0x46] = op("fstore_3", None);
    t[0x47] = op("dstore_0", None);
    t[0x48] = op("dstore_1", None);
    t[0x49] = op("dstore_2", None);
    t[0x4A] = op("dstore_3", None);
    t[0x4B] = op("astore_0", None);
    t[0x4C] = op("astore_1", None);
    t[0x4D] = op("astore_2", None);
    t[0x4E] = op("astore_3", None);
    t[0x4F] = op("iastore", None);
    t[0x50] = op("lastore", None);
    t[0x51] = op("fastore", None);
    t[0x52] = op("dastore", None);
    t[0x53] = op("aastore", None);
    t[0x54] = op("bastore", None);
    t[0x55] = op("castore", None);
    t[0x56] = op("sastore", None);
    t[0x57] = op("pop", None);
    t[0x58] = op("pop2", None);
    t[0x59] = op("dup", None);
    t[0x5A] = op("dup_x1", None);
    t[0x5B] = op("dup_x2", None);
    t[0x5C] = op("dup2", None);
    t[0x5D] = op("dup2_x1", None);
    t[0x5E] = op("dup2_x2", None);
    t[0x5F] = op("swap", None);
    t[0x60] = op("iadd", None);
    t[0x61] = op("ladd", None);
    t[0x62] = op("fadd", None);
    t[0x63] = op("dadd", None);
    t[0x64] = op("isub", None);
    t[0x65] = op("lsub", None);
    t[0x66] = op("fsub", None);
    t[0x67] = op("dsub", None);
    t[0x68] = op("imul", None);
    t[0x69] = op("lmul", None);
    t[0x6A] = op("fmul", None);
    t[0x6B] = op("dmul", None);
    t[0x6C] = op("idiv", None);
    t[0x6D] = op("ldiv", None);
    t[0x6E] = op("fdiv", None);
    t[0x6F] = op("ddiv", None);
    t[0x70] = op("irem", None);
    t[0x71] = op("lrem", None);
    t[0x72] = op("frem", None);
    t[0x73] = op("drem", None);
    t[0x74] = op("ineg", None);
    t[0x75] = op("lneg", None);
    t[0x76] = op("fneg", None);
    t[0x77] = op("dneg", None);
    t[0x78] = op("ishl", None);
    t[0x79] = op("lshl", None);
    t[0x7A] = op("ishr", None);
    t[0x7B] = op("lshr", None);
    t[0x7C] = op("iushr", None);
    t[0x7D] = op("lushr", None);
    t[0x7E] = op("iand", None);
    t[0x7F] = op("land", None);
    t[0x80] = op("ior", None);
    t[0x81] = op("lor", None);
    t[0x82] = op("ixor", None);
    t[0x83] = op("lxor", None);
    t[0x84] = op("iinc", LocalConst);
    t[0x85] = op("i2l", None);
    t[0x86] = op("i2f", None);
    t[0x87] = op("i2d", None);
    t[0x88] = op("l2i", None);
    t[0x89] = op("l2f", None);
    t[0x8A] = op("l2d", None);
    t[0x8B] = op("f2i", None);
    t[0x8C] = op("f2l", None);
    t[0x8D] = op("f2d", None);
    t[0x8E] = op("d2i", None);
    t[0x8F] = op("d2l", None);
    t[0x90] = op("d2f", None);
    t[0x91] = op("i2b", None);
    t[0x92] = op("i2c", None);
    t[0x93] = op("i2s", None);
    t[0x94] = op("lcmp", None);
    t[0x95] = op("fcmpl", None);
    t[0x96] = op("fcmpg", None);
    t[0x97] = op("dcmpl", None);
    t[0x98] = op("dcmpg", None);
    t[0x99] = op("ifeq", Branch2);
    t[0x9A] = op("ifne", Branch2);
    t[0x9B] = op("iflt", Branch2);
    t[0x9C] = op("ifge", Branch2);
    t[0x9D] = op("ifgt", Branch2);
    t[0x9E] = op("ifle", Branch2);
    t[0x9F] = op("if_icmpeq", Branch2);
    t[0xA0] = op("if_icmpne", Branch2);
    t[0xA1] = op("if_icmplt", Branch2);
    t[0xA2] = op("if_icmpge", Branch2);
    t[0xA3] = op("if_icmpgt", Branch2);
    t[0xA4] = op("if_icmple", Branch2);
    t[0xA5] = op("if_acmpeq", Branch2);
    t[0xA6] = op("if_acmpne", Branch2);
    t[0xA7] = op("goto", Branch2);
    t[0xA8] = op("jsr", Branch2);
    t[0xA9] = op("ret", LocalU1);
    t[0xAA] = op("tableswitch", TableSwitch);
    t[0xAB] = op("lookupswitch", LookupSwitch);
    t[0xAC] = op("ireturn", None);
    t[0xAD] = op("lreturn", None);
    t[0xAE] = op("freturn", None);
    t[0xAF] = op("dreturn", None);
    t[0xB0] = op("areturn", None);
    t[0xB1] = op("return", None);
    t[0xB2] = op("getstatic", Cp2);
    t[0xB3] = op("putstatic", Cp2);
    t[0xB4] = op("getfield", Cp2);
    t[0xB5] = op("putfield", Cp2);
    t[0xB6] = op("invokevirtual", Cp2);
    t[0xB7] = op("invokespecial", Cp2);
    t[0xB8] = op("invokestatic", Cp2);
    t[0xB9] = op("invokeinterface", InvokeInterface);
    t[0xBA] = op("invokedynamic", InvokeDynamic);
    t[0xBB] = op("new", Cp2);
    t[0xBC] = op("newarray", ArrayType);
    t[0xBD] = op("anewarray", Cp2);
    t[0xBE] = op("arraylength", None);
    t[0xBF] = op("athrow", None);
    t[0xC0] = op("checkcast", Cp2);
    t[0xC1] = op("instanceof", Cp2);
    t[0xC2] = op("monitorenter", None);
    t[0xC3] = op("monitorexit", None);
    t[0xC4] = op("wide", Wide);
    t[0xC5] = op("multianewarray", MultiANewArray);
    t[0xC6] = op("ifnull", Branch2);
    t[0xC7] = op("ifnonnull", Branch2);
    t[0xC8] = op("goto_w", Branch4);
    t[0xC9] = op("jsr_w", Branch4);

    t[0xCA] = reserved("breakpoint");
    t[0xFE] = reserved("impdep1");
    t[0xFF] = reserved("impdep2");

    t
};

pub fn lookup(opcode: u8) -> Option<OpInfo> {
    OPCODES[opcode as usize]
}

/// Element type operand of `newarray`.
pub fn array_type_name(code: u8) -> &'static str {
    match code {
        4 => "boolean",
        5 => "char",
        6 => "float",
        7 => "double",
        8 => "byte",
        9 => "short",
        10 => "int",
        11 => "long",
        _ => "?",
    }
}

/// Total size of the instruction at `pc`, opcode byte included.
///
/// Returns `None` when the instruction runs past the end of `code`, which is how a truncated
/// or hand-mangled `Code` attribute is detected.
pub fn instruction_len(code: &[u8], pc: usize) -> Option<usize> {
    let info = lookup(*code.get(pc)?)?;

    if let Some(fixed) = info.operands.fixed_len() {
        let len = 1 + fixed;
        return (pc + len <= code.len()).then_some(len);
    }

    match info.operands {
        // `wide` widens the next instruction's index to two bytes, and `iinc` also widens
        // its constant, which is the only case where the total is six rather than four.
        Operands::Wide => {
            let widened = *code.get(pc + 1)?;
            let len = if widened == 0x84 { 6 } else { 4 };
            (pc + len <= code.len()).then_some(len)
        }
        Operands::TableSwitch => {
            let base = pad_to_u32(pc + 1);
            let low = read_i32(code, base + 4)?;
            let high = read_i32(code, base + 8)?;
            if high < low {
                return None;
            }
            let entries = (high as i64 - low as i64 + 1) as usize;
            let end = base + 12 + entries * 4;
            (end <= code.len()).then(|| end - pc)
        }
        Operands::LookupSwitch => {
            let base = pad_to_u32(pc + 1);
            let pairs = read_i32(code, base + 4)?;
            if pairs < 0 {
                return None;
            }
            let end = base + 8 + pairs as usize * 8;
            (end <= code.len()).then(|| end - pc)
        }
        _ => None,
    }
}

/// Switch operands begin at the next four-byte boundary measured from the start of the
/// method's code array, which is why padding is computed from the absolute offset.
pub const fn pad_to_u32(offset: usize) -> usize {
    (offset + 3) & !3
}

pub fn read_i32(code: &[u8], at: usize) -> Option<i32> {
    let bytes = code.get(at..at + 4)?;
    Some(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_exactly_the_frozen_instruction_set() {
        let real = (0x00..=0xC9).filter(|&i| OPCODES[i].is_some()).count();
        let reserved = OPCODES.iter().flatten().filter(|o| o.reserved).count();
        let unused = (0xCB..=0xFD).filter(|&i| OPCODES[i].is_some()).count();

        assert_eq!(real, 202);
        assert_eq!(reserved, 3);
        assert_eq!(unused, 0);
    }

    #[test]
    fn valhalla_early_access_opcodes_are_absent() {
        for gone in [
            "aconst_init",
            "withfield",
            "if_acmp_null",
            "if_acmp_nonnull",
        ] {
            assert!(
                !OPCODES.iter().flatten().any(|o| o.name == gone),
                "{gone} was dropped from the Valhalla design and must not be in the table"
            );
        }
    }

    #[test]
    fn wide_iinc_is_six_bytes_and_wide_load_is_four() {
        assert_eq!(instruction_len(&[0xC4, 0x84, 0, 1, 0, 1], 0), Some(6));
        assert_eq!(instruction_len(&[0xC4, 0x15, 0, 1], 0), Some(4));
    }

    #[test]
    fn tableswitch_length_accounts_for_padding_and_jump_table() {
        // pc = 0, so operands start at byte 4 after three padding bytes.
        let mut code = vec![0xAA, 0, 0, 0];
        code.extend_from_slice(&0i32.to_be_bytes()); // default
        code.extend_from_slice(&1i32.to_be_bytes()); // low
        code.extend_from_slice(&3i32.to_be_bytes()); // high
        code.extend_from_slice(&[0; 12]); // three targets
        assert_eq!(instruction_len(&code, 0), Some(28));
    }

    #[test]
    fn truncated_instruction_is_rejected() {
        assert_eq!(instruction_len(&[0xB6, 0x00], 0), None);
        assert_eq!(instruction_len(&[0xCB], 0), None);
    }
}
