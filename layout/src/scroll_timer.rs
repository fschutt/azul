//! Scroll physics timer callback — the core of the timer-based scroll architecture.
//!
//! This module implements the scroll physics as a regular timer callback, using
//! the same transactional `push_change(CallbackChange::ScrollTo)` pattern as all
//! other state modifications. There is nothing special about the scroll timer —
//! it is a normal user-space timer that happens to be started by the framework.
//!
//! # Architecture
//!
//! ```text
//! Platform Event Handler
//!   → ScrollManager.record_scroll_input(ScrollInput)
//!   → starts SCROLL_MOMENTUM_TIMER if not running
//!
//! Timer fires (every timer_interval_ms from ScrollPhysics):
//!   1. queue.take_recent(100) — consume up to 100 most recent inputs
//!   2. For each input:
//!      - TrackpadContinuous → set offset directly (OS handles momentum)
//!      - WheelDiscrete → add impulse to velocity
//!      - Programmatic → set target position
//!   3. Integrate physics: velocity decay, clamping
//!   4. push_change(CallbackChange::ScrollTo) for each updated node
//!   5. Return continue_and_update() or terminate_unchanged()
//! ```
//!
//! # Key Design Decisions
//!
//! - **No mutable access to LayoutWindow needed**: Uses `CallbackChange::ScrollTo`
//!   (the same transactional pattern as all other callbacks).
//! - **Shared queue via Arc<Mutex>**: The `ScrollInputQueue` is cloned into the
//!   timer's `RefAny` data. Event handlers push, timer pops.
//! - **Platform-independent**: Works on macOS, Windows, Linux — anywhere timers work.
//! - **Self-terminating**: When all velocities are below threshold and no inputs
//!   pending, the timer returns `TerminateTimer::Terminate`.

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::{TimerCallbackReturn, Update},
    dom::{DomId, DomNodeId},
    geom::LogicalPosition,
    refany::RefAny,
    styled_dom::NodeHierarchyItemId,
    task::TerminateTimer,
};

use crate::{
    managers::scroll_state::{ScrollInput, ScrollInputQueue, ScrollInputSource, ScrollNodeInfo},
    timer::TimerCallbackInfo,
};

use azul_css::props::style::scrollbar::{ScrollPhysics, OverflowScrolling, OverscrollBehavior};

/// Maximum number of scroll events processed per timer tick.
/// Older events beyond this limit are discarded to keep the physics
/// simulation bounded and testable.
const MAX_SCROLL_EVENTS_PER_TICK: usize = 100;

/// State stored in the timer's RefAny data.
///
/// Contains the shared input queue, per-node velocity state, and the global
/// scroll physics configuration from `SystemStyle`.
#[derive(Debug)]
pub struct ScrollPhysicsState {
    /// Shared input queue — same Arc as ScrollManager.scroll_input_queue
    pub input_queue: ScrollInputQueue,
    /// Per-node velocity tracking
    pub node_velocities: BTreeMap<(DomId, NodeId), NodeScrollPhysics>,
    /// Per-node "forced position" from programmatic scroll (hard-clamped)
    pub pending_positions: BTreeMap<(DomId, NodeId), LogicalPosition>,
    /// Per-node "forced position" from trackpad scroll (rubber-band clamped)
    pub pending_trackpad_positions: BTreeMap<(DomId, NodeId), LogicalPosition>,
    /// Global scroll physics configuration (from SystemStyle)
    pub scroll_physics: ScrollPhysics,
}

/// For convenience, re-export NodeId
use azul_core::id::NodeId;

/// Per-node scroll physics state
#[derive(Debug, Clone, Default)]
pub struct NodeScrollPhysics {
    /// Current velocity in pixels/second
    pub velocity: LogicalPosition,
    /// Whether this node is currently in a rubber-band overshoot state
    pub is_rubber_banding: bool,
}

impl ScrollPhysicsState {
    /// Create a new physics state with the shared input queue and global config
    pub fn new(input_queue: ScrollInputQueue, scroll_physics: ScrollPhysics) -> Self {
        Self {
            input_queue,
            node_velocities: BTreeMap::new(),
            pending_positions: BTreeMap::new(),
            pending_trackpad_positions: BTreeMap::new(),
            scroll_physics,
        }
    }

    /// Returns true if any node has non-zero velocity or there are pending inputs
    pub fn is_active(&self) -> bool {
        let threshold = self.scroll_physics.min_velocity_threshold;
        self.input_queue.has_pending()
            || self.node_velocities.values().any(|v| {
                v.velocity.x.abs() > threshold
                    || v.velocity.y.abs() > threshold
                    || v.is_rubber_banding
            })
            || !self.pending_positions.is_empty()
            || !self.pending_trackpad_positions.is_empty()
    }
}

// Destructor for RefAny
fn scroll_physics_state_destructor(data: &mut RefAny) {
    // RefAny handles Drop automatically, nothing special needed
    let _ = data;
}

/// The scroll physics timer callback.
///
/// This is a normal timer callback registered with `SCROLL_MOMENTUM_TIMER_ID`.
/// It consumes pending scroll inputs, applies physics, and pushes ScrollTo changes.
///
/// Uses the `ScrollPhysics` configuration from `SystemStyle` for friction,
/// velocity thresholds, wheel multiplier, and rubber-banding parameters.
/// Per-node `OverflowScrolling` and `OverscrollBehavior` CSS properties are
/// respected to decide whether each node gets rubber-banding.
///
/// # C API
///
/// This function has `extern "C"` ABI so it can be used as a `TimerCallbackType`.
pub extern "C" fn scroll_physics_timer_callback(
    mut data: RefAny,
    mut timer_info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    // Downcast the RefAny to our physics state
    let mut physics = match data.downcast_mut::<ScrollPhysicsState>() {
        Some(p) => p,
        None => return TimerCallbackReturn::terminate_unchanged(),
    };

    // Extract physics config values
    let sp = &physics.scroll_physics;
    let dt = sp.timer_interval_ms.max(1) as f32 / 1000.0;
    let friction_rate = friction_from_deceleration(sp.deceleration_rate);
    let velocity_threshold = sp.min_velocity_threshold;
    let wheel_multiplier = sp.wheel_multiplier;
    let max_velocity = sp.max_velocity;
    let overscroll_elasticity = sp.overscroll_elasticity;
    let max_overscroll_distance = sp.max_overscroll_distance;
    let bounce_back_duration_ms = sp.bounce_back_duration_ms;

    // 1. Take at most MAX_SCROLL_EVENTS_PER_TICK recent inputs from the shared queue
    let inputs = physics.input_queue.take_recent(MAX_SCROLL_EVENTS_PER_TICK);

    for input in inputs {
        let key = (input.dom_id, input.node_id);
        match input.source {
            ScrollInputSource::TrackpadContinuous => {
                // Trackpad: OS handles momentum. Apply delta directly as position change.
                let current = timer_info
                    .get_scroll_node_info(input.dom_id, input.node_id)
                    .map(|info| info.current_offset)
                    .unwrap_or_default();

                let new_pos = LogicalPosition {
                    x: current.x + input.delta.x,
                    y: current.y + input.delta.y,
                };
                physics.pending_trackpad_positions.insert(key, new_pos);

                // Kill any existing velocity for this node (trackpad overrides momentum)
                physics.node_velocities.remove(&key);
            }
            ScrollInputSource::WheelDiscrete => {
                // Mouse wheel: Convert delta to velocity impulse
                let node_physics = physics
                    .node_velocities
                    .entry(key)
                    .or_insert_with(NodeScrollPhysics::default);

                // Add impulse (delta is in pixels, convert to pixels/second at ~60fps)
                node_physics.velocity.x += input.delta.x * wheel_multiplier * 60.0;
                node_physics.velocity.y += input.delta.y * wheel_multiplier * 60.0;

                // Clamp to max velocity
                node_physics.velocity.x = node_physics.velocity.x.clamp(-max_velocity, max_velocity);
                node_physics.velocity.y = node_physics.velocity.y.clamp(-max_velocity, max_velocity);
            }
            ScrollInputSource::Programmatic => {
                // Programmatic: Set position directly
                let current = timer_info
                    .get_scroll_node_info(input.dom_id, input.node_id)
                    .map(|info| info.current_offset)
                    .unwrap_or_default();

                let new_pos = LogicalPosition {
                    x: current.x + input.delta.x,
                    y: current.y + input.delta.y,
                };
                physics.pending_positions.insert(key, new_pos);
            }
            ScrollInputSource::TrackpadEnd => {
                // Trackpad gesture ended (fingers lifted).
                // If the scroll position is past the bounds (rubber-banding overshoot),
                // start a spring-back animation to snap back to the boundary.
                let pos = physics.pending_positions.remove(&key)
                    .or_else(|| timer_info.get_scroll_node_info(input.dom_id, input.node_id)
                        .map(|info| info.current_offset));

                if let Some(pos) = pos {
                    if let Some(info) = timer_info.get_scroll_node_info(input.dom_id, input.node_id) {
                        let overshoot_x = calculate_overshoot(pos.x, 0.0, info.max_scroll_x);
                        let overshoot_y = calculate_overshoot(pos.y, 0.0, info.max_scroll_y);

                        if overshoot_x.abs() > 0.01 || overshoot_y.abs() > 0.01 {
                            let node_phys = physics.node_velocities
                                .entry(key)
                                .or_insert_with(NodeScrollPhysics::default);
                            // Zero out velocity — the spring-back force in the
                            // velocity integration loop (step 2) will pull the
                            // position back to the boundary.
                            node_phys.velocity = LogicalPosition::zero();
                            node_phys.is_rubber_banding = true;
                        }

                        // Also set the current position for the spring-back start
                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(input.node_id));
                        timer_info.scroll_to(input.dom_id, hierarchy_id, pos);
                    }
                }
            }
        }
    }

    // 2. Integrate velocity physics for wheel-based momentum
    let mut velocity_updates: Vec<((DomId, NodeId), LogicalPosition)> = Vec::new();

    for ((dom_id, node_id), node_physics) in physics.node_velocities.iter_mut() {
        // Get current scroll info for clamping and per-node CSS config
        let info = match timer_info.get_scroll_node_info(*dom_id, *node_id) {
            Some(i) => i,
            None => continue,
        };

        // Determine if this node allows rubber-banding
        let rubber_band_x = node_allows_rubber_band_x(&info, overscroll_elasticity);
        let rubber_band_y = node_allows_rubber_band_y(&info, overscroll_elasticity);

        // Calculate current overshoot amounts
        let overshoot_x = calculate_overshoot(info.current_offset.x, 0.0, info.max_scroll_x);
        let overshoot_y = calculate_overshoot(info.current_offset.y, 0.0, info.max_scroll_y);

        let is_overshooting_x = overshoot_x.abs() > 0.01;
        let is_overshooting_y = overshoot_y.abs() > 0.01;

        // If we're in a rubber-band overshoot, apply spring-back force
        if is_overshooting_x && rubber_band_x {
            // Spring-back: accelerate towards the boundary
            let spring_k = spring_constant_from_bounce_duration(bounce_back_duration_ms);
            let spring_force_x = -spring_k * overshoot_x;
            node_physics.velocity.x += spring_force_x * dt;
            node_physics.is_rubber_banding = true;
        }
        if is_overshooting_y && rubber_band_y {
            let spring_k = spring_constant_from_bounce_duration(bounce_back_duration_ms);
            let spring_force_y = -spring_k * overshoot_y;
            node_physics.velocity.y += spring_force_y * dt;
            node_physics.is_rubber_banding = true;
        }

        // Skip if velocity is negligible and not rubber-banding
        if !node_physics.is_rubber_banding
            && node_physics.velocity.x.abs() < velocity_threshold
            && node_physics.velocity.y.abs() < velocity_threshold
        {
            node_physics.velocity = LogicalPosition::zero();
            continue;
        }

        // Apply velocity to position
        let displacement = LogicalPosition {
            x: node_physics.velocity.x * dt,
            y: node_physics.velocity.y * dt,
        };

        let raw_new_x = info.current_offset.x + displacement.x;
        let raw_new_y = info.current_offset.y + displacement.y;

        // Clamp with or without rubber-banding
        let new_x = if rubber_band_x && max_overscroll_distance > 0.0 {
            // Allow overshoot with diminishing returns (elasticity)
            rubber_band_clamp(raw_new_x, 0.0, info.max_scroll_x, max_overscroll_distance, overscroll_elasticity)
        } else {
            raw_new_x.max(0.0).min(info.max_scroll_x)
        };

        let new_y = if rubber_band_y && max_overscroll_distance > 0.0 {
            rubber_band_clamp(raw_new_y, 0.0, info.max_scroll_y, max_overscroll_distance, overscroll_elasticity)
        } else {
            raw_new_y.max(0.0).min(info.max_scroll_y)
        };

        let new_pos = LogicalPosition { x: new_x, y: new_y };

        // Apply exponential friction decay
        let decay = (-friction_rate * dt * 60.0).exp();
        node_physics.velocity.x *= decay;
        node_physics.velocity.y *= decay;

        // At edges without rubber-banding: kill velocity
        if !rubber_band_x {
            if new_pos.x <= 0.0 || new_pos.x >= info.max_scroll_x {
                node_physics.velocity.x = 0.0;
            }
        }
        if !rubber_band_y {
            if new_pos.y <= 0.0 || new_pos.y >= info.max_scroll_y {
                node_physics.velocity.y = 0.0;
            }
        }

        // Check if rubber-banding spring-back is almost complete
        let new_overshoot_x = calculate_overshoot(new_pos.x, 0.0, info.max_scroll_x);
        let new_overshoot_y = calculate_overshoot(new_pos.y, 0.0, info.max_scroll_y);
        if new_overshoot_x.abs() < 0.5 && new_overshoot_y.abs() < 0.5 {
            node_physics.is_rubber_banding = false;
        }

        // Snap to zero if below threshold after decay
        if node_physics.velocity.x.abs() < velocity_threshold {
            node_physics.velocity.x = 0.0;
        }
        if node_physics.velocity.y.abs() < velocity_threshold {
            node_physics.velocity.y = 0.0;
        }

        velocity_updates.push(((*dom_id, *node_id), new_pos));
    }

    // Clean up nodes with zero velocity and not rubber-banding
    physics
        .node_velocities
        .retain(|_, v| v.velocity.x.abs() > 0.0 || v.velocity.y.abs() > 0.0 || v.is_rubber_banding);

    // 3. Push ScrollTo changes for all updated positions
    let mut any_changes = false;

    // Apply programmatic position changes (hard-clamped to bounds)
    let direct_positions: Vec<_> = physics.pending_positions.iter().map(|(k, v)| (*k, *v)).collect();
    physics.pending_positions.clear();
    for ((dom_id, node_id), position) in direct_positions {
        let clamped = match timer_info.get_scroll_node_info(dom_id, node_id) {
            Some(info) => LogicalPosition {
                x: position.x.max(0.0).min(info.max_scroll_x),
                y: position.y.max(0.0).min(info.max_scroll_y),
            },
            None => position,
        };
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        timer_info.scroll_to(dom_id, hierarchy_id, clamped);
        any_changes = true;
    }

    // Apply trackpad position changes (rubber-band clamped for elastic overshoot)
    let trackpad_positions: Vec<_> = physics.pending_trackpad_positions.iter().map(|(k, v)| (*k, *v)).collect();
    physics.pending_trackpad_positions.clear();
    for ((dom_id, node_id), position) in trackpad_positions {
        let clamped = match timer_info.get_scroll_node_info(dom_id, node_id) {
            Some(info) => {
                let rubber_x = node_allows_rubber_band_x(&info, physics.scroll_physics.overscroll_elasticity);
                let rubber_y = node_allows_rubber_band_y(&info, physics.scroll_physics.overscroll_elasticity);
                let max_over = physics.scroll_physics.max_overscroll_distance;
                let elasticity = physics.scroll_physics.overscroll_elasticity;
                LogicalPosition {
                    x: if rubber_x {
                        rubber_band_clamp(position.x, 0.0, info.max_scroll_x, max_over, elasticity)
                    } else {
                        position.x.max(0.0).min(info.max_scroll_x)
                    },
                    y: if rubber_y {
                        rubber_band_clamp(position.y, 0.0, info.max_scroll_y, max_over, elasticity)
                    } else {
                        position.y.max(0.0).min(info.max_scroll_y)
                    },
                }
            },
            None => position,
        };
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        timer_info.scroll_to(dom_id, hierarchy_id, clamped);
        any_changes = true;
    }

    // Apply velocity-based position changes
    for ((dom_id, node_id), position) in velocity_updates {
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        timer_info.scroll_to(dom_id, hierarchy_id, position);
        any_changes = true;
    }

    // 4. Decide whether to continue or terminate
    if physics.is_active() || any_changes {
        TimerCallbackReturn {
            should_update: Update::DoNothing, // Scroll changes are handled via nodes_scrolled_in_callbacks, not DOM refresh
            should_terminate: TerminateTimer::Continue,
        }
    } else {
        // No more velocity, no pending inputs → terminate the timer
        TimerCallbackReturn::terminate_unchanged()
    }
}

// ============================================================================
// Rubber-banding Helper Functions
// ============================================================================

/// Determines if a node allows rubber-banding on the X axis based on:
/// 1. Per-node `overflow_scrolling` CSS property (-azul-overflow-scrolling)
/// 2. Per-node `overscroll_behavior_x` CSS property (overscroll-behavior-x)
/// 3. Global `overscroll_elasticity` from ScrollPhysics
fn node_allows_rubber_band_x(info: &ScrollNodeInfo, global_elasticity: f32) -> bool {
    // If overscroll-behavior-x is None, no rubber-band regardless
    if info.overscroll_behavior_x == OverscrollBehavior::None {
        return false;
    }
    // If -azul-overflow-scrolling: touch, force rubber-banding on
    if info.overflow_scrolling == OverflowScrolling::Touch {
        return true;
    }
    // Otherwise (Auto): use global config
    global_elasticity > 0.0
}

/// Determines if a node allows rubber-banding on the Y axis
fn node_allows_rubber_band_y(info: &ScrollNodeInfo, global_elasticity: f32) -> bool {
    if info.overscroll_behavior_y == OverscrollBehavior::None {
        return false;
    }
    if info.overflow_scrolling == OverflowScrolling::Touch {
        return true;
    }
    global_elasticity > 0.0
}

/// Calculate how far a position has overshot the valid scroll range.
/// Returns positive for overshoot past max, negative for overshoot past min, 0 if in range.
fn calculate_overshoot(pos: f32, min: f32, max: f32) -> f32 {
    if pos < min {
        pos - min // negative
    } else if pos > max {
        pos - max // positive
    } else {
        0.0
    }
}

/// Rubber-band clamping: allows overshoot up to `max_overscroll`, with
/// diminishing returns (elasticity) so it feels "springy".
///
/// The further you overshoot, the harder it becomes to scroll further.
fn rubber_band_clamp(
    raw_pos: f32,
    min: f32,
    max: f32,
    max_overscroll: f32,
    elasticity: f32,
) -> f32 {
    if raw_pos >= min && raw_pos <= max {
        return raw_pos;
    }

    let (boundary, overshoot) = if raw_pos < min {
        (min, min - raw_pos) // overshoot is positive distance past boundary
    } else {
        (max, raw_pos - max)
    };

    // Diminishing returns: as overshoot increases, actual displacement decreases
    // Formula: actual = max_overscroll * (1 - e^(-elasticity * overshoot / max_overscroll))
    let clamped_overscroll = if max_overscroll > 0.0 {
        max_overscroll * (1.0 - (-elasticity * overshoot / max_overscroll).exp())
    } else {
        0.0
    };

    if raw_pos < min {
        boundary - clamped_overscroll
    } else {
        boundary + clamped_overscroll
    }
}

/// Convert deceleration_rate (0.0 - 1.0) to a friction constant for exponential decay.
/// Higher deceleration_rate = less friction (slower deceleration).
fn friction_from_deceleration(deceleration_rate: f32) -> f32 {
    // deceleration_rate ~0.95 (fast) → friction ~0.05
    // deceleration_rate ~0.998 (iOS-like) → friction ~0.002
    (1.0 - deceleration_rate.clamp(0.0, 0.999)).max(0.001)
}

/// Calculate spring constant from bounce-back duration.
/// Higher k = faster spring back. Approximate: k ≈ (2π / duration)²
fn spring_constant_from_bounce_duration(duration_ms: u32) -> f32 {
    let duration_s = duration_ms.max(50) as f32 / 1000.0;
    let omega = core::f32::consts::TAU / duration_s;
    omega * omega
}
