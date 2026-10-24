//! The payment pills (CHECKOUT-PLAN §2.2): a pill is a payment METHOD, with its provider named
//! under it ("via GoCardless") - people pick how they pay, not whom. Where two providers offer
//! one method the pill keeps both, the cheaper route chosen (the server's `preferred`), and the
//! popover can switch.
//!
//! The token server decides which pills exist and in which order: it answers for the buyer's
//! declared country with only the methods that are switched on and routed there, ranked by that
//! country's order of use. The app shows them in that order and keeps only what it can show:
//!
//! - a method with no surface this app can show has no pill;
//! - Apple Pay needs the native sheet (never its page in a web view);
//! - a voucher is never a pill ("I have a voucher" is a line of its own);
//! - cash by post is one prepaid payment, never a recurring plan.
//!
//! The default pill is the first one (the country's most used method), cash by post aside: it
//! waits for a letter.

use crate::{
    offer::{Offer, OfferedMethod, OfferedProvider},
    registry::{Method, SurfaceKind},
};

/// Who pays what, and what the app can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PillContext<'a> {
    /// The payer's declared country (ISO alpha-2): what the offer was asked for.
    pub country: &'a str,
    /// The currency the offer was asked in (`PLN`).
    pub currency: &'a str,
    /// The months paid at once.
    pub months: u32,
    /// A mandate or a subscription (else one prepaid payment).
    pub recurring: bool,
    /// The surfaces this app can show on this platform.
    pub surfaces: &'a [SurfaceKind],
}

/// Whether the app shows `method` at all (its surfaces aside): the server has already chosen
/// for the country, the currency and the period.
#[must_use]
pub fn method_shown(method: Method, ctx: &PillContext) -> bool {
    match method {
        Method::ApplePay => ctx.surfaces.contains(&SurfaceKind::NativeSheet),
        Method::Voucher => false,
        Method::Cash => !ctx.recurring,
        _ => true,
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

/// The pills of `offer` for this payer, in the payer country's order (the methods' `rank`; a
/// method the server ranks not follows in the order it was offered).
#[must_use]
pub fn pills(offer: &Offer, ctx: &PillContext) -> Vec<Pill> {
    // Each pill with its best rank, in the order its method was first offered.
    let mut out: Vec<(Pill, u32)> = Vec::new();
    for (index, provider) in offer.providers.iter().enumerate() {
        for offered in &provider.methods {
            if !method_shown(offered.method, ctx) || showable(offered, ctx).is_empty() {
                continue;
            }
            let rank = offered.rank.unwrap_or(u32::MAX);
            match out.iter_mut().find(|(p, _)| p.method == offered.method) {
                Some((pill, best)) => {
                    pill.providers.push(index);
                    *best = (*best).min(rank);
                }
                None => out.push((
                    Pill {
                        method: offered.method,
                        providers: vec![index],
                        chosen: index,
                    },
                    rank,
                )),
            }
        }
    }
    // A stable sort: equal ranks keep the order they were offered in.
    out.sort_by_key(|(_, rank)| *rank);
    out.into_iter()
        .map(|(mut pill, _)| {
            pill.chosen = first_choice(offer, &pill);
            pill
        })
        .collect()
}

/// The provider a pill starts with: the cheaper route (`preferred`), else the server's default
/// provider, else the first.
fn first_choice(offer: &Offer, pill: &Pill) -> usize {
    let preferred = |i: usize| {
        offer
            .providers
            .get(i)
            .and_then(|p| p.method(pill.method))
            .is_some_and(|m| m.preferred)
    };
    let default = |i: usize| offer.providers.get(i).is_some_and(|p| p.default);
    pill.providers
        .iter()
        .copied()
        .find(|&i| preferred(i))
        .or_else(|| pill.providers.iter().copied().find(|&i| default(i)))
        .or_else(|| pill.providers.first().copied())
        .unwrap_or(pill.chosen)
}

/// The pill shown first: the first one (the payer country's most used method), cash by post
/// aside (0 for none).
#[must_use]
pub fn default_pill(pills: &[Pill]) -> usize {
    pills
        .iter()
        .position(|p| p.method != Method::Cash)
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
