#![deny(missing_docs)]

//! Common tables pertaining to variable fonts.

use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;

use tinyvec::{tiny_vec, TinyVec};

use crate::binary::read::{
    DebugData, ReadArray, ReadBinary, ReadBinaryDep, ReadCtxt, ReadFixedSizeDep, ReadFrom,
    ReadScope, ReadUnchecked,
};
use crate::binary::write::{WriteBinary, WriteContext};
use crate::binary::{I16Be, I32Be, U16Be, U32Be, I8, U8};
use crate::error::{ParseError, WriteError};
use crate::tables::variable_fonts::cvar::CvarTable;
use crate::tables::variable_fonts::gvar::{GvarTable, NumPoints};
use crate::tables::{F2Dot14, Fixed};
use crate::SafeFrom;

pub mod avar;
pub mod cvar;
pub mod fvar;
pub mod gvar;
pub mod hvar;
pub mod mvar;
pub mod stat;

pub use crate::tables::variable_fonts::fvar::{OwnedTuple, Tuple};

/// Coordinate array specifying a position within the font’s variation space.
///
/// The number of elements must match the
/// [axis_count](fvar::FvarTable::axis_count()) specified in
/// the [FvarTable](fvar::FvarTable).
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#tuple-records>
#[derive(Debug, Clone)]
pub struct ReadTuple<'a>(ReadArray<'a, F2Dot14>);

/// Tuple in user coordinates
///
/// **Note:** The UserTuple record and ReadTuple record both describe a position in
/// the variation space but are distinct: UserTuple uses Fixed values to
/// represent user scale coordinates, while ReadTuple record uses F2Dot14 values to
/// represent normalized coordinates.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#instancerecord>
#[derive(Debug)]
pub struct UserTuple<'a>(ReadArray<'a, Fixed>);

/// Phantom type for [TupleVariationStore] from a `gvar` table.
pub enum Gvar {}
/// Phantom type for [TupleVariationStore] from a `cvar` table.
pub enum Cvar {}

pub(crate) trait PeakTuple<'data> {
    type Table;

    fn peak_tuple<'a>(&'a self, table: &'a Self::Table) -> Result<ReadTuple<'data>, ParseError>;
}

/// Tuple Variation Store Header.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#tuple-variation-store-header>
pub struct TupleVariationStore<'a, T> {
    /// The number of points in the glyph this store is for
    num_points: u32,
    /// The serialized data block begins with shared “point” number data,
    /// followed by the variation data for the tuple variation tables.
    ///
    /// The shared point number data is optional: it is present if the
    /// corresponding flag is set in the `tuple_variation_flags_and_count`
    /// field of the header.
    shared_point_numbers: Option<PointNumbers>,
    /// Array of tuple variation headers.
    tuple_variation_headers: Vec<TupleVariationHeader<'a, T>>,
}

/// Tuple variation header.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#tuplevariationheader>
pub struct TupleVariationHeader<'a, T> {
    /// The size in bytes of the serialized data for this tuple variation table.
    variation_data_size: u16,
    /// A packed field. The high 4 bits are flags. The low 12 bits are an index
    /// into a shared tuple records array.
    tuple_flags_and_index: u16,
    /// Peak tuple record for this tuple variation table — optional, determined
    /// by flags in the tupleIndex value.
    ///
    /// Note that this must always be included in the `cvar` table.
    peak_tuple: Option<ReadTuple<'a>>,
    /// The start and end tuples for the intermediate region.
    ///
    /// Presence determined by flags in the `tuple_flags_and_index` value.
    intermediate_region: Option<(ReadTuple<'a>, ReadTuple<'a>)>,
    /// The serialized data for this Tuple Variation
    data: &'a [u8],
    variant: PhantomData<T>,
}

/// Glyph variation data.
///
/// (x, y) deltas for numbered points.
pub struct GvarVariationData<'a> {
    point_numbers: Cow<'a, PointNumbers>,
    x_coord_deltas: Vec<i16>,
    y_coord_deltas: Vec<i16>,
}

/// CVT variation data.
///
/// deltas for numbered CVTs.
pub struct CvarVariationData<'a> {
    point_numbers: Cow<'a, PointNumbers>,
    deltas: Vec<i16>,
}

#[derive(Clone)]
enum PointNumbers {
    All(u32),
    Specific(Vec<u16>),
}

/// A collection of point numbers that are shared between variations.
pub struct SharedPointNumbers<'a>(&'a PointNumbers);

/// Item variation store.
///
/// > Includes a variation region list, which defines the different regions of the font’s variation
/// > space for which variation data is defined. It also includes a set of itemVariationData
/// > sub-tables, each of which provides a portion of the total variation data. Each sub-table is
/// > associated with some subset of the defined regions, and will include deltas used for one or
/// > more target items.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#variation-data>
#[derive(Clone)]
pub struct ItemVariationStore<'a> {
    /// The variation region list.
    pub variation_region_list: VariationRegionList<'a>,
    /// The item variation data
    pub item_variation_data: Vec<ItemVariationData<'a>>,
}

/// List of regions for which delta adjustments have effect.
#[derive(Clone)]
pub struct VariationRegionList<'a> {
    /// Array of variation regions.
    pub variation_regions: ReadArray<'a, VariationRegion<'a>>,
}

/// Variation data specified as regions of influence and delta values.
#[derive(Clone)]
pub struct ItemVariationData<'a> {
    /// The number of delta sets for distinct items.
    item_count: u16,
    /// A packed field: the high bit is a flag.
    word_delta_count: u16,
    /// The number of variation regions referenced.
    region_index_count: u16,
    /// Array of indices into the variation region list for the regions
    /// referenced by this item variation data table.
    region_indexes: ReadArray<'a, U16Be>,
    /// Delta-set rows.
    delta_sets: &'a [u8],
}

#[derive(Clone, Debug)]
/// A record of the variation regions for each axis in the font.
pub struct VariationRegion<'a> {
    /// Array of region axis coordinates records, in the order of axes given in
    /// the `fvar` table.
    region_axes: ReadArray<'a, RegionAxisCoordinates>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct RegionAxisCoordinates {
    /// The region start coordinate value for the current axis.
    start_coord: F2Dot14,
    /// The region peak coordinate value for the current axis.
    peak_coord: F2Dot14,
    /// The region end coordinate value for the current axis.
    end_coord: F2Dot14,
}

/// A mapping to delta set indices.
pub struct DeltaSetIndexMap<'a> {
    /// A packed field that describes the compressed representation of delta-set
    /// indices.
    entry_format: u8,
    /// The number of mapping entries.
    map_count: u32,
    /// The delta-set index mapping data.
    map_data: &'a [u8],
}

/// An outer/inner index pair for looking up an entry in a `DeltaSetIndexMap` or
/// [ItemVariationStore].
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data>
#[derive(Debug, Copy, Clone)]
pub struct DeltaSetIndexMapEntry {
    /// Index into the outer table (row)
    pub outer_index: u16,
    /// Index into the inner table (column)
    pub inner_index: u16,
}

/// Contains owned versions of some variable font tables.
pub mod owned {
    use super::{DeltaSetIndexMapEntry, DeltaSetT, Tuple};
    use crate::error::ParseError;
    use crate::tables::F2Dot14;

    /// Owned version of [super::ItemVariationStore].
    pub struct ItemVariationStore {
        /// The variation region list.
        pub(super) variation_region_list: VariationRegionList,
        /// The item variation data
        pub(super) item_variation_data: Vec<ItemVariationData>,
    }

    /// Owned version of [super::VariationRegionList].
    pub(super) struct VariationRegionList {
        /// Array of variation regions.
        pub(super) variation_regions: Vec<VariationRegion>,
    }

    /// Owned version of [super::ItemVariationData].
    pub(super) struct ItemVariationData {
        /// A packed field: the high bit is a flag.
        pub(super) word_delta_count: u16,
        /// The number of variation regions referenced.
        pub(super) region_index_count: u16,
        /// Array of indices into the variation region list for the regions
        /// referenced by this item variation data table.
        pub(super) region_indexes: Vec<u16>,
        /// Delta-set rows.
        pub(super) delta_sets: Box<[u8]>,
    }

    /// Owned version of [super::VariationRegion].
    pub(crate) struct VariationRegion {
        /// Array of region axis coordinates records, in the order of axes given in
        /// the `fvar` table.
        pub(super) region_axes: Vec<super::RegionAxisCoordinates>,
    }

    impl ItemVariationStore {
        pub(crate) fn adjustment(
            &self,
            delta_set_entry: DeltaSetIndexMapEntry,
            instance: Tuple<'_>,
        ) -> Result<f32, ParseError> {
            let item_variation_data = self
                .item_variation_data
                .get(usize::from(delta_set_entry.outer_index))
                .ok_or(ParseError::BadIndex)?;
            let delta_set = item_variation_data
                .delta_set(delta_set_entry.inner_index)
                .ok_or(ParseError::BadIndex)?;

            let mut adjustment = 0.;
            for (delta, region_index) in delta_set
                .iter()
                .zip(item_variation_data.region_indexes.iter().copied())
            {
                let region = self
                    .variation_region(region_index)
                    .ok_or(ParseError::BadIndex)?;
                if let Some(scalar) = region.scalar(instance.iter().copied()) {
                    adjustment += scalar * delta as f32;
                }
            }
            Ok(adjustment)
        }

        fn variation_region(&self, region_index: u16) -> Option<&VariationRegion> {
            let region_index = usize::from(region_index);
            if region_index >= self.variation_region_list.variation_regions.len() {
                return None;
            }
            self.variation_region_list
                .variation_regions
                .get(region_index)
        }
    }

    impl DeltaSetT for ItemVariationData {
        fn delta_sets(&self) -> &[u8] {
            self.delta_sets.as_ref()
        }

        fn raw_word_delta_count(&self) -> u16 {
            self.word_delta_count
        }

        fn region_index_count(&self) -> u16 {
            self.region_index_count
        }
    }

    impl ItemVariationData {
        pub fn delta_set(&self, index: u16) -> Option<super::DeltaSet<'_>> {
            self.delta_set_impl(index)
        }
    }

    impl VariationRegion {
        pub(crate) fn scalar(&self, tuple: impl Iterator<Item = F2Dot14>) -> Option<f32> {
            super::scalar(self.region_axes.iter().copied(), tuple)
        }
    }
}

impl<'a> UserTuple<'a> {
    /// Iterate over the axis values in this user tuple.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Fixed> + 'a {
        self.0.iter()
    }

    /// Returns the number of values in this user tuple.
    ///
    /// Should be the same as the number of axes in the `fvar` table.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'data, T> TupleVariationStore<'data, T> {
    /// Flag indicating that some or all tuple variation tables reference a
    /// shared set of “point” numbers.
    ///
    /// These shared numbers are represented as packed point number data at the
    /// start of the serialized data.
    const SHARED_POINT_NUMBERS: u16 = 0x8000;

    /// Mask for the low bits to give the number of tuple variation tables.
    const COUNT_MASK: u16 = 0x0FFF;

    /// Iterate over the tuple variation headers.
    pub fn headers(&self) -> impl Iterator<Item = &TupleVariationHeader<'data, T>> {
        self.tuple_variation_headers.iter()
    }

    /// Get the shared point numbers for this variation store if present.
    pub fn shared_point_numbers(&self) -> Option<SharedPointNumbers<'_>> {
        self.shared_point_numbers.as_ref().map(SharedPointNumbers)
    }
}

impl<'data, T> TupleVariationStore<'data, T> {
    pub(crate) fn determine_applicable<'a>(
        &'a self,
        table: &'a <TupleVariationHeader<'data, T> as PeakTuple<'data>>::Table,
        instance: &'a OwnedTuple,
    ) -> impl Iterator<Item = (f32, &'a TupleVariationHeader<'data, T>)> + 'a
    where
        TupleVariationHeader<'data, T>: PeakTuple<'data>,
    {
        // Ok, now we have our tuple we need to get the relevant glyph variation records
        //
        // > The tuple variation headers within the selected glyph variation data table will each
        // > specify a particular region of applicability within the font’s variation space. These will
        // > be compared with the coordinates for the selected variation instance to determine which of
        // > the tuple-variation data tables are applicable, and to calculate a scalar value for each.
        // > These comparisons and scalar calculations are done using normalized-scale coordinate values.
        // >
        // > The tuple variation headers within the selected glyph variation data table will each
        // > specify a particular region of applicability within the font’s variation space. These will
        // > be compared with the coordinates for the selected variation instance to determine which of
        // > the tuple-variation data tables are applicable, and to calculate a scalar value for each.
        // > These comparisons and scalar calculations are done using normalized-scale coordinate
        // > values.For each of the tuple-variation data tables that are applicable, the point number and
        // > delta data will be unpacked and processed. The data for applicable regions can be processed
        // > in any order. Derived delta values will correspond to particular point numbers derived from
        // > the packed point number data. For a given point number, the computed scalar is applied to
        // > the X coordinate and Y coordinate deltas as a coefficient, and then resulting delta
        // > adjustments applied to the X and Y coordinates of the point.

        // Determine which ones are applicable and return the scalar value for each one
        self.headers().filter_map(move |header| {
            // https://learn.microsoft.com/en-us/typography/opentype/spec/otvaroverview#algorithm-for-interpolation-of-instance-values
            let peak_coords = header.peak_tuple(table).ok()?;
            let (start_coords, end_coords) = match header.intermediate_region() {
                // NOTE(clone): Cheap as ReadTuple just contains ReadArray
                Some((start, end)) => (
                    Coordinates::Tuple(start.clone()),
                    Coordinates::Tuple(end.clone()),
                ),
                None => {
                    let mut start_coords = tiny_vec!();
                    let mut end_coords = tiny_vec!();
                    for peak in peak_coords.0.iter() {
                        match peak.raw_value().signum() {
                            // region is from peak to zero
                            -1 => {
                                start_coords.push(peak);
                                end_coords.push(F2Dot14::from(0));
                            }
                            // When a delta is provided for a region defined by n-tuples that have
                            // a peak value of 0 for some axis, then that axis does not factor into
                            // scalar calculations.
                            0 => {
                                start_coords.push(peak);
                                end_coords.push(peak);
                            }
                            // region is from zero to peak
                            1 => {
                                start_coords.push(F2Dot14::from(0));
                                end_coords.push(peak);
                            }
                            _ => unreachable!("unknown value from signum"),
                        }
                    }
                    (
                        Coordinates::Array(start_coords),
                        Coordinates::Array(end_coords),
                    )
                }
            };

            // Now determine the scalar:
            //
            // > In calculation of scalars (S, AS) and of interpolated values (scaledDelta,
            // > netAdjustment, interpolatedValue), at least 16 fractional bits of precision should
            // > be maintained.
            let scalar = start_coords
                .iter()
                .zip(end_coords.iter())
                .zip(instance.iter().copied())
                .zip(peak_coords.0.iter())
                .map(|(((start, end), instance), peak)| {
                    calculate_scalar(instance, start, peak, end)
                })
                .fold(1., |scalar, axis_scalar| scalar * axis_scalar);

            (scalar != 0.).then_some((scalar, header))
        })
    }
}

impl TupleVariationStore<'_, Gvar> {
    /// Retrieve the variation data for the variation tuple at the given index.
    pub fn variation_data(&self, index: u16) -> Result<GvarVariationData<'_>, ParseError> {
        let header = self
            .tuple_variation_headers
            .get(usize::from(index))
            .ok_or(ParseError::BadIndex)?;
        header.variation_data(
            NumPoints::from_raw(self.num_points),
            self.shared_point_numbers(),
        )
    }
}

impl<T> ReadBinaryDep for TupleVariationStore<'_, T> {
    type Args<'a> = (u16, u32, ReadScope<'a>);
    type HostType<'a> = TupleVariationStore<'a, T>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (axis_count, num_points, table_scope): (u16, u32, ReadScope<'a>),
    ) -> Result<Self::HostType<'a>, ParseError> {
        let axis_count = usize::from(axis_count);
        let tuple_variation_flags_and_count = ctxt.read_u16be()?;
        let tuple_variation_count = usize::from(tuple_variation_flags_and_count & Self::COUNT_MASK);
        let data_offset = ctxt.read_u16be()?;

        // Now read the TupleVariationHeaders
        let mut tuple_variation_headers = (0..tuple_variation_count)
            .map(|_| ctxt.read_dep::<TupleVariationHeader<'_, T>>(axis_count))
            .collect::<Result<Vec<_>, _>>()?;

        // Read the serialized data for each tuple variation header
        let mut data_ctxt = table_scope.offset(usize::from(data_offset)).ctxt();

        // Read shared point numbers if the flag indicates they are present
        let shared_point_numbers = ((tuple_variation_flags_and_count & Self::SHARED_POINT_NUMBERS)
            == Self::SHARED_POINT_NUMBERS)
            .then(|| read_packed_point_numbers(&mut data_ctxt, num_points))
            .transpose()?;

        // Populate the data slices on the headers
        for header in tuple_variation_headers.iter_mut() {
            header.data = data_ctxt.read_slice(header.variation_data_size.into())?;
        }

        Ok(TupleVariationStore {
            num_points,
            shared_point_numbers,
            tuple_variation_headers,
        })
    }
}

impl PointNumbers {
    /// Flag indicating the data type used for point numbers in this run.
    ///
    /// If set, the point numbers are stored as unsigned 16-bit values (uint16);
    /// if clear, the point numbers are stored as unsigned bytes (uint8).
    const POINTS_ARE_WORDS: u8 = 0x80;

    /// Mask for the low 7 bits of the control byte to give the number of point
    /// number elements, minus 1.
    const POINT_RUN_COUNT_MASK: u8 = 0x7F;

    /// Returns the number of point numbers contained by this value
    pub fn len(&self) -> usize {
        match self {
            PointNumbers::All(n) => usize::safe_from(*n),
            PointNumbers::Specific(vec) => vec.len(),
        }
    }

    /// Iterate over the point numbers contained by this value.
    fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.len()).map(move |index| {
            match self {
                // NOTE(cast): Safe as len is from `n`, which is a u32
                PointNumbers::All(_n) => index as u32,
                // NOTE(unwrap): Safe as index is bounded by `len`
                PointNumbers::Specific(numbers) => {
                    numbers.get(index).copied().map(u32::from).unwrap()
                }
            }
        })
    }
}

/// Read packed point numbers for a glyph with `num_points` points.
///
/// `num_points` is expected to already have the four "phantom points" added to
/// it.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-point-numbers>
fn read_packed_point_numbers(
    ctxt: &mut ReadCtxt<'_>,
    num_points: u32,
) -> Result<PointNumbers, ParseError> {
    let count = read_count(ctxt)?;
    // If the first byte is 0, then a second count byte is not used. This value has
    // a special meaning: the tuple variation data provides deltas for all glyph
    // points (including the “phantom” points), or for all CVTs.
    if count == 0 {
        return Ok(PointNumbers::All(num_points));
    }

    let mut num_read = 0;
    let mut point_numbers = Vec::with_capacity(usize::from(count));
    while num_read < count {
        let control_byte = ctxt.read_u8()?;
        let point_run_count = u16::from(control_byte & PointNumbers::POINT_RUN_COUNT_MASK) + 1;
        let last_point_number = point_numbers.last().copied().unwrap_or(0);
        if (control_byte & PointNumbers::POINTS_ARE_WORDS) == PointNumbers::POINTS_ARE_WORDS {
            // Points are words (2 bytes)
            let array = ctxt.read_array::<U16Be>(point_run_count.into())?;
            point_numbers.extend(array.iter().scan(last_point_number, |prev, diff| {
                let number = *prev + diff;
                *prev = number;
                Some(number)
            }));
        } else {
            // Points are single bytes
            let array = ctxt.read_array::<U8>(point_run_count.into())?;
            point_numbers.extend(array.iter().scan(last_point_number, |prev, diff| {
                let number = *prev + u16::from(diff);
                *prev = number;
                Some(number)
            }));
        }
        num_read += point_run_count;
    }
    Ok(PointNumbers::Specific(point_numbers))
}

// The count may be stored in one or two bytes:
//
// * If the first byte is 0, then a second count byte is not used. This value
//   has a special meaning: the tuple variation data provides deltas for all
//   glyph points (including the “phantom” points), or for all CVTs.
// * If the first byte is non-zero and the high bit is clear (value is 1 to
//   127), then a second count byte is not used. The point count is equal to the
//   value of the first byte.
// * If the high bit of the first byte is set, then a second byte is used. The
//   count is read from interpreting the two bytes as a big-endian uint16 value
//   with the high-order bit masked out.
fn read_count(ctxt: &mut ReadCtxt<'_>) -> Result<u16, ParseError> {
    let count1 = u16::from(ctxt.read_u8()?);
    let count = match count1 {
        0 => 0,
        1..=127 => count1,
        128.. => {
            let count2 = ctxt.read_u8()?;
            ((count1 & 0x7F) << 8) | u16::from(count2)
        }
    };
    Ok(count)
}

mod packed_deltas {

    use crate::binary::read::ReadCtxt;
    use crate::binary::{I16Be, I8};
    use crate::error::ParseError;
    use crate::SafeFrom;

    /// Flag indicating that this run contains no data (no explicit delta values
    /// are stored), and that the deltas for this run are all zero.
    const DELTAS_ARE_ZERO: u8 = 0x80;
    /// Flag indicating the data type for delta values in the run.
    ///
    /// If set, the run contains 16-bit signed deltas (int16); if clear, the run
    /// contains 8-bit signed deltas (int8).
    const DELTAS_ARE_WORDS: u8 = 0x40;
    /// Mask for the low 6 bits to provide the number of delta values in the
    /// run, minus one.
    const DELTA_RUN_COUNT_MASK: u8 = 0x3F;

    /// Read `num_deltas` packed deltas.
    ///
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
    pub(super) fn read(ctxt: &mut ReadCtxt<'_>, num_deltas: u32) -> Result<Vec<i16>, ParseError> {
        let mut deltas_read = 0;
        let mut deltas = Vec::with_capacity(usize::safe_from(num_deltas));

        while deltas_read < usize::safe_from(num_deltas) {
            let control_byte = ctxt.read_u8()?;
            let count = usize::from(control_byte & DELTA_RUN_COUNT_MASK) + 1; // value is stored - 1
            deltas.reserve(count);
            if (control_byte & DELTAS_ARE_ZERO) == DELTAS_ARE_ZERO {
                deltas.extend(std::iter::repeat_n(0, count));
            } else if (control_byte & DELTAS_ARE_WORDS) == DELTAS_ARE_WORDS {
                // Points are words (2 bytes)
                let array = ctxt.read_array::<I16Be>(count)?;
                deltas.extend(array.iter())
            } else {
                // Points are single bytes
                let array = ctxt.read_array::<I8>(count)?;
                deltas.extend(array.iter().map(i16::from));
            };
            deltas_read += count;
        }

        Ok(deltas)
    }
}

impl GvarVariationData<'_> {
    /// Iterates over the point numbers and (x, y) deltas.
    pub fn iter(&self) -> impl Iterator<Item = (u32, (i16, i16))> + '_ {
        let deltas = self
            .x_coord_deltas
            .iter()
            .copied()
            .zip(self.y_coord_deltas.iter().copied());
        self.point_numbers.iter().zip(deltas)
    }

    /// Returns the number of point numbers.
    pub fn len(&self) -> usize {
        self.point_numbers.len()
    }
}

impl CvarVariationData<'_> {
    /// Iterates over the cvt indexes and deltas.
    pub fn iter(&self) -> impl Iterator<Item = (u32, i16)> + '_ {
        self.point_numbers.iter().zip(self.deltas.iter().copied())
    }

    /// Returns the number of cvt indexes.
    pub fn len(&self) -> usize {
        self.point_numbers.len()
    }
}

impl<'data> TupleVariationHeader<'data, Gvar> {
    /// Read the variation data for `gvar`.
    ///
    /// `num_points` is the number of points in the glyph this variation relates
    /// to.
    pub fn variation_data<'a>(
        &'a self,
        num_points: NumPoints,
        shared_point_numbers: Option<SharedPointNumbers<'a>>,
    ) -> Result<GvarVariationData<'a>, ParseError> {
        let mut ctxt = ReadScope::new(self.data).ctxt();

        let point_numbers =
            self.read_point_numbers(&mut ctxt, num_points.get(), shared_point_numbers)?;
        let num_deltas = u32::try_from(point_numbers.len()).map_err(ParseError::from)?;

        // The deltas are stored X, followed by Y but the delta runs can span the
        // boundary of the two so they need to be read as a single span of
        // packed deltas and then split.
        let mut x_coord_deltas = packed_deltas::read(&mut ctxt, 2 * num_deltas)?;
        let y_coord_deltas = x_coord_deltas.split_off(usize::safe_from(num_deltas));

        Ok(GvarVariationData {
            point_numbers,
            x_coord_deltas,
            y_coord_deltas,
        })
    }

    /// Returns the index of the shared tuple that this header relates to.
    ///
    /// The tuple index is an index into the shared tuples of the `Gvar` table.
    /// Pass this value to the [shared_tuple](gvar::GvarTable::shared_tuple)
    /// method to retrieve the tuple.
    ///
    /// The value returned from this method will be `None` if the header has an
    /// embedded peak tuple.
    pub fn tuple_index(&self) -> Option<u16> {
        self.peak_tuple
            .is_none()
            .then_some(self.tuple_flags_and_index & Self::TUPLE_INDEX_MASK)
    }

    /// Returns the peak tuple for this tuple variation record.
    ///
    /// If the record contains an embedded peak tuple then that is returned,
    /// otherwise the referenced shared peak tuple is returned.
    pub fn peak_tuple<'a>(
        &'a self,
        gvar: &'a GvarTable<'data>,
    ) -> Result<ReadTuple<'data>, ParseError> {
        match self.peak_tuple.as_ref() {
            // NOTE(clone): cheap as ReadTuple is just a wrapper around ReadArray
            Some(tuple) => Ok(tuple.clone()),
            None => {
                let shared_index = self.tuple_flags_and_index & Self::TUPLE_INDEX_MASK;
                gvar.shared_tuple(shared_index)
            }
        }
    }
}

impl<'data> PeakTuple<'data> for TupleVariationHeader<'data, Gvar> {
    type Table = GvarTable<'data>;

    fn peak_tuple<'a>(&'a self, table: &'a Self::Table) -> Result<ReadTuple<'data>, ParseError> {
        self.peak_tuple(table)
    }
}

impl<'data> TupleVariationHeader<'data, Cvar> {
    /// Read the variation data for `cvar`.
    ///
    /// `num_cvts` is the number of CVTs in the CVT table.
    fn variation_data<'a>(
        &'a self,
        num_cvts: u32,
        shared_point_numbers: Option<SharedPointNumbers<'a>>,
    ) -> Result<CvarVariationData<'a>, ParseError> {
        let mut ctxt = ReadScope::new(self.data).ctxt();

        let point_numbers = self.read_point_numbers(&mut ctxt, num_cvts, shared_point_numbers)?;
        let num_deltas = u32::try_from(point_numbers.len()).map_err(ParseError::from)?;
        let deltas = packed_deltas::read(&mut ctxt, num_deltas)?;

        Ok(CvarVariationData {
            point_numbers,
            deltas,
        })
    }

    /// Returns the embedded peak tuple if present.
    ///
    /// The peak tuple is meant to always be present in `cvar` tuple variations,
    /// so `None` indicates an invalid font.
    pub fn peak_tuple(&self) -> Option<ReadTuple<'data>> {
        self.peak_tuple.clone()
    }
}

impl<'data> PeakTuple<'data> for TupleVariationHeader<'data, Cvar> {
    type Table = CvarTable<'data>;

    fn peak_tuple<'a>(&'a self, _table: &'a Self::Table) -> Result<ReadTuple<'data>, ParseError> {
        self.peak_tuple().ok_or(ParseError::MissingValue)
    }
}

impl<'data, T> TupleVariationHeader<'data, T> {
    /// Flag indicating that this tuple variation header includes an embedded
    /// peak tuple record, immediately after the tupleIndex field.
    ///
    /// If set, the low 12 bits of the tupleIndex value are ignored.
    ///
    /// Note that this must always be set within the `cvar` table.
    const EMBEDDED_PEAK_TUPLE: u16 = 0x8000;

    /// Flag indicating that this tuple variation table applies to an
    /// intermediate region within the variation space.
    ///
    /// If set, the header includes the two intermediate-region, start and end
    /// tuple records, immediately after the peak tuple record (if present).
    const INTERMEDIATE_REGION: u16 = 0x4000;

    /// Flag indicating that the serialized data for this tuple variation table
    /// includes packed “point” number data.
    ///
    /// If set, this tuple variation table uses that number data; if clear, this
    /// tuple variation table uses shared number data found at the start of
    /// the serialized data for this glyph variation data or 'cvar' table.
    const PRIVATE_POINT_NUMBERS: u16 = 0x2000;

    /// Mask for the low 12 bits to give the shared tuple records index.
    const TUPLE_INDEX_MASK: u16 = 0x0FFF;

    /// Read the point numbers for this tuple.
    ///
    /// This method will return either the embedded private point numbers or the
    /// shared numbers if private points are not present.
    fn read_point_numbers<'a>(
        &'a self,
        ctxt: &mut ReadCtxt<'data>,
        num_points: u32,
        shared_point_numbers: Option<SharedPointNumbers<'a>>,
    ) -> Result<Cow<'a, PointNumbers>, ParseError> {
        // Read private point numbers if the flag indicates they are present
        let private_point_numbers = if (self.tuple_flags_and_index & Self::PRIVATE_POINT_NUMBERS)
            == Self::PRIVATE_POINT_NUMBERS
        {
            read_packed_point_numbers(ctxt, num_points).map(Some)?
        } else {
            None
        };

        // If there are private point numbers then we need to read that many points
        // otherwise we need to read as many points are specified by the shared points.
        //
        // Either private or shared point numbers should be present. If both are missing
        // that's invalid.
        private_point_numbers
            .map(Cow::Owned)
            .or_else(|| shared_point_numbers.map(|shared| Cow::Borrowed(shared.0)))
            .ok_or(ParseError::MissingValue)
    }

    /// Returns the intermediate region of the tuple variation space that this
    /// variation applies to.
    ///
    /// If an intermediate region is not specified (the region is implied by the
    /// peak tuple) then this will be `None`.
    pub fn intermediate_region(&self) -> Option<(ReadTuple<'data>, ReadTuple<'data>)> {
        // NOTE(clone): Cheap as ReadTuple just contains ReadArray
        self.intermediate_region.clone()
    }
}

impl<T> ReadBinaryDep for TupleVariationHeader<'_, T> {
    type Args<'a> = usize;
    type HostType<'a> = TupleVariationHeader<'a, T>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        axis_count: usize,
    ) -> Result<Self::HostType<'a>, ParseError> {
        // The size in bytes of the serialized data for this tuple variation table.
        let variation_data_size = ctxt.read_u16be()?;
        // A packed field. The high 4 bits are flags. The low 12 bits are an index into
        // a shared tuple records array.
        let tuple_flags_and_index = ctxt.read_u16be()?;
        // If this is absent then `tuple_flags_and_index` contains the index to one of
        // the shared tuple records to use instead:
        //
        // > Every tuple variation table has a peak n-tuple indicated either by an
        // > embedded tuple
        // > record (always true in the 'cvar' table) or by an index into a shared tuple
        // > records
        // > array (only in the 'gvar' table).
        let peak_tuple = ((tuple_flags_and_index & Self::EMBEDDED_PEAK_TUPLE)
            == Self::EMBEDDED_PEAK_TUPLE)
            .then(|| ctxt.read_array(axis_count).map(ReadTuple))
            .transpose()?;
        let intermediate_region =
            if (tuple_flags_and_index & Self::INTERMEDIATE_REGION) == Self::INTERMEDIATE_REGION {
                let start = ctxt.read_array(axis_count).map(ReadTuple)?;
                let end = ctxt.read_array(axis_count).map(ReadTuple)?;
                Some((start, end))
            } else {
                None
            };
        Ok(TupleVariationHeader {
            variation_data_size,
            tuple_flags_and_index,
            peak_tuple,
            intermediate_region,
            data: &[], // filled in later
            variant: PhantomData,
        })
    }
}

impl fmt::Debug for TupleVariationHeader<'_, Gvar> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug_struct = f.debug_struct("TupleVariationHeader");
        match &self.peak_tuple {
            Some(peak) => debug_struct.field("peak_tuple", peak),
            None => debug_struct.field("shared_tuple_index", &self.tuple_index()),
        };
        debug_struct
            .field("intermediate_region", &self.intermediate_region)
            .finish()
    }
}

impl<'a> ItemVariationStore<'a> {
    /// Retrieve the scaled delta adjustment at the supplied `delta_set_entry` according to the
    /// user tuple `instance`.
    pub fn adjustment(
        &self,
        delta_set_entry: DeltaSetIndexMapEntry,
        instance: &OwnedTuple,
    ) -> Result<f32, ParseError> {
        let item_variation_data = self
            .item_variation_data
            .get(usize::from(delta_set_entry.outer_index))
            .ok_or(ParseError::BadIndex)?;
        let delta_set = item_variation_data
            .delta_set(delta_set_entry.inner_index)
            .ok_or(ParseError::BadIndex)?;

        let mut adjustment = 0.;
        for (delta, region_index) in delta_set
            .iter()
            .zip(item_variation_data.region_indexes.iter())
        {
            let region = self
                .variation_region(region_index)
                .ok_or(ParseError::BadIndex)?;
            if let Some(scalar) = region.scalar(instance.iter().copied()) {
                adjustment += scalar * delta as f32;
            }
        }
        Ok(adjustment)
    }

    /// Iterate over the variation regions of the ItemVariationData at `index`.
    pub fn regions(
        &self,
        index: u16,
    ) -> Result<impl Iterator<Item = Result<VariationRegion<'a>, ParseError>> + '_, ParseError>
    {
        let item_variation_data = self
            .item_variation_data
            .get(usize::from(index))
            .ok_or(ParseError::BadIndex)?;
        Ok(item_variation_data
            .region_indexes
            .iter()
            .map(move |region_index| {
                self.variation_region(region_index)
                    .ok_or(ParseError::BadIndex)
            }))
    }

    fn variation_region(&self, region_index: u16) -> Option<VariationRegion<'a>> {
        let region_index = usize::from(region_index);
        self.variation_region_list
            .variation_regions
            .read_item(region_index)
            .ok()
    }

    /// Returns an owned version of `self`.
    pub fn try_to_owned(&self) -> Result<owned::ItemVariationStore, ParseError> {
        let item_variation_data = self
            .item_variation_data
            .iter()
            .map(|data| data.to_owned())
            .collect();
        Ok(owned::ItemVariationStore {
            variation_region_list: self.variation_region_list.try_to_owned()?,
            item_variation_data,
        })
    }
}

impl ReadBinary for ItemVariationStore<'_> {
    type HostType<'a> = ItemVariationStore<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let format = ctxt.read_u16be()?;
        ctxt.check(format == 1)?;
        let variation_region_list_offset = ctxt.read_u32be()?;
        let item_variation_data_count = ctxt.read_u16be()?;
        let item_variation_data_offsets =
            ctxt.read_array::<U32Be>(usize::from(item_variation_data_count))?;
        let variation_region_list = scope
            .offset(usize::safe_from(variation_region_list_offset))
            .read::<VariationRegionList<'_>>()?;
        let item_variation_data = item_variation_data_offsets
            .iter()
            .map(|offset| {
                scope
                    .offset(usize::safe_from(offset))
                    .read::<ItemVariationData<'_>>()
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ItemVariationStore {
            variation_region_list,
            item_variation_data,
        })
    }
}

impl VariationRegionList<'_> {
    fn try_to_owned(&self) -> Result<owned::VariationRegionList, ParseError> {
        let variation_regions = self
            .variation_regions
            .iter_res()
            .map(|region| region.map(|region| region.to_owned()))
            .collect::<Result<_, _>>()?;
        Ok(owned::VariationRegionList { variation_regions })
    }
}

impl WriteBinary<&Self> for ItemVariationStore<'_> {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, store: &Self) -> Result<Self::Output, WriteError> {
        U16Be::write(ctxt, 1u16)?; // format
        let variation_region_list_offset_placeholder = ctxt.placeholder::<U16Be, _>()?;
        U16Be::write(ctxt, u16::try_from(store.item_variation_data.len())?)?;
        let item_variation_data_offsets_placeholders =
            ctxt.placeholder_array::<U32Be, _>(store.item_variation_data.len())?;

        // Write out the VariationRegionList
        ctxt.write_placeholder(
            variation_region_list_offset_placeholder,
            u16::try_from(ctxt.bytes_written())?,
        )?;
        VariationRegionList::write(ctxt, &store.variation_region_list)?;

        // Write the ItemVariationData sub-tables
        for (offset_placeholder, variation_data) in item_variation_data_offsets_placeholders
            .into_iter()
            .zip(store.item_variation_data.iter())
        {
            ctxt.write_placeholder(offset_placeholder, u32::try_from(ctxt.bytes_written())?)?;
            ItemVariationData::write(ctxt, variation_data)?;
        }

        Ok(())
    }
}

impl ReadBinary for VariationRegionList<'_> {
    type HostType<'a> = VariationRegionList<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let axis_count = ctxt.read_u16be()?;
        let region_count = ctxt.read_u16be()?;
        // The high-order bit of the region_count field is reserved for future use,
        // and must be cleared.
        ctxt.check(region_count < 32768)?;
        let variation_regions = ctxt.read_array_dep(usize::from(region_count), axis_count)?;
        Ok(VariationRegionList { variation_regions })
    }
}

impl WriteBinary<&Self> for VariationRegionList<'_> {
    type Output = ();

    fn write<C: WriteContext>(
        ctxt: &mut C,
        region_list: &Self,
    ) -> Result<Self::Output, WriteError> {
        U16Be::write(ctxt, *region_list.variation_regions.args())?; // axis count
        U16Be::write(ctxt, u16::try_from(region_list.variation_regions.len())?)?; // region count
        for region in region_list.variation_regions.iter_res() {
            let region = region.map_err(|_| WriteError::BadValue)?;
            VariationRegion::write(ctxt, &region)?;
        }
        Ok(())
    }
}

// In general, variation deltas are, logically, signed 16-bit integers, and in
// most cases, they are applied to signed 16-bit values The LONG_WORDS flag
// should only be used in top-level tables that include 32-bit values that can
// be variable — currently, only the COLR table.
/// Delta data for variations.
pub struct DeltaSet<'a> {
    long_deltas: bool,
    word_data: &'a [u8],
    short_data: &'a [u8],
}

impl DeltaSet<'_> {
    fn iter(&self) -> impl Iterator<Item = i32> + '_ {
        // NOTE(unwrap): Safe as `mid` is multiple of U32Be::SIZE
        let (short_size, long_size) = if self.long_deltas {
            (I16Be::SIZE, I32Be::SIZE)
        } else {
            (I8::SIZE, I16Be::SIZE)
        };
        let words = self.word_data.chunks(long_size).map(move |chunk| {
            if self.long_deltas {
                i32::from_be_bytes(chunk.try_into().unwrap())
            } else {
                i32::from(i16::from_be_bytes(chunk.try_into().unwrap()))
            }
        });
        let shorts = self.short_data.chunks(short_size).map(move |chunk| {
            if self.long_deltas {
                i32::from(i16::from_be_bytes(chunk.try_into().unwrap()))
            } else {
                i32::from(chunk[0] as i8)
            }
        });

        words.chain(shorts)
    }
}

trait DeltaSetT {
    /// Flag indicating that "word" deltas are long (int32)
    const LONG_WORDS: u16 = 0x8000;

    /// Count of "word" deltas
    const WORD_DELTA_COUNT_MASK: u16 = 0x7FFF;

    fn delta_sets(&self) -> &[u8];

    fn raw_word_delta_count(&self) -> u16;

    fn region_index_count(&self) -> u16;

    /// Retrieve a delta-set row within this item variation data sub-table.
    fn delta_set_impl(&self, index: u16) -> Option<DeltaSet<'_>> {
        let row_length = self.row_length();
        let row_data = self
            .delta_sets()
            .get(usize::from(index) * row_length..)
            .and_then(|offset| offset.get(..row_length))?;
        let mid = self.word_delta_count() * self.word_delta_size();
        if mid > row_data.len() {
            return None;
        }
        let (word_data, short_data) = row_data.split_at(mid);

        // Check that short data is a multiple of the short size
        if short_data.len() % self.short_delta_size() != 0 {
            return None;
        }

        Some(DeltaSet {
            long_deltas: self.long_deltas(),
            word_data,
            short_data,
        })
    }

    fn word_delta_count(&self) -> usize {
        usize::from(self.raw_word_delta_count() & Self::WORD_DELTA_COUNT_MASK)
    }

    fn long_deltas(&self) -> bool {
        self.raw_word_delta_count() & Self::LONG_WORDS != 0
    }

    fn row_length(&self) -> usize {
        calculate_row_length(self.region_index_count(), self.raw_word_delta_count())
    }

    fn word_delta_size(&self) -> usize {
        if self.long_deltas() {
            I32Be::SIZE
        } else {
            I16Be::SIZE
        }
    }

    fn short_delta_size(&self) -> usize {
        if self.long_deltas() {
            I16Be::SIZE
        } else {
            U8::SIZE
        }
    }
}

fn calculate_row_length(region_index_count: u16, raw_word_delta_count: u16) -> usize {
    let row_length = usize::from(region_index_count)
        + usize::from(raw_word_delta_count & ItemVariationData::WORD_DELTA_COUNT_MASK);
    if raw_word_delta_count & ItemVariationData::LONG_WORDS == 0 {
        row_length
    } else {
        row_length * 2
    }
}

impl DeltaSetT for ItemVariationData<'_> {
    fn delta_sets(&self) -> &[u8] {
        self.delta_sets
    }

    fn raw_word_delta_count(&self) -> u16 {
        self.word_delta_count
    }

    fn region_index_count(&self) -> u16 {
        self.region_index_count
    }
}

impl ItemVariationData<'_> {
    /// Retrieve the set of deltas at the supplied `index`.
    pub fn delta_set(&self, index: u16) -> Option<DeltaSet<'_>> {
        self.delta_set_impl(index)
    }

    fn to_owned(&self) -> owned::ItemVariationData {
        owned::ItemVariationData {
            word_delta_count: self.word_delta_count,
            region_index_count: self.region_index_count(),
            region_indexes: self.region_indexes.to_vec(),
            delta_sets: Box::from(self.delta_sets),
        }
    }
}

impl ReadBinary for ItemVariationData<'_> {
    type HostType<'a> = ItemVariationData<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let item_count = ctxt.read_u16be()?;
        let word_delta_count = ctxt.read_u16be()?;
        let region_index_count = ctxt.read_u16be()?;
        let region_indexes = ctxt.read_array::<U16Be>(usize::from(region_index_count))?;
        let row_length = calculate_row_length(region_index_count, word_delta_count);
        let delta_sets = ctxt.read_slice(usize::from(item_count) * row_length)?;

        Ok(ItemVariationData {
            item_count,
            word_delta_count,
            region_index_count,
            region_indexes,
            delta_sets,
        })
    }
}

impl WriteBinary<&Self> for ItemVariationData<'_> {
    type Output = ();

    fn write<C: WriteContext>(
        ctxt: &mut C,
        variation_data: &Self,
    ) -> Result<Self::Output, WriteError> {
        U16Be::write(ctxt, variation_data.item_count)?;
        U16Be::write(ctxt, variation_data.word_delta_count)?;
        U16Be::write(ctxt, u16::try_from(variation_data.region_indexes.len())?)?;
        ctxt.write_array(&variation_data.region_indexes)?;
        ctxt.write_bytes(variation_data.delta_sets)?;
        Ok(())
    }
}

impl VariationRegion<'_> {
    pub(crate) fn scalar(&self, tuple: impl Iterator<Item = F2Dot14>) -> Option<f32> {
        scalar(self.region_axes.iter(), tuple)
    }

    fn to_owned(&self) -> owned::VariationRegion {
        owned::VariationRegion {
            region_axes: self.region_axes.to_vec(),
        }
    }
}

pub(crate) fn scalar(
    region_axes: impl Iterator<Item = RegionAxisCoordinates>,
    tuple: impl Iterator<Item = F2Dot14>,
) -> Option<f32> {
    let scalar = region_axes
        .zip(tuple)
        .map(|(region, instance)| {
            let RegionAxisCoordinates {
                start_coord: start,
                peak_coord: peak,
                end_coord: end,
            } = region;
            calculate_scalar(instance, start, peak, end)
        })
        .fold(1., |scalar, axis_scalar| scalar * axis_scalar);

    (scalar != 0.).then_some(scalar)
}

fn calculate_scalar(instance: F2Dot14, start: F2Dot14, peak: F2Dot14, end: F2Dot14) -> f32 {
    // If peak is zero or not contained by the region of applicability then it does
    // not apply
    if peak == F2Dot14::from(0) {
        // If the peak is zero for some axis, then ignore the axis.
        1.
    } else if (start..=end).contains(&instance) {
        // The region is applicable: calculate a per-axis scalar as a proportion
        // of the proximity of the instance to the peak within the region.
        if instance == peak {
            1.
        } else if instance < peak {
            (f32::from(instance) - f32::from(start)) / (f32::from(peak) - f32::from(start))
        } else {
            // instance > peak
            (f32::from(end) - f32::from(instance)) / (f32::from(end) - f32::from(peak))
        }
    } else {
        // If the instance coordinate is out of range for some axis, then the region and
        // its associated deltas are not applicable.
        0.
    }
}

impl ReadBinaryDep for VariationRegion<'_> {
    type Args<'a> = u16;
    type HostType<'a> = VariationRegion<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        axis_count: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let region_axes = ctxt.read_array(usize::from(axis_count))?;
        Ok(VariationRegion { region_axes })
    }
}

impl ReadFixedSizeDep for VariationRegion<'_> {
    fn size(axis_count: u16) -> usize {
        usize::from(axis_count) * RegionAxisCoordinates::SIZE
    }
}

impl WriteBinary<&Self> for VariationRegion<'_> {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, region: &Self) -> Result<Self::Output, WriteError> {
        ctxt.write_array(&region.region_axes)
    }
}

impl ReadFrom for RegionAxisCoordinates {
    type ReadType = (F2Dot14, F2Dot14, F2Dot14);

    fn read_from((start_coord, peak_coord, end_coord): (F2Dot14, F2Dot14, F2Dot14)) -> Self {
        RegionAxisCoordinates {
            start_coord,
            peak_coord,
            end_coord,
        }
    }
}

impl WriteBinary for RegionAxisCoordinates {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, coords: Self) -> Result<Self::Output, WriteError> {
        F2Dot14::write(ctxt, coords.start_coord)?;
        F2Dot14::write(ctxt, coords.peak_coord)?;
        F2Dot14::write(ctxt, coords.end_coord)?;
        Ok(())
    }
}

impl DeltaSetIndexMap<'_> {
    /// Mask for the low 4 bits of the DeltaSetIndexMap entry format.
    ///
    /// Gives the count of bits minus one that are used in each entry for the
    /// inner-level index.
    const INNER_INDEX_BIT_COUNT_MASK: u8 = 0x0F;

    /// Mask for bits of the DeltaSetIndexMap entry format that indicate the
    /// size in bytes minus one of each entry.
    const MAP_ENTRY_SIZE_MASK: u8 = 0x30;

    /// Returns delta-set outer-level index and inner-level index combination.
    pub fn entry(&self, mut i: u32) -> Result<DeltaSetIndexMapEntry, ParseError> {
        // If an index into the mapping array is used that is greater than or equal to mapCount,
        // then the last logical entry of the mapping array is used.
        //
        // https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data
        if i >= self.map_count {
            i = self.map_count.checked_sub(1).ok_or(ParseError::BadIndex)?;
        }

        let entry_size = usize::from(self.entry_size());
        let offset = usize::safe_from(i) * entry_size;
        let entry_bytes = self
            .map_data
            .get(offset..(offset + entry_size))
            .ok_or(ParseError::BadIndex)?;

        // entry can be 1, 2, 3, or 4 bytes
        let entry = entry_bytes
            .iter()
            .copied()
            .fold(0u32, |entry, byte| (entry << 8) | u32::from(byte));
        let outer_index =
            (entry >> (u32::from(self.entry_format & Self::INNER_INDEX_BIT_COUNT_MASK) + 1)) as u16;
        let inner_index = (entry
            & ((1 << (u32::from(self.entry_format & Self::INNER_INDEX_BIT_COUNT_MASK) + 1)) - 1))
            as u16;

        Ok(DeltaSetIndexMapEntry {
            outer_index,
            inner_index,
        })
    }

    /// Returns the number of entries in this map.
    pub fn len(&self) -> usize {
        usize::safe_from(self.map_count)
    }

    /// The size of an entry in bytes
    fn entry_size(&self) -> u8 {
        Self::entry_size_impl(self.entry_format)
    }

    fn entry_size_impl(entry_format: u8) -> u8 {
        ((entry_format & Self::MAP_ENTRY_SIZE_MASK) >> 4) + 1
    }
}

impl ReadBinary for DeltaSetIndexMap<'_> {
    type HostType<'a> = DeltaSetIndexMap<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let format = ctxt.read_u8()?;
        let entry_format = ctxt.read_u8()?;
        let map_count = match format {
            0 => ctxt.read_u16be().map(u32::from)?,
            1 => ctxt.read_u32be()?,
            _ => return Err(ParseError::BadVersion),
        };
        let entry_size = DeltaSetIndexMap::entry_size_impl(entry_format);
        let map_size = usize::from(entry_size) * usize::safe_from(map_count);
        let map_data = ctxt.read_slice(map_size)?;

        Ok(DeltaSetIndexMap {
            entry_format,
            map_count,
            map_data,
        })
    }
}

impl fmt::Debug for DeltaSetIndexMap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let DeltaSetIndexMap {
            entry_format,
            map_count,
            map_data,
        } = self;
        f.debug_struct("DeltaSetIndexMap")
            .field("entry_format", entry_format)
            .field("map_count", map_count)
            .field("map_data", &DebugData(map_data))
            .finish()
    }
}

enum Coordinates<'a> {
    Tuple(ReadTuple<'a>),
    Array(TinyVec<[F2Dot14; 4]>),
}

struct CoordinatesIter<'a, 'data> {
    coords: &'a Coordinates<'data>,
    index: usize,
}

impl<'data> Coordinates<'data> {
    pub fn iter(&self) -> CoordinatesIter<'_, 'data> {
        CoordinatesIter {
            coords: self,
            index: 0,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Coordinates::Tuple(coords) => coords.0.len(),
            Coordinates::Array(coords) => coords.len(),
        }
    }
}

impl Iterator for CoordinatesIter<'_, '_> {
    type Item = F2Dot14;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.coords.len() {
            return None;
        }

        let index = self.index;
        self.index += 1;
        match self.coords {
            Coordinates::Tuple(coords) => coords.0.get_item(index),
            Coordinates::Array(coords) => Some(coords[index]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::read::ReadScope;

    #[test]
    fn test_read_count() {
        let mut ctxt = ReadScope::new(&[0]).ctxt();
        assert_eq!(read_count(&mut ctxt).unwrap(), 0);
        let mut ctxt = ReadScope::new(&[0x32]).ctxt();
        assert_eq!(read_count(&mut ctxt).unwrap(), 50);
        let mut ctxt = ReadScope::new(&[0x81, 0x22]).ctxt();
        assert_eq!(read_count(&mut ctxt).unwrap(), 290);
    }

    #[test]
    fn test_read_packed_point_numbers() {
        let data = [0x0d, 0x0c, 1, 4, 4, 2, 1, 2, 3, 3, 2, 1, 1, 3, 4];
        let mut ctxt = ReadScope::new(&data).ctxt();

        let expected = vec![1, 5, 9, 11, 12, 14, 17, 20, 22, 23, 24, 27, 31];
        assert_eq!(
            read_packed_point_numbers(&mut ctxt, expected.len() as u32)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn test_read_packed_deltas() {
        let data = [
            0x03, 0x0A, 0x97, 0x00, 0xC6, 0x87, 0x41, 0x10, 0x22, 0xFB, 0x34,
        ];
        let mut ctxt = ReadScope::new(&data).ctxt();
        let expected = vec![10, -105, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 4130, -1228];
        assert_eq!(
            packed_deltas::read(&mut ctxt, expected.len() as u32).unwrap(),
            expected
        );
    }
}
