//! A rule block's declarations keep their SOURCE order. The cascade takes the
//! last of two declarations that set the same longhand, so the order decides
//! which one wins whenever a shorthand and its longhand share a block.
//!
//! parser2 collects a block's declarations in a `BTreeMap` keyed by property
//! name (one value per property), and `css_blocks_to_stylesheet` emitted them
//! in the map's order - alphabetical. A shorthand sorts before its longhands
//! (`padding` < `padding-top`), so a longhand ALWAYS beat its shorthand:
//! `padding-top: 5px; padding: 0` kept the top padding at 5px.

use azul_css::{
    css::{Css, CssDeclaration},
    props::property::CssPropertyType,
};

/// Every declaration of the stylesheet, in emitted order, as (type, value).
fn declarations(css: &str) -> Vec<(CssPropertyType, String)> {
    let (css, _warnings) = Css::from_string_with_warnings(css.to_string().into());
    css.rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter())
        .filter_map(|decl| match decl {
            CssDeclaration::Static(prop) => Some((prop.get_type(), prop.value())),
            CssDeclaration::Dynamic(_) | CssDeclaration::CustomProperty(_) => None,
        })
        .collect()
}

/// The value the cascade ends up with: the LAST `padding-top` of the block.
fn effective_padding_top(css: &str) -> Option<String> {
    declarations(css)
        .into_iter()
        .filter(|(ty, _)| *ty == CssPropertyType::PaddingTop)
        .map(|(_, value)| value)
        .next_back()
}

#[test]
fn a_shorthand_after_its_longhand_overrides_it() {
    assert_eq!(
        effective_padding_top(".a { padding-top: 5px; padding: 0px; }"),
        effective_padding_top(".a { padding-top: 0px; }"),
        "`padding: 0` comes after `padding-top: 5px`, so the top padding is 0"
    );
}

#[test]
fn a_longhand_after_its_shorthand_overrides_it() {
    assert_eq!(
        effective_padding_top(".a { padding: 0px; padding-top: 5px; }"),
        effective_padding_top(".a { padding-top: 5px; }"),
    );
}

#[test]
fn unrelated_declarations_come_out_in_the_order_they_were_written() {
    let types: Vec<CssPropertyType> = declarations(".a { width: 10px; color: red; height: 5px; }")
        .into_iter()
        .map(|(ty, _)| ty)
        .collect();
    assert_eq!(
        types,
        vec![
            CssPropertyType::Width,
            CssPropertyType::TextColor,
            CssPropertyType::Height,
        ]
    );
}

#[test]
fn a_repeated_shorthand_counts_from_its_last_position() {
    // The block keeps ONE value per property: the second `padding` replaces
    // the first, and it stands AFTER the `padding-top` between them.
    assert_eq!(
        effective_padding_top(".a { padding: 1px; padding-top: 5px; padding: 2px; }"),
        effective_padding_top(".a { padding-top: 2px; }"),
    );
}

#[test]
fn an_invalid_repeat_keeps_the_earlier_value_at_its_earlier_position() {
    // `padding-top: bogus` is dropped; the valid `padding-top: 5px` stays
    // where it was written - BEFORE `padding: 0`.
    assert_eq!(
        effective_padding_top(".a { padding-top: 5px; padding: 0px; padding-top: bogus; }"),
        effective_padding_top(".a { padding-top: 0px; }"),
    );
}
