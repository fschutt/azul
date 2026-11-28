//! The Preview: the selected record itself, what is behind its (?) buttons, what its banner says.

use super::*;
use crate::{
    fixtures_test::{empty_erp, erp},
    model::Status,
};

fn selected(s: &mut Erp, id: &str) {
    s.state.open("/accounting/assets");
    s.state.select(Kind::Asset, id);
}

#[test]
fn a_dragged_size_is_kept_only_where_it_is_a_size() {
    assert_eq!(clamped_size(320.0), 320.0);
    assert_eq!(clamped_size(0.0), 0.0);
    assert_eq!(clamped_size(-5.0), 0.0);
    assert_eq!(clamped_size(f32::NAN), 0.0);
    assert_eq!(clamped_size(f32::INFINITY), 0.0);
    assert_eq!(clamped_size(1.0e9), SIZE_MAX);
}

#[test]
fn the_preview_shows_on_a_table_page_with_the_records_in_and_no_form_open() {
    let mut s = erp();
    s.state.open("/accounting/assets");
    assert!(shown(&s));
    s.preview_open = false;
    assert!(!shown(&s), "View > Preview switched it off");
    s.preview_open = true;
    s.state.open("/accounting/assets/new");
    assert!(!shown(&s), "a form is the body");
    s.state.cancel_form();
    let id = s.state.book.assets[0].id.clone();
    s.state.open(&format!("/accounting/assets/{id}"));
    assert!(!shown(&s), "the record's own page is the body itself");
    s.state.open("/assets/reports");
    assert!(!shown(&s));
    let mut loading = empty_erp();
    loading.state.loaded = false;
    assert!(
        !shown(&loading),
        "nothing to show before the records are in"
    );
}

#[test]
fn an_assets_card_is_its_name_its_number_its_status_and_the_columns_facts() {
    let mut s = erp();
    let asset = s
        .state
        .book
        .assets
        .iter()
        .find(|a| a.status == Status::InUse)
        .unwrap()
        .clone();
    selected(&mut s, &asset.id);
    let card = card(&s.state).expect("a card");
    assert_eq!(card.kind, Kind::Asset);
    assert_eq!(card.id, asset.id);
    assert_eq!(card.title, asset.name, "the header's title_field");
    assert_eq!(card.subtitle, asset.number, "the header's subtitle_field");
    assert_eq!(
        card.status.as_deref(),
        Some("In use"),
        "the header's status_field"
    );
    let names: Vec<&str> = card.facts.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["Category", "Location", "Acquired", "Cost", "Book value"],
        "the table's other columns, the empty ones left out"
    );
    assert!(card.facts.iter().all(|(_, v)| !v.is_empty()));
}

#[test]
fn what_the_table_has_no_column_for_is_behind_the_first_button() {
    let mut s = erp();
    let id = s.state.book.assets[0].id.clone();
    selected(&mut s, &id);
    let card = card(&s.state).unwrap();
    let properties = &card.details[0];
    assert_eq!(properties.words, "Properties");
    assert_eq!(properties.icon, "info");
    let text = properties.lines.join("\n");
    assert!(text.contains("Useful life (years):"), "{text}");
    assert!(text.contains("Depreciation method:"), "{text}");
    assert!(
        !text.contains("Category:"),
        "the table has that column: {text}"
    );
}

#[test]
fn the_record_pages_tabs_are_buttons_of_their_own() {
    let mut s = erp();
    let id = s.state.book.assets[0].id.clone();
    selected(&mut s, &id);
    let card = card(&s.state).unwrap();
    let words: Vec<&str> = card.details.iter().map(|d| d.words.as_str()).collect();
    assert!(words.contains(&"Depreciation schedule"), "{words:?}");
    let schedule = card
        .details
        .iter()
        .find(|d| d.words == "Depreciation schedule")
        .unwrap();
    assert_eq!(schedule.icon, "show_chart");
    assert!(schedule.lines[0].contains(':'), "{:?}", schedule.lines);
    assert!(schedule.lines.len() <= MAX_LINES + 1);
}

#[test]
fn an_overdue_checkout_is_the_banner_and_its_details_are_behind_it() {
    let mut s = erp();
    let late = reports::overdue_checkouts(&s.state.book, s.state.today)
        .into_iter()
        .next()
        .expect("the sample has an overdue check-out");
    selected(&mut s, &late.asset);
    let card = card(&s.state).unwrap();
    let banner = card.banner.expect("a banner");
    assert_eq!(banner.tone, Tone::Warning);
    assert!(banner.line.starts_with("Overdue:"), "{}", banner.line);
    assert!(banner.line.contains(&late.custodian), "{}", banner.line);
    assert!(banner.details.contains("due on"), "{}", banner.details);
}

#[test]
fn a_category_has_a_card_without_a_status_or_a_banner() {
    let mut s = erp();
    s.state.open("/assets/categories");
    let id = s.state.book.categories[0].id.clone();
    s.state.select(Kind::Category, &id);
    let card = card(&s.state).expect("a card");
    assert_eq!(card.title, s.state.book.categories[0].name);
    assert_eq!(card.status, None);
    assert_eq!(card.banner, None);
    assert!(!card.facts.is_empty());
}

#[test]
fn with_no_selection_or_a_record_that_is_gone_there_is_no_card() {
    let mut s = erp();
    s.state.open("/accounting/assets");
    assert_eq!(card(&s.state), None);
    s.state.select(Kind::Asset, "no-such-record");
    assert_eq!(card(&s.state), None);
}

#[test]
fn a_long_list_behind_a_button_says_how_many_it_leaves_out() {
    let mut s = erp();
    let id = s.state.book.assets[0].id.clone();
    let before = s.state.book.maintenance.len();
    for n in 0..(MAX_LINES + 5) {
        s.state
            .book
            .maintenance
            .push(crate::model::MaintenanceEntry {
                id: format!("extra-{n}"),
                asset: id.clone(),
                date: s.state.today,
                kind: crate::model::MaintenanceKind::Service,
                description: format!("entry {n}"),
                cost: 100,
                by: String::new(),
            });
    }
    assert!(s.state.book.maintenance.len() > before);
    selected(&mut s, &id);
    let card = card(&s.state).unwrap();
    let log = card
        .details
        .iter()
        .find(|d| d.words == "Maintenance")
        .expect("the maintenance button");
    assert_eq!(
        log.lines.len(),
        MAX_LINES + 1,
        "the lines and the one that counts the rest"
    );
    assert!(
        log.lines.last().unwrap().contains("more"),
        "{:?}",
        log.lines.last()
    );
}
