//! The record the Preview shows: chosen, kept while the page stays, dropped when the page changes.

use super::*;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()
}

fn sampled() -> State {
    let mut s = State::new(today());
    s.load(&[]);
    let mut n = 0;
    let mut mint = move || {
        n += 1;
        format!("00000000-0000-4000-8000-{n:012}")
    };
    s.seed_sample(&mut mint);
    s
}

#[test]
fn a_new_state_shows_no_record() {
    let s = State::new(today());
    assert_eq!(s.selected, None);
    assert_eq!(s.selected_asset(), None);
}

#[test]
fn choosing_a_row_shows_its_record() {
    let mut s = sampled();
    let id = s.book.assets[0].id.clone();
    s.select(Kind::Asset, &id);
    assert_eq!(
        s.selected,
        Some(Selected {
            kind: Kind::Asset,
            id: id.clone()
        })
    );
    assert_eq!(s.selected_asset(), Some(id.as_str()));
    s.select(Kind::Category, &s.book.categories[0].id.clone());
    assert_eq!(s.selected_asset(), None, "a category is no asset");
    s.clear_selection();
    assert_eq!(s.selected, None);
}

#[test]
fn another_page_shows_another_list_so_the_selection_goes() {
    let mut s = sampled();
    let id = s.book.assets[0].id.clone();
    s.select(Kind::Asset, &id);
    s.open("/assets/categories");
    assert_eq!(
        s.selected, None,
        "the categories' list starts with its own row"
    );
    s.select(Kind::Category, &s.book.categories[0].id.clone());
    s.go_back();
    assert_eq!(
        s.selected, None,
        "back to the register: its first row again"
    );
}

#[test]
fn deleting_an_asset_drops_the_selection() {
    let mut s = sampled();
    let id = s.book.assets[0].id.clone();
    s.select(Kind::Asset, &id);
    s.delete_asset(&id);
    assert_eq!(s.selected, None);
}

#[test]
fn a_form_opened_over_the_page_keeps_the_selection_of_the_page() {
    let mut s = sampled();
    let id = s.book.assets[0].id.clone();
    s.select(Kind::Asset, &id);
    s.open("/accounting/assets/new");
    assert!(s.form.is_some());
    assert_eq!(s.selected_asset(), Some(id.as_str()));
}

#[test]
fn the_book_counts_its_records_by_kind() {
    let s = sampled();
    assert_eq!(s.book.count(Kind::Asset), s.book.assets.len());
    assert_eq!(s.book.count(Kind::Category), s.book.categories.len());
    assert_eq!(s.book.count(Kind::Location), s.book.locations.len());
    assert_eq!(s.book.count(Kind::Maintenance), s.book.maintenance.len());
    assert_eq!(s.book.count(Kind::Checkout), s.book.checkouts.len());
    assert!(s.book.count(Kind::Asset) > 0);
    assert_eq!(State::new(today()).book.count(Kind::Asset), 0);
}
