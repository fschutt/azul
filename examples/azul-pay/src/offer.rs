//! The token server's offer: its provider descriptors (`GET /v1/checkout/options`), checked
//! against the registry (CHECKOUT-PLAN §3.3, §3.11).
//!
//! The server chooses, the app constrains. A descriptor names a provider of the registry and
//! says, for this country, currency, tier and period:
//!
//! ```json
//! {"offers": [
//!   {"provider": "stripe", "kind": "processor", "default": true,
//!    "origins": ["checkout.stripe.com"],
//!    "fields_page": "https://pay.azlin.io/fields/stripe/v1",
//!    "return": {"success": "https://pay.azlin.io/return/ok",
//!               "cancel": "https://pay.azlin.io/return/cancel",
//!               "pending": "https://pay.azlin.io/return/pending"},
//!    "methods": [{"method": "card", "surfaces": ["fields", "page", "browser"],
//!                 "embed": "webview", "settles": "instant", "recurring": false}]}],
//!  "price": {"amount_cents": 4990, "currency": "EUR", "vat_rate_permille": 190,
//!            "vat_cents": 797, "vat_included": true},
//!  "legal": {"withdrawal_consent": "...", "order_button": "...", "terms_url": "..."}}
//! ```
//!
//! Everything but `provider` and `methods[].method` is optional. What the app keeps of it:
//!
//! - an unknown provider, a provider whose `kind` contradicts the registry, and a fake provider
//!   from a token server that is not on this computer are dropped;
//! - an unknown method, or one the registry does not have for that provider, is dropped;
//! - `surfaces` are kept only where the registry's chain has them, in the chain's order;
//!   `embed: "browser"` keeps the system browser only, `"sheet"` the native sheet only; a
//!   method left without a surface is dropped, a provider left without a method too;
//! - `origins` (patterns: `host`, `.suffix`, `loopback`, a loopback host) are kept only where a
//!   registry origin covers them - they narrow the registry's, never widen them;
//! - `fields_page` must be on the provider's pages origin (`https://pay.azlin.io`) under
//!   `/fields/<provider id>/`; without one there is no popover;
//! - `return` URLs must be on the pages origin, else the default paths stand;
//! - `name` is never taken: the chrome shows the registry's names.
//!
//! What was dropped is said in [`Offer::dropped`] (for the app's log; never a secret).

use std::fmt;

use serde_json::Value;

pub use crate::registry::Settles;
use crate::{
    registry::{
        self, Method, MethodSpec, ProviderSpec, SurfaceKind, FIELDS_PREFIX, RETURN_CANCEL,
        RETURN_PENDING, RETURN_SUCCESS,
    },
    url::{is_loopback_host, Origin, WebUrl},
};

/// Who answers the offer: whether fake providers may come from it (a token server on this
/// computer, and a build with the feature `fake-providers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfferContext {
    pub fakes: bool,
}

impl OfferContext {
    /// The context of the token server at `token_url`.
    #[must_use]
    pub fn for_token_url(token_url: &str) -> OfferContext {
        let fakes = WebUrl::parse(token_url.trim()).is_ok_and(|u| is_loopback_host(u.host()));
        OfferContext { fakes }
    }
}

/// Why an answer is no offer at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfferError {
    NotJson,
    /// No `offers` list.
    NoOffers,
}

impl fmt::Display for OfferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OfferError::NotJson => f.write_str("the payment options are not JSON"),
            OfferError::NoOffers => f.write_str("the payment options name no offers"),
        }
    }
}

impl std::error::Error for OfferError {}

/// Which return page a URL is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnKind {
    Success,
    Cancel,
    Pending,
}

/// The paths of a provider's return pages, on its pages origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Returns {
    pub success: String,
    pub cancel: String,
    pub pending: String,
}

impl Default for Returns {
    fn default() -> Returns {
        Returns {
            success: RETURN_SUCCESS.to_string(),
            cancel: RETURN_CANCEL.to_string(),
            pending: RETURN_PENDING.to_string(),
        }
    }
}

/// The price the offer quotes (the gross price with its VAT).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Price {
    pub amount_cents: u64,
    pub currency: String,
    /// 190 = 19 %.
    pub vat_rate_permille: u32,
    pub vat_cents: u64,
    pub vat_included: bool,
}

/// `4990` cents as `49.90`.
#[must_use]
pub fn amount_text(cents: u64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

impl Price {
    /// `EUR 49.90, incl. 19 % VAT (EUR 7.97)`.
    #[must_use]
    pub fn text(&self) -> String {
        let rate = if self.vat_rate_permille % 10 == 0 {
            format!("{}", self.vat_rate_permille / 10)
        } else {
            format!("{}.{}", self.vat_rate_permille / 10, self.vat_rate_permille % 10)
        };
        let how = if self.vat_included { "incl." } else { "plus" };
        format!(
            "{cur} {}, {how} {rate} % VAT ({cur} {})",
            amount_text(self.amount_cents),
            amount_text(self.vat_cents),
            cur = self.currency
        )
    }
}

/// The legal texts the offer carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Legal {
    /// The consent the order needs (a ticked box).
    pub withdrawal_consent: Option<String>,
    /// The order button's words.
    pub order_button: Option<String>,
    pub terms_url: Option<String>,
}

/// One method a provider offers, as the app may show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedMethod {
    pub method: Method,
    /// The surfaces to try, best first (the registry's order, narrowed by the server).
    pub surfaces: Vec<SurfaceKind>,
    pub settles: Settles,
    /// A mandate or subscription (else one prepaid payment).
    pub recurring: bool,
    /// Its registry entry.
    pub spec: &'static MethodSpec,
}

/// One provider of the offer, as the app may show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedProvider {
    /// Its registry entry: the names, the kind, the pages origin.
    pub spec: &'static ProviderSpec,
    /// The server's default provider (where two offer one method).
    pub default: bool,
    /// Where its own pages may be: the server's patterns the registry covers, else the
    /// registry's.
    pub origins: Vec<Origin>,
    /// Its hosted-fields page (`None`: no popover).
    pub fields_page: Option<WebUrl>,
    pub returns: Returns,
    pub methods: Vec<OfferedMethod>,
}

impl OfferedProvider {
    /// Its offer of `method`.
    #[must_use]
    pub fn method(&self, method: Method) -> Option<&OfferedMethod> {
        self.methods.iter().find(|m| m.method == method)
    }

    /// Whether `url` is one of the provider's own pages (its narrowed origins).
    #[must_use]
    pub fn allows(&self, url: &WebUrl) -> bool {
        self.origins.iter().any(|o| o.matches(url))
    }

    /// Which of its return pages `url` is, if it is one (query and fragment do not matter).
    #[must_use]
    pub fn return_kind(&self, url: &WebUrl) -> Option<ReturnKind> {
        self.returns.kind_of(url, self)
    }
}

impl Returns {
    /// Which of these return pages of `provider` `url` is, if it is one: on the provider's pages
    /// origin, the path equal (query and fragment do not matter).
    #[must_use]
    pub fn kind_of(&self, url: &WebUrl, provider: &OfferedProvider) -> Option<ReturnKind> {
        if !provider.spec.on_pages(url) {
            return None;
        }
        let path = url.path();
        if path == self.success {
            Some(ReturnKind::Success)
        } else if path == self.cancel {
            Some(ReturnKind::Cancel)
        } else if path == self.pending {
            Some(ReturnKind::Pending)
        } else {
            None
        }
    }
}

/// The offer: the providers and methods the app may show, the price, the legal texts, and what
/// it refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// In the server's order.
    pub providers: Vec<OfferedProvider>,
    pub price: Option<Price>,
    pub legal: Legal,
    /// What was dropped, one sentence each (for the log).
    pub dropped: Vec<String>,
}

impl Offer {
    /// Reads the answer of `GET /v1/checkout/options` and keeps what the registry allows (see
    /// the module docs).
    ///
    /// # Errors
    ///
    /// [`OfferError`] for an answer that is not JSON or has no `offers` list. Offers the app
    /// cannot show at all are an offer without providers, not an error.
    pub fn parse(text: &str, ctx: &OfferContext) -> Result<Offer, OfferError> {
        let value: Value = serde_json::from_str(text).map_err(|_| OfferError::NotJson)?;
        let list = value["offers"].as_array().ok_or(OfferError::NoOffers)?;
        let mut dropped = Vec::new();
        let providers = list
            .iter()
            .filter_map(|d| provider(d, ctx, &mut dropped))
            .collect();
        Ok(Offer {
            providers,
            price: price(&value["price"]),
            legal: legal(&value["legal"]),
            dropped,
        })
    }

    /// The offered provider `id`.
    #[must_use]
    pub fn provider(&self, id: &str) -> Option<&OfferedProvider> {
        self.providers.iter().find(|p| p.spec.id == id)
    }
}

/// A text of at most 64 printable ASCII characters, for a sentence of [`Offer::dropped`].
fn said(text: &str) -> String {
    let mut out: String = text
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(64)
        .collect();
    if out.is_empty() {
        out.push_str("(nothing)");
    }
    out
}

fn provider(d: &Value, ctx: &OfferContext, dropped: &mut Vec<String>) -> Option<OfferedProvider> {
    let id = d["provider"].as_str().unwrap_or_default();
    let Some(spec) = registry::provider(id) else {
        dropped.push(format!("the provider {} is not known to this app", said(id)));
        return None;
    };
    if spec.fake && !ctx.fakes {
        dropped.push(format!(
            "the fake provider {id} is only taken from a token server on this computer"
        ));
        return None;
    }
    if let Some(kind) = d["kind"].as_str() {
        if kind != spec.kind.as_str() {
            dropped.push(format!(
                "the provider {id} is a {}, not a {}",
                spec.kind.as_str(),
                said(kind)
            ));
            return None;
        }
    }
    let origins = match d["origins"].as_array() {
        None => spec.origins.to_vec(),
        Some(patterns) => {
            let mut kept = Vec::new();
            for pattern in patterns {
                let text = pattern.as_str().unwrap_or_default();
                match Origin::parse(text) {
                    Some(origin) if spec.origins.iter().any(|r| r.covers(&origin)) => {
                        if !kept.contains(&origin) {
                            kept.push(origin);
                        }
                    }
                    _ => dropped.push(format!(
                        "the origin {} is not one of {id}'s",
                        said(text)
                    )),
                }
            }
            kept
        }
    };
    let fields_page = fields_page(spec, d.get("fields_page"), dropped);
    let returns = returns_over(Returns::default(), spec, &d["return"], dropped);
    let mut methods = Vec::new();
    for m in d["methods"].as_array().map(Vec::as_slice).unwrap_or_default() {
        if let Some(offered) = method(spec, m, fields_page.is_some(), dropped) {
            if !methods.iter().any(|o: &OfferedMethod| o.method == offered.method) {
                methods.push(offered);
            }
        }
    }
    if methods.is_empty() {
        dropped.push(format!("{id} offers no method this app may show"));
        return None;
    }
    Some(OfferedProvider {
        spec,
        default: d["default"].as_bool().unwrap_or(false),
        origins,
        fields_page,
        returns,
        methods,
    })
}

/// The provider's fields page: the server's, if it is on the pages origin under
/// `/fields/<id>/`; else the registry's on a fixed host (a fake's host has no fixed port: only
/// the server can name it).
fn fields_page(
    spec: &'static ProviderSpec,
    given: Option<&Value>,
    dropped: &mut Vec<String>,
) -> Option<WebUrl> {
    let path = spec.fields?;
    let prefix = format!("{FIELDS_PREFIX}{}/", spec.id);
    match given.and_then(Value::as_str) {
        Some(text) => match WebUrl::parse(text) {
            Ok(url) if spec.on_pages(&url) && url.path().starts_with(&prefix) => {
                Some(url.clear_fragment())
            }
            _ => {
                dropped.push(format!(
                    "the fields page {} is not {}'s on its pages",
                    crate::url::shown_host(text),
                    spec.id
                ));
                None
            }
        },
        None => match &spec.pages {
            Origin::Exact(host) => WebUrl::parse(&format!("https://{host}{path}")).ok(),
            _ => None,
        },
    }
}

/// The return pages `base` with the server's paths where its URLs (`given`: `{"success",
/// "cancel", "pending"}`) are on the provider's pages origin.
pub(crate) fn returns_over(
    base: Returns,
    spec: &'static ProviderSpec,
    given: &Value,
    dropped: &mut Vec<String>,
) -> Returns {
    let mut out = base;
    for (key, slot) in [
        ("success", &mut out.success),
        ("cancel", &mut out.cancel),
        ("pending", &mut out.pending),
    ] {
        let Some(text) = given[key].as_str() else {
            continue;
        };
        match WebUrl::parse(text) {
            Ok(url) if spec.on_pages(&url) => *slot = url.path().to_string(),
            _ => dropped.push(format!(
                "the {key} page {} is not on {}'s pages",
                crate::url::shown_host(text),
                spec.id
            )),
        }
    }
    out
}

fn method(
    spec: &'static ProviderSpec,
    m: &Value,
    has_fields: bool,
    dropped: &mut Vec<String>,
) -> Option<OfferedMethod> {
    let name = m["method"].as_str().unwrap_or_default();
    let Some(method) = Method::parse(name) else {
        dropped.push(format!("the method {} is not known to this app", said(name)));
        return None;
    };
    let Some(method_spec) = spec.method(method) else {
        dropped.push(format!(
            "{} has no {} in this app",
            spec.id,
            method.as_str()
        ));
        return None;
    };
    let wanted: Option<Vec<SurfaceKind>> = m["surfaces"].as_array().map(|list| {
        list.iter()
            .filter_map(|s| {
                let text = s.as_str().unwrap_or_default();
                let kind = SurfaceKind::parse(text);
                if !kind.is_some_and(|k| method_spec.chain.contains(&k)) {
                    dropped.push(format!(
                        "the surface {} of {} {}",
                        said(text),
                        spec.id,
                        method.as_str()
                    ));
                }
                kind
            })
            .collect()
    });
    let embed = m["embed"].as_str();
    let surfaces: Vec<SurfaceKind> = method_spec
        .chain
        .iter()
        .copied()
        .filter(|k| wanted.as_ref().map_or(true, |w| w.contains(k)))
        .filter(|k| match embed {
            Some("browser") => *k == SurfaceKind::SystemBrowser,
            Some("sheet") => *k == SurfaceKind::NativeSheet,
            _ => true,
        })
        .filter(|k| *k != SurfaceKind::PopoverFields || has_fields)
        .collect();
    if surfaces.is_empty() {
        dropped.push(format!(
            "nothing of {} {} is left to show",
            spec.id,
            method.as_str()
        ));
        return None;
    }
    let settles = match m["settles"].as_str() {
        // Cash comes by post whatever the server says: the app never waits for it.
        _ if method_spec.settles == Settles::Post => Settles::Post,
        Some("days") => Settles::Days,
        Some("instant") => Settles::Instant,
        Some("post") => Settles::Post,
        _ => method_spec.settles,
    };
    Some(OfferedMethod {
        method,
        surfaces,
        settles,
        recurring: m["recurring"].as_bool().unwrap_or(false),
        spec: method_spec,
    })
}

fn price(value: &Value) -> Option<Price> {
    let amount_cents = value["amount_cents"].as_u64()?;
    Some(Price {
        amount_cents,
        currency: said(value["currency"].as_str().unwrap_or("EUR")),
        vat_rate_permille: value["vat_rate_permille"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
        vat_cents: value["vat_cents"].as_u64().unwrap_or(0),
        vat_included: value["vat_included"].as_bool().unwrap_or(true),
    })
}

fn legal(value: &Value) -> Legal {
    let text = |v: &Value| {
        v.as_str()
            .or_else(|| v["text"].as_str())
            .map(str::to_string)
            .filter(|t| !t.trim().is_empty())
    };
    Legal {
        withdrawal_consent: text(&value["withdrawal_consent"]),
        order_button: text(&value["order_button"]),
        terms_url: text(&value["terms_url"]),
    }
}
