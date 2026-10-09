//! The URL policy of a payment page (CHECKOUT-PLAN §3.7): a web view has no address bar the user
//! can trust, so every main-frame navigation is read strictly and matched against the origins
//! of the provider the page belongs to.
//!
//! [`WebUrl`] reads an `http` / `https` URL whose authority is a plain host - ASCII, compared in
//! lower case, no userinfo (`https://checkout.stripe.com@evil.example`), no backslash (which a
//! browser reads as `/`), no percent escape, no trailing dot, no empty label - and a real port
//! or none (the scheme's default port is none). Anything else is [`Refused`]: a strict reader
//! refuses what a lenient one would guess at.
//!
//! [`Origin`] is a rule a URL may match: [`Origin::Exact`] (`https`, that host, the default
//! port), [`Origin::Suffix`] (`https`, any subdomain of it - never the apex itself, never a
//! look-alike such as `evilstripe.com`), and the two loopback rules of the fake providers
//! ([`Origin::Loopback`], [`Origin::LoopbackHost`]: `http` or `https` to this computer, any
//! port). The token server writes the same rules as patterns (`checkout.stripe.com`,
//! `.stripe.com`, `loopback`, `localhost`): [`Origin::parse`] reads one, and [`Origin::covers`]
//! says whether a registry rule allows at least as much - what a server's pattern needs to be
//! kept.

use std::{borrow::Cow, fmt};

/// Why a text is no URL a payment page may go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Nothing at all.
    Empty,
    /// White space, a control character, a backslash or a character outside ASCII.
    Characters,
    /// Not `http` / `https`: the scheme (empty when there is none).
    NotWeb(String),
    /// A user name or password before the host.
    Userinfo,
    /// The host is empty, or not a plain lower-case ASCII name or address.
    Host,
    /// The port is not a number from 1 to 65535.
    Port,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::Empty => f.write_str("there is no address"),
            Refused::Characters => f.write_str(
                "the address holds white space, a backslash or characters outside ASCII",
            ),
            Refused::NotWeb(scheme) if scheme.is_empty() => {
                f.write_str("it is no web address (http or https)")
            }
            Refused::NotWeb(scheme) => write!(f, "a {scheme}: address is no web page"),
            Refused::Userinfo => f.write_str("the address names a user before its host"),
            Refused::Host => f.write_str("the address's host is no plain host name"),
            Refused::Port => f.write_str("the address's port is no port"),
        }
    }
}

impl std::error::Error for Refused {}

/// An `http` / `https` URL as the payment policy reads it. Its query and fragment may carry a
/// client secret: `Debug` shows the scheme, the host and the port only.
#[derive(Clone, PartialEq, Eq)]
pub struct WebUrl {
    https: bool,
    host: String,
    port: Option<u16>,
    path: String,
    query: String,
    fragment: Option<String>,
}

impl fmt::Debug for WebUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rest = if self.path == "/" && self.query.is_empty() && self.fragment.is_none() {
            ""
        } else {
            "/..."
        };
        write!(f, "WebUrl({}{rest})", self.origin_text())
    }
}

impl WebUrl {
    /// Reads `text` (see the module docs).
    ///
    /// # Errors
    ///
    /// [`Refused`]: why it is no URL a payment page may go to.
    pub fn parse(text: &str) -> Result<WebUrl, Refused> {
        if text.is_empty() {
            return Err(Refused::Empty);
        }
        if text
            .bytes()
            .any(|b| b <= b' ' || b >= 0x7f || b == b'\\')
        {
            return Err(Refused::Characters);
        }
        let Some((scheme, rest)) = split_scheme(text) else {
            return Err(Refused::NotWeb(String::new()));
        };
        let https = match scheme.as_str() {
            "https" => true,
            "http" => false,
            _ => return Err(Refused::NotWeb(scheme)),
        };
        let Some(rest) = rest.strip_prefix("//") else {
            return Err(Refused::NotWeb(scheme));
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        if authority.contains('@') {
            return Err(Refused::Userinfo);
        }
        let (host, port) = split_port(authority)?;
        let host = host.to_ascii_lowercase();
        if !is_plain_host(&host) {
            return Err(Refused::Host);
        }
        let port = match (port, https) {
            (Some(443), true) | (Some(80), false) => None,
            (port, _) => port,
        };
        let (before_fragment, fragment) = match tail.split_once('#') {
            Some((before, fragment)) => (before, Some(fragment.to_string())),
            None => (tail, None),
        };
        let (path, query) = before_fragment
            .split_once('?')
            .unwrap_or((before_fragment, ""));
        Ok(WebUrl {
            https,
            host,
            port,
            path: if path.is_empty() {
                String::from("/")
            } else {
                path.to_string()
            },
            query: query.to_string(),
            fragment,
        })
    }

    /// `https` (else `http`).
    #[must_use]
    pub fn https(&self) -> bool {
        self.https
    }

    /// The host, in lower case (an IPv6 address in its brackets).
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port, when it is not the scheme's default.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// The path (`/` when the URL has none).
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The query, without its `?` (empty when there is none).
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The fragment, without its `#` (empty when there is none).
    #[must_use]
    pub fn fragment(&self) -> &str {
        self.fragment.as_deref().unwrap_or("")
    }

    /// `https://checkout.stripe.com`, `http://127.0.0.1:8081`.
    #[must_use]
    pub fn origin_text(&self) -> String {
        let scheme = if self.https { "https" } else { "http" };
        match self.port {
            Some(port) => format!("{scheme}://{}:{port}", self.host),
            None => format!("{scheme}://{}", self.host),
        }
    }

    /// The host as a notice shows it: with its port when it has one (`127.0.0.1:8081`).
    #[must_use]
    pub fn shown(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        }
    }

    /// The URL without its fragment. A secret: never print it.
    #[must_use]
    pub fn without_fragment(&self) -> String {
        let mut text = self.origin_text();
        text.push_str(&self.path);
        if !self.query.is_empty() {
            text.push('?');
            text.push_str(&self.query);
        }
        text
    }

    /// The whole URL. A secret: never print it.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = self.without_fragment();
        if let Some(fragment) = &self.fragment {
            text.push('#');
            text.push_str(fragment);
        }
        text
    }

    /// The same page with the fragment `fragment` (in place of its own).
    #[must_use]
    pub fn with_fragment(&self, fragment: &str) -> WebUrl {
        WebUrl {
            fragment: Some(fragment.to_string()),
            ..self.clone()
        }
    }

    /// The same page without a fragment.
    #[must_use]
    pub fn clear_fragment(&self) -> WebUrl {
        WebUrl {
            fragment: None,
            ..self.clone()
        }
    }

    /// The same document as `other`: everything but the fragment is equal.
    #[must_use]
    pub fn same_document(&self, other: &WebUrl) -> bool {
        self.https == other.https
            && self.host == other.host
            && self.port == other.port
            && self.path == other.path
            && self.query == other.query
    }
}

/// The scheme of `text` in lower case and what follows its `:`; `None` without a scheme.
fn split_scheme(text: &str) -> Option<(String, &str)> {
    let (scheme, rest) = text.split_once(':')?;
    let mut bytes = scheme.bytes();
    let first = bytes.next()?;
    if !first.is_ascii_alphabetic()
        || !bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), rest))
}

/// The host and the port of an authority (no userinfo).
fn split_port(authority: &str) -> Result<(&str, Option<u16>), Refused> {
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']').ok_or(Refused::Host)?;
        let (host, after) = authority.split_at(end + 1);
        match after.strip_prefix(':') {
            Some(port) => (host, Some(port)),
            None if after.is_empty() => (host, None),
            None => return Err(Refused::Host),
        }
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let port = match port {
        None => None,
        Some(digits) => {
            if digits.is_empty() || digits.len() > 5 || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(Refused::Port);
            }
            match digits.parse::<u32>() {
                Ok(n) if (1..=65535).contains(&n) => Some(n as u16),
                _ => return Err(Refused::Port),
            }
        }
    };
    Ok((host, port))
}

/// A lower-case host name (labels of letters, digits and inner hyphens, 63 bytes at most each,
/// 253 in all), a dotted IPv4 address, or an IPv6 address in brackets.
fn is_plain_host(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[') {
        return inner.strip_suffix(']').is_some_and(|ip| {
            !ip.is_empty() && ip.bytes().all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
        });
    }
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

/// Whether `host` (lower case, as a [`WebUrl`] has it) is this computer: `localhost`,
/// `127.x.x.x`, `[::1]`.
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    if host == "localhost" || host == "[::1]" || host == "::1" {
        return true;
    }
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts[0] == "127"
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u8>().is_ok())
}

/// The host of `text` as a notice may show it ("The payment page tried to open <host>"): no
/// userinfo, no path, no query, no fragment, 64 characters at most - for any text, a refused one
/// included.
#[must_use]
pub fn shown_host(text: &str) -> String {
    if let Ok(url) = WebUrl::parse(text) {
        return clip(&url.shown());
    }
    let text = text.trim();
    if let Some((scheme, rest)) = split_scheme(text) {
        if scheme == "http" || scheme == "https" {
            let rest = rest.trim_start_matches(['/', '\\']);
            let end = rest.find(['/', '?', '#', '\\']).unwrap_or(rest.len());
            let authority = &rest[..end];
            let host = authority.rsplit('@').next().unwrap_or_default();
            let shown: String = host.chars().filter(char::is_ascii_graphic).collect();
            if !shown.is_empty() {
                return clip(&shown.to_ascii_lowercase());
            }
        } else {
            return format!("a {}: address", clip(&scheme));
        }
    }
    String::from("an address that is no web address")
}

fn clip(text: &str) -> String {
    const MAX: usize = 64;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    let mut out: String = text.chars().take(MAX - 3).collect();
    out.push_str("...");
    out
}

/// Where a payment page may be: a rule a [`WebUrl`] matches (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// `https://<host>`, the default port.
    Exact(Cow<'static, str>),
    /// `https://*<suffix>`: any host that ends in the suffix (written with its leading dot) and
    /// is longer than it - the subdomains, never the apex.
    Suffix(Cow<'static, str>),
    /// `http` or `https` to this computer, any port (the fake providers only).
    Loopback,
    /// `http` or `https` to this loopback host, any port (a fake provider's own pages, told
    /// apart from another fake's by the name it is reached under).
    LoopbackHost(Cow<'static, str>),
}

impl Origin {
    /// Whether `url` is a page this rule allows.
    #[must_use]
    pub fn matches(&self, url: &WebUrl) -> bool {
        match self {
            Origin::Exact(host) => url.https && url.port.is_none() && url.host == host.as_ref(),
            Origin::Suffix(suffix) => {
                url.https
                    && url.port.is_none()
                    && url.host.len() > suffix.len()
                    && url.host.ends_with(suffix.as_ref())
            }
            Origin::Loopback => is_loopback_host(&url.host),
            Origin::LoopbackHost(host) => {
                url.host == host.as_ref() && is_loopback_host(host.as_ref())
            }
        }
    }

    /// A pattern the token server wrote: `checkout.stripe.com` (one host), `.stripe.com` (its
    /// subdomains), `loopback` (this computer), `localhost` / `127.0.0.1` (that loopback host);
    /// `None` for anything else (a scheme, a path, a wildcard, an empty label).
    #[must_use]
    pub fn parse(pattern: &str) -> Option<Origin> {
        let pattern = pattern.trim().to_ascii_lowercase();
        if pattern == "loopback" {
            return Some(Origin::Loopback);
        }
        if let Some(rest) = pattern.strip_prefix('.') {
            return (is_plain_host(rest) && !rest.starts_with('['))
                .then(|| Origin::Suffix(Cow::Owned(pattern.clone())));
        }
        if !is_plain_host(&pattern) {
            return None;
        }
        if is_loopback_host(&pattern) {
            return Some(Origin::LoopbackHost(Cow::Owned(pattern)));
        }
        Some(Origin::Exact(Cow::Owned(pattern)))
    }

    /// Whether this rule allows everything `other` allows (a server's pattern is kept only
    /// when a registry rule covers it).
    #[must_use]
    pub fn covers(&self, other: &Origin) -> bool {
        match (self, other) {
            (Origin::Exact(a), Origin::Exact(b)) => a == b,
            (Origin::Suffix(s), Origin::Exact(h)) => h.len() > s.len() && h.ends_with(s.as_ref()),
            (Origin::Suffix(s), Origin::Suffix(t)) => t.ends_with(s.as_ref()),
            (Origin::Loopback, Origin::Loopback | Origin::LoopbackHost(_)) => true,
            (Origin::Loopback, Origin::Exact(h)) => is_loopback_host(h),
            (Origin::LoopbackHost(a), Origin::LoopbackHost(b)) => a == b,
            _ => false,
        }
    }

    /// A rule of this computer.
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        matches!(self, Origin::Loopback | Origin::LoopbackHost(_))
    }
}
