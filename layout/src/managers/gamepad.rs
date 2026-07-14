//! Gamepad manager — cross-platform state for the controller surface
//! (`SUPER_PLAN_2` §1 feature 6 + research/03).
//!
//! Poll + push-driven, like the sensors:
//!
//! - The **platform backend** (`dll/src/desktop/extra/gamepad/<plat>.rs`)
//!   polls `gilrs` / iOS `GCController` / Android `InputDevice` and calls
//!   [`push_gamepad_state`] whenever a pad's state changes.
//! - The dll **layout pass** drains the channel via
//!   [`drain_gamepad_states`] and folds each into the manager through
//!   [`GamepadManager::set_state`].
//! - **Callbacks** read [`GamepadManager::state`] / [`GamepadManager::primary`]
//!   synchronously (via `CallbackInfo::get_gamepad_state`) to drive
//!   movement / menu UI.
//!
//! Unlike the sensors' fixed three slots, the set of pads is dynamic: one
//! [`GamepadState`] slot per [`GamepadId`] seen this session, kept across
//! frames so a disconnect stays observable (`connected = false`). No
//! platform deps (`SUPER_PLAN_2` §0.5); the channel mirrors `sensors.rs`.

use alloc::vec::Vec;

use azul_core::dom::DomNodeId;
use azul_core::events::{
    EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
};
use azul_core::task::Instant;
pub use azul_core::gamepad::{GamepadAxis, GamepadButton, GamepadId, GamepadState};

/// Cross-platform gamepad state. One per `App` — the OS exposes a single
/// per-process controller subscription, not per-window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GamepadManager {
    /// One slot per pad seen this session; `connected` flips to `false` on
    /// unplug (the slot is retained so a callback can observe it).
    pads: Vec<GamepadState>,
    /// `true` when a pad's state advanced since the last event-pass drain.
    /// Set by [`set_state`](Self::set_state); cleared by the dll after dispatch.
    pending_event: bool,
    /// `true` while any node in the current layout registers a
    /// `GamepadInput` callback (Hover or Window filter). Recomputed on every
    /// relayout by the DOM walk in `shell2::common::layout`; the capability
    /// pump polls gilrs/GCController only while this is set (MWA-A1 arming
    /// signal — no listeners, no polling, no ~16ms timer).
    has_listeners: bool,
}

impl GamepadManager {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Latest state for `id`, or `None` if that pad was never seen.
    #[must_use] pub fn state(&self, id: GamepadId) -> Option<GamepadState> {
        self.pads.iter().find(|p| p.id == id).copied()
    }

    /// The first currently-connected pad — the common single-controller
    /// case, so a callback doesn't have to track ids.
    #[must_use] pub fn primary(&self) -> Option<GamepadState> {
        self.pads.iter().find(|p| p.connected).copied()
    }

    /// Every pad slot seen this session (connected or not).
    #[must_use] pub fn gamepads(&self) -> &[GamepadState] {
        &self.pads
    }

    /// Apply a state the backend delivered (upsert by id). Returns `true`
    /// if it advanced (bit-pattern different from the previous slot), so an
    /// idle controller doesn't make every frame look "changed".
    pub fn set_state(&mut self, state: GamepadState) -> bool {
        let changed = if let Some(slot) = self.pads.iter_mut().find(|p| p.id == state.id) {
            let changed = !state_bitwise_eq(slot, &state);
            *slot = state;
            changed
        } else {
            self.pads.push(state);
            true
        };
        if changed {
            self.pending_event = true;
        }
        changed
    }

    /// Clear the pending-event flag. The dll calls this after the event pass
    /// has collected the `GamepadInput` event.
    pub const fn clear_pending_event(&mut self) {
        self.pending_event = false;
    }

    /// Relayout walk reports whether any node listens for `GamepadInput`.
    pub const fn set_has_listeners(&mut self, has: bool) {
        self.has_listeners = has;
    }

    /// `true` while the capability pump should poll the controller backend.
    #[must_use] pub const fn has_listeners(&self) -> bool {
        self.has_listeners
    }
}

impl EventProvider for GamepadManager {
    /// Yield a window-level `GamepadInput` event when a pad's state advanced
    /// since the last drain (target = root; read it via
    /// `CallbackInfo::get_primary_gamepad` / `get_gamepad_state`).
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        if self.pending_event {
            alloc::vec![SyntheticEvent::new(
                EventType::GamepadInput,
                CoreEventSource::User,
                DomNodeId::ROOT,
                timestamp,
                EventData::None,
            )]
        } else {
            Vec::new()
        }
    }
}

fn state_bitwise_eq(a: &GamepadState, b: &GamepadState) -> bool {
    a.id == b.id
        && a.connected == b.connected
        && a.buttons == b.buttons
        && a.left_stick_x.to_bits() == b.left_stick_x.to_bits()
        && a.left_stick_y.to_bits() == b.left_stick_y.to_bits()
        && a.right_stick_x.to_bits() == b.right_stick_x.to_bits()
        && a.right_stick_y.to_bits() == b.right_stick_y.to_bits()
        && a.left_z.to_bits() == b.left_z.to_bits()
        && a.right_z.to_bits() == b.right_z.to_bits()
}

// ────────── Async update channel (platform backend → manager) ──────────
//
// gilrs / GCController / InputDevice deliver on the backend's poll thread
// with no handle to the live `GamepadManager` (inside the window's
// `LayoutWindow`). The backend parks each changed state here; the layout
// pass drains it and applies the latest per id. Pure Rust — no platform
// dependency (SUPER_PLAN_2 §0.5). Mirrors the sensor reading channel.

static PENDING_STATES: std::sync::Mutex<Vec<GamepadState>> = std::sync::Mutex::new(Vec::new());

/// Park a gamepad state delivered by a platform backend (in the dll).
/// Thread-safe; poison-recovering.
pub fn push_gamepad_state(state: GamepadState) {
    let mut q = PENDING_STATES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(state);
}

/// Drain every state parked by [`push_gamepad_state`], in arrival order.
/// Called once per layout pass; the caller applies them through
/// [`GamepadManager::set_state`] (the last per id wins).
pub fn drain_gamepad_states() -> Vec<GamepadState> {
    let mut q = PENDING_STATES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(id: u32, connected: bool, buttons: u32) -> GamepadState {
        let mut s = GamepadState::empty(GamepadId { id });
        s.connected = connected;
        s.buttons = buttons;
        s
    }

    #[test]
    fn manager_upserts_by_id_and_flags_change() {
        let mut mgr = GamepadManager::new();
        assert_eq!(mgr.state(GamepadId { id: 0 }), None);
        // First state for an id is a change + adds a slot.
        assert!(mgr.set_state(st(0, true, 0b1)));
        assert!(mgr.state(GamepadId { id: 0 }).is_some());
        // Same state again — no change.
        assert!(!mgr.set_state(st(0, true, 0b1)));
        // Different buttons — change, same slot (not a new pad).
        assert!(mgr.set_state(st(0, true, 0b11)));
        assert_eq!(mgr.gamepads().len(), 1);
        // A second pad adds a slot.
        assert!(mgr.set_state(st(1, true, 0)));
        assert_eq!(mgr.gamepads().len(), 2);
    }

    #[test]
    fn primary_is_first_connected() {
        let mut mgr = GamepadManager::new();
        mgr.set_state(st(0, false, 0)); // disconnected
        mgr.set_state(st(1, true, 0));
        assert_eq!(mgr.primary().map(|p| p.id.id), Some(1));
    }

    #[test]
    fn is_pressed_decodes_the_bitset() {
        let s = st(0, true, GamepadButton::South.bit() | GamepadButton::Start.bit());
        assert!(s.is_pressed(GamepadButton::South));
        assert!(s.is_pressed(GamepadButton::Start));
        assert!(!s.is_pressed(GamepadButton::East));
    }

    #[test]
    fn listener_flag_gates_polling_decision() {
        let mut mgr = GamepadManager::new();
        assert!(!mgr.has_listeners(), "no listeners until the relayout walk reports some");
        mgr.set_has_listeners(true);
        assert!(mgr.has_listeners());
        mgr.set_has_listeners(false);
        assert!(!mgr.has_listeners());
    }

    #[test]
    fn states_round_trip_through_the_channel() {
        drop(drain_gamepad_states());
        push_gamepad_state(st(0, true, 0b1));
        push_gamepad_state(st(0, true, 0b10)); // last per id wins
        push_gamepad_state(st(1, true, 0));
        let drained = drain_gamepad_states();
        assert_eq!(drained.len(), 3);

        let mut mgr = GamepadManager::new();
        for s in &drained {
            mgr.set_state(*s);
        }
        assert_eq!(mgr.state(GamepadId { id: 0 }).map(|p| p.buttons), Some(0b10));
        assert_eq!(mgr.gamepads().len(), 2);
        assert!(drain_gamepad_states().is_empty());
    }
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::task::SystemTick;

    use super::*;

    // NOTE on coverage: `push_gamepad_state` / `drain_gamepad_states` are
    // deliberately NOT tested here. They share one process-global
    // `PENDING_STATES` mutex, and `tests::states_round_trip_through_the_channel`
    // above already asserts an *exact* drained length on it. Since the test
    // harness runs both modules on parallel threads in the same binary, a push
    // or drain from here could interleave with that test's push/drain window and
    // make it fail spuriously. The round-trip is already covered there.

    /// Every axis field, paired with a setter, so a test can walk all six
    /// without depending on `GamepadState::axis` (a bug there must not mask a
    /// bug here).
    const AXIS_SETTERS: [(&str, fn(&mut GamepadState, f32)); 6] = [
        ("left_stick_x", |s, v| s.left_stick_x = v),
        ("left_stick_y", |s, v| s.left_stick_y = v),
        ("right_stick_x", |s, v| s.right_stick_x = v),
        ("right_stick_y", |s, v| s.right_stick_y = v),
        ("left_z", |s, v| s.left_z = v),
        ("right_z", |s, v| s.right_z = v),
    ];

    /// Floats a real backend can hand us that break naive `==` comparison.
    const NASTY_FLOATS: [f32; 10] = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        -0.0,
        -1.0,
        1.0,
        f32::MIN,
        f32::MAX,
        f32::MIN_POSITIVE,
    ];

    fn pad(id: u32) -> GamepadState {
        GamepadState::empty(GamepadId { id })
    }

    fn connected(id: u32) -> GamepadState {
        let mut s = pad(id);
        s.connected = true;
        s
    }

    fn ts(tick: u64) -> Instant {
        Instant::Tick(SystemTick::new(tick))
    }

    /// Field-by-field bit comparison, written independently of
    /// `state_bitwise_eq` so it can be used to check that function.
    fn same_bits(a: &GamepadState, b: &GamepadState) -> bool {
        a.id == b.id
            && a.connected == b.connected
            && a.buttons == b.buttons
            && [
                (a.left_stick_x, b.left_stick_x),
                (a.left_stick_y, b.left_stick_y),
                (a.right_stick_x, b.right_stick_x),
                (a.right_stick_y, b.right_stick_y),
                (a.left_z, b.left_z),
                (a.right_z, b.right_z),
            ]
            .iter()
            .all(|(x, y)| x.to_bits() == y.to_bits())
    }

    // ------------------------------------------------------------------
    // GamepadManager::new  (constructor)
    // ------------------------------------------------------------------

    /// no_panic + invariants_hold: a fresh manager is the documented zero —
    /// no slots, nothing pending, no listeners — and is indistinguishable from
    /// `Default` (the dll builds one per `App` either way).
    #[test]
    fn new_is_default_and_starts_completely_empty() {
        let mgr = GamepadManager::new();

        assert_eq!(mgr, GamepadManager::default());
        assert!(mgr.gamepads().is_empty());
        assert_eq!(mgr.gamepads().len(), 0);
        assert_eq!(mgr.primary(), None);
        assert!(!mgr.has_listeners(), "polling must be disarmed until a relayout arms it");
        assert!(!mgr.pending_event, "a manager nobody touched cannot have a pending event");
        assert!(
            mgr.get_pending_events(ts(0)).is_empty(),
            "a fresh manager must not synthesise a GamepadInput event"
        );
    }

    // ------------------------------------------------------------------
    // GamepadManager::state  (other)
    // ------------------------------------------------------------------

    /// no_panic_smoke + boundary ids: an id that was never seen is `None`, on
    /// an empty manager *and* on a populated one. `u32::MAX` / `0` are the
    /// interesting ones — the backend normalises platform device ids into a
    /// `u32`, so both ends of the range are reachable.
    #[test]
    fn state_returns_none_for_ids_never_seen() {
        let mut mgr = GamepadManager::new();
        for id in [0, 1, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            assert_eq!(mgr.state(GamepadId { id }), None, "id {id} on an empty manager");
        }

        mgr.set_state(connected(7));
        assert!(mgr.state(GamepadId { id: 7 }).is_some());
        for id in [0, 6, 8, u32::MAX] {
            assert_eq!(mgr.state(GamepadId { id }), None, "id {id} was never pushed");
        }
    }

    /// The lookup must key on the *whole* id, not a truncated / masked one:
    /// pads whose ids differ only in the low or high bits must not alias.
    #[test]
    fn state_does_not_alias_neighbouring_or_truncated_ids() {
        let mut mgr = GamepadManager::new();
        // (id, fingerprint) — ids that differ only in the low or the high half.
        let pads = [
            (0u32, 0x0000_0001u32),
            (1, 0x0000_0002),
            (0xFFFF, 0x0000_0004),
            (0x1_0000, 0x0000_0008),
            (u32::MAX - 1, 0x0000_0010),
            (u32::MAX, 0x0000_0020),
        ];
        for (id, fingerprint) in pads {
            let mut s = connected(id);
            s.buttons = fingerprint;
            mgr.set_state(s);
        }
        assert_eq!(
            mgr.gamepads().len(),
            pads.len(),
            "distinct ids must not collapse into one slot"
        );
        for (id, fingerprint) in pads {
            let got = mgr.state(GamepadId { id }).expect("pad was pushed");
            assert_eq!(got.id.id, id);
            assert_eq!(got.buttons, fingerprint, "id {id} returned another pad's snapshot");
        }
    }

    /// round-trip / encode == decode: whatever the backend delivered comes back
    /// out of `state()` bit-for-bit, including the floats `==` would mangle
    /// (NaN, ±inf, −0.0) and a fully-set button bitset.
    #[test]
    fn state_returns_the_pushed_snapshot_bit_exactly() {
        let mut mgr = GamepadManager::new();

        let mut s = connected(u32::MAX);
        s.buttons = u32::MAX;
        s.left_stick_x = f32::NAN;
        s.left_stick_y = -0.0;
        s.right_stick_x = f32::INFINITY;
        s.right_stick_y = f32::NEG_INFINITY;
        s.left_z = f32::MIN;
        s.right_z = f32::MAX;
        mgr.set_state(s);

        let got = mgr.state(GamepadId { id: u32::MAX }).expect("pad u32::MAX was pushed");
        assert!(
            same_bits(&got, &s),
            "state() did not return the pushed snapshot bit-exactly: {got:?} vs {s:?}"
        );
        // …and the NaN axis really is a NaN, i.e. nothing sanitised it on the way.
        assert!(got.left_stick_x.is_nan());
        assert_eq!(got.left_stick_y.to_bits(), (-0.0f32).to_bits(), "−0.0 collapsed to +0.0");
    }

    /// `state()` is a read-only view: calling it (even for a missing id) leaves
    /// the manager — slots, pending flag, listener flag — untouched.
    #[test]
    fn state_does_not_mutate_the_manager() {
        let mut mgr = GamepadManager::new();
        mgr.set_state(connected(0));
        mgr.clear_pending_event();
        let before = mgr.clone();

        for id in [0, 1, u32::MAX] {
            let _ = mgr.state(GamepadId { id });
        }
        assert_eq!(mgr, before);
        assert!(!mgr.pending_event, "a read must not raise the pending-event flag");
    }

    // ------------------------------------------------------------------
    // GamepadManager::primary  (getter)
    // ------------------------------------------------------------------

    /// edge_access: `None` on a default manager, and still `None` once pads
    /// exist but every one of them is disconnected (the slots are retained, so
    /// "has slots" must not be confused with "has a pad").
    #[test]
    fn primary_is_none_when_empty_or_when_nothing_is_connected() {
        let mut mgr = GamepadManager::new();
        assert_eq!(mgr.primary(), None);
        assert_eq!(GamepadManager::default().primary(), None);

        mgr.set_state(pad(0)); // connected = false
        mgr.set_state(pad(1));
        assert_eq!(mgr.gamepads().len(), 2, "disconnected pads still occupy slots");
        assert_eq!(mgr.primary(), None, "a disconnected slot is not a primary pad");
    }

    /// basic_access, pinned against the plausible misreading: "first" means
    /// *first connected slot in arrival order*, NOT lowest id. Pad 9 arrives
    /// before pad 1, so pad 9 is primary — a callback that assumed
    /// `min(id)` would drive the wrong controller.
    #[test]
    fn primary_is_the_first_connected_in_arrival_order_not_the_lowest_id() {
        let mut mgr = GamepadManager::new();
        mgr.set_state(pad(3)); // seen, but disconnected — must be skipped
        mgr.set_state(connected(9));
        mgr.set_state(connected(1));

        assert_eq!(mgr.primary().map(|p| p.id.id), Some(9));
        assert_eq!(mgr.gamepads().first().map(|p| p.id.id), Some(3), "arrival order kept");
    }

    /// A disconnect must hand primacy to the next connected pad, and the last
    /// disconnect must take it back to `None` — without ever dropping a slot.
    #[test]
    fn primary_follows_disconnects_while_slots_are_retained() {
        let mut mgr = GamepadManager::new();
        mgr.set_state(connected(0));
        mgr.set_state(connected(1));
        assert_eq!(mgr.primary().map(|p| p.id.id), Some(0));

        mgr.set_state(pad(0)); // unplug pad 0
        assert_eq!(mgr.primary().map(|p| p.id.id), Some(1), "primacy must fall through to pad 1");
        assert_eq!(mgr.gamepads().len(), 2, "the unplugged slot must be retained");
        assert_eq!(
            mgr.state(GamepadId { id: 0 }).map(|p| p.connected),
            Some(false),
            "the disconnect must stay observable"
        );

        mgr.set_state(pad(1)); // unplug the last one
        assert_eq!(mgr.primary(), None);
        assert_eq!(mgr.gamepads().len(), 2);

        mgr.set_state(connected(0)); // re-plug: the same slot comes back, no new one
        assert_eq!(mgr.primary().map(|p| p.id.id), Some(0));
        assert_eq!(mgr.gamepads().len(), 2, "a re-plug must reuse the id's slot");
    }

    // ------------------------------------------------------------------
    // GamepadManager::gamepads  (getter)
    // ------------------------------------------------------------------

    /// basic_access + invariant: one slot per *unique* id, in arrival order,
    /// no matter how many updates each pad delivers. A slot leak here would
    /// grow unboundedly at ~60 Hz for the lifetime of the process.
    #[test]
    fn gamepads_keeps_one_slot_per_id_in_arrival_order() {
        let mut mgr = GamepadManager::new();
        // (id, buttons) — 3 unique ids, each pushed more than once. The button
        // value differs on every push, so each one is a genuine change.
        let arrival = [(5u32, 1u32), (0, 2), (u32::MAX, 3), (5, 4), (0, 5), (5, 6)];

        for (id, buttons) in arrival {
            let mut s = connected(id);
            s.buttons = buttons;
            mgr.set_state(s);
        }

        let ids: Vec<u32> = mgr.gamepads().iter().map(|p| p.id.id).collect();
        assert_eq!(ids, alloc::vec![5, 0, u32::MAX], "arrival order / dedup by id broken");
        assert_eq!(mgr.gamepads().len(), 3);

        // 1000 further updates to a known id must not add a single slot.
        for i in 0..1000u32 {
            let mut s = connected(5);
            s.buttons = i;
            mgr.set_state(s);
        }
        assert_eq!(mgr.gamepads().len(), 3, "repeated updates leaked slots");
    }

    /// The slice `gamepads()` hands out must agree with `state()` /
    /// `primary()` — they are three views of the same `pads` vec, so a caller
    /// iterating the slice must never see something the id lookup denies.
    #[test]
    fn gamepads_slice_agrees_with_state_and_primary() {
        let mut mgr = GamepadManager::new();
        mgr.set_state(pad(2));
        mgr.set_state(connected(4));
        mgr.set_state(connected(8));

        for p in mgr.gamepads() {
            assert_eq!(mgr.state(p.id).as_ref(), Some(p), "slice and state() disagree for {p:?}");
        }
        assert_eq!(
            mgr.primary().as_ref(),
            mgr.gamepads().iter().find(|p| p.connected),
            "primary() must be the first connected element of the slice"
        );
    }

    // ------------------------------------------------------------------
    // GamepadManager::set_state  (other)
    // ------------------------------------------------------------------

    /// The core contract: `true` iff the bit pattern advanced. An idle
    /// controller re-reporting the same snapshot every frame must return
    /// `false`, or the dll would relayout at the poll rate forever.
    #[test]
    fn set_state_reports_change_only_when_the_bits_advance() {
        let mut mgr = GamepadManager::new();
        let mut s = connected(0);

        assert!(mgr.set_state(s), "a never-seen id is always a change");
        assert!(!mgr.set_state(s), "an idle controller must not look changed");
        assert!(!mgr.set_state(s), "…and must keep not looking changed");

        s.buttons = GamepadButton::South.bit();
        assert!(mgr.set_state(s), "a button press is a change");
        assert!(!mgr.set_state(s), "a held button is not a new change");

        s.connected = false;
        assert!(mgr.set_state(s), "a disconnect is a change");
        assert!(!mgr.set_state(s));

        s.left_stick_x = 0.5;
        assert!(mgr.set_state(s), "an axis move is a change");
        assert!(!mgr.set_state(s));

        assert_eq!(mgr.gamepads().len(), 1, "all of that was one pad");
    }

    /// Adversarial float #1 — NaN. A stick that reports NaN (a real gilrs /
    /// driver failure mode) would make a derived-`PartialEq` comparison say
    /// "changed" on *every* frame forever, because NaN != NaN. `set_state`
    /// compares `to_bits()`, so an unchanging NaN correctly reads as idle.
    /// This test pins that: the derived `==` disagrees, and `set_state` is right.
    #[test]
    fn set_state_treats_an_unchanging_nan_axis_as_idle() {
        let mut mgr = GamepadManager::new();
        let mut s = connected(0);
        s.left_stick_x = f32::NAN;

        // Sanity: the derived PartialEq really is non-reflexive here, so a
        // `!=`-based implementation would spin.
        let bit_identical_copy = s;
        assert_ne!(
            s, bit_identical_copy,
            "precondition: a NaN axis makes derived PartialEq non-reflexive"
        );

        assert!(mgr.set_state(s), "first sighting of the pad is a change");
        assert!(!mgr.set_state(s), "a stuck NaN axis must NOT look like a change every frame");
        assert!(!mgr.set_state(s));

        // A *different* NaN payload is a different bit pattern → a change.
        let mut other = s;
        other.left_stick_x = f32::from_bits(f32::NAN.to_bits() | 0x1);
        assert!(other.left_stick_x.is_nan());
        assert!(mgr.set_state(other), "a different NaN bit pattern is a bitwise change");
        assert!(!mgr.set_state(other));
    }

    /// Adversarial float #2 — signed zero. `-0.0 == 0.0` is *true* in IEEE, so
    /// a value-comparing implementation would miss a stick crossing centre from
    /// the negative side. `to_bits()` catches it: the flip is reported.
    #[test]
    fn set_state_reports_a_positive_to_negative_zero_flip() {
        let mut mgr = GamepadManager::new();
        let mut s = connected(0);
        s.left_stick_y = 0.0;
        assert!(mgr.set_state(s));

        s.left_stick_y = -0.0;
        // Precondition: the two zeroes compare *equal* under `==` (IEEE) yet
        // differ in their bits — which is exactly what set_state must key on.
        assert_ne!((0.0f32).to_bits(), (-0.0f32).to_bits());
        assert!(mgr.set_state(s), "a +0.0 → −0.0 sign flip is a bitwise change");
        assert!(!mgr.set_state(s));
        assert_eq!(
            mgr.state(GamepadId { id: 0 }).map(|p| p.left_stick_y.to_bits()),
            Some((-0.0f32).to_bits())
        );
    }

    /// Every field independently drives the change decision — a change in any
    /// one of `connected`, `buttons` or the six axes must be reported. A
    /// forgotten field in the comparison would silently swallow that input.
    #[test]
    fn set_state_notices_a_change_in_every_single_field() {
        // `connected`
        let mut mgr = GamepadManager::new();
        let base = connected(0);
        assert!(mgr.set_state(base));
        let mut flipped = base;
        flipped.connected = false;
        assert!(mgr.set_state(flipped), "a change in `connected` went unnoticed");

        // `buttons` — every defined bit, one at a time.
        for bit in 0..17u32 {
            let mut mgr = GamepadManager::new();
            assert!(mgr.set_state(base));
            let mut s = base;
            s.buttons = 1 << bit;
            assert!(mgr.set_state(s), "a change in button bit {bit} went unnoticed");
        }

        // the six axes.
        for (name, set) in AXIS_SETTERS {
            let mut mgr = GamepadManager::new();
            assert!(mgr.set_state(base));
            let mut s = base;
            set(&mut s, 1.0);
            assert!(mgr.set_state(s), "a change in axis `{name}` went unnoticed");
            assert!(!mgr.set_state(s), "…and re-reporting `{name}` is not a second change");
        }
    }

    /// no_panic_smoke over extremes: saturated ids / bitsets and every nasty
    /// float in every axis. Nothing panics, the slot count stays at one per id,
    /// and the stored snapshot is always exactly what was pushed.
    #[test]
    fn set_state_survives_extreme_ids_bitsets_and_floats() {
        let mut mgr = GamepadManager::new();

        for id in [0u32, u32::MAX] {
            for (_, set) in AXIS_SETTERS {
                for v in NASTY_FLOATS {
                    let mut s = connected(id);
                    s.buttons = u32::MAX;
                    set(&mut s, v);
                    mgr.set_state(s);
                    let got = mgr.state(GamepadId { id }).expect("just pushed");
                    assert!(same_bits(&got, &s), "pushing {v:?} into pad {id} did not round-trip");
                }
            }
        }
        assert_eq!(mgr.gamepads().len(), 2, "two ids must occupy exactly two slots");
    }

    /// Upserting one pad must not touch any other slot — the `find` walks by
    /// id, so an index/id mix-up would corrupt a neighbour.
    #[test]
    fn set_state_upsert_leaves_the_other_slots_untouched() {
        let mut mgr = GamepadManager::new();
        for (id, z) in [(0u32, 0.0f32), (1, 0.25), (2, 0.5), (3, 0.75)] {
            let mut s = connected(id);
            s.buttons = 1 << id;
            s.left_z = z;
            mgr.set_state(s);
        }
        let before: Vec<GamepadState> = mgr.gamepads().to_vec();

        let mut updated = connected(2);
        updated.buttons = u32::MAX;
        updated.left_z = -1.0;
        assert!(mgr.set_state(updated));

        assert_eq!(mgr.gamepads().len(), 4);
        for (i, p) in mgr.gamepads().iter().enumerate() {
            if i == 2 {
                assert!(same_bits(p, &updated), "the targeted slot was not updated");
            } else {
                assert!(same_bits(p, &before[i]), "slot {i} was corrupted by an upsert of pad 2");
            }
        }
    }

    /// The pending flag is set **only** by a real change, and is sticky until
    /// the dll drains it — several changes between two drains must coalesce
    /// into one flag (and one event), not queue up.
    #[test]
    fn set_state_raises_pending_only_on_a_real_change_and_coalesces() {
        let mut mgr = GamepadManager::new();
        assert!(!mgr.pending_event);

        let mut s = connected(0);
        assert!(mgr.set_state(s));
        assert!(mgr.pending_event, "a new pad must raise the pending flag");

        mgr.clear_pending_event();
        assert!(!mgr.set_state(s), "idle re-report");
        assert!(!mgr.pending_event, "an idle re-report must NOT raise the pending flag");

        // Three changes, one flag.
        s.buttons = 1;
        assert!(mgr.set_state(s));
        s.buttons = 2;
        assert!(mgr.set_state(s));
        s.left_z = 1.0;
        assert!(mgr.set_state(s));
        assert!(mgr.pending_event);
        assert_eq!(mgr.get_pending_events(ts(0)).len(), 1, "changes must coalesce into one event");
    }

    // ------------------------------------------------------------------
    // GamepadManager::clear_pending_event  (other)
    // ------------------------------------------------------------------

    /// no_panic_smoke: clearing is safe on an empty manager, is idempotent, and
    /// touches *only* the flag — not the pads, not the listener arming.
    #[test]
    fn clear_pending_event_is_idempotent_and_touches_nothing_else() {
        let mut mgr = GamepadManager::new();
        mgr.clear_pending_event(); // nothing pending, no pads at all
        mgr.clear_pending_event();
        assert!(!mgr.pending_event);

        mgr.set_has_listeners(true);
        mgr.set_state(connected(1));
        assert!(mgr.pending_event);

        mgr.clear_pending_event();
        assert!(!mgr.pending_event);
        mgr.clear_pending_event();
        assert!(!mgr.pending_event, "a second clear must not resurrect the flag");

        assert_eq!(mgr.gamepads().len(), 1, "clearing must not drop pad slots");
        assert!(mgr.has_listeners(), "clearing must not disarm the listener flag");
        assert!(mgr.get_pending_events(ts(1)).is_empty(), "no event after a clear");

        // …and a fresh change re-arms it, so the flag is not one-shot.
        let mut s = connected(1);
        s.buttons = 1;
        assert!(mgr.set_state(s));
        assert!(mgr.pending_event, "the flag must be re-raisable after a clear");
    }

    // ------------------------------------------------------------------
    // set_has_listeners / has_listeners  (other + predicate)
    // ------------------------------------------------------------------

    /// basic_true_false + edge_inputs: the arming flag round-trips, is
    /// idempotent in both directions, and is completely independent of the pad
    /// slots and the pending flag (the relayout walk owns it alone).
    #[test]
    fn has_listeners_roundtrips_and_is_independent_of_pad_state() {
        let mut mgr = GamepadManager::new();
        assert!(!mgr.has_listeners(), "default must be disarmed — no listeners, no polling");

        for _ in 0..3 {
            mgr.set_has_listeners(true);
            assert!(mgr.has_listeners());
        }
        for _ in 0..3 {
            mgr.set_has_listeners(false);
            assert!(!mgr.has_listeners());
        }

        // Pads arriving must not arm polling by themselves…
        mgr.set_state(connected(0));
        assert!(!mgr.has_listeners(), "a connected pad must not arm the pump on its own");
        // …and arming must not fabricate pads or events.
        let mut fresh = GamepadManager::new();
        fresh.set_has_listeners(true);
        assert!(fresh.gamepads().is_empty());
        assert!(!fresh.pending_event);
        assert!(fresh.get_pending_events(ts(0)).is_empty());
    }

    // ------------------------------------------------------------------
    // state_bitwise_eq  (private)
    // ------------------------------------------------------------------

    /// The whole reason this function exists: unlike the derived `PartialEq`,
    /// it is **reflexive over NaN**. A snapshot with a NaN in every axis must
    /// equal itself — otherwise an idle broken stick would look "changed"
    /// forever.
    #[test]
    fn state_bitwise_eq_is_reflexive_even_for_nan_axes() {
        let mut s = connected(u32::MAX);
        s.buttons = u32::MAX;
        for (_, set) in AXIS_SETTERS {
            set(&mut s, f32::NAN);
        }
        let copy = s;

        assert_ne!(s, copy, "precondition: derived PartialEq is non-reflexive over NaN");
        assert!(state_bitwise_eq(&s, &s), "bitwise eq must be reflexive");
        assert!(state_bitwise_eq(&s, &copy), "a bit-identical copy must compare equal");
        assert!(state_bitwise_eq(&copy, &s), "…symmetrically");
    }

    /// no_panic_smoke + exhaustive field coverage: flipping any ONE of the nine
    /// fields must make the comparison false, and the relation must stay
    /// symmetric. A field missing from the `&&` chain would show up here.
    #[test]
    fn state_bitwise_eq_detects_a_difference_in_every_field() {
        let base = {
            let mut s = connected(1);
            s.buttons = 0b1010;
            s.left_stick_x = 0.25;
            s.left_stick_y = -0.25;
            s.right_stick_x = 0.5;
            s.right_stick_y = -0.5;
            s.left_z = 0.75;
            s.right_z = 1.0;
            s
        };
        assert!(state_bitwise_eq(&base, &base));

        let mut mutations: Vec<(&str, GamepadState)> = Vec::new();

        let mut m = base;
        m.id = GamepadId { id: 2 };
        mutations.push(("id", m));

        let mut m = base;
        m.connected = false;
        mutations.push(("connected", m));

        let mut m = base;
        m.buttons = 0b1011;
        mutations.push(("buttons", m));

        for (name, set) in AXIS_SETTERS {
            let mut m = base;
            set(&mut m, -12.5); // a value no axis holds in `base`
            mutations.push((name, m));
        }

        assert_eq!(mutations.len(), 9, "all nine fields must be exercised");
        for (name, m) in mutations {
            assert!(!state_bitwise_eq(&base, &m), "a change in `{name}` was not detected");
            assert!(!state_bitwise_eq(&m, &base), "…and the relation must be symmetric (`{name}`)");
        }
    }

    /// The two IEEE traps in one place: `-0.0` vs `+0.0` (equal by `==`, must
    /// be *unequal* here) and NaN vs NaN with the same payload (unequal by
    /// `==`, must be *equal* here). Checked on every axis, so no arm of the
    /// comparison chain gets it right by accident.
    #[test]
    fn state_bitwise_eq_splits_signed_zero_and_joins_identical_nan() {
        for (name, set) in AXIS_SETTERS {
            let base = connected(0);

            let mut pos = base;
            set(&mut pos, 0.0);
            let mut neg = base;
            set(&mut neg, -0.0);
            assert_eq!(pos, neg, "precondition: ±0.0 compare equal via derived PartialEq");
            assert!(
                !state_bitwise_eq(&pos, &neg),
                "axis `{name}`: +0.0 and −0.0 must differ bitwise"
            );

            let mut nan_a = base;
            set(&mut nan_a, f32::NAN);
            let nan_b = nan_a;
            assert!(
                state_bitwise_eq(&nan_a, &nan_b),
                "axis `{name}`: identical NaN bit patterns must compare equal"
            );

            // Different NaN payloads are different bit patterns → not equal.
            let mut nan_c = base;
            set(&mut nan_c, f32::from_bits(f32::NAN.to_bits() | 0x7));
            assert!(
                !state_bitwise_eq(&nan_a, &nan_c),
                "axis `{name}`: distinct NaN payloads must not compare equal"
            );
        }
    }

    /// Infinities are ordinary bit patterns here — `inf == inf` must hold and
    /// `+inf != -inf`, with no arithmetic (which could produce a NaN) involved.
    #[test]
    fn state_bitwise_eq_handles_infinities_without_arithmetic() {
        for (name, set) in AXIS_SETTERS {
            let mut a = connected(0);
            set(&mut a, f32::INFINITY);
            let b = a;
            assert!(state_bitwise_eq(&a, &b), "axis `{name}`: +inf must equal +inf");

            let mut c = connected(0);
            set(&mut c, f32::NEG_INFINITY);
            assert!(!state_bitwise_eq(&a, &c), "axis `{name}`: +inf must not equal −inf");
        }
    }

    // ------------------------------------------------------------------
    // EventProvider::get_pending_events  (the manager's only output edge)
    // ------------------------------------------------------------------

    /// A pending change yields exactly one window-level `GamepadInput` event
    /// aimed at the root, carrying the timestamp it was given — and yields it
    /// *repeatedly* until the dll clears the flag (the event pass may run
    /// twice before the drain).
    #[test]
    fn pending_change_yields_one_root_gamepad_input_event() {
        let mut mgr = GamepadManager::new();
        assert!(mgr.get_pending_events(ts(0)).is_empty());

        mgr.set_state(connected(0));
        let evs = mgr.get_pending_events(ts(42));
        assert_eq!(evs.len(), 1);
        let ev = &evs[0];
        assert_eq!(ev.event_type, EventType::GamepadInput);
        assert_eq!(ev.source, CoreEventSource::User);
        assert_eq!(ev.target, DomNodeId::ROOT, "the gamepad event is window-level");
        assert_eq!(ev.timestamp, ts(42), "the caller's timestamp must be carried through");
        assert_eq!(ev.data, EventData::None, "the payload is read via CallbackInfo, not the event");

        // Still pending until it is explicitly cleared.
        assert_eq!(mgr.get_pending_events(ts(43)).len(), 1);
        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts(44)).is_empty(), "cleared → no more events");
    }

    /// The listener flag arms the *pump*, not the event stream: it must not, by
    /// itself, make the manager emit (or suppress) an event. Only a real state
    /// change does.
    #[test]
    fn listener_flag_does_not_fabricate_or_suppress_events() {
        let mut mgr = GamepadManager::new();
        mgr.set_has_listeners(true);
        assert!(mgr.get_pending_events(ts(0)).is_empty(), "arming alone must not emit an event");

        mgr.set_state(connected(0));
        mgr.set_has_listeners(false);
        assert_eq!(
            mgr.get_pending_events(ts(0)).len(),
            1,
            "disarming must not swallow an already-pending event"
        );
    }
}
