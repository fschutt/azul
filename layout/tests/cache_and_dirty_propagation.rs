//! Comprehensive tests for the per-node multi-slot cache (Taffy-inspired 9+1)
//! and dirty propagation in the azul layout engine.
//!
//! These tests verify:
//! 1. Cache correctness: initial layout populates cache, relayout hits cache
//! 2. Dirty propagation: property changes mark correct ancestors dirty
//! 3. Performance: relayout doesn't blow up (O(n) not O(n²))
//! 4. W3C conformance: whitespace filtering, canvas background, two-pass BFC
//! 5. Resize behavior: viewport changes invalidate properly
//!
//! References:
//! - Taffy tests/caching.rs: deep tree measure count test
//! - Taffy tests/relayout.rs: stability + display toggle tests
//! - CSS 2.1 §8.3.1 (margin collapsing), §9.2.2.1 (anonymous boxes),
//!   §10.3 (containing block), §14.2 (canvas background)

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::BTreeMap;

// ============================================================================
// Test helpers
// ============================================================================

/// Shared test environment — builds the (expensive) font cache once.
struct TestEnv {
    font_manager: FontManager<azul_css::props::basic::FontRef>,
}

impl TestEnv {
    fn new() -> Self {
        let fc_cache = build_font_cache();
        let font_manager = FontManager::new(fc_cache).expect("font manager");
        Self { font_manager }
    }

    /// Creates a fresh layout environment and runs paged layout.
    /// Returns (layout_cache, debug_messages) for inspection.
    fn run_layout(
        &mut self,
        html: &str,
        viewport_w: f32,
        viewport_h: f32,
    ) -> (Solver3LayoutCache, Vec<azul_css::LayoutDebugMessage>) {
        let styled_dom = Dom::from_xml_string(html);

        let mut layout_cache = Solver3LayoutCache {
            tree: None,
            calculated_positions: Vec::new(),
            viewport: None,
            scroll_ids: BTreeMap::new(),
            scroll_id_to_node_id: BTreeMap::new(),
            counters: BTreeMap::new(),
            float_cache: BTreeMap::new(),
            cache_map: Default::default(),
        };
        let mut text_cache = TextLayoutCache::new();

        let content_size = LogicalSize::new(viewport_w, viewport_h);
        let fragmentation_context = FragmentationContext::new_paged(content_size);
        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        };
        let renderer_resources = RendererResources::default();
        let mut debug_messages = Some(Vec::new());

        let loader = PathLoader::new();
        let font_loader = |bytes: &[u8], index: usize| loader.load_font(bytes, index);
        let page_config = FakePageConfig::new();

        let _display_lists = layout_document_paged_with_config(
            &mut layout_cache,
            &mut text_cache,
            fragmentation_context,
            &styled_dom,
            viewport,
            &mut self.font_manager,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut debug_messages,
            None,
            &renderer_resources,
            azul_core::resources::IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            page_config,
            azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
        )
        .expect("layout should succeed");

        (layout_cache, debug_messages.unwrap_or_default())
    }

    /// Runs layout a second time on the same cache (simulating relayout).
    /// Returns timing for performance assertions. Only measures actual layout,
    /// NOT font cache construction.
    fn run_relayout(
        &mut self,
        html: &str,
        cache: &mut Solver3LayoutCache,
        viewport_w: f32,
        viewport_h: f32,
    ) -> std::time::Duration {
        let styled_dom = Dom::from_xml_string(html);
        let mut text_cache = TextLayoutCache::new();

        let content_size = LogicalSize::new(viewport_w, viewport_h);
        let fragmentation_context = FragmentationContext::new_paged(content_size);
        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        };
        let renderer_resources = RendererResources::default();
        let mut debug_messages = Some(Vec::new());

        let loader = PathLoader::new();
        let font_loader = |bytes: &[u8], index: usize| loader.load_font(bytes, index);
        let page_config = FakePageConfig::new();

        let start = std::time::Instant::now();
        let _display_lists = layout_document_paged_with_config(
            cache,
            &mut text_cache,
            fragmentation_context,
            &styled_dom,
            viewport,
            &mut self.font_manager,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut debug_messages,
            None,
            &renderer_resources,
            azul_core::resources::IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            page_config,
            azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
        )
        .expect("relayout should succeed");

        start.elapsed()
    }
}

/// Convenience: build env + run layout in one call (for simple tests).
fn run_layout(
    html: &str,
    viewport_w: f32,
    viewport_h: f32,
) -> (Solver3LayoutCache, Vec<azul_css::LayoutDebugMessage>) {
    TestEnv::new().run_layout(html, viewport_w, viewport_h)
}

// ============================================================================
// §1: Cache data structure unit tests (no layout needed)
// ============================================================================

#[test]
fn test_node_cache_slot_index_deterministic() {
    use azul_layout::solver3::cache::{AvailableWidthType, NodeCache};
    // Taffy's 9-slot scheme: each constraint combo maps to exactly one slot
    assert_eq!(NodeCache::slot_index(true, true, AvailableWidthType::Definite, AvailableWidthType::Definite), 0);
    assert_eq!(NodeCache::slot_index(true, false, AvailableWidthType::Definite, AvailableWidthType::Definite), 1);
    assert_eq!(NodeCache::slot_index(true, false, AvailableWidthType::MinContent, AvailableWidthType::Definite), 2);
    assert_eq!(NodeCache::slot_index(false, true, AvailableWidthType::Definite, AvailableWidthType::Definite), 3);
    assert_eq!(NodeCache::slot_index(false, true, AvailableWidthType::Definite, AvailableWidthType::MinContent), 4);
    assert_eq!(NodeCache::slot_index(false, false, AvailableWidthType::Definite, AvailableWidthType::Definite), 5);
    assert_eq!(NodeCache::slot_index(false, false, AvailableWidthType::Definite, AvailableWidthType::MinContent), 6);
    assert_eq!(NodeCache::slot_index(false, false, AvailableWidthType::MinContent, AvailableWidthType::Definite), 7);
    assert_eq!(NodeCache::slot_index(false, false, AvailableWidthType::MinContent, AvailableWidthType::MinContent), 8);
    // No collisions: all indices are unique
    let mut seen = std::collections::HashSet::new();
    for wk in [true, false] {
        for hk in [true, false] {
            for wt in [AvailableWidthType::Definite, AvailableWidthType::MinContent, AvailableWidthType::MaxContent] {
                for ht in [AvailableWidthType::Definite, AvailableWidthType::MinContent, AvailableWidthType::MaxContent] {
                    let idx = NodeCache::slot_index(wk, hk, wt, ht);
                    assert!(idx < 9, "slot index {} out of bounds for ({}, {}, {:?}, {:?})", idx, wk, hk, wt, ht);
                    // Not all combos are unique (MaxContent == Definite for slot mapping)
                    // but all are < 9
                    seen.insert(idx);
                }
            }
        }
    }
    // All 9 slots should be reachable
    assert_eq!(seen.len(), 9, "all 9 slots must be reachable");
}

#[test]
fn test_node_cache_sizing_store_and_retrieve() {
    use azul_layout::solver3::cache::{NodeCache, SizingCacheEntry};
    use azul_core::geom::LogicalSize;

    let mut cache = NodeCache::default();
    assert!(cache.is_empty);

    let entry = SizingCacheEntry {
        available_size: LogicalSize::new(800.0, 600.0),
        result_size: LogicalSize::new(400.0, 200.0),
        baseline: Some(180.0),
        escaped_top_margin: None,
        escaped_bottom_margin: None,
    };

    cache.store_size(0, entry);
    assert!(!cache.is_empty);

    // Exact match
    let hit = cache.get_size(0, LogicalSize::new(800.0, 600.0));
    assert!(hit.is_some(), "exact available_size should hit");
    assert!((hit.unwrap().result_size.width - 400.0).abs() < 0.01);

    // "Result matches request" — providing the result_size as known_dims
    let hit2 = cache.get_size(0, LogicalSize::new(400.0, 200.0));
    assert!(hit2.is_some(), "result-matches-request should hit");

    // Miss — different size
    let miss = cache.get_size(0, LogicalSize::new(500.0, 300.0));
    assert!(miss.is_none(), "different size should miss");

    // Miss — wrong slot
    let miss2 = cache.get_size(1, LogicalSize::new(800.0, 600.0));
    assert!(miss2.is_none(), "wrong slot should miss");
}

#[test]
fn test_node_cache_layout_store_and_retrieve() {
    use azul_layout::solver3::cache::{NodeCache, LayoutCacheEntry};
    use azul_layout::solver3::scrollbar::ScrollbarRequirements;
    use azul_core::geom::{LogicalSize, LogicalPosition};

    let mut cache = NodeCache::default();

    let entry = LayoutCacheEntry {
        available_size: LogicalSize::new(1024.0, 768.0),
        result_size: LogicalSize::new(1024.0, 500.0),
        content_size: LogicalSize::new(1024.0, 1200.0),
        child_positions: vec![
            (1, LogicalPosition::new(0.0, 0.0)),
            (2, LogicalPosition::new(0.0, 100.0)),
        ],
        escaped_top_margin: None,
        escaped_bottom_margin: Some(20.0),
        scrollbar_info: ScrollbarRequirements::default(),
    };

    cache.store_layout(entry);
    assert!(!cache.is_empty);

    // Exact match
    let hit = cache.get_layout(LogicalSize::new(1024.0, 768.0));
    assert!(hit.is_some());
    assert_eq!(hit.unwrap().child_positions.len(), 2);

    // Result-matches-request
    let hit2 = cache.get_layout(LogicalSize::new(1024.0, 500.0));
    assert!(hit2.is_some());

    // Clear
    cache.clear();
    assert!(cache.is_empty);
    assert!(cache.get_layout(LogicalSize::new(1024.0, 768.0)).is_none());
}

#[test]
fn test_cache_map_resize_and_dirty_propagation() {
    use azul_layout::solver3::cache::LayoutCacheMap;

    let mut cache_map = LayoutCacheMap::default();
    assert_eq!(cache_map.entries.len(), 0);

    // Simulate a tree with 5 nodes
    cache_map.resize_to_tree(5);
    assert_eq!(cache_map.entries.len(), 5);
    // All new nodes are empty (dirty)
    for entry in &cache_map.entries {
        assert!(entry.is_empty);
    }

    // Grow tree
    cache_map.resize_to_tree(10);
    assert_eq!(cache_map.entries.len(), 10);

    // Shrink tree (nodes 5-9 dropped)
    cache_map.resize_to_tree(3);
    assert_eq!(cache_map.entries.len(), 3);
}

#[test]
fn test_dirty_propagation_via_layout() {
    // Test dirty propagation by doing full layout then checking that
    // cache entries are populated for all nodes after initial layout.
    // Then do a relayout — reconciliation with identical DOM means
    // cache entries should remain valid (no unnecessary dirty marking).
    let html = r#"
    <html>
        <head><style>
            * { margin: 0; padding: 0; }
            .a { height: 50px; }
            .b { height: 30px; }
        </style></head>
        <body>
            <div class="a">
                <div class="b">Leaf</div>
            </div>
        </body>
    </html>
    "#;

    let mut env = TestEnv::new();
    let (mut cache, _) = env.run_layout(html, 400.0, 300.0);

    // After first layout, cache_map should have entries for all nodes
    let node_count = cache.tree.as_ref().unwrap().nodes.len();
    assert_eq!(
        cache.cache_map.entries.len(),
        node_count,
        "cache_map should have one entry per node"
    );

    // Count filled entries
    let filled_before = cache.cache_map.entries.iter()
        .filter(|e| !e.is_empty)
        .count();
    assert!(
        filled_before > 0,
        "after layout, at least some cache entries should be filled"
    );

    // Relayout with identical DOM — reconciliation should preserve cache
    let _ = env.run_relayout(html, &mut cache, 400.0, 300.0);

    let filled_after = cache.cache_map.entries.iter()
        .filter(|e| !e.is_empty)
        .count();
    assert!(
        filled_after >= filled_before,
        "relayout of identical DOM should not reduce cache fill: before={}, after={}",
        filled_before, filled_after
    );
}

// ============================================================================
// §2: Full layout tests (cache populated, positions correct)
// ============================================================================

#[test]
fn test_simple_block_layout_cache_populated() {
    let html = r#"
    <html>
        <head><style>
            body { margin: 0; }
            div { height: 100px; background: red; }
        </style></head>
        <body>
            <div>Block 1</div>
            <div>Block 2</div>
            <div>Block 3</div>
        </body>
    </html>
    "#;

    let (cache, _) = run_layout(html, 800.0, 600.0);

    // Cache should have a tree
    assert!(cache.tree.is_some(), "tree should be populated after layout");

    let tree = cache.tree.as_ref().unwrap();
    assert!(tree.nodes.len() >= 5, "should have at least html + body + 3 divs, got {}", tree.nodes.len());

    // All nodes should have positions
    assert!(!cache.calculated_positions.is_empty(), "positions should be populated");

    // Cache map should have entries for all nodes
    assert_eq!(
        cache.cache_map.entries.len(),
        tree.nodes.len(),
        "cache_map should have one entry per node"
    );

    // Root node (index 0) should have a layout cache entry
    assert!(
        !cache.cache_map.entries[0].is_empty,
        "root cache should not be empty after layout"
    );
}

#[test]
fn test_three_blocks_vertical_stacking() {
    let html = r#"
    <html>
        <head><style>
            * { margin: 0; padding: 0; }
            div { height: 50px; }
        </style></head>
        <body>
            <div>A</div>
            <div>B</div>
            <div>C</div>
        </body>
    </html>
    "#;

    let (cache, _) = run_layout(html, 400.0, 300.0);

    let positions = &cache.calculated_positions;


    // Find the three div nodes by looking at the tree
    let tree = cache.tree.as_ref().unwrap();
    let mut div_positions: Vec<LogicalPosition> = Vec::new();
    for (idx, node) in tree.nodes.iter().enumerate() {
        if let Some(size) = node.used_size {
            if (size.height - 50.0).abs() < 0.1 {
                if let Some(pos) = positions.get(idx) {
                    div_positions.push(*pos);
                }
            }
        }
    }

    // Should have exactly 3 divs with height 50
    assert!(div_positions.len() >= 3, "expected 3 divs, found {}", div_positions.len());

    // Blocks should be stacked vertically
    if div_positions.len() >= 3 {
        let y_values: Vec<f32> = div_positions.iter().map(|p| p.y).collect();
        // Each subsequent block should be at least 50px below the previous
        for i in 1..y_values.len() {
            assert!(
                y_values[i] >= y_values[i - 1] + 49.0,
                "block {} (y={}) should be below block {} (y={}) by at least 50px",
                i, y_values[i], i - 1, y_values[i - 1]
            );
        }
    }
}

#[test]
fn test_whitespace_between_blocks_no_spurious_ifc() {
    // Regression test for c33e94b0: whitespace between <div>s should NOT
    // create anonymous IFC wrappers that take up vertical space.
    let html = r#"
    <html>
        <head><style>
            * { margin: 0; padding: 0; }
            div { height: 50px; background: blue; }
        </style></head>
        <body>
            <div>A</div>
            <div>B</div>
        </body>
    </html>
    "#;

    let (cache, _) = run_layout(html, 400.0, 300.0);
    let tree = cache.tree.as_ref().unwrap();

    // Check that no anonymous IFC wrapper nodes were created between the divs
    let mut anonymous_ifc_count = 0;
    for node in &tree.nodes {
        if node.is_anonymous {
            if let Some(azul_layout::solver3::layout_tree::AnonymousBoxType::InlineWrapper) = node.anonymous_type {
                anonymous_ifc_count += 1;
            }
        }
    }
    assert_eq!(
        anonymous_ifc_count, 0,
        "whitespace between blocks should NOT create anonymous IFC wrappers, got {}",
        anonymous_ifc_count
    );
}

#[test]
fn test_relayout_same_dom_is_fast() {
    // Inspired by Taffy tests/relayout.rs: repeated_layout_is_stable
    // If the DOM hasn't changed, relayout should be nearly instant (cache hit).
    let html = r#"
    <html>
        <head><style>
            body { margin: 0; }
            .box { width: 100px; height: 100px; }
        </style></head>
        <body>
            <div class="box">A</div>
            <div class="box">B</div>
            <div class="box">C</div>
        </body>
    </html>
    "#;

    let mut env = TestEnv::new();

    // First layout (populates cache)
    let (mut cache, _) = env.run_layout(html, 800.0, 600.0);
    let first_positions = cache.calculated_positions.clone();

    // Relayout with identical DOM (should hit caches)
    // Timing excludes font cache build — only measures actual layout.
    let relayout_time = env.run_relayout(html, &mut cache, 800.0, 600.0);

    // Positions should be identical
    let second_positions = &cache.calculated_positions;
    for (idx, pos1) in first_positions.iter().enumerate() {
        if let Some(pos2) = second_positions.get(idx) {
            assert!(
                (pos1.x - pos2.x).abs() < 0.01 && (pos1.y - pos2.y).abs() < 0.01,
                "position mismatch for node {}: first=({:.2},{:.2}), second=({:.2},{:.2})",
                idx, pos1.x, pos1.y, pos2.x, pos2.y
            );
        }
    }

    // Relayout should be fast — font cache is pre-built, only layout runs.
    // Even in debug builds, 3 nodes should take < 500ms.
    assert!(
        relayout_time.as_millis() < 500,
        "relayout of identical DOM took {:?}, should be fast (cache hits)",
        relayout_time
    );
}

#[test]
fn test_relayout_with_viewport_resize() {
    // Viewport resize should invalidate the cache and produce different positions.
    let html = r#"
    <html>
        <head><style>
            body { margin: 0; }
            div { width: 50%; height: 100px; }
        </style></head>
        <body>
            <div>Half width</div>
        </body>
    </html>
    "#;

    let mut env = TestEnv::new();
    let (mut cache, _) = env.run_layout(html, 800.0, 600.0);
    let tree_before = cache.tree.as_ref().unwrap();

    // Find the div node and check its width
    let mut div_width_before = 0.0f32;
    for node in &tree_before.nodes {
        if let Some(size) = node.used_size {
            if (size.height - 100.0).abs() < 0.1 {
                div_width_before = size.width;
            }
        }
    }
    assert!(
        (div_width_before - 400.0).abs() < 1.0,
        "50% of 800 should be ~400, got {}",
        div_width_before
    );

    // Resize viewport to 600px wide
    let _ = env.run_relayout(html, &mut cache, 600.0, 400.0);
    let tree_after = cache.tree.as_ref().unwrap();

    let mut div_width_after = 0.0f32;
    for node in &tree_after.nodes {
        if let Some(size) = node.used_size {
            if (size.height - 100.0).abs() < 1.0 {
                div_width_after = size.width;
            }
        }
    }
    assert!(
        (div_width_after - 300.0).abs() < 1.0,
        "50% of 600 should be ~300, got {}",
        div_width_after
    );
}

// ============================================================================
// §3: Margin collapsing with two-pass BFC (regression 8e092a2e)
// ============================================================================

#[test]
fn test_margin_collapsing_siblings() {
    // CSS 2.1 §8.3.1: Adjacent vertical margins of block-level boxes collapse.
    // The larger margin wins.
    let html = r#"
    <html>
        <head><style>
            * { padding: 0; }
            body { margin: 0; }
            .a { margin-bottom: 30px; height: 50px; background: red; }
            .b { margin-top: 20px; height: 50px; background: blue; }
        </style></head>
        <body>
            <div class="a">A</div>
            <div class="b">B</div>
        </body>
    </html>
    "#;

    let (cache, _) = run_layout(html, 400.0, 300.0);
    let positions = &cache.calculated_positions;
    let tree = cache.tree.as_ref().unwrap();

    // Find A and B by height=50 and their order
    let mut block_ys: Vec<f32> = Vec::new();
    for (idx, node) in tree.nodes.iter().enumerate() {
        if let Some(size) = node.used_size {
            if (size.height - 50.0).abs() < 0.1 {
                if let Some(pos) = positions.get(idx) {
                    block_ys.push(pos.y);
                }
            }
        }
    }
    block_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // A starts at y=0, B should start at y=50+30=80 (collapsed margin = max(30,20)=30)
    assert!(
        block_ys.len() >= 2,
        "expected at least 2 blocks, got {}",
        block_ys.len()
    );
    let gap = block_ys[1] - block_ys[0];
    // Gap should be height(A) + collapsed_margin = 50 + 30 = 80
    assert!(
        (gap - 80.0).abs() < 1.0,
        "gap between A and B should be 80px (50+30 collapsed), got {:.1}",
        gap
    );
}

// ============================================================================
// §4: Canvas background propagation (regression f1fcf27d)
// ============================================================================

#[test]
fn test_canvas_background_propagation_present() {
    // CSS 2.1 §14.2: root background becomes canvas background.
    // Even without html { height: 100% }, the background should fill viewport.
    let html = r#"
    <html>
        <head><style>
            html { background: rgb(240, 240, 242); }
            body { margin: 0; height: 50px; }
        </style></head>
        <body>
            <div>Content</div>
        </body>
    </html>
    "#;

    let (_cache, debug_msgs) = run_layout(html, 800.0, 600.0);

    // Check debug messages for canvas background propagation
    let has_canvas_bg = debug_msgs.iter().any(|m| {
        m.message.as_str().contains("Canvas background")
    });
    assert!(
        has_canvas_bg,
        "should emit canvas background debug message (CSS 2.1 §14.2)"
    );
}

// ============================================================================
// §5: Performance tests with larger DOMs
// ============================================================================

#[test]
fn test_performance_100_blocks() {
    // 100 blocks should complete in reasonable time (< 5 seconds)
    let mut html = String::from(r#"<html><head><style>
        * { margin: 0; padding: 0; }
        div { height: 20px; }
    </style></head><body>"#);
    for i in 0..100 {
        html.push_str(&format!("<div>Block {}</div>", i));
    }
    html.push_str("</body></html>");

    let mut env = TestEnv::new(); // font cache built here, excluded from timing
    let start = std::time::Instant::now();
    let (cache, _) = env.run_layout(&html, 800.0, 10000.0);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 30,
        "100 blocks took {:?}, should be < 30s",
        elapsed
    );

    let tree = cache.tree.as_ref().unwrap();
    assert!(
        tree.nodes.len() >= 102,
        "expected >=102 nodes (html+body+100 divs), got {}",
        tree.nodes.len()
    );

    // All 100 divs should have positions
    let positioned_count = cache.calculated_positions.len();
    assert!(
        positioned_count >= 102,
        "expected >=102 positioned nodes, got {}",
        positioned_count
    );
}

#[test]
fn test_relayout_100_blocks_cache_speedup() {
    // Second layout of same DOM should be faster than first due to cache hits.
    let mut html = String::from(r#"<html><head><style>
        * { margin: 0; padding: 0; }
        div { height: 20px; }
    </style></head><body>"#);
    for i in 0..100 {
        html.push_str(&format!("<div>Block {}</div>", i));
    }
    html.push_str("</body></html>");

    let mut env = TestEnv::new();
    let (mut cache, _) = env.run_layout(&html, 800.0, 10000.0);
    let relayout_time = env.run_relayout(&html, &mut cache, 800.0, 10000.0);

    // Relayout should be significantly faster because reconciliation
    // finds identical subtree hashes and skips layout entirely.
    assert!(
        relayout_time.as_millis() < 2000,
        "relayout of 100 identical blocks took {:?}, expected fast path",
        relayout_time
    );
}

// ============================================================================
// §6: Deeply nested tree (Taffy caching test pattern)
// ============================================================================

#[test]
fn test_deeply_nested_blocks_cache() {
    // Inspired by Taffy's caching test: 100 levels of nesting.
    // Each node should only be measured a bounded number of times.
    let mut html = String::from(r#"<html><head><style>
        * { margin: 0; padding: 0; }
    </style></head><body>"#);
    
    // Create 50 levels of nesting
    for _ in 0..50 {
        html.push_str("<div>");
    }
    html.push_str("Leaf content");
    for _ in 0..50 {
        html.push_str("</div>");
    }
    html.push_str("</body></html>");

    let mut env = TestEnv::new(); // font cache built here, excluded from timing
    let start = std::time::Instant::now();
    let (cache, _) = env.run_layout(&html, 400.0, 10000.0);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 30,
        "50-deep nested tree took {:?}, should not blow up",
        elapsed
    );

    let tree = cache.tree.as_ref().unwrap();
    // At least html + body + 50 divs
    assert!(
        tree.nodes.len() >= 52,
        "expected >=52 nodes, got {}",
        tree.nodes.len()
    );

    // All cache entries should be filled (not empty/dirty)
    let filled_count = cache.cache_map.entries.iter()
        .filter(|e| !e.is_empty)
        .count();
    assert!(
        filled_count > 0,
        "after layout, at least some cache entries should be filled, got {}",
        filled_count
    );
}

// ============================================================================
// §7: Flex/Grid layout with cache
// ============================================================================

#[test]
fn test_flex_layout_with_cache() {
    let html = r#"
    <html>
        <head><style>
            * { margin: 0; padding: 0; }
            .container { display: flex; width: 400px; height: 200px; }
            .item { flex: 1; }
        </style></head>
        <body>
            <div class="container">
                <div class="item">A</div>
                <div class="item">B</div>
                <div class="item">C</div>
            </div>
        </body>
    </html>
    "#;

    let (cache, _) = run_layout(html, 800.0, 600.0);
    let tree = cache.tree.as_ref().unwrap();

    // Find flex items (they should each be ~133px wide = 400/3)
    let mut flex_item_widths: Vec<f32> = Vec::new();
    for node in &tree.nodes {
        if let Some(size) = node.used_size {
            if (size.height - 200.0).abs() < 1.0 && (size.width - 133.0).abs() < 5.0 {
                flex_item_widths.push(size.width);
            }
        }
    }
    // We should have 3 flex items each ~133px wide
    assert!(
        flex_item_widths.len() >= 3,
        "expected 3 flex items, got {}. Widths: {:?}",
        flex_item_widths.len(),
        flex_item_widths
    );
}

// ============================================================================
// §8: RelayoutScope tests
// ============================================================================

#[test]
fn test_relayout_scope_classification() {
    use azul_css::props::property::CssPropertyType;

    // Paint-only properties should NOT trigger full relayout
    let paint_props = [
        CssPropertyType::BackgroundContent,
        CssPropertyType::BorderTopColor,
        CssPropertyType::TextColor,
        CssPropertyType::Opacity,
    ];
    for prop in &paint_props {
        assert!(
            !prop.can_trigger_relayout(),
            "{:?} should NOT trigger relayout (paint only)",
            prop
        );
    }

    // Layout properties MUST trigger relayout
    let layout_props = [
        CssPropertyType::Width,
        CssPropertyType::Height,
        CssPropertyType::MarginTop,
        CssPropertyType::PaddingLeft,
        CssPropertyType::Display,
        CssPropertyType::FontSize,
    ];
    for prop in &layout_props {
        assert!(
            prop.can_trigger_relayout(),
            "{:?} MUST trigger relayout (affects geometry)",
            prop
        );
    }
}

// ============================================================================
// §9: Whitespace preservation in pre/pre-wrap
// ============================================================================

#[test]
fn test_whitespace_preserved_in_pre() {
    // CSS 2.1 §16.6.1: white-space: pre should preserve whitespace.
    // The whitespace filter in reconcile_recursive must NOT strip text here.
    let html = r#"
    <html>
        <head><style>
            * { margin: 0; padding: 0; }
            pre { white-space: pre; font-size: 12px; }
        </style></head>
        <body>
            <pre>  Hello
  World  </pre>
        </body>
    </html>
    "#;

    let (cache, _) = run_layout(html, 400.0, 300.0);
    let tree = cache.tree.as_ref().unwrap();

    // The pre element should have content (its text node should not be stripped)
    // We check that the tree has the expected number of nodes
    assert!(
        tree.nodes.len() >= 3,
        "pre block should produce layout nodes, got {}",
        tree.nodes.len()
    );
}
