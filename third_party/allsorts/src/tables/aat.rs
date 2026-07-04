use crate::{
    binary::read::{ReadBinary, ReadCtxt, ReadFrom},
    error::ParseError,
};

pub const MAX_LEN: usize = 0x4000;
pub const MAX_OPS: isize = 0x4000;

/// End of text.
///
/// This class should not appear in the class array.
pub const CLASS_CODE_EOT: u16 = 0;

/// Out of bounds.
///
/// All glyph indexes that are less than firstGlyph, or greater than or equal to firstGlyph plus
/// nGlyphs will automatically be assigned class code 1. Class code 1 may also appear in the class
/// array.
pub const CLASS_CODE_OOB: u16 = 1;

/// Deleted glyph.
///
/// Sometimes contextual processing removes a glyph from the glyph array by changing its glyph
/// index to the deleted glyph index, 0xFFFF. This glyph code is automatically assigned class
/// "deleted," which should not appear in the class array.
pub const CLASS_CODE_DELETED: u16 = 2;
pub const DELETED_GLYPH: u16 = 0xFFFF;

#[derive(Debug)]
pub struct VecTable<T>(pub Vec<T>);

impl<T> ReadBinary for VecTable<T>
where
    T: ReadFrom,
{
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let mut elements = Vec::new();

        while let Ok(element) = ctxt.read::<T>() {
            elements.push(element)
        }

        Ok(VecTable::<T>(elements))
    }
}
