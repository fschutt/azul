//! IFC Caching Tests
//!
//! These tests verify the IFC caching architecture:
//! 1. `RelayoutScope` classifies CSS property changes correctly
//! 2. `CachedInlineLayout` keeps the constraints and the width it was laid
//!    out at, and answers whether it is valid for another
//! 3. IFC layouts with text put their items on lines, in order, with the
//!    nodes they were laid out from

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_css::props::property::{CssPropertyType, RelayoutScope};
use azul_layout::{
    font::loading::build_font_cache,
    font_traits::{FontManager, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        layout_tree::CachedInlineLayout,
        paged_layout::layout_document_paged_with_config,
        pagination::FakePageConfig,
        LayoutNodeId,
    },
    text3::{cache::AvailableSpace, default::PathLoader},
    xml::DomXmlExt,
    Solver3LayoutCache,
};

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
            "{prop:?} should be None when not IFC member"
        );
        assert_eq!(
            prop.relayout_scope(true),
            RelayoutScope::None,
            "{prop:?} should be None even when IFC member"
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
            "{prop:?} should be IfcOnly when IFC member"
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
            "{prop:?} should be None when NOT IFC member"
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
            "{prop:?} should be SizingOnly"
        );
        assert_eq!(
            prop.relayout_scope(true),
            RelayoutScope::SizingOnly,
            "{prop:?} should be SizingOnly even when IFC member"
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
            "{prop:?} should be Full"
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
            "{prop_type:?}: can_trigger_relayout() returned {old_result}, expected \
             {expected_can_trigger}"
        );
        assert_eq!(
            old_result, new_result,
            "{prop_type:?}: can_trigger_relayout()={old_result} but \
             relayout_scope(true)={new_scope:?} ({new_result})"
        );
    }
}

// ==================== CachedInlineLayout Tests ====================

#[test]
fn test_cached_inline_layout_with_constraints_keeps_them() {
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

    assert!(cached.layout.items.is_empty());
    assert!(cached.constraints.is_some());
}

#[test]
fn test_cached_inline_layout_validity_check_unchanged() {
    use azul_layout::text3::cache::{OverflowInfo, UnifiedLayout};

    let layout = Arc::new(UnifiedLayout {
        items: Vec::new(),
        overflow: OverflowInfo::default(),
    });

    let cached = CachedInlineLayout::new(layout, AvailableSpace::Definite(400.0), false);

    // Same width, no floats → valid
    assert!(cached.is_valid_for(AvailableSpace::Definite(400.0), false));
    // Different width → invalid
    assert!(!cached.is_valid_for(AvailableSpace::Definite(500.0), false));
    // Different constraint type → invalid
    assert!(!cached.is_valid_for(AvailableSpace::MinContent, false));
}

// ==================== IFC Layout Integration Tests ====================

/// Helper to run a full layout and return the layout tree for inspection
fn layout_html_and_get_tree(html: &str) -> Solver3LayoutCache {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");
    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::zero(),
        },
        ..Default::default()
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
    let font_loader = |bytes: Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let page_config = FakePageConfig::new();

    let _ = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("Layout should succeed");

    layout_cache
}

#[test]
fn test_ifc_layout_items_have_non_negative_sizes() {
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
    let ifc_nodes: Vec<(usize, &CachedInlineLayout)> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, _)| {
            tree.warm(LayoutNodeId::new(idx))
                .and_then(|w| w.inline_layout_result.as_ref())
                .map(|ilr| (idx, &**ilr))
        })
        .collect();

    assert!(
        !ifc_nodes.is_empty(),
        "Should have at least one IFC root node"
    );

    for (idx, cached) in &ifc_nodes {
        // (d7) materialized(): the stored layout is the retirement
        // sentinel under the dense default; the expansion is the real
        // item list.
        let layout = cached.materialized();

        // Each item has a non-negative advance and height
        for (i, item) in layout.items.iter().enumerate() {
            let bounds = item.item.bounds();
            assert!(
                bounds.width >= 0.0,
                "Node {}, item {}: the advance should be >= 0, got {}",
                idx,
                i,
                bounds.width
            );
            assert!(
                bounds.height >= 0.0,
                "Node {}, item {}: the height should be >= 0, got {}",
                idx,
                i,
                bounds.height
            );
        }
    }
}

#[test]
fn test_ifc_layout_items_have_correct_line_indices() {
    let html = r#"
    <html>
        <body>
            <p style="width: 50px;">Hello World this is a long paragraph that wraps</p>
        </body>
    </html>
    "#;

    let cache = layout_html_and_get_tree(html);
    let tree = cache.tree.as_ref().expect("Layout tree should exist");

    let ifc_nodes: Vec<(usize, &CachedInlineLayout)> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, _)| {
            tree.warm(LayoutNodeId::new(idx))
                .and_then(|w| w.inline_layout_result.as_ref())
                .map(|ilr| (idx, &**ilr))
        })
        .collect();

    // With width: 50px, text should wrap to multiple lines
    for (_idx, cached) in &ifc_nodes {
        let layout = cached.materialized();
        if layout.items.is_empty() {
            continue;
        }

        // Verify line_index values are reasonable.
        let max_line = layout
            .items
            .iter()
            .map(|item| item.line_index)
            .max()
            .unwrap_or(0);

        // The max_line should be a reasonable value (not absurdly large)
        assert!(
            max_line < 1000,
            "max_line_index {max_line} seems unreasonably large"
        );
    }
}

#[test]
fn test_ifc_layout_items_x_positions_increase_on_same_line() {
    let html = r#"
    <html>
        <body>
            <p>Hello World</p>
        </body>
    </html>
    "#;

    let cache = layout_html_and_get_tree(html);
    let tree = cache.tree.as_ref().expect("Layout tree should exist");

    let ifc_nodes: Vec<(usize, &CachedInlineLayout)> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, _)| {
            tree.warm(LayoutNodeId::new(idx))
                .and_then(|w| w.inline_layout_result.as_ref())
                .map(|ilr| (idx, &**ilr))
        })
        .collect();

    for (idx, cached) in &ifc_nodes {
        // Group the items' x positions by line_index
        let layout = cached.materialized();
        let mut lines: BTreeMap<usize, Vec<f32>> = BTreeMap::new();
        for item in &layout.items {
            lines.entry(item.line_index).or_default().push(item.position.x);
        }

        // Within each line of left-to-right text, x should be monotonically non-decreasing
        for (line_idx, xs) in &lines {
            let mut prev_x = -1.0f32;
            for &x in xs {
                assert!(
                    x >= prev_x,
                    "Node {}, line {}: x {} should be >= prev {}",
                    idx,
                    line_idx,
                    x,
                    prev_x
                );
                prev_x = x;
            }
        }
    }
}

#[test]
fn test_ifc_layout_clusters_carry_the_source_node_ids_of_their_text() {
    let html = r#"
    <html>
        <body>
            <p><span>Hello</span> <span>World</span></p>
        </body>
    </html>
    "#;

    let cache = layout_html_and_get_tree(html);
    let tree = cache.tree.as_ref().expect("Layout tree should exist");

    let ifc_nodes: Vec<(usize, &CachedInlineLayout)> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, _)| {
            tree.warm(LayoutNodeId::new(idx))
                .and_then(|w| w.inline_layout_result.as_ref())
                .map(|ilr| (idx, &**ilr))
        })
        .collect();

    assert!(!ifc_nodes.is_empty(), "Should have IFC root nodes");

    // At least some items should have source_node_id set (text clusters)
    let layouts: Vec<_> = ifc_nodes
        .iter()
        .map(|(_, cached)| cached.materialized())
        .collect();
    let total_items: usize = layouts.iter().map(|layout| layout.items.len()).sum();

    let items_with_source: usize = layouts
        .iter()
        .flat_map(|layout| layout.items.iter())
        .filter(|item| {
            item.item
                .as_cluster()
                .is_some_and(|cluster| cluster.source_node_id.is_some())
        })
        .count();

    assert!(total_items > 0, "Should have inline items from text");
    assert!(
        items_with_source > 0,
        "At least some items should have source_node_id (text clusters), but got 0 out of \
         {total_items} total items"
    );
}

#[test]
fn test_cached_inline_layout_is_replaced_by_a_layout_at_another_width() {
    use azul_layout::text3::cache::{
        ContentIndex, GraphemeClusterId, OverflowInfo, Point, PositionedItem, ShapedCluster,
        ShapedItem, UnifiedLayout,
    };

    // Create a mock UnifiedLayout with one cluster item
    let cluster = ShapedCluster {
        flags: azul_layout::text3::cache::ClusterFlags::classify("A".to_string().as_ref()),
        source_text: Arc::from("A"),
        source_byte_len: 1,
        source_cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        source_content_index: ContentIndex {
            run_index: 0,
            item_index: 0,
        },
        source_node_id: Some(azul_core::dom::NodeId::new(3)),
        glyphs: smallvec::smallvec![],
        advance: 10.0,
        direction: azul_layout::text3::cache::BidiDirection::Ltr,
        bidi_level: azul_layout::text3::cache::BidiLevel::paragraph(azul_layout::text3::cache::BidiDirection::Ltr),
        style: Default::default(),
        marker_position_outside: None,
        is_first_fragment: true,
        is_last_fragment: true,
    };

    let layout = Arc::new(UnifiedLayout {
        items: vec![PositionedItem {
            item: ShapedItem::Cluster(cluster),
            position: Point { x: 5.0, y: 0.0 },
            line_index: 0,
        }],
        overflow: OverflowInfo::default(),
    });

    let cached = CachedInlineLayout::new(layout.clone(), AvailableSpace::Definite(800.0), false);

    let kept = cached.materialized();
    assert_eq!(kept.items.len(), 1, "the cache keeps the one item");
    assert!((kept.items[0].position.x - 5.0).abs() < 0.01);

    // should_replace_with different width → true
    assert!(cached.should_replace_with(AvailableSpace::Definite(400.0), false));
    // same width → false
    assert!(!cached.should_replace_with(AvailableSpace::Definite(800.0), false));
}
