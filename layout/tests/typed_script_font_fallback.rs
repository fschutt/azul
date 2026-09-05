//! Text typed in a script the document did not contain when its font chains
//! were resolved must still get a face that can draw it.
//!
//! Chain resolution is scoped to the text the DOM CONTAINS at layout time.
//! A Latin paragraph resolves a chain with no Arabic face, and a character
//! typed afterwards lives in the content overlay — which the resolver never
//! scans — so the shaper fell through to its `.notdef` last resort and the
//! user saw boxes for as long as those chains stayed in force (AzWriter:
//! Arabic typed after the first Enter rendered as tofu; the same Arabic typed
//! into the blank document had rendered fine, since the empty DOM had taken
//! the broader legacy resolution path).
//!
//! The fixtures use a MEMORY-ONLY `FcFontCache`: the system's fonts cannot
//! rescue an assertion, and the only Arabic-capable face in the world is the
//! mock one registered by the test. A wrong verdict is therefore the engine's,
//! not the machine's. Its fallback config names the LATIN mocks for the
//! generics (see `latin_generics`), the way a real machine's does.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    text3::cache::{FontChainKey, ParsedFontTrait},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::{FcFallbackConfig, FcFontCache, GenericFamily, UnicodeRange};

/// `Azul Mock Arabic`: beh/teh/lam/meem/alef + space, nothing else (see
/// `tests/fonts/README.md`). Registered with exactly its cmap as coverage, so
/// it can be found for Arabic by a script-aware (coverage) lookup and for
/// nothing else.
const MOCK_ARABIC: &[u8] = include_bytes!("fonts/azul-mock-arabic.ttf");
const MOCK_ARABIC_COVERAGE: [UnicodeRange; 3] = [
    UnicodeRange {
        start: 0x0627,
        end: 0x0628,
    },
    UnicodeRange {
        start: 0x062A,
        end: 0x062A,
    },
    UnicodeRange {
        start: 0x0644,
        end: 0x0645,
    },
];
/// U+0628 ARABIC LETTER BEH — in the mock's cmap.
const BEH: char = '\u{0628}';

const CSS: &str = "* { margin: 0; padding: 0; } \
                   body { font-family: 'Azul Mock Mono'; font-size: 20px; width: 600px; } \
                   .p { display: block; } \
                   .wide { font-family: 'Azul Mock Wide'; }";

fn with_class(dom: Dom, class: &'static str) -> Dom {
    let ids: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class(class.into())].into();
    dom.with_ids_and_classes(ids)
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

/// `body(0) > div[contenteditable](1) > div.p(2) > text(3)` — AzWriter's shape.
fn latin_document() -> Dom {
    Dom::create_body().with_child(
        Dom::create_div()
            .with_contenteditable(true)
            .with_child(with_class(Dom::create_div(), "p").with_child(text("Hello"))),
    )
}
const HOST: usize = 1;

fn styled(mut dom: Dom) -> StyledDom {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

fn run_layout(lw: &mut LayoutWindow, styled_dom: StyledDom) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

/// The fallback config of a real machine, in miniature: the generics name
/// LATIN faces (the built-in ASCII mocks), so the Arabic mock is reachable
/// ONLY through a script-aware lookup - the situation the tests are about,
/// where `sans-serif` names a Latin font and Arabic needs a fallback.
///
/// rust-fontconfig 5 resolves a generic through this config. With NO config
/// a generic gets the best-style faces of the whole cache, narrowest first,
/// which on this three-font cache is the Arabic mock itself: the chain would
/// cover Arabic from the start and the tests would pass without the code
/// under test ever running.
fn latin_generics() -> FcFallbackConfig {
    let mut config = FcFallbackConfig::empty();
    for (generic, family) in [
        (GenericFamily::SansSerif, "Azul Mock Wide"),
        (GenericFamily::Serif, "Azul Mock Wide"),
        (GenericFamily::Monospace, "Azul Mock Mono"),
    ] {
        config
            .generic_families
            .insert(generic, vec![family.to_string()]);
    }
    config
}

/// A window over a memory-only cache: the two built-in mock faces (ASCII) plus
/// the mock Arabic face, and NOTHING the machine has installed.
fn window_with_mock_arabic(dom: Dom) -> LayoutWindow {
    let fc_cache = FcFontCache::default().with_fallback_config(latin_generics());
    let mut lw = LayoutWindow::new(fc_cache).unwrap();
    lw.font_manager
        .register_named_font("Azul Mock Arabic", MOCK_ARABIC, MOCK_ARABIC_COVERAGE.to_vec());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    run_layout(&mut lw, styled(dom));
    lw
}

fn dnid(node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

fn text_of(lw: &LayoutWindow, node: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(node));
    lw.extract_text_from_inline_content(&content)
}

/// Every `(font_hash, glyph index)` the display list paints.
fn painted_glyphs(lw: &LayoutWindow) -> Vec<(u64, u32)> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text {
                glyphs, font_hash, ..
            } => glyphs
                .iter()
                .map(|g| (font_hash.font_hash, g.index))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

/// The cache key of the paragraph's stack. The cascade appends the UA
/// fallback list (fontconfig's alias families) behind the CSS families, so
/// the key is found by its FIRST family rather than spelled out.
fn mono_key(lw: &LayoutWindow) -> FontChainKey {
    lw.font_manager
        .font_chain_cache
        .keys()
        .find(|key| {
            key.font_families.first().map(String::as_str) == Some("Azul Mock Mono")
                && key.weight == rust_fontconfig::FcWeight::Normal
                && !key.italic
                && !key.oblique
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "premise: the paragraph's stack resolved a chain at full layout; keys present: {:?}",
                lw.font_manager.font_chain_cache.keys().collect::<Vec<_>>()
            )
        })
}

/// Does the chain cached for the paragraph's stack resolve `ch` to a LOADED
/// face whose cmap really has it?
fn chain_draws(lw: &LayoutWindow, ch: char) -> bool {
    let Some(chain) = lw.font_manager.font_chain_cache.get(&mono_key(lw)) else {
        return false;
    };
    let Some((id, _)) = chain.resolve_char(&lw.font_manager.fc_cache, ch) else {
        return false;
    };
    let loaded = lw.font_manager.get_loaded_fonts();
    loaded
        .get(&id)
        .is_some_and(|font| font.has_glyph(ch as u32))
}

fn type_into_host(lw: &mut LayoutWindow, s: &str) {
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw.record_text_input(s);
    let _ = lw.apply_text_changeset();
}

fn assert_premise_no_arabic(lw: &LayoutWindow) {
    let _ = mono_key(lw);
    assert!(
        !chain_draws(lw, BEH),
        "premise: a Latin document resolves NO Arabic face — the resolver only \
         sees the text the DOM contains; chain = {:#?}; loaded = {:?}",
        lw.font_manager.font_chain_cache.get(&mono_key(lw)).map(|c| (
            c.css_fallbacks.iter().map(|g| (g.css_name.clone(), g.fonts.iter().map(|f| (f.id, f.unicode_ranges.clone())).collect::<Vec<_>>())).collect::<Vec<_>>(),
            c.unicode_fallbacks.iter().map(|g| (g.range, g.fonts.iter().map(|f| (f.id, f.unicode_ranges.clone())).collect::<Vec<_>>())).collect::<Vec<_>>(),
        )),
        lw.font_manager.get_loaded_fonts().iter().map(|(id, _)| *id).collect::<Vec<_>>()
    );
    let loaded = lw.font_manager.get_loaded_fonts();
    assert!(
        !loaded.iter().any(|(_, font)| font.has_glyph(BEH as u32)),
        "premise: no loaded face has an Arabic glyph the shaper could borrow"
    );
}

/// The Arabic mock has 26 glyphs and `.notdef` is glyph 0: an Arabic letter
/// that reached the display list as index 0 is a tofu box, whatever face it
/// was assigned to.
fn assert_no_tofu(glyphs: &[(u64, u32)], when: &str) {
    assert!(
        glyphs.iter().all(|(_, index)| *index != 0),
        "{when}: a glyph was painted as .notdef (tofu): {glyphs:?}"
    );
}

#[test]
fn arabic_typed_into_a_latin_paragraph_gets_a_face_that_can_draw_it() {
    let mut lw = window_with_mock_arabic(latin_document());
    assert_premise_no_arabic(&lw);
    let fonts_before: std::collections::BTreeSet<u64> =
        painted_glyphs(&lw).iter().map(|(f, _)| *f).collect();
    assert_eq!(fonts_before.len(), 1, "premise: 'Hello' paints in one face");

    type_into_host(&mut lw, &BEH.to_string());

    assert!(
        text_of(&lw, HOST).contains(BEH),
        "premise: the character landed in the buffer"
    );
    assert_eq!(
        lw.frame_report_synced().font_shape_deficit,
        0,
        "no shaping call may have hit an unloaded font"
    );
    assert!(
        chain_draws(&lw, BEH),
        "the paragraph's chain must now resolve the typed letter to a loaded face \
         that has it — the edit-time coverage extension did not run"
    );
    let glyphs = painted_glyphs(&lw);
    assert_no_tofu(&glyphs, "after typing Arabic into a Latin paragraph");
    let fonts_after: std::collections::BTreeSet<u64> = glyphs.iter().map(|(f, _)| *f).collect();
    assert_eq!(
        fonts_after.len(),
        2,
        "'Hello' keeps its face and the Arabic letter paints in the ONE face that \
         has it; got faces {fonts_after:?} (before: {fonts_before:?})"
    );
    assert!(
        fonts_after.is_superset(&fonts_before),
        "extending the chain must not re-font the Latin text"
    );
}

/// A keystroke in a script the chain already covers is the steady state and
/// must not grow the chain: the extension is a repair, not a per-keystroke
/// resolver.
#[test]
fn latin_keystrokes_leave_the_chain_alone() {
    let mut lw = window_with_mock_arabic(latin_document());
    let before = lw.font_manager.font_chain_cache[&mono_key(&lw)].clone();
    let loaded_before = lw.font_manager.get_loaded_fonts().iter().count();

    type_into_host(&mut lw, "X");

    let after = &lw.font_manager.font_chain_cache[&mono_key(&lw)];
    assert_eq!(
        after.unicode_fallbacks.len(),
        before.unicode_fallbacks.len(),
        "an ASCII keystroke added a fallback face to a chain that already covered it"
    );
    assert_eq!(
        lw.font_manager.get_loaded_fonts().iter().count(),
        loaded_before,
        "an ASCII keystroke loaded a font"
    );
    assert_no_tofu(&painted_glyphs(&lw), "after an ASCII keystroke");
}

/// The extension must survive the FULL layout that follows an app-side DOM
/// change while the typed text is still uncommitted (still in the overlay).
///
/// `set_font_chain_cache_with_sig` REPLACES the chain cache whenever the DOM's
/// font requirements change, so the edit-time extension is thrown away with
/// it; the resolver that rebuilds the cache scans the DOM (still "Hello"), not
/// the overlay (now "Hello" + Arabic). The full-layout half of the fix reads
/// the overlay before the chains are loaded, so the rebuilt cache covers the
/// typed text too — and the Arabic face is not garbage-collected as
/// "referenced by no chain" in between.
#[test]
fn a_relayout_that_rebuilds_the_chains_keeps_the_typed_script_covered() {
    let mut lw = window_with_mock_arabic(latin_document());
    assert_premise_no_arabic(&lw);
    type_into_host(&mut lw, &BEH.to_string());
    assert!(chain_draws(&lw, BEH), "premise: the edit-time extension ran");

    // The app re-renders with an extra node in a DIFFERENT family: the font
    // signature changes, so resolution re-runs and the chain cache is rebuilt.
    // Node ids 0..=3 are unchanged, so the overlay entry survives the swap.
    let changed = latin_document()
        .with_child(with_class(Dom::create_div(), "wide").with_child(text("W")));
    run_layout(&mut lw, styled(changed));

    assert!(
        text_of(&lw, HOST).contains(BEH),
        "premise: the uncommitted edit is still in force after the relayout"
    );
    assert!(
        chain_draws(&lw, BEH),
        "the rebuilt chain cache lost the typed script — the overlay was not \
         consulted when the chains were re-resolved"
    );
    assert_eq!(lw.frame_report_synced().font_shape_deficit, 0);
    assert_no_tofu(&painted_glyphs(&lw), "after a chain-rebuilding relayout");
}

/// Committed text takes the fast (registry) resolver, which walks the
/// stack's plain OS expansion and treats a codepoint no listed family covers
/// as a silent miss. A document whose DOM already contains a script the stack
/// does not cover must still get a face for it.
///
/// The registry here scans NOTHING (an empty scan config), so the disk cannot
/// contribute; the only Arabic face is the memory one, which the fast probe
/// cannot see (it walks file paths) and the coverage-based second lookup can.
#[test]
fn committed_arabic_in_a_latin_stack_is_covered_by_the_fast_resolver() {
    use azul_layout::FcFontRegistry;
    use rust_fontconfig::config::FcScanConfig;

    let registry = FcFontRegistry::new_with_config(FcScanConfig::empty());
    registry.spawn_scout_and_builders();
    registry.wait_for_scout();

    // The production configuration for a registry-bearing manager (see
    // `FontManager::with_registry`): a live registry means chain resolution
    // takes `resolve_font_chains_fast`.
    let mut lw = LayoutWindow::from_font_manager(
        azul_layout::text3::cache::FontManager::new(FcFontCache::default())
            .unwrap()
            .with_registry(registry),
    );
    lw.font_manager
        .register_named_font("Azul Mock Arabic", MOCK_ARABIC, MOCK_ARABIC_COVERAGE.to_vec());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;

    // The DOM text is committed Arabic next to Latin: "Hello" + beh.
    let committed = format!("Hello {BEH}");
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_contenteditable(true)
            .with_child(with_class(Dom::create_div(), "p").with_child(text(&committed))),
    );
    run_layout(&mut lw, styled(dom));

    assert_eq!(lw.frame_report_synced().font_shape_deficit, 0);
    assert!(
        chain_draws(&lw, BEH),
        "the fast resolver left committed Arabic uncovered: the chain's own \
         families have no Arabic face and no script-aware lookup ran"
    );
    assert_no_tofu(&painted_glyphs(&lw), "committed Arabic in a Latin stack");
}
