//! The payment methods registry: what people pay with in each country, which providers carry each
//! method, and how (the payer's flow, where an app may show it, recurring, currencies, limits,
//! fees, refunds), with the providers' documented test data.
//!
//! The data is `data/methods.toml`, compiled in as [`DATA`]. [`Registry::parse`] reads it and checks
//! the rules a hand edit could break, so a reader gets either a whole, consistent registry or the
//! list of what is wrong, never a part of one. Nothing here belongs to one app's prices or policy:
//! a token server decides what to offer from it, an app's payment pills read the same file, and a
//! payment sandbox fakes exactly the routes that carry documented test data.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

pub mod currency;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "currency_tests.rs"]
mod currency_tests;

/// The shipped registry.
pub const DATA: &str = include_str!("../data/methods.toml");

/// The format this parser reads; the table's `format` must match it.
pub const FORMAT: i64 = 2;

/// The whole registry, checked. The default is an empty registry, which offers nothing.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Registry {
    /// Must be [`FORMAT`]: a table in a newer format is refused rather than half understood.
    pub format: i64,
    /// When the facts were last read at their sources.
    pub read_on: String,
    /// The payment providers.
    #[serde(rename = "provider")]
    pub providers: Vec<Provider>,
    /// What payers choose (the pills).
    #[serde(rename = "method")]
    pub methods: Vec<Method>,
    /// One way to accept a method: a method through one provider.
    #[serde(rename = "route")]
    pub routes: Vec<Route>,
    /// What people pay with, per country, most used first; `*` for every other country.
    #[serde(rename = "country")]
    pub countries: Vec<Country>,
}

/// A payment provider.
#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    /// Stable id; adapters and test fakes name it (a fake serves the routes of the real id).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Whether the provider sells on the merchant's behalf or is the seller itself.
    pub kind: ProviderKind,
    /// How a business gets an account.
    pub onboarding: Onboarding,
    /// Where a business may sign up; `*` = anywhere.
    pub merchant_countries: Vec<String>,
    /// Hosts its pages may navigate to inside an app's webview. Empty means its pages can only
    /// open in the system browser, because an app refuses a page outside a provider's origins.
    pub origins: Vec<String>,
    /// How its webhooks prove where they come from.
    pub webhook: Webhook,
    /// How the money reaches the merchant, one line.
    pub payout: String,
    /// The provider's overview of its methods.
    pub docs: String,
    /// Where the facts come from.
    pub sources: Vec<String>,
}

/// What a payer chooses.
#[derive(Debug, Clone, Deserialize)]
pub struct Method {
    /// Stable id, the same across providers.
    pub id: String,
    /// Descriptive text only: no logo or wordmark comes from this file.
    pub name: String,
    /// The kind of method, for grouping.
    pub family: Family,
    /// What the payer does.
    pub flow: Flow,
    /// Where the payer finishes; a login must stay in the browser.
    pub handoff: Handoff,
    /// Payer countries; `*` = anywhere.
    pub countries: Vec<String>,
    /// Currencies it is paid in; `*` = most.
    pub currencies: Vec<String>,
}

/// One way to accept a method: the method through one provider.
#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    /// The method's id.
    pub method: String,
    /// The provider's id.
    pub provider: String,
    /// The provider's own id for the method (a Stripe `payment_method_types` value, a GoCardless
    /// scheme, a dLocal `payment_method_id`).
    pub provider_type: String,
    /// Whether anyone may use it, or only after a preview, an invitation or a contract.
    pub status: Status,
    /// Empty: the provider's merchant countries; else only these may use the route.
    pub merchant_countries: Vec<String>,
    /// The currencies the payer is charged in; `*` = most.
    pub currencies: Vec<String>,
    /// Payable on a one-off session priced in another currency, which the provider converts.
    pub adaptive: bool,
    /// The same for recurring charges.
    pub adaptive_recurring: bool,
    /// How later periods are charged, if at all.
    pub recurring: Recurring,
    /// The largest recurring charge, where the scheme caps it.
    pub recurring_cap: MaybeAmount,
    /// The smallest amount the route takes.
    pub min: MaybeAmount,
    /// The largest amount the route takes.
    pub max: MaybeAmount,
    /// How long a code, QR or voucher lives; 0 = not applicable. Apps count it down.
    pub expires_minutes: i64,
    /// When the outcome is known.
    pub settles: Settles,
    /// Where an app may show it, best first.
    pub surfaces: Vec<Surface>,
    /// The provider's or the scheme's rule on webviews.
    pub webview: Webview,
    /// False: no public list price (a contract), so [`Route::fee`] is unknown.
    pub fee_known: bool,
    /// The percentage part of the fee, in basis points.
    pub fee_bps: i64,
    /// The fixed part per payment.
    pub fee_fixed: MaybeAmount,
    /// The most one payment costs.
    pub fee_cap: MaybeAmount,
    /// Other fee facts, one line.
    pub fee_note: String,
    /// How long a payment can be refunded: 0 = never, -1 = unknown.
    pub refund_days: i64,
    /// How payers can claw a payment back.
    pub disputes: Disputes,
    /// What the provider's own page asks the payer: typed there, never seen by the merchant's server.
    pub payer_data: Vec<PayerData>,
    /// The provider's documented test data: the only inputs a test fake accepts.
    pub test_inputs: Vec<TestInput>,
    /// Where the facts come from.
    pub sources: Vec<String>,
    /// When the sources were read.
    pub read_on: String,
    /// Anything else, one line.
    pub note: String,
    /// What the provider's API wants in the merchant's own request: the app asks the payer for
    /// exactly these and the merchant's server passes them on in that one request, keeping none.
    pub request_data: Vec<PayerData>,
}

/// One documented test input and what it does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TestInput {
    /// The form field it goes into (`card_number`, `iban`, `code`, `phone`, `email`, `action`).
    pub field: String,
    /// The value, exactly as documented.
    pub value: String,
    /// What the payment does with it.
    pub outcome: Outcome,
    /// The provider's code or a short description.
    pub note: String,
    /// How long the provider takes to decide, in minutes (0 = at once): a test fake's
    /// timeline follows it, compressed.
    pub after_minutes: i64,
}

/// What people pay with in one country.
#[derive(Debug, Clone, Deserialize)]
pub struct Country {
    /// ISO 3166-1 alpha-2, or `*` for every country without its own entry.
    pub code: String,
    /// English name.
    pub name: String,
    /// The local currency; empty for `*`.
    pub currency: String,
    /// E-commerce value share in percent by kind (wallets, a2a, bnpl, crypto, credit, debit, cash);
    /// empty when unknown.
    pub mix: BTreeMap<String, i64>,
    /// The year the mix describes; 0 without one.
    pub mix_year: i64,
    /// The methods its payers are offered, most used first: an app shows these and only these,
    /// in this order.
    pub prefer: Vec<String>,
    /// Where the facts come from.
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Processor,
    Aggregator,
    MerchantOfRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Onboarding {
    SelfServe,
    Sales,
    Invite,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Webhook {
    StripeSignature,
    GocardlessSignature,
    MollieFetch,
    PaypalVerify,
    KomojuSecret,
    /// dLocal signs a notification as its API requests are signed: HMAC-SHA256 under the secret
    /// key over our login, the notification's date and its body.
    DlocalSignature,
    /// EBANX's notification names payment hashes only; each is read back through its query API.
    EbanxQuery,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Card,
    Wallet,
    BankRedirect,
    BankDebit,
    Realtime,
    Bnpl,
    Voucher,
    BankTransfer,
}

/// What the payer does; a test fake has one page per flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    /// Types card or account details into the provider's fields.
    Fields,
    /// Logs in at the bank or the wallet.
    Redirect,
    /// Scans a code on the screen with the phone.
    Qr,
    /// Types a short code from the bank app (BLIK).
    Code,
    /// Types a phone number and approves in the app.
    Push,
    /// Gets a reference and pays at a store, an ATM or the bank.
    Voucher,
    /// Gives an account and a mandate.
    Debit,
    /// Sends money to a given account and reference.
    Transfer,
}

impl Flow {
    /// The table's name for it, which apps and the token server's answers use too.
    pub fn name(self) -> &'static str {
        match self {
            Flow::Fields => "fields",
            Flow::Redirect => "redirect",
            Flow::Qr => "qr",
            Flow::Code => "code",
            Flow::Push => "push",
            Flow::Voucher => "voucher",
            Flow::Debit => "debit",
            Flow::Transfer => "transfer",
        }
    }
}

/// Where the payer finishes the payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Handoff {
    /// On the page itself.
    #[serde(rename = "none")]
    OnPage,
    PhoneQr,
    PhonePush,
    BankApp,
    BankLogin,
    ProviderLogin,
    StoreCash,
    BankTransfer,
}

impl Handoff {
    /// The table's name for it, which apps and the token server's answers use too.
    pub fn name(self) -> &'static str {
        match self {
            Handoff::OnPage => "none",
            Handoff::PhoneQr => "phone_qr",
            Handoff::PhonePush => "phone_push",
            Handoff::BankApp => "bank_app",
            Handoff::BankLogin => "bank_login",
            Handoff::ProviderLogin => "provider_login",
            Handoff::StoreCash => "store_cash",
            Handoff::BankTransfer => "bank_transfer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ga,
    Preview,
    Invite,
    Sales,
    Verify,
}

/// How later periods are charged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recurring {
    /// Not at all: each period is a new payment.
    #[serde(rename = "none")]
    OneOff,
    /// A mandate or stored credential charges later periods without the payer.
    Mandate,
    /// The first payment creates a SEPA Direct Debit mandate for the later ones.
    ViaSepaDebit,
    /// The payer pays an invoice each period: prepaid, not recurring.
    SendInvoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Settles {
    Instant,
    Minutes,
    Days,
}

/// Where an app shows a payment (the token server's surface names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// The platform's payment sheet (Apple Pay).
    Sheet,
    /// A native IBAN field that talks to the provider directly.
    Iban,
    /// The provider's hosted fields in the app's webview.
    Fields,
    /// The provider's hosted page in the app's webview.
    Page,
    /// The provider's page in the system browser.
    Browser,
}

impl Surface {
    pub const ALL: [Surface; 5] = [Surface::Sheet, Surface::Iban, Surface::Fields, Surface::Page, Surface::Browser];

    pub fn name(self) -> &'static str {
        match self {
            Surface::Sheet => "sheet",
            Surface::Iban => "iban",
            Surface::Fields => "fields",
            Surface::Page => "page",
            Surface::Browser => "browser",
        }
    }

    /// Whether this surface shows a provider's page or fields inside the app.
    pub fn in_app_webview(self) -> bool {
        matches!(self, Surface::Iban | Surface::Fields | Surface::Page)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Webview {
    Allowed,
    Discouraged,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disputes {
    /// The payer cannot claw it back through the scheme.
    #[serde(rename = "none")]
    Never,
    Chargeback,
    Claims,
    Bank,
    Unknown,
}

/// A piece of the payer's personal data a provider needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayerData {
    Name,
    Email,
    Phone,
    TaxId,
    NationalId,
    Address,
}

impl PayerData {
    /// Every kind, in the order an app lists them.
    pub const ALL: [PayerData; 6] = [
        PayerData::Name,
        PayerData::Email,
        PayerData::Phone,
        PayerData::TaxId,
        PayerData::NationalId,
        PayerData::Address,
    ];

    /// The table's name for it, which apps and the token server's answers use too.
    pub fn name(self) -> &'static str {
        match self {
            PayerData::Name => "name",
            PayerData::Email => "email",
            PayerData::Phone => "phone",
            PayerData::TaxId => "tax_id",
            PayerData::NationalId => "national_id",
            PayerData::Address => "address",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed,
    RequiresAction,
    Disputed,
}

/// An exact amount in a currency's minor unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amount {
    /// ISO 4217 code in capitals.
    pub currency: String,
    /// In the currency's minor unit: cents, or yen for JPY.
    pub minor: i64,
}

impl Amount {
    pub fn new(currency: &str, minor: i64) -> Amount {
        Amount {
            currency: currency.to_string(),
            minor,
        }
    }

    /// `"EUR 0.25"` -> 25 cents. Refuses signs, exponents, thousands separators, lower-case codes
    /// and more decimals than the currency has, so a typo never becomes another amount.
    pub fn parse(s: &str) -> Result<Amount, String> {
        let bad = || format!("the amount `{s}` is not written like `EUR 0.25`");
        let (code, number) = s.split_once(' ').ok_or_else(bad)?;
        if !currency_ok(code) {
            return Err(bad());
        }
        let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
        let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
        if whole.is_empty() || !digits(whole) || !digits(fraction) || (number.contains('.') && fraction.is_empty()) {
            return Err(bad());
        }
        let places = minor_digits(code);
        if fraction.len() > places as usize {
            return Err(format!("the amount `{s}` has more decimals than {code} has"));
        }
        let whole: i64 = whole.parse().map_err(|_| bad())?;
        let mut minor_part: i64 = if fraction.is_empty() { 0 } else { fraction.parse().map_err(|_| bad())? };
        for _ in fraction.len()..places as usize {
            minor_part = minor_part.checked_mul(10).ok_or_else(bad)?;
        }
        let minor = 10_i64
            .checked_pow(places)
            .and_then(|scale| whole.checked_mul(scale))
            .and_then(|m| m.checked_add(minor_part))
            .ok_or_else(bad)?;
        Ok(Amount::new(code, minor))
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let places = minor_digits(&self.currency) as usize;
        if places == 0 {
            return write!(f, "{} {}", self.currency, self.minor);
        }
        let scale = 10_i64.pow(places as u32);
        write!(f, "{} {}.{:0places$}", self.currency, self.minor / scale, (self.minor % scale).abs())
    }
}

/// An amount that may be absent (`""` in the table).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct MaybeAmount(pub Option<Amount>);

impl TryFrom<String> for MaybeAmount {
    type Error = String;

    fn try_from(s: String) -> Result<MaybeAmount, String> {
        if s.is_empty() {
            return Ok(MaybeAmount(None));
        }
        Amount::parse(&s).map(|a| MaybeAmount(Some(a)))
    }
}

/// How many decimals a currency's minor unit has (ISO 4217): none for the yen or the won, three
/// for the Gulf dinars, two for the rest.
pub fn minor_digits(currency: &str) -> u32 {
    const NONE: [&str; 17] = [
        "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "MGA", "PYG", "RWF", "UGX", "VND", "VUV", "XAF",
        "XOF", "XPF",
    ];
    const THREE: [&str; 7] = ["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"];
    if NONE.contains(&currency) {
        0
    } else if THREE.contains(&currency) {
        3
    } else {
        2
    }
}

fn currency_ok(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase())
}

fn country_ok(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase())
}

/// A registry that breaks its rules, with everything that is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub problems: Vec<String>,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the payment methods registry is invalid: {}", self.problems.join("; "))
    }
}

impl std::error::Error for Invalid {}

impl Registry {
    /// Reads a registry and checks every rule; all problems are reported together.
    pub fn parse(text: &str) -> Result<Registry, Invalid> {
        let registry: Registry = toml::from_str(text).map_err(|e| Invalid {
            problems: vec![format!("methods.toml does not parse: {e}")],
        })?;
        let problems = registry.problems();
        if problems.is_empty() {
            Ok(registry)
        } else {
            Err(Invalid { problems })
        }
    }

    /// The shipped registry.
    pub fn builtin() -> Result<Registry, Invalid> {
        Registry::parse(DATA)
    }

    pub fn provider(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn method(&self, id: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.id == id)
    }

    /// Every route of a method, in the table's order. The routes live as long as the registry,
    /// not as long as the method's name.
    pub fn routes_of<'r, 'm>(&'r self, method: &'m str) -> impl Iterator<Item = &'r Route> + use<'r, 'm> {
        self.routes.iter().filter(move |r| r.method == method)
    }

    /// The country's own entry, or the `*` entry for a country without one.
    pub fn country(&self, code: &str) -> Option<&Country> {
        self.countries
            .iter()
            .find(|c| c.code == code)
            .or_else(|| self.countries.iter().find(|c| c.code == "*"))
    }

    /// The methods a payer in `country` is offered: the country's own list (the `*` list for a
    /// country without one), in its order, each once, each serving the country.
    pub fn methods_for(&self, country: &str) -> Vec<&Method> {
        let mut seen = BTreeSet::new();
        let listed = self.country(country).map(|c| c.prefer.as_slice()).unwrap_or(&[]);
        listed
            .iter()
            .filter_map(|id| self.method(id))
            .filter(|m| m.serves(country) && seen.insert(m.id.as_str()))
            .collect()
    }

    /// Every rule of the format, as readable problems (none = valid).
    fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if self.format != FORMAT {
            p.push(format!("format {} is not the {FORMAT} this parser reads", self.format));
        }
        self.provider_problems(&mut p);
        self.method_problems(&mut p);
        self.route_problems(&mut p);
        self.country_problems(&mut p);
        p
    }

    fn provider_problems(&self, p: &mut Vec<String>) {
        let mut ids = BTreeSet::new();
        for x in &self.providers {
            if !ids.insert(x.id.as_str()) {
                p.push(format!("provider {} is listed twice", x.id));
            }
            codes(p, &format!("provider {}", x.id), &x.merchant_countries, true);
        }
    }

    fn method_problems(&self, p: &mut Vec<String>) {
        let mut ids = BTreeSet::new();
        for m in &self.methods {
            if !ids.insert(m.id.as_str()) {
                p.push(format!("method {} is listed twice", m.id));
            }
            codes(p, &format!("method {}", m.id), &m.countries, true);
            currencies(p, &format!("method {}", m.id), &m.currencies);
            if self.routes_of(&m.id).next().is_none() {
                p.push(format!("method {} has no route", m.id));
            }
        }
    }

    fn route_problems(&self, p: &mut Vec<String>) {
        let mut pairs = BTreeSet::new();
        for r in &self.routes {
            let at = format!("{} via {}", r.method, r.provider);
            if !pairs.insert((r.method.as_str(), r.provider.as_str())) {
                p.push(format!("{at} is listed twice"));
            }
            let method = self.method(&r.method);
            let provider = self.provider(&r.provider);
            if method.is_none() {
                p.push(format!("the route via {} names an unknown method {}", r.provider, r.method));
            }
            if provider.is_none() {
                p.push(format!("the route of {} names an unknown provider {}", r.method, r.provider));
            }
            codes(p, &at, &r.merchant_countries, false);
            currencies(p, &at, &r.currencies);
            if let Some(m) = method {
                let outside = r
                    .currencies
                    .iter()
                    .filter(|c| !m.currencies.iter().any(|mc| mc == "*" || mc == *c));
                for c in outside {
                    p.push(format!("{at} takes currency {c}, which the method is not paid in"));
                }
            }
            surfaces(p, &at, r, method, provider);
            fees(p, &at, r);
            if r.adaptive_recurring && !r.adaptive {
                p.push(format!("{at}: adaptive_recurring needs adaptive"));
            }
            if r.recurring_cap.0.is_some() && !r.recurs() {
                p.push(format!("{at}: a recurring cap on a route that does not recur"));
            }
            if r.expires_minutes < 0 || r.refund_days < -1 {
                p.push(format!("{at}: expires_minutes or refund_days below their range"));
            }
            if !r.test_inputs.is_empty() && !r.faked() {
                p.push(format!("{at} has test inputs but none that succeeded"));
            }
            if r.test_inputs.iter().any(|t| t.after_minutes < 0) {
                p.push(format!("{at}: a test input's after_minutes is below 0"));
            }
            for (key, list) in [("payer_data", &r.payer_data), ("request_data", &r.request_data)] {
                if PayerData::ALL.iter().any(|d| list.iter().filter(|x| *x == d).count() > 1) {
                    p.push(format!("{at}: {key} names a kind of data twice"));
                }
            }
        }
    }

    fn country_problems(&self, p: &mut Vec<String>) {
        const SHARES: [&str; 7] = ["wallets", "a2a", "bnpl", "crypto", "credit", "debit", "cash"];
        let mut seen = BTreeSet::new();
        if !self.countries.iter().any(|c| c.code == "*") {
            p.push("no `*` country for countries without their own entry".to_string());
        }
        for c in &self.countries {
            if !seen.insert(c.code.as_str()) {
                p.push(format!("country {} is listed twice", c.code));
            }
            if c.code != "*" && !country_ok(&c.code) {
                p.push(format!("country code `{}` is not two capital letters", c.code));
            }
            if !c.currency.is_empty() && !currency_ok(&c.currency) {
                p.push(format!("country {}: currency `{}` is not three capital letters", c.code, c.currency));
            }
            if let Some(k) = c.mix.keys().find(|k| !SHARES.contains(&k.as_str())) {
                p.push(format!("country {}: unknown mix share {k}", c.code));
            }
            let total: i64 = c.mix.values().sum();
            if !c.mix.is_empty() && !(95..=105).contains(&total) {
                p.push(format!("country {}: its mix adds up to {total} %, not about 100", c.code));
            }
            for id in &c.prefer {
                match self.method(id) {
                    None => p.push(format!("country {} lists an unknown method {id}", c.code)),
                    Some(m) if !m.serves(&c.code) => {
                        p.push(format!("country {} lists {id}, which does not serve it", c.code))
                    }
                    Some(_) => {}
                }
            }
        }
    }
}

/// Country codes in a list: two capital letters, or `*` where the list may mean anywhere.
fn codes(p: &mut Vec<String>, at: &str, list: &[String], star: bool) {
    for c in list {
        if !(country_ok(c) || (star && c == "*")) {
            p.push(format!("{at}: country code `{c}` is not two capital letters"));
        }
    }
}

fn currencies(p: &mut Vec<String>, at: &str, list: &[String]) {
    for c in list {
        if !(currency_ok(c) || c == "*") {
            p.push(format!("{at}: currency `{c}` is not three capital letters"));
        }
    }
}

/// The surface rules: a login stays in the browser, a provider that forbids or discourages
/// webviews is not framed, and a provider without known origins cannot pass an app's origin
/// check, so none of them may offer an in-app surface.
fn surfaces(p: &mut Vec<String>, at: &str, r: &Route, method: Option<&Method>, provider: Option<&Provider>) {
    let unique: BTreeSet<Surface> = r.surfaces.iter().copied().collect();
    if r.surfaces.is_empty() || unique.len() != r.surfaces.len() {
        p.push(format!("{at}: surfaces must name each surface once, at least one"));
    }
    let framed = r.surfaces.iter().any(|s| s.in_app_webview());
    if !framed {
        return;
    }
    if method.is_some_and(|m| matches!(m.handoff, Handoff::BankLogin | Handoff::ProviderLogin)) {
        p.push(format!("{at} offers a bank or provider login outside the browser"));
    }
    if r.webview != Webview::Allowed {
        p.push(format!("{at} shows a page inside the app although its webview rule is {:?}", r.webview));
    }
    if provider.is_some_and(|x| x.origins.is_empty()) {
        p.push(format!("{at} shows a page inside the app but its provider has no known origins"));
    }
}

fn fees(p: &mut Vec<String>, at: &str, r: &Route) {
    if !(0..=10_000).contains(&r.fee_bps) {
        p.push(format!("{at}: fee_bps {} is outside 0-10000", r.fee_bps));
    }
    if !r.fee_known && (r.fee_bps != 0 || r.fee_fixed.0.is_some() || r.fee_cap.0.is_some()) {
        p.push(format!("{at}: fee_known is false but it names a fee"));
    }
    if let (Some(fixed), Some(cap)) = (&r.fee_fixed.0, &r.fee_cap.0) {
        if fixed.currency != cap.currency {
            p.push(format!("{at}: the fee's fixed part and its cap are in different currencies"));
        }
    }
}

impl Method {
    /// Whether a payer in `country` can use the method.
    pub fn serves(&self, country: &str) -> bool {
        self.countries.iter().any(|c| c == "*" || c == country)
    }
}

impl Route {
    /// The provider's list price for one payment of `amount`, in its minor unit: the percentage
    /// (rounded half up), the fixed part, then the cap. Unknown (`None`) for a contract price or
    /// when the fixed part or the cap is in another currency than the amount.
    pub fn fee(&self, amount: &Amount) -> Option<i64> {
        if !self.fee_known || amount.minor < 0 {
            return None;
        }
        let parts = [&self.fee_fixed.0, &self.fee_cap.0];
        if parts.into_iter().flatten().any(|x| x.currency != amount.currency) {
            return None;
        }
        let percent = amount.minor.checked_mul(self.fee_bps)?.checked_add(5_000)? / 10_000;
        let fee = percent.checked_add(self.fee_fixed.0.as_ref().map_or(0, |f| f.minor))?;
        Some(match &self.fee_cap.0 {
            Some(cap) => fee.min(cap.minor),
            None => fee,
        })
    }

    /// Whether the payer can be charged in `currency` (a `*` route takes most currencies).
    pub fn takes_currency(&self, currency: &str) -> bool {
        self.currencies.iter().any(|c| c == "*" || c.eq_ignore_ascii_case(currency))
    }

    /// Whether a business in `merchant_country` may use this route of `provider`.
    pub fn open_to(&self, provider: &Provider, merchant_country: &str) -> bool {
        let listed = |list: &[String]| list.iter().any(|c| c == "*" || c == merchant_country);
        listed(&provider.merchant_countries) && (self.merchant_countries.is_empty() || listed(&self.merchant_countries))
    }

    /// Whether `amount` is inside the route's limits. A limit in another currency is the
    /// provider's to check at checkout, so it does not refuse here.
    pub fn amount_ok(&self, amount: &Amount) -> bool {
        let inside = |limit: &MaybeAmount, ok: fn(i64, i64) -> bool| match &limit.0 {
            Some(l) if l.currency == amount.currency => ok(amount.minor, l.minor),
            _ => true,
        };
        inside(&self.min, |a, l| a >= l) && inside(&self.max, |a, l| a <= l)
    }

    /// Whether later periods can be charged without the payer.
    pub fn recurs(&self) -> bool {
        matches!(self.recurring, Recurring::Mandate | Recurring::ViaSepaDebit)
    }

    /// Whether a test fake may stand in for this route: only with a documented success.
    pub fn faked(&self) -> bool {
        self.test_inputs.iter().any(|t| t.outcome == Outcome::Succeeded)
    }

    /// The personal data the payer gives the provider on this route, wherever it is typed (the
    /// provider's page or the app, for the request), once each in [`PayerData::ALL`]'s order:
    /// what an app tells the payer before they pick the method. Empty: the route is anonymous to
    /// the provider beyond the payment itself.
    pub fn needs_identity(&self) -> Vec<PayerData> {
        PayerData::ALL
            .into_iter()
            .filter(|d| self.payer_data.contains(d) || self.request_data.contains(d))
            .collect()
    }
}
