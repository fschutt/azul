//! [`Transport`] over azul's HTTP client (`HttpRequestConfig::http_request_blocking`), for
//! the apps. Feature `azul`.
//!
//! [`AzulTransport::send`] runs the request on the calling thread and returns its answer:
//! nothing waits for the UI thread's pump, so a modal, a long layout, App Nap or a window that
//! closed at quit no longer loses an answer the server already sent (a drive token's rotation
//! among them). Call it from an azul `Thread` (or any worker thread), never from a UI callback:
//! azul refuses a blocking request on the thread that runs its event loop (a debug build
//! panics there), and this crate's mark of the UI thread ([`crate::ui_thread`]) stops it first.
//!
//! The call's limits ([`crate::CallLimits`]) become the request's: the overall timeout, the
//! times its body may take to send and its answer to arrive, and the most bytes the answer may
//! have (a bigger one is refused, never cut short).
//!
//! Every `AzulTransport` of the process sends through ONE `HttpClient`: its connections
//! are pooled (a multipart upload's parts and a download's ranges reuse them), its DNS answers
//! are cached for [`DNS_CACHE_SECS`] and served stale while the name does not resolve, and the
//! fallback addresses a drive's failover hands it ([`Transport::fallback_addresses`]: a
//! node reached at its IP when its name does not resolve, TLS still verified for the name)
//! hold for every request. A name that does not resolve fails with [`DNS_FAILED`] in front.

use std::{sync::OnceLock, time::Duration};

use azul::{
    error::{HttpError, ResultHttpResponseHttpError},
    http::{HttpClient, HttpClientConfig, HttpMethod, HttpRequestConfig, TrustRoots},
    vec::U8Vec,
};

use crate::{transport::DNS_FAILED, HttpCall, HttpReply, Method, Transport};

/// Seconds the pool reuses a host's DNS answer; once the name stops resolving, the engine serves
/// the last answer for seven days (serve-stale) - a drive keeps working while DNS is down.
pub const DNS_CACHE_SECS: u32 = 300;

/// Sends through azul's `HttpRequestConfig`. Call only from a worker thread.
#[derive(Debug, Clone)]
pub struct AzulTransport {
    user_agent: String,
}

impl AzulTransport {
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        AzulTransport {
            user_agent: user_agent.to_string(),
        }
    }
}

/// The connection pool every request of the process goes through.
struct Pool(HttpClient);

// SAFETY: the handle points to the library's `Arc<HttpClientInner>`: a ureq agent and a
// mutex-guarded map of fallback addresses, both Send + Sync. Every call through the handle
// takes `&self`, and a clone is the library's clone of the `Arc`.
unsafe impl Send for Pool {}
unsafe impl Sync for Pool {}

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        Pool(HttpClient::create(
            HttpClientConfig::create().with_dns_cache_secs(DNS_CACHE_SECS),
        ))
    })
}

/// Why a request got no answer, as a sentence. azul's client names the URL it tried ("Connection
/// failed: {url}", "{url}: the name did not resolve"), and pCloud and WebHDFS put the password and
/// the delegation token in the query of every URL, a user may have typed `user:password@` into an
/// address: the text is scrubbed here, where it is made, before it can reach a form's red line, a
/// toast, a log or stdout (AZ-CN3).
fn http_error_text(e: &HttpError) -> String {
    let text = match e {
        HttpError::Timeout => String::from("timed out"),
        // By type, not by its words: the failover reaches the host at its addresses.
        HttpError::DnsFailed(s) => format!("{DNS_FAILED}: {}", s.as_str()),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    };
    crate::scrub::scrub_url(&text)
}

/// The answer of a blocking request.
fn answer_of(result: ResultHttpResponseHttpError) -> Result<HttpReply, String> {
    match result {
        ResultHttpResponseHttpError::Ok(response) => Ok(HttpReply {
            status: response.status_code,
            headers: response
                .headers
                .as_slice()
                .iter()
                .map(|h| (h.name.as_str().to_string(), h.value.as_str().to_string()))
                .collect(),
            body: response.body.as_slice().to_vec(),
        }),
        ResultHttpResponseHttpError::Err(e) => Err(http_error_text(&e)),
    }
}

/// Whole seconds, at least one (azul's timeouts count seconds; 0 would mean none).
fn secs(time: Duration) -> u64 {
    time.as_secs()
        .saturating_add(u64::from(time.subsec_nanos() > 0))
        .max(1)
}

/// azul's verb for `method`: every verb the drives send, WebDAV's included (azul's `HttpMethod`
/// has OPTIONS, PROPFIND, PROPPATCH, MKCOL, COPY and MOVE since its client sends any verb, and
/// bodies for the ones that take one). The match has no wildcard: a verb added to [`Method`] does
/// not compile until it is mapped (AZ-CN5: this said "azul's HTTP client cannot send PROPFIND
/// requests yet", so Nextcloud, ownCloud and Synology could not be connected at all).
fn method_of(method: Method) -> HttpMethod {
    match method {
        Method::Get => HttpMethod::Get,
        Method::Head => HttpMethod::Head,
        Method::Put => HttpMethod::Put,
        Method::Post => HttpMethod::Post,
        Method::Delete => HttpMethod::Delete,
        Method::Patch => HttpMethod::Patch,
        Method::Options => HttpMethod::Options,
        Method::Propfind => HttpMethod::Propfind,
        Method::Proppatch => HttpMethod::Proppatch,
        Method::Mkcol => HttpMethod::Mkcol,
        Method::Copy => HttpMethod::Copy,
        Method::Move => HttpMethod::Move,
    }
}

impl Transport for AzulTransport {
    /// The pool trusts the app's set (`AppConfig::trust_roots`).
    fn trusts_public_roots(&self) -> bool {
        TrustRoots::app_default().is_public_roots()
    }

    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        crate::ui_thread::assert_may_wait("AzulTransport::send");
        if crate::ui_thread::is_ui_thread() {
            return Err(String::from(
                "a request was sent from the UI thread, which never waits for an answer",
            ));
        }
        let method = method_of(call.method);
        let budget = call.budget();
        let mut config = HttpRequestConfig::create()
            .with_timeout(secs(budget.overall))
            .with_send_timeout(budget.send_body.map_or(0, secs))
            .with_recv_timeout(budget.recv_body.map_or(0, secs))
            // 0 is "no cap" to azul: an object read whole has none.
            .with_max_size(if budget.max_answer == u64::MAX { 0 } else { budget.max_answer })
            .with_user_agent(self.user_agent.as_str())
            .with_client(pool().0.clone());
        for (name, value) in &call.headers {
            config = config.with_header(name.as_str(), value.as_str());
        }
        answer_of(config.http_request_blocking(
            method,
            call.url.as_str(),
            U8Vec::from(call.body.clone()),
            call.content_type.as_str(),
        ))
    }

    /// The process's pool connects to `host` at `addresses` when its name does not resolve
    /// (the request and TLS still name `host`).
    fn fallback_addresses(&self, host: &str, addresses: &[String]) -> bool {
        let mut took = false;
        for address in addresses {
            took |= pool().0.add_fallback_address(host, address.as_str());
        }
        took
    }
}

#[cfg(test)]
mod tests {
    use azul::{error::HttpError, str::String as AzString};

    use azul::http::HttpMethod;

    use super::{http_error_text, method_of};
    use crate::{
        transport::{is_dns_failure, DNS_FAILED},
        Method,
    };

    fn text(words: &str) -> AzString {
        AzString::from(words)
    }

    /// AZ-CN3: azul's client says "Connection failed: {url}" and "{url}: the name did not
    /// resolve", and pCloud's URL holds the password: the text this transport makes names the host
    /// and the path and nothing of the query.
    #[test]
    fn az_cn3_a_failed_request_names_no_password_of_its_url() {
        let url = "https://api.pcloud.com/listfolder?path=/&username=ann&password=hunter2";
        for error in [
            HttpError::ConnectionFailed(text(&format!("Connection failed: {url}"))),
            HttpError::IoError(text(&format!("{url}: connection reset"))),
            HttpError::TlsError(text(&format!("{url}: bad certificate"))),
            HttpError::InvalidUrl(text(url)),
            HttpError::Other(text(&format!("{url}: unexpected"))),
        ] {
            let said = http_error_text(&error);
            assert!(!said.contains("hunter2"), "{said}");
            assert!(
                said.contains("https://api.pcloud.com/listfolder?<hidden>"),
                "{said}"
            );
        }
    }

    /// The failover reads a lookup that failed by its sentence: scrubbing keeps it.
    #[test]
    fn az_cn3_a_name_that_did_not_resolve_is_still_said_so() {
        let said = http_error_text(&HttpError::DnsFailed(text(
            "https://api.pcloud.com/listfolder?password=hunter2: the name did not resolve",
        )));
        assert!(said.starts_with(DNS_FAILED), "{said}");
        assert!(is_dns_failure(&said), "{said}");
        assert!(!said.contains("hunter2"), "{said}");
        assert_eq!(http_error_text(&HttpError::Timeout), "timed out");
    }

    /// AZ-CN5 / X-AR5b-21: every verb a drive sends reaches azul's client as the same verb. WebDAV's
    /// six were refused with "cannot send PROPFIND requests yet"; the first request of every WebDAV
    /// source is a PROPFIND.
    #[test]
    fn az_cn5_every_verb_of_a_drive_is_the_verb_of_azuls_client() {
        macro_rules! maps {
            ($($ours:ident),+ $(,)?) => {{
                let mut mapped = 0;
                $(
                    assert!(
                        matches!(method_of(Method::$ours), HttpMethod::$ours),
                        "{} is not sent as itself",
                        Method::$ours.as_str()
                    );
                    mapped += 1;
                )+
                mapped
            }};
        }
        let mapped = maps!(
            Get, Head, Put, Post, Delete, Patch, Options, Propfind, Proppatch, Mkcol, Copy, Move
        );
        assert_eq!(
            mapped,
            Method::ALL.len(),
            "a verb of Method is missing from this list"
        );
    }
}
