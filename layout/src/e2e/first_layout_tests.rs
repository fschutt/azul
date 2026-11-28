//! The debug timer serves nothing until its window has laid out once.
//!
//! `AZ_E2E=<file>` queues a file's scenarios in the debug channel when the process starts, before
//! its window exists, and the window's debug timer takes them at its first tick. On a phone that
//! tick can come before the first layout (the Android shell registers the timer when the window's
//! surface is created and lays out after it): step 0 of a scenario ran against a window without a
//! document and failed with "no node" for every scenario that opens on a node of the app (an
//! `assert_text` on the counter, a `click` on a button) - the same file that passes on the desktop.
//!
//! A child of `runner`: it builds the headless `Runner` and drives the timer through `Timer::invoke`,
//! as the shells do.

use std::sync::{mpsc, Arc, Mutex};

use azul_core::{
    dom::{Dom, IdOrClass},
    refany::RefAny,
    styled_dom::StyledDom,
    task::{advance_test_clock_ms, freeze_test_clock, reset_test_clock},
    xml::ComponentMap,
};
use azul_layout::callbacks::{CallbackChange, ExternalSystemCallbacks};

use super::Runner;
use crate::e2e::{
    create_debug_timer, DebugEvent, DebugRequest, DebugResponseData, E2eTest, ResponseData,
};

/// One `div#go` holding the text "Go": the node a scenario that opens on the app's own DOM asserts.
fn the_apps_document() -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Id("go".into())].into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Go")),
    );
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 200px; }",
    );
    StyledDom::create(&mut dom, css)
}

/// A scenario whose step 0 needs the app's node: what `hello_world_counter.json` opens with.
fn a_scenario_that_opens_on_a_node() -> E2eTest {
    serde_json::from_value(serde_json::json!({
        "name": "opens_on_a_node",
        "steps": [{ "op": "assert_text", "selector": "#go", "expected": "Go" }],
    }))
    .expect("the scenario literal is a valid E2eTest")
}

/// The debug channel of a process that was started with `AZ_E2E`: the scenario is queued, no window
/// exists yet. -> (the timer of the window that will be made, the answer of the queued request)
fn a_timer_with_a_queued_scenario(
    window_id: &str,
) -> (crate::timer::Timer, mpsc::Receiver<DebugResponseData>) {
    let (mut request_tx, request_rx) = spmc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    request_tx
        .send(DebugRequest {
            request_id: 1,
            event: DebugEvent::RunE2eTests {
                tests: vec![a_scenario_that_opens_on_a_node()],
                snapshots: None,
            },
            // Named: the process's registry of windows is shared by every test of the binary.
            window_id: Some(window_id.to_string()),
            wait_for_render: false,
            dom_id: None,
            response_tx,
            answered: Mutex::new(None),
        })
        .expect("the channel has a receiver");
    let timer = create_debug_timer(
        RefAny::new(()),
        ExternalSystemCallbacks::rust_internal().get_system_time_fn,
        request_rx,
        Arc::new(Mutex::new(ComponentMap::with_builtin())),
        window_id.to_string(),
    );
    (timer, response_rx)
}

/// One tick of the window's debug timer, as a shell makes it: past the timer's interval.
fn tick(runner: &Runner, timer: &mut crate::timer::Timer, changes: &Arc<Mutex<Vec<CallbackChange>>>) {
    advance_test_clock_ms(1000);
    let get_time = ExternalSystemCallbacks::rust_internal().get_system_time_fn;
    runner.with_callback_info(changes, |info| {
        let _ = timer.invoke(&*info, &get_time);
    });
}

/// Ticks the timer until the queued request is answered (a scenario may yield between steps: a few
/// ticks are what a shell gives it), at most `ticks` times.
fn serve_until_answered(
    runner: &Runner,
    timer: &mut crate::timer::Timer,
    changes: &Arc<Mutex<Vec<CallbackChange>>>,
    answers: &mpsc::Receiver<DebugResponseData>,
    ticks: usize,
) -> Option<DebugResponseData> {
    for _ in 0..ticks {
        tick(runner, timer, changes);
        if let Ok(answer) = answers.try_recv() {
            return Some(answer);
        }
    }
    None
}

/// The verdict of the one test the request ran, once the request is answered.
fn verdict_of(answer: DebugResponseData) -> (String, usize) {
    match answer {
        DebugResponseData::Ok {
            data: Some(ResponseData::E2eResults(results)),
            ..
        } => {
            let first = results.results.first().expect("the request ran one test");
            (first.status.clone(), first.steps_failed)
        }
        other => panic!("the request was answered with {other:?}, not with a test report"),
    }
}

/// The first tick of the timer comes before the window's first layout: the queued scenario must be
/// left in the channel, not run against a window without a document.
#[test]
fn a_scenario_queued_before_the_first_layout_waits_for_it() {
    reset_test_clock();
    freeze_test_clock();
    let mut runner = Runner::new(400.0, 200.0, 96, false);
    let (mut timer, answers) = a_timer_with_a_queued_scenario("first-layout-waits");
    let changes = Arc::new(Mutex::new(Vec::new()));

    tick(&runner, &mut timer, &changes);
    tick(&runner, &mut timer, &changes);
    assert!(
        answers.try_recv().is_err(),
        "the scenario ran in a window that had not laid out (its step 0 cannot have found a node)"
    );

    runner.layout(the_apps_document(), true);
    let answer = serve_until_answered(&runner, &mut timer, &changes, &answers, 8)
        .expect("the ticks after the first layout serve the queued scenario");
    assert_eq!(
        verdict_of(answer),
        ("pass".to_string(), 0),
        "the scenario that waited for the first layout finds its node"
    );
    reset_test_clock();
}

/// The guard waits for the first layout and for nothing else: a window that has laid out serves a
/// queued scenario.
#[test]
fn a_window_that_has_laid_out_serves_a_queued_scenario() {
    reset_test_clock();
    freeze_test_clock();
    let mut runner = Runner::new(400.0, 200.0, 96, false);
    runner.layout(the_apps_document(), true);
    let (mut timer, answers) = a_timer_with_a_queued_scenario("first-layout-ready");
    let changes = Arc::new(Mutex::new(Vec::new()));

    let answer = serve_until_answered(&runner, &mut timer, &changes, &answers, 8)
        .expect("a window that has laid out serves the scenario");
    assert_eq!(verdict_of(answer), ("pass".to_string(), 0));
    reset_test_clock();
}
