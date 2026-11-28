//! The system's text size in the layout: what `SystemStyle::preferences.text_scale` does to the
//! sizes of text, and what it must not do (scale an inherited size twice, scale a box that is
//! sized in px, change a window that has no preference).
//!
//! Neither phone applied the setting at all: the largest iOS size and the largest Android font
//! scale gave the same picture as the default (QA runs 46094960851, 46089359452: 0.046 % and 0 %
//! of the pixels differed).

use azul_core::{dom::Dom, styled_dom::StyledDom};
use azul_css::{
    accessibility::{AccessibilityPreferences, MAX_TEXT_SCALE},
    dynamic_selector::DynamicSelectorContext,
};

use super::*;
use crate::text3::cache::{LineHeight, Spacing};

/// The window the text is laid out in.
fn viewport() -> PhysicalSize {
    PhysicalSize::new(800.0, 600.0)
}

/// `<body>` with one `<div>` child, cascaded against `css`, in a window whose user chose
/// `scale` (`None`: a document no window adopted, `Some(1.0)`: a window without a preference).
fn document(css: &str, scale: Option<f32>) -> StyledDom {
    let mut dom = Dom::create_body().with_children(vec![Dom::create_div()].into());
    let mut styled = StyledDom::create(&mut dom, azul_css::parser2::new_from_str(css).0);
    if let Some(scale) = scale {
        styled.set_dynamic_selector_context(context_with_text_scale(scale));
    }
    styled
}

fn context_with_text_scale(scale: f32) -> DynamicSelectorContext {
    let mut preferences = AccessibilityPreferences::default();
    preferences.set_text_scale(scale);
    DynamicSelectorContext {
        preferences,
        ..DynamicSelectorContext::default()
    }
}

fn font_size(styled: &StyledDom, id: usize) -> f32 {
    let id = NodeId::new(id);
    get_element_font_size(styled, id, &styled.get_styled_node_state(&id))
}

/// A window the user gave no text size preference lays out as it always did.
#[test]
fn no_preference_changes_nothing() {
    let css = "body { font-size: 20px; } div { font-size: 1.5em; }";
    for scale in [None, Some(1.0)] {
        let styled = document(css, scale);
        assert_eq!(font_size(&styled, 0), 20.0, "{scale:?}");
        assert_eq!(font_size(&styled, 1), 30.0, "{scale:?}");
        assert!(!document_has_text_scale(&styled), "{scale:?}");
        assert_eq!(get_text_scale(&styled), 1.0, "{scale:?}");
    }
    // unset font sizes are the default one
    let styled = document("", Some(1.0));
    assert_eq!(font_size(&styled, 0), DEFAULT_FONT_SIZE);
}

/// The size the page wrote in px is multiplied, and the size that is inherited is the scaled
/// parent's: the child must not be scaled a second time.
#[test]
fn an_absolute_size_is_scaled_once_and_an_inherited_one_follows_its_parent() {
    let styled = document("body { font-size: 20px; }", Some(2.0));
    assert!(document_has_text_scale(&styled));
    assert_eq!(font_size(&styled, 0), 40.0, "20px at a scale of 2");
    assert_eq!(font_size(&styled, 1), 40.0, "inherited: the parent's scaled size, not 80");
    let child = NodeId::new(1);
    let state = styled.get_styled_node_state(&child);
    assert_eq!(get_parent_font_size(&styled, child, &state), 40.0);
    assert_eq!(get_root_font_size(&styled, &state), 40.0);
}

#[test]
fn em_and_rem_lengths_follow_the_scaled_font_sizes() {
    let styled = document(
        "body { font-size: 20px; } div { font-size: 1.5em; }",
        Some(2.0),
    );
    assert_eq!(font_size(&styled, 1), 60.0, "1.5em of the scaled 40px parent");

    let styled = document("body { font-size: 20px; } div { font-size: 2rem; }", Some(2.0));
    assert_eq!(font_size(&styled, 1), 80.0, "2rem of the scaled 40px root");

    // the em the layout resolves a length in (`padding-left: 1em`) is the scaled size
    let styled = document("body { font-size: 20px; } div { padding-left: 1em; }", Some(1.5));
    assert_eq!(font_size(&styled, 1), 30.0, "the em of the child: 20px at a scale of 1.5");
}

#[test]
fn the_default_size_scales_with_the_setting() {
    for (scale, expected) in [(0.85_f32, 13.6_f32), (1.3, 20.8), (2.0, 32.0)] {
        let styled = document("", Some(scale));
        let got = font_size(&styled, 0);
        assert!((got - expected).abs() < 0.01, "16px at {scale} is {expected}: got {got}");
    }
}

#[test]
fn a_scale_beyond_the_range_is_capped_and_never_crashes() {
    let styled = document("body { font-size: 10px; }", Some(1e9));
    assert_eq!(font_size(&styled, 0), 10.0 * MAX_TEXT_SCALE);
    let mut preferences = AccessibilityPreferences::default();
    preferences.text_scale = azul_css::OptionF32::Some(f32::NAN);
    let mut styled = document("body { font-size: 10px; }", None);
    styled.set_dynamic_selector_context(DynamicSelectorContext {
        preferences,
        ..DynamicSelectorContext::default()
    });
    assert_eq!(font_size(&styled, 0), 10.0, "a scale that is not a number is no scale");
}

/// The style the text shaper reads (`font_size_px`) goes through the same resolution as the
/// layout's em lengths: one size, in both.
#[test]
fn the_style_of_a_text_run_carries_the_scaled_size() {
    let styled = document("body { font-size: 20px; } div { font-size: 1.5em; }", Some(2.0));
    for (id, expected) in [(0_usize, 40.0_f32), (1, 60.0)] {
        let node = NodeId::new(id);
        let style = get_style_properties(&styled, node, None, viewport());
        assert_eq!(style.font_size_px, expected, "node {id}");
        assert_eq!(style.font_size_px, font_size(&styled, id), "node {id}: one resolution");
    }
    // and unscaled, the fast path answers as before
    let plain = document("body { font-size: 20px; } div { font-size: 1.5em; }", None);
    assert_eq!(get_style_properties(&plain, NodeId::new(1), None, viewport()).font_size_px, 30.0);
}

/// A line height in px, and the spacings of a run of text, are lengths of TEXT: they grow with
/// it, or two scaled lines would lie on top of each other.
#[test]
fn absolute_line_heights_and_spacings_scale_with_the_text() {
    let css = "div { line-height: 24px; letter-spacing: 1px; word-spacing: 3px; }";
    let styled = document(css, Some(2.0));
    let child = NodeId::new(1);
    let state = styled.get_styled_node_state(&child);
    let font = get_element_font_size(&styled, child, &state);
    match get_used_line_height(&styled, child, &state, font, viewport()) {
        LineHeight::Px(px) => assert!((px - 48.0).abs() < 1e-3, "24px at a scale of 2: {px}"),
        LineHeight::Normal => panic!("a declared line height is not `normal`"),
    }
    let style = get_style_properties(&styled, child, None, viewport());
    assert_eq!(style.letter_spacing, Spacing::PxF(2.0));
    assert_eq!(style.word_spacing, Spacing::PxF(6.0));

    // a number is times the (scaled) font size, `normal` stays `normal`
    let styled = document("div { line-height: 1.5; }", Some(2.0));
    let font = get_element_font_size(&styled, child, &state);
    match get_used_line_height(&styled, child, &state, font, viewport()) {
        LineHeight::Px(px) => assert!((px - 48.0).abs() < 1e-3, "1.5 of 32px: {px}"),
        LineHeight::Normal => panic!("a declared line height is not `normal`"),
    }
    let styled = document("", Some(2.0));
    assert!(matches!(
        get_used_line_height(&styled, child, &state, 32.0, viewport()),
        LineHeight::Normal
    ));
}

/// The system can change the setting while the window is open: the same document, the next
/// context, the next size (nothing the first one memoised answers for the second).
#[test]
fn a_changed_setting_changes_the_next_answer() {
    let mut styled = document("body { font-size: 20px; }", Some(1.0));
    assert_eq!(font_size(&styled, 0), 20.0);
    styled.set_dynamic_selector_context(context_with_text_scale(2.0));
    assert_eq!(font_size(&styled, 0), 40.0);
    styled.set_dynamic_selector_context(context_with_text_scale(1.3));
    assert!((font_size(&styled, 0) - 26.0).abs() < 1e-3);
    styled.set_dynamic_selector_context(context_with_text_scale(1.0));
    assert_eq!(font_size(&styled, 0), 20.0, "back to the default size");
}

/// The style cache of a layout pass keys on the cascade epoch, which a changed context moves: the
/// style of a node built at one text size is not served at another.
#[test]
fn the_style_cache_does_not_serve_a_style_of_another_text_size() {
    let mut styled = document("body { font-size: 20px; } div { font-size: 1.5em; }", Some(1.0));
    let mut cache = StyleCache::default();
    let child = NodeId::new(1);
    let at = |styled: &StyledDom, cache: &mut StyleCache| {
        get_style_properties_cached(cache, styled, child, None, viewport()).font_size_px
    };
    assert_eq!(at(&styled, &mut cache), 30.0);
    styled.set_dynamic_selector_context(context_with_text_scale(2.0));
    assert_eq!(at(&styled, &mut cache), 60.0, "the 1.0 style must not answer for the 2.0 window");
    styled.set_dynamic_selector_context(context_with_text_scale(1.0));
    assert_eq!(at(&styled, &mut cache), 30.0);
}
