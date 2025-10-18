//! Pure scroll state management
//!
//! This module provides:
//! - Smooth scroll animations with easing
//! - Event source classification for scroll events

use alloc::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    events::{EasingFunction, EventSource},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    task::{Duration, Instant},
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
        if event.source == EventSource::Programmatic || event.source == EventSource::System {
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
