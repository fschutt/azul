//! The money of an order, typed (ARCH28 RF52): what the buyer was SHOWN ([`Quote`]), what the
//! token server CHARGES ([`Charge`]) and the payment page of a v1 checkout ([`pay_page`]) -
//! built, never taken from an answer.
//!
//! A [`Charge`] is made from the checkout's answer and from nothing else, and an answer without
//! its amount or its currency is no answer: it is a protocol error here, not a free order and not
//! one in euros. A [`Quote`] is compared with it before anything of the checkout is shown or
//! opened ([`Quote::check`]): a buyer who was shown EUR 0.99 is never sent to a page that asks
//! PLN 4.49.

use std::fmt;

use azlin_contract::{Cents, Currency};
use serde_json::Value;

use super::{check_id, TokenError};

/// What the token server charges for a checkout: the amount in the smallest unit of its
/// currency, read from the checkout's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge(Cents);

impl Charge {
    /// The charge a checkout's answer names (`amount_cents`, `currency`).
    ///
    /// # Errors
    ///
    /// [`TokenError::Protocol`] when the answer lacks the amount or the currency, or the
    /// currency is no ISO 4217 code: a missing field is never a free order, nor one in euros.
    pub(crate) fn of_answer(answer: &Value) -> Result<Charge, TokenError> {
        let amount = answer["amount_cents"].as_u64().ok_or_else(|| {
            TokenError::Protocol(String::from("the checkout names no amount to charge"))
        })?;
        let currency = answer["currency"]
            .as_str()
            .ok_or_else(|| {
                TokenError::Protocol(String::from("the checkout names no currency to charge in"))
            })
            .and_then(|code| {
                Currency::parse(code).map_err(|_| {
                    TokenError::Protocol(String::from("the checkout's currency is no currency"))
                })
            })?;
        Ok(Charge(Cents { amount, currency }))
    }

    /// The amount and its currency.
    #[must_use]
    pub fn cents(&self) -> &Cents {
        &self.0
    }
}

/// What the buyer was shown for an order: the price, and for which tier and how many months.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub price: Cents,
    pub tier: String,
    pub months: u32,
    /// The price is a ceiling, not the price: a renewal under a price lock pays that version's
    /// price "or today's when lower", so a lower charge in the same currency is what was
    /// promised. A new order's quote is exact.
    pub at_most: bool,
}

/// The token server charges something other than the buyer was shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceChanged {
    pub shown: Cents,
    pub charged: Cents,
}

impl fmt::Display for PriceChanged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "shown {} {}, charged {} {}",
            self.shown.currency, self.shown.amount, self.charged.currency, self.charged.amount
        )
    }
}

impl std::error::Error for PriceChanged {}

impl Quote {
    /// Whether `charged` is what the buyer was shown: the same currency and the same amount (at
    /// most the amount for a [`Quote::at_most`]). Called BEFORE anything of the checkout is
    /// shown or opened.
    ///
    /// # Errors
    ///
    /// [`PriceChanged`]: what was shown and what is charged.
    pub fn check(&self, charged: &Charge) -> Result<(), PriceChanged> {
        let charged = charged.cents();
        let same_currency = charged.currency == self.price.currency;
        let fits = if self.at_most {
            charged.amount <= self.price.amount
        } else {
            charged.amount == self.price.amount
        };
        if same_currency && fits {
            Ok(())
        } else {
            Err(PriceChanged {
                shown: self.price.clone(),
                charged: charged.clone(),
            })
        }
    }
}

/// The payment page of the v1 checkout `checkout_id` at the token server `token_url`
/// (`{token_url}/v1/pay/{id}`): the address the token server builds it at, built here the same
/// way. An address in an answer is compared with it ([`page_is`]) and never opened for itself.
///
/// # Errors
///
/// [`TokenError::Config`] for an id that is no checkout id.
pub fn pay_page(token_url: &str, checkout_id: &str) -> Result<String, TokenError> {
    let id = check_id(checkout_id.trim())?;
    Ok(format!(
        "{}/v1/pay/{id}",
        token_url.trim().trim_end_matches('/')
    ))
}

/// Whether `answered` is the payment page of `checkout_id` at `token_url`: the same address,
/// the scheme and host in any case, nothing before or after it. A page on another host, another
/// scheme or another path is not.
#[must_use]
pub fn page_is(token_url: &str, checkout_id: &str, answered: &str) -> bool {
    let Ok(built) = pay_page(token_url, checkout_id) else {
        return false;
    };
    let answered = answered.trim();
    // Scheme and host are case-insensitive; the path (the checkout id) is exact.
    let split = |url: &str| -> Option<(String, String)> {
        let (scheme, rest) = url.split_once("://")?;
        let (host, path) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
        Some((
            format!(
                "{}://{}",
                scheme.to_ascii_lowercase(),
                host.to_ascii_lowercase()
            ),
            path.to_string(),
        ))
    };
    match (split(&built), split(answered)) {
        (Some(ours), Some(theirs)) => ours == theirs,
        _ => false,
    }
}
