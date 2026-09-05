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
    // APPENDED at the end for ABI stability — the bitset in
    // `GamepadState::buttons` is indexed by DISCRIMINANT, so inserting any of
    // these in the middle would silently renumber every bit above it and turn
    // a saved keybinding into a different button.
    /// A miscellaneous button the vendor did not standardise: the Xbox Series
    /// share button, the `DualSense` create button, the Switch capture button.
    Misc1,
    /// Rear paddle 1 (Xbox Elite, `DualSense` Edge, Steam Deck).
    Paddle1,
    /// Rear paddle 2.
    Paddle2,
    /// Rear paddle 3.
    Paddle3,
    /// Rear paddle 4.
    Paddle4,
    /// The touchpad pressed as a button (`DualShock` 4, `DualSense`). Distinct
    /// from a touch ON the pad, which is `GamepadState::touchpad`.
    Touchpad,
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
    // Field order is by DECREASING ALIGNMENT, not by topic. `#[repr(C)]`
    // lays these out literally, so an align-1 bool between align-4 floats
    // costs 3 bytes of padding each time — the FFI checker flags it, and at
    // one GamepadState per pad per frame it is not free.
    /// Which pad this snapshot is for.
    pub id: GamepadId,
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
    /// Battery charge in `[0, 1]`, or `-1.0` when the pad does not report it.
    ///
    /// A sentinel rather than an `Option` because this struct is `#[repr(C)]`
    /// and crosses the C ABI, where a niche-optimised Option would not be
    /// stable. Wired pads report `-1.0`.
    pub battery: f32,
    /// Where a finger is on the pad's touch surface, if it has one and a
    /// finger is down (`DualShock` 4, `DualSense`, Steam Deck).
    ///
    /// `x`/`y` normalized `[0, 1]` across the surface; `active` false when
    /// nothing is touching. Not a `TouchPoint`: the pad surface is not the
    /// screen, so its coordinates are not window coordinates and must not be
    /// mistaken for them.
    ///
    /// ORIGIN IS BOTTOM-LEFT: `y` grows upward, like the thumbstick axes
    /// beside it and unlike a window's y-down coordinates. Stated because the
    /// underlying hardware disagrees with itself - a `DualShock`'s raw HID
    /// report counts y downward while the Game Controller framework normalizes
    /// it upward - so a producer needs to be told which one this field is.
    pub touchpad_x: f32,
    /// See [`GamepadState::touchpad_x`].
    pub touchpad_y: f32,
    /// The SECOND finger on the touch surface (`DualShock` 4 and `DualSense`
    /// track two), same coordinates as [`GamepadState::touchpad_x`]. Valid
    /// only while `touchpad2_active`; a pinch on the pad reads both slots.
    pub touchpad2_x: f32,
    /// See [`GamepadState::touchpad2_x`].
    pub touchpad2_y: f32,
    /// Angular velocity from the pad's own gyroscope, in **rad/s**.
    ///
    /// Present on `DualShock` 4, `DualSense`, Switch Pro and Steam Deck. This is
    /// the CONTROLLER's motion, not the device's — a phone's `SensorKind`
    /// readings describe the phone, these describe the thing in your hands,
    /// and a game that aims with gyro needs the latter.
    pub gyro_x: f32,
    /// See [`GamepadState::gyro_x`].
    pub gyro_y: f32,
    /// See [`GamepadState::gyro_x`].
    pub gyro_z: f32,
    /// Acceleration from the pad's own accelerometer, in **m/s²**.
    pub accel_x: f32,
    /// See [`GamepadState::accel_x`].
    pub accel_y: f32,
    /// See [`GamepadState::accel_x`].
    pub accel_z: f32,
    /// `false` once the pad disconnects (the manager keeps the last slot so
    /// a callback can observe the disconnect).
    pub connected: bool,
    /// Whether a finger is on the pad's touch surface.
    pub touchpad_active: bool,
    /// Whether a second finger is on the pad's touch surface
    /// (`touchpad2_x` / `touchpad2_y`).
    pub touchpad2_active: bool,
}


impl GamepadButton {
    /// This button's bit in [`GamepadState::buttons`].
    #[must_use]
    pub const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

impl Default for GamepadState {
    fn default() -> Self {
        Self {
            id: GamepadId { id: 0 },
            connected: false,
            buttons: 0,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            right_stick_x: 0.0,
            right_stick_y: 0.0,
            left_z: 0.0,
            right_z: 0.0,
            // -1.0, not 0.0: zero is a real reading meaning "flat", and a pad
            // that does not report battery must not look like a dead one.
            battery: -1.0,
            touchpad_x: 0.0,
            touchpad_y: 0.0,
            touchpad2_x: 0.0,
            touchpad2_y: 0.0,
            touchpad_active: false,
            touchpad2_active: false,
            gyro_x: 0.0,
            gyro_y: 0.0,
            gyro_z: 0.0,
            accel_x: 0.0,
            accel_y: 0.0,
            accel_z: 0.0,
        }
    }
}

impl GamepadState {
    /// An empty (disconnected) state for `id` — all buttons up, axes zero.
    #[must_use]
    pub const fn empty(id: GamepadId) -> Self {
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
            // -1.0 = "does not report", so an absent battery is not mistaken
            // for a flat one. See the field docs.
            battery: -1.0,
            touchpad_x: 0.0,
            touchpad_y: 0.0,
            touchpad2_x: 0.0,
            touchpad2_y: 0.0,
            touchpad_active: false,
            touchpad2_active: false,
            gyro_x: 0.0,
            gyro_y: 0.0,
            gyro_z: 0.0,
            accel_x: 0.0,
            accel_y: 0.0,
            accel_z: 0.0,
        }
    }

    /// Whether `button` is currently held.
    #[must_use]
    pub const fn is_pressed(&self, button: GamepadButton) -> bool {
        self.buttons & button.bit() != 0
    }

    /// The current value of `axis` (sticks `[-1, 1]`, triggers `[0, 1]`).
    #[must_use]
    pub const fn axis(&self, axis: GamepadAxis) -> f32 {
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
#[path = "gamepad_test.rs"]
mod gamepad_test;
