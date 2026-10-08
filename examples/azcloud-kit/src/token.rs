//! The token server's HTTP API, as the desktop apps use it (JSON both ways, errors as
//! `{"error": "<code>", "message": "<sentence>"}`):
//!
//! | call                                  | what for                                          |
//! |---------------------------------------|---------------------------------------------------|
//! | `GET /v1/tiers`                       | the storage tiers and their prices                |
//! | `POST /v1/drives`                     | a drive without payment (development servers)     |
//! | `POST /v1/checkout`                   | a paid drive: where the browser pays              |
//! | `GET /v1/checkout/{id}`               | pending / approved (the drive, once) / declined   |
//! | `POST /v1/drives/{id}/credentials`    | fresh credentials for the drive token (rotates)   |
//! | `GET /v1/drives/{id}`                 | the drive's tier, quota, members, lockdown        |
//! | `POST /v1/drives/{id}/members`        | a token family for another device to join with    |
//! | `POST /v1/drives/{id}/lockdown`       | every other device, key and link revoked at once  |
//! | `POST /v1/drives/{id}/lockdown/cancel`| a pending recovery-key lockdown called off        |
//! | `POST /v1/drives/{id}/restore`        | a prefix as it was at a time (queued)             |
//! | `GET /v1/drives/{id}/restore/{req}`   | a restore's progress                              |
//!
//! Blocking, through azul-storage's [`Transport`]: call it from an azul `Thread`.

use std::fmt;

use azul_storage::{sigv4::uri_encode, HttpCall, HttpReply, Method, Transport};
use serde_json::{json, Value};

use crate::bundle::DriveBundle;

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
    /// Paid: the new drive (the token server hands it over ONCE - keep it).
    Approved(Box<DriveBundle>),
    /// Paid, and the drive was handed over to an earlier poll.
    ApprovedElsewhere,
    /// The payment was declined or reversed: why.
    Declined(String),
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

/// The token server's error answer as a [`TokenError`]: a refused drive token is
/// [`TokenError::SignIn`], every other refusal [`TokenError::Refused`].
fn refusal(reply: &HttpReply) -> TokenError {
    let value: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
    let code = value["error"].as_str().unwrap_or_default().to_string();
    let message = value["message"].as_str().unwrap_or_default().to_string();
    if matches!(reply.status, 401 | 403) {
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

/// The token server at a base address. Blocking: call it from an azul `Thread`.
pub struct TokenServer<'a> {
    base: String,
    transport: &'a dyn Transport,
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
        })
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
            return Err(refusal(&reply));
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
    /// `card`, ...): where the browser pays. The drive comes with [`Self::checkout_status`].
    pub fn checkout(&self, tier: &str, months: u32, method: &str) -> Result<Checkout, TokenError> {
        let body = json!({ "tier": tier.trim(), "months": months, "method": method.trim() });
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

    /// Where the checkout `checkout_id` stands; once it is paid, the new drive (handed over
    /// ONCE: the next poll answers [`CheckoutStatus::ApprovedElsewhere`]).
    pub fn checkout_status(&self, checkout_id: &str) -> Result<CheckoutStatus, TokenError> {
        let id = checkout_id.trim();
        if id.is_empty() {
            return Err(TokenError::Config(String::from("There is no checkout to ask about.")));
        }
        let path = format!("/v1/checkout/{}", uri_encode(id, true));
        let value = self.call(Method::Get, &path, None, None)?;
        match value["status"].as_str().unwrap_or_default() {
            "pending" => Ok(CheckoutStatus::Pending),
            "approved" => match value.get("signup").filter(|s| s.is_object()) {
                Some(signup) => Ok(CheckoutStatus::Approved(Box::new(
                    DriveBundle::from_value(signup)?,
                ))),
                None => Ok(CheckoutStatus::ApprovedElsewhere),
            },
            "declined" | "reversed" => Ok(CheckoutStatus::Declined(
                value["reason"]
                    .as_str()
                    .unwrap_or("the payment was declined")
                    .to_string(),
            )),
            other => Err(TokenError::Protocol(format!(
                "the checkout is \"{other}\", neither pending, approved nor declined"
            ))),
        }
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
        let value = self.call(Method::Post, &path, Some(token), Some(body))?;
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
