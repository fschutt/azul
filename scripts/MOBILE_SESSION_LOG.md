# Mobile Cross-Compile Session Log

Append-only log of autonomous progress on `mobile-ios-android` branch.
Each cron-fired tick adds one entry below. Tip: search for `## 2026-` to
jump between days.

---

## 2026-05-19 (initial bring-up)

- Sprint A: foundation committed (`c6aee0e26`). SUPER_PLAN.md + .cargo/config.toml + scripts/check-prereqs-mobile.sh.
- Installed Android cmdline-tools + platform-tools (Homebrew), NDK 27.0.12077973 + build-tools;34.0.0 + platforms;android-34 (sdkmanager). `check-prereqs-mobile.sh` reports `Android: ready`.
- iOS prereqs: only CommandLineTools, **no full Xcode** → `xcrun --sdk iphonesimulator` fails. Documented in plan; iOS sprints gated until user installs Xcode.
- Sprint F partial (commit `c83ebbb67`): Android skeleton landed.
  - `dll/src/desktop/shell2/android/mod.rs` (~200 lines): AndroidWindow with CommonWindowState + CpuBackend, android_main entry, PollEvent dispatch placeholders.
  - shell2/mod.rs: pub mod android + cfg_if branch.
  - run.rs: #[cfg(target_os="android")] run() stashes ANDROID_INITIAL_OPTIONS.
  - build.rs: configure_android() links -landroid -llog; ios-deploy panic→warning.
- Cross-compile gap fixes (pre-existing breakage discovered during Android `cargo check`):
  - `tfd` desktop-only — target-conditional in dll/Cargo.toml + layout/Cargo.toml; cfg-gate dialogs module + msg_box panic call.
  - `FontInstancePlatformOptions` stub for android/ios in core/src/resources.rs.
  - `TEXT_INPUT_*_PROPS` static defs extended to android/ios (mirror macOS) in layout/src/widgets/text_input.rs.
- Codegen step (`cargo run --release -p azul-doc -- codegen all`) ran successfully.
- **Status:** `cargo check --target aarch64-linux-android -p azul-dll --no-default-features --features 'std,logging,link-static,a11y'` still fails with ~119 pre-existing errors. Top categories:
  - codegen `*const i8` vs `*const u8` (auto-generated dll_api_internal.rs uses literal i8, not `c_char`).
  - `debug_server.rs:9806` match-on-`()` because the macos/linux/windows-only open-default-app block has no Android arm.
  - More to enumerate.

### Sprint F GATE PASSES (Android compile)

After Sprint F partial commit `c83ebbb67`, kept chipping at the remaining errors:
- 119 → 15: tfd was still being pulled by webrender → azul-layout default features. Replaced the cfg-gated `pub mod dialogs;` with a permanent stub at `layout/src/desktop/dialogs_stub.rs` so consumers (`azul-dll::desktop::dialogs::*`) keep resolving.
- 15 → 2: added missing imports (`OptionGlContextPtr`, `ScrollbarDragState`, `AsyncHitTester`, `HitTestNode`, `DocumentId`, `IdNamespace`, `WrRenderApi`) to `dll/src/desktop/shell2/android/mod.rs`; added `#[cfg(any(target_os = "android", target_os = "ios"))] let platform_options = FontInstancePlatformOptions::default();` to `dll/src/desktop/wr_translate2.rs:1206`; added Android arm to `debug_server.rs` OpenFile dispatch (synthesised `Err` so the `match result` keeps type-checking).
- 2 → 1: implemented the full `PlatformWindow` trait body for `AndroidWindow` (the `impl_platform_window_getters!` macro only emits getters; the timer/thread/queue_window_create/show_menu/tooltip methods are platform-supplied). Mirrored headless backend's no-op implementations.
- 1 → 0: `css/src/corety.rs::from_c_str` accepted `*const core::ffi::c_char`, which is `*const u8` on Android but `*const i8` on Apple / Linux / Windows — the codegen (api.json declares `*const i8` literally) couldn't match on Android. Changed signature to `*const i8` and cast internally before `CStr::from_ptr`.

**Result:** `cargo check --target aarch64-linux-android -p azul-dll --no-default-features --features 'std,logging,link-static,a11y'` **PASSES** in 11.71 s. Full `cargo build` produces `target/aarch64-linux-android/debug/libazul.so` (447 MB unstripped debug cdylib), `libazul.a` (1.2 GB static), `libazul.rlib` (224 MB). Sprint F GATE green.

Next cron tick: Android Phase 2 (CPU render via `ANativeWindow_lock`) — wire `CpuBackend::render_frame` to `render_frame()` in `android/mod.rs`. Or Sprint I (build-android.sh APK pipeline). Or pick up iOS Phase 1 if Xcode appears.

### Tick — Android pixmap→ANativeWindow blit (#11)

`android/mod.rs::render_frame()` is no longer a stub: if a native window + a `cpu_backend.last_frame` AzulPixmap are present, it calls `set_buffers_geometry(w, h, R8G8B8A8_UNORM)`, then `lock(None)`, then iterates `NativeWindowBufferLockGuard::lines()` and copies one scanline at a time (handles stride > width). Guard drop posts the frame via `ANativeWindow_unlockAndPost`. Feature-gated on `(android-activity, ndk, cpurender)` with a fallback stub. `cargo check --target aarch64-linux-android` still GREEN (~29 s incremental). Layout regeneration not yet hooked, so the path is exercise-only until the next tick wires `regenerate_layout()` → populates `last_frame`.

### Tick — Sprint I (Android APK build pipeline)

Two new files, no Rust changes (compile gate untouched):
- `scripts/android/AndroidManifest.xml` — minimal NativeActivity template with `@PACKAGE@`/`@LABEL@`/`@LIB_NAME@`/`@VERSION_*` placeholders. `android:hasCode="false"` so we ship zero Java/Kotlin; `android:configChanges` traps rotation/keyboard/density natively.
- `scripts/build-android.sh` — `cargo build --release` for any android target → copy `libazul.so` into `lib/$ABI/` → `aapt2 link` manifest → `zip` the lib into the apk → `zipalign 4` → `apksigner sign` with `scripts/android/debug.keystore` (auto-created on first run with `keytool -genkeypair`) → optional `adb install -r` + `am start`. `AZ_ANDROID_NO_DEPLOY=1` skips the device step. Bash syntax-checked clean.

### Tick — Sprint E (iOS build pipeline, no Xcode project)

Three new files, no Rust changes:
- `scripts/ios/Info.plist` — minimal app plist template with `@EXECUTABLE@`/`@BUNDLE_ID@`/`@DISPLAY_NAME@`/`@VERSION@`/`@BUILD@`/`@MIN_OS@` placeholders. `LSRequiresIPhoneOS = true`, `UILaunchStoryboardName` empty (no storyboard), portrait + landscape orientations declared.
- `scripts/ios/entitlements.xcent` — placeholder entitlements (TEAMID + `get-task-allow`) suitable for ad-hoc / development signing. For App Store, regenerate from the provisioning profile.
- `scripts/build-ios.sh` — Xcode-CLT-free pipeline: validates `xcrun --sdk $iphone{os,simulator} --show-sdk-path` is available (errors out clearly if not), `cargo build --release` for the target, copies the artifact into `target/ios-bundle/<APP_NAME>-<TARGET>.app/`, renders the Info.plist via sed, converts to binary plist via `plutil`, then on simulator → `xcrun simctl install + launch booted`, on device → optional `codesign` if `IOS_SIGNING_IDENTITY` set + `ios-deploy --bundle … --justlaunch` if installed. `AZ_IOS_DRYRUN=1` stops after bundle. Bash syntax-checked clean.

Pipeline is testable end-to-end the moment full Xcode is installed; until then the script gates cleanly with an actionable error.

### Tick — Android prerequisites for regenerate_layout (#11)

`AndroidWindow` now stores `icon_provider: SharedIconProvider` and `font_registry: Option<Arc<FcFontRegistry>>` (matching `HeadlessWindow`); `new()` takes a `mut config` and `mem::take`s the icon_provider handle. `android_main` passes the `font_registry` previously stashed in `ANDROID_INITIAL_OPTIONS`. cargo check still GREEN (~13 s incremental). Next tick: write `AndroidWindow::regenerate_layout()` mirroring `HeadlessWindow::regenerate_layout()` so cpu_backend.last_frame populates, then have `render_frame()` call it ahead of the blit.

### Tick — Android regenerate_layout + event-loop wire-up (#11)

`AndroidWindow::regenerate_layout()` lands — direct port of `HeadlessWindow::regenerate_layout()` minus the lifecycle-event dispatch (Sprint H). Calls `common::layout::regenerate_layout` with all eleven args (layout_window, app_data, current_window_state, &mut renderer_resources, image_cache, gl_context_ptr, fc_cache, font_registry, system_style, icon_provider, next_relayout_reason); rebuilds cpu_hit_tester; CPU-renders into `cpu_backend.last_frame`; resets `next_relayout_reason` to `RefreshDom` and clears `frame_needs_regeneration`. The `android_main` outer loop now calls `regenerate_layout()` whenever `frame_needs_regeneration` is true, immediately before `render_frame()`. cargo check still GREEN (~12.5 s incremental). With this in place, a real-device run should now produce pixels — the only remaining gap is Sprint H (touch/keys → process_window_events).

### Tick — Android Sprint H touch wire-up (#12)

`MainEvent::InputAvailable` now calls a new `drain_input()` helper that iterates `app.input_events_iter()` and maps the first pointer of every `MotionEvent` to the mouse left button: `Down` → `cursor_position + left_down=true`; `Move`/`HoverMove` → `cursor_position`; `Up`/`Cancel` → `left_down=false`. Every motion sets `frame_needs_regeneration` so the outer loop relayouts+blits. `KeyEvent` arm is stubbed (Sprint H follow-up: `Keycode → VirtualKeyCode + unicode_char → handle_text_input`). Imports fixed twice: `InputStatus` lives at the `android_activity` crate root, not under `input::`; `InputEvent + MotionAction + KeyAction` are under `input::`. cargo check still GREEN (~16 s incremental). Hover and click should now reach Azul callbacks; multi-touch and IME deferred.

### Tick — Android process_window_events + update_hit_test_at wired (#12)

Mouse-state mutation alone wasn't enough to dispatch `On::Click` callbacks — the framework's event system runs off **state diffing** between `previous_window_state` and `current_window_state`. `drain_input()` now follows the headless backend's three-step pattern for each MotionEvent: (1) snapshot `previous_window_state = Some(current_window_state.clone())`, (2) update `current_window_state.mouse_state`, (3) call `update_hit_test_at(pos)` then `process_window_events(0)`. If `ProcessEventResult != DoNothing`, set `frame_needs_regeneration`. The collect-then-process pattern dodges the borrow-checker dance where the `iter.next` closure holds `&AndroidApp` while we need `&mut AndroidWindow`. cargo check still GREEN (12.59 s).

### Sprint #15 — Gesture/touch/pen accessors wired through api.json + codegen

User-driven feature request: make sure all event filters (click, touch, swipe, pen) are wired through to api.json so consumer languages can subscribe to them separately, and prep the data side too. Inventory first: all 165 event-filter variants (HoverEventFilter 55, FocusEventFilter 44, WindowEventFilter 56, ComponentEventFilter 6, ApplicationEventFilter 4) are already in perfect Rust ↔ api.json sync, including pen (PenDown/Move/Up/Enter/Leave) and gestures (DoubleClick, LongPress, SwipeLeft/Right/Up/Down, PinchIn/Out, RotateClockwise, RotateCounterClockwise). Gap was on the **data side**: detected gesture results were defined in Rust (`DetectedPinch`/`DetectedRotation`/`DetectedLongPress`/`GestureDirection`) but not exposed via api.json, and `CallbackInfo` lacked convenience accessors.

Changes:
- `layout/src/managers/gesture.rs`: `#[repr(C)]` on `DetectedPinch`, `DetectedRotation`, `DetectedLongPress`, `GestureDirection`; new `impl_option!` for each so `OptionDetectedPinch` / `OptionDetectedRotation` / `OptionDetectedLongPress` / `OptionGestureDirection` are FFI-safe.
- `layout/src/callbacks.rs`: `CallbackInfo::get_swipe_direction`, `get_pinch`, `get_rotation`, `get_long_press`, `was_double_clicked` accessors that delegate to `GestureAndDragManager::detect_*`, returning the new `Option*` wrappers.
- `layout/src/desktop/dialogs.rs`: merged the previously-separate `dialogs_stub.rs` back into a single file with internal `#[cfg]` arms; the stub file was a duplicate from azul-doc's POV and tripped its 16-critical-FFI-error duplicate-type scanner. Single file = no duplicates, autofix is clean.
- `api.json`: 4 new types (DetectedPinch, DetectedRotation, DetectedLongPress, GestureDirection) + 4 Option wrappers (slotted near OptionPenTilt) + 5 CallbackInfo methods (added via `azul-doc autofix add CallbackInfo.<method>` + `autofix apply`). Final autofix report: 0 path fixes / 0 types to add / 0 modifications / 0 critical FFI issues.

`cargo run --release -p azul-doc -- codegen all` regenerated all 35 language bindings (C/C++/Python/Java/Kotlin/Go/Rust/Node/etc.) plus `dll_api_internal.rs`. `cargo check --target aarch64-linux-android -p azul-dll --no-default-features --features 'std,logging,link-static,a11y'` GREEN in 12.5 s — confirming the new types reach the cross-compile gate cleanly.

SUPER_PLAN.md gets a new Sprint M ("Native gesture recognizers") documenting the architecture for native iOS UIKit / Android `GestureDetector` / macOS `NSGestureRecognizer` hooks. Pattern: platform backend calls `GestureAndDragManager::inject_native_gesture(NativeGestureEvent::*)` before the per-frame tick; accessors observe the override before falling back to in-process detection. Linux + Windows keep using only the in-process detector (Azul's "superset of every platform" guarantee — same surface, best available implementation).

### Tick — Sprint M architecture seam (NativeGestureEvent + injection slot)

`layout/src/managers/gesture.rs` gains the architectural hook for native gesture recognizers:
- New `NativeGestureEvent` enum (`#[repr(C, u8)]`) carrying the same payloads the in-process detector produces: `DoubleClick`, `LongPress(DetectedLongPress)`, `Swipe(GestureDirection)`, `Pinch(DetectedPinch)`, `Rotation(DetectedRotation)`.
- `GestureAndDragManager.native_gesture: Option<NativeGestureEvent>` slot + `inject_native_gesture()` + `clear_native_gesture()` helpers.
- The five `detect_*` methods (long_press, double_click, swipe_direction, pinch, rotation) consult `self.native_gesture` before running their heuristic detectors. Linux/Windows/headless never inject, so heuristics remain authoritative there.

cargo check still GREEN (1m19s incremental — rebuild touched many crates). Two new follow-up tasks tracked: #16 iOS UIKit gesture-recognizer wire-up, #17 Android `GestureDetector` JNI bridge.

### Tick — Android drain_input clears native_gesture per frame

`dll/src/desktop/shell2/android/mod.rs::drain_input()` now calls `layout_window.gesture_drag_manager.clear_native_gesture()` after the final `process_window_events(0)` so any injected `NativeGestureEvent` is single-shot. No-op until #17 wires real injection through the JNI bridge, but the seam is in place — once `GestureDetector` callbacks fire, gestures will reach `CallbackInfo::get_*()` and then clear cleanly. cargo check still GREEN (17.65 s).

### Sprint #18 — e2e debug-server events for touch / pen / gestures

The e2e harness (debug-server JSON-driven tests) can now exercise every event filter end-to-end. Thirteen new `DebugEvent` variants land in `dll/src/desktop/shell2/common/debug_server.rs`:

- **Touch** (state-diff path): `TouchStart { id, x, y, force }`, `TouchMove { id, x, y, force }`, `TouchEnd { id }`, `TouchCancel`. Handlers mutate `current_window_state.touch_state.touch_points` via `callback_info.modify_window_state(...)`; the framework's event-determination then fires `HoverEventFilter::TouchStart/Move/End/Cancel`.
- **Pen / stylus** (mouse-pipe + future pen path): `PenDown { x, y, pressure, x_tilt, y_tilt }`, `PenMove { ... }`, `PenUp { x, y }`. For now these drive the mouse pipeline so click handlers fire; full pen-specific injection (`PenState` on `GestureAndDragManager`) is a follow-up.
- **Native gestures** (override slot): `Swipe { direction }`, `Pinch { scale, center_x, center_y, initial_distance, current_distance, duration_ms }`, `Rotate { angle_radians, center_x, center_y, duration_ms }`, `LongPress { x, y, duration_ms }`. New helpers: `default_force() -> f32` (= 0.5, matches `TouchPoint::force` sentinel) and `SwipeDir` enum (`Up/Down/Left/Right`).

Plumbing additions:
- `CallbackChange::InjectNativeGesture { gesture: NativeGestureEvent }` enum variant. Applied in `dll/src/desktop/shell2/common/event.rs::apply_user_change` by calling `layout_window.gesture_drag_manager.inject_native_gesture(...)`; returns `ShouldRegenerateDomCurrentWindow` so the next layout/event cycle picks up the override.
- `CallbackInfo::inject_native_gesture(&mut self, NativeGestureEvent)` queues the change. Same callable from both the platform backends (iOS UIKit recognizer callbacks etc., #16/#17) and the e2e harness — single injection path keeps semantics consistent.

cargo check --target aarch64-linux-android still GREEN in 26.79 s; host-target cargo check also GREEN. Sample JSON test now possible: `[{"op": "swipe", "dir": "left"}, {"op": "pinch", "scale": 2.0, "center_x": 200, "center_y": 300}, {"op": "long_press", "x": 50, "y": 50, "duration_ms": 800}]`.

### Tick — Sprint M Android JNI bridge for GestureDetector (#17)

Two artifacts land:
- `scripts/android/NativeGestureBridge.java` — `GestureDetector.SimpleOnGestureListener` + `ScaleGestureDetector.OnScaleGestureListener` + a custom two-finger rotation detector. Each callback dispatches to a `private static native nativeOn<Verb>(long nativePtr, ...)` JNI method. The `nativePtr` is the AndroidWindow address passed in at construction. Compiles outside Gradle with `javac -source 11 -target 11 -classpath $ANDROID_HOME/platforms/android-34/android.jar` and packs into `classes.dex` via `d8`. The Java side never holds static state — `nativePtr` is the only cookie.
- `dll/src/desktop/shell2/android/mod.rs::jni_bridge` — five `#[no_mangle] extern "system"` symbols matching the Java JNI lookup names: `Java_com_azul_gesture_NativeGestureBridge_nativeOn{DoubleTap,LongPress,Swipe,Pinch,Rotation}`. Each cast `native_ptr: i64` back to `&mut AndroidWindow` and `inject_native_gesture(NativeGestureEvent::*)`. Direction constants in the Java side mirror `GestureDirection`'s `#[repr(C)]` 0-3 ordering.

cargo check --target aarch64-linux-android still GREEN (13.38 s). The wire is complete from `setOnTouchListener` → `onDoubleTap`/`onFling`/`onScale`/2-finger rotation → JNI → `inject_native_gesture` → `clear_native_gesture` at end-of-frame. Activation requires the Java side to actually be loaded by `NativeActivity` (current AndroidManifest.xml uses pure NativeActivity with `android:hasCode="false"`, so the .dex isn't shipped). Wire-up in build-android.sh is a follow-up tick.
