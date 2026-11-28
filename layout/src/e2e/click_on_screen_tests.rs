//! A click by text lands where the text is DRAWN, in a page that has scrolled.
//!
//! The label inside a button has no hit-test area of its own, so the click op took the label's
//! laid-out box - in CONTENT coordinates - and pressed there: in a page scrolled by a few thousand
//! px that is far below the window, and nothing was hit. The showcase's tab scenarios lost two clicks
//! to it ("Pixel 8" put the window on no phone, "Review" selected no tab) on the host and on the
//! Android emulator alike, while a button clicked in a page whose BODY scrolls worked (there the
//! button's own box was found).

use azul_core::{callbacks::Update, dom::Dom, refany::RefAny, styled_dom::StyledDom};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::{
    callbacks::CallbackInfo,
    widgets::button::{Button, ButtonOnClickCallbackType, ButtonType},
};

/// How often the button's callback ran.
struct Clicks(std::sync::Arc<std::sync::atomic::AtomicUsize>);

extern "C" fn count_a_click(data: RefAny, _info: CallbackInfo) -> Update {
    let mut data = data;
    if let Some(clicks) = data.downcast_ref::<Clicks>() {
        clicks.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    Update::DoNothing
}

fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("  step {} `{}`: {}", s.step_index, s.op, s.error.clone().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The showcase's structure: a column that fills the window, bars above, and a scroller of its own
/// (`overflow-y: auto; flex-grow: 1; min-height: 0; padding: 24px`) holding `sections` tall boxes,
/// then a caption and a row of two buttons, then more boxes. The second button counts its clicks.
fn page(sections: usize, clicks: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> StyledDom {
    let on_click: ButtonOnClickCallbackType = count_a_click;
    let boxed = |text: String| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; margin-bottom: 20px; padding: 16px;")
            .with_child(Dom::create_p_with_text(text).with_css("height: 80px; margin: 0px;"))
    };
    let mut scroller = Dom::create_div().with_css(
        "display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; min-height: 0; padding: 24px;",
    );
    for i in 0..sections {
        scroller = scroller.with_child(boxed(format!("Section {i}")));
    }
    scroller = scroller.with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column;")
            .with_child(Dom::create_p_with_text("A row to click in".to_string()))
            .with_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; gap: 8px; flex-wrap: wrap;")
                    .with_child(Button::with_type("First".into(), ButtonType::Default).dom())
                    .with_child(
                        Button::with_type("Second".into(), ButtonType::Default)
                            .with_on_click(RefAny::new(Clicks(clicks.clone())), on_click)
                            .dom(),
                    ),
            ),
    );
    for i in 0..6 {
        scroller = scroller.with_child(boxed(format!("After {i}")));
    }
    let mut dom = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column; height: 100%;")
        .with_child(Dom::create_div().with_css("height: 32px; flex-shrink: 0;"))
        .with_child(Dom::create_div().with_css("height: 60px; flex-shrink: 0;"))
        .with_child(scroller);
    StyledDom::create(&mut dom, azul_css::parser2::new_from_str("").0)
}

fn click_second_button(sections: usize) -> usize {
    let clicks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "click_by_text_in_a_scrolled_page",
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 640, "window_height": 400, "dpi": 96 },
        "steps": [
            { "op": "wait_frame" },
            { "op": "scroll_into_view", "text": "A row to click in", "block": "center", "behavior": "instant" },
            { "op": "wait_frame" },
            { "op": "click", "text": "Second" },
            { "op": "wait_frame" },
            { "op": "wait_frame" },
        ],
    }))
    .expect("the scenario literal is a valid E2eTest");
    let result = run_e2e_test_with_dom(&test, Some(page(sections, &clicks)));
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(&result)
    );
    clicks.load(std::sync::atomic::Ordering::SeqCst)
}

#[test]
fn a_click_by_text_on_a_button_lands_in_a_page_scrolled_far_down() {
    assert_eq!(click_second_button(30), 1, "the button 4000 px down the page was pressed once");
}

#[test]
fn a_click_by_text_on_a_button_lands_in_a_page_that_hardly_scrolls() {
    assert_eq!(click_second_button(3), 1, "the button a screen down the page was pressed once");
}
