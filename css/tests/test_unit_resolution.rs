//! CSS Property Resolution Tests
//!
//! Tests for em, rem, percentage, and other CSS unit resolution.

use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};

#[test]
fn test_pixel_value_px() {
    let value = PixelValue::px(100.0);
    assert_eq!(value.metric, SizeMetric::Px);
    assert!((value.number.get() - 100.0).abs() < 0.001);
}

#[test]
fn test_pixel_value_em() {
    let value = PixelValue::em(2.0);
    assert_eq!(value.metric, SizeMetric::Em);
    assert!((value.number.get() - 2.0).abs() < 0.001);
}

#[test]
fn test_pixel_value_rem() {
    let value = PixelValue::rem(1.5);
    assert_eq!(value.metric, SizeMetric::Rem);
    assert!((value.number.get() - 1.5).abs() < 0.001);
}

#[test]
fn test_pixel_value_percent() {
    let value = PixelValue::percent(50.0);
    assert_eq!(value.metric, SizeMetric::Percent);
    assert!((value.number.get() - 50.0).abs() < 0.001);
}

#[test]
fn test_pixel_value_pt() {
    let value = PixelValue::pt(12.0);
    assert_eq!(value.metric, SizeMetric::Pt);
    assert!((value.number.get() - 12.0).abs() < 0.001);
}

#[test]
fn test_to_pixels_internal_px() {
    let value = PixelValue::px(100.0);
    let result = value.to_pixels_internal(50.0, 16.0);
    assert!((result - 100.0).abs() < 0.001, "Px should return raw value");
}

#[test]
fn test_to_pixels_internal_em() {
    let value = PixelValue::em(2.0);
    let result = value.to_pixels_internal(50.0, 16.0);
    // em uses em_resolve parameter
    assert!((result - 32.0).abs() < 0.001, "2em with 16px base = 32px");
}

#[test]
fn test_to_pixels_internal_percent() {
    let value = PixelValue::percent(50.0);
    let result = value.to_pixels_internal(200.0, 16.0);
    // percent uses percent_resolve parameter
    assert!((result - 100.0).abs() < 0.001, "50% of 200 = 100");
}

#[test]
fn test_to_pixels_internal_pt() {
    let value = PixelValue::pt(12.0);
    let result = value.to_pixels_internal(50.0, 16.0);
    // 1pt = 1.333... px (96/72)
    let expected = 12.0 * (96.0 / 72.0);
    assert!((result - expected).abs() < 0.1, "12pt should be ~16px");
}

#[test]
fn test_pixel_value_zero() {
    let value = PixelValue::px(0.0);
    let result = value.to_pixels_internal(100.0, 100.0);
    assert!((result - 0.0).abs() < 0.001);
}

#[test]
fn test_pixel_value_negative() {
    let value = PixelValue::px(-10.0);
    let result = value.to_pixels_internal(100.0, 100.0);
    assert!((result - (-10.0)).abs() < 0.001);
}

#[test]
fn test_pixel_value_large() {
    let value = PixelValue::px(10000.0);
    let result = value.to_pixels_internal(100.0, 100.0);
    assert!((result - 10000.0).abs() < 0.001);
}

#[test]
fn test_pixel_value_small_fraction() {
    let value = PixelValue::px(0.5);
    let result = value.to_pixels_internal(100.0, 100.0);
    assert!((result - 0.5).abs() < 0.001);
}

#[test]
fn test_em_chain_resolution() {
    // Simulate: root = 16px, parent = 2em (32px), child = 1.5em
    // Child's 1.5em should resolve against parent's 32px = 48px
    let parent_em = PixelValue::em(2.0);
    let parent_px = parent_em.to_pixels_internal(100.0, 16.0); // 2 * 16 = 32
    assert!((parent_px - 32.0).abs() < 0.001);

    let child_em = PixelValue::em(1.5);
    let child_px = child_em.to_pixels_internal(100.0, parent_px); // 1.5 * 32 = 48
    assert!((child_px - 48.0).abs() < 0.001);
}

#[test]
fn test_percent_100() {
    let value = PixelValue::percent(100.0);
    let result = value.to_pixels_internal(200.0, 16.0);
    assert!((result - 200.0).abs() < 0.001, "100% of 200 = 200");
}

#[test]
fn test_percent_over_100() {
    let value = PixelValue::percent(150.0);
    let result = value.to_pixels_internal(100.0, 16.0);
    assert!((result - 150.0).abs() < 0.001, "150% of 100 = 150");
}

#[test]
fn test_em_fractional() {
    let value = PixelValue::em(0.875); // Common value for smaller text
    let result = value.to_pixels_internal(100.0, 16.0);
    assert!(
        (result - 14.0).abs() < 0.001,
        "0.875em with 16px base = 14px"
    );
}

#[test]
fn test_inch_to_pixels() {
    // 1 inch = 96 CSS pixels
    let value = PixelValue::inch(1.0);
    let result = value.to_pixels_internal(100.0, 16.0);
    assert!((result - 96.0).abs() < 0.1, "1in = 96px");
}

#[test]
fn test_cm_to_pixels() {
    // 1 cm = 96/2.54 ≈ 37.8 CSS pixels
    let value = PixelValue::cm(1.0);
    let result = value.to_pixels_internal(100.0, 16.0);
    let expected = 96.0 / 2.54;
    assert!((result - expected).abs() < 0.1, "1cm ≈ 37.8px");
}

#[test]
fn test_mm_to_pixels() {
    // 1 mm = 96/25.4 ≈ 3.78 CSS pixels
    let value = PixelValue::mm(1.0);
    let result = value.to_pixels_internal(100.0, 16.0);
    let expected = 96.0 / 25.4;
    assert!((result - expected).abs() < 0.1, "1mm ≈ 3.78px");
}

#[test]
fn test_viewport_units_return_zero_without_context() {
    // Viewport units cannot be resolved without viewport context
    let vw = PixelValue::from_metric(SizeMetric::Vw, 100.0);
    let vh = PixelValue::from_metric(SizeMetric::Vh, 100.0);
    let vmin = PixelValue::from_metric(SizeMetric::Vmin, 100.0);
    let vmax = PixelValue::from_metric(SizeMetric::Vmax, 100.0);

    // to_pixels_internal returns 0 for viewport units (needs layout context)
    assert_eq!(vw.to_pixels_internal(100.0, 16.0), 0.0);
    assert_eq!(vh.to_pixels_internal(100.0, 16.0), 0.0);
    assert_eq!(vmin.to_pixels_internal(100.0, 16.0), 0.0);
    assert_eq!(vmax.to_pixels_internal(100.0, 16.0), 0.0);
}

#[test]
fn test_size_metric_equality() {
    assert_eq!(SizeMetric::Px, SizeMetric::Px);
    assert_eq!(SizeMetric::Em, SizeMetric::Em);
    assert_ne!(SizeMetric::Px, SizeMetric::Em);
}

#[test]
fn test_pixel_value_equality() {
    let a = PixelValue::px(10.0);
    let b = PixelValue::px(10.0);
    let c = PixelValue::px(20.0);
    let d = PixelValue::em(10.0);

    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_ne!(a, d);
}

#[test]
fn test_default_font_size_constant() {
    // CSS spec: default font-size is 16px
    let default_em = PixelValue::em(1.0);
    let result = default_em.to_pixels_internal(100.0, 16.0);
    assert!(
        (result - 16.0).abs() < 0.001,
        "1em with default 16px = 16px"
    );
}

#[test]
fn test_rem_uses_em_resolve() {
    // In to_pixels_internal, rem also uses em_resolve
    // (proper rem resolution requires root font-size context)
    let value = PixelValue::rem(2.0);
    let result = value.to_pixels_internal(100.0, 16.0);
    assert!(
        (result - 32.0).abs() < 0.001,
        "2rem with 16px em_resolve = 32px"
    );
}

#[test]
fn test_mixed_calculations() {
    // Simulate a realistic scenario:
    // Container: 500px wide
    // Padding: 5% = 25px
    // Font: 1.25em with 16px base = 20px

    let container_width = 500.0;
    let base_font = 16.0;

    let padding = PixelValue::percent(5.0);
    let padding_px = padding.to_pixels_internal(container_width, base_font);
    assert!((padding_px - 25.0).abs() < 0.001);

    let font = PixelValue::em(1.25);
    let font_px = font.to_pixels_internal(container_width, base_font);
    assert!((font_px - 20.0).abs() < 0.001);
}

// Edge cases for numerical precision
#[test]
fn test_very_small_em() {
    let value = PixelValue::em(0.001);
    let result = value.to_pixels_internal(100.0, 16.0);
    assert!((result - 0.016).abs() < 0.001);
}

#[test]
fn test_very_large_em() {
    let value = PixelValue::em(100.0);
    let result = value.to_pixels_internal(100.0, 16.0);
    assert!((result - 1600.0).abs() < 0.001);
}

#[test]
fn test_percent_with_zero_base() {
    let value = PixelValue::percent(50.0);
    let result = value.to_pixels_internal(0.0, 16.0);
    assert!((result - 0.0).abs() < 0.001, "50% of 0 = 0");
}

#[test]
fn test_em_with_zero_base() {
    let value = PixelValue::em(2.0);
    let result = value.to_pixels_internal(100.0, 0.0);
    assert!((result - 0.0).abs() < 0.001, "2em with 0px base = 0");
}
