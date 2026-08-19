//! The outcome of scenario A: typing must keep the caret on screen.
//!
//! The reveal path itself was well covered — that a glide is queued, that the
//! right container is found, that the geometry is measured in the right dom.
//! What NOTHING asserted was the outcome: that after typing, the scroll offset
//! actually moved. A sign flip, a wrong padding, a reveal that resolved the
//! correct container and then scrolled it by zero would all have stayed green.

use azul_core::dom::{Dom, DomId, IdOrClass, NodeId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::StyledDom;
use azul_core::task::Instant;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A short, scrollable editable holding more lines than it can show.
const CSS: &str = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .box { display: block; width: 600px; height: 60px; overflow-y: scroll; } \
                   .line { display: block; }";

const LINES: usize = 30;
const EDITABLE: usize = 1;

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

fn editable_with_many_lines() -> LayoutWindow {
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("box".into())].into();
    let mut editable = Dom::create_div()
        .with_ids_and_classes(class)
        .with_contenteditable(true);
    for i in 0..LINES {
        let line_class: azul_core::dom::IdOrClassVec =
            vec![IdOrClass::Class("line".into())].into();
        editable = editable.with_child(
            Dom::create_div()
                .with_ids_and_classes(line_class)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    format!("line {i}"),
                )),
        );
    }
    let mut dom = Dom::create_body().with_child(editable);

    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    // Animations OFF so the reveal takes the instant path and the offset moves
    // synchronously. With them on it queues a glide for the physics timer,
    // which is armed in the dll and so does not exist here.
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
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
    lw
}

/// What the shells' `register_scroll_nodes` does after layout — it lives in
/// the dll, so a layout test has to stand in for it.
fn register_scroll(lw: &mut LayoutWindow, content_h: f32) {
    lw.scroll_manager.update_node_bounds(
        DomId::ROOT_ID,
        NodeId::new(EDITABLE),
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 60.0)),
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, content_h)),
        now(),
    );
}

fn offset_y(lw: &LayoutWindow) -> f32 {
    lw.scroll_manager
        .get_current_offset(DomId::ROOT_ID, NodeId::new(EDITABLE))
        .map_or(0.0, |o| o.y)
}

/// Put the editing caret on the LAST line, which is far below the visible box.
fn edit_at_last_line(lw: &mut LayoutWindow) -> NodeId {
    // body(0) > editable(1) > [line div, text] * LINES — the last text leaf.
    let last_text = NodeId::new(1 + LINES * 2);
    lw.focus_manager.set_focused_node(Some(azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(last_text)),
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
        last_text,
        0,
    );
    last_text
}

/// Typing at a caret below the fold must bring it back into view.
#[test]
fn typing_below_the_fold_scrolls_the_caret_back_into_view() {
    let mut lw = editable_with_many_lines();
    register_scroll(&mut lw, 60.0 * LINES as f32);
    edit_at_last_line(&mut lw);

    assert_eq!(offset_y(&lw), 0.0, "the box starts at the top");

    let _ = lw.record_text_input("X");
    let _ = lw.apply_text_changeset();
    let scrolled = lw.scroll_selection_into_view(
        azul_layout::window::SelectionScrollType::Cursor,
        azul_layout::window::ScrollMode::Instant,
    );

    assert!(scrolled, "the reveal reported that it did nothing at all");
    assert!(
        offset_y(&lw) > 0.0,
        "the caret is below the visible box, so the offset must have moved; it is {}",
        offset_y(&lw)
    );
}

/// And the reverse: a caret already on screen must NOT move the view. Without
/// this control the test above would pass for a reveal that scrolls always.
#[test]
fn typing_at_a_visible_caret_leaves_the_view_alone() {
    let mut lw = editable_with_many_lines();
    register_scroll(&mut lw, 60.0 * LINES as f32);

    // First text leaf — line 0, comfortably inside a 60px box.
    let first_text = NodeId::new(3);
    lw.focus_manager.set_focused_node(Some(azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(first_text)),
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
        first_text,
        0,
    );

    let _ = lw.record_text_input("X");
    let _ = lw.apply_text_changeset();
    let _ = lw.scroll_selection_into_view(
        azul_layout::window::SelectionScrollType::Cursor,
        azul_layout::window::ScrollMode::Instant,
    );

    assert_eq!(
        offset_y(&lw),
        0.0,
        "a caret already in view must not scroll the container"
    );
}
