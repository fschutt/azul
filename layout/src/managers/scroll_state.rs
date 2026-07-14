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

impl crate::managers::NodeIdRemap for ScrollManager {
    /// Rewrite every `(DomId, NodeId)` key for `dom` and DROP the scroll state of
    /// nodes that were unmounted.
    ///
    /// The previous implementation only rewrote keys whose id actually changed and
    /// *kept* everything else "conservatively" — which silently re-attached the
    /// scroll offset of a deleted node to whatever node inherited its index.
    /// `node_moves` contains an entry for every matched node, so "absent from the
    /// map" unambiguously means "unmounted".
    fn remap_node_ids(&mut self, dom: DomId, map: &crate::managers::NodeIdMap) {
        crate::managers::remap_dom_keys(&mut self.states, dom, map);

        let old = core::mem::take(&mut self.scrollbar_states);
        for ((d, old_node_id, orientation), state) in old {
            if d != dom {
                self.scrollbar_states
                    .insert((d, old_node_id, orientation), state);
            } else if let Some(new_node_id) = map.resolve(old_node_id) {
                self.scrollbar_states
                    .insert((d, new_node_id, orientation), state);
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

// ============================================================================
// Adversarial unit tests (autotest fleet)
//
// Hostile inputs for every category in the task file: numeric (NaN / ±inf /
// MIN / MAX / zero / saturation), predicates (invariants at the boundary),
// getters (defined value on a default/empty instance) and constructors.
// Every assertion below documents the *actual* behavior — nothing is weakened
// to make it pass.
// ============================================================================
#[cfg(all(test, feature = "std"))]
mod autotest_generated {
    #![allow(clippy::float_cmp)] // tests assert exact float results on deterministic inputs

    use std::collections::HashMap;

    use azul_core::{
        dom::{DomId, NodeId, ScrollbarOrientation},
        events::EasingFunction,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        hit_test::{FullHitTest, HitTest, OverflowingScrollNode, ScrollHitTestItem},
        styled_dom::NodeHierarchyItem,
        task::{Duration, Instant, SystemTick, SystemTickDiff, SystemTimeDiff},
    };

    use super::*;
    use crate::managers::hover::HoverManager;

    // ---------------------------------------------------------------- helpers

    const DOM: DomId = DomId::ROOT_ID;
    const DOM1: DomId = DomId { inner: 1 };

    fn node(i: usize) -> NodeId {
        NodeId::new(i)
    }

    /// Deterministic tick-clock instant — no wall clock, no flakiness.
    fn at(t: u64) -> Instant {
        Instant::Tick(SystemTick::new(t))
    }

    fn tick_dur(d: u64) -> Duration {
        Duration::Tick(SystemTickDiff { tick_diff: d })
    }

    fn sys_dur(secs: u64, nanos: u32) -> Duration {
        Duration::System(SystemTimeDiff { secs, nanos })
    }

    fn pos(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition::new(x, y)
    }

    fn size(w: f32, h: f32) -> LogicalSize {
        LogicalSize::new(w, h)
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(pos(x, y), size(w, h))
    }

    /// A manager with node 0 registered: `container` viewport over `content`.
    fn mgr(container: LogicalSize, content: LogicalSize) -> ScrollManager {
        let mut m = ScrollManager::new();
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            LogicalRect::new(LogicalPosition::zero(), container),
            content,
            at(0),
            16.0,
            16.0,
            false,
            true,
        );
        m
    }

    /// A bare `AnimatedScrollState` with the given container/content geometry.
    fn state(container: LogicalSize, content: LogicalSize) -> AnimatedScrollState {
        let mut s = AnimatedScrollState::new(at(0));
        s.container_rect = LogicalRect::new(LogicalPosition::zero(), container);
        s.content_rect = LogicalRect::new(LogicalPosition::zero(), content);
        s
    }

    fn input(dx: f32, dy: f32, ts: u64) -> ScrollInput {
        ScrollInput {
            dom_id: DOM,
            node_id: node(0),
            delta: pos(dx, dy),
            timestamp: at(ts),
            source: ScrollInputSource::WheelDiscrete,
        }
    }

    fn scrollbar(
        orientation: ScrollbarOrientation,
        track: LogicalRect,
        button_size: f32,
        thumb_offset: f32,
        thumb_length: f32,
    ) -> ScrollbarState {
        ScrollbarState {
            visible: true,
            orientation,
            base_size: 16.0,
            scale: LogicalPosition::new(1.0, 1.0),
            thumb_position_ratio: 0.0,
            thumb_size_ratio: 0.5,
            track_rect: track,
            button_size,
            usable_track_length: 0.0,
            thumb_length,
            thumb_offset,
        }
    }

    /// A `HoverManager` whose current mouse hit-test reports `nodes` as scroll
    /// hit-test nodes in `DOM` (BTreeMap key order; `record_scroll_from_hit_test`
    /// walks them in reverse = innermost-first).
    fn hover_over(nodes: &[usize]) -> HoverManager {
        let mut ht = HitTest::empty();
        for n in nodes {
            ht.scroll_hit_test_nodes.insert(
                node(*n),
                ScrollHitTestItem {
                    point_in_viewport: LogicalPosition::zero(),
                    point_relative_to_item: LogicalPosition::zero(),
                    scroll_node: OverflowingScrollNode::default(),
                },
            );
        }
        let mut full = FullHitTest::empty(None);
        full.hovered_nodes.insert(DOM, ht);
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, full);
        hm
    }

    // ============================================================ apply_easing
    // (numeric: zero / min_max / negative / overflow / nan_inf)

    #[test]
    fn apply_easing_endpoints_are_exact_for_every_curve() {
        // The one invariant every easing curve must satisfy: f(0) == 0, f(1) == 1.
        // A violation here would make animations jump at their first/last tick.
        for e in [
            EasingFunction::Linear,
            EasingFunction::EaseOut,
            EasingFunction::EaseInOut,
        ] {
            assert_eq!(apply_easing(0.0, e), 0.0, "f(0) must be 0 for {e:?}");
            assert_eq!(apply_easing(1.0, e), 1.0, "f(1) must be 1 for {e:?}");
        }
    }

    #[test]
    fn apply_easing_is_monotonic_and_bounded_on_the_unit_interval() {
        for e in [
            EasingFunction::Linear,
            EasingFunction::EaseOut,
            EasingFunction::EaseInOut,
        ] {
            let mut prev = f32::NEG_INFINITY;
            for i in 0..=100 {
                let t = i as f32 / 100.0;
                let v = apply_easing(t, e);
                assert!(v.is_finite(), "{e:?}({t}) must be finite, got {v}");
                assert!(
                    (-1e-6..=1.0 + 1e-6).contains(&v),
                    "{e:?}({t}) = {v} escaped [0, 1]"
                );
                assert!(v >= prev - 1e-6, "{e:?} must not go backwards at t={t}");
                prev = v;
            }
        }
    }

    #[test]
    fn apply_easing_nan_propagates_without_panicking() {
        // NaN in => NaN out for every curve (no comparison panic, no unwrap).
        for e in [
            EasingFunction::Linear,
            EasingFunction::EaseOut,
            EasingFunction::EaseInOut,
        ] {
            assert!(
                apply_easing(f32::NAN, e).is_nan(),
                "{e:?}(NaN) must be NaN, not a silently-wrong number"
            );
        }
    }

    #[test]
    fn apply_easing_infinities_saturate_to_infinity_not_panic() {
        assert_eq!(apply_easing(f32::INFINITY, EasingFunction::Linear), f32::INFINITY);
        assert_eq!(
            apply_easing(f32::NEG_INFINITY, EasingFunction::Linear),
            f32::NEG_INFINITY
        );
        // EaseOut: 1 - (1 - inf)^3 = 1 + inf
        assert_eq!(apply_easing(f32::INFINITY, EasingFunction::EaseOut), f32::INFINITY);
        assert_eq!(
            apply_easing(f32::NEG_INFINITY, EasingFunction::EaseOut),
            f32::NEG_INFINITY
        );
        // EaseInOut: t >= 0.5 branch for +inf, t < 0.5 branch for -inf
        assert_eq!(
            apply_easing(f32::INFINITY, EasingFunction::EaseInOut),
            f32::INFINITY
        );
        assert_eq!(
            apply_easing(f32::NEG_INFINITY, EasingFunction::EaseInOut),
            f32::NEG_INFINITY
        );
    }

    #[test]
    fn apply_easing_f32_extremes_do_not_panic() {
        // powi(3) overflows f32 for MIN/MAX inputs — must saturate to +-inf,
        // never trap. (Callers clamp t to [0, 1]; this is the defense in depth.)
        for e in [
            EasingFunction::Linear,
            EasingFunction::EaseOut,
            EasingFunction::EaseInOut,
        ] {
            let hi = apply_easing(f32::MAX, e);
            let lo = apply_easing(f32::MIN, e);
            assert!(!hi.is_nan(), "{e:?}(f32::MAX) must not be NaN");
            assert!(!lo.is_nan(), "{e:?}(f32::MIN) must not be NaN");
        }
        // Subnormal / smallest positive: stays ~0, no denormal blowup.
        assert!(apply_easing(f32::MIN_POSITIVE, EasingFunction::EaseInOut).abs() < 1e-30);
    }

    #[test]
    fn apply_easing_negative_t_is_deterministic_extrapolation() {
        // Out-of-range t is not clamped by apply_easing (the caller does that);
        // pin the exact extrapolated values so a silent change is caught.
        assert_eq!(apply_easing(-1.0, EasingFunction::Linear), -1.0);
        assert_eq!(apply_easing(-1.0, EasingFunction::EaseOut), -7.0);
        assert_eq!(apply_easing(-1.0, EasingFunction::EaseInOut), -4.0);
    }

    #[test]
    fn apply_easing_ease_in_out_is_continuous_at_the_branch_boundary() {
        // t == 0.5 takes the `else` branch; both halves must meet at 0.5.
        assert_eq!(apply_easing(0.5, EasingFunction::EaseInOut), 0.5);
        let just_below = apply_easing(0.499_999, EasingFunction::EaseInOut);
        assert!(
            (just_below - 0.5).abs() < 1e-4,
            "discontinuity at the 0.5 branch: {just_below}"
        );
        assert_eq!(apply_easing(0.5, EasingFunction::EaseOut), 0.875);
    }

    // ================================================ AnimatedScrollState::new
    // (constructor: no_panic / invariants_hold)

    #[test]
    fn animated_scroll_state_new_starts_at_scroll_origin() {
        let s = AnimatedScrollState::new(at(0));
        assert_eq!(s.current_offset, LogicalPosition::zero());
        assert!(s.animation.is_none());
        assert_eq!(s.container_rect, LogicalRect::zero());
        assert_eq!(s.content_rect, LogicalRect::zero());
        assert!(s.virtual_scroll_size.is_none());
        assert!(s.virtual_scroll_offset.is_none());
        assert!(!s.has_horizontal_scrollbar);
        assert!(!s.has_vertical_scrollbar);
        // A zero-sized state has no travel: clamp must pin everything to origin.
        assert_eq!(s.clamp(pos(1e9, 1e9)), LogicalPosition::zero());
    }

    // ============================================== AnimatedScrollState::clamp
    // (numeric: zero / min_max / negative / overflow)

    #[test]
    fn clamp_pins_to_zero_and_max_travel() {
        let s = state(size(100.0, 100.0), size(100.0, 500.0));
        // max_x = 0 (no horizontal overflow), max_y = 400.
        assert_eq!(s.clamp(pos(0.0, 0.0)), pos(0.0, 0.0));
        assert_eq!(s.clamp(pos(50.0, 250.0)), pos(0.0, 250.0));
        assert_eq!(s.clamp(pos(-1.0, -1.0)), pos(0.0, 0.0));
        assert_eq!(s.clamp(pos(9999.0, 9999.0)), pos(0.0, 400.0));
    }

    #[test]
    fn clamp_never_produces_negative_max_when_content_is_smaller_than_container() {
        // Content smaller than the viewport => max travel is 0, not negative.
        let s = state(size(500.0, 500.0), size(10.0, 10.0));
        assert_eq!(s.clamp(pos(100.0, 100.0)), LogicalPosition::zero());
        assert_eq!(s.clamp(pos(-100.0, -100.0)), LogicalPosition::zero());
    }

    #[test]
    fn clamp_nan_position_collapses_to_origin_never_stores_nan() {
        // f32::max(NaN, 0.0) == 0.0, so a NaN offset is sanitized to the origin.
        // This is the property the whole scroll pipeline relies on to stay finite.
        let s = state(size(100.0, 100.0), size(100.0, 500.0));
        let c = s.clamp(pos(f32::NAN, f32::NAN));
        assert!(!c.x.is_nan() && !c.y.is_nan(), "clamp must not leak NaN");
        assert_eq!(c, LogicalPosition::zero());
    }

    #[test]
    fn clamp_infinite_position_saturates_to_max_travel() {
        let s = state(size(100.0, 100.0), size(100.0, 500.0));
        assert_eq!(s.clamp(pos(f32::INFINITY, f32::INFINITY)), pos(0.0, 400.0));
        assert_eq!(
            s.clamp(pos(f32::NEG_INFINITY, f32::NEG_INFINITY)),
            LogicalPosition::zero()
        );
        assert_eq!(s.clamp(pos(f32::MAX, f32::MAX)), pos(0.0, 400.0));
        assert_eq!(s.clamp(pos(f32::MIN, f32::MIN)), LogicalPosition::zero());
    }

    #[test]
    fn clamp_nan_geometry_degrades_to_zero_travel() {
        // A NaN content size must not poison the offset: (NaN - w).max(0.0) == 0.0.
        let s = state(size(100.0, 100.0), size(f32::NAN, f32::NAN));
        let c = s.clamp(pos(50.0, 50.0));
        assert!(!c.x.is_nan() && !c.y.is_nan());
        assert_eq!(c, LogicalPosition::zero());
    }

    #[test]
    fn clamp_infinite_content_minus_infinite_container_is_zero_travel_not_nan() {
        // inf - inf = NaN; `.max(0.0)` rescues it to 0.
        let s = state(
            size(f32::INFINITY, f32::INFINITY),
            size(f32::INFINITY, f32::INFINITY),
        );
        let c = s.clamp(pos(10.0, 10.0));
        assert!(!c.x.is_nan() && !c.y.is_nan());
        assert_eq!(c, LogicalPosition::zero());
    }

    #[test]
    fn clamp_prefers_virtual_scroll_size_over_content_rect() {
        let mut s = state(size(100.0, 100.0), size(100.0, 120.0));
        assert_eq!(s.clamp(pos(0.0, 1e9)), pos(0.0, 20.0), "content_rect bound");
        s.virtual_scroll_size = Some(size(100.0, 10_000.0));
        assert_eq!(
            s.clamp(pos(0.0, 1e9)),
            pos(0.0, 9900.0),
            "virtual size must override content_rect"
        );
    }

    // ============================================== ScrollInputQueue (std only)
    // (constructor / getter / predicate / numeric)

    #[test]
    fn input_queue_new_is_empty_and_default_matches() {
        let q = ScrollInputQueue::new();
        assert!(!q.has_pending());
        assert!(q.take_all().is_empty());
        assert!(q.take_recent(10).is_empty());
        assert!(!ScrollInputQueue::default().has_pending());
    }

    #[test]
    fn input_queue_take_all_drains_and_preserves_push_order() {
        let q = ScrollInputQueue::new();
        q.push(input(1.0, 1.0, 1));
        q.push(input(2.0, 2.0, 2));
        assert!(q.has_pending());
        let taken = q.take_all();
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].delta.x, 1.0);
        assert_eq!(taken[1].delta.x, 2.0);
        assert!(!q.has_pending(), "take_all must drain the queue");
        assert!(q.take_all().is_empty(), "second take_all is empty, not stale");
    }

    #[test]
    fn input_queue_take_recent_zero_discards_everything() {
        // max_events = 0: `drain(..len - 0)` removes every event. Documented as
        // "older events beyond max_events are discarded" — with 0 that is all of
        // them, and the queue is left empty (the backlog is dropped, not kept).
        let q = ScrollInputQueue::new();
        q.push(input(1.0, 1.0, 1));
        q.push(input(2.0, 2.0, 2));
        let taken = q.take_recent(0);
        assert!(taken.is_empty(), "take_recent(0) must return nothing");
        assert!(!q.has_pending(), "take_recent(0) must still drain the queue");
    }

    #[test]
    fn input_queue_take_recent_keeps_the_newest_events_sorted_oldest_first() {
        let q = ScrollInputQueue::new();
        // Pushed out of timestamp order on purpose.
        q.push(input(0.0, 0.0, 5));
        q.push(input(0.0, 0.0, 1));
        q.push(input(0.0, 0.0, 3));
        q.push(input(0.0, 0.0, 9));
        let taken = q.take_recent(2);
        assert_eq!(taken.len(), 2, "backlog must be truncated to max_events");
        assert_eq!(taken[0].timestamp, at(5));
        assert_eq!(taken[1].timestamp, at(9), "newest event must be last");
        assert!(!q.has_pending());
    }

    #[test]
    fn input_queue_take_recent_below_limit_returns_push_order_not_sorted() {
        // NOTE: the doc says "sorted by timestamp (newest last)", but the sort
        // only runs on the overflow path (len > max_events). Below the limit the
        // events come back in PUSH order. Pinning the real behavior here.
        let q = ScrollInputQueue::new();
        q.push(input(0.0, 0.0, 5));
        q.push(input(0.0, 0.0, 1));
        q.push(input(0.0, 0.0, 3));
        let taken = q.take_recent(3);
        assert_eq!(taken.len(), 3);
        let stamps: Vec<_> = taken.iter().map(|e| e.timestamp.clone()).collect();
        assert_eq!(stamps, vec![at(5), at(1), at(3)]);
    }

    #[test]
    fn input_queue_take_recent_usize_max_does_not_overflow() {
        // `events.len() - max_events` would underflow if the length guard were
        // wrong; usize::MAX must simply mean "take everything".
        let q = ScrollInputQueue::new();
        q.push(input(1.0, 0.0, 1));
        q.push(input(2.0, 0.0, 2));
        let taken = q.take_recent(usize::MAX);
        assert_eq!(taken.len(), 2);
        assert!(!q.has_pending());
        // Empty queue + usize::MAX: still no underflow, no panic.
        assert!(q.take_recent(usize::MAX).is_empty());
        assert!(q.take_recent(0).is_empty());
    }

    #[test]
    fn input_queue_clone_shares_one_backing_store() {
        // The timer callback holds a clone; a push through either handle must be
        // visible to the other, otherwise inputs would silently vanish.
        let q = ScrollInputQueue::new();
        let c = q.clone();
        c.push(input(1.0, 2.0, 1));
        assert!(q.has_pending(), "clone must not deep-copy the queue");
        assert_eq!(q.take_all().len(), 1);
        assert!(!c.has_pending(), "draining one handle drains both");
    }

    #[test]
    fn input_queue_accepts_non_finite_deltas_without_panicking() {
        let q = ScrollInputQueue::new();
        q.push(input(f32::NAN, f32::INFINITY, 1));
        q.push(input(f32::MAX, f32::MIN, 2));
        let taken = q.take_recent(usize::MAX);
        assert_eq!(taken.len(), 2);
        assert!(taken[0].delta.x.is_nan());
        assert_eq!(taken[0].delta.y, f32::INFINITY);
    }

    // ======================================= ScrollbarState::hit_test_component
    // (numeric: zero / negative / nan_inf / boundary)

    #[test]
    fn hit_test_component_vertical_maps_each_region() {
        let sb = scrollbar(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 16.0, 100.0),
            16.0, // button_size
            10.0, // thumb_offset (from end of top button)
            30.0, // thumb_length
        );
        assert_eq!(sb.hit_test_component(pos(8.0, 0.0)), ScrollbarComponent::TopButton);
        assert_eq!(sb.hit_test_component(pos(8.0, 15.9)), ScrollbarComponent::TopButton);
        assert_eq!(
            sb.hit_test_component(pos(8.0, 99.0)),
            ScrollbarComponent::BottomButton
        );
        // Thumb spans [16 + 10, 16 + 10 + 30] = [26, 56].
        assert_eq!(sb.hit_test_component(pos(8.0, 26.0)), ScrollbarComponent::Thumb);
        assert_eq!(sb.hit_test_component(pos(8.0, 56.0)), ScrollbarComponent::Thumb);
        assert_eq!(sb.hit_test_component(pos(8.0, 20.0)), ScrollbarComponent::Track);
        assert_eq!(sb.hit_test_component(pos(8.0, 60.0)), ScrollbarComponent::Track);
    }

    #[test]
    fn hit_test_component_boundaries_are_exact() {
        let sb = scrollbar(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 16.0, 100.0),
            16.0,
            0.0,
            30.0,
        );
        // y == button_size is NOT the top button (strict <) — it is the thumb start.
        assert_eq!(sb.hit_test_component(pos(0.0, 16.0)), ScrollbarComponent::Thumb);
        // y == track_height - button_size is NOT the bottom button (strict >).
        assert_eq!(sb.hit_test_component(pos(0.0, 84.0)), ScrollbarComponent::Track);
        assert_eq!(
            sb.hit_test_component(pos(0.0, 84.001)),
            ScrollbarComponent::BottomButton
        );
    }

    #[test]
    fn hit_test_component_overlay_zero_button_size_has_no_buttons() {
        // Overlay scrollbars get button_size == 0: y == 0 must NOT be a TopButton
        // (strict `<` means the button region is empty).
        let sb = scrollbar(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 8.0, 100.0),
            0.0,
            0.0,
            50.0,
        );
        assert_eq!(sb.hit_test_component(pos(0.0, 0.0)), ScrollbarComponent::Thumb);
        assert_eq!(sb.hit_test_component(pos(0.0, 50.0)), ScrollbarComponent::Thumb);
        assert_eq!(sb.hit_test_component(pos(0.0, 60.0)), ScrollbarComponent::Track);
        // y == track_height is still not "> track_height - 0" ... it IS equal, so Track.
        assert_eq!(sb.hit_test_component(pos(0.0, 100.0)), ScrollbarComponent::Track);
    }

    #[test]
    fn hit_test_component_nan_position_falls_through_to_track() {
        // Every float comparison against NaN is false, so NaN lands in the
        // final `else` — Track. Deterministic, no panic, no phantom button click.
        let sb = scrollbar(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 16.0, 100.0),
            16.0,
            10.0,
            30.0,
        );
        assert_eq!(
            sb.hit_test_component(pos(f32::NAN, f32::NAN)),
            ScrollbarComponent::Track
        );
        let hb = scrollbar(
            ScrollbarOrientation::Horizontal,
            rect(0.0, 0.0, 100.0, 16.0),
            16.0,
            10.0,
            30.0,
        );
        assert_eq!(
            hb.hit_test_component(pos(f32::NAN, f32::NAN)),
            ScrollbarComponent::Track
        );
    }

    #[test]
    fn hit_test_component_infinite_position_picks_an_end_button() {
        let sb = scrollbar(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 16.0, 100.0),
            16.0,
            10.0,
            30.0,
        );
        assert_eq!(
            sb.hit_test_component(pos(0.0, f32::NEG_INFINITY)),
            ScrollbarComponent::TopButton
        );
        assert_eq!(
            sb.hit_test_component(pos(0.0, f32::INFINITY)),
            ScrollbarComponent::BottomButton
        );
        assert_eq!(
            sb.hit_test_component(pos(0.0, f32::MIN)),
            ScrollbarComponent::TopButton
        );
        assert_eq!(
            sb.hit_test_component(pos(0.0, f32::MAX)),
            ScrollbarComponent::BottomButton
        );
    }

    #[test]
    fn hit_test_component_ignores_the_cross_axis() {
        // A vertical scrollbar must not care about x (and vice versa) — otherwise
        // a drag that leaves the bar sideways would change component mid-gesture.
        let v = scrollbar(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 16.0, 100.0),
            16.0,
            10.0,
            30.0,
        );
        for x in [-1e9, -1.0, 0.0, 8.0, 1e9, f32::NAN] {
            assert_eq!(v.hit_test_component(pos(x, 30.0)), ScrollbarComponent::Thumb);
        }
        let h = scrollbar(
            ScrollbarOrientation::Horizontal,
            rect(0.0, 0.0, 100.0, 16.0),
            16.0,
            10.0,
            30.0,
        );
        for y in [-1e9, -1.0, 0.0, 8.0, 1e9, f32::NAN] {
            assert_eq!(h.hit_test_component(pos(30.0, y)), ScrollbarComponent::Thumb);
        }
    }

    #[test]
    fn hit_test_component_degenerate_track_shorter_than_buttons_prefers_top() {
        // button_size > track length: the top/bottom regions overlap. First match
        // wins (TopButton) — no panic, no ambiguity.
        let sb = scrollbar(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 16.0, 4.0),
            16.0,
            0.0,
            0.0,
        );
        assert_eq!(sb.hit_test_component(pos(0.0, 0.0)), ScrollbarComponent::TopButton);
        assert_eq!(sb.hit_test_component(pos(0.0, 3.0)), ScrollbarComponent::TopButton);
    }

    // ========================================================= ScrollManager::new
    // (constructor / getters / predicates on an empty instance)

    #[test]
    fn manager_new_is_empty_and_traditional_by_default() {
        let m = ScrollManager::new();
        assert_eq!(m.debug_counts(), (0, 0));
        assert!(!m.has_active_animations());
        assert!(!m.has_pending_scroll_changes());
        assert!(!m.is_natural_scroll());
        assert_eq!(m.scroll_sign(), -1.0);
        assert!(m.pending_wheel_event.is_none());
        assert!(!m.get_input_queue().has_pending());
        // Getters on an empty manager return None / empty, never panic.
        assert!(m.get_current_offset(DOM, node(0)).is_none());
        assert!(m.get_last_activity_time(DOM, node(0)).is_none());
        assert!(m.get_scroll_state(DOM, node(0)).is_none());
        assert!(m.get_scroll_node_info(DOM, node(0)).is_none());
        assert!(m.a11y_scroll_info(DOM, node(0)).is_none());
        assert!(m.get_scroll_states_for_dom(DOM).is_empty());
        assert!(m
            .get_scrollbar_state(DOM, node(0), ScrollbarOrientation::Vertical)
            .is_none());
        assert!(m.hit_test_scrollbars(pos(0.0, 0.0)).is_none());
        assert_eq!(m.iter_scrollbar_states().count(), 0);
        assert!(!m.is_node_scrollable(DOM, node(0)));
        assert!(!m.can_consume_delta(DOM, node(0), 10.0, 10.0));
    }

    #[test]
    fn scroll_sign_flips_with_the_preference() {
        let mut m = ScrollManager::new();
        assert_eq!(m.scroll_sign(), -1.0);
        m.set_natural_scroll(true);
        assert!(m.is_natural_scroll());
        assert_eq!(m.scroll_sign(), 1.0);
        m.set_natural_scroll(false);
        assert_eq!(m.scroll_sign(), -1.0);
        // Idempotent: setting the same value twice must not toggle.
        m.set_natural_scroll(false);
        assert_eq!(m.scroll_sign(), -1.0);
    }

    // ================================================= dirty-flag bookkeeping
    // (predicate: has_pending_scroll_changes / clear_scroll_dirty)

    #[test]
    fn scroll_dirty_is_set_only_on_a_real_move_and_cleared_on_demand() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        assert!(!m.has_pending_scroll_changes());

        // Sub-epsilon move (< SCROLL_CHANGE_EPSILON = 0.01) must NOT dirty the
        // display list — otherwise every trackpad jitter forces a rebuild.
        m.set_scroll_position(DOM, node(0), pos(0.0, 0.005), at(1));
        assert!(!m.has_pending_scroll_changes(), "0.005px must not be 'moved'");

        m.set_scroll_position(DOM, node(0), pos(0.0, 50.0), at(2));
        assert!(m.has_pending_scroll_changes());

        m.clear_scroll_dirty();
        assert!(!m.has_pending_scroll_changes());
        // Setting the SAME position again is a no-op move: stays clean.
        m.set_scroll_position(DOM, node(0), pos(0.0, 50.0), at(3));
        assert!(!m.has_pending_scroll_changes());
    }

    #[test]
    fn clear_scroll_dirty_on_a_clean_manager_is_a_noop() {
        let mut m = ScrollManager::new();
        m.clear_scroll_dirty();
        m.clear_scroll_dirty();
        assert!(!m.has_pending_scroll_changes());
    }

    // ======================================= set_scroll_position (+unclamped)
    // (numeric: zero / min_max / negative / overflow / nan)

    #[test]
    fn set_scroll_position_clamps_extremes_into_bounds() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(f32::MAX, f32::MAX), at(1));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 400.0)));

        m.set_scroll_position(DOM, node(0), pos(f32::MIN, f32::MIN), at(2));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 0.0)));

        m.set_scroll_position(DOM, node(0), pos(f32::INFINITY, f32::INFINITY), at(3));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 400.0)));

        m.set_scroll_position(DOM, node(0), pos(f32::NAN, f32::NAN), at(4));
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan(), "clamped path must kill NaN");
        assert_eq!(off, LogicalPosition::zero());
    }

    #[test]
    fn set_scroll_position_cancels_a_running_animation() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.scroll_to(DOM, node(0), pos(0.0, 300.0), tick_dur(100), EasingFunction::Linear, at(0));
        assert!(m.has_active_animations());
        m.set_scroll_position(DOM, node(0), pos(0.0, 10.0), at(1));
        assert!(!m.has_active_animations(), "an explicit set must win over easing");
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 10.0)));
    }

    #[test]
    fn set_scroll_position_on_an_unknown_node_creates_a_pinned_zero_state() {
        // The entry API inserts a zero-sized state, so the offset can only be 0 —
        // and the map grows by exactly one (no unbounded growth per call).
        let mut m = ScrollManager::new();
        m.set_scroll_position(DOM, node(42), pos(500.0, 500.0), at(1));
        assert_eq!(m.get_current_offset(DOM, node(42)), Some(LogicalPosition::zero()));
        assert_eq!(m.debug_counts(), (1, 0));
        m.set_scroll_position(DOM, node(42), pos(600.0, 600.0), at(2));
        assert_eq!(m.debug_counts(), (1, 0), "repeat set must not grow the map");
    }

    #[test]
    fn set_scroll_position_unclamped_keeps_overscroll_values_verbatim() {
        // The physics timer relies on being able to push the offset OUTSIDE
        // [0, max] for rubber-banding — clamping here would kill the bounce.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position_unclamped(DOM, node(0), pos(-50.0, -80.0), at(1));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(-50.0, -80.0)));
        m.set_scroll_position_unclamped(DOM, node(0), pos(0.0, 9999.0), at(2));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 9999.0)));
        assert!(m.has_pending_scroll_changes());
    }

    #[test]
    fn set_scroll_position_unclamped_stores_non_finite_values_unfiltered() {
        // Documents a real hazard: the unclamped path performs NO sanitization,
        // so a NaN delta from a driver would be stored verbatim AND (because
        // `(NaN - x).abs() > EPS` is false) would not even mark the state dirty.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position_unclamped(DOM, node(0), pos(f32::NAN, f32::NAN), at(1));
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(off.x.is_nan() && off.y.is_nan(), "unclamped stores NaN as-is");
        assert!(
            !m.has_pending_scroll_changes(),
            "a NaN write does not trip the dirty flag (NaN comparisons are false)"
        );
        // But a later re-registration re-clamps it back to a finite value.
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 500.0),
            at(2),
            16.0,
            16.0,
            false,
            true,
        );
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan(), "re-clamp must sanitize NaN");
    }

    // ================================================== scroll_to / scroll_by
    // (numeric + animation lifecycle)

    #[test]
    fn scroll_to_zero_duration_is_immediate_for_both_clock_kinds() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.scroll_to(DOM, node(0), pos(0.0, 100.0), tick_dur(0), EasingFunction::Linear, at(1));
        assert!(!m.has_active_animations(), "zero Tick duration must not animate");
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 100.0)));

        m.scroll_to(
            DOM,
            node(0),
            pos(0.0, 200.0),
            sys_dur(0, 0),
            EasingFunction::EaseOut,
            at(2),
        );
        assert!(!m.has_active_animations(), "zero System duration must not animate");
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 200.0)));
    }

    #[test]
    fn scroll_to_clamps_the_animation_target_not_just_the_final_offset() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.scroll_to(DOM, node(0), pos(0.0, 1e9), tick_dur(100), EasingFunction::Linear, at(0));
        let anim_target = m
            .get_scroll_state(DOM, node(0))
            .and_then(|s| s.animation.as_ref())
            .map(|a| a.target_offset)
            .unwrap();
        assert_eq!(anim_target, pos(0.0, 400.0), "target must be pre-clamped");
        // Drive it to completion: the offset lands exactly on the clamped target.
        let r = m.tick(at(100));
        assert!(r.needs_repaint);
        assert_eq!(r.updated_nodes, vec![(DOM, node(0))]);
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 400.0)));
        assert!(!m.has_active_animations(), "animation must clear at t >= 1");
    }

    #[test]
    fn scroll_to_nan_target_animates_to_the_origin_never_to_nan() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 200.0), at(0));
        m.scroll_to(
            DOM,
            node(0),
            pos(f32::NAN, f32::NAN),
            tick_dur(10),
            EasingFunction::Linear,
            at(0),
        );
        m.tick(at(10));
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan(), "NaN target must be clamped away");
        assert_eq!(off, LogicalPosition::zero());
    }

    #[test]
    fn scroll_by_accumulates_from_the_current_offset_and_saturates() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.scroll_by(DOM, node(0), pos(0.0, 100.0), tick_dur(0), EasingFunction::Linear, at(1));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 100.0)));
        m.scroll_by(DOM, node(0), pos(0.0, 100.0), tick_dur(0), EasingFunction::Linear, at(2));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 200.0)));
        // A delta big enough to overflow f32 arithmetic: saturates at max travel.
        m.scroll_by(
            DOM,
            node(0),
            pos(f32::MAX, f32::MAX),
            tick_dur(0),
            EasingFunction::Linear,
            at(3),
        );
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 400.0)));
        // ...and back down past the origin.
        m.scroll_by(
            DOM,
            node(0),
            pos(f32::MIN, f32::MIN),
            tick_dur(0),
            EasingFunction::Linear,
            at(4),
        );
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(LogicalPosition::zero()));
    }

    #[test]
    fn scroll_by_on_an_unknown_node_defaults_to_origin_and_stays_pinned() {
        let mut m = ScrollManager::new();
        m.scroll_by(
            DOM,
            node(7),
            pos(1e9, 1e9),
            tick_dur(0),
            EasingFunction::Linear,
            at(1),
        );
        // No bounds registered => max travel 0 => still at the origin, no panic.
        assert_eq!(m.get_current_offset(DOM, node(7)), Some(LogicalPosition::zero()));
    }

    #[test]
    fn scroll_by_nan_delta_does_not_poison_the_offset() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 100.0), at(0));
        m.scroll_by(
            DOM,
            node(0),
            pos(f32::NAN, f32::NAN),
            tick_dur(0),
            EasingFunction::Linear,
            at(1),
        );
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan());
        assert_eq!(off, LogicalPosition::zero(), "NaN target clamps to origin");
    }

    // =============================================================== tick()
    // (other: no_panic_smoke + animation invariants)

    #[test]
    fn tick_on_an_empty_manager_reports_no_work() {
        let mut m = ScrollManager::new();
        let r = m.tick(at(1));
        assert!(!r.needs_repaint);
        assert!(r.updated_nodes.is_empty());
    }

    #[test]
    fn tick_interpolates_linearly_and_completes_exactly_once() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.scroll_to(DOM, node(0), pos(0.0, 400.0), tick_dur(100), EasingFunction::Linear, at(0));

        let r = m.tick(at(50));
        assert!(r.needs_repaint);
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 200.0)));
        assert!(m.has_active_animations(), "still mid-flight at t = 0.5");

        let r = m.tick(at(100));
        assert!(r.needs_repaint);
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 400.0)));
        assert!(!m.has_active_animations());

        // Ticking past the end must be a no-op, not a re-run.
        let r = m.tick(at(500));
        assert!(!r.needs_repaint);
        assert!(r.updated_nodes.is_empty());
    }

    #[test]
    fn tick_before_the_animation_start_time_saturates_to_zero_progress() {
        // `now` earlier than `start_time` => duration_since saturates to 0 =>
        // t = 0 => offset stays at start. No negative-progress overshoot.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 50.0), at(0));
        m.scroll_to(DOM, node(0), pos(0.0, 400.0), tick_dur(100), EasingFunction::Linear, at(100));
        m.tick(at(0)); // clock went backwards
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 50.0)));
        assert!(m.has_active_animations(), "no progress => still animating");
    }

    #[test]
    fn tick_with_a_zero_duration_animation_completes_instead_of_producing_nan() {
        // 0/0 = NaN, but `NaN.min(1.0)` == 1.0 in Rust, so the animation snaps to
        // its target and is cleared — the offset never becomes NaN. (scroll_to
        // short-circuits zero durations; this covers a hand-built animation, e.g.
        // one whose duration was computed to zero.)
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.states.get_mut(&(DOM, node(0))).unwrap().animation = Some(ScrollAnimation {
            start_time: at(0),
            duration: tick_dur(0),
            start_offset: pos(0.0, 0.0),
            target_offset: pos(0.0, 300.0),
            easing: EasingFunction::Linear,
        });
        let r = m.tick(at(0));
        assert!(r.needs_repaint);
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.y.is_nan(), "0/0 must not leak NaN into the offset");
        assert_eq!(off, pos(0.0, 300.0));
        assert!(!m.has_active_animations());
    }

    #[test]
    fn tick_with_a_mismatched_clock_kind_stalls_at_zero_instead_of_panicking() {
        // Tick-clock animation ticked by a System instant: duration_since and div
        // both saturate to 0 => t = 0 forever. The animation never advances and
        // never completes — but it does not panic or corrupt the offset.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 25.0), at(0));
        m.scroll_to(DOM, node(0), pos(0.0, 400.0), tick_dur(10), EasingFunction::Linear, at(0));
        let r = m.tick(Instant::now()); // System clock vs Tick animation
        assert!(r.needs_repaint);
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 25.0)));
        assert!(
            m.has_active_animations(),
            "mismatched clocks stall the animation (t stays 0) — it never completes"
        );
    }

    #[test]
    fn tick_advances_every_animating_node_in_one_pass() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.register_or_update_scroll_node(
            DOM,
            node(1),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 300.0),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );
        m.scroll_to(DOM, node(0), pos(0.0, 400.0), tick_dur(10), EasingFunction::Linear, at(0));
        m.scroll_to(DOM, node(1), pos(0.0, 200.0), tick_dur(10), EasingFunction::Linear, at(0));
        let r = m.tick(at(10));
        assert_eq!(r.updated_nodes.len(), 2);
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 400.0)));
        assert_eq!(m.get_current_offset(DOM, node(1)), Some(pos(0.0, 200.0)));
    }

    // ============================================== register_or_update_scroll_node
    // (numeric: nan_inf / zero / min_max + no unbounded growth)

    #[test]
    fn register_twice_updates_in_place_and_keeps_the_offset() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 300.0), at(1));
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 500.0),
            at(2),
            16.0,
            16.0,
            false,
            true,
        );
        assert_eq!(m.debug_counts(), (1, 0), "re-register must not grow the map");
        assert_eq!(
            m.get_current_offset(DOM, node(0)),
            Some(pos(0.0, 300.0)),
            "an existing node keeps its scroll offset across relayout"
        );
    }

    #[test]
    fn re_registering_with_shrunken_content_re_clamps_the_offset() {
        // The classic resize bug: content shrinks under a scrolled-to-bottom node.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 400.0), at(1));
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 150.0), // content shrank: max_y is now 50
            at(2),
            16.0,
            16.0,
            false,
            true,
        );
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 50.0)));
    }

    #[test]
    fn register_with_non_finite_geometry_does_not_panic_or_leak_nan() {
        let mut m = ScrollManager::new();
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            size(f32::NAN, f32::NAN),
            at(0),
            f32::NAN,
            f32::NAN,
            true,
            true,
        );
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan(), "NaN geometry must clamp to 0");
        assert_eq!(off, LogicalPosition::zero());
        assert!(!m.is_node_scrollable(DOM, node(0)), "NaN overflow check is false");

        m.register_or_update_scroll_node(
            DOM,
            node(1),
            rect(0.0, 0.0, f32::INFINITY, f32::INFINITY),
            size(f32::INFINITY, f32::INFINITY),
            at(0),
            f32::MAX,
            f32::MAX,
            true,
            true,
        );
        let off = m.get_current_offset(DOM, node(1)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan());
        assert_eq!(m.debug_counts(), (2, 0));
    }

    #[test]
    fn register_with_zero_sized_geometry_yields_a_non_scrollable_pinned_node() {
        let mut m = ScrollManager::new();
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            LogicalRect::zero(),
            LogicalSize::zero(),
            at(0),
            0.0,
            0.0,
            false,
            false,
        );
        assert!(!m.is_node_scrollable(DOM, node(0)));
        assert!(m.a11y_scroll_info(DOM, node(0)).is_none());
        let info = m.get_scroll_node_info(DOM, node(0)).unwrap();
        assert_eq!(info.max_scroll_x, 0.0);
        assert_eq!(info.max_scroll_y, 0.0);
    }

    // ===================================================== is_node_scrollable
    // (predicate: basic_true_false / edge_inputs)

    #[test]
    fn is_node_scrollable_is_strict_overflow_not_equality() {
        let mut m = ScrollManager::new();
        // Content exactly equal to the container: NOT scrollable (strict `>`).
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 100.0),
            at(0),
            16.0,
            16.0,
            false,
            false,
        );
        assert!(!m.is_node_scrollable(DOM, node(0)));
        // One extra pixel of height => scrollable.
        m.register_or_update_scroll_node(
            DOM,
            node(1),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 100.1),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );
        assert!(m.is_node_scrollable(DOM, node(1)));
        // Unknown node / unknown DOM => false, never a panic.
        assert!(!m.is_node_scrollable(DOM, node(999)));
        assert!(!m.is_node_scrollable(DOM1, node(1)));
    }

    #[test]
    fn is_node_scrollable_uses_the_virtual_size_when_present() {
        let mut m = ScrollManager::new();
        // Rendered content is tiny (only the visible slice), virtual content is huge.
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 50.0),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );
        assert!(!m.is_node_scrollable(DOM, node(0)));
        m.update_virtual_scroll_bounds(DOM, node(0), size(100.0, 100_000.0), None);
        assert!(
            m.is_node_scrollable(DOM, node(0)),
            "a VirtualView with a large virtual size must be scrollable"
        );
    }

    // ======================================================= can_consume_delta
    // (predicate: boundary / nan)

    #[test]
    fn can_consume_delta_respects_the_half_pixel_deadzone() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0)); // max_y = 400
        m.set_scroll_position(DOM, node(0), pos(0.0, 200.0), at(1));
        // |eff| <= EPS (0.5) is "not moved" on that axis.
        assert!(!m.can_consume_delta(DOM, node(0), 0.0, 0.0));
        assert!(!m.can_consume_delta(DOM, node(0), 0.5, 0.5), "exactly EPS is a no-move");
        assert!(!m.can_consume_delta(DOM, node(0), -0.5, -0.5));
        assert!(m.can_consume_delta(DOM, node(0), 0.0, 0.51));
        assert!(m.can_consume_delta(DOM, node(0), 0.0, -0.51));
        // X has no travel at all (content width == container width).
        assert!(!m.can_consume_delta(DOM, node(0), 100.0, 0.0));
    }

    #[test]
    fn can_consume_delta_is_false_at_the_pinned_edges() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        // Pinned at the top: cannot go further up, can go down.
        m.set_scroll_position(DOM, node(0), pos(0.0, 0.0), at(1));
        assert!(!m.can_consume_delta(DOM, node(0), 0.0, -10.0));
        assert!(m.can_consume_delta(DOM, node(0), 0.0, 10.0));
        // Pinned at the bottom: the mirror image.
        m.set_scroll_position(DOM, node(0), pos(0.0, 400.0), at(2));
        assert!(!m.can_consume_delta(DOM, node(0), 0.0, 10.0));
        assert!(m.can_consume_delta(DOM, node(0), 0.0, -10.0));
    }

    #[test]
    fn can_consume_delta_rejects_nan_and_accepts_infinite_deltas() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 200.0), at(1));
        assert!(
            !m.can_consume_delta(DOM, node(0), f32::NAN, f32::NAN),
            "a NaN delta consumes nothing (every comparison is false)"
        );
        assert!(m.can_consume_delta(DOM, node(0), 0.0, f32::INFINITY));
        assert!(m.can_consume_delta(DOM, node(0), 0.0, f32::NEG_INFINITY));
        assert!(m.can_consume_delta(DOM, node(0), 0.0, f32::MAX));
        assert!(!m.can_consume_delta(DOM, node(999), 0.0, f32::MAX), "unknown node");
    }

    // ==================================================== select_scroll_target
    // (numeric: nan_inf / zero + fallback invariants)

    #[test]
    fn select_scroll_target_on_no_candidates_is_none() {
        let m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        assert!(m
            .select_scroll_target(core::iter::empty(), 0.0, 10.0)
            .is_none());
        // Candidates that are not scrollable are skipped entirely (no fallback).
        assert!(m
            .select_scroll_target([(DOM, node(50)), (DOM1, node(0))].into_iter(), 0.0, 10.0)
            .is_none());
    }

    #[test]
    fn select_scroll_target_with_zero_or_nan_delta_falls_back_to_the_innermost() {
        // Nothing "can consume" a zero/NaN delta, so the gesture still anchors on
        // the innermost scrollable node rather than being dropped.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.register_or_update_scroll_node(
            DOM,
            node(9),
            rect(0.0, 0.0, 50.0, 50.0),
            size(50.0, 200.0),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );
        let inner_first = [(DOM, node(9)), (DOM, node(0))];
        assert_eq!(
            m.select_scroll_target(inner_first.into_iter(), 0.0, 0.0),
            Some((DOM, node(9)))
        );
        assert_eq!(
            m.select_scroll_target(inner_first.into_iter(), f32::NAN, f32::NAN),
            Some((DOM, node(9)))
        );
        // An infinite delta IS consumable => also the innermost (it has room).
        assert_eq!(
            m.select_scroll_target(inner_first.into_iter(), 0.0, f32::INFINITY),
            Some((DOM, node(9)))
        );
    }

    // ======================================================= a11y_scroll_info
    // (other: no_panic_smoke)

    #[test]
    fn a11y_scroll_info_reports_travel_only_for_scrollable_nodes() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 120.0), at(1));
        let (off, max_x, max_y) = m.a11y_scroll_info(DOM, node(0)).unwrap();
        assert_eq!(off, pos(0.0, 120.0));
        assert_eq!(max_x, 0.0);
        assert_eq!(max_y, 400.0);

        // Non-scrollable node => None (screen readers must not offer scroll actions).
        m.register_or_update_scroll_node(
            DOM,
            node(1),
            rect(0.0, 0.0, 100.0, 100.0),
            size(10.0, 10.0),
            at(0),
            16.0,
            16.0,
            false,
            false,
        );
        assert!(m.a11y_scroll_info(DOM, node(1)).is_none());
        assert!(m.a11y_scroll_info(DOM, node(404)).is_none());
        assert!(m.a11y_scroll_info(DOM1, node(0)).is_none());
    }

    #[test]
    fn a11y_scroll_info_uses_the_virtual_size() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 100.0));
        assert!(m.a11y_scroll_info(DOM, node(0)).is_none());
        m.update_virtual_scroll_bounds(DOM, node(0), size(100.0, 1000.0), None);
        let (_, max_x, max_y) = m.a11y_scroll_info(DOM, node(0)).unwrap();
        assert_eq!(max_x, 0.0);
        assert_eq!(max_y, 900.0);
    }

    // ================================================== get_scroll_node_info
    // (other: no_panic_smoke — max_scroll is never negative)

    #[test]
    fn get_scroll_node_info_max_scroll_is_never_negative() {
        let m = mgr(size(500.0, 500.0), size(10.0, 10.0));
        let info = m.get_scroll_node_info(DOM, node(0)).unwrap();
        assert_eq!(info.max_scroll_x, 0.0, "underflow must clamp to 0, not go negative");
        assert_eq!(info.max_scroll_y, 0.0);
        assert_eq!(info.current_offset, LogicalPosition::zero());
        assert!(m.get_scroll_node_info(DOM, node(1)).is_none());
    }

    #[test]
    fn get_scroll_node_info_prefers_the_virtual_size_for_max_travel() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 200.0));
        assert_eq!(m.get_scroll_node_info(DOM, node(0)).unwrap().max_scroll_y, 100.0);
        m.update_virtual_scroll_bounds(DOM, node(0), size(600.0, 5000.0), Some(pos(1.0, 2.0)));
        let info = m.get_scroll_node_info(DOM, node(0)).unwrap();
        assert_eq!(info.max_scroll_x, 500.0);
        assert_eq!(info.max_scroll_y, 4900.0);
        // content_rect is still the *rendered* rect — the virtual size only moves
        // the bounds, it does not rewrite the layout geometry.
        assert_eq!(info.content_rect.size, size(100.0, 200.0));
    }

    // ============================================ update_virtual_scroll_bounds
    // (numeric: nan_inf / zero + implicit state creation)

    #[test]
    fn update_virtual_scroll_bounds_creates_a_state_for_an_unknown_node() {
        let mut m = ScrollManager::new();
        m.update_virtual_scroll_bounds(DOM, node(3), size(100.0, 9000.0), Some(pos(0.0, 4.0)));
        assert_eq!(m.debug_counts(), (1, 0));
        let s = m.get_scroll_state(DOM, node(3)).unwrap();
        assert_eq!(s.virtual_scroll_size, Some(size(100.0, 9000.0)));
        assert_eq!(s.virtual_scroll_offset, Some(pos(0.0, 4.0)));
        assert_eq!(s.current_offset, LogicalPosition::zero());
        // Container is still zero-sized, so all 9000px are reachable.
        assert!(m.is_node_scrollable(DOM, node(3)));
    }

    #[test]
    fn update_virtual_scroll_bounds_re_clamps_a_shrinking_virtual_size() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 100.0));
        m.update_virtual_scroll_bounds(DOM, node(0), size(100.0, 5000.0), None);
        m.set_scroll_position(DOM, node(0), pos(0.0, 4900.0), at(1));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 4900.0)));
        // The VirtualView shrinks (rows removed): the offset must follow it down.
        m.update_virtual_scroll_bounds(DOM, node(0), size(100.0, 300.0), None);
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 200.0)));
    }

    #[test]
    fn update_virtual_scroll_bounds_with_non_finite_size_does_not_panic() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 400.0), at(1));
        m.update_virtual_scroll_bounds(DOM, node(0), size(f32::NAN, f32::NAN), None);
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan());
        assert_eq!(off, LogicalPosition::zero(), "NaN virtual size => zero travel");
        assert!(!m.is_node_scrollable(DOM, node(0)));

        m.update_virtual_scroll_bounds(DOM, node(0), size(0.0, f32::INFINITY), None);
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.y.is_nan(), "infinite virtual height must not produce NaN");
    }

    // ====================================================== update_node_bounds
    // (numeric: zero / negative / overflow / nan)

    #[test]
    fn update_node_bounds_creates_the_state_and_re_clamps_a_shrinking_content() {
        let mut m = ScrollManager::new();
        // Unknown node: the entry API materializes it at the scroll origin.
        m.update_node_bounds(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            rect(0.0, 0.0, 100.0, 500.0),
            at(0),
        );
        assert_eq!(m.debug_counts(), (1, 0));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(LogicalPosition::zero()));

        m.set_scroll_position(DOM, node(0), pos(0.0, 400.0), at(1));
        m.clear_scroll_dirty();

        // Content shrinks under a bottomed-out scroll: the offset must follow.
        m.update_node_bounds(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            rect(0.0, 0.0, 100.0, 150.0),
            at(2),
        );
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 50.0)));
        // NOTE: the forced re-clamp moved the offset by 350px but did NOT set the
        // dirty flag (unlike set_scroll_position) — pinning the real behavior.
        assert!(!m.has_pending_scroll_changes());
    }

    #[test]
    fn update_node_bounds_ignores_the_content_rect_origin() {
        // clamp() only reads `size`, so a content rect translated far away must
        // not shift the reachable travel.
        let mut m = ScrollManager::new();
        m.update_node_bounds(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            rect(999.0, 999.0, 100.0, 500.0),
            at(0),
        );
        m.set_scroll_position(DOM, node(0), pos(1e9, 1e9), at(1));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 400.0)));
    }

    #[test]
    fn update_node_bounds_with_non_finite_rects_does_not_leak_nan() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 400.0), at(1));
        m.update_node_bounds(
            DOM,
            node(0),
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            at(2),
        );
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(!off.x.is_nan() && !off.y.is_nan(), "NaN bounds must clamp to 0");
        assert_eq!(off, LogicalPosition::zero());

        // Infinite content: the offset stays finite (clamped to the old value).
        m.update_node_bounds(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            rect(0.0, 0.0, f32::INFINITY, f32::INFINITY),
            at(3),
        );
        let off = m.get_current_offset(DOM, node(0)).unwrap();
        assert!(off.x.is_finite() && off.y.is_finite());
    }

    // ============================================ get_scroll_states_for_dom /
    //                                              build_scroll_offset_map
    // (other: no_panic_smoke + DOM isolation)

    #[test]
    fn get_scroll_states_for_dom_filters_by_dom_and_reports_the_live_offset() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 42.0), at(1));
        m.register_or_update_scroll_node(
            DOM1,
            node(0),
            rect(0.0, 0.0, 10.0, 10.0),
            size(10.0, 100.0),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );

        let states = m.get_scroll_states_for_dom(DOM);
        assert_eq!(states.len(), 1, "other DOMs must not leak in");
        let sp = states.get(&node(0)).unwrap();
        assert_eq!(sp.parent_rect, rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(sp.children_rect.origin, pos(0.0, 42.0));
        assert_eq!(sp.children_rect.size, size(100.0, 500.0));

        // A DOM with no registered nodes returns an empty map, not a panic.
        assert!(m.get_scroll_states_for_dom(DomId { inner: 99 }).is_empty());
    }

    #[test]
    fn get_scroll_states_for_dom_uses_the_virtual_size_as_children_rect() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 120.0));
        m.update_virtual_scroll_bounds(DOM, node(0), size(100.0, 8000.0), None);
        let states = m.get_scroll_states_for_dom(DOM);
        assert_eq!(states.get(&node(0)).unwrap().children_rect.size, size(100.0, 8000.0));
    }

    #[test]
    fn build_scroll_offset_map_only_emits_nodes_present_in_scroll_ids() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 25.0), at(1));
        m.register_or_update_scroll_node(
            DOM,
            node(4),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 500.0),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );
        m.register_or_update_scroll_node(
            DOM1,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 500.0),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );

        // Empty id map => empty offset map (and no panic).
        assert!(m.build_scroll_offset_map(DOM, &HashMap::new()).is_empty());

        let mut ids: HashMap<usize, u64> = HashMap::new();
        ids.insert(0, 100); // node index 0 -> scroll id 100
        ids.insert(7, 700); // an id for a node that has no scroll state
        let map = m.build_scroll_offset_map(DOM, &ids);
        assert_eq!(map.len(), 1, "node 4 has no scroll_id; DOM1 is a different dom");
        assert_eq!(map.get(&100), Some(&(0.0, 25.0)));
        assert!(map.get(&700).is_none());
    }

    // ====================================================== find_scroll_parent
    // (other: no_panic_smoke)

    #[test]
    fn find_scroll_parent_walks_up_to_the_nearest_registered_ancestor() {
        // hierarchy: 0 (root) <- 1 <- 2  (parent field is 1-based encoded)
        let hierarchy = [
            NodeHierarchyItem { parent: 0, previous_sibling: 0, next_sibling: 0, last_child: 2 },
            NodeHierarchyItem { parent: 1, previous_sibling: 0, next_sibling: 0, last_child: 3 },
            NodeHierarchyItem { parent: 2, previous_sibling: 0, next_sibling: 0, last_child: 0 },
        ];
        let m = mgr(size(100.0, 100.0), size(100.0, 500.0)); // node 0 registered
        assert_eq!(
            m.find_scroll_parent(DOM, node(2), &hierarchy),
            Some(node(0)),
            "must skip the unregistered node 1 and find the root scroll container"
        );
        // The node itself is excluded even though it IS registered.
        assert_eq!(m.find_scroll_parent(DOM, node(0), &hierarchy), None);
        // No scroll container anywhere in this DOM.
        assert_eq!(m.find_scroll_parent(DOM1, node(2), &hierarchy), None);
    }

    #[test]
    fn find_scroll_parent_handles_empty_and_out_of_range_hierarchies() {
        let m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        // Empty slice: the very first `get()` misses => None, no index panic.
        assert_eq!(m.find_scroll_parent(DOM, node(0), &[]), None);
        assert_eq!(m.find_scroll_parent(DOM, node(9999), &[]), None);
        // Node id past the end of the hierarchy: still no panic.
        let hierarchy = [NodeHierarchyItem::zeroed()];
        assert_eq!(m.find_scroll_parent(DOM, node(9999), &hierarchy), None);
    }

    // ============================================== calculate_scrollbar_states
    // (other: no_panic_smoke + no unbounded growth)

    #[test]
    fn calculate_scrollbar_states_is_idempotent_and_only_for_overflowing_axes() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.calculate_scrollbar_states();
        assert_eq!(m.debug_counts(), (1, 1), "only the vertical axis overflows");
        assert!(m
            .get_scrollbar_state(DOM, node(0), ScrollbarOrientation::Vertical)
            .is_some());
        assert!(m
            .get_scrollbar_state(DOM, node(0), ScrollbarOrientation::Horizontal)
            .is_none());

        // Re-running each frame must clear first — otherwise the map grows forever.
        for _ in 0..10 {
            m.calculate_scrollbar_states();
        }
        assert_eq!(m.debug_counts(), (1, 1), "per-frame recompute must not accumulate");
        assert_eq!(m.iter_scrollbar_states().count(), 1);
    }

    #[test]
    fn calculate_scrollbar_states_drops_bars_once_the_content_fits() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.calculate_scrollbar_states();
        assert_eq!(m.debug_counts().1, 1);
        // Relayout: content now fits => the scrollbar must disappear.
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 50.0),
            at(1),
            16.0,
            16.0,
            false,
            false,
        );
        m.calculate_scrollbar_states();
        assert_eq!(m.debug_counts().1, 0);
        assert!(m.hit_test_scrollbars(pos(90.0, 50.0)).is_none());
    }

    #[test]
    fn calculate_scrollbar_states_produces_finite_geometry_for_both_axes() {
        let mut m = ScrollManager::new();
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(1000.0, 1000.0),
            at(0),
            16.0,
            16.0,
            true,
            true,
        );
        m.calculate_scrollbar_states();
        assert_eq!(m.debug_counts(), (1, 2), "both axes overflow");
        for (_, sb) in m.iter_scrollbar_states() {
            assert!(sb.visible);
            assert!(sb.base_size.is_finite() && sb.base_size > 0.0);
            assert!(sb.scale.x.is_finite() && sb.scale.y.is_finite());
            assert!(sb.thumb_length.is_finite());
            assert!(sb.thumb_offset.is_finite());
            assert!(sb.usable_track_length.is_finite());
            assert!(sb.track_rect.size.width.is_finite());
            assert!(sb.track_rect.size.height.is_finite());
        }
    }

    #[test]
    fn calculate_scrollbar_states_zero_thickness_falls_back_to_the_default_width() {
        // An overlay scrollbar reports thickness 0 from layout; the geometry must
        // still divide by a non-zero width (otherwise `scale` becomes inf/NaN).
        let mut m = ScrollManager::new();
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 400.0),
            at(0),
            0.0, // scrollbar_thickness (overlay)
            0.0, // visual_width_px
            false,
            true,
        );
        m.calculate_scrollbar_states();
        let sb = m
            .get_scrollbar_state(DOM, node(0), ScrollbarOrientation::Vertical)
            .unwrap();
        assert_eq!(sb.base_size, crate::solver3::fc::DEFAULT_SCROLLBAR_WIDTH_PX);
        assert_eq!(sb.button_size, 0.0, "overlay scrollbars have no arrow buttons");
        assert!(sb.scale.x.is_finite() && sb.scale.y.is_finite(), "no div-by-zero");
    }

    #[test]
    fn calculate_scrollbar_state_from_geometry_survives_nan_input() {
        let mut s = state(size(f32::NAN, f32::NAN), size(f32::NAN, f32::NAN));
        s.scrollbar_thickness = f32::NAN;
        s.visual_width_px = f32::NAN;
        // `NaN > 0.0` is false for both width sources, so it falls back to the
        // default width instead of dividing by NaN.
        let sb = ScrollManager::calculate_scrollbar_state_from_geometry(
            &s,
            ScrollbarOrientation::Vertical,
        );
        assert!(sb.visible);
        assert_eq!(sb.base_size, crate::solver3::fc::DEFAULT_SCROLLBAR_WIDTH_PX);
        // `.max(0.0)` rescues every length: NaN geometry degrades to a zero-length
        // thumb on a zero-length track rather than propagating NaN.
        assert_eq!(sb.usable_track_length, 0.0);
        assert_eq!(sb.thumb_length, 0.0);
        assert_eq!(sb.thumb_offset, 0.0);
        assert_eq!(sb.thumb_position_ratio, 0.0);
        // The lengths are safe, but `scale` divides the (NaN) track height by the
        // thickness with no rescue — a NaN scale reaches the render transform.
        assert!(
            sb.scale.y.is_nan(),
            "NaN container height still leaks into ScrollbarState::scale"
        );
        // Hit-testing such a bar is still total: y < button_size wins first.
        assert_eq!(
            sb.hit_test_component(pos(0.0, 5.0)),
            ScrollbarComponent::TopButton
        );
    }

    // ================================== hit_test_scrollbar / hit_test_scrollbars
    // (numeric: zero / negative / nan_inf)

    #[test]
    fn hit_test_scrollbars_finds_the_vertical_bar_and_reports_local_coords() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.calculate_scrollbar_states();
        // Track is the right-hand 16px strip: origin.x = 100 - 16 = 84.
        let hit = m.hit_test_scrollbars(pos(90.0, 50.0)).expect("inside the track");
        assert_eq!(hit.dom_id, DOM);
        assert_eq!(hit.node_id, node(0));
        assert_eq!(hit.orientation, ScrollbarOrientation::Vertical);
        assert_eq!(hit.global_position, pos(90.0, 50.0));
        assert_eq!(hit.local_position, pos(6.0, 50.0), "local = global - track origin");

        // Just outside the track (content area) => no hit.
        assert!(m.hit_test_scrollbars(pos(10.0, 50.0)).is_none());
        // Same answer through the node-targeted entry point.
        let hit2 = m.hit_test_scrollbar(DOM, node(0), pos(90.0, 50.0)).unwrap();
        assert_eq!(hit2.local_position, hit.local_position);
        assert_eq!(hit2.component, hit.component);
        assert!(m.hit_test_scrollbar(DOM, node(1), pos(90.0, 50.0)).is_none());
    }

    #[test]
    fn hit_test_scrollbars_rejects_non_finite_and_out_of_range_positions() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.calculate_scrollbar_states();
        for p in [
            pos(f32::NAN, f32::NAN),
            pos(f32::INFINITY, f32::INFINITY),
            pos(f32::NEG_INFINITY, f32::NEG_INFINITY),
            pos(f32::MAX, f32::MAX),
            pos(f32::MIN, f32::MIN),
            pos(-1.0, -1.0),
            pos(0.0, 0.0),
        ] {
            assert!(
                m.hit_test_scrollbars(p).is_none(),
                "position {p:?} must not hit the 84..100 x 0..100 track"
            );
            assert!(m.hit_test_scrollbar(DOM, node(0), p).is_none());
        }
    }

    #[test]
    fn hit_test_scrollbars_before_calculate_returns_none() {
        // The states map is only filled by calculate_scrollbar_states(); querying
        // first must be a clean miss, not a stale hit.
        let m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        assert!(m.hit_test_scrollbars(pos(90.0, 50.0)).is_none());
        assert!(m.hit_test_scrollbar(DOM, node(0), pos(90.0, 50.0)).is_none());
    }

    #[test]
    fn hit_test_scrollbars_skips_invisible_bars() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.calculate_scrollbar_states();
        m.scrollbar_states
            .get_mut(&(DOM, node(0), ScrollbarOrientation::Vertical))
            .unwrap()
            .visible = false;
        assert!(m.hit_test_scrollbars(pos(90.0, 50.0)).is_none());
        assert!(m.hit_test_scrollbar(DOM, node(0), pos(90.0, 50.0)).is_none());
    }

    // ========================================================= input recording
    // (record_scroll_input / record_scroll_from_hit_test)

    #[test]
    fn record_scroll_input_reports_start_timer_only_on_the_first_pending_event() {
        let mut m = ScrollManager::new();
        assert!(m.record_scroll_input(input(0.0, 1.0, 1)), "queue was empty => start");
        assert!(!m.record_scroll_input(input(0.0, 1.0, 2)), "timer already running");
        let _ = m.get_input_queue().take_all();
        assert!(m.record_scroll_input(input(0.0, 1.0, 3)), "drained => start again");
    }

    #[test]
    fn record_scroll_input_applies_the_sign_to_extreme_deltas_without_overflow() {
        let mut m = ScrollManager::new();
        m.record_scroll_input(input(f32::MAX, f32::INFINITY, 1));
        m.record_scroll_input(input(f32::NAN, f32::MIN, 2));
        let q = m.get_input_queue().take_all();
        assert_eq!(q[0].delta.x, -f32::MAX, "sign flip must not overflow");
        assert_eq!(q[0].delta.y, f32::NEG_INFINITY);
        assert!(q[1].delta.x.is_nan(), "NaN * -1 stays NaN, no panic");
        assert_eq!(q[1].delta.y, f32::MAX);
    }

    #[test]
    fn record_scroll_from_hit_test_records_the_wheel_delta_even_with_no_hover() {
        // The wheel-as-zoom widgets (e.g. the map) depend on pending_wheel_event
        // being set unconditionally — before the hit-test lookup can bail out.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        let hover = HoverManager::new(); // no hit-test recorded at all
        let out = m.record_scroll_from_hit_test(
            3.0,
            -7.0,
            ScrollInputSource::WheelDiscrete,
            &hover,
            &InputPointId::Mouse,
            at(1),
        );
        assert!(out.is_none(), "no hover => no scroll target");
        assert_eq!(m.pending_wheel_event, Some(pos(3.0, -7.0)), "raw delta is recorded");
        assert!(!m.get_input_queue().has_pending(), "nothing queued for physics");
    }

    #[test]
    fn record_scroll_from_hit_test_queues_the_raw_delta_and_signals_the_timer() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        let hover = hover_over(&[0]);
        let (dom_id, node_id, start_timer) = m
            .record_scroll_from_hit_test(
                0.0,
                -10.0, // raw "wheel down" under the traditional sign
                ScrollInputSource::WheelDiscrete,
                &hover,
                &InputPointId::Mouse,
                at(1),
            )
            .expect("node 0 is scrollable and under the cursor");
        assert_eq!((dom_id, node_id), (DOM, node(0)));
        assert!(start_timer, "first queued input must start the physics timer");
        assert_eq!(m.pending_wheel_event, Some(pos(0.0, -10.0)));

        // A second event while the queue is still pending must NOT re-start it.
        let (_, _, start_again) = m
            .record_scroll_from_hit_test(
                0.0,
                -10.0,
                ScrollInputSource::WheelDiscrete,
                &hover,
                &InputPointId::Mouse,
                at(2),
            )
            .unwrap();
        assert!(!start_again);

        let q = m.get_input_queue().take_all();
        assert_eq!(q.len(), 2);
        // scroll_sign() is applied exactly once, in record_scroll_input.
        assert_eq!(q[0].delta.y, 10.0, "raw -10 * traditional sign (-1) = +10");
        assert_eq!(q[0].source, ScrollInputSource::WheelDiscrete);
        assert_eq!(q[0].timestamp, at(1));
    }

    #[test]
    fn record_scroll_from_hit_test_ignores_hovered_nodes_that_cannot_scroll() {
        let mut m = ScrollManager::new();
        // Registered, but the content fits => not scrollable.
        m.register_or_update_scroll_node(
            DOM,
            node(0),
            rect(0.0, 0.0, 100.0, 100.0),
            size(100.0, 100.0),
            at(0),
            16.0,
            16.0,
            false,
            false,
        );
        let hover = hover_over(&[0]);
        let out = m.record_scroll_from_hit_test(
            0.0,
            -10.0,
            ScrollInputSource::WheelDiscrete,
            &hover,
            &InputPointId::Mouse,
            at(1),
        );
        assert!(out.is_none(), "a non-overflowing node must not swallow the wheel");
        assert_eq!(m.pending_wheel_event, Some(pos(0.0, -10.0)));
        assert!(!m.get_input_queue().has_pending());
    }

    #[test]
    fn record_scroll_from_hit_test_with_non_finite_deltas_does_not_panic() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        let hover = hover_over(&[0]);
        // NaN: nothing can consume it, so the innermost scrollable is the fallback.
        let out = m.record_scroll_from_hit_test(
            f32::NAN,
            f32::NAN,
            ScrollInputSource::TrackpadContinuous,
            &hover,
            &InputPointId::Mouse,
            at(1),
        );
        assert_eq!(out.map(|(d, n, _)| (d, n)), Some((DOM, node(0))));
        assert!(m.pending_wheel_event.unwrap().x.is_nan());
        let q = m.get_input_queue().take_all();
        assert_eq!(q.len(), 1);
        assert!(q[0].delta.x.is_nan(), "NaN is queued verbatim, no panic");

        // Infinity: consumable (there is room), still queued safely.
        let out = m.record_scroll_from_hit_test(
            0.0,
            f32::NEG_INFINITY,
            ScrollInputSource::WheelDiscrete,
            &hover,
            &InputPointId::Mouse,
            at(2),
        );
        assert!(out.is_some());
        let q = m.get_input_queue().take_all();
        assert_eq!(q[0].delta.y, f32::INFINITY, "-inf * -1 = +inf");
    }

    #[test]
    fn record_scroll_from_hit_test_picks_the_innermost_scrollable_under_the_cursor() {
        // Both nodes are hovered; scroll_hit_test_nodes is walked in reverse key
        // order, so the higher (deeper) NodeId wins when it can consume the delta.
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.register_or_update_scroll_node(
            DOM,
            node(5),
            rect(0.0, 0.0, 50.0, 50.0),
            size(50.0, 200.0),
            at(0),
            16.0,
            16.0,
            false,
            true,
        );
        let hover = hover_over(&[0, 5]);
        let (_, node_id, _) = m
            .record_scroll_from_hit_test(
                0.0,
                -10.0,
                ScrollInputSource::WheelDiscrete,
                &hover,
                &InputPointId::Mouse,
                at(1),
            )
            .unwrap();
        assert_eq!(node_id, node(5), "innermost (deepest) scrollable wins");
    }

    // ============================================================ getters
    // (get_current_offset / get_last_activity_time / get_scroll_state)

    #[test]
    fn getters_agree_with_the_recorded_state() {
        let mut m = mgr(size(100.0, 100.0), size(100.0, 500.0));
        m.set_scroll_position(DOM, node(0), pos(0.0, 33.0), at(7));
        assert_eq!(m.get_current_offset(DOM, node(0)), Some(pos(0.0, 33.0)));
        assert_eq!(m.get_last_activity_time(DOM, node(0)), Some(at(7)));
        let s = m.get_scroll_state(DOM, node(0)).unwrap();
        assert_eq!(s.current_offset, pos(0.0, 33.0));
        assert!(s.animation.is_none());
        // Unknown keys are a clean miss on every getter.
        assert!(m.get_current_offset(DOM1, node(0)).is_none());
        assert!(m.get_last_activity_time(DOM, node(1)).is_none());
        assert!(m.get_scroll_state(DOM1, node(1)).is_none());
    }

    #[test]
    fn get_input_queue_hands_out_a_shared_handle() {
        let mut m = ScrollManager::new();
        let q = m.get_input_queue();
        assert!(!q.has_pending());
        m.record_scroll_input(input(0.0, 1.0, 1));
        assert!(q.has_pending(), "the handle must observe pushes made by the manager");
        assert_eq!(q.take_all().len(), 1);
        assert!(
            !m.get_input_queue().has_pending(),
            "draining the handle drains the manager's queue"
        );
    }
}
