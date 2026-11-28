//! The system's style changing is ONE window event: `WindowEventFilter::SystemStyleChanged`.
//!
//! A window listener hears it once per change of the style the window lays out under - a text size, a
//! bold text, a contrast, a reduced motion - and not at the first layout, not when the same style is
//! handed in again (every layout pass does), and not twice for one change. An app that draws or measures
//! on its own (a canvas, a map, a chart) re-reads `get_accessibility_preferences` there.

use azul_core::{
    callbacks::Update,
    dom::Dom,
    events::{EventFilter, WindowEventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::run_e2e_tests_in_one_window;
use crate::callbacks::CallbackInfo;

/// What the listener heard: the app data it counts in.
struct Heard {
    count: u32,
}

extern "C" fn heard(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut heard) = data.downcast_mut::<Heard>() {
        heard.count += 1;
    }
    Update::DoNothing
}

/// A body listening for the system's style, and one box of text.
fn page(listener: &RefAny) -> StyledDom {
    let mut dom = Dom::create_body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::SystemStyleChanged),
            listener.clone(),
            heard as usize,
        )
        .with_child(Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("text")));
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 300px; }",
    );
    StyledDom::create(&mut dom, css)
}

/// Run `steps` in a headless window under the page above; how many times the listener was told.
fn told_by(steps: &serde_json::Value) -> u32 {
    let mut listener = RefAny::new(Heard { count: 0 });
    let tests: Vec<super::E2eTest> = serde_json::from_value(serde_json::json!([
        {
            "name": "system_style_event",
            "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
            "steps": steps,
        }
    ]))
    .expect("scenario json");
    let (results, _runner) = run_e2e_tests_in_one_window(
        &tests,
        Some(page(&listener)),
        None,
        None,
        &crate::settings::LayoutSettings::default(),
    );
    let results = results.expect("the run answers");
    assert_eq!(results[0].status, "pass", "{:#?}", results[0].steps);
    listener.downcast_ref::<Heard>().map_or(u32::MAX, |heard| heard.count)
}

fn set(options: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut op = options.clone();
    op["op"] = serde_json::json!("set_system_accessibility");
    vec![op, serde_json::json!({ "op": "wait_frame" })]
}

/// The first layout adopts the system's style: that is not a change, and frames that follow with the same
/// style are not either.
#[test]
fn a_window_that_only_lays_out_hears_no_style_change() {
    let steps = serde_json::json!([
        { "op": "wait_frame" },
        { "op": "wait_frame" },
        { "op": "wait_frame" },
    ]);
    assert_eq!(told_by(&steps), 0);
}

/// One event per change, in the frame of the change: a text size, the same text size again (nothing
/// changed), a bold text on top, and the reset back (the style differs again).
///
/// Mutation: raising the notice for every `set_system_style` (the same style handed in by every layout
/// pass) counts the repeated text size and the idle frames as changes.
#[test]
fn each_change_of_the_systems_style_is_one_event_and_a_repeat_is_none() {
    let mut steps = vec![serde_json::json!({ "op": "wait_frame" })];
    steps.extend(set(&serde_json::json!({ "text_scale": 2.0 })));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    // the same text size again: the style is the one it was
    steps.extend(set(&serde_json::json!({ "text_scale": 2.0 })));
    // another option on top of it
    steps.extend(set(&serde_json::json!({ "bold_text": true })));
    // and everything back to what the platform said
    steps.extend(set(&serde_json::json!({ "reset": true })));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    assert_eq!(told_by(&serde_json::Value::Array(steps)), 3);
}
