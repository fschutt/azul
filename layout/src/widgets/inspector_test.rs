//! The Inspector's DOM and drag: the dock, the pane's size, the grab zone, the keys, what a rebuild
//! carries over.

use azul_core::dom::TabIndex;

use super::*;

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| c.as_class() == Some(class))
}

fn first<'a>(dom: &'a Dom, class: &str) -> Option<&'a Dom> {
    if has_class(dom, class) {
        return Some(dom);
    }
    dom.children.as_ref().iter().find_map(|c| first(c, class))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

fn texts(dom: &Dom, out: &mut Vec<String>) {
    if let azul_core::dom::NodeType::Text(t) = dom.root.get_node_type() {
        out.push(t.as_ref().as_str().to_string());
    }
    for child in dom.children.as_ref() {
        texts(child, out);
    }
}

fn text_of(dom: &Dom) -> String {
    let mut v = Vec::new();
    texts(dom, &mut v);
    v.join(" ")
}

fn inspector() -> Inspector {
    Inspector::create(
        Dom::create_div().with_id(AzString::from("body")),
        Dom::create_div().with_id(AzString::from("pane")),
    )
    .with_title(AzString::from("Preview"))
}

fn state(dock: InspectorDock, size: f32) -> DragState {
    DragState {
        on_event: OptionInspectorOnEvent::None,
        dock,
        size,
        dragging: false,
        start_main: 0.0,
        start_size: size,
    }
}

// ---- the dock ----

#[test]
fn a_desktop_docks_the_pane_at_the_right_and_a_phone_at_the_bottom() {
    let desktop = inspector().with_form(FormFactor::Desktop);
    assert_eq!(desktop.resolved_dock(), InspectorDock::Right);
    assert!(has_class(&desktop.dom(), INSPECTOR_RIGHT_CLASS));
    let phone = inspector().with_form(FormFactor::Phone);
    assert_eq!(phone.resolved_dock(), InspectorDock::Bottom);
    assert!(has_class(&phone.dom(), INSPECTOR_BOTTOM_CLASS));
}

#[test]
fn a_dock_the_app_names_stays_whatever_the_form_factor() {
    let right_on_a_phone = inspector().with_form(FormFactor::Phone).with_dock(InspectorDock::Right);
    assert_eq!(right_on_a_phone.resolved_dock(), InspectorDock::Right);
    let bottom_on_a_desktop = inspector().with_form(FormFactor::Desktop).with_dock(InspectorDock::Bottom);
    assert_eq!(bottom_on_a_desktop.resolved_dock(), InspectorDock::Bottom);
}

#[test]
fn the_root_holds_the_body_then_the_pane() {
    let dom = inspector().with_form(FormFactor::Desktop).dom();
    assert_eq!(dom.children.as_ref().len(), 2);
    assert!(has_class(&dom.children.as_ref()[0], INSPECTOR_BODY_CLASS));
    assert!(has_class(&dom.children.as_ref()[1], INSPECTOR_PANE_CLASS));
}

#[test]
fn a_closed_inspector_is_the_body_alone() {
    let dom = inspector().with_form(FormFactor::Desktop).with_open(false).dom();
    assert_eq!(dom.children.as_ref().len(), 1);
    assert!(first(&dom, INSPECTOR_PANE_CLASS).is_none());
    assert!(first(&dom, INSPECTOR_GRIP_CLASS).is_none());
    // and nothing is listening for a drag that cannot start
    assert!(dom.root.get_callbacks().as_ref().is_empty());
}

#[test]
fn the_pane_has_a_grab_zone_a_header_with_its_title_and_the_content() {
    let dom = inspector().with_form(FormFactor::Phone).dom();
    let pane = first(&dom, INSPECTOR_PANE_CLASS).expect("the pane");
    let kids = pane.children.as_ref();
    assert_eq!(kids.len(), 3);
    assert!(has_class(&kids[0], INSPECTOR_GRIP_CLASS));
    assert!(has_class(&kids[1], INSPECTOR_HEADER_CLASS));
    assert!(has_class(&kids[2], INSPECTOR_CONTENT_CLASS));
    assert!(text_of(&kids[1]).contains("Preview"));
}

#[test]
fn the_bottom_grip_has_a_pill_and_the_right_dock_has_only_a_border() {
    let bottom = inspector().with_form(FormFactor::Phone).dom();
    assert!(first(first(&bottom, INSPECTOR_GRIP_CLASS).expect("a grip"), INSPECTOR_PILL_CLASS).is_some());
    let right = inspector().with_form(FormFactor::Desktop).dom();
    assert!(first(first(&right, INSPECTOR_GRIP_CLASS).expect("a border"), INSPECTOR_PILL_CLASS).is_none());
}

#[test]
fn the_grip_is_a_named_focusable_splitter_with_the_size_as_its_value() {
    let dom = inspector().with_form(FormFactor::Phone).with_size(200.0).dom();
    let grip = first(&dom, INSPECTOR_GRIP_CLASS).expect("a grip");
    let a11y = grip.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Grip);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Resize Preview"));
    assert_eq!(a11y.accessibility_value.as_ref().map(|n| n.as_str()), Some("200"));
    assert_eq!(grip.root.get_tab_index(), Some(TabIndex::Auto));
}

#[test]
fn the_pane_is_a_named_pane() {
    let dom = inspector().with_form(FormFactor::Desktop).dom();
    let pane = first(&dom, INSPECTOR_PANE_CLASS).expect("the pane");
    let a11y = pane.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Pane);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Preview"));
}

#[test]
fn what_the_app_puts_at_the_end_of_the_header_is_in_the_header() {
    let end = Dom::create_div().with_id(AzString::from("close"));
    let dom = inspector().with_form(FormFactor::Phone).with_header_end(end).dom();
    let header = first(&dom, INSPECTOR_HEADER_CLASS).expect("a header");
    fn has_id(dom: &Dom, id: &str) -> bool {
        dom.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id() == Some(id))
            || dom.children.as_ref().iter().any(|c| has_id(c, id))
    }
    assert!(has_id(header, "close"));
}

// ---- the size ----

#[test]
fn the_pane_starts_at_320_wide_or_168_tall_when_the_app_says_nothing() {
    assert!(near(inspector().with_form(FormFactor::Desktop).built_size(), 320.0));
    assert!(near(inspector().with_form(FormFactor::Phone).built_size(), 168.0));
}

#[test]
fn a_size_is_held_to_the_limits_and_a_bad_number_is_the_default() {
    assert!(near(inspector().with_form(FormFactor::Desktop).with_size(50.0).built_size(), 200.0));
    assert!(near(inspector().with_form(FormFactor::Desktop).with_size(f32::NAN).built_size(), 320.0));
    assert!(near(inspector().with_form(FormFactor::Phone).with_size(5.0).built_size(), PANE_PEEK));
    assert!(near(inspector().with_form(FormFactor::Phone).with_size(f32::INFINITY).built_size(), 168.0));
}

#[test]
fn the_pane_range_is_the_peek_to_85_percent() {
    let (low, high) = pane_range(1000.0);
    assert!(near(low, 56.0) && near(high, 850.0));
}

// ---- the grab zone and the drag ----

#[test]
fn a_press_on_the_right_docks_border_grabs_it_and_one_elsewhere_does_not() {
    let st = state(InspectorDock::Right, 320.0);
    // a 1000 px container: the border is at 680
    assert!(st.grabs(680.0, 100.0, 1000.0, 600.0));
    assert!(st.grabs(675.0, 100.0, 1000.0, 600.0));
    assert!(st.grabs(686.0, 100.0, 1000.0, 600.0));
    assert!(!st.grabs(600.0, 100.0, 1000.0, 600.0), "over the body");
    assert!(!st.grabs(900.0, 100.0, 1000.0, 600.0), "inside the pane");
}

#[test]
fn a_press_on_the_bottom_panes_grip_row_grabs_it_and_one_in_its_content_does_not() {
    let st = state(InspectorDock::Bottom, 200.0);
    // a 800 px tall container: the pane starts at 600 and its grip row is 28 tall
    assert!(st.grabs(100.0, 610.0, 400.0, 800.0));
    assert!(st.grabs(100.0, 599.0, 400.0, 800.0), "a hair above the pane");
    assert!(!st.grabs(100.0, 640.0, 400.0, 800.0), "in the header");
    assert!(!st.grabs(100.0, 300.0, 400.0, 800.0), "over the body");
}

#[test]
fn dragging_the_border_left_widens_the_pane_and_the_grip_up_heightens_it() {
    let mut st = state(InspectorDock::Right, 320.0);
    st.start_main = 680.0;
    st.start_size = 320.0;
    assert!(near(st.size_at(580.0, 1000.0, 600.0), 420.0));
    assert!(near(st.size_at(780.0, 1000.0, 600.0), 220.0));
    assert!(near(st.size_at(0.0, 1000.0, 600.0), 600.0), "never past 60 % of the container");
    let mut st = state(InspectorDock::Bottom, 168.0);
    st.start_main = 632.0;
    st.start_size = 168.0;
    assert!(near(st.size_at(532.0, 400.0, 800.0), 268.0));
    assert!(near(st.size_at(780.0, 400.0, 800.0), PANE_PEEK), "never lower than the peek");
}

#[test]
fn the_pointers_place_along_the_drag_axis_is_x_for_the_right_dock_and_y_for_the_bottom() {
    let pos = CursorNodePosition { x: 12.0, y: 34.0 };
    assert!(near(state(InspectorDock::Right, 300.0).main_of(pos), 12.0));
    assert!(near(state(InspectorDock::Bottom, 300.0).main_of(pos), 34.0));
}

#[test]
fn the_size_property_is_width_for_the_right_dock_and_height_for_the_bottom() {
    assert!(matches!(size_property(InspectorDock::Right, 300.0), CssProperty::Width(_)));
    assert!(matches!(size_property(InspectorDock::Bottom, 300.0), CssProperty::Height(_)));
}

#[test]
fn the_container_listens_for_the_mouse_and_the_finger() {
    let dom = inspector().with_form(FormFactor::Phone).dom();
    let events: Vec<EventFilter> = dom.root.get_callbacks().as_ref().iter().map(|c| c.event).collect();
    for filter in [
        EventFilter::Hover(HoverEventFilter::MouseDown),
        EventFilter::Hover(HoverEventFilter::MouseOver),
        EventFilter::Hover(HoverEventFilter::MouseUp),
        EventFilter::Hover(HoverEventFilter::TouchStart),
        EventFilter::Hover(HoverEventFilter::TouchMove),
        EventFilter::Hover(HoverEventFilter::TouchEnd),
    ] {
        assert!(events.contains(&filter), "{filter:?}");
    }
}

#[test]
fn the_grip_listens_for_keys_and_for_a_double_tap() {
    let dom = inspector().with_form(FormFactor::Phone).dom();
    let grip = first(&dom, INSPECTOR_GRIP_CLASS).expect("a grip");
    let events: Vec<EventFilter> = grip.root.get_callbacks().as_ref().iter().map(|c| c.event).collect();
    assert!(events.contains(&EventFilter::Focus(FocusEventFilter::VirtualKeyDown)));
    assert!(events.contains(&EventFilter::Hover(HoverEventFilter::DoubleClick)));
}

// ---- a rebuild during a drag ----

#[test]
fn a_rebuild_during_a_drag_carries_the_drag_and_the_live_size_over() {
    let mut old = state(InspectorDock::Right, 400.0);
    old.dragging = true;
    old.start_main = 700.0;
    old.start_size = 320.0;
    let fresh = state(InspectorDock::Right, 320.0);
    let mut merged = merge_inspector_state(RefAny::new(fresh), RefAny::new(old));
    let got = merged.downcast_ref::<DragState>().expect("a drag state");
    assert!(got.dragging);
    assert!(near(got.start_main, 700.0) && near(got.start_size, 320.0));
    assert!(near(got.size, 400.0), "the live size, not the app's stale one");
}

#[test]
fn a_rebuild_after_the_pointer_is_up_takes_the_apps_size() {
    let mut old = state(InspectorDock::Right, 400.0);
    old.dragging = false;
    let fresh = state(InspectorDock::Right, 320.0);
    let mut merged = merge_inspector_state(RefAny::new(fresh), RefAny::new(old));
    let got = merged.downcast_ref::<DragState>().expect("a drag state");
    assert!(!got.dragging);
    assert!(near(got.size, 320.0));
}

#[test]
fn a_rebuild_that_changed_the_dock_starts_over() {
    let mut old = state(InspectorDock::Right, 400.0);
    old.dragging = true;
    let fresh = state(InspectorDock::Bottom, 168.0);
    let mut merged = merge_inspector_state(RefAny::new(fresh), RefAny::new(old));
    let got = merged.downcast_ref::<DragState>().expect("a drag state");
    assert!(!got.dragging, "the anchor was on the other axis");
    assert!(near(got.size, 168.0));
}

// ---- the surface ----

#[test]
fn the_pane_wears_the_apps_surface() {
    assert!(ground(SidebarSurface::Theme).contains("system:window-background"));
    assert!(ground(SidebarSurface::Leather).contains("builtin(leather)"));
    assert!(ground(SidebarSurface::Vellum).contains("builtin(vellum-leaf)"));
}
