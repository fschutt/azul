//! Integration test for H1 margin em resolution
//!
//! This test verifies that H1 elements with `margin: 0.67em` and `font-size: 2em`
//! correctly resolve the margin to 21.44px (0.67 * 32px), not 10.72px (0.67 * 16px).
//!
//! This was the root cause of the margin collapsing bug where H1 margins were
//! calculated incorrectly due to using hardcoded DEFAULT_FONT_SIZE = 16.0 instead of
//! the element's actual computed font-size.

use azul_css::{
    props::basic::{PixelValue, PropertyContext, ResolutionContext, PhysicalSize},
};

#[test]
fn test_h1_margin_em_resolution_direct() {
    // Direct test of PixelValue resolution with proper context
    
    // H1 has font-size: 2em (parent is 16px) = 32px
    // H1 has margin: 0.67em (own font-size is 32px) = 21.44px
    
    let context = ResolutionContext {
        element_font_size: 32.0,  // H1's computed font-size
        parent_font_size: 16.0,   // Body's font-size
        root_font_size: 16.0,     // Root font-size
        containing_block_size: PhysicalSize::new(800.0, 600.0),
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };
    
    // Test margin resolution (em refers to element's own font-size)
    let margin = PixelValue::em(0.67);
    let resolved_margin = margin.resolve_with_context(&context, PropertyContext::Margin);
    
    println!("H1 margin: 0.67em with font-size 32px");
    println!("  Expected: 21.44px (0.67 * 32)");
    println!("  Actual:   {:.2}px", resolved_margin);
    
    // Allow small floating point error
    assert!(
        (resolved_margin - 21.44).abs() < 0.01,
        "H1 margin should be 21.44px, got {:.2}px",
        resolved_margin
    );
    
    // Test font-size resolution (em refers to parent's font-size)
    let font_size = PixelValue::em(2.0);
    let resolved_font_size = font_size.resolve_with_context(&context, PropertyContext::FontSize);
    
    println!("\nH1 font-size: 2em with parent font-size 16px");
    println!("  Expected: 32px (2.0 * 16)");
    println!("  Actual:   {:.2}px", resolved_font_size);
    
    assert_eq!(
        resolved_font_size, 32.0,
        "H1 font-size should be 32px, got {:.2}px",
        resolved_font_size
    );
}

#[test]
fn test_rem_unit_resolution() {
    // Test rem units (always refer to root font-size)
    
    let context = ResolutionContext {
        element_font_size: 32.0,  // H1's computed font-size
        parent_font_size: 20.0,   // Body's font-size
        root_font_size: 18.0,     // Root font-size (custom)
        containing_block_size: PhysicalSize::new(800.0, 600.0),
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };
    
    // Rem always uses root font-size
    let margin = PixelValue::rem(2.0);
    let resolved_margin = margin.resolve_with_context(&context, PropertyContext::Margin);
    
    println!("Margin: 2rem with root font-size 18px");
    println!("  Expected: 36px (2.0 * 18)");
    println!("  Actual:   {:.2}px", resolved_margin);
    
    assert_eq!(
        resolved_margin, 36.0,
        "2rem margin should be 36px with root font-size 18px, got {:.2}px",
        resolved_margin
    );
    
    // Rem in font-size also uses root
    let font_size = PixelValue::rem(1.5);
    let resolved_font_size = font_size.resolve_with_context(&context, PropertyContext::FontSize);
    
    println!("\nFont-size: 1.5rem with root font-size 18px");
    println!("  Expected: 27px (1.5 * 18)");
    println!("  Actual:   {:.2}px", resolved_font_size);
    
    assert_eq!(
        resolved_font_size, 27.0,
        "1.5rem font-size should be 27px, got {:.2}px",
        resolved_font_size
    );
}

#[test]
fn test_percent_margin_resolution() {
    // Test that margin % uses containing block WIDTH (even for top/bottom)
    
    let context = ResolutionContext {
        element_font_size: 16.0,
        parent_font_size: 16.0,
        root_font_size: 16.0,
        containing_block_size: PhysicalSize::new(800.0, 600.0),
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };
    
    let margin_top = PixelValue::percent(10.0); // 10%
    let resolved = margin_top.resolve_with_context(&context, PropertyContext::Margin);
    
    println!("Margin-top: 10% with containing block 800x600");
    println!("  Expected: 80px (10% of width 800, NOT height 600)");
    println!("  Actual:   {:.2}px", resolved);
    
    // CSS spec: margin % ALWAYS uses containing block WIDTH
    assert_eq!(
        resolved, 80.0,
        "Margin-top 10% should be 80px (10% of 800), got {:.2}px",
        resolved
    );
}

#[test]
fn test_nested_em_calculation() {
    // Test nested em resolution: body 1.5em -> 24px, then div 1.2em -> 28.8px
    
    // Simulate: html (16px) -> body (1.5em = 24px) -> div (1.2em = 28.8px) -> p (margin: 0.5em)
    
    let html_font_size = 16.0;
    let body_font_size = 1.5 * html_font_size; // 24px
    let div_font_size = 1.2 * body_font_size;  // 28.8px
    
    let context = ResolutionContext {
        element_font_size: div_font_size,  // P's font-size inherits from div
        parent_font_size: div_font_size,
        root_font_size: html_font_size,
        containing_block_size: PhysicalSize::new(800.0, 600.0),
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };
    
    let margin = PixelValue::em(0.5);
    let resolved = margin.resolve_with_context(&context, PropertyContext::Margin);
    
    println!("Nested em: html(16) -> body(24) -> div(28.8) -> p margin: 0.5em");
    println!("  Expected: 14.4px (0.5 * 28.8)");
    println!("  Actual:   {:.2}px", resolved);
    
    assert!(
        (resolved - 14.4).abs() < 0.01,
        "Nested em margin should be 14.4px, got {:.2}px",
        resolved
    );
}

#[test]
fn test_comparison_old_vs_new() {
    // Compare old (broken) behavior vs new (correct) behavior
    
    let h1_font_size = 32.0;
    let margin_factor = 0.67;
    
    // Old behavior: hardcoded DEFAULT_FONT_SIZE = 16.0
    #[allow(deprecated)]
    let old_margin = {
        use azul_css::props::basic::DEFAULT_FONT_SIZE;
        margin_factor * DEFAULT_FONT_SIZE
    };
    
    // New behavior: uses element's actual font-size
    let context = ResolutionContext {
        element_font_size: h1_font_size,
        parent_font_size: 16.0,
        root_font_size: 16.0,
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };
    
    let margin = PixelValue::em(margin_factor);
    let new_margin = margin.resolve_with_context(&context, PropertyContext::Margin);
    
    println!("\nComparison: H1 with font-size 32px, margin: 0.67em");
    println!("  Old (broken):  {:.2}px (0.67 * 16 hardcoded)", old_margin);
    println!("  New (correct): {:.2}px (0.67 * 32 actual)", new_margin);
    println!("  Difference:    {:.2}px", new_margin - old_margin);
    
    assert_eq!(old_margin, 10.72, "Old behavior should give 10.72px");
    assert!((new_margin - 21.44).abs() < 0.01, "New behavior should give 21.44px");
    
    // This is the bug we fixed!
    assert!(
        new_margin > old_margin,
        "New margin ({:.2}px) should be larger than old margin ({:.2}px)",
        new_margin,
        old_margin
    );
}
