//! Drag-select inside a container the user has already scrolled.
//!
//! `calculated_positions` is STATIC, unscrolled layout geometry, but a pointer
//! arrives in window space. The click path resolved its endpoint through the
//! scroll-correct hit test while the drag path subtracted the static position
//! and stopped — so the two disagreed by exactly the scroll offset, and a drag
//! held past the container edge resolved the SAME endpoint on every autoscroll
//! tick while the view moved underneath it (the selection appeared to freeze).

use azul_core::dom::{Dom, DomId, IdOrClass, NodeId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_core::selection::Selection;
use azul_core::styled_dom::StyledDom;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Enough lines that the box overflows and there is somewhere to scroll to.
const LINES: usize = 40;

fn scroller() -> LayoutWindow {
    let css_src = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .box { display: block; width: 600px; height: 100px; overflow-y: scroll; } \
                   .line { display: block; }";
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("box".into())].into();
    let mut box_dom = Dom::create_div().with_ids_and_classes(class);
    for i in 0..LINES {
        let line_class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("line".into())].into();
        box_dom = box_dom.with_child(
            Dom::create_div()
                .with_ids_and_classes(line_class)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    format!("line {i} with enough words to hit"),
                )),
        );
    }
    let mut dom = Dom::create_body().with_child(box_dom);
    let (css, _) = azul_css::parser2::new_from_str(css_src);
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
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

/// Scroll the box itself. The container is body's first child.
fn scroll_box_to(lw: &mut LayoutWindow, y: f32) {
    let now = azul_core::task::Instant::from(std::time::Instant::now());
    let container = NodeId::new(1);
    lw.scroll_manager.update_node_bounds(
        DomId::ROOT_ID,
        container,
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 100.0)),
        LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(600.0, 100.0 * LINES as f32),
        ),
        now.clone(),
    );
    lw.scroll_manager.set_scroll_position_unclamped(
        DomId::ROOT_ID,
        container,
        LogicalPosition::new(0.0, y),
        now,
    );
}

/// Where the selection currently ENDS, whichever representation holds it.
///
/// A drag that leaves the anchor block is stored as a cross-block selection,
/// not on the primary cursor, so a probe that only reads `multi_cursor` reports
/// the anchor unchanged no matter what the drag did.
fn focus_end(lw: &LayoutWindow) -> Option<(NodeId, u32)> {
    if let Some(cross) = lw.text_edit_manager.cross_block.as_ref() {
        // The far end of the cross-block range: the node with the highest id
        // carrying a range, and that range's end.
        let (node, ranges) = cross.affected_nodes.iter().next_back()?;
        let last = ranges.last()?;
        return Some((*node, last.end.cluster_id.start_byte_in_run));
    }
    let mc = lw.text_edit_manager.multi_cursor.as_ref()?;
    let node = mc.node_id.node.into_crate_internal()?;
    let cursor = match &mc.get_primary()?.selection {
        Selection::Cursor(c) => *c,
        Selection::Range(r) => r.end,
    };
    Some((node, cursor.cluster_id.start_byte_in_run))
}

/// A drag that ends where a click would land must select up to the same place.
///
/// Before the fix the drag endpoint was computed from unscrolled geometry, so
/// after scrolling it resolved a cursor one scroll-offset away from wherever
/// the user actually released — and the anchor, set by the scroll-CORRECT
/// click path, disagreed with it.
#[test]
fn a_drag_in_a_scrolled_container_ends_where_a_click_there_would() {
    let target = LogicalPosition::new(120.0, 60.0);
    let anchor_at = LogicalPosition::new(20.0, 10.0);

    let mut lw = scroller();
    scroll_box_to(&mut lw, 250.0);

    // What a click at `target` resolves to — the scroll-correct answer, and
    // the one the user sees under the pointer.
    lw.process_mouse_click_for_selection(target, 0);
    let by_click = focus_end(&lw).expect("a click in the scrolled box resolves a cursor");

    // Now anchor elsewhere and DRAG to exactly the same window position.
    lw.process_mouse_click_for_selection(anchor_at, 100);
    lw.process_mouse_drag_for_selection(anchor_at, target);
    let by_drag = focus_end(&lw).expect("the drag resolves a cursor");

    assert_eq!(
        by_drag, by_click,
        "a drag ending at {target:?} must select up to the same cursor a click there gives"
    );
}

/// The autoscroll case: the pointer is held still past the edge and the view
/// scrolls under it, so each tick must resolve a cursor FURTHER into the
/// document. The old code read constant static geometry against a constant
/// pointer and answered the same cursor every tick — the selection froze.
#[test]
fn holding_a_drag_while_the_view_scrolls_keeps_extending_the_selection() {
    let held = LogicalPosition::new(120.0, 95.0);

    let mut lw = scroller();
    scroll_box_to(&mut lw, 0.0);
    lw.process_mouse_click_for_selection(LogicalPosition::new(20.0, 10.0), 0);

    lw.process_mouse_drag_for_selection(held, held);
    let first = focus_end(&lw).expect("the drag resolves a cursor");

    // One autoscroll tick's worth of movement, pointer unchanged.
    scroll_box_to(&mut lw, 200.0);
    lw.process_mouse_drag_for_selection(held, held);
    let after_scroll = focus_end(&lw).expect("the drag still resolves a cursor");

    assert_ne!(
        after_scroll, first,
        "the view scrolled under a held pointer, so the selection end must move with it"
    );
}
