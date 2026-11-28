//! The Preview's DOM and drag: the dock, the pane's size, the grab zone, the keys, what a rebuild
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

fn preview() -> Preview {
    Preview::create(
        Dom::create_div().with_id(AzString::from("body")),
        Dom::create_div().with_id(AzString::from("pane")),
    )
    .with_title(AzString::from("Preview"))
}

fn state(dock: PreviewDock, size: f32) -> DragState {
    DragState {
        on_event: OptionPreviewOnEvent::None,
        dock,
        size,
        dragging: false,
        moved: false,
        start_main: 0.0,
        start_size: size,
    }
}

// ---- the dock ----

#[test]
fn a_desktop_docks_the_pane_at_the_right_and_a_phone_at_the_bottom() {
    let desktop = preview().with_form(FormFactor::Desktop);
    assert_eq!(desktop.resolved_dock(), PreviewDock::Right);
    assert!(has_class(&desktop.dom(), PREVIEW_RIGHT_CLASS));
    let phone = preview().with_form(FormFactor::Phone);
    assert_eq!(phone.resolved_dock(), PreviewDock::Bottom);
    assert!(has_class(&phone.dom(), PREVIEW_BOTTOM_CLASS));
}

#[test]
fn a_dock_the_app_names_stays_whatever_the_form_factor() {
    let right_on_a_phone = preview().with_form(FormFactor::Phone).with_dock(PreviewDock::Right);
    assert_eq!(right_on_a_phone.resolved_dock(), PreviewDock::Right);
    let bottom_on_a_desktop = preview().with_form(FormFactor::Desktop).with_dock(PreviewDock::Bottom);
    assert_eq!(bottom_on_a_desktop.resolved_dock(), PreviewDock::Bottom);
}

#[test]
fn the_root_holds_the_body_then_the_pane() {
    let dom = preview().with_form(FormFactor::Desktop).dom();
    assert_eq!(dom.children.as_ref().len(), 2);
    assert!(has_class(&dom.children.as_ref()[0], PREVIEW_BODY_CLASS));
    assert!(has_class(&dom.children.as_ref()[1], PREVIEW_PANE_CLASS));
}

#[test]
fn a_closed_preview_is_the_body_alone() {
    let dom = preview().with_form(FormFactor::Desktop).with_open(false).dom();
    assert_eq!(dom.children.as_ref().len(), 1);
    assert!(first(&dom, PREVIEW_PANE_CLASS).is_none());
    assert!(first(&dom, PREVIEW_GRIP_CLASS).is_none());
    // and nothing is listening for a drag that cannot start
    assert!(dom.root.get_callbacks().as_ref().is_empty());
}

#[test]
fn a_bottom_pane_is_one_bar_with_the_grip_in_it_and_the_content() {
    let dom = preview().with_form(FormFactor::Phone).dom();
    let pane = first(&dom, PREVIEW_PANE_CLASS).expect("the pane");
    let kids = pane.children.as_ref();
    assert_eq!(kids.len(), 2, "the bar and the content: no banner, no second header row");
    assert!(has_class(&kids[0], PREVIEW_HEADER_CLASS));
    assert!(has_class(&kids[1], PREVIEW_CONTENT_CLASS));
    assert!(first(&kids[0], PREVIEW_GRIP_CLASS).is_some(), "the grip is a layer of the bar");
    assert!(text_of(&kids[0]).contains("Preview"), "the title is in the bar: a collapsed pane shows it");
}

#[test]
fn a_right_pane_is_a_header_a_border_strip_and_the_content() {
    let dom = preview().with_form(FormFactor::Desktop).dom();
    let pane = first(&dom, PREVIEW_PANE_CLASS).expect("the pane");
    let kids = pane.children.as_ref();
    assert_eq!(kids.len(), 3);
    assert!(has_class(&kids[0], PREVIEW_HEADER_CLASS));
    assert!(has_class(&kids[1], PREVIEW_GRIP_CLASS));
    assert!(has_class(&kids[2], PREVIEW_CONTENT_CLASS));
    assert!(text_of(&kids[0]).contains("Preview"));
}

#[test]
fn the_bottom_grip_has_a_pill_and_the_right_dock_has_only_a_border() {
    let bottom = preview().with_form(FormFactor::Phone).dom();
    assert!(first(first(&bottom, PREVIEW_GRIP_CLASS).expect("a grip"), PREVIEW_PILL_CLASS).is_some());
    let right = preview().with_form(FormFactor::Desktop).dom();
    assert!(first(first(&right, PREVIEW_GRIP_CLASS).expect("a border"), PREVIEW_PILL_CLASS).is_none());
}

#[test]
fn the_grip_is_a_named_focusable_splitter_with_the_size_as_its_value() {
    let dom = preview().with_form(FormFactor::Phone).with_size(200.0).dom();
    let grip = first(&dom, PREVIEW_GRIP_CLASS).expect("a grip");
    let a11y = grip.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Grip);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Resize Preview"));
    assert_eq!(a11y.accessibility_value.as_ref().map(|n| n.as_str()), Some("200"));
    assert_eq!(grip.root.get_tab_index(), Some(TabIndex::Auto));
}

#[test]
fn the_pane_is_a_named_pane() {
    let dom = preview().with_form(FormFactor::Desktop).dom();
    let pane = first(&dom, PREVIEW_PANE_CLASS).expect("the pane");
    let a11y = pane.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Pane);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Preview"));
}

#[test]
fn what_the_app_puts_at_the_end_of_the_header_is_in_the_header() {
    let end = Dom::create_div().with_id(AzString::from("close"));
    let dom = preview().with_form(FormFactor::Phone).with_header_end(end).dom();
    let header = first(&dom, PREVIEW_HEADER_CLASS).expect("a header");
    fn has_id(dom: &Dom, id: &str) -> bool {
        dom.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id() == Some(id))
            || dom.children.as_ref().iter().any(|c| has_id(c, id))
    }
    assert!(has_id(header, "close"));
}

// ---- the size ----

#[test]
fn the_pane_starts_at_320_wide_or_at_its_130_px_peek_when_the_app_says_nothing() {
    assert!(near(preview().with_form(FormFactor::Desktop).built_size(), 320.0));
    assert!(near(preview().with_form(FormFactor::Phone).built_size(), 130.0));
}

#[test]
fn a_size_is_held_to_the_limits_and_a_bad_number_is_the_default() {
    assert!(near(preview().with_form(FormFactor::Desktop).with_size(50.0).built_size(), 200.0));
    assert!(near(preview().with_form(FormFactor::Desktop).with_size(f32::NAN).built_size(), 320.0));
    assert!(near(preview().with_form(FormFactor::Phone).with_size(5.0).built_size(), PANE_COLLAPSED));
    assert!(near(preview().with_form(FormFactor::Phone).with_size(f32::INFINITY).built_size(), 130.0));
}

#[test]
fn the_pane_range_is_the_collapsed_bar_to_85_percent() {
    let (low, high) = pane_range(1000.0);
    assert!(near(low, 30.0) && near(high, 850.0));
}

// ---- the grab zone and the drag ----

#[test]
fn a_press_on_the_right_docks_border_grabs_it_and_one_elsewhere_does_not() {
    let st = state(PreviewDock::Right, 320.0);
    // a 1000 px container: the border is at 680
    assert!(st.grabs(680.0, 100.0, 1000.0, 600.0));
    assert!(st.grabs(675.0, 100.0, 1000.0, 600.0));
    assert!(st.grabs(686.0, 100.0, 1000.0, 600.0));
    assert!(!st.grabs(600.0, 100.0, 1000.0, 600.0), "over the body");
    assert!(!st.grabs(900.0, 100.0, 1000.0, 600.0), "inside the pane");
}

#[test]
fn a_press_on_the_bottom_panes_grip_row_grabs_it_and_one_in_its_content_does_not() {
    let st = state(PreviewDock::Bottom, 200.0);
    // a 800 px tall container: the pane starts at 600 and its grip row is 28 tall
    assert!(st.grabs(100.0, 610.0, 400.0, 800.0));
    assert!(st.grabs(100.0, 599.0, 400.0, 800.0), "a hair above the pane");
    assert!(!st.grabs(100.0, 640.0, 400.0, 800.0), "in the header");
    assert!(!st.grabs(100.0, 300.0, 400.0, 800.0), "over the body");
}

#[test]
fn dragging_the_border_left_widens_the_pane_and_the_grip_up_heightens_it() {
    let mut st = state(PreviewDock::Right, 320.0);
    st.start_main = 680.0;
    st.start_size = 320.0;
    assert!(near(st.size_at(580.0, 1000.0, 600.0), 420.0));
    assert!(near(st.size_at(780.0, 1000.0, 600.0), 220.0));
    assert!(near(st.size_at(0.0, 1000.0, 600.0), 600.0), "never past 60 % of the container");
    let mut st = state(PreviewDock::Bottom, 168.0);
    st.start_main = 632.0;
    st.start_size = 168.0;
    assert!(near(st.size_at(532.0, 400.0, 800.0), 268.0));
    assert!(near(st.size_at(780.0, 400.0, 800.0), PANE_COLLAPSED), "never lower than the collapsed bar");
}

#[test]
fn the_pointers_place_along_the_drag_axis_is_x_for_the_right_dock_and_y_for_the_bottom() {
    let pos = CursorNodePosition { x: 12.0, y: 34.0 };
    assert!(near(state(PreviewDock::Right, 300.0).main_of(pos), 12.0));
    assert!(near(state(PreviewDock::Bottom, 300.0).main_of(pos), 34.0));
}

#[test]
fn the_size_property_is_width_for_the_right_dock_and_height_for_the_bottom() {
    assert!(matches!(size_property(PreviewDock::Right, 300.0), CssProperty::Width(_)));
    assert!(matches!(size_property(PreviewDock::Bottom, 300.0), CssProperty::Height(_)));
}

#[test]
fn the_container_listens_for_the_mouse_and_the_finger() {
    let dom = preview().with_form(FormFactor::Phone).dom();
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
fn the_grip_listens_for_keys_and_the_tap_is_the_containers_release_without_a_drag() {
    let dom = preview().with_form(FormFactor::Phone).dom();
    let grip = first(&dom, PREVIEW_GRIP_CLASS).expect("a grip");
    let events: Vec<EventFilter> = grip.root.get_callbacks().as_ref().iter().map(|c| c.event).collect();
    assert!(events.contains(&EventFilter::Focus(FocusEventFilter::VirtualKeyDown)));
    assert!(!events.contains(&EventFilter::Hover(HoverEventFilter::DoubleClick)), "a single tap toggles");
    assert!(!events.contains(&EventFilter::Hover(HoverEventFilter::Click)), "the release decides tap or drag");
}

// ---- a rebuild during a drag ----

#[test]
fn a_rebuild_during_a_drag_carries_the_drag_and_the_live_size_over() {
    let mut old = state(PreviewDock::Right, 400.0);
    old.dragging = true;
    old.moved = true;
    old.start_main = 700.0;
    old.start_size = 320.0;
    let fresh = state(PreviewDock::Right, 320.0);
    let mut merged = merge_preview_state(RefAny::new(fresh), RefAny::new(old));
    let got = merged.downcast_ref::<DragState>().expect("a drag state");
    assert!(got.dragging && got.moved);
    assert!(near(got.start_main, 700.0) && near(got.start_size, 320.0));
    assert!(near(got.size, 400.0), "the live size, not the app's stale one");
}

#[test]
fn a_rebuild_after_the_pointer_is_up_takes_the_apps_size() {
    let mut old = state(PreviewDock::Right, 400.0);
    old.dragging = false;
    let fresh = state(PreviewDock::Right, 320.0);
    let mut merged = merge_preview_state(RefAny::new(fresh), RefAny::new(old));
    let got = merged.downcast_ref::<DragState>().expect("a drag state");
    assert!(!got.dragging);
    assert!(near(got.size, 320.0));
}

#[test]
fn a_rebuild_that_changed_the_dock_starts_over() {
    let mut old = state(PreviewDock::Right, 400.0);
    old.dragging = true;
    let fresh = state(PreviewDock::Bottom, 130.0);
    let mut merged = merge_preview_state(RefAny::new(fresh), RefAny::new(old));
    let got = merged.downcast_ref::<DragState>().expect("a drag state");
    assert!(!got.dragging, "the anchor was on the other axis");
    assert!(near(got.size, 130.0));
}

// ---- the surface ----

#[test]
fn the_pane_wears_the_apps_surface() {
    assert!(ground(SidebarSurface::Theme).contains("system:window-background"));
    assert!(ground(SidebarSurface::Leather).contains("builtin(leather)"));
    assert!(ground(SidebarSurface::Vellum).contains("builtin(vellum-leaf)"));
}

// ---- what is behind the (?) ----

fn count(dom: &Dom, class: &str) -> usize {
    usize::from(has_class(dom, class)) + dom.children.as_ref().iter().map(|c| count(c, class)).sum::<usize>()
}

fn name_of(dom: &Dom) -> String {
    dom.root
        .get_accessibility_info()
        .and_then(|a| a.accessibility_name.as_ref().map(|n| n.as_str().to_string()))
        .unwrap_or_default()
}

fn all<'a>(dom: &'a Dom, class: &str, out: &mut Vec<&'a Dom>) {
    if has_class(dom, class) {
        out.push(dom);
    }
    for child in dom.children.as_ref() {
        all(child, class, out);
    }
}

#[test]
fn nothing_is_dumped_by_default_no_help_button_and_no_banner() {
    for form in [FormFactor::Desktop, FormFactor::Phone] {
        let dom = preview().with_form(form).dom();
        assert_eq!(count(&dom, PREVIEW_HELP_CLASS), 0, "{form:?}");
        assert_eq!(count(&dom, PREVIEW_BANNER_CLASS), 0, "{form:?}");
    }
}

#[test]
fn each_detail_is_one_named_help_button_in_the_header() {
    let props = PreviewDetail::create(AzString::from("Properties"), Dom::create_div().with_id(AzString::from("props")));
    let history = PreviewDetail::create(AzString::from("History"), Dom::create_div()).with_icon(AzString::from("history"));
    for form in [FormFactor::Desktop, FormFactor::Phone] {
        let dom = preview().with_form(form).with_detail(props.clone()).with_detail(history.clone()).dom();
        let header = first(&dom, PREVIEW_HEADER_CLASS).expect("a header");
        let mut buttons = Vec::new();
        all(header, PREVIEW_HELP_CLASS, &mut buttons);
        let names: Vec<String> = buttons.iter().map(|b| name_of(b)).collect();
        assert_eq!(names, vec!["Properties".to_string(), "History".to_string()], "{form:?}");
        assert!(buttons.iter().all(|b| b.root.get_tab_index() == Some(TabIndex::Auto)));
    }
}

#[test]
fn a_press_on_a_help_button_is_the_buttons_and_does_not_start_a_drag() {
    let dom = preview()
        .with_form(FormFactor::Phone)
        .with_detail(PreviewDetail::create(AzString::from("Properties"), Dom::create_div()))
        .dom();
    let button = first(&dom, PREVIEW_HELP_CLASS).expect("a help button");
    let events: Vec<EventFilter> = button.root.get_callbacks().as_ref().iter().map(|c| c.event).collect();
    assert!(events.contains(&EventFilter::Hover(HoverEventFilter::MouseDown)));
    assert!(events.contains(&EventFilter::Hover(HoverEventFilter::TouchStart)));
}

#[test]
fn the_details_past_the_limit_are_dropped_and_the_first_are_kept() {
    let many: Vec<PreviewDetail> = (0..MAX_DETAILS + 3)
        .map(|i| PreviewDetail::create(AzString::from(format!("d{i}")), Dom::create_div()))
        .collect();
    let kept = limited_details(many);
    assert_eq!(kept.len(), MAX_DETAILS);
    assert_eq!(kept[0].label.as_str(), "d0");
}

#[test]
fn a_banner_is_one_line_at_the_very_top_of_a_right_pane() {
    let dom = preview()
        .with_form(FormFactor::Desktop)
        .with_banner(PreviewBanner::create(AzString::from("Unverified signer"), PreviewBannerTone::Warning))
        .dom();
    let pane = first(&dom, PREVIEW_PANE_CLASS).expect("the pane");
    assert!(has_class(&pane.children.as_ref()[0], PREVIEW_BANNER_CLASS), "above the header");
    assert!(text_of(&pane.children.as_ref()[0]).contains("Unverified signer"));
    assert_eq!(count(&dom, PREVIEW_HELP_CLASS), 0, "no details, no (?)");
}

#[test]
fn a_banner_is_under_the_grip_bar_of_a_bottom_pane() {
    let dom = preview()
        .with_form(FormFactor::Phone)
        .with_banner(PreviewBanner::create(AzString::from("Shared by Ana"), PreviewBannerTone::Danger))
        .dom();
    let pane = first(&dom, PREVIEW_PANE_CLASS).expect("the pane");
    let kids = pane.children.as_ref();
    assert!(has_class(&kids[0], PREVIEW_HEADER_CLASS), "the bar first");
    assert!(has_class(&kids[1], PREVIEW_BANNER_CLASS), "the banner under it");
    assert!(has_class(&kids[2], PREVIEW_CONTENT_CLASS));
}

#[test]
fn a_banners_details_are_behind_its_own_help_button() {
    let banner = PreviewBanner::create(AzString::from("Link strength: key"), PreviewBannerTone::Info)
        .with_details(Dom::create_div().with_id(AzString::from("why")));
    let dom = preview().with_form(FormFactor::Desktop).with_banner(banner).dom();
    let banner = first(&dom, PREVIEW_BANNER_CLASS).expect("the banner");
    assert_eq!(count(banner, PREVIEW_HELP_CLASS), 1);
    // the header has none: the banner's (?) is the banner's
    let header = first(&dom, PREVIEW_HEADER_CLASS).expect("a header");
    assert_eq!(count(header, PREVIEW_HELP_CLASS), 0);
}

#[test]
fn a_banner_is_an_alert_for_a_screen_reader() {
    let dom = preview()
        .with_form(FormFactor::Desktop)
        .with_banner(PreviewBanner::create(AzString::from("Phishing form"), PreviewBannerTone::Danger))
        .dom();
    let banner = first(&dom, PREVIEW_BANNER_CLASS).expect("the banner");
    let a11y = banner.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Alert);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Phishing form"));
}

#[test]
fn the_tones_have_their_own_icons_and_a_banner_can_name_one() {
    assert_eq!(PreviewBannerTone::Info.icon(), "info");
    assert_eq!(PreviewBannerTone::Warning.icon(), "warning");
    assert_eq!(PreviewBannerTone::Danger.icon(), "error");
    let banner = PreviewBanner::create(AzString::from("x"), PreviewBannerTone::Info).with_icon(AzString::from("shield"));
    assert_eq!(banner.icon.as_str(), "shield");
}

#[test]
fn a_collapsed_bottom_bar_still_shows_that_there_is_a_banner() {
    let with = preview()
        .with_form(FormFactor::Phone)
        .with_banner(PreviewBanner::create(AzString::from("Unverified signer"), PreviewBannerTone::Warning))
        .dom();
    let without = preview().with_form(FormFactor::Phone).dom();
    let icons = |d: &Dom| {
        fn walk(dom: &Dom, n: &mut usize) {
            if matches!(dom.root.get_node_type(), azul_core::dom::NodeType::Icon(_)) {
                *n += 1;
            }
            for c in dom.children.as_ref() {
                walk(c, n);
            }
        }
        let bar = first(d, PREVIEW_HEADER_CLASS).expect("the bar");
        let mut n = 0;
        walk(bar, &mut n);
        n
    };
    assert_eq!(icons(&with), icons(&without) + 1, "the tone's icon stands before the title");
}

#[test]
fn the_default_title_is_preview() {
    assert_eq!(Preview::default().title.as_str(), "Preview");
}
