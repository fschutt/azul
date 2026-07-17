
use azul_layout::solver3::sizing::*;

#[test]
fn test_resolve_percentage_with_box_model_basic() {
    // 100% of 595px with no margins/borders/paddings should be 595px
    let result = resolve_percentage_with_box_model(
        595.0,
        1.0, // 100%
        (0.0, 0.0),
        (0.0, 0.0),
        (0.0, 0.0),
    );
    assert_eq!(result, 595.0);
}

#[test]
fn test_resolve_percentage_with_box_model_with_margins() {
    // Body element: width: 100%, margin: 20px
    // Containing block (html): 595px wide
    // CSS spec: percentage resolves against containing block, NOT available space
    // Expected: 595px (margins are ignored for percentage resolution)
    let result = resolve_percentage_with_box_model(
        595.0,
        1.0, // 100%
        (20.0, 20.0),
        (0.0, 0.0),
        (0.0, 0.0),
    );
    assert_eq!(result, 595.0);
}

#[test]
fn test_resolve_percentage_with_box_model_with_all_box_properties() {
    // Element with margin: 10px, border: 5px, padding: 8px
    // width: 100% of 500px container
    // CSS spec: percentage resolves against containing block
    // Expected: 500px (margins/borders/padding are ignored)
    let result = resolve_percentage_with_box_model(
        500.0,
        1.0, // 100%
        (10.0, 10.0),
        (5.0, 5.0),
        (8.0, 8.0),
    );
    assert_eq!(result, 500.0);
}

#[test]
fn test_resolve_percentage_with_box_model_50_percent() {
    // 50% of 600px containing block
    // CSS spec: 50% of containing block = 300px
    // (margins don't affect percentage resolution)
    let result = resolve_percentage_with_box_model(
        600.0,
        0.5, // 50%
        (20.0, 20.0),
        (0.0, 0.0),
        (0.0, 0.0),
    );
    assert_eq!(result, 300.0);
}

#[test]
fn test_resolve_percentage_with_box_model_asymmetric() {
    // Asymmetric margins/borders/paddings
    // Container: 1000px
    // CSS spec: percentage resolves against containing block
    // 100% of 1000px = 1000px (margins/borders/padding ignored)
    let result = resolve_percentage_with_box_model(
        1000.0,
        1.0,
        (100.0, 50.0),
        (10.0, 20.0),
        (5.0, 15.0),
    );
    assert_eq!(result, 1000.0);
}

#[test]
fn test_resolve_percentage_with_box_model_negative_clamping() {
    // Edge case: margins larger than container
    // CSS spec: percentage still resolves against containing block
    // Result should still be 100px (100% of 100px)
    let result = resolve_percentage_with_box_model(
        100.0,
        1.0,
        (60.0, 60.0), // margins ignored for percentage resolution
        (0.0, 0.0),
        (0.0, 0.0),
    );
    assert_eq!(result, 100.0);
}

#[test]
fn test_resolve_percentage_with_box_model_zero_percent() {
    // 0% should always give 0, regardless of margins
    let result = resolve_percentage_with_box_model(
        1000.0,
        0.0, // 0%
        (100.0, 100.0),
        (10.0, 10.0),
        (5.0, 5.0),
    );
    assert_eq!(result, 0.0);
}
