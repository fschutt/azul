//! Asking an online service through the cache ([`crate::cache`]) and the service's gate
//! ([`crate::polite`]): a fresh answer in the cache answers without the network; a stale one is
//! revalidated where the service gave a validator; a request goes out only on its turn; and when
//! the network or the service fails, the stale answer still answers, marked
//! [`Origin::Offline`].
//!
//! Only what the caller puts into the request goes out, under the caller's User-Agent (the app
//! and its version): no cookies, no ids.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{
    cache::{max_age_of, now_s, Entry, ResponseCache},
    polite::{Polite, Turn},
};

/// Where an answer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The service answered it now.
    Network,
    /// The service said the kept answer is still right (`304`).
    Revalidated,
    /// The cache had it fresh: nothing was sent.
    Cache,
    /// The network or the service failed and the cache had it, stale: shown marked "offline".
    Offline,
}

impl Origin {
    /// The word scripts read.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Revalidated => "revalidated",
            Self::Cache => "cache",
            Self::Offline => "offline",
        }
    }
}

/// An answer and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub body: String,
    pub origin: Origin,
    /// How old the answer is, seconds (0 for one the service just sent).
    pub age_s: u64,
}

/// Why there is no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// No network, no server, or offline by choice, and nothing kept.
    Offline(String),
    /// The service asks to slow down, or pauses after failures, and nothing is kept.
    Busy,
    /// The service answered with this status and body.
    Refused(u16, String),
}

/// One request.
#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    /// A GET of `url` with these query parameters (encoded by the client).
    Get {
        url: &'a str,
        query: &'a [(String, String)],
    },
    /// A POST of a JSON body to `url`.
    PostJson { url: &'a str, body: &'a str },
}

/// One online service as a session asks it: one connection pool, one gate, one cache.
#[cfg(feature = "http")]
pub struct Fetcher {
    agent: ureq::Agent,
    /// The app and its version, nothing about the user.
    user_agent: String,
    gate: Polite,
    cache: Arc<ResponseCache>,
    /// Offline by choice (`--offline`): nothing goes out, the cache answers.
    offline: AtomicBool,
}

#[cfg(feature = "http")]
impl Fetcher {
    /// The largest answer read, bytes.
    const MAX_ANSWER: u64 = 16 * 1024 * 1024;

    /// A service asked under `user_agent`, its requests `gap` apart, its answers kept in `cache`.
    #[must_use]
    pub fn new(user_agent: impl Into<String>, gap: std::time::Duration, cache: Arc<ResponseCache>) -> Self {
        Self {
            agent: crate::source::http_agent(),
            user_agent: user_agent.into(),
            gate: Polite::new(gap),
            cache,
            offline: AtomicBool::new(false),
        }
    }

    /// Offline by choice: nothing goes out, the cache answers what it can.
    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::Relaxed);
    }

    /// Whether it is offline by choice.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::Relaxed)
    }

    /// The cache its answers are kept in.
    #[must_use]
    pub fn cache(&self) -> &ResponseCache {
        &self.cache
    }

    /// The gate of the service.
    #[must_use]
    pub fn gate(&self) -> &Polite {
        &self.gate
    }

    /// The answer to `request`, kept under `key`: fresh for the service's `max-age`, else for
    /// `ttl_s`. `Ok(None)` when `stop` said to give up first.
    ///
    /// # Errors
    ///
    /// [`FetchError`] when neither the service nor the cache has an answer.
    pub fn fetch(
        &self,
        request: &Request<'_>,
        key: &str,
        ttl_s: u64,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Option<Fetched>, FetchError> {
        let kept = self.cache.get(key);
        let now = now_s();
        let offline = |why: FetchError| match kept.as_ref() {
            Some(entry) => Ok(Some(answer(entry, Origin::Offline, now))),
            None => Err(why),
        };
        // Offline by choice, every kept answer is shown as what it is: offline.
        if self.is_offline() {
            return offline(FetchError::Offline("offline".to_string()));
        }
        if let Some(entry) = kept.as_ref().filter(|e| e.is_fresh(now)) {
            return Ok(Some(answer(entry, Origin::Cache, now)));
        }
        match self.gate.turn(stop) {
            Turn::Go => {}
            Turn::Stopped => return Ok(None),
            Turn::BackingOff(_) => return offline(FetchError::Busy),
        }
        let validators = kept.as_ref().filter(|e| e.can_revalidate());
        let sent = match *request {
            Request::Get { url, query } => {
                let mut builder = self
                    .agent
                    .get(url)
                    .header("User-Agent", self.user_agent.as_str())
                    .header("Accept", "application/json");
                if let Some(entry) = validators {
                    if let Some(etag) = &entry.etag {
                        builder = builder.header("If-None-Match", etag.as_str());
                    }
                    if let Some(date) = &entry.last_modified {
                        builder = builder.header("If-Modified-Since", date.as_str());
                    }
                }
                builder
                    .query_pairs(query.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                    .call()
            }
            Request::PostJson { url, body } => self
                .agent
                .post(url)
                .header("User-Agent", self.user_agent.as_str())
                .header("Content-Type", "application/json")
                .send(body),
        };
        let response = match sent {
            Ok(response) => response,
            Err(e) => {
                self.gate.failed();
                return offline(FetchError::Offline(e.to_string()));
            }
        };
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let max_age = max_age_of(header("cache-control").as_deref()).unwrap_or(ttl_s);
        let etag = header("etag");
        let last_modified = header("last-modified");
        if status == 304 {
            self.gate.answered();
            if let Some(entry) = kept.as_ref() {
                self.cache.refresh(key, now, max_age);
                return Ok(Some(answer(entry, Origin::Revalidated, now)));
            }
            return Err(FetchError::Refused(304, String::new()));
        }
        let text = match response
            .into_body()
            .into_with_config()
            .limit(Self::MAX_ANSWER)
            .read_to_string()
        {
            Ok(text) => text,
            Err(e) => {
                self.gate.failed();
                return offline(FetchError::Offline(e.to_string()));
            }
        };
        match status {
            200 => {
                self.gate.answered();
                let mut entry = Entry::new(text.clone().into_bytes(), now, max_age);
                entry.etag = etag;
                entry.last_modified = last_modified;
                self.cache.put(key, entry);
                Ok(Some(Fetched {
                    body: text,
                    origin: Origin::Network,
                    age_s: 0,
                }))
            }
            429 => {
                self.gate.failed();
                offline(FetchError::Busy)
            }
            500.. => {
                // The service is down: kept answers stand in for it until it is back.
                self.gate.failed();
                offline(FetchError::Refused(status, text))
            }
            _ => {
                self.gate.answered();
                Err(FetchError::Refused(status, text))
            }
        }
    }
}

/// A kept answer as a [`Fetched`].
fn answer(entry: &Entry, origin: Origin, now: u64) -> Fetched {
    Fetched {
        body: String::from_utf8_lossy(&entry.body).into_owned(),
        origin,
        age_s: entry.age_s(now),
    }
}

#[cfg(all(test, feature = "http"))]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::test_server::{closed_port, Server};

    const AGENT: &str = "AzMaps-test/1 (+https://github.com/fschutt/azul)";

    fn get<'a>(url: &'a str, query: &'a [(String, String)]) -> Request<'a> {
        Request::Get { url, query }
    }

    /// A service's TLS trusts the roots the app hands over: one under a root outside them -
    /// or under one the app's set left out, distrusted - is refused at the handshake, and
    /// nothing is kept; one under a trusted root answers.
    #[test]
    fn a_service_outside_the_trusted_roots_is_refused_and_one_inside_answers() {
        use azul_core::tls_testing::{pems, serve_tls, TestCa};
        let a = TestCa::new("azul test root A");
        let b = TestCa::new("azul test root B");
        let (port, seen) = serve_tls(b.server_for_loopback());
        let url = format!("https://127.0.0.1:{port}/route");
        let ask = |roots: Vec<String>| {
            let fetcher = Fetcher::new(AGENT, Duration::ZERO, Arc::new(ResponseCache::in_memory(8)))
                .trusting(&roots);
            fetcher.fetch(&get(&url, &[]), "r", 60, &mut || false)
        };
        assert!(ask(vec![a.cert_pem.clone()]).is_err(), "b's server is outside the set");
        assert!(seen.recv().expect("seen").is_err());
        let set: Vec<String> = azul_core::tls::TrustRoots::pinned(pems(&[&a, &b]))
            .with_distrusted(pems(&[&b]))
            .get_certificates_pem()
            .into_result()
            .expect("the set")
            .as_ref()
            .iter()
            .map(|pem| pem.as_str().to_string())
            .collect();
        assert!(ask(set).is_err(), "b is distrusted");
        assert!(seen.recv().expect("seen").is_err());
        let answer = ask(vec![b.cert_pem.clone()]).expect("trusted").expect("not stopped");
        assert_eq!(answer.body, "ok");
        let broken = ask(vec![String::from("not a pem")]).unwrap_err();
        assert!(format!("{broken:?}").contains("trusted root 0"), "{broken:?}");
    }

    #[test]
    fn a_fresh_answer_is_given_from_the_cache_without_a_request() {
        let server = Server::start(|_| (200, vec![], "{\"n\":1}".to_string()));
        let fetcher = Fetcher::new(AGENT, Duration::ZERO, Arc::new(ResponseCache::in_memory(8)));
        let url = format!("{}/api", server.base);
        let query = vec![("text".to_string(), "Berlin Hbf".to_string())];
        let first = fetcher.fetch(&get(&url, &query), "q", 60, &mut || false).expect("an answer").expect("not stopped");
        assert_eq!(first.origin, Origin::Network);
        assert_eq!(first.body, "{\"n\":1}");
        let again = fetcher.fetch(&get(&url, &query), "q", 60, &mut || false).expect("an answer").expect("not stopped");
        assert_eq!(again.origin, Origin::Cache);
        assert_eq!(server.count(), 1, "the repeat sent nothing");
        let seen = &server.requests()[0];
        assert_eq!(seen.header("user-agent"), Some(AGENT));
        assert_eq!(seen.query("text").as_deref(), Some("Berlin Hbf"));
        assert_eq!(seen.header("cookie"), None);
    }

    #[test]
    fn a_stale_answer_with_an_etag_is_revalidated_and_a_304_keeps_it() {
        let server = Server::start(|seen| {
            if seen.header("if-none-match") == Some("\"v1\"") {
                (304, vec![], String::new())
            } else {
                (200, vec![("ETag".to_string(), "\"v1\"".to_string()), ("Cache-Control".to_string(), "max-age=0".to_string())], "body".to_string())
            }
        });
        let fetcher = Fetcher::new(AGENT, Duration::ZERO, Arc::new(ResponseCache::in_memory(8)));
        let url = format!("{}/tile", server.base);
        let first = fetcher.fetch(&get(&url, &[]), "t", 3_600, &mut || false).expect("an answer").expect("not stopped");
        assert_eq!(first.origin, Origin::Network, "max-age=0 wins over the caller's hour");
        let second = fetcher.fetch(&get(&url, &[]), "t", 3_600, &mut || false).expect("an answer").expect("not stopped");
        assert_eq!(second.origin, Origin::Revalidated);
        assert_eq!(second.body, "body");
        assert_eq!(server.count(), 2);
    }

    #[test]
    fn without_a_network_the_kept_answer_answers_marked_offline() {
        let cache = Arc::new(ResponseCache::in_memory(8));
        cache.put("route", Entry::new(b"kept".to_vec(), 1, 1));
        let fetcher = Fetcher::new(AGENT, Duration::ZERO, cache);
        let url = format!("{}/route", closed_port());
        let body = "{}";
        let kept = fetcher
            .fetch(&Request::PostJson { url: &url, body }, "route", 60, &mut || false)
            .expect("the kept answer")
            .expect("not stopped");
        assert_eq!(kept.origin, Origin::Offline);
        assert_eq!(kept.body, "kept");
        let none = fetcher.fetch(&Request::PostJson { url: &url, body }, "other", 60, &mut || false);
        assert!(matches!(none, Err(FetchError::Offline(_)) | Err(FetchError::Busy)), "{none:?}");
    }

    #[test]
    fn offline_by_choice_sends_nothing_and_answers_from_the_cache() {
        let server = Server::start(|_| (200, vec![], "fresh".to_string()));
        let cache = Arc::new(ResponseCache::in_memory(8));
        cache.put("k", Entry::new(b"kept".to_vec(), 1, 1));
        cache.put("fresh", Entry::new(b"new".to_vec(), now_s(), 3_600));
        let fetcher = Fetcher::new(AGENT, Duration::ZERO, cache);
        fetcher.set_offline(true);
        let url = format!("{}/x", server.base);
        let kept = fetcher.fetch(&get(&url, &[]), "k", 60, &mut || false).expect("kept").expect("not stopped");
        assert_eq!(kept.origin, Origin::Offline);
        let fresh = fetcher.fetch(&get(&url, &[]), "fresh", 60, &mut || false).expect("kept").expect("not stopped");
        assert_eq!(fresh.origin, Origin::Offline, "offline by choice, a fresh answer is marked offline too");
        assert!(matches!(fetcher.fetch(&get(&url, &[]), "new", 60, &mut || false), Err(FetchError::Offline(_))));
        assert_eq!(server.count(), 0);
    }

    #[test]
    fn a_busy_service_pauses_and_is_not_asked_again_until_the_pause_ends() {
        let server = Server::start(|_| (429, vec![], "slow down".to_string()));
        let fetcher = Fetcher::new(AGENT, Duration::ZERO, Arc::new(ResponseCache::in_memory(8)));
        let url = format!("{}/plan", server.base);
        assert_eq!(fetcher.fetch(&get(&url, &[]), "a", 60, &mut || false), Err(FetchError::Busy));
        assert_eq!(fetcher.fetch(&get(&url, &[]), "b", 60, &mut || false), Err(FetchError::Busy));
        assert_eq!(server.count(), 1, "the paused service was not asked again");
        assert!(fetcher.gate().paused_for().is_some());
    }

    #[test]
    fn a_refusal_is_the_services_answer_and_is_not_kept() {
        let server = Server::start(|_| (400, vec![], "{\"error\":\"bad place\"}".to_string()));
        let fetcher = Fetcher::new(AGENT, Duration::ZERO, Arc::new(ResponseCache::in_memory(8)));
        let url = format!("{}/plan", server.base);
        let refused = fetcher.fetch(&get(&url, &[]), "a", 60, &mut || false);
        assert_eq!(refused, Err(FetchError::Refused(400, "{\"error\":\"bad place\"}".to_string())));
        assert!(fetcher.cache().get("a").is_none());
        assert_eq!(fetcher.gate().paused_for(), None, "a refusal is an answer, not a failure");
    }
}
