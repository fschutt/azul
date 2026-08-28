//! An IME composition lives in the SHAPING, never in the text store.
//!
//! Seam-audit R8. The bug this protects against is the one where committing a
//! CJK composition inserted it TWICE (once because the preedit had already been
//! written into the document, once because the shell then delivered the
//! committed string as ordinary text input) and cancelling left the composed
//! fragment behind.
//!
//! The fix is structural rather than defensive — there is no "undo the preedit"
//! step to get wrong, because the preedit is never applied to the document in
//! the first place:
//!
//! - the composition lives in `TextEditManager::preedit_text` and in
//!   `LayoutWindow::preedit_shaped_node`, and reaches the screen only through
//!   `reshape_text_node`, which touches the layout and not the store;
//! - `ContentOverlay::set_text` — the ONE writer of the optimistic text store —
//!   has exactly one call site, the committed-edit path
//!   (`update_text_cache_after_edit`);
//! - `get_text_before_textinput`, which every reader of the document goes
//!   through, therefore never sees a composition.
//!
//! `layout/tests/text_edit_seam_regressions.rs` pins the behaviour on a FLAT
//! editable (`an_ime_composition_never_reaches_the_text_store` and its two
//! siblings), and those do go red if `apply_preedit_to_text_cache` is rerouted
//! through `update_text_cache_after_edit`. What they cannot see is a route that
//! keys the overlay on a different node than the one they read, or one added
//! somewhere else in the crate entirely. This file closes that: the structural
//! tests pin the "exactly one writer" property directly, and the behavioural
//! ones run the composition through the P-wrapped shape
//! (`div[contenteditable] > p > text`) that the text widgets, azul-writer and
//! the contenteditable e2e fixtures all use, reading the document back at BOTH
//! the host and the inline formatting root.
//!
//! TO TURN THE STRUCTURAL TESTS RED: give `ContentOverlay::set_text` a second
//! call site, or make `apply_preedit_to_text_cache` write through the overlay.

use std::path::{Path, PathBuf};

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

// ---------------------------------------------------------------------------
// Structural: the text store has exactly one writer, and it is not the preedit
// ---------------------------------------------------------------------------

/// Every `.rs` file under `layout/src`, so "exactly one call site" means the
/// whole crate and not just the file this test happened to `include_str!`.
///
/// `ContentOverlay::set_text` is `pub(crate)`, so the crate IS the reachable
/// surface: nothing outside `azul-layout` can call it at all.
fn crate_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).expect("layout/src must be readable") {
            let path = entry.expect("readable dir entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let src = std::fs::read_to_string(&path).expect("readable source file");
                out.push((path, src));
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

/// The body of `fn <name>` in `src`, braces balanced.
fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let sig = format!("fn {name}(");
    let start = src.find(&sig).unwrap_or_else(|| {
        panic!(
            "could not find `{sig}` — it was renamed or moved. Update this test rather than \
             deleting it: an unwatched writer into the text store is exactly what it exists to \
             prevent."
        )
    });
    let rest = &src[start..];
    let open = rest.find('{').expect("a signature is followed by a body");
    let mut depth = 0usize;
    for (offset, ch) in rest[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[open..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces while extracting `{name}`");
}

fn window_rs() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/window.rs"))
        .expect("layout/src/window.rs must be readable")
}

#[test]
fn the_text_store_has_exactly_one_writer() {
    let sources = crate_sources();
    // A zero is not a measurement: an empty walk would make every assertion
    // below pass while proving nothing.
    assert!(
        sources.len() >= 20,
        "walked only {} source files under layout/src — the walk broke",
        sources.len()
    );

    let call_sites: Vec<String> = sources
        .iter()
        .flat_map(|(path, src)| {
            src.lines()
                .filter(|line| line.contains("content_overlay") && line.contains(".set_text("))
                .map(move |line| format!("{}: {}", path.display(), line.trim()))
        })
        .collect();

    assert_eq!(
        call_sites.len(),
        1,
        "`content_overlay.set_text(..)` must have exactly ONE call site — the committed-edit path \
         in `update_text_cache_after_edit`. Found:\n  {}\n\nEvery reader of the document goes \
         through `get_text_before_textinput`, which answers from the overlay first. A second \
         writer is a second way for text nobody committed (an IME composition, a preview) to \
         become the document.",
        call_sites.join("\n  ")
    );

    let window = window_rs();
    assert!(
        fn_body(&window, "update_text_cache_after_edit").contains("content_overlay.set_text("),
        "the one `content_overlay.set_text` call site moved out of `update_text_cache_after_edit`"
    );
}

#[test]
fn the_preedit_path_never_touches_the_text_store() {
    let window = window_rs();

    for name in ["apply_preedit_to_text_cache", "end_preedit_shaping"] {
        let body = fn_body(&window, name);
        assert!(
            body.len() > 100,
            "`{name}`'s body extracted as {} bytes — the extraction broke",
            body.len()
        );
        assert!(
            !body.contains("content_overlay"),
            "`{name}` writes to (or reads from) `content_overlay`. The composition must reach the \
             screen through `reshape_text_node` ONLY: the store is what
             `get_text_before_textinput` answers from, so a composition in it IS the document, \
             and committing then inserts it a second time.\n{body}"
        );
        assert!(
            !body.contains("update_text_cache_after_edit"),
            "`{name}` routes through `update_text_cache_after_edit`, which writes the content into \
             `content_overlay`. That is the double-insert bug: the commit that follows delivers \
             the same string again as ordinary text input.\n{body}"
        );
    }
}

#[test]
fn the_document_reader_knows_nothing_about_compositions() {
    let window = window_rs();
    let body = fn_body(&window, "get_text_before_textinput");
    assert!(
        body.len() > 500,
        "`get_text_before_textinput`'s body extracted as {} bytes — the extraction broke",
        body.len()
    );
    assert!(
        !body.to_lowercase().contains("preedit"),
        "`get_text_before_textinput` mentions the preedit. It is THE document reader — the \
         changeset path, the a11y tree and every export read through it — so folding a \
         composition in makes the in-flight composition part of the document for all of \
         them.\n{body}"
    );
}

// ---------------------------------------------------------------------------
// Behavioural: the P-wrapped editable, which is the shape the widgets use
// ---------------------------------------------------------------------------

const CSS: &str = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .p { display: block; }";

/// `body(0) > div[contenteditable](1) > div.p(2) > text(3)`. The composition is
/// applied to the IFC root (2); the document is read back at both the host (1)
/// and the IFC root (2), so an overlay write keyed on EITHER node is caught.
const HOST: usize = 1;
const IFC_ROOT: usize = 2;

fn p_wrapped_editable(content: &str) -> LayoutWindow {
    let ids: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("p".into())].into();
    let mut dom =
        Dom::create_body().with_child(Dom::create_div().with_contenteditable(true).with_child(
            Dom::create_div().with_ids_and_classes(ids).with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper(content),
            ),
        ));

    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    lw
}

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn start_editing(lw: &mut LayoutWindow, node: usize, at: u32) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }));
    lw.text_edit_manager
        .initialize_editing(cursor(at), DomId::ROOT_ID, NodeId::new(node), 0);
}

fn text_of(lw: &LayoutWindow, node: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(node));
    lw.extract_text_from_inline_content(&content)
}

fn glyph_count(lw: &LayoutWindow) -> usize {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => glyphs.len(),
            _ => 0,
        })
        .sum()
}

fn compose(lw: &mut LayoutWindow, text: &str) {
    lw.text_edit_manager.set_preedit(text.to_string(), -1, -1);
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(IFC_ROOT));
}

fn end_composition(lw: &mut LayoutWindow) {
    lw.text_edit_manager.clear_preedit();
    lw.apply_preedit_to_text_cache(DomId::ROOT_ID, NodeId::new(IFC_ROOT));
}

#[test]
fn a_composition_in_a_p_wrapped_editable_never_reaches_the_text_store() {
    let mut lw = p_wrapped_editable("ab");
    start_editing(&mut lw, IFC_ROOT, 0);
    let base_glyphs = glyph_count(&lw);

    compose(&mut lw, "xy");

    assert_eq!(
        glyph_count(&lw),
        base_glyphs + 2,
        "premise: the composed glyphs must actually be on screen, or the assertions below pass \
         because nothing happened"
    );
    assert_eq!(
        text_of(&lw, IFC_ROOT),
        "ab",
        "the inline formatting root's text is untouched by the composition"
    );
    assert_eq!(
        text_of(&lw, HOST),
        "ab",
        "and so is the contenteditable host's, which is what the changeset path reads"
    );
}

#[test]
fn committing_a_composition_in_a_p_wrapped_editable_inserts_it_exactly_once() {
    let mut lw = p_wrapped_editable("ab");
    start_editing(&mut lw, IFC_ROOT, 0);

    compose(&mut lw, "xy");
    // The commit sequence every shell runs: drop the composition, then deliver
    // the committed string as ordinary text input.
    end_composition(&mut lw);
    let _ = lw.record_text_input("xy");
    let _ = lw.apply_text_changeset();

    assert_eq!(text_of(&lw, IFC_ROOT), "xyab");
    assert_eq!(text_of(&lw, HOST), "xyab");
}

#[test]
fn committing_a_composition_mid_string_inserts_it_once_at_the_caret() {
    let mut lw = p_wrapped_editable("abcd");
    start_editing(&mut lw, IFC_ROOT, 2);

    compose(&mut lw, "xy");
    assert_eq!(
        text_of(&lw, IFC_ROOT),
        "abcd",
        "a mid-string composition is no more part of the document than one at the start"
    );

    end_composition(&mut lw);
    let _ = lw.record_text_input("xy");
    let _ = lw.apply_text_changeset();

    assert_eq!(
        text_of(&lw, IFC_ROOT),
        "abxycd",
        "the committed string lands at the caret, once — `abxyxycd` is the composition having \
         been written into the store as well"
    );
}

#[test]
fn cancelling_a_composition_in_a_p_wrapped_editable_restores_the_clean_base() {
    let mut lw = p_wrapped_editable("ab");
    start_editing(&mut lw, IFC_ROOT, 0);
    let base_glyphs = glyph_count(&lw);

    compose(&mut lw, "xy");
    assert_eq!(
        glyph_count(&lw),
        base_glyphs + 2,
        "premise: composed glyphs are painted"
    );

    end_composition(&mut lw);

    assert_eq!(
        glyph_count(&lw),
        base_glyphs,
        "no composed glyph survives the cancel"
    );
    assert_eq!(text_of(&lw, IFC_ROOT), "ab");
    assert_eq!(text_of(&lw, HOST), "ab");
}

/// A DOM rebuild in the middle of a composition re-shapes from the store and
/// then re-applies the composition (`reapply_dirty_text_node`). That is the
/// path on which a composition living in the store would be indistinguishable
/// from committed text — the rebuild would read it back as the base.
#[test]
fn a_rebuild_mid_composition_does_not_promote_it_to_committed_text() {
    let mut lw = p_wrapped_editable("ab");
    start_editing(&mut lw, IFC_ROOT, 0);

    // A real committed edit first, so the node has an overlay entry and
    // `reapply_dirty_text_node` has something to re-apply.
    let _ = lw.record_text_input("Z");
    let _ = lw.apply_text_changeset();
    assert_eq!(
        text_of(&lw, IFC_ROOT),
        "Zab",
        "premise: the committed edit landed"
    );
    let committed_glyphs = glyph_count(&lw);

    compose(&mut lw, "xy");
    lw.reapply_dirty_text_node(DomId::ROOT_ID, NodeId::new(IFC_ROOT));

    assert_eq!(
        text_of(&lw, IFC_ROOT),
        "Zab",
        "the rebuild must re-apply the composition to the SHAPING only; if it went through the \
         store the composition is now committed text"
    );
    assert_eq!(
        glyph_count(&lw),
        committed_glyphs + 2,
        "and the composition must still be visible after the rebuild"
    );

    end_composition(&mut lw);
    assert_eq!(text_of(&lw, IFC_ROOT), "Zab");
    assert_eq!(
        glyph_count(&lw),
        committed_glyphs,
        "cancel leaves only the committed text"
    );
}
