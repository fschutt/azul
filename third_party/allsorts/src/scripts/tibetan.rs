//! Implementation of font shaping for the Tibetan script.
//!
//! Tibetan doesn't need syllable identification or reordering — subjoined consonants have
//! separate Unicode codepoints (U+0F90–U+0FBC). Preprocessing decomposes deprecated compound
//! vowels and sorts marks by modified combining class.

use crate::error::ShapingError;
use crate::gsub::{self, Feature, FeatureMask, RawGlyph};
use crate::layout::{FeatureTableSubstitution, GDEFTable, LayoutCache, LayoutTable, GSUB};
use crate::unicode::mcc::sort_by_modified_combining_class;

/// Decompose deprecated Tibetan compound vowels, then sort marks by modified combining class.
pub fn preprocess_tibetan(cs: &mut Vec<char>) {
    decompose_tibetan(cs);
    sort_by_modified_combining_class(cs);
}

/// Decompose deprecated Tibetan compound vowels into their constituent parts.
///
/// Reference: Unicode Standard, Chapter 13.4 (Tibetan), Table 13-3.
fn decompose_tibetan(cs: &mut Vec<char>) {
    let mut i = 0;
    while i < cs.len() {
        match cs[i] {
            // U+0F73 TIBETAN VOWEL SIGN II → U+0F71 + U+0F72
            '\u{0F73}' => {
                cs[i] = '\u{0F71}';
                i += 1;
                cs.insert(i, '\u{0F72}');
            }
            // U+0F75 TIBETAN VOWEL SIGN UU → U+0F71 + U+0F74
            '\u{0F75}' => {
                cs[i] = '\u{0F71}';
                i += 1;
                cs.insert(i, '\u{0F74}');
            }
            // U+0F77 TIBETAN VOWEL SIGN VOCALIC RR → U+0FB2 + U+0F71 + U+0F80
            '\u{0F77}' => {
                cs[i] = '\u{0FB2}';
                i += 1;
                cs.insert(i, '\u{0F71}');
                i += 1;
                cs.insert(i, '\u{0F80}');
            }
            // U+0F79 TIBETAN VOWEL SIGN VOCALIC LL → U+0FB3 + U+0F71 + U+0F80
            '\u{0F79}' => {
                cs[i] = '\u{0FB3}';
                i += 1;
                cs.insert(i, '\u{0F71}');
                i += 1;
                cs.insert(i, '\u{0F80}');
            }
            // U+0F81 TIBETAN VOWEL SIGN REVERSED II → U+0F71 + U+0F80
            '\u{0F81}' => {
                cs[i] = '\u{0F71}';
                i += 1;
                cs.insert(i, '\u{0F80}');
            }
            _ => {}
        }
        i += 1;
    }
}

fn apply_features(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    features: FeatureMask,
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    let index = gsub::get_lookups_cache_index(
        gsub_cache,
        script_tag,
        lang_tag,
        feature_variations,
        features,
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
            glyphs,
            max_glyphs,
            0,
            glyphs.len(),
            |_| true,
        )?;
    }
    Ok(())
}

pub fn gsub_apply_tibetan(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    extra_features: FeatureMask,
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    // Stage 1: Language forms
    let stage1 = Feature::LOCL | Feature::CCMP;
    apply_features(
        gsub_cache,
        gsub_table,
        gdef_table,
        script_tag,
        lang_tag,
        feature_variations,
        stage1,
        glyphs,
        max_glyphs,
    )?;

    // Stage 2: Conjuncts and typographical forms
    let stage2 = Feature::ABVS | Feature::BLWS | Feature::CALT | Feature::LIGA | extra_features;
    apply_features(
        gsub_cache,
        gsub_table,
        gdef_table,
        script_tag,
        lang_tag,
        feature_variations,
        stage2,
        glyphs,
        max_glyphs,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompose_0f73() {
        let mut cs = vec!['\u{0F40}', '\u{0F73}'];
        decompose_tibetan(&mut cs);
        assert_eq!(cs, vec!['\u{0F40}', '\u{0F71}', '\u{0F72}']);
    }

    #[test]
    fn test_decompose_0f75() {
        let mut cs = vec!['\u{0F40}', '\u{0F75}'];
        decompose_tibetan(&mut cs);
        assert_eq!(cs, vec!['\u{0F40}', '\u{0F71}', '\u{0F74}']);
    }

    #[test]
    fn test_decompose_0f77() {
        let mut cs = vec!['\u{0F40}', '\u{0F77}'];
        decompose_tibetan(&mut cs);
        assert_eq!(cs, vec!['\u{0F40}', '\u{0FB2}', '\u{0F71}', '\u{0F80}']);
    }

    #[test]
    fn test_decompose_0f79() {
        let mut cs = vec!['\u{0F40}', '\u{0F79}'];
        decompose_tibetan(&mut cs);
        assert_eq!(cs, vec!['\u{0F40}', '\u{0FB3}', '\u{0F71}', '\u{0F80}']);
    }

    #[test]
    fn test_decompose_0f81() {
        let mut cs = vec!['\u{0F40}', '\u{0F81}'];
        decompose_tibetan(&mut cs);
        assert_eq!(cs, vec!['\u{0F40}', '\u{0F71}', '\u{0F80}']);
    }

    #[test]
    fn test_no_decompose() {
        let mut cs = vec!['\u{0F40}', '\u{0F72}'];
        let expected = cs.clone();
        decompose_tibetan(&mut cs);
        assert_eq!(cs, expected);
    }

    #[test]
    fn test_multiple_decompositions() {
        let mut cs = vec!['\u{0F40}', '\u{0F73}', '\u{0F51}', '\u{0F75}'];
        decompose_tibetan(&mut cs);
        assert_eq!(
            cs,
            vec!['\u{0F40}', '\u{0F71}', '\u{0F72}', '\u{0F51}', '\u{0F71}', '\u{0F74}']
        );
    }
}
