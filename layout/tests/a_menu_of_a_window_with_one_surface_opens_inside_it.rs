//! A menu an app opens on a window with ONE surface (a phone) opens inside the window.
//!
//! A desktop shows a `Menu` as a native menu or a window of its own; a phone cannot. The engine
//! records the menu, the Dom pass puts a popup with its rows at the end of the page, and from there
//! the menu is an overlay like every other popup of the window: placed below the control it dropped
//! from (above it where there is no room below), closed by a press outside it, by Escape and by a
//! turn of the window.
#![cfg(feature = "widgets")]

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    menu::{Menu, MenuItem, StringMenuItem},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    transient::WindowPresentation,
    transient_reconcile::PopupShown,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const PHONE: LogicalSize = LogicalSize {
    width: 412.0,
    height: 915.0,
};

fn window(size: LogicalSize) -> (LayoutWindow, FullWindowState) {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.transient_windows
        .set_shell_presentation(WindowPresentation::OneSurface);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = size;
    lw.current_window_state = ws.clone();
    (lw, ws)
}

/// The page: a control `160 x 40` whose top left is at (20, `top`).
fn page(top: f32) -> Dom {
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("control".into())].into())
            .with_css(&format!(
                "width: 160px; height: 40px; margin-left: 20px; margin-top: {top}px;"
            )),
    )
}

fn control_rect(top: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(20.0, top), LogicalSize::new(160.0, 40.0))
}

fn menu(labels: &[&str]) -> Menu {
    Menu::create(
        labels
            .iter()
            .map(|l| MenuItem::String(StringMenuItem::create((*l).into())))
            .collect::<Vec<_>>()
            .into(),
    )
}

/// What a shell's `regenerate_layout` does, as long as the layout asks for another pass: the app's
/// DOM is made again and goes through the Dom pass (which adds the menus that are open), is laid
/// out, and the popups are reconciled.
fn show(lw: &mut LayoutWindow, ws: &FullWindowState, app: &Dom) {
    for _ in 0..4 {
        let mut dom = app.clone();
        let _ = lw.present_transient_windows(&mut dom, ws);
        let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        lw.layout_and_generate_display_list(
            styled,
            ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut Some(Vec::new()),
        )
        .unwrap();
        lw.reconcile_transients(ws, &mut || Vec::new());
        if !lw.transient_overlays_changed() && !lw.transient_docks_changed() {
            return;
        }
    }
    panic!("the layout never settled: {:?}", lw.transient_windows.open_windows());
}

fn rect_of_node(lw: &LayoutWindow, node: NodeId) -> Option<LogicalRect> {
    lw.get_node_rect_in_viewport(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
}

fn the_menu_popup(lw: &LayoutWindow) -> azul_layout::transient_reconcile::PopupInfo {
    let popups = lw.open_popups();
    assert_eq!(popups.len(), 1, "one menu is open: {popups:?}");
    popups.into_iter().next().unwrap()
}

/// THE picture: the menu of a drop-down is below its control, left edges lined up, at least as wide
/// as the control, and each of its rows is a finger high.
#[test]
fn a_menu_opens_below_its_control_inside_the_window() {
    let (mut lw, ws) = window(PHONE);
    let dom = page(100.0);
    show(&mut lw, &ws, &dom);
    assert!(lw.open_popups().is_empty(), "no menu yet");

    assert!(lw.open_menu_overlay(&menu(&["Copy", "Paste", "Delete"]), control_rect(100.0), PHONE));
    show(&mut lw, &ws, &dom);

    let popup = the_menu_popup(&lw);
    assert!(popup.is_menu, "it is a menu, not a popup the app made");
    assert_eq!(popup.shown, PopupShown::Overlay, "inside the window, not a window of its own");
    assert!(popup.top);
    let rect = popup.rect.expect("an overlay has a box");
    assert!((rect.origin.x - 20.0).abs() < 1.0, "left edges lined up with the control: {rect:?}");
    assert!(
        (rect.origin.y - 140.0).abs() < 1.0,
        "right under the control (100 + 40): {rect:?}"
    );
    assert!(rect.size.width >= 160.0, "at least as wide as the control: {rect:?}");
    assert!(rect.size.height >= 3.0 * 48.0, "three rows, each a finger high: {rect:?}");
    assert!(
        rect.origin.y + rect.size.height <= PHONE.height,
        "inside the window: {rect:?}"
    );
    assert_eq!(rect_of_node(&lw, popup.node), Some(rect), "laid out where the manager put it");
}

/// A control at the bottom of the window has no room below: its menu opens above it.
#[test]
fn a_menu_flips_above_a_control_at_the_bottom_of_the_window() {
    let (mut lw, ws) = window(PHONE);
    let top = 860.0;
    let dom = page(top);
    show(&mut lw, &ws, &dom);
    assert!(lw.open_menu_overlay(&menu(&["One", "Two", "Three"]), control_rect(top), PHONE));
    show(&mut lw, &ws, &dom);

    let rect = the_menu_popup(&lw).rect.expect("a box");
    assert!(rect.origin.y + rect.size.height <= top + 1.0, "above the control: {rect:?}");
    assert!(rect.origin.y >= 0.0, "and inside the window: {rect:?}");
}

/// A context menu is opened at a point: the menu starts there.
#[test]
fn a_context_menu_opens_at_the_point_it_was_asked_for() {
    let (mut lw, ws) = window(PHONE);
    let dom = page(100.0);
    show(&mut lw, &ws, &dom);
    let point = LogicalRect::new(LogicalPosition::new(120.0, 300.0), LogicalSize::zero());
    assert!(lw.open_menu_overlay(&menu(&["Cut", "Copy"]), point, PHONE));
    show(&mut lw, &ws, &dom);
    let rect = the_menu_popup(&lw).rect.expect("a box");
    assert!((rect.origin.x - 120.0).abs() < 1.0 && (rect.origin.y - 300.0).abs() < 1.0, "{rect:?}");
}

/// A press outside the menu closes it - the app's DOM never held it - and the press is not spent.
#[test]
fn a_press_outside_a_menu_closes_it_and_a_press_on_its_control_does_not() {
    let (mut lw, ws) = window(PHONE);
    let dom = page(100.0);
    show(&mut lw, &ws, &dom);
    assert!(lw.open_menu_overlay(&menu(&["Copy", "Paste"]), control_rect(100.0), PHONE));
    show(&mut lw, &ws, &dom);
    let rect = the_menu_popup(&lw).rect.unwrap();

    let inside = LogicalPosition::new(rect.origin.x + 10.0, rect.origin.y + 10.0);
    let on_control = LogicalPosition::new(60.0, 120.0);
    assert!(!lw.dismiss_overlays_on_press(Some(inside)), "inside the menu");
    assert!(!lw.dismiss_overlays_on_press(Some(on_control)), "on its control: the control decides");
    assert_eq!(lw.menu_overlays.len(), 1);

    assert!(lw.dismiss_overlays_on_press(Some(LogicalPosition::new(350.0, 700.0))), "outside it");
    assert!(lw.menu_overlays.is_empty(), "the menu is over, not only its popup");
    show(&mut lw, &ws, &dom);
    assert!(lw.open_popups().is_empty(), "and the next page has none");
}

/// Escape (and the back button, which is one) closes the menu.
#[test]
fn escape_closes_a_menu() {
    let (mut lw, ws) = window(PHONE);
    let dom = page(100.0);
    show(&mut lw, &ws, &dom);
    assert!(lw.open_menu_overlay(&menu(&["Copy"]), control_rect(100.0), PHONE));
    show(&mut lw, &ws, &dom);
    assert!(lw.dismiss_top_overlay_on_escape());
    assert!(lw.menu_overlays.is_empty());
    show(&mut lw, &ws, &dom);
    assert!(lw.open_popups().is_empty());
}

/// The control opens its menu and closes it again.
#[test]
fn the_control_that_opened_a_menu_closes_it_by_asking_again() {
    let (mut lw, ws) = window(PHONE);
    let dom = page(100.0);
    show(&mut lw, &ws, &dom);
    let items = menu(&["Copy", "Paste"]);
    assert!(lw.open_menu_overlay(&items, control_rect(100.0), PHONE));
    assert!(!lw.open_menu_overlay(&items, control_rect(100.0), PHONE), "the second tap closes");
    show(&mut lw, &ws, &dom);
    assert!(lw.open_popups().is_empty());
}

/// A menu belongs to the shape of the window it was opened in: a turn of the phone closes it.
#[test]
fn a_turn_of_the_window_closes_its_menu() {
    let (mut lw, ws) = window(PHONE);
    let dom = page(100.0);
    show(&mut lw, &ws, &dom);
    assert!(lw.open_menu_overlay(&menu(&["Copy", "Paste"]), control_rect(100.0), PHONE));
    show(&mut lw, &ws, &dom);
    assert_eq!(lw.open_popups().len(), 1);

    let mut turned = ws.clone();
    turned.size.dimensions = LogicalSize::new(PHONE.height, PHONE.width);
    lw.current_window_state = turned.clone();
    show(&mut lw, &turned, &dom);
    assert!(lw.menu_overlays.is_empty(), "the menu was opened in another window");
    assert!(lw.open_popups().is_empty());
}
