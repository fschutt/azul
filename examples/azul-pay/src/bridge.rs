//! The navigation bridge between a hosted-fields page and the app (CHECKOUT-PLAN §3.5).
//!
//! The web view has no script bridge and the app injects no script (an injected script switches
//! Apple Pay on the web off for the page, and a script bridge would be one more door into the
//! app). So the two talk through main-frame navigations only:
//!
//! | Direction  | How | Messages |
//! |------------|-----|----------|
//! | page -> app | the page navigates its main frame to `<pages>/_bridge/<message>?...`; the app reads the URL in `WebViewNavigationRequested` and cancels it, so the page stays | `ready`; `height?v=<px>`; `brand?v=visa\|mastercard\|amex\|discover\|diners\|jcb\|unionpay\|unknown`; `complete?v=0\|1`; `last4?v=<4 digits>`; `error?code=<code>&message=<text>`; `result?v=succeeded\|processing\|requires_action\|failed&code=<code>` |
//! | app -> page | the app navigates the web view to the same page with a new fragment; a same-document fragment navigation fires `hashchange` and nothing reloads | `cmd=confirm&name=<cardholder>`, `cmd=reset`, `cmd=look&look=<look>`, each with `&n=<sequence>` after the page's own inputs |
//!
//! [`parse_bridge`] is strict: an unknown message, an unknown, repeated, undecodable or
//! out-of-range argument, or a query over [`MAX_QUERY`] bytes is refused (and the navigation
//! cancelled all the same).

use std::fmt;

use crate::{
    registry::BRIDGE_PREFIX,
    surface::SecretUrl,
    url::{form_decode, form_encode, WebUrl},
};

/// The longest query a bridge message may have.
pub const MAX_QUERY: usize = 512;
/// The longest provider message the popover shows.
pub const MAX_MESSAGE: usize = 200;
/// The longest cardholder name handed to the page.
pub const MAX_NAME: usize = 100;
/// The tallest the page may ask its slot to be (3-D Secure needs room).
pub const MAX_HEIGHT: u32 = 4000;

/// A card's brand, as the fields page reports it (the card artwork's logo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardBrand {
    Visa,
    Mastercard,
    Amex,
    Discover,
    Diners,
    Jcb,
    UnionPay,
    Unknown,
}

impl CardBrand {
    pub const ALL: [CardBrand; 8] = [
        CardBrand::Visa,
        CardBrand::Mastercard,
        CardBrand::Amex,
        CardBrand::Discover,
        CardBrand::Diners,
        CardBrand::Jcb,
        CardBrand::UnionPay,
        CardBrand::Unknown,
    ];

    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CardBrand::Visa => "visa",
            CardBrand::Mastercard => "mastercard",
            CardBrand::Amex => "amex",
            CardBrand::Discover => "discover",
            CardBrand::Diners => "diners",
            CardBrand::Jcb => "jcb",
            CardBrand::UnionPay => "unionpay",
            CardBrand::Unknown => "unknown",
        }
    }

    /// A wire name, exactly.
    #[must_use]
    pub fn parse(text: &str) -> Option<CardBrand> {
        CardBrand::ALL.into_iter().find(|b| b.as_str() == text)
    }

    /// The artwork's logo text.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            CardBrand::Visa => "VISA",
            CardBrand::Mastercard => "Mastercard",
            CardBrand::Amex => "American Express",
            CardBrand::Discover => "Discover",
            CardBrand::Diners => "Diners Club",
            CardBrand::Jcb => "JCB",
            CardBrand::UnionPay => "UnionPay",
            CardBrand::Unknown => "Card",
        }
    }
}

/// What a confirmation came to, as the fields page reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    /// Accepted, settling (a debit): the token server's poll says when.
    Processing,
    /// The provider shows a step (3-D Secure) inside its fields.
    RequiresAction,
    Failed,
}

/// One message of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeMessage {
    /// The fields are mounted.
    Ready,
    /// The height the fields need, in px.
    Height(u32),
    Brand(CardBrand),
    /// The fields are complete (the order button may be used).
    Complete(bool),
    /// The last four digits, the provider's masked echo.
    Last4(String),
    /// A provider error: its code and its message (shown in the popover).
    Error { code: String, message: String },
    /// The confirmation's outcome, with the provider's code.
    Result { outcome: Outcome, code: String },
}

/// Why a navigation is no bridge message (it is cancelled all the same).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// Not under `/_bridge/`.
    NotBridge,
    /// A message this app does not know.
    Unknown(String),
    /// The query is longer than [`MAX_QUERY`].
    Oversized,
    /// An argument given twice.
    Repeated(String),
    /// An argument that is not one of the message's, or a value out of its range.
    Junk(String),
    /// An argument the message needs.
    Missing(&'static str),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeError::NotBridge => f.write_str("not a bridge message"),
            BridgeError::Unknown(name) => write!(f, "an unknown bridge message \"{name}\""),
            BridgeError::Oversized => f.write_str("a bridge message too long"),
            BridgeError::Repeated(key) => write!(f, "the bridge argument {key} twice"),
            BridgeError::Junk(key) => write!(f, "the bridge argument {key} is not usable"),
            BridgeError::Missing(key) => write!(f, "the bridge argument {key} is missing"),
        }
    }
}

impl std::error::Error for BridgeError {}

fn said(text: &str) -> String {
    text.chars().filter(char::is_ascii_graphic).take(64).collect()
}

/// The decoded arguments of `query`, in order; a repeated or undecodable one refused.
fn arguments(query: &str) -> Result<Vec<(String, String)>, BridgeError> {
    let mut out: Vec<(String, String)> = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = form_decode(key).ok_or_else(|| BridgeError::Junk(said(key)))?;
        let value = form_decode(value).ok_or_else(|| BridgeError::Junk(said(&key)))?;
        if out.iter().any(|(k, _)| *k == key) {
            return Err(BridgeError::Repeated(said(&key)));
        }
        out.push((key, value));
    }
    Ok(out)
}

/// A provider code: 1 to 64 of `a-z`, `0-9`, `_`.
fn code(text: &str, key: &str) -> Result<String, BridgeError> {
    let ok = !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(text.to_string())
    } else {
        Err(BridgeError::Junk(key.to_string()))
    }
}

/// The message of `url` (a navigation on the provider's pages origin).
///
/// # Errors
///
/// [`BridgeError`]: why it is no message (see the module docs).
pub fn parse_bridge(url: &WebUrl) -> Result<BridgeMessage, BridgeError> {
    let name = url
        .path()
        .strip_prefix(BRIDGE_PREFIX)
        .ok_or(BridgeError::NotBridge)?;
    if url.query().len() > MAX_QUERY {
        return Err(BridgeError::Oversized);
    }
    let args = arguments(url.query())?;
    let allowed: &[&str] = match name {
        "ready" => &[],
        "height" | "brand" | "complete" | "last4" => &["v"],
        "error" => &["code", "message"],
        "result" => &["v", "code"],
        other => return Err(BridgeError::Unknown(said(other))),
    };
    if let Some((key, _)) = args.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
        return Err(BridgeError::Junk(said(key)));
    }
    let junk = || BridgeError::Junk(String::from("v"));
    match name {
        "ready" => Ok(BridgeMessage::Ready),
        "height" => {
            let text = value(&args, "v")?;
            if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(junk());
            }
            match text.parse::<u32>() {
                Ok(px) if (1..=MAX_HEIGHT).contains(&px) => Ok(BridgeMessage::Height(px)),
                _ => Err(junk()),
            }
        }
        "brand" => CardBrand::parse(value(&args, "v")?)
            .map(BridgeMessage::Brand)
            .ok_or_else(junk),
        "complete" => match value(&args, "v")? {
            "1" => Ok(BridgeMessage::Complete(true)),
            "0" => Ok(BridgeMessage::Complete(false)),
            _ => Err(junk()),
        },
        "last4" => {
            let digits = value(&args, "v")?;
            if digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) {
                Ok(BridgeMessage::Last4(digits.to_string()))
            } else {
                Err(junk())
            }
        }
        "error" => {
            let code = code(value(&args, "code")?, "code")?;
            let message = argument(&args, "message").unwrap_or_default();
            if message.chars().count() > MAX_MESSAGE || message.chars().any(char::is_control) {
                return Err(BridgeError::Junk(String::from("message")));
            }
            Ok(BridgeMessage::Error {
                code,
                message: message.trim().to_string(),
            })
        }
        "result" => {
            let outcome = match value(&args, "v")? {
                "succeeded" => Outcome::Succeeded,
                "processing" => Outcome::Processing,
                "requires_action" => Outcome::RequiresAction,
                "failed" => Outcome::Failed,
                _ => return Err(junk()),
            };
            let code = match argument(&args, "code") {
                None => String::new(),
                Some(text) => code(text, "code")?,
            };
            Ok(BridgeMessage::Result { outcome, code })
        }
        other => Err(BridgeError::Unknown(said(other))),
    }
}

/// The argument `key` of `args`, if given.
fn argument<'a>(args: &'a [(String, String)], key: &str) -> Option<&'a str> {
    args.iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The argument `key` the message needs.
fn value<'a>(args: &'a [(String, String)], key: &'static str) -> Result<&'a str, BridgeError> {
    argument(args, key).ok_or(BridgeError::Missing(key))
}

/// What the app tells the fields page.
#[derive(Clone, PartialEq, Eq)]
pub enum PageCommand {
    /// Confirm the payment, with the cardholder name typed natively (the billing name).
    Confirm { name: String },
    /// Clear the fields.
    Reset,
    /// Switch to the look `look` (`flora-dark`).
    Look(String),
}

impl fmt::Debug for PageCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageCommand::Confirm { .. } => f.write_str("Confirm { name: <hidden> }"),
            PageCommand::Reset => f.write_str("Reset"),
            PageCommand::Look(look) => write!(f, "Look({look:?})"),
        }
    }
}

/// A cardholder name as the page gets it: no control characters, white space runs as one
/// space, at most [`MAX_NAME`] characters.
#[must_use]
pub fn clean_name(name: &str) -> String {
    let visible: String = name.chars().filter(|c| !c.is_control()).collect();
    visible
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_NAME)
        .collect()
}

/// The fields page `page` (with its inputs in the fragment) told `command`, the `seq`th command:
/// the same document with the command after its inputs - a fragment navigation, so nothing
/// reloads, and a new `seq` makes each command a new fragment (each fires `hashchange`).
#[must_use]
pub fn command_url(page: &SecretUrl, command: &PageCommand, seq: u32) -> SecretUrl {
    let cmd = match command {
        PageCommand::Confirm { name } => {
            format!("cmd=confirm&name={}", form_encode(&clean_name(name)))
        }
        PageCommand::Reset => String::from("cmd=reset"),
        PageCommand::Look(look) => format!("cmd=look&look={}", form_encode(look)),
    };
    let inputs = page.url().fragment();
    let fragment = if inputs.is_empty() {
        format!("{cmd}&n={seq}")
    } else {
        format!("{inputs}&{cmd}&n={seq}")
    };
    SecretUrl::new(page.url().with_fragment(&fragment))
}
