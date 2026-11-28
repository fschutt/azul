//! The user's contrast and transparency settings in the painters' getters: what a text, a fill, a
//! border, a ring and a frosted surface paint when the setting is on, and that nothing moves when it
//! is not.

// a test builds the preferences one option at a time, as a user's settings screen does
#![allow(clippy::field_reassign_with_default)]

use azul_core::{
    dom::{Dom, IdOrClass, IdOrClassVec},
    styled_dom::StyledDom,
};
use azul_css::{
    css::CssPropertyValue,
    accessibility::{AccessibilityPreferences, ContrastPreference, SettingState},
    dynamic_selector::DynamicSelectorContext,
    presentation::{opaque_over, MORE_CONTRAST_EDGE, MORE_CONTRAST_TEXT},
    props::basic::color::ColorU,
};

use super::*;
use crate::solver3::getters::{
    get_background_color, get_background_contents, get_backdrop_filter, get_border_info,
    get_box_shadows, get_used_text_color,
};

const BODY: NodeId = NodeId::new(0);
const OUTER: NodeId = NodeId::new(1);
const INNER: NodeId = NodeId::new(2);

fn ids(name: &str) -> IdOrClassVec {
    IdOrClassVec::from_vec(vec![IdOrClass::Id(name.into())])
}

/// `<body><div id=a><div id=b>text</div></div></body>`, cascaded against `css`, in a window whose
/// user chose what `edit` says (`None`: a document no window adopted).
fn page(css: &str, edit: Option<&dyn Fn(&mut AccessibilityPreferences)>) -> StyledDom {
    let inner = Dom::create_div().with_ids_and_classes(ids("b"));
    let outer = Dom::create_div().with_ids_and_classes(ids("a")).with_children(vec![inner].into());
    let mut dom = Dom::create_body().with_children(vec![outer].into());
    let mut styled = StyledDom::create(&mut dom, azul_css::parser2::new_from_str(css).0);
    if let Some(edit) = edit {
        let mut preferences = AccessibilityPreferences::default();
        edit(&mut preferences);
        styled.set_dynamic_selector_context(DynamicSelectorContext {
            preferences,
            ..DynamicSelectorContext::default()
        });
    }
    styled
}

fn more_contrast(preferences: &mut AccessibilityPreferences) {
    preferences.contrast = ContrastPreference::More;
}

fn reduced_transparency(preferences: &mut AccessibilityPreferences) {
    preferences.reduced_transparency = SettingState::On;
}

fn state(styled: &StyledDom, node: NodeId) -> azul_core::styled_dom::StyledNodeState {
    styled.get_styled_node_state(&node)
}

fn ink(styled: &StyledDom, node: NodeId) -> ColorU {
    get_used_text_color(styled, node, &state(styled, node))
}

fn fill(styled: &StyledDom, node: NodeId) -> ColorU {
    get_background_color(styled, node, &state(styled, node))
}

fn border_top(styled: &StyledDom, node: NodeId) -> ColorU {
    match get_border_info(styled, node, &state(styled, node)).colors.top {
        Some(CssPropertyValue::Exact(color)) => color.inner,
        other => panic!("no border colour: {other:?}"),
    }
}

/// A window the user gave no setting, and one that is not asked, paint what the page wrote.
#[test]
fn without_a_setting_nothing_moves() {
    let css = "body { background: #808080; } #a { color: #999999; border: 2px solid #cccccc; \
               box-shadow: 0 0 0 2px #dddddd; background: rgba(255,255,255,0.5); backdrop-filter: blur(4px); }";
    for edit in [None, Some(&(|_: &mut AccessibilityPreferences| {}) as &dyn Fn(&mut AccessibilityPreferences))] {
        let styled = page(css, edit);
        assert!(!Presentation::of(&styled).is_active());
        assert_eq!(fill(&styled, BODY), ColorU { r: 0x80, g: 0x80, b: 0x80, a: 255 });
        assert_eq!(ink(&styled, INNER), ColorU { r: 0x99, g: 0x99, b: 0x99, a: 255 });
        assert_eq!(border_top(&styled, OUTER), ColorU { r: 0xcc, g: 0xcc, b: 0xcc, a: 255 });
        assert_eq!(get_box_shadows(&styled, OUTER, &state(&styled, OUTER))[0].color.r, 0xdd);
        assert!(fill(&styled, OUTER).a < 255, "translucent as written");
        assert!(get_backdrop_filter(&styled, OUTER, &state(&styled, OUTER)).is_some());
    }
}

/// A grey text on a mid grey fill, neither of which an extreme ink or a grey of that kind reaches 7:1
/// on: the fill moves to a side, the text is then read on it.
#[test]
fn increased_contrast_makes_a_text_readable_on_the_surface_behind_it() {
    let css = "body { background: #808080; } #a { color: #999999; }";
    let styled = page(css, Some(&more_contrast));
    assert!(Presentation::of(&styled).more_contrast);
    let surface = fill(&styled, BODY);
    assert_ne!(surface, ColorU { r: 0x80, g: 0x80, b: 0x80, a: 255 }, "a mid-tone fill moves");
    assert_eq!(surface_at(&styled, INNER), surface, "the text's surface is the body's, as painted");
    let read = ink(&styled, INNER);
    let ratio = read.contrast_ratio(&surface);
    assert!(ratio >= MORE_CONTRAST_TEXT, "{read:?} on {surface:?}: {ratio:.2}:1");
    // the same page without the setting is what it declared
    let plain = page(css, None);
    assert_eq!(ink(&plain, INNER), ColorU { r: 0x99, g: 0x99, b: 0x99, a: 255 });
}

/// A text that already reads at 7:1 does not move, and a `color: transparent` stays nothing.
#[test]
fn a_readable_text_and_a_hidden_one_stay() {
    let styled = page("body { background: #ffffff; } #a { color: #000000; } #b { color: transparent; }", Some(&more_contrast));
    assert_eq!(ink(&styled, OUTER), ColorU::BLACK);
    assert_eq!(ink(&styled, INNER).a, 0);
}

/// A border is seen at 4.5:1 on what lies behind the box; a transparent one is no border.
#[test]
fn a_border_is_seen_at_four_and_a_half_to_one_and_a_transparent_one_stays_transparent() {
    let css = "body { background: #ffffff; } #a { border: 2px solid #cccccc; } #b { border: 2px solid transparent; }";
    let styled = page(css, Some(&more_contrast));
    let edge = border_top(&styled, OUTER);
    let behind = surface_under(&styled, OUTER);
    assert!(edge.contrast_ratio(&behind) >= MORE_CONTRAST_EDGE, "{edge:?} on {behind:?}");
    assert_eq!(border_top(&styled, INNER).a, 0);
}

/// A ring (no blur, no offset, a spread: a focus halo) is an edge and must be seen; a soft drop shadow is
/// decoration and stays.
#[test]
fn a_ring_is_seen_and_a_soft_shadow_stays() {
    let css = "body { background: #ffffff; } #a { box-shadow: 0 0 0 2px #dddddd; } \
               #b { box-shadow: 0 2px 8px rgba(0,0,0,0.2); }";
    let styled = page(css, Some(&more_contrast));
    let ring = get_box_shadows(&styled, OUTER, &state(&styled, OUTER))[0];
    assert!(ring.color.contrast_ratio(&surface_under(&styled, OUTER)) >= MORE_CONTRAST_EDGE, "{ring:?}");
    let soft = get_box_shadows(&styled, INNER, &state(&styled, INNER))[0];
    let written = get_box_shadows(&page(css, None), INNER, &state(&styled, INNER))[0];
    assert_eq!(soft.color, written.color, "a soft shadow is decoration: as written");
}

/// A gradient is one flat colour a text can be read on - unless the subtree is exempt.
#[test]
fn a_gradient_is_flat_under_increased_contrast_and_an_exempt_subtree_keeps_its_colours() {
    let css = "#a { background: linear-gradient(#2060c0, #3070d0); } \
               #b { --azul-contrast-adjust: none; background: linear-gradient(#2060c0, #3070d0); color: #999999; }";
    let styled = page(css, Some(&more_contrast));
    let layers = get_background_contents(&styled, OUTER, &state(&styled, OUTER));
    assert_eq!(layers.len(), 1);
    assert!(matches!(layers[0], azul_css::props::style::StyleBackgroundContent::Color(_)), "{layers:?}");
    // the exempt node keeps its ramp and its ink
    let exempt_layers = get_background_contents(&styled, INNER, &state(&styled, INNER));
    assert!(matches!(exempt_layers[0], azul_css::props::style::StyleBackgroundContent::LinearGradient(_)));
    assert_eq!(ink(&styled, INNER), ColorU { r: 0x99, g: 0x99, b: 0x99, a: 255 });
}

/// A translucent fill is a tint, not frosted glass: increased contrast and reduced transparency leave it, and a
/// scrim stays a scrim.
#[test]
fn a_translucent_fill_without_a_backdrop_filter_stays_translucent() {
    let css = "body { background: #ffffff; } #a { background: rgba(0,0,0,0.5); }";
    let written = fill(&page(css, None), OUTER);
    assert!(written.a > 0 && written.a < 255);
    for edit in [&more_contrast as &dyn Fn(&mut AccessibilityPreferences), &reduced_transparency] {
        let styled = page(css, Some(edit));
        assert_eq!(fill(&styled, OUTER), written);
    }
}

/// Frosted glass - a translucent fill and a backdrop filter - is an opaque fill without a blur while
/// the user has the transparency effects off.
#[test]
fn a_frosted_surface_is_opaque_and_unblurred_while_transparency_is_reduced() {
    let css = "body { background: #2060c0; } #a { background: rgba(255,255,255,0.5); backdrop-filter: blur(10px); }";
    let plain = page(css, None);
    let declared = fill(&plain, OUTER);
    assert!(declared.a > 0 && declared.a < 255, "translucent as written: {declared:?}");
    assert!(get_backdrop_filter(&plain, OUTER, &state(&plain, OUTER)).is_some());

    let styled = page(css, Some(&reduced_transparency));
    assert!(Presentation::of(&styled).opaque);
    assert!(get_backdrop_filter(&styled, OUTER, &state(&styled, OUTER)).is_none(), "no blur");
    let seen = fill(&styled, OUTER);
    assert_eq!(seen, opaque_over(declared, ColorU { r: 0x20, g: 0x60, b: 0xc0, a: 255 }), "what it looked like");
    assert_eq!(seen.a, 255);
    // the page behind it is not a frosted surface: unchanged
    assert_eq!(fill(&styled, BODY), ColorU { r: 0x20, g: 0x60, b: 0xc0, a: 255 });
}
