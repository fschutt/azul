//! Implementation of font shaping for Indic scripts

use log::debug;
use unicode_general_category::GeneralCategory;

use crate::error::{ComplexScriptError, ParseError, ShapingError};
use crate::gsub::{self, FeatureMask, GlyphData, GlyphOrigin, RawGlyph, RawGlyphFlags};
use crate::layout::{FeatureTableSubstitution, GDEFTable, LangSys, LayoutCache, LayoutTable, GSUB};
use crate::scripts::syllable::*;
use crate::tinyvec::tiny_vec;
use crate::unicode::mcc::sort_by_modified_combining_class;
use crate::{tag, DOTTED_CIRCLE};

#[derive(Copy, Clone, Debug, PartialEq)]
enum Script {
    Devanagari,
    Bengali,
    Gurmukhi,
    Gujarati,
    Oriya,
    Tamil,
    Telugu,
    Kannada,
    Malayalam,
    Sinhala,
}

#[derive(Copy, Clone, Debug)]
enum BasePos {
    // First,
    Last,
    LastSinhala,
}

#[derive(Copy, Clone, Debug)]
enum RephMode {
    Explicit,
    Implicit,
    LogicalRepha,
    // VisualRepha,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum BlwfMode {
    PostOnly,
    PreAndPost,
}

impl Script {
    fn base_consonant_pos(self) -> BasePos {
        match self {
            Script::Devanagari => BasePos::Last,
            Script::Bengali => BasePos::Last,
            Script::Gurmukhi => BasePos::Last,
            Script::Gujarati => BasePos::Last,
            Script::Oriya => BasePos::Last,
            Script::Tamil => BasePos::Last,
            Script::Telugu => BasePos::Last,
            Script::Kannada => BasePos::Last,
            Script::Malayalam => BasePos::Last,
            Script::Sinhala => BasePos::LastSinhala,
        }
    }

    fn reph_position(self) -> Pos {
        match self {
            Script::Devanagari => Pos::BeforePost,
            Script::Bengali => Pos::AfterSubjoined,
            Script::Gurmukhi => Pos::BeforeSubjoined,
            Script::Gujarati => Pos::BeforePost,
            Script::Oriya => Pos::AfterMain,
            Script::Tamil => Pos::AfterPost,
            Script::Telugu => Pos::AfterPost,
            Script::Kannada => Pos::AfterPost,
            Script::Malayalam => Pos::AfterMain,
            Script::Sinhala => Pos::AfterMain,
        }
    }

    fn reph_mode(self) -> RephMode {
        match self {
            Script::Devanagari => RephMode::Implicit,
            Script::Bengali => RephMode::Implicit,
            Script::Gurmukhi => RephMode::Implicit,
            Script::Gujarati => RephMode::Implicit,
            Script::Oriya => RephMode::Implicit,
            Script::Tamil => RephMode::Implicit,
            Script::Telugu => RephMode::Explicit,
            Script::Kannada => RephMode::Implicit,
            Script::Malayalam => RephMode::LogicalRepha,
            Script::Sinhala => RephMode::Explicit,
        }
    }

    fn blwf_mode(self) -> BlwfMode {
        match self {
            Script::Devanagari => BlwfMode::PreAndPost,
            Script::Bengali => BlwfMode::PreAndPost,
            Script::Gurmukhi => BlwfMode::PreAndPost,
            Script::Gujarati => BlwfMode::PreAndPost,
            Script::Oriya => BlwfMode::PreAndPost,
            Script::Tamil => BlwfMode::PreAndPost,
            Script::Telugu => BlwfMode::PostOnly,
            Script::Kannada => BlwfMode::PostOnly,
            Script::Malayalam => BlwfMode::PreAndPost,
            Script::Sinhala => BlwfMode::PreAndPost,
        }
    }

    fn abovebase_matra_pos(self) -> Option<Pos> {
        match self {
            Script::Devanagari => Some(Pos::AfterSubjoined),
            Script::Bengali => None,
            Script::Gurmukhi => Some(Pos::AfterPost),
            Script::Gujarati => Some(Pos::AfterSubjoined),
            Script::Oriya => Some(Pos::AfterMain),
            Script::Tamil => Some(Pos::AfterSubjoined),
            Script::Telugu => Some(Pos::BeforeSubjoined),
            Script::Kannada => Some(Pos::BeforeSubjoined),
            Script::Malayalam => None,
            Script::Sinhala => Some(Pos::AfterSubjoined),
        }
    }

    fn rightside_matra_pos(self, ch: char) -> Option<Pos> {
        match self {
            Script::Devanagari => Some(Pos::AfterSubjoined),
            Script::Bengali => Some(Pos::AfterPost),
            Script::Gurmukhi => Some(Pos::AfterPost),
            Script::Gujarati => Some(Pos::AfterPost),
            Script::Oriya => Some(Pos::AfterPost),
            Script::Tamil => Some(Pos::AfterPost),
            Script::Telugu => match ch {
                '\u{0C41}' => Some(Pos::BeforeSubjoined),
                '\u{0C42}' => Some(Pos::BeforeSubjoined),
                '\u{0C43}' => Some(Pos::AfterSubjoined),
                '\u{0C44}' => Some(Pos::AfterSubjoined),
                _ => None,
            },
            Script::Kannada => match ch {
                '\u{0CBE}' => Some(Pos::BeforeSubjoined),
                '\u{0CC0}' => Some(Pos::BeforeSubjoined),
                '\u{0CC1}' => Some(Pos::BeforeSubjoined),
                '\u{0CC2}' => Some(Pos::BeforeSubjoined),
                '\u{0CC3}' => Some(Pos::AfterSubjoined),
                '\u{0CC4}' => Some(Pos::AfterSubjoined),
                '\u{0CD5}' => Some(Pos::AfterSubjoined),
                '\u{0CD6}' => Some(Pos::AfterSubjoined),
                _ => None,
            },
            Script::Malayalam => Some(Pos::AfterPost),
            Script::Sinhala => Some(Pos::AfterSubjoined),
        }
    }

    fn belowbase_matra_pos(self) -> Pos {
        match self {
            Script::Devanagari => Pos::AfterSubjoined,
            Script::Bengali => Pos::AfterSubjoined,
            Script::Gurmukhi => Pos::AfterPost,
            Script::Gujarati => Pos::AfterPost,
            Script::Oriya => Pos::AfterSubjoined,
            Script::Tamil => Pos::AfterPost,
            Script::Telugu => Pos::BeforeSubjoined,
            Script::Kannada => Pos::BeforeSubjoined,
            Script::Malayalam => Pos::AfterPost,
            Script::Sinhala => Pos::AfterSubjoined,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ShapingModel {
    Indic1,
    Indic2,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum BasicFeature {
    Locl,
    Nukt,
    Akhn,
    Rphf,
    Rkrf,
    Pref,
    Blwf,
    Abvf,
    Half,
    Pstf,
    Vatu,
    Cjct,
    Cfar,
}

impl BasicFeature {
    const ALL: &'static [BasicFeature] = &[
        BasicFeature::Locl,
        BasicFeature::Nukt,
        BasicFeature::Akhn,
        BasicFeature::Rphf,
        BasicFeature::Rkrf,
        BasicFeature::Pref,
        BasicFeature::Blwf,
        BasicFeature::Abvf,
        BasicFeature::Half,
        BasicFeature::Pstf,
        BasicFeature::Vatu,
        BasicFeature::Cjct,
        BasicFeature::Cfar,
    ];

    fn tag(self) -> u32 {
        match self {
            BasicFeature::Locl => tag::LOCL,
            BasicFeature::Nukt => tag::NUKT,
            BasicFeature::Akhn => tag::AKHN,
            BasicFeature::Rphf => tag::RPHF,
            BasicFeature::Rkrf => tag::RKRF,
            BasicFeature::Pref => tag::PREF,
            BasicFeature::Blwf => tag::BLWF,
            BasicFeature::Abvf => tag::ABVF,
            BasicFeature::Half => tag::HALF,
            BasicFeature::Pstf => tag::PSTF,
            BasicFeature::Vatu => tag::VATU,
            BasicFeature::Cjct => tag::CJCT,
            BasicFeature::Cfar => tag::CFAR,
        }
    }

    fn mask(self) -> FeatureMask {
        match self {
            BasicFeature::Locl => FeatureMask::LOCL,
            BasicFeature::Nukt => FeatureMask::NUKT,
            BasicFeature::Akhn => FeatureMask::AKHN,
            BasicFeature::Rphf => FeatureMask::RPHF,
            BasicFeature::Rkrf => FeatureMask::RKRF,
            BasicFeature::Pref => FeatureMask::PREF,
            BasicFeature::Blwf => FeatureMask::BLWF,
            BasicFeature::Abvf => FeatureMask::ABVF,
            BasicFeature::Half => FeatureMask::HALF,
            BasicFeature::Pstf => FeatureMask::PSTF,
            BasicFeature::Vatu => FeatureMask::VATU,
            BasicFeature::Cjct => FeatureMask::CJCT,
            BasicFeature::Cfar => FeatureMask::CFAR,
        }
    }

    // Returns `true` if feature applies to the entire glyph buffer.
    fn is_global(self) -> bool {
        match self {
            BasicFeature::Locl => true,
            BasicFeature::Nukt => true,
            BasicFeature::Akhn => true,
            BasicFeature::Rphf => false,
            BasicFeature::Rkrf => true,
            BasicFeature::Pref => false,
            BasicFeature::Blwf => false,
            BasicFeature::Abvf => true,
            BasicFeature::Half => false,
            BasicFeature::Pstf => false,
            BasicFeature::Vatu => true,
            BasicFeature::Cjct => true,
            BasicFeature::Cfar => true,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ShapingClass {
    Bindu,
    Visarga,
    Avagraha,
    Nukta,
    Virama,
    Cantillation,
    GeminationMark,
    PureKiller,
    SyllableModifier,
    Consonant,
    VowelIndependent,
    VowelDependent,
    ConsonantDead,
    ConsonantMedial,
    ConsonantPlaceholder,
    ConsonantWithStacker,
    ConsonantPreRepha,
    ModifyingLetter,
    Placeholder,
    Number,
    Symbol,
    Joiner,
    NonJoiner,
    DottedCircle,
}

#[derive(Copy, Clone, Debug)]
enum MarkPlacementSubclass {
    TopPosition,
    RightPosition,
    BottomPosition,
    LeftPosition,
    LeftAndRightPosition,
    TopAndRightPosition,
    TopAndLeftPosition,
    TopLeftAndRightPosition,
    TopAndBottomPosition,
    Overstruck,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
enum Pos {
    RaToBecomeReph,
    PrebaseMatra,
    PrebaseConsonant,
    SyllableBase,
    AfterMain,
    _AbovebaseConsonant,
    BeforeSubjoined,
    BelowbaseConsonant,
    AfterSubjoined,
    BeforePost,
    PostbaseConsonant,
    AfterPost,
    _FinalConsonant,
    SMVD,
}

/////////////////////////////////////////////////////////////////////////////
// Syllable state machine
/////////////////////////////////////////////////////////////////////////////

#[derive(Copy, Clone, Debug)]
enum Syllable {
    Consonant,
    Vowel,
    Standalone,
    Symbol,
    Broken,
}

fn shaping_class(ch: char) -> Option<ShapingClass> {
    let (shaping, _) = indic_character(ch);
    shaping
}

fn consonant(ch: char) -> bool {
    match shaping_class(ch) {
        Some(ShapingClass::Consonant) => !ra(ch),
        Some(ShapingClass::ConsonantDead) => true,
        _ => false,
    }
}

fn vowel(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::VowelIndependent))
}

fn nukta(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::Nukta))
}

fn halant(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::Virama))
}

fn zwj(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::Joiner))
}

fn zwnj(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::NonJoiner))
}

fn joiner(ch: char) -> bool {
    zwj(ch) || zwnj(ch)
}

fn ra(ch: char) -> bool {
    match ch {
        '\u{0930}' => true, // Devanagari
        '\u{09B0}' => true, // Bengali
        '\u{09F0}' => true, // Bengali, Assamese
        '\u{0A30}' => true, // Gurmukhi
        '\u{0AB0}' => true, // Gujarati
        '\u{0B30}' => true, // Oriya
        '\u{0BB0}' => true, // Tamil
        '\u{0C30}' => true, // Telugu
        '\u{0CB0}' => true, // Kannada
        '\u{0D30}' => true, // Malayalam
        '\u{0DBB}' => true, // Sinhala
        _ => false,
    }
}

fn matra(ch: char) -> bool {
    match shaping_class(ch) {
        Some(ShapingClass::VowelDependent) => true,
        Some(ShapingClass::PureKiller) => true,
        _ => false,
    }
}

fn syllable_modifier(ch: char) -> bool {
    match shaping_class(ch) {
        Some(ShapingClass::SyllableModifier) => true,
        Some(ShapingClass::Bindu) => true,
        Some(ShapingClass::Visarga) => true,
        Some(ShapingClass::GeminationMark) => true,
        _ => false,
    }
}

fn vedic_sign(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::Cantillation))
}

fn placeholder(ch: char) -> bool {
    match shaping_class(ch) {
        Some(ShapingClass::Number) => true,
        Some(ShapingClass::Placeholder) => true,
        Some(ShapingClass::ConsonantPlaceholder) => true,
        _ => false,
    }
}

fn dotted_circle(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::DottedCircle))
}

fn repha(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::ConsonantPreRepha))
}

fn consonant_medial(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::ConsonantMedial))
}

fn symbol(ch: char) -> bool {
    match shaping_class(ch) {
        Some(ShapingClass::Symbol) => true,
        Some(ShapingClass::Avagraha) => true,
        _ => false,
    }
}

fn consonant_with_stacker(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::ConsonantWithStacker))
}

#[allow(dead_code)]
fn other(ch: char) -> bool {
    matches!(shaping_class(ch), Some(ShapingClass::ModifyingLetter))
}

fn match_c<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_either(match_one(consonant), match_one(ra))(cs)
}

fn match_z<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_one(joiner)(cs)
}

fn match_reph<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_either(
        match_seq(match_one(ra), match_one(halant)),
        match_one(repha),
    )(cs)
}

fn match_cn<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_seq(
        match_c,
        match_optional_seq(match_one(zwj), match_optional(match_one(nukta))),
    )(cs)
}

fn match_forced_rakar<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_seq(
        match_one(zwj),
        match_seq(match_one(halant), match_seq(match_one(zwj), match_one(ra))),
    )(cs)
}

fn match_s<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_seq(match_one(symbol), match_optional(match_one(nukta)))(cs)
}

fn match_matra_group<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_repeat_upto(
        3,
        match_z,
        match_seq(
            match_one(matra),
            match_optional_seq(
                match_one(nukta),
                match_optional(match_either(match_one(halant), match_forced_rakar)),
            ),
        ),
    )(cs)
}

fn match_syllable_tail<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional_seq(
        match_optional_seq(
            match_z,
            match_seq(
                match_one(syllable_modifier),
                match_optional_seq(
                    match_one(syllable_modifier),
                    match_optional(match_one(zwnj)),
                ),
            ),
        ),
        match_repeat_upto(3, match_one(vedic_sign), match_unit()),
    )(cs)
}

fn match_halant_group<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional_seq(
        match_z,
        match_seq(
            match_one(halant),
            match_optional(match_seq(match_one(zwj), match_optional(match_one(nukta)))),
        ),
    )(cs)
}

// This is not used as we expand it inline
/*
fn match_final_halant_group<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_either(
        match_halant_group,
        match_seq(match_one(halant), match_one(zwnj)),
    )(cs)
}
*/

fn match_medial_group<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional(match_one(consonant_medial))(cs)
}

fn match_halant_or_matra_group<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    // this can match a short sequence so we expand and reorder it
    match_either(
        match_seq(match_one(halant), match_one(zwnj)),
        // Currently deviates from spec. See:
        // https://github.com/n8willis/opentype-shaping-documents/issues/72
        match_either(
            match_repeat_upto(4, match_matra_group, match_unit()),
            match_halant_group,
        ),
    )(cs)
}

fn match_consonant_syllable<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional_seq(
        match_either(match_one(repha), match_one(consonant_with_stacker)),
        match_repeat_upto(
            4,
            match_seq(match_cn, match_halant_group),
            match_seq(
                match_cn,
                match_seq(
                    match_medial_group,
                    match_seq(match_halant_or_matra_group, match_syllable_tail),
                ),
            ),
        ),
    )(cs)
}

fn match_vowel_syllable<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_optional_seq(
        match_reph,
        match_seq(
            match_one(vowel),
            match_optional_seq(
                match_one(nukta),
                match_either(
                    match_one(zwj),
                    match_repeat_upto(
                        4,
                        match_seq(match_halant_group, match_cn),
                        match_seq(
                            match_medial_group,
                            match_seq(match_halant_or_matra_group, match_syllable_tail),
                        ),
                    ),
                ),
            ),
        ),
    )(cs)
}

fn match_standalone_syllable<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_either_seq(
        match_optional_seq(
            match_either(match_one(repha), match_one(consonant_with_stacker)),
            match_one(placeholder),
        ),
        match_seq(match_optional(match_reph), match_one(dotted_circle)),
        match_optional_seq(
            match_one(nukta),
            match_repeat_upto(
                4,
                match_seq(match_halant_group, match_cn),
                match_seq(
                    match_medial_group,
                    match_seq(match_halant_or_matra_group, match_syllable_tail),
                ),
            ),
        ),
    )(cs)
}

fn match_symbol_syllable<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_seq(match_s, match_syllable_tail)(cs)
}

fn match_broken_syllable<T: SyllableChar>(cs: &[T]) -> Option<usize> {
    match_nonempty(match_optional_seq(
        match_reph,
        match_optional_seq(
            match_one(nukta),
            match_repeat_upto(
                4,
                match_seq(match_halant_group, match_cn),
                match_seq(
                    match_medial_group,
                    match_seq(match_halant_or_matra_group, match_syllable_tail),
                ),
            ),
        ),
    ))(cs)
}

fn match_syllable<T: SyllableChar>(cs: &[T]) -> Option<(usize, Syllable)> {
    let consonant = (match_consonant_syllable(cs), Syllable::Consonant);
    let vowel = (match_vowel_syllable(cs), Syllable::Vowel);
    let standalone = (match_standalone_syllable(cs), Syllable::Standalone);
    let symbol = (match_symbol_syllable(cs), Syllable::Symbol);
    let broken = (match_broken_syllable(cs), Syllable::Broken);

    // To prevent incorrect splitting (and mis-categorisation) of a syllable,
    // greediest syllable match, wins. In the event of a tie, precedence is
    // consonant > vowel > standalone > symbol > broken
    let syllables = &mut [consonant, vowel, standalone, symbol, broken];
    syllables.sort_by(|(len1, _), (len2, _)| len2.cmp(len1));

    match syllables[0] {
        (Some(len), syllable_type) => Some((len, syllable_type)),
        (None, _) => None,
    }
}

/////////////////////////////////////////////////////////////////////////////
// Preprocessing
/////////////////////////////////////////////////////////////////////////////

/// Preprocess Indic character sequences. This function should be called
/// prior to mapping Indic characters to their corresponding glyphs.
pub(super) fn preprocess_indic(cs: &mut Vec<char>, script_tag: u32) {
    let script = script(script_tag);

    constrain_vowel(cs);
    decompose_matra(cs);
    sort_by_modified_combining_class(cs);
    if script == Script::Bengali {
        recompose_bengali_ya_nukta(cs);
    } else if script == Script::Kannada {
        reorder_kannada_ra_halant_zwj(cs);
    }
}

/// Denotes if/where a constraining character should be inserted.
enum InsertConstraint {
    /// Insert a constraining character between a pair of characters.
    Between,
    /// Insert a constraining character after a pair of characters if
    /// the `char` immediately after the pair equals the `char` contained
    /// in `MaybeAfter`.
    MaybeAfter(char),
    /// Do not insert a constraining character.
    None,
}

/// Prohibit vowel combinations that look like other vowels by inserting
/// a constraining character in between these combinations.
///
/// E.g. Bengali Letter A + Bengali Sign Aa looks like Bengali Letter Aa.
fn constrain_vowel(cs: &mut Vec<char>) {
    let mut i = 0;
    while i + 1 < cs.len() {
        i += match vowel_constraint(cs[i], cs[i + 1]) {
            InsertConstraint::Between => {
                cs.insert(i + 1, DOTTED_CIRCLE);
                3
            }
            InsertConstraint::MaybeAfter(c3) => {
                if i + 2 < cs.len() && cs[i + 2] == c3 {
                    cs.insert(i + 2, DOTTED_CIRCLE);
                    4
                } else {
                    2
                }
            }
            InsertConstraint::None => 1,
        }
    }
}

/// See the following link for the full list of prohibited vowel combinations:
///
/// <https://docs.microsoft.com/en-us/typography/script-development/use#independent-vowel-iv-plus-dependent-vowel-constraints-dv>
fn vowel_constraint(c1: char, c2: char) -> InsertConstraint {
    match (c1, c2) {
        // Devanagari
        ('\u{0905}', '\u{0946}') => InsertConstraint::Between,
        ('\u{0905}', '\u{093E}') => InsertConstraint::Between,
        ('\u{0909}', '\u{0941}') => InsertConstraint::Between,
        ('\u{090F}', '\u{0945}') => InsertConstraint::Between,
        ('\u{090F}', '\u{0946}') => InsertConstraint::Between,
        ('\u{090F}', '\u{0947}') => InsertConstraint::Between,
        ('\u{0905}', '\u{0949}') => InsertConstraint::Between,
        ('\u{0906}', '\u{0945}') => InsertConstraint::Between,
        ('\u{0905}', '\u{094A}') => InsertConstraint::Between,
        ('\u{0906}', '\u{0946}') => InsertConstraint::Between,
        ('\u{0905}', '\u{094B}') => InsertConstraint::Between,
        ('\u{0906}', '\u{0947}') => InsertConstraint::Between,
        ('\u{0905}', '\u{094C}') => InsertConstraint::Between,
        ('\u{0906}', '\u{0948}') => InsertConstraint::Between,
        ('\u{0905}', '\u{0945}') => InsertConstraint::Between,
        ('\u{0905}', '\u{093A}') => InsertConstraint::Between,
        ('\u{0905}', '\u{093B}') => InsertConstraint::Between,
        ('\u{0906}', '\u{093A}') => InsertConstraint::Between,
        ('\u{0905}', '\u{094F}') => InsertConstraint::Between,
        ('\u{0905}', '\u{0956}') => InsertConstraint::Between,
        ('\u{0905}', '\u{0957}') => InsertConstraint::Between,
        // Devanagari "Reph, Letter I"
        ('\u{0930}', '\u{094D}') => InsertConstraint::MaybeAfter('\u{0907}'),
        // Bengali
        ('\u{0985}', '\u{09BE}') => InsertConstraint::Between,
        ('\u{098B}', '\u{09C3}') => InsertConstraint::Between,
        ('\u{098C}', '\u{09E2}') => InsertConstraint::Between,
        // Gurmukhi
        ('\u{0A05}', '\u{0A3E}') => InsertConstraint::Between,
        ('\u{0A72}', '\u{0A3F}') => InsertConstraint::Between,
        ('\u{0A72}', '\u{0A40}') => InsertConstraint::Between,
        ('\u{0A73}', '\u{0A41}') => InsertConstraint::Between,
        ('\u{0A73}', '\u{0A42}') => InsertConstraint::Between,
        ('\u{0A72}', '\u{0A47}') => InsertConstraint::Between,
        ('\u{0A05}', '\u{0A48}') => InsertConstraint::Between,
        ('\u{0A73}', '\u{0A4B}') => InsertConstraint::Between,
        ('\u{0A05}', '\u{0A4C}') => InsertConstraint::Between,
        // Gujarati
        ('\u{0A85}', '\u{0ABE}') => InsertConstraint::Between,
        ('\u{0A85}', '\u{0AC5}') => InsertConstraint::Between,
        ('\u{0A85}', '\u{0AC7}') => InsertConstraint::Between,
        ('\u{0A85}', '\u{0AC8}') => InsertConstraint::Between,
        ('\u{0A85}', '\u{0AC9}') => InsertConstraint::Between,
        ('\u{0A85}', '\u{0ACB}') => InsertConstraint::Between,
        ('\u{0A85}', '\u{0ACC}') => InsertConstraint::Between,
        ('\u{0AC5}', '\u{0ABE}') => InsertConstraint::Between,
        // For the Gujarati triplets:
        //   * ('\u{0A85}', '\u{0ABE}', '\u{0AC5}')
        //   * ('\u{0A85}', '\u{0ABE}', '\u{0AC8}')
        // the constraining character is inserted between the
        // first two characters, and are therefore covered by
        // the ('\u{0A85}', '\u{0ABE}') arm
        // Oriya
        ('\u{0B05}', '\u{0B3E}') => InsertConstraint::Between,
        ('\u{0B0F}', '\u{0B57}') => InsertConstraint::Between,
        ('\u{0B13}', '\u{0B57}') => InsertConstraint::Between,
        // Telugu
        ('\u{0C12}', '\u{0C55}') => InsertConstraint::Between,
        ('\u{0C12}', '\u{0C4C}') => InsertConstraint::Between,
        ('\u{0C3F}', '\u{0C55}') => InsertConstraint::Between,
        ('\u{0C46}', '\u{0C55}') => InsertConstraint::Between,
        ('\u{0C4A}', '\u{0C55}') => InsertConstraint::Between,
        // Kannada
        ('\u{0C89}', '\u{0CBE}') => InsertConstraint::Between,
        ('\u{0C92}', '\u{0CCC}') => InsertConstraint::Between,
        ('\u{0C8B}', '\u{0CBE}') => InsertConstraint::Between,
        // Malayalam
        ('\u{0D07}', '\u{0D57}') => InsertConstraint::Between,
        ('\u{0D09}', '\u{0D57}') => InsertConstraint::Between,
        ('\u{0D0E}', '\u{0D46}') => InsertConstraint::Between,
        ('\u{0D12}', '\u{0D3E}') => InsertConstraint::Between,
        ('\u{0D12}', '\u{0D57}') => InsertConstraint::Between,
        // Sinhala
        ('\u{0D85}', '\u{0DCF}') => InsertConstraint::Between,
        ('\u{0D85}', '\u{0DD0}') => InsertConstraint::Between,
        ('\u{0D85}', '\u{0DD1}') => InsertConstraint::Between,
        ('\u{0D8B}', '\u{0DDF}') => InsertConstraint::Between,
        ('\u{0D8D}', '\u{0DD8}') => InsertConstraint::Between,
        ('\u{0D8F}', '\u{0DDF}') => InsertConstraint::Between,
        ('\u{0D91}', '\u{0DCA}') => InsertConstraint::Between,
        ('\u{0D91}', '\u{0DD9}') => InsertConstraint::Between,
        ('\u{0D91}', '\u{0DDA}') => InsertConstraint::Between,
        ('\u{0D91}', '\u{0DDC}') => InsertConstraint::Between,
        ('\u{0D91}', '\u{0DDD}') => InsertConstraint::Between,
        ('\u{0D94}', '\u{0DDF}') => InsertConstraint::Between,
        // Brahmi
        // Takri
        // Khudawadi
        // Tirhuta
        // Modi
        _ => InsertConstraint::None,
    }
}

/// A multi-part matra's constituent parts.
enum MatraSplit {
    /// Not a multi-part matra.
    None,
    /// Two-part matra.
    Two(char, char),
    /// Three-part matra.
    Three(char, char, char),
}

/// Decompose two- or three-part matras, as certain parts may be placed
/// in different positions relative to the base.
///
/// E.g. Bengali "Ka, Sign O" decomposes into "Ka, Sign E, Sign Aa", then
/// gets reordered to "Sign E, Ka, Sign Aa".
fn decompose_matra(cs: &mut Vec<char>) {
    let mut i = 0;
    while i < cs.len() {
        i += match split_matra(cs[i]) {
            MatraSplit::None => 1,
            MatraSplit::Two(c1, c2) => {
                cs[i] = c1;
                cs.insert(i + 1, c2);
                2
            }
            MatraSplit::Three(c1, c2, c3) => {
                cs[i] = c1;
                cs.insert(i + 1, c2);
                cs.insert(i + 2, c3);
                3
            }
        }
    }
}

fn split_matra(ch: char) -> MatraSplit {
    match ch {
        // Devanagari
        // Bengali
        '\u{09CB}' => MatraSplit::Two('\u{09C7}', '\u{09BE}'),
        '\u{09CC}' => MatraSplit::Two('\u{09C7}', '\u{09D7}'),
        // Gurmukhi
        // Gujarati
        // Oriya
        '\u{0B48}' => MatraSplit::Two('\u{0B47}', '\u{0B56}'),
        '\u{0B4B}' => MatraSplit::Two('\u{0B47}', '\u{0B3E}'),
        '\u{0B4C}' => MatraSplit::Two('\u{0B47}', '\u{0B57}'),
        // Tamil
        '\u{0BCA}' => MatraSplit::Two('\u{0BC6}', '\u{0BBE}'),
        '\u{0BCB}' => MatraSplit::Two('\u{0BC7}', '\u{0BBE}'),
        '\u{0BCC}' => MatraSplit::Two('\u{0BC6}', '\u{0BD7}'),
        // Telugu
        '\u{0C48}' => MatraSplit::Two('\u{0C46}', '\u{0C56}'),
        // Kannada
        '\u{0CC0}' => MatraSplit::Two('\u{0CBF}', '\u{0CD5}'),
        '\u{0CC7}' => MatraSplit::Two('\u{0CC6}', '\u{0CD5}'),
        '\u{0CC8}' => MatraSplit::Two('\u{0CC6}', '\u{0CD6}'),
        '\u{0CCA}' => MatraSplit::Two('\u{0CC6}', '\u{0CC2}'),
        '\u{0CCB}' => MatraSplit::Three('\u{0CC6}', '\u{0CC2}', '\u{0CD5}'),
        // Malayalam
        '\u{0D4A}' => MatraSplit::Two('\u{0D46}', '\u{0D3E}'),
        '\u{0D4B}' => MatraSplit::Two('\u{0D47}', '\u{0D3E}'),
        '\u{0D4C}' => MatraSplit::Two('\u{0D46}', '\u{0D57}'),
        // Sinhala
        '\u{0DDA}' => MatraSplit::Two('\u{0DD9}', '\u{0DCA}'),
        '\u{0DDC}' => MatraSplit::Two('\u{0DD9}', '\u{0DCF}'),
        '\u{0DDD}' => MatraSplit::Three('\u{0DD9}', '\u{0DCF}', '\u{0DCA}'),
        '\u{0DDE}' => MatraSplit::Two('\u{0DD9}', '\u{0DDF}'),
        _ => MatraSplit::None,
    }
}

/// Recompose Bengali "Ya, Nukta" sequences to "Yya".
///
/// HarfBuzz does this; we follow.
///
/// <https://github.com/n8willis/opentype-shaping-documents/issues/74>
fn recompose_bengali_ya_nukta(cs: &mut Vec<char>) {
    let mut i = 0;
    while i + 1 < cs.len() {
        if cs[i] == '\u{09AF}' && cs[i + 1] == '\u{09BC}' {
            cs[i] = '\u{09DF}';
            cs.remove(i + 1);
        }
        i += 1;
    }
}

/// For compatibility with legacy Kannada sequences, "Ra, Halant, ZWJ" must
/// behave like "Ra, ZWJ, Halant" such that if a consonant follows the "ZWJ"
/// (i.e. "Ra, Halant, ZWJ, Consonant"), it should take on a subjoined form.
///
/// <https://github.com/n8willis/opentype-shaping-documents/issues/61>
/// <https://github.com/harfbuzz/harfbuzz/issues/435>
fn reorder_kannada_ra_halant_zwj(cs: &mut [char]) {
    if cs.starts_with(&['\u{0CB0}', '\u{0CCD}', '\u{200D}']) {
        cs.swap(1, 2);
    }
}

/////////////////////////////////////////////////////////////////////////////
// Shaping
/////////////////////////////////////////////////////////////////////////////

#[derive(Clone)]
struct IndicData {
    pos: Option<Pos>,
    mask: FeatureMask,
}

impl GlyphData for IndicData {
    /// Merge semantics for IndicData. The values that get used in the merged
    /// glyph are the values belonging to the glyph with the higher merge
    /// precedence.
    ///
    /// Merge precedence:
    ///   1. SyllableBase
    ///   2. PrebaseConsonant
    ///   3. PostbaseConsonant (in practice, there should never be a situation
    ///      where a PostbaseConsonant glyph is merged into a PrebaseConsonant glyph)
    ///   4. !None
    ///   5. None (shouldn't happen - all glyphs should be tagged by this point)
    fn merge(data1: IndicData, data2: IndicData) -> IndicData {
        match (data1.pos, data2.pos) {
            (Some(Pos::SyllableBase), _) => data1,
            (_, Some(Pos::SyllableBase)) => data2,
            (Some(Pos::PrebaseConsonant), _) => data1,
            (_, Some(Pos::PrebaseConsonant)) => data2,
            (Some(Pos::PostbaseConsonant), _) => data1,
            (_, Some(Pos::PostbaseConsonant)) => data2,
            (_, None) => data1,
            (None, _) => data2,
            _ => data1, // Default
        }
    }
}

type RawGlyphIndic = RawGlyph<IndicData>;

impl RawGlyphIndic {
    fn is(&self, pred: impl FnOnce(char) -> bool) -> bool {
        match self.glyph_origin {
            GlyphOrigin::Char(c) => pred(c),
            GlyphOrigin::Direct => false,
        }
    }

    fn has_pos(&self, pos: Pos) -> bool {
        match self.extra_data.pos {
            Some(p) => p == pos,
            None => false,
        }
    }

    fn set_pos(&mut self, pos: Option<Pos>) {
        self.extra_data.pos = pos
    }

    fn replace_none_pos(&mut self, pos: Option<Pos>) {
        assert_eq!(self.extra_data.pos, None);
        self.set_pos(pos)
    }

    fn pos(&self) -> Option<Pos> {
        self.extra_data.pos
    }

    fn has_mask(&self, mask: FeatureMask) -> bool {
        self.extra_data.mask.contains(mask)
    }

    fn add_mask(&mut self, mask: FeatureMask) {
        self.extra_data.mask.insert(mask)
    }

    fn remove_mask(&mut self, mask: FeatureMask) {
        self.extra_data.mask.remove(mask)
    }
}

struct IndicShapingData<'tables> {
    gsub_cache: &'tables LayoutCache<GSUB>,
    gsub_table: &'tables LayoutTable<GSUB>,
    gdef_table: Option<&'tables GDEFTable>,
    langsys: &'tables LangSys,
    script_tag: u32,
    lang_tag: Option<u32>,
    script: Script,
    shaping_model: ShapingModel,
    feature_variations: Option<&'tables FeatureTableSubstitution<'tables>>,
}

impl IndicShapingData<'_> {
    fn feature_would_apply(
        &self,
        feature_tag: u32,
        glyphs: &[RawGlyphIndic],
        start_index: usize,
    ) -> Result<bool, ParseError> {
        gsub::gsub_feature_would_apply(
            self.gsub_cache,
            self.gsub_table,
            self.gdef_table,
            self.langsys,
            self.feature_variations,
            feature_tag,
            glyphs,
            start_index,
        )
    }

    fn get_lookups_cache_index(&self, mask: FeatureMask) -> Result<usize, ParseError> {
        gsub::get_lookups_cache_index(
            self.gsub_cache,
            self.script_tag,
            self.lang_tag,
            self.feature_variations,
            mask,
        )
    }

    fn apply_lookup(
        &self,
        lookup_index: usize,
        feature_tag: u32,
        glyphs: &mut Vec<RawGlyphIndic>,
        max_glyphs: usize,
        pred: impl Fn(&RawGlyphIndic) -> bool,
    ) -> Result<(), ParseError> {
        gsub::gsub_apply_lookup(
            self.gsub_cache,
            self.gsub_table,
            self.gdef_table,
            lookup_index,
            feature_tag,
            None,
            glyphs,
            max_glyphs,
            0,
            glyphs.len(),
            pred,
        )?;
        Ok(())
    }
}

/// Does the following:
///   * Splits syllables
///   * Inserts dotted circles into broken syllables
///   * Initial reordering
///   * Applies basic features
///   * Final reordering
///   * Applies presentation features
pub fn gsub_apply_indic<'a>(
    dotted_circle_index: u16,
    gsub_cache: &'a LayoutCache<GSUB>,
    gsub_table: &'a LayoutTable<GSUB>,
    gdef_table: Option<&'a GDEFTable>,
    indic1_tag: u32,
    lang_tag: Option<u32>,
    feature_variations: Option<&'a FeatureTableSubstitution<'a>>,
    glyphs: &mut Vec<RawGlyph<()>>,
) -> Result<(), ShapingError> {
    if glyphs.is_empty() {
        return Err(ComplexScriptError::EmptyBuffer.into());
    }

    // Currently, the script tag that gets passed from Mercury is the Indic1 tag.
    // Map this to the Indic2 tag, as we want to check if a font supports it
    let indic2_tag = indic2_tag(indic1_tag);

    // Priority: Indic2 > Indic1 > Default
    let (script_tag, shaping_model, script_table) = match gsub_table.find_script(indic2_tag)? {
        Some(script_table) => (indic2_tag, ShapingModel::Indic2, script_table),
        None => match gsub_table.find_script_or_default(indic1_tag)? {
            Some(script_table) => (indic1_tag, ShapingModel::Indic1, script_table),
            None => return Ok(()),
        },
    };

    let langsys = match script_table.find_langsys_or_default(lang_tag)? {
        Some(langsys) => langsys,
        None => return Ok(()),
    };

    let mut syllables = to_indic_syllables(glyphs);
    let script = script(indic1_tag);
    let shaping_data = IndicShapingData {
        gsub_cache,
        gsub_table,
        gdef_table,
        langsys,
        script_tag,
        lang_tag,
        script,
        shaping_model,
        feature_variations,
    };

    for i in 0..syllables.len() {
        // For application of INIT. If a left matra is not word-initial,
        // HarfBuzz applies INIT iff the preceding character falls outside
        // a range of GeneralCategory classes. We follow suit.
        let is_first_syllable = if i == 0 {
            true
        } else if let Some(prev_glyph) = syllables[i - 1].0.iter().last() {
            match prev_glyph.glyph_origin {
                GlyphOrigin::Char(c) => {
                    let gc = unicode_general_category::get_general_category(c);
                    !(gc == GeneralCategory::Format
                        || gc == GeneralCategory::Unassigned
                        || gc == GeneralCategory::PrivateUse
                        || gc == GeneralCategory::Surrogate
                        || gc == GeneralCategory::LowercaseLetter
                        || gc == GeneralCategory::ModifierLetter
                        || gc == GeneralCategory::OtherLetter
                        || gc == GeneralCategory::TitlecaseLetter
                        || gc == GeneralCategory::UppercaseLetter
                        || gc == GeneralCategory::SpacingMark
                        || gc == GeneralCategory::EnclosingMark
                        || gc == GeneralCategory::NonspacingMark)
                }
                GlyphOrigin::Direct => false,
            }
        } else {
            true
        };

        let (syllable, syllable_type) = &mut syllables[i];
        if let Err(err) = shape_syllable(
            dotted_circle_index,
            &shaping_data,
            syllable,
            syllable_type,
            is_first_syllable,
        ) {
            debug!("gsub apply indic: {}", err);
        }
    }

    *glyphs = syllables
        .into_iter()
        .flat_map(|(s, _)| s.into_iter())
        .map(from_raw_glyph_indic)
        .collect();

    Ok(())
}

fn shape_syllable(
    dotted_circle_index: u16,
    shaping_data: &IndicShapingData<'_>,
    syllable: &mut Vec<RawGlyphIndic>,
    syllable_type: &Option<Syllable>,
    is_first_syllable: bool,
) -> Result<(), ShapingError> {
    let max_glyphs = syllable.len().saturating_mul(gsub::MAX_GLYPHS_FACTOR);

    // Add a dotted circle to broken syllables so they can be treated
    // like standalone syllables
    // https://github.com/n8willis/opentype-shaping-documents/issues/45
    if let Some(Syllable::Broken) = syllable_type {
        insert_dotted_circle(dotted_circle_index, shaping_data.script, syllable)?;
    }

    match syllable_type {
        // HarfBuzz treats vowel and standalone syllables like consonant
        // syllables. We follow suit
        // https://github.com/n8willis/opentype-shaping-documents/issues/45
        Some(Syllable::Consonant)
        | Some(Syllable::Vowel)
        | Some(Syllable::Standalone)
        | Some(Syllable::Broken) => {
            initial_reorder_consonant_syllable(shaping_data, syllable)?;
            apply_basic_features(shaping_data, syllable, max_glyphs)?;
            final_reorder_consonant_syllable(shaping_data, syllable);
            apply_presentation_features(shaping_data, is_first_syllable, syllable, max_glyphs)?;
        }
        Some(Syllable::Symbol) | None => {}
    }

    Ok(())
}

// https://github.com/n8willis/opentype-shaping-documents/issues/45
fn insert_dotted_circle(
    dotted_circle_index: u16,
    script: Script,
    glyphs: &mut Vec<RawGlyphIndic>,
) -> Result<(), ComplexScriptError> {
    if dotted_circle_index == 0 {
        return Err(ComplexScriptError::MissingDottedCircle);
    }

    let dotted_circle = RawGlyphIndic {
        unicodes: tiny_vec![[char; 1] => DOTTED_CIRCLE],
        glyph_index: dotted_circle_index,
        liga_component_pos: 0,
        glyph_origin: GlyphOrigin::Char(DOTTED_CIRCLE),
        flags: RawGlyphFlags::empty(),
        variation: None,
        extra_data: IndicData {
            pos: None,
            mask: FeatureMask::empty(),
        },
    };

    let mut pos = 0;
    if let (Script::Malayalam, Some(glyph)) = (script, glyphs.first()) {
        // Insert dotted circle after possible "Repha"
        if glyph.is(repha) {
            pos = 1;
        }
    }
    glyphs.insert(pos, dotted_circle);

    Ok(())
}

/// Maps an Indic1 script tag to its corresponding `Script` variant.
fn script(indic1_tag: u32) -> Script {
    match indic1_tag {
        tag::DEVA => Script::Devanagari,
        tag::BENG => Script::Bengali,
        tag::GURU => Script::Gurmukhi,
        tag::GUJR => Script::Gujarati,
        tag::ORYA => Script::Oriya,
        tag::TAML => Script::Tamil,
        tag::TELU => Script::Telugu,
        tag::KNDA => Script::Kannada,
        tag::MLYM => Script::Malayalam,
        tag::SINH => Script::Sinhala,
        _ => panic!("Expected an Indic1 script tag"),
    }
}

/// Maps an Indic1 script tag to its corresponding Indic2 script tag.
pub fn indic2_tag(indic1_tag: u32) -> u32 {
    match indic1_tag {
        tag::DEVA => tag::DEV2,
        tag::BENG => tag::BNG2,
        tag::GURU => tag::GUR2,
        tag::GUJR => tag::GJR2,
        tag::ORYA => tag::ORY2,
        tag::TAML => tag::TML2,
        tag::TELU => tag::TEL2,
        tag::KNDA => tag::KND2,
        tag::MLYM => tag::MLM2,
        tag::SINH => tag::SINH, // For simplicity, just return the Indic1 Sinhala tag
        _ => panic!("Expected an Indic1 script tag"),
    }
}

/// Splits the input glyph buffer and collects it into a vector of Indic syllables.
fn to_indic_syllables(mut glyphs: &[RawGlyph<()>]) -> Vec<(Vec<RawGlyphIndic>, Option<Syllable>)> {
    let mut syllables: Vec<(Vec<RawGlyphIndic>, Option<Syllable>)> = Vec::new();

    while !glyphs.is_empty() {
        let len = match match_syllable(glyphs) {
            Some((len, syllable_type)) => {
                assert_ne!(len, 0);

                let syllable = glyphs[..len].iter().map(to_raw_glyph_indic).collect();
                syllables.push((syllable, Some(syllable_type)));

                len
            }
            None => {
                let invalid_glyph = to_raw_glyph_indic(&glyphs[0]);
                match syllables.last_mut() {
                    // If the last syllable in `syllables` is invalid, just append
                    // this invalid glyph to that syllable
                    Some((invalid_syllable, None)) => invalid_syllable.push(invalid_glyph),
                    // Collect invalid glyphs
                    _ => syllables.push((vec![invalid_glyph], None)),
                }

                1
            }
        };

        glyphs = &glyphs[len..];
    }

    syllables
}

/////////////////////////////////////////////////////////////////////////////
// Initial reordering
/////////////////////////////////////////////////////////////////////////////

fn initial_reorder_consonant_syllable(
    shaping_data: &IndicShapingData<'_>,
    glyphs: &mut [RawGlyphIndic],
) -> Result<(), ShapingError> {
    // 2.1 Base consonant
    if let Some(base_index) = tag_consonants(shaping_data, glyphs)? {
        initial_reorder_consonant_syllable_with_base(shaping_data, base_index, glyphs)
    } else {
        initial_reorder_consonant_syllable_without_base(glyphs)
    }
}

fn initial_reorder_consonant_syllable_with_base(
    shaping_data: &IndicShapingData<'_>,
    base_index: usize,
    glyphs: &mut [RawGlyphIndic],
) -> Result<(), ShapingError> {
    // 2.2 Matra decomposition
    // IMPLEMENTATION: Handled in the text preprocessing stage.

    // 2.3 Tag decomposed matras
    let glyphs_without_pos = glyphs.iter_mut().filter(|g| g.pos().is_none());
    for glyph in glyphs_without_pos {
        if let GlyphOrigin::Char(c) = glyph.glyph_origin {
            let pos = matra_pos(c, shaping_data.script);
            glyph.replace_none_pos(pos);
        }
    }

    // 2.4 Adjacent marks
    // IMPLEMENTATION: Handled in the text preprocessing stage.

    // 2.5 Pre-base consonants
    // 2.6 Reph
    // 2.7 Post-base consonants
    // IMPLEMENTATION: Handled in 2.1

    // 2.8 Mark tagging
    fn smvd_mark(c: char) -> bool {
        match shaping_class(c) {
            Some(ShapingClass::Bindu)
            | Some(ShapingClass::Visarga)
            | Some(ShapingClass::Avagraha)
            | Some(ShapingClass::Cantillation)
            | Some(ShapingClass::SyllableModifier)
            | Some(ShapingClass::GeminationMark)
            | Some(ShapingClass::Symbol) => true,
            _ => false,
        }
    }

    fn remaining_mark(c: char) -> bool {
        match shaping_class(c) {
            Some(ShapingClass::Nukta)
            | Some(ShapingClass::Virama)
            | Some(ShapingClass::PureKiller)
            | Some(ShapingClass::Joiner)
            | Some(ShapingClass::NonJoiner) => true,
            _ => false,
        }
    }

    // 2.8.1 Marks in the BINDU, VISARGA, AVAGRAHA, CANTILLATION, SYLLABLE_MODIFIER,
    // GEMINATION_MARK, and SYMBOL categories should be tagged with POS_SMVD.
    let glyphs_smvd = glyphs.iter_mut().filter(|g| g.is(smvd_mark));
    for glyph in glyphs_smvd {
        let pos = match glyph.glyph_origin {
            // Oriya's "Candrabindu" must be tagged with POS_BEFORE_SUBJOINED
            GlyphOrigin::Char('\u{0B01}') => Pos::BeforeSubjoined,
            _ => Pos::SMVD,
        };
        glyph.replace_none_pos(Some(pos));
    }

    // 2.8.2 All remaining marks must be tagged with the same positioning tag as the
    // closest non-mark character the mark has affinity with, so that they move
    // together during the sorting step.
    //
    // NOTE: In this step, joiner and non-joiner characters must also be tagged
    // according to the same rules given for marks, even though these characters
    // are not categorized as marks in Unicode.
    let mut prev_pos = None;
    for i in 0..glyphs.len() {
        if glyphs[i].is(remaining_mark) && prev_pos.is_some() {
            // HarfBuzz and Uniscribe do not move a "Halant" if it follows
            // a pre-base matra
            //
            // https://github.com/n8willis/opentype-shaping-documents/issues/63
            if glyphs[i].is(halant) && prev_pos == Some(Pos::PrebaseMatra) {
                let first_non_matra_pos = glyphs[..i]
                    .iter()
                    .rev()
                    .filter_map(RawGlyphIndic::pos)
                    .find(|pos| *pos != Pos::PrebaseMatra);

                if first_non_matra_pos.is_some() {
                    glyphs[i].replace_none_pos(first_non_matra_pos);
                }
            } else {
                glyphs[i].replace_none_pos(prev_pos);
            }
        } else if !glyphs[i].is(smvd_mark) {
            assert_ne!(glyphs[i].pos(), None);
            prev_pos = glyphs[i].pos();
        }
    }

    // 2.8.3 For all marks preceding the base consonant, the mark must be tagged
    // with the same positioning tag as the closest preceding non-mark consonant.
    //
    // IMPLEMENTATION: Already tagged in 2.8.2

    // 2.8.4 For all marks occurring after the base consonant, the mark must be tagged
    // with the same positioning tag as the closest subsequent consonant.
    //
    // NOTE: In this step, joiner and non-joiner characters must also be tagged
    // according to the same rules given for marks, even though these characters
    // are not categorized as marks in Unicode.
    let mut next_pos = None;
    for glyph in glyphs[(base_index + 1)..].iter_mut().rev() {
        if glyph.is(remaining_mark) && next_pos.is_some() {
            // No assertion, as some marks may have already been tagged
            // in 2.8.2. Overwrite instead
            glyph.set_pos(next_pos);
        } else if glyph.is(effectively_consonant) {
            assert_ne!(glyph.pos(), None); // Consonant should be tagged by now
            next_pos = glyph.pos();
        }
    }

    // Check that no glyphs have been left untagged, then reorder glyphs
    // to canonical order
    if glyphs.iter().any(|g| g.pos().is_none()) {
        return Err(ComplexScriptError::MissingTags.into());
    } else {
        glyphs.sort_by_key(|g| g.pos());
    }

    // Get base consonant position again, after reorder
    let base_index = glyphs
        .iter()
        .position(|g| g.has_pos(Pos::SyllableBase))
        .ok_or_else::<ShapingError, _>(|| ComplexScriptError::MissingBaseConsonant.into())?;

    // Handle Indic1 script tags. Move the first post-base "Halant" after the last
    // post-base consonant
    if shaping_data.shaping_model == ShapingModel::Indic1 {
        let glyphs_post_base = &mut glyphs[(base_index + 1)..];
        let first_halant = glyphs_post_base.iter().position(|g| g.is(halant));
        let last_consonant = glyphs_post_base
            .iter()
            .rposition(|g| g.is(effectively_consonant));

        if let (Some(first_halant), Some(last_consonant)) = (first_halant, last_consonant) {
            // The comments in HarfBuzz state that for _some_ scripts, Uniscribe
            // does not move the "Halant" if a "Halant" already follows the last
            // post-base consonant. Kannada is one such script
            //
            // https://github.com/n8willis/opentype-shaping-documents/issues/64
            if shaping_data.script == Script::Kannada {
                let has_halant_after_last_consonant = glyphs_post_base[(last_consonant + 1)..]
                    .iter()
                    .rev()
                    .any(|g| g.is(halant));

                if !has_halant_after_last_consonant {
                    move_element(glyphs_post_base, first_halant, last_consonant);
                }
            } else {
                move_element(glyphs_post_base, first_halant, last_consonant);
            }
        }
    }

    // Set the appropriate feature masks
    for glyph in glyphs.iter_mut() {
        let mask = match glyph.pos() {
            Some(Pos::RaToBecomeReph) => BasicFeature::Rphf.mask(),
            Some(Pos::PrebaseConsonant) => {
                if shaping_data.shaping_model != ShapingModel::Indic1
                    && shaping_data.script.blwf_mode() == BlwfMode::PreAndPost
                {
                    BasicFeature::Half.mask() | BasicFeature::Blwf.mask()
                } else {
                    BasicFeature::Half.mask()
                }
            }
            Some(Pos::BelowbaseConsonant) => BasicFeature::Blwf.mask(),
            Some(Pos::PostbaseConsonant) => BasicFeature::Pstf.mask(),
            _ => FeatureMask::empty(),
        };

        glyph.add_mask(mask);
    }

    // Remove BLWF mask from pre-base sequences that end with "Halant, ZWJ"
    // There is reason to believe that Uniscribe does this.
    //
    // Example, using Noto Sans/Serif Bengali:
    //   [Ka, Halant, Ba, Halant, Ba, Halant, Ka (Base)]
    //     * [Ka, Halant] takes on half form
    //     * [Ba, Halant]s take on subjoined form
    //   [Ka, Halant, Ba, Halant, Ba, Halant, ZWJ, Ka (Base)]
    //     * [Ka, Halant] takes on half form
    //     * [Ba, Halant]s take on half form in Uniscribe
    let last_explicit_half_form_index = glyphs[..base_index]
        .windows(2)
        .rposition(|gs| gs[0].is(halant) && gs[1].is(zwj))
        .map(|i| i + 1); // ZWJ index

    if let Some(last_explicit_half_form_index) = last_explicit_half_form_index {
        glyphs[..=last_explicit_half_form_index]
            .iter_mut()
            .for_each(|g| g.remove_mask(BasicFeature::Blwf.mask()));
    }

    // ...except non-initial, pre-base "Ra, Halant" sequences in Devanagari
    // This is to allow the application of the VATU feature
    //
    // https://github.com/n8willis/opentype-shaping-documents/issues/65
    if shaping_data.shaping_model == ShapingModel::Indic1
        && shaping_data.script == Script::Devanagari
    {
        // Collect all pre-base "Ra" indices
        //
        // IMPLEMENTATION: Includes possible "Reph", but because
        // RPHF is applied before BLWF, it shouldn't matter
        let mut ra_indices = Vec::new();
        let mut iter = glyphs[..(base_index + 1)].windows(3).enumerate();
        while let Some((i, [g0, g1, g2])) = iter.next() {
            if g0.is(ra) && g1.is(halant) && !g2.is(zwj) {
                ra_indices.push(i)
            }
        }

        let mask = BasicFeature::Blwf.mask();
        for i in ra_indices {
            glyphs[i].add_mask(mask);
            glyphs[i + 1].add_mask(mask);
        }
    }

    // Add PREF mask to pre-base-reordering "Ra" sequences in Malayalam/Telugu
    if shaping_data.script == Script::Malayalam || shaping_data.script == Script::Telugu {
        let glyphs_post_base = &mut glyphs[(base_index + 1)..];

        // Find the first occurrence of pre-base-reordering "Ra".
        // Only one can exist per syllable
        let prebase_reordering_ra_index = match shaping_data.shaping_model {
            ShapingModel::Indic1 => glyphs_post_base
                .windows(2)
                .position(|gs| gs[0].is(ra) && gs[1].is(halant)),
            ShapingModel::Indic2 => glyphs_post_base
                .windows(2)
                .position(|gs| gs[0].is(halant) && gs[1].is(ra)),
        };

        if let Some(prebase_reordering_ra_index) = prebase_reordering_ra_index {
            if shaping_data.feature_would_apply(
                BasicFeature::Pref.tag(),
                glyphs_post_base,
                prebase_reordering_ra_index,
            )? {
                let mask = BasicFeature::Pref.mask();
                glyphs_post_base[prebase_reordering_ra_index].add_mask(mask);
                glyphs_post_base[prebase_reordering_ra_index + 1].add_mask(mask);
            }
        }
    }

    Ok(())
}

/// Handle consonant glyphs that lack a base consonant. Mimics Uniscribe's
/// behaviour.
///
/// Some examples to illustrate how Uniscribe's behaviour differs from HarfBuzz's:
///
/// ```text
///            Font: Noto Sans Bengali
///                  (or any Indic2 font with a Reph, subjoined, and half forms).
///
///
/// Test sequence 1: [Ka, Halant, Ba, Halant, ZWJ]
///
///        HarfBuzz: [Ka, Halant, Ba+Halant (BLWF), ZWJ]
///                  HB terminates the base consonant search on [Halant, ZWJ]. No
///                  base is found, and by default HB considers all consonants
///                  pre-base. `bng2` has the `BLWF_MODE_PRE_AND_POST` characteristic,
///                  therefore pre-base [Ba, Halant] takes on a subjoined form.
///
///       Uniscribe: [Ka+Halant (HALF), Ba+Halant+ZWJ (HALF)]
///                  Uniscribe appears to terminate the base consonant search
///                  too, but only applies the HALF feature to the syllable.
///
///
/// Test sequence 2: [Ra, Halant, Ba, Halant, ZWJ]
///
///        HarfBuzz: [Ra+Halant+Ba (BLWF+(CJCT|PRES)), Halant, ZWJ]
///                  On encountering a possible Reph, HB marks the Ra as a
///                  possible base (in the event that Ra is the only consonant).
///                  Base consonant search terminates on [Halant, ZWJ]. Ra
///                  remains the base; therefore post-base [Halant, Ba] takes
///                  on a subjoined form.
///
///       Uniscribe: [Ra+Halant (RPHF), Ba+Halant+ZWJ (HALF)]
///                  Uniscribe chooses to shape the Reph, and positions it
///                  on the Ba half form.
/// ```
fn initial_reorder_consonant_syllable_without_base(
    glyphs: &mut [RawGlyphIndic],
) -> Result<(), ShapingError> {
    // IMPLEMENTATION: Considering the analysis above:
    //
    // No reordering is necessary, therefore the only glyph that requires
    // a `Pos` tag is the syllable-initial Ra iff it is to form a Reph.
    // This is taken care of in `tag_consonants`.
    //
    // NOTE: Our GSUB implementation is such that the remaining glyphs
    // that constitute the Reph do not need to be tagged or masked.
    for glyph in glyphs.iter_mut() {
        let mask = match glyph.pos() {
            Some(Pos::RaToBecomeReph) => BasicFeature::Rphf.mask(),
            _ => BasicFeature::Half.mask(),
        };

        glyph.add_mask(mask);
    }

    Ok(())
}

/// Assign `Pos` tags to consonants in a syllable. Return the index of the base consonant, or `None`
/// if base consonant does not exist.
fn tag_consonants(
    shaping_data: &IndicShapingData<'_>,
    glyphs: &mut [RawGlyphIndic],
) -> Result<Option<usize>, ShapingError> {
    let has_reph = has_reph(shaping_data, glyphs)?;
    let start_prebase_index;
    if has_reph {
        start_prebase_index = match shaping_data.script.reph_mode() {
            RephMode::Implicit => 2,
            RephMode::Explicit => 3,
            RephMode::LogicalRepha => 1,
        };
        glyphs[0].replace_none_pos(Some(Pos::RaToBecomeReph));
    } else {
        start_prebase_index = 0;
    };

    let base_index = match shaping_data.script.base_consonant_pos() {
        BasePos::Last => {
            tag_postbase_consonants(shaping_data, start_prebase_index, has_reph, glyphs)
        }
        BasePos::LastSinhala => tag_postbase_consonants_sinhala(start_prebase_index, glyphs),
    }?;

    if shaping_data.script == Script::Gurmukhi {
        tag_consonant_medials(glyphs);
    }

    // Tag base and pre-base consonants.
    if let Some(base_index) = base_index {
        // No untagged assertion, as this potentially replaces `Pos::RaToBecomeReph`.
        glyphs[base_index].set_pos(Some(Pos::SyllableBase));
        if start_prebase_index < base_index {
            glyphs[start_prebase_index..base_index]
                .iter_mut()
                .filter(|g| g.is(effectively_consonant))
                .for_each(|g| g.replace_none_pos(Some(Pos::PrebaseConsonant)));
        }
    }

    Ok(base_index)
}

/// Assign `Pos` tags to post-base consonants (non-Sinhala scripts). Return the index of the base
/// consonant, or `None` if base consonant does not exist.
fn tag_postbase_consonants(
    shaping_data: &IndicShapingData<'_>,
    start_prebase_index: usize,
    has_reph: bool,
    glyphs: &mut [RawGlyphIndic],
) -> Result<Option<usize>, ShapingError> {
    let mut base_index = if has_reph {
        match shaping_data.script.reph_mode() {
            // "Ra" is still a base candidate if it is the only consonant in the syllable.
            RephMode::Implicit => Some(0),
            // "Ra" is never a base candidate, as "Reph" is always formed. (HarfBuzz, Uniscribe and
            // CoreText take this approach with Sinhala. Not sure about Telugu.)
            // https://github.com/n8willis/opentype-shaping-documents/issues/81.
            RephMode::Explicit => None,
            // "Repha" is not a consonant.
            RephMode::LogicalRepha => None,
        }
    } else {
        None
    };
    let mut i = glyphs.len() - 1;
    let mut seen_belowbase = false;

    while i >= start_prebase_index {
        if i == start_prebase_index {
            if glyphs[i].is(effectively_consonant) {
                base_index = Some(i);
            }
            break;
        }

        let j = i - 1;
        if glyphs[i].is(effectively_consonant) {
            if !glyphs[j].is(halant) {
                base_index = Some(i);
                break;
            }

            // HACK: Reorder "Halant, Consonant" to "Consonant, Halant" for Indic1 compatibility.
            if shaping_data.shaping_model == ShapingModel::Indic1 {
                glyphs.swap(i, j);
            }

            let pos = postbase_tag(shaping_data, seen_belowbase, glyphs, j)?;

            // HACK: Undo the reorder.
            if shaping_data.shaping_model == ShapingModel::Indic1 {
                glyphs.swap(i, j);
            }

            // A consonant cannot be base if it has a {below, post, pre}-base reordering form.
            if let Some(pos) = pos {
                glyphs[i].replace_none_pos(Some(pos));
                if pos == Pos::BelowbaseConsonant {
                    seen_belowbase = true;
                }
                i -= 2;
            } else {
                base_index = Some(i);
                break;
            }
        } else if glyphs[i].is(zwj) && glyphs[j].is(halant) {
            // Terminate base search on "Halant, ZWJ". Mimics HarfBuzz (and possibly Uniscribe).
            base_index = None;
            break;
        } else {
            i -= 1;
        }
    }

    Ok(base_index)
}

/// Assign `Pos` tags to post-base consonants (Sinhala). Return the index of the base consonant, or
/// `None` if base consonant does not exist.
fn tag_postbase_consonants_sinhala(
    start_prebase_index: usize,
    glyphs: &mut [RawGlyphIndic],
) -> Result<Option<usize>, ShapingError> {
    let mut base_index = None; // Sinhala is `RephMode:: Explicit`, so this is always `None`.
    let mut i = glyphs.len() - 1;

    while i >= start_prebase_index {
        if i == start_prebase_index {
            if glyphs[i].is(effectively_consonant) {
                base_index = Some(i);
            }
            break;
        }

        let j = i - 1;
        if glyphs[i].is(effectively_consonant) {
            // A consonant cannot be base if it is preceded by a "ZWJ". (In Sinhala text, this
            // sequence is used to specify the subjoined form of said consonant.)
            if glyphs[j].is(zwj) {
                glyphs[i].replace_none_pos(Some(Pos::BelowbaseConsonant));
            } else {
                base_index = Some(i);
                break;
            }
        }

        i -= 1;
    }

    Ok(base_index)
}

/// Return a `Pos` tag for a (possible) postbase consonant.
///
/// <https://github.com/n8willis/opentype-shaping-documents/issues/66>
fn postbase_tag(
    shaping_data: &IndicShapingData<'_>,
    seen_belowbase: bool,
    glyphs: &[RawGlyphIndic],
    start_index: usize,
) -> Result<Option<Pos>, ShapingError> {
    const FEATURE_POS_PAIRS: &[(BasicFeature, Pos)] = &[
        (BasicFeature::Blwf, Pos::BelowbaseConsonant),
        (BasicFeature::Pstf, Pos::PostbaseConsonant),
        (BasicFeature::Pref, Pos::PostbaseConsonant),
    ];

    let applicable_feature_pos_pairs = if seen_belowbase {
        // Post-base and pre-base-reordering forms must follow below-base forms
        &FEATURE_POS_PAIRS[..1]
    } else {
        // Pre-base reordering forms only occur in Malayalam and Telugu scripts
        match shaping_data.script {
            Script::Malayalam | Script::Telugu => FEATURE_POS_PAIRS,
            _ => &FEATURE_POS_PAIRS[..2],
        }
    };

    for (basic_feature, pos) in applicable_feature_pos_pairs {
        if shaping_data.feature_would_apply(basic_feature.tag(), glyphs, start_index)? {
            return Ok(Some(*pos));
        }
    }

    Ok(None)
}

/// Tag the only Indic consonant medial, Gurmukhi Yakash U+0A75, with
/// `Pos::BelowbaseConsonant`.
///
/// <https://github.com/n8willis/opentype-shaping-documents/issues/67>
fn tag_consonant_medials(glyphs: &mut [RawGlyphIndic]) {
    glyphs
        .iter_mut()
        .filter(|g| g.is(consonant_medial))
        .for_each(|g| g.replace_none_pos(Some(Pos::BelowbaseConsonant)))
}

/// For `RephMode::Implicit` and `RephMode::Explicit` scripts, check if the RPHF feature would
/// apply. For `RephMode::LogicalRepha` scripts, check for the existence of a syllable-initial
/// "Repha" code point.
fn has_reph(
    shaping_data: &IndicShapingData<'_>,
    glyphs: &[RawGlyphIndic],
) -> Result<bool, ShapingError> {
    match shaping_data.script.reph_mode() {
        RephMode::Implicit => match glyphs.get(..3) {
            // A "ZWJ" (or "ZWNJ") after a syllable-initial "Ra, Halant" inhibits "Reph" formation.
            Some([g0, g1, g2]) if g0.is(ra) && g1.is(halant) && !g2.is(joiner) => shaping_data
                .feature_would_apply(BasicFeature::Rphf.tag(), glyphs, 0)
                .map_err(|e| e.into()),
            Some(_) | None => Ok(false),
        },
        RephMode::Explicit => match glyphs.get(..3) {
            Some([g0, g1, g2]) if g0.is(ra) && g1.is(halant) && g2.is(zwj) => shaping_data
                .feature_would_apply(BasicFeature::Rphf.tag(), glyphs, 0)
                .map_err(|e| e.into()),
            Some(_) | None => Ok(false),
        },
        RephMode::LogicalRepha => glyphs
            .first()
            .map(|g| g.is(repha))
            .ok_or_else(|| ComplexScriptError::EmptyBuffer.into()),
    }
}

/// Return the final sort-order position of a matra.
///
/// Return `None` if the input character:
///   * is not a matra.
///   * is a non-decomposable, multi-part matra (unless specially handled).
fn matra_pos(c: char, script: Script) -> Option<Pos> {
    // Handle multi-part matras that lack a canonical Unicode decomposition
    // https://github.com/n8willis/opentype-shaping-documents/issues/62
    match c {
        '\u{0AC9}' => return Some(Pos::AfterPost), // Gujarati "Sign Candra O"
        '\u{0B57}' => return Some(Pos::AfterPost), // Oriya "Au Length Mark"
        _ => {}
    }

    match indic_character(c) {
        (Some(ShapingClass::VowelDependent), Some(mark_placement)) => match mark_placement {
            MarkPlacementSubclass::TopPosition => script.abovebase_matra_pos(),
            MarkPlacementSubclass::RightPosition => script.rightside_matra_pos(c),
            MarkPlacementSubclass::BottomPosition => Some(script.belowbase_matra_pos()),
            MarkPlacementSubclass::LeftPosition => Some(Pos::PrebaseMatra),
            _ => None,
        },
        _ => None,
    }
}

/////////////////////////////////////////////////////////////////////////////
// Basic substitution features
/////////////////////////////////////////////////////////////////////////////

/// Applies Indic basic features in their required order
fn apply_basic_features(
    shaping_data: &IndicShapingData<'_>,
    glyphs: &mut Vec<RawGlyphIndic>,
    max_glyphs: usize,
) -> Result<(), ParseError> {
    for feature in BasicFeature::ALL {
        let index = shaping_data.get_lookups_cache_index(feature.mask())?;
        let lookups = &shaping_data.gsub_cache.cached_lookups.lock().unwrap()[index];

        for &(lookup_index, feature_tag) in lookups {
            shaping_data.apply_lookup(lookup_index, feature_tag, glyphs, max_glyphs, |g| {
                feature.is_global() || g.has_mask(feature.mask())
            })?;
        }
    }

    Ok(())
}

/////////////////////////////////////////////////////////////////////////////
// Final reordering
/////////////////////////////////////////////////////////////////////////////

fn final_reorder_consonant_syllable(
    shaping_data: &IndicShapingData<'_>,
    glyphs: &mut [RawGlyphIndic],
) {
    // 4.1 Base consonant
    let mut opt_base_index = glyphs.iter().position(|g| g.has_pos(Pos::SyllableBase));

    // Finding the base consonant in Malayalam appears to require special treatment.
    // If there exists below-base consonants after the original base consonant that
    // haven't taken on subjoined form, the last of these below-base consonants is
    // the new base.
    //
    // Example, using the Nirmala font:
    //                 Syllable: [Ka, Halant, Tta, Halant, Na, Sign E]
    //
    //    After initial reorder: [Sign E, Ka, Halant, Tta, Halant, Na]
    //                           Ka is base, [Halant, Tta] and [Halant, Na] are marked below-base,
    //                           but both do not take on subjoined form.
    //
    //   HarfBuzz and Uniscribe: [Ka, Halant, Tta, Halant, Sign E, Na]
    //                           The Sign E matra is moved to before the Na, as it is the new base.
    //
    // IMPLEMENTATION: If a new base is found, the new `base_index` and the glyph
    // marked `Pos::SyllableBase` will be misaligned, but at this stage it shouldn't
    // matter.
    if let (Script::Malayalam, Some(base_index)) = (shaping_data.script, opt_base_index) {
        let start = base_index + 1;
        opt_base_index = glyphs[start..]
            .iter()
            .rposition(|g| g.is(effectively_consonant) && g.has_pos(Pos::BelowbaseConsonant))
            .map(|i| i + start)
            .or(opt_base_index);
    }

    // 4.2 Pre-base matras
    if let Some(base_index) = opt_base_index {
        // Find the start index of a contiguous sequence of `Pos::PrebaseMatra` glyphs
        let first_prebase_matra_index = glyphs[..base_index]
            .iter()
            .position(|g| g.has_pos(Pos::PrebaseMatra));

        // Find the end index of a contiguous sequence of `Pos::PrebaseMatra` glyphs
        let last_prebase_matra_index = glyphs[..base_index]
            .iter()
            .rposition(|g| g.has_pos(Pos::PrebaseMatra));

        if let (Some(first_prebase_matra_index), Some(last_prebase_matra_index)) =
            (first_prebase_matra_index, last_prebase_matra_index)
        {
            // Find the new start index for this sequence
            if let Some(final_prebase_matra_index) = final_pre_base_matra_index(
                shaping_data.script,
                last_prebase_matra_index,
                base_index,
                glyphs,
            ) {
                // Move the sequence
                glyphs[first_prebase_matra_index..=final_prebase_matra_index]
                    .rotate_left(last_prebase_matra_index - first_prebase_matra_index + 1);
            }
        }
    }

    // 4.3 Reph
    if let Some(final_reph_index) = final_reph_index(shaping_data.script, opt_base_index, glyphs) {
        move_element(glyphs, 0, final_reph_index);

        // Get new base index if Reph moves after the base
        opt_base_index = opt_base_index.map(|b| if b <= final_reph_index { b - 1 } else { b });
    }

    // 4.4 Pre-base-reordering consonants
    if let (Script::Malayalam, Some(base_index)) | (Script::Telugu, Some(base_index)) =
        (shaping_data.script, opt_base_index)
    {
        let mut pref_glyphs = glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.has_mask(BasicFeature::Pref.mask()));
        let pref_glyphs_count = pref_glyphs.clone().count();

        // Check that only one glyph has the PREF feature
        if let (Some((reordering_ra_index, _)), 1) = (pref_glyphs.next(), pref_glyphs_count) {
            let final_reordering_ra_index =
                final_pre_base_reordering_consonant_index(shaping_data.script, base_index, glyphs);

            move_element(glyphs, reordering_ra_index, final_reordering_ra_index);
        }
    }

    // 4.5 Initial matras
    // IMPLEMENTATION: Handled in `apply_presentation_features`
}

fn final_pre_base_matra_index(
    script: Script,
    last_prebase_matra_index: usize,
    base_index: usize,
    glyphs: &[RawGlyphIndic],
) -> Option<usize> {
    // Malayalam and Tamil do not have HALF forms or explicit "Halant" forms.
    // Malayalam typically uses the HALF feature for chillu substitutions, and it
    // appears that Tamil can use the HALF feature for forming _ligated_ explicit
    // "Halant" forms (the TAMu_Kalyani font does this).
    //
    // The pre-base matra should be positioned after these glyphs

    // https://github.com/n8willis/opentype-shaping-documents/issues/68
    if script == Script::Malayalam || script == Script::Tamil {
        return Some(base_index - 1);
    }

    // (1) The pre-base matra's final position is defined as: after the
    // last standalone "Halant" glyph that comes after the matra's starting
    // position and also comes before the main consonant
    //
    // (2) If a ZWJ or a ZWNJ follows this last standalone "Halant", the
    // final matra position is moved to after the joiner or non-joiner
    //
    // We don't follow (2). Instead, if a ZWJ follows this last standalone
    // "Halant", the final matra position should _not_ be after said "Halant"
    // https://github.com/n8willis/opentype-shaping-documents/issues/73
    //
    // IMPLEMENTATION: ZWNJ is taken care of by the syllable state machine.
    // "Halant, ZWNJ" is a terminating sequence for a consonant syllable; any
    // pre-base matras occurring after it belong to the subsequent syllable
    let start = last_prebase_matra_index + 1;
    glyphs[start..=base_index]
        .windows(2)
        .rposition(|gs| gs[0].is(halant) && !gs[1].is(zwj))
        .map(|i| i + start)
}

// Variant of `final_pre_base_matra_index`. Differences:
//   * doesn't special-case Tamil, as the script has no pre-base-reordering consonants
//   * positions the pre-base-reordering consonant after a "Halant, ZWJ"
//     https://github.com/n8willis/opentype-shaping-documents/issues/73
//   * has a default position immediately before the base consonant
fn final_pre_base_reordering_consonant_index(
    script: Script,
    base_index: usize,
    glyphs: &[RawGlyphIndic],
) -> usize {
    if script == Script::Malayalam {
        return base_index;
    }

    let mut iter = glyphs[..=base_index].windows(2).enumerate().rev();
    while let Some((i, [g0, g1])) = iter.next() {
        if g0.is(halant) {
            if g1.is(zwj) {
                return i + 2;
            }
            return i + 1;
        }
    }

    base_index
}

// At this stage, this step has become such a mish-mash of:
//   * the OpenType spec
//   * HarfBuzz's interpretation of the OpenType spec
//   * our spec
//   * comparison against CoreText's output
// that it really deserves to be called "Final Reph Pos As Decided by Adrian"
// https://github.com/n8willis/opentype-shaping-documents/issues/48
fn final_reph_index(
    script: Script,
    base_index: Option<usize>,
    glyphs: &[RawGlyphIndic],
) -> Option<usize> {
    // No "Reph", no problems
    if glyphs.len() < 2 || !glyphs[0].has_pos(Pos::RaToBecomeReph) {
        return None;
    }

    let reph_characteristic = script.reph_position();

    // This is "Reorder Reph" step 2/b in OpenType, which HarfBuzz implements
    // (and CoreText too, from empirical testing), but our spec doesn't.
    //
    // "If the "Reph" repositioning class is not after post-base: target position is after
    // the first explicit "Halant" glyph between the first post-reph consonant and last main
    // consonant. If "ZWJ" or "ZWNJ" are following this "Halant", position is moved after it.
    // If such position is found, this is the target position." ***
    // https://docs.microsoft.com/en-us/typography/script-development/devanagari#reorder-characters
    //
    // TEST: "Ra, Halant, Ra, Halant, Ya" using Noto Sans/Serif Devanagari.
    // Without this step, the "Reph" is positioned after the "Ya", when this step dictates that
    // it should move after the first explicit "Halant" between the "Reph" and base consonant
    //
    // *** There is evidence to believe that Uniscribe may still do this for the after post-base
    //     repositioning class, and HarfBuzz _definitely_ does it
    if let Some(base_index) = base_index {
        let start = 1;
        let mut iter = glyphs[start..=base_index].windows(2).enumerate();
        while let Some((i, [g0, g1])) = iter.next() {
            if g0.is(halant) {
                if g1.is(joiner) {
                    return Some(i + 1 + start);
                }
                return Some(i + start);
            }
        }
    }

    // This is where things start getting even more fantastic.
    //
    // For scripts that have the REPH_POS_BEFORE_POST characteristic, OpenType "Reorder Reph" step 4/d
    // states:
    //
    // "If "Reph" should be positioned before post-base consonant, find first post-base classified
    // consonant not ligated with main. If no consonant is found, the target position should be
    // before the first matra, syllable modifier sign or vedic sign."
    //
    // Our spec imitates OpenType. However, it looks like HarfBuzz and CoreText don't, and instead
    // jump straight to step 5/e:
    //
    // "If no consonant is found in 3/c or 4/d, move "Reph" to a position immediately before
    // the first post-base matra, syllable modifier sign or vedic sign ***that has a reordering
    // class after the intended "Reph" position***. For example, if the reordering position for
    // "Reph" is post-main, it will skip above-base matras that also have a post-main position."
    //
    // TEST: "Ra, Halant, Ka, Sign Aa" using Noto Sans/Serif Devanagari.
    // HarfBuzz and CoreText have the "Reph" positioned after the "Sign Aa" (which is marked
    // Pos::AfterSubjoined). If we followed our spec/OpenType, it gets positioned after the "Ka"

    // HarfBuzz **does** implement their interpretation of 4/d, but for whatever reason only applies
    // it to scripts that have the REPH_POS_AFTER_SUBJOINED characteristic.
    //
    // There is no explicit handling of REPH_POS_BEFORE_SUBJOINED in HarfBuzz

    // Biting the bullet and making this change so as to be consistent with HarfBuzz and
    // Uniscribe's (Gujarati) output. Sorry CoreText!
    //
    // For scripts with the REPH_POS_BEFORE_POST characteristic, position the "Reph" after
    // ALL post-base matras
    let reordering_class = match reph_characteristic {
        Pos::BeforePost => Some(Pos::AfterPost),
        _ => Some(reph_characteristic),
    };

    let new_index = glyphs
        .iter()
        .rposition(|g| g.pos() <= reordering_class)
        .unwrap_or(glyphs.len() - 1); // Fallback index == end of syllable

    // This step doesn't appear to be covered in OpenType, but is implemented in HarfBuzz and
    // appears to be implemented in CoreText. From our spec:
    //
    // "Finally, if the final position of "Reph" occurs after a "matra, Halant" subsequence, then
    // "Reph" must be repositioned to the left of "Halant", to allow for potential matching with
    // abvs or psts substitutions from GSUB."
    //
    // Our spec applies this to all "Reph" characteristics except REPH_POS_BEFORE_POST.
    // TEST: "Ra, Halant, Ka, O, Halant" in Noto Sans/Serif Devanagari (Devanagari incorporates
    // the REPH_POS_BEFORE_POST characteristic)
    match (glyphs.get(new_index - 1), glyphs.get(new_index)) {
        (Some(g0), Some(g1)) if g0.is(matra) && g1.is(halant) => Some(new_index - 1),
        _ => Some(new_index),
    }
}

/////////////////////////////////////////////////////////////////////////////
// Remaining substitution features
/////////////////////////////////////////////////////////////////////////////

/// Apply remaining substitution features after final reordering.
///
/// If the syllable is the first in a word, applies the INIT feature.
///
/// The order in which the remaining features are applied should be in
/// the order in which they appear in the GSUB table.
fn apply_presentation_features(
    shaping_data: &IndicShapingData<'_>,
    is_first_syllable: bool,
    glyphs: &mut Vec<RawGlyphIndic>,
    max_glyphs: usize,
) -> Result<(), ParseError> {
    let mut features = FeatureMask::PRES
        | FeatureMask::ABVS
        | FeatureMask::BLWS
        | FeatureMask::PSTS
        | FeatureMask::HALN
        | FeatureMask::CALT;

    if let Some(glyph) = glyphs.first_mut() {
        if is_first_syllable && glyph.has_pos(Pos::PrebaseMatra) {
            glyph.add_mask(FeatureMask::INIT);
            features |= FeatureMask::INIT;
        }
    }
    let index = shaping_data.get_lookups_cache_index(features)?;
    let lookups = &shaping_data.gsub_cache.cached_lookups.lock().unwrap()[index];

    for &(lookup_index, feature_tag) in lookups {
        shaping_data.apply_lookup(lookup_index, feature_tag, glyphs, max_glyphs, |g| {
            feature_tag != tag::INIT || g.has_mask(FeatureMask::INIT)
        })?;
    }

    Ok(())
}

/////////////////////////////////////////////////////////////////////////////
// Helper functions
/////////////////////////////////////////////////////////////////////////////

fn to_raw_glyph_indic(glyph: &RawGlyph<()>) -> RawGlyphIndic {
    RawGlyphIndic {
        unicodes: glyph.unicodes.clone(),
        glyph_index: glyph.glyph_index,
        liga_component_pos: glyph.liga_component_pos,
        glyph_origin: glyph.glyph_origin,
        flags: glyph.flags,
        variation: glyph.variation,
        extra_data: IndicData {
            pos: None,
            mask: FeatureMask::empty(),
        },
    }
}

fn from_raw_glyph_indic(glyph: RawGlyphIndic) -> RawGlyph<()> {
    RawGlyph {
        unicodes: glyph.unicodes,
        glyph_index: glyph.glyph_index,
        liga_component_pos: glyph.liga_component_pos,
        glyph_origin: glyph.glyph_origin,
        flags: glyph.flags,
        variation: glyph.variation,
        extra_data: (),
    }
}

/// Checks if a character is effectively an Indic consonant.
///
/// Gurmukhi's two `ConsonantPlaceholder` characters "Iri" and "Ura" are
/// considered consonants.
///
/// Kannada's two `ConsonantWithStacker` characters "Jihvamuliya" and
/// "Upadhmaniya" are considered consonants.
///
/// Also, HarfBuzz treats dotted circles, placeholders, and independent
/// vowels as consonants. We follow suit.
fn effectively_consonant(c: char) -> bool {
    match shaping_class(c) {
        Some(ShapingClass::Consonant)
        | Some(ShapingClass::ConsonantDead)
        | Some(ShapingClass::ConsonantPlaceholder)
        | Some(ShapingClass::ConsonantWithStacker)
        | Some(ShapingClass::DottedCircle)
        | Some(ShapingClass::Number)
        | Some(ShapingClass::Placeholder)
        | Some(ShapingClass::VowelIndependent) => true,
        _ => false,
    }
}

fn move_element<T>(slice: &mut [T], from: usize, to: usize) {
    if from < to {
        slice[from..=to].rotate_left(1);
    } else {
        slice[to..=from].rotate_right(1);
    }
}

/////////////////////////////////////////////////////////////////////////////
// Indic character tables
/////////////////////////////////////////////////////////////////////////////

#[rustfmt::skip]
fn indic_character(ch: char) -> (Option<ShapingClass>, Option<MarkPlacementSubclass>) {
    use MarkPlacementSubclass::*;
    use ShapingClass::*;

    match ch as u32 {
        // Devanagari character table
        0x0900 => (Some(Bindu), Some(TopPosition)),             // Inverted Candrabindu
        0x0901 => (Some(Bindu), Some(TopPosition)),             // Candrabindu
        0x0902 => (Some(Bindu), Some(TopPosition)),             // Anusvara
        0x0903 => (Some(Visarga), Some(RightPosition)),         // Visarga
        0x0904 => (Some(VowelIndependent), None),               // Short A
        0x0905 => (Some(VowelIndependent), None),               // A
        0x0906 => (Some(VowelIndependent), None),               // Aa
        0x0907 => (Some(VowelIndependent), None),               // I
        0x0908 => (Some(VowelIndependent), None),               // Ii
        0x0909 => (Some(VowelIndependent), None),               // U
        0x090A => (Some(VowelIndependent), None),               // Uu
        0x090B => (Some(VowelIndependent), None),               // Vocalic R
        0x090C => (Some(VowelIndependent), None),               // Vocalic L
        0x090D => (Some(VowelIndependent), None),               // Candra E
        0x090E => (Some(VowelIndependent), None),               // Short E
        0x090F => (Some(VowelIndependent), None),               // E
        0x0910 => (Some(VowelIndependent), None),               // Ai
        0x0911 => (Some(VowelIndependent), None),               // Candra O
        0x0912 => (Some(VowelIndependent), None),               // Short O
        0x0913 => (Some(VowelIndependent), None),               // O
        0x0914 => (Some(VowelIndependent), None),               // Au
        0x0915 => (Some(Consonant), None),                      // Ka
        0x0916 => (Some(Consonant), None),                      // Kha
        0x0917 => (Some(Consonant), None),                      // Ga
        0x0918 => (Some(Consonant), None),                      // Gha
        0x0919 => (Some(Consonant), None),                      // Nga
        0x091A => (Some(Consonant), None),                      // Ca
        0x091B => (Some(Consonant), None),                      // Cha
        0x091C => (Some(Consonant), None),                      // Ja
        0x091D => (Some(Consonant), None),                      // Jha
        0x091E => (Some(Consonant), None),                      // Nya
        0x091F => (Some(Consonant), None),                      // Tta
        0x0920 => (Some(Consonant), None),                      // Ttha
        0x0921 => (Some(Consonant), None),                      // Dda
        0x0922 => (Some(Consonant), None),                      // Ddha
        0x0923 => (Some(Consonant), None),                      // Nna
        0x0924 => (Some(Consonant), None),                      // Ta
        0x0925 => (Some(Consonant), None),                      // Tha
        0x0926 => (Some(Consonant), None),                      // Da
        0x0927 => (Some(Consonant), None),                      // Dha
        0x0928 => (Some(Consonant), None),                      // Na
        0x0929 => (Some(Consonant), None),                      // Nnna
        0x092A => (Some(Consonant), None),                      // Pa
        0x092B => (Some(Consonant), None),                      // Pha
        0x092C => (Some(Consonant), None),                      // Ba
        0x092D => (Some(Consonant), None),                      // Bha
        0x092E => (Some(Consonant), None),                      // Ma
        0x092F => (Some(Consonant), None),                      // Ya
        0x0930 => (Some(Consonant), None),                      // Ra
        0x0931 => (Some(Consonant), None),                      // Rra
        0x0932 => (Some(Consonant), None),                      // La
        0x0933 => (Some(Consonant), None),                      // Lla
        0x0934 => (Some(Consonant), None),                      // Llla
        0x0935 => (Some(Consonant), None),                      // Va
        0x0936 => (Some(Consonant), None),                      // Sha
        0x0937 => (Some(Consonant), None),                      // Ssa
        0x0938 => (Some(Consonant), None),                      // Sa
        0x0939 => (Some(Consonant), None),                      // Ha
        0x093A => (Some(VowelDependent), Some(TopPosition)),    // Sign Oe
        0x093B => (Some(VowelDependent), Some(RightPosition)),  // Sign Ooe
        0x093C => (Some(Nukta), Some(BottomPosition)),          // Nukta
        0x093D => (Some(Avagraha), None),                       // Avagraha
        0x093E => (Some(VowelDependent), Some(RightPosition)),  // Sign Aa
        0x093F => (Some(VowelDependent), Some(LeftPosition)),   // Sign I
        0x0940 => (Some(VowelDependent), Some(RightPosition)),  // Sign Ii
        0x0941 => (Some(VowelDependent), Some(BottomPosition)), // Sign U
        0x0942 => (Some(VowelDependent), Some(BottomPosition)), // Sign Uu
        0x0943 => (Some(VowelDependent), Some(BottomPosition)), // Sign Vocalic R
        0x0944 => (Some(VowelDependent), Some(BottomPosition)), // Sign Vocalic Rr
        0x0945 => (Some(VowelDependent), Some(TopPosition)),    // Sign Candra E
        0x0946 => (Some(VowelDependent), Some(TopPosition)),    // Sign Short E
        0x0947 => (Some(VowelDependent), Some(TopPosition)),    // Sign E
        0x0948 => (Some(VowelDependent), Some(TopPosition)),    // Sign Ai
        0x0949 => (Some(VowelDependent), Some(RightPosition)),  // Sign Candra O
        0x094A => (Some(VowelDependent), Some(RightPosition)),  // Sign Short O
        0x094B => (Some(VowelDependent), Some(RightPosition)),  // Sign O
        0x094C => (Some(VowelDependent), Some(RightPosition)),  // Sign Au
        0x094D => (Some(Virama), Some(BottomPosition)),         // Virama
        0x094E => (Some(VowelDependent), Some(LeftPosition)),   // Sign Prishthamatra E
        0x094F => (Some(VowelDependent), Some(RightPosition)),  // Sign Aw
        0x0950 => (None, None),                                 // Om
        0x0951 => (Some(Cantillation), Some(TopPosition)),      // Udatta
        0x0952 => (Some(Cantillation), Some(BottomPosition)),   // Anudatta
        0x0953 => (None, Some(TopPosition)),                    // Grave accent
        0x0954 => (None, Some(TopPosition)),                    // Acute accent
        0x0955 => (Some(VowelDependent), Some(TopPosition)),    // Sign Candra Long E
        0x0956 => (Some(VowelDependent), Some(BottomPosition)), // Sign Ue
        0x0957 => (Some(VowelDependent), Some(BottomPosition)), // Sign Uue
        0x0958 => (Some(Consonant), None),                      // Qa
        0x0959 => (Some(Consonant), None),                      // Khha
        0x095A => (Some(Consonant), None),                      // Ghha
        0x095B => (Some(Consonant), None),                      // Za
        0x095C => (Some(Consonant), None),                      // Dddha
        0x095D => (Some(Consonant), None),                      // Rha
        0x095E => (Some(Consonant), None),                      // Fa
        0x095F => (Some(Consonant), None),                      // Yya
        0x0960 => (Some(VowelIndependent), None),               // Vocalic Rr
        0x0961 => (Some(VowelIndependent), None),               // Vocalic Ll
        0x0962 => (Some(VowelDependent), Some(BottomPosition)), // Sign Vocalic L
        0x0963 => (Some(VowelDependent), Some(BottomPosition)), // Sign Vocalic Ll
        0x0964 => (None, None),                                 // Danda
        0x0965 => (None, None),                                 // Double Danda
        0x0966 => (Some(Number), None),                         // Digit Zero
        0x0967 => (Some(Number), None),                         // Digit One
        0x0968 => (Some(Number), None),                         // Digit Two
        0x0969 => (Some(Number), None),                         // Digit Three
        0x096A => (Some(Number), None),                         // Digit Four
        0x096B => (Some(Number), None),                         // Digit Five
        0x096C => (Some(Number), None),                         // Digit Six
        0x096D => (Some(Number), None),                         // Digit Seven
        0x096E => (Some(Number), None),                         // Digit Eight
        0x096F => (Some(Number), None),                         // Digit Nine
        0x0970 => (None, None),                                 // Abbreviation Sign
        0x0971 => (None, None),                                 // Sign High Spacing Dot
        0x0972 => (Some(VowelIndependent), None),               // Candra Aa
        0x0973 => (Some(VowelIndependent), None),               // Oe
        0x0974 => (Some(VowelIndependent), None),               // Ooe
        0x0975 => (Some(VowelIndependent), None),               // Aw
        0x0976 => (Some(VowelIndependent), None),               // Ue
        0x0977 => (Some(VowelIndependent), None),               // Uue
        0x0978 => (Some(Consonant), None),                      // Marwari Dda
        0x0979 => (Some(Consonant), None),                      // Zha
        0x097A => (Some(Consonant), None),                      // Heavy Ya
        0x097B => (Some(Consonant), None),                      // Gga
        0x097C => (Some(Consonant), None),                      // Jja
        0x097D => (Some(Consonant), None),                      // Glottal Stop
        0x097E => (Some(Consonant), None),                      // Ddda
        0x097F => (Some(Consonant), None),                      // Bba

        // Bengali character table
        0x0980 => (Some(ConsonantPlaceholder), None),                 // Anji
        0x0981 => (Some(Bindu), Some(TopPosition)),                   // Candrabindu
        0x0982 => (Some(Bindu), Some(RightPosition)),                 // Anusvara
        0x0983 => (Some(Visarga), Some(RightPosition)),               // Visarga
        0x0984 => (None, None),                                       // unassigned
        0x0985 => (Some(VowelIndependent), None),                     // A
        0x0986 => (Some(VowelIndependent), None),                     // Aa
        0x0987 => (Some(VowelIndependent), None),                     // I
        0x0988 => (Some(VowelIndependent), None),                     // Ii
        0x0989 => (Some(VowelIndependent), None),                     // U
        0x098A => (Some(VowelIndependent), None),                     // Uu
        0x098B => (Some(VowelIndependent), None),                     // Vocalic R
        0x098C => (Some(VowelIndependent), None),                     // Vocalic L
        0x098D => (None, None),                                       // unassigned
        0x098E => (None, None),                                       // unassigned
        0x098F => (Some(VowelIndependent), None),                     // E
        0x0990 => (Some(VowelIndependent), None),                     // Ai
        0x0991 => (None, None),                                       // unassigned
        0x0992 => (None, None),                                       // unassigned
        0x0993 => (Some(VowelIndependent), None),                     // O
        0x0994 => (Some(VowelIndependent), None),                     // Au
        0x0995 => (Some(Consonant), None),                            // Ka
        0x0996 => (Some(Consonant), None),                            // Kha
        0x0997 => (Some(Consonant), None),                            // Ga
        0x0998 => (Some(Consonant), None),                            // Gha
        0x0999 => (Some(Consonant), None),                            // Nga
        0x099A => (Some(Consonant), None),                            // Ca
        0x099B => (Some(Consonant), None),                            // Cha
        0x099C => (Some(Consonant), None),                            // Ja
        0x099D => (Some(Consonant), None),                            // Jha
        0x099E => (Some(Consonant), None),                            // Nya
        0x099F => (Some(Consonant), None),                            // Tta
        0x09A0 => (Some(Consonant), None),                            // Ttha
        0x09A1 => (Some(Consonant), None),                            // Dda
        0x09A2 => (Some(Consonant), None),                            // Ddha
        0x09A3 => (Some(Consonant), None),                            // Nna
        0x09A4 => (Some(Consonant), None),                            // Ta
        0x09A5 => (Some(Consonant), None),                            // Tha
        0x09A6 => (Some(Consonant), None),                            // Da
        0x09A7 => (Some(Consonant), None),                            // Dha
        0x09A8 => (Some(Consonant), None),                            // Na
        0x09A9 => (None, None),                                       // unassigned
        0x09AA => (Some(Consonant), None),                            // Pa
        0x09AB => (Some(Consonant), None),                            // Pha
        0x09AC => (Some(Consonant), None),                            // Ba
        0x09AD => (Some(Consonant), None),                            // Bha
        0x09AE => (Some(Consonant), None),                            // Ma
        0x09AF => (Some(Consonant), None),                            // Ya
        0x09B0 => (Some(Consonant), None),                            // Ra
        0x09B1 => (None, None),                                       // unassigned
        0x09B2 => (Some(Consonant), None),                            // La
        0x09B3 => (None, None),                                       // unassigned
        0x09B4 => (None, None),                                       // unassigned
        0x09B5 => (None, None),                                       // unassigned
        0x09B6 => (Some(Consonant), None),                            // Sha
        0x09B7 => (Some(Consonant), None),                            // Ssa
        0x09B8 => (Some(Consonant), None),                            // Sa
        0x09B9 => (Some(Consonant), None),                            // Ha
        0x09BA => (None, None),                                       // unassigned
        0x09BB => (None, None),                                       // unassigned
        0x09BC => (Some(Nukta), Some(BottomPosition)),                // Nukta
        0x09BD => (Some(Avagraha), None),                             // Avagraha
        0x09BE => (Some(VowelDependent), Some(RightPosition)),        // Sign Aa
        0x09BF => (Some(VowelDependent), Some(LeftPosition)),         // Sign I
        0x09C0 => (Some(VowelDependent), Some(RightPosition)),        // Sign Ii
        0x09C1 => (Some(VowelDependent), Some(BottomPosition)),       // Sign U
        0x09C2 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Uu
        0x09C3 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic R
        0x09C4 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic Rr
        0x09C5 => (None, None),                                       // unassigned
        0x09C6 => (None, None),                                       // unassigned
        0x09C7 => (Some(VowelDependent), Some(LeftPosition)),         // Sign E
        0x09C8 => (Some(VowelDependent), Some(LeftPosition)),         // Sign Ai
        0x09C9 => (None, None),                                       // unassigned
        0x09CA => (None, None),                                       // unassigned
        0x09CB => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign O
        0x09CC => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign Au
        0x09CD => (Some(Virama), Some(BottomPosition)),               // Virama
        0x09CE => (Some(ConsonantDead), None),                        // Khanda Ta
        0x09CF => (None, None),                                       // unassigned
        0x09D0 => (None, None),                                       // unassigned
        0x09D1 => (None, None),                                       // unassigned
        0x09D2 => (None, None),                                       // unassigned
        0x09D3 => (None, None),                                       // unassigned
        0x09D4 => (None, None),                                       // unassigned
        0x09D5 => (None, None),                                       // unassigned
        0x09D6 => (None, None),                                       // unassigned
        0x09D7 => (Some(VowelDependent), Some(RightPosition)),        // Au Length Mark
        0x09D8 => (None, None),                                       // unassigned
        0x09D9 => (None, None),                                       // unassigned
        0x09DA => (None, None),                                       // unassigned
        0x09DB => (None, None),                                       // unassigned
        0x09DC => (Some(Consonant), None),                            // Rra
        0x09DD => (Some(Consonant), None),                            // Rha
        0x09DE => (None, None),                                       // unassigned
        0x09DF => (Some(Consonant), None),                            // Yya
        0x09E0 => (Some(VowelIndependent), None),                     // Vocalic Rr
        0x09E1 => (Some(VowelIndependent), None),                     // Vocalic Ll
        0x09E2 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic L
        0x09E3 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic Ll
        0x09E4 => (None, None),                                       // unassigned
        0x09E5 => (None, None),                                       // unassigned
        0x09E6 => (Some(Number), None),                               // Digit Zero
        0x09E7 => (Some(Number), None),                               // Digit One
        0x09E8 => (Some(Number), None),                               // Digit Two
        0x09E9 => (Some(Number), None),                               // Digit Three
        0x09EA => (Some(Number), None),                               // Digit Four
        0x09EB => (Some(Number), None),                               // Digit Five
        0x09EC => (Some(Number), None),                               // Digit Six
        0x09ED => (Some(Number), None),                               // Digit Seven
        0x09EE => (Some(Number), None),                               // Digit Eight
        0x09EF => (Some(Number), None),                               // Digit Nine
        0x09F0 => (Some(Consonant), None),                            // Assamese Ra
        0x09F1 => (Some(Consonant), None),                            // Assamese Wa
        0x09F2 => (Some(Symbol), None),                               // Rupee Mark
        0x09F3 => (Some(Symbol), None),                               // Rupee Sign
        0x09F4 => (Some(Number), None),                               // Numerator One
        0x09F5 => (Some(Number), None),                               // Numerator Two
        0x09F6 => (Some(Number), None),                               // Numerator Three
        0x09F7 => (Some(Number), None),                               // Numerator Four
        0x09F8 => (Some(Number), None),                               // Numerator One Less Than Denominator
        0x09F9 => (Some(Number), None),                               // Denominator Sixteen
        0x09FA => (Some(Symbol), None),                               // Isshar
        0x09FB => (Some(Symbol), None),                               // Ganda Mark
        0x09FC => (None, None),                                       // Vedic Anusvara
        0x09FD => (None, None),                                       // Abbreviation Sign
        0x09FE => (Some(SyllableModifier), Some(TopPosition)),        // Sandhi Mark

        // Gurmukhi character table
        0x0A00 => (None, None),                                  // unassigned
        0x0A01 => (Some(Bindu), Some(TopPosition)),              // Adak Bindi
        0x0A02 => (Some(Bindu), Some(TopPosition)),              // Bindi
        0x0A03 => (Some(Visarga), Some(RightPosition)),          // Visarga
        0x0A04 => (None, None),                                  // unassigned
        0x0A05 => (Some(VowelIndependent), None),                // A
        0x0A06 => (Some(VowelIndependent), None),                // Aa
        0x0A07 => (Some(VowelIndependent), None),                // I
        0x0A08 => (Some(VowelIndependent), None),                // Ii
        0x0A09 => (Some(VowelIndependent), None),                // U
        0x0A0A => (Some(VowelIndependent), None),                // Uu
        0x0A0B => (None, None),                                  // unassigned
        0x0A0C => (None, None),                                  // unassigned
        0x0A0D => (None, None),                                  // unassigned
        0x0A0E => (None, None),                                  // unassigned
        0x0A0F => (Some(VowelIndependent), None),                // Ee
        0x0A10 => (Some(VowelIndependent), None),                // Ai
        0x0A11 => (None, None),                                  // unassigned
        0x0A12 => (None, None),                                  // unassigned
        0x0A13 => (Some(VowelIndependent), None),                // Oo
        0x0A14 => (Some(VowelIndependent), None),                // Au
        0x0A15 => (Some(Consonant), None),                       // Ka
        0x0A16 => (Some(Consonant), None),                       // Kha
        0x0A17 => (Some(Consonant), None),                       // Ga
        0x0A18 => (Some(Consonant), None),                       // Gha
        0x0A19 => (Some(Consonant), None),                       // Nga
        0x0A1A => (Some(Consonant), None),                       // Ca
        0x0A1B => (Some(Consonant), None),                       // Cha
        0x0A1C => (Some(Consonant), None),                       // Ja
        0x0A1D => (Some(Consonant), None),                       // Jha
        0x0A1E => (Some(Consonant), None),                       // Nya
        0x0A1F => (Some(Consonant), None),                       // Tta
        0x0A20 => (Some(Consonant), None),                       // Ttha
        0x0A21 => (Some(Consonant), None),                       // Dda
        0x0A22 => (Some(Consonant), None),                       // Ddha
        0x0A23 => (Some(Consonant), None),                       // Nna
        0x0A24 => (Some(Consonant), None),                       // Ta
        0x0A25 => (Some(Consonant), None),                       // Tha
        0x0A26 => (Some(Consonant), None),                       // Da
        0x0A27 => (Some(Consonant), None),                       // Dha
        0x0A28 => (Some(Consonant), None),                       // Na
        0x0A29 => (None, None),                                  // unassigned
        0x0A2A => (Some(Consonant), None),                       // Pa
        0x0A2B => (Some(Consonant), None),                       // Pha
        0x0A2C => (Some(Consonant), None),                       // Ba
        0x0A2D => (Some(Consonant), None),                       // Bha
        0x0A2E => (Some(Consonant), None),                       // Ma
        0x0A2F => (Some(Consonant), None),                       // Ya
        0x0A30 => (Some(Consonant), None),                       // Ra
        0x0A31 => (None, None),                                  // unassigned
        0x0A32 => (Some(Consonant), None),                       // La
        0x0A33 => (Some(Consonant), None),                       // Lla
        0x0A34 => (None, None),                                  // unassigned
        0x0A35 => (Some(Consonant), None),                       // Va
        0x0A36 => (Some(Consonant), None),                       // Sha
        0x0A37 => (None, None),                                  // unassigned
        0x0A38 => (Some(Consonant), None),                       // Sa
        0x0A39 => (Some(Consonant), None),                       // Ha
        0x0A3A => (None, None),                                  // unassigned
        0x0A3B => (None, None),                                  // unassigned
        0x0A3C => (Some(Nukta), Some(BottomPosition)),           // Nukta
        0x0A3D => (None, None),                                  // unassigned
        0x0A3E => (Some(VowelDependent), Some(RightPosition)),   // Sign Aa
        0x0A3F => (Some(VowelDependent), Some(LeftPosition)),    // Sign I
        0x0A40 => (Some(VowelDependent), Some(RightPosition)),   // Sign Ii
        0x0A41 => (Some(VowelDependent), Some(BottomPosition)),  // Sign U
        0x0A42 => (Some(VowelDependent), Some(BottomPosition)),  // Sign Uu
        0x0A43 => (None, None),                                  // unassigned
        0x0A44 => (None, None),                                  // unassigned
        0x0A45 => (None, None),                                  // unassigned
        0x0A46 => (None, None),                                  // unassigned
        0x0A47 => (Some(VowelDependent), Some(TopPosition)),     // Sign Ee
        0x0A48 => (Some(VowelDependent), Some(TopPosition)),     // Sign Ai
        0x0A49 => (None, None),                                  // unassigned
        0x0A4A => (None, None),                                  // unassigned
        0x0A4B => (Some(VowelDependent), Some(TopPosition)),     // Sign Oo
        0x0A4C => (Some(VowelDependent), Some(TopPosition)),     // Sign Au
        0x0A4D => (Some(Virama), Some(BottomPosition)),          // Virama
        0x0A4E => (None, None),                                  // unassigned
        0x0A4F => (None, None),                                  // unassigned
        0x0A50 => (None, None),                                  // unassigned
        0x0A51 => (Some(Cantillation), None),                    // Udaat
        0x0A52 => (None, None),                                  // unassigned
        0x0A53 => (None, None),                                  // unassigned
        0x0A54 => (None, None),                                  // unassigned
        0x0A55 => (None, None),                                  // unassigned
        0x0A56 => (None, None),                                  // unassigned
        0x0A57 => (None, None),                                  // unassigned
        0x0A58 => (None, None),                                  // unassigned
        0x0A59 => (Some(Consonant), None),                       // Khha
        0x0A5A => (Some(Consonant), None),                       // Ghha
        0x0A5B => (Some(Consonant), None),                       // Za
        0x0A5C => (Some(Consonant), None),                       // Rra
        0x0A5D => (None, None),                                  // unassigned
        0x0A5E => (Some(Consonant), None),                       // Fa
        0x0A5F => (None, None),                                  // unassigned
        0x0A60 => (None, None),                                  // unassigned
        0x0A61 => (None, None),                                  // unassigned
        0x0A62 => (None, None),                                  // unassigned
        0x0A63 => (None, None),                                  // unassigned
        0x0A64 => (None, None),                                  // unassigned
        0x0A65 => (None, None),                                  // unassigned
        0x0A66 => (Some(Number), None),                          // Digit Zero
        0x0A67 => (Some(Number), None),                          // Digit One
        0x0A68 => (Some(Number), None),                          // Digit Two
        0x0A69 => (Some(Number), None),                          // Digit Three
        0x0A6A => (Some(Number), None),                          // Digit Four
        0x0A6B => (Some(Number), None),                          // Digit Five
        0x0A6C => (Some(Number), None),                          // Digit Six
        0x0A6D => (Some(Number), None),                          // Digit Seven
        0x0A6E => (Some(Number), None),                          // Digit Eight
        0x0A6F => (Some(Number), None),                          // Digit Nine
        0x0A70 => (Some(Bindu), Some(TopPosition)),              // Tippi
        0x0A71 => (Some(GeminationMark), Some(TopPosition)),     // Addak
        0x0A72 => (Some(ConsonantPlaceholder), None),            // Iri
        0x0A73 => (Some(ConsonantPlaceholder), None),            // Ura
        0x0A74 => (None, None),                                  // Ek Onkar
        0x0A75 => (Some(ConsonantMedial), Some(BottomPosition)), // Yakash
        0x0A76 => (None, None),                                  // Abbreviation Sign

        // Gujarati character table
        0x0A81 => (Some(Bindu), Some(TopPosition)),                  // Candrabindu
        0x0A82 => (Some(Bindu), Some(TopPosition)),                  // Anusvara
        0x0A83 => (Some(Visarga), Some(RightPosition)),              // Visarga
        0x0A84 => (None, None),                                      // unassigned
        0x0A85 => (Some(VowelIndependent), None),                    // A
        0x0A86 => (Some(VowelIndependent), None),                    // Aa
        0x0A87 => (Some(VowelIndependent), None),                    // I
        0x0A88 => (Some(VowelIndependent), None),                    // Ii
        0x0A89 => (Some(VowelIndependent), None),                    // U
        0x0A8A => (Some(VowelIndependent), None),                    // Uu
        0x0A8B => (Some(VowelIndependent), None),                    // Vocalic R
        0x0A8C => (Some(VowelIndependent), None),                    // Vocalic L
        0x0A8D => (Some(VowelIndependent), None),                    // Candra E
        0x0A8E => (None, None),                                      // unassigned
        0x0A8F => (Some(VowelIndependent), None),                    // E
        0x0A90 => (Some(VowelIndependent), None),                    // Ai
        0x0A91 => (Some(VowelIndependent), None),                    // Candra O
        0x0A92 => (None, None),                                      // unassigned
        0x0A93 => (Some(VowelIndependent), None),                    // O
        0x0A94 => (Some(VowelIndependent), None),                    // Au
        0x0A95 => (Some(Consonant), None),                           // Ka
        0x0A96 => (Some(Consonant), None),                           // Kha
        0x0A97 => (Some(Consonant), None),                           // Ga
        0x0A98 => (Some(Consonant), None),                           // Gha
        0x0A99 => (Some(Consonant), None),                           // Nga
        0x0A9A => (Some(Consonant), None),                           // Ca
        0x0A9B => (Some(Consonant), None),                           // Cha
        0x0A9C => (Some(Consonant), None),                           // Ja
        0x0A9D => (Some(Consonant), None),                           // Jha
        0x0A9E => (Some(Consonant), None),                           // Nya
        0x0A9F => (Some(Consonant), None),                           // Tta
        0x0AA0 => (Some(Consonant), None),                           // Ttha
        0x0AA1 => (Some(Consonant), None),                           // Dda
        0x0AA2 => (Some(Consonant), None),                           // Ddha
        0x0AA3 => (Some(Consonant), None),                           // Nna
        0x0AA4 => (Some(Consonant), None),                           // Ta
        0x0AA5 => (Some(Consonant), None),                           // Tha
        0x0AA6 => (Some(Consonant), None),                           // Da
        0x0AA7 => (Some(Consonant), None),                           // Dha
        0x0AA8 => (Some(Consonant), None),                           // Na
        0x0AA9 => (None, None),                                      // unassigned
        0x0AAA => (Some(Consonant), None),                           // Pa
        0x0AAB => (Some(Consonant), None),                           // Pha
        0x0AAC => (Some(Consonant), None),                           // Ba
        0x0AAD => (Some(Consonant), None),                           // Bha
        0x0AAE => (Some(Consonant), None),                           // Ma
        0x0AAF => (Some(Consonant), None),                           // Ya
        0x0AB0 => (Some(Consonant), None),                           // Ra
        0x0AB1 => (None, None),                                      // unassigned
        0x0AB2 => (Some(Consonant), None),                           // La
        0x0AB3 => (Some(Consonant), None),                           // Lla
        0x0AB4 => (None, None),                                      // unassigned
        0x0AB5 => (Some(Consonant), None),                           // Va
        0x0AB6 => (Some(Consonant), None),                           // Sha
        0x0AB7 => (Some(Consonant), None),                           // Ssa
        0x0AB8 => (Some(Consonant), None),                           // Sa
        0x0AB9 => (Some(Consonant), None),                           // Ha
        0x0ABA => (None, None),                                      // unassigned
        0x0ABB => (None, None),                                      // unassigned
        0x0ABC => (Some(Nukta), Some(BottomPosition)),               // Nukta
        0x0ABD => (Some(Avagraha), None),                            // Avagraha
        0x0ABE => (Some(VowelDependent), Some(RightPosition)),       // Sign Aa
        0x0ABF => (Some(VowelDependent), Some(LeftPosition)),        // Sign I
        0x0AC0 => (Some(VowelDependent), Some(RightPosition)),       // Sign Ii
        0x0AC1 => (Some(VowelDependent), Some(BottomPosition)),      // Sign U
        0x0AC2 => (Some(VowelDependent), Some(BottomPosition)),      // Sign Uu
        0x0AC3 => (Some(VowelDependent), Some(BottomPosition)),      // Sign Vocalic R
        0x0AC4 => (Some(VowelDependent), Some(BottomPosition)),      // Sign Vocalic Rr
        0x0AC5 => (Some(VowelDependent), Some(TopPosition)),         // Sign Candra E
        0x0AC6 => (None, None),                                      // unassigned
        0x0AC7 => (Some(VowelDependent), Some(TopPosition)),         // Sign E
        0x0AC8 => (Some(VowelDependent), Some(TopPosition)),         // Sign Ai
        0x0AC9 => (Some(VowelDependent), Some(TopAndRightPosition)), // Sign Candra O
        0x0ACA => (None, None),                                      // unassigned
        0x0ACB => (Some(VowelDependent), Some(RightPosition)),       // Sign O
        0x0ACC => (Some(VowelDependent), Some(RightPosition)),       // Sign Au
        0x0ACD => (Some(Virama), Some(BottomPosition)),              // Virama
        0x0ACE => (None, None),                                      // unassigned
        0x0ACF => (None, None),                                      // unassigned
        0x0AD0 => (None, None),                                      // Om
        0x0AD1 => (None, None),                                      // unassigned
        0x0AD2 => (None, None),                                      // unassigned
        0x0AD3 => (None, None),                                      // unassigned
        0x0AD4 => (None, None),                                      // unassigned
        0x0AD5 => (None, None),                                      // unassigned
        0x0AD6 => (None, None),                                      // unassigned
        0x0AD7 => (None, None),                                      // unassigned
        0x0AD8 => (None, None),                                      // unassigned
        0x0AD9 => (None, None),                                      // unassigned
        0x0ADA => (None, None),                                      // unassigned
        0x0ADB => (None, None),                                      // unassigned
        0x0ADC => (None, None),                                      // unassigned
        0x0ADD => (None, None),                                      // unassigned
        0x0ADE => (None, None),                                      // unassigned
        0x0ADF => (None, None),                                      // unassigned
        0x0AE0 => (Some(VowelIndependent), None),                    // Vocalic Rr
        0x0AE1 => (Some(VowelIndependent), None),                    // Vocalic Ll
        0x0AE2 => (Some(VowelDependent), Some(BottomPosition)),      // Sign Vocalic L
        0x0AE3 => (Some(VowelDependent), Some(BottomPosition)),      // Sign Vocalic Ll
        0x0AE4 => (None, None),                                      // unassigned
        0x0AE5 => (None, None),                                      // unassigned
        0x0AE6 => (Some(Number), None),                              // Digit Zero
        0x0AE7 => (Some(Number), None),                              // Digit One
        0x0AE8 => (Some(Number), None),                              // Digit Two
        0x0AE9 => (Some(Number), None),                              // Digit Three
        0x0AEA => (Some(Number), None),                              // Digit Four
        0x0AEB => (Some(Number), None),                              // Digit Five
        0x0AEC => (Some(Number), None),                              // Digit Six
        0x0AED => (Some(Number), None),                              // Digit Seven
        0x0AEE => (Some(Number), None),                              // Digit Eight
        0x0AEF => (Some(Number), None),                              // Digit Nine
        0x0AF0 => (Some(Symbol), None),                              // Abbreviation
        0x0AF1 => (Some(Symbol), None),                              // Rupee Sign
        0x0AF2 => (None, None),                                      // unassigned
        0x0AF3 => (None, None),                                      // unassigned
        0x0AF4 => (None, None),                                      // unassigned
        0x0AF5 => (None, None),                                      // unassigned
        0x0AF6 => (None, None),                                      // unassigned
        0x0AF7 => (None, None),                                      // unassigned
        0x0AF8 => (None, None),                                      // unassigned
        0x0AF9 => (Some(Consonant), None),                           // Zha
        0x0AFA => (Some(Cantillation), Some(TopPosition)),           // Sukun
        0x0AFB => (Some(Cantillation), Some(TopPosition)),           // Shadda
        0x0AFC => (Some(Cantillation), Some(TopPosition)),           // Maddah
        0x0AFD => (Some(Nukta), Some(TopPosition)),                  // Three-Dot Nukta Above
        0x0AFE => (Some(Nukta), Some(TopPosition)),                  // Circle Nukta Above
        0x0AFF => (Some(Nukta), Some(TopPosition)),                  // Two-Circle Nukta Above

        // Oriya character table
        0x0B00 => (None, None),                                          // unassigned
        0x0B01 => (Some(Bindu), Some(TopPosition)),                      // Candrabindu
        0x0B02 => (Some(Bindu), Some(RightPosition)),                    // Anusvara
        0x0B03 => (Some(Visarga), Some(RightPosition)),                  // Visarga
        0x0B04 => (None, None),                                          // unassigned
        0x0B05 => (Some(VowelIndependent), None),                        // A
        0x0B06 => (Some(VowelIndependent), None),                        // Aa
        0x0B07 => (Some(VowelIndependent), None),                        // I
        0x0B08 => (Some(VowelIndependent), None),                        // Ii
        0x0B09 => (Some(VowelIndependent), None),                        // U
        0x0B0A => (Some(VowelIndependent), None),                        // Uu
        0x0B0B => (Some(VowelIndependent), None),                        // Vocalic R
        0x0B0C => (Some(VowelIndependent), None),                        // Vocalic L
        0x0B0D => (None, None),                                          // unassigned
        0x0B0E => (None, None),                                          // unassigned
        0x0B0F => (Some(VowelIndependent), None),                        // E
        0x0B10 => (Some(VowelIndependent), None),                        // Ai
        0x0B11 => (None, None),                                          // unassigned
        0x0B12 => (None, None),                                          // unassigned
        0x0B13 => (Some(VowelIndependent), None),                        // O
        0x0B14 => (Some(VowelIndependent), None),                        // Au
        0x0B15 => (Some(Consonant), None),                               // Ka
        0x0B16 => (Some(Consonant), None),                               // Kha
        0x0B17 => (Some(Consonant), None),                               // Ga
        0x0B18 => (Some(Consonant), None),                               // Gha
        0x0B19 => (Some(Consonant), None),                               // Nga
        0x0B1A => (Some(Consonant), None),                               // Ca
        0x0B1B => (Some(Consonant), None),                               // Cha
        0x0B1C => (Some(Consonant), None),                               // Ja
        0x0B1D => (Some(Consonant), None),                               // Jha
        0x0B1E => (Some(Consonant), None),                               // Nya
        0x0B1F => (Some(Consonant), None),                               // Tta
        0x0B20 => (Some(Consonant), None),                               // Ttha
        0x0B21 => (Some(Consonant), None),                               // Dda
        0x0B22 => (Some(Consonant), None),                               // Ddha
        0x0B23 => (Some(Consonant), None),                               // Nna
        0x0B24 => (Some(Consonant), None),                               // Ta
        0x0B25 => (Some(Consonant), None),                               // Tha
        0x0B26 => (Some(Consonant), None),                               // Da
        0x0B27 => (Some(Consonant), None),                               // Dha
        0x0B28 => (Some(Consonant), None),                               // Na
        0x0B29 => (None, None),                                          // unassigned
        0x0B2A => (Some(Consonant), None),                               // Pa
        0x0B2B => (Some(Consonant), None),                               // Pha
        0x0B2C => (Some(Consonant), None),                               // Ba
        0x0B2D => (Some(Consonant), None),                               // Bha
        0x0B2E => (Some(Consonant), None),                               // Ma
        0x0B2F => (Some(Consonant), None),                               // Ya
        0x0B30 => (Some(Consonant), None),                               // Ra
        0x0B31 => (None, None),                                          // unassigned
        0x0B32 => (Some(Consonant), None),                               // La
        0x0B33 => (Some(Consonant), None),                               // Lla
        0x0B34 => (None, None),                                          // unassigned
        0x0B35 => (Some(Consonant), None),                               // Va
        0x0B36 => (Some(Consonant), None),                               // Sha
        0x0B37 => (Some(Consonant), None),                               // Ssa
        0x0B38 => (Some(Consonant), None),                               // Sa
        0x0B39 => (Some(Consonant), None),                               // Ha
        0x0B3A => (None, None),                                          // unassigned
        0x0B3B => (None, None),                                          // unassigned
        0x0B3C => (Some(Nukta), Some(BottomPosition)),                   // Nukta
        0x0B3D => (Some(Avagraha), None),                                // Avagraha
        0x0B3E => (Some(VowelDependent), Some(RightPosition)),           // Sign Aa
        0x0B3F => (Some(VowelDependent), Some(TopPosition)),             // Sign I
        0x0B40 => (Some(VowelDependent), Some(RightPosition)),           // Sign Ii
        0x0B41 => (Some(VowelDependent), Some(BottomPosition)),          // Sign U
        0x0B42 => (Some(VowelDependent), Some(BottomPosition)),          // Sign Uu
        0x0B43 => (Some(VowelDependent), Some(BottomPosition)),          // Sign Vocalic R
        0x0B44 => (Some(VowelDependent), Some(BottomPosition)),          // Sign Vocalic Rr
        0x0B45 => (None, None),                                          // unassigned
        0x0B46 => (None, None),                                          // unassigned
        0x0B47 => (Some(VowelDependent), Some(LeftPosition)),            // Sign E
        0x0B48 => (Some(VowelDependent), Some(TopAndLeftPosition)),      // Sign Ai
        0x0B49 => (None, None),                                          // unassigned
        0x0B4A => (None, None),                                          // unassigned
        0x0B4B => (Some(VowelDependent), Some(LeftAndRightPosition)),    // Sign O
        0x0B4C => (Some(VowelDependent), Some(TopLeftAndRightPosition)), // Sign Au
        0x0B4D => (Some(Virama), Some(BottomPosition)),                  // Virama
        0x0B4E => (None, None),                                          // unassigned
        0x0B4F => (None, None),                                          // unassigned
        0x0B50 => (None, None),                                          // unassigned
        0x0B51 => (None, None),                                          // unassigned
        0x0B52 => (None, None),                                          // unassigned
        0x0B53 => (None, None),                                          // unassigned
        0x0B54 => (None, None),                                          // unassigned
        0x0B55 => (None, None),                                          // unassigned
        0x0B56 => (Some(VowelDependent), Some(TopPosition)),             // Ai Length Mark
        0x0B57 => (Some(VowelDependent), Some(TopAndRightPosition)),     // Au Length Mark
        0x0B58 => (None, None),                                          // unassigned
        0x0B59 => (None, None),                                          // unassigned
        0x0B5A => (None, None),                                          // unassigned
        0x0B5B => (None, None),                                          // unassigned
        0x0B5C => (Some(Consonant), None),                               // Rra
        0x0B5D => (Some(Consonant), None),                               // Rha
        0x0B5E => (None, None),                                          // unassigned
        0x0B5F => (Some(Consonant), None),                               // Yya
        0x0B60 => (Some(VowelIndependent), None),                        // Vocalic Rr
        0x0B61 => (Some(VowelIndependent), None),                        // Vocalic Ll
        0x0B62 => (Some(VowelDependent), Some(BottomPosition)),          // Sign Vocalic L
        0x0B63 => (Some(VowelDependent), Some(BottomPosition)),          // Sign Vocalic Ll
        0x0B64 => (None, None),                                          // unassigned
        0x0B65 => (None, None),                                          // unassigned
        0x0B66 => (Some(Number), None),                                  // Digit Zero
        0x0B67 => (Some(Number), None),                                  // Digit One
        0x0B68 => (Some(Number), None),                                  // Digit Two
        0x0B69 => (Some(Number), None),                                  // Digit Three
        0x0B6A => (Some(Number), None),                                  // Digit Four
        0x0B6B => (Some(Number), None),                                  // Digit Five
        0x0B6C => (Some(Number), None),                                  // Digit Six
        0x0B6D => (Some(Number), None),                                  // Digit Seven
        0x0B6E => (Some(Number), None),                                  // Digit Eight
        0x0B6F => (Some(Number), None),                                  // Digit Nine
        0x0B70 => (Some(Symbol), None),                                  // Isshar
        0x0B71 => (Some(Consonant), None),                               // Wa
        0x0B72 => (Some(Number), None),                                  // Fraction 1/4
        0x0B73 => (Some(Number), None),                                  // Fraction 1/2
        0x0B74 => (Some(Number), None),                                  // Fraction 3/4
        0x0B75 => (Some(Number), None),                                  // Fraction 1/16
        0x0B76 => (Some(Number), None),                                  // Fraction 1/8
        0x0B77 => (Some(Number), None),                                  // Fraction 3/16
        0x0B78 => (None, None),                                          // unassigned
        0x0B79 => (None, None),                                          // unassigned
        0x0B7A => (None, None),                                          // unassigned
        0x0B7B => (None, None),                                          // unassigned
        0x0B7C => (None, None),                                          // unassigned
        0x0B7D => (None, None),                                          // unassigned
        0x0B7E => (None, None),                                          // unassigned
        0x0B7F => (None, None),                                          // unassigned

        // Tamil character table
        0x0B80 => (None, None),                                       // unassigned
        0x0B81 => (None, None),                                       // unassigned
        0x0B82 => (Some(Bindu), Some(TopPosition)),                   // Anusvara
        0x0B83 => (Some(ModifyingLetter), None),                      // Visarga
        0x0B84 => (None, None),                                       // unassigned
        0x0B85 => (Some(VowelIndependent), None),                     // A
        0x0B86 => (Some(VowelIndependent), None),                     // Aa
        0x0B87 => (Some(VowelIndependent), None),                     // I
        0x0B88 => (Some(VowelIndependent), None),                     // Ii
        0x0B89 => (Some(VowelIndependent), None),                     // U
        0x0B8A => (Some(VowelIndependent), None),                     // Uu
        0x0B8B => (None, None),                                       // unassigned
        0x0B8C => (None, None),                                       // unassigned
        0x0B8D => (None, None),                                       // unassigned
        0x0B8E => (Some(VowelIndependent), None),                     // E
        0x0B8F => (Some(VowelIndependent), None),                     // Ee
        0x0B90 => (Some(VowelIndependent), None),                     // Ai
        0x0B91 => (None, None),                                       // unassigned
        0x0B92 => (Some(VowelIndependent), None),                     // O
        0x0B93 => (Some(VowelIndependent), None),                     // Oo
        0x0B94 => (Some(VowelIndependent), None),                     // Au
        0x0B95 => (Some(Consonant), None),                            // Ka
        0x0B96 => (None, None),                                       // unassigned
        0x0B97 => (None, None),                                       // unassigned
        0x0B98 => (None, None),                                       // unassigned
        0x0B99 => (Some(Consonant), None),                            // Nga
        0x0B9A => (Some(Consonant), None),                            // Ca
        0x0B9B => (None, None),                                       // unassigned
        0x0B9C => (Some(Consonant), None),                            // Ja
        0x0B9D => (None, None),                                       // unassigned
        0x0B9E => (Some(Consonant), None),                            // Nya
        0x0B9F => (Some(Consonant), None),                            // Tta
        0x0BA0 => (None, None),                                       // unassigned
        0x0BA1 => (None, None),                                       // unassigned
        0x0BA2 => (None, None),                                       // unassigned
        0x0BA3 => (Some(Consonant), None),                            // Nna
        0x0BA4 => (Some(Consonant), None),                            // Ta
        0x0BA5 => (None, None),                                       // unassigned
        0x0BA6 => (None, None),                                       // unassigned
        0x0BA7 => (None, None),                                       // unassigned
        0x0BA8 => (Some(Consonant), None),                            // Na
        0x0BA9 => (Some(Consonant), None),                            // Nnna
        0x0BAA => (Some(Consonant), None),                            // Pa
        0x0BAB => (None, None),                                       // unassigned
        0x0BAC => (None, None),                                       // unassigned
        0x0BAD => (None, None),                                       // unassigned
        0x0BAE => (Some(Consonant), None),                            // Ma
        0x0BAF => (Some(Consonant), None),                            // Ya
        0x0BB0 => (Some(Consonant), None),                            // Ra
        0x0BB1 => (Some(Consonant), None),                            // Rra
        0x0BB2 => (Some(Consonant), None),                            // La
        0x0BB3 => (Some(Consonant), None),                            // Lla
        0x0BB4 => (Some(Consonant), None),                            // Llla
        0x0BB5 => (Some(Consonant), None),                            // Va
        0x0BB6 => (Some(Consonant), None),                            // Sha
        0x0BB7 => (Some(Consonant), None),                            // Ssa
        0x0BB8 => (Some(Consonant), None),                            // Sa
        0x0BB9 => (Some(Consonant), None),                            // Ha
        0x0BBA => (None, None),                                       // unassigned
        0x0BBB => (None, None),                                       // unassigned
        0x0BBC => (None, None),                                       // unassigned
        0x0BBD => (None, None),                                       // unassigned
        0x0BBE => (Some(VowelDependent), Some(RightPosition)),        // Sign Aa
        0x0BBF => (Some(VowelDependent), Some(RightPosition)),        // Sign I
        0x0BC0 => (Some(VowelDependent), Some(TopPosition)),          // Sign Ii
        0x0BC1 => (Some(VowelDependent), Some(RightPosition)),        // Sign U
        0x0BC2 => (Some(VowelDependent), Some(RightPosition)),        // Sign Uu
        0x0BC3 => (None, None),                                       // unassigned
        0x0BC4 => (None, None),                                       // unassigned
        0x0BC5 => (None, None),                                       // unassigned
        0x0BC6 => (Some(VowelDependent), Some(LeftPosition)),         // Sign E
        0x0BC7 => (Some(VowelDependent), Some(LeftPosition)),         // Sign Ee
        0x0BC8 => (Some(VowelDependent), Some(LeftPosition)),         // Sign Ai
        0x0BC9 => (None, None),                                       // unassigned
        0x0BCA => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign O
        0x0BCB => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign Oo
        0x0BCC => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign Au
        0x0BCD => (Some(Virama), Some(TopPosition)),                  // Virama
        0x0BCE => (None, None),                                       // unassigned
        0x0BCF => (None, None),                                       // unassigned
        0x0BD0 => (None, None),                                       // Om
        0x0BD1 => (None, None),                                       // unassigned
        0x0BD2 => (None, None),                                       // unassigned
        0x0BD3 => (None, None),                                       // unassigned
        0x0BD4 => (None, None),                                       // unassigned
        0x0BD5 => (None, None),                                       // unassigned
        0x0BD6 => (None, None),                                       // unassigned
        0x0BD7 => (Some(VowelDependent), Some(RightPosition)),        // Au Length Mark
        0x0BD8 => (None, None),                                       // unassigned
        0x0BD9 => (None, None),                                       // unassigned
        0x0BDA => (None, None),                                       // unassigned
        0x0BDB => (None, None),                                       // unassigned
        0x0BDC => (None, None),                                       // unassigned
        0x0BDD => (None, None),                                       // unassigned
        0x0BDE => (None, None),                                       // unassigned
        0x0BDF => (None, None),                                       // unassigned
        0x0BE0 => (None, None),                                       // unassigned
        0x0BE1 => (None, None),                                       // unassigned
        0x0BE2 => (None, None),                                       // unassigned
        0x0BE3 => (None, None),                                       // unassigned
        0x0BE4 => (None, None),                                       // unassigned
        0x0BE5 => (None, None),                                       // unassigned
        0x0BE6 => (Some(Number), None),                               // Digit Zero
        0x0BE7 => (Some(Number), None),                               // Digit One
        0x0BE8 => (Some(Number), None),                               // Digit Two
        0x0BE9 => (Some(Number), None),                               // Digit Three
        0x0BEA => (Some(Number), None),                               // Digit Four
        0x0BEB => (Some(Number), None),                               // Digit Five
        0x0BEC => (Some(Number), None),                               // Digit Six
        0x0BED => (Some(Number), None),                               // Digit Seven
        0x0BEE => (Some(Number), None),                               // Digit Eight
        0x0BEF => (Some(Number), None),                               // Digit Nine
        0x0BF0 => (Some(Number), None),                               // Number Ten
        0x0BF1 => (Some(Number), None),                               // Number One Hundred
        0x0BF2 => (Some(Number), None),                               // Number One Thousand
        0x0BF3 => (Some(Symbol), None),                               // Day Sign
        0x0BF4 => (Some(Symbol), None),                               // Month Sign
        0x0BF5 => (Some(Symbol), None),                               // Year Sign
        0x0BF6 => (Some(Symbol), None),                               // Debit Sign
        0x0BF7 => (Some(Symbol), None),                               // Credit Sign
        0x0BF8 => (Some(Symbol), None),                               // As Above Sign
        0x0BF9 => (Some(Symbol), None),                               // Tamil Rupee Sign
        0x0BFA => (Some(Symbol), None),                               // Number Sign

        // Telugu character table
        0x0C00 => (Some(Bindu), Some(TopPosition)),                   // Combining Candrabindu Above
        0x0C01 => (Some(Bindu), Some(RightPosition)),                 // Candrabindu
        0x0C02 => (Some(Bindu), Some(RightPosition)),                 // Anusvara
        0x0C03 => (Some(Visarga), Some(RightPosition)),               // Visarga
        0x0C04 => (Some(Bindu), Some(TopPosition)),                   // Combining Anusvara Above
        0x0C05 => (Some(VowelIndependent), None),                     // A
        0x0C06 => (Some(VowelIndependent), None),                     // Aa
        0x0C07 => (Some(VowelIndependent), None),                     // I
        0x0C08 => (Some(VowelIndependent), None),                     // Ii
        0x0C09 => (Some(VowelIndependent), None),                     // U
        0x0C0A => (Some(VowelIndependent), None),                     // Uu
        0x0C0B => (Some(VowelIndependent), None),                     // Vocalic R
        0x0C0C => (Some(VowelIndependent), None),                     // Vocalic L
        0x0C0D => (None, None),                                       // unassigned
        0x0C0E => (Some(VowelIndependent), None),                     // E
        0x0C0F => (Some(VowelIndependent), None),                     // Ee
        0x0C10 => (Some(VowelIndependent), None),                     // Ai
        0x0C11 => (None, None),                                       // unassigned
        0x0C12 => (Some(VowelIndependent), None),                     // O
        0x0C13 => (Some(VowelIndependent), None),                     // Oo
        0x0C14 => (Some(VowelIndependent), None),                     // Au
        0x0C15 => (Some(Consonant), None),                            // Ka
        0x0C16 => (Some(Consonant), None),                            // Kha
        0x0C17 => (Some(Consonant), None),                            // Ga
        0x0C18 => (Some(Consonant), None),                            // Gha
        0x0C19 => (Some(Consonant), None),                            // Nga
        0x0C1A => (Some(Consonant), None),                            // Ca
        0x0C1B => (Some(Consonant), None),                            // Cha
        0x0C1C => (Some(Consonant), None),                            // Ja
        0x0C1D => (Some(Consonant), None),                            // Jha
        0x0C1E => (Some(Consonant), None),                            // Nya
        0x0C1F => (Some(Consonant), None),                            // Tta
        0x0C20 => (Some(Consonant), None),                            // Ttha
        0x0C21 => (Some(Consonant), None),                            // Dda
        0x0C22 => (Some(Consonant), None),                            // Ddha
        0x0C23 => (Some(Consonant), None),                            // Nna
        0x0C24 => (Some(Consonant), None),                            // Ta
        0x0C25 => (Some(Consonant), None),                            // Tha
        0x0C26 => (Some(Consonant), None),                            // Da
        0x0C27 => (Some(Consonant), None),                            // Dha
        0x0C28 => (Some(Consonant), None),                            // Na
        0x0C29 => (None, None),                                       // unassigned
        0x0C2A => (Some(Consonant), None),                            // Pa
        0x0C2B => (Some(Consonant), None),                            // Pha
        0x0C2C => (Some(Consonant), None),                            // Ba
        0x0C2D => (Some(Consonant), None),                            // Bha
        0x0C2E => (Some(Consonant), None),                            // Ma
        0x0C2F => (Some(Consonant), None),                            // Ya
        0x0C30 => (Some(Consonant), None),                            // Ra
        0x0C31 => (Some(Consonant), None),                            // Rra
        0x0C32 => (Some(Consonant), None),                            // La
        0x0C33 => (Some(Consonant), None),                            // Lla
        0x0C34 => (Some(Consonant), None),                            // Llla
        0x0C35 => (Some(Consonant), None),                            // Va
        0x0C36 => (Some(Consonant), None),                            // Sha
        0x0C37 => (Some(Consonant), None),                            // Ssa
        0x0C38 => (Some(Consonant), None),                            // Sa
        0x0C39 => (Some(Consonant), None),                            // Ha
        0x0C3A => (None, None),                                       // unassigned
        0x0C3B => (None, None),                                       // unassigned
        0x0C3C => (Some(Nukta), Some(BottomPosition)),                // Nukta
        0x0C3D => (Some(Avagraha), None),                             // Avagraha
        0x0C3E => (Some(VowelDependent), Some(TopPosition)),          // Sign Aa
        0x0C3F => (Some(VowelDependent), Some(TopPosition)),          // Sign I
        0x0C40 => (Some(VowelDependent), Some(TopPosition)),          // Sign Ii
        0x0C41 => (Some(VowelDependent), Some(RightPosition)),        // Sign U
        0x0C42 => (Some(VowelDependent), Some(RightPosition)),        // Sign Uu
        0x0C43 => (Some(VowelDependent), Some(RightPosition)),        // Sign Vocalic R
        0x0C44 => (Some(VowelDependent), Some(RightPosition)),        // Sign Vocalic Rr
        0x0C45 => (None, None),                                       // unassigned
        0x0C46 => (Some(VowelDependent), Some(TopPosition)),          // Sign E
        0x0C47 => (Some(VowelDependent), Some(TopPosition)),          // Sign Ee
        0x0C48 => (Some(VowelDependent), Some(TopAndBottomPosition)), // Sign Ai
        0x0C49 => (None, None),                                       // unassigned
        0x0C4A => (Some(VowelDependent), Some(TopPosition)),          // Sign O
        0x0C4B => (Some(VowelDependent), Some(TopPosition)),          // Sign Oo
        0x0C4C => (Some(VowelDependent), Some(TopPosition)),          // Sign Au
        0x0C4D => (Some(Virama), Some(TopPosition)),                  // Virama
        0x0C4E => (None, None),                                       // unassigned
        0x0C4F => (None, None),                                       // unassigned
        0x0C50 => (None, None),                                       // unassigned
        0x0C51 => (None, None),                                       // unassigned
        0x0C52 => (None, None),                                       // unassigned
        0x0C53 => (None, None),                                       // unassigned
        0x0C54 => (None, None),                                       // unassigned
        0x0C55 => (Some(VowelDependent), Some(TopPosition)),          // Length Mark
        0x0C56 => (Some(VowelDependent), Some(BottomPosition)),       // Ai Length Mark
        0x0C57 => (None, None),                                       // unassigned
        0x0C58 => (Some(Consonant), None),                            // Tsa
        0x0C59 => (Some(Consonant), None),                            // Dza
        0x0C5A => (Some(Consonant), None),                            // Rrra
        0x0C5B => (None, None),                                       // unassigned
        0x0C5C => (None, None),                                       // unassigned
        0x0C5D => (Some(ConsonantDead), None),                        // Nakaara Pollu
        0x0C5E => (None, None),                                       // unassigned
        0x0C5F => (None, None),                                       // unassigned
        0x0C60 => (Some(VowelIndependent), None),                     // Vocalic Rr
        0x0C61 => (Some(VowelIndependent), None),                     // Vocalic Ll
        0x0C62 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic L
        0x0C63 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic Ll
        0x0C64 => (None, None),                                       // unassigned
        0x0C65 => (None, None),                                       // unassigned
        0x0C66 => (Some(Number), None),                               // Digit Zero
        0x0C67 => (Some(Number), None),                               // Digit One
        0x0C68 => (Some(Number), None),                               // Digit Two
        0x0C69 => (Some(Number), None),                               // Digit Three
        0x0C6A => (Some(Number), None),                               // Digit Four
        0x0C6B => (Some(Number), None),                               // Digit Five
        0x0C6C => (Some(Number), None),                               // Digit Six
        0x0C6D => (Some(Number), None),                               // Digit Seven
        0x0C6E => (Some(Number), None),                               // Digit Eight
        0x0C6F => (Some(Number), None),                               // Digit Nine
        0x0C70 => (None, None),                                       // unassigned
        0x0C71 => (None, None),                                       // unassigned
        0x0C72 => (None, None),                                       // unassigned
        0x0C73 => (None, None),                                       // unassigned
        0x0C74 => (None, None),                                       // unassigned
        0x0C75 => (None, None),                                       // unassigned
        0x0C76 => (None, None),                                       // unassigned
        0x0C77 => (None, None),                                       // unassigned
        0x0C78 => (Some(Number), None),                               // Fraction Zero Odd P
        0x0C79 => (Some(Number), None),                               // Fraction One Odd P
        0x0C7A => (Some(Number), None),                               // Fraction Two Odd P
        0x0C7B => (Some(Number), None),                               // Fraction Three Odd P
        0x0C7C => (Some(Number), None),                               // Fraction One Even P
        0x0C7D => (Some(Number), None),                               // Fraction Two Even P
        0x0C7E => (Some(Number), None),                               // Fraction Three Even P
        0x0C7F => (Some(Symbol), None),                               // Tuumu

        // Kannada character table
        0x0C80 => (None, None),                                      // Spacing Candrabindu
        0x0C81 => (Some(Bindu), Some(TopPosition)),                  // Candrabindu
        0x0C82 => (Some(Bindu), Some(RightPosition)),                // Anusvara
        0x0C83 => (Some(Visarga), Some(RightPosition)),              // Visarga
        0x0C84 => (None, None),                                      // Siddham
        0x0C85 => (Some(VowelIndependent), None),                    // A
        0x0C86 => (Some(VowelIndependent), None),                    // Aa
        0x0C87 => (Some(VowelIndependent), None),                    // I
        0x0C88 => (Some(VowelIndependent), None),                    // Ii
        0x0C89 => (Some(VowelIndependent), None),                    // U
        0x0C8A => (Some(VowelIndependent), None),                    // Uu
        0x0C8B => (Some(VowelIndependent), None),                    // Vocalic R
        0x0C8C => (Some(VowelIndependent), None),                    // Vocalic L
        0x0C8D => (None, None),                                      // unassigned
        0x0C8E => (Some(VowelIndependent), None),                    // E
        0x0C8F => (Some(VowelIndependent), None),                    // Ee
        0x0C90 => (Some(VowelIndependent), None),                    // Ai
        0x0C91 => (None, None),                                      // unassigned
        0x0C92 => (Some(VowelIndependent), None),                    // O
        0x0C93 => (Some(VowelIndependent), None),                    // Oo
        0x0C94 => (Some(VowelIndependent), None),                    // Au
        0x0C95 => (Some(Consonant), None),                           // Ka
        0x0C96 => (Some(Consonant), None),                           // Kha
        0x0C97 => (Some(Consonant), None),                           // Ga
        0x0C98 => (Some(Consonant), None),                           // Gha
        0x0C99 => (Some(Consonant), None),                           // Nga
        0x0C9A => (Some(Consonant), None),                           // Ca
        0x0C9B => (Some(Consonant), None),                           // Cha
        0x0C9C => (Some(Consonant), None),                           // Ja
        0x0C9D => (Some(Consonant), None),                           // Jha
        0x0C9E => (Some(Consonant), None),                           // Nya
        0x0C9F => (Some(Consonant), None),                           // Tta
        0x0CA0 => (Some(Consonant), None),                           // Ttha
        0x0CA1 => (Some(Consonant), None),                           // Dda
        0x0CA2 => (Some(Consonant), None),                           // Ddha
        0x0CA3 => (Some(Consonant), None),                           // Nna
        0x0CA4 => (Some(Consonant), None),                           // Ta
        0x0CA5 => (Some(Consonant), None),                           // Tha
        0x0CA6 => (Some(Consonant), None),                           // Da
        0x0CA7 => (Some(Consonant), None),                           // Dha
        0x0CA8 => (Some(Consonant), None),                           // Na
        0x0CA9 => (None, None),                                      // unassigned
        0x0CAA => (Some(Consonant), None),                           // Pa
        0x0CAB => (Some(Consonant), None),                           // Pha
        0x0CAC => (Some(Consonant), None),                           // Ba
        0x0CAD => (Some(Consonant), None),                           // Bha
        0x0CAE => (Some(Consonant), None),                           // Ma
        0x0CAF => (Some(Consonant), None),                           // Ya
        0x0CB0 => (Some(Consonant), None),                           // Ra
        0x0CB1 => (Some(Consonant), None),                           // Rra
        0x0CB2 => (Some(Consonant), None),                           // La
        0x0CB3 => (Some(Consonant), None),                           // Lla
        0x0CB4 => (None, None),                                      // unassigned
        0x0CB5 => (Some(Consonant), None),                           // Va
        0x0CB6 => (Some(Consonant), None),                           // Sha
        0x0CB7 => (Some(Consonant), None),                           // Ssa
        0x0CB8 => (Some(Consonant), None),                           // Sa
        0x0CB9 => (Some(Consonant), None),                           // Ha
        0x0CBA => (None, None),                                      // unassigned
        0x0CBB => (None, None),                                      // unassigned
        0x0CBC => (Some(Nukta), Some(BottomPosition)),               // Nukta
        0x0CBD => (Some(Avagraha), None),                            // Avagraha
        0x0CBE => (Some(VowelDependent), Some(RightPosition)),       // Sign Aa
        0x0CBF => (Some(VowelDependent), Some(TopPosition)),         // Sign I
        0x0CC0 => (Some(VowelDependent), Some(TopAndRightPosition)), // Sign Ii
        0x0CC1 => (Some(VowelDependent), Some(RightPosition)),       // Sign U
        0x0CC2 => (Some(VowelDependent), Some(RightPosition)),       // Sign Uu
        0x0CC3 => (Some(VowelDependent), Some(RightPosition)),       // Sign Vocalic R
        0x0CC4 => (Some(VowelDependent), Some(RightPosition)),       // Sign Vocalic Rr
        0x0CC5 => (None, None),                                      // unassigned
        0x0CC6 => (Some(VowelDependent), Some(TopPosition)),         // Sign E
        0x0CC7 => (Some(VowelDependent), Some(TopAndRightPosition)), // Sign Ee
        0x0CC8 => (Some(VowelDependent), Some(TopAndRightPosition)), // Sign Ai
        0x0CC9 => (None, None),                                      // unassigned
        0x0CCA => (Some(VowelDependent), Some(TopAndRightPosition)), // Sign O
        0x0CCB => (Some(VowelDependent), Some(TopAndRightPosition)), // Sign Oo
        0x0CCC => (Some(VowelDependent), Some(TopPosition)),         // Sign Au
        0x0CCD => (Some(Virama), Some(TopPosition)),                 // Virama
        0x0CCE => (None, None),                                      // unassigned
        0x0CCF => (None, None),                                      // unassigned
        0x0CD0 => (None, None),                                      // unassigned
        0x0CD1 => (None, None),                                      // unassigned
        0x0CD2 => (None, None),                                      // unassigned
        0x0CD3 => (None, None),                                      // unassigned
        0x0CD4 => (None, None),                                      // unassigned
        0x0CD5 => (Some(VowelDependent), Some(RightPosition)),       // Length Mark
        0x0CD6 => (Some(VowelDependent), Some(RightPosition)),       // Ai Length Mark
        0x0CD7 => (None, None),                                      // unassigned
        0x0CD8 => (None, None),                                      // unassigned
        0x0CD9 => (None, None),                                      // unassigned
        0x0CDA => (None, None),                                      // unassigned
        0x0CDB => (None, None),                                      // unassigned
        0x0CDC => (None, None),                                      // unassigned
        0x0CDD => (Some(ConsonantDead), None),                       // Nakaara Pollu
        0x0CDE => (Some(Consonant), None),                           // Fa
        0x0CDF => (None, None),                                      // unassigned
        0x0CE0 => (Some(VowelIndependent), None),                    // Vocalic Rr
        0x0CE1 => (Some(VowelIndependent), None),                    // Vocalic Ll
        0x0CE2 => (Some(VowelDependent), Some(BottomPosition)),      // Sign Vocalic L
        0x0CE3 => (Some(VowelDependent), Some(BottomPosition)),      // Sign Vocalic Ll
        0x0CE4 => (None, None),                                      // unassigned
        0x0CE5 => (None, None),                                      // unassigned
        0x0CE6 => (Some(Number), None),                              // Digit Zero
        0x0CE7 => (Some(Number), None),                              // Digit One
        0x0CE8 => (Some(Number), None),                              // Digit Two
        0x0CE9 => (Some(Number), None),                              // Digit Three
        0x0CEA => (Some(Number), None),                              // Digit Four
        0x0CEB => (Some(Number), None),                              // Digit Five
        0x0CEC => (Some(Number), None),                              // Digit Six
        0x0CED => (Some(Number), None),                              // Digit Seven
        0x0CEE => (Some(Number), None),                              // Digit Eight
        0x0CEF => (Some(Number), None),                              // Digit Nine
        0x0CF0 => (None, None),                                      // unassigned
        0x0CF1 => (Some(ConsonantWithStacker), None),                // Jihvamuliya
        0x0CF2 => (Some(ConsonantWithStacker), None),                // Upadhmaniya

        // Malayalam character table
        0x0D00 => (Some(Bindu), Some(TopPosition)),                   // Combining Anusvara Above
        0x0D01 => (Some(Bindu), Some(TopPosition)),                   // Candrabindu
        0x0D02 => (Some(Bindu), Some(RightPosition)),                 // Anusvara
        0x0D03 => (Some(Visarga), Some(RightPosition)),               // Visarga
        0x0D04 => (None, None),                                       // unassigned
        0x0D05 => (Some(VowelIndependent), None),                     // A
        0x0D06 => (Some(VowelIndependent), None),                     // Aa
        0x0D07 => (Some(VowelIndependent), None),                     // I
        0x0D08 => (Some(VowelIndependent), None),                     // Ii
        0x0D09 => (Some(VowelIndependent), None),                     // U
        0x0D0A => (Some(VowelIndependent), None),                     // Uu
        0x0D0B => (Some(VowelIndependent), None),                     // Vocalic R
        0x0D0C => (Some(VowelIndependent), None),                     // Vocalic L
        0x0D0D => (None, None),                                       // unassigned
        0x0D0E => (Some(VowelIndependent), None),                     // E
        0x0D0F => (Some(VowelIndependent), None),                     // Ee
        0x0D10 => (Some(VowelIndependent), None),                     // Ai
        0x0D11 => (None, None),                                       // unassigned
        0x0D12 => (Some(VowelIndependent), None),                     // O
        0x0D13 => (Some(VowelIndependent), None),                     // Oo
        0x0D14 => (Some(VowelIndependent), None),                     // Au
        0x0D15 => (Some(Consonant), None),                            // Ka
        0x0D16 => (Some(Consonant), None),                            // Kha
        0x0D17 => (Some(Consonant), None),                            // Ga
        0x0D18 => (Some(Consonant), None),                            // Gha
        0x0D19 => (Some(Consonant), None),                            // Nga
        0x0D1A => (Some(Consonant), None),                            // Ca
        0x0D1B => (Some(Consonant), None),                            // Cha
        0x0D1C => (Some(Consonant), None),                            // Ja
        0x0D1D => (Some(Consonant), None),                            // Jha
        0x0D1E => (Some(Consonant), None),                            // Nya
        0x0D1F => (Some(Consonant), None),                            // Tta
        0x0D20 => (Some(Consonant), None),                            // Ttha
        0x0D21 => (Some(Consonant), None),                            // Dda
        0x0D22 => (Some(Consonant), None),                            // Ddha
        0x0D23 => (Some(Consonant), None),                            // Nna
        0x0D24 => (Some(Consonant), None),                            // Ta
        0x0D25 => (Some(Consonant), None),                            // Tha
        0x0D26 => (Some(Consonant), None),                            // Da
        0x0D27 => (Some(Consonant), None),                            // Dha
        0x0D28 => (Some(Consonant), None),                            // Na
        0x0D29 => (Some(Consonant), None),                            // Nnna
        0x0D2A => (Some(Consonant), None),                            // Pa
        0x0D2B => (Some(Consonant), None),                            // Pha
        0x0D2C => (Some(Consonant), None),                            // Ba
        0x0D2D => (Some(Consonant), None),                            // Bha
        0x0D2E => (Some(Consonant), None),                            // Ma
        0x0D2F => (Some(Consonant), None),                            // Ya
        0x0D30 => (Some(Consonant), None),                            // Ra
        0x0D31 => (Some(Consonant), None),                            // Rra
        0x0D32 => (Some(Consonant), None),                            // La
        0x0D33 => (Some(Consonant), None),                            // Lla
        0x0D34 => (Some(Consonant), None),                            // Llla
        0x0D35 => (Some(Consonant), None),                            // Va
        0x0D36 => (Some(Consonant), None),                            // Sha
        0x0D37 => (Some(Consonant), None),                            // Ssa
        0x0D38 => (Some(Consonant), None),                            // Sa
        0x0D39 => (Some(Consonant), None),                            // Ha
        0x0D3A => (Some(Consonant), None),                            // Ttta
        0x0D3B => (Some(PureKiller), Some(TopPosition)),              // Vertical Bar Virama
        0x0D3C => (Some(PureKiller), Some(TopPosition)),              // Circular Virama
        0x0D3D => (Some(Avagraha), None),                             // Avagraha
        0x0D3E => (Some(VowelDependent), Some(RightPosition)),        // Sign Aa
        0x0D3F => (Some(VowelDependent), Some(RightPosition)),        // Sign I
        0x0D40 => (Some(VowelDependent), Some(RightPosition)),        // Sign Ii
        0x0D41 => (Some(VowelDependent), Some(RightPosition)),        // Sign U
        0x0D42 => (Some(VowelDependent), Some(RightPosition)),        // Sign Uu
        0x0D43 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic R
        0x0D44 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic Rr
        0x0D45 => (None, None),                                       // unassigned
        0x0D46 => (Some(VowelDependent), Some(LeftPosition)),         // Sign E
        0x0D47 => (Some(VowelDependent), Some(LeftPosition)),         // Sign Ee
        0x0D48 => (Some(VowelDependent), Some(LeftPosition)),         // Sign Ai
        0x0D49 => (None, None),                                       // unassigned
        0x0D4A => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign O
        0x0D4B => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign Oo
        0x0D4C => (Some(VowelDependent), Some(LeftAndRightPosition)), // Sign Au
        0x0D4D => (Some(Virama), Some(TopPosition)),                  // Virama
        0x0D4E => (Some(ConsonantPreRepha), None),                    // Dot Reph
        0x0D4F => (Some(Symbol), None),                               // Para
        0x0D50 => (None, None),                                       // unassigned
        0x0D51 => (None, None),                                       // unassigned
        0x0D52 => (None, None),                                       // unassigned
        0x0D53 => (None, None),                                       // unassigned
        0x0D54 => (Some(ConsonantDead), None),                        // Chillu M
        0x0D55 => (Some(ConsonantDead), None),                        // Chillu Y
        0x0D56 => (Some(ConsonantDead), None),                        // Chillu Lll
        0x0D57 => (Some(VowelDependent), Some(RightPosition)),        // Au Length Mark
        0x0D58 => (Some(Number), None),                               // Fraction 1/160
        0x0D59 => (Some(Number), None),                               // Fraction 1/40
        0x0D5A => (Some(Number), None),                               // Fraction 3/80
        0x0D5B => (Some(Number), None),                               // Fraction 1/20
        0x0D5C => (Some(Number), None),                               // Fraction 1/10
        0x0D5D => (Some(Number), None),                               // Fraction 3/20
        0x0D5E => (Some(Number), None),                               // Fraction 1/5
        0x0D5F => (Some(VowelIndependent), None),                     // Archaic Ii
        0x0D60 => (Some(VowelIndependent), None),                     // Vocalic Rr
        0x0D61 => (Some(VowelIndependent), None),                     // Vocalic Ll
        0x0D62 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic L
        0x0D63 => (Some(VowelDependent), Some(BottomPosition)),       // Sign Vocalic Ll
        0x0D64 => (None, None),                                       // unassigned
        0x0D65 => (None, None),                                       // unassigned
        0x0D66 => (Some(Number), None),                               // Digit Zero
        0x0D67 => (Some(Number), None),                               // Digit One
        0x0D68 => (Some(Number), None),                               // Digit Two
        0x0D69 => (Some(Number), None),                               // Digit Three
        0x0D6A => (Some(Number), None),                               // Digit Four
        0x0D6B => (Some(Number), None),                               // Digit Five
        0x0D6C => (Some(Number), None),                               // Digit Six
        0x0D6D => (Some(Number), None),                               // Digit Seven
        0x0D6E => (Some(Number), None),                               // Digit Eight
        0x0D6F => (Some(Number), None),                               // Digit Nine
        0x0D70 => (Some(Number), None),                               // Number Ten
        0x0D71 => (Some(Number), None),                               // Number One Hundred
        0x0D72 => (Some(Number), None),                               // Number One Thousand
        0x0D73 => (Some(Number), None),                               // Fraction 1/4
        0x0D74 => (Some(Number), None),                               // Fraction 1/2
        0x0D75 => (Some(Number), None),                               // Fraction 3/4
        0x0D76 => (Some(Number), None),                               // Fraction 1/16
        0x0D77 => (Some(Number), None),                               // Fraction 1/8
        0x0D78 => (Some(Number), None),                               // Fraction 3/16
        0x0D79 => (Some(Symbol), None),                               // Date Mark
        0x0D7A => (Some(ConsonantDead), None),                        // Chillu Nn
        0x0D7B => (Some(ConsonantDead), None),                        // Chillu N
        0x0D7C => (Some(ConsonantDead), None),                        // Chillu Rr
        0x0D7D => (Some(ConsonantDead), None),                        // Chillu L
        0x0D7E => (Some(ConsonantDead), None),                        // Chillu Ll
        0x0D7F => (Some(ConsonantDead), None),                        // Chillu K

        // Sinhala character table
        0x0D80 => (None, None),                                          // unassigned
        0x0D81 => (None, None),                                          // unassigned
        0x0D82 => (Some(Bindu), Some(RightPosition)),                    // Anusvara
        0x0D83 => (Some(Visarga), Some(RightPosition)),                  // Visarga
        0x0D84 => (None, None),                                          // unassigned
        0x0D85 => (Some(VowelIndependent), None),                        // A
        0x0D86 => (Some(VowelIndependent), None),                        // Aa
        0x0D87 => (Some(VowelIndependent), None),                        // Ae
        0x0D88 => (Some(VowelIndependent), None),                        // Aae
        0x0D89 => (Some(VowelIndependent), None),                        // I
        0x0D8A => (Some(VowelIndependent), None),                        // Ii
        0x0D8B => (Some(VowelIndependent), None),                        // U
        0x0D8C => (Some(VowelIndependent), None),                        // Uu
        0x0D8D => (Some(VowelIndependent), None),                        // Vocalic R
        0x0D8E => (Some(VowelIndependent), None),                        // Vocalic Rr
        0x0D8F => (Some(VowelIndependent), None),                        // Vocalic L
        0x0D90 => (Some(VowelIndependent), None),                        // Vocalic Ll
        0x0D91 => (Some(VowelIndependent), None),                        // E
        0x0D92 => (Some(VowelIndependent), None),                        // Ee
        0x0D93 => (Some(VowelIndependent), None),                        // Ai
        0x0D94 => (Some(VowelIndependent), None),                        // O
        0x0D95 => (Some(VowelIndependent), None),                        // Oo
        0x0D96 => (Some(VowelIndependent), None),                        // Au
        0x0D97 => (None, None),                                          // unassigned
        0x0D98 => (None, None),                                          // unassigned
        0x0D99 => (None, None),                                          // unassigned
        0x0D9A => (Some(Consonant), None),                               // Ka
        0x0D9B => (Some(Consonant), None),                               // Kha
        0x0D9C => (Some(Consonant), None),                               // Ga
        0x0D9D => (Some(Consonant), None),                               // Gha
        0x0D9E => (Some(Consonant), None),                               // Nga
        0x0D9F => (Some(Consonant), None),                               // Nnga
        0x0DA0 => (Some(Consonant), None),                               // Ca
        0x0DA1 => (Some(Consonant), None),                               // Cha
        0x0DA2 => (Some(Consonant), None),                               // Ja
        0x0DA3 => (Some(Consonant), None),                               // Jha
        0x0DA4 => (Some(Consonant), None),                               // Nya
        0x0DA5 => (Some(Consonant), None),                               // Jnya
        0x0DA6 => (Some(Consonant), None),                               // Nyja
        0x0DA7 => (Some(Consonant), None),                               // Tta
        0x0DA8 => (Some(Consonant), None),                               // Ttha
        0x0DA9 => (Some(Consonant), None),                               // Dda
        0x0DAA => (Some(Consonant), None),                               // Ddha
        0x0DAB => (Some(Consonant), None),                               // Nna
        0x0DAC => (Some(Consonant), None),                               // Nndda
        0x0DAD => (Some(Consonant), None),                               // Ta
        0x0DAE => (Some(Consonant), None),                               // Tha
        0x0DAF => (Some(Consonant), None),                               // Da
        0x0DB0 => (Some(Consonant), None),                               // Dha
        0x0DB1 => (Some(Consonant), None),                               // Na
        0x0DB2 => (None, None),                                          // unassigned
        0x0DB3 => (Some(Consonant), None),                               // Nda
        0x0DB4 => (Some(Consonant), None),                               // Pa
        0x0DB5 => (Some(Consonant), None),                               // Pha
        0x0DB6 => (Some(Consonant), None),                               // Ba
        0x0DB7 => (Some(Consonant), None),                               // Bha
        0x0DB8 => (Some(Consonant), None),                               // Ma
        0x0DB9 => (Some(Consonant), None),                               // Mba
        0x0DBA => (Some(Consonant), None),                               // Ya
        0x0DBB => (Some(Consonant), None),                               // Ra
        0x0DBC => (None, None),                                          // unassigned
        0x0DBD => (Some(Consonant), None),                               // La
        0x0DBE => (None, None),                                          // unassigned
        0x0DBF => (None, None),                                          // unassigned
        0x0DC0 => (Some(Consonant), None),                               // Va
        0x0DC1 => (Some(Consonant), None),                               // Sha
        0x0DC2 => (Some(Consonant), None),                               // Ssa
        0x0DC3 => (Some(Consonant), None),                               // Sa
        0x0DC4 => (Some(Consonant), None),                               // Ha
        0x0DC5 => (Some(Consonant), None),                               // Lla
        0x0DC6 => (Some(Consonant), None),                               // Fa
        0x0DC7 => (None, None),                                          // unassigned
        0x0DC8 => (None, None),                                          // unassigned
        0x0DC9 => (None, None),                                          // unassigned
        0x0DCA => (Some(Virama), Some(TopPosition)),                     // Virama
        0x0DCB => (None, None),                                          // unassigned
        0x0DCC => (None, None),                                          // unassigned
        0x0DCD => (None, None),                                          // unassigned
        0x0DCE => (None, None),                                          // unassigned
        0x0DCF => (Some(VowelDependent), Some(RightPosition)),           // Sign Aa
        0x0DD0 => (Some(VowelDependent), Some(RightPosition)),           // Sign Ae
        0x0DD1 => (Some(VowelDependent), Some(RightPosition)),           // Sign Aae
        0x0DD2 => (Some(VowelDependent), Some(TopPosition)),             // Sign I
        0x0DD3 => (Some(VowelDependent), Some(TopPosition)),             // Sign Ii
        0x0DD4 => (Some(VowelDependent), Some(BottomPosition)),          // Sign U
        0x0DD5 => (None, None),                                          // unassigned
        0x0DD6 => (Some(VowelDependent), Some(BottomPosition)),          // Sign Uu
        0x0DD7 => (None, None),                                          // unassigned
        0x0DD8 => (Some(VowelDependent), Some(RightPosition)),           // Sign Vocalic R
        0x0DD9 => (Some(VowelDependent), Some(LeftPosition)),            // Sign E
        0x0DDA => (Some(VowelDependent), Some(TopAndLeftPosition)),      // Sign Ee
        0x0DDB => (Some(VowelDependent), Some(LeftPosition)),            // Sign Ai
        0x0DDC => (Some(VowelDependent), Some(LeftAndRightPosition)),    // Sign O
        0x0DDD => (Some(VowelDependent), Some(TopLeftAndRightPosition)), // Sign Oo
        0x0DDE => (Some(VowelDependent), Some(LeftAndRightPosition)),    // Sign Au
        0x0DDF => (Some(VowelDependent), Some(RightPosition)),           // Sign Vocalic L
        0x0DE0 => (None, None),                                          // unassigned
        0x0DE1 => (None, None),                                          // unassigned
        0x0DE2 => (None, None),                                          // unassigned
        0x0DE3 => (None, None),                                          // unassigned
        0x0DE4 => (None, None),                                          // unassigned
        0x0DE5 => (None, None),                                          // unassigned
        0x0DE6 => (Some(Number), None),                                  // Digit Zero
        0x0DE7 => (Some(Number), None),                                  // Digit One
        0x0DE8 => (Some(Number), None),                                  // Digit Two
        0x0DE9 => (Some(Number), None),                                  // Digit Three
        0x0DEA => (Some(Number), None),                                  // Digit Four
        0x0DEB => (Some(Number), None),                                  // Digit Five
        0x0DEC => (Some(Number), None),                                  // Digit Six
        0x0DED => (Some(Number), None),                                  // Digit Seven
        0x0DEE => (Some(Number), None),                                  // Digit Eight
        0x0DEF => (Some(Number), None),                                  // Digit Nine
        0x0DF0 => (None, None),                                          // unassigned
        0x0DF1 => (None, None),                                          // unassigned
        0x0DF2 => (Some(VowelDependent), Some(RightPosition)),           // Sign Vocalic Rr
        0x0DF3 => (Some(VowelDependent), Some(RightPosition)),           // Sign Vocalic Ll
        0x0DF4 => (None, None),                                          // Kunddaliya
        0x0DF5 => (None, None),                                          // unassigned
        0x0DF6 => (None, None),                                          // unassigned
        0x0DF7 => (None, None),                                          // unassigned
        0x0DF8 => (None, None),                                          // unassigned
        0x0DF9 => (None, None),                                          // unassigned
        0x0DFA => (None, None),                                          // unassigned
        0x0DFB => (None, None),                                          // unassigned
        0x0DFC => (None, None),                                          // unassigned
        0x0DFD => (None, None),                                          // unassigned
        0x0DFE => (None, None),                                          // unassigned
        0x0DFF => (None, None),                                          // unassigned

        // Vedic Extensions character table
        0x1CD0 => (Some(Cantillation), Some(TopPosition)),    // Tone Karshana
        0x1CD1 => (Some(Cantillation), Some(TopPosition)),    // Tone Shara
        0x1CD2 => (Some(Cantillation), Some(TopPosition)),    // Tone Prenkha
        0x1CD3 => (None, None),                               // Sign Nihshvasa
        0x1CD4 => (Some(Cantillation), Some(Overstruck)),     // Tone Midline Svarita
        0x1CD5 => (Some(Cantillation), Some(BottomPosition)), // Tone Aggravated Independent Svarita
        0x1CD6 => (Some(Cantillation), Some(BottomPosition)), // Tone Independent Svarita
        0x1CD7 => (Some(Cantillation), Some(BottomPosition)), // Tone Kathaka Independent Svarita
        0x1CD8 => (Some(Cantillation), Some(BottomPosition)), // Tone Candra Below
        0x1CD9 => (Some(Cantillation), Some(BottomPosition)), // Tone Kathaka Independent Svarita Schroeder
        0x1CDA => (Some(Cantillation), Some(TopPosition)),    // Tone Double Svarita
        0x1CDB => (Some(Cantillation), Some(TopPosition)),    // Tone Triple Svarita
        0x1CDC => (Some(Cantillation), Some(BottomPosition)), // Tone Kathaka Anudatta
        0x1CDD => (Some(Cantillation), Some(BottomPosition)), // Tone Dot Below
        0x1CDE => (Some(Cantillation), Some(BottomPosition)), // Tone Two Dots Below
        0x1CDF => (Some(Cantillation), Some(BottomPosition)), // Tone Three Dots Below
        0x1CE0 => (Some(Cantillation), Some(TopPosition)),    // Tone Rigvedic Kashmiri Independent Svarita
        0x1CE1 => (Some(Cantillation), Some(RightPosition)),  // Tone Atharavedic Independent Svarita
        0x1CE2 => (Some(Avagraha), Some(Overstruck)),         // Sign Visarga Svarita
        0x1CE3 => (None, Some(Overstruck)),                   // Sign Visarga Udatta
        0x1CE4 => (None, Some(Overstruck)),                   // Sign Reversed Visarga Udatta
        0x1CE5 => (None, Some(Overstruck)),                   // Sign Visarga Anudatta
        0x1CE6 => (None, Some(Overstruck)),                   // Sign Reversed Visarga Anudatta
        0x1CE7 => (None, Some(Overstruck)),                   // Sign Visarga Udatta With Tail
        0x1CE8 => (Some(Avagraha), Some(Overstruck)),         // Sign Visarga Anudatta With Tail
        0x1CE9 => (Some(Symbol), None),                       // Sign Anusvara Antargomukha
        0x1CEA => (None, None),                               // Sign Anusvara Bahirgomukha
        0x1CEB => (None, None),                               // Sign Anusvara Vamagomukha
        0x1CEC => (Some(Symbol), None),                       // Sign Anusvara Vamagomukha With Tail
        0x1CED => (Some(Avagraha), Some(BottomPosition)),     // Sign Tiryak
        0x1CEE => (Some(Symbol), None),                       // Sign Hexiform Long Anusvara
        0x1CEF => (None, None),                               // Sign Long Anusvara
        0x1CF0 => (None, None),                               // Sign Rthang Long Anusvara
        0x1CF1 => (Some(Symbol), None),                       // Sign Anusvara Ubhayato Mukha
        0x1CF2 => (Some(Visarga), None),                      // Sign Ardhavisarga
        0x1CF3 => (Some(Visarga), None),                      // Sign Rotated Ardhavisarga
        0x1CF4 => (Some(Cantillation), Some(TopPosition)),    // Tone Candra Above
        0x1CF5 => (Some(ConsonantWithStacker), None),         // Sign Jihvamuliya
        0x1CF6 => (Some(ConsonantWithStacker), None),         // Sign Upadhmaniya
        0x1CF7 => (None, None),                               // Sign Atikrama
        0x1CF8 => (Some(Cantillation), None),                 // Tone Ring Above
        0x1CF9 => (Some(Cantillation), None),                 // Tone Double Ring Above

        // Devanagari Extended character table
        0xA8E0 => (Some(Cantillation), Some(TopPosition)),   // Combining Zero
        0xA8E1 => (Some(Cantillation), Some(TopPosition)),   // Combining One
        0xA8E2 => (Some(Cantillation), Some(TopPosition)),   // Combining Two
        0xA8E3 => (Some(Cantillation), Some(TopPosition)),   // Combining Three
        0xA8E4 => (Some(Cantillation), Some(TopPosition)),   // Combining Four
        0xA8E5 => (Some(Cantillation), Some(TopPosition)),   // Combining Five
        0xA8E6 => (Some(Cantillation), Some(TopPosition)),   // Combining Six
        0xA8E7 => (Some(Cantillation), Some(TopPosition)),   // Combining Seven
        0xA8E8 => (Some(Cantillation), Some(TopPosition)),   // Combining Eight
        0xA8E9 => (Some(Cantillation), Some(TopPosition)),   // Combining Nine
        0xA8EA => (Some(Cantillation), Some(TopPosition)),   // Combining A
        0xA8EB => (Some(Cantillation), Some(TopPosition)),   // Combining U
        0xA8EC => (Some(Cantillation), Some(TopPosition)),   // Combining Ka
        0xA8ED => (Some(Cantillation), Some(TopPosition)),   // Combining Na
        0xA8EE => (Some(Cantillation), Some(TopPosition)),   // Combining Pa
        0xA8EF => (Some(Cantillation), Some(TopPosition)),   // Combining Ra
        0xA8F0 => (Some(Cantillation), Some(TopPosition)),   // Combining Vi
        0xA8F1 => (Some(Cantillation), Some(TopPosition)),   // Combining Avagraha
        0xA8F2 => (Some(Bindu), None),                       // Spacing Candrabindu
        0xA8F3 => (Some(Bindu), None),                       // Candrabindu Virama
        0xA8F4 => (None, None),                              // Double Candrabindu Virama
        0xA8F5 => (None, None),                              // Candrabindu Two
        0xA8F6 => (None, None),                              // Candrabindu Three
        0xA8F7 => (None, None),                              // Candrabindu Avagraha
        0xA8F8 => (None, None),                              // Pushpika
        0xA8F9 => (None, None),                              // Gap Filler
        0xA8FA => (None, None),                              // Caret
        0xA8FB => (None, None),                              // Headstroke
        0xA8FC => (None, None),                              // Siddham
        0xA8FD => (None, None),                              // Jain Om
        0xA8FE => (Some(VowelIndependent), None),            // Ay
        0xA8FF => (Some(VowelDependent), Some(TopPosition)), // Sign Ay

        // Sinhala Archaic Numbers character table
        0x111E0 => (None, None),         // unassigned
        0x111E1 => (Some(Number), None), // Archaic Digit One
        0x111E2 => (Some(Number), None), // Archaic Digit Two
        0x111E3 => (Some(Number), None), // Archaic Digit Three
        0x111E4 => (Some(Number), None), // Archaic Digit Four
        0x111E5 => (Some(Number), None), // Archaic Digit Five
        0x111E6 => (Some(Number), None), // Archaic Digit Six
        0x111E7 => (Some(Number), None), // Archaic Digit Seven
        0x111E8 => (Some(Number), None), // Archaic Digit Eight
        0x111E9 => (Some(Number), None), // Archaic Digit Nine
        0x111EA => (Some(Number), None), // Archaic Number Ten
        0x111EB => (Some(Number), None), // Archaic Number 20
        0x111EC => (Some(Number), None), // Archaic Number 30
        0x111ED => (Some(Number), None), // Archaic Number 40
        0x111EE => (Some(Number), None), // Archaic Number 50
        0x111EF => (Some(Number), None), // Archaic Number 60
        0x111F0 => (Some(Number), None), // Archaic Number 70
        0x111F1 => (Some(Number), None), // Archaic Number 80
        0x111F2 => (Some(Number), None), // Archaic Number 90
        0x111F3 => (Some(Number), None), // Archaic Number 100
        0x111F4 => (Some(Number), None), // Archaic Number 1000
        0x111F5 => (None, None),         // unassigned
        0x111F6 => (None, None),         // unassigned
        0x111F7 => (None, None),         // unassigned
        0x111F8 => (None, None),         // unassigned
        0x111F9 => (None, None),         // unassigned
        0x111FA => (None, None),         // unassigned
        0x111FB => (None, None),         // unassigned
        0x111FC => (None, None),         // unassigned
        0x111FD => (None, None),         // unassigned
        0x111FE => (None, None),         // unassigned
        0x111FF => (None, None),         // unassigned

        // Grantha marks character table
        0x11301 => (Some(Bindu), Some(TopPosition)),     // Grantha Candrabindu
        0x11303 => (Some(Visarga), Some(RightPosition)), // Grantha Visarga
        0x1133B => (Some(Nukta), Some(BottomPosition)),  // Combining Bindu Below
        0x1133C => (Some(Nukta), Some(BottomPosition)),  // Grantha Nukta

        // Miscellaneous character table
        0x00A0 => (Some(Placeholder), None),      // No-break space
        0x00B2 => (Some(SyllableModifier), None), // Superscript Two (used in Tamil)
        0x00B3 => (Some(SyllableModifier), None), // Superscript Three (used in Tamil)
        0x200C => (Some(NonJoiner), None),        // Zero-width non-joiner
        0x200D => (Some(Joiner), None),           // Zero-width joiner
        0x2010 => (Some(Placeholder), None),      // Hyphen
        0x2011 => (Some(Placeholder), None),      // No-break hyphen
        0x2012 => (Some(Placeholder), None),      // Figure dash
        0x2013 => (Some(Placeholder), None),      // En dash
        0x2014 => (Some(Placeholder), None),      // Em dash
        0x2074 => (Some(SyllableModifier), None), // Superscript Four (used in Tamil)
        0x2082 => (Some(SyllableModifier), None), // Subscript Two (used in Tamil)
        0x2083 => (Some(SyllableModifier), None), // Subscript Three (used in Tamil)
        0x2084 => (Some(SyllableModifier), None), // Subscript Four (used in Tamil)
        0x25CC => (Some(DottedCircle), None),     // Dotted circle

        _ => (None, None),
    }
}

/////////////////////////////////////////////////////////////////////////////
// Unit tests
/////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    mod matra_pos {
        use super::*;

        #[test]
        fn test_no_canonical_decomposition_matra() {
            assert_eq!(
                matra_pos('\u{0AC9}', Script::Gujarati),
                Some(Pos::AfterPost)
            );
            assert_eq!(matra_pos('\u{0B57}', Script::Oriya), Some(Pos::AfterPost));
        }

        #[test]
        fn test_non_decomposed_matra() {
            // Should never happen
            assert_eq!(matra_pos('\u{09CB}', Script::Bengali), None);
        }

        #[test]
        fn test_non_matra() {
            assert_eq!(matra_pos('\u{09B6}', Script::Bengali), None);
        }
    }

    mod move_element {
        use super::*;

        #[test]
        fn test_move_forward() {
            let mut v = [1, 2, 3, 4];
            move_element(&mut v, 0, 3);

            assert_eq!([2, 3, 4, 1], v);
        }

        #[test]
        fn test_move_backward() {
            let mut v = [1, 2, 3, 4];
            move_element(&mut v, 3, 1);

            assert_eq!([1, 4, 2, 3], v);
        }
    }

    mod constrain_vowel {
        use super::*;

        #[test]
        fn test_insert_one_dotted_circle() {
            let mut cs = vec!['\u{0909}', '\u{0941}'];
            constrain_vowel(&mut cs);

            assert_eq!(vec!['\u{0909}', '\u{25CC}', '\u{0941}'], cs);
        }

        #[test]
        fn test_insert_two_dotted_circles() {
            let mut cs = vec!['\u{0909}', '\u{0941}', '\u{090F}', '\u{0945}'];
            constrain_vowel(&mut cs);

            assert_eq!(
                vec!['\u{0909}', '\u{25CC}', '\u{0941}', '\u{090F}', '\u{25CC}', '\u{0945}'],
                cs
            );
        }

        #[test]
        fn test_insert_dotted_circle_after_reph() {
            let mut cs = vec!['\u{0930}', '\u{094D}', '\u{0907}'];
            constrain_vowel(&mut cs);

            assert_eq!(vec!['\u{0930}', '\u{094D}', '\u{25CC}', '\u{0907}'], cs);
        }

        #[test]
        fn test_should_not_insert_dotted_circle() {
            let mut cs = vec!['\u{0930}', '\u{094D}'];
            constrain_vowel(&mut cs);

            assert_eq!(vec!['\u{0930}', '\u{094D}'], cs);
        }
    }

    mod decompose_matra {
        use super::*;

        #[test]
        fn test_single_decomposition() {
            let mut cs = vec!['\u{09CB}'];
            decompose_matra(&mut cs);

            assert_eq!(vec!['\u{09C7}', '\u{09BE}'], cs);
        }

        #[test]
        fn test_double_decomposition() {
            let mut cs = vec!['\u{09CB}', '\u{09CB}'];
            decompose_matra(&mut cs);

            assert_eq!(vec!['\u{09C7}', '\u{09BE}', '\u{09C7}', '\u{09BE}'], cs);
        }
    }

    mod recompose_bengali_ya_nukta {
        use super::*;

        #[test]
        fn test_single_codepoint() {
            let mut cs = vec!['\u{09AF}'];
            recompose_bengali_ya_nukta(&mut cs);

            assert_eq!(vec!['\u{09AF}'], cs);
        }

        #[test]
        fn test_ya_nukta_ya() {
            let mut cs = vec!['\u{09AF}', '\u{09BC}', '\u{09AF}'];
            recompose_bengali_ya_nukta(&mut cs);

            assert_eq!(vec!['\u{09DF}', '\u{09AF}'], cs);
        }

        #[test]
        fn test_ya_ya_nukta() {
            let mut cs = vec!['\u{09AF}', '\u{09AF}', '\u{09BC}'];
            recompose_bengali_ya_nukta(&mut cs);

            assert_eq!(vec!['\u{09AF}', '\u{09DF}'], cs);
        }
    }

    mod reorder_kannada_ra_halant_zwj {
        use super::*;

        const R: char = '\u{0CB0}';
        const H: char = '\u{0CCD}';
        const Z: char = '\u{200D}';

        #[test]
        fn test_ra_halant() {
            let mut cs = vec![R, H];
            reorder_kannada_ra_halant_zwj(&mut cs);

            assert_eq!(vec![R, H], cs);
        }

        #[test]
        fn test_ra_halant_zwj() {
            let mut cs = vec![R, H, Z];
            reorder_kannada_ra_halant_zwj(&mut cs);

            assert_eq!(vec![R, Z, H], cs);
        }

        #[test]
        fn test_non_initial_ra_halant_zwj() {
            let mut cs = vec![R, H, R, H, Z];
            reorder_kannada_ra_halant_zwj(&mut cs);

            assert_eq!(vec![R, H, R, H, Z], cs);
        }
    }
}
