//! An `env()` AMONG the components of a shorthand: in `padding:
//! env(safe-area-inset-top, 4px) 8px` the top and bottom padding read the
//! safe-area inset and the left and right padding are 8px.
//!
//! The parser took any value that STARTED with `env(` for one `env()` call,
//! cut it at its LAST `)` and dropped every token after it: all four sides
//! read the inset and `8px` was lost, without a warning. A longhand lost its
//! trailing tokens the same way.

use azul_css::{
    css::Css,
    dynamic_selector::EnvVariable,
    props::property::{parse_css_property, CssProperty, CssPropertyType},
};

/// One declared longhand: its type, the `env()` variable it reads (`None`:
/// a static value) and its value without a context (the fallback).
type Longhand = (CssPropertyType, Option<EnvVariable>, CssProperty);

/// The longhands `.x { <declaration> }` declares, and how many warnings the
/// parse gave.
fn longhands(declaration: &str) -> (Vec<Longhand>, usize) {
    let (css, warnings) = Css::from_string_with_warnings(format!(".x {{ {declaration} }}").into());
    let declared = css
        .rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter())
        .map(|d| {
            (
                d.get_type().expect("a property declaration"),
                d.env_variable(),
                d.resolve_in_cascade(None)
                    .expect("a value without a context"),
            )
        })
        .collect();
    (declared, warnings.len())
}

/// The one declaration of `ty` among `got`: its variable and its fallback.
fn side(got: &[Longhand], ty: CssPropertyType) -> (Option<EnvVariable>, CssProperty) {
    let found: Vec<&Longhand> = got.iter().filter(|(t, ..)| *t == ty).collect();
    assert_eq!(found.len(), 1, "one {ty:?} declaration expected: {got:?}");
    (found[0].1, found[0].2.clone())
}

fn value(ty: CssPropertyType, css: &str) -> CssProperty {
    parse_css_property(ty, css).expect("premise: a valid longhand value")
}

#[test]
fn an_env_before_a_length_feeds_the_sides_its_position_names() {
    let (got, warnings) = longhands("padding: env(safe-area-inset-top, 4px) 8px;");
    assert_eq!(warnings, 0, "{got:?}");
    assert_eq!(got.len(), 4, "four sides: {got:?}");
    let top = CssPropertyType::PaddingTop;
    let bottom = CssPropertyType::PaddingBottom;
    let left = CssPropertyType::PaddingLeft;
    let right = CssPropertyType::PaddingRight;
    let inset = Some(EnvVariable::SafeAreaInsetTop);
    assert_eq!(side(&got, top), (inset, value(top, "4px")));
    assert_eq!(side(&got, bottom), (inset, value(bottom, "4px")));
    assert_eq!(
        side(&got, left),
        (None, value(left, "8px")),
        "the 8px after the env() must not be dropped"
    );
    assert_eq!(side(&got, right), (None, value(right, "8px")));
}

#[test]
fn each_side_reads_the_env_in_its_own_position() {
    let (got, warnings) = longhands(
        "margin: env(safe-area-inset-top, 1px) env(safe-area-inset-right, 2px) \
         env(safe-area-inset-bottom, 3px) env(safe-area-inset-left, 4px);",
    );
    assert_eq!(warnings, 0, "{got:?}");
    let cases = [
        (
            CssPropertyType::MarginTop,
            EnvVariable::SafeAreaInsetTop,
            "1px",
        ),
        (
            CssPropertyType::MarginRight,
            EnvVariable::SafeAreaInsetRight,
            "2px",
        ),
        (
            CssPropertyType::MarginBottom,
            EnvVariable::SafeAreaInsetBottom,
            "3px",
        ),
        (
            CssPropertyType::MarginLeft,
            EnvVariable::SafeAreaInsetLeft,
            "4px",
        ),
    ];
    for (ty, env, fallback) in cases {
        assert_eq!(side(&got, ty), (Some(env), value(ty, fallback)), "{ty:?}");
    }
}

#[test]
fn a_longhand_with_a_token_after_its_env_is_rejected_not_truncated() {
    let (got, warnings) = longhands("padding-top: env(safe-area-inset-top, 4px) 8px;");
    assert!(
        got.is_empty(),
        "`env(..) 8px` is no padding-top value; it must not be cut to the env(): {got:?}"
    );
    assert_eq!(warnings, 1, "and dropping it must be said");
}

/// An `env()` stands for a WHOLE longhand value (the cascade replaces the
/// value with the live length): inside a shadow it is refused, with a
/// warning, never half-applied.
#[test]
fn an_env_inside_one_component_of_a_compound_value_is_rejected() {
    let (got, warnings) = longhands("box-shadow: 0 env(safe-area-inset-top, 1px) red;");
    assert!(got.is_empty(), "{got:?}");
    assert_eq!(warnings, 1);
}
