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
pub fn parse(_bytes: &[u8], _content_type: &str, _url: &str) -> Result<Feed, FeedError> {
    Err(FeedError::Invalid("RED".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(bytes: &[u8], url: &str) -> Feed {
        parse(bytes, "", url).unwrap_or_else(|e| panic!("{url}: {e}"))
    }

    fn wordpress() -> Feed {
        read(include_bytes!("../tests/fixtures/wordpress_rss2.xml"), "https://example.org/feed/")
    }

    #[test]
    fn a_wordpress_rss_feed_reads_its_channel() {
        let feed = wordpress();
        assert_eq!(feed.format, Format::Rss2);
        assert_eq!(feed.title, "Example Weekly");
        assert_eq!(feed.site, "https://example.org/");
        assert_eq!(feed.description, "Notes on the open web & its plumbing");
        assert_eq!(feed.icon, "https://example.org/wp-content/uploads/icon-32x32.png");
        assert_eq!(feed.items.len(), 3);
    }

    #[test]
    fn a_wordpress_item_has_its_guid_creator_categories_and_content() {
        let feed = wordpress();
        let item = &feed.items[0];
        assert_eq!(item.id, "https://example.org/?p=123");
        assert_eq!(item.title, "The quiet return of RSS");
        assert_eq!(item.link, "https://example.org/2026/09/30/the-quiet-return-of-rss/");
        assert_eq!(item.author, "Mara Schulz");
        assert_eq!(item.published, Some(1_790_757_720));
        assert_eq!(item.categories, vec!["Essays".to_string(), "Web".to_string()]);
        assert!(item.content.contains("<img src=\"/wp-content/uploads/2026/09/hero.jpg\""), "{}", item.content);
        assert!(item.summary.starts_with("<p>For a decade"), "{}", item.summary);
        assert!(item.excerpt.starts_with("For a decade the obituaries were written weekly."), "{}", item.excerpt);
        assert_eq!(item.base, item.link, "an RSS item's content points from the item's address");
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
        assert_eq!(item.body(), item.summary, "no content: the summary is the body");
        assert_eq!(feed.items[2].published, Some(1_790_586_000));
        assert_eq!(feed.items[2].summary, "");
    }

    #[test]
    fn an_atom_feed_resolves_xml_base_and_reads_html_and_xhtml_content() {
        let feed = read(include_bytes!("../tests/fixtures/blogger_atom.xml"), "https://blog.example.net/feeds/posts/default");
        assert_eq!(feed.format, Format::Atom);
        assert_eq!(feed.title, "Rust & Feeds");
        assert_eq!(feed.site, "https://blog.example.net/");
        assert_eq!(feed.description, "Notes from the workshop");
        assert_eq!(feed.icon, "https://blog.example.net/favicon.ico");
        assert_eq!(feed.items.len(), 2);
        let first = &feed.items[0];
        assert_eq!(first.id, "tag:blogger.com,1999:blog-4242.post-1");
        assert_eq!(first.title, "Rust 2026 survey results");
        assert_eq!(first.link, "https://blog.example.net/2026/09/rust-survey.html", "the alternate link, not replies");
        assert_eq!(first.author, "Jonas Weber");
        assert_eq!(first.published, Some(1_790_755_200));
        assert_eq!(first.updated, Some(1_790_756_100));
        assert_eq!(first.categories, vec!["rust".to_string()]);
        assert_eq!(first.content, "<p>The survey is in.</p><img src=\"images/chart.png\" alt=\"Chart\">");
        assert_eq!(first.base, "https://blog.example.net/", "the xml:base in effect");
        assert_eq!(first.image, "https://blog.example.net/images/chart-s72.png");
        let second = &feed.items[1];
        assert_eq!(second.title, "Why Option matters", "an html title is plain text");
        assert_eq!(second.link, "https://other.example.net/mirror/why-option.html");
        assert_eq!(second.base, "https://other.example.net/mirror/");
        assert_eq!(second.author, "Ida Novak", "the feed's author");
        assert_eq!(second.published, None);
        assert_eq!(second.updated, Some(1_790_668_800));
        assert_eq!(second.date(), 1_790_668_800);
        assert_eq!(second.summary, "A summary in plain text &lt;not a tag&gt;", "a text summary is escaped");
        assert_eq!(second.excerpt, "A summary in plain text <not a tag>");
        assert_eq!(
            second.content,
            "<p>An <em>xhtml</em> body with <a href=\"notes.html\">a relative link</a>.</p>",
            "xhtml content is the div's markup"
        );
    }

    #[test]
    fn a_youtube_feed_takes_its_picture_and_text_from_the_media_group() {
        let feed = read(include_bytes!("../tests/fixtures/youtube_atom.xml"), "https://www.youtube.com/feeds/videos.xml");
        assert_eq!(feed.title, "Example Talks");
        assert_eq!(feed.site, "https://www.youtube.com/channel/UC0000000000000000000000");
        let video = &feed.items[0];
        assert_eq!(video.id, "yt:video:abcdefghijk");
        assert_eq!(video.link, "https://www.youtube.com/watch?v=abcdefghijk");
        assert_eq!(video.image, "https://i1.ytimg.com/vi/abcdefghijk/hqdefault.jpg");
        assert_eq!(video.published, Some(1_790_757_720));
        assert_eq!(video.summary, "A talk about feeds.<br/>Second line &amp; more.");
        assert_eq!(video.excerpt, "A talk about feeds. Second line & more.");
    }

    #[test]
    fn an_rss_1_0_feed_reads_the_items_beside_its_channel() {
        let feed = read(include_bytes!("../tests/fixtures/rss1_rdf.xml"), "https://lwn.example.org/headlines/rss");
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
        let feed = read(include_bytes!("../tests/fixtures/json_feed_11.json"), "https://micro.example.org/feed.json");
        assert_eq!(feed.format, Format::JsonFeed);
        assert_eq!(feed.title, "Micro Notes");
        assert_eq!(feed.site, "https://micro.example.org/");
        assert_eq!(feed.icon, "https://micro.example.org/icon-512.png");
        let note = &feed.items[0];
        assert_eq!(note.title, "");
        assert_eq!(note.author, "Nora Peters", "the feed's authors");
        assert_eq!(note.published, Some(1_790_757_720));
        assert_eq!(note.excerpt, "Coffee & feeds this morning.");
        assert_eq!(note.categories, vec!["coffee".to_string(), "feeds".to_string()]);
        let post = &feed.items[1];
        assert_eq!(post.id, "1002", "a number for an id");
        assert_eq!(post.link, "https://micro.example.org/2026/09/29/longer/", "against the feed's address");
        assert_eq!(post.summary, "What this is about.");
        assert_eq!(post.content, "<p>Body.</p>");
        assert_eq!(post.image, "https://micro.example.org/uploads/banner.jpg");
        assert_eq!(post.author, "Guest Writer");
        assert_eq!(post.published, Some(1_790_668_800));
        assert_eq!(post.updated, Some(1_790_672_400));
        assert_eq!(post.excerpt, "What this is about.");
        assert_eq!(
            post.enclosures,
            vec![Enclosure { url: "https://micro.example.org/ep1.mp3".into(), mime: "audio/mpeg".into(), length: 1_234_567 }]
        );
    }

    #[test]
    fn a_json_feed_1_0_with_text_content_becomes_escaped_paragraphs() {
        let feed = read(include_bytes!("../tests/fixtures/json_feed_10.json"), "https://old.example.org/feed.json");
        let item = &feed.items[0];
        assert_eq!(item.title, "First <post>", "a JSON title is plain text as written");
        assert_eq!(item.author, "Karl Braun");
        assert_eq!(item.content, "<p>Line one &amp; two.</p><p>Second paragraph.</p>");
        assert_eq!(item.published, Some(1_790_622_000));
    }

    #[test]
    fn a_podcast_feed_keeps_its_enclosures_and_artwork() {
        let feed = read(include_bytes!("../tests/fixtures/podcast_rss.xml"), "https://podcast.example.org/feed.xml");
        assert_eq!(feed.icon, "https://podcast.example.org/artwork.jpg");
        let episode = &feed.items[0];
        assert_eq!(episode.link, "https://podcast.example.org/12", "a permalink guid is the link");
        assert_eq!(episode.author, "Emil Vogel", "the channel's itunes:author");
        assert_eq!(episode.summary, "<p>We talk about <em>OPML</em>.</p>");
        assert_eq!(episode.published, Some(1_790_757_720));
        assert_eq!(episode.image, "https://podcast.example.org/ep12.jpg");
        assert_eq!(
            episode.enclosures,
            vec![Enclosure { url: "https://podcast.example.org/ep12.mp3".into(), mime: "audio/mpeg".into(), length: 34_216_300 }]
        );
    }

    #[test]
    fn a_windows_1252_feed_is_decoded() {
        let feed = read(include_bytes!("../tests/fixtures/latin1_rss.xml"), "https://cafe.example.de/feed");
        assert_eq!(feed.title, "Caf\u{e9} M\u{fc}ller");
        let item = &feed.items[0];
        assert_eq!(item.title, "Neue Torten f\u{fc}r den Herbst");
        assert_eq!(item.excerpt, "K\u{e4}sekuchen & Apfelstrudel \u{2013} frisch.");
        assert_eq!(item.published, Some(1_790_755_200), "a German weekday is ignored");
    }

    #[test]
    fn a_broken_hand_written_feed_is_read_as_far_as_it_goes() {
        let feed = read(include_bytes!("../tests/fixtures/broken_unescaped.xml"), "http://news.example.net/rss.xml");
        assert_eq!(feed.title, "Local News & Weather");
        assert_eq!(feed.description, "A hand-written feed \u{a9} 2026");
        assert_eq!(feed.items.len(), 3, "{:#?}", feed.items);
        let bakery = &feed.items[0];
        assert_eq!(bakery.link, "http://news.example.net/story.php?id=7&ref=rss");
        assert_eq!(bakery.id, bakery.link, "no guid: the link");
        assert_eq!(
            bakery.summary,
            "<p>The bakery opened <b>today</b>.<br/>Queues all morning.</p><img src=\"/img/bread.jpg\"/>",
            "unescaped markup is written back"
        );
        assert_eq!(bakery.image, "http://news.example.net/img/bread-small.jpg", "media: without its declaration");
        assert_eq!(bakery.published, Some(1_790_769_600));
        let road = &feed.items[1];
        assert_eq!(road.title, "Road works on the B12", "an item closed early");
        assert!(road.id.starts_with("sha256:"), "{}", road.id);
        assert_eq!(road.published, None);
        let weather = &feed.items[2];
        assert_eq!(weather.excerpt, "Sunny \u{2014} 24\u{b0}C", "HTML's names without a DTD");
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
        let feed = read(include_bytes!("../tests/fixtures/truncated_atom.xml"), "https://science.example.org/atom.xml");
        assert_eq!(feed.title, "Science Daily Example");
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].link, "https://science.example.org/reefs");
        assert_eq!(feed.items[0].updated, Some(1_790_766_000));
        assert_eq!(feed.items[1].summary, "The connection dropped in the mid");
        assert!(!feed.problems.is_empty());
    }

    #[test]
    fn an_rss_0_91_feed_with_its_doctype_is_read() {
        let feed = read(include_bytes!("../tests/fixtures/rss091.xml"), "http://cooking.example.com/rss.xml");
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
            parse(include_bytes!("../tests/fixtures/html_page.html"), "text/html", "https://example.org/"),
            Err(FeedError::NotAFeed { html: true })
        );
        assert_eq!(
            parse(include_bytes!("../tests/fixtures/garbage.txt"), "text/plain", "https://example.org/x"),
            Err(FeedError::NotAFeed { html: false })
        );
        assert_eq!(parse(b"{\"version\": \"x\"}", "application/json", "https://example.org/x"), Err(FeedError::NotAFeed { html: false }));
        assert!(matches!(parse(b"{ not json", "", "https://example.org/x"), Err(FeedError::Invalid(_))));
        assert_eq!(parse(b"", "", "https://example.org/x"), Err(FeedError::NotAFeed { html: false }));
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
