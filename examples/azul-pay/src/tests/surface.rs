//! The checkout's surface (`POST /v1/checkout`): every URL the server returns is checked
//! against the chosen provider before anything shows it (CHECKOUT-PLAN §3.3, §3.11).

use serde_json::{json, Value};

use super::fixtures::{choice, look};
use crate::{
    offer::ReturnKind,
    registry::{Method, SurfaceKind},
    surface::{Created, SurfaceError},
    url::WebUrl,
};

fn answer(method: &str, surface: Value) -> Value {
    json!({"checkout_id": "ck_test1", "pay_url": "https://pay.azlin.io/legacy/ck_test1",
           "provider": "stripe", "method": method, "surface": surface})
}

fn fields() -> Value {
    json!({"kind": "fields", "page": "https://pay.azlin.io/fields/stripe/v1",
           "publishable_key": "pk_test_SECRETPK", "client_secret": "pi_1_secret_SECRETCS"})
}

fn parse(method: Method, value: Value) -> Result<Created, SurfaceError> {
    Created::parse(&value, &choice(method), &look())
}

#[test]
fn a_fields_surface_opens_the_fields_page_with_its_inputs_in_the_fragment() {
    let created = parse(Method::Card, answer("card", fields())).unwrap();
    assert_eq!(created.checkout_id, "ck_test1");
    assert_eq!(created.surface.kind, SurfaceKind::PopoverFields);
    let url = created.surface.url.url();
    assert_eq!(url.origin_text(), "https://pay.azlin.io");
    assert_eq!(url.path(), "/fields/stripe/v1");
    assert_eq!(url.query(), "", "the inputs never go to a server");
    assert_eq!(
        url.fragment(),
        "pk=pk_test_SECRETPK&cs=pi_1_secret_SECRETCS&locale=en&look=flora-light"
    );
    assert_eq!(
        created.surface.url.reveal(),
        "https://pay.azlin.io/fields/stripe/v1#pk=pk_test_SECRETPK&cs=pi_1_secret_SECRETCS&locale=en&look=flora-light"
    );
    assert_eq!(created.surface.url.host(), "pay.azlin.io");
}

#[test]
fn a_page_or_browser_url_must_be_one_of_the_providers_own() {
    let good = parse(
        Method::Card,
        answer("card", json!({"kind": "page", "url": "https://checkout.stripe.com/c/pay/cs_test_1"})),
    )
    .unwrap();
    assert_eq!(good.surface.kind, SurfaceKind::WebviewPage);
    assert_eq!(good.surface.url.host(), "checkout.stripe.com");
    for (kind, url) in [
        ("page", "https://evil.example/c/pay/cs_test_1"),
        ("page", "http://checkout.stripe.com/c/pay/cs_test_1"),
        ("page", "https://checkout.stripe.com.evil.example/"),
        ("page", "https://checkout.stripe.com@evil.example/"),
        ("browser", "https://evil.example/pay"),
        ("browser", "javascript:alert(1)"),
        ("page", "https://pay.azlin.io/elsewhere"),
    ] {
        let refused = parse(Method::Card, answer("card", json!({"kind": kind, "url": url})));
        assert!(matches!(refused, Err(SurfaceError::Url(_))), "{kind} {url}: {refused:?}");
    }
    let paypal = parse(
        Method::PayPal,
        answer("paypal", json!({"kind": "browser", "url": "https://checkout.stripe.com/c/pay/cs_test_2"})),
    )
    .unwrap();
    assert_eq!(paypal.surface.kind, SurfaceKind::SystemBrowser);
}

#[test]
fn a_fields_page_elsewhere_or_without_its_inputs_is_refused() {
    for page in [
        "https://evil.example/fields/stripe/v1",
        "https://pay.azlin.io/fields/gocardless/v1",
        "https://pay.azlin.io/return/ok",
    ] {
        let mut surface = fields();
        surface["page"] = json!(page);
        assert!(
            matches!(parse(Method::Card, answer("card", surface)), Err(SurfaceError::Url(_))),
            "{page}"
        );
    }
    for (key, value) in [
        ("client_secret", json!("")),
        ("client_secret", json!("a b")),
        ("client_secret", json!("x".repeat(600))),
        ("publishable_key", json!(null)),
    ] {
        let mut surface = fields();
        surface[key] = value;
        assert!(
            matches!(
                parse(Method::Card, answer("card", surface)),
                Err(SurfaceError::Field(_))
            ),
            "{key}"
        );
    }
}

#[test]
fn a_surface_the_choice_cannot_show_is_refused() {
    let iban = parse(
        Method::Card,
        answer("card", json!({"kind": "iban", "publishable_key": "pk", "client_secret": "cs"})),
    );
    assert_eq!(iban, Err(SurfaceError::NotOffered(SurfaceKind::NativeIban)));
    let popover_paypal = parse(Method::PayPal, answer("paypal", fields()));
    assert_eq!(
        popover_paypal,
        Err(SurfaceError::NotOffered(SurfaceKind::PopoverFields)),
        "PayPal never in a web view, whatever the server sends"
    );
    let unknown = parse(Method::Card, answer("card", json!({"kind": "popup", "url": "x"})));
    assert_eq!(unknown, Err(SurfaceError::Kind("popup".to_string())));
}

#[test]
fn the_answer_must_be_for_the_chosen_provider_and_method() {
    let mut other = answer("card", fields());
    other["provider"] = json!("gocardless");
    assert!(matches!(parse(Method::Card, other), Err(SurfaceError::Mismatch(_))));
    let wrong_method = answer("paypal", fields());
    assert!(matches!(parse(Method::Card, wrong_method), Err(SurfaceError::Mismatch(_))));
    let mut no_id = answer("card", fields());
    no_id["checkout_id"] = json!("");
    assert_eq!(parse(Method::Card, no_id), Err(SurfaceError::Missing("checkout_id")));
    let mut no_surface = answer("card", fields());
    no_surface.as_object_mut().unwrap().remove("surface");
    assert_eq!(parse(Method::Card, no_surface), Err(SurfaceError::Missing("surface")));
    // Without provider and method the answer is taken as the choice's (an older server).
    let mut bare = answer("card", fields());
    bare.as_object_mut().unwrap().remove("provider");
    bare.as_object_mut().unwrap().remove("method");
    assert!(parse(Method::Card, bare).is_ok());
}

#[test]
fn the_answers_return_pages_count_only_on_the_pay_host() {
    let mut value = answer("card", fields());
    value["return"] = json!({"success": "https://pay.azlin.io/return/paid",
                             "cancel": "https://evil.example/return/cancel"});
    let created = parse(Method::Card, value).unwrap();
    let at = |text: &str| {
        created
            .returns
            .kind_of(&WebUrl::parse(text).unwrap(), &choice(Method::Card).provider)
    };
    assert_eq!(at("https://pay.azlin.io/return/paid"), Some(ReturnKind::Success));
    assert_eq!(at("https://pay.azlin.io/return/ok"), None);
    assert_eq!(at("https://pay.azlin.io/return/cancel"), Some(ReturnKind::Cancel));
    assert_eq!(at("https://evil.example/return/cancel"), None);
}

#[test]
fn the_debug_text_of_a_surface_holds_no_secret() {
    let created = parse(Method::Card, answer("card", fields())).unwrap();
    let text = format!("{created:?}");
    assert!(text.contains("pay.azlin.io"), "{text}");
    for secret in ["SECRETPK", "SECRETCS", "flora-light"] {
        assert!(!text.contains(secret), "{text}");
    }
    let page = parse(
        Method::Card,
        answer("card", json!({"kind": "page", "url": "https://checkout.stripe.com/c/pay/cs_test_SECRETSESSION#SECRETFRAG"})),
    )
    .unwrap();
    let text = format!("{page:?}");
    assert!(!text.contains("SECRET"), "{text}");
}
