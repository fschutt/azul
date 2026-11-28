//! The shell's first layout waits for the font registry's scan of the system only when the DOM needs a system font.
//!
//! On a font cache MISS (the first start after an install, an OS update or a changed font) that scan takes hundreds of
//! milliseconds on a desktop and seconds on a phone. `memory_faces_draw_all_text` is the rule the shell asks, with the
//! DOM it is about to lay out, whether it can go on: the faces registered in MEMORY (azul's own, the app's) must open
//! every font stack of the DOM's text and draw every character of it. Each way the rule could say yes wrongly is a test
//! here (a character no face draws, a stack that opens with a system family, a stack with no memory family at all, a
//! second stack, a DOM without text), next to the cases it must say yes to (the text covered by one face, by two faces
//! of one stack together, names after the memory families that do not change the answer, line breaks that are no
//! glyph, a face of the fallback tier - the tier azul's own and the app's faces are registered in).

use azul_core::{dom::Dom, styled_dom::StyledDom};
use azul_css::props::basic::PhysicalSize;
use azul_layout::{solver3::getters::memory_faces_draw_all_text, text3::cache::MemoryFontTier};
use rust_fontconfig::{FcFontCache, UnicodeRange};

use crate::variable_fonts::{page, red_hat_mono_path, Manager};

fn range(start: u32, end: u32) -> UnicodeRange {
    UnicodeRange { start, end }
}

/// Basic Latin, as a face would register it.
fn latin() -> Vec<UnicodeRange> {
    vec![range(0x20, 0x7e)]
}

/// A manager with each `(family, coverage)` registered from memory with exactly that coverage.
fn manager_with(faces: &[(&str, Vec<UnicodeRange>)]) -> Manager {
    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    for (family, coverage) in faces {
        let _ = manager.register_named_font(family, &file, coverage.clone());
    }
    manager
}

/// A manager with each `(family, coverage)` registered from memory in the FALLBACK tier, the tier of azul's own faces and
/// the app's (`AppConfig::add_font`): an installed copy of the family would win over it.
fn manager_with_fallback_faces(faces: &[(&str, Vec<UnicodeRange>)]) -> Manager {
    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    for (family, coverage) in faces {
        let _ = manager.register_named_font_in_tier(family, &file, coverage.clone(), MemoryFontTier::Fallback);
    }
    manager
}

fn draws_all(manager: &Manager, styled: &StyledDom) -> bool {
    memory_faces_draw_all_text(
        styled,
        manager,
        &azul_css::system::Platform::current(),
        PhysicalSize::new(800.0, 600.0),
    )
}

#[test]
fn text_set_in_a_registered_family_that_covers_it_needs_no_system_font() {
    let manager = manager_with(&[("Mem", latin())]);
    assert!(draws_all(&manager, &page("Mem", "", "", "Hello, world 123")));
}

#[test]
fn line_breaks_are_no_glyph_and_are_not_asked_for() {
    let manager = manager_with(&[("Mem", latin())]);
    assert!(draws_all(&manager, &page("Mem", "", "", "two\nlines\tand a tab")));
}

#[test]
fn one_character_no_registered_face_draws_makes_the_dom_wait() {
    let manager = manager_with(&[("Mem", latin())]);
    assert!(!draws_all(&manager, &page("Mem", "", "", "Hello 漢")));
    assert!(!draws_all(&manager, &page("Mem", "", "", "Hello \u{1f600}")));
    assert!(!draws_all(&manager, &page("Mem", "", "", "caf\u{e9}")), "a Latin-1 letter the face does not register");
}

#[test]
fn a_stack_that_names_no_registered_family_waits() {
    let manager = manager_with(&[("Mem", latin())]);
    assert!(!draws_all(&manager, &page("Some Installed Family", "", "", "Hello")));
}

#[test]
fn a_stack_that_opens_with_a_system_family_waits_whatever_follows() {
    let manager = manager_with(&[("Mem", latin())]);
    // The system face would draw the characters it has before the memory face after it.
    let styled = page("Mem", "", "font-family: \"Some System Font\", \"Mem\"", "Hello");
    assert!(!draws_all(&manager, &styled));
    let generic = page("Mem", "", "font-family: system-ui, \"Mem\"", "Hello");
    assert!(!draws_all(&manager, &generic), "a generic family is the system's too");
}

#[test]
fn names_after_the_registered_families_do_not_change_the_answer() {
    let manager = manager_with(&[("Mem", latin())]);
    // The design's own stack: its faces, then the serifs a desktop has.
    let styled = page(
        "Mem",
        "",
        "font-family: \"Mem\", \"Hoefler Text\", Georgia, serif",
        "Hello",
    );
    assert!(draws_all(&manager, &styled));
}

#[test]
fn a_face_of_the_fallback_tier_counts_like_one_of_the_primary_tier() {
    // azul's bundled faces and the app's own are registered in the fallback tier.
    let manager = manager_with_fallback_faces(&[("Mem", latin())]);
    assert!(draws_all(&manager, &page("Mem", "", "", "Hello, world 123")));
    assert!(!draws_all(&manager, &page("Mem", "", "", "Hello 漢")));
    let stack = page("Mem", "", "font-family: \"Mem\", Georgia, \"Times New Roman\", serif", "Hello");
    assert!(draws_all(&manager, &stack), "the names after the face are the system's and are not asked");
}

#[test]
fn the_registered_faces_of_one_stack_cover_the_text_together() {
    let manager = manager_with(&[("Latin", latin()), ("Greek", vec![range(0x370, 0x3ff)])]);
    let both = "font-family: \"Latin\", \"Greek\"";
    assert!(draws_all(&manager, &page("Latin", "", both, "a \u{3b1}")));
    assert!(!draws_all(&manager, &page("Latin", "", both, "a 漢")));
    // The second face is not in the stack: its characters are not drawn.
    assert!(!draws_all(&manager, &page("Latin", "", "", "a \u{3b1}")));
}

#[test]
fn every_stack_of_the_dom_must_be_drawn_by_registered_faces() {
    let manager = manager_with(&[("Mem", latin())]);
    let text = |s: &str| Dom::create_text_do_not_use_without_block_level_wrapper(s);
    let mut covered_only = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("font-family: \"Mem\"; font-size: 16px;")
            .with_child(text("one")),
    );
    let styled = StyledDom::create(&mut covered_only, azul_css::css::Css::empty());
    assert!(draws_all(&manager, &styled));

    let mut with_a_system_stack = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_css("font-family: \"Mem\"; font-size: 16px;")
                .with_child(text("one")),
        )
        .with_child(
            Dom::create_div()
                .with_css("font-family: \"Some Installed Family\"; font-size: 16px;")
                .with_child(text("two")),
        );
    let styled = StyledDom::create(&mut with_a_system_stack, azul_css::css::Css::empty());
    assert!(!draws_all(&manager, &styled), "the second stack needs the system");
}

#[test]
fn a_dom_without_text_waits_because_an_empty_field_still_takes_its_line_from_a_face() {
    let manager = manager_with(&[("Mem", latin())]);
    let mut empty = Dom::create_body();
    let styled = StyledDom::create(&mut empty, azul_css::css::Css::empty());
    assert!(!draws_all(&manager, &styled));
}

#[test]
fn without_registered_faces_everything_waits() {
    let manager = manager_with(&[]);
    assert!(!draws_all(&manager, &page("Mem", "", "", "Hello")));
}
