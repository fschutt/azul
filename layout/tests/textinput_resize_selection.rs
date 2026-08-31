//! A TextInput's text must stay selectable — and stay PAINTED — after a
//! window resize makes it overflow its box, and after growing back.
//!
//! Device bug this was written against (AzWidgets, macOS): the field works
//! until the window is resized so the value clips; after that (and after
//! growing the window back so the text is fully visible again) a click shows
//! no caret and a drag paints no selection, while the select-scroll-drag
//! auto-scroll still fires. These tests drive the exact shell sequence
//! headlessly and VALIDATE EVERY LAYER THE LAYOUT CRATE OWNS, so the seam
//! the defect lives in can be bisected from above:
//!
//! * the resize fast path (`resize_only_hint` latch + relayout of the SAME
//!   StyledDom — `incremental_relayout_for_resize`,
//!   `dll/src/desktop/shell2/common/layout.rs`), multi-step, live-resize
//!   shaped, with the editing session alive throughout;
//! * scroll re-registration + the CPU hit-tester rebuild (the shells'
//!   finalize tail), hover push, click, drag;
//! * typed text through the REAL edit pipeline (the content overlay — layout
//!   re-runs on the DOM's original text and re-applies the edit every pass)
//!   and the caret reveal, on the real `TextInput` widget DOM;
//! * assertions at every level: session state, `CursorRect`/`SelectionRect`
//!   items in the display list (each resize pass included), rasterized
//!   pixels (so a stale clip cannot hide an item that exists), and
//!   `compute_display_list_damage` covering the fresh caret (so the present
//!   path repaints the right region and the DL Arc is never re-served).
//!
//! All of this is GREEN, which is itself a finding: the state → hit-test →
//! display-list → raster → damage pipeline in `azul-layout` survives the
//! resize, including the FULL-REGENERATION resize (an orientation-crossing
//! resize regenerates the DOM unconditionally — `window.rs`,
//! `resize_needs_full_regeneration`).
//!
//! The companion corpus scenario `e2e/bug-textinput-resize-select-visual.json`
//! drives the same sequence through the REAL event pipeline + frame loop +
//! CpuBackend damage/present, and it is what caught the one layer these
//! direct tests bypass: the e2e runner dropped every pre-callback
//! `SystemChange` except `ApplySelectionOp`, so `TextSelectionClick` /
//! `TextSelectionDrag` never ran and click-to-caret was DEAD in all headless
//! e2e (the caret only ever landed at the end of the text via the focus
//! path). Fixed in `layout/src/e2e/runner.rs` by porting the DLL's arms.

use std::collections::BTreeMap;

use azul_core::{
    dom::{Dom, DomId, NodeId, TabIndex},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::Selection,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, headless::CpuHitTester, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The value string. Long enough that it overflows the narrow window but fits
/// the wide one.
const VALUE: &str = "a quick brown fox jumped over the lazy dog";

/// The TextInput shape from `widgets/text_input.rs`, minus the chrome:
/// `container` is the contenteditable host, `p.value` is the horizontal
/// scroll box (`overflow-x: auto; overflow-y: hidden; white-space: pre`), the
/// text is its child. The container tracks the window width so a window
/// resize is what makes the text overflow.
const CSS: &str = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; display: block; } \
                   .container { display: block; width: 100%; padding: 2px; } \
                   .value { display: block; overflow-x: auto; overflow-y: hidden; \
                            white-space: pre; scrollbar-width: none; }";

fn fixture() -> Dom {
    use azul_core::dom::IdOrClass;
    let container_cls: azul_core::dom::IdOrClassVec =
        vec![IdOrClass::Class("container".into())].into();
    let value_cls: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("value".into())].into();
    Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(container_cls)
            .with_contenteditable(true)
            .with_tab_index(TabIndex::Auto)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(value_cls)
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        VALUE,
                    )),
            ),
    )
}

struct Harness {
    lw: LayoutWindow,
    hit_tester: CpuHitTester,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
}

impl Harness {
    fn new(width: f32, height: f32) -> Self {
        let mut dom = fixture();
        let (css, warnings) = azul_css::parser2::new_from_str(CSS);
        assert!(warnings.is_empty(), "css warnings: {warnings:?}");
        let styled_dom = StyledDom::create(&mut dom, css);
        Self::new_with_styled_dom(styled_dom, width, height)
    }

    /// The REAL TextInput widget (inline `with_css_props` styling), the shape
    /// AzWidgets ships.
    fn new_widget(initial_text: &str, width: f32, height: f32) -> Self {
        let widget = azul_layout::widgets::text_input::TextInput::create()
            .with_text(initial_text.into())
            .dom();
        let dom = Dom::create_body().with_child(widget);
        let styled_dom = StyledDom::create_from_dom(dom);
        Self::new_with_styled_dom(styled_dom, width, height)
    }

    fn new_with_styled_dom(styled_dom: StyledDom, width: f32, height: f32) -> Self {
        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = LogicalSize::new(width, height);
        lw.current_window_state = window_state.clone();
        let renderer_resources = RendererResources::default();
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut dbg,
        )
        .unwrap();
        lw.current_window_state = window_state;

        let mut h = Self {
            lw,
            hit_tester: CpuHitTester::new(),
            renderer_resources,
            system_callbacks,
        };
        h.register_scroll_nodes();
        h.rebuild_hit_tester();
        h
    }

    fn register_scroll_nodes(&mut self) {
        let now = azul_core::task::Instant::from(std::time::Instant::now());
        azul_layout::managers::scroll_registration::register_scroll_nodes(&mut self.lw, &now);
    }

    fn rebuild_hit_tester(&mut self) {
        self.hit_tester
            .rebuild_from_layout_with_gpu(&self.lw.layout_results, Some(&self.lw.gpu_state_manager));
    }

    /// Port of `PlatformWindow::update_hit_test_at` — same as
    /// `layout/src/e2e/runner.rs`.
    fn update_hit_test_at(&mut self, position: LogicalPosition) {
        use azul_layout::managers::hover::InputPointId;
        let focused_node = self.lw.focus_manager.get_focused_node().copied();
        let hit_test = {
            let scroll_manager = &self.lw.scroll_manager;
            let gpu = &self.lw.gpu_state_manager;
            let resolve =
                |d: DomId, n: NodeId| -> Option<LogicalPosition> {
                    scroll_manager.get_current_offset(d, n)
                };
            let resolve_tf = |d: DomId, n: NodeId| {
                gpu.caches
                    .get(&d)
                    .and_then(|c| c.css_current_transform_values.get(&n))
                    .copied()
            };
            let hits = self
                .hit_tester
                .hit_test_scrolled(position, &resolve, &resolve_tf);
            azul_layout::headless::convert_cpu_hit_test_to_full(
                &self.hit_tester,
                &hits,
                focused_node,
                &self.lw.layout_results,
                position,
                &resolve,
                &resolve_tf,
            )
        };
        self.lw
            .hover_manager
            .push_hit_test(InputPointId::Mouse, hit_test);
    }

    /// The RESIZE fast path exactly as the shells drive it:
    /// `incremental_relayout_for_resize` = `resize_only_hint` latch + relayout
    /// of the SAME StyledDom at the new size, then the finalize tail (scroll
    /// re-registration + CPU hit-tester rebuild).
    fn resize(&mut self, width: f32, height: f32) {
        let mut window_state = self.lw.current_window_state.clone();
        window_state.size.dimensions = LogicalSize::new(width, height);

        self.lw.layout_cache.resize_only_hint = true;
        let layout_result = self
            .lw
            .layout_results
            .remove(&DomId::ROOT_ID)
            .expect("root layout result");
        let styled_dom = layout_result.styled_dom;
        let mut dbg = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                styled_dom,
                &window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .unwrap();
        self.lw.current_window_state = window_state;

        self.register_scroll_nodes();
        self.rebuild_hit_tester();
    }

    /// The RESIZE that needs FULL REGENERATION (orientation crossing / size
    /// query flip): port of `Runner::regenerate_layout`'s resize arm —
    /// `clear_caches()` + relayout of the same StyledDom (no reconciliation;
    /// `is_new_tree = false`), then the finalize tail.
    fn full_regen_resize(&mut self, width: f32, height: f32) {
        let mut window_state = self.lw.current_window_state.clone();
        window_state.size.dimensions = LogicalSize::new(width, height);

        let mut styled_dom = self
            .lw
            .layout_results
            .remove(&DomId::ROOT_ID)
            .expect("root layout result")
            .styled_dom;
        self.lw.clear_caches();
        styled_dom.recompute_inheritance_and_compact_cache();

        let mut dbg = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                styled_dom,
                &window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .unwrap();
        self.lw.current_window_state = window_state;

        self.register_scroll_nodes();
        self.rebuild_hit_tester();
    }

    /// Simulate the shell's press: hover hit test at the position, then the
    /// text-selection click.
    fn click(&mut self, position: LogicalPosition, time_ms: u64) {
        self.update_hit_test_at(position);
        self.lw.process_mouse_click_for_selection(position, time_ms);
    }

    /// Type through the REAL edit pipeline: record + apply changeset (the
    /// edit lands in the content overlay, never in the DOM), then the
    /// relayout the shell would run when the extent changed, then the caret
    /// reveal. Requires focus + an editing session (a prior click).
    fn type_text(&mut self, s: &str) {
        let _ = self.lw.record_text_input(s);
        let result = self.lw.apply_text_changeset();
        if result.needs_relayout {
            self.relayout_same_dom();
        }
        let _ = self.lw.scroll_selection_into_view(
            azul_layout::window::SelectionScrollType::Cursor,
            azul_layout::window::ScrollMode::Instant,
        );
        self.register_scroll_nodes();
        self.rebuild_hit_tester();
    }

    /// The NON-resize incremental relayout (restyle/edit path): same funnel,
    /// no `resize_only_hint`.
    fn relayout_same_dom(&mut self) {
        let window_state = self.lw.current_window_state.clone();
        let layout_result = self
            .lw
            .layout_results
            .remove(&DomId::ROOT_ID)
            .expect("root layout result");
        let styled_dom = layout_result.styled_dom;
        let mut dbg = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                styled_dom,
                &window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .unwrap();
    }

    /// CPU-render the current display list — the DL *including its clip
    /// stack*, which item-presence checks cannot see: a caret item swallowed
    /// by a stale clip renders nothing.
    #[cfg(feature = "cpurender")]
    fn render(
        &self,
        glyphs: &mut azul_layout::glyph_cache::GlyphCache,
    ) -> azul_layout::cpurender::AzulPixmap {
        use azul_layout::cpurender::{self, RenderOptions};
        let dl = &self
            .lw
            .layout_results
            .get(&DomId::ROOT_ID)
            .unwrap()
            .display_list;
        let opts = RenderOptions {
            width: self.lw.current_window_state.size.dimensions.width,
            height: self.lw.current_window_state.size.dimensions.height,
            dpi_factor: 1.0,
        };
        cpurender::render_with_font_manager(
            dl,
            &self.renderer_resources,
            &self.lw.font_manager,
            opts,
            glyphs,
        )
        .unwrap()
    }

    /// Debug: dump every node's border box (static space).
    #[allow(dead_code)]
    fn debug_rects(&self, label: &str) {
        let lr = self.lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        eprintln!("--- rects: {label} ---");
        for (idx, node) in lr.layout_tree.nodes.iter().enumerate() {
            let pos = lr.calculated_positions.get(idx);
            eprintln!(
                "  layout {idx} dom {:?} pos {:?} size {:?}",
                node.dom_node_id, pos, node.used_size
            );
        }
    }

    fn focus(&mut self, node: NodeId) {
        self.lw
            .focus_manager
            .set_focused_node(Some(azul_core::dom::DomNodeId {
                dom: DomId::ROOT_ID,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node)),
            }));
    }

    /// Where the caret sits after a click, if the click placed one.
    fn caret(&self) -> Option<(NodeId, u32)> {
        let mc = self.lw.text_edit_manager.multi_cursor.as_ref()?;
        let node = mc.node_id.node.into_crate_internal()?;
        let cursor = match &mc.get_primary()?.selection {
            Selection::Cursor(c) => *c,
            Selection::Range(r) => r.end,
        };
        Some((node, cursor.cluster_id.start_byte_in_run))
    }

    /// The value text's laid-out advance width (content size of the IFC).
    ///
    /// Reads the MATERIALIZED layout: under the default `AZ_DENSE_TEXT=1` the
    /// stored `cached.layout` is the shared retirement sentinel (zero items,
    /// zero bounds) — see `text_edit_seam_regressions.rs`.
    fn text_extent(&self) -> f32 {
        let lr = self.lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        let tree = &lr.layout_tree;
        let mut max = 0.0_f32;
        for idx in 0..tree.nodes.len() {
            if let Some(layout) = tree.materialized_inline_layout_for_node(idx) {
                max = max.max(layout.bounds().width);
            }
        }
        max
    }

    /// Border-box width of the `.value` scroll box (NodeId 2 in the fixture:
    /// body=0? see assert in the test).
    fn node_width(&self, node: NodeId) -> Option<f32> {
        let lr = self.lw.layout_results.get(&DomId::ROOT_ID)?;
        let idx = lr.layout_tree.dom_to_layout.get(&node)?.first()?;
        lr.layout_tree
            .get(*idx)
            .and_then(|n| n.used_size)
            .map(|s| s.width)
    }

    fn hovered_nodes(&self) -> BTreeMap<NodeId, ()> {
        use azul_layout::managers::hover::InputPointId;
        self.lw
            .hover_manager
            .get_current(&InputPointId::Mouse)
            .map(|ht| {
                ht.hovered_nodes
                    .values()
                    .flat_map(|h| h.regular_hit_test_nodes.keys().map(|n| (*n, ())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The caret ITEMS actually in the display list — the visual layer the
    /// user sees. (`process_mouse_click_for_selection` regenerates the DL
    /// itself, so a successful click must leave one here.)
    fn caret_items(&self) -> Vec<azul_core::geom::LogicalRect> {
        use azul_layout::solver3::display_list::DisplayListItem;
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::CursorRect { bounds, .. } => Some(bounds.0),
                _ => None,
            })
            .collect()
    }

    /// Caret items with their painted alpha — an alpha-0 caret occupies the
    /// display list (stable item count for blink damage) but is INVISIBLE.
    fn caret_alphas(&self) -> Vec<(azul_core::geom::LogicalRect, u8)> {
        use azul_layout::solver3::display_list::DisplayListItem;
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::CursorRect { bounds, color } => Some((bounds.0, color.a)),
                _ => None,
            })
            .collect()
    }

    /// The painted selection bands.
    fn selection_items(&self) -> usize {
        use azul_layout::solver3::display_list::DisplayListItem;
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .filter(|item| matches!(item, DisplayListItem::SelectionRect { .. }))
            .count()
    }
}

/// RED = the device bug. A click that placed a caret before the resize must
/// still place one after the window shrank enough that the text overflows —
/// and the caret must be IN THE DISPLAY LIST, because "the state updated but
/// nothing painted" is exactly the reported symptom.
#[test]
fn click_still_places_a_caret_after_a_resize_makes_the_text_overflow() {
    let mut h = Harness::new(800.0, 600.0);

    // Premise: wide window, text fits, a click places a caret.
    let click1 = LogicalPosition::new(60.0, 12.0);
    h.click(click1, 0);
    let before = h.caret();
    assert!(
        before.is_some(),
        "premise broken: at 800px the click at {click1:?} placed no caret; hovered = {:?}",
        h.hovered_nodes()
    );
    assert!(
        !h.caret_items().is_empty(),
        "premise broken: at 800px the caret is not painted at all"
    );

    let wide_extent = h.text_extent();
    assert!(
        wide_extent > 200.0,
        "premise broken: the value text lays out at {wide_extent}px, too short to overflow 160px"
    );

    // The shells' resize fast path, at a width where the value overflows.
    h.resize(160.0, 600.0);
    let narrow_extent = h.text_extent();
    let value_width = h.node_width(NodeId::new(2));
    assert!(
        value_width.is_some_and(|w| w < narrow_extent),
        "premise broken: after the resize the value box ({value_width:?}) does not overflow \
         (text extent {narrow_extent})"
    );

    // Click on visible text well inside the narrowed box.
    let click2 = LogicalPosition::new(40.0, 12.0);
    h.click(click2, 5000);
    let after = h.caret();
    assert!(
        after.is_some(),
        "THE BUG: after the resize-overflow, the click at {click2:?} placed no caret \
         (hovered = {:?}, caret before = {before:?})",
        h.hovered_nodes()
    );
    // And the caret must be at the clicked spot, not wherever it was before.
    assert_ne!(
        after, before,
        "the second click resolved the same caret as the first — stale geometry"
    );
    assert!(
        !h.caret_items().is_empty(),
        "THE BUG (visual): the post-resize click updated the caret state but the display \
         list carries no CursorRect — the caret is invisible"
    );

    // A drag from the caret must paint a selection.
    h.lw
        .process_mouse_drag_for_selection(click2, LogicalPosition::new(120.0, 12.0));
    assert!(
        h.selection_items() > 0,
        "THE BUG (visual): the drag extended no painted selection after the resize"
    );
}

/// The user-described repro: shrink so the text clips, then grow the window
/// back so the text is fully visible again — selection must still work, and
/// must still PAINT.
#[test]
fn click_still_places_a_caret_after_growing_back() {
    let mut h = Harness::new(800.0, 600.0);
    h.click(LogicalPosition::new(60.0, 12.0), 0);
    assert!(h.caret().is_some(), "premise broken at 800px");
    assert!(
        !h.caret_items().is_empty(),
        "premise broken: caret not painted at 800px"
    );

    // Live-resize shape: several shrinking steps, then several growing ones.
    for w in [620.0, 430.0, 240.0, 160.0, 320.0, 560.0, 800.0] {
        h.resize(w, 600.0);
    }

    let click = LogicalPosition::new(120.0, 12.0);
    h.click(click, 5000);
    assert!(
        h.caret().is_some(),
        "after shrink+grow the click at {click:?} placed no caret; hovered = {:?}",
        h.hovered_nodes()
    );
    assert!(
        !h.caret_items().is_empty(),
        "THE BUG (visual): after shrink+grow the caret state exists but no CursorRect is \
         painted"
    );

    // And the drag must paint bands again.
    h.lw
        .process_mouse_drag_for_selection(click, LogicalPosition::new(40.0, 12.0));
    assert!(
        h.selection_items() > 0,
        "THE BUG (visual): after shrink+grow a drag paints no selection"
    );
}

/// The full device state on the REAL widget: initial text, click-to-focus, a
/// long string TYPED through the edit pipeline (the text lives in the content
/// OVERLAY — every relayout re-runs on the DOM's original text and re-applies
/// the edit), the caret reveal, THEN the live-resize shrink (text clips) and
/// grow-back. Selection must still work — and still PAINT.
#[test]
fn the_real_widget_survives_shrink_clip_then_grow_with_typed_text() {
    const CONTAINER: usize = 1;

    let mut h = Harness::new_widget("hello", 800.0, 600.0);
    // Instant caret reveal (the glide timer is armed in the dll, not here).
    h.lw.system_animations_override =
        Some(azul_core::resources::SystemAnimations::disabled());

    // Click into the initial value, focus the container (the shells' SetFocus).
    let click1 = LogicalPosition::new(20.0, 24.0);
    h.click(click1, 0);
    assert!(
        h.caret().is_some(),
        "premise broken: the widget click at {click1:?} placed no caret; hovered = {:?}",
        h.hovered_nodes()
    );
    h.focus(NodeId::new(CONTAINER));

    // Type enough that the value overflows a 160px window but fits 800px.
    h.type_text(" world and then a good deal of additional typed text");
    assert!(
        !h.caret_items().is_empty(),
        "premise broken: after typing, no caret is painted"
    );
    let typed_extent = h.text_extent();
    assert!(
        typed_extent > 200.0,
        "premise broken: typed value lays out at {typed_extent}px, too short to clip at 160px"
    );

    // Live resize: shrink until the field clips its text, then grow back.
    // The session is ALIVE through every step (the device state: the field
    // is focused while the user drags the window edge), so every resize
    // pass's display list must keep painting the caret.
    for w in [620.0, 430.0, 240.0, 160.0, 320.0, 560.0, 800.0] {
        h.resize(w, 600.0);
        assert!(
            !h.caret_items().is_empty(),
            "THE BUG (visual): the resize to {w}px dropped the caret from the display \
             list while the editing session is alive"
        );
    }

    // Baseline pixels before the click, so the caret/selection deltas below
    // measure what actually reaches the screen (items CAN be present in the
    // DL yet clipped away by a stale clip — the degenerate-clip landmine).
    #[cfg(feature = "cpurender")]
    let mut glyphs = azul_layout::glyph_cache::GlyphCache::new();
    #[cfg(feature = "cpurender")]
    let before_click = h.render(&mut glyphs);
    #[cfg(feature = "cpurender")]
    let dl_before_click = h
        .lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .clone();

    // Click on visible text.
    let click2 = LogicalPosition::new(60.0, 24.0);
    h.click(click2, 5000);
    assert!(
        h.caret().is_some(),
        "THE BUG: after shrink+grow the widget click at {click2:?} placed no caret; \
         hovered = {:?}",
        h.hovered_nodes()
    );
    assert!(
        !h.caret_items().is_empty(),
        "THE BUG (visual): after shrink+grow the caret state exists but no CursorRect is \
         painted"
    );
    #[cfg(feature = "cpurender")]
    let after_click = h.render(&mut glyphs);
    #[cfg(feature = "cpurender")]
    assert!(
        pixel_diff(&before_click, &after_click) > 0,
        "THE BUG (pixels): the post-grow click changed no pixel — the caret is in the DL \
         but never reaches the screen (stale clip?)"
    );

    // The DAMAGE layer the CPU present paths run (`compute_display_list_damage`
    // in `cpurender/compositor.rs`): the caret the click just painted must be
    // covered by damage, else the present skips it and the screen stays stale
    // even with a perfect display list. `None` = full repaint = fine.
    #[cfg(feature = "cpurender")]
    {
        let dl_after_click = h
            .lw
            .layout_results
            .get(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .clone();
        assert!(
            !std::sync::Arc::ptr_eq(&dl_before_click, &dl_after_click),
            "THE BUG (damage): the click re-served the SAME display-list Arc — the \
             present path's ptr-eq shortcut reports zero damage and the caret never paints"
        );
        let offsets = azul_layout::cpurender::ScrollOffsetMap::default();
        if let Some(damage) = azul_layout::cpurender::compute_display_list_damage(
            &dl_before_click,
            &dl_after_click,
            &offsets,
            &offsets,
        ) {
            let caret = h.caret_items();
            let caret_rect = caret.first().copied().expect("caret item exists (asserted)");
            let covered = damage.iter().any(|d| {
                d.origin.x <= caret_rect.origin.x + caret_rect.size.width
                    && caret_rect.origin.x <= d.origin.x + d.size.width
                    && d.origin.y <= caret_rect.origin.y + caret_rect.size.height
                    && caret_rect.origin.y <= d.origin.y + d.size.height
            });
            assert!(
                covered,
                "THE BUG (damage): the click's damage {damage:?} does not cover the new \
                 caret at {caret_rect:?} — the present repaints the wrong region"
            );
        }
    }

    // Drag: bands must paint.
    h.lw
        .process_mouse_drag_for_selection(click2, LogicalPosition::new(140.0, 24.0));
    assert!(
        h.selection_items() > 0,
        "THE BUG (visual): after shrink+grow a drag on the widget paints no selection"
    );
    #[cfg(feature = "cpurender")]
    {
        let after_drag = h.render(&mut glyphs);
        assert!(
            pixel_diff(&after_click, &after_drag) > 0,
            "THE BUG (pixels): the drag painted no selection pixels after shrink+grow"
        );
    }
}

/// Count pixels that differ between two same-size renders.
#[cfg(feature = "cpurender")]
fn pixel_diff(a: &azul_layout::cpurender::AzulPixmap, b: &azul_layout::cpurender::AzulPixmap) -> usize {
    assert_eq!(a.width(), b.width());
    assert_eq!(a.height(), b.height());
    let (ad, bd) = (a.data(), b.data());
    (0..ad.len())
        .step_by(4)
        .filter(|&i| ad[i] != bd[i] || ad[i + 1] != bd[i + 1] || ad[i + 2] != bd[i + 2])
        .count()
}

/// The corpus-red case (`e2e/bug-textinput-resize-select-visual.json`), driven
/// through the direct harness: a resize that crosses a FULL-REGENERATION
/// boundary (orientation flip — the user's narrow-window resize does exactly
/// this) runs `clear_caches()` + a same-DOM relayout. After growing back, a
/// click must still paint a VISIBLE caret at the new position.
#[test]
fn click_paints_a_visible_caret_after_a_full_regen_resize_cycle() {
    let mut h = Harness::new(800.0, 600.0);

    // Premise click; capture where the caret paints and that it is visible.
    let click1 = LogicalPosition::new(60.0, 12.0);
    h.click(click1, 0);
    assert!(h.caret().is_some(), "premise broken: no caret at 800px");
    // The click-to-focus half lives in the shells' event pass — port it, as
    // the widget test does, so `caret_editable_is_focused` holds.
    h.focus(NodeId::new(1));
    h.lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    let alphas1 = h.caret_alphas();
    assert!(
        alphas1.iter().any(|(_, a)| *a > 0),
        "premise broken: the caret paints with alpha 0 before any resize: {alphas1:?}"
    );

    // The full-regeneration resize cycle (shrink crosses orientation, grow
    // crosses back).
    h.full_regen_resize(160.0, 600.0);
    h.full_regen_resize(800.0, 600.0);

    #[cfg(feature = "cpurender")]
    let mut glyphs = azul_layout::glyph_cache::GlyphCache::new();
    #[cfg(feature = "cpurender")]
    let before_click = h.render(&mut glyphs);

    let click2 = LogicalPosition::new(160.0, 12.0);
    h.click(click2, 5000);
    assert!(
        h.caret().is_some(),
        "after the full-regen cycle the click placed no caret; hovered = {:?}",
        h.hovered_nodes()
    );
    let alphas2 = h.caret_alphas();
    assert!(
        !alphas2.is_empty(),
        "THE BUG: after the full-regen cycle no CursorRect is emitted at all"
    );
    assert!(
        alphas2.iter().any(|(_, a)| *a > 0),
        "THE BUG: after the full-regen cycle the caret paints with alpha 0 \
         (invisible): {alphas2:?}"
    );
    #[cfg(feature = "cpurender")]
    {
        let after_click = h.render(&mut glyphs);
        // Since the mid-pass focus-gate fix (2026-08-31) the caret SURVIVES
        // the full-regen cycle, so when both clicks resolve to the same
        // cluster the before/after frames are legitimately identical - the
        // old "the click changed a pixel" assertion measured the caret's
        // ABSENCE before the click. The invariant is stronger now: the
        // post-click frame actually PAINTS the visible caret. Sample its
        // centre column (dpi 1.0): on the white field background a painted
        // caret leaves non-white rows.
        let (rect, _alpha) = h
            .caret_alphas()
            .into_iter()
            .find(|(_, a)| *a > 0)
            .expect("a visible caret rect exists after the click");
        let w = after_click.width() as usize;
        let data = after_click.data();
        let cx = (rect.origin.x + rect.size.width / 2.0) as usize;
        let y0 = rect.origin.y.ceil() as usize + 1;
        let y1 = ((rect.origin.y + rect.size.height).floor() as usize).max(y0 + 1) - 1;
        let painted = (y0..y1)
            .filter(|y| {
                let i = (y * w + cx) * 4;
                (data[i], data[i + 1], data[i + 2]) != (255, 255, 255)
            })
            .count();
        assert!(
            painted * 2 >= y1.saturating_sub(y0),
            "THE BUG (pixels): the post-regen caret column x={cx} rows {y0}..{y1} is              unpainted ({painted} non-white rows); before/after diff was {}",
            pixel_diff(&before_click, &after_click)
        );
    }
}
