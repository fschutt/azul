//! RSS 0.9x / 1.0 / 2.0, Atom 1.0 / 0.3 and JSON Feed 1.0 / 1.1 into ONE model ([`Feed`],
//! [`Item`]).
//!
//! The XML formats are read through [`crate::xmltree`] (lenient: a malformed real feed keeps
//! what can be read), JSON Feed through serde_json. Where the formats differ:
//!
//! - **id**: RSS `guid`, RSS 1.0 `rdf:about`, Atom `id`, JSON `id` (a number too); else the
//!   link; else a hash of the title, the date and the text (stable: the read marks key on it);
//! - **title**: plain text. RSS titles and Atom `type="html"` titles are HTML in practice
//!   (`&amp;amp;`, `&lt;code&gt;`): their text goes through azul's HTML parser
//!   ([`crate::reader::plain_text`]); Atom `type="text"` and JSON titles are taken as written;
//! - **summary / content**: HTML. RSS `description` / `content:encoded` are HTML source (escaped
//!   or CDATA - or, in a broken feed, unescaped markup, written back from the tree); Atom text
//!   constructs by their `type` (`text` escaped, `html` as is, `xhtml` the div's markup); JSON
//!   `content_html` as is, `content_text` / `summary` escaped;
//! - **links**: absolute - against `xml:base` (Atom) or the feed's address; [`Item::base`] is
//!   where the content's own relative links point from;
//! - **dates**: [`crate::dates::parse_date`] (lenient);
//! - **author**: the item's (`author`, `dc:creator`, `itunes:author`, Atom / JSON authors),
//!   else the feed's; an RSS `mail@example.org (Name)` is the name;
//! - **image**: `media:thumbnail`, an image `media:content`, `itunes:image`, an image
//!   enclosure, JSON `image` / `banner_image`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    dates::parse_date,
    links, reader,
    xmltree::{self, Element},
};

/// The formats read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    #[default]
    Rss2,
    Rss1,
    /// RSS 0.90 / 0.91 / 0.92.
    Rss09,
    Atom,
    Atom03,
    JsonFeed,
}

impl Format {
    /// For the user ("RSS 2.0", "Atom", "JSON Feed").
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Format::Rss2 => "RSS 2.0",
            Format::Rss1 => "RSS 1.0",
            Format::Rss09 => "RSS 0.9x",
            Format::Atom => "Atom",
            Format::Atom03 => "Atom 0.3",
            Format::JsonFeed => "JSON Feed",
        }
    }
}

/// A file an item carries (a podcast's episode).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Enclosure {
    pub url: String,
    pub mime: String,
    /// Bytes (0: not said).
    pub length: u64,
}

/// One article.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    /// Unique in its feed (see the module documentation).
    pub id: String,
    /// Plain text.
    pub title: String,
    /// The article's address (absolute; empty when the feed gives none).
    pub link: String,
    pub author: String,
    /// Seconds since 1970 (UTC).
    pub published: Option<i64>,
    pub updated: Option<i64>,
    /// HTML.
    pub summary: String,
    /// HTML (empty: the summary is all there is).
    pub content: String,
    /// The list's two lines: plain text of the summary (else the content), at most
    /// [`EXCERPT_CHARS`] characters.
    pub excerpt: String,
    /// Where the content's relative links point from.
    pub base: String,
    /// A picture for the article (absolute; empty: none).
    pub image: String,
    pub enclosures: Vec<Enclosure>,
    pub categories: Vec<String>,
    /// When AzNews first saw it (set by the library, 0 from the parser).
    pub seen: i64,
}

impl Item {
    /// The date it is listed under: published, else updated, else first seen.
    #[must_use]
    pub fn date(&self) -> i64 {
        self.published.or(self.updated).unwrap_or(self.seen)
    }

    /// The HTML the reader shows: the content, else the summary.
    #[must_use]
    pub fn body(&self) -> &str {
        if self.content.trim().is_empty() {
            &self.summary
        } else {
            &self.content
        }
    }
}

/// A parsed feed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Feed {
    pub format: Format,
    /// Plain text.
    pub title: String,
    /// The website (absolute).
    pub site: String,
    /// Plain text.
    pub description: String,
    /// The feed's picture (absolute; empty: none).
    pub icon: String,
    /// In the feed's order, each id once.
    pub items: Vec<Item>,
    /// What was repaired while reading (empty for a well-formed feed).
    pub problems: Vec<String>,
}

/// Why bytes are no feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedError {
    /// Something else: a web page (`html`: look for its feed links) or other data.
    NotAFeed { html: bool },
    /// It looked like a feed but could not be read at all (`{` that is no JSON).
    Invalid(String),
}

impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeedError::NotAFeed { html: true } => write!(f, "this is a web page, not a feed"),
            FeedError::NotAFeed { html: false } => write!(f, "this is not a feed"),
            FeedError::Invalid(why) => write!(f, "the feed could not be read: {why}"),
        }
    }
}

/// The longest excerpt, in characters.
pub const EXCERPT_CHARS: usize = 240;

/// Reads a feed. `content_type` is the HTTP `Content-Type` (empty for a file), `url` the
/// address it came from (relative links resolve against it).
pub fn parse(bytes: &[u8], content_type: &str, url: &str) -> Result<Feed, FeedError> {
    let text = xmltree::decode(bytes, content_type);
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        return parse_json(trimmed, url).map(|mut feed| {
            dedup(&mut feed.items);
            feed
        });
    }
    let document = xmltree::parse(&text);
    let Some(root) = document.root.as_ref() else {
        return Err(FeedError::NotAFeed {
            html: looks_like_html(&text),
        });
    };
    let mut feed = match root.local_name().to_ascii_lowercase().as_str() {
        "rss" => {
            let format = match root.attr("version").map(str::trim) {
                Some(v) if v.starts_with("0.9") => Format::Rss09,
                _ => Format::Rss2,
            };
            parse_rss(root, child(root, "channel", rss_core), url, format)
        }
        "rdf" => {
            let channel = child(root, "channel", rss_core);
            let format = if channel.is_some_and(|c| ns(c) == Ns::Rss09) {
                Format::Rss09
            } else {
                Format::Rss1
            };
            parse_rss(root, channel, url, format)
        }
        "feed" => parse_atom(root, url),
        "html" => return Err(FeedError::NotAFeed { html: true }),
        _ => {
            return Err(FeedError::NotAFeed {
                html: looks_like_html(&text),
            })
        }
    };
    feed.problems = document.problems.clone();
    dedup(&mut feed.items);
    Ok(feed)
}

/// Whether text that holds no feed is a web page.
fn looks_like_html(text: &str) -> bool {
    let head: String = text
        .chars()
        .take(2048)
        .collect::<String>()
        .to_ascii_lowercase();
    ["<!doctype html", "<html", "<head", "<body"]
        .iter()
        .any(|m| head.contains(m))
}

/// Each id once (the first item with it stays).
fn dedup(items: &mut Vec<Item>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.id.clone()));
}

// ==== Namespaces ====

/// The namespaces feeds use, by their URI - or, undeclared, by their usual prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ns {
    /// No namespace, no prefix.
    Plain,
    Atom,
    Atom03,
    Content,
    Dc,
    Media,
    Rdf,
    Rss1,
    Rss09,
    Itunes,
    Xhtml,
    FeedBurner,
    Other,
}

fn ns(e: &Element) -> Ns {
    match e.namespace.as_str() {
        "" => match e.prefix() {
            "" => Ns::Plain,
            "content" => Ns::Content,
            "dc" | "dcterms" => Ns::Dc,
            "media" => Ns::Media,
            "atom" | "a10" => Ns::Atom,
            "rdf" => Ns::Rdf,
            "itunes" => Ns::Itunes,
            "xhtml" => Ns::Xhtml,
            "feedburner" => Ns::FeedBurner,
            _ => Ns::Other,
        },
        "http://www.w3.org/2005/Atom" => Ns::Atom,
        "http://purl.org/atom/ns#" => Ns::Atom03,
        "http://purl.org/rss/1.0/modules/content/" => Ns::Content,
        "http://purl.org/dc/elements/1.1/" | "http://purl.org/dc/terms/" => Ns::Dc,
        "http://search.yahoo.com/mrss/" | "http://search.yahoo.com/mrss" => Ns::Media,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#" => Ns::Rdf,
        "http://purl.org/rss/1.0/" => Ns::Rss1,
        "http://my.netscape.com/rdf/simple/0.9/" => Ns::Rss09,
        "http://www.itunes.com/dtds/podcast-1.0.dtd" => Ns::Itunes,
        "http://www.w3.org/1999/xhtml" => Ns::Xhtml,
        "http://rssnamespace.org/feedburner/ext/1.0" => Ns::FeedBurner,
        _ => Ns::Other,
    }
}

/// RSS's own elements: no namespace, or RSS 1.0's / 0.90's.
fn rss_core(e: &Element) -> bool {
    matches!(ns(e), Ns::Plain | Ns::Rss1 | Ns::Rss09)
}

/// Atom's elements - and an Atom feed's that forgot its namespace.
fn atomish(e: &Element) -> bool {
    matches!(ns(e), Ns::Atom | Ns::Atom03 | Ns::Plain)
}

fn atom_ns(e: &Element) -> bool {
    matches!(ns(e), Ns::Atom | Ns::Atom03)
}

fn dc(e: &Element) -> bool {
    ns(e) == Ns::Dc
}

fn content_ns(e: &Element) -> bool {
    ns(e) == Ns::Content
}

fn media(e: &Element) -> bool {
    ns(e) == Ns::Media
}

fn itunes(e: &Element) -> bool {
    ns(e) == Ns::Itunes
}

fn feedburner(e: &Element) -> bool {
    ns(e) == Ns::FeedBurner
}

/// The first child element of this local name (any case) that `test` accepts.
fn child<'a>(e: &'a Element, local: &str, test: fn(&Element) -> bool) -> Option<&'a Element> {
    e.elements()
        .find(|c| c.local_name().eq_ignore_ascii_case(local) && test(c))
}

/// Every child element of this local name (any case) that `test` accepts.
fn children<'a>(
    e: &'a Element,
    local: &'a str,
    test: fn(&Element) -> bool,
) -> impl Iterator<Item = &'a Element> + 'a {
    e.elements()
        .filter(move |c| c.local_name().eq_ignore_ascii_case(local) && test(c))
}

// ==== Text ====

/// The element's text, trimmed.
fn text(e: &Element) -> String {
    e.text().trim().to_string()
}

/// An RSS field that holds HTML: its text (escaped or CDATA HTML source), or - when a broken
/// feed put the markup in unescaped - the markup written back from the tree.
fn html_of(e: &Element) -> String {
    if e.has_element_children() {
        e.inner_markup().trim().to_string()
    } else {
        e.text().trim().to_string()
    }
}

/// An RSS title as plain text (HTML in practice when it holds `&` or `<`).
fn title_text(e: Option<&Element>) -> String {
    let Some(e) = e else {
        return String::new();
    };
    let raw = html_of(e);
    if raw.contains('<') || raw.contains('&') {
        reader::plain_text(&raw)
    } else {
        reader::collapse(&raw)
    }
}

/// A person as RSS writes one: `mail@example.org (Name)` and `Name <mail@example.org>` are the
/// name; anything else as written.
fn person(raw: &str) -> String {
    let t = reader::collapse(raw);
    if let Some(open) = t.find('(') {
        if t.ends_with(')') && t[..open].contains('@') {
            let name = t[open + 1..t.len() - 1].trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    if let Some(open) = t.find('<') {
        if t.ends_with('>') && t[open..].contains('@') {
            let name = t[..open].trim().trim_matches('"').trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    t
}

/// The first value that is not blank.
fn first_nonempty<const N: usize>(values: [String; N]) -> Option<String> {
    values.into_iter().find(|v| !v.trim().is_empty())
}

/// The id of an item without guid, id or link: a hash of what it says (the same every time).
fn hashed_id(title: &str, date: Option<i64>, summary: &str, content: &str) -> String {
    let key = format!("{title}\n{}\n{summary}\n{content}", date.unwrap_or(0));
    let hash = azul_storage::sigv4::sha256_hex(key.as_bytes());
    format!("sha256:{}", &hash[..hash.len().min(32)])
}

/// An enclosure from its address, type and length (`None` without an address).
fn enclosure(
    href: Option<&str>,
    mime: Option<&str>,
    length: Option<&str>,
    base: &str,
) -> Option<Enclosure> {
    let url = links::resolve(base, href?);
    if url.is_empty() {
        return None;
    }
    Some(Enclosure {
        url,
        mime: mime.unwrap_or("").trim().to_string(),
        length: length.and_then(|l| l.trim().parse().ok()).unwrap_or(0),
    })
}

/// A Media RSS picture: `media:thumbnail`, an image `media:content`, the same in a
/// `media:group` (YouTube).
fn media_image(e: &Element, base: &str) -> Option<String> {
    fn in_element(e: &Element) -> Option<&str> {
        child(e, "thumbnail", media)
            .and_then(|t| t.attr("url"))
            .or_else(|| {
                children(e, "content", media)
                    .find(|c| {
                        c.attr("medium")
                            .is_some_and(|m| m.eq_ignore_ascii_case("image"))
                            || c.attr("type")
                                .is_some_and(|t| t.trim().starts_with("image/"))
                    })
                    .and_then(|c| c.attr("url"))
            })
    }
    in_element(e)
        .or_else(|| child(e, "group", media).and_then(in_element))
        .map(|u| links::resolve(base, u))
        .filter(|u| !u.is_empty())
}

/// A Media RSS description (YouTube's text) as HTML.
fn media_description(e: &Element) -> Option<String> {
    child(e, "description", media)
        .or_else(|| child(e, "group", media).and_then(|g| child(g, "description", media)))
        .map(|d| reader::text_to_html(&d.text()))
        .filter(|s| !s.is_empty())
}

/// The excerpt filled in; `None` for an item that says nothing at all.
fn finish(mut item: Item) -> Option<Item> {
    if item.title.is_empty()
        && item.link.is_empty()
        && item.summary.trim().is_empty()
        && item.content.trim().is_empty()
    {
        return None;
    }
    let excerpt = {
        let source = if item.summary.trim().is_empty() {
            &item.content
        } else {
            &item.summary
        };
        reader::excerpt(source, EXCERPT_CHARS)
    };
    item.excerpt = excerpt;
    Some(item)
}

// ==== RSS ====

/// RSS 0.9x / 2.0 (`channel` holds the items) and RSS 1.0 (the items beside it); a broken
/// feed's items right under the root are read too.
fn parse_rss(root: &Element, channel: Option<&Element>, url: &str, format: Format) -> Feed {
    let channel = channel.unwrap_or(root);
    let site = child(channel, "link", rss_core)
        .map(text)
        .map(|l| links::resolve(url, &l))
        .unwrap_or_default();
    let icon = child(channel, "image", rss_core)
        .and_then(|i| child(i, "url", rss_core))
        .map(text)
        .or_else(|| {
            child(channel, "image", itunes)
                .and_then(|i| i.attr("href"))
                .map(str::to_string)
        })
        .map(|i| links::resolve(url, &i))
        .unwrap_or_default();
    let feed_author = child(channel, "author", itunes)
        .or_else(|| child(channel, "creator", dc))
        .map(|a| person(&a.text()))
        .unwrap_or_default();
    let mut elements: Vec<&Element> = children(channel, "item", rss_core).collect();
    if !std::ptr::eq(channel, root) {
        elements.extend(children(root, "item", rss_core));
    }
    let items = elements
        .into_iter()
        .filter_map(|e| rss_item(e, url, &feed_author))
        .collect();
    Feed {
        format,
        title: title_text(child(channel, "title", rss_core)),
        site,
        description: child(channel, "description", rss_core)
            .map(|d| reader::plain_text(&html_of(d)))
            .unwrap_or_default(),
        icon,
        items,
        problems: Vec::new(),
    }
}

fn rss_item(e: &Element, url: &str, feed_author: &str) -> Option<Item> {
    let guid_element = child(e, "guid", rss_core);
    let guid = guid_element.map(text).unwrap_or_default();
    let permalink = guid_element.is_some_and(|g| {
        !g.attr("isPermaLink")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("false"))
    });
    let link_text = child(e, "origLink", feedburner)
        .map(text)
        .filter(|l| !l.is_empty())
        .or_else(|| {
            child(e, "link", rss_core)
                .map(text)
                .filter(|l| !l.is_empty())
        })
        .or_else(|| {
            child(e, "link", atom_ns)
                .and_then(|l| l.attr("href"))
                .map(str::to_string)
        })
        .or_else(|| (permalink && links::is_web(&guid)).then(|| guid.clone()))
        .unwrap_or_default();
    let link = links::resolve(url, &link_text);
    let about = e.attr("rdf:about").map(str::to_string).unwrap_or_default();
    let title = title_text(child(e, "title", rss_core).or_else(|| child(e, "title", dc)));
    let summary = child(e, "description", rss_core)
        .map(html_of)
        .unwrap_or_default();
    let content = child(e, "encoded", content_ns)
        .map(html_of)
        .unwrap_or_default();
    let author = child(e, "author", rss_core)
        .or_else(|| child(e, "creator", dc))
        .or_else(|| child(e, "author", itunes))
        .map(|a| person(&a.text()))
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| feed_author.to_string());
    let published = child(e, "pubDate", rss_core)
        .or_else(|| child(e, "date", dc))
        .or_else(|| child(e, "issued", dc))
        .and_then(|d| parse_date(&d.text()));
    let updated = child(e, "updated", atom_ns)
        .or_else(|| child(e, "modified", dc))
        .and_then(|d| parse_date(&d.text()));
    let enclosures: Vec<Enclosure> = children(e, "enclosure", rss_core)
        .filter_map(|x| enclosure(x.attr("url"), x.attr("type"), x.attr("length"), url))
        .collect();
    let image = media_image(e, url)
        .or_else(|| {
            child(e, "image", itunes)
                .and_then(|i| i.attr("href"))
                .map(|h| links::resolve(url, h))
        })
        .or_else(|| {
            enclosures
                .iter()
                .find(|x| x.mime.starts_with("image/"))
                .map(|x| x.url.clone())
        })
        .unwrap_or_default();
    let categories = children(e, "category", rss_core)
        .chain(children(e, "subject", dc))
        .map(text)
        .filter(|c| !c.is_empty())
        .collect();
    let base = if link.is_empty() {
        url.to_string()
    } else {
        link.clone()
    };
    let id = first_nonempty([guid, about, link.clone()])
        .unwrap_or_else(|| hashed_id(&title, published.or(updated), &summary, &content));
    finish(Item {
        id,
        title,
        link,
        author,
        published,
        updated,
        summary,
        content,
        excerpt: String::new(),
        base,
        image,
        enclosures,
        categories,
        seen: 0,
    })
}

// ==== Atom ====

/// How an Atom text construct is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Text,
    Html,
    Xhtml,
}

/// The construct's kind: `type` (Atom 1.0: `text` / `html` / `xhtml`; 0.3: a MIME type) and
/// 0.3's `mode` (`escaped` / `xml`); unsaid, markup inside it is XHTML.
fn atom_kind(e: &Element) -> TextKind {
    let kind = e.attr("type").unwrap_or("").trim().to_ascii_lowercase();
    let mode = e.attr("mode").unwrap_or("").trim().to_ascii_lowercase();
    if kind == "xhtml"
        || kind == "application/xhtml+xml"
        || mode == "xml" && e.has_element_children()
    {
        TextKind::Xhtml
    } else if kind == "html" || kind == "text/html" || mode == "escaped" {
        TextKind::Html
    } else if e.has_element_children() {
        TextKind::Xhtml
    } else {
        TextKind::Text
    }
}

/// An Atom text construct as plain text.
fn atom_text(e: &Element) -> String {
    match atom_kind(e) {
        TextKind::Html => reader::plain_text(&e.text()),
        TextKind::Text | TextKind::Xhtml => reader::collapse(&e.text()),
    }
}

/// An Atom text construct as HTML (XHTML: the markup inside its `div`).
fn atom_html(e: &Element) -> String {
    match atom_kind(e) {
        TextKind::Text => reader::text_to_html(&e.text()),
        TextKind::Html => e.text().trim().to_string(),
        TextKind::Xhtml => match e
            .elements()
            .find(|c| c.local_name().eq_ignore_ascii_case("div"))
        {
            Some(div) => div.inner_markup().trim().to_string(),
            None => e.inner_markup().trim().to_string(),
        },
    }
}

/// An Atom person's name (else the email, else the text).
fn atom_person(a: &Element) -> String {
    child(a, "name", atomish)
        .map(text)
        .filter(|n| !n.is_empty())
        .or_else(|| child(a, "email", atomish).map(text))
        .unwrap_or_else(|| reader::collapse(&a.text()))
}

/// `base` with the element's `xml:base` applied.
fn xml_base(base: &str, e: &Element) -> String {
    match e.attr("xml:base") {
        Some(b) => links::resolve(base, b),
        None => base.to_string(),
    }
}

/// The `alternate` link (a missing `rel` is one), an HTML one first.
fn alternate_link(e: &Element) -> Option<&str> {
    let mut any_alternate = None;
    for l in children(e, "link", atomish) {
        let rel = l.attr("rel").map_or_else(
            || "alternate".to_string(),
            |r| r.trim().to_ascii_lowercase(),
        );
        if rel != "alternate" {
            continue;
        }
        let Some(href) = l.attr("href") else {
            continue;
        };
        if l.attr("type").map_or(true, |t| t.contains("html")) {
            return Some(href);
        }
        if any_alternate.is_none() {
            any_alternate = Some(href);
        }
    }
    any_alternate
}

fn parse_atom(root: &Element, url: &str) -> Feed {
    let format = if ns(root) == Ns::Atom03 {
        Format::Atom03
    } else {
        Format::Atom
    };
    let base = xml_base(url, root);
    let feed_author = child(root, "author", atomish)
        .map(atom_person)
        .unwrap_or_default();
    let site = alternate_link(root)
        .map(|h| links::resolve(&base, h))
        .unwrap_or_default();
    let icon = child(root, "icon", atomish)
        .or_else(|| child(root, "logo", atomish))
        .map(|i| links::resolve(&base, &text(i)))
        .unwrap_or_default();
    let items = children(root, "entry", atomish)
        .filter_map(|e| atom_entry(e, &base, &feed_author))
        .collect();
    Feed {
        format,
        title: child(root, "title", atomish)
            .map(atom_text)
            .unwrap_or_default(),
        site,
        description: child(root, "subtitle", atomish)
            .or_else(|| child(root, "tagline", atomish))
            .map(atom_text)
            .unwrap_or_default(),
        icon,
        items,
        problems: Vec::new(),
    }
}

fn atom_entry(e: &Element, feed_base: &str, feed_author: &str) -> Option<Item> {
    let base = xml_base(feed_base, e);
    let link = alternate_link(e)
        .map(|h| links::resolve(&base, h))
        .unwrap_or_default();
    let id = child(e, "id", atomish).map(text).unwrap_or_default();
    let title = child(e, "title", atomish)
        .map(atom_text)
        .unwrap_or_default();
    let author = child(e, "author", atomish)
        .map(atom_person)
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| feed_author.to_string());
    let published = child(e, "published", atomish)
        .or_else(|| child(e, "issued", atomish))
        .or_else(|| child(e, "created", atomish))
        .and_then(|d| parse_date(&d.text()));
    let updated = child(e, "updated", atomish)
        .or_else(|| child(e, "modified", atomish))
        .and_then(|d| parse_date(&d.text()));
    let content_element = child(e, "content", atomish);
    let summary = child(e, "summary", atomish)
        .map(atom_html)
        .filter(|s| !s.is_empty())
        .or_else(|| media_description(e))
        .unwrap_or_default();
    let content = content_element
        .filter(|c| c.attr("src").is_none())
        .map(atom_html)
        .unwrap_or_default();
    let content_base = content_element.map_or_else(|| base.clone(), |c| xml_base(&base, c));
    let enclosures: Vec<Enclosure> = children(e, "link", atomish)
        .filter(|l| {
            l.attr("rel")
                .is_some_and(|r| r.trim().eq_ignore_ascii_case("enclosure"))
        })
        .filter_map(|l| enclosure(l.attr("href"), l.attr("type"), l.attr("length"), &base))
        .collect();
    let image = media_image(e, &base)
        .or_else(|| {
            enclosures
                .iter()
                .find(|x| x.mime.starts_with("image/"))
                .map(|x| x.url.clone())
        })
        .unwrap_or_default();
    let categories = children(e, "category", atomish)
        .filter_map(|c| {
            c.attr("label")
                .or_else(|| c.attr("term"))
                .map(|t| t.trim().to_string())
        })
        .filter(|c| !c.is_empty())
        .collect();
    let id = first_nonempty([id, link.clone()])
        .unwrap_or_else(|| hashed_id(&title, published.or(updated), &summary, &content));
    finish(Item {
        id,
        title,
        link,
        author,
        published,
        updated,
        summary,
        content,
        excerpt: String::new(),
        base: content_base,
        image,
        enclosures,
        categories,
        seen: 0,
    })
}

// ==== JSON Feed ====

/// A string field, trimmed (`""` when missing or not a string).
fn json_str(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

/// JSON Feed 1.1's `authors` (the first named) or 1.0's `author`.
fn json_author(v: &Value) -> String {
    v.get("authors")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find_map(|x| x.get("name").and_then(Value::as_str)))
        .or_else(|| {
            v.get("author")
                .and_then(|a| a.get("name"))
                .and_then(Value::as_str)
        })
        .map(|n| n.trim().to_string())
        .unwrap_or_default()
}

fn parse_json(text: &str, url: &str) -> Result<Feed, FeedError> {
    let value: Value = serde_json::from_str(text).map_err(|e| FeedError::Invalid(e.to_string()))?;
    let version = value.get("version").and_then(Value::as_str).unwrap_or("");
    if !version.contains("jsonfeed.org/version/1") {
        return Err(FeedError::NotAFeed { html: false });
    }
    let feed_author = json_author(&value);
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|i| json_item(i, url, &feed_author))
                .collect()
        })
        .unwrap_or_default();
    Ok(Feed {
        format: Format::JsonFeed,
        title: json_str(&value, "title"),
        site: links::resolve(url, &json_str(&value, "home_page_url")),
        description: json_str(&value, "description"),
        icon: links::resolve(
            url,
            &first_nonempty([json_str(&value, "icon"), json_str(&value, "favicon")])
                .unwrap_or_default(),
        ),
        items,
        problems: Vec::new(),
    })
}

fn json_item(i: &Value, url: &str, feed_author: &str) -> Option<Item> {
    let id = match i.get("id") {
        Some(Value::String(t)) => t.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let link = links::resolve(
        url,
        &first_nonempty([json_str(i, "url"), json_str(i, "external_url")]).unwrap_or_default(),
    );
    let content_html = json_str(i, "content_html");
    let content = if content_html.is_empty() {
        reader::text_to_html(&json_str(i, "content_text"))
    } else {
        content_html
    };
    let summary = reader::text_to_html(&json_str(i, "summary"));
    let published = parse_date(&json_str(i, "date_published"));
    let updated = parse_date(&json_str(i, "date_modified"));
    let image = links::resolve(
        url,
        &first_nonempty([json_str(i, "image"), json_str(i, "banner_image")]).unwrap_or_default(),
    );
    let author = Some(json_author(i))
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| feed_author.to_string());
    let categories = i
        .get("tags")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .filter_map(Value::as_str)
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let enclosures = i
        .get("attachments")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let mut e = enclosure(
                        x.get("url").and_then(Value::as_str),
                        x.get("mime_type").and_then(Value::as_str),
                        None,
                        url,
                    )?;
                    e.length = x.get("size_in_bytes").and_then(Value::as_u64).unwrap_or(0);
                    Some(e)
                })
                .collect()
        })
        .unwrap_or_default();
    let title = json_str(i, "title");
    let base = if link.is_empty() {
        url.to_string()
    } else {
        link.clone()
    };
    let id = first_nonempty([id, link.clone()])
        .unwrap_or_else(|| hashed_id(&title, published.or(updated), &summary, &content));
    finish(Item {
        id,
        title,
        link,
        author,
        published,
        updated,
        summary,
        content,
        excerpt: String::new(),
        base,
        image,
        enclosures,
        categories,
        seen: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(bytes: &[u8], url: &str) -> Feed {
        parse(bytes, "", url).unwrap_or_else(|e| panic!("{url}: {e}"))
    }

    fn wordpress() -> Feed {
        read(
            include_bytes!("../tests/fixtures/wordpress_rss2.xml"),
            "https://example.org/feed/",
        )
    }

    #[test]
    fn a_wordpress_rss_feed_reads_its_channel() {
        let feed = wordpress();
        assert_eq!(feed.format, Format::Rss2);
        assert_eq!(feed.title, "Example Weekly");
        assert_eq!(feed.site, "https://example.org/");
        assert_eq!(feed.description, "Notes on the open web & its plumbing");
        assert_eq!(
            feed.icon,
            "https://example.org/wp-content/uploads/icon-32x32.png"
        );
        assert_eq!(feed.items.len(), 3);
    }

    #[test]
    fn a_wordpress_item_has_its_guid_creator_categories_and_content() {
        let feed = wordpress();
        let item = &feed.items[0];
        assert_eq!(item.id, "https://example.org/?p=123");
        assert_eq!(item.title, "The quiet return of RSS");
        assert_eq!(
            item.link,
            "https://example.org/2026/09/30/the-quiet-return-of-rss/"
        );
        assert_eq!(item.author, "Mara Schulz");
        assert_eq!(item.published, Some(1_790_757_720));
        assert_eq!(
            item.categories,
            vec!["Essays".to_string(), "Web".to_string()]
        );
        assert!(
            item.content
                .contains("<img src=\"/wp-content/uploads/2026/09/hero.jpg\""),
            "{}",
            item.content
        );
        assert!(
            item.summary.starts_with("<p>For a decade"),
            "{}",
            item.summary
        );
        assert!(
            item.excerpt
                .starts_with("For a decade the obituaries were written weekly."),
            "{}",
            item.excerpt
        );
        assert_eq!(
            item.base, item.link,
            "an RSS item's content points from the item's address"
        );
        assert_eq!(item.body(), item.content);
    }

    #[test]
    fn a_bare_ampersand_in_a_link_and_references_in_a_title_are_read() {
        let feed = wordpress();
        let item = &feed.items[1];
        assert_eq!(item.title, "Tom & Jerry\u{2019}s guide to feeds");
        assert_eq!(item.link, "https://example.org/?p=122&lang=en");
        assert_eq!(item.author, "Ben Kr\u{fc}ger");
        assert_eq!(item.published, Some(1_790_701_500));
        assert_eq!(item.excerpt, "Short & sweet: a guide.");
        assert_eq!(
            item.body(),
            item.summary,
            "no content: the summary is the body"
        );
        assert_eq!(feed.items[2].published, Some(1_790_586_000));
        assert_eq!(feed.items[2].summary, "");
    }

    #[test]
    fn an_atom_feed_resolves_xml_base_and_reads_html_and_xhtml_content() {
        let feed = read(
            include_bytes!("../tests/fixtures/blogger_atom.xml"),
            "https://blog.example.net/feeds/posts/default",
        );
        assert_eq!(feed.format, Format::Atom);
        assert_eq!(feed.title, "Rust & Feeds");
        assert_eq!(feed.site, "https://blog.example.net/");
        assert_eq!(feed.description, "Notes from the workshop");
        assert_eq!(feed.icon, "https://blog.example.net/favicon.ico");
        assert_eq!(feed.items.len(), 2);
        let first = &feed.items[0];
        assert_eq!(first.id, "tag:blogger.com,1999:blog-4242.post-1");
        assert_eq!(first.title, "Rust 2026 survey results");
        assert_eq!(
            first.link, "https://blog.example.net/2026/09/rust-survey.html",
            "the alternate link, not replies"
        );
        assert_eq!(first.author, "Jonas Weber");
        assert_eq!(first.published, Some(1_790_755_200));
        assert_eq!(first.updated, Some(1_790_756_100));
        assert_eq!(first.categories, vec!["rust".to_string()]);
        assert_eq!(
            first.content,
            "<p>The survey is in.</p><img src=\"images/chart.png\" alt=\"Chart\">"
        );
        assert_eq!(
            first.base, "https://blog.example.net/",
            "the xml:base in effect"
        );
        assert_eq!(first.image, "https://blog.example.net/images/chart-s72.png");
        let second = &feed.items[1];
        assert_eq!(
            second.title, "Why Option matters",
            "an html title is plain text"
        );
        assert_eq!(
            second.link,
            "https://other.example.net/mirror/why-option.html"
        );
        assert_eq!(second.base, "https://other.example.net/mirror/");
        assert_eq!(second.author, "Ida Novak", "the feed's author");
        assert_eq!(second.published, None);
        assert_eq!(second.updated, Some(1_790_668_800));
        assert_eq!(second.date(), 1_790_668_800);
        assert_eq!(
            second.summary, "A summary in plain text &lt;not a tag&gt;",
            "a text summary is escaped"
        );
        assert_eq!(second.excerpt, "A summary in plain text <not a tag>");
        assert_eq!(
            second.content,
            "<p>An <em>xhtml</em> body with <a href=\"notes.html\">a relative link</a>.</p>",
            "xhtml content is the div's markup"
        );
    }

    #[test]
    fn a_youtube_feed_takes_its_picture_and_text_from_the_media_group() {
        let feed = read(
            include_bytes!("../tests/fixtures/youtube_atom.xml"),
            "https://www.youtube.com/feeds/videos.xml",
        );
        assert_eq!(feed.title, "Example Talks");
        assert_eq!(
            feed.site,
            "https://www.youtube.com/channel/UC0000000000000000000000"
        );
        let video = &feed.items[0];
        assert_eq!(video.id, "yt:video:abcdefghijk");
        assert_eq!(video.link, "https://www.youtube.com/watch?v=abcdefghijk");
        assert_eq!(
            video.image,
            "https://i1.ytimg.com/vi/abcdefghijk/hqdefault.jpg"
        );
        assert_eq!(video.published, Some(1_790_757_720));
        assert_eq!(
            video.summary,
            "A talk about feeds.<br/>Second line &amp; more."
        );
        assert_eq!(video.excerpt, "A talk about feeds. Second line & more.");
    }

    #[test]
    fn an_rss_1_0_feed_reads_the_items_beside_its_channel() {
        let feed = read(
            include_bytes!("../tests/fixtures/rss1_rdf.xml"),
            "https://lwn.example.org/headlines/rss",
        );
        assert_eq!(feed.format, Format::Rss1);
        assert_eq!(feed.title, "LWN.example");
        assert_eq!(feed.site, "https://lwn.example.org/");
        assert_eq!(feed.items.len(), 2);
        let first = &feed.items[0];
        assert_eq!(first.id, "https://lwn.example.org/Articles/1001/");
        assert_eq!(first.author, "corbet");
        assert_eq!(first.published, Some(1_790_787_600));
        assert_eq!(first.content, "<p>The merge window is <b>closed</b>.</p>");
        assert_eq!(first.summary, "The merge window is closed.");
        assert_eq!(feed.items[1].published, Some(1_790_775_000));
    }

    #[test]
    fn a_json_feed_1_1_reads_its_authors_and_items_without_titles() {
        let feed = read(
            include_bytes!("../tests/fixtures/json_feed_11.json"),
            "https://micro.example.org/feed.json",
        );
        assert_eq!(feed.format, Format::JsonFeed);
        assert_eq!(feed.title, "Micro Notes");
        assert_eq!(feed.site, "https://micro.example.org/");
        assert_eq!(feed.icon, "https://micro.example.org/icon-512.png");
        let note = &feed.items[0];
        assert_eq!(note.title, "");
        assert_eq!(note.author, "Nora Peters", "the feed's authors");
        assert_eq!(note.published, Some(1_790_757_720));
        assert_eq!(note.excerpt, "Coffee & feeds this morning.");
        assert_eq!(
            note.categories,
            vec!["coffee".to_string(), "feeds".to_string()]
        );
        let post = &feed.items[1];
        assert_eq!(post.id, "1002", "a number for an id");
        assert_eq!(
            post.link, "https://micro.example.org/2026/09/29/longer/",
            "against the feed's address"
        );
        assert_eq!(post.summary, "What this is about.");
        assert_eq!(post.content, "<p>Body.</p>");
        assert_eq!(post.image, "https://micro.example.org/uploads/banner.jpg");
        assert_eq!(post.author, "Guest Writer");
        assert_eq!(post.published, Some(1_790_668_800));
        assert_eq!(post.updated, Some(1_790_672_400));
        assert_eq!(post.excerpt, "What this is about.");
        assert_eq!(
            post.enclosures,
            vec![Enclosure {
                url: "https://micro.example.org/ep1.mp3".into(),
                mime: "audio/mpeg".into(),
                length: 1_234_567
            }]
        );
    }

    #[test]
    fn a_json_feed_1_0_with_text_content_becomes_escaped_paragraphs() {
        let feed = read(
            include_bytes!("../tests/fixtures/json_feed_10.json"),
            "https://old.example.org/feed.json",
        );
        let item = &feed.items[0];
        assert_eq!(
            item.title, "First <post>",
            "a JSON title is plain text as written"
        );
        assert_eq!(item.author, "Karl Braun");
        assert_eq!(
            item.content,
            "<p>Line one &amp; two.</p><p>Second paragraph.</p>"
        );
        assert_eq!(item.published, Some(1_790_622_000));
    }

    #[test]
    fn a_podcast_feed_keeps_its_enclosures_and_artwork() {
        let feed = read(
            include_bytes!("../tests/fixtures/podcast_rss.xml"),
            "https://podcast.example.org/feed.xml",
        );
        assert_eq!(feed.icon, "https://podcast.example.org/artwork.jpg");
        let episode = &feed.items[0];
        assert_eq!(
            episode.link, "https://podcast.example.org/12",
            "a permalink guid is the link"
        );
        assert_eq!(episode.author, "Emil Vogel", "the channel's itunes:author");
        assert_eq!(episode.summary, "<p>We talk about <em>OPML</em>.</p>");
        assert_eq!(episode.published, Some(1_790_757_720));
        assert_eq!(episode.image, "https://podcast.example.org/ep12.jpg");
        assert_eq!(
            episode.enclosures,
            vec![Enclosure {
                url: "https://podcast.example.org/ep12.mp3".into(),
                mime: "audio/mpeg".into(),
                length: 34_216_300
            }]
        );
    }

    #[test]
    fn a_windows_1252_feed_is_decoded() {
        let feed = read(
            include_bytes!("../tests/fixtures/latin1_rss.xml"),
            "https://cafe.example.de/feed",
        );
        assert_eq!(feed.title, "Caf\u{e9} M\u{fc}ller");
        let item = &feed.items[0];
        assert_eq!(item.title, "Neue Torten f\u{fc}r den Herbst");
        assert_eq!(
            item.excerpt,
            "K\u{e4}sekuchen & Apfelstrudel \u{2013} frisch."
        );
        assert_eq!(
            item.published,
            Some(1_790_755_200),
            "a German weekday is ignored"
        );
    }

    #[test]
    fn a_broken_hand_written_feed_is_read_as_far_as_it_goes() {
        let feed = read(
            include_bytes!("../tests/fixtures/broken_unescaped.xml"),
            "http://news.example.net/rss.xml",
        );
        assert_eq!(feed.title, "Local News & Weather");
        assert_eq!(feed.description, "A hand-written feed \u{a9} 2026");
        assert_eq!(feed.items.len(), 3, "{:#?}", feed.items);
        let bakery = &feed.items[0];
        assert_eq!(
            bakery.link,
            "http://news.example.net/story.php?id=7&ref=rss"
        );
        assert_eq!(bakery.id, bakery.link, "no guid: the link");
        assert_eq!(
            bakery.summary,
            "<p>The bakery opened <b>today</b>.<br/>Queues all morning.</p><img src=\"/img/bread.jpg\"/>",
            "unescaped markup is written back"
        );
        assert_eq!(
            bakery.image, "http://news.example.net/img/bread-small.jpg",
            "media: without its declaration"
        );
        assert_eq!(bakery.published, Some(1_790_769_600));
        let road = &feed.items[1];
        assert_eq!(road.title, "Road works on the B12", "an item closed early");
        assert!(road.id.starts_with("sha256:"), "{}", road.id);
        assert_eq!(road.published, None);
        let weather = &feed.items[2];
        assert_eq!(
            weather.excerpt, "Sunny \u{2014} 24\u{b0}C",
            "HTML's names without a DTD"
        );
        assert_eq!(weather.published, Some(1_790_740_800));
    }

    #[test]
    fn the_id_of_an_item_without_guid_or_link_is_the_same_every_time() {
        let bytes = include_bytes!("../tests/fixtures/broken_unescaped.xml");
        let a = read(bytes, "http://news.example.net/rss.xml");
        let b = read(bytes, "http://news.example.net/rss.xml");
        assert_eq!(a.items[1].id, b.items[1].id);
        assert_ne!(a.items[1].id, a.items[2].id);
    }

    #[test]
    fn a_cut_off_feed_keeps_the_entries_before_the_cut() {
        let feed = read(
            include_bytes!("../tests/fixtures/truncated_atom.xml"),
            "https://science.example.org/atom.xml",
        );
        assert_eq!(feed.title, "Science Daily Example");
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].link, "https://science.example.org/reefs");
        assert_eq!(feed.items[0].updated, Some(1_790_766_000));
        assert_eq!(feed.items[1].summary, "The connection dropped in the mid");
        assert!(!feed.problems.is_empty());
    }

    #[test]
    fn an_rss_0_91_feed_with_its_doctype_is_read() {
        let feed = read(
            include_bytes!("../tests/fixtures/rss091.xml"),
            "http://cooking.example.com/rss.xml",
        );
        assert_eq!(feed.format, Format::Rss09);
        assert_eq!(feed.icon, "http://cooking.example.com/logo.gif");
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].title, "Grandma's apple pie");
        assert_eq!(feed.items[0].id, "http://cooking.example.com/pie.html");
        assert_eq!(feed.items[1].summary, "");
    }

    #[test]
    fn a_web_page_and_other_data_are_no_feeds() {
        assert_eq!(
            parse(
                include_bytes!("../tests/fixtures/html_page.html"),
                "text/html",
                "https://example.org/"
            ),
            Err(FeedError::NotAFeed { html: true })
        );
        assert_eq!(
            parse(
                include_bytes!("../tests/fixtures/garbage.txt"),
                "text/plain",
                "https://example.org/x"
            ),
            Err(FeedError::NotAFeed { html: false })
        );
        assert_eq!(
            parse(
                b"{\"version\": \"x\"}",
                "application/json",
                "https://example.org/x"
            ),
            Err(FeedError::NotAFeed { html: false })
        );
        assert!(matches!(
            parse(b"{ not json", "", "https://example.org/x"),
            Err(FeedError::Invalid(_))
        ));
        assert_eq!(
            parse(b"", "", "https://example.org/x"),
            Err(FeedError::NotAFeed { html: false })
        );
    }

    #[test]
    fn an_item_twice_in_a_feed_is_kept_once() {
        let feed = read(
            b"<rss version=\"2.0\"><channel><title>T</title>\
              <item><guid>a</guid><title>First</title></item>\
              <item><guid>a</guid><title>Again</title></item>\
              <item><guid>b</guid><title>Other</title></item></channel></rss>",
            "https://example.org/rss",
        );
        let titles: Vec<&str> = feed.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, vec!["First", "Other"]);
    }

    #[test]
    fn an_email_author_is_shown_by_its_name() {
        let feed = read(
            b"<rss version=\"2.0\"><channel><title>T</title><item><guid>a</guid>\
              <author>editor@example.org (Rosa Wolf)</author></item>\
              <item><guid>b</guid><author>desk@example.org</author></item></channel></rss>",
            "https://example.org/rss",
        );
        assert_eq!(feed.items[0].author, "Rosa Wolf");
        assert_eq!(feed.items[1].author, "desk@example.org");
    }
}
