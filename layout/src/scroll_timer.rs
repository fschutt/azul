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
    dom::DomId,
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

/// Assumed framerate for converting between per-frame and per-second quantities.
/// Used both in wheel impulse conversion and friction decay so the two stay coupled.
const ASSUMED_FPS: f32 = 60.0;

/// State stored in the timer's `RefAny` data.
///
/// Contains the shared input queue, per-node velocity state, and the global
/// scroll physics configuration from `SystemStyle`.
#[derive(Debug)]
pub struct ScrollPhysicsState {
    /// Shared input queue — same Arc as `ScrollManager.scroll_input_queue`
    pub input_queue: ScrollInputQueue,
    /// Per-node velocity tracking
    pub node_velocities: BTreeMap<(DomId, NodeId), NodeScrollPhysics>,
    /// Per-node "forced position" from programmatic scroll (hard-clamped)
    pub pending_positions: BTreeMap<(DomId, NodeId), LogicalPosition>,
    /// Per-node "forced position" from trackpad scroll (rubber-band clamped)
    pub pending_trackpad_positions: BTreeMap<(DomId, NodeId), LogicalPosition>,
    /// Global scroll physics configuration (from `SystemStyle`)
    pub scroll_physics: ScrollPhysics,
}

/// For convenience, re-export `NodeId`
use azul_core::id::NodeId;

/// Per-node scroll physics state
#[derive(Copy, Debug, Clone, Default)]
pub struct NodeScrollPhysics {
    /// Current velocity in pixels/second
    pub velocity: LogicalPosition,
    /// Whether this node is currently in a rubber-band overshoot state
    pub is_rubber_banding: bool,
}

impl ScrollPhysicsState {
    /// Create a new physics state with the shared input queue and global config
    #[must_use] pub const fn new(input_queue: ScrollInputQueue, scroll_physics: ScrollPhysics) -> Self {
        Self {
            input_queue,
            node_velocities: BTreeMap::new(),
            pending_positions: BTreeMap::new(),
            pending_trackpad_positions: BTreeMap::new(),
            scroll_physics,
        }
    }

    /// Returns true if any node has non-zero velocity or there are pending inputs
    fn is_active(&self) -> bool {
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

/// The scroll physics timer callback.
///
/// This is a normal timer callback registered with `SCROLL_MOMENTUM_TIMER_ID`.
/// It consumes pending scroll inputs, applies physics, and pushes `ScrollTo` changes.
///
/// Uses the `ScrollPhysics` configuration from `SystemStyle` for friction,
/// velocity thresholds, wheel multiplier, and rubber-banding parameters.
/// Per-node `OverflowScrolling` and `OverscrollBehavior` CSS properties are
/// respected to decide whether each node gets rubber-banding.
///
/// # C API
///
/// This function has `extern "C"` ABI so it can be used as a `TimerCallbackType`.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub extern "C" fn scroll_physics_timer_callback(
    mut data: RefAny,
    mut timer_info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    // Downcast the RefAny to our physics state
    let Some(mut physics) = data.downcast_mut::<ScrollPhysicsState>() else {
        return TimerCallbackReturn::terminate_unchanged();
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

                // Add impulse (delta is in pixels, convert to pixels/second)
                node_physics.velocity.x += input.delta.x * wheel_multiplier * ASSUMED_FPS;
                node_physics.velocity.y += input.delta.y * wheel_multiplier * ASSUMED_FPS;

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

                        // Preserve the overshot position for the spring-back animation.
                        // Must use unclamped so the overshot position is NOT clamped to bounds.
                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(input.node_id));
                        timer_info.scroll_to_unclamped(input.dom_id, hierarchy_id, pos);
                    }
                }
            }
        }
    }

    // 2. Integrate velocity physics for wheel-based momentum
    let mut velocity_updates: Vec<((DomId, NodeId), LogicalPosition)> = Vec::new();
    // Residual momentum from nodes that hit their boundary this tick, to be
    // transferred up the scroll chain after the iteration (can't mutate
    // node_velocities mid-loop).
    let mut momentum_handoffs: Vec<((DomId, NodeId), LogicalPosition)> = Vec::new();

    for ((dom_id, node_id), node_physics) in &mut physics.node_velocities {
        // Get current scroll info for clamping and per-node CSS config
        let Some(info) = timer_info.get_scroll_node_info(*dom_id, *node_id) else {
            continue;
        };

        // Determine if this node allows rubber-banding
        let rubber_band_x = node_allows_rubber_band(info.max_scroll_x, info.overscroll_behavior_x, info.overflow_scrolling, overscroll_elasticity);
        let rubber_band_y = node_allows_rubber_band(info.max_scroll_y, info.overscroll_behavior_y, info.overflow_scrolling, overscroll_elasticity);

        // Calculate current overshoot amounts
        let overshoot_x = calculate_overshoot(info.current_offset.x, 0.0, info.max_scroll_x);
        let overshoot_y = calculate_overshoot(info.current_offset.y, 0.0, info.max_scroll_y);

        let is_overshooting_x = overshoot_x.abs() > 0.01;
        let is_overshooting_y = overshoot_y.abs() > 0.01;

        // If we're in a rubber-band overshoot, apply critically-damped spring force.
        // F = -k*x - c*v  where c = 2*sqrt(k) for critical damping (no oscillation).
        if is_overshooting_x && rubber_band_x {
            let spring_k = spring_constant_from_bounce_duration(bounce_back_duration_ms);
            let damping = 2.0 * spring_k.sqrt(); // critical damping coefficient
            let spring_force_x = -spring_k * overshoot_x - damping * node_physics.velocity.x;
            node_physics.velocity.x += spring_force_x * dt;
            node_physics.is_rubber_banding = true;
        }
        if is_overshooting_y && rubber_band_y {
            let spring_k = spring_constant_from_bounce_duration(bounce_back_duration_ms);
            let damping = 2.0 * spring_k.sqrt(); // critical damping coefficient
            let spring_force_y = -spring_k * overshoot_y - damping * node_physics.velocity.y;
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
            raw_new_x.clamp(0.0, info.max_scroll_x)
        };

        let new_y = if rubber_band_y && max_overscroll_distance > 0.0 {
            rubber_band_clamp(raw_new_y, 0.0, info.max_scroll_y, max_overscroll_distance, overscroll_elasticity)
        } else {
            raw_new_y.clamp(0.0, info.max_scroll_y)
        };

        let new_pos = LogicalPosition { x: new_x, y: new_y };

        // Apply exponential friction decay
        let decay = (-friction_rate * dt * ASSUMED_FPS).exp();
        node_physics.velocity.x *= decay;
        node_physics.velocity.y *= decay;

        // At edges without rubber-banding: hand the remaining momentum to a
        // scrollable ancestor, then kill this node's velocity (MWA-C-scroll:
        // a fling that exhausts the inner container mid-momentum continues
        // on the outer one, mirroring the input-time boundary handoff in
        // select_scroll_target). overscroll-behavior contain/none on this
        // node stops the chain, matching CSS scroll-chaining semantics.
        if !rubber_band_x && (new_pos.x <= 0.0 || new_pos.x >= info.max_scroll_x) {
            let into_edge = (new_pos.x <= 0.0 && node_physics.velocity.x < 0.0)
                || (new_pos.x >= info.max_scroll_x && node_physics.velocity.x > 0.0);
            if into_edge
                && info.overscroll_behavior_x == OverscrollBehavior::Auto
                && node_physics.velocity.x.abs() > velocity_threshold
            {
                momentum_handoffs.push((
                    (*dom_id, *node_id),
                    LogicalPosition { x: node_physics.velocity.x, y: 0.0 },
                ));
            }
            node_physics.velocity.x = 0.0;
        }
        if !rubber_band_y && (new_pos.y <= 0.0 || new_pos.y >= info.max_scroll_y) {
            let into_edge = (new_pos.y <= 0.0 && node_physics.velocity.y < 0.0)
                || (new_pos.y >= info.max_scroll_y && node_physics.velocity.y > 0.0);
            if into_edge
                && info.overscroll_behavior_y == OverscrollBehavior::Auto
                && node_physics.velocity.y.abs() > velocity_threshold
            {
                momentum_handoffs.push((
                    (*dom_id, *node_id),
                    LogicalPosition { x: 0.0, y: node_physics.velocity.y },
                ));
            }
            node_physics.velocity.y = 0.0;
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

    // MWA-C-scroll: transfer residual momentum up the scroll chain — walk the
    // scroll-parent chain to the nearest ancestor that can still consume in
    // the fling's direction and seed it with the leftover velocity (picked up
    // by the integration loop on the next tick; is_active() keeps the timer
    // alive because the entry lands in node_velocities).
    for ((dom_id, node_id), vel) in momentum_handoffs {
        let mut cur = node_id;
        for _ in 0..64 {
            let Some(parent) = timer_info.find_scroll_parent(dom_id, cur) else {
                break;
            };
            let Some(pinfo) = timer_info.get_scroll_node_info(dom_id, parent) else {
                break;
            };
            let can_x = vel.x != 0.0
                && ((vel.x > 0.0 && pinfo.current_offset.x < pinfo.max_scroll_x - 0.5)
                    || (vel.x < 0.0 && pinfo.current_offset.x > 0.5));
            let can_y = vel.y != 0.0
                && ((vel.y > 0.0 && pinfo.current_offset.y < pinfo.max_scroll_y - 0.5)
                    || (vel.y < 0.0 && pinfo.current_offset.y > 0.5));
            if can_x || can_y {
                let entry = physics
                    .node_velocities
                    .entry((dom_id, parent))
                    .or_insert_with(NodeScrollPhysics::default);
                if can_x {
                    entry.velocity.x += vel.x;
                }
                if can_y {
                    entry.velocity.y += vel.y;
                }
                break;
            }
            // This ancestor is itself exhausted in the fling's direction —
            // respect ITS overscroll-behavior before chaining past it.
            let stop_x = vel.x != 0.0 && pinfo.overscroll_behavior_x != OverscrollBehavior::Auto;
            let stop_y = vel.y != 0.0 && pinfo.overscroll_behavior_y != OverscrollBehavior::Auto;
            if stop_x || stop_y {
                break;
            }
            cur = parent;
        }
    }

    // 3. Push ScrollTo changes for all updated positions
    let mut any_changes = false;

    // Apply programmatic position changes (hard-clamped to bounds)
    let direct_positions: Vec<_> = physics.pending_positions.iter().map(|(k, v)| (*k, *v)).collect();
    physics.pending_positions.clear();
    for ((dom_id, node_id), position) in direct_positions {
        let clamped = timer_info.get_scroll_node_info(dom_id, node_id).map_or(position, |info| LogicalPosition {
                x: position.x.clamp(0.0, info.max_scroll_x),
                y: position.y.clamp(0.0, info.max_scroll_y),
            });
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        timer_info.scroll_to(dom_id, hierarchy_id, clamped);
        any_changes = true;
    }

    // Apply trackpad position changes (rubber-band clamped for elastic overshoot)
    // Uses scroll_to_unclamped because the physics timer does its own rubber-band clamping.
    let trackpad_positions: Vec<_> = physics.pending_trackpad_positions.iter().map(|(k, v)| (*k, *v)).collect();
    physics.pending_trackpad_positions.clear();
    for ((dom_id, node_id), position) in trackpad_positions {
        let clamped = timer_info.get_scroll_node_info(dom_id, node_id).map_or(position, |info| {
                let rubber_x = node_allows_rubber_band(info.max_scroll_x, info.overscroll_behavior_x, info.overflow_scrolling, physics.scroll_physics.overscroll_elasticity);
                let rubber_y = node_allows_rubber_band(info.max_scroll_y, info.overscroll_behavior_y, info.overflow_scrolling, physics.scroll_physics.overscroll_elasticity);
                let max_over = physics.scroll_physics.max_overscroll_distance;
                let elasticity = physics.scroll_physics.overscroll_elasticity;
                LogicalPosition {
                    x: if rubber_x {
                        rubber_band_clamp(position.x, 0.0, info.max_scroll_x, max_over, elasticity)
                    } else {
                        position.x.clamp(0.0, info.max_scroll_x)
                    },
                    y: if rubber_y {
                        rubber_band_clamp(position.y, 0.0, info.max_scroll_y, max_over, elasticity)
                    } else {
                        position.y.clamp(0.0, info.max_scroll_y)
                    },
                }
            });
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        timer_info.scroll_to_unclamped(dom_id, hierarchy_id, clamped);
        any_changes = true;
    }

    // Apply velocity-based position changes (uses unclamped: physics already handles rubber-band clamping)
    for ((dom_id, node_id), position) in velocity_updates {
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        timer_info.scroll_to_unclamped(dom_id, hierarchy_id, position);
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

/// Determines if a node allows rubber-banding on a given axis based on:
/// 1. Whether the axis actually has overflow (`max_scroll` > 0)
/// 2. Per-node `overflow_scrolling` CSS property (-azul-overflow-scrolling)
/// 3. Per-node `overscroll_behavior` CSS property (overscroll-behavior-x/y)
/// 4. Global `overscroll_elasticity` from `ScrollPhysics`
fn node_allows_rubber_band(
    max_scroll: f32,
    overscroll_behavior: OverscrollBehavior,
    overflow_scrolling: OverflowScrolling,
    global_elasticity: f32,
) -> bool {
    if max_scroll <= 0.0 {
        return false;
    }
    if overscroll_behavior == OverscrollBehavior::None {
        return false;
    }
    if overflow_scrolling == OverflowScrolling::Touch {
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

/// Convert `deceleration_rate` (0.0 - 1.0) to a friction constant for exponential decay.
/// Higher `deceleration_rate` = less friction (slower deceleration).
fn friction_from_deceleration(deceleration_rate: f32) -> f32 {
    // deceleration_rate ~0.95 (fast) → friction ~0.05
    // deceleration_rate ~0.998 (iOS-like) → friction ~0.002
    (1.0 - deceleration_rate.clamp(0.0, 0.999)).max(0.001)
}

/// Calculate spring constant from bounce-back duration.
/// Higher k = faster spring back. Approximate: k ≈ (2π / duration)²
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn spring_constant_from_bounce_duration(duration_ms: u32) -> f32 {
    let duration_s = duration_ms.max(50) as f32 / 1000.0;
    let omega = core::f32::consts::TAU / duration_s;
    omega * omega
}

// ============================================================================
// Generated adversarial tests
// ============================================================================

#[cfg(all(test, feature = "std"))]
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::unreadable_literal
)]
mod autotest_generated {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomNodeId, OptionDomNodeId},
        geom::{LogicalRect, LogicalSize, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        task::Instant,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Harness
    // ------------------------------------------------------------------

    /// A live callback environment: an otherwise-empty `LayoutWindow` (optionally
    /// carrying registered scroll nodes) plus the shared change log that
    /// `scroll_to` / `scroll_to_unclamped` push into. `tick()` runs one full
    /// timer callback against it, so the physics loop can be driven repeatedly.
    struct Env<'a> {
        ref_data: &'a CallbackInfoRefData<'a>,
        changes: &'a Arc<Mutex<Vec<CallbackChange>>>,
    }

    impl Env<'_> {
        /// Run one `scroll_physics_timer_callback` tick against this environment.
        fn tick(&mut self, data: &RefAny) -> TimerCallbackReturn {
            let info = CallbackInfo::new(
                self.ref_data,
                self.changes,
                DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::NONE,
                },
                OptionLogicalPosition::None,
                OptionLogicalPosition::None,
            );
            let timer_info =
                TimerCallbackInfo::create(info, OptionDomNodeId::None, Instant::now(), 0, false);
            scroll_physics_timer_callback(data.clone(), timer_info)
        }

        /// Drain the `CallbackChange`s pushed so far.
        fn take_changes(&self) -> Vec<CallbackChange> {
            self.changes
                .lock()
                .map(|mut c| core::mem::take(&mut *c))
                .unwrap_or_default()
        }

        /// Drain the change log, asserting every entry is a `ScrollTo`, and
        /// return `(node index, position, unclamped)` for each.
        fn take_scroll_tos(&self) -> Vec<(usize, LogicalPosition, bool)> {
            self.take_changes()
                .iter()
                .map(|change| {
                    let CallbackChange::ScrollTo {
                        node_id,
                        position,
                        unclamped,
                        ..
                    } = change
                    else {
                        panic!("expected only ScrollTo changes, got {change:?}");
                    };
                    let idx = node_id
                        .into_crate_internal()
                        .expect("ScrollTo must name a concrete node")
                        .index();
                    (idx, *position, *unclamped)
                })
                .collect()
        }
    }

    /// Builds a callback environment. `setup` may register scroll nodes on the
    /// `LayoutWindow` before it is frozen behind the shared reference.
    fn with_env<R>(setup: impl FnOnce(&mut LayoutWindow), f: impl FnOnce(&mut Env<'_>) -> R) -> R {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        setup(&mut layout_window);

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let mut env = Env {
            ref_data: &ref_data,
            changes: &changes,
        };
        f(&mut env)
    }

    /// Registers node `idx` of the root DOM as a scrollable node with a
    /// `container_w x container_h` viewport over `content_w x content_h` content
    /// (so `max_scroll_x = content_w - container_w`, clamped at 0).
    fn register_node(
        window: &mut LayoutWindow,
        idx: usize,
        container: (f32, f32),
        content: (f32, f32),
    ) {
        window.scroll_manager.register_or_update_scroll_node(
            DomId::ROOT_ID,
            NodeId::new(idx),
            LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(container.0, container.1),
            ),
            LogicalSize::new(content.0, content.1),
            Instant::now(),
            0.0,
            0.0,
            false,
            false,
        );
    }

    /// A scroll input for node `idx` of the root DOM.
    fn input(idx: usize, delta: (f32, f32), source: ScrollInputSource) -> ScrollInput {
        ScrollInput {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(idx),
            delta: LogicalPosition::new(delta.0, delta.1),
            timestamp: Instant::now(),
            source,
        }
    }

    /// A `ScrollPhysicsState` wrapped in a `RefAny`, plus the queue that feeds it.
    fn state_with(physics: ScrollPhysics) -> (RefAny, ScrollInputQueue) {
        let queue = ScrollInputQueue::new();
        let state = ScrollPhysicsState::new(queue.clone(), physics);
        (RefAny::new(state), queue)
    }

    fn key(idx: usize) -> (DomId, NodeId) {
        (DomId::ROOT_ID, NodeId::new(idx))
    }

    /// Reads the physics state back out of the `RefAny` after a tick.
    fn with_state<R>(data: &mut RefAny, f: impl FnOnce(&ScrollPhysicsState) -> R) -> R {
        let state = data
            .downcast_ref::<ScrollPhysicsState>()
            .expect("RefAny must still hold a ScrollPhysicsState");
        f(&state)
    }

    /// A `ScrollPhysics` whose every float field is `NaN` and every integer field
    /// is degenerate — except `max_velocity`, which must stay non-NaN and
    /// non-negative or `f32::clamp` panics (see the `known_hazard` tests below).
    fn nan_physics() -> ScrollPhysics {
        ScrollPhysics {
            smooth_scroll_duration_ms: 0,
            deceleration_rate: f32::NAN,
            min_velocity_threshold: f32::NAN,
            max_velocity: 0.0,
            wheel_multiplier: f32::NAN,
            invert_direction: false,
            overscroll_elasticity: f32::NAN,
            max_overscroll_distance: f32::NAN,
            bounce_back_duration_ms: 0,
            timer_interval_ms: 0,
        }
    }

    // ==================================================================
    // calculate_overshoot — numeric
    // ==================================================================

    #[test]
    fn calculate_overshoot_returns_zero_inside_the_range_and_on_both_boundaries() {
        assert_eq!(calculate_overshoot(0.0, 0.0, 100.0), 0.0);
        assert_eq!(calculate_overshoot(100.0, 0.0, 100.0), 0.0);
        assert_eq!(calculate_overshoot(50.0, 0.0, 100.0), 0.0);
        // Degenerate range (min == max): only that single point is in range.
        assert_eq!(calculate_overshoot(0.0, 0.0, 0.0), 0.0);
        // -0.0 is neither < 0.0 nor > 0.0, so it counts as in-range.
        assert_eq!(calculate_overshoot(-0.0, 0.0, 100.0), 0.0);
    }

    #[test]
    fn calculate_overshoot_is_signed_by_which_boundary_was_crossed() {
        assert_eq!(calculate_overshoot(-10.0, 0.0, 100.0), -10.0);
        assert_eq!(calculate_overshoot(110.0, 0.0, 100.0), 10.0);
        // Negative range: overshoot is still measured relative to the boundary.
        assert_eq!(calculate_overshoot(-30.0, -20.0, -10.0), -10.0);
        assert_eq!(calculate_overshoot(0.0, -20.0, -10.0), 10.0);
    }

    #[test]
    fn calculate_overshoot_nan_position_reports_no_overshoot() {
        // Both `NaN < min` and `NaN > max` are false, so the in-range branch wins
        // and a NaN position is reported as "not overshooting" rather than
        // propagating NaN into the spring force.
        let out = calculate_overshoot(f32::NAN, 0.0, 100.0);
        assert!(!out.is_nan(), "NaN must not leak out of calculate_overshoot");
        assert_eq!(out, 0.0);
        // A NaN bound, however, does make every position look "in range".
        assert_eq!(calculate_overshoot(1e9, 0.0, f32::NAN), 0.0);
        assert_eq!(calculate_overshoot(-1e9, f32::NAN, 100.0), 0.0);
    }

    #[test]
    fn calculate_overshoot_infinite_position_saturates_without_panicking() {
        assert_eq!(calculate_overshoot(f32::INFINITY, 0.0, 100.0), f32::INFINITY);
        assert_eq!(
            calculate_overshoot(f32::NEG_INFINITY, 0.0, 100.0),
            f32::NEG_INFINITY
        );
        // inf - inf would be NaN; the boundary check keeps us in-range instead.
        assert_eq!(
            calculate_overshoot(f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY),
            0.0
        );
    }

    #[test]
    fn calculate_overshoot_extreme_finite_range_overflows_to_infinity_not_a_panic() {
        // f32::MAX - f32::MIN is not representable -> +inf. Defined, no panic.
        let out = calculate_overshoot(f32::MAX, f32::MIN, f32::MIN);
        assert!(out.is_infinite() && out.is_sign_positive());
        let out = calculate_overshoot(f32::MIN, f32::MAX, f32::MAX);
        assert!(out.is_infinite() && out.is_sign_negative());
    }

    #[test]
    fn calculate_overshoot_inverted_range_is_deterministic() {
        // min > max: the `pos < min` branch is checked first, so everything below
        // `min` reads as a negative overshoot. No panic, no assertion inside.
        assert_eq!(calculate_overshoot(5.0, 10.0, 0.0), -5.0);
        assert_eq!(calculate_overshoot(20.0, 10.0, 0.0), 20.0);
    }

    // ==================================================================
    // rubber_band_clamp — numeric
    // ==================================================================

    #[test]
    fn rubber_band_clamp_is_the_identity_inside_the_range() {
        assert_eq!(rubber_band_clamp(0.0, 0.0, 100.0, 50.0, 0.5), 0.0);
        assert_eq!(rubber_band_clamp(50.0, 0.0, 100.0, 50.0, 0.5), 50.0);
        assert_eq!(rubber_band_clamp(100.0, 0.0, 100.0, 50.0, 0.5), 100.0);
    }

    #[test]
    fn rubber_band_clamp_with_zero_max_overscroll_hard_clamps_to_the_boundary() {
        assert_eq!(rubber_band_clamp(1000.0, 0.0, 100.0, 0.0, 0.5), 100.0);
        assert_eq!(rubber_band_clamp(-1000.0, 0.0, 100.0, 0.0, 0.5), 0.0);
        // A negative max_overscroll takes the same `else` branch (no bounce).
        assert_eq!(rubber_band_clamp(1000.0, 0.0, 100.0, -50.0, 0.5), 100.0);
        assert_eq!(rubber_band_clamp(-1000.0, 0.0, 100.0, -50.0, 0.5), 0.0);
    }

    #[test]
    fn rubber_band_clamp_with_zero_elasticity_pins_to_the_boundary() {
        // 1 - e^0 == 0, so no overshoot displacement at all.
        assert_eq!(rubber_band_clamp(1000.0, 0.0, 100.0, 120.0, 0.0), 100.0);
        assert_eq!(rubber_band_clamp(-1000.0, 0.0, 100.0, 120.0, 0.0), 0.0);
    }

    #[test]
    fn rubber_band_clamp_never_exceeds_max_overscroll_even_for_absurd_input() {
        let (min, max, max_over, elast) = (0.0, 100.0, 120.0, 0.5);
        for raw in [101.0_f32, 500.0, 1e6, 1e30, f32::MAX, f32::INFINITY] {
            let out = rubber_band_clamp(raw, min, max, max_over, elast);
            assert!(out.is_finite(), "raw={raw} produced {out}");
            assert!(
                out >= max && out <= max + max_over,
                "raw={raw} escaped the overscroll band: {out}"
            );
        }
        for raw in [-1.0_f32, -500.0, -1e6, -1e30, f32::MIN, f32::NEG_INFINITY] {
            let out = rubber_band_clamp(raw, min, max, max_over, elast);
            assert!(out.is_finite(), "raw={raw} produced {out}");
            assert!(
                out <= min && out >= min - max_over,
                "raw={raw} escaped the overscroll band: {out}"
            );
        }
        // The band is approached asymptotically: an infinite pull lands exactly on it.
        assert_eq!(
            rubber_band_clamp(f32::INFINITY, min, max, max_over, elast),
            max + max_over
        );
        assert_eq!(
            rubber_band_clamp(f32::NEG_INFINITY, min, max, max_over, elast),
            min - max_over
        );
    }

    #[test]
    fn rubber_band_clamp_has_diminishing_returns_and_stays_monotonic() {
        let (min, max, max_over, elast) = (0.0, 100.0, 100.0, 0.5);
        let mut previous = max;
        for raw in [110.0_f32, 120.0, 200.0, 400.0, 800.0] {
            let out = rubber_band_clamp(raw, min, max, max_over, elast);
            // Monotonically increasing...
            assert!(out > previous, "not monotonic at raw={raw}: {out} <= {previous}");
            // ...but always giving back less than the raw pull (springy resistance).
            assert!(
                out < raw,
                "raw={raw} was not resisted at all (got {out})"
            );
            previous = out;
        }
    }

    #[test]
    fn rubber_band_clamp_nan_inputs_are_defined_and_do_not_panic() {
        // NaN fails both in-range comparisons, falls into the `raw_pos >= max`
        // branch, and NaN propagates to the result. The caller
        // (`scroll_to_unclamped` -> change processor) sanitises it later.
        assert!(rubber_band_clamp(f32::NAN, 0.0, 100.0, 120.0, 0.5).is_nan());
        // A NaN elasticity / max_overscroll must not panic either.
        assert!(rubber_band_clamp(500.0, 0.0, 100.0, 120.0, f32::NAN).is_nan());
        // NaN max_overscroll is not > 0.0, so the no-bounce branch pins the boundary.
        assert_eq!(rubber_band_clamp(500.0, 0.0, 100.0, f32::NAN, 0.5), 100.0);
    }

    #[test]
    fn rubber_band_clamp_negative_elasticity_stays_non_nan() {
        // A negative elasticity inverts the exponential (e^+x): the "resistance"
        // becomes an amplification. It is nonsense physically, but it must not
        // panic and must not produce NaN.
        for raw in [110.0_f32, 1e6, f32::MAX] {
            let out = rubber_band_clamp(raw, 0.0, 100.0, 100.0, -1.0);
            assert!(!out.is_nan(), "raw={raw} produced NaN");
        }
        // Small overshoot with negative elasticity: still finite and defined.
        let out = rubber_band_clamp(110.0, 0.0, 100.0, 100.0, -1.0);
        assert!(out.is_finite());
        assert!(out < 100.0, "negative elasticity flips the sign: {out}");
    }

    #[test]
    fn rubber_band_clamp_degenerate_range_still_returns_a_boundary() {
        // min == max: everything except that point overshoots.
        assert_eq!(rubber_band_clamp(0.0, 0.0, 0.0, 100.0, 0.5), 0.0);
        let out = rubber_band_clamp(10.0, 0.0, 0.0, 100.0, 0.5);
        assert!(out > 0.0 && out <= 100.0, "{out}");
        let out = rubber_band_clamp(-10.0, 0.0, 0.0, 100.0, 0.5);
        assert!((-100.0..0.0).contains(&out), "{out}");
    }

    // ==================================================================
    // friction_from_deceleration — numeric
    // ==================================================================

    #[test]
    fn friction_from_deceleration_matches_the_documented_values() {
        assert!((friction_from_deceleration(0.95) - 0.05).abs() < 1e-6);
        assert!((friction_from_deceleration(0.998) - 0.002).abs() < 1e-6);
        assert!((friction_from_deceleration(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn friction_from_deceleration_clamps_both_ends_and_never_returns_zero() {
        // Anything >= 0.999 collapses onto the 0.001 friction floor: a
        // deceleration_rate of exactly 1.0 ("never stops") must NOT produce a
        // zero friction, or momentum would run forever.
        assert_eq!(friction_from_deceleration(1.0), 0.001);
        assert_eq!(friction_from_deceleration(0.999), 0.001);
        assert_eq!(friction_from_deceleration(f32::MAX), 0.001);
        assert_eq!(friction_from_deceleration(f32::INFINITY), 0.001);
        // Anything <= 0.0 saturates to full friction.
        assert_eq!(friction_from_deceleration(-0.0), 1.0);
        assert_eq!(friction_from_deceleration(-5.0), 1.0);
        assert_eq!(friction_from_deceleration(f32::MIN), 1.0);
        assert_eq!(friction_from_deceleration(f32::NEG_INFINITY), 1.0);
    }

    #[test]
    fn friction_from_deceleration_nan_falls_back_to_the_floor() {
        // f32::clamp(NaN) == NaN, but `NaN.max(0.001)` == 0.001 (f32::max ignores
        // NaN), so the friction floor rescues the whole physics integration.
        let out = friction_from_deceleration(f32::NAN);
        assert!(!out.is_nan(), "NaN deceleration must not poison friction");
        assert_eq!(out, 0.001);
    }

    #[test]
    fn friction_from_deceleration_always_yields_a_usable_decay_factor() {
        let dt = 16.0 / 1000.0;
        for rate in [
            0.0_f32,
            0.5,
            0.9,
            0.95,
            0.996,
            0.998,
            0.999,
            1.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
        ] {
            let friction = friction_from_deceleration(rate);
            assert!(friction.is_finite(), "rate={rate} -> {friction}");
            assert!(
                (0.001..=1.0).contains(&friction),
                "rate={rate} -> friction {friction} outside [0.001, 1.0]"
            );
            // This is exactly how the callback uses it: exp(-friction*dt*60).
            let decay = (-friction * dt * ASSUMED_FPS).exp();
            assert!(decay.is_finite() && decay > 0.0 && decay < 1.0, "rate={rate} -> decay {decay}");
        }
    }

    // ==================================================================
    // spring_constant_from_bounce_duration — numeric
    // ==================================================================

    #[test]
    fn spring_constant_clamps_short_durations_to_50ms() {
        let floor = spring_constant_from_bounce_duration(50);
        assert_eq!(spring_constant_from_bounce_duration(0), floor);
        assert_eq!(spring_constant_from_bounce_duration(1), floor);
        assert_eq!(spring_constant_from_bounce_duration(49), floor);
        // (2*pi / 0.05)^2 ~= 15791.4
        assert!((floor - 15791.37).abs() < 1.0, "{floor}");
    }

    #[test]
    fn spring_constant_decreases_monotonically_with_duration() {
        let mut previous = f32::INFINITY;
        for ms in [50_u32, 100, 200, 300, 400, 500, 1000, 10_000] {
            let k = spring_constant_from_bounce_duration(ms);
            assert!(k.is_finite() && k > 0.0, "ms={ms} -> {k}");
            assert!(k < previous, "ms={ms}: {k} did not decrease below {previous}");
            previous = k;
        }
    }

    #[test]
    fn spring_constant_stays_finite_and_positive_at_u32_max() {
        // duration_ms = u32::MAX -> ~4295 seconds -> a vanishingly small k.
        // It must stay > 0 so that `2 * k.sqrt()` (the damping term) is not NaN.
        let k = spring_constant_from_bounce_duration(u32::MAX);
        assert!(k.is_finite(), "{k}");
        assert!(k > 0.0, "{k}");
        assert!(k < 1e-6, "{k}");
    }

    #[test]
    fn spring_constant_damping_term_is_always_finite() {
        // The callback computes `2.0 * spring_k.sqrt()`; a negative or NaN k
        // would make the critical-damping coefficient NaN.
        for ms in [0_u32, 1, 50, 400, 1000, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            let k = spring_constant_from_bounce_duration(ms);
            let damping = 2.0 * k.sqrt();
            assert!(damping.is_finite() && damping > 0.0, "ms={ms} -> damping {damping}");
        }
    }

    // ==================================================================
    // node_allows_rubber_band — predicate / numeric
    // ==================================================================

    #[test]
    fn node_allows_rubber_band_requires_actual_overflow_on_the_axis() {
        // An axis with no overflow can never rubber-band, whatever the config.
        for max_scroll in [0.0_f32, -0.0, -1.0, -1e9, f32::MIN, f32::NEG_INFINITY] {
            assert!(
                !node_allows_rubber_band(
                    max_scroll,
                    OverscrollBehavior::Auto,
                    OverflowScrolling::Touch,
                    1.0
                ),
                "max_scroll={max_scroll} must not rubber-band"
            );
        }
    }

    #[test]
    fn node_allows_rubber_band_is_vetoed_by_overscroll_behavior_none() {
        // `overscroll-behavior: none` wins over -azul-overflow-scrolling: touch
        // and over a fully elastic global config.
        assert!(!node_allows_rubber_band(
            400.0,
            OverscrollBehavior::None,
            OverflowScrolling::Touch,
            1.0
        ));
        assert!(!node_allows_rubber_band(
            f32::MAX,
            OverscrollBehavior::None,
            OverflowScrolling::Auto,
            1.0
        ));
    }

    #[test]
    fn node_allows_rubber_band_touch_overrides_a_zero_global_elasticity() {
        // -azul-overflow-scrolling: touch opts in even on a Windows-like
        // (elasticity 0.0) global config.
        assert!(node_allows_rubber_band(
            400.0,
            OverscrollBehavior::Auto,
            OverflowScrolling::Touch,
            0.0
        ));
        // `contain` blocks chaining but still permits the local bounce.
        assert!(node_allows_rubber_band(
            400.0,
            OverscrollBehavior::Contain,
            OverflowScrolling::Touch,
            0.0
        ));
    }

    #[test]
    fn node_allows_rubber_band_otherwise_follows_the_global_elasticity() {
        let ask = |elasticity: f32| {
            node_allows_rubber_band(
                400.0,
                OverscrollBehavior::Auto,
                OverflowScrolling::Auto,
                elasticity,
            )
        };
        assert!(!ask(0.0));
        assert!(!ask(-0.0));
        assert!(!ask(-1.0));
        assert!(!ask(f32::NEG_INFINITY));
        // NaN > 0.0 is false -> no bounce. Defined, no panic.
        assert!(!ask(f32::NAN));
        assert!(ask(f32::MIN_POSITIVE));
        assert!(ask(0.3));
        assert!(ask(f32::INFINITY));
    }

    #[test]
    fn node_allows_rubber_band_contain_still_bounces_locally() {
        // CSS: `contain` stops scroll *chaining*, not the local overscroll effect.
        assert!(node_allows_rubber_band(
            400.0,
            OverscrollBehavior::Contain,
            OverflowScrolling::Auto,
            0.5
        ));
        assert!(!node_allows_rubber_band(
            400.0,
            OverscrollBehavior::Contain,
            OverflowScrolling::Auto,
            0.0
        ));
    }

    #[test]
    fn node_allows_rubber_band_nan_max_scroll_is_treated_as_overflowing() {
        // NOTE (quirk, asserted so a change is noticed): `NaN <= 0.0` is false,
        // so a NaN max_scroll slips past the "has overflow" gate and the node is
        // allowed to rubber-band against a NaN boundary. Not reachable from a
        // sane layout (max_scroll comes from `(content - container).max(0.0)`),
        // but it is not defended against here either.
        assert!(node_allows_rubber_band(
            f32::NAN,
            OverscrollBehavior::Auto,
            OverflowScrolling::Auto,
            0.5
        ));
        assert!(node_allows_rubber_band(
            f32::NAN,
            OverscrollBehavior::Auto,
            OverflowScrolling::Touch,
            0.0
        ));
        // The other two vetoes still apply, NaN or not.
        assert!(!node_allows_rubber_band(
            f32::NAN,
            OverscrollBehavior::None,
            OverflowScrolling::Touch,
            1.0
        ));
    }

    // ==================================================================
    // ScrollPhysicsState::new — constructor
    // ==================================================================

    #[test]
    fn new_starts_empty_and_keeps_the_config_verbatim() {
        for physics in [
            ScrollPhysics::default(),
            ScrollPhysics::ios(),
            ScrollPhysics::macos(),
            ScrollPhysics::windows(),
            ScrollPhysics::android(),
            nan_physics(),
        ] {
            let state = ScrollPhysicsState::new(ScrollInputQueue::new(), physics);
            assert!(state.node_velocities.is_empty());
            assert!(state.pending_positions.is_empty());
            assert!(state.pending_trackpad_positions.is_empty());
            assert!(!state.input_queue.has_pending());
            // Config is stored verbatim (compare a field that is not NaN).
            assert_eq!(
                state.scroll_physics.timer_interval_ms,
                physics.timer_interval_ms
            );
            assert_eq!(state.scroll_physics.max_velocity, physics.max_velocity);
        }
    }

    #[test]
    fn new_shares_the_input_queue_rather_than_copying_it() {
        // The whole architecture depends on this: the event handler pushes into
        // its clone of the queue and the timer must see it.
        let queue = ScrollInputQueue::new();
        let state = ScrollPhysicsState::new(queue.clone(), ScrollPhysics::default());
        assert!(!state.input_queue.has_pending());

        queue.push(input(0, (0.0, 10.0), ScrollInputSource::WheelDiscrete));
        assert!(
            state.input_queue.has_pending(),
            "the queue must be shared (Arc), not deep-copied"
        );

        let taken = state.input_queue.take_recent(MAX_SCROLL_EVENTS_PER_TICK);
        assert_eq!(taken.len(), 1);
        assert!(!queue.has_pending(), "draining the timer side drains both");
    }

    // ==================================================================
    // ScrollPhysicsState::is_active — predicate
    // ==================================================================

    #[test]
    fn is_active_is_false_for_a_fresh_state() {
        let state = ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::default());
        assert!(!state.is_active());
    }

    #[test]
    fn is_active_is_true_while_inputs_are_pending() {
        let queue = ScrollInputQueue::new();
        let state = ScrollPhysicsState::new(queue.clone(), ScrollPhysics::default());
        queue.push(input(0, (0.0, 1.0), ScrollInputSource::WheelDiscrete));
        assert!(state.is_active());
    }

    #[test]
    fn is_active_uses_a_strict_greater_than_against_the_threshold() {
        let mut state = ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::default());
        let threshold = state.scroll_physics.min_velocity_threshold; // 50.0
        let at = |velocity: LogicalPosition| NodeScrollPhysics {
            velocity,
            is_rubber_banding: false,
        };

        // Exactly at the threshold is NOT active (strict `>`).
        state
            .node_velocities
            .insert(key(0), at(LogicalPosition::new(0.0, threshold)));
        assert!(!state.is_active(), "velocity == threshold must not be active");

        // A hair above it is.
        state
            .node_velocities
            .insert(key(0), at(LogicalPosition::new(0.0, threshold * 1.0001)));
        assert!(state.is_active());

        // Either axis is enough, and the sign does not matter.
        state
            .node_velocities
            .insert(key(0), at(LogicalPosition::new(-threshold * 2.0, 0.0)));
        assert!(state.is_active(), "|velocity| is what counts, not the sign");
    }

    #[test]
    fn is_active_is_true_while_rubber_banding_even_at_zero_velocity() {
        let mut state = ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::default());
        state.node_velocities.insert(
            key(0),
            NodeScrollPhysics {
                velocity: LogicalPosition::zero(),
                is_rubber_banding: true,
            },
        );
        assert!(
            state.is_active(),
            "the spring-back animation must keep the timer alive"
        );
    }

    #[test]
    fn is_active_is_true_while_positions_are_pending() {
        let mut state = ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::default());
        state
            .pending_positions
            .insert(key(0), LogicalPosition::zero());
        assert!(state.is_active());

        let mut state = ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::default());
        state
            .pending_trackpad_positions
            .insert(key(0), LogicalPosition::zero());
        assert!(state.is_active());
    }

    #[test]
    fn is_active_treats_nan_velocity_as_inactive_without_panicking() {
        // NaN.abs() > threshold is false -> the node reads as at rest. The
        // important part is that this is deterministic and does not panic.
        let mut state = ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::default());
        state.node_velocities.insert(
            key(0),
            NodeScrollPhysics {
                velocity: LogicalPosition::new(f32::NAN, f32::NAN),
                is_rubber_banding: false,
            },
        );
        assert!(!state.is_active());

        // A NaN *threshold* likewise never reports active.
        let mut state = ScrollPhysicsState::new(ScrollInputQueue::new(), nan_physics());
        state.node_velocities.insert(
            key(0),
            NodeScrollPhysics {
                velocity: LogicalPosition::new(1e9, 1e9),
                is_rubber_banding: false,
            },
        );
        assert!(!state.is_active());
    }

    #[test]
    fn is_active_with_infinite_velocity_is_true() {
        let mut state = ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::default());
        state.node_velocities.insert(
            key(0),
            NodeScrollPhysics {
                velocity: LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
                is_rubber_banding: false,
            },
        );
        assert!(state.is_active());
    }

    // ==================================================================
    // scroll_physics_timer_callback — smoke / integration
    // ==================================================================

    #[test]
    fn callback_with_a_foreign_refany_terminates_instead_of_panicking() {
        let data = RefAny::new(42_u32);
        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(ret.should_terminate, TerminateTimer::Terminate);
            assert_eq!(ret.should_update, Update::DoNothing);
            assert!(env.take_changes().is_empty());
        });
    }

    #[test]
    fn callback_with_nothing_to_do_terminates_the_timer() {
        let (data, _queue) = state_with(ScrollPhysics::default());
        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(
                ret.should_terminate,
                TerminateTimer::Terminate,
                "an idle physics timer must not keep spinning"
            );
            assert!(env.take_changes().is_empty());
        });
    }

    #[test]
    fn callback_programmatic_input_pushes_a_hard_clamped_scroll_to() {
        let (data, queue) = state_with(ScrollPhysics::default());
        // Viewport 100x100 over 100x500 content -> max_scroll = (0, 400).
        queue.push(input(3, (0.0, 10_000.0), ScrollInputSource::Programmatic));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let ret = env.tick(&data);
                assert_eq!(ret.should_terminate, TerminateTimer::Continue);
                // Scroll is applied via nodes_scrolled_in_callbacks, not a relayout.
                assert_eq!(ret.should_update, Update::DoNothing);

                let scrolls = env.take_scroll_tos();
                assert_eq!(scrolls.len(), 1);
                let (idx, pos, unclamped) = scrolls[0];
                assert_eq!(idx, 3);
                assert!(!unclamped, "programmatic scroll must be hard-clamped");
                assert_eq!(pos.x, 0.0);
                assert_eq!(pos.y, 400.0, "a 10000px jump must clamp to max_scroll_y");
            },
        );
    }

    #[test]
    fn callback_programmatic_negative_input_clamps_to_zero() {
        let (data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(3, (-1e9, -1e9), ScrollInputSource::Programmatic));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let _ = env.tick(&data);
                let scrolls = env.take_scroll_tos();
                assert_eq!(scrolls.len(), 1);
                assert_eq!(scrolls[0].1, LogicalPosition::zero());
            },
        );
    }

    #[test]
    fn callback_trackpad_overshoot_is_bounded_by_max_overscroll_distance() {
        // iOS physics: elasticity 0.5, max_overscroll_distance 120.
        let physics = ScrollPhysics::ios();
        let (data, queue) = state_with(physics);
        queue.push(input(3, (0.0, 1e9), ScrollInputSource::TrackpadContinuous));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let ret = env.tick(&data);
                assert_eq!(ret.should_terminate, TerminateTimer::Continue);

                let scrolls = env.take_scroll_tos();
                assert_eq!(scrolls.len(), 1);
                let (idx, pos, unclamped) = scrolls[0];
                assert_eq!(idx, 3);
                assert!(unclamped, "the timer does its own rubber-band clamping");
                assert!(pos.y.is_finite());
                // max_scroll_y (400) + max_overscroll_distance (120) is the ceiling.
                assert!(
                    pos.y > 400.0 && pos.y <= 400.0 + physics.max_overscroll_distance + 1e-3,
                    "a 1e9 px flick escaped the overscroll band: {}",
                    pos.y
                );
                // The x axis has no overflow -> no bounce, hard 0.
                assert_eq!(pos.x, 0.0);
            },
        );
    }

    #[test]
    fn callback_trackpad_without_elasticity_hard_clamps() {
        // Windows physics: elasticity 0.0, max_overscroll_distance 0.0.
        let (data, queue) = state_with(ScrollPhysics::windows());
        queue.push(input(3, (0.0, 1e9), ScrollInputSource::TrackpadContinuous));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let _ = env.tick(&data);
                let scrolls = env.take_scroll_tos();
                assert_eq!(scrolls.len(), 1);
                assert_eq!(scrolls[0].1.y, 400.0, "no bounce -> pinned to max_scroll_y");
            },
        );
    }

    #[test]
    fn callback_wheel_impulse_is_clamped_to_max_velocity() {
        let physics = ScrollPhysics::default(); // max_velocity 8000, wheel_multiplier 1.0
        let (mut data, queue) = state_with(physics);
        // delta * wheel_multiplier * 60 would be 6e10 / -inf without the clamp.
        queue.push(input(0, (1e9, -1e9), ScrollInputSource::WheelDiscrete));
        queue.push(input(1, (f32::INFINITY, f32::NEG_INFINITY), ScrollInputSource::WheelDiscrete));

        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(ret.should_terminate, TerminateTimer::Continue);
        });

        with_state(&mut data, |state| {
            for idx in [0_usize, 1] {
                let node = state
                    .node_velocities
                    .get(&key(idx))
                    .unwrap_or_else(|| panic!("node {idx} lost its velocity"));
                assert_eq!(node.velocity.x, physics.max_velocity, "node {idx}");
                assert_eq!(node.velocity.y, -physics.max_velocity, "node {idx}");
            }
        });
    }

    #[test]
    fn callback_wheel_momentum_decays_and_never_leaves_the_scroll_bounds() {
        let (mut data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(3, (0.0, 100.0), ScrollInputSource::WheelDiscrete));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let mut ticks = 0;
                // The offset in the (immutable) LayoutWindow never advances, so
                // this isolates the decay: the timer MUST still wind down.
                loop {
                    let ret = env.tick(&data);
                    for (_, pos, _) in env.take_scroll_tos() {
                        assert!(pos.x.is_finite() && pos.y.is_finite());
                        assert!(
                            (0.0..=400.0).contains(&pos.y),
                            "tick {ticks}: y={} left [0, max_scroll_y]",
                            pos.y
                        );
                        assert_eq!(pos.x, 0.0);
                    }
                    ticks += 1;
                    if ret.should_terminate == TerminateTimer::Terminate {
                        break;
                    }
                    assert!(
                        ticks < 1000,
                        "momentum never decayed below the velocity threshold"
                    );
                }
                assert!(ticks > 1, "the fling should survive at least one tick");
            },
        );

        with_state(&mut data, |state| {
            assert!(
                state.node_velocities.is_empty(),
                "a terminated timer must not leave live velocities behind"
            );
        });
    }

    #[test]
    fn callback_caps_the_events_processed_per_tick() {
        let (data, queue) = state_with(ScrollPhysics::default());
        // 5x the cap, each targeting a distinct node so they cannot coalesce.
        let total = MAX_SCROLL_EVENTS_PER_TICK * 5;
        for i in 0..total {
            queue.push(input(i, (0.0, 1.0), ScrollInputSource::Programmatic));
        }

        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(ret.should_terminate, TerminateTimer::Continue);

            let scrolls = env.take_scroll_tos();
            assert_eq!(
                scrolls.len(),
                MAX_SCROLL_EVENTS_PER_TICK,
                "the per-tick event budget must be enforced"
            );
            // take_recent keeps the NEWEST events, so the surviving nodes are the
            // last MAX_SCROLL_EVENTS_PER_TICK that were pushed.
            for (idx, _, _) in &scrolls {
                assert!(
                    *idx >= total - MAX_SCROLL_EVENTS_PER_TICK,
                    "node {idx} is a stale event that should have been dropped"
                );
            }
        });

        assert!(
            !queue.has_pending(),
            "the backlog must be drained, not left to grow unboundedly"
        );
    }

    #[test]
    fn callback_nan_delta_does_not_panic() {
        // Programmatic: the NaN reaches the change log (the change processor
        // sanitises it via AnimatedScrollState::clamp) but nothing panics.
        let (data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(0, (f32::NAN, f32::NAN), ScrollInputSource::Programmatic));

        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(ret.should_terminate, TerminateTimer::Continue);
            let scrolls = env.take_scroll_tos();
            assert_eq!(scrolls.len(), 1);
            assert!(scrolls[0].1.x.is_nan() && scrolls[0].1.y.is_nan());
        });
    }

    #[test]
    fn callback_nan_wheel_delta_drops_the_node_instead_of_spinning_forever() {
        // A NaN velocity survives the clamp, but `retain` uses `> 0.0` (false for
        // NaN) so the node is dropped and the timer terminates. Asserted so that a
        // regression into an un-killable NaN velocity loop is caught.
        let (mut data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(0, (f32::NAN, f32::NAN), ScrollInputSource::WheelDiscrete));

        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(ret.should_terminate, TerminateTimer::Terminate);
            assert!(env.take_changes().is_empty());
        });

        with_state(&mut data, |state| {
            assert!(state.node_velocities.is_empty());
        });
    }

    #[test]
    fn callback_trackpad_end_on_an_unknown_node_is_a_no_op() {
        let (data, queue) = state_with(ScrollPhysics::ios());
        queue.push(input(7, (0.0, 0.0), ScrollInputSource::TrackpadEnd));

        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(ret.should_terminate, TerminateTimer::Terminate);
            assert!(env.take_changes().is_empty());
        });
    }

    #[test]
    fn callback_trackpad_end_inside_the_bounds_does_not_start_a_spring_back() {
        let (mut data, queue) = state_with(ScrollPhysics::ios());
        queue.push(input(3, (0.0, 0.0), ScrollInputSource::TrackpadEnd));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                // offset is 0 (in range) -> no overshoot -> no rubber-banding.
                let _ = env.tick(&data);
                let scrolls = env.take_scroll_tos();
                assert_eq!(scrolls.len(), 1, "the position is re-pushed unclamped");
                assert!(scrolls[0].2, "TrackpadEnd re-pushes the raw position");
                assert_eq!(scrolls[0].1, LogicalPosition::zero());
            },
        );

        with_state(&mut data, |state| {
            assert!(
                state.node_velocities.is_empty(),
                "no overshoot must not arm the spring"
            );
        });
    }

    #[test]
    fn callback_degenerate_physics_config_does_not_panic() {
        // Every float NaN, every duration 0 (max_velocity stays 0.0: see the
        // `known_bug` tests for why a NaN/negative max_velocity panics).
        let (data, queue) = state_with(nan_physics());
        queue.push(input(0, (10.0, 10.0), ScrollInputSource::WheelDiscrete));
        queue.push(input(1, (10.0, 10.0), ScrollInputSource::TrackpadContinuous));
        queue.push(input(2, (10.0, 10.0), ScrollInputSource::Programmatic));
        queue.push(input(3, (10.0, 10.0), ScrollInputSource::TrackpadEnd));

        with_env(
            |w| {
                register_node(w, 0, (100.0, 100.0), (100.0, 500.0));
                register_node(w, 1, (100.0, 100.0), (100.0, 500.0));
                register_node(w, 2, (100.0, 100.0), (100.0, 500.0));
                register_node(w, 3, (100.0, 100.0), (100.0, 500.0));
            },
            |env| {
                let ret = env.tick(&data);
                // Whatever it decides, it must decide *something* and not panic.
                assert!(matches!(
                    ret.should_terminate,
                    TerminateTimer::Continue | TerminateTimer::Terminate
                ));
                // A second tick over the resulting state must survive too.
                let _ = env.tick(&data);
            },
        );
    }

    #[test]
    fn callback_zero_timer_interval_still_advances_time() {
        // dt = max(1) / 1000 -> a 0ms interval must not produce dt == 0 (which
        // would freeze the integration) nor a division by zero.
        let physics = ScrollPhysics {
            timer_interval_ms: 0,
            ..ScrollPhysics::default()
        };
        let (mut data, queue) = state_with(physics);
        queue.push(input(3, (0.0, 100.0), ScrollInputSource::WheelDiscrete));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let ret = env.tick(&data);
                assert_eq!(ret.should_terminate, TerminateTimer::Continue);
                let scrolls = env.take_scroll_tos();
                assert_eq!(scrolls.len(), 1);
                let y = scrolls[0].1.y;
                assert!(y.is_finite() && y > 0.0 && y <= 400.0, "y={y}");
            },
        );

        with_state(&mut data, |state| {
            let node = state.node_velocities.get(&key(3)).expect("velocity kept");
            assert!(node.velocity.y.is_finite());
        });
    }

    #[test]
    fn callback_survives_a_huge_backlog_on_a_single_node() {
        // All events coalesce onto one node: the velocity impulses accumulate but
        // must stay clamped, and the queue must be fully drained.
        let physics = ScrollPhysics::default();
        let (mut data, queue) = state_with(physics);
        for _ in 0..(MAX_SCROLL_EVENTS_PER_TICK * 10) {
            queue.push(input(0, (0.0, 1e6), ScrollInputSource::WheelDiscrete));
        }

        with_env(|_| {}, |env| {
            let ret = env.tick(&data);
            assert_eq!(ret.should_terminate, TerminateTimer::Continue);
        });

        assert!(!queue.has_pending());
        with_state(&mut data, |state| {
            let node = state.node_velocities.get(&key(0)).expect("velocity kept");
            assert_eq!(
                node.velocity.y, physics.max_velocity,
                "1000 stacked impulses must not exceed max_velocity"
            );
        });
    }

    // ------------------------------------------------------------------
    // Known hazard — a NaN or negative `max_velocity` panics inside f32::clamp.
    //
    // The `WheelDiscrete` branch does `velocity.clamp(-max_velocity, max_velocity)`
    // and `ScrollPhysics` is a plain `#[repr(C)]` struct with public fields and no
    // validation, so a bad `SystemStyle` reaches `f32::clamp` with `min > max`.
    //
    // These tests reproduce the exact expression the callback evaluates rather
    // than calling the callback itself: `scroll_physics_timer_callback` is
    // `extern "C"`, and a panic unwinding out of a C-ABI function ABORTS the
    // process instead of unwinding, which would take the whole test binary down.
    // That abort-on-bad-config is precisely why this is worth pinning.
    // ------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "min > max")]
    fn known_hazard_nan_max_velocity_panics_the_wheel_clamp() {
        let max_velocity = ScrollPhysics {
            max_velocity: f32::NAN,
            ..ScrollPhysics::default()
        }
        .max_velocity;
        // Exactly what the WheelDiscrete branch evaluates:
        let _clamped = (600.0_f32).clamp(-max_velocity, max_velocity);
    }

    #[test]
    #[should_panic(expected = "min > max")]
    fn known_hazard_negative_max_velocity_panics_the_wheel_clamp() {
        let max_velocity = ScrollPhysics {
            max_velocity: -1.0,
            ..ScrollPhysics::default()
        }
        .max_velocity;
        let _clamped = (600.0_f32).clamp(-max_velocity, max_velocity);
    }

    #[test]
    fn zero_max_velocity_is_the_only_safe_degenerate_config() {
        // -0.0 <= 0.0, so a zero max_velocity does NOT panic: it pins every
        // wheel impulse to zero. This is the boundary the two tests above sit on.
        assert_eq!((600.0_f32).clamp(-0.0, 0.0), 0.0);
        assert_eq!((-600.0_f32).clamp(-0.0, 0.0), -0.0);
        // ...and NaN passes straight through clamp without panicking.
        assert!(f32::NAN.clamp(-8000.0, 8000.0).is_nan());
    }
}
