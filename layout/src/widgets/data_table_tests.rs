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
