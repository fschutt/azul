//! What a window with one surface records of the menus an app opened: their ids, the control that
//! opens and closes its menu, the cap, and the menus a turn of the window closes.

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    menu::{Menu, MenuItem, StringMenuItem},
};

use super::{ExpandedSubmenus, MenuOverlays, MAX_OPEN_MENUS};

fn menu() -> Menu {
    Menu::create(vec![MenuItem::String(StringMenuItem::create("Copy".into()))].into())
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

fn phone() -> LogicalSize {
    LogicalSize::new(412.0, 915.0)
}

#[test]
fn a_menu_gets_an_id_and_closes_by_it() {
    let mut menus = MenuOverlays::new();
    assert!(menus.is_empty());
    let first = menus.open(&menu(), rect(10.0, 10.0, 100.0, 40.0), phone()).unwrap();
    let second = menus.open(&menu(), rect(10.0, 200.0, 100.0, 40.0), phone()).unwrap();
    assert_ne!(first, second, "every menu has its own id");
    assert_eq!(menus.len(), 2);
    assert_eq!(menus.get(first).map(|m| m.anchor), Some(rect(10.0, 10.0, 100.0, 40.0)));

    assert!(menus.close(first));
    assert!(!menus.close(first), "closed once");
    assert!(menus.get(first).is_none());
    assert_eq!(menus.len(), 1);

    // An id is never given twice in a window: the page that still names the old menu cannot reach
    // a new one.
    let third = menus.open(&menu(), rect(10.0, 10.0, 100.0, 40.0), phone()).unwrap();
    assert!(third > second);
}

/// The control a menu drops out of opens it and closes it again.
#[test]
fn the_same_control_opens_its_menu_and_a_second_tap_closes_it() {
    let mut menus = MenuOverlays::new();
    let control = rect(20.0, 100.0, 160.0, 40.0);
    assert!(menus.open(&menu(), control, phone()).is_some(), "the first tap opens");
    assert_eq!(menus.open(&menu(), control, phone()), None, "the second closes");
    assert!(menus.is_empty());
    assert!(menus.open(&menu(), control, phone()).is_some(), "the third opens again");
}

/// A point is no control: two context menus at one point are two menus, the newer on top.
#[test]
fn a_point_is_no_control_and_opens_a_menu_every_time() {
    let mut menus = MenuOverlays::new();
    let point = rect(50.0, 60.0, 0.0, 0.0);
    assert!(menus.open(&menu(), point, phone()).is_some());
    assert!(menus.open(&menu(), point, phone()).is_some());
    assert_eq!(menus.len(), 2);
}

/// An app that opens a menu on every frame fills no window: the oldest goes.
#[test]
fn a_window_shows_at_most_a_few_menus_and_the_oldest_goes() {
    let mut menus = MenuOverlays::new();
    let ids: Vec<u64> = (0..=MAX_OPEN_MENUS)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let y = i as f32 * 60.0;
            menus.open(&menu(), rect(0.0, y, 100.0, 40.0), phone()).unwrap()
        })
        .collect();
    assert_eq!(menus.len(), MAX_OPEN_MENUS);
    assert!(menus.get(ids[0]).is_none(), "the oldest was closed");
    assert!(menus.get(*ids.last().unwrap()).is_some(), "the newest is open");
}

/// A menu belongs to the shape of the window it was opened in.
#[test]
fn a_turn_of_the_window_closes_the_menus_opened_before_it() {
    let mut menus = MenuOverlays::new();
    let before = menus.open(&menu(), rect(10.0, 10.0, 100.0, 40.0), phone()).unwrap();
    let landscape = LogicalSize::new(915.0, 412.0);
    let after = menus.open(&menu(), rect(10.0, 100.0, 100.0, 40.0), landscape).unwrap();

    assert_eq!(menus.close_stale(landscape), vec![before]);
    assert!(menus.get(before).is_none());
    assert!(menus.get(after).is_some());
    assert!(menus.close_stale(landscape).is_empty(), "nothing is stale twice");
}

#[test]
fn a_submenu_unfolds_and_folds_by_its_path() {
    let mut folds = ExpandedSubmenus::default();
    assert!(!folds.is_open(&[2]));
    folds.toggle(&[2]);
    assert!(folds.is_open(&[2]));
    assert!(!folds.is_open(&[2, 0]), "the submenu of it is its own item");
    folds.toggle(&[2, 0]);
    assert!(folds.is_open(&[2, 0]));
    folds.toggle(&[2]);
    assert!(!folds.is_open(&[2]));
    assert!(folds.is_open(&[2, 0]), "folding one does not forget the other");
}
