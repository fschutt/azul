//! What a plain mouse move over the text labels of a `VirtualView` costs on the headless
//! runner, as the map editor's scenario measures it: the labels are spans with a text and a
//! `:hover { background }` (paint only) in the child DOM, and the pointer goes along a row
//! of them. Nothing is laid out and no DOM is rebuilt; the list that is rebuilt for an enter
//! or a leave is the view's, which holds the label - the runner rebuilt the ROOT's list for
//! the display-list tier, and the label's hover never reached the view's list at all.

use azul_core::{
    callbacks::{Update, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, OptionDom},
    events::{EventFilter, HoverEventFilter},
    geom::{LogicalPosition, LogicalRect},
    refany::RefAny,
    styled_dom::StyledDom,
};
use azul_css::props::basic::color::ColorU;

use super::run_e2e_test_keeping_runner;
use crate::{callbacks::CallbackInfo, solver3::display_list::DisplayListItem};

/// What a label paints under the pointer: `rgba(66, 134, 244, 0.35)`.
const HOVER: ColorU = ColorU {
    r: 66,
    g: 134,
    b: 244,
    a: 89,
};

extern "C" fn label_enter(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}
extern "C" fn label_leave(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}
extern "C" fn host_move(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

/// Six rows of twelve labels, 36 px apart and 40 px between the rows: the label of row `r`,
/// column `c` is at (10 + 36 c, 20 + 40 r) in the view, 12 px high. The editor's label: a
/// span with its text, turned, a halo, a hover block last, and its enter / leave callbacks.
extern "C" fn map_page(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let page = LogicalRect::new(LogicalPosition::zero(), info.bounds.get_logical_size());
    let mut labels = Vec::new();
    for row in 0..6 {
        for col in 0..12 {
            let css = format!(
                "position: absolute; left: {}px; top: {}px; transform-origin: 0px 9px; \
                 transform: rotate(0deg); font-size: 12px; line-height: 1; letter-spacing: 0px; \
                 color: #222222; font-family: sans-serif; white-space: pre; cursor: pointer; \
                 text-shadow: 0px 0px 0px 1px #ffffff; \
                 :hover {{ background: rgba(66, 134, 244, 0.35); }}",
                10 + col * 36,
                20 + row * 40,
            );
            labels.push(
                Dom::create_span_with_text(format!("Ab{row}{col}"))
                    .with_css(&css)
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::MouseEnter),
                        RefAny::new(()),
                        label_enter as usize,
                    )
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::MouseLeave),
                        RefAny::new(()),
                        label_leave as usize,
                    ),
            );
        }
    }
    VirtualViewReturn {
        dom: OptionDom::Some(Dom::create_div().with_children(labels.into())),
        materialized: page,
        virtual_rect: page,
    }
}

fn map_dom() -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("position: relative; width: 500px; height: 300px; overflow: hidden;")
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseMove),
                RefAny::new(()),
                host_move as usize,
            )
            .with_child(
                Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(map_page))
                    .with_css("position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;"),
            ),
    );
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { width: 600px; height: 400px; }",
    );
    StyledDom::create(&mut dom, css)
}

#[test]
fn a_plain_move_along_the_text_labels_lays_nothing_out_and_paints_the_label_under_the_pointer() {
    let mut steps = vec![
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "mouse_move", "x": 5.0, "y": 5.0 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "reset_frame_counters" }),
    ];
    // 29 moves of 12 px along the first row (y = 28: its text), the pointer entering and
    // leaving a label every three moves; then onto the first label and still.
    for i in 0..29 {
        steps.push(serde_json::json!({ "op": "mouse_move", "x": 8.0 + 12.0 * i as f32, "y": 28.0 }));
        steps.push(serde_json::json!({ "op": "wait_frame" }));
    }
    steps.push(serde_json::json!({ "op": "mouse_move", "x": 14.0, "y": 28.0 }));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    steps.push(serde_json::json!({ "op": "get_frame_report" }));
    let test: super::E2eTest = serde_json::from_value(serde_json::json!({
        "name": "hover_text_cost",
        "setup": { "window_width": 600, "window_height": 400, "dpi": 96 },
        "steps": steps
    }))
    .expect("scenario json");
    let (result, runner) = run_e2e_test_keeping_runner(&test, Some(map_dom()));
    assert_eq!(result.status, "pass", "{:#?}", result.steps);

    let report = result
        .steps
        .iter()
        .rev()
        .find(|s| s.op == "get_frame_report")
        .and_then(|s| s.response.as_ref())
        .and_then(|v| v.get("value"))
        .expect("the frame report");
    let count = |key: &str| {
        report
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_else(|| panic!("the report has no {key}: {report}"))
    };
    assert_eq!(count("layout_passes"), 0, "a hover that changes a colour lays nothing out");
    assert_eq!(count("dom_regenerations"), 0);
    // A3-window-07: a label's hover background is painted into the list the view has
    // (`dl_node_patches`), which builds no list; a restyle the patch cannot show builds it.
    assert!(
        (1..=30).contains(&(count("dl_rebuilds") + count("dl_node_patches"))),
        "the view's list is rebuilt or patched for an enter or a leave, once a move at most: \
         {} + {}",
        count("dl_rebuilds"),
        count("dl_node_patches")
    );

    // The label under the pointer is in the view's list, painted in its hover colour:
    // that list is what the frame draws it from.
    let child = *runner
        .layout_window
        .layout_results
        .keys()
        .find(|dom| **dom != DomId::ROOT_ID)
        .expect("the view's DOM");
    let painted = runner.layout_window.layout_results[&child]
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Rect { color, .. } if *color == HOVER))
        .count();
    assert_eq!(painted, 1, "exactly the label under the pointer is painted in its hover colour");
}

/// A press on the text of a label places a collapsed cursor in text nobody can edit: it paints
/// nothing, so the runner rebuilds no list for it - the view's (the press rebuilt the list of the DOM
/// it placed the cursor in) and not the window's (the display-list tier it was answered with was served
/// with a rebuild of the root's list): two rebuilds a press before.
#[test]
fn a_press_on_the_text_of_a_label_rebuilds_no_list_on_the_runner() {
    let steps = vec![
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "mouse_move", "x": 14.0, "y": 28.0 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "click", "x": 14.0, "y": 28.0 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_frame_report" }),
    ];
    let test: super::E2eTest = serde_json::from_value(serde_json::json!({
        "name": "press_text_cost",
        "setup": { "window_width": 600, "window_height": 400, "dpi": 96 },
        "steps": steps
    }))
    .expect("scenario json");
    let (result, runner) = run_e2e_test_keeping_runner(&test, Some(map_dom()));
    assert_eq!(result.status, "pass", "{:#?}", result.steps);
    let report = result
        .steps
        .iter()
        .rev()
        .find(|s| s.op == "get_frame_report")
        .and_then(|s| s.response.as_ref())
        .and_then(|v| v.get("value"))
        .expect("the frame report");
    let rebuilds = report.get("dl_rebuilds").and_then(serde_json::Value::as_u64).expect("dl_rebuilds");
    assert_eq!(rebuilds, 0, "a press on a label rebuilds no list: {report}");
    assert!(
        runner.layout_window.text_edit_manager.multi_cursor.is_some(),
        "premise: the press opened a selection session"
    );
}
