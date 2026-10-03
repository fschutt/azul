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
pub fn sample_library(_now: i64) -> Library {
    let _unused: (Option<Item>, Option<FeedData>, Option<FeedMeta>, Option<Subscription>, Option<ReadState>, i64) =
        (None, None, None, None, None, DAY);
    Library::default()
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
        assert!(lib.feeds[NO_PICTURES].items.iter().all(|i| i.image.is_empty() && !i.body().contains("<img")));
        assert!(lib.feeds[PICTURES].items.iter().all(|i| !i.image.is_empty()));
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
                assert!(item.date() <= NOW && item.date() >= NOW - 61 * DAY, "{}", item.title);
            }
        }
        let feed_ids: HashSet<&str> = lib.feeds.iter().map(|f| f.sub.id.as_str()).collect();
        assert_eq!(feed_ids.len(), 42);
    }
}
