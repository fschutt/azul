// Tests for CSS property → RelayoutScope classification.
//
// Verifies that CssPropertyType::relayout_scope() returns the correct
// RelayoutScope level for different property categories.

use azul_css::props::property::{CssPropertyType, RelayoutScope};

// =========================================================================
// FULL relayout properties
// =========================================================================

#[test]
fn display_is_full_relayout() {
    assert_eq!(CssPropertyType::Display.relayout_scope(true), RelayoutScope::Full);
    assert_eq!(CssPropertyType::Display.relayout_scope(false), RelayoutScope::Full);
}

#[test]
fn position_is_full_relayout() {
    assert_eq!(CssPropertyType::Position.relayout_scope(true), RelayoutScope::Full);
}

#[test]
fn float_is_full_relayout() {
    assert_eq!(CssPropertyType::Float.relayout_scope(true), RelayoutScope::Full);
}

#[test]
fn flex_direction_is_full_relayout() {
    assert_eq!(CssPropertyType::FlexDirection.relayout_scope(true), RelayoutScope::Full);
}

#[test]
fn overflow_x_is_full_relayout() {
    assert_eq!(CssPropertyType::OverflowX.relayout_scope(true), RelayoutScope::Full);
}

#[test]
fn overflow_y_is_full_relayout() {
    assert_eq!(CssPropertyType::OverflowY.relayout_scope(true), RelayoutScope::Full);
}

// =========================================================================
// SIZING ONLY properties (intrinsic size recompute)
// =========================================================================

#[test]
fn width_is_sizing_relayout() {
    let scope = CssPropertyType::Width.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly, "width should be at least SizingOnly, got {:?}", scope);
}

#[test]
fn height_is_sizing_relayout() {
    let scope = CssPropertyType::Height.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly, "height should be at least SizingOnly");
}

#[test]
fn min_width_is_sizing_relayout() {
    let scope = CssPropertyType::MinWidth.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly);
}

#[test]
fn max_width_is_sizing_relayout() {
    let scope = CssPropertyType::MaxWidth.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly);
}

#[test]
fn padding_top_is_sizing_relayout() {
    let scope = CssPropertyType::PaddingTop.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly,
        "padding should be at least SizingOnly, got {:?}", scope);
}

#[test]
fn margin_top_is_sizing_relayout() {
    let scope = CssPropertyType::MarginTop.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly);
}

#[test]
fn border_top_width_is_sizing_relayout() {
    let scope = CssPropertyType::BorderTopWidth.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly);
}

#[test]
fn flex_grow_is_sizing_relayout() {
    let scope = CssPropertyType::FlexGrow.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly);
}

#[test]
fn flex_shrink_is_sizing_relayout() {
    let scope = CssPropertyType::FlexShrink.relayout_scope(false);
    assert!(scope >= RelayoutScope::SizingOnly);
}

// =========================================================================
// IFC ONLY properties (text reshaping)
// =========================================================================

#[test]
fn font_size_is_ifc_only_for_ifc_member() {
    let scope = CssPropertyType::FontSize.relayout_scope(true);
    assert!(scope >= RelayoutScope::IfcOnly,
        "font-size should be at least IfcOnly for IFC member, got {:?}", scope);
}

#[test]
fn letter_spacing_is_ifc_only_for_ifc_member() {
    let scope = CssPropertyType::LetterSpacing.relayout_scope(true);
    assert!(scope >= RelayoutScope::IfcOnly,
        "letter-spacing should be at least IfcOnly, got {:?}", scope);
}

#[test]
fn word_spacing_is_ifc_only_for_ifc_member() {
    let scope = CssPropertyType::WordSpacing.relayout_scope(true);
    assert!(scope >= RelayoutScope::IfcOnly,
        "word-spacing should be at least IfcOnly, got {:?}", scope);
}

#[test]
fn line_height_is_ifc_only_for_ifc_member() {
    let scope = CssPropertyType::LineHeight.relayout_scope(true);
    assert!(scope >= RelayoutScope::IfcOnly,
        "line-height should be at least IfcOnly, got {:?}", scope);
}

#[test]
fn font_family_is_ifc_only_for_ifc_member() {
    let scope = CssPropertyType::FontFamily.relayout_scope(true);
    assert!(scope >= RelayoutScope::IfcOnly,
        "font-family should be at least IfcOnly, got {:?}", scope);
}

#[test]
fn text_align_is_ifc_only_for_ifc_member() {
    let scope = CssPropertyType::TextAlign.relayout_scope(true);
    assert!(scope >= RelayoutScope::IfcOnly,
        "text-align should be at least IfcOnly, got {:?}", scope);
}

// =========================================================================
// PAINT ONLY (None scope) — no layout needed
// =========================================================================

#[test]
fn text_color_is_none_scope() {
    assert_eq!(CssPropertyType::TextColor.relayout_scope(true), RelayoutScope::None);
    assert_eq!(CssPropertyType::TextColor.relayout_scope(false), RelayoutScope::None);
}

#[test]
fn opacity_is_none_scope() {
    assert_eq!(CssPropertyType::Opacity.relayout_scope(true), RelayoutScope::None);
}

#[test]
fn background_content_is_none_scope() {
    assert_eq!(CssPropertyType::BackgroundContent.relayout_scope(false), RelayoutScope::None);
}

#[test]
fn border_top_color_is_none_scope() {
    assert_eq!(CssPropertyType::BorderTopColor.relayout_scope(false), RelayoutScope::None);
}

#[test]
fn cursor_is_none_scope() {
    assert_eq!(CssPropertyType::Cursor.relayout_scope(false), RelayoutScope::None);
}

// =========================================================================
// IFC MEMBER FLAG: test that flag changes scope for text properties
// =========================================================================

#[test]
fn font_size_non_ifc_may_differ_from_ifc() {
    let scope_ifc = CssPropertyType::FontSize.relayout_scope(true);
    let scope_non_ifc = CssPropertyType::FontSize.relayout_scope(false);
    // IFC member should be at least IfcOnly for text reshaping
    assert!(scope_ifc >= RelayoutScope::IfcOnly);
    // Non-IFC member may have different scope (could be None if the
    // implementation only treats it as layout-affecting for IFC members)
    // Just verify it doesn't panic
    let _ = scope_non_ifc;
}

// =========================================================================
// ORDERING tests for RelayoutScope
// =========================================================================

#[test]
fn relayout_scope_ordering() {
    assert!(RelayoutScope::None < RelayoutScope::IfcOnly);
    assert!(RelayoutScope::IfcOnly < RelayoutScope::SizingOnly);
    assert!(RelayoutScope::SizingOnly < RelayoutScope::Full);
}

#[test]
fn relayout_scope_none_is_smallest() {
    assert_eq!(RelayoutScope::None, RelayoutScope::None);
    assert!(RelayoutScope::None < RelayoutScope::Full);
}

#[test]
fn relayout_scope_full_is_largest() {
    assert!(RelayoutScope::Full >= RelayoutScope::None);
    assert!(RelayoutScope::Full >= RelayoutScope::IfcOnly);
    assert!(RelayoutScope::Full >= RelayoutScope::SizingOnly);
}

// =========================================================================
// PROPERTY TYPE exhaustive tests: ensure no panics
// =========================================================================

#[test]
fn all_property_types_have_relayout_scope() {
    // Verify that relayout_scope() can be called on all property types
    // without panicking (exhaustive match coverage)
    let types = [
        CssPropertyType::TextColor,
        CssPropertyType::FontSize,
        CssPropertyType::FontFamily,
        CssPropertyType::TextAlign,
        CssPropertyType::LetterSpacing,
        CssPropertyType::LineHeight,
        CssPropertyType::WordSpacing,
        CssPropertyType::Cursor,
        CssPropertyType::Display,
        CssPropertyType::Float,
        CssPropertyType::Position,
        CssPropertyType::Top,
        CssPropertyType::Right,
        CssPropertyType::Bottom,
        CssPropertyType::Left,
        CssPropertyType::Width,
        CssPropertyType::Height,
        CssPropertyType::MinWidth,
        CssPropertyType::MinHeight,
        CssPropertyType::MaxWidth,
        CssPropertyType::MaxHeight,
        CssPropertyType::FlexDirection,
        CssPropertyType::FlexWrap,
        CssPropertyType::FlexGrow,
        CssPropertyType::FlexShrink,
        CssPropertyType::JustifyContent,
        CssPropertyType::AlignItems,
        CssPropertyType::AlignContent,
        CssPropertyType::OverflowX,
        CssPropertyType::OverflowY,
        CssPropertyType::PaddingTop,
        CssPropertyType::PaddingRight,
        CssPropertyType::PaddingBottom,
        CssPropertyType::PaddingLeft,
        CssPropertyType::MarginTop,
        CssPropertyType::MarginRight,
        CssPropertyType::MarginBottom,
        CssPropertyType::MarginLeft,
        CssPropertyType::BorderTopWidth,
        CssPropertyType::BorderRightWidth,
        CssPropertyType::BorderBottomWidth,
        CssPropertyType::BorderLeftWidth,
        CssPropertyType::BorderTopColor,
        CssPropertyType::BorderRightColor,
        CssPropertyType::BorderBottomColor,
        CssPropertyType::BorderLeftColor,
        CssPropertyType::BorderTopStyle,
        CssPropertyType::BorderRightStyle,
        CssPropertyType::BorderBottomStyle,
        CssPropertyType::BorderLeftStyle,
        CssPropertyType::Opacity,
        CssPropertyType::BackgroundContent,
        CssPropertyType::BackgroundPosition,
        CssPropertyType::BackgroundSize,
        CssPropertyType::BackgroundRepeat,
    ];

    for prop in &types {
        let _scope_ifc = prop.relayout_scope(true);
        let _scope_non_ifc = prop.relayout_scope(false);
    }
}

// =========================================================================
// INTEGRATION: classify_change_scope in ChangeAccumulator
// =========================================================================

#[test]
fn change_accumulator_uses_correct_scope_for_text() {
    use azul_core::diff::ChangeAccumulator;
    use azul_core::id::NodeId;

    let mut acc = ChangeAccumulator::new();
    acc.add_text_change(NodeId::new(0), "old".into(), "new".into());
    // Text changes are always IfcOnly
    assert_eq!(acc.max_scope, RelayoutScope::IfcOnly);
}

#[test]
fn change_accumulator_uses_correct_scope_for_width() {
    use azul_core::diff::ChangeAccumulator;
    use azul_core::id::NodeId;

    let mut acc = ChangeAccumulator::new();
    let width_scope = CssPropertyType::Width.relayout_scope(false);
    acc.add_css_change(NodeId::new(0), CssPropertyType::Width, width_scope);
    assert!(acc.max_scope >= RelayoutScope::SizingOnly);
}

#[test]
fn change_accumulator_uses_correct_scope_for_display() {
    use azul_core::diff::ChangeAccumulator;
    use azul_core::id::NodeId;

    let mut acc = ChangeAccumulator::new();
    let display_scope = CssPropertyType::Display.relayout_scope(false);
    acc.add_css_change(NodeId::new(0), CssPropertyType::Display, display_scope);
    assert_eq!(acc.max_scope, RelayoutScope::Full);
}
