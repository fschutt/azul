#![deny(missing_docs)]

//! `fvar` Font Variations Table
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/fvar>

use crate::binary::read::{
    ReadArray, ReadBinary, ReadBinaryDep, ReadCtxt, ReadFrom, ReadScope, ReadUnchecked,
};
use crate::binary::{U16Be, U32Be};
use crate::error::ParseError;
use crate::tables::variable_fonts::avar::AvarTable;
use crate::tables::variable_fonts::UserTuple;
use crate::tables::{F2Dot14, Fixed};
use tinyvec::TinyVec;

/// `fvar` font Variations Table
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#fvar-header>
pub struct FvarTable<'a> {
    /// Major version number of the font variations table
    pub major_version: u16,
    /// Minor version number of the font variations table
    pub minor_version: u16,
    /// The VariationAxisRecords
    axes: ReadArray<'a, VariationAxisRecord>,
    /// The number of named instances defined in the font (the number of records
    /// in the instances array).
    instance_count: u16,
    /// The size in bytes of each InstanceRecord
    instance_size: u16,
    instance_array: &'a [u8],
}

/// Variation axis
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#variationaxisrecord>
#[derive(Eq, PartialEq, Debug)]
pub struct VariationAxisRecord {
    /// Tag identifying the design variation for the axis.
    pub axis_tag: u32,
    /// The minimum coordinate value for the axis.
    pub min_value: Fixed,
    /// The default coordinate value for the axis.
    pub default_value: Fixed,
    /// The maximum coordinate value for the axis.
    pub max_value: Fixed,
    /// Axis qualifiers.
    pub flags: u16,
    /// The name ID for entries in the `name` table that provide a display name
    /// for this axis.
    pub axis_name_id: u16,
}

/// Variation instance record
///
/// Instances are like named presets for a variable font. Each instance has name
/// and a value for each variation axis.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#instancerecord>
#[derive(Debug)]
pub struct InstanceRecord<'a> {
    /// The name ID for entries in the `name` table that provide subfamily names
    /// for this instance.
    pub subfamily_name_id: u16,
    /// Flags
    pub flags: u16,
    /// The coordinates array for this instance.
    pub coordinates: UserTuple<'a>,
    /// Optional. The name ID for entries in the `name` table that provide
    /// PostScript names for this instance.
    pub post_script_name_id: Option<u16>,
}

// Wes counted the number of axes in 399 variable fonts in Google Fonts and this
// was the result:
//
// | Axis Count | Number |
// |------------|--------|
// | 1          | 279    |
// | 2          | 108    |
// | 3          | 2      |
// | 4          | 5      |
// | 5          | 1      |
// | 13         | 2      |
// | 15         | 2      |
//
// With this in mind the majority of fonts are handled with two axes. However,
// the minimum size of a TinyVec is 24 bytes due to the Vec it can also hold, so
// I choose 4 since it doesn't use any more space than when set to two.

/// Coordinate array specifying a position within the font’s variation space
/// (owned version).
///
/// Owned version of [Tuple].
///
/// This must be constructed through the the [FvarTable] to
/// ensure that the number of elements matches the
/// [axis_count](FvarTable::axis_count()).
#[derive(Debug)]
pub struct OwnedTuple(TinyVec<[F2Dot14; 4]>);

/// A variation tuple containing a normalized value for each variation axis.
#[derive(Debug, Copy, Clone)]
pub struct Tuple<'a>(&'a [F2Dot14]);

impl FvarTable<'_> {
    /// Returns an iterator over the variation axes of the font.
    pub fn axes(&self) -> impl Iterator<Item = VariationAxisRecord> + '_ {
        self.axes.iter()
    }

    /// Returns the number of variation axes in the font.
    pub fn axis_count(&self) -> u16 {
        // NOTE(cast): Valid as self.axes is constructed from a u16 length
        self.axes.len() as u16
    }

    /// Returns an iterator over the pre-defined instances in the font.
    pub fn instances(&self) -> impl Iterator<Item = Result<InstanceRecord<'_>, ParseError>> + '_ {
        // These are pulled out to work around lifetime errors if &self is moved into
        // the closure.
        let instance_array = self.instance_array;
        let axis_count = self.axis_count();
        let instance_size = usize::from(self.instance_size);
        (0..usize::from(self.instance_count)).map(move |i| {
            let offset = i * instance_size;
            instance_array
                .get(offset..(offset + instance_size))
                .ok_or(ParseError::BadIndex)
                .and_then(|data| {
                    ReadScope::new(data).read_dep::<InstanceRecord<'_>>((instance_size, axis_count))
                })
        })
    }

    /// Turn a user tuple into a tuple normalized over the range -1..1.
    pub fn normalize(
        &self,
        user_tuple: impl ExactSizeIterator<Item = Fixed>,
        avar: Option<&AvarTable<'_>>,
    ) -> Result<OwnedTuple, ParseError> {
        if user_tuple.len() != usize::from(self.axis_count()) {
            return Err(ParseError::BadValue);
        }

        let mut tuple = TinyVec::with_capacity(user_tuple.len());
        let mut avar_iter = avar.map(|avar| avar.segment_maps());
        for (axis, user_value) in self.axes().zip(user_tuple) {
            let mut normalized_value = default_normalize(&axis, user_value);

            // If avar table is present do more normalization with it
            if let Some(avar) = avar_iter.as_mut() {
                let segment_map = avar.next().ok_or(ParseError::BadIndex)?;
                normalized_value = segment_map.normalize(normalized_value);
                // Do the -1..1 clamping again to ensure the value remains in range
                normalized_value = normalized_value.clamp(Fixed::from(-1), Fixed::from(1));
            }

            // Convert the final, normalized 16.16 coordinate value to 2.14.
            tuple.push(F2Dot14::from(normalized_value));
        }
        Ok(OwnedTuple(tuple))
    }

    /// Construct a new [OwnedTuple].
    ///
    /// Returns `None` if the number of elements in `values` does not match
    /// [axis_count](FvarTable::axis_count()).
    pub fn owned_tuple(&self, values: &[F2Dot14]) -> Option<OwnedTuple> {
        (values.len() == usize::from(self.axis_count())).then(|| OwnedTuple(TinyVec::from(values)))
    }
}

fn default_normalize(axis: &VariationAxisRecord, coord: Fixed) -> Fixed {
    // Clamp
    let coord = coord.clamp(axis.min_value, axis.max_value);

    // Interpolate
    let normalised_value = if coord < axis.default_value {
        -(axis.default_value - coord) / (axis.default_value - axis.min_value)
    } else if coord > axis.default_value {
        (coord - axis.default_value) / (axis.max_value - axis.default_value)
    } else {
        Fixed::from(0)
    };

    // After the default normalization calculation is performed, some results may be
    // slightly outside the range [-1, +1]. Values must be clamped to this
    // range.
    normalised_value.clamp(Fixed::from(-1), Fixed::from(1))
}

impl ReadBinary for FvarTable<'_> {
    type HostType<'a> = FvarTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let major_version = ctxt.read_u16be()?;
        ctxt.check_version(major_version == 1)?;
        let minor_version = ctxt.read_u16be()?;
        let axes_array_offset = ctxt.read_u16be()?;
        let _reserved = ctxt.read_u16be()?;
        let axis_count = ctxt.read_u16be()?;
        let axis_size = ctxt.read_u16be()?;
        let instance_count = ctxt.read_u16be()?;
        let instance_size = ctxt.read_u16be()?;
        let instance_length = usize::from(instance_count) * usize::from(instance_size);
        let mut data_ctxt = scope.offset(usize::from(axes_array_offset)).ctxt();
        let axes = data_ctxt.read_array_stride(usize::from(axis_count), usize::from(axis_size))?;
        let instance_array = data_ctxt.read_slice(instance_length)?;

        Ok(FvarTable {
            major_version,
            minor_version,
            axes,
            instance_count,
            instance_size,
            instance_array,
        })
    }
}

/// Utility type for reading just the axis count from `fvar` table.
pub struct FvarAxisCount;

impl ReadBinary for FvarAxisCount {
    type HostType<'a> = u16;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let major_version = ctxt.read_u16be()?;
        ctxt.check_version(major_version == 1)?;
        let _minor_version = ctxt.read_u16be()?;
        let _axes_array_offset = ctxt.read_u16be()?;
        let _reserved = ctxt.read_u16be()?;
        let axis_count = ctxt.read_u16be()?;

        Ok(axis_count)
    }
}

impl ReadFrom for VariationAxisRecord {
    type ReadType = ((U32Be, Fixed, Fixed), (Fixed, U16Be, U16Be));

    fn read_from(
        ((axis_tag, min_value, default_value), (max_value, flags, axis_name_id)): (
            (u32, Fixed, Fixed),
            (Fixed, u16, u16),
        ),
    ) -> Self {
        VariationAxisRecord {
            axis_tag,
            min_value,
            default_value,
            max_value,
            flags,
            axis_name_id,
        }
    }
}

impl ReadBinaryDep for InstanceRecord<'_> {
    type Args<'a> = (usize, u16);
    type HostType<'a> = InstanceRecord<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (record_size, axis_count): (usize, u16),
    ) -> Result<Self::HostType<'a>, ParseError> {
        let axis_count = usize::from(axis_count);
        let subfamily_name_id = ctxt.read_u16be()?;
        let flags = ctxt.read_u16be()?;
        let coordinates = ctxt.read_array(axis_count).map(UserTuple)?;
        // If the record size is larger than the size of the subfamily_name_id, flags,
        // and coordinates then the optional post_script_name_id is present.
        let post_script_name_id = (record_size > axis_count * Fixed::SIZE + 4)
            .then(|| ctxt.read_u16be())
            .transpose()?;

        Ok(InstanceRecord {
            subfamily_name_id,
            flags,
            coordinates,
            post_script_name_id,
        })
    }
}

impl OwnedTuple {
    /// Borrow this value as a [Tuple].
    pub fn as_tuple(&self) -> Tuple<'_> {
        Tuple(&self.0)
    }
}

impl std::ops::Deref for OwnedTuple {
    type Target = [F2Dot14];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> Tuple<'a> {
    /// Construct a `Tuple` from a pointer and length.
    ///
    /// ## Safety
    ///
    /// You must ensure all the requirements of [std::slice::from_raw_parts] are upheld in
    /// addition to:
    ///
    /// - There must be exactly `fvar.axis_count` values.
    /// - Values must be clamped to -1 to 1.
    pub unsafe fn from_raw_parts(data: *const F2Dot14, length: usize) -> Tuple<'a> {
        Tuple(std::slice::from_raw_parts(data, length))
    }

    /// Retrieve the instance value for the axis at `index`
    pub fn get(&self, index: u16) -> Option<F2Dot14> {
        self.0.get(usize::from(index)).copied()
    }
}

impl std::ops::Deref for Tuple<'_> {
    type Target = [F2Dot14];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ReadWriteError;
    use crate::font_data::FontData;
    use crate::tables::{FontTableProvider, NameTable};
    use crate::tag;
    use crate::tests::read_fixture;

    #[test]
    fn fvar() {
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
        let name_table_data = table_provider
            .read_table_data(tag::NAME)
            .expect("unable to read name table data");
        let name_table = ReadScope::new(&name_table_data)
            .read::<NameTable<'_>>()
            .unwrap();

        let expected = [
            VariationAxisRecord {
                axis_tag: tag!(b"wght"),
                min_value: <Fixed as From<i32>>::from(100),
                default_value: <Fixed as From<i32>>::from(400),
                max_value: <Fixed as From<i32>>::from(900),
                flags: 0,
                axis_name_id: 279,
            },
            VariationAxisRecord {
                axis_tag: tag!(b"wdth"),
                min_value: <Fixed as From<f32>>::from(62.5),
                default_value: <Fixed as From<i32>>::from(100),
                max_value: <Fixed as From<i32>>::from(100),
                flags: 0,
                axis_name_id: 280,
            },
            VariationAxisRecord {
                axis_tag: tag!(b"CTGR"),
                min_value: <Fixed as From<i32>>::from(0),
                default_value: <Fixed as From<i32>>::from(0),
                max_value: <Fixed as From<i32>>::from(100),
                flags: 0,
                axis_name_id: 281,
            },
        ];
        assert_eq!(fvar.axes().collect::<Vec<_>>(), expected);

        let instances = fvar.instances().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(instances.len(), 72);
        let first = instances.first().unwrap();
        let subfamily_name = name_table.string_for_id(first.subfamily_name_id).unwrap();
        assert_eq!(subfamily_name, "Thin");
        // axis="wght" value="100.0", axis="wdth" value="100.0", axis="CTGR" value="0.0"
        let coordinates = [
            <Fixed as From<f32>>::from(100.),
            <Fixed as From<f32>>::from(100.),
            <Fixed as From<f32>>::from(0.),
        ];
        assert_eq!(first.coordinates.0.iter().collect::<Vec<_>>(), coordinates);

        let last = instances.last().unwrap();
        let subfamily_name = name_table.string_for_id(last.subfamily_name_id).unwrap();
        assert_eq!(subfamily_name, "Display ExtraCondensed Black");
        //  axis="wght" value="900.0", axis="wdth" value="62.5", axis="CTGR"
        // value="100.0"
        let coordinates = [
            <Fixed as From<f32>>::from(900.),
            <Fixed as From<f32>>::from(62.5),
            <Fixed as From<f32>>::from(100.),
        ];
        assert_eq!(last.coordinates.0.iter().collect::<Vec<_>>(), coordinates);
    }

    #[test]
    fn test_fvar_normalization() -> Result<(), ReadWriteError> {
        let buffer = read_fixture("tests/fonts/opentype/NotoSans-VF.abc.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope.read::<FontData<'_>>()?;
        let provider = font_file.table_provider(0)?;
        let fvar_data = provider
            .read_table_data(tag::FVAR)
            .expect("unable to read fvar table data");
        let fvar = ReadScope::new(&fvar_data).read::<FvarTable<'_>>().unwrap();
        let avar_data = provider.table_data(tag::AVAR)?;
        let avar = avar_data
            .as_ref()
            .map(|avar_data| ReadScope::new(avar_data).read::<AvarTable<'_>>())
            .transpose()?;
        let name_table_data = provider
            .read_table_data(tag::NAME)
            .expect("unable to read name table data");
        let name_table = ReadScope::new(&name_table_data)
            .read::<NameTable<'_>>()
            .unwrap();

        // Pick an instance
        let mut instance = None;
        for inst in fvar.instances() {
            let inst = inst?;
            let subfamily = name_table.string_for_id(inst.subfamily_name_id);
            if subfamily.as_deref() == Some("Display Condensed Thin") {
                // - wght = min: 100, max: 900, default: 400
                // - wdth = min: 62.5, max: 100, default: 100
                // - CTGR = min: 0, max: 100, default: 0
                //
                // Coordinates: [100.0, 62.5, 100.0]
                instance = Some(inst);
                break;
            }
        }
        let instance = instance.unwrap();

        // The instance is a UserTuple record that needs be normalised into a ReadTuple
        // record
        let tuple = fvar.normalize(instance.coordinates.iter(), avar.as_ref())?;
        assert_eq!(
            tuple.0.as_slice(),
            &[
                F2Dot14::from(-1.0),
                F2Dot14::from(-0.7000122),
                F2Dot14::from(1.0)
            ]
        );

        Ok(())
    }

    #[test]
    fn test_default_normalization() {
        // https://learn.microsoft.com/en-us/typography/opentype/spec/otvaroverview#avar-normalization-example
        let axis = VariationAxisRecord {
            axis_tag: tag!(b"wght"),
            min_value: Fixed::from(100),
            default_value: Fixed::from(400),
            max_value: Fixed::from(900),
            flags: 0,
            axis_name_id: 0,
        };
        let user_coord = Fixed::from(250);
        assert_eq!(default_normalize(&axis, user_coord), Fixed::from(-0.5))
    }
}
