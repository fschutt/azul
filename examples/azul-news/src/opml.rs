//! Subscriptions in folders, and OPML - the file every feed reader imports and exports.
//!
//! AzNews keeps its subscription list AS an OPML 2.0 file (`news/subscriptions.opml` in the data
//! tree): the user's list is a file any other reader opens, export is a copy of it. A feed is an
//! `<outline>` with an `xmlUrl`; an outline without one is a folder. AzNews writes its own id of
//! the feed (the folder `news/feeds/<id>/` with its articles and marks) as `azId`, an attribute
//! other readers ignore.
//!
//! Reading is lenient ([`crate::xmltree`]): OPML files are often hand-edited (a bare `&` in a
//! title, no `<head>`, `xmlurl` in lower case, a title only in `title`). Folders nested deeper
//! than one level are one folder whose name is the path (`Tech / Rust`). A feed listed twice is
//! kept once (the first). Writing escapes through azul's one encoder (`Xml::encode_attribute`).

use azul::xml::Xml;

use crate::xmltree::{self, Element};

/// One subscribed feed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subscription {
    /// AzNews' id of the feed (its folder in the data tree); empty in a file from elsewhere.
    pub id: String,
    pub title: String,
    /// The feed's address (`xmlUrl`).
    pub url: String,
    /// The website (`htmlUrl`).
    pub site: String,
    /// The folder (`""`: none; nested folders: `Tech / Rust`).
    pub folder: String,
}

/// The separator of a nested folder's path.
pub const FOLDER_SEPARATOR: &str = " / ";

/// The subscriptions of an OPML file, in its order.
///
/// # Errors
/// A sentence for the user when the file is no OPML.
pub fn parse(_bytes: &[u8]) -> Result<Vec<Subscription>, String> {
    let _unused: Option<(&Element, xmltree::Document)> = None;
    Err("RED".to_string())
}

/// The subscriptions as an OPML 2.0 file named `title`: the feeds without a folder first, then
/// each folder (in the order its first feed comes) with its feeds.
#[must_use]
pub fn write(_subscriptions: &[Subscription], _title: &str) -> String {
    let _encoder: Option<Xml> = None;
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(id: &str, title: &str, url: &str, folder: &str) -> Subscription {
        Subscription {
            id: id.to_string(),
            title: title.to_string(),
            url: url.to_string(),
            site: format!("{url}/site"),
            folder: folder.to_string(),
        }
    }

    #[test]
    fn an_opml_file_round_trips_its_folders_and_feeds() {
        let subs = vec![
            sub("id-1", "Example Weekly", "https://example.org/feed/", ""),
            sub("id-2", "Rust Blog", "https://blog.rust.example/feed.xml", "Tech"),
            sub("id-3", "LWN", "https://lwn.example.org/rss", "Tech"),
            sub("id-4", "Bread & Butter", "https://bread.example.net/atom.xml", "Cooking"),
        ];
        let text = write(&subs, "AzNews subscriptions");
        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{text}");
        assert!(text.contains("<opml version=\"2.0\">"), "{text}");
        assert!(text.contains("<title>AzNews subscriptions</title>"), "{text}");
        assert_eq!(parse(text.as_bytes()).expect("its own file"), subs);
    }

    #[test]
    fn titles_and_addresses_are_escaped() {
        let subs = vec![sub("x", "Tom & \"Jerry\" <3", "https://example.org/?a=1&b=2", "Fun & games")];
        let text = write(&subs, "T & T");
        assert!(text.contains("text=\"Tom &amp; &quot;Jerry&quot; &lt;3\""), "{text}");
        assert!(text.contains("xmlUrl=\"https://example.org/?a=1&amp;b=2\""), "{text}");
        assert!(text.contains("<title>T &amp; T</title>"), "{text}");
        assert_eq!(parse(text.as_bytes()).expect("its own file"), subs);
    }

    #[test]
    fn nested_folders_are_read_as_one_folder_path() {
        let opml = br#"<?xml version="1.0"?><opml version="1.0"><head><title>x</title></head><body>
            <outline text="Tech"><outline text="Rust">
              <outline type="rss" text="This Week in Rust" xmlUrl="https://this-week.example/rss.xml" htmlUrl="https://this-week.example/"/>
            </outline></outline>
            <outline type="rss" text="Top" xmlUrl="https://top.example/feed"/>
        </body></opml>"#;
        let subs = parse(opml).expect("OPML");
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].folder, "Tech / Rust");
        assert_eq!(subs[0].title, "This Week in Rust");
        assert_eq!(subs[0].site, "https://this-week.example/");
        assert_eq!(subs[0].id, "");
        assert_eq!(subs[1].folder, "");
    }

    #[test]
    fn a_hand_edited_opml_is_read_anyway() {
        let opml = b"<opml><body>\n\
            <outline title=\"Local News & Weather\" xmlurl=\"http://news.example.net/rss.xml\"/>\n\
            <outline text=\"No title given\" type=\"rss\" xmlUrl=\"  https://quiet.example.org/feed  \"/>\n\
            <outline type=\"rss\" xmlUrl=\"https://bare.example.org/feed\"/>\n\
            <outline text=\"Empty folder\"></outline>\n\
            <outline text=\"Twice\" xmlUrl=\"http://news.example.net/rss.xml\"/>\n\
            <outline text=\"No address\" xmlUrl=\"\"/>\n\
            </body>";
        let subs = parse(opml).expect("a lenient read");
        let titles: Vec<&str> = subs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Local News & Weather", "No title given", "https://bare.example.org/feed"]);
        assert_eq!(subs[1].url, "https://quiet.example.org/feed", "trimmed");
    }

    #[test]
    fn what_is_not_opml_is_refused() {
        assert!(parse(b"<rss version=\"2.0\"><channel/></rss>").is_err());
        assert!(parse(b"just text").is_err());
        assert_eq!(parse(b"<opml version=\"2.0\"><head/><body/></opml>"), Ok(Vec::new()));
    }
}
