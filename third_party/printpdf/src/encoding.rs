//! Encodings of simple fonts: how the one-byte codes of a content stream become glyph names and
//! Unicode text.
//!
//! A simple font (Type1, TrueType) shows text with one byte per glyph. The font dictionary says
//! which glyph a byte stands for: `/Encoding` names a base encoding (`StandardEncoding`,
//! `MacRomanEncoding`, `WinAnsiEncoding`) and may list `/Differences` that replace single codes
//! with glyph names. Without `/Encoding` the font program's own built-in encoding applies.
//! A glyph name becomes Unicode through the Adobe Glyph List.

use std::collections::BTreeMap;

use crate::cmap::ToUnicodeCMap;

/// The base encodings of a simple font's `/Encoding` (ISO 32000-1, D.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaseEncoding {
    /// `StandardEncoding`, the built-in encoding of Adobe Type 1 fonts.
    Standard,
    /// `MacRomanEncoding`.
    MacRoman,
    /// `WinAnsiEncoding`.
    WinAnsi,
}

impl BaseEncoding {
    /// The base encoding called `name` (`MacRomanEncoding`, ...). `None` for any other name,
    /// among them `MacExpertEncoding`.
    pub(crate) fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"StandardEncoding" => Some(Self::Standard),
            b"MacRomanEncoding" => Some(Self::MacRoman),
            b"WinAnsiEncoding" => Some(Self::WinAnsi),
            _ => None,
        }
    }

    /// The character of `code`, if the encoding has one.
    pub(crate) fn char_for(self, code: u8) -> Option<char> {
        match self {
            Self::WinAnsi => (code >= 0x20 && code != 0x7F).then(|| crate::text::win_ansi_char(code)),
            Self::Standard => standard_char(code),
            Self::MacRoman => mac_roman_char(code),
        }
    }
}

/// `StandardEncoding` as Unicode.
fn standard_char(code: u8) -> Option<char> {
    let c = match code {
        0x27 => '\u{2019}', // quoteright
        0x60 => '\u{2018}', // quoteleft
        0x20..=0x7E => char::from(code),
        0xA1 => '\u{00A1}',
        0xA2 => '\u{00A2}',
        0xA3 => '\u{00A3}',
        0xA4 => '\u{2044}', // fraction
        0xA5 => '\u{00A5}',
        0xA6 => '\u{0192}',
        0xA7 => '\u{00A7}',
        0xA8 => '\u{00A4}',
        0xA9 => '\u{0027}', // quotesingle
        0xAA => '\u{201C}',
        0xAB => '\u{00AB}',
        0xAC => '\u{2039}',
        0xAD => '\u{203A}',
        0xAE => '\u{FB01}',
        0xAF => '\u{FB02}',
        0xB1 => '\u{2013}',
        0xB2 => '\u{2020}',
        0xB3 => '\u{2021}',
        0xB4 => '\u{00B7}',
        0xB6 => '\u{00B6}',
        0xB7 => '\u{2022}',
        0xB8 => '\u{201A}',
        0xB9 => '\u{201E}',
        0xBA => '\u{201D}',
        0xBB => '\u{00BB}',
        0xBC => '\u{2026}',
        0xBD => '\u{2030}',
        0xBF => '\u{00BF}',
        0xC1 => '\u{0060}', // grave
        0xC2 => '\u{00B4}',
        0xC3 => '\u{02C6}',
        0xC4 => '\u{02DC}',
        0xC5 => '\u{00AF}',
        0xC6 => '\u{02D8}',
        0xC7 => '\u{02D9}',
        0xC8 => '\u{00A8}',
        0xCA => '\u{02DA}',
        0xCB => '\u{00B8}',
        0xCD => '\u{02DD}',
        0xCE => '\u{02DB}',
        0xCF => '\u{02C7}',
        0xD0 => '\u{2014}',
        0xE1 => '\u{00C6}',
        0xE3 => '\u{00AA}',
        0xE8 => '\u{0141}',
        0xE9 => '\u{00D8}',
        0xEA => '\u{0152}',
        0xEB => '\u{00BA}',
        0xF1 => '\u{00E6}',
        0xF5 => '\u{0131}',
        0xF8 => '\u{0142}',
        0xF9 => '\u{00F8}',
        0xFA => '\u{0153}',
        0xFB => '\u{00DF}',
        _ => return None,
    };
    Some(c)
}

/// `MacRomanEncoding` as Unicode, from the `macintosh` table of `encoding_rs` (a dependency of
/// allsorts). Code `0xDB` is the euro sign there; the PDF encoding has the currency sign.
fn mac_roman_char(code: u8) -> Option<char> {
    match code {
        0x20..=0x7E => Some(char::from(code)),
        0x80..=0xFF => encoding_rs::MACINTOSH
            .decode_without_bom_handling(&[code])
            .0
            .chars()
            .next(),
        _ => None,
    }
}

/// Glyph names the Adobe Glyph List for new fonts does not list, with their characters.
const EXTRA_NAMES: &[(&str, char)] = &[
    ("nbspace", '\u{00A0}'),
    ("nonbreakingspace", '\u{00A0}'),
    ("sfthyphen", '\u{00AD}'),
    ("softhyphen", '\u{00AD}'),
    ("ff", '\u{FB00}'),
    ("ffi", '\u{FB03}'),
    ("ffl", '\u{FB04}'),
    ("fi", '\u{FB01}'),
    ("fl", '\u{FB02}'),
    ("dotlessj", '\u{0237}'),
    ("cdot", '\u{00B7}'),
    ("Delta", '\u{2206}'),
    ("Omega", '\u{2126}'),
    ("mu", '\u{00B5}'),
    ("macron", '\u{00AF}'),
    ("overscore", '\u{00AF}'),
    ("quotesingle", '\u{0027}'),
    ("grave", '\u{0060}'),
    ("hyphenminus", '\u{002D}'),
];

/// The text of the glyph `name`, by the Adobe Glyph List specification: a suffix after the first
/// `.` is dropped, `_` joins the names of a ligature's parts, `uniXXXX` and `uXXXX` name code
/// points. `None` for `.notdef` and for names that do not map to text (`g12`, `cid5`).
pub(crate) fn glyph_name_to_string(name: &str) -> Option<String> {
    let base = name.split('.').next().unwrap_or_default();
    if base.is_empty() {
        return None;
    }
    let mut out = String::new();
    for part in base.split('_') {
        out.push_str(&component_to_string(part)?);
    }
    (!out.is_empty()).then_some(out)
}

fn component_to_string(part: &str) -> Option<String> {
    if let Some(&(c, _)) = glyph_names::GLYPH_NAME_PAIRS.iter().find(|(_, n)| *n == part) {
        return Some(c.to_string());
    }
    if let Some(&(_, c)) = EXTRA_NAMES.iter().find(|(n, _)| *n == part) {
        return Some(c.to_string());
    }
    if let Some(hex) = part.strip_prefix("uni") {
        if hex.len() >= 4 && hex.len() % 4 == 0 && hex.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)) {
            let mut s = String::new();
            for group in hex.as_bytes().chunks(4) {
                let digits = std::str::from_utf8(group).ok()?;
                s.push(char::from_u32(u32::from_str_radix(digits, 16).ok()?)?);
            }
            return Some(s);
        }
    }
    if let Some(hex) = part.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)) {
            return Some(char::from_u32(u32::from_str_radix(hex, 16).ok()?)?.to_string());
        }
    }
    None
}

/// What a simple font dictionary's `/Encoding` says: the base encoding, if one is named, and the
/// `/Differences` as code to glyph name.
#[derive(Debug, Default, Clone)]
pub(crate) struct SimpleEncoding {
    /// `/BaseEncoding`, or the name `/Encoding` itself.
    pub(crate) base: Option<BaseEncoding>,
    /// `/Differences`: the glyph name each listed code shows instead.
    pub(crate) differences: BTreeMap<u8, String>,
}

impl SimpleEncoding {
    /// Reads `/Encoding` of `font_dict`. `resolve` follows references to the encoding dictionary.
    pub(crate) fn from_font_dict(
        font_dict: &lopdf::Dictionary,
        resolve: impl Fn(&lopdf::Object) -> Option<lopdf::Object>,
    ) -> Self {
        let mut out = Self::default();
        let Some(encoding) = font_dict.get(b"Encoding").ok().and_then(|o| resolve(o)) else {
            return out;
        };
        match &encoding {
            lopdf::Object::Name(n) => out.base = BaseEncoding::from_name(n),
            lopdf::Object::Dictionary(d) => {
                if let Ok(lopdf::Object::Name(n)) = d.get(b"BaseEncoding") {
                    out.base = BaseEncoding::from_name(n);
                }
                if let Some(lopdf::Object::Array(items)) = d.get(b"Differences").ok().and_then(|o| resolve(o)) {
                    let mut code: i64 = 0;
                    for item in &items {
                        match item {
                            lopdf::Object::Integer(i) => code = *i,
                            lopdf::Object::Real(r) => code = f64::from(*r) as i64,
                            lopdf::Object::Name(n) => {
                                if let Ok(c) = u8::try_from(code) {
                                    out.differences.insert(c, String::from_utf8_lossy(n).into_owned());
                                }
                                code = code.saturating_add(1);
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Whether the dictionary says anything about the encoding.
    pub(crate) fn is_empty(&self) -> bool {
        self.base.is_none() && self.differences.is_empty()
    }

    /// The text of every code, as a ToUnicode CMap. `builtin` gives the glyph name the font
    /// program itself assigns to a code, which applies where the dictionary names no base
    /// encoding (and, with a base encoding, nowhere). Codes without a known text read as WinAnsi.
    pub(crate) fn to_unicode(&self, builtin: &BTreeMap<u8, String>) -> ToUnicodeCMap {
        let mut mappings = BTreeMap::new();
        for code in 0u8..=255 {
            let text: Option<String> = if let Some(name) = self.differences.get(&code) {
                glyph_name_to_string(name)
            } else if let Some(base) = self.base {
                base.char_for(code).map(String::from)
            } else if let Some(name) = builtin.get(&code) {
                glyph_name_to_string(name)
            } else {
                BaseEncoding::WinAnsi.char_for(code).map(String::from)
            };
            if let Some(text) = text {
                mappings.insert(u32::from(code), text.chars().map(u32::from).collect());
            }
        }
        ToUnicodeCMap { mappings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_names_become_text() {
        assert_eq!(glyph_name_to_string("A").as_deref(), Some("A"));
        assert_eq!(glyph_name_to_string("eacute").as_deref(), Some("é"));
        assert_eq!(glyph_name_to_string("f_i").as_deref(), Some("fi"));
        assert_eq!(glyph_name_to_string("a.sc").as_deref(), Some("a"));
        assert_eq!(glyph_name_to_string("uni20AC").as_deref(), Some("€"));
        assert_eq!(glyph_name_to_string("u1F600").as_deref(), Some("😀"));
        assert_eq!(glyph_name_to_string("nonbreakingspace").as_deref(), Some("\u{A0}"));
        assert_eq!(glyph_name_to_string(".notdef"), None);
        assert_eq!(glyph_name_to_string("g12"), None);
    }

    #[test]
    fn base_encodings() {
        assert_eq!(BaseEncoding::MacRoman.char_for(0x8E), Some('é'));
        assert_eq!(BaseEncoding::MacRoman.char_for(0xD5), Some('’'));
        assert_eq!(BaseEncoding::Standard.char_for(0x27), Some('\u{2019}'));
        assert_eq!(BaseEncoding::Standard.char_for(0xAE), Some('\u{FB01}'));
        assert_eq!(BaseEncoding::WinAnsi.char_for(0x80), Some('€'));
    }
}
