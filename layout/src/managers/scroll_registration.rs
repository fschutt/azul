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
    dom::{DomId, DomNodeId, NodeId, ScrollbarOrientation},
    task::Instant,
};

use crate::{solver3::layout_tree::LayoutNodeId, window::LayoutWindow};

/// Extra scrollable width given to the box that hosts the active caret.
///
/// THE CARET IS CONTENT, and `overflow_content_size` does not know that: it
/// measures the TEXT. At the end of the value the caret stands ON the text's
/// right edge, so its own width lies past it — and in a horizontally scrolled
/// field that edge is exactly `max_scroll_x`, which the reveal cannot scroll
/// beyond by definition. The blinking caret was therefore clipped away
/// precisely when the field started to overflow, which is the one moment it
/// matters. Trailing padding cannot fix this either: azul measures content
/// against the PADDING box, so padding shrinks the scroll range rather than
/// extending it.
///
/// Sized as the default caret width (1px) plus the 5px the caret reveal
/// (`calculate_instant_scroll_delta`) asks for, so the margin the reveal wants
/// is actually reachable instead of being clamped away at the end of the line.
pub const CARET_SCROLL_GUTTER_PX: f32 = 6.0;

/// The extent a scroll box publishes to the `ScrollManager`: its `content`,
/// plus [`CARET_SCROLL_GUTTER_PX`] when it is the box the active caret sits
/// on (`caret_node`, the session block's container - which in a TextInput is
/// the value `<p>`, both the IFC root and the horizontal scroll box).
///
/// ONE rule for [`register_scroll_nodes`] and for the text-edit fast path
/// (`LayoutWindow::reshape_text_node`), so a keystroke and the relayout after
/// it publish the same extent.
///
/// Only that one node, never its ancestors: widening every ancestor by the
/// gutter makes the BODY 6px wider than the viewport, so the whole page turns
/// horizontally scrollable the moment a text field takes focus.
#[must_use]
pub fn caret_scroll_extent(
    caret_node: Option<DomNodeId>,
    dom_id: DomId,
    node_id: NodeId,
    content: azul_core::geom::LogicalSize,
) -> azul_core::geom::LogicalSize {
    let hosts_caret = caret_node
        .is_some_and(|c| c.dom == dom_id && c.node.into_crate_internal() == Some(node_id));
    let mut content = content;
    if hosts_caret {
        content.width += CARET_SCROLL_GUTTER_PX;
    }
    content
}

/// The box the active caret sits on - the editing session's block
/// container, the one box [`caret_scroll_extent`] widens - named from the
/// carets a pass paints (`TextEditManager::build_cursor_locations`, which
/// lists the session's carets before the other seats'). ONE answer for
/// [`register_scroll_nodes`] and for `paint_scrollbars`, which sees only
/// that list: a thumb is painted for the extent registration publishes.
#[must_use]
pub fn caret_scroll_node(
    cursor_locations: &[crate::managers::text_edit::CursorLocation],
) -> Option<DomNodeId> {
    cursor_locations
        .iter()
        .find(|location| !location.owner.is_seat())
        .map(|location| location.block.container_dom_node())
}

pub fn register_scroll_nodes(layout_window: &mut LayoutWindow, now: &Instant) {
    // Runs after every layout - every frame of a layout-property tween - and
    // the desktop shell's incremental relayout runs it a second time.
    let _p = crate::probe::Probe::span("register_scroll_nodes");
    // Which node owns the active caret. Snapshotted BEFORE the loop below takes
    // `layout_results` mutably.
    let caret_node: Option<DomNodeId> =
        caret_scroll_node(&layout_window.text_edit_manager.build_cursor_locations());

    for (dom_id, layout_result) in &mut layout_window.layout_results {
        // The viewport the ROOT scrolls against (see the root arm below): the
        // one this layout was SOLVED in, which is what the display list paints
        // the viewport's bar along. `current_window_state` is not that - the
        // layout funnel publishes before the shell's resize path writes the
        // new size into it, so the first registration of every resize read
        // the previous window's.
        let viewport_size = layout_result.viewport.size;
        // What the VirtualView callbacks published for this DOM, read the way
        // `display_list::paint_scrollbars` reads it — same producer, same
        // `children_rect.size`, so the two cannot disagree about the extent.
        // Snapshotted before the registration loop mutates the manager;
        // registration never touches `virtual_scroll_size`.
        let scroll_states = layout_window
            .scroll_manager
            .get_scroll_states_for_dom(*dom_id);
        // The frames each box - and its bar - is painted in: the display
        // list's own answer, computed once for the whole dom.
        let scroll_chains = crate::solver3::scroll_chain::ScrollChains::compute(
            &layout_result.layout_tree,
            &layout_result.styled_dom,
            &layout_result.scroll_ids,
        );

        for node_idx in 0..layout_result.layout_tree.nodes.len() {
            let node = &layout_result.layout_tree.nodes[node_idx];
            let Some(dom_node_id) = node.dom_node_id else {
                continue;
            };

            // THE SCROLLPORT IS THE PADDING BOX. CSS Overflow 3 §2: "scrolling
            // occurs within the padding box", and `compute_scrollbar_geometry`
            // agrees (inner_rect = paint_rect − borders).
            //
            // ORIGIN AND SIZE now both come off that one box. This used to
            // publish a padding-box SIZE with a border-box ORIGIN — a
            // rectangle that is not any CSS box — so every consumer reading
            // `container_rect` as a whole (the auto-scroll timer's edge tests,
            // `scroll_into_view`'s `visible_rect`) was off by the border width
            // on the top and left, in window space, against a raw pointer.
            //
            // Coordinates are STATIC (unscrolled) layout, NOT window — the
            // comment here once claimed otherwise. `scroll_selection_into_view`
            // depends on static, because it compares against a caret rect
            // measured the same way. A consumer that needs where the container
            // APPEARS must subtract the scroll of its ANCESTORS itself.
            let border_box_origin = layout_result
                .calculated_positions
                .get(node_idx)
                .copied()
                .unwrap_or_else(azul_core::geom::LogicalPosition::zero);
            let scrollport =
                crate::solver3::display_list::BorderBoxRect(azul_core::geom::LogicalRect {
                    origin: border_box_origin,
                    size: node.used_size.unwrap_or_default(),
                })
                .to_padding_box(&node.box_props.unpack().border)
                .rect();
            // THE ROOT SCROLLS THE VIEWPORT, not its own box (CSS Overflow 3
            // §3.3). Its box has `height: auto` and grows to its content, so
            // measured against itself it never overflows and a page taller
            // than the window had nothing to scroll. The scrollport is the
            // window; what scrolls past it is the root's MARGIN box
            // (`LayoutTree::scroll_extent`, applied to the content size below).
            // The same question and the same extent `paint_scrollbars` and
            // `update_scrollbar_transforms` use, so the bar the pointer finds
            // here is the bar they paint and move.
            let is_viewport_root =
                crate::solver3::scrollbar::is_viewport_scroller(*dom_id, dom_node_id);
            let (container_size, container_origin) = if is_viewport_root {
                (
                    viewport_size,
                    azul_core::geom::LogicalPosition::zero(),
                )
            } else {
                (
                    azul_core::geom::LogicalSize {
                        width: scrollport.size.width.max(0.0),
                        height: scrollport.size.height.max(0.0),
                    },
                    scrollport.origin,
                )
            };

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
                    if let Some(warm) = layout_result
                        .layout_tree
                        .warm_mut(LayoutNodeId::new(node_idx))
                    {
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
                    if let Some(warm) = layout_result
                        .layout_tree
                        .warm_mut(LayoutNodeId::new(node_idx))
                    {
                        warm.scrollbar_info = Some(scrollbar_info);
                    }
                }
            }

            // A box that STOPPED overflowing is still in the manager, with the
            // rects it was registered with while it overflowed - and with them
            // its bar and its offset: the text of a field that fits again
            // stayed scrolled out of its own box, and a bar nobody painted any
            // more still took presses. Refresh it like any other registration
            // (the rects shrink, the bars go, the offset is clamped into the
            // new range). Only its PRINCIPAL box speaks for it: a list item's
            // `::marker` and a split preview's second part carry the same DOM
            // node, and must not overwrite its rects with their own. A box
            // that never scrolled still gets no state - one per scroll
            // container on the page would make each a candidate for the wheel.
            if !(scrollbar_info.needs_vertical || scrollbar_info.needs_horizontal) {
                let principal_box = layout_result
                    .layout_tree
                    .dom_to_layout
                    .get(&dom_node_id)
                    .and_then(|boxes| boxes.first())
                    .copied();
                let is_principal_box = principal_box == Some(LayoutNodeId::new(node_idx));
                let registered = layout_window
                    .scroll_manager
                    .get_scroll_state(*dom_id, dom_node_id)
                    .is_some();
                if !(is_principal_box && registered) {
                    continue;
                }
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

            // See [`CARET_SCROLL_GUTTER_PX`]: the caret is content the text
            // extent does not account for, so without this the reveal has
            // nowhere to scroll to and the caret is clipped at the end of an
            // overflowing line.
            let content_size = caret_scroll_extent(
                caret_node,
                *dom_id,
                dom_node_id,
                layout_result
                    .layout_tree
                    .scroll_extent(LayoutNodeId::new(node_idx), is_viewport_root),
            );

            layout_window.scroll_manager.set_overscroll_behavior(
                *dom_id,
                dom_node_id,
                overscroll_x,
                overscroll_y,
            );
            // The bars are layout's per-axis answer, amended above - the same
            // `presence` `paint_scrollbars` draws and the GPU updater moves.
            // The manager used to be handed the RESERVED width and the
            // necessity flags and to guess: a zero reservation became a 16px
            // bar of its own on any axis whose content was larger than its
            // box, so a `scrollbar-width: none` field got an invisible bar
            // over its whole value line, and an `overflow-y: hidden` box a
            // bar down its right edge.
            layout_window.scroll_manager.register_or_update_scroll_node(
                *dom_id,
                dom_node_id,
                container_rect,
                content_size,
                now.clone(),
                scrollbar_info.presence(ScrollbarOrientation::Horizontal),
                scrollbar_info.presence(ScrollbarOrientation::Vertical),
            );

            // The scroll frames this box - and its bar - are painted in: the
            // box's `ScrollChain`, the frames the display list opened around
            // it and the hit tester's chains add back. With them the scroll
            // manager hit-tests the bar where it is drawn, not where it was
            // laid out; the page's own frame puts the root above every box on
            // a page taller than its window.
            let ancestors: Vec<NodeId> = scroll_chains
                .box_chain(LayoutNodeId::new(node_idx))
                .scrolling()
                .map(|link| link.node)
                .collect();
            layout_window
                .scroll_manager
                .set_scroll_ancestors(*dom_id, dom_node_id, ancestors);
        }

        // A box that is no longer a scroll container AT ALL - re-rendered
        // `overflow: visible` or `clip` - has nothing left to scroll, and its
        // state goes, offset and all. Kept, the offset went on moving the
        // box's content for the hit tester, which scrolls the content of
        // every node with a state, while the painter only scrolls a scroll
        // container. A box that merely stopped overflowing is still one, and
        // was refreshed above.
        let stale: Vec<NodeId> = layout_window
            .scroll_manager
            .state_keys()
            .into_iter()
            .filter(|(d, n)| {
                *d == *dom_id && !is_scroll_container(&layout_result.styled_dom, *d, *n)
            })
            .map(|(_, n)| n)
            .collect();
        for node_id in stale {
            layout_window
                .scroll_manager
                .remove_scroll_node(*dom_id, node_id);
        }
    }
    publish_nested_dom_placements(layout_window);
    layout_window.scroll_manager.calculate_scrollbar_states();
}

/// Tell the scroll manager where every nested (`VirtualView`) dom is
/// composited, so the bars of its boxes are kept - and pressed - where they
/// are painted ([`NestedDomPlacement`]).
///
/// The host side comes off the display lists (`headless::nested_dom_viewports`,
/// the records the raster and the hit tester place a nested dom by). The
/// `VirtualView`'s own scroll is kept symbolic too: its item bakes
/// `materialized origin - offset` into `content_offset`, and a scroll of the
/// view patches that item without a layout, so the origin is published at
/// rest (`box origin + materialized origin`) with the view among the frames
/// whose live offsets move it.
///
/// [`NestedDomPlacement`]: crate::managers::scroll_state::NestedDomPlacement
fn publish_nested_dom_placements(layout_window: &mut LayoutWindow) {
    use crate::managers::scroll_state::NestedDomPlacement;

    // The host transforms resolved from the values the raster paints with
    // now; the frames stay symbolic, for the live offsets.
    let viewports = crate::headless::nested_dom_viewports(
        &layout_window.layout_results,
        &|dom, node| layout_window.css_transform_of(dom, node),
    );
    let placements: alloc::collections::BTreeMap<DomId, NestedDomPlacement> = viewports
        .into_iter()
        .filter_map(|(nested, viewports)| {
            // The innermost viewport is this dom's own `VirtualView` box, and
            // its transform the one the dom's content is composited under.
            let (view_box, view_frames, host_transform) = viewports.last()?.clone();
            let host = layout_window.virtual_view_manager.host_of_nested_dom(nested)?;
            let materialized = layout_window
                .virtual_view_manager
                .materialized_window_origin(host.0, host.1)
                .unwrap_or_else(azul_core::geom::LogicalPosition::zero);
            let mut host_frames = view_frames;
            host_frames.push(host);
            Some((
                nested,
                NestedDomPlacement {
                    origin: azul_core::geom::LogicalPosition::new(
                        view_box.origin.x + materialized.x,
                        view_box.origin.y + materialized.y,
                    ),
                    host_frames,
                    viewports,
                    host_transform,
                },
            ))
        })
        .collect();
    layout_window
        .scroll_manager
        .set_nested_dom_placements(placements);
}

/// Is `node_id` of `styled_dom` a SCROLL CONTAINER (CSS Overflow 3 §3.1:
/// `hidden`, `scroll` or `auto` on either axis)?
///
/// The viewport's root always is: the viewport scrolls whatever the root
/// declares (§3.3). A `VirtualView` host is too - its scroll state is its
/// callback's, published whatever the host's style. A node the DOM does not
/// have is not.
fn is_scroll_container(
    styled_dom: &azul_core::styled_dom::StyledDom,
    dom_id: DomId,
    node_id: NodeId,
) -> bool {
    if crate::solver3::scrollbar::is_viewport_scroller(dom_id, node_id) {
        return true;
    }
    let node_data = styled_dom.node_data.as_container();
    let Some(node_data) = node_data.get(node_id) else {
        return false;
    };
    if node_data.is_virtual_view_node() {
        return true;
    }
    crate::solver3::scroll_chain::is_css_scroll_container(styled_dom, node_id)
}
