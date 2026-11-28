//! The BottomSheet's DOM and what a rebuild carries over: the sheet's heights from its own numbers, the grip's name and
//! value, what is wired where, the key of its root. (The rules of the detents are `detents_test.rs`.)

use azul_core::dom::TabIndex;

use super::*;

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_class() == Some(class))
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

fn sheet() -> BottomSheet {
    BottomSheet::create(Dom::create_div().with_id(AzString::from("content")))
        .with_label(AzString::from("Place"))
        .with_container_height(800.0)
}

fn state(detent: SheetDetent, height: f32) -> DragState {
    DragState {
        on_event: OptionBottomSheetOnEvent::None,
        detent,
        lowest: SheetDetent::Collapsed,
        highest: SheetDetent::Full,
        collapsed: 0.0,
        peek: 0.0,
        top_margin: 0.0,
        container: 800.0,
        grab: GRIP_HEIGHT,
        height,
        armed: false,
        on_grip: false,
        dragging: false,
        start_y: 0.0,
        start_height: height,
        last_y: 0.0,
        step: 0.0,
    }
}

// ---- the heights of this sheet ----

#[test]
fn the_sheets_own_numbers_set_the_heights_of_its_detents() {
    let s = sheet().with_peek_height(120.0).with_collapsed_height(36.0).with_top_margin(100.0);
    assert!(near(s.height_of(SheetDetent::Collapsed), 36.0));
    assert!(near(s.height_of(SheetDetent::Peek), 120.0));
    assert!(near(s.height_of(SheetDetent::Half), 400.0));
    assert!(near(s.height_of(SheetDetent::Full), 700.0));
}

#[test]
fn a_sheet_that_names_nothing_has_the_wireframes_numbers() {
    let s = sheet();
    assert!(near(s.height_of(SheetDetent::Collapsed), 30.0) && near(s.height_of(SheetDetent::Peek), 130.0));
    assert!(near(s.height_of(SheetDetent::Half), 400.0) && near(s.height_of(SheetDetent::Full), 680.0));
}

#[test]
fn a_detent_outside_the_range_has_the_height_of_the_nearest_in_it() {
    let s = sheet().with_lowest(SheetDetent::Peek).with_highest(SheetDetent::Half);
    assert!(near(s.height_of(SheetDetent::Collapsed), 130.0));
    assert!(near(s.height_of(SheetDetent::Full), 400.0));
}

// ---- the DOM ----

#[test]
fn the_root_holds_the_grip_then_the_content_and_wears_its_detents_class() {
    let dom = sheet().dom();
    assert!(has_class(&dom, BOTTOM_SHEET_CLASS));
    assert!(has_class(&dom, BOTTOM_SHEET_PEEK_CLASS));
    let kids = dom.children.as_ref();
    assert_eq!(kids.len(), 2);
    assert!(has_class(&kids[0], BOTTOM_SHEET_GRIP_CLASS));
    assert!(has_class(&kids[1], BOTTOM_SHEET_CONTENT_CLASS));
    assert!(first(&kids[0], BOTTOM_SHEET_PILL_CLASS).is_some(), "the grip has its pill");
    for (detent, class) in [
        (SheetDetent::Collapsed, BOTTOM_SHEET_COLLAPSED_CLASS),
        (SheetDetent::Half, BOTTOM_SHEET_HALF_CLASS),
        (SheetDetent::Full, BOTTOM_SHEET_FULL_CLASS),
    ] {
        assert!(has_class(&sheet().with_detent(detent).dom(), class), "{detent:?}");
    }
}

#[test]
fn the_app_s_content_is_inside_the_scrolling_part() {
    let dom = sheet().dom();
    let content = first(&dom, BOTTOM_SHEET_CONTENT_CLASS).expect("the content");
    let inner = &content.children.as_ref()[0];
    assert!(inner.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id() == Some("content")));
}

#[test]
fn a_detent_the_app_ruled_out_is_not_built() {
    let high = sheet().with_detent(SheetDetent::Full).with_highest(SheetDetent::Half).dom();
    assert!(has_class(&high, BOTTOM_SHEET_HALF_CLASS) && !has_class(&high, BOTTOM_SHEET_FULL_CLASS));
    let low = sheet().with_detent(SheetDetent::Collapsed).with_lowest(SheetDetent::Peek).dom();
    assert!(has_class(&low, BOTTOM_SHEET_PEEK_CLASS) && !has_class(&low, BOTTOM_SHEET_COLLAPSED_CLASS));
    // a range named the wrong way round is the same range: collapsed to half
    let swapped = sheet().with_detent(SheetDetent::Full).with_lowest(SheetDetent::Half).with_highest(SheetDetent::Collapsed).dom();
    assert!(has_class(&swapped, BOTTOM_SHEET_HALF_CLASS) && !has_class(&swapped, BOTTOM_SHEET_FULL_CLASS));
}

#[test]
fn the_roots_css_names_its_height_and_how_far_it_lies_above_the_bottom() {
    let css = root_css(148.0, 0.0);
    assert!(css.contains("height: 148px;") && css.contains("bottom: 0px;"), "{css}");
    assert!(css.contains("position: absolute;"), "an overlay, not a split: {css}");
    assert!(css.contains("border-radius: 16px 16px 0px 0px;"));
    assert!(root_css(400.0, 312.0).contains("bottom: 312px;"), "the keyboard lifts the sheet");
    assert!(root_css(400.0, -5.0).contains("bottom: 0px;"), "an inset below nothing is nothing");
}

#[test]
fn the_grip_is_a_named_focusable_splitter_with_the_share_of_the_container_as_its_value() {
    let dom = sheet().with_grip_label(AzString::from("Resize the place sheet")).with_detent(SheetDetent::Half).dom();
    let grip = first(&dom, BOTTOM_SHEET_GRIP_CLASS).expect("a grip");
    let a11y = grip.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Grip);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Resize the place sheet"));
    assert_eq!(a11y.accessibility_value.as_ref().map(|n| n.as_str()), Some("50"));
    assert_eq!(grip.root.get_tab_index(), Some(TabIndex::Auto));
    // without a name of its own the grip takes the sheet's
    let plain = sheet().dom();
    let named = first(&plain, BOTTOM_SHEET_GRIP_CLASS).expect("a grip").root.get_accessibility_info().expect("info");
    assert_eq!(named.accessibility_name.as_ref().map(|n| n.as_str()), Some("Place"));
}

#[test]
fn the_sheet_is_a_named_pane() {
    let dom = sheet().dom();
    let a11y = dom.root.get_accessibility_info().expect("an a11y info");
    assert_eq!(a11y.role, AccessibilityRole::Pane);
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Place"));
}

#[test]
fn every_pointer_event_is_wired_on_the_root_and_the_keys_on_the_grip_only() {
    let dom = sheet().dom();
    let wired: Vec<(EventFilter, usize)> =
        dom.root.callbacks.as_ref().iter().map(|c| (c.event, c.callback.cb)).collect();
    let expected: Vec<(EventFilter, usize)> = vec![
        (EventFilter::Hover(HoverEventFilter::MouseDown), on_press as usize),
        (EventFilter::Hover(HoverEventFilter::MouseOver), on_move as usize),
        (EventFilter::Hover(HoverEventFilter::MouseUp), on_release as usize),
        (EventFilter::Hover(HoverEventFilter::MouseLeave), on_leave as usize),
        (EventFilter::Hover(HoverEventFilter::TouchStart), on_press as usize),
        (EventFilter::Hover(HoverEventFilter::TouchMove), on_move as usize),
        (EventFilter::Hover(HoverEventFilter::TouchEnd), on_release as usize),
    ];
    assert_eq!(wired, expected);
    let grip = first(&dom, BOTTOM_SHEET_GRIP_CLASS).expect("a grip");
    let keys: Vec<(EventFilter, usize)> =
        grip.root.callbacks.as_ref().iter().map(|c| (c.event, c.callback.cb)).collect();
    assert_eq!(keys, vec![(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), on_grip_key as usize)]);
    let content = first(&dom, BOTTOM_SHEET_CONTENT_CLASS).expect("the content");
    assert!(content.root.callbacks.as_ref().is_empty(), "the content keeps its own events");
}

#[test]
fn one_state_is_shared_by_every_callback_of_the_sheet() {
    let dom = sheet().dom();
    let mut first_cb = dom.root.callbacks.as_ref()[0].refany.clone();
    let mut last_cb = dom.root.callbacks.as_ref()[6].refany.clone();
    {
        let mut st = first_cb.downcast_mut::<DragState>().expect("the sheet's state");
        st.armed = true;
        st.start_y = 42.0;
    }
    let seen = last_cb.downcast_ref::<DragState>().expect("the same state");
    assert!(seen.armed && near(seen.start_y, 42.0), "press, move and release observe one drag");
}

#[test]
fn the_state_starts_at_the_built_detent_with_the_numbers_of_the_build() {
    let dom = sheet().with_detent(SheetDetent::Half).with_peek_height(120.0).with_top_margin(80.0).with_grab_height(90.0).dom();
    let mut cb = dom.root.callbacks.as_ref()[0].refany.clone();
    let st = cb.downcast_ref::<DragState>().expect("state");
    assert_eq!(st.detent, SheetDetent::Half);
    assert!(near(st.height, 400.0) && near(st.peek, 120.0) && near(st.top_margin, 80.0));
    assert!(near(st.grab, 90.0), "the band that drags is the one the app named");
    assert!(!st.armed && !st.dragging);
    let plain = sheet().dom();
    let mut cb = plain.root.callbacks.as_ref()[0].refany.clone();
    assert!(near(cb.downcast_ref::<DragState>().expect("state").grab, GRIP_HEIGHT), "the grip alone by default");
}

// ---- a rebuild during a drag ----

#[test]
fn a_rebuild_during_a_drag_keeps_the_drag_and_the_height_the_pointer_made() {
    let mut old = state(SheetDetent::Half, 520.0);
    old.armed = true;
    old.dragging = true;
    old.start_y = 600.0;
    old.start_height = 148.0;
    old.last_y = 228.0;
    old.step = 9.0;
    let new = state(SheetDetent::Peek, 148.0);
    let mut merged = merge_sheet_state(RefAny::new(new), RefAny::new(old));
    let st = merged.downcast_ref::<DragState>().expect("state");
    assert!(st.armed && st.dragging);
    assert!(near(st.height, 520.0) && st.detent == SheetDetent::Half, "the pointer wins while it is down");
    assert!(near(st.start_y, 600.0) && near(st.start_height, 148.0) && near(st.last_y, 228.0) && near(st.step, 9.0));
}

#[test]
fn a_rebuild_after_the_pointer_is_up_takes_the_apps_detent() {
    let old = state(SheetDetent::Half, 520.0);
    let new = state(SheetDetent::Peek, 148.0);
    let mut merged = merge_sheet_state(RefAny::new(new), RefAny::new(old));
    let st = merged.downcast_ref::<DragState>().expect("state");
    assert!(!st.armed && !st.dragging);
    assert!(near(st.height, 148.0) && st.detent == SheetDetent::Peek, "the app's number is the truth again");
}

#[test]
fn a_rebuild_in_the_middle_of_a_press_that_is_not_yet_a_drag_keeps_the_press() {
    let mut old = state(SheetDetent::Peek, 148.0);
    old.armed = true;
    old.on_grip = true;
    old.start_y = 700.0;
    let new = state(SheetDetent::Peek, 148.0);
    let mut merged = merge_sheet_state(RefAny::new(new), RefAny::new(old));
    let st = merged.downcast_ref::<DragState>().expect("state");
    assert!(st.armed && st.on_grip && !st.dragging, "a tap on the grip still lands after a rebuild");
    assert!(near(st.start_y, 700.0));
}

#[test]
fn the_callback_of_the_fresh_build_is_the_one_that_survives_a_merge() {
    extern "C" fn mine(_: RefAny, _: CallbackInfo, _: BottomSheetEvent) -> Update {
        Update::DoNothing
    }
    let mut new = state(SheetDetent::Peek, 148.0);
    new.on_event = OptionBottomSheetOnEvent::Some(BottomSheetOnEvent {
        refany: RefAny::new(7_u32),
        callback: (mine as BottomSheetOnEventCallbackType).into(),
    });
    let mut old = state(SheetDetent::Half, 400.0);
    old.armed = true;
    old.dragging = true;
    let mut merged = merge_sheet_state(RefAny::new(new), RefAny::new(old));
    assert!(merged.downcast_ref::<DragState>().expect("state").on_event.is_some());
}

#[test]
fn a_sheet_taken_out_leaves_an_empty_one() {
    let mut s = sheet().with_detent(SheetDetent::Full);
    let taken = s.swap_with_default();
    assert_eq!(taken.detent, SheetDetent::Full);
    assert_eq!(s.detent, SheetDetent::Peek);
    assert!(s.label.as_str().is_empty());
}

#[test]
fn the_node_is_a_new_one_when_the_detent_or_its_height_changes_so_a_drags_patch_is_not_carried_over() {
    let key = |s: BottomSheet| s.dom().root.key;
    let peek = key(sheet());
    assert!(peek.is_some(), "the sheet's root is keyed");
    assert_eq!(peek, key(sheet()), "the same sheet is the same node across a rebuild");
    assert_ne!(peek, key(sheet().with_detent(SheetDetent::Half)), "another detent");
    assert_ne!(peek, key(sheet().with_peek_height(120.0)), "another height for the same detent");
    assert_ne!(
        key(sheet().with_detent(SheetDetent::Half)),
        key(sheet().with_detent(SheetDetent::Half).with_container_height(1200.0)),
        "the window grew: half is another number of px"
    );
    // a detent the range rules out is built as the one in range: the same node
    assert_eq!(key(sheet().with_detent(SheetDetent::Peek)), key(sheet().with_detent(SheetDetent::Collapsed).with_lowest(SheetDetent::Peek)));
}
