//! AzNews: the feed reader of the Azlin apps (the plan:
//! azul-apps/planning/core/news-reader.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`xmltree`]: a feed's XML read leniently into a small tree (quick-xml; a bare `&`, a
//!   mismatched end tag or a cut-off feed keeps what was read), the charset found and decoded;
//! - [`dates`]: the dates feeds write (RFC 822 in its many real forms, RFC 3339, bare dates);
//! - [`feed`]: RSS 0.9x / 1.0 / 2.0, Atom 1.0 / 0.3 and JSON Feed 1.x into one model;
//! - [`opml`]: subscriptions in folders, OPML import and export;
//! - [`state`]: what was read, starred and kept for later;
//! - [`library`]: the subscriptions, their articles and their state - the views, the counts,
//!   the day groups, a refresh merged in;
//! - [`store`]: the files in the data tree (`news/...`);
//! - [`fetch`]: a conditional GET (ETag / Last-Modified) through azul-storage's Transport, the
//!   feed links of a web page;
//! - [`reader`]: an article's HTML through azul's HTML5-like parser into the reader view, and
//!   its pictures (`Xml::scan_external_resources`);
//! - [`sample`]: the `--sample` library.

pub mod dates;
pub mod feed;
pub mod fetch;
pub mod ids;
pub mod jobs;
pub mod library;
pub mod links;
pub mod opml;
pub mod reader;
pub mod sample;
pub mod state;
pub mod store;
pub mod xmltree;

/// The window (azul's PimShell: the feeds, the articles, the reader; Add feed, OPML, settings).
pub mod ui;

/// Starts AzNews (the switches are read from the command line).
pub fn start() {
    ui::start();
}
