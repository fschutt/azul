//! Pure scroll state management — the single source of truth for scroll offsets.
//!
//! # Architecture
//!
//! `ScrollManager` is the exclusive owner of all scroll state. Other modules
//! interact with scrolling only through its public API:
//!
//! - **Platform shell** (macos/events.rs, etc.): Calls `record_scroll_from_hit_test()`
//!   to queue trackpad/mouse wheel input for the physics timer.
//! - **Scroll physics timer** (scroll_timer.rs): Consumes inputs via `ScrollInputQueue`,
//!   applies physics, and pushes `CallbackChange::ScrollTo` for each updated node.
//! - **Event processing** (event_v2.rs): Processes `ScrollTo` changes, sets scroll
//!   positions, and checks IFrame re-invocation transparently.
//! - **Gesture manager** (gesture.rs): Tracks drag state and emits
//!   `AutoScrollDirection` — does NOT modify scroll offsets directly.
//! - **Render loop**: Calls `tick()` every frame to advance easing animations.
//! - **WebRender sync** (wr_translate2.rs): Reads offsets via
//!   `get_scroll_states_for_dom()` to synchronize scroll frames.
//! - **Layout** (cache.rs): Registers scroll nodes via
//!   `register_or_update_scroll_node()` after layout completes.
//!
//! # Scroll Flow
//!
//! ```text
//! Platform Event Handler
//!   → record_scroll_from_hit_test() → ScrollInputQueue
//!   → starts SCROLL_MOMENTUM_TIMER_ID if not running
//!
//! Timer fires (every ~16ms):
//!   → queue.take_all() → physics integration
//!   → push_change(CallbackChange::ScrollTo)
//!
//! ScrollTo processing (event_v2.rs):
//!   → scroll_manager.set_scroll_position()
//!   → iframe_manager.check_reinvoke() (transparent IFrame support)
//!   → repaint
//! ```
//!
//! This module provides:
//! - Smooth scroll animations with easing
//! - Event source classification for scroll events
//! - Scrollbar geometry and hit-testing
//! - ExternalScrollId mapping for WebRender integration
//! - Virtual scroll bounds for IFrame nodes

use alloc::collections::BTreeMap;
#[cfg(feature = "std")]
use alloc::vec::Vec;

use azul_core::{
    dom::{DomId, NodeId, ScrollbarOrientation},
    events::EasingFunction,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{ExternalScrollId, ScrollPosition},
    styled_dom::NodeHierarchyItemId,
    task::{Duration, Instant},
};

#[cfg(feature = "std")]
use std::sync::{Arc, Mutex};

use crate::managers::hover::InputPointId;
use crate::solver3::scrollbar::compute_scrollbar_geometry;

// ============================================================================
// Scroll Input Types (for timer-based physics architecture)
// ============================================================================

/// Classifies the source of a scroll input event.
///
/// This determines how the scroll physics timer processes the input:
/// - `TrackpadContinuous`: The OS already applies momentum — set position directly
/// - `WheelDiscrete`: Mouse wheel clicks — apply as impulse with momentum decay
/// - `Programmatic`: API-driven scroll — apply with optional easing animation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollInputSource {
    /// Continuous trackpad gesture (macOS precise scrolling).
    /// Position is set directly — the OS handles momentum/physics.
    TrackpadContinuous,
    /// Discrete mouse wheel steps (Windows/Linux mouse wheel).
    /// Applied as velocity impulse with momentum decay.
    WheelDiscrete,
    /// Programmatic scroll (scrollTo API, keyboard Page Up/Down).
    /// Applied with optional easing animation.
    Programmatic,
}

/// A single scroll input event to be processed by the physics timer.
///
/// Scroll inputs are recorded by the platform event handler and consumed
/// by the scroll physics timer callback. This decouples input recording
/// from physics simulation.
#[derive(Debug, Clone)]
pub struct ScrollInput {
    /// DOM containing the scrollable node
    pub dom_id: DomId,
    /// Target scroll node
    pub node_id: NodeId,
    /// Scroll delta (positive = scroll down/right)
    pub delta: LogicalPosition,
    /// When this input was recorded
    pub timestamp: Instant,
    /// How this input should be processed
    pub source: ScrollInputSource,
}

/// Thread-safe queue for scroll inputs, shared between event handlers and timer callbacks.
///
/// Event handlers push inputs, the physics timer pops them. Protected by a Mutex
/// so that the timer callback (which only has `&CallbackInfo` / `*const LayoutWindow`)
/// can still consume pending inputs without needing `&mut`.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Default)]
pub struct ScrollInputQueue {
    inner: Arc<Mutex<Vec<ScrollInput>>>,
}

#[cfg(feature = "std")]
impl ScrollInputQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Push a new scroll input (called from platform event handler)
    pub fn push(&self, input: ScrollInput) {
        if let Ok(mut queue) = self.inner.lock() {
            queue.push(input);
        }
    }

    /// Take all pending inputs (called from timer callback)
    pub fn take_all(&self) -> Vec<ScrollInput> {
        if let Ok(mut queue) = self.inner.lock() {
            core::mem::take(&mut *queue)
        } else {
            Vec::new()
        }
    }

    /// Take at most `max_events` recent inputs, sorted by timestamp (newest last).
    /// Any older events beyond `max_events` are discarded.
    /// This prevents the physics timer from processing an unbounded backlog.
    pub fn take_recent(&self, max_events: usize) -> Vec<ScrollInput> {
        if let Ok(mut queue) = self.inner.lock() {
            let mut events = core::mem::take(&mut *queue);
            if events.len() > max_events {
                // Sort by timestamp ascending (oldest first), keep last N
                events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                events.drain(..events.len() - max_events);
            }
            events
        } else {
            Vec::new()
        }
    }

    /// Check if there are pending inputs without consuming them
    pub fn has_pending(&self) -> bool {
        self.inner
            .lock()
            .map(|q| !q.is_empty())
            .unwrap_or(false)
    }
}

// Scrollbar Component Types

/// Which component of a scrollbar was hit during hit-testing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollbarComponent {
    /// The track (background) of the scrollbar
    Track,
    /// The draggable thumb (indicator of current scroll position)
    Thumb,
    /// Top/left button (scrolls by one page up/left)
    TopButton,
    /// Bottom/right button (scrolls by one page down/right)
    BottomButton,
}

/// Scrollbar geometry state (calculated per frame, used for hit-testing and rendering)
#[derive(Debug, Clone)]
pub struct ScrollbarState {
    /// Is this scrollbar visible? (content larger than container)
    pub visible: bool,
    /// Orientation
    pub orientation: ScrollbarOrientation,
    /// Base size (1:1 square, width = height). This is the unscaled size.
    pub base_size: f32,
    /// Scale transform to apply (calculated from container size)
    pub scale: LogicalPosition, // x = width scale, y = height scale
    /// Thumb position ratio (0.0 = top/left, 1.0 = bottom/right)
    pub thumb_position_ratio: f32,
    /// Thumb size ratio (0.0 = invisible, 1.0 = entire track)
    pub thumb_size_ratio: f32,
    /// Position of the scrollbar in the container (for hit-testing)
    pub track_rect: LogicalRect,
    /// Button size (square: button_size × button_size)
    pub button_size: f32,
    /// Usable track length after subtracting buttons
    pub usable_track_length: f32,
    /// Thumb length in pixels
    pub thumb_length: f32,
    /// Thumb offset from start of usable track region
    pub thumb_offset: f32,
}

impl ScrollbarState {
    /// Determine which component was hit at the given local position (relative to track_rect
    /// origin). Uses the shared geometry values (button_size, usable_track_length, thumb_length,
    /// thumb_offset) for consistent hit-testing.
    pub fn hit_test_component(&self, local_pos: LogicalPosition) -> ScrollbarComponent {
        match self.orientation {
            ScrollbarOrientation::Vertical => {
                // Top button
                if local_pos.y < self.button_size {
                    return ScrollbarComponent::TopButton;
                }

                // Bottom button
                let track_height = self.track_rect.size.height;
                if local_pos.y > track_height - self.button_size {
                    return ScrollbarComponent::BottomButton;
                }

                // Thumb region starts after top button
                let thumb_y_start = self.button_size + self.thumb_offset;
                let thumb_y_end = thumb_y_start + self.thumb_length;

                if local_pos.y >= thumb_y_start && local_pos.y <= thumb_y_end {
                    ScrollbarComponent::Thumb
                } else {
                    ScrollbarComponent::Track
                }
            }
            ScrollbarOrientation::Horizontal => {
                // Left button
                if local_pos.x < self.button_size {
                    return ScrollbarComponent::TopButton;
                }

                // Right button
                let track_width = self.track_rect.size.width;
                if local_pos.x > track_width - self.button_size {
                    return ScrollbarComponent::BottomButton;
                }

                // Thumb region starts after left button
                let thumb_x_start = self.button_size + self.thumb_offset;
                let thumb_x_end = thumb_x_start + self.thumb_length;

                if local_pos.x >= thumb_x_start && local_pos.x <= thumb_x_end {
                    ScrollbarComponent::Thumb
                } else {
                    ScrollbarComponent::Track
                }
            }
        }
    }
}

/// Result of a scrollbar hit-test
///
/// Contains information about which scrollbar component was hit
/// and the position relative to both the track and the window.
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarHit {
    /// DOM containing the scrollable node
    pub dom_id: DomId,
    /// Node with the scrollbar
    pub node_id: NodeId,
    /// Whether this is a vertical or horizontal scrollbar
    pub orientation: ScrollbarOrientation,
    /// Which component was hit (track, thumb, buttons)
    pub component: ScrollbarComponent,
    /// Position relative to track_rect origin
    pub local_position: LogicalPosition,
    /// Original global window position
    pub global_position: LogicalPosition,
}

// Core Scroll Manager

/// Manages all scroll state and animations for a window
#[derive(Debug, Clone, Default)]
pub struct ScrollManager {
    /// Maps (DomId, NodeId) to their scroll state
    states: BTreeMap<(DomId, NodeId), AnimatedScrollState>,
    /// Maps (DomId, NodeId) to WebRender ExternalScrollId
    external_scroll_ids: BTreeMap<(DomId, NodeId), ExternalScrollId>,
    /// Counter for generating unique ExternalScrollId values
    next_external_scroll_id: u64,
    /// Scrollbar geometry states (calculated per frame)
    scrollbar_states: BTreeMap<(DomId, NodeId, ScrollbarOrientation), ScrollbarState>,
    /// Thread-safe queue for scroll inputs (shared with timer callbacks)
    #[cfg(feature = "std")]
    pub scroll_input_queue: ScrollInputQueue,
}

/// The complete scroll state for a single node (with animation support)
#[derive(Debug, Clone)]
pub struct AnimatedScrollState {
    /// Current scroll offset (live, may be animating)
    pub current_offset: LogicalPosition,
    /// Ongoing smooth scroll animation, if any
    pub animation: Option<ScrollAnimation>,
    /// Last time scroll activity occurred (for fading scrollbars)
    pub last_activity: Instant,
    /// Bounds of the scrollable container
    pub container_rect: LogicalRect,
    /// Bounds of the total content (for calculating scroll limits)
    pub content_rect: LogicalRect,
    /// Virtual scroll size from IFrame callback (if this node hosts an IFrame).
    /// When set, clamp logic uses this instead of content_rect for max scroll bounds.
    pub virtual_scroll_size: Option<LogicalSize>,
    /// Virtual scroll offset from IFrame callback
    pub virtual_scroll_offset: Option<LogicalPosition>,
    /// Per-node overscroll behavior for X axis (from CSS `overscroll-behavior-x`)
    pub overscroll_behavior_x: azul_css::props::style::scrollbar::OverscrollBehavior,
    /// Per-node overscroll behavior for Y axis (from CSS `overscroll-behavior-y`)
    pub overscroll_behavior_y: azul_css::props::style::scrollbar::OverscrollBehavior,
    /// Per-node overflow scrolling mode (from CSS `-azul-overflow-scrolling`)
    pub overflow_scrolling: azul_css::props::style::scrollbar::OverflowScrolling,
    /// CSS-resolved scrollbar thickness (from `scrollbar-width` property).
    /// Used for rendering and hit-testing. Defaults to 16.0 if not set.
    pub scrollbar_thickness: f32,
    /// Whether this node also needs a horizontal scrollbar (affects vertical geometry)
    pub has_horizontal_scrollbar: bool,
    /// Whether this node also needs a vertical scrollbar (affects horizontal geometry)
    pub has_vertical_scrollbar: bool,
}

/// Details of an in-progress smooth scroll animation
#[derive(Debug, Clone)]
struct ScrollAnimation {
    /// When the animation started
    start_time: Instant,
    /// Total duration of the animation
    duration: Duration,
    /// Scroll offset at animation start
    start_offset: LogicalPosition,
    /// Target scroll offset at animation end
    target_offset: LogicalPosition,
    /// Easing function for interpolation
    easing: EasingFunction,
}

/// Read-only snapshot of a scroll node's state, returned by CallbackInfo queries.
///
/// Provides all the information a timer callback needs to compute scroll physics
/// without requiring mutable access to the ScrollManager.
#[derive(Debug, Clone)]
pub struct ScrollNodeInfo {
    /// Current scroll offset
    pub current_offset: LogicalPosition,
    /// Container (viewport) bounds
    pub container_rect: LogicalRect,
    /// Content bounds (total scrollable area)
    pub content_rect: LogicalRect,
    /// Maximum scroll in X direction
    pub max_scroll_x: f32,
    /// Maximum scroll in Y direction
    pub max_scroll_y: f32,
    /// Per-node overscroll behavior for X axis
    pub overscroll_behavior_x: azul_css::props::style::scrollbar::OverscrollBehavior,
    /// Per-node overscroll behavior for Y axis
    pub overscroll_behavior_y: azul_css::props::style::scrollbar::OverscrollBehavior,
    /// Per-node overflow scrolling mode (auto vs touch)
    pub overflow_scrolling: azul_css::props::style::scrollbar::OverflowScrolling,
}

/// Result of a scroll tick, indicating what actions are needed
#[derive(Debug, Default)]
pub struct ScrollTickResult {
    /// If true, a repaint is needed (scroll offset changed)
    pub needs_repaint: bool,
    /// Nodes whose scroll position was updated this tick
    pub updated_nodes: Vec<(DomId, NodeId)>,
}

// ScrollManager Implementation

impl ScrollManager {
    /// Creates a new empty ScrollManager
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Input Recording API (timer-based architecture)
    // ========================================================================

    /// Records a scroll input event into the shared queue.
    ///
    /// This is the primary entry point for platform event handlers. Instead of
    /// directly modifying scroll positions, the input is queued for the scroll
    /// physics timer to process. This decouples input from physics simulation.
    ///
    /// Returns `true` if the physics timer should be started (i.e., there are
    /// now pending inputs and no timer is running yet).
    #[cfg(feature = "std")]
    pub fn record_scroll_input(&mut self, input: ScrollInput) -> bool {
        let was_empty = !self.scroll_input_queue.has_pending();
        self.scroll_input_queue.push(input);
        was_empty // caller should start timer if this returns true
    }

    /// High-level entry point for platform event handlers: performs hit-test lookup
    /// and queues the input for the physics timer, instead of directly modifying offsets.
    ///
    /// Returns `Some((dom_id, node_id, should_start_timer))` if a scrollable node was found.
    /// The caller should start `SCROLL_MOMENTUM_TIMER_ID` when `should_start_timer` is true.
    #[cfg(feature = "std")]
    pub fn record_scroll_from_hit_test(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        source: ScrollInputSource,
        hover_manager: &crate::managers::hover::HoverManager,
        input_point_id: &InputPointId,
        now: Instant,
    ) -> Option<(DomId, NodeId, bool)> {
        let hit_test = hover_manager.get_current(input_point_id)?;

        for (dom_id, hit_node) in &hit_test.hovered_nodes {
            for (node_id, _scroll_item) in &hit_node.scroll_hit_test_nodes {
                let scrollable = self.is_node_scrollable(*dom_id, *node_id);
                if !scrollable {
                    continue;
                }
                let input = ScrollInput {
                    dom_id: *dom_id,
                    node_id: *node_id,
                    delta: LogicalPosition { x: delta_x, y: delta_y },
                    timestamp: now,
                    source,
                };
                let should_start_timer = self.record_scroll_input(input);
                return Some((*dom_id, *node_id, should_start_timer));
            }
        }

        None
    }

    /// Get a clone of the scroll input queue (for sharing with timer callbacks).
    ///
    /// The timer callback stores this in its RefAny data and calls `take_all()`
    /// each tick to consume pending inputs.
    #[cfg(feature = "std")]
    pub fn get_input_queue(&self) -> ScrollInputQueue {
        self.scroll_input_queue.clone()
    }

    /// Advances scroll animations by one tick, returns repaint info
    pub fn tick(&mut self, now: Instant) -> ScrollTickResult {
        let mut result = ScrollTickResult::default();
        for ((dom_id, node_id), state) in self.states.iter_mut() {
            if let Some(anim) = &state.animation {
                let elapsed = now.duration_since(&anim.start_time);
                let t = elapsed.div(&anim.duration).min(1.0);
                let eased_t = apply_easing(t, anim.easing);

                state.current_offset = LogicalPosition {
                    x: anim.start_offset.x + (anim.target_offset.x - anim.start_offset.x) * eased_t,
                    y: anim.start_offset.y + (anim.target_offset.y - anim.start_offset.y) * eased_t,
                };
                result.needs_repaint = true;
                result.updated_nodes.push((*dom_id, *node_id));

                if t >= 1.0 {
                    state.animation = None;
                }
            }
        }
        result
    }

    /// Finds the closest scroll-container ancestor for a given node.
    ///
    /// Walks up the node hierarchy to find a node that is registered as a
    /// scrollable node in this ScrollManager. Returns `None` if no scrollable
    /// ancestor is found.
    pub fn find_scroll_parent(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        node_hierarchy: &[azul_core::styled_dom::NodeHierarchyItem],
    ) -> Option<NodeId> {
        let mut current = Some(node_id);
        while let Some(nid) = current {
            if self.states.contains_key(&(dom_id, nid)) && nid != node_id {
                return Some(nid);
            }
            current = node_hierarchy
                .get(nid.index())
                .and_then(|item| item.parent_id());
        }
        None
    }

    /// Check if a node is scrollable (has overflow:scroll/auto and overflowing content)
    ///
    /// Uses `virtual_scroll_size` (when set) instead of `content_rect` for the
    /// overflow check, so IFrame nodes with large virtual content are correctly
    /// identified as scrollable even when only a small subset is rendered.
    fn is_node_scrollable(&self, dom_id: DomId, node_id: NodeId) -> bool {
        let result = self.states.get(&(dom_id, node_id)).map_or(false, |state| {
            let effective_width = state.virtual_scroll_size
                .map(|s| s.width)
                .unwrap_or(state.content_rect.size.width);
            let effective_height = state.virtual_scroll_size
                .map(|s| s.height)
                .unwrap_or(state.content_rect.size.height);
            let has_horizontal = effective_width > state.container_rect.size.width;
            let has_vertical = effective_height > state.container_rect.size.height;
            has_horizontal || has_vertical
        });
        result
    }

    /// Sets scroll position immediately (no animation)
    pub fn set_scroll_position(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
        now: Instant,
    ) {
        let state = self
            .states
            .entry((dom_id, node_id))
            .or_insert_with(|| AnimatedScrollState::new(now.clone()));
        state.current_offset = state.clamp(position);
        state.animation = None;
        state.last_activity = now;
    }

    /// Scrolls by a delta amount with animation
    pub fn scroll_by(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        delta: LogicalPosition,
        duration: Duration,
        easing: EasingFunction,
        now: Instant,
    ) {
        let current = self.get_current_offset(dom_id, node_id).unwrap_or_default();
        let target = LogicalPosition {
            x: current.x + delta.x,
            y: current.y + delta.y,
        };
        self.scroll_to(dom_id, node_id, target, duration, easing, now);
    }

    /// Scrolls to an absolute position with animation
    ///
    /// If duration is zero, the position is set immediately without animation.
    pub fn scroll_to(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        target: LogicalPosition,
        duration: Duration,
        easing: EasingFunction,
        now: Instant,
    ) {
        // For zero duration, set position immediately
        let is_zero = match &duration {
            Duration::System(s) => s.secs == 0 && s.nanos == 0,
            Duration::Tick(t) => t.tick_diff == 0,
        };

        if is_zero {
            self.set_scroll_position(dom_id, node_id, target, now);
            return;
        }

        let state = self
            .states
            .entry((dom_id, node_id))
            .or_insert_with(|| AnimatedScrollState::new(now.clone()));
        let clamped_target = state.clamp(target);
        state.animation = Some(ScrollAnimation {
            start_time: now.clone(),
            duration,
            start_offset: state.current_offset,
            target_offset: clamped_target,
            easing,
        });
        state.last_activity = now;
    }

    /// Updates the container and content bounds for a scrollable node
    pub fn update_node_bounds(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        container_rect: LogicalRect,
        content_rect: LogicalRect,
        now: Instant,
    ) {
        let state = self
            .states
            .entry((dom_id, node_id))
            .or_insert_with(|| AnimatedScrollState::new(now));
        state.container_rect = container_rect;
        state.content_rect = content_rect;
        state.current_offset = state.clamp(state.current_offset);
    }

    /// Updates virtual scroll bounds for an IFrame node.
    ///
    /// Called after IFrame callback returns to propagate the virtual content size
    /// to the ScrollManager. Clamp logic then uses `virtual_scroll_size` (when set)
    /// instead of `content_rect` for max scroll bounds.
    ///
    /// If no scroll state exists yet for this node (because `register_or_update_scroll_node`
    /// hasn't been called yet), this creates a default state so the virtual size is preserved.
    pub fn update_virtual_scroll_bounds(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        virtual_scroll_size: LogicalSize,
        virtual_scroll_offset: Option<LogicalPosition>,
    ) {
        let key = (dom_id, node_id);
        let state = self.states.entry(key).or_insert_with(|| {
            AnimatedScrollState {
                current_offset: LogicalPosition::zero(),
                animation: None,
                last_activity: std::time::Instant::now().into(),
                container_rect: LogicalRect::zero(),
                content_rect: LogicalRect::zero(),
                virtual_scroll_size: None,
                virtual_scroll_offset: None,
                overscroll_behavior_x: azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
                overscroll_behavior_y: azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
                overflow_scrolling: azul_css::props::style::scrollbar::OverflowScrolling::Auto,
                scrollbar_thickness: 16.0,
                has_horizontal_scrollbar: false,
                has_vertical_scrollbar: false,
            }
        });
        state.virtual_scroll_size = Some(virtual_scroll_size);
        state.virtual_scroll_offset = virtual_scroll_offset;
        // Re-clamp with new virtual bounds
        state.current_offset = state.clamp(state.current_offset);
    }

    /// Returns the current scroll offset for a node
    pub fn get_current_offset(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalPosition> {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.current_offset)
    }

    /// Returns the timestamp of last scroll activity for a node
    pub fn get_last_activity_time(&self, dom_id: DomId, node_id: NodeId) -> Option<Instant> {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.last_activity.clone())
    }

    /// Returns the internal scroll state for a node
    pub fn get_scroll_state(&self, dom_id: DomId, node_id: NodeId) -> Option<&AnimatedScrollState> {
        self.states.get(&(dom_id, node_id))
    }

    /// Returns a read-only snapshot of a scroll node's state.
    ///
    /// This is the preferred way for timer callbacks to query scroll state,
    /// since they only have `&CallbackInfo` (read-only access).
    ///
    /// When `virtual_scroll_size` is set (for IFrame nodes), the max scroll
    /// bounds are computed from the virtual size instead of `content_rect`.
    pub fn get_scroll_node_info(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<ScrollNodeInfo> {
        let state = self.states.get(&(dom_id, node_id))?;
        let effective_content_width = state.virtual_scroll_size
            .map(|s| s.width)
            .unwrap_or(state.content_rect.size.width);
        let effective_content_height = state.virtual_scroll_size
            .map(|s| s.height)
            .unwrap_or(state.content_rect.size.height);
        let max_x = (effective_content_width - state.container_rect.size.width).max(0.0);
        let max_y = (effective_content_height - state.container_rect.size.height).max(0.0);
        Some(ScrollNodeInfo {
            current_offset: state.current_offset,
            container_rect: state.container_rect,
            content_rect: state.content_rect,
            max_scroll_x: max_x,
            max_scroll_y: max_y,
            overscroll_behavior_x: state.overscroll_behavior_x,
            overscroll_behavior_y: state.overscroll_behavior_y,
            overflow_scrolling: state.overflow_scrolling,
        })
    }

    /// Returns all scroll positions for nodes in a specific DOM
    pub fn get_scroll_states_for_dom(&self, dom_id: DomId) -> BTreeMap<NodeId, ScrollPosition> {
        self.states
            .iter()
            .filter(|((d, _), _)| *d == dom_id)
            .map(|((_, node_id), state)| {
                (
                    *node_id,
                    ScrollPosition {
                        parent_rect: state.container_rect,
                        children_rect: LogicalRect::new(
                            state.current_offset,
                            state.content_rect.size,
                        ),
                    },
                )
            })
            .collect()
    }

    /// Registers or updates a scrollable node with its container and content sizes.
    /// This should be called after layout for each node that has overflow:scroll or overflow:auto
    /// with overflowing content.
    ///
    /// If the node already exists, updates container/content rects without changing scroll offset.
    /// If the node is new, initializes with zero scroll offset.
    pub fn register_or_update_scroll_node(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        container_rect: LogicalRect,
        content_size: LogicalSize,
        now: Instant,
        scrollbar_thickness: f32,
        has_horizontal_scrollbar: bool,
        has_vertical_scrollbar: bool,
    ) {
        let key = (dom_id, node_id);

        let content_rect = LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        };

        if let Some(existing) = self.states.get_mut(&key) {
            // Update rects, keep scroll offset
            existing.container_rect = container_rect;
            existing.content_rect = content_rect;
            existing.scrollbar_thickness = scrollbar_thickness;
            existing.has_horizontal_scrollbar = has_horizontal_scrollbar;
            existing.has_vertical_scrollbar = has_vertical_scrollbar;
            // Re-clamp current offset to new bounds
            existing.current_offset = existing.clamp(existing.current_offset);
        } else {
            // New scrollable node
            self.states.insert(
                key,
                AnimatedScrollState {
                    current_offset: LogicalPosition::zero(),
                    animation: None,
                    last_activity: now,
                    container_rect,
                    content_rect,
                    virtual_scroll_size: None,
                    virtual_scroll_offset: None,
                    overscroll_behavior_x: azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
                    overscroll_behavior_y: azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
                    overflow_scrolling: azul_css::props::style::scrollbar::OverflowScrolling::Auto,
                    scrollbar_thickness,
                    has_horizontal_scrollbar,
                    has_vertical_scrollbar,
                },
            );
        }
    }

    // ExternalScrollId Management

    /// Register a scroll node and get its ExternalScrollId for WebRender.
    /// If the node already has an ID, returns the existing one.
    pub fn register_scroll_node(&mut self, dom_id: DomId, node_id: NodeId) -> ExternalScrollId {
        use azul_core::hit_test::PipelineId;

        let key = (dom_id, node_id);
        if let Some(&existing_id) = self.external_scroll_ids.get(&key) {
            return existing_id;
        }

        // Generate new ExternalScrollId (id, pipeline_id)
        // PipelineId = (PipelineSourceId: u32, u32)
        // Use dom_id.inner for PipelineSourceId, node_id.index() for second part
        let pipeline_id = PipelineId(
            dom_id.inner as u32, // PipelineSourceId is just u32
            node_id.index() as u32,
        );
        let new_id = ExternalScrollId(self.next_external_scroll_id, pipeline_id);
        self.next_external_scroll_id += 1;
        self.external_scroll_ids.insert(key, new_id);
        new_id
    }

    /// Get the ExternalScrollId for a node (returns None if not registered)
    pub fn get_external_scroll_id(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<ExternalScrollId> {
        self.external_scroll_ids.get(&(dom_id, node_id)).copied()
    }

    /// Iterate over all registered external scroll IDs
    pub fn iter_external_scroll_ids(
        &self,
    ) -> impl Iterator<Item = ((DomId, NodeId), ExternalScrollId)> + '_ {
        self.external_scroll_ids.iter().map(|(k, v)| (*k, *v))
    }

    // Scrollbar State Management

    /// Calculate scrollbar states for all visible scrollbars.
    /// This should be called once per frame after layout is complete.
    /// Uses the shared `compute_scrollbar_geometry()` for consistent geometry.
    pub fn calculate_scrollbar_states(&mut self) {
        self.scrollbar_states.clear();

        // Collect vertical scrollbar states
        // Uses virtual_scroll_size (when set) for the overflow check and thumb ratio,
        // so IFrame nodes with large virtual content show correct scrollbar geometry.
        let vertical_states: Vec<_> = self
            .states
            .iter()
            .filter(|(_, s)| {
                let effective_height = s.virtual_scroll_size
                    .map(|vs| vs.height)
                    .unwrap_or(s.content_rect.size.height);
                effective_height > s.container_rect.size.height
            })
            .map(|((dom_id, node_id), scroll_state)| {
                let v_state = Self::calculate_scrollbar_state_from_geometry(
                    scroll_state,
                    ScrollbarOrientation::Vertical,
                );
                ((*dom_id, *node_id, ScrollbarOrientation::Vertical), v_state)
            })
            .collect();

        // Collect horizontal scrollbar states
        let horizontal_states: Vec<_> = self
            .states
            .iter()
            .filter(|(_, s)| {
                let effective_width = s.virtual_scroll_size
                    .map(|vs| vs.width)
                    .unwrap_or(s.content_rect.size.width);
                effective_width > s.container_rect.size.width
            })
            .map(|((dom_id, node_id), scroll_state)| {
                let h_state = Self::calculate_scrollbar_state_from_geometry(
                    scroll_state,
                    ScrollbarOrientation::Horizontal,
                );
                (
                    (*dom_id, *node_id, ScrollbarOrientation::Horizontal),
                    h_state,
                )
            })
            .collect();

        // Insert all states
        self.scrollbar_states.extend(vertical_states);
        self.scrollbar_states.extend(horizontal_states);
    }

    /// Calculate scrollbar state using the shared `compute_scrollbar_geometry()`.
    fn calculate_scrollbar_state_from_geometry(
        scroll_state: &AnimatedScrollState,
        orientation: ScrollbarOrientation,
    ) -> ScrollbarState {
        let scrollbar_thickness = if scroll_state.scrollbar_thickness > 0.0 {
            scroll_state.scrollbar_thickness
        } else {
            16.0 // fallback default
        };

        let content_size = scroll_state.virtual_scroll_size
            .map(|vs| LogicalSize { width: vs.width, height: vs.height })
            .unwrap_or(scroll_state.content_rect.size);

        let scroll_offset = match orientation {
            ScrollbarOrientation::Vertical => scroll_state.current_offset.y,
            ScrollbarOrientation::Horizontal => scroll_state.current_offset.x,
        };

        let has_other_scrollbar = match orientation {
            ScrollbarOrientation::Vertical => scroll_state.has_horizontal_scrollbar,
            ScrollbarOrientation::Horizontal => scroll_state.has_vertical_scrollbar,
        };

        let geom = compute_scrollbar_geometry(
            orientation,
            scroll_state.container_rect,
            content_size,
            scroll_offset,
            scrollbar_thickness,
            has_other_scrollbar,
        );

        // Build ScrollbarState from the shared geometry
        let scale = match orientation {
            ScrollbarOrientation::Vertical => {
                LogicalPosition::new(1.0, geom.track_rect.size.height / scrollbar_thickness)
            }
            ScrollbarOrientation::Horizontal => {
                LogicalPosition::new(geom.track_rect.size.width / scrollbar_thickness, 1.0)
            }
        };

        ScrollbarState {
            visible: true,
            orientation,
            base_size: scrollbar_thickness,
            scale,
            thumb_position_ratio: geom.scroll_ratio,
            thumb_size_ratio: geom.thumb_size_ratio,
            track_rect: geom.track_rect,
            button_size: geom.button_size,
            usable_track_length: geom.usable_track_length,
            thumb_length: geom.thumb_length,
            thumb_offset: geom.thumb_offset,
        }
    }

    /// Get scrollbar state for hit-testing
    pub fn get_scrollbar_state(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        orientation: ScrollbarOrientation,
    ) -> Option<&ScrollbarState> {
        self.scrollbar_states.get(&(dom_id, node_id, orientation))
    }

    /// Iterate over all visible scrollbar states
    pub fn iter_scrollbar_states(
        &self,
    ) -> impl Iterator<Item = ((DomId, NodeId, ScrollbarOrientation), &ScrollbarState)> + '_ {
        self.scrollbar_states.iter().map(|(k, v)| (*k, v))
    }

    // Scrollbar Hit-Testing

    /// Hit-test scrollbars for a specific node at the given position.
    /// Returns Some if the position is inside a scrollbar for this node.
    pub fn hit_test_scrollbar(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        global_pos: LogicalPosition,
    ) -> Option<ScrollbarHit> {
        // Check both vertical and horizontal scrollbars for this node
        for orientation in [
            ScrollbarOrientation::Vertical,
            ScrollbarOrientation::Horizontal,
        ] {
            let scrollbar_state = self.scrollbar_states.get(&(dom_id, node_id, orientation))?;

            if !scrollbar_state.visible {
                continue;
            }

            // Check if position is inside scrollbar track using LogicalRect::contains
            if !scrollbar_state.track_rect.contains(global_pos) {
                continue;
            }

            // Calculate local position relative to track origin
            let local_pos = LogicalPosition::new(
                global_pos.x - scrollbar_state.track_rect.origin.x,
                global_pos.y - scrollbar_state.track_rect.origin.y,
            );

            // Determine which component was hit
            let component = scrollbar_state.hit_test_component(local_pos);

            return Some(ScrollbarHit {
                dom_id,
                node_id,
                orientation,
                component,
                local_position: local_pos,
                global_position: global_pos,
            });
        }

        None
    }

    /// Perform hit-testing for all scrollbars at the given global position.
    ///
    /// This iterates through all visible scrollbars in reverse z-order (top to bottom)
    /// and returns the first hit. Use this when you don't know which node to check.
    ///
    /// For better performance, use `hit_test_scrollbar()` when you already have
    /// a hit-tested node from WebRender.
    pub fn hit_test_scrollbars(&self, global_pos: LogicalPosition) -> Option<ScrollbarHit> {
        // Iterate in reverse order to hit top-most scrollbars first
        for ((dom_id, node_id, orientation), scrollbar_state) in self.scrollbar_states.iter().rev()
        {
            if !scrollbar_state.visible {
                continue;
            }

            // Check if position is inside scrollbar track
            if !scrollbar_state.track_rect.contains(global_pos) {
                continue;
            }

            // Calculate local position relative to track origin
            let local_pos = LogicalPosition::new(
                global_pos.x - scrollbar_state.track_rect.origin.x,
                global_pos.y - scrollbar_state.track_rect.origin.y,
            );

            // Determine which component was hit
            let component = scrollbar_state.hit_test_component(local_pos);

            return Some(ScrollbarHit {
                dom_id: *dom_id,
                node_id: *node_id,
                orientation: *orientation,
                component,
                local_position: local_pos,
                global_position: global_pos,
            });
        }

        None
    }
}

// AnimatedScrollState Implementation

impl AnimatedScrollState {
    /// Create a new scroll state initialized at offset (0, 0).
    pub fn new(now: Instant) -> Self {
        Self {
            current_offset: LogicalPosition::zero(),
            animation: None,
            last_activity: now,
            container_rect: LogicalRect::zero(),
            content_rect: LogicalRect::zero(),
            virtual_scroll_size: None,
            virtual_scroll_offset: None,
            overscroll_behavior_x: azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
            overscroll_behavior_y: azul_css::props::style::scrollbar::OverscrollBehavior::Auto,
            overflow_scrolling: azul_css::props::style::scrollbar::OverflowScrolling::Auto,
            scrollbar_thickness: 16.0,
            has_horizontal_scrollbar: false,
            has_vertical_scrollbar: false,
        }
    }

    /// Clamp a scroll position to valid bounds (0 to max_scroll).
    ///
    /// When `virtual_scroll_size` is set (for IFrame nodes), the max bounds
    /// are computed from the virtual size instead of content_rect.
    pub fn clamp(&self, position: LogicalPosition) -> LogicalPosition {
        let effective_width = self.virtual_scroll_size
            .map(|s| s.width)
            .unwrap_or(self.content_rect.size.width);
        let effective_height = self.virtual_scroll_size
            .map(|s| s.height)
            .unwrap_or(self.content_rect.size.height);
        let max_x = (effective_width - self.container_rect.size.width).max(0.0);
        let max_y = (effective_height - self.container_rect.size.height).max(0.0);
        LogicalPosition {
            x: position.x.max(0.0).min(max_x),
            y: position.y.max(0.0).min(max_y),
        }
    }
}

// Easing Functions

/// Apply an easing function to a normalized time value (0.0 to 1.0).
/// Used by ScrollAnimation::tick() for smooth scroll animations.
pub fn apply_easing(t: f32, easing: EasingFunction) -> f32 {
    match easing {
        EasingFunction::Linear => t,
        EasingFunction::EaseOut => 1.0 - (1.0 - t).powi(3),
        EasingFunction::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
    }
}

// Legacy type alias
pub type ScrollStates = ScrollManager;

impl ScrollManager {
    /// Remap NodeIds after DOM reconciliation
    ///
    /// When the DOM is regenerated, NodeIds can change. This method updates all
    /// internal state to use the new NodeIds based on the provided mapping.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &std::collections::BTreeMap<NodeId, NodeId>,
    ) {
        // Only remap nodes that actually moved (old_id != new_id).
        // Nodes NOT in the map are stable (kept same NodeId) — don't touch them.
        // We cannot distinguish "not moved" from "removed" with just node_moves,
        // so we conservatively keep states that aren't in the map.
        
        // Remap states
        for (&old_node_id, &new_node_id) in node_id_map.iter() {
            if old_node_id != new_node_id {
                if let Some(state) = self.states.remove(&(dom_id, old_node_id)) {
                    self.states.insert((dom_id, new_node_id), state);
                }
            }
        }
        
        // Remap external_scroll_ids
        for (&old_node_id, &new_node_id) in node_id_map.iter() {
            if old_node_id != new_node_id {
                if let Some(scroll_id) = self.external_scroll_ids.remove(&(dom_id, old_node_id)) {
                    self.external_scroll_ids.insert((dom_id, new_node_id), scroll_id);
                }
            }
        }
        
        // Remap scrollbar_states
        let scrollbar_states_to_remap: Vec<_> = self.scrollbar_states.keys()
            .filter(|(d, node_id, _)| {
                *d == dom_id && node_id_map.get(node_id).map_or(false, |new_id| new_id != node_id)
            })
            .cloned()
            .collect();
        
        for (d, old_node_id, orientation) in scrollbar_states_to_remap {
            if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                if let Some(state) = self.scrollbar_states.remove(&(d, old_node_id, orientation)) {
                    self.scrollbar_states.insert((d, new_node_id, orientation), state);
                }
            }
        }
    }
}
