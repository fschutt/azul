# Research 03 — Hardware Sensors, Gamepads, Wacom/Stylus extensions

> SUPER_PLAN_2 §1 features **5** (accelerometer + gyro + magnetometer + orientation/proximity),
> **6** (gamepad input), and **7** (drawing-tablet beyond `PenState`) — all 5 platforms
> (iOS, Android, macOS, Linux, Windows). Plus a unified Azul-side integration sketch
> that lines up with the existing manager / EventFilter / CallbackInfo pattern
> documented in `SUPER_PLAN_2.md` §0.
>
> Existing surface already wired:
> - `PenState { position, pressure, tilt, in_contact, is_eraser, barrel_button_pressed, device_id }` (`layout/src/managers/gesture.rs:360`)
> - `PenDown / PenMove / PenUp / PenEnter / PenLeave` (`core/src/events.rs:1562-1570`)
> - `get_pen_state / get_pen_pressure / get_pen_tilt / is_pen_in_contact / is_pen_eraser / is_pen_barrel_button_pressed` (`layout/src/callbacks.rs:3115-3153`)
> - `inject_native_gesture` injection seam in iOS/Android backends; the same pattern is reused below for sensor / gamepad / stylus-extension events.
>
> Status of platform `PenState` field plumbing: `is_eraser` and `barrel_button_pressed` are declared on the struct but *no* backend currently sets them. This brief lists the platform calls that should populate them.

---

## Feature 5 — Accelerometer, Gyroscope, Magnetometer (+ Orientation, Proximity)

### Conceptual model

Three uncalibrated sensors plus three derived/fused values are what apps actually want:

| Sensor | Unit (Azul-normalized) | Notes |
|---|---|---|
| Accelerometer | m/s² (gravity *included* by default; "linear acceleration" excludes gravity) | iOS/Android both expose both raw & linear; macOS sudden-motion is gravity-only. |
| Gyroscope | rad/s | iOS reports rad/s natively; Android `Sensor.TYPE_GYROSCOPE` returns rad/s; Windows reports deg/s and needs conversion. |
| Magnetometer | µT (microtesla) | iOS `magneticField`, Android `Sensor.TYPE_MAGNETIC_FIELD`, Windows `Magnetometer`. |
| **Device-motion (fused)** | quaternion + Euler (yaw/pitch/roll) | iOS `CMDeviceMotion.attitude`; Android `Sensor.TYPE_ROTATION_VECTOR`; Windows `OrientationSensor`. The recommended path for "which way is the device pointing." |
| **Orientation (UI)** | enum { Portrait, LandscapeLeft, LandscapeRight, PortraitUpsideDown } | UI rotation lock — separate from device-motion. |
| **Proximity** | bool (near / far) | iOS `UIDevice.proximityState`; Android `Sensor.TYPE_PROXIMITY`. Phone-only. |

> **Coordinate-frame warning.** Apple (iOS *and* macOS): right-handed, +X right, +Y up, +Z out of screen toward user. Android: same axes when device is in default portrait orientation (+X right, +Y up, +Z out toward user). Windows `Accelerometer`: per the docs the reading is in "Earth's gravity," with +X right, +Y up, +Z *out of screen*. **All three converge** on the same convention in their respective default orientations, but each platform rotates with the UI orientation differently — Android applies `Display.getRotation()` automatically only on `TYPE_ROTATION_VECTOR`-type sensors; iOS keeps the device frame regardless of `UIInterfaceOrientation`. The Azul-side manager must expose both *raw device frame* and *rotated UI frame* and document the difference. `TODO: verify` what Windows does on tablet rotation.

---

### iOS — `CoreMotion` (`CMMotionManager`)

* **Framework:** `CoreMotion.framework`, available since iOS 4.
* **Entry point:** singleton-per-app pattern — `let mgr = CMMotionManager(); mgr.deviceMotionUpdateInterval = 1.0/60; mgr.startDeviceMotionUpdates(to: queue) { motion, err in ... }`. Apple's docs explicitly say *one `CMMotionManager` per app*; multiple instances "can affect the rate at which an app receives data from the accelerometer and gyroscope."
* **Streams:**
  * `startAccelerometerUpdates(to:withHandler:)` → `CMAccelerometerData` (gravity included, units = *g*; multiply by ~9.80665 for m/s²).
  * `startGyroUpdates` → `CMGyroData` in **rad/s**.
  * `startMagnetometerUpdates` → `CMMagnetometerData` in **µT** (uncalibrated, geomagnetic + device bias).
  * `startDeviceMotionUpdates(using: .xMagneticNorthZVertical)` — fused: gravity, userAcceleration (= acceleration − gravity), rotationRate (calibrated gyro), magneticField (calibrated, with `CMCalibratedMagneticField.accuracy`), attitude (`CMAttitude` — quaternion / rotation matrix / Euler). **Recommended path** because the fusion compensates for bias.
* **Sample rate config:** `accelerometerUpdateInterval` / `gyroUpdateInterval` / `magnetometerUpdateInterval` / `deviceMotionUpdateInterval`, all in **seconds**. Hardware max is typically 100 Hz (iPhone) — 1.0/100. Setting < 1/100 silently caps.
* **Permission:** none for accelerometer/gyro. iOS 17+ does ask for "motion & fitness" if you use `CMPedometer` or step-detection. Adding `NSMotionUsageDescription` to `Info.plist` is required if you call `CMSensorRecorder` or `CMPedometer`. **For raw CMMotionManager streams, no Info.plist key is required** on current iOS. `TODO: verify` for iOS 17/18 — historical behavior may have changed.
* **Proximity:** `UIDevice.current.isProximityMonitoringEnabled = true`; then read `UIDevice.current.proximityState` (bool) or observe `UIDeviceProximityStateDidChangeNotification`.
* **UI orientation:** `UIDevice.current.orientation` (`UIDeviceOrientation` enum: portrait, landscapeLeft, ...) — distinct from device-motion attitude.

### Android — `SensorManager`

* **Entry point:** `val sm = ctx.getSystemService(Context.SENSOR_SERVICE) as SensorManager; val accel = sm.getDefaultSensor(Sensor.TYPE_ACCELEROMETER); sm.registerListener(listener, accel, SensorManager.SENSOR_DELAY_GAME)`.
* **Sensor types we care about:**
  * `TYPE_ACCELEROMETER` (raw, gravity included; m/s²)
  * `TYPE_LINEAR_ACCELERATION` (gravity removed; m/s²)
  * `TYPE_GRAVITY` (only gravity; m/s²)
  * `TYPE_GYROSCOPE` (rad/s, uncalibrated)
  * `TYPE_GYROSCOPE_UNCALIBRATED` (rad/s + bias estimate)
  * `TYPE_MAGNETIC_FIELD` (µT)
  * `TYPE_MAGNETIC_FIELD_UNCALIBRATED` (µT + hard-iron bias)
  * `TYPE_ROTATION_VECTOR` (fused; quaternion as `values[0..3]`, optional accuracy in `values[4]`)
  * `TYPE_GAME_ROTATION_VECTOR` (gyro+accel only, no magnetometer — drift-prone but immune to magnetic disturbance)
  * `TYPE_PROXIMITY` (cm; some sensors report only binary near/far via `sensor.maximumRange`)
  * `TYPE_DEVICE_ORIENTATION` (Android 12+; replacement for the deprecated `OrientationEventListener`).
* **Sample rate:** `SENSOR_DELAY_FASTEST` (0 µs hint), `SENSOR_DELAY_GAME` (20 ms = 50 Hz), `SENSOR_DELAY_UI` (60 ms), `SENSOR_DELAY_NORMAL` (200 ms). Or specify in µs directly via the `samplingPeriodUs` overload. Real rate is clamped to the sensor's `getMinDelay()`.
* **Coordinate frame:** device frame — +X right, +Y up, +Z out — when the device is in its default orientation. `SensorManager.remapCoordinateSystem(...)` converts to a world frame from a rotation matrix derived via `SensorManager.getRotationMatrix(R, I, gravity, geomagnetic)`.
* **Permission:** none for accelerometer/gyro/magnetometer. Android 12+ rate-limits motion sensors to 200 Hz unless app declares `<uses-permission android:name="android.permission.HIGH_SAMPLING_RATE_SENSORS"/>`.
* **JNI surface for our backend:** `dll/src/desktop/shell2/android/mod.rs` already has a JNI bridge; new Java glue class `AzulSensorBridge` registers itself as a `SensorEventListener`, calls back into Rust via `Java_*_nativeOnSensorEvent(sensorType, jfloatArray values, int accuracy, long timestamp)`.

### macOS — `IOHIDManager` (laptops with SMS / built-in motion)

* **Native API:** `IOHIDManagerCreate` + `IOHIDManagerSetDeviceMatchingMultiple` matching by usage page `kHIDPage_Sensor` (0x20) — see `IOKit/hid/IOHIDUsageTables.h`. Apple Silicon laptops (MacBook Pro 14"/16" 2021+) and Apple Studio Display expose accelerometer + gyro + ambient-light through this path.
* **Sudden Motion Sensor (Intel-only laptops, 2005-2019):** SMC key `MOTN` returns 3 bytes (X/Y/Z, ±127). Crate `motion-sensor` (third-party, unmaintained — `TODO: verify`) wraps it. Apple Silicon dropped SMS but exposes IOHID-style sensors as above.
* **GameController.framework on macOS (Big Sur+):** when paired with an MFi controller, the controller's motion service surfaces as `GCMotion` on `GCController.motion`. Standalone Mac built-in motion is *not* exposed through GameController — only IOHID/SMC.
* **`CoreMotion` is iOS-only.** Apple has never shipped `CMMotionManager` on macOS.
* **Permission:** none.
* **Reality check:** very few Macs expose useful motion sensors today. Treat as `Unsupported` on Intel desktops; advertise on laptops that report a matching IOHID device; advertise via `GCMotion` when a controller with gyro is paired. Document this clearly so apps gracefully degrade.

### Linux — Industrial I/O (`iio`) sysfs

* **Discovery:** `/sys/bus/iio/devices/iio:device*/`. Each device exposes `name` (e.g., `bmi160`, `lis3lv02d`), and per-channel files like `in_accel_x_raw`, `in_accel_x_scale`, `in_anglvel_x_raw` (gyro), `in_magn_x_raw`.
* **Reading:** raw integer × scale = physical value (accelerometer in m/s² when `_scale` is appropriately set; gyro in rad/s; magnetometer in gauss → ×100 for µT, sometimes already in µT depending on driver).
* **Buffered mode (preferred for high rate):** `iio:device*/buffer/enable=1` plus configured trigger; read packed samples from `/dev/iio:deviceN`.
* **Crate:** `industrial-io` (https://github.com/fpagliughi/rust-industrial-io) wraps libiio; alternative is to read sysfs directly with `std::fs`. For tablets / laptops with motion (ThinkPads, Surface-Pro-on-Linux), `iio-sensor-proxy` D-Bus service from freedesktop provides a higher-level API used by GNOME's auto-rotate.
* **Permission:** typically `udev` rule grants `seat`/`input` group access; or app talks to `iio-sensor-proxy` on `net.hadess.SensorProxy` D-Bus interface (no special privilege needed).
* **Sample rate:** `in_accel_sampling_frequency` (Hz) — write the desired rate, the driver picks the nearest supported value.

### Windows — `Windows.Devices.Sensors`

* **Namespace:** `Windows.Devices.Sensors.*` (WinRT — usable from Rust via `windows-rs` (`windows` crate, `Windows_Devices_Sensors` feature)). Available since Windows 8/RT.
* **Classes:**
  * `Accelerometer.GetDefault()` → `Accelerometer`; events `ReadingChanged`, `Shaken`. Reading is in **g** (gravity included).
  * `Gyrometer.GetDefault()` — reading in **deg/s** (note: deg, not rad — Azul must convert).
  * `Magnetometer.GetDefault()` — µT.
  * `OrientationSensor.GetDefault()` — fused; `Quaternion` + rotation matrix.
  * `Inclinometer.GetDefault()` — Euler (pitch/roll/yaw) in degrees.
  * `LightSensor`, `Compass`, `ProximitySensor` for completeness.
* **Rate config:** `accelerometer.ReportInterval = 16 // ms`. Clamps to `MinimumReportInterval`. 16 ms ≈ 60 Hz.
* **Permission:** none for accelerometer/gyro; Windows 10/11 has an OS-level setting under Settings → Privacy → "Allow apps to access motion" — an unprivileged app gets 0 readings if the user disabled it. There's no per-app prompt.
* **Win32 fallback (older code, do not write new code against this):** `ISensor` COM API from `SensorsApi.h`. WinRT path supersedes it.

### Azul integration: `SensorManager`

```rust
// layout/src/managers/sensor.rs (new file)
pub struct SensorManager {
    pub subscriptions: BTreeMap<SubscriptionId, SensorSubscription>,
    pub last_readings: BTreeMap<SensorKind, SensorReading>,
    // platform backend writes here every sample (push); callbacks pull on demand.
}

#[repr(C, u8)]
pub enum SensorKind {
    Accelerometer,         // m/s² incl. gravity
    LinearAcceleration,    // m/s² excl. gravity
    Gravity,               // m/s² gravity only
    Gyroscope,             // rad/s
    Magnetometer,          // µT
    DeviceOrientation,     // quaternion (fused)
    UiOrientation,         // enum
    Proximity,             // bool
    AmbientLight,          // lux  (free-rider; iOS UIDevice / Android TYPE_LIGHT)
}

#[repr(C)]
pub struct SensorReading {
    pub kind: SensorKind,
    pub values: [f32; 4],      // x/y/z for vectors; quat = [x,y,z,w]; scalar = values[0]
    pub accuracy: f32,         // 0..1 (rotation_vector accuracy / iOS calibrated mag accuracy / clamped 0/1 elsewhere)
    pub timestamp_ns: u64,     // monotonic device timestamp
}

impl SensorManager {
    pub fn subscribe(&mut self, kind: SensorKind, rate_hz: f32) -> Result<SubscriptionId, SensorError>;
    pub fn unsubscribe(&mut self, id: SubscriptionId);
    pub fn read(&self, kind: SensorKind) -> Option<&SensorReading>;
    pub fn supported(&self, kind: SensorKind) -> bool;
}
```

`EventFilter` additions (these *do not* belong on `HoverEventFilter` — they're window-level, like `WindowResize`):

```rust
// core/src/events.rs
#[repr(C, u8)]
pub enum WindowEventFilter {
    // ...existing variants...
    SensorReading(SensorKind),   // fires every sample
    SensorAccuracyChanged(SensorKind),
}
```

`CallbackInfo` accessors (`layout/src/callbacks.rs`):

```rust
pub fn get_accelerometer(&self) -> Option<&SensorReading>;
pub fn get_linear_acceleration(&self) -> Option<&SensorReading>;
pub fn get_gravity(&self) -> Option<&SensorReading>;
pub fn get_gyroscope(&self) -> Option<&SensorReading>;
pub fn get_magnetometer(&self) -> Option<&SensorReading>;
pub fn get_device_orientation(&self) -> Option<&SensorReading>; // quaternion fused
pub fn get_ui_orientation(&self) -> UiOrientation;
pub fn get_proximity_near(&self) -> bool;
pub fn get_sensor_manager(&self) -> &SensorManager;
```

Platform injection seams (mirror `inject_native_gesture`):

```rust
pub fn inject_sensor_reading(&mut self, reading: SensorReading);
```

Each platform backend pushes into this from its native callback (CoreMotion handler block, Android `SensorEventListener.onSensorChanged` via JNI, IOHIDManager input report, libiio buffered read on a dedicated thread, `ReadingChanged` WinRT event). After injection, `frame_needs_regeneration = true` only if a subscription with `EventFilter::WindowEventFilter::SensorReading(_)` is registered.

W3C parallel: `DeviceMotionEvent` (acceleration / accelerationIncludingGravity / rotationRate) and `DeviceOrientationEvent` (alpha/beta/gamma + absolute). When the web backend lands, `WindowEventFilter::SensorReading(Accelerometer)` ↔ `devicemotion`, `SensorReading(DeviceOrientation)` ↔ `deviceorientation`. Browsers gate these behind `Permissions.query({name:"accelerometer"})` and Safari requires `DeviceOrientationEvent.requestPermission()` user-gesture.

**Risks / gotchas:**
* Battery — gyro at 100 Hz drains noticeably on mobile. Default `SensorSubscription.rate_hz` should be 30 Hz; let apps opt up.
* Axes are different *across UI orientations* on iOS+Android+Windows; document and provide a `read_in_ui_frame()` helper.
* macOS coverage is genuinely thin; expose `SensorManager::supported(Accelerometer) -> false` rather than faking values.
* Magnetometer is *very* noisy near magnets/metal cases; pair it with `accuracy` field rather than hiding it.
* Don't poll in callbacks — push samples into the manager from the backend; let `WindowEventFilter` fire `SensorReading(_)` events.

---

## Feature 6 — Gamepad input

### Conceptual model

A common, abstracted "standard gamepad" model (mirrors the W3C `Gamepad` interface's mapping = `"standard"`):

```
Buttons (indexed for W3C parity):
  0  = A / Cross         (south face)
  1  = B / Circle        (east face)
  2  = X / Square        (west face)
  3  = Y / Triangle      (north face)
  4  = L1 / LB           (left shoulder)
  5  = R1 / RB           (right shoulder)
  6  = L2 / LT           (left trigger; also analog → axis 4 or 2)
  7  = R2 / RT           (right trigger; also analog → axis 5 or 5)
  8  = Select / Back / Share
  9  = Start / Options / Menu
  10 = Left stick click
  11 = Right stick click
  12..15 = D-pad up/down/left/right
  16 = Home / Guide / PS / Xbox button
  17 = Touchpad click (DualShock4 / DualSense)
Axes:
  0 = LeftX (-1..+1)
  1 = LeftY (-1..+1; positive = down on most platforms — TODO: verify per platform sign)
  2 = RightX
  3 = RightY
  // Triggers are usually 0..1 *and* button bit; surface both.
```

A `GamepadId(u64)` disambiguates multiple controllers; on hot-plug the manager keeps `connected: bool` and a stable id for the session.

### iOS / iPadOS / tvOS — `GameController.framework`

* **API:** `GCController` (since iOS 7, expanded MFi & Bluetooth controllers in iOS 13+: Xbox Series, DualShock 4, DualSense, Stadia, Joy-Con-via-9.x).
* **Discovery:** `NSNotificationCenter` posts `GCControllerDidConnect` / `GCControllerDidDisconnect`. List via `GCController.controllers()`.
* **Reading:** `controller.extendedGamepad` (`GCExtendedGamepad`) is the standard layout. Fields like `buttonA.isPressed`, `leftThumbstick.xAxis.value`, `dpad.up.isPressed`. Value handlers: `extendedGamepad.valueChangedHandler = { gamepad, element in ... }` — pushed on the main queue per event.
* **Apple Pencil + iPad-as-controller via touch?** Not a controller. Use `PenState`.
* **Motion (`GCMotion`):** if controller has a gyro (DualShock 4, DualSense, Joy-Con), `controller.motion` exposes `.attitude` (quaternion), `.gravity`, `.userAcceleration`, `.rotationRate`. Bridges to the **SensorManager** path above.
* **Haptics:** iOS 14+ — `GCController.haptics: GCDeviceHaptics` returns a `CHHapticEngine` per locality (`.leftHandle`, `.rightHandle`, `.leftTrigger`, `.rightTrigger` on DualSense). Pattern-based rather than rumble-amplitude. For older controllers / simpler use: `GCDualShockGamepad.lightBarColor` (DualShock4), `GCDualSenseGamepad.adaptiveTriggers` (DualSense).
* **Permission:** none. Game Mode in iOS 18+ may prefer foreground app's controller routing — `TODO: verify`.
* **Background:** controllers pause when app is backgrounded; reconnect on foreground.

### Android — `InputDevice.SOURCE_GAMEPAD`

* **API:** standard input pipeline. `KeyEvent` for buttons (keycodes `KEYCODE_BUTTON_A`, `KEYCODE_BUTTON_B`, `KEYCODE_BUTTON_X`, ..., `KEYCODE_DPAD_*`, `KEYCODE_BUTTON_THUMBL/R`); `MotionEvent` for axes (`AXIS_X`, `AXIS_Y`, `AXIS_Z`, `AXIS_RZ`, `AXIS_HAT_X`, `AXIS_HAT_Y`, `AXIS_LTRIGGER`, `AXIS_RTRIGGER`, `AXIS_BRAKE`, `AXIS_GAS`).
* **Discovery:** `InputManager.getInputDeviceIds()` + `InputDevice.getDevice(id).getSources() & SOURCE_GAMEPAD == SOURCE_GAMEPAD` and `& SOURCE_JOYSTICK == SOURCE_JOYSTICK`. Hot-plug via `InputManager.InputDeviceListener`.
* **Rumble (since API 31 / Android 12):** `inputDevice.vibrator` (deprecated) → `inputDevice.vibratorManager` returning `VibratorManager`; each motor has a separate `Vibrator` you call `vibrate(VibrationEffect)` on. Pre-31: only single-motor rumble.
* **Motion sensors on controller (since API 31):** `inputDevice.getSensorManager()` returns a per-device `SensorManager` you register against just like the system one above. PS4/PS5 controllers light up `TYPE_ACCELEROMETER` + `TYPE_GYROSCOPE` here.
* **Permission:** none for buttons / axes. Vibration requires `<uses-permission android:name="android.permission.VIBRATE"/>` in manifest (long-standing, no runtime prompt).
* **JNI surface:** existing Android backend already eats `KeyEvent` / `MotionEvent` for keyboard/touch. New JNI methods `nativeOnGamepadButton(deviceId, keycode, pressed, eventTimeNs)` + `nativeOnGamepadAxis(deviceId, axisCode, value, eventTimeNs)`. Hot-plug calls into `nativeOnGamepadConnected(deviceId, vendorId, productId, name)`.

### macOS — `GameController.framework` (preferred) + IOHID fallback

* **`GameController.framework` on macOS (10.15 Catalina+):** same `GCController` API as iOS. Recommended path because Apple ships built-in Xbox / DualShock / DualSense / 8BitDo drivers via Bluetooth/USB. Connection notifications, `extendedGamepad`, `motion`, `haptics` all work.
* **IOHID fallback for legacy controllers / non-MFi:** `IOHIDManager` matching `kHIDPage_GenericDesktop` + `kHIDUsage_GD_Joystick` / `kHIDUsage_GD_GamePad`. This is what `gilrs` uses on macOS. `gilrs` does **not** use GameController.framework, so MFi-rumble paths on DualSense triggers aren't available via gilrs alone on macOS — `TODO: verify` (looked at gilrs `master` 0.10.x).
* **Permission:** none.

### Linux — `evdev` via `libevdev` / `udev`

* **Discovery:** `libudev` (or read `/run/udev/data/`); enumerate by subsystem `"input"` and look at `ID_INPUT_JOYSTICK=1`. Hot-plug via udev monitor socket.
* **Reading:** `/dev/input/eventN` is an evdev character device; events are `struct input_event { __u64 sec, __u64 usec, __u16 type, __u16 code, __s32 value }`. Types: `EV_KEY` (buttons; codes `BTN_SOUTH/EAST/NORTH/WEST/TL/TR/TL2/TR2/SELECT/START/MODE/THUMBL/THUMBR/DPAD_UP/...`), `EV_ABS` (axes; codes `ABS_X/Y/Z/RX/RY/RZ/HAT0X/HAT0Y/BRAKE/GAS`).
* **Older `/dev/input/jsN` joystick interface is deprecated** in favor of evdev. Some drivers still expose both; ignore `jsN`.
* **Rumble:** `EV_FF` (force-feedback) — `ioctl(fd, EVIOCSFF, &effect)` to upload a rumble effect, write `EV_FF` event to play it. `ff-rs` crate + `evdev-rs` crate cover this; `gilrs` wraps it.
* **Motion sensors on controller:** kernel exposes them as a *separate* input device — e.g., for DualSense, the controller appears as 3 devices: the main gamepad evdev, a motion-sensors evdev (`ABS_RX/RY/RZ/X/Y/Z` for accel+gyro), and a touchpad evdev. `gilrs` handles only the main one; for motion we'd open the second device directly. `TODO: verify` exact `dev/input/eventN` layout — relies on `hid-playstation` driver in 5.12+.
* **Permission:** `/dev/input/event*` is typically `root:input` mode 0640. `udev` rule `KERNEL=="event*", GROUP="input", MODE="0660"` + user in `input` group, or installer ships a udev rule that grants the app's user access. *Without* this, `gilrs` silently sees zero controllers.

### Windows — `Windows.Gaming.Input.Gamepad` (WinRT) + XInput legacy

* **Primary:** `Windows.Gaming.Input.Gamepad` (WinRT, since Windows 10). Auto-handles Xbox / generic XInput / WGI-aware controllers. Includes vibration motors (low-freq + high-freq + LeftTrigger + RightTrigger) and battery info.
* **Discovery:** static `Gamepad.Gamepads` (read-only list); subscribe to `Gamepad.GamepadAdded` / `GamepadRemoved`.
* **Reading:** `GamepadReading` struct from `gamepad.GetCurrentReading()` — buttons as a `GamepadButtons` bitflag, left/right thumbsticks (-1..+1), triggers (0..+1). Poll-based; no event callback for button changes (you poll each frame or wire `RawGameController` for events).
* **`RawGameController`** (also `Windows.Gaming.Input`): for non-standard controllers (joysticks, flight sticks). Reports buttons/axes/switches by index plus button-down/button-up events.
* **`Windows.Gaming.Input.Custom` & `IGameController`** for direct vendor reports.
* **DualSense advanced features (adaptive triggers, lightbar) on Windows:** *not* exposed through WinRT. Need DualSenseX-style USB HID writes or third-party crate (`hidapi`-based `dualsense-rs` exists; `TODO: verify` maintained status). For our scope: leave as future work; ship the standard 2-motor + trigger-motor rumble path through WinRT.
* **XInput (legacy):** `XInputGetState` / `XInputSetState` (8-controller max indexed 0..3 + 4..7 since Win10). Still works; `gilrs` falls back to this on older systems. Don't write new code against this when WinRT is available.
* **Permission:** none. Capability declaration in manifest only for UWP/MSIX-sandboxed apps (`<DeviceCapability Name="humaninterfacedevice">`), not for Win32 apps.

### Existing Rust crates

| Crate | Coverage | Status |
|---|---|---|
| **`gilrs`** 0.10.x | Windows (XInput + WinRT bits), Linux (evdev), macOS (IOHID) | Mature, *no iOS or Android backend*. Provides connection events, button/axis events, ID mapping. Built-in SDL_GameControllerDB mappings for hundreds of pads. |
| `sdl2` | All 5, but pulls SDL2 as a system library | Heavy dep; avoid. |
| `gamepad-rs` (older) | Linux/macOS/Windows | Less maintained; gilrs supersedes. |
| `windows` (`Windows_Gaming_Input`) | Windows-direct | Use under `cfg(windows)` for vibration / lightbar / battery. |
| `objc` + `objc-foundation` | macOS + iOS direct GameController | Needed for `GCMotion`, `GCDualSenseGamepad.adaptiveTriggers`, haptics. |
| `evdev-rs` / `libudev-sys` | Linux direct | Useful for the motion-sensor secondary device gilrs ignores. |

**Recommendation:** `gilrs` for Windows + Linux + macOS as the baseline; the iOS and Android backends use native GameController.framework / InputDevice through the platform-shell `Inject*` seam (mirrors what we did for native gestures). All four paths converge on the same `GamepadManager` state.

### Azul integration: `GamepadManager`

```rust
// layout/src/managers/gamepad.rs (new file)
pub struct GamepadManager {
    pub connected: Vec<Gamepad>,
}

#[repr(C)]
pub struct Gamepad {
    pub id: GamepadId,                      // u64 — stable for session
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: AzString,
    pub mapping: GamepadMapping,            // Standard | Unknown
    pub button_state: u32,                  // bitmask, W3C index order
    pub axes: [f32; 8],                     // 0..3 = LX/LY/RX/RY; 4..5 = triggers; 6..7 = future (e.g. hat for non-D-pad)
    pub haptics: GamepadHaptics,            // None | Rumble(low_hz, high_hz) | Trigger(left, right) | LightBar(rgb) bitmask
    pub motion: Option<GamepadMotion>,      // gyro/accel present?
    pub battery_pct: f32,                   // -1.0 if unknown
}

#[repr(C, u8)]
pub enum GamepadButton {
    A=0, B=1, X=2, Y=3,
    L1=4, R1=5, L2=6, R2=7,
    Select=8, Start=9,
    LeftStick=10, RightStick=11,
    DpadUp=12, DpadDown=13, DpadLeft=14, DpadRight=15,
    Home=16, Touchpad=17,
}

#[repr(C, u8)]
pub enum GamepadAxis { LeftX=0, LeftY=1, RightX=2, RightY=3, LeftTrigger=4, RightTrigger=5 }
```

`EventFilter` additions — these *are* window-level, not per-node:

```rust
// core/src/events.rs WindowEventFilter
GamepadConnected(GamepadId),
GamepadDisconnected(GamepadId),
GamepadButtonPressed { id: GamepadId, button: GamepadButton },
GamepadButtonReleased { id: GamepadId, button: GamepadButton },
GamepadAxis { id: GamepadId, axis: GamepadAxis, value: f32 },
```

`CallbackInfo` accessors:

```rust
pub fn get_gamepad_manager(&self) -> &GamepadManager;
pub fn get_gamepads(&self) -> &[Gamepad];
pub fn get_gamepad(&self, id: GamepadId) -> Option<&Gamepad>;
pub fn set_gamepad_rumble(&mut self, id: GamepadId, low_freq: f32, high_freq: f32, duration_ms: u32) -> Result<(), GamepadError>;
```

Platform injection seams: each backend pushes button/axis changes via `inject_gamepad_event(GamepadId, GamepadEvent)`. On iOS/Android these are real callbacks; on Win/Linux/macOS gilrs `next_event()` is pumped on the event loop tick.

W3C parallel: `Gamepad` interface + `gamepadconnected` / `gamepaddisconnected` events. Polling model (`navigator.getGamepads()`) — our manager satisfies both push and pull because `connected[]` is always live.

**Risks / gotchas:**
* Multi-controller routing: which gamepad event a callback "owns" — keep it simple, route to whichever window has focus.
* Dead-zones: bake a default dead-zone (~0.1 absolute) into axis emission but expose `GamepadManager.set_deadzone(GamepadAxis, f32)`.
* Trigger ambiguity: surface as both axis (0..1) and button (pressed > 0.5 threshold).
* iOS controller motion → don't duplicate into `SensorManager`; flag with `GamepadMotion.is_primary_motion: bool`. App opts in.
* On Linux, `gilrs` is silent if udev rules are wrong — surface "no controllers visible but expected" via a one-time diagnostic log.

---

## Feature 7 — Wacom / Drawing-Tablet Extensions (beyond existing `PenState`)

Existing `PenState` (`layout/src/managers/gesture.rs:360`) already has `pressure`, `tilt`, `in_contact`, `is_eraser`, `barrel_button_pressed`. None of the platform backends *populate* `is_eraser` or `barrel_button_pressed` yet. **First job:** wire those existing fields. **Second job:** add the *tablet-only* surface (touch-ring, ExpressKeys, touch-strip, hover/proximity, squeeze for Pencil Pro).

### iOS / iPadOS — Apple Pencil

* **`UITouch` properties on Pencil:** `touch.type == .pencil`, `touch.force` (0..maximumPossibleForce), `touch.altitudeAngle` (radians; 0 = flat to screen, π/2 = perpendicular), `touch.azimuthAngle(in: view)` (radians around screen normal). Tilt = derived from altitude+azimuth.
* **Pencil generations & features:**
  * **Apple Pencil 1 (2015):** pressure + tilt; no barrel button; no eraser tip (Apple Pencils don't have a physical eraser-end). Double-tap not supported.
  * **Apple Pencil 2 (2018):** + double-tap on the flat side. `UIPencilInteraction.delegate` reports `pencilInteractionDidTap(_:)`. The app reads `UIPencilPreferredAction` user setting (e.g. switch tool, show color picker).
  * **Apple Pencil USB-C (2023):** drops pressure & double-tap.
  * **Apple Pencil Pro (2024):** + squeeze gesture (`UIPencilInteraction.PencilTap` + `.squeeze` → `pencilInteractionDidSqueeze(_:)`), + barrel-roll (`UITouch.rollAngle` in radians — new in iPadOS 17.5+), + haptic feedback via `UIFeedbackGenerator` paired with squeeze, + Find My / lost-pencil detection (out of scope).
  * **Hover (M2 iPad Pro + Pencil 2/Pro):** Pencil up to ~12 mm above the screen reports `UITouch.phase == .stationary` with `UITouch.location` valid pre-contact. Surface via the existing `PenEnter` / `PenLeave` filters. `TODO: verify` — should we add a `PenHover` filter for the *moving* hover case?
* **No eraser tip / no barrel button on any Apple Pencil.** `PenState.is_eraser` stays `false`, `barrel_button_pressed` stays `false`. The "eraser preferred action" on Pencil 2 is *user-configured* via `UIPencilPreferredAction.switchEraser` and surfaced as a `UIPencilInteraction` event — semantic, not hardware. Treat as a separate filter:

  ```rust
  // additions
  PenSqueeze,                // Pencil Pro — once per squeeze
  PenDoubleTap,              // Pencil 2 + Pencil Pro double-tap-on-side
  PenRoll(f32),              // Pencil Pro barrel-roll radians
  ```

* **Permission:** none.

### Android — Stylus events

* **`MotionEvent.getToolType(pointerIndex)`:** `TOOL_TYPE_STYLUS` or `TOOL_TYPE_ERASER`. The latter sets `PenState.is_eraser = true` directly.
* **`MotionEvent.getPressure()`, `getOrientation()` (radians, in xy-plane around z-axis = azimuth), `getAxisValue(AXIS_TILT)` (radians from screen normal).**
* **Barrel button:** `MotionEvent.BUTTON_STYLUS_PRIMARY` / `BUTTON_STYLUS_SECONDARY` in `getButtonState()`. Maps directly to `PenState.barrel_button_pressed`.
* **Hover:** `ACTION_HOVER_ENTER / HOVER_MOVE / HOVER_EXIT` with `TOOL_TYPE_STYLUS` source — already covered by `PenEnter` / `PenMove` / `PenLeave` filters; backend just needs to honor `TOOL_TYPE_STYLUS`.
* **Samsung S Pen specifics:** double-tap and air-gestures are Samsung-only via `SpenSdk` (proprietary `com.samsung.android.sdk.pen`). Out of scope for now — treat S Pen as a regular stylus.
* **Permission:** none.

### macOS — `NSEvent` tablet events + Wintab-on-Mac (not a thing)

* **`NSEvent.type` values:** `tabletPoint` (continuous samples during contact), `tabletProximity` (entering/leaving sensing range). Subtype `NSEventSubtypeTabletPoint` on regular mouse-moved/mouse-dragged events gives tablet-only fields.
* **Fields on a tablet `NSEvent`:**
  * `pressure` (0..1)
  * `tilt` (`NSPoint` x/y in normalized -1..1, *not* radians)
  * `tangentialPressure` (0..1) — applies to airbrush-style tools
  * `rotation` (radians) — barrel rotation, exposes Wacom Art Pen and Apple Pencil Pro barrel-roll equivalent
  * `buttonMask` (NSUInteger bitmask) — bit 0 = tip switch, bit 1 = barrel/side button, bit 2 = eraser, plus higher bits for more buttons on stylus models that have them
  * `pointingDeviceType` enum: `unknown`, `pen`, `cursor` (puck), `eraser`. `eraser` → `PenState.is_eraser = true`.
  * `deviceID` (NSUInteger) and `pointingDeviceSerialNumber` — let us pin events to a particular physical pen across proximity-in / proximity-out cycles.
* **No Wintab on macOS.** Wacom's macOS driver exposes everything through the standard `NSEvent` tablet surface; nothing additional is needed for basic tablets.
* **Touch-ring, ExpressKeys, touch-strip:** Wacom driver routes these as **standard keystrokes** or as **HID button reports** depending on driver config. To intercept them programmatically:
  * **Wacom WET (`com.wacom.WacomTabletDriver`) IPC:** undocumented but present; not a public API. Skip.
  * **IOHIDManager with usagePage `kHIDPage_GenericDesktop` / `kHIDPage_Digitizer`:** Wacom tablets enumerate as a digitizer + a separate HID device exposing the touch-ring/ExpressKey/touch-strip as buttons + a wheel axis. **This is the realistic path** — we ship an IOHID consumer for known Wacom vendor IDs (Wacom = `0x056a`).
  * For Apple Pencil hardware-double-tap / squeeze when used with a paired iPad through Sidecar — not a macOS local input; out of scope.
* **Permission:** none for tablet `NSEvent`s. IOHID for HID buttons may require user grant in System Settings → Privacy → "Input Monitoring" on macOS 15+ if we want raw HID. `TODO: verify` whether digitizer-page reads avoid this.

### Linux — libwacom + libinput + XInput2 fallback

* **`libinput`** is the modern input stack (used by GNOME, KDE Wayland, sway). It exposes tablet events as `libinput_event_tablet_tool` (proximity in/out, motion, tip, button) and `libinput_event_tablet_pad` (ExpressKeys, rings, strips).
  * `libinput_event_tablet_tool_get_pressure / get_tilt_x / get_tilt_y / get_rotation / get_slider_position`
  * `libinput_event_tablet_tool_get_type` → `LIBINPUT_TABLET_TOOL_TYPE_PEN / ERASER / BRUSH / PENCIL / AIRBRUSH / MOUSE / LENS / TOTEM`
  * `libinput_event_tablet_tool_get_serial / get_tool_id` — physical pen identity
  * `libinput_event_tablet_pad_get_ring_position` (degrees), `get_strip_position` (0..1), `get_button_number`
* **`libwacom`** is the *database* (not an event API): given a USB vendor/product ID it tells you "this tablet has 4 ExpressKeys + 1 ring on the left, 2 ExpressKeys on the right, no touch-strip, has touch support, supported pens are…". Use libwacom to label rings/strips/buttons (ExpressKey 1 = top-left button, etc.) instead of just emitting indices.
* **Crates:**
  * `input` (https://crates.io/crates/input) — libinput Rust bindings. Mature; used by `winit` Wayland path.
  * `libwacom-rs` — **does not exist** as a published crate as of writing. `TODO: verify` — there's a `libwacom-sys` shim on GitHub but unpublished. We can bind manually via `bindgen` against `<libwacom/libwacom.h>` (small surface ~20 fns).
* **XInput2 fallback (X11 only, when libinput isn't available):** `XIDeviceEvent` with `valuators` array — each tablet device exposes pressure/tilt/rotation as separate valuators. Labels via `XIQueryDevice`. More fiddly than libinput; skip if at all possible.
* **Permission:** libinput typically runs as root inside the Wayland compositor and routes events via the compositor (Wayland `zwp_tablet_v2` protocol). The Wayland client sees tablet events through that protocol; the X11 client sees them via XInput2. Our existing `linux/wayland/` and `linux/x11/` backends route into the same `event::PlatformEvent` enum, then into `PenState` injection. No additional setuid / capability.

### Windows — Wintab vs Windows Ink

* **Windows Ink (preferred, since Win10):** `Windows.UI.Input.Inking.*` + `Windows.UI.Input.PointerPoint` exposes stylus events through the standard pointer pipeline. `PointerPoint.Properties` gives `Pressure` (0..1), `XTilt` / `YTilt` (degrees), `Twist` (degrees, barrel rotation), `IsBarrelButtonPressed`, `IsEraser`, `IsInverted`, `TouchConfidence`. **Use this path on Win10/11.**
* **`Windows.Devices.Input.PenDevice`:** discovery + identity.
* **Wintab (legacy, since 1991):** Wacom's cross-vendor API; DLL `Wintab32.dll` exposes packet queue. Required for **older drivers** and for apps that need things Windows Ink doesn't expose, historically including ExpressKeys/touch-rings on Wacom hardware. Crate: **`wintab-rs`** does exist (https://crates.io/crates/wintab) — small, last published ~2023; `TODO: verify` maturity. Alternative: `wintab_lite` (bindings-only). 
* **Tablet PC API (deprecated):** `RealTimeStylus` (RTS). Skip.
* **ExpressKeys / touch-rings on Windows:** Wacom's Driver Control Panel maps them to **keyboard shortcuts by default** so naive apps see them as F-keys. To capture the raw button: use Wintab `WTPacketsGet`, which delivers `tabletButton` / `tabletWheel` packets. With the driver in "default" mode the keys are remappable by user; with a per-app override the keys arrive as button events. The reliable path is **HID-direct** through `HidD_GetHidGuid` + open the Wacom HID device, parse our own reports. Same approach as macOS IOHID path.
* **Permission:** none for Win32. For UWP/MSIX `<DeviceCapability Name="humaninterfacedevice">` for raw HID.

### Azul integration: extend `PenState` + new `TabletPadManager`

Three layers of additions:

**Layer 1: populate existing `PenState` fields.** Wire `is_eraser` + `barrel_button_pressed` in every backend:
* iOS: `UITouch.type == .pencil` and a separate gesture for "preferred action = eraser" — Apple Pencil has no eraser tip, so `is_eraser` stays false there. *Android stylus with `TOOL_TYPE_ERASER`* and *macOS `NSEvent.pointingDeviceType == eraser`* and *Win10 Ink `PointerPoint.Properties.IsEraser`* and *Linux libinput `LIBINPUT_TABLET_TOOL_TYPE_ERASER`* → set `is_eraser = true`.
* Barrel button: Android `BUTTON_STYLUS_PRIMARY`, macOS `NSEvent.buttonMask & 0x2`, Win10 `IsBarrelButtonPressed`, Linux libinput `libinput_event_tablet_tool_get_button_state(..., BTN_STYLUS, ...)`. iOS Apple Pencil has none.

**Layer 2: extend `PenState` with tablet-pad-tracking fields:**

```rust
// layout/src/managers/gesture.rs PenState
pub struct PenState {
    // existing fields …
    pub tangential_pressure: f32,         // airbrush wheel (0..1)
    pub barrel_roll_rad: f32,             // Pencil Pro / Wacom Art Pen / Win Ink Twist
    pub buttons_extra: u8,                // bitmask for stylus buttons beyond barrel (some pens have 3+)
    pub tool_id: u32,                     // libinput tool_id / Wacom Wintab UniqueID — pins a physical pen
}
```

**Layer 3: new `TabletPadManager` — ExpressKeys / touch-ring / touch-strip live here, not on `PenState` (those are pad inputs, not pen inputs):**

```rust
// layout/src/managers/tablet_pad.rs (new file)
pub struct TabletPadManager {
    pub pads: Vec<TabletPad>,
}

#[repr(C)]
pub struct TabletPad {
    pub id: TabletPadId,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: AzString,
    pub num_express_keys: u8,
    pub num_rings: u8,
    pub num_strips: u8,
    pub express_keys: u32,                // bitmask of up to 32 keys (Intuos Pro Large has 8)
    pub ring_position: [f32; 2],          // degrees, per ring; -1 = not present
    pub strip_position: [f32; 2],         // 0..1, per strip; -1 = not present
}
```

`EventFilter` additions:

```rust
// HoverEventFilter — these can propagate by node hover (so you can put a
// per-node tablet brush on a canvas widget):
PenSqueeze,                              // Apple Pencil Pro
PenDoubleTap,                            // Apple Pencil 2/Pro side tap
PenHover,                                // Pencil 2 + iPad Pro / any tablet proximity-while-moving

// WindowEventFilter — pad inputs go window-level (they're peripheral controls):
TabletPadButton { pad: TabletPadId, key_index: u8, pressed: bool },
TabletPadRing { pad: TabletPadId, ring_index: u8, position_deg: f32 },
TabletPadStrip { pad: TabletPadId, strip_index: u8, position: f32 },
```

`CallbackInfo` accessors (additions to existing pen accessors):

```rust
pub fn get_pen_tangential_pressure(&self) -> Option<f32>;
pub fn get_pen_barrel_roll(&self) -> Option<f32>;
pub fn get_tablet_pad_manager(&self) -> &TabletPadManager;
pub fn get_tablet_pads(&self) -> &[TabletPad];
pub fn get_express_keys(&self, pad: TabletPadId) -> u32;        // bitmask
pub fn get_touch_ring_position(&self, pad: TabletPadId, ring: u8) -> Option<f32>;  // degrees
pub fn get_touch_strip_position(&self, pad: TabletPadId, strip: u8) -> Option<f32>; // 0..1
```

W3C parallel: `PointerEvent` already covers most pen tilt/pressure/tangential/twist/eraser/barrel-button. The W3C `PointerEvent.pointerType == "pen"` and `PointerEvent.buttons & 32` (eraser) + `& 16` (barrel-side) + properties `tiltX/tiltY/twist/tangentialPressure/altitudeAngle/azimuthAngle` give us 1-to-1 parity for the *pen* side. **There is no W3C interface for tablet-pad ExpressKeys / touch-ring** — these are HID-direct on every platform, no web equivalent. When the web backend lands, `TabletPadManager` returns empty, `WindowEventFilter::TabletPad*` filters never fire.

### Existing Rust crates

| Crate | Purpose | Notes |
|---|---|---|
| `wintab-rs` / `wintab` | Wintab32.dll bindings | `TODO: verify` last release date; if stale, vendor minimal bindings inline. |
| `wintab_lite` | Lighter Wintab bindings | Alternative. |
| `input` (libinput Rust binding) | Linux tablet pad/tool | Mature. Used in winit. |
| `libwacom-sys` (unpublished) | libwacom DB | Need to vendor our own bindgen pass. |
| `udev` / `libudev-sys` | Device discovery | For pad hot-plug on Linux. |
| `hidapi` | Cross-platform raw HID | Useful for Wacom-direct on Win/macOS when high-level APIs miss ExpressKeys. |
| `objc` + Cocoa bindings | macOS `NSEvent.tabletProximity` / `tabletPoint` | We already use `objc` for macOS shell. |
| `windows` (`Windows_UI_Input`) | Windows Ink | Standard MS bindings. |

**Risks / gotchas:**
* "Eraser" is fundamentally a *driver-decided semantic*. Apple Pencil has no physical eraser tip; the OS lets the user assign double-tap → erase. Document that `is_eraser=true` on iOS is set by `UIPencilPreferredAction.switchEraser` and the user's tool-toggle state — not a hardware bit.
* Wacom tablet-pad ExpressKeys on Windows: by default the Wacom driver maps them to keystrokes for *every* app unless a per-app profile says otherwise. Apps that want raw button events need to (a) tell users to add a "Use raw button events" profile in the Wacom driver, or (b) parse HID-direct, which collides with the driver. We should expose `TabletPadManager` only when we successfully open the HID device, log a warning otherwise, and let users decide.
* Multiple pens / pen swaps: `PenState.device_id` is already there; on Wacom + Linux libinput `tool_id` distinguishes "pen on pad #1" from "eraser-end of the same pen" from "second Art Pen." On macOS `pointingDeviceSerialNumber` + `pointingDeviceID`; on iOS no multi-pen support (one Pencil at a time); on Win10 Ink `PointerPoint.PointerId` plus `PenDevice.Id`.
* Apple Pencil 2 `UIPencilInteraction` is a per-app singleton, registered on the `UIWindow`; we wire it once in the iOS shell.

---

## Cross-feature: API surface summary

Adding three managers and a handful of event variants. To avoid duplicating the §0 architecture seam, here's a single table of edits:

| File | Edit |
|---|---|
| `layout/src/managers/sensor.rs` (new) | `SensorManager` + `SensorKind` + `SensorReading` + `subscribe` API |
| `layout/src/managers/gamepad.rs` (new) | `GamepadManager` + `Gamepad` + `GamepadButton` / `GamepadAxis` enums |
| `layout/src/managers/tablet_pad.rs` (new) | `TabletPadManager` + `TabletPad` |
| `layout/src/managers/gesture.rs` | extend `PenState` with `tangential_pressure`, `barrel_roll_rad`, `buttons_extra`, `tool_id` |
| `layout/src/managers/mod.rs` | re-export the new managers; thread them through `LayoutWindow::frame_setup` |
| `layout/src/window.rs` | add `sensor_manager / gamepad_manager / tablet_pad_manager: …` fields on `LayoutWindow` (mirror the existing manager wiring) |
| `layout/src/callbacks.rs` | new accessors listed in each section above |
| `core/src/events.rs` | new `HoverEventFilter` variants (`PenSqueeze`, `PenDoubleTap`, `PenHover`); new `WindowEventFilter` variants (`SensorReading`, `SensorAccuracyChanged`, `Gamepad*`, `TabletPad*`); update `EventType` to mirror |
| `dll/src/desktop/shell2/ios/mod.rs` | `CMMotionManager` lifetime mgmt + handler block → `inject_sensor_reading`; `GCController` notification + value handler → `inject_gamepad_event`; `UIPencilInteraction` → `inject_native_gesture(NativeGestureEvent::PenSqueeze / PenDoubleTap)`; populate `PenState.is_eraser` from `UIPencilPreferredAction` |
| `dll/src/desktop/shell2/android/mod.rs` | JNI `AzulSensorBridge` (Java glue) → `nativeOnSensorEvent` → `inject_sensor_reading`; per-controller `InputDevice.SOURCE_GAMEPAD` `KeyEvent`/`MotionEvent` JNI passthrough → `inject_gamepad_event`; `BUTTON_STYLUS_PRIMARY` + `TOOL_TYPE_ERASER` → wire existing `PenState` fields |
| `dll/src/desktop/shell2/macos/mod.rs` | `IOHIDManager` + Wacom IOHID consumer for pad events; `GCController` (preferred) or `gilrs` for gamepads; `NSEvent.tabletProximity` / `tabletPoint` populate `PenState` |
| `dll/src/desktop/shell2/linux/mod.rs` | `libinput` tablet/tablet-pad events (compositor side already wires libinput for Wayland; X11 backend uses XInput2); `gilrs` for gamepads; `iio-sensor-proxy` D-Bus client *or* direct `industrial-io` sysfs for sensors |
| `dll/src/desktop/shell2/windows/mod.rs` | `Windows.Devices.Sensors` event handlers; `Windows.Gaming.Input.Gamepad` event handlers; `PointerPoint.Properties` reads in existing pointer-event path; `wintab-rs` (or vendored Wintab bindings) for ExpressKeys/touch-ring HID |
| `dll/build.rs` | iOS: link `CoreMotion`, `GameController`; macOS: link `CoreMotion` only if used (it isn't — gate behind cfg), `GameController`, `IOKit`; Android: ensure `AzulSensorBridge.java` is included in the AAR template under `dll/src/desktop/shell2/android/java/`; Linux: pkg-config for `libinput` (existing) + optional `libwacom`; Windows: WinRT features in the `windows` crate's manifest |
| `doc/api.json` | new types + accessors so all 35 binding languages get the surface |

---

## Web/W3C cross-reference (for the future web backend)

| Azul | W3C |
|---|---|
| `SensorReading(Accelerometer)` | `DeviceMotionEvent.accelerationIncludingGravity` |
| `SensorReading(LinearAcceleration)` | `DeviceMotionEvent.acceleration` |
| `SensorReading(Gyroscope)` | `DeviceMotionEvent.rotationRate` |
| `SensorReading(DeviceOrientation)` | `DeviceOrientationEvent.{alpha,beta,gamma,absolute}` |
| `SensorReading(UiOrientation)` | `screen.orientation` |
| `SensorReading(Proximity)` | (no stable W3C, Proximity Sensor API is unshipped) — `TODO: verify` |
| `GamepadManager.connected` | `navigator.getGamepads()` |
| `GamepadConnected` / `Disconnected` | `gamepadconnected` / `gamepaddisconnected` events |
| `GamepadButtonPressed` / `GamepadAxis` | poll `Gamepad.buttons[i].pressed/value` and `Gamepad.axes[i]` |
| `set_gamepad_rumble` | `GamepadHapticActuator.playEffect("dual-rumble", …)` |
| `PenSqueeze` | (no standard) — Pencil Pro is iPadOS-only |
| `PenDoubleTap` | (no standard) — Pencil 2 is iPadOS-only |
| `PenHover` | `pointerenter` + `PointerEvent.altitudeAngle / azimuthAngle` while `buttons === 0` |
| `PenState.tangential_pressure` | `PointerEvent.tangentialPressure` |
| `PenState.barrel_roll_rad` | `PointerEvent.twist` (in degrees — need /(180/π)) |
| `TabletPadButton` / `TabletPadRing` / `TabletPadStrip` | (no standard) |

Permissions API gates (web side):
* `navigator.permissions.query({name:"accelerometer"})` / `gyroscope` / `magnetometer` / `ambient-light-sensor` — Permissions API specs each `name` separately.
* Safari requires `DeviceOrientationEvent.requestPermission()` — must be user-gesture.
* Browsers may suspend sensors when tab is backgrounded; mirrors iOS app-background behavior.
* The Gamepad API doesn't require permission but only returns non-null devices "in response to user activation" in some browsers (Chromium/Edge); Firefox is more lenient.

---

## Sprint ordering recommendation (for next session)

1. **Wire existing `PenState.is_eraser` + `barrel_button_pressed` first.** Touch every backend; no new types; verifies the pen pipeline end-to-end. (~½ day.)
2. **`SensorManager`.** Self-contained; no event-filter routing complexity. Win/Mac/Linux are easy with WinRT/IOHID/iio; iOS/Android need backend glue. (~2 days; iOS+Android dominate.)
3. **`GamepadManager` with `gilrs` baseline.** Win/Linux/macOS for free; iOS/Android native backends as separate ½-day passes each. Rumble lands together with discovery — keep it. (~2 days.)
4. **Extend `PenState` with `tangential_pressure / barrel_roll / tool_id`.** Pure data plumbing; backends fill fields. (~½ day.)
5. **`PenSqueeze` / `PenDoubleTap` filter** — iOS `UIPencilInteraction` only. Other platforms emit `Unsupported`. (~½ day.)
6. **`TabletPadManager`.** Hardest because Wacom HID-direct on Win/macOS is genuinely fiddly; libinput on Linux is straightforward; iOS/Android n/a. Defer until a user asks. (~3 days when needed.)

Total raw estimate for steps 1-5: **~5-6 dev days** including the per-backend smoke tests + api.json + codegen pass. Step 6 is its own sprint.

---

## Open verification items (`TODO: verify`)

* iOS 17/18 `Info.plist` requirement for `CMMotionManager` raw streams (we believe none; Apple's docs are ambiguous since the Motion+Fitness setting was introduced).
* Windows tablet rotation behavior for `Accelerometer` axes — does the OS pre-rotate or not?
* `gilrs` macOS path — confirm it does *not* use `GameController.framework` (in which case Mac DualSense advanced features go through a separate native code path).
* Linux PS5 motion-sensor exposure as separate evdev device — verify on a kernel with `hid-playstation` 5.12+.
* `wintab-rs` last-published / activeness — confirm we don't have to vendor.
* `libwacom-sys` — confirm no published crate; vendor bindgen wrapper.
* macOS Input-Monitoring permission requirement for IOHID digitizer-page reads (we suspect tablet `NSEvent`s are fine without it; raw HID with usagePage 0x0D might trip the prompt).
* W3C Proximity Sensor API current status (Chromium experiment vs shipped).
* Android API 31+ `inputDevice.getSensorManager()` JNI surface — make sure the Java-side glue runs in the same `Looper` as the existing input listener.
* Apple Pencil hover via `UITouch.phase == .stationary` semantics for *moving* hover — confirm vs `.began` with `force == 0` and the new `UITouch.previousLocation` chain.
