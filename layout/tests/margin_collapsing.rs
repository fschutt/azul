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

use azul_layout::solver3::fc::collapse_margins;

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
