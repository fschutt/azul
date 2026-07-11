//! POD types for the gamepad / game-controller surface
//! (SUPER_PLAN_2 §1 feature 6 + research/03 §"Feature 6").
//!
//! Cross-platform controller input: `gilrs` on the desktop
//! (Windows / Linux / macOS), iOS `GCController` + Android `InputDevice`
//! on mobile (research/03). Defined here in `azul-core` so the manager +
//! accessors cross the FFI without `azul-layout` as a dependency; the
//! stateful side lives in `azul_layout::managers::gamepad::GamepadManager`.
//!
//! Poll model, like the sensors: the backend keeps a [`GamepadState`]
//! snapshot per connected pad current, and a callback reads the latest each
//! frame (`CallbackInfo::get_gamepad_state`) to drive movement / menus.
//! Button + axis naming follows the SDL / gilrs "standard gamepad" mapping,
//! so the face buttons are Xbox-style: South = A, East = B, West = X,
//! North = Y.

/// A connected gamepad's id — stable for the lifetime of the connection,
/// assigned by the backend on connect. (gilrs `GamepadId` / the platform
/// device id, normalised to a `u32`.)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GamepadId {
    pub id: u32,
}

/// A standard-layout gamepad button. Face buttons are Xbox-style by
/// position (South = A / Cross, East = B / Circle, West = X / Square,
/// North = Y / Triangle), so layouts stay consistent across vendors.
///
/// The discriminant order is also the bit position in
/// [`GamepadState::buttons`] — don't reorder without bumping the ABI.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GamepadButton {
    /// Bottom face button (A / Cross).
    South,
    /// Right face button (B / Circle).
    East,
    /// Top face button (Y / Triangle).
    North,
    /// Left face button (X / Square).
    West,
    /// Left shoulder button (L1 / LB).
    LeftBumper,
    /// Right shoulder button (R1 / RB).
    RightBumper,
    /// Left trigger as a digital press (L2 / LT). Analog value: `LeftZ`.
    LeftTrigger,
    /// Right trigger as a digital press (R2 / RT). Analog value: `RightZ`.
    RightTrigger,
    /// Select / Back / Share.
    Select,
    /// Start / Options / Menu.
    Start,
    /// Vendor / guide button (Xbox / PS / Home).
    Mode,
    /// Left stick click (L3).
    LeftThumb,
    /// Right stick click (R3).
    RightThumb,
    /// D-pad up.
    DPadUp,
    /// D-pad down.
    DPadDown,
    /// D-pad left.
    DPadLeft,
    /// D-pad right.
    DPadRight,
}

/// A gamepad analog axis. Stick axes are in `[-1, 1]` (right / up positive);
/// trigger axes ([`GamepadAxis::LeftZ`] / [`GamepadAxis::RightZ`]) in
/// `[0, 1]`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GamepadAxis {
    /// Left stick horizontal (left −1 … right +1).
    LeftStickX,
    /// Left stick vertical (down −1 … up +1).
    LeftStickY,
    /// Right stick horizontal.
    RightStickX,
    /// Right stick vertical.
    RightStickY,
    /// Left trigger pressure (0 … 1).
    LeftZ,
    /// Right trigger pressure (0 … 1).
    RightZ,
}

/// Snapshot of one gamepad's state. Buttons are a bitset (bit `n` = the
/// [`GamepadButton`] with discriminant `n`); axes are explicit fields. All
/// POD / `Copy`, so it crosses the FFI by value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GamepadState {
    /// Which pad this snapshot is for.
    pub id: GamepadId,
    /// `false` once the pad disconnects (the manager keeps the last slot so
    /// a callback can observe the disconnect).
    pub connected: bool,
    /// Pressed-button bitset — bit `n` set ⇔ the `GamepadButton` with
    /// discriminant `n` is held. Read via [`GamepadState::is_pressed`].
    pub buttons: u32,
    /// Left stick X in `[-1, 1]`.
    pub left_stick_x: f32,
    /// Left stick Y in `[-1, 1]`.
    pub left_stick_y: f32,
    /// Right stick X in `[-1, 1]`.
    pub right_stick_x: f32,
    /// Right stick Y in `[-1, 1]`.
    pub right_stick_y: f32,
    /// Left trigger pressure in `[0, 1]`.
    pub left_z: f32,
    /// Right trigger pressure in `[0, 1]`.
    pub right_z: f32,
}

impl GamepadButton {
    /// This button's bit in [`GamepadState::buttons`].
    #[must_use] pub const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

impl GamepadState {
    /// An empty (disconnected) state for `id` — all buttons up, axes zero.
    #[must_use] pub const fn empty(id: GamepadId) -> Self {
        Self {
            id,
            connected: false,
            buttons: 0,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            right_stick_x: 0.0,
            right_stick_y: 0.0,
            left_z: 0.0,
            right_z: 0.0,
        }
    }

    /// Whether `button` is currently held.
    #[must_use] pub const fn is_pressed(&self, button: GamepadButton) -> bool {
        self.buttons & button.bit() != 0
    }

    /// The current value of `axis` (sticks `[-1, 1]`, triggers `[0, 1]`).
    #[must_use] pub const fn axis(&self, axis: GamepadAxis) -> f32 {
        match axis {
            GamepadAxis::LeftStickX => self.left_stick_x,
            GamepadAxis::LeftStickY => self.left_stick_y,
            GamepadAxis::RightStickX => self.right_stick_x,
            GamepadAxis::RightStickY => self.right_stick_y,
            GamepadAxis::LeftZ => self.left_z,
            GamepadAxis::RightZ => self.right_z,
        }
    }
}

// FFI Option wrapper for `CallbackInfo::get_gamepad_state(id) ->
// Option<GamepadState>` (mirrors `OptionSensorReading`).
impl_option!(
    GamepadState,
    OptionGamepadState,
    [Debug, Clone, Copy, PartialEq]
);

#[cfg(test)]
mod autotest_generated {
    use super::*;

    /// Every `GamepadButton`, in discriminant order. The order here is also
    /// the asserted bit order — see `bit_matches_documented_abi`.
    const ALL_BUTTONS: [GamepadButton; 17] = [
        GamepadButton::South,
        GamepadButton::East,
        GamepadButton::North,
        GamepadButton::West,
        GamepadButton::LeftBumper,
        GamepadButton::RightBumper,
        GamepadButton::LeftTrigger,
        GamepadButton::RightTrigger,
        GamepadButton::Select,
        GamepadButton::Start,
        GamepadButton::Mode,
        GamepadButton::LeftThumb,
        GamepadButton::RightThumb,
        GamepadButton::DPadUp,
        GamepadButton::DPadDown,
        GamepadButton::DPadLeft,
        GamepadButton::DPadRight,
    ];

    const ALL_AXES: [GamepadAxis; 6] = [
        GamepadAxis::LeftStickX,
        GamepadAxis::LeftStickY,
        GamepadAxis::RightStickX,
        GamepadAxis::RightStickY,
        GamepadAxis::LeftZ,
        GamepadAxis::RightZ,
    ];

    /// Bitset of every defined button — bits 0..=16.
    const ALL_BUTTONS_MASK: u32 = 0x0001_FFFF;

    /// Writes `v` into the field that `GamepadState::axis` reads for `axis`.
    /// Deliberately mirrors `axis()`; a mis-mapping here would still be caught
    /// by `axis_reads_each_field_uniquely`, which pokes the fields directly.
    fn set_axis(s: &mut GamepadState, axis: GamepadAxis, v: f32) {
        match axis {
            GamepadAxis::LeftStickX => s.left_stick_x = v,
            GamepadAxis::LeftStickY => s.left_stick_y = v,
            GamepadAxis::RightStickX => s.right_stick_x = v,
            GamepadAxis::RightStickY => s.right_stick_y = v,
            GamepadAxis::LeftZ => s.left_z = v,
            GamepadAxis::RightZ => s.right_z = v,
        }
    }

    // ------------------------------------------------------------------
    // GamepadButton::bit  (other)
    // ------------------------------------------------------------------

    /// no_panic_smoke + shift-overflow guard: `bit()` is `1u32 << (self as
    /// u32)`, which is UB / a panic in debug the moment a discriminant reaches
    /// 32. Pin the discriminants to a contiguous 0..17 so adding an 18th..32nd
    /// button stays safe and a 33rd fails HERE rather than at a user's shift.
    #[test]
    fn bit_discriminants_are_contiguous_and_shift_safe() {
        for (i, b) in ALL_BUTTONS.iter().enumerate() {
            let d = *b as u32;
            assert_eq!(
                d, i as u32,
                "{b:?} has discriminant {d}, expected {i} — the bitset in \
                 GamepadState::buttons assumes contiguous discriminants"
            );
            assert!(
                d < 32,
                "{b:?} discriminant {d} would overflow `1u32 << d` in bit()"
            );
        }
    }

    /// The ABI the doc comment promises ("the discriminant order is also the
    /// bit position … don't reorder without bumping the ABI"). Hard-coded so a
    /// reorder is a loud test failure, not a silent remap of every FFI client's
    /// button bits.
    #[test]
    fn bit_matches_documented_abi() {
        assert_eq!(GamepadButton::South.bit(), 1 << 0);
        assert_eq!(GamepadButton::East.bit(), 1 << 1);
        assert_eq!(GamepadButton::North.bit(), 1 << 2);
        assert_eq!(GamepadButton::West.bit(), 1 << 3);
        assert_eq!(GamepadButton::LeftBumper.bit(), 1 << 4);
        assert_eq!(GamepadButton::RightBumper.bit(), 1 << 5);
        assert_eq!(GamepadButton::LeftTrigger.bit(), 1 << 6);
        assert_eq!(GamepadButton::RightTrigger.bit(), 1 << 7);
        assert_eq!(GamepadButton::Select.bit(), 1 << 8);
        assert_eq!(GamepadButton::Start.bit(), 1 << 9);
        assert_eq!(GamepadButton::Mode.bit(), 1 << 10);
        assert_eq!(GamepadButton::LeftThumb.bit(), 1 << 11);
        assert_eq!(GamepadButton::RightThumb.bit(), 1 << 12);
        assert_eq!(GamepadButton::DPadUp.bit(), 1 << 13);
        assert_eq!(GamepadButton::DPadDown.bit(), 1 << 14);
        assert_eq!(GamepadButton::DPadLeft.bit(), 1 << 15);
        assert_eq!(GamepadButton::DPadRight.bit(), 1 << 16);
    }

    /// invariant: each bit is a distinct, non-zero power of two. Two buttons
    /// sharing a bit would make `is_pressed` report a phantom press.
    #[test]
    fn bit_is_a_distinct_power_of_two() {
        let mut seen = 0u32;
        for b in ALL_BUTTONS {
            let bit = b.bit();
            assert_ne!(bit, 0, "{b:?} maps to bit 0");
            assert_eq!(bit.count_ones(), 1, "{b:?} bit {bit:#x} is not a single bit");
            assert_eq!(seen & bit, 0, "{b:?} bit {bit:#x} collides with an earlier button");
            seen |= bit;
        }
        assert_eq!(seen, ALL_BUTTONS_MASK);
        assert_eq!(seen.count_ones(), ALL_BUTTONS.len() as u32);
    }

    /// `bit()` is `const fn` — usable in a `const` item / array length. A
    /// non-const-evaluable body (or an overflowing shift, which is a hard
    /// compile error in const context) fails to build.
    #[test]
    fn bit_is_const_evaluable() {
        const SOUTH: u32 = GamepadButton::South.bit();
        const DPAD_RIGHT: u32 = GamepadButton::DPadRight.bit();
        assert_eq!(SOUTH, 1);
        assert_eq!(DPAD_RIGHT, 65_536);
    }

    // ------------------------------------------------------------------
    // GamepadState::empty  (constructor)
    // ------------------------------------------------------------------

    /// no_panic + invariants_hold: extreme ids (0, 1, MAX/2, MAX) round-trip
    /// into the state unchanged and every other field is the documented zero.
    #[test]
    fn empty_preserves_id_and_zeroes_everything_else() {
        for raw in [0, 1, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            let s = GamepadState::empty(GamepadId { id: raw });
            assert_eq!(s.id, GamepadId { id: raw });
            assert_eq!(s.id.id, raw);
            assert!(!s.connected, "empty() must start disconnected");
            assert_eq!(s.buttons, 0);
        }
    }

    /// default_is_neutral: an empty state is the neutral element — no button
    /// reads as pressed and every axis is exactly *positive* zero. The
    /// `to_bits()` check is the point: a `-0.0` would still compare `== 0.0`
    /// yet flips the sign of anything a caller multiplies by it.
    #[test]
    fn empty_is_neutral_for_every_button_and_axis() {
        let s = GamepadState::empty(GamepadId { id: 42 });
        for b in ALL_BUTTONS {
            assert!(!s.is_pressed(b), "{b:?} reads as pressed in an empty state");
        }
        for a in ALL_AXES {
            assert_eq!(
                s.axis(a).to_bits(),
                0.0f32.to_bits(),
                "axis {a:?} of an empty state is not +0.0 (got {})",
                s.axis(a)
            );
        }
    }

    /// invariant: `empty()` is a pure function of `id` — same id gives an
    /// equal state, a different id gives an unequal one (so a stale slot for
    /// pad 0 can't be mistaken for pad 1's).
    #[test]
    fn empty_is_deterministic_and_id_discriminating() {
        let a = GamepadState::empty(GamepadId { id: 7 });
        let b = GamepadState::empty(GamepadId { id: 7 });
        let c = GamepadState::empty(GamepadId { id: 8 });
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    /// `empty()` is `const fn`, so a backend can build a static slot table.
    #[test]
    fn empty_is_const_evaluable() {
        const S: GamepadState = GamepadState::empty(GamepadId { id: u32::MAX });
        assert_eq!(S.id.id, u32::MAX);
        assert_eq!(S.buttons, 0);
        assert!(!S.connected);
    }

    // ------------------------------------------------------------------
    // GamepadState::is_pressed  (predicate)
    // ------------------------------------------------------------------

    /// basic_true_false + isolation: with exactly one bit set, that button —
    /// and *only* that button — reads as pressed. Catches an off-by-one shift
    /// or a bit collision that a single known-true case would miss.
    #[test]
    fn is_pressed_isolates_each_single_bit() {
        for pressed in ALL_BUTTONS {
            let mut s = GamepadState::empty(GamepadId { id: 0 });
            s.buttons = pressed.bit();
            for other in ALL_BUTTONS {
                assert_eq!(
                    s.is_pressed(other),
                    other == pressed,
                    "buttons={:#x}: is_pressed({other:?}) disagrees with the only \
                     pressed button {pressed:?}",
                    s.buttons
                );
            }
        }
    }

    /// edge_inputs: the two saturating bitsets. `0` = nothing pressed,
    /// `u32::MAX` = everything pressed. Both are deterministic, neither panics.
    #[test]
    fn is_pressed_handles_empty_and_full_bitsets() {
        let mut s = GamepadState::empty(GamepadId { id: 0 });

        s.buttons = 0;
        for b in ALL_BUTTONS {
            assert!(!s.is_pressed(b), "{b:?} pressed with buttons == 0");
        }

        s.buttons = u32::MAX;
        for b in ALL_BUTTONS {
            assert!(s.is_pressed(b), "{b:?} not pressed with buttons == u32::MAX");
        }
    }

    /// Adversarial: a backend (or a hostile FFI caller) writes junk into the
    /// 15 *reserved* high bits, 17..=31. No defined button may light up — the
    /// mask is per-button, so garbage outside the defined range must be inert.
    #[test]
    fn is_pressed_ignores_reserved_high_bits() {
        let mut s = GamepadState::empty(GamepadId { id: 0 });
        for junk in [
            !ALL_BUTTONS_MASK,        // every reserved bit
            1 << 17,                  // the first reserved bit
            1 << 31,                  // the sign bit
            0xDEAD_0000 & !ALL_BUTTONS_MASK,
        ] {
            assert_eq!(junk & ALL_BUTTONS_MASK, 0, "test vector {junk:#x} is not reserved-only");
            s.buttons = junk;
            for b in ALL_BUTTONS {
                assert!(
                    !s.is_pressed(b),
                    "reserved-bit junk {junk:#x} made {b:?} read as pressed"
                );
            }
        }
    }

    /// round-trip: encode a button set into the bitset, decode it back through
    /// `is_pressed` — the decoded set must equal the encoded one, and
    /// re-encoding must reproduce the exact same bits (encode == decode).
    #[test]
    fn is_pressed_bitset_roundtrips() {
        let subsets: [&[GamepadButton]; 5] = [
            &[],
            &[GamepadButton::South],
            &[GamepadButton::DPadRight],
            &[
                GamepadButton::South,
                GamepadButton::DPadRight,
                GamepadButton::Mode,
            ],
            &ALL_BUTTONS,
        ];

        for subset in subsets {
            let encoded = subset.iter().fold(0u32, |acc, b| acc | b.bit());

            let mut s = GamepadState::empty(GamepadId { id: 3 });
            s.buttons = encoded;

            // Decode through the predicate, then re-encode from what we decoded.
            let mut re_encoded = 0u32;
            for (i, b) in ALL_BUTTONS.iter().enumerate() {
                let decoded = s.is_pressed(*b);
                assert_eq!(
                    decoded,
                    subset.contains(b),
                    "decode of {encoded:#x}: is_pressed({b:?}) disagrees with the \
                     encoded subset (bit {i})"
                );
                if decoded {
                    re_encoded |= b.bit();
                }
            }
            assert_eq!(re_encoded, encoded, "re-encode of {encoded:#x} is not bit-stable");
        }
    }

    /// `is_pressed` is `const fn` and takes `&self` — usable on a const state.
    #[test]
    fn is_pressed_is_const_evaluable() {
        const S: GamepadState = GamepadState {
            id: GamepadId { id: 0 },
            connected: true,
            buttons: 0b101, // South (bit 0) | North (bit 2)
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            right_stick_x: 0.0,
            right_stick_y: 0.0,
            left_z: 0.0,
            right_z: 0.0,
        };
        const SOUTH: bool = S.is_pressed(GamepadButton::South);
        const EAST: bool = S.is_pressed(GamepadButton::East);
        const NORTH: bool = S.is_pressed(GamepadButton::North);
        assert!(SOUTH);
        assert!(!EAST);
        assert!(NORTH);
    }

    /// invariant: `is_pressed` is a read-only view — polling every button
    /// leaves the snapshot byte-identical.
    #[test]
    fn is_pressed_does_not_mutate_the_snapshot() {
        let mut s = GamepadState::empty(GamepadId { id: 9 });
        s.buttons = 0x1_0F0F & ALL_BUTTONS_MASK;
        let before = s;
        for b in ALL_BUTTONS {
            let _ = s.is_pressed(b);
        }
        assert_eq!(s, before);
    }

    // ------------------------------------------------------------------
    // GamepadState::axis  (other)
    // ------------------------------------------------------------------

    /// The copy-paste trap: `axis()` is a six-arm match over six near-identical
    /// fields. Give every field a unique sentinel and poke the fields directly
    /// (never through a helper that could share the same bug), so any two arms
    /// reading the same field — or reading each other's — fails here.
    #[test]
    fn axis_reads_each_field_uniquely() {
        let s = GamepadState {
            id: GamepadId { id: 1 },
            connected: true,
            buttons: 0,
            left_stick_x: 1.0,
            left_stick_y: 2.0,
            right_stick_x: 3.0,
            right_stick_y: 4.0,
            left_z: 5.0,
            right_z: 6.0,
        };
        assert_eq!(s.axis(GamepadAxis::LeftStickX).to_bits(), 1.0f32.to_bits());
        assert_eq!(s.axis(GamepadAxis::LeftStickY).to_bits(), 2.0f32.to_bits());
        assert_eq!(s.axis(GamepadAxis::RightStickX).to_bits(), 3.0f32.to_bits());
        assert_eq!(s.axis(GamepadAxis::RightStickY).to_bits(), 4.0f32.to_bits());
        assert_eq!(s.axis(GamepadAxis::LeftZ).to_bits(), 5.0f32.to_bits());
        assert_eq!(s.axis(GamepadAxis::RightZ).to_bits(), 6.0f32.to_bits());

        // …and every axis is pairwise distinct, so no two arms alias one field.
        for (i, a) in ALL_AXES.iter().enumerate() {
            for b in ALL_AXES.iter().skip(i + 1) {
                assert_ne!(
                    s.axis(*a).to_bits(),
                    s.axis(*b).to_bits(),
                    "axes {a:?} and {b:?} read the same field"
                );
            }
        }
    }

    /// no_panic_smoke over the nasty floats: NaN (incl. a signalling payload),
    /// ±inf, ±0.0, subnormals, MIN/MAX. `axis()` is a getter, so it must hand
    /// each one back *bit-for-bit* — no clamping, no NaN canonicalisation,
    /// no sign-of-zero loss (a `-0.0` silently flipping to `+0.0` would change
    /// the direction of a caller's `v.signum()` deadzone check).
    #[test]
    fn axis_returns_extreme_floats_bit_exact() {
        let nasty: [f32; 12] = [
            f32::NAN,
            -f32::NAN,
            f32::from_bits(0x7F80_0001), // signalling NaN payload
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            -0.0,
            f32::MIN_POSITIVE,
            f32::from_bits(1), // smallest subnormal
            -1.0,
            f32::MIN,
            f32::MAX,
        ];

        for a in ALL_AXES {
            for v in nasty {
                let mut s = GamepadState::empty(GamepadId { id: 0 });
                set_axis(&mut s, a, v);
                assert_eq!(
                    s.axis(a).to_bits(),
                    v.to_bits(),
                    "axis {a:?} did not return {v:?} bit-exactly (bits {:#x} vs {:#x})",
                    s.axis(a).to_bits(),
                    v.to_bits()
                );
            }
            // Writing one axis must not disturb the other five.
            let mut s = GamepadState::empty(GamepadId { id: 0 });
            set_axis(&mut s, a, f32::MAX);
            for other in ALL_AXES.iter().filter(|o| **o != a) {
                assert_eq!(s.axis(*other).to_bits(), 0.0f32.to_bits());
            }
        }
    }

    /// Boundary + out-of-range: the doc gives sticks `[-1, 1]` and triggers
    /// `[0, 1]`, but `axis()` is a plain accessor — range enforcement is the
    /// *backend's* contract, not this getter's. Pin the pass-through so nobody
    /// "helpfully" adds a silent clamp that would hide a mis-scaling backend.
    #[test]
    fn axis_does_not_clamp_out_of_range_values() {
        for a in ALL_AXES {
            for v in [-1.0f32, 0.0, 1.0, 1.000_001, -1.000_001, 1e9, -1e9] {
                let mut s = GamepadState::empty(GamepadId { id: 0 });
                set_axis(&mut s, a, v);
                assert_eq!(
                    s.axis(a).to_bits(),
                    v.to_bits(),
                    "axis {a:?} clamped or rounded {v}"
                );
            }
        }
    }

    /// `axis()` is `const fn`.
    #[test]
    fn axis_is_const_evaluable() {
        const S: GamepadState = GamepadState {
            id: GamepadId { id: 0 },
            connected: true,
            buttons: 0,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            right_stick_x: 0.0,
            right_stick_y: 0.0,
            left_z: 1.0,
            right_z: 0.0,
        };
        const LEFT_Z: f32 = S.axis(GamepadAxis::LeftZ);
        const RIGHT_Z: f32 = S.axis(GamepadAxis::RightZ);
        assert_eq!(LEFT_Z.to_bits(), 1.0f32.to_bits());
        assert_eq!(RIGHT_Z.to_bits(), 0.0f32.to_bits());
    }

    // ------------------------------------------------------------------
    // GamepadState / OptionGamepadState — derived-impl invariants
    // ------------------------------------------------------------------

    /// FFI trap, asserted rather than fixed: `GamepadState` derives
    /// `PartialEq` over `f32`, so IEEE-754 applies — a snapshot with a NaN axis
    /// is *not equal to itself*. Callers that dedupe frames with `==` (e.g.
    /// "skip the callback if the state didn't change") will therefore always
    /// see a change once a backend reports a NaN axis. Reflexivity holds for
    /// every non-NaN state.
    #[test]
    fn state_equality_is_ieee_not_reflexive_over_nan() {
        let mut nan_state = GamepadState::empty(GamepadId { id: 0 });
        nan_state.left_stick_x = f32::NAN;
        let a = nan_state;
        let b = nan_state; // a bit-identical copy
        assert_ne!(a, b, "NaN axis: derived PartialEq is expected to be non-reflexive");

        let mut sane = GamepadState::empty(GamepadId { id: 0 });
        sane.left_stick_x = 0.5;
        let c = sane;
        let d = sane;
        assert_eq!(c, d);
    }

    /// round-trip: `GamepadState` -> `Option` -> `OptionGamepadState` -> back.
    /// This is the wrapper `CallbackInfo::get_gamepad_state` returns across the
    /// FFI, so encode == decode must hold in both directions, and the default
    /// must be the "no such pad" case.
    #[test]
    fn option_gamepad_state_roundtrips() {
        assert!(OptionGamepadState::default().is_none());
        assert!(!OptionGamepadState::default().is_some());
        assert_eq!(Option::<GamepadState>::from(OptionGamepadState::default()), None);

        let mut s = GamepadState::empty(GamepadId { id: u32::MAX });
        s.connected = true;
        s.buttons = ALL_BUTTONS_MASK;
        s.right_stick_y = -1.0;
        s.left_z = 1.0;

        let wrapped: OptionGamepadState = Some(s).into();
        assert!(wrapped.is_some());
        assert!(!wrapped.is_none());
        assert_eq!(wrapped.as_option(), Some(&s));
        assert_eq!(Option::<GamepadState>::from(wrapped), Some(s));

        let none: OptionGamepadState = Option::<GamepadState>::None.into();
        assert!(none.is_none());

        // `replace` returns the PREVIOUS value (mem::replace semantics).
        let mut slot = OptionGamepadState::None;
        let prev = slot.replace(s);
        assert!(prev.is_none());
        assert_eq!(slot.as_option(), Some(&s));

        let other = GamepadState::empty(GamepadId { id: 1 });
        let prev = slot.replace(other);
        assert_eq!(Option::<GamepadState>::from(prev), Some(s));
        assert_eq!(slot.as_option(), Some(&other));
    }
}
