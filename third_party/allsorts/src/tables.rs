//! OpenType font table parsing and writing.

pub mod aat;
pub mod cmap;
pub mod colr;
pub mod cpal;
pub mod gasp;
pub mod glyf;
pub mod kern;
pub mod loca;
pub mod morx;
pub mod os2;
pub mod svg;
pub mod variable_fonts;

use std::borrow::Cow;
use std::fmt::{self, Formatter};

use encoding_rs::Encoding;

use crate::binary::read::{
    CheckIndex, ReadArray, ReadArrayCow, ReadBinary, ReadBinaryDep, ReadCtxt, ReadFrom, ReadScope,
    ReadUnchecked,
};
use crate::binary::write::{Placeholder, WriteBinary, WriteContext};
use crate::binary::{I16Be, I32Be, I64Be, U16Be, U32Be};
use crate::error::{ParseError, WriteError};
use crate::tag;
use crate::{size, SafeFrom};

/// Magic value identifying a CFF font (`OTTO`)
pub const CFF_MAGIC: u32 = tag::OTTO;

/// Magic number identifying TrueType 1.0
///
/// The version number 1.0 as a 16.16 fixed-point value, indicating TrueType glyph data.
pub const TTF_MAGIC: u32 = 0x00010000;

/// Magic number identifying TrueType (Apple version)
pub const TRUE_MAGIC: u32 = tag::TRUE;

/// Magic value identifying a TrueType font collection `ttcf`
pub const TTCF_MAGIC: u32 = tag::TTCF;

/// 32-bit signed fixed-point number (16.16)
///
/// The integer component is a signed 16-bit integer. The fraction is an unsigned
/// 16-bit numerator for denominator of 0xFFFF (65635). I.e scale of 1/65535.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Fixed(i32);

/// Date represented in number of seconds since 12:00 midnight, January 1, 1904
///
/// The value is represented as a signed 64-bit integer.
type LongDateTime = i64;

pub trait FontTableProvider {
    /// Return data for the specified table if present
    fn table_data(&self, tag: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError>;

    fn has_table(&self, tag: u32) -> bool;

    fn read_table_data(&self, tag: u32) -> Result<Cow<'_, [u8]>, ParseError> {
        self.table_data(tag)?.ok_or(ParseError::MissingTable(tag))
    }

    /// The tags of the tables within this font.
    ///
    /// Returns `None` if the tags cannot be determined.
    fn table_tags(&self) -> Option<Vec<u32>>;

}

pub trait SfntVersion {
    fn sfnt_version(&self) -> u32;
}

/// The F2DOT14 format consists of a signed, 2’s complement integer and an unsigned fraction.
///
/// To compute the actual value, take the integer and add the fraction.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct F2Dot14(i16);

/// The size of the offsets in the `loca` table
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/loca>
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndexToLocFormat {
    /// Offsets are 16-bit. The actual local offset divided by 2 is stored.
    Short,
    /// Offsets are 32-bit. The actual local offset is stored.
    Long,
}

#[derive(Clone)]
pub struct OpenTypeFont<'a> {
    pub scope: ReadScope<'a>,
    pub data: OpenTypeData<'a>,
}

/// An OpenTypeFont containing a single font or a collection of fonts
#[derive(Clone)]
pub enum OpenTypeData<'a> {
    Single(OffsetTable<'a>),
    Collection(TTCHeader<'a>),
}

/// TrueType collection header
#[derive(Clone)]
pub struct TTCHeader<'a> {
    pub major_version: u16,
    pub minor_version: u16,
    pub offset_tables: ReadArray<'a, U32Be>,
    // TODO add digital signature fields
}

/// OpenType Offset Table
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/otff#organization-of-an-opentype-font>
#[derive(Clone)]
pub struct OffsetTable<'a> {
    pub sfnt_version: u32,
    pub search_range: u16,
    pub entry_selector: u16,
    pub range_shift: u16,
    pub table_records: ReadArray<'a, TableRecord>,
}

pub struct OffsetTableFontProvider<'a> {
    scope: ReadScope<'a>,
    offset_table: OffsetTable<'a>,
}

/// An entry in the Offset Table
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/otff#organization-of-an-opentype-font>
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash)]
pub struct TableRecord {
    pub table_tag: u32,
    pub checksum: u32,
    pub offset: u32,
    pub length: u32,
}

/// `head` table
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/head>
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct HeadTable {
    pub major_version: u16,
    pub minor_version: u16,
    pub font_revision: Fixed,
    pub check_sum_adjustment: u32,
    pub magic_number: u32,
    pub flags: u16,
    pub units_per_em: u16,
    pub created: LongDateTime,
    pub modified: LongDateTime,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub mac_style: MacStyle,
    pub lowest_rec_ppem: u16,
    pub font_direction_hint: i16,
    pub index_to_loc_format: IndexToLocFormat,
    pub glyph_data_format: i16,
}

/// macStyle field in `head`
#[enumflags2::bitflags]
#[repr(u16)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(non_camel_case_types)]
pub enum MacStyleFlag {
    BOLD = 1 << 0,
    ITALIC = 1 << 1,
    UNDERLINE = 1 << 2,
    OUTLINE = 1 << 3,
    SHADOW = 1 << 4,
    CONDENSED = 1 << 5,
    EXTENDED = 1 << 6,
    // Bits 7–15: Reserved (set to 0).
}

pub type MacStyle = enumflags2::BitFlags<MacStyleFlag>;

/// `hhea` horizontal header table
///
/// > This table contains information for horizontal layout.
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/hhea>
///
/// This struct is also used for the `vhea` table.
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct HheaTable {
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
    pub min_left_side_bearing: i16,
    pub min_right_side_bearing: i16,
    pub x_max_extent: i16,
    pub caret_slope_rise: i16,
    pub caret_slope_run: i16,
    pub caret_offset: i16,
    pub num_h_metrics: u16,
}

/// `hmtx` horizontal metrics table
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx>
///
/// This struct is also used for `vmtx` table.
#[derive(Debug)]
pub struct HmtxTable<'a> {
    pub h_metrics: ReadArrayCow<'a, LongHorMetric>,
    pub left_side_bearings: ReadArrayCow<'a, I16Be>,
}

/// A `longHorMetric` record in the `hmtx` table.
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx>
///
/// This struct is also used for LongVerMetric `vmtx` table.
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct LongHorMetric {
    pub advance_width: u16,
    pub lsb: i16,
}

/// maxp - Maximum profile
///
/// This table establishes the memory requirements for this font. Fonts with CFF data must use
/// Version 0.5 of this table, specifying only the numGlyphs field. Fonts with TrueType outlines
/// must use Version 1.0 of this table, where all data is required.
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/maxp>
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct MaxpTable {
    pub num_glyphs: u16,
    /// Extra fields, present if maxp table is version 1.0, absent if version 0.5.
    pub version1_sub_table: Option<MaxpVersion1SubTable>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct MaxpVersion1SubTable {
    /// Maximum points in a non-composite glyph.
    pub max_points: u16,
    /// Maximum contours in a non-composite glyph.
    pub max_contours: u16,
    /// Maximum points in a composite glyph.
    pub max_composite_points: u16,
    /// Maximum contours in a composite glyph.
    pub max_composite_contours: u16,
    /// 1 if instructions do not use the twilight zone (Z0), or 2 if instructions do use Z0; should
    /// be set to 2 in most cases.
    pub max_zones: u16,
    /// Maximum points used in Z0.
    pub max_twilight_points: u16,
    /// Number of Storage Area locations.
    pub max_storage: u16,
    /// Number of FDEFs, equal to the highest function number + 1.
    pub max_function_defs: u16,
    /// Number of IDEFs.
    pub max_instruction_defs: u16,
    /// Maximum stack depth across Font Program ('fpgm' table), CVT Program ('prep' table) and all
    /// glyph instructions (in the 'glyf' table).
    pub max_stack_elements: u16,
    /// Maximum byte count for glyph instructions.
    pub max_size_of_instructions: u16,
    /// Maximum number of components referenced at “top level” for any composite glyph.
    pub max_component_elements: u16,
    /// Maximum levels of recursion; 1 for simple components.
    pub max_component_depth: u16,
}

/// `name` table
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/name>
pub struct NameTable<'a> {
    pub string_storage: ReadScope<'a>,
    pub name_records: ReadArray<'a, NameRecord>,
    /// Language tag records
    ///
    /// Present if `name` table version is 1 or newer. If present, language ids >= 0x8000 refer to
    /// language tag records with 0x8000 the first record, 0x8001 the second, etc.
    pub opt_langtag_records: Option<ReadArray<'a, LangTagRecord>>,
}

/// Record within the `name` table
#[derive(Debug)]
pub struct NameRecord {
    pub platform_id: u16,
    pub encoding_id: u16,
    pub language_id: u16,
    pub name_id: u16,
    pub length: u16,
    pub offset: u16,
}

/// Language-tag record within the `name` table
pub struct LangTagRecord {
    pub length: u16,
    pub offset: u16,
}

/// cvt — Control Value Table
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/cvt>
pub struct CvtTable<'a> {
    pub values: ReadArrayCow<'a, I16Be>,
}

impl<'a> OpenTypeFont<'a> {
    pub fn table_provider(&self, index: usize) -> Result<OffsetTableFontProvider<'a>, ParseError> {
        self.offset_table(index)
            .map(|offset_table| OffsetTableFontProvider {
                offset_table: offset_table.into_owned(),
                scope: self.scope,
            })
    }

    pub fn offset_table<'b>(
        &'b self,
        index: usize,
    ) -> Result<Cow<'b, OffsetTable<'a>>, ParseError> {
        match &self.data {
            OpenTypeData::Single(offset_table) => Ok(Cow::Borrowed(offset_table)),
            OpenTypeData::Collection(ttc) => {
                let offset = ttc
                    .offset_tables
                    .get_item(index)
                    .map(SafeFrom::safe_from)
                    .ok_or(ParseError::BadIndex)?;
                let offset_table = self.scope.offset(offset).read::<OffsetTable<'_>>()?;
                Ok(Cow::Owned(offset_table))
            }
        }
    }
}

impl ReadBinary for OpenTypeFont<'_> {
    type HostType<'a> = OpenTypeFont<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let mut peek = ctxt.clone();
        let magic = peek.read_u32be()?;
        match magic {
            TTF_MAGIC | TRUE_MAGIC | CFF_MAGIC => {
                let offset_table = ctxt.read::<OffsetTable<'_>>()?;
                let font = OpenTypeData::Single(offset_table);
                Ok(OpenTypeFont { scope, data: font })
            }
            TTCF_MAGIC => {
                let ttc_header = ctxt.read::<TTCHeader<'_>>()?;
                let font = OpenTypeData::Collection(ttc_header);
                Ok(OpenTypeFont { scope, data: font })
            }
            _ => Err(ParseError::BadVersion),
        }
    }
}

impl ReadBinary for TTCHeader<'_> {
    type HostType<'a> = TTCHeader<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let ttc_tag = ctxt.read_u32be()?;
        match ttc_tag {
            TTCF_MAGIC => {
                let major_version = ctxt.read_u16be()?;
                let minor_version = ctxt.read_u16be()?;
                ctxt.check(major_version == 1 || major_version == 2)?;
                let num_fonts = usize::try_from(ctxt.read_u32be()?)?;
                let offset_tables = ctxt.read_array::<U32Be>(num_fonts)?;
                // TODO read digital signature fields in TTCHeader version 2
                Ok(TTCHeader {
                    major_version,
                    minor_version,
                    offset_tables,
                })
            }
            _ => Err(ParseError::BadVersion),
        }
    }
}

impl ReadBinary for OffsetTable<'_> {
    type HostType<'a> = OffsetTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let sfnt_version = ctxt.read_u32be()?;
        match sfnt_version {
            TTF_MAGIC | TRUE_MAGIC | CFF_MAGIC => {
                let num_tables = ctxt.read_u16be()?;
                let search_range = ctxt.read_u16be()?;
                let entry_selector = ctxt.read_u16be()?;
                let range_shift = ctxt.read_u16be()?;
                let table_records = ctxt.read_array::<TableRecord>(usize::from(num_tables))?;
                Ok(OffsetTable {
                    sfnt_version,
                    search_range,
                    entry_selector,
                    range_shift,
                    table_records,
                })
            }
            _ => Err(ParseError::BadVersion),
        }
    }
}

// WEB-LIFT FIX (2026-06-02): plain big-endian reads from a byte slice. Used by the
// manual table-directory scan below. The remill/web lift mis-handles the
// `ReadArray<TableRecord>` read path (the nested-tuple `TableRecord` `read_dep` returns
// `table_tag = 0` for EVERY record — proven via a probe: tags[7]=0x0000 while the bytes
// at that offset are 0x68656164 'head' and `tag::HEAD`==0x6164), so the directory lookup
// never matches and the font fails to parse → text measures height 0. These hand-rolled
// indexed byte reads lift correctly.
#[inline]
fn be16(d: &[u8], o: usize) -> u32 {
    ((d[o] as u32) << 8) | (d[o + 1] as u32)
}
#[inline]
fn be32(d: &[u8], o: usize) -> u32 {
    ((d[o] as u32) << 24) | ((d[o + 1] as u32) << 16) | ((d[o + 2] as u32) << 8) | (d[o + 3] as u32)
}

impl OffsetTableFontProvider<'_> {
    /// Locate the table directory by hand for a single-font sfnt laid out at the start of
    /// `self.scope` (TTF/OTTO/'true'): returns `(dir_offset, num_tables)`. Returns `None`
    /// for TTC / unrecognised layouts so the caller falls back to the original
    /// `ReadArray`-based path. See the `be32` comment for why this exists.
    fn manual_dir(&self) -> Option<(usize, usize)> {
        let data = self.scope.data();
        if data.len() < 12 {
            return None;
        }
        match be32(data, 0) {
            // TTF_MAGIC | CFF_MAGIC (OTTO) | TRUE_MAGIC ('true')
            0x0001_0000 | 0x4F54_544F | 0x7472_7565 => Some((12, be16(data, 4) as usize)),
            _ => None,
        }
    }
}

impl FontTableProvider for OffsetTableFontProvider<'_> {
    fn table_data(&self, tag: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        if let Some((dir, num)) = self.manual_dir() {
            let data = self.scope.data();
            let mut i = 0;
            while i < num {
                let r = dir + i * 16;
                if r + 16 > data.len() {
                    break;
                }
                if be32(data, r) == tag {
                    let off = be32(data, r + 8) as usize;
                    let len = be32(data, r + 12) as usize;
                    return Ok(off
                        .checked_add(len)
                        .filter(|&e| e <= data.len())
                        .map(|e| Cow::Borrowed(&data[off..e])));
                }
                i += 1;
            }
            return Ok(None);
        }
        // Fallback (TTC / unrecognised): original ReadArray path.
        self.offset_table
            .read_table(&self.scope, tag)
            .map(|scope| scope.map(|scope| Cow::Borrowed(scope.data())))
    }

    fn has_table(&self, tag: u32) -> bool {
        self.table_data(tag).ok().flatten().is_some()
    }

    fn table_tags(&self) -> Option<Vec<u32>> {
        if let Some((dir, num)) = self.manual_dir() {
            let data = self.scope.data();
            let mut tags = Vec::with_capacity(num);
            let mut i = 0;
            while i < num {
                let r = dir + i * 16;
                if r + 4 > data.len() {
                    break;
                }
                tags.push(be32(data, r));
                i += 1;
            }
            return Some(tags);
        }
        // Fallback (TTC / unrecognised): original ReadArray path.
        let records = &self.offset_table.table_records;
        let n = records.len();
        let mut tags = Vec::with_capacity(n);
        let mut i = 0;
        while i < n {
            if let Ok(rec) = records.read_item(i) {
                tags.push(rec.table_tag);
            }
            i += 1;
        }
        Some(tags)
    }
}

impl SfntVersion for OffsetTableFontProvider<'_> {
    fn sfnt_version(&self) -> u32 {
        self.offset_table.sfnt_version
    }
}

impl ReadFrom for TableRecord {
    type ReadType = ((U32Be, U32Be), (U32Be, U32Be));
    fn read_from(((table_tag, checksum), (offset, length)): ((u32, u32), (u32, u32))) -> Self {
        TableRecord {
            table_tag,
            checksum,
            offset,
            length,
        }
    }
}

impl WriteBinary<&Self> for TableRecord {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &TableRecord) -> Result<(), WriteError> {
        U32Be::write(ctxt, table.table_tag)?;
        U32Be::write(ctxt, table.checksum)?;
        U32Be::write(ctxt, table.offset)?;
        U32Be::write(ctxt, table.length)?;

        Ok(())
    }
}

impl<'a> OffsetTable<'a> {
    pub fn find_table_record(&self, tag: u32) -> Option<TableRecord> {
        // Indexed loop instead of `.iter().find(closure)`: on the lifted/web backend the
        // ReadArray iterator (ReadArrayIter::next) and/or the find-closure mis-lift and
        // yield no match (same class as the css.rs map+collect element-drop). Indexed
        // `read_item` lifts correctly (it's what binary_search uses).
        let n = self.table_records.len();
        let mut i = 0;
        while i < n {
            if let Ok(rec) = self.table_records.read_item(i) {
                if rec.table_tag == tag {
                    return Some(rec);
                }
            }
            i += 1;
        }
        None
    }

    pub fn read_table(
        &self,
        scope: &ReadScope<'a>,
        tag: u32,
    ) -> Result<Option<ReadScope<'a>>, ParseError> {
        if let Some(table_record) = self.find_table_record(tag) {
            let table = table_record.read_table(scope)?;
            Ok(Some(table))
        } else {
            Ok(None)
        }
    }
}

impl TableRecord {
    pub const SIZE: usize = 4 * size::U32;

    pub fn read_table<'a>(&self, scope: &ReadScope<'a>) -> Result<ReadScope<'a>, ParseError> {
        let offset = usize::try_from(self.offset)?;
        let length = usize::try_from(self.length)?;
        scope.offset_length(offset, length)
    }
}

impl ReadBinary for HeadTable {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let major_version = ctxt.read::<U16Be>()?;
        let minor_version = ctxt.read::<U16Be>()?;
        let font_revision = ctxt.read::<Fixed>()?;
        let check_sum_adjustment = ctxt.read::<U32Be>()?;
        let magic_number = ctxt.read::<U32Be>()?;
        ctxt.check(magic_number == 0x5F0F3CF5)?;
        let flags = ctxt.read::<U16Be>()?;
        let units_per_em = ctxt.read::<U16Be>()?;
        let created = ctxt.read::<I64Be>()?;
        let modified = ctxt.read::<I64Be>()?;
        let x_min = ctxt.read::<I16Be>()?;
        let y_min = ctxt.read::<I16Be>()?;
        let x_max = ctxt.read::<I16Be>()?;
        let y_max = ctxt.read::<I16Be>()?;
        let mac_style = ctxt.read::<U16Be>().map(MacStyle::from_bits_truncate)?;
        let lowest_rec_ppem = ctxt.read::<U16Be>()?;
        let font_direction_hint = ctxt.read::<I16Be>()?;
        let index_to_loc_format = ctxt.read::<IndexToLocFormat>()?;
        let glyph_data_format = ctxt.read::<I16Be>()?;

        Ok(HeadTable {
            major_version,
            minor_version,
            font_revision,
            check_sum_adjustment,
            magic_number,
            flags,
            units_per_em,
            created,
            modified,
            x_min,
            y_min,
            x_max,
            y_max,
            mac_style,
            lowest_rec_ppem,
            font_direction_hint,
            index_to_loc_format,
            glyph_data_format,
        })
    }
}

impl WriteBinary<&Self> for HeadTable {
    type Output = Placeholder<U32Be, u32>;

    /// Writes the table to the `WriteContext` and returns a placeholder to the
    /// `check_sum_adjustment` field.
    ///
    /// The `check_sum_adjustment` field requires special handling to calculate. See:
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/head>
    fn write<C: WriteContext>(ctxt: &mut C, table: &HeadTable) -> Result<Self::Output, WriteError> {
        U16Be::write(ctxt, table.major_version)?;
        U16Be::write(ctxt, table.minor_version)?;
        Fixed::write(ctxt, table.font_revision)?;
        let check_sum_adjustment = ctxt.placeholder()?;
        U32Be::write(ctxt, table.magic_number)?;
        U16Be::write(ctxt, table.flags)?;
        U16Be::write(ctxt, table.units_per_em)?;
        I64Be::write(ctxt, table.created)?;
        I64Be::write(ctxt, table.modified)?;
        I16Be::write(ctxt, table.x_min)?;
        I16Be::write(ctxt, table.y_min)?;
        I16Be::write(ctxt, table.x_max)?;
        I16Be::write(ctxt, table.y_max)?;
        U16Be::write(ctxt, table.mac_style.bits())?;
        U16Be::write(ctxt, table.lowest_rec_ppem)?;
        I16Be::write(ctxt, table.font_direction_hint)?;
        IndexToLocFormat::write(ctxt, table.index_to_loc_format)?;
        I16Be::write(ctxt, table.glyph_data_format)?;

        Ok(check_sum_adjustment)
    }
}

impl HeadTable {
    // macStyle:
    // Bit 0: Bold (if set to 1);
    // Bit 1: Italic (if set to 1)
    // Bit 2: Underline (if set to 1)
    // Bit 3: Outline (if set to 1)
    // Bit 4: Shadow (if set to 1)
    // Bit 5: Condensed (if set to 1)
    // Bit 6: Extended (if set to 1)
    // Bits 7–15: Reserved (set to 0).
    // https://docs.microsoft.com/en-us/typography/opentype/spec/head
    pub fn is_bold(&self) -> bool {
        self.mac_style.contains(MacStyleFlag::BOLD)
    }

    pub fn is_italic(&self) -> bool {
        self.mac_style.contains(MacStyleFlag::ITALIC)
    }
}

impl ReadBinary for HheaTable {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let major_version = ctxt.read_u16be()?;
        let _minor_version = ctxt.read_u16be()?;
        ctxt.check(major_version == 1)?;
        let ascender = ctxt.read_i16be()?;
        let descender = ctxt.read_i16be()?;
        let line_gap = ctxt.read_i16be()?;
        let advance_width_max = ctxt.read_u16be()?;
        let min_left_side_bearing = ctxt.read_i16be()?;
        let min_right_side_bearing = ctxt.read_i16be()?;
        let x_max_extent = ctxt.read_i16be()?;
        let caret_slope_rise = ctxt.read_i16be()?;
        let caret_slope_run = ctxt.read_i16be()?;
        let caret_offset = ctxt.read_i16be()?;
        let _reserved1 = ctxt.read_i16be()?;
        let _reserved2 = ctxt.read_i16be()?;
        let _reserved3 = ctxt.read_i16be()?;
        let _reserved4 = ctxt.read_i16be()?;
        let metric_data_format = ctxt.read_i16be()?;
        ctxt.check(metric_data_format == 0)?;
        let num_h_metrics = ctxt.read_u16be()?;

        Ok(HheaTable {
            ascender,
            descender,
            line_gap,
            advance_width_max,
            min_left_side_bearing,
            min_right_side_bearing,
            x_max_extent,
            caret_slope_rise,
            caret_slope_run,
            caret_offset,
            num_h_metrics,
        })
    }
}

impl WriteBinary<&Self> for HheaTable {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &HheaTable) -> Result<(), WriteError> {
        U16Be::write(ctxt, 1u16)?; // major_version
        U16Be::write(ctxt, 0u16)?; // minor_version

        I16Be::write(ctxt, table.ascender)?;
        I16Be::write(ctxt, table.descender)?;
        I16Be::write(ctxt, table.line_gap)?;
        U16Be::write(ctxt, table.advance_width_max)?;
        I16Be::write(ctxt, table.min_left_side_bearing)?;
        I16Be::write(ctxt, table.min_right_side_bearing)?;
        I16Be::write(ctxt, table.x_max_extent)?;
        I16Be::write(ctxt, table.caret_slope_rise)?;
        I16Be::write(ctxt, table.caret_slope_run)?;
        I16Be::write(ctxt, table.caret_offset)?;

        I16Be::write(ctxt, 0i16)?; // reserved
        I16Be::write(ctxt, 0i16)?; // reserved
        I16Be::write(ctxt, 0i16)?; // reserved
        I16Be::write(ctxt, 0i16)?; // reserved

        I16Be::write(ctxt, 0i16)?; // metric_data_format

        U16Be::write(ctxt, table.num_h_metrics)?;

        Ok(())
    }
}

impl ReadBinaryDep for HmtxTable<'_> {
    type Args<'a> = (usize, usize); // num_glyphs, num_h_metrics
    type HostType<'a> = HmtxTable<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (num_glyphs, num_h_metrics): (usize, usize),
    ) -> Result<Self::HostType<'a>, ParseError> {
        let h_metrics = ctxt.read_array::<LongHorMetric>(num_h_metrics)?;
        let left_side_bearings =
            ctxt.read_array::<I16Be>(num_glyphs.saturating_sub(num_h_metrics))?;
        Ok(HmtxTable {
            h_metrics: ReadArrayCow::Borrowed(h_metrics),
            left_side_bearings: ReadArrayCow::Borrowed(left_side_bearings),
        })
    }
}

impl<'a> WriteBinary<&Self> for HmtxTable<'a> {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &HmtxTable<'a>) -> Result<(), WriteError> {
        ReadArrayCow::write(ctxt, &table.h_metrics)?;
        ReadArrayCow::write(ctxt, &table.left_side_bearings)?;

        Ok(())
    }
}

impl HmtxTable<'_> {
    /// Retrieve the horizontal advance for glyph with index `glyph_id`.
    pub fn horizontal_advance(&self, glyph_id: u16) -> Result<u16, ParseError> {
        if self.h_metrics.is_empty() {
            return Err(ParseError::BadIndex);
        }

        // This is largely the same as `metric` below but it avoids a lookup in the
        // `left_side_bearings` array.
        let glyph_id = usize::from(glyph_id);
        let num_h_metrics = self.h_metrics.len();
        let metric = if glyph_id < num_h_metrics {
            self.h_metrics.read_item(glyph_id)
        } else {
            // As an optimization, the number of records can be less than the number of glyphs, in
            // which case the advance width value of the last record applies to all remaining glyph
            // IDs.
            // https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx
            self.h_metrics.read_item(num_h_metrics - 1)
        };
        metric.map(|metric| metric.advance_width)
    }

    /// Retrieve the advance and left-side bearing for glyph with index `glyph_id`.
    pub fn metric(&self, glyph_id: u16) -> Result<LongHorMetric, ParseError> {
        if self.h_metrics.is_empty() {
            return Err(ParseError::BadIndex);
        }

        let glyph_id = usize::from(glyph_id);
        let num_h_metrics = self.h_metrics.len();
        if glyph_id < num_h_metrics {
            self.h_metrics.read_item(glyph_id)
        } else {
            // As an optimization, the number of records can be less than the number of glyphs, in
            // which case the advance width value of the last record applies to all remaining glyph
            // IDs. If numberOfHMetrics is less than the total number of glyphs, then the hMetrics
            // array is followed by an array for the left side bearing values of the remaining
            // glyphs.
            // https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx
            let mut metric = self.h_metrics.read_item(num_h_metrics - 1)?;
            let lsb_index = glyph_id - num_h_metrics;
            metric.lsb = self
                .left_side_bearings
                .check_index(lsb_index)
                .and_then(|_| self.left_side_bearings.read_item(lsb_index))?;
            Ok(metric)
        }
    }
}

impl ReadFrom for LongHorMetric {
    type ReadType = (U16Be, I16Be);
    fn read_from((advance_width, lsb): (u16, i16)) -> Self {
        LongHorMetric { advance_width, lsb }
    }
}

impl WriteBinary for LongHorMetric {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, metric: LongHorMetric) -> Result<(), WriteError> {
        U16Be::write(ctxt, metric.advance_width)?;
        I16Be::write(ctxt, metric.lsb)?;

        Ok(())
    }
}

impl ReadBinary for MaxpTable {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let version = ctxt.read_u32be()?;
        let num_glyphs = ctxt.read_u16be()?;
        let sub_table = if version == 0x00010000 {
            Some(ctxt.read::<MaxpVersion1SubTable>()?)
        } else {
            None
        };
        Ok(MaxpTable {
            num_glyphs,
            version1_sub_table: sub_table,
        })
    }
}

impl WriteBinary<&Self> for MaxpTable {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &MaxpTable) -> Result<(), WriteError> {
        if let Some(sub_table) = &table.version1_sub_table {
            U32Be::write(ctxt, 0x00010000u32)?; // version 1.0
            U16Be::write(ctxt, table.num_glyphs)?;
            MaxpVersion1SubTable::write(ctxt, sub_table)?;
        } else {
            U32Be::write(ctxt, 0x00005000u32)?; // version 0.5
            U16Be::write(ctxt, table.num_glyphs)?;
        }
        Ok(())
    }
}

impl ReadBinary for MaxpVersion1SubTable {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let max_points = ctxt.read_u16be()?;
        let max_contours = ctxt.read_u16be()?;
        let max_composite_points = ctxt.read_u16be()?;
        let max_composite_contours = ctxt.read_u16be()?;
        let max_zones = ctxt.read_u16be()?;
        let max_twilight_points = ctxt.read_u16be()?;
        let max_storage = ctxt.read_u16be()?;
        let max_function_defs = ctxt.read_u16be()?;
        let max_instruction_defs = ctxt.read_u16be()?;
        let max_stack_elements = ctxt.read_u16be()?;
        let max_size_of_instructions = ctxt.read_u16be()?;
        let max_component_elements = ctxt.read_u16be()?;
        let max_component_depth = ctxt.read_u16be()?;

        Ok(MaxpVersion1SubTable {
            max_points,
            max_contours,
            max_composite_points,
            max_composite_contours,
            max_zones,
            max_twilight_points,
            max_storage,
            max_function_defs,
            max_instruction_defs,
            max_stack_elements,
            max_size_of_instructions,
            max_component_elements,
            max_component_depth,
        })
    }
}

impl WriteBinary<&Self> for MaxpVersion1SubTable {
    type Output = ();

    fn write<C: WriteContext>(
        ctxt: &mut C,
        table: &MaxpVersion1SubTable,
    ) -> Result<(), WriteError> {
        U16Be::write(ctxt, table.max_points)?;
        U16Be::write(ctxt, table.max_contours)?;
        U16Be::write(ctxt, table.max_composite_points)?;
        U16Be::write(ctxt, table.max_composite_contours)?;
        U16Be::write(ctxt, table.max_zones)?;
        U16Be::write(ctxt, table.max_twilight_points)?;
        U16Be::write(ctxt, table.max_storage)?;
        U16Be::write(ctxt, table.max_function_defs)?;
        U16Be::write(ctxt, table.max_instruction_defs)?;
        U16Be::write(ctxt, table.max_stack_elements)?;
        U16Be::write(ctxt, table.max_size_of_instructions)?;
        U16Be::write(ctxt, table.max_component_elements)?;
        U16Be::write(ctxt, table.max_component_depth)?;

        Ok(())
    }
}

impl NameTable<'_> {
    pub const COPYRIGHT_NOTICE: u16 = 0;
    pub const FONT_FAMILY_NAME: u16 = 1;
    pub const FONT_SUBFAMILY_NAME: u16 = 2;
    pub const UNIQUE_FONT_IDENTIFIER: u16 = 3;
    pub const FULL_FONT_NAME: u16 = 4;
    pub const VERSION_STRING: u16 = 5;
    pub const POSTSCRIPT_NAME: u16 = 6;
    pub const TRADEMARK: u16 = 7;
    pub const MANUFACTURER_NAME: u16 = 8;
    pub const DESIGNER: u16 = 9;
    pub const DESCRIPTION: u16 = 10;
    pub const URL_VENDOR: u16 = 11;
    pub const URL_DESIGNER: u16 = 12;
    pub const LICENSE_DESCRIPTION: u16 = 13;
    pub const LICENSE_INFO_URL: u16 = 14;
    pub const TYPOGRAPHIC_FAMILY_NAME: u16 = 16;
    pub const TYPOGRAPHIC_SUBFAMILY_NAME: u16 = 17;
    pub const COMPATIBLE_FULL: u16 = 18; // (Macintosh only)
    pub const SAMPLE_TEXT: u16 = 19;
    pub const POSTSCRIPT_CID_FINDFONT_NAME: u16 = 20;
    pub const WWS_FAMILY_NAME: u16 = 21; // WWS = Weight, width, slope
    pub const WWS_SUBFAMILY_NAME: u16 = 22;
    pub const LIGHT_BACKGROUND_PALETTE: u16 = 23;
    pub const DARK_BACKGROUND_PALETTE: u16 = 24;
    pub const VARIATIONS_POSTSCRIPT_NAME_PREFIX: u16 = 25;

    /// Return a string for the supplied `name_id`.
    ///
    /// Returns the first match in this order:
    ///
    /// 1. Unicode platform
    /// 2. Windows platform, English language ids
    /// 3. Apple platform, Roman language id
    pub fn string_for_id(&self, name_id: u16) -> Option<String> {
        self.name_records
            .iter()
            .find_map(|record| {
                if record.name_id != name_id {
                    return None;
                }
                // Match English records
                match (record.platform_id, record.encoding_id, record.language_id) {
                    // Unicode
                    (0, _, _) => Some((record, encoding_rs::UTF_16BE)),
                    // Windows Unicode BMP, English language ids
                    // https://learn.microsoft.com/en-us/typography/opentype/spec/name#windows-language-ids
                    (
                        3,
                        1,
                        0x0C09 | 0x2809 | 0x1009 | 0x2409 | 0x4009 | 0x1809 | 0x2009 | 0x4409
                        | 0x1409 | 0x3409 | 0x4809 | 0x1C09 | 0x2C09 | 0x0809 | 0x0409 | 0x3009,
                    ) => Some((record, encoding_rs::UTF_16BE)),
                    // Windows Unicode full, English language ids
                    // https://learn.microsoft.com/en-us/typography/opentype/spec/name#windows-language-ids
                    (
                        3,
                        10,
                        0x0C09 | 0x2809 | 0x1009 | 0x2409 | 0x4009 | 0x1809 | 0x2009 | 0x4409
                        | 0x1409 | 0x3409 | 0x4809 | 0x1C09 | 0x2C09 | 0x0809 | 0x0409 | 0x3009,
                    ) => Some((record, encoding_rs::UTF_16BE)),
                    // Apple, Roman Script, English
                    (1, 0, 0) => Some((record, encoding_rs::MACINTOSH)),
                    _ => None,
                }
            })
            .and_then(|(record, encoding)| {
                let offset = usize::from(record.offset);
                let length = usize::from(record.length);
                let name_data = self
                    .string_storage
                    .offset_length(offset, length)
                    .ok()?
                    .data();
                Some(decode(encoding, name_data))
            })
    }
}

pub(crate) fn decode(encoding: &'static Encoding, data: &[u8]) -> String {
    let mut decoder = encoding.new_decoder();
    let size = decoder.max_utf8_buffer_length(data.len()).unwrap();
    let mut s = String::with_capacity(size);
    let (_res, _read, _repl) = decoder.decode_to_string(data, &mut s, true);
    s
}

fn utf16be_encode(string: &str) -> Vec<u8> {
    string
        .encode_utf16()
        .flat_map(|codeunit| codeunit.to_be_bytes())
        .collect()
}

impl ReadBinary for NameTable<'_> {
    type HostType<'a> = NameTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();

        let format = ctxt.read_u16be()?;
        ctxt.check(format <= 1)?;
        let count = usize::from(ctxt.read_u16be()?);
        let string_offset = usize::from(ctxt.read_u16be()?);
        let string_storage = scope.offset(string_offset);
        let name_records = ctxt.read_array::<NameRecord>(count)?;
        let opt_langtag_records = if format > 0 {
            let langtag_count = usize::from(ctxt.read_u16be()?);
            let langtag_records = ctxt.read_array::<LangTagRecord>(langtag_count)?;
            Some(langtag_records)
        } else {
            None
        };

        Ok(NameTable {
            string_storage,
            name_records,
            opt_langtag_records,
        })
    }
}

impl<'a> WriteBinary<&Self> for NameTable<'a> {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, name: &NameTable<'a>) -> Result<(), WriteError> {
        let format = name.opt_langtag_records.as_ref().map_or(0u16, |_| 1);
        U16Be::write(ctxt, format)?;
        U16Be::write(ctxt, u16::try_from(name.name_records.len())?)?; // count
        let string_offset = ctxt.placeholder::<U16Be, _>()?;
        <&ReadArray<'a, _>>::write(ctxt, &name.name_records)?;

        if let Some(lang_tag_records) = &name.opt_langtag_records {
            U16Be::write(ctxt, u16::try_from(lang_tag_records.len())?)?; // lang_tag_count
            <&ReadArray<'a, _>>::write(ctxt, lang_tag_records)?;
        }

        ctxt.write_placeholder(string_offset, u16::try_from(ctxt.bytes_written())?)?;
        ctxt.write_bytes(name.string_storage.data())?;

        Ok(())
    }
}

impl ReadFrom for NameRecord {
    type ReadType = ((U16Be, U16Be, U16Be), (U16Be, U16Be, U16Be));
    fn read_from(
        ((platform_id, encoding_id, language_id), (name_id, length, offset)): (
            (u16, u16, u16),
            (u16, u16, u16),
        ),
    ) -> Self {
        NameRecord {
            platform_id,
            encoding_id,
            language_id,
            name_id,
            length,
            offset,
        }
    }
}

impl WriteBinary for NameRecord {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, record: NameRecord) -> Result<(), WriteError> {
        U16Be::write(ctxt, record.platform_id)?;
        U16Be::write(ctxt, record.encoding_id)?;
        U16Be::write(ctxt, record.language_id)?;
        U16Be::write(ctxt, record.name_id)?;
        U16Be::write(ctxt, record.length)?;
        U16Be::write(ctxt, record.offset)?;

        Ok(())
    }
}

impl ReadFrom for LangTagRecord {
    type ReadType = (U16Be, U16Be);
    fn read_from((length, offset): (u16, u16)) -> Self {
        LangTagRecord { length, offset }
    }
}

impl WriteBinary for LangTagRecord {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, record: LangTagRecord) -> Result<(), WriteError> {
        U16Be::write(ctxt, record.length)?;
        U16Be::write(ctxt, record.offset)?;

        Ok(())
    }
}

impl ReadBinaryDep for CvtTable<'_> {
    type Args<'a> = u32;
    type HostType<'a> = CvtTable<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        length: u32,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let length = usize::safe_from(length);
        // The table contains 'n' values, where n is just as many values can be read for the
        // size of the table. We assume that `ctxt` is limited to the length of the table
        //
        // > The length of the table must be an integral number of FWORD units.
        ctxt.check(length % I16Be::SIZE == 0)?;
        let n = length / I16Be::SIZE;
        let values = ctxt.read_array(n)?;
        Ok(CvtTable {
            values: ReadArrayCow::Borrowed(values),
        })
    }
}

impl WriteBinary<&Self> for CvtTable<'_> {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, table: &Self) -> Result<(), WriteError> {
        ReadArrayCow::write(ctxt, &table.values)
    }
}

impl ReadFrom for F2Dot14 {
    type ReadType = I16Be;

    fn read_from(value: i16) -> Self {
        F2Dot14(value)
    }
}

impl WriteBinary for F2Dot14 {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, val: Self) -> Result<(), WriteError> {
        I16Be::write(ctxt, val.0)
    }
}

impl ReadBinary for IndexToLocFormat {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let index_to_loc_format = ctxt.read_i16be()?;

        match index_to_loc_format {
            0 => Ok(IndexToLocFormat::Short),
            1 => Ok(IndexToLocFormat::Long),
            _ => Err(ParseError::BadValue),
        }
    }
}

impl WriteBinary for IndexToLocFormat {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, index_to_loc_format: Self) -> Result<(), WriteError> {
        match index_to_loc_format {
            IndexToLocFormat::Short => I16Be::write(ctxt, 0i16),
            IndexToLocFormat::Long => I16Be::write(ctxt, 1i16),
        }
    }
}

impl Fixed {
    /// Create a new `Fixed` with a raw 16.16 value.
    pub const fn from_raw(value: i32) -> Fixed {
        Fixed(value)
    }

    pub fn raw_value(&self) -> i32 {
        self.0
    }

    pub fn abs(&self) -> Fixed {
        Fixed(self.0.abs())
    }
}

impl std::ops::Add for Fixed {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Fixed(self.0.wrapping_add(rhs.0))
    }
}

impl std::ops::Sub for Fixed {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Fixed(self.0.wrapping_sub(rhs.0))
    }
}

impl std::ops::Mul for Fixed {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        let a = i64::from(self.0);
        let b = i64::from(rhs.0);
        Fixed(((a * b) >> 16) as i32)
    }
}

impl std::ops::Div for Fixed {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        let a = i64::from(self.0);
        let b = i64::from(rhs.0);
        if b == 0 {
            // Closest we have to infinity. Same as what FreeType does
            // https://gitlab.freedesktop.org/freetype/freetype/-/blob/a20de84e1608f9eb1d0391d7322b2e0e0f235aba/src/base/ftcalc.c#L267
            return Fixed(0x7FFFFFFF);
        }

        Fixed(((a << 16) / b) as i32)
    }
}

impl std::ops::Neg for Fixed {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Fixed(-self.0)
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Fixed").field(&f32::from(*self)).finish()
    }
}

impl ReadFrom for Fixed {
    type ReadType = I32Be;

    fn read_from(value: i32) -> Self {
        Fixed(value)
    }
}

impl WriteBinary for Fixed {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, val: Self) -> Result<(), WriteError> {
        I32Be::write(ctxt, val.0)
    }
}

impl From<Fixed> for f32 {
    fn from(value: Fixed) -> f32 {
        (f64::from(value.0) / 65536.0) as f32
    }
}

// When converting from float or double data types to 16.16, the following method must be used:
//
// 1. Multiply the fractional component by 65536, and round the result to the nearest integer (for
//    fractional values of 0.5 and higher, take the next higher integer; for other fractional
//    values, truncate). Store the result in the low-order word.
// 2. Move the two’s-complement representation of the integer component into the high-order word
//
// https://learn.microsoft.com/en-us/typography/opentype/spec/otvaroverview#coordinate-scales-and-normalization
impl From<f32> for Fixed {
    fn from(value: f32) -> Self {
        let sign = value.signum() as i32;
        let value = value.abs();
        let fract = (value.fract() * 65536.0).round() as i32;
        let int = value.trunc() as i32;
        Fixed::from_raw(((int << 16) | fract) * sign)
    }
}

impl From<f64> for Fixed {
    fn from(value: f64) -> Self {
        let sign = value.signum() as i32;
        let value = value.abs();
        let fract = (value.fract() * 65536.0).round() as i32;
        let int = value.trunc() as i32;
        Fixed::from_raw(((int << 16) | fract) * sign)
    }
}

impl From<i32> for Fixed {
    fn from(value: i32) -> Self {
        Fixed::from_raw(value << 16)
    }
}

impl From<F2Dot14> for Fixed {
    fn from(fixed: F2Dot14) -> Self {
        Fixed(i32::from(fixed.0) << 2)
    }
}

impl F2Dot14 {
    pub fn from_raw(value: i16) -> Self {
        F2Dot14(value)
    }

    pub fn raw_value(&self) -> i16 {
        self.0
    }
}

impl std::ops::Add for F2Dot14 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        F2Dot14(self.0.wrapping_add(rhs.0))
    }
}

impl std::ops::Sub for F2Dot14 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        F2Dot14(self.0.wrapping_sub(rhs.0))
    }
}

impl std::ops::Mul for F2Dot14 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        let a = i32::from(self.0);
        let b = i32::from(rhs.0);
        F2Dot14(((a * b) >> 14) as i16)
    }
}

impl std::ops::Div for F2Dot14 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        let a = i32::from(self.0);
        let b = i32::from(rhs.0);
        if b == 0 {
            // Closest we have to infinity.
            return F2Dot14(0x7FFF);
        }

        F2Dot14(((a << 14) / b) as i16)
    }
}

impl std::ops::Neg for F2Dot14 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        F2Dot14(-self.0)
    }
}

impl fmt::Debug for F2Dot14 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("F2Dot14").field(&f32::from(*self)).finish()
    }
}

impl From<Fixed> for F2Dot14 {
    fn from(fixed: Fixed) -> Self {
        // Convert the final, normalized 16.16 coordinate value to 2.14 by this method: add
        // 0x00000002, and sign-extend shift to the right by 2.
        // https://learn.microsoft.com/en-us/typography/opentype/spec/otvaroverview#coordinate-scales-and-normalization
        F2Dot14(((fixed.0 + 2) >> 2) as i16)
    }
}

impl From<f32> for F2Dot14 {
    fn from(value: f32) -> Self {
        let sign = value.signum() as i16;
        let value = value.abs();
        let fract = (value.fract() * 16384.0).round() as i16;
        let int = value.trunc() as i16;
        F2Dot14::from_raw(((int << 14) | fract).wrapping_mul(sign))
    }
}

impl From<i16> for F2Dot14 {
    fn from(value: i16) -> Self {
        F2Dot14::from_raw(value << 14)
    }
}

impl From<F2Dot14> for f32 {
    fn from(value: F2Dot14) -> Self {
        f32::from(value.0) / 16384.
    }
}

impl From<F2Dot14> for f64 {
    fn from(value: F2Dot14) -> Self {
        f64::from(value.0) / 16384.
    }
}

impl<T: FontTableProvider> FontTableProvider for Box<T> {
    fn table_data(&self, tag: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        self.as_ref().table_data(tag)
    }

    fn has_table(&self, tag: u32) -> bool {
        self.as_ref().has_table(tag)
    }

    fn table_tags(&self) -> Option<Vec<u32>> {
        self.as_ref().table_tags()
    }
}

impl<T: SfntVersion> SfntVersion for Box<T> {
    fn sfnt_version(&self) -> u32 {
        self.as_ref().sfnt_version()
    }
}

pub(crate) fn read_and_box_table(
    provider: &impl FontTableProvider,
    tag: u32,
) -> Result<Box<[u8]>, ParseError> {
    provider
        .read_table_data(tag)
        .map(|table| Box::from(table.into_owned()))
}

pub(crate) fn read_and_box_optional_table(
    provider: &impl FontTableProvider,
    tag: u32,
) -> Result<Option<Box<[u8]>>, ParseError> {
    Ok(provider
        .table_data(tag)?
        .map(|table| Box::from(table.into_owned())))
}

pub mod owned {
    //! Owned versions of tables.

    use std::borrow::Cow;

    use super::utf16be_encode;
    use crate::binary::write::{Placeholder, WriteBinary, WriteContext};
    use crate::binary::U16Be;
    use crate::error::{ParseError, WriteError};

    /// An owned `name` table.
    ///
    /// Can be created from [super::NameTable] using `TryFrom`.
    ///
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/name>
    pub struct NameTable<'a> {
        pub name_records: Vec<NameRecord<'a>>,
        /// UTF-16BE encoded language tag strings
        pub langtag_records: Vec<Cow<'a, [u8]>>,
    }

    /// Record within the `name` table.
    pub struct NameRecord<'a> {
        pub platform_id: u16,
        pub encoding_id: u16,
        pub language_id: u16,
        pub name_id: u16,
        pub string: Cow<'a, [u8]>,
    }

    impl NameTable<'_> {
        /// Replace all instances of `name_id` with a Unicode entry with the value `string`.
        pub fn replace_entries(&mut self, name_id: u16, string: &str) {
            self.remove_entries(name_id);
            let replacement = NameRecord {
                platform_id: 0, // Unicode
                encoding_id: 4, // full repertoire
                language_id: 0,
                name_id,
                string: Cow::from(utf16be_encode(string)),
            };
            self.name_records.push(replacement);
        }

        pub fn remove_entries(&mut self, name_id: u16) {
            self.name_records.retain(|record| record.name_id != name_id);
        }
    }

    impl<'a> TryFrom<&super::NameTable<'a>> for NameTable<'a> {
        type Error = ParseError;

        fn try_from(name: &super::NameTable<'a>) -> Result<NameTable<'a>, ParseError> {
            let name_records = name
                .name_records
                .iter()
                .map(|record| {
                    let string = name
                        .string_storage
                        .offset_length(usize::from(record.offset), usize::from(record.length))
                        .map(|scope| Cow::from(scope.data()))?;
                    Ok(NameRecord {
                        platform_id: record.platform_id,
                        encoding_id: record.encoding_id,
                        language_id: record.language_id,
                        name_id: record.name_id,
                        string,
                    })
                })
                .collect::<Result<Vec<_>, ParseError>>()?;
            let langtag_records = name
                .opt_langtag_records
                .as_ref()
                .map(|langtag_records| {
                    langtag_records
                        .iter()
                        .map(|record| {
                            name.string_storage
                                .offset_length(
                                    usize::from(record.offset),
                                    usize::from(record.length),
                                )
                                .map(|scope| Cow::from(scope.data()))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_else(Vec::new);

            Ok(NameTable {
                name_records,
                langtag_records,
            })
        }
    }

    impl<'a> WriteBinary<&Self> for NameTable<'a> {
        type Output = ();

        fn write<C: WriteContext>(ctxt: &mut C, name: &NameTable<'a>) -> Result<(), WriteError> {
            let format = if name.langtag_records.is_empty() {
                0u16
            } else {
                1
            };
            U16Be::write(ctxt, format)?;
            U16Be::write(ctxt, u16::try_from(name.name_records.len())?)?; // count
            let string_offset = ctxt.placeholder::<U16Be, _>()?;
            let name_record_offsets = name
                .name_records
                .iter()
                .map(|record| NameRecord::write(ctxt, record))
                .collect::<Result<Vec<_>, _>>()?;

            if !name.langtag_records.is_empty() {
                // langtag count
                U16Be::write(ctxt, u16::try_from(name.langtag_records.len())?)?;
            }
            let langtag_record_offsets = name
                .langtag_records
                .iter()
                .map(|record| {
                    U16Be::write(ctxt, u16::try_from(record.len())?)?;
                    ctxt.placeholder::<U16Be, _>()
                })
                .collect::<Result<Vec<_>, _>>()?;

            let string_start = ctxt.bytes_written();
            ctxt.write_placeholder(string_offset, u16::try_from(string_start)?)?;

            // Write the string data
            let lang_tags = name.langtag_records.iter().zip(langtag_record_offsets);
            let records = name
                .name_records
                .iter()
                .map(|rec| &rec.string)
                .zip(name_record_offsets)
                .chain(lang_tags);

            for (string, placeholder) in records {
                ctxt.write_placeholder(
                    placeholder,
                    u16::try_from(ctxt.bytes_written() - string_start)?,
                )?;
                ctxt.write_bytes(string)?;
            }

            Ok(())
        }
    }

    impl WriteBinary<&Self> for NameRecord<'_> {
        type Output = Placeholder<U16Be, u16>;

        fn write<C: WriteContext>(ctxt: &mut C, record: &Self) -> Result<Self::Output, WriteError> {
            U16Be::write(ctxt, record.platform_id)?;
            U16Be::write(ctxt, record.encoding_id)?;
            U16Be::write(ctxt, record.language_id)?;
            U16Be::write(ctxt, record.name_id)?;
            U16Be::write(ctxt, u16::try_from(record.string.len())?)?;
            let offset = ctxt.placeholder()?;
            Ok(offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        owned, F2Dot14, Fixed, HeadTable, HmtxTable, NameTable, OpenTypeData, OpenTypeFont,
    };
    use crate::assert_close;
    use crate::binary::read::ReadScope;
    use crate::binary::write::{WriteBinary, WriteBuffer, WriteContext};
    use crate::tests::{assert_close, assert_f2dot14_close, assert_fixed_close, read_fixture};

    const NAME_DATA: &[u8] = include_bytes!("../tests/fonts/opentype/name.bin");

    #[test]
    fn test_write_head_table() {
        // Read a head table in, then write it back out and compare it
        let head_data = include_bytes!("../tests/fonts/opentype/head.bin");
        let head = ReadScope::new(head_data).read::<HeadTable>().unwrap();
        let checksum_adjustment = head.check_sum_adjustment;

        let mut ctxt = WriteBuffer::new();
        let placeholder = HeadTable::write(&mut ctxt, &head).unwrap();
        ctxt.write_placeholder(placeholder, checksum_adjustment)
            .unwrap();

        assert_eq!(ctxt.bytes(), &head_data[..]);
    }

    #[test]
    fn test_write_hmtx_table() {
        // Read a hmtx table in, then write it back out and compare it
        let hmtx_data = include_bytes!("../tests/fonts/opentype/hmtx.bin");
        let num_glyphs = 1264;
        let num_h_metrics = 1264;
        let hmtx = ReadScope::new(hmtx_data)
            .read_dep::<HmtxTable<'_>>((num_glyphs, num_h_metrics))
            .unwrap();

        let mut ctxt = WriteBuffer::new();
        HmtxTable::write(&mut ctxt, &hmtx).unwrap();

        assert_eq!(ctxt.bytes(), &hmtx_data[..]);
    }

    #[test]
    fn test_write_name_table() {
        // Read a name table in, then write it back out and compare it
        let name = ReadScope::new(NAME_DATA).read::<NameTable<'_>>().unwrap();

        let mut ctxt = WriteBuffer::new();
        NameTable::write(&mut ctxt, &name).unwrap();

        assert_eq!(ctxt.bytes(), &NAME_DATA[..]);
    }

    #[test]
    fn roundtrip_owned_name_table() {
        // Test that NameTable can be converted to owned variant, written, and read back the same
        let name = ReadScope::new(NAME_DATA).read::<NameTable<'_>>().unwrap();
        let owned = owned::NameTable::try_from(&name).unwrap();

        let mut ctxt = WriteBuffer::new();
        owned::NameTable::write(&mut ctxt, &owned).unwrap();

        assert_eq!(ctxt.bytes(), &NAME_DATA[..]);
    }

    #[test]
    fn f32_from_f2dot14() {
        // Examples from https://docs.microsoft.com/en-us/typography/opentype/spec/otff#data-types
        assert_close(f32::from(F2Dot14(0x7fff)), 1.999939);
        assert_close(f32::from(F2Dot14(0x7000)), 1.75);
        assert_close(f32::from(F2Dot14(0x0001)), 0.000061);
        assert_close(f32::from(F2Dot14(0x0000)), 0.0);
        assert_close(f32::from(F2Dot14(-1 /* 0xFFFF */)), -0.000061);
        assert_close(f32::from(F2Dot14(-32768 /* 0x8000 */)), -2.0);
    }

    #[test]
    fn f2dot14_from_f32() {
        // Examples from https://docs.microsoft.com/en-us/typography/opentype/spec/otff#data-types
        assert_eq!(F2Dot14::from(1.999939), F2Dot14::from_raw(0x7fff));
        assert_eq!(F2Dot14::from(1.75), F2Dot14::from_raw(0x7000));
        assert_eq!(F2Dot14::from(0.000061), F2Dot14::from_raw(0x0001));
        assert_eq!(F2Dot14::from(0.0), F2Dot14::from_raw(0x0000));
        assert_eq!(F2Dot14::from(-0.000061), F2Dot14::from_raw(-1 /* 0xffff */));
        assert_close!(f32::from(F2Dot14::from(-1.4)), -1.4, 1. / 16384.);
        assert_eq!(F2Dot14::from(-2.0), F2Dot14::from_raw(-32768 /* 0x8000 */));
    }

    #[test]
    fn f2dot14_from_fixed() {
        // Examples from https://docs.microsoft.com/en-us/typography/opentype/spec/otff#data-types
        assert_eq!(
            F2Dot14::from(Fixed::from(1.999939)),
            F2Dot14::from_raw(0x7fff)
        );
        assert_eq!(F2Dot14::from(Fixed::from(1.75)), F2Dot14::from_raw(0x7000));
        assert_eq!(
            F2Dot14::from(Fixed::from(0.000061)),
            F2Dot14::from_raw(0x0001)
        );
        assert_eq!(F2Dot14::from(Fixed::from(0.0)), F2Dot14::from_raw(0x0000));
        assert_eq!(
            F2Dot14::from(Fixed::from(-0.000061)),
            F2Dot14::from_raw(-1 /* 0xffff */)
        );
        assert_eq!(
            F2Dot14::from(Fixed::from(-2.0)),
            F2Dot14::from_raw(-32768 /* 0x8000 */)
        );
    }

    #[test]
    fn f32_from_fixed() {
        assert_close(f32::from(Fixed(0x7fff_0000)), 32767.);
        assert_close(f32::from(Fixed(0x7000_0001)), 28672.0001);
        assert_close(f32::from(Fixed(0x0001_0000)), 1.0);
        assert_close(f32::from(Fixed(0x0000_0000)), 0.0);
        assert_close(
            f32::from(Fixed(i32::from_be_bytes([0xff; 4]))),
            -0.000015259,
        );
        assert_close(f32::from(Fixed(0x7fff_ffff)), 32768.0);
    }

    #[test]
    fn fixed_from_f32() {
        assert_eq!(Fixed::from(32767.0_f32), Fixed(0x7fff_0000));
        assert_eq!(Fixed::from(28672.0001_f32), Fixed(0x7000_0000));
        assert_eq!(Fixed::from(1.0_f32), Fixed(0x0001_0000));
        assert_eq!(Fixed::from(-1.0_f32), Fixed(-65536));
        assert_eq!(Fixed::from(0.0_f32), Fixed(0x0000_0000));
        assert_eq!(Fixed::from(0.000015259_f32), Fixed(1));
        assert_eq!(Fixed::from(32768.0_f32), Fixed(-0x8000_0000));
        assert_eq!(Fixed::from(1.23_f32), Fixed(0x0001_3ae1));
        assert_close!(f32::from(Fixed::from(-1.4_f32)), -1.4, 1. / 65536.);
    }

    #[test]
    fn fixed_from_i32() {
        assert_eq!(Fixed::from(32767), Fixed(0x7fff_0000));
        assert_eq!(Fixed::from(28672), Fixed(0x7000_0000));
        assert_eq!(Fixed::from(1), Fixed(0x0001_0000));
        assert_eq!(Fixed::from(0), Fixed(0x0000_0000));
        assert_eq!(Fixed::from(-0), Fixed(0));
        assert_eq!(Fixed::from(32768), Fixed(-0x8000_0000));
    }

    #[test]
    fn fixed_from_f2dot14() {
        assert_eq!(Fixed::from(F2Dot14::from(0.5)), Fixed(0x0000_8000));
    }

    #[test]
    fn fixed_add() {
        assert_eq!(Fixed(10) + Fixed(20), Fixed(30));
        assert_fixed_close(Fixed::from(0.1) + Fixed::from(0.2), 0.3);
        assert_fixed_close(Fixed::from(-0.1) + Fixed::from(0.4), 0.3);
        assert_eq!(Fixed(i32::MAX) + Fixed(1), Fixed(-0x80000000)); // overflow
    }

    #[test]
    fn fixed_sub() {
        assert_eq!(Fixed(10) - Fixed(20), Fixed(-10));
        assert_fixed_close(Fixed::from(0.1) - Fixed::from(0.2), -0.1);
        assert_fixed_close(Fixed::from(-0.1) - Fixed::from(0.4), -0.5);
        assert_eq!(Fixed(i32::MIN) - Fixed(1), Fixed(0x7fffffff)); // underflow
    }

    #[test]
    fn fixed_mul() {
        assert_eq!(Fixed(0x2_0000) * Fixed(0x4_0000), Fixed(0x8_0000));
        assert_fixed_close(Fixed::from(0.1) * Fixed::from(0.2), 0.02);
        assert_fixed_close(Fixed::from(-0.1) * Fixed::from(0.4), -0.04);
    }

    #[test]
    fn fixed_div() {
        assert_eq!(Fixed(0x4_0000) / Fixed(0x2_0000), Fixed(0x2_0000));
        assert_fixed_close(Fixed::from(0.1) / Fixed::from(0.2), 0.5);
        assert_fixed_close(Fixed::from(-0.1) / Fixed::from(0.4), -0.25);
        assert_eq!(Fixed(0x4_0000) / Fixed(0), Fixed(0x7FFFFFFF)); // div 0
    }

    #[test]
    fn fixed_neg() {
        assert_eq!(-Fixed(0x4_0000), Fixed(-0x4_0000));
        assert_fixed_close(-Fixed::from(0.1), -0.1);
        assert_fixed_close(-Fixed::from(-0.25), 0.25);
        assert_eq!(-Fixed(0x7FFFFFFF), Fixed(-0x7FFFFFFF));
    }

    #[test]
    fn fixed_abs() {
        assert_fixed_close(Fixed::from(-1.0).abs(), 1.0);
        assert_fixed_close(Fixed::from(1.0).abs(), 1.0);
        assert_eq!(Fixed(-0x7FFFFFFF).abs(), Fixed(0x7FFFFFFF));
    }

    #[test]
    fn f2dot14_add() {
        assert_eq!(Fixed(10) + Fixed(20), Fixed(30));
        assert_f2dot14_close(F2Dot14::from(0.1) + F2Dot14::from(0.2), 0.3);
        assert_f2dot14_close(F2Dot14::from(-0.1) + F2Dot14::from(0.4), 0.3);
        assert_eq!(F2Dot14(i16::MAX) + F2Dot14(1), F2Dot14(-0x8000)); // overflow
    }

    #[test]
    fn f2dot14_sub() {
        assert_eq!(F2Dot14(10) - F2Dot14(20), F2Dot14(-10));
        assert_f2dot14_close(F2Dot14::from(0.1) - F2Dot14::from(0.2), -0.1);
        assert_f2dot14_close(F2Dot14::from(-0.1) - F2Dot14::from(0.4), -0.5);
        assert_eq!(F2Dot14(i16::MIN) - F2Dot14(1), F2Dot14(0x7fff)); // underflow
    }

    #[test]
    fn f2dot14_mul() {
        assert_f2dot14_close(F2Dot14::from(0.1) * F2Dot14::from(0.2), 0.02);
        assert_f2dot14_close(F2Dot14::from(-0.1) * F2Dot14::from(0.4), -0.04);
    }

    #[test]
    fn f2dot14_div() {
        assert_f2dot14_close(F2Dot14::from(0.1) / F2Dot14::from(0.2), 0.5);
        assert_f2dot14_close(F2Dot14::from(-0.1) / F2Dot14::from(0.4), -0.25);
        assert_eq!(F2Dot14(0x4_000) / F2Dot14(0), F2Dot14(0x7FFF)); // div 0
    }

    #[test]
    fn f2dot14_neg() {
        assert_eq!(-F2Dot14(0x1_000), F2Dot14(-0x1_000));
        assert_f2dot14_close(-F2Dot14::from(0.1), -0.1);
        assert_f2dot14_close(-F2Dot14::from(-0.25), 0.25);
        assert_eq!(-F2Dot14(0x7FFF), F2Dot14(-0x7FFF));
    }

    #[test]
    fn read_true_magic() {
        let buffer = read_fixture("tests/fonts/variable/Zycon.ttf");
        let fontfile = ReadScope::new(&buffer)
            .read::<OpenTypeFont<'_>>()
            .expect("error reading OpenTypeFile");
        let offset_table = match fontfile.data {
            OpenTypeData::Single(font) => font,
            OpenTypeData::Collection(_) => unreachable!(),
        };
        assert_eq!(offset_table.table_records.len(), 12);
    }
}
