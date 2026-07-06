//! Implementation of font shaping for Mongolian script
//!
//! Code herein follows the specification at:
//! <https://github.com/n8willis/opentype-shaping-documents/blob/master/opentype-shaping-mongolian.md>

use unicode_joining_type::{get_joining_type, JoiningType};

use crate::error::{ParseError, ShapingError};
use crate::gsub::{self, Feature, FeatureMask, GlyphData, GlyphOrigin, RawGlyph};
use crate::layout::{FeatureTableSubstitution, GDEFTable, LayoutCache, LayoutTable, GSUB};
use crate::tag;

#[derive(Clone)]
struct MongolianData {
    joining_type: JoiningType,
    feature_tag: u32,
}

impl GlyphData for MongolianData {
    fn merge(data1: MongolianData, _data2: MongolianData) -> MongolianData {
        data1
    }
}

type MongolianGlyph = RawGlyph<MongolianData>;

impl MongolianGlyph {
    fn is_transparent(&self) -> bool {
        self.extra_data.joining_type == JoiningType::Transparent || self.multi_subst_dup()
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

impl From<&RawGlyph<()>> for MongolianGlyph {
    fn from(raw_glyph: &RawGlyph<()>) -> MongolianGlyph {
        let joining_type = match raw_glyph.glyph_origin {
            GlyphOrigin::Char(c) => get_joining_type(c),
            GlyphOrigin::Direct => JoiningType::NonJoining,
        };

        MongolianGlyph {
            unicodes: raw_glyph.unicodes.clone(),
            glyph_index: raw_glyph.glyph_index,
            liga_component_pos: raw_glyph.liga_component_pos,
            glyph_origin: raw_glyph.glyph_origin,
            flags: raw_glyph.flags,
            variation: raw_glyph.variation,
            extra_data: MongolianData {
                joining_type,
                feature_tag: tag::ISOL,
            },
        }
    }
}

impl From<&MongolianGlyph> for RawGlyph<()> {
    fn from(mongolian_glyph: &MongolianGlyph) -> RawGlyph<()> {
        RawGlyph {
            unicodes: mongolian_glyph.unicodes.clone(),
            glyph_index: mongolian_glyph.glyph_index,
            liga_component_pos: mongolian_glyph.liga_component_pos,
            glyph_origin: mongolian_glyph.glyph_origin,
            flags: mongolian_glyph.flags,
            variation: mongolian_glyph.variation,
            extra_data: (),
        }
    }
}

pub fn gsub_apply_mongolian(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    extra_features: FeatureMask,
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

    let mongolian_glyphs = &mut raw_glyphs.iter().map(MongolianGlyph::from).collect();

    // 1. Compound character composition and decomposition

    apply_lookups(
        Feature::CCMP.mask(),
        gsub_cache,
        gsub_table,
        gdef_table,
        script_tag,
        lang_tag,
        feature_variations,
        mongolian_glyphs,
        max_glyphs,
        |_, _| true,
    )?;

    // 2. Computing letter joining states

    {
        let mut previous_i = mongolian_glyphs
            .iter()
            .position(|g| !g.is_transparent())
            .unwrap_or(0);

        for i in (previous_i + 1)..mongolian_glyphs.len() {
            if mongolian_glyphs[i].is_transparent() {
                continue;
            }

            if mongolian_glyphs[previous_i].is_left_joining()
                && mongolian_glyphs[i].is_right_joining()
            {
                mongolian_glyphs[i].set_feature_tag(tag::FINA);

                match mongolian_glyphs[previous_i].feature_tag() {
                    tag::ISOL => mongolian_glyphs[previous_i].set_feature_tag(tag::INIT),
                    tag::FINA => mongolian_glyphs[previous_i].set_feature_tag(tag::MEDI),
                    _ => {}
                }
            }

            previous_i = i;
        }
    }

    // 3. Applying the stch feature
    //
    // TODO hold off for future generalised solution

    // 4. Applying the language-form substitution features from GSUB

    const LANGUAGE_FEATURES: &[(Feature, bool)] = &[
        (Feature::LOCL, true),
        (Feature::ISOL, false),
        (Feature::FINA, false),
        (Feature::MEDI, false),
        (Feature::INIT, false),
        (Feature::RLIG, true),
        (Feature::RCLT, true),
        (Feature::CALT, true),
    ];

    for &(feature, is_global) in LANGUAGE_FEATURES {
        apply_lookups(
            feature.mask(),
            gsub_cache,
            gsub_table,
            gdef_table,
            script_tag,
            lang_tag,
            feature_variations,
            mongolian_glyphs,
            max_glyphs,
            |g, feature_tag| is_global || g.feature_tag() == feature_tag,
        )?;
    }

    // 5. Applying the typographic-form substitution features from GSUB

    let typographic_features = Feature::LIGA | Feature::MSET;

    apply_lookups(
        typographic_features | extra_features,
        gsub_cache,
        gsub_table,
        gdef_table,
        script_tag,
        lang_tag,
        feature_variations,
        mongolian_glyphs,
        max_glyphs,
        |_, _| true,
    )?;

    // 6. Mark reordering
    //
    // Handled in the text preprocessing stage.

    *raw_glyphs = mongolian_glyphs.iter().map(RawGlyph::from).collect();

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
    mongolian_glyphs: &mut Vec<MongolianGlyph>,
    max_glyphs: usize,
    pred: impl Fn(&MongolianGlyph, u32) -> bool + Copy,
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
            mongolian_glyphs,
            max_glyphs,
            0,
            mongolian_glyphs.len(),
            |g| pred(g, feature_tag),
        )?;
    }

    Ok(())
}
