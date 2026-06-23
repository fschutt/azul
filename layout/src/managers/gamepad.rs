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
    fn states_round_trip_through_the_channel() {
        let _ = drain_gamepad_states();
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
