//! `font-synthesis` and its longhands decide what the engine fakes where a face lacks what the text
//! asks for: `font-synthesis-weight` the thickened bold of a face without a bold cut (in the
//! display list both rasterizers read, and in the glyph runs the PDF writer is given),
//! `font-synthesis-small-caps` the small capitals the shaper builds from scaled capitals.
//!
//! The properties parsed and were inherited, and nothing read them: `font-synthesis: none` drew
//! the faked bold and the faked small capitals all the same.
//!
//! The face is `Azul Mock Mono`: its weight class is 400 and it has no GSUB table, so a bold text
//! is thickened and `smcp` is missing. The glyph id of an ASCII character is `code point - 0x1F`.

use crate::text_rig::{block, export_page, laid_out, lay_out, painted, span, text, window, MONO};

/// Whether every glyph of `input` set in the mono face under `css` is drawn thickened.
fn thickened(css: &str, input: &str) -> bool {
    let (_lw, dl) = laid_out(MONO, css, input, 800.0);
    let glyphs = painted(&dl);
    assert!(!glyphs.is_empty(), "`{css}`: the text is painted");
    glyphs.iter().all(|g| g.synthetic_bold)
}

/// What the PDF writer is told of the same text (exported the way `printpdf` lays a document
/// out): whether its glyph runs are drawn thickened.
fn pdf_thickened(css: &str, input: &str) -> bool {
    let runs = export_page(MONO, css, input).pdf_runs();
    assert!(!runs.is_empty(), "`{css}`: the PDF writer is given the text");
    runs.iter().all(|run| run.synthetic_bold)
}

#[test]
fn a_bold_text_in_a_face_without_a_bold_cut_is_thickened_by_default() {
    assert!(thickened("font-weight: bold", "Hi"));
    assert!(thickened("font-weight: 600", "Hi"), "600 is a bold weight too");
    assert!(!thickened("font-weight: normal", "Hi"));
    assert!(!thickened("font-weight: 500", "Hi"));
    assert!(pdf_thickened("font-weight: bold", "Hi"));
    assert!(!pdf_thickened("", "Hi"));
}

#[test]
fn font_synthesis_weight_none_draws_the_face_as_it_is() {
    for css in [
        "font-weight: bold; font-synthesis-weight: none",
        "font-weight: bold; font-synthesis: none",
        "font-weight: bold; font-synthesis: style",
        "font-weight: bold; font-synthesis: style small-caps position",
    ] {
        assert!(!thickened(css, "Hi"), "`{css}`: display list");
        assert!(!pdf_thickened(css, "Hi"), "`{css}`: PDF run");
    }
    for css in [
        "font-weight: bold; font-synthesis-weight: auto",
        "font-weight: bold; font-synthesis: weight",
        "font-weight: bold; font-synthesis: weight style small-caps position",
        "font-weight: bold; font-synthesis: auto",
    ] {
        assert!(thickened(css, "Hi"), "`{css}`: display list");
        assert!(pdf_thickened(css, "Hi"), "`{css}`: PDF run");
    }
}

/// The switch is inherited; a box that says `auto` fakes again.
#[test]
fn the_weight_switch_is_inherited_and_a_child_can_turn_it_back_on() {
    let mut lw = window();
    let dom = block(
        MONO,
        "font-synthesis: none",
        vec![
            span("font-weight: bold", "ab"),
            span("font-weight: bold; font-synthesis-weight: auto", "cd"),
            text("ef"),
        ],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    let bold: Vec<bool> = painted(&dl).iter().map(|g| g.synthetic_bold).collect();
    assert_eq!(bold, [false, false, true, true, false, false]);
}

// ---------------------------------------------------------------------------------------------
// small capitals
// ---------------------------------------------------------------------------------------------

/// The glyph ids and the sizes `input` is painted at in the mono face under `css`.
fn shown(css: &str, input: &str) -> (Vec<u32>, Vec<f32>) {
    let (_lw, dl) = laid_out(MONO, css, input, 800.0);
    let glyphs = painted(&dl);
    (glyphs.iter().map(|g| g.id).collect(), glyphs.iter().map(|g| g.size).collect())
}

/// The premise: the mono face has no small capitals, so the shaper builds them (seven tenths of
/// the size, as Chrome does: see `small_capitals_are_synthesized_like_chrome.rs`).
#[test]
fn small_capitals_are_synthesized_by_default() {
    let (capitals, _) = shown("", "AB");
    let (ids, sizes) = shown("font-variant-caps: small-caps", "ab");
    assert_eq!(ids, capitals, "a and b are drawn as the capitals A and B");
    assert_eq!(sizes, [14.0, 14.0], "at seven tenths of the size");
}

#[test]
fn font_synthesis_small_caps_none_sets_the_text_as_it_is() {
    let (lower, _) = shown("", "ab");
    for css in [
        "font-variant-caps: small-caps; font-synthesis-small-caps: none",
        "font-variant-caps: small-caps; font-synthesis: none",
        "font-variant-caps: small-caps; font-synthesis: weight style position",
    ] {
        let (ids, sizes) = shown(css, "ab");
        assert_eq!(ids, lower, "`{css}`: the lowercase letters are drawn");
        assert_eq!(sizes, [20.0, 20.0], "`{css}`: at the full size");
    }
    for css in [
        "font-variant-caps: small-caps; font-synthesis-small-caps: auto",
        "font-variant-caps: small-caps; font-synthesis: small-caps",
        "font-variant-caps: small-caps; font-synthesis: weight style small-caps position",
    ] {
        assert_eq!(shown(css, "ab").1, [14.0, 14.0], "`{css}`: synthesized");
    }
}

/// `all-small-caps` is synthesized for every character, and the switch turns that off too.
#[test]
fn the_switch_turns_off_all_small_caps_and_petite_caps_as_well() {
    assert_eq!(shown("font-variant-caps: all-small-caps", "aB").1, [14.0, 14.0]);
    assert_eq!(
        shown("font-variant-caps: all-small-caps; font-synthesis-small-caps: none", "aB").1,
        [20.0, 20.0]
    );
    assert_eq!(shown("font-variant-caps: petite-caps", "ab").1, [14.0, 14.0], "petite falls back to small");
    assert_eq!(
        shown("font-variant-caps: petite-caps; font-synthesis: none", "ab").1,
        [20.0, 20.0]
    );
}

/// The two switches are independent: no faked bold, but the small capitals are still built.
#[test]
fn each_switch_only_turns_off_its_own_fake() {
    let css = "font-weight: bold; font-variant-caps: small-caps; font-synthesis-weight: none";
    assert!(!thickened(css, "ab"));
    assert_eq!(shown(css, "ab").1, [14.0, 14.0]);
    let css = "font-weight: bold; font-variant-caps: small-caps; font-synthesis-small-caps: none";
    assert!(thickened(css, "ab"));
    assert_eq!(shown(css, "ab").1, [20.0, 20.0]);
}
