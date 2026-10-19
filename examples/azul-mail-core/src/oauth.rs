//! An OAuth 2 account's sign-in (XOAUTH2): its secret in the keyring is a grant - the access
//! token and, when the provider gave one, the refresh token, when the access token runs out, and
//! where a new one is had (the token endpoint, the client) - written as JSON. A secret that is a
//! bare token (pasted into the account form) stays as it is: it cannot be renewed.
//!
//! Before a sync or a send signs in, an access token that runs out within [`REFRESH_MARGIN_SECS`]
//! is renewed ([`refresh_shared`]): under the keyring entry's lock, so two AzMails of one user
//! never spend one refresh token twice, and a grant another one renewed meanwhile is taken from
//! the keyring instead.

use azcloud_kit::shared::SharedKeyring;
use azul_storage::{HttpCall, Method, Transport};
use serde::{Deserialize, Serialize};

/// An access token is renewed this long before it runs out.
pub const REFRESH_MARGIN_SECS: i64 = 120;

/// What the provider handed out, and how to get the next access token.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub access_token: String,
    /// Empty: the provider gave none (the token cannot be renewed).
    #[serde(default)]
    pub refresh_token: String,
    /// When the access token runs out (seconds since 1970); 0: not known.
    #[serde(default)]
    pub expires_at: i64,
    /// The provider's token endpoint (`https://.../token`).
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub client_id: String,
    /// Only for a provider that asks for one of an installed app.
    #[serde(default)]
    pub client_secret: String,
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never a token in a log.
        f.debug_struct("Grant")
            .field("expires_at", &self.expires_at)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("renewable", &self.can_refresh())
            .finish_non_exhaustive()
    }
}

impl Grant {
    /// The grant a secret holds; `None` for a bare token.
    #[must_use]
    pub fn parse(secret: &str) -> Option<Grant> {
        let trimmed = secret.trim_start();
        if !trimmed.starts_with('{') {
            return None;
        }
        serde_json::from_str(trimmed).ok()
    }

    /// The grant as the keyring keeps it.
    #[must_use]
    pub fn to_secret(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Whether a new access token can be had: a refresh token, an endpoint and a client.
    #[must_use]
    pub fn can_refresh(&self) -> bool {
        !self.refresh_token.is_empty() && !self.token_url.is_empty() && !self.client_id.is_empty()
    }

    /// Whether the access token runs out within [`REFRESH_MARGIN_SECS`] of `now` and can be
    /// renewed.
    #[must_use]
    pub fn needs_refresh(&self, now: i64) -> bool {
        self.can_refresh() && self.expires_at != 0 && now + REFRESH_MARGIN_SECS >= self.expires_at
    }

    /// The token endpoint's request body for a new access token (RFC 6749 section 6),
    /// `application/x-www-form-urlencoded`.
    #[must_use]
    pub fn refresh_form(&self) -> String {
        let mut fields = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", self.refresh_token.as_str()),
            ("client_id", self.client_id.as_str()),
        ];
        if !self.client_secret.is_empty() {
            fields.push(("client_secret", self.client_secret.as_str()));
        }
        fields
            .iter()
            .map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// The grant after the token endpoint's `answer` at `now`: the new access token and when
    /// it runs out; a new refresh token when the provider rotates it, else the old one.
    ///
    /// # Errors
    ///
    /// An answer without an access token: the endpoint's `error` and its description.
    pub fn refreshed(&self, answer: &str, now: i64) -> Result<Grant, String> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            access_token: String,
            #[serde(default)]
            expires_in: Option<i64>,
            #[serde(default)]
            refresh_token: Option<String>,
            #[serde(default)]
            error: Option<String>,
            #[serde(default)]
            error_description: Option<String>,
        }
        let a: Answer = serde_json::from_str(answer)
            .map_err(|_| String::from("the token endpoint's answer is no JSON"))?;
        if a.access_token.is_empty() {
            let error = a.error.unwrap_or_else(|| String::from("no access token"));
            return Err(match a.error_description {
                Some(why) => format!("{error}: {why}"),
                None => error,
            });
        }
        Ok(Grant {
            access_token: a.access_token,
            refresh_token: a
                .refresh_token
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| self.refresh_token.clone()),
            expires_at: a.expires_in.map_or(0, |secs| now + secs.max(0)),
            ..self.clone()
        })
    }
}

/// The token to sign in with: a grant's access token, or the secret itself (a pasted token).
#[must_use]
pub fn access_token(secret: &str) -> String {
    Grant::parse(secret).map_or_else(|| secret.to_string(), |g| g.access_token)
}

/// `text` for a form body: unreserved characters as they are, every other byte `%XX`.
fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A new access token for `held` from its endpoint, through `transport`. Blocking: from a sync
/// or send thread.
///
/// # Errors
///
/// No answer, a refusal, or an answer without an access token.
pub fn refresh(transport: &dyn Transport, held: &Grant, now: i64) -> Result<Grant, String> {
    let call = HttpCall {
        method: Method::Post,
        url: held.token_url.clone(),
        headers: vec![(String::from("Accept"), String::from("application/json"))],
        body: held.refresh_form().into_bytes(),
        content_type: String::from("application/x-www-form-urlencoded"),
    };
    let reply = transport.send(&call)?;
    let text = String::from_utf8_lossy(&reply.body);
    if !(200..300).contains(&reply.status) {
        return Err(match held.refreshed(&text, now) {
            Err(why) => format!("the token endpoint refused ({}): {why}", reply.status),
            Ok(_) => format!("the token endpoint refused ({})", reply.status),
        });
    }
    held.refreshed(&text, now)
}

/// [`refresh`] under the keyring entry `key`'s lock (every AzMail of this user shares it): a
/// grant another one renewed meanwhile (a newer access token that does not run out yet) is
/// taken from the keyring; a new one is written there before the lock is let go. With whether
/// the keyring's was taken.
///
/// # Errors
///
/// [`refresh`]'s, and another AzMail holding the lock longer than a refresh takes.
pub fn refresh_shared(
    transport: &dyn Transport,
    keyring: &SharedKeyring,
    key: &str,
    held: &Grant,
    now: i64,
) -> Result<(Grant, bool), String> {
    let _lock = keyring
        .lock(key)
        .map_err(|e| format!("another AzMail is renewing this account's sign-in ({e})"))?;
    if let Ok(Some(text)) = keyring.get(key) {
        if let Some(stored) = Grant::parse(&text) {
            if stored.access_token != held.access_token && !stored.needs_refresh(now) {
                return Ok((stored, true));
            }
        }
    }
    let fresh = refresh(transport, held, now)?;
    keyring
        .set(key, &fresh.to_secret())
        .map_err(|e| format!("the new sign-in could not be kept in the keyring ({e})"))?;
    Ok((fresh, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant() -> Grant {
        Grant {
            access_token: String::from("ya29.old"),
            refresh_token: String::from("1//refresh"),
            expires_at: 1_000_000,
            token_url: String::from("https://oauth.example/token"),
            client_id: String::from("client id"),
            client_secret: String::new(),
        }
    }

    #[test]
    fn a_grant_is_kept_as_json_and_a_pasted_token_stays_a_token() {
        let g = grant();
        assert_eq!(Grant::parse(&g.to_secret()), Some(g.clone()));
        assert_eq!(access_token(&g.to_secret()), "ya29.old");
        assert_eq!(access_token("ya29.pasted"), "ya29.pasted");
        assert!(Grant::parse("ya29.pasted").is_none());
        assert!(!format!("{g:?}").contains("ya29"), "no token in a log");
    }

    #[test]
    fn an_access_token_is_renewed_shortly_before_it_runs_out_when_it_can_be() {
        let g = grant();
        assert!(!g.needs_refresh(1_000_000 - REFRESH_MARGIN_SECS - 1));
        assert!(g.needs_refresh(1_000_000 - REFRESH_MARGIN_SECS));
        let mut pasted = g.clone();
        pasted.refresh_token.clear();
        assert!(!pasted.needs_refresh(2_000_000), "nothing to renew it with");
        let mut unknown = g;
        unknown.expires_at = 0;
        assert!(!unknown.needs_refresh(2_000_000), "an unknown lifetime is used until refused");
    }

    #[test]
    fn the_refresh_request_and_answer_follow_rfc_6749() {
        let g = grant();
        assert_eq!(
            g.refresh_form(),
            "grant_type=refresh_token&refresh_token=1%2F%2Frefresh&client_id=client%20id"
        );
        let next = g
            .refreshed(r#"{"access_token":"ya29.new","expires_in":3600,"token_type":"Bearer"}"#, 500)
            .unwrap();
        assert_eq!((next.access_token.as_str(), next.expires_at), ("ya29.new", 4100));
        assert_eq!(next.refresh_token, "1//refresh", "kept when the provider does not rotate it");
        let rotated = g
            .refreshed(r#"{"access_token":"a","expires_in":10,"refresh_token":"1//next"}"#, 0)
            .unwrap();
        assert_eq!(rotated.refresh_token, "1//next");
        let refused = g
            .refreshed(r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#, 0)
            .unwrap_err();
        assert!(refused.contains("invalid_grant") && refused.contains("revoked"), "{refused}");
    }
}
