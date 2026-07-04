use tinyvec::tiny_vec;

use crate::error::{ParseError, ShapingError};
use crate::gsub::{self, FeatureMask, GlyphData, GlyphOrigin, RawGlyph, RawGlyphFlags};
use crate::layout::{FeatureTableSubstitution, GDEFTable, LayoutCache, LayoutTable, GSUB};
use crate::scripts::syllable::*;
use crate::tag;
use crate::unicode::mcc::sort_by_modified_combining_class;
use crate::DOTTED_CIRCLE;

fn shaping_class(c: char) -> Option<ShapingClass> {
    khmer_character(c).0
}

fn mark_placement(c: char) -> Option<MarkPlacementSubclass> {
    khmer_character(c).1
}

fn ra(c: char) -> bool {
    c == '\u{179A}'
}

fn consonant(c: char) -> bool {
    match shaping_class(c) {
        Some(ShapingClass::Consonant) => !ra(c),
        _ => false,
    }
}

fn vowel(c: char) -> bool {
    shaping_class(c) == Some(ShapingClass::VowelIndependent)
}

fn nukta(c: char) -> bool {
    match shaping_class(c) {
        Some(ShapingClass::Nukta) => true,
        Some(ShapingClass::ConsonantPostRepha) => true,
        _ => false,
    }
}

fn zwj(c: char) -> bool {
    shaping_class(c) == Some(ShapingClass::Joiner)
}

fn zwnj(c: char) -> bool {
    shaping_class(c) == Some(ShapingClass::NonJoiner)
}

fn matra(c: char) -> bool {
    match shaping_class(c) {
        Some(ShapingClass::VowelDependent) => true,
        Some(ShapingClass::PureKiller) => true,
        Some(ShapingClass::ConsonantKiller) => true,
        _ => false,
    }
}

fn syllable_modifier(c: char) -> bool {
    match shaping_class(c) {
        Some(ShapingClass::SyllableModifier) => true,
        Some(ShapingClass::Bindu) => true,
        Some(ShapingClass::Visarga) => true,
        _ => false,
    }
}

fn placeholder(c: char) -> bool {
    match shaping_class(c) {
        Some(ShapingClass::Placeholder) => true,
        Some(ShapingClass::ConsonantPlaceholder) => true,
        _ => false,
    }
}

fn dotted_circle(c: char) -> bool {
    shaping_class(c) == Some(ShapingClass::DottedCircle)
}

fn register_shifter(c: char) -> bool {
    shaping_class(c) == Some(ShapingClass::RegisterShifter)
}

fn coeng(c: char) -> bool {
    shaping_class(c) == Some(ShapingClass::InvisibleStacker)
}

fn _symbol(c: char) -> bool {
    match shaping_class(c) {
        Some(ShapingClass::Symbol) => true,
        Some(ShapingClass::Avagraha) => true,
        _ => false,
    }
}

fn match_c<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_either(
        match_one(consonant),
        match_either(match_one(ra), match_one(vowel)),
    )(cs)
}

fn match_n<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional_seq(
        match_seq(match_optional(match_one(zwnj)), match_one(register_shifter)),
        match_optional(match_seq(
            match_one(nukta),
            match_optional(match_one(nukta)),
        )),
    )(cs)
}

fn match_z<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_either(match_one(zwj), match_one(zwnj))(cs)
}

fn match_cn<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_seq(match_c, match_optional(match_n))(cs)
}

fn match_matra_group<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional_seq(
        match_z,
        match_seq(match_one(matra), match_optional(match_n)),
    )(cs)
}

fn match_syllable_tail<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional(match_seq(
        match_one(syllable_modifier),
        match_optional(match_one(syllable_modifier)),
    ))(cs)
}

fn match_partial_cluster<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional_seq(
        match_n,
        match_repeat_upto(
            4,
            match_seq(match_one(coeng), match_cn),
            match_repeat_upto(
                4,
                match_matra_group,
                match_optional_seq(match_seq(match_one(coeng), match_cn), match_syllable_tail),
            ),
        ),
    )(cs)
}

fn match_valid_syllable<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_seq(
        match_either(
            match_c,
            match_either(match_one(placeholder), match_one(dotted_circle)),
        ),
        match_partial_cluster,
    )(cs)
}

fn match_syllable<T: SyllableChar>(cs: &[T]) -> Option<(usize, Syllable)> {
    match match_valid_syllable(cs) {
        Some(len) => Some((len, Syllable::Valid)),
        None => match match_partial_cluster(cs) {
            // The entire partial cluster is optional, which can lead to zero-
            // length matches. Categorise these as invalid syllables instead.
            Some(len) if len > 0 => Some((len, Syllable::Broken)),
            _ => None,
        },
    }
}

#[derive(Copy, Clone, Debug)]
enum Syllable {
    Valid,
    Broken,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq)]
enum ShapingClass {
    Avagraha,
    Bindu,
    Cantillation,
    Consonant,
    ConsonantDead,
    ConsonantKiller,
    ConsonantMedial,
    ConsonantPlaceholder,
    ConsonantPostRepha,
    ConsonantPreRepha,
    ConsonantWithStacker,
    DottedCircle,
    GeminationMark,
    InvisibleStacker,
    Joiner,
    ModifyingLetter,
    NonJoiner,
    Nukta,
    Number,
    Placeholder,
    PureKiller,
    RegisterShifter,
    SyllableModifier,
    Symbol,
    Virama,
    Visarga,
    VowelDependent,
    VowelIndependent,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq)]
enum MarkPlacementSubclass {
    Bottom,
    Left,
    LeftAndRight,
    Overstruck,
    Right,
    Top,
    TopAndBottom,
    TopAndLeft,
    TopAndLeftAndRight,
    TopAndRight,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum BasicFeature {
    Locl,
    Ccmp,
    Pref,
    Blwf,
    Abvf,
    Pstf,
    Cfar,
}

impl BasicFeature {
    const ALL: &'static [BasicFeature] = &[
        BasicFeature::Locl,
        BasicFeature::Ccmp,
        BasicFeature::Pref,
        BasicFeature::Blwf,
        BasicFeature::Abvf,
        BasicFeature::Pstf,
        BasicFeature::Cfar,
    ];

    fn mask(self) -> FeatureMask {
        match self {
            BasicFeature::Locl => FeatureMask::LOCL,
            BasicFeature::Ccmp => FeatureMask::CCMP,
            BasicFeature::Pref => FeatureMask::PREF,
            BasicFeature::Blwf => FeatureMask::BLWF,
            BasicFeature::Abvf => FeatureMask::ABVF,
            BasicFeature::Pstf => FeatureMask::PSTF,
            BasicFeature::Cfar => FeatureMask::CFAR,
        }
    }

    fn is_global(self) -> bool {
        match self {
            BasicFeature::Locl => true,
            BasicFeature::Ccmp => true,
            BasicFeature::Pref => false,
            BasicFeature::Blwf => false,
            BasicFeature::Abvf => false,
            BasicFeature::Pstf => true,
            BasicFeature::Cfar => false,
        }
    }
}

pub(super) fn preprocess_khmer(cs: &mut Vec<char>) {
    decompose_matra(cs);
    sort_by_modified_combining_class(cs);
}

fn decompose_matra(cs: &mut Vec<char>) {
    let mut i = 0;
    while i < cs.len() {
        match cs[i] {
            '\u{17BE}' | '\u{17BF}' | '\u{17C0}' | '\u{17C4}' | '\u{17C5}' => {
                cs.insert(i, '\u{17C1}');
                i += 2;
            }
            _ => i += 1,
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct KhmerData {
    mask: FeatureMask,
}

impl GlyphData for KhmerData {
    fn merge(d1: KhmerData, _d2: KhmerData) -> KhmerData {
        d1
    }
}

type RawGlyphKhmer = RawGlyph<KhmerData>;

impl RawGlyphKhmer {
    fn is(&self, f: impl FnOnce(char) -> bool) -> bool {
        match self.glyph_origin {
            GlyphOrigin::Char(c) => f(c),
            GlyphOrigin::Direct => false,
        }
    }

    fn has_mask(&self, mask: FeatureMask) -> bool {
        self.extra_data.mask.contains(mask)
    }

    fn add_mask(&mut self, mask: FeatureMask) {
        self.extra_data.mask.insert(mask)
    }
}

pub fn gsub_apply_khmer(
    dotted_circle_index: u16,
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    glyphs: &mut Vec<RawGlyph<()>>,
) -> Result<(), ShapingError> {
    let mut syllables = to_khmer_syllables(dotted_circle_index, glyphs);

    for (syllable, syllable_type) in syllables.iter_mut() {
        shape_syllable(
            gsub_cache,
            gsub_table,
            gdef_table,
            script_tag,
            lang_tag,
            feature_variations,
            syllable,
            *syllable_type,
        )?;
    }

    *glyphs = syllables
        .into_iter()
        .flat_map(|s| s.0)
        .map(from_raw_glyph_khmer)
        .collect();

    Ok(())
}

fn to_khmer_syllables(
    dotted_circle_index: u16,
    mut glyphs: &[RawGlyph<()>],
) -> Vec<(Vec<RawGlyphKhmer>, Syllable)> {
    let mut syllables: Vec<(Vec<RawGlyphKhmer>, Syllable)> = Vec::new();

    while !glyphs.is_empty() {
        match match_syllable(glyphs) {
            Some((len, syllable_type)) => {
                let mut syllable;
                match syllable_type {
                    Syllable::Valid => {
                        syllable = glyphs[..len].iter().map(to_raw_glyph_khmer).collect();
                    }
                    Syllable::Broken => {
                        // Prepend a dotted circle to a broken syllable, then treat it as valid.
                        syllable = Vec::with_capacity(len + 1);
                        insert_dotted_circle(dotted_circle_index, &mut syllable);
                        syllable.extend(glyphs[..len].iter().map(to_raw_glyph_khmer));
                    }
                }
                syllables.push((syllable, Syllable::Valid));
                glyphs = &glyphs[len..];
            }
            None => {
                let invalid_glyph = to_raw_glyph_khmer(&glyphs[0]);
                match syllables.last_mut() {
                    // Append invalid glyph to last syllable if syllable is invalid.
                    Some((invalid_syllable, Syllable::Broken)) => {
                        invalid_syllable.push(invalid_glyph)
                    }
                    _ => syllables.push((vec![invalid_glyph], Syllable::Broken)),
                }
                glyphs = &glyphs[1..];
            }
        }
    }

    syllables
}

fn insert_dotted_circle(dotted_circle_index: u16, glyphs: &mut Vec<RawGlyphKhmer>) {
    if dotted_circle_index == 0 {
        return;
    }
    let dotted_circle = RawGlyphKhmer {
        unicodes: tiny_vec![[char; 1] => DOTTED_CIRCLE],
        glyph_index: dotted_circle_index,
        liga_component_pos: 0,
        glyph_origin: GlyphOrigin::Char(DOTTED_CIRCLE),
        flags: RawGlyphFlags::empty(),
        variation: None,
        extra_data: KhmerData {
            mask: FeatureMask::empty(),
        },
    };
    glyphs.insert(0, dotted_circle);
}

fn shape_syllable(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    syllable: &mut Vec<RawGlyphKhmer>,
    syllable_type: Syllable,
) -> Result<(), ShapingError> {
    let max_glyphs = syllable.len().saturating_mul(gsub::MAX_GLYPHS_FACTOR);

    match syllable_type {
        Syllable::Valid => {
            reorder_and_mask_syllable(syllable)?;
            apply_basic_features(
                gsub_cache,
                gsub_table,
                gdef_table,
                script_tag,
                lang_tag,
                feature_variations,
                syllable,
                max_glyphs,
            )?;
            apply_remaining_features(
                gsub_cache,
                gsub_table,
                gdef_table,
                script_tag,
                lang_tag,
                feature_variations,
                syllable,
                max_glyphs,
            )?;
        }
        Syllable::Broken => {}
    }

    Ok(())
}

fn reorder_and_mask_syllable(glyphs: &mut [RawGlyphKhmer]) -> Result<(), ShapingError> {
    let mut base_i = match glyphs.iter().position(|g| g.is(base_candidate)) {
        Some(i) => i,
        None => return Ok(()),
    };

    if let Some(ra_i) = glyphs[(base_i + 1)..].iter().position(|g| g.is(ra)) {
        let ra_i = ra_i + base_i + 1;
        // CFAR is applied to glyphs occurring after a (Sign Coeng, Ro) sequence.
        glyphs[(ra_i + 1)..]
            .iter_mut()
            .for_each(|g| g.add_mask(BasicFeature::Cfar.mask()));
        // A (Sign Coeng, Ro) sequence is reordered to before the base consonant.
        // (The syllable matcher should ensure that a Sign Coeng precedes a Ro.)
        glyphs[base_i..=ra_i].rotate_right(2);
        base_i += 2;
        glyphs[base_i - 1].add_mask(BasicFeature::Pref.mask());
        glyphs[base_i - 2].add_mask(BasicFeature::Pref.mask());
    }

    let post_base_masks =
        BasicFeature::Blwf.mask() | BasicFeature::Abvf.mask() | BasicFeature::Pstf.mask();
    glyphs[(base_i + 1)..]
        .iter_mut()
        .for_each(|g| g.add_mask(post_base_masks));

    fn left_matra(c: char) -> bool {
        matra(c) && mark_placement(c) == Some(MarkPlacementSubclass::Left)
    }

    // Reorder a left matra to the start of the syllable. Consistent with Uniscribe.
    // HarfBuzz's reordering depends on the left matra's initial position relative to
    // the initial (Sign Coeng, Ro) position. Example:
    //     U+1780, U+17C1, U+17D2, U+179A (occurs before Coeng, Ro; reordered after Coeng, Ro)
    //     U+1780, U+17D2, U+179A, U+17C1 (occurs after Coeng, Ro; reordered before Coeng, Ro)
    // See: https://github.com/harfbuzz/harfbuzz/commit/1a96cc825dc9.
    if let Some(left_matra_i) = glyphs.iter().position(|g| g.is(left_matra)) {
        glyphs[..=left_matra_i].rotate_right(1);
        // base_i += 1; (Not required for now.)
    }

    Ok(())
}

fn apply_basic_features(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    glyphs: &mut Vec<RawGlyphKhmer>,
    max_glyphs: usize,
) -> Result<(), ParseError> {
    // Apply features in _lookup_ order. HarfBuzz believes that Uniscribe does this. In our
    // corpus tests, this fixes issues with certain fonts (Battambang and Khmer UI) without
    // causing regressions in others.
    let features = BasicFeature::ALL
        .iter()
        .fold(FeatureMask::empty(), |acc, f| acc | f.mask());
    let index = gsub::get_lookups_cache_index(
        gsub_cache,
        script_tag,
        lang_tag,
        feature_variations,
        features,
    )?;
    let lookups = &gsub_cache.cached_lookups.lock().unwrap()[index];

    for &(lookup_index, feature_tag) in lookups {
        let feature = match feature_tag {
            tag::LOCL => BasicFeature::Locl,
            tag::CCMP => BasicFeature::Ccmp,
            tag::PREF => BasicFeature::Pref,
            tag::BLWF => BasicFeature::Blwf,
            tag::ABVF => BasicFeature::Abvf,
            tag::PSTF => BasicFeature::Pstf,
            tag::CFAR => BasicFeature::Cfar,
            _ => panic!("unexpected feature tag"), // Should never happen.
        };
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
            |g| feature.is_global() || g.has_mask(feature.mask()),
        )?;
    }

    Ok(())
}

fn apply_remaining_features(
    gsub_cache: &LayoutCache<GSUB>,
    gsub_table: &LayoutTable<GSUB>,
    gdef_table: Option<&GDEFTable>,
    script_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&FeatureTableSubstitution<'_>>,
    glyphs: &mut Vec<RawGlyphKhmer>,
    max_glyphs: usize,
) -> Result<(), ParseError> {
    let features = FeatureMask::ABVS
        | FeatureMask::BLWS
        | FeatureMask::CALT
        | FeatureMask::CLIG
        | FeatureMask::LIGA
        | FeatureMask::PRES
        | FeatureMask::PSTS;

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

fn base_candidate(c: char) -> bool {
    consonant(c) || ra(c) || vowel(c) || placeholder(c) || dotted_circle(c)
}

fn to_raw_glyph_khmer(g: &RawGlyph<()>) -> RawGlyphKhmer {
    RawGlyphKhmer {
        unicodes: g.unicodes.clone(),
        glyph_index: g.glyph_index,
        liga_component_pos: g.liga_component_pos,
        glyph_origin: g.glyph_origin,
        flags: g.flags,
        variation: g.variation,
        extra_data: KhmerData {
            mask: FeatureMask::empty(),
        },
    }
}

fn from_raw_glyph_khmer(g: RawGlyphKhmer) -> RawGlyph<()> {
    RawGlyph {
        unicodes: g.unicodes,
        glyph_index: g.glyph_index,
        liga_component_pos: g.liga_component_pos,
        glyph_origin: g.glyph_origin,
        flags: g.flags,
        variation: g.variation,
        extra_data: (),
    }
}

fn khmer_character(c: char) -> (Option<ShapingClass>, Option<MarkPlacementSubclass>) {
    use MarkPlacementSubclass::*;
    use ShapingClass::*;

    match c as u32 {
        0x1780 => (Some(Consonant), None),                          // Ka
        0x1781 => (Some(Consonant), None),                          // Kha
        0x1782 => (Some(Consonant), None),                          // Ko
        0x1783 => (Some(Consonant), None),                          // Kho
        0x1784 => (Some(Consonant), None),                          // Ngo
        0x1785 => (Some(Consonant), None),                          // Ca
        0x1786 => (Some(Consonant), None),                          // Cha
        0x1787 => (Some(Consonant), None),                          // Co
        0x1788 => (Some(Consonant), None),                          // Cho
        0x1789 => (Some(Consonant), None),                          // Nyo
        0x178A => (Some(Consonant), None),                          // Da
        0x178B => (Some(Consonant), None),                          // Ttha
        0x178C => (Some(Consonant), None),                          // Do
        0x178D => (Some(Consonant), None),                          // Ttho
        0x178E => (Some(Consonant), None),                          // Nno
        0x178F => (Some(Consonant), None),                          // Ta
        0x1790 => (Some(Consonant), None),                          // Tha
        0x1791 => (Some(Consonant), None),                          // To
        0x1792 => (Some(Consonant), None),                          // Tho
        0x1793 => (Some(Consonant), None),                          // No
        0x1794 => (Some(Consonant), None),                          // Ba
        0x1795 => (Some(Consonant), None),                          // Pha
        0x1796 => (Some(Consonant), None),                          // Po
        0x1797 => (Some(Consonant), None),                          // Pho
        0x1798 => (Some(Consonant), None),                          // Mo
        0x1799 => (Some(Consonant), None),                          // Yo
        0x179A => (Some(Consonant), None),                          // Ro
        0x179B => (Some(Consonant), None),                          // Lo
        0x179C => (Some(Consonant), None),                          // Vo
        0x179D => (Some(Consonant), None),                          // Sha
        0x179E => (Some(Consonant), None),                          // Sso
        0x179F => (Some(Consonant), None),                          // Sa
        0x17A0 => (Some(Consonant), None),                          // Ha
        0x17A1 => (Some(Consonant), None),                          // La
        0x17A2 => (Some(Consonant), None),                          // Qa
        0x17A3 => (Some(VowelIndependent), None),                   // Qaq
        0x17A4 => (Some(VowelIndependent), None),                   // Qaa
        0x17A5 => (Some(VowelIndependent), None),                   // Qi
        0x17A6 => (Some(VowelIndependent), None),                   // Qii
        0x17A7 => (Some(VowelIndependent), None),                   // Qu
        0x17A8 => (Some(VowelIndependent), None),                   // Quk
        0x17A9 => (Some(VowelIndependent), None),                   // Quu
        0x17AA => (Some(VowelIndependent), None),                   // Quuv
        0x17AB => (Some(VowelIndependent), None),                   // Ry
        0x17AC => (Some(VowelIndependent), None),                   // Ryy
        0x17AD => (Some(VowelIndependent), None),                   // Ly
        0x17AE => (Some(VowelIndependent), None),                   // Lyy
        0x17AF => (Some(VowelIndependent), None),                   // Qe
        0x17B0 => (Some(VowelIndependent), None),                   // Qai
        0x17B1 => (Some(VowelIndependent), None),                   // Qoo Type One
        0x17B2 => (Some(VowelIndependent), None),                   // Qoo Type Two
        0x17B3 => (Some(VowelIndependent), None),                   // Qau
        0x17B4 => (None, None),                                     // Inherent Aq
        0x17B5 => (None, None),                                     // Inherent Aa
        0x17B6 => (Some(VowelDependent), Some(Right)),              // Sign Aa
        0x17B7 => (Some(VowelDependent), Some(Top)),                // Sign I
        0x17B8 => (Some(VowelDependent), Some(Top)),                // Sign Ii
        0x17B9 => (Some(VowelDependent), Some(Top)),                // Sign Y
        0x17BA => (Some(VowelDependent), Some(Top)),                // Sign Yy
        0x17BB => (Some(VowelDependent), Some(Bottom)),             // Sign U
        0x17BC => (Some(VowelDependent), Some(Bottom)),             // Sign Uu
        0x17BD => (Some(VowelDependent), Some(Bottom)),             // Sign Ua
        0x17BE => (Some(VowelDependent), Some(TopAndLeft)),         // Sign Oe
        0x17BF => (Some(VowelDependent), Some(TopAndLeftAndRight)), // Sign Ya
        0x17C0 => (Some(VowelDependent), Some(LeftAndRight)),       // Sign Ie
        0x17C1 => (Some(VowelDependent), Some(Left)),               // Sign E
        0x17C2 => (Some(VowelDependent), Some(Left)),               // Sign Ae
        0x17C3 => (Some(VowelDependent), Some(Left)),               // Sign Ai
        0x17C4 => (Some(VowelDependent), Some(LeftAndRight)),       // Sign Oo
        0x17C5 => (Some(VowelDependent), Some(LeftAndRight)),       // Sign Au
        0x17C6 => (Some(Nukta), Some(Top)),                         // Nikahit
        0x17C7 => (Some(Visarga), Some(Right)),                     // Reahmuk
        0x17C8 => (Some(VowelDependent), Some(Right)),              // Yuukaleapintu
        0x17C9 => (Some(RegisterShifter), Some(Top)),               // Muusikatoan
        0x17CA => (Some(RegisterShifter), Some(Top)),               // Triisap
        0x17CB => (Some(SyllableModifier), Some(Top)),              // Bantoc
        0x17CC => (Some(ConsonantPostRepha), Some(Top)),            // Robat
        0x17CD => (Some(ConsonantKiller), Some(Top)),               // Toandakhiat
        0x17CE => (Some(SyllableModifier), Some(Top)),              // Kakabat
        0x17CF => (Some(SyllableModifier), Some(Top)),              // Ahsda
        0x17D0 => (Some(SyllableModifier), Some(Top)),              // Samyok Sannya
        0x17D1 => (Some(PureKiller), Some(Top)),                    // Viriam
        0x17D2 => (Some(InvisibleStacker), None),                   // Sign Coeng
        0x17D3 => (Some(SyllableModifier), Some(Top)),              // Bathamasat
        0x17D4 => (None, None),                                     // Khan
        0x17D5 => (None, None),                                     // Bariyoosan
        0x17D6 => (None, None),                                     // Camnuc Pii Kuuh
        0x17D7 => (None, None),                                     // Lek Too
        0x17D8 => (None, None),                                     // Beyyal
        0x17D9 => (None, None),                                     // Phnaek Muan
        0x17DA => (None, None),                                     // Koomuut
        0x17DB => (Some(Symbol), None),                             // Riel
        0x17DC => (Some(Avagraha), None),                           // Avakrahasanya
        0x17DD => (Some(SyllableModifier), Some(Top)),              // Atthacan
        0x17E0 => (Some(Number), None),                             // Digit Zero
        0x17E1 => (Some(Number), None),                             // Digit One
        0x17E2 => (Some(Number), None),                             // Digit Two
        0x17E3 => (Some(Number), None),                             // Digit Three
        0x17E4 => (Some(Number), None),                             // Digit Four
        0x17E5 => (Some(Number), None),                             // Digit Five
        0x17E6 => (Some(Number), None),                             // Digit Six
        0x17E7 => (Some(Number), None),                             // Digit Seven
        0x17E8 => (Some(Number), None),                             // Digit Eight
        0x17E9 => (Some(Number), None),                             // Digit Nine
        0x17F0 => (None, None),                                     // Lek Attak Son
        0x17F1 => (None, None),                                     // Lek Attak Muoy
        0x17F2 => (None, None),                                     // Lek Attak Pii
        0x17F3 => (None, None),                                     // Lek Attak Bei
        0x17F4 => (None, None),                                     // Lek Attak Buon
        0x17F5 => (None, None),                                     // Lek Attak Pram
        0x17F6 => (None, None),                                     // Lek Attak Pram-Muoy
        0x17F7 => (None, None),                                     // Lek Attak Pram-Pii
        0x17F8 => (None, None),                                     // Lek Attak Pram-Bei
        0x17F9 => (None, None),                                     // Lek Attak Pram-Buon

        // Khmer symbols character table.
        0x19E0 => (None, None), // Pathamasat
        0x19E1 => (None, None), // Muoy Koet
        0x19E2 => (None, None), // Pii Koet
        0x19E3 => (None, None), // Bei Koet
        0x19E4 => (None, None), // Buon Koet
        0x19E5 => (None, None), // Pram Koet
        0x19E6 => (None, None), // Pram-Muoy Koet
        0x19E7 => (None, None), // Pram-Pii Koet
        0x19E8 => (None, None), // Pram-Bei Koet
        0x19E9 => (None, None), // Pram-Buon Koet
        0x19EA => (None, None), // Dap Koet
        0x19EB => (None, None), // Dap-Muoy Koet
        0x19EC => (None, None), // Dap-Pii Koet
        0x19ED => (None, None), // Dap-Bei Koet
        0x19EE => (None, None), // Dap-Buon Koet
        0x19EF => (None, None), // Dap-Pram Koet
        0x19F0 => (None, None), // Tuteyasat
        0x19F1 => (None, None), // Muoy ROC
        0x19F2 => (None, None), // Pii Roc
        0x19F3 => (None, None), // Bei Roc
        0x19F4 => (None, None), // Buon Roc
        0x19F5 => (None, None), // Pram Roc
        0x19F6 => (None, None), // Pram-Muoy Roc
        0x19F7 => (None, None), // Pram-Pii Roc
        0x19F8 => (None, None), // Pram-Bei Roc
        0x19F9 => (None, None), // Pram-Buon Roc
        0x19FA => (None, None), // Dap Roc
        0x19FB => (None, None), // Dap-Muoy Roc
        0x19FC => (None, None), // Dap-Pii Roc
        0x19FD => (None, None), // Dap-Bei Roc
        0x19FE => (None, None), // Dap-Buon Roc
        0x19FF => (None, None), // Dap-Pram Roc

        // Miscellaneous character table.
        0x00A0 => (Some(Placeholder), None),  // No-break space
        0x200C => (Some(NonJoiner), None),    // Zero-width non-joiner
        0x200D => (Some(Joiner), None),       // Zero-width joiner
        0x2010 => (Some(Placeholder), None),  // Hyphen
        0x2011 => (Some(Placeholder), None),  // No-break hyphen
        0x2012 => (Some(Placeholder), None),  // Figure dash
        0x2013 => (Some(Placeholder), None),  // En dash
        0x2014 => (Some(Placeholder), None),  // Em dash
        0x25CC => (Some(DottedCircle), None), // Dotted circle

        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod decompose_matra {
        use super::*;

        #[test]
        fn test_decomposition1() {
            let mut cs = vec!['\u{17C0}'];
            decompose_matra(&mut cs);

            assert_eq!(vec!['\u{17C1}', '\u{17C0}'], cs);
        }

        #[test]
        fn test_decomposition2() {
            let mut cs = vec!['\u{17C0}', '\u{17C0}'];
            decompose_matra(&mut cs);

            assert_eq!(vec!['\u{17C1}', '\u{17C0}', '\u{17C1}', '\u{17C0}'], cs);
        }
    }
}
