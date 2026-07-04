//! Implementation of font shaping for Syriac scripts
//!
//! Code herein follows the specification at:
//! <https://github.com/n8willis/opentype-shaping-documents/blob/master/opentype-shaping-syriac.md>

use unicode_joining_type::{get_joining_group, get_joining_type, JoiningGroup, JoiningType};

use crate::error::{ParseError, ShapingError};
use crate::gsub::{self, FeatureMask, GlyphData, GlyphOrigin, RawGlyph};
use crate::layout::{FeatureTableSubstitution, GDEFTable, LayoutCache, LayoutTable, GSUB};
use crate::tag;

#[derive(Clone)]
struct SyriacData {
    joining_group: JoiningGroup,
    joining_type: JoiningType,
    feature_tag: u32,
}

impl GlyphData for SyriacData {
    fn merge(data1: SyriacData, _data2: SyriacData) -> SyriacData {
        // TODO hold off for future Unicode normalisation changes
        data1
    }
}

// Syriac glyphs are represented as `RawGlyph` structs with `SyriacData` for its `extra_data`.
type SyriacGlyph = RawGlyph<SyriacData>;

impl SyriacGlyph {
    fn is_alaph(&self) -> bool {
        self.extra_data.joining_group == JoiningGroup::Alaph
    }

    fn is_dalath_rish(&self) -> bool {
        self.extra_data.joining_group == JoiningGroup::DalathRish
    }

    fn is_transparent(&self) -> bool {
        self.extra_data.joining_type == JoiningType::Transparent || self.multi_subst_dup()
    }

    fn is_non_joining(&self) -> bool {
        self.extra_data.joining_type == JoiningType::NonJoining
    }

    fn is_left_joining(&self) -> bool {
        self.extra_data.joining_type == JoiningType::LeftJoining
            || self.extra_data.joining_type == JoiningType::DualJoining
            || self.extra_data.joining_type == JoiningType::JoinCausing
    }

    fn is_right_joining(&self) -> bool {
        self.extra_data.joining_type == JoiningType::RightJoining
            || self.extra_data.joining_type == JoiningType::DualJoining
            || self.extra_data.joining_type == JoiningType::JoinCausing
    }

    fn feature_tag(&self) -> u32 {
        self.extra_data.feature_tag
    }

    fn set_feature_tag(&mut self, feature_tag: u32) {
        self.extra_data.feature_tag = feature_tag
    }
}

impl From<&RawGlyph<()>> for SyriacGlyph {
    fn from(raw_glyph: &RawGlyph<()>) -> SyriacGlyph {
        // Since there's no `Char` to work out the `SyriacGlyph`s joining type when the glyph's
        // `glyph_origin` is `GlyphOrigin::Direct`, we fallback to `JoiningType::NonJoining` as
        // the safest approach
        let joining_type = match raw_glyph.glyph_origin {
            GlyphOrigin::Char(c) => get_joining_type(c),
            GlyphOrigin::Direct => JoiningType::NonJoining,
        };

        // As above, we'll fallback onto `JoiningType::NoJoiningGroup`
        let joining_group = match raw_glyph.glyph_origin {
            GlyphOrigin::Char(c) => get_joining_group(c),
            GlyphOrigin::Direct => JoiningGroup::NoJoiningGroup,
        };

        SyriacGlyph {
            unicodes: raw_glyph.unicodes.clone(),
            glyph_index: raw_glyph.glyph_index,
            liga_component_pos: raw_glyph.liga_component_pos,
            glyph_origin: raw_glyph.glyph_origin,
            flags: raw_glyph.flags,
            variation: raw_glyph.variation,
            extra_data: SyriacData {
                joining_group,
                joining_type,
                // For convenience, we loosely follow the spec (`2. Computing letter joining
                // states`) here by initialising all `SyriacGlyph`s to `tag::ISOL`
                feature_tag: tag::ISOL,
            },
        }
    }
}

impl From<&SyriacGlyph> for RawGlyph<()> {
    fn from(syriac_glyph: &SyriacGlyph) -> RawGlyph<()> {
        RawGlyph {
            unicodes: syriac_glyph.unicodes.clone(),
            glyph_index: syriac_glyph.glyph_index,
            liga_component_pos: syriac_glyph.liga_component_pos,
            glyph_origin: syriac_glyph.glyph_origin,
            flags: syriac_glyph.flags,
            variation: syriac_glyph.variation,
            extra_data: (),
        }
    }
}

pub fn gsub_apply_syriac(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    raw_glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    match gsub_table.find_script(script_tag)? {
        Some(s) => {
            if s.find_langsys_or_default(lang_tag)?.is_none() {
                return Ok(());
            }
        }
        None => return Ok(()),
    }

    let syriac_glyphs: &mut Vec<SyriacGlyph> =
        &mut raw_glyphs.iter().map(SyriacGlyph::from).collect();

    // 1. Compound character composition and decomposition

    apply_lookups(
        FeatureMask::CCMP,
        gsub_cache,
        gsub_table,
        gdef_table,
        script_tag,
        lang_tag,
        feature_variations,
        syriac_glyphs,
        max_glyphs,
        |_, _| true,
    )?;

    // 2. Computing letter joining states

    {
        let mut previous_i = syriac_glyphs
            .iter()
            .position(|g| !g.is_transparent())
            .unwrap_or(0);

        for i in (previous_i + 1)..syriac_glyphs.len() {
            if syriac_glyphs[i].is_transparent() {
                continue;
            }

            if syriac_glyphs[previous_i].is_left_joining() && syriac_glyphs[i].is_right_joining() {
                if syriac_glyphs[i].is_alaph() {
                    syriac_glyphs[i].set_feature_tag(tag::MED2)
                } else {
                    syriac_glyphs[i].set_feature_tag(tag::FINA)
                };

                match syriac_glyphs[previous_i].feature_tag() {
                    tag::ISOL => syriac_glyphs[previous_i].set_feature_tag(tag::INIT),
                    tag::FINA => syriac_glyphs[previous_i].set_feature_tag(tag::MEDI),
                    _ => {}
                }
            }

            previous_i = i;
        }

        let last_i = syriac_glyphs
            .iter()
            .rposition(|g| !(g.is_transparent() || g.is_non_joining()))
            .unwrap_or(0);

        if last_i != 0 && syriac_glyphs[last_i].is_alaph() {
            let previous_i = last_i - 1;

            if syriac_glyphs[previous_i].is_left_joining() {
                syriac_glyphs[last_i].set_feature_tag(tag::FINA)
            } else if syriac_glyphs[previous_i].is_dalath_rish() {
                syriac_glyphs[last_i].set_feature_tag(tag::FIN3)
            } else {
                syriac_glyphs[last_i].set_feature_tag(tag::FIN2)
            }
        }
    }

    // 3. Applying the stch feature
    //
    // TODO hold off for future generalised solution (including Kashidas)

    // 4. Applying the language-form substitution features from GSUB

    const LANGUAGE_FEATURES: &[(FeatureMask, bool)] = &[
        (FeatureMask::LOCL, true),
        (FeatureMask::ISOL, false),
        (FeatureMask::FINA, false),
        (FeatureMask::FIN2, false),
        (FeatureMask::FIN3, false),
        (FeatureMask::MEDI, false),
        (FeatureMask::MED2, false),
        (FeatureMask::INIT, false),
        (FeatureMask::RLIG, true),
        (FeatureMask::CALT, true),
    ];

    for &(feature_mask, is_global) in LANGUAGE_FEATURES {
        apply_lookups(
            feature_mask,
            gsub_cache,
            gsub_table,
            gdef_table,
            script_tag,
            lang_tag,
            feature_variations,
            syriac_glyphs,
            max_glyphs,
            |g, feature_tag| is_global || g.feature_tag() == feature_tag,
        )?;
    }

    // 5. Applying the typographic-form substitution features from GSUB to all glyphs
    //
    // Note that we skip `GSUB`'s `DLIG` feature as it should be off by default

    apply_lookups(
        FeatureMask::LIGA,
        gsub_cache,
        gsub_table,
        gdef_table,
        script_tag,
        lang_tag,
        feature_variations,
        syriac_glyphs,
        max_glyphs,
        |_, _| true,
    )?;

    // 6. Mark reordering
    //
    // TODO hold off for future Unicode normalisation changes

    *raw_glyphs = syriac_glyphs.iter().map(RawGlyph::from).collect();

    Ok(())
}

fn apply_lookups(
    feature_mask: FeatureMask,
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    syriac_glyphs: &mut Vec<RawGlyph<SyriacData>>,
    max_glyphs: usize,
    pred: impl Fn(&RawGlyph<SyriacData>, u32) -> bool + Copy,
) -> Result<(), ParseError> {
    let index = gsub::get_lookups_cache_index(
        gsub_cache,
        script_tag,
        lang_tag,
        feature_variations,
        feature_mask,
    )?;
    let lookups = &gsub_cache.cached_lookups.lock().unwrap()[index];

    for &(lookup_index, feature_tag) in lookups {
        gsub::gsub_apply_lookup(
            gsub_cache,
            gsub_table,
            gdef_table,
            lookup_index,
            feature_tag,
            None,
            syriac_glyphs,
            max_glyphs,
            0,
            syriac_glyphs.len(),
            |g| pred(g, feature_tag),
        )?;
    }

    Ok(())
}
