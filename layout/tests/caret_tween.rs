//! Caret / selection tween: the framework animates caret and selection
//! geometry between frames via user-replaceable C-ABI interpolators in
//! `AppConfig.system_animations` (defaults: ease-out cubic).
//!
//! - The tween is a DISPLAY-LIST POST-PASS (`LayoutWindow::apply_text_tweens`):
//!   the solver's cached list keeps the true geometry, the stored list gets
//!   the interpolated rects patched in.
//! - Retargeting compares against the tween's TARGET, not the rendered rect —
//!   comparing against the rendered rect would restart the clock every tick
//!   and the tween would Zeno-crawl forever (pinned below).
//! - While a tween is in flight the caret is forced SOLID (blink suppressed).

use azul_core::dom::{Dom, DomId, DomNodeId, NodeId, TabIndex};
use azul_core::geom::{LogicalRect, LogicalSize};
use azul_core::resources::{RendererResources, SystemAnimations};
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::solver3::display_list::DisplayListItem;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 16px; width: 600px; }
    .editor { display: block; }
"#;

/// Node layout: body=0, div.editor=1, text=2.
const TEXT: usize = 2;

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn text_dom_node_id() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(TEXT))),
    }
}

fn build_editor_window(animations: SystemAnimations) -> LayoutWindow {
    let mut editor = Dom::create_div();
    editor =
        editor.with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("editor".into())].into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    let mut dom = Dom::create_body().with_child(editor.with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper("hello world tween target"),
    ));

    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(animations);
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();

    lw.text_edit_manager
        .initialize_editing(cursor(0), DomId::ROOT_ID, NodeId::new(TEXT), 0);
    lw.text_edit_manager.blink.set_visibility(true);
    // A real focused field has BOTH an editing session AND focus. The caret is
    // only painted when the edited node lives inside the focused subtree
    // (`LayoutWindow::caret_editable_is_focused`, so a blurred field drops its
    // caret) — set focus here so the tween tests exercise the focused state they
    // model instead of reading as "blurred" and never painting a caret.
    lw.focus_manager.set_focused_node(Some(text_dom_node_id()));
    lw
}

fn move_caret(lw: &mut LayoutWindow, byte: u32) {
    lw.text_edit_manager.multi_cursor =
        Some(azul_core::selection::MultiCursorState::new_with_cursor(
            cursor(byte),
            text_dom_node_id(),
            0,
        ));
}

fn rebuild(lw: &mut LayoutWindow) {
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
}

/// The LAST CursorRect item = the primary caret (same rule as the post-pass).
fn caret_item(lw: &LayoutWindow) -> Option<(LogicalRect, u8)> {
    let result = lw.get_layout_result(&DomId::ROOT_ID)?;
    result
        .display_list
        .items
        .iter()
        .rev()
        .find_map(|item| match item {
            DisplayListItem::CursorRect { bounds, color } => Some((bounds.0, color.a)),
            _ => None,
        })
}

fn caret_rect(lw: &LayoutWindow) -> LogicalRect {
    caret_item(lw)
        .expect("display list must carry a CursorRect item")
        .0
}

/// A window with tweens configured; caret duration and selection duration in ms.
fn tween_window(caret_ms: u32, sel_ms: u32) -> LayoutWindow {
    build_editor_window(SystemAnimations {
        caret_tween_duration_ms: caret_ms,
        selection_tween_duration_ms: sel_ms,
        ..SystemAnimations::default()
    })
}

/// The tween-free twin: same DOM, same ops, durations 0 — its caret rect is
/// the ground-truth target geometry.
fn true_caret_rect_at(byte: u32) -> LogicalRect {
    let mut truth = build_editor_window(SystemAnimations::disabled());
    rebuild(&mut truth);
    move_caret(&mut truth, byte);
    rebuild(&mut truth);
    caret_rect(&truth)
}

#[test]
fn caret_move_arms_a_tween_and_renders_near_the_previous_rect() {
    let mut lw = tween_window(10_000, 0);
    rebuild(&mut lw); // first appearance: tracking starts, no tween
    let a = caret_rect(&lw);
    assert!(
        lw.text_edit_manager.tween.caret.is_none(),
        "first caret appearance must not tween"
    );

    move_caret(&mut lw, 14);
    rebuild(&mut lw);
    let rendered = caret_rect(&lw);
    let b = true_caret_rect_at(14);

    assert!(
        b.origin.x > a.origin.x + 10.0,
        "caret must move right: {a:?} -> {b:?}"
    );
    assert!(
        lw.text_edit_manager.tween.caret.is_some(),
        "a caret move with a nonzero duration arms the tween"
    );
    assert!(
        lw.text_edit_manager.tween.is_active(),
        "tween must report active (drives the 16ms timer + blink suppression)"
    );
    // With a 10s duration, the rebuild happens at t ~ 0: the rendered rect
    // stays close to the OLD position, far from the target.
    let progress = (rendered.origin.x - a.origin.x) / (b.origin.x - a.origin.x);
    assert!(
        (-0.05..0.4).contains(&progress),
        "immediately after the move the caret renders near its previous rect \
         (got progress {progress}, rendered {rendered:?}, from {a:?}, to {b:?})"
    );
}

#[test]
fn tween_completes_exactly_on_the_target_and_retires() {
    let mut lw = tween_window(1, 0);
    rebuild(&mut lw);
    move_caret(&mut lw, 14);
    rebuild(&mut lw); // arms (t≈0 within the same millisecond)
    std::thread::sleep(std::time::Duration::from_millis(10));
    rebuild(&mut lw); // t >= 1: snap + retire
    let rendered = caret_rect(&lw);
    let b = true_caret_rect_at(14);
    assert_eq!(
        rendered, b,
        "a finished tween renders EXACTLY the layout's caret rect"
    );
    assert!(
        lw.text_edit_manager.tween.caret.is_none(),
        "finished tween retires"
    );
    assert!(!lw.text_edit_manager.tween.is_active());
}

#[test]
fn stable_target_does_not_restart_the_tween_clock() {
    // Pin for the Zeno bug: re-arming on "rendered != current" would reset
    // `start` on every tick (the rendered rect always lags mid-flight) and
    // the tween would never finish.
    let mut lw = tween_window(10_000, 0);
    rebuild(&mut lw);
    move_caret(&mut lw, 14);
    rebuild(&mut lw);
    let start0 = lw
        .text_edit_manager
        .tween
        .caret
        .as_ref()
        .expect("armed")
        .start
        .clone();
    rebuild(&mut lw);
    rebuild(&mut lw);
    let start2 = lw
        .text_edit_manager
        .tween
        .caret
        .as_ref()
        .expect("still in flight (10s duration)")
        .start
        .clone();
    assert_eq!(
        start2.duration_since(&start0).as_millis_u64(),
        0,
        "ticks toward a STABLE target must not restart the clock"
    );
}

#[test]
fn caret_stays_solid_while_the_tween_is_in_flight() {
    let mut lw = tween_window(10_000, 0);
    rebuild(&mut lw);
    move_caret(&mut lw, 14);
    rebuild(&mut lw); // arms the tween
    assert!(lw.text_edit_manager.tween.is_active());

    // Blink phase OFF would normally render the caret with alpha 0 — the
    // in-flight tween must force it solid.
    lw.text_edit_manager.blink.set_visibility(false);
    rebuild(&mut lw);
    let (_, alpha) = caret_item(&lw).expect("caret item");
    assert!(
        alpha > 0,
        "blinking is suppressed while the caret tween is running"
    );
}

#[test]
fn disabled_durations_jump_without_tween_state() {
    let mut lw = build_editor_window(SystemAnimations::disabled());
    rebuild(&mut lw);
    move_caret(&mut lw, 14);
    rebuild(&mut lw);
    let rendered = caret_rect(&lw);
    let b = true_caret_rect_at(14);
    assert_eq!(rendered, b, "duration 0 = classic jump");
    assert!(!lw.text_edit_manager.tween.is_active());
}

// ---------------------------------------------------------------------------
// Default interpolator math (the C-ABI functions users can replace)
// ---------------------------------------------------------------------------

#[test]
fn default_caret_tween_math_hits_both_endpoints_and_moves_between() {
    use azul_core::callbacks::{default_caret_tween, CaretTweenInfo};
    use azul_core::geom::LogicalPosition;
    use azul_core::refany::RefAny;

    let past = LogicalRect {
        origin: LogicalPosition { x: 10.0, y: 20.0 },
        size: LogicalSize::new(2.0, 18.0),
    };
    let current = LogicalRect {
        origin: LogicalPosition { x: 110.0, y: 60.0 },
        size: LogicalSize::new(2.0, 22.0),
    };
    let at = |t: f32| default_caret_tween(RefAny::new(()), CaretTweenInfo { past, current, t });
    assert_eq!(at(0.0), past, "t = 0 renders the past rect");
    assert_eq!(at(1.0), current, "t = 1 renders the current rect");

    // Trapezoidal velocity profile (`/‾‾‾\`): quadratic ramps over the first
    // and last quarter, LINEAR cruise between — so the symmetric midpoint is
    // exactly halfway...
    let mid = at(0.5);
    let dist = current.origin.x - past.origin.x;
    assert!(
        ((mid.origin.x - past.origin.x) / dist - 0.5).abs() < 1e-3,
        "symmetric curve: half the distance at half time, got {mid:?}"
    );
    // ...the ramp-in is slower than linear (soft start)...
    let quarter = at(0.25);
    let q = (quarter.origin.x - past.origin.x) / dist;
    assert!(
        q < 0.25 && q > 0.10,
        "quadratic ramp-in covers ~1/6 of the distance by t = 0.25, got {q}"
    );
    // ...and the cruise phase is LINEAR: equal time slices cover equal
    // distance in the middle.
    let s40 = (at(0.40).origin.x - past.origin.x) / dist;
    let s50 = (at(0.50).origin.x - past.origin.x) / dist;
    let s60 = (at(0.60).origin.x - past.origin.x) / dist;
    assert!(
        ((s50 - s40) - (s60 - s50)).abs() < 1e-3,
        "constant-velocity plateau: {s40} {s50} {s60}"
    );
    // Out-of-range t clamps.
    assert_eq!(at(2.0), current);
    assert_eq!(at(-1.0), past);
}

#[test]
fn default_selection_tween_pairs_rects_by_line_not_by_index() {
    use azul_core::callbacks::{default_selection_tween, SelectionTweenInfo};
    use azul_core::geom::LogicalPosition;
    use azul_core::refany::RefAny;

    // One rect per LINE; `y` identifies the line.
    let line = |y: f32, x: f32| LogicalRect {
        origin: LogicalPosition { x, y },
        size: LogicalSize::new(50.0, 18.0),
    };
    let at = |past: Vec<LogicalRect>, current: Vec<LogicalRect>, t: f32| -> Vec<LogicalRect> {
        default_selection_tween(
            RefAny::new(()),
            SelectionTweenInfo {
                past: past.into(),
                current: current.into(),
                t,
            },
        )
        .into_library_owned_vec()
    };

    // Extending UPWARD prepends a line. The pre-existing line (y = 20) must
    // start the tween at its OWN old geometry, and the new first line (y = 0)
    // must pop at its final geometry — index pairing lerped y=0 from the old
    // y=20 rect and slid the whole band.
    let out = at(
        vec![line(20.0, 0.0)],
        vec![line(0.0, 0.0), line(20.0, 100.0)],
        0.0,
    );
    assert_eq!(out.len(), 2, "one rect per CURRENT rect");
    assert_eq!(
        out[0],
        line(0.0, 0.0),
        "the new first line pops, it does not slide"
    );
    assert_eq!(
        out[1],
        line(20.0, 0.0),
        "line y=20 tweens from its own past rect"
    );

    // …and lands exactly on the current geometry.
    let out = at(
        vec![line(20.0, 0.0)],
        vec![line(0.0, 0.0), line(20.0, 100.0)],
        1.0,
    );
    assert_eq!(out, vec![line(0.0, 0.0), line(20.0, 100.0)]);

    // Growing DOWNWARD keeps working (this is what the old test covered).
    let out = at(
        vec![line(0.0, 0.0)],
        vec![line(0.0, 100.0), line(20.0, 300.0)],
        0.0,
    );
    assert_eq!(
        out[0],
        line(0.0, 0.0),
        "paired rect starts at its past geometry"
    );
    assert_eq!(
        out[1],
        line(20.0, 300.0),
        "unpaired (new) rect pops at final geometry"
    );

    // Two rects on ONE line (bidi splits a line) pair one-to-one, not both
    // against the same past rect.
    let out = at(
        vec![line(0.0, 0.0), line(0.0, 60.0)],
        vec![line(0.0, 10.0), line(0.0, 70.0)],
        0.0,
    );
    assert_eq!(out, vec![line(0.0, 0.0), line(0.0, 60.0)]);
}

// ---------------------------------------------------------------------------
// Selection tween through the display-list post-pass (cross-block extend)
// ---------------------------------------------------------------------------

/// body=0, then (div=1, text=2), (div=3, text=4), (div=5, text=6).
fn build_three_paragraphs(animations: SystemAnimations) -> LayoutWindow {
    const P_CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .p { display: block; }
    "#;
    let class = |name: &str| -> azul_core::dom::IdOrClassVec {
        vec![azul_core::dom::IdOrClass::Class(name.into())].into()
    };
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "first paragraph",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "second paragraph",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "third paragraph",
                )),
        );
    let (css, _) = azul_css::parser2::new_from_str(P_CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(animations);
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw
}

fn selection_bands(lw: &LayoutWindow) -> Vec<LogicalRect> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::SelectionRect { bounds, .. } => Some(bounds.0),
            _ => None,
        })
        .collect()
}

fn select_p1_to(lw: &mut LayoutWindow, end_node: usize, end_byte: u32) {
    let ok = lw.set_cross_block_selection(
        DomId::ROOT_ID,
        NodeId::new(1),
        cursor(6),
        NodeId::new(end_node),
        cursor(end_byte),
    );
    assert!(ok, "cross-block selection must be accepted");
}

#[test]
fn extending_a_selection_arms_the_band_tween() {
    let ops = |lw: &mut LayoutWindow| {
        select_p1_to(lw, 3, 6); // P1 tail + P2 head
        rebuild(lw); // first appearance: tracked, no tween
        select_p1_to(lw, 5, 5); // extend into P3
        rebuild(lw); // change: arms the tween
    };

    let mut lw = build_three_paragraphs(SystemAnimations {
        caret_tween_duration_ms: 0,
        selection_tween_duration_ms: 10_000,
        ..SystemAnimations::default()
    });
    ops(&mut lw);

    let mut truth = build_three_paragraphs(SystemAnimations::disabled());
    ops(&mut truth);
    let true_bands = selection_bands(&truth);
    let rendered = selection_bands(&lw);

    assert!(
        lw.text_edit_manager.tween.selection.is_some(),
        "extending the selection with a nonzero duration arms the band tween"
    );
    assert_eq!(
        rendered.len(),
        true_bands.len(),
        "the tween patches geometry, never the item count"
    );
    assert!(
        rendered.iter().zip(true_bands.iter()).any(|(r, t)| r != t),
        "mid-flight (t ~ 0 of a 10s tween) at least one band must still \
         differ from the final geometry: {rendered:?} vs {true_bands:?}"
    );

    // ...and completion snaps to the exact final geometry.
    let mut fast = build_three_paragraphs(SystemAnimations {
        caret_tween_duration_ms: 0,
        selection_tween_duration_ms: 1,
        ..SystemAnimations::default()
    });
    ops(&mut fast);
    std::thread::sleep(std::time::Duration::from_millis(10));
    rebuild(&mut fast);
    assert_eq!(selection_bands(&fast), true_bands);
    assert!(!fast.text_edit_manager.tween.is_active());
}

// ---------------------------------------------------------------------------
// Damage-rect compatibility: a tween tick must diff down to caret-sized
// damage, never a full-window repaint (same invariant the blink machinery
// pins via stable item counts).
// ---------------------------------------------------------------------------

#[test]
fn tween_ticks_produce_caret_sized_damage_not_full_repaints() {
    use azul_layout::cpurender::{compute_display_list_damage, ScrollOffsetMap};

    // Real default duration (60ms): the 15ms sleep lands the second tick in
    // the trapezoid's cruise phase, where the caret visibly advances. (With
    // a very long duration the quadratic ramp-in moves sub-pixel amounts in
    // the first milliseconds and the two ticks compare EQUAL - also fine,
    // but then there is nothing to assert about damage shape.)
    let mut lw = tween_window(60, 0);
    rebuild(&mut lw);
    move_caret(&mut lw, 14);
    rebuild(&mut lw); // arms the tween, renders near the old rect
    let dl_tick1 = lw
        .get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .clone();
    let caret_tick1 = caret_rect(&lw);

    std::thread::sleep(std::time::Duration::from_millis(15));
    rebuild(&mut lw); // next tick: caret advanced a little
    let dl_tick2 = lw
        .get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .clone();
    let caret_tick2 = caret_rect(&lw);

    let offsets = ScrollOffsetMap::new();
    let damage = compute_display_list_damage(&dl_tick1, &dl_tick2, &offsets, &offsets).expect(
        "a tween tick only moves item BOUNDS - the differ must never bail to a full repaint",
    );

    // Every damage rect stays within the caret's neighborhood (union of the
    // two tick positions, generously padded), and the total damaged area is
    // a tiny fraction of the 800x600 window.
    let union_min_x = caret_tick1.origin.x.min(caret_tick2.origin.x) - 8.0;
    let union_max_x = (caret_tick1.origin.x + caret_tick1.size.width)
        .max(caret_tick2.origin.x + caret_tick2.size.width)
        + 8.0;
    let total_area: f32 = damage.iter().map(|r| r.size.width * r.size.height).sum();
    assert!(
        !damage.is_empty(),
        "the caret moved between ticks - damage must not be empty"
    );
    for r in &damage {
        assert!(
            r.origin.x >= union_min_x && r.origin.x + r.size.width <= union_max_x,
            "damage {r:?} escapes the caret neighborhood [{union_min_x}, {union_max_x}]"
        );
    }
    assert!(
        total_area < 800.0 * 600.0 * 0.01,
        "tween damage must be caret-sized, got {total_area}px² across {damage:?}"
    );
}
