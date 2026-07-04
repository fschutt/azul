//! `MVAR` Metrics Variations Table
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/mvar>

use crate::binary::read::{ReadArray, ReadBinary, ReadCtxt, ReadFrom, ReadUnchecked};
use crate::binary::{U16Be, U32Be};
use crate::error::ParseError;
use crate::tables::variable_fonts::{DeltaSetIndexMapEntry, ItemVariationStore, OwnedTuple};

/// `MVAR` Metrics Variations Table
pub struct MvarTable<'a> {
    /// Major version number of the metrics variations table.
    pub major_version: u16,
    /// Minor version number of the metrics variations table.
    pub minor_version: u16,
    /// The item variation data, `None` if `value_records.len()` is zero.
    item_variation_store: Option<ItemVariationStore<'a>>,
    /// Array of value records that identify target items and the associated
    /// delta-set index for each.
    ///
    /// The valueTag records must be in binary order of their valueTag field.
    value_records: ReadArray<'a, ValueRecord>,
}

/// Identifies target items by tag their associated delta-set index.
#[derive(Copy, Clone)]
pub struct ValueRecord {
    /// Four-byte tag identifying a font-wide measure.
    pub value_tag: u32,
    /// A delta-set outer index.
    ///
    /// Used to select an item variation data sub-table within the item
    /// variation store.
    delta_set_outer_index: u16,
    /// A delta-set inner index.
    ///
    /// Used to select a delta-set row within an item variation data sub-table.
    delta_set_inner_index: u16,
}

impl<'a> MvarTable<'a> {
    /// Retrieve the delta for the supplied
    /// [value tag](https://learn.microsoft.com/en-us/typography/opentype/spec/mvar#value-tags).
    pub fn lookup(&self, tag: u32, instance: &OwnedTuple) -> Option<f32> {
        let item_variation_store = self.item_variation_store.as_ref()?;
        let value_record = self
            .value_records
            .binary_search_by(|record| record.value_tag.cmp(&tag))
            .ok()
            .and_then(|index| self.value_records.get_item(index))?;
        // To compute the interpolated instance value for a given target item, the
        // application first obtains the delta-set index for that item. It uses
        // the outer-level index portion to select an item variation data
        // sub-table within the item variation store, and the inner-level index
        // portion to select a delta-set row within that sub-table.
        //
        // The delta set contains one delta for each region referenced by the sub-table,
        // in order of the region indices given in the regionIndices array. The
        // application uses the regionIndices array for that sub-table to
        // identify applicable regions and to compute a scalar for each of these
        // regions based on the selected instance. Each of the scalars is
        // then applied to the corresponding delta within the delta set to derive a
        // scaled adjustment. The scaled adjustments for the row are then
        // combined to obtain the overall adjustment for the item.
        item_variation_store
            .adjustment(value_record.into(), instance)
            .ok()
    }

    /// Iterator over the [ValueRecords][ValueRecord] in this `MVAR` table.
    pub fn value_records(&self) -> impl Iterator<Item = ValueRecord> + 'a {
        self.value_records.iter()
    }

    /// The number of [ValueRecords][ValueRecord] in this `MVAR` table.
    pub fn value_records_len(&self) -> u16 {
        // NOTE(cast): Safe as value_records was contructed from u16 value_record_count.
        self.value_records.len() as u16
    }
}

impl ReadBinary for MvarTable<'_> {
    type HostType<'a> = MvarTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let major_version = ctxt.read_u16be()?;
        ctxt.check_version(major_version == 1)?;
        let minor_version = ctxt.read_u16be()?;
        let _reserved = ctxt.read_u16be()?;
        let value_record_size = ctxt.read_u16be()?;
        let value_record_count = ctxt.read_u16be()?;
        let item_variation_store_offset = ctxt.read_u16be()?;
        let value_records = if value_record_count > 0 {
            // The spec says that value_record_size must be greater than zero but a font
            // was encountered (DecovarAlpha) where it was zero. However the count was also
            // zero so we accept this.
            ctxt.check(usize::from(value_record_size) >= ValueRecord::SIZE)?;
            ctxt.read_array_stride::<ValueRecord>(
                usize::from(value_record_count),
                usize::from(value_record_size),
            )?
        } else {
            ReadArray::empty()
        };
        let item_variation_store = (item_variation_store_offset > 0)
            .then(|| {
                scope
                    .offset(usize::from(item_variation_store_offset))
                    .read::<ItemVariationStore<'_>>()
            })
            .transpose()?;

        Ok(MvarTable {
            major_version,
            minor_version,
            item_variation_store,
            value_records,
        })
    }
}

impl ReadFrom for ValueRecord {
    type ReadType = (U32Be, U16Be, U16Be);

    fn read_from(
        (value_tag, delta_set_outer_index, delta_set_inner_index): (u32, u16, u16),
    ) -> Self {
        ValueRecord {
            value_tag,
            delta_set_outer_index,
            delta_set_inner_index,
        }
    }
}

impl From<ValueRecord> for DeltaSetIndexMapEntry {
    fn from(record: ValueRecord) -> DeltaSetIndexMapEntry {
        DeltaSetIndexMapEntry {
            outer_index: record.delta_set_outer_index,
            inner_index: record.delta_set_inner_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::read::ReadScope;
    use crate::font_data::FontData;
    use crate::tables::variable_fonts::fvar::FvarTable;
    use crate::tables::{Fixed, FontTableProvider};
    use crate::tag;
    use crate::tests::{assert_close, read_fixture};

    #[test]
    fn lookup_value() {
        let buffer = read_fixture("tests/fonts/opentype/NotoSans-VF.abc.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope
            .read::<FontData<'_>>()
            .expect("unable to parse font file");
        let table_provider = font_file
            .table_provider(0)
            .expect("unable to create font provider");
        let fvar_data = table_provider
            .read_table_data(tag::FVAR)
            .expect("unable to read fvar table data");
        let fvar = ReadScope::new(&fvar_data).read::<FvarTable<'_>>().unwrap();
        let mvar_data = table_provider
            .read_table_data(tag::MVAR)
            .expect("unable to read mvar table data");
        let mvar = ReadScope::new(&mvar_data).read::<MvarTable<'_>>().unwrap();
        //  axis="wght" value="900.0", axis="wdth" value="62.5", axis="CTGR"
        // value="100.0"
        let user_tuple = [Fixed::from(900), Fixed::from(62.5), Fixed::from(100)];
        let instance = fvar.normalize(user_tuple.iter().copied(), None).unwrap();
        let val = mvar.lookup(tag!(b"xhgt"), &instance).unwrap();
        // Value verified by creating a static instance of the font with fonttools,
        // dumping it with ttx and then observing the OS/2.sxHeight = 553, which
        // is 17 more than the default of 536 in the original. fonttools
        // invocation: fonttools varLib.mutator
        // src/fonts/allsorts/tests/fonts/opentype/NotoSans-VF.abc.ttf  wght=900
        // wdth=62.5 CTGR=100
        assert_close(val, 17.0);
    }
}
