//! The data table: its sort / filter model, the rows in view, the keys,
//! the pointer, editing, the order job's halves, the looks.

use std::sync::{Arc, Mutex};

use azul_core::{
    a11y::{AccessibilityRole, AccessibilityState},
    callbacks::Update,
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    refany::RefAny,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::VirtualKeyCode as K,
};
use azul_css::AzString;

use super::fixtures::{self, small};
use super::*;
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    },
};

type Log = Arc<Mutex<Vec<DataTableEvent>>>;

extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: DataTableEvent) -> Update {
    if let Some(log) = data.downcast_ref::<Log>() {
        log.lock().expect("log").push(event);
    }
    Update::RefreshDom
}

fn logged(log: &Log) -> DataTable {
    small().with_on_event(RefAny::new(log.clone()), record as DataTableOnEventCallbackType)
}

fn events(log: &Log) -> Vec<DataTableEvent> {
    log.lock().expect("log").clone()
}

fn id(n: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(n)),
    }
}

fn nodes_with(styled: &StyledDom, class: &str) -> Vec<NodeId> {
    styled
        .node_data
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, nd)| {
            nd.get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == class))
        })
        .map(|(i, _)| NodeId::new(i))
        .collect()
}

fn texts(node: &Dom, out: &mut Vec<String>) {
    if let NodeType::Text(s) = node.root.get_node_type() {
        if !s.as_ref().as_str().is_empty() {
            out.push(String::from(s.as_ref().as_str()));
        }
    }
    for c in node.children.as_ref() {
        texts(c, out);
    }
}

/// The plan of `view` over the fixture's columns, its keys read for every
/// row, the order computed - what the job does, in one go.
fn ordered(view: &DataTableView, rows: u32) -> Vec<u32> {
    let table = small().with_row_count(rows).with_view(view.clone());
    let plan = plan_of(&table.view, table.columns.as_slice());
    let mut keys = Vec::new();
    read_keys(&table.data_source, &plan, &mut keys, 0, rows);
    compute_order(rows, &plan, &keys)
}

fn name(row: u32) -> String {
    fold(fixtures::NAMES[(row % 7) as usize])
}

// ---- the sort / filter model ----

#[test]
fn a_header_click_sorts_ascending_then_descending_then_not_at_all() {
    use DataTableSortDirection::{Ascending, Descending};
    let mut v = DataTableView::create();
    let s0 = v.query_serial;
    v.click_sort(1, false);
    assert_eq!(v.sort.as_slice(), &[DataTableSortKey::create(1, Ascending)]);
    assert!(v.is_sorting(), "a new sort waits for its order");
    assert_ne!(v.query_serial, s0);
    v.click_sort(1, false);
    assert_eq!(v.sort.as_slice(), &[DataTableSortKey::create(1, Descending)]);
    v.click_sort(1, false);
    assert!(v.sort.is_empty(), "the third click unsorts");
    assert!(!v.is_sorting(), "no query left: every row in the app's order at once");
    assert!(!v.ordered);

    // Shift adds a key; a plain click makes one column the only key again.
    v.click_sort(3, false);
    v.click_sort(0, true);
    assert_eq!(
        v.sort.as_slice(),
        &[DataTableSortKey::create(3, Ascending), DataTableSortKey::create(0, Ascending)]
    );
    v.click_sort(0, true);
    assert_eq!(v.sort.as_slice()[1].direction, Descending, "Shift+click flips an added key");
    v.click_sort(0, true);
    assert_eq!(v.sort.as_slice(), &[DataTableSortKey::create(3, Ascending)], "and then takes it out");
    v.click_sort(3, true);
    v.click_sort(0, true);
    v.click_sort(3, false);
    assert_eq!(
        v.sort.as_slice(),
        &[DataTableSortKey::create(3, Ascending)],
        "a plain click on the primary key of two flips it and drops the rest"
    );
}

#[test]
fn a_typed_filter_reads_as_contains_equals_or_a_range() {
    let text = |s: &str| DataTableFilter::parse(0, DataTableSortKind::Text, AzString::from(s));
    let number = |s: &str| DataTableFilter::parse(1, DataTableSortKind::Number, AzString::from(s));

    let f = text("Ber");
    assert_eq!(f.op, DataTableFilterOp::Contains);
    assert_eq!(f.needle(), "ber");
    let f = text("=Paris");
    assert_eq!(f.op, DataTableFilterOp::Equals);
    assert_eq!(f.needle(), "paris");

    let f = number("12.5");
    assert_eq!(f.op, DataTableFilterOp::Equals);
    assert!(f.admits(12.5) && f.admits(12.54) && !f.admits(12.55) && !f.admits(12.4));
    let f = number("1,250");
    assert!(f.admits(1250.0) && f.admits(1250.4) && !f.admits(1251.0), "thousands separators read");
    let f = number("10..20");
    assert_eq!(f.op, DataTableFilterOp::Range);
    assert!(f.admits(10.0) && f.admits(20.0) && !f.admits(20.01) && !f.admits(9.99));
    let f = number(">5");
    assert!(!f.admits(5.0) && f.admits(5.01));
    let f = number(">=5");
    assert!(f.admits(5.0) && !f.admits(4.99));
    let f = number("<=-1");
    assert!(f.admits(-1.0) && !f.admits(0.0));
    let f = number("..3");
    assert!(f.admits(-1e9) && f.admits(3.0) && !f.admits(3.1));
    assert!(!f.admits(f64::NAN), "a blank never passes a range");
    let f = number("abc");
    assert_eq!(f.op, DataTableFilterOp::Contains, "not a number: the shown text must contain it");
    assert!(f.reads_text(DataTableSortKind::Number));
}

#[test]
fn a_date_filter_names_a_year_a_month_or_a_day() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2024, 1, 1), fixtures::NEW_YEAR_2024);
    assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    let date = |s: &str| DataTableFilter::parse(2, DataTableSortKind::Date, AzString::from(s));
    let d = |y: i64, m: u32, day: u32| days_from_civil(y, m, day) as f64;

    let f = date("2024-03-05");
    assert_eq!(f.op, DataTableFilterOp::Equals);
    assert!(f.admits(d(2024, 3, 5)) && f.admits(d(2024, 3, 5) + 0.75), "any time that day");
    assert!(!f.admits(d(2024, 3, 6)) && !f.admits(d(2024, 3, 4)));
    let f = date("2024-02");
    assert!(f.admits(d(2024, 2, 29)) && !f.admits(d(2024, 3, 1)), "a month is every day in it");
    let f = date("2024");
    assert!(f.admits(d(2024, 12, 31)) && !f.admits(d(2025, 1, 1)) && !f.admits(d(2023, 12, 31)));
    let f = date("2024-01..2024-03");
    assert!(f.admits(d(2024, 1, 1)) && f.admits(d(2024, 3, 31)) && !f.admits(d(2024, 4, 1)));
    let f = date(">2024-06");
    assert!(!f.admits(d(2024, 6, 30)) && f.admits(d(2024, 7, 1)), "after the whole month");
    let f = date("<2024-06");
    assert!(f.admits(d(2024, 5, 31)) && !f.admits(d(2024, 6, 1)));
    let f = date("2023-02-29");
    assert_eq!(f.op, DataTableFilterOp::Contains, "not a real date: a text filter");
}

#[test]
fn a_filter_set_twice_is_one_filter_and_an_empty_one_is_none() {
    let mut v = DataTableView::create();
    v.set_filter(3, DataTableSortKind::Text, AzString::from("ber"));
    v.set_filter(3, DataTableSortKind::Text, AzString::from("par"));
    assert_eq!(v.filters.len(), 1);
    assert_eq!(v.filter_text(3).as_str(), "par");
    let serial = v.query_serial;
    v.set_filter(3, DataTableSortKind::Text, AzString::from("par"));
    assert_eq!(v.query_serial, serial, "the same filter again is no new query");
    v.set_filter(3, DataTableSortKind::Text, AzString::from("  "));
    assert!(v.filters.is_empty());
    assert!(!v.is_sorting(), "no filter and no sort: nothing to wait for");
}

#[test]
fn the_keys_are_read_only_for_the_columns_the_query_needs() {
    let mut v = DataTableView::create();
    v.click_sort(1, false);
    v.set_filter(3, DataTableSortKind::Text, AzString::from("ber"));
    v.set_filter(1, DataTableSortKind::Number, AzString::from("abc"));
    let table = small().with_view(v);
    let plan = plan_of(&table.view, table.columns.as_slice());
    assert_eq!(
        plan.columns,
        vec![
            PlanColumn { column: 1, texts: true, values: true },
            PlanColumn { column: 3, texts: true, values: false },
        ],
        "Amount sorts by value and its filter reads text; City is filtered by text; nothing else is read"
    );
    let mut keys = Vec::new();
    read_keys(&table.data_source, &plan, &mut keys, 0, 10);
    read_keys(&table.data_source, &plan, &mut keys, 10, 25);
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].values.len(), 25, "two slices read 25 rows");
    assert!(keys[0].values[5].is_nan(), "row 5's amount is blank");
    assert_eq!(keys[1].texts[1], "paris");
    assert!(keys[1].values.is_empty(), "City's values are never asked for");

    let mut bad = DataTableView::create();
    bad.click_sort(99, false);
    let plan = plan_of(&bad, small().columns.as_slice());
    assert!(plan.is_empty(), "a key on a column that does not exist is left out");
}

#[test]
fn the_order_keeps_the_rows_that_pass_every_filter_sorted_by_every_key_stably() {
    let rows = 60;
    let mut v = DataTableView::create();
    v.click_sort(0, false); // Name, ascending, case folded
    v.set_filter(3, DataTableSortKind::Text, AzString::from("=paris"));
    let order = ordered(&v, rows);
    let expected: Vec<u32> = {
        let mut r: Vec<u32> = (0..rows).filter(|r| r % 3 == 1).collect();
        // Stable: equal names keep the app's order; the blank name last.
        r.sort_by(|a, b| {
            let (na, nb) = (name(*a), name(*b));
            match (na.is_empty(), nb.is_empty()) {
                (true, false) => core::cmp::Ordering::Greater,
                (false, true) => core::cmp::Ordering::Less,
                _ => na.cmp(&nb),
            }
        });
        r
    };
    assert_eq!(order, expected);
    assert!(order.iter().all(|r| r % 3 == 1), "only Paris");
    let firsts: Vec<String> = order.iter().take(3).map(|r| name(*r)).collect();
    assert_eq!(firsts, ["alpha", "alpha", "alpha"], "alpha and Alpha fold together");
    assert!(name(*order.last().expect("rows")).is_empty(), "the blank name sorts last");

    // A second key orders the rows the first leaves equal.
    let mut v2 = DataTableView::create();
    v2.click_sort(0, false);
    v2.click_sort(1, true);
    v2.click_sort(1, true); // Amount descending
    let order = ordered(&v2, rows);
    for pair in order.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if name(a) == name(b) {
            let (x, y) = (fixtures::amount(a), fixtures::amount(b));
            assert!(
                y.is_nan() || (!x.is_nan() && x >= y),
                "rows {a} and {b}: equal names, then the amounts descending (blanks last)"
            );
        }
    }
}

#[test]
fn blanks_sort_last_in_either_direction() {
    let rows = 40;
    for clicks in [1, 2] {
        let mut v = DataTableView::create();
        for _ in 0..clicks {
            v.click_sort(1, false);
        }
        let order = ordered(&v, rows);
        assert_eq!(order.len(), rows as usize);
        let blanks: Vec<u32> = (0..rows).filter(|r| fixtures::amount(*r).is_nan()).collect();
        let tail = &order[order.len() - blanks.len()..];
        assert_eq!(tail, &blanks[..], "the blank amounts, in the app's order, at the end");
        let values: Vec<f64> = order[..order.len() - blanks.len()]
            .iter()
            .map(|r| fixtures::amount(*r))
            .collect();
        let sorted = values.windows(2).all(|w| if clicks == 1 { w[0] <= w[1] } else { w[0] >= w[1] });
        assert!(sorted, "{clicks} click(s): {values:?}");
    }
}

#[test]
fn a_new_query_waits_for_its_order_and_an_older_order_is_dropped() {
    let mut v = DataTableView::create();
    v.click_sort(1, false);
    let first = v.query_serial;
    v.click_sort(1, false);
    let second = v.query_serial;
    let stale = v.clone().with_order(first, vec![3, 2, 1]);
    assert!(stale.is_sorting() && !stale.ordered, "an order for an older query is dropped");
    let fresh = v.with_order(second, vec![3, 2, 1]);
    assert!(!fresh.is_sorting() && fresh.ordered);
    assert_eq!(fresh.shown_count(1000), 3);
    assert_eq!(fresh.row_at(0, 1000).into_option(), Some(3));
    assert_eq!(fresh.position_of(1, 1000).into_option(), Some(2));
    assert_eq!(fresh.position_of(7, 1000).into_option(), None, "filtered out");
}

#[test]
fn the_selection_keeps_only_the_rows_a_filter_still_shows() {
    let mut v = DataTableView::create();
    v.selection.select_keys(U64Vec::from_vec(vec![1, 4, 7]));
    v.set_filter(3, DataTableSortKind::Text, AzString::from("paris"));
    let serial = v.query_serial;
    let v = v.with_order(serial, vec![1, 4, 10]);
    assert_eq!(v.selection.keys.as_slice(), &[1, 4]);
}

/// The brief's yardstick: half a million rows, sorted by a number column
/// and filtered by a text column, in one pass of the job's thread half -
/// timed loosely (a debug build on a slow runner is far below the bound).
#[test]
fn half_a_million_rows_sort_and_filter_in_well_under_the_bound() {
    let rows: u32 = 500_000;
    let mut v = DataTableView::create();
    v.click_sort(1, false);
    v.click_sort(1, false); // descending
    v.set_filter(3, DataTableSortKind::Text, AzString::from("rom"));
    let table = small().with_row_count(rows).with_view(v);
    let plan = plan_of(&table.view, table.columns.as_slice());
    // The keys as the job's slices would have read them (no callback here:
    // the thread half sorts plain keys).
    let keys = vec![
        ColumnKeys {
            texts: Vec::new(),
            values: (0..rows).map(fixtures::amount).collect(),
        },
        ColumnKeys {
            texts: (0..rows).map(|r| fold(fixtures::CITIES[(r % 3) as usize])).collect(),
            values: Vec::new(),
        },
    ];
    let started = std::time::Instant::now();
    let order = compute_order(rows, &plan, &keys);
    let took = started.elapsed();
    assert_eq!(order.len(), (0..rows).filter(|r| r % 3 == 2).count(), "only Rome");
    let values: Vec<f64> = order.iter().map(|r| fixtures::amount(*r)).filter(|x| !x.is_nan()).collect();
    assert!(values.windows(2).all(|w| w[0] >= w[1]), "descending");
    assert!(took.as_secs() < 20, "500k rows took {took:?}");
}

// ---- the window of rows, the looks, the accessibility tree ----

fn geo_of(t: &DataTable) -> Geometry {
    geometry(t)
}

fn indices(bands: &[crate::widgets::cell_grid::Band]) -> Vec<u32> {
    bands.iter().map(|b| b.index).collect()
}

/// The fixture: 400 x 300 px, a 30 px header, a 26 px filter row, 26 px
/// rows, five columns 490 px wide - both scroll bars.
#[test]
fn only_the_rows_and_columns_in_view_are_built() {
    let t = small();
    let geo = geo_of(&t);
    assert_eq!(geo.body_top, 56.0);
    assert_eq!(geo.body_width, 388.0, "the vertical bar takes 12 px");
    assert_eq!(geo.body_bottom, 288.0, "the horizontal bar takes 12 px");
    assert_eq!(indices(&geo.rows), (0..9).collect::<Vec<_>>(), "8 whole rows and one cut");
    assert_eq!(geo.page_rows, 8);
    assert_eq!(indices(&geo.columns), vec![0, 1, 2, 3], "City is cut at the edge");
    assert_eq!(geo.max_top, 1000 - 8);
    assert_eq!(geo.max_left, 1, "from Amount on, the rest fits");
    assert!(geo.vbar.is_some() && geo.hbar.is_some());

    let styled = StyledDom::create_from_dom(small().with_theme(UiTheme::Flat).dom());
    assert_eq!(nodes_with(&styled, CELL_CLASS_NAME).len(), 9 * 4, "only the window is in the DOM");
    assert_eq!(nodes_with(&styled, HEADER_CLASS_NAME).len(), 4);
    assert_eq!(nodes_with(&styled, FILTER_CLASS_NAME).len(), 4);

    // Half a million rows build the same window.
    let big = small().with_row_count(500_000);
    let styled = StyledDom::create_from_dom(big.with_theme(UiTheme::Flat).dom());
    assert_eq!(nodes_with(&styled, CELL_CLASS_NAME).len(), 9 * 4);
}

#[test]
fn a_scrolled_table_shows_its_rows_from_top_and_never_past_the_last_page() {
    let mut v = DataTableView::create();
    v.top = 500;
    let dom = small().with_view(v.clone()).with_theme(UiTheme::Flat).dom();
    // Child 0 the header row, 1 the filter row, 2 the first row in view.
    let first = &dom.children.as_ref()[2];
    let info = first.root.get_accessibility_info().expect("a row role");
    assert_eq!(info.role, AccessibilityRole::Row);
    assert_eq!(info.row_index.into_option(), Some(501), "its place in the WHOLE table");

    v.top = 999_999;
    let geo = geo_of(&small().with_view(v));
    assert_eq!(geo.top, 992, "the last page, not past it");
    assert_eq!(geo.rows.first().map(|b| b.index), Some(992));
}

#[test]
fn the_table_is_one_focus_stop_with_the_grid_role_and_its_cells_carry_their_place() {
    let dom = small().with_theme(UiTheme::Flat).dom();
    let info = dom.root.get_accessibility_info().expect("a role");
    assert_eq!(info.role, AccessibilityRole::Grid);
    assert_eq!(info.accessibility_name.as_ref().map(|n| n.as_str()), Some("Orders"));
    assert_eq!(dom.root.get_tab_index(), Some(azul_core::dom::TabIndex::Auto));
    let header = &dom.children.as_ref()[0].children.as_ref()[1];
    let h = header.root.get_accessibility_info().expect("a header role");
    assert_eq!(h.role, AccessibilityRole::ColumnHeader);
    assert_eq!(h.accessibility_name.as_ref().map(|n| n.as_str()), Some("Amount"));
    let filter = &dom.children.as_ref()[1].children.as_ref()[3];
    let f = filter.root.get_accessibility_info().expect("a filter role");
    assert_eq!(f.accessibility_name.as_ref().map(|n| n.as_str()), Some("Filter City"));
    // Row 4 (child 2 + 4): its Amount cell, named by the column, its text
    // as its value.
    let cell = &dom.children.as_ref()[6].children.as_ref()[1];
    let c = cell.root.get_accessibility_info().expect("a cell role");
    assert_eq!(c.role, AccessibilityRole::GridCell);
    assert_eq!(c.accessibility_name.as_ref().map(|n| n.as_str()), Some("Amount"));
    assert_eq!(
        c.accessibility_value.as_ref().map(|v| v.as_str().to_string()),
        Some(format!("{:.2}", fixtures::amount(4)))
    );
    assert_eq!(c.row_index.into_option(), Some(5));
    assert_eq!(c.column_index.into_option(), Some(2));
}

#[test]
fn a_header_click_sorts_a_small_table_at_once_and_the_header_says_so() {
    let t = small();
    let geo = geo_of(&t);
    let e = press(&t, &geo, Hit::Header(1), false, false, (0.0, 0.0)).expect("a sortable header");
    assert_eq!(e.kind, DataTableEventKind::Sort);
    assert_eq!(e.index, 1);
    assert!(e.view.is_sorting(), "the press asks; the handler brings the order");
    let plan = plan_of(&e.view, t.columns.as_slice());
    let view = order_now(&t, e.view, &plan);
    assert!(!view.is_sorting() && view.ordered);
    assert_eq!(view.shown_count(1000), 1000);
    let first = view.row_at(0, 1000).into_option().expect("a row");
    assert_eq!(fixtures::amount(first), 0.0, "the smallest amount first");

    let dom = small().with_view(view).with_theme(UiTheme::Flat).dom();
    let header = &dom.children.as_ref()[0].children.as_ref()[1];
    let h = header.root.get_accessibility_info().expect("a header role");
    assert_eq!(h.states.as_ref(), &[AccessibilityState::SortedAscending], "aria-sort");
    let mut seen = Vec::new();
    texts(header, &mut seen);
    assert_eq!(seen, vec!["Amount \u{25B2}".to_string()]);
    let other = &dom.children.as_ref()[0].children.as_ref()[0];
    assert!(other.root.get_accessibility_info().expect("a role").states.as_ref().is_empty());
}

#[test]
fn a_big_table_sorts_off_the_ui_thread_and_says_so_meanwhile() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    // A filter being typed on 100,000 rows: Backspace changes it.
    let mut v = DataTableView::create();
    v.set_filter(3, DataTableSortKind::Text, AzString::from("par"));
    let v = start_filter_edit(&v, 3).view;
    let big = logged(&log).with_row_count(100_000).with_view(v);
    let styled = StyledDom::create_from_dom(big.with_theme(UiTheme::Flat).dom());
    let table = nodes_with(&styled, TABLE_CLASS_NAME)[0];
    let (_, changes) = rv::press(&styled, id(table), K::Back, &[]).expect("the table hears keys");
    let events = events(&log);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DataTableEventKind::Filter);
    assert_eq!(events[0].view.filter_text(3).as_str(), "pa");
    assert!(events[0].view.is_sorting(), "100,000 rows: the order comes later");
    assert!(
        changes.iter().any(|c| matches!(c, crate::callbacks::CallbackChange::AddTimer { .. })),
        "the job's timer reads the keys in slices"
    );
    let dom = small()
        .with_row_count(100_000)
        .with_view(events[0].view.clone())
        .with_theme(UiTheme::Flat)
        .dom();
    let mut seen = Vec::new();
    texts(&dom, &mut seen);
    assert!(seen.iter().any(|t| t == "Sorting 100,000 rows..."), "{seen:?}");
    let info = dom.root.get_accessibility_info().expect("a role");
    assert!(info.states.as_ref().contains(&AccessibilityState::Busy));
}

#[test]
fn a_typed_filter_on_a_small_table_shows_its_rows_at_once() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let mut v = DataTableView::create();
    v.set_filter(3, DataTableSortKind::Text, AzString::from("par"));
    let v = start_filter_edit(&v, 3).view;
    let styled = StyledDom::create_from_dom(logged(&log).with_view(v).with_theme(UiTheme::Flat).dom());
    let table = nodes_with(&styled, TABLE_CLASS_NAME)[0];
    let (update, changes) = rv::press(&styled, id(table), K::Back, &[]).expect("the table hears keys");
    assert_eq!(update, Update::RefreshDom);
    assert!(rv::prevented(&changes));
    let events = events(&log);
    let view = &events[0].view;
    assert!(!view.is_sorting(), "1,000 rows are filtered in the handler");
    assert_eq!(view.shown_count(1000), 333, "Paris: every third row");
    assert!((0..333).all(|p| view.row_at(p, 1000).into_option().is_some_and(|r| r % 3 == 1)));
    assert_eq!(view.edit, DataTableEditTarget::Filter, "the filter is still being typed");
}

#[test]
fn keys_on_the_table_node_reach_the_app_with_the_next_view() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let styled = StyledDom::create_from_dom(logged(&log).with_theme(UiTheme::Flat).dom());
    let table = nodes_with(&styled, TABLE_CLASS_NAME)[0];
    let (update, changes) = rv::press(&styled, id(table), K::Down, &[]).expect("the table hears keys");
    assert_eq!(update, Update::RefreshDom, "the app's answer is forwarded");
    assert!(rv::prevented(&changes), "the arrow is the table's");
    let events = events(&log);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DataTableEventKind::Select);
    assert_eq!(events[0].view.cursor_row().into_option(), Some(0), "the first row");
    let (_, changes) = rv::press(&styled, id(table), K::Q, &[]).expect("the handler runs");
    assert!(!rv::prevented(&changes), "a letter is typed text, not a key the table takes");
}

#[test]
fn the_arrows_pages_and_home_end_move_the_cursor_and_shift_extends_the_selection() {
    let mut t = small();
    let key = |t: &DataTable, k: K, shift: bool, ctrl: bool| {
        let geo = geo_of(t);
        table_key(t, &geo, k, shift, ctrl).expect("a key the table takes").view
    };
    t.view = key(&t, K::Down, false, false);
    t.view = key(&t, K::Down, false, false);
    assert_eq!(t.view.cursor_row().into_option(), Some(1));
    t.view = key(&t, K::Down, true, false);
    t.view = key(&t, K::Down, true, false);
    assert_eq!(t.view.selection.keys.as_slice(), &[1, 2, 3], "Shift extends from the anchor");
    t.view = key(&t, K::Down, false, true);
    assert_eq!(t.view.cursor_row().into_option(), Some(4), "Ctrl moves the cursor alone");
    assert_eq!(t.view.selection.keys.as_slice(), &[1, 2, 3]);
    t.view = key(&t, K::PageDown, false, false);
    assert_eq!(t.view.cursor_row().into_option(), Some(12), "a page is 8 rows");
    t.view = key(&t, K::End, false, true);
    assert_eq!(t.view.cursor_row().into_option(), Some(999), "Ctrl+End: the last row");
    assert_eq!(t.view.top, 992, "scrolled to show it");
    t.view = key(&t, K::Home, false, true);
    assert_eq!(t.view.cursor_row().into_option(), Some(0));
    assert_eq!(t.view.top, 0);
    t.view = key(&t, K::End, false, false);
    assert_eq!(t.view.active_column, 4, "End: the last column");
    assert_eq!(t.view.left_column, 1, "scrolled right to show it");
    t.view = key(&t, K::Left, false, false);
    assert_eq!(t.view.active_column, 3);
    t.view = key(&t, K::A, false, true);
    assert_eq!(t.view.selection.len(), 1000, "Ctrl+A: every row shown");
}

#[test]
fn the_keys_walk_the_rows_in_the_order_they_show() {
    let mut t = small();
    let mut v = DataTableView::create();
    v.click_sort(1, false);
    v.click_sort(1, false); // Amount descending
    let plan = plan_of(&v, t.columns.as_slice());
    t.view = order_now(&t, v, &plan);
    let geo = geo_of(&t);
    let top = t.view.row_at(0, 1000).into_option().expect("a row");
    let second = t.view.row_at(1, 1000).into_option().expect("a row");
    t.view = table_key(&t, &geo, K::Down, false, false).expect("down").view;
    assert_eq!(t.view.cursor_row().into_option(), Some(top), "Down starts at the first row SHOWN");
    t.view = table_key(&t, &geo, K::Down, true, false).expect("shift down").view;
    let mut want = vec![u64::from(top), u64::from(second)];
    want.sort_unstable();
    assert_eq!(t.view.selection.keys.as_slice(), &want[..], "Shift takes the rows between, as shown");
}

#[test]
fn a_click_selects_a_row_shift_extends_and_ctrl_toggles() {
    let t = small();
    let v = click_select(&t, &t.view, 2, false, false);
    assert_eq!(v.selection.keys.as_slice(), &[2]);
    let v = click_select(&t, &v, 5, true, false);
    assert_eq!(v.selection.keys.as_slice(), &[2, 3, 4, 5]);
    let v = click_select(&t, &v, 3, false, true);
    assert_eq!(v.selection.keys.as_slice(), &[2, 4, 5]);
}

#[test]
fn f2_edits_the_cursor_cell_and_enter_keeps_the_edit_through_on_edit() {
    // F2 on an editable cell opens it with its text.
    let mut t = small();
    let geo = geo_of(&t);
    t.view = table_key(&t, &geo, K::Down, false, false).expect("down").view;
    t.view.active_column = 1;
    let e = table_key(&t, &geo, K::F2, false, false).expect("F2 edits");
    assert_eq!(e.kind, DataTableEventKind::EditStart);
    assert_eq!(e.view.edit, DataTableEditTarget::Cell);
    assert_eq!(e.view.edit_text.as_str(), format!("{:.2}", fixtures::amount(0)));
    assert_eq!(e.cell, DataTableCellRef::create(0, 1));
    // F2 on a read-only column does nothing; Enter opens the row.
    t.view.active_column = 0;
    assert!(table_key(&t, &geo, K::F2, false, false).is_none());
    let e = table_key(&t, &geo, K::Return, false, false).expect("Enter activates");
    assert_eq!(e.kind, DataTableEventKind::Activate);
    assert_eq!(e.cell, DataTableCellRef::create(0, 0));
}

extern "C" fn refuse_negative(_: RefAny, _: CallbackInfo, edit: DataTableEdit) -> DataTableEditResult {
    if edit.text.as_str().starts_with('-') {
        DataTableEditResult::create_refused(AzString::from("An amount is never negative."))
    } else {
        DataTableEditResult::create_accepted()
    }
}

#[test]
fn the_app_validates_an_edit_and_a_refused_one_stays_open() {
    let editing = |text: &str| {
        let e = start_cell_edit(&DataTableView::create(), 3, 1, text);
        e.view
    };
    for (text, kept) in [("-5", false), ("12.50", true)] {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let t = logged(&log)
            .with_on_edit(RefAny::new(()), refuse_negative as DataTableOnEditCallbackType)
            .with_view(editing(text));
        let styled = StyledDom::create_from_dom(t.with_theme(UiTheme::Flat).dom());
        let table = nodes_with(&styled, TABLE_CLASS_NAME)[0];
        rv::press(&styled, id(table), K::Return, &[]).expect("the table hears keys");
        let events = events(&log);
        assert_eq!(events.len(), 1);
        if kept {
            assert_eq!(events[0].kind, DataTableEventKind::EditCommit);
            assert_eq!(events[0].text.as_str(), "12.50");
            assert_eq!(events[0].cell, DataTableCellRef::create(3, 1));
            assert!(!events[0].view.is_editing());
        } else {
            assert_eq!(events[0].kind, DataTableEventKind::EditRefused);
            assert_eq!(events[0].text.as_str(), "An amount is never negative.");
            assert!(events[0].view.is_editing(), "the edit stays open to be fixed");
        }
    }
}

#[test]
fn escape_cancels_an_edit_and_the_caret_keys_edit_the_line() {
    let t = small().with_view(start_cell_edit(&DataTableView::create(), 3, 1, "12.5").view);
    let e = cell_edit_key(&t, K::Back, false).expect("a caret key").expect("an edit");
    assert_eq!(e.view.edit_text.as_str(), "12.");
    let e = cell_edit_key(&t, K::Home, false).expect("a caret key").expect("an edit");
    assert_eq!(e.view.edit_cursor, 0);
    let e = cell_edit_key(&t, K::Escape, false).expect("Escape").expect("a cancel");
    assert_eq!(e.kind, DataTableEventKind::EditCancel);
    assert!(!e.view.is_editing());
    assert_eq!(cell_edit_key(&t, K::Tab, true), Some(Err(-1)), "Shift+Tab keeps it and moves left");
    assert_eq!(line_edit("abc", 1, K::Delete), Some((String::from("ac"), 1)));
    assert_eq!(line_edit("abc", 3, K::Right), Some((String::from("abc"), 3)));
    assert_eq!(line_edit("abc", 0, K::Back), Some((String::from("abc"), 0)));
    assert_eq!(line_edit("abc", 1, K::Q), None);
}

#[test]
fn an_edit_shows_its_text_and_caret_over_the_cell() {
    let t = small().with_view(start_cell_edit(&DataTableView::create(), 2, 1, "7.25").view);
    let styled = StyledDom::create_from_dom(t.with_theme(UiTheme::Flat).dom());
    assert_eq!(nodes_with(&styled, EDITOR_CLASS_NAME).len(), 1);
    assert_eq!(nodes_with(&styled, CARET_CLASS_NAME).len(), 1);
    assert_eq!(nodes_with(&styled, CURSOR_CLASS_NAME).len(), 0, "the editor stands for the cursor");
}

#[test]
fn a_point_hits_a_header_its_edge_a_filter_a_cell_or_a_scroll_bar() {
    let geo = geo_of(&small());
    assert_eq!(hit_test(&geo, 50.0, 10.0), Hit::Header(0));
    assert_eq!(hit_test(&geo, 118.5, 10.0), Hit::HeaderEdge(0));
    assert_eq!(hit_test(&geo, 121.0, 10.0), Hit::HeaderEdge(0), "the grip reaches into the next column");
    assert_eq!(hit_test(&geo, 150.0, 40.0), Hit::Filter(1));
    assert_eq!(hit_test(&geo, 150.0, 56.0 + 26.0 * 2.0 + 3.0), Hit::Cell(2, 1));
    assert_eq!(hit_test(&geo, 393.0, 60.0), Hit::RowsThumb, "the thumb sits at the top");
    assert_eq!(hit_test(&geo, 393.0, 280.0), Hit::RowsTrack(true));
    assert_eq!(hit_test(&geo, 395.0, 295.0), Hit::Nothing, "the corner between the bars");
}

#[test]
fn the_wheel_and_the_track_scroll_whole_rows_and_a_thumb_drag_reaches_the_end() {
    let t = small();
    let geo = geo_of(&t);
    let v = scroll_by(&t, &geo, 3, 0);
    assert_eq!(v.top, 3);
    let v = scroll_by(&t, &geo, -10, 5);
    assert_eq!((v.top, v.left_column), (0, 1), "kept in range");
    let e = press(&t, &geo, Hit::RowsTrack(true), false, false, (0.0, 0.0)).expect("a page");
    assert_eq!(e.view.top, 8);

    let grab = press(&t, &geo, Hit::RowsThumb, false, false, (393.0, 60.0)).expect("a drag");
    assert_eq!(grab.view.drag.kind, DataTableDragKind::ScrollRows);
    let dragging = small().with_view(grab.view);
    let e = drag_move(&dragging, &geo, None, (393.0, 400.0)).expect("a scroll");
    assert_eq!(e.view.top, geo.max_top, "dragged past the end: the last page");
    let end = drag_end(&small().with_view(e.view)).expect("a release");
    assert_eq!(end.view.drag.kind, DataTableDragKind::None);
}

#[test]
fn a_resize_drag_previews_the_width_and_the_release_keeps_it_in_the_view() {
    let t = small();
    let geo = geo_of(&t);
    let grab = press(&t, &geo, Hit::HeaderEdge(0), false, false, (120.0, 10.0)).expect("a grip");
    let t = small().with_view(grab.view);
    let e = drag_move(&t, &geo, None, (170.0, 10.0)).expect("a wider column");
    assert_eq!(e.view.drag.size, 170.0);
    let preview = small().with_view(e.view.clone());
    assert_eq!(geo_of(&preview).columns[0].size, 170.0, "drawn at the dragged width");
    let end = drag_end(&preview).expect("a release");
    assert_eq!(end.kind, DataTableEventKind::ResizeColumn);
    assert_eq!((end.index, end.size), (0, 170.0));
    assert_eq!(geo_of(&small().with_view(end.view)).columns[0].size, 170.0, "kept");
    let narrow = drag_move(&t, &geo, None, (0.0, 10.0)).expect("narrower");
    assert_eq!(narrow.view.drag.size, MIN_COLUMN_PX);
}

#[test]
fn a_double_click_edits_an_editable_cell_and_opens_any_other_row() {
    let t = small();
    let e = double_click(&t, Hit::Cell(4, 2)).expect("an edit");
    assert_eq!(e.kind, DataTableEventKind::EditStart);
    assert_eq!((e.view.edit_row, e.view.edit_column), (4, 2));
    let e = double_click(&t, Hit::Cell(4, 0)).expect("an activation");
    assert_eq!(e.kind, DataTableEventKind::Activate);
    assert!(double_click(&small().with_read_only(true), Hit::Cell(4, 2))
        .is_some_and(|e| e.kind == DataTableEventKind::Activate), "a read-only table never edits");
}

#[test]
fn copy_puts_the_titles_and_the_selected_rows_in_their_shown_order() {
    let mut t = small();
    t.view = click_select(&t, &t.view, 2, false, false);
    t.view = click_select(&t, &t.view, 3, true, false);
    let rows = copied_rows(&t);
    assert_eq!(rows.len(), 3, "the titles and two rows");
    assert_eq!(rows[0], vec!["Name", "Amount", "Date", "City", "Code"]);
    assert_eq!(rows[1][4], "C0002");
    assert_eq!(rows[2][4], "C0003");
    assert!(copied_rows(&small()).is_empty(), "nothing selected, no cursor: nothing");
}

#[test]
fn a_filter_that_leaves_nothing_says_so() {
    let mut v = DataTableView::create();
    v.set_filter(3, DataTableSortKind::Text, AzString::from("=nowhere"));
    let plan = plan_of(&v, small().columns.as_slice());
    let v = order_now(&small(), v, &plan);
    assert_eq!(v.shown_count(1000), 0);
    let dom = small().with_view(v).with_theme(UiTheme::Flat).dom();
    let mut seen = Vec::new();
    texts(&dom, &mut seen);
    assert!(seen.iter().any(|t| t == "No rows match the filters"), "{seen:?}");
    assert!(geo_of(&small()).vbar.is_some());
}

// ---- rows that change: the order follows them ----

/// The app's rows in the tests of a table whose rows change: one amount
/// each, in column 1 ("Amount", a Number column); column 0 names the row.
struct Amounts(Vec<f64>);

extern "C" fn amount_cells(mut data: RefAny, at: DataTableCellRef) -> DataTableCell {
    let Some(rows) = data.downcast_ref::<Amounts>() else {
        return DataTableCell::empty();
    };
    match (at.column, rows.0.get(at.row as usize)) {
        (0, Some(_)) => DataTableCell::create_text(AzString::from(format!("R{}", at.row))),
        (1, Some(v)) => DataTableCell::create(AzString::from(format!("{v:.2}")), *v),
        _ => DataTableCell::empty(),
    }
}

/// A table over `amounts` (every row of it in view) showing `view`.
fn amounts_table(amounts: &[f64], view: DataTableView) -> DataTable {
    let columns = DataTableColumnVec::from_vec(vec![
        DataTableColumn::create(AzString::from_const_str("Row"), 80.0, DataTableSortKind::Text),
        DataTableColumn::create(AzString::from_const_str("Amount"), 90.0, DataTableSortKind::Number),
    ]);
    DataTable::create(columns, u32::try_from(amounts.len()).expect("a few rows"))
        .with_viewport(400.0, 300.0)
        .with_data_source(
            RefAny::new(Amounts(amounts.to_vec())),
            amount_cells as DataTableDataSourceCallbackType,
        )
        .with_view(view)
}

/// The app's rows the table shows as it is built, top to bottom.
fn shown_rows(t: DataTable) -> Vec<u32> {
    resolve(t).rows.iter().filter_map(|r| r.row).collect()
}

/// The order `view`'s query makes of `amounts`, as the handlers make it.
fn ordered_over(amounts: &[f64], view: DataTableView) -> DataTableView {
    let t = amounts_table(amounts, view);
    let plan = plan_of(&t.view, t.columns.as_slice());
    order_now(&t, t.view.clone(), &plan)
}

/// A view sorted by the amount (column 1), `direction`.
fn by_amount(direction: DataTableSortDirection) -> DataTableView {
    let mut v = DataTableView::create();
    v.set_sort(DataTableSortKeyVec::from_vec(vec![DataTableSortKey::create(1, direction)]));
    v
}

/// A refresh brings two rows and changes a value: the app says its rows
/// changed (`invalidate_order`) and the table's next build shows every row
/// in the order the view's sort makes of the NEW rows - not the old order,
/// which knew four rows and left the new ones out.
#[test]
fn rows_that_change_are_shown_in_the_order_the_views_sort_makes_of_them() {
    let first = [5.0, 1.0, 7.0, 3.0];
    let refreshed = [5.0, 9.0, 7.0, 3.0, 8.0, 0.5];
    let sorted = ordered_over(&first, by_amount(DataTableSortDirection::Descending));
    assert_eq!(sorted.order.as_slice(), &[2, 0, 3, 1], "7, 5, 3, 1");

    let mut view = sorted;
    view.invalidate_order();
    assert!(view.is_sorting(), "the order was made of other rows");
    assert_eq!(
        shown_rows(amounts_table(&refreshed, view)),
        vec![1, 4, 2, 0, 3, 5],
        "9, 8, 7, 5, 3, 0.5: every row, by the sort the view keeps"
    );

    let mut up = ordered_over(&first, by_amount(DataTableSortDirection::Ascending));
    up.invalidate_order();
    assert_eq!(
        shown_rows(amounts_table(&refreshed, up)),
        vec![5, 3, 0, 2, 4, 1],
        "0.5, 3, 5, 7, 8, 9: ascending stays ascending"
    );
}

/// A filter is a query too: the rows that pass it are found again among
/// the new rows.
#[test]
fn a_filter_is_applied_again_to_rows_that_change() {
    let mut v = DataTableView::create();
    v.set_filter(1, DataTableSortKind::Number, AzString::from(">4"));
    let mut view = ordered_over(&[5.0, 1.0, 7.0, 3.0], v);
    assert_eq!(view.order.as_slice(), &[0, 2]);
    view.invalidate_order();
    assert_eq!(
        shown_rows(amounts_table(&[5.0, 9.0, 7.0, 3.0, 8.0, 0.5], view)),
        vec![0, 1, 2, 4],
        "the rows over 4, in the app's order"
    );
}

/// A refresh is not a new query: the scroll position, the selection and
/// the sort stay where the user left them, and the old order shows until
/// the new one is made. A view without a sort or a filter shows the app's
/// order, which is always current: nothing to wait for.
#[test]
fn invalidating_the_order_keeps_the_scroll_position_the_selection_and_the_sort() {
    let mut v = ordered_over(&[5.0, 1.0, 7.0, 3.0], by_amount(DataTableSortDirection::Descending));
    v.top = 2;
    v.selection.select_keys(U64Vec::from_vec(vec![0, 3]));
    let before = v.clone();
    v.invalidate_order();
    assert_eq!(v.top, before.top, "the scroll position stays");
    assert_eq!(v.selection, before.selection, "the selection stays");
    assert_eq!(v.sort, before.sort, "the sort stays");
    assert_eq!(v.order, before.order, "the old order shows until the new one is made");
    assert_ne!(v.query_serial, before.query_serial, "an order job for the old rows is dropped");

    let mut plain = DataTableView::create();
    plain.invalidate_order();
    assert!(!plain.is_sorting() && !plain.ordered, "the app's order: nothing to wait for");
}

/// The app sets a sort itself (a saved view, its own "Sort by amount"
/// button) without starting the query: a table of up to
/// `DATA_TABLE_SYNC_ROWS` rows shows that order at its next build, as a
/// header click would have - never "Sorting..." over the app's order.
#[test]
fn a_small_table_shows_a_sort_the_app_set_itself_at_its_next_build() {
    let view = by_amount(DataTableSortDirection::Descending);
    assert!(view.is_sorting(), "nothing ordered it yet");
    let resolved = resolve(small().with_view(view.clone()));
    assert!(!resolved.table.view.is_sorting(), "ordered at the build");
    let first = resolved.rows[0].row.expect("a row in view");
    assert_eq!(fixtures::amount(first), 25.0, "the biggest amount first");
    let dom = small().with_view(view).with_theme(UiTheme::Flat).dom();
    let mut seen = Vec::new();
    texts(&dom, &mut seen);
    assert!(!seen.iter().any(|t| t.starts_with("Sorting")), "{seen:?}");
}

/// An app's callback that starts the query of the table it carries.
extern "C" fn query_now(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(t) = data.downcast_ref::<DataTable>() {
        t.start_query(&mut info);
    }
    Update::DoNothing
}

/// A table of more rows than a build orders: after the app's rows changed
/// it shows the old order and says it is sorting, and the app's
/// `start_query` (in any of its callbacks) starts the job that reads the
/// new rows' keys - before `invalidate_order` it had nothing to start.
#[test]
fn a_big_table_whose_rows_changed_is_ordered_again_through_start_query() {
    let rows: u32 = 100_000;
    let view = by_amount(DataTableSortDirection::Ascending);
    let serial = view.query_serial;
    let mut view = view.with_order(serial, (0..rows).collect());
    assert!(!view.is_sorting());
    view.invalidate_order();
    let big = small().with_row_count(rows).with_view(view);
    let mut seen = Vec::new();
    texts(&big.clone().with_theme(UiTheme::Flat).dom(), &mut seen);
    assert!(seen.iter().any(|t| t == "Sorting 100,000 rows..."), "{seen:?}");

    let caller = Dom::create_div().with_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        RefAny::new(big),
        azul_core::callbacks::CoreCallback {
            cb: query_now as usize,
            ctx: azul_core::refany::OptionRefAny::None,
        },
    );
    let styled = StyledDom::create_from_dom(caller);
    let click = EventFilter::Hover(HoverEventFilter::Click);
    let (_, changes) = rv::fire(&styled, id(NodeId::new(0)), click).expect("the app's callback runs");
    assert!(
        changes.iter().any(|c| matches!(c, crate::callbacks::CallbackChange::AddTimer { .. })),
        "the order job's timer reads the new rows' keys"
    );
}

#[test]
fn a_table_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
    checks::assert_follows_the_app_theme(
        "data_table",
        || small().dom(),
        |t: UiTheme| small().with_theme(t).dom(),
    );
    for theme in checks::BOTH {
        let dom = checks::under(theme, || small().dom());
        theme_checks::assert_structure_is_shared(&format!("data_table built for {}", theme.name()), &dom, &[]);
    }
}
