#![deny(missing_docs)]

//! Font subsetting.

use std::collections::BTreeMap;
use std::fmt;
use std::num::Wrapping;

use crate::binary::read::{ReadArrayCow, ReadScope};
use crate::binary::write::{Placeholder, WriteBinary};
use crate::binary::write::{WriteBinaryDep, WriteBuffer, WriteContext};
use crate::binary::{long_align, U16Be, U32Be};
use crate::cff::cff2::{OutputFormat, CFF2};
use crate::cff::{CFFError, SubsetCFF, CFF};
use crate::error::{ParseError, ReadWriteError, WriteError};
use crate::post::PostTable;
use crate::tables::cmap::subset::{CmapStrategy, MappingsToKeep, NewIds, OldIds};
use crate::tables::cmap::{owned, EncodingId, PlatformId};
use crate::tables::glyf::GlyfTable;
use crate::tables::loca::{self, LocaTable};
use crate::tables::os2::Os2;
use crate::tables::{
    self, cmap, FontTableProvider, HeadTable, HheaTable, HmtxTable, IndexToLocFormat, MaxpTable,
    TableRecord,
};
use crate::{checksum, tag};

/// Minimal set of tables, suitable for PDF embedding
const PROFILE_PDF: &[u32] = &[
    tag::CMAP,
    tag::HEAD,
    tag::CVT,
    tag::FPGM,
    tag::HHEA,
    tag::HMTX,
    tag::MAXP,
    tag::NAME,
    tag::POST,
    tag::PREP,
];

/// Minimum tables required for a valid OpenType font.
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otff#required-tables>
const PROFILE_MINIMAL: &[u32] = &[
    tag::CMAP,
    tag::HEAD,
    tag::HHEA,
    tag::HMTX,
    tag::MAXP,
    tag::NAME,
    tag::OS_2,
    tag::POST,
];

/// Profiles for controlling the tables included in subset fonts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubsetProfile {
    /// Minimal set of tables, suitable for PDF embedding
    Pdf,
    /// Minimum tables required for a valid OpenType font.
    Minimal,
    /// Custom profile, allows specifying a list of tables to include.
    Custom(Vec<u32>),
}

/// Target cmap format to use when subsetting
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum CmapTarget {
    /// Use the smallest suitable cmap
    #[default]
    Unrestricted,
    /// Use a Mac Roman cmap
    ///
    /// Characters outside the Mac Roman character set will be omitted.
    MacRoman,
    /// Use a Unicode cmap format
    ///
    /// Select this option if targeting the web as browsers reject fonts with only a
    /// Mac Roman cmap.
    Unicode,
}

impl SubsetProfile {
    /// Retrieve the tables included in this subset profile
    fn get_tables(&self, extra: &[u32]) -> Vec<u32> {
        let tables = match self {
            SubsetProfile::Pdf => PROFILE_PDF,
            SubsetProfile::Minimal => PROFILE_MINIMAL,
            SubsetProfile::Custom(items) => items.as_slice(),
        };
        let mut tables = tables.to_vec();
        tables.extend_from_slice(extra);
        tables
    }

    /// Parses a custom subset profile from a string
    ///
    /// The table names may be separated by commas or whitespace, such as `gsub,vmtx,prep`.
    /// Case is ignored. Tables from the Minimal profile are included automatically.
    ///
    /// **Note:** Only the following tables may be selected. Including other tables will
    /// result in a `ParseError::BadValue` error:
    ///
    /// - cmap
    /// - head
    /// - hhea
    /// - hmtx
    /// - maxp
    /// - name
    /// - os/2
    /// - post
    /// - gpos
    /// - gsub
    /// - vhea
    /// - vmtx
    /// - gdef
    /// - cvt
    /// - fpgm
    /// - prep
    pub fn parse_custom(s: String) -> Result<Self, ParseError> {
        let mut bytes = s.into_bytes();
        let tags = bytes
            .split_mut(|&c| c == b',' || c.is_ascii_whitespace())
            .map(|name| {
                name.make_ascii_lowercase();
                match &*name {
                    b"cmap" => Ok(tag::CMAP),
                    b"head" => Ok(tag::HEAD),
                    b"hhea" => Ok(tag::HHEA),
                    b"hmtx" => Ok(tag::HMTX),
                    b"maxp" => Ok(tag::MAXP),
                    b"name" => Ok(tag::NAME),
                    b"os/2" | b"os2" | b"os_2" => Ok(tag::OS_2),
                    b"post" => Ok(tag::POST),
                    b"gpos" => Ok(tag::GPOS),
                    b"gsub" => Ok(tag::GSUB),
                    b"vhea" => Ok(tag::VHEA),
                    b"vmtx" => Ok(tag::VMTX),
                    b"gdef" => Ok(tag::GDEF),
                    b"cvt" => Ok(tag::CVT),
                    b"fpgm" => Ok(tag::FPGM),
                    b"prep" => Ok(tag::PREP),
                    _ => return Err(ParseError::BadValue),
                }
            });
        let mut tables = PROFILE_MINIMAL
            .iter()
            .copied()
            .map(Ok)
            .chain(tags)
            .collect::<Result<Vec<_>, _>>()?;
        tables.sort();
        tables.dedup();
        Ok(Self::Custom(tables))
    }
}

/// Error type returned from subsetting.
#[derive(Debug)]
pub enum SubsetError {
    /// An error occurred reading or parsing data.
    Parse(ParseError),
    /// An error occurred serializing data.
    Write(WriteError),
    /// An error occurred when interpreting CFF CharStrings
    CFF(CFFError),
    /// The glyph subset did not include glyph 0/.notdef in the first position
    NotDef,
    /// The subset glyph count exceeded the maximum number of glyphs
    TooManyGlyphs,
    /// The CFF font did not contain a sole font, which is the only supported configuration for
    /// subsetting
    InvalidFontCount,
}

pub(crate) trait SubsetGlyphs {
    /// The number of glyphs in this collection
    fn len(&self) -> usize;

    /// Return the old glyph id for the supplied new glyph id
    fn old_id(&self, new_id: u16) -> u16;

    /// Return the new glyph id for the supplied old glyph id
    fn new_id(&self, old_id: u16) -> u16;
}

pub(crate) struct FontBuilder {
    sfnt_version: u32,
    tables: BTreeMap<u32, WriteBuffer>,
    filter: TableFilter,
}

pub(crate) enum TableFilter {
    /// Include all tables
    All,
    /// Include only the selected tables
    Tables(Vec<u32>),
}

pub(crate) struct FontBuilderWithHead {
    inner: FontBuilder,
    check_sum_adjustment: Placeholder<U32Be, u32>,
    index_to_loc_format: IndexToLocFormat,
}

struct TaggedBuffer {
    tag: u32,
    buffer: WriteBuffer,
}

struct OrderedTables {
    tables: Vec<TaggedBuffer>,
    checksum: Wrapping<u32>,
}

/// Subset this font so that it only contains the glyphs with the supplied `glyph_ids`.
///
/// `glyph_ids` requirements:
///
/// * Glyph id 0, corresponding to the `.notdef` glyph must always be present.
/// * There must be no duplicate glyph ids.
///
/// If either of these requirements are not upheld this function will return
/// `ParseError::BadValue`.
pub fn subset(
    provider: &impl FontTableProvider,
    glyph_ids: &[u16],
    profile: &SubsetProfile,
    cmap_target: CmapTarget,
) -> Result<Vec<u8>, SubsetError> {
    let mappings_to_keep = MappingsToKeep::new(provider, glyph_ids, cmap_target)?;
    if provider.has_table(tag::CFF) {
        subset_cff(provider, glyph_ids, mappings_to_keep, true, profile)
    } else if provider.has_table(tag::CFF2) {
        subset_cff2(
            provider,
            glyph_ids,
            mappings_to_keep,
            false,
            OutputFormat::Type1OrCid,
            profile,
        )
    } else {
        subset_ttf(
            provider,
            glyph_ids,
            CmapStrategy::Generate(mappings_to_keep),
            profile,
        )
        .map_err(SubsetError::from)
    }
}

/// Subset a TTF font.
///
/// If `mappings_to_keep` is `None` a `cmap` table in the subset font will be omitted.
/// Otherwise it will be used to build a new `cmap` table.
fn subset_ttf(
    provider: &impl FontTableProvider,
    glyph_ids: &[u16],
    cmap_strategy: CmapStrategy,
    profile: &SubsetProfile,
) -> Result<Vec<u8>, ReadWriteError> {
    let profile_tables = profile.get_tables(&[tag::LOCA, tag::GLYF]);
    let head = ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
    let mut maxp = ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
    let loca_data = provider.read_table_data(tag::LOCA)?;
    let loca = ReadScope::new(&loca_data)
        .read_dep::<LocaTable<'_>>((maxp.num_glyphs, head.index_to_loc_format))?;
    let glyf_data = provider.read_table_data(tag::GLYF)?;
    let glyf = ReadScope::new(&glyf_data).read_dep::<GlyfTable<'_>>(&loca)?;
    let mut hhea = ReadScope::new(&provider.read_table_data(tag::HHEA)?).read::<HheaTable>()?;
    let hmtx_data = provider.read_table_data(tag::HMTX)?;
    let hmtx = ReadScope::new(&hmtx_data).read_dep::<HmtxTable<'_>>((
        usize::from(maxp.num_glyphs),
        usize::from(hhea.num_h_metrics),
    ))?;

    // Build a new post table with version set to 3, which does not contain any additional
    // PostScript data
    let post_data = provider.read_table_data(tag::POST)?;
    let mut post = ReadScope::new(&post_data).read::<PostTable<'_>>()?;
    post.header.version = 0x00030000; // version 3.0
    post.opt_sub_table = None;

    // Get the OS/2 table if needed
    let maybe_os2 = profile_tables
        .contains(&tag::OS_2)
        .then(|| {
            provider
                .read_table_data(tag::OS_2)
                .and_then(|data| ReadScope::new(&data).read_dep::<Os2>(data.len()))
        })
        .transpose()?;

    // Subset the OS/2 table if we have one and mappings
    let subset_os2 = maybe_os2.map(|os2| match &cmap_strategy {
        CmapStrategy::Generate(mappings) => subset_os2(&os2, mappings),
        CmapStrategy::MacRomanSupplied(_) | CmapStrategy::Omit => os2,
    });

    // Subset the glyphs
    let subset_glyphs = glyf.subset(glyph_ids)?;

    // Build a new cmap table
    let cmap = match cmap_strategy {
        CmapStrategy::Generate(mappings_to_keep) => {
            let mappings_to_keep = mappings_to_keep.update_to_new_ids(&subset_glyphs);
            Some(create_cmap_table(&mappings_to_keep)?)
        }
        CmapStrategy::MacRomanSupplied(cmap) => {
            Some(create_cmap_table_from_cmap_array(glyph_ids, cmap)?)
        }
        CmapStrategy::Omit => None,
    };

    // Build new maxp table
    let num_glyphs = u16::try_from(subset_glyphs.len()).map_err(ParseError::from)?;
    maxp.num_glyphs = num_glyphs;

    // Build new hhea table
    let num_h_metrics = usize::from(hhea.num_h_metrics);
    hhea.num_h_metrics = num_glyphs;

    // Build new hmtx table
    let hmtx = create_hmtx_table(&hmtx, num_h_metrics, &subset_glyphs)?;

    // Extract the new glyf table now that we're done with subset_glyphs
    let glyf = GlyfTable::from(subset_glyphs);

    // Get the remaining tables
    let cvt = provider.table_data(tag::CVT)?;
    let fpgm = provider.table_data(tag::FPGM)?;
    let name = provider.table_data(tag::NAME)?;
    let prep = provider.table_data(tag::PREP)?;

    // Build the new font
    let mut builder = FontBuilder::new(0x00010000_u32, TableFilter::Tables(profile_tables));
    if let Some(cmap) = cmap {
        builder.add_table::<_, cmap::owned::Cmap>(tag::CMAP, cmap, ())?;
    }
    if let Some(cvt) = cvt {
        builder.add_table::<_, ReadScope<'_>>(tag::CVT, ReadScope::new(&cvt), ())?;
    }
    if let Some(fpgm) = fpgm {
        builder.add_table::<_, ReadScope<'_>>(tag::FPGM, ReadScope::new(&fpgm), ())?;
    }
    builder.add_table::<_, HheaTable>(tag::HHEA, &hhea, ())?;
    builder.add_table::<_, HmtxTable<'_>>(tag::HMTX, &hmtx, ())?;
    builder.add_table::<_, MaxpTable>(tag::MAXP, &maxp, ())?;
    if let Some(name) = name {
        builder.add_table::<_, ReadScope<'_>>(tag::NAME, ReadScope::new(&name), ())?;
    }
    builder.add_table::<_, PostTable<'_>>(tag::POST, &post, ())?;
    if let Some(prep) = prep {
        builder.add_table::<_, ReadScope<'_>>(tag::PREP, ReadScope::new(&prep), ())?;
    }
    if let Some(os2) = subset_os2 {
        builder.add_table::<_, Os2>(tag::OS_2, &os2, ())?;
    }
    let mut builder = builder.add_head_table(&head)?;
    builder.add_glyf_table(glyf)?;
    builder.data()
}

fn subset_cff(
    provider: &impl FontTableProvider,
    glyph_ids: &[u16],
    mappings_to_keep: MappingsToKeep<OldIds>,
    convert_cff_to_cid_if_more_than_255_glyphs: bool,
    profile: &SubsetProfile,
) -> Result<Vec<u8>, SubsetError> {
    let cff_data = provider.read_table_data(tag::CFF)?;
    let scope = ReadScope::new(&cff_data);
    let cff: CFF<'_> = scope.read::<CFF<'_>>()?;
    if cff.name_index.len() != 1 || cff.fonts.len() != 1 {
        return Err(SubsetError::InvalidFontCount);
    }

    let head = ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
    let maxp = ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
    let hhea = ReadScope::new(&provider.read_table_data(tag::HHEA)?).read::<HheaTable>()?;
    let hmtx_data = provider.read_table_data(tag::HMTX)?;
    let hmtx = ReadScope::new(&hmtx_data).read_dep::<HmtxTable<'_>>((
        usize::from(maxp.num_glyphs),
        usize::from(hhea.num_h_metrics),
    ))?;

    // Build the new CFF table
    let cff_subset = cff.subset(glyph_ids, convert_cff_to_cid_if_more_than_255_glyphs)?;
    build_otf(
        cff_subset,
        mappings_to_keep,
        provider,
        &head,
        maxp,
        hhea,
        &hmtx,
        profile,
    )
}

fn subset_cff2(
    provider: &impl FontTableProvider,
    glyph_ids: &[u16],
    mappings_to_keep: MappingsToKeep<OldIds>,
    include_fstype: bool,
    output_format: OutputFormat,
    profile: &SubsetProfile,
) -> Result<Vec<u8>, SubsetError> {
    let cff2_data = provider.read_table_data(tag::CFF2)?;
    let scope = ReadScope::new(&cff2_data);
    let cff2: CFF2<'_> = scope.read::<CFF2<'_>>()?;
    let head = ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
    let maxp = ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
    let hhea = ReadScope::new(&provider.read_table_data(tag::HHEA)?).read::<HheaTable>()?;
    let hmtx_data = provider.read_table_data(tag::HMTX)?;
    let hmtx = ReadScope::new(&hmtx_data).read_dep::<HmtxTable<'_>>((
        usize::from(maxp.num_glyphs),
        usize::from(hhea.num_h_metrics),
    ))?;

    // Build the new CFF table
    let cff_subset = cff2.subset_to_cff(glyph_ids, provider, include_fstype, output_format)?;

    // Wrap the rest of the OpenType tables around it
    build_otf(
        cff_subset,
        mappings_to_keep,
        provider,
        &head,
        maxp,
        hhea,
        &hmtx,
        profile,
    )
}

fn build_otf(
    cff_subset: SubsetCFF<'_>,
    mappings_to_keep: MappingsToKeep<OldIds>,
    provider: &impl FontTableProvider,
    head: &HeadTable,
    mut maxp: MaxpTable,
    mut hhea: HheaTable,
    hmtx: &HmtxTable<'_>,
    profile: &SubsetProfile,
) -> Result<Vec<u8>, SubsetError> {
    let profile_tables = profile.get_tables(&[tag::CFF]);

    // Get the OS/2 table if needed
    let os2 = if profile_tables.contains(&tag::OS_2) {
        match provider.table_data(tag::OS_2) {
            Ok(Some(data)) => {
                let os2 = ReadScope::new(&data).read_dep::<Os2>(data.len())?;
                let updated_os2 = subset_os2(&os2, &mappings_to_keep);
                Some(updated_os2)
            }
            _ => None,
        }
    } else {
        None
    };

    let mappings_to_keep = mappings_to_keep.update_to_new_ids(&cff_subset);

    // Build a new post table with version set to 3, which does not contain any additional
    // PostScript data
    let post_data = provider.read_table_data(tag::POST)?;
    let mut post = ReadScope::new(&post_data).read::<PostTable<'_>>()?;
    post.header.version = 0x00030000; // version 3.0
    post.opt_sub_table = None;

    // Build a new cmap table
    let cmap = create_cmap_table(&mappings_to_keep)?;

    // Build new maxp table
    let num_glyphs = u16::try_from(cff_subset.len()).map_err(ParseError::from)?;
    maxp.num_glyphs = num_glyphs;

    // Build new hhea table
    let num_h_metrics = usize::from(hhea.num_h_metrics);
    hhea.num_h_metrics = num_glyphs;

    // Build new hmtx table
    let hmtx = create_hmtx_table(hmtx, num_h_metrics, &cff_subset)?;

    // Get the remaining tables
    let cvt = provider.table_data(tag::CVT)?;
    let fpgm = provider.table_data(tag::FPGM)?;
    let name = provider.table_data(tag::NAME)?;
    let prep = provider.table_data(tag::PREP)?;

    // Build the new font
    let mut builder = FontBuilder::new(tag::OTTO, TableFilter::Tables(profile_tables));
    builder.add_table::<_, cmap::owned::Cmap>(tag::CMAP, cmap, ())?;
    if let Some(cvt) = cvt {
        builder.add_table::<_, ReadScope<'_>>(tag::CVT, ReadScope::new(&cvt), ())?;
    }
    if let Some(fpgm) = fpgm {
        builder.add_table::<_, ReadScope<'_>>(tag::FPGM, ReadScope::new(&fpgm), ())?;
    }
    builder.add_table::<_, HheaTable>(tag::HHEA, &hhea, ())?;
    builder.add_table::<_, HmtxTable<'_>>(tag::HMTX, &hmtx, ())?;
    builder.add_table::<_, MaxpTable>(tag::MAXP, &maxp, ())?;
    if let Some(name) = name {
        builder.add_table::<_, ReadScope<'_>>(tag::NAME, ReadScope::new(&name), ())?;
    }
    if let Some(os2) = os2 {
        builder.add_table::<_, Os2>(tag::OS_2, &os2, ())?;
    }
    builder.add_table::<_, PostTable<'_>>(tag::POST, &post, ())?;
    if let Some(prep) = prep {
        builder.add_table::<_, ReadScope<'_>>(tag::PREP, ReadScope::new(&prep), ())?;
    }

    // Extract the new CFF table now that we're done with cff_subset
    let cff = CFF::from(cff_subset);
    builder.add_table::<_, CFF<'_>>(tag::CFF, &cff, ())?;
    let builder = builder.add_head_table(head)?;
    builder.data().map_err(SubsetError::from)
}

fn create_cmap_table(
    mappings_to_keep: &MappingsToKeep<NewIds>,
) -> Result<owned::Cmap, ReadWriteError> {
    let encoding_record = owned::EncodingRecord::from_mappings(mappings_to_keep)?;
    Ok(owned::Cmap {
        encoding_records: vec![encoding_record],
    })
}

fn create_cmap_table_from_cmap_array(
    glyph_ids: &[u16],
    cmap: Box<[u8; 256]>,
) -> Result<owned::Cmap, ReadWriteError> {
    use cmap::owned::{Cmap, CmapSubtable, EncodingRecord};

    if glyph_ids.len() > 256 {
        return Err(ReadWriteError::Write(WriteError::BadValue));
    }

    Ok(Cmap {
        encoding_records: vec![EncodingRecord {
            platform_id: PlatformId::MACINTOSH,
            encoding_id: EncodingId::MACINTOSH_APPLE_ROMAN,
            sub_table: CmapSubtable::Format0 {
                language: 0, // the subtable is language independent
                glyph_id_array: cmap,
            },
        }],
    })
}

/// Construct a complete font from the supplied provider and tags.
pub fn whole_font<F: FontTableProvider>(
    provider: &F,
    tags: &[u32],
) -> Result<Vec<u8>, ReadWriteError> {
    let head = ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
    let maxp = ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;

    let sfnt_version = tags
        .iter()
        .position(|&tag| tag == tag::CFF)
        .map(|_| tables::CFF_MAGIC)
        .unwrap_or(tables::TTF_MAGIC);
    let mut builder = FontBuilder::new(sfnt_version, TableFilter::All);
    let mut wants_glyf = false;
    for &tag in tags {
        match tag {
            tag::GLYF => wants_glyf = true,
            tag::HEAD | tag::MAXP | tag::LOCA => (),
            _ => {
                builder.add_table::<_, ReadScope<'_>>(
                    tag,
                    ReadScope::new(&provider.read_table_data(tag)?),
                    (),
                )?;
            }
        }
    }
    // maxp and head are required for the font to be usable, so they're always added.
    builder.add_table::<_, MaxpTable>(tag::MAXP, &maxp, ())?;
    let mut builder_with_head = builder.add_head_table(&head)?;

    // Add glyf and loca if requested, glyf implies loca. They may not be requested in the case of
    // a CFF font, or CBDT/CBLC font.
    if wants_glyf {
        let loca_data = provider.read_table_data(tag::LOCA)?;
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((maxp.num_glyphs, head.index_to_loc_format))?;
        let glyf_data = provider.read_table_data(tag::GLYF)?;
        let glyf = ReadScope::new(&glyf_data).read_dep::<GlyfTable<'_>>(&loca)?;
        builder_with_head.add_glyf_table(glyf)?;
    }
    builder_with_head.data()
}

fn create_hmtx_table<'b>(
    hmtx: &HmtxTable<'_>,
    num_h_metrics: usize,
    subset_glyphs: &impl SubsetGlyphs,
) -> Result<HmtxTable<'b>, ReadWriteError> {
    let mut h_metrics = Vec::with_capacity(num_h_metrics);

    for glyph_id in 0..subset_glyphs.len() {
        // Cast is safe as glyph indexes are 16-bit values
        let old_id = usize::from(subset_glyphs.old_id(glyph_id as u16));

        if old_id < num_h_metrics {
            h_metrics.push(hmtx.h_metrics.read_item(old_id)?);
        } else {
            // As an optimization, the number of records can be less than the number of glyphs, in which case the
            // advance width value of the last record applies to all remaining glyph IDs.
            // https://docs.microsoft.com/en-us/typography/opentype/spec/hmtx
            let mut metric = hmtx.h_metrics.read_item(num_h_metrics - 1)?;
            metric.lsb = hmtx.left_side_bearings.read_item(old_id - num_h_metrics)?;
            h_metrics.push(metric);
        }
    }

    Ok(HmtxTable {
        h_metrics: ReadArrayCow::Owned(h_metrics),
        left_side_bearings: ReadArrayCow::Owned(vec![]),
    })
}

fn subset_os2(os2: &Os2, mappings: &MappingsToKeep<OldIds>) -> Os2 {
    let (new_first, new_last) = mappings.first_last_codepoints();
    let new_unicode_mask = mappings.unicode_bitmask();
    let new_ul_unicode_range1 = (new_unicode_mask & 0xFFFF_FFFF) as u32;
    let new_ul_unicode_range2 = ((new_unicode_mask >> 32) & 0xFFFF_FFFF) as u32;
    let new_ul_unicode_range3 = ((new_unicode_mask >> 64) & 0xFFFF_FFFF) as u32;
    let new_ul_unicode_range4 = ((new_unicode_mask >> 96) & 0xFFFF_FFFF) as u32;

    Os2 {
        version: os2.version,
        x_avg_char_width: os2.x_avg_char_width, // Ideally would be recalculated based on subset glyphs
        us_weight_class: os2.us_weight_class,
        us_width_class: os2.us_width_class,
        fs_type: os2.fs_type,
        y_subscript_x_size: os2.y_subscript_x_size,
        y_subscript_y_size: os2.y_subscript_y_size,
        y_subscript_x_offset: os2.y_subscript_x_offset,
        y_subscript_y_offset: os2.y_subscript_y_offset,
        y_superscript_x_size: os2.y_superscript_x_size,
        y_superscript_y_size: os2.y_superscript_y_size,
        y_superscript_x_offset: os2.y_superscript_x_offset,
        y_superscript_y_offset: os2.y_superscript_y_offset,
        y_strikeout_size: os2.y_strikeout_size,
        y_strikeout_position: os2.y_strikeout_position,
        s_family_class: os2.s_family_class,
        panose: os2.panose,
        ul_unicode_range1: new_ul_unicode_range1,
        ul_unicode_range2: new_ul_unicode_range2,
        ul_unicode_range3: new_ul_unicode_range3,
        ul_unicode_range4: new_ul_unicode_range4,
        ach_vend_id: os2.ach_vend_id,
        fs_selection: os2.fs_selection,
        // Fonts that support supplementary characters should set the value in this field to
        // 0xFFFF if the minimum index value is a supplementary character.
        us_first_char_index: u16::try_from(new_first).unwrap_or(0xFFFF),
        us_last_char_index: u16::try_from(new_last).unwrap_or(0xFFFF),
        version0: os2.version0.clone(),
        version1: os2.version1.clone(),
        version2to4: os2.version2to4.clone(),
        version5: os2.version5.clone(),
    }
}

impl FontBuilder {
    pub fn new(sfnt_version: u32, filter: TableFilter) -> Self {
        FontBuilder {
            sfnt_version,
            tables: BTreeMap::new(),
            filter,
        }
    }

    pub fn add_table<HostType, T: WriteBinaryDep<HostType>>(
        &mut self,
        tag: u32,
        table: HostType,
        args: T::Args,
    ) -> Result<T::Output, ReadWriteError> {
        assert_ne!(tag, tag::HEAD, "head table must use add_head_table");
        assert_ne!(tag, tag::GLYF, "glyf table must use add_glyf_table");

        self.add_table_inner::<HostType, T>(tag, table, args)
    }

    pub fn table_tags(&self) -> impl Iterator<Item = u32> + '_ {
        self.tables.keys().copied()
    }

    fn add_table_inner<HostType, T: WriteBinaryDep<HostType>>(
        &mut self,
        tag: u32,
        table: HostType,
        args: T::Args,
    ) -> Result<T::Output, ReadWriteError> {
        let mut buffer = WriteBuffer::new();
        let output = T::write_dep(&mut buffer, table, args)?;

        // It's a bit wasteful doing the write when it's not needed,
        // but we need to be able to return T::Output
        if self.filter.contains(tag) {
            self.tables.insert(tag, buffer);
        }

        Ok(output)
    }

    pub fn add_head_table(
        mut self,
        table: &HeadTable,
    ) -> Result<FontBuilderWithHead, ReadWriteError> {
        let placeholder = self.add_table_inner::<_, HeadTable>(tag::HEAD, table, ())?;

        Ok(FontBuilderWithHead {
            inner: self,
            check_sum_adjustment: placeholder,
            index_to_loc_format: table.index_to_loc_format,
        })
    }
}

impl FontBuilderWithHead {
    pub fn add_glyf_table(&mut self, table: GlyfTable<'_>) -> Result<(), ReadWriteError> {
        let loca = self.inner.add_table_inner::<_, GlyfTable<'_>>(
            tag::GLYF,
            table,
            self.index_to_loc_format,
        )?;
        self.inner.add_table_inner::<_, loca::owned::LocaTable>(
            tag::LOCA,
            loca,
            self.index_to_loc_format,
        )?;

        Ok(())
    }

    /// Returns a `Vec<u8>` containing the built font
    pub fn data(mut self) -> Result<Vec<u8>, ReadWriteError> {
        let mut font = WriteBuffer::new();

        self.write_offset_table(&mut font)?;
        let table_offset =
            long_align(self.inner.tables.len() * TableRecord::SIZE + font.bytes_written());

        // Add tables in desired order
        let mut ordered_tables = self.write_table_directory(&mut font)?;

        // pad
        let length = font.bytes_written();
        let padded_length = long_align(length);
        assert_eq!(
            padded_length, table_offset,
            "offset after writing table directory is not at expected position"
        );
        font.write_zeros(padded_length - length)?;

        // Fill in check_sum_adjustment in the head table. the magic number comes from the OpenType spec.
        let headers_checksum = checksum::table_checksum(font.bytes())?;
        let checksum = Wrapping(0xB1B0AFBA) - (headers_checksum + ordered_tables.checksum);

        // Write out the font tables
        let mut placeholder = Some(self.check_sum_adjustment);
        for TaggedBuffer { tag, buffer } in ordered_tables.tables.iter_mut() {
            if *tag == tag::HEAD {
                buffer.write_placeholder(placeholder.take().unwrap(), checksum.0)?;
            }
            font.write_bytes(buffer.bytes())?;
        }

        Ok(font.into_inner())
    }

    fn write_offset_table(&self, font: &mut WriteBuffer) -> Result<(), WriteError> {
        let num_tables = u16::try_from(self.inner.tables.len())?;
        let n = max_power_of_2(num_tables);
        let search_range = (1 << n) * 16;
        let entry_selector = n;
        let range_shift = num_tables * 16 - search_range;

        U32Be::write(font, self.inner.sfnt_version)?;
        U16Be::write(font, num_tables)?;
        U16Be::write(font, search_range)?;
        U16Be::write(font, entry_selector)?;
        U16Be::write(font, range_shift)?;

        Ok(())
    }

    fn write_table_directory(
        &mut self,
        font: &mut WriteBuffer,
    ) -> Result<OrderedTables, ReadWriteError> {
        let mut tables = Vec::with_capacity(self.inner.tables.len());
        let mut checksum = Wrapping(0);
        let mut table_offset =
            long_align(self.inner.tables.len() * TableRecord::SIZE + font.bytes_written());

        for (tag, mut table) in std::mem::take(&mut self.inner.tables) {
            let length = table.len();
            let padded_length = long_align(length);
            table.write_zeros(padded_length - length)?;

            let table_checksum = checksum::table_checksum(table.bytes())?;
            checksum += table_checksum;

            let record = TableRecord {
                table_tag: tag,
                checksum: table_checksum.0,
                offset: u32::try_from(table_offset).map_err(WriteError::from)?,
                length: u32::try_from(length).map_err(WriteError::from)?,
            };

            table_offset += padded_length;
            TableRecord::write(font, &record)?;
            tables.push(TaggedBuffer { tag, buffer: table });
        }

        Ok(OrderedTables { tables, checksum })
    }
}

impl TableFilter {
    fn contains(&self, tag: u32) -> bool {
        match self {
            TableFilter::All => true,
            TableFilter::Tables(tables) => tables.contains(&tag),
        }
    }
}

/// Calculate the maximum power of 2 that is <= num
fn max_power_of_2(num: u16) -> u16 {
    15u16.saturating_sub(num.leading_zeros() as u16)
}

/// Prince specific subsetting behaviour.
///
/// prince::subset will produce a bare CFF table in the case of an input CFF font.
#[cfg(feature = "prince")]
pub mod prince {
    use super::{
        tag, FontTableProvider, MappingsToKeep, ReadScope, SubsetError, WriteBinary, WriteBuffer,
        CFF,
    };
    use crate::cff::cff2::{OutputFormat, CFF2};
    use crate::subset::{CmapTarget, SubsetProfile};
    use crate::tables::cmap::subset::CmapStrategy;
    use std::ffi::c_int;

    /// This enum describes the desired cmap generation and maps to the `cmap_target` type in Prince
    #[derive(Debug, Clone)]
    pub enum PrinceCmapTarget {
        /// Build a suitable cmap table
        Unrestricted,
        /// Build a Mac Roman cmap table
        MacRoman,
        /// Omit the cmap table entirely
        Omit,
        /// Use the supplied array as a Mac Roman cmap table
        MacRomanCmap(Box<[u8; 256]>),
    }

    impl PrinceCmapTarget {
        /// Build a new cmap from a `cmap_target` tag
        pub fn new(tag: c_int, cmap: Option<Box<[u8; 256]>>) -> Self {
            // NOTE: These tags should be kept in sync with the `cmap_target` type in Prince.
            match (tag, cmap) {
                (1, _) => PrinceCmapTarget::Unrestricted,
                (2, _) => PrinceCmapTarget::MacRoman,
                (3, _) => PrinceCmapTarget::Omit,
                (4, Some(cmap)) => PrinceCmapTarget::MacRomanCmap(cmap),
                _ => panic!("invalid value for PrinceCmapTarget: {}", tag),
            }
        }
    }

    /// Subset this font so that it only contains the glyphs with the supplied `glyph_ids`.
    ///
    /// Returns just the CFF table in the case of a CFF font, not a complete OpenType font.
    pub fn subset(
        provider: &impl FontTableProvider,
        glyph_ids: &[u16],
        cmap_target: PrinceCmapTarget,
        convert_cff_to_cid_if_more_than_255_glyphs: bool,
    ) -> Result<Vec<u8>, SubsetError> {
        if provider.has_table(tag::CFF) {
            subset_cff_table(
                provider,
                glyph_ids,
                convert_cff_to_cid_if_more_than_255_glyphs,
            )
        } else if provider.has_table(tag::CFF2) {
            subset_cff2_table(provider, glyph_ids)
        } else {
            let cmap_strategy = match cmap_target {
                PrinceCmapTarget::Unrestricted => {
                    let mappings_to_keep =
                        MappingsToKeep::new(provider, glyph_ids, CmapTarget::Unrestricted)?;
                    CmapStrategy::Generate(mappings_to_keep)
                }
                PrinceCmapTarget::MacRoman => {
                    let mappings_to_keep =
                        MappingsToKeep::new(provider, glyph_ids, CmapTarget::MacRoman)?;
                    CmapStrategy::Generate(mappings_to_keep)
                }
                PrinceCmapTarget::Omit => CmapStrategy::Omit,
                PrinceCmapTarget::MacRomanCmap(cmap) => CmapStrategy::MacRomanSupplied(cmap),
            };
            super::subset_ttf(provider, glyph_ids, cmap_strategy, &SubsetProfile::Pdf)
                .map_err(SubsetError::from)
        }
    }

    /// Subset the CFF table and discard the rest
    ///
    /// Useful for PDF because a CFF table can be embedded directly without the need to wrap it in
    /// an OTF.
    fn subset_cff_table(
        provider: &impl FontTableProvider,
        glyph_ids: &[u16],
        convert_cff_to_cid_if_more_than_255_glyphs: bool,
    ) -> Result<Vec<u8>, SubsetError> {
        let cff_data = provider.read_table_data(tag::CFF)?;
        let scope = ReadScope::new(&cff_data);
        let cff: CFF<'_> = scope.read::<CFF<'_>>()?;
        if cff.name_index.len() != 1 || cff.fonts.len() != 1 {
            return Err(SubsetError::InvalidFontCount);
        }

        // Build the new CFF table
        let cff = cff
            .subset(glyph_ids, convert_cff_to_cid_if_more_than_255_glyphs)?
            .into();

        let mut buffer = WriteBuffer::new();
        CFF::write(&mut buffer, &cff)?;

        Ok(buffer.into_inner())
    }

    /// Subset a non-variable CFF2 font into a CFF table
    pub fn subset_cff2_table(
        provider: &impl FontTableProvider,
        glyph_ids: &[u16],
    ) -> Result<Vec<u8>, SubsetError> {
        let cff2_data = provider.read_table_data(tag::CFF2)?;
        let scope = ReadScope::new(&cff2_data);
        let cff2: CFF2<'_> = scope.read::<CFF2<'_>>()?;

        // Build the new CFF table
        let cff = cff2
            .subset_to_cff(glyph_ids, provider, true, OutputFormat::CidOnly)?
            .into();

        let mut buffer = WriteBuffer::new();
        CFF::write(&mut buffer, &cff)?;

        Ok(buffer.into_inner())
    }
}

impl From<ParseError> for SubsetError {
    fn from(err: ParseError) -> SubsetError {
        SubsetError::Parse(err)
    }
}

impl From<WriteError> for SubsetError {
    fn from(err: WriteError) -> SubsetError {
        SubsetError::Write(err)
    }
}

impl From<CFFError> for SubsetError {
    fn from(err: CFFError) -> SubsetError {
        SubsetError::CFF(err)
    }
}

impl From<ReadWriteError> for SubsetError {
    fn from(err: ReadWriteError) -> SubsetError {
        match err {
            ReadWriteError::Read(err) => SubsetError::Parse(err),
            ReadWriteError::Write(err) => SubsetError::Write(err),
        }
    }
}

impl fmt::Display for SubsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubsetError::Parse(err) => write!(f, "subset: parse error: {}", err),
            SubsetError::Write(err) => write!(f, "subset: write error: {}", err),
            SubsetError::CFF(err) => write!(f, "subset: CFF error: {}", err),
            SubsetError::NotDef => write!(f, "subset: first glyph is not .notdef"),
            SubsetError::TooManyGlyphs => write!(f, "subset: too many glyphs"),
            SubsetError::InvalidFontCount => write!(f, "subset: invalid font count in CFF font"),
        }
    }
}

impl std::error::Error for SubsetError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_data::FontData;
    use crate::tables::cmap::{Cmap, CmapSubtable};
    use crate::tables::glyf::{
        BoundingBox, CompositeGlyph, CompositeGlyphArgument, CompositeGlyphComponent,
        CompositeGlyphFlag, GlyfRecord, Glyph, Point, SimpleGlyph, SimpleGlyphFlag,
    };
    use crate::tables::{LongHorMetric, OpenTypeData, OpenTypeFont};
    use crate::tag::DisplayTag;
    use crate::tests::read_fixture;
    use crate::Font;

    use std::collections::HashSet;

    macro_rules! read_table {
        ($file:ident, $scope:expr, $tag:path, $t:ty) => {
            $file
                .read_table(&$scope, $tag)
                .expect("error reading table")
                .expect("no table found")
                .read::<$t>()
                .expect("unable to parse")
        };
        ($file:ident, $scope:expr, $tag:path, $t:ty, $args:expr) => {
            $file
                .read_table(&$scope, $tag)
                .expect("error reading table")
                .expect("no table found")
                .read_dep::<$t>($args)
                .expect("unable to parse")
        };
    }

    #[test]
    fn create_glyf_and_hmtx() {
        let buffer = read_fixture("tests/fonts/opentype/SFNT-TTF-Composite.ttf");
        let fontfile = ReadScope::new(&buffer)
            .read::<OpenTypeFont<'_>>()
            .expect("error reading OpenTypeFile");
        let font = match fontfile.data {
            OpenTypeData::Single(font) => font,
            OpenTypeData::Collection(_) => unreachable!(),
        };
        let head = read_table!(font, fontfile.scope, tag::HEAD, HeadTable);
        let maxp = read_table!(font, fontfile.scope, tag::MAXP, MaxpTable);
        let hhea = read_table!(font, fontfile.scope, tag::HHEA, HheaTable);
        let loca = read_table!(
            font,
            fontfile.scope,
            tag::LOCA,
            LocaTable<'_>,
            (maxp.num_glyphs, head.index_to_loc_format)
        );
        let glyf = read_table!(font, fontfile.scope, tag::GLYF, GlyfTable<'_>, &loca);
        let hmtx = read_table!(
            font,
            fontfile.scope,
            tag::HMTX,
            HmtxTable<'_>,
            (
                usize::from(maxp.num_glyphs),
                usize::from(hhea.num_h_metrics),
            )
        );

        // 0 - .notdef
        // 2 - composite
        // 4 - simple
        let glyph_ids = [0, 2, 4];
        let subset_glyphs = glyf.subset(&glyph_ids).unwrap();
        let expected_glyf = GlyfTable::new(vec![
            GlyfRecord::empty(),
            GlyfRecord::Parsed(Glyph::Composite(CompositeGlyph {
                bounding_box: BoundingBox {
                    x_min: 205,
                    x_max: 4514,
                    y_min: 0,
                    y_max: 1434,
                },
                glyphs: vec![
                    CompositeGlyphComponent {
                        flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                            | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                            | CompositeGlyphFlag::ROUND_XY_TO_GRID
                            | CompositeGlyphFlag::MORE_COMPONENTS
                            | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET,
                        glyph_index: 3,
                        argument1: CompositeGlyphArgument::I16(3453),
                        argument2: CompositeGlyphArgument::I16(0),
                        scale: None,
                    },
                    CompositeGlyphComponent {
                        flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                            | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                            | CompositeGlyphFlag::ROUND_XY_TO_GRID
                            | CompositeGlyphFlag::MORE_COMPONENTS
                            | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET,
                        glyph_index: 4,
                        argument1: CompositeGlyphArgument::I16(2773),
                        argument2: CompositeGlyphArgument::I16(0),
                        scale: None,
                    },
                    CompositeGlyphComponent {
                        flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                            | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                            | CompositeGlyphFlag::ROUND_XY_TO_GRID
                            | CompositeGlyphFlag::MORE_COMPONENTS
                            | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET,
                        glyph_index: 5,
                        argument1: CompositeGlyphArgument::I16(1182),
                        argument2: CompositeGlyphArgument::I16(0),
                        scale: None,
                    },
                    CompositeGlyphComponent {
                        flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                            | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                            | CompositeGlyphFlag::ROUND_XY_TO_GRID
                            | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET,
                        glyph_index: 2,
                        argument1: CompositeGlyphArgument::I16(205),
                        argument2: CompositeGlyphArgument::I16(0),
                        scale: None,
                    },
                ],
                instructions: Box::default(),
                phantom_points: None,
            })),
            GlyfRecord::Parsed(Glyph::Simple(SimpleGlyph {
                bounding_box: BoundingBox {
                    x_min: 0,
                    x_max: 1073,
                    y_min: 0,
                    y_max: 1434,
                },
                end_pts_of_contours: vec![9],
                instructions: Box::default(),
                coordinates: vec![
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(0, 1434),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(1073, 1434),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(1073, 1098),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(485, 1098),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(485, 831),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(987, 831),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(987, 500),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(485, 500),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(485, 0),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point::zero(),
                    ),
                ],
                phantom_points: None,
            })),
            GlyfRecord::Parsed(Glyph::Simple(SimpleGlyph {
                bounding_box: BoundingBox {
                    x_min: 0,
                    x_max: 1061,
                    y_min: 0,
                    y_max: 1434,
                },
                end_pts_of_contours: vec![5],
                instructions: Box::default(),
                coordinates: vec![
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(0, 1434),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(485, 1434),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(485, 369),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(1061, 369),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(1061, 0),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point::zero(),
                    ),
                ],
                phantom_points: None,
            })),
            GlyfRecord::Parsed(Glyph::Simple(SimpleGlyph {
                bounding_box: BoundingBox {
                    x_min: 0,
                    x_max: 485,
                    y_min: 0,
                    y_max: 1434,
                },
                end_pts_of_contours: vec![3],
                instructions: Box::default(),
                coordinates: vec![
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(0, 1434),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(485, 1434),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(485, 0),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point::zero(),
                    ),
                ],
                phantom_points: None,
            })),
            GlyfRecord::Parsed(Glyph::Simple(SimpleGlyph {
                bounding_box: BoundingBox {
                    x_min: 0,
                    x_max: 1478,
                    y_min: 0,
                    y_max: 1434,
                },
                end_pts_of_contours: vec![7, 10],
                instructions: Box::default(),
                coordinates: vec![
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point::zero(),
                    ),
                    (SimpleGlyphFlag::ON_CURVE_POINT, Point(436, 1434)),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(1042, 1434),
                    ),
                    (SimpleGlyphFlag::ON_CURVE_POINT, Point(1478, 0)),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(975, 0),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_SHORT_VECTOR
                            | SimpleGlyphFlag::Y_SHORT_VECTOR
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(909, 244),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(493, 244),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_SHORT_VECTOR
                            | SimpleGlyphFlag::Y_SHORT_VECTOR,
                        Point(430, 0),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_SHORT_VECTOR
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                        Point(579, 565),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT
                            | SimpleGlyphFlag::X_SHORT_VECTOR
                            | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR
                            | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                        Point(825, 565),
                    ),
                    (
                        SimpleGlyphFlag::ON_CURVE_POINT | SimpleGlyphFlag::X_SHORT_VECTOR,
                        Point(702, 1032),
                    ),
                ],
                phantom_points: None,
            })),
        ])
        .unwrap();

        let num_h_metrics = usize::from(hhea.num_h_metrics);
        let hmtx = create_hmtx_table(&hmtx, num_h_metrics, &subset_glyphs).unwrap();

        let mut glyf: GlyfTable<'_> = subset_glyphs.into();
        glyf.records_mut()
            .iter_mut()
            .for_each(|rec| rec.parse().unwrap());
        assert_eq!(glyf, expected_glyf);

        let expected = vec![
            LongHorMetric {
                advance_width: 1536,
                lsb: 0,
            },
            LongHorMetric {
                advance_width: 4719,
                lsb: 205,
            },
            LongHorMetric {
                advance_width: 0,
                lsb: 0,
            },
            LongHorMetric {
                advance_width: 0,
                lsb: 0,
            },
            LongHorMetric {
                advance_width: 0,
                lsb: 0,
            },
            LongHorMetric {
                advance_width: 0,
                lsb: 0,
            },
        ];

        assert_eq!(hmtx.h_metrics.iter().collect::<Vec<_>>(), expected);
        assert_eq!(hmtx.left_side_bearings.iter().collect::<Vec<_>>(), vec![]);
    }

    #[test]
    fn font_builder() {
        // Test that reading a font in, adding all its tables and writing it out equals the
        // original font
        let buffer = read_fixture("tests/fonts/opentype/test-font.ttf");
        let fontfile = ReadScope::new(&buffer)
            .read::<OpenTypeFont<'_>>()
            .expect("error reading OpenTypeFile");
        let font = match fontfile.data {
            OpenTypeData::Single(font) => font,
            OpenTypeData::Collection(_) => unreachable!(),
        };
        let head = read_table!(font, fontfile.scope, tag::HEAD, HeadTable);
        let maxp = read_table!(font, fontfile.scope, tag::MAXP, MaxpTable);
        let hhea = read_table!(font, fontfile.scope, tag::HHEA, HheaTable);
        let loca = read_table!(
            font,
            fontfile.scope,
            tag::LOCA,
            LocaTable<'_>,
            (maxp.num_glyphs, head.index_to_loc_format)
        );
        let glyf = read_table!(font, fontfile.scope, tag::GLYF, GlyfTable<'_>, &loca);
        let hmtx = read_table!(
            font,
            fontfile.scope,
            tag::HMTX,
            HmtxTable<'_>,
            (
                usize::from(maxp.num_glyphs),
                usize::from(hhea.num_h_metrics),
            )
        );

        let mut builder = FontBuilder::new(tables::TTF_MAGIC, TableFilter::All);
        builder
            .add_table::<_, HheaTable>(tag::HHEA, &hhea, ())
            .unwrap();
        builder
            .add_table::<_, HmtxTable<'_>>(tag::HMTX, &hmtx, ())
            .unwrap();
        builder
            .add_table::<_, MaxpTable>(tag::MAXP, &maxp, ())
            .unwrap();

        let tables_added = [
            tag::HEAD,
            tag::GLYF,
            tag::HHEA,
            tag::HMTX,
            tag::MAXP,
            tag::LOCA,
        ]
        .iter()
        .collect::<HashSet<&u32>>();
        for record in font.table_records.iter() {
            if tables_added.contains(&record.table_tag) {
                continue;
            }

            let table = font
                .read_table(&fontfile.scope, record.table_tag)
                .unwrap()
                .unwrap();
            builder
                .add_table::<_, ReadScope<'_>>(record.table_tag, table, ())
                .unwrap();
        }

        let mut builder = builder.add_head_table(&head).unwrap();
        builder.add_glyf_table(glyf).unwrap();
        let data = builder.data().unwrap();

        let new_fontfile = ReadScope::new(&data)
            .read::<OpenTypeFont<'_>>()
            .expect("error reading new OpenTypeFile");
        let new_font = match new_fontfile.data {
            OpenTypeData::Single(font) => font,
            OpenTypeData::Collection(_) => unreachable!(),
        };

        assert_eq!(new_font.table_records.len(), font.table_records.len());
        for record in font.table_records.iter() {
            match record.table_tag {
                tag::GLYF | tag::LOCA => {
                    // TODO: check content of glyf and loca
                    // glyf differs because we don't do anything fancy with points at the moment
                    // and always write them out as i16 values.
                    // loca differs because glyf differs
                    continue;
                }
                tag => {
                    let new_record = new_font.find_table_record(record.table_tag).unwrap();
                    let tag = DisplayTag(tag);
                    assert_eq!((tag, new_record.checksum), (tag, record.checksum));
                }
            }
        }
    }

    #[test]
    fn invalid_glyph_id() {
        // Test to ensure that invalid glyph ids don't panic when subsetting
        let buffer = read_fixture("tests/fonts/opentype/Klei.otf");
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let mut glyph_ids = [0, 9999];

        match subset(
            &opentype_file.table_provider(0).unwrap(),
            &mut glyph_ids,
            &SubsetProfile::Pdf,
            CmapTarget::Unrestricted,
        ) {
            Err(SubsetError::Parse(ParseError::BadIndex)) => {}
            err => panic!(
                "expected SubsetError::Parse(ParseError::BadIndex) got {:?}",
                err
            ),
        }
    }

    #[test]
    fn empty_mappings_to_keep() {
        // Test to ensure that an empty mappings to keep doesn't panic when subsetting
        let buffer = read_fixture("tests/fonts/opentype/SourceCodePro-Regular.otf");
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        // glyph 118 is not Unicode, so does not end up in the mappings to keep
        let mut glyph_ids = [0, 118];
        let subset_font_data = subset(
            &opentype_file.table_provider(0).unwrap(),
            &mut glyph_ids,
            &SubsetProfile::Pdf,
            CmapTarget::Unrestricted,
        )
        .unwrap();

        let opentype_file = ReadScope::new(&subset_font_data)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let font = Font::new(opentype_file.table_provider(0).unwrap()).unwrap();
        let cmap = ReadScope::new(font.cmap_subtable_data())
            .read::<CmapSubtable<'_>>()
            .unwrap();

        // If mappings_to_keep is empty a mac roman cmap sub-table is created, which doesn't
        // care that it's empty.
        if let CmapSubtable::Format0 { glyph_id_array, .. } = cmap {
            assert!(glyph_id_array.iter().all(|x| x == 0));
        } else {
            panic!("expected cmap sub-table format 0");
        }
    }

    #[test]
    fn ttf_mappings_to_keep_is_none() {
        // Test that when subsetting a TTF font with mappings_to_keep set to None the cmap table is
        // omitted from the subset font.
        let buffer = read_fixture("tests/fonts/opentype/test-font.ttf");
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let mut glyph_ids = [0, 2];
        let subset_font_data = subset_ttf(
            &opentype_file.table_provider(0).unwrap(),
            &mut glyph_ids,
            CmapStrategy::Omit,
            &SubsetProfile::Pdf,
        )
        .unwrap();

        let opentype_file = ReadScope::new(&subset_font_data)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let table_provider = opentype_file.table_provider(0).unwrap();
        assert!(!table_provider.has_table(tag::CMAP));
    }

    // This test ensures we can call whole_font on a font without a `glyf` table (E.g. CFF).
    #[test]
    fn test_whole_font() {
        let buffer = read_fixture("tests/fonts/opentype/Klei.otf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope
            .read::<FontData<'_>>()
            .expect("unable to read FontFile");
        let provider = font_file
            .table_provider(0)
            .expect("unable to get FontTableProvider");
        let tags = [
            tag::CFF,
            tag::GDEF,
            tag::GPOS,
            tag::GSUB,
            tag::OS_2,
            tag::CMAP,
            tag::HEAD,
            tag::HHEA,
            tag::HMTX,
            tag::MAXP,
            tag::NAME,
            tag::POST,
        ];
        assert!(whole_font(&provider, &tags).is_ok());
    }

    #[test]
    fn test_max_power_of_2() {
        assert_eq!(max_power_of_2(0), 0);
        assert_eq!(max_power_of_2(1), 0);
        assert_eq!(max_power_of_2(2), 1);
        assert_eq!(max_power_of_2(4), 2);
        assert_eq!(max_power_of_2(8), 3);
        assert_eq!(max_power_of_2(16), 4);
        assert_eq!(max_power_of_2(49), 5);
        assert_eq!(max_power_of_2(std::u16::MAX), 15);
    }

    #[test]
    fn subset_cff2_type1() {
        let buffer = read_fixture("tests/fonts/opentype/cff2/SourceSans3.abc.otf");
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let provider = otf.table_provider(0).expect("error reading font file");

        // Subset the CFF2, producing CFF. Since there is only two glyphs in the subset font it
        // will produce a Type 1 CFF font.
        let new_font = subset(
            &provider,
            &[0, 1],
            &SubsetProfile::Pdf,
            CmapTarget::Unrestricted,
        )
        .unwrap();

        // Read it back
        let subset_otf = ReadScope::new(&new_font)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let provider = subset_otf
            .table_provider(0)
            .expect("error reading new font");
        let cff_data = provider
            .read_table_data(tag::CFF)
            .expect("unable to read CFF data");
        let res = ReadScope::new(&cff_data).read::<CFF<'_>>();
        assert!(res.is_ok());
        let cff = res.unwrap();
        let font = &cff.fonts[0];
        assert!(!font.is_cid_keyed());
    }

    #[test]
    fn subset_cff2_cid() {
        let buffer = read_fixture("tests/fonts/opentype/cff2/SourceSans3-Instance.256.otf");
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let provider = otf.table_provider(0).expect("error reading font file");

        // Subset the CFF2, producing CFF. Since there is more than 255 glyphs in the subset font it
        // will produce a CID-keyed CFF font.
        let glyph_ids = (0..=256).collect::<Vec<_>>();
        let new_font = subset(
            &provider,
            &glyph_ids,
            &SubsetProfile::Pdf,
            CmapTarget::Unrestricted,
        )
        .unwrap();

        // Read it back
        let subset_otf = ReadScope::new(&new_font)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let provider = subset_otf
            .table_provider(0)
            .expect("error reading new font");
        let cff_data = provider
            .read_table_data(tag::CFF)
            .expect("unable to read CFF data");
        let res = ReadScope::new(&cff_data).read::<CFF<'_>>();
        assert!(res.is_ok());
        let cff = res.unwrap();
        assert_eq!(cff.fonts.len(), 1);
        let font = &cff.fonts[0];
        assert!(font.is_cid_keyed());
    }

    #[test]
    fn test_subset_with_os2_and_unicode_cmap() {
        // Test string to use for the font subset
        let test_string = "hello world";

        // Load the font
        let buffer = read_fixture("tests/fonts/opentype/Klei.otf");
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let provider = opentype_file.table_provider(0).unwrap();

        // Create a font instance to access cmap
        let font = Font::new(provider).unwrap();

        // Get the cmap subtable for unicode mapping
        let cmap_data = font.cmap_subtable_data();
        let cmap_subtable = ReadScope::new(cmap_data)
            .read::<CmapSubtable<'_>>()
            .unwrap();

        // Map characters to glyph IDs
        let mut glyph_ids = vec![0]; // Always include glyph 0 (.notdef)

        for c in test_string.chars() {
            if let Ok(Some(glyph_id)) = cmap_subtable.map_glyph(c as u32) {
                glyph_ids.push(glyph_id);
            }
        }

        // Sort and deduplicate glyph IDs
        glyph_ids.sort();
        glyph_ids.dedup();

        // Subset the font
        let subset_buffer = subset(
            &font.font_table_provider,
            &glyph_ids,
            &SubsetProfile::Minimal,
            CmapTarget::Unicode,
        )
        .unwrap();
        drop(font); // so we don't accidentally use it below

        // Validate that the OS/2 table is present in the subsetted font
        let subset_otf = ReadScope::new(&subset_buffer)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let subset_provider = subset_otf.table_provider(0).unwrap();

        // Check that OS/2 table exists
        assert!(
            subset_provider.has_table(tag::OS_2),
            "Subset font is missing the OS/2 table."
        );

        // Read back the cmap and check that it's a unicode cmap
        let cmap_data = subset_provider.read_table_data(tag::CMAP).unwrap();
        let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>().unwrap();
        assert!(
            cmap.find_subtable(PlatformId::UNICODE, EncodingId::UNICODE_BMP)
                .is_some(),
            "subset font does not have expected Unicode cmap"
        );
    }

    #[test]
    fn test_subset_with_macroman_cmap() {
        // Test string to use for the font subset
        let test_string = "hello world";

        // Load the font
        let buffer = read_fixture("tests/fonts/opentype/Klei.otf");
        let opentype_file = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let provider = opentype_file.table_provider(0).unwrap();

        // Create a font instance to access cmap
        let font = Font::new(provider).unwrap();

        // Get the cmap subtable for unicode mapping
        let cmap_data = font.cmap_subtable_data();
        let cmap_subtable = ReadScope::new(cmap_data)
            .read::<CmapSubtable<'_>>()
            .unwrap();

        // Map characters to glyph IDs
        let mut glyph_ids = vec![0]; // Always include glyph 0 (.notdef)

        for c in test_string.chars() {
            if let Ok(Some(glyph_id)) = cmap_subtable.map_glyph(c as u32) {
                glyph_ids.push(glyph_id);
            }
        }

        // Sort and deduplicate glyph IDs
        glyph_ids.sort();
        glyph_ids.dedup();

        // Subset the font
        let subset_buffer = subset(
            &font.font_table_provider,
            &glyph_ids,
            &SubsetProfile::Minimal,
            CmapTarget::Unrestricted,
        )
        .unwrap();
        drop(font); // so we don't accidentally use it below

        let subset_otf = ReadScope::new(&subset_buffer)
            .read::<OpenTypeFont<'_>>()
            .unwrap();
        let subset_provider = subset_otf.table_provider(0).unwrap();

        // Read back the cmap and check that it's a Mac Roman cmap (because all the selected
        // glyphs are in the Mac Roman character set)
        let cmap_data = subset_provider.read_table_data(tag::CMAP).unwrap();
        let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>().unwrap();
        assert!(
            cmap.find_subtable(PlatformId::MACINTOSH, EncodingId::MACINTOSH_APPLE_ROMAN)
                .is_some(),
            "subset font does not have expected Mac Roman cmap"
        );
    }

    #[test]
    fn parse_custom_profile() {
        let tables = "fpGm,OS/2 os2,GSUB".to_string();
        let custom = SubsetProfile::parse_custom(tables)
            .unwrap()
            .get_tables(&[])
            .iter()
            .copied()
            .map(|table| DisplayTag(table).to_string())
            .collect::<Vec<_>>();
        let expected = vec![
            tag::GSUB,
            tag::OS_2,
            tag::CMAP,
            tag::FPGM,
            tag::HEAD,
            tag::HHEA,
            tag::HMTX,
            tag::MAXP,
            tag::NAME,
            tag::POST,
        ]
        .into_iter()
        .map(|table| DisplayTag(table).to_string())
        .collect::<Vec<_>>();

        assert_eq!(custom, expected)
    }

    #[test]
    fn parse_custom_profile_invalid() {
        assert!(SubsetProfile::parse_custom("toolong".to_string()).is_err());
        assert!(SubsetProfile::parse_custom("👓".to_string()).is_err());
    }
}
