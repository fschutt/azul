//! Publishing layout's scroll containers into the [`ScrollManager`].
//!
//! This ran in the dll, AFTER `layout_and_generate_display_list` returned, and
//! the e2e runner carried a hand-maintained port of it. So there were three
//! answers to "which nodes are scrollable and how big are they": the dll's,
//! the runner's, and whatever each `layout/tests` test hand-seeded — and a
//! test could seed a scroll state production would never produce, which is
//! exactly the divergence that let scroll bugs hide from the suite.
//!
//! It lives here now and all three call it.

use azul_core::{
    dom::{DomId, NodeId},
    task::Instant,
};

use crate::{solver3::layout_tree::LayoutNodeId, window::LayoutWindow};

pub fn register_scroll_nodes(layout_window: &mut LayoutWindow, now: &Instant) {
    for (dom_id, layout_result) in &mut layout_window.layout_results {
        // What the VirtualView callbacks published for this DOM, read the way
        // `display_list::paint_scrollbars` reads it — same producer, same
        // `children_rect.size`, so the two cannot disagree about the extent.
        // Snapshotted before the registration loop mutates the manager;
        // registration never touches `virtual_scroll_size`.
        let scroll_states = layout_window.scroll_manager.get_scroll_states_for_dom(*dom_id);

        for node_idx in 0..layout_result.layout_tree.nodes.len() {
            let node = &layout_result.layout_tree.nodes[node_idx];
            let Some(dom_node_id) = node.dom_node_id else {
                continue;
            };

            // CSS spec: scrolling occurs within the padding box, so the
            // viewport for scroll clamp must be padding-box, not content-box.
            // This must match compute_scrollbar_geometry() which also uses
            // padding-box (inner_rect = paint_rect - borders).
            let border_box_size = node.used_size.unwrap_or_default();
            let resolved = node.box_props.unpack();
            let border = &resolved.border;
            let container_size = azul_core::geom::LogicalSize {
                width: (border_box_size.width
                        - border.left - border.right).max(0.0),
                height: (border_box_size.height
                         - border.top - border.bottom).max(0.0),
            };

            // STATIC (unscrolled) layout coordinates, NOT window coordinates —
            // the comment here used to claim otherwise. `scroll_selection_into_view`
            // depends on static, because it compares against a caret rect measured
            // the same way. A consumer that needs where the container APPEARS must
            // subtract the scroll of its ancestors itself.
            let container_origin = layout_result
                .calculated_positions
                .get(node_idx)
                .copied()
                .unwrap_or_else(azul_core::geom::LogicalPosition::zero);

            let Some(mut scrollbar_info) = layout_result
                .layout_tree
                .warm(LayoutNodeId::new(node_idx))
                .and_then(|w| w.scrollbar_info)
            else {
                continue;
            };

            // A VirtualView's scrollable extent does not exist during layout —
            // it is whatever its callback just published. Amend the flags here,
            // where both the tree and the ScrollManager are in reach, and store
            // the answer back so every later consumer (the GPU thumb updater
            // `update_scrollbar_transforms`, the registration below, the
            // hit-test scrollbar states) reads one decision rather than
            // re-deriving it. `paint_scrollbars` calls the same function while
            // building the display list, which is why the numbers agree.
            if let Some(pos) = scroll_states.get(&dom_node_id) {
                let raised = crate::solver3::cache::apply_virtual_scroll_necessity(
                    &layout_result.styled_dom,
                    dom_node_id,
                    pos.children_rect.size,
                    container_size,
                    &mut scrollbar_info,
                );
                if raised {
                    if let Some(warm) = layout_result.layout_tree.warm_mut(LayoutNodeId::new(node_idx)) {
                        warm.scrollbar_info = Some(scrollbar_info);
                    }
                }
            }

            // The same amendment for ORDINARY nodes: a text edit can grow an
            // IFC's content after layout (`reshape_text_node` refreshes
            // `overflow_content_size` but nothing re-runs Phase 3), so re-derive
            // the necessity from the CURRENT content size. For nodes whose
            // content is unchanged since layout this reads the same
            // `overflow_content_size` Phase 3 wrote and is a no-op; flags are
            // only ever raised, never lowered.
            {
                let content_now = layout_result
                    .layout_tree
                    .get_content_size(LayoutNodeId::new(node_idx));
                let raised = crate::solver3::cache::apply_content_scroll_necessity(
                    &layout_result.styled_dom,
                    dom_node_id,
                    content_now,
                    container_size,
                    &mut scrollbar_info,
                );
                if raised {
                    if let Some(warm) = layout_result.layout_tree.warm_mut(LayoutNodeId::new(node_idx)) {
                        warm.scrollbar_info = Some(scrollbar_info);
                    }
                }
            }

            if !(scrollbar_info.needs_vertical || scrollbar_info.needs_horizontal) {
                continue;
            }

            let container_rect = azul_core::geom::LogicalRect {
                origin: container_origin,
                size: container_size,
            };

            // `overscroll-behavior` from CSS, resolved per axis. Until this
            // was wired the two fields were hardcoded to `Auto` at every
            // construction site, so `contain` (stop scroll CHAINING to an
            // ancestor) and `none` (also suppress the local bounce) were
            // unreachable — the enum and every physics branch reading it had
            // existed all along with nothing to set them.
            let node_state = layout_result
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_node_id)
                .map(|n| n.styled_node_state)
                .unwrap_or_default();
            let overscroll_x = match crate::solver3::getters::get_overscroll_behavior_x(
                &layout_result.styled_dom,
                dom_node_id,
                &node_state,
            ) {
                crate::solver3::getters::MultiValue::Exact(v) => v,
                _ => azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
            };
            let overscroll_y = match crate::solver3::getters::get_overscroll_behavior_y(
                &layout_result.styled_dom,
                dom_node_id,
                &node_state,
            ) {
                crate::solver3::getters::MultiValue::Exact(v) => v,
                _ => azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
            };

            let content_size = layout_result.layout_tree.get_content_size(LayoutNodeId::new(node_idx));

            // Use the layout-computed scrollbar width, not the
            // hardcoded default. On macOS with overlay scrollbars,
            // scrollbar_width is 0.0 (no layout space reserved).
            // The scroll_manager falls back to DEFAULT_SCROLLBAR_WIDTH_PX
            // when thickness is 0 for hit-test geometry.
            let scrollbar_thickness = scrollbar_info.scrollbar_width
                .max(scrollbar_info.scrollbar_height);

            layout_window.scroll_manager.set_overscroll_behavior(
                *dom_id,
                dom_node_id,
                overscroll_x,
                overscroll_y,
            );
            layout_window.scroll_manager.register_or_update_scroll_node(
                *dom_id,
                dom_node_id,
                container_rect,
                content_size,
                now.clone(),
                scrollbar_thickness,
                scrollbar_info.visual_width_px,
                scrollbar_info.needs_horizontal,
                scrollbar_info.needs_vertical,
            );

        }
    }
    layout_window.scroll_manager.calculate_scrollbar_states();
}

