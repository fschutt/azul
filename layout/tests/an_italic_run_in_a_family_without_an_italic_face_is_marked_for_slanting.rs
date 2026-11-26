//! Text that asks for an italic or oblique style from a family that has no such face is drawn
//! slanted (synthetic italic, CSS Fonts 4 `font-synthesis-style: auto`, the initial value):
//! the run of the display list is marked (`FontHash::synthetic_italic`, as synthetic bold is) so
//! that the renderers slant the upright face, as Chrome does for a family with only a regular
//! cut. `font-synthesis-style: none` (and a `font-synthesis` that does not list `style`) turns it
//! off.
//!
//! The faces and the page are those of the text rig: `Azul Mock Mono` has a regular cut only.
//!
//! Not compiled by the author (house rule).

use azul_layout::text3::cache::{needs_synthetic_italic, FontHash};

use crate::text_rig::{block, lay_out, laid_out, painted, span, text, window, MONO};

/// The glyphs of one block of `ab` under `css`, as the display list paints them.
fn run(css: &str) -> Vec<bool> {
    painted(&laid_out(MONO, css, "ab", 400.0).1).iter().map(|g| g.synthetic_italic).collect()
}

#[test]
fn an_italic_or_oblique_style_asks_for_a_slant_of_a_face_that_has_none() {
    for css in ["font-style: italic", "font-style: oblique"] {
        assert_eq!(run(css), vec![true, true], "`{css}`: the regular face of the family is slanted");
    }
}

#[test]
fn upright_text_is_not_slanted() {
    for css in ["", "font-style: normal"] {
        assert_eq!(run(css), vec![false, false], "`{css}`");
    }
}

#[test]
fn font_synthesis_style_none_turns_the_slant_off() {
    assert_eq!(run("font-style: italic; font-synthesis-style: none"), vec![false, false]);
    assert_eq!(run("font-style: italic; font-synthesis-style: auto"), vec![true, true]);
    assert_eq!(run("font-style: italic; font-synthesis: none"), vec![false, false]);
    assert_eq!(run("font-style: italic; font-synthesis: weight"), vec![false, false], "style not listed");
    assert_eq!(run("font-style: italic; font-synthesis: style"), vec![true, true]);
}

/// Bold and italic are decided apart: a bold italic run is thickened and slanted, and each can be
/// refused.
#[test]
fn the_slant_and_the_thickening_are_decided_apart() {
    let flags = |css: &str| {
        let glyphs = painted(&laid_out(MONO, css, "ab", 400.0).1);
        (glyphs[0].synthetic_bold, glyphs[0].synthetic_italic)
    };
    assert_eq!(flags("font-weight: bold; font-style: italic"), (true, true));
    assert_eq!(flags("font-weight: bold"), (true, false));
    assert_eq!(flags("font-style: italic"), (false, true));
    assert_eq!(
        flags("font-weight: bold; font-style: italic; font-synthesis-style: none"),
        (true, false)
    );
    assert_eq!(
        flags("font-weight: bold; font-style: italic; font-synthesis-weight: none"),
        (false, true)
    );
}

/// A run is slanted or it is not: text of two styles in one block is two runs.
#[test]
fn only_the_run_that_asks_for_italic_is_slanted() {
    let mut lw = window();
    let dom = block(MONO, "", vec![text("ab"), span("font-style: italic", "cd"), text("ef")]);
    let dl = lay_out(&mut lw, dom, 400.0);
    let flags: Vec<bool> = painted(&dl).iter().map(|g| g.synthetic_italic).collect();
    assert_eq!(flags, vec![false, false, true, true, false, false]);
}

/// A face that has an italic design is drawn as it is: the slant is for faces that lack one, and
/// a face that does not say is never slanted.
#[test]
fn a_face_with_an_italic_design_or_one_that_does_not_say_is_not_slanted() {
    assert!(needs_synthetic_italic(true, Some(false)), "asked, and the face is upright");
    assert!(!needs_synthetic_italic(true, Some(true)), "the face is italic");
    assert!(!needs_synthetic_italic(true, None), "the face does not say");
    assert!(!needs_synthetic_italic(false, Some(false)), "not asked");
    let face = FontHash::from_hash(7).with_synthetic_italic(true).with_synthetic_bold(true);
    assert!(face.synthetic_italic && face.synthetic_bold, "both are kept: {face:?}");
    let face = face.with_synthetic_bold(false);
    assert!(face.synthetic_italic && !face.synthetic_bold, "{face:?}");
    assert!(!FontHash::from_hash(7).synthetic_italic && !FontHash::invalid().synthetic_italic);
}
