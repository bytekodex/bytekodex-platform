use bk_core::{Error, Result};

use crate::reader::Reader;

/// One constant pool entry, borrowing its bytes from the class file.
///
/// `Utf8` deliberately keeps the raw bytes rather than a `&str`: the class file format uses
/// modified UTF-8, where a NUL is two bytes and characters outside the BMP are a surrogate
/// pair, so it is not valid Rust UTF-8 in general. Decoding happens only when a value is
/// actually printed.
#[derive(Clone, Copy, Debug)]
pub enum Constant<'a> {
    Utf8(&'a [u8]),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class {
        name: u16,
    },
    String {
        value: u16,
    },
    FieldRef {
        class: u16,
        name_and_type: u16,
    },
    MethodRef {
        class: u16,
        name_and_type: u16,
    },
    InterfaceMethodRef {
        class: u16,
        name_and_type: u16,
    },
    NameAndType {
        name: u16,
        descriptor: u16,
    },
    MethodHandle {
        kind: u8,
        reference: u16,
    },
    MethodType {
        descriptor: u16,
    },
    /// Java 11 and later. A condy constant, resolved by a bootstrap method.
    Dynamic {
        bootstrap: u16,
        name_and_type: u16,
    },
    InvokeDynamic {
        bootstrap: u16,
        name_and_type: u16,
    },
    Module {
        name: u16,
    },
    Package {
        name: u16,
    },
    /// The dead slot that follows a `Long` or `Double`, an artifact of the format treating
    /// eight-byte constants as occupying two indices.
    Unusable,
}

impl Constant<'_> {
    /// Tag name as `javap` prints it.
    pub fn tag_name(&self) -> &'static str {
        match self {
            Constant::Utf8(_) => "Utf8",
            Constant::Integer(_) => "Integer",
            Constant::Float(_) => "Float",
            Constant::Long(_) => "Long",
            Constant::Double(_) => "Double",
            Constant::Class { .. } => "Class",
            Constant::String { .. } => "String",
            Constant::FieldRef { .. } => "Fieldref",
            Constant::MethodRef { .. } => "Methodref",
            Constant::InterfaceMethodRef { .. } => "InterfaceMethodref",
            Constant::NameAndType { .. } => "NameAndType",
            Constant::MethodHandle { .. } => "MethodHandle",
            Constant::MethodType { .. } => "MethodType",
            Constant::Dynamic { .. } => "Dynamic",
            Constant::InvokeDynamic { .. } => "InvokeDynamic",
            Constant::Module { .. } => "Module",
            Constant::Package { .. } => "Package",
            Constant::Unusable => "",
        }
    }
}

/// Reference kind of a `MethodHandle` constant, per the JVMS table in 4.4.8.
pub fn method_handle_kind(kind: u8) -> &'static str {
    match kind {
        1 => "REF_getField",
        2 => "REF_getStatic",
        3 => "REF_putField",
        4 => "REF_putStatic",
        5 => "REF_invokeVirtual",
        6 => "REF_invokeStatic",
        7 => "REF_invokeSpecial",
        8 => "REF_newInvokeSpecial",
        9 => "REF_invokeInterface",
        _ => "REF_unknown",
    }
}

/// The constant pool, indexed the way the format does it: one-based, with a hole after every
/// `Long` and `Double`.
pub struct ConstantPool<'a> {
    entries: Vec<Constant<'a>>,
}

impl<'a> ConstantPool<'a> {
    pub fn parse(reader: &mut Reader<'a>) -> Result<Self> {
        let count = reader.u2()?;
        if count == 0 {
            return Err(Error::Malformed {
                at: reader.pos(),
                what: "constant pool count is zero",
            });
        }

        // Slot zero is never addressable; filling it keeps indexing arithmetic honest.
        let mut entries = Vec::with_capacity(count as usize);
        entries.push(Constant::Unusable);

        let mut index = 1u16;
        while index < count {
            let tag = reader.u1()?;
            let constant = match tag {
                1 => {
                    let len = reader.u2()? as usize;
                    Constant::Utf8(reader.bytes(len)?)
                }
                3 => Constant::Integer(reader.i4()?),
                4 => Constant::Float(f32::from_bits(reader.u4()?)),
                5 => Constant::Long(reader.u8v()? as i64),
                6 => Constant::Double(f64::from_bits(reader.u8v()?)),
                7 => Constant::Class { name: reader.u2()? },
                8 => Constant::String {
                    value: reader.u2()?,
                },
                9 => Constant::FieldRef {
                    class: reader.u2()?,
                    name_and_type: reader.u2()?,
                },
                10 => Constant::MethodRef {
                    class: reader.u2()?,
                    name_and_type: reader.u2()?,
                },
                11 => Constant::InterfaceMethodRef {
                    class: reader.u2()?,
                    name_and_type: reader.u2()?,
                },
                12 => Constant::NameAndType {
                    name: reader.u2()?,
                    descriptor: reader.u2()?,
                },
                15 => Constant::MethodHandle {
                    kind: reader.u1()?,
                    reference: reader.u2()?,
                },
                16 => Constant::MethodType {
                    descriptor: reader.u2()?,
                },
                17 => Constant::Dynamic {
                    bootstrap: reader.u2()?,
                    name_and_type: reader.u2()?,
                },
                18 => Constant::InvokeDynamic {
                    bootstrap: reader.u2()?,
                    name_and_type: reader.u2()?,
                },
                19 => Constant::Module { name: reader.u2()? },
                20 => Constant::Package { name: reader.u2()? },
                _ => {
                    return Err(Error::Malformed {
                        at: reader.pos() - 1,
                        what: "unknown constant pool tag",
                    });
                }
            };

            let wide = matches!(constant, Constant::Long(_) | Constant::Double(_));
            entries.push(constant);
            index += 1;

            if wide {
                entries.push(Constant::Unusable);
                index += 1;
            }
        }

        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1
    }

    pub fn get(&self, index: u16) -> Option<Constant<'a>> {
        self.entries.get(index as usize).copied()
    }

    /// Iterates real entries as `(index, constant)`, skipping slot zero and the dead slots
    /// after eight-byte constants.
    pub fn iter(&self) -> impl Iterator<Item = (u16, Constant<'a>)> + '_ {
        self.entries
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, c)| !matches!(c, Constant::Unusable))
            .map(|(i, c)| (i as u16, *c))
    }

    /// Decodes a `Utf8` entry, replacing anything that is not valid Rust UTF-8. Modified
    /// UTF-8 only diverges for NUL and astral characters, neither of which appears in a
    /// normal identifier, so the lossy path is effectively unreachable in practice.
    pub fn utf8(&self, index: u16) -> Option<std::borrow::Cow<'a, str>> {
        match self.get(index)? {
            Constant::Utf8(bytes) => Some(String::from_utf8_lossy(bytes)),
            _ => None,
        }
    }

    /// Name of a `Class` entry, already in internal form (`java/lang/Object`).
    pub fn class_name(&self, index: u16) -> Option<std::borrow::Cow<'a, str>> {
        match self.get(index)? {
            Constant::Class { name } => self.utf8(name),
            _ => None,
        }
    }

    pub fn name_and_type(
        &self,
        index: u16,
    ) -> Option<(std::borrow::Cow<'a, str>, std::borrow::Cow<'a, str>)> {
        match self.get(index)? {
            Constant::NameAndType { name, descriptor } => {
                Some((self.utf8(name)?, self.utf8(descriptor)?))
            }
            _ => None,
        }
    }
}
