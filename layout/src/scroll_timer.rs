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
    managers::scroll_state::{
        ScrollInput, ScrollInputDevice, ScrollInputQueue, ScrollInputSource, ScrollNodeInfo,
    },
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

/// Ticks without a delta after which a finger's raw accumulator is dropped
/// (≈ 200 ms at the 16 ms tick; X11 synthesises its gesture end after
/// 100 ms).
///
/// A lost `TrackpadEnd` must not pin the timer and must not leave
/// a stretched view without a spring-back.
const GESTURE_STALE_TICKS: u32 = 12;

/// Ticks without a momentum delta after which an IGNORED momentum tail is
/// considered finished and its latch is dropped (≈ 200 ms at the 16 ms tick).
///
/// The macOS tail is dense — one event per frame or better — right up to its
/// last delta, so a gap this long means it really stopped, and a genuinely new
/// fling must not be swallowed by a stale latch.
const MOMENTUM_LATCH_STALE_TICKS: u32 = 12;

/// Per-axis "ignore the OS momentum tail on this axis" latch for one node.
///
/// See [`ScrollPhysicsState::momentum_latched`] for why this is stored state
/// and not a predicate over the live physics.
#[derive(Copy, Debug, Clone, Default)]
pub struct MomentumLatch {
    /// The rubber-band spring owns X: drop this node's momentum X deltas.
    pub x: bool,
    /// The rubber-band spring owns Y: drop this node's momentum Y deltas.
    pub y: bool,
    /// Ticks since the last momentum delta arrived for this node.
    pub idle: u32,
}

impl MomentumLatch {
    /// True once neither axis is being ignored any more.
    #[must_use]
    pub const fn is_clear(&self) -> bool {
        !self.x && !self.y
    }
}

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
    /// The UNCLAMPED finger offset per node for the gesture in flight, with
    /// the number of ticks since its last delta.
    ///
    /// THE OVERSCROLL STRETCH IS A FUNCTION OF HOW FAR THE FINGER TRAVELLED
    /// PAST THE EDGE (`D(Σ deltas)`, what AppKit/UIKit do) — not of how much
    /// delta arrived in the current tick. The accumulator used to be the
    /// committed offset, i.e. the DISPLAYED, already-banded value, so every
    /// tick computed `x ← D(x + d)`: a contraction (`D' ≈ 0.3`) that forgets
    /// its history in two ticks and whose fixed point tracks the per-tick
    /// batch. A 120 Hz trackpad against the 62.5 Hz timer alternates one and
    /// two events per tick, and that beat came out as a ±1 px sawtooth at
    /// ~31 Hz — the macOS overscroll jitter. Seeded from the committed offset
    /// (inverted through the band if it is already overscrolled) on the first
    /// delta, advanced by every delta, dropped at `TrackpadEnd` or after
    /// [`GESTURE_STALE_TICKS`] without one.
    pub trackpad_raw_positions: BTreeMap<(DomId, NodeId), (LogicalPosition, u32)>,
    /// Per-node, per-axis latch: "the rubber-band spring owns this axis, so
    /// the OS momentum tail for it is being IGNORED".
    ///
    /// macOS synthesises the momentum tail from the finger velocity AT LIFT-OFF
    /// and replays that decay curve for 1-2 s. It is a canned animation: it
    /// knows nothing about our content bounds, there is no API to cancel it,
    /// and AppKit keeps delivering it to the view that was under the pointer
    /// when momentum began. So deltas keep arriving long after the content is
    /// pinned at an edge and the bounce has already finished.
    ///
    /// This was previously re-derived per event from `is_rubber_banding` — and
    /// that is the bug, because both of its inputs are transient: the spring
    /// CLEARS the flag when it lands, and the `node_velocities` retain then
    /// evicts the node's whole entry. The guard therefore evaporated at exactly
    /// the moment the bounce completed, and the next momentum delta re-seeded
    /// the raw accumulator from the committed offset and stretched the node
    /// straight back out for a WHOLE NEW BOUNCE. One flick played two or three
    /// bounces ("it snaps back, then suddenly bounces again"); a device trace
    /// shows the offset settling to 0.000 and jumping to -13.020 forty-five
    /// milliseconds later.
    ///
    /// A latch is not derivable from live physics state — it is a fact about
    /// the GESTURE — so it is stored. WebKit/Chromium's
    /// `ScrollElasticityController` carries the same flag as
    /// `ignore_momentum_scrolls_`, set when an edge is reached during momentum
    /// and cleared only by the next finger-down, with the events consumed
    /// rather than applied: "no visible scrolling but continued consumption of
    /// momentum wheel events", to stop endless stretching.
    ///
    /// Dropped when the finger comes back down (a new gesture) or after
    /// [`MOMENTUM_LATCH_STALE_TICKS`] with no momentum delta.
    ///
    /// ⚠ MUST be part of [`ScrollPhysicsState::is_active`]. The shell builds a
    /// BRAND-NEW `ScrollPhysicsState` every time it starts the momentum timer
    /// (`macos/events.rs:640`, and the same in the other four backends), so
    /// whatever the timer terminates with is LOST. When the spring lands every
    /// other map empties — while the tail is still running — so without this
    /// the timer terminated right there and the next momentum delta restarted
    /// it with an empty latch map, re-stretching the node: the very bug this
    /// field exists to prevent, reintroduced through the timer's lifetime.
    /// It cannot pin the timer, because the aging above bounds the latch to
    /// [`MOMENTUM_LATCH_STALE_TICKS`] (~200 ms) after the last momentum delta.
    /// Same reasoning as `trackpad_raw_positions`, which is in `is_active`
    /// for exactly this reason.
    pub momentum_latched: BTreeMap<(DomId, NodeId), MomentumLatch>,
    /// When the previous physics tick ran, so this one can integrate over the
    /// time that ACTUALLY elapsed.
    ///
    /// The simulation used to advance by a fixed `timer_interval_ms` — the
    /// timer's CONFIGURED period — while the real gap between admitted ticks
    /// jitters. `Timer::invoke` drops a fire that lands a hair under the
    /// interval and then stamps `last_run = now` rather than
    /// `last_run + interval`, so the phase never self-corrects and the next
    /// step arrives ~2 intervals later; two independent 16 ms timers drive the
    /// same pump; and 16 ms is neither 60 nor 120 Hz. Advancing 16 ms of
    /// simulation over 16-32 ms of wall clock makes the apparent speed vary by
    /// ±50 % frame to frame — the "blocky" scrolling.
    ///
    /// `None` on the first tick, which then falls back to the configured
    /// interval (there is no previous timestamp to difference against).
    pub last_tick: Option<azul_core::task::Instant>,
    /// Absolute offsets that `AnimateTo` / wheel-glide inputs are seeking,
    /// per node, together with the DEVICE that asked for the seek (device
    /// picks the spring duration: physical wheel clicks get the short
    /// `wheel_animate_bounce_ms` glide, everything else
    /// `bounce_back_duration_ms`). The integration loop applies a
    /// critically-damped spring toward each and removes the entry on
    /// convergence (snap to the exact target).
    pub animate_targets: BTreeMap<(DomId, NodeId), (LogicalPosition, ScrollInputDevice)>,
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
            trackpad_raw_positions: BTreeMap::new(),
            momentum_latched: BTreeMap::new(),
            animate_targets: BTreeMap::new(),
            last_tick: None,
            scroll_physics,
        }
    }

    /// Returns true if any node has non-zero velocity or there are pending inputs
    fn is_active(&self) -> bool {
        let threshold = self.scroll_physics.min_velocity_threshold;
        !self.animate_targets.is_empty()
            || self.input_queue.has_pending()
            || self.node_velocities.values().any(|v| {
                v.velocity.x.abs() > threshold
                    || v.velocity.y.abs() > threshold
                    || v.is_rubber_banding
            })
            || !self.pending_positions.is_empty()
            || !self.pending_trackpad_positions.is_empty()
            // A finger that is down but still (sparse events, empty ticks)
            // keeps the timer — and its accumulator — alive. A 60 Hz device
            // against the 62.5 Hz tick used to terminate the timer on every
            // empty tick and restart it with a fresh state on the next event.
            || !self.trackpad_raw_positions.is_empty()
            // An ignored momentum tail must keep the timer — and its latch —
            // alive. The shell rebuilds this whole state on every timer start,
            // so terminating here loses the latch and the tail re-stretches the
            // node it just finished bouncing. Bounded by the latch aging.
            || !self.momentum_latched.is_empty()
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
/// `AZ_SCROLL_DEBUG=1` turns on a per-event / per-tick trace of the scroll
/// pipeline.
///
/// It exists because the jitter reported on X11 and Wayland — a wheel scroll
/// that smooths, then jumps back and forward, damping toward the middle — could
/// not be reproduced from the code alone. Six candidate causes were eliminated
/// (spring stiffness, `dt`, device classification, X11 double ingress, the shm
/// slot count, and Wayland's slot catch-up), and the remaining ones need to
/// know what the platform actually DELIVERED, not what we think it delivers.
///
/// Off by default and checked once: this sits on the 16 ms tick and on every
/// scroll event, so it must cost nothing when unset.
#[cfg(feature = "std")]
#[must_use]
pub fn scroll_debug_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("AZ_SCROLL_DEBUG").map(|v| v == "1").unwrap_or(false))
}

#[cfg(not(feature = "std"))]
#[must_use]
pub const fn scroll_debug_enabled() -> bool {
    false
}

/// One line per scroll event as the PLATFORM delivered it, before any physics.
///
/// Call this from the platform ingress (x11 `handle_scroll_input`, wayland
/// `axis`), so a log tells us the raw delta, whether the backend called it
/// continuous, and which source/device it was classified as. That is the piece
/// no amount of reading the code can supply.
#[cfg(feature = "std")]
pub fn trace_scroll_input(
    backend: &str,
    raw_dx: f32,
    raw_dy: f32,
    continuous: bool,
    source: &str,
    device: &str,
) {
    if !scroll_debug_enabled() {
        return;
    }
    std::eprintln!(
        "[az-scroll] IN  backend={backend} raw=({raw_dx:.4},{raw_dy:.4}) \
         continuous={continuous} source={source} device={device}"
    );
}

#[cfg(not(feature = "std"))]
pub fn trace_scroll_input(_: &str, _: f32, _: f32, _: bool, _: &str, _: &str) {}

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

    // dt is the time that ACTUALLY passed, not the timer's configured period.
    // See `ScrollPhysicsState::last_tick` for why those differ in practice.
    // Clamped: a first tick, a suspended app or a debugger breakpoint must not
    // teleport the simulation, and a zero/negative delta (a clock that did not
    // move, or moved backwards) must not divide by zero downstream.
    //
    // Resolved BEFORE `sp` borrows the config, because it writes `last_tick`.
    let configured_dt = physics.scroll_physics.timer_interval_ms.max(1) as f32 / 1000.0;
    let dt = {
        let elapsed = physics.last_tick.as_ref().map(|prev| {
            // `Duration` here is azul's own (tick- or nanos-backed), so go
            // through `as_nanos` rather than std's `as_secs_f32`.
            #[allow(clippy::cast_precision_loss)] // a frame gap is far below f32's exact-integer range
            let ns = timer_info.frame_start.duration_since(prev).as_nanos() as f32;
            ns / 1_000_000_000.0
        });
        match elapsed {
            Some(e) if e.is_finite() && e > 0.0 => e.clamp(0.001, 0.050),
            // No previous tick, or a clock that did not advance: the configured
            // period is the best estimate available.
            _ => configured_dt,
        }
    };
    physics.last_tick = Some(timer_info.frame_start.clone());

    // Extract physics config values
    let sp = &physics.scroll_physics;
    let friction_rate = friction_from_deceleration(sp.deceleration_rate);
    let velocity_threshold = sp.min_velocity_threshold;
    let wheel_multiplier = sp.wheel_multiplier;
    // Sanitize: `.max(0.0)` turns a NaN or negative max_velocity (a plain repr(C)
    // field with no validation, reachable from a bad SystemStyle) into 0.0, so the
    // `velocity.clamp(-max_velocity, max_velocity)` below never hits f32::clamp's
    // `min > max` panic — which in this extern "C" callback would ABORT the process.
    let max_velocity = sp.max_velocity.max(0.0);
    let overscroll_elasticity = sp.overscroll_elasticity;
    let max_overscroll_distance = sp.max_overscroll_distance;
    let bounce_back_duration_ms = sp.bounce_back_duration_ms;
    let wheel_animate_bounce_ms = sp.wheel_animate_bounce_ms;

    // 0. Age the raw finger accumulators. One that saw no delta for
    // GESTURE_STALE_TICKS lost its `TrackpadEnd`: drop it, and give an
    // overshot node the spring-back the End would have armed.
    let mut stale: Vec<(DomId, NodeId)> = Vec::new();
    for (key, (_, idle)) in &mut physics.trackpad_raw_positions {
        *idle += 1;
        if *idle > GESTURE_STALE_TICKS {
            stale.push(*key);
        }
    }
    for key in stale {
        physics.trackpad_raw_positions.remove(&key);
        if let Some(info) = timer_info.get_scroll_node_info(key.0, key.1) {
            let ox = calculate_overshoot(info.current_offset.x, 0.0, info.max_scroll_x);
            let oy = calculate_overshoot(info.current_offset.y, 0.0, info.max_scroll_y);
            if ox.abs() > 0.01 || oy.abs() > 0.01 {
                let np = physics
                    .node_velocities
                    .entry(key)
                    .or_insert_with(NodeScrollPhysics::default);
                np.is_rubber_banding = true;
                // Same ownership rule as the `TrackpadEnd` arm below: the
                // spring has the axis, so a tail arriving on it is ignored.
                let latch = physics.momentum_latched.entry(key).or_default();
                latch.idle = 0;
                latch.x |= ox.abs() > 0.01;
                latch.y |= oy.abs() > 0.01;
            }
        }
    }

    // 0b. Age the momentum-ignore latches. The OS tail is dense right up to
    // its last delta, so this many empty ticks means it is genuinely over and
    // the next fling must not be swallowed by a leftover latch.
    physics.momentum_latched.retain(|_, latch| {
        latch.idle += 1;
        latch.idle <= MOMENTUM_LATCH_STALE_TICKS
    });

    // 1. Take at most MAX_SCROLL_EVENTS_PER_TICK recent inputs from the shared queue
    let inputs = physics.input_queue.take_recent(MAX_SCROLL_EVENTS_PER_TICK);

    for input in inputs {
        let key = (input.dom_id, input.node_id);
        match input.source {
            ScrollInputSource::TrackpadContinuous | ScrollInputSource::TrackpadMomentum => {
                let is_momentum = input.source == ScrollInputSource::TrackpadMomentum;
                // Once the rubber-band spring owns an axis, the OS momentum tail
                // for THAT axis is dropped: it knows nothing about our edge, and
                // a spring it kept killing was never seen (the view "wobbled back"
                // along the momentum decay instead).
                //
                // The decision comes from the stored per-axis latch, NOT from
                // whether the node happens to be overshooting right now — see
                // `momentum_latched`. The live-state version of this guard
                // vanished the instant the spring landed, which let the rest of
                // the tail start a second and third bounce.
                //
                // PER-AXIS. A macOS momentum NSEvent carries BOTH scrollingDeltaX
                // and scrollingDeltaY as one input. Dropping the WHOLE event once
                // EITHER axis latched the band froze the OTHER axis's fling — the
                // "an accidental X overscroll bounce kills the Y scroll" the user
                // reported. Mask only the latched axis; let the in-range axis keep
                // flinging. The banding axis re-arms its spring from the offset in
                // the integration loop regardless, so nothing is lost.
                let mut delta = input.delta;
                if is_momentum {
                    if let Some(latch) = physics.momentum_latched.get_mut(&key) {
                        // The tail is still running, so keep the latch alive.
                        latch.idle = 0;
                        if latch.x {
                            delta.x = 0.0;
                        }
                        if latch.y {
                            delta.y = 0.0;
                        }
                        if delta.x == 0.0 && delta.y == 0.0 {
                            // Both axes belong to the spring. CONSUME the event
                            // without applying it — dropping out here is what
                            // stops the endless re-stretching.
                            continue;
                        }
                    }
                } else {
                    // TrackpadContinuous: the finger is back down, so this is a
                    // NEW gesture and the previous tail's latch is void. (WebKit
                    // clears `ignore_momentum_scrolls_` at `PhaseBegan` for the
                    // same reason.)
                    physics.momentum_latched.remove(&key);
                }
                let info = timer_info.get_scroll_node_info(input.dom_id, input.node_id);

                // ACCUMULATE ON THE RAW FINGER OFFSET (see
                // `trackpad_raw_positions`), never on the displayed one. The
                // first delta of a gesture seeds it from the committed offset,
                // inverted through the band if that offset is already past an
                // edge. `current_offset` does NOT move while this callback
                // runs — the ScrollTo changes are applied after it returns —
                // so every event of a tick's batch accumulates here (N events
                // in one tick used to collapse to the LAST delta).
                let raw_base = physics
                    .trackpad_raw_positions
                    .get(&key)
                    .map(|(p, _)| *p)
                    .or_else(|| {
                        info.as_ref().map(|info| LogicalPosition {
                            x: rubber_band_unclamp(
                                info.current_offset.x,
                                0.0,
                                info.max_scroll_x,
                                max_overscroll_distance,
                                overscroll_elasticity,
                            ),
                            y: rubber_band_unclamp(
                                info.current_offset.y,
                                0.0,
                                info.max_scroll_y,
                                max_overscroll_distance,
                                overscroll_elasticity,
                            ),
                        })
                    })
                    .unwrap_or_default();
                let new_raw = LogicalPosition {
                    x: raw_base.x + delta.x,
                    y: raw_base.y + delta.y,
                };

                // The first MOMENTUM delta that pushes past an edge hands the
                // node to the spring: the delta's velocity is the bump, the
                // spring brings it back, later momentum deltas are dropped
                // above. A finger (Continuous) past the edge just stretches.
                let mut handed_to_spring = false;
                if is_momentum {
                    if let Some(info) = info.as_ref() {
                        let over_x = calculate_overshoot(new_raw.x, 0.0, info.max_scroll_x);
                        let over_y = calculate_overshoot(new_raw.y, 0.0, info.max_scroll_y);
                        if over_x.abs() > 0.01 || over_y.abs() > 0.01 {
                            let np = physics
                                .node_velocities
                                .entry(key)
                                .or_insert_with(NodeScrollPhysics::default);
                            np.velocity = LogicalPosition {
                                x: if over_x.abs() > 0.01 { delta.x / dt } else { 0.0 },
                                y: if over_y.abs() > 0.01 { delta.y / dt } else { 0.0 },
                            };
                            np.is_rubber_banding = true;
                            handed_to_spring = true;
                            // LATCH the axis that just hit the edge. From here
                            // the rest of this tail is consumed and discarded
                            // for that axis, however long it runs and however
                            // many times the spring lands in the meantime.
                            let latch = physics.momentum_latched.entry(key).or_default();
                            latch.idle = 0;
                            latch.x |= over_x.abs() > 0.01;
                            latch.y |= over_y.abs() > 0.01;
                        }
                    }
                }

                // Step 3 bands this for display; the raw value stays here.
                physics.pending_trackpad_positions.insert(key, new_raw);
                if handed_to_spring {
                    physics.trackpad_raw_positions.remove(&key);
                } else {
                    physics.trackpad_raw_positions.insert(key, (new_raw, 0));
                    // The finger overrides any momentum or spring in flight.
                    physics.node_velocities.remove(&key);
                }
            }
            ScrollInputSource::WheelDiscrete => {
                // Input provenance decides the math. A PHYSICAL wheel click
                // glides toward an accumulating ABSOLUTE target (short
                // critically-damped spring, hard stop on arrival) — running
                // discrete clicks through the velocity+friction model gave
                // them the trackpad's floaty momentum tail, which feels
                // jarring on a ratcheting wheel. Consecutive clicks extend
                // the target (glide keeps going), they don't stack impulses.
                // Touchpads that surface wheel-style events (Windows
                // precision-touchpad fallback, X11 smooth scrolling) and
                // test drivers keep the velocity model: their deltas are
                // fine-grained and the momentum tail is correct for them.
                let mut wheel_glided = false;
                if matches!(
                    input.device,
                    ScrollInputDevice::MouseWheel | ScrollInputDevice::Unknown
                ) {
                    if let Some(info) =
                        timer_info.get_scroll_node_info(input.dom_id, input.node_id)
                    {
                        let base = physics
                            .animate_targets
                            .get(&key)
                            .map_or(info.current_offset, |(t, _)| *t);
                        // Clamp into the scrollable range: a wheel click at
                        // the boundary must not build up an off-range target
                        // (chaining to the parent scroller happens at
                        // hit-test time in the ScrollManager, per click).
                        let target = LogicalPosition {
                            x: (base.x + input.delta.x * wheel_multiplier)
                                .clamp(0.0, info.max_scroll_x.max(0.0)),
                            y: (base.y + input.delta.y * wheel_multiplier)
                                .clamp(0.0, info.max_scroll_y.max(0.0)),
                        };
                        physics.animate_targets.insert(key, (target, input.device));
                        // Ensure the seek loop visits this node; keep any
                        // in-flight velocity so a retarget stays continuous.
                        physics.node_velocities.entry(key).or_default();
                        physics.pending_trackpad_positions.remove(&key);
                        physics.trackpad_raw_positions.remove(&key);
                        wheel_glided = true;
                    }
                }
                if !wheel_glided {
                    // Velocity impulse (trackpad-style wheel events, test
                    // drivers, or no scroll-node info registered yet).
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
            }
            ScrollInputSource::Programmatic => {
                // Programmatic: Set position directly. Accumulates within the
                // tick for the same reason as TrackpadContinuous above.
                let current = physics
                    .pending_positions
                    .get(&key)
                    .copied()
                    .or_else(|| {
                        timer_info
                            .get_scroll_node_info(input.dom_id, input.node_id)
                            .map(|info| info.current_offset)
                    })
                    .unwrap_or_default();

                let new_pos = LogicalPosition {
                    x: current.x + input.delta.x,
                    y: current.y + input.delta.y,
                };
                physics.pending_positions.insert(key, new_pos);
            }
            ScrollInputSource::AnimateTo => {
                // `delta` carries the ABSOLUTE target. Clamp into the node's
                // scrollable range; keep any existing velocity so a retarget
                // mid-flight stays continuous (the spring redirects it).
                if let Some(info) = timer_info.get_scroll_node_info(input.dom_id, input.node_id) {
                    let target = LogicalPosition {
                        x: input.delta.x.clamp(0.0, info.max_scroll_x.max(0.0)),
                        y: input.delta.y.clamp(0.0, info.max_scroll_y.max(0.0)),
                    };
                    physics.animate_targets.insert(key, (target, input.device));
                    physics.node_velocities.entry(key).or_default();
                    physics.pending_trackpad_positions.remove(&key);
                    physics.trackpad_raw_positions.remove(&key);
                }
            }
            ScrollInputSource::TrackpadEnd => {
                // The gesture is over: the raw accumulator goes with it (the
                // next gesture seeds afresh from the committed offset).
                physics.trackpad_raw_positions.remove(&key);
                // Trackpad gesture ended (fingers lifted).
                // If the scroll position is past the bounds (rubber-banding overshoot),
                // start a spring-back animation to snap back to the boundary.
                // Peek (do NOT remove) the position this tick has staged for
                // the node. It used to read `pending_positions`, the
                // PROGRAMMATIC map, which a trackpad gesture never writes — so
                // the overshoot decision was made from the stale, pre-tick
                // offset and the spring-back fought the finger's last delta.
                // Peeking rather than removing leaves step 3 to apply the
                // rubber-band clamp, which is the write that must win.
                let staged = physics
                    .pending_trackpad_positions
                    .get(&key)
                    .copied()
                    .or_else(|| physics.pending_positions.get(&key).copied());
                let already_staged = staged.is_some();
                let pos = staged
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
                            // A fresh arm starts from rest — the spring-back
                            // in the integration loop (step 2) pulls the
                            // position back to the boundary. A spring already
                            // in flight (a momentum bump) keeps its velocity:
                            // the momentum tail's own End arrives while it runs.
                            if !node_phys.is_rubber_banding {
                                node_phys.velocity = LogicalPosition::zero();
                            }
                            node_phys.is_rubber_banding = true;
                            // The spring owns the axis from this lift on, so the
                            // tail that follows is ignored on it. Releasing a
                            // stretched band snaps back — it must never stretch
                            // FURTHER because the OS is still replaying the
                            // finger's velocity. Same latch as the momentum
                            // hand-off, different entry point: what sets it is
                            // the spring taking ownership, not who handed over.
                            let latch = physics.momentum_latched.entry(key).or_default();
                            latch.idle = 0;
                            latch.x |= overshoot_x.abs() > 0.01;
                            latch.y |= overshoot_y.abs() > 0.01;
                        }

                        // Preserve the overshot position for the spring-back animation.
                        // Must use unclamped so the overshot position is NOT clamped to bounds.
                        //
                        // Skipped when step 3 is already going to write this
                        // node from a staged position: that write is the
                        // rubber-band-clamped one and must be the only one, or
                        // the node gets two conflicting offsets in one tick.
                        if !already_staged {
                            let hierarchy_id =
                                NodeHierarchyItemId::from_crate_internal(Some(input.node_id));
                            timer_info.scroll_to_unclamped(input.dom_id, hierarchy_id, pos);
                        }
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
    // AnimateTo targets, read-only during the iteration (the map itself is
    // mutated after the loop via `converged_targets`).
    let animate_targets = physics.animate_targets.clone();
    let mut converged_targets: Vec<(DomId, NodeId)> = Vec::new();
    // Nodes the finger moved THIS tick. Both writers below end in a
    // `scroll_to_unclamped` for the same node and the velocity one is applied
    // last, so without this the spring silently overwrote the gesture's delta
    // with a position integrated from the STALE pre-tick offset — the direct
    // "physics fighting the actual scroll" the user reported. The finger wins
    // while it is down; the spring resumes next tick from the committed offset.
    let moved_by_finger_this_tick: alloc::collections::BTreeSet<(DomId, NodeId)> =
        physics.pending_trackpad_positions.keys().copied().collect();

    for ((dom_id, node_id), node_physics) in &mut physics.node_velocities {
        if moved_by_finger_this_tick.contains(&(*dom_id, *node_id)) {
            continue;
        }
        // Get current scroll info for clamping and per-node CSS config
        let Some(info) = timer_info.get_scroll_node_info(*dom_id, *node_id) else {
            continue;
        };

        // The displacement this tick commits, per axis. `None` means plain
        // momentum: `velocity · dt`. A spring (seek or rubber band) on an
        // axis sets it to the EXACT position change of its closed-form step
        // — see `critically_damped_step` for why the spring must not go
        // through `v += F·dt; x += v·dt`.
        let mut spring_disp_x: Option<f32> = None;
        let mut spring_disp_y: Option<f32> = None;
        // The rubber-band spring's EXACT new overshoot on an axis it ran on
        // this tick. Committed directly — see `commit_spring_back`.
        let mut spring_back_x: Option<f32> = None;
        let mut spring_back_y: Option<f32> = None;

        // Target-seeking spring (scroll_to_animated): a critically-damped
        // pull toward the absolute target — the same spring the rubber band
        // uses, with the displacement measured from the TARGET instead of
        // the boundary. Close enough + slow enough snaps EXACTLY onto the
        // target and retires it (no asymptotic crawl).
        let seek_target = animate_targets.get(&(*dom_id, *node_id)).copied();
        if let Some((target, seek_device)) = seek_target {
            let err_x = info.current_offset.x - target.x;
            let err_y = info.current_offset.y - target.y;
            if err_x.abs() < 0.5
                && err_y.abs() < 0.5
                && node_physics.velocity.x.abs() < velocity_threshold
                && node_physics.velocity.y.abs() < velocity_threshold
            {
                velocity_updates.push(((*dom_id, *node_id), target));
                node_physics.velocity = LogicalPosition::zero();
                converged_targets.push((*dom_id, *node_id));
                continue;
            }
            // Provenance picks the curve: wheel clicks want a short snappy
            // glide, programmatic/other seeks the platform bounce duration.
            let seek_duration_ms = match seek_device {
                ScrollInputDevice::MouseWheel | ScrollInputDevice::Unknown => {
                    wheel_animate_bounce_ms
                }
                _ => bounce_back_duration_ms,
            };
            let omega = spring_constant_from_bounce_duration(seek_duration_ms).sqrt();
            let (err_x_after, vx) =
                critically_damped_step(err_x, node_physics.velocity.x, omega, dt);
            let (err_y_after, vy) =
                critically_damped_step(err_y, node_physics.velocity.y, omega, dt);
            node_physics.velocity = LogicalPosition { x: vx, y: vy };
            spring_disp_x = Some(err_x_after - err_x);
            spring_disp_y = Some(err_y_after - err_y);
        }

        // Determine if this node allows rubber-banding
        let rubber_band_x = node_allows_rubber_band(info.max_scroll_x, info.overscroll_behavior_x, info.overflow_scrolling, overscroll_elasticity);
        let rubber_band_y = node_allows_rubber_band(info.max_scroll_y, info.overscroll_behavior_y, info.overflow_scrolling, overscroll_elasticity);

        // Calculate current overshoot amounts
        let overshoot_x = calculate_overshoot(info.current_offset.x, 0.0, info.max_scroll_x);
        let overshoot_y = calculate_overshoot(info.current_offset.y, 0.0, info.max_scroll_y);

        let is_overshooting_x = overshoot_x.abs() > 0.01;
        let is_overshooting_y = overshoot_y.abs() > 0.01;

        // If we're in a rubber-band overshoot, run the critically-damped
        // spring-back: the overshoot IS the displacement, and one exact step
        // gives both the new overshoot and the new velocity (no oscillation
        // at any tick length — the Euler form rang at the Windows preset's
        // 200 ms bounce and diverged at the 50 ms floor).
        if is_overshooting_x && rubber_band_x {
            let omega = spring_constant_from_bounce_duration(bounce_back_duration_ms).sqrt();
            let (overshoot_after, vx) =
                critically_damped_step(overshoot_x, node_physics.velocity.x, omega, dt);
            node_physics.velocity.x = vx;
            spring_disp_x = Some(overshoot_after - overshoot_x);
            spring_back_x = Some(overshoot_after);
            node_physics.is_rubber_banding = true;
        }
        if is_overshooting_y && rubber_band_y {
            let omega = spring_constant_from_bounce_duration(bounce_back_duration_ms).sqrt();
            let (overshoot_after, vy) =
                critically_damped_step(overshoot_y, node_physics.velocity.y, omega, dt);
            node_physics.velocity.y = vy;
            spring_disp_y = Some(overshoot_after - overshoot_y);
            spring_back_y = Some(overshoot_after);
            node_physics.is_rubber_banding = true;
        }

        // Skip if velocity is negligible and not rubber-banding or seeking
        if !node_physics.is_rubber_banding
            && seek_target.is_none()
            && node_physics.velocity.x.abs() < velocity_threshold
            && node_physics.velocity.y.abs() < velocity_threshold
        {
            node_physics.velocity = LogicalPosition::zero();
            continue;
        }

        // Apply velocity to position. A spring axis commits its exact step;
        // a free-momentum axis integrates `v · dt`.
        let displacement = LogicalPosition {
            x: spring_disp_x.unwrap_or(node_physics.velocity.x * dt),
            y: spring_disp_y.unwrap_or(node_physics.velocity.y * dt),
        };

        let raw_new_x = info.current_offset.x + displacement.x;
        let raw_new_y = info.current_offset.y + displacement.y;

        // The whole jitter question in one line: what this tick READ, what the
        // writers wanted, and what it is about to COMMIT. If the offset a tick
        // reads is not the offset the previous tick wrote, the spring is
        // integrating from a stale base — and that is the oscillation.
        #[cfg(feature = "std")]
        if scroll_debug_enabled() {
            std::eprintln!(
                "[az-scroll] TICK node=({:?},{:?}) read=({:.3},{:.3}) vel=({:.3},{:.3}) \
                 disp=({:.3},{:.3}) -> commit=({:.3},{:.3}) target={:?} max=({:.1},{:.1})",
                dom_id, node_id,
                info.current_offset.x, info.current_offset.y,
                node_physics.velocity.x, node_physics.velocity.y,
                displacement.x, displacement.y,
                raw_new_x, raw_new_y,
                seek_target.map(|(t, _)| (t.x, t.y)),
                info.max_scroll_x, info.max_scroll_y,
            );
        }

        // Commit. An axis the rubber-band spring ran on commits the spring's
        // OWN output (`commit_spring_back`) — the band models the finger's
        // resistance, not the spring; passing the spring's exact step through
        // `rubber_band_clamp` again shrank the overshoot ~70 % per tick on top
        // of the spring's own ~3 %, turning the configured 400 ms bounce into
        // a 3-frame snap and leaking the crossing velocity into a free drift
        // into the content. Free momentum reaching an edge is banded as
        // before; next tick the spring takes over with that velocity (the
        // natural bump) and commits directly from then on.
        let new_x = match spring_back_x {
            Some(after) => commit_spring_back(
                overshoot_x,
                after,
                info.max_scroll_x,
                max_overscroll_distance,
                &mut node_physics.velocity.x,
            ),
            None if rubber_band_x && max_overscroll_distance > 0.0 => {
                // Allow overshoot with diminishing returns (elasticity)
                rubber_band_clamp(raw_new_x, 0.0, info.max_scroll_x, max_overscroll_distance, overscroll_elasticity)
            }
            None => raw_new_x.clamp(0.0, info.max_scroll_x),
        };

        let new_y = match spring_back_y {
            Some(after) => commit_spring_back(
                overshoot_y,
                after,
                info.max_scroll_y,
                max_overscroll_distance,
                &mut node_physics.velocity.y,
            ),
            None if rubber_band_y && max_overscroll_distance > 0.0 => {
                rubber_band_clamp(raw_new_y, 0.0, info.max_scroll_y, max_overscroll_distance, overscroll_elasticity)
            }
            None => raw_new_y.clamp(0.0, info.max_scroll_y),
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

        // NOTE: the (0.01, 0.5) px gap between this flag-clear and the
        // `TrackpadEnd` arm was once suspected of letting a late End restart a
        // landed bounce. A device trace refuted it — `band` was true on every
        // tick of a three-bounce gesture and neither TrackpadEnd lined up with
        // a restart. The cause was the momentum tail being re-admitted once the
        // node's physics entry had been evicted; see `momentum_latched`.
        if new_overshoot_x.abs() < 0.5 && new_overshoot_y.abs() < 0.5 {
            node_physics.is_rubber_banding = false;
        }

        // Snap to zero if below threshold after decay — FREE MOMENTUM ONLY.
        // A spring's velocity is meaningful all the way down to its landing:
        // zeroing it once it dropped under the threshold restarted the
        // closed form from rest on every tick, and the last ~2 px of a
        // spring-back became a 50-tick crawl (`commit_spring_back` lands it).
        if spring_back_x.is_none() && node_physics.velocity.x.abs() < velocity_threshold {
            node_physics.velocity.x = 0.0;
        }
        if spring_back_y.is_none() && node_physics.velocity.y.abs() < velocity_threshold {
            node_physics.velocity.y = 0.0;
        }

        velocity_updates.push(((*dom_id, *node_id), new_pos));
    }

    // Retire converged AnimateTo targets (snapped exactly this tick).
    for key in converged_targets {
        physics.animate_targets.remove(&key);
    }

    // Clean up nodes with zero velocity that are neither rubber-banding nor
    // still SEEKING a target. The seek spring is integrated from the
    // `node_velocities` entry, so evicting a seeking node the moment its
    // velocity dips under the threshold froze the glide short of its target
    // (119.2 of 120 px) with the target still armed: `is_active()` kept the
    // timer ticking against a node the loop no longer visited, and the next
    // wheel click extended a target the view never reached. The Euler
    // integrator's ringing kept |v| large until arrival and hid this; a
    // proper critically-damped tail is slow, and lands in exactly this gap.
    let seeking: alloc::collections::BTreeSet<(DomId, NodeId)> =
        physics.animate_targets.keys().copied().collect();
    physics.node_velocities.retain(|key, v| {
        v.velocity.x.abs() > 0.0
            || v.velocity.y.abs() > 0.0
            || v.is_rubber_banding
            || seeking.contains(key)
    });

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

        // A VirtualView materialises only the rows around the CURRENT offset,
        // so moving that offset must re-invoke its callback. The discrete
        // ScrollTo path does this (check_and_queue_virtual_view_reinvoke in
        // common/event.rs), but SMOOTH scrolling never did — and smooth is what
        // a wheel or trackpad actually produces. The result: the pages scrolled
        // while the VirtualView stayed frozen on its first window, so scrolling
        // past the materialised rows showed empty background and the view never
        // "caught up".
        //
        // Targeted rather than trigger_all_virtual_view_rerender(): this runs on
        // every physics tick of every scrolling node, and re-materialising every
        // VirtualView in the window 60 times a second is not the same cost.
        // Nodes that are not VirtualViews are ignored downstream.
        timer_info
            .callback_info
            .trigger_virtual_view_rerender(dom_id, node_id);

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

/// Inverse of [`rubber_band_clamp`]: the RAW (unbanded) position that would
/// display as `displayed`.
///
/// In range it is the identity; past an edge it
/// inverts `D(o) = M·(1 − e^(−e·o/M))` into `o = −(M/e)·ln(1 − shown/M)`.
/// The band's asymptote `M` is unreachable, so `shown` is capped just inside
/// it. Used to seed a gesture's raw accumulator from a committed offset that
/// is already overscrolled (a second gesture starting mid-stretch).
fn rubber_band_unclamp(
    displayed: f32,
    min: f32,
    max: f32,
    max_overscroll: f32,
    elasticity: f32,
) -> f32 {
    if displayed >= min && displayed <= max {
        return displayed;
    }
    if max_overscroll <= 0.0 || elasticity <= 0.0 {
        return displayed.clamp(min, max);
    }
    let (boundary, shown) = if displayed < min {
        (min, min - displayed)
    } else {
        (max, displayed - max)
    };
    let frac = (shown / max_overscroll).clamp(0.0, 0.999);
    let raw = -(max_overscroll / elasticity) * (1.0 - frac).ln();
    if displayed < min {
        boundary - raw
    } else {
        boundary + raw
    }
}

/// The offset a rubber-band spring-back axis commits this tick, given the
/// overshoot it started from and the exact overshoot its closed-form step
/// produced.
///
/// Lands EXACTLY: a step that crosses the boundary (only the
/// band/threshold interplay can make a critically-damped spring do that)
/// or ends within half a pixel snaps onto the boundary with the velocity
/// zeroed — never a crossing velocity falling through to free momentum
/// (~60 px of drift into the content after a 40 px release), never a view
/// parked at `max + 0.3` forever. Bounded by the band's envelope.
fn commit_spring_back(
    overshoot_before: f32,
    overshoot_after: f32,
    max_scroll: f32,
    max_overscroll: f32,
    velocity: &mut f32,
) -> f32 {
    let boundary = if overshoot_before > 0.0 { max_scroll } else { 0.0 };
    let crossed = overshoot_after != 0.0 && overshoot_after.signum() != overshoot_before.signum();
    if crossed || overshoot_after.abs() < 0.5 {
        *velocity = 0.0;
        boundary
    } else {
        boundary + overshoot_after.clamp(-max_overscroll, max_overscroll)
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

/// One EXACT step of a critically-damped spring pulling a displacement back
/// to zero.
///
/// `x` is the displacement from the rest position (the overshoot past an
/// edge, or the distance left to an animate-to target), `v` the velocity,
/// `omega` the spring's natural frequency (`sqrt(k)`), `dt` the step. Returns
/// `(x, v)` at the end of the step, from the closed-form solution of
/// `x'' + 2ω·x' + ω²·x = 0`:
///
/// ```text
/// x(t) = (x₀ + (v₀ + ω·x₀)·t) · e^(−ωt)
/// v(t) = (v₀ − (v₀ + ω·x₀)·ω·t) · e^(−ωt)
/// ```
///
/// WHY NOT `v += (−k·x − c·v)·dt; x += v·dt`: explicit Euler on this spring
/// is stable only for `c·dt < 2` and monotone only for `c·dt < 1`. The wheel
/// glide (120 ms spring, `ω ≈ 52/s`, `c = 2ω`) at the 16 ms tick sits at
/// `c·dt ≈ 1.68`, so `v ← v·(1 − c·dt)` FLIPPED the velocity's sign on every
/// tick: a 120 px wheel flick went 0 → 84 → 55 → 119 → 78 → 134 → 88 → …
/// ringing around its target instead of landing on it — the "smooths, then
/// jumps back and forward" reported on X11/Wayland, where a physical wheel
/// is the everyday device (a trackpad never enters the spring, which is why
/// macOS looked fine). The 50 ms minimum duration is worse still
/// (`c·dt = 4`, divergent). The closed form is exact for any `dt` and any
/// stiffness, and its cost is one `exp`.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma
fn critically_damped_step(x: f32, v: f32, omega: f32, dt: f32) -> (f32, f32) {
    let decay = (-omega * dt).exp();
    let a = v + omega * x;
    ((x + a * dt) * decay, (v - a * omega * dt) * decay)
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
                TimerCallbackInfo::create(info, OptionDomNodeId::None, advance_clock(1), 0, false);
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
            // Every committed offset is now accompanied by an
            // UpdateVirtualView for the same node: a VirtualView materialises
            // only the rows around the current offset, so moving the offset
            // without re-invoking it leaves the view frozen on its first
            // window — pages scroll past and show empty background. Skip those
            // here; `scroll_commits_pair_with_a_virtual_view_retrigger` below
            // asserts the pairing itself.
            self.take_changes()
                .iter()
                .filter(|c| !matches!(c, CallbackChange::UpdateVirtualView { .. }))
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
    /// Nominal period of the physics pump, in ms — what
    /// `ScrollPhysics::timer_interval_ms` configures.
    const TICK_MS: u64 = 16;

    /// Advance the harness clock by `ticks` pump periods and read it back.
    ///
    /// This drives the ENGINE'S OWN frozen test clock
    /// (`azul_core::task::{freeze_test_clock, advance_test_clock_ms}`) — the
    /// same one E2E scenarios advance with `tick_ms` — rather than a private
    /// counter. One clock model, shared by the scenario runner and these unit
    /// tests, and it is a WALL clock, which is what `Instant::now()` answers on
    /// every desktop target. A private tick counter would have quantised time
    /// to 1/60 s and so could never express the sub-interval case that
    /// `Timer::invoke`'s gate turns on.
    ///
    /// Freezing matters as much as advancing: without it the real component of
    /// `Instant::now()` keeps flowing, so elapsed time becomes (virtual) +
    /// (however long this build under this load took), and a timing assertion
    /// flips between runs on a loaded machine while passing in isolation.
    ///
    /// The physics integrates over the time it is actually handed, so a harness
    /// that stamped a live `Instant::now()` per call would give it the
    /// microseconds between two statements in a tight loop — dt would clamp to
    /// its 1 ms floor and every spring would crawl. The old fixed
    /// `timer_interval_ms` dt hid that this harness had no clock model at all.
    fn advance_clock(ticks: u64) -> Instant {
        azul_core::task::freeze_test_clock();
        let _ = azul_core::task::advance_test_clock_ms(ticks.saturating_mul(TICK_MS));
        Instant::now()
    }

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

    /// A scroll input for node `idx` of the root DOM (device = TestDriver,
    /// which keeps the legacy velocity model on WheelDiscrete).
    fn input(idx: usize, delta: (f32, f32), source: ScrollInputSource) -> ScrollInput {
        input_dev(idx, delta, source, ScrollInputDevice::TestDriver)
    }

    /// A scroll input with an explicit device provenance.
    fn input_dev(
        idx: usize,
        delta: (f32, f32),
        source: ScrollInputSource,
        device: ScrollInputDevice,
    ) -> ScrollInput {
        ScrollInput {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(idx),
            delta: LogicalPosition::new(delta.0, delta.1),
            timestamp: Instant::now(),
            source,
            device,
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
            wheel_animate_bounce_ms: 0,
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
    // A NaN or negative `max_velocity` (ScrollPhysics is a plain repr(C) struct
    // with no validation, so a bad SystemStyle can supply one) used to reach
    // `velocity.clamp(-max_velocity, max_velocity)` with min > max and panic
    // inside f32::clamp — which, in the extern "C" `scroll_physics_timer_callback`,
    // ABORTS the process. The branch now sanitizes with `.max(0.0)`; these tests
    // pin that the sanitized bound yields a safe clamp.
    // ------------------------------------------------------------------

    #[test]
    fn nan_max_velocity_is_sanitized_to_a_safe_clamp() {
        let max_velocity = ScrollPhysics {
            max_velocity: f32::NAN,
            ..ScrollPhysics::default()
        }
        .max_velocity
        .max(0.0);
        assert_eq!(max_velocity, 0.0);
        assert_eq!((600.0_f32).clamp(-max_velocity, max_velocity), 0.0);
    }

    #[test]
    fn negative_max_velocity_is_sanitized_to_a_safe_clamp() {
        let max_velocity = ScrollPhysics {
            max_velocity: -1.0,
            ..ScrollPhysics::default()
        }
        .max_velocity
        .max(0.0);
        assert_eq!(max_velocity, 0.0);
        assert_eq!((600.0_f32).clamp(-max_velocity, max_velocity), 0.0);
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

    // ==================================================================
    // AnimateTo — target-seeking spring (scroll_to_animated)
    // ==================================================================
    //
    // NOTE: the window's scroll offset is NOT advanced between ticks in
    // this harness (ScrollTo changes are applied by the event loop in
    // production), so multi-tick assertions here are about VELOCITY
    // continuity and single-tick outputs, both offset-independent.

    #[test]
    fn animate_to_first_tick_glides_instead_of_jumping() {
        let (mut data, queue) = state_with(ScrollPhysics::default());
        // Viewport 100x100 over 100x500 -> max_scroll_y = 400.
        queue.push(input(3, (0.0, 400.0), ScrollInputSource::AnimateTo));
        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let ret = env.tick(&data);
                assert_eq!(ret.should_terminate, TerminateTimer::Continue);
                let tos = env.take_scroll_tos();
                assert_eq!(tos.len(), 1, "one node moved: {tos:?}");
                let (idx, pos, _unclamped) = tos[0];
                assert_eq!(idx, 3);
                assert!(
                    pos.y > 0.0 && pos.y < 400.0,
                    "an animated scroll SEEKS the target across ticks instead of \
                     teleporting (Programmatic behavior): first tick landed at {pos:?}"
                );
            },
        );
        with_state(&mut data, |st| {
            assert!(
                st.animate_targets.contains_key(&key(3)),
                "the target stays armed until convergence"
            );
        });
    }

    #[test]
    fn animate_to_snaps_exactly_onto_the_target_and_retires_it() {
        let (mut data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(3, (0.0, 400.0), ScrollInputSource::AnimateTo));
        with_env(
            |w| {
                register_node(w, 3, (100.0, 100.0), (100.0, 500.0));
                // Start 0.2px short of the target with no velocity: the
                // convergence branch must snap to EXACTLY 400 (no
                // asymptotic crawl) and retire the target.
                w.scroll_manager.set_scroll_position(
                    DomId::ROOT_ID,
                    NodeId::new(3),
                    LogicalPosition::new(0.0, 399.8),
                    Instant::now(),
                );
            },
            |env| {
                let _ = env.tick(&data);
                let tos = env.take_scroll_tos();
                assert_eq!(tos.len(), 1, "{tos:?}");
                let (_, pos, _) = tos[0];
                assert!(
                    (pos.y - 400.0).abs() < f32::EPSILON,
                    "convergence snaps to the exact target, got {pos:?}"
                );
            },
        );
        with_state(&mut data, |st| {
            assert!(
                st.animate_targets.is_empty(),
                "a converged target is retired"
            );
        });
    }

    #[test]
    fn animate_to_retarget_keeps_the_current_velocity() {
        let (data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(3, (0.0, 400.0), ScrollInputSource::AnimateTo));
        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let _ = env.tick(&data);
                let first = env.take_scroll_tos();
                let (_, p1, _) = first[0];

                // Retarget mid-flight to the opposite direction.
                queue.push(input(3, (0.0, 0.0), ScrollInputSource::AnimateTo));
                let _ = env.tick(&data);
                let second = env.take_scroll_tos();
                let (_, p2, _) = second[0];

                // The spring must REDIRECT the existing velocity, not
                // restart from rest: the second position stays continuous
                // with the first (still near/above it), it does not
                // teleport toward the new target.
                assert!(
                    p2.y > 0.0 && (p2.y - p1.y).abs() < p1.y.max(1.0) * 4.0,
                    "retarget must stay continuous: first {p1:?}, second {p2:?}"
                );
            },
        );
    }

    // ==================================================================
    // WheelDiscrete provenance — physical wheel = target glide,
    // everything else keeps the velocity model
    // ==================================================================

    #[test]
    fn physical_wheel_click_arms_an_absolute_target_glide() {
        let (mut data, queue) = state_with(ScrollPhysics::default());
        // Viewport 100x100 over 100x500 -> max_scroll_y = 400.
        queue.push(input_dev(
            3,
            (0.0, 30.0),
            ScrollInputSource::WheelDiscrete,
            ScrollInputDevice::MouseWheel,
        ));
        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let ret = env.tick(&data);
                assert_eq!(ret.should_terminate, TerminateTimer::Continue);
                let tos = env.take_scroll_tos();
                assert_eq!(tos.len(), 1, "one node moved: {tos:?}");
                let (_, pos, _) = tos[0];
                assert!(
                    pos.y > 0.0 && pos.y < 30.0,
                    "a wheel click GLIDES toward its target (default \
                     wheel_multiplier = 1.0 -> target y = 30), it neither \
                     teleports nor overshoots on the first tick: {pos:?}"
                );
            },
        );
        with_state(&mut data, |st| {
            let (target, device) = st.animate_targets[&key(3)];
            assert_eq!(
                target.y, 30.0,
                "target = current offset + delta * wheel_multiplier"
            );
            assert_eq!(device, ScrollInputDevice::MouseWheel);
        });
    }

    #[test]
    fn consecutive_wheel_clicks_extend_the_target_instead_of_stacking_impulses() {
        let (mut data, queue) = state_with(ScrollPhysics::default());
        queue.push(input_dev(
            3,
            (0.0, 30.0),
            ScrollInputSource::WheelDiscrete,
            ScrollInputDevice::MouseWheel,
        ));
        queue.push(input_dev(
            3,
            (0.0, 30.0),
            ScrollInputSource::WheelDiscrete,
            ScrollInputDevice::MouseWheel,
        ));
        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let _ = env.tick(&data);
            },
        );
        with_state(&mut data, |st| {
            let (target, _) = st.animate_targets[&key(3)];
            assert_eq!(
                target.y, 60.0,
                "the second click extends the FIRST click's target (30 + 30), \
                 it does not restart from the current offset"
            );
        });
    }

    #[test]
    fn wheel_click_target_is_clamped_to_the_scrollable_range() {
        let (mut data, queue) = state_with(ScrollPhysics::default());
        // max_scroll_y = 400; one huge click must not build an off-range target.
        queue.push(input_dev(
            3,
            (0.0, 10_000.0),
            ScrollInputSource::WheelDiscrete,
            ScrollInputDevice::MouseWheel,
        ));
        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let _ = env.tick(&data);
            },
        );
        with_state(&mut data, |st| {
            let (target, _) = st.animate_targets[&key(3)];
            assert_eq!(target.y, 400.0, "target clamps to max_scroll_y");
        });
    }

    #[test]
    fn test_driver_wheel_keeps_the_velocity_model() {
        let (mut data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(3, (0.0, 30.0), ScrollInputSource::WheelDiscrete));
        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let _ = env.tick(&data);
            },
        );
        with_state(&mut data, |st| {
            assert!(
                st.animate_targets.is_empty(),
                "TestDriver wheel events must stay on the deterministic \
                 velocity model (e2e harness contract), not the glide"
            );
            let v = st.node_velocities[&key(3)].velocity;
            assert!(v.y > 0.0, "impulse recorded as velocity: {v:?}");
        });
    }

    #[test]
    fn wheel_glide_uses_the_short_wheel_spring_not_the_bounce_spring() {
        // Same geometry, same 400px seek: node 3 via physical-wheel glide
        // (wheel_animate_bounce_ms = 60, stiff), node 4 via AnimateTo from a
        // test driver (bounce_back_duration_ms = 5000, soft). The stiffer
        // wheel spring must pull farther on the first tick.
        let physics = ScrollPhysics {
            wheel_animate_bounce_ms: 60,
            bounce_back_duration_ms: 5000,
            ..ScrollPhysics::default()
        };
        let (data, queue) = state_with(physics);
        queue.push(input_dev(
            3,
            (0.0, 400.0),
            ScrollInputSource::WheelDiscrete,
            ScrollInputDevice::MouseWheel,
        ));
        queue.push(input(4, (0.0, 400.0), ScrollInputSource::AnimateTo));
        with_env(
            |w| {
                register_node(w, 3, (100.0, 100.0), (100.0, 500.0));
                register_node(w, 4, (100.0, 100.0), (100.0, 500.0));
            },
            |env| {
                let _ = env.tick(&data);
                let tos = env.take_scroll_tos();
                let wheel_y = tos.iter().find(|(i, ..)| *i == 3).map(|(_, p, _)| p.y);
                let bounce_y = tos.iter().find(|(i, ..)| *i == 4).map(|(_, p, _)| p.y);
                let (Some(wheel_y), Some(bounce_y)) = (wheel_y, bounce_y) else {
                    panic!("both nodes must move on the first tick: {tos:?}");
                };
                assert!(
                    wheel_y > bounce_y,
                    "provenance picks the spring: wheel glide (60ms) must be \
                     snappier than the bounce-duration seek (5000ms); \
                     wheel {wheel_y} vs bounce {bounce_y}"
                );
            },
        );
    }

    // ==================================================================
    // REGRESSION (B1): the physics must not fight the real scroll
    // ==================================================================
    //
    // Reported on macOS: "physics based scrolling probably leading to external
    // scroll events and constantly fighting with the actual scroll".
    //
    // The harness above deliberately freezes the `LayoutWindow` behind a shared
    // reference, so it can only inspect the `ScrollTo` changes a tick EMITS —
    // it never applies them back. That is exactly why the bugs below were
    // invisible to the suite: they only show up once the loop is CLOSED, i.e.
    // once tick N+1 reads the offset tick N wrote. `closed_loop` does that.

    /// One tick with the loop closed: run the callback, then apply every
    /// emitted `ScrollTo` back into the `ScrollManager`, exactly as
    /// `dll/src/desktop/shell2/common/event.rs` does after a callback returns.
    /// Returns the `(node index, position, unclamped)` triples of that tick.
    fn closed_loop_tick(
        layout_window: &mut LayoutWindow,
        data: &RefAny,
    ) -> Vec<(usize, LogicalPosition, bool)> {
        closed_loop_tick_full(layout_window, data).0
    }

    /// [`closed_loop_tick`] with an explicit tick SPACING, for tests that model
    /// a jittering timer rather than a well-behaved 60 Hz one.
    fn closed_loop_tick_spaced(
        layout_window: &mut LayoutWindow,
        data: &RefAny,
        ticks: u64,
    ) -> Vec<(usize, LogicalPosition, bool)> {
        // The default path advances one tick; ask for the remainder up front so
        // this call sees a gap of `ticks`.
        if ticks > 1 {
            let _ = advance_clock(ticks - 1);
        }
        closed_loop_tick_full(layout_window, data).0
    }

    /// [`closed_loop_tick`] plus the timer's own verdict (continue / terminate).
    fn closed_loop_tick_full(
        layout_window: &mut LayoutWindow,
        data: &RefAny,
    ) -> (Vec<(usize, LogicalPosition, bool)>, TerminateTimer) {
        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let verdict = {
            let renderer_resources = RendererResources::default();
            let previous_window_state: Option<FullWindowState> = None;
            let current_window_state = FullWindowState::default();
            let gl_context = OptionGlContextPtr::None;
            let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
                BTreeMap::new();
            let window_handle = RawWindowHandle::Unsupported;
            let system_callbacks = ExternalSystemCallbacks::rust_internal();
            let ref_data = CallbackInfoRefData {
                layout_window: &*layout_window,
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
            let info = CallbackInfo::new(
                &ref_data,
                &changes,
                DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::NONE,
                },
                OptionLogicalPosition::None,
                OptionLogicalPosition::None,
            );
            let timer_info =
                TimerCallbackInfo::create(info, OptionDomNodeId::None, advance_clock(1), 0, false);
            scroll_physics_timer_callback(data.clone(), timer_info).should_terminate
        };

        let emitted: Vec<(usize, LogicalPosition, bool)> = changes
            .lock()
            .map(|c| {
                c.iter()
                    // Companion of every commit — see the note on the other
                    // drain helper above.
                    .filter(|ch| !matches!(ch, CallbackChange::UpdateVirtualView { .. }))
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
                        (
                            node_id
                                .into_crate_internal()
                                .expect("ScrollTo must name a concrete node")
                                .index(),
                            *position,
                            *unclamped,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (idx, position, unclamped) in &emitted {
            let node = NodeId::new(*idx);
            if *unclamped {
                layout_window.scroll_manager.set_scroll_position_unclamped(
                    DomId::ROOT_ID,
                    node,
                    *position,
                    Instant::now(),
                );
            } else {
                layout_window.scroll_manager.set_scroll_position(
                    DomId::ROOT_ID,
                    node,
                    *position,
                    Instant::now(),
                );
            }
        }
        (emitted, verdict)
    }

    fn offset_of(layout_window: &LayoutWindow, idx: usize) -> LogicalPosition {
        layout_window
            .scroll_manager
            .get_current_offset(DomId::ROOT_ID, NodeId::new(idx))
            .unwrap_or_default()
    }

    /// A committed scroll offset must be accompanied by a VirtualView
    /// re-trigger for the same node.
    ///
    /// A VirtualView materialises only the rows around the CURRENT offset. The
    /// discrete ScrollTo path re-invokes it (check_and_queue_virtual_view_reinvoke
    /// in dll/.../common/event.rs), but the SMOOTH physics path never did — and
    /// smooth is what a wheel or trackpad actually produces. AzWriter showed the
    /// result: the pages scrolled, the VirtualView stayed frozen on its first
    /// window, and scrolling past the materialised pages showed bare background
    /// that never filled in.
    #[test]
    fn a_committed_scroll_offset_retriggers_that_nodes_virtual_view() {
        let (data, queue) = state_with(ScrollPhysics::default());
        queue.push(input(3, (0.0, 120.0), ScrollInputSource::WheelDiscrete));

        with_env(
            |w| register_node(w, 3, (100.0, 100.0), (100.0, 500.0)),
            |env| {
                let _ = env.tick(&data);
                let changes = env.take_changes();

                let scrolled: Vec<usize> = changes
                    .iter()
                    .filter_map(|c| match c {
                        CallbackChange::ScrollTo { node_id, .. } => {
                            node_id.into_crate_internal().map(|n| n.index())
                        }
                        _ => None,
                    })
                    .collect();
                assert!(
                    !scrolled.is_empty(),
                    "the wheel delta committed no offset, so this test proves nothing"
                );

                for idx in scrolled {
                    assert!(
                        changes.iter().any(|c| matches!(
                            c,
                            CallbackChange::UpdateVirtualView { node_id, .. }
                                if node_id.index() == idx
                        )),
                        "node {idx} committed a new scroll offset with no \
                         UpdateVirtualView beside it — a VirtualView on that node \
                         would stay frozen on its first window while the content \
                         scrolled past it. changes={changes:?}"
                    );
                }
            },
        );
    }

    /// REGRESSION (B1): one finger gesture must land on a STABLE offset.
    ///
    /// Drives five 10px trackpad deltas plus the gesture end, then lets the
    /// physics run 120 ticks with the loop closed. The offset must settle and
    /// stay settled — no oscillation, no drift.
    #[test]
    fn a_single_trackpad_gesture_converges_to_a_stable_offset() {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 1000.0));
        let queue = layout_window.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(
            queue.clone(),
            ScrollPhysics::default(),
        ));

        for _ in 0..5 {
            queue.push(input_dev(
                1,
                (0.0, 10.0),
                ScrollInputSource::TrackpadContinuous,
                ScrollInputDevice::Touchpad,
            ));
        }
        queue.push(input_dev(
            1,
            (0.0, 0.0),
            ScrollInputSource::TrackpadEnd,
            ScrollInputDevice::Touchpad,
        ));

        let mut trace = Vec::new();
        for _ in 0..120 {
            let _ = closed_loop_tick(&mut layout_window, &data);
            trace.push(offset_of(&layout_window, 1).y);
        }

        let settled = trace[trace.len() - 1];
        // Five 10px deltas, well inside the 900px range: the gesture must land
        // on their SUM. Losing deltas (they used to overwrite each other inside
        // a tick) shows up here as a smaller number.
        assert!(
            (settled - 50.0).abs() < 0.5,
            "a 5 x 10px gesture must land on 50px, landed on {settled} (trace tail: {:?})",
            &trace[trace.len().saturating_sub(6)..]
        );
        // Stable: the last 30 ticks must not move it at all.
        for (i, y) in trace.iter().enumerate().skip(trace.len() - 30) {
            assert!(
                (*y - settled).abs() < 0.01,
                "offset still moving at tick {i}: {y} vs settled {settled}"
            );
        }
    }

    /// REGRESSION (B1): a physics-produced update must NEVER come back as a
    /// fresh scroll input. If it did, every tick would re-integrate its own
    /// output and the gesture could never converge.
    #[test]
    fn physics_output_is_not_re_consumed_as_fresh_input() {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 1000.0));
        let queue = layout_window.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(
            queue.clone(),
            ScrollPhysics::default(),
        ));

        queue.push(input_dev(
            1,
            (0.0, 40.0),
            ScrollInputSource::TrackpadContinuous,
            ScrollInputDevice::Touchpad,
        ));
        queue.push(input_dev(
            1,
            (0.0, 0.0),
            ScrollInputSource::TrackpadEnd,
            ScrollInputDevice::Touchpad,
        ));

        // First tick drains the user's gesture...
        let _ = closed_loop_tick(&mut layout_window, &data);
        // ...and from then on nothing may re-appear in the input queue, however
        // many offsets the physics writes.
        for tick in 0..60 {
            let _ = closed_loop_tick(&mut layout_window, &data);
            assert!(
                !queue.has_pending(),
                "tick {tick}: applying a physics ScrollTo put input back on the \
                 scroll input queue — that is the feedback loop"
            );
        }
    }

    /// REGRESSION (B1): two trackpad events inside ONE 16ms tick must add up.
    ///
    /// `current_offset` does not move while the callback runs, so computing
    /// `current + delta` per event and `insert`ing collapsed the batch to the
    /// LAST delta. A 120Hz trackpad against the 16ms tick puts two events in a
    /// tick routinely, i.e. half the gesture was dropped.
    #[test]
    fn two_trackpad_events_in_one_tick_accumulate_instead_of_overwriting() {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 1000.0));
        let queue = layout_window.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(
            queue.clone(),
            ScrollPhysics::default(),
        ));

        for _ in 0..3 {
            queue.push(input_dev(
                1,
                (0.0, 10.0),
                ScrollInputSource::TrackpadContinuous,
                ScrollInputDevice::Touchpad,
            ));
        }
        let _ = closed_loop_tick(&mut layout_window, &data);

        let y = offset_of(&layout_window, 1).y;
        assert!(
            (y - 30.0).abs() < 0.01,
            "three 10px deltas in one tick must move 30px, moved {y}"
        );
    }

    /// REGRESSION (B1): only ONE writer may claim a node's offset in a tick.
    ///
    /// The trackpad staging position and the velocity/spring position are both
    /// applied as `scroll_to_unclamped` for the same node, velocity LAST — so
    /// on any tick where the finger moved AND the spring was armed, the
    /// gesture's delta was silently overwritten by a position integrated from
    /// the stale, pre-tick offset. That is the "constantly fighting" symptom.
    #[test]
    fn the_spring_does_not_also_write_a_node_the_finger_moved_this_tick() {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        // max_scroll_y = 100, so a 150px flick overshoots and arms the spring.
        register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 200.0));
        let queue = layout_window.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(
            queue.clone(),
            ScrollPhysics::macos(),
        ));

        queue.push(input_dev(
            1,
            (0.0, 150.0),
            ScrollInputSource::TrackpadContinuous,
            ScrollInputDevice::Touchpad,
        ));
        queue.push(input_dev(
            1,
            (0.0, 0.0),
            ScrollInputSource::TrackpadEnd,
            ScrollInputDevice::Touchpad,
        ));

        let emitted = closed_loop_tick(&mut layout_window, &data);
        let writes_for_node_1 = emitted.iter().filter(|(idx, ..)| *idx == 1).count();
        assert_eq!(
            writes_for_node_1, 1,
            "the finger moved node 1 this tick, so exactly one writer may claim \
             it; got {writes_for_node_1} ScrollTos: {emitted:?}"
        );
    }

    /// REGRESSION (B1): after an overscroll flick the offset must spring back
    /// to the boundary.
    ///
    /// `TrackpadEnd` used to look for the gesture's position in
    /// `pending_positions` — the PROGRAMMATIC map, which a trackpad gesture
    /// never writes — and so decided "no overshoot" from the stale pre-tick
    /// offset. The rubber band was never armed and the view stayed parked
    /// outside its own bounds.
    #[test]
    fn an_overscrolled_gesture_springs_back_to_the_boundary() {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 200.0));
        let queue = layout_window.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(
            queue.clone(),
            ScrollPhysics::macos(),
        ));

        queue.push(input_dev(
            1,
            (0.0, 150.0),
            ScrollInputSource::TrackpadContinuous,
            ScrollInputDevice::Touchpad,
        ));
        queue.push(input_dev(
            1,
            (0.0, 0.0),
            ScrollInputSource::TrackpadEnd,
            ScrollInputDevice::Touchpad,
        ));

        let after_gesture = {
            let _ = closed_loop_tick(&mut layout_window, &data);
            offset_of(&layout_window, 1).y
        };
        assert!(
            after_gesture > 100.5,
            "the flick must overshoot past max_scroll_y=100 first, got {after_gesture}"
        );

        for _ in 0..240 {
            let _ = closed_loop_tick(&mut layout_window, &data);
        }
        let settled = offset_of(&layout_window, 1).y;
        assert!(
            (settled - 100.0).abs() < 1.0,
            "the rubber band must pull the view back to max_scroll_y=100, \
             it stayed at {settled}"
        );
    }

    /// REGRESSION: a physical wheel click glides to its target WITHOUT ever
    /// moving backwards, and lands on it.
    ///
    /// Reported on X11/Wayland as "wheel scroll smooths, then jumps back and
    /// forward, damping toward the middle". The glide is a critically-damped
    /// spring, which in continuous time cannot oscillate — but it was
    /// integrated with explicit Euler (`v += (-k·e - c·v)·dt; x += v·dt`) at
    /// the 16 ms tick, where the wheel spring's damping step is
    /// `c·dt = 2ω·dt ≈ 1.68`. `v ← v·(1 - c·dt)` then FLIPS the velocity's
    /// sign every tick: three 40 px notches went 0 → 84 → 55 → 119 → 78 →
    /// 134 → 88 → … and never settled (run the old integrator through this
    /// test to see it). It showed on Linux because that is where a physical
    /// wheel is the everyday device; a trackpad never enters the spring.
    /// The closed-form integrator (`critically_damped_step`) is exact at any
    /// `dt`, so the same trace is now monotone and converges.
    #[test]
    fn a_wheel_click_glides_to_its_target_without_ever_moving_backwards() {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 2000.0));
        let queue = layout_window.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(
            queue.clone(),
            ScrollPhysics::default(),
        ));

        // Three notches of a 40 px wheel, queued before the first tick — a
        // quick flick. The target is their sum: 120 px.
        for _ in 0..3 {
            queue.push(input_dev(
                1,
                (0.0, 40.0),
                ScrollInputSource::WheelDiscrete,
                ScrollInputDevice::MouseWheel,
            ));
        }

        let mut trace = Vec::new();
        for _ in 0..60 {
            let _ = closed_loop_tick(&mut layout_window, &data);
            trace.push(offset_of(&layout_window, 1).y);
        }

        for (i, w) in trace.windows(2).enumerate() {
            assert!(
                w[1] >= w[0] - 1e-3,
                "the glide moved BACKWARDS at tick {}: {} -> {} (trace: {trace:?})",
                i + 1,
                w[0],
                w[1]
            );
        }
        assert!(
            trace.iter().all(|y| *y <= 120.0 + 0.01),
            "the glide overshot its 120 px target: {trace:?}"
        );
        let settled = trace[trace.len() - 1];
        assert!(
            (settled - 120.0).abs() < 0.5,
            "three 40 px notches must land on 120 px within a second, landed on \
             {settled} (trace: {trace:?})"
        );
        // The timer must not keep ticking against a retired target.
        let mut data = data;
        with_state(&mut data, |st| {
            assert!(
                st.animate_targets.is_empty(),
                "the target is retired once reached: {:?}",
                st.animate_targets
            );
        });
    }

    /// REGRESSION: the rubber band's spring-back must be monotone too — it is
    /// the same spring, with the overshoot as the displacement. The Windows
    /// preset's 200 ms bounce sits right at `c·dt ≈ 1.0` under explicit Euler,
    /// where each tick flipped the velocity and the edge "vibrated".
    #[test]
    fn a_rubber_band_spring_back_is_monotone_at_every_preset() {
        // (`default` and `windows` have elasticity 0: they hard-clamp and never
        // overshoot, so there is no spring-back to check there.)
        for (name, physics) in [
            ("macos", ScrollPhysics::macos()),
            ("ios", ScrollPhysics::ios()),
            ("android", ScrollPhysics::android()),
            (
                "windows+elastic",
                ScrollPhysics {
                    overscroll_elasticity: 0.5,
                    max_overscroll_distance: 100.0,
                    ..ScrollPhysics::windows()
                },
            ),
        ] {
            let mut layout_window =
                LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
            register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 200.0));
            let queue = layout_window.scroll_manager.get_input_queue();
            let data = RefAny::new(ScrollPhysicsState::new(queue.clone(), physics));

            queue.push(input_dev(
                1,
                (0.0, 150.0),
                ScrollInputSource::TrackpadContinuous,
                ScrollInputDevice::Touchpad,
            ));
            queue.push(input_dev(
                1,
                (0.0, 0.0),
                ScrollInputSource::TrackpadEnd,
                ScrollInputDevice::Touchpad,
            ));
            let _ = closed_loop_tick(&mut layout_window, &data);
            let start = offset_of(&layout_window, 1).y;
            assert!(start > 100.0, "[{name}] the gesture overshoots first: {start}");

            let mut trace = vec![start];
            for _ in 0..240 {
                let _ = closed_loop_tick(&mut layout_window, &data);
                trace.push(offset_of(&layout_window, 1).y);
            }
            for (i, w) in trace.windows(2).enumerate() {
                assert!(
                    w[1] <= w[0] + 1e-3,
                    "[{name}] the spring-back moved AWAY from the edge at tick {}: \
                     {} -> {} (trace head: {:?})",
                    i + 1,
                    w[0],
                    w[1],
                    &trace[..trace.len().min(12)]
                );
            }
            let settled = trace[trace.len() - 1];
            assert!(
                (settled - 100.0).abs() < 1e-3,
                "[{name}] must land EXACTLY on max_scroll_y=100 (the spring snaps its last \
                 half-pixel), settled at {settled}"
            );
        }
    }

    // ==================================================================
    // REPORTED (macOS trackpad, 2026-08-21): "overscroll jitters".
    // ==================================================================
    //
    // The stretch must be a function of how far the finger travelled past
    // the edge, not of how much delta arrived in the current tick. Every
    // trackpad test above pushed a whole gesture into ONE tick; these drive
    // N deltas across N ticks, which is where the per-tick map `x ← D(x + d)`
    // showed as a sawtooth. The harness is closed-loop: tick N+1 reads what
    // tick N committed, exactly like the shell.

    /// `D(o)`: the displayed overshoot for a raw overshoot `o` under `physics`.
    fn band(physics: &ScrollPhysics, raw_overshoot: f32) -> f32 {
        let m = physics.max_overscroll_distance;
        let e = physics.overscroll_elasticity;
        m * (1.0 - (-e * raw_overshoot / m).exp())
    }

    /// A (100×100) viewport over (100×200) content — `max_scroll_y = 100` —
    /// parked exactly at its bottom edge by one in-range programmatic scroll.
    fn window_at_the_bottom_edge(physics: ScrollPhysics) -> (LayoutWindow, RefAny, ScrollInputQueue) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        register_node(&mut layout_window, 1, (100.0, 100.0), (100.0, 200.0));
        let queue = layout_window.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(queue.clone(), physics));
        queue.push(input_dev(1, (0.0, 100.0), ScrollInputSource::Programmatic, ScrollInputDevice::Touchpad));
        let _ = closed_loop_tick(&mut layout_window, &data);
        assert_eq!(offset_of(&layout_window, 1).y, 100.0, "seeded at the edge");
        (layout_window, data, queue)
    }

    fn finger(delta_y: f32) -> ScrollInput {
        input_dev(1, (0.0, delta_y), ScrollInputSource::TrackpadContinuous, ScrollInputDevice::Touchpad)
    }

    fn momentum(delta_y: f32) -> ScrollInput {
        input_dev(1, (0.0, delta_y), ScrollInputSource::TrackpadMomentum, ScrollInputDevice::Touchpad)
    }

    fn lift() -> ScrollInput {
        input_dev(1, (0.0, 0.0), ScrollInputSource::TrackpadEnd, ScrollInputDevice::Touchpad)
    }

    /// REPORTED (macOS trackpad, 2026-08-25): "it's almost as if the overscroll
    /// effect is applied twice — once the bounce works as expected, and then it
    /// suddenly does the bounce again after it snapped to the correct
    /// position."
    ///
    /// macOS synthesises the momentum tail from the finger velocity AT LIFT-OFF
    /// and replays that decay curve for 1-2 s. It is a canned animation: it
    /// knows nothing about our content bounds and there is no API to cancel it,
    /// so deltas keep arriving long after the content is pinned at an edge and
    /// the bounce has finished. Every engine has to swallow them itself —
    /// WebKit/Chromium latch `ignore_momentum_scrolls_` "to stop endless
    /// stretching".
    ///
    /// An `AZ_SCROLL_TRACE` of one real flick into the top edge caught it
    /// exactly: the offset settled to 0.000 at t=548 ms, jumped back out to
    /// -13.020 at t=593, settled again at t=993, jumped to -2.085 at t=1026 —
    /// THREE bounces from one gesture, each one a leftover chunk of the tail.
    /// The deltas below are that trace's own sequence.
    #[test]
    fn the_momentum_tail_cannot_restart_a_bounce_that_already_landed() {
        // Verbatim from the device trace: macOS's real momentum decay curve.
        const TAIL: [f32; 65] = [
            106.0, 114.0, 114.0, 111.0, 107.0, 102.0, 99.0, 93.0, 89.0, 84.0, 79.0, 75.0, 70.0,
            66.0, 61.0, 57.0, 53.0, 49.0, 45.0, 43.0, 39.0, 36.0, 33.0, 30.0, 28.0, 26.0, 24.0,
            21.0, 19.0, 17.0, 16.0, 14.0, 13.0, 13.0, 11.0, 10.0, 9.0, 9.0, 8.0, 7.0, 7.0, 6.0,
            6.0, 5.0, 5.0, 5.0, 4.0, 4.0, 4.0, 3.0, 3.0, 3.0, 3.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ];

        let (mut lw, data, queue) = window_at_the_bottom_edge(ScrollPhysics::macos());

        // The fingers lift FIRST: everything below belongs to a gesture that is
        // already over, which is the whole point.
        queue.push(lift());
        let _ = closed_loop_tick(&mut lw, &data);

        let mut overshoots = Vec::new();
        for delta in TAIL {
            queue.push(momentum(delta));
            let _ = closed_loop_tick(&mut lw, &data);
            overshoots.push(offset_of(&lw, 1).y - 100.0);
        }
        // ...and let anything still in flight settle.
        for _ in 0..40 {
            let _ = closed_loop_tick(&mut lw, &data);
            overshoots.push(offset_of(&lw, 1).y - 100.0);
        }

        // A "bounce" is a rising edge past 1 px after the spring had already
        // come back under 0.5 px — the same number the spring itself treats as
        // landed.
        let mut bounces = 0;
        let mut landed = true;
        for o in &overshoots {
            if landed && *o > 1.0 {
                bounces += 1;
                landed = false;
            } else if !landed && *o < 0.5 {
                landed = true;
            }
        }

        let rounded: Vec<f32> = overshoots.iter().map(|o| (o * 100.0).round() / 100.0).collect();
        assert_eq!(
            bounces, 1,
            "one flick must bounce exactly ONCE — the momentum tail restarted a \
             landed bounce {} more time(s). overshoot per tick: {rounded:?}",
            bounces - 1
        );
        assert!(
            overshoots.last().is_some_and(|o| o.abs() < 0.01),
            "must end parked exactly at the edge, ended at {:?}",
            overshoots.last()
        );
    }

    /// The physics timer must NOT terminate while a momentum-ignore latch is
    /// held — and must terminate once it has aged out.
    ///
    /// The shell constructs a brand-new `ScrollPhysicsState` every time it
    /// starts the momentum timer (`macos/events.rs:640`, same in the other
    /// four backends), so anything the timer terminates with is lost. When the
    /// spring lands, every OTHER map empties while the OS tail is still
    /// running: the timer stopped right there, the next momentum delta
    /// restarted it with an empty latch map, and the node was stretched
    /// straight back out. The device trace showed it as `latch=true,true` on
    /// one tick and `latch=NONE` two ticks later with nothing having cleared
    /// it. The harness missed it because it drives the callback directly and
    /// never terminates or rebuilds the state.
    #[test]
    fn a_held_momentum_latch_keeps_the_physics_timer_alive() {
        let mut state =
            ScrollPhysicsState::new(ScrollInputQueue::new(), ScrollPhysics::macos());
        assert!(!state.is_active(), "an empty state is idle");

        state.momentum_latched.insert(
            key(1),
            MomentumLatch { x: false, y: true, idle: 0 },
        );
        assert!(
            state.is_active(),
            "a held latch must keep the timer alive — the shell rebuilds this \
             state on every timer start, so terminating loses the latch"
        );

        // Once it ages out the timer is free to stop again: the latch cannot
        // pin the pump indefinitely.
        state.momentum_latched.clear();
        assert!(!state.is_active(), "an expired latch must not pin the timer");
    }

    /// The latch must not outlive the tail: a genuinely NEW fling, after the
    /// old one is done, has to bounce like any other.
    #[test]
    fn a_new_fling_after_the_ignored_tail_bounces_normally() {
        let (mut lw, data, queue) = window_at_the_bottom_edge(ScrollPhysics::macos());

        queue.push(lift());
        let _ = closed_loop_tick(&mut lw, &data);
        for _ in 0..12 {
            queue.push(momentum(40.0));
            let _ = closed_loop_tick(&mut lw, &data);
        }
        // The tail stops and the bounce lands.
        for _ in 0..60 {
            let _ = closed_loop_tick(&mut lw, &data);
        }
        assert!(
            (offset_of(&lw, 1).y - 100.0).abs() < 0.01,
            "the first fling must land at the edge, at {}",
            offset_of(&lw, 1).y
        );

        // A second gesture: fingers down, flick, lift, tail.
        queue.push(finger(30.0));
        let _ = closed_loop_tick(&mut lw, &data);
        queue.push(lift());
        let _ = closed_loop_tick(&mut lw, &data);
        queue.push(momentum(60.0));
        let _ = closed_loop_tick(&mut lw, &data);

        assert!(
            offset_of(&lw, 1).y - 100.0 > 1.0,
            "a NEW fling must still be able to stretch past the edge, at {}",
            offset_of(&lw, 1).y
        );
    }

    #[test]
    fn a_held_finger_past_the_edge_stretches_monotonically_and_independently_of_batching() {
        let physics = ScrollPhysics::macos();
        let expected_final = 100.0 + band(&physics, 200.0);
        let mut finals = Vec::new();
        // Twenty 10 px deltas: one per tick (60 Hz device), two per tick
        // (120 Hz device), alternating 1/2 (120 Hz device vs the 62.5 Hz timer —
        // the real macOS case, which produced a ±1 px sawtooth at ~31 Hz).
        let patterns: [(&str, Vec<usize>); 3] = [
            ("1/tick", vec![1; 20]),
            ("2/tick", vec![2; 10]),
            ("2,1,2,1", (0..13).map(|i| if i % 2 == 0 { 2 } else { 1 }).collect()),
        ];
        for (name, per_tick) in patterns {
            let total: usize = per_tick.iter().sum();
            assert_eq!(total, 20, "{name}: every pattern carries the same 20 deltas");
            let (mut lw, data, queue) = window_at_the_bottom_edge(physics);
            let mut trace = vec![offset_of(&lw, 1).y];
            for n in per_tick {
                for _ in 0..n {
                    queue.push(finger(10.0));
                }
                let _ = closed_loop_tick(&mut lw, &data);
                let y = offset_of(&lw, 1).y;
                let prev = *trace.last().unwrap();
                assert!(
                    y > prev + 1e-3,
                    "[{name}] a tick with finger input must stretch further: {prev} -> {y} \
                     (trace {trace:?})"
                );
                trace.push(y);
            }
            let last = *trace.last().unwrap();
            assert!(
                (last - expected_final).abs() < 0.05,
                "[{name}] the stretch is D(Σ deltas) = {expected_final}, got {last} (trace {trace:?})"
            );
            finals.push(last);
        }
        let spread = finals.iter().cloned().fold(f32::MIN, f32::max)
            - finals.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.05, "batching must not change the stretch: {finals:?}");
    }

    #[test]
    fn a_finger_that_pauses_or_creeps_keeps_its_stretch() {
        let physics = ScrollPhysics::macos();
        let (mut lw, data, queue) = window_at_the_bottom_edge(physics);
        let mut trace = vec![offset_of(&lw, 1).y];
        let step = |lw: &mut LayoutWindow, deltas: &[f32], trace: &mut Vec<f32>| {
            for d in deltas {
                queue.push(finger(*d));
            }
            let (_, verdict) = closed_loop_tick_full(lw, &data);
            assert_eq!(
                verdict,
                TerminateTimer::Continue,
                "the timer must stay alive while a finger is down (trace {trace:?})"
            );
            let y = offset_of(lw, 1).y;
            let prev = *trace.last().unwrap();
            assert!(
                y >= prev - 1e-3,
                "the stretch must never shrink while the finger is down: {prev} -> {y} (trace {trace:?})"
            );
            trace.push(y);
        };
        for _ in 0..6 {
            step(&mut lw, &[10.0], &mut trace);
        }
        let held = *trace.last().unwrap();
        assert!((held - (100.0 + band(&physics, 60.0))).abs() < 0.05, "{held}");
        // The finger rests: empty ticks...
        for _ in 0..3 {
            step(&mut lw, &[], &mut trace);
        }
        // ...and creeps (deltas that pass the 0.01 px gate).
        for _ in 0..3 {
            step(&mut lw, &[0.02], &mut trace);
        }
        assert!(
            (*trace.last().unwrap() - held).abs() < 0.1,
            "a resting finger keeps its stretch (it used to collapse 4.1 -> 1.2 -> 0.4): {trace:?}"
        );
        for _ in 0..3 {
            step(&mut lw, &[10.0], &mut trace);
        }
        let end = *trace.last().unwrap();
        assert!((end - (100.0 + band(&physics, 90.06))).abs() < 0.05, "{end} (trace {trace:?})");
    }

    /// Regression: a rubber-band bounce on ONE axis must not freeze the OTHER
    /// axis's fling.
    ///
    /// A macOS momentum NSEvent carries scrollingDeltaX AND scrollingDeltaY as a
    /// single input. The physics used to DROP the whole event the instant either
    /// axis latched the rubber band, so an accidental horizontal overscroll
    /// during a vertical flick stopped the vertical scroll dead — the "once it
    /// snaps to X it also kills the Y scrolling" the user reported. The masking
    /// now silences only the overshooting axis and lets the in-range axis keep
    /// flinging.
    #[test]
    fn a_rubber_band_on_one_axis_does_not_freeze_the_other_axis_fling() {
        let physics = ScrollPhysics::macos();
        let mut lw =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        // Both axes scrollable: a 100x100 viewport over 200x200 content, so
        // max_scroll_x = max_scroll_y = 100.
        register_node(&mut lw, 1, (100.0, 100.0), (200.0, 200.0));
        let queue = lw.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(queue.clone(), physics));

        // Park at the RIGHT edge in X (x = max_scroll_x) and mid-range in Y.
        queue.push(input_dev(
            1,
            (100.0, 20.0),
            ScrollInputSource::Programmatic,
            ScrollInputDevice::Touchpad,
        ));
        let _ = closed_loop_tick(&mut lw, &data);
        assert_eq!(offset_of(&lw, 1).x, 100.0, "seeded at the right edge");
        assert_eq!(offset_of(&lw, 1).y, 20.0, "seeded mid-range in Y");

        // Fling PAST the right edge in X (momentum) → X enters the rubber band.
        queue.push(input_dev(
            1,
            (40.0, 0.0),
            ScrollInputSource::TrackpadMomentum,
            ScrollInputDevice::Touchpad,
        ));
        let _ = closed_loop_tick(&mut lw, &data);
        let x_banded = offset_of(&lw, 1).x;
        assert!(
            x_banded > 100.5,
            "X must be overscrolled past the edge (rubber-banding): {x_banded}"
        );
        let y_before = offset_of(&lw, 1).y;

        // Diagonal momentum: X keeps pushing INTO the banding edge (masked), Y is
        // a plain in-range fling. The Y component must advance every tick.
        for _ in 0..3 {
            queue.push(input_dev(
                1,
                (40.0, 15.0),
                ScrollInputSource::TrackpadMomentum,
                ScrollInputDevice::Touchpad,
            ));
            let _ = closed_loop_tick(&mut lw, &data);
        }
        let y_after = offset_of(&lw, 1).y;
        assert!(
            y_after > y_before + 1.0,
            "the Y fling must keep advancing while X rubber-bands (the whole-event \
             drop used to freeze it): {y_before} -> {y_after}"
        );
        // X stayed in its band — the Y deltas did not throw it off the edge.
        // (Correct: momentum is still PUSHING into the edge every tick, so the
        // band is being held stretched. The interesting case is what happens
        // when it stops being pushed — below.)
        let x_held = offset_of(&lw, 1).x;
        assert!(x_held > 100.5, "X stays in its rubber band: {x_held}");

        // ── The band must RELEASE once nothing pushes it ────────────────────
        //
        // Y keeps flinging, X gets a zero delta. Nothing is holding X out any
        // more, so its spring must pull it back toward the edge (100.0).
        //
        // ⚠ THIS IS EXPECTED TO FAIL until the per-axis rewrite lands, and it
        // is meant to. `pending_trackpad_positions.insert` runs for EVERY
        // trackpad/momentum event (scroll_timer.rs ~392), that key lands in
        // `moved_by_finger_this_tick` (~574) and the integration loop
        // `continue`s (~578) — so the spring is frozen for the WHOLE macOS
        // momentum tail, which runs 1-2 s after the fingers lift. The bounce
        // then plays only after the tail ends, which is what the user sees as
        // "the bounces are queued and play one after another".
        //
        // A red test that states the correct behaviour is worth more than a
        // green one that pins the defect (USER ruling 2026-08-25). The old
        // version of this test stopped at `x_held` and therefore certified the
        // freeze as intended.
        for _ in 0..6 {
            queue.push(input_dev(
                1,
                (0.0, 15.0),
                ScrollInputSource::TrackpadMomentum,
                ScrollInputDevice::Touchpad,
            ));
            let _ = closed_loop_tick(&mut lw, &data);
        }
        let x_released = offset_of(&lw, 1).x;
        assert!(
            x_released < x_held - 0.5,
            "X must spring back toward its edge once momentum stops pushing it \
             (the band is frozen for the whole momentum tail): {x_held} -> {x_released}",
        );
    }

    /// The same simulated TIME must produce the same motion, however the ticks
    /// are spaced. This is the "blocky scrolling" guard.
    ///
    /// The physics used to advance by a fixed `timer_interval_ms` — the
    /// timer's CONFIGURED period — regardless of how much wall clock had
    /// actually passed. The real spacing is not the configured period:
    /// `Timer::invoke` DROPS a fire that lands a hair under the interval and
    /// then stamps `last_run = now` instead of `last_run + interval`, so the
    /// phase never self-corrects and the following step arrives ~2 intervals
    /// later; two independent 16 ms timers drive the same pump; and 16 ms is
    /// neither 60 nor 120 Hz. Advancing 16 ms of simulation over 16-32 ms of
    /// real time makes the apparent speed swing by ±50 % from frame to frame,
    /// which is what "blocky" looks like.
    ///
    /// So: run one bounce at a steady 1 tick per step, and the same bounce at
    /// 2 ticks per step over half as many steps. Both cover the same simulated
    /// time and must land in the same place.
    #[test]
    fn the_same_elapsed_time_moves_the_same_distance_at_any_tick_spacing() {
        fn bounce_after(steps: usize, ticks_per_step: u64) -> f32 {
            let physics = ScrollPhysics::macos();
            let (mut lw, data, queue) = window_at_the_bottom_edge(physics);
            for _ in 0..19 {
                queue.push(finger(10.0));
                let _ = closed_loop_tick(&mut lw, &data);
            }
            queue.push(lift());
            for _ in 0..steps {
                let _ = closed_loop_tick_spaced(&mut lw, &data, ticks_per_step);
            }
            offset_of(&lw, 1).y - 100.0
        }

        // 12 x 1 tick and 6 x 2 ticks are both 12 ticks of simulated time.
        let steady = bounce_after(12, 1);
        let jittered = bounce_after(6, 2);
        assert!(
            steady > 0.5,
            "precondition: the bounce must still be in flight to compare: {steady}",
        );
        assert!(
            (steady - jittered).abs() < 0.5,
            "the same simulated time must travel the same distance whatever the \
             tick spacing: steady={steady} vs jittered={jittered}",
        );
    }

    /// A SECOND `TrackpadEnd` arriving during a live bounce must not restart it.
    ///
    /// macOS delivers 2-3 `TrackpadEnd`s for one flick — `phase Ended`,
    /// `momentumPhase Ended` and, when a finger lands during momentum,
    /// `momentumPhase Cancelled` all map to the same source
    /// (`shell2/macos/events.rs`). The arm at the top of this file only
    /// preserves an in-flight spring's velocity while `is_rubber_banding` is
    /// still set; once the spring gets within 0.5 px the flag is CLEARED, and a
    /// late End then re-arms the band FROM REST for another full
    /// `bounce_back_duration_ms`. That is the user-visible "the bounces are
    /// queued and play one after another".
    ///
    /// The assertion is deliberately about SETTLING, not about internals: after
    /// a bounce has essentially finished, one more End must not put the offset
    /// back out past the edge.
    #[test]
    fn a_late_trackpad_end_does_not_restart_a_finished_bounce() {
        let physics = ScrollPhysics::macos();
        let (mut lw, data, queue) = window_at_the_bottom_edge(physics);

        // Stretch past the edge, then release.
        for _ in 0..19 {
            queue.push(finger(10.0));
            let _ = closed_loop_tick(&mut lw, &data);
        }
        queue.push(lift());

        // Let the bounce run to (near) rest.
        for _ in 0..60 {
            let _ = closed_loop_tick(&mut lw, &data);
        }
        let settled = offset_of(&lw, 1).y - 100.0;
        assert!(
            settled.abs() < 0.5,
            "precondition: the first bounce should have landed: {settled}",
        );

        // The trailing End of the SAME gesture. Nothing has moved since, so
        // there is nothing to bounce.
        queue.push(lift());
        let mut worst: f32 = 0.0;
        for _ in 0..30 {
            let _ = closed_loop_tick(&mut lw, &data);
            worst = worst.max((offset_of(&lw, 1).y - 100.0).abs());
        }
        assert!(
            worst < 0.5,
            "a trailing TrackpadEnd re-armed the band and bounced again \
             (max overshoot {worst} px after the gesture had already settled)",
        );
    }

    #[test]
    fn the_spring_back_takes_the_configured_bounce_duration_and_lands_exactly() {
        let physics = ScrollPhysics::macos();
        let (mut lw, data, queue) = window_at_the_bottom_edge(physics);
        // Nineteen 10 px deltas: raw 190 px past the edge, a displayed ≈ 40 px.
        for _ in 0..19 {
            queue.push(finger(10.0));
            let _ = closed_loop_tick(&mut lw, &data);
        }
        let start = offset_of(&lw, 1).y - 100.0;
        assert!((start - band(&physics, 190.0)).abs() < 0.05, "{start}");
        assert!(start > 35.0, "the stretch must reach tens of px now: {start}");

        queue.push(lift());
        let mut trace = Vec::new();
        for _ in 0..60 {
            let _ = closed_loop_tick(&mut lw, &data);
            trace.push(offset_of(&lw, 1).y - 100.0);
        }
        for (i, w) in trace.windows(2).enumerate() {
            assert!(w[1] <= w[0] + 1e-3, "not monotone at tick {}: {trace:?}", i + 1);
        }
        assert!(
            trace[2] > 0.5 * start,
            "the bounce is {} ms, not a 3-frame snap: {trace:?}",
            physics.bounce_back_duration_ms
        );
        let landed_at = trace.iter().position(|o| o.abs() < 0.5).expect("the spring lands");
        assert!(
            (10..=36).contains(&landed_at),
            "a {} ms critically-damped bounce lands after ~27 ticks, not {landed_at}: {trace:?}",
            physics.bounce_back_duration_ms
        );
        assert!(
            trace.iter().all(|o| *o >= -0.01),
            "the spring must never cross into the content (the old crossing velocity became ~60 px \
             of drift): {trace:?}"
        );
        assert_eq!(trace[trace.len() - 1], 0.0, "lands EXACTLY on the boundary: {trace:?}");
    }

    #[test]
    fn momentum_deltas_after_the_finger_lifts_do_not_kill_the_spring_back() {
        let physics = ScrollPhysics::macos();

        // A: a stretch, a lift, then the OS momentum tail (15 px decaying ×0.93).
        let (mut lw, data, queue) = window_at_the_bottom_edge(physics);
        for _ in 0..5 {
            queue.push(finger(10.0));
            let _ = closed_loop_tick(&mut lw, &data);
        }
        let lifted = offset_of(&lw, 1).y;
        assert!(lifted > 110.0, "{lifted}");
        queue.push(lift());
        let mut trace = vec![lifted];
        let mut d = 15.0f32;
        for _ in 0..20 {
            queue.push(momentum(d));
            d *= 0.93;
            let _ = closed_loop_tick(&mut lw, &data);
            trace.push(offset_of(&lw, 1).y);
        }
        queue.push(lift()); // momentumPhase = Ended
        for _ in 0..40 {
            let _ = closed_loop_tick(&mut lw, &data);
            trace.push(offset_of(&lw, 1).y);
        }
        for (i, w) in trace.windows(2).enumerate() {
            assert!(
                w[1] <= w[0] + 1e-3,
                "from the lift on the view only returns (a momentum delta used to restart the \
                 band every tick) — tick {}: {trace:?}",
                i + 1
            );
        }
        assert_eq!(*trace.last().unwrap(), 100.0, "lands exactly: {trace:?}");

        // B: an in-range fling whose momentum reaches the edge: a bump past
        // it, then a monotone return — the spring owns the axis from the
        // first over-edge momentum delta on.
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        register_node(&mut lw, 1, (100.0, 100.0), (100.0, 200.0));
        let queue = lw.scroll_manager.get_input_queue();
        let data = RefAny::new(ScrollPhysicsState::new(queue.clone(), physics));
        let mut trace = Vec::new();
        for _ in 0..12 {
            queue.push(momentum(20.0));
            let _ = closed_loop_tick(&mut lw, &data);
            trace.push(offset_of(&lw, 1).y);
        }
        queue.push(lift());
        for _ in 0..80 {
            let _ = closed_loop_tick(&mut lw, &data);
            trace.push(offset_of(&lw, 1).y);
        }
        let peak = trace.iter().cloned().fold(f32::MIN, f32::max);
        assert!(peak > 105.0, "the momentum bumps past the edge: {trace:?}");
        assert!(
            peak <= 100.0 + physics.max_overscroll_distance + 1e-3,
            "the bump stays inside the band envelope: {peak}"
        );
        let peak_at = trace.iter().position(|y| *y == peak).unwrap();
        for (i, w) in trace[peak_at..].windows(2).enumerate() {
            assert!(
                w[1] <= w[0] + 1e-3,
                "after the bump the view only returns — tick {}: {trace:?}",
                peak_at + i + 1
            );
        }
        assert_eq!(*trace.last().unwrap(), 100.0, "lands exactly: {trace:?}");
    }
}
