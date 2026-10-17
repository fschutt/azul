//! What a method tells about the payer and which part the app asks for (the offer's
//! `needs_identity` and `identity_in_app`), and the details the popover sends once with a
//! checkout.

use serde_json::json;

use crate::identity::{Identity, Payer, PayerField};

#[test]
fn a_method_without_needs_identity_asks_for_nothing() {
    for value in [
        json!({"method": "card"}),
        json!({"method": "card", "needs_identity": null, "identity_in_app": null}),
        json!({"method": "card", "needs_identity": [], "identity_in_app": []}),
    ] {
        let identity = Identity::parse(&value).unwrap();
        assert!(identity.is_none(), "{value}");
        assert!(identity.app_fields().is_empty(), "{value}");
    }
}

#[test]
fn what_the_provider_learns_is_asked_on_its_own_page_unless_the_app_must_ask() {
    let page = Identity::parse(&json!({"needs_identity": ["name", "Email", "name"]})).unwrap();
    assert_eq!(page.fields, vec![PayerField::Name, PayerField::Email], "each once, in order");
    assert!(page.app_fields().is_empty(), "the provider's page asks: the app sends nothing");
    assert_eq!(page.page_fields(), page.fields);
    // EBANX's shape: the name and the email in the merchant's request, the ID on its page.
    let split = Identity::parse(&json!({"needs_identity": ["name", "email", "tax_id"],
                                        "identity_in_app": ["name", "email"]}))
    .unwrap();
    assert_eq!(split.app_fields(), &[PayerField::Name, PayerField::Email]);
    assert_eq!(split.page_fields(), vec![PayerField::TaxId]);
    // A detail the app asks for is one the provider learns, listed or not.
    let only_app = Identity::parse(&json!({"identity_in_app": ["national_id"]})).unwrap();
    assert_eq!(only_app.fields, vec![PayerField::NationalId]);
}

#[test]
fn the_app_never_asks_for_an_address() {
    let app = Identity::parse(&json!({"needs_identity": ["name", "email", "national_id"],
                                      "identity_in_app": ["name", "email", "national_id"]}))
    .unwrap();
    assert_eq!(
        app.app_fields(),
        &[PayerField::Name, PayerField::Email, PayerField::NationalId]
    );
    let refused = Identity::parse(&json!({"identity_in_app": ["name", "address"]}));
    assert!(refused.is_err(), "the app never asks for a postal address");
    let page = Identity::parse(&json!({"needs_identity": ["name", "address"]})).unwrap();
    assert_eq!(page.page_fields(), vec![PayerField::Name, PayerField::Address]);
}

#[test]
fn an_unknown_detail_or_a_list_that_is_none_keeps_the_method_from_being_shown() {
    assert!(Identity::parse(&json!({"needs_identity": ["shoe_size"]})).is_err());
    assert!(Identity::parse(&json!({"identity_in_app": ["shoe_size"]})).is_err());
    assert!(Identity::parse(&json!({"needs_identity": "name"})).is_err());
    assert!(Identity::parse(&json!({"identity_in_app": {"fields": ["name"]}})).is_err());
}

#[test]
fn a_payer_misses_every_asked_field_that_is_empty_or_no_email() {
    let identity = Identity::parse(&json!({"identity_in_app": ["name", "email", "national_id"]}))
        .unwrap();
    let mut payer = Payer::default();
    assert_eq!(payer.missing(&identity), identity.in_app);
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
    let page = Identity::parse(&json!({"needs_identity": ["name", "email"]})).unwrap();
    assert!(
        Payer::default().missing(&page).is_empty(),
        "nothing is missing where the provider's page asks"
    );
}

#[test]
fn the_payer_sent_with_a_checkout_holds_only_the_asked_fields_trimmed() {
    let mut payer = Payer::default();
    payer.set(PayerField::Name, " Ana Souza ");
    payer.set(PayerField::Email, "ana@example.com");
    payer.set(PayerField::NationalId, "123.456.789-09");
    payer.set(PayerField::Phone, "+55 11 5555 0000");
    let name_only = Identity::parse(&json!({"needs_identity": ["name", "email"],
                                            "identity_in_app": ["name"]}))
    .unwrap();
    let sent = payer.for_checkout(&name_only).expect("the app asks for the name");
    assert_eq!(sent.name, "Ana Souza");
    assert!(sent.email.is_empty() && sent.national_id.is_empty() && sent.phone.is_empty());
    assert_eq!(sent.to_json(), json!({"name": "Ana Souza"}));
    let page = Identity::parse(&json!({"needs_identity": ["name"]})).unwrap();
    assert_eq!(payer.for_checkout(&page), None, "the provider's page asks: nothing is sent");
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
