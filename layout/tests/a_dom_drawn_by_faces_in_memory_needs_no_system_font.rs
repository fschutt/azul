//! The layout waits for the font registry's scan of the system only where it needs a face from it.
//!
//! On a font cache MISS (the first start after an install, an OS update or a changed font) that scan takes hundreds of
//! milliseconds on a desktop and seconds on a phone. The host gives the font manager a gate (`SystemFontsGate`); the layout
//! asks it - where its chains are resolved - for the stacks the faces registered in MEMORY (azul's own, the app's) do not
//! draw: a stack must open with faces in memory that draw every character of the text. Each way the rule could say "no
//! need" wrongly is a test here (a character no face draws, a stack that opens with a system family, a stack with no memory
//! family at all, a second stack, a DOM without text, a child DOM, text that comes later), next to the cases it must say
//! "no need" to (the text covered by one face, by two faces of one stack together, names after the memory families that do
//! not change the answer, line breaks that are no glyph, a face of the fallback tier - the tier azul's own and the app's
//! faces are registered in).
//!
//! The gate here waits for nothing and writes down what it was asked for.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use azul_core::{dom::Dom, styled_dom::StyledDom};
use azul_css::props::basic::PhysicalSize;
use azul_layout::{
    solver3::getters::collect_and_resolve_font_chains_covering,
    text3::cache::{
        missing_coverage_faces, FontChainKey, FontSelector, FontStack, InlineContent, MemoryFontTier,
        StyleProperties, StyledRun, SystemFontsGate, SystemFontsWait,
    },
};
use rust_fontconfig::{FcFontCache, FontFallbackChain, UnicodeRange};

use crate::variable_fonts::{page, static_face_path, Manager};

/// What a gate was asked: the family lists and the reason.
type Asked = Vec<(Vec<Vec<String>>, String)>;

/// A gate that waits for nothing and writes down what it was asked for.
#[derive(Debug)]
struct Recorder {
    pending: bool,
    asked: Mutex<Asked>,
}

impl Recorder {
    fn new(pending: bool) -> Arc<Self> {
        Arc::new(Self {
            pending,
            asked: Mutex::new(Vec::new()),
        })
    }

    fn asked(&self) -> Asked {
        self.asked.lock().expect("the record").clone()
    }
}

impl SystemFontsGate for Recorder {
    fn pending(&self) -> bool {
        self.pending
    }

    fn wait_for(&self, stacks: &[Vec<String>], why: &str) {
        self.asked
            .lock()
            .expect("the record")
            .push((stacks.to_vec(), why.to_string()));
    }

    fn take_waits(&self) -> Vec<SystemFontsWait> {
        Vec::new()
    }
}

fn range(start: u32, end: u32) -> UnicodeRange {
    UnicodeRange { start, end }
}

/// Basic Latin, as a face would register it.
fn latin() -> Vec<UnicodeRange> {
    vec![range(0x20, 0x7e)]
}

/// A manager with each `(family, coverage)` registered from memory in `tier` with exactly that coverage, asking `gate`.
/// The face is a static one (a variable face would be drawn as an instance made on the way).
fn manager_with(
    faces: &[(&str, Vec<UnicodeRange>)],
    tier: MemoryFontTier,
    gate: &Arc<Recorder>,
) -> Manager {
    let file = std::fs::read(static_face_path()).expect("the face is checked in");
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    for (family, coverage) in faces {
        let _ = manager.register_named_font_in_tier(family, &file, coverage.clone(), tier);
    }
    let gate: Arc<dyn SystemFontsGate> = gate.clone();
    manager.system_fonts_gate = Some(gate);
    manager
}

/// Resolves the chains of `styled` as the layout does, with the characters `also` kept covered.
fn lay_out(manager: &Manager, styled: &StyledDom, also: &BTreeSet<char>) {
    let _ = collect_and_resolve_font_chains_covering(
        styled,
        &manager.fc_cache,
        manager,
        &azul_css::system::Platform::current(),
        PhysicalSize::new(800.0, 600.0),
        also,
    );
}

fn asks(faces: &[(&str, Vec<UnicodeRange>)], tier: MemoryFontTier, styled: &StyledDom) -> Asked {
    let gate = Recorder::new(true);
    let manager = manager_with(faces, tier, &gate);
    lay_out(&manager, styled, &BTreeSet::new());
    gate.asked()
}

fn primary_asks(faces: &[(&str, Vec<UnicodeRange>)], styled: &StyledDom) -> Asked {
    asks(faces, MemoryFontTier::Primary, styled)
}

#[test]
fn text_set_in_a_registered_family_that_covers_it_needs_no_system_font() {
    let asked = primary_asks(&[("Mem", latin())], &page("Mem", "", "", "Hello, world 123"));
    assert!(asked.is_empty(), "asked {asked:?}");
}

#[test]
fn line_breaks_are_no_glyph_and_are_not_asked_for() {
    let asked = primary_asks(&[("Mem", latin())], &page("Mem", "", "", "two\nlines\tand a tab"));
    assert!(asked.is_empty(), "asked {asked:?}");
}

#[test]
fn one_character_no_registered_face_draws_makes_the_dom_ask_and_the_reason_names_it() {
    for (text, code) in [("Hello 漢", "U+6F22"), ("Hello \u{1f600}", "U+1F600"), ("caf\u{e9}", "U+00E9")] {
        let asked = primary_asks(&[("Mem", latin())], &page("Mem", "", "", text));
        assert_eq!(asked.len(), 1, "{text:?}: asked {asked:?}");
        assert!(asked[0].1.contains(code), "{text:?}: the reason is {:?}", asked[0].1);
        assert!(asked[0].1.contains("\"Mem\""), "{text:?}: the reason is {:?}", asked[0].1);
    }
}

#[test]
fn a_stack_that_names_no_registered_family_asks_for_its_families() {
    let asked = primary_asks(&[("Mem", latin())], &page("Some Installed Family", "", "", "Hello"));
    assert_eq!(asked.len(), 1, "asked {asked:?}");
    assert!(
        asked[0].0.iter().any(|stack| stack.first().map(String::as_str) == Some("Some Installed Family")),
        "asked for {:?}",
        asked[0].0
    );
    assert!(
        asked[0].1.contains("opens with a system family"),
        "the reason is {:?}",
        asked[0].1
    );
}

#[test]
fn a_stack_that_opens_with_a_system_family_asks_whatever_follows() {
    // The system face would draw the characters it has before the memory face after it.
    let styled = page("Mem", "", "font-family: \"Some System Font\", \"Mem\"", "Hello");
    assert_eq!(primary_asks(&[("Mem", latin())], &styled).len(), 1);
    let generic = page("Mem", "", "font-family: system-ui, \"Mem\"", "Hello");
    assert_eq!(primary_asks(&[("Mem", latin())], &generic).len(), 1, "a generic family is the system's too");
}

#[test]
fn names_after_the_registered_families_do_not_change_the_answer() {
    // The design's own stack: its faces, then the serifs a desktop has.
    let styled = page(
        "Mem",
        "",
        "font-family: \"Mem\", \"Hoefler Text\", Georgia, serif",
        "Hello",
    );
    let asked = primary_asks(&[("Mem", latin())], &styled);
    assert!(asked.is_empty(), "asked {asked:?}");
}

#[test]
fn a_face_of_the_fallback_tier_counts_like_one_of_the_primary_tier() {
    // azul's bundled faces and the app's own are registered in the fallback tier.
    let faces = [("Mem", latin())];
    let tier = MemoryFontTier::Fallback;
    assert!(asks(&faces, tier, &page("Mem", "", "", "Hello, world 123")).is_empty());
    assert_eq!(asks(&faces, tier, &page("Mem", "", "", "Hello 漢")).len(), 1);
    let stack = page("Mem", "", "font-family: \"Mem\", Georgia, \"Times New Roman\", serif", "Hello");
    assert!(asks(&faces, tier, &stack).is_empty(), "the names after the face are the system's and are not asked");
}

#[test]
fn the_registered_faces_of_one_stack_cover_the_text_together() {
    let faces = [("Latin", latin()), ("Greek", vec![range(0x370, 0x3ff)])];
    let both = "font-family: \"Latin\", \"Greek\"";
    assert!(primary_asks(&faces, &page("Latin", "", both, "a \u{3b1}")).is_empty());
    assert_eq!(primary_asks(&faces, &page("Latin", "", both, "a 漢")).len(), 1);
    // The second face is not in the stack: its characters are not drawn.
    assert_eq!(primary_asks(&faces, &page("Latin", "", "", "a \u{3b1}")).len(), 1);
}

#[test]
fn every_stack_of_the_dom_must_be_drawn_by_registered_faces() {
    let text = |s: &str| Dom::create_text_do_not_use_without_block_level_wrapper(s);
    let mut covered_only = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("font-family: \"Mem\"; font-size: 16px;")
            .with_child(text("one")),
    );
    let styled = StyledDom::create(&mut covered_only, azul_css::css::Css::empty());
    assert!(primary_asks(&[("Mem", latin())], &styled).is_empty());

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
    let asked = primary_asks(&[("Mem", latin())], &styled);
    assert_eq!(asked.len(), 1, "one wait for the DOM: {asked:?}");
    let names: Vec<_> = asked[0].0.iter().filter_map(|stack| stack.first().cloned()).collect();
    assert!(names.iter().any(|n| n == "Some Installed Family"), "the second stack is asked for: {names:?}");
    assert!(!names.iter().any(|n| n == "Mem"), "the first is not: {names:?}");
}

#[test]
fn a_dom_without_a_stack_has_nothing_to_ask_for_and_a_text_of_spaces_asks_as_a_line_still_takes_a_face() {
    let mut empty = Dom::create_body();
    let styled = StyledDom::create(&mut empty, azul_css::css::Css::empty());
    let asked = primary_asks(&[("Mem", latin())], &styled);
    // No text, no stack: nothing the layout resolves names a family, so nothing is asked of the system either.
    assert!(asked.is_empty(), "asked {asked:?}");
    let spaces = page("Mem", "", "", "   ");
    let asked = primary_asks(&[("Mem", latin())], &spaces);
    assert_eq!(asked.len(), 1, "a text of spaces: {asked:?}");
    assert!(asked[0].1.contains("no text a face has to draw"), "the reason is {:?}", asked[0].1);
}

#[test]
fn without_registered_faces_everything_asks() {
    assert_eq!(primary_asks(&[], &page("Mem", "", "", "Hello")).len(), 1);
}

#[test]
fn a_gate_that_is_not_pending_is_never_asked() {
    // A hit, or a scan that is over: the registry has what the layout needs.
    let gate = Recorder::new(false);
    let manager = manager_with(&[("Mem", latin())], MemoryFontTier::Primary, &gate);
    lay_out(&manager, &page("Some Installed Family", "", "", "Hello 漢"), &BTreeSet::new());
    assert!(gate.asked().is_empty());
}

#[test]
fn a_manager_without_a_gate_never_waits() {
    let file = std::fs::read(static_face_path()).expect("the face is checked in");
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    let _ = manager.register_named_font("Mem", &file, latin());
    assert!(manager.system_fonts_gate.is_none());
    lay_out(&manager, &page("Some Installed Family", "", "", "Hello"), &BTreeSet::new());
}

#[test]
fn a_child_dom_that_the_first_did_not_hold_asks_for_its_own_stacks() {
    // The window lays the root out, then the child DOM of a virtual view: each resolves its chains, and each asks.
    let gate = Recorder::new(true);
    let manager = manager_with(&[("Mem", latin())], MemoryFontTier::Primary, &gate);
    lay_out(&manager, &page("Mem", "", "", "the root"), &BTreeSet::new());
    assert!(gate.asked().is_empty(), "the root is drawn by the faces in memory");
    lay_out(&manager, &page("Some Installed Family", "", "", "the child"), &BTreeSet::new());
    assert_eq!(gate.asked().len(), 1, "the child names a system family");
}

#[test]
fn text_the_window_keeps_covered_is_asked_for_with_the_dom() {
    // `also`: the characters of text typed into a field and not yet in the DOM.
    let gate = Recorder::new(true);
    let manager = manager_with(&[("Mem", latin())], MemoryFontTier::Primary, &gate);
    let also: BTreeSet<char> = std::iter::once('漢').collect();
    lay_out(&manager, &page("Mem", "", "", "Hello"), &also);
    let asked = gate.asked();
    assert_eq!(asked.len(), 1, "asked {asked:?}");
    assert!(asked[0].1.contains("U+6F22"), "the reason is {:?}", asked[0].1);
}

#[test]
fn a_run_of_typed_text_no_face_of_its_chain_draws_asks_before_its_face_is_looked_up() {
    let gate = Recorder::new(true);
    let manager = manager_with(&[("Mem", latin())], MemoryFontTier::Primary, &gate);
    let selectors = vec![FontSelector {
        family: "Mem".to_string(),
        ..FontSelector::default()
    }];
    let run = |text: &str| {
        InlineContent::Text(StyledRun {
            text: Arc::from(text),
            style: Arc::new(StyleProperties {
                font_stack: FontStack::Stack(selectors.clone()),
                ..StyleProperties::default()
            }),
            logical_start_byte: 0,
            source_node_id: None,
        })
    };
    let key = FontChainKey::from_selectors(&selectors);
    let chain = FontFallbackChain::empty(&key.font_families);
    let loaded = manager.get_loaded_fonts();
    let lookup = |content: &[InlineContent]| {
        missing_coverage_faces(
            content,
            &mut |k| (k == &key).then(|| chain.clone()),
            &manager.fc_cache,
            None,
            &*loaded,
            manager.system_fonts_gate.as_deref(),
        )
    };
    let _ = lookup(&[run("Hello")]);
    assert!(gate.asked().is_empty(), "the chain draws the text: nothing to look up, nothing to wait for");
    let _ = lookup(&[run("Hello 漢")]);
    assert_eq!(gate.asked().len(), 1, "the character no face draws is looked up in the system: {:?}", gate.asked());
}
