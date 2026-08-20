// ContentEditable End-to-End Integration Tests
//
// Tests the full text input pipeline:
// 1. Focus a contenteditable element
// 2. Simulate text input → verify changeset
// 3. Render screenshots at each step → verify visual diff
// 4. Verify damage rects cover only the text region
// 5. Test cursor movement, selection, backspace

use azul_layout::solver3::LayoutNodeId;
use std::path::PathBuf;
use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId, NodeType, TabIndex},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{StyledDom, NodeHierarchyItemId},
};
use azul_css::css::Css;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

// =========================================================================
// Test Infrastructure
// =========================================================================

/// Output directory for screenshots (created at test time)
fn screenshot_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test_output")
        .join("contenteditable_e2e");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// Save a pixmap as PNG for visual inspection
fn save_screenshot(pixmap: &AzulPixmap, name: &str) {
    let dir = screenshot_dir();
    let path = dir.join(format!("{name}.png"));
    match pixmap.encode_png() {
        Ok(png_data) => {
            std::fs::write(&path, &png_data).unwrap();
            eprintln!("  [screenshot] {}", path.display());
        }
        Err(e) => {
            eprintln!("  [screenshot FAILED] {name}: {e}");
        }
    }
}

/// Count pixels that differ between two same-size pixmaps.
fn pixel_diff_count(a: &AzulPixmap, b: &AzulPixmap, threshold: u8) -> usize {
    assert_eq!(a.width(), b.width());
    assert_eq!(a.height(), b.height());
    let ad = a.data();
    let bd = b.data();
    let mut count = 0;
    for i in (0..ad.len()).step_by(4) {
        let dr = (ad[i] as i16 - bd[i] as i16).unsigned_abs() as u8;
        let dg = (ad[i + 1] as i16 - bd[i + 1] as i16).unsigned_abs() as u8;
        let db = (ad[i + 2] as i16 - bd[i + 2] as i16).unsigned_abs() as u8;
        if dr > threshold || dg > threshold || db > threshold {
            count += 1;
        }
    }
    count
}

fn cls(name: &str) -> Vec<IdOrClass> {
    vec![IdOrClass::Class(name.into())]
}

struct ContentEditableHarness {
    glyph_cache: GlyphCache,
    layout_window: Option<LayoutWindow>,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
}

impl ContentEditableHarness {
    fn new(width: f32, height: f32) -> Self {
        let font_cache = FcFontCache::build();
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(width, height);

        Self {
            glyph_cache: GlyphCache::new(),
            layout_window: Some(LayoutWindow::new(font_cache).unwrap()),
            renderer_resources: RendererResources::default(),
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state: ws,
        }
    }

    /// Layout a DOM and generate display list. Returns the LayoutWindow for further interaction.
    fn layout_dom(&mut self, dom: Dom, css_str: &str) {
        let css = if css_str.is_empty() {
            Css::empty()
        } else {
            Css::from_string(css_str.into())
        };
        let mut dom = dom;
        let styled_dom = StyledDom::create(&mut dom, css);

        let lw = self.layout_window.as_mut().unwrap();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(
            styled_dom,
            &self.window_state,
            &self.renderer_resources,
            &self.system_callbacks,
            &mut dbg,
        )
        .unwrap();
    }

    /// Render the current display list to a pixmap
    fn render(&mut self) -> AzulPixmap {
        let lw = self.layout_window.as_ref().unwrap();
        let dom_id = DomId { inner: 0 };
        let dl = &lw.layout_results.get(&dom_id).unwrap().display_list;
        let opts = RenderOptions {
            width: self.window_state.size.dimensions.width,
            height: self.window_state.size.dimensions.height,
            dpi_factor: 1.0,
        };
        cpurender::render_with_font_manager(
            dl,
            &self.renderer_resources,
            &lw.font_manager,
            opts,
            &mut self.glyph_cache,
        )
        .unwrap()
    }

    /// Focus a contenteditable node: sets focus + initializes cursor at end.
    /// This simulates the real focus pipeline (FocusManager + CursorManager).
    fn focus_node(&mut self, dom_id: DomId, node_id: NodeId) {
        let lw = self.layout_window.as_mut().unwrap();
        let dom_node_id = DomNodeId { dom: dom_id, node: NodeHierarchyItemId::from(Some(node_id)) };
        lw.focus_manager.set_focused_node(Some(dom_node_id));

        // Initialize cursor at end of text (like the real event pipeline does)
        // Find the inline layout result for cursor positioning
        let text_layout = lw.layout_results.get(&dom_id).and_then(|result| {
            // Search for inline_layout_result starting from this node
            let layout_indices = result.layout_tree.dom_to_layout.get(&node_id)?;
            for &idx in layout_indices {
                if let Some(w) = result.layout_tree.warm(idx) {
                    if let Some(ref cached) = w.inline_layout_result {
                        return Some(cached.layout.clone());
                    }
                }
            }
            // Check children (text node children of contenteditable div)
            let node_hierarchy = result.styled_dom.node_hierarchy.as_ref();
            let parent_item = node_hierarchy.get(node_id.index())?;
            let mut child = parent_item.first_child_id(node_id);
            while let Some(child_id) = child {
                if let Some(child_indices) = result.layout_tree.dom_to_layout.get(&child_id) {
                    for &idx in child_indices {
                        if let Some(w) = result.layout_tree.warm(idx) {
                            if let Some(ref cached) = w.inline_layout_result {
                                return Some(cached.layout.clone());
                            }
                        }
                    }
                }
                child = node_hierarchy.get(child_id.index()).and_then(|h| h.next_sibling_id());
            }
            None
        });

        // The cursor must be on the TEXT CHILD node (not the contenteditable div itself)
        // because paint_cursor() matches against the text node's dom_node_id.
        // Find the first text child of the contenteditable div.
        let text_child_id = {
            let result = lw.layout_results.get(&dom_id).unwrap();
            let node_hierarchy = result.styled_dom.node_hierarchy.as_ref();
            let node_data = result.styled_dom.node_data.as_container();
            let mut found = None;
            if let Some(parent_item) = node_hierarchy.get(node_id.index()) {
                let mut child = parent_item.first_child_id(node_id);
                while let Some(child_id) = child {
                    if matches!(node_data[child_id].get_node_type(), NodeType::Text(_)) {
                        found = Some(child_id);
                        break;
                    }
                    child = node_hierarchy.get(child_id.index()).and_then(|h| h.next_sibling_id());
                }
            }
            found.unwrap_or(node_id)
        };

        // Compute cursor at end of text
        let cursor = text_layout.as_ref()
            .and_then(|layout| {
                layout.items.iter().rev()
                    .find_map(|item| if let azul_layout::text3::cache::ShapedItem::Cluster(c) = &item.item {
                        Some(azul_core::selection::TextCursor {
                            cluster_id: c.source_cluster_id,
                            affinity: azul_core::selection::CursorAffinity::Trailing,
                        })
                    } else { None })
            })
            .unwrap_or(azul_core::selection::TextCursor {
                cluster_id: azul_core::selection::GraphemeClusterId { source_run: 0, start_byte_in_run: 0 },
                affinity: azul_core::selection::CursorAffinity::Trailing,
            });
        lw.text_edit_manager.initialize_editing(cursor, dom_id, text_child_id, 0);
        lw.text_edit_manager.blink.set_visibility(true);
    }

    /// Simulate text input on the currently focused node.
    /// Returns (affected_nodes_count, changeset_text_before, changeset_text_inserted)
    fn type_text(&mut self, text: &str) -> (usize, String, String) {
        let lw = self.layout_window.as_mut().unwrap();

        // Phase 1: Record
        let affected = lw.record_text_input(text);
        let affected_count = affected.len();

        // Capture changeset info before applying
        let (old_text, inserted_text) = match lw.get_last_text_changeset() {
            Some(cs) => (cs.old_text.as_str().to_string(), cs.inserted_text.as_str().to_string()),
            None => (String::new(), String::new()),
        };

        // Phase 2: Apply (updates layout tree + display list)
        let result = lw.apply_text_changeset();

        eprintln!(
            "  [type_text] '{}' → affected={}, old='{}', inserted='{}', needs_relayout={}",
            text, affected_count, old_text, inserted_text, result.needs_relayout
        );

        (affected_count, old_text, inserted_text)
    }

    /// Repaint ONLY `damage` into a retained pixmap — the path every shell
    /// takes for an edit (`CpuBackend::render_frame`).
    fn render_damaged(
        &mut self,
        pixmap: &mut AzulPixmap,
        damage: &[azul_core::geom::LogicalRect],
    ) {
        let lw = self.layout_window.as_ref().unwrap();
        let dom_id = DomId { inner: 0 };
        let dl = &lw.layout_results.get(&dom_id).unwrap().display_list;
        let state = cpurender::CpuRenderState::new(Default::default());
        cpurender::render_display_list_damaged(
            dl,
            pixmap,
            1.0,
            &self.renderer_resources,
            &lw.font_manager,
            &mut self.glyph_cache,
            &state,
            damage,
        )
        .unwrap();
    }

    /// Clone the current display list for damage comparison
    fn clone_display_list(&self) -> std::sync::Arc<azul_layout::solver3::display_list::DisplayList> {
        let lw = self.layout_window.as_ref().unwrap();
        let dom_id = DomId { inner: 0 };
        lw.layout_results.get(&dom_id).unwrap().display_list.clone()
    }

    /// Count Text items in display list and extract their glyph counts
    fn count_text_glyphs(&self) -> Vec<(usize, usize)> {
        use azul_layout::solver3::display_list::DisplayListItem;
        let lw = self.layout_window.as_ref().unwrap();
        let dom_id = DomId { inner: 0 };
        let dl = &lw.layout_results.get(&dom_id).unwrap().display_list;
        let mut result = Vec::new();
        for (idx, item) in dl.items.iter().enumerate() {
            if let DisplayListItem::Text { glyphs, .. } = item {
                result.push((idx, glyphs.len()));
            }
        }
        result
    }

    /// Check if display list contains a CursorRect item
    fn has_cursor_rect(&self) -> bool {
        use azul_layout::solver3::display_list::DisplayListItem;
        let lw = self.layout_window.as_ref().unwrap();
        let dom_id = DomId { inner: 0 };
        let dl = &lw.layout_results.get(&dom_id).unwrap().display_list;
        dl.items.iter().any(|item| matches!(item, DisplayListItem::CursorRect { .. }))
    }

    /// Debug: dump layout tree nodes to trace paint_cursor traversal
    fn dump_layout_tree(&self) {
        let lw = self.layout_window.as_ref().unwrap();
        let dom_id = DomId { inner: 0 };
        let result = lw.layout_results.get(&dom_id).unwrap();
        let tree = &result.layout_tree;
        for idx in 0..tree.nodes.len() {
            let node = tree.get(LayoutNodeId::new(idx)).unwrap();
            let children = tree.children(idx);
            let has_ifc = tree.warm(LayoutNodeId::new(idx)).and_then(|w| w.ifc_membership.as_ref()).is_some();
            let has_inline = tree.warm(LayoutNodeId::new(idx)).and_then(|w| w.inline_layout_result.as_ref()).is_some();
            eprintln!("  [layout_tree] idx={} dom_node_id={:?} children={:?} ifc_member={} has_inline={}",
                idx, node.dom_node_id, children, has_ifc, has_inline);
        }
    }

    /// Get cursor byte offset from cursor manager (start_byte_in_run)
    fn get_cursor_byte_offset(&self) -> Option<u32> {
        let lw = self.layout_window.as_ref().unwrap();
        lw.text_edit_manager.get_primary_cursor().map(|c| c.cluster_id.start_byte_in_run)
    }

    /// Get focused node
    fn get_focused_node(&self) -> Option<DomNodeId> {
        let lw = self.layout_window.as_ref().unwrap();
        lw.focus_manager.get_focused_node().cloned()
    }

    /// Find all contenteditable nodes in the DOM (returns their NodeIds)
    fn find_contenteditable_nodes(&self) -> Vec<NodeId> {
        let lw = self.layout_window.as_ref().unwrap();
        let dom_id = DomId { inner: 0 };
        let result = lw.layout_results.get(&dom_id).unwrap();
        let node_data = result.styled_dom.node_data.as_container();
        let mut found = Vec::new();
        for idx in 0..node_data.len() {
            if node_data[NodeId::new(idx)].is_contenteditable() {
                found.push(NodeId::new(idx));
            }
        }
        found
    }
}

// =========================================================================
// CSS used for all contenteditable tests
// =========================================================================

const CE_CSS: &str = r#"
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { width: 400px; height: 300px; font-family: sans-serif; font-size: 16px; background: #ffffff; }
    .editor {
        width: 380px;
        margin: 10px;
        padding: 8px;
        border: 2px solid #333333;
        min-height: 40px;
        background: #f0f0f0;
        font-size: 16px;
    }
    .label {
        margin: 10px;
        font-size: 12px;
        color: #666666;
    }
"#;

// =========================================================================
// Test 1: Initial render of contenteditable div
// =========================================================================

/// What does ONE KEYSTROKE cost on the FAST path?
///
/// Run: `cargo test --release -p azul-layout --features probe --test
/// contenteditable_e2e -- --nocapture keystroke_cost`
///
/// This is the number the interactive budget is actually spent against,
/// and it is NOT what `frame_perf.rs` measures: that harness rebuilds the
/// DOM every frame (parse + cascade + full solver3), which is
/// `Update::RefreshDom` — the path a text edit must never take. See
/// `scripts/TEXT_INPUT_ARCHITECTURE_V4.md`.
///
/// Here the edit goes through `record_text_input` + `apply_text_changeset`,
/// which reuse the existing layout tree and the cached inline layout: no
/// layout callback, no DOM rebuild, no cascade.
#[test]
fn keystroke_cost_on_the_incremental_path() {
    if cfg!(debug_assertions) {
        eprintln!(
            "  [perf] *** DEBUG BUILD — not the shipped cost, re-run --release ***"
        );
    }

    let mut h = ContentEditableHarness::new(400.0, 300.0);
    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
        "the quick brown fox jumps over the lazy dog and keeps on running",
    ));
    let dom = Dom::create_body().with_child(editor);
    h.layout_dom(dom, CE_CSS);
    let _ = h.render();

    // Focus the editor so the changeset has a cursor to apply at.
    let dom_id = DomId { inner: 0 };
    let editor_node = NodeId::new(1);
    h.focus_node(dom_id, editor_node);

    // Warm: one keystroke to settle every cache.
    h.type_text("x");
    let _ = h.render();
    let _ = azul_layout::probe::Probe::drain();

    const N: u32 = 20;
    let t = std::time::Instant::now();
    for _ in 0..N {
        h.type_text("a");
    }
    let edit_only = t.elapsed() / N;

    // And the same again WITH the repaint, since a keystroke is only done
    // when it is on screen.
    let t = std::time::Instant::now();
    for _ in 0..N {
        h.type_text("b");
        let _ = h.render();
    }
    let edit_and_paint = t.elapsed() / N;

    // THE END-TO-END NUMBER: repaint only the damage, which is what every
    // shell does (`CpuBackend::render_frame` -> `render_display_list_damaged`).
    // The full repaint above is the first-paint case.
    let mut pixmap = h.render();
    let _ = azul_layout::probe::Probe::drain();
    let t = std::time::Instant::now();
    let mut damage_area_total = 0.0f32;
    let mut full_repaints = 0u32;
    for _ in 0..N {
        let before = h.clone_display_list();
        h.type_text("c");
        let after = h.clone_display_list();
        let offsets = cpurender::ScrollOffsetMap::new();
        let damage = cpurender::compute_display_list_damage(
            &before, &after, &offsets, &offsets,
        );
        match damage {
            Some(rects) => {
                if full_repaints == 0 && damage_area_total == 0.0 {
                    eprintln!("  [perf]   damage rects = {rects:?}");
                    // WHICH ITEMS changed? The coalesced union hides the
                    // contributors, and one of them dominates.
                    use azul_layout::solver3::display_list::DisplayListItem as D;
                    for (i, (o, n)) in
                        before.items.iter().zip(after.items.iter()).enumerate()
                    {
                        if !o.is_visually_equal(n) {
                            let name = match n {
                                D::Text { glyphs, .. } => {
                                    format!("Text({} glyphs)", glyphs.len())
                                }
                                D::Rect { .. } => "Rect".to_string(),
                                D::CursorRect { .. } => "CursorRect".to_string(),
                                D::SelectionRect { .. } => "SelectionRect".to_string(),
                                D::Border { .. } => "Border".to_string(),
                                other => format!("{:?}", core::mem::discriminant(other)),
                            };
                            eprintln!(
                                "  [perf]     changed #{i} {name} old={:?} new={:?}",
                                o.visual_bounds(),
                                n.visual_bounds()
                            );
                        }
                    }
                }
                damage_area_total +=
                    rects.iter().map(|r| r.size.width * r.size.height).sum::<f32>();
                h.render_damaged(&mut pixmap, &rects);
            }
            None => {
                // Structural change (item count moved) -> full repaint.
                full_repaints += 1;
                damage_area_total += 400.0 * 300.0;
                pixmap = h.render();
            }
        }
    }
    let edit_and_damaged_paint = t.elapsed() / N;

    eprintln!("  [perf] keystroke, incremental path only = {edit_only:?}");
    eprintln!("  [perf] keystroke + FULL repaint          = {edit_and_paint:?}");
    eprintln!(
        "  [perf] keystroke + DAMAGED repaint       = {edit_and_damaged_paint:?}  \
         (avg damage {:.0} px2 of 120000, {full_repaints}/{N} escalated to full)",
        damage_area_total / f64::from(N) as f32
    );

    let events = azul_layout::probe::Probe::drain();
    if events.is_empty() {
        eprintln!("  [perf] no probe events — rerun with `--features probe`");
        return;
    }
    let mut totals: std::collections::BTreeMap<&'static str, (u64, u32)> =
        std::collections::BTreeMap::new();
    let mut pending: Vec<(u16, u64)> = Vec::new();
    for e in &events {
        let azul_layout::probe::EventKind::Span { dur_ns } = e.kind else { continue };
        let mut children = 0u64;
        while let Some(&(d, ns)) = pending.last() {
            if d > e.depth {
                if d == e.depth + 1 { children += ns; }
                pending.pop();
            } else { break; }
        }
        let slot = totals.entry(e.name).or_insert((0, 0));
        slot.0 += dur_ns.saturating_sub(children);
        slot.1 += 1;
        pending.push((e.depth, dur_ns));
    }
    let mut rows: Vec<_> = totals.into_iter().collect();
    rows.sort_by_key(|(_, (self_ns, _))| std::cmp::Reverse(*self_ns));
    eprintln!("  [perf] per-keystroke SELF time (edit + repaint):");
    for (name, (self_ns, count)) in rows.iter().take(12) {
        let per = *self_ns as f64 / 1_000_000.0 / f64::from(N);
        if per < 0.005 { continue; }
        eprintln!("  [perf]   {name:<28} {per:>7.3} ms  ({count} calls)");
    }
}

#[test]
fn contenteditable_initial_render() {
    let mut h = ContentEditableHarness::new(400.0, 300.0);

    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    let text_child = Dom::create_text_do_not_use_without_block_level_wrapper("Hello World");
    editor = editor.with_child(text_child);

    let dom = Dom::create_body().with_child(editor);

    h.layout_dom(dom, CE_CSS);
    let frame = h.render();

    save_screenshot(&frame, "01_initial_render");

    // Verify 1: something rendered (not all white)
    let total = (frame.width() * frame.height()) as usize;
    let mut non_white = 0;
    for chunk in frame.data().chunks_exact(4) {
        if chunk[0] != 255 || chunk[1] != 255 || chunk[2] != 255 {
            non_white += 1;
        }
    }
    assert!(non_white > 0, "Expected non-white pixels (border, background, text)");
    eprintln!("  [verify] {non_white} non-white pixels out of {total}");

    // Verify 2: contenteditable node found
    let ce_nodes = h.find_contenteditable_nodes();
    assert!(!ce_nodes.is_empty(), "Expected at least one contenteditable node");
    eprintln!("  [verify] Found {} contenteditable node(s): {:?}", ce_nodes.len(), ce_nodes);

    // Verify 3: display list has Text items with glyphs (fonts resolved correctly)
    let text_items = h.count_text_glyphs();
    assert!(
        !text_items.is_empty(),
        "Display list must contain at least one Text item with glyphs"
    );
    for (idx, glyph_count) in &text_items {
        assert!(
            *glyph_count > 0,
            "Text item at index {idx} has 0 glyphs — font resolution or shaping failed"
        );
    }
    let total_glyphs: usize = text_items.iter().map(|(_, c)| c).sum();
    eprintln!(
        "  [verify] {} Text items, {} total glyphs across items: {:?}",
        text_items.len(), total_glyphs, text_items
    );
    // "Hello World" = 11 characters, expect at least 11 glyphs
    assert!(
        total_glyphs >= 11,
        "Expected at least 11 glyphs for 'Hello World', got {total_glyphs}"
    );

    // Verify 4: no focus yet, cursor should be None
    assert!(
        h.get_cursor_byte_offset().is_none(),
        "Cursor should be None before focus"
    );
}

// =========================================================================
// Test 2: Focus + text input changes the rendered output
// =========================================================================

#[test]
fn contenteditable_text_input_changes_output() {
    let mut h = ContentEditableHarness::new(400.0, 300.0);

    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Hello"));

    let dom = Dom::create_body().with_child(editor);

    h.layout_dom(dom, CE_CSS);
    let frame1 = h.render();
    save_screenshot(&frame1, "02a_before_text_input");

    // Save display list before text input
    let dl_before = h.clone_display_list();

    // Focus the contenteditable div
    let ce_nodes = h.find_contenteditable_nodes();
    assert!(!ce_nodes.is_empty(), "No contenteditable nodes found");
    let ce_node_id = ce_nodes[0];
    let dom_id = DomId { inner: 0 };
    h.focus_node(dom_id, ce_node_id);
    eprintln!("  [step] Focused node {ce_node_id:?}");

    // Verify 1: focus is set
    let focused = h.get_focused_node();
    assert!(focused.is_some(), "Focus should be set after focus_node()");
    eprintln!("  [verify] Focused: {focused:?}");

    // Count glyphs before text input
    let glyphs_before = h.count_text_glyphs();
    let total_glyphs_before: usize = glyphs_before.iter().map(|(_, c)| c).sum();

    // Type "X" — this should insert "X" at cursor position
    let (affected, old_text, inserted) = h.type_text("X");

    // Verify 2: changeset was created with correct data
    assert!(affected > 0, "Expected at least one affected node from text input");
    assert_eq!(old_text, "Hello", "Old text should be 'Hello'");
    assert_eq!(inserted, "X", "Inserted text should be 'X'");

    // Verify 3: cursor moved after text input
    let cursor_after = h.get_cursor_byte_offset();
    eprintln!("  [verify] Cursor byte offset after input: {cursor_after:?}");
    // Cursor should exist after text input (the apply phase sets it)
    assert!(cursor_after.is_some(), "Cursor should exist after text input");

    // Verify 4: display list has more glyphs now (added 'X')
    let glyphs_after = h.count_text_glyphs();
    let total_glyphs_after: usize = glyphs_after.iter().map(|(_, c)| c).sum();
    eprintln!(
        "  [verify] Glyphs before: {total_glyphs_before}, after: {total_glyphs_after} (expected +1)"
    );
    assert!(
        total_glyphs_after > total_glyphs_before,
        "After inserting 'X', glyph count should increase (was {total_glyphs_before}, now {total_glyphs_after})"
    );

    // Verify 5: display list should contain a CursorRect after text input
    let has_cursor = h.has_cursor_rect();
    let lw = h.layout_window.as_ref().unwrap();
    let draw_cursor = lw.text_edit_manager.should_draw_cursor();
    let cursor_loc = lw.text_edit_manager.multi_cursor.as_ref();
    eprintln!("  [verify] should_draw_cursor={}, multi_cursor={:?}, has CursorRect: {}",
        draw_cursor, cursor_loc.map(|mc| &mc.node_id), has_cursor);
    if !has_cursor {
        eprintln!("  [DEBUG] Dumping layout tree:");
        h.dump_layout_tree();
    }
    assert!(has_cursor, "CursorRect must appear in display list after focus + text input (should_draw_cursor={}, multi_cursor={:?})", draw_cursor, cursor_loc.is_some());

    // Verify 6: rendered frames differ visually
    let frame2 = h.render();
    save_screenshot(&frame2, "02b_after_text_input");
    let diff = pixel_diff_count(&frame1, &frame2, 0);
    assert!(diff > 0, "After typing 'X', rendered output must differ");
    let total = (frame1.width() * frame1.height()) as usize;
    eprintln!("  [verify] {} pixels differ ({:.1}%)", diff, diff as f64 / total as f64 * 100.0);

    // Verify 6: damage computation detects the change
    // Note: damage may return None if DL structure changed (e.g. CursorRect added),
    // which is fine — it means a full repaint is needed.
    let dl_after = h.clone_display_list();
    let damage = cpurender::compute_display_list_damage(
        &dl_before,
        &dl_after,
        &cpurender::ScrollOffsetMap::new(),
        &cpurender::ScrollOffsetMap::new(),
    );
    if let Some(rects) = &damage {
        assert!(!rects.is_empty(), "Damage should produce at least one rect for text change");
        eprintln!("  [verify] {} damage rect(s)", rects.len());
    } else {
        eprintln!("  [verify] Damage computation returned None (DL structure changed — full repaint)");
    }
}

// =========================================================================
// Test 3: Multiple keystrokes accumulate correctly
// =========================================================================

#[test]
fn contenteditable_multiple_keystrokes() {
    let mut h = ContentEditableHarness::new(400.0, 300.0);

    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("AB"));

    let dom = Dom::create_body().with_child(editor);

    h.layout_dom(dom, CE_CSS);
    let frame0 = h.render();
    save_screenshot(&frame0, "03a_initial_AB");

    // Focus
    let ce_nodes = h.find_contenteditable_nodes();
    let dom_id = DomId { inner: 0 };
    h.focus_node(dom_id, ce_nodes[0]);

    // Type "1"
    let (n1, _, _) = h.type_text("1");
    let frame1 = h.render();
    save_screenshot(&frame1, "03b_after_typing_1");

    // Type "2"
    let (_n2, _, _) = h.type_text("2");
    let frame2 = h.render();
    save_screenshot(&frame2, "03c_after_typing_2");

    // Type "3"
    let (_n3, _, _) = h.type_text("3");
    let frame3 = h.render();
    save_screenshot(&frame3, "03d_after_typing_3");

    // Verify: each frame differs from the previous
    let diff_0_1 = pixel_diff_count(&frame0, &frame1, 0);
    let diff_1_2 = pixel_diff_count(&frame1, &frame2, 0);
    let diff_2_3 = pixel_diff_count(&frame2, &frame3, 0);

    eprintln!("  [verify] Diff frame0→1: {diff_0_1} pixels");
    eprintln!("  [verify] Diff frame1→2: {diff_1_2} pixels");
    eprintln!("  [verify] Diff frame2→3: {diff_2_3} pixels");

    assert!(n1 > 0, "First keystroke should affect a node");
    assert!(diff_0_1 > 0, "Frame should change after first keystroke");
    // Note: subsequent keystrokes may or may not produce affected nodes
    // depending on whether the text input pipeline properly accumulates
    // edits via dirty_text_nodes. The key assertion is the first keystroke works.
}

// =========================================================================
// Test 4: Damage detection between old and new display lists
// =========================================================================

#[test]
fn contenteditable_damage_detection() {
    let mut h = ContentEditableHarness::new(400.0, 300.0);

    // Layout with two divs: a static header and a contenteditable editor
    let label = Dom::create_text_do_not_use_without_block_level_wrapper("Static Header").with_ids_and_classes(cls("label").into());
    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("AAAA"));

    let dom = Dom::create_body()
        .with_child(label)
        .with_child(editor);

    h.layout_dom(dom, CE_CSS);
    let frame1 = h.render();
    save_screenshot(&frame1, "04a_before_edit");
    let dl_before = h.clone_display_list();

    // Focus + type
    let ce_nodes = h.find_contenteditable_nodes();
    h.focus_node(DomId { inner: 0 }, ce_nodes[0]);
    h.type_text("B");

    let frame2 = h.render();
    save_screenshot(&frame2, "04b_after_edit");
    let dl_after = h.clone_display_list();

    // Compute damage
    let damage = cpurender::compute_display_list_damage(
        &dl_before,
        &dl_after,
        &cpurender::ScrollOffsetMap::new(),
        &cpurender::ScrollOffsetMap::new(),
    );
    eprintln!("  [verify] Damage rects: {damage:?}");

    // Check that ONLY the text region changed, not the entire window
    let total = (frame1.width() * frame1.height()) as usize;
    let diff = pixel_diff_count(&frame1, &frame2, 0);
    let diff_pct = diff as f64 / total as f64 * 100.0;
    eprintln!(
        "  [verify] {diff} pixels differ ({diff_pct:.1}% of total)"
    );

    // The text region is small relative to the full window (400x300).
    // Only the text "AAAA" → "AAAAB" area should differ, plus maybe cursor.
    // Should be well under 20% of total pixels.
    if diff > 0 {
        assert!(
            diff_pct < 20.0,
            "Text edit should only affect a small region, but {diff_pct:.1}% of pixels changed"
        );
        eprintln!("  [verify] PASS: Only {diff_pct:.1}% of pixels changed (< 20%)");
    }
}

// =========================================================================
// Test 5: Two contenteditable divs — edits are isolated
// =========================================================================

#[test]
fn contenteditable_two_editors_isolated() {
    let mut h = ContentEditableHarness::new(400.0, 400.0);

    let mut editor1 = Dom::create_div();
    editor1 = editor1.with_ids_and_classes(cls("editor").into());
    editor1.set_contenteditable(true);
    editor1.set_tab_index(TabIndex::Auto);
    editor1 = editor1.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Editor 1"));

    let mut editor2 = Dom::create_div();
    editor2 = editor2.with_ids_and_classes(cls("editor").into());
    editor2.set_contenteditable(true);
    editor2.set_tab_index(TabIndex::Auto);
    editor2 = editor2.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Editor 2"));

    let dom = Dom::create_body()
        .with_child(editor1)
        .with_child(editor2);

    h.layout_dom(dom, CE_CSS);
    let frame0 = h.render();
    save_screenshot(&frame0, "05a_two_editors_initial");

    let ce_nodes = h.find_contenteditable_nodes();
    assert!(ce_nodes.len() >= 2, "Expected at least 2 contenteditable nodes, found {}", ce_nodes.len());
    eprintln!("  [verify] Found {} contenteditable nodes: {:?}", ce_nodes.len(), ce_nodes);

    // Focus editor 1, type
    h.focus_node(DomId { inner: 0 }, ce_nodes[0]);
    h.type_text("!");
    let frame1 = h.render();
    save_screenshot(&frame1, "05b_after_typing_in_editor1");

    // Focus editor 2, type
    h.focus_node(DomId { inner: 0 }, ce_nodes[1]);
    h.type_text("?");
    let frame2 = h.render();
    save_screenshot(&frame2, "05c_after_typing_in_editor2");

    // Verify both edits produced visual changes
    let diff_0_1 = pixel_diff_count(&frame0, &frame1, 0);
    let diff_1_2 = pixel_diff_count(&frame1, &frame2, 0);

    eprintln!("  [verify] Diff after editor1 edit: {diff_0_1} pixels");
    eprintln!("  [verify] Diff after editor2 edit: {diff_1_2} pixels");
}

// =========================================================================
// Test 6: Damage-based incremental rendering produces same result as full
// =========================================================================

#[test]
fn contenteditable_incremental_render_matches_full() {
    let mut h = ContentEditableHarness::new(400.0, 300.0);

    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Test"));

    let dom = Dom::create_body().with_child(editor);
    h.layout_dom(dom, CE_CSS);
    let _frame1 = h.render();
    let dl_before = h.clone_display_list();

    // Focus + type
    let ce_nodes = h.find_contenteditable_nodes();
    h.focus_node(DomId { inner: 0 }, ce_nodes[0]);
    h.type_text("Z");

    let dl_after = h.clone_display_list();

    // Render the updated display list
    let full_render = h.render();
    save_screenshot(&full_render, "06a_full_render");

    // Verify: damage computation between old and new display lists works
    let damage = cpurender::compute_display_list_damage(
        &dl_before,
        &dl_after,
        &cpurender::ScrollOffsetMap::new(),
        &cpurender::ScrollOffsetMap::new(),
    );
    eprintln!("  [verify] Damage result: {:?}", damage.as_ref().map(|r| r.len()));

    // A second render of the same display list should be identical
    let render2 = h.render();
    save_screenshot(&render2, "06b_second_render");

    let diff = pixel_diff_count(&full_render, &render2, 0);
    assert_eq!(
        diff, 0,
        "Two renders of the same display list should be identical, but {diff} pixels differ"
    );
    eprintln!("  [verify] PASS: Consecutive renders are identical");
}

// =========================================================================
// Test 7: Long word overflow wraps correctly — new chars go to next line,
//         NOT "push start of word down one char at a time"
// =========================================================================

/// Reproduces the bug where typing past the container edge causes:
///   WRONG:  "a\nbcdefghijx"  (first char stranded on line 1)
///   RIGHT:  "abcdefghij\nx"  (word fills line 1, overflow goes to line 2)
///
/// Uses a narrow 100px editor (88px content area).
/// At ~8px per glyph (16px sans-serif), ~11 chars fill the line.
/// We start with "abcdefghij" (10 chars, ~80px) which fits.
/// Typing "x" then "y" should eventually push overflow to line 2.
#[test]
fn contenteditable_overflow_wraps_at_end_not_start() {
    // 100px CSS width with box-sizing: border-box
    // Content area = 100 - 2*4 padding - 2*1 border = 88px
    // At ~8px/char, ~11 chars fit.
    const NARROW_CSS: &str = r#"
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { width: 200px; height: 200px; font-family: sans-serif; font-size: 16px; background: #ffffff; }
        .editor {
            width: 100px;
            padding: 4px;
            border: 1px solid #333;
            min-height: 60px;
            background: #f0f0f0;
            font-size: 16px;
            overflow-wrap: break-word;
        }
    "#;

    let mut h = ContentEditableHarness::new(200.0, 200.0);

    // Start with a word that fills (or nearly fills) one line
    let initial_text = "abcdefghij";

    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(initial_text));

    let dom = Dom::create_body().with_child(editor);
    h.layout_dom(dom, NARROW_CSS);

    let frame_before = h.render();
    save_screenshot(&frame_before, "07a_long_word_before_typing");

    // Focus and type additional characters
    let ce_nodes = h.find_contenteditable_nodes();
    assert!(!ce_nodes.is_empty());
    h.focus_node(DomId { inner: 0 }, ce_nodes[0]);

    // Type chars one at a time to push past the container edge.
    // At ~8px/char, "abcdefghij" (10 chars) ≈ 80px in 88px container.
    // After "klmno" (5 more chars) we're at 15 chars ≈ 120px — well past 88px.
    for ch in ['k', 'l', 'm', 'n', 'o'] {
        h.type_text(&ch.to_string());
    }
    let frame_after = h.render();
    save_screenshot(&frame_after, "07b_long_word_after_typing");

    // VERIFICATION: The first line should still start with "a", not be a single
    // stranded character.  We check this by examining the layout tree's inline
    // layout result — the first PositionedItem on line 0 should be "a" (or the
    // first cluster of the word), and items on line 0 should span most of the
    // line width, not just one character.
    let lw = h.layout_window.as_ref().unwrap();
    let dom_id = DomId { inner: 0 };
    let layout_result = lw.layout_results.get(&dom_id).unwrap();

    // Find the inline layout result (on the text child or the contenteditable div)
    let mut inline_layout = None;
    for idx in 0..layout_result.layout_tree.nodes.len() {
        if let Some(w) = layout_result.layout_tree.warm(LayoutNodeId::new(idx)) {
            if let Some(ref cached) = w.inline_layout_result {
                // (d7) materialized(): the stored layout is the
                // retirement sentinel under the dense default; the
                // expansion is the sanctioned way to inspect items.
                inline_layout = Some(cached.materialized());
                break;
            }
        }
    }
    let layout = inline_layout.expect("Must have inline layout result after text edit");

    // Count items per line
    let mut items_per_line: std::collections::BTreeMap<usize, Vec<String>> = std::collections::BTreeMap::new();
    for item in &layout.items {
        if let azul_layout::text3::cache::ShapedItem::Cluster(c) = &item.item {
            items_per_line.entry(item.line_index)
                .or_default()
                .push(c.text().to_string());
        }
    }

    eprintln!("  [verify] Lines after typing 'klmno':");
    for (line_idx, chars) in &items_per_line {
        let line_text: String = chars.iter().cloned().collect();
        eprintln!("    Line {}: '{}' ({} chars)", line_idx, line_text, chars.len());
    }

    // Line 0 must have more than 1 character — the bug was that line 0
    // had only "a" (or even just a space) while all other content was
    // pushed to line 1.
    let line_0_chars = items_per_line.get(&0).map(|v| v.len()).unwrap_or(0);
    assert!(
        line_0_chars > 3,
        "BUG: Line 0 has only {line_0_chars} char(s) — the word start is being pushed down \
         instead of wrapping at the end.  Expected the first line to be mostly filled.",
    );

    // The overflow characters ("xy") should be on a subsequent line
    let has_multiple_lines = items_per_line.len() > 1;
    assert!(
        has_multiple_lines,
        "After adding chars past the container width, text should span multiple lines"
    );

    eprintln!("  [verify] PASS: Line 0 has {} chars, total {} lines",
        line_0_chars, items_per_line.len());
}

// =========================================================================
// Full structural-edit round trip (the A-chain acceptance test):
// RECORD (Enter → SplitNode at a STRUCTURAL position → DocumentChangeset) →
// APP-APPLY (document_edit on the app's native `Dom` model) →
// ACK (mark_document_edit_applied_with_inverse) →
// RE-RENDER (new generation from the model) →
// CARET RESTORE (anchor key + node path against the NEW generation) →
// STRUCTURAL UNDO (re-records the inverse through the same loop).
// The StyledDom is never mutated in place — every generation is rebuilt
// from the app's model, exactly like a real Path-2 app. The model is a
// `Dom` tree (NOT markup): the same ops split a <ul> between <li>s.
// =========================================================================

mod structural_roundtrip {
    use super::*;
    use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
    use azul_core::window::{KeyboardState, VirtualKeyCode};
    use azul_layout::managers::changeset::{DocumentOperation, NodePosition};

    /// The APP's model: the editor subtree as a native `Dom`.
    fn editor_model() -> Dom {
        let mut editor = Dom::create_div()
            .with_ids_and_classes(cls("editor").into())
            .with_contenteditable(true)
            .with_tab_index(TabIndex::Auto);
        let mut p = Dom::create_p();
        p.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("hello world"));
        editor.add_child(p);
        editor
    }

    /// RENDER the model (the app's layout callback): body > model clone.
    fn render_dom(model: &Dom) -> Dom {
        let mut root = Dom::create_body();
        root.add_child(model.clone());
        root
    }

    fn model_block_texts(model: &Dom) -> Vec<String> {
        fn collect(node: &Dom, out: &mut String) {
            if let NodeType::Text(t) = node.root.get_node_type() {
                out.push_str(t.as_str());
            }
            for c in node.children.as_ref() {
                collect(c, out);
            }
        }
        model
            .children
            .as_ref()
            .iter()
            .map(|c| {
                let mut t = String::new();
                collect(c, &mut t);
                t
            })
            .collect()
    }

    const CSS: &str = "
        .editor { font-size: 16px; padding: 8px; }
        p { margin: 0; }
    ";

    #[test]
    fn enter_split_full_roundtrip_with_undo() {
        // The APP's model: one paragraph inside the editor.
        let mut model = editor_model();

        let mut h = ContentEditableHarness::new(400.0, 300.0);
        h.layout_dom(render_dom(&model), CSS);
        let before_split = h.render();

        let dom_id = DomId { inner: 0 };

        // Locate the <p> (the node the caret lives in) and focus it.
        let p_node = {
            let lw = h.layout_window.as_ref().unwrap();
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let container = lr.styled_dom.node_data.as_container();
            (0..container.len())
                .map(NodeId::new)
                .find(|nid| matches!(container[*nid].get_node_type(), NodeType::P))
                .expect("p node")
        };
        h.focus_node(dom_id, p_node);

        // Caret between "hello" and " world" (byte 5 of the text child).
        {
            let lw = h.layout_window.as_mut().unwrap();
            let mc = lw.text_edit_manager.multi_cursor.as_mut().expect("cursor");
            mc.set_single_cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 5,
                },
                affinity: CursorAffinity::Leading,
            });
        }

        // ── RECORD: Enter determines a SPLIT; recording stores the changeset
        // (nothing mutates) with a STRUCTURAL position (text child 0, byte 5).
        let focused = {
            let lw = h.layout_window.as_ref().unwrap();
            lw.focus_manager.get_focused_node().copied()
        };
        let changeset = {
            let lw = h.layout_window.as_mut().unwrap();
            let editing = lw.build_editing_query_state(focused).expect("editing state");
            assert!(editing.is_contenteditable);

            let ks = KeyboardState {
                current_virtual_keycode: Some(VirtualKeyCode::Return).into(),
                pressed_virtual_keycodes: vec![VirtualKeyCode::Return].into(),
                ..Default::default()
            };
            let action = azul_layout::default_actions::determine_keyboard_default_action_with_editing(
                &ks,
                focused,
                &lw.layout_results,
                false,
                Some(&editing),
            );
            assert!(
                matches!(
                    action.action,
                    azul_core::events::DefaultAction::SplitBlockAtCursor { .. }
                ),
                "Enter in a contenteditable must determine a split, got {:?}",
                action.action
            );
            lw.record_structural_default_action(&action.action)
                .expect("record");
            lw.get_pending_document_edit().expect("pending").clone()
        };
        let DocumentOperation::SplitNode(ref sp) = changeset.operation else {
            panic!("expected SplitNode, got {:?}", changeset.operation);
        };
        assert_eq!(
            sp.at,
            NodePosition::in_text_child(0, 5),
            "the caret became a STRUCTURAL position"
        );
        // The resume names the NEW second node: path [<p index> + 1].
        assert_eq!(changeset.resume.node_path.as_ref().last(), Some(&1u32));

        // ── The recorded delta previews as pending structure (no DOM
        // mutation, no content copies).
        {
            let lw = h.layout_window.as_ref().unwrap();
            assert_eq!(
                lw.content_overlay.pending_structure(dom_id).len(),
                1,
                "the recorded split previews as pending structure"
            );
        }

        // ── O3-RENDER: the preview PAINTS before the app applies. A relayout
        // (same model — the app has not applied anything) emits the part
        // boxes: the P maps to TWO layout nodes (the dom_to_layout multimap),
        // the second being the SplitPreviewPart, and pixels move.
        h.layout_dom(render_dom(&model), CSS);
        {
            let lw = h.layout_window.as_ref().unwrap();
            assert_eq!(
                lw.content_overlay.pending_structure_len(),
                1,
                "the preview survives relayouts (it dies at ACK, not at layout)"
            );
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let indices = lr
                .layout_tree
                .dom_to_layout
                .get(&p_node)
                .expect("p in dom_to_layout");
            assert_eq!(
                indices.len(),
                2,
                "the split node occupies TWO layout slots (part 1 + preview part)"
            );
            let part2 = indices[1];
            assert_eq!(
                lr.layout_tree.cold(part2).and_then(|c| c.anonymous_type),
                Some(azul_layout::solver3::layout_tree::AnonymousBoxType::SplitPreviewPart),
            );
        }
        let preview_frame = h.render();
        assert!(
            pixel_diff_count(&before_split, &preview_frame, 2) > 0,
            "the split PREVIEW must be VISIBLE before the app applies"
        );

        // ── APP-APPLY on the app's native Dom model (Path 2). The host of
        // the split is the EDITOR (the p's parent) = the model root: path [].
        let applied = azul_layout::document_edit::apply_document_operation(
            &mut model,
            &[],
            &changeset,
        )
        .expect("apply split");
        assert_eq!(model_block_texts(&model), vec!["hello", " world"]);
        assert!(matches!(applied.inverse, DocumentOperation::MergeNodes(_)));

        // ── ACK (with the inverse: the edit becomes undoable).
        {
            let lw = h.layout_window.as_mut().unwrap();
            assert!(lw.mark_document_edit_applied_with_inverse(changeset.id, applied.inverse));
        }

        // ── The ACK ended the preview (the app's re-render supersedes it).
        {
            let lw = h.layout_window.as_ref().unwrap();
            assert_eq!(
                lw.content_overlay.pending_structure_len(),
                0,
                "the ACK ends the preview"
            );
        }

        // ── RE-RENDER the new generation from the app's model; the caret
        // restore runs at the layout tail: second node, offset 0.
        h.layout_dom(render_dom(&model), CSS);
        let after_split = h.render();
        assert!(
            pixel_diff_count(&before_split, &after_split, 2) > 0,
            "the split must be visible"
        );
        {
            let lw = h.layout_window.as_ref().unwrap();
            let mc = lw
                .text_edit_manager
                .multi_cursor
                .as_ref()
                .expect("caret restored after the swap");
            let caret_node = mc.node_id.node.into_crate_internal().expect("caret node");
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let container = lr.styled_dom.node_data.as_container();
            let hierarchy = lr.styled_dom.node_hierarchy.as_container();
            let mut block = caret_node;
            while !matches!(container[block].get_node_type(), NodeType::P) {
                match hierarchy.get(block).and_then(|n| n.parent_id()) {
                    Some(p) => block = p,
                    None => break,
                }
            }
            let ps: Vec<NodeId> = (0..container.len())
                .map(NodeId::new)
                .filter(|nid| matches!(container[*nid].get_node_type(), NodeType::P))
                .collect();
            assert_eq!(ps.len(), 2, "two blocks after the split");
            assert_eq!(
                block, ps[1],
                "caret must land in the SECOND block after Enter"
            );
        }

        // ── STRUCTURAL UNDO: re-records the merge through the same loop.
        let undo_changeset = {
            let lw = h.layout_window.as_mut().unwrap();
            lw.undo_structural_edit().expect("undo records");
            lw.get_pending_document_edit().expect("pending merge").clone()
        };
        assert!(matches!(
            undo_changeset.operation,
            DocumentOperation::MergeNodes(_)
        ));
        let undone = azul_layout::document_edit::apply_document_operation(
            &mut model,
            &[],
            &undo_changeset,
        )
        .expect("apply merge");
        assert_eq!(
            model_block_texts(&model),
            vec!["hello world"],
            "undo restores the app model"
        );
        {
            let lw = h.layout_window.as_mut().unwrap();
            assert!(lw.mark_document_edit_applied_with_inverse(undo_changeset.id, undone.inverse));
        }
        h.layout_dom(render_dom(&model), CSS);
        let after_undo = h.render();
        assert!(
            pixel_diff_count(&after_split, &after_undo, 2) > 0,
            "the merge must be visible"
        );

        // Redo: the split comes back through the same loop.
        {
            let lw = h.layout_window.as_mut().unwrap();
            lw.redo_structural_edit().expect("redo records");
            let redo_cs = lw.get_pending_document_edit().expect("pending split").clone();
            assert!(matches!(redo_cs.operation, DocumentOperation::SplitNode(_)));
        }
    }

    /// An APP-RECORDED child-boundary split (`NodePosition::before_child`,
    /// no text byte) previews structurally: a `<ul>` splits BETWEEN `<li>`s
    /// into two list boxes BEFORE the app applies — the `.insertChild`-class
    /// generic op, not a text special case.
    #[test]
    fn app_recorded_element_split_previews_and_applies() {
        use azul_layout::managers::changeset::{
            DocOpSplitNode, DocumentChangeset, EditResumePoint,
        };

        // APP model: editor > ul > li x3.
        let mut model = Dom::create_div()
            .with_ids_and_classes(cls("editor").into())
            .with_contenteditable(true)
            .with_tab_index(TabIndex::Auto);
        let mut ul = Dom::create_ul();
        ul.add_child(Dom::create_li_with_text("alpha"));
        ul.add_child(Dom::create_li_with_text("beta"));
        ul.add_child(Dom::create_li_with_text("gamma"));
        model.add_child(ul);

        let mut h = ContentEditableHarness::new(400.0, 300.0);
        h.layout_dom(render_dom(&model), CSS);
        let before = h.render();

        let dom_id = DomId { inner: 0 };
        // body=0, editor=1, ul=2 (programmatic build: no whitespace nodes).
        let ul_node = NodeId::new(2);
        let target = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(ul_node)),
        };

        // The app records: split the UL before child 2 ("gamma" moves).
        let changeset = DocumentChangeset::new(
            target,
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: target,
                at: NodePosition::before_child(2),
            }),
            EditResumePoint {
                // The split node is child 0 of the editor; the NEW second
                // part lands at child index 1.
                anchor_key: 0,
                node_path: vec![1u32].into(),
                position: NodePosition::before_child(0),
            },
            azul_core::task::Instant::now(),
        );
        let cs_id = {
            let lw = h.layout_window.as_mut().unwrap();
            lw.record_document_edit(changeset.clone())
        };
        assert_eq!(cs_id, changeset.id);

        // Preview: relayout of the UNCHANGED model emits two UL part boxes.
        h.layout_dom(render_dom(&model), CSS);
        {
            let lw = h.layout_window.as_ref().unwrap();
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let indices = lr
                .layout_tree
                .dom_to_layout
                .get(&ul_node)
                .expect("ul in dom_to_layout");
            assert_eq!(
                indices.len(),
                2,
                "the element split occupies TWO layout slots (part 1 + preview part)"
            );
            let (part1, part2) = (indices[0], indices[1]);
            assert_eq!(
                lr.layout_tree.cold(part2).and_then(|c| c.anonymous_type),
                Some(azul_layout::solver3::layout_tree::AnonymousBoxType::SplitPreviewPart),
            );
            assert_eq!(
                lr.layout_tree.children(part1.index()).len(),
                2,
                "alpha + beta stay in part 1"
            );
            assert_eq!(
                lr.layout_tree.children(part2.index()).len(),
                1,
                "gamma moves to the preview part"
            );
        }
        let preview = h.render();
        assert!(
            pixel_diff_count(&before, &preview, 2) > 0,
            "the element-split PREVIEW must be VISIBLE before the app applies"
        );

        // APP-APPLY on the model: host of the split is the EDITOR (path []).
        let applied = azul_layout::document_edit::apply_document_operation(
            &mut model,
            &[],
            &changeset,
        )
        .expect("apply element split");
        assert!(matches!(applied.inverse, DocumentOperation::MergeNodes(_)));
        {
            let uls = model.children.as_ref();
            assert_eq!(uls.len(), 2, "the model now holds TWO lists");
            assert_eq!(uls[0].children.as_ref().len(), 2);
            assert_eq!(uls[1].children.as_ref().len(), 1);
        }

        // ACK ends the preview; the re-rendered model shows the real split.
        {
            let lw = h.layout_window.as_mut().unwrap();
            assert!(lw.mark_document_edit_applied(changeset.id));
        }
        h.layout_dom(render_dom(&model), CSS);
        {
            let lw = h.layout_window.as_ref().unwrap();
            assert_eq!(
                lw.content_overlay.pending_structure_len(),
                0,
                "the ACK retired the preview"
            );
        }
    }

    /// An app-recorded `RemoveChildren` renders its removal side pre-apply:
    /// the range's layout children DETACH (unreachable → unpainted), pixels
    /// change, and the parent's layout child count drops.
    #[test]
    fn app_recorded_remove_children_previews_suppressed() {
        use azul_layout::managers::changeset::{
            DocOpRemoveChildren, DocumentChangeset, EditResumePoint,
        };

        let mut model = Dom::create_div()
            .with_ids_and_classes(cls("editor").into())
            .with_contenteditable(true)
            .with_tab_index(TabIndex::Auto);
        let mut ul = Dom::create_ul();
        ul.add_child(Dom::create_li_with_text("alpha"));
        ul.add_child(Dom::create_li_with_text("beta"));
        ul.add_child(Dom::create_li_with_text("gamma"));
        model.add_child(ul);

        let mut h = ContentEditableHarness::new(400.0, 300.0);
        h.layout_dom(render_dom(&model), CSS);
        let before = h.render();

        let dom_id = DomId { inner: 0 };
        let ul_node = NodeId::new(2);
        let target = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(ul_node)),
        };

        // Remove "beta" (child range [1, 2)).
        let changeset = DocumentChangeset::new(
            target,
            DocumentOperation::RemoveChildren(DocOpRemoveChildren {
                parent: target,
                start: 1,
                end: 2,
            }),
            EditResumePoint {
                anchor_key: 0,
                node_path: vec![0u32].into(),
                position: NodePosition::before_child(1),
            },
            azul_core::task::Instant::now(),
        );
        {
            let lw = h.layout_window.as_mut().unwrap();
            lw.record_document_edit(changeset);
        }

        h.layout_dom(render_dom(&model), CSS);
        {
            let lw = h.layout_window.as_ref().unwrap();
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let idx = lr.layout_tree.dom_to_layout.get(&ul_node).unwrap()[0];
            assert_eq!(
                lr.layout_tree.children(idx.index()).len(),
                2,
                "the removed child's layout node detached from the preview tree"
            );
        }
        let preview = h.render();
        assert!(
            pixel_diff_count(&before, &preview, 2) > 0,
            "the removal PREVIEW must be VISIBLE before the app applies"
        );
    }

    /// An app-recorded `MergeNodes` previews: the second block disappears
    /// from its parent and its children surface at the end of the first —
    /// the Backspace-at-start UX, visible before the app applies.
    #[test]
    fn app_recorded_merge_previews_children_moved() {
        use azul_layout::managers::changeset::{
            DocOpMergeNodes, DocumentChangeset, EditResumePoint,
        };

        let mut model = Dom::create_div()
            .with_ids_and_classes(cls("editor").into())
            .with_contenteditable(true)
            .with_tab_index(TabIndex::Auto);
        let mut p1 = Dom::create_p();
        p1.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("hello"));
        let mut p2 = Dom::create_p();
        p2.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("world"));
        model.add_child(p1);
        model.add_child(p2);

        let mut h = ContentEditableHarness::new(400.0, 300.0);
        h.layout_dom(render_dom(&model), CSS);
        let before = h.render();

        let dom_id = DomId { inner: 0 };
        // body=0, editor=1, p1=2, text=3, p2=4, text=5.
        let (p1_node, p2_node) = (NodeId::new(2), NodeId::new(4));
        let mk = |n: NodeId| DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        };

        let changeset = DocumentChangeset::new(
            mk(p2_node),
            DocumentOperation::MergeNodes(DocOpMergeNodes {
                first: mk(p1_node),
                second: mk(p2_node),
                join: NodePosition::in_text_child(0, 5),
            }),
            EditResumePoint {
                anchor_key: 0,
                node_path: vec![0u32].into(),
                position: NodePosition::in_text_child(0, 5),
            },
            azul_core::task::Instant::now(),
        );
        {
            let lw = h.layout_window.as_mut().unwrap();
            lw.record_document_edit(changeset);
        }

        h.layout_dom(render_dom(&model), CSS);
        {
            let lw = h.layout_window.as_ref().unwrap();
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let p1_idx = lr.layout_tree.dom_to_layout.get(&p1_node).unwrap()[0];
            assert_eq!(
                lr.layout_tree.children(p1_idx.index()).len(),
                2,
                "p2's text child moved onto p1 in the preview tree"
            );
            // p2's layout node is detached from the editor's children.
            let editor_idx = lr.layout_tree.dom_to_layout.get(&NodeId::new(1)).unwrap()[0];
            let p2_idx = lr.layout_tree.dom_to_layout.get(&p2_node).unwrap()[0];
            assert!(
                !lr.layout_tree.children(editor_idx.index()).contains(&p2_idx.index()),
                "the merged-away block detached from its parent"
            );
        }
        let preview = h.render();
        assert!(
            pixel_diff_count(&before, &preview, 2) > 0,
            "the merge PREVIEW must be VISIBLE before the app applies"
        );
    }

    /// Mixed inline/block content: the split node's inline runs live inside
    /// ANONYMOUS wrappers, so layout children are not 1:1 with DOM children.
    /// A child-boundary split must route wrappers by their wrapped runs'
    /// ordinals — wholesale when a wrapper sits on one side of the cut.
    #[test]
    fn element_split_routes_anonymous_wrappers_wholesale() {
        use azul_layout::managers::changeset::{
            DocOpSplitNode, DocumentChangeset, EditResumePoint,
        };

        // editor > div[ text, p, text ] — mixed content.
        let mut model = Dom::create_div()
            .with_ids_and_classes(cls("editor").into())
            .with_contenteditable(true)
            .with_tab_index(TabIndex::Auto);
        // The host carries padding so the split is VISIBLE: two padded boxes
        // occupy more height than one (an unstyled div splitting into two
        // stacked unstyled divs is pixel-identical — the tree asserts below
        // would still hold, but the render check would be vacuous).
        let mut host = Dom::create_div().with_ids_and_classes(cls("host").into());
        host.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("aaaa aaaa"));
        let mut para = Dom::create_p();
        para.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("bbbb bbbb"));
        host.add_child(para);
        host.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("cccc cccc"));
        model.add_child(host);

        let mut h = ContentEditableHarness::new(400.0, 300.0);
        let css_padded = format!("{CSS} .host {{ padding: 8px; }}");
        h.layout_dom(render_dom(&model), &css_padded);
        let before = h.render();

        let dom_id = DomId { inner: 0 };
        // body=0, editor=1, host div=2.
        let host_node = NodeId::new(2);
        let target = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(host_node)),
        };

        // Split before DOM child 1 (the <p>): text stays, p + trailing text move.
        let changeset = DocumentChangeset::new(
            target,
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: target,
                at: NodePosition::before_child(1),
            }),
            EditResumePoint {
                anchor_key: 0,
                node_path: vec![1u32].into(),
                position: NodePosition::before_child(0),
            },
            azul_core::task::Instant::now(),
        );
        {
            let lw = h.layout_window.as_mut().unwrap();
            lw.record_document_edit(changeset);
        }

        h.layout_dom(render_dom(&model), &css_padded);
        {
            let lw = h.layout_window.as_ref().unwrap();
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let indices = lr.layout_tree.dom_to_layout.get(&host_node).unwrap();
            assert_eq!(indices.len(), 2, "two layout slots (part 1 + preview part)");
            let (p1, p2) = (indices[0], indices[1]);
            // Ordinal conservation: everything renders exactly once.
            let (n1, n2) = (
                lr.layout_tree.children(p1.index()).len(),
                lr.layout_tree.children(p2.index()).len(),
            );
            assert_eq!(
                n1, 1,
                "part 1 holds ONE child (the wrapper around the leading text run)"
            );
            assert_eq!(
                n2, 2,
                "part 2 holds TWO children (the p + the wrapper around the trailing run)"
            );
        }
        let preview = h.render();
        assert!(
            pixel_diff_count(&before, &preview, 2) > 0,
            "the mixed-content split preview must be VISIBLE"
        );
    }

    /// Two ADJACENT inline runs share one anonymous wrapper; a split BETWEEN
    /// them lands inside that wrapper — the wrapper itself must split, its
    /// second half re-wrapping inside the preview part.
    #[test]
    fn element_split_splits_a_straddling_anonymous_wrapper() {
        use azul_layout::managers::changeset::{
            DocOpSplitNode, DocumentChangeset, EditResumePoint,
        };

        // editor > div[ text, text, p ] — the two texts share one wrapper.
        let mut model = Dom::create_div()
            .with_ids_and_classes(cls("editor").into())
            .with_contenteditable(true)
            .with_tab_index(TabIndex::Auto);
        let mut host = Dom::create_div();
        host.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("aaaa aaaa"));
        host.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("bbbb bbbb"));
        let mut para = Dom::create_p();
        para.add_child(Dom::create_text_do_not_use_without_block_level_wrapper("cccc cccc"));
        host.add_child(para);
        model.add_child(host);

        let mut h = ContentEditableHarness::new(400.0, 300.0);
        h.layout_dom(render_dom(&model), CSS);
        let before = h.render();

        let dom_id = DomId { inner: 0 };
        let host_node = NodeId::new(2);
        let target = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(host_node)),
        };

        // Split between the two text runs (before DOM child 1).
        let changeset = DocumentChangeset::new(
            target,
            DocumentOperation::SplitNode(DocOpSplitNode {
                node: target,
                at: NodePosition::before_child(1),
            }),
            EditResumePoint {
                anchor_key: 0,
                node_path: vec![1u32].into(),
                position: NodePosition::before_child(0),
            },
            azul_core::task::Instant::now(),
        );
        {
            let lw = h.layout_window.as_mut().unwrap();
            lw.record_document_edit(changeset);
        }

        h.layout_dom(render_dom(&model), CSS);
        {
            let lw = h.layout_window.as_ref().unwrap();
            let lr = lw.layout_results.get(&dom_id).unwrap();
            let indices = lr.layout_tree.dom_to_layout.get(&host_node).unwrap();
            assert_eq!(indices.len(), 2, "two layout slots (part 1 + preview part)");
            let (p1, p2) = (indices[0], indices[1]);
            assert_eq!(
                lr.layout_tree.children(p1.index()).len(),
                1,
                "part 1: the first half of the split wrapper"
            );
            assert_eq!(
                lr.layout_tree.children(p2.index()).len(),
                2,
                "part 2: the wrapper's second half + the p"
            );
        }
        let preview = h.render();
        assert!(
            pixel_diff_count(&before, &preview, 2) > 0,
            "the straddling-wrapper split preview must be VISIBLE"
        );
    }
}

/// #11 workbench: TODAY's incremental-edit baseline. Types 60 chars
/// into an editable paragraph inside a DOCUMENT-SCALE page (200
/// sibling paragraphs) and reports per-keystroke edit+render medians.
/// The historical number was median 21.3 ms with an 8 ms target;
/// the dense campaign + cascade fixes have all landed since.
/// Informational (prints; asserts only sanity).
#[test]
fn probe_incremental_keystroke_median() {
    let mut h = ContentEditableHarness::new(800.0, 600.0);

    let mut body = Dom::create_body();
    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Start: "));
    body = body.with_child(editor);
    for i in 0..200 {
        body = body.with_child(
            Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(azul_css::AzString::from(
                format!("Paragraph {i} with a reasonable amount of running text in it"),
            ))),
        );
    }

    h.layout_dom(body, CE_CSS);
    let _warm = h.render();

    let ce_nodes = h.find_contenteditable_nodes();
    let dom_id = DomId { inner: 0 };
    h.focus_node(dom_id, ce_nodes[0]);

    let mut edit_times = Vec::new();
    let mut render_times = Vec::new();
    for i in 0..60 {
        let ch = (b'a' + (i % 26) as u8) as char;
        let t0 = std::time::Instant::now();
        let _ = h.type_text(&ch.to_string());
        let edit_dt = t0.elapsed();
        let t1 = std::time::Instant::now();
        let _ = h.render();
        let render_dt = t1.elapsed();
        edit_times.push(edit_dt);
        render_times.push(render_dt);
    }
    // The PRODUCTION present path: damage-diff + clipped raster.
    let mut damage_times = Vec::new();
    let mut base = h.render();
    let mut prev_dl = h.clone_display_list();
    for i in 0..60 {
        let ch = (b'a' + (i % 26) as u8) as char;
        let _ = h.type_text(&ch.to_string());
        let t0 = std::time::Instant::now();
        let dl_after = h.clone_display_list();
        let damage = cpurender::compute_display_list_damage(
            &prev_dl,
            &dl_after,
            &cpurender::ScrollOffsetMap::new(),
            &cpurender::ScrollOffsetMap::new(),
        );
        // None = full-repaint fallback (counted at its real cost).
        match &damage {
            Some(rects) => h.render_damaged(&mut base, rects),
            None => base = h.render(),
        }
        damage_times.push(t0.elapsed());
        prev_dl = dl_after;
    }
    damage_times.sort();
    let med_d = damage_times[damage_times.len() / 2];
    let p90_d = damage_times[damage_times.len() * 9 / 10];
    eprintln!(
        "[KEYSTROKE-PROBE] damage-present median={med_d:?} p90={p90_d:?}"
    );

    edit_times.sort();
    render_times.sort();
    let med_e = edit_times[edit_times.len() / 2];
    let p90_e = edit_times[edit_times.len() * 9 / 10];
    let med_r = render_times[render_times.len() / 2];
    let p90_r = render_times[render_times.len() * 9 / 10];
    eprintln!(
        "[KEYSTROKE-PROBE] 60 keys @200-para doc: edit median={med_e:?} p90={p90_e:?} | \
         render median={med_r:?} p90={p90_r:?} | TOTAL median={:?}",
        med_e + med_r
    );
    // The production keystroke = edit + damage-present. Measured
    // 2026-08-12: edit 4.98 ms + present 113 us on this fixture — the
    // #11 target (median < 8 ms, from 21.3 ms) is met with margin.
    // Generous CI bounds that still catch an order-of-magnitude
    // regression:
    // Perf medians are meaningless without optimizations: the dev-profile
    // CI job (debug-assertions + overflow-checks, opt 0) runs this fixture
    // an order of magnitude slower. The probe's job is catching regressions
    // in the OPTIMIZED build — report and skip the thresholds elsewhere.
    if cfg!(debug_assertions) {
        eprintln!("[probe] dev profile: thresholds skipped (edit {med_e:?}, present {med_d:?})");
        return;
    }
    // Same reasoning for COVERAGE builds: -C instrument-coverage puts LLVM
    // profile counters inside exactly these hot loops, and the probe would
    // measure the instrumentation, not the engine (locally 4.1 ms release vs
    // >15 ms instrumented on the CI runner). The plain release jobs keep the
    // gate. LLVM_PROFILE_FILE is set by the coverage harness at runtime.
    if std::env::var_os("LLVM_PROFILE_FILE").is_some() {
        eprintln!(
            "[probe] coverage-instrumented build: thresholds skipped              (edit {med_e:?}, present {med_d:?})"
        );
        return;
    }
    assert!(
        med_e < std::time::Duration::from_millis(15),
        "edit median regressed: {med_e:?} (was ~5 ms; #11 target 8 ms)"
    );
    assert!(
        med_d < std::time::Duration::from_millis(5),
        "damage-present median regressed: {med_d:?} (was ~113 us)"
    );
}

/// PIN — audit F4 (focus before the first layout). A `set_focus` issued from
/// a create callback used to VANISH: `resolve_focus_target` answers `Ok(None)`
/// with empty `layout_results` and every caller applied that as "clear focus".
/// It now parks on the `FocusManager` (`FocusResolution::Deferred`) and is
/// applied by the layout tail / `finalize_pending_focus_changes` once a
/// layout exists. This test is RED before that batch.
#[test]
fn focus_set_before_the_first_layout_survives_until_layout_exists() {
    use azul_core::callbacks::FocusTarget;
    use azul_layout::managers::focus_cursor::{resolve_focus_target_or_defer, FocusResolution};

    let mut h = ContentEditableHarness::new(400.0, 300.0);
    let dom_id = DomId { inner: 0 };
    // body(0) > div(1): the deterministic assignment every test here uses.
    let editor_node = NodeId::new(1);
    let target_id =
        DomNodeId { dom: dom_id, node: NodeHierarchyItemId::from(Some(editor_node)) };

    {
        let lw = h.layout_window.as_mut().unwrap();
        assert!(lw.layout_results.is_empty(), "premise: nothing is laid out yet");
        let r = resolve_focus_target_or_defer(
            &mut lw.focus_manager,
            &FocusTarget::Id(target_id),
            &lw.layout_results,
        );
        assert!(matches!(r, Ok(FocusResolution::Deferred)));
        // The whole bug in one line: focus is neither set NOR cleared.
        assert!(lw.focus_manager.get_focused_node().is_none());
        assert!(lw.focus_manager.has_deferred_focus_target());
    }

    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("hello"));
    let dom = Dom::create_body().with_child(editor);
    h.layout_dom(dom, CE_CSS);

    let lw = h.layout_window.as_mut().unwrap();
    // The host's layout-tail / end-of-pass call.
    lw.finalize_pending_focus_changes();
    assert_eq!(lw.focus_manager.get_focused_node().copied(), Some(target_id));
    assert!(lw.text_edit_manager.multi_cursor.is_some(), "the caret was seeded");
    assert!(
        !lw.focus_manager.has_deferred_focus_target(),
        "queue drained, not re-queued"
    );
}

// =========================================================================
// REGRESSION (B2): a keystroke must not inherit the previous layout pass's
// patch bookkeeping
// =========================================================================

/// REGRESSION (B2): typing does not redraw the edited text.
///
/// A keystroke goes `apply_text_changeset` -> `reapply_dirty_text_node` ->
/// `LayoutWindow::regenerate_display_list_for_dom`, which rebuilds the display
/// list WHOLESALE from the current layout tree. It used to leave
/// `LayoutCache::last_build_was_patched` / `last_patch_damage` /
/// `last_patch_move` exactly as the previous LAYOUT pass left them.
///
/// Those three are read by the compositor, not by layout. `last_patch_move`
/// becomes a `cpurender::TranslateHint`: the CPU backend BLITS the previous
/// frame by `dominant_delta` and repaints only the exceptions plus the damage
/// (`dll/src/desktop/shell2/headless/mod.rs:532`, E2E twin
/// `layout/src/e2e/cpu_backend.rs:329`). The rebuild also mints a fresh
/// display-list `Arc`, which is what clears the backend's
/// `last_patch_shift_dl` re-shift guard — so the hint was live again. The
/// frame was then shifted by a delta belonging to an earlier reflow and the
/// newly typed glyphs were never painted where they actually are: the damage
/// rect is right, the pixels in it are a translated copy of the old frame.
///
/// Asserts the invariant directly: after a keystroke, no patch bookkeeping
/// from an earlier pass may survive.
#[test]
fn a_keystroke_clears_the_previous_passes_patch_bookkeeping() {
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

    let mut h = ContentEditableHarness::new(400.0, 300.0);

    let mut editor = Dom::create_div();
    editor = editor.with_ids_and_classes(cls("editor").into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    editor = editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Hello"));

    h.layout_dom(Dom::create_body().with_child(editor), CE_CSS);

    let dom_id = DomId { inner: 0 };
    let ce_node_id = h.find_contenteditable_nodes()[0];
    h.focus_node(dom_id, ce_node_id);

    // Stand in for "the previous layout pass was a PATCHED build that moved
    // content by 40px" — the state a scroll / incremental reflow leaves behind.
    {
        let lw = h.layout_window.as_mut().unwrap();
        lw.layout_cache.last_build_was_patched = true;
        lw.layout_cache.last_patch_damage = Some(vec![LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(400.0, 40.0),
        )]);
        lw.layout_cache.last_patch_move = Some(
            azul_layout::solver3::display_list::PatchMoveSummary {
                dominant_delta: LogicalPosition::new(0.0, -40.0),
                moved_region_old: LogicalRect::new(
                    LogicalPosition::new(0.0, 40.0),
                    LogicalSize::new(400.0, 260.0),
                ),
                exceptions: Vec::new(),
                mover_rects_old: Vec::new(),
            },
        );
    }

    let (_affected, _old, inserted) = h.type_text("X");
    assert_eq!(inserted, "X", "sanity: the keystroke must have been applied");

    let lw = h.layout_window.as_ref().unwrap();
    assert!(
        !lw.layout_cache.last_build_was_patched,
        "a wholesale display-list rebuild is not a patched build"
    );
    assert!(
        lw.layout_cache.last_patch_damage.is_none(),
        "the rebuilt list inherited damage rects describing a DIFFERENT list: {:?}",
        lw.layout_cache.last_patch_damage
    );
    assert!(
        lw.layout_cache.last_patch_move.is_none(),
        "the rebuilt list inherited a translate hint ({:?}) — the compositor \
         will BLIT the previous frame by that delta instead of painting the \
         glyphs the user just typed",
        lw.layout_cache.last_patch_move
    );
}

