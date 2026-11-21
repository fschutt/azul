//! The Stripe-shaped test provider: the API an app's backend calls (Checkout Sessions with
//! `ui_mode` `hosted_page` or `elements`, PaymentIntents, refunds, subscriptions, customers and
//! test clocks), the payer's pages (the split-screen checkout, the fields page an app's payment
//! popover loads, a 3-D Secure step), and webhooks signed with Stripe's `Stripe-Signature`
//! scheme. The shapes follow Stripe's API reference (version 2026-09-30.endive); the look is our
//! own test look.
//!
//! Only Stripe's documented test cards (https://docs.stripe.com/testing, read 2026-10-17) and the
//! test IBANs of [`crate::testdata`] are taken; any other number, even a valid one, is refused
//! before anything happens. No number, expiry, CVC, IBAN or name is kept: a session keeps its
//! outcome only.
//!
//! The local methods (iDEAL, BLIK, TWINT, Pix, Multibanco, MB WAY and the rest) are the payment
//! methods registry's Stripe routes that carry Stripe's documented test data; their pages and
//! outcomes come from [`crate::local`], and a type without documented test data is refused when
//! the session is made.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use azul_pay_methods::{Flow, Outcome, Registry};

use crate::engine::{bound, due_steps, take, Cx, FakeProvider, Request, Response};
use crate::local::{self, Answer};
use crate::testdata::{debit_of, iban_problem, Debit, TEST_IBANS_ONLY};
use crate::{kit, util, Config, Error, Kind};

const API_VERSION: &str = "2026-09-30.endive";
const SHAPE: &str = "Stripe-shaped test provider";
const FIELDS_PAGE: &str = "/fields/fake-stripe/v1";
/// The methods this fake has its own pages for: the card fields (shared with the popover's fields
/// page), the IBAN form with its mandate text and the wallet approval. Every other method is a
/// local one, paid with the registry's documented test data on its generic page.
const OWN_PAGES: [&str; 3] = ["card", "sepa_debit", "paypal"];

/// The local method `method` stands for, if it is one: never a method with its own page, whose
/// registry entry also carries test data but whose page asks for other fields.
fn local_of<'r>(registry: &'r Registry, method: &str) -> Option<local::Local<'r>> {
    if OWN_PAGES.contains(&method) {
        return None;
    }
    local::find(registry, "stripe", method)
}
const TEST_CARDS_ONLY: &str = "This sandbox only accepts test card numbers, for example 4242 4242 4242 4242.";
/// The payer the events name, as Stripe's do: a placeholder the backend must not keep.
const PAYER_EMAIL: &str = "payer@example.com";
const PAYER_NAME: &str = "Test Payer";
const DAY: i64 = 86_400;
/// The most periods one test-clock advance charges.
const MAX_PERIODS: usize = 36;

/// What a documented test card does.
#[derive(Debug, Clone, Copy)]
enum Card {
    /// Paid; the card's country.
    Ok(&'static str),
    /// Declined: the error code, the decline code (empty: none), Stripe's message.
    Decline(&'static str, &'static str, &'static str),
    /// 3-D Secure first; then paid, or declined after the challenge.
    ThreeDs(bool),
    /// Paid, then disputed as fraudulent.
    Dispute,
}

const DECLINED: &str = "Your card was declined.";

/// Stripe's test cards (number, brand, behaviour).
const CARDS: &[(&str, &str, Card)] = &[
    ("4242424242424242", "visa", Card::Ok("US")),
    ("5555555555554444", "mastercard", Card::Ok("US")),
    ("378282246310005", "amex", Card::Ok("US")),
    ("6011111111111117", "discover", Card::Ok("US")),
    ("4000002760000016", "visa", Card::Ok("DE")),
    ("4000000400000008", "visa", Card::Ok("AT")),
    ("4000000560000004", "visa", Card::Ok("BE")),
    ("4000002500000003", "visa", Card::Ok("FR")),
    ("4000000000000002", "visa", Card::Decline("card_declined", "generic_decline", DECLINED)),
    ("4000000000009995", "visa", Card::Decline("card_declined", "insufficient_funds", "Your card has insufficient funds.")),
    ("4000000000009987", "visa", Card::Decline("card_declined", "lost_card", DECLINED)),
    ("4000000000009979", "visa", Card::Decline("card_declined", "stolen_card", DECLINED)),
    ("4000000000000069", "visa", Card::Decline("expired_card", "", "Your card has expired.")),
    ("4000000000000127", "visa", Card::Decline("incorrect_cvc", "", "Your card's security code is incorrect.")),
    ("4000000000000119", "visa", Card::Decline("processing_error", "", "An error occurred while processing your card. Try again in a little bit.")),
    ("4242424242424241", "visa", Card::Decline("incorrect_number", "", "Your card number is incorrect.")),
    ("4000000000006975", "visa", Card::Decline("card_declined", "card_velocity_exceeded", "Your card was declined for making repeated attempts too frequently or exceeding its amount limit.")),
    ("4000002500003155", "visa", Card::ThreeDs(false)),
    ("4000002760003184", "visa", Card::ThreeDs(false)),
    ("4000000000003220", "visa", Card::ThreeDs(false)),
    ("4000008400000027", "visa", Card::ThreeDs(false)),
    ("4000008400001629", "visa", Card::ThreeDs(true)),
    ("4000000000000259", "visa", Card::Dispute),
];

fn card_of(number: &str) -> Option<(&'static str, Card)> {
    CARDS.iter().find(|(n, _, _)| *n == number).map(|(_, b, c)| (*b, *c))
}

#[derive(Debug, Clone)]
struct Session {
    id: String,
    mode: String,
    ui_mode: String,
    status: String,
    payment_status: String,
    amount: i64,
    currency: String,
    method: String,
    product: String,
    client_reference_id: Option<String>,
    metadata_ref: Option<String>,
    pi_ref: Option<String>,
    sub_ref: Option<String>,
    trial_days: i64,
    success_url: Option<String>,
    cancel_url: Option<String>,
    return_url: Option<String>,
    client_secret: Option<String>,
    expires_at: i64,
    created: i64,
    payment_intent: Option<String>,
    subscription: Option<String>,
    customer: Option<String>,
    /// What the payer sees when Stripe converts the price (Adaptive Pricing): the amount in the
    /// minor unit and the currency, lower case.
    presented: Option<(i64, String)>,
}

/// A PaymentIntent: its outcome, never its card.
#[derive(Debug, Clone)]
struct Intent {
    id: String,
    session: String,
    amount: i64,
    currency: String,
    status: String,
    method: String,
    metadata_ref: Option<String>,
    received: i64,
    refunded: i64,
    charge: String,
    country: Option<String>,
    brand: Option<String>,
    error: Option<(String, String, String)>,
    /// A 3-D Secure challenge waiting: whether the card is declined after it.
    challenge: Option<bool>,
    created: i64,
}

#[derive(Debug, Clone)]
struct Subscription {
    id: String,
    status: String,
    amount: i64,
    currency: String,
    metadata_ref: Option<String>,
    customer: Option<String>,
    periods: i64,
    /// When its next period is charged (on its customer's test clock, if it has one).
    next_at: i64,
    created: i64,
}

#[derive(Debug, Clone)]
enum Step {
    Settle { pi: String, decline: Option<&'static str> },
    Dispute { pi: String },
    /// The app approved a payment it was waiting for (MB WAY's test number).
    Approve { pi: String },
    /// A voucher or a code that was never paid: it fails with `code`.
    Expire { pi: String, code: String },
}

/// What a payment attempt came to.
enum Pay {
    Paid { last4: String },
    Processing,
    Challenge(String),
    Declined { code: String, message: String },
    /// Not a test card or test IBAN, or not this session's method: nothing happened.
    Refused { code: &'static str, message: String },
    Closed,
}

#[derive(Default)]
pub(crate) struct FakeStripe {
    /// Which local methods exist and their documented test data.
    registry: Registry,
    sessions: BTreeMap<String, Session>,
    /// client secret -> session id (the fields page names its session by it).
    secrets: BTreeMap<String, String>,
    intents: BTreeMap<String, Intent>,
    subscriptions: BTreeMap<String, Subscription>,
    refunds: BTreeMap<String, Value>,
    disputes: BTreeMap<String, Value>,
    /// Every invoice a subscription made, as it was sent (a renewal's month is refunded through
    /// its invoice's PaymentIntent, read back here).
    invoices: BTreeMap<String, Value>,
    /// customer -> its test clock.
    customers: BTreeMap<String, Option<String>>,
    /// test clock -> its frozen time.
    clocks: BTreeMap<String, i64>,
    /// Idempotency key -> (fingerprint of the request, status, answer).
    idem: BTreeMap<String, (String, u16, Value)>,
    steps: Vec<(i64, Step)>,
}

/// A balance transaction of `amount` euro cents (negative: taken back), named after the object it
/// books; the euros come from the sandbox's marked test rates.
fn balance_tx(of: &str, amount: i64) -> Value {
    json!({"id": format!("txn_{of}"), "object": "balance_transaction", "amount": amount, "currency": "eur",
           "fee": 0, "net": amount, "description": "[TEST] at the sandbox's test rate"})
}

fn error(status: u16, kind: &str, code: Option<&str>, message: &str) -> (u16, Value) {
    let mut e = json!({"type": kind, "message": message});
    if let Some(c) = code {
        e["code"] = json!(c);
    }
    (status, json!({ "error": e }))
}


fn event(kind: &str, object: Value, now: i64) -> Value {
    json!({
        "id": util::id("evt_"),
        "object": "event",
        "api_version": API_VERSION,
        "created": now,
        "livemode": false,
        "type": kind,
        "data": {"object": object},
        "pending_webhooks": 1,
        "request": {"id": null, "idempotency_key": null},
    })
}

/// `MM/YY`, `MM / YY` or `MMYY` as (month, year); none when incomplete.
fn expiry(text: &str) -> Option<(i64, i64)> {
    let d = util::digits(text);
    if d.len() != 4 {
        return None;
    }
    let m = d.get(..2)?.parse::<i64>().ok()?;
    let y = 2000 + d.get(2..)?.parse::<i64>().ok()?;
    Some((m, y))
}

impl FakeStripe {
    pub(crate) fn new() -> Result<FakeStripe, Error> {
        let registry = Registry::builtin().map_err(|e| Error::Registry(e.to_string()))?;
        Ok(FakeStripe {
            registry,
            ..FakeStripe::default()
        })
    }

    fn session_json(&self, s: &Session, base: &str) -> Value {
        let hosted = s.ui_mode == "hosted_page";
        let details = if s.status == "complete" {
            json!({"email": PAYER_EMAIL, "name": PAYER_NAME, "address": {"country": "DE"}})
        } else {
            Value::Null
        };
        json!({
            "id": s.id, "object": "checkout.session", "mode": s.mode, "ui_mode": s.ui_mode,
            "status": s.status, "payment_status": s.payment_status,
            "amount_total": s.amount, "amount_subtotal": s.amount, "currency": s.currency,
            "client_reference_id": s.client_reference_id, "metadata": util::ref_meta(&s.metadata_ref),
            "payment_intent": s.payment_intent, "subscription": s.subscription, "customer": s.customer,
            "payment_method_types": [s.method],
            "url": if hosted && s.status == "open" { json!(format!("{base}/stripe/c/pay/{}", s.id)) } else { Value::Null },
            "client_secret": if hosted { Value::Null } else { json!(s.client_secret) },
            "success_url": s.success_url, "cancel_url": s.cancel_url, "return_url": s.return_url,
            "expires_at": s.expires_at, "created": s.created, "livemode": false,
            "customer_details": details,
            "presentment_details": s.presented.as_ref().map(|(amount, currency)| json!({
                "presentment_amount": amount, "presentment_currency": currency})),
        })
    }

    fn intent_json(i: &Intent) -> Value {
        let error = i.error.as_ref().map(|(code, decline, message)| {
            let mut e = json!({"type": "card_error", "code": code, "message": message,
                               "payment_method": {"billing_details": {"email": PAYER_EMAIL, "name": PAYER_NAME}}});
            if !decline.is_empty() {
                e["decline_code"] = json!(decline);
            }
            e
        });
        json!({
            "id": i.id, "object": "payment_intent", "amount": i.amount, "amount_received": i.received,
            "currency": i.currency, "status": i.status, "metadata": util::ref_meta(&i.metadata_ref),
            "payment_method_types": [i.method], "latest_charge": i.charge, "created": i.created,
            "last_payment_error": error, "livemode": false,
        })
    }

    /// A PaymentIntent as `GET` answers it: its charge expanded with its balance transaction
    /// when `expand[]=latest_charge.balance_transaction` asks (the euros at the test rate).
    fn intent_read(i: &Intent, expand: bool) -> Value {
        let mut j = Self::intent_json(i);
        if expand && i.status == "succeeded" && !i.charge.is_empty() {
            let euros = local::convert(i.received, &i.currency, "EUR").unwrap_or(0);
            j["latest_charge"] = json!({"id": i.charge, "object": "charge",
                                        "balance_transaction": balance_tx(&i.charge, euros)});
        }
        j
    }

    fn charge_json(i: &Intent) -> Value {
        // what Stripe tells of the instrument: its kind, brand and country, never its number
        let mut details = json!({ "type": i.method });
        details[i.method.as_str()] = if i.method == "card" {
            json!({"brand": i.brand, "country": i.country})
        } else {
            json!({ "country": i.country })
        };
        json!({
            "id": i.charge, "object": "charge", "amount": i.amount, "amount_refunded": i.refunded,
            "currency": i.currency, "payment_intent": i.id, "paid": true, "refunded": i.refunded >= i.amount,
            "status": "succeeded", "billing_details": {"email": PAYER_EMAIL, "name": PAYER_NAME},
            "payment_method_details": details, "created": i.created,
        })
    }

    fn subscription_json(s: &Subscription) -> Value {
        json!({"id": s.id, "object": "subscription", "status": s.status, "metadata": util::ref_meta(&s.metadata_ref),
               "customer": s.customer, "created": s.created,
               "items": {"object": "list", "data": [{"price": {"unit_amount": s.amount, "currency": s.currency,
                                                               "recurring": {"interval": "month"}}}]}})
    }

    /// Keeps `invoice` for its read back and gives it back.
    fn keep_invoice(&mut self, invoice: Value) -> Value {
        if let Some(id) = invoice["id"].as_str() {
            self.invoices.insert(id.to_string(), invoice.clone());
            bound(&mut self.invoices);
        }
        invoice
    }

    fn invoice_json(s: &Subscription, amount_paid: i64, reason: &str, pi: Option<&str>, now: i64) -> Value {
        let payments: Vec<Value> = pi
            .map(|p| vec![json!({"payment": {"type": "payment_intent", "payment_intent": p}})])
            .unwrap_or_default();
        json!({
            "id": util::id("in_"), "object": "invoice", "amount_paid": amount_paid, "amount_due": s.amount,
            "currency": s.currency, "billing_reason": reason, "status": if amount_paid > 0 || reason == "subscription_create" { "paid" } else { "open" },
            "customer": s.customer, "customer_email": PAYER_EMAIL, "customer_name": PAYER_NAME,
            "period_start": now, "period_end": now + 30 * DAY, "created": now,
            "parent": {"type": "subscription_details",
                       "subscription_details": {"subscription": s.id, "metadata": util::ref_meta(&s.metadata_ref)}},
            "payments": {"object": "list", "data": payments},
        })
    }

    /// The session `id` while it may be paid; an open one past its time expires first.
    fn open_session(&mut self, cx: &mut Cx<'_>, id: &str) -> Option<Session> {
        let s = self.sessions.get_mut(id)?;
        if s.status == "open" && cx.now >= s.expires_at {
            s.status = "expired".into();
            let s = s.clone();
            let j = self.session_json(&s, cx.base);
            cx.emit(event("checkout.session.expired", j, cx.now));
            return None;
        }
        (s.status == "open").then(|| s.clone())
    }

    /// The session's PaymentIntent, made on its first attempt.
    fn intent_for(&mut self, cx: &Cx<'_>, s: &Session) -> String {
        if let Some(pi) = &s.payment_intent {
            return pi.clone();
        }
        let id = util::id("pi_");
        self.intents.insert(
            id.clone(),
            Intent {
                id: id.clone(),
                session: s.id.clone(),
                amount: s.amount,
                currency: s.currency.clone(),
                status: "requires_payment_method".into(),
                method: s.method.clone(),
                metadata_ref: s.pi_ref.clone(),
                received: 0,
                refunded: 0,
                charge: util::id("ch_"),
                country: None,
                brand: None,
                error: None,
                challenge: None,
                created: cx.now,
            },
        );
        bound(&mut self.intents);
        if let Some(session) = self.sessions.get_mut(&s.id) {
            session.payment_intent = Some(id.clone());
        }
        id
    }

    fn declined(&mut self, cx: &mut Cx<'_>, pi: &str, code: &str, decline: &str, message: &str) -> Pay {
        if let Some(i) = self.intents.get_mut(pi) {
            i.status = "requires_payment_method".into();
            i.error = Some((code.into(), decline.into(), message.into()));
            let j = Self::intent_json(i);
            cx.emit(event("payment_intent.payment_failed", j, cx.now));
        }
        Pay::Declined {
            code: code.into(),
            message: message.into(),
        }
    }

    /// A card or PayPal payment that went through: the charge, the PaymentIntent, the session.
    fn succeeded(&mut self, cx: &mut Cx<'_>, pi: &str) {
        let Some(i) = self.intents.get_mut(pi) else {
            return;
        };
        i.status = "succeeded".into();
        i.received = i.amount;
        i.error = None;
        i.challenge = None;
        let (charge, intent, session_id) = (Self::charge_json(i), Self::intent_json(i), i.session.clone());
        cx.emit(event("charge.succeeded", charge, cx.now));
        cx.emit(event("payment_intent.succeeded", intent, cx.now));
        if let Some(s) = self.sessions.get_mut(&session_id) {
            s.status = "complete".into();
            s.payment_status = "paid".into();
            let s = s.clone();
            let j = self.session_json(&s, cx.base);
            cx.emit(event("checkout.session.completed", j, cx.now));
        }
        if take(&mut cx.switches.chargeback_next) {
            self.steps.push((cx.now + 2 * cx.cfg.step_secs, Step::Dispute { pi: pi.into() }));
        }
    }

    /// A subscription's session completed: the plan starts (with its trial), its first invoice.
    fn subscribed(&mut self, cx: &mut Cx<'_>, s: &Session) {
        let sub_id = util::id("sub_");
        let start = s
            .customer
            .as_ref()
            .and_then(|c| self.customers.get(c).cloned().flatten())
            .and_then(|clock| self.clocks.get(&clock).copied())
            .unwrap_or(cx.now);
        let mut sub = Subscription {
            id: sub_id.clone(),
            status: if s.trial_days > 0 { "trialing".into() } else { "active".into() },
            amount: s.amount,
            currency: s.currency.clone(),
            metadata_ref: s.sub_ref.clone(),
            customer: s.customer.clone(),
            periods: 0,
            next_at: start + s.trial_days * DAY,
            created: cx.now,
        };
        if let Some(session) = self.sessions.get_mut(&s.id) {
            session.status = "complete".into();
            session.payment_status = if s.trial_days > 0 { "no_payment_required".into() } else { "paid".into() };
            session.subscription = Some(sub_id.clone());
        }
        if let Some(session) = self.sessions.get(&s.id).cloned() {
            let j = self.session_json(&session, cx.base);
            cx.emit(event("checkout.session.completed", j, cx.now));
        }
        let first = if s.trial_days > 0 { 0 } else { s.amount };
        if first > 0 {
            sub.periods = 1;
            sub.next_at = start + 30 * DAY;
        }
        let invoice = self.keep_invoice(Self::invoice_json(&sub, first, "subscription_create", None, cx.now));
        cx.emit(event("invoice.paid", invoice, cx.now));
        self.subscriptions.insert(sub_id, sub);
        bound(&mut self.subscriptions);
    }

    /// One more month of `sub_id`: paid, or failed when a switch says so.
    fn renew(&mut self, cx: &mut Cx<'_>, sub_id: &str) -> bool {
        let Some(sub) = self.subscriptions.get(sub_id).cloned() else {
            return false;
        };
        if !(sub.status == "active" || sub.status == "trialing") {
            return false;
        }
        if take(&mut cx.switches.return_next_debit) || take(&mut cx.switches.decline_next) {
            let invoice = self.keep_invoice(Self::invoice_json(&sub, 0, "subscription_cycle", None, cx.now));
            cx.emit(event("invoice.payment_failed", invoice, cx.now));
            return true;
        }
        let pi = util::id("pi_");
        self.intents.insert(
            pi.clone(),
            Intent {
                id: pi.clone(),
                session: String::new(),
                amount: sub.amount,
                currency: sub.currency.clone(),
                status: "succeeded".into(),
                method: "card".into(),
                metadata_ref: None,
                received: sub.amount,
                refunded: 0,
                charge: util::id("ch_"),
                country: None,
                brand: None,
                error: None,
                challenge: None,
                created: cx.now,
            },
        );
        bound(&mut self.intents);
        let invoice = self.keep_invoice(Self::invoice_json(&sub, sub.amount, "subscription_cycle", Some(&pi), cx.now));
        cx.emit(event("invoice.paid", invoice, cx.now));
        if let Some(s) = self.subscriptions.get_mut(sub_id) {
            s.periods += 1;
            s.status = "active".into();
            s.next_at += 30 * DAY;
        }
        true
    }

    fn pay_card(&mut self, cx: &mut Cx<'_>, id: &str, number: &str, exp: &str, cvc: &str) -> Pay {
        let Some(s) = self.open_session(cx, id) else {
            return Pay::Closed;
        };
        if s.method != "card" {
            return Pay::Refused { code: "payment_method_unexpected", message: format!("This test checkout takes {}, not a card.", s.method) };
        }
        let number = util::digits(number);
        let Some((brand, card)) = card_of(&number) else {
            return Pay::Refused { code: "sandbox_test_cards_only", message: TEST_CARDS_ONLY.into() };
        };
        let Some((month, year)) = expiry(exp) else {
            return Pay::Declined {
                code: "incomplete_expiry".into(),
                message: "Your card's expiration date is incomplete.".into(),
            };
        };
        let (y, m, _) = util::civil_now(cx.now);
        if !(1..=12).contains(&month) || year < y || (year == y && month < m) {
            return Pay::Declined {
                code: "invalid_expiry".into(),
                message: "Your card's expiration date is in the past.".into(),
            };
        }
        if !(3..=4).contains(&util::digits(cvc).len()) {
            return Pay::Declined {
                code: "incomplete_cvc".into(),
                message: "Your card's security code is incomplete.".into(),
            };
        }
        let last4: String = number.chars().skip(number.len().saturating_sub(4)).collect();
        if s.mode == "subscription" {
            return match card {
                Card::Decline(code, _, message) => Pay::Declined {
                    code: code.into(),
                    message: message.into(),
                },
                _ => {
                    self.subscribed(cx, &s);
                    Pay::Paid { last4 }
                }
            };
        }
        let pi = self.intent_for(cx, &s);
        let card = if take(&mut cx.switches.decline_next) {
            Card::Decline("card_declined", "generic_decline", DECLINED)
        } else if take(&mut cx.switches.three_ds_next) {
            Card::ThreeDs(false)
        } else {
            card
        };
        if let Some(i) = self.intents.get_mut(&pi) {
            i.brand = Some(brand.into());
            i.country = Some(match card {
                Card::Ok(c) => c.into(),
                _ => "US".into(),
            });
        }
        match card {
            Card::Ok(_) => {
                self.succeeded(cx, &pi);
                Pay::Paid { last4 }
            }
            Card::Dispute => {
                self.succeeded(cx, &pi);
                self.steps.push((cx.now + 2 * cx.cfg.step_secs, Step::Dispute { pi }));
                Pay::Paid { last4 }
            }
            Card::Decline(code, decline, message) => self.declined(cx, &pi, code, decline, message),
            Card::ThreeDs(then_decline) => {
                if let Some(i) = self.intents.get_mut(&pi) {
                    i.status = "requires_action".into();
                    i.challenge = Some(then_decline);
                }
                Pay::Challenge(pi)
            }
        }
    }

    fn pay_iban(&mut self, cx: &mut Cx<'_>, id: &str, iban: &str) -> Pay {
        let Some(s) = self.open_session(cx, id) else {
            return Pay::Closed;
        };
        if s.method != "sepa_debit" {
            return Pay::Refused { code: "payment_method_unexpected", message: format!("This test checkout takes {}, not a SEPA debit.", s.method) };
        }
        let iban = util::compact_iban(iban);
        if let Some(problem) = iban_problem(&iban) {
            return if problem == TEST_IBANS_ONLY {
                Pay::Refused { code: "sandbox_test_ibans_only", message: problem.into() }
            } else {
                Pay::Declined {
                    code: "invalid_bank_account_iban".into(),
                    message: problem.into(),
                }
            };
        }
        let Some(debit) = debit_of(&iban) else {
            return Pay::Refused { code: "sandbox_test_ibans_only", message: TEST_IBANS_ONLY.into() };
        };
        let pi = self.intent_for(cx, &s);
        if let Some(i) = self.intents.get_mut(&pi) {
            i.status = "processing".into();
            i.country = iban.get(..2).map(String::from);
        }
        if let Some(session) = self.sessions.get_mut(id) {
            session.status = "complete".into();
            session.payment_status = "unpaid".into();
        }
        if let Some(session) = self.sessions.get(id).cloned() {
            let j = self.session_json(&session, cx.base);
            cx.emit(event("checkout.session.completed", j, cx.now));
        }
        if let Some(i) = self.intents.get(&pi) {
            cx.emit(event("payment_intent.processing", Self::intent_json(i), cx.now));
        }
        let step = cx.cfg.step_secs;
        let debit = if take(&mut cx.switches.return_next_debit) { Debit::Fail("generic_decline") } else { debit };
        match debit {
            Debit::Ok => self.steps.push((cx.now + 2 * step, Step::Settle { pi: pi.clone(), decline: None })),
            Debit::Slow => self.steps.push((cx.now + 6 * step, Step::Settle { pi: pi.clone(), decline: None })),
            Debit::Fail(code) => self.steps.push((cx.now + 2 * step, Step::Settle { pi: pi.clone(), decline: Some(code) })),
            Debit::Dispute => {
                self.steps.push((cx.now + 2 * step, Step::Settle { pi: pi.clone(), decline: None }));
                self.steps.push((cx.now + 4 * step, Step::Dispute { pi: pi.clone() }));
            }
        }
        if take(&mut cx.switches.chargeback_next) {
            self.steps.push((cx.now + 4 * step, Step::Dispute { pi }));
        }
        Pay::Processing
    }

    fn pay_paypal(&mut self, cx: &mut Cx<'_>, id: &str) -> Pay {
        let Some(s) = self.open_session(cx, id) else {
            return Pay::Closed;
        };
        if s.method != "paypal" {
            return Pay::Refused { code: "payment_method_unexpected", message: format!("This test checkout takes {}, not PayPal.", s.method) };
        }
        let pi = self.intent_for(cx, &s);
        self.succeeded(cx, &pi);
        Pay::Paid { last4: String::new() }
    }

    fn create_session(&mut self, cx: &mut Cx<'_>, f: &BTreeMap<String, String>) -> (u16, Value) {
        let get = |k: &str| f.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let bad = |param: &str| error(400, "invalid_request_error", Some("parameter_invalid"), &format!("Invalid or missing {param}."));
        let Some(mode) = get("mode").filter(|m| m == "payment" || m == "subscription") else {
            return bad("mode");
        };
        let ui_mode = get("ui_mode").unwrap_or_else(|| "hosted_page".into());
        if ui_mode != "hosted_page" && ui_mode != "elements" {
            return bad("ui_mode (this sandbox shows hosted_page and elements)");
        }
        let Some(currency) = get("line_items[0][price_data][currency]").filter(|c| c.len() == 3) else {
            return bad("line_items[0][price_data][currency]");
        };
        let Some(amount) = get("line_items[0][price_data][unit_amount]").and_then(|a| a.parse::<i64>().ok()).filter(|a| *a > 0) else {
            return bad("line_items[0][price_data][unit_amount]");
        };
        let Some(product) = get("line_items[0][price_data][product_data][name]") else {
            return bad("line_items[0][price_data][product_data][name]");
        };
        let method = get("payment_method_types[0]").unwrap_or_else(|| "card".into());
        let presented = if OWN_PAGES.contains(&method.as_str()) {
            None
        } else {
            let converts = get("adaptive_pricing[enabled]").as_deref() == Some("true");
            match self.local_rules(&method, &mode, &ui_mode, &currency, amount, converts, &get) {
                Ok(p) => p,
                Err(e) => return e,
            }
        };
        if mode == "subscription" {
            if get("line_items[0][price_data][recurring][interval]").as_deref() != Some("month") {
                return bad("line_items[0][price_data][recurring][interval] (month)");
            }
            if method == "paypal" {
                return bad("payment_method_types (no PayPal subscriptions in this sandbox)");
            }
        }
        if ui_mode == "elements" {
            if f.contains_key("cancel_url") {
                return error(400, "invalid_request_error", Some("parameter_unknown"), "cancel_url is not allowed with ui_mode elements.");
            }
            if get("return_url").is_none() {
                return bad("return_url");
            }
        } else if get("success_url").is_none() {
            return bad("success_url");
        }
        let expires_at = match get("expires_at") {
            Some(t) => match t.parse::<i64>() {
                Ok(t) if t >= cx.now + 1800 && t <= cx.now + DAY => t,
                _ => return bad("expires_at (30 minutes to 24 hours ahead)"),
            },
            None => cx.now + DAY,
        };
        let customer = get("customer");
        if let Some(c) = &customer {
            if !self.customers.contains_key(c) {
                return error(400, "invalid_request_error", Some("resource_missing"), "No such customer.");
            }
        }
        let id = util::id("cs_test_");
        let client_secret = (ui_mode == "elements").then(|| format!("{id}_secret_{}", util::id("")));
        let s = Session {
            id: id.clone(),
            mode,
            ui_mode,
            status: "open".into(),
            payment_status: "unpaid".into(),
            amount,
            currency: currency.to_ascii_lowercase(),
            method,
            product: product.chars().take(200).collect(),
            client_reference_id: get("client_reference_id"),
            metadata_ref: get("metadata[provider_ref]"),
            pi_ref: get("payment_intent_data[metadata][provider_ref]"),
            sub_ref: get("subscription_data[metadata][provider_ref]"),
            trial_days: get("subscription_data[trial_period_days]").and_then(|d| d.parse::<i64>().ok()).unwrap_or(0).clamp(0, 730),
            success_url: get("success_url"),
            cancel_url: get("cancel_url"),
            return_url: get("return_url"),
            client_secret: client_secret.clone(),
            expires_at,
            created: cx.now,
            payment_intent: None,
            subscription: None,
            customer,
            presented,
        };
        let j = self.session_json(&s, cx.base);
        if let Some(cs) = client_secret {
            self.secrets.insert(cs, id.clone());
            bound(&mut self.secrets);
        }
        self.sessions.insert(id, s);
        bound(&mut self.sessions);
        (200, j)
    }

    fn refund(&mut self, cx: &mut Cx<'_>, f: &BTreeMap<String, String>) -> (u16, Value) {
        let pi = f.get("payment_intent").cloned().unwrap_or_default();
        let Some(i) = self.intents.get_mut(&pi) else {
            return error(400, "invalid_request_error", Some("resource_missing"), "No such payment_intent.");
        };
        if i.status != "succeeded" {
            return error(400, "invalid_request_error", Some("charge_not_refundable"), "This PaymentIntent has not succeeded.");
        }
        let left = i.received - i.refunded;
        let amount = f.get("amount").and_then(|a| a.parse::<i64>().ok()).unwrap_or(left);
        if amount <= 0 || amount > left {
            return error(400, "invalid_request_error", Some("amount_too_large"), "The refund is more than what is left to refund.");
        }
        i.refunded += amount;
        let refund = json!({
            "id": util::id("re_"), "object": "refund", "amount": amount, "currency": i.currency,
            "payment_intent": i.id, "charge": i.charge, "status": "succeeded",
            "metadata": util::ref_meta(&f.get("metadata[provider_ref]").cloned()), "created": cx.now,
        });
        let charge = Self::charge_json(i);
        cx.emit(event("refund.created", refund.clone(), cx.now));
        cx.emit(event("charge.refunded", charge, cx.now));
        if let Some(id) = refund["id"].as_str() {
            self.refunds.insert(id.to_string(), refund.clone());
            bound(&mut self.refunds);
        }
        (200, refund)
    }

    fn api(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, segs: &[&str]) -> (u16, Value) {
        let missing = || error(404, "invalid_request_error", Some("resource_missing"), "No such object.");
        match (req.method, segs) {
            ("POST", ["checkout", "sessions"]) => self.create_session(cx, &req.form()),
            ("GET", ["checkout", "sessions", id]) => match self.sessions.get(*id) {
                Some(s) => (200, self.session_json(s, cx.base)),
                None => missing(),
            },
            ("POST", ["checkout", "sessions", id, "expire"]) => {
                let Some(s) = self.sessions.get_mut(*id) else {
                    return missing();
                };
                if s.status != "open" {
                    return error(400, "invalid_request_error", Some("checkout_session_not_expirable"), "Only an open Checkout Session can be expired.");
                }
                s.status = "expired".into();
                let s = s.clone();
                let j = self.session_json(&s, cx.base);
                cx.emit(event("checkout.session.expired", j.clone(), cx.now));
                (200, j)
            }
            ("GET", ["payment_intents", id]) => {
                let expand = req.query_param("expand[]").is_some_and(|e| e.contains("balance_transaction"));
                self.intents.get(*id).map_or_else(missing, |i| (200, Self::intent_read(i, expand)))
            }
            ("POST", ["refunds"]) => self.refund(cx, &req.form()),
            ("GET", ["refunds", id]) => {
                let expand = req.query_param("expand[]").is_some_and(|e| e.contains("balance_transaction"));
                self.refunds.get(*id).map_or_else(missing, |r| {
                    let mut r = r.clone();
                    if expand {
                        let amount = r["amount"].as_i64().unwrap_or(0);
                        let currency = r["currency"].as_str().unwrap_or("EUR").to_string();
                        let euros = local::convert(amount, &currency, "EUR").unwrap_or(0);
                        r["balance_transaction"] = balance_tx(id, -euros);
                    }
                    (200, r)
                })
            }
            ("GET", ["disputes", id]) => self.disputes.get(*id).map_or_else(missing, |d| (200, d.clone())),
            ("GET", ["invoices", id]) => self.invoices.get(*id).map_or_else(missing, |i| (200, i.clone())),
            ("GET", ["subscriptions", id]) => self.subscriptions.get(*id).map_or_else(missing, |s| (200, Self::subscription_json(s))),
            ("DELETE", ["subscriptions", id]) => {
                let Some(s) = self.subscriptions.get_mut(*id) else {
                    return missing();
                };
                if s.status != "canceled" {
                    s.status = "canceled".into();
                    let j = Self::subscription_json(s);
                    cx.emit(event("customer.subscription.deleted", j, cx.now));
                }
                (200, Self::subscription_json(s))
            }
            ("POST", ["customers"]) => {
                let f = req.form();
                let clock = f.get("test_clock").cloned().filter(|c| self.clocks.contains_key(c));
                let id = util::id("cus_");
                self.customers.insert(id.clone(), clock.clone());
                bound(&mut self.customers);
                (200, json!({"id": id, "object": "customer", "test_clock": clock, "livemode": false}))
            }
            ("POST", ["test_helpers", "test_clocks"]) => {
                let f = req.form();
                let frozen = f.get("frozen_time").and_then(|t| t.parse::<i64>().ok()).unwrap_or(cx.now);
                let id = util::id("clock_");
                self.clocks.insert(id.clone(), frozen);
                bound(&mut self.clocks);
                (200, json!({"id": id, "object": "test_helpers.test_clock", "frozen_time": frozen, "status": "ready"}))
            }
            ("POST", ["test_helpers", "test_clocks", id, "advance"]) => {
                let f = req.form();
                let Some(old) = self.clocks.get(*id).copied() else {
                    return missing();
                };
                let to = f.get("frozen_time").and_then(|t| t.parse::<i64>().ok()).filter(|t| *t > old).unwrap_or(old);
                self.clocks.insert(id.to_string(), to);
                let on_clock: Vec<String> = self
                    .subscriptions
                    .values()
                    .filter(|s| s.customer.as_ref().and_then(|c| self.customers.get(c).cloned().flatten()).as_deref() == Some(*id))
                    .map(|s| s.id.clone())
                    .collect();
                for sub in on_clock {
                    for _ in 0..MAX_PERIODS {
                        let due = self.subscriptions.get(&sub).is_some_and(|s| s.next_at <= to);
                        if !due || !self.renew(cx, &sub) {
                            break;
                        }
                    }
                }
                (200, json!({"id": id, "object": "test_helpers.test_clock", "frozen_time": to, "status": "ready"}))
            }
            _ => error(404, "invalid_request_error", Some("resource_missing"), "Unrecognized request URL."),
        }
    }

    fn api_call(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, segs: &[&str]) -> Response {
        if req.bearer() != Some(cx.cfg.stripe.secret_key.as_str()) {
            let (s, v) = error(401, "invalid_request_error", None, "Invalid API Key provided.");
            return Response::json(s, v);
        }
        let key = if req.method == "POST" { req.header("idempotency-key").map(String::from) } else { None };
        let mut fingerprint_input = format!("{} {}\n", req.method, req.path).into_bytes();
        fingerprint_input.extend_from_slice(req.body);
        let fingerprint = util::hmac_hex(b"idempotency fingerprint", &fingerprint_input);
        if let Some(k) = &key {
            if let Some((fp, status, answer)) = self.idem.get(k) {
                if *fp == fingerprint {
                    return Response::json(*status, answer.clone());
                }
                let (s, v) = error(400, "idempotency_error", None, "Keys for idempotent requests can only be used with the same parameters they were first used with.");
                return Response::json(s, v);
            }
        }
        let (status, answer) = self.api(cx, req, segs);
        // Stripe keeps a key's result only once the endpoint ran: a request that failed its
        // validation (400) or named no endpoint (404) may be sent again with the same key and
        // other parameters; a card error (402) happened while running and is kept.
        let ran = status < 400 || status == 402;
        if let Some(k) = key.filter(|_| ran) {
            self.idem.insert(k, (fingerprint, status, answer.clone()));
            bound(&mut self.idem);
        }
        Response::json(status, answer)
    }

    // ---- the payer's pages ----

    fn checkout_page(&self, s: &Session, base: &str, message: &str) -> Response {
        let amount = local::money(s.amount, &s.currency);
        let note = if s.mode == "subscription" {
            format!("{amount} per month, after a {}-day trial", s.trial_days)
        } else {
            "One payment".to_string()
        };
        let mut left = kit::order(&s.product, &amount, &note);
        if let Some((shown, currency)) = &s.presented {
            let converted = local::money(*shown, currency);
            left.push_str(&format!("<p class=\"muted\">You pay {} at a [TEST] fake rate.</p>", kit::esc(&converted)));
        }
        let error = format!("<div class=\"error\">{}</div>", kit::esc(message));
        let cancel = s
            .cancel_url
            .as_ref()
            .map(|u| format!("<p class=\"muted\"><a href=\"{}\">Cancel and go back</a></p>", kit::esc(u)))
            .unwrap_or_default();
        let pay = if s.mode == "subscription" { "Start the plan".to_string() } else { format!("Pay {amount}") };
        if let Some(l) = local_of(&self.registry, &s.method) {
            let phone = format!("{base}/stripe/phone/{}", s.id);
            let status = format!("{base}/stripe/c/pay/{}/status", s.id);
            let done = s.success_url.clone().or(s.return_url.clone()).unwrap_or_else(|| base.to_string());
            let right = format!("{}{cancel}", l.form(&pay, &phone, &status, &done, message));
            return Response::html(200, kit::split("Checkout", SHAPE, &left, &right, local::POLL_SCRIPT));
        }
        let right = match s.method.as_str() {
            "sepa_debit" => format!(
                "<h2>SEPA Direct Debit (test)</h2><form class=\"iban\" method=\"post\" autocomplete=\"off\">\
                 <label>Account holder<input name=\"name\" autocomplete=\"off\"></label>\
                 <label>IBAN<input name=\"iban\" placeholder=\"DE89 3704 0044 0532 0130 00\" autocomplete=\"off\"></label>\
                 <p class=\"muted\">By paying you allow this test merchant to debit your account (a test mandate: no bank is asked).</p>\
                 {error}<button>{}</button></form>{cancel}",
                kit::esc(&pay)
            ),
            "paypal" => format!(
                "<h2>PayPal-shaped approval (test)</h2><form method=\"post\"><input type=\"hidden\" name=\"approve\" value=\"1\">\
                 <p class=\"muted\">No login: this test approval stands in for the wallet's.</p>{error}<button>Approve (test)</button></form>{cancel}"
            ),
            _ => format!(
                "<h2>Pay with a test card</h2><form class=\"card\" method=\"post\" autocomplete=\"off\">\
                 <label>Card number<input name=\"card_number\" inputmode=\"numeric\" placeholder=\"4242 4242 4242 4242\" autocomplete=\"off\"></label>\
                 <div class=\"row\"><div><label>MM / YY<input name=\"exp\" placeholder=\"12 / 34\" autocomplete=\"off\"></label></div>\
                 <div><label>CVC<input name=\"cvc\" placeholder=\"123\" autocomplete=\"off\"></label></div></div>\
                 <label>Country<select name=\"country\"><option>DE</option><option>AT</option><option>FR</option><option>NL</option><option>US</option></select></label>\
                 {error}<button>{}</button></form>{cancel}",
                kit::esc(&pay)
            ),
        };
        let script = format!("{}{}", kit::CARD_SCRIPT, kit::IBAN_SCRIPT);
        Response::html(200, kit::split("Checkout", SHAPE, &left, &right, &script))
    }

    fn closed_page(status: u16) -> Response {
        let body = "<h2>This test checkout is closed</h2><p>It was paid, expired or cancelled. Go back to the app.</p>";
        Response::html(status, kit::single("Checkout closed", SHAPE, body, ""))
    }

    fn checkout_submit(&mut self, cx: &mut Cx<'_>, id: &str, req: &Request<'_>) -> Response {
        let Some(s) = self.sessions.get(id).cloned() else {
            return Self::closed_page(404);
        };
        let f = req.form();
        if local_of(&self.registry, &s.method).is_some() {
            return self.pay_local(cx, &s.id, &f);
        }
        let field = |k: &str| f.get(k).cloned().unwrap_or_default();
        let pay = match s.method.as_str() {
            "sepa_debit" => self.pay_iban(cx, id, &field("iban")),
            "paypal" => self.pay_paypal(cx, id),
            _ => self.pay_card(cx, id, &field("card_number"), &field("exp"), &field("cvc")),
        };
        let back = s.success_url.clone().or(s.return_url.clone()).unwrap_or_else(|| cx.base.to_string());
        match pay {
            Pay::Paid { .. } | Pay::Processing => Response::redirect(&back),
            Pay::Challenge(pi) => Response::redirect(&format!("{}/stripe/3ds/{pi}", cx.base)),
            Pay::Declined { message, .. } | Pay::Refused { message, .. } => self.checkout_page(&s, cx.base, &message),
            Pay::Closed => Self::closed_page(409),
        }
    }

    fn challenge_page(&self, pi: &str) -> Response {
        let Some(i) = self.intents.get(pi).filter(|i| i.challenge.is_some()) else {
            return Self::closed_page(404);
        };
        let body = format!(
            "<h2>Test bank: confirm this payment</h2><p>A 3-D Secure step for {} to a test merchant.</p>\
             <form method=\"post\"><button name=\"action\" value=\"complete\">Complete</button>\
             <button class=\"plain\" name=\"action\" value=\"fail\">Fail</button></form>",
            kit::esc(&kit::money(i.amount, &i.currency))
        );
        Response::html(200, kit::single("Confirm the payment", SHAPE, &body, ""))
    }

    /// The challenge answered: paid, or failed back to the session's page.
    fn challenge_submit(&mut self, cx: &mut Cx<'_>, pi: &str, req: &Request<'_>) -> Response {
        let json_wanted = req.header("content-type").is_some_and(|t| t.contains("json"));
        let action = if json_wanted {
            req.json().get("action").and_then(Value::as_str).unwrap_or("").to_string()
        } else {
            req.form().get("action").cloned().unwrap_or_default()
        };
        let Some(i) = self.intents.get(pi).cloned() else {
            return Self::closed_page(404);
        };
        let Some(then_decline) = i.challenge else {
            return Self::closed_page(409);
        };
        let session = self.sessions.get(&i.session).cloned();
        let pay = if action == "complete" && !then_decline {
            self.succeeded(cx, pi);
            Pay::Paid { last4: String::new() }
        } else if action == "complete" {
            self.declined(cx, pi, "card_declined", "generic_decline", DECLINED)
        } else {
            if let Some(x) = self.intents.get_mut(pi) {
                x.challenge = None;
            }
            self.declined(cx, pi, "authentication_required", "", "We are unable to authenticate your payment method. Choose another payment method and try again.")
        };
        if let Some(x) = self.intents.get_mut(pi) {
            x.challenge = None;
        }
        if json_wanted {
            return Response::json(200, pay_json(&pay));
        }
        let Some(s) = session else {
            return Self::closed_page(404);
        };
        match pay {
            Pay::Paid { .. } => Response::redirect(&s.success_url.or(s.return_url).unwrap_or_else(|| cx.base.to_string())),
            _ => Response::redirect(&format!("{}/stripe/c/pay/{}", cx.base, s.id)),
        }
    }

    // ---- the local methods ----

    /// Stripe's refusals for a local method, mirrored: a type without documented test data, a
    /// method that cannot recur in subscription mode, the Payment Element (this sandbox shows
    /// local methods on its hosted page), WeChat Pay without its client, another currency without
    /// Adaptive Pricing. Otherwise the converted price the payer sees, if Stripe converts.
    #[allow(clippy::too_many_arguments)]
    fn local_rules(
        &self,
        method: &str,
        mode: &str,
        ui_mode: &str,
        currency: &str,
        amount: i64,
        converts: bool,
        get: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Option<(i64, String)>, (u16, Value)> {
        let refuse = |code: &str, message: String| error(400, "invalid_request_error", Some(code), &message);
        let Some(l) = local_of(&self.registry, method) else {
            let why = format!("This sandbox has no documented test data for payment_method_types {method}.");
            return Err(refuse("payment_method_unavailable", why));
        };
        let r = l.route;
        if mode == "subscription" && !r.recurs() {
            return Err(refuse("parameter_invalid", format!("{method} cannot be used in subscription mode.")));
        }
        if ui_mode != "hosted_page" {
            return Err(refuse("parameter_invalid", format!("This sandbox shows {method} on its hosted page only.")));
        }
        if method == "wechat_pay" && get("payment_method_options[wechat_pay][client]").as_deref() != Some("web") {
            let why = "payment_method_options[wechat_pay][client] is required for wechat_pay in Checkout.".to_string();
            return Err(refuse("parameter_missing", why));
        }
        if r.takes_currency(currency) {
            return Ok(None);
        }
        let allowed = converts && if mode == "subscription" { r.adaptive_recurring } else { r.adaptive };
        match (allowed, r.currencies.iter().find(|c| *c != "*")) {
            (true, Some(to)) => {
                let shown = local::convert(amount, currency, to)
                    .ok_or_else(|| refuse("parameter_invalid", format!("This sandbox has no test rate for {to}.")))?;
                Ok(Some((shown, to.to_ascii_lowercase())))
            }
            _ => Err(refuse("payment_method_invalid_parameter", format!("payment_method_types {method} is invalid for currency {currency}."))),
        }
    }

    /// The session's state for a page that waits for the payer's phone.
    fn status(&self, id: &str) -> Response {
        match self.sessions.get(id) {
            Some(s) => Response::json(200, json!({"status": s.status, "payment_status": s.payment_status})),
            None => Response::json(404, json!({"error": "resource_missing"})),
        }
    }

    /// The PaymentIntent waits: for the bank app (`processing`) or for the payer (`requires_action`).
    fn waiting(&mut self, cx: &mut Cx<'_>, pi: &str, status: &str) {
        if let Some(i) = self.intents.get_mut(pi) {
            i.status = status.into();
            let j = Self::intent_json(i);
            cx.emit(event(&format!("payment_intent.{status}"), j, cx.now));
        }
    }

    /// A voucher or a code issued: the session completes unpaid and the payment waits.
    fn awaiting_payer(&mut self, cx: &mut Cx<'_>, id: &str, pi: &str) {
        if let Some(session) = self.sessions.get_mut(id) {
            session.status = "complete".into();
            session.payment_status = "unpaid".into();
        }
        if let Some(session) = self.sessions.get(id).cloned() {
            let j = self.session_json(&session, cx.base);
            cx.emit(event("checkout.session.completed", j, cx.now));
        }
        self.waiting(cx, pi, "requires_action");
    }

    /// A local payment went through: a plan starts, a payment succeeds.
    fn paid_local(&mut self, cx: &mut Cx<'_>, s: &Session, pi: &str) {
        if s.mode == "subscription" {
            self.subscribed(cx, s);
        } else {
            self.succeeded(cx, pi);
        }
    }

    /// A local method's checkout form: the documented input it names decides what happens.
    fn pay_local(&mut self, cx: &mut Cx<'_>, id: &str, form: &BTreeMap<String, String>) -> Response {
        let Some(s) = self.open_session(cx, id) else {
            return Self::closed_page(409);
        };
        let Some(l) = local_of(&self.registry, &s.method) else {
            return Self::closed_page(404);
        };
        let (flow, name) = (l.method.flow, l.method.name.clone());
        let input = match l.answer(form) {
            Answer::Matched(t) => t,
            Answer::Refused(message) => return self.checkout_page(&s, cx.base, &message),
        };
        let back = s.success_url.clone().or(s.return_url.clone()).unwrap_or_else(|| cx.base.to_string());
        let phone = format!("{}/stripe/phone/{}", cx.base, s.id);
        let pi = self.intent_for(cx, &s);
        let failed = input.outcome != Outcome::Succeeded;
        if failed && input.after_minutes == 0 && matches!(flow, Flow::Redirect | Flow::Code | Flow::Push) {
            // declined at once: the payer stays on the page and may try again
            let code = local::decline_code(&input);
            self.declined(cx, &pi, &code, "", "The test payment failed.");
            return self.checkout_page(&s, cx.base, &format!("The test payment failed ({code})."));
        }
        let later = cx.now + cx.cfg.step_secs * input.after_minutes.max(1);
        let ends = if failed {
            Step::Expire { pi: pi.clone(), code: local::decline_code(&input) }
        } else {
            Step::Settle { pi: pi.clone(), decline: None }
        };
        match flow {
            Flow::Redirect => {
                self.paid_local(cx, &s, &pi);
                Response::redirect(&back)
            }
            Flow::Code => {
                // the code is right: the bank app asks the payer, or the documented failure comes
                self.waiting(cx, &pi, "processing");
                if failed {
                    self.steps.push((later, ends));
                }
                Response::redirect(&phone)
            }
            Flow::Push => {
                // a documented number: the app answers by itself after its time
                self.waiting(cx, &pi, "requires_action");
                let step = if failed { ends } else { Step::Approve { pi } };
                self.steps.push((later, step));
                Response::redirect(&phone)
            }
            Flow::Qr | Flow::Voucher => {
                self.awaiting_payer(cx, &s.id, &pi);
                self.steps.push((later, ends));
                let amount = local::money(s.amount, &s.currency);
                if flow == Flow::Qr {
                    let status = format!("{}/stripe/c/pay/{}/status", cx.base, s.id);
                    let body = format!("{}{}", local::qr_box(&phone), local::waiting(&status, &back));
                    Response::html(200, kit::single("Scan the test code", SHAPE, &body, local::POLL_SCRIPT))
                } else {
                    Response::html(200, local::voucher(SHAPE, &name, &amount, &local::reference(&s.id), &back))
                }
            }
            Flow::Fields | Flow::Debit | Flow::Transfer => Self::closed_page(404),
        }
    }

    /// The fake phone: the method's app, standing in for the payer's approval.
    fn phone_page(&self, id: &str) -> Response {
        let Some(s) = self.sessions.get(id) else {
            return Self::closed_page(404);
        };
        let Some(l) = local_of(&self.registry, &s.method) else {
            return Self::closed_page(404);
        };
        let processing = s
            .payment_intent
            .as_ref()
            .and_then(|p| self.intents.get(p))
            .is_some_and(|i| i.status == "processing");
        let actions: Vec<(&str, &str)> = if s.status != "open" {
            Vec::new()
        } else if l.method.flow == Flow::Code && processing {
            vec![("authorize", "Approve (test)"), ("fail", "Decline (test)"), ("timeout", "Let it time out (test)")]
        } else if l.decided_on_phone() {
            vec![("authorize", "Approve (test)"), ("fail", "Decline (test)")]
        } else {
            Vec::new()
        };
        let note = if actions.is_empty() {
            "Nothing to do here: the sandbox decides this test payment from its documented test data."
        } else {
            "Only this sandbox listens to these buttons."
        };
        let amount = local::money(s.amount, &s.currency);
        Response::html(200, l.phone(SHAPE, &amount, &actions, note))
    }

    /// An answer on the fake phone: approved, declined, or (a BLIK code) left to time out.
    fn phone_submit(&mut self, cx: &mut Cx<'_>, id: &str, req: &Request<'_>) -> Response {
        let f = req.form();
        let action = f.get("action").cloned().unwrap_or_default();
        let Some(s) = self.open_session(cx, id) else {
            return Self::closed_page(409);
        };
        let Some(l) = local_of(&self.registry, &s.method) else {
            return Self::closed_page(404);
        };
        let (flow, on_phone) = (l.method.flow, l.decided_on_phone());
        let answered = on_phone.then(|| l.answer(&f));
        let pi = self.intent_for(cx, &s);
        let processing = self.intents.get(&pi).is_some_and(|i| i.status == "processing");
        let decision = if flow == Flow::Code && processing {
            match action.as_str() {
                "authorize" => Some(None),
                "fail" => Some(Some("payment_method_provider_decline".to_string())),
                "timeout" => Some(Some("payment_method_provider_timeout".to_string())),
                _ => None,
            }
        } else {
            match answered {
                Some(Answer::Matched(t)) if t.outcome == Outcome::Succeeded => Some(None),
                Some(Answer::Matched(t)) => Some(Some(local::decline_code(&t))),
                _ => None,
            }
        };
        match decision {
            Some(None) => self.paid_local(cx, &s, &pi),
            Some(Some(code)) => {
                self.declined(cx, &pi, &code, "", "The test payment was not approved.");
            }
            None => return Response::html(400, kit::single("Not here", SHAPE, "<h2>Nothing to answer here (test)</h2>", "")),
        }
        self.phone_page(id)
    }

    /// What the fields page learns from its client secret (Stripe.js gets the same).
    fn elements_info(&self, req: &Request<'_>) -> Response {
        let cs = req.query_param("cs").unwrap_or_default();
        let Some(s) = self.secrets.get(&cs).and_then(|id| self.sessions.get(id)) else {
            return Response::json(404, json!({"error": "resource_missing"}));
        };
        Response::json(200, json!({"method": s.method, "amount": s.amount, "currency": s.currency,
                                   "mode": s.mode, "product": s.product, "status": s.status}))
    }

    /// The fields page's confirm (the Payment Element's `confirmPayment`).
    fn confirm(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let b = req.json();
        let field = |k: &str| b.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let Some(id) = self.secrets.get(&field("client_secret")).cloned() else {
            return Response::json(404, json!({"result": "failed", "code": "resource_missing", "message": "No such checkout session."}));
        };
        let method = self.sessions.get(&id).map(|s| s.method.clone()).unwrap_or_default();
        let pay = match method.as_str() {
            "sepa_debit" => self.pay_iban(cx, &id, &field("iban")),
            _ => self.pay_card(cx, &id, &field("card_number"), &field("exp"), &field("cvc")),
        };
        let status = if matches!(pay, Pay::Closed) { 409 } else { 200 };
        Response::json(status, pay_json(&pay))
    }
}

fn pay_json(pay: &Pay) -> Value {
    match pay {
        Pay::Paid { last4 } if last4.is_empty() => json!({"result": "succeeded"}),
        Pay::Paid { last4 } => json!({"result": "succeeded", "last4": last4}),
        Pay::Processing => json!({"result": "processing"}),
        Pay::Challenge(pi) => json!({"result": "requires_action", "pi": pi}),
        Pay::Declined { code, message } => json!({"result": "failed", "code": code, "message": message}),
        Pay::Refused { code, message } => json!({"result": "failed", "code": code, "message": message}),
        Pay::Closed => json!({"result": "failed", "code": "session_closed", "message": "This checkout is closed."}),
    }
}

/// The fields page an app's payment popover loads: the inputs only (the frame, the cardholder
/// name and the Pay button are the app's), talking to the app through main-frame navigations
/// to `/_bridge/<message>` (the app cancels them; a browser gets a 204 and stays) and hearing it
/// through new fragments (`#...&cmd=confirm&name=...&n=<seq>`).
const FIELDS_SCRIPT: &str = "\
var q=new URLSearchParams(location.hash.slice(1)),cs=q.get('cs')||'',m='card',queue=[],busy=false,seen='',last='';\
var F=document.getElementById('f'),E=document.getElementById('err'),S=document.getElementById('sca');\
function go(){if(busy||!queue.length)return;busy=true;location.href=queue.shift();setTimeout(function(){busy=false;go()},150)}\
function bridge(msg,a){var s=new URLSearchParams(a||{}).toString();queue.push('/_bridge/'+msg+(s?'?'+s:''));go()}\
function d(s){return s.replace(/\\D/g,'')}\
function state(){var n=d(F.card_number.value),c=m=='card'?n.length>=14&&d(F.exp.value).length==4&&d(F.cvc.value).length>=3:F.iban.value.replace(/\\s/g,'').length>=15;\
var b=/^4/.test(n)?'visa':/^(5[1-5]|2[2-7])/.test(n)?'mastercard':/^3[47]/.test(n)?'amex':'unknown',now=b+c;\
if(now!=last){last=now;if(m=='card')bridge('brand',{v:b});bridge('complete',{v:c?'1':'0'})}}\
F.oninput=function(){F.card_number.value=d(F.card_number.value).slice(0,19).replace(/(.{4})/g,'$1 ').trim();\
var e=d(F.exp.value).slice(0,4);F.exp.value=e.length>2?e.slice(0,2)+' / '+e.slice(2):e;F.cvc.value=d(F.cvc.value).slice(0,4);state()};\
function post(u,b){return fetch(u,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(b)}).then(function(r){return r.json()})}\
function done(a){if(a.last4)bridge('last4',{v:a.last4});if(a.message){E.textContent=a.message;bridge('error',{code:a.code||'failed',message:a.message})}\
if(a.result=='requires_action'){S.hidden=false;S.dataset.pi=a.pi;bridge('height',{v:'560'})}bridge('result',a.code?{v:a.result,code:a.code}:{v:a.result})}\
window.addEventListener('hashchange',function(){var c=new URLSearchParams(location.hash.slice(1)),k=c.get('cmd'),n=c.get('n')||'';if(n&&n==seen)return;seen=n;\
if(k=='reset'){F.reset();E.textContent='';S.hidden=true;last='';state();return}if(k!='confirm')return;E.textContent='';\
post('/stripe/confirm',{client_secret:cs,card_number:d(F.card_number.value),exp:d(F.exp.value),cvc:d(F.cvc.value),iban:F.iban.value,country:F.country.value}).then(done)});\
S.onclick=function(ev){var a=ev.target.value;if(!a)return;S.hidden=true;post('/stripe/3ds/'+S.dataset.pi,{action:a}).then(done)};\
fetch('/stripe/elements?cs='+encodeURIComponent(cs)).then(function(r){return r.json()}).then(function(s){m=s.method||'card';\
document.getElementById('card').hidden=m!='card';document.getElementById('sepa').hidden=m!='sepa_debit';bridge('ready');if(F.dataset.filled)state()});";

/// The fields page; `fill`: a test card in its inputs, as if typed (`/_dev/fill`), and the form
/// marked so the page tells the app the brand and the card's completeness once it is ready.
fn fields_page(fill: Option<crate::engine::Fill>) -> Response {
    // only digits reach here (`/_dev/fill` refuses anything else), grouped as the page groups them
    let (filled, number, exp, cvc) = match &fill {
        Some(f) => {
            let grouped: Vec<String> = f.card_number.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
            let exp = format!("{} / {}", f.exp.get(..2).unwrap_or(""), f.exp.get(2..).unwrap_or(""));
            (" data-filled=\"1\"", format!(" value=\"{}\"", grouped.join(" ")), format!(" value=\"{exp}\""), format!(" value=\"{}\"", f.cvc))
        }
        None => ("", String::new(), String::new(), String::new()),
    };
    let body = format!("<!-- the bridge: /_bridge/ready, /_bridge/height, /_bridge/brand, /_bridge/complete, /_bridge/last4, /_bridge/error, /_bridge/result -->\
        <form id=\"f\" autocomplete=\"off\" onsubmit=\"return false\"{filled}><div id=\"card\">\
        <label>Card number<input name=\"card_number\" inputmode=\"numeric\" placeholder=\"4242 4242 4242 4242\" autocomplete=\"off\"{number}></label>\
        <div class=\"row\"><div><label>MM / YY<input name=\"exp\" placeholder=\"12 / 34\" autocomplete=\"off\"{exp}></label></div>\
        <div><label>CVC<input name=\"cvc\" placeholder=\"123\" autocomplete=\"off\"{cvc}></label></div></div>\
        <label>Country<select name=\"country\"><option>DE</option><option>AT</option><option>FR</option><option>NL</option><option>US</option></select></label></div>\
        <div id=\"sepa\" hidden><label>IBAN<input name=\"iban\" placeholder=\"DE89 3704 0044 0532 0130 00\" autocomplete=\"off\"></label>\
        <p class=\"muted\">A test mandate: no bank is asked.</p></div><div class=\"error\" id=\"err\"></div></form>\
        <div id=\"sca\" hidden><h3>Test bank: confirm this payment</h3><button value=\"complete\">Complete</button>\
        <button class=\"plain\" value=\"fail\">Fail</button></div>");
    Response::html(200, kit::single("Card fields", SHAPE, &body, FIELDS_SCRIPT))
}

impl FakeProvider for FakeStripe {
    fn kind(&self) -> Kind {
        Kind::Stripe
    }

    fn owns(&self, path: &str) -> bool {
        path == FIELDS_PAGE
    }

    fn handle(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        if req.path == FIELDS_PAGE {
            return if req.method == "GET" { fields_page(cx.switches.fill_next.take()) } else { Response::not_found() };
        }
        let segs: Vec<&str> = req.path.trim_start_matches('/').split('/').collect();
        match (req.method, segs.as_slice()) {
            ("GET", ["c", "pay", id]) => match self.open_session(cx, id) {
                Some(s) if s.ui_mode == "hosted_page" => self.checkout_page(&s, cx.base, ""),
                Some(_) => Self::closed_page(404),
                None => Self::closed_page(if self.sessions.contains_key(*id) { 409 } else { 404 }),
            },
            ("POST", ["c", "pay", id]) => self.checkout_submit(cx, id, req),
            ("GET", ["c", "pay", id, "status"]) => self.status(id),
            ("GET", ["phone", id]) => self.phone_page(id),
            ("POST", ["phone", id]) => self.phone_submit(cx, id, req),
            ("GET", ["3ds", pi]) => self.challenge_page(pi),
            ("POST", ["3ds", pi]) => self.challenge_submit(cx, pi, req),
            ("GET", ["elements"]) => self.elements_info(req),
            ("POST", ["confirm"]) => self.confirm(cx, req),
            (_, ["v1", rest @ ..]) => self.api_call(cx, req, rest),
            _ => Response::not_found(),
        }
    }

    fn steps(&mut self, cx: &mut Cx<'_>) {
        let due = due_steps(&mut self.steps, cx.now);
        for step in due {
            match step {
                Step::Settle { pi, decline: None } => {
                    let Some(i) = self.intents.get_mut(&pi) else { continue };
                    i.status = "succeeded".into();
                    i.received = i.amount;
                    let (charge, intent, sid) = (Self::charge_json(i), Self::intent_json(i), i.session.clone());
                    cx.emit(event("charge.succeeded", charge, cx.now));
                    cx.emit(event("payment_intent.succeeded", intent, cx.now));
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.payment_status = "paid".into();
                        let s = s.clone();
                        let j = self.session_json(&s, cx.base);
                        cx.emit(event("checkout.session.async_payment_succeeded", j, cx.now));
                    }
                }
                Step::Settle { pi, decline: Some(code) } => {
                    let Some(i) = self.intents.get_mut(&pi) else { continue };
                    i.status = "requires_payment_method".into();
                    i.error = Some(("payment_method_provider_decline".into(), code.into(), "The SEPA debit was returned by the bank (test).".into()));
                    let (intent, sid) = (Self::intent_json(i), i.session.clone());
                    cx.emit(event("payment_intent.payment_failed", intent, cx.now));
                    if let Some(s) = self.sessions.get(&sid).cloned() {
                        let j = self.session_json(&s, cx.base);
                        cx.emit(event("checkout.session.async_payment_failed", j, cx.now));
                    }
                }
                Step::Approve { pi } => self.succeeded(cx, &pi),
                Step::Expire { pi, code } => {
                    let Some(i) = self.intents.get_mut(&pi) else { continue };
                    i.status = "requires_payment_method".into();
                    i.error = Some((code, String::new(), "The test payment expired before it was paid.".into()));
                    let (intent, sid) = (Self::intent_json(i), i.session.clone());
                    cx.emit(event("payment_intent.payment_failed", intent, cx.now));
                    // a voucher's session completed when it was issued: its payment failed later
                    if let Some(s) = self.sessions.get(&sid).cloned().filter(|s| s.status == "complete") {
                        let j = self.session_json(&s, cx.base);
                        cx.emit(event("checkout.session.async_payment_failed", j, cx.now));
                    }
                }
                Step::Dispute { pi } => {
                    let Some(i) = self.intents.get(&pi) else { continue };
                    let reason = if i.method == "sepa_debit" { "general" } else { "fraudulent" };
                    let dispute_id = util::id("du_");
                    let euros = local::convert(i.amount, &i.currency, "EUR").unwrap_or(0);
                    let dispute = json!({
                        "id": dispute_id, "object": "dispute", "amount": i.amount, "currency": i.currency,
                        "charge": i.charge, "payment_intent": i.id, "reason": reason, "status": "needs_response",
                        "created": cx.now, "evidence": {"customer_email_address": PAYER_EMAIL},
                        "balance_transactions": [balance_tx(&dispute_id, -euros)],
                    });
                    cx.emit(event("charge.dispute.created", dispute.clone(), cx.now));
                    if let Some(id) = dispute["id"].as_str() {
                        self.disputes.insert(id.to_string(), dispute.clone());
                        bound(&mut self.disputes);
                    }
                }
            }
        }
    }

    fn advance(&mut self, cx: &mut Cx<'_>, months: i64) -> usize {
        let subs: Vec<String> = self
            .subscriptions
            .values()
            .filter(|s| s.status == "active" || s.status == "trialing")
            .map(|s| s.id.clone())
            .collect();
        let mut charged = 0;
        for sub in subs {
            for _ in 0..months {
                if self.renew(cx, &sub) {
                    charged += 1;
                }
            }
        }
        charged
    }

    fn sign(&self, cfg: &Config, body: &[u8], now: i64) -> (&'static str, String) {
        ("Stripe-Signature", util::stripe_signature(&cfg.stripe.webhook_secret, now, body))
    }

    fn keys(&self, cfg: &Config, base: &str) -> Value {
        json!({"api": format!("{base}{}", Kind::Stripe.prefix()), "secret_key": cfg.stripe.secret_key,
               "webhook_secret": cfg.stripe.webhook_secret, "publishable_key": cfg.stripe.publishable_key,
               "fields_page": format!("{base}{FIELDS_PAGE}"), "api_version": API_VERSION})
    }

    fn summary(&self) -> Value {
        let sessions: Vec<Value> = self
            .sessions
            .values()
            .map(|s| json!({"id": s.id, "mode": s.mode, "ui_mode": s.ui_mode, "method": s.method,
                            "status": s.status, "payment_status": s.payment_status}))
            .collect();
        let intents: Vec<Value> = self
            .intents
            .values()
            .map(|i| json!({"id": i.id, "status": i.status, "amount": i.amount, "refunded": i.refunded}))
            .collect();
        let subs: Vec<Value> = self
            .subscriptions
            .values()
            .map(|s| json!({"id": s.id, "status": s.status, "periods": s.periods}))
            .collect();
        json!({"sessions": sessions, "payment_intents": intents, "subscriptions": subs,
               "refunds": self.refunds.len(), "disputes": self.disputes.len()})
    }
}
