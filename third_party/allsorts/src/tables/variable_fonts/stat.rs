#![deny(missing_docs)]

//! `STAT` Style Attributes Table
//!
//! The style attributes table describes design attributes that distinguish
//! font-style variants within a font family. It also provides associations
//! between those attributes and name elements that may be used to present font
//! options within application user interfaces. This information is especially
//! important for variable fonts, but also relevant for non-variable fonts.
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/stat>

use std::fmt;

use bitflags::bitflags;

use crate::binary::read::{
    ReadArray, ReadBinary, ReadBinaryDep, ReadCtxt, ReadFixedSizeDep, ReadFrom, ReadScope,
    ReadUnchecked,
};
use crate::binary::{U16Be, U32Be};
use crate::error::ParseError;
use crate::tables::Fixed;
use crate::tag::DisplayTag;
use crate::{size, SafeFrom};

/// `STAT` Style Attributes Table
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#style-attributes-header>
pub struct StatTable<'a> {
    /// Major version number of the style attributes table.
    pub major_version: u16,
    /// Minor version number of the style attributes table.
    pub minor_version: u16,
    /// The size in bytes of each axis record.
    design_axis_size: u16,
    /// The number of axis records.
    ///
    /// In a font with an `fvar` table, this value must be greater than or
    /// equal to the axisCount value in the `fvar` table. In all fonts, must be
    /// greater than zero if the number of axis value tables is greater than
    /// zero.
    design_axis_count: u16,
    /// The design axes records.
    design_axes_array: &'a [u8],
    /// A read scope from the beginning of the axis offsets array.
    ///
    /// Used for reading the axis value tables.
    axis_value_scope: ReadScope<'a>,
    /// The array of offsets to the axis value tables.
    axis_value_offsets: ReadArray<'a, U16Be>,
    /// Name ID used as fallback when projection of names into a particular font
    /// model produces a subfamily name containing only elidable elements.
    pub elided_fallback_name_id: Option<u16>,
}

/// Information about a single design axis.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#axis-records>
#[derive(Eq, PartialEq, Copy, Clone)]
pub struct AxisRecord {
    /// A tag identifying the axis of design variation.
    pub axis_tag: u32,
    /// The name ID for entries in the `name` table that provide a display
    /// string for this axis.
    pub axis_name_id: u16,
    /// A value that applications can use to determine primary sorting of face
    /// names, or for ordering of labels when composing family or face
    /// names.
    pub axis_ordering: u16,
}

/// Axis value table.
///
/// Axis value tables provide details regarding a specific style-attribute value
/// on some specific axis of design variation, or a combination of
/// design-variation axis values, and the relationship of those values to labels
/// used as elements in subfamily names.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-tables>
#[derive(Debug)]
pub enum AxisValueTable<'a> {
    /// Format 1 axis value table: name associated with a value.
    Format1(AxisValueTableFormat1),
    /// Format 2 axis value table: name associated with a range of values.
    Format2(AxisValueTableFormat2),
    /// Format 3 axis value table: name associated with a value and style-linked
    /// mapping.
    Format3(AxisValueTableFormat3),
    /// Format 4 axis value table: name associated with a value for each design
    /// axis.
    Format4(AxisValueTableFormat4<'a>),
}

/// Format 1 axis value table: name associated with a value.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-1>
#[derive(Debug, Eq, PartialEq)]
pub struct AxisValueTableFormat1 {
    /// Zero-base index into the axis record array identifying the axis of
    /// design variation to which the axis value table applies.
    pub axis_index: u16,
    /// Flags.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the `name` table that provide a display
    /// string for this attribute value.
    value_name_id: u16,
    /// A numeric value for this attribute value.
    pub value: Fixed,
}

/// Format 2 axis value table: name associated with a range of values.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-2>
#[derive(Debug, Eq, PartialEq)]
pub struct AxisValueTableFormat2 {
    /// Zero-base index into the axis record array identifying the axis of
    /// design variation to which the axis value table applies.
    pub axis_index: u16,
    /// Flags.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the `name` table that provide a display
    /// string for this attribute value.
    value_name_id: u16,
    /// A nominal numeric value for this attribute value.
    pub nominal_value: Fixed,
    /// The minimum value for a range associated with the specified name ID.
    pub range_min_value: Fixed,
    /// The maximum value for a range associated with the specified name ID.
    pub range_max_value: Fixed,
}

/// Format 3 axis value table: name associated with a value and style-linked
/// mapping.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-3>
#[derive(Debug, Eq, PartialEq)]
pub struct AxisValueTableFormat3 {
    /// Zero-base index into the axis record array identifying the axis of
    /// design variation to which the axis value table applies.
    pub axis_index: u16,
    /// Flags.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the `name` table that provide a display
    /// string for this attribute value.
    value_name_id: u16,
    /// A numeric value for this attribute value.
    pub value: Fixed,
    /// The numeric value for a style-linked mapping from this value.
    pub linked_value: Fixed,
}

/// Format 4 axis value table: name associated with a value for each design
/// axis.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#axis-value-table-format-4>
#[derive(Debug)]
pub struct AxisValueTableFormat4<'a> {
    /// Flags.
    flags: AxisValueTableFlags,
    /// The name ID for entries in the `name` table that provide a display
    /// string for this combination of axis values.
    value_name_id: u16,
    /// Array of AxisValue records that provide the combination of axis values,
    /// one for each contributing axis.
    pub axis_values: ReadArray<'a, AxisValue>,
}

/// An axis value record from a format 4 axis value table.
#[derive(Debug, Copy, Clone)]
pub struct AxisValue {
    /// Zero-base index into the axis record array identifying the axis to which
    /// this value applies.
    pub axis_index: u16,
    /// A numeric value for this attribute value.
    pub value: Fixed,
}

bitflags! {
    /// Flags for axis value tables.
    ///
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/stat#flags>
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct AxisValueTableFlags: u16 {
        /// If set, this axis value table provides axis value information that is applicable to
        /// other fonts within the same font family. This is used if the other fonts were released
        /// earlier and did not include information about values for some axis. If newer versions
        /// of the other fonts include the information themselves and are present, then this table
        /// is ignored.
        const OLDER_SIBLING_FONT_ATTRIBUTE = 0x0001;
        /// If set, it indicates that the axis value represents the “normal” value for the axis and
        /// may be omitted when composing name strings.
        const ELIDABLE_AXIS_VALUE_NAME = 0x0002;
        // 0xFFFC 	Reserved 	Reserved for future use — set to zero.
    }
}

/// Boolean value to indicate to [StatTable::name_for_axis_value] whether names
/// from tables with the `ELIDABLE_AXIS_VALUE_NAME` flag set should be included
/// or excluded in the result.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ElidableName {
    /// Include elidable names
    Include,
    /// Exclude elidable names
    Exclude,
}

impl<'a> StatTable<'a> {
    /// Iterate over the design axes.
    pub fn design_axes(&'a self) -> impl Iterator<Item = Result<AxisRecord, ParseError>> + 'a {
        (0..usize::from(self.design_axis_count)).map(move |i| self.design_axis(i))
    }

    /// Retrieve the design axis at the supplied index.
    pub fn design_axis(&self, index: usize) -> Result<AxisRecord, ParseError> {
        let design_axis_size = usize::from(self.design_axis_size);
        let offset = index * design_axis_size;
        self.design_axes_array
            .get(offset..(offset + design_axis_size))
            .ok_or(ParseError::BadIndex)
            .and_then(|data| ReadScope::new(data).read::<AxisRecord>())
    }

    /// Iterate over the axis value tables.
    pub fn axis_value_tables(
        &'a self,
    ) -> impl Iterator<Item = Result<AxisValueTable<'a>, ParseError>> {
        self.axis_value_offsets.iter().filter_map(move |offset| {
            let res = self
                .axis_value_scope
                .offset(usize::from(offset))
                .read_dep::<AxisValueTable<'_>>(self.design_axis_count);
            match res {
                Ok(table) => Some(Ok(table)),
                // "If the format is not recognized, then the axis value table can be ignored"
                Err(ParseError::BadVersion) => None,
                Err(err) => Some(Err(err)),
            }
        })
    }

    /// Find a name that best describes `value` in the axis at index
    /// `axis_index`.
    ///
    /// `axis_index` is the index of the axis in
    /// [design_axes](Self::design_axes).
    pub fn name_for_axis_value(
        &'a self,
        axis_index: u16,
        value: Fixed,
        include_elidable: ElidableName,
    ) -> Option<u16> {
        // Find candidate entries
        let mut best: Option<(Fixed, u16, bool)> = None;
        for table in self.axis_value_tables() {
            let Ok(table) = table else { continue };

            match &table {
                AxisValueTable::Format1(t) if t.axis_index == axis_index => consider(
                    &mut best,
                    t.value,
                    t.value_name_id,
                    table.is_elidable(),
                    value,
                ),
                AxisValueTable::Format2(t) if t.axis_index == axis_index => {
                    if (t.range_min_value..=t.range_max_value).contains(&value) {
                        consider(
                            &mut best,
                            t.nominal_value,
                            t.value_name_id,
                            table.is_elidable(),
                            value,
                        )
                    }
                }
                AxisValueTable::Format3(t) if t.axis_index == axis_index => consider(
                    &mut best,
                    t.value,
                    t.value_name_id,
                    table.is_elidable(),
                    value,
                ),
                AxisValueTable::Format4(t) => {
                    // NOTE: It's unclear if there be multiple entries for the same axis index
                    let Some(axis_value) = t.axis_values.iter_res().find_map(|value| {
                        value
                            .ok()
                            .and_then(|value| (value.axis_index == axis_index).then_some(value))
                    }) else {
                        continue;
                    };
                    consider(
                        &mut best,
                        axis_value.value,
                        t.value_name_id,
                        table.is_elidable(),
                        value,
                    )
                }
                AxisValueTable::Format1(_)
                | AxisValueTable::Format2(_)
                | AxisValueTable::Format3(_) => {}
            }
        }

        best.and_then(|(_best_val, name, is_elidable)| match include_elidable {
            ElidableName::Include => Some(name),
            // If the best match is elidable and include_elidable is Exclude then return None
            ElidableName::Exclude => (!is_elidable).then_some(name),
        })
    }
}

fn consider(
    best: &mut Option<(Fixed, u16, bool)>,
    candidate_value: Fixed,
    candidate_name_id: u16,
    is_elidable: bool,
    value: Fixed,
) {
    match best {
        Some((best_val, _name, _is_elidable)) => {
            if (candidate_value - value).abs() < (*best_val - value).abs() {
                *best = Some((candidate_value, candidate_name_id, is_elidable));
            }
        }
        None => *best = Some((candidate_value, candidate_name_id, is_elidable)),
    }
}

impl ReadBinary for StatTable<'_> {
    type HostType<'a> = StatTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<StatTable<'a>, ParseError> {
        let scope = ctxt.scope();
        let major_version = ctxt.read_u16be()?;
        ctxt.check_version(major_version == 1)?;
        let minor_version = ctxt.read_u16be()?;
        let design_axis_size = ctxt.read_u16be()?;
        let design_axis_count = ctxt.read_u16be()?;
        let design_axes_offset = ctxt.read_u32be()?;
        let design_axes_array = if design_axis_count > 0 {
            let design_axes_length = usize::from(design_axis_count) * usize::from(design_axis_size);
            scope
                .offset(usize::safe_from(design_axes_offset))
                .ctxt()
                .read_slice(design_axes_length)?
        } else {
            &[]
        };

        let axis_value_count = ctxt.read_u16be()?;
        let offset_to_axis_value_offsets = ctxt.read_u32be()?;
        let (axis_value_scope, axis_value_offsets) = if axis_value_count > 0 {
            let axis_value_scope = scope.offset(usize::safe_from(offset_to_axis_value_offsets));
            (
                axis_value_scope,
                axis_value_scope
                    .ctxt()
                    .read_array(usize::from(axis_value_count))?,
            )
        } else {
            (ReadScope::new(&[]), ReadArray::empty())
        };
        let elided_fallback_name_id = (minor_version > 0).then(|| ctxt.read_u16be()).transpose()?;

        Ok(StatTable {
            major_version,
            minor_version,
            design_axis_size,
            design_axis_count,
            design_axes_array,
            axis_value_scope,
            axis_value_offsets,
            elided_fallback_name_id,
        })
    }
}

impl ReadFrom for AxisRecord {
    type ReadType = (U32Be, U16Be, U16Be);

    fn read_from((axis_tag, axis_name_id, axis_ordering): (u32, u16, u16)) -> Self {
        AxisRecord {
            axis_tag,
            axis_name_id,
            axis_ordering,
        }
    }
}

impl ReadFrom for AxisValueTableFlags {
    type ReadType = U16Be;

    fn read_from(flag: u16) -> Self {
        AxisValueTableFlags::from_bits_truncate(flag)
    }
}

impl AxisValueTable<'_> {
    /// Retrieve the flags for this axis value table.
    pub fn flags(&self) -> AxisValueTableFlags {
        match self {
            AxisValueTable::Format1(AxisValueTableFormat1 { flags, .. })
            | AxisValueTable::Format2(AxisValueTableFormat2 { flags, .. })
            | AxisValueTable::Format3(AxisValueTableFormat3 { flags, .. })
            | AxisValueTable::Format4(AxisValueTableFormat4 { flags, .. }) => *flags,
        }
    }

    /// Retrieve the name id in the `NAME` table for this value.
    pub fn value_name_id(&self) -> u16 {
        match self {
            AxisValueTable::Format1(AxisValueTableFormat1 { value_name_id, .. })
            | AxisValueTable::Format2(AxisValueTableFormat2 { value_name_id, .. })
            | AxisValueTable::Format3(AxisValueTableFormat3 { value_name_id, .. })
            | AxisValueTable::Format4(AxisValueTableFormat4 { value_name_id, .. }) => {
                *value_name_id
            }
        }
    }

    /// If set, it indicates that the axis value represents the “normal” value
    /// for the axis and may be omitted when composing name strings.
    pub fn is_elidable(&self) -> bool {
        self.flags()
            .contains(AxisValueTableFlags::ELIDABLE_AXIS_VALUE_NAME)
    }
}

impl ReadBinaryDep for AxisValueTable<'_> {
    type Args<'a> = u16;
    type HostType<'a> = AxisValueTable<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        design_axis_count: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let format = ctxt.read_u16be()?;
        match format {
            1 => {
                let axis_index = ctxt.read_u16be()?;
                ctxt.check_index(axis_index < design_axis_count)?;
                let flags = ctxt.read::<AxisValueTableFlags>()?;
                let value_name_id = ctxt.read_u16be()?;
                let value = ctxt.read::<Fixed>()?;
                Ok(AxisValueTable::Format1(AxisValueTableFormat1 {
                    axis_index,
                    flags,
                    value_name_id,
                    value,
                }))
            }
            2 => {
                let axis_index = ctxt.read_u16be()?;
                ctxt.check_index(axis_index < design_axis_count)?;
                let flags = ctxt.read::<AxisValueTableFlags>()?;
                let value_name_id = ctxt.read_u16be()?;
                let nominal_value = ctxt.read::<Fixed>()?;
                let range_min_value = ctxt.read::<Fixed>()?;
                let range_max_value = ctxt.read::<Fixed>()?;
                Ok(AxisValueTable::Format2(AxisValueTableFormat2 {
                    axis_index,
                    flags,
                    value_name_id,
                    nominal_value,
                    range_min_value,
                    range_max_value,
                }))
            }
            3 => {
                let axis_index = ctxt.read_u16be()?;
                ctxt.check_index(axis_index < design_axis_count)?;
                let flags = ctxt.read::<AxisValueTableFlags>()?;
                let value_name_id = ctxt.read_u16be()?;
                let value = ctxt.read::<Fixed>()?;
                let linked_value = ctxt.read::<Fixed>()?;
                Ok(AxisValueTable::Format3(AxisValueTableFormat3 {
                    axis_index,
                    flags,
                    value_name_id,
                    value,
                    linked_value,
                }))
            }
            4 => {
                let axis_count = ctxt.read_u16be()?;
                let flags = ctxt.read::<AxisValueTableFlags>()?;
                let value_name_id = ctxt.read_u16be()?;
                let axis_values =
                    ctxt.read_array_dep(usize::from(axis_count), design_axis_count)?;
                Ok(AxisValueTable::Format4(AxisValueTableFormat4 {
                    flags,
                    value_name_id,
                    axis_values,
                }))
            }
            _ => Err(ParseError::BadVersion),
        }
    }
}

impl ReadBinaryDep for AxisValue {
    type Args<'a> = u16;
    type HostType<'a> = AxisValue;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        design_axis_count: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let axis_index = ctxt.read_u16be()?;
        ctxt.check_index(axis_index < design_axis_count)?;
        let value = ctxt.read::<Fixed>()?;
        Ok(AxisValue { axis_index, value })
    }
}

impl ReadFixedSizeDep for AxisValue {
    fn size(_args: Self::Args<'_>) -> usize {
        size::U16 + Fixed::SIZE
    }
}

impl fmt::Debug for AxisRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = format!("{:?} ({})", self.axis_tag, DisplayTag(self.axis_tag));
        f.debug_struct("AxisRecord")
            .field("axis_tag", &tag)
            .field("axis_name_id", &self.axis_name_id)
            .field("axis_ordering", &self.axis_ordering)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_data::FontData;
    use crate::tables::{FontTableProvider, NameTable};
    use crate::tag;
    use crate::tests::read_fixture;

    #[test]
    fn stat() {
        let buffer = read_fixture("tests/fonts/opentype/NotoSans-VF.abc.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope
            .read::<FontData<'_>>()
            .expect("unable to parse font file");
        let table_provider = font_file
            .table_provider(0)
            .expect("unable to create font provider");
        let stat_data = table_provider
            .read_table_data(tag::STAT)
            .expect("unable to read fvar table data");
        let stat = ReadScope::new(&stat_data).read::<StatTable<'_>>().unwrap();
        let name_table_data = table_provider
            .read_table_data(tag::NAME)
            .expect("unable to read name table data");
        let name_table = ReadScope::new(&name_table_data)
            .read::<NameTable<'_>>()
            .unwrap();

        let expected = [
            AxisRecord {
                axis_tag: tag!(b"wght"),
                axis_name_id: 261,
                axis_ordering: 0,
            },
            AxisRecord {
                axis_tag: tag!(b"wdth"),
                axis_name_id: 271,
                axis_ordering: 1,
            },
            AxisRecord {
                axis_tag: tag!(b"CTGR"),
                axis_name_id: 276,
                axis_ordering: 2,
            },
        ];
        let design_axes = stat.design_axes().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(design_axes, expected);

        let axis_value_tables = stat
            .axis_value_tables()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(axis_value_tables.len(), 15);
        let first = axis_value_tables.first().unwrap();
        let AxisValueTable::Format1(table) = first else {
            panic!("expected AxisValueTableFormat1")
        };
        let expected = AxisValueTableFormat1 {
            axis_index: 0,
            flags: AxisValueTableFlags::empty(),
            value_name_id: 262,
            value: <Fixed as From<f32>>::from(100.),
        };
        assert_eq!(table, &expected);
        let value_name = name_table.string_for_id(first.value_name_id()).unwrap();
        assert_eq!(value_name, "Thin");

        let last = axis_value_tables.last().unwrap();
        let AxisValueTable::Format1(table) = last else {
            panic!("expected AxisValueTableFormat1")
        };
        let expected = AxisValueTableFormat1 {
            axis_index: 2,
            flags: AxisValueTableFlags::empty(),
            value_name_id: 278,
            value: <Fixed as From<f32>>::from(100.),
        };
        assert_eq!(table, &expected);
        let value_name = name_table.string_for_id(last.value_name_id()).unwrap();
        assert_eq!(value_name, "Display");
    }
}
