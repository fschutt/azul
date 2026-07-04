//! Implementation of font shaping for Arabic scripts
//!
//! Code herein follows the specification at:
//! <https://github.com/n8willis/opentype-shaping-documents/blob/master/opentype-shaping-arabic-general.md>

use unicode_joining_type::{get_joining_type, JoiningType};

use crate::error::{ParseError, ShapingError};
use crate::gsub::{self, FeatureMask, GlyphData, GlyphOrigin, RawGlyph};
use crate::layout::{FeatureTableSubstitution, GDEFTable, LayoutCache, LayoutTable, GSUB};
use crate::tag;
use crate::unicode::mcc::{
    modified_combining_class, sort_by_modified_combining_class, ModifiedCombiningClass,
};

#[derive(Clone)]
struct ArabicData {
    joining_type: JoiningType,
    feature_tag: u32,
}

impl GlyphData for ArabicData {
    fn merge(data1: ArabicData, _data2: ArabicData) -> ArabicData {
        // TODO hold off for future Unicode normalisation changes
        data1
    }
}

// Arabic glyphs are represented as `RawGlyph` structs with `ArabicData` for its `extra_data`.
type ArabicGlyph = RawGlyph<ArabicData>;

impl ArabicGlyph {
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

impl From<&RawGlyph<()>> for ArabicGlyph {
    fn from(raw_glyph: &RawGlyph<()>) -> ArabicGlyph {
        // Since there's no `Char` to work out the `ArabicGlyph`s joining type when the glyph's
        // `glyph_origin` is `GlyphOrigin::Direct`, we fallback to `JoiningType::NonJoining` as
        // the safest approach
        let joining_type = match raw_glyph.glyph_origin {
            GlyphOrigin::Char(c) => get_joining_type(c),
            GlyphOrigin::Direct => JoiningType::NonJoining,
        };

        ArabicGlyph {
            unicodes: raw_glyph.unicodes.clone(),
            glyph_index: raw_glyph.glyph_index,
            liga_component_pos: raw_glyph.liga_component_pos,
            glyph_origin: raw_glyph.glyph_origin,
            flags: raw_glyph.flags,
            variation: raw_glyph.variation,
            extra_data: ArabicData {
                joining_type,
                // For convenience, we loosely follow the spec (`2. Computing letter joining
                // states`) here by initialising all `ArabicGlyph`s to `tag::ISOL`
                feature_tag: tag::ISOL,
            },
        }
    }
}

impl From<&ArabicGlyph> for RawGlyph<()> {
    fn from(arabic_glyph: &ArabicGlyph) -> RawGlyph<()> {
        RawGlyph {
            unicodes: arabic_glyph.unicodes.clone(),
            glyph_index: arabic_glyph.glyph_index,
            liga_component_pos: arabic_glyph.liga_component_pos,
            glyph_origin: arabic_glyph.glyph_origin,
            flags: arabic_glyph.flags,
            variation: arabic_glyph.variation,
            extra_data: (),
        }
    }
}

pub fn gsub_apply_arabic(
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

    let arabic_glyphs = &mut raw_glyphs.iter().map(ArabicGlyph::from).collect();

    // 1. Compound character composition and decomposition

    apply_lookups(
        FeatureMask::CCMP,
        gsub_cache,
        gsub_table,
        gdef_table,
        script_tag,
        lang_tag,
        feature_variations,
        arabic_glyphs,
        max_glyphs,
        |_, _| true,
    )?;

    // 2. Computing letter joining states

    {
        let mut previous_i = arabic_glyphs
            .iter()
            .position(|g| !g.is_transparent())
            .unwrap_or(0);

        for i in (previous_i + 1)..arabic_glyphs.len() {
            if arabic_glyphs[i].is_transparent() {
                continue;
            }

            if arabic_glyphs[previous_i].is_left_joining() && arabic_glyphs[i].is_right_joining() {
                arabic_glyphs[i].set_feature_tag(tag::FINA);

                match arabic_glyphs[previous_i].feature_tag() {
                    tag::ISOL => arabic_glyphs[previous_i].set_feature_tag(tag::INIT),
                    tag::FINA => arabic_glyphs[previous_i].set_feature_tag(tag::MEDI),
                    _ => {}
                }
            }

            previous_i = i;
        }
    }

    // 3. Applying the stch feature
    //
    // TODO hold off for future generalised solution (including the Syriac Abbreviation Mark)

    // 4. Applying the language-form substitution features from GSUB

    const LANGUAGE_FEATURES: &[(FeatureMask, bool)] = &[
        (FeatureMask::LOCL, true),
        (FeatureMask::ISOL, false),
        (FeatureMask::FINA, false),
        (FeatureMask::MEDI, false),
        (FeatureMask::INIT, false),
        (FeatureMask::RLIG, true),
        (FeatureMask::RCLT, true),
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
            arabic_glyphs,
            max_glyphs,
            |g, feature_tag| is_global || g.feature_tag() == feature_tag,
        )?;
    }

    // 5. Applying the typographic-form substitution features from GSUB
    //
    // Note that we skip `GSUB`'s `DLIG` and `CSWH` features as results would differ from other
    // Arabic shapers

    const TYPOGRAPHIC_FEATURES: &[FeatureMask] = &[FeatureMask::LIGA, FeatureMask::MSET];

    for &feature_mask in TYPOGRAPHIC_FEATURES {
        apply_lookups(
            feature_mask,
            gsub_cache,
            gsub_table,
            gdef_table,
            script_tag,
            lang_tag,
            feature_variations,
            arabic_glyphs,
            max_glyphs,
            |_, _| true,
        )?;
    }

    // 6. Mark reordering
    //
    // Handled in the text preprocessing stage.

    *raw_glyphs = arabic_glyphs.iter().map(RawGlyph::from).collect();

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
    arabic_glyphs: &mut Vec<ArabicGlyph>,
    max_glyphs: usize,
    pred: impl Fn(&ArabicGlyph, u32) -> bool + Copy,
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
            arabic_glyphs,
            max_glyphs,
            0,
            arabic_glyphs.len(),
            |g| pred(g, feature_tag),
        )?;
    }

    Ok(())
}

/// Reorder Arabic marks per AMTRA. See: <https://www.unicode.org/reports/tr53/>.
pub(super) fn reorder_marks(cs: &mut [char]) {
    sort_by_modified_combining_class(cs);

    for css in
        cs.split_mut(|&c| modified_combining_class(c) == ModifiedCombiningClass::NotReordered)
    {
        reorder_marks_shadda(css);
        reorder_marks_other_combining(css, ModifiedCombiningClass::Above);
        reorder_marks_other_combining(css, ModifiedCombiningClass::Below);
    }
}

fn reorder_marks_shadda(cs: &mut [char]) {
    use std::cmp::Ordering;

    // 2a. Move any Shadda characters to the beginning of S, where S is a max
    // length substring of non-starter characters.
    fn comparator(c1: &char, _c2: &char) -> Ordering {
        if modified_combining_class(*c1) == ModifiedCombiningClass::CCC33 {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }
    cs.sort_by(comparator)
}

fn reorder_marks_other_combining(cs: &mut [char], mcc: ModifiedCombiningClass) {
    debug_assert!(mcc == ModifiedCombiningClass::Below || mcc == ModifiedCombiningClass::Above);

    // Get the start index of a possible sequence of characters with canonical
    // combining class equal to `mcc`. (Assumes that `glyphs` is normalised to
    // NFD.)
    let first = cs.iter().position(|&c| modified_combining_class(c) == mcc);

    if let Some(first) = first {
        // 2b/2c. If the sequence of characters _begins_ with any MCM characters,
        // move the sequence of such characters to the beginning of S.
        let count = cs[first..]
            .iter()
            .take_while(|&&c| is_modifier_combining_mark(c))
            .count();
        cs[..(first + count)].rotate_right(count);
    }
}

fn is_modifier_combining_mark(ch: char) -> bool {
    // https://www.unicode.org/reports/tr53/tr53-6.html#MCM
    match ch {
        | '\u{0654}' // ARABIC HAMZA ABOVE
        | '\u{0655}' // ARABIC HAMZA BELOW
        | '\u{0658}' // ARABIC MARK NOON GHUNNA
        | '\u{06DC}' // ARABIC SMALL HIGH SEEN
        | '\u{06E3}' // ARABIC SMALL LOW SEEN
        | '\u{06E7}' // ARABIC SMALL HIGH YEH
        | '\u{06E8}' // ARABIC SMALL HIGH NOON
        | '\u{08CA}' // ARABIC SMALL HIGH FARSI YEH
        | '\u{08CB}' // ARABIC SMALL HIGH YEH BARREE WITH TWO DOTS BELOW
        | '\u{08CD}' // ARABIC SMALL HIGH ZAH
        | '\u{08CE}' // ARABIC LARGE ROUND DOT ABOVE
        | '\u{08CF}' // ARABIC LARGE ROUND DOT BELOW
        | '\u{08D3}' // ARABIC SMALL LOW WAW
        | '\u{08F3}' => true, // ARABIC SMALL HIGH WAW
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // https://www.unicode.org/reports/tr53/#Demonstrating_AMTRA.
    mod reorder_marks {
        use super::*;

        #[test]
        fn test_artificial() {
            let cs = vec![
                '\u{0618}', '\u{0619}', '\u{064E}', '\u{064F}', '\u{0654}', '\u{0658}', '\u{0653}',
                '\u{0654}', '\u{0651}', '\u{0656}', '\u{0651}', '\u{065C}', '\u{0655}', '\u{0650}',
            ];
            let cs_exp = vec![
                '\u{0654}', '\u{0658}', '\u{0651}', '\u{0651}', '\u{0618}', '\u{064E}', '\u{0619}',
                '\u{064F}', '\u{0650}', '\u{0656}', '\u{065C}', '\u{0655}', '\u{0653}', '\u{0654}',
            ];
            test_reorder_marks(&cs, &cs_exp);
        }

        // Variant of `test_artificial` where U+0656 is replaced with U+0655
        // to test the reordering of MCM characters for the ccc = 220 group.
        #[test]
        fn test_artificial_custom() {
            let cs = vec![
                '\u{0618}', '\u{0619}', '\u{064E}', '\u{064F}', '\u{0654}', '\u{0658}', '\u{0653}',
                '\u{0654}', '\u{0651}', '\u{0655}', '\u{0651}', '\u{065C}', '\u{0655}', '\u{0650}',
            ];
            let cs_exp = vec![
                '\u{0655}', '\u{0654}', '\u{0658}', '\u{0651}', '\u{0651}', '\u{0618}', '\u{064E}',
                '\u{0619}', '\u{064F}', '\u{0650}', '\u{065C}', '\u{0655}', '\u{0653}', '\u{0654}',
            ];
            test_reorder_marks(&cs, &cs_exp);
        }

        #[test]
        fn test_example1() {
            let cs1 = vec!['\u{0627}', '\u{064F}', '\u{0654}'];
            let cs1_exp = vec!['\u{0627}', '\u{0654}', '\u{064F}'];
            test_reorder_marks(&cs1, &cs1_exp);

            let cs2 = vec!['\u{0627}', '\u{064F}', '\u{034F}', '\u{0654}'];
            test_reorder_marks(&cs2, &cs2);

            let cs3 = vec!['\u{0649}', '\u{0650}', '\u{0655}'];
            let cs3_exp = vec!['\u{0649}', '\u{0655}', '\u{0650}'];
            test_reorder_marks(&cs3, &cs3_exp);

            let cs4 = vec!['\u{0649}', '\u{0650}', '\u{034F}', '\u{0655}'];
            test_reorder_marks(&cs4, &cs4);
        }

        #[test]
        fn test_example2a() {
            let cs = vec!['\u{0635}', '\u{06DC}', '\u{0652}'];
            test_reorder_marks(&cs, &cs);
        }

        #[test]
        fn test_example2b() {
            let cs1 = vec!['\u{0647}', '\u{0652}', '\u{06DC}'];
            let cs1_exp = vec!['\u{0647}', '\u{06DC}', '\u{0652}'];
            test_reorder_marks(&cs1, &cs1_exp);

            let cs2 = vec!['\u{0647}', '\u{0652}', '\u{034F}', '\u{06DC}'];
            test_reorder_marks(&cs2, &cs2);
        }

        #[test]
        fn test_example3() {
            let cs1 = vec!['\u{0640}', '\u{0650}', '\u{0651}', '\u{06E7}'];
            // The expected output in https://www.unicode.org/reports/tr53/#Example3
            //
            // [U+0640, U+0650, U+06E7, U+0651]
            //
            // is incorrect, in that it fails to account for U+0651 Shadda moving to
            // the front of U+0650 Kasra, per step 2a of AMTRA.
            //
            // U+06E7 Small High Yeh should then move to the front of Shadda per step
            // 2b, resulting in:
            let cs1_exp = vec!['\u{0640}', '\u{06E7}', '\u{0651}', '\u{0650}'];
            test_reorder_marks(&cs1, &cs1_exp);

            let cs2 = vec!['\u{0640}', '\u{0650}', '\u{0651}', '\u{034F}', '\u{06E7}'];
            // As above, Shadda should move to the front of Kasra, so the expected
            // output in https://www.unicode.org/reports/tr53/#Example3
            //
            // [U+0640, U+0650, U+0651, U+034F, U+06E7]
            //
            // (i.e. no changes) is also incorrect.
            let cs2_exp = vec!['\u{0640}', '\u{0651}', '\u{0650}', '\u{034F}', '\u{06E7}'];
            test_reorder_marks(&cs2, &cs2_exp);
        }

        #[test]
        fn test_example4a() {
            let cs = vec!['\u{0640}', '\u{0652}', '\u{034F}', '\u{06E8}'];
            test_reorder_marks(&cs, &cs);
        }

        #[test]
        fn test_example4b() {
            let cs1 = vec!['\u{06C6}', '\u{064F}', '\u{06E8}'];
            let cs1_exp = vec!['\u{06C6}', '\u{06E8}', '\u{064F}'];
            test_reorder_marks(&cs1, &cs1_exp);

            let cs2 = vec!['\u{06C6}', '\u{064F}', '\u{034F}', '\u{06E8}'];
            test_reorder_marks(&cs2, &cs2);
        }

        fn test_reorder_marks(cs: &Vec<char>, cs_exp: &Vec<char>) {
            let mut cs_act = cs.clone();
            reorder_marks(&mut cs_act);
            assert_eq!(cs_exp, &cs_act);
        }
    }
}
