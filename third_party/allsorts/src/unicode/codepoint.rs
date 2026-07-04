use crate::unicode::bool_prop_emoji_presentation;

pub fn is_cjk_letter(ch: char) -> bool {
    match ch as u32 {
        // Hiragana, Katakana
        0x3040..=0x30FF => true,
        // Katakana Phonetic Extensions
        0x31F0..=0x31FF => true,
        // CJK Unified Ideographs Extension A
        0x3400..=0x4DFF => true,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF => true,
        // Hangul Syllables
        0xAC00..=0xD7A3 => true,
        // CJK Compatibility Ideographs
        0xF900..=0xFAFF => true,
        // CJK Unified Ideographs Extension B
        0x20000..=0x2A6DF => true,
        // CJK Compatibility Ideographs Supplement
        0x2F800..=0x2FA1F => true,
        _ => false,
    }
}

pub fn is_upright_char(ch: char) -> bool {
    let u = ch as u32;
    match u {
        _ if is_cjk_letter(ch) => true,
        // CJK Symbols and Punctuation
        0x3000..=0x303F => {
            // but not brackets or wave dashes
            match u {
                0x3008..=0x3011 => false,
                0x3014..=0x301C => false,
                0x3030 => false,
                _ => true,
            }
        }
        // Vertical Forms
        0xFE10..=0xFE1F => true,
        // CJK Compatibility Forms
        0xFE30..=0xFE4F => true,
        // Small Form Variants
        0xFE50..=0xFE6F => {
            match u {
                // but not brackets
                0xFE59..=0xFE5E => false,
                _ => true,
            }
        }
        // Halfwidth and Fullwidth Forms
        0xFF00..=0xFFEF => {
            // but not brackets or wave dashes
            match u {
                0xFF08 | 0xFF09 | 0xFF3B | 0xFF3D | 0xFF5B | 0xFF5D | 0xFF5E | 0xFF5F | 0xFF60
                | 0xFF62 | 0xFF63 => false,
                _ => true,
            }
        }
        // Emoji
        _ if bool_prop_emoji_presentation(ch) => true,
        _ => false,
    }
}
