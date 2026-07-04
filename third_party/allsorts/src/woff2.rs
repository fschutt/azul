//! Reading of the WOFF2 font format.
//!
//! <https://www.w3.org/TR/WOFF2/>

mod collection;
mod lut;

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Cursor, Read};

use bitflags::bitflags;
use self::lut::{XYTriplet, COORD_LUT, KNOWN_TABLE_TAGS};

/// Sum type that lets a function return one of two iterator shapes
/// behind a single `impl Iterator` without pulling in `itertools::Either`.
enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R, T> Iterator for Either<L, R>
where
    L: Iterator<Item = T>,
    R: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match self {
            Either::Left(l) => l.next(),
            Either::Right(r) => r.next(),
        }
    }
}
use crate::binary::read::{
    ReadArray, ReadArrayCow, ReadBinary, ReadBinaryDep, ReadBuf, ReadCtxt, ReadFrom, ReadScope,
};
use crate::binary::{write, I16Be, U16Be, U8};
use crate::error::{ParseError, ReadWriteError};
use crate::tables::glyf::{
    BoundingBox, CompositeGlyph, CompositeGlyphs, GlyfRecord, GlyfTable, Glyph, Point, SimpleGlyph,
    SimpleGlyphFlag,
};
use crate::tables::loca::{owned, LocaTable};
use crate::tables::{
    FontTableProvider, HeadTable, HheaTable, HmtxTable, IndexToLocFormat, LongHorMetric, MaxpTable,
    SfntVersion, TTCF_MAGIC,
};
use crate::{read_table, tag};

pub const MAGIC: u32 = tag!(b"wOF2");
// This is the default size of the buffer in the brotli crate.
// There's no guidance on how to choose this value.
const BROTLI_DECODER_BUFFER_SIZE: usize = 4096;
const BITS_0_TO_5: u8 = 0x3F;
const LOWEST_UCODE: u16 = 253;

/// UIntBase128, Variable-length encoding of 32-bit unsigned integers.
#[derive(Copy, Clone)]
pub enum U32Base128 {}

/// 255UInt16, Variable-length encoding of a 16-bit unsigned integer for optimized intermediate
/// font data storage.
#[derive(Copy, Clone)]
pub enum PackedU16 {}

#[derive(Clone, Copy)]
struct WoffFlag(u8);

#[derive(Clone)]
pub struct Woff2Font<'a> {
    pub scope: ReadScope<'a>,
    pub woff_header: Woff2Header,
    // We have to read and parse the table directory to know where the font tables are stored
    // so in doing so we hold onto the TableDirectoryEntries produced as a result
    pub table_directory: Vec<TableDirectoryEntry>,
    pub collection_directory: Option<collection::Directory>,
    pub table_data_block: Vec<u8>,
}

pub struct Woff2TableProvider {
    flavor: u32,
    tables: HashMap<u32, Box<[u8]>>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Woff2Header {
    pub flavor: u32,
    pub length: u32,
    pub num_tables: u16,
    pub total_sfnt_size: u32,
    pub total_compressed_size: u32,
    pub _major_version: u16,
    pub _minor_version: u16,
    pub meta_offset: u32,
    pub meta_length: u32,
    pub meta_orig_length: u32,
    pub priv_offset: u32,
    pub priv_length: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TableDirectoryEntry {
    pub tag: u32,
    pub offset: usize,
    pub orig_length: u32,
    pub transform_length: Option<u32>,
}

struct TransformedGlyphTable<'a> {
    /// Number of glyphs
    num_glyphs: u16,
    /// Offset format for loca table
    ///
    /// Should be consistent with indexToLocFormat of the original head table
    /// (see OpenType specification).
    _index_format: u16,
    /// Stream of i16 values representing number of contours for each glyph record
    n_contour_scope: ReadScope<'a>,
    /// Stream of values representing number of outline points for each contour in glyph records
    n_points_scope: ReadScope<'a>,
    /// Stream of u8 values representing flag values for each outline point.
    flag_scope: ReadScope<'a>,
    /// Stream of bytes representing point coordinate values using variable length encoding format (defined in subclause 5.2)
    glyph_scope: ReadScope<'a>,
    /// Stream of bytes representing component flag values and associated composite glyph data
    composite_scope: ReadScope<'a>,
    /// Bitmap (a numGlyphs-long bit array) indicating explicit bounding boxes
    bbox_bitmap_scope: ReadScope<'a>,
    /// Stream of i16 values representing glyph bounding box data
    bbox_scope: ReadScope<'a>,
    /// Stream of u8 values representing a set of instructions for each corresponding glyph
    instruction_scope: ReadScope<'a>,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct HmtxTableFlag: u8 {
        const LSB_ABSENT = 0b01;
        const LEFT_SIDE_BEARING_ABSENT = 0b10;
    }
}

pub enum Woff2GlyfTable {}
pub enum Woff2LocaTable {}
pub enum Woff2HmtxTable {}

pub struct BitSlice<'a> {
    data: &'a [u8],
}

impl<'a> Woff2Font<'a> {
    /// The "sfnt version" of the input font
    pub fn flavor(&self) -> u32 {
        self.woff_header.flavor
    }

    /// Decompress and return the extended metadata XML if present
    pub fn extended_metadata(&self) -> Result<Option<String>, ParseError> {
        let offset = usize::try_from(self.woff_header.meta_offset)?;
        let length = usize::try_from(self.woff_header.meta_length)?;
        if offset == 0 || length == 0 {
            return Ok(None);
        }

        let compressed_metadata = self.scope.offset_length(offset, length)?;

        let mut input = brotli_decompressor::Decompressor::new(
            Cursor::new(compressed_metadata.data()),
            BROTLI_DECODER_BUFFER_SIZE,
        );
        let mut metadata = String::new();
        input
            .read_to_string(&mut metadata)
            .map_err(|_err| ParseError::CompressionError)?;

        Ok(Some(metadata))
    }

    pub fn table_data_block_scope(&'a self) -> ReadScope<'a> {
        ReadScope::new(&self.table_data_block)
    }

    fn read_table_directory(
        ctxt: &mut ReadCtxt<'_>,
        num_tables: usize,
    ) -> Result<Vec<TableDirectoryEntry>, ParseError> {
        let mut offset = 0;
        let mut table_directory = Vec::with_capacity(num_tables);
        for _i in 0..num_tables {
            let entry = ctxt.read_dep::<TableDirectoryEntry>(offset)?;
            offset += entry.length();
            table_directory.push(entry);
        }

        Ok(table_directory)
    }

    pub fn find_table_entry(&self, tag: u32, index: usize) -> Option<&TableDirectoryEntry> {
        if let Some(collection_directory) = &self.collection_directory {
            collection_directory
                .get(index)
                .and_then(|font| font.table_entries(self).find(|entry| entry.tag == tag))
        } else {
            self.table_directory.iter().find(|entry| entry.tag == tag)
        }
    }

    pub fn read_table(&self, tag: u32, index: usize) -> Result<Option<ReadBuf<'_>>, ParseError> {
        self.find_table_entry(tag, index)
            .map(|entry| entry.read_table(&self.table_data_block_scope()))
            .transpose()
    }

    pub fn table_provider(&self, index: usize) -> Result<Woff2TableProvider, ReadWriteError> {
        Woff2TableProvider::new(self, index)
    }
}

impl ReadBinary for Woff2Font<'_> {
    type HostType<'a> = Woff2Font<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let woff_header = ctxt.read::<Woff2Header>()?;

        let table_directory =
            Self::read_table_directory(ctxt, usize::from(woff_header.num_tables))?;

        let collection_directory = if woff_header.flavor == TTCF_MAGIC {
            Some(ctxt.read::<collection::Directory>()?)
        } else {
            None
        };

        // Read compressed font table data
        let compressed_data =
            ctxt.read_slice(usize::try_from(woff_header.total_compressed_size)?)?;
        let mut input = brotli_decompressor::Decompressor::new(
            Cursor::new(compressed_data),
            BROTLI_DECODER_BUFFER_SIZE,
        );
        let mut table_data_block = Vec::new();
        input
            .read_to_end(&mut table_data_block)
            .map_err(|_err| ParseError::CompressionError)?;

        Ok(Woff2Font {
            scope,
            woff_header,
            table_directory,
            collection_directory,
            table_data_block,
        })
    }
}

impl FontTableProvider for Woff2TableProvider {
    fn table_data(&self, tag: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        Ok(self.tables.get(&tag).map(|table| Cow::from(table.as_ref())))
    }

    fn has_table(&self, tag: u32) -> bool {
        self.tables.contains_key(&tag)
    }

    fn table_tags(&self) -> Option<Vec<u32>> {
        Some(self.tables.keys().copied().collect())
    }
}

impl SfntVersion for Woff2TableProvider {
    fn sfnt_version(&self) -> u32 {
        self.flavor
    }
}

impl ReadBinary for Woff2Header {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let signature = ctxt.read_u32be()?;
        match signature {
            MAGIC => {
                let flavor = ctxt.read_u32be()?;
                let length = ctxt.read_u32be()?;
                let num_tables = ctxt.read_u16be()?;
                let reserved = ctxt.read_u16be()?;
                // The header includes a reserved field; this MUST be set to zero. If this field is
                // non-zero, a conforming user agent MUST reject the file as invalid.
                ctxt.check(reserved == 0)?;
                let total_sfnt_size = ctxt.read_u32be()?;
                let total_compressed_size = ctxt.read_u32be()?;
                // The WOFF majorVersion and minorVersion fields specify the version number for a
                // given WOFF file, which can be based on the version number of the input font but
                // is not required to be. These fields have no effect on font loading or usage
                // behavior in user agents.
                let _major_version = ctxt.read_u16be()?;
                let _minor_version = ctxt.read_u16be()?;
                let meta_offset = ctxt.read_u32be()?;
                let meta_length = ctxt.read_u32be()?;
                let meta_orig_length = ctxt.read_u32be()?;
                let priv_offset = ctxt.read_u32be()?;
                let priv_length = ctxt.read_u32be()?;

                Ok(Woff2Header {
                    flavor,
                    length,
                    num_tables,
                    total_sfnt_size,
                    total_compressed_size,
                    _major_version,
                    _minor_version,
                    meta_offset,
                    meta_length,
                    meta_orig_length,
                    priv_offset,
                    priv_length,
                })
            }
            _ => Err(ParseError::BadVersion),
        }
    }
}

impl ReadBinaryDep for TableDirectoryEntry {
    type Args<'a> = usize;
    type HostType<'a> = Self;

    fn read_dep(ctxt: &mut ReadCtxt<'_>, offset: usize) -> Result<Self, ParseError> {
        let flags = ctxt.read_u8()?;
        let tag = if flags & BITS_0_TO_5 == 63 {
            // Tag is the following 4 bytes
            ctxt.read_u32be()
        } else {
            Ok(KNOWN_TABLE_TAGS[usize::from(flags & BITS_0_TO_5)])
        }?;
        let transformation_version = (flags & 0xC0) >> 6;
        let orig_length = ctxt.read::<U32Base128>()?;

        let transform_length = match (transformation_version, tag) {
            (3, tag::GLYF) | (3, tag::LOCA) => None,
            (_, tag::GLYF) | (_, tag::LOCA) | (1, tag::HMTX) => Some(ctxt.read::<U32Base128>()?),
            (0, _) => None,
            _ => Some(ctxt.read::<U32Base128>()?),
        };

        Ok(TableDirectoryEntry {
            tag,
            offset,
            orig_length,
            transform_length,
        })
    }
}

impl ReadBinary for TransformedGlyphTable<'_> {
    type HostType<'a> = TransformedGlyphTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let _version = ctxt.read_u32be()?;
        let num_glyphs = ctxt.read_u16be()?;
        let index_format = ctxt.read_u16be()?;

        let n_contour_stream_size = usize::try_from(ctxt.read_u32be()?)?;
        let n_points_stream_size = usize::try_from(ctxt.read_u32be()?)?;
        let flag_stream_size = usize::try_from(ctxt.read_u32be()?)?;
        let glyph_stream_size = usize::try_from(ctxt.read_u32be()?)?;
        let composite_stream_size = usize::try_from(ctxt.read_u32be()?)?;
        let bbox_stream_size = usize::try_from(ctxt.read_u32be()?)?;
        let instruction_stream_size = usize::try_from(ctxt.read_u32be()?)?;

        // Build sub contexts for each of the streams, then iterate a glyph at a time pulling from
        // those contexts as needed
        let n_contour_scope = ReadScope::new(ctxt.read_slice(n_contour_stream_size)?);
        let n_points_scope = ReadScope::new(ctxt.read_slice(n_points_stream_size)?);
        let flag_scope = ReadScope::new(ctxt.read_slice(flag_stream_size)?);
        let glyph_scope = ReadScope::new(ctxt.read_slice(glyph_stream_size)?);
        let composite_scope = ReadScope::new(ctxt.read_slice(composite_stream_size)?);
        // The total number of bytes in bboxBitmap is equal to 4 * floor((numGlyphs + 31) / 32).
        // The bits are packed so that glyph number 0 corresponds to the most significant bit of
        // the first byte, glyph number 7 corresponds to the least significant bit of the first
        // byte, glyph number 8 corresponds to the most significant bit of the second byte, and so
        // on. A bit=1 value indicates an explicitly set bounding box.
        let bbox_bitmap_length = (4. * ((num_glyphs + 31) as f64 / 32.).floor()) as usize;
        let bbox_bitmap_scope = ReadScope::new(ctxt.read_slice(bbox_bitmap_length)?);
        let bbox_scope = ReadScope::new(ctxt.read_slice(bbox_stream_size - bbox_bitmap_length)?);
        let instruction_scope = ReadScope::new(ctxt.read_slice(instruction_stream_size)?);

        Ok(TransformedGlyphTable {
            num_glyphs,
            _index_format: index_format,
            n_contour_scope,
            n_points_scope,
            flag_scope,
            glyph_scope,
            composite_scope,
            bbox_bitmap_scope,
            bbox_scope,
            instruction_scope,
        })
    }
}

impl ReadBinaryDep for Woff2GlyfTable {
    type Args<'a> = (&'a TableDirectoryEntry, &'a LocaTable<'a>);
    type HostType<'a> = GlyfTable<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (entry, loca): Self::Args<'a>,
    ) -> Result<Self::HostType<'a>, ParseError> {
        if entry.transform_length.is_some() {
            let table = ctxt.read::<TransformedGlyphTable<'_>>()?;

            // Read a glyph at a time and handle reconstructing each one
            let num_glyphs = usize::from(table.num_glyphs);
            let mut n_contour_ctxt = table.n_contour_scope.ctxt();
            let mut n_points_ctxt = table.n_points_scope.ctxt();
            let mut flags_ctxt = table.flag_scope.ctxt();
            let mut glyphs_ctxt = table.glyph_scope.ctxt();
            let mut instructions_ctxt = table.instruction_scope.ctxt();
            let mut composite_ctxt = table.composite_scope.ctxt();
            let bbox_bitmap = BitSlice::new(table.bbox_bitmap_scope.data());
            let mut bbox_bitmap_ctxt = table.bbox_scope.ctxt();

            let mut records = Vec::with_capacity(num_glyphs);
            for i in 0..num_glyphs {
                let number_of_contours = n_contour_ctxt.read_i16be()?;

                let glyf_record = match number_of_contours {
                    // Empty glyph
                    0 => GlyfRecord::empty(),
                    // Composite glyph
                    -1 => {
                        let glyphs = composite_ctxt.read::<CompositeGlyphs>()?;

                        // Step 3a.
                        let instruction_length = if glyphs.have_instructions {
                            usize::from(glyphs_ctxt.read::<PackedU16>()?)
                        } else {
                            0
                        };
                        let instructions = instructions_ctxt.read_slice(instruction_length)?;

                        // A composite glyph MUST have an explicitly supplied bounding box.
                        // A decoder MUST check for presence of the bounding box info as part of
                        // the composite glyph record and MUST NOT load a font file with the
                        // composite bounding box data missing.
                        match bbox_bitmap.get(i) {
                            Some(true) => (),
                            _ => return Err(ParseError::BadIndex),
                        }

                        // Read the bounding box
                        let bounding_box = bbox_bitmap_ctxt.read::<BoundingBox>()?;

                        GlyfRecord::Parsed(Glyph::Composite(CompositeGlyph {
                            bounding_box,
                            glyphs: glyphs.glyphs,
                            instructions: Box::from(instructions),
                            phantom_points: None,
                        }))
                    }
                    // Simple glyph
                    num if num > 0 => {
                        let mut data = Self::decode_simple_glyph(
                            &mut n_points_ctxt,
                            &mut flags_ctxt,
                            &mut glyphs_ctxt,
                            &mut instructions_ctxt,
                            number_of_contours,
                        )?;

                        let bounding_box = match bbox_bitmap.get(i) {
                            Some(true) => bbox_bitmap_ctxt.read::<BoundingBox>(),
                            Some(false) => Ok(data.bounding_box()),
                            _ => return Err(ParseError::BadIndex),
                        }?;
                        data.bounding_box = bounding_box;

                        GlyfRecord::Parsed(Glyph::Simple(data))
                    }
                    _ => return Err(ParseError::BadValue),
                };

                records.push(glyf_record);
            }

            GlyfTable::new(records)
        } else {
            // glyf table has not been transformed
            ctxt.read_dep::<GlyfTable<'_>>(loca)
        }
    }
}

impl ReadBinaryDep for Woff2LocaTable {
    type Args<'a> = (&'a TableDirectoryEntry, u16, IndexToLocFormat);
    type HostType<'a> = LocaTable<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (loca_entry, num_glyphs, index_to_loc_format): Self::Args<'a>,
    ) -> Result<Self::HostType<'a>, ParseError> {
        if loca_entry.transform_length.is_some() {
            Ok(LocaTable::empty())
        } else {
            ctxt.read_dep::<LocaTable<'_>>((num_glyphs, index_to_loc_format))
        }
    }
}

impl ReadBinaryDep for Woff2HmtxTable {
    type Args<'a> = (&'a TableDirectoryEntry, &'a GlyfTable<'a>, usize, usize);
    type HostType<'a> = HmtxTable<'a>;

    /// Read hmtx table from WOFF2 file
    ///
    /// num_h_metrics is defined by the `hhea` table
    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (hmtx_entry, glyf, num_glyphs, num_h_metrics): Self::Args<'a>,
    ) -> Result<Self::HostType<'a>, ParseError> {
        if hmtx_entry.transform_length.is_some() {
            let flags = ctxt.read::<HmtxTableFlag>()?;
            let advance_width_stream = ctxt.read_array::<U16Be>(num_h_metrics)?;

            let lsb = if flags.lsb_is_present() {
                // read the lsb stream
                ReadArrayCow::Borrowed(ctxt.read_array::<I16Be>(num_h_metrics)?)
            } else {
                // Reconstitute lsb from glyf
                //
                // The transformation version "1" exploits the built-in redundancy of the TrueType
                // glyphs where the outlines of the glyphs designed according to the TrueType
                // recommendations would likely have their left side bearing values equal to xMin
                // value of the glyph bounding box.
                //
                // If the hmtx table transform is both applicable and desired, the encoder MUST
                // check that leftSideBearing values match the xMin values of the glyph bounding
                // box for every glyph in a font (or check that leftSideBearing == 0 for an empty
                // glyph)
                ReadArrayCow::Owned(
                    glyf.records()
                        .iter()
                        .map(|glyf_record| match glyf_record {
                            GlyfRecord::Present { .. } => unreachable!(),
                            GlyfRecord::Parsed(glyph) => {
                                glyph.bounding_box().map(|bbox| bbox.x_min).unwrap_or(0)
                            }
                        })
                        .collect(),
                )
            };

            let length = num_glyphs
                .checked_sub(num_h_metrics)
                .ok_or(ParseError::BadIndex)?;
            let left_side_bearings = if flags.left_side_bearing_is_present() {
                ReadArrayCow::Borrowed(ctxt.read_array::<I16Be>(length)?)
            } else {
                // Reconstitute from glyf
                ReadArrayCow::Owned(
                    glyf.records()
                        .iter()
                        .map(|glyf_record| match glyf_record {
                            GlyfRecord::Present { .. } => unreachable!(),
                            GlyfRecord::Parsed(glyph) => {
                                glyph.bounding_box().map(|bbox| bbox.x_min).unwrap_or(0)
                            }
                        })
                        .collect(),
                )
            };

            let h_metrics = lsb
                .iter()
                .zip(advance_width_stream.iter())
                .map(|(lsb, advance_width)| LongHorMetric { advance_width, lsb })
                .collect();

            Ok(HmtxTable {
                h_metrics: ReadArrayCow::Owned(h_metrics),
                left_side_bearings,
            })
        } else {
            ctxt.read_dep::<HmtxTable<'a>>((num_glyphs, num_h_metrics))
        }
    }
}

impl ReadFrom for WoffFlag {
    type ReadType = U8;

    fn read_from(flag: u8) -> Self {
        WoffFlag::new(flag)
    }
}

// Parse "255UInt16" Data Type
// https://w3c.github.io/woff/woff2/#255UInt16-0
impl ReadBinary for PackedU16 {
    type HostType<'a> = u16;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<u16, ParseError> {
        match ctxt.read_u8()? {
            253 => ctxt.read_u16be(),
            254 => ctxt
                .read_u8()
                .map(|value| u16::from(value) + LOWEST_UCODE * 2),
            255 => ctxt.read_u8().map(|value| u16::from(value) + LOWEST_UCODE),
            code => Ok(u16::from(code)),
        }
        .map_err(ParseError::from)
    }
}

// Parse "UIntBase128" Data Type
// https://w3c.github.io/woff/woff2/#UIntBase128-0
impl ReadBinary for U32Base128 {
    type HostType<'a> = u32;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<u32, ParseError> {
        let mut accum = 0u32;

        for i in 0..5 {
            let byte = ctxt.read_u8()?;

            // No leading 0's
            if i == 0 && byte == 0x80 {
                return Err(ParseError::BadValue);
            }

            // If any of the top 7 bits are set then << 7 would overflow
            if accum & 0xFE000000 != 0 {
                return Err(ParseError::BadValue);
            }

            // value = old value times 128 + (byte bitwise-and 127)
            accum = (accum << 7) | u32::from(byte & 0x7F);

            // Spin until most significant bit of data byte is false
            if byte & 0x80 == 0 {
                return Ok(accum);
            }
        }

        // UIntBase128 sequence exceeds 5 bytes
        Err(ParseError::BadValue)
    }
}

impl ReadFrom for HmtxTableFlag {
    type ReadType = U8;

    fn read_from(flag: u8) -> Self {
        HmtxTableFlag::from_bits_truncate(flag)
    }
}

impl WoffFlag {
    fn new(flag: u8) -> Self {
        WoffFlag(flag)
    }

    fn bytes_to_read(&self) -> usize {
        usize::from(self.xy_triplet().byte_count)
    }

    fn is_on_curve_point(&self) -> bool {
        // WOFF2 says this about the MSB of flags:
        // The most significant bit of a flag indicates whether the point is on- or off-curve point.
        // The OpenType equivalent of this bit (Simple Glyph Flags) is defined as:
        // Bit 0: If set, the point is on the curve; otherwise, it is off the curve.
        // However it appears that in WOFF2 the bit is cleared to indicate that it is on-curve.
        // I.e. opposite to OpenType. MicroType, which WOFF2 is based on adds:
        // if the most significant bit is 0, then the point is on-curve.
        self.0 & 0x80 == 0
    }

    fn xy_triplet(&self) -> &XYTriplet {
        &COORD_LUT[usize::from(self.0 & 0x7F)]
    }
}

impl From<WoffFlag> for SimpleGlyphFlag {
    fn from(woff_flag: WoffFlag) -> Self {
        if woff_flag.is_on_curve_point() {
            SimpleGlyphFlag::ON_CURVE_POINT
        } else {
            SimpleGlyphFlag::empty()
        }
    }
}

impl Woff2GlyfTable {
    fn compute_end_pts_of_contours(
        n_points_ctxt: &mut ReadCtxt<'_>,
        number_of_contours: i16,
    ) -> Result<(Vec<u16>, u16), ParseError> {
        // Read numberOfContours 255UInt16 values from the nPoints stream. Each of
        // these is the number of points of that contour. Convert this into the
        // endPtsOfContours[] array by computing the cumulative sum, then
        // subtracting one.

        // Also, the sum of all the values in the array is the total number of
        // points in the glyph, nPoints.
        let mut n_points = 0;
        let end_pts_of_contours = (0..number_of_contours)
            .map(|_i| {
                n_points_ctxt.read::<PackedU16>().map(|n_contours| {
                    n_points += n_contours;
                    n_points - 1
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok((end_pts_of_contours, n_points))
    }

    fn decode_coordinates(flag: WoffFlag, coordinates: ReadArray<'_, U8>) -> Point {
        let xy_triplet = flag.xy_triplet();

        let data = coordinates.iter().fold(0u32, |mut data, byte| {
            data <<= 8;
            data |= u32::from(byte);
            data
        });

        // Extract x-bits and y-bits from the data value
        Point(xy_triplet.dx(data), xy_triplet.dy(data))
    }

    fn decode_simple_glyph(
        n_points_ctxt: &mut ReadCtxt<'_>,
        flags_ctxt: &mut ReadCtxt<'_>,
        glyphs_ctxt: &mut ReadCtxt<'_>,
        instructions_ctxt: &mut ReadCtxt<'_>,
        number_of_contours: i16,
    ) -> Result<SimpleGlyph, ParseError> {
        // Step 1. from spec section 5.1, Decoding of Simple Glyphs
        let (end_pts_of_contours, n_points) =
            Self::compute_end_pts_of_contours(n_points_ctxt, number_of_contours)?;

        // Step 2.
        let flags = flags_ctxt.read_array::<WoffFlag>(usize::from(n_points))?;

        // Step 3.
        let mut prev_point = Point::zero();
        let mut points = Vec::with_capacity(flags.len());
        for flag in flags.iter() {
            let coordinates = glyphs_ctxt.read_array::<U8>(flag.bytes_to_read())?;
            let point = Self::decode_coordinates(flag, coordinates);

            // The x and y coordinates are stored as deltas against the previous point, with the
            // first one being implicitly against (0, 0). Here we resolve these deltas into
            // absolute (x, y) values.
            prev_point = Point(prev_point.0 + point.0, prev_point.1 + point.1);
            points.push((From::from(flag), prev_point));
        }

        // Step 4.
        let instruction_length = usize::from(glyphs_ctxt.read::<PackedU16>()?);

        // Step 5.
        let instructions = instructions_ctxt.read_slice(instruction_length)?;

        Ok(SimpleGlyph {
            bounding_box: BoundingBox::empty(), // filled in later
            end_pts_of_contours,
            instructions: Box::from(instructions),
            coordinates: points,
            phantom_points: None,
        })
    }
}

impl TableDirectoryEntry {
    fn length(&self) -> usize {
        self.transform_length.unwrap_or(self.orig_length) as usize
    }

    /// Read the contents of a table entry
    pub fn read_table<'a>(&self, scope: &ReadScope<'a>) -> Result<ReadBuf<'a>, ParseError> {
        let table_data = scope.offset_length(self.offset, self.length())?;

        Ok(ReadBuf::from(table_data.data()))
    }
}

impl HmtxTableFlag {
    pub fn lsb_is_present(self) -> bool {
        self & Self::LSB_ABSENT == Self::empty()
    }

    pub fn left_side_bearing_is_present(self) -> bool {
        self & Self::LEFT_SIDE_BEARING_ABSENT == Self::empty()
    }
}

impl<'a> BitSlice<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitSlice { data }
    }

    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.len() {
            return None;
        }

        // Find byte that holds the bit we're after
        let byte_index = index / 8;
        // The bits are packed so that glyph number 0 corresponds to the most significant bit of
        // the first byte, glyph number 7 corresponds to the least significant bit of the first
        // byte, glyph number 8 corresponds to the most significant bit of the second byte,
        // and so on.
        let shl = 8 - (index % 8) - 1;
        let mask = 1 << shl;

        Some(self.data[byte_index] & mask == mask)
    }

    pub fn len(&self) -> usize {
        self.data.len() * 8
    }
}

// The FontTableProvider implementation for WOFF2 provides some challenges because there's
// dependencies between the tables. The implementation as it stands takes the somewhat brute force
// approach of eager loading all the tables up front, which makes accessing them individually later
// much easier.
impl Woff2TableProvider {
    fn new(woff: &Woff2Font<'_>, index: usize) -> Result<Self, ReadWriteError> {
        let mut tables = HashMap::with_capacity(woff.table_directory.len());

        // if hmtx is transformed then that means we have to parse glyf
        // otherwise we only have to parse glyf if it's transformed
        let hmtx_entry = woff.find_table_entry(tag::HMTX, index);
        let glyf_entry = woff.find_table_entry(tag::GLYF, index);
        let hmtx_is_transformed = hmtx_entry
            .map(|entry| entry.transform_length.is_some())
            .unwrap_or(false);
        let glyf_is_transformed = glyf_entry
            .map(|entry| entry.transform_length.is_some())
            .unwrap_or(false);

        if hmtx_is_transformed || glyf_is_transformed {
            let glyf_entry = glyf_entry.ok_or(ParseError::MissingValue)?;
            let glyf_table = glyf_entry.read_table(&woff.table_data_block_scope())?;
            let mut head = read_table!(woff, tag::HEAD, HeadTable, index)?;
            let maxp = read_table!(woff, tag::MAXP, MaxpTable, index)?;
            let hhea = read_table!(woff, tag::HHEA, HheaTable, index)?;
            let loca_entry = woff
                .find_table_entry(tag::LOCA, index)
                .ok_or(ParseError::MissingValue)?;
            let loca = loca_entry.read_table(&woff.table_data_block_scope())?;
            let loca = loca.scope().read_dep::<Woff2LocaTable>((
                loca_entry,
                maxp.num_glyphs,
                head.index_to_loc_format,
            ))?;
            let glyf = glyf_table
                .scope()
                .read_dep::<Woff2GlyfTable>((glyf_entry, &loca))?;

            if hmtx_is_transformed {
                let hmtx_entry = hmtx_entry.ok_or(ParseError::MissingValue)?;
                let hmtx_table = hmtx_entry.read_table(&woff.table_data_block_scope())?;
                let hmtx = hmtx_table.scope().read_dep::<Woff2HmtxTable>((
                    hmtx_entry,
                    &glyf,
                    usize::from(maxp.num_glyphs),
                    usize::from(hhea.num_h_metrics),
                ))?;
                let ((), data) = write::buffer::<_, HmtxTable<'_>>(&hmtx, ())?;
                tables.insert(tag::HMTX, Box::from(data.into_inner()));
            }

            // Add head, glyf and loca
            let (loca, data) = write::buffer::<_, GlyfTable<'_>>(glyf, head.index_to_loc_format)?;
            tables.insert(tag::GLYF, Box::from(data.into_inner()));
            match loca.offsets.last() {
                Some(&last) if (last / 2) > u32::from(u16::MAX) => {
                    head.index_to_loc_format = IndexToLocFormat::Long
                }
                _ => {}
            }
            let (_placeholder, data) = write::buffer::<_, HeadTable>(&head, ())?;
            tables.insert(tag::HEAD, Box::from(data.into_inner()));
            let ((), data) = write::buffer::<_, owned::LocaTable>(loca, head.index_to_loc_format)?;
            tables.insert(tag::LOCA, Box::from(data.into_inner()));
        }

        // Add remaining tables
        for table_entry in Self::table_directory(woff, index) {
            let tag = table_entry.tag;
            if tables.contains_key(&tag) {
                // Skip tables that were inserted above
                continue;
            }
            let data: Box<[u8]> = Box::from(
                table_entry
                    .read_table(&woff.table_data_block_scope())?
                    .scope()
                    .data(),
            );
            tables.insert(tag, data);
        }

        Ok(Woff2TableProvider {
            flavor: woff.woff_header.flavor,
            tables,
        })
    }

    pub fn into_tables(self) -> HashMap<u32, Box<[u8]>> {
        self.tables
    }

    fn table_directory<'a>(
        woff: &'a Woff2Font<'a>,
        index: usize,
    ) -> impl Iterator<Item = &'a TableDirectoryEntry> {
        if let Some(collection_directory) = &woff.collection_directory {
            // NOTE(unwrap): index is determined valid in woff2_read_tables.
            Either::Left(
                collection_directory
                    .get(index)
                    .map(|font| font.table_entries(woff))
                    .unwrap(),
            )
        } else {
            Either::Right(woff.table_directory.iter())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_end_pts_of_contours() {
        let data = [2u8, 4];
        let mut ctxt = ReadScope::new(&data).ctxt();
        let (end_pts_of_contours, n_points) =
            Woff2GlyfTable::compute_end_pts_of_contours(&mut ctxt, data.len() as i16)
                .expect("unable to decode simple glyph");
        assert_eq!(end_pts_of_contours, vec![1, 5]);
        assert_eq!(n_points, 6);
    }

    #[test]
    fn test_xy_triplet_dx_dy() {
        let triplet = XYTriplet {
            byte_count: 2,
            x_bits: 8,
            y_bits: 8,
            delta_x: 1,
            delta_y: 257,
            x_is_negative: true,
            y_is_negative: false,
        };
        let data = 0x7AD2;

        assert_eq!(triplet.dx(data), -(0x7A + 1));
        assert_eq!(triplet.dy(data), 0xD2 + 257);
    }

    #[test]
    fn test_bit_slice_len() {
        let inner = vec![0b1000000, 0b00000001];
        let bits = BitSlice::new(&inner);

        assert_eq!(bits.len(), 16);
    }

    #[test]
    fn test_bit_slice_get_out_of_bounds() {
        let inner = vec![0b1000000, 0b00000001];
        let bits = BitSlice::new(&inner);

        assert_eq!(bits.get(16), None);
    }

    #[test]
    fn test_bit_slice_start() {
        let inner = vec![0b1000_0000, 0b0000_0000];
        let bits = BitSlice::new(&inner);

        assert_eq!(bits.get(0), Some(true));
    }

    #[test]
    fn test_bit_slice_middle() {
        let inner = vec![0b1111_1110, 0b1111_1111];
        let bits = BitSlice::new(&inner);

        assert_eq!(bits.get(7), Some(false));
    }

    #[test]
    fn test_bit_slice_end() {
        let inner = vec![0b0000_0000, 0b0000_0001];
        let bits = BitSlice::new(&inner);

        assert_eq!(bits.get(15), Some(true));
    }

    #[test]
    fn test_read_packed_u16() {
        assert_eq!(
            ReadScope::new(&[255, 253]).read::<PackedU16>().unwrap(),
            506
        );
        assert_eq!(ReadScope::new(&[254, 0]).read::<PackedU16>().unwrap(), 506);
        assert_eq!(
            ReadScope::new(&[253, 1, 250]).read::<PackedU16>().unwrap(),
            506
        );
        assert_eq!(ReadScope::new(&[5u8]).read::<PackedU16>().unwrap(), 5);
        assert!(ReadScope::new(&[254u8]).read::<PackedU16>().is_err());
    }

    #[test]
    fn test_read_u32base128() {
        assert_eq!(ReadScope::new(&[0x3F]).read::<U32Base128>().unwrap(), 63);
        assert_eq!(
            ReadScope::new(&[0x85, 0x07]).read::<U32Base128>().unwrap(),
            647
        );
        assert_eq!(
            ReadScope::new(&[0xFF, 0xFA, 0x00])
                .read::<U32Base128>()
                .unwrap(),
            2_096_384
        );
        assert_eq!(
            ReadScope::new(&[0x8F, 0xFF, 0xFF, 0xFF, 0x7F])
                .read::<U32Base128>()
                .unwrap(),
            0xFFFFFFFF
        );
    }

    #[test]
    fn test_read_u32base128_err() {
        // Leading zeros
        assert!(ReadScope::new(&[0x80, 0x01]).read::<U32Base128>().is_err());

        // Overflow
        assert!(ReadScope::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0x7F])
            .read::<U32Base128>()
            .is_err());

        // More than 5 bytes
        assert!(ReadScope::new(&[0x8F, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F])
            .read::<U32Base128>()
            .is_err());
    }
}
