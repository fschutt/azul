//! Articles through azul's HTML5-like parser (`Xml::create_from_html`, the lenient tokenizer
//! and the browser-like tree construction every azul loader shares).
//!
//! - [`plain_text`] / [`excerpt`]: the text of a piece of HTML (a title that holds `&amp;` or
//!   `<code>`, a description for the list's two lines), its character references decoded by
//!   the parser - the engine's one table of HTML's names.

use azul::{
    dom::{XmlAttributeMap, XmlNode, XmlNodeChild},
    str::{String as AzString, StringPair},
    vec::{StringPairVec, XmlNodeChildVec},
    xml::{ExternalResourceKind, Xml, XmlTagName},
};

use crate::{fetch::FeedLink, ids, links};

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

/// The feeds a web page names - `<link rel="alternate" type="application/rss+xml" (atom+xml,
/// feed+json, json) href title>` - absolute against `base`, each once, in the page's order.
#[must_use]
pub fn feed_links(html: &str, base: &str) -> Vec<FeedLink> {
    let document = Xml::create_from_html(html);
    let mut out = Vec::new();
    collect_feed_links(&document.root, base, &mut out, 0);
    out
}

/// The `type`s of a feed link.
const FEED_TYPES: &[&str] = &[
    "application/rss+xml",
    "application/atom+xml",
    "application/feed+json",
    "application/json",
    "application/rdf+xml",
];

/// An attribute of a parsed element (any case), trimmed; `""` when it has none.
fn attribute_of(element: &XmlNode, name: &str) -> String {
    element
        .attributes
        .inner
        .iter()
        .find(|pair| pair.key.as_str().eq_ignore_ascii_case(name))
        .map(|pair| pair.value.as_str().trim().to_string())
        .unwrap_or_default()
}

fn collect_feed_links(nodes: &[XmlNodeChild], base: &str, out: &mut Vec<FeedLink>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for node in nodes {
        let XmlNodeChild::Element(element) = node else {
            continue;
        };
        if element.node_type.inner.as_str() == "link" {
            let rel = attribute_of(element, "rel").to_ascii_lowercase();
            let mime = attribute_of(element, "type").to_ascii_lowercase();
            let href = attribute_of(element, "href");
            if rel.split_whitespace().any(|r| r == "alternate")
                && FEED_TYPES.contains(&mime.as_str())
                && !href.is_empty()
            {
                let url = links::resolve(base, &href);
                if !out.iter().any(|l| l.url == url) {
                    out.push(FeedLink {
                        url,
                        title: collapse(&attribute_of(element, "title")),
                        mime,
                    });
                }
            }
        }
        collect_feed_links(&element.children, base, out, depth + 1);
    }
}

// ==== The reader view ====

/// An article made ready for the reading pane.
#[derive(Debug, Clone)]
pub struct Article {
    /// `<html><head><style>` the reader stylesheet `</style></head><body><div class="__aznews_article">`
    /// the cleaned article `</div></body></html>`, for `Dom::create_from_parsed_xml`.
    pub xml: Xml,
    /// The web pictures it shows, each once (`Xml::scan_external_resources` of the cleaned tree):
    /// what is fetched on a Thread and put into the image cache under its address.
    pub images: Vec<String>,
    /// Pictures not loaded (shown as `[image: alt]`): the "load pictures" notice counts them.
    pub blocked: usize,
    /// Words of text (the reading time).
    pub words: usize,
}

/// The reading time of `words`, in minutes (230 words a minute, at least one).
#[must_use]
pub fn reading_minutes(words: usize) -> usize {
    words.div_ceil(230).max(1)
}

/// The reader stylesheet: typography only - the colours come from the app's theme and mode, so
/// the article reads in flat and flora, light and dark; `sepia` puts it on warm paper.
#[must_use]
pub fn reader_css(font_px: u32, measure_px: u32, sepia: bool) -> String {
    let a = format!(".{}", ids::ARTICLE_CLASS);
    let paper = if sepia {
        "background-color: #f4ecd8; color: #3b2f1e;"
    } else {
        ""
    };
    let mut css = format!(
        "{a} {{ font-family: serif; font-size: {font_px}px; line-height: 1.6; max-width: {measure_px}px; \
         padding: 8px 24px 32px 24px; overflow-wrap: break-word; {paper} }}\n"
    );
    let rules = [
        "p { margin: 0px 0px 0.9em 0px; }",
        "h2, h3, h4, h5, h6 { font-family: sans-serif; line-height: 1.25; margin: 1.2em 0px 0.5em 0px; }",
        "h2 { font-size: 1.4em; }",
        "h3 { font-size: 1.2em; }",
        "h4, h5, h6 { font-size: 1em; }",
        "blockquote { margin: 1em 0px; padding: 0px 0px 0px 1em; border-left: 3px solid rgba(128, 128, 128, 0.6); \
         font-style: italic; }",
        "pre { font-family: monospace; font-size: 0.8em; padding: 0.8em; white-space: pre; overflow-x: auto; \
         background-color: rgba(127, 127, 127, 0.12); }",
        "code { font-family: monospace; font-size: 0.85em; }",
        "img { max-width: 100%; }",
        "a { color: #2f74d0; }",
        "ul, ol { margin: 0px 0px 0.9em 0px; padding-left: 1.5em; }",
        "table { border-collapse: collapse; margin: 0px 0px 0.9em 0px; }",
        "td, th { border: 1px solid rgba(128, 128, 128, 0.4); padding: 4px 8px; }",
        "hr { border-top: 1px solid rgba(128, 128, 128, 0.4); margin: 1.5em 0px; }",
    ];
    for rule in rules {
        // Every selector of the rule inside the article.
        let (selectors, body) = rule.split_once('{').unwrap_or((rule, ""));
        let scoped: Vec<String> = selectors
            .split(',')
            .map(|sel| format!("{a} {}", sel.trim()))
            .collect();
        css.push_str(&format!("{} {{{body}\n", scoped.join(", ")));
    }
    css.push_str(&format!(
        ".{} {{ font-family: sans-serif; font-size: 0.8em; opacity: 0.75; margin-top: 4px; }}\n",
        ids::CAPTION_CLASS
    ));
    css.push_str(&format!(
        ".{} {{ font-family: sans-serif; font-size: 0.8em; opacity: 0.6; }}\n",
        ids::IMAGE_PLACEHOLDER_CLASS
    ));
    css
}

/// `html` (an article's body) through azul's HTML5-like parser into the reader view: only what
/// reads (paragraphs, headings, quotes, lists, code, tables, figures, links, pictures), every
/// link and picture absolute against `base` (tracking parameters off with `strip_tracking`),
/// pictures as placeholders unless `load_images`, tracking pixels gone, scripts, styles, forms
/// and frames gone with their content, `css` (see [`reader_css`]) in its head.
#[must_use]
pub fn article(
    html: &str,
    base: &str,
    load_images: bool,
    strip_tracking: bool,
    css: &str,
) -> Article {
    let parsed = Xml::create_from_html(html);
    let mut cleaner = Cleaner {
        base,
        load_images,
        strip_tracking,
        blocked: 0,
        words: 0,
    };
    let content = cleaner.children(&parsed.root, 0);
    let head = element(
        "head",
        Vec::new(),
        vec![element("style", Vec::new(), vec![text_node(css)])],
    );
    let body = element(
        "body",
        Vec::new(),
        vec![element(
            "div",
            vec![("class", ids::ARTICLE_CLASS.to_string())],
            content,
        )],
    );
    let xml = Xml {
        root: XmlNodeChildVec::from_vec(vec![element("html", Vec::new(), vec![head, body])]),
    };
    let images = images_of(&xml);
    Article {
        xml,
        images,
        blocked: cleaner.blocked,
        words: cleaner.words,
    }
}

/// The web pictures of a document, each once (`Xml::scan_external_resources`).
fn images_of(xml: &Xml) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for resource in xml.scan_external_resources().iter() {
        if matches!(resource.kind, ExternalResourceKind::Image) {
            let url = resource.url.as_str().to_string();
            if links::is_web(&url) && !out.contains(&url) {
                out.push(url);
            }
        }
    }
    out
}

/// Elements that go with everything in them.
const DROPPED: &[&str] = &[
    "script", "style", "noscript", "template", "iframe", "frame", "frameset", "object", "embed",
    "applet", "form", "input", "button", "select", "textarea", "option", "svg", "math", "nav",
    "head", "title", "meta", "link", "canvas", "audio", "video", "source", "track", "map",
    "dialog",
];

/// Elements kept as they are (without their attributes).
const KEPT: &[&str] = &[
    "p",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
    "code",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "em",
    "strong",
    "b",
    "i",
    "u",
    "s",
    "sub",
    "sup",
    "mark",
    "small",
    "q",
    "cite",
    "abbr",
    "kbd",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "caption",
    "br",
    "hr",
    "del",
    "ins",
];

/// Elements kept as a block (`div`).
const BLOCKISH: &[&str] = &[
    "div", "figure", "section", "article", "header", "footer", "main", "aside", "details",
    "summary", "address", "center",
];

/// An element of the reader's document.
fn element(
    name: &str,
    attributes: Vec<(&str, String)>,
    children: Vec<XmlNodeChild>,
) -> XmlNodeChild {
    XmlNodeChild::Element(XmlNode {
        node_type: XmlTagName {
            inner: AzString::from(name),
        },
        attributes: XmlAttributeMap {
            inner: StringPairVec::from_vec(
                attributes
                    .into_iter()
                    .map(|(key, value)| StringPair {
                        key: AzString::from(key),
                        value: AzString::from(value),
                    })
                    .collect(),
            ),
        },
        children: XmlNodeChildVec::from_vec(children),
    })
}

fn text_node(text: &str) -> XmlNodeChild {
    XmlNodeChild::Text(AzString::from(text))
}

/// The walk that keeps what reads (see [`article`]).
struct Cleaner<'a> {
    base: &'a str,
    load_images: bool,
    strip_tracking: bool,
    blocked: usize,
    words: usize,
}

impl Cleaner<'_> {
    fn children(&mut self, nodes: &[XmlNodeChild], depth: usize) -> Vec<XmlNodeChild> {
        let mut out = Vec::new();
        if depth > MAX_DEPTH {
            return out;
        }
        for node in nodes {
            match node {
                XmlNodeChild::Text(text) => {
                    self.words += text.as_str().split_whitespace().count();
                    out.push(text_node(text.as_str()));
                }
                XmlNodeChild::Element(e) => out.extend(self.element(e, depth + 1)),
            }
        }
        out
    }

    fn element(&mut self, e: &XmlNode, depth: usize) -> Vec<XmlNodeChild> {
        let name = e.node_type.inner.as_str().to_ascii_lowercase();
        let name = name.as_str();
        if DROPPED.contains(&name) {
            return Vec::new();
        }
        match name {
            "img" => self.image(e).into_iter().collect(),
            "a" => {
                let children = self.children(&e.children, depth);
                let href = links::resolve(self.base, &attribute_of(e, "href"));
                if links::is_web(&href) || href.to_ascii_lowercase().starts_with("mailto:") {
                    let href = if self.strip_tracking {
                        links::strip_tracking(&href)
                    } else {
                        href
                    };
                    vec![element("a", vec![("href", href)], children)]
                } else {
                    // An anchor without a link one can follow: its text.
                    children
                }
            }
            "h1" => vec![element("h2", Vec::new(), self.children(&e.children, depth))],
            "figcaption" => vec![element(
                "div",
                vec![("class", ids::CAPTION_CLASS.to_string())],
                self.children(&e.children, depth),
            )],
            "td" | "th" => {
                let mut attributes = Vec::new();
                for key in ["colspan", "rowspan"] {
                    let value = attribute_of(e, key);
                    if !value.is_empty() && value.chars().all(|c: char| c.is_ascii_digit()) {
                        attributes.push((key, value));
                    }
                }
                vec![element(name, attributes, self.children(&e.children, depth))]
            }
            _ if KEPT.contains(&name) => {
                vec![element(name, Vec::new(), self.children(&e.children, depth))]
            }
            _ if BLOCKISH.contains(&name) => vec![element(
                "div",
                Vec::new(),
                self.children(&e.children, depth),
            )],
            // html, body, span, font, anything unknown: what is inside it.
            _ => self.children(&e.children, depth),
        }
    }

    /// A picture: kept (absolute), a placeholder when pictures are not loaded, gone when it is
    /// a tracking pixel or not on the web.
    fn image(&mut self, e: &XmlNode) -> Option<XmlNodeChild> {
        let src = Some(attribute_of(e, "src"))
            .filter(|s| !s.is_empty() && !s.starts_with("data:"))
            .or_else(|| Some(attribute_of(e, "data-src")).filter(|s| !s.is_empty()))?;
        let tiny = |key: &str| {
            attribute_of(e, key)
                .trim_end_matches("px")
                .parse::<u32>()
                .is_ok_and(|v| v <= 2)
        };
        if tiny("width") || tiny("height") {
            return None;
        }
        let url = links::resolve(self.base, &src);
        if !links::is_web(&url) {
            return None;
        }
        let alt = collapse(&attribute_of(e, "alt"));
        if self.load_images {
            Some(element("img", vec![("src", url), ("alt", alt)], Vec::new()))
        } else {
            self.blocked += 1;
            let label = if alt.is_empty() {
                "[image]".to_string()
            } else {
                format!("[image: {alt}]")
            };
            Some(element(
                "span",
                vec![("class", ids::IMAGE_PLACEHOLDER_CLASS.to_string())],
                vec![text_node(&label)],
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every element's name in document order, `name.class` when it has a class.
    fn outline(nodes: &[XmlNodeChild], out: &mut Vec<String>) {
        for node in nodes {
            if let XmlNodeChild::Element(e) = node {
                let class = attribute_of(e, "class");
                let name = e.node_type.inner.as_str().to_string();
                out.push(if class.is_empty() {
                    name
                } else {
                    format!("{name}.{class}")
                });
                outline(&e.children, out);
            }
        }
    }

    fn names(a: &Article) -> Vec<String> {
        let mut out = Vec::new();
        outline(&a.xml.root, &mut out);
        out
    }

    /// The values of `attribute` on every `element` in document order.
    fn values(nodes: &[XmlNodeChild], element: &str, attribute: &str, out: &mut Vec<String>) {
        for node in nodes {
            if let XmlNodeChild::Element(e) = node {
                if e.node_type.inner.as_str() == element {
                    out.push(attribute_of(e, attribute));
                }
                values(&e.children, element, attribute, out);
            }
        }
    }

    fn attr_values(a: &Article, element: &str, attribute: &str) -> Vec<String> {
        let mut out = Vec::new();
        values(&a.xml.root, element, attribute, &mut out);
        out
    }

    fn text(a: &Article) -> String {
        let mut out = String::new();
        text_of(&a.xml.root, &mut out, 0);
        collapse(&out)
    }

    const PAGE: &str = "<h1>Title</h1><p>Hello <b>world</b>, read <a href=\"/more?utm_source=rss&id=3\">more</a>.</p>\
        <script>alert('x')</script><style>p { color: red }</style><form><input name=q><button>Go</button></form>\
        <iframe src=\"https://ads.example.net/\"></iframe>\
        <figure><img src=\"images/hero.jpg\" alt=\"The hero\"><figcaption>A caption</figcaption></figure>\
        <p><img src=\"https://pixel.example.net/t.gif\" width=\"1\" height=\"1\"></p>\
        <custom-widget><p>Kept text</p></custom-widget>";

    #[test]
    fn the_reader_keeps_what_reads_and_drops_scripts_forms_and_frames() {
        let a = article(PAGE, "https://example.org/blog/post.html", true, false, "");
        let names = names(&a);
        for gone in [
            "script",
            "form",
            "input",
            "button",
            "iframe",
            "custom-widget",
            "h1",
        ] {
            assert!(!names.iter().any(|n| n == gone), "{gone} in {names:?}");
        }
        assert!(
            names.contains(&"h2".to_string()),
            "h1 becomes h2: {names:?}"
        );
        assert!(
            names.contains(&"div.__aznews_article".to_string()),
            "{names:?}"
        );
        assert!(
            names.contains(&"div.__aznews_caption".to_string()),
            "{names:?}"
        );
        let t = text(&a);
        assert!(t.contains("Hello world, read more."), "{t}");
        assert!(
            t.contains("Kept text"),
            "an unknown element keeps its text: {t}"
        );
        assert!(!t.contains("alert"), "{t}");
        assert!(
            !t.contains("color: red"),
            "the article's own style is gone: {t}"
        );
    }

    #[test]
    fn links_and_pictures_resolve_against_the_base_and_tracking_comes_off() {
        let a = article(PAGE, "https://example.org/blog/post.html", true, true, "");
        assert_eq!(
            attr_values(&a, "a", "href"),
            vec!["https://example.org/more?id=3".to_string()]
        );
        assert_eq!(
            attr_values(&a, "img", "src"),
            vec!["https://example.org/blog/images/hero.jpg".to_string()]
        );
        assert_eq!(attr_values(&a, "img", "alt"), vec!["The hero".to_string()]);
        let kept = article(PAGE, "https://example.org/blog/post.html", true, false, "");
        assert_eq!(
            attr_values(&kept, "a", "href"),
            vec!["https://example.org/more?utm_source=rss&id=3".to_string()]
        );
    }

    #[test]
    fn the_pictures_are_listed_when_loaded_and_placeholders_otherwise() {
        let loaded = article(PAGE, "https://example.org/blog/post.html", true, false, "");
        assert_eq!(
            loaded.images,
            vec!["https://example.org/blog/images/hero.jpg".to_string()],
            "the tracking pixel is not one"
        );
        assert_eq!(loaded.blocked, 0);
        let blocked = article(PAGE, "https://example.org/blog/post.html", false, false, "");
        assert!(blocked.images.is_empty());
        assert_eq!(blocked.blocked, 1);
        assert!(attr_values(&blocked, "img", "src").is_empty());
        assert!(
            text(&blocked).contains("[image: The hero]"),
            "{}",
            text(&blocked)
        );
        assert!(names(&blocked).contains(&"span.__aznews_image-placeholder".to_string()));
    }

    #[test]
    fn the_reader_stylesheet_is_in_the_head_and_follows_the_settings() {
        let css = reader_css(20, 680, false);
        assert!(css.contains(".__aznews_article"), "{css}");
        assert!(css.contains("font-size: 20px"), "{css}");
        assert!(css.contains("max-width: 680px"), "{css}");
        assert!(
            !css.contains("background-color: #f4ecd8"),
            "no paper colour unless sepia"
        );
        assert!(reader_css(18, 600, true).contains("background-color: #f4ecd8"));
        let a = article("<p>x</p>", "https://example.org/", true, false, &css);
        fn style_texts(nodes: &[XmlNodeChild], out: &mut Vec<String>) {
            for node in nodes {
                if let XmlNodeChild::Element(e) = node {
                    if e.node_type.inner.as_str() == "style" {
                        let mut t = String::new();
                        for c in e.children.iter() {
                            if let XmlNodeChild::Text(x) = c {
                                t.push_str(x.as_str());
                            }
                        }
                        out.push(t);
                    }
                    style_texts(&e.children, out);
                }
            }
        }
        let mut styles = Vec::new();
        style_texts(&a.xml.root, &mut styles);
        assert_eq!(
            styles,
            vec![css.clone()],
            "one style element holds the sheet"
        );
    }

    #[test]
    fn the_reading_time_is_the_words_over_230_at_least_one_minute() {
        assert_eq!(reading_minutes(0), 1);
        assert_eq!(reading_minutes(230), 1);
        assert_eq!(reading_minutes(231), 2);
        assert_eq!(reading_minutes(2_300), 10);
        let a = article(
            "<p>one two three</p><script>four five</script>",
            "https://example.org/",
            true,
            false,
            "",
        );
        assert_eq!(a.words, 3);
    }

    #[test]
    fn a_web_page_names_its_feeds_in_alternate_links() {
        let html = String::from_utf8_lossy(include_bytes!("../tests/fixtures/html_page.html"))
            .into_owned();
        let links = feed_links(&html, "https://example.org/");
        let found: Vec<(&str, &str, &str)> = links
            .iter()
            .map(|l| (l.url.as_str(), l.title.as_str(), l.mime.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                (
                    "https://example.org/feed/",
                    "Example Weekly \u{bb} Feed",
                    "application/rss+xml"
                ),
                (
                    "https://example.org/comments/feed/",
                    "Example Weekly \u{bb} Comments Feed",
                    "application/rss+xml"
                ),
                (
                    "https://example.org/feed/atom/",
                    "Atom",
                    "application/atom+xml"
                ),
                (
                    "https://example.org/feed.json",
                    "JSON",
                    "application/feed+json"
                ),
            ],
            "the stylesheet, the language version and the icon are no feeds"
        );
        assert!(feed_links("<p>no links</p>", "https://example.org/").is_empty());
    }

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
