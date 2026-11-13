//! The payer's side of a hosted pay-in page, for the shapes whose page the payer reaches with an
//! address only (dLocal's checkout, EBANX's Payment Page): the order, the local method's test page
//! (a QR box or a voucher the fake phone pays), the fake phone and the status the page polls.
//!
//! The payer's identity the API took is never here: a payment keeps its amount, its method, our
//! reference and where the payer goes back to, and the pages show nothing else.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use azul_pay_methods::currency::{decimal_text, parse_decimal};
use azul_pay_methods::{Registry, TestInput};

use crate::engine::Response;
use crate::kit;
use crate::local::{self, Answer, Local};

/// The most characters of a description a page shows.
const MAX_DESCRIPTION: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Open,
    Paid,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone)]
pub(crate) struct Refund {
    pub id: String,
    pub amount: i64,
    pub done: bool,
}

/// One payment of a hosted page.
#[derive(Debug, Clone)]
pub(crate) struct Pay {
    pub id: String,
    /// The provider's own type of the method (`PQ`, `pix`).
    pub provider_type: String,
    pub amount: i64,
    pub currency: String,
    pub country: String,
    /// The backend's reference for it.
    pub order: String,
    /// Where the payer goes back to.
    pub back: String,
    pub description: String,
    pub state: State,
    pub created: i64,
    pub refunds: Vec<Refund>,
}

impl Pay {
    /// What of it is refunded or being refunded.
    pub fn refunded(&self) -> i64 {
        self.refunds.iter().map(|r| r.amount).sum()
    }
}

/// A shape's hosted pages: the registry provider whose routes it fakes, its path prefix, the
/// words its pages say it is.
pub(crate) struct Shape {
    pub provider: &'static str,
    pub prefix: &'static str,
    pub name: &'static str,
}

impl Shape {
    /// The faked route of `provider_type`: a type without documented test data has none.
    pub fn local<'r>(&self, registry: &'r Registry, provider_type: &str) -> Option<Local<'r>> {
        local::find(registry, self.provider, provider_type)
    }

    pub fn closed(&self, status: u16) -> Response {
        let body = "<h2>This test payment is closed</h2><p>It was paid, declined or cancelled. Go back to the app.</p>";
        Response::html(status, kit::single("Payment closed", self.name, body, ""))
    }

    /// The checkout page of `p`, served at `base`.
    pub fn page(&self, registry: &Registry, base: &str, p: &Pay) -> Response {
        let Some(l) = self.local(registry, &p.provider_type) else {
            return self.closed(404);
        };
        if p.state != State::Open {
            return self.closed(409);
        }
        let amount = local::money(p.amount, &p.currency);
        let left = kit::order(&p.description, &amount, "One payment");
        let phone = format!("{base}{}/phone/{}", self.prefix, p.id);
        let status = format!("{base}{}/status/{}", self.prefix, p.id);
        let right = l.form(&format!("Pay {amount}"), &phone, &status, &p.back, "");
        Response::html(200, kit::split("Checkout", self.name, &left, &right, local::POLL_SCRIPT))
    }

    /// The fake phone of `p`: the method's app, standing in for the payer paying it.
    pub fn phone(&self, registry: &Registry, p: &Pay) -> Response {
        let Some(l) = self.local(registry, &p.provider_type) else {
            return self.closed(404);
        };
        let actions: Vec<(&str, &str)> = if p.state == State::Open {
            vec![("authorize", "Pay (test)"), ("fail", "Decline (test)")]
        } else {
            Vec::new()
        };
        let note = if actions.is_empty() {
            "Nothing to do here: this test payment is decided."
        } else {
            "Only this sandbox listens to these buttons."
        };
        Response::html(200, l.phone(self.name, &local::money(p.amount, &p.currency), &actions, note))
    }

    /// The documented test input an answer on the phone names; none for anything else.
    pub fn decide(&self, registry: &Registry, p: &Pay, form: &BTreeMap<String, String>) -> Option<TestInput> {
        match self.local(registry, &p.provider_type)?.answer(form) {
            Answer::Matched(t) => Some(t),
            Answer::Refused(_) => None,
        }
    }
}

/// What the page polls: complete once the payment is decided.
pub(crate) fn status(p: &Pay) -> Response {
    let s = if p.state == State::Open { "open" } else { "complete" };
    Response::json(200, json!({ "status": s }))
}

/// A description as a page shows it: printable, short.
pub(crate) fn description(raw: Option<&str>) -> String {
    raw.unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DESCRIPTION)
        .collect()
}

/// An amount the API was sent as a JSON number or a decimal string, in `currency`'s minor units.
pub(crate) fn minor_of(v: Option<&Value>, currency: &str) -> Option<i64> {
    let text = match v? {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().to_string(),
        _ => return None,
    };
    parse_decimal(&text, currency).filter(|m| *m > 0)
}

/// `minor` as the JSON number a provider writes (`342.9`).
pub(crate) fn amount_number(minor: i64, currency: &str) -> Value {
    serde_json::from_str(&decimal_text(minor, currency)).unwrap_or(Value::Null)
}

/// `minor` as the decimal string EBANX writes (`"342.90"`).
pub(crate) fn amount_text(minor: i64, currency: &str) -> Value {
    Value::String(decimal_text(minor, currency))
}

/// The payer a provider's answer names, as a stand-in writes it: a fixed test placeholder, never
/// what the API was sent.
pub(crate) fn placeholder_payer() -> Value {
    json!({"name": "[TEST] payer", "email": "payer@sandbox.invalid", "document": "00000000000"})
}
