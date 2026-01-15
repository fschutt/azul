//! URL parsing module for C API
//!
//! Provides a C-compatible URL type based on the `url` crate.

use alloc::string::String;
use core::fmt;
use azul_css::{AzString, impl_result, impl_result_inner};

/// A parsed URL
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Url {
    /// The full URL string
    pub href: AzString,
    /// The scheme (e.g., "https")
    pub scheme: AzString,
    /// The host (e.g., "example.com")
    pub host: AzString,
    /// The port (0 if not specified)
    pub port: u16,
    /// The path (e.g., "/path/to/resource")
    pub path: AzString,
    /// The query string without '?' (e.g., "key=value")
    pub query: AzString,
    /// The fragment without '#' (e.g., "section")
    pub fragment: AzString,
}

/// Error when parsing a URL
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct UrlParseError {
    /// Error message
    pub message: AzString,
}

impl fmt::Display for UrlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message.as_str())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UrlParseError {}

// FFI-safe Result type for URL parsing
impl_result!(
    Url,
    UrlParseError,
    ResultUrlUrlParseError,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl Url {
    /// Parse a URL from a string
    /// 
    /// # Arguments
    /// * `s` - The URL string to parse
    /// 
    /// # Returns
    /// * `Result<Url, UrlParseError>` - The parsed URL or an error
    #[cfg(feature = "http")]
    pub fn parse(s: &str) -> Result<Self, UrlParseError> {
        use ::url::Url as UrlParser;
        
        let parsed = UrlParser::parse(s)
            .map_err(|e| UrlParseError {
                message: AzString::from(e.to_string()),
            })?;
        
        Ok(Self {
            href: AzString::from(parsed.as_str().to_string()),
            scheme: AzString::from(parsed.scheme().to_string()),
            host: AzString::from(parsed.host_str().unwrap_or("").to_string()),
            port: parsed.port().unwrap_or(0),
            path: AzString::from(parsed.path().to_string()),
            query: AzString::from(parsed.query().unwrap_or("").to_string()),
            fragment: AzString::from(parsed.fragment().unwrap_or("").to_string()),
        })
    }
    
    /// Create a URL from components
    pub fn from_parts(
        scheme: &str,
        host: &str,
        port: u16,
        path: &str,
    ) -> Self {
        let port_str = if port == 0 || (scheme == "http" && port == 80) || (scheme == "https" && port == 443) {
            String::new()
        } else {
            format!(":{}", port)
        };
        
        let href = format!("{}://{}{}{}", scheme, host, port_str, path);
        
        Self {
            href: AzString::from(href),
            scheme: AzString::from(scheme.to_string()),
            host: AzString::from(host.to_string()),
            port,
            path: AzString::from(path.to_string()),
            query: AzString::from(String::new()),
            fragment: AzString::from(String::new()),
        }
    }
    
    /// Get the full URL as a string slice
    pub fn as_str(&self) -> &str {
        self.href.as_str()
    }
    
    /// Check if this is an HTTPS URL
    pub fn is_https(&self) -> bool {
        self.scheme.as_str() == "https"
    }
    
    /// Check if this is an HTTP URL
    pub fn is_http(&self) -> bool {
        self.scheme.as_str() == "http"
    }
    
    /// Get the effective port (using default ports for http/https)
    pub fn effective_port(&self) -> u16 {
        if self.port != 0 {
            self.port
        } else if self.is_https() {
            443
        } else if self.is_http() {
            80
        } else {
            0
        }
    }
    
    /// Join a relative path to this URL
    #[cfg(feature = "http")]
    pub fn join(&self, path: &str) -> Result<Self, UrlParseError> {
        use ::url::Url as UrlParser;
        
        let base = UrlParser::parse(self.href.as_str())
            .map_err(|e| UrlParseError {
                message: AzString::from(e.to_string()),
            })?;
        
        let joined = base.join(path)
            .map_err(|e| UrlParseError {
                message: AzString::from(e.to_string()),
            })?;
        
        Self::parse(joined.as_str())
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.href.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    #[cfg(feature = "http")]
    fn test_url_parse() {
        let url = Url::parse("https://example.com:8080/path?query=1#frag").unwrap();
        assert_eq!(url.scheme.as_str(), "https");
        assert_eq!(url.host.as_str(), "example.com");
        assert_eq!(url.port, 8080);
        assert_eq!(url.path.as_str(), "/path");
        assert_eq!(url.query.as_str(), "query=1");
        assert_eq!(url.fragment.as_str(), "frag");
    }
    
    #[test]
    fn test_url_from_parts() {
        let url = Url::from_parts("https", "example.com", 443, "/api");
        assert!(url.is_https());
        assert_eq!(url.effective_port(), 443);
    }
}
