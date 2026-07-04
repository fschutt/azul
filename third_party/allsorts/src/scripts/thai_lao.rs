//! Implementation of font shaping for Thai and Lao scripts, following the specification at:
//! <https://github.com/n8willis/opentype-shaping-documents/blob/master/opentype-shaping-thai-lao.md>.

use crate::error::ShapingError;
use crate::gsub::{self, FeatureMask, RawGlyph};
use crate::layout::{FeatureTableSubstitution, GDEFTable, LayoutCache, LayoutTable, GSUB};
use crate::unicode::mcc::sort_by_modified_combining_class;

pub(super) fn reorder_marks(cs: &mut Vec<char>) {
    // U+0E4D THAI NIKHAHIT and U+0ECD LAO NIGGAHITA marks that originate from an AM vowel
    // must be reordered before any tone markers. NIKHAHIT and NIGGAHITA marks that do not
    // originate from an AM vowel should not be reordered.
    //
    // Reordering may not just be limited to tone markers, but all abovebase marks:
    // https://github.com/n8willis/opentype-shaping-documents/issues/125.
    for i in 0..cs.len() {
        if let Some((c1, c2)) = split_am_vowel(cs[i]) {
            cs[i] = c1;
            cs.insert(i + 1, c2);

            let mut j = i;
            while j > 0 && is_abovebase_mark(cs[j - 1]) {
                j -= 1;
            }
            cs[j..=i].rotate_right(1);
        }
    }

    // U+0E3A PHINTHU is reordered so it occurs after any U+0E38 SARA U or U+0E39 SARA UU marks.
    // This is done by mapping the Thai combining class `CCC103` to `CCC3`, see the `unicode::mcc`
    // module.
    sort_by_modified_combining_class(cs);
}

fn split_am_vowel(c: char) -> Option<(char, char)> {
    match c {
        // Thai
        '\u{0E33}' => Some(('\u{0E4D}', '\u{0E32}')),
        // Lao
        '\u{0EB3}' => Some(('\u{0ECD}', '\u{0EB2}')),
        _ => None,
    }
}

fn is_abovebase_mark(c: char) -> bool {
    match c {
        // Thai
        '\u{0E31}' => true,
        '\u{0E34}'..='\u{0E37}' => true,
        '\u{0E47}'..='\u{0E4E}' => true,
        // Lao
        '\u{0EB1}' => true,
        '\u{0EB4}'..='\u{0EB7}' => true,
        '\u{0EBB}' => true,
        '\u{0EC8}'..='\u{0ECD}' => true,
        _ => false,
    }
}

pub fn gsub_apply_thai_lao(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    glyphs: &mut Vec<RawGlyph<()>>,
    max_glyphs: usize,
) -> Result<(), ShapingError> {
    let index = gsub::get_lookups_cache_index(
        gsub_cache,
        script_tag,
        lang_tag,
        feature_variations,
        FeatureMask::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    mod reorder_marks {
        use super::*;

        #[test]
        fn test_am1() {
            let mut cs = vec!['\u{0E33}'];
            let cs_exp = vec!['\u{0E4D}', '\u{0E32}'];
            reorder_marks(&mut cs);
            assert_eq!(cs_exp, cs);
        }

        #[test]
        fn test_am2() {
            let mut cs = vec!['\u{0E49}', '\u{0E33}'];
            let cs_exp = vec!['\u{0E4D}', '\u{0E49}', '\u{0E32}'];
            reorder_marks(&mut cs);
            assert_eq!(cs_exp, cs);
        }

        #[test]
        fn test_am3() {
            let mut cs = vec!['\u{0E49}', '\u{0E4D}', '\u{0E32}'];
            let cs_exp = cs.clone();
            reorder_marks(&mut cs);
            assert_eq!(cs_exp, cs);
        }

        #[test]
        fn test_am4() {
            let mut cs = vec!['\u{0E19}', '\u{0E49}', '\u{0E19}', '\u{0E49}', '\u{0E33}'];
            let cs_exp = vec![
                '\u{0E19}', '\u{0E49}', '\u{0E19}', '\u{0E4D}', '\u{0E49}', '\u{0E32}',
            ];
            reorder_marks(&mut cs);
            assert_eq!(cs_exp, cs);
        }

        #[test]
        fn test_am5() {
            let mut cs = vec![
                '\u{0E19}', '\u{0E49}', '\u{0E19}', '\u{0E49}', '\u{0E4D}', '\u{0E32}',
            ];
            let cs_exp = cs.clone();
            reorder_marks(&mut cs);
            assert_eq!(cs_exp, cs);
        }

        #[test]
        fn test_phinthu1() {
            let mut cs = vec!['\u{0E19}', '\u{0E38}', '\u{0E3A}'];
            let cs_exp = cs.clone();
            reorder_marks(&mut cs);
            assert_eq!(cs_exp, cs);
        }

        #[test]
        fn test_phinthu2() {
            let mut cs = vec!['\u{0E19}', '\u{0E3A}', '\u{0E38}'];
            let cs_exp = vec!['\u{0E19}', '\u{0E38}', '\u{0E3A}'];
            reorder_marks(&mut cs);
            assert_eq!(cs_exp, cs);
        }
    }
}
