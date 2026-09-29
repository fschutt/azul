//! The custom-property resolver the cascade consults (design
//! `RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` §7.3: "one resolver,
//! consulted by both cascades").
//!
//! Two halves:
//! - `CustomPropertyMap::cascade`: the variables ONE node sees - its parent's, overlaid with the
//!   node's own definitions. A definition's `var()` references are substituted where the
//!   definition IS (CSS computed-value semantics), so a child inherits the substituted text.
//! - `resolve_var`: the value a `var()` consumer takes in such a map.

use azul_css::{
    css::DynamicCssProperty,
    custom_properties::{resolve_var, CustomPropertyMap},
    props::property::{parse_css_property, CssProperty, CssPropertyType},
};

fn map(own: &[(&str, &str)]) -> CustomPropertyMap {
    CustomPropertyMap::cascade(&CustomPropertyMap::default(), own)
}

fn reference(names: &str, ty: CssPropertyType, fallback: &str) -> DynamicCssProperty {
    DynamicCssProperty {
        dynamic_id: names.into(),
        default_value: parse_css_property(ty, fallback).expect("fallback parses"),
    }
}

fn parsed(ty: CssPropertyType, value: &str) -> CssProperty {
    parse_css_property(ty, value).expect("parses")
}

// ------------------------------------------------------------ the node map

#[test]
fn own_definitions_override_inherited_ones_and_the_last_one_wins() {
    let root = map(&[("fg", "red"), ("bg", "white")]);
    let panel = CustomPropertyMap::cascade(&root, &[("fg", "green"), ("fg", "blue")]);
    assert_eq!(panel.get("fg"), Some("blue"));
    assert_eq!(panel.get("bg"), Some("white"), "not redefined: inherited");
    assert_eq!(root.get("fg"), Some("red"), "the parent's map is untouched");
}

#[test]
fn a_definition_reads_an_inherited_variable_where_it_is_defined() {
    let root = map(&[("base", "#0000ff")]);
    let node = CustomPropertyMap::cascade(&root, &[("fg", "var(--base)")]);
    assert_eq!(node.get("fg"), Some("#0000ff"));
    // A child that redefines `--base` does NOT change the inherited `--fg`:
    // it was substituted at the node that defined it.
    let child = CustomPropertyMap::cascade(&node, &[("base", "#ff0000")]);
    assert_eq!(child.get("fg"), Some("#0000ff"));
}

#[test]
fn definitions_in_one_block_may_read_each_other_in_any_order() {
    let m = map(&[("fg", "var(--base)"), ("base", "#0000ff")]);
    assert_eq!(m.get("fg"), Some("#0000ff"));
}

#[test]
fn a_value_keeps_its_other_tokens_around_a_substitution() {
    let m = map(&[("c", "#000000"), ("shadow", "0 0 4px var(--c)")]);
    assert_eq!(m.get("shadow"), Some("0 0 4px #000000"));
}

#[test]
fn a_cycle_makes_every_name_in_it_invalid_and_hides_the_inherited_value() {
    let root = map(&[("a", "red"), ("b", "blue")]);
    let node = CustomPropertyMap::cascade(&root, &[("a", "var(--b)"), ("b", "var(--a)")]);
    // CSS: a custom property in a cycle is guaranteed-invalid at that node -
    // it does NOT fall back to the parent's value.
    assert_eq!(node.get("a"), None);
    assert_eq!(node.get("b"), None);
}

#[test]
fn a_self_reference_is_a_cycle() {
    let m = map(&[("a", "var(--a)")]);
    assert_eq!(m.get("a"), None);
    let m = map(&[("a", "var(--a, red)")]);
    assert_eq!(m.get("a"), None, "a fallback does not break a cycle");
}

#[test]
fn a_fallback_inside_a_definition_is_used_when_its_variable_is_missing() {
    let m = map(&[("fg", "var(--nope, var(--gone, #00ff00))")]);
    assert_eq!(m.get("fg"), Some("#00ff00"));
    let m = map(&[("fg", "var(--nope)")]);
    assert_eq!(m.get("fg"), None, "missing and no fallback: invalid");
}

#[test]
fn an_exponential_definition_is_cut_off_instead_of_hanging() {
    // Each level doubles the text: 2^40 bytes if nothing stops it.
    let mut own: Vec<(String, String)> = vec![("l0".into(), "xxxxxxxx".into())];
    for i in 1..40 {
        own.push((format!("l{i}"), format!("var(--l{}) var(--l{})", i - 1, i - 1)));
    }
    let own_ref: Vec<(&str, &str)> = own.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let m = map(&own_ref);
    assert_eq!(m.get("l0"), Some("xxxxxxxx"));
    assert_eq!(m.get("l39"), None, "past the size cap the value is invalid");
}

// -------------------------------------------------------------- consumers

#[test]
fn a_reference_parses_its_variable_as_the_property_type() {
    let m = map(&[("w", "150px"), ("fg", "#0000ff")]);
    let w = CssPropertyType::Width;
    let c = CssPropertyType::TextColor;
    assert_eq!(resolve_var(&reference("w", w, "10px"), &m), parsed(w, "150px"));
    assert_eq!(resolve_var(&reference("fg", c, "#ff0000"), &m), parsed(c, "#0000ff"));
}

#[test]
fn a_missing_or_unparseable_variable_falls_through_the_chain_to_the_fallback() {
    let w = CssPropertyType::Width;
    let m = map(&[("b", "20px"), ("junk", "not-a-length")]);
    assert_eq!(resolve_var(&reference("a,b", w, "10px"), &m), parsed(w, "20px"));
    assert_eq!(resolve_var(&reference("junk,b", w, "10px"), &m), parsed(w, "20px"));
    assert_eq!(resolve_var(&reference("a,c", w, "10px"), &m), parsed(w, "10px"));
    assert_eq!(
        resolve_var(&reference("a", w, "10px"), &CustomPropertyMap::default()),
        parsed(w, "10px")
    );
}
