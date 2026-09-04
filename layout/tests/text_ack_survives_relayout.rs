//! The text-revision handshake across the two funnel entries.
//!
//! An app folds azul's committed text into its model from `TextChanged`
//! (`get_unsynced_text_edits` + `mark_text_revision_synced`) WITHOUT
//! re-rendering — a live word count does exactly that. The ack says "my
//! model has the text"; the overlay must keep painting it until a DOM built
//! from that model arrives (`LayoutWindow::layout_new_generation`). Retiring
//! acked entries on the relayout entry (`layout_and_generate_display_list`)
//! deleted the typed text from the screen at the next line-growth relayout,
//! which then laid out the DOM's pre-edit text.
//!
//! And a relayout is a re-LAND of overlay text, not a re-COMMIT: it must
//! neither bump the text revision (that made an acked entry "unsynced" again
//! and unretirable) nor re-announce `TextChanged`.

use azul_core::dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId};
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::solver3::display_list::DisplayListItem;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .host { display: block; } \
                   p { display: block; }";

/// body(0) > div.host(1, contenteditable) > p(2) > text(3) "hello".
const HOST: NodeId = NodeId::new(1);
const PARAGRAPH: NodeId = NodeId::new(2);
const TEXT: NodeId = NodeId::new(3);

fn editable_dom() -> StyledDom {
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("host".into())].into();
    let host = Dom::create_div()
        .with_ids_and_classes(class)
        .with_contenteditable(true)
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("hello"),
        ));
    let mut dom = Dom::create_body().with_child(host);
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

/// The app re-renders: a fresh DOM from its model, installed through the
/// new-generation entry.
fn re_render(lw: &mut LayoutWindow) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_new_generation(
        editable_dom(),
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

/// A relayout of the DOM the app already rendered (the shells'
/// `incremental_relayout`).
fn relayout_same_dom(lw: &mut LayoutWindow) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    let existing = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    lw.layout_and_generate_display_list(
        existing,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    re_render(&mut lw);
    lw
}

/// Type `s` at the START of the paragraph and commit it, the way a keystroke
/// does (`record_text_input` + `apply_text_changeset`): focus on the
/// contenteditable HOST, caret in the text leaf - the commit keys the overlay
/// entry to the caret's IFC owner, the paragraph.
fn type_at_start(lw: &mut LayoutWindow, s: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
    }));
    lw.text_edit_manager.initialize_editing(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        TEXT,
        0,
    );
    let _ = lw.record_text_input(s);
    let _ = lw.apply_text_changeset();
}

/// The text the engine would hand the next keystroke for `node`: the overlay
/// entry if there is one, the DOM's text otherwise.
fn text_of(lw: &LayoutWindow, node: NodeId) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, node);
    lw.extract_text_from_inline_content(&content)
}

/// How many glyphs the current display list paints — the screen truth.
fn painted_glyph_count(lw: &LayoutWindow) -> usize {
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

/// The overlay's one edited IFC root after typing (whichever node the commit
/// keyed it to), with its flattened text.
fn overlay_entry(lw: &LayoutWindow) -> Option<(NodeId, String)> {
    let mut it = lw.content_overlay.iter_text();
    let (&(_, node), dirty) = it.next()?;
    assert!(it.next().is_none(), "exactly one edited IFC root");
    Some((node, azul_layout::overlay::flatten_inline_content(&dirty.content)))
}

#[test]
fn acked_text_keeps_painting_until_the_app_re_renders() {
    let mut lw = window();
    let glyphs_before = painted_glyph_count(&lw);
    assert_eq!(glyphs_before, 5, "premise: 'hello' paints 5 glyphs");

    type_at_start(&mut lw, "X");
    let (edited, text) = overlay_entry(&lw).expect("the keystroke wrote an overlay entry");
    assert_eq!(edited, PARAGRAPH, "keyed to the caret's IFC owner");
    assert_eq!(text, "Xhello");
    assert_eq!(painted_glyph_count(&lw), 6, "the typed X paints");

    // The app (from its TextChanged callback) folds the text into its model
    // and acks - WITHOUT re-rendering.
    let unsynced = lw.unsynced_text_edits();
    assert_eq!(unsynced.len(), 1);
    assert_eq!(unsynced[0].1, "Xhello");
    let revision = unsynced[0].2;
    assert_eq!(revision, lw.document_text_revision);
    lw.mark_text_revision_synced(revision);
    assert!(lw.unsynced_text_edits().is_empty(), "acked = nothing unsynced");

    // A relayout of the DOM the app already rendered: the acked entry is
    // still the only source of the typed text, so it stays and paints.
    relayout_same_dom(&mut lw);
    assert_eq!(
        overlay_entry(&lw).map(|(n, t)| (n, t)),
        Some((edited, "Xhello".to_string())),
        "an acked entry survives a relayout of the unchanged DOM"
    );
    assert_eq!(text_of(&lw, edited), "Xhello");
    assert_eq!(
        painted_glyph_count(&lw),
        6,
        "the typed X still paints after the relayout (it used to vanish)"
    );

    // The app's next re-render installs a DOM built from the model that
    // acked: NOW the entry retires, without any text comparison.
    re_render(&mut lw);
    assert!(
        overlay_entry(&lw).is_none(),
        "the acked entry retires at the app's re-render"
    );
}

#[test]
fn an_unacked_entry_survives_both_passes_until_the_text_converges() {
    let mut lw = window();
    type_at_start(&mut lw, "X");
    let (edited, _) = overlay_entry(&lw).unwrap();

    // No ack: neither a relayout nor a re-render whose DOM still reads
    // "hello" may drop the entry (the equality rule needs the DOM to catch up).
    relayout_same_dom(&mut lw);
    assert_eq!(text_of(&lw, edited), "Xhello");
    re_render(&mut lw);
    assert_eq!(
        text_of(&lw, edited),
        "Xhello",
        "un-acked text stays authoritative over a DOM that has not caught up"
    );
    assert_eq!(painted_glyph_count(&lw), 6);
}

#[test]
fn a_relayout_re_lands_overlay_text_without_re_committing_it() {
    let mut lw = window();
    type_at_start(&mut lw, "X");

    // The commit announced itself exactly once ...
    let changed = lw.take_text_changed_notifications();
    assert_eq!(changed.len(), 1, "one TextChanged per commit, got {changed:?}");
    let revision = lw.document_text_revision;
    assert!(revision > 0);

    // ... and a relayout re-lands the entry: same revision, no new
    // announcement. (Before, the re-land went through the commit writer:
    // revision bumped, entry re-stamped, TextChanged queued again - per pass.)
    relayout_same_dom(&mut lw);
    assert_eq!(
        lw.document_text_revision, revision,
        "a relayout is not an edit: the text revision does not move"
    );
    assert_eq!(
        overlay_entry(&lw).map(|(n, _)| lw.content_overlay.text_for_node(DomId::ROOT_ID, n).unwrap().revision),
        Some(revision),
        "the entry keeps the revision of the commit that wrote it"
    );
    assert!(
        lw.take_text_changed_notifications().is_empty(),
        "no TextChanged for text that did not change"
    );
    assert!(lw.take_text_edit_notifications().is_empty());

    // And an ack made BEFORE the relayout still holds after it.
    lw.mark_text_revision_synced(revision);
    relayout_same_dom(&mut lw);
    assert!(
        lw.unsynced_text_edits().is_empty(),
        "the relayout did not re-stamp the acked entry as unsynced"
    );
}
