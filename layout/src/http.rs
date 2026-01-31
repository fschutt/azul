//! Simple HTTP client module for downloading resources (language packs, etc.)
//!
//! Uses ureq for simple, blocking HTTP requests. Designed to be exposed via C API.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use core::fmt;

use azul_css::{AzString, U8Vec, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut, impl_option, impl_option_inner};

#[cfg(feature = "std")]
use std::path::Path;

// ============================================================================
// Error types (C-compatible, single field per variant)
// ============================================================================

/// HTTP status error (4xx, 5xx responses)
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct HttpStatusError {
    /// HTTP status code
    pub status_code: u16,
    /// Status message
    pub message: AzString,
}

/// Response too large error
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct HttpResponseTooLargeError {
    /// Maximum allowed size in bytes
    pub max_size: u64,
    /// Actual size in bytes
    pub actual_size: u64,
}

/// HTTP error types (C-compatible)
#[derive(Debug, Clone, PartialEq)]
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
    pub fn invalid_url(url: AzString) -> Self {
        Self::InvalidUrl(url)
    }
    
    pub fn connection_failed(msg: AzString) -> Self {
        Self::ConnectionFailed(msg)
    }
    
    pub fn tls_error(msg: AzString) -> Self {
        Self::TlsError(msg)
    }
    
    pub fn http_status(status_code: u16, message: AzString) -> Self {
        Self::HttpStatus(HttpStatusError {
            status_code,
            message,
        })
    }
    
    pub fn io_error(msg: AzString) -> Self {
        Self::IoError(msg)
    }
    
    pub fn response_too_large(max_size: u64, actual_size: u64) -> Self {
        Self::ResponseTooLarge(HttpResponseTooLargeError {
            max_size,
            actual_size,
        })
    }
    
    pub fn other(msg: AzString) -> Self {
        Self::Other(msg)
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::InvalidUrl(url) => write!(f, "Invalid URL: {}", url.as_str()),
            HttpError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg.as_str()),
            HttpError::Timeout => write!(f, "Request timed out"),
            HttpError::TlsError(msg) => write!(f, "TLS error: {}", msg.as_str()),
            HttpError::HttpStatus(e) => write!(f, "HTTP {} - {}", e.status_code, e.message.as_str()),
            HttpError::IoError(msg) => write!(f, "I/O error: {}", msg.as_str()),
            HttpError::ResponseTooLarge(e) => {
                write!(f, "Response too large: {} bytes (max: {})", e.actual_size, e.max_size)
            }
            HttpError::Other(msg) => write!(f, "HTTP error: {}", msg.as_str()),
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
#[derive(Debug, Clone, PartialEq)]
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

impl_option!(HttpHeader, OptionHttpHeader, copy = false, [Debug, Clone, PartialEq]);
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
}

impl Default for HttpRequestConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_response_size: 100 * 1024 * 1024, // 100 MB
            user_agent: AzString::from("azul-http/1.0".to_string()),
            headers: HttpHeaderVec::from_const_slice(&[]),
        }
    }
}

impl HttpRequestConfig {
    /// Create a new config with default values
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set timeout in seconds
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
    
    /// Set maximum response size (0 = unlimited)
    pub fn with_max_size(mut self, max_bytes: u64) -> Self {
        self.max_response_size = max_bytes;
        self
    }
    
    /// Set User-Agent header
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = AzString::from(ua.into());
        self
    }
    
    /// Add a header
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
        http_get(url.as_str()).into()
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
    pub fn is_success(&self) -> bool {
        self.status_code >= 200 && self.status_code < 300
    }
    
    /// Check if the response is a redirect (3xx status)
    pub fn is_redirect(&self) -> bool {
        self.status_code >= 300 && self.status_code < 400
    }
    
    /// Check if the response is a client error (4xx status)
    pub fn is_client_error(&self) -> bool {
        self.status_code >= 400 && self.status_code < 500
    }
    
    /// Check if the response is a server error (5xx status)
    pub fn is_server_error(&self) -> bool {
        self.status_code >= 500 && self.status_code < 600
    }
    
    /// Try to convert the body to a UTF-8 string
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
    [Debug, Clone, PartialEq]
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

/// HTTP GET request with custom configuration
/// 
/// # Arguments
/// * `url` - The URL to request
/// * `config` - Request configuration
/// 
/// # Returns
/// * `HttpResult<HttpResponse>` - The response or an error
#[cfg(feature = "http")]
pub fn http_get_with_config(url: &str, config: &HttpRequestConfig) -> HttpResult<HttpResponse> {
    use std::io::Read;
    use std::time::Duration;
    
    // Build the request
    let mut request = ureq::get(url)
        .timeout(Duration::from_secs(config.timeout_secs));
    
    // Add user agent
    if !config.user_agent.as_str().is_empty() {
        request = request.set("User-Agent", config.user_agent.as_str());
    }
    
    // Add custom headers
    for header in config.headers.as_slice() {
        request = request.set(header.name.as_str(), header.value.as_str());
    }
    
    // Execute request
    let response = request.call().map_err(|e| match e {
        ureq::Error::Status(code, response) => {
            HttpError::http_status(code, response.status_text().to_string().into())
        }
        ureq::Error::Transport(transport) => {
            let kind = transport.kind();
            match kind {
                ureq::ErrorKind::Dns => HttpError::connection_failed(format!("DNS resolution failed: {}", transport).into()),
                ureq::ErrorKind::ConnectionFailed => HttpError::connection_failed(transport.to_string().into()),
                ureq::ErrorKind::Io => HttpError::io_error(transport.to_string().into()),
                ureq::ErrorKind::InvalidUrl => HttpError::invalid_url(url.to_string().into()),
                ureq::ErrorKind::TooManyRedirects => HttpError::other("Too many redirects".into()),
                _ => HttpError::other(transport.to_string().into()),
            }
        }
    })?;
    
    let status_code = response.status();
    let content_type = AzString::from(response.content_type().to_string());
    let content_length = response.header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    
    // Collect response headers
    let mut headers = Vec::new();
    for name in response.headers_names() {
        if let Some(value) = response.header(&name) {
            headers.push(HttpHeader::new(name, value));
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
        config.max_response_size
    } else {
        u64::MAX
    };
    let mut reader = response.into_reader().take(limit);
    reader.read_to_end(&mut body).map_err(|e| HttpError::io_error(e.to_string().into()))?;
    
    Ok(HttpResponse {
        status_code,
        body: U8Vec::from(body),
        content_type,
        content_length,
        headers: HttpHeaderVec::from_vec(headers),
    })
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

/// Check if a URL is reachable (HEAD request)
/// 
/// # Arguments
/// * `url` - The URL to check
/// 
/// # Returns
/// * `bool` - True if reachable (2xx status)
#[cfg(feature = "http")]
pub fn is_url_reachable(url: &str) -> bool {
    use std::time::Duration;
    
    let response = ureq::head(url)
        .timeout(Duration::from_secs(10))
        .call();
    
    match response {
        Ok(resp) => resp.status() >= 200 && resp.status() < 300,
        Err(ureq::Error::Status(code, _)) => code >= 200 && code < 300,
        Err(_) => false,
    }
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
            headers: Vec::new(),
        };
        assert!(response.is_success());
        assert!(!response.is_redirect());
        assert!(!response.is_client_error());
        assert!(!response.is_server_error());
    }
    
    #[test]
    fn test_http_error_constructors() {
        let err = HttpError::http_status(404, "Not Found");
        assert!(err.to_string().contains("404"));
        
        let err2 = HttpError::response_too_large(100, 200);
        assert!(err2.to_string().contains("200"));
    }
}
