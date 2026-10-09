//! The payment pills (CHECKOUT-PLAN §2.2): a pill is a payment METHOD, with its provider named
//! under it ("via GoCardless") - people pick how they pay, not whom. Where two providers offer
//! one method the pill keeps both, the server's default chosen, and the popover can switch.
//!
//! The server's offer decides which pills may exist; the app decides which it shows, by the
//! launch rules of the plan:
//!
//! | Method        | Shown when                                                        |
//! |---------------|-------------------------------------------------------------------|
//! | Direct debit  | the payer's country is in the SEPA zone and the currency is EUR   |
//! | Card          | always                                                            |
//! | Apple Pay     | the app can show the native sheet                                 |
//! | PayPal        | a prepaid period of 12 or 24 months (not recurring)               |
//! | Wero          | a payer in DE, BE, FR, LU or NL, in EUR, not recurring            |
//! | Bank transfer | a prepaid period of 12 or 24 months (not recurring)               |
//! | Voucher       | never a pill ("I have a voucher" is a line of its own)            |
//!
//! and only with a surface the app can show. The default pill is the direct debit where it
//! shows, else the card, else the first.

use crate::{
    offer::{Offer, OfferedMethod, OfferedProvider},
    registry::{Method, SurfaceKind},
};

/// The countries of the SEPA zone (ISO 3166 alpha-2): the EU, the EEA, Switzerland, the United
/// Kingdom, the microstates and the members admitted since (EPC's list).
pub const SEPA_COUNTRIES: &[&str] = &[
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GR", "HR", "HU", "IE",
    "IT", "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK", // the EU
    "IS", "LI", "NO", // the EEA
    "CH", "GB", "MC", "SM", "VA", "AD", // the others
    "AL", "MD", "ME", "MK", // admitted 2024 - 2025
];

/// Where Wero takes payments from (its launch countries).
pub const WERO_COUNTRIES: &[&str] = &["DE", "BE", "FR", "LU", "NL"];

/// Who pays what, and what the app can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PillContext<'a> {
    /// The payer's country (ISO alpha-2, any case): the dialog's "Country".
    pub country: &'a str,
    /// `EUR`.
    pub currency: &'a str,
    /// The months paid at once.
    pub months: u32,
    /// A mandate or a subscription (else one prepaid payment).
    pub recurring: bool,
    /// The surfaces this app can show on this platform.
    pub surfaces: &'a [SurfaceKind],
}

/// Whether the plan's launch rules show `method` to this payer (the surfaces aside).
#[must_use]
pub fn method_shown(method: Method, ctx: &PillContext) -> bool {
    let country = ctx.country.trim().to_ascii_uppercase();
    let euro = ctx.currency.trim().eq_ignore_ascii_case("EUR");
    let prepaid_year = ctx.months >= 12 && !ctx.recurring;
    match method {
        Method::SepaDebit => euro && SEPA_COUNTRIES.contains(&country.as_str()),
        Method::Card => true,
        Method::ApplePay => ctx.surfaces.contains(&SurfaceKind::NativeSheet),
        Method::PayPal | Method::BankTransfer => prepaid_year,
        Method::Wero => euro && !ctx.recurring && WERO_COUNTRIES.contains(&country.as_str()),
        Method::Voucher => false,
    }
}

/// One pill: a method, the providers that offer it (indices of [`Offer::providers`], in the
/// server's order) and the chosen one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pill {
    pub method: Method,
    pub providers: Vec<usize>,
    /// One of `providers`.
    pub chosen: usize,
}

impl Pill {
    /// The chosen provider.
    ///
    /// # Panics
    ///
    /// When `offer` is not the offer the pill was made from.
    #[must_use]
    pub fn provider<'o>(&self, offer: &'o Offer) -> &'o OfferedProvider {
        &offer.providers[self.chosen]
    }

    /// The pill with the provider `index` chosen (unchanged when it is not one of its
    /// providers).
    #[must_use]
    pub fn with_provider(&self, index: usize) -> Pill {
        let mut pill = self.clone();
        if pill.providers.contains(&index) {
            pill.chosen = index;
        }
        pill
    }
}

/// The surfaces of `offered` the app can show, in their order.
fn showable(offered: &OfferedMethod, ctx: &PillContext) -> Vec<SurfaceKind> {
    offered
        .surfaces
        .iter()
        .copied()
        .filter(|s| ctx.surfaces.contains(s))
        .collect()
}

/// The pills of `offer` for this payer, in the order the server offered their methods.
#[must_use]
pub fn pills(offer: &Offer, ctx: &PillContext) -> Vec<Pill> {
    let mut out: Vec<Pill> = Vec::new();
    for (index, provider) in offer.providers.iter().enumerate() {
        for offered in &provider.methods {
            if !method_shown(offered.method, ctx) || showable(offered, ctx).is_empty() {
                continue;
            }
            match out.iter_mut().find(|p| p.method == offered.method) {
                Some(pill) => {
                    pill.providers.push(index);
                    if provider.default && !offer.providers[pill.chosen].default {
                        pill.chosen = index;
                    }
                }
                None => out.push(Pill {
                    method: offered.method,
                    providers: vec![index],
                    chosen: index,
                }),
            }
        }
    }
    out
}

/// The pill shown first: the direct debit, else the card, else the first (0 for none).
#[must_use]
pub fn default_pill(pills: &[Pill]) -> usize {
    [Method::SepaDebit, Method::Card]
        .iter()
        .find_map(|m| pills.iter().position(|p| p.method == *m))
        .unwrap_or(0)
}

/// What the order button starts: the chosen provider and method, with only the surfaces the
/// app can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub provider: OfferedProvider,
    pub method: OfferedMethod,
}

impl Choice {
    /// The choice `pill` makes in `offer` for this payer; `None` when it offers nothing the app
    /// can show (or `pill` is not of `offer`).
    #[must_use]
    pub fn of(offer: &Offer, pill: &Pill, ctx: &PillContext) -> Option<Choice> {
        let provider = offer.providers.get(pill.chosen)?;
        let offered = provider.method(pill.method)?;
        let surfaces = showable(offered, ctx);
        if surfaces.is_empty() {
            return None;
        }
        Some(Choice {
            provider: provider.clone(),
            method: OfferedMethod {
                surfaces,
                ..offered.clone()
            },
        })
    }

    /// "via Stripe" (and "sold by ..." for a merchant of record).
    #[must_use]
    pub fn via(&self) -> String {
        via(&self.provider)
    }
}

/// The pill's provider line: `via Stripe`, `via Fake MoR - sold by Fake MoR Inc.`.
#[must_use]
pub fn via(provider: &OfferedProvider) -> String {
    match provider.spec.kind.seller() {
        Some(seller) => format!("via {} - sold by {seller}", provider.spec.name),
        None => format!("via {}", provider.spec.name),
    }
}
