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

### Sprint B GATE GREEN — iOS source compiles for every iOS target

`dll/src/desktop/shell2/ios/mod.rs` rewritten to mirror Android's clean skeleton, fixing the 18 stale errors from the original WIP:
- Imports: dropped `WindowState` / `WrTransaction` / `ShareId` / `INSObject` / `NSObject`; added `HitTestNode`, `IdNamespace`, `ScrollbarDragState`, `CpuBackend`, `SharedIconProvider`, `FcFontRegistry`, `AsyncHitTester`, `WrRenderApi`, `RelayoutReason`.
- `unsafe impl Encode for CGPoint/CGSize/CGRect` using objc 0.2's `fn encode() -> Encoding` API (string-based: `{CGPoint=dd}` etc.) — not the objc2 `const ENCODING` surface.
- `extern "C" fn(self: &Object, ...)` → `extern "C" fn(_this: &Object, ...)` (`self` is reserved). Four touch handlers (`began`/`moved`/`ended`/`cancelled`) all stubbed but registered.
- `FullWindowState::new(options.state)` → `options.window_state` (consistent with Android).
- Native UI build uses raw `*mut Object` pointers through `msg_send!` chain, then wraps in `Id::from_ptr(...)` once at the end; no more `id.clone()` / `Id::as_ptr(&id)` (neither exists on `Id<Object>` in objc_id 0.1). Raw-pointer extraction for `IOSHandle` is `(&*self.ui_window as *const Object) as *mut c_void`.
- Full `PlatformWindow` trait impl mirroring Android (prepare_callback_invocation, timer/thread/queue/menu/tooltip stubs).
- `IOSWindow::new` takes the same 5-arg signature as `AndroidWindow::new`; `did_finish_launching` retrieves all five from `INITIAL_OPTIONS`.

Verified GREEN:
- `cargo check --target aarch64-apple-ios -p azul-dll …`        (0.37s)
- `cargo check --target aarch64-apple-ios-sim -p azul-dll …`    (25.49s)
- `cargo check --target x86_64-apple-ios -p azul-dll …`         (26.22s)
- `cargo check --target aarch64-linux-android -p azul-dll …`    (0.38s — no regression)

Sprint B (iOS Phase 1: compile) is now **GREEN at the source level on every iOS target**. The linker step still needs the iOS SDK (`xcrun --sdk iphonesimulator` currently errors with "SDK cannot be located" on this box); once Xcode finishes installing, `cargo build` should complete without further code changes.

### Tick — Sprint M iOS UIKit gesture recognizers (#16)

`dll/src/desktop/shell2/ios/mod.rs` now wires the iOS half of the native-gesture path. Same shape as the Android JNI bridge:
- 8 new `extern "C" fn(_this: &Object, _cmd: Sel, sender: *mut Object)` action selectors: `on_double_tap`, `on_long_press`, `on_swipe_{left,right,up,down}`, `on_pinch`, `on_rotation`. Each reads `[sender state]` (gates to `Recognized` / `Began` / `Changed` as appropriate) and `[sender locationInView:nil]` / `[sender scale]` / `[sender rotation]` as needed, then calls `inject(window, NativeGestureEvent::*)`. The same `GestureAndDragManager::inject_native_gesture` hook the e2e harness and the Android JNI bridge use.
- New `AzulGestureTarget` NSObject subclass (registered via `objc::declare::ClassDecl`) with the eight action methods. One instance is leaked once at app startup and shared by every recognizer.
- `install_gesture_recognizers(view)` builds + attaches `UITapGestureRecognizer(numberOfTapsRequired=2)`, `UILongPressGestureRecognizer`, `UIPinchGestureRecognizer`, `UIRotationGestureRecognizer`, and four `UISwipeGestureRecognizer`s (one per direction, since UIKit fires the action once per direction). Direction bitmask values mirror UIKit's `UISwipeGestureRecognizerDirection` enum: Right=1, Left=2, Up=4, Down=8.
- Called from `IOSWindow::new` after `[window makeKeyAndVisible]`.

cargo check --target aarch64-apple-ios still GREEN (15.07s). The wire is complete from UIKit recognizer → action selector → `inject_native_gesture` → `CallbackInfo::get_*` accessors. Activation requires the iOS SDK to actually link, which `xcrun --sdk iphonesimulator` will surface once Xcode finishes installing.

### Tick — build-android.sh ships classes.dex (#17 activation)

`scripts/build-android.sh` extended with an optional Java-compile pass that auto-activates whenever `scripts/android/*.java` exist (currently `NativeGestureBridge.java`). Pipeline order:
1. `javac -source 11 -target 11 -classpath $ANDROID_HOME/platforms/android-34/android.jar -d $BUILD_DIR/classes …`
2. `$BUILD_TOOLS/d8 --output $BUILD_DIR/dex $(find classes -name '*.class')`
3. Sed in the manifest: `android:hasCode="false"` → `"true"` when shipping a .dex.
4. After `aapt2 link` + lib zip, also `zip -r base.apk classes.dex`.

Escape hatch: `AZ_ANDROID_NO_JAVA=1` skips the dex pass entirely (manifest stays `hasCode="false"`). Bash syntax-checked clean. No Rust changes — Android cargo check unaffected.

The NativeActivity glue still won't *load* the bridge class on its own; #17 is one step closer but the runtime instantiation (`NewGlobalRef` via JNI from `android_main` or a thin `Activity` subclass) is the next tick.

### Tick — AzulActivity + window-pointer publication (#17 runtime hookup)

`scripts/android/AzulActivity.java` — 50-line `NativeActivity` subclass whose `onWindowFocusChanged` does the one-shot `new NativeGestureBridge(nativePtr).attach(this, decor)` call. Uses `nativeGetWindowPointer()` (JNI) to fetch the address `android_main` published. Lazy-instantiates on first focus so race-conditions with `android_main` startup are bounded; idempotent (gestureBridge != null guard).

`dll/src/desktop/shell2/android/mod.rs`:
- New `ANDROID_WINDOW_PTR: AtomicI64` (cfg'd to the android-activity feature). Initialized to 0.
- `android_main` stores `&mut window as *mut AndroidWindow as i64` into the slot right after `AndroidWindow::new` succeeds.
- New `#[no_mangle] extern "system" fn Java_com_azul_app_AzulActivity_nativeGetWindowPointer` reads the slot.

`scripts/android/AndroidManifest.xml` — `android:name="android.app.NativeActivity"` → `"com.azul.app.AzulActivity"`. Combined with the previous tick's `hasCode="true"` flip and `classes.dex` ship, the activity stack is now: Android OS → AzulActivity (Java, in dex) → super.onCreate loads libazul.so → android_main runs → publishes window ptr → AzulActivity.onWindowFocusChanged constructs NativeGestureBridge → bridge attaches OnTouchListener that fans into GestureDetector / ScaleGestureDetector / 2-finger rotation → JNI back into Rust → `GestureAndDragManager::inject_native_gesture`.

cargo check --target aarch64-linux-android still GREEN (17.28s).

### Tick — IOSWindow::regenerate_layout (#8 iOS Phase 2 prep)

`IOSWindow` gains `pub fn regenerate_layout()` — exact port of `AndroidWindow::regenerate_layout()`. Calls `common::layout::regenerate_layout` with all eleven args (layout_window, app_data, current_window_state, &mut renderer_resources, image_cache, gl_context_ptr, fc_cache, font_registry, system_style, icon_provider, next_relayout_reason); rebuilds `cpu_backend.hit_tester`; CPU-renders into `cpu_backend.last_frame`; resets `next_relayout_reason` to RefreshDom and clears `frame_needs_regeneration`. The actual `drawRect:` blit (CGImage from AzulPixmap → CALayer.contents) lives in Sprint C-iOS; this tick lands the prerequisite layout pump. cargo check aarch64-apple-ios GREEN (12.23s); aarch64-linux-android still GREEN (0.54s no-op).

### Tick — iOS touch events drive process_window_events (#9)

`extern "C" fn touches_began/moved/ended/cancelled` all delegate to a single shared `handle_touch(this, touches, phase)` helper. Phase encoding: 0=began, 1=moved, 2=ended, 3=cancelled. The helper mirrors Android's `drain_input` three-step pattern:
1. Snapshot `previous_window_state = current_window_state.clone()`.
2. Update `current_window_state.mouse_state`: cursor_position from `[anyTouch locationInView: this_view]`; `left_down` set on began, cleared on ended/cancelled.
3. `update_hit_test_at(pos)` then `process_window_events(0)`. If result != DoNothing, set `frame_needs_regeneration`.
4. `clear_native_gesture()` so an injected OS gesture from Sprint M doesn't double-fire.
5. `[view setNeedsDisplay]` so drawRect: picks up the new layout.

`anyObject` selector pulls one UITouch from the NSSet (sufficient for hover/click; multi-touch is a Sprint M follow-up). cargo check aarch64-apple-ios GREEN (13.81s); aarch64-linux-android still GREEN (0.56s).

iOS Phase 3 is now structurally complete: tap on a button → touch event → state diff → callback fires → drawRect re-renders. Linker still gated on Xcode.

### Tick — Sprint J PNG-diff wrapper script

`scripts/mobile-snapshot.sh <example> [<golden>]` lands the third leg of Sprint J: build + run the example with `AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=actual.png`, then diff against `scripts/mobile/golden/<name>.png`. Diff tooling priority: `compare -metric AE -fuzz 1%` (imagemagick) → `cmp -s` (POSIX byte-equal) → final-message hint to install imagemagick. `AZ_SNAPSHOT_UPDATE=1` re-baselines the golden. `scripts/mobile/golden/.gitkeep` so the directory exists in-repo even before any golden lands. Bash syntax-checked clean. No example wired yet — the script is parametric; supplying `bash scripts/mobile-snapshot.sh azul-example-headless` once such a crate exists will run end-to-end.

### Tick — iOS layoutSubviews handles orientation / split-view resize

AzulView gets a `layoutSubviews` selector. UIKit fires it whenever the view's bounds change (device rotation, split-view drag on iPad, safe-area-insets shift after status-bar hide/show). The handler reads `[this bounds]`, updates `current_window_state.size.dimensions` if it changed by more than 0.5 pt, and flips `frame_needs_regeneration` so the next CADisplayLink tick triggers a relayout + redraw. Plus `[this setNeedsDisplay]` so the layer redraws even when the size delta is below threshold (e.g. safe-area shift inside the same orientation).

All 5 mobile cargo-check targets still GREEN (17/5/3/1/0 s).

### Tick — iOS [UIScreen scale] + Android density math both → ws.size.dpi (96 baseline)

azul-layout treats 96 dpi as its 1× baseline (`dpi_factor = ws.size.dpi / 96.0`). The previous tick wrote Android's raw `density()` (mdpi 160 baseline) straight in, which would have given a 5× too-big factor on xxhdpi (480/96 = 5 vs the correct 3). Both platforms now normalize to the framework's 96-baseline:

- **Android `InitWindow`**: `dpi = round(density × 96 / 160)`. mdpi 160 → 96 (1×), xhdpi 320 → 192 (2×), xxhdpi 480 → 288 (3×). The motion-event scale in `drain_input` still uses raw `density / 160` (Android-native semantics, distinct from the framework's dpi_factor).
- **iOS `IOSWindow::new`**: `[UIScreen mainScreen].scale` is 1 / 2 / 3 (points per pixel). `dpi = round(scale × 96)`. 2× retina → 192, 3× retina → 288. `bounds.size` from `[screen bounds]` is already in points (logical units) so it goes straight into `dimensions.width/height`. `full_window_state` is now `let mut` so we can write to it before the `Self` struct literal.

All 5 mobile cargo-check targets still GREEN (24/6/10/4/4 s).

### Tick — Android InitWindow / WindowResized propagate dpi + logical dims to current_window_state

`handle_poll_event` now writes `current_window_state.size.{dpi, dimensions}` from `app.config().density()` + the `NativeWindow`'s physical dimensions. Without this, `regenerate_layout` was computing `dpi_factor = ws.size.dpi / 96.0` against the default `dpi = 96` and shrinking layout 3× on a 480-dpi screen. `MainEvent::WindowResized` reuses the same dpi to recompute logical dimensions from the new physical size. All 5 mobile cargo-check targets stay GREEN (15/0/1/4/5 s).

### Tick — Android real DPI scale from AConfiguration.density

`drain_input` no longer hardcodes `dpi = 1.0`. It now reads `app.config().density()` (returned in DPI; Android's baseline is 160 = mdpi) and divides each raw pointer x/y by `density / 160.0` to convert to logical pixels. Falls back to `1.0` if the config returns `None` (rare devices) or 0 (defensive). A 480-dpi xxhdpi phone now gives logical positions equal to physical_px / 3 — matching Compose / web `1dp = 1/160 inch` semantics. cargo check all 5 mobile targets still GREEN (~13 s total warm-cache).

### Tick — iOS CADisplayLink → display_tick: present-on-refresh

`install_display_link(view)` constructs a `CADisplayLink` via `displayLinkWithTarget:selector:`, points it at the shared `AzulGestureTarget` NSObject (now carries a `displayTick:` method alongside the gesture selectors), and adds it to `[NSRunLoop mainRunLoop]` with `kCFRunLoopDefaultMode`. The `extern "C" fn display_tick` reads the singleton `AZUL_IOS_WINDOW` and calls `window.present()` (which kicks `[view setNeedsDisplay]`) whenever `frame_needs_regeneration` is true — gating means we don't redraw 60×/s when nothing changed. `IOSWindow::new` calls `install_display_link(view)` right after `install_gesture_recognizers(view)`. All 5 mobile cargo-check targets stay GREEN (15/4/3/1/0 s).

Without this hook, frames only ticked on touch / timer events — animations would have been stuck. Now any layout change pumps a redraw at the screen's native refresh.

### Tick — iOS AppDelegate lifecycle selectors

Five new `extern "C"` selectors registered on `AppDelegate`:
- `applicationDidBecomeActive:` — force `frame_needs_regeneration = true` + `present()` so the layer is fresh after returning from background.
- `applicationWillResignActive:` — log stub. (CADisplayLink stops firing automatically while inactive, so render work pauses naturally.)
- `applicationDidEnterBackground:` — log stub. Sprint M-iOS-life will use the ~5 s background window for checkpointing.
- `applicationWillEnterForeground:` — log stub.
- `applicationWillTerminate:` — drops the boxed `AZUL_IOS_WINDOW` so `CommonWindowState` (RefAny, LayoutWindow) gets released in a controlled scope before process exit.

All 5 mobile cargo-check targets still GREEN (~25 s warm-cache).

### Tick — AZ_HEADLESS_SNAPSHOT_PATH for golden-PNG snapshotting (#13)

`HeadlessWindow::run` now honors `AZ_HEADLESS_SNAPSHOT_PATH=/path/to/out.png`. After the initial layout fires, if the env var is set, the backend calls `AzulPixmap::encode_png()` on `cpu_backend.last_frame` and writes the bytes to the given path, then calls `self.close()` so the event loop exits and the process returns 0. Gated on `feature = "cpurender"` (no-op otherwise). Logs a warning if `last_frame` is still `None` (empty DOM), an error if encoding / IO fails. Unlocks golden-image CI testing: `AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=actual.png ./my_app && diff actual.png reference.png` — no JSON harness, no full E2E pipeline.

mobile-check-all.sh still ALL 5 GREEN (~24s total).

### Tick — scripts/mobile-check-all.sh: cargo check across all 5 mobile targets

`scripts/mobile-check-all.sh` runs `cargo check --target $T -p azul-dll --no-default-features --features 'std,logging,link-static,a11y'` across **aarch64-apple-ios / aarch64-apple-ios-sim / x86_64-apple-ios / aarch64-linux-android / x86_64-linux-android** and emits a PASS/FAIL summary. Source-level only (cargo check doesn't link, so iOS targets succeed even without the iOS SDK installed). macOS-bash-3.2 compatible (no `declare -A`). Current state: **all 5 targets PASS**, total ~9 s on a warm cache. CI-friendly: exit 0 iff every target checks clean.

### Tick — Android KeyEvent → VirtualKeyCode (#12 follow-up)

`drain_input` now collects `KeyEvent` alongside `MotionEvent`. New `map_keycode(Keycode) -> Option<VirtualKeyCode>` translates the 16 most common navigation/editing keycodes (Enter, NumpadEnter, Space, Tab, Escape, Del→Back, ForwardDel→Delete, Dpad{Left,Right,Up,Down}, Shift/Ctrl/Alt left+right). For each key event we snapshot `previous_window_state`, update `keyboard_state.current_virtual_keycode` + `pressed_virtual_keycodes`, then `process_window_events(0)`. State-diff fires `HoverEventFilter::VirtualKeyDown` / `VirtualKeyUp` from the diff. Letter keys still arrive via the soft keyboard's text-input path (KeyCharacterMap → unicode follow-up). cargo check Android GREEN (11.43s).

### Tick — iOS drawRect → CGImage → CALayer.contents blit (#8)

`extern "C" fn draw_rect` is no longer empty. Flow:
1. Read the AZUL_IOS_WINDOW singleton.
2. If `frame_needs_regeneration`, call `window.regenerate_layout()` (populates `cpu_backend.last_frame`).
3. Wrap the `AzulPixmap` RGBA8 bytes in a `CGDataProvider` and pass to `CGImageCreate(width, height, 8 bpc, 32 bpp, width*4 bpr, deviceRGB, kCGImageAlphaPremultipliedLast | kCGBitmapByteOrderDefault, provider, NULL, false, kCGRenderingIntentDefault)`.
4. `[[this layer] setContents: cgimage]`.
5. Release CGImage, CGDataProvider, CGColorSpace.

New `#[link(name = "CoreGraphics", kind = "framework")]` block declares the six CG functions needed. Constants for the bitmap-info flags (`PremultipliedLast | ByteOrderDefault` and `RenderingIntentDefault`) live at module scope. cargo check aarch64-apple-ios GREEN (35.86s); aarch64-linux-android still GREEN (0.58s no-op). Sprint C-iOS pixmap blit closes; iOS Phase 2 is now structurally complete — once Xcode installs and a binary actually links, drawRect will produce real pixels.

### Tick — Sprint M Android JNI bridge for GestureDetector (#17)

Two artifacts land:
- `scripts/android/NativeGestureBridge.java` — `GestureDetector.SimpleOnGestureListener` + `ScaleGestureDetector.OnScaleGestureListener` + a custom two-finger rotation detector. Each callback dispatches to a `private static native nativeOn<Verb>(long nativePtr, ...)` JNI method. The `nativePtr` is the AndroidWindow address passed in at construction. Compiles outside Gradle with `javac -source 11 -target 11 -classpath $ANDROID_HOME/platforms/android-34/android.jar` and packs into `classes.dex` via `d8`. The Java side never holds static state — `nativePtr` is the only cookie.
- `dll/src/desktop/shell2/android/mod.rs::jni_bridge` — five `#[no_mangle] extern "system"` symbols matching the Java JNI lookup names: `Java_com_azul_gesture_NativeGestureBridge_nativeOn{DoubleTap,LongPress,Swipe,Pinch,Rotation}`. Each cast `native_ptr: i64` back to `&mut AndroidWindow` and `inject_native_gesture(NativeGestureEvent::*)`. Direction constants in the Java side mirror `GestureDirection`'s `#[repr(C)]` 0-3 ordering.

cargo check --target aarch64-linux-android still GREEN (13.38 s). The wire is complete from `setOnTouchListener` → `onDoubleTap`/`onFling`/`onScale`/2-finger rotation → JNI → `inject_native_gesture` → `clear_native_gesture` at end-of-frame. Activation requires the Java side to actually be loaded by `NativeActivity` (current AndroidManifest.xml uses pure NativeActivity with `android:hasCode="false"`, so the .dex isn't shipped). Wire-up in build-android.sh is a follow-up tick.

### P1.1 — rust-fontconfig iOS + Android arms landed 2026-05-19

User-owned `/Users/fschutt/Development/rust-fontconfig` gains the two missing mobile arms (commit `ea0107a` on master). SUPER_PLAN_2 §0 + research/05 §1.5 punch list is now closed at the *source* level — full runtime verification still needs Xcode/emulator.

- `lib.rs::OperatingSystem` adds `IOS` + `Android` variants; `current()` resolves `target_os = "ios"` → `IOS` and `target_os = "android"` → `Android` (previously both fell through to `Linux`, which explains why `FcFontCache` was empty on every mobile build).
- `lib.rs::FcFontCache::build_inner` gains an iOS arm calling `mobile_ios::copy_available_font_urls()` and an Android arm calling `FcScanDirectoriesInner` with `["/system/fonts", "/product/fonts", "/system_ext/fonts", "/data/fonts"]`. Vendor partitions cover Samsung One UI / MIUI / EMUI OEM-specific families.
- New `src/mobile_ios.rs` — `extern "C"` wrappers around `CTFontManagerCopyAvailableFontURLs` + `CFArrayGetCount/ValueAtIndex` + `CFURLGetFileSystemRepresentation`. Gated on `(target_os = "ios", feature = "std", feature = "parsing")`. Direct CoreFoundation FFI, no `core-foundation` / `core-text` crate dep — keeps the rust-fontconfig dep tree tight.
- `multithread.rs::scout_thread` branches to CoreText on iOS (the async-registry path); the per-directory walk continues to drive desktop + Android. iOS scout publishes paths via new `publish_ios_font_urls` helper that mirrors the desktop per-directory merge.
- `config.rs::{system_font_dirs, font_directories, common_font_families}` exhaustive on the two new variants. iOS `common_font_families` covers SFNS/SFNSDisplay/SFUI variants (the actual filename prefixes Apple ships) + Helvetica Neue / Avenir / Menlo / SF Mono. Android covers Roboto / Roboto Flex / Noto Sans / Droid Sans + the Mono variants.

`bash scripts/mobile-check-all.sh` GREEN across all 5 targets (13/8/8/8/8 s; previously 1/0/1/0/0 s warm-cache — the new rust-fontconfig source actually rebuilds this run). No regressions in azul-dll. Runtime verification (≥ 200 families on iOS sim, ≥ 30 on Android emulator) deferred until iOS Xcode + Android emulator land — `cargo check` only confirms the compile gate.

### P1.2 — PermissionManager core landed 2026-05-19

`layout/src/managers/permission.rs` (~400 LOC including tests) lands the cross-platform half of the "permission-as-DOM" architecture (SUPER_PLAN_2 §1.5 + research/08). Pure-rust, no platform deps — lives in `azul-layout` not `dll/extra/` per the §0.5 carve-out for state-only managers. `pub mod permission;` added to `managers/mod.rs`.

- `Capability` (field-less, `#[repr(C)]`) — 18 variants covering camera / mic / screen capture / geo / biometric / motion / photos / contacts / calendar / reminders / notifications / bluetooth / nearby-wifi / local-network / ATT. Parameters like `facing` / `accuracy` / `mode` move onto the bearing `NodeType` (so changes don't force a re-prompt).
- `PermissionQuality` (`#[repr(C)]`) — `Full | Reduced` (precise location vs approximate; full library vs "Selected Photos").
- `PermissionState` (`#[repr(C, u8)]`) — `NotDetermined | Requested | Granted{quality} | Denied | Restricted | EphemeralGranted{until_app_close}`. `is_granted()` accessor covers both granted variants.
- `PermissionDiffEvent` (`#[repr(C, u8)]`) — `Subscribe{cap, node_id} | Release{cap} | Reconfigure{cap}`. `Reconfigure` is reserved for future `CameraPreview` facing-change semantics; currently never emitted.
- `PermissionManager` — `BTreeMap<Capability, CapabilityEntry>` + pending-event queue. Refcount-based: first subscriber (0 → 1) emits `Subscribe`; last release (1 → 0) emits `Release`. `force_release` exists for OS-level revocation paths. `diff_layout(closure)` is the entry point the layout pass will call once `NodeType::GeolocationProbe` etc. land — closure-shaped to avoid pulling `StyledDom` into this manager and re-creating the dep cycle.
- 7 unit tests cover: subscribe/release round-trip; refcount math under multiple subscribers; `force_release` for OS revocation; `set_status` returning a change flag; full diff_layout pass with a probe appearing then disappearing across two frames; re-subscribe after a Release cycle re-emits a Subscribe (so the platform layer re-issues the native prompt).

`cargo test -p azul-layout --lib permission::` — 7/7 pass. `bash scripts/mobile-check-all.sh` GREEN across all 5 targets (12/12/11/11/11 s).

Open follow-ups: (a) the platform-stub layer at `dll/src/desktop/extra/permission/{ios,android,macos,linux,windows}.rs` consumes `PermissionDiffEvent::Subscribe / Release` and issues the matching native call; (b) the `NodeType::GeolocationProbe` / `CameraPreview` / `SensorProbe` variants will close the loop — `diff_layout`'s closure will then enumerate them from the styled DOM. Both follow-ups are queued.

### Tick — P1.2 platform-stub scaffold (dll/desktop/extra/permission/)

Lands the second half of the §0.5 split: cross-platform state lives in `azul-layout`, all platform-specific code lives in `dll/src/desktop/extra/<feature>/`. New tree:

- `dll/src/desktop/extra/mod.rs` — top-level `pub mod permission;` (more features to follow: camera, geo, biometric, sensors, …).
- `dll/src/desktop/extra/permission/mod.rs` — `apply_diff_events(events: &[PermissionDiffEvent])` dispatcher cfg-routed to the right OS arm; `probe_status(Capability) -> PermissionState` sync read used by `CallbackInfo::get_permission_status`.
- Five platform stubs (`ios.rs`, `android.rs`, `macos.rs`, `linux.rs`, `windows.rs`), each carrying a `handle_event` no-op + `probe_status` returning `NotDetermined`, plus a header comment summarizing the native API that should land in the follow-up tick (e.g. iOS → `AVCaptureDevice.requestAccess`, Android → `ActivityCompat.requestPermissions` via JNI, macOS → reuse-iOS-via-cfg(any), Linux → xdg-portal/ashpd, Windows → `AppCapabilityAccess.CheckAccessAsync`).
- `dll/src/desktop/mod.rs` registers `pub mod extra;` between `display` and `file`.

`bash scripts/mobile-check-all.sh` GREEN across all 5 targets (6/5/6/4/4 s). No new deps; the stubs only import `azul_layout::managers::permission::*`. Next tick will land the `LayoutWindow → take_pending_events() → extra::permission::apply_diff_events` wire-up in the layout pass.
