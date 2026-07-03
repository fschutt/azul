//! Pure scroll state management — the single source of truth for scroll offsets.
//!
//! # Architecture
//!
//! `ScrollManager` is the exclusive owner of all scroll state. Other modules
//! interact with scrolling only through its public API:
//!
//! - **Platform shell** (macos/events.rs, etc.): Calls `record_scroll_from_hit_test()`
//!   to queue trackpad/mouse wheel input for the physics timer.
//! - **Scroll physics timer** (`scroll_timer.rs)`: Consumes inputs via `ScrollInputQueue`,
//!   applies physics, and pushes `CallbackChange::ScrollTo` for each updated node.
//! - **Event processing** (`event_v2.rs)`: Processes `ScrollTo` changes, sets scroll
//!   positions, and checks `VirtualView` re-invocation transparently.
//! - **Gesture manager** (gesture.rs): Tracks drag state and emits
//!   `AutoScrollDirection` — does NOT modify scroll offsets directly.
//! - **Render loop**: Calls `tick()` every frame to advance easing animations.
//! - **`WebRender` sync** (`wr_translate2.rs)`: Reads offsets via
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
//!   → virtual_view_manager.check_reinvoke() (transparent VirtualView support)
//!   → repaint
//! ```
//!
//! This module provides:
//! - Smooth scroll animations with easing
//! - Event source classification for scroll events
//! - Scrollbar geometry and hit-testing
//! - Virtual scroll bounds for `VirtualView` nodes

use alloc::collections::BTreeMap;
#[cfg(feature = "std")]
use alloc::vec::Vec;

use azul_core::{
    dom::{DomId, NodeId, ScrollbarOrientation},
    events::EasingFunction,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    styled_dom::NodeHierarchyItemId,
    task::{Duration, Instant},
};

#[cfg(feature = "std")]
use std::sync::{Arc, Mutex};

use crate::managers::hover::InputPointId;
use crate::solver3::scrollbar::compute_scrollbar_geometry_with_button_size;

/// Minimum change in scroll offset (in logical pixels) to consider the position
/// "actually moved" and mark the scroll state dirty.
const SCROLL_CHANGE_EPSILON: f32 = 0.01;

// ============================================================================
// Scroll Input Types (for timer-based physics architecture)
// ============================================================================

/// Classifies the source of a scroll input event.
///
/// This determines how the scroll physics timer processes the input:
/// - `TrackpadContinuous`: The OS already applies momentum — set position directly
/// - `WheelDiscrete`: Mouse wheel clicks — apply as impulse with momentum decay
/// - `Programmatic`: API-driven scroll — apply with optional easing animation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollInputSource {
    /// Continuous trackpad gesture (macOS precise scrolling).
    /// Position is set directly — the OS handles momentum/physics.
    TrackpadContinuous,
    /// Trackpad gesture ended (fingers lifted off trackpad).
    /// Triggers spring-back if the scroll position is past the bounds
    /// (rubber-banding overshoot). The OS sends this when
    /// `NSEventPhaseEnded` or momentumPhaseEnded is detected.
    TrackpadEnd,
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
    #[must_use] pub fn new() -> Self {
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
    #[must_use] pub fn take_all(&self) -> Vec<ScrollInput> {
        self.inner.lock().map_or_else(
            |_| Vec::new(),
            |mut queue| core::mem::take(&mut *queue),
        )
    }

    /// Take at most `max_events` recent inputs, sorted by timestamp (newest last).
    /// Any older events beyond `max_events` are discarded.
    /// This prevents the physics timer from processing an unbounded backlog.
    #[must_use] pub fn take_recent(&self, max_events: usize) -> Vec<ScrollInput> {
        self.inner.lock().map_or_else(
            |_| Vec::new(),
            |mut queue| {
                let mut events = core::mem::take(&mut *queue);
                if events.len() > max_events {
                    // Sort by timestamp ascending (oldest first), keep last N
                    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                    events.drain(..events.len() - max_events);
                }
                events
            },
        )
    }

    /// Check if there are pending inputs without consuming them
    #[must_use] pub fn has_pending(&self) -> bool {
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
#[derive(Copy, Debug, Clone)]
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
    /// Button size (square: `button_size` × `button_size`)
    pub button_size: f32,
    /// Usable track length after subtracting buttons
    pub usable_track_length: f32,
    /// Thumb length in pixels
    pub thumb_length: f32,
    /// Thumb offset from start of usable track region
    pub thumb_offset: f32,
}

impl ScrollbarState {
    /// Determine which component was hit at the given local position (relative to `track_rect`
    /// origin). Uses the shared geometry values (`button_size`, `usable_track_length`, `thumb_length`,
    /// `thumb_offset`) for consistent hit-testing.
    #[must_use] pub fn hit_test_component(&self, local_pos: LogicalPosition) -> ScrollbarComponent {
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
    /// Position relative to `track_rect` origin
    pub local_position: LogicalPosition,
    /// Original global window position
    pub global_position: LogicalPosition,
}

// Core Scroll Manager

/// Manages all scroll state and animations for a window
#[derive(Debug, Clone, Default)]
pub struct ScrollManager {
    /// Maps (`DomId`, `NodeId`) to their scroll state
    states: BTreeMap<(DomId, NodeId), AnimatedScrollState>,
    /// Scrollbar geometry states (calculated per frame)
    scrollbar_states: BTreeMap<(DomId, NodeId, ScrollbarOrientation), ScrollbarState>,
    /// Thread-safe queue for scroll inputs (shared with timer callbacks)
    #[cfg(feature = "std")]
    pub scroll_input_queue: ScrollInputQueue,
    /// Raw wheel/trackpad delta recorded *this input pass*, regardless of whether
    /// a scrollable node was under the cursor. The scroll input queue only carries
    /// deltas destined for scrollable containers (consumed by the physics timer);
    /// this field additionally lets `determine_all_events` synthesize a `Scroll`
    /// event aimed at the hovered node so non-scroll-container widgets (e.g. the
    /// map, which treats wheel = zoom) can react via a `HoverEventFilter::Scroll`
    /// callback + `CallbackInfo::get_scroll_delta`. Set in
    /// [`Self::record_scroll_from_hit_test`]; read during event determination and
    /// callback dispatch, then cleared at the end of the pass.
    pub pending_wheel_event: Option<LogicalPosition>,
    /// Set when a scroll position changes; cleared after the display list
    /// is regenerated.  Used by the CPU renderer path to detect when the
    /// display list must be rebuilt even though the DOM hasn't changed.
    scroll_dirty: bool,
    /// Scroll-direction preference, applied ONCE in [`Self::record_scroll_input`]
    /// (the single chokepoint every platform's wheel/axis event flows through).
    ///
    /// `false` (default) = traditional desktop wheel: a raw "scroll down" event
    /// increases the offset (content moves up). `true` = natural: inverted.
    /// Replaces the per-platform hardcoded `-delta` negations so the sign lives
    /// in one configurable place ([`Self::set_natural_scroll`]).
    ///
    /// CAVEAT: on macOS and on Linux touchpads via libinput the OS/driver ALREADY
    /// applies the user's natural-scroll preference before azul sees the delta, so
    /// this flag must stay at its default there (we preserve current behavior) and
    /// primarily controls mouse-wheel direction on platforms that don't pre-apply.
    natural_scroll: bool,
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
    /// Virtual scroll size from `VirtualView` callback (if this node hosts a `VirtualView`).
    /// When set, clamp logic uses this instead of `content_rect` for max scroll bounds.
    pub virtual_scroll_size: Option<LogicalSize>,
    /// Virtual scroll offset from `VirtualView` callback
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
    /// Visual rendering width in CSS pixels (e.g. 8.0 for thin overlay).
    /// Non-zero even for overlay scrollbars. Falls back to `scrollbar_thickness` if 0.
    pub visual_width_px: f32,
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

/// Read-only snapshot of a scroll node's state, returned by `CallbackInfo` queries.
///
/// Provides all the information a timer callback needs to compute scroll physics
/// without requiring mutable access to the `ScrollManager`.
#[derive(Copy, Debug, Clone)]
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
    /// Creates a new empty `ScrollManager`
    #[must_use] pub fn new() -> Self {
        let mut m = Self::default();
        // Power-user / test override. Platform shells should call
        // `set_natural_scroll` from the OS preference; this env var wins so the
        // direction can be flipped without a rebuild and so tests are hermetic.
        #[cfg(feature = "std")]
        if let Some(v) = std::env::var_os("AZ_NATURAL_SCROLL") {
            m.natural_scroll = matches!(v.to_str(), Some("1" | "true" | "TRUE"));
        }
        m
    }

    /// Set the scroll-direction preference. `true` = natural (content follows the
    /// gesture / inverted from the traditional wheel). Platform shells call this
    /// from the detected OS preference. See the `natural_scroll` field docs for the
    /// macOS/libinput pre-application caveat.
    pub const fn set_natural_scroll(&mut self, natural: bool) {
        self.natural_scroll = natural;
    }

    /// Current scroll-direction preference (`true` = natural/inverted).
    #[must_use] pub const fn is_natural_scroll(&self) -> bool {
        self.natural_scroll
    }

    /// The sign applied to a raw input delta to get the offset delta:
    /// `-1.0` traditional (default), `+1.0` natural. Centralises what used to be a
    /// hardcoded `-delta` at every platform call site.
    #[inline]
    const fn scroll_sign(&self) -> f32 {
        if self.natural_scroll {
            1.0
        } else {
            -1.0
        }
    }

    /// Sizes of the internal maps — used by `AZ_E2E_TEST` to watch for
    /// unbounded growth across resize/tick iterations.
    #[must_use] pub fn debug_counts(&self) -> (usize, usize) {
        (self.states.len(), self.scrollbar_states.len())
    }

    /// Returns `true` if any scroll position changed since the last
    /// `clear_scroll_dirty()` call.
    pub(crate) const fn has_pending_scroll_changes(&self) -> bool {
        self.scroll_dirty
    }

    /// Clear the dirty flag after the display list has been regenerated.
    pub const fn clear_scroll_dirty(&mut self) {
        self.scroll_dirty = false;
    }

    /// Build a map from `scroll_id` (`LocalScrollId`) to current scroll offset.
    ///
    /// Used by the CPU renderer to look up scroll positions at render time
    /// without embedding them in the display list.
    ///
    /// `scroll_ids` maps layout-tree node index → `scroll_id`. We need to
    /// convert our (`DomId`, `NodeId`) keys to `scroll_ids`.
    #[must_use] pub fn build_scroll_offset_map(
        &self,
        dom_id: DomId,
        scroll_ids: &std::collections::HashMap<usize, u64>,
    ) -> std::collections::HashMap<u64, (f32, f32)> {
        let mut map = std::collections::HashMap::new();
        for ((d, node_id), state) in &self.states {
            if *d != dom_id { continue; }
            // Find the scroll_id for this node_id by searching scroll_ids
            // (scroll_ids maps layout_index → scroll_id, and node_id.index() == layout_index
            // for the root DOM)
            let node_idx = node_id.index();
            if let Some(&scroll_id) = scroll_ids.get(&node_idx) {
                map.insert(scroll_id, (state.current_offset.x, state.current_offset.y));
            }
        }
        map
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
    /// The scroll-direction sign ([`Self::scroll_sign`]) is applied HERE — the
    /// single chokepoint every wheel/axis event flows through — so platform shells
    /// pass the RAW delta and no longer hardcode `-delta` at each call site.
    ///
    /// Returns `true` if the physics timer should be started (i.e., there are
    /// now pending inputs and no timer is running yet).
    #[cfg(feature = "std")]
    pub fn record_scroll_input(&mut self, mut input: ScrollInput) -> bool {
        let sign = self.scroll_sign();
        input.delta.x *= sign;
        input.delta.y *= sign;
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
        // Record the raw wheel delta for this pass unconditionally — even when the
        // cursor isn't over a scroll container — so a `Scroll` event can be aimed
        // at the hovered node (wheel-as-zoom widgets like the map rely on this).
        self.pending_wheel_event = Some(LogicalPosition { x: delta_x, y: delta_y });

        let hit_test = hover_manager.get_current(input_point_id)?;

        // MWA-B2: nested scroll containers — innermost-first with boundary
        // handoff. The previous ascending iteration always picked the
        // OUTERMOST scrollable ancestor (BTreeMap keys ascend; ancestors
        // have lower arena NodeIds), so wheeling over a list inside a
        // scrollable page scrolled the page instead of the list. We now
        // walk innermost-first and give the event to the first candidate
        // that can still move in the delta's direction (the web's default
        // overscroll handoff); when every candidate is pinned, the
        // innermost scrollable wins so the gesture still targets the node
        // under the pointer.
        let sign = self.scroll_sign();
        let (eff_x, eff_y) = (delta_x * sign, delta_y * sign);
        let target = self.select_scroll_target(
            hit_test.hovered_nodes.iter().flat_map(|(dom_id, hit_node)| {
                hit_node
                    .scroll_hit_test_nodes
                    .keys()
                    .rev()
                    .map(move |node_id| (*dom_id, *node_id))
            }),
            eff_x,
            eff_y,
        );
        let (dom_id, node_id) = target?;
        let input = ScrollInput {
            dom_id,
            node_id,
            // Raw delta — record_scroll_input applies scroll_sign() itself.
            delta: LogicalPosition { x: delta_x, y: delta_y },
            timestamp: now,
            source,
        };
        let should_start_timer = self.record_scroll_input(input);
        Some((dom_id, node_id, should_start_timer))
    }

    /// MWA-B2: choose the scroll node a wheel/trackpad event should drive.
    ///
    /// `candidates` must be ordered innermost-first; `eff_x`/`eff_y` are the
    /// direction-normalized deltas (post `scroll_sign()`: positive = offset
    /// grows = view moves toward content's down/right). The first candidate
    /// with remaining travel in a moved direction wins; if every candidate
    /// is pinned, the innermost scrollable is returned so the gesture still
    /// anchors under the pointer (matches CSS default overscroll behavior).
    fn select_scroll_target<I>(
        &self,
        candidates: I,
        eff_x: f32,
        eff_y: f32,
    ) -> Option<(DomId, NodeId)>
    where
        I: Iterator<Item = (DomId, NodeId)>,
    {
        let mut fallback = None;
        for (dom_id, node_id) in candidates {
            if !self.is_node_scrollable(dom_id, node_id) {
                continue;
            }
            if fallback.is_none() {
                fallback = Some((dom_id, node_id));
            }
            if self.can_consume_delta(dom_id, node_id, eff_x, eff_y) {
                return Some((dom_id, node_id));
            }
        }
        fallback
    }

    /// MWA-B10: the a11y tree's scroll surface for a node — current offset
    /// plus max travel per axis, or `None` when the node isn't scrollable.
    /// Screen readers use this (with the ScrollUp/Down/... actions) to
    /// drive the same inbound handler mouse users exercise.
    #[must_use] pub fn a11y_scroll_info(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<(LogicalPosition, f32, f32)> {
        let state = self.states.get(&(dom_id, node_id))?;
        let effective_width = state
            .virtual_scroll_size
            .map_or(state.content_rect.size.width, |s| s.width);
        let effective_height = state
            .virtual_scroll_size
            .map_or(state.content_rect.size.height, |s| s.height);
        let max_x = (effective_width - state.container_rect.size.width).max(0.0);
        let max_y = (effective_height - state.container_rect.size.height).max(0.0);
        if max_x <= 0.0 && max_y <= 0.0 {
            return None;
        }
        Some((state.current_offset, max_x, max_y))
    }

    /// `true` when the node still has travel in the direction of the
    /// normalized delta on at least one moved axis — the boundary-handoff
    /// test for [`select_scroll_target`](Self::select_scroll_target).
    fn can_consume_delta(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        eff_x: f32,
        eff_y: f32,
    ) -> bool {
        const EPS: f32 = 0.5;
        let Some(state) = self.states.get(&(dom_id, node_id)) else {
            return false;
        };
        let effective_width = state
            .virtual_scroll_size
            .map_or(state.content_rect.size.width, |s| s.width);
        let effective_height = state
            .virtual_scroll_size
            .map_or(state.content_rect.size.height, |s| s.height);
        let max_x = (effective_width - state.container_rect.size.width).max(0.0);
        let max_y = (effective_height - state.container_rect.size.height).max(0.0);
        let off = state.current_offset;

        let x_ok = if eff_x > EPS {
            off.x < max_x - EPS
        } else if eff_x < -EPS {
            off.x > EPS
        } else {
            false
        };
        let y_ok = if eff_y > EPS {
            off.y < max_y - EPS
        } else if eff_y < -EPS {
            off.y > EPS
        } else {
            false
        };
        x_ok || y_ok
    }

    /// Get a clone of the scroll input queue (for sharing with timer callbacks).
    ///
    /// The timer callback stores this in its `RefAny` data and calls `take_all()`
    /// each tick to consume pending inputs.
    #[cfg(feature = "std")]
    #[must_use] pub fn get_input_queue(&self) -> ScrollInputQueue {
        self.scroll_input_queue.clone()
    }

    /// Advances scroll animations by one tick, returns repaint info
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    // Instant is a ref-counted FFI clock handle; called by every dll backend's event loop by value.
    #[allow(clippy::needless_pass_by_value)]
    pub fn tick(&mut self, now: Instant) -> ScrollTickResult {
        let mut result = ScrollTickResult::default();
        for ((dom_id, node_id), state) in &mut self.states {
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

    /// Returns `true` if any scroll node has an active easing animation.
    ///
    /// Used by GPU render paths to skip rendering when the UI is completely
    /// static (no scroll animations, no layout changes).
    #[must_use] pub fn has_active_animations(&self) -> bool {
        self.states.values().any(|s| s.animation.is_some())
    }

    /// Finds the closest scroll-container ancestor for a given node.
    ///
    /// Walks up the node hierarchy to find a node that is registered as a
    /// scrollable node in this `ScrollManager`. Returns `None` if no scrollable
    /// ancestor is found.
    #[must_use] pub fn find_scroll_parent(
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
                .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
        }
        None
    }

    /// Check if a node is scrollable (has overflow:scroll/auto and overflowing content)
    ///
    /// Uses `virtual_scroll_size` (when set) instead of `content_rect` for the
    /// overflow check, so `VirtualView` nodes with large virtual content are correctly
    /// identified as scrollable even when only a small subset is rendered.
    fn is_node_scrollable(&self, dom_id: DomId, node_id: NodeId) -> bool {
        let result = self.states.get(&(dom_id, node_id)).is_some_and(|state| {
            let effective_width = state.virtual_scroll_size
                .map_or(state.content_rect.size.width, |s| s.width);
            let effective_height = state.virtual_scroll_size
                .map_or(state.content_rect.size.height, |s| s.height);
            let has_horizontal = effective_width > state.container_rect.size.width;
            let has_vertical = effective_height > state.container_rect.size.height;
            has_horizontal || has_vertical
        });
        result
    }

    // +spec:overflow:4000a6 - scroll position as offset from scroll origin within scrollport
    /// Sets scroll position immediately (no animation), clamped to valid bounds.
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
        let clamped = state.clamp(position);
        if (clamped.x - state.current_offset.x).abs() > SCROLL_CHANGE_EPSILON
            || (clamped.y - state.current_offset.y).abs() > SCROLL_CHANGE_EPSILON
        {
            self.scroll_dirty = true;
        }
        state.current_offset = clamped;
        state.animation = None;
        state.last_activity = now;
    }

    /// Sets scroll position immediately without clamping.
    ///
    /// Used by the scroll physics timer which does its own rubber-band clamping.
    /// Allows the offset to go outside [0, `max_scroll`] for overscroll/rubber-banding.
    pub fn set_scroll_position_unclamped(
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
        if (position.x - state.current_offset.x).abs() > SCROLL_CHANGE_EPSILON
            || (position.y - state.current_offset.y).abs() > SCROLL_CHANGE_EPSILON
        {
            self.scroll_dirty = true;
        }
        state.current_offset = position;
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

    /// Updates virtual scroll bounds for a `VirtualView` node.
    ///
    /// Called after `VirtualView` callback returns to propagate the virtual content size
    /// to the `ScrollManager`. Clamp logic then uses `virtual_scroll_size` (when set)
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
            // AzInstant (System on std, safe Tick on no-clock targets) — not the
            // WASM-panicking std::time::Instant::now(). (A refinement would thread
            // the window's get_system_time_fn callback through for hookability.)
            AnimatedScrollState::new(Instant::now())
        });
        state.virtual_scroll_size = Some(virtual_scroll_size);
        state.virtual_scroll_offset = virtual_scroll_offset;
        // Re-clamp with new virtual bounds
        state.current_offset = state.clamp(state.current_offset);
    }

    /// Returns the current scroll offset for a node
    #[must_use] pub fn get_current_offset(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalPosition> {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.current_offset)
    }

    /// Returns the timestamp of last scroll activity for a node
    #[must_use] pub fn get_last_activity_time(&self, dom_id: DomId, node_id: NodeId) -> Option<Instant> {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.last_activity.clone())
    }

    /// Returns the internal scroll state for a node
    #[must_use] pub fn get_scroll_state(&self, dom_id: DomId, node_id: NodeId) -> Option<&AnimatedScrollState> {
        self.states.get(&(dom_id, node_id))
    }

    /// Returns a read-only snapshot of a scroll node's state.
    ///
    /// This is the preferred way for timer callbacks to query scroll state,
    /// since they only have `&CallbackInfo` (read-only access).
    ///
    /// When `virtual_scroll_size` is set (for `VirtualView` nodes), the max scroll
    /// bounds are computed from the virtual size instead of `content_rect`.
    #[must_use] pub fn get_scroll_node_info(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<ScrollNodeInfo> {
        let state = self.states.get(&(dom_id, node_id))?;
        let effective_content_width = state.virtual_scroll_size
            .map_or(state.content_rect.size.width, |s| s.width);
        let effective_content_height = state.virtual_scroll_size
            .map_or(state.content_rect.size.height, |s| s.height);
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
    #[must_use] pub fn get_scroll_states_for_dom(&self, dom_id: DomId) -> BTreeMap<NodeId, ScrollPosition> {
        // M12.7: iterating an EMPTY hashbrown map (RawIterRange) mis-lifts to
        // wasm and loops forever (same class as the font-id / GPU-cache loops).
        // For the headless web path `states` is empty; guard it (len-based, no
        // iteration). Desktop unchanged.
        if self.states.is_empty() {
            return BTreeMap::new();
        }
        self.states
            .iter()
            .filter(|((d, _), _)| *d == dom_id)
            .map(|((_, node_id), state)| {
                // Use virtual_scroll_size (from VirtualView callback) when available,
                // otherwise fall back to content_rect.size from layout.
                let effective_content_size = state.virtual_scroll_size
                    .unwrap_or(state.content_rect.size);
                (
                    *node_id,
                    ScrollPosition {
                        parent_rect: state.container_rect,
                        children_rect: LogicalRect::new(
                            state.current_offset,
                            effective_content_size,
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
        visual_width_px: f32,
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
            existing.visual_width_px = visual_width_px;
            existing.has_horizontal_scrollbar = has_horizontal_scrollbar;
            existing.has_vertical_scrollbar = has_vertical_scrollbar;
            // Re-clamp current offset to new bounds
            existing.current_offset = existing.clamp(existing.current_offset);
        } else {
            // +spec:overflow:8c7aa1 - initial scroll position is zero (scroll origin for LTR/TTB)
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
                    visual_width_px,
                    has_horizontal_scrollbar,
                    has_vertical_scrollbar,
                },
            );
        }
    }

    // Scrollbar State Management

    /// Calculate scrollbar states for all visible scrollbars.
    /// This should be called once per frame after layout is complete.
    /// Uses the shared `compute_scrollbar_geometry()` for consistent geometry.
    pub fn calculate_scrollbar_states(&mut self) {
        self.scrollbar_states.clear();

        // Uses virtual_scroll_size (when set) for the overflow check and thumb ratio,
        // so VirtualView nodes with large virtual content show correct scrollbar geometry.
        for orientation in [ScrollbarOrientation::Vertical, ScrollbarOrientation::Horizontal] {
            let states: Vec<_> = self
                .states
                .iter()
                .filter(|(_, s)| {
                    let (effective, container) = match orientation {
                        ScrollbarOrientation::Vertical => (
                            s.virtual_scroll_size.map_or(s.content_rect.size.height, |vs| vs.height),
                            s.container_rect.size.height,
                        ),
                        ScrollbarOrientation::Horizontal => (
                            s.virtual_scroll_size.map_or(s.content_rect.size.width, |vs| vs.width),
                            s.container_rect.size.width,
                        ),
                    };
                    effective > container
                })
                .map(|((dom_id, node_id), scroll_state)| {
                    let state = Self::calculate_scrollbar_state_from_geometry(
                        scroll_state,
                        orientation,
                    );
                    ((*dom_id, *node_id, orientation), state)
                })
                .collect();

            self.scrollbar_states.extend(states);
        }
    }

    /// Calculate scrollbar state using the shared `compute_scrollbar_geometry()`.
    fn calculate_scrollbar_state_from_geometry(
        scroll_state: &AnimatedScrollState,
        orientation: ScrollbarOrientation,
    ) -> ScrollbarState {
        let scrollbar_thickness = if scroll_state.visual_width_px > 0.0 {
            scroll_state.visual_width_px
        } else if scroll_state.scrollbar_thickness > 0.0 {
            scroll_state.scrollbar_thickness
        } else {
            crate::solver3::fc::DEFAULT_SCROLLBAR_WIDTH_PX
        };

        let content_size = scroll_state.virtual_scroll_size
            .map_or(scroll_state.content_rect.size, |vs| vs);

        let scroll_offset = match orientation {
            ScrollbarOrientation::Vertical => scroll_state.current_offset.y,
            ScrollbarOrientation::Horizontal => scroll_state.current_offset.x,
        };

        let has_other_scrollbar = match orientation {
            ScrollbarOrientation::Vertical => scroll_state.has_horizontal_scrollbar,
            ScrollbarOrientation::Horizontal => scroll_state.has_vertical_scrollbar,
        };

        // Overlay scrollbars (thickness == 0 from layout) have no arrow buttons
        let is_overlay = scroll_state.scrollbar_thickness == 0.0;
        let button_size = if is_overlay { 0.0 } else { scrollbar_thickness };
        let geom = compute_scrollbar_geometry_with_button_size(
            orientation,
            scroll_state.container_rect,
            content_size,
            scroll_offset,
            scrollbar_thickness,
            has_other_scrollbar,
            button_size,
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
    #[must_use] pub fn get_scrollbar_state(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        orientation: ScrollbarOrientation,
    ) -> Option<&ScrollbarState> {
        self.scrollbar_states.get(&(dom_id, node_id, orientation))
    }

    /// Iterate over all visible scrollbar states
    pub(crate) fn iter_scrollbar_states(
        &self,
    ) -> impl Iterator<Item = ((DomId, NodeId, ScrollbarOrientation), &ScrollbarState)> + '_ {
        self.scrollbar_states.iter().map(|(k, v)| (*k, v))
    }

    // Scrollbar Hit-Testing

    /// Hit-test scrollbars for a specific node at the given position.
    /// Returns Some if the position is inside a scrollbar for this node.
    pub(crate) fn hit_test_scrollbar(
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
            let Some(scrollbar_state) = self.scrollbar_states.get(&(dom_id, node_id, orientation)) else {
                continue;
            };

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
    /// a hit-tested node from `WebRender`.
    #[must_use] pub fn hit_test_scrollbars(&self, global_pos: LogicalPosition) -> Option<ScrollbarHit> {
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
    // +spec:overflow:60f6a1 - scroll origin defaults to block-start inline-start corner (0,0)
    /// Create a new scroll state initialized at offset (0, 0).
    pub(crate) const fn new(now: Instant) -> Self {
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
            scrollbar_thickness: crate::solver3::fc::DEFAULT_SCROLLBAR_WIDTH_PX,
            visual_width_px: 0.0,
            has_horizontal_scrollbar: false,
            has_vertical_scrollbar: false,
        }
    }

    /// Clamp a scroll position to valid bounds (0 to `max_scroll`).
    ///
    /// When `virtual_scroll_size` is set (for `VirtualView` nodes), the max bounds
    /// are computed from the virtual size instead of `content_rect`.
    pub(crate) fn clamp(&self, position: LogicalPosition) -> LogicalPosition {
        let effective_width = self.virtual_scroll_size
            .map_or(self.content_rect.size.width, |s| s.width);
        let effective_height = self.virtual_scroll_size
            .map_or(self.content_rect.size.height, |s| s.height);
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
/// Used by `ScrollAnimation::tick()` for smooth scroll animations.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
pub(crate) fn apply_easing(t: f32, easing: EasingFunction) -> f32 {
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

impl ScrollManager {
    /// Remap `NodeIds` after DOM reconciliation
    ///
    /// When the DOM is regenerated, `NodeIds` can change. This method updates all
    /// internal state to use the new `NodeIds` based on the provided mapping.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &BTreeMap<NodeId, NodeId>,
    ) {
        // Only remap nodes that actually moved (old_id != new_id).
        // Nodes NOT in the map are stable (kept same NodeId) — don't touch them.
        // We cannot distinguish "not moved" from "removed" with just node_moves,
        // so we conservatively keep states that aren't in the map.
        
        // Remap states
        for (&old_node_id, &new_node_id) in node_id_map {
            if old_node_id != new_node_id {
                if let Some(state) = self.states.remove(&(dom_id, old_node_id)) {
                    self.states.insert((dom_id, new_node_id), state);
                }
            }
        }
        
        // Remap scrollbar_states
        let scrollbar_states_to_remap: Vec<_> = self.scrollbar_states.keys()
            .filter(|(d, node_id, _)| {
                *d == dom_id && node_id_map.get(node_id).is_some_and(|new_id| new_id != node_id)
            })
            .copied()
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

// ============================================================================
// Natural-scroll direction — unit tests (#17)
// ============================================================================
#[cfg(all(test, feature = "std"))]
mod natural_scroll_tests {
    use super::*;
    use azul_core::dom::{DomId, NodeId};
    use azul_core::geom::LogicalPosition;
    use azul_core::task::Instant;

    fn raw_input(dx: f32, dy: f32) -> ScrollInput {
        ScrollInput {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(0),
            delta: LogicalPosition::new(dx, dy),
            timestamp: Instant::from(std::time::Instant::now()),
            source: ScrollInputSource::WheelDiscrete,
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // test asserts exact float equality on deterministic values
    fn default_is_traditional_and_inverts_raw_delta() {
        // With AZ_NATURAL_SCROLL unset, the default is traditional: the offset
        // delta is the NEGATION of the raw input — exactly what the per-platform
        // `-delta` hardcodes used to do, now centralised.
        let mut m = ScrollManager::new();
        assert!(!m.is_natural_scroll(), "default must be traditional");
        m.record_scroll_input(raw_input(3.0, 10.0));
        let q = m.get_input_queue().take_all();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].delta.x, -3.0, "x must be inverted by the default sign");
        assert_eq!(q[0].delta.y, -10.0, "y must be inverted by the default sign");
    }

    #[test]
    #[allow(clippy::float_cmp)] // test asserts exact float equality on deterministic values
    fn natural_passes_raw_delta_through() {
        let mut m = ScrollManager::new();
        m.set_natural_scroll(true);
        assert!(m.is_natural_scroll());
        m.record_scroll_input(raw_input(3.0, 10.0));
        let q = m.get_input_queue().take_all();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].delta.x, 3.0, "natural mode must NOT invert x");
        assert_eq!(q[0].delta.y, 10.0, "natural mode must NOT invert y");
    }

    #[test]
    #[allow(clippy::float_cmp)] // test asserts exact float equality on deterministic values
    fn toggling_flips_sign_for_subsequent_input() {
        // Same raw input, opposite directions before/after the toggle — proves the
        // single flag is the only thing controlling direction.
        let mut m = ScrollManager::new();
        m.record_scroll_input(raw_input(0.0, 5.0));
        m.set_natural_scroll(true);
        m.record_scroll_input(raw_input(0.0, 5.0));
        let q = m.get_input_queue().take_all();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].delta.y, -5.0, "traditional first");
        assert_eq!(q[1].delta.y, 5.0, "natural after toggle");
    }

    // MWA-B2: nested-scroll target selection (innermost-first + handoff).

    fn nested_setup() -> (ScrollManager, DomId, NodeId, NodeId) {
        use azul_core::geom::{LogicalRect, LogicalSize};

        let now = Instant::now();
        let mut m = ScrollManager::new();
        let dom = DomId::ROOT_ID;
        // Ancestors have LOWER arena ids than descendants.
        let outer = NodeId::from_usize(1).unwrap();
        let inner = NodeId::from_usize(9).unwrap();
        // Outer: 200x200 viewport over 200x1000 content → max_y = 800.
        m.register_or_update_scroll_node(
            dom,
            outer,
            LogicalRect {
                origin: LogicalPosition::zero(),
                size: LogicalSize { width: 200.0, height: 200.0 },
            },
            LogicalSize { width: 200.0, height: 1000.0 },
            now.clone(),
            8.0,
            8.0,
            false,
            true,
        );
        // Inner: 100x100 viewport over 100x300 content → max_y = 200.
        m.register_or_update_scroll_node(
            dom,
            inner,
            LogicalRect {
                origin: LogicalPosition::zero(),
                size: LogicalSize { width: 100.0, height: 100.0 },
            },
            LogicalSize { width: 100.0, height: 300.0 },
            now,
            8.0,
            8.0,
            false,
            true,
        );
        (m, dom, outer, inner)
    }

    #[test]
    fn nested_scroll_prefers_innermost_with_room() {
        let (m, dom, outer, inner) = nested_setup();
        // Innermost-first candidate order, scrolling "down" (eff +y).
        let picked = m.select_scroll_target(
            [(dom, inner), (dom, outer)].into_iter(),
            0.0,
            1.0,
        );
        assert_eq!(picked, Some((dom, inner)), "inner has room → inner wins");
    }

    #[test]
    fn nested_scroll_hands_off_to_ancestor_at_boundary() {
        let (mut m, dom, outer, inner) = nested_setup();
        // Pin the inner container at its bottom edge (max_y = 200).
        m.states.get_mut(&(dom, inner)).unwrap().current_offset =
            LogicalPosition { x: 0.0, y: 200.0 };

        let down = m.select_scroll_target(
            [(dom, inner), (dom, outer)].into_iter(),
            0.0,
            1.0,
        );
        assert_eq!(down, Some((dom, outer)), "inner pinned at bottom → handoff");

        let up = m.select_scroll_target(
            [(dom, inner), (dom, outer)].into_iter(),
            0.0,
            -1.0,
        );
        assert_eq!(up, Some((dom, inner)), "inner has room upward → inner again");
    }

    #[test]
    fn nested_scroll_falls_back_to_innermost_when_all_pinned() {
        let (mut m, dom, outer, inner) = nested_setup();
        m.states.get_mut(&(dom, inner)).unwrap().current_offset =
            LogicalPosition { x: 0.0, y: 200.0 };
        m.states.get_mut(&(dom, outer)).unwrap().current_offset =
            LogicalPosition { x: 0.0, y: 800.0 };

        let picked = m.select_scroll_target(
            [(dom, inner), (dom, outer)].into_iter(),
            0.0,
            1.0,
        );
        assert_eq!(
            picked,
            Some((dom, inner)),
            "everything pinned → innermost fallback (gesture stays under pointer)"
        );
    }
}
