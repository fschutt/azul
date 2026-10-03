//! Asking for feeds over HTTP - through azul-storage's one seam ([`Transport`]: the app sends
//! with `AzulTransport`, azul's `HttpRequestConfig`, from an azul Thread; the tests with a fake).
//!
//! - A refresh is a CONDITIONAL GET: the `ETag` and `Last-Modified` of the last answer go back as
//!   `If-None-Match` / `If-Modified-Since`, and a `304 Not Modified` costs the server nothing
//!   and AzNews no parsing ([`fetch`] / [`Fetched::NotModified`]).
//! - "Add feed" takes what the user typed - a feed's address or a website's: an address without
//!   a scheme gets `https://`, `feed://` is `https://`. A web page is searched for the feeds it
//!   names (`<link rel="alternate" type="application/rss+xml" ...>`, read by azul's HTML parser:
//!   [`crate::reader::feed_links`]); each is fetched for its preview ([`find_feeds`]).
//! - The User-Agent names the app and its project, never a person ([`USER_AGENT`]).

use azul_storage::{HttpCall, HttpReply, Method, Transport};

use crate::{
    feed::{self, Feed, FeedError},
    links, reader, xmltree,
};

/// The User-Agent AzNews' requests carry (the transport sets it): the app and its project, no
/// personal data (house rule).
pub const USER_AGENT: &str = "AzNews/0.1 (+https://github.com/fschutt/azul)";

/// What AzNews accepts, feeds first.
pub const ACCEPT: &str =
    "application/rss+xml, application/atom+xml, application/feed+json, application/json;q=0.9, \
                          application/xml;q=0.8, text/xml;q=0.8, text/html;q=0.5, */*;q=0.3";

/// How many feeds a web page may name that are fetched for the preview.
pub const MAX_CANDIDATES: usize = 6;

/// What asking for a feed found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// 304: nothing new since the validators (they stay).
    NotModified { status: u16 },
    /// The feed, with the validators of this answer.
    Feed {
        feed: Feed,
        etag: String,
        last_modified: String,
        status: u16,
    },
    /// A web page and the feeds it names.
    Page { links: Vec<FeedLink> },
    /// Nothing usable: the status (0: no answer) and why, as a sentence.
    Failed { status: u16, error: String },
}

/// A feed a web page names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedLink {
    pub url: String,
    pub title: String,
    /// The link's `type` (`application/rss+xml`).
    pub mime: String,
}

/// A feed found for "Add feed", read for its preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub url: String,
    /// The title the page's link gave it (empty: none).
    pub link_title: String,
    pub feed: Feed,
    pub etag: String,
    pub last_modified: String,
}

/// The GET for a feed: what AzNews accepts and the validators of the last answer.
#[must_use]
pub fn request(url: &str, etag: &str, last_modified: &str) -> HttpCall {
    let mut headers = vec![("Accept".to_string(), ACCEPT.to_string())];
    if !etag.trim().is_empty() {
        headers.push(("If-None-Match".to_string(), etag.trim().to_string()));
    }
    if !last_modified.trim().is_empty() {
        headers.push((
            "If-Modified-Since".to_string(),
            last_modified.trim().to_string(),
        ));
    }
    HttpCall {
        method: Method::Get,
        url: url.to_string(),
        headers,
        body: Vec::new(),
        content_type: String::new(),
    }
}

/// An answer read: see [`Fetched`].
#[must_use]
pub fn interpret(url: &str, reply: &HttpReply) -> Fetched {
    if reply.status == 304 {
        return Fetched::NotModified { status: 304 };
    }
    if !reply.is_success() {
        return Fetched::Failed {
            status: reply.status,
            error: format!("the server answered HTTP {}", reply.status),
        };
    }
    let content_type = reply.header("content-type").unwrap_or("");
    match feed::parse(&reply.body, content_type, url) {
        Ok(feed) => Fetched::Feed {
            feed,
            etag: reply.header("etag").unwrap_or("").trim().to_string(),
            last_modified: reply
                .header("last-modified")
                .unwrap_or("")
                .trim()
                .to_string(),
            status: reply.status,
        },
        Err(FeedError::NotAFeed { html: true }) => {
            let text = xmltree::decode(&reply.body, content_type);
            Fetched::Page {
                links: reader::feed_links(&text, url),
            }
        }
        Err(e) => Fetched::Failed {
            status: reply.status,
            error: e.to_string(),
        },
    }
}

/// Asks for a feed - conditionally when the validators are known - and reads the answer.
#[must_use]
pub fn fetch(transport: &dyn Transport, url: &str, etag: &str, last_modified: &str) -> Fetched {
    match transport.send(&request(url, etag, last_modified)) {
        Ok(reply) => interpret(url, &reply),
        Err(error) => Fetched::Failed { status: 0, error },
    }
}

/// What the user typed as an address: trimmed, `https://` when there is no scheme, `feed://`
/// as `https://`; only web addresses.
///
/// # Errors
/// A sentence for the user.
pub fn normalize_input(input: &str) -> Result<String, String> {
    let t = input.trim();
    if t.is_empty() {
        return Err("Type the address of a website or of a feed.".to_string());
    }
    let lower = t.to_ascii_lowercase();
    let candidate = if lower.starts_with("feed://") {
        format!("https://{}", &t["feed://".len()..])
    } else if lower.starts_with("feed:") {
        t["feed:".len()..].to_string()
    } else if lower.contains("://")
        || lower.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.contains('.')
                && !scheme.contains('/')
                && scheme.chars().all(|c: char| c.is_ascii_alphabetic())
        })
    {
        t.to_string()
    } else {
        format!("https://{t}")
    };
    if !links::is_web(&candidate) {
        return Err(format!("\u{201c}{t}\u{201d} is not a web address."));
    }
    url::Url::parse(&candidate)
        .map(|u| u.to_string())
        .map_err(|_| format!("\u{201c}{t}\u{201d} is not a web address."))
}

/// The feeds for "Add feed": the address itself when it is a feed, else the feeds its page names
/// that can be read (at most [`MAX_CANDIDATES`]).
///
/// # Errors
/// A sentence for the user (no answer, not found, no feed on the page).
pub fn find_feeds(transport: &dyn Transport, input: &str) -> Result<Vec<Candidate>, String> {
    let url = normalize_input(input)?;
    match fetch(transport, &url, "", "") {
        Fetched::Feed {
            feed,
            etag,
            last_modified,
            ..
        } => Ok(vec![Candidate {
            url,
            link_title: String::new(),
            feed,
            etag,
            last_modified,
        }]),
        Fetched::Page { links } => {
            if links.is_empty() {
                return Err(format!("There is no feed on {url}."));
            }
            let mut out = Vec::new();
            for link in links.into_iter().take(MAX_CANDIDATES) {
                if let Fetched::Feed {
                    feed,
                    etag,
                    last_modified,
                    ..
                } = fetch(transport, &link.url, "", "")
                {
                    out.push(Candidate {
                        url: link.url,
                        link_title: link.title,
                        feed,
                        etag,
                        last_modified,
                    });
                }
            }
            if out.is_empty() {
                Err(format!(
                    "{url} names feeds, but none of them could be read."
                ))
            } else {
                Ok(out)
            }
        }
        Fetched::NotModified { .. } => {
            Err("The server answered \u{201c}not modified\u{201d} to a first request.".to_string())
        }
        Fetched::Failed { error, .. } => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use super::*;

    /// A web of fixed answers that records what was asked.
    #[derive(Default)]
    struct FakeWeb {
        answers: HashMap<String, Result<HttpReply, String>>,
        calls: Mutex<Vec<HttpCall>>,
    }

    impl FakeWeb {
        fn serve(mut self, url: &str, status: u16, headers: &[(&str, &str)], body: &[u8]) -> Self {
            self.answers.insert(
                url.to_string(),
                Ok(HttpReply {
                    status,
                    headers: headers
                        .iter()
                        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                        .collect(),
                    body: body.to_vec(),
                }),
            );
            self
        }

        fn unreachable(mut self, url: &str) -> Self {
            self.answers
                .insert(url.to_string(), Err("could not connect".to_string()));
            self
        }

        fn asked(&self) -> Vec<HttpCall> {
            self.calls.lock().map(|c| c.clone()).unwrap_or_default()
        }
    }

    impl Transport for FakeWeb {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            if let Ok(mut calls) = self.calls.lock() {
                calls.push(call.clone());
            }
            self.answers.get(&call.url).cloned().unwrap_or_else(|| {
                Ok(HttpReply {
                    status: 404,
                    headers: Vec::new(),
                    body: b"not found".to_vec(),
                })
            })
        }
    }

    fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
        call.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    const WORDPRESS: &[u8] = include_bytes!("../tests/fixtures/wordpress_rss2.xml");

    #[test]
    fn a_first_fetch_asks_plainly_and_brings_the_validators() {
        let web = FakeWeb::default().serve(
            "https://example.org/feed/",
            200,
            &[
                ("Content-Type", "application/rss+xml; charset=UTF-8"),
                ("ETag", "\"v1\""),
                ("Last-Modified", "Wed, 30 Sep 2026 08:42:00 GMT"),
            ],
            WORDPRESS,
        );
        match fetch(&web, "https://example.org/feed/", "", "") {
            Fetched::Feed {
                feed,
                etag,
                last_modified,
                status,
            } => {
                assert_eq!(feed.items.len(), 3);
                assert_eq!(etag, "\"v1\"");
                assert_eq!(last_modified, "Wed, 30 Sep 2026 08:42:00 GMT");
                assert_eq!(status, 200);
            }
            other => panic!("a feed, got {other:?}"),
        }
        let call = &web.asked()[0];
        assert_eq!(call.method, Method::Get);
        assert_eq!(header(call, "If-None-Match"), None);
        assert_eq!(header(call, "If-Modified-Since"), None);
        assert!(header(call, "Accept").is_some_and(|a| a.starts_with("application/rss+xml")));
    }

    #[test]
    fn a_refresh_sends_the_validators_and_a_304_is_nothing_new() {
        let web = FakeWeb::default().serve("https://example.org/feed/", 304, &[], b"");
        assert_eq!(
            fetch(
                &web,
                "https://example.org/feed/",
                "\"v1\"",
                "Wed, 30 Sep 2026 08:42:00 GMT"
            ),
            Fetched::NotModified { status: 304 }
        );
        let call = &web.asked()[0];
        assert_eq!(header(call, "If-None-Match"), Some("\"v1\""));
        assert_eq!(
            header(call, "If-Modified-Since"),
            Some("Wed, 30 Sep 2026 08:42:00 GMT")
        );
    }

    #[test]
    fn an_http_error_or_no_answer_is_a_failure_that_says_why() {
        let web = FakeWeb::default().unreachable("https://down.example.org/feed");
        assert_eq!(
            fetch(&web, "https://down.example.org/feed", "", ""),
            Fetched::Failed {
                status: 0,
                error: "could not connect".to_string()
            }
        );
        match fetch(&web, "https://example.org/missing", "", "") {
            Fetched::Failed { status: 404, error } => assert!(error.contains("404"), "{error}"),
            other => panic!("a 404, got {other:?}"),
        }
        let garbage = FakeWeb::default().serve(
            "https://example.org/x",
            200,
            &[("Content-Type", "text/plain")],
            b"just words",
        );
        assert!(matches!(
            fetch(&garbage, "https://example.org/x", "", ""),
            Fetched::Failed { status: 200, .. }
        ));
    }

    #[test]
    fn what_the_user_types_becomes_a_web_address() {
        assert_eq!(
            normalize_input("  example.org  "),
            Ok("https://example.org/".to_string())
        );
        assert_eq!(
            normalize_input("feed://example.org/rss"),
            Ok("https://example.org/rss".to_string())
        );
        assert_eq!(
            normalize_input("http://example.org/rss.xml"),
            Ok("http://example.org/rss.xml".to_string())
        );
        assert!(normalize_input("").is_err());
        assert!(normalize_input("file:///etc/passwd").is_err());
        assert!(normalize_input("javascript:alert(1)").is_err());
    }

    #[test]
    fn a_web_page_is_searched_for_its_feeds_and_each_is_read_for_the_preview() {
        let web = FakeWeb::default()
            .serve(
                "https://example.org/",
                200,
                &[("Content-Type", "text/html")],
                include_bytes!("../tests/fixtures/html_page.html"),
            )
            .serve(
                "https://example.org/feed/",
                200,
                &[("ETag", "\"w\"")],
                WORDPRESS,
            )
            .serve(
                "https://example.org/feed/atom/",
                200,
                &[],
                include_bytes!("../tests/fixtures/blogger_atom.xml"),
            )
            .serve(
                "https://example.org/feed.json",
                200,
                &[],
                include_bytes!("../tests/fixtures/json_feed_11.json"),
            );
        // The comments feed answers 404 (the fake's default): it is left out.
        let found = find_feeds(&web, "example.org").expect("feeds");
        let urls: Vec<&str> = found.iter().map(|c| c.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://example.org/feed/",
                "https://example.org/feed/atom/",
                "https://example.org/feed.json"
            ]
        );
        assert_eq!(found[0].link_title, "Example Weekly \u{bb} Feed");
        assert_eq!(found[0].feed.title, "Example Weekly");
        assert_eq!(found[0].etag, "\"w\"");
    }

    #[test]
    fn a_feed_address_is_its_own_candidate_and_a_page_without_feeds_says_so() {
        let web = FakeWeb::default()
            .serve("https://example.org/feed/", 200, &[], WORDPRESS)
            .serve(
                "https://plain.example.org/",
                200,
                &[("Content-Type", "text/html")],
                b"<html><body>no feeds</body></html>",
            );
        let found = find_feeds(&web, "https://example.org/feed/").expect("the feed");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].link_title, "");
        let err = find_feeds(&web, "plain.example.org").expect_err("no feed");
        assert!(err.contains("no feed"), "{err}");
    }

    #[test]
    fn the_user_agent_names_the_app_and_no_person() {
        assert!(USER_AGENT.starts_with("AzNews/"));
        assert!(!USER_AGENT.contains('@'));
    }
}
