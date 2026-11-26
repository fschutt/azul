//! A window-level MouseEnter or MouseLeave listener hears the window's own events - the pointer entering or
//! leaving the OS window - and not the enter or leave of an element that gained or lost the hover.
//!
//! The pointer crossing a page gains and loses the hover of the nodes under it, and each node that lost it has its
//! own `MouseLeave` (each one that gained it its own `MouseEnter`). The dispatchers collected the window listeners
//! of the `MouseLeave` filter for every one of those events: they asked planning, which probes with an event
//! aimed at the root, and never the event itself. An app that ended a stroke when the pointer left the window ended
//! it at every element the pointer crossed; one that saved its session on a window leave saved it every few hundred
//! milliseconds of movement (a real window of AzReview: 20 in 3 seconds of moves over its pages).

use std::sync::Mutex;

use azul_core::{
    callbacks::Update,
    dom::Dom,
    events::{EventFilter, HoverEventFilter, WindowEventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::run_e2e_tests_in_one_window;
use crate::callbacks::CallbackInfo;

/// What the callbacks saw, by name, in order. One test at a time takes it.
static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static LOCK: Mutex<()> = Mutex::new(());

fn seen(name: &'static str) {
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).push(name);
}

macro_rules! recorder {
    ($fn_name:ident, $label:literal) => {
        extern "C" fn $fn_name(_data: RefAny, _info: CallbackInfo) -> Update {
            seen($label);
            Update::DoNothing
        }
    };
}
recorder!(window_left, "window MouseLeave");
recorder!(window_entered, "window MouseEnter");
recorder!(a_left, "a MouseLeave");
recorder!(b_left, "b MouseLeave");

/// body > div#a, div#b side by side (200 x 100 each, from the origin), the body listening for the window's
/// enter and leave and each box for its own leave.
fn page() -> StyledDom {
    let mut dom = Dom::create_body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::MouseLeave),
            RefAny::new(()),
            window_left as usize,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::MouseEnter),
            RefAny::new(()),
            window_entered as usize,
        )
        .with_child(Dom::create_div().with_id("a".into()).with_callback(
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            RefAny::new(()),
            a_left as usize,
        ))
        .with_child(Dom::create_div().with_id("b".into()).with_callback(
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            RefAny::new(()),
            b_left as usize,
        ));
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { display: flex; flex-direction: row; width: 400px; height: 300px; } \
         div { width: 200px; height: 100px; }",
    );
    StyledDom::create(&mut dom, css)
}

fn count(seen: &[&str], name: &str) -> usize {
    seen.iter().filter(|s| **s == name).count()
}

/// The pointer goes into the window over box a, across to b, back to a, and to b again: box a loses the hover
/// twice, box b once, and the pointer never leaves the window.
///
/// Mutation: the dispatcher collecting the window listeners of every `MouseLeave` (the code before) - the body's
/// window listener runs for the three element leaves, and for every element's `MouseEnter` too.
#[test]
fn a_window_listener_does_not_hear_an_element_losing_the_hover() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let tests: Vec<super::E2eTest> = serde_json::from_value(serde_json::json!([
        {
            "name": "the_pointer_crosses_two_boxes",
            "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "mouse_move", "x": 50.0, "y": 50.0 },
                { "op": "wait_frame" },
                { "op": "mouse_move", "x": 250.0, "y": 50.0 },
                { "op": "wait_frame" },
                { "op": "mouse_move", "x": 50.0, "y": 50.0 },
                { "op": "wait_frame" },
                { "op": "mouse_move", "x": 250.0, "y": 50.0 },
                { "op": "wait_frame" }
            ]
        }
    ]))
    .expect("scenario json");
    let (results, _runner) = run_e2e_tests_in_one_window(
        &tests,
        Some(page()),
        None,
        None,
        &crate::settings::LayoutSettings::default(),
    );
    let results = results.expect("the run answers");
    assert_eq!(results[0].status, "pass", "{:#?}", results[0].steps);
    let seen = SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // The premise: each element still hears its own leave.
    assert_eq!(count(&seen, "a MouseLeave"), 2, "box a lost the hover twice: {seen:?}");
    assert_eq!(count(&seen, "b MouseLeave"), 1, "box b lost the hover once: {seen:?}");
    // The pointer stayed in the window: no leave for a window listener, and its enter at most the once the
    // pointer came in.
    assert_eq!(count(&seen, "window MouseLeave"), 0, "the pointer never left the window: {seen:?}");
    assert!(
        count(&seen, "window MouseEnter") <= 1,
        "the pointer came into the window once: {seen:?}"
    );
}
