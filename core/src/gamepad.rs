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
    pub fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

impl GamepadState {
    /// An empty (disconnected) state for `id` — all buttons up, axes zero.
    pub fn empty(id: GamepadId) -> Self {
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
    pub fn is_pressed(&self, button: GamepadButton) -> bool {
        self.buttons & button.bit() != 0
    }

    /// The current value of `axis` (sticks `[-1, 1]`, triggers `[0, 1]`).
    pub fn axis(&self, axis: GamepadAxis) -> f32 {
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
