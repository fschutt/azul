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

fn is_cyrillic(ch: char) -> bool {
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
fn is_latin(ch: char) -> bool {
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
fn is_arabic(ch: char) -> bool {
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
fn is_devanagari(ch: char) -> bool {
    matches!(ch, '\u{0900}'..='\u{097F}' | '\u{A8E0}'..='\u{A8FF}' | '\u{1CD0}'..='\u{1CFF}')
}

// Based on https://www.key-shortcut.com/en/writing-systems/ethiopian-script/
fn is_ethiopic(ch: char) -> bool {
    matches!(ch, '\u{1200}'..='\u{139F}' | '\u{2D80}'..='\u{2DDF}' | '\u{AB00}'..='\u{AB2F}')
}

// Based on https://en.wikipedia.org/wiki/Hebrew_(Unicode_block)
fn is_hebrew(ch: char) -> bool {
    matches!(ch, '\u{0590}'..='\u{05FF}')
}

fn is_georgian(ch: char) -> bool {
    matches!(ch, '\u{10A0}'..='\u{10FF}')
}

fn is_mandarin(ch: char) -> bool {
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

fn is_bengali(ch: char) -> bool {
    matches!(ch, '\u{0980}'..='\u{09FF}')
}

fn is_hiragana(ch: char) -> bool {
    matches!(ch, '\u{3040}'..='\u{309F}')
}

fn is_katakana(ch: char) -> bool {
    matches!(ch, '\u{30A0}'..='\u{30FF}')
}

// Hangul is Korean Alphabet. Unicode ranges are taken from: https://en.wikipedia.org/wiki/Hangul
fn is_hangul(ch: char) -> bool {
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
fn is_greek(ch: char) -> bool {
    matches!(ch, '\u{0370}'..='\u{03FF}')
}

// Based on: https://en.wikipedia.org/wiki/Kannada_(Unicode_block)
fn is_kannada(ch: char) -> bool {
    matches!(ch, '\u{0C80}'..='\u{0CFF}')
}

// Based on: https://en.wikipedia.org/wiki/Tamil_(Unicode_block)
fn is_tamil(ch: char) -> bool {
    matches!(ch, '\u{0B80}'..='\u{0BFF}')
}

// Based on: https://en.wikipedia.org/wiki/Thai_(Unicode_block)
fn is_thai(ch: char) -> bool {
    matches!(ch, '\u{0E00}'..='\u{0E7F}')
}

// Based on: https://en.wikipedia.org/wiki/Gujarati_(Unicode_block)
fn is_gujarati(ch: char) -> bool {
    matches!(ch, '\u{0A80}'..='\u{0AFF}')
}

// Gurmukhi is the script for Punjabi language.
// Based on: https://en.wikipedia.org/wiki/Gurmukhi_(Unicode_block)
fn is_gurmukhi(ch: char) -> bool {
    matches!(ch, '\u{0A00}'..='\u{0A7F}')
}

fn is_telugu(ch: char) -> bool {
    matches!(ch, '\u{0C00}'..='\u{0C7F}')
}

// Based on: https://en.wikipedia.org/wiki/Malayalam_(Unicode_block)
fn is_malayalam(ch: char) -> bool {
    matches!(ch, '\u{0D00}'..='\u{0D7F}')
}

// Based on: https://en.wikipedia.org/wiki/Malayalam_(Unicode_block)
fn is_oriya(ch: char) -> bool {
    matches!(ch, '\u{0B00}'..='\u{0B7F}')
}

// Based on: https://en.wikipedia.org/wiki/Myanmar_(Unicode_block)
fn is_myanmar(ch: char) -> bool {
    matches!(ch, '\u{1000}'..='\u{109F}')
}

// Based on: https://en.wikipedia.org/wiki/Sinhala_(Unicode_block)
fn is_sinhala(ch: char) -> bool {
    matches!(ch, '\u{0D80}'..='\u{0DFF}')
}

// Based on: https://en.wikipedia.org/wiki/Khmer_alphabet
fn is_khmer(ch: char) -> bool {
    matches!(ch, '\u{1780}'..='\u{17FF}' | '\u{19E0}'..='\u{19FF}')
}
