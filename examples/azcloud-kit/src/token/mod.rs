//! The token server's HTTP API, as the desktop apps use it (JSON both ways, errors as
//! `{"error": "<code>", "message": "<sentence>"}`):
//!
//! | call                                  | what for                                          |
//! |---------------------------------------|---------------------------------------------------|
//! | `GET /v1/tiers[?country=PL]`          | the storage tiers and their prices (in the        |
//! |                                       | buyer's country's currency where the ladder has   |
//! |                                       | it)                                               |
//! | `POST /v1/drives`                     | a drive without payment (development servers)     |
//! | `POST /v1/checkout`                   | a paid drive: where the browser pays (the claim   |
//! |                                       | key the sign-up is sealed to)                     |
//! | `GET /v1/checkout/{id}`               | pending / approved (the sealed sign-up, 30 days)  |
//! |                                       | / declined / expired; a cash checkout             |
//! |                                       | awaiting_cash / rejected (with its reason)        |
//! | `GET /v1/checkout/options`            | the providers' offer for a tier, period, country  |
//! |                                       | (azul-pay narrows it to the app's registry)       |
//! | `POST /v1/checkout` + provider        | a checkout through a provider: its surface (the   |
//! |                                       | fields page, a hosted page, the browser); `payer` |
//! |                                       | only for a method that asks the app for it        |
//! | `POST /v1/checkout/{id}/surface`      | the same checkout on the next surface             |
//! | `POST /v1/checkout/{id}/abandon`      | the provider session expires (popover closed);    |
//! |                                       | 409 checkout_paid for a checkout paid meanwhile   |
//! | `GET /v1/tokens/keys`                 | the period tokens' issuer keys (tier and year)    |
//! | `POST /v1/tokens/issue`               | a paid checkout's blind-signed period tokens      |
//! |                                       | (against the sealed sign-up's issue key)          |
//! | `POST /v1/drives/{id}/redeem`         | a period token: the drive's next month            |
//! | `POST /v1/drives/{id}/credentials`    | fresh credentials for the drive token (rotates)   |
//! | `GET /v1/drives/{id}`                 | the drive's tier, quota, members, lockdown, a ban |
//! |                                       | (status banned, ban_reason, ban_until; past the   |
//! |                                       | end 403 drive_banned)                             |
//! | `POST /v1/drives/{id}/members`        | a token family for another device to join with    |
//! | `POST /v1/drives/{id}/claim`          | a picked-up paid drive's own token family, by the |
//! |                                       | sealed sign-up's ticket (3 within 30 days)        |
//! | `POST /v1/drives/{id}/lockdown`       | every other device, key and link revoked at once  |
//! |                                       | (or by the recovery key: a fresh nonce, 48 h)     |
//! | `POST /v1/drives/{id}/lockdown/cancel`| a pending recovery-key lockdown called off        |
//! | `POST /v1/drives/{id}/recovery`       | the drive's recovery key (what signs a lockdown)  |
//! | `GET /v1/drives/{id}/recovery_keys`   | the drive's recovery keys (kits, passkeys)        |
//! | `POST /v1/drives/{id}/recovery_keys`  | one more, signed by a current key                 |
//! | `DELETE .../recovery_keys/{key}`      | one fewer (never the last), signed by a key       |
//! | `POST /v1/recovery/challenge`         | a challenge for the lookup (5 minutes)            |
//! | `POST /v1/recovery/lookup`            | the drives a recovery key belongs to (signed)     |
//! | `POST /v1/vouchers/redeem`            | a voucher: days on a drive, or a new drive        |
//! | `POST /v1/drives/{id}/restore`        | a prefix as it was at a time (queued)             |
//! | `GET /v1/drives/{id}/restore/{req}`   | a restore's progress                              |
//! | `POST /v1/drives/{id}/device_keys`    | this device's key for the drive (while it is      |
//! |                                       | open): it proves the owner if the drive is locked |
//! | `POST .../archive/challenge`          | a locked drive: a challenge (5 minutes, once)     |
//! | `POST .../archive/proof`              | the owner's signature over it (a recovery key or  |
//! |                                       | a device key); paid too, the drive comes back     |
//!
//! Blocking, through azul-storage's [`Transport`]: call it from an azul `Thread`.

use std::{fmt, sync::Arc, time::Duration};

use azlin_contract::{IdempotencyKey, IdempotentScope, IDEMPOTENCY_HEADER};
use azul_storage::{sigv4::uri_encode, HttpCall, HttpReply, Method, Transport};
use serde_json::{json, Value};

use crate::{
    answer::{Outcome, Spent},
    bundle::DriveBundle,
    claim::ClaimKey,
    outbox::Outbox,
    period::{IssuerKey, PeriodToken},
    policy::{Policy, Repeat, Sleep},
};

// One file per group of routes, so changes to one group do not meet changes to another.
mod archive;
mod checkout;
mod members;
mod period;
mod recovery;
mod refresh;

pub use archive::ProofAnswer;
pub use checkout::{
    Checkout, CheckoutFor, CheckoutStatus, CheckoutVia, OptionsQuery, Tier, TierNews, Tiers,
    VoucherRedeemed,
};
pub use members::ClaimedFamily;
pub use period::{BlindSignatures, IssueAnswer, MAX_BLINDED};
pub use recovery::{
    lockdown_cancel_message, recovery_add_message, recovery_key_message,
    recovery_lockdown_message, recovery_lookup_message, recovery_remove_message, FoundDrive,
    Presigned, RecoveryKeyInfo, RecoveryLockdown, RECOVERY_MEMBER,
};
pub use refresh::{
    ban_fluent_source, ban_reason_arg, banner_phrase, banner_text, closed_phrase, closed_text,
    Ban, DriveStatus, DRIVE_BANNED,
};

/// The tier a sign-up without one gets (the token server's default too).
pub const DEFAULT_TIER: &str = "100GB";
/// The payment method of a checkout when the app does not ask (a SEPA direct debit).
pub const DEFAULT_METHOD: &str = "sepa";

/// Why the token server could not help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// No answer (DNS, connection, TLS, a timeout).
    Connect(String),
    /// The drive token was refused (revoked, reused, unknown): this device must sign in to the
    /// drive again.
    SignIn(String),
    /// The token server said no, and asking again does not change it: its HTTP status, its
    /// error code, its sentence.
    Refused {
        status: u16,
        code: String,
        message: String,
    },
    /// The token server could not take the request now, and it passes by itself (its rate
    /// limit, a busy moment, a payment provider that did not answer): its HTTP status, its
    /// error code, its sentence, the seconds it asked to wait (`Retry-After`), and whether the
    /// request may have done something. The same request - the same drive token, the same
    /// idempotency key - goes again: at once by the policy when the pause is short and nothing
    /// was spent, else by the caller once `retry_after` is over.
    Busy {
        status: u16,
        code: String,
        message: String,
        retry_after: Option<u64>,
        spent: Spent,
    },
    /// An answer that makes no sense.
    Protocol(String),
    /// Not set up: no token server, an address that cannot be one, no drive id.
    Config(String),
}

impl TokenError {
    /// The token server sells drives through a checkout only: it has no development sign-up
    /// (`POST /v1/drives` answers 404).
    #[must_use]
    pub fn is_checkout_only(&self) -> bool {
        matches!(self, TokenError::Refused { status: 404, .. })
    }

    /// The token server's refusal `code` with `status` (its sentence `message`, the seconds of
    /// its `Retry-After`, what its body says it spent) as what it means ([`crate::answer`]):
    /// [`TokenError::Busy`] when it passes by itself, [`TokenError::Refused`] when it does not.
    #[must_use]
    pub fn answered(
        status: u16,
        code: &str,
        message: &str,
        retry_after: Option<u64>,
        said_spent: Option<bool>,
    ) -> TokenError {
        let outcome = crate::answer::classify(
            crate::answer::Service::TokenServer,
            status,
            Some(code),
            retry_after,
            said_spent,
        );
        match outcome {
            Outcome::Transient {
                retry_after, spent, ..
            } => TokenError::Busy {
                status,
                code: code.to_string(),
                message: message.to_string(),
                retry_after: retry_after.map(|pause| pause.as_secs()),
                spent,
            },
            Outcome::Done | Outcome::Refused(_) => TokenError::Refused {
                status,
                code: code.to_string(),
                message: message.to_string(),
            },
        }
    }

    /// The seconds the token server asked to wait before the same request goes again.
    #[must_use]
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            TokenError::Busy { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenError::Connect(e) => write!(f, "no answer from the Azlin token server: {e}"),
            TokenError::SignIn(e) => write!(
                f,
                "the Azlin token server refused this device's drive token ({e}): sign in to the \
                 drive again"
            ),
            TokenError::Refused {
                status,
                code,
                message,
            } => {
                let what = if message.is_empty() { code } else { message };
                write!(f, "the Azlin token server said: {what} (HTTP {status})")
            }
            TokenError::Busy {
                status,
                code,
                message,
                retry_after,
                ..
            } => {
                let what = if message.is_empty() { code } else { message };
                write!(f, "the Azlin token server is busy: {what} (HTTP {status})")?;
                match retry_after {
                    Some(secs) => write!(f, "; it asked to wait {secs} s"),
                    None => Ok(()),
                }
            }
            TokenError::Protocol(e) => {
                write!(f, "the Azlin token server answered unexpectedly: {e}")
            }
            TokenError::Config(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TokenError {}

// ==== The tiers ====

/// The host of an `http://` or `https://` URL (`[::1]` keeps its brackets), and whether it is
/// `https`; `None` for anything else.
#[must_use]
pub fn url_host(url: &str) -> Option<(&str, bool)> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    let https = if scheme.eq_ignore_ascii_case("https") {
        true
    } else if scheme.eq_ignore_ascii_case("http") {
        false
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return None;
    }
    let host = if authority.starts_with('[') {
        let end = authority.find(']')?;
        &authority[..=end]
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    (!host.is_empty()).then_some((host, https))
}

/// Whether `host` is this computer: `localhost`, `127.x.x.x`, `::1`.
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    let host = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || (host.starts_with("127.")
            && host.split('.').count() == 4
            && host.split('.').all(|part| part.parse::<u8>().is_ok()))
}

/// Checks a token server's address: `https://`, or `http://` to this computer only (the drive
/// token and the credentials would cross the network in the clear).
pub fn check_token_url(url: &str) -> Result<(), TokenError> {
    let Some((host, https)) = url_host(url) else {
        return Err(TokenError::Config(format!(
            "The token server \"{}\" is not a web address (https://...).",
            url.trim()
        )));
    };
    if !https && !is_loopback_host(host) {
        return Err(TokenError::Config(format!(
            "An unencrypted token server (http://) is only allowed on this computer, not {host}: \
             the drive token would cross the network in the clear."
        )));
    }
    Ok(())
}

/// The token server's error answer as a [`TokenError`]: a 401 to a call with a drive token
/// (`with_token`) is [`TokenError::SignIn`] - the token is gone (reused, revoked, unknown) -,
/// every other refusal what it means ([`TokenError::answered`]): a 403 refuses the call, not the
/// token, and a 503 (`not_verified`, `try_again`) or a rate limit is busy - the same token
/// again, later.
fn refusal(reply: &HttpReply, with_token: bool) -> TokenError {
    let value: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
    let code = value["error"].as_str().unwrap_or_default().to_string();
    let message = value["message"].as_str().unwrap_or_default().to_string();
    if reply.status == 401 && with_token {
        let why = match (code.is_empty(), message.is_empty()) {
            (true, true) => format!("HTTP {}", reply.status),
            (false, true) => code,
            (true, false) => message,
            (false, false) => format!("{message}, {code}"),
        };
        return TokenError::SignIn(why);
    }
    let retry_after = reply
        .header("retry-after")
        .and_then(|secs| secs.trim().parse::<u64>().ok());
    TokenError::answered(
        reply.status,
        &code,
        &message,
        retry_after,
        value["spent"].as_bool(),
    )
}

/// The token server at a base address. Blocking: call it from an azul `Thread`.
pub struct TokenServer<'a> {
    base: String,
    transport: &'a dyn Transport,
    /// How a request is asked again ([`Policy::TOKEN`]).
    policy: Policy,
    /// How its pauses are taken.
    sleep: Sleep,
    /// Where the keys of the requests it names itself (a checkout, its abandon) are kept until
    /// their final answer; `None`: each call names its request with a key of its own.
    outbox: Option<Outbox>,
}

impl<'a> TokenServer<'a> {
    /// The token server at `base` (`http://127.0.0.1:8081`, `https://token.example`), its
    /// requests sent through `transport`; refused when it is no usable address
    /// ([`check_token_url`]). Sends nothing.
    pub fn new(base: &str, transport: &'a dyn Transport) -> Result<TokenServer<'a>, TokenError> {
        check_token_url(base)?;
        Ok(TokenServer {
            base: base.trim().trim_end_matches('/').to_string(),
            transport,
            policy: Policy::TOKEN,
            sleep: Arc::new(std::thread::sleep) as Sleep,
            outbox: None,
        })
    }

    /// Keeps the keys of the requests it names itself (a checkout, its abandon) in `outbox`
    /// until their final answer, so a call asked again after a lost answer, a crash or a
    /// restart carries the key of its first try.
    #[must_use]
    pub fn with_outbox(mut self, outbox: Outbox) -> Self {
        self.outbox = Some(outbox);
        self
    }

    /// Takes every pause of the policy as `pause` instead (the tests: none at all), whatever the
    /// backoff or the token server's `Retry-After` says.
    #[must_use]
    pub fn with_retry_pause(mut self, pause: Duration) -> Self {
        self.sleep = Arc::new(move |_: Duration| {
            if !pause.is_zero() {
                std::thread::sleep(pause);
            }
        });
        self
    }

    /// Takes the policy's pauses with `sleep` (the tests record them).
    #[must_use]
    pub fn with_sleep(mut self, sleep: Sleep) -> Self {
        self.sleep = sleep;
        self
    }

    /// Asks requests again under `policy` instead of [`Policy::TOKEN`].
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Sends the grant call `call` until its outcome is final ([`crate::outbox::is_final`]),
    /// under the policy: what the token server said spent nothing was asked again with the
    /// request itself already (every request is); what may have been done - no answer, an
    /// answer cut off, a 5xx that does not say - goes again here. Safe for a grant only, whose
    /// every try carries the same idempotency key.
    pub fn until_final<T>(
        &self,
        mut call: impl FnMut() -> Result<T, TokenError>,
    ) -> Result<T, TokenError> {
        let mut attempt = 1;
        loop {
            let outcome = call();
            let answer = match &outcome {
                Ok(_)
                | Err(TokenError::Busy {
                    spent: Spent::Nothing,
                    ..
                }) => None,
                Err(error) => Some(crate::answer::of_token_error(error)),
            };
            let pause =
                answer.and_then(|answer| self.policy.again(&answer, attempt, Repeat::Keyed));
            let Some(pause) = pause else {
                return outcome;
            };
            (self.sleep)(pause);
            attempt += 1;
        }
    }

    /// Sends `send` - a request the token server keeps the first answer of, under the key it is
    /// given - until its outcome is final ([`Self::until_final`]), every try under the same key:
    /// the one the outbox keeps for `scope`, `name` and `words` (the request's own: another
    /// request gets a key of its own), else a key of this call. A lost answer is safe to send
    /// again then; the same key with another request is refused (`idempotency_key_reused`), and
    /// that refusal is final.
    fn named<T>(
        &self,
        scope: IdempotentScope,
        name: &str,
        words: &str,
        mut send: impl FnMut(&IdempotencyKey) -> Result<T, TokenError>,
    ) -> Result<T, TokenError> {
        match &self.outbox {
            Some(outbox) => outbox.send(scope, name, words, |key| self.until_final(|| send(key))),
            None => {
                let key = crate::outbox::fresh_key()?;
                self.until_final(|| send(&key))
            }
        }
    }

    /// The base address, without a trailing slash.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// One request; the answer's JSON when it says 2xx, its refusal otherwise.
    fn call(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
    ) -> Result<Value, TokenError> {
        let reply = self.send(method, path, bearer, body)?;
        serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))
    }

    /// [`Self::call`] of an account call, whose answer may be empty (`null` then).
    fn call_or_null(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
    ) -> Result<Value, TokenError> {
        let reply = self.send(method, path, bearer, body)?;
        if reply.body.iter().all(u8::is_ascii_whitespace) {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))
    }

    /// One request; the answer when it says 2xx, its refusal otherwise.
    fn send(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
    ) -> Result<HttpReply, TokenError> {
        self.send_keyed(method, path, bearer, body, None)
    }

    /// [`Self::send`] of a request named by the idempotency key `key`.
    fn send_keyed(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
        key: Option<&IdempotencyKey>,
    ) -> Result<HttpReply, TokenError> {
        let reply = self.exchange_keyed(method, path, bearer, body, key)?;
        if !reply.is_success() {
            return Err(refusal(&reply, bearer.is_some()));
        }
        Ok(reply)
    }

    /// A call that grants something (a lockdown, a claim, a member, a restore, a recovery key),
    /// named by `key`: the one key of the request across its retries ([`crate::outbox`]), so a
    /// repeat after a lost answer gets the first answer. Its answer's JSON, `null` when empty.
    fn grant(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
        key: &IdempotencyKey,
    ) -> Result<Value, TokenError> {
        let reply = self.send_keyed(method, path, bearer, body, Some(key))?;
        if reply.body.iter().all(u8::is_ascii_whitespace) {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))
    }

    /// One request and whatever it answers (a refusal too); only no answer is an error.
    fn exchange(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
    ) -> Result<HttpReply, TokenError> {
        self.exchange_keyed(method, path, bearer, body, None)
    }

    /// [`Self::exchange`] of a request named by the idempotency key `key`.
    fn exchange_keyed(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
        key: Option<&IdempotencyKey>,
    ) -> Result<HttpReply, TokenError> {
        let mut headers = vec![(String::from("accept"), String::from("application/json"))];
        if let Some(token) = bearer {
            headers.push((String::from("authorization"), format!("Bearer {token}")));
        }
        if let Some(key) = key {
            headers.push((IDEMPOTENCY_HEADER.to_string(), key.header_value()));
        }
        crate::hello::stamp_headers(&mut headers);
        let (body, content_type) = match body {
            Some(value) => (value.to_string().into_bytes(), String::from("application/json")),
            None => (Vec::new(), String::new()),
        };
        let call = HttpCall {
            method,
            url: format!("{}{path}", self.base),
            headers,
            body,
            content_type,
            // The token server answers small JSON documents; a body of any size is refused.
            limits: azul_storage::CallLimits::json(),
        };
        // A refusal that spent nothing (a rate limit, a moment the token server could not take
        // it) goes again as it is - the same drive token, the same idempotency key, the same
        // order - after the pause the policy takes; anything else is the caller's answer.
        let mut attempt = 1;
        loop {
            let reply = self.transport.send(&call).map_err(TokenError::Connect)?;
            let outcome = crate::answer::of_token_reply(&reply);
            let Some(pause) = self.policy.again(&outcome, attempt, Repeat::NothingSpent) else {
                return Ok(reply);
            };
            (self.sleep)(pause);
            attempt += 1;
        }
    }
}

/// A drive token to send; refused when there is none (this device must sign in first).
fn token_of(drive_token: &str) -> Result<&str, TokenError> {
    let token = drive_token.trim();
    if token.is_empty() {
        return Err(TokenError::SignIn(String::from("there is no drive token")));
    }
    Ok(token)
}

/// A drive or request id as it may stand in a URL path: letters, digits, `_` and `-` (the token
/// server's `d_` and base32). Anything else is refused before it reaches a URL.
pub fn check_id(id: &str) -> Result<&str, TokenError> {
    // The contract's rule for an id in a path; blanks around it are not taken here.
    match azlin_contract::DriveId::parse(id) {
        Ok(parsed) if parsed.as_str() == id => Ok(id),
        _ => Err(TokenError::Config(format!(
            "{id:?} is not a drive or request id"
        ))),
    }
}
