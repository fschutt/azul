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

fn http_error_text(e: &HttpError) -> String {
    match e {
        HttpError::Timeout => String::from("timed out"),
        // By type, not by its words: the failover reaches the host at its addresses.
        HttpError::DnsFailed(s) => format!("{DNS_FAILED}: {}", s.as_str()),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
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

/// azul's verb for `method`. azul's `HttpMethod` has GET, HEAD, POST, PUT, PATCH and DELETE
/// only: WebDAV's verbs (PROPFIND, MKCOL, COPY, MOVE, ...) cannot be sent through it yet
/// (layout/src/http.rs `HttpMethod`), so a WebDAV source fails with this sentence until the
/// engine sends any verb.
fn method_of(method: Method) -> Result<HttpMethod, String> {
    match method {
        Method::Get => Ok(HttpMethod::Get),
        Method::Head => Ok(HttpMethod::Head),
        Method::Put => Ok(HttpMethod::Put),
        Method::Post => Ok(HttpMethod::Post),
        Method::Delete => Ok(HttpMethod::Delete),
        Method::Patch => Ok(HttpMethod::Patch),
        other => Err(format!(
            "azul's HTTP client cannot send {} requests yet (WebDAV needs them)",
            other.as_str()
        )),
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
        let method = method_of(call.method)?;
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
