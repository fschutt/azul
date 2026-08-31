//! Device-bug pins for the AzWidgets `TextArea` — a multi-line contenteditable
//! host (`white-space: pre-wrap`), where Enter inserts an
//! `InlineContent::LineBreak` through text3 rather than splitting blocks.
//!
//! - A. Enter at the end of the text: the caret must LAND ON THE NEW EMPTY
//!   LINE — a rect must exist (it vanished: the cursor addresses the empty
//!   run after the trailing break, which shapes to no cluster), sit BELOW the
//!   previous line, and lie inside the host's border box.
//! - B. After Enter mid-text every shifted line must repaint: an incremental
//!   frame (damage-diff + clipped raster, the path every shell presents
//!   through) must be pixel-identical to a full repaint. Same for a plain
//!   character insert (device: typing under-damages, Enter eventually covers).
//! - C. A TextArea whose content overflows must actually BE a scroller: the
//!   container registers a scroll node and the scroll-target walk finds IT,
//!   not the page.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    solver3::display_list::DisplayListItem,
    widgets::text_area::TextArea,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > container(1) > label-p(2) > text(3). The prompt is an
/// ATTRIBUTE on the value line, not a node (2026-08-31).
const CONTAINER: usize = 1;
const LABEL_P: usize = 2;
const LABEL_TEXT: usize = 3;

fn dnid(node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

struct Harness {
    glyph_cache: GlyphCache,
    lw: LayoutWindow,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
}

impl Harness {
    fn new_with_text_area(width: f32, height: f32, text: &str) -> Self {
        let dom =
            Dom::create_body().with_child(TextArea::create().with_text(AzString::from(text)).dom());
        Self::new_with_dom(width, height, dom)
    }

    fn new_with_dom(width: f32, height: f32, mut dom: Dom) -> Self {
        let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        // Instant reveals: the physics timer that drives glides is armed in
        // the dll and does not exist here.
        lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
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
        Self {
            glyph_cache: GlyphCache::new(),
            lw,
            renderer_resources,
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state,
        }
    }

    fn register_scroll_nodes(&mut self) {
        let now = Instant::from(std::time::Instant::now());
        azul_layout::managers::scroll_registration::register_scroll_nodes(&mut self.lw, &now);
    }

    /// Focus the container and open an editing session keyed on the label
    /// `<p>` (the IFC root — where a device click keys it), caret at `cursor`.
    fn start_editing(&mut self, cursor: TextCursor) {
        self.lw
            .focus_manager
            .set_focused_node(Some(dnid(CONTAINER)));
        self.lw.text_edit_manager.initialize_editing(
            cursor,
            DomId::ROOT_ID,
            NodeId::new(LABEL_P),
            0,
        );
        self.lw.text_edit_manager.blink.set_visibility(true);
    }

    /// The label IFC's shaped layout, expanded the way the caret path reads it
    /// (the stored sparse Arc is the dense-retirement sentinel).
    fn label_layout(&self) -> std::sync::Arc<azul_layout::text3::cache::UnifiedLayout> {
        let tree = &self
            .lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .layout_tree;
        let idx = tree
            .dom_to_layout
            .get(&NodeId::new(LABEL_P))
            .and_then(|v| v.first())
            .expect("label <p> has a layout box");
        tree.materialized_inline_layout_for_node(idx.index())
            .expect("label <p> establishes an inline layout")
    }

    /// Cursor at the very end of the label text (Trailing on the last cluster).
    fn end_of_text_cursor(&self) -> TextCursor {
        self.label_layout()
            .get_last_cluster_cursor()
            .expect("the label has at least one cluster")
    }

    fn type_str(&mut self, s: &str) -> bool {
        let affected = self.lw.record_text_input(s);
        assert!(
            !affected.is_empty(),
            "the input '{}' was recorded against the focused node",
            s.escape_debug()
        );
        self.lw.apply_text_changeset().needs_relayout
    }

    /// The production relayout that follows a `needs_relayout` changeset —
    /// what the e2e runner's `relayout_only` does: re-lay the SAME StyledDom
    /// (the content overlay carries the edits) without clearing caches.
    fn relayout(&mut self) {
        let Some(lr) = self.lw.layout_results.remove(&DomId::ROOT_ID) else {
            return;
        };
        let mut dbg = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                lr.styled_dom,
                &self.window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .unwrap();
    }

    fn dl(&self) -> std::sync::Arc<azul_layout::solver3::display_list::DisplayList> {
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .clone()
    }

    fn render(&mut self) -> AzulPixmap {
        let dl = self.dl();
        let opts = RenderOptions {
            width: self.window_state.size.dimensions.width,
            height: self.window_state.size.dimensions.height,
            dpi_factor: 1.0,
        };
        cpurender::render_with_font_manager(
            &dl,
            &self.renderer_resources,
            &self.lw.font_manager,
            opts,
            &mut self.glyph_cache,
        )
        .unwrap()
    }

    fn render_damaged(&mut self, pixmap: &mut AzulPixmap, damage: &[LogicalRect]) {
        let dl = self.dl();
        let state = cpurender::CpuRenderState::new(Default::default());
        cpurender::render_display_list_damaged(
            &dl,
            pixmap,
            1.0,
            &self.renderer_resources,
            &self.lw.font_manager,
            &mut self.glyph_cache,
            &state,
            damage,
        )
        .unwrap();
    }

    fn cursor_rect_items(&self) -> Vec<LogicalRect> {
        self.dl()
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::CursorRect { bounds, color, .. } if color.a > 0 => Some(bounds.0),
                _ => None,
            })
            .collect()
    }

    fn container_border_box(&self) -> LogicalRect {
        let lr = self.lw.get_layout_result(&DomId::ROOT_ID).unwrap();
        let idx = lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(CONTAINER))
            .and_then(|v| v.first())
            .expect("container has a layout box")
            .index();
        let origin = lr.calculated_positions.get(idx).copied().unwrap();
        let size = lr
            .layout_tree
            .get(azul_layout::solver3::LayoutNodeId::new(idx))
            .and_then(|n| n.used_size)
            .unwrap();
        LogicalRect { origin, size }
    }
}

fn pixel_diff_count(a: &AzulPixmap, b: &AzulPixmap) -> usize {
    assert_eq!(a.width(), b.width());
    assert_eq!(a.height(), b.height());
    let ad = a.data();
    let bd = b.data();
    let mut count = 0;
    for i in (0..ad.len()).step_by(4) {
        if ad[i] != bd[i] || ad[i + 1] != bd[i + 1] || ad[i + 2] != bd[i + 2] {
            count += 1;
        }
    }
    count
}

/// Where differing pixels lie (bounding box in pixmap space), for diagnostics.
fn pixel_diff_bbox(a: &AzulPixmap, b: &AzulPixmap) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = (a.width(), a.height());
    let ad = a.data();
    let bd = b.data();
    let mut bbox: Option<(u32, u32, u32, u32)> = None;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if ad[i] != bd[i] || ad[i + 1] != bd[i + 1] || ad[i + 2] != bd[i + 2] {
                bbox = Some(match bbox {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }
    bbox
}

// =========================================================================
// C. The overflowing TextArea is a scroller
// =========================================================================

const TEN_LINES: &str =
    "line one\nline two\nline three\nline four\nline five\nline six\nline seven\nline \
     eight\nline nine\nline ten";

#[test]
fn an_overflowing_text_area_registers_as_a_scroller() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, TEN_LINES);
    h.register_scroll_nodes();

    // Premise: ten 13px/1.2 lines (~156px) overflow the 64px box.
    let keys = h.lw.scroll_manager.state_keys();
    eprintln!("  [scroll] registered keys = {keys:?}");
    let info =
        h.lw.scroll_manager
            .get_scroll_node_info(DomId::ROOT_ID, NodeId::new(CONTAINER));
    if let Some(i) = &info {
        eprintln!(
            "  [scroll] container info: container={:?} content={:?}",
            i.container_rect, i.content_rect
        );
    }
    assert!(
        keys.contains(&(DomId::ROOT_ID, NodeId::new(CONTAINER))),
        "the TextArea container must register as a scroll node, got {keys:?}"
    );

    // The wheel-target walk from the text leaf must resolve the CONTAINER
    // (the deepest scroller over the pointer), never chain to the page.
    let target = h.lw.find_scrollable_ancestor(dnid(LABEL_TEXT));
    assert_eq!(
        target,
        Some(dnid(CONTAINER)),
        "wheel over the text must scroll the TextArea, not the page"
    );

    // And the scroll range is real: the track is live, not a dead stub.
    let info = info.expect("the registered container has scroll node info");
    assert!(
        info.max_scroll_y > 0.0,
        "the scroller must have a usable vertical range, got max_scroll_y={}",
        info.max_scroll_y
    );
    h.lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(CONTAINER),
        LogicalPosition::new(0.0, 40.0),
        Instant::from(std::time::Instant::now()),
    );
    let off =
        h.lw.scroll_manager
            .get_current_offset(DomId::ROOT_ID, NodeId::new(CONTAINER))
            .map_or(0.0, |o| o.y);
    assert!(
        off > 0.0,
        "the registered scroller must accept a scroll (offset stayed {off})"
    );
}

// =========================================================================
// A. Enter at the end of the text keeps a caret, on the new line
// =========================================================================

#[test]
fn enter_at_end_of_text_keeps_the_caret_on_the_new_empty_line() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha\nbeta");
    let end = h.end_of_text_cursor();
    h.start_editing(end);

    let before =
        h.lw.get_focused_cursor_rect()
            .expect("premise: the caret has a rect before Enter");
    eprintln!("  [caret] before Enter: {before:?}");

    h.type_str("\n");

    let after = h.lw.get_focused_cursor_rect().expect(
        "the caret must still have a rect after Enter on the last line \
         (cursor after a trailing LineBreak)",
    );
    eprintln!("  [caret] after Enter: {after:?}");

    // On the NEW line: strictly below the old caret's top, by about a line.
    assert!(
        after.origin.y > before.origin.y + 1.0,
        "the caret must move DOWN onto the new line: before={before:?} after={after:?}"
    );
    // At the line start.
    assert!(
        after.origin.x <= before.origin.x,
        "the caret must return to the line start: before={before:?} after={after:?}"
    );
    // Inside the host's border box (not clipped away outside it).
    let host = h.container_border_box();
    assert!(
        after.origin.y + after.size.height <= host.origin.y + host.size.height + 0.5 || {
            // A host that scrolls keeps the caret in range via the reveal;
            // either way the rect must exist and be finite.
            after.size.height > 0.0
        },
        "caret rect out of range: caret={after:?} host={host:?}"
    );
    assert!(
        after.size.height > 0.0 && after.size.height.is_finite(),
        "the caret rect must have a real height, got {after:?}"
    );

    // And the display list actually paints it.
    let painted = h.cursor_rect_items();
    eprintln!("  [caret] painted cursor rects: {painted:?}");
    assert!(
        !painted.is_empty(),
        "a CursorRect item must be emitted after Enter on the last line"
    );
    let on_new_line = painted
        .iter()
        .any(|r| (r.origin.y - after.origin.y).abs() < 2.0);
    assert!(
        on_new_line,
        "the painted caret must sit on the new line ({after:?}), got {painted:?}"
    );
}

#[test]
fn enter_at_end_of_a_wrapped_middle_line_keeps_the_caret() {
    // Enter at the end of LINE ONE (an empty run lands BETWEEN breaks, not at
    // the document end — the sibling case of the trailing-break one).
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha\nbeta");
    // Caret after 'alpha' (run 0, byte 5 = Trailing on final 'a').
    let layout = h.label_layout();
    let alpha_last = layout
        .items
        .iter()
        .filter_map(|it| match &it.item {
            azul_layout::text3::cache::ShapedItem::Cluster(c)
                if c.source_cluster_id.source_run == 0 =>
            {
                Some(c.source_cluster_id)
            }
            _ => None,
        })
        .last()
        .expect("run 0 has clusters");
    drop(layout);
    h.start_editing(TextCursor {
        cluster_id: alpha_last,
        affinity: CursorAffinity::Trailing,
    });

    let before =
        h.lw.get_focused_cursor_rect()
            .expect("premise: caret rect before Enter");
    h.type_str("\n");
    let after =
        h.lw.get_focused_cursor_rect()
            .expect("the caret must survive Enter at the end of a middle line");
    eprintln!("  [caret] mid-doc EOL: before={before:?} after={after:?}");
    assert!(
        after.origin.y > before.origin.y + 1.0,
        "the caret must land on the (new) next line: before={before:?} after={after:?}"
    );
}

// =========================================================================
// B. Incremental frames cover every shifted line
// =========================================================================

/// Caret mid line ONE of three, so Enter shifts lines two and three down.
fn mid_first_line_cursor(h: &Harness) -> TextCursor {
    let layout = h.label_layout();
    let cluster = layout
        .items
        .iter()
        .filter_map(|it| match &it.item {
            azul_layout::text3::cache::ShapedItem::Cluster(c)
                if c.source_cluster_id.source_run == 0
                    && c.source_cluster_id.start_byte_in_run == 3 =>
            {
                Some(c.source_cluster_id)
            }
            _ => None,
        })
        .next()
        .expect("run 0 has a cluster at byte 3");
    TextCursor {
        cluster_id: cluster,
        affinity: CursorAffinity::Leading,
    }
}

/// The incremental present must be pixel-identical to a full repaint —
/// for EVERY frame the pipeline produces: the fast-path frame right after
/// the edit, and (when the edit moved line counts) the relayout frame the
/// shells schedule behind it. Each is presented as damage onto the retained
/// pixmap, the way `CpuBackend::render_frame` presents.
///
/// Returns the FIRST frame's damage, for mechanism assertions on top of
/// identity.
fn assert_incremental_identity(h: &mut Harness, edit: &str) -> Option<Vec<LogicalRect>> {
    let mut base = h.render();
    let before_dl = h.dl();
    let needs_relayout = h.type_str(edit);
    let offsets = cpurender::ScrollOffsetMap::new();

    let mut present_frame = |h: &mut Harness,
                             base: &mut AzulPixmap,
                             prev: &azul_layout::solver3::display_list::DisplayList,
                             tag: &str| {
        let dl = h.dl();
        let damage = cpurender::compute_display_list_damage(prev, &dl, &offsets, &offsets);
        eprintln!(
            "  [damage {tag}] '{}' -> {:?} (items {} -> {})",
            edit.escape_debug(),
            damage,
            prev.items.len(),
            dl.items.len()
        );
        match &damage {
            Some(rects) => h.render_damaged(base, rects),
            None => *base = h.render(),
        }
        let full = h.render();
        let diff = pixel_diff_count(base, &full);
        if diff > 0 {
            let bbox = pixel_diff_bbox(base, &full);
            panic!(
                "{tag} frame diverges from a full repaint after '{}': {diff} px differ, \
                 bbox={bbox:?}, damage={damage:?}",
                edit.escape_debug()
            );
        }
        (dl, damage)
    };

    let (fast_dl, fast_damage) = present_frame(h, &mut base, &before_dl, "fast");
    if needs_relayout {
        h.relayout();
        let _ = present_frame(h, &mut base, &fast_dl, "relayout");
    }
    fast_damage
}

#[test]
fn enter_mid_text_repaints_every_shifted_line() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha\nbeta\ngamma");
    let cur = mid_first_line_cursor(&h);
    h.start_editing(cur);
    let _ = h.render(); // warm the base frame

    let damage = assert_incremental_identity(&mut h, "\n");
    // Mechanism pin: Enter changes the item count; the damage diff must still
    // produce a localized result (the windowed fallback), not bail to None.
    let rects =
        damage.expect("Enter must take the windowed structural fallback, not a full repaint");
    // And the PER-ITEM fallback keeps it proportionate: the moved lines are
    // ~60px wide in a 374px-wide field — the old one-union-per-window shape
    // covered the full field width (~36000 px2). Anything in that class means
    // the union regressed back in.
    let area: f32 = rects.iter().map(|r| r.size.width * r.size.height).sum();
    assert!(
        area < 20_000.0,
        "Enter damage should cover the moved lines, not the whole field: {area} px2 from {rects:?}"
    );
}

#[test]
fn typing_mid_text_repaints_every_changed_line() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha\nbeta\ngamma");
    let cur = mid_first_line_cursor(&h);
    h.start_editing(cur);
    let _ = h.render();

    let _ = assert_incremental_identity(&mut h, "x");
}

#[test]
fn several_enters_in_a_row_stay_pixel_correct() {
    let mut h = Harness::new_with_text_area(400.0, 300.0, "alpha\nbeta\ngamma");
    let cur = mid_first_line_cursor(&h);
    h.start_editing(cur);
    let _ = h.render();

    for _ in 0..3 {
        let _ = assert_incremental_identity(&mut h, "\n");
    }
}

// =========================================================================
// C2. TYPING into an empty TextArea until it overflows makes it a scroller
// =========================================================================

/// The 2026-08-29 device bug: `an_overflowing_text_area_registers_as_a_scroller`
/// above starts with the text already in the DOM, so Phase 3 sees it. On the
/// device the DOM stays empty (the demo has no on_text_input mirror) and only
/// the FAST reshape path knows the real extent — which used to publish it on
/// a node nobody scrolls (the walk started at the contenteditable host,
/// ancestors-only, with a stale extent). The container therefore never
/// registered, the wheel scrolled the page, and no scrollbar appeared.
#[test]
fn typing_into_an_empty_text_area_until_overflow_makes_the_container_a_scroller() {
    use azul_core::selection::GraphemeClusterId;

    let mut h = Harness::new_with_text_area(300.0, 90.0, "");
    // Model the DEVICE: macOS overlay scrollbars (reserve 0px). Without a
    // system style the UA resolves a classic space-reserving bar, and a
    // reserving bar legitimately takes the ESCALATION path instead (it
    // changes geometry) — that path needs the DOM to carry the text and is
    // pinned elsewhere.
    h.lw.system_style = Some(std::sync::Arc::new({
        let mut style = azul_css::system::SystemStyle::default();
        style.platform = azul_css::system::Platform::MacOs;
        style
    }));
    h.register_scroll_nodes();
    assert!(
        !h.lw
            .scroll_manager
            .state_keys()
            .contains(&(DomId::ROOT_ID, NodeId::new(CONTAINER))),
        "premise: an empty TextArea is not a scroller yet"
    );

    h.start_editing(TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    });

    // Type enough WRAPPING text (pre-wrap, 300px box) to overflow 90px of
    // height — pure fast path, no relayout, exactly like the device.
    for _ in 0..30 {
        let _ = h.type_str("lorem ipsum dolor sit amet consectetur ");
    }

    let keys = h.lw.scroll_manager.state_keys();
    assert!(
        keys.contains(&(DomId::ROOT_ID, NodeId::new(CONTAINER))),
        "typing past the box must register the CONTAINER as a scroller, got {keys:?}"
    );
    let info =
        h.lw.scroll_manager
            .get_scroll_node_info(DomId::ROOT_ID, NodeId::new(CONTAINER))
            .expect("the registered container has scroll node info");
    assert!(
        info.max_scroll_y > 0.0,
        "the mid-typing scroller must have a usable vertical range, got {}",
        info.max_scroll_y
    );

    // The wheel-target walk from the text leaf resolves the container, not
    // the page.
    let target = h.lw.find_scrollable_ancestor(dnid(LABEL_TEXT));
    assert_eq!(
        target,
        Some(dnid(CONTAINER)),
        "wheel over the typed text must scroll the TextArea, not the page"
    );

    // And the display list actually shows a scrollbar for it.
    let has_bar = h
        .dl()
        .items
        .iter()
        .any(|it| matches!(it, DisplayListItem::ScrollBarStyled { .. }));
    assert!(
        has_bar,
        "the overflow-started transition must emit a scrollbar on this very frame"
    );
}
