//! Pin for the 2026-08-29 "squashed garbage / weirdly large" TextInput: the
//! FIRST keystroke into an empty editable seeds the edit buffer with one
//! empty run, and that run's style must be resolved on the node a full
//! layout shapes the text at — the value `<p>`'s Text child — NOT on the
//! contenteditable container. The stock widget styles the value at 11px and
//! leaves the container inheriting 16px; a container-resolved seed shaped
//! the whole field at 16px forever (the overlay is sticky), and the
//! incremental reshape then pasted 16px glyphs onto 11px cached positions.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, text3::cache::InlineContent,
    widgets::text_input::TextInput, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > container(1) > label-p(2) > text(3). The prompt is an
/// ATTRIBUTE on the value line, not a node (2026-08-31).
const CONTAINER: usize = 1;
const LABEL_TEXT: usize = 3;

fn dnid(node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

#[test]
fn the_first_keystroke_into_an_empty_field_carries_the_values_style_not_the_containers() {
    let mut dom = Dom::create_body().with_child(TextInput::create().dom());
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut dbg,
    )
    .unwrap();

    lw.focus_manager.set_focused_node(Some(dnid(CONTAINER)));
    lw.text_edit_manager.initialize_editing(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        NodeId::new(CONTAINER),
        0,
    );

    let affected = lw.record_text_input("a");
    assert!(!affected.is_empty(), "the keystroke reached the focused node");
    let _ = lw.apply_text_changeset();

    // The overlay now carries the seeded (then spliced-into) run, keyed on
    // the node the edit was recorded against (the focused container; the
    // label is the fallback in case the recording key ever moves).
    let dirty = lw
        .content_overlay
        .text_for_node(DomId::ROOT_ID, NodeId::new(CONTAINER))
        .or_else(|| lw.content_overlay.text_for_node(DomId::ROOT_ID, NodeId::new(2)))
        .expect("the edit landed in the content overlay");
    let run = dirty
        .content
        .iter()
        .find_map(|c| match c {
            InlineContent::Text(r) => Some(r),
            _ => None,
        })
        .expect("the seeded content has a text run");

    // The style node is the value's Text child — the same node the full
    // layout resolves runs on — so hit-test areas and live color re-resolve
    // keep working, and the font size matches the styled value.
    assert_eq!(
        run.source_node_id,
        Some(NodeId::new(LABEL_TEXT)),
        "the seeded run must carry the value's Text node, not None"
    );
    let expected = {
        // Whatever the cascade says at the value's Text node — resolved the
        // same way the widget styles it. On macOS/Windows this is 11px vs
        // the container's inherited 16px, which is the bug this pins.
        let lr = lw.get_layout_result(&DomId::ROOT_ID).unwrap();
        let vp = lr.viewport.size;
        azul_layout::solver3::getters::get_style_properties(
            &lr.styled_dom,
            NodeId::new(LABEL_TEXT),
            None,
            azul_css::props::basic::PhysicalSize::new(vp.width, vp.height),
        )
        .font_size_px
    };
    assert!(
        (run.style.font_size_px - expected).abs() < 0.01,
        "seed style {} != value style {} — the run was resolved on the wrong node",
        run.style.font_size_px,
        expected
    );
}

/// THE DEVICE REGRESSION (2026-08-31, third report): after typing, the
/// placeholder was still painted OVER the typed text.
///
/// Not a condition bug - the conditions (empty, unfocused) were both false.
/// The prompt item was emitted INSIDE the per-node content run, and
/// `try_copy_cached_run` replays a recorded run wholesale, so the item
/// recorded while the field was empty came back on every cached emission,
/// bypassing both checks. The earlier pixel pin could not see this: it built
/// a STATICALLY filled host, which never takes the cached-run path.
///
/// This types through the real edit pipeline and then asks the display list
/// what it actually emitted.
#[test]
fn typing_into_an_empty_field_stops_the_placeholder_from_being_painted() {
    use azul_layout::solver3::display_list::DisplayListItem;

    let mut dom = Dom::create_body().with_child(
        TextInput::create()
            .with_placeholder("Type something...".into())
            .dom(),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut dbg,
    )
    .unwrap();

    // Glyphs emitted across the whole list, so the count is independent of
    // which node the prompt is attributed to.
    let total_glyphs = |lw: &LayoutWindow| -> usize {
        lw.get_layout_result(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .items
            .iter()
            .map(|it| match it {
                DisplayListItem::Text { glyphs, .. } => glyphs.len(),
                _ => 0,
            })
            .sum()
    };

    // Premise: empty + unfocused paints the prompt, so there ARE glyphs.
    let empty_glyphs = total_glyphs(&lw);
    assert!(
        empty_glyphs >= 10,
        "premise: an empty unfocused field paints its prompt, got {empty_glyphs} glyphs"
    );

    lw.focus_manager.set_focused_node(Some(dnid(CONTAINER)));
    lw.text_edit_manager.initialize_editing(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        NodeId::new(CONTAINER),
        0,
    );
    let affected = lw.record_text_input("a");
    assert!(!affected.is_empty(), "the keystroke reached the focused node");
    let _ = lw.apply_text_changeset();

    // Re-emit: this is the path that replayed the cached run.
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);

    let typed_glyphs = total_glyphs(&lw);
    assert!(
        typed_glyphs < empty_glyphs,
        "after typing, the field must paint its VALUE ({typed_glyphs} glyphs) \
         and not the 17-glyph prompt on top of it (was {empty_glyphs} while empty)"
    );
}
