#![deny(missing_docs)]

//! `CPAL` table parsing.
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/cpal>

use bitflags::bitflags;

use crate::binary::read::{ReadArray, ReadBinary, ReadCtxt, ReadFrom};
use crate::binary::{U16Be, U32Be, U8};
use crate::error::ParseError;
use crate::SafeFrom;

/// `CPAL` — Color Palette Table
pub struct CpalTable<'a> {
    /// Table version number.
    pub version: u16,
    /// Number of palette entries in each palette.
    num_palette_entries: u16,
    /// Color records for all palettes.
    color_records_array: ReadArray<'a, ColorRecord>,
    /// Index of each palette’s first color record in the combined color record array.
    color_record_indices: ReadArray<'a, U16Be>,
    /// Palette Types Array.
    palette_types_array: Option<ReadArray<'a, U32Be>>,
    /// Palette Labels Array.
    palette_labels_array: Option<ReadArray<'a, U16Be>>,
    /// Palette Entry Labels Array.
    palette_entry_labels_array: Option<ReadArray<'a, U16Be>>,
}

bitflags! {
    /// Flags describing features of a palette.
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct PaletteFlags: u32 {
        /// Palette is appropriate to use when displaying the font on a light background such as white.
        const USABLE_WITH_LIGHT_BACKGROUND = 0b00000001;
        /// Palette is appropriate to use when displaying the font on a dark background such as black.
        const USABLE_WITH_DARK_BACKGROUND  = 0b00000010;
    }
}

impl<'data> CpalTable<'data> {
    /// Obtain the palette at `index`.
    ///
    /// > The first palette, palette index 0, is the default palette.
    /// > A minimum of one palette must be provided in the `CPAL` table if the table is present.
    /// > Palettes must have a minimum of one color record.
    pub fn palette<'a>(&'a self, index: u16) -> Option<Palette<'a, 'data>> {
        let base_index = self.color_record_indices.get_item(usize::from(index))?;
        Some(Palette {
            cpal: self,
            index,
            base_index,
        })
    }

    /// Id of an entry in the [NameTable][crate::tables::NameTable] that
    /// provides a user-interface associated with each palette entry.
    ///
    /// If the palette entry does not have a label, `None` is returned.
    pub fn entry_label(&self, entry_index: u16) -> Option<u16> {
        // 0xFFFF indicates there is no string for a particular palette entry
        self.palette_entry_labels_array
            .as_ref()
            .and_then(|labels| labels.get_item(usize::from(entry_index)))
            .filter(|name_id| *name_id != 0xFFFF)
    }
}

impl ReadBinary for CpalTable<'_> {
    type HostType<'a> = CpalTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let start = ctxt.scope();
        let version = ctxt.read_u16be()?;
        // Number of palette entries in each palette.
        // Palettes must have a minimum of one color record.
        let num_palette_entries = ctxt.read_u16be()?;
        ctxt.check(num_palette_entries > 0)?;
        let num_palettes = ctxt.read_u16be()?;
        // A minimum of one palette must be provided in the CPAL table if the table is present.
        ctxt.check(num_palettes > 0)?;
        let num_color_records = ctxt.read_u16be()?;
        let color_records_array_offset = ctxt.read_u32be()?;
        // Multiple colorRecordIndices may refer to the same color record, in which case multiple
        // palettes would use the same color records
        let color_record_indices = ctxt.read_array(usize::from(num_palettes))?;
        let color_records_array = start
            .offset(usize::safe_from(color_records_array_offset))
            .ctxt()
            .read_array(usize::from(num_color_records))?;

        let (
            palette_types_array_offset,
            palette_labels_array_offset,
            palette_entry_labels_array_offset,
        ) = if version == 1 {
            let palette_types_array_offset = ctxt.read_u32be()?;
            let palette_labels_array_offset = ctxt.read_u32be()?;
            let palette_entry_labels_array_offset = ctxt.read_u32be()?;
            (
                palette_types_array_offset,
                palette_labels_array_offset,
                palette_entry_labels_array_offset,
            )
        } else {
            (0, 0, 0)
        };

        let palette_types_array =
            start.read_optional_array(palette_types_array_offset, num_palettes)?;

        let palette_labels_array =
            start.read_optional_array(palette_labels_array_offset, num_palettes)?;

        let palette_entry_labels_array =
            start.read_optional_array(palette_entry_labels_array_offset, num_palette_entries)?;

        Ok(CpalTable {
            version,
            num_palette_entries,
            color_records_array,
            color_record_indices,
            palette_types_array,
            palette_labels_array,
            palette_entry_labels_array,
        })
    }
}

/// A `CPAL` palette.
#[derive(Copy, Clone)]
pub struct Palette<'a, 'data> {
    cpal: &'a CpalTable<'data>,
    /// Palette index of this palette.
    index: u16,
    /// Base index in the first color record in the color record array for this palette.
    base_index: u16,
}

impl Palette<'_, '_> {
    /// Retrieve the color record at `index` in this palette.
    pub fn color(&self, index: u16) -> Option<ColorRecord> {
        // TODO: A palette entry index value of 0xFFFF is a special case
        // indicating that the text foreground color (defined by the application) should be used,
        // and must not be treated as an actual index into the CPAL ColorRecord array.
        if index == 0xFFFF {
            return Some(ColorRecord {
                blue: 0,
                green: 0,
                red: 0,
                alpha: u8::MAX,
            });
        } else if index >= self.cpal.num_palette_entries {
            return None;
        }

        let color_index = u32::from(self.base_index) + u32::from(index);
        self.cpal
            .color_records_array
            .get_item(usize::safe_from(color_index))
    }

    /// Returns the id of an entry in the [NameTable][crate::tables::NameTable] that
    /// provides a user-interface string for the palette.
    ///
    /// If the palette does not have a label, `None` is returned.
    pub fn label(&self) -> Option<u16> {
        // 0xFFFF indicates there is no string for a particular palette
        self.cpal
            .palette_labels_array
            .as_ref()
            .and_then(|labels| labels.get_item(usize::from(self.index)))
            .filter(|name_id| *name_id != 0xFFFF)
    }

    /// Retrieve the flags for this palette.
    ///
    /// **Note:** The USABLE_WITH_LIGHT_BACKGROUND and USABLE_WITH_DARK_BACKGROUND flags
    /// are not mutually exclusive: they may both be set.
    pub fn flags(&self) -> PaletteFlags {
        self.cpal
            .palette_types_array
            .as_ref()
            .and_then(|types| types.get_item(usize::from(self.index)))
            .map(PaletteFlags::from_bits_truncate)
            .unwrap_or(PaletteFlags::empty())
    }
}

/// A BGRA color record.
#[derive(Debug, Copy, Clone)]
pub struct ColorRecord {
    /// Blue value (B0).
    pub blue: u8,
    /// Green value (B1).
    pub green: u8,
    /// Red value (B2).
    pub red: u8,
    /// Alpha value (B3).
    pub alpha: u8,
}

impl ReadFrom for ColorRecord {
    type ReadType = (U8, U8, U8, U8);

    fn read_from((blue, green, red, alpha): (u8, u8, u8, u8)) -> Self {
        ColorRecord {
            blue,
            green,
            red,
            alpha,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        binary::read::ReadScope,
        tables::{FontTableProvider, OpenTypeFont},
        tag,
        tests::read_fixture,
    };

    #[test]
    fn test_read_cpal_v1_variable() {
        let buffer = read_fixture(
            "tests/fonts/colr/SixtyfourConvergence-Regular-VariableFont_BLED,SCAN,XELA,YELA.ttf",
        );
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let table_provider = otf.table_provider(0).expect("error reading font file");

        let cpal_data = table_provider
            .read_table_data(tag::CPAL)
            .expect("unable to read CPAL data");
        let cpal = ReadScope::new(&cpal_data)
            .read::<CpalTable<'_>>()
            .expect("unable to parse CPAL table");

        assert_eq!(cpal.version, 1);
        assert_eq!(cpal.num_palette_entries, 6);
        assert_eq!(cpal.color_records_array.len(), 12);
        assert_eq!(cpal.color_record_indices.len(), 2);
        assert_eq!(
            cpal.palette_types_array.as_ref().map(|map| map.len()),
            Some(2)
        );
        assert!(cpal.palette_labels_array.is_none());
        assert!(cpal.palette_entry_labels_array.is_none());
    }
}
