//! Scroll-into-view implementation
//!
//! Provides W3C CSSOM View Module compliant scroll-into-view functionality.
//! This module contains the core primitive `scroll_rect_into_view` which all
//! higher-level scroll-into-view APIs build upon.
//!
//! # Architecture
//!
//! The core principle is that all scroll-into-view operations reduce to scrolling
//! a rectangle into the visible area of its scroll container ancestry:
//!
//! - `scroll_rect_into_view`: Core primitive - scroll any rect into view
//! - `scroll_node_into_view`: Scroll a DOM node's bounding rect into view
//! - `scroll_cursor_into_view`: Scroll a text cursor position into view
//!
//! # W3C Compliance
//!
//! This implementation follows the W3C CSSOM View Module specification:
//! - `ScrollLogicalPosition`: start, center, end, nearest
//! - `ScrollBehavior`: auto, instant, smooth
//! - Proper scroll ancestor chain traversal

use alloc::vec::Vec;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    task::{Duration, Instant},
};

use crate::{
    managers::scroll_state::ScrollManager,
    solver3::getters::{get_overflow_x, get_overflow_y},
    window::DomLayoutResult,
};

// Re-export types from core for public API
pub use azul_core::events::{ScrollIntoViewBehavior, ScrollIntoViewOptions, ScrollLogicalPosition};

/// Minimum scroll delta (in logical pixels) below which scrolling is skipped
const SCROLL_DELTA_THRESHOLD: f32 = 0.5;
/// Duration of smooth scroll animations in milliseconds
const SMOOTH_SCROLL_DURATION_MS: u64 = 300;

/// Calculated scroll adjustment for one scroll container
#[derive(Copy, Debug, Clone)]
pub struct ScrollAdjustment {
    /// The DOM containing the scroll container
    pub scroll_container_dom_id: DomId,
    /// The node ID of the scroll container within the DOM
    pub scroll_container_node_id: NodeId,
    /// The scroll delta to apply
    pub delta: LogicalPosition,
    /// The scroll behavior to use
    pub behavior: ScrollIntoViewBehavior,
}

/// Information about a scrollable ancestor
#[derive(Debug, Clone)]
struct ScrollableAncestor {
    dom_id: DomId,
    node_id: NodeId,
    /// The visible rect of the scroll container (content area)
    visible_rect: LogicalRect,
    /// Whether horizontal scroll is enabled
    scroll_x: bool,
    /// Whether vertical scroll is enabled
    scroll_y: bool,
}

// ============================================================================
// Core API: scroll_rect_into_view
// ============================================================================

/// Core function: scroll a rect into the visible area of its scroll containers
///
/// This is the ONLY scroll-into-view primitive. All higher-level APIs call this.
///
/// # Arguments
///
/// * `target_rect` - The rectangle to make visible (in absolute coordinates)
/// * `target_dom_id` - The DOM containing the target node
/// * `target_node_id` - The target node (used for finding scroll ancestors)
/// * `layout_results` - Layout data for all DOMs
/// * `scroll_manager` - Current scroll state
/// * `options` - How to scroll (alignment and animation)
/// * `now` - Current timestamp for animation
///
/// # Returns
///
/// A vector of scroll adjustments for each scroll container in the ancestry chain.
/// The adjustments are ordered from innermost (closest to target) to outermost.
// Instant is a ref-counted FFI clock handle threaded through the event loop by value;
// &-converting would cascade through the loop call chain.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn scroll_rect_into_view(
    target_rect: LogicalRect,
    target_dom_id: DomId,
    target_node_id: NodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    let mut adjustments = Vec::new();
    
    // Find scrollable ancestors from target to root
    let scroll_ancestors = find_scrollable_ancestors(
        target_dom_id,
        target_node_id,
        layout_results,
        scroll_manager,
    );
    
    if scroll_ancestors.is_empty() {
        return adjustments;
    }
    
    // Transform target_rect relative to each scroll container and calculate deltas
    let mut current_rect = target_rect;
    
    for ancestor in scroll_ancestors {
        // Calculate the scroll delta based on options
        let delta = calculate_scroll_delta(
            current_rect,
            ancestor.visible_rect,
            options.block,
            options.inline_axis,
            ancestor.scroll_x,
            ancestor.scroll_y,
        );
        
        // Only add adjustment if there's actual scrolling to do
        if delta.x.abs() > SCROLL_DELTA_THRESHOLD || delta.y.abs() > SCROLL_DELTA_THRESHOLD {
            // Resolve scroll behavior
            let behavior = resolve_scroll_behavior(
                options.behavior,
                ancestor.dom_id,
                ancestor.node_id,
                layout_results,
            );
            
            // Apply the scroll adjustment
            apply_scroll_adjustment(
                scroll_manager,
                ancestor.dom_id,
                ancestor.node_id,
                delta,
                behavior,
                now.clone(),
            );
            
            adjustments.push(ScrollAdjustment {
                scroll_container_dom_id: ancestor.dom_id,
                scroll_container_node_id: ancestor.node_id,
                delta,
                behavior,
            });
            
            // Adjust current_rect for next iteration (relative to new scroll position)
            current_rect.origin.x -= delta.x;
            current_rect.origin.y -= delta.y;
        }
    }
    
    adjustments
}

// ============================================================================
// Higher-Level APIs
// ============================================================================

/// Scroll a DOM node's bounding rect into view
///
/// This is a convenience wrapper around `scroll_rect_into_view` that
/// automatically gets the node's bounding rect from layout results.
pub fn scroll_node_into_view(
    node_id: DomNodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    // Get node's bounding rect from layout
    let Some(target_rect) = get_node_rect(node_id, layout_results) else {
        return Vec::new();
    };
    
    let Some(internal_node_id) = node_id.node.into_crate_internal() else {
        return Vec::new();
    };

    // Call the core rect-based API
    scroll_rect_into_view(
        target_rect,
        node_id.dom,
        internal_node_id,
        layout_results,
        scroll_manager,
        options,
        now,
    )
}

/// Scroll a text cursor position into view
///
/// Transforms the cursor's visual rect (in node-local coordinates) to absolute
/// coordinates before scrolling.
pub fn scroll_cursor_into_view(
    cursor_rect: LogicalRect,
    node_id: DomNodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    // Get node's position to transform cursor_rect to absolute coordinates
    let Some(node_rect) = get_node_rect(node_id, layout_results) else {
        return Vec::new();
    };
    
    // Transform cursor rect to absolute coordinates
    let absolute_cursor_rect = LogicalRect {
        origin: LogicalPosition {
            x: node_rect.origin.x + cursor_rect.origin.x,
            y: node_rect.origin.y + cursor_rect.origin.y,
        },
        size: cursor_rect.size,
    };
    
    let Some(internal_node_id) = node_id.node.into_crate_internal() else {
        return Vec::new();
    };

    // Call the core rect-based API
    scroll_rect_into_view(
        absolute_cursor_rect,
        node_id.dom,
        internal_node_id,
        layout_results,
        scroll_manager,
        options,
        now,
    )
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Find all scrollable ancestors from a node to the root
///
/// Returns ancestors ordered from innermost (closest to target) to outermost (root).
fn find_scrollable_ancestors(
    dom_id: DomId,
    node_id: NodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &ScrollManager,
) -> Vec<ScrollableAncestor> {
    let mut ancestors = Vec::new();
    
    let Some(layout_result) = layout_results.get(&dom_id) else {
        return ancestors;
    };
    
    let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();

    // Walk up the DOM tree from parent of target node
    let mut current = node_hierarchy.get(node_id).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
    
    while let Some(current_node_id) = current {
        // Check if this node is scrollable
        if let Some(ancestor) = check_if_scrollable(
            dom_id,
            current_node_id,
            layout_result,
            scroll_manager,
        ) {
            ancestors.push(ancestor);
        }
        
        // Move to parent
        current = node_hierarchy.get(current_node_id).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
    }
    
    ancestors
}

/// Check if a node is scrollable and return its scroll info
fn check_if_scrollable(
    dom_id: DomId,
    node_id: NodeId,
    layout_result: &DomLayoutResult,
    scroll_manager: &ScrollManager,
) -> Option<ScrollableAncestor> {
    let styled_nodes = layout_result.styled_dom.styled_nodes.as_container();
    let styled_node = styled_nodes.get(node_id)?;
    
    let overflow_x = get_overflow_x(
        &layout_result.styled_dom,
        node_id,
        &styled_node.styled_node_state,
    );
    let overflow_y = get_overflow_y(
        &layout_result.styled_dom,
        node_id,
        &styled_node.styled_node_state,
    );
    
    let scroll_x = overflow_x.is_scroll();
    let scroll_y = overflow_y.is_scroll();
    
    // If neither axis is scrollable, skip this node
    if !scroll_x && !scroll_y {
        return None;
    }
    
    // Check if the scroll manager has scroll state for this node
    // (which means it actually has overflowing content)
    let scroll_state = scroll_manager.get_scroll_state(dom_id, node_id)?;
    
    // Check if content actually overflows (use virtual_scroll_size when set, e.g. for VirtualView)
    let effective_width = scroll_state.virtual_scroll_size.map_or(scroll_state.content_rect.size.width, |s| s.width);
    let effective_height = scroll_state.virtual_scroll_size.map_or(scroll_state.content_rect.size.height, |s| s.height);
    let has_overflow_x = effective_width > scroll_state.container_rect.size.width;
    let has_overflow_y = effective_height > scroll_state.container_rect.size.height;
    
    if !has_overflow_x && !has_overflow_y {
        return None;
    }
    
    // Get the visible rect (container rect minus current scroll offset)
    let visible_rect = LogicalRect {
        origin: LogicalPosition {
            x: scroll_state.container_rect.origin.x + scroll_state.current_offset.x,
            y: scroll_state.container_rect.origin.y + scroll_state.current_offset.y,
        },
        size: scroll_state.container_rect.size,
    };
    
    Some(ScrollableAncestor {
        dom_id,
        node_id,
        visible_rect,
        scroll_x: scroll_x && has_overflow_x,
        scroll_y: scroll_y && has_overflow_y,
    })
}

/// Calculate the scroll delta needed to bring target into view within container
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn calculate_scroll_delta(
    target: LogicalRect,
    container: LogicalRect,
    block: ScrollLogicalPosition,
    inline: ScrollLogicalPosition,
    scroll_x_enabled: bool,
    scroll_y_enabled: bool,
) -> LogicalPosition {
    LogicalPosition {
        x: if scroll_x_enabled {
            calculate_axis_delta(
                target.origin.x,
                target.size.width,
                container.origin.x,
                container.size.width,
                inline,
            )
        } else {
            0.0
        },
        y: if scroll_y_enabled {
            calculate_axis_delta(
                target.origin.y,
                target.size.height,
                container.origin.y,
                container.size.height,
                block,
            )
        } else {
            0.0
        },
    }
}

/// Calculate scroll delta for a single axis
#[must_use] pub fn calculate_axis_delta(
    target_start: f32,
    target_size: f32,
    container_start: f32,
    container_size: f32,
    position: ScrollLogicalPosition,
) -> f32 {
    let target_end = target_start + target_size;
    let container_end = container_start + container_size;
    
    match position {
        ScrollLogicalPosition::Start => {
            // Align target start with container start
            target_start - container_start
        }
        ScrollLogicalPosition::End => {
            // Align target end with container end
            target_end - container_end
        }
        ScrollLogicalPosition::Center => {
            // Center target in container
            let target_center = target_start + target_size / 2.0;
            let container_center = container_start + container_size / 2.0;
            target_center - container_center
        }
        ScrollLogicalPosition::Nearest => {
            // Minimum scroll to make target fully visible
            if target_start < container_start {
                // Target is above/left of visible area - scroll up/left
                target_start - container_start
            } else if target_end > container_end {
                // Target is below/right of visible area
                if target_size <= container_size {
                    // Target fits, align end with container end
                    target_end - container_end
                } else {
                    // Target doesn't fit, align start with container start
                    target_start - container_start
                }
            } else {
                // Target is already fully visible
                0.0
            }
        }
    }
}

/// Resolve scroll behavior based on options and CSS properties
// +spec:containing-block:03528c - scroll-behavior on root element applies to viewport
const fn resolve_scroll_behavior(
    requested: ScrollIntoViewBehavior,
    _dom_id: DomId,
    _node_id: NodeId,
    _layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
) -> ScrollIntoViewBehavior {
    match requested {
        ScrollIntoViewBehavior::Auto => {
            // TODO: Check CSS scroll-behavior property on the scroll container
            // For now, default to instant
            ScrollIntoViewBehavior::Instant
        }
        other => other,
    }
}

/// Apply a scroll adjustment to the scroll manager
fn apply_scroll_adjustment(
    scroll_manager: &mut ScrollManager,
    dom_id: DomId,
    node_id: NodeId,
    delta: LogicalPosition,
    behavior: ScrollIntoViewBehavior,
    now: Instant,
) {
    use azul_core::events::EasingFunction;
    use azul_core::task::SystemTimeDiff;
    
    let current = scroll_manager
        .get_current_offset(dom_id, node_id)
        .unwrap_or_default();
    
    let new_position = LogicalPosition {
        x: current.x + delta.x,
        y: current.y + delta.y,
    };
    
    match behavior {
        ScrollIntoViewBehavior::Instant | ScrollIntoViewBehavior::Auto => {
            scroll_manager.set_scroll_position(dom_id, node_id, new_position, now);
        }
        ScrollIntoViewBehavior::Smooth => {
            // Use smooth scroll with 300ms duration
            let duration = Duration::System(SystemTimeDiff::from_millis(SMOOTH_SCROLL_DURATION_MS));
            scroll_manager.scroll_to(
                dom_id,
                node_id,
                new_position,
                duration,
                EasingFunction::EaseOut,
                now,
            );
        }
    }
}

/// Get a node's bounding rect from layout results
fn get_node_rect(
    node_id: DomNodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
) -> Option<LogicalRect> {
    let layout_result = layout_results.get(&node_id.dom)?;
    let nid = node_id.node.into_crate_internal()?;
    
    // Get position
    let layout_indices = layout_result.layout_tree.dom_to_layout.get(&nid)?;
    let layout_index = *layout_indices.first()?;
    let position = *layout_result.calculated_positions.get(layout_index)?;
    
    // Get size
    let layout_node = layout_result.layout_tree.get(layout_index)?;
    let size = layout_node.used_size?;
    
    Some(LogicalRect::new(position, size))
}

#[cfg(test)]
mod autotest_generated {
    use alloc::collections::BTreeMap;
    use std::collections::HashMap;

    use azul_core::{
        dom::{Dom, FormattingContext, IdOrClass},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::solver3::{
        display_list::DisplayList,
        geometry::PackedBoxProps,
        layout_tree::{LayoutNodeHot, LayoutTree},
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// Flat node indices of [`chain_dom`]. The fixture is a *linear* chain, so
    /// these indices hold under any tree-flattening order.
    const OUTER: usize = 1;
    const INNER: usize = 2;
    const TARGET: usize = 3;
    /// A node index far past the end of the fixture DOM.
    const OUT_OF_RANGE: usize = 9999;

    const SCROLL_CSS: &str = ".outer { overflow-x: scroll; overflow-y: scroll; } .inner { \
                              overflow-x: scroll; overflow-y: scroll; }";
    const X_ONLY_CSS: &str = ".inner { overflow-x: scroll; }";
    const NO_CSS: &str = "";

    fn dom_id(inner: usize) -> DomId {
        DomId { inner }
    }

    fn nid(index: usize) -> NodeId {
        NodeId::new(index)
    }

    fn dnid(dom: usize, index: usize) -> DomNodeId {
        DomNodeId {
            dom: dom_id(dom),
            node: NodeHierarchyItemId::from_crate_internal(Some(nid(index))),
        }
    }

    /// A `DomNodeId` whose node slot is the "no node" sentinel.
    fn null_dnid(dom: usize) -> DomNodeId {
        DomNodeId {
            dom: dom_id(dom),
            node: NodeHierarchyItemId::NONE,
        }
    }

    fn pos(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition::new(x, y)
    }

    fn size(width: f32, height: f32) -> LogicalSize {
        LogicalSize::new(width, height)
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
        LogicalRect::new(pos(x, y), size(width, height))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn now() -> Instant {
        Instant::now()
    }

    fn opts(
        block: ScrollLogicalPosition,
        inline_axis: ScrollLogicalPosition,
        behavior: ScrollIntoViewBehavior,
    ) -> ScrollIntoViewOptions {
        ScrollIntoViewOptions {
            block,
            inline_axis,
            behavior,
        }
    }

    fn div_with_class(class: &str) -> Dom {
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    /// `body(0) > .outer(1) > .inner(2) > .target(3)` — a strictly linear chain,
    /// styled through a real class stylesheet (not `Dom::with_css`, whose scoped
    /// `*` rule would also match the descendants and blur which node is styled).
    fn chain_dom(css_str: &str) -> StyledDom {
        let mut dom = Dom::create_body().with_child(
            div_with_class("outer")
                .with_child(div_with_class("inner").with_child(div_with_class("target"))),
        );
        let (css, _warnings) = azul_css::parser2::new_from_str(css_str);
        StyledDom::create(&mut dom, css)
    }

    fn empty_layout_tree() -> LayoutTree {
        LayoutTree {
            nodes: Vec::new(),
            warm: Vec::new(),
            cold: Vec::new(),
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena: Vec::new(),
            children_offsets: Vec::new(),
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    /// A `DomLayoutResult` with an *empty* layout tree. Everything except
    /// `get_node_rect` reads only `styled_dom`, so no real layout is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: empty_layout_tree(),
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: DisplayList::default(),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// One layout box per entry, in order: DOM node `n` maps to layout index `i`,
    /// laid out at `p` with used size `s`.
    fn layout_result_with_boxes(
        styled_dom: StyledDom,
        boxes: &[(usize, LogicalPosition, Option<LogicalSize>)],
    ) -> DomLayoutResult {
        let mut lr = layout_result(styled_dom);
        for (layout_index, (node_index, position, used_size)) in boxes.iter().enumerate() {
            lr.layout_tree
                .dom_to_layout
                .insert(nid(*node_index), vec![layout_index]);
            lr.layout_tree.nodes.push(LayoutNodeHot {
                box_props: PackedBoxProps::default(),
                dom_node_id: Some(nid(*node_index)),
                used_size: *used_size,
                formatting_context: FormattingContext::Block {
                    establishes_new_context: false,
                },
                parent: None,
            });
            lr.calculated_positions.push(*position);
        }
        lr
    }

    fn window(lr: DomLayoutResult) -> BTreeMap<DomId, DomLayoutResult> {
        let mut map = BTreeMap::new();
        map.insert(dom_id(0), lr);
        map
    }

    fn register(sm: &mut ScrollManager, node: usize, container: LogicalRect, content: LogicalSize) {
        sm.register_or_update_scroll_node(
            dom_id(0),
            nid(node),
            container,
            content,
            now(),
            16.0,
            8.0,
            false,
            false,
        );
    }

    /// `.inner` (node 2) is a 100×100 scroll container at the origin holding
    /// 100×1000 of content: vertical overflow only, max scroll y = 900.
    /// `.outer` (node 1) is styled scrollable but never registered with the scroll
    /// manager, so it is *not* a live scroll container.
    fn inner_only() -> (BTreeMap<DomId, DomLayoutResult>, ScrollManager) {
        let layout_results = window(layout_result(chain_dom(SCROLL_CSS)));
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 1000.0),
        );
        (layout_results, sm)
    }

    // ==================================================================
    // calculate_axis_delta — numeric: zero / min_max / negative / nan_inf
    // ==================================================================

    #[test]
    fn axis_delta_all_zero_inputs_are_zero_for_every_position() {
        for position in [
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::Center,
            ScrollLogicalPosition::End,
            ScrollLogicalPosition::Nearest,
        ] {
            let delta = calculate_axis_delta(0.0, 0.0, 0.0, 0.0, position);
            assert!(close(delta, 0.0), "{position:?} on all-zero input");
        }
    }

    #[test]
    fn axis_delta_start_aligns_target_start_with_container_start() {
        assert!(close(
            calculate_axis_delta(100.0, 50.0, 20.0, 30.0, ScrollLogicalPosition::Start),
            80.0
        ));
    }

    #[test]
    fn axis_delta_end_aligns_target_end_with_container_end() {
        // target 100..150, container 0..30 => 150 - 30
        assert!(close(
            calculate_axis_delta(100.0, 50.0, 0.0, 30.0, ScrollLogicalPosition::End),
            120.0
        ));
    }

    #[test]
    fn axis_delta_center_aligns_midpoints() {
        // target center 120, container center 50
        assert!(close(
            calculate_axis_delta(100.0, 40.0, 0.0, 100.0, ScrollLogicalPosition::Center),
            70.0
        ));
    }

    #[test]
    fn axis_delta_center_of_zero_sized_target_is_offset_of_container_center() {
        assert!(close(
            calculate_axis_delta(0.0, 0.0, 0.0, 100.0, ScrollLogicalPosition::Center),
            -50.0
        ));
    }

    #[test]
    fn axis_delta_nearest_leaves_fully_visible_target_alone() {
        // target 10..30 inside container 0..100
        assert!(close(
            calculate_axis_delta(10.0, 20.0, 0.0, 100.0, ScrollLogicalPosition::Nearest),
            0.0
        ));
    }

    #[test]
    fn axis_delta_nearest_scrolls_back_for_target_before_container() {
        assert!(close(
            calculate_axis_delta(-40.0, 20.0, 0.0, 100.0, ScrollLogicalPosition::Nearest),
            -40.0
        ));
    }

    #[test]
    fn axis_delta_nearest_aligns_end_when_target_fits() {
        // target 150..170 (size 20) below container 0..100 => end-align
        assert!(close(
            calculate_axis_delta(150.0, 20.0, 0.0, 100.0, ScrollLogicalPosition::Nearest),
            70.0
        ));
    }

    #[test]
    fn axis_delta_nearest_aligns_start_when_target_does_not_fit() {
        // target 150..450 (size 300) is bigger than the 100px container => start-align
        assert!(close(
            calculate_axis_delta(150.0, 300.0, 0.0, 100.0, ScrollLogicalPosition::Nearest),
            150.0
        ));
    }

    #[test]
    fn axis_delta_handles_negative_coordinates_deterministically() {
        // container -100..-50, target -200..-190
        assert!(close(
            calculate_axis_delta(-200.0, 10.0, -100.0, 50.0, ScrollLogicalPosition::Start),
            -100.0
        ));
        assert!(close(
            calculate_axis_delta(-200.0, 10.0, -100.0, 50.0, ScrollLogicalPosition::End),
            -140.0
        ));
        assert!(close(
            calculate_axis_delta(-200.0, 10.0, -100.0, 50.0, ScrollLogicalPosition::Nearest),
            -100.0
        ));
    }

    #[test]
    fn axis_delta_nearest_never_scrolls_the_wrong_way() {
        let (container_start, container_size) = (100.0f32, 50.0f32);
        for target_start in [-1000.0f32, 0.0, 99.0, 100.0, 120.0, 149.0, 150.0, 1000.0] {
            for target_size in [0.0f32, 10.0, 50.0, 60.0] {
                let delta = calculate_axis_delta(
                    target_start,
                    target_size,
                    container_start,
                    container_size,
                    ScrollLogicalPosition::Nearest,
                );
                let start_align = target_start - container_start;
                let end_align = (target_start + target_size) - (container_start + container_size);
                let fully_visible = target_start >= container_start
                    && (target_start + target_size) <= (container_start + container_size);

                // The delta is always one of {0, start-align, end-align} — never an
                // arbitrary value, and never an overshoot past both alignments.
                assert!(
                    close(delta, 0.0) || close(delta, start_align) || close(delta, end_align),
                    "delta {delta} for target {target_start}+{target_size}"
                );
                if fully_visible {
                    assert!(
                        close(delta, 0.0),
                        "visible target {target_start}+{target_size} must not scroll"
                    );
                }
                if target_start < container_start {
                    // Target starts before the viewport: only ever scroll backwards.
                    assert!(delta <= 0.0, "delta {delta} scrolled the wrong way");
                }
            }
        }
    }

    #[test]
    fn axis_delta_nan_target_start_does_not_panic() {
        // Start/End/Center are pure arithmetic: NaN propagates (a defined f32 result).
        for position in [
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::End,
            ScrollLogicalPosition::Center,
        ] {
            let delta = calculate_axis_delta(f32::NAN, 10.0, 0.0, 100.0, position);
            assert!(delta.is_nan(), "{position:?} should propagate NaN, got {delta}");
        }
    }

    #[test]
    fn axis_delta_nan_target_start_is_zero_for_nearest() {
        // Every NaN comparison is false, so `Nearest` falls through to the
        // "already fully visible" branch and returns exactly 0.0 — the safe
        // choice (no scroll) rather than a NaN leaking into the scroll offset.
        let delta = calculate_axis_delta(f32::NAN, 10.0, 0.0, 100.0, ScrollLogicalPosition::Nearest);
        assert_eq!(delta, 0.0);
    }

    #[test]
    fn axis_delta_nan_container_does_not_panic() {
        for position in [
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::End,
            ScrollLogicalPosition::Center,
            ScrollLogicalPosition::Nearest,
        ] {
            let delta = calculate_axis_delta(0.0, 10.0, f32::NAN, f32::NAN, position);
            assert!(
                delta.is_nan() || delta == 0.0,
                "{position:?} gave {delta} for a NaN container"
            );
        }
    }

    #[test]
    fn axis_delta_infinite_target_start_saturates_to_infinity() {
        let delta =
            calculate_axis_delta(f32::INFINITY, 10.0, 0.0, 100.0, ScrollLogicalPosition::Start);
        assert!(delta.is_infinite() && delta.is_sign_positive());

        let delta = calculate_axis_delta(
            f32::NEG_INFINITY,
            10.0,
            0.0,
            100.0,
            ScrollLogicalPosition::Start,
        );
        assert!(delta.is_infinite() && delta.is_sign_negative());
    }

    #[test]
    fn axis_delta_infinity_minus_infinity_is_nan_not_a_panic() {
        let delta = calculate_axis_delta(
            f32::INFINITY,
            10.0,
            f32::INFINITY,
            100.0,
            ScrollLogicalPosition::Start,
        );
        assert!(delta.is_nan());
    }

    #[test]
    fn axis_delta_infinite_sizes_do_not_panic() {
        for position in [
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::End,
            ScrollLogicalPosition::Center,
            ScrollLogicalPosition::Nearest,
        ] {
            let delta = calculate_axis_delta(0.0, f32::INFINITY, 0.0, f32::INFINITY, position);
            assert!(
                delta.is_nan() || delta.is_infinite() || delta.is_finite(),
                "{position:?} produced a non-f32 value"
            );
        }
    }

    #[test]
    fn axis_delta_f32_max_overflow_saturates_instead_of_panicking() {
        // target_start + target_size overflows f32 => +inf (IEEE saturation, no panic)
        let delta =
            calculate_axis_delta(f32::MAX, f32::MAX, 0.0, 100.0, ScrollLogicalPosition::End);
        assert!(delta.is_infinite() && delta.is_sign_positive());

        // Nearest sees target_end == +inf > container_end, and the (infinite)
        // target does not fit, so it start-aligns to a finite f32::MAX.
        let delta =
            calculate_axis_delta(f32::MAX, f32::MAX, 0.0, 100.0, ScrollLogicalPosition::Nearest);
        assert_eq!(delta, f32::MAX);
    }

    #[test]
    fn axis_delta_f32_min_does_not_panic() {
        for position in [
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::End,
            ScrollLogicalPosition::Center,
            ScrollLogicalPosition::Nearest,
        ] {
            let delta = calculate_axis_delta(f32::MIN, 1.0, f32::MAX, 1.0, position);
            assert!(!delta.is_nan(), "{position:?} produced NaN from finite input");
        }
    }

    // ==================================================================
    // calculate_scroll_delta — numeric
    // ==================================================================

    #[test]
    fn scroll_delta_zero_rects_are_zero() {
        let delta = calculate_scroll_delta(
            LogicalRect::zero(),
            LogicalRect::zero(),
            ScrollLogicalPosition::Nearest,
            ScrollLogicalPosition::Nearest,
            true,
            true,
        );
        assert_eq!((delta.x, delta.y), (0.0, 0.0));
    }

    #[test]
    fn scroll_delta_disabled_axes_are_exactly_zero_even_for_nan_and_infinite_rects() {
        let poison = LogicalRect::new(
            pos(f32::NAN, f32::INFINITY),
            size(f32::NAN, f32::NEG_INFINITY),
        );
        let delta = calculate_scroll_delta(
            poison,
            poison,
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::Start,
            false,
            false,
        );
        // Disabled axes short-circuit to 0.0 before any arithmetic runs, so no
        // NaN can reach the scroll offset.
        assert_eq!((delta.x, delta.y), (0.0, 0.0));
        assert!(delta.x.is_finite() && delta.y.is_finite());
    }

    #[test]
    fn scroll_delta_does_not_swap_the_axes() {
        // x must use `inline` + width, y must use `block` + height.
        let delta = calculate_scroll_delta(
            rect(10.0, 200.0, 5.0, 5.0),
            rect(0.0, 0.0, 100.0, 50.0),
            ScrollLogicalPosition::Start, // block  -> y
            ScrollLogicalPosition::End,   // inline -> x
            true,
            true,
        );
        assert!(close(delta.x, -85.0), "x used the wrong axis/position: {}", delta.x);
        assert!(close(delta.y, 200.0), "y used the wrong axis/position: {}", delta.y);
    }

    #[test]
    fn scroll_delta_only_enabled_axis_moves() {
        let target = rect(500.0, 500.0, 10.0, 10.0);
        let container = rect(0.0, 0.0, 100.0, 100.0);

        let x_only = calculate_scroll_delta(
            target,
            container,
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::Start,
            true,
            false,
        );
        assert!(close(x_only.x, 500.0));
        assert_eq!(x_only.y, 0.0);

        let y_only = calculate_scroll_delta(
            target,
            container,
            ScrollLogicalPosition::Start,
            ScrollLogicalPosition::Start,
            false,
            true,
        );
        assert_eq!(y_only.x, 0.0);
        assert!(close(y_only.y, 500.0));
    }

    // ==================================================================
    // resolve_scroll_behavior — predicate / invariant
    // ==================================================================

    #[test]
    fn resolve_behavior_maps_auto_to_instant_and_passes_the_rest_through() {
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        assert_eq!(
            resolve_scroll_behavior(
                ScrollIntoViewBehavior::Auto,
                dom_id(0),
                nid(TARGET),
                &empty
            ),
            ScrollIntoViewBehavior::Instant
        );
        assert_eq!(
            resolve_scroll_behavior(
                ScrollIntoViewBehavior::Instant,
                dom_id(0),
                nid(TARGET),
                &empty
            ),
            ScrollIntoViewBehavior::Instant
        );
        assert_eq!(
            resolve_scroll_behavior(
                ScrollIntoViewBehavior::Smooth,
                dom_id(OUT_OF_RANGE),
                nid(OUT_OF_RANGE),
                &empty
            ),
            ScrollIntoViewBehavior::Smooth
        );
    }

    #[test]
    fn resolve_behavior_is_idempotent() {
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        for behavior in [
            ScrollIntoViewBehavior::Auto,
            ScrollIntoViewBehavior::Instant,
            ScrollIntoViewBehavior::Smooth,
        ] {
            let once = resolve_scroll_behavior(behavior, dom_id(0), nid(0), &empty);
            let twice = resolve_scroll_behavior(once, dom_id(0), nid(0), &empty);
            assert_eq!(once, twice, "resolving {behavior:?} twice changed the result");
        }
    }

    // ==================================================================
    // apply_scroll_adjustment — numeric: zero / min_max / negative / overflow
    // ==================================================================

    /// 100×100 container, 500×500 content => max scroll (400, 400).
    fn registered_manager() -> ScrollManager {
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(500.0, 500.0),
        );
        sm
    }

    #[test]
    fn apply_zero_delta_leaves_the_offset_at_zero() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(0.0, 0.0),
            ScrollIntoViewBehavior::Instant,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn apply_on_an_unregistered_node_creates_a_zero_bounded_state() {
        // No bounds are known for the node, so max scroll is 0 and the delta is
        // clamped away entirely — it must not panic or store an unbounded offset.
        let mut sm = ScrollManager::new();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(OUT_OF_RANGE),
            pos(1234.0, 5678.0),
            ScrollIntoViewBehavior::Instant,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(OUT_OF_RANGE)).unwrap();
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn apply_instant_delta_moves_the_offset() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(50.0, 60.0),
            ScrollIntoViewBehavior::Instant,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.x, 50.0) && close(offset.y, 60.0));
        assert!(!sm.has_active_animations());
    }

    #[test]
    fn apply_negative_delta_clamps_to_zero() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(-1000.0, -1000.0),
            ScrollIntoViewBehavior::Instant,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn apply_f32_max_delta_clamps_to_max_scroll() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(f32::MAX, f32::MAX),
            ScrollIntoViewBehavior::Instant,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.x, 400.0) && close(offset.y, 400.0), "{offset:?}");
    }

    #[test]
    fn apply_infinite_delta_clamps_to_the_scroll_bounds() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(f32::INFINITY, f32::NEG_INFINITY),
            ScrollIntoViewBehavior::Instant,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(offset.x.is_finite() && offset.y.is_finite(), "{offset:?}");
        assert!(close(offset.x, 400.0) && close(offset.y, 0.0), "{offset:?}");
    }

    #[test]
    fn apply_nan_delta_cannot_poison_the_scroll_offset() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(f32::NAN, f32::NAN),
            ScrollIntoViewBehavior::Instant,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        // f32::max(NaN, 0.0) == 0.0, so the clamp scrubs the NaN.
        assert!(offset.x.is_finite() && offset.y.is_finite(), "{offset:?}");
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn apply_auto_behaves_like_instant() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(25.0, 25.0),
            ScrollIntoViewBehavior::Auto,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.x, 25.0) && close(offset.y, 25.0));
        assert!(!sm.has_active_animations());
    }

    #[test]
    fn apply_smooth_animates_instead_of_jumping() {
        let mut sm = registered_manager();
        apply_scroll_adjustment(
            &mut sm,
            dom_id(0),
            nid(INNER),
            pos(50.0, 50.0),
            ScrollIntoViewBehavior::Smooth,
            now(),
        );
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!((offset.x, offset.y), (0.0, 0.0), "smooth must not jump");
        assert!(sm.has_active_animations(), "smooth must arm an animation");
    }

    #[test]
    fn apply_deltas_accumulate_onto_the_current_offset() {
        let mut sm = registered_manager();
        for _ in 0..3 {
            apply_scroll_adjustment(
                &mut sm,
                dom_id(0),
                nid(INNER),
                pos(100.0, 100.0),
                ScrollIntoViewBehavior::Instant,
                now(),
            );
        }
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.x, 300.0) && close(offset.y, 300.0), "{offset:?}");
    }

    // ==================================================================
    // get_node_rect — missing / stale / corrupt layout data
    // ==================================================================

    #[test]
    fn get_node_rect_missing_dom_is_none() {
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        assert!(get_node_rect(dnid(0, TARGET), &empty).is_none());
    }

    #[test]
    fn get_node_rect_wrong_dom_id_is_none() {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(10.0, 20.0), Some(size(30.0, 40.0)))],
        ));
        assert!(get_node_rect(dnid(7, TARGET), &lrs).is_none());
    }

    #[test]
    fn get_node_rect_null_node_id_is_none() {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(10.0, 20.0), Some(size(30.0, 40.0)))],
        ));
        assert!(get_node_rect(null_dnid(0), &lrs).is_none());
    }

    #[test]
    fn get_node_rect_unmapped_node_is_none() {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(10.0, 20.0), Some(size(30.0, 40.0)))],
        ));
        assert!(get_node_rect(dnid(0, OUTER), &lrs).is_none());
        assert!(get_node_rect(dnid(0, OUT_OF_RANGE), &lrs).is_none());
    }

    #[test]
    fn get_node_rect_empty_layout_index_list_is_none() {
        let mut lr = layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(10.0, 20.0), Some(size(30.0, 40.0)))],
        );
        // A DOM node mapped to *no* layout box (can happen for display:none).
        lr.layout_tree.dom_to_layout.insert(nid(TARGET), Vec::new());
        assert!(get_node_rect(dnid(0, TARGET), &window(lr)).is_none());
    }

    #[test]
    fn get_node_rect_dangling_layout_index_is_none_not_a_panic() {
        let mut lr = layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(10.0, 20.0), Some(size(30.0, 40.0)))],
        );
        // Stale mapping pointing past the end of both `calculated_positions` and
        // `layout_tree.nodes` — must be a None, not an out-of-bounds index panic.
        lr.layout_tree.dom_to_layout.insert(nid(TARGET), vec![7]);
        assert!(get_node_rect(dnid(0, TARGET), &window(lr)).is_none());
    }

    #[test]
    fn get_node_rect_unsized_node_is_none() {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(10.0, 20.0), None)],
        ));
        assert!(get_node_rect(dnid(0, TARGET), &lrs).is_none());
    }

    #[test]
    fn get_node_rect_returns_position_and_used_size() {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(10.0, 20.0), Some(size(30.0, 40.0)))],
        ));
        let r = get_node_rect(dnid(0, TARGET), &lrs).expect("target has a layout box");
        assert!(close(r.origin.x, 10.0) && close(r.origin.y, 20.0));
        assert!(close(r.size.width, 30.0) && close(r.size.height, 40.0));
    }

    // ==================================================================
    // check_if_scrollable — predicate invariants
    // ==================================================================

    #[test]
    fn check_if_scrollable_out_of_range_node_is_none() {
        let lr = layout_result(chain_dom(SCROLL_CSS));
        let sm = ScrollManager::new();
        assert!(check_if_scrollable(dom_id(0), nid(OUT_OF_RANGE), &lr, &sm).is_none());
    }

    #[test]
    fn check_if_scrollable_without_overflow_css_is_none() {
        // Registered *and* overflowing, but the CSS says the node does not scroll.
        let lr = layout_result(chain_dom(NO_CSS));
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(500.0, 500.0),
        );
        assert!(check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm).is_none());
    }

    #[test]
    fn check_if_scrollable_without_scroll_state_is_none() {
        // CSS says scrollable, but the scroll manager has never seen the node.
        let lr = layout_result(chain_dom(SCROLL_CSS));
        let sm = ScrollManager::new();
        assert!(check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm).is_none());
    }

    #[test]
    fn check_if_scrollable_with_content_that_fits_is_none() {
        let lr = layout_result(chain_dom(SCROLL_CSS));
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 100.0),
        );
        assert!(check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm).is_none());
    }

    #[test]
    fn check_if_scrollable_reports_the_overflowing_axes_and_visible_rect() {
        let lr = layout_result(chain_dom(SCROLL_CSS));
        let mut sm = ScrollManager::new();
        // Container at (5, 7), overflowing vertically only.
        register(
            &mut sm,
            INNER,
            rect(5.0, 7.0, 100.0, 100.0),
            size(100.0, 1000.0),
        );
        sm.set_scroll_position(dom_id(0), nid(INNER), pos(0.0, 300.0), now());

        let ancestor = check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm)
            .expect("an overflowing scroll container");
        assert_eq!(ancestor.node_id, nid(INNER));
        assert_eq!(ancestor.dom_id, dom_id(0));
        assert!(!ancestor.scroll_x, "x does not overflow, so it is not scrollable");
        assert!(ancestor.scroll_y);
        // visible_rect = container origin + current scroll offset, container size.
        assert!(close(ancestor.visible_rect.origin.x, 5.0));
        assert!(close(ancestor.visible_rect.origin.y, 307.0));
        assert!(close(ancestor.visible_rect.size.width, 100.0));
        assert!(close(ancestor.visible_rect.size.height, 100.0));
    }

    #[test]
    fn check_if_scrollable_uses_virtual_scroll_size_over_content_rect() {
        let lr = layout_result(chain_dom(SCROLL_CSS));
        let mut sm = ScrollManager::new();
        // Content fits the container exactly => no overflow from `content_rect`...
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 100.0),
        );
        assert!(check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm).is_none());

        // ...but a VirtualView reports a much larger virtual size, which wins.
        sm.update_virtual_scroll_bounds(dom_id(0), nid(INNER), size(100.0, 10_000.0), None);
        let ancestor = check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm)
            .expect("virtual_scroll_size must drive the overflow check");
        assert!(ancestor.scroll_y);
        assert!(!ancestor.scroll_x);
    }

    #[test]
    fn check_if_scrollable_zero_sized_container_with_content_overflows() {
        let lr = layout_result(chain_dom(SCROLL_CSS));
        let mut sm = ScrollManager::new();
        register(&mut sm, INNER, LogicalRect::zero(), size(1.0, 1.0));
        let ancestor =
            check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm).expect("1px > 0px overflows");
        assert!(ancestor.scroll_x && ancestor.scroll_y);
    }

    #[test]
    fn check_if_scrollable_x_only_css_does_not_enable_the_y_axis() {
        // `.inner` declares only `overflow-x: scroll`. Per CSS Overflow 3 § 3.1 the
        // computed `overflow-y` of such a box becomes `auto` (i.e. scrollable), and
        // `MultiValue::<LayoutOverflow>::resolve_computed` implements exactly that —
        // but `check_if_scrollable` reads the *specified* values, so the y axis stays
        // non-scrollable here even though the content overflows it.
        let lr = layout_result(chain_dom(X_ONLY_CSS));
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(500.0, 500.0),
        );
        let ancestor = check_if_scrollable(dom_id(0), nid(INNER), &lr, &sm)
            .expect("overflow-x: scroll + overflowing content");
        assert!(ancestor.scroll_x);
        assert!(!ancestor.scroll_y);
    }

    // ==================================================================
    // find_scrollable_ancestors
    // ==================================================================

    #[test]
    fn find_ancestors_missing_dom_is_empty() {
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        let sm = ScrollManager::new();
        assert!(find_scrollable_ancestors(dom_id(0), nid(TARGET), &empty, &sm).is_empty());
    }

    #[test]
    fn find_ancestors_out_of_range_node_is_empty() {
        let (lrs, sm) = inner_only();
        assert!(find_scrollable_ancestors(dom_id(0), nid(OUT_OF_RANGE), &lrs, &sm).is_empty());
    }

    #[test]
    fn find_ancestors_of_the_root_is_empty() {
        // The root has no parent — the walk must terminate immediately.
        let (lrs, sm) = inner_only();
        assert!(find_scrollable_ancestors(dom_id(0), nid(0), &lrs, &sm).is_empty());
    }

    #[test]
    fn find_ancestors_excludes_the_target_itself() {
        // `.inner` is a live scroll container, but scrolling *itself* into view is
        // not the job of its own scrollport: the walk starts at the parent.
        let (lrs, sm) = inner_only();
        let ancestors = find_scrollable_ancestors(dom_id(0), nid(INNER), &lrs, &sm);
        assert!(ancestors.iter().all(|a| a.node_id != nid(INNER)));
    }

    #[test]
    fn find_ancestors_skips_styled_but_non_overflowing_containers() {
        // `.outer` is styled `overflow: scroll` but was never registered.
        let (lrs, sm) = inner_only();
        let ancestors = find_scrollable_ancestors(dom_id(0), nid(TARGET), &lrs, &sm);
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].node_id, nid(INNER));
    }

    #[test]
    fn find_ancestors_orders_innermost_first() {
        let lrs = window(layout_result(chain_dom(SCROLL_CSS)));
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 400.0, 100.0, 100.0),
            size(100.0, 1000.0),
        );
        register(
            &mut sm,
            OUTER,
            rect(0.0, 0.0, 200.0, 200.0),
            size(200.0, 2000.0),
        );

        let ancestors = find_scrollable_ancestors(dom_id(0), nid(TARGET), &lrs, &sm);
        assert_eq!(ancestors.len(), 2);
        assert_eq!(ancestors[0].node_id, nid(INNER), "innermost must come first");
        assert_eq!(ancestors[1].node_id, nid(OUTER));
    }

    // ==================================================================
    // scroll_rect_into_view — the core primitive
    // ==================================================================

    #[test]
    fn rect_into_view_without_scroll_containers_is_a_no_op() {
        let lrs = window(layout_result(chain_dom(SCROLL_CSS)));
        let mut sm = ScrollManager::new();
        let adjustments = scroll_rect_into_view(
            rect(0.0, 5000.0, 10.0, 10.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert!(adjustments.is_empty());
        assert!(sm.get_current_offset(dom_id(0), nid(INNER)).is_none());
    }

    #[test]
    fn rect_into_view_missing_dom_is_a_no_op() {
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        let mut sm = ScrollManager::new();
        let adjustments = scroll_rect_into_view(
            rect(0.0, 5000.0, 10.0, 10.0),
            dom_id(0),
            nid(TARGET),
            &empty,
            &mut sm,
            ScrollIntoViewOptions::nearest(),
            now(),
        );
        assert!(adjustments.is_empty());
    }

    #[test]
    fn rect_into_view_already_visible_target_does_not_scroll() {
        let (lrs, mut sm) = inner_only();
        let adjustments = scroll_rect_into_view(
            rect(0.0, 10.0, 50.0, 20.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            ScrollIntoViewOptions::nearest(),
            now(),
        );
        assert!(adjustments.is_empty());
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn rect_into_view_scrolls_and_reports_the_delta() {
        let (lrs, mut sm) = inner_only();
        let adjustments = scroll_rect_into_view(
            rect(0.0, 500.0, 50.0, 20.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].scroll_container_node_id, nid(INNER));
        assert!(close(adjustments[0].delta.y, 500.0));
        assert_eq!(adjustments[0].delta.x, 0.0, "x does not overflow");
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.y, 500.0), "{offset:?}");
    }

    #[test]
    fn rect_into_view_ignores_a_delta_at_exactly_the_threshold() {
        // The guard is `abs() > SCROLL_DELTA_THRESHOLD`, so a delta of exactly
        // 0.5px is *not* applied. 0.5 and 0.75 are both exact in binary f32.
        assert!(close(SCROLL_DELTA_THRESHOLD, 0.5));

        let (lrs, mut sm) = inner_only();
        let at_threshold = scroll_rect_into_view(
            rect(0.0, 0.5, 50.0, 20.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert!(at_threshold.is_empty(), "0.5px must be below the threshold");
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!(offset.y, 0.0);

        let above_threshold = scroll_rect_into_view(
            rect(0.0, 0.75, 50.0, 20.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(above_threshold.len(), 1, "0.75px must scroll");
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.y, 0.75), "{offset:?}");
    }

    #[test]
    fn rect_into_view_f32_max_rect_clamps_to_max_scroll() {
        let (lrs, mut sm) = inner_only();
        let adjustments = scroll_rect_into_view(
            rect(f32::MAX, f32::MAX, f32::MAX, f32::MAX),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(offset.x.is_finite() && offset.y.is_finite(), "{offset:?}");
        // max scroll y = content 1000 - container 100
        assert!(close(offset.y, 900.0), "{offset:?}");
    }

    #[test]
    fn rect_into_view_nan_rect_does_not_scroll_or_poison_the_offset() {
        let (lrs, mut sm) = inner_only();
        let adjustments = scroll_rect_into_view(
            LogicalRect::new(pos(f32::NAN, f32::NAN), size(f32::NAN, f32::NAN)),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        // `NaN.abs() > threshold` is false, so the adjustment is skipped entirely.
        assert!(adjustments.is_empty());
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(offset.x.is_finite() && offset.y.is_finite(), "{offset:?}");
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn rect_into_view_negative_rect_scrolls_back_and_clamps_at_zero() {
        let (lrs, mut sm) = inner_only();
        sm.set_scroll_position(dom_id(0), nid(INNER), pos(0.0, 300.0), now());

        let adjustments = scroll_rect_into_view(
            rect(0.0, -500.0, 10.0, 10.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        // visible rect starts at y = 0 + 300, so the reported delta is unclamped...
        assert!(close(adjustments[0].delta.y, -800.0), "{:?}", adjustments[0]);
        // ...while the stored offset is clamped into [0, max_scroll].
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!(offset.y, 0.0);
    }

    #[test]
    fn rect_into_view_auto_behavior_is_resolved_to_instant() {
        let (lrs, mut sm) = inner_only();
        let adjustments = scroll_rect_into_view(
            rect(0.0, 500.0, 50.0, 20.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Auto,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].behavior, ScrollIntoViewBehavior::Instant);
        assert!(!sm.has_active_animations());
    }

    #[test]
    fn rect_into_view_smooth_behavior_animates() {
        let (lrs, mut sm) = inner_only();
        let adjustments = scroll_rect_into_view(
            rect(0.0, 500.0, 50.0, 20.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Smooth,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0].behavior, ScrollIntoViewBehavior::Smooth);
        assert!(sm.has_active_animations());
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!(offset.y, 0.0, "a smooth scroll must not jump");
    }

    #[test]
    fn rect_into_view_walks_the_whole_scroll_chain_innermost_first() {
        let lrs = window(layout_result(chain_dom(SCROLL_CSS)));
        let mut sm = ScrollManager::new();
        // `.inner` sits at absolute y=400 inside `.outer`, which is itself scrolled
        // to the top. The target is deep inside `.inner`'s content at y=900.
        register(
            &mut sm,
            INNER,
            rect(0.0, 400.0, 100.0, 100.0),
            size(100.0, 1000.0),
        );
        register(
            &mut sm,
            OUTER,
            rect(0.0, 0.0, 200.0, 200.0),
            size(200.0, 2000.0),
        );

        let adjustments = scroll_rect_into_view(
            rect(0.0, 900.0, 50.0, 20.0),
            dom_id(0),
            nid(TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );

        assert_eq!(adjustments.len(), 2);
        assert_eq!(adjustments[0].scroll_container_node_id, nid(INNER));
        assert_eq!(adjustments[1].scroll_container_node_id, nid(OUTER));
        // inner: target 900 - visible 400 => 500
        assert!(close(adjustments[0].delta.y, 500.0), "{:?}", adjustments[0]);
        // outer: the rect is re-based by the inner scroll (900 - 500 = 400), so the
        // outer container only has to scroll the remaining 400.
        assert!(close(adjustments[1].delta.y, 400.0), "{:?}", adjustments[1]);

        let inner_offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        let outer_offset = sm.get_current_offset(dom_id(0), nid(OUTER)).unwrap();
        assert!(close(inner_offset.y, 500.0), "{inner_offset:?}");
        assert!(close(outer_offset.y, 400.0), "{outer_offset:?}");
    }

    // ==================================================================
    // scroll_node_into_view
    // ==================================================================

    #[test]
    fn node_into_view_missing_layout_results_is_empty() {
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        let mut sm = ScrollManager::new();
        assert!(scroll_node_into_view(
            dnid(0, TARGET),
            &empty,
            &mut sm,
            ScrollIntoViewOptions::nearest(),
            now(),
        )
        .is_empty());
    }

    #[test]
    fn node_into_view_null_node_is_empty() {
        let (lrs, mut sm) = inner_only();
        assert!(scroll_node_into_view(
            null_dnid(0),
            &lrs,
            &mut sm,
            ScrollIntoViewOptions::nearest(),
            now(),
        )
        .is_empty());
    }

    #[test]
    fn node_into_view_without_a_layout_box_is_empty() {
        // `inner_only()` has an empty layout tree, so `get_node_rect` finds nothing.
        let (lrs, mut sm) = inner_only();
        assert!(scroll_node_into_view(
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            ScrollIntoViewOptions::center(),
            now(),
        )
        .is_empty());
    }

    #[test]
    fn node_into_view_scrolls_the_nodes_bounding_rect() {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(0.0, 500.0), Some(size(50.0, 20.0)))],
        ));
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 1000.0),
        );

        let adjustments = scroll_node_into_view(
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.y, 500.0), "{offset:?}");
    }

    #[test]
    fn node_into_view_center_alignment_centers_the_node() {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(0.0, 500.0), Some(size(50.0, 20.0)))],
        ));
        let mut sm = ScrollManager::new();
        register(
            &mut sm,
            INNER,
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 1000.0),
        );

        let adjustments = scroll_node_into_view(
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Center,
                ScrollLogicalPosition::Center,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        // target center 510, container center 50 => 460
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.y, 460.0), "{offset:?}");
    }

    // ==================================================================
    // scroll_cursor_into_view — numeric
    // ==================================================================

    /// `.target` is a 100×1000 text box filling `.inner`'s content area.
    fn cursor_fixture(content: LogicalSize) -> (BTreeMap<DomId, DomLayoutResult>, ScrollManager) {
        let lrs = window(layout_result_with_boxes(
            chain_dom(SCROLL_CSS),
            &[(TARGET, pos(0.0, 0.0), Some(size(100.0, 1000.0)))],
        ));
        let mut sm = ScrollManager::new();
        register(&mut sm, INNER, rect(0.0, 0.0, 100.0, 100.0), content);
        (lrs, sm)
    }

    #[test]
    fn cursor_into_view_missing_node_is_empty() {
        let empty: BTreeMap<DomId, DomLayoutResult> = BTreeMap::new();
        let mut sm = ScrollManager::new();
        assert!(scroll_cursor_into_view(
            rect(0.0, 800.0, 2.0, 16.0),
            dnid(0, TARGET),
            &empty,
            &mut sm,
            ScrollIntoViewOptions::nearest(),
            now(),
        )
        .is_empty());
    }

    #[test]
    fn cursor_into_view_null_node_is_empty() {
        let (lrs, mut sm) = cursor_fixture(size(100.0, 1000.0));
        assert!(scroll_cursor_into_view(
            rect(0.0, 800.0, 2.0, 16.0),
            null_dnid(0),
            &lrs,
            &mut sm,
            ScrollIntoViewOptions::nearest(),
            now(),
        )
        .is_empty());
    }

    #[test]
    fn cursor_into_view_zero_rect_maps_to_the_node_origin() {
        // The node origin is the container origin, so a zero cursor rect there is
        // already visible and nothing scrolls.
        let (lrs, mut sm) = cursor_fixture(size(100.0, 1000.0));
        let adjustments = scroll_cursor_into_view(
            LogicalRect::zero(),
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            ScrollIntoViewOptions::nearest(),
            now(),
        );
        assert!(adjustments.is_empty());
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn cursor_into_view_transforms_local_coordinates_to_absolute() {
        let (lrs, mut sm) = cursor_fixture(size(100.0, 1000.0));
        // Cursor at node-local (0, 800) => absolute (0, 800).
        let adjustments = scroll_cursor_into_view(
            rect(0.0, 800.0, 2.0, 16.0),
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.y, 800.0), "{offset:?}");
    }

    #[test]
    fn cursor_into_view_scrolls_back_up_to_a_cursor_above_the_viewport() {
        let (lrs, mut sm) = cursor_fixture(size(100.0, 1000.0));
        sm.set_scroll_position(dom_id(0), nid(INNER), pos(0.0, 300.0), now());

        let adjustments = scroll_cursor_into_view(
            rect(0.0, 50.0, 2.0, 16.0),
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        // visible rect starts at 300, cursor at 50 => delta -250 => offset 50
        assert!(close(adjustments[0].delta.y, -250.0), "{:?}", adjustments[0]);
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(close(offset.y, 50.0), "{offset:?}");
    }

    #[test]
    fn cursor_into_view_nan_cursor_rect_does_not_scroll_or_panic() {
        let (lrs, mut sm) = cursor_fixture(size(100.0, 1000.0));
        let adjustments = scroll_cursor_into_view(
            LogicalRect::new(pos(f32::NAN, f32::NAN), size(2.0, 16.0)),
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert!(adjustments.is_empty());
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(offset.x.is_finite() && offset.y.is_finite(), "{offset:?}");
        assert_eq!((offset.x, offset.y), (0.0, 0.0));
    }

    #[test]
    fn cursor_into_view_f32_max_cursor_clamps_to_max_scroll_on_both_axes() {
        // 500×1000 of content in a 100×100 container => max scroll (400, 900).
        let (lrs, mut sm) = cursor_fixture(size(500.0, 1000.0));
        let adjustments = scroll_cursor_into_view(
            LogicalRect::new(pos(f32::MAX, f32::MAX), size(2.0, 16.0)),
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Start,
                ScrollLogicalPosition::Start,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        assert_eq!(adjustments.len(), 1);
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(offset.x.is_finite() && offset.y.is_finite(), "{offset:?}");
        assert!(close(offset.x, 400.0) && close(offset.y, 900.0), "{offset:?}");
    }

    #[test]
    fn cursor_into_view_infinite_cursor_rect_does_not_panic() {
        let (lrs, mut sm) = cursor_fixture(size(500.0, 1000.0));
        let adjustments = scroll_cursor_into_view(
            LogicalRect::new(
                pos(f32::NEG_INFINITY, f32::INFINITY),
                size(f32::INFINITY, f32::INFINITY),
            ),
            dnid(0, TARGET),
            &lrs,
            &mut sm,
            opts(
                ScrollLogicalPosition::Nearest,
                ScrollLogicalPosition::Nearest,
                ScrollIntoViewBehavior::Instant,
            ),
            now(),
        );
        // Whatever it decides, the stored offset must stay inside the bounds.
        let _ = adjustments;
        let offset = sm.get_current_offset(dom_id(0), nid(INNER)).unwrap();
        assert!(offset.x.is_finite() && offset.y.is_finite(), "{offset:?}");
        assert!(offset.x >= 0.0 && offset.x <= 400.0, "{offset:?}");
        assert!(offset.y >= 0.0 && offset.y <= 900.0, "{offset:?}");
    }
}
