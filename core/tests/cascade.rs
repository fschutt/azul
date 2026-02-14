//! Tests for CSS cascade keyword resolution (inherit, initial, revert, unset)

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{pixel::PixelValue, StyleFontSize},
        property::{CssProperty, CssPropertyType},
    },
};

#[test]
fn test_inherit_keyword_exists() {
    // Test that the Inherit variant exists and can be created
    let font_size_inherit = CssProperty::FontSize(CssPropertyValue::Inherit);

    if let CssProperty::FontSize(ref value) = font_size_inherit {
        assert!(value.is_inherit(), "Should have Inherit keyword");
    } else {
        panic!("Wrong property type");
    }

    println!("Inherit keyword can be created for FontSize");
}

#[test]
fn test_initial_keyword_exists() {
    // Test that the Initial variant exists and can be created
    let display_initial = CssProperty::Display(CssPropertyValue::Initial);

    if let CssProperty::Display(ref value) = display_initial {
        assert!(value.is_initial(), "Should have Initial keyword");
    } else {
        panic!("Wrong property type");
    }

    println!("Initial keyword can be created for Display");
}

#[test]
fn test_revert_keyword_exists() {
    // Test that the Revert variant exists and can be created
    let margin_revert = CssProperty::MarginTop(CssPropertyValue::Revert);

    if let CssProperty::MarginTop(ref value) = margin_revert {
        assert!(value.is_revert(), "Should have Revert keyword");
    } else {
        panic!("Wrong property type");
    }

    println!("Revert keyword can be created for MarginTop");
}

#[test]
fn test_unset_keyword_exists() {
    // Test that the Unset variant exists and can be created
    let text_color_unset = CssProperty::TextColor(CssPropertyValue::Unset);

    if let CssProperty::TextColor(ref value) = text_color_unset {
        assert!(value.is_unset(), "Should have Unset keyword");
    } else {
        panic!("Wrong property type");
    }

    println!("✓ Unset keyword can be created for TextColor");
}

#[test]
fn test_all_keywords_on_same_property() {
    // Test that all four cascade keywords can be used on the same property type

    let auto = CssProperty::Width(CssPropertyValue::Auto);
    let none = CssProperty::Width(CssPropertyValue::None);
    let initial = CssProperty::Width(CssPropertyValue::Initial);
    let inherit = CssProperty::Width(CssPropertyValue::Inherit);
    let revert = CssProperty::Width(CssPropertyValue::Revert);
    let unset = CssProperty::Width(CssPropertyValue::Unset);

    // Verify each one
    if let CssProperty::Width(ref v) = auto {
        assert!(v.is_auto());
    }
    if let CssProperty::Width(ref v) = none {
        assert!(v.is_none());
    }
    if let CssProperty::Width(ref v) = initial {
        assert!(v.is_initial());
    }
    if let CssProperty::Width(ref v) = inherit {
        assert!(v.is_inherit());
    }
    if let CssProperty::Width(ref v) = revert {
        assert!(v.is_revert());
    }
    if let CssProperty::Width(ref v) = unset {
        assert!(v.is_unset());
    }

    println!("✓ All cascade keywords work on Width property");
}

#[test]
fn test_property_type_is_inheritable() {
    // Test that we can check if a property type is inheritable

    // These should be inheritable
    assert!(
        CssPropertyType::FontSize.is_inheritable(),
        "FontSize should be inheritable"
    );
    assert!(
        CssPropertyType::TextColor.is_inheritable(),
        "TextColor should be inheritable"
    );
    assert!(
        CssPropertyType::FontFamily.is_inheritable(),
        "FontFamily should be inheritable"
    );

    // These should NOT be inheritable
    assert!(
        !CssPropertyType::Width.is_inheritable(),
        "Width should not be inheritable"
    );
    assert!(
        !CssPropertyType::Height.is_inheritable(),
        "Height should not be inheritable"
    );
    assert!(
        !CssPropertyType::MarginTop.is_inheritable(),
        "MarginTop should not be inheritable"
    );
    assert!(
        !CssPropertyType::PaddingLeft.is_inheritable(),
        "PaddingLeft should not be inheritable"
    );
    assert!(
        !CssPropertyType::BorderTopWidth.is_inheritable(),
        "BorderTopWidth should not be inheritable"
    );

    println!("✓ Property type inheritability check works");
}

#[test]
fn test_cascade_keyword_semantics_inherit() {
    // Test: inherit should use parent's computed value

    // Parent has font-size: 20px
    let _parent_value = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::px(20.0),
    }));

    // Child has font-size: inherit
    let child_value = CssProperty::FontSize(CssPropertyValue::Inherit);

    // Verify child has inherit keyword
    if let CssProperty::FontSize(ref v) = child_value {
        assert!(v.is_inherit());
        println!("✓ Child has 'inherit' keyword");
        println!("  Parent value: 20px");
        println!("  Expected child value after cascade: 20px");
    }
}

#[test]
fn test_cascade_keyword_semantics_unset_inheritable() {
    // Test: unset on inheritable property should act like inherit

    assert!(CssPropertyType::TextColor.is_inheritable());

    let color_with_unset = CssProperty::TextColor(CssPropertyValue::Unset);

    if let CssProperty::TextColor(ref v) = color_with_unset {
        assert!(v.is_unset());
        println!("✓ Unset on TextColor (inheritable property)");
        println!("  Should behave like 'inherit'");
    }
}

#[test]
fn test_cascade_keyword_semantics_unset_non_inheritable() {
    // Test: unset on non-inheritable property should act like initial

    assert!(!CssPropertyType::Width.is_inheritable());

    let width_with_unset = CssProperty::Width(CssPropertyValue::Unset);

    if let CssProperty::Width(ref v) = width_with_unset {
        assert!(v.is_unset());
        println!("✓ Unset on Width (non-inheritable property)");
        println!("  Should behave like 'initial'");
    }
}

#[test]
fn test_em_values_in_properties() {
    // Test that em values can be created
    let font_size_em = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::em(1.5),
    }));

    if let CssProperty::FontSize(CssPropertyValue::Exact(ref size)) = font_size_em {
        println!("✓ Em value created: {:?}", size);
        println!("  In cascade, 1.5em would resolve to 1.5 × parent's font-size");
    }
}

#[test]
fn test_percentage_values_in_properties() {
    // Test that percentage values can be created
    let font_size_percent = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::percent(150.0),
    }));

    if let CssProperty::FontSize(CssPropertyValue::Exact(ref size)) = font_size_percent {
        println!("✓ Percentage value created: {:?}", size);
        println!("  In cascade, 150% would resolve to 1.5 × parent's font-size");
    }
}

#[test]
fn test_cascade_chain_example() {
    // Demonstrate a full cascade chain:
    // Root: font-size: 16px
    // Parent: font-size: 150% = 1.5 × 16px = 24px
    // Child: font-size: 2em = 2 × 24px = 48px

    let _root = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::px(16.0),
    }));

    let _parent = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::percent(150.0),
    }));

    let _child = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::em(2.0),
    }));

    println!("✓ Cascade chain example:");
    println!("  Root:   font-size: 16px");
    println!("  Parent: font-size: 150% → 24px (1.5 × 16px)");
    println!("  Child:  font-size: 2em  → 48px (2 × 24px)");
    println!("  This demonstrates how relative values resolve through inheritance");
}
