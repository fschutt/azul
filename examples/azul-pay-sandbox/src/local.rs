//! The payer's side of the local payment methods, for any provider fake.
//!
//! The payment methods registry says which method types a provider's fake may take (the routes
//! that carry the provider's documented test data), what the payer does with each (the flow) and
//! which documented input means which outcome. This module turns that into one small page per
//! flow - a list of test banks, a code, a QR that is a loopback URL to the fake phone, a phone
//! number, a voucher - and into the outcome of a submitted form. Input that is not documented
//! test data is refused with the documented values as the hint; it is never echoed or kept.

use std::collections::BTreeMap;

use azul_pay_methods::currency::convert_minor;
use azul_pay_methods::{minor_digits, Flow, Handoff, Method, Registry, Route, TestInput};

use crate::kit;

/// A local method a fake serves: the registry's method and the provider's route of it.
#[derive(Clone, Copy)]
pub struct Local<'a> {
    pub method: &'a Method,
    pub route: &'a Route,
}

/// The faked route of `provider` whose own type is `provider_type`; none for a type without
/// documented test data (a fake must not invent what a provider would accept).
pub fn find<'a>(registry: &'a Registry, provider: &str, provider_type: &str) -> Option<Local<'a>> {
    registry
        .routes
        .iter()
        .filter(|r| r.provider == provider && r.provider_type == provider_type && r.faked())
        .find_map(|r| {
            Some(Local {
                method: registry.method(&r.method)?,
                route: r,
            })
        })
}

/// What a submitted form comes to.
pub enum Answer {
    /// A documented test input, and with it the outcome.
    Matched(TestInput),
    /// Not documented test data, or a required field left empty: the payer's message.
    Refused(String),
}

/// The order a form shows the fields of the test data in.
const FIELD_ORDER: [&str; 4] = ["code", "phone", "email", "action"];

impl<'a> Local<'a> {
    /// The fields its documented test data uses, in the order a form shows them: a code, a phone
    /// number, an email, an action, then any other (a debit scheme's account fields) as the data
    /// lists them.
    pub fn fields(&self) -> Vec<&'a str> {
        let mut out: Vec<&'a str> = Vec::new();
        let known = FIELD_ORDER.iter().filter_map(|f| self.route.test_inputs.iter().find(|t| t.field == *f));
        let others = self.route.test_inputs.iter().filter(|t| !FIELD_ORDER.contains(&t.field.as_str()));
        for t in known.chain(others) {
            if !out.contains(&t.field.as_str()) {
                out.push(t.field.as_str());
            }
        }
        out
    }

    fn documented(&self, field: &str) -> Vec<&'a str> {
        self.route
            .test_inputs
            .iter()
            .filter(|t| t.field == field)
            .map(|t| t.value.as_str())
            .collect()
    }

    /// The refusal: the documented values, never what was typed.
    fn only_documented(&self) -> String {
        let hints: Vec<String> = self
            .fields()
            .into_iter()
            .filter(|f| *f != "action")
            .map(|f| format!("{f} {}", self.documented(f).join(", ")))
            .collect();
        format!(
            "This sandbox only accepts the documented test data for {}: {}.",
            self.method.name,
            hints.join("; ")
        )
    }

    /// The documented input the form names. Each documented field the form fills must hold one of
    /// its documented values, and every field but an optional email must be filled. An email
    /// decides first (the providers key their failure cases to it), then a code, a phone number
    /// or an action.
    pub fn answer(&self, form: &BTreeMap<String, String>) -> Answer {
        let fields = self.fields();
        let given = |f: &str| form.get(f).map(|v| v.trim()).filter(|v| !v.is_empty());
        for f in fields.iter().copied() {
            match given(f) {
                Some(v) if !self.documented(f).contains(&v) => return Answer::Refused(self.only_documented()),
                None if f != "email" || fields.len() == 1 => return Answer::Refused(self.only_documented()),
                _ => {}
            }
        }
        let deciding = std::iter::once("email").chain(fields.iter().copied().filter(|f| *f != "email"));
        for f in deciding {
            let hit = given(f).and_then(|v| self.route.test_inputs.iter().find(|t| t.field == f && t.value == v));
            if let Some(t) = hit {
                return Answer::Matched(t.clone());
            }
        }
        Answer::Refused(self.only_documented())
    }

    /// Whether the payer decides on the fake phone with the documented actions (authorize, fail).
    pub fn decided_on_phone(&self) -> bool {
        matches!(self.method.flow, Flow::Qr | Flow::Push) && self.fields() == ["action"]
    }

    /// The right half of the checkout page: this method's inputs and buttons. `phone` is the fake
    /// phone's URL (the QR's payload), `status` the session's status URL the page polls, `done`
    /// where the payer goes when the session is paid.
    pub fn form(&self, pay: &str, phone: &str, status: &str, done: &str, message: &str) -> String {
        let name = kit::esc(&self.method.name);
        let error = format!("<div class=\"error\">{}</div>", kit::esc(message));
        let hint = |f: &str| kit::esc(&self.documented(f).first().map(|v| v.to_string()).unwrap_or_default());
        let email = if self.fields().contains(&"email") {
            let optional = if self.fields().len() > 1 { " (optional: a documented failure case)" } else { "" };
            format!("<label>Email{optional}<input name=\"email\" placeholder=\"{}\" autocomplete=\"off\"></label>", hint("email"))
        } else {
            String::new()
        };
        match self.method.flow {
            Flow::Redirect => {
                let at = if self.method.handoff == Handoff::BankLogin {
                    "<label>Your bank (test)<select name=\"bank\"><option value=\"a\">Test Bank A</option>\
                     <option value=\"b\">Test Bank B</option><option value=\"c\">Test Bank C</option></select></label>"
                        .to_string()
                } else {
                    format!("<p class=\"muted\">This stands in for the sign-in at {name}: no login, no password.</p>")
                };
                format!(
                    "<h2>{name} (test)</h2><form method=\"post\" autocomplete=\"off\">{at}{error}\
                     <button name=\"action\" value=\"authorize\">Authorize test payment</button>\
                     <button class=\"plain\" name=\"action\" value=\"fail\">Fail test payment</button></form>"
                )
            }
            Flow::Code => format!(
                "<h2>{name} (test)</h2><form method=\"post\" autocomplete=\"off\">\
                 <label>The 6-digit code from your bank app<input name=\"code\" inputmode=\"numeric\" maxlength=\"6\" \
                 placeholder=\"{}\" autocomplete=\"off\"></label>{email}{error}<button>{}</button></form>",
                hint("code"),
                kit::esc(pay)
            ),
            Flow::Push if self.fields().contains(&"phone") => format!(
                "<h2>{name} (test)</h2><form method=\"post\" autocomplete=\"off\">\
                 <label>Phone number (documented test numbers only)<input name=\"phone\" placeholder=\"{}\" \
                 autocomplete=\"off\"></label>{error}<button>Send to the app (test)</button></form>",
                hint("phone")
            ),
            Flow::Qr | Flow::Voucher if self.fields().contains(&"email") => format!(
                "<h2>{name} (test)</h2><form method=\"post\" autocomplete=\"off\">{email}{error}<button>{}</button></form>",
                if self.method.flow == Flow::Qr { "Show the test code" } else { "Get the test voucher" }
            ),
            _ => format!(
                "<h2>{name} (test)</h2>{}{}{error}",
                if self.method.flow == Flow::Qr { qr_box(phone) } else { phone_link(phone) },
                waiting(status, done)
            ),
        }
    }

    /// The fake phone: the method's app, standing in for the payer's approval.
    pub fn phone(&self, shape: &str, amount: &str, actions: &[(&str, &str)], note: &str) -> String {
        let mut buttons = String::new();
        for (i, (value, label)) in actions.iter().enumerate() {
            let class = if i == 0 { "" } else { " class=\"plain\"" };
            buttons.push_str(&format!(
                "<button{class} name=\"action\" value=\"{}\">{}</button>",
                kit::esc(value),
                kit::esc(label)
            ));
        }
        let form = if actions.is_empty() {
            String::new()
        } else {
            format!("<form method=\"post\">{buttons}</form>")
        };
        let body = format!(
            "<h2>Fake phone: the {} app (test)</h2><p>Pay {} to a test merchant?</p>{form}<p class=\"muted\">{}</p>",
            kit::esc(&self.method.name),
            kit::esc(amount),
            kit::esc(note)
        );
        kit::single("Approve on your phone", shape, &body, "")
    }
}

/// A debit scheme's account inputs (Bacs's sort code and account number, Autogiro's clearing
/// number, account number and personal number, ...), one per field of its documented test data.
pub fn account_inputs(l: &Local<'_>) -> String {
    let mut out = String::new();
    for f in l.fields() {
        let hint = l.documented(f).first().map(|v| v.to_string()).unwrap_or_default();
        out.push_str(&format!(
            "<label>{} (documented test account only)<input name=\"{}\" placeholder=\"{}\" autocomplete=\"off\"></label>",
            kit::esc(label(f)),
            kit::esc(f),
            kit::esc(&hint)
        ));
    }
    out
}

/// What a form calls a field of the test data.
fn label(field: &str) -> &str {
    match field {
        "sort_code_account" => "Sort code and account number",
        "clearing_account_personnummer" => "Clearing number, account number and personal identity number",
        "reg_account_cpr" => "Registration number, account number and CPR number",
        "bsb_account" => "BSB and account number",
        "bank_branch_account" => "Bank, branch and account number",
        "institution_transit_account" => "Institution, transit and account number",
        "routing_account" => "Routing number and account number",
        other => other,
    }
}

/// The QR stand-in: a box with the loopback URL of the fake phone. It is a test code that no
/// banking or wallet app can pay, and it says so.
pub fn qr_box(phone: &str) -> String {
    let url = kit::esc(phone);
    format!(
        "<div class=\"qr\"><b>TEST QR</b><p class=\"muted\">Not a payment code: it opens the sandbox's fake phone.</p>\
         <code>{url}</code><p><a href=\"{url}\">Open the fake phone on this computer</a></p></div>"
    )
}

fn phone_link(phone: &str) -> String {
    let url = kit::esc(phone);
    format!("<p>Approve the payment in the app.</p><p><a href=\"{url}\">Open the fake phone on this computer</a></p>")
}

/// The marker the polling script reads: the session's status URL and where to go when paid.
pub fn waiting(status: &str, done: &str) -> String {
    format!(
        "<div id=\"wait\" data-status=\"{}\" data-done=\"{}\" class=\"muted\">Waiting for the payment (test)...</div>",
        kit::esc(status),
        kit::esc(done)
    )
}

/// The page's few lines: ask the session's status every two seconds, go on when it is complete.
pub const POLL_SCRIPT: &str = "var w=document.getElementById('wait');if(w){setInterval(function(){\
fetch(w.dataset.status).then(function(r){return r.json()}).then(function(s){if(s.status=='complete')location.href=w.dataset.done})\
},2000)}";

/// A test voucher's page: its reference, and that the sandbox's timeline decides it.
pub fn voucher(shape: &str, name: &str, amount: &str, reference: &str, done: &str) -> String {
    let body = format!(
        "<h2>{} voucher (test)</h2><p>Amount: <b>{}</b></p><p>Reference: <b>[TEST] {}</b></p>\
         <p class=\"muted\">No store, ATM or bank can pay this: the sandbox's timeline pays it or lets it \
         expire, as the documented email said.</p><p><a href=\"{}\">Back to the app</a></p>",
        kit::esc(name),
        kit::esc(amount),
        kit::esc(reference),
        kit::esc(done)
    );
    kit::single("Your voucher", shape, &body, "")
}

/// A voucher reference made from the session id: digits only, the same each time it is shown.
pub fn reference(id: &str) -> String {
    let mut n: u64 = 1_469_598_103_934_665_603;
    for b in id.bytes() {
        n = (n ^ u64::from(b)).wrapping_mul(1_099_511_628_211);
    }
    let digits = format!("{:015}", n % 1_000_000_000_000_000);
    let (a, b) = digits.split_at(5);
    let (b, c) = b.split_at(5);
    format!("{a} {b} {c}")
}

/// The code a failed test input stands for: its note when that is a provider's code, else a
/// generic decline.
pub fn decline_code(t: &TestInput) -> String {
    let id_like = !t.note.is_empty() && t.note.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    if id_like {
        t.note.clone()
    } else {
        "payment_method_provider_decline".to_string()
    }
}

/// `PLN 257.95`, `JPY 9898`: an amount in its currency's minor unit, with its decimals.
pub fn money(minor: i64, currency: &str) -> String {
    let code = currency.to_ascii_uppercase();
    let places = minor_digits(&code);
    match 10_i64.checked_pow(places) {
        Some(scale) if places > 0 => {
            let width = places as usize;
            format!("{code} {}.{:0width$}", minor / scale, (minor % scale).abs())
        }
        _ => format!("{code} {minor}"),
    }
}

/// Fake conversion rates per euro, in millionths, for showing a converted price. They are marked
/// as a test rate wherever they appear; no real rate is looked up.
const RATES: &[(&str, i64)] = &[
    ("EUR", 1_000_000),
    ("PLN", 4_300_000),
    ("CHF", 940_000),
    ("SEK", 11_500_000),
    ("NOK", 11_700_000),
    ("DKK", 7_460_000),
    ("GBP", 860_000),
    ("USD", 1_100_000),
    ("CAD", 1_550_000),
    ("AUD", 1_700_000),
    ("NZD", 1_850_000),
    ("CZK", 25_000_000),
    ("HUF", 400_000_000),
    ("RON", 5_000_000),
    ("INR", 95_000_000),
    ("BRL", 6_200_000),
    ("MXN", 21_000_000),
    ("CNY", 8_300_000),
    ("HKD", 9_100_000),
    ("JPY", 165_000_000),
    ("KRW", 1_500_000_000),
    ("SGD", 1_450_000),
    ("MYR", 5_100_000),
    ("THB", 39_000_000),
    ("PHP", 63_000_000),
    ("IDR", 17_500_000_000),
    ("VND", 28_000_000_000),
];

fn fake_rate(currency: &str) -> Option<i64> {
    RATES.iter().find(|(x, _)| x.eq_ignore_ascii_case(currency)).map(|(_, r)| *r)
}

/// `minor` units of `from` in `to` at the fake rates, by the registry crate's one conversion;
/// none for a currency without a fake rate.
pub fn convert(minor: i64, from: &str, to: &str) -> Option<i64> {
    let (from, to) = (from.to_ascii_uppercase(), to.to_ascii_uppercase());
    convert_minor(minor, &from, fake_rate(&from)?, &to, fake_rate(&to)?)
}

/// Euros per one unit of `currency` at the fake rates, six decimals (`1.162791` for the pound):
/// the rate a payout's exchange names.
pub fn euros_per_unit(currency: &str) -> Option<String> {
    let rate = i128::from(fake_rate(currency)?);
    let micro = 1_000_000_000_000_i128.checked_div(rate)?;
    Some(format!("{}.{:06}", micro / 1_000_000, micro % 1_000_000))
}
