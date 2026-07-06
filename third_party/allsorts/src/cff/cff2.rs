//! CFF2 font handling.
//!
//! Refer to [OpenType CFF2 spec](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2)
//! for more information.

use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Debug;

use super::{
    owned, read_local_subr_index, CFFError, CFFFont, CFFVariant, CIDData, Charset, CustomCharset,
    Dict, DictDefault, DictDelta, Encoding, FDSelect, Index, IndexU32, MaybeOwnedIndex, Operand,
    default_blue_scale, default_expansion_factor, default_font_matrix, Operator, SubsetCFF,
    Type1Data, CFF, DEFAULT_BLUE_FUZZ, DEFAULT_BLUE_SHIFT, ISO_ADOBE_LAST_SID, OFFSET_ZERO,
    OPERAND_ZERO,
    STANDARD_STRINGS,
};
use crate::binary::read::{ReadArrayCow, ReadBinary, ReadCtxt, ReadScope};
use crate::binary::write::{WriteBinary, WriteBinaryDep, WriteBuffer, WriteContext};
use crate::binary::{I16Be, U16Be, U32Be, U8};
use crate::cff::charstring::{
    operator, ArgumentsStack, CharStringConversionError, CharStringVisitor,
    CharStringVisitorContext, VariableCharStringVisitorContext, VisitOp, TWO_BYTE_OPERATOR_MARK,
};
use crate::cff::subset::{
    rebuild_global_subr_index, rebuild_local_subr_indices, rebuild_type_1_local_subr_index,
};
use crate::error::{ParseError, WriteError};
use crate::font::find_good_cmap_subtable;
use crate::glyph_info::GlyphNames;
use crate::post::PostTable;
use crate::subset::SubsetError;
use crate::tables::cmap::{Cmap, CmapSubtable};
use crate::tables::os2::{FsSelectionFlag, Os2};
use crate::tables::variable_fonts::{ItemVariationStore, OwnedTuple};
use crate::tables::{
    Fixed, FontTableProvider, HeadTable, HheaTable, HmtxTable, MaxpTable, NameTable,
};
use crate::variations::VariationError;
use crate::{cff, tag, SafeFrom, TryNumFrom};

/// Maximum number of operands in Top DICT, Font DICTs, Private DICTs and CharStrings.
///
/// > Operators in Top DICT, Font DICTs, Private DICTs and CharStrings may be preceded by up to a
/// > maximum of 513 operands.
pub const MAX_OPERANDS: usize = 513;

/// Top level representation of a CFF2 font file, typically read from a CFF2 OpenType table.
///
/// [OpenType CFF2 spec](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2)
#[derive(Clone)]
pub struct CFF2<'a> {
    /// CFF2 Header.
    pub header: Header,
    /// Top DICT with top-level properties of the font.
    pub top_dict: TopDict,
    /// INDEX of global subroutines.
    pub global_subr_index: MaybeOwnedIndex<'a>,
    /// INDEX of char strings (glyphs).
    pub char_strings_index: MaybeOwnedIndex<'a>,
    /// Item variation store. Required/present for variable fonts.
    pub vstore: Option<ItemVariationStore<'a>>,
    /// Font dict select. Maps glyph ids to Font DICTs.
    pub fd_select: Option<FDSelect<'a>>,
    /// Sub-fonts of this CFF2 font.
    ///
    /// Contains Font DICT, Private DICT, and optional local subroutine INDEX.
    pub fonts: Vec<Font<'a>>,
}

/// CFF2 Font Header
///
/// <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#6-header>
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Header {
    /// Major version (2).
    pub major: u8,
    /// Minor version.
    pub minor: u8,
    /// Size of the header in the font (maybe larger than this structure).
    pub header_size: u8,
    /// Length of the Top DICT
    pub top_dict_length: u16,
}

#[derive(Debug, PartialEq, Clone)]
pub struct TopDictDefault;

#[derive(Debug, PartialEq, Clone)]
pub struct FontDictDefault;

#[derive(Debug, PartialEq, Clone)]
pub struct PrivateDictDefault;

pub type TopDict = Dict<TopDictDefault>;

pub type FontDict = Dict<FontDictDefault>;

pub type PrivateDict = Dict<PrivateDictDefault>;

#[derive(Clone)]
pub struct Font<'a> {
    pub font_dict: FontDict,
    pub private_dict: PrivateDict,
    pub local_subr_index: Option<MaybeOwnedIndex<'a>>,
}

struct CharStringInstancer<'a> {
    new_char_string: &'a mut WriteBuffer,
}

struct StringTable<'a> {
    /// Maps strings to string ids
    strings: FxHashMap<&'a str, u16>,
    next_sid: u16,
}

// A type for holding CharString operands in their original form (int/fixed point).
#[derive(Debug, Copy, Clone)]
pub(crate) enum StackValue {
    Int(i16),
    Fixed(Fixed),
}

// The CFF format to output when subsetting CFF2 to CFF.
#[derive(Debug, Copy, Clone)]
pub enum OutputFormat {
    Type1OrCid,
    CidOnly,
}

impl<'a> CFF2<'a> {
    /// Create a non-variable instance of a variable CFF2 font according to `instance`.
    pub fn instance_char_strings(&mut self, instance: &OwnedTuple) -> Result<(), VariationError> {
        let mut new_char_strings = Vec::with_capacity(self.char_strings_index.len());

        let mut stack = ArgumentsStack {
            data: &mut [StackValue::Int(0); MAX_OPERANDS],
            len: 0,
            max_len: MAX_OPERANDS,
        };

        let vstore = self
            .vstore
            .as_ref()
            .ok_or(CFFError::MissingVariationStore)?;

        // For each glyph in the font, apply variations
        let mut new_char_string = WriteBuffer::new();
        for glyph_id in 0..self.char_strings_index.len() as u16 {
            let font_index = match &self.fd_select {
                Some(fd_select) => fd_select
                    .font_dict_index(glyph_id)
                    .ok_or(CFFError::InvalidFontIndex)?,
                None => 0,
            };
            let font = self
                .fonts
                .get(usize::from(font_index))
                .ok_or(CFFError::InvalidFontIndex)?;

            let mut instancer = CharStringInstancer {
                new_char_string: &mut new_char_string,
            };
            let variable = VariableCharStringVisitorContext { vstore, instance };
            let mut ctx = CharStringVisitorContext::new(
                glyph_id,
                &self.char_strings_index,
                font.local_subr_index.as_ref(),
                &self.global_subr_index,
                Some(variable),
            );

            stack.clear();
            ctx.visit(CFFFont::CFF2(font), &mut stack, &mut instancer)?;
            new_char_strings.push(new_char_string.bytes().to_vec());
            new_char_string.clear();
        }

        // All local subroutines should have been inlined, so they can be dropped now
        for font in self.fonts.iter_mut() {
            font.local_subr_index = None;
            font.private_dict.remove(Operator::Subrs);
            // The Private DICT has to be instanced since it can also contain a blend operator
            font.private_dict = font.private_dict.instance(instance, vstore)?;
        }
        // Global subr INDEX is required so make it empty
        self.global_subr_index = MaybeOwnedIndex::Owned(owned::Index { data: Vec::new() });
        self.char_strings_index = MaybeOwnedIndex::Owned(owned::Index {
            data: new_char_strings,
        });

        Ok(())
    }

    /// Create a subset of this CFF2 font, converting it to CFF.
    ///
    /// `glyph_ids` contains the ids of the glyphs to retain. It must begin with 0 (`.notdef`).
    pub fn subset_to_cff(
        &'a self,
        glyph_ids: &[u16],
        table_provider: &impl FontTableProvider,
        include_fstype: bool,
        output_format: OutputFormat,
    ) -> Result<SubsetCFF<'a>, SubsetError> {
        let num_glyphs = u16::try_from(glyph_ids.len()).map_err(|_| SubsetError::TooManyGlyphs)?;
        if glyph_ids.first().copied() != Some(0) {
            // .notdef must be first
            return Err(SubsetError::NotDef);
        }

        let mut fd_select = Vec::with_capacity(glyph_ids.len());
        let mut new_to_old_id = Vec::with_capacity(glyph_ids.len());
        let mut old_to_new_id =
            FxHashMap::with_capacity_and_hasher(glyph_ids.len(), Default::default());
        let mut glyph_data = Vec::with_capacity(glyph_ids.len());
        let mut used_local_subrs = FxHashMap::default();
        let mut used_global_subrs = FxHashSet::default();

        // > If generating CFF 1-compatible font instance from a CFF2 variable font that has more
        // > than one Font DICT in the Font DICT INDEX, the CFF 1 font must be written as a
        // > CID-keyed font.
        let type_1 = match output_format {
            OutputFormat::Type1OrCid => glyph_ids.len() < 256 && self.fonts.len() == 1,
            OutputFormat::CidOnly => false,
        };

        // Read tables needed for the conversion
        let cmap_data = table_provider.read_table_data(tag::CMAP)?;
        let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>()?;
        let (cmap_subtable_encoding, cmap_subtable_offset) = find_good_cmap_subtable(&cmap)
            .map(|(encoding, encoding_record)| (encoding, encoding_record.offset))
            .ok_or(ParseError::UnsuitableCmap)?;
        let cmap_subtable = ReadScope::new(&cmap_data[usize::safe_from(cmap_subtable_offset)..])
            .read::<CmapSubtable<'_>>()
            .ok()
            .map(|table| (cmap_subtable_encoding, table));
        let maxp =
            ReadScope::new(&table_provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
        let hhea =
            ReadScope::new(&table_provider.read_table_data(tag::HHEA)?).read::<HheaTable>()?;
        let hmtx_data = table_provider.read_table_data(tag::HMTX)?;
        let hmtx = ReadScope::new(&hmtx_data).read_dep::<HmtxTable<'_>>((
            usize::from(maxp.num_glyphs),
            usize::from(hhea.num_h_metrics),
        ))?;
        let post_data = table_provider
            .table_data(tag::POST)
            .unwrap()
            .map(|data| data.into_owned().into_boxed_slice());
        let post = post_data
            .as_ref()
            .map(|data| ReadScope::new(data).read::<PostTable<'_>>())
            .transpose()?;
        let head_data = table_provider.read_table_data(tag::HEAD)?;
        let head = ReadScope::new(&head_data).read::<HeadTable>()?;
        let os2_data = table_provider.read_table_data(tag::OS_2)?;
        let os2 = ReadScope::new(&os2_data).read_dep::<Os2>(os2_data.len())?;

        // Calculate the width of each glyph. These are used to update the CharStrings when
        // converting from CFF2 to CFF CharStrings.
        let widths = glyph_ids
            .iter()
            .copied()
            .map(|glyph_id| hmtx.horizontal_advance(glyph_id))
            .collect::<Result<Vec<_>, _>>()?;
        let default_width_x = mode(&widths).unwrap();
        let nominal_width_x = default_width_x;

        // Process each glyph (CharString)
        for (&glyph_id, &width) in glyph_ids.iter().zip(widths.iter()) {
            let font_index = match &self.fd_select {
                Some(fd_select) => fd_select
                    .font_dict_index(glyph_id)
                    .ok_or(CFFError::InvalidFontIndex)?,
                None => 0,
            };
            let font = self
                .fonts
                .get(usize::from(font_index))
                .ok_or(CFFError::InvalidFontIndex)?;

            // Determine which subrs are used
            let subrs = super::charstring::char_string_used_subrs(
                CFFFont::CFF2(font),
                &self.char_strings_index,
                &self.global_subr_index,
                glyph_id,
            )?;
            used_global_subrs.extend(subrs.global_subr_used);
            if !subrs.local_subr_used.is_empty() {
                used_local_subrs.insert(glyph_id, subrs.local_subr_used);
            }

            // Convert CFF2 CharString to CFF CharString
            let new_char_string = super::charstring::convert_cff2_to_cff(
                CFFFont::CFF2(font),
                &self.char_strings_index,
                &self.global_subr_index,
                glyph_id,
                width,
                default_width_x,
                nominal_width_x,
            )
            .map_err(|err| match err {
                CharStringConversionError::Write(err) => SubsetError::Write(err),
                CharStringConversionError::Cff(err) => SubsetError::CFF(err),
            })?;
            glyph_data.push(new_char_string);

            // Cast is safe as we checked that there is less than u16::MAX glyphs at the start
            old_to_new_id.insert(glyph_id, new_to_old_id.len() as u16);
            new_to_old_id.push(glyph_id);

            fd_select.push(font_index);
        }

        // Subset the Global Subr INDEX
        let global_subr_index =
            rebuild_global_subr_index(&self.global_subr_index, used_global_subrs)?;
        let char_strings_index = MaybeOwnedIndex::Owned(owned::Index { data: glyph_data });

        // Build a new Name INDEX
        // FontName/CIDFontName
        let name_data = table_provider.read_table_data(tag::NAME)?;
        let name_table = ReadScope::new(&name_data).read::<NameTable<'_>>()?;
        let font_name = name_table
            .string_for_id(NameTable::POSTSCRIPT_NAME)
            .unwrap_or_else(|| String::from("Untitled"));
        let name_index = owned::Index {
            data: vec![font_name.into_bytes()],
        };

        let mut string_table = StringTable::new();
        let glyph_names;

        // Create the charset
        let charset = if type_1 {
            // Determine the Charset string ids
            // Skip the first glyph_id as it is zero/.notdef which is implied in the charset
            let glyph_namer = GlyphNames::new(&cmap_subtable, post_data.clone());
            glyph_names = glyph_namer.unique_glyph_names(&glyph_ids[1..]);
            let charset_sids = glyph_names
                .iter()
                .map(|name| string_table.get_or_insert(name))
                .collect::<Vec<_>>();

            let iso_adobe = 1..=ISO_ADOBE_LAST_SID;
            if charset_sids
                .iter()
                .zip(iso_adobe)
                .all(|(sid, iso_adobe_sid)| *sid == iso_adobe_sid)
            {
                Charset::ISOAdobe
            } else {
                Charset::Custom(CustomCharset::Format0 {
                    glyphs: ReadArrayCow::Owned(charset_sids),
                })
            }
        } else {
            // Because we are using identity encoding the glyph ids map to the CIDs
            // in a CID-keyed CFF.
            Charset::Custom(CustomCharset::Format0 {
                glyphs: ReadArrayCow::Owned(glyph_ids[1..].to_vec()),
            })
        };

        // Top DICT
        let mut top_dict = cff::TopDict::new();

        if !type_1 {
            // The Top DICT of CID fonts start with ROS to identify it as a CID font
            //
            // > If generating CFF 1-compatible font instance from a CFF2 variable font that has
            // > more than one Font DICT in the Font DICT INDEX, the CFF 1 font must be written as a
            // > CID-keyed font. The ROS used should be Adobe-Identity-0. This maps all glyph IDs to
            // > a CID of the same value and carries no semantic content.
            let registry = Operand::Integer(string_table.get_or_insert("Adobe").into());
            let ordering = Operand::Integer(string_table.get_or_insert("Identity").into());
            let supplement = Operand::Integer(0);
            top_dict
                .inner_mut()
                .push((Operator::ROS, vec![registry, ordering, supplement]));
        }

        // If the FontMatrix operator is present then copy it across
        if let Some(matrix) = self.top_dict.get(Operator::FontMatrix) {
            top_dict
                .inner_mut()
                .push((Operator::FontMatrix, matrix.to_vec()));
        }

        // Version
        //
        // > Equivalent to the fontRevision field in the 'head' table. A CFF 1 version operand can
        // > be derived from the fontRevision field, which is a 16.16 Fixed value, and formatting it
        // > as a decimal number with three decimal places of precision.
        let version = format!("{:.3}", f32::from(head.font_revision));
        let sid = string_table.get_or_insert(&version);
        top_dict
            .inner_mut()
            .push((Operator::Version, vec![Operand::Integer(sid.into())]));

        // Notice
        //
        // > Equivalent to the concatenation of strings from the 'name' table: the Copyright string
        // > (name ID 0), a space, followed by the Trademark string (name ID 7).
        let copyright = name_table
            .string_for_id(NameTable::COPYRIGHT_NOTICE)
            .unwrap_or_else(|| String::from("Unspecified"));
        let notice = if let Some(trademark) = name_table.string_for_id(NameTable::TRADEMARK) {
            format!("{copyright} {trademark}")
        } else {
            copyright.clone()
        };
        // Add notice to the string table and push its SID as the operand
        let sid = string_table.get_or_insert(&notice);
        top_dict
            .inner_mut()
            .push((Operator::Notice, vec![Operand::Integer(sid.into())]));

        // Copyright
        let sid = string_table.get_or_insert(&copyright);
        top_dict
            .inner_mut()
            .push((Operator::Copyright, vec![Operand::Integer(sid.into())]));

        // Full Name
        //
        // > Full font name that reflects all family and relevant subfamily descriptors.
        // > The full font name is generally a combination of name IDs 1 and 2,
        // > or of name IDs 16 and 17, or a similar human-readable variant.
        let full_name = name_table
            .string_for_id(NameTable::FULL_FONT_NAME)
            .or_else(|| {
                match (
                    name_table.string_for_id(NameTable::FONT_FAMILY_NAME),
                    name_table.string_for_id(NameTable::FONT_SUBFAMILY_NAME),
                ) {
                    (Some(family), Some(subfamily)) => Some(format!("{family} {subfamily}")),
                    _ => None,
                }
            })
            .or_else(|| {
                match (
                    name_table.string_for_id(NameTable::TYPOGRAPHIC_FAMILY_NAME),
                    name_table.string_for_id(NameTable::TYPOGRAPHIC_SUBFAMILY_NAME),
                ) {
                    (Some(family), Some(subfamily)) => Some(format!("{family} {subfamily}")),
                    _ => None,
                }
            })
            .unwrap_or_else(|| {
                let bold = os2.fs_selection.contains(FsSelectionFlag::BOLD);
                let italic = os2.fs_selection.contains(FsSelectionFlag::ITALIC);
                match (bold, italic) {
                    (true, true) => String::from("Unknown Bold Italic"),
                    (true, false) => String::from("Unknown Bold"),
                    (false, true) => String::from("Unknown Italic"),
                    (false, false) => String::from("Unknown Regular"),
                }
            });
        let sid = string_table.get_or_insert(&full_name);
        top_dict
            .inner_mut()
            .push((Operator::FullName, vec![Operand::Integer(sid.into())]));

        // Family Name
        let family_name = name_table
            .string_for_id(NameTable::TYPOGRAPHIC_FAMILY_NAME)
            .or_else(|| name_table.string_for_id(NameTable::FONT_FAMILY_NAME))
            .unwrap_or_else(|| String::from("Unknown"));
        let sid = string_table.get_or_insert(&family_name);
        top_dict
            .inner_mut()
            .push((Operator::FamilyName, vec![Operand::Integer(sid.into())]));

        // Weight
        // In the Top DICT the weight is stored as a string, map the numeric us_weight_class to
        // a weight name, favouring names in the CFF standard strings
        let weight = match os2.us_weight_class {
            0..=149 => "Thin",
            150..=249 => "Extra-light",
            250..=349 => "Light",
            350..=449 => "Regular",
            450..=549 => "Medium",
            550..=649 => "Semibold",
            650..=749 => "Bold",
            750..=849 => "Extra-bold",
            850.. => "Black",
        };
        let sid = string_table.get_or_insert(weight);
        top_dict
            .inner_mut()
            .push((Operator::Weight, vec![Operand::Integer(sid.into())]));

        // FontBBox
        // Default is 0 0 0 0, so only add if any value is non-zero
        if [head.x_min, head.y_min, head.x_max, head.y_max]
            .iter()
            .any(|val| *val != 0)
        {
            let bbox = vec![
                Operand::Integer(head.x_min.into()),
                Operand::Integer(head.y_min.into()),
                Operand::Integer(head.x_max.into()),
                Operand::Integer(head.y_max.into()),
            ];
            top_dict.inner_mut().push((Operator::FontBBox, bbox));
        }

        // All these operators have defaults so if the `post` table is absent the defaults will be
        // used.
        if let Some(post) = post {
            let is_fixed_pitch = post.header.is_fixed_pitch;
            if is_fixed_pitch != 0 {
                top_dict
                    .inner_mut()
                    .push((Operator::IsFixedPitch, vec![Operand::Integer(1)]));
            }

            let italic_angle = post.header.italic_angle;
            if italic_angle != 0 {
                top_dict
                    .inner_mut()
                    .push((Operator::ItalicAngle, vec![Operand::Integer(italic_angle)]));
            }

            let underline_position = post.header.underline_position;
            if underline_position != -100 {
                top_dict.inner_mut().push((
                    Operator::UnderlinePosition,
                    vec![Operand::Integer(underline_position.into())],
                ));
            }

            let underline_thickness = post.header.underline_thickness;
            if underline_thickness != 50 {
                top_dict.inner_mut().push((
                    Operator::UnderlineThickness,
                    vec![Operand::Integer(underline_thickness.into())],
                ));
            }
        }

        // PaintType: There is no equivalent in a CFF2 font. If deriving CFF 1-compatible data,
        // use the CFF 1 default value of zero.
        // StrokeWidth: Use default

        // PostScript
        // > There is no equivalent in a CFF2 font. CFF 1 allowed for embedded PostScript code, but
        // > this was only ever used in CFF 1 OpenType fonts to provide an FSType key in the Top
        // > DICT, to carry the font embedding permissions from the fsType field of the OS/2 table.
        // > If deriving CFF 1-compatible data, the value can be copied from the OS/2 table.
        //
        // — https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#table-19-cff-1-top-dict-operators-not-used-in-cff2
        //
        // > When OpenType fonts are converted into CFF for embedding in
        // > a document, the font embedding information specified by the
        // > FSType bits, and the type of the original font, should be included
        // > in the resulting file. (See Technical Note #5147: “Font Embed-
        // > ding Guidelines for Adobe Third-party Developers,” for more
        // > information.)
        // >
        // > The embedding information is added to the Top DICT using the
        // > PostScript operator (12 21) with a SID operand. The SID points to
        // > a string containing the PostScript commands and arguments in
        // > the String INDEX.
        //
        // — https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf pp56

        // [tx](http://adobe-type-tools.github.io/afdko/AFDKO-Overview.html#tx) emits a warning
        // when this is present in addition to the fsType in the OS/2 table. Only include it
        // when requested by caller.
        let post_script;
        if include_fstype {
            post_script = format!("/FSType {} def /OrigFontType /TrueType def", os2.fs_type);
            let sid = string_table.get_or_insert(&post_script);
            top_dict
                .inner_mut()
                .push((Operator::PostScript, vec![Operand::Integer(sid.into())]));
        }

        // CIDFontVersion: default
        // CIFFontRevision: default
        // CIDFontType: default

        // CIDCount
        if !type_1 {
            top_dict.inner_mut().push((
                Operator::CIDCount,
                vec![Operand::Integer(num_glyphs.into())],
            ));
        }

        // Charset - will be updated when writing
        //
        // The choice of 1 for the placeholder offset is load bearing. The Charset and Encoding
        // operators take an offset as their operand but unlike other operators that do this they
        // also have a default of 0 since there are some reserved values that refer to predefined
        // data. If 0 is used as the placeholder here when calculating the size of the TopDict
        // during the WriteBinary impl CFF the operator is omitted since it matches the default.
        // However, when the Top DICT is written the offset is always written since it's updated
        // the actual offset when it does not refer to predefined data. Using 1 here ensures that
        // it is a non-default value and is not omitted when calculating the size of the written
        // Top DICT.
        top_dict
            .inner_mut()
            .push((Operator::Charset, vec![Operand::Offset(1)]));

        // Encoding - omitted as we use the default, Standard Encoding

        // CharStrings INDEX offset, will be updated when writing Top DICT
        top_dict
            .inner_mut()
            .push((Operator::CharStrings, vec![Operand::Offset(0)]));

        // The font names need to be stored out here because string_table borrows them. So they
        // have to live as long as that.
        let mut font_names = if type_1 {
            Vec::new()
        } else {
            Vec::with_capacity(self.fonts.len())
        };

        // Private DICT and CID/Type 1 specific structures
        let variant = if type_1 {
            let font = &self.fonts[0]; // There can only be one font if type_1 is true

            // Build new local_subr_index
            let local_subr_index = rebuild_type_1_local_subr_index(
                self.fonts[0].local_subr_index.as_ref(),
                used_local_subrs,
            )?;

            // Build new Private DICT
            let mut private_dict = cff::PrivateDict::new();
            for (op, operands) in font.private_dict.iter() {
                // Filter out Subr ops in the Private DICT if the local subr INDEX is None.
                if *op == Operator::Subrs && local_subr_index.is_none() {
                    continue;
                }

                private_dict.inner_mut().push((*op, operands.clone()));
            }
            private_dict.inner_mut().push((
                Operator::DefaultWidthX,
                vec![Operand::Integer(default_width_x.into())],
            ));
            private_dict.inner_mut().push((
                Operator::NominalWidthX,
                vec![Operand::Integer(nominal_width_x.into())],
            ));

            // Ensure Private operator is in Top DICT
            // Size and offset operands will be updated when written out. Both are set to offsets
            // so their output size is predictable
            top_dict.inner_mut().push((
                Operator::Private,
                vec![Operand::Offset(0), Operand::Offset(0)],
            ));

            let type1 = Type1Data {
                encoding: Encoding::Standard,
                private_dict,
                local_subr_index,
            };

            CFFVariant::Type1(type1)
        } else {
            // Populate font names
            (0..self.fonts.len()).for_each(|i| {
                let name = format!("{family_name}-{weight}-Part{}", i + 1);
                font_names.push(name);
            });

            // Build Font DICT and Private DICT for each font
            let mut private_dicts = Vec::with_capacity(self.fonts.len());
            let mut font_dicts = Vec::with_capacity(self.fonts.len());
            for (font, name) in self.fonts.iter().zip(font_names.iter()) {
                // Font DICT
                let mut font_dict = cff::FontDict::new();
                for (op, operands) in font.font_dict.iter() {
                    font_dict.inner_mut().push((*op, operands.clone()));
                }

                let sid = string_table.get_or_insert(name);
                font_dict.replace(Operator::FontName, vec![Operand::Integer(sid.into())]);

                let mut buf = WriteBuffer::new();
                cff::FontDict::write_dep(&mut buf, &font_dict, DictDelta::new())?;
                font_dicts.push(buf.into_inner());

                // Private DICT
                let mut private_dict = cff::PrivateDict::new();
                for (op, operands) in font.private_dict.iter() {
                    // Filter out Subr ops in the Private DICT if the local subr INDEX is None
                    // for this DICT.
                    if *op == Operator::Subrs && font.local_subr_index.is_none() {
                        continue;
                    }
                    private_dict.inner_mut().push((*op, operands.clone()));
                }
                private_dict.inner_mut().push((
                    Operator::DefaultWidthX,
                    vec![Operand::Integer(default_width_x.into())],
                ));
                private_dict.inner_mut().push((
                    Operator::NominalWidthX,
                    vec![Operand::Integer(nominal_width_x.into())],
                ));
                private_dicts.push(private_dict);
            }
            let font_dict_index = MaybeOwnedIndex::Owned(owned::Index { data: font_dicts });

            // Add placeholders for FDArray and FDSelect
            top_dict
                .inner_mut()
                .push((Operator::FDArray, vec![Operand::Offset(0)]));
            top_dict
                .inner_mut()
                .push((Operator::FDSelect, vec![Operand::Offset(0)]));

            // Ideally we'd combine this with rebuilding the subr indices below
            let (rebuild, local_subr_indices) = if used_local_subrs.is_empty() {
                // If there is no used local subrs then all the indices will be None
                (false, vec![None; self.fonts.len()])
            } else {
                (
                    true,
                    self.fonts
                        .iter()
                        .map(|font| font.local_subr_index.clone())
                        .collect(),
                )
            };

            let mut cid_data = CIDData {
                font_dict_index,
                private_dicts,
                local_subr_indices,
                fd_select: FDSelect::Format0 {
                    glyph_font_dict_indices: ReadArrayCow::Owned(fd_select),
                },
            };

            // Build new local_subr_indices, but only if any local subrs are actually present
            if rebuild {
                cid_data.local_subr_indices =
                    rebuild_local_subr_indices(&cid_data, used_local_subrs)?;
            }

            CFFVariant::CID(cid_data)
        };

        let font = cff::Font {
            top_dict,
            char_strings_index,
            charset,
            data: variant,
        };

        // Build new String INDEX
        let string_index = string_table.into_string_index();

        let cff = CFF {
            header: super::Header {
                major: 1,
                minor: 0,
                hdr_size: 4, // Ignored by WriteBinary
                off_size: 4, // We always use 32-bit offsets
            },
            name_index: MaybeOwnedIndex::Owned(name_index),
            string_index: MaybeOwnedIndex::Owned(string_index),
            global_subr_index,
            fonts: vec![font],
        };

        Ok(SubsetCFF::new(cff, new_to_old_id, old_to_new_id))
    }
}

impl<'a> StringTable<'a> {
    fn new() -> Self {
        // Load the standard strings into the lookup table
        // NOTE(cast): Safe as STANDARD_STRINGS has statically known valid length
        let strings = STANDARD_STRINGS
            .iter()
            .enumerate()
            .map(|(sid, &string)| (string, sid as u16))
            .collect();
        StringTable {
            strings,
            next_sid: STANDARD_STRINGS.len() as u16,
        }
    }

    /// find the name in the standard strings, or the string index, or insert into the string index
    fn get_or_insert(&mut self, s: &'a str) -> u16 {
        // Do a little dance to avoid borrowck errors mutating self.next_sid inside or_insert_with.
        let mut next_sid = self.next_sid;

        let sid = *self.strings.entry(s).or_insert_with(|| {
            let sid = next_sid;
            next_sid += 1;
            sid
        });

        if next_sid != self.next_sid {
            self.next_sid = next_sid;
        }
        sid
    }

    pub fn into_string_index(self) -> owned::Index {
        let mut data = Vec::with_capacity(self.strings.len());
        let mut strings = self.strings.into_iter().collect::<Vec<_>>();
        strings.sort_unstable_by_key(|(_string, sid)| *sid);
        let non_standard_strings = strings.into_iter().filter_map(|(string, sid)| {
            (usize::from(sid) >= STANDARD_STRINGS.len()).then_some(string)
        });

        for string in non_standard_strings {
            data.push(string.as_bytes().to_vec());
        }
        owned::Index { data }
    }
}

impl CharStringVisitor<StackValue, VariationError> for CharStringInstancer<'_> {
    fn visit(
        &mut self,
        op: VisitOp,
        stack: &ArgumentsStack<'_, StackValue>,
    ) -> Result<(), VariationError> {
        match op {
            VisitOp::HorizontalStem
            | VisitOp::VerticalStem
            | VisitOp::HorizontalStemHintMask
            | VisitOp::VerticalStemHintMask
            | VisitOp::VerticalMoveTo
            | VisitOp::LineTo
            | VisitOp::HorizontalLineTo
            | VisitOp::VerticalLineTo
            | VisitOp::CurveTo
            | VisitOp::HintMask
            | VisitOp::CounterMask
            | VisitOp::MoveTo
            | VisitOp::HorizontalMoveTo
            | VisitOp::CurveLine
            | VisitOp::LineCurve
            | VisitOp::VvCurveTo
            | VisitOp::HhCurveTo
            | VisitOp::VhCurveTo
            | VisitOp::HvCurveTo => {
                write_stack(self.new_char_string, stack)?;
                Ok(U8::write(self.new_char_string, op)?)
            }
            VisitOp::Return | VisitOp::Endchar => {
                // Removed in CFF2
                Err(CFFError::InvalidOperator.into())
            }
            VisitOp::Hflex | VisitOp::Flex | VisitOp::Hflex1 | VisitOp::Flex1 => {
                write_stack(self.new_char_string, stack)?;
                U8::write(self.new_char_string, TWO_BYTE_OPERATOR_MARK)?;
                Ok(U8::write(self.new_char_string, op)?)
            }
            VisitOp::VsIndex | VisitOp::Blend => Ok(()),
        }
    }

    fn hint_data(&mut self, _op: VisitOp, hints: &[u8]) -> Result<(), VariationError> {
        Ok(self.new_char_string.write_bytes(hints)?)
    }
}

impl ReadBinary for CFF2<'_> {
    type HostType<'a> = CFF2<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        // Get a scope that starts at the beginning of the CFF data. This is needed for reading
        // data that is specified as an offset from the start of the data later.
        let scope = ctxt.scope();

        let header = ctxt.read::<Header>()?;
        let top_dict_data = ctxt.read_slice(usize::from(header.top_dict_length))?;
        let top_dict = ReadScope::new(top_dict_data)
            .ctxt()
            .read_dep::<TopDict>(MAX_OPERANDS)?;
        let global_subr_index = ctxt.read::<IndexU32>().map(MaybeOwnedIndex::Borrowed)?;

        // CharStrings index
        let char_strings_offset = top_dict
            .get_i32(Operator::CharStrings)
            .unwrap_or(Err(ParseError::MissingValue))?;
        let char_strings_index = scope
            .offset(usize::try_from(char_strings_offset)?)
            .read::<IndexU32>()
            .map(MaybeOwnedIndex::Borrowed)?;
        let n_glyphs = char_strings_index.len();

        // Font DICT Index
        let fd_array_offset = top_dict
            .get_i32(Operator::FDArray)
            .unwrap_or(Err(ParseError::MissingValue))?;
        let fd_array = scope
            .offset(usize::try_from(fd_array_offset)?)
            .read::<IndexU32>()?;

        // FDSelect if more than one font is present
        let fd_select = if fd_array.count > 1 {
            let fs_select_offset = top_dict
                .get_i32(Operator::FDSelect)
                .unwrap_or(Err(ParseError::MissingValue))?;
            scope
                .offset(usize::try_from(fs_select_offset)?)
                .read_dep::<FDSelect<'a>>(n_glyphs)
                .map(Some)?
        } else {
            None
        };

        // VariationStore for variable fonts (required for variable fonts, absent otherwise)
        let vstore = top_dict
            .get_i32(Operator::VStore)
            .transpose()?
            .map(|offset| {
                let mut ctxt = scope.offset(usize::try_from(offset)?).ctxt();
                // "The VariationStore data is comprised of two parts: a uint16 field that specifies
                // a length, followed by an Item Variation Store structure of the specified length."
                let _length = ctxt.read_u16be()?;
                ctxt.read::<ItemVariationStore<'_>>()
            })
            .transpose()?;

        // Font/glyph data
        let mut fonts = Vec::with_capacity(fd_array.count);
        for font_index in 0..fd_array.count {
            let font_dict = fd_array.read::<FontDict>(font_index, MAX_OPERANDS)?;
            let (private_dict, private_dict_offset) =
                font_dict.read_private_dict::<PrivateDict>(&scope, MAX_OPERANDS)?;
            let local_subr_index =
                read_local_subr_index::<_, IndexU32>(&scope, &private_dict, private_dict_offset)?
                    .map(MaybeOwnedIndex::Borrowed);

            fonts.push(Font {
                font_dict,
                private_dict,
                local_subr_index,
            });
        }

        Ok(CFF2 {
            header,
            top_dict,
            global_subr_index,
            char_strings_index,
            vstore,
            fd_select,
            fonts,
        })
    }
}

impl WriteBinary for CFF2<'_> {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, cff2: Self) -> Result<Self::Output, WriteError> {
        // Build new Top DICT
        let mut top_dict = TopDict::new();
        if let Some(font_matrix) = cff2.top_dict.get(Operator::FontMatrix) {
            top_dict
                .inner_mut()
                .push((Operator::FontMatrix, font_matrix.to_vec()))
        }
        // Add CharStrings INDEX, FDSelect, and FDArray offsets.
        // Actual offsets will be filled in when writing
        top_dict
            .inner_mut()
            .push((Operator::CharStrings, OFFSET_ZERO.to_vec()));
        top_dict
            .inner_mut()
            .push((Operator::FDArray, OFFSET_ZERO.to_vec()));
        if cff2.fonts.len() > 1 {
            // The FDSelect operator and the structure it points to are required if the Font DICT INDEX
            // contains more than one Font DICT, else it must be omitted.
            top_dict
                .inner_mut()
                .push((Operator::FDSelect, OFFSET_ZERO.to_vec()));
        }
        if cff2.vstore.is_some() {
            top_dict
                .inner_mut()
                .push((Operator::VStore, OFFSET_ZERO.to_vec()));
        }

        // Calculate size of TopDict
        let mut write_buffer = WriteBuffer::new();
        TopDict::write_dep(&mut write_buffer, &top_dict, DictDelta::new())?;
        let top_dict_length = u16::try_from(write_buffer.bytes_written())?;

        // Now that the size of the Top DICT is known we can write out the header
        let header = Header {
            top_dict_length,
            ..cff2.header
        };
        Header::write(ctxt, header)?;

        // Reserve space for the Top DICT to be filled in later when the offsets it holds are resolved.
        let top_dict_placeholder = ctxt.reserve::<TopDict, _>(usize::from(top_dict_length))?;

        // Write global Subr INDEX
        MaybeOwnedIndex::write32(ctxt, &cff2.global_subr_index)?;

        // CharStrings INDEX
        top_dict.replace(
            Operator::CharStrings,
            vec![Operand::Offset(i32::try_from(ctxt.bytes_written())?)],
        );
        MaybeOwnedIndex::write32(ctxt, &cff2.char_strings_index)?;

        // FDSelect
        match &cff2.fd_select {
            Some(fd_select) if cff2.fonts.len() > 1 => {
                top_dict.replace(
                    Operator::FDSelect,
                    vec![Operand::Offset(i32::try_from(ctxt.bytes_written())?)],
                );
                FDSelect::write(ctxt, fd_select)?;
            }
            // If there is more than one font then FDSelect is required
            None if cff2.fonts.len() > 1 => return Err(WriteError::BadValue),
            Some(_) | None => {}
        }

        // Write out Private DICTs and Local Subr INDEXes so the offsets can be updated
        let mut font_dicts = Vec::with_capacity(cff2.fonts.len());
        for font in &cff2.fonts {
            // Write Local Subr INDEX
            let local_subr_offset = if let Some(local_subr_index) = &font.local_subr_index {
                let local_subr_offset = i32::try_from(ctxt.bytes_written())?;
                MaybeOwnedIndex::write32(ctxt, local_subr_index)?;
                Some(local_subr_offset)
            } else {
                None
            };

            // Write Private DICT
            // NOTE: The offset to local subrs INDEX is from the start of the Private DICT.
            let private_dict_offset = i32::try_from(ctxt.bytes_written())?;
            let mut private_dict_deltas = DictDelta::new();
            if let Some(local_subr_offset) = local_subr_offset {
                private_dict_deltas
                    .push_offset(Operator::Subrs, local_subr_offset - private_dict_offset);
            }
            PrivateDict::write_dep(ctxt, &font.private_dict, private_dict_deltas)?;
            let private_dict_len = i32::try_from(ctxt.bytes_written())? - private_dict_offset;

            // Build and write out new Font DICT
            let mut font_dict = FontDict::new();
            font_dict.inner_mut().push((
                Operator::Private,
                vec![
                    Operand::Offset(private_dict_len),
                    Operand::Offset(private_dict_offset),
                ],
            ));

            let mut font_dict_buffer = WriteBuffer::new();
            FontDict::write_dep(&mut font_dict_buffer, &font_dict, DictDelta::new())?;
            font_dicts.push(font_dict_buffer.into_inner());
        }

        // Font DICT INDEX
        top_dict.replace(
            Operator::FDArray,
            vec![Operand::Offset(i32::try_from(ctxt.bytes_written())?)],
        );
        let font_dict_index = owned::Index { data: font_dicts };
        owned::IndexU32::write(ctxt, &font_dict_index)?;

        // Variation store, if present
        if let Some(variation_store) = &cff2.vstore {
            top_dict.replace(
                Operator::VStore,
                vec![Operand::Offset(i32::try_from(ctxt.bytes_written())?)],
            );
            ItemVariationStore::write(ctxt, variation_store)?;
        }

        // Now that the offsets are known, write out the Top DICT
        ctxt.write_placeholder_dep(top_dict_placeholder, &top_dict, DictDelta::new())?;

        Ok(())
    }
}

pub(crate) fn write_stack(
    new_char_string: &mut WriteBuffer,
    stack: &ArgumentsStack<'_, StackValue>,
) -> Result<(), WriteError> {
    stack
        .all()
        .iter()
        .try_for_each(|value| write_stack_value(*value, new_char_string))
}

pub(crate) fn write_stack_value(
    value: StackValue,
    new_char_string: &mut WriteBuffer,
) -> Result<(), WriteError> {
    StackValue::write(new_char_string, value)
}

impl BlendOperand for StackValue {
    fn try_as_i32(self) -> Option<i32> {
        match self {
            StackValue::Int(int) => Some(i32::from(int)),
            StackValue::Fixed(fixed) => i32::try_num_from(f32::from(fixed)),
        }
    }

    fn try_as_u16(self) -> Option<u16> {
        match self {
            StackValue::Int(int) => u16::try_from(int).ok(),
            StackValue::Fixed(fixed) => u16::try_num_from(f32::from(fixed)),
        }
    }

    fn try_as_u8(self) -> Option<u8> {
        match self {
            StackValue::Int(int) => u8::try_from(int).ok(),
            StackValue::Fixed(fixed) => u8::try_num_from(f32::from(fixed)),
        }
    }
}

impl WriteBinary for StackValue {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, val: Self) -> Result<Self::Output, WriteError> {
        match val {
            // Refer to Table 3 Operand Encoding in section 4 of Technical Note #5176 for details on the
            // integer encoding scheme.
            StackValue::Int(int) => {
                match int {
                    // NOTE: Casts are safe due to patterns limiting range
                    -107..=107 => U8::write(ctxt, (int + 139) as u8),
                    108..=1131 => {
                        let int = int - 108;
                        U8::write(ctxt, ((int >> 8) + 247) as u8)?;
                        U8::write(ctxt, int as u8)
                    }
                    -1131..=-108 => {
                        let int = -int - 108;
                        U8::write(ctxt, ((int >> 8) + 251) as u8)?;
                        U8::write(ctxt, int as u8)
                    }
                    -32768..=32767 => {
                        U8::write(ctxt, operator::SHORT_INT)?;
                        I16Be::write(ctxt, int)
                    }
                }
            }
            StackValue::Fixed(fixed) => {
                U8::write(ctxt, operator::FIXED_16_16)?;
                Fixed::write(ctxt, fixed)
            }
        }
    }
}

impl From<StackValue> for f32 {
    fn from(value: StackValue) -> Self {
        match value {
            StackValue::Int(int) => f32::from(int),
            StackValue::Fixed(fixed) => f32::from(fixed),
        }
    }
}

impl From<f32> for StackValue {
    fn from(value: f32) -> Self {
        if value.fract() == 0.0 {
            StackValue::Int(value as i16)
        } else {
            StackValue::Fixed(Fixed::from(value))
        }
    }
}

impl From<i16> for StackValue {
    fn from(value: i16) -> Self {
        StackValue::Int(value)
    }
}

impl From<Fixed> for StackValue {
    fn from(value: Fixed) -> Self {
        StackValue::Fixed(value)
    }
}

/// Trait for values that can be used to implement the `blend` operator.
pub trait BlendOperand: Debug + Copy + Into<f32> + From<f32> + From<i16> + From<Fixed> {
    /// Try to convert `self` into an `i32`.
    fn try_as_i32(self) -> Option<i32>;

    /// Try to convert `self` into a `u16`.
    fn try_as_u16(self) -> Option<u16>;

    /// Try to convert `self` into a `u8`.
    fn try_as_u8(self) -> Option<u8>;
}

pub(super) fn scalars(
    vs_index: u16,
    vstore: &ItemVariationStore<'_>,
    instance: &OwnedTuple,
) -> Result<Vec<Option<f32>>, ParseError> {
    // Each region can now produce its scalar for the particular variation tuple
    vstore
        .regions(vs_index)?
        .map(|region| {
            let region = region?;
            Ok(region.scalar(instance.iter().copied()))
        })
        .collect::<Result<Vec<_>, ParseError>>()
}

pub(super) fn blend<T: BlendOperand>(
    scalars: &[Option<f32>],
    stack: &mut ArgumentsStack<'_, T>,
) -> Result<(), CFFError> {
    // > For k regions, produces n interpolated result value(s) from n*(k + 1) operands.
    //
    // > The last operand on the stack, n, specifies the number of operands that will be left on the
    // > stack for the next operator.
    //
    // > For example, if the blend operator is used in conjunction with the hflex operator, which
    // > requires 6 operands, then n would be set to 6. This operand also informs the handler for
    // > the blend operator that the operator is preceded by n+1 sets of operands. Clear all but n
    // > values from the stack, leaving the values for the subsequent operator corresponding to the
    // > default instance
    let k = scalars.len();
    if stack.len < 1 {
        return Err(CFFError::InvalidArgumentsStackLength);
    }
    let n = stack
        .pop()
        .try_as_u16()
        .map(usize::from)
        .ok_or(CFFError::InvalidOperand)?;

    let num_operands = n * (k + 1);
    if stack.len() < num_operands {
        return Err(CFFError::InvalidArgumentsStackLength);
    }

    // Process n*k operands applying the scalars
    let mut blended = [0.0; MAX_OPERANDS]; // 513 * 32-bit = 2KiB
    let blended = blended.get_mut(..n).ok_or(CFFError::InvalidOperand)?;
    let operands = stack.pop_n(num_operands);

    // for each set of deltas apply the scalar and calculate a new delta to
    // apply to the default values
    let (defaults, rest) = operands.split_at(n);
    for (adjustment, deltas) in blended.iter_mut().zip(rest.chunks(k)) {
        for (delta, scalar) in deltas.iter().copied().zip(scalars.iter()) {
            if let Some(scalar) = scalar {
                *adjustment += scalar * delta.into();
            }
        }
    }

    // apply the deltas to the default values
    defaults
        .iter()
        .copied()
        .zip(blended.iter_mut())
        .for_each(|(default, delta)| *delta += default.into());

    // push the blended values back onto the stack
    blended
        .iter_mut()
        .try_for_each(|value| stack.push(T::from(*value)))
}

impl Header {
    // Sum of size of the four fields in the header
    const SIZE: u8 = 1 + 1 + 1 + 2;
}

impl ReadBinary for Header {
    type HostType<'b> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let major = ctxt.read_u8()?;
        ctxt.check(major == 2)?;
        let minor = ctxt.read_u8()?;
        let header_size = ctxt.read_u8()?;
        let top_dict_length = ctxt.read_u16be()?;

        if header_size < Header::SIZE {
            return Err(ParseError::BadValue);
        }

        // Skip any unknown data
        let _unknown = ctxt.read_slice((header_size - Header::SIZE) as usize)?;

        Ok(Header {
            major,
            minor,
            header_size,
            top_dict_length,
        })
    }
}

impl WriteBinary for Header {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, header: Self) -> Result<Self::Output, WriteError> {
        U8::write(ctxt, header.major)?;
        U8::write(ctxt, header.minor)?;
        U8::write(ctxt, Header::SIZE)?;
        U16Be::write(ctxt, header.top_dict_length)?;
        Ok(())
    }
}

impl ReadBinary for IndexU32 {
    type HostType<'a> = Index<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let count = usize::safe_from(ctxt.read_u32be()?);
        super::read_index(ctxt, count)
    }
}

impl<'a> WriteBinary<&Index<'a>> for IndexU32 {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, index: &Index<'a>) -> Result<(), WriteError> {
        U32Be::write(ctxt, u16::try_from(index.count)?)?;
        super::write_index_body(ctxt, index)
    }
}

impl DictDefault for TopDictDefault {
    fn default(op: Operator) -> Option<&'static [Operand]> {
        match op {
            Operator::FontMatrix => Some(default_font_matrix().as_ref()),
            _ => None,
        }
    }
}

impl DictDefault for FontDictDefault {
    fn default(_op: Operator) -> Option<&'static [Operand]> {
        None
    }
}

impl DictDefault for PrivateDictDefault {
    fn default(op: Operator) -> Option<&'static [Operand]> {
        match op {
            Operator::BlueScale => Some(default_blue_scale().as_ref()),
            Operator::BlueShift => Some(&DEFAULT_BLUE_SHIFT),
            Operator::BlueFuzz => Some(&DEFAULT_BLUE_FUZZ),
            Operator::LanguageGroup => Some(&OPERAND_ZERO),
            Operator::ExpansionFactor => Some(default_expansion_factor().as_ref()),
            Operator::VSIndex => Some(&OPERAND_ZERO),
            _ => None,
        }
    }
}

/// Calculate the mode (most common value)
fn mode(widths: &[u16]) -> Option<u16> {
    if widths.is_empty() {
        return None;
    }

    let mut sorted_widths = widths.to_vec();
    sorted_widths.sort_unstable();
    let mut mode = (sorted_widths[0], 1);
    let last = sorted_widths
        .iter()
        .copied()
        .fold((sorted_widths[0], 0), |(prev, count), width| {
            if width == prev {
                (prev, count + 1)
            } else {
                if count > mode.1 {
                    mode = (prev, count)
                }
                (width, 1)
            }
        });
    if last.1 > mode.1 {
        mode = last
    }
    Some(mode.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::variable_fonts::avar::AvarTable;
    use crate::tables::variable_fonts::fvar::FvarTable;
    use crate::tables::{Fixed, FontTableProvider, OpenTypeData, OpenTypeFont};
    use crate::tag;
    use crate::tests::read_fixture;

    #[test]
    fn read_cff2() {
        let buffer = read_fixture("tests/fonts/opentype/cff2/SourceSansVariable-Roman.abc.otf");
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();

        let offset_table = match otf.data {
            OpenTypeData::Single(ttf) => ttf,
            OpenTypeData::Collection(_) => unreachable!(),
        };

        let cff_table_data = offset_table
            .read_table(&otf.scope, tag::CFF2)
            .unwrap()
            .unwrap();
        let cff = cff_table_data
            .read::<CFF2<'_>>()
            .expect("error parsing CFF2 table");
        assert_eq!(cff.header.major, 2);

        let vstore = cff.vstore.as_ref().unwrap();
        assert_eq!(vstore.variation_region_list.variation_regions.len(), 3);
        assert_eq!(vstore.item_variation_data.len(), 2);

        for i in 0..vstore.item_variation_data.len() as u16 {
            let regions = vstore.regions(i).unwrap();
            for region in regions {
                assert!(region.is_ok());
            }
        }
    }

    #[test]
    fn instance_char_strings() {
        let buffer = read_fixture("tests/fonts/opentype/cff2/SourceSansVariable-Roman.abc.otf");
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();

        let offset_table = match otf.data {
            OpenTypeData::Single(ttf) => ttf,
            OpenTypeData::Collection(_) => unreachable!(),
        };

        let cff2_table_data = offset_table
            .read_table(&otf.scope, tag::CFF2)
            .unwrap()
            .unwrap();
        let mut cff2 = cff2_table_data
            .read::<CFF2<'_>>()
            .expect("error parsing CFF2 table");
        let fvar_data = offset_table
            .read_table(&otf.scope, tag::FVAR)
            .unwrap()
            .unwrap();
        let fvar = fvar_data
            .read::<FvarTable<'_>>()
            .expect("unable to parse fvar");
        let avar_data = offset_table.read_table(&otf.scope, tag::FVAR).unwrap();
        let avar = avar_data
            .map(|avar_data| avar_data.read::<AvarTable<'_>>())
            .transpose()
            .expect("unable to parse avar table");
        let user_tuple = [Fixed::from(654.0)];
        let normalised_tuple = fvar
            .normalize(user_tuple.iter().copied(), avar.as_ref())
            .expect("unable to normalise user tuple");

        assert!(cff2.instance_char_strings(&normalised_tuple).is_ok());
    }

    #[test]
    fn subset_cff2_table() {
        let buffer = read_fixture("tests/fonts/opentype/cff2/SourceSans3.abc.otf");
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();
        let provider = otf.table_provider(0).expect("error reading font file");
        let cff2_data = provider
            .read_table_data(tag::CFF2)
            .expect("unable to read CFF2 data");
        let cff2 = ReadScope::new(&cff2_data)
            .read::<CFF2<'_>>()
            .expect("error parsing CFF2 table");
        let subset = cff2
            .subset_to_cff(&[0, 1], &provider, true, OutputFormat::Type1OrCid)
            .expect("unable to subset CFF2");

        // Write it out
        let mut buf = WriteBuffer::new();
        CFF::write(&mut buf, &subset.into()).unwrap();
        let subset_data = buf.into_inner();

        // Read it back
        let _subset_cff = ReadScope::new(&subset_data)
            .read::<CFF<'_>>()
            .expect("error parsing CFF2 table");
    }

    #[test]
    fn test_mode() {
        assert_eq!(mode(&[]), None);
        assert_eq!(mode(&[5]), Some(5));
        assert_eq!(mode(&[4, 1, 9, 1, 9, 4, 5, 6, 8, 3, 4]), Some(4));
        assert_eq!(mode(&[4, 1, 9, 1, 9, 4, 5, 6, 8, 3, 7]), Some(1));
        assert_eq!(mode(&[4, 1, 9, 1, 9, 4, 5, 6, 8, 3, 9]), Some(9));
    }
}
