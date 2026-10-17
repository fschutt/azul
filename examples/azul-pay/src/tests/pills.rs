//! The payment pills (CHECKOUT-PLAN §2.2): which methods show for a country, a currency and a
//! period, with which provider, and which is the default.

use serde_json::{json, Value};

use crate::{
    offer::{Offer, OfferContext},
    pills::{default_pill, pills, Choice, PillContext, SEPA_COUNTRIES},
    registry::{Method, SurfaceKind},
};

/// What AzDrive can show today: the popover's fields, a hosted page, the browser.
const DESKTOP: &[SurfaceKind] = &[
    SurfaceKind::PopoverFields,
    SurfaceKind::WebviewPage,
    SurfaceKind::SystemBrowser,
];

fn offer(value: Value) -> Offer {
    Offer::parse(
        &value.to_string(),
        &OfferContext::for_token_url("https://token.azlin.io"),
    )
    .unwrap()
}

/// Stripe with every method it has, GoCardless with SEPA (the server's default).
fn full_offer() -> Offer {
    offer(json!({"offers": [
        {"provider": "gocardless", "default": true,
         "methods": [{"method": "sepa_debit", "surfaces": ["page", "browser"], "settles": "days"}]},
        {"provider": "stripe",
         "methods": [{"method": "card"}, {"method": "sepa_debit"}, {"method": "paypal"},
                     {"method": "apple_pay"}, {"method": "wero"}]}
    ]}))
}

fn ctx(country: &'static str, months: u32) -> PillContext<'static> {
    PillContext {
        country,
        currency: "EUR",
        months,
        recurring: false,
        surfaces: DESKTOP,
    }
}

fn methods(offer: &Offer, ctx: &PillContext) -> Vec<Method> {
    pills(offer, ctx).iter().map(|p| p.method).collect()
}

#[test]
fn sepa_shows_in_the_sepa_zone_in_euros_and_is_the_default() {
    let offer = full_offer();
    let de = ctx("DE", 1);
    let shown = pills(&offer, &de);
    assert_eq!(
        shown.iter().map(|p| p.method).collect::<Vec<_>>(),
        vec![Method::SepaDebit, Method::Card, Method::Wero],
        "in the server's order; PayPal waits for a prepaid year, Apple Pay for the native sheet"
    );
    assert_eq!(shown[default_pill(&shown)].method, Method::SepaDebit);
    assert!(SEPA_COUNTRIES.contains(&"DE") && SEPA_COUNTRIES.contains(&"CH"));
    assert!(!SEPA_COUNTRIES.contains(&"US"));
    let usd = PillContext {
        currency: "USD",
        ..ctx("DE", 1)
    };
    assert!(!methods(&offer, &usd).contains(&Method::SepaDebit), "SEPA is in euros");
}

#[test]
fn outside_the_sepa_zone_the_card_is_the_default() {
    let offer = full_offer();
    let us = ctx("US", 12);
    let shown = pills(&offer, &us);
    assert_eq!(
        shown.iter().map(|p| p.method).collect::<Vec<_>>(),
        vec![Method::Card, Method::PayPal]
    );
    assert_eq!(shown[default_pill(&shown)].method, Method::Card);
    assert_eq!(default_pill(&[]), 0);
}

#[test]
fn paypal_shows_for_a_prepaid_year_or_two_only() {
    let offer = full_offer();
    for months in [1, 3, 6] {
        assert!(!methods(&offer, &ctx("DE", months)).contains(&Method::PayPal), "{months}");
    }
    for months in [12, 24] {
        assert!(methods(&offer, &ctx("DE", months)).contains(&Method::PayPal), "{months}");
    }
    let recurring = PillContext {
        recurring: true,
        ..ctx("DE", 12)
    };
    assert!(!methods(&offer, &recurring).contains(&Method::PayPal));
}

#[test]
fn wero_shows_in_its_five_countries_and_never_for_a_recurring_plan() {
    let offer = full_offer();
    for country in ["DE", "BE", "FR", "LU", "NL", "de"] {
        assert!(methods(&offer, &ctx(country, 1)).contains(&Method::Wero), "{country}");
    }
    for country in ["AT", "IT", "US"] {
        assert!(!methods(&offer, &ctx(country, 1)).contains(&Method::Wero), "{country}");
    }
    let recurring = PillContext {
        recurring: true,
        ..ctx("DE", 1)
    };
    assert!(!methods(&offer, &recurring).contains(&Method::Wero));
}

#[test]
fn apple_pay_needs_the_native_sheet() {
    let offer = full_offer();
    assert!(!methods(&offer, &ctx("DE", 12)).contains(&Method::ApplePay));
    let with_sheet = [SurfaceKind::NativeSheet, SurfaceKind::SystemBrowser];
    let mac = PillContext {
        surfaces: &with_sheet,
        ..ctx("DE", 12)
    };
    assert!(methods(&offer, &mac).contains(&Method::ApplePay));
}

#[test]
fn two_providers_of_one_method_make_one_pill_with_a_switch() {
    let offer = full_offer();
    let shown = pills(&offer, &ctx("DE", 1));
    let sepa = shown.iter().find(|p| p.method == Method::SepaDebit).unwrap();
    let ids: Vec<&str> = sepa
        .providers
        .iter()
        .map(|&i| offer.providers[i].spec.id)
        .collect();
    assert_eq!(ids, vec!["gocardless", "stripe"]);
    assert_eq!(sepa.provider(&offer).spec.id, "gocardless", "the server's default provider");
    let card = shown.iter().find(|p| p.method == Method::Card).unwrap();
    assert_eq!(card.providers.len(), 1);
    assert_eq!(card.provider(&offer).spec.id, "stripe");
    assert_eq!(card.with_provider(0).chosen, card.chosen, "GoCardless offers no card");
    let switched = sepa.with_provider(sepa.providers[1]);
    assert_eq!(switched.provider(&offer).spec.id, "stripe");
}

#[test]
fn a_method_the_app_cannot_show_has_no_pill() {
    let offer = full_offer();
    let nothing = PillContext {
        surfaces: &[],
        ..ctx("DE", 12)
    };
    assert!(pills(&offer, &nothing).is_empty());
    let fields_only = [SurfaceKind::PopoverFields];
    let popover = PillContext {
        surfaces: &fields_only,
        ..ctx("DE", 1)
    };
    let shown = pills(&offer, &popover);
    let sepa = shown.iter().find(|p| p.method == Method::SepaDebit).unwrap();
    let ids: Vec<&str> = sepa
        .providers
        .iter()
        .map(|&i| offer.providers[i].spec.id)
        .collect();
    assert_eq!(ids, vec!["stripe"], "GoCardless offers a page and the browser only");
}

#[test]
fn a_choice_carries_only_the_surfaces_the_app_can_show() {
    let offer = full_offer();
    let page_and_browser = [SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser];
    let context = PillContext {
        surfaces: &page_and_browser,
        ..ctx("DE", 1)
    };
    let shown = pills(&offer, &context);
    let card = shown.iter().find(|p| p.method == Method::Card).unwrap();
    let choice = Choice::of(&offer, card, &context).unwrap();
    assert_eq!(choice.provider.spec.id, "stripe");
    assert_eq!(choice.method.method, Method::Card);
    assert_eq!(
        choice.method.surfaces,
        vec![SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser]
    );
}

#[test]
fn a_merchant_of_record_says_who_sells() {
    let shown = full_offer();
    let card = pills(&shown, &ctx("DE", 1))
        .into_iter()
        .find(|p| p.method == Method::Card)
        .unwrap();
    assert_eq!(card.provider(&shown).spec.kind.seller(), None, "Stripe processes, Azlin sells");
}

// ==== The server's choice: only its methods, in the payer country's order ====

fn ids(offer: &Offer, ctx: &PillContext) -> Vec<String> {
    pills(offer, ctx)
        .iter()
        .map(|p| format!("{}:{}", p.method.as_str(), p.provider(offer).spec.id))
        .collect()
}

#[test]
fn the_pills_come_in_the_payer_countrys_order_across_providers() {
    let de = super::fixtures::live_offer(&super::fixtures::options_de());
    assert_eq!(
        ids(&de, &ctx("DE", 1)),
        vec!["paypal:stripe", "sepa_debit:gocardless", "card:stripe", "wero:stripe"],
        "the ranks' order, not the providers'; the server already chose for the country and \
         the period"
    );
    let pl = super::fixtures::live_offer(&super::fixtures::options_pl());
    let poland = PillContext {
        currency: "PLN",
        ..ctx("PL", 1)
    };
    assert_eq!(
        ids(&pl, &poland),
        vec!["blik:stripe", "p24:stripe", "card:stripe"]
    );
}

#[test]
fn the_first_pill_is_the_default() {
    let pl = super::fixtures::live_offer(&super::fixtures::options_pl());
    let shown = pills(&pl, &ctx("PL", 1));
    assert_eq!(shown[default_pill(&shown)].method, Method::Blik);
    let de = super::fixtures::live_offer(&super::fixtures::options_de());
    let shown = pills(&de, &ctx("DE", 1));
    assert_eq!(shown[default_pill(&shown)].method, Method::PayPal);
    assert_eq!(default_pill(&[]), 0);
}

#[test]
fn only_the_methods_the_server_returned_are_pills_whatever_the_country() {
    // A payer in Switzerland: the server offers TWINT and the card - no direct debit although
    // Switzerland is in the SEPA zone, no Wero.
    let ch = offer(json!({"country": "CH", "currency": "CHF", "offers": [
        {"provider": "stripe", "methods": [
            {"method": "twint", "surfaces": ["page", "browser"], "rank": 0},
            {"method": "card", "rank": 1}]}]}));
    assert_eq!(
        methods(&ch, &PillContext { currency: "CHF", ..ctx("CH", 1) }),
        vec![Method::Twint, Method::Card]
    );
    // What the server returned for a payer shows, also where the app once had its own rules:
    // a direct debit in dollars, PayPal for a single month, Wero outside its launch countries.
    let us = offer(json!({"offers": [{"provider": "stripe", "methods": [
        {"method": "card", "rank": 0}, {"method": "paypal", "rank": 1},
        {"method": "wero", "rank": 2}]}]}));
    let shown = methods(&us, &PillContext { currency: "USD", ..ctx("US", 1) });
    assert_eq!(shown, vec![Method::Card, Method::PayPal, Method::Wero]);
}

#[test]
fn a_method_two_providers_carry_starts_with_the_preferred_one() {
    let de = super::fixtures::live_offer(&super::fixtures::options_de());
    let shown = pills(&de, &ctx("DE", 1));
    let sepa = shown.iter().find(|p| p.method == Method::SepaDebit).unwrap();
    assert_eq!(sepa.providers.len(), 2);
    assert_eq!(sepa.provider(&de).spec.id, "gocardless", "the cheaper route");
    // The same offer with Stripe's route marked cheaper.
    let mut value = super::fixtures::options_de();
    value["offers"][0]["methods"][0]["preferred"] = json!(false);
    value["offers"][1]["methods"][1]["preferred"] = json!(true);
    let switched = offer(value);
    let shown = pills(&switched, &ctx("DE", 1));
    let sepa = shown.iter().find(|p| p.method == Method::SepaDebit).unwrap();
    assert_eq!(sepa.provider(&switched).spec.id, "stripe");
}

#[test]
fn a_voucher_is_never_a_pill_and_cash_never_for_a_recurring_plan() {
    let both = offer(json!({"offers": [
        {"provider": "cash", "methods": [{"method": "cash", "surfaces": ["paper"],
                                          "settles": "post"}]},
        {"provider": "stripe", "methods": [{"method": "voucher"}, {"method": "card"}]}]}));
    let with_paper = [SurfaceKind::PopoverFields, SurfaceKind::WebviewPage,
                      SurfaceKind::SystemBrowser, SurfaceKind::Paper];
    let prepaid = PillContext { surfaces: &with_paper, ..ctx("DE", 12) };
    assert_eq!(methods(&both, &prepaid), vec![Method::Cash, Method::Card]);
    let monthly = PillContext { recurring: true, ..prepaid };
    assert_eq!(methods(&both, &monthly), vec![Method::Card]);
}
