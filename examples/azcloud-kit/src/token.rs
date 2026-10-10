//! The token server's HTTP API, as the desktop apps use it (JSON both ways, errors as
//! `{"error": "<code>", "message": "<sentence>"}`):
//!
//! | call                                  | what for                                          |
//! |---------------------------------------|---------------------------------------------------|
//! | `GET /v1/tiers`                       | the storage tiers and their prices                |
//! | `POST /v1/drives`                     | a drive without payment (development servers)     |
//! | `POST /v1/checkout`                   | a paid drive: where the browser pays (the claim   |
//! |                                       | key the sign-up is sealed to)                     |
//! | `GET /v1/checkout/{id}`               | pending / approved (the sealed sign-up, 30 days)  |
//! |                                       | / declined / expired                              |
//! | `GET /v1/tokens/keys`                 | the period tokens' issuer keys (tier and year)    |
//! | `POST /v1/tokens/issue`               | a paid checkout's blind-signed period tokens      |
//! |                                       | (against the sealed sign-up's issue key)          |
//! | `POST /v1/drives/{id}/redeem`         | a period token: the drive's next month            |
//! | `POST /v1/drives/{id}/credentials`    | fresh credentials for the drive token (rotates)   |
//! | `GET /v1/drives/{id}`                 | the drive's tier, quota, members, lockdown        |
//! | `POST /v1/drives/{id}/members`        | a token family for another device to join with    |
//! | `POST /v1/drives/{id}/lockdown`       | every other device, key and link revoked at once  |
//! |                                       | (or by the recovery key: a fresh nonce, 48 h)     |
//! | `POST /v1/drives/{id}/lockdown/cancel`| a pending recovery-key lockdown called off        |
//! | `POST /v1/vouchers/redeem`            | a voucher: days on a drive, or a new drive        |
//! | `POST /v1/drives/{id}/restore`        | a prefix as it was at a time (queued)             |
//! | `GET /v1/drives/{id}/restore/{req}`   | a restore's progress                              |
//!
//! Blocking, through azul-storage's [`Transport`]: call it from an azul `Thread`.

use std::{fmt, time::Duration};

use azul_storage::{sigv4::uri_encode, HttpCall, HttpReply, Method, Transport};
use serde_json::{json, Value};

use crate::{
    bundle::DriveBundle,
    claim::ClaimKey,
    period::{IssuerKey, PeriodToken},
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
    /// The token server said no: its HTTP status, its error code, its sentence.
    Refused {
        status: u16,
        code: String,
        message: String,
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
            TokenError::Protocol(e) => {
                write!(f, "the Azlin token server answered unexpectedly: {e}")
            }
            TokenError::Config(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TokenError {}

// ==== The tiers ====

/// One storage tier: its size and its prices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tier {
    /// `100GB`, `1TB`: what a sign-up or a checkout names.
    pub id: String,
    pub quota_bytes: u64,
    /// The price of a month, in cents of `currency`; `None` when the server names none.
    pub price_cents_month: Option<u64>,
    /// The price of a year (paid yearly).
    pub price_cents_year: Option<u64>,
    /// `EUR`.
    pub currency: String,
    /// The first month costs nothing.
    pub first_month_free: bool,
}

/// `99` cents as `0.99`.
fn amount(cents: u64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

impl Tier {
    /// The size as people read it: `100 GB`, `1 TB`, `12 TB` (decimal units, as the tiers are
    /// sold).
    #[must_use]
    pub fn quota_text(&self) -> String {
        const TB: u64 = 1_000_000_000_000;
        const GB: u64 = 1_000_000_000;
        let bytes = self.quota_bytes;
        if bytes >= TB && bytes % TB == 0 {
            format!("{} TB", bytes / TB)
        } else if bytes >= TB {
            format!("{:.1} TB", bytes as f64 / TB as f64)
        } else if bytes % GB == 0 {
            format!("{} GB", bytes / GB)
        } else {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        }
    }

    /// The price as people read it: `EUR 0.99 a month`, or with `yearly` `EUR 9.90 a year`;
    /// `None` when the server names no price.
    #[must_use]
    pub fn price_text(&self, yearly: bool) -> Option<String> {
        let (cents, per) = if yearly {
            (self.price_cents_year?, "a year")
        } else {
            (self.price_cents_month?, "a month")
        };
        Some(format!("{} {} {per}", self.currency, amount(cents)))
    }
}

/// The token server's tier list (`GET /v1/tiers`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tiers {
    pub tiers: Vec<Tier>,
    /// The payment methods a checkout takes: `sepa`, `card`, ...
    pub methods: Vec<String>,
    /// The sentence a buyer agrees to (the right of withdrawal ends when the service starts).
    pub withdrawal_consent: Option<String>,
}

impl Tiers {
    /// Reads the answer of `GET /v1/tiers`.
    pub fn parse(text: &str) -> Result<Tiers, TokenError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|_| TokenError::Protocol(String::from("the tier list is not JSON")))?;
        let list = value["tiers"]
            .as_array()
            .ok_or_else(|| TokenError::Protocol(String::from("the answer has no tier list")))?;
        let mut tiers = Vec::with_capacity(list.len());
        for tier in list {
            let id = tier["id"].as_str().unwrap_or_default().trim().to_string();
            let quota_bytes = tier["quota_bytes"].as_u64().unwrap_or(0);
            if id.is_empty() || quota_bytes == 0 {
                return Err(TokenError::Protocol(String::from(
                    "a tier has no id or no size",
                )));
            }
            tiers.push(Tier {
                id,
                quota_bytes,
                price_cents_month: tier["price_cents_month"].as_u64(),
                price_cents_year: tier["price_cents_year"].as_u64(),
                currency: tier["currency"].as_str().unwrap_or("EUR").to_string(),
                first_month_free: tier["first_month_free"].as_bool().unwrap_or(false),
            });
        }
        let methods = value["methods"]
            .as_array()
            .map(|m| {
                m.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let withdrawal_consent = value["legal"]["withdrawal_consent"]
            .as_str()
            .map(str::to_string);
        Ok(Tiers {
            tiers,
            methods,
            withdrawal_consent,
        })
    }
}

// ==== The checkout ====

/// A checkout: where the browser pays for a tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub checkout_id: String,
    /// The payment page to open in the browser.
    pub pay_url: String,
    pub tier: String,
    pub months: u32,
    pub amount_cents: u64,
    pub currency: String,
    /// The token server's own test provider takes the payment (a development server).
    pub mock: bool,
}

/// Where a checkout stands (`GET /v1/checkout/{id}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutStatus {
    /// Not paid yet.
    Pending,
    /// Paid: the new drive, opened from the sign-up the token server sealed to the checkout's
    /// claim key (it answers it to every poll for 30 days).
    Approved(Box<DriveBundle>),
    /// The payment was declined or reversed: why.
    Declined(String),
    /// The token server no longer has the checkout (it expired, or it is unknown: a 404): why.
    Gone(String),
}

/// What `POST /v1/tokens/issue` answers: one blind signature per blinded message, by the
/// issuer key of the tier and year.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlindSignatures {
    /// `100GB`.
    pub tier: String,
    /// `<tier>/<year>`: the issuer key's name.
    pub key_id: String,
    /// The issuer's public key (SPKI PEM): what the finished tokens verify against.
    pub public_key_pem: String,
    /// Standard base64, in the order of the blinded messages.
    pub signatures: Vec<String>,
}

/// The most blinded messages one `POST /v1/tokens/issue` takes (24 months, prepaid).
pub const MAX_BLINDED: usize = 24;

/// What a recovery-key lockdown answers (202): the drive is read-only until `pending_until`
/// (every device may cancel until then), and `drive_token` is the new family this side gets
/// after it. `Debug` shows no token.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryLockdown {
    /// The request's nonce (new for every request).
    pub nonce: String,
    /// In seconds since 1970.
    pub pending_until: Option<u64>,
    pub drive_token: String,
}

impl fmt::Debug for RecoveryLockdown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoveryLockdown")
            .field("nonce", &self.nonce)
            .field("pending_until", &self.pending_until)
            .field("drive_token", &"<hidden>")
            .finish()
    }
}

/// What a voucher bought.
#[derive(Debug)]
pub enum VoucherRedeemed {
    /// The drive's period grew by `days_added` (its months and its value pro rata, AZLINSEC17
    /// F29), to `period_until` (seconds since 1970).
    Extended {
        days_added: u32,
        period_until: Option<u64>,
    },
    /// A new drive of the voucher's tier (its sign-up: save it before its first refresh).
    NewDrive(Box<DriveBundle>),
}

/// What a recovery-key lockdown signs: `lockdown:<drive>:<nonce>`.
#[must_use]
pub fn recovery_lockdown_message(drive_id: &str, nonce: &str) -> String {
    format!("lockdown:{drive_id}:{nonce}")
}

/// A lockdown nonce: 16 random bytes, hex (32 characters; the token server takes 16 to 128 and
/// each once per drive).
fn lockdown_nonce() -> Result<String, TokenError> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| {
        TokenError::Config(String::from(
            "This computer's random source does not answer: no lockdown request can be made.",
        ))
    })?;
    Ok(crate::period::hex(&bytes))
}

// ==== The client ====

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
/// every other refusal [`TokenError::Refused`]: a 403 refuses the call, not the token, and a 503
/// (`not_verified`, `try_again`) asks for the same token again later.
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
    TokenError::Refused {
        status: reply.status,
        code,
        message,
    }
}

/// How often a refresh is sent with one drive token while the token server answers 503 (the
/// token is not spent then).
const REFRESH_TRIES: u32 = 3;
/// The pause between two such tries.
const REFRESH_RETRY_PAUSE: Duration = Duration::from_secs(2);

/// The token server at a base address. Blocking: call it from an azul `Thread`.
pub struct TokenServer<'a> {
    base: String,
    transport: &'a dyn Transport,
    retry_pause: Duration,
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
            retry_pause: REFRESH_RETRY_PAUSE,
        })
    }

    /// Pauses `pause` (instead of two seconds) before a refresh the token server answered 503
    /// is sent again.
    #[must_use]
    pub fn with_retry_pause(mut self, pause: Duration) -> Self {
        self.retry_pause = pause;
        self
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
        let mut headers = vec![(String::from("accept"), String::from("application/json"))];
        if let Some(token) = bearer {
            headers.push((String::from("authorization"), format!("Bearer {token}")));
        }
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
        };
        let reply = self.transport.send(&call).map_err(TokenError::Connect)?;
        if !reply.is_success() {
            return Err(refusal(&reply, bearer.is_some()));
        }
        Ok(reply)
    }

    /// The storage tiers and their prices.
    pub fn tiers(&self) -> Result<Tiers, TokenError> {
        let value = self.call(Method::Get, "/v1/tiers", None, None)?;
        Tiers::parse(&value.to_string())
    }

    /// A drive named `name` of tier `tier` without payment - a development token server only;
    /// a production one answers [`TokenError::is_checkout_only`].
    pub fn create_dev_drive(&self, name: &str, tier: &str) -> Result<DriveBundle, TokenError> {
        let body = json!({ "name": name.trim(), "tier": tier.trim() });
        let value = self.call(Method::Post, "/v1/drives", None, Some(&body))?;
        DriveBundle::from_value(&value)
    }

    /// A checkout of `tier` for `months` months (1, 3, 6, 12 or 24) paid by `method` (`sepa`,
    /// `card`, ...): where the browser pays. Its sign-up is sealed to `claim` (the public half
    /// is sent, `claim_key`); the drive comes with [`Self::checkout_status`], opened with it.
    pub fn checkout(
        &self,
        tier: &str,
        months: u32,
        method: &str,
        claim: &ClaimKey,
    ) -> Result<Checkout, TokenError> {
        let body = json!({
            "tier": tier.trim(),
            "months": months,
            "method": method.trim(),
            "claim_key": claim.public_base64(),
        });
        let value = self.call(Method::Post, "/v1/checkout", None, Some(&body))?;
        let text = |key: &str| value[key].as_str().unwrap_or_default().trim().to_string();
        let checkout = Checkout {
            checkout_id: text("checkout_id"),
            pay_url: text("pay_url"),
            tier: text("tier"),
            months: value["months"]
                .as_u64()
                .and_then(|m| u32::try_from(m).ok())
                .unwrap_or(months),
            amount_cents: value["amount_cents"].as_u64().unwrap_or(0),
            currency: value["currency"].as_str().unwrap_or("EUR").to_string(),
            mock: value["mock"].as_bool().unwrap_or(false),
        };
        if checkout.checkout_id.is_empty() || checkout.pay_url.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the checkout has no id or no payment page",
            )));
        }
        Ok(checkout)
    }

    /// Where the checkout `checkout_id` stands; once it is paid, the new drive - its sign-up
    /// opened with `claim`, the key the checkout named (every poll for 30 days answers it).
    ///
    /// # Errors
    ///
    /// No answer or a refusal; [`TokenError::Protocol`] for an approved checkout whose sign-up
    /// is not sealed to `claim` for this checkout (or is no drive bundle).
    pub fn checkout_status(
        &self,
        checkout_id: &str,
        claim: &ClaimKey,
    ) -> Result<CheckoutStatus, TokenError> {
        let id = checkout_id.trim();
        if id.is_empty() {
            return Err(TokenError::Config(String::from("There is no checkout to ask about.")));
        }
        let path = format!("/v1/checkout/{}", uri_encode(id, true));
        let value = match self.call(Method::Get, &path, None, None) {
            Ok(value) => value,
            Err(TokenError::Refused {
                status: 404,
                code,
                message,
            }) => {
                let why = match (message.is_empty(), code.is_empty()) {
                    (false, _) => message,
                    (true, false) => code,
                    (true, true) => String::from("the token server does not know the checkout"),
                };
                return Ok(CheckoutStatus::Gone(why));
            }
            Err(e) => return Err(e),
        };
        let reason = |default: &str| value["reason"].as_str().unwrap_or(default).to_string();
        match value["status"].as_str().unwrap_or_default() {
            "pending" => Ok(CheckoutStatus::Pending),
            "approved" => {
                // The plaintext `signup` of a token server without claims is never taken: anyone
                // who knew the checkout id could have read it.
                let sealed = value["sealed_signup"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        TokenError::Protocol(String::from(
                            "the approved checkout has no sealed sign-up",
                        ))
                    })?;
                let signup = claim
                    .open(id, sealed)
                    .map_err(|e| TokenError::Protocol(e.to_string()))?;
                Ok(CheckoutStatus::Approved(Box::new(DriveBundle::parse(
                    &signup,
                )?)))
            }
            "declined" | "reversed" => Ok(CheckoutStatus::Declined(reason(
                "the payment was declined",
            ))),
            "expired" => Ok(CheckoutStatus::Gone(reason(
                "the checkout expired at the token server",
            ))),
            other => Err(TokenError::Protocol(format!(
                "the checkout is \"{other}\", neither pending, approved, declined nor expired"
            ))),
        }
    }

    /// The period tokens' issuer keys of this year, one per tier (`GET /v1/tokens/keys`): what
    /// a token is blinded for ([`crate::period::Issuer`]). Entries without a tier, a year or a
    /// key are left out.
    pub fn issuer_keys(&self) -> Result<Vec<IssuerKey>, TokenError> {
        let value = self.call(Method::Get, "/v1/tokens/keys", None, None)?;
        let keys = value["keys"].as_array().ok_or_else(|| {
            TokenError::Protocol(String::from("the answer lists no issuer keys"))
        })?;
        Ok(keys
            .iter()
            .filter_map(|key| {
                let tier = key["tier"].as_str()?.trim().to_string();
                let year = u32::try_from(key["year"].as_u64()?).ok()?;
                let public_key_pem = key["public_key_pem"].as_str()?.to_string();
                let key_id = key["key_id"]
                    .as_str()
                    .map_or_else(|| format!("{tier}/{year}"), str::to_string);
                (!tier.is_empty()).then_some(IssuerKey {
                    tier,
                    year,
                    key_id,
                    public_key_pem,
                })
            })
            .collect())
    }

    /// Blind signatures of a paid checkout's period tokens (`POST /v1/tokens/issue`): one per
    /// message of `blinded` (standard base64, at most [`MAX_BLINDED`]), up to the checkout's
    /// months in all. Only with `issue_key`, the key its sealed sign-up carries
    /// ([`crate::bundle::PeriodTokens`]): the checkout id alone, which the payment provider sees,
    /// issues nothing.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] without an issue key or messages (nothing is sent); the token
    /// server's refusals with their codes - `issue_key_required`, `issue_key_wrong` (403, no
    /// sign-in matter), `already_issued`, `not_paid`, `mandate_stopped`; an answer whose
    /// signatures do not match the messages.
    pub fn issue_period_tokens(
        &self,
        checkout_id: &str,
        issue_key: &str,
        blinded: &[String],
    ) -> Result<BlindSignatures, TokenError> {
        let issue_key = issue_key.trim();
        if issue_key.is_empty() {
            return Err(TokenError::Config(String::from(
                "There is no issue key: a checkout's period tokens are issued only against the \
                 key its sealed sign-up carries.",
            )));
        }
        if blinded.is_empty() || blinded.len() > MAX_BLINDED {
            return Err(TokenError::Config(format!(
                "1 to {MAX_BLINDED} blinded messages, not {}",
                blinded.len()
            )));
        }
        let body = json!({
            "checkout_id": checkout_id.trim(),
            "issue_key": issue_key,
            "blinded": blinded,
        });
        let value = self.call(Method::Post, "/v1/tokens/issue", None, Some(&body))?;
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let signatures: Vec<String> = value["blind_signatures"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if signatures.len() != blinded.len() {
            return Err(TokenError::Protocol(format!(
                "{} blind signatures for {} blinded messages",
                signatures.len(),
                blinded.len()
            )));
        }
        Ok(BlindSignatures {
            tier: text("tier"),
            key_id: text("key_id"),
            public_key_pem: text("public_key_pem"),
            signatures,
        })
    }

    /// One more month for `drive_id` paid with `token` (`POST /v1/drives/{id}/redeem`, with
    /// this device's drive token, which it does not spend): the period's new end, in seconds
    /// since 1970, when the answer names it.
    ///
    /// # Errors
    ///
    /// The token server's refusals: `token_used` (409: redeemed before - drop it),
    /// `wrong_tier`, `token_expired`, `bad_token`, `unknown_issuer`; a 401 is a sign-in.
    pub fn redeem_period_token(
        &self,
        drive_id: &str,
        drive_token: &str,
        token: &PeriodToken,
    ) -> Result<Option<u64>, TokenError> {
        let path = format!("/v1/drives/{}/redeem", check_id(drive_id)?);
        let body = json!({
            "tier": token.tier,
            "year": token.year,
            "nonce": token.nonce,
            "signature": token.signature,
            "randomizer": token.randomizer,
        });
        let value =
            self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))?;
        Ok(value["period_until"]
            .as_str()
            .and_then(azul_storage::time::parse_iso8601))
    }

    /// Fresh credentials for `drive_id` with this device's drive token. The answer carries the
    /// NEXT drive token: the one given is spent, and spending it again makes the token server
    /// revoke this device.
    pub fn refresh(&self, drive_id: &str, drive_token: &str) -> Result<DriveBundle, TokenError> {
        self.refresh_with(drive_id, drive_token, &json!({}))
    }

    /// [`Self::refresh`] that tells the token server what this device calls the drive: the
    /// answer's drive carries `name` instead of the server's default one.
    pub fn refresh_named(
        &self,
        drive_id: &str,
        drive_token: &str,
        name: &str,
    ) -> Result<DriveBundle, TokenError> {
        self.refresh_with(drive_id, drive_token, &json!({ "name": name.trim() }))
    }

    fn refresh_with(
        &self,
        drive_id: &str,
        drive_token: &str,
        body: &Value,
    ) -> Result<DriveBundle, TokenError> {
        let drive_id = drive_id.trim();
        if drive_id.is_empty() {
            return Err(TokenError::Config(String::from("The drive has no id.")));
        }
        let token = token_of(drive_token)?;
        let path = format!("/v1/drives/{}/credentials", uri_encode(drive_id, true));
        // A 503 (`not_verified`: the token family is not adopted after an upgrade yet;
        // `try_again`: it changed between the check and the rotation) spent nothing: the SAME
        // token again after a pause, and never a reason to drop it.
        let mut tries = 1;
        let value = loop {
            match self.call(Method::Post, &path, Some(token), Some(body)) {
                Err(TokenError::Refused { status: 503, .. }) if tries < REFRESH_TRIES => {
                    tries += 1;
                    std::thread::sleep(self.retry_pause);
                }
                answer => break answer?,
            }
        };
        let bundle = DriveBundle::from_value(&value)?;
        if bundle.drive_id() != drive_id {
            return Err(TokenError::Protocol(format!(
                "the answer is for the drive {}, not {drive_id}",
                bundle.drive_id()
            )));
        }
        Ok(bundle)
    }

    // ==== A drive's account calls, with this device's drive token ====

    /// The drive's state as the token server keeps it (`GET /v1/drives/{id}`): its tier, quota,
    /// read-only flag, members, a pending lockdown.
    pub fn info(&self, drive_id: &str, drive_token: &str) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}", check_id(drive_id)?);
        self.call_or_null(Method::Get, &path, Some(token_of(drive_token)?), None)
    }

    /// A new member token family of the drive (`POST /v1/drives/{id}/members`): what a second
    /// device joins with, `{"member", "drive_token"}`. `member` names it, else the token server
    /// does.
    pub fn add_member(
        &self,
        drive_id: &str,
        drive_token: &str,
        member: Option<&str>,
    ) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}/members", check_id(drive_id)?);
        let body = match member {
            Some(member) => json!({ "member": member }),
            None => json!({}),
        };
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))
    }

    /// The immediate lockdown by this device (`POST /v1/drives/{id}/lockdown`): every other
    /// token family, key and public link of the drive is revoked at once; the answer is this
    /// device's new grant (a bundle).
    pub fn lockdown(&self, drive_id: &str, drive_token: &str) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}/lockdown", check_id(drive_id)?);
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&json!({})))
    }

    /// The lockdown by the drive's recovery key (`POST /v1/drives/{id}/lockdown {"nonce",
    /// "signature"}`): `sign` signs [`recovery_lockdown_message`] of a nonce made new for this
    /// request (the token server answers a request it saw before with 409 `nonce_used`) and
    /// answers the signature as the token server takes it (base64). It takes effect after 48 h
    /// unless a device cancels it; no drive token is sent.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals (`nonce_used`, `no_recovery_key`, a bad signature: 401 without a drive token is
    /// a refusal too).
    pub fn recovery_lockdown(
        &self,
        drive_id: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<RecoveryLockdown, TokenError> {
        let id = check_id(drive_id)?;
        let nonce = lockdown_nonce()?;
        let signature = sign(recovery_lockdown_message(id, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The lockdown request is not signed: {e}")))?;
        let path = format!("/v1/drives/{id}/lockdown");
        let body = json!({ "nonce": nonce, "signature": signature });
        let value = self.call(Method::Post, &path, None, Some(&body))?;
        Ok(RecoveryLockdown {
            nonce,
            pending_until: value["pending_until"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601),
            drive_token: value["drive_token"].as_str().unwrap_or_default().to_string(),
        })
    }

    /// A voucher `code` (`POST /v1/vouchers/redeem`): on the drive `drive` (its id and this
    /// device's drive token, which it does not spend) the days it added; without one a new drive
    /// of `tier` (empty: the voucher's own, else the token server's default).
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] for an empty code (nothing is sent); the token server's refusals
    /// (`voucher_invalid`, `voucher_too_small`, `bad_tier`).
    pub fn redeem_voucher(
        &self,
        code: &str,
        drive: Option<(&str, &str)>,
        tier: &str,
    ) -> Result<VoucherRedeemed, TokenError> {
        let code = code.trim();
        if code.is_empty() {
            return Err(TokenError::Config(String::from("There is no voucher code.")));
        }
        let Some((drive_id, drive_token)) = drive else {
            let mut body = json!({ "code": code });
            if !tier.trim().is_empty() {
                body["tier"] = json!(tier.trim());
            }
            let value = self.call(Method::Post, "/v1/vouchers/redeem", None, Some(&body))?;
            return Ok(VoucherRedeemed::NewDrive(Box::new(
                DriveBundle::from_value(&value)?,
            )));
        };
        let body = json!({ "code": code, "drive_id": check_id(drive_id)? });
        let value = self.call(
            Method::Post,
            "/v1/vouchers/redeem",
            Some(token_of(drive_token)?),
            Some(&body),
        )?;
        // An older token server answers whole months only.
        let days_added = value["days_added"]
            .as_u64()
            .or_else(|| value["months_added"].as_u64().map(|m| m.saturating_mul(30)))
            .and_then(|d| u32::try_from(d).ok())
            .unwrap_or(0);
        Ok(VoucherRedeemed::Extended {
            days_added,
            period_until: value["period_until"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601),
        })
    }

    /// Cancels a pending recovery-key lockdown (`POST /v1/drives/{id}/lockdown/cancel`; a 409
    /// when none is pending).
    pub fn lockdown_cancel(&self, drive_id: &str, drive_token: &str) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}/lockdown/cancel", check_id(drive_id)?);
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&json!({})))
    }

    /// Queues a restore of `prefix` (a key or a folder) as it was at `as_of` (RFC 3339; `POST
    /// /v1/drives/{id}/restore`): `{"request_id", "status"}`.
    pub fn restore(
        &self,
        drive_id: &str,
        drive_token: &str,
        prefix: &str,
        as_of: &str,
    ) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}/restore", check_id(drive_id)?);
        let body = json!({ "prefix": prefix, "as_of": as_of });
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))
    }

    /// A restore's progress (`GET /v1/drives/{id}/restore/{request}`).
    pub fn restore_status(
        &self,
        drive_id: &str,
        drive_token: &str,
        request: &str,
    ) -> Result<Value, TokenError> {
        let path = format!(
            "/v1/drives/{}/restore/{}",
            check_id(drive_id)?,
            check_id(request)?
        );
        self.call_or_null(Method::Get, &path, Some(token_of(drive_token)?), None)
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
    let ok = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !ok {
        return Err(TokenError::Config(format!(
            "{id:?} is not a drive or request id"
        )));
    }
    Ok(id)
}
