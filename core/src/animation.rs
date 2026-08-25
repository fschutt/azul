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
            interp: Interp::Curve { function, duration_secs },
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
            Interp::Curve { function, duration_secs } => {
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
                self.velocity = if dt > 0.0 { (self.current - previous) / dt } else { 0.0 };
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
        AnimationInterpolationFunction::EaseInOut
        | AnimationInterpolationFunction::Spring(_) => {
            cubic_bezier_y(0.42, 0.0, 0.58, 1.0, t)
        }
        // A CSS timing bezier is normalised to P0 = (0,0), P3 = (1,1), so only
        // the two control points carry information.
        AnimationInterpolationFunction::CubicBezier(curve) => {
            cubic_bezier_y(curve.ctrl_1.x, curve.ctrl_1.y, curve.ctrl_2.x, curve.ctrl_2.y, t)
        }
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
    pub const IDENTITY: Self =
        Self { translate_x: 0.0, translate_y: 0.0, scale_x: 1.0, scale_y: 1.0 };

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
        Interp::Curve { function, duration_secs } => {
            AnimChannel::curve(from, to, function, duration_secs)
        }
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
        Self { active: BTreeMap::new() }
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
            self.active.insert(key, ActiveAnim::move_from_flip(flip, interp));
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
                self.active.insert(key, ActiveAnim::exit_slide(to.0, to.1, interp));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{LogicalPosition, LogicalRect, LogicalSize};

    /// `Spring` is a real `AnimationInterpolationFunction` variant now, so it
    /// must survive the C-ABI enum's own accessors rather than panicking or
    /// silently answering as some other curve.
    #[test]
    fn spring_is_a_first_class_interpolation_function() {
        let f = AnimationInterpolationFunction::Spring(Spring::SNAPPY);
        assert!(f.is_spring());
        assert!(!AnimationInterpolationFunction::EaseInOut.is_spring());
        // No duration to evaluate against: it answers as the documented
        // ease-in-out stand-in, and `get_curve`/`ease` must not disagree.
        assert_eq!(f.get_curve(), AnimationInterpolationFunction::EaseInOut.get_curve());
        for t in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            assert!((ease(f, t) - ease(AnimationInterpolationFunction::EaseInOut, t)).abs() < 1e-6);
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition::new(x, y),
            size: LogicalSize::new(w, h),
        }
    }

    #[test]
    fn flip_inverts_a_pure_translation() {
        // Moved right 100 and down 50, same size: the inversion must put it back.
        let f = flip(rect(0.0, 0.0, 10.0, 10.0), rect(100.0, 50.0, 10.0, 10.0));
        assert_eq!(f.translate_x, -100.0);
        assert_eq!(f.translate_y, -50.0);
        assert_eq!(f.scale_x, 1.0);
        assert_eq!(f.scale_y, 1.0);
    }

    #[test]
    fn flip_never_scales_a_size_change() {
        // USER ruling 2026-08-17 (was `flip_inverts_a_pure_scale`, asserting
        // 0.5): a resized node has already RELAYOUTED at its final size, and
        // drawing it at half scale for the flight squashes freshly laid-out
        // content — a card growing from half-width to full-width rendered its
        // text visibly compressed for the whole transition. Size is layout's
        // job; the animation only travels.
        let f = flip(rect(0.0, 0.0, 50.0, 20.0), rect(0.0, 0.0, 100.0, 40.0));
        assert_eq!(f.scale_x, 1.0);
        assert_eq!(f.scale_y, 1.0);
        assert!(f.is_identity(), "same origin, changed size: nothing to animate");
    }

    #[test]
    fn flip_of_an_unchanged_rect_is_identity() {
        let r = rect(12.0, 34.0, 56.0, 78.0);
        assert!(flip(r, r).is_identity());
    }

    #[test]
    fn flip_never_produces_a_non_finite_scale() {
        // A collapsed target would divide by zero; the display list must never
        // see a NaN transform.
        let f = flip(rect(0.0, 0.0, 10.0, 10.0), rect(0.0, 0.0, 0.0, 0.0));
        assert!(f.scale_x.is_finite() && f.scale_y.is_finite());
        assert_eq!(f.scale_x, 1.0);
        assert_eq!(f.scale_y, 1.0);
    }

    #[test]
    fn a_spring_settles_at_its_target() {
        let mut c = AnimChannel::spring(0.0, 100.0, Spring::SMOOTH);
        for _ in 0..600 {
            c.tick(1.0 / 60.0);
            if c.is_finished() {
                break;
            }
        }
        assert!(c.is_finished(), "spring did not settle within 10s");
        assert_eq!(c.current, 100.0);
        assert_eq!(c.velocity, 0.0);
    }

    #[test]
    fn a_curve_reaches_its_target_at_the_duration() {
        // NOTE the frame budget: 60 ticks of 1/60 sum to 0.99999994, not 1.0,
        // so a curve legitimately lands on the frame AFTER its nominal
        // duration. Asserting exact arrival at tick 60 would be asserting that
        // f32 addition is exact.
        let mut c = AnimChannel::curve(0.0, 10.0, AnimationInterpolationFunction::Linear, 1.0);
        for _ in 0..60 {
            c.tick(1.0 / 60.0);
        }
        assert!(
            (c.current - 10.0).abs() < 0.01,
            "should be at the target within a frame, got {}",
            c.current
        );
        c.tick(1.0 / 60.0);
        assert!(c.is_finished(), "curve did not finish one frame past its duration");
        assert_eq!(c.current, 10.0, "a finished curve must land exactly on `to`");
    }

    #[test]
    fn retarget_preserves_position_and_velocity() {
        // THE differentiator: mid-flight redirect must not snap back to a new
        // `from`, and must keep the momentum it had.
        let mut c = AnimChannel::spring(0.0, 100.0, Spring::SMOOTH);
        for _ in 0..10 {
            c.tick(1.0 / 60.0);
        }
        let value_before = c.current;
        let velocity_before = c.velocity;
        assert!(value_before > 0.0 && velocity_before > 0.0, "should be mid-flight");

        c.retarget(-50.0);

        assert_eq!(c.current, value_before, "retarget must not move the value");
        assert_eq!(c.velocity, velocity_before, "retarget must not discard velocity");
        assert_eq!(c.from, value_before);
        assert_eq!(c.to, -50.0);
        assert!(!c.is_finished());
    }

    #[test]
    fn retargeting_to_the_same_target_does_not_restart_the_clock() {
        let mut c = AnimChannel::curve(0.0, 10.0, AnimationInterpolationFunction::Linear, 1.0);
        c.tick(0.5);
        let elapsed = c.elapsed_secs;
        c.retarget(10.0);
        assert_eq!(c.elapsed_secs, elapsed, "a no-op retarget restarted the animation");
    }

    #[test]
    fn a_settled_spring_can_be_woken_by_a_retarget() {
        let mut c = AnimChannel::spring(0.0, 1.0, Spring::SNAPPY);
        for _ in 0..600 {
            c.tick(1.0 / 60.0);
            if c.is_finished() {
                break;
            }
        }
        assert!(c.is_finished());
        c.retarget(0.0);
        assert!(!c.is_finished(), "retarget must un-finish a settled channel");
        c.tick(1.0 / 60.0);
        assert!(c.current < 1.0, "woken channel did not move toward the new target");
    }

    #[test]
    fn a_huge_frame_gap_cannot_fling_a_spring() {
        // A stalled frame must be clamped, not integrated verbatim.
        let mut c = AnimChannel::spring(0.0, 1.0, Spring::SNAPPY);
        c.tick(10.0);
        assert!(c.current.is_finite());
        assert!(c.current.abs() < 100.0, "clamping failed: {}", c.current);
    }

    #[test]
    fn zero_duration_curves_apply_instantly() {
        let mut c = AnimChannel::curve(0.0, 42.0, AnimationInterpolationFunction::Ease, 0.0);
        c.tick(0.0);
        assert!(c.is_finished());
        assert_eq!(c.current, 42.0);
    }

    #[test]
    fn easing_curves_are_pinned_at_both_ends() {
        for f in [
            AnimationInterpolationFunction::Linear,
            AnimationInterpolationFunction::Ease,
            AnimationInterpolationFunction::EaseIn,
            AnimationInterpolationFunction::EaseOut,
            AnimationInterpolationFunction::EaseInOut,
        ] {
            assert_eq!(ease(f, 0.0), 0.0, "{f:?} did not start at 0");
            assert_eq!(ease(f, 1.0), 1.0, "{f:?} did not end at 1");
            // Out of range must clamp, not extrapolate.
            assert_eq!(ease(f, -1.0), 0.0);
            assert_eq!(ease(f, 2.0), 1.0);
        }
    }

    #[test]
    fn ease_in_starts_slower_than_linear_and_ease_out_starts_faster() {
        let t = 0.25;
        let linear = ease(AnimationInterpolationFunction::Linear, t);
        assert!(ease(AnimationInterpolationFunction::EaseIn, t) < linear);
        assert!(ease(AnimationInterpolationFunction::EaseOut, t) > linear);
    }

    #[test]
    fn damping_ratio_identifies_the_regime() {
        // Critically damped: damping = 2*sqrt(k*m).
        let critical = Spring { stiffness: 100.0, damping: 20.0, mass: 1.0 };
        assert!((critical.damping_ratio() - 1.0).abs() < 1e-5);
        assert!(Spring { stiffness: 100.0, damping: 5.0, mass: 1.0 }.damping_ratio() < 1.0);
        assert!(Spring { stiffness: 100.0, damping: 40.0, mass: 1.0 }.damping_ratio() > 1.0);
    }

    #[test]
    fn a_degenerate_spring_snaps_instead_of_dividing_by_zero() {
        let s = Spring { stiffness: 100.0, damping: 10.0, mass: 0.0 };
        let (value, velocity) = s.step(0.0, 5.0, 0.0, 1.0 / 60.0);
        assert_eq!(value, 5.0);
        assert_eq!(velocity, 0.0);
    }

    #[test]
    fn the_manager_retargets_instead_of_stacking() {
        let mut m = AnimationManager::new();
        let key = AnimKey(7);
        let interp = Interp::Spring(Spring::SMOOTH);

        m.start_or_retarget_move(key, flip(rect(0.0, 0.0, 10.0, 10.0), rect(100.0, 0.0, 10.0, 10.0)), interp);
        assert_eq!(m.len(), 1);
        for _ in 0..10 {
            m.tick(1.0 / 60.0);
        }
        let mid = m.get(key).expect("still animating").current_transform();

        // A second move for the SAME key must not create a second animation.
        m.start_or_retarget_move(key, flip(rect(0.0, 0.0, 10.0, 10.0), rect(200.0, 0.0, 10.0, 10.0)), interp);
        assert_eq!(m.len(), 1, "retarget created a second animation");
        let after = m.get(key).expect("still animating").current_transform();
        assert_ne!(after.translate_x, mid.translate_x, "retarget did not fold in the new offset");
    }

    #[test]
    fn the_manager_reports_and_drops_finished_animations() {
        let mut m = AnimationManager::new();
        m.start_enter(AnimKey(1), (-120.0, 0.0), Interp::Curve {
            function: AnimationInterpolationFunction::Linear,
            duration_secs: 0.1,
        });
        assert_eq!(m.len(), 1);
        let mut finished = Vec::new();
        for _ in 0..20 {
            finished = m.tick(1.0 / 60.0);
            if !finished.is_empty() {
                break;
            }
        }
        assert_eq!(finished, alloc::vec![AnimKey(1)]);
        assert!(m.is_empty(), "finished animation was not dropped");
    }

    #[test]
    fn an_exit_replaces_an_in_flight_move() {
        // The node is leaving; continuing toward a layout slot it will never
        // occupy would be wrong.
        let mut m = AnimationManager::new();
        let key = AnimKey(3);
        let interp = Interp::Spring(Spring::SMOOTH);
        m.start_or_retarget_move(key, flip(rect(0.0, 0.0, 10.0, 10.0), rect(50.0, 0.0, 10.0, 10.0)), interp);
        assert_eq!(m.get(key).map(|a| a.class), Some(AnimClass::Move));
        m.start_exit(key, (-120.0, 0.0), interp);
        assert_eq!(m.get(key).map(|a| a.class), Some(AnimClass::Exit));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn the_anim_key_survives_a_node_id_change() {
        // THE property the whole store depends on. A keyed node that shifts
        // position in the array (a sibling was prepended) must keep its
        // identity — otherwise the second produce looks like a brand-new
        // animation and retargeting never fires.
        use crate::dom::NodeData;

        let tree_a = [NodeData::create_div().with_key("hero")];
        let tree_b = [
            NodeData::create_div().with_key("spacer"),
            NodeData::create_div().with_key("hero"),
        ];

        let key_a = AnimKey(calculate_reconciliation_key(&tree_a, &[], NodeId::ZERO));
        let key_b = AnimKey(calculate_reconciliation_key(
            &tree_b,
            &[],
            NodeId::new(1),
        ));
        assert_eq!(key_a, key_b, "the same keyed node got two different AnimKeys");

        // And a DIFFERENT key must not collide with it.
        let other = AnimKey(calculate_reconciliation_key(&tree_b, &[], NodeId::ZERO));
        assert_ne!(key_a, other);
    }

    #[test]
    fn correspondences_drop_pairs_with_no_geometry() {
        use crate::dom::NodeData;
        let new_data = [NodeData::create_div().with_key("a"), NodeData::create_div().with_key("b")];
        let moves = [
            NodeMove { old_node_id: NodeId::ZERO, new_node_id: NodeId::ZERO },
            NodeMove { old_node_id: NodeId::new(1), new_node_id: NodeId::new(1) },
        ];
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let out = correspondences_from_moves(
            &moves,
            &new_data,
            &[],
            |id| (id == NodeId::ZERO).then_some(r),   // only node 0 existed before
            |_| Some(rect(5.0, 0.0, 10.0, 10.0)),
        );
        assert_eq!(out.len(), 1, "a node with no previous geometry has nothing to fly from");
    }

    #[test]
    fn seed_moves_skips_nodes_that_did_not_move() {
        // An identity FLIP would take a GPU key and animate nothing.
        let mut m = AnimationManager::new();
        let stayed = rect(0.0, 0.0, 10.0, 10.0);
        let moved_first = rect(0.0, 0.0, 10.0, 10.0);
        let moved_last = rect(40.0, 0.0, 10.0, 10.0);
        let seeded = seed_moves(
            &mut m,
            [
                (AnimKey(1), stayed, stayed),
                (AnimKey(2), moved_first, moved_last),
            ],
            Interp::Spring(Spring::SMOOTH),
        );
        assert_eq!(seeded, 1, "only the node that moved should animate");
        assert!(m.get(AnimKey(1)).is_none());
        assert!(m.get(AnimKey(2)).is_some());
    }

    #[test]
    fn seed_moves_retargets_a_key_that_is_already_animating() {
        // Two produces in quick succession must not stack two animations on
        // one node — that is the visible "fighting" artefact.
        let mut m = AnimationManager::new();
        let interp = Interp::Spring(Spring::SMOOTH);
        seed_moves(
            &mut m,
            [(AnimKey(9), rect(0.0, 0.0, 10.0, 10.0), rect(50.0, 0.0, 10.0, 10.0))],
            interp,
        );
        for _ in 0..5 {
            m.tick(1.0 / 60.0);
        }
        let seeded = seed_moves(
            &mut m,
            [(AnimKey(9), rect(0.0, 0.0, 10.0, 10.0), rect(90.0, 0.0, 10.0, 10.0))],
            interp,
        );
        assert_eq!(seeded, 1);
        assert_eq!(m.len(), 1, "a second produce stacked a second animation");
    }

    #[test]
    fn an_enter_does_not_clobber_an_animation_already_in_flight() {
        let mut m = AnimationManager::new();
        let key = AnimKey(5);
        let interp = Interp::Spring(Spring::SMOOTH);
        m.start_exit(key, (-120.0, 0.0), interp);
        m.start_enter(key, (-120.0, 0.0), interp);
        assert_eq!(m.get(key).map(|a| a.class), Some(AnimClass::Exit));
    }
}
