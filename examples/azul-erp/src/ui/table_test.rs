//! The table: a phone's columns, what the first build selects, what the selection and the Preview
//! do when the table scrolls, is clicked, is sorted.

use azul::widgets::{
    DataTableCellRef, DataTableEvent, DataTableEventKind, DataTableView, ListSelection,
};

use super::*;
use crate::{fixtures_test::erp, store::Kind};

/// The app on the register page with the table's rows as a build leaves them.
fn on_register() -> Erp {
    let mut s = erp();
    s.state.open("/accounting/assets");
    s.table_kind = Some(Kind::Asset);
    s.row_ids = s.state.book.assets.iter().map(|a| a.id.clone()).collect();
    s.row_paths = s
        .row_ids
        .iter()
        .map(|id| format!("/accounting/assets/{id}"))
        .collect();
    s
}

fn event(kind: DataTableEventKind, view: DataTableView) -> DataTableEvent {
    DataTableEvent::create(kind, view)
}

fn view_at(top: u32, row: Option<u64>) -> DataTableView {
    let mut view = DataTableView::create();
    view.top = top;
    if let Some(row) = row {
        view.selection = ListSelection::create().apply(row, false, false);
    }
    view
}

#[test]
fn a_desktop_keeps_every_column_and_a_phone_the_first_two_and_the_last() {
    let s = erp();
    let view = s.state.views.view("assets_fixed_asset_list").unwrap();
    let desktop = columns_for(view, &s.state.labels, false, 1000.0);
    assert_eq!(desktop.len(), 8);
    let phone = columns_for(view, &s.state.labels, true, 342.0);
    let titles: Vec<&str> = phone.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Asset no.", "Name", "Status"]);
    let total: f32 = phone.iter().map(|c| c.width).sum();
    assert!(
        (total - 330.0).abs() < 0.5,
        "the columns fill the width less the scroll bar: {total}"
    );
    assert!(phone.iter().all(|c| c.width >= nav::MIN_COLUMN_PX));
}

#[test]
fn the_first_build_selects_the_row_at_the_top_edge() {
    let mut s = on_register();
    assert_eq!(s.state.selected, None);
    sync_selection(&mut s, Kind::Asset);
    let first = s.row_ids[0].clone();
    assert_eq!(s.state.selected.as_ref().map(|x| x.id.clone()), Some(first));
    assert_eq!(
        s.table.selection.focus.into_option(),
        Some(0),
        "the table highlights it"
    );
}

#[test]
fn a_record_that_left_the_rows_lets_the_top_row_take_over() {
    let mut s = on_register();
    s.state.select(Kind::Asset, "gone");
    sync_selection(&mut s, Kind::Asset);
    assert_eq!(
        s.state.selected.as_ref().map(|x| x.id.clone()),
        Some(s.row_ids[0].clone())
    );
}

#[test]
fn a_selection_that_is_on_a_row_stays_where_it_is() {
    let mut s = on_register();
    let third = s.row_ids[2].clone();
    s.state.select(Kind::Asset, &third);
    sync_selection(&mut s, Kind::Asset);
    assert_eq!(s.state.selected.as_ref().map(|x| x.id.clone()), Some(third));
    assert_eq!(s.table.selection.focus.into_option(), Some(2));
}

#[test]
fn scrolling_moves_the_selection_to_the_top_row_and_a_click_holds_it() {
    let mut s = on_register();
    sync_selection(&mut s, Kind::Asset);
    // the reader scrolls five rows
    apply_event(
        &mut s,
        &event(DataTableEventKind::Scroll, view_at(5, None)),
        false,
    );
    assert_eq!(
        s.state.selected.as_ref().map(|x| x.id.clone()),
        Some(s.row_ids[5].clone())
    );
    assert_eq!(
        s.table.selection.focus.into_option(),
        Some(5),
        "the table shows it selected"
    );
    // a click on row 7 holds the selection there
    apply_event(
        &mut s,
        &event(DataTableEventKind::Select, view_at(5, Some(7))),
        false,
    );
    assert_eq!(
        s.state.selected.as_ref().map(|x| x.id.clone()),
        Some(s.row_ids[7].clone())
    );
    // a nudge of the top edge by one is the table keeping the row in view: the click still holds
    apply_event(
        &mut s,
        &event(DataTableEventKind::Scroll, view_at(6, Some(7))),
        false,
    );
    assert_eq!(
        s.state.selected.as_ref().map(|x| x.id.clone()),
        Some(s.row_ids[7].clone())
    );
    // scrolled on: the Preview follows the edge again
    apply_event(
        &mut s,
        &event(DataTableEventKind::Scroll, view_at(9, Some(7))),
        false,
    );
    assert_eq!(
        s.state.selected.as_ref().map(|x| x.id.clone()),
        Some(s.row_ids[9].clone())
    );
}

#[test]
fn sorting_or_filtering_drops_the_selection_so_the_top_row_takes_the_preview() {
    let mut s = on_register();
    sync_selection(&mut s, Kind::Asset);
    s.state.select(Kind::Asset, &s.row_ids[4].clone());
    apply_event(
        &mut s,
        &event(DataTableEventKind::Sort, view_at(0, None)),
        false,
    );
    assert_eq!(s.state.selected, None);
    assert_eq!(s.follow, crate::follow::Follow::new());
}

#[test]
fn an_activated_row_opens_its_record_page() {
    let mut s = on_register();
    let mut e = event(DataTableEventKind::Activate, view_at(0, Some(3)));
    e.cell = DataTableCellRef::create(3, 0);
    apply_event(&mut s, &e, false);
    assert_eq!(s.state.page, format!("/accounting/assets/{}", s.row_ids[3]));
}

#[test]
fn a_long_press_on_a_phone_selects_the_row_and_asks_for_the_sheet() {
    let mut s = on_register();
    let mut e = event(DataTableEventKind::ContextMenu, view_at(0, None));
    e.cell = DataTableCellRef::create(2, 0);
    apply_event(&mut s, &e, false);
    assert!(!s.sheet_open, "a desktop's right click is not the sheet");
    apply_event(&mut s, &e, true);
    assert!(s.sheet_open);
    assert_eq!(
        s.state.selected.as_ref().map(|x| x.id.clone()),
        Some(s.row_ids[2].clone())
    );
    // a long press on empty space asks for nothing
    s.sheet_open = false;
    let mut none = event(DataTableEventKind::ContextMenu, view_at(0, None));
    none.cell = DataTableCellRef::create(u32::MAX, 0);
    apply_event(&mut s, &none, true);
    assert!(!s.sheet_open);
}

#[test]
fn a_table_that_lists_nothing_has_no_events_to_hear() {
    let mut s = erp();
    s.table_kind = None;
    apply_event(
        &mut s,
        &event(DataTableEventKind::Select, view_at(0, Some(1))),
        false,
    );
    assert_eq!(s.state.selected, None);
}
