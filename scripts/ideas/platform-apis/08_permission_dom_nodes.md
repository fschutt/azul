# 08 — Permission-aware DOM nodes (architectural design)

**Sprint:** SUPER_PLAN_2 §1.5 — cross-cutting refactor that lands with the first permission-bearing feature (likely geolocation + the map widget).
**Author:** research-agent, 2026-05-19.
**Status:** research / architecture — no code yet.

This brief turns the user's framing in `SUPER_PLAN_2.md` §1.5 into an
implementable architecture: (a) the semantic model, (b) the per-capability
state machine, (c) the "permission diff" pass that turns DOM presence into
OS subscriptions, (d) the new `EventFilter` variants, (e) invisible-probe
NodeType design vs alternatives, (f) the `CallbackInfo` surface, (g) the
W3C-equivalent map for the future web backend, (h) privacy risks.

Pattern to mirror, in shorthand:

> *platform backend* → *manager override slot* → *`CallbackInfo` accessor*.
> Modelled on `dll/src/desktop/shell2/<plat>/mod.rs::inject_*` →
> `GestureAndDragManager::inject_native_gesture` →
> `CallbackInfo::get_gesture_drag_manager`. See
> `/Users/fschutt/Development/azul-mobile/layout/src/managers/gesture.rs:435`
> (the `native_gesture` override slot) and
> `/Users/fschutt/Development/azul-mobile/layout/src/managers/text_input.rs:128`
> (a simpler cross-platform state-only manager).

---

## 1. The semantic model

### Option A — Imperative `App::request_camera_permission(...) -> Future<...>`

```rust
let camera = app.request_camera_permission().await?;        // hypothetical
let stream = camera.start_capture(CameraFacing::Front)?;
let frame  = stream.latest_frame();                         // somewhere in layout
```

Matches native idioms: iOS `AVCaptureDevice.requestAccess(for:.video) { granted in ... }` [^ios-avcapture], Android `ActivityCompat.requestPermissions(...)` + `onRequestPermissionsResult` [^android-perm], macOS = iOS, Linux `org.freedesktop.portal.Device`, Windows `MediaCapture.InitializeAsync` [^win-mediacapture].

What it doesn't match: the rest of Azul. The framework is built around `layout(state: &State, _: LayoutCallbackInfo) -> StyledDom` (`architecture.md` §1 and `/Users/fschutt/Development/azul-mobile/scripts/architecture.md:28`). An imperative `request_*` call sits *outside* that pipeline — the caller has to manually start/stop subscriptions as screens change, mirroring the gymnastics React Native users hit with `componentDidMount` / `componentWillUnmount` before hooks landed.

### Option B — DOM-node + lifecycle event (user's preferred)

```rust
fn layout(state: &State, _: LayoutCallbackInfo) -> StyledDom {
    Div::new()
        .with_child(if state.show_camera {
            CameraPreview::front()
                .with_callback(On::CameraPermissionDenied, fall_back_to_avatar)
                .with_callback(On::CameraFrame,             record_frame)
                .dom()
        } else { Dom::default() })
        .dom()
}
```

Why it composes:

* **Derived, not declared.** A single pass over the styled DOM at end-of-layout enumerates what capabilities are *currently* needed. The framework issues `requestPermission(...)` lazily on first appearance and tears down the subscription (stops the camera, releases `CLLocationManager`, etc.) when the last bearing node leaves the tree.
* **Composable.** A reusable `<CameraButton>` widget declares "I need camera" inside its returned `Dom`; users composing it inherit the permission flow for free — same way `<input>` inherits text-input behavior.
* **Survives state-driven UI churn.** Azul already handles "subscription added/removed" cleanly for animation timers, image-render callbacks, and IFrames (`layout/src/managers/iframe.rs` lazy load/unload). Permissions become another instance.
* **Lines up with `RefAny` mutation.** Once a permission is granted and a frame / fix / sample arrives, the platform backend lifts it into the `PermissionManager`. The next callback reads it via a `CallbackInfo` accessor and writes whatever it needs back into application state via `data.downcast_mut::<MyState>()` — the same `RefAny` write path every other callback uses (`core/src/refany.rs:906`).

### Option C — Hybrid

For *one-shot confirmation prompts* (biometric unlock, payment confirm) the DOM-node model is awkward — the prompt is a verb. A `<BiometricGate>` mounted *just* to fire "please prompt now" has bad ergonomics: when to mount, when to unmount, what if the user cancels twice?

**Continuous capabilities are DOM nodes; one-shot verbs are imperative:**

| Capability | Model | Why |
|---|---|---|
| Camera frame | DOM node `CameraPreview` | continuous |
| Screen capture | DOM node `ScreenCapture` | continuous |
| Geolocation | DOM node `GeolocationProbe` | continuous (one-shot via `set_one_shot(true)`) |
| Motion sensors | DOM node `SensorProbe` | continuous |
| Biometric unlock | `app.request_biometric_auth(prompt) -> Future` | one-shot verb |
| Photo-library pick | `app.pick_photo() -> Future` | one-shot verb |

The framework still routes *permission state* for one-shot verbs through the same `PermissionManager` (so an app can pre-check "is biometric available?" without firing the prompt); only the prompt itself is imperative. This matches what `02_biometric_auth.md` already proposes.

### Recommendation: B with C-as-fallback for verbs

`02_biometric_auth.md` already treats biometric as imperative — correct. File pickers (#8) and Android `MediaProjection` [^android-mp] join that camp.

---

## 2. Permission state machine

Per capability:

```
                ┌──────────────────────┐
                │   NotDetermined      │  initial — no prompt yet shown
                └──────────┬───────────┘
                           │ first appearance of capability-bearing node
                           ▼
                ┌──────────────────────┐
                │     Requested        │  OS prompt visible
                └──────────┬───────────┘
                           │ user decides
                           ▼
            ┌──────────────┼──────────────┐
            ▼              ▼              ▼
       ┌────────┐    ┌──────────┐    ┌────────────┐
       │Granted │    │ Denied   │    │ Restricted │  (MDM, parental controls)
       └───┬────┘    └────┬─────┘    └─────┬──────┘
           │              │                │
           │              │     (re-show settings link or fallback UI)
           ▼              ▼                ▼
       Subscription   Fire Denied      Fire Denied
       active         event            event
```

### Encoded in Rust

```rust
#[repr(C, u8)]
pub enum PermissionState {
    NotDetermined,
    Requested,
    Granted { quality: PermissionQuality },
    Denied,
    Restricted,                                          // MDM, parental, kiosk
    EphemeralGranted { until_app_close: bool },          // iOS 14+ "Allow Once", Android 11+ one-time
}

#[repr(C, u8)]
pub enum PermissionQuality {
    Full,     // exact location, full photo library
    Reduced,  // approximate location, limited photo selection
}
```

### Platform mappings (collapse on the way in)

**iOS** — `CLAuthorizationStatus` has 5 states [^ios-clauth]; `PHAuthorizationStatus` adds `.limited` in iOS 14 [^ios-phauth]; `AVAuthorizationStatus` has 3 [^ios-avauth].

| iOS value | Azul `PermissionState` |
|---|---|
| `.notDetermined` | `NotDetermined` |
| `.restricted` | `Restricted` |
| `.denied` | `Denied` |
| `.authorized` / `.authorizedAlways` | `Granted { quality: Full }` |
| `.authorizedWhenInUse` (location) | `Granted { quality: Full }` + `usage_hint: WhenInUse` on accessor |
| `.limited` (photo library 14+) | `Granted { quality: Reduced }` |
| user toggled "Allow Once" (14+) | `EphemeralGranted { until_app_close: false }` |

**Android pre-API-29 vs post.** Pre-API-23: granted at install time, no runtime prompt — query via `PackageManager.getPackageInfo(..., GET_PERMISSIONS)` [^android-prepm]. API-23+: runtime prompt with `Granted`/`Denied`; `NotDetermined` distinguished via `shouldShowRequestPermissionRationale(...)` + `checkSelfPermission(...) == DENIED` [^android-rationale]. API-29+: `ACCESS_BACKGROUND_LOCATION` is a *separate* permission, gated behind `ACCESS_FINE_LOCATION` + manual "Allow all the time" in Settings [^android-bg-location]. API-30+: one-time permission for camera/mic/location is granted only for the session → `EphemeralGranted { until_app_close: true }` [^android-onetime]. API-33+ (Tiramisu): `READ_EXTERNAL_STORAGE` splits into `READ_MEDIA_IMAGES`/`_VIDEO`/`_AUDIO` [^android-tiramisu]. API-34+: "selected photos" partial access (iOS 14 analog) [^android-udc].

**Windows — capability vs runtime prompt.** Two separate gates: appx manifest `<Capabilities>` declaration (compile-time) [^win-cap] *and* the runtime prompt via `DeviceAccessInformation` exposing `DeviceAccessStatus ∈ {Allowed, DeniedByUser, DeniedBySystem, Unspecified}` [^win-deviceaccess]. `DeniedBySystem` = MDM/Group-Policy → `Restricted`; `DeniedByUser` → `Denied`; `Unspecified` → `NotDetermined`. For Win32 (non-UWP), the model is weaker — most APIs succeed-or-fail, but camera/mic/location honor the system privacy switches via `AppCapability.CheckAccessAsync()` [^win-appcap].

**Linux.** No standard model. xdg-desktop-portal (Flatpak/Snap) exposes `org.freedesktop.portal.{Camera,Location,ScreenCast}` [^linux-portal]. For non-sandboxed Linux, default to `Granted` with docs noting the caveat.

**macOS.** Same as iOS for camera/mic/location. Grant keyed to bundle ID; revoke via System Settings → Privacy & Security [^macos-priv].

**Web (W3C).** `navigator.permissions.query({name: "camera"})` returns `PermissionStatus.state ∈ {granted, denied, prompt}` (= our `NotDetermined`) [^w3c-perm]. No `Restricted`; collapse on the JS bridge.

### Capability enum

```rust
#[repr(C, u8)]
pub enum Capability {
    Camera { facing: CameraFacing },                     // front / back / external
    Microphone,
    ScreenCapture { kind: ScreenCaptureKind },           // entire / window / tab
    Geolocation { accuracy: GeoAccuracy },               // precise / approximate
    Biometric,
    Motion { kinds: SensorKindMask },                    // accel | gyro | mag
    PhotoLibrary { mode: PhotoLibraryMode },             // read / write / picker
    Contacts, Calendars, Reminders,
    Notifications,
    Bluetooth, BluetoothBackground,
    NearbyWifi,
    AppTrackingTransparency,                             // iOS 14.5+
}
```

iOS 14+ "precise vs approximate" location is a *user choice* delivered with the grant — modeled as a property of the capability, not a separate capability. Same for `mode` on `PhotoLibrary`.

---

## 3. The "permission diff" pass

After every layout pass, the `PermissionManager` walks the new styled DOM:

```text
needed_now: Set<Capability>      = nodes with permission-bearing NodeType
needed_before: Set<Capability>   = saved from last frame
to_subscribe   = needed_now    - needed_before    → PermissionManager::subscribe(cap)
to_release     = needed_before - needed_now       → PermissionManager::release(cap)
```

### Reference counting

Implicit in the DOM node count, not maintained separately. Two `<CameraPreview>` nodes (front + back) → 2 subscribers for `Camera`; remove one → 1; remove both → released.

Implementation: `HashMap<Capability, RefCount>` recomputed from scratch each frame by scanning `StyledDom::node_data` for permission-bearing NodeTypes. The diff is `(new_map - old_map)` for subscribe and vice versa for release. O(n) — same complexity as the existing reconciliation walk (`core/src/dom.rs:3520`). Folded into the same pass.

### Interaction with `LifecycleEvent::Mount/Unmount`

The framework already emits per-node Mount/Unmount via `detect_lifecycle_events_with_reconciliation` (`core/src/events.rs:1482`). We *could* piggyback: on `Mount` of a `<CameraPreview>`, subscribe; on `Unmount`, release. But two issues:

1. **Mount/Unmount fires *per node*, not per *capability*.** Two `<CameraPreview>` nodes both Mount → would subscribe twice. Fixable with a ref-count map, but at that point the per-node events reduce to "increment/decrement a counter" — same work as a global rescan.
2. **Mount/Unmount doesn't model `Update`.** If a `<CameraPreview facing=front>` becomes `<CameraPreview facing=back>`, the W3C `Update` event fires (not Mount/Unmount), but the capability *did* change — we need to release front and acquire back without re-prompting.

**Recommendation: separate permission-diff pass**, run immediately *after* lifecycle detection (reads the same `StyledDom` and reconciled `node_data`). Produces:

```rust
pub enum PermissionDiffEvent {
    Subscribe   { capability: Capability, node_id: DomNodeId },
    Release     { capability: Capability },
    Reconfigure { capability: Capability, new_params: CapabilityParams },
}
```

`Reconfigure` covers the camera-facing-change case without release+subscribe (which would tear down the session and re-prompt).

### Placement in the manager set

The `PermissionManager` lives in `/Users/fschutt/Development/azul-mobile/layout/src/managers/permission.rs` (new). However: a camera subscription is *per-process*, not per-window — so the actual storage lives on `App` (`dll/src/desktop/app.rs`) with `LayoutWindow` holding a reference. See §8.7. `TODO: verify` against the existing `ImageCache` shared-storage pattern (`core/src/resources.rs:623`).

---

## 4. New `EventFilter` variants

Events tied to a *specific bearing node* go on `HoverEventFilter` (camera frames arrive *for* a specific `<CameraPreview>` — multiple cameras in one screen get separate streams). Window-level signals (sensors not tied to a node) go on `WindowEventFilter`. Application-wide signals (permission revoked while backgrounded) go on the existing `ApplicationEventFilter` (`core/src/events.rs:2022`).

```rust
// HoverEventFilter additions — one node carries the capability

// Camera
CameraPermissionRequired,    // OS prompt is about to show
CameraPermissionGranted,
CameraPermissionDenied,
CameraPermissionRestricted,  // MDM / parental controls
CameraFrame,                 // payload in CallbackInfo
CameraError,                 // device dropped / lost / busy

// Screen capture (per <ScreenCapture> node)
ScreenCapturePermissionRequired, ScreenCapturePermissionGranted,
ScreenCapturePermissionDenied,
ScreenCaptureFrame,
ScreenCaptureEnded,          // user clicked OS "stop sharing" indicator

// Geolocation (per <GeolocationProbe>)
GeolocationPermissionRequired, GeolocationPermissionGranted,
GeolocationPermissionDenied,   GeolocationPermissionRestricted,
GeolocationFix,                // LocationFix in CallbackInfo
GeolocationError,              // signal lost, timeout, no provider

// Sensor (per <SensorProbe>)
SensorPermissionRequired,      // iOS 13+ DeviceMotion / DeviceOrientation
SensorPermissionGranted,
SensorPermissionDenied,
SensorReading,                 // (kind, vec3) in CallbackInfo
```

```rust
// WindowEventFilter additions — window-global, no bearing node

WindowSensorReading,         // SensorKind + vec3 (no SensorProbe present)

// Gamepad — already mooted in SUPER_PLAN_2 §1 feature 6
GamepadButtonPressed, GamepadButtonReleased,
GamepadAxisMove, GamepadConnected, GamepadDisconnected,
```

```rust
// ApplicationEventFilter additions

PermissionRevoked,   // Capability in CallbackInfo — user revoked via Settings while backgrounded
PermissionRestored,  // user re-enabled via Settings while backgrounded
```

`FocusEventFilter` is *not* used: permissions fire on the node regardless of focus state.

---

## 5. Invisible-probe `NodeType` design

`GeolocationProbe`, `SensorProbe` are side-effect-only nodes. (`BiometricGate` collapses into Option C from §1 — dropped.) Three modeling alternatives:

### Alt 1 — Dedicated `NodeType` variants

```rust
NodeType::GeolocationProbe(GeolocationProbeConfig),
NodeType::SensorProbe(SensorKindMask),
```

**Pros:** matches the rest of `NodeType` design (~140 variants today, see `core/src/dom.rs:239`). The diff pass pattern-matches exhaustively — adding a new probe *forces* a compile-time touch of the permission diff. Renderer is trivial: zero-size in layout, skipped in display-list generation (same pattern `Head` / `Meta` / `Title` / `Style` already use).

**Cons:** another touchpoint across parser, `Display`, `Hash`, codegen — and ×35 binding languages.

### Alt 2 — `NodeType::Div` + `attribute = ProbeKind::Geolocation`

Add a tagged attribute on `NodeDataExt`:

```rust
pub enum AttributeType {
    IdOrClass(IdOrClass),
    Dataset(...),
    ProbeKind(ProbeKind),           // new
    ...
}
```

**Pros:** doesn't bloat `NodeType`. Probes are *attached* to nodes, not standalone.

**Cons:** loses the "this node is *only* a probe" guarantee — does it lay out as a div? Show in tab order? Have children? Diff has to scan *every* node for probe attributes. Less type-safe: `Dom::create_div().with_probe(...)` vs `Dom::create_geolocation_probe()`.

### Alt 3 — `LifecycleCallback` on any node

User registers a Mount-keyed callback that calls `info.subscribe_capability(...)`. **Pros:** no new types. Lightest weight. **Cons:** subscription is opaque to the framework — diff can't see it by inspecting the DOM, so the OS prompt fires *after* the first frame (visible lag). No protection against forgetting the matching Unmount. Doesn't fit Azul's "single layout-time scan tells us everything" pattern.

### Recommendation: Alt 1

1. **Forcing function.** New probe → compile-time touch of permission diff and renderer skip-list.
2. **Synchronous semantics.** Diff at end-of-layout *knows* what permissions are needed before the first paint — OS prompt can fire in parallel with paint, not after.
3. **Type-safe user API.** `Dom::create_geolocation_probe()` over a "div with a magic attribute".
4. **Layout is trivial.** `Head` / `Meta` / `Title` / `Style` are already zero-render — same treatment.

### Concrete additions to `NodeType`

```rust
// Visual probe nodes (replace a real surface region)
NodeType::CameraPreview(CameraSource),             // CameraSource = facing + resolution hint
NodeType::ScreenCapture(ScreenCaptureSource),

// Invisible probe nodes (zero-size, skipped in display list)
NodeType::GeolocationProbe(GeolocationProbeConfig),
NodeType::SensorProbe(SensorKindMask),

// Side-effect content nodes (visual, no permission)
NodeType::MapTile(MapTileSource),                  // tile URL / lat,lon / zoom
NodeType::Pdf(PdfRef),                              // printpdf integration
```

`CameraSource`, `ScreenCaptureSource`, `GeolocationProbeConfig`, `MapTileSource` become small `#[repr(C)]` types alongside `ImageRef`. `CameraPreview` renderer reuses the existing `Image(ImageRef)` path — same texture handle updated per frame.

---

## 6. `CallbackInfo` surface

Accessor pattern follows `CallbackInfo::get_gesture_drag_manager()` (`layout/src/callbacks.rs` — already wired). Read-side and write-side accessors below.

### Read-side

```rust
impl CallbackInfo {
    // Latest frame/fix/sample, scoped to the node that received the event.
    pub fn get_camera_frame(&self) -> Option<&CameraFrame>;
    pub fn get_screen_capture_frame(&self) -> Option<&ScreenCaptureFrame>;
    pub fn get_geolocation_fix(&self) -> Option<&LocationFix>;
    pub fn get_sensor_reading(&self, kind: SensorKind) -> Option<&SensorReading>;

    // Cross-capability state probe.
    pub fn get_permission_status(&self, capability: Capability) -> PermissionState;

    // Aggregate (debug overlays, settings page).
    pub fn list_active_permissions(&self) -> &[ActivePermission];
}
```

`CameraFrame`, `ScreenCaptureFrame`, `LocationFix`, `SensorReading` are `#[repr(C)]` POD-ish types in `core/src/resources.rs`. Frame types carry a `texture_handle` so the renderer can blit without CPU copy.

### Write-side

```rust
impl CallbackInfo {
    // Open OS settings panel deep-linked to the offending capability
    // for the "you previously denied → please go to Settings" path.
    pub fn open_app_settings(&mut self, capability: Option<Capability>);

    // For imperative one-shot biometric (option C):
    pub fn request_biometric_auth(&mut self, prompt: AzString, fallback: BiometricFallback);

    // Reconfigure a capability without releasing it (e.g. switch camera facing).
    pub fn reconfigure_capability(&mut self, capability: Capability, params: CapabilityParams);

    // Force the prompt after a user gesture (see §8.3).
    pub fn request_capability_now(&mut self, capability: Capability);
}
```

`open_app_settings` lowers to `CallbackChange::OpenAppSettings { ... }` (new variant in `layout/src/callbacks.rs::CallbackChange` — see `/Users/fschutt/Development/azul-mobile/layout/src/callbacks.rs:167`), which the platform backend translates:

| Platform | Deep link |
|---|---|
| iOS | `UIApplication.shared.open(URL(string: UIApplication.openSettingsURLString)!)` [^ios-opensettings] |
| Android | `Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}"))` [^android-opensettings] |
| macOS | `x-apple.systempreferences:com.apple.preference.security?Privacy_Camera` (or `Privacy_Location` / `Privacy_Microphone`) via `NSWorkspace.shared.open(URL)` [^macos-opensettings] |
| Windows | `ms-settings:privacy-camera` (or `ms-settings:privacy-location` / `ms-settings:privacy-microphone`) via `LaunchUriAsync` [^win-opensettings] |
| Linux | **No standard.** GNOME: `gnome-control-center privacy`. KDE: `systemsettings5 kcm_componentchooser`. XDG-portal: `org.freedesktop.portal.OpenURI`. Default: log a warning. |

`Option<Capability>`: most platforms deep-link to the specific permission page; Android pre-API-30 can only open the app's own settings page (user navigates to "Permissions" manually).

---

## 7. W3C-equivalent mapping (for the future web backend)

| Azul `NodeType` | W3C primitive | Azul `EventFilter` | W3C event |
|---|---|---|---|
| `CameraPreview(CameraSource)` | `<video>` + `navigator.mediaDevices.getUserMedia({video: {facingMode: "user"\|"environment"}})` [^w3c-getusermedia] | `CameraFrame` | `<video>` `onloadedmetadata` / `requestVideoFrameCallback` |
| `ScreenCapture(...)` | `<video>` + `navigator.mediaDevices.getDisplayMedia(...)` [^w3c-getdisplaymedia] | `ScreenCaptureFrame` | same |
| `GeolocationProbe` | `navigator.geolocation.watchPosition(...)` [^w3c-geo] | `GeolocationFix` | success callback of `watchPosition` |
| `SensorProbe(Accelerometer\|Gyro\|Magnetometer)` | `Accelerometer` / `Gyroscope` / `Magnetometer` Generic Sensor API + iOS 13+ `DeviceMotionEvent.requestPermission()` [^w3c-genericsensor] [^ios-devicemotion] | `SensorReading` | `reading` event on the sensor |
| imperative `request_biometric_auth(...)` | `navigator.credentials.get({publicKey: {...userVerification: "required"}})` [^w3c-webauthn] | `BiometricResult` | promise resolution |
| `PermissionState` query | `navigator.permissions.query({name: ...})` [^w3c-perm] | n/a (read-only) | `permissionchange` event |
| `open_app_settings(...)` | **No web equivalent.** Web backend no-ops (PWA can suggest "Site settings"). | n/a | n/a |

`MapTile` and `Pdf` have no W3C primitive — web backend implements them with `<canvas>` + JS tile renderer / `<embed type="application/pdf">`.

Mapping gaps:

* W3C `state` has 3 values (`granted`/`denied`/`prompt`). No `Restricted` → collapse `Restricted → Denied` on the JS bridge.
* W3C has no `EphemeralGranted`. Modern browsers re-prompt each session for `getUserMedia({video: true})` — closer to `EphemeralGranted { until_app_close: true }`. The web bridge reports `EphemeralGranted` for camera/mic when it sees `granted`.
* iOS Safari 13+ requires `DeviceMotionEvent.requestPermission()` from a user gesture (button click) [^ios-devicemotion]. The Azul web backend has to translate `SensorProbe`-on-mount into "show a prompt-yourself UI with a real button" — can't auto-call. Same gesture-gate risk discussed in §8.3.

---

## 8. Risks

### 8.1 State can change out from under the running app

User revokes camera permission in System Settings while the app is backgrounded. When it returns to foreground, the next OS API call returns `denied` without warning.

Solution: on `applicationDidBecomeActive:` (iOS) [^ios-active] / `Activity.onResume()` (Android) [^android-onresume] / `applicationDidBecomeActive:` (macOS) / `WM_ACTIVATE` (Windows), the platform backend calls `PermissionManager::recheck_all()`, which polls every subscribed capability and fires `EventFilter::PermissionRevoked` / `PermissionRestored` if OS state diverged from the cached value.

Linux: no standard "activity resume" hook outside Flatpak portals. Poll on a slow timer (every 30s) as fallback.

### 8.2 "Ask Every Time" / ephemeral grants

* iOS 14+ location: user can pick "Allow Once". Grant lasts until app is backgrounded for >few minutes, then reverts to `NotDetermined` [^ios-allow-once].
* Android 11+: one-time permission for camera/mic/location. Granted only for this activity; revoked when activity finishes [^android-onetime].

Both map to `EphemeralGranted { until_app_close: bool }`. Framework should *not* re-prompt within the same session — but the state is observable via `get_permission_status(...)` so the app can show "you'll be asked again next time" copy.

### 8.3 Auto-prompting on first DOM appearance is surprising

If a `<CameraPreview>` appears as a side-effect of *navigation* (routing to a new screen), the OS prompt fires automatically. Two risks:

1. **App Store rejection.** Apple's review guidelines require a *user-initiated context* for camera prompts [^apple-review]. Auto-prompting when the user just opened the app and the home screen contains a `<CameraPreview>` will get rejected.
2. **iOS Safari + DeviceMotion + iOS 13+:** the W3C bridge *forbids* non-gesture-initiated `requestPermission` calls [^ios-devicemotion].

Mitigation — a per-capability flag and gesture log:

```rust
impl PermissionManager {
    pub fn requires_user_gesture(capability: Capability) -> bool;
    // default = true for camera, mic, screen capture, location, motion
}
```

The diff pass uses this flag: if the capability requires a gesture and no recent gesture is in the event log, the diff *defers* the subscription and fires `EventFilter::*PermissionRequired` instead:

```rust
match (requires_gesture, recent_gesture_within(Duration::from_secs(2))) {
    (false, _)    => subscribe_now(),
    (true,  true) => subscribe_now(),
    (true,  false) => fire(EventFilter::CameraPermissionRequired),
                      // App shows "Tap to enable camera" button;
                      // its onClick callback then calls
                      // info.request_capability_now(Capability::Camera)
}
```

`request_capability_now` forces the prompt (we *just* had a gesture). Recommended user-code pattern:

```rust
fn render_camera_screen(state: &State) -> Dom {
    if state.camera_status == PermissionState::NotDetermined {
        // Haven't asked yet — show a prompt-yourself button.
        // The CameraPreview node is *not* in the tree, so no auto-prompt.
        Dom::create_button()
            .with_text("Enable Camera")
            .with_callback(On::Click, request_camera)
    } else {
        match state.camera_status {
            PermissionState::Granted { .. } => CameraPreview::front().dom(),
            PermissionState::Denied | PermissionState::Restricted =>
                Dom::create_text("Camera unavailable.")
                    .with_callback(On::Click, open_settings),
            _ => unreachable!(),
        }
    }
}

extern "C" fn request_camera(_: &mut RefAny, info: &mut CallbackInfo) -> Update {
    info.request_capability_now(Capability::Camera { facing: CameraFacing::Front });
    Update::DoNothing
}
```

Identical to how W3C apps gate `getUserMedia` behind a button click. Documenting this as "the recommended prompt pattern" is mandatory.

### 8.4 `Info.plist` / `AndroidManifest.xml` / appx manifest declarations

Camera/mic/location/motion all require a *declaration* at build-time in the platform manifest. iOS additionally requires a *reason string* (`NSCameraUsageDescription`, `NSLocationWhenInUseUsageDescription`, etc.) — App Store *rejects* apps that prompt without one [^ios-plist].

The framework can't enforce this from Rust at run-time. But the build tooling can:

1. The codegen (`azul-doc autofix add`) emits manifest fragments per capability.
2. The `cargo-mobile2` / `xcodegen` / Android Gradle plugin integration merges them into the user's manifest.
3. If the user shipped a binary using `Capability::Camera` but `Info.plist` lacks `NSCameraUsageDescription`, iOS *crashes the app immediately* on first prompt. Document this prominently.

### 8.5 Privacy indicators (the green dot)

iOS 14+ shows a green dot when camera is in use, orange for mic. Android 12+ same [^ios-greendot] [^android-mic-cam-indicator]. Framework can't suppress them (OS-level). Users see them whenever a `<CameraPreview>` / mic probe is mounted. App-Store-targeting apps need to release subscriptions promptly when the preview is off-screen — which the diff does correctly on Unmount.

### 8.6 Multi-window apps (desktop)

Camera subscription is *per-process*, not per-window. Window A + Window B both mount `<CameraPreview>` → both get frames from one capture session; framework multiplexes. Last release closes the session.

This requires `PermissionManager` to live on `App`, not on each `LayoutWindow`. Revises §3's placement: `App::permission_manager` (in `dll/src/desktop/app.rs`), with `LayoutWindow` holding a reference/index. `TODO: verify` against how `ImageCache` is shared today (`/Users/fschutt/Development/azul-mobile/core/src/resources.rs:623`).

### 8.7 Diff timing — what if a one-frame "blip" subscribes-then-releases?

Layout returns `<CameraPreview>` for one frame, then doesn't on the next:

1. Frame N: subscribe → OS prompt fires.
2. Frame N+1: release → camera teardown begins.
3. User clicks "Allow" → grant fires for a now-released capability → confused state.

Mitigation: debounce. `PermissionManager` keeps a `grace_period: Duration` (default 500ms) on `release`. A re-subscribe within the grace period cancels the in-flight teardown and preserves the session. Same pattern `IFrameManager` already uses for lazy-load thrashing.

### 8.8 Web backend partitioned storage

On the web, `getUserMedia()` is gated per-origin + per-session. Browser keeps the grant for the lifetime of the tab but drops on hard reload. Model as `EphemeralGranted { until_app_close: true }` on the web bridge — no diff-pass changes needed.

---

## 9. Open questions

* **Should `Microphone` be a separate capability or part of `Camera`?** iOS treats them as distinct (`AVAuthorizationStatus(for: .audio)` vs `for: .video`). Model separately. A `<MicrophoneProbe>` NodeType might mirror `<SensorProbe>` for audio capture without a visual surface.
* **Should `Granted { Reduced }` be a separate state (`LimitedlyGranted`)?** Pro: one enum-value-one-meaning. Con: most callers want `if granted { ... }`. Keep `Granted { quality }` + `is_granted()` accessor returning true for both Full and Reduced.
* **Background subscription** (location updates while backgrounded) is a separate permission on iOS (`alwaysAuthorized`) and Android (`ACCESS_BACKGROUND_LOCATION`). Out of scope here; track as `Capability::BackgroundGeolocation`.
* **Web Bluetooth / Web USB / Web MIDI** — same gesture-gate pattern. Defer until demand.

---

## 10. Files this design will touch

| File | Change |
|---|---|
| `core/src/dom.rs` | Add `NodeType::CameraPreview`, `ScreenCapture`, `GeolocationProbe`, `SensorProbe`, `MapTile`, `Pdf`. Update Hash/Ord/Display. |
| `core/src/events.rs` | Add 20+ `HoverEventFilter` variants + a few `WindowEventFilter` + 2 `ApplicationEventFilter` variants. Add `PermissionState`, `Capability`, `LocationFix`, `CameraFrame`, `SensorReading` types. |
| `layout/src/managers/permission.rs` (new) | The `PermissionManager` with subscribe/release/recheck/gesture-gate. |
| `layout/src/window.rs` | Add `permission_manager_handle` field to `LayoutWindow`. |
| `dll/src/desktop/app.rs` | Add `App::permission_manager` (shared across windows). |
| `dll/src/desktop/shell2/<plat>/mod.rs` (×5) | Add `inject_permission_result`, `inject_camera_frame`, `inject_location_fix`, `inject_sensor_reading`, `open_app_settings_deep_link`. |
| `layout/src/callbacks.rs` | Add `CallbackChange::OpenAppSettings`, `RequestCapability`, `ReconfigureCapability`. Add `CallbackInfo::get_camera_frame`, `get_geolocation_fix`, `get_sensor_reading`, `get_permission_status`, `list_active_permissions`, `request_capability_now`. |
| `layout/src/cpurender.rs` + `webrender/...` | `CameraPreview` and `ScreenCapture` renderers (reuse `ImageRef` / texture-update). `MapTile` and `Pdf` per `06_mvt_pdf.md`. |
| `api.json` / `doc/` | `azul-doc autofix add` for each new type; `codegen all` regenerates the 35 bindings. |
| `scripts/mobile/golden/permission_*.png` | Snapshot tests per platform. |

Estimated LOC: ~2,000–3,000 new lines. Per-platform `inject_*` skeletons ~50–100 LOC each, mirroring `inject_native_gesture` (see `/Users/fschutt/Development/azul-mobile/dll/src/desktop/shell2/ios/mod.rs:370` and `/Users/fschutt/Development/azul-mobile/dll/src/desktop/shell2/android/mod.rs:768`).

---

## Footnotes / references

`TODO: verify` notes throughout this brief flag spots where behavior should be confirmed against the platform SDK before landing code.

[^ios-avcapture]: [Apple Developer — AVCaptureDevice requestAccess(for:completionHandler:)](https://developer.apple.com/documentation/avfoundation/avcapturedevice/1624584-requestaccess).
[^android-perm]: [Android Developers — Request app permissions](https://developer.android.com/training/permissions/requesting).
[^win-mediacapture]: [Microsoft Learn — MediaCapture.InitializeAsync](https://learn.microsoft.com/en-us/uwp/api/windows.media.capture.mediacapture.initializeasync).
[^android-mp]: [Android Developers — MediaProjection](https://developer.android.com/reference/android/media/projection/MediaProjection).
[^ios-clauth]: [Apple Developer — CLAuthorizationStatus](https://developer.apple.com/documentation/corelocation/clauthorizationstatus).
[^ios-phauth]: [Apple Developer — PHAuthorizationStatus](https://developer.apple.com/documentation/photokit/phauthorizationstatus).
[^ios-avauth]: [Apple Developer — AVAuthorizationStatus](https://developer.apple.com/documentation/avfoundation/avauthorizationstatus).
[^android-prepm]: [Android Developers — PackageManager.PERMISSION_GRANTED](https://developer.android.com/reference/android/content/pm/PackageManager#PERMISSION_GRANTED).
[^android-rationale]: [Android Developers — shouldShowRequestPermissionRationale](https://developer.android.com/reference/androidx/core/app/ActivityCompat#shouldShowRequestPermissionRationale(android.app.Activity,java.lang.String)).
[^android-bg-location]: [Android Developers — Request background location](https://developer.android.com/training/location/permissions#background).
[^android-onetime]: [Android Developers — Permissions overview § One-time permission](https://developer.android.com/guide/topics/permissions/overview#one-time).
[^android-tiramisu]: [Android Developers — Behavior changes in Tiramisu](https://developer.android.com/about/versions/13/behavior-changes-13#granular-media-permissions).
[^android-udc]: [Android Developers — Selected photos access (UpsideDownCake)](https://developer.android.com/about/versions/14/changes/partial-photo-video-access).
[^win-cap]: [Microsoft Learn — App capability declarations](https://learn.microsoft.com/en-us/windows/uwp/packaging/app-capability-declarations).
[^win-deviceaccess]: [Microsoft Learn — DeviceAccessStatus](https://learn.microsoft.com/en-us/uwp/api/windows.devices.enumeration.deviceaccessstatus).
[^win-appcap]: [Microsoft Learn — AppCapability.CheckAccessAsync](https://learn.microsoft.com/en-us/uwp/api/windows.security.authorization.appcapabilityaccess.appcapability.checkaccessasync).
[^linux-portal]: [Flatpak — XDG Desktop Portal documentation](https://flatpak.github.io/xdg-desktop-portal/).
[^macos-priv]: [Apple Developer — Protecting the user's privacy](https://developer.apple.com/documentation/security/protecting_the_user_s_privacy).
[^w3c-perm]: [W3C Permissions API spec](https://w3c.github.io/permissions/).
[^ios-opensettings]: [Apple Developer — UIApplication.openSettingsURLString](https://developer.apple.com/documentation/uikit/uiapplication/1648162-opensettingsurlstring).
[^android-opensettings]: [Android Developers — Settings.ACTION_APPLICATION_DETAILS_SETTINGS](https://developer.android.com/reference/android/provider/Settings#ACTION_APPLICATION_DETAILS_SETTINGS).
[^macos-opensettings]: [Apple — System Settings URL schemes](https://developer.apple.com/library/archive/featuredarticles/iPhoneURLScheme_Reference/Articles/SystemPreferencesURLSchemes.html) — `TODO: verify` macOS Ventura+ scheme names (System Preferences → System Settings renamed several panes).
[^win-opensettings]: [Microsoft Learn — Launch the Settings app](https://learn.microsoft.com/en-us/windows/uwp/launch-resume/launch-settings-app).
[^w3c-getusermedia]: [W3C Media Capture and Streams spec § getUserMedia](https://www.w3.org/TR/mediacapture-streams/#dom-mediadevices-getusermedia).
[^w3c-getdisplaymedia]: [W3C Screen Capture spec § getDisplayMedia](https://www.w3.org/TR/screen-capture/).
[^w3c-geo]: [W3C Geolocation API](https://www.w3.org/TR/geolocation/).
[^w3c-genericsensor]: [W3C Generic Sensor API](https://www.w3.org/TR/generic-sensor/).
[^ios-devicemotion]: [W3C DeviceOrientation Event spec](https://www.w3.org/TR/orientation-event/) — Safari 13+ requires `DeviceMotionEvent.requestPermission()` from a user gesture; `TODO: verify` precise Apple ref (Safari release notes).
[^w3c-webauthn]: [W3C Web Authentication Level 2](https://www.w3.org/TR/webauthn-2/).
[^ios-active]: [Apple Developer — applicationDidBecomeActive(_:)](https://developer.apple.com/documentation/uikit/uiapplicationdelegate/1623003-applicationdidbecomeactive).
[^android-onresume]: [Android Developers — Activity.onResume()](https://developer.android.com/reference/android/app/Activity#onResume()).
[^ios-allow-once]: [Apple — iOS 14 Location Privacy](https://developer.apple.com/news/?id=8tjnxc28).
[^apple-review]: [Apple App Review Guidelines § 5.1.1](https://developer.apple.com/app-store/review/guidelines/#privacy) — `TODO: verify` exact section number (Apple renumbers periodically).
[^ios-plist]: [Apple Developer — Requesting access to protected resources](https://developer.apple.com/documentation/uikit/protecting_the_user_s_privacy/requesting_access_to_protected_resources).
[^ios-greendot]: [Apple — iOS 14 privacy indicators](https://support.apple.com/guide/iphone/control-access-to-information-in-apps-iph251e92810/ios).
[^android-mic-cam-indicator]: [Android Developers — Privacy indicators](https://developer.android.com/about/versions/12/behavior-changes-12#mic-camera-indicators).

---

End of brief.
