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

use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use core::fmt;

use azul_css::{impl_result, AzString};

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
    ///
    /// # Errors
    ///
    /// Returns a `UrlParseError` if `s` is not a valid absolute URL.
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
    #[must_use]
    pub fn from_parts(scheme: &str, host: &str, port: u16, path: &str) -> Self {
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
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.href.as_str()
    }

    /// Check if this is an HTTPS URL
    #[must_use]
    pub fn is_https(&self) -> bool {
        self.scheme.as_str() == "https"
    }

    /// Check if this is an HTTP URL
    #[must_use]
    pub fn is_http(&self) -> bool {
        self.scheme.as_str() == "http"
    }

    /// Opens this URL in the system's default browser. `true` once the
    /// opener started (`false` where the platform has none). A headless or
    /// scripted run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`) starts nothing and
    /// answers `true`.
    #[cfg(feature = "std")]
    #[must_use]
    pub fn open(&self) -> bool {
        spawn_opener(self.href.as_str(), false)
    }

    /// Opens a file in its default app, or a folder in the file manager
    /// (`open` on macOS, `xdg-open` on Linux and the BSDs, `explorer` on
    /// Windows; the path is one argument, no shell parses it). `true` once
    /// the opener started (`false` where the platform has none). A headless
    /// or scripted run starts nothing and answers `true`, as [`Self::open`].
    #[cfg(feature = "std")]
    #[must_use]
    pub fn open_path(path: &str) -> bool {
        spawn_opener(path, true)
    }

    /// Get the effective port (using default ports for http/https)
    #[must_use]
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
    ///
    /// # Errors
    ///
    /// Returns a `UrlParseError` if this URL's `href` is not parseable as a
    /// base, or if `path` cannot be resolved against it.
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

/// The program and arguments that hand `target` - a URL, or a file / folder
/// path when `is_path` - to its default handler on `os`
/// (`std::env::consts::OS`), or `None` where there is no opener.
///
/// The target is always ONE argument and no shell runs in between. Windows
/// used `cmd /C start <url>`: cmd re-parses the line, so an `&` (every URL
/// with two query parameters) ended the command, and a quoted first argument
/// became the window title. A URL goes to `rundll32 url.dll,FileProtocolHandler`
/// (the shell's URL handler, no command-line parsing of the URL); a path goes
/// to `explorer`, which opens a file in its default app and a folder in a
/// window. Elsewhere `open` (macOS) and `xdg-open` (Linux and the BSDs) take
/// both.
#[cfg(feature = "std")]
fn opener_command<'a>(
    target: &'a str,
    is_path: bool,
    os: &str,
) -> Option<(&'static str, Vec<&'a str>)> {
    match os {
        "windows" if is_path => Some(("explorer", alloc::vec![target])),
        "windows" => Some((
            "rundll32",
            alloc::vec!["url.dll,FileProtocolHandler", target],
        )),
        "macos" => Some(("open", alloc::vec![target])),
        "linux" | "freebsd" | "openbsd" | "netbsd" | "dragonfly" => {
            Some(("xdg-open", alloc::vec![target]))
        }
        _ => None,
    }
}

/// Whether this run starts no opener: a headless one (`AZ_BACKEND=headless`)
/// or one a test scripts (`AZ_E2E_TEST`), as `var` reads the environment.
///
/// There is no desktop to show a page or a file on, and a test must never
/// pop up the browser of the machine it runs on - the keyring and the
/// biometric prompt have headless stand-ins for the same reason.
#[cfg(feature = "std")]
#[must_use]
fn opens_nothing(var: &dyn Fn(&str) -> Option<String>) -> bool {
    var("AZ_BACKEND").is_some_and(|backend| backend == "headless")
        || var("AZ_E2E_TEST").is_some()
}

/// Spawns [`opener_command`] for this platform; `true` once it started.
///
/// A headless or scripted run ([`opens_nothing`]) starts nothing and answers
/// `true`: the test plays the browser, and the app goes on as it would for
/// its user.
#[cfg(feature = "std")]
fn spawn_opener(target: &str, is_path: bool) -> bool {
    if opens_nothing(&|name| std::env::var(name).ok()) {
        return true;
    }
    opener_command(target, is_path, std::env::consts::OS).is_some_and(|(program, args)| {
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .is_ok()
    })
}

#[cfg(test)]
#[path = "url_test.rs"]
mod url_test;

#[cfg(all(test, feature = "std"))]
mod opener_tests {
    use super::{opener_command, opens_nothing};

    /// The environment `pairs` describe, as `opens_nothing` reads it.
    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        }
    }

    /// A headless or scripted run (an E2E test) starts no browser and no app: an app that
    /// opens a payment page there must not pop it up on the machine running the test.
    #[test]
    fn a_headless_or_scripted_run_starts_no_opener() {
        assert!(opens_nothing(&env(&[("AZ_BACKEND", "headless")])));
        assert!(opens_nothing(&env(&[("AZ_E2E_TEST", "/tmp/scenario.json")])));
        assert!(!opens_nothing(&env(&[])));
        assert!(!opens_nothing(&env(&[("AZ_BACKEND", "metal")])));
    }

    /// `cmd /C start <url>` re-parsed the line: an `&` (every URL with two
    /// query parameters) ended the command there, and a quoted first
    /// argument became the window title. The URL must reach the opener as
    /// ONE argument that no shell parses again.
    #[test]
    fn the_windows_open_command_keeps_an_ampersand_url_whole() {
        let url = "https://example.com/search?q=a&lang=en";
        let (program, args) = opener_command(url, false, "windows").expect("an opener");
        assert_ne!(program, "cmd", "cmd splits the URL at its `&`: {args:?}");
        assert_eq!(args.last(), Some(&url));
        assert_eq!(args.iter().filter(|a| a.contains("example.com")).count(), 1);
    }

    /// A file or folder opens in its default app on every desktop, the path
    /// whole as one argument (a space in it included).
    #[test]
    fn a_file_path_opens_in_its_default_app_on_every_desktop() {
        let path = r"C:\Users\me\My Documents\report & summary.pdf";
        assert_eq!(
            opener_command(path, true, "windows"),
            Some(("explorer", alloc::vec![path]))
        );
        let path = "/home/me/My Documents/report.pdf";
        assert_eq!(
            opener_command(path, true, "linux"),
            Some(("xdg-open", alloc::vec![path]))
        );
        assert_eq!(
            opener_command(path, true, "freebsd"),
            Some(("xdg-open", alloc::vec![path]))
        );
        assert_eq!(
            opener_command(path, true, "macos"),
            Some(("open", alloc::vec![path]))
        );
        assert_eq!(opener_command(path, true, "android"), None);
    }
}
