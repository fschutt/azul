# SUPER_PLAN_2 — Mobile-era integrations

**Status:** planning. SUPER_PLAN.md (Sprints A–N) is closed; iOS + Android backends compile and the gesture / event surface is unified across 5 platforms. This document is the *next* day's work: the broad feature integrations a real-world mobile app expects but a desktop-only UI framework leaves to the user.

**Targets (5 platforms):** iOS, Android, macOS, Linux, Windows. Web/WASM is its own world; each feature's design must be web-shape-compatible (i.e., when we add the web backend, the same DOM node / EventFilter / CallbackInfo accessor should map cleanly to a W3C primitive — `<video>`, `Permissions API`, `WebAuthn`, etc.).

**Scope rule, again:** Azul is a *superset of every platform*. A feature exposes the highest-quality native implementation per platform with a single Azul-side API; platforms with weaker native support get a fallback (or, if no fallback is possible, the call returns a typed `Unsupported` error rather than failing silently).

---

## 0. Crates the user owns (free to patch upstream)

- **`rust-fontconfig`** — local path override at `/Users/fschutt/Development/rust-fontconfig/`. The mobile font-discovery gap (`research/05` — *zero system fonts found on iOS/Android today*) gets fixed here, not by swapping to `fontique`. Add `OperatingSystem::IOS` + `Android` variants; iOS arm = `core-text::CTFontManagerCopyAvailableFontURLs`, Android arm = walk `/system/fonts/` + parse `/system/etc/fonts.xml`.
- **`printpdf`** — the user's own crate. Already does `azul-layout` → `displaylist` → PDF on the *emission* side. For the *render* side, **`printpdf` can parse a PDF file and emit SVG-as-string** — we then feed that SVG through the existing `azul_layout::svg` renderer per page. **No `pdfium-render` / `mupdf` / external rasteriser needed** (overrides `research/06`'s Direction A choice).

Both crates ship as-is from the user's source-of-truth; no upstream PRs blocking us.

---

## 0.5. Dependency-isolation rule for new features

**Hard rule:** `azul-css`, `azul-core`, and `azul-layout` accept **no new dependencies** for any feature in this plan. Every camera / screen-capture / biometric / sensor / map / PDF / SQLite / location integration lives as a submodule inside `dll/` — concretely `dll/src/desktop/extra/<feature>/`. The reason: those three crates are the "pure" layout core that web-backend (wasm32), wasm-only consumers, and embedded users depend on; adding tokio / objc / WinRT / pipewire / libsql there would balloon the closure for every consumer.

The user-facing API for each feature is **re-exported** from `dll`:

```text
azul_dll::extra::camera::CameraPreview / CameraManager / ...
azul_dll::extra::screencap::ScreenCapture / ...
azul_dll::extra::biometric::request_biometric_auth / ...
azul_dll::extra::geolocation::GeolocationProbe / ...
azul_dll::extra::map::MapTile / ...
azul_dll::extra::pdf::Pdf / App::export_pdf / ...
azul_dll::extra::sqlite::Database / ...
azul_dll::extra::sensors::SensorProbe / ...
```

Codegen / api.json picks them up the same way it picks up `azul_dll::desktop::dialogs::*` today.

Where this conflicts with the research briefs (which placed `CameraManager` etc. under `layout/src/managers/`), **the dll-submodule placement wins**. Only the truly cross-platform-state-only `PermissionManager` stays in `layout/src/managers/` because it has no platform deps and the layout pass needs to walk it.

---

## 0.6. PDF render path — printpdf → SVG → existing SVG renderer

`research/06_mvt_pdf.md` recommended `pdfium-render` for inline PDF display. Override: **use printpdf's "PDF parse → SVG-per-page" path.** The flow becomes:

```text
PdfRef::from_bytes(pdf_bytes)
   ↓ printpdf::parse_pdf(pdf_bytes)
   ↓ for each page: printpdf::page_to_svg() -> String
   ↓ feed SVG string into azul_layout::svg::Svg::parse(...)
   ↓ render via existing CPU + GPU SVG renderer (already in azul-layout)
   ↓ caches per (page, dpi, viewport) into ImageRef
NodeType::Pdf(PdfRef { page: u32, scale: f32, ... })
```

Wins: zero new C / C++ / wasm-incompatible code; uses the SVG primitives the framework already ships; the `printpdf` upgrade also benefits everyone exporting PDFs the other direction.

---

## 0.7. Goal apps — keep the agent focused on real users

Every priority tier in §4 is anchored to a **hypothetical "goal app"** the agent can keep in mind. The features must serve the goal app, not "what's interesting to build." If a sub-feature doesn't unblock the goal app, defer it to a follow-up sprint.

| Tier | Goal app | Why this app drives the features |
|---|---|---|
| P1 — **Foundation** | (no goal app; enables P2+) | Without system-font discovery + permission plumbing, *every* mobile screen looks broken or blocked at first prompt. |
| P2 — **AzulPaint** | A finger-paint / stylus drawing canvas. Save as PNG/SVG. Eraser tip works. | Forces real `TouchEvent` + `PenState` (tilt + force + barrel button + eraser) end-to-end. Forces the "permission-as-DOM" model for `Photos` (save dialog). |
| P3 — **AzulMaps** | Pan/zoom map widget centered on user's location, tap a pin → callout. | Forces `GeolocationProbe` + `MapTile(MVT)` + a touch-driven viewport state — the exact loop a Google Maps clone uses. |
| P4 — **AzulVault** | A 30-entry password manager. Unlock via biometric. Sync entries via libsql remote. | Forces biometric + system-keyring + libsql remote auth-token handling. Permissions / fallbacks tested under realistic conditions. |
| P5 — **AzulDoc** | A markdown editor that exports a styled PDF. Inline-renders a reference PDF for diff view. | Forces both directions of PDF — display-list → PDF emit (printpdf direct), and PDF → SVG → render (printpdf+svg). Tests text shaping fidelity. |
| P6 — stretch | Camera / screen-share / sensors / gamepad / Wacom-pad — these are demo features that don't yet anchor a single goal app. Add them as horizontal expansions once P1-P5 ship. |

Carrying the goal app prevents the implementation agent from sprawling into "let's also do video filters" / "let's also do AR overlays." If a feature isn't in the goal-app punch list, it doesn't land in this sprint.

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

## 4. Implementation ordering — priorities + goal apps

Concrete ordering for the next implementation sprint, anchored by §0.7's goal apps. Each tier is one or more sprints; tier N strictly blocks tier N+1 only where noted.

### Priority 1 — Foundation (no goal app, enables P2+)

1.1. **`rust-fontconfig` mobile arms** (research/05) — patch the user-owned crate. iOS via `core-text::CTFontManagerCopyAvailableFontURLs`, Android via `/system/fonts/` walk + `/system/etc/fonts.xml`. **Without this, all mobile text is invisible.** ~3 days.

1.2. **`PermissionManager`** + Info.plist / AndroidManifest declaration scaffolding + the permission-diff pass after every layout (research/08). Lands as part of P2 if blocked, but the manager itself is the cross-cutting prerequisite. **Lives in `dll/src/desktop/extra/permission/` per §0.5.** ~2 days.

1.3. **File pickers on mobile** (research/04) — fills in the no-op stubs in `layout/src/desktop/dialogs.rs`. `UIDocumentPickerViewController` + Android Storage Access Framework. **The dll-side wiring lives in `dll/src/desktop/extra/file_picker/`; the trait shape stays in `azul-layout::desktop::dialogs` since the user-facing API is already there.** ~2 days.

### Priority 2 — Pen + touch (goal app: **AzulPaint**)

2.1. **Populate existing `PenState` fields on every backend** (`is_eraser`, `barrel_button_pressed` — declared but never set today, per research/03). ~1 day.

2.2. **Wire iOS UIKit `touchesBegan/Moved/Ended/Cancelled` to multi-touch `TouchPointVec`** (one entry per finger) — currently `handle_touch` only reads the first. ~1 day.

2.3. **Extend `PenState`** with `tangential_pressure`, `barrel_roll_rad`, `tool_id` (for Apple Pencil 2 / Surface Pen) + new `HoverEventFilter::PenSqueeze` / `PenDoubleTap` / `PenHover`. ~2 days.

2.4. **AzulPaint demo crate** at `examples/azul-paint/` — finger-paint + stylus support + PNG/SVG save via file picker. Lives in `examples/` not `dll/`. ~3 days.

### Priority 3 — Maps (goal app: **AzulMaps**)

3.1. **`GeolocationProbe` invisible NodeType + `GeolocationManager` + per-platform inject** (research/04 + research/08). Lives in `dll/src/desktop/extra/geolocation/`. ~3 days.

3.2. **`MapTile` NodeType + `MapTileManager`** — MVT decode via `mvt` crate + `agg-rust` line/polygon primitives (already present per research/06) + style-spec subset (10-15% of MapLibre). Lives in `dll/src/desktop/extra/map/`. **OpenFreeMap base URL pinned (current `20260513_001001_pt` snapshot), ODbL attribution string baked in.** ~6 days.

3.3. **AzulMaps demo crate** — viewport state, pan/zoom touch handlers, tap-to-pin-callout. ~2 days.

### Priority 4 — Auth (goal app: **AzulVault**)

4.1. **Biometric auth** (research/02): one Azul API `request_biometric_auth(prompt) -> Future<BiometricResult>`. iOS / macOS `LAContext`, Android `BiometricPrompt`, Windows `UserConsentVerifier`, Linux fallback. Lives in `dll/src/desktop/extra/biometric/`. ~4 days.

4.2. **System-keyring / passkey integration on non-Apple platforms** — this is the new bit the user called out. iOS / macOS use `Keychain` (already secure-enclave bound when `kSecAttrAccessControl=biometryCurrentSet`). For Linux: `libsecret` D-Bus → `gnome-keyring` / `kwallet`. Windows: `CredentialLocker` (`Windows.Security.Credentials.PasswordVault`). Android: `KeyStore` with `setUserAuthenticationRequired(true)`. The Web equivalent is `navigator.credentials` (WebAuthn). New `dll/src/desktop/extra/keyring/`. ~5 days.

4.3. **libsql remote auth-token handling** (research/07) — `DbHandle::url` redacts the token in `Debug`; the Cargo-feature `db-libsql-remote` gates the tokio dep. Lives in `dll/src/desktop/extra/sqlite/`. ~4 days for the basic three modes; the remote + encryption sprint is a follow-up.

4.4. **AzulVault demo crate** — 30-entry password manager, biometric unlock, libsql remote sync. ~4 days.

### Priority 5 — Documents (goal app: **AzulDoc**)

5.1. **PDF export** via `printpdf` — walk the display list, dispatch each `DisplayListItem` to printpdf `Op`s using the 18-row table from research/06. `App::export_pdf(path)`. Already half-wired via `DisplayListItem::TextLayout` (research/06's discovery). Lives in `dll/src/desktop/extra/pdf/`. ~4 days.

5.2. **PDF render** via `printpdf::page_to_svg()` → `azul_layout::svg::Svg::parse(...)` → existing SVG render (CPU + GPU). Per §0.6 — **no `pdfium-render` dependency**. The `NodeType::Pdf(PdfRef { page, scale, ... })` opens the doc once, caches each rendered page into an `ImageRef`. ~3 days.

5.3. **Watch for recursive dep risk**: `printpdf` already pulls `azul-layout` (the experimental integration on its import side). When `dll/extra/pdf/` adds `printpdf` as a dep, cargo resolves the cycle via the path-override. Document the manifest dance in the sprint plan to avoid a surprise. ~half-day investigation up front.

5.4. **AzulDoc demo crate** — markdown editor, real-time preview, export-to-PDF button, inline-render of a reference PDF for diff. ~4 days.

### Priority 6 — Horizontal expansions (no single goal app)

These don't anchor a goal app and can land any time after P5:

- **Camera** (research/01) — `<video>`-shaped node; needs `RawImageFormat::Nv12` to land zero-copy. Lives in `dll/src/desktop/extra/camera/`.
- **Screen sharing** (research/01) — most platform-specific. `dll/src/desktop/extra/screencap/`.
- **Accelerometer / gyro / magnetometer** (research/03). `dll/src/desktop/extra/sensors/`.
- **Gamepad** (research/03) — note `gilrs` covers desktop, mobile needs custom backend glue. `dll/src/desktop/extra/gamepad/`.
- **Wacom pad extensions** (research/03) — ExpressKeys + touch-rings. `dll/src/desktop/extra/wacom_pad/`.

---

### Notes on cross-cutting refactors

- The "permission-aware DOM" architecture (§1.5 + research/08) is a *cross-cutting* refactor of features #2, #3, #4, #5, #10 — not a separate sprint. The `PermissionManager` lands in P1.2; each subsequent feature plugs into it.
- The IME / `UITextInput` work the gesture sprint started (research/04) — Wayland `zwp_text_input_v3`'s six empty handlers, Android `InputConnection` JNI, iOS `UITextInput` — is shared infrastructure. Lands either in P1 or as a P2 sub-feature (text-input for the AzulPaint Save Dialog).
- The `RawImageFormat::Nv12` variant (research/01's prerequisite) lands in P6 with Camera; it can wait.

---

### Aggregate effort estimate

P1: ~7 days. P2: ~7 days. P3: ~11 days. P4: ~17 days. P5: ~12 days. P6: ~20 days.

Total to ship five goal apps: **~54 days** at one engineer; substantially faster in parallel since P2/P3/P4/P5 are largely independent after P1.

---

## 5. Tracker

Status as of 2026-05-20 (branch `mobile-ios-android`). Tick-by-tick detail in `scripts/MOBILE_SESSION_LOG.md`. All mobile work is verified by `cargo check` only (no iOS sim / Android emulator), so "done" means *compiles + correct per platform docs / unit-tested where pure-Rust*, not runtime-verified.

**P1.1 fonts** — DONE. `rust-fontconfig` iOS (CoreText) + Android (`/system/fonts` walk) arms.

**P1.2 permissions** — core DONE; some platform tails open.
- Sync probe: iOS + Android + macOS (real native status getters). Linux/Windows probe: not done (desktop, low-value / unverifiable here).
- Async result channel (`push_async_result`/`drain_async_results` in `azul-layout`) + layout-pass consumer: DONE + unit-tested.
- Request path: Android `requestPermissions` producer DONE (Rust side; `AzulPermissions.java` glue pending). Apple location auth-changes routed to the channel. **iOS/macOS generic request: GATED** — needs ObjC completion blocks (objc2-migration vs objc-0.2 block-bridge decision).
- Permission-diff pass wired to `NodeType::GeolocationProbe`: DONE.

**P1.3 file pickers** — iOS open + directory DONE; Android open/save/directory DONE. iOS save deferred (needs an API decision — `initForExportingURLs` has no source-file in the signature).

**P2 pen/touch (AzulPaint)** — DONE. `PenState` populated (is_eraser, barrel_button, multi-touch `TouchPointVec`, + `barrel_roll_rad` from Apple Pencil Pro `rollAngle`). AzulPaint demo complete (clear, point counter, pressure + chisel-nib brush). **P2.3 `HoverEventFilter::PenSqueeze/PenDoubleTap/PenHover`: GATED** — core-enum + codegen + dispatch + `UIPencilInteraction`.

**P3.1 geolocation** — DONE (Rust side). Manager + probe-diff + async fix channel + session producers on Android/iOS/macOS + Apple auth-feedback. `CallbackInfo::get_location_fix` layout accessor + `impl_option!(LocationFix)` landed; **FFI exposure BLOCKED on disk** (a `codegen all` job; recipe in the session log — register `OptionLocationFix` api.json type first).

**P3.2 MapWidget** — DONE + fully unit-tested (Web-Mercator projection, pan-delta, merge-callback cache survival, visible-tile range). MVT+MapCSS→SVG→DOM pipeline + VirtualView + thread-based fetch wired at source level. **Real-tile demo wiring: GATED** — worker-exposure decision (`dom_with_default_tiles()` vs `tile_fetch_thread_callback()`).

**P3.3 AzulMaps** — demo with viewport state, pan/zoom toolbar, composed geolocation probe + placeholder dot. **Tap-to-pin GATED** (worker exposure); real-position dot needs the P3.1 `get_location_fix` FFI exposure.

**P4 (AzulVault) / P5 (AzulDoc) / P6 (camera/sensors/gamepad)** — not started.

### Blocking on the user
1. **Disk** — volume at ~97% (6–7 GiB free; prior ENOSPC crisis at 100%). `codegen all` and core-cascade rebuilds risk corruption; needs non-azul space freed for durable headroom.
2. **Decisions** (open ~15 ticks): iOS/macOS permission-request ObjC blocks; P3.3 tap-to-pin worker exposure; P2.3 `HoverEventFilter` variants.
