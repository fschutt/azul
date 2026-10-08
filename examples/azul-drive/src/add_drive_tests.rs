//! The Add drive dialog as data: its pages, its forms, what Buy storage offers. No window.

use azcloud_kit::{Tier, Tiers};
use azul_storage::{
    catalog::ServiceGroup,
    config::{DriveAuth, DriveEntry, DriveLocation},
};

use crate::add_drive::{source_groups, AddDialog, AddPage, TiersState};

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
