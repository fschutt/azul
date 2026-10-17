//! What a method tells about the payer and who asks for it (`needs_identity` of the offer), and
//! the details the popover sends once with a checkout.

use serde_json::json;

use crate::identity::{AskedBy, Identity, Payer, PayerField};

#[test]
fn a_method_without_needs_identity_asks_for_nothing() {
    for value in [json!(null), json!(false), json!([]), json!({"fields": []})] {
        let identity = Identity::parse(&value).unwrap();
        assert!(identity.is_none(), "{value}");
        assert!(identity.app_fields().is_empty(), "{value}");
    }
}

#[test]
fn a_bare_list_of_payer_details_is_asked_by_the_providers_own_page() {
    let identity = Identity::parse(&json!(["name", "Email", "name"])).unwrap();
    assert_eq!(identity.fields, vec![PayerField::Name, PayerField::Email], "each once, in order");
    assert_eq!(identity.asked_by, AskedBy::Provider);
    assert!(identity.app_fields().is_empty(), "the provider's page asks: the app sends nothing");
    let page = Identity::parse(&json!({"fields": ["name", "address"], "asked_by": "provider"}))
        .unwrap();
    assert_eq!(page.fields, vec![PayerField::Name, PayerField::Address]);
}

#[test]
fn the_app_asks_only_for_a_name_an_email_and_a_national_id() {
    let app = Identity::parse(&json!({"fields": ["name", "email", "national_id"],
                                      "asked_by": "app"}))
    .unwrap();
    assert_eq!(app.asked_by, AskedBy::App);
    assert_eq!(
        app.app_fields(),
        &[PayerField::Name, PayerField::Email, PayerField::NationalId]
    );
    for field in ["address", "phone", "tax_id"] {
        let refused = Identity::parse(&json!({"fields": ["name", field], "asked_by": "app"}));
        assert!(refused.is_err(), "the app never asks for {field}");
    }
}

#[test]
fn an_unknown_detail_or_asker_keeps_the_method_from_being_shown() {
    assert!(Identity::parse(&json!(["shoe_size"])).is_err());
    assert!(Identity::parse(&json!({"fields": ["name"], "asked_by": "someone"})).is_err());
    assert!(Identity::parse(&json!("name")).is_err());
}

#[test]
fn a_payer_misses_every_asked_field_that_is_empty_or_no_email() {
    let identity = Identity::parse(&json!({"fields": ["name", "email", "national_id"],
                                           "asked_by": "app"}))
    .unwrap();
    let mut payer = Payer::default();
    assert_eq!(payer.missing(&identity), identity.fields);
    payer.set(PayerField::Name, "  Ana Souza ");
    payer.set(PayerField::Email, "ana-at-example");
    payer.set(PayerField::NationalId, "123.456.789-09");
    assert_eq!(payer.missing(&identity), vec![PayerField::Email]);
    for bad in ["@example.com", "ana@", "ana@example", "ana@example."] {
        payer.set(PayerField::Email, bad);
        assert_eq!(payer.missing(&identity), vec![PayerField::Email], "{bad}");
    }
    payer.set(PayerField::Email, "ana@example.com");
    assert!(payer.missing(&identity).is_empty());
    let provider = Identity::parse(&json!(["name", "email"])).unwrap();
    assert!(
        Payer::default().missing(&provider).is_empty(),
        "nothing is missing where the provider's page asks"
    );
}

#[test]
fn the_payer_sent_with_a_checkout_holds_only_the_asked_fields_trimmed() {
    let mut payer = Payer::default();
    payer.set(PayerField::Name, " Ana Souza ");
    payer.set(PayerField::Email, "ana@example.com");
    payer.set(PayerField::NationalId, "123.456.789-09");
    let name_only = Identity::parse(&json!({"fields": ["name"], "asked_by": "app"})).unwrap();
    let sent = payer.for_checkout(&name_only).expect("the app asks for the name");
    assert_eq!(sent.name, "Ana Souza");
    assert!(sent.email.is_empty() && sent.national_id.is_empty());
    assert_eq!(sent.to_json(), json!({"name": "Ana Souza"}));
    let provider = Identity::parse(&json!(["name"])).unwrap();
    assert_eq!(payer.for_checkout(&provider), None, "the provider's page asks: nothing is sent");
    assert_eq!(payer.for_checkout(&Identity::default()), None);
}

#[test]
fn a_payer_never_shows_what_was_typed() {
    let mut payer = Payer::default();
    payer.set(PayerField::Name, "Ana Souza");
    payer.set(PayerField::NationalId, "123.456.789-09");
    let shown = format!("{payer:?}");
    assert!(!shown.contains("Ana") && !shown.contains("123"), "{shown}");
    assert!(shown.contains("name") && shown.contains("national_id"), "{shown}");
    assert!(!payer.is_empty());
    assert!(Payer::default().is_empty());
}
