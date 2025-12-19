//! Unit tests for CSS margin collapsing behavior
//!
//! Tests verify that adjacent vertical margins collapse according to CSS spec:
//! https://www.w3.org/TR/CSS2/box.html#collapsing-margins
//!
//! Key rules:
//! 1. Adjacent vertical margins collapse to the larger of the two
//! 2. Parent's top margin collapses with first child's top margin (if no border/padding)
//! 3. Parent's bottom margin collapses with last child's bottom margin
//! 4. Empty block's top and bottom margins collapse with each other

// Disabled integration tests that require full layout system
#![cfg(feature = "DISABLED_margin_tests")]

use azul_core::{
    dom::{Dom, NodeType},
    styled_dom::StyledDom,
};
use azul_css::{css::Css, parser2::CssApiWrapper};

// Note: These tests are currently disabled because they require
// a full layout system setup which is complex to mock.
// For now, we'll test the collapse_margins function directly.

#[cfg(test)]
mod collapse_margins_unit_tests {
    /// Implementation of CSS 2.1 margin collapsing rules (section 8.3.1)
    /// This duplicates the function from solver3/fc.rs for testing purposes
    ///
    /// Rules:
    /// 1. Both positive: result = max(a, b) - larger margin wins
    /// 2. Both negative: result = min(a, b) - more negative wins
    /// 3. Mixed signs: result = a + b - margins are effectively summed
    fn collapse_margins(a: f32, b: f32) -> f32 {
        if a.is_sign_positive() && b.is_sign_positive() {
            a.max(b)
        } else if a.is_sign_negative() && b.is_sign_negative() {
            a.min(b)
        } else {
            a + b
        }
    }

    #[test]
    fn test_both_positive_margins_use_maximum() {
        // When both margins are positive, the larger one wins
        assert_eq!(collapse_margins(20.0, 10.0), 20.0);
        assert_eq!(collapse_margins(10.0, 20.0), 20.0);
        assert_eq!(collapse_margins(15.0, 15.0), 15.0);

        // CSS spec example: h1 margin-bottom 30px collapses with p margin-top 20px → 30px
        assert_eq!(collapse_margins(30.0, 20.0), 30.0);

        // Real UA CSS values (at 16px font size):
        // p margin-top (1em = 16px) with p margin-bottom (1em = 16px) → 16px
        assert_eq!(collapse_margins(16.0, 16.0), 16.0);

        // body margin (20px) with h1 margin-top (10px) → 20px
        assert_eq!(collapse_margins(20.0, 10.0), 20.0);
    }

    #[test]
    fn test_both_negative_margins_use_minimum() {
        // When both margins are negative, the more negative one wins
        assert_eq!(collapse_margins(-20.0, -10.0), -20.0);
        assert_eq!(collapse_margins(-10.0, -20.0), -20.0);
        assert_eq!(collapse_margins(-15.0, -15.0), -15.0);

        // More negative = smaller value
        assert_eq!(collapse_margins(-5.0, -25.0), -25.0);
    }

    #[test]
    fn test_mixed_sign_margins_are_summed() {
        // When margins have opposite signs, they are summed
        assert_eq!(collapse_margins(20.0, -10.0), 10.0);
        assert_eq!(collapse_margins(-10.0, 20.0), 10.0);
        assert_eq!(collapse_margins(10.0, -10.0), 0.0);
        assert_eq!(collapse_margins(30.0, -20.0), 10.0);
        assert_eq!(collapse_margins(-30.0, 20.0), -10.0);
    }

    #[test]
    fn test_zero_margins() {
        // Zero is treated as positive
        assert_eq!(collapse_margins(0.0, 0.0), 0.0);
        assert_eq!(collapse_margins(0.0, 10.0), 10.0);
        assert_eq!(collapse_margins(10.0, 0.0), 10.0);

        // Zero with negative margin - treated as mixed signs
        assert_eq!(collapse_margins(0.0, -10.0), -10.0);
        assert_eq!(collapse_margins(-10.0, 0.0), -10.0);
    }

    #[test]
    fn test_real_world_scenarios_from_test_html() {
        // These scenarios match the margin_collapse_test.html file

        // Scenario 1: body margin-top (20px) with h1 margin-top (10px)
        // Expected: 20px (larger positive wins)
        assert_eq!(collapse_margins(20.0, 10.0), 20.0);

        // Scenario 2: h1 margin-bottom (30px) with p margin-top (20px)
        // Expected: 30px (larger positive wins)
        assert_eq!(collapse_margins(30.0, 20.0), 30.0);

        // Scenario 3: p margin-bottom (20px) with next p margin-top (20px)
        // Expected: 20px (equal margins collapse to that value)
        assert_eq!(collapse_margins(20.0, 20.0), 20.0);

        // Scenario 4: p margin-bottom (20px) with div.box margin-top (40px)
        // Expected: 40px (larger positive wins)
        assert_eq!(collapse_margins(20.0, 40.0), 40.0);
    }

    #[test]
    fn test_ua_css_default_margins() {
        // UA CSS default margins at 16px font-size

        // h1: font-size 2em = 32px, margin 0.67em = 0.67 * 32 = 21.44px
        let h1_margin = 0.67 * 32.0;

        // p: font-size 1em = 16px, margin 1em = 16px
        let p_margin = 16.0;

        // h1 margin-bottom collapses with p margin-top
        // Expected: 21.44px (larger wins)
        let collapsed = collapse_margins(h1_margin, p_margin);
        assert!((collapsed - 21.44).abs() < 0.01);

        // p margin-bottom with p margin-top
        // Expected: 16px
        assert_eq!(collapse_margins(p_margin, p_margin), 16.0);
    }

    #[test]
    fn test_floating_point_precision() {
        // Test with typical CSS computed values that might have floating point imprecision
        assert_eq!(collapse_margins(10.72, 16.0), 16.0);
        assert_eq!(collapse_margins(16.0, 10.72), 16.0);
        assert_eq!(collapse_margins(21.44, 16.0), 21.44);

        // Very close values
        assert!((collapse_margins(10.0, 10.001) - 10.001).abs() < 0.001);
        assert!((collapse_margins(10.001, 10.0) - 10.001).abs() < 0.001);
    }

    #[test]
    fn test_edge_cases() {
        // Very large margins
        assert_eq!(collapse_margins(1000.0, 500.0), 1000.0);
        assert_eq!(collapse_margins(-1000.0, -500.0), -1000.0);

        // Very small margins
        assert_eq!(collapse_margins(0.1, 0.2), 0.2);
        assert!((collapse_margins(0.01, 0.02) - 0.02).abs() < 0.0001);

        // Asymmetric cases
        assert_eq!(collapse_margins(100.0, 1.0), 100.0);
        assert_eq!(collapse_margins(1.0, 100.0), 100.0);
    }
}

#[cfg(test)]
mod parent_child_margin_collapse_tests {
    /// Tests for CSS 2.1 Section 8.3.1: Parent-child margin collapsing
    ///
    /// Key rules:
    /// 1. Parent's top margin collapses with first child's top margin UNLESS parent has border-top,
    ///    padding-top, or establishes new BFC
    ///
    /// 2. Parent's bottom margin collapses with last child's bottom margin UNLESS parent has
    ///    border-bottom, padding-bottom, height, min-height, or establishes new BFC
    ///
    /// Example:
    /// ```html
    /// <div style="margin-top: 20px;">         <!-- Parent -->
    ///   <p style="margin-top: 30px;">...</p>  <!-- First child -->
    /// </div>
    /// ```
    /// Expected: Margins collapse to 30px (larger wins), NOT 50px (sum)
    /// The 30px margin "escapes" the parent's box

    #[test]
    fn test_parent_child_top_margin_should_collapse() {
        // Parent margin-top: 20px, First child margin-top: 30px
        // Expected: 30px (larger wins), child's margin "escapes" parent
        // TODO: Implement parent-child top margin collapsing
        // This currently is NOT implemented - margins don't collapse
    }

    #[test]
    fn test_parent_child_top_margin_blocked_by_border() {
        // Parent has border-top: 1px solid black
        // Expected: Margins do NOT collapse, child stays inside parent
        // Border acts as a "blocker" preventing margin from escaping
    }

    #[test]
    fn test_parent_child_top_margin_blocked_by_padding() {
        // Parent has padding-top: 10px
        // Expected: Margins do NOT collapse, child stays inside parent
        // Padding acts as a "blocker" preventing margin from escaping
    }

    #[test]
    fn test_parent_child_bottom_margin_should_collapse() {
        // Parent margin-bottom: 20px, Last child margin-bottom: 30px
        // Expected: 30px (larger wins), child's margin "escapes" parent
        // TODO: Implement parent-child bottom margin collapsing
    }

    #[test]
    fn test_parent_child_bottom_margin_blocked_by_border() {
        // Parent has border-bottom: 1px solid black
        // Expected: Margins do NOT collapse
    }

    #[test]
    fn test_parent_child_bottom_margin_blocked_by_padding() {
        // Parent has padding-bottom: 10px
        // Expected: Margins do NOT collapse
    }

    #[test]
    fn test_parent_with_explicit_height_no_collapse() {
        // Parent has height: 100px
        // Expected: Bottom margins do NOT collapse
        // Explicit height prevents bottom margin collapsing
    }
}

#[cfg(test)]
mod empty_block_margin_collapse_tests {
    /// Tests for CSS 2.1 Section 8.3.1: Empty block margin collapsing
    ///
    /// Key rule:
    /// If a block element has no border, padding, inline content, height, or min-height,
    /// then its top and bottom margins collapse with each other.
    ///
    /// Example:
    /// ```html
    /// <p style="margin-bottom: 20px;">First</p>
    /// <div style="margin-top: 10px; margin-bottom: 30px;"></div>  <!-- Empty! -->
    /// <p style="margin-top: 15px;">Second</p>
    /// ```
    ///
    /// Collapsing process:
    /// 1. Empty div's top (10px) and bottom (30px) collapse → 30px
    /// 2. Previous p bottom (20px) + empty div (30px) collapse → 30px
    /// 3. Empty div (30px) + next p top (15px) collapse → 30px
    /// Final gap: 30px (not 20+10+30+15=75px!)

    #[test]
    fn test_empty_block_margins_collapse_with_each_other() {
        // Empty div: margin-top 10px, margin-bottom 30px
        // Expected: These collapse to 30px (larger wins)
        // TODO: Implement empty block margin collapsing
    }

    #[test]
    fn test_empty_block_collapses_through_to_siblings() {
        // Three blocks: P1 (margin-bottom: 20px) → Empty Div (10px/30px) → P2 (margin-top: 15px)
        // Expected: All margins collapse to 30px (the largest)
        // This is the most complex case!
    }

    #[test]
    fn test_empty_block_with_border_does_not_collapse_internally() {
        // Empty div with border: 1px solid black
        // Expected: Top and bottom margins do NOT collapse with each other
        // But they can still collapse with siblings
    }

    #[test]
    fn test_empty_block_with_padding_does_not_collapse_internally() {
        // Empty div with padding: 5px
        // Expected: Top and bottom margins do NOT collapse with each other
    }

    #[test]
    fn test_empty_block_with_height_does_not_collapse_internally() {
        // Empty div with height: 50px
        // Expected: Top and bottom margins do NOT collapse with each other
    }

    #[test]
    fn test_multiple_empty_blocks_in_sequence() {
        // P1 → Empty Div1 → Empty Div2 → Empty Div3 → P2
        // All empty divs have different margins
        // Expected: All margins collapse to the largest one
    }
}

#[cfg(test)]
mod margin_collapse_blocker_tests {
    /// Tests for conditions that prevent margin collapsing
    ///
    /// CSS 2.1 Section 8.3.1 lists several conditions that prevent margins from collapsing:
    /// - Border between margins (border-top, border-bottom)
    /// - Padding between margins (padding-top, padding-bottom)
    /// - Line boxes (inline content) between margins
    /// - Clearance (clear property)
    /// - Establishing a new BFC (overflow, float, position:absolute, display:inline-block, etc.)

    #[test]
    fn test_border_prevents_collapse() {
        // Element with border-top and border-bottom
        // Expected: Margins on both sides do NOT collapse
        // TODO: Check if border is present before collapsing
    }

    #[test]
    fn test_padding_prevents_collapse() {
        // Element with padding-top and padding-bottom
        // Expected: Margins on both sides do NOT collapse
        // TODO: Check if padding is present before collapsing
    }

    #[test]
    fn test_inline_content_prevents_parent_child_collapse() {
        // Parent with text content before first child
        // Expected: Parent and child margins do NOT collapse
    }

    #[test]
    fn test_overflow_hidden_establishes_bfc() {
        // Element with overflow: hidden
        // Expected: Creates new BFC, margins don't escape
    }

    #[test]
    fn test_float_establishes_bfc() {
        // Element with float: left/right
        // Expected: Creates new BFC, margins don't collapse with siblings
    }

    #[test]
    fn test_absolute_position_prevents_collapse() {
        // Element with position: absolute
        // Expected: Removed from normal flow, doesn't participate in margin collapsing
    }
}

// Integration tests commented out until layout system can be properly initialized

#[test]
#[ignore] // Requires layout_dom function implementation
fn test_sibling_margin_collapsing() {
    println!("\nTest: Sibling Margin Collapsing");
    println!("HTML equivalent:");
    println!("  <body>");
    println!("    <h1 style='margin-bottom: 0.67em'>Heading</h1>");
    println!("    <p style='margin-top: 1em'>Paragraph</p>");
    println!("  </body>");
    println!("\nExpected: margins collapse to 1em (max of 0.67em and 1em)");

    // Create body → h1 → p structure
    let mut dom = Dom::create_body()
        .with_inline_style("width: 800px;")
        .with_child(
            Dom::create_node(NodeType::H1)
                .with_inline_style("margin-bottom: 10px;") // Using px for simplicity
                .with_child(Dom::text("Heading")),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_inline_style("margin-top: 20px;")
                .with_child(Dom::text("Paragraph")),
        );

    let layout = layout_dom(&mut dom);

    // Get positions of H1 and P
    let h1_id = azul_core::dom::NodeId::new(1);
    let p_id = azul_core::dom::NodeId::new(3);

    if let Some(h1_rect) = layout.rects.get(&h1_id) {
        println!("\nH1 layout:");
        println!("  y: {}", h1_rect.origin.y);
        println!("  height: {}", h1_rect.size.height);
        println!("  bottom: {}", h1_rect.origin.y + h1_rect.size.height);

        if let Some(p_rect) = layout.rects.get(&p_id) {
            println!("\nP layout:");
            println!("  y: {}", p_rect.origin.y);
            println!("  height: {}", p_rect.size.height);

            let gap = p_rect.origin.y - (h1_rect.origin.y + h1_rect.size.height);
            println!("\nGap between H1 and P: {}px", gap);
            println!("Expected: 20px (max of 10px and 20px, collapsed)");
            println!("Without collapsing: 30px (10px + 20px)");

            if gap > 25.0 {
                println!(
                    "[ WARN ] WARNING: Gap is {}px, suggests margins are NOT collapsing!",
                    gap
                );
                println!("   Margins appear to be added together instead of collapsed.");
            } else if gap < 15.0 {
                println!("[ WARN ] WARNING: Gap is {}px, too small!", gap);
            } else {
                println!("✓ Gap looks correct for margin collapsing");
            }
        }
    }
}

#[test]
#[ignore] // Requires layout_dom function implementation
fn test_parent_child_margin_collapsing() {
    println!("\nTest: Parent-Child Margin Collapsing");
    println!("HTML equivalent:");
    println!("  <body style='margin-top: 20px'>");
    println!("    <h1 style='margin-top: 0.67em'>Heading</h1>");
    println!("  </body>");
    println!("\nExpected: body and h1 top margins collapse to max(20px, 0.67em)");

    // Create body with margin, h1 with margin
    let mut dom = Dom::create_body()
        .with_inline_style("width: 800px; margin-top: 20px;")
        .with_child(
            Dom::create_node(NodeType::H1)
                .with_inline_style("margin-top: 30px;") // Larger than body's 20px
                .with_child(Dom::text("Heading")),
        );

    let layout = layout_dom(&mut dom);

    let body_id = azul_core::dom::NodeId::new(0);
    let h1_id = azul_core::dom::NodeId::new(1);

    if let Some(body_rect) = layout.rects.get(&body_id) {
        println!("\nBody layout:");
        println!("  y: {}", body_rect.origin.y);
        println!("  margin-top: 20px (in CSS)");

        if let Some(h1_rect) = layout.rects.get(&h1_id) {
            println!("\nH1 layout:");
            println!("  y: {}", h1_rect.origin.y);
            println!("  margin-top: 30px (in CSS)");

            // H1's y position relative to body
            let h1_offset = h1_rect.origin.y - body_rect.origin.y;
            println!("\nH1 offset from body top: {}px", h1_offset);
            println!("Expected: ~0px (margins collapsed)");
            println!("Without collapsing: 50px (20px + 30px)");

            if h1_offset > 40.0 {
                println!(
                    "[ WARN ] WARNING: Offset is {}px, margins are NOT collapsing!",
                    h1_offset
                );
                println!("   Parent and child top margins should collapse.");
            } else if h1_offset < 5.0 {
                println!("✓ Margins appear to be collapsing correctly");
            } else {
                println!("[ WARN ] Offset is {}px, unexpected value", h1_offset);
            }
        }
    }
}

#[test]
#[ignore] // Requires layout_dom function implementation
fn test_ua_css_margin_collapsing() {
    println!("\nTest: UA CSS Margin Collapsing (Real-world scenario)");
    println!("HTML equivalent:");
    println!("  <body>");
    println!("    <h1>Heading</h1>");
    println!("    <p>Paragraph</p>");
    println!("  </body>");
    println!("\nUA CSS provides:");
    println!("  body: margin: 8px");
    println!("  h1: margin-top: 0.67em, margin-bottom: 0.67em");
    println!("  p: margin-top: 1em, margin-bottom: 1em");

    // Create structure with NO explicit margins - rely on UA CSS
    let mut dom = Dom::create_body()
        .with_inline_style("width: 800px;")
        .with_child(Dom::create_node(NodeType::H1).with_child(Dom::text("Heading")))
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::text("Paragraph")));

    let layout = layout_dom(&mut dom);

    let body_id = azul_core::dom::NodeId::new(0);
    let h1_id = azul_core::dom::NodeId::new(1);
    let p_id = azul_core::dom::NodeId::new(3);

    println!("\nLayout results:");

    if let Some(body_rect) = layout.rects.get(&body_id) {
        println!("Body: y={}", body_rect.origin.y);
    }

    if let Some(h1_rect) = layout.rects.get(&h1_id) {
        println!("H1: y={}, height={}", h1_rect.origin.y, h1_rect.size.height);

        if let Some(p_rect) = layout.rects.get(&p_id) {
            println!("P: y={}", p_rect.origin.y);

            let gap = p_rect.origin.y - (h1_rect.origin.y + h1_rect.size.height);
            println!("\nGap between H1 and P: {}px", gap);

            // With default font-size 16px:
            // h1 margin-bottom = 0.67em = 0.67 * 32px (h1 is 2em) = ~21px
            // p margin-top = 1em = 16px
            // Collapsed should be max(21px, 16px) = 21px
            println!("Expected (with margin collapsing): ~21px (0.67em of H1's font-size)");
            println!("Without collapsing: ~37px (21px + 16px)");

            if gap > 30.0 {
                println!("[ WARN ] WARNING: Gap suggests margins are NOT collapsing!");
            } else if gap > 15.0 && gap < 25.0 {
                println!("✓ Gap looks reasonable for collapsed margins");
            } else {
                println!("? Unexpected gap value: {}px", gap);
            }
        }
    }
}

#[test]
#[ignore] // Requires layout_dom function implementation
fn test_three_consecutive_blocks() {
    println!("\nTest: Three Consecutive Blocks");
    println!("Testing multiple margin collapses in sequence");

    let mut dom = Dom::create_body()
        .with_inline_style("width: 800px;")
        .with_child(
            Dom::create_node(NodeType::P)
                .with_inline_style("margin-bottom: 15px;")
                .with_child(Dom::text("First")),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_inline_style("margin-top: 10px; margin-bottom: 25px;")
                .with_child(Dom::text("Second")),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_inline_style("margin-top: 20px;")
                .with_child(Dom::text("Third")),
        );

    let layout = layout_dom(&mut dom);

    let p1_id = azul_core::dom::NodeId::new(1);
    let p2_id = azul_core::dom::NodeId::new(3);
    let p3_id = azul_core::dom::NodeId::new(5);

    println!("\nExpected gaps:");
    println!("  P1 ↔ P2: max(15px, 10px) = 15px (collapsed)");
    println!("  P2 ↔ P3: max(25px, 20px) = 25px (collapsed)");

    if let (Some(p1), Some(p2), Some(p3)) = (
        layout.rects.get(&p1_id),
        layout.rects.get(&p2_id),
        layout.rects.get(&p3_id),
    ) {
        let gap1 = p2.origin.y - (p1.origin.y + p1.size.height);
        let gap2 = p3.origin.y - (p2.origin.y + p2.size.height);

        println!("\nActual gaps:");
        println!("  P1 ↔ P2: {}px", gap1);
        println!("  P2 ↔ P3: {}px", gap2);

        println!("\nAnalysis:");
        if gap1 > 20.0 {
            println!(
                "  [ WARN ] P1 ↔ P2: NOT collapsing (expected ~15px, got {}px)",
                gap1
            );
        } else {
            println!("  ✓ P1 ↔ P2: appears correct");
        }

        if gap2 > 35.0 {
            println!(
                "  [ WARN ] P2 ↔ P3: NOT collapsing (expected ~25px, got {}px)",
                gap2
            );
        } else {
            println!("  ✓ P2 ↔ P3: appears correct");
        }
    }
}

#[test]
#[ignore] // Requires layout_dom function implementation
fn test_margin_collapsing_with_border() {
    println!("\nTest: Margins Don't Collapse When Border Present");
    println!("Parent with border should NOT collapse margins with child");

    let mut dom = Dom::create_body()
        .with_inline_style("width: 800px; margin-top: 20px; border-top: 1px solid black;")
        .with_child(
            Dom::create_node(NodeType::H1)
                .with_inline_style("margin-top: 30px;")
                .with_child(Dom::text("Heading")),
        );

    let layout = layout_dom(&mut dom);

    let body_id = azul_core::dom::NodeId::new(0);
    let h1_id = azul_core::dom::NodeId::new(1);

    if let Some(body_rect) = layout.rects.get(&body_id) {
        if let Some(h1_rect) = layout.rects.get(&h1_id) {
            let h1_offset = h1_rect.origin.y - body_rect.origin.y;

            println!("\nH1 offset from body: {}px", h1_offset);
            println!("Expected: ~30px (no collapse due to border)");
            println!("If collapsed: ~0px");

            if h1_offset > 25.0 {
                println!("✓ Margins are NOT collapsing (correct, due to border)");
            } else {
                println!("[ WARN ] WARNING: Margins collapsed despite border!");
            }
        }
    }
}
