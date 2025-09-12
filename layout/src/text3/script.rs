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

use hyphenation::Language;
use rust_fontconfig::UnicodeRange;

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

impl Script {
    /// Maps a Script to a vector of its representative Unicode character ranges.
    ///
    /// The ranges are extracted from the `is_*` functions in the provided source code.
    pub fn get_unicode_ranges(&self) -> Vec<UnicodeRange> {
        match self {
            Script::Arabic => vec![
                UnicodeRange {
                    start: 0x0600,
                    end: 0x06FF,
                },
                UnicodeRange {
                    start: 0x0750,
                    end: 0x07FF,
                },
                UnicodeRange {
                    start: 0x08A0,
                    end: 0x08FF,
                },
                UnicodeRange {
                    start: 0xFB50,
                    end: 0xFDFF,
                },
                UnicodeRange {
                    start: 0xFE70,
                    end: 0xFEFF,
                },
                UnicodeRange {
                    start: 0x10E60,
                    end: 0x10E7F,
                },
                UnicodeRange {
                    start: 0x1EE00,
                    end: 0x1EEFF,
                },
            ],
            Script::Bengali => vec![UnicodeRange {
                start: 0x0980,
                end: 0x09FF,
            }],
            Script::Cyrillic => vec![
                UnicodeRange {
                    start: 0x0400,
                    end: 0x0484,
                },
                UnicodeRange {
                    start: 0x0487,
                    end: 0x052F,
                },
                UnicodeRange {
                    start: 0x2DE0,
                    end: 0x2DFF,
                },
                UnicodeRange {
                    start: 0xA640,
                    end: 0xA69D,
                },
                UnicodeRange {
                    start: 0x1D2B,
                    end: 0x1D2B,
                },
                UnicodeRange {
                    start: 0x1D78,
                    end: 0x1D78,
                },
                UnicodeRange {
                    start: 0xA69F,
                    end: 0xA69F,
                },
            ],
            Script::Devanagari => vec![
                UnicodeRange {
                    start: 0x0900,
                    end: 0x097F,
                },
                UnicodeRange {
                    start: 0xA8E0,
                    end: 0xA8FF,
                },
                UnicodeRange {
                    start: 0x1CD0,
                    end: 0x1CFF,
                },
            ],
            Script::Ethiopic => vec![
                UnicodeRange {
                    start: 0x1200,
                    end: 0x139F,
                },
                UnicodeRange {
                    start: 0x2D80,
                    end: 0x2DDF,
                },
                UnicodeRange {
                    start: 0xAB00,
                    end: 0xAB2F,
                },
            ],
            Script::Georgian => vec![UnicodeRange {
                start: 0x10A0,
                end: 0x10FF,
            }],
            Script::Greek => vec![UnicodeRange {
                start: 0x0370,
                end: 0x03FF,
            }],
            Script::Gujarati => vec![UnicodeRange {
                start: 0x0A80,
                end: 0x0AFF,
            }],
            Script::Gurmukhi => vec![UnicodeRange {
                start: 0x0A00,
                end: 0x0A7F,
            }],
            Script::Hangul => vec![
                UnicodeRange {
                    start: 0xAC00,
                    end: 0xD7AF,
                },
                UnicodeRange {
                    start: 0x1100,
                    end: 0x11FF,
                },
                UnicodeRange {
                    start: 0x3130,
                    end: 0x318F,
                },
                UnicodeRange {
                    start: 0x3200,
                    end: 0x32FF,
                },
                UnicodeRange {
                    start: 0xA960,
                    end: 0xA97F,
                },
                UnicodeRange {
                    start: 0xD7B0,
                    end: 0xD7FF,
                },
                UnicodeRange {
                    start: 0xFF00,
                    end: 0xFFEF,
                },
            ],
            Script::Hebrew => vec![UnicodeRange {
                start: 0x0590,
                end: 0x05FF,
            }],
            Script::Hiragana => vec![UnicodeRange {
                start: 0x3040,
                end: 0x309F,
            }],
            Script::Kannada => vec![UnicodeRange {
                start: 0x0C80,
                end: 0x0CFF,
            }],
            Script::Katakana => vec![UnicodeRange {
                start: 0x30A0,
                end: 0x30FF,
            }],
            Script::Khmer => vec![
                UnicodeRange {
                    start: 0x1780,
                    end: 0x17FF,
                },
                UnicodeRange {
                    start: 0x19E0,
                    end: 0x19FF,
                },
            ],
            Script::Latin => vec![
                UnicodeRange {
                    start: 0x0041,
                    end: 0x005A,
                }, // A-Z
                UnicodeRange {
                    start: 0x0061,
                    end: 0x007A,
                }, // a-z
                UnicodeRange {
                    start: 0x0080,
                    end: 0x00FF,
                },
                UnicodeRange {
                    start: 0x0100,
                    end: 0x017F,
                },
                UnicodeRange {
                    start: 0x0180,
                    end: 0x024F,
                },
                UnicodeRange {
                    start: 0x0250,
                    end: 0x02AF,
                },
                UnicodeRange {
                    start: 0x1D00,
                    end: 0x1D7F,
                },
                UnicodeRange {
                    start: 0x1D80,
                    end: 0x1DBF,
                },
                UnicodeRange {
                    start: 0x1E00,
                    end: 0x1EFF,
                },
                UnicodeRange {
                    start: 0x2100,
                    end: 0x214F,
                },
                UnicodeRange {
                    start: 0x2C60,
                    end: 0x2C7F,
                },
                UnicodeRange {
                    start: 0xA720,
                    end: 0xA7FF,
                },
                UnicodeRange {
                    start: 0xAB30,
                    end: 0xAB6F,
                },
            ],
            Script::Malayalam => vec![UnicodeRange {
                start: 0x0D00,
                end: 0x0D7F,
            }],
            Script::Mandarin => vec![
                UnicodeRange {
                    start: 0x2E80,
                    end: 0x2E99,
                },
                UnicodeRange {
                    start: 0x2E9B,
                    end: 0x2EF3,
                },
                UnicodeRange {
                    start: 0x2F00,
                    end: 0x2FD5,
                },
                UnicodeRange {
                    start: 0x3005,
                    end: 0x3005,
                },
                UnicodeRange {
                    start: 0x3007,
                    end: 0x3007,
                },
                UnicodeRange {
                    start: 0x3021,
                    end: 0x3029,
                },
                UnicodeRange {
                    start: 0x3038,
                    end: 0x303B,
                },
                UnicodeRange {
                    start: 0x3400,
                    end: 0x4DB5,
                },
                UnicodeRange {
                    start: 0x4E00,
                    end: 0x9FCC,
                },
                UnicodeRange {
                    start: 0xF900,
                    end: 0xFA6D,
                },
                UnicodeRange {
                    start: 0xFA70,
                    end: 0xFAD9,
                },
            ],
            Script::Myanmar => vec![UnicodeRange {
                start: 0x1000,
                end: 0x109F,
            }],
            Script::Oriya => vec![UnicodeRange {
                start: 0x0B00,
                end: 0x0B7F,
            }],
            Script::Sinhala => vec![UnicodeRange {
                start: 0x0D80,
                end: 0x0DFF,
            }],
            Script::Tamil => vec![UnicodeRange {
                start: 0x0B80,
                end: 0x0BFF,
            }],
            Script::Telugu => vec![UnicodeRange {
                start: 0x0C00,
                end: 0x0C7F,
            }],
            Script::Thai => vec![UnicodeRange {
                start: 0x0E00,
                end: 0x0E7F,
            }],
        }
    }
}

// Is it space, punctuation or digit?
// Stop character is a character that does not give any value for script
// or language detection.
#[inline]
pub fn is_stop_char(ch: char) -> bool {
    matches!(ch, '\u{0000}'..='\u{0040}' | '\u{005B}'..='\u{0060}' | '\u{007B}'..='\u{007E}')
}

type ScriptCounter = (Script, fn(char) -> bool, usize);

/// Detect only a script by a given text
pub fn detect_script(text: &str) -> Option<Script> {
    let mut script_counters: [ScriptCounter; 24] = [
        (Script::Latin, is_latin, 0),
        (Script::Cyrillic, is_cyrillic, 0),
        (Script::Arabic, is_arabic, 0),
        (Script::Mandarin, is_mandarin, 0),
        (Script::Devanagari, is_devanagari, 0),
        (Script::Hebrew, is_hebrew, 0),
        (Script::Ethiopic, is_ethiopic, 0),
        (Script::Georgian, is_georgian, 0),
        (Script::Bengali, is_bengali, 0),
        (Script::Hangul, is_hangul, 0),
        (Script::Hiragana, is_hiragana, 0),
        (Script::Katakana, is_katakana, 0),
        (Script::Greek, is_greek, 0),
        (Script::Kannada, is_kannada, 0),
        (Script::Tamil, is_tamil, 0),
        (Script::Thai, is_thai, 0),
        (Script::Gujarati, is_gujarati, 0),
        (Script::Gurmukhi, is_gurmukhi, 0),
        (Script::Telugu, is_telugu, 0),
        (Script::Malayalam, is_malayalam, 0),
        (Script::Oriya, is_oriya, 0),
        (Script::Myanmar, is_myanmar, 0),
        (Script::Sinhala, is_sinhala, 0),
        (Script::Khmer, is_khmer, 0),
    ];

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
        .cloned()
        .max_by_key(|&(_, _, count)| count)
        .unwrap();
    if count != 0 {
        Some(script)
    } else {
        None
    }
}

pub fn detect_char_script(ch: char) -> Option<Script> {
    let script_counters: [ScriptCounter; 24] = [
        (Script::Latin, is_latin, 0),
        (Script::Cyrillic, is_cyrillic, 0),
        (Script::Arabic, is_arabic, 0),
        (Script::Mandarin, is_mandarin, 0),
        (Script::Devanagari, is_devanagari, 0),
        (Script::Hebrew, is_hebrew, 0),
        (Script::Ethiopic, is_ethiopic, 0),
        (Script::Georgian, is_georgian, 0),
        (Script::Bengali, is_bengali, 0),
        (Script::Hangul, is_hangul, 0),
        (Script::Hiragana, is_hiragana, 0),
        (Script::Katakana, is_katakana, 0),
        (Script::Greek, is_greek, 0),
        (Script::Kannada, is_kannada, 0),
        (Script::Tamil, is_tamil, 0),
        (Script::Thai, is_thai, 0),
        (Script::Gujarati, is_gujarati, 0),
        (Script::Gurmukhi, is_gurmukhi, 0),
        (Script::Telugu, is_telugu, 0),
        (Script::Malayalam, is_malayalam, 0),
        (Script::Oriya, is_oriya, 0),
        (Script::Myanmar, is_myanmar, 0),
        (Script::Sinhala, is_sinhala, 0),
        (Script::Khmer, is_khmer, 0),
    ];

    for i in 0..script_counters.len() {
        let (script, check_fn, _) = script_counters[i];
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
    let mut has_polytonic = false;

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

fn detect_latin_language(text: &str) -> Language {
    // Flags for languages checked near the end of the original if-else chain.
    let mut has_french_c = false;
    let mut has_portugese_o = false;
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
            'õ' => has_portugese_o = true,
            'ã' => has_portuguese_a = true,

            // --- Flag-setting Cases ---
            'ç' => has_french_c = true, // Also in Portuguese
            'á' | 'é' | 'í' | 'ó' | 'ú' => return Language::Spanish,

            _ => (),
        }
    }

    // decide between portuguese, estonian and french

    if has_french_c && !has_portugese_o && !has_portuguese_a {
        return Language::French;
    }

    if has_portugese_o && !has_french_c && !has_portuguese_a {
        return Language::Estonian;
    }

    if has_portugese_o || has_portuguese_a || has_french_c {
        return Language::Portuguese;
    }

    Language::EnglishUS
}

pub fn script_to_language(script: Script, text: &str) -> Language {
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

pub fn is_cyrillic(ch: char) -> bool {
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
pub fn is_latin(ch: char) -> bool {
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
pub fn is_arabic(ch: char) -> bool {
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
pub fn is_devanagari(ch: char) -> bool {
    matches!(ch, '\u{0900}'..='\u{097F}' | '\u{A8E0}'..='\u{A8FF}' | '\u{1CD0}'..='\u{1CFF}')
}

// Based on https://www.key-shortcut.com/en/writing-systems/ethiopian-script/
pub fn is_ethiopic(ch: char) -> bool {
    matches!(ch, '\u{1200}'..='\u{139F}' | '\u{2D80}'..='\u{2DDF}' | '\u{AB00}'..='\u{AB2F}')
}

// Based on https://en.wikipedia.org/wiki/Hebrew_(Unicode_block)
pub fn is_hebrew(ch: char) -> bool {
    matches!(ch, '\u{0590}'..='\u{05FF}')
}

pub fn is_georgian(ch: char) -> bool {
    matches!(ch, '\u{10A0}'..='\u{10FF}')
}

pub fn is_mandarin(ch: char) -> bool {
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

pub fn is_bengali(ch: char) -> bool {
    matches!(ch, '\u{0980}'..='\u{09FF}')
}

pub fn is_hiragana(ch: char) -> bool {
    matches!(ch, '\u{3040}'..='\u{309F}')
}

pub fn is_katakana(ch: char) -> bool {
    matches!(ch, '\u{30A0}'..='\u{30FF}')
}

// Hangul is Korean Alphabet. Unicode ranges are taken from: https://en.wikipedia.org/wiki/Hangul
pub fn is_hangul(ch: char) -> bool {
    matches!(ch,
        '\u{AC00}'..='\u{D7AF}'
        | '\u{1100}'..='\u{11FF}'
        | '\u{3130}'..='\u{318F}'
        | '\u{3200}'..='\u{32FF}'
        | '\u{A960}'..='\u{A97F}'
        | '\u{D7B0}'..='\u{D7FF}'
        | '\u{FF00}'..='\u{FFEF}'
    )
}

// Taken from: https://en.wikipedia.org/wiki/Greek_and_Coptic
pub fn is_greek(ch: char) -> bool {
    matches!(ch, '\u{0370}'..='\u{03FF}')
}

// Based on: https://en.wikipedia.org/wiki/Kannada_(Unicode_block)
pub fn is_kannada(ch: char) -> bool {
    matches!(ch, '\u{0C80}'..='\u{0CFF}')
}

// Based on: https://en.wikipedia.org/wiki/Tamil_(Unicode_block)
pub fn is_tamil(ch: char) -> bool {
    matches!(ch, '\u{0B80}'..='\u{0BFF}')
}

// Based on: https://en.wikipedia.org/wiki/Thai_(Unicode_block)
pub fn is_thai(ch: char) -> bool {
    matches!(ch, '\u{0E00}'..='\u{0E7F}')
}

// Based on: https://en.wikipedia.org/wiki/Gujarati_(Unicode_block)
pub fn is_gujarati(ch: char) -> bool {
    matches!(ch, '\u{0A80}'..='\u{0AFF}')
}

// Gurmukhi is the script for Punjabi language.
// Based on: https://en.wikipedia.org/wiki/Gurmukhi_(Unicode_block)
pub fn is_gurmukhi(ch: char) -> bool {
    matches!(ch, '\u{0A00}'..='\u{0A7F}')
}

pub fn is_telugu(ch: char) -> bool {
    matches!(ch, '\u{0C00}'..='\u{0C7F}')
}

// Based on: https://en.wikipedia.org/wiki/Malayalam_(Unicode_block)
pub fn is_malayalam(ch: char) -> bool {
    matches!(ch, '\u{0D00}'..='\u{0D7F}')
}

// Based on: https://en.wikipedia.org/wiki/Malayalam_(Unicode_block)
pub fn is_oriya(ch: char) -> bool {
    matches!(ch, '\u{0B00}'..='\u{0B7F}')
}

// Based on: https://en.wikipedia.org/wiki/Myanmar_(Unicode_block)
pub fn is_myanmar(ch: char) -> bool {
    matches!(ch, '\u{1000}'..='\u{109F}')
}

// Based on: https://en.wikipedia.org/wiki/Sinhala_(Unicode_block)
pub fn is_sinhala(ch: char) -> bool {
    matches!(ch, '\u{0D80}'..='\u{0DFF}')
}

// Based on: https://en.wikipedia.org/wiki/Khmer_alphabet
pub fn is_khmer(ch: char) -> bool {
    matches!(ch, '\u{1780}'..='\u{17FF}' | '\u{19E0}'..='\u{19FF}')
}

/// Estimate the language and the script from the text (uses trigrams)
#[allow(dead_code)]
pub fn estimate_script_and_language(text: &str) -> (u32, Option<u32>) {
    use allsorts::tag;

    use crate::text2::script::Script; // whatlang::Script

    // https://docs.microsoft.com/en-us/typography/opentype/spec/scripttags

    const TAG_ADLM: u32 = tag!(b"adlm"); // Adlam
    const TAG_AHOM: u32 = tag!(b"ahom"); // Ahom
    const TAG_HLUW: u32 = tag!(b"hluw"); // Anatolian Hieroglyphs
    const TAG_ARAB: u32 = tag!(b"arab"); // Arabic
    const TAG_ARMN: u32 = tag!(b"armn"); // Armenian
    const TAG_AVST: u32 = tag!(b"avst"); // Avestan
    const TAG_BALI: u32 = tag!(b"bali"); // Balinese
    const TAG_BAMU: u32 = tag!(b"bamu"); // Bamum
    const TAG_BASS: u32 = tag!(b"bass"); // Bassa Vah
    const TAG_BATK: u32 = tag!(b"batk"); // Batak
    const TAG_BENG: u32 = tag!(b"beng"); // Bengali
    const TAG_BNG2: u32 = tag!(b"bng2"); // Bengali v.2
    const TAG_BHKS: u32 = tag!(b"bhks"); // Bhaiksuki
    const TAG_BOPO: u32 = tag!(b"bopo"); // Bopomofo
    const TAG_BRAH: u32 = tag!(b"brah"); // Brahmi
    const TAG_BRAI: u32 = tag!(b"brai"); // Braille
    const TAG_BUGI: u32 = tag!(b"bugi"); // Buginese
    const TAG_BUHD: u32 = tag!(b"buhd"); // Buhid
    const TAG_BYZM: u32 = tag!(b"byzm"); // Byzantine Music
    const TAG_CANS: u32 = tag!(b"cans"); // Canadian Syllabics
    const TAG_CARI: u32 = tag!(b"cari"); // Carian
    const TAG_AGHB: u32 = tag!(b"aghb"); // Caucasian Albanian
    const TAG_CAKM: u32 = tag!(b"cakm"); // Chakma
    const TAG_CHAM: u32 = tag!(b"cham"); // Cham
    const TAG_CHER: u32 = tag!(b"cher"); // Cherokee
    const TAG_CHRS: u32 = tag!(b"chrs"); // Chorasmian
    const TAG_HANI: u32 = tag!(b"hani"); // CJK Ideographic
    const TAG_COPT: u32 = tag!(b"copt"); // Coptic
    const TAG_CPRT: u32 = tag!(b"cprt"); // Cypriot Syllabary
    const TAG_CYRL: u32 = tag!(b"cyrl"); // Cyrillic
    const TAG_DFLT: u32 = tag!(b"DFLT"); // Default
    const TAG_DSRT: u32 = tag!(b"dsrt"); // Deseret
    const TAG_DEVA: u32 = tag!(b"deva"); // Devanagari
    const TAG_DEV2: u32 = tag!(b"dev2"); // Devanagari v.2
    const TAG_DIAK: u32 = tag!(b"diak"); // Dives Akuru
    const TAG_DOGR: u32 = tag!(b"dogr"); // Dogra
    const TAG_DUPL: u32 = tag!(b"dupl"); // Duployan
    const TAG_EGYP: u32 = tag!(b"egyp"); // Egyptian Hieroglyphs
    const TAG_ELBA: u32 = tag!(b"elba"); // Elbasan
    const TAG_ELYM: u32 = tag!(b"elym"); // Elymaic
    const TAG_ETHI: u32 = tag!(b"ethi"); // Ethiopic
    const TAG_GEOR: u32 = tag!(b"geor"); // Georgian
    const TAG_GLAG: u32 = tag!(b"glag"); // Glagolitic
    const TAG_GOTH: u32 = tag!(b"goth"); // Gothic
    const TAG_GRAN: u32 = tag!(b"gran"); // Grantha
    const TAG_GREK: u32 = tag!(b"grek"); // Greek
    const TAG_GUJR: u32 = tag!(b"gujr"); // Gujarati
    const TAG_GJR2: u32 = tag!(b"gjr2"); // Gujarati v.2
    const TAG_GONG: u32 = tag!(b"gong"); // Gunjala Gondi
    const TAG_GURU: u32 = tag!(b"guru"); // Gurmukhi
    const TAG_GUR2: u32 = tag!(b"gur2"); // Gurmukhi v.2
    const TAG_HANG: u32 = tag!(b"hang"); // Hangul
    const TAG_JAMO: u32 = tag!(b"jamo"); // Hangul Jamo
    const TAG_ROHG: u32 = tag!(b"rohg"); // Hanifi Rohingya
    const TAG_HANO: u32 = tag!(b"hano"); // Hanunoo
    const TAG_HATR: u32 = tag!(b"hatr"); // Hatran
    const TAG_HEBR: u32 = tag!(b"hebr"); // Hebrew
    const TAG_HIRG: u32 = tag!(b"kana"); // Hiragana
    const TAG_ARMI: u32 = tag!(b"armi"); // Imperial Aramaic
    const TAG_PHLI: u32 = tag!(b"phli"); // Inscriptional Pahlavi
    const TAG_PRTI: u32 = tag!(b"prti"); // Inscriptional Parthian
    const TAG_JAVA: u32 = tag!(b"java"); // Javanese
    const TAG_KTHI: u32 = tag!(b"kthi"); // Kaithi
    const TAG_KNDA: u32 = tag!(b"knda"); // Kannada
    const TAG_KND2: u32 = tag!(b"knd2"); // Kannada v.2
    const TAG_KANA: u32 = tag!(b"kana"); // Katakana
    const TAG_KALI: u32 = tag!(b"kali"); // Kayah Li
    const TAG_KHAR: u32 = tag!(b"khar"); // Kharosthi
    const TAG_KITS: u32 = tag!(b"kits"); // Khitan Small Script
    const TAG_KHMR: u32 = tag!(b"khmr"); // Khmer
    const TAG_KHOJ: u32 = tag!(b"khoj"); // Khojki
    const TAG_SIND: u32 = tag!(b"sind"); // Khudawadi
    const TAG_LAO: u32 = tag!(b"lao "); // Lao
    const TAG_LATN: u32 = tag!(b"latn"); // Latin
    const TAG_LEPC: u32 = tag!(b"lepc"); // Lepcha
    const TAG_LIMB: u32 = tag!(b"limb"); // Limbu
    const TAG_LINA: u32 = tag!(b"lina"); // Linear A
    const TAG_LINB: u32 = tag!(b"linb"); // Linear B
    const TAG_LISU: u32 = tag!(b"lisu"); // Lisu (Fraser)
    const TAG_LYCI: u32 = tag!(b"lyci"); // Lycian
    const TAG_LYDI: u32 = tag!(b"lydi"); // Lydian
    const TAG_MAHJ: u32 = tag!(b"mahj"); // Mahajani
    const TAG_MAKA: u32 = tag!(b"maka"); // Makasar
    const TAG_MLYM: u32 = tag!(b"mlym"); // Malayalam
    const TAG_MLM2: u32 = tag!(b"mlm2"); // Malayalam v.2
    const TAG_MAND: u32 = tag!(b"mand"); // Mandaic, Mandaean
    const TAG_MANI: u32 = tag!(b"mani"); // Manichaean
    const TAG_MARC: u32 = tag!(b"marc"); // Marchen
    const TAG_GONM: u32 = tag!(b"gonm"); // Masaram Gondi
    const TAG_MATH: u32 = tag!(b"math"); // Mathematical Alphanumeric Symbols
    const TAG_MEDF: u32 = tag!(b"medf"); // Medefaidrin (Oberi Okaime, Oberi kaim)
    const TAG_MTEI: u32 = tag!(b"mtei"); // Meitei Mayek (Meithei, Meetei)
    const TAG_MEND: u32 = tag!(b"mend"); // Mende Kikakui
    const TAG_MERC: u32 = tag!(b"merc"); // Meroitic Cursive
    const TAG_MERO: u32 = tag!(b"mero"); // Meroitic Hieroglyphs
    const TAG_PLRD: u32 = tag!(b"plrd"); // Miao
    const TAG_MODI: u32 = tag!(b"modi"); // Modi
    const TAG_MONG: u32 = tag!(b"mong"); // Mongolian
    const TAG_MROO: u32 = tag!(b"mroo"); // Mro
    const TAG_MULT: u32 = tag!(b"mult"); // Multani
    const TAG_MUSC: u32 = tag!(b"musc"); // Musical Symbols
    const TAG_MYMR: u32 = tag!(b"mymr"); // Myanmar
    const TAG_MYM2: u32 = tag!(b"mym2"); // Myanmar v.2
    const TAG_NBAT: u32 = tag!(b"nbat"); // Nabataean
    const TAG_NAND: u32 = tag!(b"nand"); // Nandinagari
    const TAG_NEWA: u32 = tag!(b"newa"); // Newa
    const TAG_TALU: u32 = tag!(b"talu"); // New Tai Lue
    const TAG_NKO: u32 = tag!(b"nko "); // N'Ko
    const TAG_NSHU: u32 = tag!(b"nshu"); // Nüshu
    const TAG_HMNP: u32 = tag!(b"hmnp"); // Nyiakeng Puachue Hmong
    const TAG_ORYA: u32 = tag!(b"orya"); // Odia (formerly Oriya)
    const TAG_ORY2: u32 = tag!(b"ory2"); // Odia v.2 (formerly Oriya v.2)
    const TAG_OGAM: u32 = tag!(b"ogam"); // Ogham
    const TAG_OLCK: u32 = tag!(b"olck"); // Ol Chiki
    const TAG_ITAL: u32 = tag!(b"ital"); // Old Italic
    const TAG_HUNG: u32 = tag!(b"hung"); // Old Hungarian
    const TAG_NARB: u32 = tag!(b"narb"); // Old North Arabian
    const TAG_PERM: u32 = tag!(b"perm"); // Old Permic
    const TAG_XPEO: u32 = tag!(b"xpeo"); // Old Persian Cuneiform
    const TAG_SOGO: u32 = tag!(b"sogo"); // Old Sogdian
    const TAG_SARB: u32 = tag!(b"sarb"); // Old South Arabian
    const TAG_ORKH: u32 = tag!(b"orkh"); // Old Turkic, Orkhon Runic
    const TAG_OSGE: u32 = tag!(b"osge"); // Osage
    const TAG_OSMA: u32 = tag!(b"osma"); // Osmanya
    const TAG_HMNG: u32 = tag!(b"hmng"); // Pahawh Hmong
    const TAG_PALM: u32 = tag!(b"palm"); // Palmyrene
    const TAG_PAUC: u32 = tag!(b"pauc"); // Pau Cin Hau
    const TAG_PHAG: u32 = tag!(b"phag"); // Phags-pa
    const TAG_PHNX: u32 = tag!(b"phnx"); // Phoenician
    const TAG_PHLP: u32 = tag!(b"phlp"); // Psalter Pahlavi
    const TAG_RJNG: u32 = tag!(b"rjng"); // Rejang
    const TAG_RUNR: u32 = tag!(b"runr"); // Runic
    const TAG_SAMR: u32 = tag!(b"samr"); // Samaritan
    const TAG_SAUR: u32 = tag!(b"saur"); // Saurashtra
    const TAG_SHRD: u32 = tag!(b"shrd"); // Sharada
    const TAG_SHAW: u32 = tag!(b"shaw"); // Shavian
    const TAG_SIDD: u32 = tag!(b"sidd"); // Siddham
    const TAG_SGNW: u32 = tag!(b"sgnw"); // Sign Writing
    const TAG_SINH: u32 = tag!(b"sinh"); // Sinhala
    const TAG_SOGD: u32 = tag!(b"sogd"); // Sogdian
    const TAG_SORA: u32 = tag!(b"sora"); // Sora Sompeng
    const TAG_SOYO: u32 = tag!(b"soyo"); // Soyombo
    const TAG_XSUX: u32 = tag!(b"xsux"); // Sumero-Akkadian Cuneiform
    const TAG_SUND: u32 = tag!(b"sund"); // Sundanese
    const TAG_SYLO: u32 = tag!(b"sylo"); // Syloti Nagri
    const TAG_SYRC: u32 = tag!(b"syrc"); // Syriac
    const TAG_TGLG: u32 = tag!(b"tglg"); // Tagalog
    const TAG_TAGB: u32 = tag!(b"tagb"); // Tagbanwa
    const TAG_TALE: u32 = tag!(b"tale"); // Tai Le
    const TAG_LANA: u32 = tag!(b"lana"); // Tai Tham (Lanna)
    const TAG_TAVT: u32 = tag!(b"tavt"); // Tai Viet
    const TAG_TAKR: u32 = tag!(b"takr"); // Takri
    const TAG_TAML: u32 = tag!(b"taml"); // Tamil
    const TAG_TML2: u32 = tag!(b"tml2"); // Tamil v.2
    const TAG_TANG: u32 = tag!(b"tang"); // Tangut
    const TAG_TELU: u32 = tag!(b"telu"); // Telugu
    const TAG_TEL2: u32 = tag!(b"tel2"); // Telugu v.2
    const TAG_THAA: u32 = tag!(b"thaa"); // Thaana
    const TAG_THAI: u32 = tag!(b"thai"); // Thai
    const TAG_TIBT: u32 = tag!(b"tibt"); // Tibetan
    const TAG_TFNG: u32 = tag!(b"tfng"); // Tifinagh
    const TAG_TIRH: u32 = tag!(b"tirh"); // Tirhuta
    const TAG_UGAR: u32 = tag!(b"ugar"); // Ugaritic Cuneiform
    const TAG_VAI: u32 = tag!(b"vai "); // Vai
    const TAG_WCHO: u32 = tag!(b"wcho"); // Wancho
    const TAG_WARA: u32 = tag!(b"wara"); // Warang Citi
    const TAG_YEZI: u32 = tag!(b"yezi"); // Yezidi
    const TAG_ZANB: u32 = tag!(b"zanb"); // Zanabazar Square
                                         // missing: Yi

    // auto-detect script + language from text (todo: performance!)

    // let (lang, script) = whatlang::detect(text)
    //     .map(|info| (info.lang(), info.script()))
    //     .unwrap_or((Lang::Eng, Script::Latin));

    let lang = None; // detecting the language is only necessary for special font features

    // let lang = tag_mod::from_string(&lang.code().to_string().to_uppercase()).unwrap();

    let script = match crate::text2::script::detect_script(text).unwrap_or(Script::Latin) {
        Script::Arabic => TAG_ARAB,
        Script::Bengali => TAG_BENG,
        Script::Cyrillic => TAG_CYRL,
        Script::Devanagari => TAG_DEVA,
        Script::Ethiopic => TAG_ETHI,
        Script::Georgian => TAG_GEOR,
        Script::Greek => TAG_GREK,
        Script::Gujarati => TAG_GUJR,
        Script::Gurmukhi => TAG_GUR2,
        Script::Hangul => TAG_HANG,
        Script::Hebrew => TAG_HEBR,
        Script::Hiragana => TAG_HIRG, // NOTE: tag = 'kana', probably error
        Script::Kannada => TAG_KND2,
        Script::Katakana => TAG_KANA,
        Script::Khmer => TAG_KHMR,
        Script::Latin => TAG_LATN,
        Script::Malayalam => TAG_MLYM,
        Script::Mandarin => TAG_MAND,
        Script::Myanmar => TAG_MYM2,
        Script::Oriya => TAG_ORYA,
        Script::Sinhala => TAG_SINH,
        Script::Tamil => TAG_TAML,
        Script::Telugu => TAG_TELU,
        Script::Thai => TAG_THAI,
    };

    (script, lang)
}
