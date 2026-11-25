//! Text items and the encoding of PDF strings.
//!
//! In a PDF, text is shown by the `Tj` and `TJ` operators. `Tj` takes one string and `TJ` takes
//! an array that mixes strings with numbers; each number shifts the pen by that many
//! thousandths of the font size (a positive number moves it left, which is how kerning is
//! written). A PDF string is only bytes, and the font decides what the bytes mean:
//!
//! - the 14 built-in fonts use `WinAnsiEncoding`, one byte per character ([`win_ansi_char`]);
//! - embedded fonts use `Identity-H`, where every two bytes are a glyph id (a CID), so the
//!   string holds no readable text. The text comes back through the font's `/ToUnicode` CMap
//!   (see [`crate::cmap`]).
//!
//! printpdf models what a `TJ` array holds as [`TextItem`]: a piece of text, a number, or a
//! run of glyph ids ([`Codepoint`]). [`Op::ShowText`](crate::Op::ShowText) carries a list of
//! them. When a document is saved, `Text` is encoded for the current font; when a PDF is read,
//! strings are decoded with the functions of this module.
//!
//! # Examples
//!
//! ```
//! use printpdf::{encode_pdf_string_literal, win_ansi_char};
//!
//! assert_eq!(encode_pdf_string_literal("a (b)"), r"(a \(b\))");
//! assert_eq!(win_ansi_char(0x80), '\u{20AC}'); // the euro sign
//! ```

use lopdf::{Object, StringFormat};
use serde_derive::{Deserialize, Serialize};

/// A glyph shown by a `TJ` array, with the kerning that follows it.
#[derive(Debug, Clone, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Codepoint {
    /// Glyph id in the font. For an embedded font this is the code written to the content
    /// stream (before subsetting renumbers it).
    pub gid: u16,
    /// Kerning that follows the glyph, in thousandths of the font size. Written as a `TJ`
    /// number, so a positive value moves the next glyph to the left.
    pub offset: f32,
    /// The text this glyph stands for, used to build the font's `/ToUnicode` map: several
    /// characters for a ligature, empty for a glyph whose text another glyph carries. Despite
    /// the name it is not a CID. `None` leaves the text to the font's reverse lookup.
    pub cid: Option<String>,
}

impl Codepoint {
    /// A glyph without text.
    pub fn new(gid: u16, offset: f32) -> Self {
        Self { gid, offset, cid: None }
    }
    
    /// A glyph with the text it stands for.
    pub fn with_cid(gid: u16, offset: f32, cid: String) -> Self {
        Self { gid, offset, cid: Some(cid) }
    }
}

/// One element of a `TJ` array: text, a kerning number, or a run of glyph ids.
///
/// Serialized with serde untagged: a string, a number, or an array of [`Codepoint`].
#[derive(Debug, Clone, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TextItem {
    /// Text, encoded for the current font when the document is saved.
    Text(String),
    /// Kerning, in thousandths of the font size; a positive value moves the pen left.
    Offset(f32),
    /// Glyph ids written as they are, for text that was shaped or read from a PDF; no
    /// character lookup is needed.
    GlyphIds(Vec<Codepoint>),
}

impl From<String> for TextItem {
    fn from(s: String) -> Self {
        TextItem::Text(s)
    }
}

impl From<&str> for TextItem {
    fn from(s: &str) -> Self {
        TextItem::Text(s.to_string())
    }
}

impl From<f32> for TextItem {
    fn from(n: f32) -> Self {
        TextItem::Offset(n)
    }
}

impl From<f64> for TextItem {
    fn from(n: f64) -> Self {
        TextItem::Offset(n as f32)
    }
}

impl From<i32> for TextItem {
    fn from(n: i32) -> Self {
        TextItem::Offset(n as f32)
    }
}

impl From<i64> for TextItem {
    fn from(n: i64) -> Self {
        TextItem::Offset(n as f32)
    }
}

impl From<u8> for TextItem {
    fn from(n: u8) -> Self {
        TextItem::Offset(n as f32)
    }
}

/// Maps the bytes of a PDF string to text, as a `/ToUnicode` CMap does.
///
/// Implemented by [`ToUnicodeCMap`](crate::cmap::ToUnicodeCMap).
pub trait CMap {
    /// Decodes `bytes` to text. Unmapped codes are skipped.
    fn map_bytes(&self, bytes: &[u8]) -> String;
}

/// Decodes one `WinAnsiEncoding` (Windows-1252) byte to its character.
///
/// This is the inverse of the encoding used for the 14 built-in fonts (ISO 32000-1, Annex D.2).
/// It agrees with ASCII in `0x20..=0x7E` and with Latin-1 in `0xA0..=0xFF`; `0x80..=0x9F` holds
/// typographic punctuation. Tab, newline and carriage return pass through. Every other byte
/// decodes to U+FFFD.
pub fn win_ansi_char(b: u8) -> char {
    match b {
        0x20..=0x7E => b as char,
        0xA0..=0xFF => b as char,
        0x80 => '\u{20AC}', // € euro
        0x82 => '\u{201A}', // ‚ single low-9 quote
        0x83 => '\u{0192}', // ƒ florin
        0x84 => '\u{201E}', // „ double low-9 quote
        0x85 => '\u{2026}', // … ellipsis
        0x86 => '\u{2020}', // † dagger
        0x87 => '\u{2021}', // ‡ double dagger
        0x88 => '\u{02C6}', // ˆ circumflex
        0x89 => '\u{2030}', // ‰ per mille
        0x8A => '\u{0160}', // Š S caron
        0x8B => '\u{2039}', // ‹ single left angle quote
        0x8C => '\u{0152}', // Œ OE ligature
        0x8E => '\u{017D}', // Ž Z caron
        0x91 => '\u{2018}', // ' left single quote
        0x92 => '\u{2019}', // ' right single quote
        0x93 => '\u{201C}', // " left double quote
        0x94 => '\u{201D}', // " right double quote
        0x95 => '\u{2022}', // • bullet
        0x96 => '\u{2013}', // – en dash
        0x97 => '\u{2014}', // — em dash
        0x98 => '\u{02DC}', // ˜ small tilde
        0x99 => '\u{2122}', // ™ trademark
        0x9A => '\u{0161}', // š s caron
        0x9B => '\u{203A}', // › single right angle quote
        0x9C => '\u{0153}', // œ oe ligature
        0x9E => '\u{017E}', // ž z caron
        0x9F => '\u{0178}', // Ÿ Y diaeresis
        // Tab, newline and carriage return pass through so extraction keeps whitespace;
        // the other control codes and 0x81/0x8D/0x8F/0x90/0x9D are unassigned.
        b'\t' | b'\n' | b'\r' => b as char,
        _ => '\u{FFFD}',
    }
}

/// Decodes the bytes of a text-showing operator of a simple (one byte per code) font.
///
/// Each byte is looked up in the font's `/ToUnicode` CMap when there is one, and decoded as
/// WinAnsi otherwise.
pub fn decode_simple_font_bytes(bytes: &[u8], to_unicode: Option<&crate::cmap::ToUnicodeCMap>) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if let Some(s) = to_unicode.and_then(|c| c.lookup_string(b as u32)) {
            out.push_str(&s);
        } else {
            out.push(win_ansi_char(b));
        }
    }
    out
}

/// Decodes a PDF string object to text; any other object gives an empty string.
///
/// A hexadecimal string that starts with the byte order mark `FE FF` is UTF-16BE. Any other
/// string goes through `to_unicode` when given, and is read as UTF-8 (lossily) otherwise.
pub fn decode_pdf_string(obj: &Object, to_unicode: Option<&impl CMap>) -> String {
    if let Object::String(ref bytes, format) = obj {
        match format {
            StringFormat::Literal => {
                // lopdf has already unescaped the bytes of a literal string.
                if let Some(cmap) = to_unicode {
                    cmap.map_bytes(bytes)
                } else {
                    String::from_utf8_lossy(bytes).into_owned()
                }
            }
            StringFormat::Hexadecimal => {
                // For hex strings the bytes are the raw binary data.
                if let Some(utf16_bytes) = bytes.strip_prefix(&[0xFE, 0xFF]) {
                    // Contains a BOM – assume UTF-16BE. A trailing odd byte is dropped.
                    let utf16_iter = utf16_bytes.chunks_exact(2).filter_map(|pair| match *pair {
                        [hi, lo] => Some(u16::from_be_bytes([hi, lo])),
                        _ => None,
                    });
                    String::from_utf16(&utf16_iter.collect::<Vec<_>>()).unwrap_or_default()
                } else {
                    // Without BOM, use the ToUnicode mapping if available, or fallback.
                    if let Some(cmap) = to_unicode {
                        cmap.map_bytes(bytes)
                    } else {
                        String::from_utf8_lossy(bytes).into_owned()
                    }
                }
            }
        }
    } else {
        String::new()
    }
}

/// Decodes the operands of a `TJ` operator: strings become [`TextItem::Text`] (see
/// [`decode_pdf_string`]) and numbers become [`TextItem::Offset`]. Other objects are skipped.
pub fn decode_tj_operands(operands: &[Object], to_unicode: Option<&impl CMap>) -> Vec<TextItem> {
    let mut items = Vec::new();
    for obj in operands {
        match obj {
            Object::String(_, _) => {
                let s = decode_pdf_string(obj, to_unicode);
                items.push(TextItem::Text(s));
            }
            Object::Integer(i) => {
                items.push(TextItem::Offset(*i as f32));
            }
            Object::Real(r) => {
                items.push(TextItem::Offset(*r));
            }
            _ => {
                // Other object types are skipped.
            }
        }
    }
    items
}

/// Decodes the operands of a `TJ` operator to glyph ids, without looking up any text.
///
/// Strings become [`TextItem::GlyphIds`] and numbers become [`TextItem::Offset`]; other
/// objects are skipped. The length of a string decides how it is read: an even length of at
/// least 2 is taken as two-byte codes (an `Identity-H` font), anything else as one byte per
/// code.
pub fn decode_tj_operands_as_glyph_ids(operands: &[Object]) -> Vec<TextItem> {
    let mut items = Vec::new();
    let mut current_glyphs = Vec::new();
    
    for obj in operands {
        match obj {
            Object::String(bytes, _) => {
                // Extract glyph IDs from the byte string
                // For CID fonts, each glyph is typically 2 bytes (big-endian u16)
                // For simple fonts, each byte is a glyph ID
                if bytes.len() >= 2 && bytes.len() % 2 == 0 {
                    // Assume CID font (2 bytes per glyph)
                    for chunk in bytes.chunks_exact(2) {
                        if let [hi, lo] = *chunk {
                            let gid = u16::from_be_bytes([hi, lo]);
                            current_glyphs.push(Codepoint::new(gid, 0.0));
                        }
                    }
                } else {
                    // Simple font (1 byte per glyph)
                    for &byte in bytes {
                        current_glyphs.push(Codepoint::new(byte as u16, 0.0));
                    }
                }
            }
            Object::Integer(i) => {
                // Offset in thousandths of an em
                if !current_glyphs.is_empty() {
                    // Flush current glyphs before adding offset
                    items.push(TextItem::GlyphIds(std::mem::take(&mut current_glyphs)));
                }
                items.push(TextItem::Offset(*i as f32));
            }
            Object::Real(r) => {
                if !current_glyphs.is_empty() {
                    items.push(TextItem::GlyphIds(std::mem::take(&mut current_glyphs)));
                }
                items.push(TextItem::Offset(*r));
            }
            _ => {
                // Ignore unsupported types
            }
        }
    }
    
    // Flush remaining glyphs
    if !current_glyphs.is_empty() {
        items.push(TextItem::GlyphIds(current_glyphs));
    }
    
    items
}

/// Decodes the string of a `Tj` operator to glyph ids, with the same length rule as
/// [`decode_tj_operands_as_glyph_ids`]. An empty string gives no items.
pub fn decode_tj_string_as_glyph_ids(bytes: &[u8]) -> Vec<TextItem> {
    let mut glyphs = Vec::new();
    
    // Extract glyph IDs from the byte string
    if bytes.len() >= 2 && bytes.len().is_multiple_of(2) {
        // Assume CID font (2 bytes per glyph)
        for chunk in bytes.chunks_exact(2) {
            if let [hi, lo] = *chunk {
                let gid = u16::from_be_bytes([hi, lo]);
                glyphs.push(Codepoint::new(gid, 0.0));
            }
        }
    } else {
        // Simple font (1 byte per glyph)
        for &byte in bytes {
            glyphs.push(Codepoint::new(byte as u16, 0.0));
        }
    }
    
    if glyphs.is_empty() {
        vec![]
    } else {
        vec![TextItem::GlyphIds(glyphs)]
    }
}

/// Encodes text as a PDF literal string: wrapped in parentheses, with `(`, `)`, `\` and the
/// control characters `\n`, `\r`, `\t`, `\b`, `\f` escaped. Other characters are kept as they
/// are, so the result is not a WinAnsi byte string.
pub fn encode_pdf_string_literal(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('(');
    for c in s.chars() {
        match c {
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\x08' => result.push_str("\\b"),
            '\x0C' => result.push_str("\\f"),
            _ => result.push(c),
        }
    }
    result.push(')');
    result
}

/// Encodes text as a PDF hexadecimal string of UTF-16BE code units with a byte order mark
/// (`<FEFF...>`).
pub fn encode_pdf_string_hex(s: &str) -> String {
    // Encode as UTF-16BE with BOM.
    let mut utf16: Vec<u16> = Vec::new();
    utf16.push(0xFEFF); // BOM
    utf16.extend(s.encode_utf16());
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for code in utf16 {
        bytes.extend_from_slice(&code.to_be_bytes());
    }
    let hex: String = bytes.iter().map(|b| format!("{:02X}", b)).collect();
    format!("<{}>", hex)
}

/// Encodes text as a literal string or a hexadecimal string, whichever is shorter; the
/// literal string wins a tie.
pub fn encode_pdf_string_minimal(s: &str) -> String {
    let literal = encode_pdf_string_literal(s);
    let hex = encode_pdf_string_hex(s);
    if literal.len() <= hex.len() {
        literal
    } else {
        hex
    }
}

/// Encodes [`TextItem`]s as the operands of a `TJ` operator.
///
/// Text becomes a string (literal as UTF-8 bytes, or hexadecimal as UTF-16BE with a byte
/// order mark, whichever is shorter), an offset becomes a real number, and a glyph run becomes
/// one two-byte hexadecimal string per glyph, followed by its offset as an integer (truncated)
/// when it is not zero.
///
/// This is a generic encoding. When a document is saved, text is encoded for the font in use
/// instead (WinAnsi for built-in fonts, glyph ids for embedded fonts).
pub fn encode_text_items(items: &[TextItem]) -> Vec<Object> {
    let mut objs = Vec::new();
    for item in items {
        match item {
            TextItem::Text(s) => {
                let pdf_str = encode_pdf_string_minimal(s);
                // Check if the encoding is hex or literal based on its delimiters.
                if let Some(inner) = pdf_str.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
                    // For hex, remove the delimiters and convert back to bytes.
                    let bytes: Vec<u8> = inner
                        .as_bytes()
                        .chunks_exact(2)
                        .filter_map(|pair| {
                            let digits = std::str::from_utf8(pair).ok()?;
                            u8::from_str_radix(digits, 16).ok()
                        })
                        .collect();
                    objs.push(Object::String(bytes, StringFormat::Hexadecimal));
                } else {
                    // For literal strings, we assume a UTF-8 encoding.
                    objs.push(Object::String(s.as_bytes().to_vec(), StringFormat::Literal));
                }
            }
            TextItem::Offset(n) => {
                // A real number keeps fractional kerning.
                objs.push(Object::Real(*n));
            }
            TextItem::GlyphIds(glyphs) => {
                // Encode glyph IDs as hex string (2 bytes per glyph ID)
                // Interleaved with offsets
                for codepoint in glyphs {
                    let bytes = codepoint.gid.to_be_bytes().to_vec();
                    objs.push(Object::String(bytes, StringFormat::Hexadecimal));
                    if codepoint.offset != 0.0 {
                        objs.push(Object::Integer(codepoint.offset as i64));
                    }
                }
            }
        }
    }
    objs
}

/// The `BDC` operator that opens a `/Span` marked-content sequence with `text` as its
/// `/ActualText` (UTF-16BE with a byte order mark). Close the span with
/// [`Op::EndMarkedContent`](crate::Op::EndMarkedContent).
///
/// Viewers use the `/ActualText` for copying, searching and accessibility instead of
/// reconstructing the text from the glyphs, which fails where a glyph is shared by two letters
/// (Arabic) or where every glyph is placed on its own and the gaps read as spaces. Saving with
/// [`PdfSaveOptions::optimize`](crate::PdfSaveOptions::optimize) drops the span where the
/// glyphs read as `text` anyway ([`crate::optimize_text`]).
pub fn actual_text_span(text: &str) -> crate::Op {
    let mut data = vec![0xFE, 0xFF];
    for u in text.encode_utf16() {
        data.extend_from_slice(&u.to_be_bytes());
    }
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        "ActualText".to_string(),
        crate::DictItem::String {
            data,
            literal: false,
        },
    );
    crate::Op::BeginMarkedContentWithProperties {
        tag: "Span".to_string(),
        properties: crate::DictItem::Dict { map },
    }
}

/// A glyph at the position a text layout gave it, for [`show_placed_glyphs`].
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedGlyph {
    /// Index of its font in the `fonts` given to [`show_placed_glyphs`].
    pub font: usize,
    /// Glyph id in that font.
    pub gid: u16,
    /// Horizontal origin in pt, along the baseline from its start, in the text space of the
    /// current text matrix.
    pub x: f32,
    /// Vertical origin in pt, up from the baseline (a mark raised over its letter).
    pub y: f32,
    /// The text the glyph stands for in the font's `/ToUnicode` map: the cluster's text on one
    /// glyph of the cluster, empty on the others (a mark, a dot drawn apart from its letter).
    pub text: String,
}

/// Returns the operations that draw one line of `glyphs` where a text layout put them.
///
/// The glyphs are shown left to right inside a text section whose text matrix is at the start
/// of the baseline (after [`Op::StartTextSection`](crate::Op::StartTextSection) and a matrix,
/// with `Tc` at 0 and `Tz` at 100, which are their initial values). Each glyph is drawn in
/// `fonts[glyph.font]` at `size`; a `font` out of range selects the first font. Nothing is
/// returned when `fonts` or `glyphs` is empty or `size` is not positive.
///
/// The operations are:
///
/// - one `TJ` per stretch of one font and one rise, with kerning numbers that move the pen to
///   each glyph. They are measured against the glyph widths in `/W` ([`glyph_pdf_width`]), so
///   kerning, mark offsets, right-to-left runs and letter spacing land where the layout put
///   them;
/// - a glyph above or below the baseline (a mark) is shown with the text rise `Ts`, which is 0
///   again at the end;
/// - with `actual_text`, all of it inside an [`actual_text_span`] of that text.
///
/// `fonts` pairs each font's [`FontId`](crate::FontId), as added to the document, with its
/// parsed face.
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
pub fn show_placed_glyphs(
    fonts: &[(crate::FontId, &crate::font::AzulParsedFont)],
    size: crate::Pt,
    glyphs: &[PlacedGlyph],
    actual_text: Option<&str>,
) -> Vec<crate::Op> {
    use crate::Op;

    fn flush(ops: &mut Vec<Op>, items: &mut Vec<TextItem>, run: &mut Vec<Codepoint>) {
        if !run.is_empty() {
            items.push(TextItem::GlyphIds(std::mem::take(run)));
        }
        if !items.is_empty() {
            ops.push(Op::ShowText {
                items: std::mem::take(items),
            });
        }
    }

    let mut ops = Vec::new();
    if fonts.is_empty() || size.0 <= 0.0 || glyphs.is_empty() {
        return ops;
    }
    let span = actual_text.filter(|t| !t.is_empty());
    if let Some(text) = span {
        ops.push(actual_text_span(text));
    }
    // where the viewer's pen stands and the rise in force, in pt from the baseline's start
    let mut pen = 0.0f32;
    let mut rise = 0.0f32;
    let mut current: Option<usize> = None;
    let mut items: Vec<TextItem> = Vec::new();
    let mut run: Vec<Codepoint> = Vec::new();
    for g in glyphs {
        let font = if g.font < fonts.len() { g.font } else { 0 };
        // `fonts` is not empty and `font` is in range, so there is always a face.
        let Some((font_id, face)) = fonts.get(font) else {
            continue;
        };
        if current != Some(font) {
            flush(&mut ops, &mut items, &mut run);
            ops.push(Op::SetFont {
                font: crate::ops::PdfFontHandle::External(font_id.clone()),
                size,
            });
            current = Some(font);
        }
        if (g.y - rise).abs() > 1e-5 {
            flush(&mut ops, &mut items, &mut run);
            ops.push(Op::SetLineOffset { multiplier: g.y });
            rise = g.y;
        }
        // a TJ number moves the pen back by a thousandth of the font size
        let kern = (g.x - pen) / size.0 * 1000.0;
        if kern.abs() > 1e-3 {
            match run.last_mut() {
                Some(last) => last.offset -= kern,
                None => items.push(TextItem::Offset(-kern)),
            }
        }
        run.push(Codepoint {
            gid: g.gid,
            offset: 0.0,
            cid: Some(g.text.clone()),
        });
        pen = g.x + glyph_pdf_width(face, g.gid) as f32 / 1000.0 * size.0;
    }
    flush(&mut ops, &mut items, &mut run);
    if rise != 0.0 {
        ops.push(Op::SetLineOffset { multiplier: 0.0 });
    }
    if span.is_some() {
        ops.push(Op::EndMarkedContent);
    }
    ops
}

/// The width of a glyph as printpdf writes it to the `/W` array: its advance in thousandths of
/// an em, truncated to a whole number, or 0 when unknown. A viewer advances the pen by it.
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
pub fn glyph_pdf_width(font: &crate::font::AzulParsedFont, gid: u16) -> i64 {
    let units_per_em = f32::from(font.pdf_font_metrics.units_per_em);
    if units_per_em <= 0.0 {
        return 0;
    }
    font.get_or_decode_glyph(gid)
        .map(|g| (f32::from(g.horz_advance) * 1000.0 / units_per_em) as i64)
        .unwrap_or(0)
}
