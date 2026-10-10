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
//! |                                       | / declined / expired; a cash checkout             |
//! |                                       | awaiting_cash / rejected (with its reason)        |
//! | `GET /v1/checkout/options`            | the providers' offer for a tier, period, country  |
//! |                                       | (azul-pay narrows it to the app's registry)       |
//! | `POST /v1/checkout` + provider        | a checkout through a provider: its surface (the   |
//! |                                       | fields page, a hosted page, the browser)          |
//! | `POST /v1/checkout/{id}/surface`      | the same checkout on the next surface             |
//! | `POST /v1/checkout/{id}/abandon`      | the provider session expires (popover closed)     |
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
    /// Whether the token server takes cash by post (its methods name `cash`: it has its cash key
    /// and the operator's address; cash contract v1).
    #[must_use]
    pub fn takes_cash(&self) -> bool {
        self.methods
            .iter()
            .any(|m| m.trim() == crate::cash::CASH_METHOD)
    }

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

/// What `GET /v1/checkout/options` is asked for (CHECKOUT-PLAN §3.11): the tier, the months
/// paid at once, the payer's country and currency, the surfaces this app can show (`fields`,
/// `page`, `browser`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionsQuery<'a> {
    pub tier: &'a str,
    pub months: u32,
    pub country: &'a str,
    pub currency: &'a str,
    pub surfaces: &'a [&'a str],
}

/// How a checkout is paid (claim contract v1, extended): the provider and method of the payer's
/// pill, the first surface to open, the VAT country, the consent the order needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckoutVia<'a> {
    pub provider: &'a str,
    pub method: &'a str,
    pub surface: &'a str,
    pub vat_country: &'a str,
    pub withdrawal_consent: bool,
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

/// What `POST /v1/tokens/issue` answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueAnswer {
    /// The blind signatures, by the key the request named.
    Signed(BlindSignatures),
    /// 409 `key_changed`: the request named a key the token server no longer signs with
    /// (the year turned since `GET /v1/tokens/keys`); nothing was signed or counted. Blind the
    /// messages again for `key_id` - with `public_key_pem` when the server sent it, else from
    /// the keys read again.
    KeyChanged {
        key_id: String,
        public_key_pem: Option<String>,
    },
}

/// What a recovery-key lockdown answers (202): the drive is read-only until `pending_until`
/// (the recovery code may cancel until then), and `drive_token` is the new family this side gets
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

/// The member a recovery-key lockdown adds while it is pending: the device that holds the
/// recovery code, the owner's when the 48 hours are over.
pub const RECOVERY_MEMBER: &str = "recovery-pending";

/// What `POST /v1/drives/{id}/claim` answers: a token family of the claiming computer's own.
/// `Debug` shows no token.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaimedFamily {
    /// `owner`.
    pub member: String,
    pub drive_token: String,
    /// The pick-ups the ticket still takes.
    pub claims_left: Option<u32>,
}

impl fmt::Debug for ClaimedFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaimedFamily")
            .field("member", &self.member)
            .field("drive_token", &"<hidden>")
            .field("claims_left", &self.claims_left)
            .finish()
    }
}

/// A drive as the token server keeps it ([`TokenServer::drive_status`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DriveStatus {
    /// `100GB`.
    pub tier: Option<String>,
    /// Paid until, in seconds since 1970.
    pub period_until: Option<u64>,
    /// A recovery-key lockdown takes effect then (seconds since 1970) unless a device of the
    /// owner cancels it with the recovery code ([`TokenServer::lockdown_cancel_signed`]).
    pub lockdown_pending_until: Option<u64>,
    /// The drive takes no writes (unpaid past its grace, a pending lockdown).
    pub read_only: bool,
    /// Every member the drive was given (its devices; [`RECOVERY_MEMBER`] while a recovery-key
    /// lockdown is pending), sorted.
    pub members: Vec<String>,
    /// The member whose token asked.
    pub you: Option<String>,
    /// The drive's quota in bytes - of STORED bytes: what arrives at the storage nodes,
    /// compressed and encrypted.
    pub quota_bytes: Option<u64>,
    /// The bytes the drive holds as the storage nodes count them (stored: compressed and
    /// encrypted); what the quota is counted against.
    pub used_bytes: Option<u64>,
    /// The drive is banned (ban contract v1): why, and until when it may still be read.
    pub ban: Option<Ban>,
}

/// The error code of a banned drive's refusals: its writes during the grace period, everything
/// (the credentials, its status) after it.
pub const DRIVE_BANNED: &str = "drive_banned";

/// The banner over a banned drive: why, and how long its files can still be copied (`hours`;
/// `None`: no end said). AzDrive and AzMail show the same words.
#[must_use]
pub fn banner_text(reason: &str, hours: Option<u64>) -> String {
    match hours {
        Some(1) => format!(
            "Due to {reason}, your account has been banned, but you have 1 hour to migrate \
             your files."
        ),
        Some(hours) => format!(
            "Due to {reason}, your account has been banned, but you have {hours} hours to \
             migrate your files."
        ),
        None => format!(
            "Due to {reason}, your account has been banned: copy your files to this computer \
             now."
        ),
    }
}

/// What a banned drive shows past its end: when (`until`, the ban's end) and why.
#[must_use]
pub fn closed_text(until: Option<u64>, reason: &str) -> String {
    match until {
        Some(until) => {
            let when = azul_storage::time::iso8601(until);
            format!(
                "This drive was closed on {} because {reason}.",
                when.get(..10).unwrap_or(&when)
            )
        }
        None => format!("This drive was closed because {reason}."),
    }
}

/// What the token server says when a ban has no reason of its own.
const BAN_REASON: &str = "a breach of the terms of service";

/// A ban's words: (message id, English, German) - [`ban_fluent_source`].
const BAN_WORDS: [(&str, &str, &str); 5] = [
    (
        "azlin-ban-banner",
        "Due to { $reason }, your account has been banned, but you have { $hours ->\n        \
         [one] 1 hour\n       *[other] { $hours } hours\n    } to migrate your files.",
        "Wegen { $reason } wurde dein Konto gesperrt, aber du hast noch { $hours ->\n        \
         [one] 1 Stunde\n       *[other] { $hours } Stunden\n    }, um deine Dateien zu \
         übertragen.",
    ),
    (
        "azlin-ban-banner-no-end",
        "Due to { $reason }, your account has been banned: copy your files to this computer now.",
        "Wegen { $reason } wurde dein Konto gesperrt: Kopiere deine Dateien jetzt auf diesen \
         Computer.",
    ),
    (
        "azlin-ban-closed",
        "This drive was closed on { $day } because { $reason }.",
        "Dieses Laufwerk wurde am { $day } geschlossen, wegen { $reason }.",
    ),
    (
        "azlin-ban-closed-no-day",
        "This drive was closed because { $reason }.",
        "Dieses Laufwerk wurde geschlossen, wegen { $reason }.",
    ),
    (
        "azlin-ban-reason-terms",
        BAN_REASON,
        "eines Verstoßes gegen die Nutzungsbedingungen",
    ),
];

/// A ban's words as a Fluent resource in `lang`: an app gives it to the engine with the error
/// table and says a ban by [`Ban::banner_phrase`] / [`Ban::closed_phrase`].
#[must_use]
pub fn ban_fluent_source(lang: crate::user_errors::Lang) -> String {
    let mut out = String::from("# A banned drive's words (azcloud-kit token).\n");
    for (id, en, de) in BAN_WORDS {
        let text = match lang {
            crate::user_errors::Lang::En => en,
            crate::user_errors::Lang::De => de,
        };
        out.push_str(&format!("{id} = {text}\n"));
    }
    out
}

/// A ban's reason as a phrase's argument: the default one a word of [`BAN_WORDS`] (said in the
/// window's language), the token server's own as it is.
#[must_use]
pub fn ban_reason_arg(reason: &str) -> azul_appkit::phrase::Arg {
    if reason == BAN_REASON {
        azul_appkit::phrase::Arg::word("azlin-ban-reason-terms", BAN_REASON)
    } else {
        azul_appkit::phrase::Arg::from(reason)
    }
}

/// [`banner_text`] as a phrase of [`BAN_WORDS`].
#[must_use]
pub fn banner_phrase(reason: &str, hours: Option<u64>) -> azul_appkit::phrase::Phrase {
    use azul_appkit::phrase::Phrase;
    match hours {
        Some(hours) => Phrase::new("azlin-ban-banner")
            .arg("reason", ban_reason_arg(reason))
            .arg("hours", hours),
        None => Phrase::new("azlin-ban-banner-no-end").arg("reason", ban_reason_arg(reason)),
    }
}

/// [`closed_text`] as a phrase of [`BAN_WORDS`].
#[must_use]
pub fn closed_phrase(until: Option<u64>, reason: &str) -> azul_appkit::phrase::Phrase {
    use azul_appkit::phrase::Phrase;
    match until {
        Some(until) => {
            let when = azul_storage::time::iso8601(until);
            Phrase::new("azlin-ban-closed")
                .arg("day", when.get(..10).unwrap_or(&when))
                .arg("reason", ban_reason_arg(reason))
        }
        None => Phrase::new("azlin-ban-closed-no-day").arg("reason", ban_reason_arg(reason)),
    }
}

/// A drive banned with a grace period (ban contract v1): its files can be read, listed and
/// copied until `until`; nothing new is written; then the drive is closed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ban {
    /// Why, as the token server says it ("spam distribution").
    pub reason: String,
    /// Reads and credentials work until then (seconds since 1970); `None`: no end said.
    pub until: Option<u64>,
    /// The token server refused the drive past the end (403 `drive_banned`): it is closed,
    /// whatever this computer's clock says.
    pub closed: bool,
}

impl Ban {
    /// The ban an answer describes - `GET /v1/drives/{id}`, a credentials bundle (`status:
    /// "banned"`, `ban_reason`, `ban_until`) or a 403 `drive_banned`; `None` for a drive in good
    /// standing.
    #[must_use]
    pub fn of(value: &Value) -> Option<Ban> {
        let banned = value["status"].as_str() == Some("banned")
            || value["error"].as_str() == Some(DRIVE_BANNED)
            || value["ban_reason"].is_string();
        if !banned {
            return None;
        }
        let reason = value["ban_reason"]
            .as_str()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .unwrap_or(BAN_REASON);
        let until = value["ban_until"].as_u64().or_else(|| {
            value["ban_until"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601)
        });
        Some(Ban {
            reason: reason.chars().take(200).collect(),
            until,
            closed: value["error"].as_str() == Some(DRIVE_BANNED),
        })
    }

    /// Whether the drive is closed at `now` (seconds since 1970): the token server said so, or
    /// its end has come.
    #[must_use]
    pub fn is_closed(&self, now: u64) -> bool {
        self.closed || self.until.is_some_and(|until| until <= now)
    }

    /// What every app shows over the banned drive at `now`: [`banner_text`] with the hours left,
    /// then, past the end, [`closed_text`].
    #[must_use]
    pub fn banner(&self, now: u64) -> String {
        if self.is_closed(now) {
            return self.closed_text();
        }
        banner_text(&self.reason, self.until.map(|_| self.hours_left(now)))
    }

    /// "This drive was closed on <date> because <reason>." ([`closed_text`]).
    #[must_use]
    pub fn closed_text(&self) -> String {
        closed_text(self.until, &self.reason)
    }

    /// [`Ban::banner`] as a phrase of the kit's messages (an app says it in the window's
    /// language).
    #[must_use]
    pub fn banner_phrase(&self, now: u64) -> azul_appkit::phrase::Phrase {
        if self.is_closed(now) {
            return self.closed_phrase();
        }
        banner_phrase(&self.reason, self.until.map(|_| self.hours_left(now)))
    }

    /// [`Ban::closed_text`] as a phrase of the kit's messages.
    #[must_use]
    pub fn closed_phrase(&self) -> azul_appkit::phrase::Phrase {
        closed_phrase(self.until, &self.reason)
    }

    /// The hours left at `now` to copy the files, rounded up (0 once it is closed, and when no
    /// end was said).
    #[must_use]
    pub fn hours_left(&self, now: u64) -> u64 {
        if self.is_closed(now) {
            return 0;
        }
        self.until
            .map_or(0, |until| until.saturating_sub(now).div_ceil(3_600))
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

/// What a cancel of a recovery-key lockdown signs (F12: "the recovery code always wins"):
/// `lockdown-cancel:<drive>:<nonce>`.
#[must_use]
pub fn lockdown_cancel_message(drive_id: &str, nonce: &str) -> String {
    format!("lockdown-cancel:{drive_id}:{nonce}")
}

/// What a change of the recovery key signs with the CURRENT key (F12 C):
/// `recovery:<drive>:<new key>:<nonce>`.
#[must_use]
pub fn recovery_key_message(drive_id: &str, new_key: &str, nonce: &str) -> String {
    format!("recovery:{drive_id}:{new_key}:{nonce}")
}

/// What a recovery lookup signs: `recovery-lookup:<challenge>`.
#[must_use]
pub fn recovery_lookup_message(challenge: &str) -> String {
    format!("recovery-lookup:{challenge}")
}

/// What adding a recovery key signs (by a current key of the drive): `recovery-add:<drive>:<the
/// new key as sent>:<nonce>`.
#[must_use]
pub fn recovery_add_message(drive_id: &str, public_key_base64: &str, nonce: &str) -> String {
    format!("recovery-add:{drive_id}:{public_key_base64}:{nonce}")
}

/// What removing one signs: `recovery-remove:<drive>:<key_id>:<nonce>`.
#[must_use]
pub fn recovery_remove_message(drive_id: &str, key_id: &str, nonce: &str) -> String {
    format!("recovery-remove:{drive_id}:{key_id}:{nonce}")
}

/// A drive a recovery key belongs to ([`TokenServer::recovery_lookup`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDrive {
    pub drive_id: String,
    /// The key's id at the drive (`rk_...`; `rk_legacy` for a key from before several keys).
    pub key_id: String,
}

/// One of a drive's recovery keys as the token server lists it ([`TokenServer::recovery_keys`]):
/// public halves only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryKeyInfo {
    pub key_id: String,
    /// The owner's words (`recovery code` for the kit's).
    pub label: String,
    /// Ed25519, standard base64.
    pub recovery_pubkey: String,
    /// The day it was added (seconds since 1970); none for a key from before.
    pub created_at: Option<u64>,
    /// A token master MAC'd it: it signs lockdowns and lookups (not yet: 503 `not_verified`).
    pub verified: bool,
}

impl RecoveryKeyInfo {
    fn from_value(value: &Value) -> RecoveryKeyInfo {
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        RecoveryKeyInfo {
            key_id: text("key_id"),
            label: text("label"),
            recovery_pubkey: text("recovery_pubkey"),
            created_at: value["created_at"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601),
            verified: value["verified"].as_bool().unwrap_or(false),
        }
    }
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
        let reply = self.exchange(method, path, bearer, body)?;
        if !reply.is_success() {
            return Err(refusal(&reply, bearer.is_some()));
        }
        Ok(reply)
    }

    /// One request and whatever it answers (a refusal too); only no answer is an error.
    fn exchange(
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
        self.transport.send(&call).map_err(TokenError::Connect)
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
        let checkout = checkout_of(&value, months);
        if checkout.checkout_id.is_empty() || checkout.pay_url.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the checkout has no id or no payment page",
            )));
        }
        Ok(checkout)
    }

    /// The payment options for `query` (`GET /v1/checkout/options`): the offer's JSON text, for
    /// azul-pay to read and narrow; `None` from a token server without them (a 404: an older one,
    /// whose checkout is claim contract v1's - a payment page in the browser).
    pub fn checkout_options(&self, query: &OptionsQuery<'_>) -> Result<Option<String>, TokenError> {
        let path = format!(
            "/v1/checkout/options?tier={}&months={}&country={}&currency={}&surfaces={}",
            uri_encode(query.tier.trim(), true),
            query.months,
            uri_encode(&query.country.trim().to_ascii_uppercase(), true),
            uri_encode(query.currency.trim(), true),
            uri_encode(&query.surfaces.join(","), true),
        );
        match self.call(Method::Get, &path, None, None) {
            Ok(value) => Ok(Some(value.to_string())),
            Err(TokenError::Refused { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// A checkout of `tier` for `months` months paid through `via` (claim contract v1,
    /// extended): its sign-up sealed to `claim` (the public half is sent), and the whole answer -
    /// its `surface` (a fields page with a client secret, a hosted page, a page for the browser)
    /// and `return` pages, for azul-pay to check. The answer holds a client secret: never print
    /// or `Debug` it.
    ///
    /// # Errors
    ///
    /// No answer, a refusal, or an answer without a checkout id.
    pub fn checkout_via(
        &self,
        tier: &str,
        months: u32,
        via: &CheckoutVia<'_>,
        claim: &ClaimKey,
    ) -> Result<(Checkout, Value), TokenError> {
        let body = json!({
            "tier": tier.trim(),
            "months": months,
            "provider": via.provider.trim(),
            "method": via.method.trim(),
            "surface": via.surface.trim(),
            "vat_country": via.vat_country.trim().to_ascii_uppercase(),
            "withdrawal_consent": via.withdrawal_consent,
            "claim_key": claim.public_base64(),
        });
        let value = self.call(Method::Post, "/v1/checkout", None, Some(&body))?;
        let checkout = checkout_of(&value, months);
        if checkout.checkout_id.is_empty() {
            return Err(TokenError::Protocol(String::from("the checkout has no id")));
        }
        Ok((checkout, value))
    }

    /// A cash checkout of `tier` for `months` months (cash contract v1): `POST /v1/checkout
    /// {"method": "cash"}` with the VAT country and the order's consent, its sign-up sealed to
    /// `claim` - no provider, no surface: the token server answers the activation code, the
    /// address to post the cash to and the checkout's end, all in the answer for azul-pay to
    /// read. 400 `cash_unavailable` from a server that takes no cash.
    ///
    /// # Errors
    ///
    /// No answer, a refusal, or an answer without a checkout id.
    pub fn checkout_cash(
        &self,
        tier: &str,
        months: u32,
        vat_country: &str,
        withdrawal_consent: bool,
        claim: &ClaimKey,
    ) -> Result<(Checkout, Value), TokenError> {
        let body = json!({
            "tier": tier.trim(),
            "months": months,
            "method": crate::cash::CASH_METHOD,
            "vat_country": vat_country.trim().to_ascii_uppercase(),
            "withdrawal_consent": withdrawal_consent,
            "claim_key": claim.public_base64(),
        });
        let value = self.call(Method::Post, "/v1/checkout", None, Some(&body))?;
        let checkout = checkout_of(&value, months);
        if checkout.checkout_id.is_empty() {
            return Err(TokenError::Protocol(String::from("the checkout has no id")));
        }
        Ok((checkout, value))
    }

    /// A token family of this computer's own for the paid drive `drive_id`, claimed with its
    /// sealed sign-up's `ticket` (`POST /v1/drives/{id}/claim {"ticket"}`, no drive token): what
    /// a computer that picks the drive up by its claim code spends, so it and the buyer's
    /// computer never share a family.
    ///
    /// # Errors
    ///
    /// The token server's refusal: 401 another ticket (or a drive from before the tickets), 409
    /// `claims_used`, 410 `claim_expired`, 403 `drive_banned`; no answer.
    pub fn claim_drive(&self, drive_id: &str, ticket: &str) -> Result<ClaimedFamily, TokenError> {
        let path = format!("/v1/drives/{}/claim", check_id(drive_id)?);
        let body = json!({ "ticket": ticket.trim() });
        let value = self.call(Method::Post, &path, None, Some(&body))?;
        let drive_token = value["drive_token"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        if drive_token.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the claim answered no drive token",
            )));
        }
        Ok(ClaimedFamily {
            member: value["member"].as_str().unwrap_or("owner").to_string(),
            drive_token,
            claims_left: value["claims_left"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok()),
        })
    }

    /// The checkout `checkout_id` on another surface (`POST /v1/checkout/{id}/surface`, the
    /// fallback chain: the hosted page, the browser): the answer, its `surface` for azul-pay to
    /// check. It holds a client secret: never print it.
    pub fn checkout_surface(&self, checkout_id: &str, kind: &str) -> Result<Value, TokenError> {
        let path = format!("/v1/checkout/{}/surface", check_id(checkout_id.trim())?);
        self.call_or_null(Method::Post, &path, None, Some(&json!({ "kind": kind.trim() })))
    }

    /// Abandons the checkout `checkout_id` (`POST /v1/checkout/{id}/abandon`): the provider
    /// session expires, so a forgotten browser tab can never take money for it.
    pub fn abandon_checkout(&self, checkout_id: &str) -> Result<(), TokenError> {
        let path = format!("/v1/checkout/{}/abandon", check_id(checkout_id.trim())?);
        self.send(Method::Post, &path, None, Some(&json!({})))
            .map(|_| ())
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
            // A cash checkout waits for its letter like any unpaid one (cash contract v1).
            "pending" | "awaiting_cash" => Ok(CheckoutStatus::Pending),
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
            // The operator did not take the letter's cash (the amount, the slip).
            "rejected" => Ok(CheckoutStatus::Declined(reason(
                "the cash sent was not accepted",
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
    /// issues nothing. `key_id` names the issuer key the messages were blinded for
    /// (`GET /v1/tokens/keys`); when it is no longer the one the token server signs with, the
    /// answer is [`IssueAnswer::KeyChanged`] and nothing is signed or counted. The same request
    /// again (same key, same messages, same order) is answered the same, counted once: what to
    /// send when an answer was lost.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] without an issue key, a key id or messages (nothing is sent); the
    /// token server's refusals with their codes - `issue_key_required`, `key_id_required`,
    /// `issue_key_wrong` (403, no sign-in matter), `already_issued`, `not_paid`,
    /// `mandate_stopped`; an answer whose signatures do not match the messages.
    pub fn issue_period_tokens(
        &self,
        checkout_id: &str,
        issue_key: &str,
        key_id: &str,
        blinded: &[String],
    ) -> Result<IssueAnswer, TokenError> {
        let issue_key = issue_key.trim();
        if issue_key.is_empty() {
            return Err(TokenError::Config(String::from(
                "There is no issue key: a checkout's period tokens are issued only against the \
                 key its sealed sign-up carries.",
            )));
        }
        let key_id = key_id.trim();
        if key_id.is_empty() {
            return Err(TokenError::Config(String::from(
                "Period tokens name the issuer key they are blinded for.",
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
            "key_id": key_id,
            "blinded": blinded,
        });
        let reply = self.exchange(Method::Post, "/v1/tokens/issue", None, Some(&body))?;
        if !reply.is_success() {
            let value: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
            if reply.status == 409 && value["error"] == "key_changed" {
                return Ok(IssueAnswer::KeyChanged {
                    key_id: value["key_id"].as_str().unwrap_or_default().trim().to_string(),
                    public_key_pem: value["public_key_pem"]
                        .as_str()
                        .filter(|pem| !pem.trim().is_empty())
                        .map(str::to_string),
                });
            }
            return Err(refusal(&reply, false));
        }
        let value: Value = serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))?;
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
        Ok(IssueAnswer::Signed(BlindSignatures {
            tier: text("tier"),
            key_id: text("key_id"),
            public_key_pem: text("public_key_pem"),
            signatures,
        }))
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

    /// [`Self::info`] as a [`DriveStatus`]: the tier, the period's end, a pending recovery-key
    /// lockdown, whether the drive takes no writes, its members and which one asked, a ban.
    /// Past a ban's end the token server refuses the drive (403 `drive_banned`): that answer is
    /// the status of a closed drive, not an error.
    pub fn drive_status(
        &self,
        drive_id: &str,
        drive_token: &str,
    ) -> Result<DriveStatus, TokenError> {
        let path = format!("/v1/drives/{}", check_id(drive_id)?);
        let reply = self.exchange(Method::Get, &path, Some(token_of(drive_token)?), None)?;
        if reply.status == 403 {
            let value: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
            if value["error"].as_str() == Some(DRIVE_BANNED) {
                return Ok(DriveStatus {
                    read_only: true,
                    ban: Ban::of(&value),
                    ..DriveStatus::default()
                });
            }
        }
        if !reply.is_success() {
            return Err(refusal(&reply, true));
        }
        let info: Value = if reply.body.iter().all(u8::is_ascii_whitespace) {
            Value::Null
        } else {
            serde_json::from_slice(&reply.body)
                .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))?
        };
        let time = |key: &str| info[key].as_str().and_then(azul_storage::time::parse_iso8601);
        let mut members: Vec<String> = info["members"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|m| m["member"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        members.sort();
        members.dedup();
        Ok(DriveStatus {
            tier: info["tier"].as_str().map(str::to_string),
            period_until: time("period_until"),
            lockdown_pending_until: time("lockdown_pending_until"),
            read_only: info["read_only"].as_bool().unwrap_or(false),
            members,
            you: info["you"].as_str().map(str::to_string),
            quota_bytes: info["quota_bytes"].as_u64(),
            // Stored bytes (compressed and encrypted, what arrived); `stored_bytes` is the same
            // count under the name some servers give it.
            used_bytes: info["used_bytes"]
                .as_u64()
                .or_else(|| info["stored_bytes"].as_u64()),
            ban: Ban::of(&info),
        })
    }

    /// Registers the drive's recovery key (`POST /v1/drives/{id}/recovery {"recovery_pubkey"}`,
    /// [`crate::recovery::RecoveryKey::public_base64`]): what a lockdown without a drive token
    /// is signed with. A grant: send it with the newest drive token
    /// ([`crate::SharedKeyring::with_drive_token`]).
    pub fn set_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        public_key_base64: &str,
    ) -> Result<(), TokenError> {
        let path = format!("/v1/drives/{}/recovery", check_id(drive_id)?);
        let body = json!({ "recovery_pubkey": public_key_base64.trim() });
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))
            .map(|_| ())
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

    /// The drives the recovery key `public_key_base64` belongs to, for a computer that never had
    /// them (§18.8: the kit leaves the drive id out): a challenge (`POST /v1/recovery/challenge`),
    /// `sign` signs [`recovery_lookup_message`] of it with the key, then `POST
    /// /v1/recovery/lookup`. No drive token. An empty list: no drive has the key.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (the lookup is not sent); the token
    /// server's refusals (401 `bad_challenge` or `unauthorized`, 503 `not_verified`, 429
    /// `rate_limited`).
    pub fn recovery_lookup(
        &self,
        public_key_base64: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<Vec<FoundDrive>, TokenError> {
        let value = self.call(Method::Post, "/v1/recovery/challenge", None, Some(&json!({})))?;
        let challenge = value["challenge"].as_str().unwrap_or_default().to_string();
        if challenge.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the recovery challenge is missing",
            )));
        }
        let signature = sign(recovery_lookup_message(&challenge).as_bytes())
            .map_err(|e| TokenError::Config(format!("The lookup is not signed: {e}")))?;
        let body = json!({
            "recovery_pubkey": public_key_base64.trim(),
            "challenge": challenge,
            "signature": signature,
        });
        let value = self.call(Method::Post, "/v1/recovery/lookup", None, Some(&body))?;
        Ok(value["drives"]
            .as_array()
            .map(|drives| {
                drives
                    .iter()
                    .filter_map(|d| {
                        Some(FoundDrive {
                            drive_id: d["drive_id"].as_str()?.to_string(),
                            key_id: d["key_id"].as_str().unwrap_or_default().to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The drive's recovery keys (`GET /v1/drives/{id}/recovery_keys`, a read).
    pub fn recovery_keys(
        &self,
        drive_id: &str,
        drive_token: &str,
    ) -> Result<Vec<RecoveryKeyInfo>, TokenError> {
        let path = format!("/v1/drives/{}/recovery_keys", check_id(drive_id)?);
        let value = self.call(Method::Get, &path, Some(token_of(drive_token)?), None)?;
        Ok(value["keys"]
            .as_array()
            .map(|keys| keys.iter().map(RecoveryKeyInfo::from_value).collect())
            .unwrap_or_default())
    }

    /// One more recovery key of the drive (`POST /v1/drives/{id}/recovery_keys`, a grant): `sign`
    /// signs [`recovery_add_message`] with a CURRENT key of the drive (F12: the drive token
    /// alone adds none once the drive has a key).
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals (403 `recovery_key_required`, 401 a key that is not the drive's, 409
    /// `recovery_key_exists` / `too_many_recovery_keys`, 403 `lockdown_pending`).
    pub fn add_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        public_key_base64: &str,
        label: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<RecoveryKeyInfo, TokenError> {
        let id = check_id(drive_id)?;
        let token = token_of(drive_token)?;
        let public = public_key_base64.trim();
        let nonce = lockdown_nonce()?;
        let signature = sign(recovery_add_message(id, public, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The new key is not signed: {e}")))?;
        let body = json!({
            "recovery_pubkey": public,
            "label": label.trim(),
            "nonce": nonce,
            "signature": signature,
        });
        let path = format!("/v1/drives/{id}/recovery_keys");
        let value = self.call(Method::Post, &path, Some(token), Some(&body))?;
        Ok(RecoveryKeyInfo::from_value(&value))
    }

    /// One recovery key fewer (`DELETE /v1/drives/{id}/recovery_keys/{key_id}`, a grant): `sign`
    /// signs [`recovery_remove_message`] with a current key. Never the drive's last key.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] for a key id that is no path segment, or when `sign` cannot sign
    /// (nothing is sent); the token server's refusals (409 `last_recovery_key`, 404
    /// `no_such_key`, 403 `recovery_key_required`).
    pub fn remove_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        key_id: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<(), TokenError> {
        let id = check_id(drive_id)?;
        let key = check_id(key_id)?;
        let token = token_of(drive_token)?;
        let nonce = lockdown_nonce()?;
        let signature = sign(recovery_remove_message(id, key, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The removal is not signed: {e}")))?;
        let body = json!({ "nonce": nonce, "signature": signature });
        let path = format!("/v1/drives/{id}/recovery_keys/{key}");
        self.call_or_null(Method::Delete, &path, Some(token), Some(&body))
            .map(|_| ())
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

    /// Cancels a pending recovery-key lockdown (`POST /v1/drives/{id}/lockdown/cancel {"nonce",
    /// "signature"}`, F12 - "the recovery code always wins"): `sign` signs
    /// [`lockdown_cancel_message`] of a nonce made new for this request with the drive's
    /// recovery key (whoever holds the code; a device's drive token alone cancels nothing, so no
    /// token is sent). A 409 `no_pending_lockdown` when none is pending, `nonce_used` for a
    /// request the server saw before.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign` cannot sign (nothing is sent); the token server's
    /// refusals.
    pub fn lockdown_cancel_signed(
        &self,
        drive_id: &str,
        sign: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<Value, TokenError> {
        let id = check_id(drive_id)?;
        let nonce = lockdown_nonce()?;
        let signature = sign(lockdown_cancel_message(id, &nonce).as_bytes())
            .map_err(|e| TokenError::Config(format!("The cancel request is not signed: {e}")))?;
        let path = format!("/v1/drives/{id}/lockdown/cancel");
        let body = json!({ "nonce": nonce, "signature": signature });
        self.call_or_null(Method::Post, &path, None, Some(&body))
    }

    /// Replaces the drive's recovery key with `public_key_base64` (`POST
    /// /v1/drives/{id}/recovery {"recovery_pubkey", "nonce", "signature"}`, F12 C): the CURRENT
    /// key signs [`recovery_key_message`] of the new key and a fresh nonce - a thief with a
    /// device token cannot swap the owner's code out. A grant: with the newest drive token. The
    /// drive's first key needs no signature ([`Self::set_recovery_key`]).
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] when `sign_with_current` cannot sign (nothing is sent); the token
    /// server's refusals (401 a signature of another key).
    pub fn replace_recovery_key(
        &self,
        drive_id: &str,
        drive_token: &str,
        public_key_base64: &str,
        sign_with_current: impl FnOnce(&[u8]) -> Result<String, String>,
    ) -> Result<(), TokenError> {
        let id = check_id(drive_id)?;
        let new_key = public_key_base64.trim();
        let nonce = lockdown_nonce()?;
        let signature = sign_with_current(recovery_key_message(id, new_key, &nonce).as_bytes())
            .map_err(|e| {
                TokenError::Config(format!("The new recovery key is not signed: {e}"))
            })?;
        let path = format!("/v1/drives/{id}/recovery");
        let body = json!({ "recovery_pubkey": new_key, "nonce": nonce, "signature": signature });
        self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))
            .map(|_| ())
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

/// The checkout an answer of `POST /v1/checkout` describes (`months`: asked for, when it names
/// none). Its id and payment page may be empty: the caller says which it needs.
fn checkout_of(value: &Value, months: u32) -> Checkout {
    let text = |key: &str| value[key].as_str().unwrap_or_default().trim().to_string();
    Checkout {
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
