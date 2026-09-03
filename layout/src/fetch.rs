//! One way to turn a URI into bytes, whatever scheme it names.
//!
//! Several places now hold a URI-valued field and need the CONTENT rather than
//! the string: album art (`NowPlayingInfo::artwork_url`), and anything else
//! that follows. Each of them faces the same two-case split - a local file is
//! a `read`, a remote one is an HTTP GET - and writing that split per call site
//! is how one of them ends up supporting only half the schemes, which is
//! exactly what the macOS artwork path did.
//!
//! # Scheme handling
//!
//! | input | route |
//! |---|---|
//! | `file:///a/b.png` | percent-DECODED, then read from disk |
//! | `/a/b.png`, `b.png` | read from disk as written |
//! | `http://…`, `https://…` | `HttpRequestConfig::http_get` |
//! | anything else | rejected by name, not silently treated as a path |
//!
//! A bare path is deliberately accepted: an app that stored a filename rather
//! than a URI is the common case, and `Url::parse` rejects it precisely because
//! it is not a URL.
//!
//! # This does NOT decide WHERE to call it from
//!
//! [`fetch_uri`] blocks. That is correct for a file and wrong for a network
//! round trip on the event loop, so a caller that may be handed a remote URI
//! runs it on a thread and re-publishes when the bytes land. Keeping the policy
//! out of here is what lets a caller that only ever sees local paths stay
//! synchronous.

use alloc::{string::String, vec::Vec};

/// Why a fetch produced no bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The URI names a scheme this cannot resolve (`data:`, `ftp:`, …).
    UnsupportedScheme(String),
    /// A local read failed - missing file, no permission.
    Io(String),
    /// The HTTP request failed, or answered with a non-2xx status.
    Http(String),
    /// The `http` feature is not built in, so a remote URI cannot be fetched.
    HttpUnavailable,
}

impl core::fmt::Display for FetchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedScheme(s) => write!(f, "unsupported URI scheme: {s}"),
            Self::Io(e) => write!(f, "read failed: {e}"),
            Self::Http(e) => write!(f, "http failed: {e}"),
            Self::HttpUnavailable => {
                write!(f, "a remote URI needs the `http` feature, which is not built in")
            }
        }
    }
}

/// How a URI resolves, decided before any I/O happens.
///
/// Separated from the fetch itself so a caller can ask "would this block on a
/// network?" without performing it - which is what a media publish on the event
/// loop needs to know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UriRoute {
    /// Read this path from disk. Already percent-decoded.
    LocalPath(String),
    /// GET this URL.
    Remote(String),
    /// Neither, and naming it is better than guessing.
    Unsupported(String),
}

impl UriRoute {
    /// Would resolving this block on a network round trip?
    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

/// Decide how a URI resolves, without touching the disk or the network.
///
/// The scheme comes from the real URL parser rather than a hand-rolled scan -
/// `azul_core::url::Url` is backed by the `url` crate, and getting scheme
/// detection subtly wrong is how `mailto:` or a Windows drive letter ends up in
/// the wrong branch.
#[must_use]
pub fn route_of(uri: &str) -> UriRoute {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return UriRoute::LocalPath(String::new());
    }

    // A WINDOWS DRIVE LETTER IS A PATH, and the URL spec disagrees: `C:` is a
    // perfectly good one-character scheme, so `C:\cover.png` parses as a URL
    // with scheme "c". This is checked before parsing rather than after,
    // because the parser's answer is not wrong so much as about a different
    // question.
    let mut chars = trimmed.chars();
    if let (Some(first), Some(':')) = (chars.next(), chars.next()) {
        if first.is_ascii_alphabetic() {
            return UriRoute::LocalPath(String::from(trimmed));
        }
    }

    match parse_scheme_and_path(trimmed) {
        // A `file:` URL's path is PERCENT-ENCODED: a space is `%20`, and
        // handing that to the filesystem asks for a file nobody has.
        Some((scheme, path)) if scheme == "file" => {
            UriRoute::LocalPath(percent_decode(&path))
        }
        Some((scheme, _)) if scheme == "http" || scheme == "https" => {
            UriRoute::Remote(String::from(trimmed))
        }
        Some((scheme, _)) => UriRoute::Unsupported(scheme),
        // Not a URL at all - a bare path, which is what an app that stored a
        // filename has.
        None => UriRoute::LocalPath(String::from(trimmed)),
    }
}

/// `(scheme, path)` when this parses as an absolute URL.
#[cfg(feature = "http")]
fn parse_scheme_and_path(uri: &str) -> Option<(String, String)> {
    let parsed = azul_core::url::Url::parse(uri).ok()?;
    Some((
        parsed.scheme.as_str().to_ascii_lowercase(),
        String::from(parsed.path.as_str()),
    ))
}

/// Without the `url` dependency the scheme is read directly.
///
/// Deliberately CONSERVATIVE where it differs from the parser: it accepts only
/// what RFC 3986 allows in a scheme, so anything it is unsure about falls
/// through to "a path", which is the safe answer - a wrong path fails loudly
/// when the file is missing, where a wrong scheme fails silently.
#[cfg(not(feature = "http"))]
fn parse_scheme_and_path(uri: &str) -> Option<(String, String)> {
    let colon = uri.find(':')?;
    let scheme = &uri[..colon];
    if scheme.len() < 2
        || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return None;
    }
    let rest = &uri[colon + 1..];
    let path = rest.strip_prefix("//").map_or(rest, |after| {
        // Skip the authority: `//host/path` -> `/path`.
        after.find('/').map_or("", |slash| &after[slash..])
    });
    Some((scheme.to_ascii_lowercase(), String::from(path)))
}

/// Decode `%XX` escapes. Invalid escapes are left as written rather than
/// dropped, so a filename that legitimately contains a `%` still resolves.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = core::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(byte) = hex {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolve a URI to bytes. BLOCKS - see the module docs.
///
/// # Errors
///
/// [`FetchError`] naming which half failed, so a caller can tell a missing
/// file from a refused download.
#[cfg(feature = "std")]
pub fn fetch_uri(uri: &str) -> Result<Vec<u8>, FetchError> {
    match route_of(uri) {
        UriRoute::LocalPath(path) => {
            std::fs::read(&path).map_err(|e| FetchError::Io(alloc::format!("{path}: {e}")))
        }
        UriRoute::Remote(url) => fetch_remote(&url),
        UriRoute::Unsupported(scheme) => Err(FetchError::UnsupportedScheme(scheme)),
    }
}

#[cfg(all(feature = "std", feature = "http"))]
fn fetch_remote(url: &str) -> Result<Vec<u8>, FetchError> {
    use azul_css::AzString;

    use crate::http::HttpRequestConfig;

    match HttpRequestConfig::default().http_get(AzString::from(String::from(url))) {
        crate::http::ResultHttpResponseHttpError::Ok(response) => {
            // A 404 body is an error page, not the image asked for. Treating a
            // non-2xx as success is how a broken URL becomes a corrupt file.
            if !(200..300).contains(&response.status_code) {
                return Err(FetchError::Http(alloc::format!(
                    "{url}: status {}",
                    response.status_code
                )));
            }
            Ok(response.body.as_ref().to_vec())
        }
        crate::http::ResultHttpResponseHttpError::Err(e) => {
            Err(FetchError::Http(alloc::format!("{url}: {e:?}")))
        }
    }
}

#[cfg(all(feature = "std", not(feature = "http")))]
fn fetch_remote(_url: &str) -> Result<Vec<u8>, FetchError> {
    Err(FetchError::HttpUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_url_is_local_and_percent_decoded() {
        assert_eq!(
            route_of("file:///Users/me/My%20Covers/art.png"),
            UriRoute::LocalPath(String::from("/Users/me/My Covers/art.png")),
            "a space is `%20` in a real cover path, and the filesystem wants the space"
        );
        assert_eq!(
            route_of("file:///plain.png"),
            UriRoute::LocalPath(String::from("/plain.png"))
        );
    }

    /// An app that stored a FILENAME rather than a URI is the common case, and
    /// `Url::parse` rejects those precisely because they are not URLs.
    #[test]
    fn a_bare_path_is_local_and_left_alone() {
        for path in ["/abs/cover.png", "cover.png", "./art/cover.jpg", ""] {
            assert_eq!(
                route_of(path),
                UriRoute::LocalPath(String::from(path)),
                "`{path}` must be treated as a path, unchanged"
            );
        }
    }

    /// The URL spec says `C:` is a scheme. The filesystem says it is a drive.
    #[test]
    fn a_windows_drive_letter_is_a_path_not_a_scheme() {
        assert_eq!(
            route_of("C:\\Music\\cover.png"),
            UriRoute::LocalPath(String::from("C:\\Music\\cover.png"))
        );
        assert_eq!(
            route_of("d:/music/cover.png"),
            UriRoute::LocalPath(String::from("d:/music/cover.png"))
        );
    }

    #[test]
    fn http_and_https_are_remote_and_everything_else_is_named() {
        assert!(route_of("http://example.com/a.png").is_remote());
        assert!(route_of("https://example.com/a.png").is_remote());
        assert!(route_of("HTTPS://EXAMPLE.COM/a.png").is_remote());

        // NOT silently treated as a path: a caller that gets `Unsupported`
        // can say which scheme it could not follow.
        assert_eq!(
            route_of("ftp://example.com/a.png"),
            UriRoute::Unsupported(String::from("ftp"))
        );
        assert!(matches!(
            route_of("data:image/png;base64,AAAA"),
            UriRoute::Unsupported(_)
        ));
    }

    #[test]
    fn a_percent_that_is_not_an_escape_survives() {
        // `100%` in a folder name is legal and must not be eaten.
        assert_eq!(percent_decode("100%_done/a.png"), "100%_done/a.png");
        assert_eq!(percent_decode("a%zzb"), "a%zzb");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("a%2Fb"), "a/b");
    }

    #[cfg(feature = "std")]
    #[test]
    fn a_missing_local_file_names_itself_in_the_error() {
        let err = fetch_uri("/definitely/not/here/cover.png").unwrap_err();
        match err {
            FetchError::Io(msg) => assert!(
                msg.contains("/definitely/not/here/cover.png"),
                "the error must name the path, got {msg}"
            ),
            other => panic!("expected an Io error, got {other:?}"),
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn a_local_file_round_trips() {
        let dir = std::env::temp_dir().join("azul_fetch_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("cover bytes.bin");
        std::fs::write(&path, b"hello").expect("write");

        assert_eq!(fetch_uri(path.to_str().expect("utf8")).unwrap(), b"hello");
        // ...and through a percent-encoded file URL, which is the whole point
        // of the decode step.
        let url = alloc::format!("file://{}", path.display().to_string().replace(' ', "%20"));
        assert_eq!(fetch_uri(&url).unwrap(), b"hello");

        let _ = std::fs::remove_file(&path);
    }
}
