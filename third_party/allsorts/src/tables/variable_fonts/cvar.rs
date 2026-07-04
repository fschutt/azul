#![deny(missing_docs)]

//! `cvar` CVT Variations Table
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/cvar>

use crate::binary::read::{ReadArrayCow, ReadBinaryDep, ReadCtxt};
use crate::error::ParseError;
use crate::tables::variable_fonts::{OwnedTuple, TupleVariationStore};
use crate::tables::CvtTable;
use crate::SafeFrom;

/// `cvar` CVT Variations Table
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/cvar#table-format>
pub struct CvarTable<'a> {
    /// Major version number of the glyph variations table.
    pub major_version: u16,
    /// Minor version number of the glyph variations table.
    pub minor_version: u16,
    /// Variation data
    pub store: TupleVariationStore<'a, super::Cvar>,
}

impl CvarTable<'_> {
    /// Apply `cvar` variations to `cvt` table, returning adjusted table.
    pub fn apply<'new>(
        &self,
        instance: &OwnedTuple,
        cvt: &CvtTable<'_>,
    ) -> Result<CvtTable<'new>, ParseError> {
        let num_cvts = cvt.values.len() as u32;
        let mut values = cvt.values.iter().map(|val| val as f32).collect::<Vec<_>>();

        for (scale, region) in self.store.determine_applicable(self, instance) {
            let variation_data =
                region.variation_data(num_cvts, self.store.shared_point_numbers())?;
            for (cvt_index, delta) in variation_data.iter() {
                let val = values
                    .get_mut(usize::safe_from(cvt_index))
                    .ok_or(ParseError::BadIndex)?;
                *val += scale * delta as f32
            }
        }

        Ok(CvtTable {
            values: ReadArrayCow::Owned(values.into_iter().map(|val| val.round() as i16).collect()),
        })
    }
}

impl ReadBinaryDep for CvarTable<'_> {
    type Args<'a> = (u16, u32);
    type HostType<'a> = CvarTable<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (axis_count, num_cvts): (u16, u32),
    ) -> Result<Self::HostType<'a>, ParseError> {
        let table_scope = ctxt.scope();
        let major_version = ctxt.read_u16be()?;
        ctxt.check_version(major_version == 1)?;
        let minor_version = ctxt.read_u16be()?;
        let store = ctxt.read_dep::<TupleVariationStore<'_, super::Cvar>>((
            axis_count,
            num_cvts,
            table_scope,
        ))?;

        Ok(CvarTable {
            major_version,
            minor_version,
            store,
        })
    }
}
