//! HTML as a browser reads it, and the ONE tree construction every XML
//! loader shares - the HTML Living Standard's parser (13.2), its special cases as data.
//!
//! The pieces, each used by more than one loader:
//!
//! - [`HtmlTokenizer`] (`xml_html_tokenizer.rs`): the LENIENT tokenizer for HTML content (a
//!   mail, a paste from a browser or Word), the standard's tokenization (13.2.5) as a state
//!   machine. It never fails: unquoted and bare (boolean) attributes, upper-case names, `--`
//!   inside a comment, `<` and `&` in text and attribute values, Word's `<![if ...]>`, the raw
//!   text of `<style>` / `<script>`, RCDATA `<title>` / `<textarea>`, `<plaintext>`, doctypes
//!   (quirks mode), the HTML Standard's named character references; stray control characters
//!   are dropped. The strict loaders keep `xmlparser` as their tokenizer (an XML syntax error
//!   is an error there).
//! - The rules (`xml_html_rules.rs`): every special case of the tree construction as a row of
//!   a table citing the standard - the elements' categories, the body's start and end tags,
//!   the table insertion modes, the quirks doctypes. A new rule is a new row.
//! - [`TreeBuilder`] (`xml_html_tree.rs`): the tree construction (13.2.6). Under
//!   [`TreeRules::Html`] what a browser builds: the implied `<html>` / `<head>` / `<body>`,
//!   implied end tags, the table modes (implied `<tbody>` / `<tr>` / `<colgroup>`, foster
//!   parenting of what is misplaced in a table), the adoption agency for misnested formatting
//!   elements, the formatting elements reopened in the next block, SVG / `MathML` content and
//!   where HTML breaks out of it, quirks mode. Under [`TreeRules::Xml`] / `XmlFolded` (the
//!   strict loaders: the tree loader `azul_layout::xml::parse_xml_string`, the document
//!   loader `azul_layout::xml::parse_xml_to_fast_dom`) the XML conveniences of the same rule
//!   rows: void elements, implied end tags, end tags matched within their scope - so the
//!   loaders can no longer build two different trees from one document.
//! - [`XmlTreeSink`]: the tree construction's output as an [`XmlNode`] tree.
//!
//! What a browser does that this does not (see `xml_html_tree.rs`): comments are not nodes,
//! `select` / `template` content is body content, `frameset`, SVG's camel-case names, script
//! data's escapes.

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

/// `s` as the text content of an HTML / XML element.
///
/// `&`, `<` and `>` as references (quotes are plain text between tags), the
/// characters XML 1.0 cannot carry left out (see [`encode_attribute`]). The inverse of
/// [`decode_character_references`] in every [`CharRefMode`] - THE encoder
/// for every writer of markup (clipboard HTML, the e2e builder, toasts,
/// the web renderer).
#[must_use]
pub fn encode_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    encode_into(&mut out, s, false);
    out
}

/// `s` as an attribute value.
///
/// [`encode_text`] plus both quotes (`&quot;`, `&apos;`), so it is safe
/// between `"..."` and `'...'` alike. Left out, as
/// in text: the C0 controls other than tab, line feed and carriage return,
/// and U+FFFE / U+FFFF - XML 1.0 has no way to write them, not even as a
/// reference, and a strict reader rejects the document that has them.
#[must_use]
pub fn encode_attribute(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    encode_into(&mut out, s, true);
    out
}

/// [`encode_text`] / [`encode_attribute`] (`quotes`) appended to `out`.
fn encode_into(out: &mut String, s: &str, quotes: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quotes => out.push_str("&quot;"),
            '\'' if quotes => out.push_str("&apos;"),
            // XML 1.0 cannot carry these, not even as a reference (tab, line
            // feed and carriage return - 0x09, 0x0A, 0x0D - it can).
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => {}
            c => out.push(c),
        }
    }
}

// ============================================================================
// Text as the tokenizer reads it
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

// ============================================================================
// The tokenizer, the rules, the tree construction
// ============================================================================

/// The tree construction's rules as data (the HTML Living Standard's 13.2.6 as tables).
#[path = "xml_html_rules.rs"]
mod rules;

/// The tokenizer (13.2.5) as a state machine.
#[path = "xml_html_tokenizer.rs"]
mod tokenizer;

/// The tree construction (13.2.6): HTML's, and the strict loaders' XML conveniences.
#[path = "xml_html_tree.rs"]
mod tree;

pub use rules::{is_void_element, start_tag_closes};
pub use tokenizer::{Doctype, HtmlToken, HtmlTokenizer, TextMode};
pub use tree::{TreeBuilder, TreeRules};

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
///
/// The tree construction drives the tokenizer, as in the standard: after
/// a start tag it says how the element's content is read (raw text for an HTML
/// `<style>`, markup for an SVG one), and whether a CDATA section is one.
pub fn parse_html_into(html: &str, sink: &mut dyn TreeSink) {
    let mut builder = TreeBuilder::new(TreeRules::Html);
    let mut tokenizer = HtmlTokenizer::new(html);
    while let Some(token) = tokenizer.next() {
        match token {
            HtmlToken::StartTag {
                name,
                attributes,
                self_closing,
            } => {
                let mode = builder.text_mode_for(&name);
                builder.start_tag(sink, &name, attributes, self_closing);
                tokenizer.set_text_mode(mode);
            }
            HtmlToken::EndTag { name } => builder.end_tag(sink, &name),
            HtmlToken::Text(text) => builder.text(sink, &text),
            HtmlToken::Comment(text) => builder.comment(sink, text),
            HtmlToken::Cdata(text) => builder.cdata(sink, text),
            HtmlToken::Doctype(doctype) => builder.doctype(&doctype),
        }
        tokenizer.set_cdata_allowed(builder.in_foreign_content());
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
