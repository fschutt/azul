//! The AzWidgets demo follows the desktop's theme.
//!
//! On a dark desktop the demo painted its page `#f2f4f7`, its section cards
//! white and its headings `#1d2939` - light colours with no dark counterpart -
//! around widgets that DO follow the theme, so a text field came out as a dark
//! box on a white card. The demo is the first thing anyone runs, and the page
//! an application copies its styles from.
//!
//! The demo is an example crate this one cannot link, so this reads its
//! SOURCE: every string literal that is an inline style is parsed the way
//! `Dom::with_css` parses it, and every colour it paints must either be a
//! `system:` keyword (which resolves in the theme it is rendered in) or come
//! with a counterpart under `@media (prefers-color-scheme: dark)` in the same
//! style.

use azul_css::{
    css::{Css, CssDeclaration},
    dynamic_selector::{DynamicSelector, ThemeCondition},
    props::{
        basic::color::SystemColorRef,
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
};

/// `examples/azul-widgets/src/lib.rs`, verbatim, at compile time.
const DEMO: &str = include_str!("../../examples/azul-widgets/src/lib.rs");

/// Every `"..."` literal in `src`, escapes decoded (`\` line continuations
/// included). Line comments are skipped; the demo writes no raw strings.
fn string_literals(src: &str) -> Vec<String> {
    let b: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // The one char literal that could open a phantom string.
        if b[i] == '\'' && b.get(i + 1) == Some(&'"') && b.get(i + 2) == Some(&'\'') {
            i += 3;
            continue;
        }
        if b[i] != '"' {
            i += 1;
            continue;
        }
        i += 1;
        let mut s = String::new();
        while i < b.len() && b[i] != '"' {
            if b[i] == '\\' {
                match b.get(i + 1) {
                    Some('\n') => {
                        i += 2;
                        while i < b.len() && b[i].is_whitespace() {
                            i += 1;
                        }
                        continue;
                    }
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(c) => s.push(*c),
                    None => {}
                }
                i += 2;
                continue;
            }
            s.push(b[i]);
            i += 1;
        }
        i += 1;
        out.push(s);
    }
    out
}

fn paints_a_colour(ty: CssPropertyType) -> bool {
    matches!(
        ty,
        CssPropertyType::TextColor
            | CssPropertyType::BackgroundContent
            | CssPropertyType::BorderTopColor
            | CssPropertyType::BorderRightColor
            | CssPropertyType::BorderBottomColor
            | CssPropertyType::BorderLeftColor
    )
}

/// A `system:` keyword resolves in the theme it is painted in, so it needs
/// no twin.
fn follows_the_theme(p: &CssProperty) -> bool {
    let token = |c: azul_css::props::basic::color::ColorU| {
        SystemColorRef::from_color_token(c).is_some()
    };
    match p {
        CssProperty::BackgroundContent(v) => v.get_property().is_some_and(|layers| {
            layers
                .as_ref()
                .iter()
                .all(|l| matches!(l, StyleBackgroundContent::SystemColor(_)))
        }),
        CssProperty::TextColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderTopColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderRightColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderBottomColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        CssProperty::BorderLeftColor(v) => v.get_property().is_some_and(|c| token(c.inner)),
        _ => false,
    }
}

/// The colour properties `style` paints in the light theme with a fixed
/// colour and never re-states for the dark one.
fn colours_without_a_dark_twin(style: &str) -> Vec<CssPropertyType> {
    let css = Css::parse_inline(style);
    let mut light = Vec::new();
    let mut dark = Vec::new();
    for rule in css.rules.as_ref() {
        let conditions = rule.conditions.as_ref();
        let is_dark = conditions.contains(&DynamicSelector::Theme(ThemeCondition::Dark));
        for d in rule.declarations.as_ref() {
            let CssDeclaration::Static(p) = d else {
                continue;
            };
            let ty = p.get_type();
            if !paints_a_colour(ty) {
                continue;
            }
            if is_dark {
                dark.push(ty);
            } else if conditions.is_empty() && !follows_the_theme(p) {
                light.push(ty);
            }
        }
    }
    light.retain(|t| !dark.contains(t));
    light
}

#[test]
fn every_colour_the_demo_paints_follows_the_theme() {
    let styles: Vec<String> = string_literals(DEMO)
        .into_iter()
        .filter(|l| l.contains(':') && l.contains(';'))
        .collect();
    assert!(
        styles.iter().any(|s| s.contains("flex-direction")),
        "premise: the scan found the demo's inline styles"
    );

    let bad: Vec<String> = styles
        .iter()
        .filter_map(|s| {
            let missing = colours_without_a_dark_twin(s);
            (!missing.is_empty()).then(|| {
                format!(
                    "{missing:?} in {:?}",
                    s.chars().take(72).collect::<String>()
                )
            })
        })
        .collect();
    assert!(
        bad.is_empty(),
        "{} demo style(s) paint a light-theme colour with no dark counterpart:\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}

/// A guard on the guard: the check must see a bare colour, and must accept
/// both ways a style can follow the theme.
#[test]
fn the_check_tells_a_themed_style_from_a_fixed_one() {
    assert_eq!(
        colours_without_a_dark_twin("color: #123456;"),
        vec![CssPropertyType::TextColor]
    );
    assert!(colours_without_a_dark_twin(
        "color: #123456; @media (prefers-color-scheme: dark) { color: system:text; }"
    )
    .is_empty());
    assert!(colours_without_a_dark_twin("color: system:text; width: 4px;").is_empty());
    assert_eq!(
        colours_without_a_dark_twin(
            "background-color: #fff; color: #000; @media (prefers-color-scheme: dark) { color: \
             system:text; }"
        ),
        vec![CssPropertyType::BackgroundContent],
        "a twin for one property does not excuse another"
    );
}
