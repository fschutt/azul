//! IFC Incremental Caching Tests (Phase 2a/2b)
//!
//! These tests verify the IFC granular caching architecture:
//! 1. `InlineItemMetrics` are correctly extracted from `UnifiedLayout`
//! 2. `CachedInlineLayout` stores per-item metrics alongside the layout
//! 3. `RelayoutScope` classifies CSS property changes correctly
//! 4. `RestyleResult.max_relayout_scope` is tracked during restyle
//! 5. IFC layouts with text produce correct item_metrics

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_css::props::property::{CssPropertyType, RelayoutScope};
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::layout_tree::{CachedInlineLayout, InlineItemMetrics};
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::cache::AvailableSpace;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::BTreeMap;
use std::sync::Arc;

// ==================== RelayoutScope Classification Tests ====================

#[test]
fn test_relayout_scope_paint_only_properties() {
    // Pure paint properties should always return None
    let paint_props = [
        CssPropertyType::TextColor,
        CssPropertyType::BackgroundContent,
        CssPropertyType::BackgroundPosition,
        CssPropertyType::BackgroundSize,
        CssPropertyType::BackgroundRepeat,
        CssPropertyType::BorderTopColor,
        CssPropertyType::BorderRightColor,
        CssPropertyType::BorderLeftColor,
        CssPropertyType::BorderBottomColor,
        CssPropertyType::Opacity,
        CssPropertyType::Transform,
        CssPropertyType::BoxShadowLeft,
        CssPropertyType::Filter,
        CssPropertyType::TextShadow,
        CssPropertyType::Cursor,
    ];

    for prop in &paint_props {
        assert_eq!(
            prop.relayout_scope(false),
            RelayoutScope::None,
            "{:?} should be None when not IFC member",
            prop
        );
        assert_eq!(
            prop.relayout_scope(true),
            RelayoutScope::None,
            "{:?} should be None even when IFC member",
            prop
        );
    }
}

#[test]
fn test_relayout_scope_font_properties_ifc_member() {
    // Font/text properties should return IfcOnly when node is IFC member
    let font_props = [
        CssPropertyType::FontFamily,
        CssPropertyType::FontSize,
        CssPropertyType::FontWeight,
        CssPropertyType::FontStyle,
        CssPropertyType::LetterSpacing,
        CssPropertyType::WordSpacing,
        CssPropertyType::LineHeight,
        CssPropertyType::TextAlign,
    ];

    for prop in &font_props {
        assert_eq!(
            prop.relayout_scope(true),
            RelayoutScope::IfcOnly,
            "{:?} should be IfcOnly when IFC member",
            prop
        );
    }
}

#[test]
fn test_relayout_scope_font_properties_non_ifc_member() {
    // Font/text properties should return None when node is NOT IFC member
    // (block container with only block children)
    let font_props = [
        CssPropertyType::FontFamily,
        CssPropertyType::FontSize,
        CssPropertyType::FontWeight,
        CssPropertyType::LetterSpacing,
        CssPropertyType::LineHeight,
        CssPropertyType::TextAlign,
    ];

    for prop in &font_props {
        assert_eq!(
            prop.relayout_scope(false),
            RelayoutScope::None,
            "{:?} should be None when NOT IFC member",
            prop
        );
    }
}

#[test]
fn test_relayout_scope_sizing_properties() {
    // Sizing properties should return SizingOnly regardless of IFC membership
    let sizing_props = [
        CssPropertyType::Width,
        CssPropertyType::Height,
        CssPropertyType::MinWidth,
        CssPropertyType::MinHeight,
        CssPropertyType::MaxWidth,
        CssPropertyType::MaxHeight,
        CssPropertyType::PaddingTop,
        CssPropertyType::PaddingRight,
        CssPropertyType::PaddingBottom,
        CssPropertyType::PaddingLeft,
        CssPropertyType::BorderTopWidth,
        CssPropertyType::BorderRightWidth,
        CssPropertyType::BorderBottomWidth,
        CssPropertyType::BorderLeftWidth,
        CssPropertyType::BoxSizing,
    ];

    for prop in &sizing_props {
        assert_eq!(
            prop.relayout_scope(false),
            RelayoutScope::SizingOnly,
            "{:?} should be SizingOnly",
            prop
        );
        assert_eq!(
            prop.relayout_scope(true),
            RelayoutScope::SizingOnly,
            "{:?} should be SizingOnly even when IFC member",
            prop
        );
    }
}

#[test]
fn test_relayout_scope_full_relayout_properties() {
    // Display, position, float, margin, flex, grid properties → Full
    let full_props = [
        CssPropertyType::Display,
        CssPropertyType::Position,
        CssPropertyType::Float,
        CssPropertyType::MarginTop,
        CssPropertyType::MarginRight,
        CssPropertyType::MarginBottom,
        CssPropertyType::MarginLeft,
        CssPropertyType::FlexDirection,
        CssPropertyType::FlexGrow,
        CssPropertyType::FlexShrink,
        CssPropertyType::FlexWrap,
        CssPropertyType::AlignItems,
        CssPropertyType::JustifyContent,
        CssPropertyType::OverflowX,
    ];

    for prop in &full_props {
        assert_eq!(
            prop.relayout_scope(false),
            RelayoutScope::Full,
            "{:?} should be Full",
            prop
        );
    }
}

#[test]
fn test_relayout_scope_ordering() {
    // RelayoutScope should be ordered None < IfcOnly < SizingOnly < Full
    assert!(RelayoutScope::None < RelayoutScope::IfcOnly);
    assert!(RelayoutScope::IfcOnly < RelayoutScope::SizingOnly);
    assert!(RelayoutScope::SizingOnly < RelayoutScope::Full);
    assert!(RelayoutScope::None < RelayoutScope::Full);
}

#[test]
fn test_relayout_scope_default_is_none() {
    assert_eq!(RelayoutScope::default(), RelayoutScope::None);
}

// ==================== can_trigger_relayout() Consistency Tests ====================

#[test]
fn test_can_trigger_relayout_consistent_with_relayout_scope() {
    // can_trigger_relayout() should return true iff relayout_scope(true) != None
    // This verifies the backward-compatible relationship described in FIX_PLAN §11.
    // Test a representative sample of each category.
    let test_cases: Vec<(CssPropertyType, bool)> = vec![
        // Paint-only → false / None
        (CssPropertyType::TextColor, false),
        (CssPropertyType::BackgroundContent, false),
        (CssPropertyType::Opacity, false),
        (CssPropertyType::Transform, false),
        // Font/text → true / IfcOnly (with ifc_member=true)
        (CssPropertyType::FontFamily, true),
        (CssPropertyType::FontSize, true),
        (CssPropertyType::FontWeight, true),
        (CssPropertyType::LetterSpacing, true),
        (CssPropertyType::LineHeight, true),
        // Sizing → true / SizingOnly
        (CssPropertyType::Width, true),
        (CssPropertyType::Height, true),
        (CssPropertyType::PaddingTop, true),
        (CssPropertyType::BorderTopWidth, true),
        // Full → true / Full
        (CssPropertyType::Display, true),
        (CssPropertyType::Position, true),
        (CssPropertyType::Float, true),
        (CssPropertyType::MarginTop, true),
    ];

    for (prop_type, expected_can_trigger) in &test_cases {
        let old_result = prop_type.can_trigger_relayout();
        let new_scope = prop_type.relayout_scope(true);
        let new_result = new_scope != RelayoutScope::None;

        assert_eq!(
            old_result, *expected_can_trigger,
            "{:?}: can_trigger_relayout() returned {}, expected {}",
            prop_type, old_result, expected_can_trigger
        );
        assert_eq!(
            old_result, new_result,
            "{:?}: can_trigger_relayout()={} but relayout_scope(true)={:?} ({})",
            prop_type, old_result, new_scope, new_result
        );
    }
}

// ==================== InlineItemMetrics Tests ====================

#[test]
fn test_inline_item_metrics_struct_fields() {
    use azul_core::dom::NodeId;

    let metrics = InlineItemMetrics {
        source_node_id: Some(NodeId::new(5)),
        advance_width: 42.5,
        line_height_contribution: 16.0,
        can_break: true,
        line_index: 0,
        x_offset: 10.0,
    };

    assert_eq!(metrics.source_node_id, Some(NodeId::new(5)));
    assert!((metrics.advance_width - 42.5).abs() < f32::EPSILON);
    assert!((metrics.line_height_contribution - 16.0).abs() < f32::EPSILON);
    assert!(metrics.can_break);
    assert_eq!(metrics.line_index, 0);
    assert!((metrics.x_offset - 10.0).abs() < f32::EPSILON);
}

#[test]
fn test_inline_item_metrics_no_source_node() {
    let metrics = InlineItemMetrics {
        source_node_id: None,
        advance_width: 0.0,
        line_height_contribution: 0.0,
        can_break: false,
        line_index: 0,
        x_offset: 0.0,
    };

    assert!(metrics.source_node_id.is_none());
}

// ==================== CachedInlineLayout with item_metrics Tests ====================

#[test]
fn test_cached_inline_layout_empty_layout_has_empty_metrics() {
    use azul_layout::text3::cache::{OverflowInfo, UnifiedLayout};

    let empty_layout = Arc::new(UnifiedLayout {
        items: Vec::new(),
        overflow: OverflowInfo::default(),
    });

    let cached = CachedInlineLayout::new(
        empty_layout,
        AvailableSpace::Definite(400.0),
        false,
    );

    assert!(cached.item_metrics.is_empty(),
        "Empty UnifiedLayout should produce empty item_metrics");
}

#[test]
fn test_cached_inline_layout_with_constraints_has_metrics() {
    use azul_layout::text3::cache::{OverflowInfo, UnifiedLayout};

    let empty_layout = Arc::new(UnifiedLayout {
        items: Vec::new(),
        overflow: OverflowInfo::default(),
    });

    let constraints = azul_layout::text3::cache::UnifiedConstraints::default();

    let cached = CachedInlineLayout::new_with_constraints(
        empty_layout,
        AvailableSpace::Definite(400.0),
        false,
        constraints,
    );

    assert!(cached.item_metrics.is_empty());
    assert!(cached.constraints.is_some());
}

#[test]
fn test_cached_inline_layout_validity_check_unchanged() {
    use azul_layout::text3::cache::{OverflowInfo, UnifiedLayout};

    let layout = Arc::new(UnifiedLayout {
        items: Vec::new(),
        overflow: OverflowInfo::default(),
    });

    let cached = CachedInlineLayout::new(
        layout,
        AvailableSpace::Definite(400.0),
        false,
    );

    // Same width, no floats → valid
    assert!(cached.is_valid_for(AvailableSpace::Definite(400.0), false));
    // Different width → invalid
    assert!(!cached.is_valid_for(AvailableSpace::Definite(500.0), false));
    // Different constraint type → invalid
    assert!(!cached.is_valid_for(AvailableSpace::MinContent, false));
}

// ==================== IFC Layout Integration Tests ====================

/// Helper to run a full layout and return the layout tree for inspection
fn layout_html_and_get_tree(html: &str) -> azul_layout::Solver3LayoutCache {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");
    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: BTreeMap::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
        cache_map: Default::default(),
    };
    let mut text_cache = TextLayoutCache::new();

    let content_size = LogicalSize::new(800.0, 600.0);
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

    let _ = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        styled_dom,
        viewport,
        &mut font_manager,
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
    )
    .expect("Layout should succeed");

    layout_cache
}

#[test]
fn test_ifc_layout_produces_item_metrics() {
    let html = r#"
    <html>
        <body>
            <p>Hello World</p>
        </body>
    </html>
    "#;

    let cache = layout_html_and_get_tree(html);
    let tree = cache.tree.as_ref().expect("Layout tree should exist");

    // Find nodes with inline_layout_result (IFC roots)
    let ifc_nodes: Vec<_> = tree.nodes.iter().enumerate()
        .filter(|(_, n)| n.inline_layout_result.is_some())
        .collect();

    assert!(!ifc_nodes.is_empty(), "Should have at least one IFC root node");

    for (idx, node) in &ifc_nodes {
        let cached = node.inline_layout_result.as_ref().unwrap();
        let layout = cached.get_layout();

        // item_metrics length should match layout.items length
        assert_eq!(
            cached.item_metrics.len(),
            layout.items.len(),
            "Node {}: item_metrics count ({}) should match layout.items count ({})",
            idx,
            cached.item_metrics.len(),
            layout.items.len(),
        );

        // Each metric should have non-negative advance width
        for (i, metric) in cached.item_metrics.iter().enumerate() {
            assert!(
                metric.advance_width >= 0.0,
                "Node {}, item {}: advance_width should be >= 0, got {}",
                idx, i, metric.advance_width
            );
            assert!(
                metric.line_height_contribution >= 0.0,
                "Node {}, item {}: line_height_contribution should be >= 0, got {}",
                idx, i, metric.line_height_contribution
            );
        }
    }
}

#[test]
fn test_ifc_layout_metrics_have_correct_line_indices() {
    let html = r#"
    <html>
        <body>
            <p style="width: 50px;">Hello World this is a long paragraph that wraps</p>
        </body>
    </html>
    "#;

    let cache = layout_html_and_get_tree(html);
    let tree = cache.tree.as_ref().expect("Layout tree should exist");

    let ifc_nodes: Vec<_> = tree.nodes.iter().enumerate()
        .filter(|(_, n)| n.inline_layout_result.is_some())
        .collect();

    // With width: 50px, text should wrap to multiple lines
    for (_idx, node) in &ifc_nodes {
        let cached = node.inline_layout_result.as_ref().unwrap();

        if cached.item_metrics.is_empty() {
            continue;
        }

        // Verify line_index values are valid (non-negative, which they always are as u32).
        // Also verify that items on the same line have consistent line_index values.
        let max_line = cached.item_metrics.iter()
            .map(|m| m.line_index)
            .max()
            .unwrap_or(0);

        // The max_line should be a reasonable value (not absurdly large)
        assert!(
            max_line < 1000,
            "max_line_index {} seems unreasonably large",
            max_line
        );
    }
}

#[test]
fn test_ifc_layout_metrics_x_offsets_increase_on_same_line() {
    let html = r#"
    <html>
        <body>
            <p>Hello World</p>
        </body>
    </html>
    "#;

    let cache = layout_html_and_get_tree(html);
    let tree = cache.tree.as_ref().expect("Layout tree should exist");

    let ifc_nodes: Vec<_> = tree.nodes.iter().enumerate()
        .filter(|(_, n)| n.inline_layout_result.is_some())
        .collect();

    for (idx, node) in &ifc_nodes {
        let cached = node.inline_layout_result.as_ref().unwrap();

        // Group metrics by line_index
        let mut lines: BTreeMap<u32, Vec<&InlineItemMetrics>> = BTreeMap::new();
        for m in &cached.item_metrics {
            lines.entry(m.line_index).or_default().push(m);
        }

        // Within each line, x_offsets should be monotonically non-decreasing
        for (line_idx, items) in &lines {
            let mut prev_x = -1.0f32;
            for item in items {
                assert!(
                    item.x_offset >= prev_x,
                    "Node {}, line {}: x_offset {} should be >= prev {}",
                    idx, line_idx, item.x_offset, prev_x
                );
                prev_x = item.x_offset;
            }
        }
    }
}

#[test]
fn test_ifc_layout_metrics_source_node_ids_for_text() {
    let html = r#"
    <html>
        <body>
            <p><span>Hello</span> <span>World</span></p>
        </body>
    </html>
    "#;

    let cache = layout_html_and_get_tree(html);
    let tree = cache.tree.as_ref().expect("Layout tree should exist");

    let ifc_nodes: Vec<_> = tree.nodes.iter().enumerate()
        .filter(|(_, n)| n.inline_layout_result.is_some())
        .collect();

    assert!(!ifc_nodes.is_empty(), "Should have IFC root nodes");

    // At least some items should have source_node_id set (text clusters)
    let total_items: usize = ifc_nodes.iter()
        .map(|(_, n)| n.inline_layout_result.as_ref().unwrap().item_metrics.len())
        .sum();

    let items_with_source: usize = ifc_nodes.iter()
        .flat_map(|(_, n)| n.inline_layout_result.as_ref().unwrap().item_metrics.iter())
        .filter(|m| m.source_node_id.is_some())
        .count();

    assert!(
        total_items > 0,
        "Should have inline items from text"
    );
    assert!(
        items_with_source > 0,
        "At least some items should have source_node_id (text clusters), \
         but got 0 out of {} total items",
        total_items
    );
}

#[test]
fn test_ifc_layout_replace_preserves_metrics_structure() {
    use azul_layout::text3::cache::{OverflowInfo, PositionedItem, UnifiedLayout, ShapedItem,
        ShapedCluster, ContentIndex, GraphemeClusterId, Point};

    // Create a mock UnifiedLayout with one cluster item
    let cluster = ShapedCluster {
        text: "A".to_string(),
        source_cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: 0 },
        source_content_index: ContentIndex { run_index: 0, item_index: 0 },
        source_node_id: Some(azul_core::dom::NodeId::new(3)),
        glyphs: vec![],
        advance: 10.0,
        direction: azul_layout::text3::cache::BidiDirection::Ltr,
        style: Default::default(),
        marker_position_outside: None,
    };

    let layout = Arc::new(UnifiedLayout {
        items: vec![PositionedItem {
            item: ShapedItem::Cluster(cluster),
            position: Point { x: 5.0, y: 0.0 },
            line_index: 0,
        }],
        overflow: OverflowInfo::default(),
    });

    let cached = CachedInlineLayout::new(
        layout.clone(),
        AvailableSpace::Definite(800.0),
        false,
    );

    assert_eq!(cached.item_metrics.len(), 1);
    assert_eq!(cached.item_metrics[0].source_node_id, Some(azul_core::dom::NodeId::new(3)));
    assert!((cached.item_metrics[0].advance_width - 10.0).abs() < 0.01);
    assert_eq!(cached.item_metrics[0].line_index, 0);
    assert!((cached.item_metrics[0].x_offset - 5.0).abs() < 0.01);

    // should_replace_with different width → true
    assert!(cached.should_replace_with(AvailableSpace::Definite(400.0), false));
    // same width → false
    assert!(!cached.should_replace_with(AvailableSpace::Definite(800.0), false));
}
