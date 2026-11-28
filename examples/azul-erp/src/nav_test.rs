//! The sidebar's plan, the quick access, the sheet's order and a phone's columns.

use super::*;

fn page(path: &str, label: &str, count: Option<usize>) -> Page {
    Page {
        path: path.to_string(),
        label: label.to_string(),
        icon: "article",
        count,
    }
}

fn assets() -> Module {
    Module {
        title: String::from("Assets"),
        pages: vec![
            page("/accounting/assets", "Register", Some(12)),
            page("/assets/categories", "Categories", Some(5)),
            page("/assets/locations", "Locations", Some(0)),
            page("/assets/reports", "Reports", None),
        ],
        current: Some(1),
        open: true,
    }
}

#[test]
fn a_module_is_a_section_and_its_pages_are_the_rows_with_their_counts() {
    let sections = plan(&[assets()]);
    assert_eq!(sections.len(), 1);
    let s = &sections[0];
    assert_eq!(s.title, "Assets");
    assert!(s.open);
    let rows: Vec<(&str, &str)> = s
        .rows
        .iter()
        .map(|r| (r.label.as_str(), r.badge.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Register", "12"),
            ("Categories", "5"),
            ("Locations", ""),
            ("Reports", "")
        ],
        "no badge for none or zero"
    );
    assert_eq!(
        s.rows.iter().map(|r| r.selected).collect::<Vec<_>>(),
        [false, true, false, false],
        "the page the window shows is the selected row"
    );
}

#[test]
fn a_rows_tag_names_its_module_and_its_page() {
    let two = vec![assets(), assets()];
    let sections = plan(&two);
    assert_eq!(sections[1].rows[2].tag, tag(1, 2));
    assert_eq!(target(tag(1, 2)), (1, 2));
    assert_eq!(target(tag(0, 0)), (0, 0));
    assert_eq!(target(tag(7, 98)), (7, 98));
    assert_eq!(sections[0].rows[0].tag, 0);
}

#[test]
fn a_count_reads_as_a_badge() {
    assert_eq!(badge(None), "");
    assert_eq!(badge(Some(0)), "");
    assert_eq!(badge(Some(1)), "1");
    assert_eq!(badge(Some(999)), "999");
    assert_eq!(badge(Some(1000)), "999+");
    assert_eq!(badge(Some(usize::MAX)), "999+");
}

#[test]
fn more_pages_or_modules_than_the_tags_hold_are_dropped_not_crashed_on() {
    let mut module = assets();
    module.pages = (0..250)
        .map(|i| page(&format!("/p/{i}"), "x", None))
        .collect();
    let sections = plan(&[module.clone()]);
    assert_eq!(sections[0].rows.len(), MAX_PAGES);
    assert!(sections[0]
        .rows
        .iter()
        .all(|r| target(r.tag).0 == 0 && target(r.tag).1 < TAG_STRIDE));
    let many: Vec<Module> = (0..150).map(|_| module.clone()).collect();
    assert_eq!(plan(&many).len(), MAX_MODULES);
}

#[test]
fn the_menu_keys_have_their_icons_and_an_unknown_key_a_plain_page() {
    assert_eq!(page_icon("menu.fixed_assets"), "inventory_2");
    assert_eq!(page_icon("menu.asset_reports"), "bar_chart");
    assert_eq!(page_icon("menu.something_new"), "article");
    assert_eq!(page_icon(""), "article");
}

#[test]
fn the_current_page_is_the_longest_menu_path_the_window_is_under() {
    let paths = [
        "/accounting/assets",
        "/assets/categories",
        "/assets/locations",
    ];
    assert_eq!(current_page(&paths, "/accounting/assets"), Some(0));
    assert_eq!(
        current_page(&paths, "/accounting/assets/6f1c"),
        Some(0),
        "a record of the register"
    );
    assert_eq!(current_page(&paths, "/assets/categories/new"), Some(1));
    assert_eq!(current_page(&paths, "/assets/locations/x/edit"), Some(2));
    // the pages the register opens belong to it
    assert_eq!(current_page(&paths, "/assets/import"), Some(0));
    assert_eq!(
        current_page(&paths, "/assets/depreciation-runs/new"),
        Some(0)
    );
    assert_eq!(current_page(&paths, ""), Some(0));
    assert_eq!(current_page(&[], "/accounting/assets"), None);
    // a path that only starts with the same letters is not under it
    assert_eq!(current_page(&["/a", "/ab"], "/abc"), Some(0));
    assert_eq!(current_page(&["/a", "/ab"], "/ab/c"), Some(1));
    let nested = ["/assets", "/assets/reports"];
    assert_eq!(
        current_page(&nested, "/assets/reports"),
        Some(1),
        "the longer one wins"
    );
    assert_eq!(current_page(&nested, "/assets/other"), Some(0));
}

#[test]
fn the_rail_holds_what_the_page_can_run_with_the_most_used_button_last() {
    let all = quick_plan(QuickInput {
        has_new: true,
        has_import: true,
        has_export: true,
    });
    assert_eq!(
        all.iter().map(|q| q.quick).collect::<Vec<_>>(),
        [Quick::ImportCsv, Quick::ExportCsv, Quick::New],
        "New is last: nearest the thumb"
    );
    let only_new = quick_plan(QuickInput {
        has_new: true,
        ..QuickInput::default()
    });
    assert_eq!(
        only_new.iter().map(|q| q.quick).collect::<Vec<_>>(),
        [Quick::New]
    );
    assert!(
        quick_plan(QuickInput::default()).is_empty(),
        "a page with nothing to run has no quick access"
    );
}

#[test]
fn a_quick_buttons_tag_comes_back_as_the_button() {
    for quick in [Quick::ImportCsv, Quick::ExportCsv, Quick::New] {
        assert_eq!(quick_of(quick_tag(quick)), Some(quick));
    }
    assert_eq!(quick_of(0), None);
    assert_eq!(quick_of(99), None);
}

#[test]
fn the_sheet_lists_what_destroys_last() {
    assert_eq!(sheet_order(&[false, false, true, false]), [0, 1, 3, 2]);
    assert_eq!(sheet_order(&[true, true, false]), [2, 0, 1]);
    assert_eq!(sheet_order(&[]), Vec::<usize>::new());
    assert_eq!(sheet_order(&[false, false]), [0, 1]);
}

#[test]
fn a_phone_keeps_the_first_two_columns_and_the_last() {
    assert_eq!(phone_columns(8), [0, 1, 7]);
    assert_eq!(phone_columns(4), [0, 1, 3]);
    assert_eq!(phone_columns(3), [0, 1, 2]);
    assert_eq!(phone_columns(2), [0, 1]);
    assert_eq!(phone_columns(1), [0]);
    assert_eq!(phone_columns(0), Vec::<usize>::new());
}

#[test]
fn columns_are_scaled_to_fill_the_room_in_proportion() {
    let fitted = fit_widths(&[100.0, 200.0, 100.0], 400.0, 40.0);
    assert_eq!(fitted, [100.0, 200.0, 100.0]);
    let grown = fit_widths(&[100.0, 200.0, 100.0], 800.0, 40.0);
    assert_eq!(grown, [200.0, 400.0, 200.0]);
    let shrunk = fit_widths(&[96.0, 220.0, 110.0], 342.0, 72.0);
    let sum: f32 = shrunk.iter().sum();
    assert!((sum - 342.0).abs() < 0.01, "{shrunk:?}");
    assert!(shrunk.iter().all(|w| *w >= 72.0), "{shrunk:?}");
}

#[test]
fn a_column_never_gets_under_the_minimum_and_the_others_share_the_rest() {
    let fitted = fit_widths(&[10.0, 500.0, 500.0], 300.0, 72.0);
    assert_eq!(fitted[0], 72.0);
    assert!(
        (fitted[1] - 114.0).abs() < 0.01 && (fitted[2] - 114.0).abs() < 0.01,
        "{fitted:?}"
    );
    // no room for the minimum: every column keeps it and the table scrolls sideways
    let tight = fit_widths(&[100.0, 100.0, 100.0], 100.0, 72.0);
    assert_eq!(tight, [72.0, 72.0, 72.0]);
}

#[test]
fn widths_that_are_no_numbers_still_make_columns() {
    let fitted = fit_widths(&[f32::NAN, -3.0, f32::INFINITY, 0.0], 400.0, 72.0);
    assert_eq!(fitted.len(), 4);
    assert!(
        fitted.iter().all(|w| w.is_finite() && *w >= 72.0),
        "{fitted:?}"
    );
    assert!(fit_widths(&[], 400.0, 72.0).is_empty());
    let nan_room = fit_widths(&[100.0, 100.0], f32::NAN, 72.0);
    assert_eq!(nan_room, [72.0, 72.0]);
}
