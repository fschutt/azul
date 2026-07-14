//! Simple HTTP client module for downloading resources (language packs, etc.)
//!
//! Uses ureq for simple, blocking HTTP requests. Designed to be exposed via C API.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use core::fmt;

use azul_css::{AzString, U8Vec, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut, impl_option, impl_option_inner};

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
}

impl HttpError {
    #[must_use] pub const fn invalid_url(url: AzString) -> Self {
        Self::InvalidUrl(url)
    }
    
    #[must_use] pub const fn connection_failed(msg: AzString) -> Self {
        Self::ConnectionFailed(msg)
    }
    
    #[must_use] pub const fn tls_error(msg: AzString) -> Self {
        Self::TlsError(msg)
    }
    
    #[must_use] pub const fn http_status(status_code: u16, message: AzString) -> Self {
        Self::HttpStatus(HttpStatusError {
            status_code,
            message,
        })
    }
    
    #[must_use] pub const fn io_error(msg: AzString) -> Self {
        Self::IoError(msg)
    }
    
    #[must_use] pub const fn response_too_large(max_size: u64, actual_size: u64) -> Self {
        Self::ResponseTooLarge(HttpResponseTooLargeError {
            max_size,
            actual_size,
        })
    }
    
    #[must_use] pub const fn other(msg: AzString) -> Self {
        Self::Other(msg)
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
                write!(f, "Response too large: {} bytes (max: {})", e.actual_size, e.max_size)
            }
            Self::Other(msg) => write!(f, "HTTP error: {}", msg.as_str()),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for HttpError {}

/// Result type for HTTP operations
pub type HttpResult<T> = Result<T, HttpError>;

// FFI-safe Result types for HTTP operations
use azul_css::{impl_result, impl_result_inner};

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

impl_option!(HttpHeader, OptionHttpHeader, copy = false, [Debug, Clone, PartialEq, Eq]);
impl_vec!(HttpHeader, HttpHeaderVec, HttpHeaderVecDestructor, HttpHeaderVecDestructorType, HttpHeaderVecSlice, OptionHttpHeader);
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
}

impl Default for HttpRequestConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_response_size: 100 * 1024 * 1024, // 100 MB
            user_agent: AzString::from("azul-http/1.0".to_string()),
            headers: HttpHeaderVec::from_const_slice(&[]),
            disable_tls_cert_verification: false,
        }
    }
}

impl HttpRequestConfig {
    /// Create a new config with default values
    #[must_use] pub fn new() -> Self {
        Self::default()
    }
    
    /// Set timeout in seconds
    #[must_use] pub const fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
    
    /// Set maximum response size (0 = unlimited)
    #[must_use] pub const fn with_max_size(mut self, max_bytes: u64) -> Self {
        self.max_response_size = max_bytes;
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

    /// Simple HTTP GET request with default configuration
    ///
    /// # Arguments
    /// * `url` - The URL to request
    ///
    /// # Returns
    /// * `ResultHttpResponseHttpError` - The response or an error
    #[cfg(feature = "http")]
    pub fn http_get_default(url: AzString) -> ResultHttpResponseHttpError {
        let config = HttpRequestConfig::default();
        http_get_with_config(url.as_str(), &config).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(not(feature = "http"))]
    #[must_use] pub fn http_get_default(_url: AzString) -> ResultHttpResponseHttpError {
        ResultHttpResponseHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// HTTP GET request using this configuration
    /// 
    /// # Arguments
    /// * `url` - The URL to request
    /// 
    /// # Returns
    /// * `ResultHttpResponseHttpError` - The response or an error
    #[cfg(feature = "http")]
    pub fn http_get(&self, url: AzString) -> ResultHttpResponseHttpError {
        http_get_with_config(url.as_str(), self).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(not(feature = "http"))]
    #[must_use] pub fn http_get(&self, _url: AzString) -> ResultHttpResponseHttpError {
        ResultHttpResponseHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// Download URL to bytes with default configuration
    /// 
    /// # Arguments
    /// * `url` - The URL to download
    /// 
    /// # Returns
    /// * `ResultU8VecHttpError` - The response body or an error
    #[cfg(feature = "http")]
    pub fn download_bytes_default(url: AzString) -> ResultU8VecHttpError {
        download_bytes(url.as_str()).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(not(feature = "http"))]
    #[must_use] pub fn download_bytes_default(_url: AzString) -> ResultU8VecHttpError {
        ResultU8VecHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// Download URL to bytes using this configuration
    /// 
    /// # Arguments
    /// * `url` - The URL to download
    /// 
    /// # Returns
    /// * `ResultU8VecHttpError` - The response body or an error
    #[cfg(feature = "http")]
    pub fn download_bytes(&self, url: AzString) -> ResultU8VecHttpError {
        download_bytes_with_config(url.as_str(), self).into()
    }

    /// Stub: `http` feature disabled.
    #[cfg(not(feature = "http"))]
    #[must_use] pub fn download_bytes(&self, _url: AzString) -> ResultU8VecHttpError {
        ResultU8VecHttpError::Err(HttpError::other("http feature not enabled".into()))
    }

    /// Check if a URL is reachable (HEAD request)
    /// 
    /// # Arguments
    /// * `url` - The URL to check
    /// 
    /// # Returns
    /// * `bool` - True if reachable (2xx status)
    #[cfg(feature = "http")]
    pub fn is_url_reachable(url: AzString) -> bool {
        is_url_reachable(url.as_str())
    }

    /// Stub: `http` feature disabled.
    #[cfg(not(feature = "http"))]
    #[must_use] pub fn is_url_reachable(_url: AzString) -> bool {
        false
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
}

impl HttpResponse {
    /// Check if the response was successful (2xx status)
    #[must_use] pub const fn is_success(&self) -> bool {
        self.status_code >= 200 && self.status_code < 300
    }
    
    /// Check if the response is a redirect (3xx status)
    #[must_use] pub const fn is_redirect(&self) -> bool {
        self.status_code >= 300 && self.status_code < 400
    }
    
    /// Check if the response is a client error (4xx status)
    #[must_use] pub const fn is_client_error(&self) -> bool {
        self.status_code >= 400 && self.status_code < 500
    }
    
    /// Check if the response is a server error (5xx status)
    #[must_use] pub const fn is_server_error(&self) -> bool {
        self.status_code >= 500 && self.status_code < 600
    }
    
    /// Try to convert the body to a UTF-8 string
    #[must_use] pub fn body_as_string(&self) -> Option<AzString> {
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

/// Simple HTTP GET request
///
/// # Arguments
/// * `url` - The URL to request
///
/// # Returns
/// * `HttpResult<HttpResponse>` - The response or an error
#[cfg(feature = "http")]
pub fn http_get(url: &str) -> HttpResult<HttpResponse> {
    http_get_with_config(url, &HttpRequestConfig::default())
}

/// Stub: `http` feature disabled.
#[cfg(not(feature = "http"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is disabled).
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
#[cfg(feature = "http")]
fn make_agent(timeout_secs: u64, disable_tls_cert_verification: bool) -> ureq::Agent {
    use std::time::Duration;

    let mut tls_builder = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .unversioned_rustls_crypto_provider(
            std::sync::Arc::new(rustls_rustcrypto::provider())
        );

    if disable_tls_cert_verification {
        tls_builder = tls_builder.disable_verification(true);
    } else {
        tls_builder = tls_builder.root_certs(ureq::tls::RootCerts::WebPki);
    }

    let tls_config = tls_builder.build();

    ureq::Agent::config_builder()
        .tls_config(tls_config)
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

#[cfg(feature = "http")]
pub fn http_get_with_config(url: &str, config: &HttpRequestConfig) -> HttpResult<HttpResponse> {
    use std::io::Read;

    let agent = make_agent(config.timeout_secs, config.disable_tls_cert_verification);

    // Build the request
    let mut request = agent.get(url);

    // Add user agent
    if !config.user_agent.as_str().is_empty() {
        request = request.header("User-Agent", config.user_agent.as_str());
    }

    // Add custom headers
    for header in config.headers.as_slice() {
        request = request.header(header.name.as_str(), header.value.as_str());
    }

    // Execute request — map transport errors to specific HttpError variants
    let response = request.call().map_err(|e| {
        match &e {
            ureq::Error::Timeout(_) => HttpError::Timeout,
            ureq::Error::HostNotFound => HttpError::connection_failed(
                format!("DNS resolution failed for {}", url).into(),
            ),
            ureq::Error::ConnectionFailed => HttpError::connection_failed(
                format!("Connection failed: {}", url).into(),
            ),
            ureq::Error::Io(io_err) => HttpError::io_error(
                format!("{}", io_err).into(),
            ),
            ureq::Error::BadUri(msg) => HttpError::invalid_url(
                format!("{}: {}", url, msg).into(),
            ),
            ureq::Error::Tls(msg) => HttpError::tls_error(
                format!("TLS error: {}", msg).into(),
            ),
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
    })?;

    let status_code = response.status().as_u16();
    let content_type = AzString::from(
        response.headers().get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string()
    );
    let content_length = response.headers().get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Collect response headers
    let mut headers = Vec::new();
    for (name, value) in response.headers().iter() {
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
    reader.read_to_end(&mut body).map_err(|e| HttpError::io_error(e.to_string().into()))?;

    Ok(HttpResponse {
        status_code,
        body: U8Vec::from(body),
        content_type,
        content_length,
        headers: HttpHeaderVec::from_vec(headers),
    })
}

/// Stub: `http` feature disabled.
#[cfg(not(feature = "http"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is disabled).
pub fn http_get_with_config(_url: &str, _config: &HttpRequestConfig) -> HttpResult<HttpResponse> {
    Err(HttpError::other("http feature not enabled".into()))
}

/// Download a URL to bytes (convenience wrapper with default config)
/// 
/// # Arguments
/// * `url` - The URL to download
/// 
/// # Returns
/// * `HttpResult<U8Vec>` - The response body or an error
#[cfg(feature = "http")]
pub fn download_bytes(url: &str) -> HttpResult<U8Vec> {
    download_bytes_with_config(url, &HttpRequestConfig::default())
}

/// Stub: `http` feature disabled.
#[cfg(not(feature = "http"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is disabled).
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
#[cfg(feature = "http")]
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
#[cfg(not(feature = "http"))]
/// # Errors
///
/// Returns an `HttpError` if the request fails (network/status error, or the networking feature is disabled).
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
#[cfg(feature = "http")]
pub fn is_url_reachable(url: &str) -> bool {
    const REACHABILITY_TIMEOUT_SECS: u64 = 10;
    let agent = make_agent(REACHABILITY_TIMEOUT_SECS, false);
    match agent.head(url).call() {
        Ok(resp) => {
            let code = resp.status().as_u16();
            code >= 200 && code < 300
        }
        Err(_) => false,
    }
}

/// Stub: `http` feature disabled.
#[cfg(not(feature = "http"))]
#[must_use] pub const fn is_url_reachable(_url: &str) -> bool {
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
        }
    }

    fn response_with_body(body: Vec<u8>) -> HttpResponse {
        HttpResponse {
            status_code: 200,
            body: U8Vec::from(body),
            content_type: AzString::from("text/plain"),
            content_length: 0,
            headers: HttpHeaderVec::from_const_slice(&[]),
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
        assert_ne!(HttpError::invalid_url(s.clone()), HttpError::other(s.clone()));
        assert_ne!(HttpError::tls_error(s.clone()), HttpError::io_error(s.clone()));
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
            format!(
                "Response too large: {} bytes (max: {})",
                u64::MAX,
                u64::MAX
            )
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
        assert_eq!(HttpRequestConfig::new().with_max_size(0).max_response_size, 0);
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
            &[0xFF],                    // never valid
            &[0x80],                    // lone continuation byte
            &[0xC3],                    // truncated 2-byte sequence
            &[0xE2, 0x82],              // truncated 3-byte sequence
            &[0xED, 0xA0, 0x80],        // UTF-16 surrogate half (CESU-8)
            &[0xF4, 0x90, 0x80, 0x80],  // above U+10FFFF
            &[0xC0, 0x80],              // overlong NUL
            &[b'o', b'k', 0xFE, b'!'],  // valid prefix, invalid tail
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
        assert_eq!(
            ffi.as_result().map(HttpResponse::is_client_error),
            Ok(true)
        );
    }

    // =========================================================================
    // `http` feature DISABLED — the stubs must fail closed
    // =========================================================================

    #[cfg(not(feature = "http"))]
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

    #[cfg(not(feature = "http"))]
    #[test]
    fn stub_is_url_reachable_is_always_false() {
        // Fails closed: a disabled HTTP stack must never claim a URL is reachable.
        for url in ["", "https://example.com", NASTY, huge_ascii().as_str()] {
            assert!(!is_url_reachable(url));
            assert!(!HttpRequestConfig::is_url_reachable(AzString::from(url)));
        }
    }

    #[cfg(not(feature = "http"))]
    #[test]
    fn stub_config_methods_return_err_results() {
        let cfg = HttpRequestConfig::new().with_timeout(u64::MAX).with_max_size(0);
        for url in ["", "https://example.com", NASTY] {
            let u = AzString::from(url);
            assert!(HttpRequestConfig::http_get_default(u.clone()).is_err());
            assert!(cfg.http_get(u.clone()).is_err());
            assert!(HttpRequestConfig::download_bytes_default(u.clone()).is_err());
            assert!(cfg.download_bytes(u.clone()).is_err());
        }
    }

    // =========================================================================
    // `http` feature ENABLED — offline-only checks
    // =========================================================================

    #[cfg(feature = "http")]
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

    #[cfg(feature = "http")]
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

    #[cfg(feature = "http")]
    #[test]
    fn malformed_urls_are_rejected_through_the_ffi_wrappers() {
        let cfg = HttpRequestConfig::new().with_timeout(1);
        for url in ["", "not a url"] {
            let u = AzString::from(url);
            assert!(HttpRequestConfig::http_get_default(u.clone()).is_err());
            assert!(cfg.http_get(u.clone()).is_err());
            assert!(HttpRequestConfig::download_bytes_default(u.clone()).is_err());
            assert!(cfg.download_bytes(u.clone()).is_err());
            assert!(!HttpRequestConfig::is_url_reachable(u.clone()));
        }
    }
}
