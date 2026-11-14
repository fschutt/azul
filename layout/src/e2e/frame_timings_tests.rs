//! A window reports how long its frames take (`CallbackInfo::get_frame_timings`,
//! `LayoutWindow::frame_timings`), through the real frame loop of a headless
//! window: an app that adapts its own work to the frame rate had no clock for
//! it - `AZ_PROFILE`'s spans are for a profiler, not for the app.

use azul_core::{dom::Dom, styled_dom::StyledDom};

use super::run_e2e_test_keeping_runner;

fn page() -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("frames")),
    );
    let (css, _) = azul_css::parser2::new_from_str("body { width: 300px; height: 200px; }");
    StyledDom::create(&mut dom, css)
}

/// After frames were presented: their count, the layout pass that made
/// them, and the work of the first one (it started with that pass).
#[test]
fn a_window_times_the_frames_it_presents() {
    let test: super::E2eTest = serde_json::from_value(serde_json::json!({
        "name": "frame_timings",
        "setup": { "window_width": 300, "window_height": 200, "dpi": 96 },
        "steps": [
            { "op": "wait_frame" },
            { "op": "mouse_move", "x": 10.0, "y": 10.0 },
            { "op": "wait_frame" },
            { "op": "wait_frame" }
        ]
    }))
    .expect("scenario json");
    let (result, runner) = run_e2e_test_keeping_runner(&test, Some(page()));
    assert_eq!(result.status, "pass", "{:#?}", result.steps);
    let timings = runner.layout_window.frame_timings();
    assert!(timings.frame_count >= 1, "{timings:?}");
    assert!(
        timings.layout_us > 0,
        "a layout pass takes more than a microsecond: {timings:?}"
    );
}
