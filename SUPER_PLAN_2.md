# SUPER_PLAN_2 — Mobile-era integrations

**Status:** planning. SUPER_PLAN.md (Sprints A–N) is closed; iOS + Android backends compile and the gesture / event surface is unified across 5 platforms. This document is the *next* day's work: the broad feature integrations a real-world mobile app expects but a desktop-only UI framework leaves to the user.

**Targets (5 platforms):** iOS, Android, macOS, Linux, Windows. Web/WASM is its own world; each feature's design must be web-shape-compatible (i.e., when we add the web backend, the same DOM node / EventFilter / CallbackInfo accessor should map cleanly to a W3C primitive — `<video>`, `Permissions API`, `WebAuthn`, etc.).

**Scope rule, again:** Azul is a *superset of every platform*. A feature exposes the highest-quality native implementation per platform with a single Azul-side API; platforms with weaker native support get a fallback (or, if no fallback is possible, the call returns a typed `Unsupported` error rather than failing silently).

---

## 0. Architecture seams already in place

These existed before SUPER_PLAN_2 and unblock most of the integrations below:

* **NodeType** (`core/src/dom.rs`) — every DOM node is one of a closed enum. Adding new media nodes (`Video`, `Camera`, `MapTile`, `Pdf`) is a NodeType extension + a renderer in `cpurender.rs` / `wr_translate2.rs`. Pattern: see how `NodeType::Image(ImageRef)` works.
* **EventFilter** (`core/src/events.rs`) — 165 variants today (touch / pen / gestures / DnD / IME landed in this branch). Adding `On::CameraFrame`, `On::GeolocationUpdate`, `On::Accelerometer`, `On::Gamepad*` is more variants of `HoverEventFilter` (or a new `SensorEventFilter` if sensor events shouldn't propagate by node hover).
* **CallbackInfo accessors** (`layout/src/callbacks.rs`) — already exposes `get_gesture_drag_manager`, `get_pen_state`, etc. New permissions / capture / sensor managers attach here.
* **Manager pattern** (`layout/src/managers/`) — `GestureAndDragManager`, `FocusManager`, `TextInputManager`, etc. Each new capability gets its own manager so the cross-platform plumbing stays consistent.
* **Platform backends** (`dll/src/desktop/shell2/<platform>/mod.rs`) — each is responsible for translating native events into manager mutations. The injection seam we built for native gestures (`inject_native_gesture` → override slot → `detect_*` accessor) is the model for native permission prompts, camera frames, sensor samples, etc.
* **api.json + codegen** (`doc/`) — every new type / accessor goes through `azul-doc autofix add` + `codegen all` so 35 binding languages get the surface for free.

---

## 1. Feature inventory (12 topics, 5 platforms each)

### Asset / permission foundation

1. **Mobile font / image loading + permissions** — does `rust-fontconfig` find system fonts on iOS / Android? Image decode paths (the existing `image` crate) work across, but file-system reads on iOS require `Documents/` sandbox awareness; Android wants `MediaStore`-aware paths. Permission flow for camera-roll image picking is its own thing (`Photos` / `READ_MEDIA_IMAGES`).

### Media capture

2. **Camera (front / back)** — `<video>`-equivalent DOM node. iOS `AVCaptureSession`, Android `CameraX`, macOS `AVCaptureSession`, Linux `pipewire` (or `v4l2` direct), Windows `MediaCapture`. Frame delivery via a new `CameraSource` resource type, rendered through the existing `ImageRef` pipeline (we update the same texture every frame).
3. **Screen sharing (window / entire screen)** — like Chrome's `getDisplayMedia`. macOS `ScreenCaptureKit`, iOS RPBroadcastSampleHandler (limited, app-only), Android `MediaProjection`, Linux PipeWire screencast portal, Windows `Graphics.Capture`. UX: an `App::request_display_capture()` returning a permission prompt → frame stream.

### Security / identity

4. **Biometric auth (FaceID / TouchID / Hello / fingerprint)** — single Azul API: `request_biometric_auth(prompt: AzString, fallback: BiometricFallback) -> BiometricResult`. iOS `LAContext`, Android `BiometricPrompt`, macOS `LAContext`, Linux `polkit` + PAM (poor — fallback to password), Windows Hello (`UserConsentVerifier`).

### Hardware sensors / extended input

5. **Accelerometer (+ gyro + magnetometer)** — iOS `CMMotionManager`, Android `SensorManager`, macOS `IOHIDManager` (laptops have it!), Linux `iio` sysfs, Windows `Windows.Devices.Sensors`. Push samples into a new `SensorManager` → CallbackInfo accessor + new `EventFilter::Sensor(Accelerometer)`.
6. **Gamepad input** — `gilrs` crate is the cross-platform fallback (works on Win/Linux/macOS). iOS / Android need GCController / InputDevice. Two new EventFilter variants: `GamepadButton{Pressed,Released}(GamepadButton)` and `GamepadAxis(Axis, f32)`.
7. **Wacom / drawing tablet** — we already have `PenState` + `PenTilt`. The extension is the *barrel-button* / *eraser* tip / *touch-ring* / *express-keys* surface. iOS Pencil reports tilt and force; Android stylus too. macOS / Windows wacom drivers expose extra device events via Wintab / `NSEvent.tabletProximity`. Linux: libwacom + libinput.

### System integration

8. **File pickers on mobile** — iOS `UIDocumentPickerViewController`, Android `Intent.ACTION_OPEN_DOCUMENT` (Storage Access Framework). Desktop `tfd` is already wired; mobile equivalents need their own backend impls. API stays `FileDialog::open_file(...)`.
9. **Text input on mobile (soft keyboard, IME)** — iOS `UITextInput` protocol on `AzulView`, Android `InputConnection` (we already have a JNI bridge skeleton). Composition events (`compositionstart`/`update`/`end`) are already wired in `HoverEventFilter`. What's missing: actually feeding the IME composition state into `TextInputManager::handle_composition`.
10. **Geolocation** — `<MapWidget>` clone needs `navigator.geolocation`-equivalent. iOS `CLLocationManager`, Android `LocationManager` / `FusedLocationProviderClient`, macOS `CLLocationManager`, Linux `geoclue` D-Bus, Windows `Windows.Devices.Geolocation`. Single Azul API: `request_geolocation(prompt: AzString) -> Result<LocationStream, _>`.

### External format support

11. **MVT vector tiles + map widget (openfreemap)** — `mvt` crate decodes Mapbox Vector Tile protobufs. Map widget = a `NodeType::Map` that owns a viewport (lat/lon + zoom), fetches tiles via `ureq` (already a dep), decodes MVT, and renders via the existing display-list path (lines / polygons / labels). Renderer can be CPU (cpurender) or GPU (a new MapDisplayItem in webrender).
12. **PDF integration via printpdf** — `printpdf` is the user's own crate. Two directions: (a) *render* a `NodeType::Pdf(PdfRef)` so a PDF page shows inline like an image (printpdf has a render path); (b) *export* — `App::export_pdf(path)` walks the current display list and emits a PDF via printpdf. The latter is more interesting for a doc/report tool.

### Data layer

13. **SQLite / libsql protocol support** — completes the "full app framework" by adding a built-in persistence layer reachable from `f(State) -> Dom` callbacks. Connection string covers three modes: `:memory:` (transient, perfect for tests + serverless tabs), `file:./app.db` (local file, sandboxed `Documents/` path on iOS / scoped storage on Android), and `libsql://host[:port]?authToken=...` (Turso-style remote). The Rust crate `libsql` already speaks all three modes. A new `Database` resource type alongside `ImageRef` lets users hold a connection in a `RefAny` and run queries from inside layout callbacks; query results post back to the DOM via a callback-driven `RefAny` mutation (so the existing reactive `f(State) -> Dom` model handles cache invalidation).

---

## 1.5. DOM nodes with permission-aware events (architectural addition)

User's request after the initial plan: camera, screen-share, geolocation should be **DOM nodes** rather than imperative API calls, with `EventFilter::*PermissionGranted` / `*PermissionDenied` / `*PermissionRequired` events. This composes cleanly into `f(State) -> Dom`:

```rust
fn layout(state: &State, _info: LayoutCallbackInfo) -> StyledDom {
    Div::new()
        .with_child(if state.show_camera {
            CameraPreview::front()        // NodeType::CameraPreview(CameraSource::FrontFacing)
                .with_callback(On::CameraPermissionDenied, fall_back_to_avatar)
                .with_callback(On::CameraFrame,             record_frame)
                .dom()
        } else { Dom::default() })
        .with_child(if state.location_enabled {
            GeolocationProbe::new()       // NodeType::GeolocationProbe
                .with_callback(On::GeolocationPermissionGranted, save_first_fix)
                .with_callback(On::GeolocationUpdate,             update_marker)
                .dom()
        } else { Dom::default() })
        .dom()
}
```

Why this is better than the imperative `App::request_camera_permission(...)` approach:

* **Derived from the DOM tree, not lifecycle code.** A single pass over the styled DOM at layout time tells the framework which permissions are *currently* needed. The platform layer issues `requestPermission(...)` lazily, only when a permission-bearing node first appears in the tree, and revokes its subscription (stopping the camera, releasing the location listener) the moment that node leaves.
* **Composable.** A re-usable `<CameraButton>` widget can declare "I need camera" via its returned `Dom`; the user composing it gets the permission flow for free.
* **Survives state-driven UI churn.** The reactive pipeline already handles "subscription added/removed" cleanly for animation timers, image-callback re-renders, and IFrames. Permissions become another instance of that same pattern.
* **Maps to W3C primitives perfectly.** `<video autoplay>` already implies "request camera if the source is `getUserMedia()`"; the `Permissions API` exposes the state. The proposed `EventFilter::*Permission*` line up with the W3C `permissionchange` event + the `PermissionStatus.state` enum.

### New NodeTypes (visual or invisible)

| NodeType | Visual? | Permission |
|---|---|---|
| `CameraPreview(CameraSource)` | ✓ | Camera |
| `ScreenCapture(ScreenCaptureSource)` | ✓ | Screen recording |
| `GeolocationProbe` | invisible (zero-size) | Location |
| `BiometricGate` | invisible | Biometric |
| `SensorProbe(SensorKind)` | invisible | Motion (iOS 12+) / none elsewhere |
| `MapTile(MapTileSource)` | ✓ | none (just HTTP) |
| `Pdf(PdfRef)` | ✓ | none (just file IO) |
| `Database(DbHandle)` | invisible | none |

Invisible probe nodes (`GeolocationProbe`, `SensorProbe`, `BiometricGate`) are the "side-effect-only" pattern — they hold subscriptions on behalf of their subtree but render nothing. Layout treats them as zero-size flex items.

### New EventFilter variants

Under `HoverEventFilter` (so they only fire on the bearing node):

```text
// Permissions (per-capability)
CameraPermissionRequired       // permission state = "not yet asked"
CameraPermissionGranted
CameraPermissionDenied
CameraFrame                    // frame ready
ScreenCapturePermissionRequired / Granted / Denied / Frame
GeolocationPermissionRequired / Granted / Denied
GeolocationUpdate              // LocationFix delivered
BiometricGateRequired / Granted / Denied / Locked
SensorPermissionRequired / Granted / Denied
SensorReading                  // (Accelerometer | Gyro | Magnetometer, vec3)
```

### CallbackInfo accessors

```rust
fn get_camera_frame(&self) -> Option<&CameraFrame>
fn get_geolocation_fix(&self) -> Option<&LocationFix>
fn get_accelerometer(&self) -> Option<&Vec3>
fn get_biometric_state(&self) -> BiometricState  // NotEvaluated | Granted | Denied | Locked
fn get_permission_status(&self, capability: Capability) -> PermissionState
```

### PermissionManager

`layout/src/managers/permission.rs`. State machine `NotDetermined → Requested → (Granted | Denied | Restricted)`. Owns a map `Capability → SubscriptionId` + reference-count from active DOM nodes. Diffed at the end of every layout pass (same place `LifecycleEvent`s emit on Mount/Unmount): nodes that just appeared → request their permissions; nodes that just disappeared and held the last reference → release the subscription (stop camera, release CLLocationManager, …).

### Platform backend responsibilities

Each `shell2/<platform>/mod.rs` exposes `inject_permission_result(Capability, PermissionState)`, `inject_camera_frame(CameraFrame)`, `inject_location_fix(LocationFix)`, `inject_sensor_reading(...)`. Symmetric to `inject_native_gesture` from Sprint M.

---

## 2. Per-feature deliverable shape

Each feature lands as a sprint with the following artifacts (mirrors how Sprint M handled gestures):

| Artifact | Where |
|---|---|
| Manager | `layout/src/managers/<feature>.rs` (cross-platform state + accessors) |
| EventFilter variants | `core/src/events.rs` |
| NodeType variant (if visual) | `core/src/dom.rs` + renderer in `layout/src/cpurender.rs` + `webrender/` |
| CallbackInfo accessors | `layout/src/callbacks.rs` |
| Platform injection points | `dll/src/desktop/shell2/<platform>/mod.rs::inject_native_*` |
| Permission flow | `App::request_<feature>(prompt) -> Result<_, PermissionError>` returning a typed error |
| api.json + codegen | `azul-doc autofix add <Type>.<method>` + `codegen all` |
| Sample test | `scripts/mobile/golden/<feature>.png` via `mobile-snapshot.sh` |

---

## 3. Research outputs (this session)

Before implementation starts, we dispatch research agents to inventory APIs + sketch integration per platform. Each agent writes one markdown file under `scripts/research/`. Outputs become the implementation brief for the next session.

| File | Coverage |
|---|---|
| `scripts/research/01_camera_screen_capture.md` | Camera + screen sharing across 5 platforms |
| `scripts/research/02_biometric_auth.md` | FaceID/TouchID/Hello/BiometricPrompt across 5 platforms |
| `scripts/research/03_sensors_gamepad_stylus.md` | Accelerometer + gamepad + wacom across 5 platforms |
| `scripts/research/04_system_integration.md` | File pickers + IME/text input + geolocation across 5 platforms |
| `scripts/research/05_assets_fonts_perms.md` | Font discovery + image loading + media permissions on mobile |
| `scripts/research/06_mvt_pdf.md` | MVT tile decode/render + printpdf integration (both directions) |
| `scripts/research/07_libsql_sqlite.md` | libsql crate (local file / `:memory:` / remote `libsql://`); SQLCipher equivalents; mobile sandboxing of DB files |
| `scripts/research/08_permission_dom_nodes.md` | "Permission-aware DOM" architecture: lifecycle diff after layout, EventFilter::*Permission* surface, invisible probe nodes, W3C `Permissions API` mapping |

Each output should contain, for each platform:
- The native API (framework, class, function entry points)
- Required permission strings / capability declarations (Info.plist keys, AndroidManifest permissions, macOS entitlements)
- A proposed Azul-side integration sketch tied to the architecture seams in §0
- Web/W3C-equivalent primitive (so the future web backend has a target)
- Risks / known gotchas

---

## 4. Implementation ordering (sketch — for the next session)

1. **Asset foundation** (#1) — without correct font/image loading on mobile, every visual feature looks broken. Cheap, no permission UX.
2. **Text input on mobile** (#9) — finishes the IME / `UITextInput` work the gesture sprint started. Highest user-facing value.
3. **File pickers on mobile** (#8) — desktop API parity; matches the user's existing `tfd` muscle memory.
4. **Geolocation + MVT** (#10 + #11) — together they unlock the user's "Google Maps clone" example. Most demo-worthy combination.
5. **Camera** (#2) — `<video>`-shaped node; reuses the ImageRef + texture-update path.
6. **Biometric** (#4) — small surface, big credibility.
7. **Sensors + gamepad + stylus extension** (#5 + #6 + #7) — orthogonal input expansion.
8. **Screen sharing** (#3) — most platform-specific, lowest demand from typical apps.
9. **PDF** (#12) — two flavors (render + export), both standalone.
10. **SQLite / libsql** (#13) — orthogonal to the visual stack; can land any time it's needed. Doubles the "real-app" credibility once it's there.

The "permission-aware DOM" architecture (§1.5) is a *cross-cutting* refactor of features #2, #3, #4, #5, #10 — not a separate sprint. Implementation lands as part of whichever permission-bearing feature is built first (likely geolocation, since the map widget #11 depends on it).

---

## 5. Tracker

The autonomous cron loop is **stopped** as of this document. Implementation starts in the next session, fed by `scripts/research/*.md`.
