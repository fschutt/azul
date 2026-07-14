//! Unicode script detection and language identification for text shaping
//!
// Taken from: https://github.com/greyblake/whatlang-rs/blob/master/src/scripts/detect.rs
//
// See: https://github.com/greyblake/whatlang-rs/pull/67

// License:
//
// (The MIT License)
//
// Copyright (c) 2017 Sergey Potapov <blake131313@gmail.com>
// Copyright (c) 2014 Titus Wormer <tituswormer@gmail.com>
// Copyright (c) 2008 Kent S Johnson
// Copyright (c) 2006 Jacob R Rideout <kde@jacobrideout.net>
// Copyright (c) 2004 Maciej Ceglowski
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// 'Software'), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED 'AS IS', WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::Language as HyphenationLanguage;
#[cfg(feature = "text_layout_hyphenation")]
pub use hyphenation::Language;

/// Stub Language enum for when hyphenation is not enabled.
/// This mirrors the variants used in script detection functions.
#[cfg(not(feature = "text_layout_hyphenation"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Language {
    // Latin script languages
    EnglishUS,
    French,
    German1996,
    Spanish,
    Portuguese,
    Estonian,
    Hungarian,
    Polish,
    Czech,
    Slovak,
    Latvian,
    Lithuanian,
    Romanian,
    Turkish,
    Croatian,
    Icelandic,
    Welsh,
    NorwegianBokmal,
    Swedish,
    // Cyrillic script languages
    Russian,
    Ukrainian,
    Belarusian,
    Bulgarian,
    Macedonian,
    SerbianCyrillic,
    Mongolian,
    SlavonicChurch,
    // Greek script languages
    GreekMono,
    GreekPoly,
    Coptic,
    // Indic script languages
    Hindi,
    Bengali,
    Assamese,
    Marathi,
    Sanskrit,
    Gujarati,
    Panjabi,
    Kannada,
    Malayalam,
    Oriya,
    Tamil,
    Telugu,
    // Other scripts
    Georgian,
    Ethiopic,
    Thai,
    Chinese,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Script {
    // Keep this in alphabetic order (for C bindings)
    Arabic,
    Bengali,
    Cyrillic,
    Devanagari,
    Ethiopic,
    Georgian,
    Greek,
    Gujarati,
    Gurmukhi,
    Hangul,
    Hebrew,
    Hiragana,
    Kannada,
    Katakana,
    Khmer,
    Latin,
    Malayalam,
    Mandarin,
    Myanmar,
    Oriya,
    Sinhala,
    Tamil,
    Telugu,
    Thai,
}

// Is it space, punctuation or digit?
// Stop character is a character that does not give any value for script
// or language detection.
#[inline]
#[must_use] pub const fn is_stop_char(ch: char) -> bool {
    matches!(ch, '\u{0000}'..='\u{0040}' | '\u{005B}'..='\u{0060}' | '\u{007B}'..='\u{007E}')
}

type ScriptChecker = (Script, fn(char) -> bool);
type ScriptCounter = (Script, fn(char) -> bool, usize);

const SCRIPT_CHECKERS: [ScriptChecker; 24] = [
    (Script::Latin, is_latin),
    (Script::Cyrillic, is_cyrillic),
    (Script::Arabic, is_arabic),
    (Script::Mandarin, is_mandarin),
    (Script::Devanagari, is_devanagari),
    (Script::Hebrew, is_hebrew),
    (Script::Ethiopic, is_ethiopic),
    (Script::Georgian, is_georgian),
    (Script::Bengali, is_bengali),
    (Script::Hangul, is_hangul),
    (Script::Hiragana, is_hiragana),
    (Script::Katakana, is_katakana),
    (Script::Greek, is_greek),
    (Script::Kannada, is_kannada),
    (Script::Tamil, is_tamil),
    (Script::Thai, is_thai),
    (Script::Gujarati, is_gujarati),
    (Script::Gurmukhi, is_gurmukhi),
    (Script::Telugu, is_telugu),
    (Script::Malayalam, is_malayalam),
    (Script::Oriya, is_oriya),
    (Script::Myanmar, is_myanmar),
    (Script::Sinhala, is_sinhala),
    (Script::Khmer, is_khmer),
];

/// Detect only a script by a given text
/// # Panics
///
/// Panics only if the internal script-counter table were empty, which cannot happen (it is a fixed-size array).
pub fn detect_script(text: &str) -> Option<Script> {
    let mut script_counters: [ScriptCounter; 24] = SCRIPT_CHECKERS.map(|(s, f)| (s, f, 0));

    let half = text.chars().count() / 2;

    for ch in text.chars() {
        if is_stop_char(ch) {
            continue;
        }

        // For performance reasons, we need to mutate script_counters by calling
        // `swap` function, it would not be possible to do using normal iterator.
        for i in 0..script_counters.len() {
            let found = {
                let (script, check_fn, ref mut count) = script_counters[i];
                if check_fn(ch) {
                    *count += 1;
                    if *count > half {
                        return Some(script);
                    }
                    true
                } else {
                    false
                }
            };
            // Have to let borrow of count fall out of scope before doing swapping, or we could
            // do this above.
            if found {
                // If script was found, move it closer to the front.
                // If the text contains largely 1 or 2 scripts, this will
                // cause these scripts to be eventually checked first.
                if i > 0 {
                    script_counters.swap(i - 1, i);
                }
                break;
            }
        }
    }

    let (script, _, count) = script_counters
        .iter()
        .copied()
        .max_by_key(|&(_, _, count)| count)
        .unwrap();
    if count != 0 {
        Some(script)
    } else {
        None
    }
}

#[must_use] pub fn detect_char_script(ch: char) -> Option<Script> {
    for &(script, check_fn) in &SCRIPT_CHECKERS {
        if check_fn(ch) {
            return Some(script);
        }
    }
    None
}

/// Iterates through the text once and returns as soon as an Assamese-specific character is found.
fn detect_bengali_language(text: &str) -> Language {
    for c in text.chars() {
        // These characters are specific to Assamese in the Bengali script block.
        // We can return immediately as this is the highest priority check.
        if matches!(c, '\u{09F0}' | '\u{09F1}') {
            // ৰ, ৱ
            return Language::Assamese;
        }
    }
    // If we finish the loop without finding any Assamese characters, it's Bengali.
    Language::Bengali
}

fn detect_cyrillic_language(text: &str) -> Language {
    for c in text.chars() {
        match c {
            // Highest priority: Old Cyrillic characters for Slavonic Church. Return immediately.
            '\u{0460}'..='\u{047F}' => return Language::SlavonicChurch,
            // Set flags for other languages. We don't return yet because a higher-priority
            // character (like the one above) could still appear.
            'ѓ' | 'ќ' | 'ѕ' => return Language::Macedonian,
            'ў' => return Language::Belarusian,
            'є' | 'і' | 'ї' | 'ґ' => return Language::Ukrainian,
            'ө' | 'ү' | 'һ' => return Language::Mongolian,
            'ј' | 'љ' | 'њ' | 'ћ' | 'ђ' | 'џ' => return Language::SerbianCyrillic,
            // Bulgarian 'ъ' is also in Russian, but 'щ' is a stronger indicator.
            // The logic implies that if either is present, it might be Bulgarian.
            'щ' => return Language::Bulgarian,
            _ => {}
        }
    }

    Language::Russian
}

fn detect_devanagari_language(text: &str) -> Language {
    for c in text.chars() {
        match c {
            // Marathi has higher priority in the original logic. Return immediately.
            '\u{0933}' => return Language::Marathi, // ळ
            // Flag for Sanskrit Vedic extensions.
            '\u{1CD0}'..='\u{1CFF}' => return Language::Sanskrit,
            _ => (),
        }
    }

    Language::Hindi
}

fn detect_greek_language(text: &str) -> Language {
    for c in text.chars() {
        match c {
            // Coptic has higher priority. Return immediately.
            '\u{2C80}'..='\u{2CFF}' => return Language::Coptic,
            // Flag for Greek Extended (Polytonic) characters.
            '\u{1F00}'..='\u{1FFF}' => return Language::GreekPoly,
            _ => {}
        }
    }

    Language::GreekMono
}

#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn detect_latin_language(text: &str) -> Language {
    // Flags for languages checked near the end of the original if-else chain.
    let mut has_french_c = false;
    let mut has_portuguese_o = false;
    let mut has_portuguese_a = false;

    for c in text.chars() {
        match c {
            // --- Early Return Cases (in order of priority) ---
            'ß' => return Language::German1996,
            'ő' | 'ű' => return Language::Hungarian,
            'ł' => return Language::Polish,
            'ř' | 'ů' => return Language::Czech,
            'ľ' | 'ĺ' | 'ŕ' => return Language::Slovak,
            'ā' | 'ē' | 'ģ' | 'ī' | 'ķ' | 'ļ' | 'ņ' | 'ō' | 'ū' => {
                return Language::Latvian
            }
            'ą' | 'ę' | 'ė' | 'į' | 'ų' => return Language::Lithuanian,
            'ă' | 'ș' | 'ț' => return Language::Romanian,
            'ğ' | 'ı' | 'ş' => return Language::Turkish,
            'đ' => return Language::Croatian, /* Also used in Vietnamese, but Croatian is the */
            // original's intent
            'þ' | 'ð' => return Language::Icelandic,
            'ŵ' | 'ŷ' => return Language::Welsh,
            'æ' | 'ø' => return Language::NorwegianBokmal, // And Danish
            'å' => return Language::Swedish,               // And Norwegian, Finnish
            'ñ' => return Language::Spanish,
            'ä' | 'ö' | 'ü' => return Language::German1996,

            // NOTE: 'õ' is used by both Estonian and Portuguese
            // Since Estonian is checked first, it takes precedence.
            'õ' => has_portuguese_o = true,
            'ã' => has_portuguese_a = true,

            // --- Flag-setting Cases ---
            'ç' => has_french_c = true, // Also in Portuguese
            'á' | 'é' | 'í' | 'ó' | 'ú' => return Language::Spanish,

            _ => (),
        }
    }

    // decide between portuguese, estonian and french

    if has_french_c && !has_portuguese_o && !has_portuguese_a {
        return Language::French;
    }

    if has_portuguese_o && !has_french_c && !has_portuguese_a {
        return Language::Estonian;
    }

    if has_portuguese_o || has_portuguese_a || has_french_c {
        return Language::Portuguese;
    }

    Language::EnglishUS
}

#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[must_use] pub fn script_to_language(script: Script, text: &str) -> Language {
    match script {
        Script::Ethiopic => Language::Ethiopic,
        Script::Georgian => Language::Georgian,
        Script::Gujarati => Language::Gujarati,
        Script::Gurmukhi => Language::Panjabi,
        Script::Kannada => Language::Kannada,
        Script::Malayalam => Language::Malayalam,
        Script::Mandarin => Language::Chinese,
        Script::Oriya => Language::Oriya,
        Script::Tamil => Language::Tamil,
        Script::Telugu => Language::Telugu,
        Script::Thai => Language::Thai,
        Script::Bengali => detect_bengali_language(text),
        Script::Cyrillic => detect_cyrillic_language(text),
        Script::Devanagari => detect_devanagari_language(text),
        Script::Greek => detect_greek_language(text),
        Script::Latin => detect_latin_language(text),

        // not directly matchable
        Script::Myanmar => Language::Thai,
        Script::Khmer => Language::Thai,
        Script::Sinhala => Language::Hindi,

        // no classical hyphenation behaviour
        Script::Arabic => Language::Chinese,
        Script::Hebrew => Language::Chinese,
        Script::Hangul => Language::Chinese,
        Script::Hiragana => Language::Chinese,
        Script::Katakana => Language::Chinese,
    }
}

#[must_use] pub const fn is_cyrillic(ch: char) -> bool {
    matches!(ch,
        '\u{0400}'..='\u{0484}'
        | '\u{0487}'..='\u{052F}'
        | '\u{2DE0}'..='\u{2DFF}'
        | '\u{A640}'..='\u{A69D}'
        | '\u{1D2B}'
        | '\u{1D78}'
        | '\u{A69F}'
    )
}

// https://en.wikipedia.org/wiki/Latin_script_in_Unicode
#[must_use] pub const fn is_latin(ch: char) -> bool {
    matches!(ch,
        'a'..='z'
        | 'A'..='Z'
        | '\u{0080}'..='\u{00FF}'
        | '\u{0100}'..='\u{017F}'
        | '\u{0180}'..='\u{024F}'
        | '\u{0250}'..='\u{02AF}'
        | '\u{1D00}'..='\u{1D7F}'
        | '\u{1D80}'..='\u{1DBF}'
        | '\u{1E00}'..='\u{1EFF}'
        | '\u{2100}'..='\u{214F}'
        | '\u{2C60}'..='\u{2C7F}'
        | '\u{A720}'..='\u{A7FF}'
        | '\u{AB30}'..='\u{AB6F}'
    )
}

// Based on https://en.wikipedia.org/wiki/Arabic_script_in_Unicode
#[must_use] pub const fn is_arabic(ch: char) -> bool {
    matches!(ch,
        '\u{0600}'..='\u{06FF}'
        | '\u{0750}'..='\u{07FF}'
        | '\u{08A0}'..='\u{08FF}'
        | '\u{FB50}'..='\u{FDFF}'
        | '\u{FE70}'..='\u{FEFF}'
        | '\u{10E60}'..='\u{10E7F}'
        | '\u{1EE00}'..='\u{1EEFF}'
    )
}

// Based on https://en.wikipedia.org/wiki/Devanagari#Unicode
#[must_use] pub const fn is_devanagari(ch: char) -> bool {
    matches!(ch, '\u{0900}'..='\u{097F}' | '\u{A8E0}'..='\u{A8FF}' | '\u{1CD0}'..='\u{1CFF}')
}

// Based on https://www.key-shortcut.com/en/writing-systems/ethiopian-script/
#[must_use] pub const fn is_ethiopic(ch: char) -> bool {
    matches!(ch, '\u{1200}'..='\u{139F}' | '\u{2D80}'..='\u{2DDF}' | '\u{AB00}'..='\u{AB2F}')
}

// Based on https://en.wikipedia.org/wiki/Hebrew_(Unicode_block)
#[must_use] pub const fn is_hebrew(ch: char) -> bool {
    matches!(ch, '\u{0590}'..='\u{05FF}')
}

#[must_use] pub const fn is_georgian(ch: char) -> bool {
    matches!(ch, '\u{10A0}'..='\u{10FF}')
}

#[must_use] pub const fn is_mandarin(ch: char) -> bool {
    matches!(ch,
        '\u{2E80}'..='\u{2E99}'
        | '\u{2E9B}'..='\u{2EF3}'
        | '\u{2F00}'..='\u{2FD5}'
        | '\u{3005}'
        | '\u{3007}'
        | '\u{3021}'..='\u{3029}'
        | '\u{3038}'..='\u{303B}'
        | '\u{3400}'..='\u{4DB5}'
        | '\u{4E00}'..='\u{9FCC}'
        | '\u{F900}'..='\u{FA6D}'
        | '\u{FA70}'..='\u{FAD9}'
    )
}

#[must_use] pub const fn is_bengali(ch: char) -> bool {
    matches!(ch, '\u{0980}'..='\u{09FF}')
}

#[must_use] pub const fn is_hiragana(ch: char) -> bool {
    matches!(ch, '\u{3040}'..='\u{309F}')
}

#[must_use] pub const fn is_katakana(ch: char) -> bool {
    matches!(ch,
        '\u{30A0}'..='\u{30FF}'
        // Halfwidth Katakana (part of the Halfwidth and Fullwidth Forms block).
        // U+FF66..=FF9F are katakana; U+FF61..=FF65 are halfwidth CJK punctuation.
        | '\u{FF66}'..='\u{FF9F}'
    )
}

// Hangul is Korean Alphabet. Unicode ranges are taken from: https://en.wikipedia.org/wiki/Hangul
#[must_use] pub const fn is_hangul(ch: char) -> bool {
    matches!(ch,
        '\u{AC00}'..='\u{D7AF}'
        | '\u{1100}'..='\u{11FF}'
        | '\u{3130}'..='\u{318F}'
        | '\u{A960}'..='\u{A97F}'
        | '\u{D7B0}'..='\u{D7FF}'
        // Halfwidth Hangul variants only. The rest of the Halfwidth and Fullwidth
        // Forms block (U+FF00..=FF60 fullwidth ASCII/Latin, U+FF61..=FF9F halfwidth
        // katakana/punct, U+FFE0..=FFEF fullwidth/halfwidth symbols) and Enclosed CJK
        // Letters and Months (U+3200..=32FF) are NOT Hangul and were previously
        // swallowed here, misclassifying halfwidth kana and fullwidth Latin as Hangul.
        | '\u{FFA0}'..='\u{FFDC}'
    )
}

// Taken from: https://en.wikipedia.org/wiki/Greek_and_Coptic
#[must_use] pub const fn is_greek(ch: char) -> bool {
    matches!(ch, '\u{0370}'..='\u{03FF}')
}

// Based on: https://en.wikipedia.org/wiki/Kannada_(Unicode_block)
#[must_use] pub const fn is_kannada(ch: char) -> bool {
    matches!(ch, '\u{0C80}'..='\u{0CFF}')
}

// Based on: https://en.wikipedia.org/wiki/Tamil_(Unicode_block)
#[must_use] pub const fn is_tamil(ch: char) -> bool {
    matches!(ch, '\u{0B80}'..='\u{0BFF}')
}

// Based on: https://en.wikipedia.org/wiki/Thai_(Unicode_block)
#[must_use] pub const fn is_thai(ch: char) -> bool {
    matches!(ch, '\u{0E00}'..='\u{0E7F}')
}

// Based on: https://en.wikipedia.org/wiki/Gujarati_(Unicode_block)
#[must_use] pub const fn is_gujarati(ch: char) -> bool {
    matches!(ch, '\u{0A80}'..='\u{0AFF}')
}

// Gurmukhi is the script for Punjabi language.
// Based on: https://en.wikipedia.org/wiki/Gurmukhi_(Unicode_block)
#[must_use] pub const fn is_gurmukhi(ch: char) -> bool {
    matches!(ch, '\u{0A00}'..='\u{0A7F}')
}

#[must_use] pub const fn is_telugu(ch: char) -> bool {
    matches!(ch, '\u{0C00}'..='\u{0C7F}')
}

// Based on: https://en.wikipedia.org/wiki/Malayalam_(Unicode_block)
#[must_use] pub const fn is_malayalam(ch: char) -> bool {
    matches!(ch, '\u{0D00}'..='\u{0D7F}')
}

// Based on: https://en.wikipedia.org/wiki/Oriya_(Unicode_block)
#[must_use] pub const fn is_oriya(ch: char) -> bool {
    matches!(ch, '\u{0B00}'..='\u{0B7F}')
}

// Based on: https://en.wikipedia.org/wiki/Myanmar_(Unicode_block)
#[must_use] pub const fn is_myanmar(ch: char) -> bool {
    matches!(ch, '\u{1000}'..='\u{109F}')
}

// Based on: https://en.wikipedia.org/wiki/Sinhala_(Unicode_block)
#[must_use] pub const fn is_sinhala(ch: char) -> bool {
    matches!(ch, '\u{0D80}'..='\u{0DFF}')
}

// Based on: https://en.wikipedia.org/wiki/Khmer_alphabet
#[must_use] pub const fn is_khmer(ch: char) -> bool {
    matches!(ch, '\u{1780}'..='\u{17FF}' | '\u{19E0}'..='\u{19FF}')
}

#[cfg(test)]
mod script_class_tests {
    use super::{detect_script, is_hangul, is_katakana, Script};

    #[test]
    fn halfwidth_katakana_is_not_hangul() {
        // U+FF71..FF73 = halfwidth katakana ｱｲｳ — must classify as Katakana, not Hangul.
        for ch in ['\u{FF71}', '\u{FF72}', '\u{FF73}'] {
            assert!(!is_hangul(ch), "{ch:?} wrongly matched is_hangul");
            assert!(is_katakana(ch), "{ch:?} should match is_katakana");
        }
        assert_eq!(detect_script("\u{FF71}\u{FF72}\u{FF73}"), Some(Script::Katakana));
    }

    #[test]
    fn fullwidth_latin_is_not_hangul() {
        // U+FF21..FF23 = fullwidth ＡＢＣ — must not be classified as Hangul.
        for ch in ['\u{FF21}', '\u{FF22}', '\u{FF23}'] {
            assert!(!is_hangul(ch), "{ch:?} wrongly matched is_hangul");
        }
        assert_ne!(detect_script("\u{FF21}\u{FF22}\u{FF23}"), Some(Script::Hangul));
    }

    #[test]
    fn real_hangul_still_detected() {
        assert!(is_hangul('\u{AC00}')); // 가
        assert_eq!(detect_script("\u{AC00}\u{AC01}"), Some(Script::Hangul));
    }
}

#[cfg(test)]
#[allow(clippy::unicode_not_nfc, clippy::non_ascii_literal)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------

    /// Every `Script` variant, in declaration order.
    const ALL_SCRIPTS: [Script; 24] = [
        Script::Arabic,
        Script::Bengali,
        Script::Cyrillic,
        Script::Devanagari,
        Script::Ethiopic,
        Script::Georgian,
        Script::Greek,
        Script::Gujarati,
        Script::Gurmukhi,
        Script::Hangul,
        Script::Hebrew,
        Script::Hiragana,
        Script::Kannada,
        Script::Katakana,
        Script::Khmer,
        Script::Latin,
        Script::Malayalam,
        Script::Mandarin,
        Script::Myanmar,
        Script::Oriya,
        Script::Sinhala,
        Script::Tamil,
        Script::Telugu,
        Script::Thai,
    ];

    /// `Script` has no `Hash`/`Ord`, so index it by hand for set-like bookkeeping.
    fn script_index(s: Script) -> usize {
        ALL_SCRIPTS
            .iter()
            .position(|&x| x == s)
            .expect("ALL_SCRIPTS must list every Script variant")
    }

    /// The first checker in `SCRIPT_CHECKERS` that claims `ch` — i.e. exactly what
    /// both `detect_char_script` and (for a 1-char text) `detect_script` must return.
    fn first_checker_hit(ch: char) -> Option<Script> {
        SCRIPT_CHECKERS
            .iter()
            .find(|(_, check_fn)| check_fn(ch))
            .map(|&(script, _)| script)
    }

    /// Iterate every scalar value in the BMP (surrogates are not `char`s).
    fn bmp_chars() -> impl Iterator<Item = char> {
        (0u32..=0xFFFF).filter_map(char::from_u32)
    }

    /// Deterministic pseudo-random scalar values — no `rand` dependency, and the
    /// sequence is identical on every run so a failure is always reproducible.
    fn lcg_chars(count: usize, seed: u64) -> String {
        let mut state = seed;
        let mut out = String::with_capacity(count * 4);
        let mut pushed = 0usize;
        while pushed < count {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let cp = (state >> 16) as u32 % 0x0011_0000;
            if let Some(ch) = char::from_u32(cp) {
                out.push(ch);
                pushed += 1;
            }
        }
        out
    }

    // ---------------------------------------------------------------------
    // is_stop_char (predicate)
    // ---------------------------------------------------------------------

    // Const-evaluability is part of the API: these must fold at compile time.
    const _: bool = is_stop_char(' ');
    const _: bool = is_latin('a');
    const _: bool = is_khmer('\u{1780}');

    #[test]
    fn is_stop_char_basic_true_false() {
        for ch in [
            '\u{0000}', '\t', '\n', '\r', ' ', '!', '0', '9', '@', '[', '\\', ']', '^', '_', '`',
            '{', '|', '}', '~',
        ] {
            assert!(is_stop_char(ch), "{ch:?} should be a stop char");
        }
        for ch in ['a', 'z', 'A', 'Z', '\u{007F}', 'é', 'あ', 'م', '\u{10FFFF}'] {
            assert!(!is_stop_char(ch), "{ch:?} should not be a stop char");
        }
    }

    #[test]
    fn is_stop_char_range_boundaries_are_exact() {
        // '\u{0000}'..='\u{0040}'
        assert!(is_stop_char('\u{0040}'));
        assert!(!is_stop_char('\u{0041}')); // 'A' — first char past the first range
        // '\u{005B}'..='\u{0060}'
        assert!(!is_stop_char('\u{005A}')); // 'Z'
        assert!(is_stop_char('\u{005B}'));
        assert!(is_stop_char('\u{0060}'));
        assert!(!is_stop_char('\u{0061}')); // 'a'
        // '\u{007B}'..='\u{007E}'
        assert!(!is_stop_char('\u{007A}')); // 'z'
        assert!(is_stop_char('\u{007B}'));
        assert!(is_stop_char('\u{007E}'));
        // U+007F (DEL) is deliberately *outside* every stop range and every script
        // range: it is counted toward `half` in detect_script but never scores.
        assert!(!is_stop_char('\u{007F}'));
        assert_eq!(detect_char_script('\u{007F}'), None);
        assert!(!is_stop_char('\u{0080}'));
    }

    #[test]
    fn stop_chars_never_carry_a_script() {
        // Invariant the whole detector rests on: a stop char scores for nothing,
        // so a stop-only text can never produce a Some(script).
        for ch in bmp_chars() {
            if is_stop_char(ch) {
                assert_eq!(
                    detect_char_script(ch),
                    None,
                    "stop char {ch:?} (U+{:04X}) also matched a script checker",
                    ch as u32
                );
            }
        }
    }

    // ---------------------------------------------------------------------
    // detect_script (parser)
    // ---------------------------------------------------------------------

    #[test]
    fn detect_script_empty_input_returns_none() {
        assert_eq!(detect_script(""), None);
    }

    #[test]
    fn detect_script_whitespace_only_returns_none() {
        for text in ["   ", "\t\n", "\r\n\r\n", "\t \t \n", "\u{0000}\u{0000}"] {
            assert_eq!(detect_script(text), None, "whitespace {text:?}");
        }
    }

    #[test]
    fn detect_script_non_ascii_whitespace_scores_nothing() {
        // U+2003 EM SPACE / U+2028 LINE SEPARATOR are *not* stop chars (they are
        // above U+007E) but no checker claims them either — so still None.
        assert_eq!(detect_script("\u{2003}\u{2003}"), None);
        assert_eq!(detect_script("\u{2028}"), None);
    }

    #[test]
    fn detect_script_garbage_returns_none_without_panicking() {
        for text in [
            "\u{0001}\u{0002}\u{0003}",
            "\u{007F}\u{007F}\u{007F}",
            "!@#$%^&*()_+-=[]{}|;':\",./<>?",
            "\u{FFFD}\u{FFFD}",
            "\u{200B}\u{200C}\u{200D}", // zero-width space / non-joiner / joiner
        ] {
            assert_eq!(detect_script(text), None, "garbage {text:?}");
        }
    }

    #[test]
    fn detect_script_boundary_number_strings() {
        // Digits, signs and dots are all stop chars → nothing to score.
        for text in [
            "0",
            "-0",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "18446744073709551615", // u64::MAX
            "1e309",                // f64 overflow literal — 'e' is Latin though
            "0.0000000000000000001",
        ] {
            let got = detect_script(text);
            let expected = if text.chars().any(|c| c.is_ascii_alphabetic()) {
                Some(Script::Latin)
            } else {
                None
            };
            assert_eq!(got, expected, "numeric text {text:?}");
        }
        // "NaN" / "inf" are pure ASCII letters → Latin, not a crash.
        assert_eq!(detect_script("NaN"), Some(Script::Latin));
        assert_eq!(detect_script("inf"), Some(Script::Latin));
        assert_eq!(detect_script("-inf"), Some(Script::Latin));
    }

    #[test]
    fn detect_script_leading_trailing_junk_is_skipped() {
        assert_eq!(detect_script("  hello  "), Some(Script::Latin));
        assert_eq!(detect_script("valid;garbage"), Some(Script::Latin));
        assert_eq!(detect_script("\t\nПривет\t\n"), Some(Script::Cyrillic));
    }

    #[test]
    fn detect_script_minority_script_still_wins_over_stop_chars() {
        // "a!!!!!!!!" is 9 chars → half == 4, so the single Latin char never crosses
        // the early-exit threshold. It must still win via the max-count fallback
        // (count != 0), not fall through to None.
        assert_eq!(detect_script("a!!!!!!!!"), Some(Script::Latin));
        assert_eq!(detect_script("!!!!!!!!!"), None);
    }

    #[test]
    fn detect_script_unicode_input_does_not_panic() {
        for text in [
            "\u{1F600}",                           // emoji, matches no script
            "\u{1F600}\u{1F468}\u{200D}\u{1F469}", // ZWJ sequence
            "e\u{0301}",                           // 'e' + combining acute
            "\u{0301}\u{0302}\u{0303}",            // bare combining marks
            "\u{10FFFF}",                          // char::MAX
            "\u{FFFF}",                            // BMP noncharacter
        ] {
            let got = detect_script(text);
            assert_eq!(got, detect_script(text), "not deterministic for {text:?}");
        }
        assert_eq!(detect_script("\u{1F600}"), None);
        assert_eq!(detect_script("\u{0301}\u{0302}"), None);
        assert_eq!(detect_script("\u{10FFFF}"), None);
        assert_eq!(detect_script("e\u{0301}"), Some(Script::Latin));
    }

    #[test]
    fn detect_script_deeply_nested_brackets_do_not_stack_overflow() {
        // 10_000 nested brackets: the detector is iterative, and every bracket is a
        // stop char, so this must terminate with None rather than recursing.
        let depth = 10_000;
        let mut text = String::with_capacity(depth * 2);
        for _ in 0..depth {
            text.push('(');
        }
        for _ in 0..depth {
            text.push(')');
        }
        assert_eq!(detect_script(&text), None);

        let mut nested = String::new();
        for _ in 0..depth {
            nested.push_str("{[");
        }
        for _ in 0..depth {
            nested.push_str("]}");
        }
        assert_eq!(detect_script(&nested), None);
    }

    #[test]
    fn detect_script_extremely_long_input_terminates() {
        // 1M identical Latin chars: must early-exit once count > half.
        let long_latin = "a".repeat(1_000_000);
        assert_eq!(detect_script(&long_latin), Some(Script::Latin));

        // 200k chars that match *no* checker: worst case — all 24 checkers run for
        // every char and there is no early exit. Must still finish, returning None.
        let long_junk = "\u{1F600}".repeat(200_000);
        assert_eq!(detect_script(&long_junk), None);

        // 200k stop chars: skipped, but still walked.
        let long_stops = " ".repeat(200_000);
        assert_eq!(detect_script(&long_stops), None);
    }

    #[test]
    fn detect_script_long_mixed_script_input_is_deterministic() {
        // Alternating scripts defeat the "move winner to the front" heuristic and
        // never cross the half threshold. It must still terminate and be stable.
        let mixed = "aб".repeat(100_000);
        let first = detect_script(&mixed);
        let second = detect_script(&mixed);
        assert_eq!(first, second, "detect_script is not deterministic");
        assert!(
            first == Some(Script::Latin) || first == Some(Script::Cyrillic),
            "expected one of the two present scripts, got {first:?}"
        );
    }

    #[test]
    fn detect_script_pseudo_random_garbage_never_panics() {
        for seed in [1u64, 0xDEAD_BEEF, u64::MAX] {
            let text = lcg_chars(5_000, seed);
            let first = detect_script(&text);
            let second = detect_script(&text);
            assert_eq!(first, second, "non-deterministic for seed {seed}");
        }
    }

    #[test]
    fn detect_script_majority_wins() {
        assert_eq!(detect_script("aaaaaб"), Some(Script::Latin));
        assert_eq!(detect_script("бббббa"), Some(Script::Cyrillic));
        // Latin body with a couple of CJK chars mixed in.
        assert_eq!(detect_script("hello 世界"), Some(Script::Latin));
        assert_eq!(detect_script("世界世界 hi"), Some(Script::Mandarin));
    }

    #[test]
    fn detect_script_valid_minimal_positive_controls() {
        let cases: [(&str, Script); 16] = [
            ("hello", Script::Latin),
            ("Привет", Script::Cyrillic),
            ("مرحبا", Script::Arabic),
            ("你好世界", Script::Mandarin),
            ("नमस्ते", Script::Devanagari),
            ("שלום", Script::Hebrew),
            ("ሰላም", Script::Ethiopic),
            ("გამარჯობა", Script::Georgian),
            ("আমার", Script::Bengali),
            ("안녕하세요", Script::Hangul),
            ("こんにちは", Script::Hiragana),
            ("カタカナ", Script::Katakana),
            ("Γειά", Script::Greek),
            ("ಕನ್ನಡ", Script::Kannada),
            ("தமிழ்", Script::Tamil),
            ("สวัสดี", Script::Thai),
        ];
        for (text, expected) in cases {
            assert_eq!(detect_script(text), Some(expected), "text {text:?}");
        }
    }

    #[test]
    fn detect_script_is_pure_no_state_leaks_between_calls() {
        // detect_script mutates (swaps) its counter table; that table must be local.
        // Priming it with Cyrillic must not change the verdict for a later Latin text.
        assert_eq!(detect_script("ббббб"), Some(Script::Cyrillic));
        assert_eq!(detect_script("aaaaa"), Some(Script::Latin));
        assert_eq!(detect_script("ббббб"), Some(Script::Cyrillic));
        assert_eq!(detect_script("aaaaa"), Some(Script::Latin));
    }

    #[test]
    fn detect_script_single_char_agrees_with_detect_char_script() {
        // Strong cross-check over the whole BMP: a 1-char text has half == 0, so the
        // first checker that claims the char wins immediately — exactly what
        // detect_char_script returns. Any divergence is a table-ordering bug.
        for ch in bmp_chars() {
            let expected = first_checker_hit(ch);
            assert_eq!(
                detect_char_script(ch),
                expected,
                "detect_char_script disagrees with SCRIPT_CHECKERS for U+{:04X}",
                ch as u32
            );
            let text = ch.to_string();
            assert_eq!(
                detect_script(&text),
                expected,
                "detect_script disagrees with detect_char_script for U+{:04X}",
                ch as u32
            );
        }
    }

    // ---------------------------------------------------------------------
    // detect_char_script (dispatch table)
    // ---------------------------------------------------------------------

    #[test]
    fn detect_char_script_extreme_inputs() {
        assert_eq!(detect_char_script('\u{0000}'), None);
        assert_eq!(detect_char_script('\u{10FFFF}'), None); // char::MAX
        assert_eq!(detect_char_script(char::MAX), None);
        assert_eq!(detect_char_script('\u{FFFF}'), None); // noncharacter
        assert_eq!(detect_char_script('\u{E000}'), None); // private use area
        assert_eq!(detect_char_script('a'), Some(Script::Latin));
        assert_eq!(detect_char_script('\u{1EE00}'), Some(Script::Arabic)); // astral Arabic
        assert_eq!(detect_char_script('\u{10E60}'), Some(Script::Arabic)); // Rumi digits
    }

    #[test]
    fn detect_char_script_astral_planes_agree_with_the_table() {
        // The two astral Arabic ranges plus the surrounding gaps, which the BMP
        // sweep above cannot reach.
        for cp in (0x1_0E00u32..=0x1_0F00).chain(0x1_ED00..=0x1_EF00).chain([
            0x1_F600, 0x2_0000, 0x10_FFFF,
        ]) {
            let Some(ch) = char::from_u32(cp) else {
                continue;
            };
            assert_eq!(
                detect_char_script(ch),
                first_checker_hit(ch),
                "astral U+{cp:05X} disagrees with SCRIPT_CHECKERS"
            );
            assert_eq!(
                detect_script(&ch.to_string()),
                first_checker_hit(ch),
                "astral U+{cp:05X}: detect_script != detect_char_script"
            );
        }
    }

    #[test]
    fn detect_char_script_none_implies_no_checker_matched() {
        for ch in bmp_chars() {
            if detect_char_script(ch).is_none() {
                for (script, check_fn) in SCRIPT_CHECKERS {
                    assert!(
                        !check_fn(ch),
                        "U+{:04X} is unclassified yet {script:?}'s checker claims it",
                        ch as u32
                    );
                }
            }
        }
    }

    #[test]
    fn detect_char_script_some_implies_that_scripts_checker_matched() {
        for ch in bmp_chars() {
            if let Some(script) = detect_char_script(ch) {
                let (_, check_fn) = SCRIPT_CHECKERS[script_index_in_table(script)];
                assert!(
                    check_fn(ch),
                    "detect_char_script said {script:?} for U+{:04X} but its checker says no",
                    ch as u32
                );
            }
        }
    }

    fn script_index_in_table(script: Script) -> usize {
        SCRIPT_CHECKERS
            .iter()
            .position(|&(s, _)| s == script)
            .expect("every Script must appear in SCRIPT_CHECKERS")
    }

    #[test]
    fn script_checkers_table_covers_every_script_exactly_once() {
        assert_eq!(SCRIPT_CHECKERS.len(), ALL_SCRIPTS.len());
        let mut seen = [0usize; 24];
        for (script, _) in SCRIPT_CHECKERS {
            seen[script_index(script)] += 1;
        }
        for (i, count) in seen.iter().enumerate() {
            assert_eq!(*count, 1, "{:?} appears {count} times in SCRIPT_CHECKERS", ALL_SCRIPTS[i]);
        }
    }

    #[test]
    fn every_script_is_reachable_from_some_bmp_char() {
        // Guards against a checker being fully shadowed by an earlier, broader one:
        // if some script can never be produced, the table order has swallowed it.
        let mut reachable = [false; 24];
        for ch in bmp_chars() {
            if let Some(script) = detect_char_script(ch) {
                reachable[script_index(script)] = true;
            }
        }
        for (i, ok) in reachable.iter().enumerate() {
            assert!(*ok, "{:?} is unreachable — shadowed by an earlier checker", ALL_SCRIPTS[i]);
        }
    }

    #[test]
    fn overlapping_ranges_resolve_to_the_first_checker_in_the_table() {
        // U+1D2B (CYRILLIC LETTER SMALL CAPITAL EL) and U+1D78 (MODIFIER LETTER
        // CYRILLIC EN) are listed by *both* is_latin (via U+1D00..=U+1D7F) and
        // is_cyrillic. Latin is checked first, so Latin wins. Pinned here because a
        // reordering of SCRIPT_CHECKERS would silently flip these to Cyrillic.
        for ch in ['\u{1D2B}', '\u{1D78}'] {
            assert!(is_latin(ch), "{ch:?} in is_latin's U+1D00..=U+1D7F range");
            assert!(is_cyrillic(ch), "{ch:?} is explicitly listed by is_cyrillic");
            assert_eq!(detect_char_script(ch), Some(Script::Latin));
            assert_eq!(detect_script(&ch.to_string()), Some(Script::Latin));
        }
    }

    // ---------------------------------------------------------------------
    // is_* predicates: exact range boundaries
    // ---------------------------------------------------------------------

    /// Assert a predicate accepts both ends and the midpoint of `[lo, hi]`. The chars
    /// bracketing the range are checked separately with `assert_rejects`, because a
    /// neighbour may legitimately belong to another range of the *same* predicate.
    fn assert_range(name: &str, f: fn(char) -> bool, lo: u32, hi: u32) {
        for cp in [lo, hi] {
            let ch = char::from_u32(cp).unwrap_or_else(|| panic!("{name}: U+{cp:04X} not a char"));
            assert!(f(ch), "{name} should accept its boundary U+{cp:04X}");
        }
        let mid = char::from_u32(lo + (hi - lo) / 2).unwrap();
        assert!(f(mid), "{name} should accept its midpoint {mid:?}");
    }

    fn assert_rejects(name: &str, f: fn(char) -> bool, cps: &[u32]) {
        for &cp in cps {
            let Some(ch) = char::from_u32(cp) else { continue };
            assert!(!f(ch), "{name} should reject U+{cp:04X}");
        }
    }

    #[test]
    fn single_range_predicates_have_exact_boundaries() {
        // (name, fn, start, end): each of these is a single contiguous block, so the
        // chars immediately before/after must be rejected.
        let cases: [(&str, fn(char) -> bool, u32, u32); 12] = [
            ("is_hebrew", is_hebrew, 0x0590, 0x05FF),
            ("is_georgian", is_georgian, 0x10A0, 0x10FF),
            ("is_bengali", is_bengali, 0x0980, 0x09FF),
            ("is_hiragana", is_hiragana, 0x3040, 0x309F),
            ("is_greek", is_greek, 0x0370, 0x03FF),
            ("is_kannada", is_kannada, 0x0C80, 0x0CFF),
            ("is_tamil", is_tamil, 0x0B80, 0x0BFF),
            ("is_thai", is_thai, 0x0E00, 0x0E7F),
            ("is_gujarati", is_gujarati, 0x0A80, 0x0AFF),
            ("is_gurmukhi", is_gurmukhi, 0x0A00, 0x0A7F),
            ("is_telugu", is_telugu, 0x0C00, 0x0C7F),
            ("is_malayalam", is_malayalam, 0x0D00, 0x0D7F),
        ];
        for (name, f, lo, hi) in cases {
            assert_range(name, f, lo, hi);
            assert_rejects(name, f, &[lo - 1, hi + 1, 0x0000, 0x0041, 0x10_FFFF]);
        }
        // The two remaining single-range predicates, spelled out (0x0B00-1 etc. all
        // land in neighbouring script blocks, which is exactly what we want to check).
        assert_range("is_oriya", is_oriya, 0x0B00, 0x0B7F);
        assert_rejects("is_oriya", is_oriya, &[0x0AFF, 0x0B80]);
        assert_range("is_myanmar", is_myanmar, 0x1000, 0x109F);
        assert_rejects("is_myanmar", is_myanmar, &[0x0FFF, 0x10A0]);
        assert_range("is_sinhala", is_sinhala, 0x0D80, 0x0DFF);
        assert_rejects("is_sinhala", is_sinhala, &[0x0D7F, 0x0E00]);
    }

    #[test]
    fn is_latin_boundaries() {
        assert!(is_latin('a') && is_latin('z') && is_latin('A') && is_latin('Z'));
        // The chars bracketing the ASCII letter ranges are all stop chars.
        assert_rejects("is_latin", is_latin, &[0x0040, 0x005B, 0x0060, 0x007B, 0x007F]);
        assert_range("is_latin", is_latin, 0x0080, 0x024F); // Latin-1 Sup .. Latin Ext-B
        assert_range("is_latin", is_latin, 0x0250, 0x02AF); // IPA extensions
        assert_rejects("is_latin", is_latin, &[0x02B0, 0x0300, 0x0400, 0x1CFF]);
        assert_range("is_latin", is_latin, 0x1D00, 0x1DBF);
        assert_rejects("is_latin", is_latin, &[0x1DC0]);
        assert_range("is_latin", is_latin, 0x1E00, 0x1EFF);
        assert_rejects("is_latin", is_latin, &[0x1DFF, 0x1F00]);
        assert_range("is_latin", is_latin, 0x2100, 0x214F);
        assert_rejects("is_latin", is_latin, &[0x20FF, 0x2150]);
        assert_range("is_latin", is_latin, 0x2C60, 0x2C7F);
        assert_rejects("is_latin", is_latin, &[0x2C5F, 0x2C80]);
        assert_range("is_latin", is_latin, 0xA720, 0xA7FF);
        assert_rejects("is_latin", is_latin, &[0xA71F, 0xA800]);
        assert_range("is_latin", is_latin, 0xAB30, 0xAB6F);
        assert_rejects("is_latin", is_latin, &[0xAB2F, 0xAB70]);
    }

    #[test]
    fn is_latin_swallows_latin1_symbols_and_letterlike_forms() {
        // Pinned quirk, not an endorsement: is_latin's U+0080..=U+00FF and
        // U+2100..=U+214F ranges are whole *blocks*, so NBSP, ©, ×, ÷, ™ and ℃ all
        // report as Latin and score for Latin in detect_script.
        for ch in ['\u{00A0}', '\u{00A9}', '\u{00D7}', '\u{00F7}', '\u{2122}', '\u{2103}'] {
            assert!(is_latin(ch), "U+{:04X} is inside is_latin's block ranges", ch as u32);
            assert_eq!(detect_script(&ch.to_string()), Some(Script::Latin));
        }
    }

    #[test]
    fn is_cyrillic_boundaries_including_the_titlo_gap() {
        assert_range("is_cyrillic", is_cyrillic, 0x0400, 0x0484);
        // U+0485/U+0486 (combining Cyrillic titlo) are deliberately excluded.
        assert_rejects("is_cyrillic", is_cyrillic, &[0x03FF, 0x0485, 0x0486, 0x0530]);
        assert_range("is_cyrillic", is_cyrillic, 0x0487, 0x052F);
        assert_range("is_cyrillic", is_cyrillic, 0x2DE0, 0x2DFF);
        assert_rejects("is_cyrillic", is_cyrillic, &[0x2DDF, 0x2E00]);
        assert_range("is_cyrillic", is_cyrillic, 0xA640, 0xA69D);
        assert!(is_cyrillic('\u{A69F}'));
        assert_rejects("is_cyrillic", is_cyrillic, &[0xA63F, 0xA69E, 0xA6A0]);
        assert!(is_cyrillic('\u{1D2B}') && is_cyrillic('\u{1D78}'));
    }

    #[test]
    fn is_arabic_boundaries_and_the_bom() {
        assert_range("is_arabic", is_arabic, 0x0600, 0x06FF);
        assert_rejects("is_arabic", is_arabic, &[0x05FF, 0x0700, 0x074F, 0x0800, 0x089F]);
        assert_range("is_arabic", is_arabic, 0x0750, 0x07FF);
        assert_range("is_arabic", is_arabic, 0x08A0, 0x08FF);
        assert_range("is_arabic", is_arabic, 0xFB50, 0xFDFF);
        assert_range("is_arabic", is_arabic, 0xFE70, 0xFEFF);
        assert_rejects("is_arabic", is_arabic, &[0xFB4F, 0xFE00, 0xFE6F, 0xFF00]);
        assert_range("is_arabic", is_arabic, 0x1_0E60, 0x1_0E7F);
        assert_range("is_arabic", is_arabic, 0x1_EE00, 0x1_EEFF);
        assert_rejects("is_arabic", is_arabic, &[0x1_0E5F, 0x1_0E80, 0x1_EDFF, 0x1_EF00]);

        // BUG PIN: U+FEFF is the byte-order mark / ZERO WIDTH NO-BREAK SPACE, whose
        // Unicode script is Common — but it sits at the top of the Arabic
        // Presentation Forms-B block, so is_arabic claims it. A BOM-prefixed text is
        // therefore scored as containing one Arabic char. Behaviour pinned as-is;
        // see the report.
        assert!(is_arabic('\u{FEFF}'));
        assert_eq!(detect_script("\u{FEFF}"), Some(Script::Arabic));
        // The BOM is not enough to beat a real majority, at least.
        assert_eq!(detect_script("\u{FEFF}hello"), Some(Script::Latin));
    }

    #[test]
    fn is_devanagari_boundaries() {
        assert_range("is_devanagari", is_devanagari, 0x0900, 0x097F);
        assert_range("is_devanagari", is_devanagari, 0xA8E0, 0xA8FF);
        assert_range("is_devanagari", is_devanagari, 0x1CD0, 0x1CFF); // Vedic extensions
        assert_rejects(
            "is_devanagari",
            is_devanagari,
            &[0x08FF, 0x0980, 0xA8DF, 0xA900, 0x1CCF, 0x1D00],
        );
    }

    #[test]
    fn is_ethiopic_boundaries() {
        assert_range("is_ethiopic", is_ethiopic, 0x1200, 0x139F);
        assert_range("is_ethiopic", is_ethiopic, 0x2D80, 0x2DDF);
        assert_range("is_ethiopic", is_ethiopic, 0xAB00, 0xAB2F);
        assert_rejects("is_ethiopic", is_ethiopic, &[0x11FF, 0x13A0, 0x2D7F, 0xAAFF]);
        // U+2DE0 is where Cyrillic Extended-A starts — must NOT be Ethiopic.
        assert!(!is_ethiopic('\u{2DE0}'));
        assert!(is_cyrillic('\u{2DE0}'));
        // U+AB30 is where is_latin's Latin Extended-E range starts.
        assert!(!is_ethiopic('\u{AB30}'));
        assert!(is_latin('\u{AB30}'));
    }

    #[test]
    fn is_mandarin_boundaries_and_gaps() {
        assert_range("is_mandarin", is_mandarin, 0x2E80, 0x2E99);
        assert!(!is_mandarin('\u{2E9A}')); // documented hole in the CJK Radicals block
        assert_range("is_mandarin", is_mandarin, 0x2E9B, 0x2EF3);
        assert_range("is_mandarin", is_mandarin, 0x2F00, 0x2FD5);
        assert!(is_mandarin('\u{3005}') && is_mandarin('\u{3007}'));
        assert!(!is_mandarin('\u{3006}')); // U+3006 IDEOGRAPHIC CLOSING MARK is excluded
        assert_range("is_mandarin", is_mandarin, 0x3021, 0x3029);
        assert_range("is_mandarin", is_mandarin, 0x3038, 0x303B);
        assert_range("is_mandarin", is_mandarin, 0x3400, 0x4DB5);
        assert_range("is_mandarin", is_mandarin, 0x4E00, 0x9FCC);
        assert_range("is_mandarin", is_mandarin, 0xF900, 0xFA6D);
        assert_range("is_mandarin", is_mandarin, 0xFA70, 0xFAD9);
        assert_rejects(
            "is_mandarin",
            is_mandarin,
            &[0x2E7F, 0x2EF4, 0x2FD6, 0x3004, 0x4DB6, 0x9FCD, 0xF8FF, 0xFA6E, 0xFADA],
        );
    }

    #[test]
    fn is_katakana_and_is_hangul_do_not_overlap_in_halfwidth_forms() {
        assert_range("is_katakana", is_katakana, 0x30A0, 0x30FF);
        assert_range("is_katakana", is_katakana, 0xFF66, 0xFF9F);
        assert_rejects("is_katakana", is_katakana, &[0x309F, 0x3100, 0xFF65, 0xFFA0]);

        assert_range("is_hangul", is_hangul, 0xAC00, 0xD7AF);
        assert_range("is_hangul", is_hangul, 0x1100, 0x11FF);
        assert_range("is_hangul", is_hangul, 0x3130, 0x318F);
        assert_range("is_hangul", is_hangul, 0xA960, 0xA97F);
        assert_range("is_hangul", is_hangul, 0xD7B0, 0xD7FF);
        assert_range("is_hangul", is_hangul, 0xFFA0, 0xFFDC);
        assert_rejects(
            "is_hangul",
            is_hangul,
            &[0x10FF, 0x1200, 0x312F, 0x3190, 0x3200, 0xABFF, 0xFF66, 0xFF9F, 0xFFDD, 0xFFE0],
        );

        // The two halfwidth ranges must stay disjoint.
        for cp in 0xFF61u32..=0xFFDCu32 {
            let ch = char::from_u32(cp).unwrap();
            assert!(
                !(is_katakana(ch) && is_hangul(ch)),
                "U+{cp:04X} claimed by both is_katakana and is_hangul"
            );
        }
    }

    #[test]
    fn is_khmer_boundaries() {
        assert_range("is_khmer", is_khmer, 0x1780, 0x17FF);
        assert_range("is_khmer", is_khmer, 0x19E0, 0x19FF);
        assert_rejects("is_khmer", is_khmer, &[0x177F, 0x1800, 0x19DF, 0x1A00]);
    }

    #[test]
    fn predicates_reject_the_extremes_and_are_pure() {
        for (script, check_fn) in SCRIPT_CHECKERS {
            for ch in ['\u{0000}', ' ', '0', '\u{007F}', '\u{10FFFF}'] {
                let first = check_fn(ch);
                assert_eq!(first, check_fn(ch), "{script:?} checker is not pure for {ch:?}");
                assert!(!first, "{script:?} checker claims the non-letter {ch:?}");
            }
        }
    }

    // ---------------------------------------------------------------------
    // detect_bengali_language
    // ---------------------------------------------------------------------

    #[test]
    fn detect_bengali_language_defaults_to_bengali() {
        assert_eq!(detect_bengali_language(""), Language::Bengali);
        assert_eq!(detect_bengali_language("   "), Language::Bengali);
        assert_eq!(detect_bengali_language("আমার সোনার বাংলা"), Language::Bengali);
        // Out-of-script text is not validated — it still falls through to Bengali.
        assert_eq!(detect_bengali_language("hello"), Language::Bengali);
        assert_eq!(detect_bengali_language("\u{1F600}"), Language::Bengali);
    }

    #[test]
    fn detect_bengali_language_finds_assamese_at_any_position() {
        for text in ["\u{09F0}", "\u{09F1}", "\u{09F0}আমার", "আমার\u{09F0}", "আ\u{09F1}র"] {
            assert_eq!(detect_bengali_language(text), Language::Assamese, "text {text:?}");
        }
        // Boundary: the code points either side of the ৰ/ৱ pair are plain Bengali.
        assert_eq!(detect_bengali_language("\u{09EF}"), Language::Bengali);
        assert_eq!(detect_bengali_language("\u{09F2}"), Language::Bengali);
    }

    #[test]
    fn detect_bengali_language_long_input_terminates() {
        let long = "আ".repeat(200_000);
        assert_eq!(detect_bengali_language(&long), Language::Bengali);
        // Assamese marker at the very end — worst case for the early-return scan.
        let mut with_marker = long.clone();
        with_marker.push('\u{09F0}');
        assert_eq!(detect_bengali_language(&with_marker), Language::Assamese);
    }

    // ---------------------------------------------------------------------
    // detect_cyrillic_language
    // ---------------------------------------------------------------------

    #[test]
    fn detect_cyrillic_language_defaults_to_russian() {
        assert_eq!(detect_cyrillic_language(""), Language::Russian);
        assert_eq!(detect_cyrillic_language("Привет мир"), Language::Russian);
        assert_eq!(detect_cyrillic_language("hello"), Language::Russian);
        assert_eq!(detect_cyrillic_language("\u{1F600}"), Language::Russian);
    }

    #[test]
    fn detect_cyrillic_language_markers() {
        let cases: [(&str, Language); 7] = [
            ("\u{0460}", Language::SlavonicChurch),
            ("ѓ", Language::Macedonian),
            ("ў", Language::Belarusian),
            ("ї", Language::Ukrainian),
            ("ө", Language::Mongolian),
            ("ј", Language::SerbianCyrillic),
            ("щ", Language::Bulgarian),
        ];
        for (text, expected) in cases {
            assert_eq!(detect_cyrillic_language(text), expected, "text {text:?}");
        }
        // Old-Cyrillic block boundaries: U+0460..=U+047F inclusive, nothing outside.
        assert_eq!(detect_cyrillic_language("\u{047F}"), Language::SlavonicChurch);
        assert_eq!(detect_cyrillic_language("\u{0480}"), Language::Russian);
        // U+045F sits one below the Old-Cyrillic range — and it is 'џ', so it falls
        // through to the Serbian arm rather than to the Russian default.
        assert_eq!(detect_cyrillic_language("\u{045F}"), Language::SerbianCyrillic);
    }

    #[test]
    fn detect_cyrillic_language_is_positional_not_priority_ordered() {
        // The comments in the function claim Old-Cyrillic is the "highest priority"
        // check, but every arm returns immediately, so the *first marker char in the
        // text* wins regardless of its claimed rank. Pinned; see the report.
        assert_eq!(detect_cyrillic_language("щ\u{0460}"), Language::Bulgarian);
        assert_eq!(detect_cyrillic_language("\u{0460}щ"), Language::SlavonicChurch);
        assert_eq!(detect_cyrillic_language("ўщ"), Language::Belarusian);
        assert_eq!(detect_cyrillic_language("щў"), Language::Bulgarian);
    }

    #[test]
    fn detect_cyrillic_language_long_input_terminates() {
        let long = "а".repeat(200_000);
        assert_eq!(detect_cyrillic_language(&long), Language::Russian);
        let mut trailing = long;
        trailing.push('\u{0460}');
        assert_eq!(detect_cyrillic_language(&trailing), Language::SlavonicChurch);
    }

    // ---------------------------------------------------------------------
    // detect_devanagari_language
    // ---------------------------------------------------------------------

    #[test]
    fn detect_devanagari_language_defaults_to_hindi() {
        assert_eq!(detect_devanagari_language(""), Language::Hindi);
        assert_eq!(detect_devanagari_language("नमस्ते"), Language::Hindi);
        assert_eq!(detect_devanagari_language("hello"), Language::Hindi);
    }

    #[test]
    fn detect_devanagari_language_markers_and_boundaries() {
        assert_eq!(detect_devanagari_language("\u{0933}"), Language::Marathi); // ळ
        assert_eq!(detect_devanagari_language("\u{1CD0}"), Language::Sanskrit);
        assert_eq!(detect_devanagari_language("\u{1CFF}"), Language::Sanskrit);
        assert_eq!(detect_devanagari_language("\u{1CCF}"), Language::Hindi);
        assert_eq!(detect_devanagari_language("\u{1D00}"), Language::Hindi);
        assert_eq!(detect_devanagari_language("\u{0932}"), Language::Hindi);
        assert_eq!(detect_devanagari_language("\u{0934}"), Language::Hindi);
        // Positional, not priority-ordered — whichever marker comes first wins.
        assert_eq!(detect_devanagari_language("\u{1CD0}\u{0933}"), Language::Sanskrit);
        assert_eq!(detect_devanagari_language("\u{0933}\u{1CD0}"), Language::Marathi);
    }

    #[test]
    fn detect_devanagari_language_long_input_terminates() {
        let long = "न".repeat(200_000);
        assert_eq!(detect_devanagari_language(&long), Language::Hindi);
    }

    // ---------------------------------------------------------------------
    // detect_greek_language
    // ---------------------------------------------------------------------

    #[test]
    fn detect_greek_language_defaults_to_monotonic() {
        assert_eq!(detect_greek_language(""), Language::GreekMono);
        assert_eq!(detect_greek_language("Γειά σου"), Language::GreekMono);
        assert_eq!(detect_greek_language("hello"), Language::GreekMono);
    }

    #[test]
    fn detect_greek_language_markers_and_boundaries() {
        assert_eq!(detect_greek_language("\u{2C80}"), Language::Coptic);
        assert_eq!(detect_greek_language("\u{2CFF}"), Language::Coptic);
        assert_eq!(detect_greek_language("\u{2C7F}"), Language::GreekMono);
        assert_eq!(detect_greek_language("\u{2D00}"), Language::GreekMono);
        assert_eq!(detect_greek_language("\u{1F00}"), Language::GreekPoly);
        assert_eq!(detect_greek_language("\u{1FFF}"), Language::GreekPoly);
        assert_eq!(detect_greek_language("\u{1EFF}"), Language::GreekMono);
        assert_eq!(detect_greek_language("\u{2000}"), Language::GreekMono);
        // Positional, not priority-ordered.
        assert_eq!(detect_greek_language("\u{1F00}\u{2C80}"), Language::GreekPoly);
        assert_eq!(detect_greek_language("\u{2C80}\u{1F00}"), Language::Coptic);
    }

    #[test]
    fn detect_greek_language_long_input_terminates() {
        let long = "α".repeat(200_000);
        assert_eq!(detect_greek_language(&long), Language::GreekMono);
    }

    // ---------------------------------------------------------------------
    // detect_latin_language
    // ---------------------------------------------------------------------

    #[test]
    fn detect_latin_language_defaults_to_english() {
        assert_eq!(detect_latin_language(""), Language::EnglishUS);
        assert_eq!(detect_latin_language("the quick brown fox"), Language::EnglishUS);
        assert_eq!(detect_latin_language("0123456789 !@#$%"), Language::EnglishUS);
        assert_eq!(detect_latin_language("\u{1F600}"), Language::EnglishUS);
        // Non-Latin text is not validated — it still falls through to English.
        assert_eq!(detect_latin_language("你好"), Language::EnglishUS);
    }

    #[test]
    fn detect_latin_language_single_char_markers() {
        let cases: [(char, Language); 17] = [
            ('ß', Language::German1996),
            ('ä', Language::German1996),
            ('ő', Language::Hungarian),
            ('ł', Language::Polish),
            ('ř', Language::Czech),
            ('ľ', Language::Slovak),
            ('ā', Language::Latvian),
            ('ą', Language::Lithuanian),
            ('ă', Language::Romanian),
            ('ğ', Language::Turkish),
            ('đ', Language::Croatian),
            ('þ', Language::Icelandic),
            ('ŵ', Language::Welsh),
            ('æ', Language::NorwegianBokmal),
            ('å', Language::Swedish),
            ('ñ', Language::Spanish),
            ('á', Language::Spanish),
        ];
        for (ch, expected) in cases {
            assert_eq!(detect_latin_language(&ch.to_string()), expected, "char {ch:?}");
            // Position within the text must not matter for early-return markers.
            assert_eq!(detect_latin_language(&format!("word {ch} word")), expected);
        }
    }

    #[test]
    fn detect_latin_language_flag_combinations() {
        // The three deferred flags (ç / õ / ã) drive the French-Estonian-Portuguese
        // tie-break at the end of the scan.
        assert_eq!(detect_latin_language("ç"), Language::French);
        assert_eq!(detect_latin_language("garçon"), Language::French);
        assert_eq!(detect_latin_language("õ"), Language::Estonian);
        assert_eq!(detect_latin_language("õhtu"), Language::Estonian);
        assert_eq!(detect_latin_language("ã"), Language::Portuguese);
        assert_eq!(detect_latin_language("çõ"), Language::Portuguese);
        assert_eq!(detect_latin_language("çã"), Language::Portuguese);
        assert_eq!(detect_latin_language("õã"), Language::Portuguese);
        assert_eq!(detect_latin_language("çõã"), Language::Portuguese);
        assert_eq!(detect_latin_language("informação"), Language::Portuguese);
    }

    #[test]
    fn detect_latin_language_accented_vowel_short_circuits_the_flags() {
        // BUG PIN: 'á'|'é'|'í'|'ó'|'ú' return Spanish *immediately*, so any French or
        // Portuguese word carrying an accented vowel before its ç/õ/ã is reported as
        // Spanish — the flag tie-break never runs. Pinned as-is; see the report.
        assert_eq!(detect_latin_language("café"), Language::Spanish);
        assert_eq!(detect_latin_language("présentation"), Language::Spanish);
        assert_eq!(detect_latin_language("é ç"), Language::Spanish);
        // Order matters: with the ç first, the flag survives to the tie-break — but
        // only because no accented vowel is seen at all.
        assert_eq!(detect_latin_language("ç e"), Language::French);
    }

    #[test]
    fn detect_latin_language_first_marker_wins() {
        assert_eq!(detect_latin_language("ßä"), Language::German1996);
        assert_eq!(detect_latin_language("łß"), Language::Polish);
        assert_eq!(detect_latin_language("åæ"), Language::Swedish);
        assert_eq!(detect_latin_language("æå"), Language::NorwegianBokmal);
    }

    #[test]
    fn detect_latin_language_long_input_terminates() {
        let long = "a".repeat(500_000);
        assert_eq!(detect_latin_language(&long), Language::EnglishUS);
        // Marker at the very end — no early return until the last char.
        let mut trailing = long;
        trailing.push('ß');
        assert_eq!(detect_latin_language(&trailing), Language::German1996);
    }

    // ---------------------------------------------------------------------
    // script_to_language
    // ---------------------------------------------------------------------

    #[test]
    fn script_to_language_is_total_over_every_script() {
        // No script/text combination may panic, and the result must be deterministic.
        let texts = [
            "",
            " ",
            "hello",
            "\u{1F600}",
            "\u{0000}\u{FFFF}\u{10FFFF}",
            "ß ç õ ã щ ळ \u{09F0} \u{2C80}",
        ];
        for script in ALL_SCRIPTS {
            for text in texts {
                let first = script_to_language(script, text);
                let second = script_to_language(script, text);
                assert_eq!(first, second, "{script:?} + {text:?} is not deterministic");
            }
        }
    }

    #[test]
    fn script_to_language_direct_mappings_ignore_the_text() {
        // These 19 scripts map to a fixed language; the text argument must not matter,
        // not even for text stuffed with every other script's marker chars.
        let cases: [(Script, Language); 19] = [
            (Script::Ethiopic, Language::Ethiopic),
            (Script::Georgian, Language::Georgian),
            (Script::Gujarati, Language::Gujarati),
            (Script::Gurmukhi, Language::Panjabi),
            (Script::Kannada, Language::Kannada),
            (Script::Malayalam, Language::Malayalam),
            (Script::Mandarin, Language::Chinese),
            (Script::Oriya, Language::Oriya),
            (Script::Tamil, Language::Tamil),
            (Script::Telugu, Language::Telugu),
            (Script::Thai, Language::Thai),
            (Script::Myanmar, Language::Thai),
            (Script::Khmer, Language::Thai),
            (Script::Sinhala, Language::Hindi),
            (Script::Arabic, Language::Chinese),
            (Script::Hebrew, Language::Chinese),
            (Script::Hangul, Language::Chinese),
            (Script::Hiragana, Language::Chinese),
            (Script::Katakana, Language::Chinese),
        ];
        let long = "x".repeat(10_000);
        for (script, expected) in cases {
            for text in ["", "ß щ ळ \u{09F0} \u{2C80} \u{1F600}", long.as_str()] {
                assert_eq!(
                    script_to_language(script, text),
                    expected,
                    "{script:?} must map to {expected:?} regardless of the text"
                );
            }
        }
    }

    #[test]
    fn script_to_language_delegates_the_five_text_sensitive_scripts() {
        let probes = ["", "hello", "ß", "щ", "\u{0933}", "\u{2C80}", "\u{09F0}"];
        for text in probes {
            assert_eq!(
                script_to_language(Script::Bengali, text),
                detect_bengali_language(text),
                "Bengali delegation broke for {text:?}"
            );
            assert_eq!(
                script_to_language(Script::Cyrillic, text),
                detect_cyrillic_language(text),
                "Cyrillic delegation broke for {text:?}"
            );
            assert_eq!(
                script_to_language(Script::Devanagari, text),
                detect_devanagari_language(text),
                "Devanagari delegation broke for {text:?}"
            );
            assert_eq!(
                script_to_language(Script::Greek, text),
                detect_greek_language(text),
                "Greek delegation broke for {text:?}"
            );
            assert_eq!(
                script_to_language(Script::Latin, text),
                detect_latin_language(text),
                "Latin delegation broke for {text:?}"
            );
        }
    }

    #[test]
    fn detect_script_then_script_to_language_end_to_end() {
        let cases: [(&str, Script, Language); 6] = [
            ("straße", Script::Latin, Language::German1996),
            ("Привет", Script::Cyrillic, Language::Russian),
            ("Здравейте, щастие", Script::Cyrillic, Language::Bulgarian),
            ("你好世界", Script::Mandarin, Language::Chinese),
            ("สวัสดี", Script::Thai, Language::Thai),
            ("ಕನ್ನಡ", Script::Kannada, Language::Kannada),
        ];
        for (text, script, language) in cases {
            let detected = detect_script(text).unwrap_or_else(|| panic!("no script for {text:?}"));
            assert_eq!(detected, script, "script for {text:?}");
            assert_eq!(script_to_language(detected, text), language, "language for {text:?}");
        }
    }

    #[test]
    fn script_to_language_survives_pseudo_random_text() {
        for seed in [7u64, 0x1234_5678_9ABC_DEF0] {
            let text = lcg_chars(2_000, seed);
            for script in ALL_SCRIPTS {
                let lang = script_to_language(script, &text);
                assert_eq!(lang, script_to_language(script, &text));
            }
        }
    }
}
