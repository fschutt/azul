//! Sign-in sessions, the platform-free half: OAuth 2.0 for native apps (RFC 8252) with PKCE
//! (RFC 7636).
//!
//! A cloud drive's sign-in (Google Drive, Dropbox, OneDrive) runs in the system's browser, never
//! in a web view of the app: Google refuses sign-in inside embedded web views, and a user should
//! type a password only where the address bar is the browser's. The app asks the platform for an
//! AUTH SESSION (`AuthSession::start` in azul-dll): it opens the provider's authorize page and
//! answers with the URL the provider sent the browser back to (the redirect, carrying `code` and
//! `state`). What the platform uses:
//!
//! - macOS, iOS: `ASWebAuthenticationSession` for a custom-scheme redirect
//!   (`com.example.app:/oauth2redirect`);
//! - Windows, Linux (and macOS with a loopback redirect): the system browser and a one-shot HTTP
//!   listener on the loopback interface ([`loopback`]) - `http://127.0.0.1:<port>/<path>`;
//! - Android: a Custom Tab and the app's redirect intent;
//! - a headless or E2E run: a fake that answers from `AZ_AUTH_SESSION_REDIRECT`.
//!
//! This module is what every platform shares, and what the app needs around the session:
//!
//! - [`AuthPkce`]: the code verifier, its S256 challenge and the `state` of one sign-in, from
//!   entropy the caller draws (this crate has no randomness source on purpose; azul-dll's
//!   `AuthPkce::create` takes it from the OS);
//! - [`AuthPkce::authorize_url`]: the authorization request (`response_type=code`, the client,
//!   the scope, the challenge, the state);
//! - [`AuthPkce::read_redirect`]: the state checked, then the code or the provider's error;
//! - [`parse_redirect`], [`with_redirect_uri`], [`matches_redirect`], [`finish`]: what the
//!   platform half does with a request and with the URL the browser came back to - the redirect
//!   must match EXACTLY (scheme, host, port, path) and carry the request's state, or the session
//!   fails instead of handing the app someone else's code.
//!
//! Nothing here sends anything: the token request (the code exchanged for tokens) is the app's,
//! over its own HTTP client, to the provider the user picked.

use alloc::string::{String, ToString};
use core::fmt;

use azul_core::{refany::RefAny, webview::url_query_param};
use azul_css::{impl_option, AzString};

/// How long a sign-in waits for its redirect when the request names no time: five minutes.
pub const DEFAULT_TIMEOUT_SECS: u32 = 300;

/// The entropy [`AuthPkce::from_entropy`] needs: 32 bytes for the code verifier, 16 for the
/// state.
pub const PKCE_ENTROPY_LEN: usize = 48;

/// What the platform did with a sign-in.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AuthSessionStatus {
    /// The browser came back to the redirect URI: [`AuthSessionResult::redirect_url`] is the
    /// whole URL it was sent to (its query carries the `code` or the provider's `error`).
    Redirected,
    /// The user closed the sign-in window or sheet first.
    Cancelled,
    /// No redirect came within the request's time.
    TimedOut,
    /// This platform has no sign-in session for this kind of redirect (a custom scheme on
    /// Windows or Linux, a loopback one on a phone, a headless run without a fake).
    Unsupported,
    /// The session could not run or ended wrongly: no browser, the redirect's port taken, a
    /// redirect to another address, a state that is not the request's.
    Failed,
}

/// One sign-in: where the browser goes, and where the provider sends it back.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AuthRequest {
    /// The provider's authorization endpoint with every parameter of the request
    /// ([`AuthPkce::authorize_url`]) except `redirect_uri`: the session adds the one it
    /// listens on (a loopback redirect's port is only known once it listens).
    pub authorize_url: AzString,
    /// Where the provider sends the browser back. A loopback address - `http://127.0.0.1/<path>`
    /// (no port: a free one is picked), `http://127.0.0.1:<port>/<path>`, `localhost` or `[::1]`
    /// instead of `127.0.0.1` - on macOS, Windows and Linux; a custom scheme -
    /// `com.example.app:/oauth2redirect` - on macOS, iOS and Android.
    pub redirect_uri: AzString,
    /// Ask the platform not to share cookies with the user's browser (macOS and iOS:
    /// `prefersEphemeralWebBrowserSession`; Android: an ephemeral Custom Tab where the browser
    /// has them). The system browser of a loopback sign-in is the user's own and ignores it.
    pub prefers_ephemeral: bool,
    /// Seconds to wait for the redirect; `0` waits [`DEFAULT_TIMEOUT_SECS`].
    pub timeout_secs: u32,
}

impl AuthRequest {
    /// A sign-in at `authorize_url` coming back to `redirect_uri`, sharing the browser's
    /// cookies, waiting the default time.
    #[must_use]
    pub const fn create(authorize_url: AzString, redirect_uri: AzString) -> Self {
        Self {
            authorize_url,
            redirect_uri,
            prefers_ephemeral: false,
            timeout_secs: 0,
        }
    }

    /// The same request, with or without the browser's cookies.
    #[must_use]
    pub const fn with_prefers_ephemeral(mut self, prefers_ephemeral: bool) -> Self {
        self.prefers_ephemeral = prefers_ephemeral;
        self
    }

    /// The same request, waiting `timeout_secs` seconds (`0`: the default).
    #[must_use]
    pub const fn with_timeout_secs(mut self, timeout_secs: u32) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// The seconds the session waits: [`Self::timeout_secs`], or the default for `0`.
    #[must_use]
    pub const fn effective_timeout_secs(&self) -> u32 {
        if self.timeout_secs == 0 {
            DEFAULT_TIMEOUT_SECS
        } else {
            self.timeout_secs
        }
    }
}

/// What a sign-in session answers: delivered to the request's `ResumeCallback` as its `result`
/// (read it with [`AuthSessionResult::downcast`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AuthSessionResult {
    pub status: AuthSessionStatus,
    /// The URL the browser came back to (`Redirected`); empty otherwise. Read its code with
    /// [`AuthPkce::read_redirect`]. It carries a one-time code: never log it.
    pub redirect_url: AzString,
    /// The redirect URI the request went out with, a loopback port filled in: the token
    /// request has to repeat it exactly.
    pub redirect_uri: AzString,
    /// Why not, as a sentence (every status but `Redirected`); empty otherwise.
    pub message: AzString,
}

impl_option!(
    AuthSessionResult,
    OptionAuthSessionResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl AuthSessionResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionAuthSessionResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }

    /// The browser came back to `redirect_url` (a request made with `redirect_uri`).
    #[must_use]
    pub fn redirected(redirect_url: &str, redirect_uri: &str) -> Self {
        Self {
            status: AuthSessionStatus::Redirected,
            redirect_url: AzString::from(redirect_url),
            redirect_uri: AzString::from(redirect_uri),
            message: AzString::default(),
        }
    }

    /// A session that ended without a redirect, and why.
    #[must_use]
    pub fn ended(status: AuthSessionStatus, redirect_uri: &str, message: &str) -> Self {
        Self {
            status,
            redirect_url: AzString::default(),
            redirect_uri: AzString::from(redirect_uri),
            message: AzString::from(message),
        }
    }

    /// Whether the browser came back to the redirect URI.
    #[must_use]
    pub const fn is_redirected(&self) -> bool {
        matches!(self.status, AuthSessionStatus::Redirected)
    }
}

/// The PKCE pair and the `state` of ONE sign-in (RFC 7636, S256). Keep it until the redirect
/// is read; send the verifier only to the token endpoint. `Debug` hides the verifier.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AuthPkce {
    /// 43 characters of base64url (32 random bytes): sent with the code to the token endpoint.
    pub code_verifier: AzString,
    /// `BASE64URL(SHA256(code_verifier))`: sent with the authorization request.
    pub code_challenge: AzString,
    /// 22 characters of base64url (16 random bytes): sent with the authorization request, and
    /// the redirect has to bring it back.
    pub state: AzString,
}

impl fmt::Debug for AuthPkce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthPkce")
            .field("code_verifier", &"<hidden>")
            .field("code_challenge", &self.code_challenge.as_str())
            .field("state", &self.state.as_str())
            .finish()
    }
}

impl AuthPkce {
    /// The pair and the state from `entropy` - at least [`PKCE_ENTROPY_LEN`] bytes from the
    /// OS's random source (the first 32 make the verifier, the next 16 the state); `None` with
    /// fewer. The same entropy makes the same values: draw it fresh for every sign-in.
    #[must_use]
    pub fn from_entropy(entropy: &[u8]) -> Option<Self> {
        let verifier_bytes = entropy.get(..32)?;
        let state_bytes = entropy.get(32..PKCE_ENTROPY_LEN)?;
        let code_verifier = base64url(verifier_bytes);
        let code_challenge = code_challenge_s256(&code_verifier);
        Some(Self {
            code_verifier: AzString::from(code_verifier),
            code_challenge: AzString::from(code_challenge),
            state: AzString::from(base64url(state_bytes)),
        })
    }

    /// The authorization request at `endpoint` (the provider's authorize URL; a query it has
    /// already is kept): `response_type=code`, `client_id`, `scope` (left out when empty), the
    /// S256 challenge and the state, each escaped. The session adds `redirect_uri`; provider
    /// extras (`access_type=offline`, `token_access_type=offline`) are appended by the app.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // C API: api.json hands the AzStrings over by value
    pub fn authorize_url(
        &self,
        endpoint: AzString,
        client_id: AzString,
        scope: AzString,
    ) -> AzString {
        let mut url = endpoint.as_str().trim().to_string();
        push_param(&mut url, "response_type", "code");
        push_param(&mut url, "client_id", client_id.as_str().trim());
        let scope = scope.as_str().trim();
        if !scope.is_empty() {
            push_param(&mut url, "scope", scope);
        }
        push_param(&mut url, "code_challenge", self.code_challenge.as_str());
        push_param(&mut url, "code_challenge_method", "S256");
        push_param(&mut url, "state", self.state.as_str());
        AzString::from(url)
    }

    /// The URL the browser came back to, read: its `state` must be this sign-in's (else
    /// [`AuthCodeStatus::StateMismatch`], whatever else it says), then the provider's `error`
    /// (with its `error_description`) or the `code`.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // C API: api.json hands the AzString over by value
    pub fn read_redirect(&self, redirect_url: AzString) -> AuthCode {
        let url = redirect_url.as_str();
        let state = url_query_param(url, "state");
        if !state
            .as_deref()
            .is_some_and(|state| same_secret(state, self.state.as_str()))
        {
            return AuthCode::without(
                AuthCodeStatus::StateMismatch,
                "the redirect does not carry this sign-in's state: it is ignored",
            );
        }
        if let Some(error) = url_query_param(url, "error").filter(|e| !e.trim().is_empty()) {
            let error = error.trim();
            let message =
                match url_query_param(url, "error_description").filter(|d| !d.trim().is_empty()) {
                    Some(description) => format!(
                        "the provider refused the sign-in ({error}): {}",
                        description.trim()
                    ),
                    None => format!("the provider refused the sign-in ({error})"),
                };
            return AuthCode::without(AuthCodeStatus::ProviderError, &message);
        }
        match url_query_param(url, "code").filter(|c| !c.is_empty()) {
            Some(code) => AuthCode {
                status: AuthCodeStatus::Code,
                code: AzString::from(code),
                message: AzString::default(),
            },
            None => AuthCode::without(
                AuthCodeStatus::NoCode,
                "the redirect carries no authorization code",
            ),
        }
    }
}

/// `BASE64URL(SHA256(verifier))` without padding: the S256 code challenge of RFC 7636.
#[must_use]
pub fn code_challenge_s256(verifier: &str) -> String {
    use sha2::{Digest as _, Sha256};
    base64url(&Sha256::digest(verifier.as_bytes()))
}

/// `bytes` as base64url without padding (RFC 4648 section 5).
fn base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Whether two secrets are equal, compared in a time that does not depend on where they
/// differ (a state is compared with what a page sent).
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Appends `name=value` (the value escaped) to `url`'s query.
fn push_param(url: &mut String, name: &str, value: &str) {
    if !(url.ends_with('?') || url.ends_with('&')) {
        url.push(if url.contains('?') { '&' } else { '?' });
    }
    url.push_str(name);
    url.push('=');
    url.push_str(&percent_encode(value));
}

/// What a redirect said.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AuthCodeStatus {
    /// An authorization code: [`AuthCode::code`].
    Code,
    /// The provider refused (the user denied access, the client is not allowed, ...):
    /// [`AuthCode::message`] says what it said.
    ProviderError,
    /// The redirect's `state` is not this sign-in's (or it has none): it is ignored.
    StateMismatch,
    /// The redirect has neither a code nor an error.
    NoCode,
}

/// A redirect, read ([`AuthPkce::read_redirect`]). `Debug` hides the code.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AuthCode {
    pub status: AuthCodeStatus,
    /// The authorization code (`Code`); empty otherwise. One-time and short-lived: exchange it
    /// at once, never log it.
    pub code: AzString,
    /// Why there is no code, as a sentence; empty with a code.
    pub message: AzString,
}

impl fmt::Debug for AuthCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthCode")
            .field("status", &self.status)
            .field(
                "code",
                &if self.code.as_str().is_empty() {
                    ""
                } else {
                    "<hidden>"
                },
            )
            .field("message", &self.message.as_str())
            .finish()
    }
}

impl AuthCode {
    /// Whether the redirect brought a code.
    #[must_use]
    pub const fn is_code(&self) -> bool {
        matches!(self.status, AuthCodeStatus::Code)
    }

    /// A redirect without a code, and why.
    fn without(status: AuthCodeStatus, message: &str) -> Self {
        Self {
            status,
            code: AzString::default(),
            message: AzString::from(message),
        }
    }
}

// ==== What the platform half does with a request ====

/// Where a request's redirect goes ([`parse_redirect`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redirect {
    /// An `http` address on the loopback interface: `host` as written (`127.0.0.1`,
    /// `localhost`, `[::1]`), `port` when it names one (else a free one is picked), `path` with
    /// its leading `/`.
    Loopback {
        host: String,
        port: Option<u16>,
        path: String,
    },
    /// A custom scheme the platform hands back to the app (`com.example.app`), lowercase.
    CustomScheme { scheme: String },
}

impl Redirect {
    /// A loopback redirect's URI with `port`; a custom scheme's is the request's own.
    #[must_use]
    pub fn uri_with_port(&self, port: u16) -> Option<String> {
        match self {
            Self::Loopback { host, path, .. } => Some(format!("http://{host}:{port}{path}")),
            Self::CustomScheme { .. } => None,
        }
    }
}

/// What kind of redirect `redirect_uri` is - or why a sign-in cannot come back to it: an
/// `https` or `http` address on another host (only the provider's own web apps can use one),
/// a loopback address with a query or a fragment, a scheme that is no scheme.
pub fn parse_redirect(redirect_uri: &str) -> Result<Redirect, String> {
    let uri = redirect_uri.trim();
    let Some((scheme, rest)) = uri.split_once(':') else {
        return Err(format!("{uri:?} is no redirect URI: it has no scheme"));
    };
    if !is_scheme(scheme) {
        return Err(format!(
            "{uri:?} is no redirect URI: {scheme:?} is no scheme"
        ));
    }
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Ok(Redirect::CustomScheme { scheme });
    }
    let Some(rest) = rest.strip_prefix("//") else {
        return Err(format!("{uri:?} is no web address"));
    };
    if rest.contains(['?', '#']) {
        return Err(format!(
            "a loopback redirect URI has no query and no fragment: {uri:?}"
        ));
    }
    let (authority, path) = rest
        .find('/')
        .map_or((rest, "/"), |slash| (&rest[..slash], &rest[slash..]));
    let (host, port) = split_host_port(authority)
        .ok_or_else(|| format!("{uri:?} has no host and port a browser can reach"))?;
    let host = host.to_ascii_lowercase();
    if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]") {
        return Err(format!(
            "a sign-in comes back to this computer (http://127.0.0.1/...) or to a custom \
             scheme, not to {host}"
        ));
    }
    if scheme != "http" {
        return Err(format!(
            "a loopback redirect is plain http, not {scheme}: nothing on this computer has a \
             certificate for {host}"
        ));
    }
    Ok(Redirect::Loopback {
        host,
        port,
        path: path.to_string(),
    })
}

/// Whether `scheme` is one (RFC 3986: a letter, then letters, digits, `+`, `-`, `.`).
fn is_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// An authority's host (an IPv6 one in its brackets) and port (`None` for none, or `0`);
/// `None` when it is no host and port (user info, an empty host, a port that is no port).
fn split_host_port(authority: &str) -> Option<(&str, Option<u16>)> {
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if authority.starts_with('[') {
        let close = authority.find(']')?;
        let (host, after) = authority.split_at(close + 1);
        if after.is_empty() {
            (host, None)
        } else {
            (host, Some(after.strip_prefix(':')?))
        }
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return None;
    }
    let port = match port {
        None => None,
        Some(digits) => Some(digits.parse::<u16>().ok()?).filter(|&p| p != 0),
    };
    Some((host, port))
}

/// `authorize_url` with `redirect_uri` added to its query - refused when it names one already
/// (the session adds the address it listens on; a second one would be the provider's choice).
pub fn with_redirect_uri(authorize_url: &str, redirect_uri: &str) -> Result<String, String> {
    if url_query_param(authorize_url, "redirect_uri").is_some() {
        return Err(String::from(
            "the authorize URL names a redirect_uri already: the session adds the address it \
             listens on",
        ));
    }
    let mut url = authorize_url.trim().to_string();
    push_param(&mut url, "redirect_uri", redirect_uri.trim());
    Ok(url)
}

/// The `state` parameter of an authorization request, if it has one.
#[must_use]
pub fn state_of(authorize_url: &str) -> Option<String> {
    url_query_param(authorize_url, "state").filter(|state| !state.is_empty())
}

/// Whether `url` (where the browser came back to) is `redirect_uri` exactly: the same scheme
/// and host (any case), the same port, the same path (an empty one is `/`); its query and
/// fragment do not count.
#[must_use]
pub fn matches_redirect(url: &str, redirect_uri: &str) -> bool {
    match (redirect_base(url), redirect_base(redirect_uri)) {
        (Some(came), Some(expected)) => came == expected,
        _ => false,
    }
}

/// `url` without its query and fragment.
fn without_query(url: &str) -> &str {
    url.find(['?', '#']).map_or(url, |end| &url[..end]).trim()
}

/// `url` as a redirect is compared: without its query and fragment, the scheme and the
/// authority in lowercase, an empty web path as `/`. `None` for no URL.
fn redirect_base(url: &str) -> Option<String> {
    let (scheme, rest) = without_query(url).split_once(':')?;
    if !is_scheme(scheme) {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    let Some(rest) = rest.strip_prefix("//") else {
        return Some(format!("{scheme}:{rest}"));
    };
    let (authority, path) = rest
        .find('/')
        .map_or((rest, ""), |slash| (&rest[..slash], &rest[slash..]));
    let path = if path.is_empty() && (scheme == "http" || scheme == "https") {
        "/"
    } else {
        path
    };
    Some(format!(
        "{scheme}://{}{path}",
        authority.to_ascii_lowercase()
    ))
}

/// The session's answer for the URL the browser came back to: `Redirected` when it is the
/// request's redirect URI exactly and carries the request's state (the one of
/// `authorize_url`; no check when the request has none), `Failed` - and the URL dropped - when
/// not.
#[must_use]
pub fn finish(url: &str, redirect_uri: &str, authorize_url: &str) -> AuthSessionResult {
    if !matches_redirect(url, redirect_uri) {
        return AuthSessionResult::ended(
            AuthSessionStatus::Failed,
            redirect_uri,
            &format!(
                "the browser came back to {} instead of {redirect_uri}",
                without_query(url)
            ),
        );
    }
    if let Some(expected) = state_of(authorize_url) {
        let came = url_query_param(url, "state");
        if !came
            .as_deref()
            .is_some_and(|state| same_secret(state, &expected))
        {
            return AuthSessionResult::ended(
                AuthSessionStatus::Failed,
                redirect_uri,
                "the redirect does not carry this sign-in's state: it is ignored",
            );
        }
    }
    AuthSessionResult::redirected(url, redirect_uri)
}

/// `component` escaped for a URL's query: the unreserved characters of RFC 3986 (`A-Z a-z 0-9
/// - . _ ~`) as they are, every other byte as `%XX`.
#[must_use]
pub fn percent_encode(component: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(component.len());
    for &byte in component.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
    out
}

/// The one-shot HTTP listener of a loopback redirect (RFC 8252 section 7.3): bound to the
/// loopback interface only, ONE request taken - the first complete one - and answered with a
/// small page, then closed.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
pub mod loopback {
    use std::{
        io::{ErrorKind, Read, Write},
        net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
        time::{Duration, Instant},
    };

    use super::{finish, AuthSessionResult, AuthSessionStatus, Redirect};

    /// How long a connection may take to send its request before it is dropped (and does not
    /// count as THE request: a browser's preconnect sends nothing).
    pub const IDLE_SECS: u64 = 5;

    /// The longest request head read: a redirect's request line and its headers.
    const MAX_HEAD: usize = 16 * 1024;

    /// How often the listener looks for a connection.
    const POLL: Duration = Duration::from_millis(25);

    /// The page of the redirect the app waited for (whatever the provider answered: the app
    /// says what happened).
    const PAGE_DONE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
        <title>Back to the app</title></head><body style=\"font-family: sans-serif; margin: \
        3em; line-height: 1.5;\"><h1>You can close this tab</h1><p>The app has the answer of \
        the sign-in.</p></body></html>";

    /// The page of any other request: the sign-in did not finish.
    const PAGE_FAILED: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
        <title>Sign-in not finished</title></head><body style=\"font-family: sans-serif; \
        margin: 3em; line-height: 1.5;\"><h1>The sign-in did not finish</h1><p>This is not the \
        answer the app waited for. You can close this tab and sign in again from the app.</p>\
        </body></html>";

    /// A bound listener, waiting to be [`Loopback::wait`]ed on.
    #[derive(Debug)]
    pub struct Loopback {
        /// One listener; two for `localhost` (127.0.0.1 and ::1, the same port), which a
        /// browser may resolve to either.
        listeners: Vec<TcpListener>,
        redirect_uri: String,
        /// `http://<host>:<port>`: what a request's target is appended to.
        origin: String,
        port: u16,
    }

    impl Loopback {
        /// Listens for `redirect` (a [`Redirect::Loopback`]) on the loopback interface: on its
        /// port, or a free one the system picks when it names none.
        pub fn bind(redirect: &Redirect) -> Result<Self, String> {
            let Redirect::Loopback { host, port, .. } = redirect else {
                return Err(String::from(
                    "a custom-scheme redirect is not answered on this computer's loopback \
                     interface",
                ));
            };
            let wanted = port.unwrap_or(0);
            let first_ip: IpAddr = if host == "[::1]" {
                Ipv6Addr::LOCALHOST.into()
            } else {
                Ipv4Addr::LOCALHOST.into()
            };
            let first = TcpListener::bind(SocketAddr::new(first_ip, wanted)).map_err(|e| {
                if wanted == 0 {
                    format!("no port on this computer could be opened for the sign-in: {e}")
                } else {
                    format!(
                        "the sign-in's port {wanted} cannot be opened (another program may use \
                         it): {e}"
                    )
                }
            })?;
            let bound = first
                .local_addr()
                .map_err(|e| format!("the sign-in's listener has no address: {e}"))?
                .port();
            let mut listeners = vec![first];
            if host == "localhost" {
                if let Ok(second) =
                    TcpListener::bind(SocketAddr::new(Ipv6Addr::LOCALHOST.into(), bound))
                {
                    listeners.push(second);
                }
            }
            for listener in &listeners {
                listener
                    .set_nonblocking(true)
                    .map_err(|e| format!("the sign-in's listener cannot wait: {e}"))?;
            }
            let redirect_uri = redirect
                .uri_with_port(bound)
                .ok_or_else(|| String::from("not a loopback redirect"))?;
            Ok(Self {
                listeners,
                redirect_uri,
                origin: format!("http://{host}:{bound}"),
                port: bound,
            })
        }

        /// The redirect URI with the port it listens on.
        #[must_use]
        pub fn redirect_uri(&self) -> &str {
            &self.redirect_uri
        }

        /// The port it listens on.
        #[must_use]
        pub const fn port(&self) -> u16 {
            self.port
        }

        /// Waits until `deadline` for ONE request, answers it and closes: `Redirected` for a
        /// GET of the redirect URI with the state of `authorize_url`, `Failed` for anything
        /// else (answered with a page that says so), `TimedOut` without a request.
        #[must_use]
        pub fn wait(self, deadline: Instant, authorize_url: &str) -> AuthSessionResult {
            self.wait_with(deadline, Duration::from_secs(IDLE_SECS), authorize_url)
        }

        /// [`Self::wait`] with a connection's idle time `idle`.
        #[must_use]
        pub fn wait_with(
            self,
            deadline: Instant,
            idle: Duration,
            authorize_url: &str,
        ) -> AuthSessionResult {
            loop {
                for listener in &self.listeners {
                    match listener.accept() {
                        Ok((stream, peer)) => {
                            if !peer.ip().is_loopback() {
                                continue;
                            }
                            if let Some(result) = self.serve(stream, idle, authorize_url) {
                                return result;
                            }
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                ErrorKind::WouldBlock | ErrorKind::Interrupted
                            ) => {}
                        Err(e) => {
                            return AuthSessionResult::ended(
                                AuthSessionStatus::Failed,
                                &self.redirect_uri,
                                &format!("the sign-in's listener failed: {e}"),
                            );
                        }
                    }
                }
                if Instant::now() >= deadline {
                    return AuthSessionResult::ended(
                        AuthSessionStatus::TimedOut,
                        &self.redirect_uri,
                        "the sign-in did not come back in time",
                    );
                }
                std::thread::sleep(POLL);
            }
        }

        /// One connection: `None` when it sent no complete request within `idle` (it does not
        /// count), else the session's answer - the request answered with its page.
        fn serve(
            &self,
            mut stream: TcpStream,
            idle: Duration,
            authorize_url: &str,
        ) -> Option<AuthSessionResult> {
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(idle));
            let _ = stream.set_write_timeout(Some(idle));
            let head = read_head(&mut stream, Instant::now() + idle)?;
            let request_line = head.lines().next().unwrap_or_default();
            let mut parts = request_line.split(' ');
            let method = parts.next().unwrap_or_default();
            let target = parts.next().unwrap_or_default();
            let result = if method != "GET" {
                let method: String = method.chars().take(16).collect();
                AuthSessionResult::ended(
                    AuthSessionStatus::Failed,
                    &self.redirect_uri,
                    &format!("the browser sent a {method} request, not the sign-in's redirect"),
                )
            } else if !target.starts_with('/') {
                AuthSessionResult::ended(
                    AuthSessionStatus::Failed,
                    &self.redirect_uri,
                    "the browser's request names no path",
                )
            } else {
                finish(
                    &format!("{}{target}", self.origin),
                    &self.redirect_uri,
                    authorize_url,
                )
            };
            respond(&mut stream, result.is_redirected());
            Some(result)
        }
    }

    /// A request's head (the request line and the headers), read until its empty line;
    /// `None` when the connection ends, stays silent until `until` or sends more than a head.
    fn read_head(stream: &mut TcpStream, until: Instant) -> Option<String> {
        let mut head: Vec<u8> = Vec::with_capacity(1024);
        let mut buf = [0_u8; 1024];
        loop {
            if head.windows(4).any(|w| w == b"\r\n\r\n") || head.windows(2).any(|w| w == b"\n\n") {
                return Some(String::from_utf8_lossy(&head).into_owned());
            }
            if head.len() >= MAX_HEAD || Instant::now() >= until {
                return None;
            }
            match stream.read(&mut buf) {
                Ok(0) => return None,
                Ok(n) => head.extend_from_slice(buf.get(..n)?),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
    }

    /// Answers the request with the page for `done` (the redirect) or the one for anything
    /// else, and closes the connection.
    fn respond(stream: &mut TcpStream, done: bool) {
        let (status, page) = if done {
            ("200 OK", PAGE_DONE)
        } else {
            ("400 Bad Request", PAGE_FAILED)
        };
        let answer = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: \
             {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: \
             close\r\n\r\n{page}",
            page.len()
        );
        let _ = stream.write_all(answer.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Write);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7636 appendix B: the verifier's 32 octets.
    const RFC_VERIFIER_BYTES: [u8; 32] = [
        116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212, 37,
        77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
    ];
    const RFC_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const RFC_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    fn entropy(first: u8) -> Vec<u8> {
        (0..PKCE_ENTROPY_LEN as u8)
            .map(|i| first.wrapping_add(i.wrapping_mul(7)))
            .collect()
    }

    fn pkce() -> AuthPkce {
        AuthPkce::from_entropy(&entropy(1)).expect("48 bytes are enough")
    }

    fn is_base64url(text: &str) -> bool {
        text.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }

    // ---- PKCE ----

    #[test]
    fn the_code_challenge_of_rfc_7636s_example_verifier_is_its_s256_challenge() {
        assert_eq!(code_challenge_s256(RFC_VERIFIER), RFC_CHALLENGE);
    }

    #[test]
    fn rfc_7636s_example_octets_make_its_example_verifier_and_challenge() {
        let mut bytes = RFC_VERIFIER_BYTES.to_vec();
        bytes.extend_from_slice(&[9; 16]);
        let pkce = AuthPkce::from_entropy(&bytes).unwrap();
        assert_eq!(pkce.code_verifier.as_str(), RFC_VERIFIER);
        assert_eq!(pkce.code_challenge.as_str(), RFC_CHALLENGE);
    }

    #[test]
    fn pkce_makes_a_43_character_verifier_and_a_22_character_state_of_base64url() {
        let pkce = pkce();
        assert_eq!(pkce.code_verifier.as_str().len(), 43);
        assert_eq!(pkce.state.as_str().len(), 22);
        assert_eq!(pkce.code_challenge.as_str().len(), 43);
        for text in [&pkce.code_verifier, &pkce.state, &pkce.code_challenge] {
            assert!(
                is_base64url(text.as_str()),
                "{} is base64url",
                text.as_str()
            );
        }
        assert_eq!(
            pkce.code_challenge.as_str(),
            code_challenge_s256(pkce.code_verifier.as_str())
        );
    }

    #[test]
    fn pkce_needs_48_bytes_of_entropy() {
        assert!(AuthPkce::from_entropy(&[7; PKCE_ENTROPY_LEN - 1]).is_none());
        assert!(AuthPkce::from_entropy(&[]).is_none());
        assert!(AuthPkce::from_entropy(&[7; PKCE_ENTROPY_LEN + 10]).is_some());
    }

    #[test]
    fn two_entropies_make_two_verifiers_and_two_states() {
        let a = AuthPkce::from_entropy(&entropy(1)).unwrap();
        let b = AuthPkce::from_entropy(&entropy(2)).unwrap();
        assert_ne!(a.code_verifier, b.code_verifier);
        assert_ne!(a.state, b.state);
        assert_ne!(a.code_verifier.as_str(), a.state.as_str());
    }

    #[test]
    fn the_debug_output_of_a_pkce_and_a_code_hides_the_secrets() {
        let pkce = pkce();
        let shown = format!("{pkce:?}");
        assert!(!shown.contains(pkce.code_verifier.as_str()), "{shown}");
        let code = AuthCode {
            status: AuthCodeStatus::Code,
            code: AzString::from("4/0AbCdEf-one-time"),
            message: AzString::default(),
        };
        assert!(!format!("{code:?}").contains("one-time"));
    }

    // ---- The authorization request ----

    #[test]
    fn the_authorize_url_carries_the_code_flow_the_client_the_scope_the_s256_challenge_and_the_state(
    ) {
        let pkce = pkce();
        let url = pkce.authorize_url(
            AzString::from("https://accounts.google.com/o/oauth2/v2/auth"),
            AzString::from("123-abc.apps.googleusercontent.com"),
            AzString::from("https://www.googleapis.com/auth/drive openid"),
        );
        let url = url.as_str();
        assert!(
            url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"),
            "{url}"
        );
        let param = |name: &str| azul_core::webview::url_query_param(url, name);
        assert_eq!(param("response_type").as_deref(), Some("code"));
        assert_eq!(
            param("client_id").as_deref(),
            Some("123-abc.apps.googleusercontent.com")
        );
        assert_eq!(
            param("scope").as_deref(),
            Some("https://www.googleapis.com/auth/drive openid")
        );
        assert_eq!(
            param("code_challenge").as_deref(),
            Some(pkce.code_challenge.as_str())
        );
        assert_eq!(param("code_challenge_method").as_deref(), Some("S256"));
        assert_eq!(param("state").as_deref(), Some(pkce.state.as_str()));
        assert_eq!(param("redirect_uri"), None, "the session adds it");
        assert!(
            !url.contains(pkce.code_verifier.as_str()),
            "the verifier stays here"
        );
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive%20openid"));
    }

    #[test]
    fn the_authorize_url_keeps_the_query_an_endpoint_has_and_leaves_an_empty_scope_out() {
        let pkce = pkce();
        let url = pkce.authorize_url(
            AzString::from("https://login.example/authorize?tenant=common"),
            AzString::from("client"),
            AzString::from(""),
        );
        let url = url.as_str();
        assert!(
            url.starts_with("https://login.example/authorize?tenant=common&response_type=code"),
            "{url}"
        );
        assert!(!url.contains("scope="), "{url}");
    }

    #[test]
    fn percent_encoding_keeps_the_unreserved_characters_and_escapes_every_other_byte() {
        assert_eq!(percent_encode("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(
            percent_encode("a b&c=d/e?f#g+h"),
            "a%20b%26c%3Dd%2Fe%3Ff%23g%2Bh"
        );
        assert_eq!(percent_encode("\u{fc}"), "%C3%BC");
        assert_eq!(
            percent_encode("http://127.0.0.1:5000/cb"),
            "http%3A%2F%2F127.0.0.1%3A5000%2Fcb"
        );
    }

    // ---- Reading the redirect ----

    #[test]
    fn a_redirect_with_this_sign_ins_state_and_a_code_reads_as_the_code() {
        let pkce = pkce();
        let url = format!(
            "http://127.0.0.1:53682/callback?state={}&code=4%2F0AbC-x&scope=drive",
            pkce.state.as_str()
        );
        let read = pkce.read_redirect(AzString::from(url));
        assert_eq!(read.status, AuthCodeStatus::Code);
        assert_eq!(read.code.as_str(), "4/0AbC-x");
        assert!(read.is_code());
        assert_eq!(read.message.as_str(), "");
    }

    #[test]
    fn a_redirect_with_another_state_is_a_state_mismatch_even_with_a_code() {
        let pkce = pkce();
        let read = pkce.read_redirect(AzString::from(
            "com.example.app:/oauth2redirect?code=stolen&state=someone-elses",
        ));
        assert_eq!(read.status, AuthCodeStatus::StateMismatch);
        assert_eq!(
            read.code.as_str(),
            "",
            "the code of a foreign redirect is dropped"
        );
        assert!(!read.message.as_str().is_empty());
    }

    #[test]
    fn a_redirect_without_a_state_is_a_state_mismatch() {
        let read = pkce().read_redirect(AzString::from("http://127.0.0.1:5000/cb?code=abc"));
        assert_eq!(read.status, AuthCodeStatus::StateMismatch);
    }

    #[test]
    fn a_provider_error_reads_as_the_error_and_its_description() {
        let pkce = pkce();
        let url = format!(
            "http://127.0.0.1:5000/cb?error=access_denied&error_description=The+user+said+no&state={}",
            pkce.state.as_str()
        );
        let read = pkce.read_redirect(AzString::from(url));
        assert_eq!(read.status, AuthCodeStatus::ProviderError);
        assert_eq!(read.code.as_str(), "");
        let message = read.message.as_str();
        assert!(message.contains("access_denied"), "{message}");
        assert!(message.contains("The user said no"), "{message}");
    }

    #[test]
    fn a_redirect_without_a_code_or_an_error_says_so() {
        let pkce = pkce();
        let url = format!("http://127.0.0.1:5000/cb?state={}", pkce.state.as_str());
        let read = pkce.read_redirect(AzString::from(url));
        assert_eq!(read.status, AuthCodeStatus::NoCode);
        assert!(!read.message.as_str().is_empty());
    }

    // ---- Redirect URIs ----

    #[test]
    fn a_loopback_redirect_without_a_port_asks_for_a_free_one() {
        assert_eq!(
            parse_redirect("http://127.0.0.1/callback"),
            Ok(Redirect::Loopback {
                host: String::from("127.0.0.1"),
                port: None,
                path: String::from("/callback"),
            })
        );
        assert_eq!(
            parse_redirect("http://127.0.0.1").unwrap(),
            Redirect::Loopback {
                host: String::from("127.0.0.1"),
                port: None,
                path: String::from("/"),
            }
        );
    }

    #[test]
    fn a_loopback_redirect_with_a_port_keeps_it_and_one_without_gets_it_filled_in() {
        assert_eq!(
            parse_redirect("http://localhost:53682/").unwrap(),
            Redirect::Loopback {
                host: String::from("localhost"),
                port: Some(53682),
                path: String::from("/"),
            }
        );
        let ipv6 = parse_redirect("http://[::1]/cb").unwrap();
        assert_eq!(
            ipv6.uri_with_port(40000).as_deref(),
            Some("http://[::1]:40000/cb")
        );
        assert_eq!(
            parse_redirect("http://127.0.0.1/cb")
                .unwrap()
                .uri_with_port(5000)
                .as_deref(),
            Some("http://127.0.0.1:5000/cb")
        );
    }

    #[test]
    fn a_web_address_that_is_not_loopback_https_or_a_loopback_with_a_query_is_refused() {
        for uri in [
            "http://example.com/callback",
            "https://127.0.0.1/callback",
            "https://app.example/oauth",
            "http://127.0.0.1:5000/cb?x=1",
            "http://127.0.0.1:5000/cb#frag",
            "http://127.0.0.1:99999/cb",
            "",
            "callback",
            "1app:/x",
        ] {
            assert!(parse_redirect(uri).is_err(), "{uri:?} is refused");
        }
    }

    #[test]
    fn a_custom_scheme_redirect_names_its_scheme_in_lowercase() {
        assert_eq!(
            parse_redirect("com.Example.App:/oauth2redirect"),
            Ok(Redirect::CustomScheme {
                scheme: String::from("com.example.app"),
            })
        );
        assert_eq!(
            parse_redirect("msal1234://auth").unwrap(),
            Redirect::CustomScheme {
                scheme: String::from("msal1234"),
            }
        );
        assert_eq!(
            parse_redirect("com.example.app:/cb")
                .unwrap()
                .uri_with_port(5000),
            None
        );
    }

    #[test]
    fn the_session_adds_the_redirect_uri_to_the_authorize_url_once() {
        assert_eq!(
            with_redirect_uri(
                "https://p.example/auth?client_id=c",
                "http://127.0.0.1:5000/cb"
            )
            .as_deref(),
            Ok("https://p.example/auth?client_id=c&redirect_uri=http%3A%2F%2F127.0.0.1%3A5000%2Fcb")
        );
        assert_eq!(
            with_redirect_uri("https://p.example/auth", "com.example.app:/cb").as_deref(),
            Ok("https://p.example/auth?redirect_uri=com.example.app%3A%2Fcb")
        );
        assert!(with_redirect_uri(
            "https://p.example/auth?redirect_uri=https%3A%2F%2Fevil.example",
            "http://127.0.0.1:5000/cb"
        )
        .is_err());
    }

    #[test]
    fn the_state_of_an_authorize_url_is_read_from_its_query() {
        assert_eq!(
            state_of("https://p.example/auth?client_id=c&state=a-b_c").as_deref(),
            Some("a-b_c")
        );
        assert_eq!(state_of("https://p.example/auth?client_id=c"), None);
    }

    #[test]
    fn the_redirect_must_be_the_redirect_uri_exactly_scheme_host_port_and_path() {
        let uri = "http://127.0.0.1:5000/callback";
        assert!(matches_redirect(
            "http://127.0.0.1:5000/callback?code=x&state=y",
            uri
        ));
        assert!(matches_redirect("HTTP://127.0.0.1:5000/callback#x", uri));
        assert!(!matches_redirect(
            "http://127.0.0.1:5001/callback?code=x",
            uri
        ));
        assert!(!matches_redirect(
            "http://127.0.0.1:5000/callback2?code=x",
            uri
        ));
        assert!(!matches_redirect(
            "http://127.0.0.1:5000/Callback?code=x",
            uri
        ));
        assert!(!matches_redirect(
            "http://localhost:5000/callback?code=x",
            uri
        ));
        assert!(!matches_redirect(
            "https://127.0.0.1:5000/callback?code=x",
            uri
        ));
        assert!(!matches_redirect(
            "http://127.0.0.1:5000/callback/x?code=x",
            uri
        ));
        assert!(matches_redirect(
            "http://127.0.0.1:5000/?code=x",
            "http://127.0.0.1:5000"
        ));
        assert!(matches_redirect(
            "com.example.app:/oauth2redirect?code=x",
            "com.example.app:/oauth2redirect"
        ));
        assert!(!matches_redirect(
            "com.example.app:/other?code=x",
            "com.example.app:/oauth2redirect"
        ));
    }

    #[test]
    fn finish_hands_back_a_matching_redirect_with_the_requests_state() {
        let auth = "https://p.example/auth?client_id=c&state=s123";
        let url = "http://127.0.0.1:5000/cb?code=abc&state=s123";
        let done = finish(url, "http://127.0.0.1:5000/cb", auth);
        assert_eq!(done.status, AuthSessionStatus::Redirected);
        assert_eq!(done.redirect_url.as_str(), url);
        assert_eq!(done.redirect_uri.as_str(), "http://127.0.0.1:5000/cb");
        assert!(done.is_redirected());
    }

    #[test]
    fn finish_fails_a_redirect_to_another_address_or_with_another_state_and_drops_its_url() {
        let auth = "https://p.example/auth?client_id=c&state=s123";
        for url in [
            "http://127.0.0.1:5000/other?code=abc&state=s123",
            "http://127.0.0.1:5000/cb?code=abc&state=s124",
            "http://127.0.0.1:5000/cb?code=abc",
        ] {
            let done = finish(url, "http://127.0.0.1:5000/cb", auth);
            assert_eq!(done.status, AuthSessionStatus::Failed, "{url}");
            assert_eq!(done.redirect_url.as_str(), "", "{url}");
            assert!(
                !done.message.as_str().contains("abc"),
                "no code in the message"
            );
        }
        // A request without a state checks none.
        let done = finish(
            "http://127.0.0.1:5000/cb?code=abc",
            "http://127.0.0.1:5000/cb",
            "https://p.example/auth?client_id=c",
        );
        assert_eq!(done.status, AuthSessionStatus::Redirected);
    }

    #[test]
    fn a_request_waits_five_minutes_unless_it_names_a_time() {
        let request = AuthRequest::create(AzString::from("https://p"), AzString::from("x:/y"));
        assert_eq!(request.effective_timeout_secs(), 300);
        assert_eq!(request.with_timeout_secs(30).effective_timeout_secs(), 30);
    }

    #[test]
    fn a_session_result_survives_the_resume_callbacks_refany() {
        let result =
            AuthSessionResult::redirected("http://127.0.0.1:1/cb?code=x", "http://127.0.0.1:1/cb");
        let back = AuthSessionResult::downcast(RefAny::new(result.clone()));
        assert_eq!(back.into_option(), Some(result));
        assert_eq!(
            AuthSessionResult::downcast(RefAny::new(5_u32)).into_option(),
            None
        );
    }

    // ---- The loopback listener ----

    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    mod listener {
        use std::{
            io::{Read, Write},
            net::TcpStream,
            time::{Duration, Instant},
        };

        use super::super::{loopback::Loopback, parse_redirect, AuthSessionStatus};

        const AUTH: &str = "https://p.example/auth?client_id=c&state=st4te";

        fn bind(uri: &str) -> Loopback {
            Loopback::bind(&parse_redirect(uri).unwrap()).expect("a free loopback port")
        }

        /// Sends `request` to `port` and reads the whole answer.
        fn send(port: u16, request: &str) -> String {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut answer = String::new();
            let _ = stream.read_to_string(&mut answer);
            answer
        }

        fn get(target: &str) -> String {
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nUser-Agent: test\r\n\r\n")
        }

        fn in_secs(secs: u64) -> Instant {
            Instant::now() + Duration::from_secs(secs)
        }

        #[test]
        fn the_listener_picks_a_free_port_and_names_it_in_the_redirect_uri() {
            let listener = bind("http://127.0.0.1/callback");
            assert_ne!(listener.port(), 0);
            assert_eq!(
                listener.redirect_uri(),
                format!("http://127.0.0.1:{}/callback", listener.port())
            );
        }

        #[test]
        fn the_listener_answers_the_redirect_with_a_page_and_returns_its_url() {
            let listener = bind("http://127.0.0.1/callback");
            let port = listener.port();
            let waiting = std::thread::spawn(move || listener.wait(in_secs(10), AUTH));
            let answer = send(port, &get("/callback?code=4%2FAbC&state=st4te"));
            let result = waiting.join().unwrap();
            assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
            assert!(answer.contains("You can close this tab"), "{answer}");
            assert!(
                answer.to_ascii_lowercase().contains("connection: close"),
                "{answer}"
            );
            assert_eq!(result.status, AuthSessionStatus::Redirected);
            assert_eq!(
                result.redirect_url.as_str(),
                format!("http://127.0.0.1:{port}/callback?code=4%2FAbC&state=st4te")
            );
            assert_eq!(
                result.redirect_uri.as_str(),
                format!("http://127.0.0.1:{port}/callback")
            );
        }

        #[test]
        fn the_listener_takes_one_request_only_and_closes() {
            let listener = bind("http://127.0.0.1/cb");
            let port = listener.port();
            let waiting = std::thread::spawn(move || listener.wait(in_secs(10), AUTH));
            let _ = send(port, &get("/cb?code=x&state=st4te"));
            let result = waiting.join().unwrap();
            assert_eq!(result.status, AuthSessionStatus::Redirected);
            assert!(
                TcpStream::connect_timeout(
                    &([127, 0, 0, 1], port).into(),
                    Duration::from_millis(500)
                )
                .is_err(),
                "nothing listens any more"
            );
        }

        #[test]
        fn a_first_request_to_another_path_fails_the_sign_in_and_is_answered_so() {
            let listener = bind("http://127.0.0.1/cb");
            let port = listener.port();
            let waiting = std::thread::spawn(move || listener.wait(in_secs(10), AUTH));
            let answer = send(port, &get("/favicon.ico"));
            let result = waiting.join().unwrap();
            assert!(answer.starts_with("HTTP/1.1 400"), "{answer}");
            assert_eq!(result.status, AuthSessionStatus::Failed);
            assert_eq!(result.redirect_url.as_str(), "");
        }

        #[test]
        fn a_redirect_with_another_state_fails_the_sign_in() {
            let listener = bind("http://127.0.0.1/cb");
            let port = listener.port();
            let waiting = std::thread::spawn(move || listener.wait(in_secs(10), AUTH));
            let answer = send(port, &get("/cb?code=x&state=forged"));
            let result = waiting.join().unwrap();
            assert!(answer.starts_with("HTTP/1.1 400"), "{answer}");
            assert_eq!(result.status, AuthSessionStatus::Failed);
        }

        #[test]
        fn a_post_is_not_the_redirect() {
            let listener = bind("http://127.0.0.1/cb");
            let port = listener.port();
            let waiting = std::thread::spawn(move || listener.wait(in_secs(10), AUTH));
            let _ = send(
                port,
                "POST /cb?code=x&state=st4te HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n",
            );
            assert_eq!(waiting.join().unwrap().status, AuthSessionStatus::Failed);
        }

        #[test]
        fn the_listener_times_out_without_a_request() {
            let listener = bind("http://127.0.0.1/cb");
            let started = Instant::now();
            let result = listener.wait(Instant::now() + Duration::from_millis(300), AUTH);
            assert_eq!(result.status, AuthSessionStatus::TimedOut);
            assert!(started.elapsed() < Duration::from_secs(5));
        }

        #[test]
        fn a_connection_that_sends_nothing_does_not_count_as_the_request() {
            let listener = bind("http://127.0.0.1/cb");
            let port = listener.port();
            let waiting = std::thread::spawn(move || {
                listener.wait_with(in_secs(10), Duration::from_millis(200), AUTH)
            });
            let idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
            std::thread::sleep(Duration::from_millis(500));
            let answer = send(port, &get("/cb?code=x&state=st4te"));
            drop(idle);
            assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
            assert_eq!(
                waiting.join().unwrap().status,
                AuthSessionStatus::Redirected
            );
        }

        #[test]
        fn a_redirect_that_names_its_port_is_refused_while_the_port_is_taken() {
            let first = bind("http://127.0.0.1/cb");
            let taken = format!("http://127.0.0.1:{}/cb", first.port());
            let again = Loopback::bind(&parse_redirect(&taken).unwrap());
            let message = again.expect_err("the port is taken");
            assert!(message.contains(&first.port().to_string()), "{message}");
            drop(first);
        }
    }
}
