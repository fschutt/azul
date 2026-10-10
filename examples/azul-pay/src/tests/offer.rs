//! The token server's offer (`GET /v1/checkout/options`): it picks, orders and narrows - and
//! never widens what the registry allows (CHECKOUT-PLAN §3.3).

use serde_json::{json, Value};

use crate::{
    offer::{Offer, OfferContext, OfferError, ReturnKind, Settles},
    registry::{Method, SurfaceKind},
    url::{Origin, WebUrl},
};

fn live() -> OfferContext {
    OfferContext::for_token_url("https://token.azlin.io")
}

fn parse(value: Value) -> Offer {
    Offer::parse(&value.to_string(), &live()).expect("an offer")
}

fn stripe_card(extra: Value) -> Value {
    let mut method = json!({"method": "card", "surfaces": ["fields", "page", "browser"],
                            "settles": "instant", "recurring": false});
    if let (Some(m), Some(e)) = (method.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            m.insert(k.clone(), v.clone());
        }
    }
    method
}

#[test]
fn an_offer_of_known_providers_keeps_their_methods_in_the_registry_chain_order() {
    let offer = parse(json!({
        "offers": [
            {"provider": "gocardless", "kind": "processor", "default": true,
             "methods": [{"method": "sepa_debit", "surfaces": ["browser", "page"],
                          "settles": "days", "recurring": true}]},
            {"provider": "stripe", "kind": "processor",
             "methods": [stripe_card(json!({"surfaces": ["browser", "page", "fields"]}))]}
        ]
    }));
    assert_eq!(offer.providers.len(), 2);
    let gc = &offer.providers[0];
    assert_eq!(gc.spec.id, "gocardless");
    assert!(gc.default);
    assert_eq!(gc.methods[0].method, Method::SepaDebit);
    assert_eq!(
        gc.methods[0].surfaces,
        vec![SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser],
        "the registry's order, best first"
    );
    assert_eq!(gc.methods[0].settles, Settles::Days);
    assert!(gc.methods[0].recurring);
    let stripe = &offer.providers[1];
    assert!(!stripe.default);
    assert_eq!(
        stripe.methods[0].surfaces,
        vec![
            SurfaceKind::PopoverFields,
            SurfaceKind::WebviewPage,
            SurfaceKind::SystemBrowser
        ]
    );
    assert!(offer.dropped.is_empty(), "{:?}", offer.dropped);
}

#[test]
fn an_unknown_provider_or_method_is_dropped_and_said_never_shown() {
    let offer = parse(json!({
        "offers": [
            {"provider": "evilpay", "methods": [{"method": "card"}]},
            {"provider": "stripe", "methods": [{"method": "bitcoin"}, stripe_card(json!({}))]},
            {"provider": "gocardless", "methods": [{"method": "card"}]}
        ]
    }));
    let ids: Vec<&str> = offer.providers.iter().map(|p| p.spec.id).collect();
    assert_eq!(ids, vec!["stripe"], "gocardless offers no method the registry has for it");
    assert_eq!(offer.providers[0].methods.len(), 1);
    let said = offer.dropped.join(" | ");
    for word in ["evilpay", "bitcoin", "gocardless"] {
        assert!(said.contains(word), "{said}");
    }
}

#[test]
fn a_surface_outside_the_registry_chain_is_ignored() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe", "methods": [
            {"method": "paypal", "surfaces": ["fields", "page", "browser"]},
            stripe_card(json!({"surfaces": ["popup", "page"]}))
        ]}]
    }));
    let stripe = &offer.providers[0];
    let paypal = stripe.method(Method::PayPal).unwrap();
    assert_eq!(paypal.surfaces, vec![SurfaceKind::SystemBrowser]);
    let card = stripe.method(Method::Card).unwrap();
    assert_eq!(card.surfaces, vec![SurfaceKind::WebviewPage]);
}

#[test]
fn the_server_cannot_widen_a_forbidden_method_into_a_web_view() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe", "methods": [
            {"method": "paypal", "surfaces": ["page"], "embed": "webview"},
            stripe_card(json!({}))
        ]}]
    }));
    let stripe = &offer.providers[0];
    assert!(stripe.method(Method::PayPal).is_none(), "nothing of PayPal is left to show");
    assert!(offer.dropped.join(" ").contains("paypal"));
}

#[test]
fn embed_browser_from_the_server_leaves_only_the_system_browser() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe", "methods": [stripe_card(json!({"embed": "browser"}))]}]
    }));
    assert_eq!(
        offer.providers[0].methods[0].surfaces,
        vec![SurfaceKind::SystemBrowser]
    );
    let sheet = parse(json!({
        "offers": [{"provider": "stripe", "methods": [
            {"method": "apple_pay", "surfaces": ["sheet", "browser"], "embed": "sheet"}]}]
    }));
    assert_eq!(
        sheet.providers[0].methods[0].surfaces,
        vec![SurfaceKind::NativeSheet]
    );
}

#[test]
fn a_server_origin_outside_the_registry_is_dropped_and_one_inside_narrows() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe",
                    "origins": ["checkout.stripe.com", "evil.example", "stripe.com.evil.example",
                                ".stripe.com", "not a pattern"],
                    "methods": [stripe_card(json!({}))]}]
    }));
    let stripe = &offer.providers[0];
    assert_eq!(
        stripe.origins,
        vec![Origin::Exact("checkout.stripe.com".into())]
    );
    let said = offer.dropped.join(" | ");
    for word in ["evil.example", ".stripe.com", "not a pattern"] {
        assert!(said.contains(word), "{said}");
    }
    let hooks = WebUrl::parse("https://hooks.stripe.com/3d_secure").unwrap();
    assert!(!stripe.allows(&hooks), "narrowed away");
    let none_given = parse(json!({
        "offers": [{"provider": "stripe", "methods": [stripe_card(json!({}))]}]
    }));
    assert!(none_given.providers[0].allows(&hooks), "the registry's origins by default");
}

#[test]
fn the_fields_page_must_be_the_providers_own_on_the_pay_host() {
    let with = |page: &str| {
        parse(json!({
            "offers": [{"provider": "stripe", "fields_page": page,
                        "methods": [stripe_card(json!({}))]}]
        }))
    };
    let good = with("https://pay.azlin.io/fields/stripe/v2");
    assert_eq!(
        good.providers[0].fields_page.as_ref().map(WebUrl::path),
        Some("/fields/stripe/v2")
    );
    for bad in [
        "https://evil.example/fields/stripe/v1",
        "https://pay.azlin.io/fields/gocardless/v1",
        "http://pay.azlin.io/fields/stripe/v1",
        "https://pay.azlin.io.evil.example/fields/stripe/v1",
        "javascript:alert(1)",
    ] {
        let offer = with(bad);
        let stripe = &offer.providers[0];
        assert!(stripe.fields_page.is_none(), "{bad}");
        assert_eq!(
            stripe.methods[0].surfaces,
            vec![SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser],
            "no popover without a fields page: {bad}"
        );
        assert!(!offer.dropped.is_empty(), "{bad}");
    }
    let default = parse(json!({
        "offers": [{"provider": "stripe", "methods": [stripe_card(json!({}))]}]
    }));
    assert_eq!(
        default.providers[0]
            .fields_page
            .as_ref()
            .map(WebUrl::to_text)
            .as_deref(),
        Some("https://pay.azlin.io/fields/stripe/v1"),
        "the registry's page when the server names none"
    );
}

#[test]
fn a_server_name_never_replaces_the_registry_name() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe", "name": "Totally Your Bank",
                    "methods": [stripe_card(json!({}))]}]
    }));
    assert_eq!(offer.providers[0].spec.name, "Stripe");
    assert_eq!(offer.providers[0].spec.legal_name, "Stripe Payments Europe, Limited");
}

#[test]
fn a_kind_that_contradicts_the_registry_drops_the_provider() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe", "kind": "merchant_of_record",
                    "methods": [stripe_card(json!({}))]}]
    }));
    assert!(offer.providers.is_empty());
    assert!(offer.dropped.join(" ").contains("stripe"));
}

#[test]
fn return_urls_off_the_pages_origin_fall_back_to_the_defaults() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe",
                    "return": {"success": "https://evil.example/return/ok",
                               "cancel": "https://pay.azlin.io/return/stop"},
                    "methods": [stripe_card(json!({}))]}]
    }));
    let stripe = &offer.providers[0];
    let at = |text: &str| stripe.return_kind(&WebUrl::parse(text).unwrap());
    assert_eq!(at("https://pay.azlin.io/return/ok"), Some(ReturnKind::Success));
    assert_eq!(at("https://pay.azlin.io/return/ok?session=1"), Some(ReturnKind::Success));
    assert_eq!(at("https://evil.example/return/ok"), None);
    assert_eq!(at("https://pay.azlin.io/return/stop"), Some(ReturnKind::Cancel));
    assert_eq!(at("https://pay.azlin.io/return/cancel"), None, "replaced by the server's");
    assert_eq!(at("https://pay.azlin.io/return/pending"), Some(ReturnKind::Pending));
    assert_eq!(at("https://checkout.stripe.com/return/ok"), None);
}

#[test]
fn the_price_and_the_legal_texts_are_read() {
    let offer = parse(json!({
        "offers": [{"provider": "stripe", "methods": [stripe_card(json!({}))]}],
        "price": {"amount_cents": 4990, "currency": "EUR", "vat_rate_permille": 190,
                  "vat_cents": 797, "vat_included": true},
        "legal": {"withdrawal_consent": {"id": "wc-1", "text": "I ask Azlin to start now."},
                  "order_button": "Buy now", "terms_url": "https://azlin.io/terms"}
    }));
    let price = offer.price.as_ref().unwrap();
    assert_eq!(price.amount_cents, 4990);
    assert_eq!(price.text(), "EUR 49.90, incl. 19 % VAT (EUR 7.97)");
    assert_eq!(offer.legal.withdrawal_consent.as_deref(), Some("I ask Azlin to start now."));
    assert_eq!(offer.legal.order_button.as_deref(), Some("Buy now"));
    let plain = parse(json!({
        "offers": [{"provider": "stripe", "methods": [stripe_card(json!({}))]}],
        "legal": {"withdrawal_consent": "Start now."}
    }));
    assert_eq!(plain.legal.withdrawal_consent.as_deref(), Some("Start now."));
    assert!(plain.price.is_none());
}

#[test]
fn what_is_no_offer_is_an_error() {
    assert_eq!(Offer::parse("not json", &live()), Err(OfferError::NotJson));
    assert_eq!(Offer::parse("{}", &live()), Err(OfferError::NoOffers));
    assert_eq!(Offer::parse("{\"offers\": 3}", &live()), Err(OfferError::NoOffers));
    // Offers the app cannot show at all are an empty offer, not an error: the pills say so.
    let empty = Offer::parse("{\"offers\": [{\"provider\": \"evilpay\"}]}", &live()).unwrap();
    assert!(empty.providers.is_empty());
}

#[test]
fn only_a_token_server_on_this_computer_may_offer_a_fake() {
    assert!(OfferContext::for_token_url("http://127.0.0.1:8081").fakes);
    assert!(OfferContext::for_token_url("http://localhost:8081/").fakes);
    assert!(!OfferContext::for_token_url("https://token.azlin.io").fakes);
    assert!(!OfferContext::for_token_url("http://10.0.0.2:8081").fakes);
    assert!(!OfferContext::for_token_url("nonsense").fakes);
    let text = json!({"offers": [{"provider": "fake-stripe",
                                  "methods": [{"method": "card"}]}]})
    .to_string();
    let offer = Offer::parse(&text, &live()).unwrap();
    assert!(offer.providers.is_empty(), "never from a token server elsewhere");
}

#[cfg(feature = "fake-providers")]
#[test]
fn a_fake_provider_from_a_local_token_server_keeps_its_loopback_pages() {
    let local = OfferContext::for_token_url("http://127.0.0.1:8081");
    let text = json!({"offers": [{
        "provider": "fake-stripe",
        "fields_page": "http://127.0.0.1:8081/fields/fake-stripe/v1",
        "return": {"success": "http://127.0.0.1:8081/return/ok",
                   "cancel": "http://127.0.0.1:8081/return/cancel",
                   "pending": "http://127.0.0.1:8081/return/pending"},
        "methods": [{"method": "card", "surfaces": ["fields", "page", "browser"]}]}]})
    .to_string();
    let offer = Offer::parse(&text, &local).unwrap();
    let fake = &offer.providers[0];
    assert_eq!(fake.spec.id, "fake-stripe");
    assert_eq!(fake.methods[0].surfaces.len(), 3);
    let ok = WebUrl::parse("http://127.0.0.1:8081/return/ok").unwrap();
    assert_eq!(fake.return_kind(&ok), Some(ReturnKind::Success));
    let elsewhere = WebUrl::parse("http://10.0.0.2:8081/return/ok").unwrap();
    assert_eq!(fake.return_kind(&elsewhere), None);
}
