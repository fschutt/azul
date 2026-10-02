//! HTML as a browser reads it, and the ONE tree construction every XML
//! loader shares.
//!
//! Three pieces, each used by more than one loader:
//!
//! - [`HtmlTokenizer`]: the LENIENT tokenizer for HTML content (a mail, a paste from a browser
//!   or Word). It never fails. It reads unquoted and bare (boolean) attributes, upper-case names,
//!   `--` inside a comment, `<` and `&` in text and attribute values, a `<!DOCTYPE>` anywhere,
//!   `<![if ...]>` (Word), the raw text of `<style>` / `<script>` and the HTML Standard's named
//!   character references, and drops stray control characters. The strict loaders keep
//!   `xmlparser` as their tokenizer (an XML syntax error is an error there).
//! - [`TreeBuilder`]: the tree construction - void elements, implied end tags (`p`, `li`,
//!   `dt` / `dd`, `option`, `tr` / `td` / `th`, the row groups), an end tag closing up to its
//!   element only within the element's scope (a stray `</div>` inside a table cell does not
//!   close the cell, the row and the table), a misnested formatting end tag (`<b><p>x</b>`)
//!   closing its element once the block inside it closes. Under [`TreeRules::Html`] also what
//!   a browser repairs in a document: the implied `<html>` / `<head>` / `<body>`, `<tbody>`
//!   and `<tr>`, `</p>` and `</br>` without their start tags, the formatting elements reopened
//!   in the next block (`<p><b>x<p>y` - "y" is bold too), the elements left open at the end.
//!   Every XML loader runs it: the tree loader (`azul_layout::xml::parse_xml_string`), the
//!   document loader (`azul_layout::xml::parse_xml_to_fast_dom`) and the lenient loaders - so
//!   the loaders can no longer build two different trees from one document.
//! - [`XmlTreeSink`]: the tree construction's output as an [`XmlNode`] tree.
//!
//! What a browser does that this does not (a simplified tree construction, see
//! `scripts/LENIENT_2026_10_01.md`): foster parenting (content misplaced inside a `<table>`
//! stays where it is), the full adoption agency (a misnested formatting element is closed
//! when its block closes, not cloned), quirks mode (a `<table>` always closes an open `<p>`),
//! namespaces (an `<svg>`'s elements are told apart only by being inside it).

use alloc::{borrow::Cow, string::String, vec::Vec};
use core::cmp::Ordering;

use super::{
    entities::HTML5_NAMED_REFERENCES, AzString, AzStringPair, XmlNode, XmlNodeChild,
    MAX_XML_NESTING_DEPTH,
};

// ============================================================================
// Character references
// ============================================================================

/// How [`decode_character_references`] reads a `&...` reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharRefMode {
    /// XML's rules with HTML's names (the strict loaders): a reference ends
    /// with `;`; a numeric one that is not a Unicode scalar value, and an
    /// unknown name, stay as written.
    Xml,
    /// HTML's rules in text (the HTML Standard's character reference state):
    /// the legacy names also without their `;` (`&copy 2026`), a numeric
    /// reference through the Windows-1252 repair (`&#150;` is an en dash),
    /// an invalid one as U+FFFD.
    HtmlText,
    /// HTML's rules in an attribute value: as in text, except that a legacy
    /// name without its `;` that is followed by `=`, a letter or a digit
    /// stays as written (`href="?a=1&copy=2"` keeps its query).
    HtmlAttribute,
}

/// The characters a named character reference stands for.
///
/// `name` is written without the `&`; `with_semicolon` looks it up as
/// written with its `;` (`"copy"` + `;` is `©`), else as one of the legacy
/// names a browser also reads without one. `None` for an unknown name.
#[must_use]
pub fn named_character_reference(name: &str, with_semicolon: bool) -> Option<&'static str> {
    let suffix: &[u8] = if with_semicolon { b";" } else { b"" };
    HTML5_NAMED_REFERENCES
        .binary_search_by(|(k, _)| compare_with_suffix(k, name, suffix))
        .ok()
        .map(|i| HTML5_NAMED_REFERENCES[i].1)
}

/// `key` against `name` followed by `suffix`, byte-wise (the table's order).
fn compare_with_suffix(key: &str, name: &str, suffix: &[u8]) -> Ordering {
    key.bytes()
        .cmp(name.bytes().chain(suffix.iter().copied()))
}

/// The C1 range of a numeric reference as Windows-1252 reads it (the HTML
/// Standard's table): `&#128;` is the euro sign. 0 = the code point stays.
const WINDOWS_1252_C1: [u32; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
    0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
];

/// The character an HTML numeric reference stands for.
fn html_numeric_character(code: u32) -> char {
    let code = match code {
        0 | 0xD800..=0xDFFF => 0xFFFD,
        0x80..=0x9F => {
            let repaired = usize::try_from(code - 0x80)
                .ok()
                .and_then(|i| WINDOWS_1252_C1.get(i).copied())
                .unwrap_or(0);
            if repaired == 0 {
                code
            } else {
                repaired
            }
        }
        c => c,
    };
    char::from_u32(code).unwrap_or('\u{FFFD}')
}

/// Decode the one reference that starts after an `&` in `after`, pushing its
/// characters to `out`; the bytes it took (after the `&`), or `None` when
/// `&` is not the start of a reference here (it is then a literal `&`).
fn decode_one(after: &str, mode: CharRefMode, out: &mut String) -> Option<usize> {
    let bytes = after.as_bytes();
    if bytes.first() == Some(&b'#') {
        let hex = matches!(bytes.get(1), Some(b'x' | b'X'));
        let start = if hex { 2 } else { 1 };
        let digits = after.get(start..)?;
        let len = digits
            .bytes()
            .take_while(|b| {
                if hex {
                    b.is_ascii_hexdigit()
                } else {
                    b.is_ascii_digit()
                }
            })
            .count();
        if len == 0 {
            return None;
        }
        let terminated = digits.as_bytes().get(len) == Some(&b';');
        let taken = start + len + usize::from(terminated);
        let radix = if hex { 16 } else { 10 };
        let value = u32::from_str_radix(&digits[..len], radix);
        if mode == CharRefMode::Xml {
            if !terminated {
                return None;
            }
            out.push(char::from_u32(value.ok()?)?);
        } else {
            // An overflowing number is out of range like any other.
            out.push(html_numeric_character(value.unwrap_or(u32::MAX)));
        }
        return Some(taken);
    }
    let len = bytes.iter().take_while(|b| b.is_ascii_alphanumeric()).count();
    if len == 0 {
        return None;
    }
    let name = &after[..len];
    if bytes.get(len) == Some(&b';') {
        if let Some(chars) = named_character_reference(name, true) {
            out.push_str(chars);
            return Some(len + 1);
        }
    }
    if mode == CharRefMode::Xml {
        return None;
    }
    // A legacy name (2..=6 letters) the name starts with, the longest first:
    // `&notit;` is `¬it;`, as in a browser.
    for n in (2..=len.min(6)).rev() {
        let Some(chars) = named_character_reference(&name[..n], false) else {
            continue;
        };
        if mode == CharRefMode::HtmlAttribute {
            let next = bytes.get(n);
            if next == Some(&b'=') || next.is_some_and(u8::is_ascii_alphanumeric) {
                return None;
            }
        }
        out.push_str(chars);
        return Some(n);
    }
    None
}

/// Decode the character references (`&amp;`, `&#65;`, `&#x41;`, `&copy;` ...)
/// in `s` by `mode`'s rules, in one left-to-right pass (`&amp;lt;` is the
/// text `&lt;`). Borrows `s` when it has no `&`.
#[must_use]
pub fn decode_character_references(s: &str, mode: CharRefMode) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        if let Some(taken) = decode_one(after, mode, &mut out) {
            rest = &after[taken..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// `s` as the text content of an HTML / XML element (RED stub: no encoding yet).
#[must_use]
pub fn encode_text(s: &str) -> String {
    String::from(s)
}

/// `s` as an attribute value (RED stub: no encoding yet).
#[must_use]
pub fn encode_attribute(s: &str) -> String {
    String::from(s)
}

// ============================================================================
// The lenient tokenizer
// ============================================================================

/// HTML's white space: tab, line feed, form feed, carriage return, space.
const fn is_html_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

/// `s` without its `\r` (CR LF and a lone CR are a line feed, as HTML's
/// input preprocessing has it) and without the control characters other
/// than tab, line feed and form feed; then its character references decoded
/// by `decode` (`None`: raw text, as written).
fn clean_text(s: &str, decode: Option<CharRefMode>) -> Cow<'_, str> {
    let needs_cleaning = s
        .bytes()
        .any(|b| b < 0x20 && !matches!(b, b'\t' | b'\n' | 0x0C));
    let cleaned: Cow<'_, str> = if needs_cleaning {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    if chars.peek() != Some(&'\n') {
                        out.push('\n');
                    }
                }
                '\t' | '\n' | '\u{C}' => out.push(c),
                c if u32::from(c) < 0x20 => {}
                c => out.push(c),
            }
        }
        Cow::Owned(out)
    } else {
        Cow::Borrowed(s)
    };
    let Some(mode) = decode else {
        return cleaned;
    };
    match cleaned {
        Cow::Borrowed(b) => decode_character_references(b, mode),
        Cow::Owned(o) if o.contains('&') => {
            Cow::Owned(decode_character_references(&o, mode).into_owned())
        }
        Cow::Owned(o) => Cow::Owned(o),
    }
}

/// The elements whose content is text, not markup: `(name, decoded)` -
/// RCDATA (`title`, `textarea`) has its character references decoded, raw
/// text (`style`, `script` ...) is kept as written.
fn raw_text_element(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "style" => ("style", false),
        "script" => ("script", false),
        "xmp" => ("xmp", false),
        "iframe" => ("iframe", false),
        "noembed" => ("noembed", false),
        "noframes" => ("noframes", false),
        "plaintext" => ("plaintext", false),
        "title" => ("title", true),
        "textarea" => ("textarea", true),
        _ => return None,
    })
}

/// Where the raw text in `rest` ends: at `</name` followed by white space,
/// `/`, `>` or the end (the name in any case), else at the end.
fn raw_text_end(rest: &str, name: &str) -> usize {
    let bytes = rest.as_bytes();
    let n = name.len();
    let mut from = 0;
    while let Some(found) = rest.get(from..).and_then(|r| r.find("</")) {
        let lt = from + found;
        let after = lt + 2;
        if bytes
            .get(after..after + n)
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name.as_bytes()))
        {
            match bytes.get(after + n) {
                None => return lt,
                Some(&b) if is_html_space(b) || b == b'/' || b == b'>' => return lt,
                Some(_) => {}
            }
        }
        from = after;
    }
    rest.len()
}

/// `true` if `s` (starting with `<`) starts markup: a tag, an end tag, a
/// comment, a doctype or a processing instruction. Any other `<` is text.
fn starts_markup(s: &str) -> bool {
    let b = s.as_bytes();
    b.first() == Some(&b'<')
        && match b.get(1) {
            Some(c) if c.is_ascii_alphabetic() => true,
            Some(b'!' | b'?') => true,
            Some(b'/') => b.get(2).is_some(),
            _ => false,
        }
}

/// How long the text run at the start of `rest` is: up to the next `<` that
/// starts markup (a `<` that does not is part of the text).
fn text_run_len(rest: &str) -> usize {
    let mut from = 0;
    while let Some(found) = rest.get(from..).and_then(|r| r.find('<')) {
        let lt = from + found;
        if lt > 0 && starts_markup(&rest[lt..]) {
            return lt;
        }
        from = lt + 1;
    }
    rest.len()
}

/// A tag or attribute name's length: up to white space, `/` or `>`.
fn name_len(s: &str) -> usize {
    s.bytes()
        .take_while(|&b| !(is_html_space(b) || b == b'/' || b == b'>'))
        .count()
}

/// A token of HTML markup ([`HtmlTokenizer`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlToken<'a> {
    /// `<name a=1 b="2" c>` / `<name ... />`: the name and the attribute
    /// names lower-cased, the values decoded (an attribute written twice
    /// keeps its first value; a bare one is `""`).
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    /// `</name>`, the name lower-cased (what else the tag holds is ignored).
    EndTag { name: String },
    /// Text: its character references decoded (the raw text of a `<style>`
    /// or a `<script>` as written), CR LF as LF, stray control characters
    /// dropped.
    Text(Cow<'a, str>),
    /// `<!-- .. -->`: the text between the markers.
    Comment(&'a str),
    /// `<![CDATA[ .. ]]>`: its text.
    Cdata(&'a str),
}

/// The lenient HTML tokenizer: an iterator of [`HtmlToken`]s that never fails.
///
/// Doctypes, processing instructions (`<?xml ..?>`) and bogus comments
/// (`<![if !supportLists]>`, `</ x>`) are skipped; a tag cut off by the end
/// of the input is dropped, as a browser drops it.
#[derive(Debug, Clone)]
pub struct HtmlTokenizer<'a> {
    src: &'a str,
    pos: usize,
    /// After the start tag of a raw-text element: its name, and whether its
    /// text has its references decoded.
    raw: Option<(&'static str, bool)>,
}

impl<'a> HtmlTokenizer<'a> {
    /// A tokenizer over `src` (a leading byte order mark is skipped).
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self {
            src: src.strip_prefix('\u{FEFF}').unwrap_or(src),
            pos: 0,
            raw: None,
        }
    }

    /// Move past the next `c` (or to the end).
    fn skip_past(&mut self, c: char) {
        self.pos = match self.src.get(self.pos..).and_then(|r| r.find(c)) {
            Some(i) => self.pos + i + c.len_utf8(),
            None => self.src.len(),
        };
    }

    /// The markup at `pos` (which [`starts_markup`]); `None` when it yields
    /// no token (a doctype, a bogus comment, a cut-off tag).
    fn markup(&mut self) -> Option<HtmlToken<'a>> {
        let src = self.src;
        let rest = &src[self.pos..];
        let b = rest.as_bytes();
        match b.get(1) {
            Some(b'!') => {
                if let Some(body) = rest.strip_prefix("<!--") {
                    // `<!-->` and `<!--->`: empty comments.
                    if body.starts_with('>') {
                        self.pos += 5;
                        return Some(HtmlToken::Comment(""));
                    }
                    if body.starts_with("->") {
                        self.pos += 6;
                        return Some(HtmlToken::Comment(""));
                    }
                    let end = [body.find("-->").map(|i| (i, 3)), body.find("--!>").map(|i| (i, 4))]
                        .into_iter()
                        .flatten()
                        .min();
                    let (len, close) = end.unwrap_or((body.len(), 0));
                    self.pos += 4 + len + close;
                    return Some(HtmlToken::Comment(&body[..len]));
                }
                if let Some(body) = rest.strip_prefix("<![CDATA[") {
                    let (len, close) = body.find("]]>").map_or((body.len(), 0), |i| (i, 3));
                    self.pos += 9 + len + close;
                    return Some(HtmlToken::Cdata(&body[..len]));
                }
                // `<!DOCTYPE ..>`, Word's `<![if ..]>` / `<![endif]>`.
                self.skip_past('>');
                None
            }
            Some(b'?') => {
                self.skip_past('>');
                None
            }
            Some(b'/') => match b.get(2) {
                Some(c) if c.is_ascii_alphabetic() => {
                    let end = 2 + name_len(&rest[2..]);
                    let name = rest[2..end].to_ascii_lowercase();
                    if let Some(gt) = rest[end..].find('>') {
                        self.pos += end + gt + 1;
                        Some(HtmlToken::EndTag { name })
                    } else {
                        self.pos = src.len();
                        None
                    }
                }
                Some(b'>') => {
                    self.pos += 3;
                    None
                }
                _ => {
                    self.skip_past('>');
                    None
                }
            },
            _ => self.start_tag(),
        }
    }

    /// The start tag at `pos` (`<` and a letter).
    fn start_tag(&mut self) -> Option<HtmlToken<'a>> {
        let src = self.src;
        let rest = &src[self.pos..];
        let bytes = rest.as_bytes();
        let name_end = 1 + name_len(&rest[1..]);
        let name = rest[1..name_end].to_ascii_lowercase();
        let mut attributes: Vec<(String, String)> = Vec::new();
        let mut self_closing = false;
        let mut i = name_end;
        loop {
            while bytes.get(i).is_some_and(|&b| is_html_space(b)) {
                i += 1;
            }
            let Some(&c) = bytes.get(i) else {
                // Cut off by the end of the input: dropped.
                self.pos = src.len();
                return None;
            };
            if c == b'>' {
                i += 1;
                break;
            }
            if c == b'/' {
                i += 1;
                if bytes.get(i) == Some(&b'>') {
                    self_closing = true;
                    i += 1;
                    break;
                }
                continue;
            }
            // The name: up to white space, `/`, `>` or `=` (a first `=` is
            // part of it).
            let name_start = i;
            i += 1;
            while bytes
                .get(i)
                .is_some_and(|&b| !(is_html_space(b) || matches!(b, b'/' | b'>' | b'=')))
            {
                i += 1;
            }
            let attribute = rest[name_start..i].to_ascii_lowercase();
            let mut j = i;
            while bytes.get(j).is_some_and(|&b| is_html_space(b)) {
                j += 1;
            }
            let mut value = String::new();
            if bytes.get(j) == Some(&b'=') {
                j += 1;
                while bytes.get(j).is_some_and(|&b| is_html_space(b)) {
                    j += 1;
                }
                match bytes.get(j) {
                    Some(&quote) if quote == b'"' || quote == b'\'' => {
                        let start = j + 1;
                        let Some(len) = rest[start..].find(char::from(quote)) else {
                            self.pos = src.len();
                            return None;
                        };
                        value = clean_text(&rest[start..start + len], Some(CharRefMode::HtmlAttribute))
                            .into_owned();
                        j = start + len + 1;
                    }
                    Some(b'>') | None => {}
                    Some(_) => {
                        let start = j;
                        while bytes.get(j).is_some_and(|&b| !(is_html_space(b) || b == b'>')) {
                            j += 1;
                        }
                        value = clean_text(&rest[start..j], Some(CharRefMode::HtmlAttribute))
                            .into_owned();
                    }
                }
                i = j;
            }
            if !attributes.iter().any(|(k, _)| *k == attribute) {
                attributes.push((attribute, value));
            }
        }
        self.pos += i;
        // A raw-text element's text follows (a self-closing flag does not
        // end a `<style/>` in HTML).
        self.raw = raw_text_element(&name);
        Some(HtmlToken::StartTag {
            name,
            attributes,
            self_closing,
        })
    }
}

impl<'a> Iterator for HtmlTokenizer<'a> {
    type Item = HtmlToken<'a>;

    fn next(&mut self) -> Option<HtmlToken<'a>> {
        let src = self.src;
        loop {
            if self.pos >= src.len() {
                return None;
            }
            if let Some((name, decoded)) = self.raw.take() {
                let rest = &src[self.pos..];
                let len = raw_text_end(rest, name);
                self.pos += len;
                if len > 0 {
                    let mode = decoded.then_some(CharRefMode::HtmlText);
                    return Some(HtmlToken::Text(clean_text(&rest[..len], mode)));
                }
                continue;
            }
            let rest = &src[self.pos..];
            if starts_markup(rest) {
                if let Some(token) = self.markup() {
                    return Some(token);
                }
                continue;
            }
            let len = text_run_len(rest);
            self.pos += len;
            return Some(HtmlToken::Text(clean_text(
                &rest[..len],
                Some(CharRefMode::HtmlText),
            )));
        }
    }
}

// ============================================================================
// The tree construction
// ============================================================================

/// Which repairs a [`TreeBuilder`] makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeRules {
    /// XML as written (the tree loader, `parse_xml_string`): names keep
    /// their case; the HTML conveniences every loader has - void elements,
    /// implied end tags, an end tag closing only within its element's
    /// scope; `<x/>` is an empty element; an element left open is reported
    /// by [`TreeBuilder::finish`].
    Xml,
    /// The same with the element names lower-cased (the document loader,
    /// `parse_xml_to_fast_dom`).
    XmlFolded,
    /// HTML as a browser reads it (the lenient loaders): lower-cased names,
    /// and the document repairs (see the module docs). `<x/>` is an empty
    /// element only for the void elements and inside `<svg>` / `<math>`.
    Html,
}

impl TreeRules {
    const fn folds_case(self) -> bool {
        !matches!(self, Self::Xml)
    }

    const fn html(self) -> bool {
        matches!(self, Self::Html)
    }
}

/// Where a [`TreeBuilder`] puts what it builds: open and close are always
/// balanced, the element of a `close_element` is the last one opened and
/// not yet closed.
pub trait TreeSink {
    /// An element opens (as a child of the open one, or as a root).
    fn open_element(&mut self, name: &str, attributes: &[(String, String)]);
    /// The open element closes.
    fn close_element(&mut self);
    /// Text in the open element (or at the root).
    fn text(&mut self, text: &str);
}

/// The HTML void elements (the HTML Standard's, and the legacy ones its
/// parser treats so): no content, no end tag.
#[must_use]
pub fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "source"
            | "track"
            | "wbr"
            | "param"
            | "keygen"
            | "basefont"
            | "bgsound"
            | "frame"
    )
}

/// Whether a `start` tag ends an open `open` element it would go into
/// directly: the tree construction's implied end tags for a direct parent
/// (`<p>` ends where a `<div>` starts, an `<li>` at the next `<li>`, a cell at
/// the next cell or row). What a document editor asks before it nests
/// `start` inside `open`: a loader would make them siblings. Names are
/// lower-case.
#[must_use]
pub fn start_tag_closes(open: &str, start: &str) -> bool {
    match open {
        "p" => closes_p(start),
        "li" => start == "li",
        "dd" | "dt" => matches!(start, "dd" | "dt"),
        "option" => matches!(start, "option" | "optgroup"),
        "optgroup" => start == "optgroup",
        "td" | "th" => matches!(
            start,
            "td" | "th" | "tr" | "thead" | "tbody" | "tfoot" | "caption" | "colgroup"
        ),
        "tr" => matches!(
            start,
            "tr" | "thead" | "tbody" | "tfoot" | "caption" | "colgroup"
        ),
        "thead" | "tbody" | "tfoot" | "caption" | "colgroup" => matches!(
            start,
            "thead" | "tbody" | "tfoot" | "caption" | "colgroup"
        ),
        h if is_heading(h) => is_heading(start),
        _ => false,
    }
}

/// The formatting elements: what a browser reopens in the next block when
/// a block start closes them (`<p><b>x<p>y`).
fn is_formatting(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "b"
            | "big"
            | "code"
            | "em"
            | "font"
            | "i"
            | "nobr"
            | "s"
            | "small"
            | "strike"
            | "strong"
            | "tt"
            | "u"
    )
}

fn is_heading(tag: &str) -> bool {
    matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// The start tags that close an open `<p>` (the HTML Standard's "close a p
/// element" in the body; `li`, `dd`, `dt` and the headings close it too).
fn closes_p(tag: &str) -> bool {
    is_heading(tag)
        || matches!(
            tag,
            "address"
                | "article"
                | "aside"
                | "blockquote"
                | "center"
                | "details"
                | "dialog"
                | "dir"
                | "div"
                | "dl"
                | "fieldset"
                | "figcaption"
                | "figure"
                | "footer"
                | "form"
                | "header"
                | "hgroup"
                | "hr"
                | "main"
                | "menu"
                | "nav"
                | "ol"
                | "p"
                | "pre"
                | "search"
                | "section"
                | "summary"
                | "ul"
                | "listing"
                | "xmp"
                | "plaintext"
                | "table"
                | "li"
                | "dd"
                | "dt"
        )
}

/// The HTML Standard's "special" elements: the blocks (and the others) that
/// an `<li>`'s search for an open `<li>` stops at, and that make a
/// formatting element's end tag misnested.
fn is_special(tag: &str) -> bool {
    is_heading(tag)
        || is_void_element(tag)
        || matches!(
            tag,
            "address"
                | "applet"
                | "article"
                | "aside"
                | "blockquote"
                | "body"
                | "button"
                | "caption"
                | "center"
                | "colgroup"
                | "dd"
                | "details"
                | "dir"
                | "div"
                | "dl"
                | "dt"
                | "fieldset"
                | "figcaption"
                | "figure"
                | "footer"
                | "form"
                | "frameset"
                | "head"
                | "header"
                | "hgroup"
                | "html"
                | "iframe"
                | "li"
                | "listing"
                | "main"
                | "marquee"
                | "menu"
                | "nav"
                | "noembed"
                | "noframes"
                | "noscript"
                | "object"
                | "ol"
                | "p"
                | "plaintext"
                | "pre"
                | "script"
                | "search"
                | "section"
                | "select"
                | "style"
                | "summary"
                | "table"
                | "tbody"
                | "td"
                | "template"
                | "textarea"
                | "tfoot"
                | "th"
                | "thead"
                | "title"
                | "tr"
                | "ul"
                | "xmp"
                | "foreignobject"
        )
}

/// The elements that belong in the `<head>`.
fn is_head_content(tag: &str) -> bool {
    matches!(
        tag,
        "base"
            | "basefont"
            | "bgsound"
            | "link"
            | "meta"
            | "noframes"
            | "noscript"
            | "script"
            | "style"
            | "template"
            | "title"
    )
}

/// The elements whose text is not the document's content: no formatting
/// element is reopened inside them, and their text does not start a body.
fn holds_raw_text(tag: &str) -> bool {
    matches!(
        tag,
        "style"
            | "script"
            | "title"
            | "textarea"
            | "xmp"
            | "iframe"
            | "noembed"
            | "noframes"
            | "plaintext"
            | "template"
    )
}

/// The table structure that holds no content of its own.
fn is_table_structure(tag: &str) -> bool {
    matches!(tag, "table" | "tbody" | "thead" | "tfoot" | "tr")
}

/// The parts of a table, which a browser ignores outside one.
fn is_table_part(tag: &str) -> bool {
    matches!(
        tag,
        "caption" | "col" | "colgroup" | "tbody" | "td" | "tfoot" | "th" | "thead" | "tr"
    )
}

/// The elements that put a marker on the list of formatting elements: a
/// formatting element opened outside one is not reopened inside it.
fn is_marker_element(tag: &str) -> bool {
    matches!(
        tag,
        "td" | "th" | "caption" | "applet" | "object" | "marquee" | "template"
    )
}

/// Whether a browser reopens the formatting elements before inserting this
/// start tag (the inline content; not the blocks, the table, the document).
fn reopens_formatting_before(tag: &str) -> bool {
    !(closes_p(tag)
        || is_head_content(tag)
        || matches!(
            tag,
            "html"
                | "head"
                | "body"
                | "caption"
                | "colgroup"
                | "col"
                | "tbody"
                | "thead"
                | "tfoot"
                | "tr"
                | "td"
                | "th"
                | "frameset"
        ))
}

/// The scope an element is searched in (the HTML Standard's "has an element
/// in scope"): the search from the current node stops at a boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Default,
    ListItem,
    Button,
    Table,
}

impl Scope {
    fn is_boundary(self, tag: &str) -> bool {
        let default = matches!(
            tag,
            "applet"
                | "caption"
                | "html"
                | "table"
                | "td"
                | "th"
                | "marquee"
                | "object"
                | "template"
                | "foreignobject"
        );
        match self {
            Self::Default => default,
            Self::ListItem => default || matches!(tag, "ol" | "ul"),
            Self::Button => default || tag == "button",
            Self::Table => matches!(tag, "html" | "table" | "template"),
        }
    }

    /// The scope an end tag `</tag>` looks for its element in.
    fn of_end_tag(tag: &str) -> Self {
        match tag {
            "li" => Self::ListItem,
            "p" => Self::Button,
            "td" | "th" | "tr" | "tbody" | "thead" | "tfoot" | "table" | "caption" | "colgroup" => {
                Self::Table
            }
            _ => Self::Default,
        }
    }
}

/// `name` lower-cased, borrowed when it already is.
fn lower(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

/// An element the builder has open.
#[derive(Debug, Clone)]
struct OpenElement {
    /// Its name, lower-cased (the rules compare these).
    key: String,
    /// Which element it is (the list of formatting elements refers to it).
    id: u32,
    /// Its end tag came while a block inside it was open
    /// (`<b><p>x</b>`): it closes as soon as it is the current node again.
    pending_close: bool,
}

/// An entry of the list of formatting elements (HTML only).
#[derive(Debug, Clone)]
enum Formatting {
    /// A cell (or caption ...) opened: nothing before it is reopened in it.
    Marker,
    /// A formatting element: reopened, with these attributes, wherever text
    /// or inline content comes while it is not open.
    Element {
        id: u32,
        key: String,
        attributes: Vec<(String, String)>,
    },
}

/// The tree construction (see the module docs): fed the tokens of a
/// document in order (`start_tag`, `end_tag`, `text` ...), it opens and
/// closes the elements of the tree on a [`TreeSink`], balanced.
#[derive(Debug)]
pub struct TreeBuilder {
    rules: TreeRules,
    stack: Vec<OpenElement>,
    active: Vec<Formatting>,
    next_id: u32,
    /// The next text's first line feed is dropped (after `<pre>`).
    skip_newline: bool,
    /// HTML: a `<head>` was opened (or can no longer be).
    head_seen: bool,
    /// HTML: the `<body>` is open.
    body_open: bool,
    /// The text since the last element opened or closed: a browser's text
    /// node is one run, also across a comment or a tag it ignores
    /// (`a<!-- x -->b`, `a</font>b` with no `<font>` open).
    pending_text: String,
    /// How many `<p>` / `<li>` `<dd>` `<dt>` are open: a block start looks
    /// for one only when there is one (ten thousand nested `<div>`s are not
    /// searched ten thousand times).
    open_paragraphs: usize,
    open_list_items: usize,
}

impl TreeBuilder {
    /// A builder that applies `rules`.
    #[must_use]
    pub const fn new(rules: TreeRules) -> Self {
        Self {
            rules,
            stack: Vec::new(),
            active: Vec::new(),
            next_id: 0,
            skip_newline: false,
            head_seen: false,
            body_open: false,
            pending_text: String::new(),
            open_paragraphs: 0,
            open_list_items: 0,
        }
    }

    /// How many elements are open.
    #[must_use]
    pub const fn open_elements(&self) -> usize {
        self.stack.len()
    }

    fn current(&self) -> Option<&str> {
        self.stack.last().map(|e| e.key.as_str())
    }

    fn in_document(&self) -> bool {
        self.stack.first().is_some_and(|e| e.key == "html")
    }

    fn in_foreign_content(&self) -> bool {
        for e in self.stack.iter().rev() {
            match e.key.as_str() {
                "foreignobject" => return false,
                "svg" | "math" => return true,
                _ => {}
            }
        }
        false
    }

    fn find_in_scope(&self, key: &str, scope: Scope) -> Option<usize> {
        for (i, e) in self.stack.iter().enumerate().rev() {
            if e.key == key {
                return Some(i);
            }
            if scope.is_boundary(&e.key) {
                return None;
            }
        }
        None
    }

    /// Hand the pending text to the sink.
    fn flush_text(&mut self, sink: &mut dyn TreeSink) {
        if !self.pending_text.is_empty() {
            sink.text(&self.pending_text);
            self.pending_text.clear();
        }
    }

    /// Open an element on the sink (the pending text goes first).
    fn sink_open(&mut self, sink: &mut dyn TreeSink, name: &str, attributes: &[(String, String)]) {
        self.flush_text(sink);
        sink.open_element(name, attributes);
    }

    /// Close the sink's open element (the pending text goes first).
    fn sink_close(&mut self, sink: &mut dyn TreeSink) {
        self.flush_text(sink);
        sink.close_element();
    }

    /// Open an element on the sink and the stack.
    fn push(&mut self, sink: &mut dyn TreeSink, name: &str, key: String, attributes: &[(String, String)]) -> u32 {
        self.sink_open(sink, name, attributes);
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.count_open(&key, true);
        self.stack.push(OpenElement {
            key,
            id,
            pending_close: false,
        });
        id
    }

    /// Close the element at `index` and every element above it; then every
    /// element whose end tag is pending and that is now the current node.
    fn pop_to(&mut self, sink: &mut dyn TreeSink, index: usize) {
        while self.stack.len() > index {
            self.pop_one(sink);
        }
        while self.stack.last().is_some_and(|e| e.pending_close) {
            self.pop_one(sink);
        }
    }

    fn pop_one(&mut self, sink: &mut dyn TreeSink) {
        let Some(e) = self.stack.pop() else {
            return;
        };
        self.count_open(&e.key, false);
        self.sink_close(sink);
        if self.rules.html() && is_marker_element(&e.key) {
            // A cell closed: what was opened in it is not reopened outside.
            while let Some(f) = self.active.pop() {
                if matches!(f, Formatting::Marker) {
                    break;
                }
            }
        }
    }

    fn count_open(&mut self, key: &str, opened: bool) {
        let counter = match key {
            "p" => &mut self.open_paragraphs,
            "li" | "dd" | "dt" => &mut self.open_list_items,
            _ => return,
        };
        if opened {
            *counter += 1;
        } else {
            *counter = counter.saturating_sub(1);
        }
    }

    fn close_p_in_button_scope(&mut self, sink: &mut dyn TreeSink) {
        if self.open_paragraphs == 0 {
            return;
        }
        if let Some(i) = self.find_in_scope("p", Scope::Button) {
            self.pop_to(sink, i);
        }
    }

    /// An `<li>` closes the open `<li>` (a `<dd>` / `<dt>` the open `<dd>`
    /// or `<dt>`) - not one of an outer list.
    fn close_list_item(&mut self, sink: &mut dyn TreeSink, names: &[&str]) {
        if self.open_list_items == 0 {
            return;
        }
        for i in (0..self.stack.len()).rev() {
            let key = self.stack[i].key.as_str();
            if names.contains(&key) {
                self.pop_to(sink, i);
                return;
            }
            if is_special(key) && !matches!(key, "address" | "div" | "p") {
                return;
            }
        }
    }

    /// Close the open elements named `names` of the current table (and what
    /// is open inside them): the cell before a new cell, the cell and the
    /// row before a new row.
    fn close_in_table(&mut self, sink: &mut dyn TreeSink, names: &[&str]) {
        let mut lowest = None;
        for i in (0..self.stack.len()).rev() {
            let key = self.stack[i].key.as_str();
            if Scope::Table.is_boundary(key) {
                break;
            }
            if names.contains(&key) {
                lowest = Some(i);
            }
        }
        if let Some(i) = lowest {
            self.pop_to(sink, i);
        }
    }

    /// The end tags a start tag implies.
    fn close_implied_by(&mut self, sink: &mut dyn TreeSink, key: &str) {
        match key {
            "li" => {
                self.close_list_item(sink, &["li"]);
                self.close_p_in_button_scope(sink);
            }
            "dd" | "dt" => {
                self.close_list_item(sink, &["dd", "dt"]);
                self.close_p_in_button_scope(sink);
            }
            "option" => {
                if self.current() == Some("option") {
                    self.pop_to(sink, self.stack.len() - 1);
                }
            }
            "optgroup" => {
                if self.current() == Some("option") {
                    self.pop_to(sink, self.stack.len() - 1);
                }
                if self.current() == Some("optgroup") {
                    self.pop_to(sink, self.stack.len() - 1);
                }
            }
            "tr" => self.close_in_table(sink, &["td", "th", "tr"]),
            "td" | "th" => self.close_in_table(sink, &["td", "th"]),
            "thead" | "tbody" | "tfoot" | "caption" | "colgroup" => self.close_in_table(
                sink,
                &[
                    "td", "th", "tr", "thead", "tbody", "tfoot", "caption", "colgroup",
                ],
            ),
            k if is_heading(k) => {
                self.close_p_in_button_scope(sink);
                if self.current().is_some_and(is_heading) {
                    self.pop_to(sink, self.stack.len() - 1);
                }
            }
            k if closes_p(k) => self.close_p_in_button_scope(sink),
            _ => {}
        }
    }

    // ---- HTML only: the document, the table, the formatting elements ----

    fn ensure_html(&mut self, sink: &mut dyn TreeSink) {
        if self.stack.is_empty() && !self.head_seen && !self.body_open {
            let _ = self.push(sink, "html", String::from("html"), &[]);
        }
    }

    fn close_head(&mut self, sink: &mut dyn TreeSink) {
        if let Some(i) = self.stack.iter().position(|e| e.key == "head") {
            self.pop_to(sink, i);
        }
    }

    /// Open the implied `<body>` of a document (closing its head first).
    fn open_body(&mut self, sink: &mut dyn TreeSink) {
        if self.body_open || !self.in_document() {
            return;
        }
        self.close_head(sink);
        if self.stack.len() == 1 {
            self.head_seen = true;
            self.body_open = true;
            let _ = self.push(sink, "body", String::from("body"), &[]);
        }
    }

    /// `<html>`, `<head>`, `<body>`: `true` if the start tag was taken care
    /// of (opened where a document has them, or dropped as a duplicate).
    fn document_start_tag(
        &mut self,
        sink: &mut dyn TreeSink,
        key: &str,
        attributes: &[(String, String)],
    ) -> bool {
        let at_top = self
            .stack
            .iter()
            .all(|e| e.key == "html" || e.key == "head");
        match key {
            "html" => {
                if self.stack.is_empty() && !self.head_seen && !self.body_open {
                    let _ = self.push(sink, "html", String::from("html"), attributes);
                }
                true
            }
            "head" => {
                if !self.head_seen && !self.body_open && (self.stack.is_empty() || self.current() == Some("html")) {
                    self.ensure_html(sink);
                    self.head_seen = true;
                    let _ = self.push(sink, "head", String::from("head"), attributes);
                }
                true
            }
            "body" => {
                if !self.body_open && at_top {
                    self.ensure_html(sink);
                    self.close_head(sink);
                    self.head_seen = true;
                    self.body_open = true;
                    let _ = self.push(sink, "body", String::from("body"), attributes);
                }
                true
            }
            _ => false,
        }
    }

    /// Before a start tag in a document whose body is not open yet: the
    /// head's elements go into the (implied) head, everything else opens the
    /// body.
    fn before_body_content(&mut self, sink: &mut dyn TreeSink, key: &str) {
        if self.body_open || !self.in_document() {
            return;
        }
        if is_head_content(key) {
            if self.current() == Some("html") && !self.head_seen {
                self.head_seen = true;
                let _ = self.push(sink, "head", String::from("head"), &[]);
            } else if !self.stack.iter().any(|e| e.key == "head") {
                // After the head: in the body, where a stylesheet still
                // applies.
                self.open_body(sink);
            }
            return;
        }
        self.open_body(sink);
    }

    /// The `<tbody>` and `<tr>` a row or a cell needs.
    fn open_implied_table_parents(&mut self, sink: &mut dyn TreeSink, key: &str) {
        match key {
            "tr" => {
                if self.current() == Some("table") {
                    let _ = self.push(sink, "tbody", String::from("tbody"), &[]);
                }
            }
            "td" | "th" => {
                if self.current() == Some("table") {
                    let _ = self.push(sink, "tbody", String::from("tbody"), &[]);
                }
                if matches!(self.current(), Some("tbody" | "thead" | "tfoot")) {
                    let _ = self.push(sink, "tr", String::from("tr"), &[]);
                }
            }
            _ => {}
        }
    }

    fn last_marker_end(&self) -> usize {
        self.active
            .iter()
            .rposition(|f| matches!(f, Formatting::Marker))
            .map_or(0, |m| m + 1)
    }

    fn is_open(&self, id: u32) -> bool {
        self.stack.iter().any(|e| e.id == id)
    }

    /// Reopen the formatting elements that were closed by a block's start
    /// (the HTML Standard's "reconstruct the active formatting elements").
    fn reopen_formatting(&mut self, sink: &mut dyn TreeSink) {
        if self.current().is_some_and(|c| is_table_structure(c) || holds_raw_text(c)) {
            return;
        }
        let start = self.last_marker_end();
        let mut first = self.active.len();
        while first > start {
            match &self.active[first - 1] {
                Formatting::Element { id, .. } if !self.is_open(*id) => first -= 1,
                _ => break,
            }
        }
        for index in first..self.active.len() {
            if self.stack.len() >= MAX_XML_NESTING_DEPTH {
                return;
            }
            let (key, attributes) = match &self.active[index] {
                Formatting::Element {
                    key, attributes, ..
                } => (key.clone(), attributes.clone()),
                Formatting::Marker => continue,
            };
            let new_id = self.push(sink, &key, key.clone(), &attributes);
            if let Some(Formatting::Element { id, .. }) = self.active.get_mut(index) {
                *id = new_id;
            }
        }
    }

    /// Put a formatting element on the list; at most three equal ones after
    /// the last marker (the HTML Standard's "Noah's Ark" clause), so a
    /// thousand unclosed `<font>`s are not reopened a thousand times.
    fn remember_formatting(&mut self, id: u32, key: &str, attributes: Vec<(String, String)>) {
        let start = self.last_marker_end();
        let equal: Vec<usize> = self.active[start..]
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                matches!(f, Formatting::Element { key: k, attributes: a, .. } if k == key && *a == attributes)
            })
            .map(|(i, _)| start + i)
            .collect();
        if equal.len() >= 3 {
            let _ = self.active.remove(equal[0]);
        }
        self.active.push(Formatting::Element {
            id,
            key: String::from(key),
            attributes,
        });
    }

    fn forget_formatting(&mut self, id: u32) {
        if let Some(i) = self
            .active
            .iter()
            .rposition(|f| matches!(f, Formatting::Element { id: e, .. } if *e == id))
        {
            let _ = self.active.remove(i);
        }
    }

    // ---- the tokens ----

    /// A start tag `<name ...>` (`self_closing`: `<name ... />`).
    pub fn start_tag(
        &mut self,
        sink: &mut dyn TreeSink,
        name: &str,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    ) {
        self.skip_newline = false;
        let name: Cow<'_, str> = if self.rules.folds_case() {
            lower(name)
        } else {
            Cow::Borrowed(name)
        };
        let key = lower(&name).into_owned();
        let html = self.rules.html();
        if html {
            if self.document_start_tag(sink, &key, &attributes) {
                return;
            }
            // Markup without `<html>` is a document all the same (a
            // fragment, a mail's body): its `<html>`, `<head>` and `<body>`
            // are implied, as a browser implies them.
            self.ensure_html(sink);
            self.before_body_content(sink, &key);
            if is_table_part(&key) && !self.stack.iter().any(|e| e.key == "table") {
                // A cell, a row ... outside any table: ignored (its content
                // goes where it stands), as in a browser.
                return;
            }
        }
        self.close_implied_by(sink, &key);
        if html {
            if key == "a" {
                // A link does not nest in a link.
                let start = self.last_marker_end();
                if self.active[start..]
                    .iter()
                    .any(|f| matches!(f, Formatting::Element { key: k, .. } if k == "a"))
                {
                    self.end_tag(sink, "a");
                }
            }
            if reopens_formatting_before(&key) {
                self.reopen_formatting(sink);
            }
            self.open_implied_table_parents(sink, &key);
        }
        if is_void_element(&key) || (html && self.stack.len() >= MAX_XML_NESTING_DEPTH) {
            // No content: open and closed at once (past the depth limit, an
            // element's content goes to its parent, as in a browser).
            self.sink_open(sink, &name, &attributes);
            self.sink_close(sink);
            return;
        }
        let empty = self_closing
            && (!html || key == "svg" || key == "math" || self.in_foreign_content());
        if empty {
            self.sink_open(sink, &name, &attributes);
            self.sink_close(sink);
            return;
        }
        let id = self.push(sink, &name, key.clone(), &attributes);
        if html {
            if is_formatting(&key) {
                self.remember_formatting(id, &key, attributes);
            } else if is_marker_element(&key) {
                self.active.push(Formatting::Marker);
            }
        }
        // HTML drops a line feed right after `<pre>` (XML keeps it).
        self.skip_newline = html && matches!(key.as_str(), "pre" | "listing" | "textarea");
    }

    /// An end tag `</name>`.
    pub fn end_tag(&mut self, sink: &mut dyn TreeSink, name: &str) {
        self.skip_newline = false;
        let key = lower(name);
        let key: &str = &key;
        let html = self.rules.html();
        if is_void_element(key) {
            if html && key == "br" {
                // `</br>` is a line break in HTML.
                self.start_tag(sink, "br", Vec::new(), false);
            }
            return;
        }
        if html {
            match key {
                // Closed at the end: content after them still goes in.
                "html" | "body" => return,
                "head" => {
                    if self.current() == Some("head") {
                        self.pop_to(sink, self.stack.len() - 1);
                    }
                    return;
                }
                "p" if self.find_in_scope("p", Scope::Button).is_none() => {
                    // `</p>` without a `<p>` is an empty paragraph.
                    self.before_body_content(sink, "p");
                    self.sink_open(sink, "p", &[]);
                    self.sink_close(sink);
                    return;
                }
                _ => {}
            }
        }
        let Some(i) = self.find_in_scope(key, Scope::of_end_tag(key)) else {
            return;
        };
        let id = self.stack[i].id;
        if is_formatting(key) && self.stack[i + 1..].iter().any(|e| is_special(&e.key)) {
            // `<b><p>x</b>y</p>`: the paragraph stays open; the `<b>`
            // closes when the paragraph does.
            self.stack[i].pending_close = true;
            if html {
                self.forget_formatting(id);
            }
            return;
        }
        self.pop_to(sink, i);
        if html && is_formatting(key) {
            self.forget_formatting(id);
        }
    }

    /// Text (already decoded).
    pub fn text(&mut self, sink: &mut dyn TreeSink, text: &str) {
        let mut text = text;
        if core::mem::take(&mut self.skip_newline) {
            text = text.strip_prefix('\n').unwrap_or(text);
        }
        if text.is_empty() {
            return;
        }
        if self.rules.html() {
            if self.stack.is_empty() {
                // White space before the document is nothing; anything else
                // starts it (its body).
                if text.bytes().all(is_html_space) {
                    return;
                }
                self.ensure_html(sink);
            }
            let raw = self.current().is_some_and(holds_raw_text);
            if !raw {
                if !self.body_open
                    && self.in_document()
                    && !text.bytes().all(is_html_space)
                {
                    self.open_body(sink);
                }
                if self.body_open || !self.in_document() {
                    self.reopen_formatting(sink);
                }
            }
        }
        self.pending_text.push_str(text);
    }

    /// A comment: dropped, except inside a `<style>` (`<style><!-- .. --></style>`,
    /// how Outlook writes every stylesheet), where it is part of the sheet.
    pub fn comment(&mut self, _sink: &mut dyn TreeSink, text: &str) {
        self.skip_newline = false;
        if self.current() == Some("style") {
            self.pending_text.push_str("<!--");
            self.pending_text.push_str(text);
            self.pending_text.push_str("-->");
        }
    }

    /// A CDATA section: its text inside a `<style>`, else dropped.
    pub fn cdata(&mut self, _sink: &mut dyn TreeSink, text: &str) {
        self.skip_newline = false;
        if self.current() == Some("style") {
            self.pending_text.push_str(text);
        }
    }

    /// The end of the input: every open element closes. The number of
    /// elements that were open (under [`TreeRules::Html`] the `<html>` and
    /// `<body>` a document always leaves open count too).
    pub fn finish(mut self, sink: &mut dyn TreeSink) -> usize {
        if self.rules.html() && self.in_document() && !self.body_open {
            self.open_body(sink);
        }
        self.flush_text(sink);
        let open = self.stack.len();
        while self.stack.pop().is_some() {
            sink.close_element();
        }
        open
    }
}

// ============================================================================
// The XmlNode tree
// ============================================================================

/// A [`TreeSink`] that builds the [`XmlNode`] tree (the tree loader's and
/// the lenient tree loader's output).
#[derive(Debug, Default)]
pub struct XmlTreeSink {
    roots: Vec<XmlNodeChild>,
    stack: Vec<XmlNode>,
}

impl XmlTreeSink {
    /// An empty tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The root nodes (every element still open closed).
    #[must_use]
    pub fn finish(mut self) -> Vec<XmlNodeChild> {
        while !self.stack.is_empty() {
            self.close_element();
        }
        self.roots
    }

    fn append(&mut self, child: XmlNodeChild) {
        match self.stack.last_mut() {
            Some(top) => top.children.push(child),
            None => self.roots.push(child),
        }
    }
}

impl TreeSink for XmlTreeSink {
    fn open_element(&mut self, name: &str, attributes: &[(String, String)]) {
        let mut node = XmlNode::create(name);
        for (key, value) in attributes {
            node.attributes.push(AzStringPair {
                key: AzString::from(key.as_str()),
                value: AzString::from(value.as_str()),
            });
        }
        self.stack.push(node);
    }

    fn close_element(&mut self) {
        if let Some(node) = self.stack.pop() {
            self.append(XmlNodeChild::Element(node));
        }
    }

    fn text(&mut self, text: &str) {
        self.append(XmlNodeChild::Text(AzString::from(text)));
    }
}

/// Feed `html` through the lenient tokenizer and the HTML tree construction
/// into `sink`.
pub fn parse_html_into(html: &str, sink: &mut dyn TreeSink) {
    let mut builder = TreeBuilder::new(TreeRules::Html);
    for token in HtmlTokenizer::new(html) {
        match token {
            HtmlToken::StartTag {
                name,
                attributes,
                self_closing,
            } => builder.start_tag(sink, &name, attributes, self_closing),
            HtmlToken::EndTag { name } => builder.end_tag(sink, &name),
            HtmlToken::Text(text) => builder.text(sink, &text),
            HtmlToken::Comment(text) => builder.comment(sink, text),
            HtmlToken::Cdata(text) => builder.cdata(sink, text),
        }
    }
    let _ = builder.finish(sink);
}

/// HTML as a browser reads it, as an [`XmlNode`] tree: the lenient loader.
/// Never fails - whatever the markup, this is the tree a browser would
/// build from it (see the module docs for what is simplified).
#[must_use]
pub fn parse_html_nodes(html: &str) -> Vec<XmlNodeChild> {
    let mut sink = XmlTreeSink::new();
    parse_html_into(html, &mut sink);
    sink.finish()
}

// ============================================================================
// Outline
// ============================================================================

/// A tree as one line, for tests and logs.
///
/// An element is its name, then `[name=value ...]` (with
/// `with_attributes`), then `{children}`; a text is a JSON string with its
/// white space runs collapsed to one space; text that is only white space is
/// left out: `html{head body{p{"Hi " b{"you"}}}}`.
#[must_use]
pub fn outline(nodes: &[XmlNodeChild], with_attributes: bool) -> String {
    let mut out = String::new();
    outline_into(nodes, with_attributes, &mut out);
    out
}

fn outline_into(nodes: &[XmlNodeChild], with_attributes: bool, out: &mut String) {
    for node in nodes {
        match node {
            XmlNodeChild::Text(text) => {
                let text = text.as_str();
                if text.bytes().all(is_html_space) {
                    continue;
                }
                if !out.is_empty() && !out.ends_with('{') {
                    out.push(' ');
                }
                push_json_string(out, text);
            }
            XmlNodeChild::Element(element) => {
                if !out.is_empty() && !out.ends_with('{') {
                    out.push(' ');
                }
                out.push_str(element.node_type.as_str());
                let attributes = element.attributes.as_ref();
                if with_attributes && !attributes.is_empty() {
                    out.push('[');
                    for (i, pair) in attributes.iter().enumerate() {
                        if i > 0 {
                            out.push(' ');
                        }
                        out.push_str(pair.key.as_str());
                        out.push('=');
                        out.push_str(pair.value.as_str());
                    }
                    out.push(']');
                }
                let mut inner = String::new();
                outline_into(element.children.as_ref(), with_attributes, &mut inner);
                if !inner.is_empty() {
                    out.push('{');
                    out.push_str(&inner);
                    out.push('}');
                }
            }
        }
    }
}

/// `text` as a JSON string (as JavaScript's `JSON.stringify` writes it),
/// HTML white space runs collapsed to one space.
fn push_json_string(out: &mut String, text: &str) {
    out.push('"');
    let mut in_space = false;
    for c in text.chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{C}') {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
            continue;
        }
        in_space = false;
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            c if u32::from(c) < 0x20 => {
                let _ = core::fmt::Write::write_fmt(out, format_args!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
#[path = "xml_html_test.rs"]
mod tests;
