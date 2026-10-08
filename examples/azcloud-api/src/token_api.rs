//! The token server's routes as a client calls them (the router is azul-apps
//! `iso/crates/azlin-token/src/lib.rs`; the Worker in production, `azctl
//! token` or `azctl dev up` locally). JSON in, JSON out; an error answer is
//! `{"error": <code>, "message": ...}` with its status, kept as a
//! [`TokenError`] so a caller can tell "sign in again" (401) from "try later".
//!
//! The credential refresh posts to `/v1/drives/{id}/credentials`, the route
//! the server has. (azlin-client's `TokenServer::refresh` posts to
//! `/v1/drives/{id}/refresh`, which the router answers with 404; that is why
//! this crate calls the routes itself.)

use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::Method;
use serde_json::{json, Value};

/// How long one call may take.
const TIMEOUT: Duration = Duration::from_secs(30);

/// An error answer of the token server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError {
    /// What was asked ("signup", "credential refresh").
    pub what: String,
    /// The HTTP status.
    pub status: u16,
    /// The server's code (`credentials_revoked`, `token_reuse`, `no_such_drive`, ...).
    pub code: String,
    /// The server's sentence.
    pub message: String,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: the token server answered {}",
            self.what, self.status
        )?;
        if !self.code.is_empty() {
            write!(f, " {}", self.code)?;
        }
        if !self.message.is_empty() {
            write!(f, " ({})", self.message)?;
        }
        if self.signed_out() {
            write!(
                f,
                "; this device must join the drive again (azcloud join <code>)"
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for TokenError {}

impl TokenError {
    /// Whether the device lost its right to the drive: its token family was
    /// revoked (a lockdown, an old token used again, a removed member) or the
    /// token is unknown. Retrying does not help; joining again does.
    #[must_use]
    pub fn signed_out(&self) -> bool {
        self.status == 401
    }
}

/// A drive id as it may stand in a URL path: `d_` and base32 from the
/// server; anything else is refused before it reaches a URL.
///
/// # Errors
///
/// When `id` holds anything but letters, digits, `_` and `-`.
pub fn check_id(id: &str) -> Result<&str> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !ok {
        bail!("{id:?} is not a drive or request id");
    }
    Ok(id)
}

/// The route of the credential refresh (pinned by a test: the server has no
/// `/refresh`).
#[must_use]
pub fn credentials_path(drive: &str) -> String {
    format!("/v1/drives/{drive}/credentials")
}

/// A client of one token server.
#[derive(Clone, Debug)]
pub struct TokenApi {
    base: String,
    http: reqwest::Client,
}

impl TokenApi {
    /// A client of the token server at `base` (no trailing slash needed).
    ///
    /// # Errors
    ///
    /// When the HTTP client cannot be built.
    pub fn new(base: &str) -> Result<TokenApi> {
        azlin_client::install_crypto_provider();
        let http = reqwest::Client::builder().timeout(TIMEOUT).build()?;
        Ok(TokenApi {
            base: base.trim().trim_end_matches('/').to_string(),
            http,
        })
    }

    /// The server's address.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    async fn call(
        &self,
        what: &str,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let mut request = self.http.request(method, &url);
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| {
            anyhow::anyhow!(
                "{what}: the token server at {} does not answer ({e})",
                self.base
            )
        })?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let value: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::Null)
        };
        if (200..300).contains(&status) {
            return Ok(value);
        }
        Err(TokenError {
            what: what.to_string(),
            status,
            code: value["error"].as_str().unwrap_or_default().to_string(),
            message: value["message"].as_str().unwrap_or_default().to_string(),
        }
        .into())
    }

    /// Whether the server answers: `GET /v1/tiers` (the tier ladder).
    ///
    /// # Errors
    ///
    /// When it does not.
    pub async fn tiers(&self) -> Result<Value> {
        self.call("the tier list", Method::GET, "/v1/tiers", None, None)
            .await
    }

    /// Signup without payment (`POST /v1/drives`, dev servers only; a real
    /// server sends signups through `/v1/checkout`): a drive of `tier` with
    /// its first credentials, the node list and the owner's drive token.
    ///
    /// # Errors
    ///
    /// The server's refusal, or no answer.
    pub async fn signup(&self, tier: &str, name: &str) -> Result<Value> {
        self.call(
            "signup",
            Method::POST,
            "/v1/drives",
            None,
            Some(json!({"tier": tier, "name": name})),
        )
        .await
    }

    /// The credential refresh (`POST /v1/drives/{id}/credentials`): 12-hour
    /// credentials, the endpoint, the node list and the NEXT drive token (the
    /// one given is spent by the call).
    ///
    /// # Errors
    ///
    /// The server's refusal (a [`TokenError`]), or no answer.
    pub async fn credentials(&self, drive: &str, token: &str, name: &str) -> Result<Value> {
        let path = credentials_path(check_id(drive)?);
        self.call(
            "credential refresh",
            Method::POST,
            &path,
            Some(token),
            Some(json!({"name": name})),
        )
        .await
    }

    /// The drive's state (`GET /v1/drives/{id}`): tier, quota, read-only,
    /// members, a pending lockdown.
    ///
    /// # Errors
    ///
    /// The server's refusal, or no answer.
    pub async fn info(&self, drive: &str, token: &str) -> Result<Value> {
        let path = format!("/v1/drives/{}", check_id(drive)?);
        self.call("drive info", Method::GET, &path, Some(token), None)
            .await
    }

    /// A new member token family (`POST /v1/drives/{id}/members`): what a
    /// second device joins with (`{"member", "drive_token"}`).
    ///
    /// # Errors
    ///
    /// The server's refusal, or no answer.
    pub async fn add_member(
        &self,
        drive: &str,
        token: &str,
        member: Option<&str>,
    ) -> Result<Value> {
        let path = format!("/v1/drives/{}/members", check_id(drive)?);
        let body = match member {
            Some(m) => json!({"member": m}),
            None => json!({}),
        };
        self.call("new member", Method::POST, &path, Some(token), Some(body))
            .await
    }

    /// The immediate lockdown by this device (`POST /v1/drives/{id}/lockdown`,
    /// §18.7): every other token family, key and public link is revoked; the
    /// answer is a new grant for this device.
    ///
    /// # Errors
    ///
    /// The server's refusal, or no answer.
    pub async fn lockdown(&self, drive: &str, token: &str) -> Result<Value> {
        let path = format!("/v1/drives/{}/lockdown", check_id(drive)?);
        self.call(
            "lockdown",
            Method::POST,
            &path,
            Some(token),
            Some(json!({})),
        )
        .await
    }

    /// Cancels a pending recovery-key lockdown (`POST .../lockdown/cancel`).
    ///
    /// # Errors
    ///
    /// The server's refusal (409 when none is pending), or no answer.
    pub async fn lockdown_cancel(&self, drive: &str, token: &str) -> Result<Value> {
        let path = format!("/v1/drives/{}/lockdown/cancel", check_id(drive)?);
        self.call(
            "lockdown cancel",
            Method::POST,
            &path,
            Some(token),
            Some(json!({})),
        )
        .await
    }

    /// Queues a restore of `prefix` (a key or a folder) as it was at `as_of`
    /// (RFC 3339; `POST /v1/drives/{id}/restore`, D38): `{"request_id",
    /// "status"}`.
    ///
    /// # Errors
    ///
    /// The server's refusal, or no answer.
    pub async fn restore(
        &self,
        drive: &str,
        token: &str,
        prefix: &str,
        as_of: &str,
    ) -> Result<Value> {
        let path = format!("/v1/drives/{}/restore", check_id(drive)?);
        self.call(
            "restore",
            Method::POST,
            &path,
            Some(token),
            Some(json!({"prefix": prefix, "as_of": as_of})),
        )
        .await
    }

    /// A restore's progress (`GET /v1/drives/{id}/restore/{request}`).
    ///
    /// # Errors
    ///
    /// The server's refusal, or no answer.
    pub async fn restore_status(&self, drive: &str, token: &str, request: &str) -> Result<Value> {
        let path = format!(
            "/v1/drives/{}/restore/{}",
            check_id(drive)?,
            check_id(request)?
        );
        self.call("restore status", Method::GET, &path, Some(token), None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credential_refresh_posts_to_the_route_the_token_server_has() {
        // azlin-token's router: ("POST", ["v1", "drives", id, "credentials"]).
        assert_eq!(credentials_path("d_abc"), "/v1/drives/d_abc/credentials");
    }

    #[test]
    fn an_id_that_could_change_the_url_is_refused() {
        assert!(check_id("d_k3f9-x").is_ok());
        for bad in ["", "../keys", "d_1/members", "d 1", "d_1?x", "%2e%2e"] {
            assert!(check_id(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_401_means_joining_again_and_says_so() {
        let e = TokenError {
            what: String::from("credential refresh"),
            status: 401,
            code: String::from("token_reuse"),
            message: String::from("an old token was reused"),
        };
        assert!(e.signed_out());
        let text = e.to_string();
        assert!(text.contains("401 token_reuse"), "{text}");
        assert!(text.contains("azcloud join"), "{text}");
        let busy = TokenError { status: 503, ..e };
        assert!(!busy.signed_out());
    }
}
