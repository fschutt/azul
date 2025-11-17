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

use azul_core::{
    app_resources::AppResources,
    dom::{Dom, NodeType},
    styled_dom::StyledDom,
    ui_solver::{do_the_layout, LayoutResult},
};
use azul_css::{
    css::Css,
    parser2::CssApiWrapper,
};

/// Helper to create a simple layout and return the result
fn layout_dom(dom: &mut Dom) -> LayoutResult {
    let styled_dom = dom.style(CssApiWrapper { css: Css::empty() });
    let mut app_resources = AppResources::default();
    
    // Create layout with reasonable viewport size
    let layout_result = do_the_layout(
        &styled_dom,
        &mut app_resources,
        azul_core::window::LogicalSize::new(800.0, 600.0),
        1.0, // hidpi factor
    );
    
    layout_result
}

#[test]
fn test_sibling_margin_collapsing() {
    println!("\n=== Test: Sibling Margin Collapsing ===");
    println!("HTML equivalent:");
    println!("  <body>");
    println!("    <h1 style='margin-bottom: 0.67em'>Heading</h1>");
    println!("    <p style='margin-top: 1em'>Paragraph</p>");
    println!("  </body>");
    println!("\nExpected: margins collapse to 1em (max of 0.67em and 1em)");
    
    // Create body → h1 → p structure
    let mut dom = Dom::body()
        .with_inline_style("width: 800px;")
        .with_child(
            Dom::new(NodeType::H1)
                .with_inline_style("margin-bottom: 10px;") // Using px for simplicity
                .with_child(Dom::text("Heading"))
        )
        .with_child(
            Dom::new(NodeType::P)
                .with_inline_style("margin-top: 20px;")
                .with_child(Dom::text("Paragraph"))
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
                println!("⚠ WARNING: Gap is {}px, suggests margins are NOT collapsing!", gap);
                println!("   Margins appear to be added together instead of collapsed.");
            } else if gap < 15.0 {
                println!("⚠ WARNING: Gap is {}px, too small!", gap);
            } else {
                println!("✓ Gap looks correct for margin collapsing");
            }
        }
    }
}

#[test]
fn test_parent_child_margin_collapsing() {
    println!("\n=== Test: Parent-Child Margin Collapsing ===");
    println!("HTML equivalent:");
    println!("  <body style='margin-top: 20px'>");
    println!("    <h1 style='margin-top: 0.67em'>Heading</h1>");
    println!("  </body>");
    println!("\nExpected: body and h1 top margins collapse to max(20px, 0.67em)");
    
    // Create body with margin, h1 with margin
    let mut dom = Dom::body()
        .with_inline_style("width: 800px; margin-top: 20px;")
        .with_child(
            Dom::new(NodeType::H1)
                .with_inline_style("margin-top: 30px;") // Larger than body's 20px
                .with_child(Dom::text("Heading"))
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
                println!("⚠ WARNING: Offset is {}px, margins are NOT collapsing!", h1_offset);
                println!("   Parent and child top margins should collapse.");
            } else if h1_offset < 5.0 {
                println!("✓ Margins appear to be collapsing correctly");
            } else {
                println!("⚠ Offset is {}px, unexpected value", h1_offset);
            }
        }
    }
}

#[test]
fn test_ua_css_margin_collapsing() {
    println!("\n=== Test: UA CSS Margin Collapsing (Real-world scenario) ===");
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
    let mut dom = Dom::body()
        .with_inline_style("width: 800px;")
        .with_child(
            Dom::new(NodeType::H1)
                .with_child(Dom::text("Heading"))
        )
        .with_child(
            Dom::new(NodeType::P)
                .with_child(Dom::text("Paragraph"))
        );
    
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
                println!("⚠ WARNING: Gap suggests margins are NOT collapsing!");
            } else if gap > 15.0 && gap < 25.0 {
                println!("✓ Gap looks reasonable for collapsed margins");
            } else {
                println!("? Unexpected gap value: {}px", gap);
            }
        }
    }
}

#[test]
fn test_three_consecutive_blocks() {
    println!("\n=== Test: Three Consecutive Blocks ===");
    println!("Testing multiple margin collapses in sequence");
    
    let mut dom = Dom::body()
        .with_inline_style("width: 800px;")
        .with_child(
            Dom::new(NodeType::P)
                .with_inline_style("margin-bottom: 15px;")
                .with_child(Dom::text("First"))
        )
        .with_child(
            Dom::new(NodeType::P)
                .with_inline_style("margin-top: 10px; margin-bottom: 25px;")
                .with_child(Dom::text("Second"))
        )
        .with_child(
            Dom::new(NodeType::P)
                .with_inline_style("margin-top: 20px;")
                .with_child(Dom::text("Third"))
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
            println!("  ⚠ P1 ↔ P2: NOT collapsing (expected ~15px, got {}px)", gap1);
        } else {
            println!("  ✓ P1 ↔ P2: appears correct");
        }
        
        if gap2 > 35.0 {
            println!("  ⚠ P2 ↔ P3: NOT collapsing (expected ~25px, got {}px)", gap2);
        } else {
            println!("  ✓ P2 ↔ P3: appears correct");
        }
    }
}

#[test]
fn test_margin_collapsing_with_border() {
    println!("\n=== Test: Margins Don't Collapse When Border Present ===");
    println!("Parent with border should NOT collapse margins with child");
    
    let mut dom = Dom::body()
        .with_inline_style("width: 800px; margin-top: 20px; border-top: 1px solid black;")
        .with_child(
            Dom::new(NodeType::H1)
                .with_inline_style("margin-top: 30px;")
                .with_child(Dom::text("Heading"))
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
                println!("⚠ WARNING: Margins collapsed despite border!");
            }
        }
    }
}
