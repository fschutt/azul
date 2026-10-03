//! A feed's XML (RSS, Atom, OPML) read LENIENTLY into a small tree.
//!
//! Real feeds are often not well-formed XML: a bare `&` in a link, `&nbsp;` without a DTD, a
//! `<br>` that is never closed inside an unescaped description, a stray end tag, blank lines
//! before the XML declaration, a feed cut off by a timeout, a Windows-1252 feed that says
//! nothing. A strict parser (roxmltree, azul's own strict loaders) refuses the whole feed for
//! any of these; a reader shows what it can. So:
//!
//! - the bytes are decoded first ([`decode`]): a byte order mark wins; bytes that are valid
//!   UTF-8 are UTF-8 (whatever an old server's default `charset` or a misconfigured declaration
//!   says - real Latin-1 text is practically never valid UTF-8); other bytes are read in the XML
//!   declaration's `encoding`, else the HTTP `charset`, else as Windows-1252 (what an undeclared
//!   8-bit feed nearly always is) - never dropped;
//! - the tokens come from quick-xml 0.41 with its lenient switches (a dangling `&` is text, an
//!   unmatched end tag is no error, end names are not checked) and the TREE is built here: an
//!   end tag closes the nearest open element of its name (and everything opened inside it), an
//!   end tag that matches nothing is ignored, the elements still open when the input ends are
//!   closed - what was read stays;
//! - the five XML references and the numeric ones are decoded; any other name (`&nbsp;`,
//!   `&eacute;` - HTML's, used without a DTD) stays as written, so content that is HTML keeps it
//!   for the HTML parser, which knows every name;
//! - namespaces are resolved from the `xmlns` declarations in scope; a prefix nobody declared
//!   (`<media:thumbnail>` without `xmlns:media`) keeps its name and an empty namespace, and the
//!   feed reader matches it by its conventional prefix.
//!
//! What the repairs were is listed in [`Document::problems`] (for the feed's "broken" notice).

use quick_xml::{events::Event, reader::Reader};

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    /// Text with its references decoded (CDATA as written).
    Text(String),
}

/// One element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// The name as written (`content:encoded`).
    pub name: String,
    /// The namespace its prefix (or the default namespace) is bound to; empty when none is.
    pub namespace: String,
    /// The attributes as written, their values unescaped.
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
}

/// A parsed document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    /// The first top-level element (`None`: no element at all).
    pub root: Option<Element>,
    /// What was repaired, in order ("a stray </b> was ignored", "the input ends inside <item>").
    pub problems: Vec<String>,
}

/// The elements HTML never closes: written back as `<br/>` with what the lenient tree put
/// inside them after it ([`Element::inner_markup`]).
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

impl Element {
    /// The name after its prefix (`encoded` of `content:encoded`).
    #[must_use]
    pub fn local_name(&self) -> &str {
        self.name.rsplit_once(':').map_or(self.name.as_str(), |(_, local)| local)
    }

    /// The prefix of the name (`content` of `content:encoded`; empty without one).
    #[must_use]
    pub fn prefix(&self) -> &str {
        self.name.rsplit_once(':').map_or("", |(prefix, _)| prefix)
    }

    /// The attribute's value: by its exact name, else by its name in any case.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .or_else(|| self.attributes.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)))
            .map(|(_, v)| v.as_str())
    }

    /// The child elements, in order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// Whether any child is an element.
    #[must_use]
    pub fn has_element_children(&self) -> bool {
        self.elements().next().is_some()
    }

    /// Every piece of text inside, in order (child elements' text included).
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        collect_text(self, &mut out);
        out
    }

    /// The children written back as markup: text escaped (`&`, `<`, `>` - an `&` that starts a
    /// reference kept from the input, `&nbsp;`, stays), elements by their local names, a void
    /// element as `<br/>` followed by whatever the lenient tree put inside it.
    #[must_use]
    pub fn inner_markup(&self) -> String {
        let mut out = String::new();
        for child in &self.children {
            write_markup(child, &mut out);
        }
        out
    }
}

fn collect_text(element: &Element, out: &mut String) {
    for child in &element.children {
        match child {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) => collect_text(e, out),
        }
    }
}

fn write_markup(_node: &Node, _out: &mut String) {
    // RED: written back in the GREEN commit.
}

/// The text of a feed's bytes. `content_type` is the HTTP `Content-Type` (its `charset`), empty
/// when there is none (a file).
#[must_use]
pub fn decode(_bytes: &[u8], _content_type: &str) -> String {
    String::new()
}

/// Reads `text` into a tree; never fails (see the module documentation).
#[must_use]
pub fn parse(text: &str) -> Document {
    let _reader = Reader::from_str(text);
    let _unused: Option<Event<'_>> = None;
    Document::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(text: &str) -> Element {
        parse(text).root.expect("a root element")
    }

    fn child<'a>(e: &'a Element, name: &str) -> &'a Element {
        e.elements().find(|c| c.name == name).unwrap_or_else(|| panic!("no <{name}> in <{}>", e.name))
    }

    #[test]
    fn a_well_formed_feed_reads_into_elements_with_their_namespaces() {
        let doc = parse(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\" \
             xmlns:content=\"http://purl.org/rss/1.0/modules/content/\"><channel><title>T</title>\
             <item><content:encoded><![CDATA[<p>Hi & bye</p>]]></content:encoded></item></channel></rss>",
        );
        assert!(doc.problems.is_empty(), "{:?}", doc.problems);
        let rss = doc.root.expect("the rss element");
        assert_eq!(rss.name, "rss");
        assert_eq!(rss.attr("version"), Some("2.0"));
        let channel = child(&rss, "channel");
        assert_eq!(child(channel, "title").text(), "T");
        let encoded = child(child(channel, "item"), "content:encoded");
        assert_eq!(encoded.namespace, "http://purl.org/rss/1.0/modules/content/");
        assert_eq!(encoded.local_name(), "encoded");
        assert_eq!(encoded.prefix(), "content");
        assert_eq!(encoded.text(), "<p>Hi & bye</p>", "CDATA is text as written");
    }

    #[test]
    fn the_default_namespace_applies_to_unprefixed_elements() {
        let feed = root("<feed xmlns=\"http://www.w3.org/2005/Atom\"><entry><title>A</title></entry></feed>");
        assert_eq!(feed.namespace, "http://www.w3.org/2005/Atom");
        assert_eq!(child(child(&feed, "entry"), "title").namespace, "http://www.w3.org/2005/Atom");
    }

    #[test]
    fn a_bare_ampersand_is_text_and_the_xml_and_numeric_references_are_decoded() {
        let t = root("<t>Tom & Jerry &amp; &#233;&#x41; &lt;b&gt; &quot;q&apos; &nbsp;&copy;</t>");
        assert_eq!(t.text(), "Tom & Jerry & \u{e9}A <b> \"q' &nbsp;&copy;");
    }

    #[test]
    fn a_mismatched_end_tag_closes_up_to_its_own_element() {
        let rss = root("<channel><item><title>A</item><item><title>B</title></item></channel>");
        let items: Vec<&Element> = rss.elements().filter(|e| e.name == "item").collect();
        assert_eq!(items.len(), 2, "{rss:?}");
        assert_eq!(child(items[0], "title").text(), "A");
        assert_eq!(child(items[1], "title").text(), "B");
    }

    #[test]
    fn an_end_tag_that_matches_nothing_is_ignored() {
        let doc = parse("<channel><title>A</b></title><link>x</link></channel>");
        let channel = doc.root.expect("channel");
        assert_eq!(child(&channel, "title").text(), "A");
        assert_eq!(child(&channel, "link").text(), "x");
        assert!(!doc.problems.is_empty(), "the stray </b> is reported");
    }

    #[test]
    fn a_cut_off_feed_keeps_everything_that_was_read() {
        let doc = parse("<rss><channel><item><title>A</title></item><item><title>B is cut");
        let rss = doc.root.expect("rss");
        let channel = child(&rss, "channel");
        let titles: Vec<String> = channel.elements().map(|i| child(i, "title").text()).collect();
        assert_eq!(titles, vec!["A".to_string(), "B is cut".to_string()]);
        assert!(doc.problems.iter().any(|p| p.contains("ends inside")), "{:?}", doc.problems);
    }

    #[test]
    fn unescaped_html_becomes_elements_and_is_written_back_as_markup() {
        let d = root("<description>Hello <b>bold</b> &amp; <br> world &nbsp;<img src=\"a.png\"/></description>");
        assert!(d.has_element_children());
        assert_eq!(d.inner_markup(), "Hello <b>bold</b> &amp; <br/> world &nbsp;<img src=\"a.png\"/>");
        assert_eq!(d.text(), "Hello bold &  world &nbsp;");
    }

    #[test]
    fn attributes_are_unescaped_and_an_unquoted_value_is_read() {
        let a = root("<a href=\"?a=1&amp;b=2&c=3\" title=plain t2='x &lt; y'/>");
        assert_eq!(a.attr("href"), Some("?a=1&b=2&c=3"));
        assert_eq!(a.attr("title"), Some("plain"));
        assert_eq!(a.attr("T2"), Some("x < y"), "a name in any case");
    }

    #[test]
    fn blank_lines_before_the_declaration_and_comments_are_no_problem() {
        let rss = root("\n\n   <?xml version=\"1.0\"?>\n<!-- generator --><rss><channel/></rss>");
        assert_eq!(rss.name, "rss");
        assert_eq!(child(&rss, "channel").children.len(), 0);
    }

    #[test]
    fn an_undeclared_prefix_keeps_its_name_and_an_empty_namespace() {
        let item = root("<item><media:thumbnail url=\"https://example.org/t.jpg\"/></item>");
        let thumb = child(&item, "media:thumbnail");
        assert_eq!(thumb.namespace, "");
        assert_eq!(thumb.prefix(), "media");
        assert_eq!(thumb.attr("url"), Some("https://example.org/t.jpg"));
    }

    #[test]
    fn crlf_line_ends_are_read_as_newlines() {
        assert_eq!(root("<t>a\r\nb\rc</t>").text(), "a\nb\nc");
    }

    #[test]
    fn the_charset_comes_from_the_bom_the_declaration_or_the_content_type() {
        assert_eq!(decode(b"\xEF\xBB\xBF<a>x</a>", ""), "<a>x</a>", "a UTF-8 BOM is dropped");
        let latin1 = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><t>Caf\xe9</t>";
        assert!(decode(latin1, "").contains("Caf\u{e9}"), "the declaration names Latin-1");
        assert!(decode(b"<t>Caf\xe9</t>", "text/xml; charset=windows-1252").contains("Caf\u{e9}"));
        assert!(
            decode(b"<t>\x93quoted\x94</t>", "").contains("\u{201c}quoted\u{201d}"),
            "undeclared bytes that are not UTF-8 are read as Windows-1252"
        );
        assert_eq!(
            decode("<t>\u{e9}</t>".as_bytes(), "text/xml; charset=iso-8859-1"),
            "<t>\u{e9}</t>",
            "bytes that are valid UTF-8 are UTF-8, whatever an old server's default charset says"
        );
    }

    #[test]
    fn utf16_with_a_bom_is_decoded() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "<a>\u{e9}</a>".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode(&bytes, ""), "<a>\u{e9}</a>");
    }
}
