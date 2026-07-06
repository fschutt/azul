//! Glyph substitution (`gsub`) implementation.
//!
//! > The Glyph Substitution (GSUB) table provides data for substition of glyphs for appropriate
//! > rendering of scripts, such as cursively-connecting forms in Arabic script, or for advanced
//! > typographic effects, such as ligatures.
//!
//! — <https://docs.microsoft.com/en-us/typography/opentype/spec/gsub>

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Debug;

use enumflags2::BitFlags;
use tinyvec::{tiny_vec, TinyVec};

use crate::context::{ContextLookupHelper, Glyph, GlyphTable, MatchType};
use crate::error::{ParseError, ShapingError};
use crate::layout::{
    chain_context_lookup_info, context_lookup_info, AlternateSet, AlternateSubst,
    ChainContextLookup, ContextLookup, FeatureTableSubstitution, GDEFTable, LangSys, LayoutCache,
    LayoutTable, Ligature, LigatureSubst, LookupCacheItem, LookupList, MultipleSubst,
    ReverseChainSingleSubst, SequenceTable, SingleSubst, SubstLookup, GSUB,
};
use crate::scripts::{self, ScriptType};
use crate::tables::variable_fonts::Tuple;
use crate::unicode::VariationSelector;
use crate::{tag, GlyphId};

const SUBST_RECURSION_LIMIT: usize = 2;
// Matches Harfbuzz:
// https://github.com/harfbuzz/harfbuzz/blob/8062c372590980d36d5b4cc720d33dca2662c56e/src/hb-limits.hh#L32
pub(crate) const MAX_GLYPHS_FACTOR: usize = 256;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeatureInfo {
    pub feature_tag: u32,
    pub alternate: Option<usize>,
}

type SubstContext<'a> = ContextLookupHelper<'a, GSUB>;

impl Ligature {
    pub fn matches<T>(
        &self,
        match_type: MatchType,
        opt_gdef_table: Option<&GDEFTable>,
        i: usize,
        glyphs: &[RawGlyph<T>],
    ) -> bool {
        let mut last_index = 0;
        match_type
            .match_front(
                opt_gdef_table,
                &GlyphTable::ById(&self.component_glyphs),
                glyphs,
                i,
                &mut last_index,
            )
            .is_some()
    }

    pub fn apply<T: GlyphData>(
        &self,
        match_type: MatchType,
        opt_gdef_table: Option<&GDEFTable>,
        i: usize,
        glyphs: &mut Vec<RawGlyph<T>>,
    ) -> usize {
        let mut index = i + 1;
        let mut matched = 0;
        let mut skip = 0;
        while matched < self.component_glyphs.len() {
            if index < glyphs.len() {
                if match_type.match_glyph(opt_gdef_table, &glyphs[index]) {
                    matched += 1;
                    let mut matched_glyph = glyphs.remove(index);
                    glyphs[i].unicodes.append(&mut matched_glyph.unicodes);
                    glyphs[i].extra_data =
                        GlyphData::merge(glyphs[i].extra_data.clone(), matched_glyph.extra_data);
                    glyphs[i].flags.set(RawGlyphFlag::LIGATURE, true);
                } else {
                    glyphs[index].liga_component_pos = matched as u16;
                    skip += 1;
                    index += 1;
                }
            } else {
                panic!("ran out of glyphs");
            }
        }
        while index < glyphs.len()
            && MatchType::marks_only().match_glyph(opt_gdef_table, &glyphs[index])
        {
            glyphs[index].liga_component_pos = matched as u16;
            index += 1;
        }
        glyphs[i].glyph_index = self.ligature_glyph;
        glyphs[i].glyph_origin = GlyphOrigin::Direct;
        skip
    }
}

#[derive(Clone, Debug)]
pub struct RawGlyph<T> {
    pub unicodes: TinyVec<[char; 1]>,
    pub glyph_index: GlyphId,
    pub liga_component_pos: u16,
    pub glyph_origin: GlyphOrigin,
    pub flags: RawGlyphFlags,
    pub variation: Option<VariationSelector>,
    pub extra_data: T,
}

#[enumflags2::bitflags]
#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(non_camel_case_types)]
pub enum RawGlyphFlag {
    SMALL_CAPS = 1 << 0,
    MULTI_SUBST_DUP = 1 << 1,
    IS_VERT_ALT = 1 << 2,
    LIGATURE = 1 << 3,
    FAKE_BOLD = 1 << 4,
    FAKE_ITALIC = 1 << 5,
}

pub type RawGlyphFlags = BitFlags<RawGlyphFlag>;

/// `merge` is called during ligature substitution (i.e. merging of glyphs),
/// and determines how the `RawGlyph.extra_data` field should be merged
pub trait GlyphData: Clone {
    fn merge(data1: Self, data2: Self) -> Self;
}

impl GlyphData for () {
    fn merge(_data1: (), _data2: ()) {}
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GlyphOrigin {
    Char(char),
    Direct,
}

impl<T> RawGlyph<T> {
    pub fn small_caps(&self) -> bool {
        self.flags.contains(RawGlyphFlag::SMALL_CAPS)
    }

    pub fn multi_subst_dup(&self) -> bool {
        self.flags.contains(RawGlyphFlag::MULTI_SUBST_DUP)
    }

    pub fn is_vert_alt(&self) -> bool {
        self.flags.contains(RawGlyphFlag::IS_VERT_ALT)
    }

    pub fn ligature(&self) -> bool {
        self.flags.contains(RawGlyphFlag::LIGATURE)
    }

    pub fn fake_bold(&self) -> bool {
        self.flags.contains(RawGlyphFlag::FAKE_BOLD)
    }

    pub fn fake_italic(&self) -> bool {
        self.flags.contains(RawGlyphFlag::FAKE_ITALIC)
    }
}

impl<T> Glyph for RawGlyph<T> {
    fn get_glyph_index(&self) -> u16 {
        self.glyph_index
    }
}

pub fn gsub_feature_would_apply<T: GlyphData>(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    langsys: &LangSys,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    feature_tag: u32,
    glyphs: &[RawGlyph<T>],
    i: usize,
) -> Result<bool, ParseError> {
    if let Some(feature_table) =
        gsub_table.find_langsys_feature(langsys, feature_tag, feature_variations)?
    {
        if let Some(ref lookup_list) = gsub_table.opt_lookup_list {
            for lookup_index in &feature_table.lookup_indices {
                let lookup_index = usize::from(*lookup_index);
                let lookup_cache_item = lookup_list.lookup_cache_gsub(gsub_cache, lookup_index)?;
                if gsub_lookup_would_apply(opt_gdef_table, &lookup_cache_item, glyphs, i)? {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

pub fn gsub_lookup_would_apply<T: GlyphData>(
    opt_gdef_table: Option<&GDEFTable>,
    lookup: &LookupCacheItem<SubstLookup>,
    glyphs: &[RawGlyph<T>],
    i: usize,
) -> Result<bool, ParseError> {
    let match_type = MatchType::from_lookup_flag(lookup.lookup_flag, lookup.mark_filtering_set);
    if i < glyphs.len() && match_type.match_glyph(opt_gdef_table, &glyphs[i]) {
        return match lookup.lookup_subtables {
            SubstLookup::SingleSubst(ref subtables) => {
                match singlesubst_would_apply(subtables, &glyphs[i])? {
                    Some(_output_glyph) => Ok(true),
                    None => Ok(false),
                }
            }
            SubstLookup::MultipleSubst(ref subtables) => {
                match multiplesubst_would_apply(subtables, i, glyphs)? {
                    Some(_sequence_table) => Ok(true),
                    None => Ok(false),
                }
            }
            SubstLookup::AlternateSubst(ref subtables) => {
                match alternatesubst_would_apply(subtables, &glyphs[i])? {
                    Some(_alternate_set) => Ok(true),
                    None => Ok(false),
                }
            }
            SubstLookup::LigatureSubst(ref subtables) => {
                match ligaturesubst_would_apply(opt_gdef_table, subtables, match_type, i, glyphs)? {
                    Some(_ligature) => Ok(true),
                    None => Ok(false),
                }
            }
            SubstLookup::ContextSubst(ref subtables) => {
                match contextsubst_would_apply(opt_gdef_table, subtables, match_type, i, glyphs)? {
                    Some(_subst) => Ok(true),
                    None => Ok(false),
                }
            }
            SubstLookup::ChainContextSubst(ref subtables) => {
                match chaincontextsubst_would_apply(
                    opt_gdef_table,
                    subtables,
                    match_type,
                    i,
                    glyphs,
                )? {
                    Some(_subst) => Ok(true),
                    None => Ok(false),
                }
            }
            SubstLookup::ReverseChainSingleSubst(ref subtables) => {
                match reversechainsinglesubst_would_apply(
                    opt_gdef_table,
                    subtables,
                    match_type,
                    i,
                    glyphs,
                )? {
                    Some(_subst) => Ok(true),
                    None => Ok(false),
                }
            }
        };
    }
    Ok(false)
}

/// Apply the specified lookup to the given glyphs.
///
/// ## Arguments
///
/// * `gsub_cache` - The GSUB layout cache, created via [new_layout_cache][crate::layout::new_layout_cache].
/// * `gsub_table` - The GSUB layout table.
/// * `opt_gdef_table` - The GDEF table, if available.
/// * `lookup_index` - The index of the lookup to apply.
/// * `feature_tag` - The feature tag associated with the lookup.
/// * `opt_alternate` - The index of an alternate glyph in the alternate set, if available.
/// * `glyphs` - The glyphs to apply the lookup to.
/// * `max_glyphs` - The limit to which `glyphs` can grow through substitutions.
///   The length of `glyphs` will remain less than this value. If the limit is reached,
///   further substitutions will not be applied.
/// * `start` - The starting index of the glyphs to apply the lookup to.
/// * `length` - The length of the input sequence substituted.
/// * `pred` - The predicate function to filter the glyphs to apply the lookup to.
pub fn gsub_apply_lookup<T: GlyphData>(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    lookup_index: usize,
    feature_tag: u32,
    opt_alternate: Option<usize>,
    glyphs: &mut Vec<RawGlyph<T>>,
    max_glyphs: usize,
    start: usize,
    mut length: usize,
    pred: impl Fn(&RawGlyph<T>) -> bool,
) -> Result<usize, ParseError> {
    if let Some(ref lookup_list) = gsub_table.opt_lookup_list {
        let lookup = lookup_list.lookup_cache_gsub(gsub_cache, lookup_index)?;
        let match_type = MatchType::from_lookup_flag(lookup.lookup_flag, lookup.mark_filtering_set);
        match lookup.lookup_subtables {
            SubstLookup::SingleSubst(ref subtables) => {
                for glyph in glyphs[start..(start + length)].iter_mut() {
                    if match_type.match_glyph(opt_gdef_table, glyph) && pred(glyph) {
                        singlesubst(subtables, feature_tag, glyph)?;
                    }
                }
            }
            SubstLookup::MultipleSubst(ref subtables) => {
                let mut i = start;
                while i < start + length {
                    if match_type.match_glyph(opt_gdef_table, &glyphs[i]) && pred(&glyphs[i]) {
                        match multiplesubst(subtables, i, glyphs, max_glyphs)? {
                            Some(replace_count) => {
                                i += replace_count;
                                length += replace_count;
                                length -= 1;
                            }
                            None => i += 1,
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            SubstLookup::AlternateSubst(ref subtables) => {
                for glyph in glyphs[start..(start + length)].iter_mut() {
                    if match_type.match_glyph(opt_gdef_table, glyph) && pred(glyph) {
                        let alternate = opt_alternate.unwrap_or(0);
                        alternatesubst(subtables, alternate, glyph)?;
                    }
                }
            }
            SubstLookup::LigatureSubst(ref subtables) => {
                let mut i = start;
                while i < start + length {
                    if match_type.match_glyph(opt_gdef_table, &glyphs[i]) && pred(&glyphs[i]) {
                        match ligaturesubst(opt_gdef_table, subtables, match_type, i, glyphs)? {
                            Some((removed_count, skip_count)) => {
                                i += skip_count + 1;
                                length -= removed_count;
                            }
                            None => i += 1,
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            SubstLookup::ContextSubst(ref subtables) => {
                let mut i = start;
                while i < start + length {
                    if match_type.match_glyph(opt_gdef_table, &glyphs[i]) && pred(&glyphs[i]) {
                        match contextsubst(
                            SUBST_RECURSION_LIMIT,
                            gsub_cache,
                            lookup_list,
                            opt_gdef_table,
                            subtables,
                            feature_tag,
                            match_type,
                            i,
                            glyphs,
                            max_glyphs,
                        )? {
                            Some((input_length, changes)) => {
                                i += input_length;
                                length = checked_add(length, changes).unwrap();
                            }
                            None => i += 1,
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            SubstLookup::ChainContextSubst(ref subtables) => {
                let mut i = start;
                while i < start + length {
                    if match_type.match_glyph(opt_gdef_table, &glyphs[i]) && pred(&glyphs[i]) {
                        match chaincontextsubst(
                            SUBST_RECURSION_LIMIT,
                            gsub_cache,
                            lookup_list,
                            opt_gdef_table,
                            subtables,
                            feature_tag,
                            match_type,
                            i,
                            glyphs,
                            max_glyphs,
                        )? {
                            Some((input_length, changes)) => {
                                i += input_length;
                                length = checked_add(length, changes).unwrap();
                            }
                            None => i += 1,
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            SubstLookup::ReverseChainSingleSubst(ref subtables) => {
                for i in (start..start + length).rev() {
                    if match_type.match_glyph(opt_gdef_table, &glyphs[i]) && pred(&glyphs[i]) {
                        reversechainsinglesubst(opt_gdef_table, subtables, match_type, i, glyphs)?;
                    }
                }
            }
        }
    }
    Ok(length)
}

fn singlesubst_would_apply<T: GlyphData>(
    subtables: &[SingleSubst],
    glyph: &RawGlyph<T>,
) -> Result<Option<u16>, ParseError> {
    let glyph_index = glyph.glyph_index;
    for single_subst in subtables {
        if let Some(glyph_index) = single_subst.apply_glyph(glyph_index)? {
            return Ok(Some(glyph_index));
        }
    }
    Ok(None)
}

fn singlesubst<T: GlyphData>(
    subtables: &[SingleSubst],
    subst_tag: u32,
    glyph: &mut RawGlyph<T>,
) -> Result<(), ParseError> {
    if let Some(output_glyph) = singlesubst_would_apply(subtables, glyph)? {
        glyph.glyph_index = output_glyph;
        glyph.glyph_origin = GlyphOrigin::Direct;
        if subst_tag == tag::VERT || subst_tag == tag::VRT2 {
            glyph.flags.set(RawGlyphFlag::IS_VERT_ALT, true);
        }
    }
    Ok(())
}

fn multiplesubst_would_apply<'a, T: GlyphData>(
    subtables: &'a [MultipleSubst],
    i: usize,
    glyphs: &[RawGlyph<T>],
) -> Result<Option<&'a SequenceTable>, ParseError> {
    let glyph_index = glyphs[i].glyph_index;
    for multiple_subst in subtables {
        if let Some(sequence_table) = multiple_subst.apply_glyph(glyph_index)? {
            return Ok(Some(sequence_table));
        }
    }
    Ok(None)
}

fn multiplesubst<T: GlyphData>(
    subtables: &[MultipleSubst],
    i: usize,
    glyphs: &mut Vec<RawGlyph<T>>,
    max_glyphs: usize,
) -> Result<Option<usize>, ParseError> {
    match multiplesubst_would_apply(subtables, i, glyphs)? {
        Some(sequence_table) => {
            if sequence_table.substitute_glyphs.len() + glyphs.len() >= max_glyphs {
                // The Unicode text rendering tests say that shaping should stop when the limit is
                // reached, but not fail:
                //
                // "If your implementation is immune to this attack, it should neither crash nor
                // hang when rendering lol with this font. Instead, your implementation should stop
                // executing once its internal buffer has reached a size limit."
                return Ok(Some(0));
            }

            if !sequence_table.substitute_glyphs.is_empty() {
                let first_glyph_index = sequence_table.substitute_glyphs[0];
                glyphs[i].glyph_index = first_glyph_index;
                glyphs[i].glyph_origin = GlyphOrigin::Direct;
                for j in 1..sequence_table.substitute_glyphs.len() {
                    let output_glyph_index = sequence_table.substitute_glyphs[j];
                    let mut flags = glyphs[i].flags;
                    flags.set(RawGlyphFlag::MULTI_SUBST_DUP, true);
                    flags.set(RawGlyphFlag::LIGATURE, false);
                    let glyph = RawGlyph {
                        unicodes: glyphs[i].unicodes.clone(),
                        glyph_index: output_glyph_index,
                        liga_component_pos: 0, //glyphs[i].liga_component_pos,
                        glyph_origin: GlyphOrigin::Direct,
                        flags,
                        extra_data: glyphs[i].extra_data.clone(),
                        variation: glyphs[i].variation,
                    };
                    glyphs.insert(i + j, glyph);
                }
                Ok(Some(sequence_table.substitute_glyphs.len()))
            } else {
                // the spec forbids this, but implementations all allow it
                glyphs.remove(i);
                Ok(Some(0))
            }
        }
        None => Ok(None),
    }
}

fn alternatesubst_would_apply<'a, T: GlyphData>(
    subtables: &'a [AlternateSubst],
    glyph: &RawGlyph<T>,
) -> Result<Option<&'a AlternateSet>, ParseError> {
    let glyph_index = glyph.glyph_index;
    for alternate_subst in subtables {
        if let Some(alternate_set) = alternate_subst.apply_glyph(glyph_index)? {
            return Ok(Some(alternate_set));
        }
    }
    Ok(None)
}

fn alternatesubst<T: GlyphData>(
    subtables: &[AlternateSubst],
    alternate: usize,
    glyph: &mut RawGlyph<T>,
) -> Result<(), ParseError> {
    if let Some(alternateset) = alternatesubst_would_apply(subtables, glyph)? {
        // TODO allow users to specify which alternate glyph they want
        if alternate < alternateset.alternate_glyphs.len() {
            glyph.glyph_index = alternateset.alternate_glyphs[alternate];
            glyph.glyph_origin = GlyphOrigin::Direct;
        }
    }
    Ok(())
}

fn ligaturesubst_would_apply<'a, T: GlyphData>(
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &'a [LigatureSubst],
    match_type: MatchType,
    i: usize,
    glyphs: &[RawGlyph<T>],
) -> Result<Option<&'a Ligature>, ParseError> {
    let glyph_index = glyphs[i].glyph_index;
    for ligature_subst in subtables {
        if let Some(ligatureset) = ligature_subst.apply_glyph(glyph_index)? {
            for ligature in &ligatureset.ligatures {
                if ligature.matches(match_type, opt_gdef_table, i, glyphs) {
                    return Ok(Some(ligature));
                }
            }
        }
    }
    Ok(None)
}

fn ligaturesubst<T: GlyphData>(
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &[LigatureSubst],
    match_type: MatchType,
    i: usize,
    glyphs: &mut Vec<RawGlyph<T>>,
) -> Result<Option<(usize, usize)>, ParseError> {
    match ligaturesubst_would_apply(opt_gdef_table, subtables, match_type, i, glyphs)? {
        Some(ligature) => Ok(Some((
            ligature.component_glyphs.len(),
            ligature.apply(match_type, opt_gdef_table, i, glyphs),
        ))),
        None => Ok(None),
    }
}

fn contextsubst_would_apply<'a, T: GlyphData>(
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &'a [ContextLookup<GSUB>],
    match_type: MatchType,
    i: usize,
    glyphs: &[RawGlyph<T>],
) -> Result<Option<Box<SubstContext<'a>>>, ParseError> {
    let glyph_index = glyphs[i].glyph_index;
    for context_lookup in subtables {
        if let Some(context) = context_lookup_info(context_lookup, glyph_index, |context| {
            context.matches(opt_gdef_table, match_type, glyphs, i)
        })? {
            return Ok(Some(context));
        }
    }
    Ok(None)
}

fn contextsubst<T: GlyphData>(
    recursion_limit: usize,
    gsub_cache: &LayoutCache<GSUB>,
    lookup_list: &LookupList<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &[ContextLookup<GSUB>],
    feature_tag: u32,
    match_type: MatchType,
    i: usize,
    glyphs: &mut Vec<RawGlyph<T>>,
    max_glyphs: usize,
) -> Result<Option<(usize, isize)>, ParseError> {
    match contextsubst_would_apply(opt_gdef_table, subtables, match_type, i, glyphs)? {
        Some(subst) => apply_subst_context(
            recursion_limit,
            gsub_cache,
            lookup_list,
            opt_gdef_table,
            feature_tag,
            match_type,
            &subst,
            i,
            glyphs,
            max_glyphs,
        ),
        None => Ok(None),
    }
}

fn chaincontextsubst_would_apply<'a, T: GlyphData>(
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &'a [ChainContextLookup<GSUB>],
    match_type: MatchType,
    i: usize,
    glyphs: &[RawGlyph<T>],
) -> Result<Option<Box<SubstContext<'a>>>, ParseError> {
    let glyph_index = glyphs[i].glyph_index;
    for chain_context_lookup in subtables {
        if let Some(context) =
            chain_context_lookup_info(chain_context_lookup, glyph_index, |context| {
                context.matches(opt_gdef_table, match_type, glyphs, i)
            })?
        {
            return Ok(Some(context));
        }
    }
    Ok(None)
}

fn chaincontextsubst<T: GlyphData>(
    recursion_limit: usize,
    gsub_cache: &LayoutCache<GSUB>,
    lookup_list: &LookupList<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &[ChainContextLookup<GSUB>],
    feature_tag: u32,
    match_type: MatchType,
    i: usize,
    glyphs: &mut Vec<RawGlyph<T>>,
    max_glyphs: usize,
) -> Result<Option<(usize, isize)>, ParseError> {
    match chaincontextsubst_would_apply(opt_gdef_table, subtables, match_type, i, glyphs)? {
        Some(subst) => apply_subst_context(
            recursion_limit,
            gsub_cache,
            lookup_list,
            opt_gdef_table,
            feature_tag,
            match_type,
            &subst,
            i,
            glyphs,
            max_glyphs,
        ),
        None => Ok(None),
    }
}

fn reversechainsinglesubst_would_apply<T: GlyphData>(
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &[ReverseChainSingleSubst],
    match_type: MatchType,
    i: usize,
    glyphs: &[RawGlyph<T>],
) -> Result<Option<u16>, ParseError> {
    let glyph_index = glyphs[i].glyph_index;
    for reversechainsinglesubst in subtables {
        if let new_glyph_index @ Some(_) =
            reversechainsinglesubst.apply_glyph(glyph_index, |context| {
                context
                    .matches(opt_gdef_table, match_type, glyphs, i)
                    .is_some()
            })?
        {
            return Ok(new_glyph_index);
        }
    }
    Ok(None)
}

fn reversechainsinglesubst<T: GlyphData>(
    opt_gdef_table: Option<&GDEFTable>,
    subtables: &[ReverseChainSingleSubst],
    match_type: MatchType,
    i: usize,
    glyphs: &mut [RawGlyph<T>],
) -> Result<(), ParseError> {
    if let Some(output_glyph_index) =
        reversechainsinglesubst_would_apply(opt_gdef_table, subtables, match_type, i, glyphs)?
    {
        glyphs[i].glyph_index = output_glyph_index;
        glyphs[i].glyph_origin = GlyphOrigin::Direct;
    }
    Ok(())
}

fn apply_subst_context<T: GlyphData>(
    recursion_limit: usize,
    gsub_cache: &LayoutCache<GSUB>,
    lookup_list: &LookupList<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    feature_tag: u32,
    match_type: MatchType,
    subst: &SubstContext<'_>,
    i: usize,
    glyphs: &mut Vec<RawGlyph<T>>,
    max_glyphs: usize,
) -> Result<Option<(usize, isize)>, ParseError> {
    let mut changes = 0;
    let len = match match_type.find_nth(
        opt_gdef_table,
        glyphs,
        i,
        subst.match_context.input_table.len(),
    ) {
        Some(last) => last - i + 1,
        None => return Ok(None), // FIXME actually an error/impossible?
    };
    for (subst_index, subst_lookup_index) in subst.lookup_array {
        if let Some(changes0) = apply_subst(
            recursion_limit,
            gsub_cache,
            lookup_list,
            opt_gdef_table,
            match_type,
            usize::from(*subst_index),
            usize::from(*subst_lookup_index),
            feature_tag,
            glyphs,
            max_glyphs,
            i,
        )? {
            changes += changes0
        }
    }
    match checked_add(len, changes) {
        Some(new_len) => Ok(Some((new_len, changes))),
        None => panic!("apply_subst_context: len < 0"),
    }
}

fn checked_add(base: usize, changes: isize) -> Option<usize> {
    if changes < 0 {
        base.checked_sub(changes.wrapping_abs() as usize)
    } else {
        base.checked_add(changes as usize)
    }
}

fn apply_subst<T: GlyphData>(
    recursion_limit: usize,
    gsub_cache: &LayoutCache<GSUB>,
    lookup_list: &LookupList<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    parent_match_type: MatchType,
    subst_index: usize,
    lookup_index: usize,
    feature_tag: u32,
    glyphs: &mut Vec<RawGlyph<T>>,
    max_glyphs: usize,
    index: usize,
) -> Result<Option<isize>, ParseError> {
    let lookup = lookup_list.lookup_cache_gsub(gsub_cache, lookup_index)?;
    let match_type = MatchType::from_lookup_flag(lookup.lookup_flag, lookup.mark_filtering_set);
    let i = match parent_match_type.find_nth(opt_gdef_table, glyphs, index, subst_index) {
        Some(index1) => index1,
        None => return Ok(None), // FIXME error?
    };
    match lookup.lookup_subtables {
        SubstLookup::SingleSubst(ref subtables) => {
            singlesubst(subtables, feature_tag, &mut glyphs[i])?;
            Ok(Some(0))
        }
        SubstLookup::MultipleSubst(ref subtables) => {
            match multiplesubst(subtables, i, glyphs, max_glyphs)? {
                Some(replace_count) => Ok(Some((replace_count as isize) - 1)),
                None => Ok(None),
            }
        }
        SubstLookup::AlternateSubst(ref subtables) => {
            alternatesubst(subtables, 0, &mut glyphs[i])?;
            Ok(Some(0))
        }
        SubstLookup::LigatureSubst(ref subtables) => {
            match ligaturesubst(opt_gdef_table, subtables, match_type, i, glyphs)? {
                Some((removed_count, _skip_count)) => Ok(Some(-(removed_count as isize))),
                None => Ok(None), // FIXME error?
            }
        }
        SubstLookup::ContextSubst(ref subtables) => {
            if recursion_limit > 0 {
                match contextsubst(
                    recursion_limit - 1,
                    gsub_cache,
                    lookup_list,
                    opt_gdef_table,
                    subtables,
                    feature_tag,
                    match_type,
                    i,
                    glyphs,
                    max_glyphs,
                )? {
                    Some((_length, change)) => Ok(Some(change)),
                    None => Ok(None),
                }
            } else {
                Err(ParseError::LimitExceeded)
            }
        }
        SubstLookup::ChainContextSubst(ref subtables) => {
            if recursion_limit > 0 {
                match chaincontextsubst(
                    recursion_limit - 1,
                    gsub_cache,
                    lookup_list,
                    opt_gdef_table,
                    subtables,
                    feature_tag,
                    match_type,
                    i,
                    glyphs,
                    max_glyphs,
                )? {
                    Some((_length, change)) => Ok(Some(change)),
                    None => Ok(None),
                }
            } else {
                Err(ParseError::LimitExceeded)
            }
        }
        SubstLookup::ReverseChainSingleSubst(ref subtables) => {
            reversechainsinglesubst(opt_gdef_table, subtables, match_type, i, glyphs)?;
            Ok(Some(0))
        }
    }
}

fn build_lookups_custom(
    gsub_table: &LayoutTable<GSUB>,
    langsys: &LangSys,
    feature_tags: &[FeatureInfo],
    feature_mask: FeatureMask,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
) -> Result<BTreeMap<usize, u32>, ParseError> {
    let mut lookups = BTreeMap::new();
    for feature_info in feature_tags {
        // Skip features already applied via the feature mask to avoid
        // applying them twice (e.g. font-feature-settings: "ccmp" 1).
        if let Some(feature) = Feature::from_tag(feature_info.feature_tag) {
            if feature_mask.contains(feature) {
                continue;
            }
        }
        if let Some(feature_table) = gsub_table.find_langsys_feature(
            langsys,
            feature_info.feature_tag,
            feature_variations,
        )? {
            lookups.extend(
                feature_table
                    .lookup_indices
                    .iter()
                    .map(|&lookup_index| (usize::from(lookup_index), feature_info.feature_tag)),
            );
        }
    }
    Ok(lookups)
}

fn build_lookups_default(
    gsub_table: &LayoutTable<GSUB>,
    langsys: &LangSys,
    feature_masks: FeatureMask,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
) -> Result<Vec<(usize, u32)>, ParseError> {
    let mut lookups = BTreeMap::new();
    for feature in feature_masks {
        let feature_tag = feature.tag();
        if let Some(feature_table) =
            gsub_table.find_langsys_feature(langsys, feature_tag, feature_variations)?
        {
            for lookup_index in &feature_table.lookup_indices {
                lookups.insert(usize::from(*lookup_index), feature_tag);
            }
        } else if feature == Feature::VRT2_OR_VERT {
            let vert_tag = tag::VERT;
            if let Some(feature_table) =
                gsub_table.find_langsys_feature(langsys, vert_tag, feature_variations)?
            {
                for lookup_index in &feature_table.lookup_indices {
                    lookups.insert(usize::from(*lookup_index), vert_tag);
                }
            }
        }
    }

    // note: iter() returns sorted by key
    Ok(lookups.into_iter().collect())
}

fn make_supported_features_mask(
    gsub_table: &LayoutTable<GSUB>,
    langsys: &LangSys,
) -> Result<FeatureMask, ParseError> {
    let mut feature_mask = FeatureMask::empty();
    for feature_index in langsys.feature_indices_iter() {
        let feature_record = gsub_table.feature_by_index(*feature_index)?;
        feature_mask |= FeatureMask::from_tag(feature_record.feature_tag);
    }
    Ok(feature_mask)
}

fn lang_tag_key(opt_lang_tag: Option<u32>) -> u32 {
    // `DFLT` is not a valid lang tag so we use it to indicate the default
    opt_lang_tag.unwrap_or(tag::DFLT)
}

fn get_supported_features(
    gsub_cache: &LayoutCache<GSUB>,
    script_tag: u32,
    opt_lang_tag: Option<u32>,
) -> Result<FeatureMask, ParseError> {
    let feature_mask = match gsub_cache
        .supported_features
        .lock()
        .unwrap()
        .entry((script_tag, lang_tag_key(opt_lang_tag)))
    {
        Entry::Occupied(entry) => BitFlags::from_bits_truncate(*entry.get()),
        Entry::Vacant(entry) => {
            let gsub_table = &gsub_cache.layout_table;
            let feature_mask =
                if let Some(script) = gsub_table.find_script_or_default(script_tag)? {
                    if let Some(langsys) = script.find_langsys_or_default(opt_lang_tag)? {
                        make_supported_features_mask(gsub_table, langsys)?
                    } else {
                        FeatureMask::empty()
                    }
                } else {
                    FeatureMask::empty()
                };
            entry.insert(feature_mask.bits());
            feature_mask
        }
    };
    Ok(feature_mask)
}

fn find_alternate(features_list: &[FeatureInfo], feature_tag: u32) -> Option<usize> {
    for feature_info in features_list {
        if feature_info.feature_tag == feature_tag {
            return feature_info.alternate;
        }
    }
    None
}

pub fn replace_missing_glyphs<T: GlyphData>(glyphs: &mut [RawGlyph<T>], num_glyphs: u16) {
    for glyph in glyphs.iter_mut() {
        if glyph.glyph_index >= num_glyphs {
            glyph.unicodes = tiny_vec![];
            glyph.glyph_index = 0;
            glyph.liga_component_pos = 0;
            glyph.glyph_origin = GlyphOrigin::Direct;
            glyph.flags = RawGlyphFlags::empty();
            glyph.variation = None;
        }
    }
}

fn strip_joiners<T: GlyphData>(glyphs: &mut Vec<RawGlyph<T>>) {
    glyphs.retain(|g| match g.glyph_origin {
        GlyphOrigin::Char('\u{200C}') => false,
        GlyphOrigin::Char('\u{200D}') => false,
        _ => true,
    })
}

#[enumflags2::bitflags]
#[repr(u64)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[allow(non_camel_case_types)]
pub enum Feature {
    ABVF,
    ABVS,
    AFRC,
    AKHN,
    BLWF,
    BLWS,
    C2SC,
    CALT,
    CASE,
    CCMP,
    CFAR,
    CJCT,
    CLIG,
    CPSP,
    CSWH,
    DLIG,
    FINA,
    FIN2,
    FIN3,
    FRAC,
    HALF,
    HALN,
    HIST,
    HLIG,
    INIT,
    ISOL,
    LIGA,
    LNUM,
    LOCL,
    MEDI,
    MED2,
    MSET,
    NUKT,
    ONUM,
    ORDN,
    PNUM,
    PREF,
    PRES,
    PSTF,
    PSTS,
    RCLT,
    RKRF,
    RLIG,
    RPHF,
    SMCP,
    TNUM,
    VATU,
    VRT2_OR_VERT,
    ZERO,
    RVRN,
}

pub type FeatureMask = BitFlags<Feature>;

impl Feature {
    /// Convert a single feature into a `FeatureMask` containing just that feature.
    pub fn mask(self) -> FeatureMask {
        FeatureMask::from(self)
    }

    /// Return the OpenType GSUB feature tag corresponding to this feature.
    pub fn tag(self) -> u32 {
        match self {
            Feature::ABVF => tag::ABVF,
            Feature::ABVS => tag::ABVS,
            Feature::AFRC => tag::AFRC,
            Feature::AKHN => tag::AKHN,
            Feature::BLWF => tag::BLWF,
            Feature::BLWS => tag::BLWS,
            Feature::C2SC => tag::C2SC,
            Feature::CALT => tag::CALT,
            Feature::CASE => tag::CASE,
            Feature::CCMP => tag::CCMP,
            Feature::CFAR => tag::CFAR,
            Feature::CJCT => tag::CJCT,
            Feature::CLIG => tag::CLIG,
            Feature::CPSP => tag::CPSP,
            Feature::CSWH => tag::CSWH,
            Feature::DLIG => tag::DLIG,
            Feature::FINA => tag::FINA,
            Feature::FIN2 => tag::FIN2,
            Feature::FIN3 => tag::FIN3,
            Feature::FRAC => tag::FRAC,
            Feature::HALF => tag::HALF,
            Feature::HALN => tag::HALN,
            Feature::HIST => tag::HIST,
            Feature::HLIG => tag::HLIG,
            Feature::INIT => tag::INIT,
            Feature::ISOL => tag::ISOL,
            Feature::LIGA => tag::LIGA,
            Feature::LNUM => tag::LNUM,
            Feature::LOCL => tag::LOCL,
            Feature::MEDI => tag::MEDI,
            Feature::MED2 => tag::MED2,
            Feature::MSET => tag::MSET,
            Feature::NUKT => tag::NUKT,
            Feature::ONUM => tag::ONUM,
            Feature::ORDN => tag::ORDN,
            Feature::PNUM => tag::PNUM,
            Feature::PREF => tag::PREF,
            Feature::PRES => tag::PRES,
            Feature::PSTF => tag::PSTF,
            Feature::PSTS => tag::PSTS,
            Feature::RCLT => tag::RCLT,
            Feature::RKRF => tag::RKRF,
            Feature::RLIG => tag::RLIG,
            Feature::RPHF => tag::RPHF,
            Feature::RVRN => tag::RVRN,
            Feature::SMCP => tag::SMCP,
            Feature::TNUM => tag::TNUM,
            Feature::VATU => tag::VATU,
            Feature::VRT2_OR_VERT => tag::VRT2,
            Feature::ZERO => tag::ZERO,
        }
    }

    pub fn from_tag(tag: u32) -> Option<Feature> {
        match tag {
            tag::ABVF => Some(Feature::ABVF),
            tag::ABVS => Some(Feature::ABVS),
            tag::AFRC => Some(Feature::AFRC),
            tag::AKHN => Some(Feature::AKHN),
            tag::BLWF => Some(Feature::BLWF),
            tag::BLWS => Some(Feature::BLWS),
            tag::C2SC => Some(Feature::C2SC),
            tag::CALT => Some(Feature::CALT),
            tag::CASE => Some(Feature::CASE),
            tag::CCMP => Some(Feature::CCMP),
            tag::CFAR => Some(Feature::CFAR),
            tag::CJCT => Some(Feature::CJCT),
            tag::CLIG => Some(Feature::CLIG),
            tag::CPSP => Some(Feature::CPSP),
            tag::CSWH => Some(Feature::CSWH),
            tag::DLIG => Some(Feature::DLIG),
            tag::FINA => Some(Feature::FINA),
            tag::FIN2 => Some(Feature::FIN2),
            tag::FIN3 => Some(Feature::FIN3),
            tag::FRAC => Some(Feature::FRAC),
            tag::HALF => Some(Feature::HALF),
            tag::HALN => Some(Feature::HALN),
            tag::HIST => Some(Feature::HIST),
            tag::HLIG => Some(Feature::HLIG),
            tag::INIT => Some(Feature::INIT),
            tag::ISOL => Some(Feature::ISOL),
            tag::LIGA => Some(Feature::LIGA),
            tag::LNUM => Some(Feature::LNUM),
            tag::LOCL => Some(Feature::LOCL),
            tag::MEDI => Some(Feature::MEDI),
            tag::MED2 => Some(Feature::MED2),
            tag::MSET => Some(Feature::MSET),
            tag::NUKT => Some(Feature::NUKT),
            tag::ONUM => Some(Feature::ONUM),
            tag::ORDN => Some(Feature::ORDN),
            tag::PNUM => Some(Feature::PNUM),
            tag::PREF => Some(Feature::PREF),
            tag::PRES => Some(Feature::PRES),
            tag::PSTF => Some(Feature::PSTF),
            tag::PSTS => Some(Feature::PSTS),
            tag::RCLT => Some(Feature::RCLT),
            tag::RKRF => Some(Feature::RKRF),
            tag::RLIG => Some(Feature::RLIG),
            tag::RPHF => Some(Feature::RPHF),
            tag::RVRN => Some(Feature::RVRN),
            tag::SMCP => Some(Feature::SMCP),
            tag::TNUM => Some(Feature::TNUM),
            tag::VATU => Some(Feature::VATU),
            tag::VERT => Some(Feature::VRT2_OR_VERT),
            tag::VRT2 => Some(Feature::VRT2_OR_VERT),
            tag::ZERO => Some(Feature::ZERO),
            _ => None,
        }
    }
}

/// Extension trait adding methods to `FeatureMask` (`BitFlags<Feature>`).
pub trait FeatureMaskExt {
    /// Convert a feature tag to a FeatureMask. Returns empty mask for unknown tags.
    fn from_tag(tag: u32) -> FeatureMask;

    /// Return the default FeatureMask for basic Latin/default shaping.
    fn default_mask() -> FeatureMask;

    /// Iterate over the individual features in a FeatureMask.
    fn features(&self) -> impl Iterator<Item = FeatureInfo>;
}

impl FeatureMaskExt for FeatureMask {
    fn from_tag(tag: u32) -> FeatureMask {
        Feature::from_tag(tag).map_or(FeatureMask::empty(), FeatureMask::from)
    }

    fn default_mask() -> FeatureMask {
        Feature::CCMP
            | Feature::RLIG
            | Feature::CLIG
            | Feature::LIGA
            | Feature::LOCL
            | Feature::CALT
    }

    fn features(&self) -> impl Iterator<Item = FeatureInfo> {
        self.iter().map(|feature| FeatureInfo {
            feature_tag: feature.tag(),
            alternate: None,
        })
    }
}

pub fn features_supported(
    gsub_cache: &LayoutCache<GSUB>,
    script_tag: u32,
    opt_lang_tag: Option<u32>,
    feature_mask: FeatureMask,
) -> Result<bool, ShapingError> {
    let supported_features = get_supported_features(gsub_cache, script_tag, opt_lang_tag)?;
    Ok(supported_features.contains(feature_mask))
}

pub fn get_lookups_cache_index(
    gsub_cache: &LayoutCache<GSUB>,
    script_tag: u32,
    opt_lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    feature_mask: FeatureMask,
) -> Result<usize, ParseError> {
    let index = match gsub_cache.lookups_index.lock().unwrap().entry((
        script_tag,
        lang_tag_key(opt_lang_tag),
        feature_mask.bits(),
    )) {
        Entry::Occupied(entry) => *entry.get(),
        Entry::Vacant(entry) => {
            let gsub_table = &gsub_cache.layout_table;
            if let Some(script) = gsub_table.find_script_or_default(script_tag)? {
                if let Some(langsys) = script.find_langsys_or_default(opt_lang_tag)? {
                    let lookups = build_lookups_default(
                        gsub_table,
                        langsys,
                        feature_mask,
                        feature_variations,
                    )?;
                    let mut cached_lookups = gsub_cache.cached_lookups.lock().unwrap();
                    let index = cached_lookups.len();
                    cached_lookups.push(lookups);
                    *entry.insert(index)
                } else {
                    *entry.insert(0)
                }
            } else {
                *entry.insert(0)
            }
        }
    };
    Ok(index)
}

/// Perform glyph substitution according to the supplied features, script and language.
///
/// `dotted_circle_index` is the glyph index of U+25CC DOTTED CIRCLE: ◌. This is inserted
/// when shaping some complex scripts where the input text contains incomplete syllables.
/// If you have an instance of `FontDataImpl` the glyph index can be retrieved via the
/// `lookup_glyph_index` method.
///
/// ## Example
///
/// The following shows a complete example of loading a font, mapping text to glyphs, and
/// applying glyph substitution.
///
/// ```
/// use std::error::Error;
/// use std::sync::Arc;
///
/// use allsorts::binary::read::ReadScope;
/// use allsorts::error::ParseError;
/// use allsorts::font::{MatchingPresentation};
/// use allsorts::font_data::FontData;
/// use allsorts::gsub::{FeatureMask, FeatureMaskExt, GlyphOrigin, RawGlyph, RawGlyphFlags};
/// use allsorts::tinyvec::tiny_vec;
/// use allsorts::unicode::VariationSelector;
/// use allsorts::DOTTED_CIRCLE;
/// use allsorts::{gsub, tag, Font};
///
/// fn shape(text: &str) -> Result<Vec<RawGlyph<()>>, Box<dyn Error>> {
///     let script = tag::from_string("LATN")?;
///     let lang = tag::from_string("DFLT")?;
///     let buffer = std::fs::read("tests/fonts/opentype/Klei.otf")
///         .expect("unable to read Klei.otf");
///     let scope = ReadScope::new(&buffer);
///     let font_file = scope.read::<FontData<'_>>()?;
///     // Use a different index to access other fonts in a font collection (E.g. TTC)
///     let provider = font_file.table_provider(0)?;
///     let mut font = Font::new(provider)?;
///
///     let opt_gsub_cache = font.gsub_cache()?;
///     let opt_gpos_cache = font.gpos_cache()?;
///     let opt_gdef_table = font.gdef_table()?;
///     let opt_gdef_table = opt_gdef_table.as_ref().map(Arc::as_ref);
///
///     // Map glyphs
///     //
///     // We look ahead in the char stream for variation selectors. If one is found it is used for
///     // mapping the current glyph. When a variation selector is reached in the stream it is
///     // skipped as it was handled as part of the preceding character.
///     let mut chars_iter = text.chars().peekable();
///     let mut glyphs = Vec::new();
///     while let Some(ch) = chars_iter.next() {
///         match VariationSelector::try_from(ch) {
///             Ok(_) => {} // filter out variation selectors
///             Err(()) => {
///                 let vs = chars_iter
///                     .peek()
///                     .and_then(|&next| VariationSelector::try_from(next).ok());
///                 let (glyph_index, used_variation) = font.lookup_glyph_index(
///                     ch,
///                     MatchingPresentation::NotRequired,
///                     vs,
///                 );
///                 let glyph = RawGlyph {
///                     unicodes: tiny_vec![[char; 1] => ch],
///                     glyph_index: glyph_index,
///                     liga_component_pos: 0,
///                     glyph_origin: GlyphOrigin::Char(ch),
///                     flags: RawGlyphFlags::empty(),
///                     extra_data: (),
///                     variation: Some(used_variation),
///                 };
///                 glyphs.push(glyph);
///             }
///         }
///     }
///
///     let (dotted_circle_index, _) = font.lookup_glyph_index(
///         DOTTED_CIRCLE,
///         MatchingPresentation::NotRequired,
///         None,
///     );
///
///     // If the font was a variable font you would want to supply the variation tuple
///     let tuple = None;
///
///     // Apply gsub if table is present
///     let num_glyphs = font.num_glyphs();
///     if let Some(gsub_cache) = opt_gsub_cache {
///         gsub::apply(
///             dotted_circle_index,
///             &gsub_cache,
///             opt_gdef_table,
///             script,
///             Some(lang),
///             FeatureMask::default_mask(),
///             &[],
///             tuple,
///             num_glyphs,
///             &mut glyphs,
///         )?;
///     }
///
///     // This is where you would apply `gpos` if the table is present.
///
///     Ok(glyphs)
/// }
///
/// match shape("This is the first example.") {
///     Ok(glyphs) => {
///         assert!(!glyphs.is_empty());
///     }
///     Err(err) => panic!("Unable to shape text: {}", err),
/// }
/// ```
pub fn apply(
    dotted_circle_index: u16,
    gsub_cache: &LayoutCache<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    opt_lang_tag: Option<u32>,
    mut feature_mask: FeatureMask,
    custom_features: &[FeatureInfo],
    tuple: Option<Tuple<'_>>,
    num_glyphs: u16,
    glyphs: &mut Vec<RawGlyph<()>>,
) -> Result<(), ShapingError> {
    let max_glyphs = glyphs.len().saturating_mul(MAX_GLYPHS_FACTOR);
    let gsub_table = &gsub_cache.layout_table;
    let feature_variations = gsub_table.feature_variations(tuple)?;
    let feature_variations = feature_variations.as_ref();

    // Apply rvrn early if font is variable:
    //
    // "The 'rvrn' feature is mandatory: it should be active by default and not directly exposed to
    // user control."
    //
    // "It should be processed early in GSUB processing, before application of the localized forms
    // feature or features related to shaping of complex scripts or discretionary typographic
    // effects."
    //
    // https://learn.microsoft.com/en-us/typography/opentype/spec/features_pt#tag-rvrn
    if tuple.is_some() {
        apply_rvrn(
            gsub_cache,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            glyphs,
            max_glyphs,
        )?;
    }

    // Extract optional features requested by the user for script shapers.
    let extra_features = feature_mask & (Feature::DLIG | Feature::HLIG | Feature::HIST);

    match ScriptType::from(script_tag) {
        ScriptType::Arabic => scripts::arabic::gsub_apply_arabic(
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
            max_glyphs,
        )?,
        ScriptType::Indic => scripts::indic::gsub_apply_indic(
            dotted_circle_index,
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
        )?,
        ScriptType::Khmer => scripts::khmer::gsub_apply_khmer(
            dotted_circle_index,
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
        )?,
        ScriptType::Mongolian => scripts::mongolian::gsub_apply_mongolian(
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
            max_glyphs,
        )?,
        ScriptType::Myanmar => scripts::myanmar::gsub_apply_myanmar(
            dotted_circle_index,
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
        )?,
        ScriptType::Syriac => scripts::syriac::gsub_apply_syriac(
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
            max_glyphs,
        )?,
        ScriptType::Tibetan => scripts::tibetan::gsub_apply_tibetan(
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
            max_glyphs,
        )?,
        ScriptType::ThaiLao => scripts::thai_lao::gsub_apply_thai_lao(
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            script_tag,
            opt_lang_tag,
            feature_variations,
            extra_features,
            glyphs,
            max_glyphs,
        )?,
        ScriptType::Default => {
            feature_mask |= Feature::CCMP | Feature::RLIG | Feature::LOCL;
            feature_mask &= get_supported_features(gsub_cache, script_tag, opt_lang_tag)?;
            if feature_mask.contains(Feature::FRAC) {
                let index_frac = get_lookups_cache_index(
                    gsub_cache,
                    script_tag,
                    opt_lang_tag,
                    feature_variations,
                    feature_mask,
                )?;
                feature_mask.remove(Feature::FRAC);
                let index = get_lookups_cache_index(
                    gsub_cache,
                    script_tag,
                    opt_lang_tag,
                    feature_variations,
                    feature_mask,
                )?;
                let cached_lookups = gsub_cache.cached_lookups.lock().unwrap();
                let lookups = &cached_lookups[index];
                let lookups_frac = &cached_lookups[index_frac];
                gsub_apply_lookups_frac(
                    gsub_cache,
                    gsub_table,
                    opt_gdef_table,
                    lookups,
                    lookups_frac,
                    glyphs,
                    max_glyphs,
                )?;
            } else {
                let index = get_lookups_cache_index(
                    gsub_cache,
                    script_tag,
                    opt_lang_tag,
                    feature_variations,
                    feature_mask,
                )?;
                let lookups = &gsub_cache.cached_lookups.lock().unwrap()[index];
                gsub_apply_lookups(
                    gsub_cache,
                    gsub_table,
                    opt_gdef_table,
                    lookups,
                    glyphs,
                    max_glyphs,
                )?;
            }
        }
    }

    // Apply custom features (font-variant-alternates) after script-specific
    // shaping but before cleanup.
    gsub_apply_custom_features(
        gsub_cache,
        gsub_table,
        opt_gdef_table,
        script_tag,
        opt_lang_tag,
        feature_variations,
        feature_mask,
        custom_features,
        glyphs,
        max_glyphs,
    )?;

    strip_joiners(glyphs);
    replace_missing_glyphs(glyphs, num_glyphs);
    Ok(())
}

fn gsub_apply_custom_features(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    opt_lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    feature_mask: FeatureMask,
    custom_features: &[FeatureInfo],
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    if custom_features.is_empty() {
        return Ok(());
    }
    // Resolve the script table. For Indic scripts, the shaper uses the v2
    // tag (dev2, bng2, etc.) so we must look up features there too.
    let script_table = match ScriptType::from(script_tag) {
        ScriptType::Indic => {
            let indic2 = scripts::indic::indic2_tag(script_tag);
            match gsub_table.find_script(indic2)? {
                Some(table) => Some(table),
                None => gsub_table.find_script_or_default(script_tag)?,
            }
        }
        _ => gsub_table.find_script_or_default(script_tag)?,
    };
    if let Some(script) = script_table {
        if let Some(langsys) = script.find_langsys_or_default(opt_lang_tag)? {
            let lookups = build_lookups_custom(
                gsub_table,
                langsys,
                custom_features,
                feature_mask,
                feature_variations,
            )?;
            for (lookup_index, feature_tag) in lookups {
                let alternate = find_alternate(custom_features, feature_tag);
                gsub_apply_lookup(
                    gsub_cache,
                    gsub_table,
                    opt_gdef_table,
                    lookup_index,
                    feature_tag,
                    alternate,
                    glyphs,
                    max_glyphs,
                    0,
                    glyphs.len(),
                    |_| true,
                )?;
            }
        }
    }
    Ok(())
}

fn gsub_apply_lookups(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    lookups: &[(usize, u32)],
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    gsub_apply_lookups_impl(
        gsub_cache,
        gsub_table,
        opt_gdef_table,
        lookups,
        glyphs,
        max_glyphs,
        0,
        glyphs.len(),
    )?;
    Ok(())
}

fn gsub_apply_lookups_impl(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    lookups: &[(usize, u32)],
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
    start: usize,
    mut length: usize,
) -> Result<usize, ShapingError> {
    for (lookup_index, feature_tag) in lookups {
        length = gsub_apply_lookup(
            gsub_cache,
            gsub_table,
            opt_gdef_table,
            *lookup_index,
            *feature_tag,
            None,
            glyphs,
            max_glyphs,
            start,
            length,
            |_| true,
        )?;
    }
    Ok(length)
}

fn gsub_apply_lookups_frac(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    lookups: &[(usize, u32)],
    lookups_frac: &[(usize, u32)],
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    let mut i = 0;
    while i < glyphs.len() {
        if let Some((start_pos, _slash_pos, end_pos)) = find_fraction(&glyphs[i..]) {
            if start_pos > 0 {
                i += gsub_apply_lookups_impl(
                    gsub_cache,
                    gsub_table,
                    opt_gdef_table,
                    lookups,
                    glyphs,
                    max_glyphs,
                    i,
                    start_pos,
                )?;
            }
            i += gsub_apply_lookups_impl(
                gsub_cache,
                gsub_table,
                opt_gdef_table,
                lookups_frac,
                glyphs,
                max_glyphs,
                i,
                end_pos - start_pos + 1,
            )?;
        } else {
            gsub_apply_lookups_impl(
                gsub_cache,
                gsub_table,
                opt_gdef_table,
                lookups,
                glyphs,
                max_glyphs,
                i,
                glyphs.len() - i,
            )?;
            break;
        }
    }
    Ok(())
}

fn find_fraction(glyphs: &[RawGlyph<()>]) -> Option<(usize, usize, usize)> {
    let slash_pos = glyphs
        .iter()
        .position(|g| g.glyph_origin == GlyphOrigin::Char('/'))?;
    let mut start_pos = slash_pos;
    while start_pos > 0 {
        match glyphs[start_pos - 1].glyph_origin {
            GlyphOrigin::Char(c) if c.is_ascii_digit() => {
                start_pos -= 1;
            }
            _ => break,
        }
    }
    let mut end_pos = slash_pos;
    while end_pos + 1 < glyphs.len() {
        match glyphs[end_pos + 1].glyph_origin {
            GlyphOrigin::Char(c) if c.is_ascii_digit() => {
                end_pos += 1;
            }
            _ => break,
        }
    }
    if start_pos < slash_pos && slash_pos < end_pos {
        Some((start_pos, slash_pos, end_pos))
    } else {
        None
    }
}

fn apply_rvrn(
    gsub_cache: &LayoutCache<GSUB>,
    opt_gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    opt_lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    let gsub_table = &gsub_cache.layout_table;
    let index = get_lookups_cache_index(
        gsub_cache,
        script_tag,
        opt_lang_tag,
        feature_variations,
        Feature::RVRN.mask(),
    )?;
    let lookups = &gsub_cache.cached_lookups.lock().unwrap()[index];
    gsub_apply_lookups(
        gsub_cache,
        gsub_table,
        opt_gdef_table,
        lookups,
        glyphs,
        max_glyphs,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        binary::read::ReadScope, font::MatchingPresentation, font_data::FontData,
        tests::read_fixture, Font,
    };

    #[test]
    fn feature_mask_iter() {
        let mask = FeatureMask::empty();
        assert_eq!(mask.features().count(), 0);

        let mask = FeatureMask::default_mask();
        let expected = &[
            FeatureInfo {
                feature_tag: tag::CALT,
                alternate: None,
            },
            FeatureInfo {
                feature_tag: tag::CCMP,
                alternate: None,
            },
            FeatureInfo {
                feature_tag: tag::CLIG,
                alternate: None,
            },
            FeatureInfo {
                feature_tag: tag::LIGA,
                alternate: None,
            },
            FeatureInfo {
                feature_tag: tag::LOCL,
                alternate: None,
            },
            FeatureInfo {
                feature_tag: tag::RLIG,
                alternate: None,
            },
        ];
        assert_eq!(&mask.features().collect::<Vec<_>>(), expected);
    }

    /// Verify that Feature::tag and Feature::from_tag stay in sync.
    ///
    /// Every Feature variant must round-trip through tag/from_tag.
    #[test]
    fn feature_from_tag_in_sync() {
        // Check that every Feature variant round-trips through tag/from_tag.
        for feature in FeatureMask::all() {
            let tag = feature.tag();
            let back = Feature::from_tag(tag);
            assert!(
                back == Some(feature),
                "from_tag(tag({:?})) = {:?}, expected Some({:?})",
                feature,
                back,
                feature,
            );
        }
    }

    #[test]
    fn billion_laughs() -> Result<(), Box<dyn std::error::Error>> {
        let data = read_fixture("tests/fonts/opentype/TestGSUBThree.ttf");
        let scope = ReadScope::new(&data);
        let font_file = scope.read::<FontData<'_>>()?;
        let provider = font_file.table_provider(0)?;
        let mut font = Font::new(provider)?;

        // Map text to glyphs and then apply font shaping
        let script = tag::LATN;
        let lang = tag!(b"ENG ");
        let glyphs = font.map_glyphs("lol", script, MatchingPresentation::NotRequired);
        let infos = font
            .shape(
                glyphs,
                script,
                Some(lang),
                FeatureMask::default_mask(),
                &[],
                None,
                true,
            )
            .map_err(|(err, _info)| err)?;

        assert_eq!(infos.len(), 759);

        Ok(())
    }
}
