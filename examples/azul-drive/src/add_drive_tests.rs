//! The Add drive dialog as data: its pages, its forms, what Buy storage offers. No window.

use azcloud_kit::{Tier, Tiers};
use azul_pay::{
    offer::{Offer, OfferContext},
    registry::{Method, SurfaceKind},
};
use azul_storage::{
    catalog::ServiceGroup,
    config::{DriveAuth, DriveEntry, DriveLocation},
    oauth::{self, Tokens},
};

use crate::{
    add_drive::{source_groups, AddDialog, AddPage, OfferState, TiersState},
    sign_in::{self, PendingSignIn, SignInSettings, SignInStep},
};

fn tiers() -> Tiers {
    let tier = |id: &str, gb: u64, month: u64, year: u64| Tier {
        id: id.to_string(),
        quota_bytes: gb * 1_000_000_000,
        price_month: Some(crate::pay_words_tests::cents(month, "EUR")),
        price_year: Some(crate::pay_words_tests::cents(year, "EUR")),
        currency: crate::pay_words_tests::currency("EUR"),
        first_month_free: true,
        yearly_only: false,
        prepay: azcloud_kit::azlin_contract::Prepay::new(
            azcloud_kit::azlin_contract::Months::ALL.to_vec(),
        )
        .expect("every period"),
        methods: Vec::new(),
    };
    Tiers {
        tiers: vec![tier("100GB", 100, 99, 990), tier("1TB", 1000, 499, 4990)],
        methods: vec!["sepa".to_string()],
        withdrawal_consent: None,
    }
}

#[test]
fn the_dialog_opens_on_its_two_choices() {
    let dialog = AddDialog::new(7);
    assert_eq!(dialog.page, AddPage::Choose);
    assert_eq!(dialog.page_line(), "choose");
    assert_eq!(dialog.serial, 7);
    assert!(!dialog.busy());
}

#[test]
fn connect_lists_the_sources_and_a_source_opens_its_form_back_and_forth() {
    let mut dialog = AddDialog::new(1);
    dialog.choose_connect();
    assert_eq!(dialog.page_line(), "sources");
    assert!(dialog.open_service("webdav") || !cfg!(feature = "opendal"));
    if cfg!(feature = "opendal") {
        assert_eq!(dialog.page_line(), "form webdav");
        assert_eq!(dialog.name, "WebDAV");
        dialog.back();
        assert_eq!(dialog.page_line(), "sources");
    }
    assert!(dialog.open_service("s3"));
    assert_eq!(dialog.page_line(), "form s3");
    assert_eq!(dialog.value("path_style"), "true", "the form starts at its defaults");
    dialog.back();
    dialog.back();
    assert_eq!(dialog.page_line(), "choose");
    assert!(!dialog.open_service("no-such-source"));
}

#[test]
fn a_filled_form_builds_its_drive_and_typing_forgets_the_last_test() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::new(1);
    dialog.choose_connect();
    assert!(dialog.open_service("s3"));
    dialog.set_name("Photos");
    assert!(dialog.check().unwrap_err().contains("Endpoint"));
    dialog.set_value("endpoint", "http://127.0.0.1:9000");
    dialog.set_value("bucket", "photos");
    dialog.set_value("access_key_id", "AKIDTEST");
    dialog.set_value("secret_access_key", "test-secret");
    assert!(dialog.check().is_ok());
    dialog.tested = Some(Ok("Connection OK".to_string()));
    dialog.error = "an old error".to_string();
    dialog.set_value("region", "eu-central-1");
    assert_eq!(dialog.tested, None, "a changed form is not the one tested");
    assert!(dialog.error.is_empty());
    let new = dialog.build("photos-1").unwrap();
    assert_eq!(new.entry.name, "Photos");
    assert!(matches!(
        &new.entry.location,
        DriveLocation::S3 { region, auth: DriveAuth::Keyring, .. } if region == "eu-central-1"
    ));
    let text = format!("{dialog:?}");
    assert!(!text.contains("test-secret"), "{text}");
}

#[test]
fn a_switch_flips_and_a_choice_takes_its_word() {
    let mut dialog = AddDialog::new(1);
    assert!(dialog.open_service("s3"));
    assert!(dialog.bool_value("path_style"));
    dialog.toggle("path_style");
    assert!(!dialog.bool_value("path_style"));
    assert_eq!(dialog.value("path_style"), "false");
    if dialog.open_service("postgres") {
        assert_eq!(dialog.value("sslmode"), "prefer");
        dialog.choose("sslmode", 2);
        assert_eq!(dialog.value("sslmode"), "require");
        dialog.choose("sslmode", 99);
        assert_eq!(dialog.value("sslmode"), "require", "no such word: unchanged");
    }
}

#[test]
fn a_drive_whose_keys_are_gone_opens_its_form_again_without_them() {
    let entry = DriveEntry {
        id: "s3-1".to_string(),
        name: "Archive".to_string(),
        location: DriveLocation::S3 {
            endpoint: "https://s3.example".to_string(),
            region: "eu-central-1".to_string(),
            bucket: "archive".to_string(),
            path_style: false,
            auth: DriveAuth::Keyring,
        },
    };
    let dialog = AddDialog::editing(&entry, 3);
    assert_eq!(dialog.page, AddPage::Form);
    assert_eq!(dialog.page_line(), "form s3");
    assert_eq!(dialog.editing.as_deref(), Some("s3-1"));
    assert_eq!(dialog.name, "Archive");
    assert_eq!(dialog.value("bucket"), "archive");
    assert_eq!(dialog.value("path_style"), "false");
    assert_eq!(dialog.value("secret_access_key"), "");
}

#[test]
fn buy_storage_shows_the_chosen_tier_and_its_price() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::new(1);
    dialog.country = String::from("DE");
    dialog.choose_buy();
    assert_eq!(dialog.page_line(), "buy");
    assert!(matches!(dialog.tiers, TiersState::NotLoaded));
    dialog.tiers = TiersState::Loaded(tiers());
    assert_eq!(dialog.chosen_tier().map(|t| t.id.as_str()), Some("100GB"));
    assert_eq!(dialog.buy_label(), "Buy 100 GB - EUR 0.99 for 1 month");
    assert_eq!(dialog.months(), 1);
    dialog.yearly = true;
    dialog.tier = 1;
    assert_eq!(dialog.buy_label(), "Buy 1 TB - EUR 49.90 for 12 months");
    assert_eq!(dialog.months(), 12);
    dialog.tier = 9;
    assert_eq!(dialog.chosen_tier(), None);
    dialog.back();
    assert_eq!(dialog.page_line(), "choose");
}

#[test]
fn the_sources_page_lists_every_group_that_has_a_source_of_this_build() {
    let (groups, unavailable) = source_groups();
    let ids = |group: ServiceGroup| -> Vec<&str> {
        groups
            .iter()
            .find(|(g, _)| *g == group)
            .map(|(_, list)| list.iter().map(|s| s.id).collect())
            .unwrap_or_default()
    };
    assert!(ids(ServiceGroup::CloudStorage).contains(&"s3"));
    assert!(ids(ServiceGroup::NetworkNas).contains(&"local"));
    for (_, list) in &groups {
        assert!(!list.is_empty());
        assert!(list.iter().all(|s| s.available()));
    }
    let listed: usize = groups.iter().map(|(_, l)| l.len()).sum();
    assert_eq!(listed + unavailable, azul_storage::catalog::services().len());
}

// ==== Buy storage's payment: the pills, the consent, the country (azul-pay) ====

/// GoCardless's SEPA (the server's default) and Stripe's card, SEPA and PayPal.
fn offer() -> Offer {
    let text = serde_json::json!({"offers": [
        {"provider": "gocardless", "default": true,
         "methods": [{"method": "sepa_debit", "surfaces": ["page", "browser"], "settles": "days"}]},
        {"provider": "stripe",
         "methods": [{"method": "card"}, {"method": "sepa_debit"}, {"method": "paypal"}]}
    ]})
    .to_string();
    Offer::parse(&text, &OfferContext::for_token_url("https://token.azlin.io")).unwrap()
}

/// Buy storage with the tiers and the offer in, paying from `country`.
fn paying_dialog(country: &str) -> AddDialog {
    let mut dialog = AddDialog::new(1);
    dialog.country = country.to_string();
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(tiers());
    dialog.offer_loaded(offer());
    dialog
}

fn methods(dialog: &AddDialog) -> Vec<Method> {
    dialog.pills().iter().map(|p| p.method).collect()
}

#[test]
fn buy_storage_without_payment_options_pays_on_the_v1_payment_page() {
    let mut dialog = AddDialog::new(1);
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(tiers());
    assert!(matches!(dialog.offer, OfferState::NotLoaded));
    assert!(!dialog.pays_with_pills());
    dialog.offer = OfferState::Legacy;
    assert!(!dialog.pays_with_pills());
    assert!(dialog.pills().is_empty());
    assert_eq!(dialog.choice(), None);
    let nothing_to_show = Offer::parse(
        r#"{"offers": [{"provider": "evilpay", "methods": [{"method": "card"}]}]}"#,
        &OfferContext::for_token_url("https://token.azlin.io"),
    )
    .unwrap();
    dialog.offer_loaded(nothing_to_show);
    assert!(!dialog.pays_with_pills(), "an offer of nothing this app knows is the v1 page");
}

#[test]
fn the_pills_are_the_methods_the_server_offered_in_its_order() {
    // The server answered for the payer's country and period: the app hides none of it.
    for country in ["DE", "US"] {
        let mut dialog = paying_dialog(country);
        assert!(dialog.pays_with_pills());
        assert_eq!(
            methods(&dialog),
            vec![Method::SepaDebit, Method::Card, Method::PayPal],
            "{country}"
        );
        assert_eq!(dialog.chosen_pill().map(|p| p.method), Some(Method::SepaDebit));
        dialog.yearly = true;
        assert_eq!(methods(&dialog).len(), 3, "{country}: the same offer, the same pills");
    }
}

#[test]
fn a_chosen_pill_stays_chosen_while_the_offer_has_it() {
    let mut dialog = paying_dialog("DE");
    dialog.choose_pill(Method::PayPal);
    assert_eq!(dialog.chosen_pill().map(|p| p.method), Some(Method::PayPal));
    dialog.yearly = true;
    assert_eq!(dialog.chosen_pill().map(|p| p.method), Some(Method::PayPal));
    let without_paypal = Offer::parse(
        &serde_json::json!({"offers": [{"provider": "stripe",
                                        "methods": [{"method": "card"}]}]})
        .to_string(),
        &OfferContext::for_token_url("https://token.azlin.io"),
    )
    .unwrap();
    dialog.offer = OfferState::Loaded(without_paypal);
    assert_eq!(
        dialog.chosen_pill().map(|p| p.method),
        Some(Method::Card),
        "a pill that went away gives way to the default"
    );
}

#[test]
fn a_pill_with_two_providers_switches_between_them() {
    let mut dialog = paying_dialog("DE");
    let sepa = dialog.chosen_pill().unwrap();
    assert_eq!(sepa.providers.len(), 2);
    assert_eq!(dialog.choice().unwrap().provider.spec.id, "gocardless");
    let stripe = sepa.providers[1];
    dialog.choose_provider(stripe);
    assert_eq!(dialog.choice().unwrap().provider.spec.id, "stripe");
    dialog.choose_pill(Method::Card);
    dialog.choose_pill(Method::SepaDebit);
    assert_eq!(
        dialog.choice().unwrap().provider.spec.id,
        "gocardless",
        "another pill forgets the switch"
    );
}

#[test]
fn the_choice_carries_the_surfaces_azdrive_can_show() {
    let mut dialog = paying_dialog("DE");
    assert_eq!(
        dialog.choice().unwrap().method.surfaces,
        vec![SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser],
        "GoCardless's hosted page, then the browser"
    );
    dialog.choose_pill(Method::Card);
    assert_eq!(
        dialog.choice().unwrap().method.surfaces,
        vec![
            SurfaceKind::PopoverFields,
            SurfaceKind::WebviewPage,
            SurfaceKind::SystemBrowser
        ]
    );
    assert_eq!(
        dialog.pills_line(),
        "sepa_debit:gocardless card:stripe paypal:stripe"
    );
}

#[test]
fn a_new_country_asks_for_the_tiers_and_the_options_again_in_its_currency() {
    let mut dialog = paying_dialog("DE");
    dialog.choose_pill(Method::Card);
    assert!(dialog.set_country("pl"));
    assert_eq!(dialog.country, "PL");
    assert!(matches!(dialog.tiers, TiersState::NotLoaded), "prices in zloty now");
    assert!(matches!(dialog.offer, OfferState::NotLoaded));
    assert_eq!(dialog.pill, None);
    assert!(!dialog.set_country("PL"), "the same country");
    assert!(!dialog.set_country("XX"), "not a listed country");
    assert!(!dialog.set_country("RU"), "a country no checkout may come from");
    assert_eq!(dialog.country, "PL");
}

// ==== Buy storage's first question: where the buyer lives ====

#[test]
fn buy_storage_asks_where_the_buyer_lives_first_when_nothing_is_declared() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::new(1);
    assert_eq!(dialog.country, "", "nothing declared yet");
    dialog.country_pick = Some("PL");
    dialog.choose_buy();
    assert_eq!(dialog.page, AddPage::Country);
    assert_eq!(dialog.page_line(), "country");
    assert_eq!(dialog.country_matches().len(), crate::countries::COUNTRIES.len());
    assert!(dialog.confirm_country(), "the guess confirmed");
    assert_eq!(dialog.country, "PL");
    assert_eq!(dialog.page, AddPage::Buy);
    let mut unsure = AddDialog::new(2);
    unsure.choose_buy();
    assert_eq!(unsure.page, AddPage::Country);
    assert!(!unsure.confirm_country(), "no guess and nothing picked");
    assert_eq!(unsure.page, AddPage::Country);
    assert!(!unsure.notice.is_empty(), "it says to pick a country");
}

#[test]
fn a_declared_country_goes_straight_to_the_tiers_and_can_be_changed() {
    let mut dialog = AddDialog::new(1);
    dialog.country = String::from("DE");
    dialog.choose_buy();
    assert_eq!(dialog.page, AddPage::Buy);
    dialog.change_country();
    assert_eq!(dialog.page, AddPage::Country);
    assert_eq!(dialog.country_pick, Some("DE"), "the declared country is picked");
    dialog.back();
    assert_eq!(dialog.page, AddPage::Buy, "back to the tiers of the declared country");
    let mut first = AddDialog::new(2);
    first.choose_buy();
    first.back();
    assert_eq!(first.page, AddPage::Choose);
}

#[test]
fn the_picker_lists_the_countries_matching_the_search_and_keeps_the_pick() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::new(1);
    dialog.choose_buy();
    dialog.search_country("pol");
    assert_eq!(dialog.country_query, "pol");
    assert_eq!(dialog.country_matches().first(), Some(&"PL"));
    assert!(dialog.pick_country("pl"));
    assert_eq!(dialog.country_pick, Some("PL"));
    assert!(!dialog.pick_country("RU"), "never a country no checkout may come from");
    assert!(!dialog.pick_country("XX"));
    assert_eq!(dialog.country_pick, Some("PL"));
    dialog.search_country("united");
    assert_eq!(dialog.country_pick, Some("PL"), "searching again keeps the pick");
    assert!(dialog.confirm_country());
    assert_eq!(dialog.country, "PL");
}

// ==== The payer's details: only for a method whose provider wants them from the app ====

/// Stripe's card described further by `extra` (its `needs_identity`, `identity_in_app`).
fn card_offer(extra: serde_json::Value) -> Offer {
    let mut card = serde_json::json!({"method": "card"});
    if let (Some(c), Some(e)) = (card.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            c.insert(k.clone(), v.clone());
        }
    }
    Offer::parse(
        &serde_json::json!({"offers": [{"provider": "stripe", "methods": [card]}]}).to_string(),
        &OfferContext::for_token_url("https://token.azlin.io"),
    )
    .unwrap()
}

#[test]
fn the_payers_details_are_asked_only_where_the_provider_wants_them_from_the_app() {
    use azul_pay::PayerField;
    let mut dialog = paying_dialog("BR");
    dialog.offer_loaded(card_offer(serde_json::json!({
        "needs_identity": ["name", "email", "national_id"],
        "identity_in_app": ["name", "email", "national_id"]})));
    assert_eq!(
        dialog.asked_fields(),
        vec![PayerField::Name, PayerField::Email, PayerField::NationalId]
    );
    dialog.offer_loaded(card_offer(serde_json::json!({"needs_identity": ["name"]})));
    assert!(dialog.asked_fields().is_empty(), "the provider's own page asks for the name");
    dialog.offer_loaded(card_offer(serde_json::json!({})));
    assert!(dialog.asked_fields().is_empty());
}

#[test]
fn the_payers_details_leave_the_dialog_with_the_checkout_and_are_never_shown() {
    use azul_pay::PayerField;
    let mut dialog = paying_dialog("BR");
    dialog.set_payer(PayerField::Name, "Ana Souza");
    dialog.set_payer(PayerField::NationalId, "123.456.789-09");
    let text = format!("{dialog:?}");
    assert!(!text.contains("Ana") && !text.contains("123.456"), "{text}");
    let sent = dialog.take_payer();
    assert_eq!(sent.name, "Ana Souza");
    assert!(dialog.payer.is_empty(), "the dialog keeps none of them");
}

#[test]
fn new_options_keep_the_chosen_pill_while_they_offer_it() {
    let mut dialog = paying_dialog("DE");
    dialog.choose_pill(Method::Card);
    dialog.offer_loaded(offer());
    assert_eq!(
        dialog.chosen_pill().map(|p| p.method),
        Some(Method::Card),
        "the options asked again for another tier or period"
    );
}

#[test]
fn an_answer_for_another_country_tier_or_period_is_not_taken() {
    // The options are asked for again on every change; an answer that was on its way for the
    // order as it was before must not replace the current one.
    let mut dialog = paying_dialog("DE");
    let asked = dialog.options_key();
    assert!(dialog.options_current(&asked));
    dialog.yearly = true;
    assert!(!dialog.options_current(&asked), "the period changed since");
    dialog.yearly = false;
    dialog.tier = 1;
    assert!(!dialog.options_current(&asked), "another tier");
    dialog.tier = 0;
    assert!(dialog.options_current(&asked));
    dialog.set_country("PL");
    assert!(!dialog.options_current(&asked), "another country");
    assert!(dialog.tiers_current("PL") && !dialog.tiers_current("DE"));
}

#[test]
fn the_buyers_country_is_remembered_in_this_computers_settings_only() {
    let mut settings = crate::model::Settings::default();
    assert_eq!(settings.residence_country, None);
    settings.residence_country = Some(String::from("PL"));
    let text = settings.to_json();
    assert!(text.contains("\"residence_country\": \"PL\""), "{text}");
    assert_eq!(
        crate::model::Settings::from_json(&text).residence_country.as_deref(),
        Some("PL")
    );
    assert_eq!(crate::model::Settings::from_json("{}").residence_country, None);
}

#[test]
fn the_dialog_is_busy_while_a_payment_runs_and_its_debug_text_holds_no_secret() {
    let mut dialog = paying_dialog("DE");
    dialog.choose_pill(Method::Card);
    assert!(!dialog.busy());
    let choice = dialog.choice().unwrap();
    let (preparing, effects) = azul_pay::step(
        azul_pay::State::Choosing,
        azul_pay::Event::Pay {
            choice: choice.clone(),
            consent: true,
            payer: azul_pay::Payer::default(),
        },
    );
    assert_eq!(effects.len(), 1);
    dialog.pay = preparing;
    assert!(dialog.busy());
    let answer = serde_json::json!({"checkout_id": "ck_1", "provider": "stripe", "method": "card",
        "surface": {"kind": "fields", "page": "https://pay.azlin.io/fields/stripe/v1",
                    "publishable_key": "pk_test_SECRETPK", "client_secret": "pi_1_secret_SECRETCS"}});
    let created = azul_pay::Created::parse(&answer, &choice, &dialog.look()).unwrap();
    let (presenting, _) = azul_pay::step(
        std::mem::replace(&mut dialog.pay, azul_pay::State::Choosing),
        azul_pay::Event::Created(Box::new(created)),
    );
    dialog.pay = presenting;
    dialog.card_name = "Felix Example".to_string();
    assert!(dialog.busy());
    let text = format!("{dialog:?}");
    for secret in ["SECRETPK", "SECRETCS", "Felix"] {
        assert!(!text.contains(secret), "{secret}: {text}");
    }
}

// ==== Connect data source > a consumer cloud's sign-in ====

/// The environment `pairs` describe, as `SignInSettings::resolve` reads it.
fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_string())
    }
}

fn settings(pairs: &'static [(&'static str, &'static str)]) -> SignInSettings {
    SignInSettings::resolve(&vars(pairs), None)
}

fn tokens(refresh: Option<&str>) -> Tokens {
    Tokens {
        access_token: String::from("ya29.ACCESS"),
        refresh_token: refresh.map(String::from),
        expires_in: Some(3599),
        scope: None,
    }
}

#[test]
fn a_consumer_cloud_without_a_client_id_says_which_setting_is_missing() {
    crate::l10n::in_english();
    for (scheme, var, key, name) in [
        ("gdrive", "AZDRIVE_GOOGLE_CLIENT_ID", "\"google\"", "Google Drive"),
        ("dropbox", "AZDRIVE_DROPBOX_CLIENT_ID", "\"dropbox\"", "Dropbox"),
        ("onedrive", "AZDRIVE_ONEDRIVE_CLIENT_ID", "\"onedrive\"", "OneDrive"),
    ] {
        let why = sign_in::plan(scheme, &settings(&[])).unwrap_err();
        for part in [var, key, "client_id", name, ".azlin/config.json"] {
            assert!(why.contains(part), "{scheme}: {part} in {why}");
        }
        let provider = oauth::provider(scheme).unwrap();
        assert_eq!(sign_in::missing_client(provider), why);
    }
}

#[test]
fn a_client_id_from_the_environment_plans_a_sign_in_at_the_providers_endpoints() {
    let plan = sign_in::plan(
        "gdrive",
        &settings(&[
            ("AZDRIVE_GOOGLE_CLIENT_ID", "123-abc.apps.googleusercontent.com"),
            ("AZDRIVE_GOOGLE_CLIENT_SECRET", "GOCSPX-x"),
        ]),
    )
    .unwrap();
    assert_eq!(plan.provider, &oauth::GOOGLE);
    assert_eq!(plan.client.client_id, "123-abc.apps.googleusercontent.com");
    assert_eq!(plan.client.client_secret.as_deref(), Some("GOCSPX-x"));
    assert_eq!(plan.authorize_endpoint, oauth::GOOGLE.authorize_url);
    assert_eq!(plan.token_url, oauth::GOOGLE.token_url);
    assert_eq!(plan.scope, oauth::GOOGLE.scope);
    assert_eq!(plan.redirect_uri, sign_in::DEFAULT_REDIRECT_URI);
    assert_eq!(plan.extras_query(), "&access_type=offline&prompt=consent");
    assert_eq!(plan.token_url_option(), None, "the provider's own endpoint");
    let dropbox = sign_in::plan("dropbox", &settings(&[("AZDRIVE_DROPBOX_CLIENT_ID", "d")]));
    assert_eq!(dropbox.unwrap().extras_query(), "&token_access_type=offline");
}

#[test]
fn the_settings_name_another_redirect_token_endpoint_and_scope() {
    let plan = sign_in::plan(
        "dropbox",
        &settings(&[
            ("AZDRIVE_DROPBOX_CLIENT_ID", "dbx"),
            ("AZDRIVE_DROPBOX_REDIRECT_URI", "http://localhost:53682/"),
            ("AZDRIVE_DROPBOX_TOKEN_URL", "http://127.0.0.1:8081/oauth/dropbox/token"),
            ("AZDRIVE_DROPBOX_AUTHORIZE_URL", "http://127.0.0.1:8081/oauth/dropbox/authorize"),
            ("AZDRIVE_DROPBOX_SCOPE", "files.content.read"),
        ]),
    )
    .unwrap();
    assert_eq!(plan.redirect_uri, "http://localhost:53682/");
    assert_eq!(
        plan.authorize_endpoint,
        "http://127.0.0.1:8081/oauth/dropbox/authorize"
    );
    assert_eq!(plan.scope, "files.content.read");
    assert_eq!(
        plan.token_url_option(),
        Some("http://127.0.0.1:8081/oauth/dropbox/token")
    );
}

#[test]
fn a_source_without_a_sign_in_has_no_plan() {
    assert!(sign_in::plan("webdav", &settings(&[])).is_err());
    assert!(sign_in::plan("s3", &settings(&[])).is_err());
}

#[test]
fn the_client_ids_come_from_the_shared_config_under_the_environment() {
    let dir = azul_storage::testing::TempDir::new("azdrive-sign-in-settings");
    let path = dir.0.join("config.json");
    std::fs::write(
        &path,
        r#"{"oauth": {"google": {"client_id": "file-google"}, "onedrive": {"client_id": "file-onedrive"}}}"#,
    )
    .unwrap();
    let resolved = SignInSettings::resolve(
        &vars(&[("AZDRIVE_GOOGLE_CLIENT_ID", "env-google")]),
        Some(&path),
    );
    let id = |scheme: &str| sign_in::plan(scheme, &resolved).map(|p| p.client.client_id);
    assert_eq!(id("gdrive").as_deref(), Ok("env-google"));
    assert_eq!(id("onedrive").as_deref(), Ok("file-onedrive"));
    assert!(id("dropbox").is_err());
}

#[test]
fn a_token_answer_without_a_refresh_token_cannot_keep_a_drive_signed_in() {
    crate::l10n::in_english();
    let plan = sign_in::plan("gdrive", &settings(&[("AZDRIVE_GOOGLE_CLIENT_ID", "id")])).unwrap();
    let why = sign_in::form_settings(&plan, &tokens(None)).unwrap_err();
    assert!(why.contains("refresh token"), "{why}");
    assert!(why.contains("Google Drive"), "{why}");
    let filled = sign_in::form_settings(&plan, &tokens(Some("1//R"))).unwrap();
    assert!(filled.contains(&("refresh_token", String::from("1//R"))));
    assert!(filled.contains(&("client_id", String::from("id"))));
    assert!(filled.contains(&("access_token", String::new())), "no fixed token");
}

fn signing_in_dialog(pairs: &'static [(&'static str, &'static str)]) -> Option<AddDialog> {
    let mut dialog = AddDialog::new(3);
    dialog.choose_connect();
    if !dialog.open_service("gdrive") {
        assert!(!cfg!(feature = "opendal"), "Google Drive is a source of this build");
        return None;
    }
    assert_eq!(dialog.sign_in_provider(), Some(&oauth::GOOGLE));
    let plan = sign_in::plan("gdrive", &settings(pairs)).unwrap();
    dialog.sign_in_waiting(PendingSignIn {
        plan,
        code_verifier: String::from("VERIFIER-dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1g"),
        code_challenge: String::from("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"),
        state: String::from("st4te"),
    });
    Some(dialog)
}

#[test]
fn a_sign_in_fills_the_forms_refresh_token_and_client_and_the_drive_keeps_them_apart() {
    let Some(mut dialog) = signing_in_dialog(&[
        ("AZDRIVE_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
        ("AZDRIVE_GOOGLE_CLIENT_SECRET", "GOCSPX-s"),
    ]) else {
        return;
    };
    assert_eq!(dialog.sign_in, SignInStep::Waiting);
    assert!(dialog.signing_in());
    dialog.sign_in_exchanging();
    assert_eq!(dialog.sign_in, SignInStep::Exchanging);
    dialog.signed_in(&tokens(Some("1//REFRESH"))).unwrap();
    assert_eq!(dialog.sign_in, SignInStep::SignedIn);
    assert!(!dialog.signing_in());
    assert!(dialog.pending_sign_in.is_none(), "the verifier is gone");
    assert_eq!(dialog.value("refresh_token"), "1//REFRESH");
    assert_eq!(dialog.value("client_id"), "id.apps.googleusercontent.com");
    assert_eq!(dialog.value("client_secret"), "GOCSPX-s");
    let new = dialog.build("g1").unwrap();
    let DriveLocation::Opendal {
        scheme,
        options,
        keyring,
    } = &new.entry.location
    else {
        panic!("{:?}", new.entry.location);
    };
    assert_eq!(scheme, "gdrive");
    assert!(*keyring);
    assert_eq!(
        options.get("client_id").map(String::as_str),
        Some("id.apps.googleusercontent.com")
    );
    assert!(!options.contains_key("token_url"), "the provider's own endpoint");
    let entry_text = serde_json::to_string(&new.entry).unwrap();
    assert!(
        !entry_text.contains("1//REFRESH") && !entry_text.contains("GOCSPX"),
        "{entry_text}"
    );
    let secret = new.secret.unwrap();
    assert!(secret.contains("1//REFRESH"), "the keyring's text holds it");
}

#[test]
fn a_drive_signed_in_at_another_token_endpoint_keeps_it() {
    let Some(mut dialog) = signing_in_dialog(&[
        ("AZDRIVE_GOOGLE_CLIENT_ID", "id"),
        ("AZDRIVE_GOOGLE_TOKEN_URL", "http://127.0.0.1:8081/oauth/google/token"),
    ]) else {
        return;
    };
    dialog.signed_in(&tokens(Some("R"))).unwrap();
    let new = dialog.build("g2").unwrap();
    let DriveLocation::Opendal { options, .. } = &new.entry.location else {
        panic!("not an OpenDAL drive");
    };
    assert_eq!(
        options.get("token_url").map(String::as_str),
        Some("http://127.0.0.1:8081/oauth/google/token")
    );
}

#[test]
fn a_failed_sign_in_says_why_and_choosing_the_source_again_forgets_it() {
    let Some(mut dialog) = signing_in_dialog(&[("AZDRIVE_GOOGLE_CLIENT_ID", "id")]) else {
        return;
    };
    dialog.sign_in_failed("The sign-in was cancelled.");
    assert_eq!(
        dialog.sign_in,
        SignInStep::Failed(String::from("The sign-in was cancelled."))
    );
    assert!(dialog.pending_sign_in.is_none());
    assert!(dialog.signed_in(&tokens(Some("R"))).is_err(), "no sign-in waits");
    dialog.back();
    assert!(dialog.open_service("gdrive"));
    assert_eq!(dialog.sign_in, SignInStep::Idle);
}

#[test]
fn the_dialogs_debug_text_shows_no_verifier_and_no_refresh_token() {
    let Some(mut dialog) = signing_in_dialog(&[
        ("AZDRIVE_GOOGLE_CLIENT_ID", "id"),
        ("AZDRIVE_GOOGLE_CLIENT_SECRET", "GOCSPX-secret"),
    ]) else {
        return;
    };
    let waiting = format!("{dialog:?}");
    assert!(!waiting.contains("VERIFIER"), "{waiting}");
    assert!(!waiting.contains("GOCSPX"), "{waiting}");
    dialog.signed_in(&tokens(Some("1//REFRESH"))).unwrap();
    let signed_in = format!("{dialog:?}");
    for secret in ["1//REFRESH", "GOCSPX", "ya29"] {
        assert!(!signed_in.contains(secret), "{secret}: {signed_in}");
    }
}

/// The user's ruling (2026-10-10): "we always encrypt and compress" - AzDrive's default build
/// opens Azlin drives through the encryption and makes new ones encrypted.
#[test]
fn a_default_build_of_azdrive_encrypts_its_azlin_drives() {
    assert!(
        cfg!(feature = "encryption"),
        "AzDrive's default features take `encryption`"
    );
}

/// A tier sold only by the year (12 or 24 months), with a direct debit and a card only.
fn yearly_only_tiers() -> Tiers {
    let yearly = Tier {
        id: String::from("12TB"),
        quota_bytes: 12_000_000_000_000,
        price_month: None,
        price_year: Some(crate::pay_words_tests::cents(34_990, "EUR")),
        currency: crate::pay_words_tests::currency("EUR"),
        first_month_free: true,
        yearly_only: true,
        prepay: azcloud_kit::azlin_contract::Prepay::new(vec![
            azcloud_kit::azlin_contract::Months::try_from(12).expect("12"),
            azcloud_kit::azlin_contract::Months::try_from(24).expect("24"),
        ])
        .expect("a year or two"),
        methods: vec![String::from("sepa"), String::from("card")],
    };
    let mut both = tiers();
    both.tiers.push(yearly);
    both.methods.push(String::from("cash"));
    both
}

#[test]
fn a_tier_sold_only_by_the_year_is_offered_and_ordered_by_the_year_only() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::new(1);
    dialog.country = String::from("DE");
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(yearly_only_tiers());
    dialog.tier = 2;
    // "Pay yearly" is off, as it starts: the tier is still ordered for a year.
    assert!(!dialog.yearly);
    assert_eq!(dialog.periods(), crate::add_drive::Periods::YearOnly);
    assert_eq!(dialog.months(), 12, "never a month the token server refuses");
    assert_eq!(dialog.buy_label(), "Buy 12 TB - EUR 349.90 for 12 months");
    // A tier sold both ways keeps the switch.
    dialog.tier = 0;
    assert_eq!(dialog.periods(), crate::add_drive::Periods::MonthOrYear);
    assert_eq!(dialog.months(), 1);
}

#[test]
fn cash_by_post_is_offered_only_for_a_tier_sold_with_it() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::new(1);
    dialog.country = String::from("DE");
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(yearly_only_tiers());
    dialog.tier = 0;
    assert!(dialog.cash_choice().is_some(), "every method: cash too");
    dialog.tier = 2;
    assert!(dialog.cash_choice().is_none(), "this tier is sold by debit or card only");
    let tier = dialog.chosen_tier().expect("the tier");
    assert_eq!(tier.v1_method(), "sepa");
}

#[test]
fn a_country_azlin_does_not_sell_to_is_said_in_the_windows_words_and_not_offered_again() {
    crate::l10n::in_english();
    let refused = azcloud_kit::TokenError::Refused {
        status: 403,
        code: String::from("blocked_country"),
        message: String::from("Azlin Storage is not sold to XX yet"),
    };
    let said = crate::jobs::buyer_refused(&refused).expect("a refusal of this buyer");
    let text = azul_appkit::l10n::t_text(&said);
    assert!(text.contains("not sold in your country"), "{text}");
    assert!(!text.contains("XX") && !text.contains("HTTP"), "never the server's sentence: {text}");
    let busy = azcloud_kit::TokenError::Refused {
        status: 503,
        code: String::from("no_price_ladder"),
        message: String::new(),
    };
    assert!(crate::jobs::buyer_refused(&busy).is_none(), "asked again with Try again");
}

/// AZ-P14: only the contract's `blocked_country` says "not sold in your country". A 403 of an edge,
/// a firewall or a proxy (any other word, or none) is an answer to try again, not a lock-out of
/// every buyer behind it until the dialog is reopened.
#[test]
fn az_p14_a_403_without_the_word_blocked_country_is_not_a_refused_buyer() {
    for (code, message) in [
        ("forbidden", "denied by the edge"),
        ("", ""),
        ("waf_blocked", "request blocked"),
        ("drive_banned", "this drive was closed"),
    ] {
        let edge = azcloud_kit::TokenError::Refused {
            status: 403,
            code: code.to_string(),
            message: message.to_string(),
        };
        assert!(
            crate::jobs::buyer_refused(&edge).is_none(),
            "a 403 {code:?} is asked again with Try again"
        );
    }
}

/// Every drive is encrypted as it is made: a build of AzDrive that cannot encrypt starts no
/// purchase, test drive, voucher or claim (no payment is taken for a drive it would leave
/// plain), and says why; connecting a data source of the user's own goes on.
#[test]
fn a_build_that_cannot_encrypt_makes_no_azlin_drive_and_says_why() {
    use crate::add_flow::{refuses_making, AddEvent};
    crate::l10n::in_english();
    for event in [
        AddEvent::ChooseBuy,
        AddEvent::Buy,
        AddEvent::CreateTestDrive,
        AddEvent::VoucherPage,
        AddEvent::RedeemVoucher,
        AddEvent::ClaimCodePage,
        AddEvent::PickUp,
        AddEvent::PayCash,
    ] {
        let said = refuses_making(&event, false).unwrap_or_else(|| panic!("{event:?} runs"));
        assert!(said.contains("encrypt"), "{said}");
        assert_eq!(refuses_making(&event, true), None, "{event:?}");
    }
    for event in [AddEvent::ChooseConnect, AddEvent::Back, AddEvent::Cancel, AddEvent::Save] {
        assert_eq!(refuses_making(&event, false), None, "{event:?}");
    }
}

// ==== RF52: the price the buyer is shown is the price the checkout charges ====

use crate::pay_words_tests::{cents, currency};

/// A buyer in Poland: the ladder prices the tiers in zloty; Stripe's card charges zloty, the SEPA
/// direct debit of GoCardless takes euros only (the server's default pill).
fn polish_dialog() -> AddDialog {
    let mut dialog = AddDialog::new(1);
    dialog.country = String::from("PL");
    dialog.choose_buy();
    let mut zloty = tiers();
    for tier in &mut zloty.tiers {
        tier.price_month = Some(cents(449, "PLN"));
        tier.price_year = Some(cents(4490, "PLN"));
        tier.currency = currency("PLN");
    }
    dialog.tiers = TiersState::Loaded(zloty);
    let text = serde_json::json!({
        "country": "PL", "currency": "PLN",
        "offers": [
            {"provider": "gocardless", "default": true, "methods": [
                {"method": "sepa_debit", "surfaces": ["page", "browser"], "settles": "days",
                 "rank": 0, "amount_cents": 99, "currency": "EUR"}]},
            {"provider": "stripe", "methods": [
                {"method": "card", "rank": 1, "amount_cents": 449, "currency": "PLN"}]}],
        "price": {"amount_cents": 449, "currency": "PLN", "vat_rate_permille": 230,
                  "vat_cents": 84, "vat_included": true},
    })
    .to_string();
    dialog.offer_loaded(
        Offer::parse(
            &text,
            &OfferContext::for_token_url("https://token.azlin.io"),
        )
        .unwrap(),
    );
    dialog
}

/// AZ-P2: a payer in Poland whose SEPA direct debit takes euros only. The button, the price line
/// and the quote the checkout is held to say what THE METHOD charges - EUR 0.99 - and not the
/// ladder's zloty; the card says zloty.
#[test]
fn az_p2_the_buy_button_and_the_quote_say_the_chosen_methods_own_charge() {
    crate::l10n::in_english();
    let mut dialog = polish_dialog();
    assert_eq!(
        dialog.chosen_pill().map(|p| p.method),
        Some(Method::SepaDebit)
    );
    assert_eq!(dialog.buy_label(), "Buy 100 GB - EUR 0.99 for 1 month");
    let shown = dialog.price_shown().expect("the method's charge");
    assert_eq!((shown.amount, shown.currency.as_str()), (99, "EUR"));
    let quote = dialog.quote_for(dialog.choice().as_ref()).expect("a quote");
    assert_eq!(
        (
            quote.price.amount,
            quote.price.currency.as_str(),
            quote.months,
            quote.at_most
        ),
        (99, "EUR", 1, false)
    );
    assert_eq!(quote.tier, "100GB");
    dialog.choose_pill(Method::Card);
    assert_eq!(dialog.buy_label(), "Buy 100 GB - PLN 4.49 for 1 month");
    let shown = dialog.price_shown().expect("the card's charge");
    assert_eq!((shown.amount, shown.currency.as_str()), (449, "PLN"));
    // A method that names no charge reads the offer's price - in the tier's currency, here
    // zloty -, else the tier list's.
    let mut no_charge = polish_dialog();
    no_charge.offer = OfferState::Loaded(
        Offer::parse(
            &serde_json::json!({"offers": [{"provider": "stripe", "methods": [{"method": "card"}]}],
                "price": {"amount_cents": 449, "currency": "PLN"}})
            .to_string(),
            &OfferContext::for_token_url("https://token.azlin.io"),
        )
        .unwrap(),
    );
    assert_eq!(no_charge.price_shown(), Some(cents(449, "PLN")));
    no_charge.offer = OfferState::Loaded(
        Offer::parse(
            &serde_json::json!({"offers": [{"provider": "stripe", "methods": [{"method": "card"}]}],
                "price": {"amount_cents": 99, "currency": "EUR"}})
            .to_string(),
            &OfferContext::for_token_url("https://token.azlin.io"),
        )
        .unwrap(),
    );
    assert_eq!(
        no_charge.price_shown(),
        Some(cents(449, "PLN")),
        "an offer price in another currency than the tier's is not the tier's price"
    );
}

/// X-AR5b-20: a renewal under a price lock pays the lock's price in the lock's currency - not
/// today's list price in the declared country's currency. The dialog says the lock's price only
/// when this computer paid for the lock for as many months; it says none otherwise, and the
/// order is held to it as a ceiling.
#[test]
fn x_ar5b_20_a_renewal_under_a_price_lock_never_quotes_todays_list_price() {
    use crate::renewals::{LockPaid, LockSeen, RenewFor};
    crate::l10n::in_english();
    let renewing = |lock: Option<LockSeen>| {
        let mut dialog = AddDialog::renewing(
            1,
            RenewFor {
                drive_id: String::from("d_1"),
                name: String::from("Photos"),
                tier: String::from("100GB"),
                quota_bytes: 100_000_000_000,
                lock,
                lock_unread: None,
            },
        );
        dialog.country = String::from("DE");
        dialog.tiers_loaded(tiers());
        dialog
    };
    // No lock: today's price is what is charged.
    let plain = renewing(None);
    assert_eq!(plain.buy_label(), "Renew 100 GB - EUR 0.99 for 1 month");
    assert!(!plain.quote_for(None).expect("a quote").at_most);
    // A lock this computer did not pay for: no number is said, and none is checked.
    let unknown = renewing(Some(LockSeen { paid: None }));
    assert_eq!(unknown.price_shown(), None);
    assert_eq!(unknown.quote_for(None), None);
    assert_eq!(unknown.buy_label(), "Renew 100 GB");
    // A lock paid for one month at EUR 0.89: that is the price, as a ceiling.
    let mut locked = renewing(Some(LockSeen {
        paid: Some(LockPaid {
            months: 1,
            price: cents(89, "EUR"),
        }),
    }));
    assert_eq!(locked.buy_label(), "Renew 100 GB - EUR 0.89 for 1 month");
    let quote = locked.quote_for(None).expect("a quote");
    assert_eq!((quote.price.amount, quote.at_most), (89, true));
    // Another number of months than the lock was paid for: the dialog does not guess.
    locked.yearly = true;
    assert_eq!(locked.price_shown(), None);
    assert_eq!(locked.buy_label(), "Renew 100 GB");
}

/// X-DELTA-AZDRIVE-MONEY-6: a renewal of a drive whose vaults were not all read at its last look
/// waits - "no price lock" is not what an unreadable vault says - with its own reason; a drive
/// whose look read every vault, and a move to another tier (which no lock covers), do not wait.
/// Before, the dialog offered the renewal at today's price and the order was placed at it.
#[test]
fn x_delta_azdrive_money_6_a_renewal_waits_while_the_price_lock_could_not_be_looked_for() {
    use crate::renewals::RenewFor;
    crate::l10n::in_english();
    let renewing = |lock_unread: Option<&str>| {
        let mut dialog = AddDialog::renewing(
            1,
            RenewFor {
                drive_id: String::from("d_1"),
                name: String::from("Photos"),
                tier: String::from("100GB"),
                quota_bytes: 100_000_000_000,
                lock: None,
                lock_unread: lock_unread.map(String::from),
            },
        );
        dialog.country = String::from("DE");
        dialog.tiers_loaded(tiers());
        dialog.offer = OfferState::Legacy;
        dialog
    };
    assert_eq!(
        renewing(None).order_waits_for(),
        None,
        "every vault was read"
    );
    let mut unread = renewing(Some("the drive is not open"));
    assert_eq!(unread.order_waits_for(), Some("azdrive-renew-lock-unread"));
    // Another tier is a move, which no price lock covers: it does not wait for the lock.
    unread.tier = 1;
    assert!(unread.renew_order().is_some_and(|order| order.change_tier));
    assert_eq!(unread.order_waits_for(), None);
}

/// AZ-P4 (the v1 page's wiring): the order button ends a checkout the dialog has in flight before
/// it orders again - it may be paid already or any moment. A dialog with nothing in flight orders
/// at once, one whose checkout is being ended waits for the end and does not end it twice. Before,
/// only azul-pay's machine was tested for this; the button of the v1 page decides by this answer.
#[test]
fn az_p4_the_order_button_ends_a_checkout_in_flight_before_it_orders() {
    use azul_pay::State as PayState;

    use crate::add_flow::must_release_first;
    let mut dialog = AddDialog::new(1);
    assert!(
        !must_release_first(&dialog),
        "nothing in flight: order at once"
    );
    dialog.pay = PayState::InFlight {
        checkout_id: String::from("ck_1"),
    };
    assert!(
        must_release_first(&dialog),
        "in flight: it may be paid, end it first"
    );
    dialog.pay = PayState::Releasing {
        checkout_id: String::from("ck_1"),
    };
    assert!(
        !must_release_first(&dialog),
        "being ended: wait for the end"
    );
    // The dialog's own state agrees with the machine's.
    dialog.pay = PayState::InFlight {
        checkout_id: String::from("ck_1"),
    };
    assert!(!dialog.pay.may_order() && !dialog.pay.busy());
}

/// X-AR5b-19 (the wiring): the options call's answer makes the dialog's offer. No such route at a
/// token server on this computer is the v1 page; at any other token server it is a failure to ask
/// again, never a page. A call that failed is its failure, and the token server's refusal of this
/// buyer is shown as it is. Before, only the helper `v1_page_allowed` was tested.
#[test]
fn x_ar5b_19_the_options_answer_makes_the_offer_by_where_the_token_server_is() {
    use crate::{
        add_flow::{offer_from, OptionsAnswer},
        jobs::OptionsFailed,
    };
    crate::l10n::in_english();
    let state = |answer: OptionsAnswer| match answer {
        OptionsAnswer::State(state) => state,
        OptionsAnswer::Offer(_) => panic!("an offer"),
    };
    assert!(matches!(
        state(offer_from("http://127.0.0.1:18081", Ok(None))),
        OfferState::Legacy
    ));
    match state(offer_from("https://token.azlin.io", Ok(None))) {
        OfferState::Failed(why) => assert!(!why.is_empty()),
        _ => panic!("a production token server without the route is a failure"),
    }
    match state(offer_from(
        "https://token.azlin.io",
        Err(OptionsFailed::Failed(String::from("no answer"))),
    )) {
        OfferState::Failed(why) => assert_eq!(why, "no answer"),
        _ => panic!("a failed call is a failure"),
    }
    assert!(matches!(
        state(offer_from(
            "http://127.0.0.1:18081",
            Err(OptionsFailed::Refused(azul_appkit::l10n::Text::plain(
                "not here"
            )))
        )),
        OfferState::Refused(_)
    ));
    // An answer that is no offer is a failure too, never a page.
    match state(offer_from(
        "http://127.0.0.1:18081",
        Ok(Some(String::from("not json"))),
    )) {
        OfferState::Failed(why) => assert!(!why.is_empty()),
        _ => panic!("an answer that is no offer"),
    }
}

/// X-AR5b-19: the v1 payment page stands in only for a token server on this computer. A
/// production token server whose payment options fail, or have no such route, is "Try again".
#[test]
fn x_ar5b_19_the_v1_page_is_for_a_token_server_on_this_computer_only() {
    use crate::add_drive::v1_page_allowed;
    assert!(v1_page_allowed("http://127.0.0.1:8081"));
    assert!(v1_page_allowed("http://localhost:8081/"));
    assert!(!v1_page_allowed("https://token.azlin.io"));
    assert!(!v1_page_allowed("https://127.0.0.1.evil.example"));
    assert!(!v1_page_allowed(""));
    // A failed call is a reason to ask again, never a page.
    let mut dialog = AddDialog::new(1);
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(tiers());
    dialog.offer = OfferState::Failed(String::from("no answer"));
    assert_eq!(dialog.options_failed(), Some("no answer"));
    assert_eq!(
        dialog.order_waits_for(),
        Some("azdrive-add-options-failed-short")
    );
    dialog.offer = OfferState::Legacy;
    assert_eq!(dialog.options_failed(), None);
    assert_eq!(
        dialog.order_waits_for(),
        None,
        "a development server's v1 page"
    );
}

/// X-AR5b-10: what the order button waits for is named for every state of the tiers and of the
/// options; none is "ready" by default.
#[test]
fn x_ar5b_10_the_order_button_waits_for_each_state_by_name() {
    let mut dialog = AddDialog::new(1);
    for tiers in [
        TiersState::NotLoaded,
        TiersState::Loading,
        TiersState::Failed(String::from("x")),
        TiersState::Refused(String::from("x")),
    ] {
        dialog.tiers = tiers;
        assert_eq!(
            dialog.order_waits_for(),
            Some("azdrive-add-tiers-not-loaded")
        );
    }
    dialog.tiers = TiersState::Loaded(tiers());
    for (offer, waits) in [
        (OfferState::NotLoaded, Some("azdrive-add-tiers-not-loaded")),
        (OfferState::Loading, Some("azdrive-add-tiers-not-loaded")),
        (
            OfferState::Failed(String::from("x")),
            Some("azdrive-add-options-failed-short"),
        ),
        (OfferState::Legacy, None),
        (OfferState::Refused(String::from("not here")), None),
        (OfferState::Loaded(offer()), None),
    ] {
        dialog.offer = offer;
        assert_eq!(dialog.order_waits_for(), waits);
    }
}

/// AZ-P11: the yearly switch says what a year saves, worked out from the ladder's own prices -
/// not "two months for free" whatever the ladder is. A year that is not cheaper says nothing of
/// savings.
#[test]
fn az_p11_the_yearly_label_follows_the_ladder() {
    crate::l10n::in_english();
    let tier = |month: u64, year: u64| {
        let mut tier = tiers().tiers.remove(0);
        tier.price_month = Some(cents(month, "EUR"));
        tier.price_year = Some(cents(year, "EUR"));
        tier
    };
    assert_eq!(
        crate::pay_words::yearly_label(Some(&tier(99, 990))),
        "Pay yearly (saves 16 %)"
    );
    assert_eq!(
        crate::pay_words::yearly_label(Some(&tier(100, 1000))),
        "Pay yearly (saves 16 %)"
    );
    assert_eq!(
        crate::pay_words::yearly_label(Some(&tier(100, 600))),
        "Pay yearly (saves 50 %)"
    );
    // Not cheaper: no claim. A saving that rounds to nothing: no claim.
    assert_eq!(
        crate::pay_words::yearly_label(Some(&tier(100, 1200))),
        "Pay yearly"
    );
    assert_eq!(
        crate::pay_words::yearly_label(Some(&tier(100, 1500))),
        "Pay yearly"
    );
    assert_eq!(
        crate::pay_words::yearly_label(Some(&tier(10_000, 119_999))),
        "Pay yearly"
    );
    // A year in another currency than the month: nothing to compare.
    let mut mixed = tier(100, 1000);
    mixed.price_year = Some(cents(1000, "PLN"));
    assert_eq!(crate::pay_words::yearly_label(Some(&mixed)), "Pay yearly");
    assert_eq!(crate::pay_words::yearly_label(None), "Pay yearly");
    crate::l10n::in_english();
    azul_appkit::l10n::set_locale("de-DE");
    assert_eq!(
        crate::pay_words::yearly_label(Some(&tier(100, 1000))),
        "Jährlich bezahlen (spart 16 %)"
    );
    azul_appkit::l10n::set_locale("en-US");
}

/// AZ-P12: a renewal's toast says what was BOUGHT and that the first month is being added now -
/// the period grows one month at a time, as each token is redeemed.
#[test]
fn az_p12_the_toast_of_a_renewal_says_bought_not_added() {
    use azul_appkit::l10n::{t_phrase, Phrase};
    crate::l10n::in_english();
    let said = |months: u32| {
        t_phrase(
            &Phrase::new("azdrive-months-renewed")
                .arg("months", months)
                .arg("drive", "Photos"),
        )
    };
    assert_eq!(
        said(1),
        "1 month is bought for \"Photos\" and is being added now."
    );
    assert!(
        said(12).starts_with("12 months are bought for \"Photos\": the first is being added now"),
        "{}",
        said(12)
    );
    for text in [said(1), said(12)] {
        assert!(!text.contains("were added"), "{text}");
    }
    azul_appkit::l10n::set_locale("de-DE");
    assert!(said(12).contains("gekauft"), "{}", said(12));
    azul_appkit::l10n::set_locale("en-US");
}
