//! URL types for the C API.
//!
//! Provides a C-compatible, parsed-URL type. Key types: [`Url`],
//! [`UrlParseError`], [`ResultUrlUrlParseError`].
//!
//! The POD type and the cheap accessors live here in `azul-core` (so consumers
//! like `crate::video::VideoSource` can hold a typed `Url` without an
//! `azul-layout` dependency). `Url::parse` / `Url::join`, which rely on the
//! `url` crate, are gated behind the `url` feature; `azul_layout`'s `http`
//! feature enables it. Re-exported as `azul_layout::url`.

#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::string::String;
use core::fmt;

use azul_css::{impl_result, impl_result_inner, AzString};

/// A parsed URL
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct Url {
    /// The full URL string
    pub href: AzString,
    /// The scheme (e.g., "https")
    pub scheme: AzString,
    /// The host (e.g., "example.com")
    pub host: AzString,
    /// The port number, or 0 if not specified (sentinel value; see `effective_port()`)
    pub port: u16,
    /// The path (e.g., "/path/to/resource")
    pub path: AzString,
    /// The query string without '?' (e.g., "key=value")
    pub query: AzString,
    /// The fragment without '#' (e.g., "section")
    pub fragment: AzString,
}

/// Error when parsing a URL
#[derive(Debug, Clone, PartialEq, Eq)]
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
    [Debug, Clone, PartialEq, Eq]
);

impl Url {
    /// Parse a URL from a string
    #[cfg(feature = "url")]
    pub fn parse(s: &str) -> Result<Self, UrlParseError> {
        use ::url::Url as UrlParser;

        let parsed = UrlParser::parse(s).map_err(|e| UrlParseError {
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
    #[must_use] pub fn from_parts(scheme: &str, host: &str, port: u16, path: &str) -> Self {
        let port_str = if port == 0
            || (scheme == "http" && port == 80)
            || (scheme == "https" && port == 443)
        {
            String::new()
        } else {
            alloc::format!(":{port}")
        };

        let href = alloc::format!("{scheme}://{host}{port_str}{path}");

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
    #[must_use] pub fn as_str(&self) -> &str {
        self.href.as_str()
    }

    /// Check if this is an HTTPS URL
    #[must_use] pub fn is_https(&self) -> bool {
        self.scheme.as_str() == "https"
    }

    /// Check if this is an HTTP URL
    #[must_use] pub fn is_http(&self) -> bool {
        self.scheme.as_str() == "http"
    }

    /// Get the effective port (using default ports for http/https)
    #[must_use] pub fn effective_port(&self) -> u16 {
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
    #[cfg(feature = "url")]
    pub fn join(&self, path: &str) -> Result<Self, UrlParseError> {
        use ::url::Url as UrlParser;

        let base = UrlParser::parse(self.href.as_str()).map_err(|e| UrlParseError {
            message: AzString::from(e.to_string()),
        })?;

        let joined = base.join(path).map_err(|e| UrlParseError {
            message: AzString::from(e.to_string()),
        })?;

        Self::parse(joined.as_str())
    }

    /// Stub: `url` feature disabled (the `url` crate is gated behind it).
    #[cfg(not(feature = "url"))]
    /// # Errors
    ///
    /// Returns an error: the `url` feature is disabled, so URL parsing is unsupported.
    pub const fn parse(_s: &str) -> Result<Self, UrlParseError> {
        Err(UrlParseError {
            message: AzString::from_const_str("url feature not enabled"),
        })
    }

    /// Stub: `url` feature disabled (the `url` crate is gated behind it).
    #[cfg(not(feature = "url"))]
    /// # Errors
    ///
    /// Returns an error: the `url` feature is disabled, so URL joining is unsupported.
    pub const fn join(&self, _path: &str) -> Result<Self, UrlParseError> {
        Err(UrlParseError {
            message: AzString::from_const_str("url feature not enabled"),
        })
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
    #[cfg(feature = "url")]
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

#[cfg(test)]
mod autotest_generated {
    use alloc::format;

    use super::*;

    /// Structural invariants that must hold for ANY `Url`, however it was built.
    ///
    /// Only checks properties derivable from the type's own contract, so it is
    /// safe to apply to the output of the external `url` crate parser too.
    fn assert_url_invariants(u: &Url) {
        // as_str() is exactly the href field, and Display agrees with it.
        assert_eq!(u.as_str(), u.href.as_str());
        assert_eq!(format!("{u}"), u.href.as_str());

        // is_http / is_https are mutually exclusive and match the scheme exactly.
        assert_eq!(u.is_https(), u.scheme.as_str() == "https");
        assert_eq!(u.is_http(), u.scheme.as_str() == "http");
        assert!(!(u.is_http() && u.is_https()));

        // effective_port() is a pure function of (port, scheme).
        let expected_port = if u.port != 0 {
            u.port
        } else if u.is_https() {
            443
        } else if u.is_http() {
            80
        } else {
            0
        };
        assert_eq!(u.effective_port(), expected_port);

        // Equality/clone consistency (the type derives Clone + PartialEq).
        assert_eq!(u.clone(), *u);
    }

    // ---------------------------------------------------------------------
    // Url::default() / getters / predicates on degenerate instances
    // ---------------------------------------------------------------------

    #[test]
    fn default_url_is_inert_and_does_not_panic() {
        let u = Url::default();
        assert_eq!(u.as_str(), "");
        assert_eq!(u.href.as_str(), "");
        assert_eq!(u.scheme.as_str(), "");
        assert_eq!(u.host.as_str(), "");
        assert_eq!(u.port, 0);
        assert!(!u.is_https());
        assert!(!u.is_http());
        // Unknown scheme + sentinel port => no default port to infer.
        assert_eq!(u.effective_port(), 0);
        assert_eq!(format!("{u}"), "");
        assert_url_invariants(&u);
    }

    #[test]
    fn effective_port_covers_every_scheme_port_combination() {
        // Explicit port always wins, even when it contradicts the scheme default.
        assert_eq!(
            Url::from_parts("https", "h", 80, "/").effective_port(),
            80,
            "explicit port must win over the https default"
        );
        assert_eq!(Url::from_parts("http", "h", 443, "/").effective_port(), 443);
        assert_eq!(
            Url::from_parts("http", "h", u16::MAX, "/").effective_port(),
            u16::MAX
        );
        assert_eq!(Url::from_parts("https", "h", 1, "/").effective_port(), 1);

        // Sentinel port 0 => infer from scheme.
        assert_eq!(Url::from_parts("https", "h", 0, "/").effective_port(), 443);
        assert_eq!(Url::from_parts("http", "h", 0, "/").effective_port(), 80);
        assert_eq!(Url::from_parts("ftp", "h", 0, "/").effective_port(), 0);
        assert_eq!(Url::from_parts("", "", 0, "").effective_port(), 0);
    }

    #[test]
    fn predicates_are_case_sensitive_and_reject_near_misses() {
        // Near-miss schemes must NOT be reported as http/https.
        for scheme in [
            "HTTPS", "Https", "httpss", "https ", " https", "http\0", "ws", "httpx", "",
        ] {
            let u = Url::from_parts(scheme, "example.com", 0, "/");
            assert!(
                !u.is_https(),
                "scheme {scheme:?} must not be treated as https"
            );
            assert_url_invariants(&u);
        }
        for scheme in ["HTTP", "Http", "httpx", "https", "htt", ""] {
            let u = Url::from_parts(scheme, "example.com", 0, "/");
            assert!(
                !u.is_http(),
                "scheme {scheme:?} must not be treated as http"
            );
        }
        assert!(Url::from_parts("https", "h", 0, "/").is_https());
        assert!(Url::from_parts("http", "h", 0, "/").is_http());
    }

    // ---------------------------------------------------------------------
    // Url::from_parts — constructor, no panics, invariants
    // ---------------------------------------------------------------------

    #[test]
    fn from_parts_omits_default_and_sentinel_ports_only() {
        // Sentinel 0 => no port in href.
        assert_eq!(
            Url::from_parts("https", "example.com", 0, "/a").as_str(),
            "https://example.com/a"
        );
        // Scheme-matching default ports => elided.
        assert_eq!(
            Url::from_parts("http", "example.com", 80, "/").as_str(),
            "http://example.com/"
        );
        assert_eq!(
            Url::from_parts("https", "example.com", 443, "/").as_str(),
            "https://example.com/"
        );
        // Cross-scheme "defaults" are NOT elided.
        assert_eq!(
            Url::from_parts("https", "example.com", 80, "/").as_str(),
            "https://example.com:80/"
        );
        assert_eq!(
            Url::from_parts("http", "example.com", 443, "/").as_str(),
            "http://example.com:443/"
        );
        // A non-default port is always rendered, including the u16 boundary.
        assert_eq!(
            Url::from_parts("http", "example.com", u16::MAX, "/").as_str(),
            "http://example.com:65535/"
        );
        assert_eq!(
            Url::from_parts("ftp", "example.com", 443, "/").as_str(),
            "ftp://example.com:443/"
        );
    }

    #[test]
    fn from_parts_keeps_the_port_field_even_when_elided_from_href() {
        // The elision is purely cosmetic: the struct field must still carry the
        // caller's port, and effective_port() must agree with it.
        let u = Url::from_parts("https", "example.com", 443, "/api");
        assert_eq!(u.port, 443);
        assert!(!u.as_str().contains(":443"));
        assert_eq!(u.effective_port(), 443);
        assert_url_invariants(&u);
    }

    #[test]
    fn from_parts_fields_mirror_the_arguments_verbatim() {
        let u = Url::from_parts("https", "example.com", 8080, "/a/b");
        assert_eq!(u.scheme.as_str(), "https");
        assert_eq!(u.host.as_str(), "example.com");
        assert_eq!(u.port, 8080);
        assert_eq!(u.path.as_str(), "/a/b");
        // from_parts has no query/fragment inputs; they must be empty, not garbage.
        assert_eq!(u.query.as_str(), "");
        assert_eq!(u.fragment.as_str(), "");
        assert_eq!(u.as_str(), "https://example.com:8080/a/b");
        assert_url_invariants(&u);
    }

    #[test]
    fn from_parts_does_not_panic_on_extreme_or_empty_arguments() {
        // Every argument empty: degenerate but must not panic.
        let u = Url::from_parts("", "", 0, "");
        assert_eq!(u.as_str(), "://");
        assert_url_invariants(&u);

        // from_parts is a raw formatter, not a validator: it must not panic on
        // inputs that could never parse, and must reproduce them byte-for-byte.
        for (scheme, host, port, path) in [
            ("://", "://", 1, "://"),
            ("http", "user:pw@host", 0, "/x"),
            ("http", "[::1]", 8080, "/x"),
            ("http", "a b c", 0, "/p a t h"),
            ("http", "example.com", 0, "no-leading-slash"),
            ("http", "example.com", 0, "?query#frag"),
            ("\n\t", "\r", 65535, "\0"),
        ] {
            let u = Url::from_parts(scheme, host, port, path);
            assert_eq!(u.scheme.as_str(), scheme);
            assert_eq!(u.host.as_str(), host);
            assert_eq!(u.port, port);
            assert_eq!(u.path.as_str(), path);
            assert!(u.as_str().starts_with(scheme));
            assert_url_invariants(&u);
        }
    }

    #[test]
    fn from_parts_handles_unicode_without_panicking_or_mangling() {
        let u = Url::from_parts("https", "例え.テスト", 0, "/パス/😀");
        assert_eq!(u.host.as_str(), "例え.テスト");
        assert_eq!(u.path.as_str(), "/パス/😀");
        // No IDNA/percent-encoding happens here — from_parts is a plain formatter.
        assert_eq!(u.as_str(), "https://例え.テスト/パス/😀");
        assert!(u.is_https());
        assert_url_invariants(&u);

        // Combining marks + a lone emoji as the whole host.
        let u = Url::from_parts("http", "e\u{0301}xample", 1, "/\u{1F600}");
        assert!(u.as_str().contains('\u{0301}'));
        assert_url_invariants(&u);
    }

    #[test]
    fn from_parts_handles_huge_inputs_without_hanging() {
        let host = "a".repeat(100_000);
        let path = "/".to_string() + &"b".repeat(100_000);
        let u = Url::from_parts("https", &host, 8080, &path);
        // "https" + "://" + host + ":8080" + path
        assert_eq!(u.as_str().len(), 5 + 3 + 100_000 + 5 + 100_001);
        assert_eq!(u.host.as_str().len(), 100_000);
        assert!(u.is_https());
        assert_eq!(u.effective_port(), 8080);
        assert_url_invariants(&u);
    }

    // ---------------------------------------------------------------------
    // Display / serializer
    // ---------------------------------------------------------------------

    #[test]
    fn url_display_never_reinterprets_the_href() {
        // Display must be a verbatim echo of href, not a re-serialization.
        for href in ["", "://", "not a url", "{}{{}", "%s%n", "\u{1F600}"] {
            let u = Url {
                href: AzString::from(href),
                ..Url::default()
            };
            assert_eq!(format!("{u}"), href);
            assert_eq!(u.as_str(), href);
        }
    }

    #[test]
    fn url_parse_error_display_is_verbatim_and_panic_free() {
        // Default / empty message.
        let e = UrlParseError {
            message: AzString::from(""),
        };
        assert_eq!(format!("{e}"), "");

        // Format-specifier-looking payloads must NOT be interpreted.
        for msg in [
            "relative URL without a base",
            "{}",
            "{0} {1} {}",
            "%s %n %p",
            "\u{1F600} invalid",
            "e\u{0301}",
            "\0\t\n",
        ] {
            let e = UrlParseError {
                message: AzString::from(msg),
            };
            assert_eq!(format!("{e}"), msg);
        }

        // Non-empty for a representative value, and huge messages don't panic.
        let big = "x".repeat(100_000);
        let e = UrlParseError {
            message: AzString::from(big.as_str()),
        };
        assert_eq!(format!("{e}").len(), 100_000);

        // from_const_str path (used by the no-`url`-feature stubs).
        let e = UrlParseError {
            message: AzString::from_const_str("url feature not enabled"),
        };
        assert_eq!(format!("{e}"), "url feature not enabled");
    }

    // ---------------------------------------------------------------------
    // FFI result type
    // ---------------------------------------------------------------------

    #[test]
    fn ffi_result_round_trips_both_variants() {
        let ok: Result<Url, UrlParseError> = Ok(Url::from_parts("https", "a.b", 0, "/"));
        let ffi: ResultUrlUrlParseError = ok.clone().into();
        assert!(ffi.is_ok());
        assert!(!ffi.is_err());
        let back: Result<Url, UrlParseError> = ffi.into();
        assert_eq!(back, ok);

        let err: Result<Url, UrlParseError> = Err(UrlParseError {
            message: AzString::from("boom"),
        });
        let ffi: ResultUrlUrlParseError = err.clone().into();
        assert!(ffi.is_err());
        assert!(!ffi.is_ok());
        assert_eq!(
            ffi.as_result().unwrap_err().message.as_str(),
            "boom",
            "as_result() must borrow the same error payload"
        );
        let back: Result<Url, UrlParseError> = ffi.into();
        assert_eq!(back, err);
    }

    #[cfg(feature = "std")]
    #[test]
    fn eq_implies_equal_hash() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        fn hash_of(u: &Url) -> u64 {
            let mut h = DefaultHasher::new();
            u.hash(&mut h);
            h.finish()
        }

        let a = Url::from_parts("https", "example.com", 8080, "/a");
        let b = Url::from_parts("https", "example.com", 8080, "/a");
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));

        // Port is part of identity even when it is elided from the href.
        let c = Url::from_parts("https", "example.com", 443, "/a");
        let d = Url::from_parts("https", "example.com", 0, "/a");
        assert_eq!(c.as_str(), d.as_str(), "hrefs are identical…");
        assert_ne!(c, d, "…but the port field still distinguishes them");
    }

    // =====================================================================
    // Parser tests — only meaningful with the `url` feature.
    // =====================================================================

    /// Parse `s`; if it succeeds, assert the general invariants AND that
    /// re-parsing the serialization is a fixed point (`parse(as_str(x)) == x`).
    ///
    /// Used for inputs whose accept/reject verdict is the `url` crate's business:
    /// the point is that we never panic and never produce an inconsistent `Url`.
    #[cfg(feature = "url")]
    fn assert_no_panic_and_idempotent(s: &str) {
        match Url::parse(s) {
            Ok(u) => {
                assert_url_invariants(&u);
                let again = Url::parse(u.as_str())
                    .expect("a serialized URL must always re-parse (idempotent normalization)");
                assert_eq!(again, u, "parse(serialize(x)) must equal x for input {s:?}");
                assert_eq!(again.as_str(), u.as_str());
            }
            Err(e) => {
                // An error must carry a diagnosable, non-empty message.
                assert!(
                    !e.message.as_str().is_empty(),
                    "error for {s:?} must have a message"
                );
                assert_eq!(format!("{e}"), e.message.as_str());
            }
        }
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_valid_minimal_positive_control() {
        let u = Url::parse("http://example.com").expect("minimal absolute URL must parse");
        assert_eq!(u.scheme.as_str(), "http");
        assert_eq!(u.host.as_str(), "example.com");
        // No explicit port => the 0 sentinel, resolved by effective_port().
        assert_eq!(u.port, 0);
        assert_eq!(u.effective_port(), 80);
        assert!(u.is_http());
        assert!(!u.is_https());
        // The parser normalizes the empty path to "/".
        assert_eq!(u.path.as_str(), "/");
        assert_eq!(u.query.as_str(), "");
        assert_eq!(u.fragment.as_str(), "");
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_rejects_empty_and_whitespace_only_input() {
        for s in ["", " ", "   ", "\t", "\n", "\t\n", "\r\n  \t "] {
            let r = Url::parse(s);
            assert!(r.is_err(), "{s:?} must not parse as an absolute URL");
            let e = r.unwrap_err();
            assert!(!e.message.as_str().is_empty());
        }
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_rejects_garbage_without_panicking() {
        for s in [
            "not a url",
            "///",
            "::::",
            "http://",
            "\u{0}\u{1}\u{2}",
            "\u{FFFD}\u{FFFD}",
            "?query-only",
            "#fragment-only",
            "/absolute/path/only",
            "../relative",
        ] {
            let r = Url::parse(s);
            if let Ok(ref u) = r {
                // If the parser DOES accept it, the result must still be coherent.
                assert_url_invariants(u);
            } else {
                assert!(!r.unwrap_err().message.as_str().is_empty());
            }
        }
        // These are unambiguously relative and must be rejected.
        assert!(Url::parse("not a url").is_err());
        assert!(Url::parse("/absolute/path/only").is_err());
        assert!(Url::parse("http://").is_err(), "empty host must be an error");
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_rejects_out_of_range_and_non_numeric_ports() {
        // u16 overflow at the boundary and far beyond it.
        assert!(Url::parse("http://example.com:65536/").is_err());
        assert!(Url::parse("http://example.com:99999/").is_err());
        assert!(Url::parse("http://example.com:4294967296/").is_err());
        assert!(Url::parse("http://example.com:18446744073709551616/").is_err());
        assert!(Url::parse("http://example.com:-1/").is_err());
        assert!(Url::parse("http://example.com:NaN/").is_err());
        assert!(Url::parse("http://example.com:inf/").is_err());
        assert!(Url::parse("http://example.com:+80/").is_err());
        assert!(Url::parse(&format!("http://example.com:{}/", "9".repeat(10_000))).is_err());

        // The largest in-range port must survive intact (no truncation to 0).
        let u = Url::parse("http://example.com:65535/").expect("65535 is a valid port");
        assert_eq!(u.port, u16::MAX);
        assert_eq!(u.effective_port(), u16::MAX);
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_port_zero_collides_with_the_no_port_sentinel() {
        // `port: 0` doubles as "unspecified", so an explicit :0 is indistinguishable
        // from no port at all — effective_port() then reports the scheme default.
        // This documents the sentinel's cost; it must at least stay self-consistent.
        if let Ok(u) = Url::parse("http://example.com:0/") {
            assert_eq!(u.port, 0);
            assert_eq!(
                u.effective_port(),
                80,
                "explicit :0 is swallowed by the 0 sentinel"
            );
            assert_url_invariants(&u);
        }
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_default_ports_are_reported_via_effective_port() {
        // Whether the parser stores or elides a scheme-default port, effective_port()
        // must resolve to the same answer.
        let u = Url::parse("https://example.com:443/").unwrap();
        assert!(u.port == 0 || u.port == 443);
        assert_eq!(u.effective_port(), 443);
        assert_url_invariants(&u);

        let u = Url::parse("http://example.com:80/").unwrap();
        assert!(u.port == 0 || u.port == 80);
        assert_eq!(u.effective_port(), 80);
        assert_url_invariants(&u);

        // A non-http(s) scheme with no port has no default to fall back on.
        let u = Url::parse("ftp://example.com/").unwrap();
        assert_eq!(u.effective_port(), 0);
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_is_idempotent_across_hostile_inputs() {
        for s in [
            // boundary numbers
            "http://example.com/0",
            "http://example.com/-0",
            "http://example.com/9223372036854775807",
            "http://example.com/-9223372036854775808",
            "http://example.com/18446744073709551615",
            "http://example.com/?n=NaN&i=inf&e=1e400&t=1e-400",
            "http://example.com/#-0.0",
            // leading/trailing junk
            "  https://example.com/  ",
            "\thttps://example.com/\n",
            "https://example.com/valid;garbage",
            "https://example.com/a?b=c;d#e;f",
            // odd but legal shapes
            "https://user:pw@example.com:8080/p?q#f",
            "https://example.com/%2e%2e/%2E%2E/",
            "https://example.com/a//b///c",
            "https://example.com/?",
            "https://example.com/#",
            "https://example.com/?#",
            "http://[::1]:8080/",
            "http://[2001:db8::1]/",
            "http://127.0.0.1:8080/",
            "file:///etc/passwd",
            "data:text/plain,hello",
            "mailto:user@example.com",
            "urn:isbn:0451450523",
            "blob:https://example.com/uuid",
            // percent-encoding edge cases
            "https://example.com/%",
            "https://example.com/%zz",
            "https://example.com/%00",
            "https://example.com/%%%%",
            // unicode
            "https://example.com/\u{1F600}",
            "https://example.com/e\u{0301}\u{0301}\u{0301}",
            "https://example.com/?q=\u{4F8B}\u{3048}",
            "https://example.com/#\u{200B}\u{FEFF}",
            "https://\u{4F8B}\u{3048}.\u{30C6}\u{30B9}\u{30C8}/",
        ] {
            assert_no_panic_and_idempotent(s);
        }
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_normalizes_leading_and_trailing_whitespace_deterministically() {
        // Either the junk is stripped (spec behaviour) or the input is rejected —
        // never a Url carrying stray whitespace in its href.
        match Url::parse("  https://example.com/  ") {
            Ok(u) => {
                assert_eq!(u.host.as_str(), "example.com");
                assert_eq!(u.as_str(), "https://example.com/");
                assert!(!u.as_str().contains(' '));
                assert_url_invariants(&u);
            }
            Err(e) => assert!(!e.message.as_str().is_empty()),
        }
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_unicode_host_is_idna_encoded_to_ascii() {
        let u = Url::parse("https://\u{4F8B}\u{3048}.\u{30C6}\u{30B9}\u{30C8}/")
            .expect("an IDN host must parse");
        assert!(
            u.host.as_str().is_ascii(),
            "host must be punycode/ASCII after IDNA, got {:?}",
            u.host.as_str()
        );
        assert!(!u.host.as_str().is_empty());
        assert!(u.as_str().is_ascii(), "serialized href must be ASCII");
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_unicode_path_is_percent_encoded() {
        let u = Url::parse("https://example.com/\u{1F600}").expect("emoji path must parse");
        assert!(
            u.path.as_str().is_ascii(),
            "path must be percent-encoded, got {:?}",
            u.path.as_str()
        );
        assert!(u.as_str().is_ascii());
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_survives_extremely_long_input() {
        // 1M-char path segment: must be linear-time and allocation-safe, not a hang.
        let long = format!("http://example.com/{}", "a".repeat(1_000_000));
        let u = Url::parse(&long).expect("a long-but-valid URL must parse");
        assert_eq!(u.host.as_str(), "example.com");
        assert_eq!(u.path.as_str().len(), 1 + 1_000_000);
        assert_url_invariants(&u);

        // A 1M-char query and a 1M-char host label must also not panic.
        let long_q = format!("http://example.com/?{}", "k=v&".repeat(250_000));
        assert_no_panic_and_idempotent(&long_q);
        let long_host = format!("http://{}/", "h".repeat(100_000));
        assert_no_panic_and_idempotent(&long_host);
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_survives_deeply_nested_and_repetitive_input() {
        // 10k nested path segments — must not blow the stack.
        let nested = format!("http://example.com/{}", "a/".repeat(10_000));
        let u = Url::parse(&nested).expect("deeply nested path must parse");
        assert_eq!(u.path.as_str().matches('/').count(), 10_001);
        assert_url_invariants(&u);

        // 10k dot-dot segments that all try to escape the root.
        let dotdot = format!("http://example.com/{}", "../".repeat(10_000));
        let u = Url::parse(&dotdot).expect("dot-dot flood must parse");
        assert_eq!(u.host.as_str(), "example.com", "must not escape the origin");
        assert!(u.path.as_str().starts_with('/'));
        assert!(
            !u.path.as_str().contains(".."),
            "dot-dot segments must be resolved away, got {:?}",
            u.path.as_str()
        );
        assert_url_invariants(&u);

        // 10k nested brackets/parens as raw path junk.
        let brackets = format!(
            "http://example.com/{}{}",
            "(".repeat(10_000),
            ")".repeat(10_000)
        );
        assert_no_panic_and_idempotent(&brackets);
    }

    #[cfg(feature = "url")]
    #[test]
    fn parse_round_trips_a_fully_populated_url() {
        let src = "https://example.com:8080/path?query=1#frag";
        let u = Url::parse(src).unwrap();
        // Serialization is byte-identical to the (already-normalized) input…
        assert_eq!(u.as_str(), src);
        // …and re-parsing it is a fixed point across every field.
        let u2 = Url::parse(u.as_str()).unwrap();
        assert_eq!(u2, u);
        assert_eq!(format!("{u2}"), format!("{u}"));
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn from_parts_output_reparses_into_the_same_url() {
        // Non-default port: from_parts and parse must agree on every field.
        let built = Url::from_parts("https", "example.com", 8080, "/a/b");
        let parsed = Url::parse(built.as_str()).expect("from_parts output must be parseable");
        assert_eq!(parsed, built, "from_parts must be a faithful serializer");

        // Elided default port: the href round-trips, but the port FIELD does not —
        // the parser reports the 0 sentinel where from_parts kept 443. The two
        // disagree on `port` yet must still agree on `effective_port()`.
        let built = Url::from_parts("https", "example.com", 443, "/a");
        let parsed = Url::parse(built.as_str()).unwrap();
        assert_eq!(parsed.as_str(), built.as_str());
        assert_eq!(built.port, 443);
        assert_eq!(parsed.port, 0);
        assert_ne!(parsed, built, "port-field asymmetry across the round trip");
        assert_eq!(parsed.effective_port(), built.effective_port());
        assert_eq!(parsed.effective_port(), 443);
    }

    // ---------------------------------------------------------------------
    // Url::join
    // ---------------------------------------------------------------------

    #[cfg(feature = "url")]
    #[test]
    fn join_valid_minimal_positive_control() {
        let base = Url::parse("https://example.com/a/b").unwrap();
        let j = base.join("c").expect("relative join must work");
        assert_eq!(j.as_str(), "https://example.com/a/c");
        assert_eq!(j.host.as_str(), "example.com");
        assert!(j.is_https());
        assert_eq!(j.effective_port(), 443);
        assert_url_invariants(&j);

        // Absolute path, absolute URL, and fragment-only joins.
        assert_eq!(
            base.join("/root").unwrap().as_str(),
            "https://example.com/root"
        );
        assert_eq!(
            base.join("http://other.com/x").unwrap().host.as_str(),
            "other.com"
        );
        assert_eq!(base.join("#f").unwrap().fragment.as_str(), "f");
        assert_eq!(base.join("?q=1").unwrap().query.as_str(), "q=1");
    }

    #[cfg(feature = "url")]
    #[test]
    fn join_on_an_unparseable_base_errors_instead_of_panicking() {
        // Default Url: href is "" — the base itself cannot be parsed.
        let e = Url::default()
            .join("/x")
            .expect_err("joining onto an empty base must fail");
        assert!(!e.message.as_str().is_empty());

        // from_parts can produce hrefs that are not valid URLs at all.
        for bad in [
            Url::from_parts("", "", 0, ""),
            Url::from_parts("://", "://", 1, "://"),
            Url::from_parts("http", "", 0, "/x"),
        ] {
            let r = bad.join("/y");
            if let Ok(ref u) = r {
                assert_url_invariants(u);
            } else {
                assert!(!r.unwrap_err().message.as_str().is_empty());
            }
        }
    }

    #[cfg(feature = "url")]
    #[test]
    fn join_never_panics_on_hostile_relative_inputs() {
        let base = Url::parse("https://example.com/a/b?q=1#f").unwrap();
        for path in [
            "",
            " ",
            "   ",
            "\t\n",
            "..",
            "../..",
            "/",
            "//",
            "///",
            "//other.com/x",
            "?",
            "#",
            "?#",
            ":",
            "::::",
            "not a path",
            "valid;garbage",
            "  padded  ",
            "%",
            "%zz",
            "%00",
            "\u{1F600}",
            "e\u{0301}",
            "\u{4F8B}\u{3048}",
            "\u{0}",
            "0",
            "-0",
            "9223372036854775807",
            "NaN",
            "inf",
            "javascript:alert(1)",
            "data:text/plain,x",
            "mailto:a@b.c",
        ] {
            match base.join(path) {
                Ok(u) => {
                    assert_url_invariants(&u);
                    // join() funnels through parse(), so the result must be a
                    // fixed point of the parser too.
                    let again = Url::parse(u.as_str())
                        .expect("join() output must always re-parse");
                    assert_eq!(again, u, "join({path:?}) is not idempotent");
                }
                Err(e) => assert!(
                    !e.message.as_str().is_empty(),
                    "join({path:?}) error needs a message"
                ),
            }
        }
    }

    #[cfg(feature = "url")]
    #[test]
    fn join_cannot_escape_the_origin_with_a_dot_dot_flood() {
        let base = Url::parse("https://example.com/a/b/c").unwrap();
        let escape = "../".repeat(10_000);
        let u = base
            .join(&escape)
            .expect("a dot-dot flood must resolve, not fail");
        assert_eq!(u.host.as_str(), "example.com");
        assert_eq!(u.scheme.as_str(), "https");
        assert_eq!(u.path.as_str(), "/", "must clamp at the root");
        assert!(!u.path.as_str().contains(".."));
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn join_survives_extremely_long_relative_paths() {
        let base = Url::parse("https://example.com/").unwrap();
        let long = "a".repeat(1_000_000);
        let u = base.join(&long).expect("a long relative path must join");
        assert_eq!(u.host.as_str(), "example.com");
        assert_eq!(u.path.as_str().len(), 1 + 1_000_000);
        assert_url_invariants(&u);

        // 10k nested segments.
        let nested = "x/".repeat(10_000);
        let u = base.join(&nested).expect("deep nesting must join");
        assert_eq!(u.path.as_str().matches('/').count(), 10_001);
        assert_url_invariants(&u);
    }

    #[cfg(feature = "url")]
    #[test]
    fn join_result_is_a_fully_populated_url_not_a_partial_one() {
        // join() re-parses, so query/fragment/port must all be repopulated
        // from the joined string rather than inherited or dropped.
        let base = Url::parse("http://example.com:8080/a?old=1#oldfrag").unwrap();
        let u = base.join("b?new=2#newfrag").unwrap();
        assert_eq!(u.as_str(), "http://example.com:8080/b?new=2#newfrag");
        assert_eq!(u.scheme.as_str(), "http");
        assert_eq!(u.host.as_str(), "example.com");
        assert_eq!(u.port, 8080);
        assert_eq!(u.path.as_str(), "/b");
        assert_eq!(u.query.as_str(), "new=2");
        assert_eq!(u.fragment.as_str(), "newfrag");
        assert_eq!(u.effective_port(), 8080);
        assert_url_invariants(&u);
    }

    // =====================================================================
    // Stub tests — the `url` feature is OFF, parse/join are const Err stubs.
    // =====================================================================

    #[cfg(not(feature = "url"))]
    #[test]
    fn stub_parse_always_errors_and_never_panics() {
        let huge = "a".repeat(1_000_000);
        let nested = "(".repeat(10_000);
        for s in [
            "",
            " ",
            "\t\n",
            "https://example.com/",
            "not a url",
            "0",
            "-0",
            "NaN",
            "inf",
            "9223372036854775807",
            "\u{1F600}",
            "e\u{0301}",
            "\u{0}",
            huge.as_str(),
            nested.as_str(),
        ] {
            let e = Url::parse(s).expect_err("the stub must always fail");
            assert_eq!(e.message.as_str(), "url feature not enabled");
            assert_eq!(format!("{e}"), "url feature not enabled");
        }
    }

    #[cfg(not(feature = "url"))]
    #[test]
    fn stub_join_always_errors_for_every_base_and_path() {
        let huge = "b".repeat(1_000_000);
        let bases = [
            Url::default(),
            Url::from_parts("https", "example.com", 8080, "/a"),
            Url::from_parts("", "", 0, ""),
        ];
        for base in &bases {
            for path in ["", " ", "../..", "\u{1F600}", "x", huge.as_str()] {
                let e = base.join(path).expect_err("the stub must always fail");
                assert_eq!(e.message.as_str(), "url feature not enabled");
            }
            // The base must be left untouched by the failed join.
            assert_url_invariants(base);
        }
    }

    #[cfg(not(feature = "url"))]
    #[test]
    fn stub_parse_and_join_are_usable_in_const_context() {
        // Both stubs are `const fn`; evaluating them at compile time must not
        // trip a const-eval panic.
        const PARSED: Result<Url, UrlParseError> = Url::parse("https://example.com/");
        assert!(PARSED.is_err());
    }
}
