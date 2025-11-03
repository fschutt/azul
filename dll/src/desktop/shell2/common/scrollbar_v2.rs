//! Cross-platform scrollbar interaction logic
//!
//! This module contains the unified scrollbar hit-testing and drag handling logic that is
//! shared across all platforms. Previously, this logic was duplicated in macOS, Windows,
//! and X11 implementations.

use azul_core::geom::LogicalPosition;
use azul_layout::{ScrollbarDragState, scroll::ScrollState};

use super::event_v2::PlatformWindowV2;
use crate::desktop::wr_translate2;

/// Scrollbar interaction result
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarAction {
    /// No scrollbar interaction
    None,
    /// Scrollbar thumb is being dragged
    DragStarted,
    /// Click on scrollbar track (not thumb)
    TrackClicked,
    /// Scrollbar interaction handled
    Handled,
}

/// Perform scrollbar hit-test at the given position.
///
/// Returns `Some(ScrollbarAction)` if a scrollbar was hit, `None` otherwise.
pub fn perform_scrollbar_hit_test<W: PlatformWindowV2>(
    window: &W,
    position: LogicalPosition,
) -> Option<(azul_core::dom::DomId, azul_core::dom::NodeId, ScrollbarAction)> {
    let layout_window = window.get_layout_window()?;
    let hit_test = layout_window.hover_manager.get_current()?;

    // Check if any hovered node has a scrollbar
    for (dom_id, node_hit_test) in &hit_test.hovered_nodes {
        for (node_id, _hit_item) in &node_hit_test.regular_hit_test_nodes {
            // Get scroll state for this node
            let scroll_state = layout_window.scroll_manager.get_scroll_state(*dom_id, *node_id)?;

            // Check if we hit a scrollbar
            let action = check_scrollbar_hit(scroll_state, position)?;
            
            return Some((*dom_id, *node_id, action));
        }
    }

    None
}

/// Check if a specific scroll state's scrollbars were hit at the given position.
fn check_scrollbar_hit(scroll_state: &ScrollState, position: LogicalPosition) -> Option<ScrollbarAction> {
    // Check vertical scrollbar
    if let Some(ref vertical_info) = scroll_state.vertical_scrollbar_info {
        if let Some(ref thumb_rect) = vertical_info.thumb_rect {
            // Check if we hit the thumb
            if thumb_rect.contains(position) {
                return Some(ScrollbarAction::DragStarted);
            }
            
            // Check if we hit the track (but not thumb)
            if let Some(ref track_rect) = vertical_info.track_rect {
                if track_rect.contains(position) {
                    return Some(ScrollbarAction::TrackClicked);
                }
            }
        }
    }

    // Check horizontal scrollbar
    if let Some(ref horizontal_info) = scroll_state.horizontal_scrollbar_info {
        if let Some(ref thumb_rect) = horizontal_info.thumb_rect {
            // Check if we hit the thumb
            if thumb_rect.contains(position) {
                return Some(ScrollbarAction::DragStarted);
            }
            
            // Check if we hit the track (but not thumb)
            if let Some(ref track_rect) = horizontal_info.track_rect {
                if track_rect.contains(position) {
                    return Some(ScrollbarAction::TrackClicked);
                }
            }
        }
    }

    None
}

/// Handle scrollbar click (start drag or track click).
///
/// Returns `true` if a scrollbar was clicked.
pub fn handle_scrollbar_click<W: PlatformWindowV2>(
    window: &mut W,
    position: LogicalPosition,
    scrollbar_drag_state: &mut Option<ScrollbarDragState>,
) -> bool {
    let hit_result = match perform_scrollbar_hit_test(window, position) {
        Some(result) => result,
        None => return false,
    };

    let (dom_id, node_id, action) = hit_result;

    match action {
        ScrollbarAction::DragStarted => {
            // Start dragging the scrollbar thumb
            *scrollbar_drag_state = Some(ScrollbarDragState {
                dom_id,
                node_id,
                start_mouse_position: position,
                original_scroll_position: get_current_scroll_position(window, dom_id, node_id),
            });
            true
        }
        ScrollbarAction::TrackClicked => {
            // Handle track click (page up/down)
            handle_track_click(window, dom_id, node_id, position);
            true
        }
        _ => false,
    }
}

/// Handle scrollbar drag (update scroll position based on mouse movement).
pub fn handle_scrollbar_drag<W: PlatformWindowV2>(
    window: &mut W,
    position: LogicalPosition,
    drag_state: &ScrollbarDragState,
) {
    let layout_window = match window.get_layout_window_mut() {
        Some(lw) => lw,
        None => return,
    };

    let scroll_state = match layout_window.scroll_manager.get_scroll_state(drag_state.dom_id, drag_state.node_id) {
        Some(state) => state,
        None => return,
    };

    // Calculate how far the mouse has moved since drag started
    let delta_x = position.x - drag_state.start_mouse_position.x;
    let delta_y = position.y - drag_state.start_mouse_position.y;

    // Convert pixel delta to scroll delta based on scrollbar size
    let new_scroll_x = calculate_scroll_from_drag(
        drag_state.original_scroll_position.x,
        delta_x,
        scroll_state.horizontal_scrollbar_info.as_ref(),
    );

    let new_scroll_y = calculate_scroll_from_drag(
        drag_state.original_scroll_position.y,
        delta_y,
        scroll_state.vertical_scrollbar_info.as_ref(),
    );

    // Update scroll position
    layout_window.scroll_manager.set_scroll_position(
        drag_state.dom_id,
        drag_state.node_id,
        new_scroll_x,
        new_scroll_y,
    );

    window.mark_frame_needs_regeneration();
}

/// Calculate new scroll position from drag delta.
fn calculate_scroll_from_drag(
    original_scroll: f32,
    pixel_delta: f32,
    scrollbar_info: Option<&azul_layout::ScrollbarInfo>,
) -> f32 {
    let info = match scrollbar_info {
        Some(info) => info,
        None => return original_scroll,
    };

    // Get track length (distance the thumb can travel)
    let track_length = match &info.track_rect {
        Some(track) => {
            if info.is_horizontal {
                track.size.width
            } else {
                track.size.height
            }
        }
        None => return original_scroll,
    };

    // Get thumb length
    let thumb_length = match &info.thumb_rect {
        Some(thumb) => {
            if info.is_horizontal {
                thumb.size.width
            } else {
                thumb.size.height
            }
        }
        None => return original_scroll,
    };

    // Calculate usable track length (track minus thumb)
    let usable_track_length = track_length - thumb_length;
    if usable_track_length <= 0.0 {
        return original_scroll;
    }

    // Calculate content that's scrollable
    let scrollable_content = info.content_size - info.viewport_size;
    if scrollable_content <= 0.0 {
        return original_scroll;
    }

    // Convert pixel delta to scroll delta
    let scroll_ratio = pixel_delta / usable_track_length;
    let scroll_delta = scroll_ratio * scrollable_content;

    // Clamp to valid range
    (original_scroll + scroll_delta).max(0.0).min(scrollable_content)
}

/// Handle click on scrollbar track (not thumb) - page up/down.
fn handle_track_click<W: PlatformWindowV2>(
    window: &mut W,
    dom_id: azul_core::dom::DomId,
    node_id: azul_core::dom::NodeId,
    position: LogicalPosition,
) {
    let layout_window = match window.get_layout_window_mut() {
        Some(lw) => lw,
        None => return,
    };

    let scroll_state = match layout_window.scroll_manager.get_scroll_state(dom_id, node_id) {
        Some(state) => state,
        None => return,
    };

    let current_scroll = get_current_scroll_position_from_state(scroll_state);

    // Determine if we should scroll up/left or down/right
    // Check vertical scrollbar
    if let Some(ref vertical_info) = scroll_state.vertical_scrollbar_info {
        if let Some(ref thumb_rect) = vertical_info.thumb_rect {
            if let Some(ref track_rect) = vertical_info.track_rect {
                if track_rect.contains(position) {
                    // Page size is typically the viewport size
                    let page_size = vertical_info.viewport_size;
                    
                    let new_scroll_y = if position.y < thumb_rect.origin.y {
                        // Clicked above thumb - scroll up (decrease scroll position)
                        (current_scroll.y - page_size).max(0.0)
                    } else {
                        // Clicked below thumb - scroll down (increase scroll position)
                        let max_scroll = (vertical_info.content_size - vertical_info.viewport_size).max(0.0);
                        (current_scroll.y + page_size).min(max_scroll)
                    };

                    layout_window.scroll_manager.set_scroll_position(
                        dom_id,
                        node_id,
                        current_scroll.x,
                        new_scroll_y,
                    );

                    window.mark_frame_needs_regeneration();
                    return;
                }
            }
        }
    }

    // Check horizontal scrollbar
    if let Some(ref horizontal_info) = scroll_state.horizontal_scrollbar_info {
        if let Some(ref thumb_rect) = horizontal_info.thumb_rect {
            if let Some(ref track_rect) = horizontal_info.track_rect {
                if track_rect.contains(position) {
                    let page_size = horizontal_info.viewport_size;
                    
                    let new_scroll_x = if position.x < thumb_rect.origin.x {
                        // Clicked left of thumb - scroll left
                        (current_scroll.x - page_size).max(0.0)
                    } else {
                        // Clicked right of thumb - scroll right
                        let max_scroll = (horizontal_info.content_size - horizontal_info.viewport_size).max(0.0);
                        (current_scroll.x + page_size).min(max_scroll)
                    };

                    layout_window.scroll_manager.set_scroll_position(
                        dom_id,
                        node_id,
                        new_scroll_x,
                        current_scroll.y,
                    );

                    window.mark_frame_needs_regeneration();
                }
            }
        }
    }
}

/// Get current scroll position for a node.
fn get_current_scroll_position<W: PlatformWindowV2>(
    window: &W,
    dom_id: azul_core::dom::DomId,
    node_id: azul_core::dom::NodeId,
) -> LogicalPosition {
    let layout_window = match window.get_layout_window() {
        Some(lw) => lw,
        None => return LogicalPosition::zero(),
    };

    let scroll_state = match layout_window.scroll_manager.get_scroll_state(dom_id, node_id) {
        Some(state) => state,
        None => return LogicalPosition::zero(),
    };

    get_current_scroll_position_from_state(scroll_state)
}

/// Get current scroll position from scroll state.
fn get_current_scroll_position_from_state(scroll_state: &ScrollState) -> LogicalPosition {
    LogicalPosition::new(
        scroll_state.horizontal_scrollbar_info.as_ref()
            .map(|info| info.current_scroll_position)
            .unwrap_or(0.0),
        scroll_state.vertical_scrollbar_info.as_ref()
            .map(|info| info.current_scroll_position)
            .unwrap_or(0.0),
    )
}
