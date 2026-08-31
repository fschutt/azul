//! Ledger #29: focus-ring transitions — an OPT-IN focus outline
//! (`SystemAnimations.focus_ring_duration_ms`, default 0 = feature off,
//! zero visual change for existing apps) that GLIDES between focused
//! elements via the caret interpolator, painted as a plain appended
//! `Border` item in the tween post-pass. Suppressed while a text-editing
//! session owns focus (there the caret is the indicator).

use azul_core::dom::{Dom, DomId, DomNodeId, NodeId, TabIndex};
use azul_core::geom::{LogicalRect, LogicalSize};
use azul_core::resources::{RendererResources, SystemAnimations};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::solver3::display_list::DisplayListItem;
use azul_layout::window::LayoutWindow;
use azul_layout::{callbacks::ExternalSystemCallbacks, window_state::FullWindowState};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; }
    .btn { display: block; width: 120px; height: 30px; }
    .gap { display: block; height: 170px; }
"#;

/// body=0 > btn1=1(text 2) > gap=3 > btn2=4(text 5)
fn build(animations: SystemAnimations) -> LayoutWindow {
    let btn = |label: &str| {
        let mut b = Dom::create_div()
            .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("btn".into())].into());
        b.set_tab_index(TabIndex::Auto);
        b.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            label,
        ))
    };
    let mut dom = Dom::create_body()
        .with_child(btn("one"))
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("gap".into())].into()),
        )
        .with_child(btn("two"));
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(animations);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

fn focus(lw: &mut LayoutWindow, node: usize) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }));
}

fn rebuild(lw: &mut LayoutWindow) {
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
}

/// The ring is ALWAYS the last item in the list (the post-pass appends it
/// after everything else). The DOM's own UA/cascade styling produces
/// ordinary Border items too, so "is a Border" alone identifies nothing.
fn ring_rect(lw: &LayoutWindow) -> Option<LogicalRect> {
    match lw
        .get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .items
        .last()?
    {
        DisplayListItem::Border { bounds, .. } => Some(bounds.0),
        _ => None,
    }
}

fn border_count(lw: &LayoutWindow) -> usize {
    lw.get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .items
        .iter()
        .filter(|i| matches!(i, DisplayListItem::Border { .. }))
        .count()
}

fn ring_on(ms: u32) -> SystemAnimations {
    SystemAnimations {
        focus_ring_duration_ms: ms,
        ..SystemAnimations::default()
    }
}

#[test]
fn the_default_config_paints_a_ring_and_disabled_paints_none() {
    // 2026-08-31 RULING (device report): keyboard focus must be VISIBLE with
    // no author CSS. Focus and Enter/Space activation both already worked,
    // but nothing on screen said WHICH control had focus, which is
    // indistinguishable from "Tab does nothing". The ring is therefore ON by
    // default now - this test previously pinned the opposite (opt-in, 0 = no
    // ring), which is the behaviour that made the toolkit look broken.
    //
    // `SystemAnimations::disabled()` still paints none, so e2e screenshots
    // stay deterministic.
    let mut default_cfg = build(SystemAnimations::default());
    focus(&mut default_cfg, 1);
    rebuild(&mut default_cfg);

    let mut disabled = build(SystemAnimations::disabled());
    focus(&mut disabled, 1);
    rebuild(&mut disabled);

    assert_eq!(
        border_count(&disabled) + 1,
        border_count(&default_cfg),
        "the DEFAULT config must paint a focus ring, and disabled() must not",
    );
}

#[test]
fn opted_in_ring_wraps_the_focused_node_and_glides_on_focus_move() {
    let mut lw = build(ring_on(10_000));
    focus(&mut lw, 1);
    rebuild(&mut lw); // first appearance: ring at btn1, no tween
    let at_one = ring_rect(&lw).expect("ring appended as the last item");
    let btn1 = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        })
        .unwrap();
    assert!(
        (at_one.origin.y - (btn1.origin.y - 2.0)).abs() < 0.6
            && (at_one.size.height - (btn1.size.height + 4.0)).abs() < 0.6,
        "ring hugs the focused node (2px inflation): ring {at_one:?} vs node {btn1:?}"
    );
    assert!(!lw.text_edit_manager.tween.is_active(), "no glide yet");

    // Move focus far down the page: the ring GLIDES (10s duration → the
    // immediate rebuild renders near btn1, far from btn2).
    focus(&mut lw, 4);
    rebuild(&mut lw);
    let mid = ring_rect(&lw).expect("ring present mid-glide");
    let btn2_y = at_one.origin.y + 200.0; // btn2 sits 200px below btn1
    assert!(
        lw.text_edit_manager.tween.focus_ring.is_some(),
        "focus move arms the ring glide"
    );
    assert!(
        mid.origin.y < at_one.origin.y + (btn2_y - at_one.origin.y) * 0.4,
        "t ~ 0: the ring is still near its previous position, got {mid:?}"
    );

    // Completion snaps exactly onto the new target.
    let mut fast = build(ring_on(1));
    focus(&mut fast, 1);
    rebuild(&mut fast);
    focus(&mut fast, 4);
    rebuild(&mut fast);
    std::thread::sleep(std::time::Duration::from_millis(10));
    rebuild(&mut fast);
    let done = ring_rect(&fast).expect("ring present after completion");
    let btn2 = fast
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(4))),
        })
        .unwrap();
    assert!(
        (done.origin.y - (btn2.origin.y - 2.0)).abs() < f32::EPSILON,
        "finished glide lands exactly: {done:?} vs {btn2:?}"
    );
    assert!(fast.text_edit_manager.tween.focus_ring.is_none(), "retired");
}

#[test]
fn ring_is_suppressed_while_a_text_editing_session_owns_focus() {
    let mut lw = build(ring_on(10_000));
    focus(&mut lw, 1);
    // An active editing session (caret) = the caret is the focus indicator.
    lw.text_edit_manager.initialize_editing(
        azul_core::selection::TextCursor {
            cluster_id: azul_core::selection::GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: azul_core::selection::CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        NodeId::new(2),
        0,
    );
    rebuild(&mut lw);
    assert!(
        ring_rect(&lw).is_none(),
        "no ring while a text-editing session is active (the caret is the          indicator) — the last item must not be the appended ring"
    );
}
