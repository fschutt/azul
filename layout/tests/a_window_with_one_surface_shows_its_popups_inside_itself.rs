//! A window with ONE surface (a phone) shows its popups and dialogs inside itself.
//!
//! A `<transient-window>` is an OS window of its own on a desktop. A phone cannot open another
//! window, so the engine lays the very same node out over the page: `position: fixed`, at the box the
//! transient manager placed, above everything - and gives a window the OS would have framed what the
//! OS gave it (a sheet: the scrim, the title, the close box). The page under it is laid out as if the
//! overlay were not there, the app's callbacks and DOM are the ones they were.
#![cfg(feature = "widgets")]

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    dom::{AttributeType, Dom, DomId, DomNodeId, IdOrClass, NodeData, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    transient::WindowPresentation,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const PHONE: LogicalSize = LogicalSize {
    width: 412.0,
    height: 915.0,
};

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

/// A window of `size` whose shell reports one surface (`one_surface`) or opens windows.
fn window(size: LogicalSize, one_surface: bool) -> (LayoutWindow, FullWindowState) {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    if one_surface {
        lw.transient_windows
            .set_shell_presentation(WindowPresentation::OneSurface);
    }
    let mut ws = FullWindowState::default();
    ws.size.dimensions = size;
    lw.current_window_state = ws.clone();
    (lw, ws)
}

/// The popup `anchor`ed to its parent, open or closed, around `children`.
fn popup(anchor: TransientAnchor, open: bool, label: Option<&str>, children: Vec<Dom>) -> Dom {
    let config = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_anchor(anchor)
    .with_dismiss(TransientDismiss::Outside);
    let mut node = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)));
    if let Some(label) = label {
        node = node.with_attribute(AttributeType::AriaLabel(label.into()));
    }
    for child in children {
        node = node.with_child(child);
    }
    node
}

/// `body > [div.top (100 px), div.row (40 px, 120 wide, 20 px from the left) > popup, div.page]`.
fn page(popup: Dom) -> Dom {
    Dom::create_body()
        .with_css("margin: 0;")
        .with_child(Dom::create_div().with_ids_and_classes(class("top")).with_css("height: 100px;"))
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("row"))
                .with_css("height: 40px; width: 120px; margin-left: 20px;")
                .with_child(popup),
        )
        .with_child(Dom::create_div().with_ids_and_classes(class("page")).with_css("height: 600px;"))
}

fn panel(width: f32, height: f32) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("panel"))
        .with_css(&format!("width: {width}px; height: {height}px;"))
}

/// What a shell's `regenerate_layout` does: the app's DOM goes through the Dom pass, is laid out, the
/// popups are reconciled - and while that asks for another pass, again.
fn show(lw: &mut LayoutWindow, ws: &FullWindowState, dom: &Dom) {
    let mut dom = dom.clone();
    let _ = lw.present_transient_windows(&mut dom, ws);
    for _ in 0..4 {
        let styled = StyledDom::create(&mut dom.clone(), azul_css::css::Css::empty());
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

fn node_of_type(lw: &LayoutWindow, matches: impl Fn(&NodeType) -> bool) -> NodeId {
    let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| matches(nodes[*n].get_node_type()))
        .expect("the node")
}

fn popup_node(lw: &LayoutWindow) -> NodeId {
    node_of_type(lw, |t| matches!(t, NodeType::TransientWindow(_)))
}

fn node_with_class(lw: &LayoutWindow, name: &str) -> NodeId {
    let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == name))
        })
        .unwrap_or_else(|| panic!("no node with class {name}"))
}

fn rect_of(lw: &LayoutWindow, node: NodeId) -> Option<LogicalRect> {
    lw.get_node_rect_in_viewport(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

/// Opens the popup the way a widget's callback does (`set_transient_window_open`).
fn open_popup(lw: &mut LayoutWindow) {
    let node = popup_node(lw);
    assert!(lw.transient_windows.set_forced_open(node, true));
}

/// THE picture: a popover opened from a row of the page is laid out at the box the manager placed,
/// below its invoker, over the page - and nothing of the page moved.
#[test]
fn a_popover_is_laid_out_below_its_invoker_over_the_page() {
    let (mut lw, ws) = window(PHONE, true);
    let dom = page(popup(TransientAnchor::Bottom, false, None, vec![panel(200.0, 150.0)]));
    show(&mut lw, &ws, &dom);
    let node = popup_node(&lw);
    let page_before = rect_of(&lw, node_with_class(&lw, "page")).unwrap();
    assert!(rect_of(&lw, node).is_none(), "closed: nothing to lay out");

    open_popup(&mut lw);
    show(&mut lw, &ws, &dom);

    let window_of = &lw.transient_windows.open_windows()[0];
    let overlay = window_of.overlay.expect("an overlay, not a window");
    assert_eq!(overlay.rect, rect(20.0, 140.0, 200.0, 150.0), "below the row, left edges lined up");
    assert_eq!(
        rect_of(&lw, popup_node(&lw)),
        Some(overlay.rect),
        "the node is laid out where the manager put it"
    );
    assert_eq!(
        rect_of(&lw, node_with_class(&lw, "panel")),
        Some(rect(20.0, 140.0, 200.0, 150.0)),
        "and its content in it"
    );
    assert_eq!(
        rect_of(&lw, node_with_class(&lw, "page")),
        Some(page_before),
        "the page under the overlay is where it was: an overlay is out of the flow"
    );
}

/// Closing it takes it out of the layout again.
#[test]
fn a_closed_popover_is_not_laid_out() {
    let (mut lw, ws) = window(PHONE, true);
    let dom = page(popup(TransientAnchor::Bottom, false, None, vec![panel(200.0, 150.0)]));
    show(&mut lw, &ws, &dom);
    open_popup(&mut lw);
    show(&mut lw, &ws, &dom);
    assert!(rect_of(&lw, popup_node(&lw)).is_some(), "premise: shown");

    let node = popup_node(&lw);
    assert!(lw.transient_windows.set_forced_open(node, false));
    show(&mut lw, &ws, &dom);
    assert!(lw.transient_windows.open_windows().is_empty());
    assert!(rect_of(&lw, popup_node(&lw)).is_none(), "gone");
}

/// A window that opens real windows is left alone: the popup is a window of the shell's, the node
/// is nothing in the page, the DOM is the app's.
#[test]
fn a_window_that_opens_windows_is_not_touched() {
    let (mut lw, ws) = window(PHONE, false);
    let dom = page(popup(TransientAnchor::Bottom, true, None, vec![panel(200.0, 150.0)]));
    show(&mut lw, &ws, &dom);
    let w = &lw.transient_windows.open_windows()[0];
    assert!(w.overlay.is_none() && !w.has_no_surface(), "a window: {w:?}");
    assert!(rect_of(&lw, popup_node(&lw)).is_none(), "not laid out in the page");
}

/// The preset of a phone display makes ANY window one with one surface: the host tests and the
/// pictures of a desktop window put on a phone show the popups as the phone does.
#[test]
fn a_display_preset_makes_a_desktop_window_a_one_surface_window() {
    let (mut lw, mut ws) = window(PHONE, false);
    assert_eq!(lw.effective_presentation(&ws), WindowPresentation::Windows);
    ws.flags.display_preset = azul_core::display_features::DisplayPreset::Pixel8;
    assert_eq!(lw.effective_presentation(&ws), WindowPresentation::OneSurface);
    let dom = page(popup(TransientAnchor::Bottom, true, None, vec![panel(200.0, 150.0)]));
    show(&mut lw, &ws, &dom);
    assert!(lw.transient_windows.open_windows()[0].overlay.is_some());
    ws.flags.display_preset = azul_core::display_features::DisplayPreset::Off;
    assert_eq!(lw.effective_presentation(&ws), WindowPresentation::Windows);
}

/// A dialog is the window's top layer: the scrim is the whole window, the card is a bottom sheet on
/// a narrow window, with the title the OS's title bar was and a close box.
#[test]
fn a_dialog_is_a_sheet_at_the_bottom_of_a_narrow_window() {
    let (mut lw, ws) = window(PHONE, true);
    let content = Dom::create_div()
        .with_ids_and_classes(class("panel"))
        .with_css("width: 280px; height: 120px;")
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Dialog,
            accessibility_name: Some(azul_css::AzString::from("Delete?")).into(),
            ..Default::default()
        });
    let dom = page(popup(TransientAnchor::Dialog, true, Some("Delete?"), vec![content]));
    show(&mut lw, &ws, &dom);

    let overlay = lw.transient_windows.open_windows()[0].overlay.expect("an overlay");
    assert!(overlay.cover);
    assert_eq!(rect_of(&lw, popup_node(&lw)), Some(rect(0.0, 0.0, 412.0, 915.0)), "the scrim is the window");

    let sheet = rect_of(&lw, node_with_class(&lw, "__azul-overlay-sheet")).expect("the sheet is laid out");
    assert_eq!(sheet.size.width, 412.0, "full width");
    assert_eq!(sheet.origin.y + sheet.size.height, 915.0, "at the bottom of the window");
    assert!(sheet.size.height >= 120.0, "it holds the content: {sheet:?}");

    let title = rect_of(&lw, node_with_class(&lw, "__azul-overlay-title")).expect("the title row");
    let close = rect_of(&lw, node_with_class(&lw, "__azul-overlay-close")).expect("the close box");
    assert!(title.origin.y >= sheet.origin.y && close.origin.y >= sheet.origin.y);
    assert!(close.size.width >= 44.0 && close.size.height >= 44.0, "a finger can hit it: {close:?}");
    let content_rect = rect_of(&lw, node_with_class(&lw, "panel")).expect("the content");
    assert!(
        content_rect.origin.y >= title.origin.y + title.size.height - 1.0,
        "the content is under the title: {content_rect:?} {title:?}"
    );
}

/// From 600 px of width a dialog is a centred card, not a sheet.
#[test]
fn a_dialog_is_a_centred_card_on_a_wide_window() {
    let wide = LogicalSize::new(800.0, 600.0);
    let (mut lw, ws) = window(wide, true);
    let content = panel(280.0, 120.0);
    let dom = page(popup(TransientAnchor::Dialog, true, Some("Delete?"), vec![content]));
    show(&mut lw, &ws, &dom);
    let sheet = rect_of(&lw, node_with_class(&lw, "__azul-overlay-sheet")).expect("the card");
    assert!(sheet.size.width <= 560.0 && sheet.size.width >= 320.0, "a card: {sheet:?}");
    let centre_x = sheet.origin.x + sheet.size.width / 2.0;
    let centre_y = sheet.origin.y + sheet.size.height / 2.0;
    assert!((centre_x - 400.0).abs() <= 1.0, "centred across: {sheet:?}");
    assert!((centre_y - 300.0).abs() <= 1.0, "and down: {sheet:?}");
}

/// A press outside an open popover closes it - and the press on its invoker does not, the invoker
/// decides about a press on itself.
#[test]
fn a_press_outside_a_popover_closes_it_and_a_press_inside_or_on_its_invoker_does_not() {
    let (mut lw, ws) = window(PHONE, true);
    let dom = page(popup(TransientAnchor::Bottom, false, None, vec![panel(200.0, 150.0)]));
    show(&mut lw, &ws, &dom);
    open_popup(&mut lw);
    show(&mut lw, &ws, &dom);

    let inside = LogicalPosition::new(100.0, 200.0);
    let on_invoker = LogicalPosition::new(60.0, 120.0);
    let outside = LogicalPosition::new(300.0, 600.0);
    assert!(!lw.dismiss_overlays_on_press(Some(inside)), "inside it");
    assert!(!lw.dismiss_overlays_on_press(Some(on_invoker)), "on its invoker");
    assert!(!lw.dismiss_overlays_on_press(None), "off the window");
    assert_eq!(lw.transient_windows.open_windows().len(), 1);

    let events_before = lw.pending_lifecycle_events.len();
    assert!(lw.dismiss_overlays_on_press(Some(outside)), "outside it");
    assert!(lw.transient_windows.open_windows().is_empty());
    assert_eq!(
        lw.pending_lifecycle_events.len(),
        events_before + 1,
        "the node hears `Dismissed`"
    );
}

/// A modal's page is inert: a press on the scrim is not a light dismiss, the dialog closes by a
/// request (Escape, back, its close box).
#[test]
fn a_modal_dialog_is_not_closed_by_a_press_outside_it() {
    let (mut lw, ws) = window(PHONE, true);
    let dom = page(popup(TransientAnchor::Dialog, true, Some("Delete?"), vec![panel(280.0, 120.0)]));
    show(&mut lw, &ws, &dom);
    assert!(!lw.dismiss_overlays_on_press(Some(LogicalPosition::new(10.0, 10.0))));
    assert_eq!(lw.transient_windows.open_windows().len(), 1);
}

/// Escape closes ONE overlay, the top one: the one opened last. A dropdown open over a popover
/// closes first.
#[test]
fn escape_closes_the_top_overlay_only() {
    let (mut lw, ws) = window(PHONE, true);
    let dom = Dom::create_body()
        .with_css("margin: 0;")
        .with_child(
            Dom::create_div()
                .with_css("height: 40px; width: 120px;")
                .with_child(popup(TransientAnchor::Bottom, true, None, vec![panel(100.0, 50.0)])),
        )
        .with_child(
            Dom::create_div()
                .with_css("height: 40px; width: 120px;")
                .with_child(popup(TransientAnchor::Bottom, true, None, vec![panel(100.0, 50.0)])),
        );
    show(&mut lw, &ws, &dom);
    let nodes: Vec<NodeId> = lw
        .transient_windows
        .open_windows()
        .iter()
        .map(|w| w.source_node)
        .collect();
    assert_eq!(nodes.len(), 2);

    assert!(lw.dismiss_top_overlay_on_escape());
    let left: Vec<NodeId> = lw
        .transient_windows
        .open_windows()
        .iter()
        .map(|w| w.source_node)
        .collect();
    assert_eq!(left, vec![nodes[0]], "the last opened closed");
    assert!(lw.dismiss_top_overlay_on_escape());
    assert!(!lw.dismiss_top_overlay_on_escape(), "nothing left");
}

/// A popup you work in takes the keyboard when it appears: its first control, never the close box
/// (last in tree order), once it is laid out - and not before.
#[test]
fn a_dialog_takes_the_keyboard_when_it_opens_and_not_before_it_is_laid_out() {
    use azul_core::dom::TabIndex;
    let (mut lw, ws) = window(PHONE, true);
    let control = |name: &str| {
        let mut d = Dom::create_div().with_ids_and_classes(class(name)).with_css("height: 30px;");
        d.set_tab_index(TabIndex::Auto);
        d
    };
    let content = Dom::create_div()
        .with_css("width: 280px;")
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Dialog,
            ..Default::default()
        })
        .with_child(control("first"))
        .with_child(control("second"));
    let dom = page(popup(TransientAnchor::Dialog, false, Some("Rename"), vec![content]));
    show(&mut lw, &ws, &dom);
    assert!(lw.overlay_autofocus_target().is_none(), "nothing is open");

    open_popup(&mut lw);
    // The reconcile has found it, the root has not laid it out: not yet.
    let mut dom_pass = dom.clone();
    let _ = lw.present_transient_windows(&mut dom_pass, &ws);
    let styled = StyledDom::create(&mut dom_pass.clone(), azul_css::css::Css::empty());
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    lw.reconcile_transients(&ws, &mut || Vec::new());
    assert!(lw.overlay_autofocus_target().is_none(), "the overlay is not laid out yet");

    show(&mut lw, &ws, &dom);
    let target = lw.overlay_autofocus_target().expect("the first control of the dialog");
    assert_eq!(target.node.into_crate_internal(), Some(node_with_class(&lw, "first")));
    assert!(lw.overlay_autofocus_target().is_none(), "once");
}

/// A list popup (a combobox's options) leaves the keyboard on its field.
#[test]
fn a_list_popup_leaves_the_keyboard_on_its_invoker() {
    use azul_core::dom::TabIndex;
    let (mut lw, ws) = window(PHONE, true);
    let mut option = Dom::create_div().with_css("height: 30px; width: 100px;");
    option.set_tab_index(TabIndex::Auto);
    let list = Dom::create_div()
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::List,
            ..Default::default()
        })
        .with_child(option);
    let dom = page(popup(TransientAnchor::Bottom, true, None, vec![list]));
    show(&mut lw, &ws, &dom);
    assert!(lw.overlay_autofocus_target().is_none());
}

/// The focus a closed overlay hid is not "moved away by the user": it goes back.
#[test]
fn focus_inside_a_dismissed_overlay_is_lost_with_it() {
    let (mut lw, ws) = window(PHONE, true);
    let dom = page(popup(TransientAnchor::Bottom, false, None, vec![panel(200.0, 150.0)]));
    show(&mut lw, &ws, &dom);
    open_popup(&mut lw);
    show(&mut lw, &ws, &dom);
    let inside = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_with_class(&lw, "panel"))),
    };
    let elsewhere = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_with_class(&lw, "page"))),
    };
    let node = popup_node(&lw);
    assert!(lw.dismiss_transient_window(node).is_some());
    assert!(lw.focus_is_inside_dismissed_overlay(inside), "focus was in the overlay");
    assert!(!lw.focus_is_inside_dismissed_overlay(inside), "asked once");

    open_popup(&mut lw);
    show(&mut lw, &ws, &dom);
    let node = popup_node(&lw);
    assert!(lw.dismiss_transient_window(node).is_some());
    assert!(!lw.focus_is_inside_dismissed_overlay(elsewhere), "the user clicked elsewhere");
}
