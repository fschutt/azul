//! How a request leaves the process: the one seam between the S3 client (which
//! builds and signs its requests, and reads the answers) and the network.
//!
//! The apps send through `AzulTransport` (azul's `HttpRequestConfig`, feature
//! `azul`); the tests through a fake that records the calls.

use std::fmt;

/// The HTTP verbs the drives use: the S3 client's five, PATCH (the REST APIs of the consumer
/// clouds OpenDAL reaches) and WebDAV's (OpenDAL's WebDAV service).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Head,
    Put,
    Post,
    Delete,
    Patch,
    Options,
    /// WebDAV: a folder's listing, an item's properties.
    Propfind,
    /// WebDAV: set an item's properties.
    Proppatch,
    /// WebDAV: a new folder.
    Mkcol,
    /// WebDAV: a server-side copy.
    Copy,
    /// WebDAV: a server-side move.
    Move,
}

impl Method {
    /// Every verb, for [`Method::parse`].
    pub const ALL: [Method; 12] = [
        Method::Get,
        Method::Head,
        Method::Put,
        Method::Post,
        Method::Delete,
        Method::Patch,
        Method::Options,
        Method::Propfind,
        Method::Proppatch,
        Method::Mkcol,
        Method::Copy,
        Method::Move,
    ];

    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Put => "PUT",
            Method::Post => "POST",
            Method::Delete => "DELETE",
            Method::Patch => "PATCH",
            Method::Options => "OPTIONS",
            Method::Propfind => "PROPFIND",
            Method::Proppatch => "PROPPATCH",
            Method::Mkcol => "MKCOL",
            Method::Copy => "COPY",
            Method::Move => "MOVE",
        }
    }

    /// The verb of a wire name (any case); `None` for one the drives never send.
    #[must_use]
    pub fn parse(name: &str) -> Option<Method> {
        Method::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(name.trim()))
    }
}

/// One request, ready to send: the URL is final (encoded, query sorted) and the
/// headers carry the signature. `Host` is not among them: the HTTP client sets it
/// from the URL, and it was signed as such.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpCall {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The `Content-Type` of the body (not signed); empty without a body.
    pub content_type: String,
}

/// Header values that must not show up in logs or `Debug` output.
const HIDDEN_HEADERS: &[&str] = &["authorization", "x-amz-security-token"];

impl fmt::Debug for HttpCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                let hidden = HIDDEN_HEADERS.iter().any(|h| name.eq_ignore_ascii_case(h));
                (
                    name.as_str(),
                    if hidden { "<hidden>" } else { value.as_str() },
                )
            })
            .collect();
        f.debug_struct("HttpCall")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body_len", &self.body.len())
            .field("content_type", &self.content_type)
            .finish()
    }
}

/// The answer: any status (4xx / 5xx included, with their bodies).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpReply {
    /// The first header of this name, compared without case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 2xx.
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends one request and blocks until its answer. An `Err` is a request that got
/// no HTTP answer at all (DNS, connection, TLS, timeout), as a readable sentence.
pub trait Transport: Send + Sync {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String>;
}
