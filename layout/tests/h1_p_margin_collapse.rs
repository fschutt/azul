//! Tests for H1 and P margin collapsing with proper em unit resolution
//!
//! This verifies that the margin collapsing logic works correctly when
//! em units are properly resolved against each element's own font-size.

use azul_css::props::basic::{PixelValue, PropertyContext, ResolutionContext, PhysicalSize};

/// Test the collapse_margins function directly
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
fn test_h1_p_margin_collapse_calculation() {
    // Test that margin collapse logic works with correctly resolved values
    
    // H1: font-size: 2em = 32px, margin: 0.67em
    let h1_context = ResolutionContext {
        element_font_size: 32.0,
        parent_font_size: 16.0,
        root_font_size: 16.0,
        containing_block_size: PhysicalSize::new(800.0, 600.0),
        element_size: None,
        dpi_scale: 1.0,
    };
    
    let h1_margin = PixelValue::em(0.67);
    let h1_margin_px = h1_margin.resolve_with_context(&h1_context, PropertyContext::Margin);
    
    // P: font-size: 1em = 16px, margin: 1em
    let p_context = ResolutionContext {
        element_font_size: 16.0,
        parent_font_size: 16.0,
        root_font_size: 16.0,
        containing_block_size: PhysicalSize::new(800.0, 600.0),
        element_size: None,
        dpi_scale: 1.0,
    };
    
    let p_margin = PixelValue::em(1.0);
    let p_margin_px = p_margin.resolve_with_context(&p_context, PropertyContext::Margin);
    
    // Verify resolutions
    assert!((h1_margin_px - 21.44).abs() < 0.01, "H1 margin should be 21.44px, got {}", h1_margin_px);
    assert_eq!(p_margin_px, 16.0, "P margin should be 16.0px, got {}", p_margin_px);
    
    // Test collapsing
    let body_margin = 20.0;
    let body_h1_collapsed = collapse_margins(body_margin, h1_margin_px);
    assert!((body_h1_collapsed - 21.44).abs() < 0.01, 
        "Body (20px) and H1 (21.44px) should collapse to 21.44px, got {}", body_h1_collapsed);
    
    let h1_p_collapsed = collapse_margins(h1_margin_px, p_margin_px);
    assert!((h1_p_collapsed - 21.44).abs() < 0.01,
        "H1 (21.44px) and P (16px) should collapse to 21.44px, got {}", h1_p_collapsed);
}

#[test]
fn test_margin_em_uses_element_font_size() {
    // Verify that em in margins resolves against element's OWN font-size, not parent's
    
    let context = ResolutionContext {
        element_font_size: 32.0,  // Element's own font-size
        parent_font_size: 16.0,   // Parent's font-size
        root_font_size: 16.0,
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        dpi_scale: 1.0,
    };
    
    let margin = PixelValue::em(0.67);
    let resolved = margin.resolve_with_context(&context, PropertyContext::Margin);
    
    // Should use element_font_size (32.0), not parent_font_size (16.0)
    let expected = 0.67 * 32.0; // 21.44
    assert!((resolved - expected).abs() < 0.01,
        "Margin em should use element font-size (32px), not parent (16px). Expected {}, got {}", 
        expected, resolved);
}

#[test]
fn test_comparison_old_vs_new_behavior() {
    // This test verifies the bug fix: old hardcoded behavior vs new correct behavior
    
    let h1_font_size = 32.0;
    let margin_factor = 0.67;
    
    // Old (broken) behavior: hardcoded DEFAULT_FONT_SIZE = 16.0
    #[allow(deprecated)]
    let old_margin = {
        use azul_css::props::basic::DEFAULT_FONT_SIZE;
        margin_factor * DEFAULT_FONT_SIZE  // 10.72px
    };
    
    // New (correct) behavior: uses element's actual font-size
    let context = ResolutionContext {
        element_font_size: h1_font_size,
        parent_font_size: 16.0,
        root_font_size: 16.0,
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        dpi_scale: 1.0,
    };
    
    let margin = PixelValue::em(margin_factor);
    let new_margin = margin.resolve_with_context(&context, PropertyContext::Margin);
    
    // Verify old behavior was wrong
    assert!((old_margin - 10.72).abs() < 0.01, "Old behavior should give 10.72px");
    
    // Verify new behavior is correct
    assert!((new_margin - 21.44).abs() < 0.01, "New behavior should give 21.44px");
    
    // Verify they differ (bug is fixed)
    assert!(new_margin > old_margin,
        "New margin ({:.2}px) should be larger than old margin ({:.2}px)", 
        new_margin, old_margin);
    
    // Verify margin collapsing difference
    let body_margin = 20.0;
    let old_collapsed = collapse_margins(body_margin, old_margin);
    let new_collapsed = collapse_margins(body_margin, new_margin);
    
    assert_eq!(old_collapsed, 20.0, "Old: max(20, 10.72) should be 20");
    assert!((new_collapsed - 21.44).abs() < 0.01, "New: max(20, 21.44) should be 21.44");
}
