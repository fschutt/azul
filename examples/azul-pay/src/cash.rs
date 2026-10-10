//! Cash by post (cash contract v1): the checkout the token server makes for `method: "cash"`
//! and what the app prints of it. Nothing opens - no web view, no browser: the buyer prints two
//! pages, keeps one and posts the other with the cash; the operator activates the checkout when
//! the letter arrived, and the drive follows by the claim (claim contract v1, azcloud-kit).
//!
//! ```json
//! {"checkout_id": "ck_...", "status": "awaiting_cash", "amount_cents": 990,
//!  "currency": "EUR", "activation_code": "AZC1-MNVV-6YLB-...",
//!  "mail_to": {"name": "...", "lines": ["...", "..."]}, "expires_at": "2026-12-09T10:00:00Z"}
//! ```
//!
//! The activation code is `AZC1-` and the RFC 4648 base32 (upper case, no padding, blocks of
//! four joined by `-`) of the checkout id, the amount (u32, big endian), the currency (three
//! ASCII letters) and ten bytes of the token server's MAC over them: the operator's AzCtl reads
//! the slip back and checks it. This crate checks its shape only (azcloud-kit reads it); the
//! slip holds no secret of the drive.

use std::fmt::Write as _;

use serde_json::Value;

use crate::{
    offer::amount_text,
    pills::Choice,
    registry::{Method, SurfaceKind},
    surface::SurfaceError,
};

/// What every activation code starts with (its version).
pub const ACTIVATION_PREFIX: &str = "AZC1-";
/// The fewest base32 characters of an activation code: a checkout id of one byte, the amount,
/// the currency and the MAC (18 bytes).
const MIN_CODE_CHARS: usize = 29;
/// The longest line of the operator's address taken from the answer, in characters.
const MAX_LINE_CHARS: usize = 120;
/// The most lines of the address.
const MAX_LINES: usize = 8;

/// Where the slip goes: the operator's postal address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailTo {
    pub name: String,
    pub lines: Vec<String>,
}

/// A cash checkout the token server made, checked: what the two pages print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashSlip {
    pub checkout_id: String,
    pub amount_cents: u64,
    /// `EUR`.
    pub currency: String,
    /// `AZC1-...`, as the answer wrote it.
    pub activation_code: String,
    pub mail_to: MailTo,
    /// When the checkout ends unpaid (RFC 3339, as the answer wrote it).
    pub expires_at: Option<String>,
}

/// Whether `text` has an activation code's shape: [`ACTIVATION_PREFIX`], then blocks of four
/// upper-case base32 characters joined by `-` (the last one of one to four), enough of them for
/// a checkout id, an amount, a currency and a MAC, and a whole number of bytes.
#[must_use]
pub fn is_activation_code(text: &str) -> bool {
    let Some(rest) = text.strip_prefix(ACTIVATION_PREFIX) else {
        return false;
    };
    let blocks: Vec<&str> = rest.split('-').collect();
    let last = blocks.len() - 1;
    let shaped = blocks.iter().enumerate().all(|(at, block)| {
        let size_ok = if at == last {
            (1..=4).contains(&block.len())
        } else {
            block.len() == 4
        };
        size_ok
            && block
                .bytes()
                .all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b))
    });
    let chars = rest.len() - last;
    shaped && chars >= MIN_CODE_CHARS && matches!(chars % 8, 0 | 2 | 4 | 5 | 7)
}

/// A text of the answer: trimmed, at most `max` characters, no control characters; `None` for
/// none or an empty one.
fn text_of(value: &Value, max: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    if text.is_empty() || text.chars().count() > max || text.chars().any(char::is_control) {
        return None;
    }
    Some(text.to_string())
}

impl CashSlip {
    /// The answer of `POST /v1/checkout` for the cash `choice`.
    ///
    /// # Errors
    ///
    /// [`SurfaceError`]: the choice is no cash by post, the answer is for another provider or
    /// method or not awaiting the cash, or it lacks (or has a broken) checkout id, amount,
    /// currency, activation code or address.
    pub fn parse(answer: &Value, choice: &Choice) -> Result<CashSlip, SurfaceError> {
        if choice.method.method != Method::Cash
            || !choice.method.surfaces.contains(&SurfaceKind::Paper)
        {
            return Err(SurfaceError::NotOffered(SurfaceKind::Paper));
        }
        let id = answer["checkout_id"].as_str().unwrap_or_default().trim();
        if id.is_empty() {
            return Err(SurfaceError::Missing("checkout_id"));
        }
        if id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(SurfaceError::Field("checkout_id"));
        }
        if let Some(provider) = answer["provider"].as_str() {
            if provider != choice.provider.spec.id {
                return Err(SurfaceError::Mismatch(format!(
                    "the checkout is for {}, not {}",
                    shown(provider),
                    choice.provider.spec.id
                )));
            }
        }
        if let Some(method) = answer["method"].as_str() {
            if Method::parse(method) != Some(Method::Cash) {
                return Err(SurfaceError::Mismatch(format!(
                    "the checkout is paid by {}, not in cash",
                    shown(method)
                )));
            }
        }
        if let Some(status) = answer["status"].as_str() {
            if status != "awaiting_cash" {
                return Err(SurfaceError::Mismatch(format!(
                    "the new checkout is {}, not awaiting the cash",
                    shown(status)
                )));
            }
        }
        let amount_cents = answer["amount_cents"]
            .as_u64()
            .ok_or(SurfaceError::Missing("amount_cents"))?;
        if amount_cents == 0 || amount_cents > u64::from(u32::MAX) {
            return Err(SurfaceError::Field("amount_cents"));
        }
        let currency = answer["currency"]
            .as_str()
            .ok_or(SurfaceError::Missing("currency"))?;
        if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(SurfaceError::Field("currency"));
        }
        let code = answer["activation_code"]
            .as_str()
            .ok_or(SurfaceError::Missing("activation_code"))?
            .trim();
        if !is_activation_code(code) {
            return Err(SurfaceError::Field("activation_code"));
        }
        let mail_to = &answer["mail_to"];
        if !mail_to.is_object() {
            return Err(SurfaceError::Missing("mail_to"));
        }
        let name =
            text_of(&mail_to["name"], MAX_LINE_CHARS).ok_or(SurfaceError::Field("mail_to"))?;
        let lines = mail_to["lines"]
            .as_array()
            .ok_or(SurfaceError::Field("mail_to"))?
            .iter()
            .map(|line| text_of(line, MAX_LINE_CHARS))
            .collect::<Option<Vec<String>>>()
            .ok_or(SurfaceError::Field("mail_to"))?;
        if lines.is_empty() || lines.len() > MAX_LINES {
            return Err(SurfaceError::Field("mail_to"));
        }
        let expires_at = answer["expires_at"].as_str().and_then(|text| {
            let text = text.trim();
            (!text.is_empty() && text.len() <= 40 && text.bytes().all(|b| b.is_ascii_graphic()))
                .then(|| text.to_string())
        });
        Ok(CashSlip {
            checkout_id: id.to_string(),
            amount_cents,
            currency: currency.to_string(),
            activation_code: code.to_string(),
            mail_to: MailTo { name, lines },
            expires_at,
        })
    }

    /// `EUR 9.90`.
    #[must_use]
    pub fn amount_text(&self) -> String {
        format!("{} {}", self.currency, amount_text(self.amount_cents))
    }

    /// `nine euros and ninety cents`.
    #[must_use]
    pub fn amount_words(&self) -> String {
        amount_in_words(self.amount_cents, &self.currency)
    }
}

/// A text of at most 64 printable ASCII characters (for an error).
fn shown(text: &str) -> String {
    text.chars().filter(char::is_ascii_graphic).take(64).collect()
}

const ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// `n` in English words: `forty-nine`, `one thousand two hundred thirty-four`.
#[must_use]
pub fn number_in_words(n: u64) -> String {
    if n < 20 {
        return ONES[n as usize].to_string();
    }
    if n < 100 {
        let tens = TENS[(n / 10) as usize];
        return match n % 10 {
            0 => tens.to_string(),
            ones => format!("{tens}-{}", ONES[ones as usize]),
        };
    }
    let (scale, name) = match n {
        _ if n < 1_000 => (100, "hundred"),
        _ if n < 1_000_000 => (1_000, "thousand"),
        _ if n < 1_000_000_000 => (1_000_000, "million"),
        _ => (1_000_000_000, "billion"),
    };
    let mut out = format!("{} {name}", number_in_words(n / scale));
    if n % scale > 0 {
        let _ = write!(out, " {}", number_in_words(n % scale));
    }
    out
}

/// The names of a currency's unit and its hundredth, one and many: `euro`, `euros`, `cent`,
/// `cents`; a currency without names of its own is its code and "hundredth".
fn unit_names(currency: &str) -> [&str; 4] {
    match currency {
        "EUR" => ["euro", "euros", "cent", "cents"],
        "USD" => ["dollar", "dollars", "cent", "cents"],
        "GBP" => ["pound", "pounds", "penny", "pence"],
        "CHF" => ["Swiss franc", "Swiss francs", "centime", "centimes"],
        other => [other, other, "hundredth", "hundredths"],
    }
}

/// `cents` of `currency` in words, as a slip writes the amount out: `nine euros and ninety
/// cents`, `one euro`, `two cents`.
#[must_use]
pub fn amount_in_words(cents: u64, currency: &str) -> String {
    let [one, many, minor_one, minor_many] = unit_names(currency.trim());
    let (major, minor) = (cents / 100, cents % 100);
    let units = |n: u64, one: &str, many: &str| {
        format!("{} {}", number_in_words(n), if n == 1 { one } else { many })
    };
    match (major, minor) {
        (0, 0) => format!("zero {many}"),
        (0, minor) => units(minor, minor_one, minor_many),
        (major, 0) => units(major, one, many),
        (major, minor) => format!(
            "{} and {}",
            units(major, one, many),
            units(minor, minor_one, minor_many)
        ),
    }
}
