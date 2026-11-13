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
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

impl Element {
    /// The name after its prefix (`encoded` of `content:encoded`).
    #[must_use]
    pub fn local_name(&self) -> &str {
        self.name
            .rsplit_once(':')
            .map_or(self.name.as_str(), |(_, local)| local)
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
            .or_else(|| {
                self.attributes
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
            })
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

fn write_markup(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => push_escaped(out, text, false),
        Node::Element(e) => {
            let name = e.local_name();
            out.push('<');
            out.push_str(name);
            for (key, value) in &e.attributes {
                if key == "xmlns" || key.starts_with("xmlns:") {
                    continue;
                }
                out.push(' ');
                out.push_str(key);
                out.push_str("=\"");
                push_escaped(out, value, true);
                out.push('"');
            }
            if VOID_ELEMENTS.contains(&name.to_ascii_lowercase().as_str()) {
                out.push_str("/>");
                for child in &e.children {
                    write_markup(child, out);
                }
            } else {
                out.push('>');
                for child in &e.children {
                    write_markup(child, out);
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
        }
    }
}

/// `text` escaped for markup: `&` (unless it starts a reference - one kept from the input),
/// `<`, `>`, and in an attribute value `"`.
fn push_escaped(out: &mut String, text: &str, attribute: bool) {
    let mut rest = text;
    while let Some(at) = rest.find(|c: char| matches!(c, '&' | '<' | '>' | '"')) {
        out.push_str(&rest[..at]);
        match rest.as_bytes()[at] {
            b'&' => {
                let after = &rest[at + 1..];
                out.push_str(if starts_with_reference(after) {
                    "&"
                } else {
                    "&amp;"
                });
            }
            b'<' => out.push_str("&lt;"),
            b'>' => out.push_str("&gt;"),
            _ => out.push_str(if attribute { "&quot;" } else { "\"" }),
        }
        rest = &rest[at + 1..];
    }
    out.push_str(rest);
}

/// The longest reference name read (`&CounterClockwiseContourIntegral;` is 31).
const MAX_REFERENCE: usize = 32;

/// Whether `after` (the text after an `&`) is a reference name and its `;`.
fn starts_with_reference(after: &str) -> bool {
    after
        .find(';')
        .is_some_and(|end| end > 0 && end <= MAX_REFERENCE && is_reference_name(&after[..end]))
}

/// `amp`, `eacute`, `#233`, `#xE9`.
fn is_reference_name(name: &str) -> bool {
    match name.strip_prefix('#') {
        Some(number) => match number
            .strip_prefix('x')
            .or_else(|| number.strip_prefix('X'))
        {
            Some(hex) => !hex.is_empty() && hex.chars().all(|c: char| c.is_ascii_hexdigit()),
            None => !number.is_empty() && number.chars().all(|c: char| c.is_ascii_digit()),
        },
        None => {
            name.chars()
                .next()
                .is_some_and(|c: char| c.is_ascii_alphabetic())
                && name.chars().all(|c: char| c.is_ascii_alphanumeric())
        }
    }
}

/// What a reference stands for: the five XML names and the numeric references decoded (a
/// numeric one in 128-159 as Windows-1252, as browsers read `&#150;`; an invalid one as
/// U+FFFD), any other name kept as written (`&nbsp;` - HTML's, for the HTML parser).
fn reference(name: &str) -> String {
    match name {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        _ => match name.strip_prefix('#') {
            Some(number) => {
                let code = match number
                    .strip_prefix('x')
                    .or_else(|| number.strip_prefix('X'))
                {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse::<u32>().ok(),
                };
                match code {
                    Some(c @ 0x80..=0x9f) => {
                        let byte = [c as u8];
                        encoding_rs::WINDOWS_1252
                            .decode_without_bom_handling(&byte)
                            .0
                            .into_owned()
                    }
                    Some(c) => char::from_u32(c)
                        .filter(|ch| *ch != '\0')
                        .map_or_else(|| "\u{fffd}".to_string(), |ch| ch.to_string()),
                    None => "\u{fffd}".to_string(),
                }
            }
            None => format!("&{name};"),
        },
    }
}

/// An attribute's raw value with its references decoded ([`reference`]); a bare `&` stays.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        if starts_with_reference(after) {
            let end = after.find(';').unwrap_or(0);
            out.push_str(&reference(&after[..end]));
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// CR LF and a lone CR as LF (XML's end-of-line handling).
fn newlines(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

/// The `charset` parameter of a `Content-Type` (`text/xml; charset="utf-8"` gives `utf-8`).
fn charset_of(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|param| {
        let (key, value) = param.split_once('=')?;
        if key.trim().eq_ignore_ascii_case("charset") {
            Some(value.trim().trim_matches(|c: char| c == '"' || c == '\''))
        } else {
            None
        }
    })
}

/// The `encoding` of an XML declaration at the start of `bytes` (read as ASCII).
fn declared_encoding(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let start = head.find("<?xml")?;
    let end = head[start..].find("?>").map_or(head.len(), |e| start + e);
    let declaration = &head[start..end];
    let at = declaration.find("encoding")?;
    let rest = declaration[at + "encoding".len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let quote = rest.chars().next().filter(|q| *q == '"' || *q == '\'')?;
    let value = &rest[1..];
    let close = value.find(quote)?;
    Some(value[..close].trim().to_string())
}

/// An 8-bit (ASCII-compatible) encoding by its label; UTF-8 itself is not one to fall back to.
fn eight_bit(label: &str) -> Option<&'static encoding_rs::Encoding> {
    encoding_rs::Encoding::for_label(label.as_bytes())
        .filter(|e| e.is_ascii_compatible() && *e != encoding_rs::UTF_8)
}

/// The text of a feed's bytes. `content_type` is the HTTP `Content-Type` (its `charset`), empty
/// when there is none (a file). See the module documentation for the order.
#[must_use]
pub fn decode(bytes: &[u8], content_type: &str) -> String {
    if let Some((encoding, bom)) = encoding_rs::Encoding::for_bom(bytes) {
        return encoding
            .decode_without_bom_handling(&bytes[bom..])
            .0
            .into_owned();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let encoding = declared_encoding(bytes)
        .and_then(|label| eight_bit(&label))
        .or_else(|| charset_of(content_type).and_then(eight_bit))
        .unwrap_or(encoding_rs::WINDOWS_1252);
    encoding.decode_without_bom_handling(bytes).0.into_owned()
}

/// Elements nested deeper than this are not opened (their text lands in the deepest one).
const MAX_DEPTH: usize = 256;
/// Ill-formed spots after which the rest of the input is not read.
const MAX_PROBLEMS: usize = 1000;

/// The tree under construction.
struct Builder {
    /// The open elements; `[0]` is the document (its children are the top-level nodes).
    open: Vec<Element>,
    /// The namespaces each open element declared, `(prefix, uri)` (`""` = the default one).
    scopes: Vec<Vec<(String, String)>>,
    problems: Vec<String>,
}

impl Builder {
    fn new() -> Builder {
        Builder {
            open: vec![Element::default()],
            scopes: vec![Vec::new()],
            problems: Vec::new(),
        }
    }

    fn top(&mut self) -> &mut Element {
        let last = self.open.len() - 1;
        &mut self.open[last]
    }

    /// The namespace `prefix` is bound to in scope (`""`: none).
    fn lookup(&self, prefix: &str) -> String {
        if prefix == "xml" {
            return "http://www.w3.org/XML/1998/namespace".to_string();
        }
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| {
                scope
                    .iter()
                    .rev()
                    .find(|(p, _)| p == prefix)
                    .map(|(_, uri)| uri.clone())
            })
            .unwrap_or_default()
    }

    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let top = self.top();
        if let Some(Node::Text(last)) = top.children.last_mut() {
            last.push_str(text);
        } else {
            top.children.push(Node::Text(text.to_string()));
        }
    }

    fn start(&mut self, name: String, attributes: Vec<(String, String)>, empty: bool) {
        if self.open.len() > MAX_DEPTH {
            return;
        }
        let mut scope = Vec::new();
        for (key, value) in &attributes {
            if key == "xmlns" {
                scope.push((String::new(), value.clone()));
            } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                scope.push((prefix.to_string(), value.clone()));
            }
        }
        self.scopes.push(scope);
        let prefix = name.rsplit_once(':').map_or("", |(p, _)| p).to_string();
        let namespace = self.lookup(&prefix);
        let element = Element {
            name,
            namespace,
            attributes,
            children: Vec::new(),
        };
        if empty {
            self.scopes.pop();
            self.top().children.push(Node::Element(element));
        } else {
            self.open.push(element);
        }
    }

    /// Closes the innermost open element (never the document).
    fn close_top(&mut self) {
        if self.open.len() <= 1 {
            return;
        }
        if let Some(element) = self.open.pop() {
            self.scopes.pop();
            self.top().children.push(Node::Element(element));
        }
    }

    /// An end tag: closes the nearest open element of its name (by name, else in any case)
    /// and everything opened inside it; one that matches nothing is ignored.
    fn end(&mut self, name: &str) {
        let found = self
            .open
            .iter()
            .rposition(|e| e.name == name)
            .filter(|&i| i > 0)
            .or_else(|| {
                self.open
                    .iter()
                    .rposition(|e| e.name.eq_ignore_ascii_case(name))
                    .filter(|&i| i > 0)
            });
        match found {
            Some(index) => {
                while self.open.len() > index {
                    self.close_top();
                }
            }
            None => self.problems.push(format!("a stray </{name}> was ignored")),
        }
    }

    /// A `<...>` that is no tag (`a < b` in a title): its text up to a `</name` in it, which
    /// ends `name` (the tokenizer read on to the next `>`).
    fn bogus_tag(&mut self, raw: &str) {
        self.problems
            .push("a `<` that starts no tag was read as text".to_string());
        match raw.find("</") {
            Some(at) => {
                self.text(&format!("<{}", &raw[..at]));
                let name = raw[at + 2..].trim();
                self.end(name);
            }
            None => self.text(&format!("<{raw}>")),
        }
    }

    fn finish(mut self) -> Document {
        if self.open.len() > 1 {
            let inner = self.open.last().map(|e| e.name.clone()).unwrap_or_default();
            self.problems
                .push(format!("the input ends inside <{inner}>"));
        }
        while self.open.len() > 1 {
            self.close_top();
        }
        let document = self.open.pop().unwrap_or_default();
        let root = document.children.into_iter().find_map(|node| match node {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        });
        Document {
            root,
            problems: self.problems,
        }
    }
}

/// A start tag's name and attributes (`None`: no name - `a < b` read as a tag).
fn start_tag(e: &quick_xml::events::BytesStart<'_>) -> Option<(String, Vec<(String, String)>)> {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    if !name
        .chars()
        .next()
        .is_some_and(|c: char| c.is_alphabetic() || c == '_')
    {
        return None;
    }
    let mut attributes = e.html_attributes();
    attributes.with_checks(false);
    let attributes = attributes
        .take(256)
        .flatten()
        .map(|a| {
            (
                String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                unescape(&String::from_utf8_lossy(&a.value)),
            )
        })
        .collect();
    Some((name, attributes))
}

/// Reads `text` into a tree; never fails (see the module documentation).
#[must_use]
pub fn parse(text: &str) -> Document {
    let mut reader = Reader::from_str(text);
    {
        let config = reader.config_mut();
        config.allow_dangling_amp = true;
        config.allow_unmatched_ends = true;
        config.check_end_names = false;
    }
    let mut b = Builder::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match start_tag(&e) {
                Some((name, attributes)) => b.start(name, attributes, false),
                None => b.bogus_tag(&String::from_utf8_lossy(&e)),
            },
            Ok(Event::Empty(e)) => match start_tag(&e) {
                Some((name, attributes)) => b.start(name, attributes, true),
                None => b.bogus_tag(&format!("{}/", String::from_utf8_lossy(&e))),
            },
            Ok(Event::End(e)) => b.end(&String::from_utf8_lossy(e.name().as_ref())),
            Ok(Event::Text(e)) => b.text(&newlines(&String::from_utf8_lossy(&e))),
            Ok(Event::CData(e)) => b.text(&newlines(&String::from_utf8_lossy(&e))),
            Ok(Event::GeneralRef(e)) => b.text(&reference(&String::from_utf8_lossy(&e))),
            Ok(Event::Eof) => break,
            // Declarations, processing instructions, comments, doctypes.
            Ok(_) => {}
            Err(quick_xml::Error::IllFormed(e)) => {
                b.problems.push(e.to_string());
                if b.problems.len() > MAX_PROBLEMS {
                    break;
                }
            }
            Err(e) => {
                // A syntax error (an unclosed tag or comment at the end): the rest is not read.
                b.problems.push(e.to_string());
                break;
            }
        }
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(text: &str) -> Element {
        parse(text).root.expect("a root element")
    }

    fn child<'a>(e: &'a Element, name: &str) -> &'a Element {
        e.elements()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no <{name}> in <{}>", e.name))
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
        assert_eq!(
            encoded.namespace,
            "http://purl.org/rss/1.0/modules/content/"
        );
        assert_eq!(encoded.local_name(), "encoded");
        assert_eq!(encoded.prefix(), "content");
        assert_eq!(
            encoded.text(),
            "<p>Hi & bye</p>",
            "CDATA is text as written"
        );
    }

    #[test]
    fn the_default_namespace_applies_to_unprefixed_elements() {
        let feed = root(
            "<feed xmlns=\"http://www.w3.org/2005/Atom\"><entry><title>A</title></entry></feed>",
        );
        assert_eq!(feed.namespace, "http://www.w3.org/2005/Atom");
        assert_eq!(
            child(child(&feed, "entry"), "title").namespace,
            "http://www.w3.org/2005/Atom"
        );
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
        let titles: Vec<String> = channel
            .elements()
            .map(|i| child(i, "title").text())
            .collect();
        assert_eq!(titles, vec!["A".to_string(), "B is cut".to_string()]);
        assert!(
            doc.problems.iter().any(|p| p.contains("ends inside")),
            "{:?}",
            doc.problems
        );
    }

    #[test]
    fn unescaped_html_becomes_elements_and_is_written_back_as_markup() {
        let d = root("<description>Hello <b>bold</b> &amp; <br> world &nbsp;<img src=\"a.png\"/></description>");
        assert!(d.has_element_children());
        assert_eq!(
            d.inner_markup(),
            "Hello <b>bold</b> &amp; <br/> world &nbsp;<img src=\"a.png\"/>"
        );
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
        assert_eq!(
            decode(b"\xEF\xBB\xBF<a>x</a>", ""),
            "<a>x</a>",
            "a UTF-8 BOM is dropped"
        );
        let latin1 = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><t>Caf\xe9</t>";
        assert!(
            decode(latin1, "").contains("Caf\u{e9}"),
            "the declaration names Latin-1"
        );
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
