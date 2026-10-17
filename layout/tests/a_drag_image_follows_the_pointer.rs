//! A drag image follows the pointer; the dragged row stays where it is.
//!
//! AzDrive drags files with a ghost - the file's glyph, its name, the count
//! of files and where they would go - that follows the pointer 14px right of
//! and 10px below it, while the rows stay in the list (dimmed). The ghost is
//! a DOM element the app renders (`position: fixed`, at the top left); a
//! `DragStart` callback names it, by its id, as the drag image
//! (`CallbackInfo::set_drag_image`, the live counterpart of the browser's
//! `dataTransfer.setDragImage`). Every pointer move then puts it at the
//! pointer plus its offset through the GPU transform values - no relayout,
//! no callback per move. Without a drag image, the dragged node itself
//! follows the pointer, as before.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    drag::{DragContext, DragData},
    events::ProcessEventResult,
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, managers::gesture::DragImage, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body (0) > [row (1) > label (2) > text (3), ghost #dragghost (4) > name
/// (5) > text (6)]. The label and the name are what is asked where it is on
/// screen: a node's own reference frame moves its content.
const ROW: usize = 1;
const ROW_LABEL: usize = 2;
const GHOST_NAME: usize = 5;
const ROW_TEXT_Y: f32 = 40.0;
const PRESS: LogicalPosition = LogicalPosition { x: 20.0, y: 50.0 };
const POINTER: LogicalPosition = LogicalPosition { x: 120.0, y: 80.0 };
const OFFSET: LogicalPosition = LogicalPosition { x: 14.0, y: 10.0 };

fn list() -> LayoutWindow {
    let ghost_id: azul_core::dom::IdOrClassVec =
        vec![IdOrClass::Id("dragghost".into())].into();
    let dom = Dom::create_body()
        .with_css("margin: 0px; font-size: 12px;")
        .with_child(
            Dom::create_div()
                .with_css(&format!("margin-top: {ROW_TEXT_Y}px; height: 20px;"))
                .with_child(Dom::create_div().with_child(
                    Dom::create_text_do_not_use_without_block_level_wrapper("Calmet 1732.pdf"),
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(ghost_id)
                .with_css("position: fixed; left: 0px; top: 0px; padding: 4px;")
                .with_child(
                    Dom::create_div().with_child(
                        Dom::create_text_do_not_use_without_block_level_wrapper(
                            "Calmet 1732.pdf",
                        ),
                    ),
                ),
        );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the list lays out");
    lw
}

fn node(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

/// The row is dragged from `PRESS` to `POINTER`.
fn drag_the_row(lw: &mut LayoutWindow) {
    lw.gesture_drag_manager.active_drag = Some(DragContext::node_drag(
        DomId::ROOT_ID,
        NodeId::new(ROW),
        PRESS,
        DragData::default(),
        1,
    ));
    lw.gesture_drag_manager.update_active_drag_positions(POINTER);
}

/// Apply what a pointer move during the drag shows, rebuilding the display
/// list when the move asks for it, as the shells do.
fn move_pointer(lw: &mut LayoutWindow) {
    if lw.update_drag_visual() == ProcessEventResult::ShouldUpdateDisplayListCurrentWindow {
        lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
        let _ = lw.update_drag_visual();
    }
}

fn origin(lw: &LayoutWindow, index: usize) -> LogicalPosition {
    lw.get_node_hit_test_bounds(node(index))
        .unwrap_or_else(|| panic!("node {index} is painted"))
        .origin
}

#[test]
fn the_drag_image_stands_at_the_pointer_plus_its_offset() {
    let mut lw = list();
    let rest = origin(&lw, GHOST_NAME);
    drag_the_row(&mut lw);
    lw.gesture_drag_manager.set_drag_image(DragImage {
        id: "dragghost".into(),
        offset: OFFSET,
    });
    move_pointer(&mut lw);
    let now = origin(&lw, GHOST_NAME);
    // The ghost's border box goes to the pointer plus the offset; its name
    // sits inside it where it sat inside it at rest.
    assert!(
        (now.x - (POINTER.x + OFFSET.x + rest.x)).abs() < 0.5
            && (now.y - (POINTER.y + OFFSET.y + rest.y)).abs() < 0.5,
        "the ghost's name at {now:?}, at rest {rest:?}, the pointer {POINTER:?} + {OFFSET:?}"
    );
}

#[test]
fn with_a_drag_image_the_dragged_row_stays_in_the_list() {
    let mut lw = list();
    let rest = lw.get_node_hit_test_bounds(node(ROW_LABEL)).map(|r| r.origin);
    drag_the_row(&mut lw);
    lw.gesture_drag_manager.set_drag_image(DragImage {
        id: "dragghost".into(),
        offset: OFFSET,
    });
    move_pointer(&mut lw);
    assert_eq!(
        lw.get_node_hit_test_bounds(node(ROW_LABEL)).map(|r| r.origin),
        rest,
        "the row's text did not move"
    );
}

#[test]
fn the_drag_image_ends_with_the_drag() {
    let mut lw = list();
    let rest = origin(&lw, GHOST_NAME);
    drag_the_row(&mut lw);
    lw.gesture_drag_manager.set_drag_image(DragImage {
        id: "dragghost".into(),
        offset: OFFSET,
    });
    move_pointer(&mut lw);
    lw.end_drag_visual();
    lw.gesture_drag_manager.end_drag();
    assert!(lw.gesture_drag_manager.drag_image().is_none(), "the next drag starts without one");
    let after = origin(&lw, GHOST_NAME);
    assert!(
        (after.x - rest.x).abs() < 0.5 && (after.y - rest.y).abs() < 0.5,
        "the ghost is back where the app put it: {after:?} vs {rest:?}"
    );
}

#[test]
fn without_a_drag_image_the_dragged_node_follows_the_pointer() {
    let mut lw = list();
    let rest = origin(&lw, ROW_LABEL);
    drag_the_row(&mut lw);
    move_pointer(&mut lw);
    let now = origin(&lw, ROW_LABEL);
    assert!(
        (now.x - (rest.x + POINTER.x - PRESS.x)).abs() < 0.5
            && (now.y - (rest.y + POINTER.y - PRESS.y)).abs() < 0.5,
        "the row's text moved with the pointer: {now:?} from {rest:?}"
    );
}
