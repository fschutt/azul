//! Glyph positioning (`gpos`) implementation.
//!
//! > The Glyph Positioning table (GPOS) provides precise control over glyph placement for
//! > sophisticated text layout and rendering in each script and language system that a font
//! > supports.
//!
//! — <https://docs.microsoft.com/en-us/typography/opentype/spec/gpos>

use tinyvec::tiny_vec;
use unicode_general_category::GeneralCategory;

use crate::context::{ContextLookupHelper, Glyph, LookupFlag, MatchType};
use crate::error::ParseError;
use crate::gdef::gdef_is_mark;
use crate::gsub::{FeatureInfo, FeatureMask, FeatureMaskExt, RawGlyph};
use crate::layout::{
    chain_context_lookup_info, context_lookup_info, Adjust, Anchor, ChainContextLookup,
    ContextLookup, CursivePos, GDEFTable, LangSys, LayoutCache, LayoutTable, LookupList,
    MarkBasePos, MarkLigPos, PairPos, PosLookup, SinglePos, ValueRecord, VariationIndex, GPOS,
};
use crate::scripts;
use crate::scripts::ScriptType;
use crate::tables::kern::{self, KernTable};
use crate::tables::variable_fonts::fvar::Tuple;
use crate::tables::variable_fonts::owned;
use crate::tag;

type PosContext<'a> = ContextLookupHelper<'a, GPOS>;

/// Apply glyph positioning rules to glyph `Info`.
pub fn apply(
    gpos_cache: &LayoutCache<GPOS>,
    opt_gdef_table: Option<&GDEFTable>,
    kern_table: Option<KernTable<'_>>,
    kerning: bool,
    feature_mask: FeatureMask,
    custom_features: &[FeatureInfo],
    tuple: Option<Tuple<'_>>,
    script_tag: u32,
    opt_lang_tag: Option<u32>,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    let gpos_table = &gpos_cache.layout_table;
    let script_type = ScriptType::from(script_tag);

    let script = match script_type {
        ScriptType::Indic => {
            let indic2_tag = scripts::indic::indic2_tag(script_tag);
            match gpos_table.find_script(indic2_tag)? {
                Some(script) => script,
                None => match gpos_table.find_script_or_default(script_tag)? {
                    Some(script) => script,
                    None => return Ok(()),
                },
            }
        }
        _ => match gpos_table.find_script_or_default(script_tag)? {
            Some(script) => script,
            None => return Ok(()),
        },
    };

    let langsys = match script.find_langsys_or_default(opt_lang_tag)? {
        Some(langsys) => langsys,
        None => return Ok(()),
    };

    let base_features: &[u32] = match script_type {
        ScriptType::Arabic => &[tag::CURS, tag::KERN, tag::MARK, tag::MKMK],
        ScriptType::Mongolian => &[tag::CURS, tag::KERN, tag::MARK, tag::MKMK],
        ScriptType::Indic => &[
            tag::ABVM,
            tag::BLWM,
            tag::DIST,
            tag::KERN,
            tag::MARK,
            tag::MKMK,
        ],
        ScriptType::Khmer => &[tag::ABVM, tag::BLWM, tag::DIST, tag::MARK, tag::MKMK],
        // opentype-shaping-docs: kern is not mandatory for shaping Myanmar text and may be disabled by user preference.
        ScriptType::Myanmar if kerning => &[
            tag::DIST,
            tag::ABVM,
            tag::BLWM,
            tag::MARK,
            tag::MKMK,
            tag::KERN,
        ],
        ScriptType::Myanmar => &[tag::DIST, tag::ABVM, tag::BLWM, tag::MARK, tag::MKMK],
        ScriptType::Syriac => &[tag::CURS, tag::KERN, tag::MARK, tag::MKMK],
        ScriptType::Tibetan => &[tag::KERN, tag::ABVM, tag::BLWM, tag::MARK, tag::MKMK],
        ScriptType::ThaiLao => &[tag::KERN, tag::MARK, tag::MKMK],
        ScriptType::Default if kerning => &[tag::DIST, tag::KERN, tag::MARK, tag::MKMK],
        ScriptType::Default => &[tag::DIST, tag::MARK, tag::MKMK],
    };

    apply_features(
        gpos_cache,
        gpos_table,
        opt_gdef_table,
        kern_table,
        langsys,
        base_features.iter().map(|&feature_tag| FeatureInfo {
            feature_tag,
            alternate: None,
        }),
        tuple,
        script_tag,
        infos,
    )?;
    apply_features(
        gpos_cache,
        gpos_table,
        opt_gdef_table,
        kern_table,
        langsys,
        feature_mask.features(),
        tuple,
        script_tag,
        infos,
    )?;
    if !custom_features.is_empty() {
        apply_features(
            gpos_cache,
            gpos_table,
            opt_gdef_table,
            kern_table,
            langsys,
            custom_features.iter().copied(),
            tuple,
            script_tag,
            infos,
        )?;
    }
    Ok(())
}

/// Apply glyph positioning using specified OpenType features.
///
/// Generally prefer to use [apply], which will enable features based on script and language.
/// Use this method if you need more low-level control over the enabled features.
pub fn apply_features(
    gpos_cache: &LayoutCache<GPOS>,
    gpos_table: &LayoutTable<GPOS>,
    opt_gdef_table: Option<&GDEFTable>,
    kern_table: Option<KernTable<'_>>,
    langsys: &LangSys,
    features: impl Iterator<Item = FeatureInfo>,
    tuple: Option<Tuple<'_>>,
    script_tag: u32,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    let mut lookup_indices = tiny_vec!([u16; 128]);
    let feature_variations = gpos_table.feature_variations(tuple)?;

    // Collect the lookup indices in order across all features
    let mut should_apply_kern = None;
    for feature in features {
        let feature_table = gpos_table.find_langsys_feature(
            langsys,
            feature.feature_tag,
            feature_variations.as_ref(),
        )?;

        match feature_table {
            Some(feature_table) => {
                lookup_indices.extend_from_slice(&feature_table.lookup_indices);
            }
            // Apply kerning from kern table if `kern` feature was requested, but there is no `kern`
            // feature table in `GPOS`.
            None if feature.feature_tag == tag::KERN && kern_table.is_some() => {
                // NOTE(unwrap): Safe due to `is_some` call above
                should_apply_kern = Some(kern_table.unwrap());
            }
            None => {}
        }
    }
    lookup_indices.sort_unstable();

    // Apply kerning from kern table if there is no kern feature table
    if let Some(kern) = should_apply_kern {
        kern::apply(&kern, script_tag, infos)?;
    }

    // Equivalent to `Itertools::dedup` — skip runs of equal consecutive
    // values. The slice is sorted just above, so this collapses duplicates.
    let mut last: Option<u16> = None;
    for lookup_index in lookup_indices.iter().copied().filter(|&i| {
        let new = last != Some(i);
        last = Some(i);
        new
    }) {
        gpos_apply_lookup(
            gpos_cache,
            gpos_table,
            opt_gdef_table,
            usize::from(lookup_index),
            tuple,
            infos,
        )?;
    }
    Ok(())
}

/// Apply `kern` and basic mark processing when there is no `GPOS` table available.
///
/// Call this method when there is no `LayoutCache<GPOS>` available for this font.
pub fn apply_fallback(
    kern_table: Option<KernTable<'_>>,
    script_tag: u32,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    // Apply kerning from `kern` table if present
    if let Some(kern) = kern_table {
        kern::apply(&kern, script_tag, infos)?;
    }
    apply_fallback_mark_positioning(infos);
    Ok(())
}

/// Apply fallback mark positioning.
///
/// Call this function if a font lacks a mechanism for positioning marks.
pub fn apply_fallback_mark_positioning(infos: &mut [Info]) {
    for info in infos.iter_mut() {
        if !info.is_mark && unicodes_are_marks(&info.glyph.unicodes) {
            info.is_mark = true;
        }
    }
    let mut base_index = 0;
    for (i, info) in infos.iter_mut().enumerate().skip(1) {
        if info.is_mark {
            info.placement = Placement::MarkOverprint(base_index);
        } else {
            base_index = i;
        }
    }
}

fn unicodes_are_marks(unicodes: &[char]) -> bool {
    unicodes
        .iter()
        .copied()
        .map(unicode_general_category::get_general_category)
        .all(|cat| cat == GeneralCategory::NonspacingMark)
}

fn gpos_apply_lookup(
    gpos_cache: &LayoutCache<GPOS>,
    gpos_table: &LayoutTable<GPOS>,
    opt_gdef_table: Option<&GDEFTable>,
    lookup_index: usize,
    tuple: Option<Tuple<'_>>,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    if let Some(ref lookup_list) = gpos_table.opt_lookup_list {
        let lookup = lookup_list.lookup_cache_gpos(gpos_cache, lookup_index)?;
        let match_type = MatchType::from_lookup_flag(lookup.lookup_flag, lookup.mark_filtering_set);
        match lookup.lookup_subtables {
            PosLookup::SinglePos(ref subtables) => {
                forall_glyphs_match(match_type, opt_gdef_table, infos, |i, infos| {
                    singlepos(subtables, tuple, opt_gdef_table, &mut infos[i])
                })
            }
            PosLookup::PairPos(ref subtables) => {
                // Spec suggests that the lookup will only be applied to the second glyph if it was
                // not repositioned, ie. if the value_format is zero, but applying the lookup
                // regardless does not break any test cases.
                forall_glyph_pairs_match(match_type, opt_gdef_table, infos, |i1, i2, infos| {
                    pairpos(subtables, tuple, opt_gdef_table, i1, i2, infos)
                })
            }
            PosLookup::CursivePos(ref subtables) => forall_glyph_pairs_match(
                MatchType::ignore_marks(),
                opt_gdef_table,
                infos,
                |i1, i2, infos| cursivepos(subtables, i1, i2, lookup.lookup_flag, infos),
            ),
            PosLookup::MarkBasePos(ref subtables) => {
                forall_base_mark_glyph_pairs(infos, |i1, i2, infos| {
                    markbasepos(subtables, i1, i2, infos)
                })
            }
            PosLookup::MarkLigPos(ref subtables) => {
                forall_base_mark_glyph_pairs(infos, |i1, i2, infos| {
                    markligpos(subtables, i1, i2, infos)
                })
            }
            PosLookup::MarkMarkPos(ref subtables) => {
                forall_mark_mark_glyph_pairs(infos, |i1, i2, infos| {
                    markmarkpos(subtables, i1, i2, infos)
                })
            }
            PosLookup::ContextPos(ref subtables) => {
                forall_glyphs_match(match_type, opt_gdef_table, infos, |i, infos| {
                    contextpos(
                        gpos_cache,
                        lookup_list,
                        opt_gdef_table,
                        tuple,
                        match_type,
                        subtables,
                        i,
                        infos,
                    )
                })
            }
            PosLookup::ChainContextPos(ref subtables) => {
                forall_glyphs_chain_match(match_type, opt_gdef_table, infos, |i, infos| {
                    chaincontextpos(
                        gpos_cache,
                        lookup_list,
                        opt_gdef_table,
                        tuple,
                        match_type,
                        subtables,
                        i,
                        infos,
                    )
                })
            }
        }
    } else {
        Ok(())
    }
}

fn gpos_lookup_singlepos(
    subtables: &[SinglePos],
    glyph_index: u16,
) -> Result<ValueRecord, ParseError> {
    for singlepos in subtables {
        if let Some(val) = singlepos.apply(glyph_index)? {
            return Ok(Some(val));
        }
    }
    Ok(None)
}

fn gpos_lookup_pairpos(
    subtables: &[PairPos],
    glyph_index1: u16,
    glyph_index2: u16,
) -> Result<Option<(ValueRecord, ValueRecord)>, ParseError> {
    for pairpos in subtables {
        if let Some((val1, val2)) = pairpos.apply(glyph_index1, glyph_index2)? {
            return Ok(Some((val1, val2)));
        }
    }
    Ok(None)
}

fn gpos_lookup_cursivepos(
    subtables: &[CursivePos],
    glyph_index1: u16,
    glyph_index2: u16,
) -> Result<Option<(Anchor, Anchor)>, ParseError> {
    for cursivepos in subtables {
        if let Some((an1, an2)) = cursivepos.apply(glyph_index1, glyph_index2)? {
            return Ok(Some((an1, an2)));
        }
    }
    Ok(None)
}

fn gpos_lookup_markbasepos(
    subtables: &[MarkBasePos],
    glyph_index1: u16,
    glyph_index2: u16,
) -> Result<Option<(Anchor, Anchor)>, ParseError> {
    for markbasepos in subtables {
        if let Some((an1, an2)) = markbasepos.apply(glyph_index1, glyph_index2)? {
            return Ok(Some((an1, an2)));
        }
    }
    Ok(None)
}

fn gpos_lookup_markligpos(
    subtables: &[MarkLigPos],
    glyph_index1: u16,
    glyph_index2: u16,
    liga_component_index: u16,
) -> Result<Option<(Anchor, Anchor)>, ParseError> {
    for markligpos in subtables {
        if let Some((an1, an2)) = markligpos.apply(
            glyph_index1,
            glyph_index2,
            usize::from(liga_component_index),
        )? {
            return Ok(Some((an1, an2)));
        }
    }
    Ok(None)
}

fn gpos_lookup_markmarkpos(
    subtables: &[MarkBasePos],
    glyph_index1: u16,
    glyph_index2: u16,
) -> Result<Option<(Anchor, Anchor)>, ParseError> {
    for markmarkpos in subtables {
        if let Some((an1, an2)) = markmarkpos.apply(glyph_index1, glyph_index2)? {
            return Ok(Some((an1, an2)));
        }
    }
    Ok(None)
}

fn gpos_lookup_contextpos<'a>(
    opt_gdef_table: Option<&GDEFTable>,
    match_type: MatchType,
    subtables: &'a [ContextLookup<GPOS>],
    glyph_index: u16,
    i: usize,
    infos: &mut [Info],
) -> Result<Option<Box<PosContext<'a>>>, ParseError> {
    for context_lookup in subtables {
        if let Some(context) = context_lookup_info(context_lookup, glyph_index, |context| {
            context.matches(opt_gdef_table, match_type, infos, i)
        })? {
            return Ok(Some(context));
        }
    }
    Ok(None)
}

fn gpos_lookup_chaincontextpos<'a>(
    opt_gdef_table: Option<&GDEFTable>,
    match_type: MatchType,
    subtables: &'a [ChainContextLookup<GPOS>],
    glyph_index: u16,
    i: usize,
    infos: &mut [Info],
) -> Result<Option<Box<PosContext<'a>>>, ParseError> {
    for chain_context_lookup in subtables {
        if let Some(context) =
            chain_context_lookup_info(chain_context_lookup, glyph_index, |context| {
                context.matches(opt_gdef_table, match_type, infos, i)
            })?
        {
            return Ok(Some(context));
        }
    }
    Ok(None)
}

/// Adjustment to the placement of a glyph as a result of kerning and
/// placement of an attachment relative to a base glyph.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Placement {
    None,
    /// Placement offset by distance delta.
    ///
    /// Fields
    /// (delta x, delta y)
    Distance(i32, i32),
    /// An anchored mark.
    ///
    /// This is a mark where its anchor is aligned with the base glyph anchor.
    ///
    /// Fields:
    /// (base glyph index in `Vec<Info>`, base glyph anchor, mark anchor)
    MarkAnchor(usize, Anchor, Anchor),
    /// An overprint mark.
    ///
    /// This mark is shown at the same position as the base glyph.
    ///
    /// Fields:
    /// (base glyph index in `Vec<Info>`)
    MarkOverprint(usize),
    /// Cursive anchored placement.
    ///
    /// Fields:
    /// * exit glyph index in `Vec<Info>`,
    /// * [RIGHT_TO_LEFT flag from lookup table](https://docs.microsoft.com/en-us/typography/opentype/spec/chapter2#lookupFlags_1),
    /// * exit glyph anchor,
    /// * entry glyph anchor
    ///
    /// <https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-3-cursive-attachment-positioning-subtable>
    CursiveAnchor(usize, bool, Anchor, Anchor),
}

impl Placement {
    pub(super) fn combine_distance(&mut self, x2: i32, y2: i32) {
        use Placement::*;

        *self = match *self {
            None | MarkOverprint(_) => Distance(x2, y2),
            // FIXME HarfBuzz also updates cursive anchors
            // but we haven't found any fonts that test this codepath yet.
            CursiveAnchor(..) => Distance(x2, y2),
            Distance(x1, y1) => Distance(x1 + x2, y1 + y2),
            MarkAnchor(i, an1, an2) => {
                let x = an1.x + (x2 as i16);
                let y = an1.y + (y2 as i16);
                MarkAnchor(i, Anchor { x, y }, an2)
            }
        }
    }
}

/// A positioned glyph.
///
/// This struct is the output of applying glyph positioning (`gpos`). It contains the glyph
/// and information about how it should be positioned.
///
/// For more information about glyph placement refer to the OpenType documentation:
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#positioning-glyphs-with-opentype>
#[derive(Clone, Debug)]
pub struct Info {
    /// The glyph.
    pub glyph: RawGlyph<()>,
    /// An offset from the horizontal glyph advance position for this glyph.
    pub kerning: i16,
    /// When not `Placement::None` indicates that this glyph should be placed according to
    /// the variant.
    pub placement: Placement,
    /// Indicates that cross-stream kerning values (i.e., kerning values perpendicular to the flow
    /// of text) should be reset to zero, and should no longer be accumulated to.
    pub reset_cross_stream: bool,
    is_mark: bool,
}

impl Glyph for Info {
    fn get_glyph_index(&self) -> u16 {
        self.glyph.glyph_index
    }
}

impl Info {
    pub fn init_from_glyphs(
        opt_gdef_table: Option<&GDEFTable>,
        glyphs: Vec<RawGlyph<()>>,
    ) -> Vec<Info> {
        let mut infos = Vec::with_capacity(glyphs.len());
        for glyph in glyphs {
            let is_mark = gdef_is_mark(opt_gdef_table, glyph.glyph_index);
            let info = Info {
                glyph,
                kerning: 0,
                placement: Placement::None,
                reset_cross_stream: false,
                is_mark,
            };
            infos.push(info);
        }
        infos
    }
}

impl Adjust {
    fn apply(&self, tuple: Option<Tuple<'_>>, opt_gdef_table: Option<&GDEFTable>, info: &mut Info) {
        let variation_store =
            opt_gdef_table.and_then(|gdef| gdef.opt_item_variation_store.as_ref());
        if self.x_placement == 0 && self.y_placement == 0 {
            if self.x_advance != 0 && self.y_advance == 0 {
                info.kerning +=
                    self.x_advance + self.x_advance_delta(tuple, variation_store).round() as i16;
            } else if self.y_advance != 0 {
                // error: y_advance non-zero
            } else {
                // both zero, but delta could still be present
                let x_advance_delta = self.x_advance_delta(tuple, variation_store).round() as i16;
                info.kerning += x_advance_delta;
            }
        } else if self.y_advance == 0 {
            let x_placement = i32::from(self.x_placement)
                + self.x_placement_delta(tuple, variation_store).round() as i32;
            let y_placement = i32::from(self.y_placement)
                + self.y_placement_delta(tuple, variation_store).round() as i32;
            info.placement.combine_distance(x_placement, y_placement);

            let x_advance =
                self.x_advance + self.x_advance_delta(tuple, variation_store).round() as i16;
            info.kerning += x_advance;
        } else {
            // error: y_advance non-zero
        }
    }

    pub fn x_advance_delta(
        &self,
        tuple: Option<Tuple<'_>>,
        variation_store: Option<&owned::ItemVariationStore>,
    ) -> f32 {
        Self::delta(self.x_advance_variation.as_ref(), tuple, variation_store)
    }

    pub fn y_advance_delta(
        &self,
        tuple: Option<Tuple<'_>>,
        variation_store: Option<&owned::ItemVariationStore>,
    ) -> f32 {
        Self::delta(self.y_advance_variation.as_ref(), tuple, variation_store)
    }

    pub fn x_placement_delta(
        &self,
        tuple: Option<Tuple<'_>>,
        variation_store: Option<&owned::ItemVariationStore>,
    ) -> f32 {
        Self::delta(self.x_placement_variation.as_ref(), tuple, variation_store)
    }

    pub fn y_placement_delta(
        &self,
        tuple: Option<Tuple<'_>>,
        variation_store: Option<&owned::ItemVariationStore>,
    ) -> f32 {
        Self::delta(self.y_placement_variation.as_ref(), tuple, variation_store)
    }

    fn delta(
        variation: Option<&VariationIndex>,
        tuple: Option<Tuple<'_>>,
        variation_store: Option<&owned::ItemVariationStore>,
    ) -> f32 {
        match (tuple, variation_store, variation) {
            (Some(tuple), Some(store), Some(placement_variation)) => {
                store.adjustment(*placement_variation, tuple).unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }
}

fn forall_glyphs_match(
    match_type: MatchType,
    opt_gdef_table: Option<&GDEFTable>,
    infos: &mut [Info],
    f: impl Fn(usize, &mut [Info]) -> Result<(), ParseError>,
) -> Result<(), ParseError> {
    for i in 0..infos.len() {
        if match_type.match_glyph(opt_gdef_table, &infos[i]) {
            f(i, infos)?;
        }
    }
    Ok(())
}

fn forall_glyphs_chain_match(
    match_type: MatchType,
    opt_gdef_table: Option<&GDEFTable>,
    infos: &mut [Info],
    f: impl Fn(usize, &mut [Info]) -> Result<usize, ParseError>,
) -> Result<(), ParseError> {
    let mut i = 0;
    while i < infos.len() {
        // `f` returns how many glyphs were matched
        let inc = if match_type.match_glyph(opt_gdef_table, &infos[i]) {
            // We always want to increment by at least one glyph to avoid getting stuck.
            f(i, infos)?.max(1)
        } else {
            1
        };
        i += inc;
    }
    Ok(())
}

fn forall_glyph_pairs_match(
    match_type: MatchType,
    opt_gdef_table: Option<&GDEFTable>,
    infos: &mut [Info],
    f: impl Fn(usize, usize, &mut [Info]) -> Result<(), ParseError>,
) -> Result<(), ParseError> {
    if let Some(mut i1) = match_type.find_first(opt_gdef_table, infos) {
        while let Some(i2) = match_type.find_next(opt_gdef_table, infos, i1) {
            f(i1, i2, infos)?;
            i1 = i2;
        }
    }
    Ok(())
}

fn forall_base_mark_glyph_pairs(
    infos: &mut [Info],
    f: impl Fn(usize, usize, &mut [Info]) -> Result<(), ParseError>,
) -> Result<(), ParseError> {
    let mut i = 0;
    'outer: while i + 1 < infos.len() {
        if !infos[i].is_mark {
            for j in i + 1..infos.len() {
                f(i, j, infos)?;
                if !infos[j].is_mark {
                    i = j;
                    continue 'outer;
                }
            }
        }
        i += 1;
    }
    Ok(())
}

fn forall_mark_mark_glyph_pairs(
    infos: &mut [Info],
    f: impl Fn(usize, usize, &mut [Info]) -> Result<(), ParseError>,
) -> Result<(), ParseError> {
    let mut start = 0;
    'outer: loop {
        let mut i = start;
        while i + 1 < infos.len() {
            if infos[i].is_mark {
                // infos[i] is the base mark. Scan forward looking for attaching marks
                for j in i + 1..infos.len() {
                    if !infos[j].is_mark {
                        start = i + 1;
                        continue 'outer;
                    }

                    // infos[j] is a candidate attaching mark
                    if infos[i].glyph.liga_component_pos == infos[j].glyph.liga_component_pos {
                        f(i, j, infos)?;
                    } else if infos[i].glyph.ligature() || infos[j].glyph.ligature() {
                        f(i, j, infos)?;
                    }
                }
            }
            i += 1;
        }
        break;
    }
    Ok(())
}

fn singlepos(
    subtables: &[SinglePos],
    tuple: Option<Tuple<'_>>,
    opt_gdef_table: Option<&GDEFTable>,
    i: &mut Info,
) -> Result<(), ParseError> {
    let glyph_index = i.glyph.glyph_index;
    if let Some(adj) = gpos_lookup_singlepos(subtables, glyph_index)? {
        adj.apply(tuple, opt_gdef_table, i);
    }
    Ok(())
}

fn pairpos(
    subtables: &[PairPos],
    tuple: Option<Tuple<'_>>,
    opt_gdef_table: Option<&GDEFTable>,
    i1: usize,
    i2: usize,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    match gpos_lookup_pairpos(
        subtables,
        infos[i1].glyph.glyph_index,
        infos[i2].glyph.glyph_index,
    )? {
        Some((opt_adj1, opt_adj2)) => {
            if let Some(adj1) = opt_adj1 {
                adj1.apply(tuple, opt_gdef_table, &mut infos[i1]);
            }
            if let Some(adj2) = opt_adj2 {
                adj2.apply(tuple, opt_gdef_table, &mut infos[i2]);
            }
            Ok(())
        }
        None => Ok(()),
    }
}

fn cursivepos(
    subtables: &[CursivePos],
    i1: usize,
    i2: usize,
    lookup_flag: LookupFlag,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    match gpos_lookup_cursivepos(
        subtables,
        infos[i1].glyph.glyph_index,
        infos[i2].glyph.glyph_index,
    )? {
        Some((anchor1, anchor2)) => {
            infos[i1].placement =
                Placement::CursiveAnchor(i2, lookup_flag.get_rtl(), anchor2, anchor1);
            Ok(())
        }
        None => Ok(()),
    }
}

fn markbasepos(
    subtables: &[MarkBasePos],
    i1: usize,
    i2: usize,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    match gpos_lookup_markbasepos(
        subtables,
        infos[i1].glyph.glyph_index,
        infos[i2].glyph.glyph_index,
    )? {
        Some((anchor1, anchor2)) => {
            infos[i2].placement = Placement::MarkAnchor(i1, anchor1, anchor2);
            infos[i2].is_mark = true;
            Ok(())
        }
        None => Ok(()),
    }
}

fn markligpos(
    subtables: &[MarkLigPos],
    i1: usize,
    i2: usize,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    match gpos_lookup_markligpos(
        subtables,
        infos[i1].glyph.glyph_index,
        infos[i2].glyph.glyph_index,
        infos[i2].glyph.liga_component_pos,
    )? {
        Some((anchor1, anchor2)) => {
            infos[i2].placement = Placement::MarkAnchor(i1, anchor1, anchor2);
            infos[i2].is_mark = true;
            Ok(())
        }
        None => Ok(()),
    }
}

fn markmarkpos(
    subtables: &[MarkBasePos],
    i1: usize,
    i2: usize,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    match gpos_lookup_markmarkpos(
        subtables,
        infos[i1].glyph.glyph_index,
        infos[i2].glyph.glyph_index,
    )? {
        Some((anchor1, anchor2)) => {
            infos[i2].placement = Placement::MarkAnchor(i1, anchor1, anchor2);
            infos[i2].is_mark = true;
            Ok(())
        }
        None => Ok(()),
    }
}

fn contextpos(
    gpos_cache: &LayoutCache<GPOS>,
    lookup_list: &LookupList<GPOS>,
    opt_gdef_table: Option<&GDEFTable>,
    tuple: Option<Tuple<'_>>,
    match_type: MatchType,
    subtables: &[ContextLookup<GPOS>],
    i: usize,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    let glyph_index = infos[i].glyph.glyph_index;
    match gpos_lookup_contextpos(opt_gdef_table, match_type, subtables, glyph_index, i, infos)? {
        Some(pos) => apply_pos_context(
            gpos_cache,
            lookup_list,
            opt_gdef_table,
            tuple,
            match_type,
            &pos,
            i,
            infos,
        ),
        None => Ok(()),
    }
}

fn chaincontextpos(
    gpos_cache: &LayoutCache<GPOS>,
    lookup_list: &LookupList<GPOS>,
    opt_gdef_table: Option<&GDEFTable>,
    tuple: Option<Tuple<'_>>,
    match_type: MatchType,
    subtables: &[ChainContextLookup<GPOS>],
    i: usize,
    infos: &mut [Info],
) -> Result<usize, ParseError> {
    let glyph_index = infos[i].glyph.glyph_index;
    match gpos_lookup_chaincontextpos(opt_gdef_table, match_type, subtables, glyph_index, i, infos)?
    {
        Some(pos) => {
            apply_pos_context(
                gpos_cache,
                lookup_list,
                opt_gdef_table,
                tuple,
                match_type,
                &pos,
                i,
                infos,
            )?;

            // RangeInclusive<usize> does not implement ExactSizeIterator, so it has no len
            // method (because len returns a usize and if the range was 0..usize::MAX len
            // is not representable). However, in our case, we can only ever match up to u16::MAX
            // glyphs, so this isn't an issue.
            let len = pos.input_seq.end() - pos.input_seq.start() + 1;
            Ok(len)
        }
        // Just skip this glyph
        None => Ok(1),
    }
}

fn apply_pos_context(
    gpos_cache: &LayoutCache<GPOS>,
    lookup_list: &LookupList<GPOS>,
    opt_gdef_table: Option<&GDEFTable>,
    tuple: Option<Tuple<'_>>,
    _match_type: MatchType,
    pos: &PosContext<'_>,
    i: usize,
    infos: &mut [Info],
) -> Result<(), ParseError> {
    for (pos_index, pos_lookup_index) in pos.lookup_array {
        apply_pos(
            gpos_cache,
            lookup_list,
            opt_gdef_table,
            tuple,
            usize::from(*pos_index),
            usize::from(*pos_lookup_index),
            infos,
            i,
        )?;
    }
    Ok(())
}

fn apply_pos(
    gpos_cache: &LayoutCache<GPOS>,
    lookup_list: &LookupList<GPOS>,
    opt_gdef_table: Option<&GDEFTable>,
    tuple: Option<Tuple<'_>>,
    pos_index: usize,
    lookup_index: usize,
    infos: &mut [Info],
    index: usize,
) -> Result<(), ParseError> {
    let lookup = lookup_list.lookup_cache_gpos(gpos_cache, lookup_index)?;
    let match_type = MatchType::from_lookup_flag(lookup.lookup_flag, lookup.mark_filtering_set);
    let i1 = match match_type.find_nth(opt_gdef_table, infos, index, pos_index) {
        Some(index1) => index1,
        None => return Ok(()),
    };
    match lookup.lookup_subtables {
        PosLookup::SinglePos(ref subtables) => {
            singlepos(subtables, tuple, opt_gdef_table, &mut infos[i1])
        }
        PosLookup::PairPos(ref subtables) => {
            if let Some(i2) = match_type.find_next(opt_gdef_table, infos, i1) {
                pairpos(subtables, tuple, opt_gdef_table, i1, i2, infos)
            } else {
                Ok(())
            }
        }
        PosLookup::CursivePos(ref subtables) => {
            if let Some(i2) = match_type.find_next(opt_gdef_table, infos, i1) {
                cursivepos(subtables, i1, i2, lookup.lookup_flag, infos)
            } else {
                Ok(())
            }
        }
        PosLookup::MarkBasePos(ref subtables) => {
            // FIXME is this correct?
            if let Some(base_index) = MatchType::ignore_marks().find_prev(opt_gdef_table, infos, i1)
            {
                markbasepos(subtables, base_index, i1, infos)
            } else {
                Ok(())
            }
        }
        PosLookup::MarkLigPos(ref subtables) => {
            // FIXME is this correct?
            if let Some(base_index) = MatchType::ignore_marks().find_prev(opt_gdef_table, infos, i1)
            {
                markligpos(subtables, base_index, i1, infos)
            } else {
                Ok(())
            }
        }
        PosLookup::MarkMarkPos(ref subtables) => {
            // FIXME is this correct?
            if let Some(base_index) = match_type.find_prev(opt_gdef_table, infos, i1) {
                markmarkpos(subtables, base_index, i1, infos)
            } else {
                Ok(())
            }
        }
        PosLookup::ContextPos(ref _subtables) => Ok(()),
        PosLookup::ChainContextPos(ref _subtables) => Ok(()),
    }
}
