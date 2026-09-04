//! DOM-morph animation: interpolation core, FLIP geometry, and the keyed
//! animation store.
//!
//! # Model
//!
//! Animation compiles to keyed timer callbacks writing into the top cascade layer
//! (`user_overridden_properties`). This module handles the math and bookkeeping
//! across frames:
//!
//! * [`Spring`] / [`AnimChannel`]: How a single value transitions from `from` to `to`,
//!   with interruption as a first-class operation ([`AnimChannel::retarget`]).
//! * [`flip`]: The First/Last inversion that turns a layout change into a
//!   composited transform, saving relayouts.
//! * [`AnimationManager`]: The keyed store - keys are reconciliation identities
//!   so that retargeting finds the in-flight state instead of overlapping.
//!
//! # Curve vs. Spring
//!
//! Easing curves are functions of normalized time. Interrupting them discards velocity,
//! causing snapping. Springs integrate from the current `(value, velocity)`, allowing
//! smooth retargeting.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use azul_css::props::basic::animation::AnimationInterpolationFunction;

use crate::{
    diff::{calculate_reconciliation_key, NodeMove},
    dom::NodeData,
    geom::LogicalRect,
    id::NodeId,
    styled_dom::NodeHierarchyItem,
};

/// Re-export of [`SpringCurve`].
pub use azul_css::props::basic::animation::SpringCurve as Spring;

/// A single animated value over time (e.g., an x-coordinate or opacity).
///
/// Complex animations are created by combining multiple channels. For example,
/// moving an element in 2D requires four channels: x and y translation, and x and y scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimChannel {
    /// Where the value started. Re-seeded on every retarget.
    pub from: f32,
    /// Where it is heading.
    pub to: f32,
    /// The current value to write into the cascade.
    pub current: f32,
    /// Units per second. Carried across retargets.
    pub velocity: f32,
    /// Seconds since the animation began (curve mode only).
    pub elapsed_secs: f32,
    /// How it is driven.
    pub mode: InterpolationMode,
    /// Latched once the channel arrives.
    finished: bool,
}

/// Specifies how an animated value transitions over time.
///
/// It can either be driven by a time-based curve (duration-based) or by a
/// physics-based spring (velocity and target-based).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterpolationMode {
    /// Duration-based easing. Not interruptible without a discontinuity.
    Curve {
        /// The CSS easing curve.
        function: AnimationInterpolationFunction,
        /// Total duration in seconds. Zero means apply instantly.
        duration_secs: f32,
    },
    /// Physics-based. Interruptible with velocity continuity.
    Spring(Spring),
}

impl Default for InterpolationMode {
    fn default() -> Self {
        Self::Spring(Spring::SMOOTH)
    }
}

impl AnimChannel {
    /// A channel that eases `from → to` over `duration_secs`.
    #[must_use]
    pub const fn curve(
        from: f32,
        to: f32,
        function: AnimationInterpolationFunction,
        duration_secs: f32,
    ) -> Self {
        Self {
            from,
            to,
            current: from,
            velocity: 0.0,
            elapsed_secs: 0.0,
            mode: InterpolationMode::Curve {
                function,
                duration_secs,
            },
            finished: false,
        }
    }

    /// A channel that springs `from → to`.
    #[must_use]
    pub const fn spring(from: f32, to: f32, spring: Spring) -> Self {
        Self {
            from,
            to,
            current: from,
            velocity: 0.0,
            elapsed_secs: 0.0,
            mode: InterpolationMode::Spring(spring),
            finished: false,
        }
    }

    /// Advance by `dt` seconds and return the new current value.
    pub fn tick(&mut self, dt: f32) -> f32 {
        if self.finished {
            return self.current;
        }
        match self.mode {
            InterpolationMode::Curve {
                function,
                duration_secs,
            } => {
                if duration_secs <= 0.0 {
                    self.current = self.to;
                    self.velocity = 0.0;
                    self.finished = true;
                    return self.current;
                }
                self.elapsed_secs += dt.max(0.0);
                let linear_t = (self.elapsed_secs / duration_secs).clamp(0.0, 1.0);
                let eased = ease(function, linear_t);
                let previous = self.current;
                // Suboptimal flops allowed for bit-reproducibility across builds.
                #[allow(clippy::suboptimal_flops)]
                {
                    self.current = self.from + (self.to - self.from) * eased;
                }
                // Track velocity even on curves to ensure continuous handover to springs.
                self.velocity = if dt > 0.0 {
                    (self.current - previous) / dt
                } else {
                    0.0
                };
                if linear_t >= 1.0 {
                    self.current = self.to;
                    self.finished = true;
                }
            }
            InterpolationMode::Spring(spring) => {
                let (value, velocity) = spring.step(self.current, self.to, self.velocity, dt);
                self.current = value;
                self.velocity = velocity;
                if spring.is_settled(value, self.to, velocity) {
                    self.current = self.to;
                    self.velocity = 0.0;
                    self.finished = true;
                }
            }
        }
        self.current
    }

    /// Whether this channel has arrived at the target value and can be dropped (animation finished)
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Aim at a new target without losing the current value or velocity.
    ///
    /// This allows smooth retargeting mid-flight.
    pub fn retarget(&mut self, new_to: f32) {
        if (self.to - new_to).abs() < f32::EPSILON && !self.finished {
            return; // already heading there; do not restart the clock
        }
        self.from = self.current;
        self.to = new_to;
        self.elapsed_secs = 0.0;
        self.finished = false;
        // Velocity is deliberately not reset to allow smooth retargeting.
    }
}

/// Evaluate a CSS easing curve at `t ∈ [0, 1]`.
///
/// `Ease` and the cubic-beziers use the same evaluator; the named curves are
/// their standard control points.
#[must_use]
pub fn ease(function: AnimationInterpolationFunction, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match function {
        AnimationInterpolationFunction::Linear => t,
        // The CSS keyword control points.
        AnimationInterpolationFunction::Ease => cubic_bezier_y(0.25, 0.1, 0.25, 1.0, t),
        AnimationInterpolationFunction::EaseIn => cubic_bezier_y(0.42, 0.0, 1.0, 1.0, t),
        AnimationInterpolationFunction::EaseOut => cubic_bezier_y(0.0, 0.0, 0.58, 1.0, t),
        AnimationInterpolationFunction::EaseInOut => cubic_bezier_y(0.42, 0.0, 0.58, 1.0, t),
        AnimationInterpolationFunction::Spring(_) => {
            crate::diagnostics::emit(String::from(
                "Warning: Spring evaluated as an easing curve. This is a misusage.",
            ));
            // Degrade gracefully to ease-in-out to prevent crashes.
            cubic_bezier_y(0.42, 0.0, 0.58, 1.0, t)
        }
        // The 0,0 and 1,1 points are hardcoded directly into the math equation itself,
        // so only ctrl_1 and ctrl_2 are needed.
        AnimationInterpolationFunction::CubicBezier(curve) => cubic_bezier_y(
            curve.ctrl_1.x,
            curve.ctrl_1.y,
            curve.ctrl_2.x,
            curve.ctrl_2.y,
            t,
        ),
    }
}

/// Calculates the y value of a CSS timing bezier given an x (time progress).
///
/// Unlike a standard bezier evaluation that calculates `(x, y)` from a curve parameter `t`,
/// CSS easing requires finding `y` (eased progress) for a specific `x` (linear time).
/// To do this, we must first reverse-engineer `t` from `x`.
///
/// We try a fast math shortcut to find `t`. If the curve is too flat, we fall back
/// to a slower, safer method to find the exact `t`, which is then used to calculate `y`.
fn cubic_bezier_y(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    const NEWTON_ITERATIONS: usize = 4;
    const BISECTION_ITERATIONS: usize = 12;
    const EPSILON: f32 = 1e-5;

    // allowed for bit-reproducibility across builds
    #[allow(clippy::suboptimal_flops)]
    let bezier = |a: f32, b: f32, t: f32| {
        let inv = 1.0 - t;
        3.0 * inv * inv * t * a + 3.0 * inv * t * t * b + t * t * t
    };
    #[allow(clippy::suboptimal_flops)]
    let bezier_slope = |a: f32, b: f32, t: f32| {
        let inv = 1.0 - t;
        3.0 * inv * inv * a + 6.0 * inv * t * (b - a) + 3.0 * t * t * (1.0 - b)
    };

    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    let mut t = x;
    for _ in 0..NEWTON_ITERATIONS {
        let error = bezier(x1, x2, t) - x;
        if error.abs() < EPSILON {
            return bezier(y1, y2, t);
        }
        let slope = bezier_slope(x1, x2, t);
        if slope.abs() < EPSILON {
            break;
        }
        t -= error / slope;
    }

    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    let mut t = x;
    for _ in 0..BISECTION_ITERATIONS {
        let value = bezier(x1, x2, t);
        if (value - x).abs() < EPSILON {
            break;
        }
        if value < x {
            low = t;
        } else {
            high = t;
        }
        t = (low + high) * 0.5;
    }
    bezier(y1, y2, t)
}

/// The inverted transform of a FLIP move.
///
/// "First" is the element's original position. "Last" is its new position after layout.
/// FLIP animation works by placing the element at Last, then applying a transform to
/// make it look like it's at First, and finally animating that transform down to zero.
/// This allows elements to move smoothly on the GPU without triggering expensive layout
/// recalculations on every frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct FlipTransform {
    /// Horizontal offset, logical px.
    pub translate_x: f32,
    /// Vertical offset, logical px.
    pub translate_y: f32,
    /// Horizontal scale, 1.0 = unchanged.
    pub scale_x: f32,
    /// Vertical scale, 1.0 = unchanged.
    pub scale_y: f32,
}

impl FlipTransform {
    /// The no-op transform.
    pub const IDENTITY: Self = Self {
        translate_x: 0.0,
        translate_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
    };

    /// Whether this is close enough to identity.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.translate_x.abs() < 0.01
            && self.translate_y.abs() < 0.01
            && (self.scale_x - 1.0).abs() < 0.001
            && (self.scale_y - 1.0).abs() < 0.001
    }
}

/// Compute the FLIP transform to move from the `first` rect to the `last` rect.
///
/// If the `last` rect has a zero size, the scale falls back to 1.
/// The function only calculates changes in position, avoiding content distortion.
#[must_use]
pub fn flip(first: LogicalRect, last: LogicalRect) -> FlipTransform {
    let _ = (first.size, last.size); // sizes are layout's job, not the animation's
    FlipTransform {
        translate_x: first.origin.x - last.origin.x,
        translate_y: first.origin.y - last.origin.y,
        scale_x: 1.0,
        scale_y: 1.0,
    }
}

/// Which presence class an animation belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimClass {
    /// Node exists in the new DOM only.
    Enter,
    /// Node existed in the old DOM only. Needs exit-retention to be visible.
    Exit,
    /// Node exists in both, at different geometry.
    Move,
}

/// Identity of an animation across frames.
///
/// Uses the reconciliation key (`.with_key()` / `#id` / structural hash)
/// to remain stable across frames, unlike `NodeId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AnimKey(pub u64);

/// One in-flight animation: the FLIP channels plus opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveAnim {
    /// What kind of presence change started this.
    pub class: AnimClass,
    /// Horizontal offset channel.
    pub translate_x: AnimChannel,
    /// Vertical offset channel.
    pub translate_y: AnimChannel,
    /// Horizontal scale channel.
    pub scale_x: AnimChannel,
    /// Vertical scale channel.
    pub scale_y: AnimChannel,
    /// Opacity channel.
    pub opacity: AnimChannel,
}

impl ActiveAnim {
    /// A move: start at the FLIP inversion, animate to identity.
    #[must_use]
    pub const fn move_from_flip(flip: FlipTransform, mode: InterpolationMode) -> Self {
        Self {
            class: AnimClass::Move,
            translate_x: channel(flip.translate_x, 0.0, mode),
            translate_y: channel(flip.translate_y, 0.0, mode),
            scale_x: channel(flip.scale_x, 1.0, mode),
            scale_y: channel(flip.scale_y, 1.0, mode),
            opacity: channel(1.0, 1.0, mode),
        }
    }

    /// An enter: slide in from `(from_x, from_y)` to identity, full opacity and size.
    #[must_use]
    pub const fn enter_slide(from_x: f32, from_y: f32, mode: InterpolationMode) -> Self {
        Self {
            class: AnimClass::Enter,
            translate_x: channel(from_x, 0.0, mode),
            translate_y: channel(from_y, 0.0, mode),
            scale_x: channel(1.0, 1.0, mode),
            scale_y: channel(1.0, 1.0, mode),
            opacity: channel(1.0, 1.0, mode),
        }
    }

    /// An exit: slide out from identity to `(to_x, to_y)`, full opacity and size.
    /// Only visible with exit-retention.
    ///
    /// Reversing a presence animation in flight will retarget from the current
    /// values with velocity preserved.
    pub fn retarget_presence(&mut self, class: AnimClass, to_x: f32, to_y: f32) {
        self.class = class;
        self.translate_x.retarget(to_x);
        self.translate_y.retarget(to_y);
        self.scale_x.retarget(1.0);
        self.scale_y.retarget(1.0);
        self.opacity.retarget(1.0);
    }

    #[must_use]
    pub const fn exit_slide(to_x: f32, to_y: f32, mode: InterpolationMode) -> Self {
        Self {
            class: AnimClass::Exit,
            translate_x: channel(0.0, to_x, mode),
            translate_y: channel(0.0, to_y, mode),
            scale_x: channel(1.0, 1.0, mode),
            scale_y: channel(1.0, 1.0, mode),
            opacity: channel(1.0, 1.0, mode),
        }
    }

    /// Advance every channel by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.translate_x.tick(dt);
        self.translate_y.tick(dt);
        self.scale_x.tick(dt);
        self.scale_y.tick(dt);
        self.opacity.tick(dt);
    }

    /// True once every channel has arrived.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.translate_x.is_finished()
            && self.translate_y.is_finished()
            && self.scale_x.is_finished()
            && self.scale_y.is_finished()
            && self.opacity.is_finished()
    }

    /// The transform to write this frame.
    #[must_use]
    pub const fn current_transform(&self) -> FlipTransform {
        FlipTransform {
            translate_x: self.translate_x.current,
            translate_y: self.translate_y.current,
            scale_x: self.scale_x.current,
            scale_y: self.scale_y.current,
        }
    }

    /// The opacity to write this frame.
    #[must_use]
    pub const fn current_opacity(&self) -> f32 {
        self.opacity.current
    }

    /// Re-aim at a new FLIP target, preserving position and velocity.
    pub fn retarget_move(&mut self, flip: FlipTransform) {
        // Seed toward identity from the current position.
        self.translate_x.retarget(0.0);
        self.translate_y.retarget(0.0);
        self.scale_x.retarget(1.0);
        self.scale_y.retarget(1.0);
        // Fold the freshly measured offset in, rather than snapping to it.
        self.translate_x.current += flip.translate_x;
        self.translate_y.current += flip.translate_y;
    }
}

const fn channel(from: f32, to: f32, mode: InterpolationMode) -> AnimChannel {
    match mode {
        InterpolationMode::Curve {
            function,
            duration_secs,
        } => AnimChannel::curve(from, to, function, duration_secs),
        InterpolationMode::Spring(spring) => AnimChannel::spring(from, to, spring),
    }
}

/// The keyed store of in-flight animations.
///
/// Holds only what must outlive a frame.
#[derive(Debug, Clone, Default)]
pub struct AnimationManager {
    active: BTreeMap<AnimKey, ActiveAnim>,
}

impl AnimationManager {
    /// An empty manager.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: BTreeMap::new(),
        }
    }

    /// How many animations are in flight.
    #[must_use]
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// Whether anything is animating (i.e. whether a frame needs scheduling).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Start a move, or retarget one already in flight under this key.
    pub fn start_or_retarget_move(
        &mut self,
        key: AnimKey,
        flip: FlipTransform,
        mode: InterpolationMode,
    ) {
        if let Some(existing) = self.active.get_mut(&key) {
            existing.retarget_move(flip);
        } else {
            self.active
                .insert(key, ActiveAnim::move_from_flip(flip, mode));
        }
    }

    /// Start an enter animation, unless this key is already animating.
    pub fn start_enter(&mut self, key: AnimKey, from: (f32, f32), mode: InterpolationMode) {
        self.active
            .entry(key)
            .or_insert_with(|| ActiveAnim::enter_slide(from.0, from.1, mode));
    }

    /// Start an exit animation.
    ///
    /// If an animation is already in flight, the channels retarget from their
    /// current value with velocity preserved.
    pub fn start_exit(&mut self, key: AnimKey, to: (f32, f32), mode: InterpolationMode) {
        match self.active.get_mut(&key) {
            Some(anim) => anim.retarget_presence(AnimClass::Exit, to.0, to.1),
            None => {
                self.active
                    .insert(key, ActiveAnim::exit_slide(to.0, to.1, mode));
            }
        }
    }

    /// Mutable access to an in-flight animation.
    pub fn get_mut(&mut self, key: AnimKey) -> Option<&mut ActiveAnim> {
        self.active.get_mut(&key)
    }

    /// Read the current state for a key.
    #[must_use]
    pub fn get(&self, key: AnimKey) -> Option<&ActiveAnim> {
        self.active.get(&key)
    }

    /// Every in-flight animation with its key.
    pub fn iter(&self) -> impl Iterator<Item = (AnimKey, &ActiveAnim)> {
        self.active.iter().map(|(k, v)| (*k, v))
    }

    /// Advance every animation and drop the ones that arrived.
    ///
    /// Returns the keys that finished this tick.
    pub fn tick(&mut self, dt: f32) -> Vec<AnimKey> {
        let mut finished = Vec::new();
        for (key, anim) in &mut self.active {
            anim.tick(dt);
            if anim.is_finished() {
                finished.push(*key);
            }
        }
        for key in &finished {
            self.active.remove(key);
        }
        finished
    }

    /// Drop an animation without letting it finish.
    pub fn cancel(&mut self, key: AnimKey) -> Option<ActiveAnim> {
        self.active.remove(&key)
    }
}

/// Turn the diff's correspondence map into `(key, First, Last)` triples.
///
/// Pairs missing either rect are dropped.
pub fn correspondences_from_moves<F, L>(
    node_moves: &[NodeMove],
    new_node_data: &[NodeData],
    new_hierarchy: &[NodeHierarchyItem],
    first_rect: F,
    last_rect: L,
) -> Vec<(AnimKey, LogicalRect, LogicalRect)>
where
    F: Fn(NodeId) -> Option<LogicalRect>,
    L: Fn(NodeId) -> Option<LogicalRect>,
{
    let mut out = Vec::new();
    for m in node_moves {
        let (Some(first), Some(last)) = (first_rect(m.old_node_id), last_rect(m.new_node_id))
        else {
            continue;
        };
        if m.new_node_id.index() >= new_node_data.len() {
            continue; // stale correspondence; the new tree does not have this node
        }
        let key = AnimKey(calculate_reconciliation_key(
            new_node_data,
            new_hierarchy,
            m.new_node_id,
        ));
        out.push((key, first, last));
    }
    out
}

/// The `AnimKey` -> current `NodeId` mapping for this frame's correspondences.
///
/// Bridges the reconciliation identity and per-frame `NodeId`.
#[must_use]
pub fn anim_keys_for_moves(
    node_moves: &[NodeMove],
    new_node_data: &[NodeData],
    new_hierarchy: &[NodeHierarchyItem],
) -> Vec<(AnimKey, NodeId)> {
    node_moves
        .iter()
        .filter(|m| m.new_node_id.index() < new_node_data.len())
        .map(|m| {
            (
                AnimKey(calculate_reconciliation_key(
                    new_node_data,
                    new_hierarchy,
                    m.new_node_id,
                )),
                m.new_node_id,
            )
        })
        .collect()
}

/// Seed (or retarget) a FLIP move for every correspondence whose geometry moved.
///
/// Returns how many animations were started or retargeted.
pub fn seed_moves<I>(
    manager: &mut AnimationManager,
    correspondences: I,
    mode: InterpolationMode,
) -> usize
where
    I: IntoIterator<Item = (AnimKey, LogicalRect, LogicalRect)>,
{
    let mut seeded = 0;
    for (key, first, last) in correspondences {
        let transform = flip(first, last);
        if transform.is_identity() {
            continue;
        }
        manager.start_or_retarget_move(key, transform, mode);
        seeded += 1;
    }
    seeded
}

#[cfg(test)]
#[path = "animation_test.rs"]
mod animation_test;
