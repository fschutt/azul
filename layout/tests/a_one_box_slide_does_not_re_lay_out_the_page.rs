//! A one-box slide does not re-lay out the page.
//!
//! Found by ANIM8 (wave 8) and measured again on the wave-7 AzWidgets
//! (`scripts/layoutperf8_tick_scenario_gen.py`): every frame of the switch
//! knob's 16 px `margin-left` slide cost 157-309 ms - 2853 text re-flows,
//! 12181 flex items laid out again, `root_layout_pass` 107-229 ms - for a knob
//! inside a 36x20 track, so a 150 ms glide showed one or two frames.
//!
//! The knob's containers are flex boxes all the way up to the body, so the
//! dirty knob is re-solved from the body (`promote_layout_roots_to_containers`)
//! - correct, a flex item's slot is its container's to decide. What made that
//! pass cost the whole page is what it found beside the knob's ancestors:
//! every relayout reconciles, the reconcile CLONES every clean node, and the
//! clone threw its flex measurements away (`clone_node_from_old` cleared its
//! `taffy_cache`). A flex item with an empty cache is measured and laid out
//! again - its subtree and its text with it - so the body's pass did the page.
//!
//! The rule pinned here: what one moving box costs does not grow with the
//! page. And reusing the clean boxes must not lose what they paint - text
//! beside a block sits in an anonymous block the reconcile builds anew every
//! time, and a cached box above it must still find it laid out.
//!
//! Not compiled by the author (house rule). Expected RED before the fix: the
//! first test (the cost doubles with the page), the third (the text beside a
//! block vanishes when a sibling restyles - reproduced on the prebuilt
//! azul-doc e2e runner, `text_count` 3 -> 2).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    layout::{LayoutMarginLeft, LayoutWidth},
    property::{CssProperty, CssPropertyType},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    overlay::ContentChange,
    probe::{Event, Probe},
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

// ---- the pages ----

/// An AzWidgets-like settings card: a heading, a label row with a button, a
/// paragraph, and text beside a block (an anonymous block holds the text).
/// An `edited` card's label reads differently.
fn card(i: usize, edited: bool) -> Dom {
    let label = if edited {
        format!("Label {i}: a setting whose description was edited")
    } else {
        format!("Label {i}: a setting with a longer description")
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; padding: 12px; margin-top: 8px;")
        .with_child(Dom::create_div_with_text(format!("Card {i}")).with_css("font-size: 18px;"))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(Dom::create_div_with_text(label).with_css("flex-grow: 1;"))
                .with_child(Dom::create_div_with_text("Press").with_css("padding: 4px 10px;")),
        )
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                "Paragraph {i}: lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
                 eiusmod tempor incididunt ut labore et dolore magna aliqua."
            )),
        ))
        .with_child(
            Dom::create_div()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    format!("Note {i}"),
                ))
                .with_child(Dom::create_div_with_text("details")),
        )
}

/// The card holding the switch: a label and a fixed-size track with its knob,
/// the knob `knob_px` from the track's left edge.
fn switch_card(knob_px: isize) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; padding: 18px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(Dom::create_div_with_text("Wi-Fi").with_css("flex-grow: 1;"))
                .with_child(
                    Dom::create_div()
                        .with_class("track".into())
                        .with_css("display: flex; width: 36px; height: 20px; padding: 2px;")
                        .with_child(Dom::create_div().with_class("knob".into()).with_css(
                            &format!("width: 16px; height: 16px; margin-left: {knob_px}px;"),
                        )),
                ),
        )
}

/// AzWidgets' shape: a menu bar above a full-height flex body, a scrolling
/// flex column inside it, the switch card first and `cards` cards below.
fn widgets_page(cards: usize) -> Dom {
    widgets_page_with(cards, 0, None)
}

/// [`widgets_page`] with the knob `knob_px` into its track and card `edited`
/// (if any) relabelled.
fn widgets_page_with(cards: usize, knob_px: isize, edited: Option<usize>) -> Dom {
    let mut content = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; \
             padding: 24px;",
        )
        .with_child(switch_card(knob_px));
    for i in 0..cards {
        content = content.with_child(card(i, edited == Some(i)));
    }
    Dom::create_html()
        .with_child(Dom::create_div().with_css("height: 26px;"))
        .with_child(
            Dom::create_body()
                .with_css("display: flex; flex-direction: column; height: 100%;")
                .with_child(content),
        )
}

// ---- the harness ----

/// A window that has laid the page out cold and once more warm: the pass
/// under test is an incremental one, as every animation frame is.
fn window(mut dom: Dom) -> LayoutWindow {
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    relayout(&mut lw);
    lw
}

fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

fn with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .expect("the node exists")
}

fn rect(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(node).into(),
    })
    .expect("laid out")
}

/// Restyle `node` the way an animation frame does (`tick_animations`): an
/// override, plus the node staged as layout dirt - no inline-style change for
/// the reconcile to notice - then lay the window out.
fn restyle(lw: &mut LayoutWindow, node: NodeId, prop: CssProperty) {
    let scope = prop.get_type().relayout_scope(false);
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        props: vec![prop],
        override_only: true,
    });
    lw.pending_css_dirty = Some((DomId::ROOT_ID, vec![(node, scope)]));
    relayout(lw);
}

/// Lay the window out with a REBUILT page - a new `StyledDom`, as an app's
/// `RefreshDom` hands one over - through the reconcile.
fn rebuild(lw: &mut LayoutWindow, mut dom: Dom) {
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the rebuilt page lays out");
}

/// Every node's box and every glyph `lw` paints are where a fresh window,
/// which reuses nothing, puts them for `page`.
fn assert_lays_out_like_a_fresh_window(lw: &LayoutWindow, page: Dom, what: &str) {
    let fresh = window(page);
    let nodes = |w: &LayoutWindow| {
        w.layout_results[&DomId::ROOT_ID]
            .styled_dom
            .node_data
            .as_container()
            .len()
    };
    assert_eq!(nodes(lw), nodes(&fresh), "harness: {what}: the same page");
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    for n in 0..nodes(lw) {
        let id = DomNodeId {
            dom: DomId::ROOT_ID,
            node: Some(NodeId::new(n)).into(),
        };
        let (is, fresh_box) = (lw.get_node_layout_rect(id), fresh.get_node_layout_rect(id));
        let same = match (is, fresh_box) {
            (Some(a), Some(b)) => {
                close(a.origin.x, b.origin.x)
                    && close(a.origin.y, b.origin.y)
                    && close(a.size.width, b.size.width)
                    && close(a.size.height, b.size.height)
            }
            (None, None) => true,
            _ => false,
        };
        assert!(
            same,
            "{what}: node {n} is laid out at {is:?}, a fresh window puts it at {fresh_box:?}"
        );
    }
    let (painted, fresh_painted) = (painted_glyphs(lw), painted_glyphs(&fresh));
    assert_eq!(
        painted.len(),
        fresh_painted.len(),
        "{what}: the window paints {} glyphs, a fresh window {}",
        painted.len(),
        fresh_painted.len()
    );
    for (a, b) in painted.iter().zip(&fresh_painted) {
        assert!(
            a.0 == b.0 && close(a.1, b.1) && close(a.2, b.2),
            "{what}: a glyph paints as {a:?}, a fresh window paints it as {b:?}"
        );
    }
}

/// Every glyph the window paints: `(glyph, x, y)`.
fn painted_glyphs(lw: &LayoutWindow) -> Vec<(u32, f32, f32)> {
    lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .flatten()
        .map(|g| (g.index, g.point.x, g.point.y))
        .collect()
}

/// What one knob frame cost, counted from the probe spans of its relayout.
#[derive(Debug, Clone, Copy)]
struct FrameCost {
    /// `root_layout_pass`: the frame ran a layout pass at all.
    layout_passes: usize,
    /// `text_layout_flow`: text runs broken into lines again.
    text_flows: usize,
    /// Flex / grid items laid out again (taffy cache misses, and final
    /// layouts that could not be served).
    flex_items_laid_out: usize,
    /// Formatting contexts laid out (`fc_block`, `fc_inline`,
    /// `fc_inline_block`, `fc_flex_grid`).
    formatting_contexts: usize,
}

fn count(events: &[Event], name: &str) -> usize {
    events.iter().filter(|e| e.name == name).count()
}

/// Lay out a page of `cards` cards, then move its knob by 8 px and count
/// what that one frame re-laid out.
fn knob_frame_cost(cards: usize) -> FrameCost {
    let mut lw = window(widgets_page(cards));
    let knob = with_class(&lw, "knob");
    let before = rect(&lw, knob);

    // Spans buffer only while recording is on; drain whatever an earlier
    // test on this thread left behind.
    Probe::set_recording(true);
    let _ = Probe::drain();
    restyle(
        &mut lw,
        knob,
        CssProperty::const_margin_left(LayoutMarginLeft::const_px(8)),
    );
    let events = Probe::drain();
    Probe::set_recording(false);

    let after = rect(&lw, knob);
    assert!(
        (after.origin.x - before.origin.x - 8.0).abs() < 0.5,
        "harness: the frame moved the knob by 8 px, {before:?} -> {after:?}"
    );
    FrameCost {
        layout_passes: count(&events, "root_layout_pass"),
        text_flows: count(&events, "text_layout_flow"),
        flex_items_laid_out: count(&events, "taffy_cache_get_miss")
            + count(&events, "taffy_final_layout_stale"),
        formatting_contexts: count(&events, "fc_block")
            + count(&events, "fc_inline")
            + count(&events, "fc_inline_block")
            + count(&events, "fc_flex_grid"),
    }
}

/// Put the recording flag back the way `AZ_PROFILE` asked for it (the same
/// derivation `probe_gate` uses), so `frame_perf` and `pagination_perf` in
/// this binary keep their profile.
fn restore_ambient_recording() {
    let ambient = azul_core::profile::cpu_enabled()
        || azul_core::profile::memory_enabled()
        || azul_core::profile::heap_enabled();
    Probe::set_recording(ambient);
    let _ = Probe::drain();
}

// ---- the tests ----

/// A knob frame costs what the knob's ancestors cost, however long the page.
///
/// The same switch on a page of 40 cards and on a page of 80: the frame that
/// moves its knob by 8 px re-flows no text and lays out no more flex items on
/// the long page than on the short one. Before the fix both counts doubled
/// with the page (every card's items measured and laid out again, every text
/// re-flowed - AzWidgets: 2853 re-flows per frame).
#[test]
fn a_knob_frame_costs_the_same_on_a_page_twice_as_long() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_one_box_slide] the probe is compiled out: nothing to count");
        return;
    }
    let short = knob_frame_cost(40);
    let long = knob_frame_cost(80);
    restore_ambient_recording();
    eprintln!("[a_one_box_slide] 40 cards: {short:?}");
    eprintln!("[a_one_box_slide] 80 cards: {long:?}");

    assert!(
        short.layout_passes >= 1 && long.layout_passes >= 1,
        "harness: each frame ran a layout pass, {short:?} / {long:?}"
    );
    assert!(
        long.text_flows <= short.text_flows && long.text_flows <= 4,
        "moving one knob re-flowed {} text runs on 80 cards and {} on 40: the frame re-lays out \
         the page, not the knob's ancestors",
        long.text_flows,
        short.text_flows
    );
    assert!(
        long.flex_items_laid_out <= short.flex_items_laid_out,
        "moving one knob laid out {} flex items again on 80 cards and {} on 40: the clean cards \
         beside the knob's ancestors were laid out again instead of reused",
        long.flex_items_laid_out,
        short.flex_items_laid_out
    );
    assert!(
        long.formatting_contexts <= short.formatting_contexts,
        "moving one knob laid out {} formatting contexts on 80 cards and {} on 40",
        long.formatting_contexts,
        short.formatting_contexts
    );
}

/// The cards beside the moving knob paint exactly what they painted before -
/// the text beside a block included.
///
/// Reusing a clean card means not laying it out again; its anonymous block
/// (the "Note" text beside the "details" block) is built anew by every
/// reconcile, so the reuse is only sound if that block carries what it last
/// laid out. Without it the notes vanish from the page.
#[test]
fn the_cards_beside_a_moving_knob_paint_what_they_painted() {
    let mut lw = window(widgets_page(12));
    let knob = with_class(&lw, "knob");
    let before = painted_glyphs(&lw);
    assert!(!before.is_empty(), "harness: the page paints its text");

    for px in [4isize, 8, 12, 16] {
        restyle(
            &mut lw,
            knob,
            CssProperty::const_margin_left(LayoutMarginLeft::const_px(px)),
        );
        let after = painted_glyphs(&lw);
        assert_eq!(
            after.len(),
            before.len(),
            "knob at {px} px: the page paints {} glyphs, painted {} before the knob moved",
            after.len(),
            before.len()
        );
        assert!(
            after == before,
            "knob at {px} px: a glyph moved although only the knob did"
        );
    }
}

/// Text beside a block keeps painting when the block's sibling restyles.
///
/// A plain block page: the restyled box and, beside it, a box holding text
/// and a block. The restyle is re-solved where it happened (block siblings
/// are only re-stacked), so the second box is not laid out again - and its
/// text, in the anonymous block the reconcile rebuilt, was never laid out in
/// the new tree: the prebuilt azul-doc e2e runner painted 3 text runs before
/// and 2 after (`/tmp` probe in scripts/LAYOUTPERF8_2026_10_03.md).
#[test]
fn text_beside_a_block_keeps_painting_when_a_sibling_restyles() {
    let mut lw = window(
        Dom::create_body()
            .with_child(
                Dom::create_div()
                    .with_class("mover".into())
                    .with_css("width: 50px; height: 20px;"),
            )
            .with_child(
                Dom::create_div()
                    .with_css("width: 300px;")
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "Some text",
                    ))
                    .with_child(Dom::create_div_with_text("block")),
            ),
    );
    let mover = with_class(&lw, "mover");
    let before = painted_glyphs(&lw);
    assert!(
        before.len() >= 12,
        "harness: \"Some text\" and \"block\" paint, {} glyphs",
        before.len()
    );

    restyle(&mut lw, mover, CssProperty::width(LayoutWidth::px(60.0)));

    assert!(
        (rect(&lw, mover).size.width - 60.0).abs() < 0.5,
        "harness: the restyle applied, the box is {:?}",
        rect(&lw, mover)
    );
    assert_eq!(
        painted_glyphs(&lw),
        before,
        "the text beside the block must paint where it painted before its sibling restyled"
    );
}

/// A page laid out again - after a knob frame, after a rebuild that changes
/// one card's text, after another knob frame - is laid out the way a fresh
/// window lays out the same page: every node's box, every glyph.
///
/// The tests above prove the clean cards are reused; this proves that reusing
/// them is right, against a window that reuses nothing.
#[test]
fn a_page_laid_out_again_matches_a_fresh_window() {
    let mut lw = window(widgets_page(12));

    let knob = with_class(&lw, "knob");
    restyle(
        &mut lw,
        knob,
        CssProperty::const_margin_left(LayoutMarginLeft::const_px(8)),
    );
    assert_lays_out_like_a_fresh_window(&lw, widgets_page_with(12, 8, None), "knob at 8 px");

    rebuild(&mut lw, widgets_page_with(12, 8, Some(3)));
    assert_lays_out_like_a_fresh_window(
        &lw,
        widgets_page_with(12, 8, Some(3)),
        "card 3 relabelled",
    );

    let knob = with_class(&lw, "knob");
    restyle(
        &mut lw,
        knob,
        CssProperty::const_margin_left(LayoutMarginLeft::const_px(16)),
    );
    assert_lays_out_like_a_fresh_window(
        &lw,
        widgets_page_with(12, 16, Some(3)),
        "knob at 16 px after the rebuild",
    );
}
