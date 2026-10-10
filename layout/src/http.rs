//! Simple HTTP client module for downloading resources (language packs, etc.)
//!
//! Uses ureq for simple, blocking HTTP requests. Designed to be exposed via C API.

use alloc::{format, string::String, vec::Vec};
use core::fmt;

use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq, AzString, U8Vec,
};

// ============================================================================
// Error types (C-compatible, single field per variant)
// ============================================================================

/// HTTP status error (4xx, 5xx responses)
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HttpStatusError {
    /// HTTP status code
    pub status_code: u16,
    /// Status message
    pub message: AzString,
}

/// Response too large error
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HttpResponseTooLargeError {
    /// Maximum allowed size in bytes
    pub max_size: u64,
    /// Actual size in bytes
    pub actual_size: u64,
}

/// HTTP error types (C-compatible)
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum HttpError {
    /// Invalid URL format
    InvalidUrl(AzString),
    /// Connection failed
    ConnectionFailed(AzString),
    /// Request timed out
    Timeout,
    /// TLS/SSL error
    TlsError(AzString),
    /// HTTP error response (4xx, 5xx)
    HttpStatus(HttpStatusError),
    /// I/O error during request
    IoError(AzString),
    /// Response body too large
    ResponseTooLarge(HttpResponseTooLargeError),
    /// Other error
    Other(AzString),
    /// The host name did not resolve: the lookup failed or gave up (DNS down, an unknown
    /// name). The host may still answer at an address known another way
    /// ([`HttpClient::add_fallback_address`]).
    DnsFailed(AzString),
}

impl HttpError {
    #[must_use]
    pub const fn invalid_url(url: AzString) -> Self {
        Self::InvalidUrl(url)
    }

    #[must_use]
    pub const fn connection_failed(msg: AzString) -> Self {
        Self::ConnectionFailed(msg)
    }

    #[must_use]
    pub const fn tls_error(msg: AzString) -> Self {
        Self::TlsError(msg)
    }

    #[must_use]
    pub const fn http_status(status_code: u16, message: AzString) -> Self {
        Self::HttpStatus(HttpStatusError {
            status_code,
            message,
        })
    }

    #[must_use]
    pub const fn io_error(msg: AzString) -> Self {
        Self::IoError(msg)
    }

    #[must_use]
    pub const fn response_too_large(max_size: u64, actual_size: u64) -> Self {
        Self::ResponseTooLarge(HttpResponseTooLargeError {
            max_size,
            actual_size,
        })
    }

    #[must_use]
    pub const fn other(msg: AzString) -> Self {
        Self::Other(msg)
    }

    #[must_use]
    pub const fn dns_failed(msg: AzString) -> Self {
        Self::DnsFailed(msg)
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(url) => write!(f, "Invalid URL: {}", url.as_str()),
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg.as_str()),
            Self::Timeout => write!(f, "Request timed out"),
            Self::TlsError(msg) => write!(f, "TLS error: {}", msg.as_str()),
            Self::HttpStatus(e) => write!(f, "HTTP {} - {}", e.status_code, e.message.as_str()),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg.as_str()),
            Self::ResponseTooLarge(e) => {
                write!(
                    f,
                    "Response too large: {} bytes (max: {})",
                    e.actual_size, e.max_size
                )
            }
            Self::Other(msg) => write!(f, "HTTP error: {}", msg.as_str()),
            Self::DnsFailed(msg) => write!(f, "DNS lookup failed: {}", msg.as_str()),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for HttpError {}

/// Result type for HTTP operations
pub type HttpResult<T> = Result<T, HttpError>;

// FFI-safe Result types for HTTP operations
use azul_css::impl_result;

// Forward declaration - actual impl_result! calls are after HttpResponse definition

// ============================================================================
// Request configuration (C-compatible)
// ============================================================================

/// HTTP header key-value pair
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HttpHeader {
    /// Header name
    pub name: AzString,
    /// Header value
    pub value: AzString,
}

impl HttpHeader {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: AzString::from(name.into()),
            value: AzString::from(value.into()),
        }
    }
}

impl_option!(
    HttpHeader,
    OptionHttpHeader,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    HttpHeader,
    HttpHeaderVec,
    HttpHeaderVecDestructor,
    HttpHeaderVecDestructorType,
    HttpHeaderVecSlice,
    OptionHttpHeader
);
impl_vec_clone!(HttpHeader, HttpHeaderVec, HttpHeaderVecDestructor);
impl_vec_debug!(HttpHeader, HttpHeaderVec);
impl_vec_partialeq!(HttpHeader, HttpHeaderVec);
impl_vec_mut!(HttpHeader, HttpHeaderVec);

/// HTTP request configuration (C-compatible)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct HttpRequestConfig {
    /// Request timeout in seconds (default: 30)
    pub timeout_secs: u64,
    /// Maximum response size in bytes (default: 100MB, 0 = unlimited)
    pub max_response_size: u64,
    /// User-Agent header value
    pub user_agent: AzString,
    /// Additional headers
    pub headers: HttpHeaderVec,
    /// Disable TLS certificate verification (default: false).
    /// WARNING: This makes HTTPS connections vulnerable to MITM attacks.
    /// Use only for testing or when connecting to servers with self-signed
    /// or cross-signed certificates not in the Mozilla root store.
    pub disable_tls_cert_verification: bool,
    /// Connection pool to send the request through (default: none).
    ///
    /// `None` opens a fresh connection for this request and closes it after.
    /// `Some` reuses the client's idle connections, and TLS verification then
    /// follows the client's `HttpClientConfig`, not the field above.
    pub client: OptionHttpClient,
}

impl Default for HttpRequestConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_response_size: 100 * 1024 * 1024, // 100 MB
            user_agent: AzString::from("azul-http/1.0".to_string()),
            headers: HttpHeaderVec::from_const_slice(&[]),
            disable_tls_cert_verification: false,
            client: OptionHttpClient::None,
        }
    }
}

/// Settings for an [`HttpClient`]'s connection pool (C-compatible).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HttpClientConfig {
    /// Idle connections kept open across all hosts (default: 32).
    pub max_idle_connections: u32,
    /// Idle connections kept open to any one host (default: 8).
    pub max_idle_connections_per_host: u32,
    /// Seconds a host's DNS answer is reused (default: 0 = look it up for
    /// every request, like a request without a client does).
    ///
    /// Reusing a pooled connection does not skip the lookup: the address is
    /// resolved before the pool is asked for a connection. With a cache the
    /// lookup happens once per host per period. A host that moves to another
    /// address is only reached again once its entry expires.
    pub dns_cache_secs: u32,
    /// Disable TLS certificate verification for every request of this client
    /// (default: false). Same warning as on `HttpRequestConfig`.
    pub disable_tls_cert_verification: bool,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            max_idle_connections: 32,
            max_idle_connections_per_host: 8,
            dns_cache_secs: 0,
            disable_tls_cert_verification: false,
        }
    }
}

impl HttpClientConfig {
    /// Create a new config with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set how many idle connections the pool keeps across all hosts
    #[must_use]
    pub const fn with_max_idle_connections(mut self, n: u32) -> Self {
        self.max_idle_connections = n;
        self
    }

    /// Set how many idle connections the pool keeps to any one host
    #[must_use]
    pub const fn with_max_idle_connections_per_host(mut self, n: u32) -> Self {
        self.max_idle_connections_per_host = n;
        self
    }

    /// Reuse each host's DNS answer for `secs` seconds (0 = no cache)
    #[must_use]
    pub const fn with_dns_cache_secs(mut self, secs: u32) -> Self {
        self.dns_cache_secs = secs;
        self
    }
}

/// A connection pool that requests can share (C-compatible handle).
///
/// Created by the application, never by the framework: a request without a
/// client opens its own connection. Hand clones to
/// [`HttpRequestConfig::with_client`] or to a widget; all clones share one pool,
/// which closes its connections when the last clone is dropped.
#[repr(C)]
pub struct HttpClient {
    pub ptr: Box<alloc::sync::Arc<HttpClientInner>>,
    pub run_destructor: bool,
}

/// The shared state behind an [`HttpClient`] handle.
#[derive(Debug)]
#[allow(missing_copy_implementations)] // only Copy in builds without the `http` feature
pub struct HttpClientInner {
    pub config: HttpClientConfig,
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    agent: ureq::Agent,
    /// Where a host is reached when its name does not resolve
    /// ([`HttpClient::add_fallback_address`]); the agent's resolver reads it.
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    fallback: FallbackAddresses,
}

impl HttpClient {
    /// Create a connection pool with the given settings
    #[must_use]
    pub fn create(config: HttpClientConfig) -> Self {
        #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
        let fallback = FallbackAddresses::default();
        Self {
            ptr: Box::new(alloc::sync::Arc::new(HttpClientInner {
                config,
                #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
                agent: client_agent(
                    &config,
                    ureq::unversioned::resolver::DefaultResolver::default(),
                    fallback.clone(),
                ),
                #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
                fallback,
            })),
            run_destructor: true,
        }
    }

    /// The settings this client was created with
    #[must_use]
    pub fn get_config(&self) -> HttpClientConfig {
        self.ptr.config
    }

    /// Connects to `host` at `address` (an IP address, `ip:port` or `[ipv6]:port`) whenever
    /// the host's name does not resolve: the request still goes to `host` - its Host header
    /// and the TLS server name, so the certificate is verified for that name, not for the
    /// address. A host collects up to 16 addresses. Returns false when `address` is no address
    /// (or this build has no HTTP client).
    // const only in the stub without the `http` feature; the real one locks a mutex.
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn add_fallback_address(&self, host: &str, address: &str) -> bool {
        #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
        {
            let Some(parsed) = parse_fallback_address(address) else {
                return false;
            };
            let host = fallback_host(host);
            if host.is_empty() {
                return false;
            }
            let Ok(mut known) = self.ptr.fallback.lock() else {
                return false;
            };
            let addresses = known.entry(host).or_default();
            if !addresses.contains(&parsed) && addresses.len() < MAX_FALLBACK_ADDRESSES {
                addresses.push(parsed);
            }
            true
        }
        #[cfg(not(all(feature = "http", not(target_arch = "wasm32"))))]
        {
            let _ = (host, address);
            false
        }
    }

    /// Forgets the fallback addresses of `host`: its name is looked up only.
    // const only in the stub without the `http` feature; the real one locks a mutex.
    #[allow(clippy::missing_const_for_fn)]
    pub fn clear_fallback_addresses(&self, host: &str) {
        #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
        if let Ok(mut known) = self.ptr.fallback.lock() {
            known.remove(&fallback_host(host));
        }
        #[cfg(not(all(feature = "http", not(target_arch = "wasm32"))))]
        let _ = host;
    }
}

impl Clone for HttpClient {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for HttpClient {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpClient")
            .field("config", &self.ptr.config)
            .finish_non_exhaustive()
    }
}

/// Two handles are equal when they share the same pool.
impl PartialEq for HttpClient {
    fn eq(&self, other: &Self) -> bool {
        alloc::sync::Arc::ptr_eq(&self.ptr, &other.ptr)
    }
}

impl_option!(
    HttpClient,
    OptionHttpClient,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// The e2e mock store's answer for `url`, if the store is armed. `None` =
/// not an e2e run, perform the real transfer.
#[cfg(feature = "text_layout")]
fn mocked_http(url: &str) -> Option<ResultHttpResponseHttpError> {
    use crate::request::mock::{Answer, MockHttp};
    match crate::request::mock::take_http(url) {
        Answer::NotArmed => None,
        Answer::Mocked(MockHttp::Response(r)) => {
            let content_length = r.body.len() as u64;
            Some(ResultHttpResponseHttpError::Ok(HttpResponse {
                status_code: r.status,
                body: U8Vec::from_vec(r.body),
                content_type: r.content_type,
                content_length,
                headers: HttpHeaderVec::from_const_slice(&[]),
                final_url: AzString::from(url),
            }))
        }
        Answer::Mocked(MockHttp::Error(message)) => {
            Some(ResultHttpResponseHttpError::Err(HttpError::other(message)))
        }
        Answer::Unmocked => Some(ResultHttpResponseHttpError::Err(HttpError::other(
            AzString::from(format!("unmocked http request under e2e: {url}")),
        ))),
    }
}

/// [`mocked_http`] narrowed to the byte body: a non-2xx status is an
/// `HttpError::HttpStatus`, like `download_bytes_with_config`.
#[cfg(feature = "text_layout")]
fn mocked_download(url: &str) -> Option<ResultU8VecHttpError> {
    match mocked_http(url)? {
        ResultHttpResponseHttpError::Ok(response) => Some(if response.status_code >= 400 {
            ResultU8VecHttpError::Err(HttpError::http_status(
                response.status_code,
                AzString::from(format!("HTTP error {}", response.status_code)),
            ))
        } else {
            ResultU8VecHttpError::Ok(response.body)
        }),
        ResultHttpResponseHttpError::Err(e) => Some(ResultU8VecHttpError::Err(e)),
    }
}

/// [`mocked_http`] as a reachability probe's `(reachable, error)`.
#[cfg(feature = "text_layout")]
fn mocked_reachable(url: &str) -> Option<(bool, Option<AzString>)> {
    Some(match mocked_http(url)? {
        ResultHttpResponseHttpError::Ok(response) => (response.is_success(), None),
        ResultHttpResponseHttpError::Err(e) => (false, Some(AzString::from(e.to_string()))),
    })
}

/// What a resumable request answers when its worker thread could not start,
/// or ended without an answer (it panicked).
#[cfg(feature = "text_layout")]
const LOST_TRANSFER: &str = "the request's worker thread ended without an answer";

/// [`LOST_TRANSFER`] as the answer of `http_get` / `http_request` / `http_post`.
#[cfg(feature = "text_layout")]
fn lost_get_result() -> HttpGetResult {
    HttpGetResult {
        result: ResultHttpResponseHttpError::Err(HttpError::other(LOST_TRANSFER.into())),
    }
}

/// Resumes `on_result` with the request's answer WITHOUT blocking the calling
/// activation: the one way every resumable request of [`HttpRequestConfig`]
/// runs.
///
/// `mocked` is the e2e mock store's answer (`mocked_http` / `mocked_download`):
/// a canned answer, or the immediate "unmocked" error of a scripted run, is
/// queued for the next pump right away - a scripted run stays deterministic.
/// Otherwise, with a network transport (the `http` feature, not wasm32), the
/// transfer runs on a worker thread and the request is deferred: the pump
/// (which keeps ticking while `request::has_work()`) polls the thread's
/// channel and resumes on the first poll after the answer. A thread that
/// cannot start, or ends without an answer, resumes with `lost()`. Without a
/// transport (no `http` feature; on wasm32 the web host services the queue)
/// `transfer` runs here.
#[cfg(feature = "text_layout")]
fn resume_without_blocking<T: Send + 'static>(
    data: azul_core::refany::RefAny,
    on_result: crate::callbacks::ResumeCallback,
    mocked: Option<T>,
    transfer: impl FnOnce() -> T + Send + 'static,
    lost: fn() -> T,
) -> azul_core::task::RequestId {
    if let Some(answer) = mocked {
        return crate::request::complete(data, on_result, answer);
    }
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    {
        use std::sync::mpsc::{channel, TryRecvError};

        let (answer_tx, answer_rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("azul-http".into())
            .spawn(move || {
                // Fails only when the request was dropped unanswered.
                let _ = answer_tx.send(transfer());
            });
        if spawned.is_err() {
            return crate::request::complete(data, on_result, lost());
        }
        crate::request::defer(
            data,
            on_result,
            alloc::boxed::Box::new(move || match answer_rx.try_recv() {
                Ok(answer) => Some(azul_core::refany::RefAny::new(answer)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(azul_core::refany::RefAny::new(lost())),
            }),
        )
    }
    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    {
        let _ = lost;
        crate::request::complete(data, on_result, transfer())
    }
}

impl HttpRequestConfig {
    /// Create a new config with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set timeout in seconds
    #[must_use]
    pub const fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set maximum response size (0 = unlimited)
    #[must_use]
    pub const fn with_max_size(mut self, max_bytes: u64) -> Self {
        self.max_response_size = max_bytes;
        self
    }

    /// Send requests through a shared connection pool instead of opening a
    /// connection per request
    #[must_use]
    pub fn with_client(mut self, client: HttpClient) -> Self {
        self.client = OptionHttpClient::Some(client);
        self
    }

    /// Set User-Agent header
    #[must_use]
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = AzString::from(ua.into());
        self
    }

    /// Add a header
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(HttpHeader::new(name, value));
        self
    }

    /// HTTP GET request using this configuration, resuming `on_result` with
    /// an [`HttpGetResult`].
    ///
    /// Never blocks the calling activation: on desktop the transfer runs on a
    /// worker thread and the callback runs on the first frame after the answer
    /// arrives (under an armed e2e mock store the canned answer resumes right
    /// after the current activation); on web `fetch()` runs and the callback
    /// runs on a later task. `data` is handed back untouched.
    ///
    /// On web CORS applies and cannot be escaped: a target that does not
    /// send `Access-Control-Allow-Origin` fails with `HttpError::Other`
    /// naming CORS. Chromium 142+ additionally prompts for Local Network
    /// Access on loopback / LAN targets.
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub fn http_get(
        &self,
        url: AzString,
        data: azul_core::refany::RefAny,
        on_result: crate::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let config = self.clone();
        resume_without_blocking(
            data,
            on_result,
            mocked_http(url.as_str()).map(|result| HttpGetResult { result }),
            move || HttpGetResult {
                result: config.http_get_blocking(url),
            },
            lost_get_result,
        )
    }

    /// The synchronous transport behind [`Self::http_get`]. Not part of the
    /// public API (it cannot exist on web); framework-internal callers that
    /// are already on a worker thread may use it.
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[must_use]
    pub fn http_get_blocking(&self, url: AzString) -> ResultHttpResponseHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_http(url.as_str()) {
            return mocked;
        }
        http_get_with_config(url.as_str(), self).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[must_use]
    pub fn http_get_blocking(&self, url: AzString) -> ResultHttpResponseHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_http(url.as_str()) {
            return mocked;
        }
        ResultHttpResponseHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// HTTP request with an arbitrary verb and an optional body, using this
    /// configuration, resuming `on_result` with an [`HttpGetResult`]. An
    /// EMPTY `body` sends no body (GET/HEAD semantics); `content_type` is
    /// only applied when a body is present. Same contract as
    /// [`Self::http_get`].
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub fn http_request(
        &self,
        method: HttpMethod,
        url: AzString,
        body: U8Vec,
        content_type: AzString,
        data: azul_core::refany::RefAny,
        on_result: crate::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let config = self.clone();
        resume_without_blocking(
            data,
            on_result,
            mocked_http(url.as_str()).map(|result| HttpGetResult { result }),
            move || HttpGetResult {
                result: config.http_request_blocking(method, url, body, content_type),
            },
            lost_get_result,
        )
    }

    /// The synchronous transport behind [`Self::http_request`]; see
    /// [`Self::http_get_blocking`].
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[must_use]
    pub fn http_request_blocking(
        &self,
        method: HttpMethod,
        url: AzString,
        body: U8Vec,
        content_type: AzString,
    ) -> ResultHttpResponseHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_http(url.as_str()) {
            return mocked;
        }
        let body_ref = body.as_ref();
        let body_opt = if body_ref.is_empty() {
            None
        } else {
            Some(body_ref)
        };
        http_request_with_config(method, url.as_str(), body_opt, content_type.as_str(), self).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[must_use]
    pub fn http_request_blocking(
        &self,
        _method: HttpMethod,
        url: AzString,
        _body: U8Vec,
        _content_type: AzString,
    ) -> ResultHttpResponseHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_http(url.as_str()) {
            return mocked;
        }
        ResultHttpResponseHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// HTTP POST with a body, using this configuration, resuming `on_result`
    /// with an [`HttpGetResult`]. Same contract as [`Self::http_get`].
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub fn http_post(
        &self,
        url: AzString,
        body: U8Vec,
        content_type: AzString,
        data: azul_core::refany::RefAny,
        on_result: crate::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let config = self.clone();
        resume_without_blocking(
            data,
            on_result,
            mocked_http(url.as_str()).map(|result| HttpGetResult { result }),
            move || HttpGetResult {
                result: config.http_post_blocking(url, body, content_type),
            },
            lost_get_result,
        )
    }

    /// The synchronous transport behind [`Self::http_post`]; see
    /// [`Self::http_get_blocking`].
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[must_use]
    pub fn http_post_blocking(
        &self,
        url: AzString,
        body: U8Vec,
        content_type: AzString,
    ) -> ResultHttpResponseHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_http(url.as_str()) {
            return mocked;
        }
        http_post_with_config(url.as_str(), body.as_ref(), content_type.as_str(), self).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[must_use]
    pub fn http_post_blocking(
        &self,
        url: AzString,
        _body: U8Vec,
        _content_type: AzString,
    ) -> ResultHttpResponseHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_http(url.as_str()) {
            return mocked;
        }
        ResultHttpResponseHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// Download a URL to bytes using this configuration, resuming
    /// `on_result` with an [`HttpBytesResult`] (a non-2xx status is an
    /// `HttpError::HttpStatus`). Same contract as [`Self::http_get`].
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub fn download_bytes(
        &self,
        url: AzString,
        data: azul_core::refany::RefAny,
        on_result: crate::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let config = self.clone();
        resume_without_blocking(
            data,
            on_result,
            mocked_download(url.as_str()).map(|result| HttpBytesResult { result }),
            move || HttpBytesResult {
                result: config.download_bytes_blocking(url),
            },
            || HttpBytesResult {
                result: ResultU8VecHttpError::Err(HttpError::other(LOST_TRANSFER.into())),
            },
        )
    }

    /// The synchronous transport behind [`Self::download_bytes`]; see
    /// [`Self::http_get_blocking`].
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[must_use]
    pub fn download_bytes_blocking(&self, url: AzString) -> ResultU8VecHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_download(url.as_str()) {
            return mocked;
        }
        download_bytes_with_config(url.as_str(), self).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[must_use]
    pub fn download_bytes_blocking(&self, url: AzString) -> ResultU8VecHttpError {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_download(url.as_str()) {
            return mocked;
        }
        ResultU8VecHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// Check whether a URL is reachable (a HEAD request answered with a 2xx
    /// status), using this configuration's timeout and TLS settings, and
    /// resume `on_result` with an [`HttpReachableResult`]. Same contract as
    /// [`Self::http_get`].
    ///
    /// On web, opaque `no-cors` answers make "reachable" approximate: a
    /// server that answers at all counts as reachable even when its status
    /// cannot be read.
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub fn is_url_reachable(
        &self,
        url: AzString,
        data: azul_core::refany::RefAny,
        on_result: crate::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let config = self.clone();
        let result = |(reachable, error): (bool, Option<AzString>)| HttpReachableResult {
            reachable,
            error: error.into(),
        };
        resume_without_blocking(
            data,
            on_result,
            mocked_reachable(url.as_str()).map(result),
            move || result(config.is_url_reachable_blocking(url)),
            || HttpReachableResult {
                reachable: false,
                error: azul_css::corety::OptionString::Some(LOST_TRANSFER.into()),
            },
        )
    }

    /// The synchronous probe behind [`Self::is_url_reachable`]: `(reachable,
    /// transport error)`; see [`Self::http_get_blocking`].
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[must_use]
    pub fn is_url_reachable_blocking(&self, url: AzString) -> (bool, Option<AzString>) {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_reachable(url.as_str()) {
            return mocked;
        }
        match http_request_with_config(HttpMethod::Head, url.as_str(), None, "", self) {
            Ok(response) => (response.is_success(), None),
            Err(e) => (false, Some(AzString::from(e.to_string()))),
        }
    }

    /// Stub: `http` feature disabled. The answer self-describes through the
    /// error instead of a bare `false` that reads exactly like "server down".
    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[must_use]
    pub fn is_url_reachable_blocking(&self, url: AzString) -> (bool, Option<AzString>) {
        #[cfg(feature = "text_layout")]
        if let Some(mocked) = mocked_reachable(url.as_str()) {
            return mocked;
        }
        (
            false,
            Some(AzString::from(
                "http feature not enabled: this build has no network transport (rebuild \
                 azul-layout with the `http` feature)",
            )),
        )
    }
}

// ============================================================================
// Response (C-compatible)
// ============================================================================

/// HTTP response with status code, headers, and body
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct HttpResponse {
    /// HTTP status code (200, 404, etc.)
    pub status_code: u16,
    /// Response body as bytes
    pub body: U8Vec,
    /// Content-Type header value
    pub content_type: AzString,
    /// Content-Length header value (0 if unknown)
    pub content_length: u64,
    /// Response headers
    pub headers: HttpHeaderVec,
    /// The URL this response came from: the requested URL, or where its
    /// redirects ended (a feed or a page that moved). Resolve relative links
    /// in the body against this, not against the requested URL.
    pub final_url: AzString,
}

impl HttpResponse {
    /// Check if the response was successful (2xx status)
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status_code >= 200 && self.status_code < 300
    }

    /// Check if the response is a redirect (3xx status)
    #[must_use]
    pub const fn is_redirect(&self) -> bool {
        self.status_code >= 300 && self.status_code < 400
    }

    /// Check if the response is a client error (4xx status)
    #[must_use]
    pub const fn is_client_error(&self) -> bool {
        self.status_code >= 400 && self.status_code < 500
    }

    /// Check if the response is a server error (5xx status)
    #[must_use]
    pub const fn is_server_error(&self) -> bool {
        self.status_code >= 500 && self.status_code < 600
    }

    /// Try to convert the body to a UTF-8 string
    #[must_use]
    pub fn body_as_string(&self) -> Option<AzString> {
        core::str::from_utf8(self.body.as_slice())
            .ok()
            .map(|s| AzString::from(s.to_string()))
    }
}

// FFI-safe Result types for HTTP operations (must be after HttpResponse definition)
impl_result!(
    HttpResponse,
    HttpError,
    ResultHttpResponseHttpError,
    copy = false,
    clone = false,
    [Debug, Clone, PartialEq]
);

impl_result!(
    U8Vec,
    HttpError,
    ResultU8VecHttpError,
    copy = false,
    clone = false,
    [Debug, Clone, PartialEq, Eq]
);

// ============================================================================
// Resumable request results
// ============================================================================
//
// `HttpRequestConfig::http_get` / `http_post` / `http_request` /
// `download_bytes` / `is_url_reachable` are request functions (see
// `crate::request`): they return a `RequestId` and resume the caller's
// `ResumeCallback` with one of these structs, type-erased into a `RefAny`.

/// Result of [`HttpRequestConfig::http_get`], [`HttpRequestConfig::http_post`]
/// and [`HttpRequestConfig::http_request`].
#[derive(Debug, Clone)]
#[repr(C)]
pub struct HttpGetResult {
    pub result: ResultHttpResponseHttpError,
}

impl_option!(
    HttpGetResult,
    OptionHttpGetResult,
    copy = false,
    [Debug, Clone]
);

impl HttpGetResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionHttpGetResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Result of [`HttpRequestConfig::download_bytes`].
#[derive(Debug, Clone)]
#[repr(C)]
pub struct HttpBytesResult {
    pub result: ResultU8VecHttpError,
}

impl_option!(
    HttpBytesResult,
    OptionHttpBytesResult,
    copy = false,
    [Debug, Clone]
);

impl HttpBytesResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionHttpBytesResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Result of [`HttpRequestConfig::is_url_reachable`]. `reachable` is `true`
/// for a 2xx answer to a HEAD request; `error` carries the transport error
/// when the request could not be made at all (`None` for a plain non-2xx
/// status).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HttpReachableResult {
    pub reachable: bool,
    pub error: azul_css::corety::OptionString,
}

impl_option!(
    HttpReachableResult,
    OptionHttpReachableResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl HttpReachableResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionHttpReachableResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Simple HTTP GET request
///
/// # Arguments
/// * `url` - The URL to request
///
/// # Returns
/// * `HttpResult<HttpResponse>` - The response or an error
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub fn http_get(url: &str) -> HttpResult<HttpResponse> {
    http_get_with_config(url, &HttpRequestConfig::default())
}

/// Stub: `http` feature disabled.
#[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is
/// disabled).
pub fn http_get(_url: &str) -> HttpResult<HttpResponse> {
    Err(HttpError::other("http feature not enabled".into()))
}

/// HTTP GET request with custom configuration
///
/// # Arguments
/// * `url` - The URL to request
/// * `config` - Request configuration
///
/// # Returns
/// * `HttpResult<HttpResponse>` - The response or an error
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn make_agent(timeout_secs: u64, disable_tls_cert_verification: bool) -> ureq::Agent {
    use std::time::Duration;

    // The fallback resolver without addresses: a lookup error comes back as a DNS failure.
    ureq::Agent::with_parts(
        agent_config(disable_tls_cert_verification)
            .timeout_global(Some(Duration::from_secs(timeout_secs)))
            .build(),
        ureq::unversioned::transport::DefaultConnector::default(),
        FallbackResolver {
            inner: ureq::unversioned::resolver::DefaultResolver::default(),
            fallback: FallbackAddresses::default(),
        },
    )
}

/// How long one DNS lookup may take before it counts as failed ([`HttpError::DnsFailed`]): a
/// resolver that hangs (DNS down) must not use up the request's whole time - what is left goes
/// to a cached answer or a fallback address ([`HttpClient::add_fallback_address`]).
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub const DNS_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// How long a client's cached DNS answer is still served once its name stops resolving
/// (serve-stale, [`HttpClientConfig::dns_cache_secs`]): seven days.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub const DNS_STALE_FOR: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

/// After a lookup failed, how long a client with a DNS cache answers the name from what it has
/// (the stale answer, else the failure) without asking again: while DNS is down every request
/// would otherwise wait for the lookup to give up first.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
const DNS_FAILURE_HOLD: std::time::Duration = std::time::Duration::from_secs(30);

/// A lookup that failed, as the resolver chain hands it to ureq: [`map_ureq_error`] makes it an
/// [`HttpError::DnsFailed`] (not text that a caller has to recognize).
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
#[derive(Debug)]
struct DnsFailure(String);

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl fmt::Display for DnsFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl std::error::Error for DnsFailure {}

/// `error` of a lookup as a [`DnsFailure`] when it is one (the system resolver's I/O error, no
/// address, the lookup's time ran out); any other error (a bad URL) as it is.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn dns_failure(error: ureq::Error) -> ureq::Error {
    match error {
        ureq::Error::Io(_) | ureq::Error::HostNotFound | ureq::Error::Timeout(_) => {
            ureq::Error::Other(Box::new(DnsFailure(error.to_string())))
        }
        other => other,
    }
}

/// The agent settings every request shares, whether its agent is built for one
/// request ([`make_agent`]) or kept in an [`HttpClient`].
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn agent_config(
    disable_tls_cert_verification: bool,
) -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    let mut tls_builder = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .unversioned_rustls_crypto_provider(std::sync::Arc::new(rustls_rustcrypto::provider()));

    if disable_tls_cert_verification {
        tls_builder = tls_builder.disable_verification(true);
    } else {
        tls_builder = tls_builder.root_certs(ureq::tls::RootCerts::WebPki);
    }

    let tls_config = tls_builder.build();

    ureq::Agent::config_builder()
        .tls_config(tls_config)
        .http_status_as_error(false)
        .timeout_resolve(Some(DNS_LOOKUP_TIMEOUT))
}

/// The agent behind an [`HttpClient`]: pooled per `config`, and resolving hosts
/// through `resolver`, cached when `config.dns_cache_secs` asks for it; a host whose
/// lookup fails is answered from `fallback`.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn client_agent(
    config: &HttpClientConfig,
    resolver: impl ureq::unversioned::resolver::Resolver,
    fallback: FallbackAddresses,
) -> ureq::Agent {
    let agent_config = agent_config(config.disable_tls_cert_verification)
        .max_idle_connections(config.max_idle_connections as usize)
        .max_idle_connections_per_host(config.max_idle_connections_per_host as usize)
        .build();
    let connector = ureq::unversioned::transport::DefaultConnector::default();
    if config.dns_cache_secs == 0 {
        ureq::Agent::with_parts(
            agent_config,
            connector,
            FallbackResolver {
                inner: resolver,
                fallback,
            },
        )
    } else {
        ureq::Agent::with_parts(
            agent_config,
            connector,
            FallbackResolver {
                inner: CachingResolver::new(
                    resolver,
                    std::time::Duration::from_secs(u64::from(config.dns_cache_secs)),
                ),
                fallback,
            },
        )
    }
}

/// The most fallback addresses of one host (ureq's resolver answers at most 16).
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
const MAX_FALLBACK_ADDRESSES: usize = 16;

/// A client's fallback addresses: host (lowercase, no brackets) -> its addresses, each with
/// the port it names (`None`: the URL's).
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
type FallbackAddresses = alloc::sync::Arc<
    std::sync::Mutex<alloc::collections::BTreeMap<String, Vec<(std::net::IpAddr, Option<u16>)>>>,
>;

/// A host as the fallback addresses are kept under: lowercase, an IPv6 one without brackets.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn fallback_host(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}

/// `192.0.2.7`, `2001:db8::1`, `[2001:db8::1]`, `192.0.2.7:8443`, `[2001:db8::1]:8443`.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn parse_fallback_address(address: &str) -> Option<(std::net::IpAddr, Option<u16>)> {
    let address = address.trim();
    if let Ok(socket) = address.parse::<std::net::SocketAddr>() {
        return Some((socket.ip(), Some(socket.port())));
    }
    address
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| (ip, None))
}

/// A resolver that asks `inner` first and, when the lookup fails, answers the host from
/// the client's fallback addresses: the connection goes to the address, the request (and
/// TLS) still to the host. A lookup that fails without one is a [`DnsFailure`].
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
#[derive(Debug)]
struct FallbackResolver<R> {
    inner: R,
    fallback: FallbackAddresses,
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl<R: ureq::unversioned::resolver::Resolver> ureq::unversioned::resolver::Resolver
    for FallbackResolver<R>
{
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let error = match self.inner.resolve(uri, config, timeout) {
            Ok(found) => return Ok(found),
            Err(error) => error,
        };
        let Some(host) = uri.host().map(fallback_host) else {
            return Err(dns_failure(error));
        };
        let known = self
            .fallback
            .lock()
            .ok()
            .and_then(|known| known.get(&host).cloned())
            .filter(|addresses| !addresses.is_empty());
        let Some(known) = known else {
            return Err(dns_failure(error));
        };
        let default_port = if uri.scheme_str() == Some("https") {
            443
        } else {
            80
        };
        let port = uri.port_u16().unwrap_or(default_port);
        let mut out = self.inner.empty();
        for (ip, named) in known.into_iter().take(MAX_FALLBACK_ADDRESSES) {
            out.push(std::net::SocketAddr::new(ip, named.unwrap_or(port)));
        }
        Ok(out)
    }

    fn empty(&self) -> ureq::unversioned::resolver::ResolvedSocketAddrs {
        self.inner.empty()
    }
}

/// A resolver that answers each `host:port` from memory for `ttl` after asking
/// `inner` once. When a later lookup fails, the last answer is served for `stale_for`
/// ([`DNS_STALE_FOR`]: serve-stale), and the name is not asked again for
/// [`DNS_FAILURE_HOLD`].
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
#[derive(Debug)]
struct CachingResolver<R> {
    inner: R,
    ttl: std::time::Duration,
    stale_for: std::time::Duration,
    answers: std::sync::Mutex<
        std::collections::HashMap<String, (std::time::Instant, Vec<std::net::SocketAddr>)>,
    >,
    /// When the lookup of a `host:port` last failed.
    failed: std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl<R> CachingResolver<R> {
    fn new(inner: R, ttl: std::time::Duration) -> Self {
        Self {
            inner,
            ttl,
            stale_for: DNS_STALE_FOR,
            answers: std::sync::Mutex::new(std::collections::HashMap::new()),
            failed: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Serves an expired answer for `stale_for` when the lookup fails (instead of seven days).
    #[cfg(test)]
    fn with_stale_for(mut self, stale_for: std::time::Duration) -> Self {
        self.stale_for = stale_for;
        self
    }

    /// The answer of `key` if it is younger than `age`.
    fn answer_within(
        &self,
        key: &str,
        age: std::time::Duration,
    ) -> Option<Vec<std::net::SocketAddr>> {
        self.answers.lock().ok().and_then(|answers| {
            answers
                .get(key)
                .filter(|(at, _)| at.elapsed() < age)
                .map(|(_, addrs)| addrs.clone())
        })
    }

    /// Whether the lookup of `key` failed less than [`DNS_FAILURE_HOLD`] ago.
    fn failed_recently(&self, key: &str) -> bool {
        self.failed.lock().ok().is_some_and(|failed| {
            failed
                .get(key)
                .is_some_and(|at| at.elapsed() < DNS_FAILURE_HOLD)
        })
    }
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl<R: ureq::unversioned::resolver::Resolver> ureq::unversioned::resolver::Resolver
    for CachingResolver<R>
{
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let key = uri
            .scheme()
            .zip(uri.authority())
            .and_then(|(scheme, authority)| {
                ureq::unversioned::resolver::DefaultResolver::host_and_port(scheme, authority)
            });
        let Some(key) = key else {
            return self.inner.resolve(uri, config, timeout); // let it report the bad URL
        };
        let answer = |addrs: Vec<std::net::SocketAddr>| {
            let mut out = self.inner.empty();
            for addr in addrs {
                out.push(addr);
            }
            out
        };
        if let Some(addrs) = self.answer_within(&key, self.ttl) {
            return Ok(answer(addrs));
        }
        let stale = self.answer_within(&key, self.stale_for);
        if self.failed_recently(&key) {
            // DNS was down a moment ago: what is known now, without waiting for the lookup.
            return match stale {
                Some(addrs) => Ok(answer(addrs)),
                None => Err(ureq::Error::HostNotFound),
            };
        }
        match self.inner.resolve(uri, config, timeout) {
            Ok(resolved) => {
                if let Ok(mut answers) = self.answers.lock() {
                    answers.insert(
                        key.clone(),
                        (
                            std::time::Instant::now(),
                            resolved.iter().copied().collect(),
                        ),
                    );
                }
                if let Ok(mut failed) = self.failed.lock() {
                    failed.remove(&key);
                }
                Ok(resolved)
            }
            Err(error) => {
                if let Ok(mut failed) = self.failed.lock() {
                    failed.insert(key, std::time::Instant::now());
                }
                // The last answer, if it is not too old (serve-stale); else the failure.
                stale.map(answer).ok_or(error)
            }
        }
    }

    fn empty(&self) -> ureq::unversioned::resolver::ResolvedSocketAddrs {
        self.inner.empty()
    }
}

/// HTTP verb for [`http_request_with_config`].
///
/// Rust-side only — this type deliberately has no C-ABI mirror in `api.json`;
/// the C bindings keep the pre-existing `HttpRequestConfig::http_get` /
/// `download_bytes` entry points.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// The uppercase wire name of the verb.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    /// Whether this verb carries a request body.
    ///
    /// Mirrors ureq's request typestate split: `POST`/`PUT`/`PATCH` build a
    /// `WithBody` request terminated by `send()`, while `GET`/`HEAD`/`DELETE`
    /// build a `WithoutBody` one terminated by `call()`.
    #[must_use]
    pub const fn takes_body(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Maps a ureq transport error onto the C-ABI-safe [`HttpError`].
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn map_ureq_error(url: &str, e: &ureq::Error) -> HttpError {
    match e {
        // The resolver gave up: the name is the problem, not a slow server.
        ureq::Error::Timeout(ureq::Timeout::Resolve) => {
            HttpError::dns_failed(format!("{url}: the lookup of the name timed out").into())
        }
        ureq::Error::Timeout(_) => HttpError::Timeout,
        ureq::Error::HostNotFound => {
            HttpError::dns_failed(format!("{url}: the name did not resolve").into())
        }
        ureq::Error::Other(inner) if inner.downcast_ref::<DnsFailure>().is_some() => {
            HttpError::dns_failed(format!("{url}: {inner}").into())
        }
        ureq::Error::ConnectionFailed => {
            HttpError::connection_failed(format!("Connection failed: {url}").into())
        }
        ureq::Error::Io(io_err) => HttpError::io_error(format!("{io_err}").into()),
        ureq::Error::BadUri(msg) => HttpError::invalid_url(format!("{url}: {msg}").into()),
        ureq::Error::Tls(msg) => HttpError::tls_error(format!("TLS error: {msg}").into()),
        // Catch-all for feature-gated variants (Rustls, Pem, etc.)
        _ => {
            let msg = e.to_string();
            if msg.starts_with("rustls:") || msg.contains("TLS") || msg.contains("certificate") {
                HttpError::tls_error(msg.into())
            } else {
                HttpError::other(msg.into())
            }
        }
    }
}

/// Turns a ureq response into the C-ABI [`HttpResponse`], enforcing
/// `config.max_response_size` both on the advertised `Content-Length` and on
/// the actual number of bytes read.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn decode_response(
    response: ureq::http::Response<ureq::Body>,
    config: &HttpRequestConfig,
) -> HttpResult<HttpResponse> {
    use std::io::Read;

    use ureq::ResponseExt as _;

    let status_code = response.status().as_u16();
    // Where the request ended: ureq follows redirects (up to 10) and records
    // the last URI on the response.
    let final_url = AzString::from(response.get_uri().to_string());
    let content_type = AzString::from(
        response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string(),
    );
    let content_length = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Collect response headers
    let mut headers = Vec::new();
    for (name, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            headers.push(HttpHeader::new(name.to_string(), v.to_string()));
        }
    }

    // Check response size limit
    if config.max_response_size > 0 && content_length > config.max_response_size {
        return Err(HttpError::response_too_large(
            config.max_response_size,
            content_length,
        ));
    }

    // Read body with size limit
    let mut body = Vec::new();
    let limit = if config.max_response_size > 0 {
        config.max_response_size as usize
    } else {
        usize::MAX
    };
    let mut body_reader = response.into_body();
    let mut reader = body_reader.as_reader().take(limit as u64);
    reader
        .read_to_end(&mut body)
        .map_err(|e| HttpError::io_error(e.to_string().into()))?;

    Ok(HttpResponse {
        status_code,
        body: U8Vec::from(body),
        content_type,
        content_length,
        headers: HttpHeaderVec::from_vec(headers),
        final_url,
    })
}

/// Generic HTTP request with an optional body — the single code path every
/// verb-specific helper in this module funnels through.
///
/// `content_type` is applied only when a body is present; explicit entries in
/// `config.headers` are applied afterwards and therefore win. To gzip a
/// request body, compress it yourself and pass
/// `HttpRequestConfig::with_header("Content-Encoding", "gzip")`.
///
/// Note that 4xx/5xx are returned as an `Ok(HttpResponse)` with the status
/// code set (the agent is built with `http_status_as_error(false)`); only
/// transport failures produce an `Err`.
///
/// # Errors
///
/// Returns an `HttpError` on DNS/connect/TLS/IO failure, on timeout, if the
/// response exceeds `config.max_response_size`, or if the `http` feature is
/// disabled.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub fn http_request_with_config(
    method: HttpMethod,
    url: &str,
    body: Option<&[u8]>,
    content_type: &str,
    config: &HttpRequestConfig,
) -> HttpResult<HttpResponse> {
    // A pooled agent was built without this request's timeout, so every request
    // below sets it on itself; for a one-off agent that restates the same value.
    let agent = match &config.client {
        OptionHttpClient::Some(client) => client.ptr.agent.clone(),
        OptionHttpClient::None => {
            make_agent(config.timeout_secs, config.disable_tls_cert_verification)
        }
    };
    let timeout = Some(std::time::Duration::from_secs(config.timeout_secs));

    // ureq 3.x splits the request builder by typestate: `WithoutBody` for
    // GET/HEAD/DELETE (terminated by `.call()`) and `WithBody` for
    // POST/PUT/PATCH (terminated by `.send()`). The two are different types,
    // so the header application is written out per branch.
    let response = if method.takes_body() {
        let mut request = match method {
            HttpMethod::Put => agent.put(url),
            HttpMethod::Patch => agent.patch(url),
            // `takes_body()` admits only POST/PUT/PATCH here.
            _ => agent.post(url),
        };
        if !config.user_agent.as_str().is_empty() {
            request = request.header("User-Agent", config.user_agent.as_str());
        }
        if !content_type.is_empty() {
            request = request.header("Content-Type", content_type);
        }
        for header in config.headers.as_slice() {
            request = request.header(header.name.as_str(), header.value.as_str());
        }
        request
            .config()
            .timeout_global(timeout)
            .build()
            .send(body.unwrap_or(&[]))
            .map_err(|e| map_ureq_error(url, &e))?
    } else {
        let mut request = match method {
            HttpMethod::Head => agent.head(url),
            HttpMethod::Delete => agent.delete(url),
            // `takes_body()` admits only GET/HEAD/DELETE here.
            _ => agent.get(url),
        };
        if !config.user_agent.as_str().is_empty() {
            request = request.header("User-Agent", config.user_agent.as_str());
        }
        for header in config.headers.as_slice() {
            request = request.header(header.name.as_str(), header.value.as_str());
        }
        request
            .config()
            .timeout_global(timeout)
            .build()
            .call()
            .map_err(|e| map_ureq_error(url, &e))?
    };

    decode_response(response, config)
}

/// Stub: `http` feature disabled.
///
/// # Errors
///
/// Always returns an `HttpError` — the networking feature is disabled.
#[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
pub fn http_request_with_config(
    _method: HttpMethod,
    _url: &str,
    _body: Option<&[u8]>,
    _content_type: &str,
    _config: &HttpRequestConfig,
) -> HttpResult<HttpResponse> {
    Err(HttpError::other("http feature not enabled".into()))
}

/// HTTP GET request with custom configuration.
///
/// # Errors
///
/// See [`http_request_with_config`].
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub fn http_get_with_config(url: &str, config: &HttpRequestConfig) -> HttpResult<HttpResponse> {
    http_request_with_config(HttpMethod::Get, url, None, "", config)
}

/// Stub: `http` feature disabled.
#[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is
/// disabled).
pub fn http_get_with_config(_url: &str, _config: &HttpRequestConfig) -> HttpResult<HttpResponse> {
    Err(HttpError::other("http feature not enabled".into()))
}

/// HTTP POST with the default configuration.
///
/// # Errors
///
/// See [`http_request_with_config`].
pub fn http_post(url: &str, body: &[u8], content_type: &str) -> HttpResult<HttpResponse> {
    http_post_with_config(url, body, content_type, &HttpRequestConfig::default())
}

/// HTTP POST with custom configuration.
///
/// This is the transport under the telemetry uploader (OTLP/HTTP JSON), crash
/// bundle upload and the update-manifest fetch.
///
/// # Errors
///
/// See [`http_request_with_config`].
pub fn http_post_with_config(
    url: &str,
    body: &[u8],
    content_type: &str,
    config: &HttpRequestConfig,
) -> HttpResult<HttpResponse> {
    http_request_with_config(HttpMethod::Post, url, Some(body), content_type, config)
}

/// HTTP PUT with custom configuration.
///
/// # Errors
///
/// See [`http_request_with_config`].
pub fn http_put_with_config(
    url: &str,
    body: &[u8],
    content_type: &str,
    config: &HttpRequestConfig,
) -> HttpResult<HttpResponse> {
    http_request_with_config(HttpMethod::Put, url, Some(body), content_type, config)
}

/// Download a URL to bytes (convenience wrapper with default config)
///
/// # Arguments
/// * `url` - The URL to download
///
/// # Returns
/// * `HttpResult<U8Vec>` - The response body or an error
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub fn download_bytes(url: &str) -> HttpResult<U8Vec> {
    download_bytes_with_config(url, &HttpRequestConfig::default())
}

/// Stub: `http` feature disabled.
#[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is
/// disabled).
pub fn download_bytes(_url: &str) -> HttpResult<U8Vec> {
    Err(HttpError::other("http feature not enabled".into()))
}

/// Download a URL to bytes with custom configuration
///
/// # Arguments
/// * `url` - The URL to download
/// * `config` - Request configuration (timeout, max size, etc.)
///
/// # Returns
/// * `HttpResult<U8Vec>` - The response body or an error
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub fn download_bytes_with_config(url: &str, config: &HttpRequestConfig) -> HttpResult<U8Vec> {
    let response = http_get_with_config(url, config)?;

    // Check for successful status
    if response.status_code >= 400 {
        return Err(HttpError::http_status(
            response.status_code,
            format!("HTTP error {}", response.status_code).into(),
        ));
    }

    Ok(response.body)
}

/// Stub: `http` feature disabled.
#[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is
/// disabled).
pub fn download_bytes_with_config(_url: &str, _config: &HttpRequestConfig) -> HttpResult<U8Vec> {
    Err(HttpError::other("http feature not enabled".into()))
}

/// Check if a URL is reachable (HEAD request)
///
/// # Arguments
/// * `url` - The URL to check
///
/// # Returns
/// * `bool` - True if reachable (2xx status)
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
#[must_use]
pub fn is_url_reachable(url: &str) -> bool {
    const REACHABILITY_TIMEOUT_SECS: u64 = 10;
    let agent = make_agent(REACHABILITY_TIMEOUT_SECS, false);
    match agent.head(url).call() {
        Ok(resp) => {
            let code = resp.status().as_u16();
            (200..300).contains(&code)
        }
        Err(_) => false,
    }
}

/// Stub: `http` feature disabled.
#[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
#[must_use]
pub const fn is_url_reachable(_url: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_request_config_default() {
        let config = HttpRequestConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_response_size, 100 * 1024 * 1024);
        assert!(!config.user_agent.as_str().is_empty());
    }

    #[test]
    fn test_http_response_status_checks() {
        let response = HttpResponse {
            status_code: 200,
            body: U8Vec::from(Vec::new()),
            content_type: AzString::from(String::new()),
            content_length: 0,
            headers: HttpHeaderVec::from_const_slice(&[]),
            final_url: AzString::from(String::new()),
        };
        assert!(response.is_success());
        assert!(!response.is_redirect());
        assert!(!response.is_client_error());
        assert!(!response.is_server_error());
    }

    #[test]
    fn test_http_error_constructors() {
        let err = HttpError::http_status(404, "Not Found".into());
        assert!(err.to_string().contains("404"));

        let err2 = HttpError::response_too_large(100, 200);
        assert!(err2.to_string().contains("200"));
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // =========================================================================
    // Shared fixtures
    //
    // Everything below is offline: the `http`-gated tests only touch URIs that
    // fail during URI parsing (no DNS lookup, no socket) or construct a ureq
    // agent without ever calling it.
    // =========================================================================

    /// 256 KiB of ASCII — used to check the constructors don't choke on big payloads.
    fn huge_ascii() -> String {
        "A".repeat(256 * 1024)
    }

    /// A string designed to break naive formatting / escaping.
    const NASTY: &str = "\u{0}\r\n\t\"{}{{}}%s%n\u{7f}héllo·🦀·\u{202e}\u{feff}";

    fn response_with_status(status_code: u16) -> HttpResponse {
        HttpResponse {
            status_code,
            body: U8Vec::from(Vec::new()),
            content_type: AzString::from("application/octet-stream"),
            content_length: 0,
            headers: HttpHeaderVec::from_const_slice(&[]),
            final_url: AzString::from("http://example.com/"),
        }
    }

    fn response_with_body(body: Vec<u8>) -> HttpResponse {
        HttpResponse {
            status_code: 200,
            body: U8Vec::from(body),
            content_type: AzString::from("text/plain"),
            content_length: 0,
            headers: HttpHeaderVec::from_const_slice(&[]),
            final_url: AzString::from("http://example.com/"),
        }
    }

    // =========================================================================
    // HttpError constructors (`other` category) — extreme AzString payloads
    // =========================================================================

    #[test]
    fn http_error_string_constructors_store_payload_verbatim() {
        for payload in ["", "http://example.com", NASTY, huge_ascii().as_str()] {
            let s = AzString::from(payload);

            assert_eq!(
                HttpError::invalid_url(s.clone()),
                HttpError::InvalidUrl(s.clone())
            );
            assert_eq!(
                HttpError::connection_failed(s.clone()),
                HttpError::ConnectionFailed(s.clone())
            );
            assert_eq!(
                HttpError::tls_error(s.clone()),
                HttpError::TlsError(s.clone())
            );
            assert_eq!(
                HttpError::io_error(s.clone()),
                HttpError::IoError(s.clone())
            );
            assert_eq!(HttpError::other(s.clone()), HttpError::Other(s.clone()));

            // The payload survives the round-trip through the enum untouched:
            // no truncation at NUL, no escaping, no normalization.
            match HttpError::invalid_url(s.clone()) {
                HttpError::InvalidUrl(inner) => assert_eq!(inner.as_str(), payload),
                other => panic!("wrong variant: {other:?}"),
            }
        }
    }

    #[test]
    fn http_error_variants_are_not_conflated() {
        let s = AzString::from("x");
        assert_ne!(
            HttpError::invalid_url(s.clone()),
            HttpError::other(s.clone())
        );
        assert_ne!(
            HttpError::tls_error(s.clone()),
            HttpError::io_error(s.clone())
        );
        assert_ne!(HttpError::connection_failed(s.clone()), HttpError::Timeout);
    }

    // =========================================================================
    // HttpError::http_status / response_too_large (`numeric` category)
    // =========================================================================

    #[test]
    fn http_status_accepts_full_u16_range_without_clamping() {
        // 0 and u16::MAX are not valid HTTP status codes, but the constructor is
        // a plain data carrier: it must store them as-is rather than clamp/panic.
        for code in [0_u16, 1, 99, 100, 200, 299, 400, 599, 600, 999, u16::MAX] {
            let err = HttpError::http_status(code, AzString::from("msg"));
            match err {
                HttpError::HttpStatus(ref e) => {
                    assert_eq!(e.status_code, code);
                    assert_eq!(e.message.as_str(), "msg");
                }
                ref other => panic!("wrong variant: {other:?}"),
            }
            // Display must render the raw number, never a saturated stand-in.
            assert!(err.to_string().contains(&code.to_string()));
        }
    }

    #[test]
    fn http_status_with_empty_and_huge_message() {
        let empty = HttpError::http_status(u16::MAX, AzString::from(""));
        assert_eq!(empty.to_string(), "HTTP 65535 - ");

        let big = huge_ascii();
        let huge = HttpError::http_status(0, AzString::from(big.as_str()));
        assert_eq!(huge.to_string().len(), "HTTP 0 - ".len() + big.len());
    }

    #[test]
    fn response_too_large_stores_both_sizes_at_u64_limits() {
        // Includes the nonsensical actual < max ordering: the constructor performs
        // no validation and no arithmetic, so nothing can overflow here.
        for (max, actual) in [
            (0_u64, 0_u64),
            (0, u64::MAX),
            (u64::MAX, 0),
            (u64::MAX, u64::MAX),
            (1, 1),
            (100, 200),
            (u64::MAX, u64::MAX - 1),
        ] {
            let err = HttpError::response_too_large(max, actual);
            match err {
                HttpError::ResponseTooLarge(ref e) => {
                    assert_eq!(e.max_size, max);
                    assert_eq!(e.actual_size, actual);
                }
                ref other => panic!("wrong variant: {other:?}"),
            }
            let msg = err.to_string();
            assert!(msg.contains(&actual.to_string()));
            assert!(msg.contains(&max.to_string()));
        }
    }

    // =========================================================================
    // Display impl (`serializer` category)
    // =========================================================================

    #[test]
    fn display_is_non_empty_for_every_variant() {
        let variants = [
            HttpError::invalid_url(AzString::from("u")),
            HttpError::connection_failed(AzString::from("c")),
            HttpError::Timeout,
            HttpError::tls_error(AzString::from("t")),
            HttpError::http_status(500, AzString::from("s")),
            HttpError::io_error(AzString::from("i")),
            HttpError::response_too_large(1, 2),
            HttpError::other(AzString::from("o")),
            HttpError::dns_failed(AzString::from("d")),
        ];
        for v in &variants {
            let s = v.to_string();
            assert!(!s.is_empty(), "empty Display for {v:?}");
        }
        assert_eq!(HttpError::Timeout.to_string(), "Request timed out");
    }

    #[test]
    fn display_does_not_interpret_the_payload_as_a_format_string() {
        // A payload full of `{}` / `%s` must be echoed literally — a Display impl
        // that re-formatted its own output would either panic or eat the braces.
        let err = HttpError::other(AzString::from("{} {0} {{}} %s %n"));
        assert_eq!(err.to_string(), "HTTP error: {} {0} {{}} %s %n");
    }

    #[test]
    fn display_preserves_nul_newlines_and_unicode() {
        let err = HttpError::invalid_url(AzString::from(NASTY));
        let s = err.to_string();
        assert!(s.starts_with("Invalid URL: "));
        assert!(s.ends_with(NASTY));
        assert!(s.contains('\u{0}'));
        assert!(s.contains('🦀'));
    }

    #[test]
    fn display_of_edge_numeric_values_does_not_panic() {
        assert_eq!(
            HttpError::http_status(u16::MAX, AzString::from("x")).to_string(),
            "HTTP 65535 - x"
        );
        assert_eq!(
            HttpError::response_too_large(u64::MAX, u64::MAX).to_string(),
            format!("Response too large: {} bytes (max: {})", u64::MAX, u64::MAX)
        );
        assert_eq!(
            HttpError::response_too_large(0, 0).to_string(),
            "Response too large: 0 bytes (max: 0)"
        );
    }

    // =========================================================================
    // HttpHeader::new (`constructor` category)
    // =========================================================================

    #[test]
    fn http_header_new_keeps_fields_exactly_as_given() {
        for (name, value) in [
            ("", ""),
            ("Content-Type", "text/html; charset=utf-8"),
            (NASTY, NASTY),
            (huge_ascii().as_str(), ""),
            ("", huge_ascii().as_str()),
        ] {
            let h = HttpHeader::new(name, value);
            assert_eq!(h.name.as_str(), name);
            assert_eq!(h.value.as_str(), value);
        }
    }

    #[test]
    fn http_header_new_does_not_sanitize_crlf() {
        // Documented behaviour, not an endorsement: HttpHeader is a dumb pair, so a
        // CRLF-bearing name is stored verbatim. Rejecting it is the transport's job
        // (ureq validates at request time) — assert the value is at least not
        // silently truncated at the newline, which would hide the injection attempt.
        let h = HttpHeader::new("X-Evil\r\nInjected: 1", "v\r\nSet-Cookie: pwned=1");
        assert_eq!(h.name.as_str(), "X-Evil\r\nInjected: 1");
        assert_eq!(h.value.as_str(), "v\r\nSet-Cookie: pwned=1");
    }

    #[test]
    fn http_header_new_accepts_string_and_str() {
        let from_str = HttpHeader::new("a", "b");
        let from_string = HttpHeader::new(String::from("a"), String::from("b"));
        assert_eq!(from_str, from_string);
    }

    // =========================================================================
    // HttpRequestConfig builders (`constructor` category)
    // =========================================================================

    #[test]
    fn config_new_matches_default_and_documented_values() {
        let a = HttpRequestConfig::new();
        let b = HttpRequestConfig::default();
        assert_eq!(a.timeout_secs, b.timeout_secs);
        assert_eq!(a.max_response_size, b.max_response_size);
        assert_eq!(a.user_agent.as_str(), b.user_agent.as_str());
        assert_eq!(a.headers.len(), b.headers.len());
        assert_eq!(
            a.disable_tls_cert_verification,
            b.disable_tls_cert_verification
        );

        assert_eq!(a.timeout_secs, 30);
        assert_eq!(a.max_response_size, 100 * 1024 * 1024);
        assert!(a.headers.is_empty());
        // Secure by default: certificate verification must be ON unless opted out.
        assert!(!a.disable_tls_cert_verification);
    }

    #[test]
    fn with_timeout_stores_extremes_verbatim() {
        for secs in [0_u64, 1, 30, u64::MAX / 2, u64::MAX - 1, u64::MAX] {
            let cfg = HttpRequestConfig::new().with_timeout(secs);
            assert_eq!(cfg.timeout_secs, secs);
            // Nothing else may be disturbed by the setter.
            assert_eq!(cfg.max_response_size, 100 * 1024 * 1024);
            assert!(cfg.headers.is_empty());
        }
    }

    #[test]
    fn with_max_size_stores_extremes_verbatim() {
        for max in [0_u64, 1, u64::MAX] {
            let cfg = HttpRequestConfig::new().with_max_size(max);
            assert_eq!(cfg.max_response_size, max);
            assert_eq!(cfg.timeout_secs, 30);
        }
        // 0 is the documented "unlimited" sentinel, not a "reject everything" limit.
        assert_eq!(
            HttpRequestConfig::new().with_max_size(0).max_response_size,
            0
        );
    }

    #[test]
    fn builder_setters_are_last_write_wins_and_independent() {
        let cfg = HttpRequestConfig::new()
            .with_timeout(1)
            .with_timeout(u64::MAX)
            .with_max_size(5)
            .with_max_size(0)
            .with_user_agent("first")
            .with_user_agent("second");

        assert_eq!(cfg.timeout_secs, u64::MAX);
        assert_eq!(cfg.max_response_size, 0);
        assert_eq!(cfg.user_agent.as_str(), "second");
    }

    #[test]
    fn with_user_agent_accepts_empty_and_extreme_values() {
        let empty = HttpRequestConfig::new().with_user_agent("");
        // Empty UA is meaningful: http_get_with_config skips the header entirely.
        assert!(empty.user_agent.as_str().is_empty());

        let unicode = HttpRequestConfig::new().with_user_agent(NASTY);
        assert_eq!(unicode.user_agent.as_str(), NASTY);

        let big = huge_ascii();
        let huge = HttpRequestConfig::new().with_user_agent(big.clone());
        assert_eq!(huge.user_agent.as_str().len(), big.len());
    }

    #[test]
    fn with_header_appends_in_order_and_keeps_duplicates() {
        let mut cfg = HttpRequestConfig::new();
        assert!(cfg.headers.is_empty());

        for i in 0..100_usize {
            cfg = cfg.with_header(format!("H{i}"), format!("v{i}"));
        }
        // Duplicate names are kept, not deduplicated or overwritten.
        cfg = cfg.with_header("H0", "second-value");

        assert_eq!(cfg.headers.len(), 101);
        let slice = cfg.headers.as_slice();
        for (i, h) in slice.iter().take(100).enumerate() {
            assert_eq!(h.name.as_str(), format!("H{i}"));
            assert_eq!(h.value.as_str(), format!("v{i}"));
        }
        assert_eq!(slice[100].name.as_str(), "H0");
        assert_eq!(slice[100].value.as_str(), "second-value");
    }

    #[test]
    fn with_header_accepts_empty_name_and_value() {
        let cfg = HttpRequestConfig::new().with_header("", "");
        assert_eq!(cfg.headers.len(), 1);
        assert!(cfg.headers.as_slice()[0].name.as_str().is_empty());
        assert!(cfg.headers.as_slice()[0].value.as_str().is_empty());
    }

    #[test]
    fn cloning_a_config_gives_an_independent_header_vec() {
        // The header vec is an FFI vec with a destructor field; a shallow clone that
        // aliased the original's buffer would show up here (and later double-free).
        let base = HttpRequestConfig::new().with_header("A", "1");
        let cloned = base.clone().with_header("B", "2");

        assert_eq!(base.headers.len(), 1);
        assert_eq!(cloned.headers.len(), 2);
        assert_eq!(base.headers.as_slice()[0].name.as_str(), "A");
        assert_eq!(cloned.headers.as_slice()[0].name.as_str(), "A");
        assert_eq!(cloned.headers.as_slice()[1].name.as_str(), "B");

        drop(cloned);
        // `base` must still be readable after the clone is dropped.
        assert_eq!(base.headers.as_slice()[0].value.as_str(), "1");
    }

    // =========================================================================
    // HttpResponse predicates (`predicate` category)
    // =========================================================================

    #[test]
    fn status_predicates_at_class_boundaries() {
        let cases: &[(u16, bool, bool, bool, bool)] = &[
            // status, success, redirect, client_err, server_err
            (0, false, false, false, false),
            (100, false, false, false, false),
            (199, false, false, false, false),
            (200, true, false, false, false),
            (204, true, false, false, false),
            (299, true, false, false, false),
            (300, false, true, false, false),
            (399, false, true, false, false),
            (400, false, false, true, false),
            (499, false, false, true, false),
            (500, false, false, false, true),
            (599, false, false, false, true),
            (600, false, false, false, false),
            (999, false, false, false, false),
            (u16::MAX, false, false, false, false),
        ];

        for &(status, success, redirect, client, server) in cases {
            let r = response_with_status(status);
            assert_eq!(r.is_success(), success, "is_success({status})");
            assert_eq!(r.is_redirect(), redirect, "is_redirect({status})");
            assert_eq!(r.is_client_error(), client, "is_client_error({status})");
            assert_eq!(r.is_server_error(), server, "is_server_error({status})");
        }
    }

    #[test]
    fn status_predicates_are_mutually_exclusive_over_the_whole_u16_range() {
        let mut r = response_with_status(0);
        for status in 0..=u16::MAX {
            r.status_code = status;
            let hits = u8::from(r.is_success())
                + u8::from(r.is_redirect())
                + u8::from(r.is_client_error())
                + u8::from(r.is_server_error());
            assert!(hits <= 1, "status {status} matched {hits} classes");
            // Exactly one class must match inside 200..=599, and none outside it.
            let expected = u8::from((200_u16..600_u16).contains(&status));
            assert_eq!(hits, expected, "status {status}");
        }
    }

    #[test]
    fn status_predicates_ignore_body_and_headers() {
        let mut r = response_with_body(vec![0xFF; 1024]);
        r.status_code = 503;
        r.content_length = u64::MAX;
        r.headers = HttpHeaderVec::from_vec(vec![HttpHeader::new("X", "Y")]);
        assert!(r.is_server_error());
        assert!(!r.is_success());
    }

    // =========================================================================
    // HttpResponse::body_as_string (`getter` category)
    // =========================================================================

    #[test]
    fn body_as_string_on_empty_body_is_some_empty_string() {
        let r = response_with_body(Vec::new());
        let s = r.body_as_string().expect("empty body is valid UTF-8");
        assert_eq!(s.as_str(), "");
    }

    #[test]
    fn body_as_string_round_trips_valid_utf8() {
        for text in ["hello", NASTY, "🦀🦀🦀", "a\u{0}b"] {
            let r = response_with_body(text.as_bytes().to_vec());
            let s = r.body_as_string().expect("valid UTF-8 must decode");
            assert_eq!(s.as_str(), text);
            assert_eq!(s.as_str().len(), text.len());
        }
    }

    #[test]
    fn body_as_string_returns_none_for_invalid_utf8() {
        let invalid: &[&[u8]] = &[
            &[0xFF],                   // never valid
            &[0x80],                   // lone continuation byte
            &[0xC3],                   // truncated 2-byte sequence
            &[0xE2, 0x82],             // truncated 3-byte sequence
            &[0xED, 0xA0, 0x80],       // UTF-16 surrogate half (CESU-8)
            &[0xF4, 0x90, 0x80, 0x80], // above U+10FFFF
            &[0xC0, 0x80],             // overlong NUL
            &[b'o', b'k', 0xFE, b'!'], // valid prefix, invalid tail
        ];
        for bytes in invalid {
            let r = response_with_body(bytes.to_vec());
            assert!(
                r.body_as_string().is_none(),
                "expected None for {bytes:02X?}"
            );
        }
    }

    #[test]
    fn body_as_string_is_pure_and_repeatable() {
        let r = response_with_body(b"payload".to_vec());
        let first = r.body_as_string();
        let second = r.body_as_string();
        assert_eq!(first, second);
        // The getter must not consume or mutate the body.
        assert_eq!(r.body.as_slice(), &b"payload"[..]);
    }

    #[test]
    fn body_as_string_ignores_a_lying_content_length() {
        // content_length is untrusted server metadata and is not an invariant of
        // `body`; the decoder must go by the actual byte slice.
        let mut r = response_with_body(b"1234".to_vec());
        r.content_length = u64::MAX;
        assert_eq!(r.body_as_string().expect("valid").as_str(), "1234");

        r.content_length = 0;
        assert_eq!(r.body_as_string().expect("valid").as_str(), "1234");
    }

    #[test]
    fn body_as_string_handles_a_large_body() {
        let big = huge_ascii();
        let r = response_with_body(big.clone().into_bytes());
        let s = r.body_as_string().expect("ASCII is valid UTF-8");
        assert_eq!(s.as_str().len(), big.len());
    }

    // =========================================================================
    // FFI result round-trips (encode == decode)
    // =========================================================================

    #[test]
    fn result_http_response_round_trips_through_the_ffi_enum() {
        let ok: Result<HttpResponse, HttpError> = Ok(response_with_status(200));
        let ffi: ResultHttpResponseHttpError = ok.clone().into();
        assert!(ffi.is_ok());
        assert!(!ffi.is_err());
        assert_eq!(ffi.into_result(), ok);

        let err: Result<HttpResponse, HttpError> =
            Err(HttpError::http_status(u16::MAX, AzString::from(NASTY)));
        let ffi: ResultHttpResponseHttpError = err.clone().into();
        assert!(ffi.is_err());
        assert!(!ffi.is_ok());
        assert_eq!(ffi.into_result(), err);
    }

    #[test]
    fn result_u8vec_round_trips_through_the_ffi_enum() {
        let ok: Result<U8Vec, HttpError> = Ok(U8Vec::from(vec![0u8, 0xFF, 0x7F]));
        let ffi: ResultU8VecHttpError = ok.clone().into();
        assert!(ffi.is_ok());
        assert_eq!(ffi.into_result(), ok);

        let err: Result<U8Vec, HttpError> = Err(HttpError::response_too_large(0, u64::MAX));
        let ffi: ResultU8VecHttpError = err.clone().into();
        assert!(ffi.is_err());
        assert_eq!(ffi.into_result(), err);

        // An empty Ok payload must stay Ok — not collapse into Err.
        let empty: ResultU8VecHttpError = Ok(U8Vec::from(Vec::new())).into();
        assert!(empty.is_ok());
        assert_eq!(empty.as_result().map(|v| v.len()), Ok(0));
    }

    #[test]
    fn ffi_result_as_result_agrees_with_is_ok() {
        let ffi: ResultHttpResponseHttpError = Ok(response_with_status(404)).into();
        assert_eq!(ffi.is_ok(), ffi.as_result().is_ok());
        assert_eq!(ffi.as_result().map(HttpResponse::is_client_error), Ok(true));
    }

    // =========================================================================
    // `http` feature DISABLED — the stubs must fail closed
    // =========================================================================

    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[test]
    fn stub_free_functions_return_err_for_any_url() {
        for url in ["", "https://example.com", NASTY, huge_ascii().as_str()] {
            let cfg = HttpRequestConfig::new();
            assert!(matches!(http_get(url), Err(HttpError::Other(_))));
            assert!(matches!(
                http_get_with_config(url, &cfg),
                Err(HttpError::Other(_))
            ));
            assert!(matches!(download_bytes(url), Err(HttpError::Other(_))));
            assert!(matches!(
                download_bytes_with_config(url, &cfg),
                Err(HttpError::Other(_))
            ));
        }
    }

    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[test]
    fn stub_is_url_reachable_is_always_false() {
        // Fails closed: a disabled HTTP stack must never claim a URL is reachable.
        for url in ["", "https://example.com", NASTY, huge_ascii().as_str()] {
            assert!(!is_url_reachable(url));
            let (reachable, error) =
                HttpRequestConfig::new().is_url_reachable_blocking(AzString::from(url));
            assert!(!reachable);
            assert!(error.is_some(), "the stub must say why it answered false");
        }
    }

    #[cfg(any(not(feature = "http"), target_arch = "wasm32"))]
    #[test]
    fn stub_config_methods_return_err_results() {
        let cfg = HttpRequestConfig::new()
            .with_timeout(u64::MAX)
            .with_max_size(0);
        for url in ["", "https://example.com", NASTY] {
            let u = AzString::from(url);
            assert!(HttpRequestConfig::new()
                .http_get_blocking(u.clone())
                .is_err());
            assert!(cfg.http_get_blocking(u.clone()).is_err());
            assert!(HttpRequestConfig::new()
                .download_bytes_blocking(u.clone())
                .is_err());
            assert!(cfg.download_bytes_blocking(u.clone()).is_err());
        }
    }

    // =========================================================================
    // `http` feature ENABLED — offline-only checks
    // =========================================================================

    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[test]
    fn make_agent_builds_at_timeout_extremes() {
        // Duration::from_secs(u64::MAX) is representable, so agent construction must
        // not panic at either end of the range (the agent is never called here).
        for secs in [0_u64, 1, 30, u64::MAX] {
            for disable_tls in [false, true] {
                let _agent = make_agent(secs, disable_tls);
            }
        }
    }

    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[test]
    fn malformed_urls_are_rejected_without_touching_the_network() {
        // Each of these fails in ureq's URI parser: no DNS resolution, no socket.
        let cfg = HttpRequestConfig::new().with_timeout(1);
        for url in ["", "not a url", "://no-scheme", "ht tp://spaces"] {
            assert!(http_get(url).is_err(), "expected Err for {url:?}");
            assert!(
                http_get_with_config(url, &cfg).is_err(),
                "expected Err for {url:?}"
            );
            assert!(download_bytes(url).is_err(), "expected Err for {url:?}");
            assert!(!is_url_reachable(url), "expected false for {url:?}");
        }
    }

    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    #[test]
    fn malformed_urls_are_rejected_through_the_ffi_wrappers() {
        let cfg = HttpRequestConfig::new().with_timeout(1);
        for url in ["", "not a url"] {
            let u = AzString::from(url);
            assert!(HttpRequestConfig::new()
                .http_get_blocking(u.clone())
                .is_err());
            assert!(cfg.http_get_blocking(u.clone()).is_err());
            assert!(HttpRequestConfig::new()
                .download_bytes_blocking(u.clone())
                .is_err());
            assert!(cfg.download_bytes_blocking(u.clone()).is_err());
            let (reachable, error) = cfg.is_url_reachable_blocking(u.clone());
            assert!(!reachable);
            assert!(error.is_some());
        }
    }

    // The resumable form parks the answer in the runtime queue instead of
    // returning it: the request id is valid and one completion carrying the
    // typed result struct reaches the queue - on a later pump, once the
    // worker thread has answered (`resume_without_blocking`).
    #[cfg(all(feature = "http", not(target_arch = "wasm32"), feature = "text_layout"))]
    #[test]
    fn resumable_requests_park_their_result_in_the_queue() {
        use azul_core::refany::RefAny;

        extern "C" fn noop(
            _: RefAny,
            _: crate::callbacks::CallbackInfo,
            _: RefAny,
        ) -> azul_core::callbacks::Update {
            azul_core::callbacks::Update::DoNothing
        }

        let cfg = HttpRequestConfig::new().with_timeout(1);
        let id = cfg.http_get(
            AzString::from("not a url"),
            RefAny::new(()),
            crate::callbacks::ResumeCallback::create(noop),
        );
        assert!(id.is_valid());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let entry = loop {
            let mut completed = crate::request::take_completed();
            if let Some(at) = completed.iter().position(|e| e.request_id == id) {
                break completed.remove(at);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the answer never reached the queue"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        let answer = HttpGetResult::downcast(entry.result)
            .into_option()
            .expect("an HttpGetResult");
        assert!(answer.result.is_err());
    }
}

#[cfg(all(test, feature = "http", not(target_arch = "wasm32")))]
mod client_pool_tests {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    use super::*;

    /// A keep-alive HTTP/1.1 server on localhost that counts the connections it
    /// accepts. It serves one connection at a time until the client closes it.
    fn serve() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/", listener.local_addr().expect("addr"));
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                while read_request_head(&mut reader).is_some() {
                    let reply = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: text/plain\r\n\r\nok";
                    if stream.write_all(reply).is_err() {
                        break;
                    }
                }
            }
        });
        (url, accepted)
    }

    /// Consume one request head and return its request line (`GET /path
    /// HTTP/1.1`); `None` once the client has closed the connection.
    fn read_request_head(reader: &mut impl BufRead) -> Option<String> {
        let mut request_line = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) if line == "\r\n" => return Some(request_line),
                Ok(_) if request_line.is_empty() => request_line = line.trim_end().to_string(),
                Ok(_) => {}
            }
        }
    }

    fn get_three_times(url: &str, config: &HttpRequestConfig) {
        for _ in 0..3 {
            let response = http_get_with_config(url, config).expect("GET");
            assert_eq!(response.status_code, 200);
            assert_eq!(response.body.as_ref(), b"ok");
        }
    }

    #[test]
    fn requests_through_a_client_reuse_one_connection() {
        let (url, accepted) = serve();
        let client = HttpClient::create(HttpClientConfig::default());
        get_three_times(
            &url,
            &HttpRequestConfig::default()
                .with_timeout(5)
                .with_client(client),
        );
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
    }

    /// Answers every host with 127.0.0.1:`port`, counting how often it is asked.
    #[derive(Debug)]
    struct CountingResolver {
        port: u16,
        calls: Arc<AtomicUsize>,
    }

    impl ureq::unversioned::resolver::Resolver for CountingResolver {
        fn resolve(
            &self,
            _uri: &ureq::http::Uri,
            _config: &ureq::config::Config,
            _timeout: ureq::unversioned::transport::NextTimeout,
        ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut out = self.empty();
            out.push(std::net::SocketAddr::from(([127, 0, 0, 1], self.port)));
            Ok(out)
        }
    }

    /// A client for a host name only `CountingResolver` knows, so every lookup
    /// the client makes is counted.
    fn counted_client(
        agent: impl FnOnce(CountingResolver) -> ureq::Agent,
    ) -> (String, Arc<AtomicUsize>) {
        let (url, _) = serve();
        let port: u16 = url
            .trim_end_matches('/')
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .expect("port");
        let calls = Arc::new(AtomicUsize::new(0));
        let client = HttpClient {
            ptr: Box::new(Arc::new(HttpClientInner {
                config: HttpClientConfig::default(),
                fallback: FallbackAddresses::default(),
                agent: agent(CountingResolver {
                    port,
                    calls: Arc::clone(&calls),
                }),
            })),
            run_destructor: true,
        };
        let config = HttpRequestConfig::default()
            .with_timeout(5)
            .with_client(client);
        get_three_times(&format!("http://tiles.azul.invalid:{port}/"), &config);
        (url, calls)
    }

    #[test]
    fn a_client_without_a_dns_cache_looks_the_host_up_for_every_request() {
        let config = HttpClientConfig::default();
        let (_, calls) = counted_client(|resolver| {
            client_agent(&config, resolver, FallbackAddresses::default())
        });
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn a_client_with_a_dns_cache_looks_the_host_up_once() {
        let config = HttpClientConfig::default().with_dns_cache_secs(60);
        let (_, calls) = counted_client(|resolver| {
            client_agent(&config, resolver, FallbackAddresses::default())
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_expired_dns_answer_is_looked_up_again() {
        let (_, calls) = counted_client(|resolver| {
            ureq::Agent::with_parts(
                agent_config(false).build(),
                ureq::unversioned::transport::DefaultConnector::default(),
                CachingResolver::new(resolver, std::time::Duration::ZERO),
            )
        });
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn requests_without_a_client_each_open_a_connection() {
        let (url, accepted) = serve();
        get_three_times(&url, &HttpRequestConfig::default().with_timeout(5));
        assert_eq!(accepted.load(Ordering::SeqCst), 3);
    }

    /// A resolver that knows no host: every lookup fails, as when DNS is down.
    #[derive(Debug)]
    struct NoDns;

    impl ureq::unversioned::resolver::Resolver for NoDns {
        fn resolve(
            &self,
            _uri: &ureq::http::Uri,
            _config: &ureq::config::Config,
            _timeout: ureq::unversioned::transport::NextTimeout,
        ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
            Err(ureq::Error::HostNotFound)
        }
    }

    /// A keep-alive HTTP/1.1 server on localhost answering `ok`, each connection on a thread of
    /// its own (a pooled client keeps one connection per host and port open: a server serving
    /// one connection at a time would never answer the next); the `Host` headers it was sent.
    fn serve_hosts() -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let hosts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&hosts);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let seen = Arc::clone(&seen);
                std::thread::spawn(move || answer_hosts(stream, &seen));
            }
        });
        (port, hosts)
    }

    /// Answers `ok` to every request of one connection, noting its `Host` header.
    fn answer_hosts(mut stream: std::net::TcpStream, seen: &std::sync::Mutex<Vec<String>>) {
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        loop {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) if line == "\r\n" => break,
                    Ok(_) => {
                        if let Some(host) = line
                            .strip_prefix("Host: ")
                            .or_else(|| line.strip_prefix("host: "))
                        {
                            seen.lock().unwrap().push(host.trim().to_string());
                        }
                    }
                }
            }
            let reply = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
            if stream.write_all(reply).is_err() {
                return;
            }
        }
    }

    #[test]
    fn a_host_whose_name_does_not_resolve_is_reached_at_its_fallback_address_under_its_name() {
        let (port, hosts) = serve_hosts();
        let config = HttpClientConfig::default();
        let fallback = FallbackAddresses::default();
        let client = HttpClient {
            ptr: Box::new(Arc::new(HttpClientInner {
                config,
                agent: client_agent(&config, NoDns, fallback.clone()),
                fallback,
            })),
            run_destructor: true,
        };
        let request = HttpRequestConfig::default()
            .with_timeout(5)
            .with_client(client.clone());
        let url = format!("http://n2.azul.invalid:{port}/");
        assert!(
            http_get_with_config(&url, &request).is_err(),
            "no address is known yet"
        );
        assert!(!client.add_fallback_address("n2.azul.invalid", "not an address"));
        assert!(client.add_fallback_address("N2.azul.invalid", "127.0.0.1"));
        let response = http_get_with_config(&url, &request).expect("reached at its address");
        assert_eq!(response.status_code, 200);
        assert_eq!(
            hosts.lock().unwrap().last().cloned(),
            Some(format!("n2.azul.invalid:{port}")),
            "the request still names the host: TLS verifies that name, not the address"
        );
        client.clear_fallback_addresses("n2.azul.invalid");
        assert!(http_get_with_config(&url, &request).is_err());
        assert!(
            client.add_fallback_address("n2.azul.invalid", &format!("127.0.0.1:{port}")),
            "an address may name its port"
        );
        let named = http_get_with_config("http://n2.azul.invalid:1/", &request);
        assert!(named.is_ok(), "{named:?}");
        assert_eq!(
            hosts.lock().unwrap().last().cloned(),
            Some(String::from("n2.azul.invalid:1")),
            "the address's port is where it connects; the request names the URL's"
        );
    }

    /// A resolver that answers 127.0.0.1:`port` for its first `answers` lookups and fails every
    /// later one the way the system's does when DNS is down (an I/O error of the lookup).
    #[derive(Debug)]
    struct DnsGoesDown {
        port: u16,
        answers: usize,
        calls: Arc<AtomicUsize>,
    }

    impl ureq::unversioned::resolver::Resolver for DnsGoesDown {
        fn resolve(
            &self,
            _uri: &ureq::http::Uri,
            _config: &ureq::config::Config,
            _timeout: ureq::unversioned::transport::NextTimeout,
        ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
            let before = self.calls.fetch_add(1, Ordering::SeqCst);
            if before >= self.answers {
                return Err(ureq::Error::Io(std::io::Error::other(
                    "failed to lookup address information: nodename nor servname provided",
                )));
            }
            let mut out = self.empty();
            out.push(std::net::SocketAddr::from(([127, 0, 0, 1], self.port)));
            Ok(out)
        }
    }

    /// A pooled client whose lookups go through `resolver` (and its fallback addresses).
    fn client_over(
        config: HttpClientConfig,
        agent: impl FnOnce(FallbackAddresses) -> ureq::Agent,
    ) -> HttpRequestConfig {
        let fallback = FallbackAddresses::default();
        let client = HttpClient {
            ptr: Box::new(Arc::new(HttpClientInner {
                config,
                agent: agent(fallback.clone()),
                fallback,
            })),
            run_destructor: true,
        };
        HttpRequestConfig::default()
            .with_timeout(5)
            .with_client(client)
    }

    #[test]
    fn a_lookup_that_times_out_fails_as_a_dns_failure_and_not_as_a_slow_server() {
        let url = "https://n2.azul.invalid/d-1/a.txt";
        assert!(
            matches!(
                map_ureq_error(url, &ureq::Error::Timeout(ureq::Timeout::Resolve)),
                HttpError::DnsFailed(_)
            ),
            "the resolver gave up: the name is the problem, its addresses may still answer"
        );
        assert!(matches!(
            map_ureq_error(url, &ureq::Error::HostNotFound),
            HttpError::DnsFailed(_)
        ));
        assert_eq!(
            map_ureq_error(url, &ureq::Error::Timeout(ureq::Timeout::RecvResponse)),
            HttpError::Timeout,
            "a server that is slow to answer stays a timeout"
        );
        assert!(HttpError::dns_failed("n2.azul.invalid".into())
            .to_string()
            .contains("n2.azul.invalid"));
    }

    #[test]
    fn every_lookup_gives_up_after_the_dns_timeout_so_the_fallback_addresses_have_time_left() {
        assert_eq!(
            agent_config(false).build().timeouts().resolve,
            Some(DNS_LOOKUP_TIMEOUT),
            "a lookup that hangs must not use up the request's whole time"
        );
    }

    #[test]
    fn a_name_that_does_not_resolve_fails_as_a_dns_failure() {
        let calls = Arc::new(AtomicUsize::new(0));
        let config = HttpClientConfig::default();
        let resolver = DnsGoesDown {
            port: 1,
            answers: 0,
            calls: Arc::clone(&calls),
        };
        let request = client_over(config, |fallback| client_agent(&config, resolver, fallback));
        match http_get_with_config("http://n2.azul.invalid:1/", &request) {
            Err(HttpError::DnsFailed(why)) => {
                assert!(why.as_str().contains("n2.azul.invalid"), "{}", why.as_str())
            }
            other => panic!("a lookup error is a DNS failure, not {other:?}"),
        }
    }

    #[test]
    fn a_cached_answer_is_served_for_seven_days_after_the_name_stops_resolving() {
        let (url, _) = serve();
        let port: u16 = url
            .trim_end_matches('/')
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .expect("port");
        let calls = Arc::new(AtomicUsize::new(0));
        let config = HttpClientConfig::default().with_dns_cache_secs(60);
        let resolver = DnsGoesDown {
            port,
            answers: 1,
            calls: Arc::clone(&calls),
        };
        let request = client_over(config, |fallback| {
            ureq::Agent::with_parts(
                agent_config(false).build(),
                ureq::unversioned::transport::DefaultConnector::default(),
                FallbackResolver {
                    // Every answer is due again at once: only the stale one is left.
                    inner: CachingResolver::new(resolver, std::time::Duration::ZERO),
                    fallback,
                },
            )
        });
        let url = format!("http://tiles.azul.invalid:{port}/");
        for round in 0..3 {
            let response = http_get_with_config(&url, &request)
                .unwrap_or_else(|e| panic!("round {round}: {e}"));
            assert_eq!(response.status_code, 200);
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "after the lookup failed, the next requests take the stale answer without waiting \
             for another lookup"
        );
        assert_eq!(DNS_STALE_FOR, std::time::Duration::from_secs(7 * 24 * 3600));
    }

    #[test]
    fn an_answer_older_than_its_stale_limit_is_not_served() {
        let (url, _) = serve();
        let port: u16 = url
            .trim_end_matches('/')
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .expect("port");
        let calls = Arc::new(AtomicUsize::new(0));
        let config = HttpClientConfig::default();
        let resolver = DnsGoesDown {
            port,
            answers: 1,
            calls: Arc::clone(&calls),
        };
        let request = client_over(config, |fallback| {
            ureq::Agent::with_parts(
                agent_config(false).build(),
                ureq::unversioned::transport::DefaultConnector::default(),
                FallbackResolver {
                    inner: CachingResolver::new(resolver, std::time::Duration::ZERO)
                        .with_stale_for(std::time::Duration::ZERO),
                    fallback,
                },
            )
        });
        let url = format!("http://tiles.azul.invalid:{port}/");
        assert!(http_get_with_config(&url, &request).is_ok());
        assert!(matches!(
            http_get_with_config(&url, &request),
            Err(HttpError::DnsFailed(_))
        ));
    }

    /// MAIL9: the resumable `http_get` ran the transfer inside the calling
    /// callback, so a slow server froze the window for the whole request. It
    /// returns at once; the answer resumes the callback on a later pump.
    #[cfg(feature = "text_layout")]
    #[test]
    fn http_get_from_a_callback_returns_before_the_response_arrives() {
        use std::{
            sync::mpsc,
            time::{Duration, Instant},
        };

        use azul_core::refany::RefAny;

        extern "C" fn noop(
            _: RefAny,
            _: crate::callbacks::CallbackInfo,
            _: RefAny,
        ) -> azul_core::callbacks::Update {
            azul_core::callbacks::Update::DoNothing
        }

        // A server that answers once the test says so - or after 3 s, so a
        // blocking `http_get` fails the test instead of hanging it.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/", listener.local_addr().expect("addr"));
        let (release, released) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            if read_request_head(&mut reader).is_none() {
                return;
            }
            let _ = released.recv_timeout(Duration::from_secs(3));
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            );
        });

        let started = Instant::now();
        let id = HttpRequestConfig::default().with_timeout(10).http_get(
            AzString::from(url),
            RefAny::new(()),
            crate::callbacks::ResumeCallback::create(noop),
        );
        let returned_after = started.elapsed();
        let _ = release.send(());
        assert!(
            returned_after < Duration::from_secs(1),
            "http_get held the calling callback for {returned_after:?}"
        );

        // The answer resumes on a later pump: this thread's pump (under tests
        // the request queue is per thread, so no test beside this one can
        // drain the entry).
        let deadline = Instant::now() + Duration::from_secs(10);
        let answer = loop {
            if let Some(entry) = crate::request::take_completed()
                .into_iter()
                .find(|e| e.request_id == id)
            {
                break HttpGetResult::downcast(entry.result)
                    .into_option()
                    .expect("an HttpGetResult");
            }
            assert!(Instant::now() < deadline, "the answer never arrived");
            std::thread::sleep(Duration::from_millis(1));
        };
        // An armed e2e mock store (another test's scenario) answers instead.
        if !crate::request::mock::is_armed() {
            match answer.result {
                ResultHttpResponseHttpError::Ok(response) => {
                    assert_eq!(response.status_code, 200);
                    assert_eq!(response.body.as_ref(), b"ok");
                }
                ResultHttpResponseHttpError::Err(e) => panic!("{e:?}"),
            }
        }
    }

    /// NEWS9: a feed that moved answers with a redirect; the reader follows it
    /// and must learn where it ended - to resolve the feed's relative links
    /// and to update the subscription.
    #[test]
    fn a_followed_redirect_reports_the_final_url() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                while let Some(request_line) = read_request_head(&mut reader) {
                    let reply: &[u8] = if request_line.starts_with("GET /old ") {
                        b"HTTP/1.1 301 Moved Permanently\r\nLocation: /new\r\nContent-Length: 0\r\n\r\n"
                    } else {
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: text/plain\r\n\r\nok"
                    };
                    if stream.write_all(reply).is_err() {
                        break;
                    }
                }
            }
        });

        let response = http_get_with_config(
            &format!("{base}/old"),
            &HttpRequestConfig::default().with_timeout(5),
        )
        .expect("GET");
        assert_eq!(response.status_code, 200);
        assert_eq!(response.body.as_ref(), b"ok");
        assert_eq!(response.final_url.as_str(), format!("{base}/new"));
    }
}
