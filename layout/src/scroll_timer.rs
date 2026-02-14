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
//! Timer fires (every ~16ms):
//!   1. queue.take_all() — consume pending inputs
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

/// Velocity below this threshold is snapped to zero (pixels/second)
const VELOCITY_STOP_THRESHOLD: f32 = 0.5;

/// Friction coefficient for velocity decay (per-second multiplier).
/// At 60fps: effective per-frame = 0.95^1 ≈ 0.95. Lower = more friction.
const FRICTION_DECAY_RATE: f32 = 0.05; // velocity *= e^(-FRICTION_DECAY_RATE * dt * 60)

/// Multiplier for converting wheel deltas to velocity impulses
/// macOS delivers deltas in "points" which are already reasonable pixel values
const WHEEL_IMPULSE_MULTIPLIER: f32 = 1.0;

/// State stored in the timer's RefAny data.
///
/// Contains the shared input queue and per-node velocity state.
#[derive(Debug)]
pub struct ScrollPhysicsState {
    /// Shared input queue — same Arc as ScrollManager.scroll_input_queue
    pub input_queue: ScrollInputQueue,
    /// Per-node velocity tracking
    pub node_velocities: BTreeMap<(DomId, NodeId), NodeScrollPhysics>,
    /// Per-node "forced position" from trackpad or programmatic scroll
    pub pending_positions: BTreeMap<(DomId, NodeId), LogicalPosition>,
}

/// For convenience, re-export NodeId
use azul_core::id::NodeId;

/// Per-node scroll physics state
#[derive(Debug, Clone, Default)]
pub struct NodeScrollPhysics {
    /// Current velocity in pixels/second
    pub velocity: LogicalPosition,
}

impl ScrollPhysicsState {
    /// Create a new physics state with the shared input queue
    pub fn new(input_queue: ScrollInputQueue) -> Self {
        Self {
            input_queue,
            node_velocities: BTreeMap::new(),
            pending_positions: BTreeMap::new(),
        }
    }

    /// Returns true if any node has non-zero velocity or there are pending inputs
    pub fn is_active(&self) -> bool {
        self.input_queue.has_pending()
            || self.node_velocities.values().any(|v| {
                v.velocity.x.abs() > VELOCITY_STOP_THRESHOLD
                    || v.velocity.y.abs() > VELOCITY_STOP_THRESHOLD
            })
            || !self.pending_positions.is_empty()
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

    // Calculate dt from frame timing
    // TimerCallbackInfo has frame_start and call_count
    // Use a fixed 16ms dt (matching our 60Hz timer interval) for stability
    let dt = 1.0 / 60.0_f32;

    // 1. Drain pending scroll inputs from the shared queue
    let inputs = physics.input_queue.take_all();

    for input in inputs {
        let key = (input.dom_id, input.node_id);
        match input.source {
            ScrollInputSource::TrackpadContinuous => {
                // Trackpad: OS handles momentum. Apply delta directly as position change.
                // We read current offset from CallbackInfo (read-only) and compute new position.
                let current = timer_info
                    .get_scroll_node_info(input.dom_id, input.node_id)
                    .map(|info| info.current_offset)
                    .unwrap_or_default();

                let new_pos = LogicalPosition {
                    x: current.x + input.delta.x,
                    y: current.y + input.delta.y,
                };
                physics.pending_positions.insert(key, new_pos);

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
                node_physics.velocity.x += input.delta.x * WHEEL_IMPULSE_MULTIPLIER * 60.0;
                node_physics.velocity.y += input.delta.y * WHEEL_IMPULSE_MULTIPLIER * 60.0;
            }
            ScrollInputSource::Programmatic => {
                // Programmatic: Set position directly (already handled by scroll_to API)
                // This path is for ScrollManager.record_scroll_input() with Programmatic source
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
        }
    }

    // 2. Integrate velocity physics for wheel-based momentum
    let mut velocity_updates: Vec<((DomId, NodeId), LogicalPosition)> = Vec::new();

    for ((dom_id, node_id), node_physics) in physics.node_velocities.iter_mut() {
        // Skip if velocity is negligible
        if node_physics.velocity.x.abs() < VELOCITY_STOP_THRESHOLD
            && node_physics.velocity.y.abs() < VELOCITY_STOP_THRESHOLD
        {
            node_physics.velocity = LogicalPosition::zero();
            continue;
        }

        // Get current scroll info for clamping
        let info = match timer_info.get_scroll_node_info(*dom_id, *node_id) {
            Some(i) => i,
            None => continue,
        };

        // Apply velocity to position
        let displacement = LogicalPosition {
            x: node_physics.velocity.x * dt,
            y: node_physics.velocity.y * dt,
        };

        let new_pos = LogicalPosition {
            x: (info.current_offset.x + displacement.x)
                .max(0.0)
                .min(info.max_scroll_x),
            y: (info.current_offset.y + displacement.y)
                .max(0.0)
                .min(info.max_scroll_y),
        };

        // Apply exponential friction decay
        let decay = (-FRICTION_DECAY_RATE * dt * 60.0).exp();
        node_physics.velocity.x *= decay;
        node_physics.velocity.y *= decay;

        // Stop at edges (kill velocity if clamped)
        if new_pos.x <= 0.0 || new_pos.x >= info.max_scroll_x {
            node_physics.velocity.x = 0.0;
        }
        if new_pos.y <= 0.0 || new_pos.y >= info.max_scroll_y {
            node_physics.velocity.y = 0.0;
        }

        // Snap to zero if below threshold after decay
        if node_physics.velocity.x.abs() < VELOCITY_STOP_THRESHOLD {
            node_physics.velocity.x = 0.0;
        }
        if node_physics.velocity.y.abs() < VELOCITY_STOP_THRESHOLD {
            node_physics.velocity.y = 0.0;
        }

        velocity_updates.push(((*dom_id, *node_id), new_pos));
    }

    // Clean up nodes with zero velocity
    physics
        .node_velocities
        .retain(|_, v| v.velocity.x.abs() > 0.0 || v.velocity.y.abs() > 0.0);

    // 3. Push ScrollTo changes for all updated positions
    let mut any_changes = false;

    // Apply direct position changes (trackpad/programmatic)
    let direct_positions: Vec<_> = physics.pending_positions.iter().map(|(k, v)| (*k, *v)).collect();
    physics.pending_positions.clear();
    for ((dom_id, node_id), position) in direct_positions {
        // Clamp to valid bounds
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

    // Apply velocity-based position changes
    for ((dom_id, node_id), position) in velocity_updates {
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        timer_info.scroll_to(dom_id, hierarchy_id, position);
        any_changes = true;
    }

    // 4. Decide whether to continue or terminate
    if physics.is_active() || any_changes {
        TimerCallbackReturn {
            should_update: if any_changes {
                Update::RefreshDom
            } else {
                Update::DoNothing
            },
            should_terminate: TerminateTimer::Continue,
        }
    } else {
        // No more velocity, no pending inputs → terminate the timer
        TimerCallbackReturn::terminate_unchanged()
    }
}
