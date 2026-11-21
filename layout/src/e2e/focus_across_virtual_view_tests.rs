//! A press focuses the NEAREST FOCUSABLE ANCESTOR of what it hit - and the
//! walk goes on past a `VirtualView` page's root at the node hosting it.
//!
//! User ruling 2026-10-03 ("yeah, as you decided, nearest focus parent"): a
//! child dom is content of its host the way a shadow tree is content of its
//! host element, so a click on plain text inside a `VirtualView` (a progress
//! bar, a list row, a document page) focuses the focusable box around the
//! view - exactly as a click on plain text inside a focusable `<div>` focuses
//! that `<div>`. EVENTS7 made events bubble through the host
//! (`core::events::get_event_path`); the focus choice
//! (`managers::hover::focusable_under_pointer`) still stopped at the child
//! dom's root and BLURRED instead.
//!
//! Driven through the REAL dispatcher on a headless window (the `click` op:
//! move, press, release), so the runner's click-to-focus block - the mirror
//! of the dll's - is what decides.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId, OptionDom, TabIndex},
    geom::{LogicalPosition, LogicalRect},
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::run_e2e_test_keeping_runner;

/// The page's nodes: body (0) > div#host, focusable (1) > the `VirtualView` (2).
const HOST: NodeId = NodeId::new(1);

/// The `VirtualView`'s child document: plain, unfocusable text at its origin.
extern "C" fn plain_text_page(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let page = LogicalRect::new(LogicalPosition::zero(), info.bounds.get_logical_size());
    VirtualViewReturn {
        dom: OptionDom::Some(
            Dom::create_body()
                .with_css("margin: 0; padding: 0; font-size: 16px;")
                .with_child(
                    Dom::create_div()
                        .with_css("margin: 0; padding: 0;")
                        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                            "plain text in a page",
                        )),
                ),
        ),
        materialized: page,
        virtual_rect: page,
    }
}

/// Click `(x, y)` on `dom` and return the finished runner's focused node.
fn focus_after_click(mut dom: Dom, x: f32, y: f32) -> Option<azul_core::dom::DomNodeId> {
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 200px; }",
    );
    let styled_dom = StyledDom::create(&mut dom, css);
    let test: super::E2eTest = serde_json::from_value(serde_json::json!({
        "name": "focus_nearest_focusable_ancestor",
        "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
        "steps": [
            { "op": "wait_frame" },
            { "op": "click", "x": x, "y": y },
            { "op": "wait_frame" }
        ]
    }))
    .expect("scenario json");
    let (result, runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
    assert_eq!(result.status, "pass", "{:#?}", result.steps);
    runner
        .layout_window
        .focus_manager
        .get_focused_node()
        .copied()
}

#[test]
fn a_click_on_plain_text_in_a_virtual_view_focuses_the_focusable_host_around_it() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_id("host".into())
            .with_tab_index(TabIndex::Auto)
            .with_css("width: 300px; height: 120px;")
            .with_child(
                Dom::create_virtual_view(
                    RefAny::new(()),
                    VirtualViewCallback::create(plain_text_page),
                )
                .with_css("width: 300px; height: 100px;"),
            ),
    );
    // On the first glyphs of the page's text: the front-most hit is in the
    // child dom, whose own chain (text > div > body) holds nothing focusable.
    let focused = focus_after_click(dom, 12.0, 8.0);
    assert_eq!(
        focused.map(|f| (f.dom, f.node.into_crate_internal())),
        Some((DomId::ROOT_ID, Some(HOST))),
        "the press on the page's plain text must focus div#host - the nearest focusable \
         ancestor, reached through the VirtualView that hosts the page - not blur"
    );
}

#[test]
fn a_click_on_plain_text_focuses_the_nearest_focusable_ancestor_in_its_own_dom() {
    // The plain case (no VirtualView): body (0) > div#host, focusable (1) >
    // div (2) > text (3).
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_id("host".into())
            .with_tab_index(TabIndex::Auto)
            .with_css("width: 300px; height: 120px;")
            .with_child(Dom::create_div().with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper("plain text"),
            )),
    );
    let focused = focus_after_click(dom, 12.0, 8.0);
    assert_eq!(
        focused.map(|f| (f.dom, f.node.into_crate_internal())),
        Some((DomId::ROOT_ID, Some(HOST))),
        "the press on plain text must focus its nearest focusable ancestor, div#host"
    );
}

#[test]
fn a_click_on_a_virtual_view_with_no_focusable_ancestor_anywhere_blurs() {
    // Nothing focusable on the whole path (page chain, host, host's
    // ancestors): the press is a blur - there is nothing to focus.
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("width: 300px; height: 120px;")
            .with_child(
                Dom::create_virtual_view(
                    RefAny::new(()),
                    VirtualViewCallback::create(plain_text_page),
                )
                .with_css("width: 300px; height: 100px;"),
            ),
    );
    assert_eq!(focus_after_click(dom, 12.0, 8.0), None);
}
