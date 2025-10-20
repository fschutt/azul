//! Pure scroll state management
//!
//! This module provides:
//! - Smooth scroll animations with easing
//! - Event source classification for scroll events
//! - Scrollbar geometry and hit-testing
//! - ExternalScrollId mapping for WebRender integration

use alloc::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    events::{EasingFunction, EventSource},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{ExternalScrollId, ScrollPosition},
    task::{Duration, Instant},
};

// ============================================================================
// Scrollbar Component Types
// ============================================================================

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

/// Orientation of a scrollbar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScrollbarOrientation {
    Vertical,
    Horizontal,
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

// ============================================================================
// Core Scroll Manager
// ============================================================================

/// Manages all scroll state and animations for a window
#[derive(Debug, Clone, Default)]
pub struct ScrollManager {
    /// Maps (DomId, NodeId) to their scroll state
    states: BTreeMap<(DomId, NodeId), ScrollState>,
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

/// Information about what happened during a frame
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameScrollInfo {
    pub had_scroll_activity: bool,
    pub had_programmatic_scroll: bool,
    pub had_new_doms: bool,
}

/// Scroll event to be processed, now with source tracking
#[derive(Debug, Clone)]
pub struct ScrollEvent {
    pub dom_id: DomId,
    pub node_id: NodeId,
    pub delta: LogicalPosition,
    pub source: EventSource,
    pub duration: Option<Duration>,
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

// ============================================================================
// ScrollManager Implementation
// ============================================================================

impl ScrollManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_frame(&mut self) {
        self.had_scroll_activity = false;
        self.had_programmatic_scroll = false;
        self.had_new_doms = false;
    }

    pub fn end_frame(&self) -> FrameScrollInfo {
        FrameScrollInfo {
            had_scroll_activity: self.had_scroll_activity,
            had_programmatic_scroll: self.had_programmatic_scroll,
            had_new_doms: self.had_new_doms,
        }
    }

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
    }

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
            .or_insert_with(|| ScrollState::new(now));
        state.container_rect = container_rect;
        state.content_rect = content_rect;
        state.current_offset = state.clamp(state.current_offset);
    }

    pub fn get_current_offset(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalPosition> {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.current_offset)
    }

    pub fn get_last_activity_time(&self, dom_id: DomId, node_id: NodeId) -> Option<Instant> {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.last_activity.clone())
    }

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

    // ========================================================================
    // ExternalScrollId Management
    // ========================================================================

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

    // ========================================================================
    // Scrollbar State Management
    // ========================================================================

    /// Calculate scrollbar states for all visible scrollbars.
    /// This should be called once per frame after layout is complete.
    pub fn calculate_scrollbar_states(&mut self) {
        self.scrollbar_states.clear();

        for ((dom_id, node_id), scroll_state) in self.states.iter() {
            // Check if vertical scrollbar is needed
            let needs_vertical =
                scroll_state.content_rect.size.height > scroll_state.container_rect.size.height;
            if needs_vertical {
                let v_state = self.calculate_vertical_scrollbar(*dom_id, *node_id, scroll_state);
                self.scrollbar_states
                    .insert((*dom_id, *node_id, ScrollbarOrientation::Vertical), v_state);
            }

            // Check if horizontal scrollbar is needed
            let needs_horizontal =
                scroll_state.content_rect.size.width > scroll_state.container_rect.size.width;
            if needs_horizontal {
                let h_state = self.calculate_horizontal_scrollbar(*dom_id, *node_id, scroll_state);
                self.scrollbar_states.insert(
                    (*dom_id, *node_id, ScrollbarOrientation::Horizontal),
                    h_state,
                );
            }
        }
    }

    /// Calculate vertical scrollbar geometry
    fn calculate_vertical_scrollbar(
        &self,
        _dom_id: DomId,
        _node_id: NodeId,
        scroll_state: &ScrollState,
    ) -> ScrollbarState {
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
        let track_rect = LogicalRect::new(
            LogicalPosition::new(
                scroll_state.container_rect.origin.x + scroll_state.container_rect.size.width
                    - SCROLLBAR_WIDTH,
                scroll_state.container_rect.origin.y,
            ),
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
    fn calculate_horizontal_scrollbar(
        &self,
        _dom_id: DomId,
        _node_id: NodeId,
        scroll_state: &ScrollState,
    ) -> ScrollbarState {
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

        let track_rect = LogicalRect::new(
            LogicalPosition::new(
                scroll_state.container_rect.origin.x,
                scroll_state.container_rect.origin.y + scroll_state.container_rect.size.height
                    - SCROLLBAR_HEIGHT,
            ),
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
}

// ============================================================================
// ScrollState Implementation
// ============================================================================

impl ScrollState {
    pub fn new(now: Instant) -> Self {
        Self {
            current_offset: LogicalPosition::zero(),
            animation: None,
            last_activity: now,
            container_rect: LogicalRect::zero(),
            content_rect: LogicalRect::zero(),
        }
    }

    pub fn clamp(&self, position: LogicalPosition) -> LogicalPosition {
        let max_x = (self.content_rect.size.width - self.container_rect.size.width).max(0.0);
        let max_y = (self.content_rect.size.height - self.container_rect.size.height).max(0.0);
        LogicalPosition {
            x: position.x.max(0.0).min(max_x),
            y: position.y.max(0.0).min(max_y),
        }
    }
}

// ============================================================================
// Easing Functions
// ============================================================================

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
