//! The ribbon's commands: what the views name, for the page and for the record.

use super::*;
use crate::{fixtures_test::erp, store::Kind};

fn labels(commands: &[(ActionSpec, Params)]) -> Vec<String> {
    commands.iter().map(|(a, _)| a.label.clone()).collect()
}

#[test]
fn the_registers_own_commands_are_the_views_actions_in_order() {
    let mut s = erp();
    s.state.open("/accounting/assets");
    assert_eq!(
        labels(&page_commands(&s)),
        ["New asset", "Import CSV", "Export CSV", "Depreciation run"]
    );
    s.state.open("/assets/categories");
    assert_eq!(labels(&page_commands(&s)), ["New category"]);
    s.state.open("/assets/maintenance");
    assert!(
        page_commands(&s).is_empty(),
        "the log has no command of its own"
    );
}

#[test]
fn a_form_open_over_the_page_offers_save_and_cancel() {
    let mut s = erp();
    s.state.open("/accounting/assets/new");
    assert_eq!(labels(&page_commands(&s)), ["Save", "Cancel"]);
}

#[test]
fn with_no_record_selected_the_ribbon_has_no_record_commands() {
    let mut s = erp();
    s.state.open("/accounting/assets");
    assert!(record_commands(&s).is_empty());
}

#[test]
fn a_selected_asset_offers_open_and_what_its_page_offers_where_the_conditions_hold() {
    let mut s = erp();
    s.state.open("/accounting/assets");
    // an asset in use: it can be checked out, not in
    let free = s
        .state
        .book
        .assets
        .iter()
        .find(|a| {
            a.status == crate::model::Status::InUse && s.state.book.open_checkout(&a.id).is_none()
        })
        .expect("the sample has an asset in use")
        .id
        .clone();
    s.state.select(Kind::Asset, &free);
    let commands = record_commands(&s);
    let names = labels(&commands);
    assert_eq!(names[0], "Open");
    assert!(names.contains(&String::from("Edit")), "{names:?}");
    assert!(names.contains(&String::from("Check out")), "{names:?}");
    assert!(!names.contains(&String::from("Check in")), "{names:?}");
    assert!(
        names.contains(&String::from("Log maintenance")),
        "{names:?}"
    );
    assert!(names.contains(&String::from("Dispose")), "{names:?}");
    assert_eq!(
        names.last().map(String::as_str),
        Some("Delete"),
        "Delete is the last"
    );
    // every command of the record runs on the record
    assert!(commands
        .iter()
        .skip(1)
        .all(|(_, params)| params.get("id") == Some(&free)));
    // Open leads to the record's page
    assert_eq!(
        commands[0].0.kind,
        ActionKind::Link(format!("/accounting/assets/{free}"))
    );
}

#[test]
fn a_checked_out_asset_offers_check_in_and_not_check_out() {
    let mut s = erp();
    s.state.open("/accounting/assets");
    let out = s
        .state
        .book
        .assets
        .iter()
        .find(|a| a.status == crate::model::Status::CheckedOut)
        .expect("the sample has an asset checked out")
        .id
        .clone();
    s.state.select(Kind::Asset, &out);
    let names = labels(&record_commands(&s));
    assert!(names.contains(&String::from("Check in")), "{names:?}");
    assert!(!names.contains(&String::from("Check out")), "{names:?}");
}

#[test]
fn on_the_records_own_page_the_commands_are_its_header_actions() {
    let mut s = erp();
    let id = s.state.book.assets[0].id.clone();
    s.state.open(&format!("/accounting/assets/{id}"));
    let names = labels(&record_commands(&s));
    assert!(!names.contains(&String::from("Open")), "{names:?}");
    assert!(names.contains(&String::from("Edit")), "{names:?}");
    assert!(
        page_commands(&s).is_empty(),
        "the detail view has no list actions"
    );
}

#[test]
fn a_category_row_opens_its_edit_form() {
    let mut s = erp();
    s.state.open("/assets/categories");
    let id = s.state.book.categories[0].id.clone();
    s.state.select(Kind::Category, &id);
    let commands = record_commands(&s);
    assert_eq!(commands.len(), 1, "a category has no page of its own");
    assert_eq!(
        commands[0].0.kind,
        ActionKind::Link(format!("/assets/categories/{id}/edit"))
    );
}

#[test]
fn the_reports_and_the_run_are_found_by_what_their_views_are() {
    let s = erp();
    assert_eq!(
        path_of(&s, ViewKind::Report).as_deref(),
        Some("/assets/reports")
    );
    assert_eq!(
        path_of(&s, ViewKind::Wizard).as_deref(),
        Some("/assets/depreciation-runs/new")
    );
    assert_eq!(path_of(&s, ViewKind::Dashboard), None);
}
