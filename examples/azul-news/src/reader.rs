//! Articles through azul's HTML5-like parser (`Xml::create_from_html`, the lenient tokenizer
//! and the browser-like tree construction every azul loader shares).
//!
//! - [`plain_text`] / [`excerpt`]: the text of a piece of HTML (a title that holds `&amp;` or
//!   `<code>`, a description for the list's two lines), its character references decoded by
//!   the parser - the engine's one table of HTML's names.

use azul::{dom::XmlNodeChild, xml::Xml};

/// Elements whose text is never shown.
const HIDDEN: &[&str] = &["script", "style", "noscript", "template", "head", "title"];

/// Elements that separate their text from the text around them (a space in plain text).
const BLOCKS: &[&str] = &[
    "p",
    "div",
    "br",
    "li",
    "ul",
    "ol",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
    "tr",
    "td",
    "th",
    "table",
    "section",
    "article",
    "header",
    "footer",
    "figure",
    "figcaption",
    "hr",
    "dd",
    "dt",
    "dl",
    "main",
    "aside",
    "nav",
    "address",
    "details",
    "summary",
    "caption",
];

/// Deeper than this, an article's elements are not walked (the parser stops at 512).
const MAX_DEPTH: usize = 256;

/// Every run of white space (a no-break space too) one space; trimmed.
#[must_use]
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text of `html`: references decoded, scripts and styles left out, blocks apart, every
/// run of white space (a no-break space too) one space, trimmed.
#[must_use]
pub fn plain_text(html: &str) -> String {
    if !html.contains('<') && !html.contains('&') {
        return collapse(html);
    }
    let document = Xml::create_from_html(html);
    let mut out = String::new();
    text_of(&document.root, &mut out, 0);
    collapse(&out)
}

fn text_of(nodes: &[XmlNodeChild], out: &mut String, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for node in nodes {
        match node {
            XmlNodeChild::Text(text) => out.push_str(text.as_str()),
            XmlNodeChild::Element(element) => {
                let name = element.node_type.inner.as_str();
                if HIDDEN.contains(&name) {
                    continue;
                }
                let block = BLOCKS.contains(&name);
                if block {
                    out.push(' ');
                }
                text_of(&element.children, out, depth + 1);
                if block {
                    out.push(' ');
                }
            }
        }
    }
}

/// [`plain_text`] cut to at most `max_chars` characters at a word's end, with `…` when cut.
#[must_use]
pub fn excerpt(html: &str, max_chars: usize) -> String {
    cut_at_word(&plain_text(html), max_chars)
}

/// `text` cut to at most `max_chars` characters at a word's end (inside one long word when it
/// is all there is), with `…` when cut.
#[must_use]
pub fn cut_at_word(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    let at_word_end = text
        .chars()
        .nth(max_chars)
        .map_or(true, char::is_whitespace);
    let kept = if at_word_end {
        head.trim_end().to_string()
    } else {
        match head.rfind(' ') {
            Some(at) if at > 0 => head[..at].trim_end().to_string(),
            _ => head,
        }
    };
    format!("{kept}\u{2026}")
}

/// Plain text as HTML: escaped by azul's one encoder (`Xml::encode_text`), a line break a
/// `<br/>`, and - when there are several paragraphs (blank lines between them) - each in a `<p>`.
#[must_use]
pub fn text_to_html(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let paragraphs: Vec<String> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.lines()
                .map(|line| Xml::encode_text(line.trim()).as_str().to_string())
                .collect::<Vec<_>>()
                .join("<br/>")
        })
        .collect();
    match paragraphs.len() {
        0 => String::new(),
        1 => paragraphs.into_iter().next().unwrap_or_default(),
        _ => paragraphs
            .into_iter()
            .map(|p| format!("<p>{p}</p>"))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_decodes_references_and_keeps_blocks_apart() {
        assert_eq!(
            plain_text("<p>Hello <b>world</b></p><p>Second</p>"),
            "Hello world Second"
        );
        assert_eq!(
            plain_text("Tom &amp; Jerry&rsquo;s &nbsp; caf&eacute;"),
            "Tom & Jerry\u{2019}s caf\u{e9}"
        );
        assert_eq!(plain_text("a<br>b<script>x()</script>c"), "a bc");
        assert_eq!(
            plain_text("<style>p { color: red }</style>  spaced \n\t out  "),
            "spaced out"
        );
        assert_eq!(
            plain_text("x < y & z"),
            "x < y & z",
            "text that only looks like markup"
        );
        assert_eq!(
            plain_text("Why <code>Option</code> matters"),
            "Why Option matters"
        );
        assert_eq!(plain_text(""), "");
    }

    #[test]
    fn an_excerpt_ends_at_a_word() {
        assert_eq!(
            excerpt("<p>one two three four</p>", 100),
            "one two three four"
        );
        assert_eq!(excerpt("<p>one two three four</p>", 9), "one two\u{2026}");
        assert_eq!(
            excerpt("<p>abcdefghijkl</p>", 5),
            "abcde\u{2026}",
            "one long word is cut inside"
        );
    }
}
