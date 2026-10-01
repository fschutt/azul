//! HTML as a browser reads it, and the ONE tree construction every XML
//! loader shares.
//!
//! RED: the lenient loader's surface with nothing behind it yet (the
//! tokenizer and the tree construction land with the fix).

use alloc::{borrow::Cow, string::String, vec::Vec};

use super::{XmlNode, XmlNodeChild, MAX_XML_NESTING_DEPTH};

/// How [`decode_character_references`] reads a `&...` reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharRefMode {
    /// XML's rules with HTML's names.
    Xml,
    /// HTML's rules in text.
    HtmlText,
    /// HTML's rules in an attribute value.
    HtmlAttribute,
}

/// The characters a named character reference stands for (RED: none).
#[must_use]
pub fn named_character_reference(_name: &str, _with_semicolon: bool) -> Option<&'static str> {
    None
}

/// Decode the character references in `s` (RED: nothing is decoded).
#[must_use]
pub fn decode_character_references(s: &str, _mode: CharRefMode) -> Cow<'_, str> {
    Cow::Borrowed(s)
}

/// HTML's white space: tab, line feed, form feed, carriage return, space.
const fn is_html_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

/// HTML as a browser reads it, as an [`XmlNode`] tree (RED: nothing).
#[must_use]
pub fn parse_html_nodes(_html: &str) -> Vec<XmlNodeChild> {
    Vec::new()
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
