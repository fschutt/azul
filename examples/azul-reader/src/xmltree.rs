//! Reading a document parsed by azul's XML / HTML5-like parser: the one place AzReader
//! walks an `XmlNode` tree (the package, the navigation documents, the chapters).
//!
//! A tree from the strict loader (`Xml::from_str`) keeps its names as written (`dc:title`,
//! `opf:item`, `epub:type`); one from the HTML parser (`Xml::create_from_html`) has them in
//! lower case. Names are therefore compared case-insensitively, and an element's LOCAL name
//! (`title` for `dc:title`) is what the package and navigation readers match.

use azul::{
    dom::{XmlNode, XmlNodeChild},
    error::ResultXmlXmlError,
    xml::Xml,
};

/// How a document was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parsed {
    /// Well-formed XML (an XHTML chapter, the package, the NCX).
    Xml,
    /// The strict parse failed (or the document is HTML): the HTML5-like parser's tree.
    Html,
}

/// `text` as a tree: the strict XML loader first (XHTML is XML: `<a id="x"/>`, `<title/>`
/// are EMPTY elements there, where the HTML tree construction would open them and - for a
/// `<title/>`, raw text - swallow the rest of the document), the HTML5-like parser when that
/// fails. `html` = the document is HTML (an `.html` file, `text/html`): straight to the HTML
/// parser.
#[must_use]
pub fn parse_document(text: &str, html: bool) -> (Xml, Parsed) {
    if !html {
        if let ResultXmlXmlError::Ok(xml) = Xml::from_str(text) {
            if !xml.root.as_slice().is_empty() {
                return (xml, Parsed::Xml);
            }
        }
    }
    (Xml::create_from_html(text), Parsed::Html)
}

/// The part of a qualified name after its prefix, lower case: `dc:Title` -> `title`.
#[must_use]
pub fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

/// The element's local name, lower case.
#[must_use]
pub fn name(node: &XmlNode) -> String {
    local(node.node_type.inner.as_str())
}

/// The element's name as written, lower case (with its prefix: `epub:switch`).
#[must_use]
pub fn full_name(node: &XmlNode) -> String {
    node.node_type.inner.as_str().to_ascii_lowercase()
}

/// The value of attribute `key`: an exact (case-insensitive) match first, else - for a key
/// without a prefix - an attribute whose local name it is (`type` finds `epub:type`, `lang`
/// finds `xml:lang`).
#[must_use]
pub fn attr<'a>(node: &'a XmlNode, key: &str) -> Option<&'a str> {
    let pairs = node.attributes.inner.as_slice();
    if let Some(pair) = pairs
        .iter()
        .find(|p| p.key.as_str().eq_ignore_ascii_case(key))
    {
        return Some(pair.value.as_str());
    }
    if key.contains(':') {
        return None;
    }
    pairs
        .iter()
        .find(|p| local(p.key.as_str()) == key.to_ascii_lowercase())
        .map(|p| p.value.as_str())
}

/// Every attribute as `(name, value)`, names as written.
#[must_use]
pub fn attrs(node: &XmlNode) -> Vec<(&str, &str)> {
    node.attributes
        .inner
        .as_slice()
        .iter()
        .map(|p| (p.key.as_str(), p.value.as_str()))
        .collect()
}

/// The element children of `node`, in order.
pub fn elements(node: &XmlNode) -> impl Iterator<Item = &XmlNode> {
    node.children.as_slice().iter().filter_map(|c| match c {
        XmlNodeChild::Element(e) => Some(e),
        XmlNodeChild::Text(_) => None,
    })
}

/// The element children of `node` whose local name is `local_name`.
pub fn children_named<'a>(
    node: &'a XmlNode,
    local_name: &'a str,
) -> impl Iterator<Item = &'a XmlNode> {
    elements(node).filter(move |e| name(e) == local_name)
}

/// The first element child named `local_name`.
#[must_use]
pub fn child<'a>(node: &'a XmlNode, local_name: &str) -> Option<&'a XmlNode> {
    elements(node).find(|e| name(e) == local_name)
}

/// Every element named `local_name` in `nodes` and below, in document order.
#[must_use]
pub fn find_all<'a>(nodes: &'a [XmlNodeChild], local_name: &str) -> Vec<&'a XmlNode> {
    let mut out = Vec::new();
    let mut stack: Vec<&XmlNodeChild> = nodes.iter().rev().collect();
    while let Some(next) = stack.pop() {
        if let XmlNodeChild::Element(e) = next {
            if name(e) == local_name {
                out.push(e);
            }
            stack.extend(e.children.as_slice().iter().rev());
        }
    }
    out
}

/// The first element named `local_name` in `nodes` and below.
#[must_use]
pub fn find_first<'a>(nodes: &'a [XmlNodeChild], local_name: &str) -> Option<&'a XmlNode> {
    let mut stack: Vec<&XmlNodeChild> = nodes.iter().rev().collect();
    while let Some(next) = stack.pop() {
        if let XmlNodeChild::Element(e) = next {
            if name(e) == local_name {
                return Some(e);
            }
            stack.extend(e.children.as_slice().iter().rev());
        }
    }
    None
}

/// The text of `node` and everything in it, in document order, as written (no white space
/// folded).
#[must_use]
pub fn raw_text(node: &XmlNode) -> String {
    let mut out = String::new();
    let mut stack: Vec<&XmlNodeChild> = node.children.as_slice().iter().rev().collect();
    while let Some(next) = stack.pop() {
        match next {
            XmlNodeChild::Text(t) => out.push_str(t.as_str()),
            XmlNodeChild::Element(e) => stack.extend(e.children.as_slice().iter().rev()),
        }
    }
    out
}

/// [`raw_text`] with every run of white space as one blank, trimmed.
#[must_use]
pub fn text(node: &XmlNode) -> String {
    fold_space(&raw_text(node))
}

/// Every run of white space as one blank, trimmed.
#[must_use]
pub fn fold_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_document_is_read_as_xml_and_an_empty_element_stays_empty() {
        let (xml, how) = parse_document(
            "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title/></head>\
             <body><p><a id=\"p1\"/>One</p><p>Two</p></body></html>",
            false,
        );
        assert_eq!(how, Parsed::Xml);
        let anchor = find_first(xml.root.as_slice(), "a").expect("the anchor");
        assert!(
            anchor.children.as_slice().is_empty(),
            "<a/> is empty in XHTML"
        );
        let paragraphs = find_all(xml.root.as_slice(), "p");
        assert_eq!(paragraphs.len(), 2, "the empty <title/> swallowed nothing");
        assert_eq!(text(paragraphs[0]), "One");
        assert_eq!(text(paragraphs[1]), "Two");
    }

    #[test]
    fn a_malformed_document_falls_back_to_the_html_parser() {
        let (xml, how) = parse_document(
            "<div class=intro><p>One<p>Two &amp; three<br> four</div>",
            false,
        );
        assert_eq!(how, Parsed::Html, "an unquoted attribute is no XML");
        let paragraphs = find_all(xml.root.as_slice(), "p");
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(text(paragraphs[1]), "Two & three four");
        let (xml, how) = parse_document("<p>One<p>Two", true);
        assert_eq!(
            how,
            Parsed::Html,
            "an HTML document goes straight to the HTML parser"
        );
        assert_eq!(find_all(xml.root.as_slice(), "p").len(), 2);
    }

    #[test]
    fn names_and_attributes_match_without_case_and_prefix() {
        let (xml, _) = parse_document(
            "<package xmlns:dc=\"x\" xmlns:epub=\"y\"><metadata><dc:Title xml:lang=\"en\">A Book\
             </dc:Title></metadata><nav epub:type=\"toc\"/></package>",
            false,
        );
        let title = find_first(xml.root.as_slice(), "title").expect("dc:Title by its local name");
        assert_eq!(name(title), "title");
        assert_eq!(full_name(title), "dc:title");
        assert_eq!(
            attr(title, "lang"),
            Some("en"),
            "xml:lang by its local name"
        );
        let nav = find_first(xml.root.as_slice(), "nav").expect("nav");
        assert_eq!(attr(nav, "epub:type"), Some("toc"));
        assert_eq!(attr(nav, "type"), Some("toc"));
        assert_eq!(
            attr(nav, "other:type"),
            None,
            "a prefixed key matches exactly only"
        );
    }
}
