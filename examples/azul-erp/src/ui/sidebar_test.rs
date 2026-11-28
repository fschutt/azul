//! The sidebar's modules, counts and quick access, from the sample register.

use super::*;
use crate::{
    fixtures_test::{empty_erp, erp},
    store::Kind,
};

#[test]
fn the_asset_module_lists_the_pages_of_the_menu_with_their_counts() {
    let s = erp();
    let modules = modules(&s);
    assert_eq!(modules.len(), 1);
    let pages = &modules[0].pages;
    assert_eq!(pages.len(), s.state.views.menu.len());
    assert_eq!(pages.len(), 6);
    let counts: Vec<Option<usize>> = pages.iter().map(|p| p.count).collect();
    assert_eq!(
        counts,
        [
            Some(s.state.book.count(Kind::Asset)),
            Some(s.state.book.count(Kind::Category)),
            Some(s.state.book.count(Kind::Location)),
            Some(s.state.book.count(Kind::Maintenance)),
            Some(s.state.book.count(Kind::Checkout)),
            None
        ],
        "the reports are no list"
    );
    assert_eq!(pages[0].label, "Register");
    assert_eq!(pages[5].label, "Reports");
    assert!(modules[0].open);
}

#[test]
fn the_module_is_on_the_page_the_window_shows() {
    let mut s = erp();
    assert_eq!(modules(&s)[0].current, Some(0));
    s.state.open("/assets/locations");
    assert_eq!(modules(&s)[0].current, Some(2));
    s.state.open("/assets/reports");
    assert_eq!(modules(&s)[0].current, Some(5));
    s.state.open("/assets/import");
    assert_eq!(
        modules(&s)[0].current,
        Some(0),
        "the import starts from the register"
    );
}

#[test]
fn a_row_opens_its_page_and_a_tag_that_is_none_of_them_opens_nothing() {
    let mut s = erp();
    assert!(open_page(&mut s, nav::tag(0, 3)));
    assert_eq!(s.state.page, "/assets/maintenance");
    assert!(!open_page(&mut s, nav::tag(0, 40)));
    assert_eq!(s.state.page, "/assets/maintenance", "nothing changed");
}

#[test]
fn the_sample_has_overdue_work_and_an_empty_register_has_none() {
    let s = erp();
    let (checkouts, services) = overdue(&s);
    assert!(checkouts >= 1, "the sample has an overdue check-out");
    let _ = services;
    assert_eq!(overdue(&empty_erp()), (0, 0));
}

#[test]
fn the_rail_holds_what_the_page_can_run() {
    let mut s = erp();
    let register: Vec<nav::Quick> = nav::quick_plan(quick_input(&s))
        .iter()
        .map(|q| q.quick)
        .collect();
    assert_eq!(
        register,
        [
            nav::Quick::ImportCsv,
            nav::Quick::ExportCsv,
            nav::Quick::New
        ]
    );
    s.state.open("/assets/categories");
    let categories: Vec<nav::Quick> = nav::quick_plan(quick_input(&s))
        .iter()
        .map(|q| q.quick)
        .collect();
    assert_eq!(
        categories,
        [nav::Quick::New],
        "a category list can add and not import"
    );
    s.state.open("/assets/maintenance");
    assert!(
        nav::quick_plan(quick_input(&s)).is_empty(),
        "the log has nothing to run"
    );
    s.state.open("/accounting/assets");
    s.state.open("/accounting/assets/new");
    assert!(s.state.form.is_some());
    assert!(
        nav::quick_plan(quick_input(&s)).is_empty(),
        "a form open over the page: the rail leaves the page's commands alone"
    );
}

#[test]
fn a_quick_button_runs_the_pages_own_action() {
    let mut s = erp();
    run_quick(&mut s, nav::Quick::New);
    assert!(s.state.form.is_some(), "New opens the new-asset form");
    s.state.cancel_form();
    run_quick(&mut s, nav::Quick::ImportCsv);
    assert_eq!(s.state.page, crate::app::IMPORT);
    s.state.open("/accounting/assets");
    // the sample's own writes are taken first: what is queued after is the export
    let _ = s.state.queue.take();
    s.state.queue.finish(Vec::new());
    assert_eq!(s.state.queue.pending(), 0);
    run_quick(&mut s, nav::Quick::ExportCsv);
    assert_eq!(s.state.queue.pending(), 1, "the export is queued as a file");
}
