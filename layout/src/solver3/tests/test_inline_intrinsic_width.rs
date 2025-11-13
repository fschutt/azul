/// Test for CSS Sizing Level 3 compliance - Inline intrinsic width calculation
/// Reference: https://www.w3.org/TR/css-sizing-3/#intrinsic-sizes
///
/// According to the spec, the intrinsic size of an inline element should be based on
/// its content (text). For inline text, the max-content width is the width of the text
/// without line breaks.
///
/// This test verifies that inline elements with text content have their intrinsic
/// widths calculated correctly, which is essential for proper line breaking in parent
/// block containers.

#[cfg(test)]
mod inline_intrinsic_width_tests {
    use std::collections::BTreeMap;
    
    use azul_core::{
        dom::{Dom, DomId, FormattingContext, NodeType},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        styled_dom::StyledDom,
    };
    use azul_css::parser2::CssApiWrapper;
    
    use crate::{
        solver3::{
            cache::LayoutCache,
            layout_document,
            LayoutError,
        },
        text3::cache::{FontManager, LayoutCache as TextLayoutCache},
    };
    
    use super::super::create_test_font_manager;

    /// Test that inline elements containing text have non-zero intrinsic width
    ///
    /// CSS Sizing Level 3, Section 4.1:
    /// "The min-content inline size of an inline box is the width of its longest
    /// unbreakable unit (ignoring soft wrap opportunities)."
    ///
    /// "The max-content inline size of an inline box is the width of its content
    /// with no wrapping."
    ///
    /// For text content "Hello world", the max-content width should be the width
    /// of the full string, not zero.
    #[test]
    fn test_inline_text_has_nonzero_intrinsic_width() {
        // Create a simple DOM with text content
        // Using Dom::label creates a proper inline element with text
        let mut dom = Dom::body();
        dom.add_child(Dom::text("Hello world"));

        let mut styled_dom = StyledDom::new(&mut dom, CssApiWrapper::empty());
        styled_dom.dom_id = DomId::ROOT_ID;

        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::new(800.0, 600.0),
        };

        let mut layout_cache = LayoutCache {
            tree: None,
            calculated_positions: BTreeMap::new(),
            viewport: None,
            scroll_ids: BTreeMap::new(),
            scroll_id_to_node_id: BTreeMap::new(),
        };
        let mut text_cache = TextLayoutCache::new();
        let font_manager = create_test_font_manager().expect("Failed to create font manager");
        let scroll_offsets = BTreeMap::new();
        let selections = BTreeMap::new();
        let mut debug_messages = None;

        let result = layout_document(
            &mut layout_cache,
            &mut text_cache,
            styled_dom,
            viewport,
            &font_manager,
            &scroll_offsets,
            &selections,
            &mut debug_messages,
            None,
            &azul_core::resources::RendererResources::default(),
            azul_core::resources::IdNamespace(0),
            DomId::ROOT_ID,
        );

        assert!(
            result.is_ok(),
            "Layout should succeed: {:?}",
            result.err()
        );

        // Check the layout tree
        let tree = layout_cache.tree.as_ref()
            .expect("Layout tree should exist after layout");

        eprintln!("\n=== LAYOUT TREE ===");
        for (idx, node) in tree.nodes.iter().enumerate() {
            eprintln!("Node {}: {:?}", idx, node.dom_node_id);
            eprintln!("  Formatting Context: {:?}", node.formatting_context);
            eprintln!("  Intrinsic sizes: {:?}", node.intrinsic_sizes);
            eprintln!("  Used size: {:?}", node.used_size);
            eprintln!("  Children: {:?}", node.children);
        }

        // Find nodes with inline formatting context
        let mut found_inline_with_zero_intrinsic = false;
        let mut found_inline_with_nonzero_intrinsic = false;
        
        for (idx, node) in tree.nodes.iter().enumerate() {
            if let Some(intrinsic) = &node.intrinsic_sizes {
                if matches!(node.formatting_context, FormattingContext::Inline) {
                    eprintln!("\nFound INLINE node at index {}", idx);
                    eprintln!("  max_content_width: {}", intrinsic.max_content_width);
                    eprintln!("  min_content_width: {}", intrinsic.min_content_width);
                    
                    if intrinsic.max_content_width > 0.0 {
                        found_inline_with_nonzero_intrinsic = true;
                    } else {
                        found_inline_with_zero_intrinsic = true;
                    }
                }
            }
        }

        // CRITICAL TEST: According to CSS Sizing Level 3, inline elements containing
        // text MUST have non-zero intrinsic width based on their content.
        //
        // Current bug: Inline elements have max_content_width=0, which causes
        // them to be laid out with available_width=0, forcing every character
        // onto a new line.
        //
        // Expected: max_content_width should be the width of "Hello world" (~80-100px)
        assert!(
            !found_inline_with_zero_intrinsic,
            "BUG DETECTED: Inline nodes with text have zero intrinsic width! \
             This violates CSS Sizing Level 3 and causes incorrect line breaking."
        );

        assert!(
            found_inline_with_nonzero_intrinsic,
            "Expected to find at least one inline node with non-zero intrinsic width"
        );
    }

    /// Test a more explicit case: block container with inline child containing text
    ///
    /// This tests the specific bug where inline elements get width=0 in their
    /// constraints when being laid out as children of a block container.
    #[test]
    fn test_inline_child_gets_correct_available_width_in_bfc() {
        // Create DOM: <body><p>Test text that should wrap properly</p></body>
        let mut dom = Dom::body();
        dom.add_child(Dom::text("Test text that should wrap properly"));

        let mut styled_dom = StyledDom::new(&mut dom, CssApiWrapper::empty());
        styled_dom.dom_id = DomId::ROOT_ID;

        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::new(400.0, 600.0), // Narrow width to test wrapping
        };

        let mut layout_cache = LayoutCache {
            tree: None,
            calculated_positions: BTreeMap::new(),
            viewport: None,
            scroll_ids: BTreeMap::new(),
            scroll_id_to_node_id: BTreeMap::new(),
        };
        let mut text_cache = TextLayoutCache::new();
        let font_manager = create_test_font_manager().expect("Failed to create font manager");
        let scroll_offsets = BTreeMap::new();
        let selections = BTreeMap::new();
        let mut debug_messages = Some(Vec::new());

        let result = layout_document(
            &mut layout_cache,
            &mut text_cache,
            styled_dom,
            viewport,
            &font_manager,
            &scroll_offsets,
            &selections,
            &mut debug_messages,
            None,
            &azul_core::resources::RendererResources::default(),
            azul_core::resources::IdNamespace(0),
            DomId::ROOT_ID,
        );

        assert!(result.is_ok(), "Layout should succeed");

        // Check debug messages for the bug signature:
        // "available_size=0xinf" or "available_width=0"
        if let Some(messages) = debug_messages {
            eprintln!("\n=== DEBUG MESSAGES ===");
            for msg in &messages {
                eprintln!("{}", msg.message);
            }

            // Look for the bug: inline nodes getting zero available width
            let has_zero_width_bug = messages.iter().any(|msg| {
                msg.message.contains("available_width=0") && 
                !msg.message.contains("available_width=0.0") && // Avoid false positives like "800.0"
                msg.message.contains("Inline")
            });

            assert!(
                !has_zero_width_bug,
                "BUG DETECTED: Inline elements are receiving available_width=0! \
                 This causes all characters to break onto separate lines."
            );
        }

        // Also check the layout tree
        let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
        
        for (idx, node) in tree.nodes.iter().enumerate() {
            if let Some(intrinsic) = &node.intrinsic_sizes {
                if matches!(node.formatting_context, FormattingContext::Inline) {
                    eprintln!("Inline node {} intrinsic: max={}, min={}", 
                        idx, intrinsic.max_content_width, intrinsic.min_content_width);
                    
                    // CRITICAL: Inline nodes with text MUST have non-zero intrinsic width
                    assert!(
                        intrinsic.max_content_width > 0.0,
                        "Inline node {} has zero max_content_width - violates CSS Sizing Level 3",
                        idx
                    );
                }
            }
        }
    }
}
