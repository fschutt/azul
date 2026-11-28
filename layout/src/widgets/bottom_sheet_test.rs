//! The BottomSheet's DOM and what a rebuild carries over: the sheet's heights from its own numbers, the grip's name and
//! value, what is wired where, the key of its root. (The rules of the detents are `detents_test.rs`.)

use std::sync::{Arc, Mutex};

use azul_core::{
    dom::{DomId, NodeId, TabIndex},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};

use super::*;
use crate::{callbacks::CallbackChange, widgets::roving::test_support as rv};

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
        inset: 0.0,
        grab: GRIP_HEIGHT,
        height,
        armed: false,
        on_grip: false,
        dragging: false,
        start_y: 0.0,
        start_height: height,
        last_y: 0.0,
        step: 0.0,
        last_ms: 0,
        origin: None,
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
fn the_root_keeps_one_identity_whatever_detent_or_height_the_app_builds_it_at() {
    // A drag's override follows the node's identity through a rebuild; the override is handed back when the pointer is up, so
    // nothing of it reaches the next build and the key need not change with the detent.
    let key = |s: BottomSheet| s.dom().root.get_key();
    let peek = key(sheet());
    assert!(peek.is_some(), "the sheet's root is keyed");
    assert_eq!(peek, key(sheet()), "the same sheet is the same node across a rebuild");
    assert_eq!(peek, key(sheet().with_detent(SheetDetent::Half)), "another detent is the same node");
    assert_eq!(peek, key(sheet().with_peek_height(120.0).with_container_height(1200.0)), "another height too");
}

// ---- what a key on the grip does to the node ----

type Log = Arc<Mutex<Vec<BottomSheetEvent>>>;

extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: BottomSheetEvent) -> Update {
    if let Some(log) = data.downcast_ref::<Log>() {
        log.lock().expect("log").push(event);
    }
    Update::DoNothing
}

/// The styled DOM of a sheet at `detent` that logs what it tells the app, and the node of its grip.
fn logged(detent: SheetDetent) -> (StyledDom, DomNodeId, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let styled = StyledDom::create_from_dom(
        sheet()
            .with_detent(detent)
            .with_on_event(RefAny::new(log.clone()), record as BottomSheetOnEventCallbackType)
            .dom(),
    );
    let grip = styled
        .node_data
        .as_ref()
        .iter()
        .position(|nd| nd.get_ids_and_classes().as_ref().iter().any(|c| c.as_class() == Some(BOTTOM_SHEET_GRIP_CLASS)))
        .expect("the grip is in the DOM");
    let node = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(grip))),
    };
    (styled, node, log)
}

/// The height writes of `changes`, in order: whether each is the override channel's, and what it writes.
fn height_writes(changes: &[CallbackChange]) -> Vec<(bool, CssProperty)> {
    changes
        .iter()
        .filter_map(|c| {
            let (overridden, properties) = match c {
                CallbackChange::ChangeNodeCssProperties { properties, .. } => (false, properties),
                CallbackChange::OverrideNodeCssProperties { properties, .. } => (true, properties),
                _ => return None,
            };
            properties
                .as_ref()
                .iter()
                .find(|p| p.get_type() == CssPropertyType::Height)
                .map(|p| (overridden, p.clone()))
        })
        .collect()
}

#[test]
fn a_key_on_the_grip_settles_the_sheet_writes_its_height_and_hands_the_override_back() {
    let (styled, grip, log) = logged(SheetDetent::Peek);
    let (_, changes) = rv::press(&styled, grip, VirtualKeyCode::Up, &[]).expect("the grip takes the key");
    assert!(rv::prevented(&changes), "the key's own action (scrolling) is cancelled");
    let events = log.lock().expect("log").clone();
    assert_eq!(events.len(), 1);
    assert_eq!((events[0].kind, events[0].detent), (BottomSheetEventKind::Settled, SheetDetent::Half));
    assert!(near(events[0].height, 400.0));
    // the node's own style takes 400 px, then the override that a drag may have left is dropped: the next build is the truth
    assert_eq!(
        height_writes(&changes),
        vec![
            (false, height_property(400.0)),
            (true, CssProperty::initial(CssPropertyType::Height)),
        ]
    );
}

#[test]
fn a_key_the_grip_does_not_know_is_not_taken() {
    let (styled, grip, log) = logged(SheetDetent::Peek);
    let (_, changes) = rv::press(&styled, grip, VirtualKeyCode::A, &[]).expect("the grip is wired");
    assert!(!rv::prevented(&changes) && height_writes(&changes).is_empty());
    assert!(log.lock().expect("log").is_empty());
}

#[test]
fn the_highest_detent_stays_when_the_key_goes_up_and_the_app_is_still_told() {
    let (styled, grip, log) = logged(SheetDetent::Full);
    let (_, changes) = rv::press(&styled, grip, VirtualKeyCode::Up, &[]).expect("the grip takes the key");
    let events = log.lock().expect("log").clone();
    assert_eq!(events.last().map(|e| e.detent), Some(SheetDetent::Full));
    assert_eq!(height_writes(&changes).first(), Some(&(false, height_property(680.0))));
}

// ---- the drag ----

/// A sheet at its peek (130 px) in an 800 px box, its top edge at 670.
fn peeking() -> DragState {
    state(SheetDetent::Peek, 130.0)
}

#[test]
fn a_press_on_the_grip_arms_the_sheet_and_a_tap_on_it_steps_a_detent_up() {
    let mut st = peeking();
    assert!(st.press(672.0, 670.0, 130.0, 800.0, 0));
    assert!(st.armed && st.on_grip && !st.dragging);
    assert!(near(st.start_y, 672.0) && near(st.start_height, 130.0));
    assert_eq!(st.lift(false, 0), Some(SheetDetent::Half), "a tap on the grip: the next detent up");
    assert!(!st.armed, "the press is over");
    assert_eq!(st.lift(false, 0), None, "and a second lift has nothing to settle");
}

#[test]
fn the_highest_detent_goes_round_to_the_lowest_on_a_tap() {
    let mut st = state(SheetDetent::Full, 680.0);
    assert!(st.press(122.0, 120.0, 680.0, 800.0, 0));
    assert_eq!(st.lift(false, 0), Some(SheetDetent::Collapsed));
}

#[test]
fn a_press_in_the_band_under_the_grip_is_armed_but_a_release_without_a_drag_belongs_to_what_is_there() {
    let mut st = peeking();
    st.grab = 90.0;
    assert!(st.press(730.0, 670.0, 130.0, 800.0, 0), "60 px down, inside the 90 px band");
    assert!(st.armed && !st.on_grip);
    assert_eq!(st.lift(false, 0), None, "a press and release that did not move reach the buttons in the band");
    assert!(!st.armed);
}

#[test]
fn a_press_outside_the_band_is_not_the_sheets() {
    let mut st = peeking();
    st.container = 777.0;
    assert!(!st.press(720.0, 670.0, 130.0, 800.0, 0), "50 px down is the content, not the grip");
    assert!(!st.press(660.0, 670.0, 130.0, 800.0, 0), "above the sheet is not the sheet");
    assert!(!st.press(f32::NAN, 670.0, 130.0, 800.0, 0));
    assert!(!st.armed && near(st.container, 777.0), "nothing of the sheet changed");
}

#[test]
fn a_press_takes_the_height_the_layout_gave_the_sheet_when_it_is_a_number() {
    let mut st = peeking();
    assert!(st.press(672.0, 670.0, 211.0, 900.0, 0));
    assert!(near(st.height, 211.0) && near(st.start_height, 211.0) && near(st.container, 900.0));
    let mut other = peeking();
    assert!(other.press(672.0, 670.0, f32::NAN, 800.0, 0));
    assert!(near(other.height, 130.0), "no number from the layout: the sheet's own");
}

#[test]
fn a_move_inside_the_slop_is_still_a_press_and_one_past_it_starts_the_drag() {
    let mut st = peeking();
    assert!(st.press(672.0, 670.0, 130.0, 800.0, 0));
    assert_eq!(st.drag_to(672.0 - TAP_SLOP, 0), None, "exactly the slop is not past it");
    assert!(!st.dragging);
    let first = st.drag_to(672.0 - TAP_SLOP - 1.0, 0).expect("past the slop");
    assert!(first.started && st.dragging);
    assert!(near(first.height, 139.0), "130 px and the 9 px the pointer went up: {first:?}");
    let second = st.drag_to(600.0, 0).expect("a drag in flight");
    assert!(!second.started);
    assert!(near(second.height, 202.0) && near(st.height, 202.0));
}

#[test]
fn a_move_without_a_press_does_nothing() {
    assert_eq!(peeking().drag_to(10.0, 0), None);
}

#[test]
fn the_pointer_going_up_grows_the_sheet_only_as_far_as_its_highest_detent_and_down_as_far_as_the_lowest() {
    let mut st = peeking();
    assert!(st.press(672.0, 670.0, 130.0, 800.0, 0));
    assert!(near(st.drag_to(-5000.0, 0).expect("a drag").height, 680.0), "no taller than full");
    assert!(near(st.drag_to(9000.0, 0).expect("a drag").height, 30.0), "no lower than collapsed");
    let mut ranged = peeking();
    ranged.lowest = SheetDetent::Peek;
    ranged.highest = SheetDetent::Half;
    assert!(ranged.press(672.0, 670.0, 130.0, 800.0, 0));
    assert!(near(ranged.drag_to(-5000.0, 0).expect("a drag").height, 400.0));
    assert!(near(ranged.drag_to(9000.0, 0).expect("a drag").height, 130.0));
}

#[test]
fn the_app_is_told_the_detent_the_sheet_is_nearest_to_while_it_is_dragged() {
    let mut st = peeking();
    assert!(st.press(672.0, 670.0, 130.0, 800.0, 0));
    st.drag_to(672.0 - 120.0, 0).expect("a drag");
    assert_eq!(st.detent, SheetDetent::Peek, "250 px: nearer 130 than 400");
    st.drag_to(672.0 - 150.0, 0).expect("a drag");
    assert_eq!(st.detent, SheetDetent::Half, "280 px: nearer 400 than 130");
}

#[test]
fn a_slow_drag_settles_on_the_nearest_detent_and_a_fling_carries_on() {
    let mut slow = peeking();
    assert!(slow.press(672.0, 670.0, 130.0, 800.0, 0));
    slow.drag_to(600.0, 0).expect("a drag");
    slow.drag_to(599.0, 0).expect("a drag");
    assert_eq!(slow.lift(false, 0), Some(SheetDetent::Peek), "203 px, moving a pixel at a time: the peek");
    assert!(!slow.dragging && !slow.armed);

    let mut fling = peeking();
    assert!(fling.press(672.0, 670.0, 130.0, 800.0, 0));
    fling.drag_to(650.0, 0).expect("a drag");
    assert_eq!(fling.lift(false, 0), Some(SheetDetent::Half), "152 px, last step 22 px: it would land at 416, nearest half");
}

#[test]
fn a_pointer_that_leaves_with_the_button_up_settles_a_drag_and_lets_a_tap_go() {
    let mut dragging = peeking();
    assert!(dragging.press(672.0, 670.0, 130.0, 800.0, 0));
    dragging.drag_to(600.0, 0).expect("a drag");
    dragging.drag_to(599.0, 0).expect("a drag");
    assert_eq!(dragging.lift(true, 0), Some(SheetDetent::Peek), "the drag ends where it was, as a release would");
    let mut tapping = peeking();
    assert!(tapping.press(672.0, 670.0, 130.0, 800.0, 0));
    assert_eq!(tapping.lift(true, 0), None, "a press that never moved is let go, it is no tap");
    assert!(!tapping.armed);
}

#[test]
fn the_sheet_rests_at_the_height_of_its_detent() {
    let mut st = peeking();
    assert!(near(st.rest(SheetDetent::Half), 400.0));
    assert!(st.detent == SheetDetent::Half && near(st.height, 400.0));
    st.container = 1000.0;
    assert!(near(st.rest(SheetDetent::Half), 500.0), "half of the box it floats in now");
}

#[test]
fn a_pointer_that_rested_before_it_was_lifted_has_no_fling() {
    // 152 px with the last step 22 px: moving, it would land at 416 (half); after more than FLING_STALE_MS of rest it is the nearest
    // detent to where it is: the peek.
    let mut moving = peeking();
    assert!(moving.press(672.0, 670.0, 130.0, 800.0, 0));
    moving.drag_to(650.0, 10).expect("a drag");
    assert_eq!(moving.lift(false, 10 + FLING_STALE_MS), Some(SheetDetent::Half), "the rest is not longer than the limit");
    let mut rested = peeking();
    assert!(rested.press(672.0, 670.0, 130.0, 800.0, 0));
    rested.drag_to(650.0, 10).expect("a drag");
    assert_eq!(rested.lift(false, 10 + FLING_STALE_MS + 1), Some(SheetDetent::Peek), "it stopped: no speed");
}

#[test]
fn the_second_report_of_one_move_is_no_step_and_does_not_make_a_stop_of_it() {
    // A phone reports a finger as a touch and as a mouse: the same y twice. The second must not zero the step the fling is made of.
    let mut st = peeking();
    assert!(st.press(672.0, 670.0, 130.0, 800.0, 0));
    assert!(st.drag_to(650.0, 5).is_some());
    assert_eq!(st.drag_to(650.0, 5), None, "the same place again");
    assert!(near(st.step, 22.0), "the step of the real move stays: {}", st.step);
    assert_eq!(st.lift(false, 5), Some(SheetDetent::Half), "and the fling it makes carries the sheet on");
}

// ---- the keyboard's room ----

#[test]
fn the_keyboard_takes_its_room_from_the_detents_the_sheet_never_reaches_under_it() {
    // 800 px box, a 300 px keyboard under the sheet: the room is 500 px; half of it, and the room less the top margin.
    let s = sheet().with_bottom_inset(300.0).with_top_margin(100.0);
    assert!(near(s.height_of(SheetDetent::Peek), 130.0));
    assert!(near(s.height_of(SheetDetent::Half), 250.0));
    assert!(near(s.height_of(SheetDetent::Full), 400.0), "the full sheet's top edge is 100 px from the box's top");
    // without a top margin it is 85 % of the room
    assert!(near(sheet().with_bottom_inset(300.0).height_of(SheetDetent::Full), 425.0));
    // the root keeps its own place: it lies above the keyboard
    assert!(root_css(250.0, 300.0).contains("bottom: 300px;"));
}

#[test]
fn a_keyboard_that_is_no_number_or_taller_than_the_box_leaves_a_pixel_of_room_not_a_failure() {
    assert!(near(room(800.0, f32::NAN), 800.0) && near(room(800.0, -50.0), 800.0));
    assert!(near(room(800.0, 5000.0), 1.0));
    assert!(near(room(0.0, 100.0), 700.0), "no container named: the default 800");
    assert!(near(room(f32::NAN, 0.0), 800.0));
    let s = sheet().with_bottom_inset(5000.0);
    assert!(s.height_of(SheetDetent::Full).is_finite() && s.height_of(SheetDetent::Full) >= 1.0);
}

#[test]
fn the_drag_state_measures_its_detents_over_the_same_room() {
    let dom = sheet().with_bottom_inset(300.0).with_top_margin(100.0).dom();
    let mut cb = dom.root.callbacks.as_ref()[0].refany.clone();
    let st = cb.downcast_ref::<DragState>().expect("state");
    assert!(near(st.inset, 300.0));
    let h = st.heights();
    assert!(near(h.half, 250.0) && near(h.full, 400.0), "{h:?}");
}
