//! The `--sample` library (on an empty data folder), as the plan describes it: 42 feeds in 6
//! folders, about 900 articles over 60 days - short notes and long essays with headings, quotes,
//! code and tables -, two broken feeds (an HTTP 404 and a feed that could not be read), a feed
//! without pictures and a picture-heavy one, read / starred / saved marks. Every address is
//! under example.org / example.net / example.com: nothing real is asked. Deterministic for a
//! given `now`: the same library every time.

use crate::{
    feed::Item,
    library::{FeedData, FeedMeta, Library, DAY},
    opml::Subscription,
    state::ReadState,
};

/// The folders and their feeds: (folder, [(title, host)]).
pub const FOLDERS: [(&str, &[(&str, &str)]); 6] = [
    (
        "Tech",
        &[
            ("Example Weekly", "weekly.example.org"),
            ("Rust Notes", "rust.example.org"),
            ("LWN.example", "lwn.example.net"),
            ("Kernel Diary", "kernel.example.com"),
            ("Web Platform News", "webplatform.example.org"),
            ("Databases Daily", "db.example.net"),
            ("The Compiler Room", "compilers.example.com"),
            ("Open Hardware", "hardware.example.org"),
            ("Security Digest", "security.example.net"),
            ("Small Tools", "tools.example.com"),
        ],
    ),
    (
        "Science",
        &[
            ("Science Daily Example", "science.example.org"),
            ("Ocean Watch", "ocean.example.net"),
            ("Star Charts", "stars.example.com"),
            ("Field Biology", "biology.example.org"),
            ("Climate Notes", "climate.example.net"),
            ("Maths Corner", "maths.example.com"),
            ("Lab Notebook", "lab.example.org"),
        ],
    ),
    (
        "Local",
        &[
            ("Local News & Weather", "news.example.net"),
            ("City Council", "council.example.org"),
            ("Market Hall", "market.example.com"),
            ("Neighbourhood Board", "board.example.org"),
            ("Transit Updates", "transit.example.net"),
            ("Library Events", "library.example.com"),
        ],
    ),
    (
        "Cooking",
        &[
            ("Bread & Butter", "bread.example.net"),
            ("Weeknight Dinners", "dinners.example.org"),
            ("The Spice Shelf", "spice.example.com"),
            ("Fermentation Club", "ferment.example.net"),
            ("Baking Science", "baking.example.org"),
            ("Soup Season", "soup.example.com"),
            ("Market to Table", "table.example.org"),
        ],
    ),
    (
        "Culture",
        &[
            ("Photo Diary", "photos.example.net"),
            ("Bookshelf", "books.example.org"),
            ("Film Notes", "film.example.com"),
            ("Museum Nights", "museum.example.org"),
            ("Typography Today", "type.example.net"),
            ("Architecture Walks", "walks.example.com"),
            ("Letters", "letters.example.org"),
        ],
    ),
    (
        "Podcasts",
        &[
            ("The Feed Show", "podcast.example.org"),
            ("Two Developers", "twodevs.example.net"),
            ("Kitchen Radio", "kitchenradio.example.com"),
            ("Night Sky Hour", "nightsky.example.org"),
            ("History Minutes", "history.example.net"),
        ],
    ),
];

/// The topics of each folder's feeds (in [`FOLDERS`]' order): every article carries one of its
/// folder's four, as a forum's post carries its subforum - what the source list groups by.
pub const TOPICS: [[&str; 4]; 6] = [
    ["Releases", "Security", "Tutorials", "Opinion"],
    ["Research", "Field notes", "Data", "Interviews"],
    ["Council", "Events", "Transit", "Weather"],
    ["Recipes", "Techniques", "Seasonal", "Reviews"],
    ["Books", "Film", "Exhibitions", "Essays"],
    ["Episodes", "Show notes", "Guests", "Announcements"],
];

/// The feed whose last refresh failed with an HTTP 404 (its index in the sample).
pub const BROKEN_404: usize = 20;
/// The feed whose last answer could not be read.
pub const BROKEN_INVALID: usize = 33;
/// The feed without any picture.
pub const NO_PICTURES: usize = 2;
/// The picture-heavy feed ("Photo Diary").
pub const PICTURES: usize = 30;

/// The sample library, its dates counted back from `now` (seconds since 1970).
#[must_use]
pub fn sample_library(now: i64) -> Library {
    let mut lib = Library::default();
    let mut index = 0usize;
    for (topics, (folder, feeds)) in TOPICS.iter().zip(FOLDERS) {
        for (title, host) in feeds {
            let i = index;
            index += 1;
            let site = format!("https://{host}/");
            let kind = if i % 3 == 0 {
                "Atom"
            } else if i % 7 == 0 {
                "JSON Feed"
            } else {
                "RSS 2.0"
            };
            lib.feeds.push(FeedData {
                sub: Subscription {
                    id: format!("00000000-0000-4000-8000-{:012x}", i + 1),
                    title: (*title).to_string(),
                    url: format!("https://{host}/feed.xml"),
                    site: site.clone(),
                    folder: folder.to_string(),
                    paused: false,
                },
                meta: FeedMeta {
                    title: (*title).to_string(),
                    site,
                    kind: kind.to_string(),
                    checked: now - 600,
                    updated: now - 3_600,
                    status: 200,
                    ..FeedMeta::default()
                },
                items: items(i, host, topics, now),
                state: ReadState::default(),
            });
        }
    }
    for (i, feed) in lib.feeds.iter_mut().enumerate() {
        for (j, item) in feed.items.iter().enumerate() {
            let age = now - item.date();
            if (age > 3 * DAY && (i + j) % 5 != 0) || (i + j) % 11 == 0 {
                feed.state.set_read(&item.id, true);
            }
            if (i * 31 + j) % 53 == 0 {
                feed.state.toggle_starred(&item.id);
            }
            if (i * 17 + j) % 151 == 0 {
                feed.state.toggle_later(&item.id);
            }
        }
    }
    lib.feeds[BROKEN_404].meta.error = "the server answered HTTP 404".to_string();
    lib.feeds[BROKEN_404].meta.status = 404;
    lib.feeds[BROKEN_INVALID].meta.error =
        "the feed could not be read: the input ends inside <item>".to_string();
    lib
}

const OPENERS: [&str; 10] = [
    "Notes on",
    "Why we still love",
    "A field guide to",
    "What changed in",
    "Ten years of",
    "The case for",
    "Rethinking",
    "A short history of",
    "How to start with",
    "Letters about",
];

const SUBJECTS: [&str; 16] = [
    "feeds",
    "the open web",
    "small tools",
    "a quiet morning",
    "the city",
    "old maps",
    "bread",
    "the night sky",
    "compilers",
    "the river",
    "typefaces",
    "a long walk",
    "the market",
    "tide pools",
    "winter soup",
    "the archive",
];

const AUTHORS: [&str; 8] = [
    "Mara Schulz",
    "Ben Kr\u{fc}ger",
    "Ida Novak",
    "Jonas Weber",
    "Nora Peters",
    "Emil Vogel",
    "Rosa Wolf",
    "Karl Braun",
];

/// Feed `i`'s articles: 12 to 31 of them, spread over the 60 days before `now`, newest first,
/// each in one of `topics`.
fn items(i: usize, host: &str, topics: &[&str; 4], now: i64) -> Vec<Item> {
    let count = 12 + (i * 7) % 20;
    (0..count)
        .map(|j| {
            let hours_back = (j * 60 * 24 / count + i % 7) as i64;
            let date = now - hours_back * 3_600 - ((i * 13 + j * 7) % 60) as i64 * 60;
            let opener = OPENERS[(i + j * 3) % OPENERS.len()];
            let subject = SUBJECTS[(i * 5 + j) % SUBJECTS.len()];
            let link = format!("https://{host}/2026/{}-{j}/", subject.replace(' ', "-"));
            let image = if i == PICTURES {
                format!("https://{host}/img/{j}.jpg")
            } else if i != NO_PICTURES && j % 6 == 0 {
                format!("https://{host}/img/hero-{j}.jpg")
            } else {
                String::new()
            };
            let long = (i + j) % 4 == 0;
            Item {
                id: format!("https://{host}/?p={}", 1000 + j),
                title: format!("{opener} {subject}"),
                author: AUTHORS[(i + j) % AUTHORS.len()].to_string(),
                published: Some(date),
                content: body(j, subject, long, &image),
                excerpt: format!(
                    "This is a note about {subject}. It was written for the sample library of AzNews, so nothing \
                     here is real, but the shape is."
                ),
                base: link.clone(),
                link,
                image,
                categories: vec![topics[(i + j) % topics.len()].to_string()],
                seen: date,
                ..Item::default()
            }
        })
        .collect()
}

/// An article's HTML: a note, or an essay with headings, a quote, code and a table.
fn body(j: usize, subject: &str, long: bool, image: &str) -> String {
    let mut html = format!(
        "<p>This is a note about {subject}. It was written for the sample library of AzNews, so nothing here is \
         real, but the shape is: paragraphs, a quote, sometimes code.</p>"
    );
    if !image.is_empty() {
        html.push_str(&format!(
            "<figure><img src=\"{image}\" alt=\"A picture of {subject}\"><figcaption>{subject}, seen from the \
             window</figcaption></figure>"
        ));
    }
    if long {
        html.push_str(&format!(
            "<h2>What happened</h2><p>For a decade the obituaries of {subject} were written weekly. Yet it kept \
             working, quietly, in the background of a thousand small habits.</p><blockquote><p>Feeds are the \
             plumbing of the open web.</p></blockquote><h2>The details</h2>"
        ));
        for k in 0..6 {
            html.push_str(&format!(
                "<p>Paragraph {} goes on about {subject} at the length an essay needs: a sentence that sets the \
                 scene, one that turns it around, and one that leaves the reader with a question about where \
                 this is going and why it matters at all.</p>",
                k + 1
            ));
        }
        if j % 3 == 0 {
            html.push_str(
                "<pre><code>fn main() {\n    println!(\"hello, feeds\");\n}</code></pre>",
            );
        }
        if j % 5 == 0 {
            html.push_str(
                "<table><tr><th>Day</th><th>Articles</th></tr><tr><td>Monday</td><td>12</td></tr>\
                 <tr><td>Tuesday</td><td>9</td></tr></table>",
            );
        }
    }
    html
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::library::View;

    /// Thursday, 1 October 2026, 12:00 UTC.
    const NOW: i64 = 1_790_856_000;

    #[test]
    fn the_sample_has_42_feeds_in_6_folders_and_about_900_articles() {
        let lib = sample_library(NOW);
        assert_eq!(lib.feeds.len(), 42);
        assert_eq!(lib.folders().len(), 6);
        let articles: usize = lib.feeds.iter().map(|f| f.items.len()).sum();
        assert!((850..=950).contains(&articles), "{articles} articles");
    }

    #[test]
    fn every_sample_feed_has_its_folders_four_topics() {
        let lib = sample_library(NOW);
        for (i, feed) in lib.feeds.iter().enumerate() {
            let topics = lib.topics(i);
            assert_eq!(topics.len(), 4, "{}", feed.sub.title);
            let folder = FOLDERS
                .iter()
                .position(|(name, _)| *name == feed.sub.folder)
                .expect("a sample folder");
            assert!(
                topics.iter().all(|t| TOPICS[folder].contains(&t.name.as_str())),
                "{}",
                feed.sub.title
            );
        }
    }

    #[test]
    fn the_sample_is_the_same_every_time() {
        assert_eq!(sample_library(NOW), sample_library(NOW));
    }

    #[test]
    fn the_sample_has_unread_starred_saved_and_broken_feeds() {
        let lib = sample_library(NOW);
        assert!(lib.unread_total() > 100, "{} unread", lib.unread_total());
        assert!(lib.unread_total() < 400, "{} unread", lib.unread_total());
        assert!(lib.starred_count() >= 10, "{} starred", lib.starred_count());
        assert!(lib.later_count() >= 3, "{} saved", lib.later_count());
        assert_eq!(lib.broken(), vec![BROKEN_404, BROKEN_INVALID]);
        assert!(lib.feeds[NO_PICTURES]
            .items
            .iter()
            .all(|i| i.image.is_empty() && !i.body().contains("<img")));
        assert!(lib.feeds[PICTURES]
            .items
            .iter()
            .all(|i| !i.image.is_empty()));
        assert!(!lib.list(&View::All, "").is_empty());
    }

    #[test]
    fn every_article_has_a_unique_id_a_body_and_a_date_in_the_last_60_days() {
        let lib = sample_library(NOW);
        for feed in &lib.feeds {
            let ids: HashSet<&str> = feed.items.iter().map(|i| i.id.as_str()).collect();
            assert_eq!(ids.len(), feed.items.len(), "{}", feed.sub.title);
            assert!(!feed.sub.id.is_empty());
            for item in &feed.items {
                assert!(!item.title.is_empty());
                assert!(!item.body().is_empty());
                assert!(!item.excerpt.is_empty());
                assert!(
                    item.date() <= NOW && item.date() >= NOW - 61 * DAY,
                    "{}",
                    item.title
                );
            }
        }
        let feed_ids: HashSet<&str> = lib.feeds.iter().map(|f| f.sub.id.as_str()).collect();
        assert_eq!(feed_ids.len(), 42);
    }
}
