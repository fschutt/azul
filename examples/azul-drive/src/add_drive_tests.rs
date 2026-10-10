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
    add_drive::{
        country_from, country_of_locale, source_groups, AddDialog, AddPage, OfferState,
        TiersState, COUNTRIES,
    },
    sign_in::{self, PendingSignIn, SignInSettings, SignInStep},
};

fn tiers() -> Tiers {
    let tier = |id: &str, gb: u64, month: u64, year: u64| Tier {
        id: id.to_string(),
        quota_bytes: gb * 1_000_000_000,
        price_cents_month: Some(month),
        price_cents_year: Some(year),
        currency: "EUR".to_string(),
        first_month_free: true,
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
    let mut dialog = AddDialog::new(1);
    dialog.choose_buy();
    assert_eq!(dialog.page_line(), "buy");
    assert!(matches!(dialog.tiers, TiersState::NotLoaded));
    dialog.tiers = TiersState::Loaded(tiers());
    assert_eq!(dialog.chosen_tier().map(|t| t.id.as_str()), Some("100GB"));
    assert_eq!(dialog.buy_label(), "Buy 100 GB - EUR 0.99 a month");
    assert_eq!(dialog.months(), 1);
    dialog.yearly = true;
    dialog.tier = 1;
    assert_eq!(dialog.buy_label(), "Buy 1 TB - EUR 49.90 a year");
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
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(tiers());
    dialog.country = country.to_string();
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
fn the_pills_follow_the_country_and_the_period() {
    let mut dialog = paying_dialog("DE");
    assert!(dialog.pays_with_pills());
    assert_eq!(methods(&dialog), vec![Method::SepaDebit, Method::Card]);
    assert_eq!(dialog.chosen_pill().map(|p| p.method), Some(Method::SepaDebit));
    dialog.yearly = true;
    assert_eq!(
        methods(&dialog),
        vec![Method::SepaDebit, Method::Card, Method::PayPal],
        "PayPal for a prepaid year"
    );
    let mut us = paying_dialog("US");
    assert_eq!(methods(&us), vec![Method::Card]);
    assert_eq!(us.chosen_pill().map(|p| p.method), Some(Method::Card));
    us.yearly = true;
    assert_eq!(methods(&us), vec![Method::Card, Method::PayPal]);
}

#[test]
fn a_chosen_pill_stays_chosen_when_the_period_changes() {
    let mut dialog = paying_dialog("DE");
    dialog.choose_pill(Method::Card);
    assert_eq!(dialog.chosen_pill().map(|p| p.method), Some(Method::Card));
    dialog.yearly = true;
    assert_eq!(dialog.chosen_pill().map(|p| p.method), Some(Method::Card));
    dialog.choose_pill(Method::PayPal);
    dialog.yearly = false;
    assert_eq!(
        dialog.chosen_pill().map(|p| p.method),
        Some(Method::SepaDebit),
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
    assert_eq!(dialog.pills_line(), "sepa_debit:gocardless card:stripe");
}

#[test]
fn a_country_change_asks_for_the_options_again() {
    let mut dialog = paying_dialog("DE");
    assert!(dialog.set_country("FR"));
    assert_eq!(dialog.country, "FR");
    assert!(matches!(dialog.offer, OfferState::NotLoaded));
    assert!(!dialog.set_country("FR"), "the same country");
    assert!(!dialog.set_country("XX"), "not a country of the list");
    assert_eq!(dialog.country, "FR");
    assert!(COUNTRIES.iter().any(|(code, _)| *code == "DE"));
}

#[test]
fn the_country_comes_from_the_locale() {
    assert_eq!(country_of_locale("de_DE.UTF-8").as_deref(), Some("DE"));
    assert_eq!(country_of_locale("en_GB").as_deref(), Some("GB"));
    assert_eq!(country_of_locale("fr-FR").as_deref(), Some("FR"));
    assert_eq!(country_of_locale("nl_NL@euro").as_deref(), Some("NL"));
    assert_eq!(country_of_locale("C"), None);
    assert_eq!(country_of_locale("POSIX"), None);
    assert_eq!(country_of_locale("en_ZZ.UTF-8"), None);
    assert_eq!(country_of_locale(""), None);
}

#[test]
fn azlin_country_names_the_payers_country_before_the_locale() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_string())
        }
    };
    assert_eq!(
        country_from(env(&[("AZLIN_COUNTRY", "fr"), ("LANG", "de_DE.UTF-8")])),
        "FR"
    );
    assert_eq!(
        country_from(env(&[("AZLIN_COUNTRY", "XX"), ("LANG", "nl_NL.UTF-8")])),
        "NL",
        "a country not of the list is no choice"
    );
    assert_eq!(country_from(env(&[("LC_ALL", "C"), ("LANG", "en_GB.UTF-8")])), "GB");
    assert_eq!(country_from(env(&[])), "DE");
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
