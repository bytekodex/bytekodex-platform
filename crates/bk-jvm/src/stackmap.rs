//! The `StackMapTable` attribute.
//!
//! Every frame says what the verifier should expect at one bytecode offset, and the offsets are
//! stored as deltas — each frame's real offset is the previous one plus the delta plus one, except
//! for the first. Getting that arithmetic wrong is silent: the frames still parse, they just point
//! at the wrong instructions.
//!
//! JDK 28 adds one frame kind. JEP 539 (Strict Field Initialization) uses type 246, previously
//! reserved, for a frame that *wraps* another one:
//!
//! ```text
//! early_larval_frame {
//!     u1 frame_type = EARLY_LARVAL;  /* 246 */
//!     u2 number_of_unset_fields;
//!     u2 unset_fields[number_of_unset_fields];  // NameAndType indices
//!     base_stack_map_frame base_frame;          // any other kind of frame
//! }
//! ```
//!
//! The wrapping is the part worth being careful about: the offset delta lives in the frame inside,
//! not in the wrapper, so a reader that treats 246 as a leaf loses its place in the stream and
//! everything after it is garbage.

use crate::reader::Reader;
use bk_core::error::{Error, Result};

/// One verification type: what a single local slot or stack entry holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VerificationType {
    Top,
    Integer,
    Float,
    Long,
    Double,
    Null,
    UninitializedThis,
    /// A reference to a class, named by a constant pool index.
    Object(u16),
    /// An object created by a `new` that has not run its constructor yet, identified by the
    /// bytecode offset of that `new`.
    Uninitialized(u16),
}

impl VerificationType {
    fn parse(reader: &mut Reader<'_>) -> Result<Self> {
        let tag = reader.u1()?;
        Ok(match tag {
            0 => Self::Top,
            1 => Self::Integer,
            2 => Self::Float,
            3 => Self::Double,
            4 => Self::Long,
            5 => Self::Null,
            6 => Self::UninitializedThis,
            7 => Self::Object(reader.u2()?),
            8 => Self::Uninitialized(reader.u2()?),
            _ => {
                return Err(Error::Malformed {
                    at: reader.pos(),
                    what: "verification type tag is not one of the nine defined",
                });
            }
        })
    }

    /// Whether this type needs a constant pool lookup to print.
    pub fn class_index(self) -> Option<u16> {
        match self {
            Self::Object(index) => Some(index),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Integer => "int",
            Self::Float => "float",
            Self::Long => "long",
            Self::Double => "double",
            Self::Null => "null",
            Self::UninitializedThis => "uninitializedThis",
            Self::Object(_) => "class",
            Self::Uninitialized(_) => "uninitialized",
        }
    }
}

/// What one frame says, after the deltas have been resolved into real offsets.
#[derive(Clone, Debug)]
pub struct Frame {
    /// Bytecode offset this frame applies to.
    pub offset: u32,
    pub kind: FrameKind,
    /// Fields that are still unset at this point, as NameAndType indices. Empty except inside a
    /// constructor of a class with strict fields, which is JDK 28 and later only.
    pub unset_fields: Vec<u16>,
}

/// Which of the six shapes a frame took. Kept rather than normalized because "same" and "full with
/// identical contents" mean the same thing to the verifier but not to a reader trying to
/// understand what javac emitted.
#[derive(Clone, Debug)]
pub enum FrameKind {
    Same,
    SameLocalsOneStackItem(VerificationType),
    /// Locals dropped from the previous frame.
    Chop(u8),
    /// Locals added to the previous frame.
    Append(Vec<VerificationType>),
    Full {
        locals: Vec<VerificationType>,
        stack: Vec<VerificationType>,
    },
}

impl FrameKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::SameLocalsOneStackItem(_) => "same_locals_1_stack_item",
            Self::Chop(_) => "chop",
            Self::Append(_) => "append",
            Self::Full { .. } => "full",
        }
    }
}

/// Parses the whole attribute, resolving offset deltas as it goes.
pub fn parse(data: &[u8]) -> Result<Vec<Frame>> {
    let mut reader = Reader::new(data);
    let count = reader.u2()? as usize;

    let mut frames = Vec::with_capacity(count.min(1024));
    // The first frame's offset is its delta; every later one is previous + delta + 1. Tracking
    // "have we seen one yet" is simpler than seeding this with a sentinel.
    let mut previous: Option<u32> = None;

    for _ in 0..count {
        let (delta, kind, unset_fields) = parse_frame(&mut reader)?;
        let offset = match previous {
            None => delta,
            Some(previous) => previous + delta + 1,
        };
        previous = Some(offset);
        frames.push(Frame {
            offset,
            kind,
            unset_fields,
        });
    }

    Ok(frames)
}

/// Reads one frame, returning its delta separately because the caller owns the running offset.
fn parse_frame(reader: &mut Reader<'_>) -> Result<(u32, FrameKind, Vec<u16>)> {
    let frame_type = reader.u1()?;

    // JEP 539, preview in JDK 28. The wrapper carries the unset fields and the frame inside
    // carries everything else, including the delta.
    if frame_type == EARLY_LARVAL {
        let count = reader.u2()? as usize;
        let mut unset_fields = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            unset_fields.push(reader.u2()?);
        }

        let base_type = reader.u1()?;
        if base_type == EARLY_LARVAL {
            // The spec says the base frame is "any other kind", and a wrapper around a wrapper
            // would mean nothing. Checking the type rather than the payload matters: a nested
            // wrapper with no unset fields is indistinguishable from a plain frame otherwise.
            return Err(Error::Malformed {
                at: reader.pos(),
                what: "early_larval_frame wraps another early_larval_frame",
            });
        }

        let (delta, kind) = parse_base_frame(base_type, reader)?;
        return Ok((delta, kind, unset_fields));
    }

    let (delta, kind) = parse_base_frame(frame_type, reader)?;
    Ok((delta, kind, Vec::new()))
}

/// Reads everything except the JDK 28 wrapper, whose frame type the caller has already consumed.
fn parse_base_frame(frame_type: u8, reader: &mut Reader<'_>) -> Result<(u32, FrameKind)> {
    match frame_type {
        0..=63 => Ok((u32::from(frame_type), FrameKind::Same)),
        64..=127 => Ok((
            u32::from(frame_type) - 64,
            FrameKind::SameLocalsOneStackItem(VerificationType::parse(reader)?),
        )),
        247 => Ok((
            u32::from(reader.u2()?),
            FrameKind::SameLocalsOneStackItem(VerificationType::parse(reader)?),
        )),
        248..=250 => {
            // 251 minus the type is how many locals were dropped: 250 drops one, 248 drops three.
            let dropped = 251 - frame_type;
            Ok((u32::from(reader.u2()?), FrameKind::Chop(dropped)))
        }
        251 => Ok((u32::from(reader.u2()?), FrameKind::Same)),
        252..=254 => {
            let added = usize::from(frame_type - 251);
            let delta = u32::from(reader.u2()?);
            let mut locals = Vec::with_capacity(added);
            for _ in 0..added {
                locals.push(VerificationType::parse(reader)?);
            }
            Ok((delta, FrameKind::Append(locals)))
        }
        255 => {
            let delta = u32::from(reader.u2()?);
            let locals = parse_types(reader)?;
            let stack = parse_types(reader)?;
            Ok((delta, FrameKind::Full { locals, stack }))
        }
        // 128..=245 have never been assigned, and 246 is handled by the caller. Reaching one here
        // means the stream is out of step, which is worth saying rather than guessing at.
        _ => Err(Error::Malformed {
            at: reader.pos(),
            what: "stack map frame type is reserved",
        }),
    }
}

/// Frame type 246, `early_larval_frame`, introduced by JEP 539 for JDK 28.
const EARLY_LARVAL: u8 = 246;

fn parse_types(reader: &mut Reader<'_>) -> Result<Vec<VerificationType>> {
    let count = reader.u2()? as usize;
    let mut types = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        types.push(VerificationType::parse(reader)?);
    }
    Ok(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Offsets are deltas, and every frame after the first is one greater than the sum suggests.
    // An off-by-one here points every frame at the wrong instruction while still parsing cleanly.
    #[test]
    fn offsets_accumulate_with_the_implicit_plus_one() {
        // three same_frames: deltas 5, 0, 3
        let data = [0x00, 0x03, 5, 0, 3];
        let frames = parse(&data).expect("parse");

        let offsets: Vec<u32> = frames.iter().map(|f| f.offset).collect();
        assert_eq!(offsets, vec![5, 6, 10]);
    }

    #[test]
    fn chop_frames_report_how_many_locals_went_away() {
        // frame_type 249 chops two, delta 7
        let data = [0x00, 0x01, 249, 0x00, 0x07];
        let frames = parse(&data).expect("parse");

        assert!(matches!(frames[0].kind, FrameKind::Chop(2)));
        assert_eq!(frames[0].offset, 7);
    }

    #[test]
    fn append_frames_carry_their_new_locals() {
        // 253 appends two: int, then class #9
        let data = [0x00, 0x01, 253, 0x00, 0x04, 1, 7, 0x00, 0x09];
        let frames = parse(&data).expect("parse");

        let FrameKind::Append(locals) = &frames[0].kind else {
            panic!("not an append: {:?}", frames[0].kind);
        };
        assert_eq!(
            locals,
            &vec![VerificationType::Integer, VerificationType::Object(9)]
        );
    }

    #[test]
    fn full_frames_carry_locals_and_stack_separately() {
        let data = [
            0x00, 0x01, 255, 0x00, 0x0C, // full_frame, delta 12
            0x00, 0x01, 7, 0x00, 0x05, // one local: class #5
            0x00, 0x02, 1, 4, // two stack entries: int, long
        ];
        let frames = parse(&data).expect("parse");

        let FrameKind::Full { locals, stack } = &frames[0].kind else {
            panic!("not a full frame");
        };
        assert_eq!(locals, &vec![VerificationType::Object(5)]);
        assert_eq!(
            stack,
            &vec![VerificationType::Integer, VerificationType::Long]
        );
    }

    // The JDK 28 addition. The delta lives in the wrapped frame, so a reader that treats 246 as a
    // leaf loses its place and every frame after it is nonsense.
    #[test]
    fn early_larval_frames_wrap_another_frame_and_keep_its_delta() {
        let data = [
            0x00, 0x02, // two frames
            246, 0x00, 0x02, 0x00, 0x11, 0x00, 0x12, // unset fields #17 and #18
            0x0A, // wrapped same_frame, delta 10
            0x05, // a plain same_frame, delta 5
        ];
        let frames = parse(&data).expect("parse");

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].unset_fields, vec![17, 18]);
        assert!(matches!(frames[0].kind, FrameKind::Same));
        assert_eq!(frames[0].offset, 10);

        // The second frame proves the reader stayed in step: 10 + 5 + 1.
        assert_eq!(frames[1].offset, 16);
        assert!(frames[1].unset_fields.is_empty());
    }

    #[test]
    fn an_early_larval_frame_can_wrap_a_full_frame() {
        let data = [
            0x00, 0x01, 246, 0x00, 0x01, 0x00, 0x07, // unset field #7
            255, 0x00, 0x14, // full_frame, delta 20
            0x00, 0x01, 6, // one local: uninitializedThis
            0x00, 0x00, // empty stack
        ];
        let frames = parse(&data).expect("parse");

        assert_eq!(frames[0].offset, 20);
        assert_eq!(frames[0].unset_fields, vec![7]);
        let FrameKind::Full { locals, .. } = &frames[0].kind else {
            panic!("not a full frame");
        };
        assert_eq!(locals, &vec![VerificationType::UninitializedThis]);
    }

    #[test]
    fn a_wrapper_wrapping_a_wrapper_is_refused() {
        let data = [0x00, 0x01, 246, 0x00, 0x00, 246, 0x00, 0x00, 0x05];
        assert!(parse(&data).is_err());
    }

    #[test]
    fn reserved_frame_types_are_refused_rather_than_guessed_at() {
        for frame_type in [128u8, 200, 245] {
            let data = [0x00, 0x01, frame_type];
            assert!(
                parse(&data).is_err(),
                "frame type {frame_type} should be refused"
            );
        }
    }

    #[test]
    fn a_truncated_attribute_is_an_error_not_a_panic() {
        // Claims four frames, provides one byte.
        let data = [0x00, 0x04, 0x05];
        assert!(parse(&data).is_err());
    }
}
