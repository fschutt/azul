//! Comprehensive scroll state management for layout
//!
//! This module provides:
//! - Smooth scroll animations with easing
//! - Fading scrollbars (auto-hide after inactivity)
//! - IFrame scroll edge detection for lazy loading
//! - Scrollbar necessity calculation (with reflow detection)
//! - Integration with layout engine for conditional IFrame re-invocation

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::IFrameCallbackReason,
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    task::{Duration, Instant, SystemTick},
};

// ============================================================================
// Core Scroll Manager
// ============================================================================

/// Manages all scroll state and animations for a window
#[derive(Debug, Clone, Default)]
pub struct ScrollManager {
    /// Maps (DomId, NodeId) to their scroll state
    states: BTreeMap<(DomId, NodeId), ScrollState>,
    /// Track if we had any scroll activity this frame
    had_scroll_activity: bool,
    /// Track if we had any programmatic scroll this frame
    had_programmatic_scroll: bool,
    /// Track if any new DOMs were added this frame
    had_new_doms: bool,
}

/// The complete scroll state for a single node
#[derive(Debug, Clone)]
struct ScrollState {
    /// Current scroll offset (live, may be animating)
    current_offset: LogicalPosition,
    /// Ongoing smooth scroll animation, if any
    animation: Option<ScrollAnimation>,
    /// Last time scroll activity occurred (for fading scrollbars)
    last_activity: Instant,
    /// Bounds of the scrollable container
    container_rect: LogicalRect,
    /// Bounds of the total content (for calculating scroll limits)
    content_rect: LogicalRect,
    /// For IFrames: The actual rendered content size
    iframe_scroll_size: Option<LogicalSize>,
    /// For IFrames: The virtual content size (for scrollbar sizing)
    iframe_virtual_scroll_size: Option<LogicalSize>,
    /// Which edges triggered last IFrame re-invocation
    last_edge_triggered: EdgeFlags,
    /// Have we invoked for current bounds expansion?
    invoked_for_current_expansion: bool,
    /// Have we invoked for current edge approach?
    invoked_for_current_edge: bool,
    /// Has this IFrame been invoked at least once?
    iframe_was_invoked: bool,
}

/// Details of an in-progress smooth scroll animation
#[derive(Debug, Clone)]
struct ScrollAnimation {
    start_time: Instant,
    duration: Duration,
    start_offset: LogicalPosition,
    target_offset: LogicalPosition,
    easing: EasingFunction,
}

/// Easing functions for smooth scrolling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingFunction {
    Linear,
    EaseInOut,
    EaseOut,
}

/// Tracks which edges are near scroll boundaries
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeFlags {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// Result of a scroll tick, indicating what actions are needed
#[derive(Debug, Default)]
pub struct ScrollTickResult {
    /// If true, a repaint is needed (scroll offset changed)
    pub needs_repaint: bool,
    /// IFrames that need re-invocation with their reasons
    pub iframes_to_update: Vec<(DomId, NodeId, IFrameCallbackReason)>,
}

/// Source of a scroll event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollSource {
    /// User-initiated scroll (mouse wheel, trackpad, scrollbar drag)
    UserInput,
    /// Programmatic scroll (scroll_to, scroll_by API calls)
    Programmatic,
    /// System-initiated scroll (keyboard navigation, find-in-page)
    System,
}

/// Scroll event to be processed
#[derive(Debug, Clone)]
pub struct ScrollEvent {
    /// Which node to scroll
    pub dom_id: DomId,
    pub node_id: NodeId,
    /// Delta to scroll by (in logical pixels)
    pub delta: LogicalPosition,
    /// Source of the scroll event
    pub source: ScrollSource,
    /// For smooth scrolling: duration (None = instant)
    pub duration: Option<Duration>,
    /// For smooth scrolling: easing function
    pub easing: EasingFunction,
}

/// Information about what happened during a frame
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameScrollInfo {
    /// Did we have any scroll activity (user or programmatic)?
    pub had_scroll_activity: bool,
    /// Did we have programmatic scroll specifically?
    pub had_programmatic_scroll: bool,
    /// Were any new DOMs added this frame?
    pub had_new_doms: bool,
}

// ============================================================================
// ScrollManager Implementation
// ============================================================================

impl ScrollManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Called at the beginning of a frame to reset per-frame flags
    pub fn begin_frame(&mut self) {
        self.had_scroll_activity = false;
        self.had_programmatic_scroll = false;
        self.had_new_doms = false;
    }

    /// Called at the end of a frame to check what happened
    ///
    /// Returns information about what actions occurred this frame.
    pub fn end_frame(&self) -> FrameScrollInfo {
        FrameScrollInfo {
            had_scroll_activity: self.had_scroll_activity,
            had_programmatic_scroll: self.had_programmatic_scroll,
            had_new_doms: self.had_new_doms,
        }
    }

    /// Called once per frame to update animations and check IFrame conditions
    pub fn tick(&mut self, now: Instant) -> ScrollTickResult {
        let mut result = ScrollTickResult::default();

        for ((dom_id, node_id), state) in self.states.iter_mut() {
            // Update any ongoing animations
            if let Some(anim) = &state.animation {
                let elapsed = now.duration_since(&anim.start_time);
                let t = elapsed.div(&anim.duration);
                let t = t.min(1.0);

                // Apply easing
                let eased_t = apply_easing(t, anim.easing);

                // Interpolate position
                state.current_offset = LogicalPosition {
                    x: anim.start_offset.x + (anim.target_offset.x - anim.start_offset.x) * eased_t,
                    y: anim.start_offset.y + (anim.target_offset.y - anim.start_offset.y) * eased_t,
                };

                result.needs_repaint = true;

                // Animation complete?
                if t >= 1.0 {
                    state.animation = None;
                }
            }

            // Check IFrame edge conditions (if this is an IFrame)
            if let Some(scroll_size) = state.iframe_scroll_size {
                if let Some(reason) = state.check_iframe_reinvoke_condition(scroll_size) {
                    result.iframes_to_update.push((*dom_id, *node_id, reason));
                }
            }
        }

        result
    }

    /// Instantly sets scroll position, cancelling any animation
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
            .or_insert_with(|| ScrollState::new(now.clone()));

        state.current_offset = state.clamp(position);
        state.animation = None;
        state.last_activity = now;

        // Reset edge flags when explicitly setting position
        state.invoked_for_current_edge = false;
    }

    /// Initiates a smooth scroll to a target position
    pub fn scroll_to(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        target: LogicalPosition,
        duration: Duration,
        easing: EasingFunction,
        now: Instant,
    ) {
        let state = self
            .states
            .entry((dom_id, node_id))
            .or_insert_with(|| ScrollState::new(now.clone()));

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

    /// Smoothly scrolls by a delta
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

    /// Processes a scroll event (user input, programmatic, or system)
    ///
    /// This is the main entry point for handling scroll events from various sources.
    /// Returns true if the scroll was applied and a repaint is needed.
    pub fn process_scroll_event(&mut self, event: ScrollEvent, now: Instant) -> bool {
        // Track scroll activity
        self.had_scroll_activity = true;
        if event.source == ScrollSource::Programmatic || event.source == ScrollSource::System {
            self.had_programmatic_scroll = true;
        }

        match event.duration {
            Some(duration) if event.source != ScrollSource::UserInput => {
                // Smooth scroll for programmatic and system scrolls
                self.scroll_by(
                    event.dom_id,
                    event.node_id,
                    event.delta,
                    duration,
                    event.easing,
                    now,
                );
                true
            }
            _ => {
                // Instant scroll for user input and programmatic without duration
                let current = self
                    .get_current_offset(event.dom_id, event.node_id)
                    .unwrap_or_default();
                let new_position = LogicalPosition {
                    x: current.x + event.delta.x,
                    y: current.y + event.delta.y,
                };
                self.set_scroll_position(event.dom_id, event.node_id, new_position, now);
                true
            }
        }
    }

    /// Convenience: Process a mouse wheel event
    ///
    /// # Arguments
    /// - `scroll_amount`: Mouse wheel delta (typically -1.0 to 1.0 per detent)
    /// - `pixels_per_detent`: How many pixels to scroll per wheel detent (e.g. 120.0)
    /// - `horizontal`: If true, scroll horizontally; otherwise vertically
    pub fn process_wheel_event(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        scroll_amount: f32,
        pixels_per_detent: f32,
        horizontal: bool,
        now: Instant,
    ) -> bool {
        let delta = if horizontal {
            LogicalPosition {
                x: scroll_amount * pixels_per_detent,
                y: 0.0,
            }
        } else {
            LogicalPosition {
                x: 0.0,
                y: scroll_amount * pixels_per_detent,
            }
        };

        let event = ScrollEvent {
            dom_id,
            node_id,
            delta,
            source: ScrollSource::UserInput,
            duration: None, // User input is always instant
            easing: EasingFunction::Linear,
        };

        self.process_scroll_event(event, now)
    }

    /// Gets the current, live scroll offset
    pub fn get_current_offset(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalPosition> {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.current_offset)
    }

    /// Updates the bounds of a scrollable node after layout
    pub fn update_node_bounds(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        container_rect: LogicalRect,
        content_rect: LogicalRect,
        now: Instant,
    ) {
        let is_new = !self.states.contains_key(&(dom_id, node_id));

        let state = self
            .states
            .entry((dom_id, node_id))
            .or_insert_with(|| ScrollState::new(now.clone()));

        state.container_rect = container_rect;
        state.content_rect = content_rect;

        // Re-clamp offset in case bounds changed
        state.current_offset = state.clamp(state.current_offset);

        // Track if this is a new DOM
        if is_new {
            self.had_new_doms = true;
        }
    }

    /// Updates IFrame-specific scroll information
    pub fn update_iframe_scroll_info(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        scroll_size: LogicalSize,
        virtual_scroll_size: LogicalSize,
        now: Instant,
    ) {
        let is_new = !self.states.contains_key(&(dom_id, node_id));

        let state = self
            .states
            .entry((dom_id, node_id))
            .or_insert_with(|| ScrollState::new(now));

        // Check if scroll_size expanded - reset expansion flag if so
        if let Some(old_size) = state.iframe_scroll_size {
            if scroll_size.width > old_size.width || scroll_size.height > old_size.height {
                state.invoked_for_current_expansion = false;
            }
        }

        state.iframe_scroll_size = Some(scroll_size);
        state.iframe_virtual_scroll_size = Some(virtual_scroll_size);

        // Track if this is a new IFrame
        if is_new {
            self.had_new_doms = true;
        }
    }

    /// Calculates scrollbar opacity for fading effect
    ///
    /// The scrollbar remains fully visible during the `fade_delay` period after the last
    /// scroll activity. After that, it fades out over `fade_duration`.
    ///
    /// # Example
    /// - fade_delay = 500ms, fade_duration = 300ms
    /// - t=0ms: opacity = 1.0 (just scrolled)
    /// - t=400ms: opacity = 1.0 (still in delay)
    /// - t=500ms: opacity = 1.0 (delay just ended)
    /// - t=650ms: opacity = 0.5 (halfway through fade)
    /// - t=800ms: opacity = 0.0 (fully faded)
    pub fn get_scrollbar_opacity(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        now: Instant,
        fade_delay: Duration,
        fade_duration: Duration,
    ) -> f32 {
        if let Some(state) = self.states.get(&(dom_id, node_id)) {
            let time_since_activity = now.duration_since(&state.last_activity);

            // How far through the delay are we? (0.0 = just started, 1.0 = delay over)
            let delay_progress = time_since_activity.div(&fade_delay) as f32;

            if delay_progress < 1.0 {
                // Still in delay period - fully visible
                1.0
            } else {
                // Delay is over, now calculate fade progress
                // How much time has passed since delay ended?
                let time_in_fade = delay_progress - 1.0; // In units of fade_delay

                // Convert to units of fade_duration
                let fade_progress = time_in_fade * fade_delay.div(&fade_duration) as f32;

                // Clamp to [0.0, 1.0] and invert (1.0 = visible, 0.0 = invisible)
                (1.0 - fade_progress).max(0.0).min(1.0)
            }
        } else {
            1.0 // Default to visible
        }
    }

    /// Gets the scroll state for a specific DOM (for compatibility)
    pub fn get_scroll_states_for_dom(&self, dom_id: DomId) -> BTreeMap<NodeId, ScrollPosition> {
        self.states
            .iter()
            .filter_map(|((d, node_id), state)| {
                if *d == dom_id {
                    Some((
                        *node_id,
                        ScrollPosition {
                            parent_rect: state.container_rect,
                            children_rect: LogicalRect::new(
                                state.current_offset,
                                state.content_rect.size,
                            ),
                        },
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Legacy compatibility: Get scroll position
    pub fn get(&self, dom_id: DomId, node_id: NodeId) -> Option<ScrollPosition> {
        self.states
            .get(&(dom_id, node_id))
            .map(|state| ScrollPosition {
                parent_rect: state.container_rect,
                // Return content_rect as-is (which includes scroll offset as origin)
                children_rect: state.content_rect,
            })
    }

    /// Legacy compatibility: Set scroll position
    pub fn set(&mut self, dom_id: DomId, node_id: NodeId, position: ScrollPosition) {
        use azul_core::task::Instant;
        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(SystemTick { tick_counter: 0 });

        self.set_scroll_position(dom_id, node_id, position.children_rect.origin, now.clone());
        self.update_node_bounds(
            dom_id,
            node_id,
            position.parent_rect,
            position.children_rect,
            now,
        );
    }

    /// Clear all scroll states
    pub fn clear(&mut self) {
        self.states.clear();
    }

    /// Removes a node's scroll state
    pub fn remove(&mut self, dom_id: DomId, node_id: NodeId) -> bool {
        self.states.remove(&(dom_id, node_id)).is_some()
    }

    /// Legacy compatibility: Insert method
    pub fn insert(&mut self, key: (DomId, NodeId), value: ScrollPosition) {
        self.set(key.0, key.1, value);
    }

    /// Marks an IFrame callback as invoked, updating internal state
    pub fn mark_iframe_invoked(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        reason: IFrameCallbackReason,
    ) {
        if let Some(state) = self.states.get_mut(&(dom_id, node_id)) {
            // Always mark that this IFrame was invoked at least once
            state.iframe_was_invoked = true;

            match reason {
                IFrameCallbackReason::BoundsExpanded => {
                    state.invoked_for_current_expansion = true;
                }
                IFrameCallbackReason::EdgeScrolled(edge) => {
                    state.invoked_for_current_edge = true;
                    state.last_edge_triggered = edge.into();
                }
                _ => {}
            }
        }
    }

    /// Check if any scrollable node needs a repaint (animations running, etc.)
    pub fn needs_repaint(&self) -> bool {
        self.states.values().any(|state| state.animation.is_some())
    }

    /// Check if we should rebuild hit-test data (scroll positions changed)
    pub fn needs_hit_test_rebuild(&self) -> bool {
        self.had_scroll_activity
    }

    /// Check if we should re-invoke IFrame callbacks (new DOMs, scroll near edges)
    pub fn should_check_iframe_callbacks(&self) -> bool {
        self.had_new_doms || self.had_scroll_activity
    }

    /// Check if an IFrame was already invoked (for avoiding duplicate InitialRender)
    pub fn was_iframe_invoked(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.states
            .get(&(dom_id, node_id))
            .map(|state| state.iframe_was_invoked)
            .unwrap_or(false)
    }

    /// Get all scroll states (legacy compatibility)
    pub fn all(&self) -> BTreeMap<(DomId, NodeId), ScrollPosition> {
        self.states
            .iter()
            .map(|((dom_id, node_id), state)| {
                (
                    (*dom_id, *node_id),
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

    /// Merge scroll states from another source (legacy compatibility)
    pub fn merge(&mut self, other: BTreeMap<(DomId, NodeId), ScrollPosition>) {
        for ((dom_id, node_id), scroll_pos) in other {
            self.set(dom_id, node_id, scroll_pos);
        }
    }
}

// ============================================================================
// ScrollState Implementation
// ============================================================================

impl ScrollState {
    fn new(now: Instant) -> Self {
        Self {
            current_offset: LogicalPosition::zero(),
            animation: None,
            last_activity: now,
            container_rect: LogicalRect::zero(),
            content_rect: LogicalRect::zero(),
            iframe_scroll_size: None,
            iframe_virtual_scroll_size: None,
            last_edge_triggered: EdgeFlags::default(),
            invoked_for_current_expansion: false,
            invoked_for_current_edge: false,
            iframe_was_invoked: false,
        }
    }

    /// Clamps a position to valid scroll bounds
    fn clamp(&self, position: LogicalPosition) -> LogicalPosition {
        let max_x = (self.content_rect.size.width - self.container_rect.size.width).max(0.0);
        let max_y = (self.content_rect.size.height - self.container_rect.size.height).max(0.0);

        LogicalPosition {
            x: position.x.max(0.0).min(max_x),
            y: position.y.max(0.0).min(max_y),
        }
    }

    /// Checks if this IFrame needs re-invocation based on 5 conditional rules
    fn check_iframe_reinvoke_condition(
        &mut self,
        scroll_size: LogicalSize,
    ) -> Option<IFrameCallbackReason> {
        // Rule 1: Don't re-invoke if no scrolling possible
        let scrollable_width = scroll_size.width > self.container_rect.size.width;
        let scrollable_height = scroll_size.height > self.container_rect.size.height;

        if !scrollable_width && !scrollable_height {
            return None;
        }

        // Rule 2: Bounds expansion - check if scroll_size increased
        if !self.invoked_for_current_expansion {
            // This is set externally when scroll_size changes
            // (handled in update_iframe_scroll_info)
        }

        // Rule 3: Edge scroll detection (lazy loading)
        const EDGE_THRESHOLD: f32 = 200.0;

        let current_edges = EdgeFlags {
            top: scrollable_height && self.current_offset.y <= EDGE_THRESHOLD,
            bottom: scrollable_height && {
                let max_scroll = scroll_size.height - self.container_rect.size.height;
                (max_scroll - self.current_offset.y) <= EDGE_THRESHOLD
            },
            left: scrollable_width && self.current_offset.x <= EDGE_THRESHOLD,
            right: scrollable_width && {
                let max_scroll = scroll_size.width - self.container_rect.size.width;
                (max_scroll - self.current_offset.x) <= EDGE_THRESHOLD
            },
        };

        // Check if we crossed into a new edge zone
        if !self.invoked_for_current_edge && current_edges.any() {
            // Determine which edge(s) we're near
            if current_edges.bottom && !self.last_edge_triggered.bottom {
                return Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Bottom));
            }
            if current_edges.right && !self.last_edge_triggered.right {
                return Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Right));
            }
            if current_edges.top && !self.last_edge_triggered.top {
                return Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Top));
            }
            if current_edges.left && !self.last_edge_triggered.left {
                return Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Left));
            }
        }

        // Rule 4: Scroll beyond rendered content
        // This happens when scroll_size < virtual_scroll_size and we're near the limit
        if let Some(virtual_size) = self.iframe_virtual_scroll_size {
            if scroll_size.height < virtual_size.height || scroll_size.width < virtual_size.width {
                let near_rendered_limit_y = scrollable_height && {
                    let max_scroll = scroll_size.height - self.container_rect.size.height;
                    (max_scroll - self.current_offset.y) <= EDGE_THRESHOLD
                };
                let near_rendered_limit_x = scrollable_width && {
                    let max_scroll = scroll_size.width - self.container_rect.size.width;
                    (max_scroll - self.current_offset.x) <= EDGE_THRESHOLD
                };

                if near_rendered_limit_y || near_rendered_limit_x {
                    return Some(IFrameCallbackReason::ScrollBeyondContent);
                }
            }
        }

        None
    }
}

impl EdgeFlags {
    fn any(&self) -> bool {
        self.top || self.bottom || self.left || self.right
    }
}

// ============================================================================
// EdgeType <-> EdgeFlags Conversion
// ============================================================================

use azul_core::callbacks::EdgeType;

impl From<EdgeType> for EdgeFlags {
    fn from(edge: EdgeType) -> Self {
        match edge {
            EdgeType::Top => EdgeFlags {
                top: true,
                bottom: false,
                left: false,
                right: false,
            },
            EdgeType::Bottom => EdgeFlags {
                top: false,
                bottom: true,
                left: false,
                right: false,
            },
            EdgeType::Left => EdgeFlags {
                top: false,
                bottom: false,
                left: true,
                right: false,
            },
            EdgeType::Right => EdgeFlags {
                top: false,
                bottom: false,
                left: false,
                right: true,
            },
        }
    }
}

// ============================================================================
// Easing Functions
// ============================================================================

fn apply_easing(t: f32, easing: EasingFunction) -> f32 {
    match easing {
        EasingFunction::Linear => t,
        EasingFunction::EaseOut => 1.0 - (1.0 - t).powi(3), // Cubic ease out
        EasingFunction::EaseInOut => {
            // Cubic ease in-out
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
    }
}

// ============================================================================
// Scrollbar Info (for reflow loop integration)
// ============================================================================

/// Information about scrollbar necessity, calculated during layout
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarInfo {
    /// Does this node need a vertical scrollbar?
    pub needs_vertical: bool,
    /// Does this node need a horizontal scrollbar?
    pub needs_horizontal: bool,
    /// Width of the vertical scrollbar (if shown)
    pub vertical_width: f32,
    /// Height of the horizontal scrollbar (if shown)
    pub horizontal_height: f32,
}

impl ScrollbarInfo {
    pub fn none() -> Self {
        Self {
            needs_vertical: false,
            needs_horizontal: false,
            vertical_width: 0.0,
            horizontal_height: 0.0,
        }
    }

    /// How much should the container shrink due to scrollbars?
    pub fn shrink_size(&self) -> LogicalSize {
        LogicalSize {
            width: if self.needs_vertical {
                self.vertical_width
            } else {
                0.0
            },
            height: if self.needs_horizontal {
                self.horizontal_height
            } else {
                0.0
            },
        }
    }

    /// Does adding these scrollbars require a layout reflow?
    pub fn needs_reflow(&self, old: &ScrollbarInfo) -> bool {
        self.needs_vertical != old.needs_vertical || self.needs_horizontal != old.needs_horizontal
    }
}

// ============================================================================
// Legacy Compatibility Type Alias
// ============================================================================

/// Backward compatibility: Old name for ScrollManager
pub type ScrollStates = ScrollManager;
