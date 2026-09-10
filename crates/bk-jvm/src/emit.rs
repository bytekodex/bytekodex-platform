use bk_core::{DocumentBuilder, MethodStat, Result, Stats, TokenKind, ViewFlags, ViewOptions};

use crate::class::{ClassFile, Code, Member, parse_local_variable_table};
use crate::flags::{FlagContext, declaration_keywords, decode};
use crate::opcodes::{Operands, array_type_name, instruction_len, lookup, pad_to_u32, read_i32};
use crate::pool::{Constant, ConstantPool, method_handle_kind};

/// Column the mnemonic starts at, matching `javap`'s layout closely enough to be familiar.
const MNEMONIC_COLUMN: u32 = 15;
/// Column operand comments start at.
const COMMENT_COLUMN: u32 = 48;

/// Moves to the comment column, or leaves a single space when the line already ran past it.
/// Without the minimum gap a long descriptor would sit flush against the `//`.
fn gap_to_comment(out: &mut DocumentBuilder) {
    if out.current_column() >= COMMENT_COLUMN {
        out.pad(1);
    } else {
        out.pad_to(COMMENT_COLUMN);
    }
}

/// Renders one class file into the shared document.
pub fn emit_class(
    class: &ClassFile<'_>,
    options: &ViewOptions,
    out: &mut DocumentBuilder,
) -> Result<Stats> {
    let mut stats = Stats {
        classes: 1,
        ..Stats::default()
    };

    emit_header(class, out);
    if options.flags.contains(ViewFlags::CONSTANT_POOL) {
        emit_constant_pool(&class.pool, out);
    }
    emit_fields(class, &mut stats, out);

    for method in &class.methods {
        emit_method(class, method, options, &mut stats, out)?;
    }

    out.push(TokenKind::Plain, "}");
    out.newline();

    bk_core::stats::record_opcodes(stats.opcodes_total);
    Ok(stats)
}

fn emit_header(class: &ClassFile<'_>, out: &mut DocumentBuilder) {
    let preview = class.is_preview();

    out.push(TokenKind::Comment, "// class version ");
    out.push_fmt(
        TokenKind::Comment,
        format_args!("{}.{}", class.major, class.minor),
    );
    if let Some(release) = crate::class::jdk_release(class.major) {
        out.push_fmt(TokenKind::Comment, format_args!(" (Java {release}"));
        out.push(TokenKind::Comment, if preview { ", preview)" } else { ")" });
    }
    if class.is_newer_than_known() {
        out.push(
            TokenKind::Comment,
            " — newer than this build knows, unknown attributes shown raw",
        );
    }
    out.newline();

    out.push(TokenKind::Comment, "// flags: ");
    let flag_names = decode(FlagContext::Class, class.flags, preview);
    for (i, name) in flag_names.iter().enumerate() {
        if i > 0 {
            out.push(TokenKind::Comment, ", ");
        }
        out.push(TokenKind::AccessFlag, name);
    }
    out.newline();

    for keyword in declaration_keywords(FlagContext::Class, class.flags) {
        out.push(TokenKind::Keyword, keyword);
        out.push(TokenKind::Plain, " ");
    }
    out.push(
        TokenKind::Keyword,
        if class.flags & 0x0200 != 0 {
            "interface "
        } else {
            "class "
        },
    );
    out.push(
        TokenKind::TypeName,
        class.name().as_deref().unwrap_or("<unnamed>"),
    );

    // `extends java/lang/Object` is noise, and an interface's implicit Object superclass would be
    // actively misleading.
    if class.super_class != 0
        && let Some(name) = class.pool.class_name(class.super_class)
        && name != "java/lang/Object"
    {
        out.push(TokenKind::Keyword, " extends ");
        out.push(TokenKind::TypeName, &name);
    }

    if !class.interfaces.is_empty() {
        out.push(TokenKind::Keyword, " implements ");
        for (i, &index) in class.interfaces.iter().enumerate() {
            if i > 0 {
                out.push(TokenKind::Plain, ", ");
            }
            out.push(
                TokenKind::TypeName,
                class.pool.class_name(index).as_deref().unwrap_or("?"),
            );
        }
    }

    out.push(TokenKind::Plain, " {");
    out.newline();
}

fn emit_constant_pool(pool: &ConstantPool<'_>, out: &mut DocumentBuilder) {
    out.push(TokenKind::Comment, "  Constant pool:");
    out.newline();

    for (index, constant) in pool.iter() {
        out.pad(4);
        out.push_fmt(TokenKind::ConstPoolIndex, format_args!("#{index}"));
        out.pad_to(12);
        out.push(TokenKind::ConstPoolTag, constant.tag_name());
        out.pad_to(34);

        match constant {
            Constant::Utf8(bytes) => {
                out.push(TokenKind::StringLiteral, &String::from_utf8_lossy(bytes))
            }
            Constant::Integer(v) => out.push_fmt(TokenKind::Number, format_args!("{v}")),
            Constant::Float(v) => out.push_fmt(TokenKind::Number, format_args!("{v}f")),
            Constant::Long(v) => out.push_fmt(TokenKind::Number, format_args!("{v}L")),
            Constant::Double(v) => out.push_fmt(TokenKind::Number, format_args!("{v}d")),
            Constant::Class { name } => reference(out, name, pool),
            Constant::String { value } => reference(out, value, pool),
            Constant::FieldRef {
                class,
                name_and_type,
            }
            | Constant::MethodRef {
                class,
                name_and_type,
            }
            | Constant::InterfaceMethodRef {
                class,
                name_and_type,
            } => {
                out.push_fmt(
                    TokenKind::ConstPoolIndex,
                    format_args!("#{class}.#{name_and_type}"),
                );
                gap_to_comment(out);
                out.push(TokenKind::Comment, "// ");
                out.push(
                    TokenKind::TypeName,
                    pool.class_name(class).as_deref().unwrap_or("?"),
                );
                if let Some((name, descriptor)) = pool.name_and_type(name_and_type) {
                    out.push(TokenKind::Plain, ".");
                    out.push(TokenKind::Plain, &name);
                    out.push(TokenKind::Plain, ":");
                    out.push(TokenKind::Descriptor, &descriptor);
                }
            }
            Constant::NameAndType { name, descriptor } => {
                out.push_fmt(
                    TokenKind::ConstPoolIndex,
                    format_args!("#{name}:#{descriptor}"),
                );
                gap_to_comment(out);
                out.push(TokenKind::Comment, "// ");
                out.push(TokenKind::Plain, pool.utf8(name).as_deref().unwrap_or("?"));
                out.push(TokenKind::Plain, ":");
                out.push(
                    TokenKind::Descriptor,
                    pool.utf8(descriptor).as_deref().unwrap_or("?"),
                );
            }
            Constant::MethodHandle { kind, reference: r } => {
                out.push(TokenKind::MethodHandleRef, method_handle_kind(kind));
                out.push(TokenKind::Plain, " ");
                out.push_fmt(TokenKind::ConstPoolIndex, format_args!("#{r}"));
            }
            Constant::MethodType { descriptor } => out.push(
                TokenKind::Descriptor,
                pool.utf8(descriptor).as_deref().unwrap_or("?"),
            ),
            Constant::Dynamic {
                bootstrap,
                name_and_type,
            }
            | Constant::InvokeDynamic {
                bootstrap,
                name_and_type,
            } => {
                out.push_fmt(
                    TokenKind::ConstPoolIndex,
                    format_args!("#{bootstrap}:#{name_and_type}"),
                );
            }
            Constant::Module { name } | Constant::Package { name } => reference(out, name, pool),
            Constant::Unusable => {}
        }
        out.newline();
    }
    out.newline();
}

fn reference(out: &mut DocumentBuilder, index: u16, pool: &ConstantPool<'_>) {
    out.push_fmt(TokenKind::ConstPoolIndex, format_args!("#{index}"));
    if let Some(text) = pool.utf8(index) {
        gap_to_comment(out);
        out.push(TokenKind::Comment, "// ");
        out.push(TokenKind::TypeName, &text);
    }
}

fn emit_fields(class: &ClassFile<'_>, stats: &mut Stats, out: &mut DocumentBuilder) {
    for field in &class.fields {
        stats.fields += 1;
        out.pad(2);
        for keyword in declaration_keywords(FlagContext::Field, field.flags) {
            out.push(TokenKind::Keyword, keyword);
            out.push(TokenKind::Plain, " ");
        }
        out.push(
            TokenKind::Descriptor,
            class.pool.utf8(field.descriptor).as_deref().unwrap_or("?"),
        );
        out.push(TokenKind::Plain, " ");
        out.push(
            TokenKind::Plain,
            class.pool.utf8(field.name).as_deref().unwrap_or("?"),
        );
        out.push(TokenKind::Plain, ";");

        let flag_names = decode(FlagContext::Field, field.flags, class.is_preview());
        if !flag_names.is_empty() {
            gap_to_comment(out);
            out.push(TokenKind::Comment, "// ");
            for (i, name) in flag_names.iter().enumerate() {
                if i > 0 {
                    out.push(TokenKind::Comment, ", ");
                }
                out.push(TokenKind::AccessFlag, name);
            }
        }
        out.newline();
    }
    if !class.fields.is_empty() {
        out.newline();
    }
}

fn emit_method(
    class: &ClassFile<'_>,
    method: &Member<'_>,
    options: &ViewOptions,
    stats: &mut Stats,
    out: &mut DocumentBuilder,
) -> Result<()> {
    stats.methods += 1;

    let name = class
        .pool
        .utf8(method.name)
        .unwrap_or(std::borrow::Cow::Borrowed("?"));
    let descriptor = class
        .pool
        .utf8(method.descriptor)
        .unwrap_or(std::borrow::Cow::Borrowed("?"));

    out.pad(2);
    for keyword in declaration_keywords(FlagContext::Method, method.flags) {
        out.push(TokenKind::Keyword, keyword);
        out.push(TokenKind::Plain, " ");
    }
    out.push(TokenKind::Plain, &name);
    // No trailing semicolon: an object return type already ends the descriptor with one, and
    // `()Ljava/lang/String;;` reads as a typo.
    out.push(TokenKind::Descriptor, &descriptor);

    let flag_names = decode(FlagContext::Method, method.flags, class.is_preview());
    if !flag_names.is_empty() {
        gap_to_comment(out);
        out.push(TokenKind::Comment, "// ");
        for (i, flag) in flag_names.iter().enumerate() {
            if i > 0 {
                out.push(TokenKind::Comment, ", ");
            }
            out.push(TokenKind::AccessFlag, flag);
        }
    }
    out.newline();

    let Some(code_attribute) = method.attribute(&class.pool, "Code") else {
        // Abstract and native methods have no `Code` attribute at all.
        out.newline();
        return Ok(());
    };
    let code = Code::parse(code_attribute.data)?;

    out.pad(4);
    out.push(TokenKind::AttributeName, "Code:");
    gap_to_comment(out);
    out.push_fmt(
        TokenKind::Comment,
        format_args!(
            "// stack={}, locals={}, code_len={}",
            code.max_stack,
            code.max_locals,
            code.bytes.len()
        ),
    );
    out.newline();

    let opcodes = emit_instructions(&class.pool, &code, out)?;
    stats.opcodes_total += u64::from(opcodes);
    stats.per_method.push(MethodStat {
        name: name.to_string(),
        descriptor: descriptor.to_string(),
        opcodes,
        code_len: code.bytes.len() as u32,
    });

    if !code.exceptions.is_empty() {
        emit_exception_table(class, &code, out);
    }
    if options.flags.contains(ViewFlags::LOCALS) {
        emit_locals(class, &code, out);
    }

    out.newline();
    Ok(())
}

/// Walks the instruction array, printing each instruction and returning how many there were.
///
/// The count is a plain local rather than an atomic on purpose: this loop runs once per
/// instruction, and an atomic read-modify-write here would dominate the cost of parsing.
/// Aggregation happens once, in `Stats::merge`.
fn emit_instructions(
    pool: &ConstantPool<'_>,
    code: &Code<'_>,
    out: &mut DocumentBuilder,
) -> Result<u32> {
    let bytes = code.bytes;
    let mut pc = 0usize;
    let mut count = 0u32;

    while pc < bytes.len() {
        let Some(info) = lookup(bytes[pc]) else {
            out.pad(6);
            out.push_fmt(TokenKind::InstructionOffset, format_args!("{pc:>4}: "));
            out.push_fmt(
                TokenKind::Malformed,
                format_args!("unknown opcode 0x{:02X}", bytes[pc]),
            );
            out.newline();
            // An unassigned opcode makes every following offset a guess, so stop rather than
            // print a plausible-looking lie.
            break;
        };
        let Some(len) = instruction_len(bytes, pc) else {
            out.pad(6);
            out.push_fmt(TokenKind::InstructionOffset, format_args!("{pc:>4}: "));
            out.push(TokenKind::Malformed, "truncated instruction");
            out.newline();
            break;
        };

        count += 1;
        out.pad(6);
        out.push_fmt(TokenKind::InstructionOffset, format_args!("{pc:>4}:"));
        out.pad_to(MNEMONIC_COLUMN);
        out.push(TokenKind::Instruction, info.name);

        emit_operands(pool, bytes, pc, info.operands, out);
        out.newline();

        pc += len;
    }

    Ok(count)
}

fn emit_operands(
    pool: &ConstantPool<'_>,
    bytes: &[u8],
    pc: usize,
    operands: Operands,
    out: &mut DocumentBuilder,
) {
    let u1 = |offset: usize| bytes.get(pc + offset).copied().unwrap_or(0);
    let u2 = |offset: usize| u16::from_be_bytes([u1(offset), u1(offset + 1)]);

    match operands {
        Operands::None => {}
        Operands::LocalU1 => {
            out.push(TokenKind::Plain, " ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u1(1)));
        }
        Operands::ImmI1 => {
            out.push(TokenKind::Plain, " ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u1(1) as i8));
        }
        Operands::ImmI2 => {
            out.push(TokenKind::Plain, " ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u2(1) as i16));
        }
        Operands::ArrayType => {
            out.push(TokenKind::Plain, " ");
            out.push(TokenKind::Primitive, array_type_name(u1(1)));
        }
        Operands::LocalConst => {
            out.push(TokenKind::Plain, " ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u1(1)));
            out.push(TokenKind::Plain, ", ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u1(2) as i8));
        }
        Operands::Cp1 => emit_pool_operand(pool, u1(1) as u16, out),
        Operands::Cp2 | Operands::InvokeDynamic => emit_pool_operand(pool, u2(1), out),
        Operands::InvokeInterface => {
            emit_pool_operand(pool, u2(1), out);
            out.push(TokenKind::Plain, ", ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u1(3)));
        }
        Operands::MultiANewArray => {
            emit_pool_operand(pool, u2(1), out);
            out.push(TokenKind::Plain, ", ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u1(3)));
        }
        Operands::Branch2 => {
            let target = pc as i64 + i64::from(u2(1) as i16);
            out.push(TokenKind::Plain, " ");
            out.push_fmt(TokenKind::Label, format_args!("{target}"));
        }
        Operands::Branch4 => {
            let offset = read_i32(bytes, pc + 1).unwrap_or(0);
            out.push(TokenKind::Plain, " ");
            out.push_fmt(
                TokenKind::Label,
                format_args!("{}", pc as i64 + i64::from(offset)),
            );
        }
        Operands::Wide => {
            let widened = u1(1);
            out.push(TokenKind::Plain, " ");
            out.push(
                TokenKind::Instruction,
                lookup(widened).map(|i| i.name).unwrap_or("?"),
            );
            out.push(TokenKind::Plain, " ");
            out.push_fmt(TokenKind::Number, format_args!("{}", u2(2)));
            if widened == 0x84 {
                out.push(TokenKind::Plain, ", ");
                out.push_fmt(TokenKind::Number, format_args!("{}", u2(4) as i16));
            }
        }
        Operands::TableSwitch => {
            let base = pad_to_u32(pc + 1);
            let default = read_i32(bytes, base).unwrap_or(0);
            let low = read_i32(bytes, base + 4).unwrap_or(0);
            let high = read_i32(bytes, base + 8).unwrap_or(0);
            out.push(TokenKind::Plain, " { ");
            out.push_fmt(TokenKind::Number, format_args!("{low}"));
            out.push(TokenKind::Plain, " to ");
            out.push_fmt(TokenKind::Number, format_args!("{high}"));
            out.push(TokenKind::Plain, ", default ");
            out.push_fmt(
                TokenKind::Label,
                format_args!("{}", pc as i64 + i64::from(default)),
            );
            out.push(TokenKind::Plain, " }");
        }
        Operands::LookupSwitch => {
            let base = pad_to_u32(pc + 1);
            let default = read_i32(bytes, base).unwrap_or(0);
            let pairs = read_i32(bytes, base + 4).unwrap_or(0);
            out.push(TokenKind::Plain, " { ");
            out.push_fmt(TokenKind::Number, format_args!("{pairs}"));
            out.push(TokenKind::Plain, " cases, default ");
            out.push_fmt(
                TokenKind::Label,
                format_args!("{}", pc as i64 + i64::from(default)),
            );
            out.push(TokenKind::Plain, " }");
        }
    }
}

/// Prints `#index` and, past the comment column, what it resolves to.
fn emit_pool_operand(pool: &ConstantPool<'_>, index: u16, out: &mut DocumentBuilder) {
    out.push(TokenKind::Plain, " ");
    out.push_fmt(TokenKind::ConstPoolIndex, format_args!("#{index}"));

    let Some(constant) = pool.get(index) else {
        return;
    };
    gap_to_comment(out);
    out.push(TokenKind::Comment, "// ");

    match constant {
        Constant::Class { name } => out.push(
            TokenKind::TypeName,
            pool.utf8(name).as_deref().unwrap_or("?"),
        ),
        Constant::String { value } => {
            out.push(TokenKind::StringLiteral, "\"");
            out.push(
                TokenKind::StringLiteral,
                pool.utf8(value).as_deref().unwrap_or("?"),
            );
            out.push(TokenKind::StringLiteral, "\"");
        }
        Constant::Integer(v) => out.push_fmt(TokenKind::Number, format_args!("{v}")),
        Constant::Long(v) => out.push_fmt(TokenKind::Number, format_args!("{v}L")),
        Constant::Float(v) => out.push_fmt(TokenKind::Number, format_args!("{v}f")),
        Constant::Double(v) => out.push_fmt(TokenKind::Number, format_args!("{v}d")),
        Constant::FieldRef {
            class,
            name_and_type,
        }
        | Constant::MethodRef {
            class,
            name_and_type,
        }
        | Constant::InterfaceMethodRef {
            class,
            name_and_type,
        } => {
            out.push(
                TokenKind::TypeName,
                pool.class_name(class).as_deref().unwrap_or("?"),
            );
            out.push(TokenKind::Plain, ".");
            if let Some((name, descriptor)) = pool.name_and_type(name_and_type) {
                out.push(TokenKind::Plain, &name);
                out.push(TokenKind::Plain, ":");
                out.push(TokenKind::Descriptor, &descriptor);
            }
        }
        Constant::Dynamic { name_and_type, .. } | Constant::InvokeDynamic { name_and_type, .. } => {
            if let Some((name, descriptor)) = pool.name_and_type(name_and_type) {
                out.push(TokenKind::Plain, &name);
                out.push(TokenKind::Descriptor, &descriptor);
            }
        }
        other => out.push(TokenKind::ConstPoolTag, other.tag_name()),
    }
}

fn emit_exception_table(class: &ClassFile<'_>, code: &Code<'_>, out: &mut DocumentBuilder) {
    out.pad(4);
    out.push(TokenKind::AttributeName, "Exception table:");
    out.newline();
    for entry in &code.exceptions {
        out.pad(6);
        out.push_fmt(
            TokenKind::InstructionOffset,
            format_args!("{:>4} .. {:>4}", entry.start_pc, entry.end_pc),
        );
        out.push(TokenKind::Plain, " -> ");
        out.push_fmt(TokenKind::Label, format_args!("{}", entry.handler_pc));
        out.push(TokenKind::Plain, " ");
        if entry.catch_type == 0 {
            out.push(TokenKind::Keyword, "any");
        } else {
            out.push(
                TokenKind::TypeName,
                class
                    .pool
                    .class_name(entry.catch_type)
                    .as_deref()
                    .unwrap_or("?"),
            );
        }
        out.newline();
    }
}

fn emit_locals(class: &ClassFile<'_>, code: &Code<'_>, out: &mut DocumentBuilder) {
    let table = code.attributes.iter().find(|a| {
        class
            .pool
            .utf8(a.name)
            .is_some_and(|n| n == "LocalVariableTable")
    });
    let Some(attribute) = table else {
        return;
    };
    let Ok(locals) = parse_local_variable_table(attribute.data) else {
        return;
    };

    out.pad(4);
    out.push(TokenKind::AttributeName, "LocalVariableTable:");
    out.newline();
    for local in &locals {
        out.pad(6);
        out.push_fmt(
            TokenKind::InstructionOffset,
            format_args!("slot {:>2}", local.slot),
        );
        out.push(TokenKind::Plain, "  ");
        out.push(
            TokenKind::LocalName,
            class.pool.utf8(local.name).as_deref().unwrap_or("?"),
        );
        out.push(TokenKind::Plain, " ");
        out.push(
            TokenKind::Descriptor,
            class.pool.utf8(local.descriptor).as_deref().unwrap_or("?"),
        );
        gap_to_comment(out);
        out.push_fmt(
            TokenKind::Comment,
            format_args!(
                "// live [{}, {})",
                local.start_pc,
                local.start_pc + local.length
            ),
        );
        out.newline();
    }
}
