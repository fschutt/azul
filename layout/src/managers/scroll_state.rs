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
                if self.is_node_scrollable(*dom_id, *node_id) {
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
            self.states.insert(key, AnimatedScrollState {
                current_offset: LogicalPosition::zero(),
                previous_offset: LogicalPosition::zero(),
                animation: None,
                last_activity: now,
                container_rect,
                content_rect,
            });
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
