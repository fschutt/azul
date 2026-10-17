//! OAuth 2.0 for the consumer clouds OpenDAL reaches (Google Drive, Dropbox, OneDrive): the
//! providers, the token endpoint, and a transport that keeps a drive signed in.
//!
//! - The sign-in itself - the system browser or the platform's sign-in sheet, the redirect,
//!   PKCE and the state - is azul's `AuthSession`; what comes back is an authorization code.
//!   [`exchange_code`] trades it, with the PKCE verifier, for [`Tokens`] at the provider's
//!   token endpoint: one form POST through the app's [`Transport`] (azul's HTTP client and its
//!   TLS in the apps, a scripted fake in the tests - no HTTP stack of its own).
//! - The refresh token goes into the drive's keyring entry (its [`SecretOptions`]:
//!   [`REFRESH_TOKEN`], beside a [`CLIENT_SECRET`] where the provider has one); the client id
//!   into the drive's plain options ([`CLIENT_ID`]), with [`TOKEN_URL`] only when the drive was
//!   signed in at another token endpoint than the provider's (a test's).
//! - [`signed_in`] opens such a drive: OpenDAL gets a placeholder access token, so it refreshes
//!   nothing itself (and a public client without a secret works, which OpenDAL's own refresh
//!   does not allow), and [`RefreshingTransport`] puts the current access token on every
//!   request that carries a bearer token: it refreshes first when it has none, and when the
//!   service answers 401 it refreshes once and sends the request again. A provider that hands
//!   out a new refresh token with each refresh (OneDrive) has the drive's new keyring text
//!   handed to the caller's [`SecretSink`], which stores it.
//!
//! Requests go to the provider the user picked: its token endpoint (or the one the drive was
//! signed in at), and a code or a token only over https - plain http only to this computer
//! ([`is_token_endpoint`]). Nothing here logs a token or puts one in `Debug` output.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{Mutex, PoisonError},
};

use crate::{config::SecretOptions, sigv4::uri_encode, HttpCall, HttpReply, Method, Transport};

/// A provider a drive signs in at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OAuthProvider {
    /// The OpenDAL scheme and the catalog's id: `gdrive`, `dropbox`, `onedrive`.
    pub scheme: &'static str,
    /// Its name in settings: `oauth.<id>` in the shared Azlin config, `AZDRIVE_<ID>_CLIENT_ID`.
    pub id: &'static str,
    /// Its name for people.
    pub name: &'static str,
    /// The authorization endpoint the browser opens.
    pub authorize_url: &'static str,
    /// The token endpoint the code and the refresh token go to.
    pub token_url: &'static str,
    /// The scopes a drive needs (space-separated; empty: the ones the app was registered with).
    pub scope: &'static str,
    /// Parameters the authorization request needs besides the standard ones (a refresh token
    /// at all, a consent page each time so it comes again).
    pub authorize_extras: &'static [(&'static str, &'static str)],
}

/// Google Drive: a refresh token only with `access_type=offline`, and only on a consent page.
pub const GOOGLE: OAuthProvider = OAuthProvider {
    scheme: "gdrive",
    id: "google",
    name: "Google Drive",
    authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    scope: "https://www.googleapis.com/auth/drive",
    authorize_extras: &[("access_type", "offline"), ("prompt", "consent")],
};

/// Dropbox: long-lived refresh tokens with `token_access_type=offline`; the scopes are the
/// app's own (its permissions in the App Console).
pub const DROPBOX: OAuthProvider = OAuthProvider {
    scheme: "dropbox",
    id: "dropbox",
    name: "Dropbox",
    authorize_url: "https://www.dropbox.com/oauth2/authorize",
    token_url: "https://api.dropboxapi.com/oauth2/token",
    scope: "",
    authorize_extras: &[("token_access_type", "offline")],
};

/// OneDrive (Microsoft identity platform, personal and work accounts): `offline_access` for a
/// refresh token, which rotates with every refresh.
pub const ONEDRIVE: OAuthProvider = OAuthProvider {
    scheme: "onedrive",
    id: "onedrive",
    name: "OneDrive",
    authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
    token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
    scope: "offline_access Files.ReadWrite",
    authorize_extras: &[],
};

/// Every provider.
pub const PROVIDERS: [OAuthProvider; 3] = [GOOGLE, DROPBOX, ONEDRIVE];

/// The provider of the OpenDAL scheme `scheme`, if a drive of it signs in.
#[must_use]
pub fn provider(scheme: &str) -> Option<&'static OAuthProvider> {
    PROVIDERS.iter().find(|p| p.scheme == scheme)
}

/// The plain option of a signed-in drive: the app's OAuth client id.
pub const CLIENT_ID: &str = "client_id";
/// The plain option of a drive signed in at another token endpoint than its provider's.
pub const TOKEN_URL: &str = "token_url";
/// The secret setting: the refresh token.
pub const REFRESH_TOKEN: &str = "refresh_token";
/// The secret setting: the client secret, where the provider gives the app one (Google's
/// desktop clients; it is not secret in an app anybody can download, but the token endpoint
/// wants it).
pub const CLIENT_SECRET: &str = "client_secret";
/// OpenDAL's secret setting of a fixed access token.
pub const ACCESS_TOKEN: &str = "access_token";
/// What OpenDAL's signer gets as its access token: [`RefreshingTransport`] puts the real one
/// on the request.
pub const PLACEHOLDER_ACCESS_TOKEN: &str = "azul-storage-refreshing-transport";

/// Where a drive's new keyring text goes when its provider rotated the refresh token (the
/// caller stores it under the drive's keyring entry).
pub type SecretSink = Box<dyn Fn(String) + Send + Sync>;

/// The app's OAuth client at a provider. `Debug` hides the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthClient {
    pub client_id: String,
    /// `None` for a public client (PKCE alone).
    pub client_secret: Option<String>,
}

impl fmt::Debug for OAuthClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthClient")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<hidden>"),
            )
            .finish()
    }
}

impl OAuthClient {
    /// A public client: its id, no secret.
    #[must_use]
    pub fn public(client_id: &str) -> Self {
        OAuthClient {
            client_id: client_id.trim().to_string(),
            client_secret: None,
        }
    }

    /// The same client with `secret` (a blank one is none).
    #[must_use]
    pub fn with_secret(mut self, secret: Option<&str>) -> Self {
        self.client_secret = secret
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        self
    }
}

/// What a token endpoint hands out. `Debug` hides the tokens.
#[derive(Clone, PartialEq, Eq)]
pub struct Tokens {
    pub access_token: String,
    /// `None` when the endpoint gave none (a refresh of a provider that keeps its refresh
    /// tokens).
    pub refresh_token: Option<String>,
    /// Seconds the access token lives, as the endpoint said.
    pub expires_in: Option<u64>,
    /// The scopes granted, as the endpoint said.
    pub scope: Option<String>,
}

impl fmt::Debug for Tokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tokens")
            .field("access_token", &"<hidden>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<hidden>"),
            )
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .finish()
    }
}

/// Why a token request did not bring tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthError {
    /// The endpoint refused: OAuth's `error` code (`invalid_grant`: the code or the refresh
    /// token is no longer good) and its description.
    Rejected {
        status: u16,
        error: String,
        description: String,
    },
    /// The request got no answer (DNS, connection, TLS, a timeout).
    Transport(String),
    /// An answer that is no token response, or an endpoint a token may not go to.
    Protocol(String),
}

impl fmt::Display for OAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OAuthError::Rejected {
                status,
                error,
                description,
            } => {
                let error = if error.is_empty() {
                    format!("HTTP {status}")
                } else {
                    error.clone()
                };
                if description.is_empty() {
                    write!(f, "the provider refused ({error})")
                } else {
                    write!(f, "the provider refused ({error}): {description}")
                }
            }
            OAuthError::Transport(why) => write!(f, "the token endpoint did not answer: {why}"),
            OAuthError::Protocol(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for OAuthError {}

impl OAuthError {
    /// Whether only a new sign-in helps: the grant (the code, the refresh token) or the client
    /// was refused.
    #[must_use]
    pub fn needs_sign_in(&self) -> bool {
        matches!(
            self,
            OAuthError::Rejected { error, .. }
                if matches!(error.as_str(), "invalid_grant" | "invalid_client" | "unauthorized_client")
        )
    }
}

/// Whether a code or a token may be sent to `url`: an https address, or an http one on this
/// computer (`127.0.0.1`, `localhost`, `[::1]`: a test's endpoint).
#[must_use]
pub fn is_token_endpoint(url: &str) -> bool {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://") {
        return !host_of(rest).is_empty();
    }
    lower
        .strip_prefix("http://")
        .is_some_and(|rest| matches!(host_of(rest), "127.0.0.1" | "localhost" | "[::1]"))
}

/// The host of a URL's rest after `scheme://` (an IPv6 one in its brackets); empty for none or
/// for one with user info (`user@host`).
fn host_of(rest: &str) -> &str {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return "";
    }
    if authority.starts_with('[') {
        return authority.find(']').map_or("", |end| &authority[..=end]);
    }
    authority.split(':').next().unwrap_or_default()
}

/// POSTs `form` to `token_url` and reads the token response.
fn post_form(
    transport: &dyn Transport,
    token_url: &str,
    form: &[(&str, &str)],
) -> Result<Tokens, OAuthError> {
    if !is_token_endpoint(token_url) {
        return Err(OAuthError::Protocol(format!(
            "{token_url} is no token endpoint a code or a token may be sent to: it must be https"
        )));
    }
    let body = form
        .iter()
        .map(|(name, value)| format!("{}={}", uri_encode(name, true), uri_encode(value, true)))
        .collect::<Vec<_>>()
        .join("&");
    let call = HttpCall {
        method: Method::Post,
        url: token_url.trim().to_string(),
        headers: vec![(String::from("Accept"), String::from("application/json"))],
        body: body.into_bytes(),
        content_type: String::from("application/x-www-form-urlencoded"),
    };
    let reply = transport.send(&call).map_err(OAuthError::Transport)?;
    token_response(&reply)
}

/// A token endpoint's answer (RFC 6749 sections 5.1 and 5.2).
fn token_response(reply: &HttpReply) -> Result<Tokens, OAuthError> {
    let json: Option<serde_json::Value> = serde_json::from_slice(&reply.body).ok();
    let text = |name: &str| {
        json.as_ref()
            .and_then(|j| j.get(name))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    if !reply.is_success() {
        return Err(OAuthError::Rejected {
            status: reply.status,
            error: text("error").unwrap_or_default(),
            description: text("error_description").unwrap_or_default(),
        });
    }
    if json.is_none() {
        return Err(OAuthError::Protocol(String::from(
            "the token endpoint's answer is no JSON",
        )));
    }
    let access_token = text("access_token").ok_or_else(|| {
        OAuthError::Protocol(String::from(
            "the token endpoint's answer has no access token",
        ))
    })?;
    if let Some(kind) = text("token_type").filter(|t| !t.eq_ignore_ascii_case("bearer")) {
        return Err(OAuthError::Protocol(format!(
            "the token endpoint hands out {kind} tokens, not bearer tokens"
        )));
    }
    let expires_in = json
        .as_ref()
        .and_then(|j| j.get("expires_in"))
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        });
    Ok(Tokens {
        access_token,
        refresh_token: text("refresh_token"),
        expires_in,
        scope: text("scope"),
    })
}

/// Trades the authorization `code` of a sign-in (and its PKCE `code_verifier`, and the exact
/// `redirect_uri` the sign-in went out with) for tokens at `token_url`.
pub fn exchange_code(
    transport: &dyn Transport,
    token_url: &str,
    client: &OAuthClient,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<Tokens, OAuthError> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client.client_id.as_str()),
        ("code_verifier", code_verifier),
    ];
    if let Some(secret) = client.client_secret.as_deref() {
        form.push(("client_secret", secret));
    }
    post_form(transport, token_url, &form)
}

/// A new access token for `refresh_token` at `token_url` (and, with some providers, a new
/// refresh token).
pub fn refresh(
    transport: &dyn Transport,
    token_url: &str,
    client: &OAuthClient,
    refresh_token: &str,
) -> Result<Tokens, OAuthError> {
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client.client_id.as_str()),
    ];
    if let Some(secret) = client.client_secret.as_deref() {
        form.push(("client_secret", secret));
    }
    post_form(transport, token_url, &form)
}

/// The tokens [`RefreshingTransport`] holds.
struct Held {
    access_token: Option<String>,
    refresh_token: String,
}

/// A [`Transport`] that keeps a drive signed in (see the module documentation). `Debug` shows
/// the provider and the endpoint, never a token.
pub struct RefreshingTransport {
    inner: Box<dyn Transport>,
    provider_name: String,
    token_url: String,
    client: OAuthClient,
    held: Mutex<Held>,
    on_new_refresh_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl fmt::Debug for RefreshingTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RefreshingTransport")
            .field("provider", &self.provider_name)
            .field("token_url", &self.token_url)
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

impl RefreshingTransport {
    /// Sends through `inner`, refreshing at `token_url` with `refresh_token` and `client`;
    /// `provider_name` names the provider in its errors.
    #[must_use]
    pub fn new(
        inner: Box<dyn Transport>,
        provider_name: &str,
        token_url: &str,
        client: OAuthClient,
        refresh_token: &str,
    ) -> Self {
        RefreshingTransport {
            inner,
            provider_name: provider_name.to_string(),
            token_url: token_url.to_string(),
            client,
            held: Mutex::new(Held {
                access_token: None,
                refresh_token: refresh_token.to_string(),
            }),
            on_new_refresh_token: None,
        }
    }

    /// Starts with `access_token` (one the sign-in just got) instead of a refresh.
    #[must_use]
    pub fn with_access_token(self, access_token: &str) -> Self {
        if let Ok(mut held) = self.held.lock() {
            held.access_token = Some(access_token.to_string()).filter(|t| !t.is_empty());
        }
        self
    }

    /// Hands every new refresh token the provider rotates to `keep` (to be stored).
    #[must_use]
    pub fn on_new_refresh_token(mut self, keep: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_new_refresh_token = Some(Box::new(keep));
        self
    }

    /// The access token to send: the one held, unless it is `stale` (the one a 401 refused) or
    /// there is none - then a new one from the token endpoint. One refresh at a time: the
    /// others wait for it and take its token.
    fn access_token(&self, stale: Option<&str>) -> Result<String, String> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(token) = held
            .access_token
            .as_ref()
            .filter(|token| stale != Some(token.as_str()))
        {
            return Ok(token.clone());
        }
        let tokens = refresh(
            &*self.inner,
            &self.token_url,
            &self.client,
            &held.refresh_token,
        )
        .map_err(|e| {
            if e.needs_sign_in() {
                format!(
                    "the sign-in to {} is no longer valid ({e}): sign in again",
                    self.provider_name
                )
            } else {
                format!("{} gave no new access token: {e}", self.provider_name)
            }
        })?;
        held.access_token = Some(tokens.access_token.clone());
        if let Some(rotated) = tokens
            .refresh_token
            .filter(|r| !r.is_empty() && *r != held.refresh_token)
        {
            held.refresh_token.clone_from(&rotated);
            if let Some(keep) = &self.on_new_refresh_token {
                keep(&rotated);
            }
        }
        Ok(tokens.access_token)
    }
}

/// Whether `call` carries a bearer token (the requests a signed-in service authorizes; an
/// upload to a pre-authorized URL carries none and must get none).
fn carries_bearer(call: &HttpCall) -> bool {
    call.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("authorization")
            && value
                .get(..7)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer "))
    })
}

/// `call` with `token` as its bearer token.
fn with_bearer(call: &HttpCall, token: &str) -> HttpCall {
    let mut call = call.clone();
    for (name, value) in &mut call.headers {
        if name.eq_ignore_ascii_case("authorization") {
            *value = format!("Bearer {token}");
        }
    }
    call
}

impl Transport for RefreshingTransport {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        if !carries_bearer(call) {
            return self.inner.send(call);
        }
        let token = self.access_token(None)?;
        let reply = self.inner.send(&with_bearer(call, &token))?;
        if reply.status != 401 {
            return Ok(reply);
        }
        // The service refused the token (it expired, or was revoked): a new one, once.
        let fresh = self.access_token(Some(&token))?;
        self.inner.send(&with_bearer(call, &fresh))
    }
}

/// The settings and the transport the OpenDAL service `scheme` opens with: a drive signed in
/// at a provider ([`provider`]; its secrets hold a [`REFRESH_TOKEN`], its options a
/// [`CLIENT_ID`]) gets OpenDAL's placeholder access token, its OAuth settings taken out, and a
/// [`RefreshingTransport`] around `transport` (a rotated refresh token goes to `rotated` as the
/// drive's whole new keyring text). Any other drive gets its settings and `transport` as they
/// are.
pub fn signed_in(
    scheme: &str,
    options: &BTreeMap<String, String>,
    secrets: &SecretOptions,
    transport: Box<dyn Transport>,
    rotated: Option<SecretSink>,
) -> (BTreeMap<String, String>, SecretOptions, Box<dyn Transport>) {
    fn non_blank(value: Option<&str>) -> Option<&str> {
        value.map(str::trim).filter(|v| !v.is_empty())
    }
    let (Some(provider), Some(refresh_token), Some(client_id)) = (
        provider(scheme),
        non_blank(secrets.get(REFRESH_TOKEN)),
        non_blank(options.get(CLIENT_ID).map(String::as_str)),
    ) else {
        return (options.clone(), secrets.clone(), transport);
    };
    let token_url =
        non_blank(options.get(TOKEN_URL).map(String::as_str)).unwrap_or(provider.token_url);
    let client = OAuthClient::public(client_id).with_secret(secrets.get(CLIENT_SECRET));
    let mut refreshing =
        RefreshingTransport::new(transport, provider.name, token_url, client, refresh_token);
    if let Some(sink) = rotated {
        let kept = secrets.clone();
        refreshing = refreshing.on_new_refresh_token(move |new_token| {
            let mut secrets = kept.clone();
            secrets.insert(REFRESH_TOKEN, new_token);
            sink(secrets.to_keyring_secret());
        });
    }
    let options = options
        .iter()
        .filter(|(key, _)| key.as_str() != CLIENT_ID && key.as_str() != TOKEN_URL)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut opendal_secrets: SecretOptions = secrets
        .iter()
        .filter(|(key, _)| ![REFRESH_TOKEN, CLIENT_SECRET, ACCESS_TOKEN].contains(key))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    opendal_secrets.insert(ACCESS_TOKEN, PLACEHOLDER_ACCESS_TOKEN);
    (options, opendal_secrets, Box::new(refreshing))
}
