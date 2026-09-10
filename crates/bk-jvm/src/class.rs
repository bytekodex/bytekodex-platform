use bk_core::{Error, Result};

use crate::pool::ConstantPool;
use crate::reader::Reader;

pub const MAGIC: u32 = 0xCAFE_BABE;

/// Lowest class file version we bother to read. Anything older predates the format's stable
/// attribute set and is not something a modern compiler will hand us.
pub const MIN_MAJOR: u16 = 45;

/// Highest version we claim to understand. Newer files are still parsed — the structure has
/// not changed — but unknown attributes are printed as opaque, and that is worth reporting.
pub const MAX_KNOWN_MAJOR: u16 = 72;

/// A class file compiled with `--enable-preview` carries this minor version, and can only be
/// loaded by the exact JDK matching its major version.
pub const PREVIEW_MINOR: u16 = 0xFFFF;

/// `major - 44` is the JDK feature release, which has held since Java 1.1.
pub fn jdk_release(major: u16) -> Option<u16> {
    (major >= 49).then(|| major - 44)
}

/// Any attribute, kept as raw bytes until something asks for it.
///
/// Attributes are a wire-format extension point: a JVM must ignore ones it does not know, and
/// so must we. Parsing lazily means an unknown `LoadableDescriptors` in a JDK 28 file costs us
/// nothing and breaks nothing.
#[derive(Clone, Copy, Debug)]
pub struct Attribute<'a> {
    pub name: u16,
    pub data: &'a [u8],
}

impl<'a> Attribute<'a> {
    fn parse_list(reader: &mut Reader<'a>) -> Result<Vec<Attribute<'a>>> {
        let count = reader.u2()? as usize;
        let mut attributes = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            let name = reader.u2()?;
            let len = reader.u4()? as usize;
            attributes.push(Attribute {
                name,
                data: reader.bytes(len)?,
            });
        }
        Ok(attributes)
    }
}

/// A field or a method. Both have the same shape in the format.
#[derive(Clone, Debug)]
pub struct Member<'a> {
    pub flags: u16,
    pub name: u16,
    pub descriptor: u16,
    pub attributes: Vec<Attribute<'a>>,
}

impl<'a> Member<'a> {
    fn parse_list(reader: &mut Reader<'a>) -> Result<Vec<Member<'a>>> {
        let count = reader.u2()? as usize;
        let mut members = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            members.push(Member {
                flags: reader.u2()?,
                name: reader.u2()?,
                descriptor: reader.u2()?,
                attributes: Attribute::parse_list(reader)?,
            });
        }
        Ok(members)
    }

    pub fn attribute(&self, pool: &ConstantPool<'a>, wanted: &str) -> Option<Attribute<'a>> {
        self.attributes
            .iter()
            .copied()
            .find(|a| pool.utf8(a.name).is_some_and(|n| n == wanted))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExceptionEntry {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    /// Zero means "any", which is how a `finally` block is encoded.
    pub catch_type: u16,
}

/// The `Code` attribute of one method.
pub struct Code<'a> {
    pub max_stack: u16,
    pub max_locals: u16,
    pub bytes: &'a [u8],
    pub exceptions: Vec<ExceptionEntry>,
    pub attributes: Vec<Attribute<'a>>,
}

impl<'a> Code<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let mut reader = Reader::new(data);
        let max_stack = reader.u2()?;
        let max_locals = reader.u2()?;
        let code_len = reader.u4()? as usize;
        let bytes = reader.bytes(code_len)?;

        let exception_count = reader.u2()? as usize;
        let mut exceptions = Vec::with_capacity(exception_count.min(256));
        for _ in 0..exception_count {
            exceptions.push(ExceptionEntry {
                start_pc: reader.u2()?,
                end_pc: reader.u2()?,
                handler_pc: reader.u2()?,
                catch_type: reader.u2()?,
            });
        }

        Ok(Self {
            max_stack,
            max_locals,
            bytes,
            exceptions,
            attributes: Attribute::parse_list(&mut reader)?,
        })
    }
}

/// One local variable slot, from `LocalVariableTable`.
#[derive(Clone, Copy, Debug)]
pub struct LocalVariable {
    pub start_pc: u16,
    pub length: u16,
    pub name: u16,
    pub descriptor: u16,
    pub slot: u16,
}

pub fn parse_local_variable_table(data: &[u8]) -> Result<Vec<LocalVariable>> {
    let mut reader = Reader::new(data);
    let count = reader.u2()? as usize;
    let mut locals = Vec::with_capacity(count.min(512));
    for _ in 0..count {
        locals.push(LocalVariable {
            start_pc: reader.u2()?,
            length: reader.u2()?,
            name: reader.u2()?,
            descriptor: reader.u2()?,
            slot: reader.u2()?,
        });
    }
    Ok(locals)
}

/// A parsed class file. Borrows the input buffer for its whole life; nothing is copied.
pub struct ClassFile<'a> {
    pub minor: u16,
    pub major: u16,
    pub pool: ConstantPool<'a>,
    pub flags: u16,
    pub this_class: u16,
    /// Zero only for `java/lang/Object`.
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<Member<'a>>,
    pub methods: Vec<Member<'a>>,
    pub attributes: Vec<Attribute<'a>>,
}

impl<'a> ClassFile<'a> {
    pub fn parse(input: &'a [u8]) -> Result<Self> {
        let mut reader = Reader::new(input);

        if reader.u4()? != MAGIC {
            return Err(Error::Malformed {
                at: 0,
                what: "not a class file, magic is not 0xCAFEBABE",
            });
        }

        let minor = reader.u2()?;
        let major = reader.u2()?;
        if major < MIN_MAJOR {
            return Err(Error::UnsupportedVersion { major, minor });
        }

        let pool = ConstantPool::parse(&mut reader)?;
        let flags = reader.u2()?;
        let this_class = reader.u2()?;
        let super_class = reader.u2()?;

        let interface_count = reader.u2()? as usize;
        let mut interfaces = Vec::with_capacity(interface_count.min(256));
        for _ in 0..interface_count {
            interfaces.push(reader.u2()?);
        }

        Ok(Self {
            minor,
            major,
            pool,
            flags,
            this_class,
            super_class,
            interfaces,
            fields: Member::parse_list(&mut reader)?,
            methods: Member::parse_list(&mut reader)?,
            attributes: Attribute::parse_list(&mut reader)?,
        })
    }

    pub fn is_preview(&self) -> bool {
        self.minor == PREVIEW_MINOR
    }

    /// True when the file is newer than the format revision this build knows about.
    pub fn is_newer_than_known(&self) -> bool {
        self.major > MAX_KNOWN_MAJOR
    }

    pub fn name(&self) -> Option<std::borrow::Cow<'a, str>> {
        self.pool.class_name(self.this_class)
    }

    pub fn attribute(&self, wanted: &str) -> Option<Attribute<'a>> {
        self.attributes
            .iter()
            .copied()
            .find(|a| self.pool.utf8(a.name).is_some_and(|n| n == wanted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_class_input() {
        match ClassFile::parse(b"not a class at all") {
            Err(Error::Malformed { at: 0, .. }) => {}
            other => panic!("expected a malformed-magic error, got {:?}", other.err()),
        }
    }

    #[test]
    fn maps_major_version_to_jdk_release() {
        assert_eq!(jdk_release(65), Some(21));
        assert_eq!(jdk_release(69), Some(25));
        assert_eq!(jdk_release(72), Some(28));
    }
}
