//! `auto` is NOT a CSS-wide keyword (those are `initial`, `inherit`, `unset`
//! and `revert`). A bare `auto` on a property whose grammar lacks it is an
//! invalid value: the declaration is dropped and an earlier valid one stays.
//!
//! `parse_css_property` used to turn a bare `auto` into `CssProperty::auto`
//! for EVERY property without a typed `auto`, so an invalid
//! `spatial-navigation-function: auto` overrode an earlier valid declaration.

use azul_css::{
    css::{Css, CssDeclaration},
    props::property::{parse_css_property, CssProperty, CssPropertyType},
};

#[test]
fn spatial_navigation_function_auto_is_an_invalid_value() {
    let parsed = parse_css_property(CssPropertyType::SpatialNavigationFunction, "auto");
    assert!(
        parsed.is_err(),
        "the grammar is `normal | grid`, so a bare auto must be rejected, got {parsed:?}"
    );
}

#[test]
fn an_invalid_auto_does_not_override_an_earlier_valid_declaration() {
    let (css, warnings) = Css::from_string_with_warnings(
        ".nav { spatial-navigation-function: grid; spatial-navigation-function: auto; }"
            .to_string()
            .into(),
    );
    let values: Vec<String> = css
        .rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter())
        .filter_map(|decl| match decl {
            CssDeclaration::Static(prop)
                if prop.get_type() == CssPropertyType::SpatialNavigationFunction =>
            {
                Some(prop.value())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        values,
        vec!["grid".to_string()],
        "the invalid `auto` must be dropped; warnings: {warnings:?}"
    );
}

#[test]
fn properties_without_auto_in_their_grammar_reject_a_bare_auto() {
    for ty in [
        CssPropertyType::SpatialNavigationFunction,
        CssPropertyType::PaddingTop,
        CssPropertyType::MaxWidth,
        CssPropertyType::MaxHeight,
        CssPropertyType::Opacity,
        CssPropertyType::FontSize,
        CssPropertyType::LineHeight,
        CssPropertyType::LetterSpacing,
        CssPropertyType::BorderTopWidth,
        CssPropertyType::JustifyItems,
        CssPropertyType::AlignmentBaseline,
        CssPropertyType::LineFitEdge,
        CssPropertyType::ColumnSpan,
    ] {
        let parsed = parse_css_property(ty, "auto");
        assert!(
            parsed.is_err(),
            "`{}: auto` must be rejected, got {parsed:?}",
            ty.to_str()
        );
    }
}

/// Control: every property whose grammar DOES contain `auto` keeps reading a
/// bare `auto` exactly as before (the generic `CssProperty::auto`).
#[test]
fn properties_with_auto_in_their_grammar_keep_the_bare_auto() {
    for ty in [
        CssPropertyType::Width,
        CssPropertyType::Height,
        CssPropertyType::MinWidth,
        CssPropertyType::MinHeight,
        CssPropertyType::MarginTop,
        CssPropertyType::MarginLeft,
        CssPropertyType::Top,
        CssPropertyType::Left,
        CssPropertyType::ZIndex,
        CssPropertyType::FlexBasis,
        CssPropertyType::AlignSelf,
        CssPropertyType::JustifySelf,
        CssPropertyType::GridColumn,
        CssPropertyType::GridAutoRows,
        CssPropertyType::Cursor,
        CssPropertyType::CaretColor,
        CssPropertyType::BackgroundSize,
        CssPropertyType::ColumnCount,
        CssPropertyType::ColumnWidth,
        CssPropertyType::BreakBefore,
        CssPropertyType::TableLayout,
        CssPropertyType::ScrollbarColor,
        CssPropertyType::ScrollbarWidth,
        CssPropertyType::SpatialNavigationAction,
        CssPropertyType::SpatialNavigationContain,
    ] {
        assert_eq!(
            parse_css_property(ty, "auto").ok(),
            Some(CssProperty::auto(ty)),
            "`{}: auto` must stay the generic auto",
            ty.to_str()
        );
    }
}
