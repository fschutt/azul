#![deny(missing_docs)]

//! `gvar` Glyph Variations Table
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar>

use crate::binary::read::{ReadBinary, ReadCtxt, ReadScope, ReadUnchecked};
use crate::binary::{U16Be, U32Be};
use crate::error::ParseError;
use crate::tables::loca::LocaOffsets;
use crate::tables::variable_fonts::{ReadTuple, TupleVariationStore};
use crate::tables::F2Dot14;
use crate::SafeFrom;
use std::fmt;
use std::fmt::Formatter;

/// `gvar` Glyph Variations Table
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#gvar-header>
pub struct GvarTable<'a> {
    /// Major version number of the glyph variations table.
    pub major_version: u16,
    /// Minor version number of the glyph variations table.
    pub minor_version: u16,
    /// The number of variation axes for this font.
    ///
    /// This must be the same number as axisCount in
    /// the 'fvar' table.
    pub axis_count: u16,
    /// The number of shared tuple records.
    ///
    /// Shared tuple records can be referenced within glyph
    /// variation data tables for multiple glyphs, as opposed to other tuple
    /// records stored directly within a glyph variation data table.
    shared_tuple_count: u16,
    /// Scope containing data for the shared tuple records.
    shared_tuples_scope: ReadScope<'a>,
    /// The number of glyphs in this font.
    ///
    /// This must match the number of glyphs stored elsewhere in
    /// the font.
    pub glyph_count: u16,
    /// Scope containing the data for the array of GlyphVariationData tables.
    glyph_variation_data_array_scope: ReadScope<'a>,
    /// Offsets from the start of the GlyphVariationData array to each
    /// GlyphVariationData table.
    glyph_variation_data_offsets: LocaOffsets<'a>,
}

/// A count of the number of points in a glyph including the four
/// [phantom points](https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructing_glyphs#phantom-points).
#[derive(Debug, Copy, Clone)]
pub struct NumPoints(u32);

impl NumPoints {
    /// Create a new NumPoints instance with `num` glyph points (excluding
    /// phantom points).
    pub fn new(num: u16) -> NumPoints {
        NumPoints(u32::from(num) + 4)
    }

    /// Construct a NumPoints instance from a value that has already had the
    /// phantom points added.
    pub(crate) fn from_raw(num: u32) -> NumPoints {
        NumPoints(num)
    }

    /// Get the underlying number of points (including phantom points).
    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for NumPoints {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'a> GvarTable<'a> {
    /// Returns the variation data for the glyph at `glyph_index` that has
    /// `num_points` points (including and phantom points).
    ///
    /// If the glyph has no variations, such as when the glyph is an empty
    /// glyph, then `None` is returned.
    pub fn glyph_variation_data(
        &self,
        glyph_index: u16,
        num_points: NumPoints,
    ) -> Result<Option<TupleVariationStore<'a, super::Gvar>>, ParseError> {
        let glyph_index = usize::from(glyph_index);
        let start = self
            .glyph_variation_data_offsets
            .get(glyph_index)
            .map(usize::safe_from)
            .ok_or(ParseError::BadIndex)?;
        let end = self
            .glyph_variation_data_offsets
            .get(glyph_index + 1)
            .map(usize::safe_from)
            .ok_or(ParseError::BadIndex)?;
        let length = end.checked_sub(start).ok_or(ParseError::BadOffset)?;
        if length > 0 {
            let scope = self
                .glyph_variation_data_array_scope
                .offset_length(start, length)?;
            scope
                .read_dep::<TupleVariationStore<'_, super::Gvar>>((
                    self.axis_count,
                    num_points.get(),
                    scope,
                ))
                .map(Some)
        } else {
            Ok(None)
        }
    }

    /// Returns the shared peak tuple at the supplied index.
    pub fn shared_tuple(&self, index: u16) -> Result<ReadTuple<'a>, ParseError> {
        if index >= self.shared_tuple_count {
            return Err(ParseError::BadIndex);
        }

        let offset = usize::from(index) * usize::from(self.axis_count) * F2Dot14::SIZE;
        let shared_tuple = self
            .shared_tuples_scope
            .offset(offset)
            .ctxt()
            .read_array::<F2Dot14>(usize::from(self.axis_count))
            .map(ReadTuple)?;
        Ok(shared_tuple)
    }
}

impl ReadBinary for GvarTable<'_> {
    type HostType<'a> = GvarTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let major_version = ctxt.read_u16be()?;
        ctxt.check_version(major_version == 1)?;
        let minor_version = ctxt.read_u16be()?;
        let axis_count = ctxt.read_u16be()?;
        let shared_tuple_count = ctxt.read_u16be()?;
        let shared_tuples_offset = ctxt.read_u32be()?;
        let glyph_count = ctxt.read_u16be()?;
        let flags = ctxt.read_u16be()?;
        // Offset from the start of this table to the array of GlyphVariationData
        // tables.
        let glyph_variation_data_array_offset = ctxt.read_u32be()?;
        // Offsets from the start of the GlyphVariationData array to each
        // GlyphVariationData table. If bit 0 is clear, the offsets are uint16;
        // if bit 0 is set, the offsets are uint32.
        let glyph_variation_data_offsets = if flags & 1 == 1 {
            // The actual local offset is stored. The value of n is numGlyphs + 1.
            LocaOffsets::Long(ctxt.read_array::<U32Be>(usize::from(glyph_count) + 1)?)
        } else {
            // The actual local offset divided by 2 is stored. The value of n is numGlyphs +
            // 1.
            LocaOffsets::Short(ctxt.read_array::<U16Be>(usize::from(glyph_count) + 1)?)
        };

        // Store the shared tuples
        let shared_tuples_len =
            usize::from(shared_tuple_count) * usize::from(axis_count) * F2Dot14::SIZE;
        let shared_tuples_scope =
            scope.offset_length(usize::safe_from(shared_tuples_offset), shared_tuples_len)?;

        // Read the glyph variation data
        if glyph_variation_data_offsets.len() < 2 {
            return Err(ParseError::BadIndex);
        }
        // NOTE(unwrap): Safe due to check above
        let glyph_variation_data_array_scope = scope.offset_length(
            usize::safe_from(glyph_variation_data_array_offset),
            usize::safe_from(glyph_variation_data_offsets.last().unwrap()),
        )?;

        Ok(GvarTable {
            major_version,
            minor_version,
            axis_count,
            shared_tuple_count,
            shared_tuples_scope,
            glyph_count,
            glyph_variation_data_array_scope,
            glyph_variation_data_offsets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::read::ReadScope;
    use crate::error::ReadWriteError;
    use crate::font_data::FontData;
    use crate::tables::glyf::GlyfTable;
    use crate::tables::loca::LocaTable;
    use crate::tables::{FontTableProvider, HeadTable, MaxpTable};
    use crate::tag;
    use crate::tests::read_fixture;

    #[test]
    fn gvar() {
        let buffer = read_fixture("tests/fonts/opentype/NotoSans-VF.abc.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope
            .read::<FontData<'_>>()
            .expect("unable to parse font file");
        let table_provider = font_file
            .table_provider(0)
            .expect("unable to create font provider");
        let stat_data = table_provider
            .read_table_data(tag::GVAR)
            .expect("unable to read fvar table data");
        let gvar = ReadScope::new(&stat_data).read::<GvarTable<'_>>().unwrap();
        assert_eq!(gvar.major_version, 1);
        assert_eq!(gvar.minor_version, 0);
        assert_eq!(gvar.axis_count, 3);
        assert_eq!(gvar.shared_tuple_count, 15);
        assert_eq!(gvar.shared_tuples_scope.data().len(), 90);
        assert_eq!(gvar.glyph_count, 4);
        assert_eq!(gvar.glyph_variation_data_array_scope.data().len(), 3028);
        assert_eq!(gvar.glyph_variation_data_offsets.len(), 5);
    }

    #[test]
    fn glyph_variation_data() -> Result<(), ReadWriteError> {
        let buffer = read_fixture("tests/fonts/opentype/NotoSans-VF.abc.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope.read::<FontData<'_>>()?;
        let provider = font_file.table_provider(0)?;
        let head = ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
        let maxp = ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
        let loca_data = provider.read_table_data(tag::LOCA)?;
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((maxp.num_glyphs, head.index_to_loc_format))?;
        let glyf_data = provider.read_table_data(tag::GLYF)?;
        let glyf = ReadScope::new(&glyf_data).read_dep::<GlyfTable<'_>>(&loca)?;
        let gvar_data = provider.read_table_data(tag::GVAR)?;
        let gvar = ReadScope::new(&gvar_data).read::<GvarTable<'_>>().unwrap();

        let glyph = 3; // 'c' glyph
        let num_points = NumPoints::new(glyf.records()[3].number_of_points()?);
        let store = gvar
            .glyph_variation_data(glyph, num_points)?
            .expect("variation store");
        let variation_data = (0..store.tuple_variation_headers.len())
            .into_iter()
            .map(|i| store.variation_data(i as u16))
            .collect::<Result<Vec<_>, _>>()?;

        // This one uses shared point numbers, which specify all 34 points in the glyph
        let deltas = variation_data[0].iter().collect::<Vec<_>>();
        let expected = vec![
            (0, (2, 0)),
            (1, (-11, 0)),
            (2, (-8, 13)),
            (3, (4, 14)),
            (4, (4, -4)),
            (5, (4, -22)),
            (6, (1, -21)),
            (7, (4, -8)),
            (8, (13, -8)),
            (9, (8, -8)),
            (10, (-9, -3)),
            (11, (-5, -3)),
            (12, (17, 45)),
            (13, (11, 50)),
            (14, (16, 44)),
            (15, (15, 44)),
            (16, (-3, 44)),
            (17, (-37, 26)),
            (18, (-60, 2)),
            (19, (-60, -5)),
            (20, (-60, -9)),
            (21, (-49, -31)),
            (22, (-22, -51)),
            (23, (3, -51)),
            (24, (-5, -51)),
            (25, (-1, -55)),
            (26, (0, -56)),
            (27, (0, -3)),
            (28, (2, 1)),
            (29, (-3, 0)),
            (30, (0, 0)),
            (31, (-8, 0)),
            (32, (0, 0)),
            (33, (0, 0)),
        ];
        assert_eq!(deltas, expected);

        // This one has private point numbers
        let deltas = variation_data[4].iter().collect::<Vec<_>>();
        let expected = vec![
            (1, (-15, 0)),
            (5, (-4, 0)),
            (9, (-20, 0)),
            (11, (-24, 0)),
            (12, (-24, 0)),
            (14, (-20, 0)),
            (17, (-7, 0)),
            (20, (-5, 0)),
            (22, (-12, 0)),
            (23, (-17, 0)),
            (24, (-20, 0)),
            (27, (-24, 0)),
            (31, (-26, 0)),
        ];
        assert_eq!(deltas, expected);

        Ok(())
    }
}
