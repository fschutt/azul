# Azul Scrollbar & Layout Constraint Analysis
# Generated: 2026-01-28T20:36:17.003282
# Purpose: Debug scrollbar visibility/geometry (Bugs 4-7) and line wrapping (Bug 2)

## BUGS BEING ANALYZED

### Bug 2: Line Wrapping When It Shouldn't
- ContentEditable wraps lines but shouldn't
- Should horizontally overflow and be horizontally scrollable
- Suspected cause: `UnifiedConstraints::default()` sets `available_width` to `Definite(0.0)`

### Bug 4: Scrollbar Visibility (auto mode)
- Scrollbar shows at 100% even though nothing to scroll
- `overflow: auto` should hide scrollbar when content fits
- Also affects size reservation (auto shouldn't reserve space if not scrollable)

### Bug 5: Scrollbar Track Width
- Track width calculated wrong
- Should be "100% - button sizes on each side"

### Bug 6: Scrollbar Position
- Scrollbar in wrong position/size
- Should use "inner" box (directly below text), not outer box

### Bug 7: Border Position
- Light grey 1px border around content is mispositioned
- Border and content with padding should NEVER be visually detached

---

## REQUESTED FILES FOR ANALYSIS


================================================================================
## FILE: layout/src/managers/scroll_state.rs
## Description: Scroll state management - visibility logic, overflow: auto handling
================================================================================
```rust
//! Pure scroll state management
//!
//! This module provides:
//! - Smooth scroll animations with easing
//! - Event source classification for scroll events
//! - Scrollbar geometry and hit-testing
//! - ExternalScrollId mapping for WebRender integration

use alloc::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId, ScrollbarOrientation},
    events::{
        EasingFunction, EventData, EventProvider, EventSource, EventType, ScrollDeltaMode,
        ScrollEventData, SyntheticEvent,
    },
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{ExternalScrollId, ScrollPosition},
    styled_dom::NodeHierarchyItemId,
    task::{Duration, Instant},
};

use crate::managers::hover::InputPointId;

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
}

impl ScrollbarState {
    /// Determine which component was hit at the given local position (relative to track_rect
    /// origin)
    pub fn hit_test_component(&self, local_pos: LogicalPosition) -> ScrollbarComponent {
        match self.orientation {
            ScrollbarOrientation::Vertical => {
                let button_height = self.base_size;

                // Top button
                if local_pos.y < button_height {
                    return ScrollbarComponent::TopButton;
                }

                // Bottom button
                let track_height = self.track_rect.size.height;
                if local_pos.y > track_height - button_height {
                    return ScrollbarComponent::BottomButton;
                }

                // Calculate thumb bounds
                let track_height_usable = track_height - 2.0 * button_height;
                let thumb_height = track_height_usable * self.thumb_size_ratio;
                let thumb_y_start = button_height
                    + (track_height_usable - thumb_height) * self.thumb_position_ratio;
                let thumb_y_end = thumb_y_start + thumb_height;

                // Check if inside thumb
                if local_pos.y >= thumb_y_start && local_pos.y <= thumb_y_end {
                    ScrollbarComponent::Thumb
                } else {
                    ScrollbarComponent::Track
                }
            }
            ScrollbarOrientation::Horizontal => {
                let button_width = self.base_size;

                // Left button
                if local_pos.x < button_width {
                    return ScrollbarComponent::TopButton;
                }

                // Right button
                let track_width = self.track_rect.size.width;
                if local_pos.x > track_width - button_width {
                    return ScrollbarComponent::BottomButton;
                }

                // Calculate thumb bounds
                let track_width_usable = track_width - 2.0 * button_width;
                let thumb_width = track_width_usable * self.thumb_size_ratio;
                let thumb_x_start =
                    button_width + (track_width_usable - thumb_width) * self.thumb_position_ratio;
                let thumb_x_end = thumb_x_start + thumb_width;

                // Check if inside thumb
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
    /// Track if we had any scroll activity this frame
    had_scroll_activity: bool,
    /// Track if we had any programmatic scroll this frame
    had_programmatic_scroll: bool,
    /// Track if any new DOMs were added this frame
    had_new_doms: bool,
}

/// The complete scroll state for a single node (with animation support)
#[derive(Debug, Clone)]
pub struct AnimatedScrollState {
    /// Current scroll offset (live, may be animating)
    pub current_offset: LogicalPosition,
    /// Previous frame's scroll offset (for delta calculation)
    pub previous_offset: LogicalPosition,
    /// Ongoing smooth scroll animation, if any
    pub animation: Option<ScrollAnimation>,
    /// Last time scroll activity occurred (for fading scrollbars)
    pub last_activity: Instant,
    /// Bounds of the scrollable container
    pub container_rect: LogicalRect,
    /// Bounds of the total content (for calculating scroll limits)
    pub content_rect: LogicalRect,
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

/// Summary of scroll-related events that occurred during a frame
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameScrollInfo {
    /// Whether any scroll input occurred this frame
    pub had_scroll_activity: bool,
    /// Whether programmatic scroll (scrollTo) occurred
    pub had_programmatic_scroll: bool,
    /// Whether new scrollable DOMs were added
    pub had_new_doms: bool,
}

/// Scroll event to be processed with source tracking
#[derive(Debug, Clone)]
pub struct ScrollEvent {
    /// DOM containing the scrollable node
    pub dom_id: DomId,
    /// Target scroll node
    pub node_id: NodeId,
    /// Scroll delta (positive = scroll down/right)
    pub delta: LogicalPosition,
    /// Event source (User, Programmatic, etc.)
    pub source: EventSource,
    /// Animation duration (None = instant)
    pub duration: Option<Duration>,
    /// Easing function for smooth scrolling
    pub easing: EasingFunction,
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

    /// Prepares state for a new frame by saving current offsets as previous
    pub fn begin_frame(&mut self) {
        self.had_scroll_activity = false;
        self.had_programmatic_scroll = false;
        self.had_new_doms = false;

        // Save current offsets as previous for delta calculation
        for state in self.states.values_mut() {
            state.previous_offset = state.current_offset;
        }
    }

    /// Returns scroll activity summary for the completed frame
    pub fn end_frame(&self) -> FrameScrollInfo {
        FrameScrollInfo {
            had_scroll_activity: self.had_scroll_activity,
            had_programmatic_scroll: self.had_programmatic_scroll,
            had_new_doms: self.had_new_doms,
        }
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

    /// Processes a scroll event, applying immediate or animated scroll
    pub fn process_scroll_event(&mut self, event: ScrollEvent, now: Instant) -> bool {
        self.had_scroll_activity = true;
        if event.source == EventSource::Programmatic || event.source == EventSource::User {
            self.had_programmatic_scroll = true;
        }

        if let Some(duration) = event.duration {
            self.scroll_by(
                event.dom_id,
                event.node_id,
                event.delta,
                duration,
                event.easing,
                now,
            );
        } else {
            let current = self
                .get_current_offset(event.dom_id, event.node_id)
                .unwrap_or_default();
            let new_position = LogicalPosition {
                x: current.x + event.delta.x,
                y: current.y + event.delta.y,
            };
            self.set_scroll_position(event.dom_id, event.node_id, new_position, now);
        }
        true
    }

    /// Records a scroll input sample and applies it to the first scrollable node under the cursor
    ///
    /// Finds the first scrollable node in the hit test hierarchy and applies
    /// the scroll delta. Returns the scrolled node if successful.
    pub fn record_sample(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        hover_manager: &crate::managers::hover::HoverManager,
        input_point_id: &InputPointId,
        now: Instant,
    ) -> Option<(DomId, NodeId)> {
        let hit_test = hover_manager.get_current(input_point_id)?;
        
        // Find first scrollable node in hit test hierarchy
        for (dom_id, hit_node) in &hit_test.hovered_nodes {
            for (node_id, _scroll_item) in &hit_node.scroll_hit_test_nodes {
                let is_scrollable = self.is_node_scrollable(*dom_id, *node_id);
                if is_scrollable {
                    let delta = LogicalPosition {
                        x: delta_x,
                        y: delta_y,
                    };

                    let current = self
                        .get_current_offset(*dom_id, *node_id)
                        .unwrap_or_default();
                    let new_position = LogicalPosition {
                        x: current.x + delta.x,
                        y: current.y + delta.y,
                    };

                    self.set_scroll_position(*dom_id, *node_id, new_position, now);
                    self.had_scroll_activity = true;

                    return Some((*dom_id, *node_id));
                }
            }
        }

        None
    }

    /// Check if a node is scrollable (has overflow:scroll/auto and overflowing content)
    fn is_node_scrollable(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.states.get(&(dom_id, node_id)).map_or(false, |state| {
            let has_horizontal = state.content_rect.size.width > state.container_rect.size.width;
            let has_vertical = state.content_rect.size.height > state.container_rect.size.height;
            has_horizontal || has_vertical
        })
    }

    /// Returns the scroll delta applied this frame, if non-zero
    pub fn get_scroll_delta(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalPosition> {
        let state = self.states.get(&(dom_id, node_id))?;
        let delta = LogicalPosition {
            x: state.current_offset.x - state.previous_offset.x,
            y: state.current_offset.y - state.previous_offset.y,
        };
        (delta.x.abs() > 0.001 || delta.y.abs() > 0.001).then_some(delta)
    }

    /// Returns true if the node had scroll activity this frame
    pub fn had_scroll_activity_for_node(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.get_scroll_delta(dom_id, node_id).is_some()
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
        if !self.states.contains_key(&(dom_id, node_id)) {
            self.had_new_doms = true;
        }
        let state = self
            .states
            .entry((dom_id, node_id))
            .or_insert_with(|| AnimatedScrollState::new(now));
        state.container_rect = container_rect;
        state.content_rect = content_rect;
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
            // Re-clamp current offset to new bounds
            existing.current_offset = existing.clamp(existing.current_offset);
        } else {
            // New scrollable node
            self.states.insert(
                key,
                AnimatedScrollState {
                    current_offset: LogicalPosition::zero(),
                    previous_offset: LogicalPosition::zero(),
                    animation: None,
                    last_activity: now,
                    container_rect,
                    content_rect,
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
    pub fn calculate_scrollbar_states(&mut self) {
        self.scrollbar_states.clear();

        // Collect vertical scrollbar states
        let vertical_states: Vec<_> = self
            .states
            .iter()
            .filter(|(_, s)| s.content_rect.size.height > s.container_rect.size.height)
            .map(|((dom_id, node_id), scroll_state)| {
                let v_state = Self::calculate_vertical_scrollbar_static(scroll_state);
                ((*dom_id, *node_id, ScrollbarOrientation::Vertical), v_state)
            })
            .collect();

        // Collect horizontal scrollbar states
        let horizontal_states: Vec<_> = self
            .states
            .iter()
            .filter(|(_, s)| s.content_rect.size.width > s.container_rect.size.width)
            .map(|((dom_id, node_id), scroll_state)| {
                let h_state = Self::calculate_horizontal_scrollbar_static(scroll_state);
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

    /// Calculate vertical scrollbar geometry
    fn calculate_vertical_scrollbar_static(scroll_state: &AnimatedScrollState) -> ScrollbarState {
        const SCROLLBAR_WIDTH: f32 = 12.0; // Base size (1:1 square)

        let container_height = scroll_state.container_rect.size.height;
        let content_height = scroll_state.content_rect.size.height;

        // Thumb size ratio = visible_height / total_height
        let thumb_size_ratio = (container_height / content_height).min(1.0);

        // Thumb position ratio = scroll_offset / max_scroll
        let max_scroll = (content_height - container_height).max(0.0);
        let thumb_position_ratio = if max_scroll > 0.0 {
            (scroll_state.current_offset.y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Scale: width = 1.0 (SCROLLBAR_WIDTH), height = container_height / SCROLLBAR_WIDTH
        let scale = LogicalPosition::new(1.0, container_height / SCROLLBAR_WIDTH);

        // Track rect (positioned at right edge of container)
        let track_x = scroll_state.container_rect.origin.x + scroll_state.container_rect.size.width
            - SCROLLBAR_WIDTH;
        let track_y = scroll_state.container_rect.origin.y;
        let track_rect = LogicalRect::new(
            LogicalPosition::new(track_x, track_y),
            LogicalSize::new(SCROLLBAR_WIDTH, container_height),
        );

        ScrollbarState {
            visible: true,
            orientation: ScrollbarOrientation::Vertical,
            base_size: SCROLLBAR_WIDTH,
            scale,
            thumb_position_ratio,
            thumb_size_ratio,
            track_rect,
        }
    }

    /// Calculate horizontal scrollbar geometry
    fn calculate_horizontal_scrollbar_static(scroll_state: &AnimatedScrollState) -> ScrollbarState {
        const SCROLLBAR_HEIGHT: f32 = 12.0; // Base size (1:1 square)

        let container_width = scroll_state.container_rect.size.width;
        let content_width = scroll_state.content_rect.size.width;

        let thumb_size_ratio = (container_width / content_width).min(1.0);

        let max_scroll = (content_width - container_width).max(0.0);
        let thumb_position_ratio = if max_scroll > 0.0 {
            (scroll_state.current_offset.x / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let scale = LogicalPosition::new(container_width / SCROLLBAR_HEIGHT, 1.0);

        let track_x = scroll_state.container_rect.origin.x;
        let track_y = scroll_state.container_rect.origin.y
            + scroll_state.container_rect.size.height
            - SCROLLBAR_HEIGHT;
        let track_rect = LogicalRect::new(
            LogicalPosition::new(track_x, track_y),
            LogicalSize::new(container_width, SCROLLBAR_HEIGHT),
        );

        ScrollbarState {
            visible: true,
            orientation: ScrollbarOrientation::Horizontal,
            base_size: SCROLLBAR_HEIGHT,
            scale,
            thumb_position_ratio,
            thumb_size_ratio,
            track_rect,
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
            previous_offset: LogicalPosition::zero(),
            animation: None,
            last_activity: now,
            container_rect: LogicalRect::zero(),
            content_rect: LogicalRect::zero(),
        }
    }

    /// Clamp a scroll position to valid bounds (0 to max_scroll).
    pub fn clamp(&self, position: LogicalPosition) -> LogicalPosition {
        let max_x = (self.content_rect.size.width - self.container_rect.size.width).max(0.0);
        let max_y = (self.content_rect.size.height - self.container_rect.size.height).max(0.0);
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

// EventProvider Implementation

impl EventProvider for ScrollManager {
    /// Get pending scroll events.
    ///
    /// Returns Scroll/ScrollStart/ScrollEnd events for nodes whose scroll
    /// position changed this frame.
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();

        // Generate events for all nodes that scrolled this frame
        for ((dom_id, node_id), state) in &self.states {
            // Check if scroll offset changed (delta != 0)
            let delta = LogicalPosition {
                x: state.current_offset.x - state.previous_offset.x,
                y: state.current_offset.y - state.previous_offset.y,
            };

            if delta.x.abs() > 0.001 || delta.y.abs() > 0.001 {
                let target = azul_core::dom::DomNodeId {
                    dom: *dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                };

                // Determine event source
                let event_source = if self.had_programmatic_scroll {
                    EventSource::Programmatic
                } else {
                    EventSource::User
                };

                // Generate Scroll event
                events.push(SyntheticEvent::new(
                    EventType::Scroll,
                    event_source,
                    target,
                    timestamp.clone(),
                    EventData::Scroll(ScrollEventData {
                        delta,
                        delta_mode: ScrollDeltaMode::Pixel,
                    }),
                ));

                // TODO: Generate ScrollStart/ScrollEnd events
                // Need to track when scroll starts/stops (first/last frame with delta)
            }
        }

        events
    }
}

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
        // Remap states
        let states_to_update: Vec<_> = self.states.keys()
            .filter(|(d, _)| *d == dom_id)
            .cloned()
            .collect();
        
        for (d, old_node_id) in states_to_update {
            if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                if let Some(state) = self.states.remove(&(d, old_node_id)) {
                    self.states.insert((d, new_node_id), state);
                }
            } else {
                // Node was removed, remove its scroll state
                self.states.remove(&(d, old_node_id));
            }
        }
        
        // Remap external_scroll_ids
        let scroll_ids_to_update: Vec<_> = self.external_scroll_ids.keys()
            .filter(|(d, _)| *d == dom_id)
            .cloned()
            .collect();
        
        for (d, old_node_id) in scroll_ids_to_update {
            if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                if let Some(scroll_id) = self.external_scroll_ids.remove(&(d, old_node_id)) {
                    self.external_scroll_ids.insert((d, new_node_id), scroll_id);
                }
            } else {
                self.external_scroll_ids.remove(&(d, old_node_id));
            }
        }
        
        // Remap scrollbar_states
        let scrollbar_states_to_update: Vec<_> = self.scrollbar_states.keys()
            .filter(|(d, _, _)| *d == dom_id)
            .cloned()
            .collect();
        
        for (d, old_node_id, orientation) in scrollbar_states_to_update {
            if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                if let Some(state) = self.scrollbar_states.remove(&(d, old_node_id, orientation)) {
                    self.scrollbar_states.insert((d, new_node_id, orientation), state);
                }
            } else {
                self.scrollbar_states.remove(&(d, old_node_id, orientation));
            }
        }
    }
}

```

================================================================================
## FILE: layout/src/solver3/scrollbar.rs
## Description: Scrollbar geometry calculations - track and thumb sizing
================================================================================
```rust
use azul_core::geom::LogicalSize;

/// Information about scrollbar requirements and dimensions
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct ScrollbarRequirements {
    pub needs_horizontal: bool,
    pub needs_vertical: bool,
    pub scrollbar_width: f32,
    pub scrollbar_height: f32,
}

impl ScrollbarRequirements {
    /// Checks if the presence of scrollbars reduces the available inner size,
    /// which would necessitate a reflow of the content.
    pub fn needs_reflow(&self) -> bool {
        self.scrollbar_width > 0.0 || self.scrollbar_height > 0.0
    }

    /// Takes a size (representing a content-box) and returns a new size
    /// reduced by the dimensions of any active scrollbars.
    pub fn shrink_size(&self, size: LogicalSize) -> LogicalSize {
        LogicalSize {
            width: (size.width - self.scrollbar_width).max(0.0),
            height: (size.height - self.scrollbar_height).max(0.0),
        }
    }
}

```

================================================================================
## FILE: layout/src/solver3/display_list.rs
## Description: Display list generation - scrollbar rendering, z-ordering, clipping
================================================================================
```rust
//! Generates a renderer-agnostic display list from a laid-out tree

use std::{collections::BTreeMap, sync::Arc};

use allsorts::glyph_position;
use azul_core::{
    dom::{DomId, FormattingContext, NodeId, NodeType, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gpu::GpuValueCache,
    hit_test::ScrollPosition,
    hit_test_tag::{CursorType, TAG_TYPE_CURSOR},
    resources::{
        IdNamespace, ImageRef, OpacityKey, RendererResources,
    },
    selection::{Selection, SelectionRange, SelectionState, TextSelection},
    styled_dom::StyledDom,
    ui_solver::GlyphInstance,
};
use azul_css::{
    css::CssPropertyValue,
    format_rust_code::GetHash,
    props::{
        basic::{ColorU, FontRef, PixelValue},
        layout::{LayoutDisplay, LayoutOverflow, LayoutPosition},
        property::{CssProperty, CssPropertyType},
        style::{
            background::{ConicGradient, ExtendMode, LinearGradient, RadialGradient},
            border_radius::StyleBorderRadius,
            box_shadow::{BoxShadowClipMode, StyleBoxShadow},
            filter::{StyleFilter, StyleFilterVec},
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBorderBottomColor, StyleBorderBottomStyle,
            StyleBorderLeftColor, StyleBorderLeftStyle, StyleBorderRightColor,
            StyleBorderRightStyle, StyleBorderTopColor, StyleBorderTopStyle,
        },
    },
    LayoutDebugMessage,
};

#[cfg(feature = "text_layout")]
use crate::text3;
#[cfg(feature = "text_layout")]
use crate::text3::cache::{InlineShape, PositionedItem};
use crate::{
    debug_info,
    font_traits::{
        FontHash, FontLoaderTrait, ImageSource, InlineContent, ParsedFontTrait, ShapedItem,
        UnifiedLayout,
    },
    solver3::{
        getters::{
            get_background_color, get_background_contents, get_border_info, get_border_radius,
            get_caret_style, get_overflow_x, get_overflow_y, get_scrollbar_info_from_layout,
            get_scrollbar_style, get_selection_style, get_style_border_radius, get_z_index,
            BorderInfo, CaretStyle, ComputedScrollbarStyle, SelectionStyle,
        },
        layout_tree::{LayoutNode, LayoutTree},
        positioning::get_position_type,
        scrollbar::ScrollbarRequirements,
        LayoutContext, LayoutError, Result,
    },
};

/// Border widths for all four sides.
///
/// Each field is optional to allow partial border specifications.
/// Used in [`DisplayListItem::Border`] to specify per-side border widths.
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderWidths {
    /// Top border width (CSS `border-top-width`)
    pub top: Option<CssPropertyValue<LayoutBorderTopWidth>>,
    /// Right border width (CSS `border-right-width`)
    pub right: Option<CssPropertyValue<LayoutBorderRightWidth>>,
    /// Bottom border width (CSS `border-bottom-width`)
    pub bottom: Option<CssPropertyValue<LayoutBorderBottomWidth>>,
    /// Left border width (CSS `border-left-width`)
    pub left: Option<CssPropertyValue<LayoutBorderLeftWidth>>,
}

/// Border colors for all four sides.
///
/// Each field is optional to allow partial border specifications.
/// Used in [`DisplayListItem::Border`] to specify per-side border colors.
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderColors {
    /// Top border color (CSS `border-top-color`)
    pub top: Option<CssPropertyValue<StyleBorderTopColor>>,
    /// Right border color (CSS `border-right-color`)
    pub right: Option<CssPropertyValue<StyleBorderRightColor>>,
    /// Bottom border color (CSS `border-bottom-color`)
    pub bottom: Option<CssPropertyValue<StyleBorderBottomColor>>,
    /// Left border color (CSS `border-left-color`)
    pub left: Option<CssPropertyValue<StyleBorderLeftColor>>,
}

/// Border styles for all four sides.
///
/// Each field is optional to allow partial border specifications.
/// Used in [`DisplayListItem::Border`] to specify per-side border styles
/// (solid, dashed, dotted, none, etc.).
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderStyles {
    /// Top border style (CSS `border-top-style`)
    pub top: Option<CssPropertyValue<StyleBorderTopStyle>>,
    /// Right border style (CSS `border-right-style`)
    pub right: Option<CssPropertyValue<StyleBorderRightStyle>>,
    /// Bottom border style (CSS `border-bottom-style`)
    pub bottom: Option<CssPropertyValue<StyleBorderBottomStyle>>,
    /// Left border style (CSS `border-left-style`)
    pub left: Option<CssPropertyValue<StyleBorderLeftStyle>>,
}

/// A rectangle in border-box coordinates (includes padding and border).
/// This is what layout calculates and stores in `used_size` and absolute positions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderBoxRect(pub LogicalRect);

/// Simple struct for passing element dimensions to border-radius calculation
#[derive(Debug, Clone, Copy)]
pub struct PhysicalSizeImport {
    pub width: f32,
    pub height: f32,
}

/// Complete drawing information for a scrollbar with all visual components.
///
/// This contains the resolved geometry and colors for all scrollbar parts:
/// - Track: The background area where the thumb slides
/// - Thumb: The draggable indicator showing current scroll position
/// - Buttons: Optional up/down or left/right arrow buttons
/// - Corner: The area where horizontal and vertical scrollbars meet
#[derive(Debug, Clone)]
pub struct ScrollbarDrawInfo {
    /// Overall bounds of the entire scrollbar (including track and buttons)
    pub bounds: LogicalRect,
    /// Scrollbar orientation (horizontal or vertical)
    pub orientation: ScrollbarOrientation,

    // Track area (the background rail)
    /// Bounds of the track area
    pub track_bounds: LogicalRect,
    /// Color of the track background
    pub track_color: ColorU,

    // Thumb (the draggable part)
    /// Bounds of the thumb
    pub thumb_bounds: LogicalRect,
    /// Color of the thumb
    pub thumb_color: ColorU,
    /// Border radius for rounded thumb corners
    pub thumb_border_radius: BorderRadius,

    // Optional buttons (arrows at ends)
    /// Optional decrement button bounds (up/left arrow)
    pub button_decrement_bounds: Option<LogicalRect>,
    /// Optional increment button bounds (down/right arrow)
    pub button_increment_bounds: Option<LogicalRect>,
    /// Color for buttons
    pub button_color: ColorU,

    /// Optional opacity key for GPU-side fading animation.
    pub opacity_key: Option<OpacityKey>,
    /// Optional hit-test ID for WebRender hit-testing.
    pub hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    /// Whether to clip scrollbar to container's border-radius
    pub clip_to_container_border: bool,
    /// Container's border-radius (for clipping)
    pub container_border_radius: BorderRadius,
}

impl BorderBoxRect {
    /// Convert border-box to content-box by subtracting padding and border.
    /// Content-box is where inline layout and text actually render.
    pub fn to_content_box(
        self,
        padding: &crate::solver3::geometry::EdgeSizes,
        border: &crate::solver3::geometry::EdgeSizes,
    ) -> ContentBoxRect {
        ContentBoxRect(LogicalRect {
            origin: LogicalPosition {
                x: self.0.origin.x + padding.left + border.left,
                y: self.0.origin.y + padding.top + border.top,
            },
            size: LogicalSize {
                width: self.0.size.width
                    - padding.left
                    - padding.right
                    - border.left
                    - border.right,
                height: self.0.size.height
                    - padding.top
                    - padding.bottom
                    - border.top
                    - border.bottom,
            },
        })
    }

    /// Get the inner LogicalRect
    pub fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// A rectangle in content-box coordinates (excludes padding and border).
/// This is where text and inline content is positioned by the inline formatter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentBoxRect(pub LogicalRect);

impl ContentBoxRect {
    /// Get the inner LogicalRect
    pub fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// The final, renderer-agnostic output of the layout engine.
///
/// This is a flat list of drawing and state-management commands, already sorted
/// according to the CSS paint order. A renderer can consume this list directly.
#[derive(Debug, Default)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
    /// Optional mapping from item index to the DOM NodeId that generated it.
    /// Used for pagination to look up CSS break properties.
    /// Not all items have a source node (e.g., synthesized decorations).
    pub node_mapping: Vec<Option<NodeId>>,
}

impl DisplayList {
    /// Generates a JSON representation of the display list for debugging.
    /// This includes clip chain analysis showing how clips are stacked.
    pub fn to_debug_json(&self) -> String {
        use std::fmt::Write;
        let mut json = String::new();
        writeln!(json, "{{").unwrap();
        writeln!(json, "  \"total_items\": {},", self.items.len()).unwrap();
        writeln!(json, "  \"items\": [").unwrap();

        let mut clip_depth = 0i32;
        let mut scroll_depth = 0i32;
        let mut stacking_depth = 0i32;

        for (i, item) in self.items.iter().enumerate() {
            let comma = if i < self.items.len() - 1 { "," } else { "" };
            let node_id = self.node_mapping.get(i).and_then(|n| *n);

            match item {
                DisplayListItem::PushClip {
                    bounds,
                    border_radius,
                } => {
                    clip_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PushClip\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},", 
                        bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height).unwrap();
                    writeln!(json, "      \"border_radius\": {{ \"tl\": {:.1}, \"tr\": {:.1}, \"bl\": {:.1}, \"br\": {:.1} }},",
                        border_radius.top_left, border_radius.top_right,
                        border_radius.bottom_left, border_radius.bottom_right).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::PopClip => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PopClip\",").unwrap();
                    writeln!(json, "      \"clip_depth_before\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"clip_depth_after\": {}", clip_depth - 1).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                    clip_depth -= 1;
                }
                DisplayListItem::PushScrollFrame {
                    clip_bounds,
                    content_size,
                    scroll_id,
                } => {
                    scroll_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PushScrollFrame\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"clip_bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        clip_bounds.origin.x, clip_bounds.origin.y,
                        clip_bounds.size.width, clip_bounds.size.height).unwrap();
                    writeln!(
                        json,
                        "      \"content_size\": {{ \"w\": {:.1}, \"h\": {:.1} }},",
                        content_size.width, content_size.height
                    )
                    .unwrap();
                    writeln!(json, "      \"scroll_id\": {},", scroll_id).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::PopScrollFrame => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PopScrollFrame\",").unwrap();
                    writeln!(json, "      \"scroll_depth_before\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"scroll_depth_after\": {}", scroll_depth - 1).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                    scroll_depth -= 1;
                }
                DisplayListItem::PushStackingContext { z_index, bounds } => {
                    stacking_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PushStackingContext\",").unwrap();
                    writeln!(json, "      \"stacking_depth\": {},", stacking_depth).unwrap();
                    writeln!(json, "      \"z_index\": {},", z_index).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }}",
                        bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::PopStackingContext => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PopStackingContext\",").unwrap();
                    writeln!(json, "      \"stacking_depth_before\": {},", stacking_depth).unwrap();
                    writeln!(
                        json,
                        "      \"stacking_depth_after\": {}",
                        stacking_depth - 1
                    )
                    .unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                    stacking_depth -= 1;
                }
                DisplayListItem::Rect {
                    bounds,
                    color,
                    border_radius,
                } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"Rect\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height).unwrap();
                    writeln!(
                        json,
                        "      \"color\": \"rgba({},{},{},{})\",",
                        color.r, color.g, color.b, color.a
                    )
                    .unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::Border { bounds, .. } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"Border\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::ScrollBarStyled { info } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"ScrollBarStyled\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"orientation\": \"{:?}\",", info.orientation).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }}",
                        info.bounds.origin.x, info.bounds.origin.y,
                        info.bounds.size.width, info.bounds.size.height).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                _ => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(
                        json,
                        "      \"type\": \"{:?}\",",
                        std::mem::discriminant(item)
                    )
                    .unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
            }
        }

        writeln!(json, "  ],").unwrap();
        writeln!(json, "  \"final_clip_depth\": {},", clip_depth).unwrap();
        writeln!(json, "  \"final_scroll_depth\": {},", scroll_depth).unwrap();
        writeln!(json, "  \"final_stacking_depth\": {},", stacking_depth).unwrap();
        writeln!(
            json,
            "  \"balanced\": {}",
            clip_depth == 0 && scroll_depth == 0 && stacking_depth == 0
        )
        .unwrap();
        writeln!(json, "}}").unwrap();

        json
    }
}

/// A command in the display list. Can be either a drawing primitive or a
/// state-management instruction for the renderer's graphics context.
#[derive(Debug, Clone)]
pub enum DisplayListItem {
    // Drawing Primitives
    /// A filled rectangle with optional rounded corners.
    /// Used for backgrounds, colored boxes, and other solid fills.
    Rect {
        /// The rectangle bounds in logical coordinates
        bounds: LogicalRect,
        /// The fill color (RGBA)
        color: ColorU,
        /// Corner radii for rounded rectangles
        border_radius: BorderRadius,
    },
    /// A selection highlight rectangle (e.g., for text selection).
    /// Rendered behind text to show selected regions.
    SelectionRect {
        /// The rectangle bounds in logical coordinates
        bounds: LogicalRect,
        /// Corner radii for rounded selection
        border_radius: BorderRadius,
        /// The selection highlight color (typically semi-transparent)
        color: ColorU,
    },
    /// A text cursor (caret) rectangle.
    /// Typically a thin vertical line indicating text insertion point.
    CursorRect {
        /// The cursor bounds (usually narrow width)
        bounds: LogicalRect,
        /// The cursor color
        color: ColorU,
    },
    /// A CSS border with per-side widths, colors, and styles.
    /// Supports different styles per side (solid, dashed, dotted, etc.).
    Border {
        /// The border-box bounds
        bounds: LogicalRect,
        /// Border widths for each side
        widths: StyleBorderWidths,
        /// Border colors for each side
        colors: StyleBorderColors,
        /// Border styles for each side (solid, dashed, etc.)
        styles: StyleBorderStyles,
        /// Corner radii for rounded borders
        border_radius: StyleBorderRadius,
    },
    /// Text layout with full metadata (for PDF, accessibility, etc.)
    /// This is pushed BEFORE the individual Text items and contains
    /// the original text, glyph-to-unicode mapping, and positioning info
    TextLayout {
        layout: Arc<dyn std::any::Any + Send + Sync>, // Type-erased UnifiedLayout
        bounds: LogicalRect,
        font_hash: FontHash,
        font_size_px: f32,
        color: ColorU,
    },
    /// Text rendered with individual glyph positioning (for simple renderers)
    Text {
        glyphs: Vec<GlyphInstance>,
        font_hash: FontHash, // Changed from FontRef - just store the hash
        font_size_px: f32,
        color: ColorU,
        clip_rect: LogicalRect,
    },
    /// Underline decoration for text (CSS text-decoration: underline)
    Underline {
        bounds: LogicalRect,
        color: ColorU,
        thickness: f32,
    },
    /// Strikethrough decoration for text (CSS text-decoration: line-through)
    Strikethrough {
        bounds: LogicalRect,
        color: ColorU,
        thickness: f32,
    },
    /// Overline decoration for text (CSS text-decoration: overline)
    Overline {
        bounds: LogicalRect,
        color: ColorU,
        thickness: f32,
    },
    Image {
        bounds: LogicalRect,
        image: ImageRef,
    },
    /// A dedicated primitive for a scrollbar with optional GPU-animated opacity.
    /// This is a simple single-color scrollbar used for basic rendering.
    ScrollBar {
        bounds: LogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
        /// Optional opacity key for GPU-side fading animation.
        /// If present, the renderer will use this key to look up dynamic opacity.
        /// If None, the alpha channel of `color` is used directly.
        opacity_key: Option<OpacityKey>,
        /// Optional hit-test ID for WebRender hit-testing.
        /// If present, allows event handlers to identify which scrollbar component was clicked.
        hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    },
    /// A fully styled scrollbar with separate track, thumb, and optional buttons.
    /// Used when CSS scrollbar properties are specified.
    ScrollBarStyled {
        /// Complete drawing information for all scrollbar components
        info: Box<ScrollbarDrawInfo>,
    },

    /// An embedded IFrame that references a child DOM with its own display list.
    /// This mirrors webrender's IframeDisplayItem. The renderer will look up
    /// the child display list by child_dom_id and render it within the bounds.
    IFrame {
        /// The DomId of the child DOM (similar to webrender's pipeline_id)
        child_dom_id: DomId,
        /// The bounds where the IFrame should be rendered
        bounds: LogicalRect,
        /// The clip rect for the IFrame content
        clip_rect: LogicalRect,
    },

    // --- State-Management Commands ---
    /// Pushes a new clipping rectangle onto the renderer's clip stack.
    /// All subsequent primitives will be clipped by this rect until a PopClip.
    PushClip {
        bounds: LogicalRect,
        border_radius: BorderRadius,
    },
    /// Pops the current clip from the renderer's clip stack.
    PopClip,

    /// Defines a scrollable area. This is a specialized clip that also
    /// establishes a new coordinate system for its children, which can be offset.
    PushScrollFrame {
        /// The clip rect in the parent's coordinate space.
        clip_bounds: LogicalRect,
        /// The total size of the scrollable content.
        content_size: LogicalSize,
        /// An ID for the renderer to track this scrollable area between frames.
        scroll_id: LocalScrollId, // This would be a renderer-agnostic ID type
    },
    /// Pops the current scroll frame.
    PopScrollFrame,

    /// Pushes a new stacking context for proper z-index layering.
    /// All subsequent primitives until PopStackingContext will be in this stacking context.
    PushStackingContext {
        /// The z-index for this stacking context (for debugging/validation)
        z_index: i32,
        /// The bounds of the stacking context root element
        bounds: LogicalRect,
    },
    /// Pops the current stacking context.
    PopStackingContext,

    /// Defines a region for hit-testing.
    HitTestArea {
        bounds: LogicalRect,
        tag: DisplayListTagId, // This would be a renderer-agnostic ID type
    },

    // --- Gradient Primitives ---
    /// A linear gradient fill.
    LinearGradient {
        bounds: LogicalRect,
        gradient: LinearGradient,
        border_radius: BorderRadius,
    },
    /// A radial gradient fill.
    RadialGradient {
        bounds: LogicalRect,
        gradient: RadialGradient,
        border_radius: BorderRadius,
    },
    /// A conic (angular) gradient fill.
    ConicGradient {
        bounds: LogicalRect,
        gradient: ConicGradient,
        border_radius: BorderRadius,
    },

    // --- Shadow Effects ---
    /// A box shadow (either outset or inset).
    BoxShadow {
        bounds: LogicalRect,
        shadow: StyleBoxShadow,
        border_radius: BorderRadius,
    },

    // --- Filter Effects ---
    /// Push a filter effect that applies to subsequent content.
    PushFilter {
        bounds: LogicalRect,
        filters: Vec<StyleFilter>,
    },
    /// Pop a previously pushed filter.
    PopFilter,

    /// Push a backdrop filter (applies to content behind the element).
    PushBackdropFilter {
        bounds: LogicalRect,
        filters: Vec<StyleFilter>,
    },
    /// Pop a previously pushed backdrop filter.
    PopBackdropFilter,

    /// Push an opacity layer.
    PushOpacity {
        bounds: LogicalRect,
        opacity: f32,
    },
    /// Pop an opacity layer.
    PopOpacity,
}

// Helper structs for the DisplayList
#[derive(Debug, Copy, Clone, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub bottom_right: f32,
}

impl BorderRadius {
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_left == 0.0
            && self.bottom_right == 0.0
    }
}

// Dummy types for compilation
pub type LocalScrollId = u64;
/// Display list tag ID as (payload, type_marker) tuple.
/// The u16 field is used as a namespace marker:
/// - 0x0100 = DOM Node (regular interactive elements)
/// - 0x0200 = Scrollbar component
pub type DisplayListTagId = (u64, u16);

/// Internal builder to accumulate display list items during generation.
#[derive(Debug, Default)]
struct DisplayListBuilder {
    items: Vec<DisplayListItem>,
    node_mapping: Vec<Option<NodeId>>,
    /// Current node being processed (set by generator)
    current_node: Option<NodeId>,
    /// Collected debug messages (transferred to ctx on finalize)
    debug_messages: Vec<LayoutDebugMessage>,
    /// Whether debug logging is enabled
    debug_enabled: bool,
}

impl DisplayListBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_debug(debug_enabled: bool) -> Self {
        Self {
            items: Vec::new(),
            node_mapping: Vec::new(),
            current_node: None,
            debug_messages: Vec::new(),
            debug_enabled,
        }
    }

    /// Log a debug message if debug is enabled
    fn debug_log(&mut self, message: String) {
        if self.debug_enabled {
            self.debug_messages.push(LayoutDebugMessage::info(message));
        }
    }

    /// Build the display list and transfer debug messages to the provided option
    pub fn build_with_debug(
        mut self,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> DisplayList {
        // Transfer collected debug messages to the context
        if let Some(msgs) = debug_messages.as_mut() {
            msgs.append(&mut self.debug_messages);
        }
        DisplayList {
            items: self.items,
            node_mapping: self.node_mapping,
        }
    }

    /// Set the current node context for subsequent push operations
    pub fn set_current_node(&mut self, node_id: Option<NodeId>) {
        self.current_node = node_id;
    }

    /// Push an item and record its node mapping
    fn push_item(&mut self, item: DisplayListItem) {
        self.items.push(item);
        self.node_mapping.push(self.current_node);
    }

    pub fn build(self) -> DisplayList {
        DisplayList {
            items: self.items,
            node_mapping: self.node_mapping,
        }
    }

    pub fn push_hit_test_area(&mut self, bounds: LogicalRect, tag: DisplayListTagId) {
        self.push_item(DisplayListItem::HitTestArea { bounds, tag });
    }

    /// Push a simple single-color scrollbar (legacy method).
    pub fn push_scrollbar(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
        opacity_key: Option<OpacityKey>,
        hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    ) {
        if color.a > 0 || opacity_key.is_some() {
            // Optimization: Don't draw fully transparent items without opacity keys.
            self.push_item(DisplayListItem::ScrollBar {
                bounds,
                color,
                orientation,
                opacity_key,
                hit_id,
            });
        }
    }

    /// Push a fully styled scrollbar with track, thumb, and optional buttons.
    pub fn push_scrollbar_styled(&mut self, info: ScrollbarDrawInfo) {
        // Only push if at least the thumb or track is visible
        if info.thumb_color.a > 0 || info.track_color.a > 0 || info.opacity_key.is_some() {
            self.push_item(DisplayListItem::ScrollBarStyled {
                info: Box::new(info),
            });
        }
    }

    pub fn push_rect(&mut self, bounds: LogicalRect, color: ColorU, border_radius: BorderRadius) {
        if color.a > 0 {
            // Optimization: Don't draw fully transparent items.
            self.push_item(DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            });
        }
    }

    /// Unified method to paint all background layers and border for an element.
    ///
    /// This consolidates the background/border painting logic that was previously
    /// duplicated across:
    /// - paint_node_background_and_border() for block elements
    /// - paint_inline_shape() for inline-block elements
    ///
    /// The backgrounds are painted in order (back to front per CSS spec), followed
    /// by the border.
    pub fn push_backgrounds_and_border(
        &mut self,
        bounds: LogicalRect,
        background_contents: &[azul_css::props::style::StyleBackgroundContent],
        border_info: &BorderInfo,
        simple_border_radius: BorderRadius,
        style_border_radius: StyleBorderRadius,
    ) {
        use azul_css::props::style::StyleBackgroundContent;

        // Paint all background layers in order (CSS paints backgrounds back to front)
        for bg in background_contents {
            match bg {
                StyleBackgroundContent::Color(color) => {
                    self.push_rect(bounds, *color, simple_border_radius);
                }
                StyleBackgroundContent::LinearGradient(gradient) => {
                    self.push_linear_gradient(bounds, gradient.clone(), simple_border_radius);
                }
                StyleBackgroundContent::RadialGradient(gradient) => {
                    self.push_radial_gradient(bounds, gradient.clone(), simple_border_radius);
                }
                StyleBackgroundContent::ConicGradient(gradient) => {
                    self.push_conic_gradient(bounds, gradient.clone(), simple_border_radius);
                }
                StyleBackgroundContent::Image(_image_id) => {
                    // TODO: Implement image backgrounds
                }
            }
        }

        // Paint border
        self.push_border(
            bounds,
            border_info.widths,
            border_info.colors,
            border_info.styles,
            style_border_radius,
        );
    }

    /// Paint backgrounds and border for inline text elements.
    ///
    /// Similar to push_backgrounds_and_border but uses InlineBorderInfo which stores
    /// pre-resolved pixel values instead of CSS property values. This is used for
    /// inline (display: inline) elements where the border info is computed during
    /// text layout and stored in the glyph runs.
    pub fn push_inline_backgrounds_and_border(
        &mut self,
        bounds: LogicalRect,
        background_color: Option<ColorU>,
        background_contents: &[azul_css::props::style::StyleBackgroundContent],
        border: Option<&crate::text3::cache::InlineBorderInfo>,
    ) {
        use azul_css::props::style::StyleBackgroundContent;

        // Paint solid background color if present
        if let Some(bg_color) = background_color {
            self.push_rect(bounds, bg_color, BorderRadius::default());
        }

        // Paint all background layers in order (CSS paints backgrounds back to front)
        for bg in background_contents {
            match bg {
                StyleBackgroundContent::Color(color) => {
                    self.push_rect(bounds, *color, BorderRadius::default());
                }
                StyleBackgroundContent::LinearGradient(gradient) => {
                    self.push_linear_gradient(bounds, gradient.clone(), BorderRadius::default());
                }
                StyleBackgroundContent::RadialGradient(gradient) => {
                    self.push_radial_gradient(bounds, gradient.clone(), BorderRadius::default());
                }
                StyleBackgroundContent::ConicGradient(gradient) => {
                    self.push_conic_gradient(bounds, gradient.clone(), BorderRadius::default());
                }
                StyleBackgroundContent::Image(_image_id) => {
                    // TODO: Implement image backgrounds for inline text
                }
            }
        }

        // Paint border if present
        if let Some(border) = border {
            if border.top > 0.0 || border.right > 0.0 || border.bottom > 0.0 || border.left > 0.0 {
                let border_widths = StyleBorderWidths {
                    top: Some(CssPropertyValue::Exact(LayoutBorderTopWidth {
                        inner: PixelValue::px(border.top),
                    })),
                    right: Some(CssPropertyValue::Exact(LayoutBorderRightWidth {
                        inner: PixelValue::px(border.right),
                    })),
                    bottom: Some(CssPropertyValue::Exact(LayoutBorderBottomWidth {
                        inner: PixelValue::px(border.bottom),
                    })),
                    left: Some(CssPropertyValue::Exact(LayoutBorderLeftWidth {
                        inner: PixelValue::px(border.left),
                    })),
                };
                let border_colors = StyleBorderColors {
                    top: Some(CssPropertyValue::Exact(StyleBorderTopColor {
                        inner: border.top_color,
                    })),
                    right: Some(CssPropertyValue::Exact(StyleBorderRightColor {
                        inner: border.right_color,
                    })),
                    bottom: Some(CssPropertyValue::Exact(StyleBorderBottomColor {
                        inner: border.bottom_color,
                    })),
                    left: Some(CssPropertyValue::Exact(StyleBorderLeftColor {
                        inner: border.left_color,
                    })),
                };
                let border_styles = StyleBorderStyles {
                    top: Some(CssPropertyValue::Exact(StyleBorderTopStyle {
                        inner: BorderStyle::Solid,
                    })),
                    right: Some(CssPropertyValue::Exact(StyleBorderRightStyle {
                        inner: BorderStyle::Solid,
                    })),
                    bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle {
                        inner: BorderStyle::Solid,
                    })),
                    left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle {
                        inner: BorderStyle::Solid,
                    })),
                };
                let radius_px = PixelValue::px(border.radius.unwrap_or(0.0));
                let border_radius = StyleBorderRadius {
                    top_left: radius_px,
                    top_right: radius_px,
                    bottom_left: radius_px,
                    bottom_right: radius_px,
                };

                self.push_border(
                    bounds,
                    border_widths,
                    border_colors,
                    border_styles,
                    border_radius,
                );
            }
        }
    }

    /// Push a linear gradient background
    pub fn push_linear_gradient(
        &mut self,
        bounds: LogicalRect,
        gradient: LinearGradient,
        border_radius: BorderRadius,
    ) {
        self.push_item(DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        });
    }

    /// Push a radial gradient background
    pub fn push_radial_gradient(
        &mut self,
        bounds: LogicalRect,
        gradient: RadialGradient,
        border_radius: BorderRadius,
    ) {
        self.push_item(DisplayListItem::RadialGradient {
            bounds,
            gradient,
            border_radius,
        });
    }

    /// Push a conic gradient background
    pub fn push_conic_gradient(
        &mut self,
        bounds: LogicalRect,
        gradient: ConicGradient,
        border_radius: BorderRadius,
    ) {
        self.push_item(DisplayListItem::ConicGradient {
            bounds,
            gradient,
            border_radius,
        });
    }

    pub fn push_selection_rect(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        border_radius: BorderRadius,
    ) {
        if color.a > 0 {
            self.push_item(DisplayListItem::SelectionRect {
                bounds,
                color,
                border_radius,
            });
        }
    }

    pub fn push_cursor_rect(&mut self, bounds: LogicalRect, color: ColorU) {
        if color.a > 0 {
            self.push_item(DisplayListItem::CursorRect { bounds, color });
        }
    }
    pub fn push_clip(&mut self, bounds: LogicalRect, border_radius: BorderRadius) {
        self.push_item(DisplayListItem::PushClip {
            bounds,
            border_radius,
        });
    }
    pub fn pop_clip(&mut self) {
        self.push_item(DisplayListItem::PopClip);
    }
    pub fn push_scroll_frame(
        &mut self,
        clip_bounds: LogicalRect,
        content_size: LogicalSize,
        scroll_id: LocalScrollId,
    ) {
        self.push_item(DisplayListItem::PushScrollFrame {
            clip_bounds,
            content_size,
            scroll_id,
        });
    }
    pub fn pop_scroll_frame(&mut self) {
        self.push_item(DisplayListItem::PopScrollFrame);
    }
    pub fn push_border(
        &mut self,
        bounds: LogicalRect,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        border_radius: StyleBorderRadius,
    ) {
        // Check if any border side is visible
        let has_visible_border = {
            let has_width = widths.top.is_some()
                || widths.right.is_some()
                || widths.bottom.is_some()
                || widths.left.is_some();
            let has_style = styles.top.is_some()
                || styles.right.is_some()
                || styles.bottom.is_some()
                || styles.left.is_some();
            has_width && has_style
        };

        if has_visible_border {
            self.push_item(DisplayListItem::Border {
                bounds,
                widths,
                colors,
                styles,
                border_radius,
            });
        }
    }

    pub fn push_stacking_context(&mut self, z_index: i32, bounds: LogicalRect) {
        self.push_item(DisplayListItem::PushStackingContext { z_index, bounds });
    }

    pub fn pop_stacking_context(&mut self) {
        self.push_item(DisplayListItem::PopStackingContext);
    }

    pub fn push_text_run(
        &mut self,
        glyphs: Vec<GlyphInstance>,
        font_hash: FontHash, // Just the hash, not the full FontRef
        font_size_px: f32,
        color: ColorU,
        clip_rect: LogicalRect,
    ) {
        self.debug_log(format!(
            "[push_text_run] {} glyphs, font_size={}px, color=({},{},{},{}), clip={:?}",
            glyphs.len(),
            font_size_px,
            color.r,
            color.g,
            color.b,
            color.a,
            clip_rect
        ));

        if !glyphs.is_empty() && color.a > 0 {
            self.push_item(DisplayListItem::Text {
                glyphs,
                font_hash,
                font_size_px,
                color,
                clip_rect,
            });
        } else {
            self.debug_log(format!(
                "[push_text_run] SKIPPED: glyphs.is_empty()={}, color.a={}",
                glyphs.is_empty(),
                color.a
            ));
        }
    }

    pub fn push_text_layout(
        &mut self,
        layout: Arc<dyn std::any::Any + Send + Sync>,
        bounds: LogicalRect,
        font_hash: FontHash,
        font_size_px: f32,
        color: ColorU,
    ) {
        if color.a > 0 {
            self.push_item(DisplayListItem::TextLayout {
                layout,
                bounds,
                font_hash,
                font_size_px,
                color,
            });
        }
    }

    pub fn push_underline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Underline {
                bounds,
                color,
                thickness,
            });
        }
    }

    pub fn push_strikethrough(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            });
        }
    }

    pub fn push_overline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            });
        }
    }

    pub fn push_image(&mut self, bounds: LogicalRect, image: ImageRef) {
        self.push_item(DisplayListItem::Image { bounds, image });
    }
}

/// Main entry point for generating the display list.
pub fn generate_display_list<T: ParsedFontTrait + Sync + 'static>(
    ctx: &mut LayoutContext<T>,
    tree: &LayoutTree,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    scroll_ids: &BTreeMap<usize, u64>,
    gpu_value_cache: Option<&GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
) -> Result<DisplayList> {
    debug_info!(
        ctx,
        "[DisplayList] generate_display_list: tree has {} nodes, {} positions calculated",
        tree.nodes.len(),
        calculated_positions.len()
    );

    debug_info!(ctx, "Starting display list generation");
    debug_info!(
        ctx,
        "Collecting stacking contexts from root node {}",
        tree.root
    );

    let positioned_tree = PositionedTree {
        tree,
        calculated_positions,
    };
    let mut generator = DisplayListGenerator::new(
        ctx,
        scroll_offsets,
        &positioned_tree,
        scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    );

    // Create builder with debug enabled if ctx has debug messages
    let debug_enabled = generator.ctx.debug_messages.is_some();
    let mut builder = DisplayListBuilder::with_debug(debug_enabled);

    // 1. Build a tree of stacking contexts, which defines the global paint order.
    let stacking_context_tree = generator.collect_stacking_contexts(tree.root)?;

    // 2. Traverse the stacking context tree to generate display items in the correct order.
    debug_info!(
        generator.ctx,
        "Generating display items from stacking context tree"
    );
    generator.generate_for_stacking_context(&mut builder, &stacking_context_tree)?;

    // Build display list and transfer debug messages to context
    let display_list = builder.build_with_debug(generator.ctx.debug_messages);
    debug_info!(
        generator.ctx,
        "[DisplayList] Generated {} display items",
        display_list.items.len()
    );
    Ok(display_list)
}

/// A helper struct that holds all necessary state and context for the generation process.
struct DisplayListGenerator<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
    positioned_tree: &'a PositionedTree<'a>,
    scroll_ids: &'a BTreeMap<usize, u64>,
    gpu_value_cache: Option<&'a GpuValueCache>,
    renderer_resources: &'a RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
}

/// Represents a node in the CSS stacking context tree, not the DOM tree.
#[derive(Debug)]
struct StackingContext {
    node_index: usize,
    z_index: i32,
    child_contexts: Vec<StackingContext>,
    /// Children that do not create their own stacking contexts and are painted in DOM order.
    in_flow_children: Vec<usize>,
}

impl<'a, 'b, T> DisplayListGenerator<'a, 'b, T>
where
    T: ParsedFontTrait + Sync + 'static,
{
    pub fn new(
        ctx: &'a mut LayoutContext<'b, T>,
        scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
        positioned_tree: &'a PositionedTree<'a>,
        scroll_ids: &'a BTreeMap<usize, u64>,
        gpu_value_cache: Option<&'a GpuValueCache>,
        renderer_resources: &'a RendererResources,
        id_namespace: IdNamespace,
        dom_id: DomId,
    ) -> Self {
        Self {
            ctx,
            scroll_offsets,
            positioned_tree,
            scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        }
    }

    /// Helper to get styled node state for a node
    fn get_styled_node_state(&self, dom_id: NodeId) -> azul_core::styled_dom::StyledNodeState {
        self.ctx
            .styled_dom
            .styled_nodes
            .as_container()
            .get(dom_id)
            .map(|n| n.styled_node_state.clone())
            .unwrap_or_default()
    }

    /// Gets the cursor type for a text node from its CSS properties.
    /// Defaults to Text (I-beam) cursor if no explicit cursor is set.
    fn get_cursor_type_for_text_node(&self, node_id: NodeId) -> CursorType {
        use azul_css::props::style::effects::StyleCursor;
        
        let styled_node_state = self.get_styled_node_state(node_id);
        let node_data_container = self.ctx.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id);
        
        // Query the cursor CSS property for this text node
        if let Some(node_data) = node_data {
            if let Some(cursor_value) = self.ctx.styled_dom.get_css_property_cache().get_cursor(
                node_data,
                &node_id,
                &styled_node_state,
            ) {
                if let CssPropertyValue::Exact(cursor) = cursor_value {
                    return match cursor {
                        StyleCursor::Default => CursorType::Default,
                        StyleCursor::Pointer => CursorType::Pointer,
                        StyleCursor::Text => CursorType::Text,
                        StyleCursor::Crosshair => CursorType::Crosshair,
                        StyleCursor::Move => CursorType::Move,
                        StyleCursor::Help => CursorType::Help,
                        StyleCursor::Wait => CursorType::Wait,
                        StyleCursor::Progress => CursorType::Progress,
                        StyleCursor::NsResize => CursorType::NsResize,
                        StyleCursor::EwResize => CursorType::EwResize,
                        StyleCursor::NeswResize => CursorType::NeswResize,
                        StyleCursor::NwseResize => CursorType::NwseResize,
                        StyleCursor::NResize => CursorType::NResize,
                        StyleCursor::SResize => CursorType::SResize,
                        StyleCursor::EResize => CursorType::EResize,
                        StyleCursor::WResize => CursorType::WResize,
                        StyleCursor::Grab => CursorType::Grab,
                        StyleCursor::Grabbing => CursorType::Grabbing,
                        StyleCursor::RowResize => CursorType::RowResize,
                        StyleCursor::ColResize => CursorType::ColResize,
                        // Map less common cursors to closest available
                        StyleCursor::SeResize | StyleCursor::NeswResize => CursorType::NeswResize,
                        StyleCursor::ZoomIn | StyleCursor::ZoomOut => CursorType::Default,
                        StyleCursor::Copy | StyleCursor::Alias => CursorType::Default,
                        StyleCursor::Cell => CursorType::Crosshair,
                        StyleCursor::AllScroll => CursorType::Move,
                        StyleCursor::ContextMenu => CursorType::Default,
                        StyleCursor::VerticalText => CursorType::Text,
                        StyleCursor::Unset => CursorType::Text, // Default to text for text nodes
                    };
                }
            }
        }
        
        // Default: Text cursor (I-beam) for text nodes
        CursorType::Text
    }

    /// Emits drawing commands for text selections only (not cursor).
    /// The cursor is drawn separately via `paint_cursor()`.
    fn paint_selections(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };
        
        // Get inline layout using the unified helper that handles IFC membership
        // This is critical: text nodes don't have their own inline_layout_result,
        // but they have ifc_membership pointing to their IFC root
        let Some(layout) = self.positioned_tree.tree.get_inline_layout_for_node(node_index) else {
            return Ok(());
        };

        // Get the absolute position of this node (border-box position)
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(&node_index)
            .copied()
            .unwrap_or_default();

        // Selection rects are relative to content-box origin
        let padding = &node.box_props.padding;
        let border = &node.box_props.border;
        let content_box_offset_x = node_pos.x + padding.left + border.left;
        let content_box_offset_y = node_pos.y + padding.top + border.top;

        // Check if text is selectable (respects CSS user-select property)
        let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        let is_selectable = super::getters::is_text_selectable(self.ctx.styled_dom, dom_id, node_state);
        
        if !is_selectable {
            return Ok(());
        }

        // === NEW: Check text_selections first (multi-node selection support) ===
        if let Some(text_selection) = self.ctx.text_selections.get(&self.ctx.styled_dom.dom_id) {
            if let Some(range) = text_selection.affected_nodes.get(&dom_id) {
                let is_collapsed = text_selection.is_collapsed();
                
                // Only draw selection highlight if NOT collapsed
                if !is_collapsed {
                    let rects = layout.get_selection_rects(range);
                    let style = get_selection_style(self.ctx.styled_dom, Some(dom_id));

                    let border_radius = BorderRadius {
                        top_left: style.radius,
                        top_right: style.radius,
                        bottom_left: style.radius,
                        bottom_right: style.radius,
                    };

                    for mut rect in rects {
                        rect.origin.x += content_box_offset_x;
                        rect.origin.y += content_box_offset_y;
                        builder.push_selection_rect(rect, style.bg_color, border_radius);
                    }
                }
                
                return Ok(());
            }
        }

        // === LEGACY: Fall back to old selections for backward compatibility ===
        let Some(selection_state) = self.ctx.selections.get(&self.ctx.styled_dom.dom_id) else {
            return Ok(());
        };

        if selection_state.node_id.node.into_crate_internal() != Some(dom_id) {
            return Ok(());
        }

        for selection in selection_state.selections.as_slice() {
            if let Selection::Range(range) = &selection {
                let rects = layout.get_selection_rects(range);
                let style = get_selection_style(self.ctx.styled_dom, Some(dom_id));

                let border_radius = BorderRadius {
                    top_left: style.radius,
                    top_right: style.radius,
                    bottom_left: style.radius,
                    bottom_right: style.radius,
                };

                for mut rect in rects {
                    rect.origin.x += content_box_offset_x;
                    rect.origin.y += content_box_offset_y;
                    builder.push_selection_rect(rect, style.bg_color, border_radius);
                }
            }
        }

        Ok(())
    }

    /// Emits drawing commands for the text cursor (caret) only.
    /// This is separate from selections and reads from `ctx.cursor_location`.
    fn paint_cursor(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        // Early exit if cursor is not visible (blinking off phase)
        if !self.ctx.cursor_is_visible {
            return Ok(());
        }
        
        // Early exit if no cursor location is set
        let Some((cursor_dom_id, cursor_node_id, cursor)) = &self.ctx.cursor_location else {
            return Ok(());
        };

        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };
        
        // Only paint cursor on the node that has the cursor
        if dom_id != *cursor_node_id {
            return Ok(());
        }
        
        // Check DOM ID matches
        if self.ctx.styled_dom.dom_id != *cursor_dom_id {
            return Ok(());
        }

        // Get inline layout using the unified helper that handles IFC membership
        // This is critical: text nodes don't have their own inline_layout_result,
        // but they have ifc_membership pointing to their IFC root
        let Some(layout) = self.positioned_tree.tree.get_inline_layout_for_node(node_index) else {
            return Ok(());
        };

        // Check if this node is contenteditable (or inherits contenteditable from ancestor)
        // Text nodes don't have contenteditable directly, but inherit it from their container
        let is_contenteditable = super::getters::is_node_contenteditable_inherited(self.ctx.styled_dom, dom_id);
        if !is_contenteditable {
            return Ok(());
        }
        
        // Check if text is selectable
        let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        let is_selectable = super::getters::is_text_selectable(self.ctx.styled_dom, dom_id, node_state);
        if !is_selectable {
            return Ok(());
        }

        // Get cursor rect from text layout
        let Some(mut rect) = layout.get_cursor_rect(cursor) else {
            return Ok(());
        };

        // Get the absolute position of this node (border-box position)
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(&node_index)
            .copied()
            .unwrap_or_default();

        // Adjust to content-box coordinates
        let padding = &node.box_props.padding;
        let border = &node.box_props.border;
        let content_box_offset_x = node_pos.x + padding.left + border.left;
        let content_box_offset_y = node_pos.y + padding.top + border.top;

        rect.origin.x += content_box_offset_x;
        rect.origin.y += content_box_offset_y;

        let style = get_caret_style(self.ctx.styled_dom, Some(dom_id));
        
        // Apply caret width from CSS (default is 2px, get_cursor_rect returns 1px)
        rect.size.width = style.width;
        
        builder.push_cursor_rect(rect, style.color);

        Ok(())
    }

    /// Emits drawing commands for selection and cursor.
    /// Delegates to `paint_selections()` and `paint_cursor()`.
    fn paint_selection_and_cursor(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        self.paint_selections(builder, node_index)?;
        self.paint_cursor(builder, node_index)?;
        Ok(())
    }

    /// Recursively builds the tree of stacking contexts starting from a given layout node.
    fn collect_stacking_contexts(&mut self, node_index: usize) -> Result<StackingContext> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let z_index = get_z_index(self.ctx.styled_dom, node.dom_node_id);

        if let Some(dom_id) = node.dom_node_id {
            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(
                self.ctx,
                "Collecting stacking context for node {} ({:?}), z-index={}",
                node_index,
                node_type.get_node_type(),
                z_index
            );
        }

        let mut child_contexts = Vec::new();
        let mut in_flow_children = Vec::new();

        for &child_index in &node.children {
            if self.establishes_stacking_context(child_index) {
                child_contexts.push(self.collect_stacking_contexts(child_index)?);
            } else {
                in_flow_children.push(child_index);
            }
        }

        Ok(StackingContext {
            node_index,
            z_index,
            child_contexts,
            in_flow_children,
        })
    }

    /// Recursively traverses the stacking context tree, emitting drawing commands to the builder
    /// according to the CSS Painting Algorithm specification.
    fn generate_for_stacking_context(
        &mut self,
        builder: &mut DisplayListBuilder,
        context: &StackingContext,
    ) -> Result<()> {
        // Before painting the node, check if it establishes a new clip or scroll frame.
        let node = self
            .positioned_tree
            .tree
            .get(context.node_index)
            .ok_or(LayoutError::InvalidTree)?;

        if let Some(dom_id) = node.dom_node_id {
            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(
                self.ctx,
                "Painting stacking context for node {} ({:?}), z-index={}, {} child contexts, {} \
                 in-flow children",
                context.node_index,
                node_type.get_node_type(),
                context.z_index,
                context.child_contexts.len(),
                context.in_flow_children.len()
            );
        }

        // Push a stacking context for WebRender
        // Get the node's bounds for the stacking context
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(&context.node_index)
            .copied()
            .unwrap_or_default();
        let node_size = node.used_size.unwrap_or(LogicalSize {
            width: 0.0,
            height: 0.0,
        });
        let node_bounds = LogicalRect {
            origin: node_pos,
            size: node_size,
        };
        builder.push_stacking_context(context.z_index, node_bounds);

        // 1. Paint background and borders for the context's root element.
        // This must be BEFORE push_node_clips so the container background
        // is rendered in parent space (stationary), not scroll space.
        self.paint_node_background_and_border(builder, context.node_index)?;

        // 1b. For scrollable containers, push the hit-test area BEFORE the scroll frame
        // so the hit-test covers the entire container box (including visible area),
        // not just the scrolled content. This ensures scroll wheel events hit the
        // container regardless of scroll position.
        if let Some(dom_id) = node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
            let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
            if overflow_x.is_scroll() || overflow_y.is_scroll() {
                if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, node.dom_node_id) {
                    builder.push_hit_test_area(node_bounds, tag_id);
                }
            }
        }

        // 2. Push clips and scroll frames AFTER painting background
        let did_push_clip_or_scroll = self.push_node_clips(builder, context.node_index, node)?;

        // 3. Paint child stacking contexts with negative z-indices.
        let mut negative_z_children: Vec<_> = context
            .child_contexts
            .iter()
            .filter(|c| c.z_index < 0)
            .collect();
        negative_z_children.sort_by_key(|c| c.z_index);
        for child in negative_z_children {
            self.generate_for_stacking_context(builder, child)?;
        }

        // 4. Paint the in-flow descendants of the context root.
        self.paint_in_flow_descendants(builder, context.node_index, &context.in_flow_children)?;

        // 5. Paint child stacking contexts with z-index: 0 / auto.
        for child in context.child_contexts.iter().filter(|c| c.z_index == 0) {
            self.generate_for_stacking_context(builder, child)?;
        }

        // 6. Paint child stacking contexts with positive z-indices.
        let mut positive_z_children: Vec<_> = context
            .child_contexts
            .iter()
            .filter(|c| c.z_index > 0)
            .collect();

        positive_z_children.sort_by_key(|c| c.z_index);

        for child in positive_z_children {
            self.generate_for_stacking_context(builder, child)?;
        }

        // Pop the stacking context for WebRender
        builder.pop_stacking_context();

        // After painting the node and all its descendants, pop any contexts it pushed.
        if did_push_clip_or_scroll {
            self.pop_node_clips(builder, node)?;
        }

        // Paint scrollbars AFTER popping the clip, so they appear on top of content
        // and are not clipped by the scroll frame
        self.paint_scrollbars(builder, context.node_index)?;

        Ok(())
    }

    /// Paints the content and non-stacking-context children.
    fn paint_in_flow_descendants(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        children_indices: &[usize],
    ) -> Result<()> {
        // NOTE: We do NOT paint the node's background here - that was already done by
        // generate_for_stacking_context! Only paint selection, cursor, and content for the
        // current node

        // 2. Paint selection highlights and the text cursor if applicable.
        self.paint_selection_and_cursor(builder, node_index)?;

        // 3. Paint the node's own content (text, images, hit-test areas).
        self.paint_node_content(builder, node_index)?;

        // 4. Recursively paint the in-flow children in correct CSS painting order:
        //    - First: Non-float block-level children
        //    - Then: Float children (so they appear on top)
        //    - Finally: Inline-level children (though typically handled above in
        //      paint_node_content)

        // Separate children into floats and non-floats
        let mut non_float_children = Vec::new();
        let mut float_children = Vec::new();

        for &child_index in children_indices {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child is a float
            let is_float = if let Some(dom_id) = child_node.dom_node_id {
                use crate::solver3::getters::get_float;
                let styled_node_state = self.get_styled_node_state(dom_id);
                let float_value = get_float(self.ctx.styled_dom, dom_id, &styled_node_state);
                !matches!(
                    float_value.unwrap_or_default(),
                    azul_css::props::layout::LayoutFloat::None
                )
            } else {
                false
            };

            if is_float {
                float_children.push(child_index);
            } else {
                non_float_children.push(child_index);
            }
        }

        // Paint non-float children first
        for child_index in non_float_children {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // IMPORTANT: Paint background and border BEFORE pushing clips!
            // This ensures the container's background is in parent space (stationary),
            // not in scroll space. Same logic as generate_for_stacking_context.
            self.paint_node_background_and_border(builder, child_index)?;

            // Push clips and scroll frames AFTER painting background
            let did_push_clip = self.push_node_clips(builder, child_index, child_node)?;

            // Paint descendants inside the clip/scroll frame
            self.paint_in_flow_descendants(builder, child_index, &child_node.children)?;

            // Pop the child's clips.
            if did_push_clip {
                self.pop_node_clips(builder, child_node)?;
            }

            // Paint scrollbars AFTER popping clips so they appear on top of content
            self.paint_scrollbars(builder, child_index)?;
        }

        // Paint float children AFTER non-floats (so they appear on top)
        for child_index in float_children {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Same as above: paint background BEFORE clips
            self.paint_node_background_and_border(builder, child_index)?;
            let did_push_clip = self.push_node_clips(builder, child_index, child_node)?;
            self.paint_in_flow_descendants(builder, child_index, &child_node.children)?;

            if did_push_clip {
                self.pop_node_clips(builder, child_node)?;
            }

            // Paint scrollbars AFTER popping clips so they appear on top of content
            self.paint_scrollbars(builder, child_index)?;
        }

        Ok(())
    }

    /// Checks if a node requires clipping or scrolling and pushes the appropriate commands.
    /// Returns true if any command was pushed.
    fn push_node_clips(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        node: &LayoutNode,
    ) -> Result<bool> {
        let Some(dom_id) = node.dom_node_id else {
            return Ok(false);
        };

        let styled_node_state = self.get_styled_node_state(dom_id);

        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);

        let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();
        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(
            self.ctx.styled_dom,
            dom_id,
            &styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        let needs_clip = overflow_x.is_clipped() || overflow_y.is_clipped();

        if !needs_clip {
            return Ok(false);
        }

        let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();

        let border = &node.box_props.border;

        // Get scrollbar info to adjust clip rect for content area
        let scrollbar_info = get_scrollbar_info_from_layout(node);

        // The clip rect for content should exclude the scrollbar area
        // Scrollbars are drawn inside the border-box, on the right/bottom edges
        let clip_rect = LogicalRect {
            origin: LogicalPosition {
                x: paint_rect.origin.x + border.left,
                y: paint_rect.origin.y + border.top,
            },
            size: LogicalSize {
                // Reduce width/height by scrollbar dimensions so content doesn't overlap scrollbar
                width: (paint_rect.size.width
                    - border.left
                    - border.right
                    - scrollbar_info.scrollbar_width)
                    .max(0.0),
                height: (paint_rect.size.height
                    - border.top
                    - border.bottom
                    - scrollbar_info.scrollbar_height)
                    .max(0.0),
            },
        };

        if overflow_x.is_scroll() || overflow_y.is_scroll() {
            // For scroll/auto: push BOTH a clip AND a scroll frame
            // The clip ensures content is clipped (in parent space)
            // The scroll frame enables scrolling (creates new spatial node)
            builder.push_clip(clip_rect, border_radius);
            let scroll_id = self.scroll_ids.get(&node_index).copied().unwrap_or(0);
            let content_size = get_scroll_content_size(node);
            builder.push_scroll_frame(clip_rect, content_size, scroll_id);
        } else {
            // Simple clip for hidden/clip
            builder.push_clip(clip_rect, border_radius);
        }

        Ok(true)
    }

    /// Pops any clip/scroll commands associated with a node.
    fn pop_node_clips(&self, builder: &mut DisplayListBuilder, node: &LayoutNode) -> Result<()> {
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);

        let paint_rect = self
            .get_paint_rect(
                self.positioned_tree
                    .tree
                    .nodes
                    .iter()
                    .position(|n| n.dom_node_id == Some(dom_id))
                    .unwrap_or(0),
            )
            .unwrap_or_default();

        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(
            self.ctx.styled_dom,
            dom_id,
            &styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        let needs_clip =
            overflow_x.is_clipped() || overflow_y.is_clipped() || !border_radius.is_zero();

        if needs_clip {
            if overflow_x.is_scroll() || overflow_y.is_scroll() {
                // For scroll or auto overflow, pop both scroll frame AND clip
                builder.pop_scroll_frame();
                builder.pop_clip();
            } else {
                // For hidden/clip, pop the simple clip
                builder.pop_clip();
            }
        }
        Ok(())
    }

    /// Calculates the final paint-time rectangle for a node.
    /// 
    /// ## Coordinate Space
    /// 
    /// Returns the node's position in **absolute window coordinates** (logical pixels).
    /// This is the coordinate space used throughout the display list:
    /// 
    /// - Origin: Top-left corner of the window
    /// - Units: Logical pixels (HiDPI scaling happens in compositor2.rs)
    /// - Scroll: NOT applied here - WebRender scroll frames handle scroll offset
    ///   transformation internally via `define_scroll_frame()`
    /// 
    /// ## Important
    /// 
    /// Do NOT manually subtract scroll offset here! WebRender's scroll spatial
    /// transforms handle this. Subtracting here would cause double-offset and
    /// parallax effects (backgrounds and text moving at different speeds).
    fn get_paint_rect(&self, node_index: usize) -> Option<LogicalRect> {
        let node = self.positioned_tree.tree.get(node_index)?;
        let pos = self
            .positioned_tree
            .calculated_positions
            .get(&node_index)
            .copied()
            .unwrap_or_default();
        let size = node.used_size.unwrap_or_default();

        // NOTE: Scroll offset is NOT applied here!
        // WebRender scroll frames handle scroll transformation.
        // See compositor2.rs PushScrollFrame for details.

        Some(LogicalRect::new(pos, size))
    }

    /// Emits drawing commands for the background and border of a single node.
    fn paint_node_background_and_border(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;

        // Set current node for node mapping (for pagination break properties)
        builder.set_current_node(node.dom_node_id);

        // Skip inline and inline-block elements - they are rendered by text3 in paint_inline_content
        // Inline elements participate in inline formatting context and their backgrounds
        // must be positioned by the text layout engine, not the block layout engine
        if let Some(dom_id) = node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let display = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_display(
                    &self.ctx.styled_dom.node_data.as_container()[dom_id],
                    &dom_id,
                    &styled_node_state,
                )
                .and_then(|v| v.get_property().cloned())
                .unwrap_or(LayoutDisplay::Inline);

            if display == LayoutDisplay::InlineBlock || display == LayoutDisplay::Inline {
                // text3 will handle this via InlineShape (for inline-block)
                // or glyph runs with background_color (for inline)
                return Ok(());
            }
        }

        // CSS 2.2 Section 17.5.1: Tables in the visual formatting model
        // Tables have a special 6-layer background painting order
        if matches!(node.formatting_context, FormattingContext::Table) {
            debug_info!(
                self.ctx,
                "Painting table backgrounds/borders for node {} at {:?}",
                node_index,
                paint_rect
            );
            // Delegate to specialized table painting function
            return self.paint_table_items(builder, node_index);
        }

        let border_radius = if let Some(dom_id) = node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let background_contents =
                get_background_contents(self.ctx.styled_dom, dom_id, &styled_node_state);
            let border_info = get_border_info(self.ctx.styled_dom, dom_id, &styled_node_state);

            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(
                self.ctx,
                "Painting background/border for node {} ({:?}) at {:?}, backgrounds={:?}",
                node_index,
                node_type.get_node_type(),
                paint_rect,
                background_contents.len()
            );

            // Get both versions: simple BorderRadius for rect clipping and StyleBorderRadius for
            // border rendering
            let element_size = PhysicalSizeImport {
                width: paint_rect.size.width,
                height: paint_rect.size.height,
            };
            let simple_border_radius = get_border_radius(
                self.ctx.styled_dom,
                dom_id,
                &styled_node_state,
                element_size,
                self.ctx.viewport_size,
            );
            let style_border_radius =
                get_style_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state);

            // Use unified background/border painting
            builder.push_backgrounds_and_border(
                paint_rect,
                &background_contents,
                &border_info,
                simple_border_radius,
                style_border_radius,
            );

            simple_border_radius
        } else {
            BorderRadius::default()
        };

        Ok(())
    }

    /// CSS 2.2 Section 17.5.1: Table background painting in 6 layers
    ///
    /// Implements the CSS 2.2 specification for table background painting order.
    /// Unlike regular block elements, tables paint backgrounds in layers from back to front:
    ///
    /// 1. Table background (lowest layer)
    /// 2. Column group backgrounds
    /// 3. Column backgrounds
    /// 4. Row group backgrounds
    /// 5. Row backgrounds
    /// 6. Cell backgrounds (topmost layer)
    ///
    /// Then borders are painted (respecting border-collapse mode).
    /// Finally, cell content is painted on top of everything.
    ///
    /// This function generates simple display list items (Rect, Border) in the correct
    /// CSS paint order, making WebRender integration trivial.
    fn paint_table_items(
        &self,
        builder: &mut DisplayListBuilder,
        table_index: usize,
    ) -> Result<()> {
        let table_node = self
            .positioned_tree
            .tree
            .get(table_index)
            .ok_or(LayoutError::InvalidTree)?;

        let Some(table_paint_rect) = self.get_paint_rect(table_index) else {
            return Ok(());
        };

        // Layer 1: Table background
        if let Some(dom_id) = table_node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);
            let element_size = PhysicalSizeImport {
                width: table_paint_rect.size.width,
                height: table_paint_rect.size.height,
            };
            let border_radius = get_border_radius(
                self.ctx.styled_dom,
                dom_id,
                &styled_node_state,
                element_size,
                self.ctx.viewport_size,
            );

            builder.push_rect(table_paint_rect, bg_color, border_radius);
        }

        // Traverse table children to paint layers 2-6

        // Layer 2: Column group backgrounds
        // Layer 3: Column backgrounds (columns are children of column groups)
        for &child_idx in &table_node.children {
            let child_node = self.positioned_tree.tree.get(child_idx);
            if let Some(node) = child_node {
                if matches!(node.formatting_context, FormattingContext::TableColumnGroup) {
                    // Paint column group background
                    self.paint_element_background(builder, child_idx)?;

                    // Paint backgrounds of individual columns within this group
                    for &col_idx in &node.children {
                        self.paint_element_background(builder, col_idx)?;
                    }
                }
            }
        }

        // Layer 4: Row group backgrounds (tbody, thead, tfoot)
        // Layer 5: Row backgrounds
        // Layer 6: Cell backgrounds
        for &child_idx in &table_node.children {
            let child_node = self.positioned_tree.tree.get(child_idx);
            if let Some(node) = child_node {
                match node.formatting_context {
                    FormattingContext::TableRowGroup => {
                        // Paint row group background
                        self.paint_element_background(builder, child_idx)?;

                        // Paint rows within this group
                        for &row_idx in &node.children {
                            self.paint_table_row_and_cells(builder, row_idx)?;
                        }
                    }
                    FormattingContext::TableRow => {
                        // Direct row child (no row group wrapper)
                        self.paint_table_row_and_cells(builder, child_idx)?;
                    }
                    _ => {}
                }
            }
        }

        // Borders are painted separately after all backgrounds
        // This is handled by the normal rendering flow for each element
        // TODO: Implement border-collapse conflict resolution using BorderInfo::resolve_conflict()

        Ok(())
    }

    /// Helper function to paint a table row's background and then its cells' backgrounds
    /// Layer 5: Row background
    /// Layer 6: Cell backgrounds (painted after row, so they appear on top)
    fn paint_table_row_and_cells(
        &self,
        builder: &mut DisplayListBuilder,
        row_idx: usize,
    ) -> Result<()> {
        // Layer 5: Paint row background
        self.paint_element_background(builder, row_idx)?;

        // Layer 6: Paint cell backgrounds (topmost layer)
        let row_node = self.positioned_tree.tree.get(row_idx);
        if let Some(node) = row_node {
            for &cell_idx in &node.children {
                self.paint_element_background(builder, cell_idx)?;
            }
        }

        Ok(())
    }

    /// Helper function to paint an element's background (used for all table elements)
    /// Reads background-color and border-radius from CSS properties and emits push_rect()
    fn paint_element_background(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        let Some(node) = self.positioned_tree.tree.get(node_index) else {
            return Ok(());
        };
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);

        // Only paint if background color has alpha > 0 (optimization)
        if bg_color.a == 0 {
            return Ok(());
        }

        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(
            self.ctx.styled_dom,
            dom_id,
            &styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        builder.push_rect(paint_rect, bg_color, border_radius);

        Ok(())
    }

    /// Emits drawing commands for the foreground content, including hit-test areas and scrollbars.
    fn paint_node_content(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;

        // Set current node for node mapping (for pagination break properties)
        builder.set_current_node(node.dom_node_id);

        let Some(mut paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // For text nodes (with inline layout), the used_size might be 0x0.
        // In this case, compute the bounds from the inline layout result.
        if paint_rect.size.width == 0.0 || paint_rect.size.height == 0.0 {
            if let Some(cached_layout) = &node.inline_layout_result {
                let content_bounds = cached_layout.layout.bounds();
                paint_rect.size.width = content_bounds.width;
                paint_rect.size.height = content_bounds.height;
            }
        }

        // Add a hit-test area for this node if it's interactive.
        // NOTE: For scrollable containers (overflow: scroll/auto), the hit-test area
        // was already pushed in generate_for_stacking_context BEFORE the scroll frame,
        // so we skip it here to avoid duplicate hit-test areas that would scroll with content.
        if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, node.dom_node_id) {
            let is_scrollable = if let Some(dom_id) = node.dom_node_id {
                let styled_node_state = self.get_styled_node_state(dom_id);
                let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
                let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
                overflow_x.is_scroll() || overflow_y.is_scroll()
            } else {
                false
            };

            // Push hit-test area for this node ONLY if it's not a scrollable container.
            // Scrollable containers already have their hit-test area pushed BEFORE the scroll frame
            // in generate_for_stacking_context, ensuring the hit-test stays stationary in parent space
            // while content scrolls. Pushing it again here would create a duplicate that scrolls
            // with content, causing hit-test failures when scrolled to the bottom.
            if !is_scrollable {
                builder.push_hit_test_area(paint_rect, tag_id);
            }
        }

        // Paint the node's visible content.
        if let Some(cached_layout) = &node.inline_layout_result {
            let inline_layout = &cached_layout.layout;
            debug_info!(
                self.ctx,
                "[paint_node] node {} has inline_layout with {} items",
                node_index,
                inline_layout.items.len()
            );

            if let Some(dom_id) = node.dom_node_id {
                let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                debug_info!(
                    self.ctx,
                    "Painting inline content for node {} ({:?}) at {:?}, {} layout items",
                    node_index,
                    node_type.get_node_type(),
                    paint_rect,
                    inline_layout.items.len()
                );
            }

            // paint_rect is the border-box, but inline layout positions are relative to
            // content-box. Use type-safe conversion to make this clear and avoid manual
            // calculations.
            let border_box = BorderBoxRect(paint_rect);
            let mut content_box_rect =
                border_box.to_content_box(&node.box_props.padding, &node.box_props.border).rect();
            
            // For scrollable containers, extend the content rect to the full content size.
            // The scroll frame handles clipping - we need to paint ALL content, not just
            // what fits in the viewport. Otherwise glyphs beyond the viewport are not rendered.
            let content_size = get_scroll_content_size(node);
            if content_size.height > content_box_rect.size.height {
                content_box_rect.size.height = content_size.height;
            }
            if content_size.width > content_box_rect.size.width {
                content_box_rect.size.width = content_size.width;
            }

            self.paint_inline_content(builder, content_box_rect, inline_layout)?;
        } else if let Some(dom_id) = node.dom_node_id {
            // This node might be a simple replaced element, like an <img> tag.
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            if let NodeType::Image(image_ref) = node_data.get_node_type() {
                debug_info!(
                    self.ctx,
                    "Painting image for node {} at {:?}",
                    node_index,
                    paint_rect
                );
                // Store the ImageRef directly in the display list
                builder.push_image(paint_rect, image_ref.clone());
            }
        }

        Ok(())
    }

    /// Emits drawing commands for scrollbars. This is called AFTER popping the scroll frame
    /// clip so scrollbars appear on top of content and are not clipped.
    fn paint_scrollbars(&self, builder: &mut DisplayListBuilder, node_index: usize) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;

        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // Check if we need to draw scrollbars for this node.
        let scrollbar_info = get_scrollbar_info_from_layout(node);

        // Get node_id for GPU cache lookup and CSS style lookup
        let node_id = node.dom_node_id;

        // Get CSS scrollbar style for this node
        let scrollbar_style = node_id
            .map(|nid| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
                get_scrollbar_style(self.ctx.styled_dom, nid, node_state)
            })
            .unwrap_or_default();

        // Skip if scrollbar-width: none
        if matches!(
            scrollbar_style.width_mode,
            azul_css::props::style::scrollbar::LayoutScrollbarWidth::None
        ) {
            return Ok(());
        }

        // Get border dimensions to position scrollbar inside the border-box
        let border = &node.box_props.border;

        // Get border-radius for potential clipping
        let container_border_radius = node_id
            .map(|nid| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
                let element_size = PhysicalSizeImport {
                    width: paint_rect.size.width,
                    height: paint_rect.size.height,
                };
                let viewport_size =
                    LogicalSize::new(self.ctx.viewport_size.width, self.ctx.viewport_size.height);
                get_border_radius(
                    self.ctx.styled_dom,
                    nid,
                    node_state,
                    element_size,
                    viewport_size,
                )
            })
            .unwrap_or_default();

        // Calculate the inner rect (content-box) where scrollbars should be placed
        // Scrollbars are positioned inside the border, at the right/bottom edges
        let inner_rect = LogicalRect {
            origin: LogicalPosition::new(
                paint_rect.origin.x + border.left,
                paint_rect.origin.y + border.top,
            ),
            size: LogicalSize::new(
                (paint_rect.size.width - border.left - border.right).max(0.0),
                (paint_rect.size.height - border.top - border.bottom).max(0.0),
            ),
        };

        // Get scroll position for thumb calculation
        // ScrollPosition contains parent_rect and children_rect
        // The scroll offset is the difference between children_rect.origin and parent_rect.origin
        let (scroll_offset_x, scroll_offset_y) = node_id
            .and_then(|nid| {
                self.scroll_offsets.get(&nid).map(|pos| {
                    (
                        pos.children_rect.origin.x - pos.parent_rect.origin.x,
                        pos.children_rect.origin.y - pos.parent_rect.origin.y,
                    )
                })
            })
            .unwrap_or((0.0, 0.0));

        // Get content size for thumb proportional sizing
        // Use the node's get_content_size() method which returns the actual content size
        // from overflow_content_size (set during layout) or computes it from text/children.
        // This is critical for correct thumb sizing - we must NOT use arbitrary multipliers.
        let content_size = node.get_content_size();

        // Calculate thumb border-radius (half the scrollbar width for pill-shaped thumb)
        let thumb_radius = scrollbar_style.width_px / 2.0;
        let thumb_border_radius = BorderRadius {
            top_left: thumb_radius,
            top_right: thumb_radius,
            bottom_left: thumb_radius,
            bottom_right: thumb_radius,
        };

        if scrollbar_info.needs_vertical {
            // Look up opacity key from GPU cache
            let opacity_key = node_id.and_then(|nid| {
                self.gpu_value_cache.and_then(|cache| {
                    cache
                        .scrollbar_v_opacity_keys
                        .get(&(self.dom_id, nid))
                        .copied()
                })
            });

            // Vertical scrollbar: positioned at the right edge of the inner rect
            let track_height = if scrollbar_info.needs_horizontal {
                inner_rect.size.height - scrollbar_style.width_px
            } else {
                inner_rect.size.height
            };

            let track_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    inner_rect.origin.x + inner_rect.size.width - scrollbar_style.width_px,
                    inner_rect.origin.y,
                ),
                size: LogicalSize::new(scrollbar_style.width_px, track_height),
            };

            // Calculate thumb size and position
            let viewport_height = inner_rect.size.height;
            let thumb_ratio = (viewport_height / content_size.height).min(1.0);
            let thumb_height = (track_height * thumb_ratio).max(scrollbar_style.width_px * 2.0);

            let max_scroll = (content_size.height - viewport_height).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 {
                scroll_offset_y.abs() / max_scroll
            } else {
                0.0
            };
            let thumb_y = track_bounds.origin.y
                + (track_height - thumb_height) * scroll_ratio.clamp(0.0, 1.0);

            let thumb_bounds = LogicalRect {
                origin: LogicalPosition::new(track_bounds.origin.x, thumb_y),
                size: LogicalSize::new(scrollbar_style.width_px, thumb_height),
            };

            // Generate hit-test ID for vertical scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::VerticalThumb(self.dom_id, nid));

            // Add page-scroll buttons at top/bottom of scrollbar track (green for debug)
            let button_size = scrollbar_style.width_px;
            let button_decrement_bounds = Some(LogicalRect {
                origin: LogicalPosition::new(track_bounds.origin.x, track_bounds.origin.y),
                size: LogicalSize::new(button_size, button_size),
            });
            let button_increment_bounds = Some(LogicalRect {
                origin: LogicalPosition::new(
                    track_bounds.origin.x,
                    track_bounds.origin.y + track_height - button_size,
                ),
                size: LogicalSize::new(button_size, button_size),
            });
            // Light green color for debug visibility
            let debug_button_color = ColorU { r: 144, g: 238, b: 144, a: 255 };

            builder.push_scrollbar_styled(ScrollbarDrawInfo {
                bounds: track_bounds,
                orientation: ScrollbarOrientation::Vertical,
                track_bounds,
                track_color: scrollbar_style.track_color,
                thumb_bounds,
                thumb_color: scrollbar_style.thumb_color,
                thumb_border_radius,
                button_decrement_bounds,
                button_increment_bounds,
                button_color: debug_button_color,
                opacity_key,
                hit_id,
                clip_to_container_border: scrollbar_style.clip_to_container_border,
                container_border_radius,
            });
        }

        if scrollbar_info.needs_horizontal {
            // Look up opacity key from GPU cache
            let opacity_key = node_id.and_then(|nid| {
                self.gpu_value_cache.and_then(|cache| {
                    cache
                        .scrollbar_h_opacity_keys
                        .get(&(self.dom_id, nid))
                        .copied()
                })
            });

            // Horizontal scrollbar: positioned at the bottom edge of the inner rect
            let track_width = if scrollbar_info.needs_vertical {
                inner_rect.size.width - scrollbar_style.width_px
            } else {
                inner_rect.size.width
            };

            let track_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    inner_rect.origin.x,
                    inner_rect.origin.y + inner_rect.size.height - scrollbar_style.width_px,
                ),
                size: LogicalSize::new(track_width, scrollbar_style.width_px),
            };

            // Calculate thumb size and position
            let viewport_width = inner_rect.size.width;
            let thumb_ratio = (viewport_width / content_size.width).min(1.0);
            let thumb_width = (track_width * thumb_ratio).max(scrollbar_style.width_px * 2.0);

            let max_scroll = (content_size.width - viewport_width).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 {
                scroll_offset_x.abs() / max_scroll
            } else {
                0.0
            };
            let thumb_x = track_bounds.origin.x
                + (track_width - thumb_width) * scroll_ratio.clamp(0.0, 1.0);

            let thumb_bounds = LogicalRect {
                origin: LogicalPosition::new(thumb_x, track_bounds.origin.y),
                size: LogicalSize::new(thumb_width, scrollbar_style.width_px),
            };

            // Generate hit-test ID for horizontal scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::HorizontalThumb(self.dom_id, nid));

            // Add page-scroll buttons at left/right of scrollbar track (green for debug)
            let button_size = scrollbar_style.width_px;
            let button_decrement_bounds = Some(LogicalRect {
                origin: LogicalPosition::new(track_bounds.origin.x, track_bounds.origin.y),
                size: LogicalSize::new(button_size, button_size),
            });
            let button_increment_bounds = Some(LogicalRect {
                origin: LogicalPosition::new(
                    track_bounds.origin.x + track_width - button_size,
                    track_bounds.origin.y,
                ),
                size: LogicalSize::new(button_size, button_size),
            });
            // Light green color for debug visibility
            let debug_button_color = ColorU { r: 144, g: 238, b: 144, a: 255 };

            builder.push_scrollbar_styled(ScrollbarDrawInfo {
                bounds: track_bounds,
                orientation: ScrollbarOrientation::Horizontal,
                track_bounds,
                track_color: scrollbar_style.track_color,
                thumb_bounds,
                thumb_color: scrollbar_style.thumb_color,
                thumb_border_radius,
                button_decrement_bounds,
                button_increment_bounds,
                button_color: debug_button_color,
                opacity_key,
                hit_id,
                clip_to_container_border: scrollbar_style.clip_to_container_border,
                container_border_radius,
            });
        }

        Ok(())
    }

    /// Converts the rich layout information from `text3` into drawing commands.
    fn paint_inline_content(
        &self,
        builder: &mut DisplayListBuilder,
        container_rect: LogicalRect,
        layout: &UnifiedLayout,
    ) -> Result<()> {
        // TODO: This will always paint images over the glyphs
        // TODO: Handle z-index within inline content (e.g. background images)
        // TODO: Handle text decorations (underline, strikethrough, etc.)
        // TODO: Handle text shadows
        // TODO: Handle text overflowing (based on container_rect and overflow behavior)

        // Push the TextLayout item FIRST, containing the full UnifiedLayout for PDF/accessibility
        // This provides complete metadata including original text and glyph-to-unicode mapping
        builder.push_text_layout(
            Arc::new(layout.clone()) as Arc<dyn std::any::Any + Send + Sync>,
            container_rect,
            FontHash::from_hash(0), // Will be updated per glyph run
            12.0,                   // Default font size, will be updated per glyph run
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }, // Default color
        );

        let glyph_runs = crate::text3::glyphs::get_glyph_runs_simple(layout);

        // FIRST PASS: Render backgrounds (solid colors, gradients) and borders for each glyph run
        // This must happen BEFORE rendering text so that backgrounds appear behind text.
        for glyph_run in glyph_runs.iter() {
            // Calculate the bounding box for this glyph run
            if let (Some(first_glyph), Some(last_glyph)) =
                (glyph_run.glyphs.first(), glyph_run.glyphs.last())
            {
                // Calculate run bounds from glyph positions
                let run_start_x = container_rect.origin.x + first_glyph.point.x;
                let run_end_x = container_rect.origin.x + last_glyph.point.x;
                let run_width = (run_end_x - run_start_x).max(0.0);

                // Skip if run has no width
                if run_width <= 0.0 {
                    continue;
                }

                // Approximate height based on font size (baseline is at glyph.point.y)
                let baseline_y = container_rect.origin.y + first_glyph.point.y;
                let font_size = glyph_run.font_size_px;
                let ascent = font_size * 0.8; // Approximate ascent

                let run_bounds = LogicalRect::new(
                    LogicalPosition::new(run_start_x, baseline_y - ascent),
                    LogicalSize::new(run_width, font_size),
                );

                // Use unified inline background/border painting
                builder.push_inline_backgrounds_and_border(
                    run_bounds,
                    glyph_run.background_color,
                    &glyph_run.background_content,
                    glyph_run.border.as_ref(),
                );
            }
        }

        // SECOND PASS: Render text runs
        for (idx, glyph_run) in glyph_runs.iter().enumerate() {
            let clip_rect = container_rect; // Clip to the container rect

            // Fix: Offset glyph positions by the container origin.
            // Text layout is relative to (0,0) of the IFC, but we need absolute coordinates.
            let offset_glyphs: Vec<GlyphInstance> = glyph_run
                .glyphs
                .iter()
                .map(|g| {
                    let mut g = g.clone();
                    g.point.x += container_rect.origin.x;
                    g.point.y += container_rect.origin.y;
                    g
                })
                .collect();

            // Store only the font hash in the display list to keep it lean
            builder.push_text_run(
                offset_glyphs,
                FontHash::from_hash(glyph_run.font_hash),
                glyph_run.font_size_px,
                glyph_run.color,
                clip_rect,
            );

            // Render text decorations if present OR if this is IME composition preview
            let needs_underline = glyph_run.text_decoration.underline || glyph_run.is_ime_preview;
            let needs_strikethrough = glyph_run.text_decoration.strikethrough;
            let needs_overline = glyph_run.text_decoration.overline;

            if needs_underline || needs_strikethrough || needs_overline {
                // Calculate the bounding box for this glyph run
                if let (Some(first_glyph), Some(last_glyph)) =
                    (glyph_run.glyphs.first(), glyph_run.glyphs.last())
                {
                    let decoration_start_x = container_rect.origin.x + first_glyph.point.x;
                    let decoration_end_x = container_rect.origin.x + last_glyph.point.x;
                    let decoration_width = decoration_end_x - decoration_start_x;

                    // Use font metrics to determine decoration positions
                    // Standard ratios based on CSS specification
                    let font_size = glyph_run.font_size_px;
                    let thickness = (font_size * 0.08).max(1.0); // ~8% of font size, min 1px

                    // Baseline is at glyph.point.y
                    let baseline_y = container_rect.origin.y + first_glyph.point.y;

                    if needs_underline {
                        // Underline is typically 10-15% below baseline
                        // IME composition always gets underlined
                        let underline_y = baseline_y + (font_size * 0.12);
                        let underline_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, underline_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_underline(underline_bounds, glyph_run.color, thickness);
                    }

                    if needs_strikethrough {
                        // Strikethrough is typically 40% above baseline (middle of x-height)
                        let strikethrough_y = baseline_y - (font_size * 0.3);
                        let strikethrough_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, strikethrough_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_strikethrough(
                            strikethrough_bounds,
                            glyph_run.color,
                            thickness,
                        );
                    }

                    if needs_overline {
                        // Overline is typically at cap-height (75% above baseline)
                        let overline_y = baseline_y - (font_size * 0.85);
                        let overline_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, overline_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_overline(overline_bounds, glyph_run.color, thickness);
                    }
                }
            }
        }

        // THIRD PASS: Generate hit-test areas for text runs
        // This enables cursor resolution directly on text nodes instead of their containers
        for glyph_run in glyph_runs.iter() {
            // Only generate hit-test areas for runs with a source node id
            let Some(source_node_id) = glyph_run.source_node_id else {
                continue;
            };

            // Calculate the bounding box for this glyph run
            if let (Some(first_glyph), Some(last_glyph)) =
                (glyph_run.glyphs.first(), glyph_run.glyphs.last())
            {
                let run_start_x = container_rect.origin.x + first_glyph.point.x;
                let run_end_x = container_rect.origin.x + last_glyph.point.x;
                let run_width = (run_end_x - run_start_x).max(0.0);

                // Skip if run has no width
                if run_width <= 0.0 {
                    continue;
                }

                // Calculate run bounds using font metrics
                let baseline_y = container_rect.origin.y + first_glyph.point.y;
                let font_size = glyph_run.font_size_px;
                let ascent = font_size * 0.8; // Approximate ascent

                let run_bounds = LogicalRect::new(
                    LogicalPosition::new(run_start_x, baseline_y - ascent),
                    LogicalSize::new(run_width, font_size),
                );

                // Query the cursor type for this text node from the CSS property cache
                // Default to Text cursor (I-beam) for text nodes
                let cursor_type = self.get_cursor_type_for_text_node(source_node_id);

                // Construct the hit-test tag for cursor resolution
                // tag.0 = DomId (upper 32 bits) | NodeId (lower 32 bits)
                // tag.1 = TAG_TYPE_CURSOR | cursor_type
                let tag_value = ((self.dom_id.inner as u64) << 32) | (source_node_id.index() as u64);
                let tag_type = TAG_TYPE_CURSOR | (cursor_type as u16);
                let tag_id = (tag_value, tag_type);

                builder.push_hit_test_area(run_bounds, tag_id);
            }
        }

        // Render inline objects (images, shapes/inline-blocks, etc.)
        // These are positioned by the text3 engine and need to be rendered at their calculated
        // positions
        for positioned_item in &layout.items {
            self.paint_inline_object(builder, container_rect.origin, positioned_item)?;
        }
        Ok(())
    }

    /// Paints a single inline object (image, shape, or inline-block)
    fn paint_inline_object(
        &self,
        builder: &mut DisplayListBuilder,
        base_pos: LogicalPosition,
        positioned_item: &PositionedItem,
    ) -> Result<()> {
        let ShapedItem::Object {
            content, bounds, ..
        } = &positioned_item.item
        else {
            // Other item types (e.g., breaks) don't produce painted output.
            return Ok(());
        };

        // Calculate the absolute position of this object
        // positioned_item.position is relative to the container
        let object_bounds = LogicalRect::new(
            LogicalPosition::new(
                base_pos.x + positioned_item.position.x,
                base_pos.y + positioned_item.position.y,
            ),
            LogicalSize::new(bounds.width, bounds.height),
        );

        match content {
            InlineContent::Image(image) => {
                if let Some(image_ref) = get_image_ref_for_image_source(&image.source) {
                    builder.push_image(object_bounds, image_ref);
                }
            }
            InlineContent::Shape(shape) => {
                self.paint_inline_shape(builder, object_bounds, shape, bounds)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Paints an inline shape (inline-block background and border)
    fn paint_inline_shape(
        &self,
        builder: &mut DisplayListBuilder,
        object_bounds: LogicalRect,
        shape: &InlineShape,
        bounds: &crate::text3::cache::Rect,
    ) -> Result<()> {
        // Render inline-block backgrounds and borders using their CSS styling
        // The text3 engine positions these correctly in the inline flow
        let Some(node_id) = shape.source_node_id else {
            return Ok(());
        };

        let styled_node_state =
            &self.ctx.styled_dom.styled_nodes.as_container()[node_id].styled_node_state;

        // Get all background layers (colors, gradients, images)
        let background_contents =
            get_background_contents(self.ctx.styled_dom, node_id, styled_node_state);

        // Get border information
        let border_info = get_border_info(self.ctx.styled_dom, node_id, styled_node_state);

        // FIX: object_bounds is the margin-box position from text3.
        // We need to convert to border-box for painting backgrounds/borders.
        let margins = if let Some(indices) = self.positioned_tree.tree.dom_to_layout.get(&node_id) {
            if let Some(&idx) = indices.first() {
                self.positioned_tree.tree.nodes[idx].box_props.margin
            } else {
                Default::default()
            }
        } else {
            Default::default()
        };

        // Convert margin-box bounds to border-box bounds
        let border_box_bounds = LogicalRect {
            origin: LogicalPosition {
                x: object_bounds.origin.x + margins.left,
                y: object_bounds.origin.y + margins.top,
            },
            size: LogicalSize {
                width: (object_bounds.size.width - margins.left - margins.right).max(0.0),
                height: (object_bounds.size.height - margins.top - margins.bottom).max(0.0),
            },
        };

        let element_size = PhysicalSizeImport {
            width: border_box_bounds.size.width,
            height: border_box_bounds.size.height,
        };

        // Get border radius for background clipping
        let simple_border_radius = get_border_radius(
            self.ctx.styled_dom,
            node_id,
            styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        // Get style border radius for border rendering
        let style_border_radius =
            get_style_border_radius(self.ctx.styled_dom, node_id, styled_node_state);

        // Use unified background/border painting with border-box bounds
        builder.push_backgrounds_and_border(
            border_box_bounds,
            &background_contents,
            &border_info,
            simple_border_radius,
            style_border_radius,
        );

        // Push hit-test area for this inline-block element
        // This is critical for buttons and other inline-block elements to receive
        // mouse events and display the correct cursor (e.g., cursor: pointer)
        if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, Some(node_id)) {
            builder.push_hit_test_area(border_box_bounds, tag_id);
        }

        Ok(())
    }

    /// Determines if a node establishes a new stacking context based on CSS rules.
    fn establishes_stacking_context(&self, node_index: usize) -> bool {
        let Some(node) = self.positioned_tree.tree.get(node_index) else {
            return false;
        };
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };

        let position = get_position_type(self.ctx.styled_dom, Some(dom_id));
        if position == LayoutPosition::Absolute || position == LayoutPosition::Fixed {
            return true;
        }

        let z_index = get_z_index(self.ctx.styled_dom, Some(dom_id));
        if position == LayoutPosition::Relative && z_index != 0 {
            return true;
        }

        if let Some(styled_node) = self.ctx.styled_dom.styled_nodes.as_container().get(dom_id) {
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Opacity < 1
            let opacity = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_opacity(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
                .map(|v| v.inner.normalized())
                .unwrap_or(1.0);

            if opacity < 1.0 {
                return true;
            }

            // Transform != none
            let has_transform = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_transform(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
                .map(|v| !v.is_empty())
                .unwrap_or(false);

            if has_transform {
                return true;
            }
        }

        false
    }
}

/// Helper struct to pass layout results to the display list generator.
///
/// Combines the layout tree with pre-calculated absolute positions for each node.
/// The positions are stored separately because they are computed in a final
/// positioning pass after layout is complete.
pub struct PositionedTree<'a> {
    /// The layout tree containing all nodes with their computed sizes
    pub tree: &'a LayoutTree,
    /// Map from node index to its absolute position in the document
    pub calculated_positions: &'a BTreeMap<usize, LogicalPosition>,
}

/// Describes how overflow content should be handled for an element.
///
/// This maps to the CSS `overflow-x` and `overflow-y` properties and determines
/// whether content that exceeds the element's bounds should be visible, clipped,
/// or scrollable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    /// Content is not clipped and may render outside the element's box (default)
    Visible,
    /// Content is clipped to the padding box, no scrollbars provided
    Hidden,
    /// Content is clipped to the padding box (CSS `overflow: clip`)
    Clip,
    /// Content is clipped and scrollbars are always shown
    Scroll,
    /// Content is clipped and scrollbars appear only when needed
    Auto,
}

impl OverflowBehavior {
    /// Returns `true` if this overflow behavior clips content.
    ///
    /// All behaviors except `Visible` result in content being clipped
    /// to the element's padding box.
    pub fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    /// Returns `true` if this overflow behavior enables scrolling.
    ///
    /// Only `Scroll` and `Auto` allow the user to scroll to see
    /// overflowing content.
    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
}

fn get_scroll_id(id: Option<NodeId>) -> LocalScrollId {
    id.map(|i| i.index() as u64).unwrap_or(0)
}

/// Calculates the actual content size of a node, including all children and text.
/// This is used to determine if scrollbars should appear for overflow: auto.
fn get_scroll_content_size(node: &LayoutNode) -> LogicalSize {
    // First check if we have a pre-calculated overflow_content_size (for block children)
    if let Some(overflow_size) = node.overflow_content_size {
        return overflow_size;
    }

    // Start with the node's own size
    let mut content_size = node.used_size.unwrap_or_default();

    // If this node has text layout, calculate the bounds of all text items
    if let Some(ref cached_layout) = node.inline_layout_result {
        let text_layout = &cached_layout.layout;
        // Find the maximum extent of all positioned items
        let mut max_x: f32 = 0.0;
        let mut max_y: f32 = 0.0;

        for positioned_item in &text_layout.items {
            let item_bounds = positioned_item.item.bounds();
            let item_right = positioned_item.position.x + item_bounds.width;
            let item_bottom = positioned_item.position.y + item_bounds.height;

            max_x = max_x.max(item_right);
            max_y = max_y.max(item_bottom);
        }

        // Use the maximum extent as content size if it's larger
        content_size.width = content_size.width.max(max_x);
        content_size.height = content_size.height.max(max_y);
    }

    content_size
}

fn get_tag_id(dom: &StyledDom, id: Option<NodeId>) -> Option<DisplayListTagId> {
    let node_id = id?;
    let styled_nodes = dom.styled_nodes.as_container();
    let styled_node = styled_nodes.get(node_id)?;
    let tag_id = styled_node.tag_id.into_option()?;
    // Use TAG_TYPE_DOM_NODE (0x0100) as namespace marker in u16 field
    // This distinguishes DOM nodes from scrollbars (0x0200) and other tag types
    Some((tag_id.inner, 0x0100))
}

fn get_image_ref_for_image_source(
    source: &ImageSource,
) -> Option<ImageRef> {
    match source {
        ImageSource::Ref(image_ref) => Some(image_ref.clone()),
        ImageSource::Url(_url) => {
            // TODO: Look up in ImageCache
            // For now, CSS url() images are not yet supported
            None
        }
        ImageSource::Data(_) | ImageSource::Svg(_) | ImageSource::Placeholder(_) => {
            // TODO: Decode raw data / SVG to ImageRef
            None
        }
    }
}

/// Get the bounds of a display list item, if it has spatial extent.
fn get_display_item_bounds(item: &DisplayListItem) -> Option<LogicalRect> {
    match item {
        DisplayListItem::Rect { bounds, .. } => Some(*bounds),
        DisplayListItem::SelectionRect { bounds, .. } => Some(*bounds),
        DisplayListItem::CursorRect { bounds, .. } => Some(*bounds),
        DisplayListItem::Border { bounds, .. } => Some(*bounds),
        DisplayListItem::TextLayout { bounds, .. } => Some(*bounds),
        DisplayListItem::Text { clip_rect, .. } => Some(*clip_rect),
        DisplayListItem::Underline { bounds, .. } => Some(*bounds),
        DisplayListItem::Strikethrough { bounds, .. } => Some(*bounds),
        DisplayListItem::Overline { bounds, .. } => Some(*bounds),
        DisplayListItem::Image { bounds, .. } => Some(*bounds),
        DisplayListItem::ScrollBar { bounds, .. } => Some(*bounds),
        DisplayListItem::ScrollBarStyled { info } => Some(info.bounds),
        DisplayListItem::PushClip { bounds, .. } => Some(*bounds),
        DisplayListItem::PushScrollFrame { clip_bounds, .. } => Some(*clip_bounds),
        DisplayListItem::HitTestArea { bounds, .. } => Some(*bounds),
        DisplayListItem::PushStackingContext { bounds, .. } => Some(*bounds),
        DisplayListItem::IFrame { bounds, .. } => Some(*bounds),
        _ => None,
    }
}

/// Clip a display list item to page bounds and offset to page-relative coordinates.
/// Returns None if the item is completely outside the page bounds.
fn clip_and_offset_display_item(
    item: &DisplayListItem,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    match item {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => clip_rect_item(*bounds, *color, *border_radius, page_top, page_bottom),

        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => clip_border_item(
            *bounds,
            *widths,
            *colors,
            *styles,
            border_radius.clone(),
            page_top,
            page_bottom,
        ),

        DisplayListItem::SelectionRect {
            bounds,
            border_radius,
            color,
        } => clip_selection_rect_item(*bounds, *border_radius, *color, page_top, page_bottom),

        DisplayListItem::CursorRect { bounds, color } => {
            clip_cursor_rect_item(*bounds, *color, page_top, page_bottom)
        }

        DisplayListItem::Image { bounds, image } => {
            clip_image_item(*bounds, image.clone(), page_top, page_bottom)
        }

        DisplayListItem::TextLayout {
            layout,
            bounds,
            font_hash,
            font_size_px,
            color,
        } => clip_text_layout_item(
            layout,
            *bounds,
            *font_hash,
            *font_size_px,
            *color,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px,
            color,
            clip_rect,
        } => clip_text_item(
            glyphs,
            *font_hash,
            *font_size_px,
            *color,
            *clip_rect,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Underline {
            bounds,
            color,
            thickness,
        } => clip_text_decoration_item(
            *bounds,
            *color,
            *thickness,
            TextDecorationType::Underline,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Strikethrough {
            bounds,
            color,
            thickness,
        } => clip_text_decoration_item(
            *bounds,
            *color,
            *thickness,
            TextDecorationType::Strikethrough,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Overline {
            bounds,
            color,
            thickness,
        } => clip_text_decoration_item(
            *bounds,
            *color,
            *thickness,
            TextDecorationType::Overline,
            page_top,
            page_bottom,
        ),

        DisplayListItem::ScrollBar {
            bounds,
            color,
            orientation,
            opacity_key,
            hit_id,
        } => clip_scrollbar_item(
            *bounds,
            *color,
            *orientation,
            *opacity_key,
            *hit_id,
            page_top,
            page_bottom,
        ),

        DisplayListItem::HitTestArea { bounds, tag } => {
            clip_hit_test_area_item(*bounds, *tag, page_top, page_bottom)
        }

        DisplayListItem::IFrame {
            child_dom_id,
            bounds,
            clip_rect,
        } => clip_iframe_item(*child_dom_id, *bounds, *clip_rect, page_top, page_bottom),

        // ScrollBarStyled - clip based on overall bounds
        DisplayListItem::ScrollBarStyled { info } => {
            let bounds = info.bounds;
            if bounds.origin.y + bounds.size.height < page_top || bounds.origin.y > page_bottom {
                None
            } else {
                // Clone and offset all the internal bounds
                let mut clipped_info = (**info).clone();
                let y_offset = -page_top;
                clipped_info.bounds = offset_rect_y(clipped_info.bounds, y_offset);
                clipped_info.track_bounds = offset_rect_y(clipped_info.track_bounds, y_offset);
                clipped_info.thumb_bounds = offset_rect_y(clipped_info.thumb_bounds, y_offset);
                if let Some(b) = clipped_info.button_decrement_bounds {
                    clipped_info.button_decrement_bounds = Some(offset_rect_y(b, y_offset));
                }
                if let Some(b) = clipped_info.button_increment_bounds {
                    clipped_info.button_increment_bounds = Some(offset_rect_y(b, y_offset));
                }
                Some(DisplayListItem::ScrollBarStyled {
                    info: Box::new(clipped_info),
                })
            }
        }

        // State management items - skip for now (would need proper per-page tracking)
        DisplayListItem::PushClip { .. }
        | DisplayListItem::PopClip
        | DisplayListItem::PushScrollFrame { .. }
        | DisplayListItem::PopScrollFrame
        | DisplayListItem::PushStackingContext { .. }
        | DisplayListItem::PopStackingContext => None,

        // Gradient items - simple bounds check
        DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            if bounds.origin.y + bounds.size.height < page_top || bounds.origin.y > page_bottom {
                None
            } else {
                Some(DisplayListItem::LinearGradient {
                    bounds: offset_rect_y(*bounds, -page_top),
                    gradient: gradient.clone(),
                    border_radius: *border_radius,
                })
            }
        }
        DisplayListItem::RadialGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            if bounds.origin.y + bounds.size.height < page_top || bounds.origin.y > page_bottom {
                None
            } else {
                Some(DisplayListItem::RadialGradient {
                    bounds: offset_rect_y(*bounds, -page_top),
                    gradient: gradient.clone(),
                    border_radius: *border_radius,
                })
            }
        }
        DisplayListItem::ConicGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            if bounds.origin.y + bounds.size.height < page_top || bounds.origin.y > page_bottom {
                None
            } else {
                Some(DisplayListItem::ConicGradient {
                    bounds: offset_rect_y(*bounds, -page_top),
                    gradient: gradient.clone(),
                    border_radius: *border_radius,
                })
            }
        }

        // BoxShadow - simple bounds check
        DisplayListItem::BoxShadow {
            bounds,
            shadow,
            border_radius,
        } => {
            if bounds.origin.y + bounds.size.height < page_top || bounds.origin.y > page_bottom {
                None
            } else {
                Some(DisplayListItem::BoxShadow {
                    bounds: offset_rect_y(*bounds, -page_top),
                    shadow: *shadow,
                    border_radius: *border_radius,
                })
            }
        }

        // Filter effects - skip for now (would need proper per-page tracking)
        DisplayListItem::PushFilter { .. }
        | DisplayListItem::PopFilter
        | DisplayListItem::PushBackdropFilter { .. }
        | DisplayListItem::PopBackdropFilter
        | DisplayListItem::PushOpacity { .. }
        | DisplayListItem::PopOpacity => None,
    }
}

// Helper functions for clip_and_offset_display_item

/// Internal enum for text decoration type dispatch
#[derive(Debug, Clone, Copy)]
enum TextDecorationType {
    Underline,
    Strikethrough,
    Overline,
}

/// Clips a filled rectangle to page bounds.
fn clip_rect_item(
    bounds: LogicalRect,
    color: ColorU,
    border_radius: BorderRadius,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::Rect {
        bounds: clipped,
        color,
        border_radius,
    })
}

/// Clips a border to page bounds, hiding top/bottom borders when clipped.
fn clip_border_item(
    bounds: LogicalRect,
    widths: StyleBorderWidths,
    colors: StyleBorderColors,
    styles: StyleBorderStyles,
    border_radius: StyleBorderRadius,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    let original_bounds = bounds;
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| {
        let new_widths = adjust_border_widths_for_clipping(
            widths,
            original_bounds,
            clipped,
            page_top,
            page_bottom,
        );
        DisplayListItem::Border {
            bounds: clipped,
            widths: new_widths,
            colors,
            styles,
            border_radius,
        }
    })
}

/// Adjusts border widths when a border is clipped at page boundaries.
/// Hides top border if clipped at top, bottom border if clipped at bottom.
fn adjust_border_widths_for_clipping(
    mut widths: StyleBorderWidths,
    original_bounds: LogicalRect,
    clipped: LogicalRect,
    page_top: f32,
    page_bottom: f32,
) -> StyleBorderWidths {
    // Hide top border if we clipped the top
    if clipped.origin.y > 0.0 && original_bounds.origin.y < page_top {
        widths.top = None;
    }

    // Hide bottom border if we clipped the bottom
    let original_bottom = original_bounds.origin.y + original_bounds.size.height;
    let clipped_bottom = clipped.origin.y + clipped.size.height;
    if original_bottom > page_bottom && clipped_bottom >= page_bottom - page_top - 1.0 {
        widths.bottom = None;
    }

    widths
}

/// Clips a selection rectangle to page bounds.
fn clip_selection_rect_item(
    bounds: LogicalRect,
    border_radius: BorderRadius,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::SelectionRect {
        bounds: clipped,
        border_radius,
        color,
    })
}

/// Clips a cursor rectangle to page bounds.
fn clip_cursor_rect_item(
    bounds: LogicalRect,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::CursorRect {
        bounds: clipped,
        color,
    })
}

/// Clips an image to page bounds if it overlaps the page.
fn clip_image_item(
    bounds: LogicalRect,
    image: ImageRef,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    if !rect_intersects(&bounds, page_top, page_bottom) {
        return None;
    }
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::Image {
        bounds: clipped,
        image,
    })
}

/// Clips a text layout block to page bounds, filtering individual text items.
fn clip_text_layout_item(
    layout: &Arc<dyn std::any::Any + Send + Sync>,
    bounds: LogicalRect,
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    if !rect_intersects(&bounds, page_top, page_bottom) {
        return None;
    }

    // Try to downcast and filter UnifiedLayout items
    #[cfg(feature = "text_layout")]
    if let Some(unified_layout) = layout.downcast_ref::<crate::text3::cache::UnifiedLayout>() {
        return clip_unified_layout(
            unified_layout,
            bounds,
            font_hash,
            font_size_px,
            color,
            page_top,
            page_bottom,
        );
    }

    // Fallback: simple bounds offset (legacy behavior)
    Some(DisplayListItem::TextLayout {
        layout: layout.clone(),
        bounds: offset_rect_y(bounds, -page_top),
        font_hash,
        font_size_px,
        color,
    })
}

/// Clips a UnifiedLayout by filtering items to those on the current page.
#[cfg(feature = "text_layout")]
fn clip_unified_layout(
    unified_layout: &crate::text3::cache::UnifiedLayout,
    bounds: LogicalRect,
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    let layout_origin_y = bounds.origin.y;
    let layout_origin_x = bounds.origin.x;

    // Filter items whose center falls within this page
    let filtered_items: Vec<_> = unified_layout
        .items
        .iter()
        .filter(|item| item_center_on_page(item, layout_origin_y, page_top, page_bottom))
        .cloned()
        .collect();

    if filtered_items.is_empty() {
        return None;
    }

    // Calculate new origin for page-relative positioning
    let new_origin_y = (layout_origin_y - page_top).max(0.0);

    // Transform items to page-relative coordinates and calculate bounds
    let (offset_items, min_y, max_y, max_width) =
        transform_items_to_page_coords(filtered_items, layout_origin_y, page_top, new_origin_y);

    let new_layout = crate::text3::cache::UnifiedLayout {
        items: offset_items,
        overflow: unified_layout.overflow.clone(),
    };

    let new_bounds = LogicalRect {
        origin: LogicalPosition {
            x: layout_origin_x,
            y: new_origin_y,
        },
        size: LogicalSize {
            width: max_width.max(bounds.size.width),
            height: (max_y - min_y.min(0.0)).max(0.0),
        },
    };

    Some(DisplayListItem::TextLayout {
        layout: Arc::new(new_layout) as Arc<dyn std::any::Any + Send + Sync>,
        bounds: new_bounds,
        font_hash,
        font_size_px,
        color,
    })
}

/// Checks if an item's center point falls within the page bounds.
#[cfg(feature = "text_layout")]
fn item_center_on_page(
    item: &crate::text3::cache::PositionedItem,
    layout_origin_y: f32,
    page_top: f32,
    page_bottom: f32,
) -> bool {
    let item_y_absolute = layout_origin_y + item.position.y;
    let item_height = item.item.bounds().height;
    let item_center_y = item_y_absolute + (item_height / 2.0);
    item_center_y >= page_top && item_center_y < page_bottom
}

/// Transforms filtered items to page-relative coordinates.
/// Returns (items, min_y, max_y, max_width).
#[cfg(feature = "text_layout")]
fn transform_items_to_page_coords(
    items: Vec<crate::text3::cache::PositionedItem>,
    layout_origin_y: f32,
    page_top: f32,
    new_origin_y: f32,
) -> (Vec<crate::text3::cache::PositionedItem>, f32, f32, f32) {
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    let mut max_width = 0.0f32;

    let offset_items: Vec<_> = items
        .into_iter()
        .map(|mut item| {
            let abs_y = layout_origin_y + item.position.y;
            let page_y = abs_y - page_top;
            let new_item_y = page_y - new_origin_y;

            let item_bounds = item.item.bounds();
            min_y = min_y.min(new_item_y);
            max_y = max_y.max(new_item_y + item_bounds.height);
            max_width = max_width.max(item.position.x + item_bounds.width);

            item.position.y = new_item_y;
            item
        })
        .collect();

    (offset_items, min_y, max_y, max_width)
}

/// Clips a text glyph run to page bounds, filtering individual glyphs.
fn clip_text_item(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    clip_rect: LogicalRect,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    if !rect_intersects(&clip_rect, page_top, page_bottom) {
        return None;
    }

    // Filter glyphs using center-point decision (baseline position)
    let page_glyphs: Vec<_> = glyphs
        .iter()
        .filter(|g| g.point.y >= page_top && g.point.y < page_bottom)
        .map(|g| GlyphInstance {
            index: g.index,
            point: LogicalPosition {
                x: g.point.x,
                y: g.point.y - page_top,
            },
            size: g.size,
        })
        .collect();

    if page_glyphs.is_empty() {
        return None;
    }

    Some(DisplayListItem::Text {
        glyphs: page_glyphs,
        font_hash,
        font_size_px,
        color,
        clip_rect: offset_rect_y(clip_rect, -page_top),
    })
}

/// Clips a text decoration (underline, strikethrough, or overline) to page bounds.
fn clip_text_decoration_item(
    bounds: LogicalRect,
    color: ColorU,
    thickness: f32,
    decoration_type: TextDecorationType,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| match decoration_type {
        TextDecorationType::Underline => DisplayListItem::Underline {
            bounds: clipped,
            color,
            thickness,
        },
        TextDecorationType::Strikethrough => DisplayListItem::Strikethrough {
            bounds: clipped,
            color,
            thickness,
        },
        TextDecorationType::Overline => DisplayListItem::Overline {
            bounds: clipped,
            color,
            thickness,
        },
    })
}

/// Clips a scrollbar to page bounds.
fn clip_scrollbar_item(
    bounds: LogicalRect,
    color: ColorU,
    orientation: ScrollbarOrientation,
    opacity_key: Option<OpacityKey>,
    hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::ScrollBar {
        bounds: clipped,
        color,
        orientation,
        opacity_key,
        hit_id,
    })
}

/// Clips a hit test area to page bounds.
fn clip_hit_test_area_item(
    bounds: LogicalRect,
    tag: DisplayListTagId,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::HitTestArea {
        bounds: clipped,
        tag,
    })
}

/// Clips an iframe to page bounds.
fn clip_iframe_item(
    child_dom_id: DomId,
    bounds: LogicalRect,
    clip_rect: LogicalRect,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::IFrame {
        child_dom_id,
        bounds: clipped,
        clip_rect: offset_rect_y(clip_rect, -page_top),
    })
}

/// Clip a rectangle to page bounds and offset to page-relative coordinates.
/// Returns None if the rectangle is completely outside the page.
fn clip_rect_bounds(bounds: LogicalRect, page_top: f32, page_bottom: f32) -> Option<LogicalRect> {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;

    // Check if completely outside page
    if item_bottom <= page_top || item_top >= page_bottom {
        return None;
    }

    // Calculate clipped bounds
    let clipped_top = item_top.max(page_top);
    let clipped_bottom = item_bottom.min(page_bottom);
    let clipped_height = clipped_bottom - clipped_top;

    // Offset to page-relative coordinates
    let page_relative_y = clipped_top - page_top;

    Some(LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: page_relative_y,
        },
        size: LogicalSize {
            width: bounds.size.width,
            height: clipped_height,
        },
    })
}

/// Check if a rectangle intersects the page bounds.
fn rect_intersects(bounds: &LogicalRect, page_top: f32, page_bottom: f32) -> bool {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;
    item_bottom > page_top && item_top < page_bottom
}

/// Offset a rectangle's Y coordinate.
fn offset_rect_y(bounds: LogicalRect, offset_y: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: bounds.origin.y + offset_y,
        },
        size: bounds.size,
    }
}

// Slicer based pagination: "Infinite Canvas with Clipping"
//
// This approach treats pages as "viewports" into a single infinite canvas:
//
// 1. Layout generates ONE display list on an infinite vertical strip
// 2. Each page is a clip rectangle that "views" a portion of that strip
// 3. Items that span page boundaries are clipped and appear on BOTH pages

use azul_css::props::layout::fragmentation::{BreakInside, PageBreak};

use crate::solver3::pagination::{
    HeaderFooterConfig, MarginBoxContent, PageInfo, TableHeaderInfo, TableHeaderTracker,
};

/// Configuration for the slicer-based pagination.
#[derive(Debug, Clone, Default)]
pub struct SlicerConfig {
    /// Height of each page's content area (excludes margins, headers, footers)
    pub page_content_height: f32,
    /// Height of "dead zone" between pages (for margins, headers, footers)
    /// This represents space that content should NOT overlap with
    pub page_gap: f32,
    /// Whether to clip items that span page boundaries (true) or push them to next page (false)
    pub allow_clipping: bool,
    /// Header and footer configuration
    pub header_footer: HeaderFooterConfig,
    /// Width of the page content area (for centering headers/footers)
    pub page_width: f32,
    /// Table headers that need repetition across pages
    pub table_headers: TableHeaderTracker,
}

impl SlicerConfig {
    /// Create a simple slicer config with no gaps between pages.
    pub fn simple(page_height: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: 0.0,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: 595.0, // Default A4 width in points
            table_headers: TableHeaderTracker::default(),
        }
    }

    /// Create a slicer config with margins/gaps between pages.
    pub fn with_gap(page_height: f32, gap: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: gap,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: 595.0,
            table_headers: TableHeaderTracker::default(),
        }
    }

    /// Add header/footer configuration.
    pub fn with_header_footer(mut self, config: HeaderFooterConfig) -> Self {
        self.header_footer = config;
        self
    }

    /// Set the page width (for header/footer positioning).
    pub fn with_page_width(mut self, width: f32) -> Self {
        self.page_width = width;
        self
    }

    /// Add table headers for repetition.
    pub fn with_table_headers(mut self, tracker: TableHeaderTracker) -> Self {
        self.table_headers = tracker;
        self
    }

    /// Register a single table header.
    pub fn register_table_header(&mut self, info: TableHeaderInfo) {
        self.table_headers.register_table_header(info);
    }

    /// The total height of a page "slot" including the gap.
    pub fn page_slot_height(&self) -> f32 {
        self.page_content_height + self.page_gap
    }

    /// Calculate which page a Y coordinate falls on.
    pub fn page_for_y(&self, y: f32) -> usize {
        if self.page_slot_height() <= 0.0 {
            return 0;
        }
        (y / self.page_slot_height()).floor() as usize
    }

    /// Get the Y range for a specific page (in infinite canvas coordinates).
    pub fn page_bounds(&self, page_index: usize) -> (f32, f32) {
        let start = page_index as f32 * self.page_slot_height();
        let end = start + self.page_content_height;
        (start, end)
    }
}

/// Paginate with CSS break property support.
///
/// This function calculates page boundaries based on CSS break-before, break-after,
/// and break-inside properties, then clips content to those boundaries.
///
/// **Key insight**: Items are NEVER shifted. Instead, page boundaries are adjusted
/// to honor break properties.
pub fn paginate_display_list_with_slicer_and_breaks<F>(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    get_break_properties: F,
) -> Result<Vec<DisplayList>>
where
    F: Fn(Option<NodeId>) -> BreakProperties,
{
    if config.page_content_height <= 0.0 || config.page_content_height >= f32::MAX {
        return Ok(vec![full_display_list]);
    }

    // Calculate base header/footer space (used for pages that show headers/footers)
    let base_header_space = if config.header_footer.show_header {
        config.header_footer.header_height
    } else {
        0.0
    };
    let base_footer_space = if config.header_footer.show_footer {
        config.header_footer.footer_height
    } else {
        0.0
    };

    // Calculate effective heights for different page types
    let normal_page_content_height =
        config.page_content_height - base_header_space - base_footer_space;
    let first_page_content_height = if config.header_footer.skip_first_page {
        // First page has full height when skipping headers/footers
        config.page_content_height
    } else {
        normal_page_content_height
    };

    // Step 1: Calculate page break positions based on CSS properties
    //
    // Instead of using regular intervals, we calculate where page breaks
    // should occur based on:
    //
    // - break-before: always → force break before this item
    // - break-after: always → force break after this item
    // - break-inside: avoid → don't break inside this item (push to next page if needed)

    let page_breaks = calculate_page_break_positions(
        &full_display_list,
        first_page_content_height,
        normal_page_content_height,
        &get_break_properties,
    );

    let num_pages = page_breaks.len();

    // Create per-page display lists by slicing the master list
    let mut pages: Vec<DisplayList> = Vec::with_capacity(num_pages);

    for (page_idx, &(content_start_y, content_end_y)) in page_breaks.iter().enumerate() {
        // Generate page info for header/footer content
        let page_info = PageInfo::new(page_idx + 1, num_pages);

        // Calculate per-page header/footer space
        let skip_this_page = config.header_footer.skip_first_page && page_info.is_first;
        let header_space = if config.header_footer.show_header && !skip_this_page {
            config.header_footer.header_height
        } else {
            0.0
        };
        let footer_space = if config.header_footer.show_footer && !skip_this_page {
            config.header_footer.footer_height
        } else {
            0.0
        };

        let _ = footer_space; // Currently unused but reserved for future

        let mut page_items = Vec::new();
        let mut page_node_mapping = Vec::new();

        // 1. Add header if enabled
        if config.header_footer.show_header && !skip_this_page {
            let header_text = config.header_footer.header_text(page_info);
            if !header_text.is_empty() {
                let header_items = generate_text_display_items(
                    &header_text,
                    LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: 0.0 },
                        size: LogicalSize {
                            width: config.page_width,
                            height: config.header_footer.header_height,
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
                );
                for item in header_items {
                    page_items.push(item);
                    page_node_mapping.push(None);
                }
            }
        }

        // 2. Inject repeated table headers (if any)
        let repeated_headers = config.table_headers.get_repeated_headers_for_page(
            page_idx,
            content_start_y,
            content_end_y,
        );

        let mut thead_total_height = 0.0f32;
        for (y_offset_from_page_top, thead_items, thead_height) in repeated_headers {
            let thead_y = header_space + y_offset_from_page_top;
            for item in thead_items {
                let translated_item = offset_display_item_y(item, thead_y);
                page_items.push(translated_item);
                page_node_mapping.push(None);
            }
            thead_total_height = thead_total_height.max(thead_height);
        }

        // 3. Calculate content offset (after header and repeated table headers)
        let content_y_offset = header_space + thead_total_height;

        // 4. Slice and offset content items
        for (item_idx, item) in full_display_list.items.iter().enumerate() {
            if let Some(clipped_item) =
                clip_and_offset_display_item(item, content_start_y, content_end_y)
            {
                let final_item = if content_y_offset > 0.0 {
                    offset_display_item_y(&clipped_item, content_y_offset)
                } else {
                    clipped_item
                };
                page_items.push(final_item);
                let node_mapping = full_display_list
                    .node_mapping
                    .get(item_idx)
                    .copied()
                    .flatten();
                page_node_mapping.push(node_mapping);
            }
        }

        // 5. Add footer if enabled
        if config.header_footer.show_footer && !skip_this_page {
            let footer_text = config.header_footer.footer_text(page_info);
            if !footer_text.is_empty() {
                let footer_y = config.page_content_height - config.header_footer.footer_height;
                let footer_items = generate_text_display_items(
                    &footer_text,
                    LogicalRect {
                        origin: LogicalPosition {
                            x: 0.0,
                            y: footer_y,
                        },
                        size: LogicalSize {
                            width: config.page_width,
                            height: config.header_footer.footer_height,
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
                );
                for item in footer_items {
                    page_items.push(item);
                    page_node_mapping.push(None);
                }
            }
        }

        pages.push(DisplayList {
            items: page_items,
            node_mapping: page_node_mapping,
        });
    }

    // Ensure at least one page
    if pages.is_empty() {
        pages.push(DisplayList::default());
    }

    Ok(pages)
}

/// Calculate page break positions using REGULAR INTERVALS.
///
/// Returns a vector of (start_y, end_y) tuples representing each page's content bounds.
///
/// **IMPORTANT**: CSS break properties (break-before, break-after, break-inside) are
/// currently NOT supported because they require shifting items, not just adjusting
/// page boundaries. The slicer model assumes items stay at their original canvas
/// positions. See PAGINATION_DEBUG_STATE.md for details.
///
/// TODO: To properly support CSS break properties, we need a commitment-based approach
/// that actually moves items when break-before: always is encountered.
fn calculate_page_break_positions<F>(
    display_list: &DisplayList,
    first_page_height: f32,
    normal_page_height: f32,
    _get_break_properties: &F, // Currently unused - see note above
) -> Vec<(f32, f32)>
where
    F: Fn(Option<NodeId>) -> BreakProperties,
{
    let total_height = calculate_display_list_height(display_list);

    if total_height <= 0.0 || first_page_height <= 0.0 {
        return vec![(0.0, total_height.max(first_page_height))];
    }

    // Use simple regular intervals for page breaks
    // This ensures items at Y=X are correctly placed on page floor(X / page_height)
    let mut page_breaks: Vec<(f32, f32)> = Vec::new();
    let mut y = 0.0f32;
    let mut current_page_height = first_page_height;

    while y < total_height {
        let page_end = (y + current_page_height).min(total_height);
        page_breaks.push((y, page_end));
        y = page_end;
        current_page_height = normal_page_height;
    }

    // Ensure at least one page
    if page_breaks.is_empty() {
        page_breaks.push((0.0, total_height.max(first_page_height)));
    }

    page_breaks
}

/// Text alignment for generated header/footer text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Helper to offset all Y coordinates of a display item.
fn offset_display_item_y(item: &DisplayListItem, y_offset: f32) -> DisplayListItem {
    if y_offset == 0.0 {
        return item.clone();
    }

    match item {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => DisplayListItem::Rect {
            bounds: offset_rect_y(*bounds, y_offset),
            color: *color,
            border_radius: *border_radius,
        },
        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => DisplayListItem::Border {
            bounds: offset_rect_y(*bounds, y_offset),
            widths: widths.clone(),
            colors: *colors,
            styles: *styles,
            border_radius: border_radius.clone(),
        },
        DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px,
            color,
            clip_rect,
        } => {
            let offset_glyphs: Vec<GlyphInstance> = glyphs
                .iter()
                .map(|g| GlyphInstance {
                    index: g.index,
                    point: LogicalPosition {
                        x: g.point.x,
                        y: g.point.y + y_offset,
                    },
                    size: g.size,
                })
                .collect();
            DisplayListItem::Text {
                glyphs: offset_glyphs,
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
                clip_rect: offset_rect_y(*clip_rect, y_offset),
            }
        }
        DisplayListItem::TextLayout {
            layout,
            bounds,
            font_hash,
            font_size_px,
            color,
        } => DisplayListItem::TextLayout {
            layout: layout.clone(),
            bounds: offset_rect_y(*bounds, y_offset),
            font_hash: *font_hash,
            font_size_px: *font_size_px,
            color: *color,
        },
        DisplayListItem::Image { bounds, image } => DisplayListItem::Image {
            bounds: offset_rect_y(*bounds, y_offset),
            image: image.clone(),
        },
        // Pass through other items with their bounds offset
        DisplayListItem::SelectionRect {
            bounds,
            border_radius,
            color,
        } => DisplayListItem::SelectionRect {
            bounds: offset_rect_y(*bounds, y_offset),
            border_radius: *border_radius,
            color: *color,
        },
        DisplayListItem::CursorRect { bounds, color } => DisplayListItem::CursorRect {
            bounds: offset_rect_y(*bounds, y_offset),
            color: *color,
        },
        DisplayListItem::Underline {
            bounds,
            color,
            thickness,
        } => DisplayListItem::Underline {
            bounds: offset_rect_y(*bounds, y_offset),
            color: *color,
            thickness: *thickness,
        },
        DisplayListItem::Strikethrough {
            bounds,
            color,
            thickness,
        } => DisplayListItem::Strikethrough {
            bounds: offset_rect_y(*bounds, y_offset),
            color: *color,
            thickness: *thickness,
        },
        DisplayListItem::Overline {
            bounds,
            color,
            thickness,
        } => DisplayListItem::Overline {
            bounds: offset_rect_y(*bounds, y_offset),
            color: *color,
            thickness: *thickness,
        },
        DisplayListItem::ScrollBar {
            bounds,
            color,
            orientation,
            opacity_key,
            hit_id,
        } => DisplayListItem::ScrollBar {
            bounds: offset_rect_y(*bounds, y_offset),
            color: *color,
            orientation: *orientation,
            opacity_key: *opacity_key,
            hit_id: *hit_id,
        },
        DisplayListItem::HitTestArea { bounds, tag } => DisplayListItem::HitTestArea {
            bounds: offset_rect_y(*bounds, y_offset),
            tag: *tag,
        },
        DisplayListItem::PushClip {
            bounds,
            border_radius,
        } => DisplayListItem::PushClip {
            bounds: offset_rect_y(*bounds, y_offset),
            border_radius: *border_radius,
        },
        DisplayListItem::PushScrollFrame {
            clip_bounds,
            content_size,
            scroll_id,
        } => DisplayListItem::PushScrollFrame {
            clip_bounds: offset_rect_y(*clip_bounds, y_offset),
            content_size: *content_size,
            scroll_id: *scroll_id,
        },
        DisplayListItem::PushStackingContext { bounds, z_index } => {
            DisplayListItem::PushStackingContext {
                bounds: offset_rect_y(*bounds, y_offset),
                z_index: *z_index,
            }
        }
        DisplayListItem::IFrame {
            child_dom_id,
            bounds,
            clip_rect,
        } => DisplayListItem::IFrame {
            child_dom_id: *child_dom_id,
            bounds: offset_rect_y(*bounds, y_offset),
            clip_rect: offset_rect_y(*clip_rect, y_offset),
        },
        // Pass through stateless items
        DisplayListItem::PopClip => DisplayListItem::PopClip,
        DisplayListItem::PopScrollFrame => DisplayListItem::PopScrollFrame,
        DisplayListItem::PopStackingContext => DisplayListItem::PopStackingContext,

        // Gradient items
        DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        } => DisplayListItem::LinearGradient {
            bounds: offset_rect_y(*bounds, y_offset),
            gradient: gradient.clone(),
            border_radius: *border_radius,
        },
        DisplayListItem::RadialGradient {
            bounds,
            gradient,
            border_radius,
        } => DisplayListItem::RadialGradient {
            bounds: offset_rect_y(*bounds, y_offset),
            gradient: gradient.clone(),
            border_radius: *border_radius,
        },
        DisplayListItem::ConicGradient {
            bounds,
            gradient,
            border_radius,
        } => DisplayListItem::ConicGradient {
            bounds: offset_rect_y(*bounds, y_offset),
            gradient: gradient.clone(),
            border_radius: *border_radius,
        },

        // BoxShadow
        DisplayListItem::BoxShadow {
            bounds,
            shadow,
            border_radius,
        } => DisplayListItem::BoxShadow {
            bounds: offset_rect_y(*bounds, y_offset),
            shadow: *shadow,
            border_radius: *border_radius,
        },

        // Filter effects
        DisplayListItem::PushFilter { bounds, filters } => DisplayListItem::PushFilter {
            bounds: offset_rect_y(*bounds, y_offset),
            filters: filters.clone(),
        },
        DisplayListItem::PopFilter => DisplayListItem::PopFilter,
        DisplayListItem::PushBackdropFilter { bounds, filters } => {
            DisplayListItem::PushBackdropFilter {
                bounds: offset_rect_y(*bounds, y_offset),
                filters: filters.clone(),
            }
        }
        DisplayListItem::PopBackdropFilter => DisplayListItem::PopBackdropFilter,
        DisplayListItem::PushOpacity { bounds, opacity } => DisplayListItem::PushOpacity {
            bounds: offset_rect_y(*bounds, y_offset),
            opacity: *opacity,
        },
        DisplayListItem::PopOpacity => DisplayListItem::PopOpacity,
        DisplayListItem::ScrollBarStyled { info } => {
            let mut offset_info = (**info).clone();
            offset_info.bounds = offset_rect_y(offset_info.bounds, y_offset);
            offset_info.track_bounds = offset_rect_y(offset_info.track_bounds, y_offset);
            offset_info.thumb_bounds = offset_rect_y(offset_info.thumb_bounds, y_offset);
            if let Some(b) = offset_info.button_decrement_bounds {
                offset_info.button_decrement_bounds = Some(offset_rect_y(b, y_offset));
            }
            if let Some(b) = offset_info.button_increment_bounds {
                offset_info.button_increment_bounds = Some(offset_rect_y(b, y_offset));
            }
            DisplayListItem::ScrollBarStyled {
                info: Box::new(offset_info),
            }
        }
    }
}

/// Generate display list items for simple text (headers/footers).
///
/// This creates a simplified text rendering without full text layout.
/// For now, this creates a placeholder that renderers should handle specially.
fn generate_text_display_items(
    text: &str,
    bounds: LogicalRect,
    font_size: f32,
    color: ColorU,
    alignment: TextAlignment,
) -> Vec<DisplayListItem> {
    use crate::font_traits::FontHash;

    if text.is_empty() {
        return Vec::new();
    }

    // Calculate approximate text position based on alignment
    // For now, we estimate character width as 0.5 * font_size (monospace approximation)
    let char_width = font_size * 0.5;
    let text_width = text.len() as f32 * char_width;

    let x_offset = match alignment {
        TextAlignment::Left => bounds.origin.x,
        TextAlignment::Center => bounds.origin.x + (bounds.size.width - text_width) / 2.0,
        TextAlignment::Right => bounds.origin.x + bounds.size.width - text_width,
    };

    // Position text vertically centered in the bounds
    let y_pos = bounds.origin.y + (bounds.size.height + font_size) / 2.0 - font_size * 0.2;

    // Create simple glyph instances for each character
    // Note: This is a simplified approach - proper text rendering should use text3
    let glyphs: Vec<GlyphInstance> = text
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_control())
        .map(|(i, c)| GlyphInstance {
            index: c as u32, // Use Unicode codepoint as glyph index (placeholder)
            point: LogicalPosition {
                x: x_offset + i as f32 * char_width,
                y: y_pos,
            },
            size: LogicalSize::new(char_width, font_size),
        })
        .collect();

    if glyphs.is_empty() {
        return Vec::new();
    }

    vec![DisplayListItem::Text {
        glyphs,
        font_hash: FontHash::from_hash(0), // Default font hash - renderer should use default font
        font_size_px: font_size,
        color,
        clip_rect: bounds,
    }]
}

/// Calculate the total height of a display list (max Y + height of all items).
fn calculate_display_list_height(display_list: &DisplayList) -> f32 {
    let mut max_bottom = 0.0f32;

    for item in &display_list.items {
        if let Some(bounds) = get_display_item_bounds(item) {
            let item_bottom = bounds.origin.y + bounds.size.height;
            max_bottom = max_bottom.max(item_bottom);
        }
    }

    max_bottom
}

/// Break property information for pagination decisions.
#[derive(Debug, Clone, Copy, Default)]
pub struct BreakProperties {
    pub break_before: PageBreak,
    pub break_after: PageBreak,
    pub break_inside: BreakInside,
}

```

================================================================================
## FILE: layout/src/solver3/fc.rs
## Description: Formatting context - UnifiedConstraints instantiation
================================================================================
```rust
//! solver3/fc.rs - Formatting Context Layout
//!
//! This module implements the CSS Visual Formatting Model's formatting contexts:
//!
//! - **Block Formatting Context (BFC)**: CSS 2.2 § 9.4.1 Block-level boxes in normal flow, with
//!   margin collapsing and float positioning.
//!
//! - **Inline Formatting Context (IFC)**: CSS 2.2 § 9.4.2 Inline-level content (text,
//!   inline-blocks) laid out in line boxes.
//!
//! - **Table Formatting Context**: CSS 2.2 § 17 Table layout with column width calculation and cell
//!   positioning.
//!
//! - **Flex/Grid Formatting Contexts**: CSS Flexbox/Grid via Taffy Delegated to the Taffy layout
//!   engine for modern layout modes.
//!
//! # Module Organization
//!
//! 1. **Constants & Types** - Magic numbers as named constants, core types
//! 2. **Entry Point** - `layout_formatting_context` dispatcher
//! 3. **BFC Layout** - Block formatting context implementation
//! 4. **IFC Layout** - Inline formatting context implementation
//! 5. **Table Layout** - Table formatting context implementation
//! 6. **Flex/Grid Layout** - Taffy bridge wrappers
//! 7. **Helper Functions** - Property getters, margin collapsing, utilities

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontStyle, StyleFontWeight},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric,
        },
        layout::{
            ColumnCount, LayoutBorderSpacing, LayoutClear, LayoutDisplay, LayoutFloat,
            LayoutHeight, LayoutJustifyContent, LayoutOverflow, LayoutPosition, LayoutTableLayout,
            LayoutTextJustify, LayoutWidth, LayoutWritingMode, ShapeInside, ShapeOutside,
            StyleBorderCollapse, StyleCaptionSide,
        },
        property::CssProperty,
        style::{
            BorderStyle, StyleDirection, StyleHyphens, StyleListStylePosition, StyleListStyleType,
            StyleTextAlign, StyleTextCombineUpright, StyleVerticalAlign, StyleVisibility,
            StyleWhiteSpace,
        },
    },
};
use rust_fontconfig::FcWeight;
use taffy::{AvailableSpace, LayoutInput, Line, Size as TaffySize};

#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_ifc_layout, debug_info, debug_log, debug_table_layout, debug_warning,
    font_traits::{
        ContentIndex, FontLoaderTrait, ImageSource, InlineContent, InlineImage, InlineShape,
        LayoutFragment, ObjectFit, ParsedFontTrait, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, TextLayoutCache,
        UnifiedConstraints,
    },
    solver3::{
        geometry::{BoxProps, EdgeSizes, IntrinsicSizes},
        getters::{
            get_css_height, get_css_width, get_display_property, get_element_font_size, get_float,
            get_list_style_position, get_list_style_type, get_overflow_x, get_overflow_y,
            get_parent_font_size, get_root_font_size, get_style_properties, get_writing_mode,
            MultiValue,
        },
        layout_tree::{
            AnonymousBoxType, CachedInlineLayout, LayoutNode, LayoutTree, PseudoElement,
        },
        positioning::get_position_type,
        scrollbar::ScrollbarRequirements,
        sizing::extract_text_from_node,
        taffy_bridge, LayoutContext, LayoutDebugMessage, LayoutError, Result,
    },
    text3::cache::{AvailableSpace as Text3AvailableSpace, TextAlign as Text3TextAlign},
};

/// Default scrollbar width in pixels (CSS Overflow Module Level 3).
/// Used when `overflow: scroll` or `overflow: auto` triggers scrollbar display.
pub const SCROLLBAR_WIDTH_PX: f32 = 16.0;

// Note: DEFAULT_FONT_SIZE and PT_TO_PX are imported from pixel

/// Result of BFC layout with margin escape information
#[derive(Debug, Clone)]
pub(crate) struct BfcLayoutResult {
    /// Standard layout output (positions, overflow size, baseline)
    pub output: LayoutOutput,
    /// Top margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should be used by parent instead of positioning this BFC
    pub escaped_top_margin: Option<f32>,
    /// Bottom margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should collapse with next sibling
    pub escaped_bottom_margin: Option<f32>,
}

impl BfcLayoutResult {
    pub fn from_output(output: LayoutOutput) -> Self {
        Self {
            output,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
        }
    }
}

/// The CSS `overflow` property behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl OverflowBehavior {
    pub fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
}

/// Input constraints for a layout function.
#[derive(Debug)]
pub struct LayoutConstraints<'a> {
    /// The available space for the content, excluding padding and borders.
    pub available_size: LogicalSize,
    /// The CSS writing-mode of the context.
    pub writing_mode: LayoutWritingMode,
    /// The state of the parent Block Formatting Context, if applicable.
    /// This is how state (like floats) is passed down.
    pub bfc_state: Option<&'a mut BfcState>,
    // Other properties like text-align would go here.
    pub text_align: TextAlign,
    /// The size of the containing block (parent's content box).
    /// This is used for resolving percentage-based sizes and as parent_size for Taffy.
    pub containing_block_size: LogicalSize,
    /// The semantic type of the available width constraint.
    ///
    /// This field is crucial for correct inline layout caching:
    /// - `Definite(w)`: Normal layout with a specific available width
    /// - `MinContent`: Intrinsic minimum width measurement (maximum wrapping)
    /// - `MaxContent`: Intrinsic maximum width measurement (no wrapping)
    ///
    /// When caching inline layouts, we must track which constraint type was used
    /// to compute the cached result. A layout computed with `MinContent` (width=0)
    /// must not be reused when the actual available width is known.
    pub available_width_type: Text3AvailableSpace,
}

/// Manages all layout state for a single Block Formatting Context.
/// This struct is created by the BFC root and lives for the duration of its layout.
#[derive(Debug, Clone)]
pub struct BfcState {
    /// The current position for the next in-flow block element.
    pub pen: LogicalPosition,
    /// The state of all floated elements within this BFC.
    pub floats: FloatingContext,
    /// The state of margin collapsing within this BFC.
    pub margins: MarginCollapseContext,
}

impl BfcState {
    pub fn new() -> Self {
        Self {
            pen: LogicalPosition::zero(),
            floats: FloatingContext::default(),
            margins: MarginCollapseContext::default(),
        }
    }
}

/// Manages vertical margin collapsing within a BFC.
#[derive(Debug, Default, Clone)]
pub struct MarginCollapseContext {
    /// The bottom margin of the last in-flow, block-level element.
    /// Can be positive or negative.
    pub last_in_flow_margin_bottom: f32,
}

/// The result of laying out a formatting context.
#[derive(Debug, Default, Clone)]
pub struct LayoutOutput {
    /// The final positions of child nodes, relative to the container's content-box origin.
    pub positions: BTreeMap<usize, LogicalPosition>,
    /// The total size occupied by the content, which may exceed `available_size`.
    pub overflow_size: LogicalSize,
    /// The baseline of the context, if applicable, measured from the top of its content box.
    pub baseline: Option<f32>,
}

/// Text alignment options
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Center,
    Justify,
}

/// Represents a single floated element within a BFC.
#[derive(Debug, Clone, Copy)]
struct FloatBox {
    /// The type of float (Left or Right).
    kind: LayoutFloat,
    /// The rectangle of the float's content box (origin includes top/left margin offset).
    rect: LogicalRect,
    /// The margin sizes (needed to calculate true margin-box bounds).
    margin: EdgeSizes,
}

/// Manages the state of all floated elements within a Block Formatting Context.
#[derive(Debug, Default, Clone)]
pub struct FloatingContext {
    /// All currently positioned floats within the BFC.
    pub floats: Vec<FloatBox>,
}

impl FloatingContext {
    /// Add a newly positioned float to the context
    pub fn add_float(&mut self, kind: LayoutFloat, rect: LogicalRect, margin: EdgeSizes) {
        self.floats.push(FloatBox { kind, rect, margin });
    }

    /// Finds the available space on the cross-axis for a line box at a given main-axis range.
    ///
    /// Returns a tuple of (`cross_start_offset`, `cross_end_offset`) relative to the
    /// BFC content box, defining the available space for an in-flow element.
    pub fn available_line_box_space(
        &self,
        main_start: f32,
        main_end: f32,
        bfc_cross_size: f32,
        wm: LayoutWritingMode,
    ) -> (f32, f32) {
        let mut available_cross_start = 0.0_f32;
        let mut available_cross_end = bfc_cross_size;

        for float in &self.floats {
            // Get the logical main-axis span of the existing float.
            let float_main_start = float.rect.origin.main(wm);
            let float_main_end = float_main_start + float.rect.size.main(wm);

            // Check for overlap on the main axis.
            if main_end > float_main_start && main_start < float_main_end {
                // The float overlaps with the main-axis range of the element we're placing.
                let float_cross_start = float.rect.origin.cross(wm);
                let float_cross_end = float_cross_start + float.rect.size.cross(wm);

                if float.kind == LayoutFloat::Left {
                    // "line-left", i.e., cross-start
                    available_cross_start = available_cross_start.max(float_cross_end);
                } else {
                    // Float::Right, i.e., cross-end
                    available_cross_end = available_cross_end.min(float_cross_start);
                }
            }
        }
        (available_cross_start, available_cross_end)
    }

    /// Returns the main-axis offset needed to be clear of floats of the given type.
    pub fn clearance_offset(
        &self,
        clear: LayoutClear,
        current_main_offset: f32,
        wm: LayoutWritingMode,
    ) -> f32 {
        let mut max_end_offset = 0.0_f32;

        let check_left = clear == LayoutClear::Left || clear == LayoutClear::Both;
        let check_right = clear == LayoutClear::Right || clear == LayoutClear::Both;

        for float in &self.floats {
            let should_clear_this_float = (check_left && float.kind == LayoutFloat::Left)
                || (check_right && float.kind == LayoutFloat::Right);

            if should_clear_this_float {
                // CSS 2.2 § 9.5.2: "the top border edge of the box be below the bottom outer edge"
                // Outer edge = margin-box boundary (content + padding + border + margin)
                let float_margin_box_end = float.rect.origin.main(wm)
                    + float.rect.size.main(wm)
                    + float.margin.main_end(wm);
                max_end_offset = max_end_offset.max(float_margin_box_end);
            }
        }

        if max_end_offset > current_main_offset {
            max_end_offset
        } else {
            current_main_offset
        }
    }
}

/// Encapsulates all state needed to lay out a single Block Formatting Context.
struct BfcLayoutState {
    /// The current position for the next in-flow block element.
    pen: LogicalPosition,
    floats: FloatingContext,
    margins: MarginCollapseContext,
    /// The writing mode of the BFC root.
    writing_mode: LayoutWritingMode,
}

/// Result of a formatting context layout operation
#[derive(Debug, Default)]
pub struct LayoutResult {
    pub positions: Vec<(usize, LogicalPosition)>,
    pub overflow_size: Option<LogicalSize>,
    pub baseline_offset: f32,
}

// Entry Point & Dispatcher

/// Main dispatcher for formatting context layout.
///
/// Routes layout to the appropriate formatting context handler based on the node's
/// `formatting_context` property. This is the main entry point for all layout operations.
///
/// # CSS Spec References
/// - CSS 2.2 § 9.4: Formatting contexts
/// - CSS Flexbox § 3: Flex formatting contexts
/// - CSS Grid § 5: Grid formatting contexts
pub fn layout_formatting_context<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut std::collections::BTreeMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    debug_info!(
        ctx,
        "[layout_formatting_context] node_index={}, fc={:?}, available_size={:?}",
        node_index,
        node.formatting_context,
        constraints.available_size
    );

    match node.formatting_context {
        FormattingContext::Block { .. } => {
            layout_bfc(ctx, tree, text_cache, node_index, constraints, float_cache)
        }
        FormattingContext::Inline => layout_ifc(ctx, text_cache, tree, node_index, constraints)
            .map(BfcLayoutResult::from_output),
        FormattingContext::Table => layout_table_fc(ctx, tree, text_cache, node_index, constraints)
            .map(BfcLayoutResult::from_output),
        FormattingContext::Flex | FormattingContext::Grid => {
            layout_flex_grid(ctx, tree, text_cache, node_index, constraints)
        }
        _ => {
            // Unknown formatting context - fall back to BFC
            let mut temp_float_cache = std::collections::BTreeMap::new();
            layout_bfc(
                ctx,
                tree,
                text_cache,
                node_index,
                constraints,
                &mut temp_float_cache,
            )
        }
    }
}

// Flex / grid layout (taffy Bridge)

/// Lays out a Flex or Grid formatting context using the Taffy layout engine.
///
/// # CSS Spec References
///
/// - CSS Flexbox § 9: Flex Layout Algorithm
/// - CSS Grid § 12: Grid Layout Algorithm
///
/// # Implementation Notes
///
/// - Resolves explicit CSS dimensions to pixel values for `known_dimensions`
/// - Uses `InherentSize` mode when explicit dimensions are set
/// - Uses `ContentSize` mode for auto-sizing (shrink-to-fit)
fn layout_flex_grid<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<BfcLayoutResult> {
    let available_space = TaffySize {
        width: AvailableSpace::Definite(constraints.available_size.width),
        height: AvailableSpace::Definite(constraints.available_size.height),
    };

    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // Resolve explicit CSS dimensions to pixel values.
    // This is CRITICAL for align-items: stretch to work correctly!
    // Taffy uses known_dimensions to calculate cross_axis_available_space for children.
    let (explicit_width, has_explicit_width) =
        resolve_explicit_dimension_width(ctx, node, constraints);
    let (explicit_height, has_explicit_height) =
        resolve_explicit_dimension_height(ctx, node, constraints);

    // FIX: Taffy interprets known_dimensions as Border Box size.
    // CSS width/height properties define Content Box size (by default, box-sizing: content-box).
    // We must add border and padding to the explicit dimensions to get the correct Border
    // Box size for Taffy.
    let width_adjustment = node.box_props.border.left
        + node.box_props.border.right
        + node.box_props.padding.left
        + node.box_props.padding.right;
    let height_adjustment = node.box_props.border.top
        + node.box_props.border.bottom
        + node.box_props.padding.top
        + node.box_props.padding.bottom;

    // Apply adjustment only if dimensions are explicit (convert content-box to border-box)
    let adjusted_width = explicit_width.map(|w| w + width_adjustment);
    let adjusted_height = explicit_height.map(|h| h + height_adjustment);

    // CSS Flexbox § 9.2: Use InherentSize when explicit dimensions are set,
    // ContentSize for auto-sizing (shrink-to-fit behavior).
    let sizing_mode = if has_explicit_width || has_explicit_height {
        taffy::SizingMode::InherentSize
    } else {
        taffy::SizingMode::ContentSize
    };

    let known_dimensions = TaffySize {
        width: adjusted_width,
        height: adjusted_height,
    };

    let taffy_inputs = LayoutInput {
        known_dimensions,
        parent_size: translate_taffy_size(constraints.containing_block_size),
        available_space,
        run_mode: taffy::RunMode::PerformLayout,
        sizing_mode,
        axis: taffy::RequestedAxis::Both,
        // Flex and Grid containers establish a new BFC, preventing margin collapse.
        vertical_margins_are_collapsible: Line::FALSE,
    };

    debug_info!(
        ctx,
        "CALLING LAYOUT_TAFFY FOR FLEX/GRID FC node_index={:?}",
        node_index
    );

    let taffy_output =
        taffy_bridge::layout_taffy_subtree(ctx, tree, text_cache, node_index, taffy_inputs);

    // Collect child positions from the tree (Taffy stores results directly on nodes).
    let mut output = LayoutOutput::default();
    // Use content_size for overflow detection, not container size.
    // content_size represents the actual size of all children, which may exceed the container.
    output.overflow_size = translate_taffy_size_back(taffy_output.content_size);

    let children: Vec<usize> = tree.get(node_index).unwrap().children.clone();
    for &child_idx in &children {
        if let Some(child_node) = tree.get(child_idx) {
            if let Some(pos) = child_node.relative_position {
                output.positions.insert(child_idx, pos);
            }
        }
    }

    Ok(BfcLayoutResult::from_output(output))
}

/// Resolves explicit CSS width to pixel value for Taffy layout.
fn resolve_explicit_dimension_width<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    constraints: &LayoutConstraints,
) -> (Option<f32>, bool) {
    node.dom_node_id
        .map(|id| {
            let width = get_css_width(
                ctx.styled_dom,
                id,
                &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
            );
            match width.unwrap_or_default() {
                LayoutWidth::Auto => (None, false),
                LayoutWidth::Px(px) => {
                    let pixels = resolve_size_metric(
                        px.metric,
                        px.number.get(),
                        constraints.available_size.width,
                    );
                    (Some(pixels), true)
                }
                LayoutWidth::MinContent | LayoutWidth::MaxContent => (None, false),
            }
        })
        .unwrap_or((None, false))
}

/// Resolves explicit CSS height to pixel value for Taffy layout.
fn resolve_explicit_dimension_height<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    constraints: &LayoutConstraints,
) -> (Option<f32>, bool) {
    node.dom_node_id
        .map(|id| {
            let height = get_css_height(
                ctx.styled_dom,
                id,
                &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
            );
            match height.unwrap_or_default() {
                LayoutHeight::Auto => (None, false),
                LayoutHeight::Px(px) => {
                    let pixels = resolve_size_metric(
                        px.metric,
                        px.number.get(),
                        constraints.available_size.height,
                    );
                    (Some(pixels), true)
                }
                LayoutHeight::MinContent | LayoutHeight::MaxContent => (None, false),
            }
        })
        .unwrap_or((None, false))
}

/// Position a float within a BFC, considering existing floats.
/// Returns the LogicalRect (margin box) for the float.
fn position_float(
    float_ctx: &FloatingContext,
    float_type: LayoutFloat,
    size: LogicalSize,
    margin: &EdgeSizes,
    current_main_offset: f32,
    bfc_cross_size: f32,
    wm: LayoutWritingMode,
) -> LogicalRect {
    // Start at the current main-axis position (Y in horizontal-tb)
    let mut main_start = current_main_offset;

    // Calculate total size including margins
    let total_main = size.main(wm) + margin.main_start(wm) + margin.main_end(wm);
    let total_cross = size.cross(wm) + margin.cross_start(wm) + margin.cross_end(wm);

    // Find a position where the float fits
    let cross_start = loop {
        let (avail_start, avail_end) = float_ctx.available_line_box_space(
            main_start,
            main_start + total_main,
            bfc_cross_size,
            wm,
        );

        let available_width = avail_end - avail_start;

        if available_width >= total_cross {
            // Found space that fits
            if float_type == LayoutFloat::Left {
                // Position at line-left (avail_start)
                break avail_start + margin.cross_start(wm);
            } else {
                // Position at line-right (avail_end - size)
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }

        // Not enough space at this Y, move down past the lowest overlapping float
        let next_main = float_ctx
            .floats
            .iter()
            .filter(|f| {
                let f_main_start = f.rect.origin.main(wm);
                let f_main_end = f_main_start + f.rect.size.main(wm);
                f_main_end > main_start && f_main_start < main_start + total_main
            })
            .map(|f| f.rect.origin.main(wm) + f.rect.size.main(wm))
            .max_by(|a, b| a.partial_cmp(b).unwrap());

        if let Some(next) = next_main {
            main_start = next;
        } else {
            // No overlapping floats found, use current position anyway
            if float_type == LayoutFloat::Left {
                break avail_start + margin.cross_start(wm);
            } else {
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }
    };

    LogicalRect {
        origin: LogicalPosition::from_main_cross(
            main_start + margin.main_start(wm),
            cross_start,
            wm,
        ),
        size,
    }
}

// Block Formatting Context (CSS 2.2 § 9.4.1)

/// Lays out a Block Formatting Context (BFC).
///
/// This is the corrected, architecturally-sound implementation. It solves the
/// "chicken-and-egg" problem by performing its own two-pass layout:
///
/// 1. **Sizing Pass:** It first iterates through its children and triggers their layout recursively
///    by calling `calculate_layout_for_subtree`. This ensures that the `used_size` property of each
///    child is correctly populated.
///
/// 2. **Positioning Pass:** It then iterates through the children again. Now that each child has a
///    valid size, it can apply the standard block-flow logic: stacking them vertically and
///    advancing a "pen" by each child's outer height.
///
/// # Margin Collapsing Architecture
///
/// CSS 2.1 Section 8.3.1 compliant margin collapsing:
///
/// ```text
/// layout_bfc()
///   ├─ Check parent border/padding blockers
///   ├─ For each child:
///   │   ├─ Check child border/padding blockers
///   │   ├─ is_first_child?
///   │   │   └─ Check parent-child top collapse
///   │   ├─ Sibling collapse?
///   │   │   └─ advance_pen_with_margin_collapse()
///   │   │       └─ collapse_margins(prev_bottom, curr_top)
///   │   ├─ Position child
///   │   ├─ is_empty_block()?
///   │   │   └─ Collapse own top+bottom margins (collapse through)
///   │   └─ Save bottom margin for next sibling
///   └─ Check parent-child bottom collapse
/// ```
///
/// **Collapsing Rules:**
///
/// - Sibling margins: Adjacent vertical margins collapse to max (or sum if mixed signs)
/// - Parent-child: First child's top margin can escape parent (if no border/padding)
/// - Parent-child: Last child's bottom margin can escape parent (if no border/padding/height)
/// - Empty blocks: Top+bottom margins collapse with each other, then with siblings
/// - Blockers: Border, padding, inline content, or new BFC prevents collapsing
///
/// This approach is compliant with the CSS visual formatting model and works within
/// the constraints of the existing layout engine architecture.
fn layout_bfc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut std::collections::BTreeMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone();
    let writing_mode = constraints.writing_mode;
    let mut output = LayoutOutput::default();

    debug_info!(
        ctx,
        "\n[layout_bfc] ENTERED for node_index={}, children.len()={}, incoming_bfc_state={}",
        node_index,
        node.children.len(),
        constraints.bfc_state.is_some()
    );

    // Initialize FloatingContext for this BFC
    //
    // We always recalculate float positions in this pass, but we'll store them in the cache
    // so that subsequent layout passes (for auto-sizing) have access to the positioned floats
    let mut float_context = FloatingContext::default();

    // Calculate this node's content-box size for use as containing block for children
    // CSS 2.2 § 10.1: The containing block for in-flow children is formed by the
    // content edge of the parent's content box.
    //
    // We use constraints.available_size directly as this already represents the
    // content-box available to this node (set by parent). For nodes with explicit
    // sizes, used_size contains the border-box which we convert to content-box.
    let mut children_containing_block_size = if let Some(used_size) = node.used_size {
        // Node has explicit used_size (border-box) - convert to content-box
        node.box_props.inner_size(used_size, writing_mode)
    } else {
        // No used_size yet - use available_size directly (this is already content-box
        // when coming from parent's layout constraints)
        constraints.available_size
    };

    // Proactively reserve space for vertical scrollbar if overflow-y is auto/scroll.
    // This ensures children are laid out with the correct available width from the start,
    // preventing the "children overlap scrollbar" layout issue.
    let scrollbar_reservation = node
        .dom_node_id
        .map(|dom_id| {
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|s| s.styled_node_state.clone())
                .unwrap_or_default();
            let overflow_y =
                crate::solver3::getters::get_overflow_y(ctx.styled_dom, dom_id, &styled_node_state);
            use azul_css::props::layout::LayoutOverflow;
            match overflow_y.unwrap_or_default() {
                LayoutOverflow::Scroll | LayoutOverflow::Auto => SCROLLBAR_WIDTH_PX,
                _ => 0.0,
            }
        })
        .unwrap_or(0.0);

    if scrollbar_reservation > 0.0 {
        children_containing_block_size.width =
            (children_containing_block_size.width - scrollbar_reservation).max(0.0);
    }

    // Pass 1: Size all children (floats and normal flow)
    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        // Skip out-of-flow children (absolute/fixed)
        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // Size all children (floats and normal flow) - floats will be positioned later in Pass 2
        let mut temp_positions = BTreeMap::new();
        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            child_index,
            LogicalPosition::zero(),
            children_containing_block_size, // Use this node's content-box as containing block
            &mut temp_positions,
            &mut bool::default(),
            float_cache,
        )?;
    }

    // Pass 2: Single-pass interleaved layout (position floats and normal flow in DOM order)

    let mut main_pen = 0.0f32;
    let mut max_cross_size = 0.0f32;

    // Track escaped margins separately from content-box height
    // CSS 2.2 § 8.3.1: Escaped margins don't contribute to parent's content-box height,
    // but DO affect sibling positioning within the parent
    let mut total_escaped_top_margin = 0.0f32;
    // Track all inter-sibling margins (collapsed) - these are also not part of content height
    let mut total_sibling_margins = 0.0f32;

    // Margin collapsing state
    let mut last_margin_bottom = 0.0f32;
    let mut is_first_child = true;
    let mut first_child_index: Option<usize> = None;
    let mut last_child_index: Option<usize> = None;

    // Parent's own margins (for escape calculation)
    let parent_margin_top = node.box_props.margin.main_start(writing_mode);
    let parent_margin_bottom = node.box_props.margin.main_end(writing_mode);

    // Check if parent (this BFC root) has border/padding that prevents parent-child collapse
    let parent_has_top_blocker = has_margin_collapse_blocker(&node.box_props, writing_mode, true);
    let parent_has_bottom_blocker =
        has_margin_collapse_blocker(&node.box_props, writing_mode, false);

    // Track accumulated top margin for first-child escape
    let mut accumulated_top_margin = 0.0f32;
    let mut top_margin_resolved = false;
    // Track if first child's margin escaped (for return value)
    let mut top_margin_escaped = false;

    // Track if we have any actual content (non-empty blocks)
    let mut has_content = false;

    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // Check if this child is a float - if so, position it at current main_pen
        let is_float = if let Some(node_id) = child_dom_id {
            let float_type = get_float_property(ctx.styled_dom, Some(node_id));

            if float_type != LayoutFloat::None {
                let float_size = child_node.used_size.unwrap_or_default();
                let float_margin = &child_node.box_props.margin;

                // CSS 2.2 § 9.5: Float margins don't collapse with any other margins.
                // If there's a previous in-flow element with a bottom margin, we must
                // include it in the Y position calculation for this float.
                let float_y = main_pen + last_margin_bottom;

                debug_info!(
                    ctx,
                    "[layout_bfc] Positioning float: index={}, type={:?}, size={:?}, at Y={} \
                     (main_pen={} + last_margin={})",
                    child_index,
                    float_type,
                    float_size,
                    float_y,
                    main_pen,
                    last_margin_bottom
                );

                // Position the float at the CURRENT main_pen + last margin (respects DOM order!)
                let float_rect = position_float(
                    &float_context,
                    float_type,
                    float_size,
                    float_margin,
                    // Include last_margin_bottom since float margins don't collapse!
                    float_y,
                    constraints.available_size.cross(writing_mode),
                    writing_mode,
                );

                debug_info!(ctx, "[layout_bfc] Float positioned at: {:?}", float_rect);

                // Add to float context BEFORE positioning next element
                float_context.add_float(float_type, float_rect, *float_margin);

                // Store position in output
                output.positions.insert(child_index, float_rect.origin);

                debug_info!(
                    ctx,
                    "[layout_bfc] *** FLOAT POSITIONED: child={}, main_pen={} (unchanged - floats \
                     don't advance pen)",
                    child_index,
                    main_pen
                );

                // Floats are taken out of normal flow - DON'T advance main_pen
                // Continue to next child
                continue;
            }
            false
        } else {
            false
        };

        // Early exit for floats (already handled above)
        if is_float {
            continue;
        }

        // From here: normal flow (non-float) children only

        // Track first and last in-flow children for parent-child collapse
        if first_child_index.is_none() {
            first_child_index = Some(child_index);
        }
        last_child_index = Some(child_index);

        let child_size = child_node.used_size.unwrap_or_default();
        let child_margin = &child_node.box_props.margin;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} margin from box_props: top={}, right={}, bottom={}, left={}",
            child_index,
            child_margin.top,
            child_margin.right,
            child_margin.bottom,
            child_margin.left
        );

        // IMPORTANT: Use the ACTUAL margins from box_props, NOT escaped margins!
        //
        // Escaped margins are only relevant for the parent-child relationship WITHIN a node's
        // own BFC layout. When positioning this child in ITS parent's BFC, we use its actual
        // margins. CSS 2.2 § 8.3.1: Margin collapsing happens between ADJACENT margins,
        // which means:
        //
        // - Parent's top and first child's top (if no blocker)
        // - Sibling's bottom and next sibling's top
        // - Parent's bottom and last child's bottom (if no blocker)
        //
        // The escaped_top_margin stored in the child node is for its OWN children, not for itself!
        let child_margin_top = child_margin.main_start(writing_mode);
        let child_margin_bottom = child_margin.main_end(writing_mode);

        debug_info!(
            ctx,
            "[layout_bfc] Child {} final margins: margin_top={}, margin_bottom={}",
            child_index,
            child_margin_top,
            child_margin_bottom
        );

        // Check if this child has border/padding that prevents margin collapsing
        let child_has_top_blocker =
            has_margin_collapse_blocker(&child_node.box_props, writing_mode, true);
        let child_has_bottom_blocker =
            has_margin_collapse_blocker(&child_node.box_props, writing_mode, false);

        // Check for clear property FIRST - clearance affects whether element is considered empty
        // CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"
        // An element with clearance is NOT empty even if it has no content
        let child_clear = if let Some(node_id) = child_dom_id {
            get_clear_property(ctx.styled_dom, Some(node_id))
        } else {
            LayoutClear::None
        };
        debug_info!(
            ctx,
            "[layout_bfc] Child {} clear property: {:?}",
            child_index,
            child_clear
        );

        // PHASE 1: Empty Block Detection & Self-Collapse
        let is_empty = is_empty_block(child_node);

        // Handle empty blocks FIRST (they collapse through and don't participate in layout)
        // EXCEPTION: Elements with clear property are NOT skipped even if empty!
        // CSS 2.2 § 9.5.2: Clear property affects positioning even for empty elements
        if is_empty
            && !child_has_top_blocker
            && !child_has_bottom_blocker
            && child_clear == LayoutClear::None
        {
            // Empty block: collapse its own top and bottom margins FIRST
            let self_collapsed = collapse_margins(child_margin_top, child_margin_bottom);

            // Then collapse with previous margin (sibling or parent)
            if is_first_child {
                is_first_child = false;
                // Empty first child: its collapsed margin can escape with parent's
                if !parent_has_top_blocker {
                    accumulated_top_margin = collapse_margins(parent_margin_top, self_collapsed);
                } else {
                    // Parent has blocker: add margins
                    if accumulated_top_margin == 0.0 {
                        accumulated_top_margin = parent_margin_top;
                    }
                    main_pen += accumulated_top_margin + self_collapsed;
                    top_margin_resolved = true;
                    accumulated_top_margin = 0.0;
                }
                last_margin_bottom = self_collapsed;
            } else {
                // Empty sibling: collapse with previous sibling's bottom margin
                last_margin_bottom = collapse_margins(last_margin_bottom, self_collapsed);
            }

            // Skip positioning and pen advance (empty has no visual presence)
            continue;
        }

        // From here on: non-empty blocks only (or empty blocks with clear property)

        // Apply clearance if needed
        // CSS 2.2 § 9.5.2: Clearance inhibits margin collapsing
        let clearance_applied = if child_clear != LayoutClear::None {
            let cleared_offset =
                float_context.clearance_offset(child_clear, main_pen, writing_mode);
            debug_info!(
                ctx,
                "[layout_bfc] Child {} clearance check: cleared_offset={}, main_pen={}",
                child_index,
                cleared_offset,
                main_pen
            );
            if cleared_offset > main_pen {
                debug_info!(
                    ctx,
                    "[layout_bfc] Applying clearance: child={}, clear={:?}, old_pen={}, new_pen={}",
                    child_index,
                    child_clear,
                    main_pen,
                    cleared_offset
                );
                main_pen = cleared_offset;
                true // Signal that clearance was applied
            } else {
                false
            }
        } else {
            false
        };

        // PHASE 2: Parent-Child Top Margin Escape (First Child)
        //
        // CSS 2.2 § 8.3.1: "The top margin of a box is adjacent to the top margin of its first
        // in-flow child if the box has no top border, no top padding, and the child has no
        // clearance." CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

        if is_first_child {
            is_first_child = false;

            // Clearance prevents collapse (acts as invisible blocker)
            if clearance_applied {
                // Clearance inhibits all margin collapsing for this element
                // The clearance has already positioned main_pen past floats
                //
                // CSS 2.2 § 8.3.1: Parent's margin was already handled by parent's parent BFC
                // We only add child's margin in our content-box coordinate space
                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} with CLEARANCE: no collapse, child_margin={}, \
                     main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else if !parent_has_top_blocker {
                // Margin Escape Case
                //
                // CSS 2.2 § 8.3.1: "The top margin of an in-flow block element collapses with
                // its first in-flow block-level child's top margin if the element has no top
                // border, no top padding, and the child has no clearance."
                //
                // When margins collapse, they "escape" upward through the parent to be resolved
                // in the grandparent's coordinate space. This is critical for understanding the
                // coordinate system separation:
                //
                // Example:
                // <body padding=20>
                //  <div margin=0>
                //      <div margin=30></div>
                //  </div>
                // </body>
                //
                //   - Middle div (our parent) has no padding → margins can escape
                //   - Inner div's 30px margin collapses with middle div's 0px margin = 30px
                //   - This 30px margin "escapes" to be handled by body's BFC
                //   - Body positions middle div at Y=30 (relative to body's content-box)
                //   - Middle div's content-box height does NOT include the escaped 30px
                //   - Inner div is positioned at Y=0 in middle div's content-box
                //
                // **NOTE**: This is a subtle but critical distinction in coordinate systems:
                //
                //   - Parent's margin belongs to grandparent's coordinate space
                //   - Child's margin (when escaped) also belongs to grandparent's coordinate space
                //   - They collapse BEFORE entering this BFC's coordinate space
                //   - We return the collapsed margin so grandparent can position parent correctly
                //
                // **NOTE**: Child's own blocker status (padding/border) is IRRELEVANT for
                // parent-child  collapse. The child may have padding that prevents
                // collapse with ITS OWN  children, but this doesn't prevent its
                // margin from escaping  through its parent.
                //
                // **NOTE**: Previously, we incorrectly added parent_margin_top to main_pen in
                //  the blocked case, which double-counted the margin by mixing
                //  coordinate systems. The parent's margin is NEVER in our (the
                //  parent's content-box) coordinate system!

                accumulated_top_margin = collapse_margins(parent_margin_top, child_margin_top);
                top_margin_resolved = true;
                top_margin_escaped = true;

                // Track escaped margin so it gets subtracted from content-box height
                // The escaped margin is NOT part of our content-box - it belongs to our
                // parent's parent
                total_escaped_top_margin = accumulated_top_margin;

                // Position child at pen (no margin applied - it escaped!)
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} margin ESCAPES: parent_margin={}, \
                     child_margin={}, collapsed={}, total_escaped={}",
                    child_index,
                    parent_margin_top,
                    child_margin_top,
                    accumulated_top_margin,
                    total_escaped_top_margin
                );
            } else {
                // Margin Blocked Case
                //
                // CSS 2.2 § 8.3.1: "no top padding and no top border" required for collapse.
                // When padding or border exists, margins do NOT collapse and exist in different
                // coordinate spaces.
                //
                // CRITICAL COORDINATE SYSTEM SEPARATION:
                //
                //   This is where the architecture becomes subtle. When layout_bfc() is called:
                //   1. We are INSIDE the parent's content-box coordinate space (main_pen starts at
                //      0)
                //   2. The parent's own margin was ALREADY RESOLVED by the grandparent's BFC
                //   3. The parent's margin is in the grandparent's coordinate space, not ours
                //   4. We NEVER reference the parent's margin in this BFC - it's outside our scope
                //
                // Example:
                //
                // <body padding=20>
                //   <div margin=30 padding=20>
                //      <div margin=30></div>
                //   </div>
                // </body>
                //
                //   - Middle div has padding=20 → blocker exists, margins don't collapse
                //   - Body's BFC positions middle div at Y=30 (middle div's margin, in body's
                //     space)
                //   - Middle div's BFC starts at its content-box (after the padding)
                //   - main_pen=0 at the top of middle div's content-box
                //   - Inner div has margin=30 → we add 30 to main_pen (in OUR coordinate space)
                //   - Inner div positioned at Y=30 (relative to middle div's content-box)
                //   - Absolute position: 20 (body padding) + 30 (middle margin) + 20 (middle
                //     padding) + 30 (inner margin) = 100px
                //
                // **NOTE**: Previous code incorrectly added parent_margin_top to main_pen here:
                //
                //     - main_pen += parent_margin_top;  // WRONG! Mixes coordinate systems
                //     - main_pen += child_margin_top;
                //
                //   This caused the "double margin" bug where margins were applied twice:
                //
                //   - Once by grandparent positioning parent (correct)
                //   - Again inside parent's BFC (INCORRECT - wrong coordinate system)
                //
                //   The parent's margin belongs to GRANDPARENT's coordinate space and was already
                //   used to position the parent. Adding it again here is like adding feet to
                //   meters.
                //
                //   We ONLY add the child's margin in our (parent's content-box) coordinate space.
                //   The parent's margin is irrelevant to us - it's outside our scope.

                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} BLOCKED: parent_has_blocker={}, advanced by \
                     child_margin={}, main_pen={}",
                    child_index,
                    parent_has_top_blocker,
                    child_margin_top,
                    main_pen
                );
            }
        } else {
            // Not first child: handle sibling collapse
            // CSS 2.2 § 8.3.1 Rule 1: "Vertical margins of adjacent block boxes in the normal flow
            // collapse" CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

            // Resolve accumulated top margin if not yet done (for parent's first in-flow child)
            if !top_margin_resolved {
                main_pen += accumulated_top_margin;
                top_margin_resolved = true;
                debug_info!(
                    ctx,
                    "[layout_bfc] RESOLVED top margin for node {} at sibling {}: accumulated={}, \
                     main_pen={}",
                    node_index,
                    child_index,
                    accumulated_top_margin,
                    main_pen
                );
            }

            if clearance_applied {
                // Clearance inhibits collapsing - add full margin
                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} with CLEARANCE: no collapse with sibling, \
                     child_margin_top={}, main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else {
                // Sibling Margin Collapse
                //
                // CSS 2.2 § 8.3.1: "Vertical margins of adjacent block boxes in the normal
                // flow collapse." The collapsed margin is the maximum of the two margins.
                //
                // IMPORTANT: Sibling margins ARE part of the parent's content-box height!
                //
                // Unlike escaped margins (which belong to grandparent's space), sibling margins
                // are the space BETWEEN children within our content-box.
                //
                // Example:
                //
                // <div>
                //  <div margin-bottom=30></div>
                //  <div margin-top=40></div>
                // </div>
                //
                //   - First child ends at Y=100 (including its content + margins)
                //   - Collapsed margin = max(30, 40) = 40px
                //   - Second child starts at Y=140 (100 + 40)
                //   - Parent's content-box height includes this 40px gap
                //
                // We track total_sibling_margins for debugging, but NOTE: we do **not**
                // subtract these from content-box height! They are part of the layout space.
                //
                // Previously we subtracted total_sibling_margins from content-box height:
                //
                //   content_box_height = main_pen - total_escaped_top_margin -
                // total_sibling_margins;
                //
                // This was wrong because sibling margins are between boxes (part of content),
                // not outside boxes (like escaped margins).

                let collapsed = collapse_margins(last_margin_bottom, child_margin_top);
                main_pen += collapsed;
                total_sibling_margins += collapsed;
                debug_info!(
                    ctx,
                    "[layout_bfc] Sibling collapse for child {}: last_margin_bottom={}, \
                     child_margin_top={}, collapsed={}, main_pen={}, total_sibling_margins={}",
                    child_index,
                    last_margin_bottom,
                    child_margin_top,
                    collapsed,
                    main_pen,
                    total_sibling_margins
                );
            }
        }

        // Position child (non-empty blocks only reach here)
        //
        // CSS 2.2 § 9.4.1: "In a block formatting context, each box's left outer edge touches
        // the left edge of the containing block (for right-to-left formatting, right edges touch).
        // This is true even in the presence of floats (although a box's line boxes may shrink
        // due to the floats), unless the box establishes a new block formatting context
        // (in which case the box itself may become narrower due to the floats)."
        //
        // CSS 2.2 § 9.5: "The border box of a table, a block-level replaced element, or an element
        // in the normal flow that establishes a new block formatting context (such as an element
        // with 'overflow' other than 'visible') must not overlap any floats in the same block
        // formatting context as the element itself."

        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let establishes_bfc = establishes_new_bfc(ctx, child_node);

        // Query available space considering floats ONLY if child establishes new BFC
        let (cross_start, cross_end, available_cross) = if establishes_bfc {
            // New BFC: Must shrink or move down to avoid overlapping floats
            let (start, end) = float_context.available_line_box_space(
                main_pen,
                main_pen + child_size.main(writing_mode),
                constraints.available_size.cross(writing_mode),
                writing_mode,
            );
            let available = end - start;

            debug_info!(
                ctx,
                "[layout_bfc] Child {} establishes BFC: shrinking to avoid floats, \
                 cross_range={}..{}, available_cross={}",
                child_index,
                start,
                end,
                available
            );

            (start, end, available)
        } else {
            // Normal flow: Overlaps floats, positioned at full width
            // Only the child's INLINE CONTENT (if any) wraps around floats
            let start = 0.0;
            let end = constraints.available_size.cross(writing_mode);
            let available = end - start;

            debug_info!(
                ctx,
                "[layout_bfc] Child {} is normal flow: overlapping floats at full width, \
                 available_cross={}",
                child_index,
                available
            );

            (start, end, available)
        };

        // Get child's margin and formatting context
        let (child_margin_cloned, is_inline_fc) = {
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            (
                child_node.box_props.margin.clone(),
                child_node.formatting_context == FormattingContext::Inline,
            )
        };
        let child_margin = &child_margin_cloned;

        // Position child
        // For normal flow blocks (including IFCs): position at full width (cross_start = 0)
        // For BFC-establishing blocks: position in available space between floats
        let (child_cross_pos, mut child_main_pos) = if establishes_bfc {
            // BFC: Position in space between floats
            (
                cross_start + child_margin.cross_start(writing_mode),
                main_pen,
            )
        } else {
            // Normal flow: Position at full width (floats don't affect box position)
            (child_margin.cross_start(writing_mode), main_pen)
        };

        // CSS 2.2 § 8.3.1: If child's top margin escaped through parent, adjust position
        // "If the top margin of a box collapses with its first child's top margin,
        // the top border edge of the box is defined to coincide with the top border edge of the
        // child." This means the child's margin appears ABOVE the parent, so we offset the
        // child down. IMPORTANT: This only applies to the FIRST child! For siblings, normal
        // margin collapse applies.
        let child_escaped_margin = child_node.escaped_top_margin.unwrap_or(0.0);
        let is_first_in_flow_child = Some(child_index) == first_child_index;

        if child_escaped_margin > 0.0 && is_first_in_flow_child {
            child_main_pos += child_escaped_margin;
            total_escaped_top_margin += child_escaped_margin;
            debug_info!(
                ctx,
                "[layout_bfc] FIRST child {} has escaped_top_margin={}, adjusting position from \
                 {} to {}, total_escaped={}",
                child_index,
                child_escaped_margin,
                main_pen,
                child_main_pos,
                total_escaped_top_margin
            );
        } else if child_escaped_margin > 0.0 {
            debug_info!(
                ctx,
                "[layout_bfc] NON-FIRST child {} has escaped_top_margin={} but NOT adjusting \
                 position (sibling margin collapse handles this)",
                child_index,
                child_escaped_margin
            );
        }

        let final_pos =
            LogicalPosition::from_main_cross(child_main_pos, child_cross_pos, writing_mode);

        debug_info!(
            ctx,
            "[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child={}, final_pos={:?}, \
             main_pen={}, establishes_bfc={}",
            child_index,
            final_pos,
            main_pen,
            establishes_bfc
        );

        // Re-layout IFC children with float context for correct text wrapping
        // Normal flow blocks WITH inline content need float context propagated
        if is_inline_fc && !establishes_bfc {
            // Use cached floats if available (from previous layout passes),
            // otherwise use the floats positioned in this pass
            let floats_for_ifc = float_cache.get(&node_index).unwrap_or(&float_context);

            debug_info!(
                ctx,
                "[layout_bfc] Re-layouting IFC child {} (normal flow) with parent's float context \
                 at Y={}, child_cross_pos={}",
                child_index,
                main_pen,
                child_cross_pos
            );
            debug_info!(
                ctx,
                "[layout_bfc]   Using {} floats (from cache: {})",
                floats_for_ifc.floats.len(),
                float_cache.contains_key(&node_index)
            );

            // Translate float coordinates from BFC-relative to IFC-relative
            // The IFC child is positioned at (child_cross_pos, main_pen) in BFC coordinates
            // Floats need to be relative to the IFC's CONTENT-BOX origin (inside padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let padding_border_cross = child_node.box_props.padding.cross_start(writing_mode)
                + child_node.box_props.border.cross_start(writing_mode);
            let padding_border_main = child_node.box_props.padding.main_start(writing_mode)
                + child_node.box_props.border.main_start(writing_mode);

            // Content-box origin in BFC coordinates
            let content_box_cross = child_cross_pos + padding_border_cross;
            let content_box_main = main_pen + padding_border_main;

            debug_info!(
                ctx,
                "[layout_bfc]   Border-box at ({}, {}), Content-box at ({}, {}), \
                 padding+border=({}, {})",
                child_cross_pos,
                main_pen,
                content_box_cross,
                content_box_main,
                padding_border_cross,
                padding_border_main
            );

            let mut ifc_floats = FloatingContext::default();
            for float_box in &floats_for_ifc.floats {
                // Convert float position from BFC coords to IFC CONTENT-BOX relative coords
                let float_rel_to_ifc = LogicalRect {
                    origin: LogicalPosition {
                        x: float_box.rect.origin.x - content_box_cross,
                        y: float_box.rect.origin.y - content_box_main,
                    },
                    size: float_box.rect.size,
                };

                debug_info!(
                    ctx,
                    "[layout_bfc] Float {:?}: BFC coords = {:?}, IFC-content-relative = {:?}",
                    float_box.kind,
                    float_box.rect,
                    float_rel_to_ifc
                );

                ifc_floats.add_float(float_box.kind, float_rel_to_ifc, float_box.margin);
            }

            // Create a BfcState with IFC-relative float coordinates
            let mut bfc_state = BfcState {
                pen: LogicalPosition::zero(), // IFC starts at its own origin
                floats: ifc_floats.clone(),
                margins: MarginCollapseContext::default(),
            };

            debug_info!(
                ctx,
                "[layout_bfc]   Created IFC-relative FloatingContext with {} floats",
                ifc_floats.floats.len()
            );

            // Get the IFC child's content-box size (after padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_dom_id = child_node.dom_node_id;

            // For inline elements (display: inline), use containing block width as available
            // width. Inline elements flow within the containing block and wrap at its width.
            // CSS 2.2 § 10.3.1: For inline elements, available width = containing block width.
            let display = get_display_property(ctx.styled_dom, child_dom_id).unwrap_or_default();
            let child_content_size = if display == LayoutDisplay::Inline {
                // Inline elements use the containing block's content-box width
                LogicalSize::new(
                    children_containing_block_size.width,
                    children_containing_block_size.height,
                )
            } else {
                // Block-level elements use their own content-box
                child_node.box_props.inner_size(child_size, writing_mode)
            };

            debug_info!(
                ctx,
                "[layout_bfc]   IFC child size: border-box={:?}, content-box={:?}",
                child_size,
                child_content_size
            );

            // Create new constraints with float context
            // IMPORTANT: Use the child's CONTENT-BOX width, not the BFC width!
            let ifc_constraints = LayoutConstraints {
                available_size: child_content_size,
                bfc_state: Some(&mut bfc_state),
                writing_mode,
                text_align: constraints.text_align,
                containing_block_size: constraints.containing_block_size,
                available_width_type: Text3AvailableSpace::Definite(child_content_size.width),
            };

            // Re-layout the IFC with float awareness
            // This will pass floats as exclusion zones to text3 for line wrapping
            let ifc_result = layout_formatting_context(
                ctx,
                tree,
                text_cache,
                child_index,
                &ifc_constraints,
                float_cache,
            )?;

            // DON'T update used_size - the box keeps its full width!
            // Only the text layout inside changes to wrap around floats

            debug_info!(
                ctx,
                "[layout_bfc] IFC child {} re-layouted with float context (text will wrap, box \
                 stays full width)",
                child_index
            );

            // Merge positions from IFC output (inline-block children, etc.)
            for (idx, pos) in ifc_result.output.positions {
                output.positions.insert(idx, pos);
            }
        }

        output.positions.insert(child_index, final_pos);

        // Advance the pen past the child's content size
        // For FIRST child with escaped margin: the escaped margin was added to position,
        // so we need to add it to main_pen too for correct sibling positioning
        // For NON-FIRST children: escaped margins are internal to that child, don't affect our pen
        if is_first_in_flow_child && child_escaped_margin > 0.0 {
            main_pen += child_size.main(writing_mode) + child_escaped_margin;
            debug_info!(
                ctx,
                "[layout_bfc] Advanced main_pen by child_size={} + escaped={} = {} total",
                child_size.main(writing_mode),
                child_escaped_margin,
                main_pen
            );
        } else {
            main_pen += child_size.main(writing_mode);
        }
        has_content = true;

        // Update last margin for next sibling
        // CSS 2.2 § 8.3.1: The bottom margin of this box will collapse with the top margin
        // of the next sibling (if no clearance or blockers intervene)
        // CSS 2.2 § 9.5.2: If clearance was applied, margin collapsing is inhibited
        if clearance_applied {
            // Clearance inhibits collapse - next sibling starts fresh
            last_margin_bottom = 0.0;
        } else {
            last_margin_bottom = child_margin_bottom;
        }

        debug_info!(
            ctx,
            "[layout_bfc] Child {} positioned at final_pos={:?}, size={:?}, advanced main_pen to \
             {}, last_margin_bottom={}, clearance_applied={}",
            child_index,
            final_pos,
            child_size,
            main_pen,
            last_margin_bottom,
            clearance_applied
        );

        // Track the maximum cross-axis size to determine the BFC's overflow size.
        let child_cross_extent =
            child_cross_pos + child_size.cross(writing_mode) + child_margin.cross_end(writing_mode);
        max_cross_size = max_cross_size.max(child_cross_extent);
    }

    // Store the float context in cache for future layout passes
    // This happens after ALL children (floats and normal) have been positioned
    debug_info!(
        ctx,
        "[layout_bfc] Storing {} floats in cache for node {}",
        float_context.floats.len(),
        node_index
    );
    float_cache.insert(node_index, float_context.clone());

    // PHASE 3: Parent-Child Bottom Margin Escape
    let mut escaped_top_margin = None;
    let mut escaped_bottom_margin = None;

    // Handle top margin escape
    if top_margin_escaped {
        // First child's margin escaped through parent
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Returning escaped top margin: accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else if !top_margin_resolved && accumulated_top_margin > 0.0 {
        // No content was positioned, all margins accumulated (empty blocks)
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Escaping top margin (no content): accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else if !top_margin_resolved {
        // Unusual case: no content, zero margin
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Escaping top margin (zero, no content): accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else {
        debug_info!(
            ctx,
            "[layout_bfc] NOT escaping top margin: top_margin_resolved={}, escaped={}, \
             accumulated={}, node={}",
            top_margin_resolved,
            top_margin_escaped,
            accumulated_top_margin,
            node_index
        );
    }

    // Handle bottom margin escape
    if let Some(last_idx) = last_child_index {
        let last_child = tree.get(last_idx).ok_or(LayoutError::InvalidTree)?;
        let last_has_bottom_blocker =
            has_margin_collapse_blocker(&last_child.box_props, writing_mode, false);

        debug_info!(
            ctx,
            "[layout_bfc] Bottom margin for node {}: parent_has_bottom_blocker={}, \
             last_has_bottom_blocker={}, last_margin_bottom={}, main_pen_before={}",
            node_index,
            parent_has_bottom_blocker,
            last_has_bottom_blocker,
            last_margin_bottom,
            main_pen
        );

        if !parent_has_bottom_blocker && !last_has_bottom_blocker && has_content {
            // Last child's bottom margin can escape
            let collapsed_bottom = collapse_margins(parent_margin_bottom, last_margin_bottom);
            escaped_bottom_margin = Some(collapsed_bottom);
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin ESCAPED for node {}: collapsed={}",
                node_index,
                collapsed_bottom
            );
            // Don't add last_margin_bottom to pen (it escaped)
        } else {
            // Can't escape: add to pen
            main_pen += last_margin_bottom;
            // NOTE: We do NOT add parent_margin_bottom to main_pen here!
            // parent_margin_bottom is added OUTSIDE the content-box (in the margin-box)
            // The content-box height should only include children's content and margins
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin BLOCKED for node {}: added last_margin_bottom={}, \
                 main_pen_after={}",
                node_index,
                last_margin_bottom,
                main_pen
            );
        }
    } else {
        // No children: just use parent's margins
        if !top_margin_resolved {
            main_pen += parent_margin_top;
        }
        main_pen += parent_margin_bottom;
    }

    // CRITICAL: If this is a root node (no parent), apply escaped margins directly
    // instead of propagating them upward (since there's no parent to receive them)
    let is_root_node = node.parent.is_none();
    if is_root_node {
        if let Some(top) = escaped_top_margin {
            // Adjust all child positions downward by the escaped top margin
            for (_, pos) in output.positions.iter_mut() {
                let current_main = pos.main(writing_mode);
                *pos = LogicalPosition::from_main_cross(
                    current_main + top,
                    pos.cross(writing_mode),
                    writing_mode,
                );
            }
            main_pen += top;
        }
        if let Some(bottom) = escaped_bottom_margin {
            main_pen += bottom;
        }
        // For root nodes, don't propagate margins further
        escaped_top_margin = None;
        escaped_bottom_margin = None;
    }

    // CSS 2.2 § 9.5: Floats don't contribute to container height with overflow:visible
    //
    // However, browsers DO expand containers to contain floats in specific cases:
    //
    // 1. If there's NO in-flow content (main_pen == 0), floats determine height
    // 2. If container establishes a BFC (overflow != visible)
    //
    // In this case, we have in-flow content (main_pen > 0) and overflow:visible,
    // so floats should NOT expand the container. Their margins can "bleed" beyond
    // the container boundaries into the parent.
    //
    // This matches Chrome/Firefox behavior where float margins escape through
    // the container's padding when there's existing in-flow content.

    // Content-box Height Calculation
    //
    // CSS 2.2 § 8.3.1: "The top border edge of the box is defined to coincide with
    // the top border edge of the [first] child" when margins collapse/escape.
    //
    // This means escaped margins do NOT contribute to the parent's content-box height.
    //
    // Calculation:
    //
    //   main_pen = total vertical space used by all children and margins
    //
    //   Components of main_pen:
    //
    //   1. Children's border-boxes (always included)
    //   2. Sibling collapsed margins (space BETWEEN children - part of content)
    //   3. First child's position (0 if margin escaped, margin_top if blocked)
    //
    //   What to subtract:
    //
    //   - total_escaped_top_margin: First child's margin that went to grandparent's space This
    //     margin is OUTSIDE our content-box, so we must subtract it.
    //
    //   What NOT to subtract:
    //
    //   - total_sibling_margins: These are the gaps BETWEEN children, which are
    //    legitimately part of our content area's layout space.
    //
    // Example with escaped margin:
    //   <div class="parent" padding=0>              <!-- Node 2 -->
    //     <div class="child1" margin=30></div>      <!-- Node 3, margin escapes -->
    //     <div class="child2" margin=40></div>      <!-- Node 5 -->
    //   </div>
    //
    //   Layout process:
    //
    //   - Node 3 positioned at main_pen=0 (margin escaped)
    //   - Node 3 size=140px → main_pen advances to 140
    //   - Sibling collapse: max(30 child1 bottom, 40 child2 top) = 40px
    //   - main_pen advances to 180
    //   - Node 5 size=130px → main_pen advances to 310
    //   - total_escaped_top_margin = 30
    //   - total_sibling_margins = 40 (tracked but NOT subtracted)
    //   - content_box_height = 310 - 30 = 280px ✓
    //
    // Previously, we calculated:
    //
    //   content_box_height = main_pen - total_escaped_top_margin - total_sibling_margins
    //
    // This incorrectly subtracted sibling margins, making parent too small.
    // Sibling margins are *between* boxes (part of layout), not *outside* boxes
    // (like escaped margins).

    let content_box_height = main_pen - total_escaped_top_margin;
    output.overflow_size =
        LogicalSize::from_main_cross(content_box_height, max_cross_size, writing_mode);

    debug_info!(
        ctx,
        "[layout_bfc] FINAL for node {}: main_pen={}, total_escaped_top={}, \
         total_sibling_margins={}, content_box_height={}",
        node_index,
        main_pen,
        total_escaped_top_margin,
        total_sibling_margins,
        content_box_height
    );

    // Baseline calculation would happen here in a full implementation.
    output.baseline = None;

    // Store escaped margins in the LayoutNode for use by parent
    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.escaped_top_margin = escaped_top_margin;
        node_mut.escaped_bottom_margin = escaped_bottom_margin;
    }

    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.baseline = output.baseline;
    }

    Ok(BfcLayoutResult {
        output,
        escaped_top_margin,
        escaped_bottom_margin,
    })
}

// Inline Formatting Context (CSS 2.2 § 9.4.2)

/// Lays out an Inline Formatting Context (IFC) by delegating to the `text3` engine.
///
/// This function acts as a bridge between the box-tree world of `solver3` and the
/// rich text layout world of `text3`. Its responsibilities are:
///
/// 1. **Collect Content**: Traverse the direct children of the IFC root and convert them into a
///    `Vec<InlineContent>`, the input format for `text3`. This involves:
///
///     - Recursively laying out `inline-block` children to determine their final size and baseline,
///       which are then passed to `text3` as opaque objects.
///     - Extracting raw text runs from inline text nodes.
///
/// 2. **Translate Constraints**: Convert the `LayoutConstraints` (available space, floats) from
///    `solver3` into the more detailed `UnifiedConstraints` that `text3` requires.
///
/// 3. **Invoke Text Layout**: Call the `text3` cache's `layout_flow` method to perform the complex
///    tasks of BIDI analysis, shaping, line breaking, justification, and vertical alignment.
///
/// 4. **Integrate Results**: Process the `UnifiedLayout` returned by `text3`:
///
///     - Store the rich layout result on the IFC root `LayoutNode` for the display list generation
///       pass.
///     - Update the `positions` map for all `inline-block` children based on the positions
///       calculated by `text3`.
///     - Extract the final overflow size and baseline for the IFC root itself
fn layout_ifc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    tree: &mut LayoutTree,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let float_count = constraints
        .bfc_state
        .as_ref()
        .map(|s| s.floats.floats.len())
        .unwrap_or(0);
    debug_info!(
        ctx,
        "[layout_ifc] ENTRY: node_index={}, has_bfc_state={}, float_count={}",
        node_index,
        constraints.bfc_state.is_some(),
        float_count
    );
    debug_ifc_layout!(ctx, "CALLED for node_index={}", node_index);

    // For anonymous boxes, we need to find the DOM ID from a parent or child
    // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let ifc_root_dom_id = match node.dom_node_id {
        Some(id) => id,
        None => {
            // Anonymous box - get DOM ID from parent or first child with DOM ID
            let parent_dom_id = node
                .parent
                .and_then(|p| tree.get(p))
                .and_then(|n| n.dom_node_id);

            if let Some(id) = parent_dom_id {
                id
            } else {
                // Try to find DOM ID from first child
                node.children
                    .iter()
                    .filter_map(|&child_idx| tree.get(child_idx))
                    .filter_map(|n| n.dom_node_id)
                    .next()
                    .ok_or(LayoutError::InvalidTree)?
            }
        }
    };

    debug_ifc_layout!(ctx, "ifc_root_dom_id={:?}", ifc_root_dom_id);

    // Phase 1: Collect and measure all inline-level children.
    let (inline_content, child_map) =
        collect_and_measure_inline_content(ctx, text_cache, tree, node_index, constraints)?;

    debug_info!(
        ctx,
        "[layout_ifc] Collected {} inline content items for node {}",
        inline_content.len(),
        node_index
    );
    for (i, item) in inline_content.iter().enumerate() {
        match item {
            InlineContent::Text(run) => debug_info!(ctx, "  [{}] Text: '{}'", i, run.text),
            InlineContent::Marker {
                run,
                position_outside,
            } => debug_info!(
                ctx,
                "  [{}] Marker: '{}' (outside={})",
                i,
                run.text,
                position_outside
            ),
            InlineContent::Shape(_) => debug_info!(ctx, "  [{}] Shape", i),
            InlineContent::Image(_) => debug_info!(ctx, "  [{}] Image", i),
            _ => debug_info!(ctx, "  [{}] Other", i),
        }
    }

    debug_ifc_layout!(
        ctx,
        "Collected {} inline content items",
        inline_content.len()
    );

    if inline_content.is_empty() {
        debug_warning!(ctx, "inline_content is empty, returning default output!");
        return Ok(LayoutOutput::default());
    }

    // Phase 2: Translate constraints and define a single layout fragment for text3.
    let text3_constraints =
        translate_to_text3_constraints(ctx, constraints, ctx.styled_dom, ifc_root_dom_id);

    // Clone constraints for caching (before they're moved into fragments)
    let cached_constraints = text3_constraints.clone();

    debug_info!(
        ctx,
        "[layout_ifc] CALLING text_cache.layout_flow for node {} with {} exclusions",
        node_index,
        text3_constraints.shape_exclusions.len()
    );

    let fragments = vec![LayoutFragment {
        id: "main".to_string(),
        constraints: text3_constraints,
    }];

    // Phase 3: Invoke the text layout engine.
    // Get pre-loaded fonts from font manager (fonts should be loaded before layout)
    let loaded_fonts = ctx.font_manager.get_loaded_fonts();
    let text_layout_result = match text_cache.layout_flow(
        &inline_content,
        &[],
        &fragments,
        &ctx.font_manager.font_chain_cache,
        &ctx.font_manager.fc_cache,
        &loaded_fonts,
        ctx.debug_messages,
    ) {
        Ok(result) => result,
        Err(e) => {
            // Font errors should not stop layout of other elements.
            // Log the error and return a zero-sized layout.
            debug_warning!(ctx, "Text layout failed: {:?}", e);
            debug_warning!(
                ctx,
                "Continuing with zero-sized layout for node {}",
                node_index
            );

            let mut output = LayoutOutput::default();
            output.overflow_size = LogicalSize::new(0.0, 0.0);
            return Ok(output);
        }
    };

    // Phase 4: Integrate results back into the solver3 layout tree.
    let mut output = LayoutOutput::default();
    let node = tree.get_mut(node_index).ok_or(LayoutError::InvalidTree)?;

    debug_ifc_layout!(
        ctx,
        "text_layout_result has {} fragment_layouts",
        text_layout_result.fragment_layouts.len()
    );

    if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
        let frag_bounds = main_frag.bounds();
        debug_ifc_layout!(
            ctx,
            "Found 'main' fragment with {} items, bounds={}x{}",
            main_frag.items.len(),
            frag_bounds.width,
            frag_bounds.height
        );
        debug_ifc_layout!(ctx, "Storing inline_layout_result on node {}", node_index);

        // Determine if we should store this layout result using the new
        // CachedInlineLayout system. The key insight is that inline layouts
        // depend on available width:
        //
        // - Min-content measurement uses width ≈ 0 (maximum line wrapping)
        // - Max-content measurement uses width = ∞ (no line wrapping)
        // - Final layout uses the actual column/container width
        //
        // We must track which constraint type was used, otherwise a min-content
        // measurement would incorrectly be reused for final rendering.
        let has_floats = constraints
            .bfc_state
            .as_ref()
            .map(|s| !s.floats.floats.is_empty())
            .unwrap_or(false);
        let current_width_type = constraints.available_width_type;

        let should_store = match &node.inline_layout_result {
            None => {
                // No cached result - always store
                debug_info!(
                    ctx,
                    "[layout_ifc] Storing NEW inline_layout_result for node {} (width_type={:?}, \
                     has_floats={})",
                    node_index,
                    current_width_type,
                    has_floats
                );
                true
            }
            Some(cached) => {
                // Check if the new result should replace the cached one
                if cached.should_replace_with(current_width_type, has_floats) {
                    debug_info!(
                        ctx,
                        "[layout_ifc] REPLACING inline_layout_result for node {} (old: \
                         width={:?}, floats={}) with (new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    true
                } else {
                    debug_info!(
                        ctx,
                        "[layout_ifc] KEEPING cached inline_layout_result for node {} (cached: \
                         width={:?}, floats={}, new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    false
                }
            }
        };

        if should_store {
            node.inline_layout_result = Some(CachedInlineLayout::new_with_constraints(
                main_frag.clone(),
                current_width_type,
                has_floats,
                cached_constraints,
            ));
        }

        // Extract the overall size and baseline for the IFC root.
        output.overflow_size = LogicalSize::new(frag_bounds.width, frag_bounds.height);
        output.baseline = main_frag.last_baseline();
        node.baseline = output.baseline;

        // Position all the inline-block children based on text3's calculations.
        for positioned_item in &main_frag.items {
            if let ShapedItem::Object { source, content, .. } = &positioned_item.item {
                if let Some(&child_node_index) = child_map.get(source) {
                    let new_relative_pos = LogicalPosition {
                        x: positioned_item.position.x,
                        y: positioned_item.position.y,
                    };
                    output.positions.insert(child_node_index, new_relative_pos);
                }
            }
        }
    }

    Ok(output)
}

fn translate_taffy_size(size: LogicalSize) -> TaffySize<Option<f32>> {
    TaffySize {
        width: Some(size.width),
        height: Some(size.height),
    }
}

/// Helper: Convert StyleFontStyle to text3::cache::FontStyle
pub(crate) fn convert_font_style(style: StyleFontStyle) -> crate::font_traits::FontStyle {
    match style {
        StyleFontStyle::Normal => crate::font_traits::FontStyle::Normal,
        StyleFontStyle::Italic => crate::font_traits::FontStyle::Italic,
        StyleFontStyle::Oblique => crate::font_traits::FontStyle::Oblique,
    }
}

/// Helper: Convert StyleFontWeight to FcWeight
pub(crate) fn convert_font_weight(weight: StyleFontWeight) -> FcWeight {
    match weight {
        StyleFontWeight::W100 => FcWeight::Thin,
        StyleFontWeight::W200 => FcWeight::ExtraLight,
        StyleFontWeight::W300 | StyleFontWeight::Lighter => FcWeight::Light,
        StyleFontWeight::Normal => FcWeight::Normal,
        StyleFontWeight::W500 => FcWeight::Medium,
        StyleFontWeight::W600 => FcWeight::SemiBold,
        StyleFontWeight::Bold => FcWeight::Bold,
        StyleFontWeight::W800 => FcWeight::ExtraBold,
        StyleFontWeight::W900 | StyleFontWeight::Bolder => FcWeight::Black,
    }
}

/// Resolves a CSS size metric to pixels.
#[inline]
fn resolve_size_metric(metric: SizeMetric, value: f32, containing_block_size: f32) -> f32 {
    match metric {
        SizeMetric::Px => value,
        SizeMetric::Pt => value * PT_TO_PX,
        SizeMetric::Percent => value / 100.0 * containing_block_size,
        SizeMetric::Em | SizeMetric::Rem => value * DEFAULT_FONT_SIZE,
        _ => value, // Fallback
    }
}

pub fn translate_taffy_size_back(size: TaffySize<f32>) -> LogicalSize {
    LogicalSize {
        width: size.width,
        height: size.height,
    }
}

pub fn translate_taffy_point_back(point: taffy::Point<f32>) -> LogicalPosition {
    LogicalPosition {
        x: point.x,
        y: point.y,
    }
}

/// Checks if a node establishes a new Block Formatting Context (BFC).
///
/// Per CSS 2.2 § 9.4.1, a BFC is established by:
/// - Floats (elements with float other than 'none')
/// - Absolutely positioned elements (position: absolute or fixed)
/// - Block containers that are not block boxes (e.g., inline-blocks, table-cells)
/// - Block boxes with 'overflow' other than 'visible' and 'clip'
/// - Elements with 'display: flow-root'
/// - Table cells, table captions, and inline-blocks
///
/// Normal flow block-level boxes do NOT establish a new BFC.
///
/// This is critical for correct float interaction: normal blocks should overlap floats
/// (not shrink around them), while their inline content wraps around floats.
fn establishes_new_bfc<T: ParsedFontTrait>(ctx: &LayoutContext<'_, T>, node: &LayoutNode) -> bool {
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

    // 1. Floats establish BFC
    let float_val = get_float(ctx.styled_dom, dom_id, node_state);
    if matches!(
        float_val,
        MultiValue::Exact(LayoutFloat::Left | LayoutFloat::Right)
    ) {
        return true;
    }

    // 2. Absolutely positioned elements establish BFC
    let position = crate::solver3::positioning::get_position_type(ctx.styled_dom, Some(dom_id));
    if matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed) {
        return true;
    }

    // 3. Inline-blocks, table-cells, table-captions establish BFC
    let display = get_display_property(ctx.styled_dom, Some(dom_id));
    if matches!(
        display,
        MultiValue::Exact(
            LayoutDisplay::InlineBlock | LayoutDisplay::TableCell | LayoutDisplay::TableCaption
        )
    ) {
        return true;
    }

    // 4. display: flow-root establishes BFC
    if matches!(display, MultiValue::Exact(LayoutDisplay::FlowRoot)) {
        return true;
    }

    // 5. Block boxes with overflow other than 'visible' or 'clip' establish BFC
    // Note: 'clip' does NOT establish BFC per CSS Overflow Module Level 3
    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, node_state);

    let creates_bfc_via_overflow = |ov: &MultiValue<LayoutOverflow>| {
        matches!(
            ov,
            &MultiValue::Exact(
                LayoutOverflow::Hidden | LayoutOverflow::Scroll | LayoutOverflow::Auto
            )
        )
    };

    if creates_bfc_via_overflow(&overflow_x) || creates_bfc_via_overflow(&overflow_y) {
        return true;
    }

    // 6. Table, Flex, and Grid containers establish BFC (via FormattingContext)
    if matches!(
        node.formatting_context,
        FormattingContext::Table | FormattingContext::Flex | FormattingContext::Grid
    ) {
        return true;
    }

    // Normal flow block boxes do NOT establish BFC
    false
}

/// Translates solver3 layout constraints into the text3 engine's unified constraints.
fn translate_to_text3_constraints<'a, T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    constraints: &'a LayoutConstraints<'a>,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> UnifiedConstraints {
    // Convert floats into exclusion zones for text3 to flow around.
    let mut shape_exclusions = if let Some(ref bfc_state) = constraints.bfc_state {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, converting {} floats to exclusions",
            dom_id,
            bfc_state.floats.floats.len()
        );
        bfc_state
            .floats
            .floats
            .iter()
            .enumerate()
            .map(|(i, float_box)| {
                let rect = crate::text3::cache::Rect {
                    x: float_box.rect.origin.x,
                    y: float_box.rect.origin.y,
                    width: float_box.rect.size.width,
                    height: float_box.rect.size.height,
                };
                debug_info!(
                    ctx,
                    "[translate_to_text3]   Exclusion #{}: {:?} at ({}, {}) size {}x{}",
                    i,
                    float_box.kind,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height
                );
                ShapeBoundary::Rectangle(rect)
            })
            .collect()
    } else {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, NO bfc_state - no float exclusions",
            dom_id
        );
        Vec::new()
    };

    debug_info!(
        ctx,
        "[translate_to_text3] dom_id={:?}, available_size={}x{}, shape_exclusions.len()={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        shape_exclusions.len()
    );

    // Map text-align and justify-content from CSS to text3 enums.
    let id = dom_id;
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;

    // Read CSS Shapes properties
    // For reference box, use the element's CSS height if available, otherwise available_size
    // This is important because available_size.height might be infinite during auto height
    // calculation
    let ref_box_height = if constraints.available_size.height.is_finite() {
        constraints.available_size.height
    } else {
        // Try to get explicit CSS height
        // NOTE: If height is infinite, we can't properly resolve % heights
        // This is a limitation - shape-inside with % heights requires finite containing block
        styled_dom
            .css_property_cache
            .ptr
            .get_height(node_data, &id, node_state)
            .and_then(|v| v.get_property())
            .and_then(|h| match h {
                LayoutHeight::Px(v) => {
                    // Only accept absolute units (px, pt, in, cm, mm) - no %, em, rem
                    // since we can't resolve relative units without proper context
                    match v.metric {
                        SizeMetric::Px => Some(v.number.get()),
                        SizeMetric::Pt => Some(v.number.get() * PT_TO_PX),
                        SizeMetric::In => Some(v.number.get() * 96.0),
                        SizeMetric::Cm => Some(v.number.get() * 96.0 / 2.54),
                        SizeMetric::Mm => Some(v.number.get() * 96.0 / 25.4),
                        _ => None, // Ignore %, em, rem
                    }
                }
                _ => None,
            })
            .unwrap_or(constraints.available_size.width) // Fallback: use width as height (square)
    };

    let reference_box = crate::text3::cache::Rect {
        x: 0.0,
        y: 0.0,
        width: constraints.available_size.width,
        height: ref_box_height,
    };

    // shape-inside: Text flows within the shape boundary
    debug_info!(ctx, "Checking shape-inside for node {:?}", id);
    debug_info!(
        ctx,
        "Reference box: {:?} (available_size height was: {})",
        reference_box,
        constraints.available_size.height
    );

    let shape_boundaries = styled_dom
        .css_property_cache
        .ptr
        .get_shape_inside(node_data, &id, node_state)
        .and_then(|v| {
            debug_info!(ctx, "Got shape-inside value: {:?}", v);
            v.get_property()
        })
        .and_then(|shape_inside| {
            debug_info!(ctx, "shape-inside property: {:?}", shape_inside);
            if let ShapeInside::Shape(css_shape) = shape_inside {
                debug_info!(
                    ctx,
                    "Converting CSS shape to ShapeBoundary: {:?}",
                    css_shape
                );
                let boundary =
                    ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                debug_info!(ctx, "Created ShapeBoundary: {:?}", boundary);
                Some(vec![boundary])
            } else {
                debug_info!(ctx, "shape-inside is None");
                None
            }
        })
        .unwrap_or_default();

    debug_info!(
        ctx,
        "Final shape_boundaries count: {}",
        shape_boundaries.len()
    );

    // shape-outside: Text wraps around the shape (adds to exclusions)
    debug_info!(ctx, "Checking shape-outside for node {:?}", id);
    if let Some(shape_outside_value) = styled_dom
        .css_property_cache
        .ptr
        .get_shape_outside(node_data, &id, node_state)
    {
        debug_info!(ctx, "Got shape-outside value: {:?}", shape_outside_value);
        if let Some(shape_outside) = shape_outside_value.get_property() {
            debug_info!(ctx, "shape-outside property: {:?}", shape_outside);
            if let ShapeOutside::Shape(css_shape) = shape_outside {
                debug_info!(
                    ctx,
                    "Converting CSS shape-outside to ShapeBoundary: {:?}",
                    css_shape
                );
                let boundary =
                    ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                debug_info!(ctx, "Created ShapeBoundary (exclusion): {:?}", boundary);
                shape_exclusions.push(boundary);
            }
        }
    } else {
        debug_info!(ctx, "No shape-outside value found");
    }

    // TODO: clip-path will be used for rendering clipping (not text layout)

    let writing_mode = styled_dom
        .css_property_cache
        .ptr
        .get_writing_mode(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let text_align = styled_dom
        .css_property_cache
        .ptr
        .get_text_align(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let text_justify = styled_dom
        .css_property_cache
        .ptr
        .get_text_justify(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    // Get font-size for resolving line-height
    // Use helper function which checks dependency chain first
    let font_size = get_element_font_size(styled_dom, id, node_state);

    let line_height_value = styled_dom
        .css_property_cache
        .ptr
        .get_line_height(node_data, &id, node_state)
        .and_then(|s| s.get_property().cloned())
        .unwrap_or_default();

    let hyphenation = styled_dom
        .css_property_cache
        .ptr
        .get_hyphens(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let overflow_behaviour = styled_dom
        .css_property_cache
        .ptr
        .get_overflow_x(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    // Get vertical-align from CSS property cache (defaults to Baseline per CSS spec)
    let vertical_align = styled_dom
        .css_property_cache
        .ptr
        .get_vertical_align(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let vertical_align = match vertical_align {
        StyleVerticalAlign::Baseline => text3::cache::VerticalAlign::Baseline,
        StyleVerticalAlign::Top => text3::cache::VerticalAlign::Top,
        StyleVerticalAlign::Middle => text3::cache::VerticalAlign::Middle,
        StyleVerticalAlign::Bottom => text3::cache::VerticalAlign::Bottom,
        StyleVerticalAlign::Sub => text3::cache::VerticalAlign::Sub,
        StyleVerticalAlign::Superscript => text3::cache::VerticalAlign::Super,
        StyleVerticalAlign::TextTop => text3::cache::VerticalAlign::TextTop,
        StyleVerticalAlign::TextBottom => text3::cache::VerticalAlign::TextBottom,
    };
    let text_orientation = text3::cache::TextOrientation::default();

    // Get the direction property from the CSS cache (defaults to LTR if not set)
    let direction = styled_dom
        .css_property_cache
        .ptr
        .get_direction(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .map(|d| match d {
            StyleDirection::Ltr => text3::cache::BidiDirection::Ltr,
            StyleDirection::Rtl => text3::cache::BidiDirection::Rtl,
        });

    debug_info!(
        ctx,
        "dom_id={:?}, available_size={}x{}, setting available_width={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        constraints.available_size.width
    );

    // Get text-indent
    let text_indent = styled_dom
        .css_property_cache
        .ptr
        .get_text_indent(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|ti| {
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(constraints.available_size.width, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            ti.inner
                .resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or(0.0);

    // Get column-count for multi-column layout (default: 1 = no columns)
    let columns = styled_dom
        .css_property_cache
        .ptr
        .get_column_count(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cc| match cc {
            ColumnCount::Integer(n) => *n,
            ColumnCount::Auto => 1,
        })
        .unwrap_or(1);

    // Get column-gap for multi-column layout (default: normal = 1em)
    let column_gap = styled_dom
        .css_property_cache
        .ptr
        .get_column_gap(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cg| {
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(0.0, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            cg.inner
                .resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or_else(|| {
            // Default: 1em
            get_element_font_size(styled_dom, id, node_state)
        });

    // Map white-space CSS property to TextWrap
    let text_wrap = styled_dom
        .css_property_cache
        .ptr
        .get_white_space(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|ws| match ws {
            StyleWhiteSpace::Normal => text3::cache::TextWrap::Wrap,
            StyleWhiteSpace::Nowrap => text3::cache::TextWrap::NoWrap,
            StyleWhiteSpace::Pre => text3::cache::TextWrap::NoWrap,
        })
        .unwrap_or(text3::cache::TextWrap::Wrap);

    // Get initial-letter for drop caps
    let initial_letter = styled_dom
        .css_property_cache
        .ptr
        .get_initial_letter(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|il| {
            use std::num::NonZeroUsize;
            let sink = match il.sink {
                azul_css::corety::OptionU32::Some(s) => s,
                azul_css::corety::OptionU32::None => il.size,
            };
            text3::cache::InitialLetter {
                size: il.size as f32,
                sink,
                count: NonZeroUsize::new(1).unwrap(),
            }
        });

    // Get line-clamp for limiting visible lines
    let line_clamp = styled_dom
        .css_property_cache
        .ptr
        .get_line_clamp(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .and_then(|lc| std::num::NonZeroUsize::new(lc.max_lines));

    // Get hanging-punctuation for hanging punctuation marks
    let hanging_punctuation = styled_dom
        .css_property_cache
        .ptr
        .get_hanging_punctuation(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|hp| hp.enabled)
        .unwrap_or(false);

    // Get text-combine-upright for vertical text combination
    let text_combine_upright = styled_dom
        .css_property_cache
        .ptr
        .get_text_combine_upright(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|tcu| match tcu {
            StyleTextCombineUpright::None => text3::cache::TextCombineUpright::None,
            StyleTextCombineUpright::All => text3::cache::TextCombineUpright::All,
            StyleTextCombineUpright::Digits(n) => text3::cache::TextCombineUpright::Digits(*n),
        });

    // Get exclusion-margin for shape exclusions
    let exclusion_margin = styled_dom
        .css_property_cache
        .ptr
        .get_exclusion_margin(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|em| em.inner.get() as f32)
        .unwrap_or(0.0);

    // Get hyphenation-language for language-specific hyphenation
    let hyphenation_language = styled_dom
        .css_property_cache
        .ptr
        .get_hyphenation_language(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .and_then(|hl| {
            #[cfg(feature = "text_layout_hyphenation")]
            {
                use hyphenation::{Language, Load};
                // Parse BCP 47 language code to hyphenation::Language
                match hl.inner.as_str() {
                    "en-US" | "en" => Some(Language::EnglishUS),
                    "de-DE" | "de" => Some(Language::German1996),
                    "fr-FR" | "fr" => Some(Language::French),
                    "es-ES" | "es" => Some(Language::Spanish),
                    "it-IT" | "it" => Some(Language::Italian),
                    "pt-PT" | "pt" => Some(Language::Portuguese),
                    "nl-NL" | "nl" => Some(Language::Dutch),
                    "pl-PL" | "pl" => Some(Language::Polish),
                    "ru-RU" | "ru" => Some(Language::Russian),
                    "zh-CN" | "zh" => Some(Language::Chinese),
                    _ => None, // Unsupported language
                }
            }
            #[cfg(not(feature = "text_layout_hyphenation"))]
            {
                None::<crate::text3::script::Language>
            }
        });

    UnifiedConstraints {
        exclusion_margin,
        hyphenation_language,
        text_indent,
        initial_letter,
        line_clamp,
        columns,
        column_gap,
        hanging_punctuation,
        text_wrap,
        text_combine_upright,
        segment_alignment: SegmentAlignment::Total,
        overflow: match overflow_behaviour {
            LayoutOverflow::Visible => text3::cache::OverflowBehavior::Visible,
            LayoutOverflow::Hidden | LayoutOverflow::Clip => text3::cache::OverflowBehavior::Hidden,
            LayoutOverflow::Scroll => text3::cache::OverflowBehavior::Scroll,
            LayoutOverflow::Auto => text3::cache::OverflowBehavior::Auto,
        },
        available_width: text3::cache::AvailableSpace::from_f32(constraints.available_size.width),
        // For scrollable containers (overflow: scroll/auto), don't constrain height
        // so that the full content is laid out and content_size is calculated correctly.
        available_height: match overflow_behaviour {
            LayoutOverflow::Scroll | LayoutOverflow::Auto => None,
            _ => Some(constraints.available_size.height),
        },
        shape_boundaries, // CSS shape-inside: text flows within shape
        shape_exclusions, // CSS shape-outside + floats: text wraps around shapes
        writing_mode: Some(match writing_mode {
            LayoutWritingMode::HorizontalTb => text3::cache::WritingMode::HorizontalTb,
            LayoutWritingMode::VerticalRl => text3::cache::WritingMode::VerticalRl,
            LayoutWritingMode::VerticalLr => text3::cache::WritingMode::VerticalLr,
        }),
        direction, // Use the CSS direction property (currently defaulting to LTR)
        hyphenation: match hyphenation {
            StyleHyphens::None => false,
            StyleHyphens::Auto => true,
        },
        text_orientation,
        text_align: match text_align {
            StyleTextAlign::Start => text3::cache::TextAlign::Start,
            StyleTextAlign::End => text3::cache::TextAlign::End,
            StyleTextAlign::Left => text3::cache::TextAlign::Left,
            StyleTextAlign::Right => text3::cache::TextAlign::Right,
            StyleTextAlign::Center => text3::cache::TextAlign::Center,
            StyleTextAlign::Justify => text3::cache::TextAlign::Justify,
        },
        text_justify: match text_justify {
            LayoutTextJustify::None => text3::cache::JustifyContent::None,
            LayoutTextJustify::Auto => text3::cache::JustifyContent::None,
            LayoutTextJustify::InterWord => text3::cache::JustifyContent::InterWord,
            LayoutTextJustify::InterCharacter => text3::cache::JustifyContent::InterCharacter,
            LayoutTextJustify::Distribute => text3::cache::JustifyContent::Distribute,
        },
        line_height: line_height_value.inner.normalized() * font_size, /* Resolve line-height relative to font-size */
        vertical_align, // CSS vertical-align property (defaults to Baseline)
    }
}

// Table Formatting Context (CSS 2.2 § 17)

/// Lays out a Table Formatting Context.
/// Table column information for layout calculations
#[derive(Debug, Clone)]
pub struct TableColumnInfo {
    /// Minimum width required for this column
    pub min_width: f32,
    /// Maximum width desired for this column
    pub max_width: f32,
    /// Computed final width for this column
    pub computed_width: Option<f32>,
}

/// Information about a table cell for layout
#[derive(Debug, Clone)]
pub struct TableCellInfo {
    /// Node index in the layout tree
    pub node_index: usize,
    /// Column index (0-based)
    pub column: usize,
    /// Number of columns this cell spans
    pub colspan: usize,
    /// Row index (0-based)
    pub row: usize,
    /// Number of rows this cell spans
    pub rowspan: usize,
}

/// Table layout context - holds all information needed for table layout
#[derive(Debug)]
struct TableLayoutContext {
    /// Information about each column
    columns: Vec<TableColumnInfo>,
    /// Information about each cell
    cells: Vec<TableCellInfo>,
    /// Number of rows in the table
    num_rows: usize,
    /// Whether to use fixed or auto layout algorithm
    use_fixed_layout: bool,
    /// Computed height for each row
    row_heights: Vec<f32>,
    /// Border collapse mode
    border_collapse: StyleBorderCollapse,
    /// Border spacing (only used when border_collapse is Separate)
    border_spacing: LayoutBorderSpacing,
    /// CSS 2.2 Section 17.4: Index of table-caption child, if any
    caption_index: Option<usize>,
    /// CSS 2.2 Section 17.6: Rows with visibility:collapse (dynamic effects)
    /// Set of row indices that have visibility:collapse
    collapsed_rows: std::collections::HashSet<usize>,
    /// CSS 2.2 Section 17.6: Columns with visibility:collapse (dynamic effects)
    /// Set of column indices that have visibility:collapse
    collapsed_columns: std::collections::HashSet<usize>,
}

impl TableLayoutContext {
    fn new() -> Self {
        Self {
            columns: Vec::new(),
            cells: Vec::new(),
            num_rows: 0,
            use_fixed_layout: false,
            row_heights: Vec::new(),
            border_collapse: StyleBorderCollapse::Separate,
            border_spacing: LayoutBorderSpacing::default(),
            caption_index: None,
            collapsed_rows: std::collections::HashSet::new(),
            collapsed_columns: std::collections::HashSet::new(),
        }
    }
}

/// Source of a border in the border conflict resolution algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BorderSource {
    Table = 0,
    ColumnGroup = 1,
    Column = 2,
    RowGroup = 3,
    Row = 4,
    Cell = 5,
}

/// Information about a border for conflict resolution
#[derive(Debug, Clone)]
pub struct BorderInfo {
    pub width: f32,
    pub style: BorderStyle,
    pub color: ColorU,
    pub source: BorderSource,
}

impl BorderInfo {
    pub fn new(width: f32, style: BorderStyle, color: ColorU, source: BorderSource) -> Self {
        Self {
            width,
            style,
            color,
            source,
        }
    }

    /// Get the priority of a border style for conflict resolution
    /// Higher number = higher priority
    pub fn style_priority(style: &BorderStyle) -> u8 {
        match style {
            BorderStyle::Hidden => 255, // Highest - suppresses all borders
            BorderStyle::None => 0,     // Lowest - loses to everything
            BorderStyle::Double => 8,
            BorderStyle::Solid => 7,
            BorderStyle::Dashed => 6,
            BorderStyle::Dotted => 5,
            BorderStyle::Ridge => 4,
            BorderStyle::Outset => 3,
            BorderStyle::Groove => 2,
            BorderStyle::Inset => 1,
        }
    }

    /// Compare two borders for conflict resolution per CSS 2.2 Section 17.6.2.1
    /// Returns the winning border
    pub fn resolve_conflict(a: &BorderInfo, b: &BorderInfo) -> Option<BorderInfo> {
        // 1. 'hidden' wins and suppresses all borders
        if a.style == BorderStyle::Hidden || b.style == BorderStyle::Hidden {
            return None;
        }

        // 2. Filter out 'none' - if both are none, no border
        let a_is_none = a.style == BorderStyle::None;
        let b_is_none = b.style == BorderStyle::None;

        if a_is_none && b_is_none {
            return None;
        }
        if a_is_none {
            return Some(b.clone());
        }
        if b_is_none {
            return Some(a.clone());
        }

        // 3. Wider border wins
        if a.width > b.width {
            return Some(a.clone());
        }
        if b.width > a.width {
            return Some(b.clone());
        }

        // 4. If same width, compare style priority
        let a_priority = Self::style_priority(&a.style);
        let b_priority = Self::style_priority(&b.style);

        if a_priority > b_priority {
            return Some(a.clone());
        }
        if b_priority > a_priority {
            return Some(b.clone());
        }

        // 5. If same style, source priority:
        // Cell > Row > RowGroup > Column > ColumnGroup > Table
        if a.source > b.source {
            return Some(a.clone());
        }
        if b.source > a.source {
            return Some(b.clone());
        }

        // 6. Same priority - prefer first one (left/top in LTR)
        Some(a.clone())
    }
}

/// Get border information for a node
fn get_border_info<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    source: BorderSource,
) -> (BorderInfo, BorderInfo, BorderInfo, BorderInfo) {
    use azul_css::props::{
        basic::{
            pixel::{PhysicalSize, PropertyContext, ResolutionContext},
            ColorU,
        },
        style::BorderStyle,
    };
    use get_element_font_size;
    use get_parent_font_size;
    use get_root_font_size;

    let default_border = BorderInfo::new(
        0.0,
        BorderStyle::None,
        ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        },
        source,
    );

    let Some(dom_id) = node.dom_node_id else {
        return (
            default_border.clone(),
            default_border.clone(),
            default_border.clone(),
            default_border.clone(),
        );
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = StyledNodeState::default();
    let cache = &ctx.styled_dom.css_property_cache.ptr;

    // Create resolution context for border-width (em/rem support, no % support)
    let element_font_size = get_element_font_size(ctx.styled_dom, dom_id, &node_state);
    let parent_font_size = get_parent_font_size(ctx.styled_dom, dom_id, &node_state);
    let root_font_size = get_root_font_size(ctx.styled_dom, &node_state);

    let resolution_context = ResolutionContext {
        element_font_size,
        parent_font_size,
        root_font_size,
        // Not used for border-width
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        // Not used for border-width
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };

    // Top border
    let top = cache
        .get_border_top_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_top_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_top_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    // Right border
    let right = cache
        .get_border_right_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_right_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_right_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    // Bottom border
    let bottom = cache
        .get_border_bottom_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_bottom_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_bottom_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    // Left border
    let left = cache
        .get_border_left_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_left_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_left_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    (top, right, bottom, left)
}

/// Get the table-layout property for a table node
fn get_table_layout_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> LayoutTableLayout {
    let Some(dom_id) = node.dom_node_id else {
        return LayoutTableLayout::Auto;
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = StyledNodeState::default();

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_table_layout(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(LayoutTableLayout::Auto)
}

/// Get the border-collapse property for a table node
fn get_border_collapse_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> StyleBorderCollapse {
    let Some(dom_id) = node.dom_node_id else {
        return StyleBorderCollapse::Separate;
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = StyledNodeState::default();

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_border_collapse(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(StyleBorderCollapse::Separate)
}

/// Get the border-spacing property for a table node
fn get_border_spacing_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> LayoutBorderSpacing {
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();

        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_border_spacing(
            node_data,
            &dom_id,
            &node_state,
        ) {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }

    LayoutBorderSpacing::default() // Default: 0
}

/// CSS 2.2 Section 17.4 - Tables in the visual formatting model:
///
/// "The caption box is a block box that retains its own content, padding,
/// border, and margin areas. The caption-side property specifies the position
/// of the caption box with respect to the table box."
///
/// Get the caption-side property for a table node.
/// Returns Top (default) or Bottom.
fn get_caption_side_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> StyleCaptionSide {
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();

        if let Some(prop) =
            ctx.styled_dom
                .css_property_cache
                .ptr
                .get_caption_side(node_data, &dom_id, &node_state)
        {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }

    StyleCaptionSide::Top // Default per CSS 2.2
}

/// CSS 2.2 Section 17.6 - Dynamic row and column effects:
///
/// "The 'visibility' value 'collapse' removes a row or column from display,
/// but it has a different effect than 'visibility: hidden' on other elements.
/// When a row or column is collapsed, the space normally occupied by the row
/// or column is removed."
///
/// Check if a node has visibility:collapse set.
///
/// This is used for table rows and columns to optimize dynamic hiding.
fn is_visibility_collapsed<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> bool {
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();

        if let Some(prop) =
            ctx.styled_dom
                .css_property_cache
                .ptr
                .get_visibility(node_data, &dom_id, &node_state)
        {
            if let Some(value) = prop.get_property() {
                return matches!(value, StyleVisibility::Collapse);
            }
        }
    }

    false
}

/// CSS 2.2 Section 17.6.1.1 - Borders and Backgrounds around empty cells
///
/// In the separated borders model, the 'empty-cells' property controls the rendering of
/// borders and backgrounds around cells that have no visible content. Empty means it has no
/// children, or has children that are only collapsed whitespace."
///
/// Check if a table cell is empty (has no visible content).
///
/// This is used by the rendering pipeline to decide whether to paint borders/backgrounds
/// when empty-cells: hide is set in separated border model.
///
/// A cell is considered empty if:
///
/// - It has no children, OR
/// - It has children but no inline_layout_result (no rendered content)
///
/// Note: Full whitespace detection would require checking text content during rendering.
/// This function provides a basic check suitable for layout phase.
fn is_cell_empty(tree: &LayoutTree, cell_index: usize) -> bool {
    let cell_node = match tree.get(cell_index) {
        Some(node) => node,
        None => return true, // Invalid cell is considered empty
    };

    // No children = empty
    if cell_node.children.is_empty() {
        return true;
    }

    // If cell has an inline layout result, check if it's empty
    if let Some(ref cached_layout) = cell_node.inline_layout_result {
        // Check if inline layout has any rendered content
        // Empty inline layouts have no items (glyphs/fragments)
        // Note: This is a heuristic - full detection requires text content analysis
        return cached_layout.layout.items.is_empty();
    }

    // Check if all children have no content
    // A more thorough check would recursively examine all descendants
    //
    // For now, we use a simple heuristic: if there are children, assume not empty
    // unless proven otherwise by inline_layout_result

    // Cell with children but no inline layout = likely has block-level content = not empty
    false
}

/// Main function to layout a table formatting context
pub fn layout_table_fc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    debug_log!(ctx, "Laying out table");

    debug_table_layout!(
        ctx,
        "node_index={}, available_size={:?}, writing_mode={:?}",
        node_index,
        constraints.available_size,
        constraints.writing_mode
    );

    // Multi-pass table layout algorithm:
    //
    // 1. Analyze table structure - identify rows, cells, columns
    // 2. Determine table-layout property (fixed vs auto)
    // 3. Calculate column widths
    // 4. Layout cells and calculate row heights
    // 5. Position cells in final grid

    // Get the table node to read CSS properties
    let table_node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone();

    // Calculate the table's border-box width for column distribution
    // This accounts for the table's own width property (e.g., width: 100%)
    let table_border_box_width = if let Some(dom_id) = table_node.dom_node_id {
        // Use calculate_used_size_for_node to resolve table width (respects width:100%)
        let intrinsic = table_node.intrinsic_sizes.clone().unwrap_or_default();
        let containing_block_size = LogicalSize {
            width: constraints.available_size.width,
            height: constraints.available_size.height,
        };

        let table_size = crate::solver3::sizing::calculate_used_size_for_node(
            ctx.styled_dom,
            Some(dom_id),
            containing_block_size,
            intrinsic,
            &table_node.box_props,
        )?;

        table_size.width
    } else {
        constraints.available_size.width
    };

    // Subtract padding and border to get content-box width for column distribution
    let table_content_box_width = {
        let padding_width = table_node.box_props.padding.left + table_node.box_props.padding.right;
        let border_width = table_node.box_props.border.left + table_node.box_props.border.right;
        (table_border_box_width - padding_width - border_width).max(0.0)
    };

    debug_table_layout!(ctx, "Table Layout Debug");
    debug_table_layout!(ctx, "Node index: {}", node_index);
    debug_table_layout!(
        ctx,
        "Available size from parent: {:.2} x {:.2}",
        constraints.available_size.width,
        constraints.available_size.height
    );
    debug_table_layout!(ctx, "Table border-box width: {:.2}", table_border_box_width);
    debug_table_layout!(
        ctx,
        "Table content-box width: {:.2}",
        table_content_box_width
    );
    debug_table_layout!(
        ctx,
        "Table padding: L={:.2} R={:.2}",
        table_node.box_props.padding.left,
        table_node.box_props.padding.right
    );
    debug_table_layout!(
        ctx,
        "Table border: L={:.2} R={:.2}",
        table_node.box_props.border.left,
        table_node.box_props.border.right
    );
    debug_table_layout!(ctx, "=");

    // Phase 1: Analyze table structure
    let mut table_ctx = analyze_table_structure(tree, node_index, ctx)?;

    // Phase 2: Read CSS properties and determine layout algorithm
    let table_layout = get_table_layout_property(ctx, &table_node);
    table_ctx.use_fixed_layout = matches!(table_layout, LayoutTableLayout::Fixed);

    // Read border properties
    table_ctx.border_collapse = get_border_collapse_property(ctx, &table_node);
    table_ctx.border_spacing = get_border_spacing_property(ctx, &table_node);

    debug_log!(
        ctx,
        "Table layout: {:?}, border-collapse: {:?}, border-spacing: {:?}",
        table_layout,
        table_ctx.border_collapse,
        table_ctx.border_spacing
    );

    // Phase 3: Calculate column widths
    if table_ctx.use_fixed_layout {
        // DEBUG: Log available width passed into fixed column calculation
        debug_table_layout!(
            ctx,
            "FIXED layout: table_content_box_width={:.2}",
            table_content_box_width
        );
        calculate_column_widths_fixed(ctx, &mut table_ctx, table_content_box_width);
    } else {
        // Pass table_content_box_width for column distribution in auto layout
        calculate_column_widths_auto_with_width(
            &mut table_ctx,
            tree,
            text_cache,
            ctx,
            constraints,
            table_content_box_width,
        )?;
    }

    debug_table_layout!(ctx, "After column width calculation:");
    debug_table_layout!(ctx, "  Number of columns: {}", table_ctx.columns.len());
    for (i, col) in table_ctx.columns.iter().enumerate() {
        debug_table_layout!(
            ctx,
            "  Column {}: width={:.2}",
            i,
            col.computed_width.unwrap_or(0.0)
        );
    }
    let total_col_width: f32 = table_ctx
        .columns
        .iter()
        .filter_map(|c| c.computed_width)
        .sum();
    debug_table_layout!(ctx, "  Total column width: {:.2}", total_col_width);

    // Phase 4: Calculate row heights based on cell content
    calculate_row_heights(&mut table_ctx, tree, text_cache, ctx, constraints)?;

    // Phase 5: Position cells in final grid and collect positions
    let mut cell_positions =
        position_table_cells(&mut table_ctx, tree, ctx, node_index, constraints)?;

    // Calculate final table size including border-spacing
    let mut table_width: f32 = table_ctx
        .columns
        .iter()
        .filter_map(|col| col.computed_width)
        .sum();
    let mut table_height: f32 = table_ctx.row_heights.iter().sum();

    debug_table_layout!(
        ctx,
        "After calculate_row_heights: table_height={:.2}, row_heights={:?}",
        table_height,
        table_ctx.row_heights
    );

    // Add border-spacing to table size if border-collapse is separate
    if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        use get_element_font_size;
        use get_parent_font_size;
        use get_root_font_size;
        use PhysicalSize;
        use PropertyContext;
        use ResolutionContext;

        let styled_dom = ctx.styled_dom;
        let table_id = tree.nodes[node_index].dom_node_id.unwrap();
        let table_state = &styled_dom.styled_nodes.as_container()[table_id].styled_node_state;

        let spacing_context = ResolutionContext {
            element_font_size: get_element_font_size(styled_dom, table_id, table_state),
            parent_font_size: get_parent_font_size(styled_dom, table_id, table_state),
            root_font_size: get_root_font_size(styled_dom, table_state),
            containing_block_size: PhysicalSize::new(0.0, 0.0),
            element_size: None,
            // TODO: Get actual DPI scale from ctx
            viewport_size: PhysicalSize::new(0.0, 0.0),
        };

        let h_spacing = table_ctx
            .border_spacing
            .horizontal
            .resolve_with_context(&spacing_context, PropertyContext::Other);
        let v_spacing = table_ctx
            .border_spacing
            .vertical
            .resolve_with_context(&spacing_context, PropertyContext::Other);

        // Add spacing: left + (n-1 between columns) + right = n+1 spacings
        let num_cols = table_ctx.columns.len();
        if num_cols > 0 {
            table_width += h_spacing * (num_cols + 1) as f32;
        }

        // Add spacing: top + (n-1 between rows) + bottom = n+1 spacings
        if table_ctx.num_rows > 0 {
            table_height += v_spacing * (table_ctx.num_rows + 1) as f32;
        }
    }

    // CSS 2.2 Section 17.4: Layout and position the caption if present
    //
    // "The caption box is a block box that retains its own content,
    // padding, border, and margin areas."
    let caption_side = get_caption_side_property(ctx, &table_node);
    let mut caption_height = 0.0;
    let mut table_y_offset = 0.0;

    if let Some(caption_idx) = table_ctx.caption_index {
        debug_log!(
            ctx,
            "Laying out caption with caption-side: {:?}",
            caption_side
        );

        // Layout caption as a block with the table's width as available width
        let caption_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: table_width,
                height: constraints.available_size.height,
            },
            writing_mode: constraints.writing_mode,
            bfc_state: None, // Caption creates its own BFC
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            available_width_type: Text3AvailableSpace::Definite(table_width),
        };

        // Layout the caption node
        let mut empty_float_cache = std::collections::BTreeMap::new();
        let caption_result = layout_formatting_context(
            ctx,
            tree,
            text_cache,
            caption_idx,
            &caption_constraints,
            &mut empty_float_cache,
        )?;
        caption_height = caption_result.output.overflow_size.height;

        // Position caption based on caption-side property
        let caption_position = match caption_side {
            StyleCaptionSide::Top => {
                // Caption on top: position at y=0, table starts below caption
                table_y_offset = caption_height;
                LogicalPosition { x: 0.0, y: 0.0 }
            }
            StyleCaptionSide::Bottom => {
                // Caption on bottom: table starts at y=0, caption below table
                LogicalPosition {
                    x: 0.0,
                    y: table_height,
                }
            }
        };

        // Add caption position to the positions map
        cell_positions.insert(caption_idx, caption_position);

        debug_log!(
            ctx,
            "Caption positioned at x={:.2}, y={:.2}, height={:.2}",
            caption_position.x,
            caption_position.y,
            caption_height
        );
    }

    // Adjust all table cell positions if caption is on top
    if table_y_offset > 0.0 {
        debug_log!(
            ctx,
            "Adjusting table cells by y offset: {:.2}",
            table_y_offset
        );

        // Adjust cell positions in the map
        for cell_info in &table_ctx.cells {
            if let Some(pos) = cell_positions.get_mut(&cell_info.node_index) {
                pos.y += table_y_offset;
            }
        }
    }

    // Total table height includes caption
    let total_height = table_height + caption_height;

    debug_table_layout!(ctx, "Final table dimensions:");
    debug_table_layout!(ctx, "  Content width (columns): {:.2}", table_width);
    debug_table_layout!(ctx, "  Content height (rows): {:.2}", table_height);
    debug_table_layout!(ctx, "  Caption height: {:.2}", caption_height);
    debug_table_layout!(ctx, "  Total height: {:.2}", total_height);
    debug_table_layout!(ctx, "End Table Debug");

    // Create output with the table's final size and cell positions
    let output = LayoutOutput {
        overflow_size: LogicalSize {
            width: table_width,
            height: total_height,
        },
        // Cell positions calculated in position_table_cells
        positions: cell_positions,
        // Tables don't have a baseline
        baseline: None,
    };

    Ok(output)
}

/// Analyze the table structure to identify rows, cells, and columns
fn analyze_table_structure<T: ParsedFontTrait>(
    tree: &LayoutTree,
    table_index: usize,
    ctx: &mut LayoutContext<'_, T>,
) -> Result<TableLayoutContext> {
    let mut table_ctx = TableLayoutContext::new();

    let table_node = tree.get(table_index).ok_or(LayoutError::InvalidTree)?;

    // CSS 2.2 Section 17.4: A table may have one table-caption child.
    // Traverse children to find caption, columns/colgroups, rows, and row groups
    for &child_idx in &table_node.children {
        if let Some(child) = tree.get(child_idx) {
            // Check if this is a table caption
            if matches!(child.formatting_context, FormattingContext::TableCaption) {
                debug_log!(ctx, "Found table caption at index {}", child_idx);
                table_ctx.caption_index = Some(child_idx);
                continue;
            }

            // CSS 2.2 Section 17.2: Check for column groups
            if matches!(
                child.formatting_context,
                FormattingContext::TableColumnGroup
            ) {
                analyze_table_colgroup(tree, child_idx, &mut table_ctx, ctx)?;
                continue;
            }

            // Check if this is a table row or row group
            match child.formatting_context {
                FormattingContext::TableRow => {
                    analyze_table_row(tree, child_idx, &mut table_ctx, ctx)?;
                }
                FormattingContext::TableRowGroup => {
                    // Process rows within the row group
                    for &row_idx in &child.children {
                        if let Some(row) = tree.get(row_idx) {
                            if matches!(row.formatting_context, FormattingContext::TableRow) {
                                analyze_table_row(tree, row_idx, &mut table_ctx, ctx)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    debug_log!(
        ctx,
        "Table structure: {} rows, {} columns, {} cells{}",
        table_ctx.num_rows,
        table_ctx.columns.len(),
        table_ctx.cells.len(),
        if table_ctx.caption_index.is_some() {
            ", has caption"
        } else {
            ""
        }
    );

    Ok(table_ctx)
}

/// Analyze a table column group to identify columns and track collapsed columns
///
/// - CSS 2.2 Section 17.2: Column groups contain columns
/// - CSS 2.2 Section 17.6: Columns can have visibility:collapse
fn analyze_table_colgroup<T: ParsedFontTrait>(
    tree: &LayoutTree,
    colgroup_index: usize,
    table_ctx: &mut TableLayoutContext,
    ctx: &mut LayoutContext<'_, T>,
) -> Result<()> {
    let colgroup_node = tree.get(colgroup_index).ok_or(LayoutError::InvalidTree)?;

    // Check if the colgroup itself has visibility:collapse
    if is_visibility_collapsed(ctx, colgroup_node) {
        // All columns in this group should be collapsed
        // TODO: For now, just mark the group (actual column indices will be determined later)
        debug_log!(
            ctx,
            "Column group at index {} has visibility:collapse",
            colgroup_index
        );
    }

    // Check for individual column elements within the group
    for &col_idx in &colgroup_node.children {
        if let Some(col_node) = tree.get(col_idx) {
            // Note: Individual columns don't have a FormattingContext::TableColumn
            // They are represented as children of TableColumnGroup
            // Check visibility:collapse on each column
            if is_visibility_collapsed(ctx, col_node) {
                // We need to determine the actual column index this represents
                // For now, we'll track it during cell analysis
                debug_log!(ctx, "Column at index {} has visibility:collapse", col_idx);
            }
        }
    }

    Ok(())
}

/// Analyze a table row to identify cells and update column count
fn analyze_table_row<T: ParsedFontTrait>(
    tree: &LayoutTree,
    row_index: usize,
    table_ctx: &mut TableLayoutContext,
    ctx: &mut LayoutContext<'_, T>,
) -> Result<()> {
    let row_node = tree.get(row_index).ok_or(LayoutError::InvalidTree)?;
    let row_num = table_ctx.num_rows;
    table_ctx.num_rows += 1;

    // CSS 2.2 Section 17.6: Check if this row has visibility:collapse
    if is_visibility_collapsed(ctx, row_node) {
        debug_log!(ctx, "Row {} has visibility:collapse", row_num);
        table_ctx.collapsed_rows.insert(row_num);
    }

    let mut col_index = 0;

    for &cell_idx in &row_node.children {
        if let Some(cell) = tree.get(cell_idx) {
            if matches!(cell.formatting_context, FormattingContext::TableCell) {
                // Get colspan and rowspan (TODO: from CSS properties)
                let colspan = 1; // TODO: Get from CSS
                let rowspan = 1; // TODO: Get from CSS

                let cell_info = TableCellInfo {
                    node_index: cell_idx,
                    column: col_index,
                    colspan,
                    row: row_num,
                    rowspan,
                };

                table_ctx.cells.push(cell_info);

                // Update column count
                let max_col = col_index + colspan;
                while table_ctx.columns.len() < max_col {
                    table_ctx.columns.push(TableColumnInfo {
                        min_width: 0.0,
                        max_width: 0.0,
                        computed_width: None,
                    });
                }

                col_index += colspan;
            }
        }
    }

    Ok(())
}

/// Calculate column widths using the fixed table layout algorithm
///
/// CSS 2.2 Section 17.5.2.1: In fixed table layout, the table width is
/// not dependent on cell contents
///
/// CSS 2.2 Section 17.6: Columns with visibility:collapse are excluded
/// from width calculations
fn calculate_column_widths_fixed<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    table_ctx: &mut TableLayoutContext,
    available_width: f32,
) {
    debug_table_layout!(
        ctx,
        "calculate_column_widths_fixed: num_cols={}, available_width={:.2}",
        table_ctx.columns.len(),
        available_width
    );

    // Fixed layout: distribute width equally among non-collapsed columns
    // TODO: Respect column width properties and first-row cell widths
    let num_cols = table_ctx.columns.len();
    if num_cols == 0 {
        return;
    }

    // Count non-collapsed columns
    let num_visible_cols = num_cols - table_ctx.collapsed_columns.len();
    if num_visible_cols == 0 {
        // All columns collapsed - set all to zero width
        for col in &mut table_ctx.columns {
            col.computed_width = Some(0.0);
        }
        return;
    }

    // Distribute width only among visible columns
    let col_width = available_width / num_visible_cols as f32;
    for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
        if table_ctx.collapsed_columns.contains(&col_idx) {
            col.computed_width = Some(0.0);
        } else {
            col.computed_width = Some(col_width);
        }
    }
}

/// Measure a cell's minimum content width (with maximum wrapping)
fn measure_cell_min_content_width<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    cell_index: usize,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    // CSS 2.2 Section 17.5.2.2: "Calculate the minimum content width (MCW) of each cell"
    //
    // Min-content width is the width with maximum wrapping.
    // Use AvailableSpace::MinContent to signal intrinsic min-content sizing to the
    // text layout engine.
    use crate::text3::cache::AvailableSpace;
    let min_constraints = LayoutConstraints {
        available_size: LogicalSize {
            width: AvailableSpace::MinContent.to_f32_for_layout(),
            height: f32::INFINITY,
        },
        writing_mode: constraints.writing_mode,
        bfc_state: None, // Don't propagate BFC state for measurement
        text_align: constraints.text_align,
        containing_block_size: constraints.containing_block_size,
        // CRITICAL: Mark this as min-content measurement, not definite width!
        // This ensures the cached layout won't be incorrectly reused for final rendering.
        available_width_type: Text3AvailableSpace::MinContent,
    };

    let mut temp_positions = BTreeMap::new();
    let mut temp_scrollbar_reflow = false;
    let mut temp_float_cache = std::collections::BTreeMap::new();

    crate::solver3::cache::calculate_layout_for_subtree(
        ctx,
        tree,
        text_cache,
        cell_index,
        LogicalPosition::zero(),
        min_constraints.available_size,
        &mut temp_positions,
        &mut temp_scrollbar_reflow,
        &mut temp_float_cache,
    )?;

    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let size = cell_node.used_size.unwrap_or_default();

    // Add padding and border to get the total minimum width
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;

    let min_width = size.width
        + padding.cross_start(writing_mode)
        + padding.cross_end(writing_mode)
        + border.cross_start(writing_mode)
        + border.cross_end(writing_mode);

    Ok(min_width)
}

/// Measure a cell's maximum content width (without wrapping)
fn measure_cell_max_content_width<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    cell_index: usize,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    // CSS 2.2 Section 17.5.2.2: "Calculate the maximum content width (MCW) of each cell"
    //
    // Max-content width is the width without any wrapping.
    // Use AvailableSpace::MaxContent to signal intrinsic max-content sizing to
    // the text layout engine.
    use crate::text3::cache::AvailableSpace;
    let max_constraints = LayoutConstraints {
        available_size: LogicalSize {
            width: AvailableSpace::MaxContent.to_f32_for_layout(),
            height: f32::INFINITY,
        },
        writing_mode: constraints.writing_mode,
        bfc_state: None, // Don't propagate BFC state for measurement
        text_align: constraints.text_align,
        containing_block_size: constraints.containing_block_size,
        // CRITICAL: Mark this as max-content measurement, not definite width!
        // This ensures the cached layout won't be incorrectly reused for final rendering.
        available_width_type: Text3AvailableSpace::MaxContent,
    };

    let mut temp_positions = BTreeMap::new();
    let mut temp_scrollbar_reflow = false;
    let mut temp_float_cache = std::collections::BTreeMap::new();

    crate::solver3::cache::calculate_layout_for_subtree(
        ctx,
        tree,
        text_cache,
        cell_index,
        LogicalPosition::zero(),
        max_constraints.available_size,
        &mut temp_positions,
        &mut temp_scrollbar_reflow,
        &mut temp_float_cache,
    )?;

    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let size = cell_node.used_size.unwrap_or_default();

    // Add padding and border to get the total maximum width
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;

    let max_width = size.width
        + padding.cross_start(writing_mode)
        + padding.cross_end(writing_mode)
        + border.cross_start(writing_mode)
        + border.cross_end(writing_mode);

    Ok(max_width)
}

/// Calculate column widths using the auto table layout algorithm
fn calculate_column_widths_auto<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints,
) -> Result<()> {
    calculate_column_widths_auto_with_width(
        table_ctx,
        tree,
        text_cache,
        ctx,
        constraints,
        constraints.available_size.width,
    )
}

/// Calculate column widths using the auto table layout algorithm with explicit table width
fn calculate_column_widths_auto_with_width<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints,
    table_width: f32,
) -> Result<()> {
    // Auto layout: calculate min/max content width for each cell
    let num_cols = table_ctx.columns.len();
    if num_cols == 0 {
        return Ok(());
    }

    // Step 1: Measure all cells to determine column min/max widths
    // CSS 2.2 Section 17.6: Skip cells in collapsed columns
    for cell_info in &table_ctx.cells {
        // Skip cells in collapsed columns
        if table_ctx.collapsed_columns.contains(&cell_info.column) {
            continue;
        }

        // Skip cells that span into collapsed columns
        let mut spans_collapsed = false;
        for col_offset in 0..cell_info.colspan {
            if table_ctx
                .collapsed_columns
                .contains(&(cell_info.column + col_offset))
            {
                spans_collapsed = true;
                break;
            }
        }
        if spans_collapsed {
            continue;
        }

        let min_width = measure_cell_min_content_width(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            constraints,
        )?;

        let max_width = measure_cell_max_content_width(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            constraints,
        )?;

        // Handle single-column cells
        if cell_info.colspan == 1 {
            let col = &mut table_ctx.columns[cell_info.column];
            col.min_width = col.min_width.max(min_width);
            col.max_width = col.max_width.max(max_width);
        } else {
            // Handle multi-column cells (colspan > 1)
            // Distribute the cell's min/max width across the spanned columns
            distribute_cell_width_across_columns(
                &mut table_ctx.columns,
                cell_info.column,
                cell_info.colspan,
                min_width,
                max_width,
                &table_ctx.collapsed_columns,
            );
        }
    }

    // Step 2: Calculate final column widths based on available space
    // Exclude collapsed columns from total width calculations
    let total_min_width: f32 = table_ctx
        .columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !table_ctx.collapsed_columns.contains(idx))
        .map(|(_, c)| c.min_width)
        .sum();
    let total_max_width: f32 = table_ctx
        .columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !table_ctx.collapsed_columns.contains(idx))
        .map(|(_, c)| c.max_width)
        .sum();
    let available_width = table_width; // Use table's content-box width, not constraints

    debug_table_layout!(
        ctx,
        "calculate_column_widths_auto: min={:.2}, max={:.2}, table_width={:.2}",
        total_min_width,
        total_max_width,
        table_width
    );

    // Handle infinity and NaN cases
    if !total_max_width.is_finite() || !available_width.is_finite() {
        // If max_width is infinite or unavailable, distribute available width equally
        let num_non_collapsed = table_ctx.columns.len() - table_ctx.collapsed_columns.len();
        let width_per_column = if num_non_collapsed > 0 {
            available_width / num_non_collapsed as f32
        } else {
            0.0
        };

        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                // Use the larger of min_width and equal distribution
                col.computed_width = Some(col.min_width.max(width_per_column));
            }
        }
    } else if available_width >= total_max_width {
        // Case 1: More space than max-content - distribute excess proportionally
        //
        // CSS 2.1 Section 17.5.2.2: Distribute extra space proportionally to
        // max-content widths
        let excess_width = available_width - total_max_width;

        // First pass: collect column info (max_width) to avoid borrowing issues
        let column_info: Vec<(usize, f32, bool)> = table_ctx
            .columns
            .iter()
            .enumerate()
            .map(|(idx, c)| (idx, c.max_width, table_ctx.collapsed_columns.contains(&idx)))
            .collect();

        // Calculate total weight for proportional distribution (use max_width as weight)
        let total_weight: f32 = column_info.iter()
            .filter(|(_, _, is_collapsed)| !is_collapsed)
            .map(|(_, max_w, _)| max_w.max(1.0)) // Avoid division by zero
            .sum();

        let num_non_collapsed = column_info
            .iter()
            .filter(|(_, _, is_collapsed)| !is_collapsed)
            .count();

        // Second pass: set computed widths
        for (col_idx, max_width, is_collapsed) in column_info {
            let col = &mut table_ctx.columns[col_idx];
            if is_collapsed {
                col.computed_width = Some(0.0);
            } else {
                // Start with max-content width, then add proportional share of excess
                let weight_factor = if total_weight > 0.0 {
                    max_width.max(1.0) / total_weight
                } else {
                    // If all columns have 0 max_width, distribute equally
                    1.0 / num_non_collapsed.max(1) as f32
                };

                let final_width = max_width + (excess_width * weight_factor);
                col.computed_width = Some(final_width);
            }
        }
    } else if available_width >= total_min_width {
        // Case 2: Between min and max - interpolate proportionally
        // Avoid division by zero if min == max
        let scale = if total_max_width > total_min_width {
            (available_width - total_min_width) / (total_max_width - total_min_width)
        } else {
            0.0 // If min == max, just use min width
        };
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                let interpolated = col.min_width + (col.max_width - col.min_width) * scale;
                col.computed_width = Some(interpolated);
            }
        }
    } else {
        // Case 3: Not enough space - scale down from min widths
        let scale = available_width / total_min_width;
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                col.computed_width = Some(col.min_width * scale);
            }
        }
    }

    Ok(())
}

/// Distribute a multi-column cell's width across the columns it spans
fn distribute_cell_width_across_columns(
    columns: &mut [TableColumnInfo],
    start_col: usize,
    colspan: usize,
    cell_min_width: f32,
    cell_max_width: f32,
    collapsed_columns: &std::collections::HashSet<usize>,
) {
    let end_col = start_col + colspan;
    if end_col > columns.len() {
        return;
    }

    // Calculate current total of spanned non-collapsed columns
    let current_min_total: f32 = columns[start_col..end_col]
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(&(start_col + idx)))
        .map(|(_, c)| c.min_width)
        .sum();
    let current_max_total: f32 = columns[start_col..end_col]
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(&(start_col + idx)))
        .map(|(_, c)| c.max_width)
        .sum();

    // Count non-collapsed columns in the span
    let num_visible_cols = (start_col..end_col)
        .filter(|idx| !collapsed_columns.contains(idx))
        .count();

    if num_visible_cols == 0 {
        return; // All spanned columns are collapsed
    }

    // Only distribute if the cell needs more space than currently available
    if cell_min_width > current_min_total {
        let extra_min = cell_min_width - current_min_total;
        let per_col = extra_min / num_visible_cols as f32;
        for (idx, col) in columns[start_col..end_col].iter_mut().enumerate() {
            if !collapsed_columns.contains(&(start_col + idx)) {
                col.min_width += per_col;
            }
        }
    }

    if cell_max_width > current_max_total {
        let extra_max = cell_max_width - current_max_total;
        let per_col = extra_max / num_visible_cols as f32;
        for (idx, col) in columns[start_col..end_col].iter_mut().enumerate() {
            if !collapsed_columns.contains(&(start_col + idx)) {
                col.max_width += per_col;
            }
        }
    }
}

/// Layout a cell with its computed column width to determine its content height
fn layout_cell_for_height<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    cell_index: usize,
    cell_width: f32,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let cell_dom_id = cell_node.dom_node_id.ok_or(LayoutError::InvalidTree)?;

    // Check if cell has text content directly in DOM (not in LayoutTree)
    // Text nodes are intentionally not included in LayoutTree per CSS spec,
    // but we need to measure them for table cell height calculation.
    let has_text_children = cell_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .any(|child_id| {
            let node_data = &ctx.styled_dom.node_data.as_container()[child_id];
            matches!(node_data.get_node_type(), NodeType::Text(_))
        });

    debug_table_layout!(
        ctx,
        "layout_cell_for_height: cell_index={}, has_text_children={}",
        cell_index,
        has_text_children
    );

    // Get padding and border to calculate content width
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;

    // cell_width is the border-box width (includes padding/border from column
    // width calculation) but layout functions need content-box width
    let content_width = cell_width
        - padding.cross_start(writing_mode)
        - padding.cross_end(writing_mode)
        - border.cross_start(writing_mode)
        - border.cross_end(writing_mode);

    debug_table_layout!(
        ctx,
        "Cell width: border_box={:.2}, content_box={:.2}",
        cell_width,
        content_width
    );

    let content_height = if has_text_children {
        // Cell contains text - use IFC to measure it
        debug_table_layout!(ctx, "Using IFC to measure text content");

        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width, // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            bfc_state: None,
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            // Use definite width for final cell layout!
            // This replaces any previous MinContent/MaxContent measurement.
            available_width_type: Text3AvailableSpace::Definite(content_width),
        };

        let output = layout_ifc(ctx, text_cache, tree, cell_index, &cell_constraints)?;

        debug_table_layout!(
            ctx,
            "IFC returned height={:.2}",
            output.overflow_size.height
        );

        output.overflow_size.height
    } else {
        // Cell contains block-level children or is empty - use regular layout
        debug_table_layout!(ctx, "Using regular layout for block children");

        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width, // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            bfc_state: None,
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            // Use Definite width for final cell layout!
            available_width_type: Text3AvailableSpace::Definite(content_width),
        };

        let mut temp_positions = BTreeMap::new();
        let mut temp_scrollbar_reflow = false;
        let mut temp_float_cache = std::collections::BTreeMap::new();

        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            cell_index,
            LogicalPosition::zero(),
            cell_constraints.available_size,
            &mut temp_positions,
            &mut temp_scrollbar_reflow,
            &mut temp_float_cache,
        )?;

        let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
        cell_node.used_size.unwrap_or_default().height
    };

    // Add padding and border to get the total height
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;

    let total_height = content_height
        + padding.main_start(writing_mode)
        + padding.main_end(writing_mode)
        + border.main_start(writing_mode)
        + border.main_end(writing_mode);

    debug_table_layout!(
        ctx,
        "Cell total height: cell_index={}, content={:.2}, padding/border={:.2}, total={:.2}",
        cell_index,
        content_height,
        padding.main_start(writing_mode)
            + padding.main_end(writing_mode)
            + border.main_start(writing_mode)
            + border.main_end(writing_mode),
        total_height
    );

    Ok(total_height)
}

/// Calculate row heights based on cell content after column widths are determined
fn calculate_row_heights<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints,
) -> Result<()> {
    debug_table_layout!(
        ctx,
        "calculate_row_heights: num_rows={}, available_size={:?}",
        table_ctx.num_rows,
        constraints.available_size
    );

    // Initialize row heights
    table_ctx.row_heights = vec![0.0; table_ctx.num_rows];

    // CSS 2.2 Section 17.6: Set collapsed rows to height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }

    // First pass: Calculate heights for cells that don't span multiple rows
    for cell_info in &table_ctx.cells {
        // Skip cells in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }

        // Get the cell's width (sum of column widths if colspan > 1)
        let mut cell_width = 0.0;
        for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
            if let Some(col) = table_ctx.columns.get(col_idx) {
                if let Some(width) = col.computed_width {
                    cell_width += width;
                }
            }
        }

        debug_table_layout!(
            ctx,
            "Cell layout: node_index={}, row={}, col={}, width={:.2}",
            cell_info.node_index,
            cell_info.row,
            cell_info.column,
            cell_width
        );

        // Layout the cell to get its height
        let cell_height = layout_cell_for_height(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            cell_width,
            constraints,
        )?;

        debug_table_layout!(
            ctx,
            "Cell height calculated: node_index={}, height={:.2}",
            cell_info.node_index,
            cell_height
        );

        // For single-row cells, update the row height
        if cell_info.rowspan == 1 {
            let current_height = table_ctx.row_heights[cell_info.row];
            table_ctx.row_heights[cell_info.row] = current_height.max(cell_height);
        }
    }

    // Second pass: Handle cells that span multiple rows (rowspan > 1)
    for cell_info in &table_ctx.cells {
        // Skip cells that start in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }

        if cell_info.rowspan > 1 {
            // Get the cell's width
            let mut cell_width = 0.0;
            for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
                if let Some(col) = table_ctx.columns.get(col_idx) {
                    if let Some(width) = col.computed_width {
                        cell_width += width;
                    }
                }
            }

            // Layout the cell to get its height
            let cell_height = layout_cell_for_height(
                ctx,
                tree,
                text_cache,
                cell_info.node_index,
                cell_width,
                constraints,
            )?;

            // Calculate the current total height of spanned rows (excluding collapsed rows)
            let end_row = cell_info.row + cell_info.rowspan;
            let current_total: f32 = table_ctx.row_heights[cell_info.row..end_row]
                .iter()
                .enumerate()
                .filter(|(idx, _)| !table_ctx.collapsed_rows.contains(&(cell_info.row + idx)))
                .map(|(_, height)| height)
                .sum();

            // If the cell needs more height, distribute extra height across
            // non-collapsed spanned rows
            if cell_height > current_total {
                let extra_height = cell_height - current_total;

                // Count non-collapsed rows in span
                let non_collapsed_rows = (cell_info.row..end_row)
                    .filter(|row_idx| !table_ctx.collapsed_rows.contains(row_idx))
                    .count();

                if non_collapsed_rows > 0 {
                    let per_row = extra_height / non_collapsed_rows as f32;

                    for row_idx in cell_info.row..end_row {
                        if !table_ctx.collapsed_rows.contains(&row_idx) {
                            table_ctx.row_heights[row_idx] += per_row;
                        }
                    }
                }
            }
        }
    }

    // CSS 2.2 Section 17.6: Final pass - ensure collapsed rows have height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }

    Ok(())
}

/// Position all cells in the table grid with calculated widths and heights
fn position_table_cells<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    ctx: &mut LayoutContext<'_, T>,
    table_index: usize,
    constraints: &LayoutConstraints,
) -> Result<BTreeMap<usize, LogicalPosition>> {
    debug_log!(ctx, "Positioning table cells in grid");

    let mut positions = BTreeMap::new();

    // Get border spacing values if border-collapse is separate
    let (h_spacing, v_spacing) = if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        let styled_dom = ctx.styled_dom;
        let table_id = tree.nodes[table_index].dom_node_id.unwrap();
        let table_state = &styled_dom.styled_nodes.as_container()[table_id].styled_node_state;

        let spacing_context = ResolutionContext {
            element_font_size: get_element_font_size(styled_dom, table_id, table_state),
            parent_font_size: get_parent_font_size(styled_dom, table_id, table_state),
            root_font_size: get_root_font_size(styled_dom, table_state),
            containing_block_size: PhysicalSize::new(0.0, 0.0),
            element_size: None,
            viewport_size: PhysicalSize::new(0.0, 0.0), // TODO: Get actual DPI scale from ctx
        };

        let h = table_ctx
            .border_spacing
            .horizontal
            .resolve_with_context(&spacing_context, PropertyContext::Other);

        let v = table_ctx
            .border_spacing
            .vertical
            .resolve_with_context(&spacing_context, PropertyContext::Other);

        (h, v)
    } else {
        (0.0, 0.0)
    };

    debug_log!(
        ctx,
        "Border spacing: h={:.2}, v={:.2}",
        h_spacing,
        v_spacing
    );

    // Calculate cumulative column positions (x-offsets) with spacing
    let mut col_positions = vec![0.0; table_ctx.columns.len()];
    let mut x_offset = h_spacing; // Start with spacing on the left
    for (i, col) in table_ctx.columns.iter().enumerate() {
        col_positions[i] = x_offset;
        if let Some(width) = col.computed_width {
            x_offset += width + h_spacing; // Add spacing between columns
        }
    }

    // Calculate cumulative row positions (y-offsets) with spacing
    let mut row_positions = vec![0.0; table_ctx.num_rows];
    let mut y_offset = v_spacing; // Start with spacing on the top
    for (i, &height) in table_ctx.row_heights.iter().enumerate() {
        row_positions[i] = y_offset;
        y_offset += height + v_spacing; // Add spacing between rows
    }

    // Position each cell
    for cell_info in &table_ctx.cells {
        let cell_node = tree
            .get_mut(cell_info.node_index)
            .ok_or(LayoutError::InvalidTree)?;

        // Calculate cell position
        let x = col_positions.get(cell_info.column).copied().unwrap_or(0.0);
        let y = row_positions.get(cell_info.row).copied().unwrap_or(0.0);

        // Calculate cell size (sum of spanned columns/rows)
        let mut width = 0.0;
        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: calculating width from cols {}..{}",
            cell_info.node_index,
            cell_info.column,
            cell_info.column + cell_info.colspan
        );
        for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
            if let Some(col) = table_ctx.columns.get(col_idx) {
                debug_info!(
                    ctx,
                    "[position_table_cells]   Col {}: computed_width={:?}",
                    col_idx,
                    col.computed_width
                );
                if let Some(col_width) = col.computed_width {
                    width += col_width;
                    // Add spacing between spanned columns (but not after the last one)
                    if col_idx < cell_info.column + cell_info.colspan - 1 {
                        width += h_spacing;
                    }
                } else {
                    debug_info!(
                        ctx,
                        "[position_table_cells]   WARN:  Col {} has NO computed_width!",
                        col_idx
                    );
                }
            } else {
                debug_info!(
                    ctx,
                    "[position_table_cells]   WARN:  Col {} not found in table_ctx.columns!",
                    col_idx
                );
            }
        }

        let mut height = 0.0;
        let end_row = cell_info.row + cell_info.rowspan;
        for row_idx in cell_info.row..end_row {
            if let Some(&row_height) = table_ctx.row_heights.get(row_idx) {
                height += row_height;
                // Add spacing between spanned rows (but not after the last one)
                if row_idx < end_row - 1 {
                    height += v_spacing;
                }
            }
        }

        // Update cell's used size and position
        let writing_mode = constraints.writing_mode;
        // Table layout works in main/cross axes, must convert back to logical width/height

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: BEFORE from_main_cross: width={}, height={}, \
             writing_mode={:?}",
            cell_info.node_index,
            width,
            height,
            writing_mode
        );

        cell_node.used_size = Some(LogicalSize::from_main_cross(height, width, writing_mode));

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: AFTER from_main_cross: used_size={:?}",
            cell_info.node_index,
            cell_node.used_size
        );

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: setting used_size to {}x{} (row_heights={:?})",
            cell_info.node_index,
            width,
            height,
            table_ctx.row_heights
        );

        // Apply vertical-align to cell content if it has inline layout
        if let Some(ref cached_layout) = cell_node.inline_layout_result {
            let inline_result = &cached_layout.layout;
            use StyleVerticalAlign;

            // Get vertical-align property from styled_dom
            let vertical_align = if let Some(dom_id) = cell_node.dom_node_id {
                let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
                let node_state = StyledNodeState::default();

                ctx.styled_dom
                    .css_property_cache
                    .ptr
                    .get_vertical_align(node_data, &dom_id, &node_state)
                    .and_then(|v| v.get_property().copied())
                    .unwrap_or(StyleVerticalAlign::Top)
            } else {
                StyleVerticalAlign::Top
            };

            // Calculate content height from inline layout bounds
            let content_bounds = inline_result.bounds();
            let content_height = content_bounds.height;

            // Get padding and border to calculate content-box height
            // height is border-box, but vertical alignment should be within content-box
            let padding = &cell_node.box_props.padding;
            let border = &cell_node.box_props.border;
            let content_box_height = height
                - padding.main_start(writing_mode)
                - padding.main_end(writing_mode)
                - border.main_start(writing_mode)
                - border.main_end(writing_mode);

            // Calculate vertical offset based on alignment within content-box
            let align_factor = match vertical_align {
                StyleVerticalAlign::Top => 0.0,
                StyleVerticalAlign::Middle => 0.5,
                StyleVerticalAlign::Bottom => 1.0,
                // For inline text alignments within table cells, default to middle
                StyleVerticalAlign::Baseline
                | StyleVerticalAlign::Sub
                | StyleVerticalAlign::Superscript
                | StyleVerticalAlign::TextTop
                | StyleVerticalAlign::TextBottom => 0.5,
            };
            let y_offset = (content_box_height - content_height) * align_factor;

            debug_info!(
                ctx,
                "[position_table_cells] Cell {}: vertical-align={:?}, border_box_height={}, \
                 content_box_height={}, content_height={}, y_offset={}",
                cell_info.node_index,
                vertical_align,
                height,
                content_box_height,
                content_height,
                y_offset
            );

            // Create new layout with adjusted positions
            if y_offset.abs() > 0.01 {
                // Only adjust if offset is significant
                use std::sync::Arc;

                use crate::text3::cache::{PositionedItem, UnifiedLayout};

                let adjusted_items: Vec<PositionedItem> = inline_result
                    .items
                    .iter()
                    .map(|item| PositionedItem {
                        item: item.item.clone(),
                        position: crate::text3::cache::Point {
                            x: item.position.x,
                            y: item.position.y + y_offset,
                        },
                        line_index: item.line_index,
                    })
                    .collect();

                let adjusted_layout = UnifiedLayout {
                    items: adjusted_items,
                    overflow: inline_result.overflow.clone(),
                };

                // Keep the same constraint type from the cached layout
                cell_node.inline_layout_result = Some(CachedInlineLayout::new(
                    Arc::new(adjusted_layout),
                    cached_layout.available_width,
                    cached_layout.has_floats,
                ));
            }
        }

        // Store position relative to table origin
        let position = LogicalPosition::from_main_cross(y, x, writing_mode);

        // Insert position into map so cache module can position the cell
        positions.insert(cell_info.node_index, position);

        debug_log!(
            ctx,
            "Cell at row={}, col={}: pos=({:.2}, {:.2}), size=({:.2}x{:.2})",
            cell_info.row,
            cell_info.column,
            x,
            y,
            width,
            height
        );
    }

    Ok(positions)
}

/// Gathers all inline content for `text3`, recursively laying out `inline-block` children
/// to determine their size and baseline before passing them to the text engine.
///
/// This function also assigns IFC membership to all participating nodes:
/// - The IFC root gets an `ifc_id` assigned
/// - Each text/inline child gets `ifc_membership` set with a reference back to the IFC root
///
/// This mapping enables efficient cursor hit-testing: when a text node is clicked,
/// we can find its parent IFC's `inline_layout_result` via `ifc_membership.ifc_root_layout_index`.
fn collect_and_measure_inline_content<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    ifc_root_index: usize,
    constraints: &LayoutConstraints,
) -> Result<(Vec<InlineContent>, HashMap<ContentIndex, usize>)> {
    use crate::solver3::layout_tree::{IfcId, IfcMembership};

    debug_ifc_layout!(
        ctx,
        "collect_and_measure_inline_content: node_index={}",
        ifc_root_index
    );

    // Generate a unique IFC ID for this inline formatting context
    let ifc_id = IfcId::unique();

    // Store IFC ID on the IFC root node
    if let Some(ifc_root_node) = tree.get_mut(ifc_root_index) {
        ifc_root_node.ifc_id = Some(ifc_id);
    }

    let mut content = Vec::new();
    // Maps the `ContentIndex` used by text3 back to the `LayoutNode` index.
    let mut child_map = HashMap::new();
    // Track the current run index for IFC membership assignment
    let mut current_run_index: u32 = 0;

    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    // Check if this is an anonymous IFC wrapper (has no DOM ID)
    let is_anonymous = ifc_root_node.dom_node_id.is_none();

    // Get the DOM node ID of the IFC root, or find it from parent/children for anonymous boxes
    // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let ifc_root_dom_id = match ifc_root_node.dom_node_id {
        Some(id) => id,
        None => {
            // Anonymous box - get DOM ID from parent or first child with DOM ID
            let parent_dom_id = ifc_root_node
                .parent
                .and_then(|p| tree.get(p))
                .and_then(|n| n.dom_node_id);

            if let Some(id) = parent_dom_id {
                id
            } else {
                // Try to find DOM ID from first child
                match ifc_root_node
                    .children
                    .iter()
                    .filter_map(|&child_idx| tree.get(child_idx))
                    .filter_map(|n| n.dom_node_id)
                    .next()
                {
                    Some(id) => id,
                    None => {
                        debug_warning!(ctx, "IFC root and all ancestors/children have no DOM ID");
                        return Ok((content, child_map));
                    }
                }
            }
        }
    };

    // Collect children to avoid holding an immutable borrow during iteration
    let children: Vec<_> = ifc_root_node.children.clone();
    drop(ifc_root_node);

    debug_ifc_layout!(
        ctx,
        "Node {} has {} layout children, is_anonymous={}",
        ifc_root_index,
        children.len(),
        is_anonymous
    );

    // For anonymous IFC wrappers, we collect content from layout tree children
    // For regular IFC roots, we also check DOM children for text nodes
    if is_anonymous {
        // Anonymous IFC wrapper - iterate over layout tree children and collect their content
        for (item_idx, &child_index) in children.iter().enumerate() {
            let content_index = ContentIndex {
                run_index: ifc_root_index as u32,
                item_index: item_idx as u32,
            };

            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let Some(dom_id) = child_node.dom_node_id else {
                debug_warning!(
                    ctx,
                    "Anonymous IFC child at index {} has no DOM ID",
                    child_index
                );
                continue;
            };

            let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];

            // Check if this is a text node
            if let NodeType::Text(ref text_content) = node_data.get_node_type() {
                debug_info!(
                    ctx,
                    "[collect_and_measure_inline_content] OK: Found text node (DOM {:?}) in anonymous wrapper: '{}'",
                    dom_id,
                    text_content.as_str()
                );
                // Get style from the TEXT NODE itself (dom_id), not the IFC root
                // This ensures inline styles like color: #666666 are applied to the text
                // Uses split_text_for_whitespace to correctly handle white-space: pre with \n
                let style = Arc::new(get_style_properties(ctx.styled_dom, dom_id));
                let text_items = split_text_for_whitespace(
                    ctx.styled_dom,
                    dom_id,
                    text_content.as_str(),
                    style,
                );
                content.extend(text_items);
                child_map.insert(content_index, child_index);
                
                // Set IFC membership on the text node - drop child_node borrow first
                drop(child_node);
                if let Some(child_node_mut) = tree.get_mut(child_index) {
                    child_node_mut.ifc_membership = Some(IfcMembership {
                        ifc_id,
                        ifc_root_layout_index: ifc_root_index,
                        run_index: current_run_index,
                    });
                }
                current_run_index += 1;
                
                continue;
            }

            // Non-text inline child - add as shape for inline-block
            let display = get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default();
            if display != LayoutDisplay::Inline {
                // This is an atomic inline-level box (e.g., inline-block, image).
                // We must determine its size and baseline before passing it to text3.

                // The intrinsic sizing pass has already calculated its preferred size.
                let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
                let box_props = child_node.box_props.clone();

                let styled_node_state = ctx
                    .styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| n.styled_node_state.clone())
                    .unwrap_or_default();

                // Calculate tentative border-box size based on CSS properties
                let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
                    ctx.styled_dom,
                    Some(dom_id),
                    constraints.containing_block_size,
                    intrinsic_size,
                    &box_props,
                )?;

                let writing_mode = get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state)
                    .unwrap_or_default();

                // Determine content-box size for laying out children
                let content_box_size = box_props.inner_size(tentative_size, writing_mode);

                // To find its height and baseline, we must lay out its contents.
                let child_constraints = LayoutConstraints {
                    available_size: LogicalSize::new(content_box_size.width, f32::INFINITY),
                    writing_mode,
                    bfc_state: None,
                    text_align: TextAlign::Start,
                    containing_block_size: constraints.containing_block_size,
                    available_width_type: Text3AvailableSpace::Definite(content_box_size.width),
                };

                // Drop the immutable borrow before calling layout_formatting_context
                drop(child_node);

                // Recursively lay out the inline-block to get its final height and baseline.
                let mut empty_float_cache = std::collections::BTreeMap::new();
                let layout_result = layout_formatting_context(
                    ctx,
                    tree,
                    text_cache,
                    child_index,
                    &child_constraints,
                    &mut empty_float_cache,
                )?;

                let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);

                // Determine final border-box height
                let final_height = match css_height.unwrap_or_default() {
                    LayoutHeight::Auto => {
                        let content_height = layout_result.output.overflow_size.height;
                        content_height
                            + box_props.padding.main_sum(writing_mode)
                            + box_props.border.main_sum(writing_mode)
                    }
                    _ => tentative_size.height,
                };

                let final_size = LogicalSize::new(tentative_size.width, final_height);

                // Update the node in the tree with its now-known used size.
                tree.get_mut(child_index).unwrap().used_size = Some(final_size);

                let baseline_offset = layout_result.output.baseline.unwrap_or(final_height);

                // Get margins for inline-block positioning in the inline flow
                // The margin-box size is used so text3 positions inline-blocks with proper spacing
                let margin = &box_props.margin;
                let margin_box_width = final_size.width + margin.left + margin.right;
                let margin_box_height = final_size.height + margin.top + margin.bottom;

                // For inline-block shapes, text3 uses the content array index as run_index
                // and always item_index=0 for objects. We must match this when inserting into child_map.
                let shape_content_index = ContentIndex {
                    run_index: content.len() as u32,
                    item_index: 0,
                };
                content.push(InlineContent::Shape(InlineShape {
                    shape_def: ShapeDefinition::Rectangle {
                        size: crate::text3::cache::Size {
                            // Use margin-box size for positioning in inline flow
                            width: margin_box_width,
                            height: margin_box_height,
                        },
                        corner_radius: None,
                    },
                    fill: None,
                    stroke: None,
                    // Adjust baseline offset by top margin
                    baseline_offset: baseline_offset + margin.top,
                    source_node_id: Some(dom_id),
                }));
                child_map.insert(shape_content_index, child_index);
            } else {
                // Regular inline element - collect its text children
                let span_style = get_style_properties(ctx.styled_dom, dom_id);
                collect_inline_span_recursive(
                    ctx,
                    tree,
                    dom_id,
                    span_style,
                    &mut content,
                    &mut child_map,
                    &children,
                    constraints,
                )?;
            }
        }

        return Ok((content, child_map));
    }

    // Regular (non-anonymous) IFC root - check for list markers and use DOM traversal

    // Check if this IFC root OR its parent is a list-item and needs a marker
    // Case 1: IFC root itself is list-item (e.g., <li> with display: list-item)
    // Case 2: IFC root's parent is list-item (e.g., <li><text>...</text></li>)
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;
    let mut list_item_dom_id: Option<NodeId> = None;

    // Check IFC root itself
    if let Some(dom_id) = ifc_root_node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();

        if let Some(display_value) =
            ctx.styled_dom
                .css_property_cache
                .ptr
                .get_display(node_data, &dom_id, &node_state)
        {
            if let Some(display) = display_value.get_property() {
                use LayoutDisplay;
                if *display == LayoutDisplay::ListItem {
                    debug_ifc_layout!(ctx, "IFC root NodeId({:?}) is list-item", dom_id);
                    list_item_dom_id = Some(dom_id);
                }
            }
        }
    }

    // Check IFC root's parent
    if list_item_dom_id.is_none() {
        if let Some(parent_idx) = ifc_root_node.parent {
            if let Some(parent_node) = tree.get(parent_idx) {
                if let Some(parent_dom_id) = parent_node.dom_node_id {
                    let parent_node_data = &ctx.styled_dom.node_data.as_container()[parent_dom_id];
                    let parent_node_state = StyledNodeState::default();

                    if let Some(display_value) = ctx.styled_dom.css_property_cache.ptr.get_display(
                        parent_node_data,
                        &parent_dom_id,
                        &parent_node_state,
                    ) {
                        if let Some(display) = display_value.get_property() {
                            use LayoutDisplay;
                            if *display == LayoutDisplay::ListItem {
                                debug_ifc_layout!(
                                    ctx,
                                    "IFC root parent NodeId({:?}) is list-item",
                                    parent_dom_id
                                );
                                list_item_dom_id = Some(parent_dom_id);
                            }
                        }
                    }
                }
            }
        }
    }

    // If we found a list-item, generate markers
    if let Some(list_dom_id) = list_item_dom_id {
        debug_ifc_layout!(
            ctx,
            "Found list-item (NodeId({:?})), generating marker",
            list_dom_id
        );

        // Find the layout node index for the list-item DOM node
        let list_item_layout_idx = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| {
                node.dom_node_id == Some(list_dom_id) && node.pseudo_element.is_none()
            })
            .map(|(idx, _)| idx);

        if let Some(list_idx) = list_item_layout_idx {
            // Per CSS spec, the ::marker pseudo-element is the first child of the list-item
            // Find the ::marker pseudo-element in the list-item's children
            let list_item_node = tree.get(list_idx).ok_or(LayoutError::InvalidTree)?;
            let marker_idx = list_item_node
                .children
                .iter()
                .find(|&&child_idx| {
                    tree.get(child_idx)
                        .map(|child| child.pseudo_element == Some(PseudoElement::Marker))
                        .unwrap_or(false)
                })
                .copied();

            if let Some(marker_idx) = marker_idx {
                debug_ifc_layout!(ctx, "Found ::marker pseudo-element at index {}", marker_idx);

                // Get the DOM ID for style resolution (marker references the same DOM node as
                // list-item)
                let list_dom_id_for_style = tree
                    .get(marker_idx)
                    .and_then(|n| n.dom_node_id)
                    .unwrap_or(list_dom_id);

                // Get list-style-position to determine marker positioning
                // Default is 'outside' per CSS Lists Module Level 3

                let list_style_position =
                    get_list_style_position(ctx.styled_dom, Some(list_dom_id));
                let position_outside =
                    matches!(list_style_position, StyleListStylePosition::Outside);

                debug_ifc_layout!(
                    ctx,
                    "List marker list-style-position: {:?} (outside={})",
                    list_style_position,
                    position_outside
                );

                // Generate marker text segments - font fallback happens during shaping
                let base_style =
                    Arc::new(get_style_properties(ctx.styled_dom, list_dom_id_for_style));
                let marker_segments = generate_list_marker_segments(
                    tree,
                    ctx.styled_dom,
                    marker_idx, // Pass the marker index, not the list-item index
                    ctx.counters,
                    base_style,
                    ctx.debug_messages,
                );

                debug_ifc_layout!(
                    ctx,
                    "Generated {} list marker segments",
                    marker_segments.len()
                );

                // Add markers as InlineContent::Marker with position information
                // Outside markers will be positioned in the padding gutter by the layout engine
                for segment in marker_segments {
                    content.push(InlineContent::Marker {
                        run: segment,
                        position_outside,
                    });
                }
            } else {
                debug_ifc_layout!(
                    ctx,
                    "WARNING: List-item at index {} has no ::marker pseudo-element",
                    list_idx
                );
            }
        }
    }

    drop(ifc_root_node);

    // IMPORTANT: We need to traverse the DOM, not just the layout tree!
    //
    // According to CSS spec, a block container with inline-level children establishes
    // an IFC and should collect ALL inline content, including text nodes.
    // Text nodes exist in the DOM but might not have their own layout tree nodes.

    // Debug: Check what the node_hierarchy says about this node
    let node_hier_item = &ctx.styled_dom.node_hierarchy.as_container()[ifc_root_dom_id];
    debug_info!(
        ctx,
        "[collect_and_measure_inline_content] DEBUG: node_hier_item.first_child={:?}, \
         last_child={:?}",
        node_hier_item.first_child_id(ifc_root_dom_id),
        node_hier_item.last_child_id()
    );

    let dom_children: Vec<NodeId> = ifc_root_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();

    let ifc_root_node_data = &ctx.styled_dom.node_data.as_container()[ifc_root_dom_id];

    // SPECIAL CASE: If the IFC root itself is a text node (leaf node),
    // add its text content directly instead of iterating over children
    // Uses split_text_for_whitespace to correctly handle white-space: pre with \n
    if let NodeType::Text(ref text_content) = ifc_root_node_data.get_node_type() {
        let style = Arc::new(get_style_properties(ctx.styled_dom, ifc_root_dom_id));
        let text_items = split_text_for_whitespace(
            ctx.styled_dom,
            ifc_root_dom_id,
            text_content.as_str(),
            style,
        );
        content.extend(text_items);
        return Ok((content, child_map));
    }

    let ifc_root_node_type = match ifc_root_node_data.get_node_type() {
        NodeType::Div => "Div",
        NodeType::Text(_) => "Text",
        NodeType::Body => "Body",
        _ => "Other",
    };

    debug_info!(
        ctx,
        "[collect_and_measure_inline_content] IFC root has {} DOM children",
        dom_children.len()
    );

    for (item_idx, &dom_child_id) in dom_children.iter().enumerate() {
        let content_index = ContentIndex {
            run_index: ifc_root_index as u32,
            item_index: item_idx as u32,
        };

        let node_data = &ctx.styled_dom.node_data.as_container()[dom_child_id];

        // Check if this is a text node
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] OK: Found text node (DOM child {:?}): '{}'",
                dom_child_id,
                text_content.as_str()
            );
            
            // Get style from the TEXT NODE itself (dom_child_id), not the IFC root
            // This ensures inline styles like color: #666666 are applied to the text
            // Uses split_text_for_whitespace to correctly handle white-space: pre with \n
            let style = Arc::new(get_style_properties(ctx.styled_dom, dom_child_id));
            let text_items = split_text_for_whitespace(
                ctx.styled_dom,
                dom_child_id,
                text_content.as_str(),
                style,
            );
            content.extend(text_items);
            
            // Set IFC membership on the text node's layout node (if it exists)
            // Text nodes may or may not have their own layout tree entry depending on
            // whether they're wrapped in an anonymous IFC wrapper
            if let Some(&layout_idx) = tree.dom_to_layout.get(&dom_child_id).and_then(|v| v.first()) {
                if let Some(text_layout_node) = tree.get_mut(layout_idx) {
                    text_layout_node.ifc_membership = Some(IfcMembership {
                        ifc_id,
                        ifc_root_layout_index: ifc_root_index,
                        run_index: current_run_index,
                    });
                }
            }
            current_run_index += 1;
            
            continue;
        }

        // For non-text nodes, find their corresponding layout tree node
        let child_index = children
            .iter()
            .find(|&&idx| {
                tree.get(idx)
                    .and_then(|n| n.dom_node_id)
                    .map(|id| id == dom_child_id)
                    .unwrap_or(false)
            })
            .copied();

        let Some(child_index) = child_index else {
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] WARN: DOM child {:?} has no layout node",
                dom_child_id
            );
            continue;
        };

        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        // At this point we have a non-text DOM child with a layout node
        let dom_id = child_node.dom_node_id.unwrap();

        let display = get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default();

        if display != LayoutDisplay::Inline {
            // This is an atomic inline-level box (e.g., inline-block, image).
            // We must determine its size and baseline before passing it to text3.

            // The intrinsic sizing pass has already calculated its preferred size.
            let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
            let box_props = child_node.box_props.clone();

            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|n| n.styled_node_state.clone())
                .unwrap_or_default();

            // Calculate tentative border-box size based on CSS properties
            // This correctly handles explicit width/height, box-sizing, and constraints
            let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
                ctx.styled_dom,
                Some(dom_id),
                constraints.containing_block_size,
                intrinsic_size,
                &box_props,
            )?;

            let writing_mode =
                get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default();

            // Determine content-box size for laying out children
            let content_box_size = box_props.inner_size(tentative_size, writing_mode);

            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Inline-block NodeId({:?}): \
                 tentative_border_box={:?}, content_box={:?}",
                dom_id,
                tentative_size,
                content_box_size
            );

            // To find its height and baseline, we must lay out its contents.
            let child_constraints = LayoutConstraints {
                available_size: LogicalSize::new(content_box_size.width, f32::INFINITY),
                writing_mode,
                // Inline-blocks establish a new BFC, so no state is passed in.
                bfc_state: None,
                // Does not affect size/baseline of the container.
                text_align: TextAlign::Start,
                containing_block_size: constraints.containing_block_size,
                available_width_type: Text3AvailableSpace::Definite(content_box_size.width),
            };

            // Drop the immutable borrow before calling layout_formatting_context
            drop(child_node);

            // Recursively lay out the inline-block to get its final height and baseline.
            // Note: This does not affect its final position, only its dimensions.
            let mut empty_float_cache = std::collections::BTreeMap::new();
            let layout_result = layout_formatting_context(
                ctx,
                tree,
                text_cache,
                child_index,
                &child_constraints,
                &mut empty_float_cache,
            )?;

            let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);

            // Determine final border-box height
            let final_height = match css_height.unwrap_or_default() {
                LayoutHeight::Auto => {
                    // For auto height, add padding and border to the content height
                    let content_height = layout_result.output.overflow_size.height;
                    content_height
                        + box_props.padding.main_sum(writing_mode)
                        + box_props.border.main_sum(writing_mode)
                }
                // For explicit height, calculate_used_size_for_node already gave us the correct border-box height
                _ => tentative_size.height,
            };

            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Inline-block NodeId({:?}): \
                 layout_content_height={}, css_height={:?}, final_border_box_height={}",
                dom_id,
                layout_result.output.overflow_size.height,
                css_height,
                final_height
            );

            let final_size = LogicalSize::new(tentative_size.width, final_height);

            // Update the node in the tree with its now-known used size.
            tree.get_mut(child_index).unwrap().used_size = Some(final_size);

            // CSS 2.2 § 10.8.1: For inline-block elements, the baseline is the baseline of the
            // last line box in the normal flow, unless it has no in-flow line boxes, in which
            // case the baseline is the bottom margin edge.
            //
            // `layout_result.output.baseline` returns the Y-position of the baseline measured
            // from the TOP of the content box. But `get_item_vertical_metrics` expects
            // `baseline_offset` to be the distance from the BOTTOM to the baseline.
            //
            // Conversion: baseline_offset_from_bottom = height - baseline_from_top
            //
            // If no baseline is found (e.g., the inline-block has no text), we fall back to
            // the bottom margin edge (baseline_offset = 0, meaning baseline at bottom).
            let baseline_from_top = layout_result.output.baseline;
            let baseline_offset = match baseline_from_top {
                Some(baseline_y) => {
                    // baseline_y is measured from top of content box
                    // We need to add padding and border to get the position within the border-box
                    let content_box_top = box_props.padding.top + box_props.border.top;
                    let baseline_from_border_box_top = baseline_y + content_box_top;
                    // Convert to distance from bottom
                    (final_height - baseline_from_border_box_top).max(0.0)
                }
                None => {
                    // No baseline found - use bottom margin edge (baseline at bottom)
                    0.0
                }
            };
            
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Inline-block NodeId({:?}): \
                 baseline_from_top={:?}, final_height={}, baseline_offset_from_bottom={}",
                dom_id,
                baseline_from_top,
                final_height,
                baseline_offset
            );

            // Get margins for inline-block positioning
            // For inline-blocks, we need to include margins in the shape size
            // so that text3 positions them correctly with spacing
            let margin = &box_props.margin;
            let margin_box_width = final_size.width + margin.left + margin.right;
            let margin_box_height = final_size.height + margin.top + margin.bottom;

            // For inline-block shapes, text3 uses the content array index as run_index
            // and always item_index=0 for objects. We must match this when inserting into child_map.
            let shape_content_index = ContentIndex {
                run_index: content.len() as u32,
                item_index: 0,
            };
            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: crate::text3::cache::Size {
                        // Use margin-box size for positioning in inline flow
                        width: margin_box_width,
                        height: margin_box_height,
                    },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                // Adjust baseline offset by top margin
                baseline_offset: baseline_offset + margin.top,
                source_node_id: Some(dom_id),
            }));
            child_map.insert(shape_content_index, child_index);
        } else if let NodeType::Image(image_ref) =
            ctx.styled_dom.node_data.as_container()[dom_id].get_node_type()
        {
            // Images are replaced elements - they have intrinsic dimensions
            // and CSS width/height can constrain them
            
            // Re-get child_node since we dropped it earlier for the inline-block case
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let box_props = child_node.box_props.clone();

            // Get intrinsic size from the image data or fall back to layout node
            let intrinsic_size = child_node
                .intrinsic_sizes
                .clone()
                .unwrap_or(IntrinsicSizes {
                    max_content_width: 50.0,
                    max_content_height: 50.0,
                    ..Default::default()
                });
            
            // Get styled node state for CSS property lookup
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|n| n.styled_node_state.clone())
                .unwrap_or_default();
            
            // Calculate the used size respecting CSS width/height constraints
            let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
                ctx.styled_dom,
                Some(dom_id),
                constraints.containing_block_size,
                intrinsic_size.clone(),
                &box_props,
            )?;
            
            // Drop immutable borrow before mutable access
            drop(child_node);
            
            // Set the used_size on the layout node so paint_rect works correctly
            let final_size = LogicalSize::new(tentative_size.width, tentative_size.height);
            tree.get_mut(child_index).unwrap().used_size = Some(final_size);
            
            println!("[FC IMAGE] Image node {} final_size: {}x{}", 
                child_index, final_size.width, final_size.height);
            
            // Calculate display size for text3 (this is what text3 uses for positioning)
            let display_width = if final_size.width > 0.0 { 
                Some(final_size.width) 
            } else { 
                None 
            };
            let display_height = if final_size.height > 0.0 { 
                Some(final_size.height) 
            } else { 
                None 
            };
            
            content.push(InlineContent::Image(InlineImage {
                source: ImageSource::Ref(image_ref.clone()),
                intrinsic_size: crate::text3::cache::Size {
                    width: intrinsic_size.max_content_width,
                    height: intrinsic_size.max_content_height,
                },
                display_size: if display_width.is_some() || display_height.is_some() {
                    Some(crate::text3::cache::Size {
                        width: display_width.unwrap_or(intrinsic_size.max_content_width),
                        height: display_height.unwrap_or(intrinsic_size.max_content_height),
                    })
                } else {
                    None
                },
                // Images are bottom-aligned with the baseline by default
                baseline_offset: 0.0,
                alignment: crate::text3::cache::VerticalAlign::Baseline,
                object_fit: ObjectFit::Fill,
            }));
            // For images, text3 uses the content array index as run_index
            // and always item_index=0 for objects. We must match this.
            let image_content_index = ContentIndex {
                run_index: (content.len() - 1) as u32,  // -1 because we just pushed
                item_index: 0,
            };
            child_map.insert(image_content_index, child_index);
        } else {
            // This is a regular inline box (display: inline) - e.g., <span>, <em>, <strong>
            //
            // According to CSS Inline-3 spec §2, inline boxes are "transparent" wrappers
            // We must recursively collect their text children with inherited style
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Found inline span (DOM {:?}), recursing",
                dom_id
            );

            let span_style = get_style_properties(ctx.styled_dom, dom_id);
            collect_inline_span_recursive(
                ctx,
                tree,
                dom_id,
                span_style,
                &mut content,
                &mut child_map,
                &children,
                constraints,
            )?;
        }
    }
    Ok((content, child_map))
}

/// Recursively collects inline content from an inline span (display: inline) element.
///
/// According to CSS Inline Layout Module Level 3 §2:
///
/// "Inline boxes are transparent wrappers that wrap their content."
///
/// They don't create a new formatting context - their children participate in the
/// same IFC as the parent. This function processes:
///
/// - Text nodes: collected with the span's inherited style
/// - Nested inline spans: recursively descended
/// - Inline-blocks, images: measured and added as shapes
fn collect_inline_span_recursive<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    span_dom_id: NodeId,
    span_style: StyleProperties,
    content: &mut Vec<InlineContent>,
    child_map: &mut HashMap<ContentIndex, usize>,
    parent_children: &[usize], // Layout tree children of parent IFC
    constraints: &LayoutConstraints,
) -> Result<()> {
    debug_info!(
        ctx,
        "[collect_inline_span_recursive] Processing inline span {:?}",
        span_dom_id
    );

    // Get DOM children of this span
    let span_dom_children: Vec<NodeId> = span_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();

    debug_info!(
        ctx,
        "[collect_inline_span_recursive] Span has {} DOM children",
        span_dom_children.len()
    );

    for &child_dom_id in &span_dom_children {
        let node_data = &ctx.styled_dom.node_data.as_container()[child_dom_id];

        // CASE 1: Text node - collect with span's style
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            debug_info!(
                ctx,
                "[collect_inline_span_recursive] ✓ Found text in span: '{}'",
                text_content.as_str()
            );
            content.push(InlineContent::Text(StyledRun {
                text: text_content.to_string(),
                style: Arc::new(span_style.clone()),
                logical_start_byte: 0,
                source_node_id: Some(child_dom_id),
            }));
            continue;
        }

        // CASE 2: Element node - check its display type
        let child_display =
            get_display_property(ctx.styled_dom, Some(child_dom_id)).unwrap_or_default();

        // Find the corresponding layout tree node
        let child_index = parent_children
            .iter()
            .find(|&&idx| {
                tree.get(idx)
                    .and_then(|n| n.dom_node_id)
                    .map(|id| id == child_dom_id)
                    .unwrap_or(false)
            })
            .copied();

        match child_display {
            LayoutDisplay::Inline => {
                // Nested inline span - recurse with child's style
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] Found nested inline span {:?}",
                    child_dom_id
                );
                let child_style = get_style_properties(ctx.styled_dom, child_dom_id);
                collect_inline_span_recursive(
                    ctx,
                    tree,
                    child_dom_id,
                    child_style,
                    content,
                    child_map,
                    parent_children,
                    constraints,
                )?;
            }
            LayoutDisplay::InlineBlock => {
                // Inline-block inside span - measure and add as shape
                let Some(child_index) = child_index else {
                    debug_info!(
                        ctx,
                        "[collect_inline_span_recursive] WARNING: inline-block {:?} has no layout \
                         node",
                        child_dom_id
                    );
                    continue;
                };

                let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
                let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
                let width = intrinsic_size.max_content_width;

                let styled_node_state = ctx
                    .styled_dom
                    .styled_nodes
                    .as_container()
                    .get(child_dom_id)
                    .map(|n| n.styled_node_state.clone())
                    .unwrap_or_default();
                let writing_mode =
                    get_writing_mode(ctx.styled_dom, child_dom_id, &styled_node_state)
                        .unwrap_or_default();
                let child_constraints = LayoutConstraints {
                    available_size: LogicalSize::new(width, f32::INFINITY),
                    writing_mode,
                    bfc_state: None,
                    text_align: TextAlign::Start,
                    containing_block_size: constraints.containing_block_size,
                    available_width_type: Text3AvailableSpace::Definite(width),
                };

                drop(child_node);

                let mut empty_float_cache = std::collections::BTreeMap::new();
                let layout_result = layout_formatting_context(
                    ctx,
                    tree,
                    &mut TextLayoutCache::default(),
                    child_index,
                    &child_constraints,
                    &mut empty_float_cache,
                )?;
                let final_height = layout_result.output.overflow_size.height;
                let final_size = LogicalSize::new(width, final_height);

                tree.get_mut(child_index).unwrap().used_size = Some(final_size);
                let baseline_offset = layout_result.output.baseline.unwrap_or(final_height);

                content.push(InlineContent::Shape(InlineShape {
                    shape_def: ShapeDefinition::Rectangle {
                        size: crate::text3::cache::Size {
                            width,
                            height: final_height,
                        },
                        corner_radius: None,
                    },
                    fill: None,
                    stroke: None,
                    baseline_offset,
                    source_node_id: Some(child_dom_id),
                }));

                // Note: We don't add to child_map here because this is inside a span
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] Added inline-block shape {}x{}",
                    width,
                    final_height
                );
            }
            _ => {
                // Other display types inside span (shouldn't normally happen)
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] WARNING: Unsupported display type {:?} \
                     inside inline span",
                    child_display
                );
            }
        }
    }

    Ok(())
}

/// Positions a floated child within the BFC and updates the floating context.
/// This function is fully writing-mode aware.
fn position_floated_child(
    _child_index: usize,
    child_margin_box_size: LogicalSize,
    float_type: LayoutFloat,
    constraints: &LayoutConstraints,
    _bfc_content_box: LogicalRect,
    current_main_offset: f32,
    floating_context: &mut FloatingContext,
) -> Result<LogicalPosition> {
    let wm = constraints.writing_mode;
    let child_main_size = child_margin_box_size.main(wm);
    let child_cross_size = child_margin_box_size.cross(wm);
    let bfc_cross_size = constraints.available_size.cross(wm);
    let mut placement_main_offset = current_main_offset;

    loop {
        // 1. Determine the available cross-axis space at the current
        // `placement_main_offset`.
        let (available_cross_start, available_cross_end) = floating_context
            .available_line_box_space(
                placement_main_offset,
                placement_main_offset + child_main_size,
                bfc_cross_size,
                wm,
            );

        let available_cross_width = available_cross_end - available_cross_start;

        // 2. Check if the new float can fit in the available space.
        if child_cross_size <= available_cross_width {
            // It fits! Determine the final position and add it to the context.
            let final_cross_pos = match float_type {
                LayoutFloat::Left => available_cross_start,
                LayoutFloat::Right => available_cross_end - child_cross_size,
                LayoutFloat::None => unreachable!(),
            };
            let final_pos =
                LogicalPosition::from_main_cross(placement_main_offset, final_cross_pos, wm);

            let new_float_box = FloatBox {
                kind: float_type,
                rect: LogicalRect::new(final_pos, child_margin_box_size),
                margin: EdgeSizes::default(), // TODO: Pass actual margin if this function is used
            };
            floating_context.floats.push(new_float_box);
            return Ok(final_pos);
        } else {
            // It doesn't fit. We must move the float down past an obstacle.
            // Find the lowest main-axis end of all floats that are blocking
            // the current line.
            let mut next_main_offset = f32::INFINITY;
            for existing_float in &floating_context.floats {
                let float_main_start = existing_float.rect.origin.main(wm);
                let float_main_end = float_main_start + existing_float.rect.size.main(wm);

                // Consider only floats that are above or at the current placement line.
                if placement_main_offset < float_main_end {
                    next_main_offset = next_main_offset.min(float_main_end);
                }
            }

            if next_main_offset.is_infinite() {
                // This indicates an unrecoverable state, e.g., a float wider
                // than the container.
                return Err(LayoutError::PositioningFailed);
            }
            placement_main_offset = next_main_offset;
        }
    }
}

// CSS Property Getters

/// Get the CSS `float` property for a node.
fn get_float_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutFloat {
    let Some(id) = dom_id else {
        return LayoutFloat::None;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_float(node_data, &id, node_state)
        .and_then(|f| {
            f.get_property().map(|inner| match inner {
                LayoutFloat::Left => LayoutFloat::Left,
                LayoutFloat::Right => LayoutFloat::Right,
                LayoutFloat::None => LayoutFloat::None,
            })
        })
        .unwrap_or(LayoutFloat::None)
}

fn get_clear_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutClear {
    let Some(id) = dom_id else {
        return LayoutClear::None;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_clear(node_data, &id, node_state)
        .and_then(|c| {
            c.get_property().map(|inner| match inner {
                LayoutClear::Left => LayoutClear::Left,
                LayoutClear::Right => LayoutClear::Right,
                LayoutClear::Both => LayoutClear::Both,
                LayoutClear::None => LayoutClear::None,
            })
        })
        .unwrap_or(LayoutClear::None)
}
/// Helper to determine if scrollbars are needed.
///
/// # CSS Spec Reference
/// CSS Overflow Module Level 3 § 3: Scrollable overflow
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
) -> ScrollbarRequirements {
    let mut needs_horizontal = match overflow_x {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.width > container_size.width,
    };

    let mut needs_vertical = match overflow_y {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.height > container_size.height,
    };

    // A classic layout problem: a vertical scrollbar can reduce horizontal space,
    // causing a horizontal scrollbar to appear, which can reduce vertical space...
    // A full solution involves a loop, but this two-pass check handles most cases.
    if needs_vertical && !needs_horizontal && overflow_x == OverflowBehavior::Auto {
        if content_size.width > (container_size.width - SCROLLBAR_WIDTH_PX) {
            needs_horizontal = true;
        }
    }
    if needs_horizontal && !needs_vertical && overflow_y == OverflowBehavior::Auto {
        if content_size.height > (container_size.height - SCROLLBAR_WIDTH_PX) {
            needs_vertical = true;
        }
    }

    ScrollbarRequirements {
        needs_horizontal,
        needs_vertical,
        scrollbar_width: if needs_vertical {
            SCROLLBAR_WIDTH_PX
        } else {
            0.0
        },
        scrollbar_height: if needs_horizontal {
            SCROLLBAR_WIDTH_PX
        } else {
            0.0
        },
    }
}

/// Calculates a single collapsed margin from two adjoining vertical margins.
///
/// Implements the rules from CSS 2.1 section 8.3.1:
/// - If both margins are positive, the result is the larger of the two.
/// - If both margins are negative, the result is the more negative of the two.
/// - If the margins have mixed signs, they are effectively summed.
pub(crate) fn collapse_margins(a: f32, b: f32) -> f32 {
    if a.is_sign_positive() && b.is_sign_positive() {
        a.max(b)
    } else if a.is_sign_negative() && b.is_sign_negative() {
        a.min(b)
    } else {
        a + b
    }
}

/// Helper function to advance the pen position with margin collapsing.
///
/// This implements CSS 2.1 margin collapsing for adjacent block-level boxes in a BFC.
///
/// - `pen` - Current main-axis position (will be modified)
/// - `last_margin_bottom` - The bottom margin of the previous in-flow element
/// - `current_margin_top` - The top margin of the current element
///
/// # Returns
///
/// The new `last_margin_bottom` value (the bottom margin of the current element)
///
/// # CSS Spec Compliance
///
/// Per CSS 2.1 Section 8.3.1 "Collapsing margins":
///
/// - Adjacent vertical margins of block boxes collapse
/// - The resulting margin width is the maximum of the adjoining margins (if both positive)
/// - Or the sum of the most positive and most negative (if signs differ)
fn advance_pen_with_margin_collapse(
    pen: &mut f32,
    last_margin_bottom: f32,
    current_margin_top: f32,
) -> f32 {
    // Collapse the previous element's bottom margin with current element's top margin
    let collapsed_margin = collapse_margins(last_margin_bottom, current_margin_top);

    // Advance pen by the collapsed margin
    *pen += collapsed_margin;

    // Return collapsed_margin so caller knows how much space was actually added
    collapsed_margin
}

/// Checks if an element's border or padding prevents margin collapsing.
///
/// Per CSS 2.1 Section 8.3.1:
///
/// - Border between margins prevents collapsing
/// - Padding between margins prevents collapsing
///
/// # Arguments
///
/// - `box_props` - The box properties containing border and padding
/// - `writing_mode` - The writing mode to determine main axis
/// - `check_start` - If true, check main-start (top); if false, check main-end (bottom)
///
/// # Returns
///
/// `true` if border or padding exists and prevents collapsing
fn has_margin_collapse_blocker(
    box_props: &crate::solver3::geometry::BoxProps,
    writing_mode: LayoutWritingMode,
    check_start: bool, // true = check top/start, false = check bottom/end
) -> bool {
    if check_start {
        // Check if there's border-top or padding-top
        let border_start = box_props.border.main_start(writing_mode);
        let padding_start = box_props.padding.main_start(writing_mode);
        border_start > 0.0 || padding_start > 0.0
    } else {
        // Check if there's border-bottom or padding-bottom
        let border_end = box_props.border.main_end(writing_mode);
        let padding_end = box_props.padding.main_end(writing_mode);
        border_end > 0.0 || padding_end > 0.0
    }
}

/// Checks if an element is empty (has no content).
///
/// Per CSS 2.1 Section 8.3.1:
///
/// > If a block element has no border, padding, inline content, height, or min-height,
/// > then its top and bottom margins collapse with each other.
///
/// # Arguments
///
/// - `node` - The layout node to check
///
/// # Returns
///
/// `true` if the element is empty and its margins can collapse internally
fn is_empty_block(node: &LayoutNode) -> bool {
    // Per CSS 2.2 § 8.3.1: An empty block is one that:
    // - Has zero computed 'min-height'
    // - Has zero or 'auto' computed 'height'
    // - Has no in-flow children
    // - Has no line boxes (no text/inline content)

    // Check if node has children
    if !node.children.is_empty() {
        return false;
    }

    // Check if node has inline content (text)
    if node.inline_layout_result.is_some() {
        return false;
    }

    // Check if node has explicit height > 0
    // CSS 2.2 § 8.3.1: Elements with explicit height are NOT empty
    if let Some(size) = node.used_size {
        if size.height > 0.0 {
            return false;
        }
    }

    // Empty block: no children, no inline content, no height
    true
}

/// Generates marker text for a list item marker.
///
/// This function looks up the counter value from the cache and formats it
/// according to the list-style-type property.
///
/// Per CSS Lists Module Level 3, the ::marker pseudo-element is the first child
/// of the list-item, and references the same DOM node. Counter resolution happens
/// on the list-item (parent) node.
fn generate_list_marker_text(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &BTreeMap<(usize, String), i32>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> String {
    use crate::solver3::counters::format_counter;

    // Get the marker node
    let marker_node = match tree.get(marker_index) {
        Some(n) => n,
        None => return String::new(),
    };

    // Verify this is actually a ::marker pseudo-element
    // Per spec, markers must be pseudo-elements, not anonymous boxes
    if marker_node.pseudo_element != Some(PseudoElement::Marker) {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::warning(format!(
                "[generate_list_marker_text] WARNING: Node {} is not a ::marker pseudo-element \
                 (pseudo={:?}, anonymous_type={:?})",
                marker_index, marker_node.pseudo_element, marker_node.anonymous_type
            )));
        }
        // Fallback for old-style anonymous markers during transition
        if marker_node.anonymous_type != Some(AnonymousBoxType::ListItemMarker) {
            return String::new();
        }
    }

    // Get the parent list-item node (::marker is first child of list-item)
    let list_item_index = match marker_node.parent {
        Some(p) => p,
        None => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::error(
                    "[generate_list_marker_text] ERROR: Marker has no parent".to_string(),
                ));
            }
            return String::new();
        }
    };

    let list_item_node = match tree.get(list_item_index) {
        Some(n) => n,
        None => return String::new(),
    };

    let list_item_dom_id = match list_item_node.dom_node_id {
        Some(id) => id,
        None => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::error(
                    "[generate_list_marker_text] ERROR: List-item has no DOM ID".to_string(),
                ));
            }
            return String::new();
        }
    };

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_text] marker_index={}, list_item_index={}, \
             list_item_dom_id={:?}",
            marker_index, list_item_index, list_item_dom_id
        )));
    }

    // Get list-style-type from the list-item or its container
    let list_container_dom_id = if let Some(grandparent_index) = list_item_node.parent {
        if let Some(grandparent) = tree.get(grandparent_index) {
            grandparent.dom_node_id
        } else {
            None
        }
    } else {
        None
    };

    // Try to get list-style-type from the list container first,
    // then fall back to the list-item
    let list_style_type = if let Some(container_id) = list_container_dom_id {
        let container_type = get_list_style_type(styled_dom, Some(container_id));
        if container_type != StyleListStyleType::default() {
            container_type
        } else {
            get_list_style_type(styled_dom, Some(list_item_dom_id))
        }
    } else {
        get_list_style_type(styled_dom, Some(list_item_dom_id))
    };

    // Get the counter value for "list-item" counter from the LIST-ITEM node
    // Per CSS spec, counters are scoped to elements, and the list-item counter
    // is incremented at the list-item element, not the marker pseudo-element
    let counter_value = counters
        .get(&(list_item_index, "list-item".to_string()))
        .copied()
        .unwrap_or_else(|| {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::warning(format!(
                    "[generate_list_marker_text] WARNING: No counter found for list-item at index \
                     {}, defaulting to 1",
                    list_item_index
                )));
            }
            1
        });

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_text] counter_value={} for list_item_index={}",
            counter_value, list_item_index
        )));
    }

    // Format the counter according to the list-style-type
    let marker_text = format_counter(counter_value, list_style_type);

    // For ordered lists (non-symbolic markers), add a period and space
    // For unordered lists (symbolic markers like •, ◦, ▪), just add a space
    if matches!(
        list_style_type,
        StyleListStyleType::Decimal
            | StyleListStyleType::DecimalLeadingZero
            | StyleListStyleType::LowerAlpha
            | StyleListStyleType::UpperAlpha
            | StyleListStyleType::LowerRoman
            | StyleListStyleType::UpperRoman
            | StyleListStyleType::LowerGreek
            | StyleListStyleType::UpperGreek
    ) {
        format!("{}. ", marker_text)
    } else {
        format!("{} ", marker_text)
    }
}

/// Generates marker text segments for a list item marker.
///
/// Simply returns a single StyledRun with the marker text using the base_style.
/// The font stack in base_style already includes fallbacks with 100% Unicode coverage,
/// so font resolution happens during text shaping, not here.
fn generate_list_marker_segments(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &BTreeMap<(usize, String), i32>,
    base_style: Arc<StyleProperties>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<StyledRun> {
    // Generate the marker text
    let marker_text =
        generate_list_marker_text(tree, styled_dom, marker_index, counters, debug_messages);
    if marker_text.is_empty() {
        return Vec::new();
    }

    if let Some(msgs) = debug_messages {
        let font_families: Vec<&str> = match &base_style.font_stack {
            crate::text3::cache::FontStack::Stack(selectors) => {
                selectors.iter().map(|f| f.family.as_str()).collect()
            }
            crate::text3::cache::FontStack::Ref(_) => vec!["<embedded-font>"],
        };
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_segments] Marker text: '{}' with font stack: {:?}",
            marker_text,
            font_families
        )));
    }

    // Return single segment - font fallback happens during shaping
    // List markers are generated content, not from DOM nodes
    vec![StyledRun {
        text: marker_text,
        style: base_style,
        logical_start_byte: 0,
        source_node_id: None,
    }]
}

/// Splits text content into InlineContent items based on white-space CSS property.
///
/// For `white-space: pre`, `pre-wrap`, and `pre-line`, newlines (`\n`) are treated as
/// forced line breaks per CSS Text Level 3 specification:
/// https://www.w3.org/TR/css-text-3/#white-space-property
///
/// This function:
/// 1. Checks the white-space property of the node (or its parent for text nodes)
/// 2. If `pre`, `pre-wrap`, or `pre-line`: splits text by `\n` and inserts `InlineContent::LineBreak`
/// 3. Otherwise: returns the text as a single `InlineContent::Text`
///
/// Returns a Vec of InlineContent items that correctly represent line breaks.
pub(crate) fn split_text_for_whitespace(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    text: &str,
    style: Arc<StyleProperties>,
) -> Vec<InlineContent> {
    use crate::text3::cache::{BreakType, ClearType, InlineBreak};
    
    // Get the white-space property - TEXT NODES inherit from parent!
    // We need to check the parent element's white-space, not the text node itself
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let parent_id = node_hierarchy[dom_id].parent_id();
    
    // Try parent first, then fall back to the node itself
    let white_space = if let Some(parent) = parent_id {
        let parent_node_data = &styled_dom.node_data.as_container()[parent];
        let styled_nodes = styled_dom.styled_nodes.as_container();
        let parent_state = styled_nodes
            .get(parent)
            .map(|n| n.styled_node_state.clone())
            .unwrap_or_default();
        
        styled_dom
            .css_property_cache
            .ptr
            .get_white_space(parent_node_data, &parent, &parent_state)
            .and_then(|s| s.get_property().cloned())
            .unwrap_or(StyleWhiteSpace::Normal)
    } else {
        StyleWhiteSpace::Normal
    };
    
    let mut result = Vec::new();
    
    // For `pre`, `pre-wrap`, and `pre-line`, newlines must be preserved as forced breaks
    // CSS Text Level 3: "Newlines in the source will be honored as forced line breaks."
    match white_space {
        StyleWhiteSpace::Pre => {
            // Split by newlines and insert LineBreak between parts
            let mut lines = text.split('\n').peekable();
            let mut content_index = 0;
            
            while let Some(line) = lines.next() {
                // Add the text part if not empty
                if !line.is_empty() {
                    result.push(InlineContent::Text(StyledRun {
                        text: line.to_string(),
                        style: Arc::clone(&style),
                        logical_start_byte: 0,
                        source_node_id: Some(dom_id),
                    }));
                }
                
                // If there's more content, insert a forced line break
                if lines.peek().is_some() {
                    result.push(InlineContent::LineBreak(InlineBreak {
                        break_type: BreakType::Hard,
                        clear: ClearType::None,
                        content_index,
                    }));
                    content_index += 1;
                }
            }
        }
        // TODO: Implement pre-wrap and pre-line when CSS parser supports them
        StyleWhiteSpace::Normal | StyleWhiteSpace::Nowrap => {
            // Normal behavior: newlines are collapsed to spaces
            if !text.is_empty() {
                result.push(InlineContent::Text(StyledRun {
                    text: text.to_string(),
                    style,
                    logical_start_byte: 0,
                    source_node_id: Some(dom_id),
                }));
            }
        }
    }
    
    result
}

```

================================================================================
## FILE: layout/src/text3/cache.rs
## Description: Text cache - UnifiedConstraints default implementation
================================================================================
```rust
use std::{
    any::{Any, TypeId},
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, Entry, HashMap},
        BTreeSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

pub use azul_core::selection::{ContentIndex, GraphemeClusterId};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage, props::basic::ColorU, props::style::StyleBackgroundContent,
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, FontId, PatternMatch, UnicodeRange};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;

// Stub type when hyphenation is disabled
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct Standard;

#[cfg(not(feature = "text_layout_hyphenation"))]
impl Standard {
    /// Stub hyphenate method that returns no breaks
    pub fn hyphenate<'a>(&'a self, _word: &'a str) -> StubHyphenationBreaks {
        StubHyphenationBreaks { breaks: Vec::new() }
    }
}

/// Result of hyphenation (stub when feature is disabled)
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct StubHyphenationBreaks {
    pub breaks: alloc::vec::Vec<usize>,
}

// Always import Language from script module
use crate::text3::script::{script_to_language, Language, Script};

/// Available space for layout, similar to Taffy's AvailableSpace.
///
/// This type explicitly represents the three possible states for available space:
///
/// - `Definite(f32)`: A specific pixel width is available
/// - `MinContent`: Layout should use minimum content width (shrink-wrap)
/// - `MaxContent`: Layout should use maximum content width (no line breaks unless necessary)
///
/// This is critical for proper handling of intrinsic sizing in Flexbox/Grid
/// where the available space may be indefinite during the measure phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvailableSpace {
    /// A specific amount of space is available (in pixels)
    Definite(f32),
    /// The node should be laid out under a min-content constraint
    MinContent,
    /// The node should be laid out under a max-content constraint  
    MaxContent,
}

impl Default for AvailableSpace {
    fn default() -> Self {
        AvailableSpace::Definite(0.0)
    }
}

impl AvailableSpace {
    /// Returns true if this is a definite (finite, known) amount of space
    pub fn is_definite(&self) -> bool {
        matches!(self, AvailableSpace::Definite(_))
    }

    /// Returns true if this is an indefinite (min-content or max-content) constraint
    pub fn is_indefinite(&self) -> bool {
        !self.is_definite()
    }

    /// Returns the definite value if available, or a fallback for indefinite constraints
    pub fn unwrap_or(self, fallback: f32) -> f32 {
        match self {
            AvailableSpace::Definite(v) => v,
            _ => fallback,
        }
    }

    /// Returns the definite value, or 0.0 for min-content, or f32::MAX for max-content
    pub fn to_f32_for_layout(self) -> f32 {
        match self {
            AvailableSpace::Definite(v) => v,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f32::MAX,
        }
    }

    /// Create from an f32 value, recognizing special sentinel values.
    ///
    /// This function provides backwards compatibility with code that uses f32 for constraints:
    /// - `f32::INFINITY` or `f32::MAX` → `MaxContent` (no line wrapping)
    /// - `0.0` → `MinContent` (maximum line wrapping, return longest word width)
    /// - Other values → `Definite(value)`
    ///
    /// Note: Using sentinel values like 0.0 for MinContent is fragile. Prefer using
    /// `AvailableSpace::MinContent` directly when possible.
    pub fn from_f32(value: f32) -> Self {
        if value.is_infinite() || value >= f32::MAX / 2.0 {
            // Treat very large values (including f32::MAX) as MaxContent
            AvailableSpace::MaxContent
        } else if value <= 0.0 {
            // Treat zero or negative as MinContent (shrink-wrap)
            AvailableSpace::MinContent
        } else {
            AvailableSpace::Definite(value)
        }
    }
}

impl Hash for AvailableSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        if let AvailableSpace::Definite(v) = self {
            (v.round() as usize).hash(state);
        }
    }
}

// Re-export traits for backwards compatibility
pub use crate::font_traits::{ParsedFontTrait, ShallowClone};

// --- Core Data Structures for the New Architecture ---

/// Key for caching font chains - based only on CSS properties, not text content
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontChainKey {
    pub font_families: Vec<String>,
    pub weight: FcWeight,
    pub italic: bool,
    pub oblique: bool,
}

/// Either a FontChainKey (resolved via fontconfig) or a direct FontRef hash.
/// 
/// This enum cleanly separates:
/// - `Chain`: Fonts resolved through fontconfig with fallback support
/// - `Ref`: Direct FontRef that bypasses fontconfig entirely (e.g., embedded icon fonts)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FontChainKeyOrRef {
    /// Regular font chain resolved via fontconfig
    Chain(FontChainKey),
    /// Direct FontRef identified by pointer address (covers entire Unicode range, no fallbacks)
    Ref(usize),
}

impl FontChainKeyOrRef {
    /// Create from a FontStack enum
    pub fn from_font_stack(font_stack: &FontStack) -> Self {
        match font_stack {
            FontStack::Stack(selectors) => FontChainKeyOrRef::Chain(FontChainKey::from_selectors(selectors)),
            FontStack::Ref(font_ref) => FontChainKeyOrRef::Ref(font_ref.parsed as usize),
        }
    }
    
    /// Returns true if this is a direct FontRef
    pub fn is_ref(&self) -> bool {
        matches!(self, FontChainKeyOrRef::Ref(_))
    }
    
    /// Returns the FontRef pointer if this is a Ref variant
    pub fn as_ref_ptr(&self) -> Option<usize> {
        match self {
            FontChainKeyOrRef::Ref(ptr) => Some(*ptr),
            _ => None,
        }
    }
    
    /// Returns the FontChainKey if this is a Chain variant
    pub fn as_chain(&self) -> Option<&FontChainKey> {
        match self {
            FontChainKeyOrRef::Chain(key) => Some(key),
            _ => None,
        }
    }
}

impl FontChainKey {
    /// Create a FontChainKey from a slice of font selectors
    pub fn from_selectors(font_stack: &[FontSelector]) -> Self {
        let font_families: Vec<String> = font_stack
            .iter()
            .map(|s| s.family.clone())
            .filter(|f| !f.is_empty())
            .collect();

        let font_families = if font_families.is_empty() {
            vec!["serif".to_string()]
        } else {
            font_families
        };

        let weight = font_stack
            .first()
            .map(|s| s.weight)
            .unwrap_or(FcWeight::Normal);
        let is_italic = font_stack
            .first()
            .map(|s| s.style == FontStyle::Italic)
            .unwrap_or(false);
        let is_oblique = font_stack
            .first()
            .map(|s| s.style == FontStyle::Oblique)
            .unwrap_or(false);

        FontChainKey {
            font_families,
            weight,
            italic: is_italic,
            oblique: is_oblique,
        }
    }
}

/// A map of pre-loaded fonts, keyed by FontId (from rust-fontconfig)
///
/// This is passed to the shaper - no font loading happens during shaping
/// The fonts are loaded BEFORE layout based on the font chains and text content.
///
/// Provides both FontId and hash-based lookup for efficient glyph operations.
#[derive(Debug, Clone)]
pub struct LoadedFonts<T> {
    /// Primary storage: FontId -> Font
    pub fonts: HashMap<FontId, T>,
    /// Reverse index: font_hash -> FontId for fast hash-based lookups
    hash_to_id: HashMap<u64, FontId>,
}

impl<T: ParsedFontTrait> LoadedFonts<T> {
    pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            hash_to_id: HashMap::new(),
        }
    }

    /// Insert a font with its FontId
    pub fn insert(&mut self, font_id: FontId, font: T) {
        let hash = font.get_hash();
        self.hash_to_id.insert(hash, font_id.clone());
        self.fonts.insert(font_id, font);
    }

    /// Get a font by FontId
    pub fn get(&self, font_id: &FontId) -> Option<&T> {
        self.fonts.get(font_id)
    }

    /// Get a font by its hash
    pub fn get_by_hash(&self, hash: u64) -> Option<&T> {
        self.hash_to_id.get(&hash).and_then(|id| self.fonts.get(id))
    }

    /// Get the FontId for a hash
    pub fn get_font_id_by_hash(&self, hash: u64) -> Option<&FontId> {
        self.hash_to_id.get(&hash)
    }

    /// Check if a FontId is present
    pub fn contains_key(&self, font_id: &FontId) -> bool {
        self.fonts.contains_key(font_id)
    }

    /// Check if a hash is present
    pub fn contains_hash(&self, hash: u64) -> bool {
        self.hash_to_id.contains_key(&hash)
    }

    /// Iterate over all fonts
    pub fn iter(&self) -> impl Iterator<Item = (&FontId, &T)> {
        self.fonts.iter()
    }

    /// Get the number of loaded fonts
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl<T: ParsedFontTrait> Default for LoadedFonts<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ParsedFontTrait> FromIterator<(FontId, T)> for LoadedFonts<T> {
    fn from_iter<I: IntoIterator<Item = (FontId, T)>>(iter: I) -> Self {
        let mut loaded = LoadedFonts::new();
        for (id, font) in iter {
            loaded.insert(id, font);
        }
        loaded
    }
}

/// Enum that wraps either a fontconfig-resolved font (T) or a direct FontRef.
///
/// This allows the shaping code to handle both fontconfig-resolved fonts
/// and embedded fonts (FontRef) uniformly through the ParsedFontTrait interface.
#[derive(Debug, Clone)]
pub enum FontOrRef<T> {
    /// A font loaded via fontconfig
    Font(T),
    /// A direct FontRef (embedded font, bypasses fontconfig)
    Ref(azul_css::props::basic::FontRef),
}

impl<T: ParsedFontTrait> ShallowClone for FontOrRef<T> {
    fn shallow_clone(&self) -> Self {
        match self {
            FontOrRef::Font(f) => FontOrRef::Font(f.shallow_clone()),
            FontOrRef::Ref(r) => FontOrRef::Ref(r.clone()),
        }
    }
}

impl<T: ParsedFontTrait> ParsedFontTrait for FontOrRef<T> {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        match self {
            FontOrRef::Font(f) => f.shape_text(text, script, language, direction, style),
            FontOrRef::Ref(r) => r.shape_text(text, script, language, direction, style),
        }
    }

    fn get_hash(&self) -> u64 {
        match self {
            FontOrRef::Font(f) => f.get_hash(),
            FontOrRef::Ref(r) => r.get_hash(),
        }
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        match self {
            FontOrRef::Font(f) => f.get_glyph_size(glyph_id, font_size),
            FontOrRef::Ref(r) => r.get_glyph_size(glyph_id, font_size),
        }
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            FontOrRef::Font(f) => f.get_hyphen_glyph_and_advance(font_size),
            FontOrRef::Ref(r) => r.get_hyphen_glyph_and_advance(font_size),
        }
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            FontOrRef::Font(f) => f.get_kashida_glyph_and_advance(font_size),
            FontOrRef::Ref(r) => r.get_kashida_glyph_and_advance(font_size),
        }
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        match self {
            FontOrRef::Font(f) => f.has_glyph(codepoint),
            FontOrRef::Ref(r) => r.has_glyph(codepoint),
        }
    }

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        match self {
            FontOrRef::Font(f) => f.get_vertical_metrics(glyph_id),
            FontOrRef::Ref(r) => r.get_vertical_metrics(glyph_id),
        }
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        match self {
            FontOrRef::Font(f) => f.get_font_metrics(),
            FontOrRef::Ref(r) => r.get_font_metrics(),
        }
    }

    fn num_glyphs(&self) -> u16 {
        match self {
            FontOrRef::Font(f) => f.num_glyphs(),
            FontOrRef::Ref(r) => r.num_glyphs(),
        }
    }
}

#[derive(Debug)]
pub struct FontManager<T> {
    ///  Cache that holds the **file paths** of the fonts (not any font data itself)
    pub fc_cache: Arc<FcFontCache>,
    /// Holds the actual parsed font (usually with the font bytes attached)
    pub parsed_fonts: Mutex<HashMap<FontId, T>>,
    // Cache for font chains - populated by resolve_all_font_chains() before layout
    // This is read-only during layout - no locking needed for reads
    pub font_chain_cache: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    /// Cache for direct FontRefs (embedded fonts like Material Icons)
    /// These are fonts referenced via FontStack::Ref that bypass fontconfig
    pub embedded_fonts: Mutex<HashMap<u64, azul_css::props::basic::FontRef>>,
}

impl<T: ParsedFontTrait> FontManager<T> {
    pub fn new(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache: Arc::new(fc_cache),
            parsed_fonts: Mutex::new(HashMap::new()),
            font_chain_cache: HashMap::new(), // Populated via set_font_chain_cache()
            embedded_fonts: Mutex::new(HashMap::new()),
        })
    }

    /// Set the font chain cache from externally resolved chains
    ///
    /// This should be called with the result of `resolve_font_chains()` or
    /// `collect_and_resolve_font_chains()` from `solver3::getters`.
    pub fn set_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache = chains;
    }

    /// Merge additional font chains into the existing cache
    ///
    /// Useful when processing multiple DOMs that may have different font requirements.
    pub fn merge_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache.extend(chains);
    }

    /// Get a reference to the font chain cache
    pub fn get_font_chain_cache(
        &self,
    ) -> &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain> {
        &self.font_chain_cache
    }

    /// Get an embedded font by its hash (used for WebRender registration)
    /// Returns the FontRef if it exists in the embedded_fonts cache.
    pub fn get_embedded_font_by_hash(&self, font_hash: u64) -> Option<azul_css::props::basic::FontRef> {
        let embedded = self.embedded_fonts.lock().unwrap();
        embedded.get(&font_hash).cloned()
    }

    /// Get a parsed font by its hash (used for WebRender registration)
    /// Returns the parsed font if it exists in the parsed_fonts cache.
    pub fn get_font_by_hash(&self, font_hash: u64) -> Option<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // Linear search through all cached fonts to find one with matching hash
        for (_, font) in parsed.iter() {
            if font.get_hash() == font_hash {
                return Some(font.clone());
            }
        }
        None
    }

    /// Register an embedded FontRef for later lookup by hash
    /// This is called when using FontStack::Ref during shaping
    pub fn register_embedded_font(&self, font_ref: &azul_css::props::basic::FontRef) {
        let hash = font_ref.get_hash();
        let mut embedded = self.embedded_fonts.lock().unwrap();
        embedded.insert(hash, font_ref.clone());
    }

    /// Get a snapshot of all currently loaded fonts
    ///
    /// This returns a copy of all parsed fonts, which can be passed to the shaper.
    /// No locking is required after this call - the returned HashMap is independent.
    ///
    /// NOTE: This should be called AFTER loading all required fonts for a layout pass.
    pub fn get_loaded_fonts(&self) -> LoadedFonts<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        parsed
            .iter()
            .map(|(id, font)| (id.clone(), font.shallow_clone()))
            .collect()
    }

    /// Get the set of FontIds that are currently loaded
    ///
    /// This is useful for computing which fonts need to be loaded
    /// (diff with required fonts).
    pub fn get_loaded_font_ids(&self) -> std::collections::HashSet<FontId> {
        let parsed = self.parsed_fonts.lock().unwrap();
        parsed.keys().cloned().collect()
    }

    /// Insert a loaded font into the cache
    ///
    /// Returns the old font if one was already present for this FontId.
    pub fn insert_font(&self, font_id: FontId, font: T) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.insert(font_id, font)
    }

    /// Insert multiple loaded fonts into the cache
    ///
    /// This is more efficient than calling `insert_font` multiple times
    /// because it only acquires the lock once.
    pub fn insert_fonts(&self, fonts: impl IntoIterator<Item = (FontId, T)>) {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        for (font_id, font) in fonts {
            parsed.insert(font_id, font);
        }
    }

    /// Remove a font from the cache
    ///
    /// Returns the removed font if it was present.
    pub fn remove_font(&self, font_id: &FontId) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.remove(font_id)
    }
}

// Error handling
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("Bidi analysis failed: {0}")]
    BidiError(String),
    #[error("Shaping failed: {0}")]
    ShapingError(String),
    #[error("Font not found: {0:?}")]
    FontNotFound(FontSelector),
    #[error("Invalid text input: {0}")]
    InvalidText(String),
    #[error("Hyphenation failed: {0}")]
    HyphenationError(String),
}

/// Text boundary types for cursor movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBoundary {
    /// Reached top of text (first line)
    Top,
    /// Reached bottom of text (last line)
    Bottom,
    /// Reached start of text (first character)
    Start,
    /// Reached end of text (last character)
    End,
}

/// Error returned when cursor movement hits a boundary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorBoundsError {
    /// The boundary that was hit
    pub boundary: TextBoundary,
    /// The cursor position (unchanged from input)
    pub cursor: TextCursor,
}

/// Unified constraints combining all layout features
///
/// # CSS Inline Layout Module Level 3: Constraint Mapping
///
/// This structure maps CSS properties to layout constraints:
///
/// ## \u00a7 2.1 Layout of Line Boxes
/// - `available_width`: \u26a0\ufe0f CRITICAL - Should equal containing block's inner width
///   * Currently defaults to 0.0 which causes immediate line breaking
///   * Per spec: "logical width of a line box is equal to the inner logical width of its containing
///     block"
/// - `available_height`: For block-axis constraints (max-height)
///
/// ## \u00a7 2.2 Layout Within Line Boxes
/// - `text_align`: \u2705 Horizontal alignment (start, end, center, justify)
/// - `vertical_align`: \u26a0\ufe0f PARTIAL - Only baseline supported, missing:
///   * top, bottom, middle, text-top, text-bottom
///   * <length>, <percentage> values
///   * sub, super positions
/// - `line_height`: \u2705 Distance between baselines
///
/// ## \u00a7 3 Baselines and Alignment Metrics
/// - `text_orientation`: \u2705 For vertical writing (sideways, upright)
/// - `writing_mode`: \u2705 horizontal-tb, vertical-rl, vertical-lr
/// - `direction`: \u2705 ltr, rtl for BiDi
///
/// ## \u00a7 4 Baseline Alignment (vertical-align property)
/// \u26a0\ufe0f INCOMPLETE: Only basic baseline alignment implemented
///
/// ## \u00a7 5 Line Spacing (line-height property)
/// - `line_height`: \u2705 Implemented
/// - \u274c MISSING: line-fit-edge for controlling which edges contribute to line height
///
/// ## \u00a7 6 Trimming Leading (text-box-trim)
/// - \u274c NOT IMPLEMENTED: text-box-trim property
/// - \u274c NOT IMPLEMENTED: text-box-edge property
///
/// ## CSS Text Module Level 3
/// - `text_indent`: \u2705 First line indentation
/// - `text_justify`: \u2705 Justification algorithm (auto, inter-word, inter-character)
/// - `hyphenation`: \u2705 Automatic hyphenation
/// - `hanging_punctuation`: \u2705 Hanging punctuation at line edges
///
/// ## CSS Text Level 4
/// - `text_wrap`: \u2705 balance, pretty, stable
/// - `line_clamp`: \u2705 Max number of lines
///
/// ## CSS Writing Modes Level 4
/// - `text_combine_upright`: \u2705 Tate-chu-yoko for vertical text
///
/// ## CSS Shapes Module
/// - `shape_boundaries`: \u2705 Custom line box shapes
/// - `shape_exclusions`: \u2705 Exclusion areas (float-like behavior)
/// - `exclusion_margin`: \u2705 Margin around exclusions
///
/// ## Multi-column Layout
/// - `columns`: \u2705 Number of columns
/// - `column_gap`: \u2705 Gap between columns
///
/// # Known Issues:
/// 1. [ISSUE] available_width defaults to Definite(0.0) instead of containing block width
/// 2. [ISSUE] vertical_align only supports baseline
/// 3. [TODO] initial-letter (drop caps) not implemented
#[derive(Debug, Clone)]
pub struct UnifiedConstraints {
    // Shape definition
    pub shape_boundaries: Vec<ShapeBoundary>,
    pub shape_exclusions: Vec<ShapeBoundary>,

    // Basic layout - using AvailableSpace for proper indefinite handling
    pub available_width: AvailableSpace,
    pub available_height: Option<f32>,

    // Text layout
    pub writing_mode: Option<WritingMode>,
    // Base direction from CSS, overrides auto-detection
    pub direction: Option<BidiDirection>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub text_justify: JustifyContent,
    pub line_height: f32,
    pub vertical_align: VerticalAlign,

    // Overflow handling
    pub overflow: OverflowBehavior,
    pub segment_alignment: SegmentAlignment,

    // Advanced features
    pub text_combine_upright: Option<TextCombineUpright>,
    pub exclusion_margin: f32,
    pub hyphenation: bool,
    pub hyphenation_language: Option<Language>,
    pub text_indent: f32,
    pub initial_letter: Option<InitialLetter>,
    pub line_clamp: Option<NonZeroUsize>,

    // text-wrap: balance
    pub text_wrap: TextWrap,
    pub columns: u32,
    pub column_gap: f32,
    pub hanging_punctuation: bool,
}

impl Default for UnifiedConstraints {
    fn default() -> Self {
        Self {
            shape_boundaries: Vec::new(),
            shape_exclusions: Vec::new(),

            // IMPORTANT: This should be set to the containing block's inner width
            // per CSS Inline-3 § 2.1, but defaults to Definite(0.0) which causes immediate line
            // breaking. This value should be passed from the box layout solver (fc.rs)
            // when creating UnifiedConstraints for text layout.
            available_width: AvailableSpace::Definite(0.0),
            available_height: None,
            writing_mode: None,
            direction: None, // Will default to LTR if not specified
            text_orientation: TextOrientation::default(),
            text_align: TextAlign::default(),
            text_justify: JustifyContent::default(),
            line_height: 16.0, // A more sensible default
            vertical_align: VerticalAlign::default(),
            overflow: OverflowBehavior::default(),
            segment_alignment: SegmentAlignment::default(),
            text_combine_upright: None,
            exclusion_margin: 0.0,
            hyphenation: false,
            hyphenation_language: None,
            columns: 1,
            column_gap: 0.0,
            hanging_punctuation: false,
            text_indent: 0.0,
            initial_letter: None,
            line_clamp: None,
            text_wrap: TextWrap::default(),
        }
    }
}

// UnifiedConstraints
impl Hash for UnifiedConstraints {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_boundaries.hash(state);
        self.shape_exclusions.hash(state);
        self.available_width.hash(state);
        self.available_height
            .map(|h| h.round() as usize)
            .hash(state);
        self.writing_mode.hash(state);
        self.direction.hash(state);
        self.text_orientation.hash(state);
        self.text_align.hash(state);
        self.text_justify.hash(state);
        (self.line_height.round() as usize).hash(state);
        self.vertical_align.hash(state);
        self.overflow.hash(state);
        self.text_combine_upright.hash(state);
        (self.exclusion_margin.round() as usize).hash(state);
        self.hyphenation.hash(state);
        self.hyphenation_language.hash(state);
        self.columns.hash(state);
        (self.column_gap.round() as usize).hash(state);
        self.hanging_punctuation.hash(state);
    }
}

impl PartialEq for UnifiedConstraints {
    fn eq(&self, other: &Self) -> bool {
        self.shape_boundaries == other.shape_boundaries
            && self.shape_exclusions == other.shape_exclusions
            && self.available_width == other.available_width
            && match (self.available_height, other.available_height) {
                (None, None) => true,
                (Some(h1), Some(h2)) => round_eq(h1, h2),
                _ => false,
            }
            && self.writing_mode == other.writing_mode
            && self.direction == other.direction
            && self.text_orientation == other.text_orientation
            && self.text_align == other.text_align
            && self.text_justify == other.text_justify
            && round_eq(self.line_height, other.line_height)
            && self.vertical_align == other.vertical_align
            && self.overflow == other.overflow
            && self.text_combine_upright == other.text_combine_upright
            && round_eq(self.exclusion_margin, other.exclusion_margin)
            && self.hyphenation == other.hyphenation
            && self.hyphenation_language == other.hyphenation_language
            && self.columns == other.columns
            && round_eq(self.column_gap, other.column_gap)
            && self.hanging_punctuation == other.hanging_punctuation
    }
}

impl Eq for UnifiedConstraints {}

impl UnifiedConstraints {
    fn direction(&self, fallback: BidiDirection) -> BidiDirection {
        match self.writing_mode {
            Some(s) => s.get_direction().unwrap_or(fallback),
            None => fallback,
        }
    }
    fn is_vertical(&self) -> bool {
        matches!(
            self.writing_mode,
            Some(WritingMode::VerticalRl) | Some(WritingMode::VerticalLr)
        )
    }
}

/// Line constraints with multi-segment support
#[derive(Debug, Clone)]
pub struct LineConstraints {
    pub segments: Vec<LineSegment>,
    pub total_available: f32,
}

impl WritingMode {
    fn get_direction(&self) -> Option<BidiDirection> {
        match self {
            // determined by text content
            WritingMode::HorizontalTb => None,
            WritingMode::VerticalRl => Some(BidiDirection::Rtl),
            WritingMode::VerticalLr => Some(BidiDirection::Ltr),
            WritingMode::SidewaysRl => Some(BidiDirection::Rtl),
            WritingMode::SidewaysLr => Some(BidiDirection::Ltr),
        }
    }
}

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone, Hash)]
pub struct StyledRun {
    pub text: String,
    pub style: Arc<StyleProperties>,
    /// Byte index in the original logical paragraph text
    pub logical_start_byte: usize,
    /// The DOM NodeId of the Text node this run came from.
    /// None for generated content (e.g., list markers, ::before/::after).
    pub source_node_id: Option<NodeId>,
}

// Stage 2: Bidi Analysis - Visual runs in display order
#[derive(Debug, Clone)]
pub struct VisualRun<'a> {
    pub text_slice: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start_byte: usize,
    pub bidi_level: BidiLevel,
    pub script: Script,
    pub language: Language,
}

// Font and styling types

/// A selector for loading fonts from the font cache.
/// Used by FontManager to query fontconfig and load font files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontSelector {
    pub family: String,
    pub weight: FcWeight,
    pub style: FontStyle,
    pub unicode_ranges: Vec<UnicodeRange>,
}

impl Default for FontSelector {
    fn default() -> Self {
        Self {
            family: "serif".to_string(),
            weight: FcWeight::Normal,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
        }
    }
}

/// Font stack that can be either a list of font selectors (resolved via fontconfig)
/// or a direct FontRef (bypasses fontconfig entirely).
///
/// When a `FontRef` is used, it bypasses fontconfig resolution entirely
/// and uses the pre-parsed font data directly. This is used for embedded
/// fonts like Material Icons.
#[derive(Debug, Clone)]
pub enum FontStack {
    /// A stack of font selectors to be resolved via fontconfig
    /// First font is primary, rest are fallbacks
    Stack(Vec<FontSelector>),
    /// A direct reference to a pre-parsed font (e.g., embedded icon fonts)
    /// This font covers the entire Unicode range and has no fallbacks.
    Ref(azul_css::props::basic::font::FontRef),
}

impl Default for FontStack {
    fn default() -> Self {
        FontStack::Stack(vec![FontSelector::default()])
    }
}

impl FontStack {
    /// Returns true if this is a direct FontRef
    pub fn is_ref(&self) -> bool {
        matches!(self, FontStack::Ref(_))
    }

    /// Returns the FontRef if this is a Ref variant
    pub fn as_ref(&self) -> Option<&azul_css::props::basic::font::FontRef> {
        match self {
            FontStack::Ref(r) => Some(r),
            _ => None,
        }
    }

    /// Returns the font selectors if this is a Stack variant
    pub fn as_stack(&self) -> Option<&[FontSelector]> {
        match self {
            FontStack::Stack(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the first FontSelector if this is a Stack variant, None if Ref
    pub fn first_selector(&self) -> Option<&FontSelector> {
        match self {
            FontStack::Stack(s) => s.first(),
            FontStack::Ref(_) => None,
        }
    }

    /// Returns the first font family name (for Stack) or a placeholder (for Ref)
    pub fn first_family(&self) -> &str {
        match self {
            FontStack::Stack(s) => s.first().map(|f| f.family.as_str()).unwrap_or("serif"),
            FontStack::Ref(_) => "<embedded-font>",
        }
    }
}

impl PartialEq for FontStack {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FontStack::Stack(a), FontStack::Stack(b)) => a == b,
            (FontStack::Ref(a), FontStack::Ref(b)) => a.parsed == b.parsed,
            _ => false,
        }
    }
}

impl Eq for FontStack {}

impl Hash for FontStack {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            FontStack::Stack(s) => s.hash(state),
            FontStack::Ref(r) => (r.parsed as usize).hash(state),
        }
    }
}

/// A reference to a font for rendering, identified by its hash.
/// This hash corresponds to ParsedFont::hash and is used to look up
/// the actual font data in the renderer's font cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontHash {
    /// The hash of the ParsedFont. 0 means invalid/unknown font.
    pub font_hash: u64,
}

impl FontHash {
    pub fn invalid() -> Self {
        Self { font_hash: 0 }
    }

    pub fn from_hash(font_hash: u64) -> Self {
        Self { font_hash }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Defines how text should be aligned when a line contains multiple disjoint segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SegmentAlignment {
    /// Align text within the first available segment on the line.
    #[default]
    First,
    /// Align text relative to the total available width of all
    /// segments on the line combined.
    Total,
}

#[derive(Debug, Clone)]
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

/// Layout-specific font metrics extracted from FontMetrics
/// Contains only the metrics needed for text layout and rendering
#[derive(Debug, Clone)]
pub struct LayoutFontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: u16,
}

impl LayoutFontMetrics {
    pub fn baseline_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / self.units_per_em as f32;
        self.ascent * scale
    }

    /// Convert from full FontMetrics to layout-specific metrics
    pub fn from_font_metrics(metrics: &azul_css::props::basic::FontMetrics) -> Self {
        Self {
            ascent: metrics.ascender as f32,
            descent: metrics.descender as f32,
            line_gap: metrics.line_gap as f32,
            units_per_em: metrics.units_per_em,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LineSegment {
    pub start_x: f32,
    pub width: f32,
    // For choosing best segment when multiple available
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextWrap {
    #[default]
    Wrap,
    Balance,
    NoWrap,
}

// initial-letter
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct InitialLetter {
    /// How many lines tall the initial letter should be.
    pub size: f32,
    /// How many lines the letter should sink into.
    pub sink: u32,
    /// How many characters to apply this styling to.
    pub count: NonZeroUsize,
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// This is a marker trait, indicating that `a == b` is a true equivalence
// relation. The derived `PartialEq` already satisfies this.
impl Eq for InitialLetter {}

impl Hash for InitialLetter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Per the request, round the f32 to a usize for hashing.
        // This is a lossy conversion; values like 2.3 and 2.4 will produce
        // the same hash value for this field. This is acceptable as long as
        // the `PartialEq` implementation correctly distinguishes them.
        (self.size.round() as usize).hash(state);
        self.sink.hash(state);
        self.count.hash(state);
    }
}

// Path and shape definitions
#[derive(Debug, Clone, PartialOrd)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    CurveTo {
        control1: Point,
        control2: Point,
        end: Point,
    },
    QuadTo {
        control: Point,
        end: Point,
    },
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
    },
    Close,
}

// PathSegment
impl Hash for PathSegment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the enum variant's discriminant first to distinguish them
        discriminant(self).hash(state);

        match self {
            PathSegment::MoveTo(p) => p.hash(state),
            PathSegment::LineTo(p) => p.hash(state),
            PathSegment::CurveTo {
                control1,
                control2,
                end,
            } => {
                control1.hash(state);
                control2.hash(state);
                end.hash(state);
            }
            PathSegment::QuadTo { control, end } => {
                control.hash(state);
                end.hash(state);
            }
            PathSegment::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
                (start_angle.round() as usize).hash(state);
                (end_angle.round() as usize).hash(state);
            }
            PathSegment::Close => {} // No data to hash
        }
    }
}

impl PartialEq for PathSegment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PathSegment::MoveTo(a), PathSegment::MoveTo(b)) => a == b,
            (PathSegment::LineTo(a), PathSegment::LineTo(b)) => a == b,
            (
                PathSegment::CurveTo {
                    control1: c1a,
                    control2: c2a,
                    end: ea,
                },
                PathSegment::CurveTo {
                    control1: c1b,
                    control2: c2b,
                    end: eb,
                },
            ) => c1a == c1b && c2a == c2b && ea == eb,
            (
                PathSegment::QuadTo {
                    control: ca,
                    end: ea,
                },
                PathSegment::QuadTo {
                    control: cb,
                    end: eb,
                },
            ) => ca == cb && ea == eb,
            (
                PathSegment::Arc {
                    center: ca,
                    radius: ra,
                    start_angle: sa_a,
                    end_angle: ea_a,
                },
                PathSegment::Arc {
                    center: cb,
                    radius: rb,
                    start_angle: sa_b,
                    end_angle: ea_b,
                },
            ) => ca == cb && round_eq(*ra, *rb) && round_eq(*sa_a, *sa_b) && round_eq(*ea_a, *ea_b),
            (PathSegment::Close, PathSegment::Close) => true,
            _ => false, // Variants are different
        }
    }
}

impl Eq for PathSegment {}

// Enhanced content model supporting mixed inline content
#[derive(Debug, Clone, Hash)]
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Shape(InlineShape),
    Space(InlineSpace),
    LineBreak(InlineBreak),
    Tab,
    /// List marker (::marker pseudo-element)
    /// Markers with list-style-position: outside are positioned
    /// in the padding gutter of the list container
    Marker {
        run: StyledRun,
        /// Whether marker is positioned outside (in padding) or inside (inline)
        position_outside: bool,
    },
    // Ruby annotation
    Ruby {
        base: Vec<InlineContent>,
        text: Vec<InlineContent>,
        // Style for the ruby text itself
        style: Arc<StyleProperties>,
    },
}

#[derive(Debug, Clone)]
pub struct InlineImage {
    pub source: ImageSource,
    pub intrinsic_size: Size,
    pub display_size: Option<Size>,
    // How much to shift baseline
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub object_fit: ObjectFit,
}

impl PartialEq for InlineImage {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.source == other.source
            && self.intrinsic_size == other.intrinsic_size
            && self.display_size == other.display_size
            && self.alignment == other.alignment
            && self.object_fit == other.object_fit
    }
}

impl Eq for InlineImage {}

impl Hash for InlineImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.intrinsic_size.hash(state);
        self.display_size.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.alignment.hash(state);
        self.object_fit.hash(state);
    }
}

impl PartialOrd for InlineImage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineImage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source
            .cmp(&other.source)
            .then_with(|| self.intrinsic_size.cmp(&other.intrinsic_size))
            .then_with(|| self.display_size.cmp(&other.display_size))
            .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
            .then_with(|| self.alignment.cmp(&other.alignment))
            .then_with(|| self.object_fit.cmp(&other.object_fit))
    }
}

/// Enhanced glyph with all features
#[derive(Debug, Clone)]
pub struct Glyph {
    // Core glyph data
    pub glyph_id: u16,
    pub codepoint: char,
    /// Hash of the font - use LoadedFonts to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
    pub style: Arc<StyleProperties>,
    pub source: GlyphSource,

    // Text mapping
    pub logical_byte_index: usize,
    pub logical_byte_len: usize,
    pub content_index: usize,
    pub cluster: u32,

    // Metrics
    pub advance: f32,
    pub kerning: f32,
    pub offset: Point,

    // Vertical text support
    pub vertical_advance: f32,
    pub vertical_origin_y: f32, // from VORG
    pub vertical_bearing: Point,
    pub orientation: GlyphOrientation,

    // Layout properties
    pub script: Script,
    pub bidi_level: BidiLevel,
}

impl Glyph {
    #[inline]
    fn bounds(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: self.advance,
            height: self.style.line_height,
        }
    }

    #[inline]
    fn character_class(&self) -> CharacterClass {
        classify_character(self.codepoint as u32)
    }

    #[inline]
    fn is_whitespace(&self) -> bool {
        self.character_class() == CharacterClass::Space
    }

    #[inline]
    fn can_justify(&self) -> bool {
        !self.codepoint.is_whitespace() && self.character_class() != CharacterClass::Combining
    }

    #[inline]
    fn justification_priority(&self) -> u8 {
        get_justification_priority(self.character_class())
    }

    #[inline]
    fn break_opportunity_after(&self) -> bool {
        let is_whitespace = self.codepoint.is_whitespace();
        let is_soft_hyphen = self.codepoint == '\u{00AD}';
        is_whitespace || is_soft_hyphen
    }
}

// Information about text runs after initial analysis
#[derive(Debug, Clone)]
pub struct TextRunInfo<'a> {
    pub text: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start: usize,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Direct reference to decoded image (from DOM NodeType::Image)
    Ref(ImageRef),
    /// CSS url reference (from background-image, needs ImageCache lookup)
    Url(String),
    /// Raw image data
    Data(Arc<[u8]>),
    /// SVG source
    Svg(Arc<str>),
    /// Placeholder for layout without actual image
    Placeholder(Size),
}

impl PartialEq for ImageSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ImageSource::Ref(a), ImageSource::Ref(b)) => a.get_hash() == b.get_hash(),
            (ImageSource::Url(a), ImageSource::Url(b)) => a == b,
            (ImageSource::Data(a), ImageSource::Data(b)) => Arc::ptr_eq(a, b),
            (ImageSource::Svg(a), ImageSource::Svg(b)) => Arc::ptr_eq(a, b),
            (ImageSource::Placeholder(a), ImageSource::Placeholder(b)) => {
                a.width.to_bits() == b.width.to_bits() && a.height.to_bits() == b.height.to_bits()
            }
            _ => false,
        }
    }
}

impl Eq for ImageSource {}

impl std::hash::Hash for ImageSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            ImageSource::Ref(r) => r.get_hash().hash(state),
            ImageSource::Url(s) => s.hash(state),
            ImageSource::Data(d) => (Arc::as_ptr(d) as *const u8 as usize).hash(state),
            ImageSource::Svg(s) => (Arc::as_ptr(s) as *const u8 as usize).hash(state),
            ImageSource::Placeholder(sz) => {
                sz.width.to_bits().hash(state);
                sz.height.to_bits().hash(state);
            }
        }
    }
}

impl PartialOrd for ImageSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImageSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index(s: &ImageSource) -> u8 {
            match s {
                ImageSource::Ref(_) => 0,
                ImageSource::Url(_) => 1,
                ImageSource::Data(_) => 2,
                ImageSource::Svg(_) => 3,
                ImageSource::Placeholder(_) => 4,
            }
        }
        match (self, other) {
            (ImageSource::Ref(a), ImageSource::Ref(b)) => a.get_hash().cmp(&b.get_hash()),
            (ImageSource::Url(a), ImageSource::Url(b)) => a.cmp(b),
            (ImageSource::Data(a), ImageSource::Data(b)) => {
                (Arc::as_ptr(a) as *const u8 as usize).cmp(&(Arc::as_ptr(b) as *const u8 as usize))
            }
            (ImageSource::Svg(a), ImageSource::Svg(b)) => {
                (Arc::as_ptr(a) as *const u8 as usize).cmp(&(Arc::as_ptr(b) as *const u8 as usize))
            }
            (ImageSource::Placeholder(a), ImageSource::Placeholder(b)) => {
                (a.width.to_bits(), a.height.to_bits())
                    .cmp(&(b.width.to_bits(), b.height.to_bits()))
            }
            // Different variants: compare by variant index
            _ => variant_index(self).cmp(&variant_index(other)),
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum VerticalAlign {
    // Align image baseline with text baseline
    #[default]
    Baseline,
    // Align image bottom with line bottom
    Bottom,
    // Align image top with line top
    Top,
    // Align image middle with text middle
    Middle,
    // Align with tallest text in line
    TextTop,
    // Align with lowest text in line
    TextBottom,
    // Subscript alignment
    Sub,
    // Superscript alignment
    Super,
    // Custom offset from baseline
    Offset(f32),
}

impl std::hash::Hash for VerticalAlign {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let VerticalAlign::Offset(f) = self {
            f.to_bits().hash(state);
        }
    }
}

impl Eq for VerticalAlign {}

impl Ord for VerticalAlign {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectFit {
    // Stretch to fit display size
    Fill,
    // Scale to fit within display size
    Contain,
    // Scale to cover display size
    Cover,
    // Use intrinsic size
    None,
    // Like contain but never scale up
    ScaleDown,
}

/// Border information for inline elements (display: inline, inline-block)
///
/// This stores the resolved border properties needed for rendering inline element borders.
/// Unlike block elements which render borders via paint_node_background_and_border(),
/// inline element borders must be rendered per glyph-run to handle line breaks correctly.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineBorderInfo {
    /// Border widths in pixels for each side
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    /// Border colors for each side
    pub top_color: ColorU,
    pub right_color: ColorU,
    pub bottom_color: ColorU,
    pub left_color: ColorU,
    /// Border radius (if any)
    pub radius: Option<f32>,
}

impl Default for InlineBorderInfo {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
            top_color: ColorU::TRANSPARENT,
            right_color: ColorU::TRANSPARENT,
            bottom_color: ColorU::TRANSPARENT,
            left_color: ColorU::TRANSPARENT,
            radius: None,
        }
    }
}

impl InlineBorderInfo {
    /// Returns true if any border has a non-zero width
    pub fn has_border(&self) -> bool {
        self.top > 0.0 || self.right > 0.0 || self.bottom > 0.0 || self.left > 0.0
    }
}

#[derive(Debug, Clone)]
pub struct InlineShape {
    pub shape_def: ShapeDefinition,
    pub fill: Option<ColorU>,
    pub stroke: Option<Stroke>,
    pub baseline_offset: f32,
    /// The NodeId of the element that created this shape
    /// (e.g., inline-block) - this allows us to look up
    /// styling information (background, border) when rendering
    pub source_node_id: Option<azul_core::dom::NodeId>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverflowBehavior {
    // Content extends outside shape
    Visible,
    // Content is clipped to shape
    Hidden,
    // Scrollable overflow
    Scroll,
    // Browser/system decides
    #[default]
    Auto,
    // Break into next shape/page
    Break,
}

#[derive(Debug, Clone)]
pub struct MeasuredImage {
    pub source: ImageSource,
    pub size: Size,
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub struct MeasuredShape {
    pub shape_def: ShapeDefinition,
    pub size: Size,
    pub baseline_offset: f32,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub struct InlineSpace {
    pub width: f32,
    pub is_breaking: bool, // Can line break here
    pub is_stretchy: bool, // Can be expanded for justification
}

impl PartialEq for InlineSpace {
    fn eq(&self, other: &Self) -> bool {
        self.width.to_bits() == other.width.to_bits()
            && self.is_breaking == other.is_breaking
            && self.is_stretchy == other.is_stretchy
    }
}

impl Eq for InlineSpace {}

impl Hash for InlineSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.to_bits().hash(state);
        self.is_breaking.hash(state);
        self.is_stretchy.hash(state);
    }
}

impl PartialOrd for InlineSpace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineSpace {
    fn cmp(&self, other: &Self) -> Ordering {
        self.width
            .total_cmp(&other.width)
            .then_with(|| self.is_breaking.cmp(&other.is_breaking))
            .then_with(|| self.is_stretchy.cmp(&other.is_stretchy))
    }
}

impl PartialEq for InlineShape {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.shape_def == other.shape_def
            && self.fill == other.fill
            && self.stroke == other.stroke
            && self.source_node_id == other.source_node_id
    }
}

impl Eq for InlineShape {}

impl Hash for InlineShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_def.hash(state);
        self.fill.hash(state);
        self.stroke.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.source_node_id.hash(state);
    }
}

impl PartialOrd for InlineShape {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.shape_def
                .partial_cmp(&other.shape_def)?
                .then_with(|| self.fill.cmp(&other.fill))
                .then_with(|| {
                    self.stroke
                        .partial_cmp(&other.stroke)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
                .then_with(|| self.source_node_id.cmp(&other.source_node_id)),
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PartialEq for Rect {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x)
            && round_eq(self.y, other.y)
            && round_eq(self.width, other.width)
            && round_eq(self.height, other.height)
    }
}
impl Eq for Rect {}

impl Hash for Rect {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The order in which you hash the fields matters.
        // A consistent order is crucial.
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Ord for Size {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.width.round() as usize)
            .cmp(&(other.width.round() as usize))
            .then_with(|| (self.height.round() as usize).cmp(&(other.height.round() as usize)))
    }
}

// Size
impl Hash for Size {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}
impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.width, other.width) && round_eq(self.height, other.height)
    }
}
impl Eq for Size {}

impl Size {
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Point
impl Hash for Point {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
    }
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x) && round_eq(self.y, other.y)
    }
}

impl Eq for Point {}

#[derive(Debug, Clone, PartialOrd)]
pub enum ShapeDefinition {
    Rectangle {
        size: Size,
        corner_radius: Option<f32>,
    },
    Circle {
        radius: f32,
    },
    Ellipse {
        radii: Size,
    },
    Polygon {
        points: Vec<Point>,
    },
    Path {
        segments: Vec<PathSegment>,
    },
}

// ShapeDefinition
impl Hash for ShapeDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeDefinition::Rectangle {
                size,
                corner_radius,
            } => {
                size.hash(state);
                corner_radius.map(|r| r.round() as usize).hash(state);
            }
            ShapeDefinition::Circle { radius } => {
                (radius.round() as usize).hash(state);
            }
            ShapeDefinition::Ellipse { radii } => {
                radii.hash(state);
            }
            ShapeDefinition::Polygon { points } => {
                // Since Point implements Hash, we can hash the Vec directly.
                points.hash(state);
            }
            ShapeDefinition::Path { segments } => {
                // Same for Vec<PathSegment>
                segments.hash(state);
            }
        }
    }
}

impl PartialEq for ShapeDefinition {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ShapeDefinition::Rectangle {
                    size: s1,
                    corner_radius: r1,
                },
                ShapeDefinition::Rectangle {
                    size: s2,
                    corner_radius: r2,
                },
            ) => {
                s1 == s2
                    && match (r1, r2) {
                        (None, None) => true,
                        (Some(v1), Some(v2)) => round_eq(*v1, *v2),
                        _ => false,
                    }
            }
            (ShapeDefinition::Circle { radius: r1 }, ShapeDefinition::Circle { radius: r2 }) => {
                round_eq(*r1, *r2)
            }
            (ShapeDefinition::Ellipse { radii: r1 }, ShapeDefinition::Ellipse { radii: r2 }) => {
                r1 == r2
            }
            (ShapeDefinition::Polygon { points: p1 }, ShapeDefinition::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeDefinition::Path { segments: s1 }, ShapeDefinition::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeDefinition {}

impl ShapeDefinition {
    /// Calculates the bounding box size for the shape.
    pub fn get_size(&self) -> Size {
        match self {
            // The size is explicitly defined.
            ShapeDefinition::Rectangle { size, .. } => *size,

            // The bounding box of a circle is a square with sides equal to the diameter.
            ShapeDefinition::Circle { radius } => {
                let diameter = radius * 2.0;
                Size::new(diameter, diameter)
            }

            // The bounding box of an ellipse has width and height equal to twice its radii.
            ShapeDefinition::Ellipse { radii } => Size::new(radii.width * 2.0, radii.height * 2.0),

            // For a polygon, we must find the min/max coordinates to get the bounds.
            ShapeDefinition::Polygon { points } => calculate_bounding_box_size(points),

            // For a path, we find the bounding box of all its anchor and control points.
            //
            // NOTE: This is a common and fast approximation. The true bounding box of
            // bezier curves can be slightly smaller than the box containing their control
            // points. For pixel-perfect results, one would need to calculate the
            // curve's extrema.
            ShapeDefinition::Path { segments } => {
                let mut points = Vec::new();
                let mut current_pos = Point { x: 0.0, y: 0.0 };

                for segment in segments {
                    match segment {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                            points.push(*p);
                            current_pos = *p;
                        }
                        PathSegment::QuadTo { control, end } => {
                            points.push(current_pos);
                            points.push(*control);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::CurveTo {
                            control1,
                            control2,
                            end,
                        } => {
                            points.push(current_pos);
                            points.push(*control1);
                            points.push(*control2);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::Arc {
                            center,
                            radius,
                            start_angle,
                            end_angle,
                        } => {
                            // 1. Calculate and add the arc's start and end points to the list.
                            let start_point = Point {
                                x: center.x + radius * start_angle.cos(),
                                y: center.y + radius * start_angle.sin(),
                            };
                            let end_point = Point {
                                x: center.x + radius * end_angle.cos(),
                                y: center.y + radius * end_angle.sin(),
                            };
                            points.push(start_point);
                            points.push(end_point);

                            // 2. Normalize the angles to handle cases where the arc crosses the
                            //    0-radian line.
                            // This ensures we can iterate forward from a start to an end angle.
                            let mut normalized_end = *end_angle;
                            while normalized_end < *start_angle {
                                normalized_end += 2.0 * std::f32::consts::PI;
                            }

                            // 3. Find the first cardinal point (multiples of PI/2) at or after the
                            //    start angle.
                            let mut check_angle = (*start_angle / std::f32::consts::FRAC_PI_2)
                                .ceil()
                                * std::f32::consts::FRAC_PI_2;

                            // 4. Iterate through all cardinal points that fall within the arc's
                            //    sweep and add them.
                            // These points define the maximum extent of the arc's bounding box.
                            while check_angle < normalized_end {
                                points.push(Point {
                                    x: center.x + radius * check_angle.cos(),
                                    y: center.y + radius * check_angle.sin(),
                                });
                                check_angle += std::f32::consts::FRAC_PI_2;
                            }

                            // 5. The end of the arc is the new current position for subsequent path
                            //    segments.
                            current_pos = end_point;
                        }
                        PathSegment::Close => {
                            // No new points are added for closing the path
                        }
                    }
                }
                calculate_bounding_box_size(&points)
            }
        }
    }
}

/// Helper function to calculate the size of the bounding box enclosing a set of points.
fn calculate_bounding_box_size(points: &[Point]) -> Size {
    if points.is_empty() {
        return Size::zero();
    }

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;

    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }

    // Handle case where points might be collinear or a single point
    if min_x > max_x || min_y > max_y {
        return Size::zero();
    }

    Size::new(max_x - min_x, max_y - min_y)
}

#[derive(Debug, Clone, PartialOrd)]
pub struct Stroke {
    pub color: ColorU,
    pub width: f32,
    pub dash_pattern: Option<Vec<f32>>,
}

// Stroke
impl Hash for Stroke {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        (self.width.round() as usize).hash(state);

        // Manual hashing for Option<Vec<f32>>
        match &self.dash_pattern {
            None => 0u8.hash(state), // Hash a discriminant for None
            Some(pattern) => {
                1u8.hash(state); // Hash a discriminant for Some
                pattern.len().hash(state); // Hash the length
                for &val in pattern {
                    (val.round() as usize).hash(state); // Hash each rounded value
                }
            }
        }
    }
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        if self.color != other.color || !round_eq(self.width, other.width) {
            return false;
        }
        match (&self.dash_pattern, &other.dash_pattern) {
            (None, None) => true,
            (Some(p1), Some(p2)) => {
                p1.len() == p2.len() && p1.iter().zip(p2.iter()).all(|(a, b)| round_eq(*a, *b))
            }
            _ => false,
        }
    }
}

impl Eq for Stroke {}

// Helper function to round f32 for comparison
fn round_eq(a: f32, b: f32) -> bool {
    (a.round() as isize) == (b.round() as isize)
}

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
}

impl ShapeBoundary {
    pub fn inflate(&self, margin: f32) -> Self {
        if margin == 0.0 {
            return self.clone();
        }
        match self {
            Self::Rectangle(rect) => Self::Rectangle(Rect {
                x: rect.x - margin,
                y: rect.y - margin,
                width: (rect.width + margin * 2.0).max(0.0),
                height: (rect.height + margin * 2.0).max(0.0),
            }),
            Self::Circle { center, radius } => Self::Circle {
                center: *center,
                radius: radius + margin,
            },
            // For simplicity, Polygon and Path inflation is not implemented here.
            // A full implementation would require a geometry library to offset the path.
            _ => self.clone(),
        }
    }
}

// ShapeBoundary
impl Hash for ShapeBoundary {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeBoundary::Rectangle(rect) => rect.hash(state),
            ShapeBoundary::Circle { center, radius } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
            }
            ShapeBoundary::Ellipse { center, radii } => {
                center.hash(state);
                radii.hash(state);
            }
            ShapeBoundary::Polygon { points } => points.hash(state),
            ShapeBoundary::Path { segments } => segments.hash(state),
        }
    }
}
impl PartialEq for ShapeBoundary {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ShapeBoundary::Rectangle(r1), ShapeBoundary::Rectangle(r2)) => r1 == r2,
            (
                ShapeBoundary::Circle {
                    center: c1,
                    radius: r1,
                },
                ShapeBoundary::Circle {
                    center: c2,
                    radius: r2,
                },
            ) => c1 == c2 && round_eq(*r1, *r2),
            (
                ShapeBoundary::Ellipse {
                    center: c1,
                    radii: r1,
                },
                ShapeBoundary::Ellipse {
                    center: c2,
                    radii: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (ShapeBoundary::Polygon { points: p1 }, ShapeBoundary::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeBoundary::Path { segments: s1 }, ShapeBoundary::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeBoundary {}

impl ShapeBoundary {
    /// Converts a CSS shape (from azul-css) to a layout engine ShapeBoundary
    ///
    /// # Arguments
    /// * `css_shape` - The parsed CSS shape from azul-css
    /// * `reference_box` - The containing box for resolving coordinates (from layout solver)
    ///
    /// # Returns
    /// A ShapeBoundary ready for use in the text layout engine
    pub fn from_css_shape(
        css_shape: &azul_css::shape::CssShape,
        reference_box: Rect,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self {
        use azul_css::shape::CssShape;

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Input CSS shape: {:?}",
                css_shape
            )));
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Reference box: {:?}",
                reference_box
            )));
        }

        let result = match css_shape {
            CssShape::Circle(circle) => {
                let center = Point {
                    x: reference_box.x + circle.center.x,
                    y: reference_box.y + circle.center.y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - CSS center: ({}, {}), radius: {}",
                        circle.center.x, circle.center.y, circle.radius
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - Absolute center: ({}, {}), \
                         radius: {}",
                        center.x, center.y, circle.radius
                    )));
                }
                ShapeBoundary::Circle {
                    center,
                    radius: circle.radius,
                }
            }

            CssShape::Ellipse(ellipse) => {
                let center = Point {
                    x: reference_box.x + ellipse.center.x,
                    y: reference_box.y + ellipse.center.y,
                };
                let radii = Size {
                    width: ellipse.radius_x,
                    height: ellipse.radius_y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Ellipse - center: ({}, {}), radii: ({}, \
                         {})",
                        center.x, center.y, radii.width, radii.height
                    )));
                }
                ShapeBoundary::Ellipse { center, radii }
            }

            CssShape::Polygon(polygon) => {
                let points = polygon
                    .points
                    .as_ref()
                    .iter()
                    .map(|pt| Point {
                        x: reference_box.x + pt.x,
                        y: reference_box.y + pt.y,
                    })
                    .collect();
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Polygon - {} points",
                        polygon.points.as_ref().len()
                    )));
                }
                ShapeBoundary::Polygon { points }
            }

            CssShape::Inset(inset) => {
                // Inset defines distances from reference box edges
                let x = reference_box.x + inset.inset_left;
                let y = reference_box.y + inset.inset_top;
                let width = reference_box.width - inset.inset_left - inset.inset_right;
                let height = reference_box.height - inset.inset_top - inset.inset_bottom;

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - insets: ({}, {}, {}, {})",
                        inset.inset_top, inset.inset_right, inset.inset_bottom, inset.inset_left
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - resulting rect: x={}, y={}, \
                         w={}, h={}",
                        x, y, width, height
                    )));
                }

                ShapeBoundary::Rectangle(Rect {
                    x,
                    y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                })
            }

            CssShape::Path(path) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "[ShapeBoundary::from_css_shape] Path - fallback to rectangle".to_string(),
                    ));
                }
                // TODO: Parse SVG path data into PathSegments
                // For now, fall back to rectangle
                ShapeBoundary::Rectangle(reference_box)
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Result: {:?}",
                result
            )));
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineBreak {
    pub break_type: BreakType,
    pub clear: ClearType,
    pub content_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BreakType {
    Soft,   // Preferred break (like <wbr>)
    Hard,   // Forced break (like <br>)
    Page,   // Page break
    Column, // Column break
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClearType {
    None,
    Left,
    Right,
    Both,
}

// Complex shape constraints for non-rectangular text flow
#[derive(Debug, Clone)]
pub struct ShapeConstraints {
    pub boundaries: Vec<ShapeBoundary>,
    pub exclusions: Vec<ShapeBoundary>,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum WritingMode {
    #[default]
    HorizontalTb, // horizontal-tb (normal horizontal)
    VerticalRl, // vertical-rl (vertical right-to-left)
    VerticalLr, // vertical-lr (vertical left-to-right)
    SidewaysRl, // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr, // sideways-lr (rotated horizontal in vertical context)
}

impl WritingMode {
    /// Necessary to determine if the glyphs are advancing in a horizontal direction
    pub fn is_advance_horizontal(&self) -> bool {
        matches!(
            self,
            WritingMode::HorizontalTb | WritingMode::SidewaysRl | WritingMode::SidewaysLr
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum JustifyContent {
    #[default]
    None,
    InterWord,      // Expand spaces between words
    InterCharacter, // Expand spaces between all characters (for CJK)
    Distribute,     // Distribute space evenly including start/end
    Kashida,        // Stretch Arabic text using kashidas
}

// Enhanced text alignment with logical directions
#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,        // Logical start/end
    JustifyAll, // Justify including last line
}

// Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq, Default, Eq, PartialOrd, Ord, Hash)]
pub enum TextOrientation {
    #[default]
    Mixed, // Default: upright for scripts, rotated for others
    Upright,  // All characters upright
    Sideways, // All characters rotated 90 degrees
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

impl Default for TextDecoration {
    fn default() -> Self {
        TextDecoration {
            underline: false,
            overline: false,
            strikethrough: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

// Type alias for OpenType feature tags
pub type FourCc = [u8; 4];

// Enum for relative or absolute spacing
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Spacing {
    Px(i32), // Use integer pixels to simplify hashing and equality
    Em(f32),
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// The derived `PartialEq` is sufficient for this marker trait.
impl Eq for Spacing {}

impl Hash for Spacing {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // First, hash the enum variant to distinguish between Px and Em.
        discriminant(self).hash(state);
        match self {
            Spacing::Px(val) => val.hash(state),
            // For hashing floats, convert them to their raw bit representation.
            // This ensures that identical float values produce identical hashes.
            Spacing::Em(val) => val.to_bits().hash(state),
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing::Px(0)
    }
}

impl Default for FontHash {
    fn default() -> Self {
        Self::invalid()
    }
}

/// Style properties with vertical text support
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProperties {
    /// Font stack for fallback support (priority order)
    /// Can be either a list of FontSelectors (resolved via fontconfig)
    /// or a direct FontRef (bypasses fontconfig entirely).
    pub font_stack: FontStack,
    pub font_size_px: f32,
    pub color: ColorU,
    /// Background color for inline elements (e.g., `<span style="background-color: yellow">`)
    ///
    /// This is propagated from CSS through the style system and eventually used by
    /// the PDF renderer to draw filled rectangles behind text. The value is `None`
    /// for transparent backgrounds (the default).
    ///
    /// The propagation chain is:
    /// CSS -> `get_style_properties()` -> `StyleProperties` -> `ShapedGlyph` -> `PdfGlyphRun`
    ///
    /// See `PdfGlyphRun::background_color` for how this is used in PDF rendering.
    pub background_color: Option<ColorU>,
    /// Full background content layers (for gradients, images, etc.)
    /// This extends background_color to support CSS gradients on inline elements.
    pub background_content: Vec<StyleBackgroundContent>,
    /// Border information for inline elements
    pub border: Option<InlineBorderInfo>,
    pub letter_spacing: Spacing,
    pub word_spacing: Spacing,

    pub line_height: f32,
    pub text_decoration: TextDecoration,

    // Represents CSS font-feature-settings like `"liga"`, `"smcp=1"`.
    pub font_features: Vec<String>,

    // Variable fonts
    pub font_variations: Vec<(FourCc, f32)>,
    // Multiplier of the space width
    pub tab_size: f32,
    // text-transform
    pub text_transform: TextTransform,
    // Vertical text properties
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    // Tate-chu-yoko
    pub text_combine_upright: Option<TextCombineUpright>,

    // Variant handling
    pub font_variant_caps: FontVariantCaps,
    pub font_variant_numeric: FontVariantNumeric,
    pub font_variant_ligatures: FontVariantLigatures,
    pub font_variant_east_asian: FontVariantEastAsian,
}

impl Default for StyleProperties {
    fn default() -> Self {
        const FONT_SIZE: f32 = 16.0;
        const TAB_SIZE: f32 = 8.0;
        Self {
            font_stack: FontStack::default(),
            font_size_px: FONT_SIZE,
            color: ColorU::default(),
            background_color: None,
            background_content: Vec::new(),
            border: None,
            letter_spacing: Spacing::default(), // Px(0)
            word_spacing: Spacing::default(),   // Px(0)
            line_height: FONT_SIZE * 1.2,
            text_decoration: TextDecoration::default(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            tab_size: TAB_SIZE, // CSS default
            text_transform: TextTransform::default(),
            writing_mode: WritingMode::default(),
            text_orientation: TextOrientation::default(),
            text_combine_upright: None,
            font_variant_caps: FontVariantCaps::default(),
            font_variant_numeric: FontVariantNumeric::default(),
            font_variant_ligatures: FontVariantLigatures::default(),
            font_variant_east_asian: FontVariantEastAsian::default(),
        }
    }
}

impl Hash for StyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.color.hash(state);
        self.background_color.hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);

        // For f32 fields, round and cast to usize before hashing.
        (self.font_size_px.round() as usize).hash(state);
        (self.line_height.round() as usize).hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub enum TextCombineUpright {
    None,
    All,        // Combine all characters in horizontal layout
    Digits(u8), // Combine up to N digits
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphSource {
    /// Glyph generated from a character in the source text.
    Char,
    /// Glyph inserted dynamically by the layout engine (e.g., a hyphen).
    Hyphen,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharacterClass {
    Space,       // Regular spaces - highest justification priority
    Punctuation, // Can sometimes be adjusted
    Letter,      // Normal letters
    Ideograph,   // CJK characters - can be justified between
    Symbol,      // Symbols, emojis
    Combining,   // Combining marks - never justified
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphOrientation {
    Horizontal, // Keep horizontal (normal in horizontal text)
    Vertical,   // Rotate to vertical (normal in vertical text)
    Upright,    // Keep upright regardless of writing mode
    Mixed,      // Use script-specific default orientation
}

// Bidi and script detection
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BidiDirection {
    Ltr,
    Rtl,
}

impl BidiDirection {
    pub fn is_rtl(&self) -> bool {
        matches!(self, BidiDirection::Rtl)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantCaps {
    #[default]
    Normal,
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantNumeric {
    #[default]
    Normal,
    LiningNums,
    OldstyleNums,
    ProportionalNums,
    TabularNums,
    DiagonalFractions,
    StackedFractions,
    Ordinal,
    SlashedZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantLigatures {
    #[default]
    Normal,
    None,
    Common,
    NoCommon,
    Discretionary,
    NoDiscretionary,
    Historical,
    NoHistorical,
    Contextual,
    NoContextual,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantEastAsian {
    #[default]
    Normal,
    Jis78,
    Jis83,
    Jis90,
    Jis04,
    Simplified,
    Traditional,
    FullWidth,
    ProportionalWidth,
    Ruby,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BidiLevel(u8);

impl BidiLevel {
    pub fn new(level: u8) -> Self {
        Self(level)
    }
    pub fn is_rtl(&self) -> bool {
        self.0 % 2 == 1
    }
    pub fn level(&self) -> u8 {
        self.0
    }
}

// Add this new struct for style overrides
#[derive(Debug, Clone)]
pub struct StyleOverride {
    /// The specific character this override applies to.
    pub target: ContentIndex,
    /// The style properties to apply.
    /// Any `None` value means "inherit from the base style".
    pub style: PartialStyleProperties,
}

#[derive(Debug, Clone, Default)]
pub struct PartialStyleProperties {
    pub font_stack: Option<FontStack>,
    pub font_size_px: Option<f32>,
    pub color: Option<ColorU>,
    pub letter_spacing: Option<Spacing>,
    pub word_spacing: Option<Spacing>,
    pub line_height: Option<f32>,
    pub text_decoration: Option<TextDecoration>,
    pub font_features: Option<Vec<String>>,
    pub font_variations: Option<Vec<(FourCc, f32)>>,
    pub tab_size: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub writing_mode: Option<WritingMode>,
    pub text_orientation: Option<TextOrientation>,
    pub text_combine_upright: Option<Option<TextCombineUpright>>,
    pub font_variant_caps: Option<FontVariantCaps>,
    pub font_variant_numeric: Option<FontVariantNumeric>,
    pub font_variant_ligatures: Option<FontVariantLigatures>,
    pub font_variant_east_asian: Option<FontVariantEastAsian>,
}

impl Hash for PartialStyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.font_size_px.map(|f| f.to_bits()).hash(state);
        self.color.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);
        self.line_height.map(|f| f.to_bits()).hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);

        // Manual hashing for Vec<(FourCc, f32)>
        self.font_variations.as_ref().map(|v| {
            for (tag, val) in v {
                tag.hash(state);
                val.to_bits().hash(state);
            }
        });

        self.tab_size.map(|f| f.to_bits()).hash(state);
        self.text_transform.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.font_variant_caps.hash(state);
        self.font_variant_numeric.hash(state);
        self.font_variant_ligatures.hash(state);
        self.font_variant_east_asian.hash(state);
    }
}

impl PartialEq for PartialStyleProperties {
    fn eq(&self, other: &Self) -> bool {
        self.font_stack == other.font_stack &&
        self.font_size_px.map(|f| f.to_bits()) == other.font_size_px.map(|f| f.to_bits()) &&
        self.color == other.color &&
        self.letter_spacing == other.letter_spacing &&
        self.word_spacing == other.word_spacing &&
        self.line_height.map(|f| f.to_bits()) == other.line_height.map(|f| f.to_bits()) &&
        self.text_decoration == other.text_decoration &&
        self.font_features == other.font_features &&
        self.font_variations == other.font_variations && // Vec<(FourCc, f32)> is PartialEq
        self.tab_size.map(|f| f.to_bits()) == other.tab_size.map(|f| f.to_bits()) &&
        self.text_transform == other.text_transform &&
        self.writing_mode == other.writing_mode &&
        self.text_orientation == other.text_orientation &&
        self.text_combine_upright == other.text_combine_upright &&
        self.font_variant_caps == other.font_variant_caps &&
        self.font_variant_numeric == other.font_variant_numeric &&
        self.font_variant_ligatures == other.font_variant_ligatures &&
        self.font_variant_east_asian == other.font_variant_east_asian
    }
}

impl Eq for PartialStyleProperties {}

impl StyleProperties {
    fn apply_override(&self, partial: &PartialStyleProperties) -> Self {
        let mut new_style = self.clone();
        if let Some(val) = &partial.font_stack {
            new_style.font_stack = val.clone();
        }
        if let Some(val) = partial.font_size_px {
            new_style.font_size_px = val;
        }
        if let Some(val) = &partial.color {
            new_style.color = val.clone();
        }
        if let Some(val) = partial.letter_spacing {
            new_style.letter_spacing = val;
        }
        if let Some(val) = partial.word_spacing {
            new_style.word_spacing = val;
        }
        if let Some(val) = partial.line_height {
            new_style.line_height = val;
        }
        if let Some(val) = &partial.text_decoration {
            new_style.text_decoration = val.clone();
        }
        if let Some(val) = &partial.font_features {
            new_style.font_features = val.clone();
        }
        if let Some(val) = &partial.font_variations {
            new_style.font_variations = val.clone();
        }
        if let Some(val) = partial.tab_size {
            new_style.tab_size = val;
        }
        if let Some(val) = partial.text_transform {
            new_style.text_transform = val;
        }
        if let Some(val) = partial.writing_mode {
            new_style.writing_mode = val;
        }
        if let Some(val) = partial.text_orientation {
            new_style.text_orientation = val;
        }
        if let Some(val) = &partial.text_combine_upright {
            new_style.text_combine_upright = val.clone();
        }
        if let Some(val) = partial.font_variant_caps {
            new_style.font_variant_caps = val;
        }
        if let Some(val) = partial.font_variant_numeric {
            new_style.font_variant_numeric = val;
        }
        if let Some(val) = partial.font_variant_ligatures {
            new_style.font_variant_ligatures = val;
        }
        if let Some(val) = partial.font_variant_east_asian {
            new_style.font_variant_east_asian = val;
        }
        new_style
    }
}

/// The kind of a glyph, used to distinguish characters from layout-inserted items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphKind {
    /// A standard glyph representing one or more characters from the source text.
    Character,
    /// A hyphen glyph inserted by the line breaking algorithm.
    Hyphen,
    /// A `.notdef` glyph, indicating a character that could not be found in any font.
    NotDef,
    /// A Kashida justification glyph, inserted to stretch Arabic text.
    Kashida {
        /// The target width of the kashida.
        width: f32,
    },
}

// --- Stage 1: Logical Representation ---

#[derive(Debug, Clone)]
pub enum LogicalItem {
    Text {
        /// A stable ID pointing back to the original source character.
        source: ContentIndex,
        /// The text of this specific logical item (often a single grapheme cluster).
        text: String,
        style: Arc<StyleProperties>,
        /// If this text is a list marker: whether it should be positioned outside
        /// (in the padding gutter) or inside (inline with content).
        /// None for non-marker content.
        marker_position_outside: Option<bool>,
        /// The DOM NodeId of the Text node this item originated from.
        /// None for generated content (list markers, ::before/::after, etc.)
        source_node_id: Option<NodeId>,
    },
    /// Tate-chu-yoko: Run of text to be laid out horizontally within a vertical context.
    CombinedText {
        source: ContentIndex,
        text: String,
        style: Arc<StyleProperties>,
    },
    Ruby {
        source: ContentIndex,
        // For the stub, we simplify to strings. A full implementation
        // would need to handle Vec<LogicalItem> for both.
        base_text: String,
        ruby_text: String,
        style: Arc<StyleProperties>,
    },
    Object {
        /// A stable ID pointing back to the original source object.
        source: ContentIndex,
        /// The original non-text object.
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        style: Arc<StyleProperties>,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl Hash for LogicalItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            LogicalItem::Text {
                source,
                text,
                style,
                marker_position_outside,
                source_node_id,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state); // Hash the content, not the Arc pointer
                marker_position_outside.hash(state);
                source_node_id.hash(state);
            }
            LogicalItem::CombinedText {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                source.hash(state);
                base_text.hash(state);
                ruby_text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Object { source, content } => {
                source.hash(state);
                content.hash(state);
            }
            LogicalItem::Tab { source, style } => {
                source.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Break { source, break_info } => {
                source.hash(state);
                break_info.hash(state);
            }
        }
    }
}

// --- Stage 2: Visual Representation ---

#[derive(Debug, Clone)]
pub struct VisualItem {
    /// A reference to the logical item this visual item originated from.
    /// A single LogicalItem can be split into multiple VisualItems.
    pub logical_source: LogicalItem,
    /// The Bidi embedding level for this item.
    pub bidi_level: BidiLevel,
    /// The script detected for this run, crucial for shaping.
    pub script: Script,
    /// The text content for this specific visual run.
    pub text: String,
}

// --- Stage 3: Shaped Representation ---

#[derive(Debug, Clone)]
pub enum ShapedItem {
    Cluster(ShapedCluster),
    /// A block of combined text (tate-chu-yoko) that is laid out
    // as a single unbreakable object.
    CombinedBlock {
        source: ContentIndex,
        /// The glyphs to be rendered horizontally within the vertical line.
        glyphs: Vec<ShapedGlyph>,
        bounds: Rect,
        baseline_offset: f32,
    },
    Object {
        source: ContentIndex,
        bounds: Rect,
        baseline_offset: f32,
        // Store original object for rendering
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        bounds: Rect,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl ShapedItem {
    pub fn as_cluster(&self) -> Option<&ShapedCluster> {
        match self {
            ShapedItem::Cluster(c) => Some(c),
            _ => None,
        }
    }
    /// Returns the bounding box of the item, relative to its own origin.
    ///
    /// The origin of the returned `Rect` is `(0,0)`, representing the top-left corner
    /// of the item's layout space before final positioning. The size represents the
    /// item's total advance (width in horizontal mode) and its line height (ascent + descent).
    pub fn bounds(&self) -> Rect {
        match self {
            ShapedItem::Cluster(cluster) => {
                // The width of a text cluster is its total advance.
                let width = cluster.advance;

                // The height is the sum of its ascent and descent, which defines its line box.
                // We use the existing helper function which correctly calculates this from font
                // metrics.
                let (ascent, descent) = get_item_vertical_metrics(self);
                let height = ascent + descent;

                Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                }
            }
            // For atomic inline items like objects, combined blocks, and tabs,
            // their bounds have already been calculated during the shaping or measurement phase.
            ShapedItem::CombinedBlock { bounds, .. } => *bounds,
            ShapedItem::Object { bounds, .. } => *bounds,
            ShapedItem::Tab { bounds, .. } => *bounds,

            // Breaks are control characters and have no visual geometry.
            ShapedItem::Break { .. } => Rect::default(), // A zero-sized rectangle.
        }
    }
}

/// A group of glyphs that corresponds to one or more source characters (a cluster).
#[derive(Debug, Clone)]
pub struct ShapedCluster {
    /// The original text that this cluster was shaped from.
    /// This is crucial for correct hyphenation.
    pub text: String,
    /// The ID of the grapheme cluster this glyph cluster represents.
    pub source_cluster_id: GraphemeClusterId,
    /// The source `ContentIndex` for mapping back to logical items.
    pub source_content_index: ContentIndex,
    /// The DOM NodeId of the Text node this cluster originated from.
    /// None for generated content (list markers, ::before/::after, etc.)
    pub source_node_id: Option<NodeId>,
    /// The glyphs that make up this cluster.
    pub glyphs: Vec<ShapedGlyph>,
    /// The total advance width (horizontal) or height (vertical) of the cluster.
    pub advance: f32,
    /// The direction of this cluster, inherited from its `VisualItem`.
    pub direction: BidiDirection,
    /// Font style of this cluster
    pub style: Arc<StyleProperties>,
    /// If this cluster is a list marker: whether it should be positioned outside
    /// (in the padding gutter) or inside (inline with content).
    /// None for non-marker content.
    pub marker_position_outside: Option<bool>,
}

/// A single, shaped glyph with its essential metrics.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// The kind of glyph this is (character, hyphen, etc.).
    pub kind: GlyphKind,
    /// Glyph ID inside of the font
    pub glyph_id: u16,
    /// The byte offset of this glyph's source character(s) within its cluster text.
    pub cluster_offset: u32,
    /// The horizontal advance for this glyph (for horizontal text) - this is the BASE advance
    /// from the font metrics, WITHOUT kerning applied
    pub advance: f32,
    /// The kerning adjustment for this glyph (positive = more space, negative = less space)
    /// This is separate from advance so we can position glyphs absolutely
    pub kerning: f32,
    /// The horizontal offset/bearing for this glyph
    pub offset: Point,
    /// The vertical advance for this glyph (for vertical text).
    pub vertical_advance: f32,
    /// The vertical offset/bearing for this glyph.
    pub vertical_offset: Point,
    pub script: Script,
    pub style: Arc<StyleProperties>,
    /// Hash of the font - use LoadedFonts to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
}

impl ShapedGlyph {
    pub fn into_glyph_instance<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        let position = if writing_mode.is_advance_horizontal() {
            LogicalPosition {
                x: self.offset.x,
                y: self.offset.y,
            }
        } else {
            LogicalPosition {
                x: self.vertical_offset.x,
                y: self.vertical_offset.y,
            }
        };

        GlyphInstance {
            index: self.glyph_id as u32,
            point: position,
            size,
        }
    }

    /// Convert this ShapedGlyph into a GlyphInstance with an absolute position.
    /// This is used for display list generation where glyphs need their final page coordinates.
    pub fn into_glyph_instance_at<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        absolute_position: LogicalPosition,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        GlyphInstance {
            index: self.glyph_id as u32,
            point: absolute_position,
            size,
        }
    }

    /// Convert this ShapedGlyph into a GlyphInstance with an absolute position.
    /// This version doesn't require fonts - it uses a default size.
    /// Use this when you don't need precise glyph bounds (e.g., display list generation).
    pub fn into_glyph_instance_at_simple(
        &self,
        _writing_mode: WritingMode,
        absolute_position: LogicalPosition,
    ) -> GlyphInstance {
        // Use font metrics to estimate size, or default to zero
        // The actual rendering will use the font directly
        GlyphInstance {
            index: self.glyph_id as u32,
            point: absolute_position,
            size: LogicalSize::default(),
        }
    }
}

// --- Stage 4: Positioned Representation (Final Layout) ---

#[derive(Debug, Clone)]
pub struct PositionedItem {
    pub item: ShapedItem,
    pub position: Point,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,
    /// Information about content that did not fit.
    pub overflow: OverflowInfo,
}

impl UnifiedLayout {
    /// Calculate the bounding box of all positioned items.
    /// This is computed on-demand rather than cached.
    pub fn bounds(&self) -> Rect {
        if self.items.is_empty() {
            return Rect::default();
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for item in &self.items {
            let item_x = item.position.x;
            let item_y = item.position.y;

            // Get item dimensions
            let item_bounds = item.item.bounds();
            let item_width = item_bounds.width;
            let item_height = item_bounds.height;

            min_x = min_x.min(item_x);
            min_y = min_y.min(item_y);
            max_x = max_x.max(item_x + item_width);
            max_y = max_y.max(item_y + item_height);
        }

        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn last_baseline(&self) -> Option<f32> {
        self.items
            .iter()
            .rev()
            .find_map(|item| get_baseline_for_item(&item.item))
    }

    /// Takes a point relative to the layout's origin and returns the closest
    /// logical cursor position.
    ///
    /// This is the unified hit-testing implementation. The old `hit_test_to_cursor`
    /// method is deprecated in favor of this one.
    pub fn hittest_cursor(&self, point: LogicalPosition) -> Option<TextCursor> {
        if self.items.is_empty() {
            return None;
        }

        // Find the closest cluster vertically and horizontally
        let mut closest_item_idx = 0;
        let mut closest_distance = f32::MAX;

        for (idx, item) in self.items.iter().enumerate() {
            // Only consider cluster items for cursor placement
            if !matches!(item.item, ShapedItem::Cluster(_)) {
                continue;
            }

            let item_bounds = item.item.bounds();
            let item_center_y = item.position.y + item_bounds.height / 2.0;

            // Distance from click position to item center
            let vertical_distance = (point.y - item_center_y).abs();

            // For horizontal distance, check if we're within the cluster bounds
            let horizontal_distance = if point.x < item.position.x {
                item.position.x - point.x
            } else if point.x > item.position.x + item_bounds.width {
                point.x - (item.position.x + item_bounds.width)
            } else {
                0.0 // Inside the cluster horizontally
            };

            // Combined distance (prioritize vertical proximity)
            let distance = vertical_distance * 2.0 + horizontal_distance;

            if distance < closest_distance {
                closest_distance = distance;
                closest_item_idx = idx;
            }
        }

        // Get the closest cluster
        let closest_item = &self.items[closest_item_idx];
        let cluster = match &closest_item.item {
            ShapedItem::Cluster(c) => c,
            // Objects are treated as a single cluster for selection
            ShapedItem::Object { source, .. } | ShapedItem::CombinedBlock { source, .. } => {
                return Some(TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: source.run_index,
                        start_byte_in_run: source.item_index,
                    },
                    affinity: if point.x
                        < closest_item.position.x + (closest_item.item.bounds().width / 2.0)
                    {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                });
            }
            _ => return None,
        };

        // Determine affinity based on which half of the cluster was clicked
        let cluster_mid_x = closest_item.position.x + cluster.advance / 2.0;
        let affinity = if point.x < cluster_mid_x {
            CursorAffinity::Leading
        } else {
            CursorAffinity::Trailing
        };

        Some(TextCursor {
            cluster_id: cluster.source_cluster_id,
            affinity,
        })
    }

    /// Given a logical selection range, returns a vector of visual rectangles
    /// that cover the selected text, in the layout's coordinate space.
    pub fn get_selection_rects(&self, range: &SelectionRange) -> Vec<LogicalRect> {
        // 1. Build a map from the logical cluster ID to the visual PositionedItem for fast lookups.
        let mut cluster_map: HashMap<GraphemeClusterId, &PositionedItem> = HashMap::new();
        for item in &self.items {
            if let Some(cluster) = item.item.as_cluster() {
                cluster_map.insert(cluster.source_cluster_id, item);
            }
        }

        // 2. Normalize the range to ensure start always logically precedes end.
        let (start_cursor, end_cursor) = if range.start.cluster_id > range.end.cluster_id
            || (range.start.cluster_id == range.end.cluster_id
                && range.start.affinity > range.end.affinity)
        {
            (range.end, range.start)
        } else {
            (range.start, range.end)
        };

        // 3. Find the positioned items corresponding to the start and end of the selection.
        let Some(start_item) = cluster_map.get(&start_cursor.cluster_id) else {
            return Vec::new();
        };
        let Some(end_item) = cluster_map.get(&end_cursor.cluster_id) else {
            return Vec::new();
        };

        let mut rects = Vec::new();

        // Helper to get the absolute visual X coordinate of a cursor.
        let get_cursor_x = |item: &PositionedItem, affinity: CursorAffinity| -> f32 {
            match affinity {
                CursorAffinity::Leading => item.position.x,
                CursorAffinity::Trailing => item.position.x + get_item_measure(&item.item, false),
            }
        };

        // Helper to get the visual bounding box of all content on a specific line index.
        let get_line_bounds = |line_index: usize| -> Option<LogicalRect> {
            let items_on_line = self.items.iter().filter(|i| i.line_index == line_index);

            let mut min_x: Option<f32> = None;
            let mut max_x: Option<f32> = None;
            let mut min_y: Option<f32> = None;
            let mut max_y: Option<f32> = None;

            for item in items_on_line {
                // Skip items that don't take up space (like hard breaks)
                let item_bounds = item.item.bounds();
                if item_bounds.width <= 0.0 && item_bounds.height <= 0.0 {
                    continue;
                }

                let item_x_end = item.position.x + item_bounds.width;
                let item_y_end = item.position.y + item_bounds.height;

                min_x = Some(min_x.map_or(item.position.x, |mx| mx.min(item.position.x)));
                max_x = Some(max_x.map_or(item_x_end, |mx| mx.max(item_x_end)));
                min_y = Some(min_y.map_or(item.position.y, |my| my.min(item.position.y)));
                max_y = Some(max_y.map_or(item_y_end, |my| my.max(item_y_end)));
            }

            if let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) =
                (min_x, max_x, min_y, max_y)
            {
                Some(LogicalRect {
                    origin: LogicalPosition { x: min_x, y: min_y },
                    size: LogicalSize {
                        width: max_x - min_x,
                        height: max_y - min_y,
                    },
                })
            } else {
                None
            }
        };

        // 4. Handle single-line selection.
        if start_item.line_index == end_item.line_index {
            if let Some(line_bounds) = get_line_bounds(start_item.line_index) {
                let start_x = get_cursor_x(start_item, start_cursor.affinity);
                let end_x = get_cursor_x(end_item, end_cursor.affinity);

                // Use min/max and abs to correctly handle selections made from right-to-left.
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: start_x.min(end_x),
                        y: line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: (end_x - start_x).abs(),
                        height: line_bounds.size.height,
                    },
                });
            }
        }
        // 5. Handle multi-line selection.
        else {
            // Rectangle for the start line (from cursor to end of line).
            if let Some(start_line_bounds) = get_line_bounds(start_item.line_index) {
                let start_x = get_cursor_x(start_item, start_cursor.affinity);
                let line_end_x = start_line_bounds.origin.x + start_line_bounds.size.width;
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: start_x,
                        y: start_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: line_end_x - start_x,
                        height: start_line_bounds.size.height,
                    },
                });
            }

            // Rectangles for all full lines in between.
            for line_idx in (start_item.line_index + 1)..end_item.line_index {
                if let Some(line_bounds) = get_line_bounds(line_idx) {
                    rects.push(line_bounds);
                }
            }

            // Rectangle for the end line (from start of line to cursor).
            if let Some(end_line_bounds) = get_line_bounds(end_item.line_index) {
                let line_start_x = end_line_bounds.origin.x;
                let end_x = get_cursor_x(end_item, end_cursor.affinity);
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: line_start_x,
                        y: end_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: end_x - line_start_x,
                        height: end_line_bounds.size.height,
                    },
                });
            }
        }

        rects
    }

    /// Calculates the visual rectangle for a cursor at a given logical position.
    pub fn get_cursor_rect(&self, cursor: &TextCursor) -> Option<LogicalRect> {
        // Find the item and glyph corresponding to the cursor's cluster ID.
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                if cluster.source_cluster_id == cursor.cluster_id {
                    // This is the correct cluster. Now find the position.
                    let line_height = item.item.bounds().height;
                    let cursor_x = match cursor.affinity {
                        CursorAffinity::Leading => item.position.x,
                        CursorAffinity::Trailing => item.position.x + cluster.advance,
                    };
                    return Some(LogicalRect {
                        origin: LogicalPosition {
                            x: cursor_x,
                            y: item.position.y,
                        },
                        size: LogicalSize {
                            width: 1.0,
                            height: line_height,
                        }, // 1px wide cursor
                    });
                }
            }
        }
        None
    }

    /// Get a cursor at the first cluster (leading edge) in the layout.
    pub fn get_first_cluster_cursor(&self) -> Option<TextCursor> {
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                });
            }
        }
        None
    }

    /// Get a cursor at the last cluster (trailing edge) in the layout.
    pub fn get_last_cluster_cursor(&self) -> Option<TextCursor> {
        for item in self.items.iter().rev() {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                });
            }
        }
        None
    }

    /// Moves a cursor one visual unit to the left, handling line wrapping and Bidi text.
    pub fn move_cursor_left(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        // Find current item
        let current_item_pos = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        });

        let Some(current_pos) = current_item_pos else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        // If we're at trailing edge, move to leading edge of same cluster
        if cursor.affinity == CursorAffinity::Trailing {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: moving from trailing to leading edge of byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return TextCursor {
                cluster_id: cursor.cluster_id,
                affinity: CursorAffinity::Leading,
            };
        }

        // We're at leading edge, move to previous cluster's trailing edge
        // Search backwards for a cluster on the same line, or any cluster if at line start
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: at leading edge, current line {}",
                current_line
            ));
        }

        // First, try to find previous item on same line
        for i in (0..current_pos).rev() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == current_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_left: found previous cluster on same line, byte \
                             {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Trailing,
                    };
                }
            }
        }

        // If no previous item on same line, try to move to end of previous line
        if current_line > 0 {
            let prev_line = current_line - 1;
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: trying previous line {}",
                    prev_line
                ));
            }
            for i in (0..current_pos).rev() {
                if let Some(cluster) = self.items[i].item.as_cluster() {
                    if self.items[i].line_index == prev_line {
                        if let Some(d) = debug {
                            d.push(format!(
                                "[Cursor] move_cursor_left: found cluster on previous line, byte \
                                 {}",
                                cluster.source_cluster_id.start_byte_in_run
                            ));
                        }
                        return TextCursor {
                            cluster_id: cluster.source_cluster_id,
                            affinity: CursorAffinity::Trailing,
                        };
                    }
                }
            }
        }

        // At start of text, can't move further
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: at start of text, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor one visual unit to the right.
    pub fn move_cursor_right(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        // Find current item
        let current_item_pos = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        });

        let Some(current_pos) = current_item_pos else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_right: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        // If we're at leading edge, move to trailing edge of same cluster
        if cursor.affinity == CursorAffinity::Leading {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_right: moving from leading to trailing edge of byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return TextCursor {
                cluster_id: cursor.cluster_id,
                affinity: CursorAffinity::Trailing,
            };
        }

        // We're at trailing edge, move to next cluster's leading edge
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: at trailing edge, current line {}",
                current_line
            ));
        }

        // First, try to find next item on same line
        for i in (current_pos + 1)..self.items.len() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == current_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_right: found next cluster on same line, byte {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
        }

        // If no next item on same line, try to move to start of next line
        let next_line = current_line + 1;
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: trying next line {}",
                next_line
            ));
        }
        for i in (current_pos + 1)..self.items.len() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == next_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_right: found cluster on next line, byte {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
        }

        // At end of text, can't move further
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: at end of text, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor up one line, attempting to preserve the horizontal column.
    pub fn move_cursor_up(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let target_line_idx = current_item.line_index.saturating_sub(1);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: already at top line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        // Find the Y coordinate of the middle of the target line
        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map(|i| i.position.y + (i.item.bounds().height / 2.0))
            .unwrap_or(current_item.position.y);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: target line {}, hittesting at ({}, {})",
                target_line_idx, current_x, target_y
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: result byte {} (affinity {:?})",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor down one line, attempting to preserve the horizontal column.
    pub fn move_cursor_down(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let max_line = self.items.iter().map(|i| i.line_index).max().unwrap_or(0);
        let target_line_idx = (current_item.line_index + 1).min(max_line);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: already at bottom line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map(|i| i.position.y + (i.item.bounds().height / 2.0))
            .unwrap_or(current_item.position.y);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: hit testing at ({}, {})",
                current_x, target_y
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: result byte {}, affinity {:?}",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor to the visual start of its current line.
    pub fn move_cursor_to_line_start(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_start: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let first_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .min_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = first_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_start: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: no first item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor to the visual end of its current line.
    pub fn move_cursor_to_line_end(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_end: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let last_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .max_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = last_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_end: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: no last item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }
}

fn get_baseline_for_item(item: &ShapedItem) -> Option<f32> {
    match item {
        ShapedItem::CombinedBlock {
            baseline_offset, ..
        } => Some(*baseline_offset),
        ShapedItem::Object {
            baseline_offset, ..
        } => Some(*baseline_offset),
        // We have to get the clusters font from the last glyph
        ShapedItem::Cluster(ref cluster) => {
            if let Some(last_glyph) = cluster.glyphs.last() {
                Some(
                    last_glyph
                        .font_metrics
                        .baseline_scaled(last_glyph.style.font_size_px),
                )
            } else {
                None
            }
        }
        ShapedItem::Break { source, break_info } => {
            // Breaks do not contribute to baseline
            None
        }
        ShapedItem::Tab { source, bounds } => {
            // Tabs do not contribute to baseline
            None
        }
    }
}

/// Stores information about content that exceeded the available layout space.
#[derive(Debug, Clone, Default)]
pub struct OverflowInfo {
    /// The items that did not fit within the constraints.
    pub overflow_items: Vec<ShapedItem>,
    /// The total bounds of all content, including overflowing items.
    /// This is useful for `OverflowBehavior::Visible` or `Scroll`.
    pub unclipped_bounds: Rect,
}

impl OverflowInfo {
    pub fn has_overflow(&self) -> bool {
        !self.overflow_items.is_empty()
    }
}

/// Intermediate structure carrying information from the line breaker to the positioner.
#[derive(Debug, Clone)]
pub struct UnifiedLine {
    pub items: Vec<ShapedItem>,
    /// The y-position (for horizontal) or x-position (for vertical) of the line's baseline.
    pub cross_axis_position: f32,
    /// The geometric segments this line must fit into.
    pub constraints: LineConstraints,
    pub is_last: bool,
}

// --- Caching Infrastructure ---

pub type CacheId = u64;

/// Defines a single area for layout, with its own shape and properties.
#[derive(Debug, Clone)]
pub struct LayoutFragment {
    /// A unique identifier for this fragment (e.g., "main-content", "sidebar").
    pub id: String,
    /// The geometric and style constraints for this specific fragment.
    pub constraints: UnifiedConstraints,
}

/// Represents the final layout distributed across multiple fragments.
#[derive(Debug, Clone)]
pub struct FlowLayout {
    /// A map from a fragment's unique ID to the layout it contains.
    pub fragment_layouts: HashMap<String, Arc<UnifiedLayout>>,
    /// Any items that did not fit into the last fragment in the flow chain.
    /// This is useful for pagination or determining if more layout space is needed.
    pub remaining_items: Vec<ShapedItem>,
}

pub struct LayoutCache {
    // Stage 1 Cache: InlineContent -> LogicalItems
    logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>,
    // Stage 2 Cache: LogicalItems -> VisualItems
    visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>,
    // Stage 3 Cache: VisualItems -> ShapedItems (now strongly typed)
    shaped_items: HashMap<CacheId, Arc<Vec<ShapedItem>>>,
    // Stage 4 Cache: ShapedItems + Constraints -> Final Layout (now strongly typed)
    layouts: HashMap<CacheId, Arc<UnifiedLayout>>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            logical_items: HashMap::new(),
            visual_items: HashMap::new(),
            shaped_items: HashMap::new(),
            layouts: HashMap::new(),
        }
    }

    /// Get a layout from the cache by its ID
    pub fn get_layout(&self, cache_id: &CacheId) -> Option<&Arc<UnifiedLayout>> {
        self.layouts.get(cache_id)
    }

    /// Get all layout cache IDs (for iteration/debugging)
    pub fn get_all_layout_ids(&self) -> Vec<CacheId> {
        self.layouts.keys().copied().collect()
    }
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Key for caching the conversion from `InlineContent` to `LogicalItem`s.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LogicalItemsKey<'a> {
    pub inline_content_hash: u64, // Pre-hash the content for efficiency
    pub default_font_size: u32,   // Affects space widths
    // Add other relevant properties from constraints if they affect this stage
    pub _marker: std::marker::PhantomData<&'a ()>,
}

/// Key for caching the Bidi reordering stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct VisualItemsKey {
    pub logical_items_id: CacheId,
    pub base_direction: BidiDirection,
}

/// Key for caching the shaping stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ShapedItemsKey {
    pub visual_items_id: CacheId,
    pub style_hash: u64, // Represents a hash of all font/style properties
}

impl ShapedItemsKey {
    pub fn new(visual_items_id: CacheId, visual_items: &[VisualItem]) -> Self {
        let style_hash = {
            let mut hasher = DefaultHasher::new();
            for item in visual_items.iter() {
                // Hash the style from the logical source, as this is what determines the font.
                match &item.logical_source {
                    LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                        style.as_ref().hash(&mut hasher);
                    }
                    _ => {}
                }
            }
            hasher.finish()
        };

        Self {
            visual_items_id,
            style_hash,
        }
    }
}

/// Key for the final layout stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LayoutKey {
    pub shaped_items_id: CacheId,
    pub constraints: UnifiedConstraints,
}

/// Helper to create a `CacheId` from any `Hash`able type.
fn calculate_id<T: Hash>(item: &T) -> CacheId {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}

// --- Main Layout Pipeline Implementation ---

impl LayoutCache {
    /// New top-level entry point for flowing layout across multiple regions.
    ///
    /// This function orchestrates the entire layout pipeline, but instead of fitting
    /// content into a single set of constraints, it flows the content through an
    /// ordered sequence of `LayoutFragment`s.
    ///
    /// # CSS Inline Layout Module Level 3: Pipeline Implementation
    ///
    /// This implements the inline formatting context with 5 stages:
    ///
    /// ## Stage 1: Logical Analysis (InlineContent -> LogicalItem)
    /// \u2705 IMPLEMENTED: Parses raw content into logical units
    /// - Handles text runs, inline-blocks, replaced elements
    /// - Applies style overrides at character level
    /// - Implements \u00a7 2.2: Content size contribution calculation
    ///
    /// ## Stage 2: BiDi Reordering (LogicalItem -> VisualItem)
    /// \u2705 IMPLEMENTED: Uses CSS 'direction' property per CSS Writing Modes
    /// - Reorders items for right-to-left text (Arabic, Hebrew)
    /// - Respects containing block direction (not auto-detection)
    /// - Conforms to Unicode BiDi Algorithm (UAX #9)
    ///
    /// ## Stage 3: Shaping (VisualItem -> ShapedItem)
    /// \u2705 IMPLEMENTED: Converts text to glyphs
    /// - Uses HarfBuzz for OpenType shaping
    /// - Handles ligatures, kerning, contextual forms
    /// - Caches shaped results for performance
    ///
    /// ## Stage 4: Text Orientation Transformations
    /// \u26a0\ufe0f PARTIAL: Applies text-orientation for vertical text
    /// - Uses constraints from *first* fragment only
    /// - \u274c TODO: Should re-orient if fragments have different writing modes
    ///
    /// ## Stage 5: Flow Loop (ShapedItem -> PositionedItem)
    /// \u2705 IMPLEMENTED: Breaks lines and positions content
    /// - Calls perform_fragment_layout for each fragment
    /// - Uses BreakCursor to flow content across fragments
    /// - Implements \u00a7 5: Line breaking and hyphenation
    ///
    /// # Missing Features from CSS Inline-3:
    /// - \u00a7 3.3: initial-letter (drop caps)
    /// - \u00a7 4: vertical-align (only baseline supported)
    /// - \u00a7 6: text-box-trim (leading trim)
    /// - \u00a7 7: inline-sizing (aspect-ratio for inline-blocks)
    ///
    /// # Arguments
    /// * `content` - The raw `InlineContent` to be laid out.
    /// * `style_overrides` - Character-level style changes.
    /// * `flow_chain` - An ordered slice of `LayoutFragment` defining the regions (e.g., columns,
    ///   pages) that the content should flow through.
    /// * `font_chain_cache` - Pre-resolved font chains (from FontManager.font_chain_cache)
    /// * `fc_cache` - The fontconfig cache for font lookups
    /// * `loaded_fonts` - Pre-loaded fonts, keyed by FontId
    ///
    /// # Returns
    /// A `FlowLayout` struct containing the positioned items for each fragment that
    /// was filled, and any content that did not fit in the final fragment.
    pub fn layout_flow<T: ParsedFontTrait>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        flow_chain: &[LayoutFragment],
        font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        fc_cache: &FcFontCache,
        loaded_fonts: &LoadedFonts<T>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<FlowLayout, LayoutError> {
        // --- Stages 1-3: Preparation ---
        // These stages are independent of the final geometry. We perform them once
        // on the entire content block before flowing. Caching is used at each stage.

        // Stage 1: Logical Analysis (InlineContent -> LogicalItem)
        let logical_items_id = calculate_id(&content);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| {
                Arc::new(create_logical_items(
                    content,
                    style_overrides,
                    debug_messages,
                ))
            })
            .clone();

        // Get the first fragment's constraints to extract the CSS direction property.
        // This is used for BiDi reordering in Stage 2.
        let default_constraints = UnifiedConstraints::default();
        let first_constraints = flow_chain
            .first()
            .map(|f| &f.constraints)
            .unwrap_or(&default_constraints);

        // Stage 2: Bidi Reordering (LogicalItem -> VisualItem)
        // Use CSS direction property from constraints instead of auto-detecting from text content.
        // This fixes issues with mixed-direction text (e.g., "Arabic - Latin") where auto-detection
        // would treat the entire paragraph as RTL if the first strong character is Arabic.
        // Per HTML/CSS spec, base direction should come from the 'direction' CSS property,
        // defaulting to LTR if not specified.
        let base_direction = first_constraints.direction.unwrap_or(BidiDirection::Ltr);
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(
                    reorder_logical_items(&logical_items, base_direction, debug_messages).unwrap(),
                )
            })
            .clone();

        // Stage 3: Shaping (VisualItem -> ShapedItem)
        let shaped_key = ShapedItemsKey::new(visual_items_id, &visual_items);
        let shaped_items_id = calculate_id(&shaped_key);
        let shaped_items = match self.shaped_items.get(&shaped_items_id) {
            Some(cached) => cached.clone(),
            None => {
                let items = Arc::new(shape_visual_items(
                    &visual_items,
                    font_chain_cache,
                    fc_cache,
                    loaded_fonts,
                    debug_messages,
                )?);
                self.shaped_items.insert(shaped_items_id, items.clone());
                items
            }
        };

        // --- Stage 4: Apply Vertical Text Transformations ---

        // Note: first_constraints was already extracted above for BiDi reordering (Stage 2).
        // This orients all text based on the constraints of the *first* fragment.
        // A more advanced system could defer orientation until inside the loop if
        // fragments can have different writing modes.
        let oriented_items = apply_text_orientation(shaped_items, first_constraints)?;

        // --- Stage 5: The Flow Loop ---

        let mut fragment_layouts = HashMap::new();
        // The cursor now manages the stream of items for the entire flow.
        let mut cursor = BreakCursor::new(&oriented_items);

        for fragment in flow_chain {
            // Perform layout for this single fragment, consuming items from the cursor.
            let fragment_layout = perform_fragment_layout(
                &mut cursor,
                &logical_items,
                &fragment.constraints,
                debug_messages,
                loaded_fonts,
            )?;

            fragment_layouts.insert(fragment.id.clone(), Arc::new(fragment_layout));
            if cursor.is_done() {
                break; // All content has been laid out.
            }
        }

        Ok(FlowLayout {
            fragment_layouts,
            remaining_items: cursor.drain_remaining(),
        })
    }
}

// --- Stage 1 Implementation ---
pub fn create_logical_items(
    content: &[InlineContent],
    style_overrides: &[StyleOverride],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<LogicalItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering create_logical_items (Refactored) ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input content length: {}",
            content.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input overrides length: {}",
            style_overrides.len()
        )));
    }

    let mut items = Vec::new();
    let mut style_cache: HashMap<u64, Arc<StyleProperties>> = HashMap::new();

    // 1. Organize overrides for fast lookup per run.
    let mut run_overrides: HashMap<u32, HashMap<u32, &PartialStyleProperties>> = HashMap::new();
    for override_item in style_overrides {
        run_overrides
            .entry(override_item.target.run_index)
            .or_default()
            .insert(override_item.target.item_index, &override_item.style);
    }

    for (run_idx, inline_item) in content.iter().enumerate() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Processing content run #{}",
                run_idx
            )));
        }

        // Extract marker information if this is a marker
        let marker_position_outside = match inline_item {
            InlineContent::Marker {
                position_outside, ..
            } => Some(*position_outside),
            _ => None,
        };

        match inline_item {
            InlineContent::Text(run) | InlineContent::Marker { run, .. } => {
                let text = &run.text;
                if text.is_empty() {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(
                            "  Run is empty, skipping.".to_string(),
                        ));
                    }
                    continue;
                }
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!("  Run text: '{}'", text)));
                }

                let current_run_overrides = run_overrides.get(&(run_idx as u32));
                let mut boundaries = BTreeSet::new();
                boundaries.insert(0);
                boundaries.insert(text.len());

                // --- Stateful Boundary Generation ---
                let mut scan_cursor = 0;
                while scan_cursor < text.len() {
                    let style_at_cursor = if let Some(partial) =
                        current_run_overrides.and_then(|o| o.get(&(scan_cursor as u32)))
                    {
                        // Create a temporary, full style to check its properties
                        run.style.apply_override(partial)
                    } else {
                        (*run.style).clone()
                    };

                    let current_char = text[scan_cursor..].chars().next().unwrap();

                    // Rule 1: Multi-character features take precedence.
                    if let Some(TextCombineUpright::Digits(max_digits)) =
                        style_at_cursor.text_combine_upright
                    {
                        if max_digits > 0 && current_char.is_ascii_digit() {
                            let digit_chunk: String = text[scan_cursor..]
                                .chars()
                                .take(max_digits as usize)
                                .take_while(|c| c.is_ascii_digit())
                                .collect();

                            let end_of_chunk = scan_cursor + digit_chunk.len();
                            boundaries.insert(scan_cursor);
                            boundaries.insert(end_of_chunk);
                            scan_cursor = end_of_chunk; // Jump past the entire sequence
                            continue;
                        }
                    }

                    // Rule 2: If no multi-char feature, check for a normal single-grapheme
                    // override.
                    if current_run_overrides
                        .and_then(|o| o.get(&(scan_cursor as u32)))
                        .is_some()
                    {
                        let grapheme_len = text[scan_cursor..]
                            .graphemes(true)
                            .next()
                            .unwrap_or("")
                            .len();
                        boundaries.insert(scan_cursor);
                        boundaries.insert(scan_cursor + grapheme_len);
                        scan_cursor += grapheme_len;
                        continue;
                    }

                    // Rule 3: No special features or overrides at this point, just advance one
                    // char.
                    scan_cursor += current_char.len_utf8();
                }

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  Boundaries: {:?}",
                        boundaries
                    )));
                }

                // --- Chunk Processing ---
                for (start, end) in boundaries.iter().zip(boundaries.iter().skip(1)) {
                    let (start, end) = (*start, *end);
                    if start >= end {
                        continue;
                    }

                    let text_slice = &text[start..end];
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  Processing chunk from {} to {}: '{}'",
                            start, end, text_slice
                        )));
                    }

                    let style_to_use = if let Some(partial_style) =
                        current_run_overrides.and_then(|o| o.get(&(start as u32)))
                    {
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "  -> Applying override at byte {}",
                                start
                            )));
                        }
                        let mut hasher = DefaultHasher::new();
                        Arc::as_ptr(&run.style).hash(&mut hasher);
                        partial_style.hash(&mut hasher);
                        style_cache
                            .entry(hasher.finish())
                            .or_insert_with(|| Arc::new(run.style.apply_override(partial_style)))
                            .clone()
                    } else {
                        run.style.clone()
                    };

                    let is_combinable_chunk = if let Some(TextCombineUpright::Digits(max_digits)) =
                        &style_to_use.text_combine_upright
                    {
                        *max_digits > 0
                            && !text_slice.is_empty()
                            && text_slice.chars().all(|c| c.is_ascii_digit())
                            && text_slice.chars().count() <= *max_digits as usize
                    } else {
                        false
                    };

                    if is_combinable_chunk {
                        items.push(LogicalItem::CombinedText {
                            source: ContentIndex {
                                run_index: run_idx as u32,
                                item_index: start as u32,
                            },
                            text: text_slice.to_string(),
                            style: style_to_use,
                        });
                    } else {
                        items.push(LogicalItem::Text {
                            source: ContentIndex {
                                run_index: run_idx as u32,
                                item_index: start as u32,
                            },
                            text: text_slice.to_string(),
                            style: style_to_use,
                            marker_position_outside,
                            source_node_id: run.source_node_id,
                        });
                    }
                }
            }
            // Handle explicit line breaks (from white-space: pre or <br>)
            InlineContent::LineBreak(break_info) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  LineBreak: {:?}",
                        break_info
                    )));
                }
                items.push(LogicalItem::Break {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    break_info: break_info.clone(),
                });
            }
            // Other cases (Image, Shape, Space, Tab, Ruby)
            _ => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "  Run is not text, creating generic LogicalItem.".to_string(),
                    ));
                }
                items.push(LogicalItem::Object {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    content: inline_item.clone(),
                });
            }
        }
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting create_logical_items, created {} items ---",
            items.len()
        )));
    }
    items
}

// --- Stage 2 Implementation ---

pub fn get_base_direction_from_logical(logical_items: &[LogicalItem]) -> BidiDirection {
    let first_strong = logical_items.iter().find_map(|item| {
        if let LogicalItem::Text { text, .. } = item {
            Some(unicode_bidi::get_base_direction(text.as_str()))
        } else {
            None
        }
    });

    match first_strong {
        Some(unicode_bidi::Direction::Rtl) => BidiDirection::Rtl,
        _ => BidiDirection::Ltr,
    }
}

pub fn reorder_logical_items(
    logical_items: &[LogicalItem],
    base_direction: BidiDirection,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<VisualItem>, LayoutError> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering reorder_logical_items ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input logical items count: {}",
            logical_items.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Base direction: {:?}",
            base_direction
        )));
    }

    let mut bidi_str = String::new();
    let mut item_map = Vec::new();
    for (idx, item) in logical_items.iter().enumerate() {
        let text = match item {
            LogicalItem::Text { text, .. } => text.as_str(),
            LogicalItem::CombinedText { text, .. } => text.as_str(),
            _ => "\u{FFFC}",
        };
        let start_byte = bidi_str.len();
        bidi_str.push_str(text);
        for _ in start_byte..bidi_str.len() {
            item_map.push(idx);
        }
    }

    if bidi_str.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Bidi string is empty, returning.".to_string(),
            ));
        }
        return Ok(Vec::new());
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Constructed bidi string: '{}'",
            bidi_str
        )));
    }

    let bidi_level = if base_direction == BidiDirection::Rtl {
        Some(Level::rtl())
    } else {
        Some(Level::ltr())
    };
    let bidi_info = BidiInfo::new(&bidi_str, bidi_level);
    let para = &bidi_info.paragraphs[0];
    let (levels, visual_runs) = bidi_info.visual_runs(para, para.range.clone());

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Bidi visual runs generated:".to_string(),
        ));
        for (i, run_range) in visual_runs.iter().enumerate() {
            let level = levels[run_range.start].number();
            let slice = &bidi_str[run_range.start..run_range.end];
            msgs.push(LayoutDebugMessage::info(format!(
                "  Run {}: range={:?}, level={}, text='{}'",
                i, run_range, level, slice
            )));
        }
    }

    let mut visual_items = Vec::new();
    for run_range in visual_runs {
        let bidi_level = BidiLevel::new(levels[run_range.start].number());
        let mut sub_run_start = run_range.start;

        for i in (run_range.start + 1)..run_range.end {
            if item_map[i] != item_map[sub_run_start] {
                let logical_idx = item_map[sub_run_start];
                let logical_item = &logical_items[logical_idx];
                let text_slice = &bidi_str[sub_run_start..i];
                visual_items.push(VisualItem {
                    logical_source: logical_item.clone(),
                    bidi_level,
                    script: crate::text3::script::detect_script(text_slice)
                        .unwrap_or(Script::Latin),
                    text: text_slice.to_string(),
                });
                sub_run_start = i;
            }
        }

        let logical_idx = item_map[sub_run_start];
        let logical_item = &logical_items[logical_idx];
        let text_slice = &bidi_str[sub_run_start..run_range.end];
        visual_items.push(VisualItem {
            logical_source: logical_item.clone(),
            bidi_level,
            script: crate::text3::script::detect_script(text_slice).unwrap_or(Script::Latin),
            text: text_slice.to_string(),
        });
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Final visual items produced:".to_string(),
        ));
        for (i, item) in visual_items.iter().enumerate() {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Item {}: level={}, text='{}'",
                i,
                item.bidi_level.level(),
                item.text
            )));
        }
        msgs.push(LayoutDebugMessage::info(
            "--- Exiting reorder_logical_items ---".to_string(),
        ));
    }
    Ok(visual_items)
}

// --- Stage 3 Implementation ---

/// Shape visual items into ShapedItems using pre-loaded fonts.
///
/// This function does NOT load any fonts - all fonts must be pre-loaded and passed in.
/// If a required font is not in `loaded_fonts`, the text will be skipped with a warning.
pub fn shape_visual_items<T: ParsedFontTrait>(
    visual_items: &[VisualItem],
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<ShapedItem>, LayoutError> {
    let mut shaped = Vec::new();

    for item in visual_items {
        match &item.logical_source {
            LogicalItem::Text {
                style,
                source,
                marker_position_outside,
                source_node_id,
                ..
            } => {
                let direction = if item.bidi_level.is_rtl() {
                    BidiDirection::Rtl
                } else {
                    BidiDirection::Ltr
                };

                let language = script_to_language(item.script, &item.text);

                // Shape text using either FontRef directly or fontconfig-resolved font
                let shaped_clusters_result: Result<Vec<ShapedCluster>, LayoutError> = match &style.font_stack {
                    FontStack::Ref(font_ref) => {
                        // For FontRef, use the font directly without fontconfig
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[TextLayout] Using direct FontRef for text: '{}'",
                                item.text.chars().take(30).collect::<String>()
                            )));
                        }
                        shape_text_correctly(
                            &item.text,
                            item.script,
                            language,
                            direction,
                            font_ref,
                            style,
                            *source,
                            *source_node_id,
                        )
                    }
                    FontStack::Stack(selectors) => {
                        // Build FontChainKey and resolve through fontconfig
                        let cache_key = FontChainKey::from_selectors(selectors);

                        // Look up pre-resolved font chain
                        let font_chain = match font_chain_cache.get(&cache_key) {
                            Some(chain) => chain,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font chain not pre-resolved for {:?} - text will \
                                         not be rendered",
                                        cache_key.font_families
                                    )));
                                }
                                continue;
                            }
                        };

                        // Use the font chain to resolve which font to use for the first character
                        let first_char = item.text.chars().next().unwrap_or('A');
                        let font_id = match font_chain.resolve_char(fc_cache, first_char) {
                            Some((id, _css_source)) => id,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] No font in chain can render character '{}' \
                                         (U+{:04X})",
                                        first_char, first_char as u32
                                    )));
                                }
                                continue;
                            }
                        };

                        // Look up the pre-loaded font
                        match loaded_fonts.get(&font_id) {
                            Some(font) => {
                                shape_text_correctly(
                                    &item.text,
                                    item.script,
                                    language,
                                    direction,
                                    font,
                                    style,
                                    *source,
                                    *source_node_id,
                                )
                            }
                            None => {
                                if let Some(msgs) = debug_messages {
                                    let truncated_text = item.text.chars().take(50).collect::<String>();
                                    let display_text = if item.text.chars().count() > 50 {
                                        format!("{}...", truncated_text)
                                    } else {
                                        truncated_text
                                    };

                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font {:?} not pre-loaded for text: '{}'",
                                        font_id, display_text
                                    )));
                                }
                                continue;
                            }
                        }
                    }
                };

                let mut shaped_clusters = shaped_clusters_result?;

                // Set marker flag on all clusters if this is a marker
                if let Some(is_outside) = marker_position_outside {
                    for cluster in &mut shaped_clusters {
                        cluster.marker_position_outside = Some(*is_outside);
                    }
                }

                shaped.extend(shaped_clusters.into_iter().map(ShapedItem::Cluster));
            }
            LogicalItem::Tab { source, style } => {
                // TODO: To get the space width accurately, we would need to shape
                // a space character with the current font.
                // For now, we approximate it as a fraction of the font size.
                let space_advance = style.font_size_px * 0.33;
                let tab_width = style.tab_size * space_advance;
                shaped.push(ShapedItem::Tab {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: tab_width,
                        height: 0.0,
                    },
                });
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                // TODO: Implement Ruby layout. This is a major feature.
                // 1. Recursively call layout for the `base_text` to get its size.
                // 2. Recursively call layout for the `ruby_text` (with a smaller font from
                //    `style`).
                // 3. Position the ruby text bounds above/beside the base text bounds.
                // 4. Create a single `ShapedItem::Object` or `ShapedItem::CombinedBlock` that
                //    represents the combined metric bounds of the group, which will be used for
                //    line breaking and positioning on the main line.
                // For now, create a placeholder object.
                let placeholder_width = base_text.chars().count() as f32 * style.font_size_px * 0.6;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: placeholder_width,
                        height: style.line_height * 1.5,
                    },
                    baseline_offset: 0.0,
                    content: InlineContent::Text(StyledRun {
                        text: base_text.clone(),
                        style: style.clone(),
                        logical_start_byte: 0,
                        source_node_id: None, // Ruby text is generated, not from DOM
                    }),
                });
            }
            LogicalItem::CombinedText {
                style,
                source,
                text,
            } => {
                let language = script_to_language(item.script, &item.text);

                // Shape CombinedText using either FontRef directly or fontconfig-resolved font
                let glyphs: Vec<Glyph> = match &style.font_stack {
                    FontStack::Ref(font_ref) => {
                        // For FontRef, use the font directly without fontconfig
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[TextLayout] Using direct FontRef for CombinedText: '{}'",
                                text.chars().take(30).collect::<String>()
                            )));
                        }
                        font_ref.shape_text(
                            text,
                            item.script,
                            language,
                            BidiDirection::Ltr,
                            style.as_ref(),
                        )?
                    }
                    FontStack::Stack(selectors) => {
                        // Build FontChainKey and resolve through fontconfig
                        let cache_key = FontChainKey::from_selectors(selectors);

                        let font_chain = match font_chain_cache.get(&cache_key) {
                            Some(chain) => chain,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font chain not pre-resolved for CombinedText {:?}",
                                        cache_key.font_families
                                    )));
                                }
                                continue;
                            }
                        };

                        let first_char = text.chars().next().unwrap_or('A');
                        let font_id = match font_chain.resolve_char(fc_cache, first_char) {
                            Some((id, _)) => id,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] No font for CombinedText char '{}'",
                                        first_char
                                    )));
                                }
                                continue;
                            }
                        };

                        match loaded_fonts.get(&font_id) {
                            Some(font) => {
                                font.shape_text(
                                    text,
                                    item.script,
                                    language,
                                    BidiDirection::Ltr,
                                    style.as_ref(),
                                )?
                            }
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font {:?} not pre-loaded for CombinedText",
                                        font_id
                                    )));
                                }
                                continue;
                            }
                        }
                    }
                };

                let shaped_glyphs = glyphs
                    .into_iter()
                    .map(|g| ShapedGlyph {
                        kind: GlyphKind::Character,
                        glyph_id: g.glyph_id,
                        script: g.script,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics,
                        style: g.style,
                        cluster_offset: 0,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                    })
                    .collect::<Vec<_>>();

                let total_width: f32 = shaped_glyphs.iter().map(|g| g.advance + g.kerning).sum();
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: total_width,
                    height: style.line_height,
                };

                shaped.push(ShapedItem::CombinedBlock {
                    source: *source,
                    glyphs: shaped_glyphs,
                    bounds,
                    baseline_offset: 0.0,
                });
            }
            LogicalItem::Object {
                content, source, ..
            } => {
                let (bounds, baseline) = measure_inline_object(content)?;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds,
                    baseline_offset: baseline,
                    content: content.clone(),
                });
            }
            LogicalItem::Break { source, break_info } => {
                shaped.push(ShapedItem::Break {
                    source: *source,
                    break_info: break_info.clone(),
                });
            }
        }
    }
    Ok(shaped)
}

/// Helper to check if a cluster contains only hanging punctuation.
fn is_hanging_punctuation(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        if c.glyphs.len() == 1 {
            match c.text.as_str() {
                "." | "," | ":" | ";" => true,
                _ => false,
            }
        } else {
            false
        }
    } else {
        false
    }
}

fn shape_text_correctly<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: crate::text3::script::Language,
    direction: BidiDirection,
    font: &T, // Changed from &Arc<T>
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
    source_node_id: Option<NodeId>,
) -> Result<Vec<ShapedCluster>, LayoutError> {
    let glyphs = font.shape_text(text, script, language, direction, style.as_ref())?;

    if glyphs.is_empty() {
        return Ok(Vec::new());
    }

    let mut clusters = Vec::new();

    // Group glyphs by cluster ID from the shaper.
    let mut current_cluster_glyphs = Vec::new();
    let mut cluster_id = glyphs[0].cluster;
    let mut cluster_start_byte_in_text = glyphs[0].logical_byte_index;

    for glyph in glyphs {
        if glyph.cluster != cluster_id {
            // Finalize previous cluster
            let advance = current_cluster_glyphs
                .iter()
                .map(|g: &Glyph| g.advance)
                .sum();

            // Safely extract cluster text - handle cases where byte indices may be out of order
            // (can happen with RTL text or complex GSUB reordering)
            let (start, end) = if cluster_start_byte_in_text <= glyph.logical_byte_index {
                (cluster_start_byte_in_text, glyph.logical_byte_index)
            } else {
                (glyph.logical_byte_index, cluster_start_byte_in_text)
            };
            let cluster_text = text.get(start..end).unwrap_or("");

            clusters.push(ShapedCluster {
                text: cluster_text.to_string(), // Store original text for hyphenation
                source_cluster_id: GraphemeClusterId {
                    source_run: source_index.run_index,
                    start_byte_in_run: cluster_id,
                },
                source_content_index: source_index,
                source_node_id,
                glyphs: current_cluster_glyphs
                    .iter()
                    .map(|g| {
                        let source_char = text
                            .get(g.logical_byte_index..)
                            .and_then(|s| s.chars().next())
                            .unwrap_or('\u{FFFD}');
                        // Calculate cluster_offset safely
                        let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                            (g.logical_byte_index - cluster_start_byte_in_text) as u32
                        } else {
                            0
                        };
                        ShapedGlyph {
                            kind: if g.glyph_id == 0 {
                                GlyphKind::NotDef
                            } else {
                                GlyphKind::Character
                            },
                            glyph_id: g.glyph_id,
                            script: g.script,
                            font_hash: g.font_hash,
                            font_metrics: g.font_metrics.clone(),
                            style: g.style.clone(),
                            cluster_offset,
                            advance: g.advance,
                            kerning: g.kerning,
                            vertical_advance: g.vertical_advance,
                            vertical_offset: g.vertical_bearing,
                            offset: g.offset,
                        }
                    })
                    .collect(),
                advance,
                direction,
                style: style.clone(),
                marker_position_outside: None,
            });
            current_cluster_glyphs.clear();
            cluster_id = glyph.cluster;
            cluster_start_byte_in_text = glyph.logical_byte_index;
        }
        current_cluster_glyphs.push(glyph);
    }

    // Finalize the last cluster
    if !current_cluster_glyphs.is_empty() {
        let advance = current_cluster_glyphs
            .iter()
            .map(|g: &Glyph| g.advance)
            .sum();
        let cluster_text = text.get(cluster_start_byte_in_text..).unwrap_or("");
        clusters.push(ShapedCluster {
            text: cluster_text.to_string(), // Store original text
            source_cluster_id: GraphemeClusterId {
                source_run: source_index.run_index,
                start_byte_in_run: cluster_id,
            },
            source_content_index: source_index,
            source_node_id,
            glyphs: current_cluster_glyphs
                .iter()
                .map(|g| {
                    let source_char = text
                        .get(g.logical_byte_index..)
                        .and_then(|s| s.chars().next())
                        .unwrap_or('\u{FFFD}');
                    // Calculate cluster_offset safely
                    let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                        (g.logical_byte_index - cluster_start_byte_in_text) as u32
                    } else {
                        0
                    };
                    ShapedGlyph {
                        kind: if g.glyph_id == 0 {
                            GlyphKind::NotDef
                        } else {
                            GlyphKind::Character
                        },
                        glyph_id: g.glyph_id,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics.clone(),
                        style: g.style.clone(),
                        script: g.script,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                        cluster_offset,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                    }
                })
                .collect(),
            advance,
            direction,
            style: style.clone(),
            marker_position_outside: None,
        });
    }

    Ok(clusters)
}

/// Measures a non-text object, returning its bounds and baseline offset.
fn measure_inline_object(item: &InlineContent) -> Result<(Rect, f32), LayoutError> {
    match item {
        InlineContent::Image(img) => {
            let size = img.display_size.unwrap_or(img.intrinsic_size);
            Ok((
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                img.baseline_offset,
            ))
        }
        InlineContent::Shape(shape) => Ok({
            let size = shape.shape_def.get_size();
            (
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                shape.baseline_offset,
            )
        }),
        InlineContent::Space(space) => Ok((
            Rect {
                x: 0.0,
                y: 0.0,
                width: space.width,
                height: 0.0,
            },
            0.0,
        )),
        InlineContent::Marker { .. } => {
            // Markers are treated as text content, not measurable objects
            Err(LayoutError::InvalidText(
                "Marker is text content, not a measurable object".into(),
            ))
        }
        _ => Err(LayoutError::InvalidText("Not a measurable object".into())),
    }
}

// --- Stage 4 Implementation: Vertical Text ---

/// Applies orientation and vertical metrics to glyphs if the writing mode is vertical.
fn apply_text_orientation(
    items: Arc<Vec<ShapedItem>>,
    constraints: &UnifiedConstraints,
) -> Result<Arc<Vec<ShapedItem>>, LayoutError> {
    if !constraints.is_vertical() {
        return Ok(items);
    }

    let mut oriented_items = Vec::with_capacity(items.len());
    let writing_mode = constraints.writing_mode.unwrap_or_default();

    for item in items.iter() {
        match item {
            ShapedItem::Cluster(cluster) => {
                let mut new_cluster = cluster.clone();
                let mut total_vertical_advance = 0.0;

                for glyph in &mut new_cluster.glyphs {
                    // Use the vertical metrics already computed during shaping
                    // If they're zero, use fallback values
                    if glyph.vertical_advance > 0.0 {
                        total_vertical_advance += glyph.vertical_advance;
                    } else {
                        // Fallback: use line height for vertical advance
                        let fallback_advance = cluster.style.line_height;
                        glyph.vertical_advance = fallback_advance;
                        // Center the glyph horizontally as a fallback
                        glyph.vertical_offset = Point {
                            x: -glyph.advance / 2.0,
                            y: 0.0,
                        };
                        total_vertical_advance += fallback_advance;
                    }
                }
                // The cluster's `advance` now represents vertical advance.
                new_cluster.advance = total_vertical_advance;
                oriented_items.push(ShapedItem::Cluster(new_cluster));
            }
            // Non-text objects also need their advance axis swapped.
            ShapedItem::Object {
                source,
                bounds,
                baseline_offset,
                content,
            } => {
                let mut new_bounds = *bounds;
                std::mem::swap(&mut new_bounds.width, &mut new_bounds.height);
                oriented_items.push(ShapedItem::Object {
                    source: *source,
                    bounds: new_bounds,
                    baseline_offset: *baseline_offset,
                    content: content.clone(),
                });
            }
            _ => oriented_items.push(item.clone()),
        }
    }

    Ok(Arc::new(oriented_items))
}

// --- Stage 5 & 6 Implementation: Combined Layout Pass ---
// This section replaces the previous simple line breaking and positioning logic.

/// Gets the ascent (distance from baseline to top) and descent (distance from baseline to bottom)
/// for a single item.
pub fn get_item_vertical_metrics(item: &ShapedItem) -> (f32, f32) {
    // (ascent, descent)
    match item {
        ShapedItem::Cluster(c) => {
            if c.glyphs.is_empty() {
                // For an empty text cluster, use the line height from its style as a fallback.
                return (c.style.line_height, 0.0);
            }
            // CORRECTED: Iterate through ALL glyphs in the cluster to find the true max
            // ascent/descent.
            c.glyphs
                .iter()
                .fold((0.0f32, 0.0f32), |(max_asc, max_desc), glyph| {
                    let metrics = &glyph.font_metrics;
                    if metrics.units_per_em == 0 {
                        return (max_asc, max_desc);
                    }
                    let scale = glyph.style.font_size_px / metrics.units_per_em as f32;
                    let item_asc = metrics.ascent * scale;
                    // Descent in OpenType is typically negative, so we negate it to get a positive
                    // distance.
                    let item_desc = (-metrics.descent * scale).max(0.0);
                    (max_asc.max(item_asc), max_desc.max(item_desc))
                })
        }
        ShapedItem::Object {
            bounds,
            baseline_offset,
            ..
        } => {
            // Per analysis, `baseline_offset` is the distance from the bottom.
            let ascent = bounds.height - *baseline_offset;
            let descent = *baseline_offset;
            (ascent.max(0.0), descent.max(0.0))
        }
        ShapedItem::CombinedBlock {
            bounds,
            baseline_offset,
            ..
        } => {
            // CORRECTED: Treat baseline_offset consistently as distance from the bottom (descent).
            let ascent = bounds.height - *baseline_offset;
            let descent = *baseline_offset;
            (ascent.max(0.0), descent.max(0.0))
        }
        _ => (0.0, 0.0), // Breaks and other non-visible items don't affect line height.
    }
}

/// Calculates the maximum ascent and descent for an entire line of items.
/// This determines the "line box" used for vertical alignment.
fn calculate_line_metrics(items: &[ShapedItem]) -> (f32, f32) {
    // (max_ascent, max_descent)
    items
        .iter()
        .fold((0.0f32, 0.0f32), |(max_asc, max_desc), item| {
            let (item_asc, item_desc) = get_item_vertical_metrics(item);
            (max_asc.max(item_asc), max_desc.max(item_desc))
        })
}

/// Performs layout for a single fragment, consuming items from a `BreakCursor`.
///
/// This function contains the core line-breaking and positioning logic, but is
/// designed to operate on a portion of a larger content stream and within the
/// constraints of a single geometric area (a fragment).
///
/// The loop terminates when either the fragment is filled (e.g., runs out of
/// vertical space) or the content stream managed by the `cursor` is exhausted.
///
/// # CSS Inline Layout Module Level 3 Implementation
///
/// This function implements the inline formatting context as described in:
/// https://www.w3.org/TR/css-inline-3/#inline-formatting-context
///
/// ## § 2.1 Layout of Line Boxes
/// "In general, the line-left edge of a line box touches the line-left edge of its
/// containing block and the line-right edge touches the line-right edge of its
/// containing block, and thus the logical width of a line box is equal to the inner
/// logical width of its containing block."
///
/// [ISSUE] available_width should be set to the containing block's inner width,
/// but is currently defaulting to 0.0 in UnifiedConstraints::default().
/// This causes premature line breaking.
///
/// ## § 2.2 Layout Within Line Boxes
/// The layout process follows these steps:
/// 1. Baseline Alignment: All inline-level boxes are aligned by their baselines
/// 2. Content Size Contribution: Calculate layout bounds for each box
/// 3. Line Box Sizing: Size line box to fit aligned layout bounds
/// 4. Content Positioning: Position boxes within the line box
///
/// ## Missing Features:
/// - § 3 Baselines and Alignment Metrics: Only basic baseline alignment implemented
/// - § 4 Baseline Alignment: vertical-align property not fully supported
/// - § 5 Line Spacing: line-height implemented, but line-fit-edge missing
/// - § 6 Trimming Leading: text-box-trim not implemented
pub fn perform_fragment_layout<T: ParsedFontTrait>(
    cursor: &mut BreakCursor,
    logical_items: &[LogicalItem],
    fragment_constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Result<UnifiedLayout, LayoutError> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering perform_fragment_layout ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Constraints: available_width={:?}, available_height={:?}, columns={}, text_wrap={:?}",
            fragment_constraints.available_width,
            fragment_constraints.available_height,
            fragment_constraints.columns,
            fragment_constraints.text_wrap
        )));
    }

    // For TextWrap::Balance, use Knuth-Plass algorithm for optimal line breaking
    // This produces more visually balanced lines at the cost of more computation
    if fragment_constraints.text_wrap == TextWrap::Balance {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Using Knuth-Plass algorithm for text-wrap: balance".to_string(),
            ));
        }

        // Get the shaped items from the cursor
        let shaped_items: Vec<ShapedItem> = cursor.drain_remaining();

        let hyphenator = if fragment_constraints.hyphenation {
            fragment_constraints
                .hyphenation_language
                .and_then(|lang| get_hyphenator(lang).ok())
        } else {
            None
        };

        // Use the Knuth-Plass algorithm for optimal line breaking
        return crate::text3::knuth_plass::kp_layout(
            &shaped_items,
            logical_items,
            fragment_constraints,
            hyphenator.as_ref(),
            fonts,
        );
    }

    let hyphenator = if fragment_constraints.hyphenation {
        fragment_constraints
            .hyphenation_language
            .and_then(|lang| get_hyphenator(lang).ok())
    } else {
        None
    };

    let mut positioned_items = Vec::new();
    let mut layout_bounds = Rect::default();

    let num_columns = fragment_constraints.columns.max(1);
    let total_column_gap = fragment_constraints.column_gap * (num_columns - 1) as f32;

    // CSS Inline Layout § 2.1: "the logical width of a line box is equal to the inner
    // logical width of its containing block"
    //
    // Handle the different available space modes:
    // - Definite(width): Use the specified width for column calculation
    // - MinContent: Use 0.0 to force line breaks at every opportunity
    // - MaxContent: Use a large value to allow content to expand naturally
    let column_width = match fragment_constraints.available_width {
        AvailableSpace::Definite(width) => (width - total_column_gap) / num_columns as f32,
        AvailableSpace::MinContent => {
            // Min-content: effectively 0 width forces immediate line breaks
            0.0
        }
        AvailableSpace::MaxContent => {
            // Max-content: very large width allows content to expand
            // Using f32::MAX / 2.0 to avoid overflow issues
            f32::MAX / 2.0
        }
    };
    let mut current_column = 0;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Column width calculated: {}",
            column_width
        )));
    }

    // Use the CSS direction from constraints instead of auto-detecting from text
    // This ensures that mixed-direction text (e.g., "مرحبا - Hello") uses the
    // correct paragraph-level direction for alignment purposes
    let base_direction = fragment_constraints.direction.unwrap_or(BidiDirection::Ltr);

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PFLayout] Base direction: {:?} (from CSS), Text align: {:?}",
            base_direction, fragment_constraints.text_align
        )));
    }

    'column_loop: while current_column < num_columns {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "\n-- Starting Column {} --",
                current_column
            )));
        }
        let column_start_x =
            (column_width + fragment_constraints.column_gap) * current_column as f32;
        let mut line_top_y = 0.0;
        let mut line_index = 0;
        let mut empty_segment_count = 0; // Failsafe counter for infinite loops
        const MAX_EMPTY_SEGMENTS: usize = 1000; // Maximum allowed consecutive empty segments

        while !cursor.is_done() {
            if let Some(max_height) = fragment_constraints.available_height {
                if line_top_y >= max_height {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  Column full (pen {} >= height {}), breaking to next column.",
                            line_top_y, max_height
                        )));
                    }
                    break;
                }
            }

            if let Some(clamp) = fragment_constraints.line_clamp {
                if line_index >= clamp.get() {
                    break;
                }
            }

            // Create constraints specific to the current column for the line breaker.
            let mut column_constraints = fragment_constraints.clone();
            column_constraints.available_width = AvailableSpace::Definite(column_width);
            let line_constraints = get_line_constraints(
                line_top_y,
                fragment_constraints.line_height,
                &column_constraints,
                debug_messages,
            );

            if line_constraints.segments.is_empty() {
                empty_segment_count += 1;
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  No available segments at y={}, skipping to next line. (empty count: \
                         {}/{})",
                        line_top_y, empty_segment_count, MAX_EMPTY_SEGMENTS
                    )));
                }

                // Failsafe: If we've skipped too many lines without content, break out
                if empty_segment_count >= MAX_EMPTY_SEGMENTS {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "  [WARN] Reached maximum empty segment count ({}). Breaking to \
                             prevent infinite loop.",
                            MAX_EMPTY_SEGMENTS
                        )));
                        msgs.push(LayoutDebugMessage::warning(
                            "  This likely means the shape constraints are too restrictive or \
                             positioned incorrectly."
                                .to_string(),
                        ));
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "  Current y={}, shape boundaries might be outside this range.",
                            line_top_y
                        )));
                    }
                    break;
                }

                // Additional check: If we have shapes and are far beyond the expected height,
                // also break to avoid infinite loops
                if !fragment_constraints.shape_boundaries.is_empty() && empty_segment_count > 50 {
                    // Calculate maximum shape height
                    let max_shape_y: f32 = fragment_constraints
                        .shape_boundaries
                        .iter()
                        .map(|shape| {
                            match shape {
                                ShapeBoundary::Circle { center, radius } => center.y + radius,
                                ShapeBoundary::Ellipse { center, radii } => center.y + radii.height,
                                ShapeBoundary::Polygon { points } => {
                                    points.iter().map(|p| p.y).fold(0.0, f32::max)
                                }
                                ShapeBoundary::Rectangle(rect) => rect.y + rect.height,
                                ShapeBoundary::Path { .. } => f32::MAX, // Can't determine for path
                            }
                        })
                        .fold(0.0, f32::max);

                    if line_top_y > max_shape_y + 100.0 {
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "  [INFO] Current y={} is far beyond maximum shape extent y={}. \
                                 Breaking layout.",
                                line_top_y, max_shape_y
                            )));
                            msgs.push(LayoutDebugMessage::info(
                                "  Shape boundaries exist but no segments available - text cannot \
                                 fit in shape."
                                    .to_string(),
                            ));
                        }
                        break;
                    }
                }

                line_top_y += fragment_constraints.line_height;
                continue;
            }

            // Reset counter when we find valid segments
            empty_segment_count = 0;

            // CSS Text Module Level 3 § 5 Line Breaking and Word Boundaries
            // https://www.w3.org/TR/css-text-3/#line-breaking
            // "When an inline box exceeds the logical width of a line box, it is split
            // into several fragments, which are partitioned across multiple line boxes."
            let (mut line_items, was_hyphenated) =
                break_one_line(cursor, &line_constraints, false, hyphenator.as_ref(), fonts);
            if line_items.is_empty() {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "  Break returned no items. Ending column.".to_string(),
                    ));
                }
                break;
            }

            let line_text_before_rev: String = line_items
                .iter()
                .filter_map(|i| i.as_cluster())
                .map(|c| c.text.as_str())
                .collect();
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    // FIX: The log message was misleading. Items are in visual order.
                    "[PFLayout] Line items from breaker (visual order): [{}]",
                    line_text_before_rev
                )));
            }

            let (mut line_pos_items, line_height) = position_one_line(
                line_items,
                &line_constraints,
                line_top_y,
                line_index,
                fragment_constraints.text_align,
                base_direction,
                cursor.is_done() && !was_hyphenated,
                fragment_constraints,
                debug_messages,
                fonts,
            );

            for item in &mut line_pos_items {
                item.position.x += column_start_x;
            }

            line_top_y += line_height.max(fragment_constraints.line_height);
            line_index += 1;
            positioned_items.extend(line_pos_items);
        }
        current_column += 1;
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting perform_fragment_layout, positioned {} items ---",
            positioned_items.len()
        )));
    }

    let layout = UnifiedLayout {
        items: positioned_items,
        overflow: OverflowInfo::default(),
    };

    // Calculate bounds on demand via the bounds() method
    let calculated_bounds = layout.bounds();
    
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Calculated bounds: width={}, height={} ---",
            calculated_bounds.width, calculated_bounds.height
        )));
    }

    Ok(layout)
}

/// Breaks a single line of items to fit within the given geometric constraints,
/// handling multi-segment lines and hyphenation.
/// Break a single line from the current cursor position.
///
/// # CSS Text Module Level 3 \u00a7 5 Line Breaking and Word Boundaries
/// https://www.w3.org/TR/css-text-3/#line-breaking
///
/// Implements the line breaking algorithm:
/// 1. "When an inline box exceeds the logical width of a line box, it is split into several
///    fragments, which are partitioned across multiple line boxes."
///
/// ## \u2705 Implemented Features:
/// - **Break Opportunities**: Identifies word boundaries and break points
/// - **Soft Wraps**: Wraps at spaces between words
/// - **Hard Breaks**: Handles explicit line breaks (\\n)
/// - **Overflow**: If a word is too long, places it anyway to avoid infinite loop
/// - **Hyphenation**: Tries to break long words at hyphenation points (\u00a7 5.4)
///
/// ## \u26a0\ufe0f Known Issues:
/// - If `line_constraints.total_available` is 0.0 (from `available_width: 0.0` bug), every word
///   will overflow, causing single-word lines
/// - This is the symptom visible in the PDF: "List items break extremely early"
///
/// ## \u00a7 5.2 Breaking Rules for Letters
/// \u2705 IMPLEMENTED: Uses Unicode line breaking algorithm
/// - Relies on UAX #14 for break opportunities
/// - Respects non-breaking spaces and zero-width joiners
///
/// ## \u00a7 5.3 Breaking Rules for Punctuation
/// \u26a0\ufe0f PARTIAL: Basic punctuation handling
/// - \u274c TODO: hanging-punctuation is declared in UnifiedConstraints but not used here
/// - \u274c TODO: Should implement punctuation trimming at line edges
///
/// ## \u00a7 5.4 Hyphenation
/// \u2705 IMPLEMENTED: Automatic hyphenation with hyphenator library
/// - Tries to hyphenate words that overflow
/// - Inserts hyphen glyph at break point
/// - Carries remainder to next line
///
/// ## \u00a7 5.5 Overflow Wrapping
/// \u2705 IMPLEMENTED: Emergency breaking
/// - If line is empty and word doesn't fit, forces at least one item
/// - Prevents infinite loop
/// - This is "overflow-wrap: break-word" behavior
///
/// # Missing Features:
/// - \u274c word-break property (normal, break-all, keep-all)
/// - \u274c line-break property (auto, loose, normal, strict, anywhere)
/// - \u274c overflow-wrap: anywhere vs break-word distinction
/// - \u274c white-space: break-spaces handling
pub fn break_one_line<T: ParsedFontTrait>(
    cursor: &mut BreakCursor,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    hyphenator: Option<&Standard>,
    fonts: &LoadedFonts<T>,
) -> (Vec<ShapedItem>, bool) {
    let mut line_items = Vec::new();
    let mut current_width = 0.0;

    if cursor.is_done() {
        return (Vec::new(), false);
    }

    // CSS Text Module Level 3 § 4.1.1: At the beginning of a line, white space
    // is collapsed away. Skip leading whitespace at line start.
    // https://www.w3.org/TR/css-text-3/#white-space-phase-2
    while !cursor.is_done() {
        let next_unit = cursor.peek_next_unit();
        if next_unit.is_empty() {
            break;
        }
        // Check if the first item is whitespace-only
        if next_unit.len() == 1 && is_word_separator(&next_unit[0]) {
            // Skip this whitespace at line start
            cursor.consume(1);
        } else {
            break;
        }
    }

    loop {
        // 1. Identify the next unbreakable unit (word) or break opportunity.
        let next_unit = cursor.peek_next_unit();
        if next_unit.is_empty() {
            break; // End of content
        }

        // Handle hard breaks immediately.
        if let Some(ShapedItem::Break { .. }) = next_unit.first() {
            line_items.push(next_unit[0].clone());
            cursor.consume(1);
            return (line_items, false);
        }

        let unit_width: f32 = next_unit
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();
        let available_width = line_constraints.total_available - current_width;

        // 2. Can the whole unit fit on the current line?
        if unit_width <= available_width {
            line_items.extend_from_slice(&next_unit);
            current_width += unit_width;
            cursor.consume(next_unit.len());
        } else {
            // 3. The unit overflows. Can we hyphenate it?
            if let Some(hyphenator) = hyphenator {
                // We only try to hyphenate if the unit is a word (not a space).
                if !is_break_opportunity(next_unit.last().unwrap()) {
                    if let Some(hyphenation_result) = try_hyphenate_word_cluster(
                        &next_unit,
                        available_width,
                        is_vertical,
                        hyphenator,
                        fonts,
                    ) {
                        line_items.extend(hyphenation_result.line_part);
                        // Consume the original full word from the cursor.
                        cursor.consume(next_unit.len());
                        // Put the remainder back for the next line.
                        cursor.partial_remainder = hyphenation_result.remainder_part;
                        return (line_items, true);
                    }
                }
            }

            // 4. Cannot hyphenate or fit. The line is finished.
            // If the line is empty, we must force at least one item to avoid an infinite loop.
            if line_items.is_empty() {
                line_items.push(next_unit[0].clone());
                cursor.consume(1);
            }
            break;
        }
    }

    (line_items, false)
}

/// Represents a single valid hyphenation point within a word.
#[derive(Clone)]
pub struct HyphenationBreak {
    /// The number of characters from the original word string included on the line.
    pub char_len_on_line: usize,
    /// The total advance width of the line part + the hyphen.
    pub width_on_line: f32,
    /// The cluster(s) that will remain on the current line.
    pub line_part: Vec<ShapedItem>,
    /// The cluster that represents the hyphen character itself.
    pub hyphen_item: ShapedItem,
    /// The cluster(s) that will be carried over to the next line.
    /// CRITICAL FIX: Changed from ShapedItem to Vec<ShapedItem>
    pub remainder_part: Vec<ShapedItem>,
}

/// A "word" is defined as a sequence of one or more adjacent ShapedClusters.
pub fn find_all_hyphenation_breaks<T: ParsedFontTrait>(
    word_clusters: &[ShapedCluster],
    hyphenator: &Standard,
    is_vertical: bool, // Pass this in to use correct metrics
    fonts: &LoadedFonts<T>,
) -> Option<Vec<HyphenationBreak>> {
    if word_clusters.is_empty() {
        return None;
    }

    // --- 1. Concatenate the TRUE text and build a robust map ---
    let mut word_string = String::new();
    let mut char_map = Vec::new();
    let mut current_width = 0.0;

    for (cluster_idx, cluster) in word_clusters.iter().enumerate() {
        for (char_byte_offset, _ch) in cluster.text.char_indices() {
            let glyph_idx = cluster
                .glyphs
                .iter()
                .rposition(|g| g.cluster_offset as usize <= char_byte_offset)
                .unwrap_or(0);
            let glyph = &cluster.glyphs[glyph_idx];

            let num_chars_in_glyph = cluster.text[glyph.cluster_offset as usize..]
                .chars()
                .count();
            let advance_per_char = if is_vertical {
                glyph.vertical_advance
            } else {
                glyph.advance
            } / (num_chars_in_glyph as f32).max(1.0);

            current_width += advance_per_char;
            char_map.push((cluster_idx, glyph_idx, current_width));
        }
        word_string.push_str(&cluster.text);
    }

    // --- 2. Get hyphenation opportunities ---
    let opportunities = hyphenator.hyphenate(&word_string);
    if opportunities.breaks.is_empty() {
        return None;
    }

    let last_cluster = word_clusters.last().unwrap();
    let last_glyph = last_cluster.glyphs.last().unwrap();
    let style = last_cluster.style.clone();

    // Look up font from hash
    let font = fonts.get_by_hash(last_glyph.font_hash)?;
    let (hyphen_glyph_id, hyphen_advance) =
        font.get_hyphen_glyph_and_advance(style.font_size_px)?;

    let mut possible_breaks = Vec::new();

    // --- 3. Generate a HyphenationBreak for each valid opportunity ---
    for &break_char_idx in &opportunities.breaks {
        // The break is *before* the character at this index.
        // So the last character on the line is at `break_char_idx - 1`.
        if break_char_idx == 0 || break_char_idx > char_map.len() {
            continue;
        }

        let (_, _, width_at_break) = char_map[break_char_idx - 1];

        // The line part is all clusters *before* the break index.
        let line_part: Vec<ShapedItem> = word_clusters[..break_char_idx]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();

        // The remainder is all clusters *from* the break index onward.
        let remainder_part: Vec<ShapedItem> = word_clusters[break_char_idx..]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();

        let hyphen_item = ShapedItem::Cluster(ShapedCluster {
            text: "-".to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            source_node_id: None, // Hyphen is generated, not from DOM
            glyphs: vec![ShapedGlyph {
                kind: GlyphKind::Hyphen,
                glyph_id: hyphen_glyph_id,
                font_hash: last_glyph.font_hash,
                font_metrics: last_glyph.font_metrics.clone(),
                cluster_offset: 0,
                script: Script::Latin,
                advance: hyphen_advance,
                kerning: 0.0,
                offset: Point::default(),
                style: style.clone(),
                vertical_advance: hyphen_advance,
                vertical_offset: Point::default(),
            }],
            advance: hyphen_advance,
            direction: BidiDirection::Ltr,
            style: style.clone(),
            marker_position_outside: None,
        });

        possible_breaks.push(HyphenationBreak {
            char_len_on_line: break_char_idx,
            width_on_line: width_at_break + hyphen_advance,
            line_part,
            hyphen_item,
            remainder_part,
        });
    }

    Some(possible_breaks)
}

/// Tries to find a hyphenation point within a word, returning the line part and remainder.
fn try_hyphenate_word_cluster<T: ParsedFontTrait>(
    word_items: &[ShapedItem],
    remaining_width: f32,
    is_vertical: bool,
    hyphenator: &Standard,
    fonts: &LoadedFonts<T>,
) -> Option<HyphenationResult> {
    let word_clusters: Vec<ShapedCluster> = word_items
        .iter()
        .filter_map(|item| item.as_cluster().cloned())
        .collect();

    if word_clusters.is_empty() {
        return None;
    }

    let all_breaks = find_all_hyphenation_breaks(&word_clusters, hyphenator, is_vertical, fonts)?;

    if let Some(best_break) = all_breaks
        .into_iter()
        .rfind(|b| b.width_on_line <= remaining_width)
    {
        let mut line_part = best_break.line_part;
        line_part.push(best_break.hyphen_item);

        return Some(HyphenationResult {
            line_part,
            remainder_part: best_break.remainder_part,
        });
    }

    None
}

/// Positions a single line of items, handling alignment and justification within segments.
///
/// This function is architecturally critical for cache safety. It does not mutate the
/// `advance` or `bounds` of the input `ShapedItem`s. Instead, it applies justification
/// spacing by adjusting the drawing pen's position (`main_axis_pen`).
///
/// # Returns
/// A tuple containing the `Vec` of positioned items and the calculated height of the line box.
/// Position items on a single line after breaking.
///
/// # CSS Inline Layout Module Level 3 \u00a7 2.2 Layout Within Line Boxes
/// https://www.w3.org/TR/css-inline-3/#layout-within-line-boxes
///
/// Implements the positioning algorithm:
/// 1. "All inline-level boxes are aligned by their baselines"
/// 2. "Calculate layout bounds for each inline box"
/// 3. "Size the line box to fit the aligned layout bounds"
/// 4. "Position all inline boxes within the line box"
///
/// ## \u2705 Implemented Features:
///
/// ### \u00a7 4 Baseline Alignment (vertical-align)
/// \u26a0\ufe0f PARTIAL IMPLEMENTATION:
/// - \u2705 `baseline`: Aligns box baseline with parent baseline (default)
/// - \u2705 `top`: Aligns top of box with top of line box
/// - \u2705 `middle`: Centers box within line box
/// - \u2705 `bottom`: Aligns bottom of box with bottom of line box
/// - \u274c MISSING: `text-top`, `text-bottom`, `sub`, `super`
/// - \u274c MISSING: `<length>`, `<percentage>` values for custom offset
///
/// ### \u00a7 2.2.1 Text Alignment (text-align)
/// \u2705 IMPLEMENTED:
/// - `left`, `right`, `center`: Physical alignment
/// - `start`, `end`: Logical alignment (respects direction: ltr/rtl)
/// - `justify`: Distributes space between words/characters
/// - `justify-all`: Justifies last line too
///
/// ### \u00a7 7.3 Text Justification (text-justify)
/// \u2705 IMPLEMENTED:
/// - `inter-word`: Adds space between words
/// - `inter-character`: Adds space between characters
/// - `kashida`: Arabic kashida elongation
/// - \u274c MISSING: `distribute` (CJK justification)
///
/// ### CSS Text \u00a7 8.1 Text Indentation (text-indent)
/// \u2705 IMPLEMENTED: First line indentation
///
/// ### CSS Text \u00a7 4.1 Word Spacing (word-spacing)
/// \u2705 IMPLEMENTED: Additional space between words
///
/// ### CSS Text \u00a7 4.2 Letter Spacing (letter-spacing)
/// \u2705 IMPLEMENTED: Additional space between characters
///
/// ## Segment-Aware Layout:
/// \u2705 Handles CSS Shapes and multi-column layouts
/// - Breaks line into segments (for shape boundaries)
/// - Calculates justification per segment
/// - Applies alignment within each segment's bounds
///
/// ## Known Issues:
/// - \u26a0\ufe0f If segment.width is infinite (from intrinsic sizing), sets alignment_offset=0 to
///   avoid infinite positioning. This is correct for measurement but documented for clarity.
/// - The function assumes `line_index == 0` means first line for text-indent. A more robust system
///   would track paragraph boundaries.
///
/// # Missing Features:
/// - \u274c \u00a7 6 Trimming Leading (text-box-trim, text-box-edge)
/// - \u274c \u00a7 3.3 Initial Letters (drop caps)
/// - \u274c Full vertical-align support (sub, super, lengths, percentages)
/// - \u274c white-space: break-spaces alignment behavior
pub fn position_one_line<T: ParsedFontTrait>(
    line_items: Vec<ShapedItem>,
    line_constraints: &LineConstraints,
    line_top_y: f32,
    line_index: usize,
    text_align: TextAlign,
    base_direction: BidiDirection,
    is_last_line: bool,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> (Vec<PositionedItem>, f32) {
    let line_text: String = line_items
        .iter()
        .filter_map(|i| i.as_cluster())
        .map(|c| c.text.as_str())
        .collect();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "\n--- Entering position_one_line for line: [{}] ---",
            line_text
        )));
    }
    // NEW: Resolve the final physical alignment here, inside the function.
    let physical_align = match (text_align, base_direction) {
        (TextAlign::Start, BidiDirection::Ltr) => TextAlign::Left,
        (TextAlign::Start, BidiDirection::Rtl) => TextAlign::Right,
        (TextAlign::End, BidiDirection::Ltr) => TextAlign::Right,
        (TextAlign::End, BidiDirection::Rtl) => TextAlign::Left,
        // Physical alignments are returned as-is, regardless of direction.
        (other, _) => other,
    };
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[Pos1Line] Physical align: {:?}",
            physical_align
        )));
    }

    if line_items.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut positioned = Vec::new();
    let is_vertical = constraints.is_vertical();

    // The line box is calculated once for all items on the line, regardless of segment.
    let (line_ascent, line_descent) = calculate_line_metrics(&line_items);
    let line_box_height = line_ascent + line_descent;

    // The baseline for the entire line is determined by its tallest item.
    let line_baseline_y = line_top_y + line_ascent;

    // --- Segment-Aware Positioning ---
    let mut item_cursor = 0;
    let is_first_line_of_para = line_index == 0; // Simplified assumption

    for (segment_idx, segment) in line_constraints.segments.iter().enumerate() {
        if item_cursor >= line_items.len() {
            break;
        }

        // 1. Collect all items that fit into the current segment.
        let mut segment_items = Vec::new();
        let mut current_segment_width = 0.0;
        while item_cursor < line_items.len() {
            let item = &line_items[item_cursor];
            let item_measure = get_item_measure(item, is_vertical);
            // Put at least one item in the segment to avoid getting stuck.
            if current_segment_width + item_measure > segment.width && !segment_items.is_empty() {
                break;
            }
            segment_items.push(item.clone());
            current_segment_width += item_measure;
            item_cursor += 1;
        }

        if segment_items.is_empty() {
            continue;
        }

        // 2. Calculate justification spacing *for this segment only*.
        let (extra_word_spacing, extra_char_spacing) = if constraints.text_justify
            != JustifyContent::None
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
            && constraints.text_justify != JustifyContent::Kashida
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![segment.clone()],
                total_available: segment.width,
            };
            calculate_justification_spacing(
                &segment_items,
                &segment_line_constraints,
                constraints.text_justify,
                is_vertical,
            )
        } else {
            (0.0, 0.0)
        };

        // Kashida justification needs to be segment-aware if used.
        let justified_segment_items = if constraints.text_justify == JustifyContent::Kashida
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![segment.clone()],
                total_available: segment.width,
            };
            justify_kashida_and_rebuild(
                segment_items,
                &segment_line_constraints,
                is_vertical,
                debug_messages,
                fonts,
            )
        } else {
            segment_items
        };

        // Recalculate width in case kashida changed the item list
        let final_segment_width: f32 = justified_segment_items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        // 3. Calculate alignment offset *within this segment*.
        let remaining_space = segment.width - final_segment_width;

        // Handle MaxContent/indefinite width: when available_width is MaxContent (for intrinsic
        // sizing), segment.width will be f32::MAX / 2.0. Alignment calculations would
        // produce huge offsets. In this case, treat as left-aligned (offset = 0) since
        // we're measuring natural content width. We check for both infinite AND very large
        // values (> 1e30) to catch the MaxContent case.
        let is_indefinite_width = segment.width.is_infinite() || segment.width > 1e30;
        let alignment_offset = if is_indefinite_width {
            0.0 // No alignment offset for indefinite width
        } else {
            match physical_align {
                TextAlign::Center => remaining_space / 2.0,
                TextAlign::Right => remaining_space,
                _ => 0.0, // Left, Justify
            }
        };

        let mut main_axis_pen = segment.start_x + alignment_offset;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[Pos1Line] Segment width: {}, Item width: {}, Remaining space: {}, Initial pen: \
                 {}",
                segment.width, final_segment_width, remaining_space, main_axis_pen
            )));
        }

        // Apply text-indent only to the very first segment of the first line.
        if is_first_line_of_para && segment_idx == 0 {
            main_axis_pen += constraints.text_indent;
        }

        // Calculate total marker width for proper outside marker positioning
        // We need to position all marker clusters together in the padding gutter
        let total_marker_width: f32 = justified_segment_items
            .iter()
            .filter_map(|item| {
                if let ShapedItem::Cluster(c) = item {
                    if c.marker_position_outside == Some(true) {
                        return Some(get_item_measure(item, is_vertical));
                    }
                }
                None
            })
            .sum();

        // Track marker pen separately - starts at negative position for outside markers
        let marker_spacing = 4.0; // Small gap between marker and content
        let mut marker_pen = if total_marker_width > 0.0 {
            -(total_marker_width + marker_spacing)
        } else {
            0.0
        };

        // 4. Position the items belonging to this segment.
        //
        // Vertical alignment positioning (CSS vertical-align)
        //
        // Currently, we use `constraints.vertical_align` for ALL items on the line.
        // This is the GLOBAL vertical alignment set on the containing block.
        //
        // KNOWN LIMITATION / TODO:
        //
        // Per-item vertical-align (stored in `InlineImage.alignment`) is NOT used here.
        // According to CSS, each inline element can have its own vertical-align value:
        //   <img style="vertical-align: top"> would align to line top
        //   <img style="vertical-align: middle"> would center in line box
        //   <img style="vertical-align: bottom"> would align to line bottom
        //
        // To fix this, we would need dir_to:
        // 1. Add a helper function `get_item_vertical_align(&item)` that extracts the alignment
        //    from ShapedItem::Object -> InlineContent::Image -> alignment
        // 2. Use that alignment instead of `constraints.vertical_align` for Objects
        //
        // For now, all items use the global alignment which works correctly for
        // text-only content or when all images have the same alignment.
        //
        // Reference: CSS Inline Layout Level 3 § 4 Baseline Alignment
        // https://www.w3.org/TR/css-inline-3/#baseline-alignment
        for item in justified_segment_items {
            let (item_ascent, item_descent) = get_item_vertical_metrics(&item);
            let item_baseline_pos = match constraints.vertical_align {
                VerticalAlign::Top => line_top_y + item_ascent,
                VerticalAlign::Middle => {
                    line_top_y + (line_box_height / 2.0) - ((item_ascent + item_descent) / 2.0)
                        + item_ascent
                }
                VerticalAlign::Bottom => line_top_y + line_box_height - item_descent,
                _ => line_baseline_y, // Baseline
            };

            // Calculate item measure (needed for both positioning and pen advance)
            let item_measure = get_item_measure(&item, is_vertical);

            let position = if is_vertical {
                Point {
                    x: item_baseline_pos - item_ascent,
                    y: main_axis_pen,
                }
            } else {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[Pos1Line] is_vertical=false, main_axis_pen={}, item_baseline_pos={}, \
                         item_ascent={}",
                        main_axis_pen, item_baseline_pos, item_ascent
                    )));
                }

                // Check if this is an outside marker - if so, position it in the padding gutter
                let x_position = if let ShapedItem::Cluster(cluster) = &item {
                    if cluster.marker_position_outside == Some(true) {
                        // Use marker_pen for sequential marker positioning
                        let marker_width = item_measure;
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[Pos1Line] Outside marker detected! width={}, positioning at \
                                 marker_pen={}",
                                marker_width, marker_pen
                            )));
                        }
                        let pos = marker_pen;
                        marker_pen += marker_width; // Advance marker pen for next marker cluster
                        pos
                    } else {
                        main_axis_pen
                    }
                } else {
                    main_axis_pen
                };

                Point {
                    y: item_baseline_pos - item_ascent,
                    x: x_position,
                }
            };

            // item_measure is calculated above for marker positioning
            let item_text = item
                .as_cluster()
                .map(|c| c.text.as_str())
                .unwrap_or("[OBJ]");
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[Pos1Line] Positioning item '{}' at pen_x={}",
                    item_text, main_axis_pen
                )));
            }
            positioned.push(PositionedItem {
                item: item.clone(),
                position,
                line_index,
            });

            // Outside markers don't advance the pen - they're positioned in the padding gutter
            let is_outside_marker = if let ShapedItem::Cluster(c) = &item {
                c.marker_position_outside == Some(true)
            } else {
                false
            };

            if !is_outside_marker {
                main_axis_pen += item_measure;
            }

            // Apply calculated spacing to the pen (skip for outside markers)
            if !is_outside_marker && extra_char_spacing > 0.0 && can_justify_after(&item) {
                main_axis_pen += extra_char_spacing;
            }
            if let ShapedItem::Cluster(c) = &item {
                if !is_outside_marker {
                    let letter_spacing_px = match c.style.letter_spacing {
                        Spacing::Px(px) => px as f32,
                        Spacing::Em(em) => em * c.style.font_size_px,
                    };
                    main_axis_pen += letter_spacing_px;
                    if is_word_separator(&item) {
                        let word_spacing_px = match c.style.word_spacing {
                            Spacing::Px(px) => px as f32,
                            Spacing::Em(em) => em * c.style.font_size_px,
                        };
                        main_axis_pen += word_spacing_px;
                        main_axis_pen += extra_word_spacing;
                    }
                }
            }
        }
    }

    (positioned, line_box_height)
}

/// Calculates the starting pen offset to achieve the desired text alignment.
fn calculate_alignment_offset(
    items: &[ShapedItem],
    line_constraints: &LineConstraints,
    align: TextAlign,
    is_vertical: bool,
    constraints: &UnifiedConstraints,
) -> f32 {
    // Simplified to use the first segment for alignment.
    if let Some(segment) = line_constraints.segments.first() {
        let total_width: f32 = items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        let available_width = if constraints.segment_alignment == SegmentAlignment::Total {
            line_constraints.total_available
        } else {
            segment.width
        };

        if total_width >= available_width {
            return 0.0; // No alignment needed if line is full or overflows
        }

        let remaining_space = available_width - total_width;

        match align {
            TextAlign::Center => remaining_space / 2.0,
            TextAlign::Right => remaining_space,
            _ => 0.0, // Left, Justify, Start, End
        }
    } else {
        0.0
    }
}

/// Calculates the extra spacing needed for justification without modifying the items.
///
/// This function is pure and does not mutate any state, making it safe to use
/// with cached `ShapedItem` data.
///
/// # Arguments
/// * `items` - A slice of items on the line.
/// * `line_constraints` - The geometric constraints for the line.
/// * `text_justify` - The type of justification to calculate.
/// * `is_vertical` - Whether the layout is vertical.
///
/// # Returns
/// A tuple `(extra_per_word, extra_per_char)` containing the extra space in pixels
/// to add at each word or character justification opportunity.
fn calculate_justification_spacing(
    items: &[ShapedItem],
    line_constraints: &LineConstraints,
    text_justify: JustifyContent,
    is_vertical: bool,
) -> (f32, f32) {
    // (extra_per_word, extra_per_char)
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;

    if total_width >= available_width || available_width <= 0.0 {
        return (0.0, 0.0);
    }

    let extra_space = available_width - total_width;

    match text_justify {
        JustifyContent::InterWord => {
            // Count justification opportunities (spaces).
            let space_count = items.iter().filter(|item| is_word_separator(item)).count();
            if space_count > 0 {
                (extra_space / space_count as f32, 0.0)
            } else {
                (0.0, 0.0) // No spaces to expand, do nothing.
            }
        }
        JustifyContent::InterCharacter | JustifyContent::Distribute => {
            // Count justification opportunities (between non-combining characters).
            let gap_count = items
                .iter()
                .enumerate()
                .filter(|(i, item)| *i < items.len() - 1 && can_justify_after(item))
                .count();
            if gap_count > 0 {
                (0.0, extra_space / gap_count as f32)
            } else {
                (0.0, 0.0) // No gaps to expand, do nothing.
            }
        }
        // Kashida justification modifies the item list and is handled by a separate function.
        _ => (0.0, 0.0),
    }
}

/// Rebuilds a line of items, inserting Kashida glyphs for justification.
///
/// This function is non-mutating with respect to its inputs. It takes ownership of the
/// original items and returns a completely new `Vec`. This is necessary because Kashida
/// justification changes the number of items on the line, and must not modify cached data.
pub fn justify_kashida_and_rebuild<T: ParsedFontTrait>(
    items: Vec<ShapedItem>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Vec<ShapedItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering justify_kashida_and_rebuild ---".to_string(),
        ));
    }
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Total item width: {}, Available width: {}",
            total_width, available_width
        )));
    }

    if total_width >= available_width || available_width <= 0.0 {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No justification needed (line is full or invalid).".to_string(),
            ));
        }
        return items;
    }

    let extra_space = available_width - total_width;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Extra space to fill: {}",
            extra_space
        )));
    }

    let font_info = items.iter().find_map(|item| {
        if let ShapedItem::Cluster(c) = item {
            if let Some(glyph) = c.glyphs.first() {
                if glyph.script == Script::Arabic {
                    // Look up font from hash
                    if let Some(font) = fonts.get_by_hash(glyph.font_hash) {
                        return Some((
                            font.clone(),
                            glyph.font_hash,
                            glyph.font_metrics.clone(),
                            glyph.style.clone(),
                        ));
                    }
                }
            }
        }
        None
    });

    let (font, font_hash, font_metrics, style) = match font_info {
        Some(info) => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(
                    "Found Arabic font for kashida.".to_string(),
                ));
            }
            info
        }
        None => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(
                    "No Arabic font found on line. Cannot insert kashidas.".to_string(),
                ));
            }
            return items;
        }
    };

    let (kashida_glyph_id, kashida_advance) =
        match font.get_kashida_glyph_and_advance(style.font_size_px) {
            Some((id, adv)) if adv > 0.0 => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "Font provides kashida glyph with advance {}",
                        adv
                    )));
                }
                (id, adv)
            }
            _ => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "Font does not support kashida justification.".to_string(),
                    ));
                }
                return items;
            }
        };

    let opportunity_indices: Vec<usize> = items
        .windows(2)
        .enumerate()
        .filter_map(|(i, window)| {
            if let (ShapedItem::Cluster(cur), ShapedItem::Cluster(next)) = (&window[0], &window[1])
            {
                if is_arabic_cluster(cur)
                    && is_arabic_cluster(next)
                    && !is_word_separator(&window[1])
                {
                    return Some(i + 1);
                }
            }
            None
        })
        .collect();

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Found {} kashida insertion opportunities at indices: {:?}",
            opportunity_indices.len(),
            opportunity_indices
        )));
    }

    if opportunity_indices.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No opportunities found. Exiting.".to_string(),
            ));
        }
        return items;
    }

    let num_kashidas_to_insert = (extra_space / kashida_advance).floor() as usize;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Calculated number of kashidas to insert: {}",
            num_kashidas_to_insert
        )));
    }

    if num_kashidas_to_insert == 0 {
        return items;
    }

    let kashidas_per_point = num_kashidas_to_insert / opportunity_indices.len();
    let mut remainder = num_kashidas_to_insert % opportunity_indices.len();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Distributing kashidas: {} per point, with {} remainder.",
            kashidas_per_point, remainder
        )));
    }

    let kashida_item = {
        /* ... as before ... */
        let kashida_glyph = ShapedGlyph {
            kind: GlyphKind::Kashida {
                width: kashida_advance,
            },
            glyph_id: kashida_glyph_id,
            font_hash,
            font_metrics: font_metrics.clone(),
            style: style.clone(),
            script: Script::Arabic,
            advance: kashida_advance,
            kerning: 0.0,
            cluster_offset: 0,
            offset: Point::default(),
            vertical_advance: 0.0,
            vertical_offset: Point::default(),
        };
        ShapedItem::Cluster(ShapedCluster {
            text: "\u{0640}".to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            source_node_id: None, // Kashida is generated, not from DOM
            glyphs: vec![kashida_glyph],
            advance: kashida_advance,
            direction: BidiDirection::Ltr,
            style,
            marker_position_outside: None,
        })
    };

    let mut new_items = Vec::with_capacity(items.len() + num_kashidas_to_insert);
    let mut last_copy_idx = 0;
    for &point in &opportunity_indices {
        new_items.extend_from_slice(&items[last_copy_idx..point]);
        let mut num_to_insert = kashidas_per_point;
        if remainder > 0 {
            num_to_insert += 1;
            remainder -= 1;
        }
        for _ in 0..num_to_insert {
            new_items.push(kashida_item.clone());
        }
        last_copy_idx = point;
    }
    new_items.extend_from_slice(&items[last_copy_idx..]);

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting justify_kashida_and_rebuild, new item count: {} ---",
            new_items.len()
        )));
    }
    new_items
}

/// Helper to determine if a cluster belongs to the Arabic script.
fn is_arabic_cluster(cluster: &ShapedCluster) -> bool {
    // A cluster is considered Arabic if its first non-NotDef glyph is from the Arabic script.
    // This is a robust heuristic for mixed-script lines.
    cluster.glyphs.iter().any(|g| g.script == Script::Arabic)
}

/// Helper to identify if an item is a word separator (like a space).
pub fn is_word_separator(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        // A cluster is a word separator if its text is whitespace.
        // This is a simplification; a single glyph might be whitespace.
        c.text.chars().any(|g| g.is_whitespace())
    } else {
        false
    }
}

/// Helper to identify if space can be added after an item.
fn can_justify_after(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.chars().last().map_or(false, |g| {
            !g.is_whitespace() && classify_character(g as u32) != CharacterClass::Combining
        })
    } else {
        // Can generally justify after inline objects unless they are followed by a break.
        !matches!(item, ShapedItem::Break { .. })
    }
}

/// Classifies a character for layout purposes (e.g., justification behavior).
/// Copied from `mod.rs`.
fn classify_character(codepoint: u32) -> CharacterClass {
    match codepoint {
        0x0020 | 0x00A0 | 0x3000 => CharacterClass::Space,
        0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E => {
            CharacterClass::Punctuation
        }
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => CharacterClass::Ideograph,
        0x0300..=0x036F | 0x1AB0..=0x1AFF => CharacterClass::Combining,
        // Mongolian script range
        0x1800..=0x18AF => CharacterClass::Letter,
        _ => CharacterClass::Letter,
    }
}

/// Helper to get the primary measure (width or height) of a shaped item.
pub fn get_item_measure(item: &ShapedItem, is_vertical: bool) -> f32 {
    match item {
        ShapedItem::Cluster(c) => {
            // Total width = base advance + kerning adjustments
            // Kerning is stored separately in glyphs for inspection, but the total
            // cluster width must include it for correct layout positioning
            let total_kerning: f32 = c.glyphs.iter().map(|g| g.kerning).sum();
            c.advance + total_kerning
        }
        ShapedItem::Object { bounds, .. }
        | ShapedItem::CombinedBlock { bounds, .. }
        | ShapedItem::Tab { bounds, .. } => {
            if is_vertical {
                bounds.height
            } else {
                bounds.width
            }
        }
        ShapedItem::Break { .. } => 0.0,
    }
}

/// Helper to get the final positioned bounds of an item.
fn get_item_bounds(item: &PositionedItem) -> Rect {
    let measure = get_item_measure(&item.item, false); // for simplicity, use horizontal
    let cross_measure = match &item.item {
        ShapedItem::Object { bounds, .. } => bounds.height,
        _ => 20.0, // placeholder line height
    };
    Rect {
        x: item.position.x,
        y: item.position.y,
        width: measure,
        height: cross_measure,
    }
}

/// Calculates the available horizontal segments for a line at a given vertical position,
/// considering both shape boundaries and exclusions.
fn get_line_constraints(
    line_y: f32,
    line_height: f32,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LineConstraints {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "\n--- Entering get_line_constraints for y={} ---",
            line_y
        )));
    }

    let mut available_segments = Vec::new();
    if constraints.shape_boundaries.is_empty() {
        // The segment_width is determined by available_width, NOT by TextWrap.
        // TextWrap::NoWrap only affects whether the LineBreaker can insert soft breaks,
        // it should NOT override a definite width constraint from CSS.
        // CSS Text Level 3: For 'white-space: pre/nowrap', text overflows horizontally
        // if it doesn't fit, rather than expanding the container.
        let segment_width = match constraints.available_width {
            AvailableSpace::Definite(w) => w, // Respect definite width from CSS
            AvailableSpace::MaxContent => f32::MAX / 2.0, // For intrinsic max-content sizing
            AvailableSpace::MinContent => 0.0, // For intrinsic min-content sizing
        };
        // Note: TextWrap::NoWrap is handled by the LineBreaker in break_one_line()
        // to prevent soft wraps. The text will simply overflow if it exceeds segment_width.
        available_segments.push(LineSegment {
            start_x: 0.0,
            width: segment_width,
            priority: 0,
        });
    } else {
        // ... complex boundary logic ...
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Initial available segments: {:?}",
            available_segments
        )));
    }

    for (idx, exclusion) in constraints.shape_exclusions.iter().enumerate() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Applying exclusion #{}: {:?}",
                idx, exclusion
            )));
        }
        let exclusion_spans =
            get_shape_horizontal_spans(exclusion, line_y, line_height).unwrap_or_default();
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Exclusion spans at y={}: {:?}",
                line_y, exclusion_spans
            )));
        }

        if exclusion_spans.is_empty() {
            continue;
        }

        let mut next_segments = Vec::new();
        for (excl_start, excl_end) in exclusion_spans {
            for segment in &available_segments {
                let seg_start = segment.start_x;
                let seg_end = segment.start_x + segment.width;

                // Create new segments by subtracting the exclusion
                if seg_end > excl_start && seg_start < excl_end {
                    if seg_start < excl_start {
                        // Left part
                        next_segments.push(LineSegment {
                            start_x: seg_start,
                            width: excl_start - seg_start,
                            priority: segment.priority,
                        });
                    }
                    if seg_end > excl_end {
                        // Right part
                        next_segments.push(LineSegment {
                            start_x: excl_end,
                            width: seg_end - excl_end,
                            priority: segment.priority,
                        });
                    }
                } else {
                    next_segments.push(segment.clone()); // No overlap
                }
            }
            available_segments = merge_segments(next_segments);
            next_segments = Vec::new();
        }
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Segments after exclusion #{}: {:?}",
                idx, available_segments
            )));
        }
    }

    let total_width = available_segments.iter().map(|s| s.width).sum();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Final segments: {:?}, total available width: {}",
            available_segments, total_width
        )));
        msgs.push(LayoutDebugMessage::info(
            "--- Exiting get_line_constraints ---".to_string(),
        ));
    }

    LineConstraints {
        segments: available_segments,
        total_available: total_width,
    }
}

/// Helper function to get the horizontal spans of any shape at a given y-coordinate.
/// Returns a list of (start_x, end_x) tuples.
fn get_shape_horizontal_spans(
    shape: &ShapeBoundary,
    y: f32,
    line_height: f32,
) -> Result<Vec<(f32, f32)>, LayoutError> {
    match shape {
        ShapeBoundary::Rectangle(rect) => {
            // Check for any overlap between the line box [y, y + line_height]
            // and the rectangle's vertical span [rect.y, rect.y + rect.height].
            let line_start = y;
            let line_end = y + line_height;
            let rect_start = rect.y;
            let rect_end = rect.y + rect.height;

            if line_start < rect_end && line_end > rect_start {
                Ok(vec![(rect.x, rect.x + rect.width)])
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Circle { center, radius } => {
            let line_center_y = y + line_height / 2.0;
            let dy = (line_center_y - center.y).abs();
            if dy <= *radius {
                let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                Ok(vec![(center.x - dx, center.x + dx)])
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Ellipse { center, radii } => {
            let line_center_y = y + line_height / 2.0;
            let dy = line_center_y - center.y;
            if dy.abs() <= radii.height {
                // Formula: (x-h)^2/a^2 + (y-k)^2/b^2 = 1
                let y_term = dy / radii.height;
                let x_term_squared = 1.0 - y_term.powi(2);
                if x_term_squared >= 0.0 {
                    let dx = radii.width * x_term_squared.sqrt();
                    Ok(vec![(center.x - dx, center.x + dx)])
                } else {
                    Ok(vec![])
                }
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Polygon { points } => {
            let segments = polygon_line_intersection(points, y, line_height)?;
            Ok(segments
                .iter()
                .map(|s| (s.start_x, s.start_x + s.width))
                .collect())
        }
        ShapeBoundary::Path { .. } => Ok(vec![]), // TODO!
    }
}

/// Merges overlapping or adjacent line segments into larger ones.
fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }
    segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap());
    let mut merged = vec![segments[0].clone()];
    for next_seg in segments.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if next_seg.start_x <= last.start_x + last.width {
            let new_width = (next_seg.start_x + next_seg.width) - last.start_x;
            last.width = last.width.max(new_width);
        } else {
            merged.push(next_seg.clone());
        }
    }
    merged
}

// TODO: Dummy polygon function to make it compile
fn polygon_line_intersection(
    points: &[Point],
    y: f32,
    line_height: f32,
) -> Result<Vec<LineSegment>, LayoutError> {
    if points.len() < 3 {
        return Ok(vec![]);
    }

    let line_center_y = y + line_height / 2.0;
    let mut intersections = Vec::new();

    // Use winding number algorithm for robustness with complex polygons.
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];

        // Skip horizontal edges as they don't intersect a horizontal scanline in a meaningful way.
        if (p2.y - p1.y).abs() < f32::EPSILON {
            continue;
        }

        // Check if our horizontal scanline at `line_center_y` crosses this polygon edge.
        let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
            || (p1.y > line_center_y && p2.y <= line_center_y);

        if crosses {
            // Calculate intersection x-coordinate using linear interpolation.
            let t = (line_center_y - p1.y) / (p2.y - p1.y);
            let x = p1.x + t * (p2.x - p1.x);
            intersections.push(x);
        }
    }

    // Sort intersections by x-coordinate to form spans.
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Build segments from paired intersection points.
    let mut segments = Vec::new();
    for chunk in intersections.chunks_exact(2) {
        let start_x = chunk[0];
        let end_x = chunk[1];
        if end_x > start_x {
            segments.push(LineSegment {
                start_x,
                width: end_x - start_x,
                priority: 0,
            });
        }
    }

    Ok(segments)
}

// ADDITION: A helper function to get a hyphenator.
/// Helper to get a hyphenator for a given language.
/// TODO: In a real app, this would be cached.
#[cfg(feature = "text_layout_hyphenation")]
fn get_hyphenator(language: HyphenationLanguage) -> Result<Standard, LayoutError> {
    Standard::from_embedded(language).map_err(|e| LayoutError::HyphenationError(e.to_string()))
}

/// Stub when hyphenation is disabled - always returns an error
#[cfg(not(feature = "text_layout_hyphenation"))]
fn get_hyphenator(_language: Language) -> Result<Standard, LayoutError> {
    Err(LayoutError::HyphenationError("Hyphenation feature not enabled".to_string()))
}

fn is_break_opportunity(item: &ShapedItem) -> bool {
    // Break after spaces or explicit break items.
    if is_word_separator(item) {
        return true;
    }
    if let ShapedItem::Break { .. } = item {
        return true;
    }
    // Also consider soft hyphens as opportunities.
    if let ShapedItem::Cluster(c) = item {
        if c.text.starts_with('\u{00AD}') {
            return true;
        }
    }
    false
}

// A cursor to manage the state of the line breaking process.
// This allows us to handle items that are partially consumed by hyphenation.
pub struct BreakCursor<'a> {
    /// A reference to the complete list of shaped items.
    pub items: &'a [ShapedItem],
    /// The index of the next *full* item to be processed from the `items` slice.
    pub next_item_index: usize,
    /// The remainder of an item that was split by hyphenation on the previous line.
    /// This will be the very first piece of content considered for the next line.
    pub partial_remainder: Vec<ShapedItem>,
}

impl<'a> BreakCursor<'a> {
    pub fn new(items: &'a [ShapedItem]) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
        }
    }

    /// Checks if the cursor is at the very beginning of the content stream.
    pub fn is_at_start(&self) -> bool {
        self.next_item_index == 0 && self.partial_remainder.is_empty()
    }

    /// Consumes the cursor and returns all remaining items as a `Vec`.
    pub fn drain_remaining(&mut self) -> Vec<ShapedItem> {
        let mut remaining = std::mem::take(&mut self.partial_remainder);
        if self.next_item_index < self.items.len() {
            remaining.extend_from_slice(&self.items[self.next_item_index..]);
        }
        self.next_item_index = self.items.len();
        remaining
    }

    /// Checks if all content, including any partial remainders, has been processed.
    pub fn is_done(&self) -> bool {
        self.next_item_index >= self.items.len() && self.partial_remainder.is_empty()
    }

    /// Consumes a number of items from the cursor's stream.
    pub fn consume(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        let remainder_len = self.partial_remainder.len();
        if count <= remainder_len {
            // Consuming only from the remainder.
            self.partial_remainder.drain(..count);
        } else {
            // Consuming all of the remainder and some from the main list.
            let from_main_list = count - remainder_len;
            self.partial_remainder.clear();
            self.next_item_index += from_main_list;
        }
    }

    /// Looks ahead and returns the next "unbreakable" unit of content.
    /// This is typically a word (a series of non-space clusters) followed by a
    /// space, or just a single space if that's next.
    pub fn peek_next_unit(&self) -> Vec<ShapedItem> {
        let mut unit = Vec::new();
        let mut source_items = self.partial_remainder.clone();
        source_items.extend_from_slice(&self.items[self.next_item_index..]);

        if source_items.is_empty() {
            return unit;
        }

        // If the first item is a break opportunity (like a space), it's a unit on its own.
        if is_break_opportunity(&source_items[0]) {
            unit.push(source_items[0].clone());
            return unit;
        }

        // Otherwise, collect all items until the next break opportunity.
        for item in source_items {
            if is_break_opportunity(&item) {
                break;
            }
            unit.push(item.clone());
        }
        unit
    }
}

// A structured result from a hyphenation attempt.
struct HyphenationResult {
    /// The items that fit on the current line, including the new hyphen.
    line_part: Vec<ShapedItem>,
    /// The remainder of the split item to be carried over to the next line.
    remainder_part: Vec<ShapedItem>,
}

fn perform_bidi_analysis<'a, 'b: 'a>(
    styled_runs: &'a [TextRunInfo],
    full_text: &'b str,
    force_lang: Option<Language>,
) -> Result<(Vec<VisualRun<'a>>, BidiDirection), LayoutError> {
    if full_text.is_empty() {
        return Ok((Vec::new(), BidiDirection::Ltr));
    }

    let bidi_info = BidiInfo::new(full_text, None);
    let para = &bidi_info.paragraphs[0];
    let base_direction = if para.level.is_rtl() {
        BidiDirection::Rtl
    } else {
        BidiDirection::Ltr
    };

    // Create a map from each byte index to its original styled run.
    let mut byte_to_run_index: Vec<usize> = vec![0; full_text.len()];
    for (run_idx, run) in styled_runs.iter().enumerate() {
        let start = run.logical_start;
        let end = start + run.text.len();
        for i in start..end {
            byte_to_run_index[i] = run_idx;
        }
    }

    let mut final_visual_runs = Vec::new();
    let (levels, visual_run_ranges) = bidi_info.visual_runs(para, para.range.clone());

    for range in visual_run_ranges {
        let bidi_level = levels[range.start];
        let mut sub_run_start = range.start;

        // Iterate through the bytes of the visual run to detect style changes.
        for i in (range.start + 1)..range.end {
            if byte_to_run_index[i] != byte_to_run_index[sub_run_start] {
                // Style boundary found. Finalize the previous sub-run.
                let original_run_idx = byte_to_run_index[sub_run_start];
                let script = crate::text3::script::detect_script(&full_text[sub_run_start..i])
                    .unwrap_or(Script::Latin);
                final_visual_runs.push(VisualRun {
                    text_slice: &full_text[sub_run_start..i],
                    style: styled_runs[original_run_idx].style.clone(),
                    logical_start_byte: sub_run_start,
                    bidi_level: BidiLevel::new(bidi_level.number()),
                    language: force_lang.unwrap_or_else(|| {
                        crate::text3::script::script_to_language(
                            script,
                            &full_text[sub_run_start..i],
                        )
                    }),
                    script,
                });
                // Start a new sub-run.
                sub_run_start = i;
            }
        }

        // Add the last sub-run (or the only one if no style change occurred).
        let original_run_idx = byte_to_run_index[sub_run_start];
        let script = crate::text3::script::detect_script(&full_text[sub_run_start..range.end])
            .unwrap_or(Script::Latin);

        final_visual_runs.push(VisualRun {
            text_slice: &full_text[sub_run_start..range.end],
            style: styled_runs[original_run_idx].style.clone(),
            logical_start_byte: sub_run_start,
            bidi_level: BidiLevel::new(bidi_level.number()),
            script,
            language: force_lang.unwrap_or_else(|| {
                crate::text3::script::script_to_language(
                    script,
                    &full_text[sub_run_start..range.end],
                )
            }),
        });
    }

    Ok((final_visual_runs, base_direction))
}

fn get_justification_priority(class: CharacterClass) -> u8 {
    match class {
        CharacterClass::Space => 0,
        CharacterClass::Punctuation => 64,
        CharacterClass::Ideograph => 128,
        CharacterClass::Letter => 192,
        CharacterClass::Symbol => 224,
        CharacterClass::Combining => 255,
    }
}

```

================================================================================
## FILE: layout/src/lib.rs
## Description: Layout lib root - module structure
================================================================================
```rust
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

pub mod font_traits;
#[cfg(feature = "image_decoding")]
pub mod image;
#[cfg(feature = "text_layout")]
pub mod managers;
#[cfg(feature = "text_layout")]
pub mod solver3;

// String formatting utilities
#[cfg(feature = "strfmt")]
pub mod fmt;
#[cfg(feature = "strfmt")]
pub use fmt::{FmtArg, FmtArgVec, FmtArgVecDestructor, FmtValue, fmt_string};

// Widget modules (behind feature flags)
#[cfg(feature = "widgets")]
pub mod widgets;

// Extra APIs (dialogs, file operations)
#[cfg(feature = "extra")]
pub mod desktop;
#[cfg(feature = "extra")]
pub mod extra;

// ICU4X internationalization support
#[cfg(feature = "icu")]
pub mod icu;
#[cfg(feature = "icu")]
pub use icu::{
    DateTimeFieldSet, FormatLength, IcuDate, IcuDateTime, IcuError,
    IcuLocalizer, IcuLocalizerHandle, IcuResult, IcuStringVec, IcuTime,
    LayoutCallbackInfoIcuExt, ListType, PluralCategory,
};

// Project Fluent localization support
#[cfg(feature = "fluent")]
pub mod fluent;
#[cfg(feature = "fluent")]
pub use fluent::{
    check_fluent_syntax, check_fluent_syntax_bytes, create_fluent_zip,
    create_fluent_zip_from_strings, export_to_zip, FluentError,
    FluentLanguageInfo, FluentLanguageInfoVec,
    FluentLocalizerHandle, FluentResult, FluentSyntaxCheckResult,
    FluentSyntaxError, FluentZipLoadResult, LayoutCallbackInfoFluentExt,
};

// URL parsing support
#[cfg(feature = "http")]
pub mod url;
#[cfg(feature = "http")]
pub use url::{Url, UrlParseError, ResultUrlUrlParseError};

// File system operations (C-compatible wrapper for std::fs)
pub mod file;
pub use file::{
    dir_create, dir_create_all, dir_list, dir_delete, dir_delete_all,
    file_copy, path_exists, file_metadata, file_read, file_delete, file_rename, file_write,
    path_is_dir, path_is_file, path_join, temp_dir,
    DirEntry, DirEntryVec, DirEntryVecDestructor, DirEntryVecDestructorType,
    FileError, FileErrorKind, FileMetadata, FilePath, OptionFilePath,
};

// HTTP client support
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "http")]
pub use http::{
    download_bytes, download_bytes_with_config, http_get,
    http_get_with_config, is_url_reachable, HttpError, HttpHeader,
    HttpRequestConfig, HttpResponse, HttpResponseTooLargeError, HttpResult,
    HttpStatusError,
};

// JSON parsing support
#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "json")]
pub use json::{
    json_parse, json_parse_bytes, json_stringify, json_stringify_pretty,
    Json, JsonInternal, JsonKeyValue, JsonKeyValueVec, JsonKeyValueVecDestructor, JsonKeyValueVecDestructorType,
    JsonParseError, JsonType, JsonVec,
    ResultJsonJsonParseError, OptionJson, OptionJsonVec, OptionJsonKeyValueVec,
};

// ZIP file manipulation support
#[cfg(feature = "zip_support")]
pub mod zip;
#[cfg(feature = "zip_support")]
pub use zip::{
    zip_create, zip_create_from_files, zip_extract_all, zip_list_contents,
    ZipFile, ZipFileEntry, ZipFileEntryVec, ZipPathEntry, ZipPathEntryVec,
    ZipReadConfig, ZipWriteConfig, ZipReadError, ZipWriteError,
};

// Icon provider support (always available)
pub mod icon;
pub use icon::{
    // Resolver
    default_icon_resolver,
    // Data types for RefAny
    ImageIconData, FontIconData,
    // Helpers
    create_default_icon_provider,
    register_material_icons,
    register_embedded_material_icons,
};
// Re-export core icon types
pub use azul_core::icon::{
    IconProviderHandle, IconResolverCallbackType,
    resolve_icons_in_styled_dom, OptionIconProviderHandle,
};

#[cfg(feature = "text_layout")]
pub mod callbacks;
#[cfg(feature = "cpurender")]
pub mod cpurender;
#[cfg(feature = "text_layout")]
pub mod default_actions;
#[cfg(feature = "text_layout")]
pub mod event_determination;
#[cfg(feature = "text_layout")]
pub mod font;
// Re-export allsorts types needed by printpdf
#[cfg(feature = "text_layout")]
pub use allsorts::subset::CmapTarget;
#[cfg(feature = "text_layout")]
pub use font::parsed::{
    FontParseWarning, FontParseWarningSeverity, FontType, OwnedGlyph, ParsedFont, PdfFontMetrics,
    SubsetFont,
};
// Re-export hyphenation for external crates (like printpdf)
#[cfg(feature = "text_layout_hyphenation")]
pub use hyphenation;
pub mod fragmentation;
#[cfg(feature = "text_layout")]
pub mod hit_test;
pub mod paged;
#[cfg(feature = "text_layout")]
pub mod text3;
#[cfg(feature = "text_layout")]
pub mod thread;
#[cfg(feature = "text_layout")]
pub mod timer;
#[cfg(feature = "text_layout")]
pub mod window;
#[cfg(feature = "text_layout")]
pub mod window_state;
#[cfg(feature = "xml")]
pub mod xml;

// Export the main layout function and window management
pub use fragmentation::{
    BoxBreakBehavior, BreakDecision, FragmentationDefaults, FragmentationLayoutContext,
    KeepTogetherPriority, PageCounter, PageFragment, PageMargins, PageNumberStyle, PageSlot,
    PageSlotContent, PageSlotPosition, PageTemplate,
};
#[cfg(feature = "text_layout")]
pub use hit_test::{CursorTypeHitTest, FullHitTest};
pub use paged::FragmentationState;
#[cfg(feature = "text_layout")]
pub use solver3::cache::LayoutCache as Solver3LayoutCache;
#[cfg(feature = "text_layout")]
pub use solver3::display_list::DisplayList as DisplayList3;
#[cfg(feature = "text_layout")]
pub use solver3::layout_document;
#[cfg(feature = "text_layout")]
pub use solver3::paged_layout::layout_document_paged;
#[cfg(feature = "text_layout")]
pub use solver3::{LayoutContext, LayoutError, Result as LayoutResult3};
#[cfg(feature = "text_layout")]
pub use text3::cache::{FontManager, LayoutCache as TextLayoutCache};
#[cfg(feature = "text_layout")]
pub use window::{CursorBlinkTimerAction, LayoutWindow, ScrollbarDragState};
#[cfg(feature = "text_layout")]
pub use managers::text_input::{PendingTextEdit, OptionPendingTextEdit};

// #[cfg(feature = "text_layout")]
// pub use solver::{callback_info_shape_text, do_the_layout, do_the_relayout};
#[cfg(feature = "text_layout")]
pub fn parse_font_fn(
    source: azul_core::resources::LoadedFontSource,
) -> Option<azul_css::props::basic::FontRef> {
    use core::ffi::c_void;

    use crate::font::parsed::ParsedFont;

    fn parsed_font_destructor(ptr: *mut c_void) {
        unsafe {
            let _ = Box::from_raw(ptr as *mut ParsedFont);
        }
    }

    ParsedFont::from_bytes(
        source.data.as_ref(),
        source.index as usize,
        &mut Vec::new(), // Ignore warnings for now
    )
    .map(|parsed_font| parsed_font_to_font_ref(parsed_font))
}

#[cfg(feature = "text_layout")]
pub fn parsed_font_to_font_ref(
    parsed_font: crate::font::parsed::ParsedFont,
) -> azul_css::props::basic::FontRef {
    use core::ffi::c_void;

    extern "C" fn parsed_font_destructor(ptr: *mut c_void) {
        unsafe {
            let _ = Box::from_raw(ptr as *mut crate::font::parsed::ParsedFont);
        }
    }

    let boxed = Box::new(parsed_font);
    let raw_ptr = Box::into_raw(boxed) as *const c_void;
    azul_css::props::basic::FontRef::new(raw_ptr, parsed_font_destructor)
}

#[cfg(feature = "text_layout")]
pub fn font_ref_to_parsed_font(
    font_ref: &azul_css::props::basic::FontRef,
) -> &crate::font::parsed::ParsedFont {
    unsafe { &*(font_ref.parsed as *const crate::font::parsed::ParsedFont) }
}

```

[FILE NOT FOUND: /Users/fschutt/Development/azul/layout/src/block_layout.rs]

[FILE NOT FOUND: /Users/fschutt/Development/azul/layout/src/inline_layout.rs]


---

## ANALYSIS REQUEST

Please analyze the above code and provide:

1. **Root Cause for Bug 2 (Line Wrapping):**
   - Where exactly is `UnifiedConstraints` instantiated with wrong `available_width`?
   - What should the fix be?

2. **Root Cause for Bugs 4-7 (Scrollbar Issues):**
   - Where is the `overflow: auto` visibility logic?
   - Where is the scrollbar geometry calculated?
   - Why is the border visually detached?

3. **Specific Code Fixes** with exact file paths and line numbers

4. **Verification Steps** to test each fix
