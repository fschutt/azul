//! DOM-morph animation: interpolation core, FLIP geometry, and the keyed
//! animation store.
//!
//! # The model
//!
//! Animation here is **not** a new scheduler. It compiles down to keyed timer
//! callbacks writing into the top cascade layer (`user_overridden_properties`),
//! exactly as the existing caret/selection tweens do. This module owns only the
//! parts that are pure math plus the bookkeeping that has to survive across
//! frames:
//!
//! * [`Spring`] / [`AnimChannel`] — how a scalar gets from `from` to `to`, with
//!   **interruption as a first-class operation** ([`AnimChannel::retarget`]).
//! * [`flip`] — the First/Last inversion that turns a layout change into a
//!   composited transform, so a move costs a GPU key rather than a relayout.
//! * [`AnimationManager`] — the keyed store. Keys are reconciliation identities,
//!   which is what makes retargeting possible at all: A→B→C finds the in-flight
//!   state instead of starting a second animation over the top of the first.
//!
//! # Why springs rather than only easing curves
//!
//! A cubic-bezier is a function of *normalised time*, so interrupting one and
//! starting another discards the current velocity — the element visibly snaps.
//! A spring integrates from the current `(value, velocity)`, so a retarget mid
//! flight continues smoothly. That is the whole reason drag-release and
//! rapid A→B→C feel right.
//!
//! `Spring` is a real `#[repr(C)]` variant of `AnimationInterpolationFunction`,
//! so a spring is expressible in CSS and across the C ABI like any other timing
//! function — not a Rust-only concept bolted beside one. [`SpringCurve`] lives
//! in `azul-css` next to the enum it belongs to and is re-exported here as
//! [`Spring`], so there is exactly one definition of a type that crosses the
//! ABI.
//!
//! That variant is unlike the others in one way worth knowing: it has **no
//! duration**, so it cannot be evaluated at a normalised `t`. Anything holding a
//! timeline must branch on `is_spring()`; `AnimChannel` does this by dispatching
//! on [`Interp`], which is why springs never reach [`ease`].
//!
//! [`SpringCurve`]: azul_css::props::basic::animation::SpringCurve
//!
//! # no_std
//!
//! `alloc` only. Note the module-level `use alloc::vec::Vec` — a body-level
//! `use` would not be in scope for function *signatures*, which is precisely
//! how a `--no-default-features` build was broken once already.

use alloc::{collections::BTreeMap, vec::Vec};

use azul_css::props::basic::animation::AnimationInterpolationFunction;

use crate::{
    diff::{calculate_reconciliation_key, NodeMove},
    dom::NodeData,
    geom::LogicalRect,
    id::NodeId,
    styled_dom::NodeHierarchyItem,
};

/// Mass-spring-damper parameters.
///
/// Re-exported from `azul_css` so there is ONE definition: the spring is part
/// of `AnimationInterpolationFunction`, which crosses the C ABI, and a second
/// structurally-identical `Spring` in core would be a silent ABI trap the day
/// one of them gained a field.
///
/// Integrated with semi-implicit (symplectic) Euler, which is the cheap,
/// stable choice for interactive springs: it does not blow up at the frame
/// rates a UI actually sees, and unlike the closed-form solution it needs no
/// case split on the damping regime.
pub use azul_css::props::basic::animation::SpringCurve as Spring;

/// How a channel is driven.
///
/// Wraps [`AnimationInterpolationFunction`] rather than extending it — see the
/// module docs on the ABI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Interp {
    /// Duration-based easing. Not interruptible without a visible discontinuity.
    Curve {
        /// The CSS easing curve.
        function: AnimationInterpolationFunction,
        /// Total duration in seconds. Zero means "apply instantly".
        duration_secs: f32,
    },
    /// Physics-based. Interruptible with velocity continuity.
    Spring(Spring),
}

impl Default for Interp {
    fn default() -> Self {
        Self::Spring(Spring::SMOOTH)
    }
}

/// One animated scalar.
///
/// Compose these for anything richer: a FLIP move is four channels (translate
/// x/y, scale x/y), a fade is one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimChannel {
    /// Where the value started. Re-seeded on every retarget.
    pub from: f32,
    /// Where it is heading.
    pub to: f32,
    /// The value right now — what the caller writes into the cascade.
    pub current: f32,
    /// Units per second. Carried across retargets; that is the point.
    pub velocity: f32,
    /// Seconds since this leg started (curve mode only).
    pub elapsed_secs: f32,
    /// How it is driven.
    pub interp: Interp,
    /// Latched once the channel arrives, so `is_finished` cannot flicker.
    finished: bool,
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
            interp: Interp::Curve {
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
            interp: Interp::Spring(spring),
            finished: false,
        }
    }

    /// Advance by `dt` seconds and return the new current value.
    pub fn tick(&mut self, dt: f32) -> f32 {
        if self.finished {
            return self.current;
        }
        match self.interp {
            Interp::Curve {
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
                // Explicit FP on purpose: mul_add fuses only with +fma and
                // changes results bit-for-bit; animation sampling must stay
                // bit-reproducible. (clippy::suboptimal_flops)
                #[allow(clippy::suboptimal_flops)]
                {
                    self.current = self.from + (self.to - self.from) * eased;
                }
                // Track velocity even on curves: if this channel is later
                // retargeted onto a spring, the handover is continuous.
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
            Interp::Spring(spring) => {
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

    /// Whether this channel has arrived and can be dropped.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Aim at a new target **without losing the current value or velocity**.
    ///
    /// This is the operation a browser's WAAPI cannot express: there, a new
    /// animation replaces the old one and the element jumps to the new `from`.
    /// Here A→B→C mid-flight continues from wherever it actually is, at the
    /// speed it is actually travelling.
    pub fn retarget(&mut self, new_to: f32) {
        if (self.to - new_to).abs() < f32::EPSILON && !self.finished {
            return; // already heading there; do not restart the clock
        }
        self.from = self.current;
        self.to = new_to;
        self.elapsed_secs = 0.0;
        self.finished = false;
        // `velocity` is deliberately NOT reset — that is the whole feature.
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
        // `Spring(_)` shares the ease-in-out body ON PURPOSE: a spring has
        // no `t` — reaching here means a caller put a spring where a
        // duration-based curve was expected (`AnimChannel` dispatches on
        // `Interp`, so a spring never takes this path — it integrates in
        // `Spring::step` from its live (value, velocity)). The stand-in
        // matches `AnimationInterpolationFunction::get_curve`, so the two
        // disagree nowhere, and it degrades to plausible motion rather than
        // a panic or a frozen element.
        AnimationInterpolationFunction::EaseInOut | AnimationInterpolationFunction::Spring(_) => {
            cubic_bezier_y(0.42, 0.0, 0.58, 1.0, t)
        }
        // A CSS timing bezier is normalised to P0 = (0,0), P3 = (1,1), so only
        // the two control points carry information.
        AnimationInterpolationFunction::CubicBezier(curve) => cubic_bezier_y(
            curve.ctrl_1.x,
            curve.ctrl_1.y,
            curve.ctrl_2.x,
            curve.ctrl_2.y,
            t,
        ),
    }
}

/// y of a CSS timing bezier at parameter x, with P0 = (0,0) and P3 = (1,1).
///
/// CSS timing functions are parameterised by x (progress), not by the curve's
/// own parameter, so x must be inverted first. Newton converges in a couple of
/// iterations for the well-behaved curves; the bisection fallback keeps it
/// correct for curves with near-zero derivative.
fn cubic_bezier_y(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    const NEWTON_ITERATIONS: usize = 4;
    const BISECTION_ITERATIONS: usize = 12;
    const EPSILON: f32 = 1e-5;

    // Explicit FP on purpose: mul_add fuses only with +fma and changes
    // results bit-for-bit; easing must stay bit-reproducible across builds.
    // (clippy::suboptimal_flops)
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
/// Applied to an element already laid out at Last, it makes the element *appear*
/// at First. Animating these four numbers to identity plays the move on the GPU
/// with no relayout — which is why a move costs a transform key rather than a
/// per-frame re-solve.
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

    /// Whether this is close enough to identity that emitting it is pointless.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.translate_x.abs() < 0.01
            && self.translate_y.abs() < 0.01
            && (self.scale_x - 1.0).abs() < 0.001
            && (self.scale_y - 1.0).abs() < 0.001
    }
}

/// Compute the FLIP inversion from a First (pre-change) and Last (post-change) rect.
///
/// Degenerate Last extents fall back to scale 1 rather than producing infinities:
/// a zero-sized target is a collapsed or not-yet-measured node, and a NaN
/// transform would poison the display list.
/// POSITION ONLY — a USER ruling (2026-08-17), not an omission, and also what
/// the design doc's Move row specifies ("FLIP: transform Δ→identity"). A
/// matched node whose size changed has already RELAYOUTED at its final size;
/// scaling it from the old size squashes freshly laid-out content (text set
/// for the wide layout rendered at half width) for the whole flight. Content
/// never distorts unless the user explicitly animates CSS `transform`, which
/// is a pure transform without relayout — there, distortion is the point.
/// A move travels; it does not morph.
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
///
/// These map 1:1 onto what the diff already reports: unmatched-new is Enter,
/// unmatched-old is Exit, and a `NodeMove` pair whose geometry changed is Move.
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
/// This is deliberately **not** a `NodeId`: node ids are array positions and are
/// not stable across a re-produce, so keying on them would make every frame look
/// like a fresh animation and retargeting would never fire. The reconciliation
/// key (`.with_key()` / `#id` / structural hash) is what survives.
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
    pub const fn move_from_flip(flip: FlipTransform, interp: Interp) -> Self {
        Self {
            class: AnimClass::Move,
            translate_x: channel(flip.translate_x, 0.0, interp),
            translate_y: channel(flip.translate_y, 0.0, interp),
            scale_x: channel(flip.scale_x, 1.0, interp),
            scale_y: channel(flip.scale_y, 1.0, interp),
            opacity: channel(1.0, 1.0, interp),
        }
    }

    /// An enter: SLIDE IN from `(from_x, from_y)` to identity, full opacity,
    /// full size.
    ///
    /// Was fade+scale-up; changed by the same USER ruling as [`flip`]:
    /// presence changes travel, content never distorts or ghosts. The offset
    /// is the caller's choice — the engine default slides from the nearest
    /// viewport edge, so a sidebar re-opens the way it left.
    #[must_use]
    pub const fn enter_slide(from_x: f32, from_y: f32, interp: Interp) -> Self {
        Self {
            class: AnimClass::Enter,
            translate_x: channel(from_x, 0.0, interp),
            translate_y: channel(from_y, 0.0, interp),
            scale_x: channel(1.0, 1.0, interp),
            scale_y: channel(1.0, 1.0, interp),
            opacity: channel(1.0, 1.0, interp),
        }
    }

    /// An exit: SLIDE OUT from identity to `(to_x, to_y)`, full opacity,
    /// full size. Only visible with exit-retention.
    ///
    /// Was fade+shrink-in-place; same USER ruling as [`flip`]: a departing
    /// sidebar slides away to its edge — it does not dissolve.
    ///
    /// Reverse a presence animation IN FLIGHT: the channels retarget from
    /// their CURRENT values (velocity preserved — `Channel::retarget`'s whole
    /// feature) toward the new destination. `Exit` + a slide target turns an
    /// entering node around; `Enter` + identity catches an exiting node
    /// (the remount-mid-exit catch: the zombie is dropped and the LIVE node
    /// travels home from wherever the exit had carried it).
    pub fn retarget_presence(&mut self, class: AnimClass, to_x: f32, to_y: f32) {
        self.class = class;
        self.translate_x.retarget(to_x);
        self.translate_y.retarget(to_y);
        self.scale_x.retarget(1.0);
        self.scale_y.retarget(1.0);
        self.opacity.retarget(1.0);
    }

    #[must_use]
    pub const fn exit_slide(to_x: f32, to_y: f32, interp: Interp) -> Self {
        Self {
            class: AnimClass::Exit,
            translate_x: channel(0.0, to_x, interp),
            translate_y: channel(0.0, to_y, interp),
            scale_x: channel(1.0, 1.0, interp),
            scale_y: channel(1.0, 1.0, interp),
            opacity: channel(1.0, 1.0, interp),
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
        // The NEW inversion is where the element must appear to start, so the
        // channels are re-seeded toward identity from wherever they are now.
        self.translate_x.retarget(0.0);
        self.translate_y.retarget(0.0);
        self.scale_x.retarget(1.0);
        self.scale_y.retarget(1.0);
        // Fold the freshly measured offset in, rather than snapping to it.
        self.translate_x.current += flip.translate_x;
        self.translate_y.current += flip.translate_y;
    }
}

const fn channel(from: f32, to: f32, interp: Interp) -> AnimChannel {
    match interp {
        Interp::Curve {
            function,
            duration_secs,
        } => AnimChannel::curve(from, to, function, duration_secs),
        Interp::Spring(spring) => AnimChannel::spring(from, to, spring),
    }
}

/// The keyed store of in-flight animations.
///
/// Sibling to `GpuStateManager`. Holds only what must outlive a frame; the
/// actual writing of values happens in the layout crate, which owns the cascade.
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

    /// Start a move, or **retarget** one already in flight under this key.
    ///
    /// This is the entry point that makes rapid A→B→C smooth: the second call
    /// does not stack a new animation on the first, it redirects it.
    pub fn start_or_retarget_move(&mut self, key: AnimKey, flip: FlipTransform, interp: Interp) {
        if let Some(existing) = self.active.get_mut(&key) {
            existing.retarget_move(flip);
        } else {
            self.active
                .insert(key, ActiveAnim::move_from_flip(flip, interp));
        }
    }

    /// Start an enter animation, unless this key is already animating.
    pub fn start_enter(&mut self, key: AnimKey, from: (f32, f32), interp: Interp) {
        self.active
            .entry(key)
            .or_insert_with(|| ActiveAnim::enter_slide(from.0, from.1, interp));
    }

    /// Start an exit animation. An exit always WINS — the node is leaving, so
    /// continuing toward a layout position it will never occupy is wrong —
    /// but it does not RESTART: if an animation is already in flight under
    /// this key (a node unmounted mid-enter, or mid-move), the channels
    /// RETARGET from their current value with velocity preserved, so the
    /// node turns around instead of snapping to its laid-out position first.
    pub fn start_exit(&mut self, key: AnimKey, to: (f32, f32), interp: Interp) {
        match self.active.get_mut(&key) {
            Some(anim) => anim.retarget_presence(AnimClass::Exit, to.0, to.1),
            None => {
                self.active
                    .insert(key, ActiveAnim::exit_slide(to.0, to.1, interp));
            }
        }
    }

    /// Mutable access to an in-flight animation — the mid-flight-catch hook.
    pub fn get_mut(&mut self, key: AnimKey) -> Option<&mut ActiveAnim> {
        self.active.get_mut(&key)
    }

    /// Read the current state for a key.
    #[must_use]
    pub fn get(&self, key: AnimKey) -> Option<&ActiveAnim> {
        self.active.get(&key)
    }

    /// Every in-flight animation with its key.
    ///
    /// The compositor needs this each frame to turn identity-keyed animation
    /// state into per-`NodeId` GPU values; it cannot ask for keys it does not
    /// already know about.
    pub fn iter(&self) -> impl Iterator<Item = (AnimKey, &ActiveAnim)> {
        self.active.iter().map(|(k, v)| (*k, v))
    }

    /// Advance every animation and drop the ones that arrived.
    ///
    /// Returns the keys that finished this tick, so the caller can release the
    /// GPU keys and — for exits — drop the retained subtree exactly once.
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

    /// Drop an animation without letting it finish (e.g. its node vanished).
    pub fn cancel(&mut self, key: AnimKey) -> Option<ActiveAnim> {
        self.active.remove(&key)
    }
}

/// Turn the diff's correspondence map into `(key, First, Last)` triples.
///
/// `node_moves` is what `reconcile_dom` already produced: old `NodeId` ->
/// new `NodeId` for every node that survived the re-produce. This pairs each
/// one with its pre-swap and post-solve geometry so [`seed_moves`] can decide
/// what actually moved.
///
/// Geometry is fetched through closures rather than a map parameter because the
/// two rects live in different crates and different *phases*: First comes from
/// the previous frame's `LayoutCache` (still alive at the diff seam), Last from
/// the freshly solved layout, which does not exist until well after that seam.
/// Passing accessors lets the caller bridge that gap without core depending on
/// the layout crate.
///
/// The key is the **reconciliation key**, the same identity the diff matched on
/// — not the `NodeId`. A `NodeId` is an array position: it can change for a
/// node that did not move, and can be reused by an unrelated node. Keying an
/// animation store on it would make retargeting fire on the wrong element, or
/// not at all.
///
/// Pairs missing either rect are dropped: a node with no previous geometry has
/// nothing to fly from, and one with no new geometry is not on screen to fly to.
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

/// The `AnimKey` → current `NodeId` mapping for this frame's correspondences.
///
/// Animation state is keyed by reconciliation identity so it can outlive a
/// rebuild, but the compositor writes GPU values per `NodeId`. Something has to
/// bridge the two, and it has to be rebuilt every layout: the key is stable
/// across rebuilds precisely because the `NodeId` is not.
///
/// Kept separate from [`correspondences_from_moves`] rather than folded into its
/// return type, because the two have different lifetimes — the correspondences
/// are consumed once at seed time, this map is read every frame until the
/// animation settles.
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
/// This is the engine entry point for Phase 1: the caller hands over the
/// old↔new correspondences the diff already produced, paired with the First
/// (pre-swap) and Last (post-solve) rects, and every pair that actually moved
/// becomes a composited transform animation. Pairs that did not move are
/// skipped — seeding an identity FLIP would allocate a GPU key and animate
/// nothing.
///
/// Returns how many animations were started or retargeted, which is exactly the
/// number of nodes that need a GPU transform key this frame.
pub fn seed_moves<I>(manager: &mut AnimationManager, correspondences: I, interp: Interp) -> usize
where
    I: IntoIterator<Item = (AnimKey, LogicalRect, LogicalRect)>,
{
    let mut seeded = 0;
    for (key, first, last) in correspondences {
        let transform = flip(first, last);
        if transform.is_identity() {
            continue;
        }
        manager.start_or_retarget_move(key, transform, interp);
        seeded += 1;
    }
    seeded
}
