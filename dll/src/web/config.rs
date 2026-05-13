//! Parse `AZ_BACKEND=web://ip:port[?options]` URL format.
//!
//! Used by the backend selection logic in `compositor.rs` when parsing
//! the `AZ_BACKEND` environment variable to configure the web backend.
//!
//! # URL format
//!
//! `web://<host>:<port>[?key=value&key=value...]`
//!
//! Supported query parameters (see `WebConfig` for defaults):
//! - `tls_cert=<path>` / `tls_key=<path>` — TLS PEM files. Both must be
//!   set together; setting only one is an error.
//! - `max_body=<bytes>` — request body cap. Must be `> 0` and `<= 1 GiB`.
//!   Default `16 MiB`.
//! - `auth_token=<string>` — required `Authorization: Bearer <token>`
//!   for `/az/exec/*`. Embedded NUL / CR / LF are rejected.
//! - `allow_public=1` — opt-in to bind on a non-loopback address. By
//!   default, binding to `0.0.0.0` / any public IP is rejected because
//!   the server has no authentication on by default and only minimal
//!   DoS protection.
//! - `max_connections=<n>` — concurrent connection cap. Default unlimited.
//!
//! All values are percent-decoded. Duplicate keys are rejected as
//! defence-in-depth against override smuggling.
//!
//! See `doc/guide/en/internals/web.md` for the user-facing documentation.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

/// Default body cap when `max_body` isn't supplied — 16 MiB.
pub const DEFAULT_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Hard upper bound on `max_body`; values above this are rejected.
pub const MAX_BODY_BYTES_LIMIT: usize = 1024 * 1024 * 1024;

/// Configuration for the web backend, parsed from the `AZ_BACKEND` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfig {
    /// Bind address (`host:port`).
    pub bind: SocketAddr,
    /// TLS certificate PEM path. Always paired with `tls_key`.
    pub tls_cert: Option<PathBuf>,
    /// TLS private-key PEM path. Always paired with `tls_cert`.
    pub tls_key: Option<PathBuf>,
    /// Maximum POST body size in bytes. Bodies exceeding this get 413.
    pub max_body_bytes: usize,
    /// Bearer token required on `/az/exec/*`. `None` disables auth.
    pub auth_token: Option<String>,
    /// `true` if the caller opted in to a non-loopback bind.
    pub allow_public: bool,
    /// Optional concurrent connection cap.
    pub max_connections: Option<usize>,
}

impl WebConfig {
    /// Default config bound to a given loopback address.
    pub fn loopback(bind: SocketAddr) -> Self {
        Self {
            bind,
            tls_cert: None,
            tls_key: None,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            auth_token: None,
            allow_public: false,
            max_connections: None,
        }
    }
}

/// Error variants returned by `parse_web_config`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebConfigError {
    /// Doesn't start with `web://`.
    NotWebUrl,
    /// `<host>:<port>` part doesn't parse as a `SocketAddr`.
    InvalidSocketAddr,
    /// `bind` is non-loopback and `allow_public=1` was not set.
    PublicBindWithoutOptIn,
    /// `tls_cert` set but `tls_key` missing (or vice versa).
    IncompleteTlsPair,
    /// `max_body` is `0` or exceeds the 1 GiB hard cap.
    MaxBodyOutOfRange,
    /// Same query key supplied more than once.
    DuplicateKey(String),
    /// Query parameter value contains an embedded control character.
    InvalidControlChar(String),
    /// Query parameter value isn't valid percent-encoded UTF-8.
    InvalidPercentEncoding,
    /// `max_connections=0` or non-numeric value.
    InvalidMaxConnections,
    /// Unknown query key.
    UnknownQueryKey(String),
}

/// Parse a `web://ip:port[?query]` URL string into a full `WebConfig`.
///
/// Returns `Err(WebConfigError)` on any failure (bad URL, failed
/// security check, malformed query). Callers typically log the error
/// and fall back to a different backend.
///
/// Accepted forms:
/// - `web://127.0.0.1:8080`
/// - `web://[::1]:8080`
/// - `web://0.0.0.0:3000?allow_public=1`
/// - `web://127.0.0.1:8443?tls_cert=cert.pem&tls_key=key.pem`
/// - `web://127.0.0.1:8080?auth_token=secret&max_body=4194304`
pub fn parse_web_config(s: &str) -> Result<WebConfig, WebConfigError> {
    let s = s.trim();

    let rest = if s.get(..6).map_or(false, |p| p.eq_ignore_ascii_case("web://")) {
        &s[6..]
    } else {
        return Err(WebConfigError::NotWebUrl);
    };

    let (addr_str, query_str) = match rest.find('?') {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };

    let bind: SocketAddr = addr_str
        .parse()
        .map_err(|_| WebConfigError::InvalidSocketAddr)?;

    let mut cfg = WebConfig {
        bind,
        tls_cert: None,
        tls_key: None,
        max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        auth_token: None,
        allow_public: false,
        max_connections: None,
    };

    if let Some(q) = query_str {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for pair in q.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = match pair.split_once('=') {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            if !seen.insert(k.to_string()) {
                return Err(WebConfigError::DuplicateKey(k.to_string()));
            }
            let decoded =
                percent_decode(v).ok_or(WebConfigError::InvalidPercentEncoding)?;
            match k {
                "tls_cert" => cfg.tls_cert = Some(PathBuf::from(decoded)),
                "tls_key" => cfg.tls_key = Some(PathBuf::from(decoded)),
                "max_body" => {
                    let n: usize = decoded
                        .parse()
                        .map_err(|_| WebConfigError::MaxBodyOutOfRange)?;
                    if n == 0 || n > MAX_BODY_BYTES_LIMIT {
                        return Err(WebConfigError::MaxBodyOutOfRange);
                    }
                    cfg.max_body_bytes = n;
                }
                "auth_token" => {
                    if decoded.bytes().any(|b| matches!(b, 0 | b'\n' | b'\r')) {
                        return Err(WebConfigError::InvalidControlChar(
                            "auth_token".into(),
                        ));
                    }
                    cfg.auth_token = Some(decoded);
                }
                "allow_public" => {
                    cfg.allow_public = matches!(decoded.as_str(), "1" | "true" | "yes");
                }
                "max_connections" => {
                    let n: usize = decoded
                        .parse()
                        .map_err(|_| WebConfigError::InvalidMaxConnections)?;
                    if n == 0 {
                        return Err(WebConfigError::InvalidMaxConnections);
                    }
                    cfg.max_connections = Some(n);
                }
                _ => return Err(WebConfigError::UnknownQueryKey(k.to_string())),
            }
        }
    }

    if cfg.tls_cert.is_some() != cfg.tls_key.is_some() {
        return Err(WebConfigError::IncompleteTlsPair);
    }

    if !cfg.allow_public && !is_loopback(cfg.bind.ip()) {
        return Err(WebConfigError::PublicBindWithoutOptIn);
    }

    Ok(cfg)
}

/// Parse a `web://ip:port` URL string into a `SocketAddr`.
///
/// Thin wrapper around `parse_web_config` that discards everything but
/// the bind address. Returns `None` if `parse_web_config` would fail
/// for any reason (bad URL, failed security check, malformed query).
///
/// Accepted formats:
/// - `web://127.0.0.1:8080`
/// - `web://[::1]:8080` (IPv6)
/// - `web://0.0.0.0:3000?allow_public=1` (opt-in for public bind)
///
/// Returns `None` if the string doesn't start with `web://`, the
/// address cannot be parsed, the bind is non-loopback without
/// `?allow_public=1`, or any query parameter fails validation.
pub fn parse_web_url(s: &str) -> Option<SocketAddr> {
    parse_web_config(s).ok().map(|c| c.bind)
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Minimal RFC 3986 percent-decoder. Accepts `+` as space (form
/// encoding) since `AZ_BACKEND` values are typically hand-typed.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hi = hex_digit(bytes[i + 1])?;
                let lo = hex_digit(bytes[i + 2])?;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

    fn loopback_v4(port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
    }

    #[test]
    fn parse_ipv4() {
        assert_eq!(
            parse_web_url("web://127.0.0.1:8080"),
            Some(loopback_v4(8080))
        );
    }

    #[test]
    fn parse_ipv6() {
        assert_eq!(
            parse_web_url("web://[::1]:8080"),
            Some(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::LOCALHOST,
                8080,
                0,
                0
            )))
        );
    }

    #[test]
    fn parse_case_insensitive() {
        assert!(parse_web_url("WEB://127.0.0.1:8080").is_some());
        assert!(parse_web_url("Web://127.0.0.1:8080").is_some());
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse_web_url("headless"), None);
        assert_eq!(parse_web_url("web://"), None);
        assert_eq!(parse_web_url("web://not-an-address"), None);
        assert_eq!(parse_web_url("http://127.0.0.1:8080"), None);
    }

    #[test]
    fn parse_loopback_allowed_by_default() {
        assert!(parse_web_config("web://127.0.0.1:8080").is_ok());
        assert!(parse_web_config("web://[::1]:8080").is_ok());
    }

    #[test]
    fn parse_public_bind_requires_allow_public() {
        // Without opt-in, 0.0.0.0 is rejected.
        assert_eq!(
            parse_web_config("web://0.0.0.0:3000"),
            Err(WebConfigError::PublicBindWithoutOptIn)
        );
        // Public IPv4 also rejected without opt-in.
        assert_eq!(
            parse_web_config("web://192.0.2.1:8080"),
            Err(WebConfigError::PublicBindWithoutOptIn)
        );
        // With opt-in, accepted.
        let cfg = parse_web_config("web://0.0.0.0:3000?allow_public=1").unwrap();
        assert!(cfg.allow_public);
    }

    #[test]
    fn parse_tls_pair() {
        let cfg = parse_web_config(
            "web://127.0.0.1:8443?tls_cert=cert.pem&tls_key=key.pem",
        )
        .unwrap();
        assert_eq!(cfg.tls_cert, Some(PathBuf::from("cert.pem")));
        assert_eq!(cfg.tls_key, Some(PathBuf::from("key.pem")));
    }

    #[test]
    fn parse_tls_cert_without_key_rejected() {
        assert_eq!(
            parse_web_config("web://127.0.0.1:8443?tls_cert=cert.pem"),
            Err(WebConfigError::IncompleteTlsPair)
        );
        assert_eq!(
            parse_web_config("web://127.0.0.1:8443?tls_key=key.pem"),
            Err(WebConfigError::IncompleteTlsPair)
        );
    }

    #[test]
    fn parse_max_body_bounds() {
        // Zero rejected.
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?max_body=0"),
            Err(WebConfigError::MaxBodyOutOfRange)
        );
        // Above 1 GiB rejected.
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?max_body=2147483648"),
            Err(WebConfigError::MaxBodyOutOfRange)
        );
        // Valid value accepted.
        let cfg =
            parse_web_config("web://127.0.0.1:8080?max_body=4194304").unwrap();
        assert_eq!(cfg.max_body_bytes, 4 * 1024 * 1024);
        // Default applies when absent.
        let cfg = parse_web_config("web://127.0.0.1:8080").unwrap();
        assert_eq!(cfg.max_body_bytes, DEFAULT_MAX_BODY_BYTES);
    }

    #[test]
    fn parse_auth_token_rejects_control_chars() {
        // Raw NUL byte (`%00`).
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?auth_token=foo%00bar"),
            Err(WebConfigError::InvalidControlChar("auth_token".into()))
        );
        // CR / LF.
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?auth_token=foo%0Abar"),
            Err(WebConfigError::InvalidControlChar("auth_token".into()))
        );
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?auth_token=foo%0Dbar"),
            Err(WebConfigError::InvalidControlChar("auth_token".into()))
        );
        // Plain printable token accepted.
        let cfg =
            parse_web_config("web://127.0.0.1:8080?auth_token=s3cr3t").unwrap();
        assert_eq!(cfg.auth_token.as_deref(), Some("s3cr3t"));
    }

    #[test]
    fn parse_duplicate_keys_rejected() {
        assert_eq!(
            parse_web_config(
                "web://127.0.0.1:8080?max_body=1024&max_body=2048"
            ),
            Err(WebConfigError::DuplicateKey("max_body".into()))
        );
    }

    #[test]
    fn parse_multiple_query_options() {
        let cfg = parse_web_config(
            "web://127.0.0.1:8080?max_body=1048576&auth_token=secret&max_connections=64",
        )
        .unwrap();
        assert_eq!(cfg.bind, loopback_v4(8080));
        assert_eq!(cfg.max_body_bytes, 1024 * 1024);
        assert_eq!(cfg.auth_token.as_deref(), Some("secret"));
        assert_eq!(cfg.max_connections, Some(64));
    }

    #[test]
    fn parse_unknown_query_key_rejected() {
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?bogus=1"),
            Err(WebConfigError::UnknownQueryKey("bogus".into()))
        );
    }

    #[test]
    fn parse_invalid_max_connections_rejected() {
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?max_connections=0"),
            Err(WebConfigError::InvalidMaxConnections)
        );
        assert_eq!(
            parse_web_config("web://127.0.0.1:8080?max_connections=abc"),
            Err(WebConfigError::InvalidMaxConnections)
        );
    }

    #[test]
    fn parse_url_back_compat_wrapper() {
        // `parse_web_url` is the back-compat wrapper used by the
        // compositor — it returns just the bind address.
        assert_eq!(
            parse_web_url("web://127.0.0.1:8080"),
            Some(loopback_v4(8080))
        );
        // Failed validation propagates as `None`.
        assert_eq!(parse_web_url("web://0.0.0.0:3000"), None);
        assert_eq!(
            parse_web_url("web://0.0.0.0:3000?allow_public=1"),
            Some(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::UNSPECIFIED,
                3000
            )))
        );
    }

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("hello").as_deref(), Some("hello"));
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("a+b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("%C3%A9").as_deref(), Some("é"));
        // Invalid encodings.
        assert!(percent_decode("%ZZ").is_none());
        assert!(percent_decode("%2").is_none());
    }
}
