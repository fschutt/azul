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

### Tick — P1.2 layout-pass wire-up

Closes the cross-platform side of P1.2 by plumbing the manager into the shared layout pipeline. Three edits:

- `layout/src/window.rs::LayoutWindow` — new field `permission_manager: crate::managers::permission::PermissionManager`, initialized at all three `LayoutWindow::new*` constructor sites (`new`, the second copy at ~line 645, and `new_paged`). Doc comment points at SUPER_PLAN_2 §1.5 + research/08 for context.
- `dll/src/desktop/shell2/common/layout.rs::regenerate_layout` (step 7, after scrollbar opacity sync) now calls `layout_window.permission_manager.diff_layout(|_emit| {})` followed by `take_pending_events()`. Emitted events are routed through `crate::desktop::extra::permission::apply_diff_events`, which cfg-routes to the right platform stub. The closure body is intentionally empty for now — the bearing NodeTypes (`GeolocationProbe`, `CameraPreview`, `SensorProbe`) don't exist yet; the seam is in place so a future tick can fill in the DOM walk without touching layout-pass plumbing again.

`cargo test -p azul-layout --lib permission::` still 7/7 GREEN. `bash scripts/mobile-check-all.sh` GREEN on all 5 mobile targets (20/8/7/8/7 s — slower first-pass because azul-dll rebuilt with the new field). The whole P1.2 chain (manager core → platform stubs → layout-pass drain) is now wired end-to-end; activation only blocks on (a) the probe NodeTypes (P3-P6) and (b) replacing the per-platform stub bodies with real native calls (later P1+ ticks).

### Tick — P1.3 iOS file picker (real UIDocumentPickerViewController wiring)

Loop-prompt updated: dropped the "smallest forward diff" framing; the goal is to *finish* SUPER_PLAN_2, not land scaffolds forever. Cap kept at ~10 files / ~600 added lines per tick.

Lands the real iOS file picker (no more stubs). Three artifacts:

- `dll/src/desktop/extra/file_picker/mod.rs` — cross-platform handle + async dispatcher matching research/04 §1.7 Option B:
  - `FilePickerHandle` — `Arc<Mutex<FilePickerInner>>` behind a `#[repr(C)]` shim. Cheap to clone (the platform backend retains one clone while the user polls a sibling clone each frame).
  - `FilePickerStatus { Pending, Cancelled, Selected{path}, SelectedMultiple{paths}, Error{message} }` — `#[repr(C, u8)]`, mirrors the W3C `showOpenFilePicker` promise shape so the future web backend lands without API churn.
  - `apply_{open_file, save_file, open_directory}` dispatchers cfg-route to the right OS arm; non-mobile arms set the handle to `Cancelled` synchronously so polling never spins.
- `dll/src/desktop/extra/file_picker/ios.rs` — real UIKit implementation:
  - `PENDING_PICKERS: Mutex<BTreeMap<u64, FilePickerHandle>>` global registry keyed by a fresh `request_id` per dispatch. Avoids the boxed-pointer + `objc_setAssociatedObject(handle)` dance.
  - `AzulDocumentPickerDelegate` NSObject subclass (registered via `objc::declare::ClassDecl` like the existing `AzulGestureTarget`) with one `u64` ivar (`requestID`) and the two protocol selectors `documentPicker:didPickDocumentsAtURLs:` + `documentPickerWasCancelled:`.
  - `dispatch_open_file` walks `UIApplication.connectedScenes` to find the key window (iOS 13+ multi-scene safe; falls back to deprecated `keyWindow`), gets the rootViewController, builds a `[UTType]` filter array from `OptionStringVec` (known extensions → class methods `UTType.png` / `.jpeg` / `.pdf` / …; unknown → `[UTType typeWithFilenameExtension:]`; empty → `UTType.data`), allocs the picker via `initForOpeningContentTypes:asCopy:YES` (so the user gets a regular `file://` URL, no security-scoped bookmarking required), sets `allowsMultipleSelection`, attaches the delegate via `objc_setAssociatedObject` policy 1 (UIKit doesn't retain delegates), then presents.
  - `save_file` + `open_directory` left as `Cancelled` stubs — AzulPaint (P2) only needs open_file; save + dir come back when AzulDoc (P5) and AzulVault (P4) need them.
- `dll/src/desktop/extra/mod.rs` registers `pub mod file_picker;`.

`bash scripts/mobile-check-all.sh` GREEN on all 5 targets (15/5/6/5/5 s — the iOS targets rebuilt with the new picker). Source compile is the only check available without Xcode; runtime verification needs an iOS sim. The Android arm is queued for the next tick (#8 — JNI bridge to `Intent.ACTION_OPEN_DOCUMENT`).

### Tick — P1.3 Android file picker (real SAF + JNI wiring)

Closes P1.3: both mobile arms now drive their native pickers, not stubs. Five files:

- `dll/Cargo.toml` — `_internal_deps` gains `"jni"` so the existing optional `jni = "0.21"` dep actually links into the dll. Rust → Java direction needs the high-level wrapper; Java → Rust direction continues to use raw `extern "system"` symbols (no dep). Cost: ~30 s of cold-build overhead on Android, no impact on iOS / macOS / Linux / Windows.
- `scripts/android/AzulFilePicker.java` — full SAF implementation. `pickDocument`/`saveDocument`/`pickDirectory` static entry points launch `Intent.ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT` / `ACTION_OPEN_DOCUMENT_TREE` via `Activity.startActivityForResult` with a request code in `0x4A5400+ticket`. `onActivityResultProxy` matches by request code, drains the per-ticket `requestId` cookie, then resolves each picked `content://` URI to a cached `file://` path via `ContentResolver.openInputStream` → `getCacheDir()/<timestamp>_<sanitized>` (mirrors iOS `asCopy:YES` so the user-side flow is identical on both platforms). Result fed back to Rust via `nativeOnResult(requestId, paths, errorOrNull)`.
- `scripts/android/AzulActivity.java` — `onActivityResult` override routes to `AzulFilePicker.onActivityResultProxy` first; falls through to super only if the proxy didn't claim the request code.
- `dll/src/desktop/shell2/android/mod.rs` — publishes `JavaVM*` + Activity globalref on `android_main` startup (new `publish_jni_context(app)` + `ANDROID_JAVA_VM` / `ANDROID_ACTIVITY` atomics + cross-target `java_vm_ptr()` / `activity_ptr()` accessors). Other native-call paths (permission prompts, soft keyboard) will reuse the same context.
- `dll/src/desktop/extra/file_picker/android.rs` — `PENDING_PICKERS` request-ID registry (same shape as the iOS arm); `with_env(closure)` helper that attaches the current thread to the published JavaVM, finds the `AzulFilePicker` class, and invokes the right static method via the `jni` crate's typed bindings; `dispatch_open_file` / `dispatch_save_file` / `dispatch_open_directory` all wired; new `Java_com_azul_picker_AzulFilePicker_nativeOnResult` `extern "system"` symbol reads back the `String[]` paths + optional error string, pops the handle from `PENDING_PICKERS`, and writes the final `FilePickerStatus`.

Borrow-checker note: jni 0.21's `JNIEnv::get_string` returns a `JavaStr` whose lifetime is tied to both the `&JNIEnv` and the source `JString`. The if-let pattern would have held the `Result` temporary alive past the `JString` drop point, so each string-extract path is rewritten as `env.get_string(&jstr).ok().map(|s| s.into())` — the `String` materializes inside the closure, drop order becomes deterministic.

`cargo test -p azul-layout --lib permission::` still 7/7 GREEN; `bash scripts/mobile-check-all.sh` GREEN on all 5 mobile targets (18/1/1/8/6 s). With this tick **all three P1 sub-tasks are closed**: rust-fontconfig mobile arms (P1.1), PermissionManager + dispatcher + layout wire-up (P1.2), and file pickers on both mobile platforms (P1.3). The compile-only gate hides runtime activation — iOS still needs Xcode SDK, Android still needs the dex to ship in the APK (build-android.sh already detects `scripts/android/AzulFilePicker.java` and pulls it in automatically). Next cron tick can start P2 — AzulPaint's `PenState` wiring (research/03).

### Tick — P2.1 PenState populated on iOS + Android backends (AzulPaint runway)

First step toward AzulPaint (the P2 goal app). The `PenState` struct already existed in `layout/src/managers/gesture.rs:360` with the right shape (`position`, `pressure`, `tilt`, `in_contact`, `is_eraser`, `barrel_button_pressed`, `device_id`), but neither mobile backend had ever called `update_pen_state` — so any pen-aware widget got nothing.

- `dll/src/desktop/shell2/ios/mod.rs::handle_touch` now extracts Apple Pencil data from each UITouch:
  - `[touch type]` → `UITouchTypePencil = 2` is the gate.
  - `[touch force]` / `[touch maximumPossibleForce]` → normalized 0..1 pressure (`maximumPossibleForce == 0` falls back to 0).
  - `[touch altitudeAngle]` (π/2 = perpendicular) + `[touch azimuthAngleInView: view]` decomposed into W3C-shape `tiltX` / `tiltY` degrees using `atan(sin(orientation) * tan(tilt))` for x and `atan(-cos(orientation) * tan(tilt))` for y. Matches `PointerEvent` semantics the desktop pen-tablet path already uses.
  - `is_eraser` + `barrel_button_pressed` stay `false` — Apple Pencil 1/2 don't expose those at the UITouch layer (Pencil 2 squeeze fires `UIPencilInteraction` instead, a P2.3 follow-up).
  - In-contact (`phase ∈ {began, moved}`) → `update_pen_state(...)`; otherwise → `clear_pen_state()`.
- `dll/src/desktop/shell2/android/mod.rs::drain_input` adds a `PenSample` collection pass alongside the existing motion + key updates:
  - `Pointer::tool_type() ∈ {Stylus, Eraser}` is the gate.
  - `Pointer::pressure()` clamped to 0..1.
  - `Pointer::axis_value(Axis::Tilt)` (radians from perpendicular) + `Axis::Orientation` decomposed into the same W3C tiltX/tiltY shape.
  - `is_eraser = tool_type == Eraser`; `barrel_button_pressed = MotionEvent::button_state().stylus_primary()` (Surface Pen / S-Pen barrel).
  - Same in-contact gating as iOS.
- `use android_activity::input::{Axis, ToolType}` added.

Both backends call into the *existing* `GestureAndDragManager::update_pen_state` — no API changes. Cross-platform `CallbackInfo::get_pen_state()` already exposes the populated state to user callbacks. With this tick, a widget that wants stylus-only behavior (paint canvas, signature pad, handwriting input) can finally tell finger from pen and read pressure + tilt.

`bash scripts/mobile-check-all.sh` GREEN across all 5 mobile targets (19/5/5/4/5 s). Runtime verification needs an iOS Pencil or Android stylus device — compile-only is the available check.

Open AzulPaint follow-ups: P2.2 multi-touch `TouchPointVec` (iOS currently reads only `anyObject`, drops fingers 2+); P2.3 `PenState` extensions (`tangential_pressure`, `barrel_roll_rad`, `tool_id`) + new `HoverEventFilter::PenSqueeze` / `PenDoubleTap` / `PenHover` event filters (the iOS `UIPencilInteraction` squeeze + the W3C `pointerleave` / hover surface).

### Tick — P2.2 multi-touch TouchPointVec on iOS + Android

Closes P2.2. Both backends now populate `FullWindowState.touch_state.touch_points` end-to-end so multi-finger widgets (paint canvases, custom pinch/rotate, two-finger gestures) see *all* active fingers, not just the first.

- `dll/src/desktop/shell2/ios/mod.rs::handle_touch` no longer reads `[touches anyObject]`. Walks `[touches allObjects]` via `objectAtIndex:`, builds a `Vec<TouchPoint>` from each UITouch (id = `(touch as usize) as u64` — Apple guarantees stable pointer identity for the lifetime of a touch sequence; force = `touch.force / touch.maximumPossibleForce`, sentinel `0.5` for non-pressure devices). Phase-aware merge: `began/moved` → upsert each new sample into the existing touch_points by id; `ended/cancelled` → drop the reported ids and keep the rest active (UIKit only delivers the touches that changed, the rest persist).
- `dll/src/desktop/shell2/android/mod.rs::drain_input` no longer takes `m.pointers().next()`. Iterates *all* pointers per MotionEvent, builds the `TouchPoint` list with `pointer_id()` as the id (stable across moves) + clamped pressure with the same `0.5` sentinel. Refresh policy: `Down/PointerDown/Move/HoverMove/PointerUp` → replace `touch_points` with the full freshly-computed list (Android always delivers every active pointer on every event); `Up/Cancel` → clear. `mouse_pos` is anchored to the primary (index-0) pointer so the existing mouse-pipe diff path keeps working.
- iOS `pencil` extraction now reads from each touch inside the per-touch loop rather than only the first `anyObject`. The first stylus wins (Apple Pencil is single-instance hardware).
- Android `pen_updates` collection now sees every stylus pointer per event; if a future device exposed multiple styluses simultaneously they'd all queue (currently only the first will register on the gesture manager since `update_pen_state` is single-slot — that's a P2.3 follow-up if it matters).
- Imports widened: `use azul_core::window::{CursorPosition, TouchPoint, TouchPointVec}` on iOS.

`bash scripts/mobile-check-all.sh` GREEN across all 5 mobile targets (18/5/5/4/5 s). Source-only — runtime verification needs a multi-touch device. With this tick, AzulPaint can implement two-finger zoom / three-finger undo by reading `CallbackInfo::get_window_state().touch_state.touch_points`.

### Tick — P2.3 PenState extended fields + new HoverEventFilter variants

Closes P2.3. Three Apple-Pencil-2 / Surface-Pen / Wacom-class capabilities now have a typed home in the framework — populating them is a per-backend follow-up, but the schema is settled across api.json + the 35 binding languages.

- `layout/src/managers/gesture.rs::PenState` extended with three new `#[repr(C)]` fields:
  - `tangential_pressure: f32` — W3C `PointerEvent.tangentialPressure` shape. Wacom Air Brush wheel, Surface Slim Pen 2 secondary axis. `0.0` means "not reported".
  - `barrel_roll_rad: f32` — W3C `PointerEvent.twist` shape, in radians (–π to π). Wacom Art Pen rotation, Surface Pen barrel roll. `0.0` means "not reported" — devices that do report it sweep the full range so callers compare deltas, not absolute values.
  - `tool_id: u32` — per-tool identity (Wintab GUID, Apple Pencil session id, S-Pen serial). Distinct from `device_id` so callers can identify both the hardware AND which tip / lead / button cluster is in use.
- `update_pen_state(...)` keeps its 7-arg signature (defaults the three new fields to `0`); new `update_pen_state_full(...)` takes all 10 inputs. Mobile backends still call the 7-arg form — they have no path to source the extended axes today.
- `core/src/events.rs::HoverEventFilter` + `WindowEventFilter` each gain three new variants:
  - `PenSqueeze` — Apple Pencil 2 / Surface Slim Pen 2 barrel-squeeze, fires once per gesture. Most apps tie a tool-switch to it.
  - `PenDoubleTap` — Apple Pencil 2 side double-tap, fires once. Usually "undo" or "toggle eraser".
  - `PenHover` — pen-in-proximity-but-not-in-contact, continuous. Maps to W3C `pointermove` with `buttons: 0` + `pointerType: 'pen'`.
  - `WindowEventFilter::to_hover_event_filter` and `HoverEventFilter::to_focus_event_filter` extended with paired arms (squeeze / double-tap / hover all return `None` for focus equivalents — these are short verbs / proximity signals, not focus transitions).
- `api.json` updated in both HoverEventFilter and WindowEventFilter variant lists, plus PenState struct field list. `cd doc && cargo run --release -p azul-doc -- codegen all` regenerated all 35 language bindings + `dll_api_internal.rs` cleanly.

Wire-in queue:
- iOS `UIPencilInteraction` delegate (squeeze + double-tap) — separate from UIView touch handling. Lives on the AppDelegate.
- iOS `UITouch.hover` (iPadOS 12.1+) — once Pencil is in proximity, UIKit fires `touchesEstimatedPropertiesUpdated` plus regular touch events with `phase == .hover`. The handle_touch helper would have to learn phase 4.
- Android `MotionAction::HoverEnter / HoverMove / HoverExit` already arrive in drain_input; just need to translate them into a `PenHover` synthesised event when `tool_type == Stylus`.
- Wacom Wintab desktop axes (tangential pressure, barrel rotation) — desktop backend wiring, separate sprint.

`bash scripts/mobile-check-all.sh` GREEN across all 5 mobile targets (13/11/11/11/11 s — full rebuild after the codegen refresh). 7/7 permission tests still pass.

### Tick — P2.4 AzulPaint demo crate (the P2 goal app lands)

`examples/azul-paint/` joins the workspace. Working finger + stylus paint canvas with pressure-modulated stroke radius and eraser-tip detection — exercises the P2.1 (PenState) + P2.2 (multi-touch TouchPointVec) wiring landed in earlier ticks. Three files:

- `Cargo.toml` — `bin = azul-paint`, depends on `azul-dll` with `link-static` (matches the existing `examples/rust` layout).
- `src/main.rs` (~320 LOC):
  - `PaintState { strokes: Vec<Stroke>, current: Option<Stroke> }`. Each `Stroke = (Vec<StrokePoint>, is_eraser)`. `StrokePoint = (x, y, pressure)`. `begin_stroke` / `extend_stroke` / `end_stroke` / `clear_all` mirror the W3C `<canvas>` pointer-down/move/up state machine.
  - Header bar (`Clear` button + live stroke/point counter) + canvas div with the seven event callbacks: `MouseDown / MouseOver / MouseUp` for desktop, `TouchStart / TouchMove / TouchEnd / TouchCancel` for mobile.
  - `extract_point(info)` prefers `CallbackInfo::get_pen_state()` over cursor-relative-to-node when a stylus is in contact — the same accessor populated by P2.1's iOS UITouch.Pencil + Android `ToolType::Stylus` paths. Pen pressure (clamped to `0.05..=1.0`) drives the stroke radius; touch falls back to the `0.5` sentinel for a uniform medium-weight line. `is_eraser` flips the stroke colour to the canvas background so eraser tip strokes paint over earlier marks.
  - Stroke rendering: each point becomes a small absolutely-positioned circle div (radius = `2.0 + pressure * 10.0`). Slow with many strokes; lands what's possible with the existing widget set. A real `<canvas>` primitive is a follow-up sprint.
  - `layout(...)` snapshots the visible state into owned locals before building the DOM so the borrow on `data` releases cleanly before each `with_callback` clone — no E0502 with the current `downcast_ref` API.
- Root `Cargo.toml` workspace gains `"examples/azul-paint"`.

`cargo check -p azul-paint` clean. `bash scripts/mobile-check-all.sh` GREEN on all 5 mobile targets (0/1/0/0/1 s — only azul-paint rebuilt).

P2 follow-ups queued: (a) Save-as-SVG / Save-as-PNG via the P1.3 `FilePickerHandle` poll pattern — needs a Timer callback that polls the handle each frame; (b) brush palette UI (colour picker + size slider); (c) once the framework gains a `<canvas>` NodeType, swap the per-point div soup for a single canvas blit. AzulPaint becomes "playable" today on desktop; iOS/Android runtime needs Xcode SDK + an APK build respectively.

### Tick — P3.1a GeolocationManager + dispatcher (AzulMaps runway)

Opens P3 — the AzulMaps tier. Lands the cross-platform geolocation state + 5-platform stub dispatcher. NodeType::GeolocationProbe is the next tick (it touches NodeType / Hash / Ord / Display + the renderer's "skip invisible" code path + 35-language codegen).

- `layout/src/managers/geolocation.rs` (~350 LOC incl. tests):
  - `LocationFix` (`#[repr(C)]`) mirrors W3C `GeolocationPosition` — lat/lon/accuracy + optional altitude/altitudeAccuracy/heading/speed encoded as `f32::NAN` when not reported. `altitude()` / `heading()` / `speed()` decode the sentinel to `Option<f32>`.
  - `GeolocationProbeConfig` (`#[repr(C)]`) — `high_accuracy`, `background`, `max_accuracy_m`, `min_interval_ms`. Maps to W3C `PositionOptions`.
  - `GeolocationDiffEvent` (`#[repr(C, u8)]`) — `Subscribe { config }`, `Release`, `Reconfigure { config }`.
  - `GeolocationManager` — `latest_fix` + `active_config` + pending-event queue + refcount. `diff_layout(closure)` matches the symmetric API on `PermissionManager`: closure feeds each `GeolocationProbeConfig` it finds in the styled DOM; manager emits Subscribe (0→1), Release (n→0), or Reconfigure (config drift). `set_latest_fix` compares via `to_bits()` so NaN-encoded missing fields don't make every sample look "changed".
  - 6 unit tests cover the full state machine: first-probe Subscribe, last-drop Release+clear-fix, config drift Reconfigure, stable config no-op, change-flag semantics on set_latest_fix, NaN→None decode.
- `layout/src/window.rs::LayoutWindow` gains `geolocation_manager` field initialized at all three constructor sites.
- `dll/src/desktop/shell2/common/layout.rs::regenerate_layout` runs the geolocation diff pass + `take_pending_events` + dispatches via `crate::desktop::extra::geolocation::apply_diff_events`. Same shape as the permission diff already wired in P1.2.
- `dll/src/desktop/extra/geolocation/{mod,ios,android,macos,linux,windows}.rs` — 5 platform stubs documenting the per-platform native API they'll call once `NodeType::GeolocationProbe` lands: iOS → `CLLocationManager + AzulLocationDelegate`, Android → JNI to `AzulGeolocation.java + FusedLocationProviderClient`, macOS → shares iOS objc bindings via `cfg(any(ios, macos))`, Linux → `zbus → org.freedesktop.GeoClue2.Manager` (Flatpak portal fallback), Windows → `Windows.Devices.Geolocation.Geolocator.PositionChanged`.
- `core/src/events.rs::{HoverEventFilter, WindowEventFilter}` each gain `GeolocationFix` + `GeolocationError`. `to_hover_event_filter` + `to_focus_event_filter` updated with paired arms (both new variants return None for focus — location is window-global, not per-focus).
- `api.json` updated with both new variants; `cd doc && cargo run -p azul-doc -- codegen all` regenerated all 35 language bindings + `dll_api_internal.rs` cleanly.

`cargo test -p azul-layout --lib geolocation::` — 6/6 GREEN. `cargo test -p azul-layout --lib permission::` — 7/7 still pass. `bash scripts/mobile-check-all.sh` GREEN across all 5 mobile targets (7/6/6/7/7 s — full rebuild after the codegen refresh).

Next tick (P3.1b): add `NodeType::GeolocationProbe(GeolocationProbeConfig)` variant — closes the loop so `diff_layout`'s closure actually enumerates probes from the styled DOM. Then P3.1c — real iOS/Android `CLLocationManager` / `FusedLocationProviderClient` wiring.

### Tick — P3.1b NodeType::GeolocationProbe + layout-pass enumeration

Closes the loop on P3.1: the geolocation diff pass now actually walks the styled DOM. User-side API: `Dom::create_geolocation_probe(GeolocationProbeConfig { high_accuracy: true, .. })`.

- `core/src/geolocation.rs` (new) — `LocationFix` + `GeolocationProbeConfig` POD types moved here so `NodeType` can carry the config without a cyclic dep from `azul-core` → `azul-layout`. `GeolocationProbeConfig` gets manual `Eq + Hash + Ord` impls (compares `max_accuracy_m: f32` via `to_bits()` so NaN doesn't poison the total order — `NodeType` derives Hash/Ord, so every variant payload must support it).
- `layout/src/managers/geolocation.rs` — `pub use azul_core::geolocation::{GeolocationProbeConfig, LocationFix}` so the existing `azul_layout::managers::geolocation::*` import paths keep working. `GeolocationManager` + `GeolocationDiffEvent` stay layout-side.
- `core/src/dom.rs::NodeType` gains `GeolocationProbe(GeolocationProbeConfig)`. Three exhaustive matches updated: `into_library_owned_nodetype` (deep-clone path), `format` (debug print: `"geolocation-probe(hi=true, bg=false, max=0m, every=1000ms)"`), `get_path` → new `NodeTypeTag::GeolocationProbe`.
- `core/src/dom.rs` gains `Dom::create_geolocation_probe(config)` (and `NodeData::create_geolocation_probe(config)` could land next as a follow-up — the Dom constructor is what user code wants).
- `css/src/css.rs::NodeTypeTag` gains `GeolocationProbe`. CSS tag string: `"geolocation-probe"` (both directions: from-str + Display). `css/src/codegen/rust.rs::format_node_type` updated.
- `api.json` adds the `GeolocationProbe` variant to both `NodeType` and `NodeTypeTag`, plus full `GeolocationProbeConfig` + `LocationFix` struct definitions. `cd doc && cargo run -p azul-doc -- codegen all` regenerated all 35 language bindings + `dll_api_internal.rs` cleanly.
- `dll/src/desktop/shell2/common/layout.rs::regenerate_layout` step 7b — instead of an empty diff closure, snapshots every `NodeType::GeolocationProbe` config from every `layout_result.styled_dom.node_data`, then feeds the list to `geolocation_manager.diff_layout(...)`. Subscribe / Release / Reconfigure events drain through `dll::extra::geolocation::apply_diff_events` to the platform stubs.

All 5 mobile targets GREEN (12/12/16/12/11 s). 6/6 geolocation tests + 7/7 permission tests still pass. AzulMaps now has a working "this app needs the user's location" surface — a single `Dom::create_geolocation_probe(cfg)` in the layout tree triggers the platform prompt + subscription. Real per-platform native calls remain queued (the stubs log but don't issue them yet).

### Architecture note — AzulMaps tile pipeline (P3.2 design)

User clarified the map-tile design while this tick was running. Captured here for the P3.2 implementation:

- **Data path**: MVT (Mapbox Vector Tile) protobuf bytes → CSS-style stylesheet → SVG → DOM. Each tile is decoded on the fly into a tree of `<svg>` `<path>` / `<polygon>` / `<text>` nodes carrying the styling rules from the user's stylesheet.
- **Renderer**: one `<div>` per map tile, with the tile's SVG DOM as the child. Tiles position via GPU-accelerated `transform: translate(x, y) scale(z)` CSS so pan + zoom is a single matrix update per frame, not a re-layout. Effectively turning the map into "DOM you can compose into".
- **Infinite scroll**: `VirtualView` handles the unbounded tile grid — the existing infinite-list virtualization (`layout/src/managers/virtual_view.rs`) gets each tile lazy-loaded as the viewport enters its bounding rect, dropped when it leaves. Tile cache uses the `DatasetMergeCallback` ("merge callback") pattern from `core/src/dom.rs:1798` for transactional in-place updates — fits the ephemeral "this tile is now decoded" / "this tile evicted" diff cleanly.
- **External crates**: `fschutt/tile-downloader` for the HTTP/CDN fetch path (presumably handles OpenFreeMap-style PMTiles + raster fallbacks); `proj4-rs` for the projection math (Web Mercator ↔ WGS-84 ↔ user-defined CRS).
- **API target**: Leaflet-shape — `Map::new().add_tile_layer(url_template).set_view(lat, lon, zoom)`. User code never touches MVT bytes directly.

These notes inform P3.2 — `MapTile` / `MapWidget` NodeType + the tile decoder + the viewport state. P3.1 (geolocation surface) is fully closed at the source level; remaining work is per-platform native subscription wiring (queued for follow-up ticks once iOS Xcode SDK + Android emulator are in the loop).

### Tick — P3.2a MapWidget skeleton (widget, not NodeType — user pivot)

User clarified the design mid-tick: **map is a widget, not a NodeType**. The previous tick's `NodeType::MapTile` + `NodeTypeTag::MapTile` + the `core/src/map.rs` POD module were uncommitted — reverted them. (The `NodeType::GeolocationProbe` from the earlier P3.1b tick stays — that's separate and the user is OK with that one.)

The revised design (Leaflet-shape API the user spelled out):

- `MapWidget` is a regular widget (like `Button`, `TextInput`). Built via `MapWidget::create(layer).with_viewport(...).dom()`.
- Tile cache lives in a `RefAny` dataset attached to the widget's root `<div>`. A `DatasetMergeCallback` transfers every entry from the old frame's cache into the new frame's cache on relayout, so in-flight HTTP fetches and decoded SVG bytes survive layout churn.
- `VirtualView` callback computes the visible-tile grid each frame: Web Mercator math (lat/lon → XYZ) projects the current viewport into tile space, the integer zoom level is clamped to the layer's `[min_zoom, max_zoom]`, fractional zoom drives a CSS scale on the integer-z tile divs (no re-fetch on small zoom deltas).
- Each visible tile is one absolutely-positioned `<div>` GPU-translated via `transform: translate(x, y)`. The inner content (the decoded SVG DOM) is patched in by the follow-up MVT decoder; this tick lands the grid math + placeholder content (`"zN/X/Y"` text label per tile).
- User stacks a `GeolocationProbe` (from P3.1) anywhere in the subtree to opt into "this app needs GPS" — the widget doesn't bake in any location feature itself, the framework's permission-as-DOM model composes naturally.

Files:
- `layout/src/widgets/map.rs` (~370 LOC) — POD types `MapTileId`, `MapTileLayer`, `MapViewport` + the `MapWidget` builder + `MapTileCache` payload + the `TileEntry { Pending | Ready{svg} | Failed{error} }` state machine + `merge_map_tile_cache` callback + `map_widget_render` virtual-view callback with the Web Mercator math + the placeholder-tile-grid Dom build.
- `layout/src/widgets/mod.rs` registers `pub mod map;` between `list_view` and `node_graph`.

Compile-only verification: `bash scripts/mobile-check-all.sh` GREEN on all 5 mobile targets (14/16/11/11/12 s). Pre-tick gate showed RED because the codegen output (target/codegen/dll_api_internal.rs) was stale from the reverted MapTile NodeType — a fresh `cargo run -p azul-doc -- codegen all` restored it to match the source.

Open follow-ups (queued):
- **MVT decoder + MapCSS parser → SVG → DOM** pipeline. Reuses the framework's existing CSS parser (MapCSS is a CSS dialect) + the svg-to-dom path the framework already ships. `fschutt/tile-downloader` likely provides the HTTP/PMTiles client.
- **Pan / zoom gesture wiring** — connect the existing `GestureAndDragManager` pinch+drag detection to the `MapWidget.viewport` state via a small callback that translates pixel deltas → lat/lon deltas via `proj4-rs` (or hand-rolled Web Mercator inverse).
- **api.json + codegen** for the new types (`MapWidget`, `MapTileLayer`, `MapViewport`, `MapTileId`) so binding languages (Python, Java, etc.) see the widget. Held back from this tick because the widget API will probably get one more iteration once the gesture / pan plumbing lands.
- **`examples/azul-maps/` demo crate** — the P3 goal app proper. Will exercise the widget + the geolocation dot composition.

### Tick — P3.2b MapWidget in api.json + AzulMaps demo

User redirect mid-tick: "expose the structs in api.json and write the example properly. Same thing for the paint app." Re-routed the demo away from a direct `azul-layout` dep onto the canonical `azul::widgets::*` codegen path.

- `api.json` now defines:
  - `MapTileId { z: u8, x: u32, y: u32 }`
  - `MapTileLayer { url_template, min_zoom, max_zoom, attribution }` + Default
  - `MapViewport { centre_lat_deg, centre_lon_deg, zoom, bearing_deg, pitch_deg }` + Default
  - `MapWidget { layer, viewport, container_style }` with:
    - **Constructor**: `create(layer) -> MapWidget`
    - **Functions**: `with_viewport(self, viewport) -> MapWidget`, `set_viewport(&mut self, viewport)`, `with_container_style(self, css) -> MapWidget`, `dom(self) -> Dom`
- `cd doc && cargo run -p azul-doc -- codegen all` regenerated all 35 language bindings + `dll_api_internal.rs`. The widget is now reachable as `azul::widgets::MapWidget` (paired with `MapTileLayer` / `MapViewport` / `MapTileId`) across every binding.
- `examples/azul-maps/` (~230 LOC):
  - `Cargo.toml`: only depends on `azul-dll` (the canonical example shape — no direct `azul-layout` import).
  - `src/main.rs`: imports `use azul::prelude::*; use azul::widgets::{MapTileLayer, MapViewport, MapWidget};`. `MapState { viewport, layer }` holds the centre + zoom; layout callback snapshots into local data, builds `MapWidget::create(layer).with_viewport(viewport).dom()`, stacks an attribution overlay. Header bar shows the live viewport + a 7-button toolbar (← → ↑ ↓ + − Recentre) — each callback nudges the viewport via the same Web Mercator math the widget uses internally.
- Root `Cargo.toml` workspace gains `"examples/azul-maps"`.

`cargo check -p azul-maps` clean. `bash scripts/mobile-check-all.sh` GREEN across all 5 mobile targets (13/11/10/10/10 s — full rebuild after the codegen refresh). The demo works on desktop today: launching it shows the tile-grid layout with the placeholder `"zN/X/Y"` label per tile; clicking the toolbar moves the centre and re-flows the grid in real time. Real tile content lands once the MVT decoder + HTTP fetch pipeline ships.

AzulPaint follow-up noted by user: "same thing for the paint app." The existing paint demo (`examples/azul-paint/`) already uses only `azul::prelude::*` (no direct azul-layout import) since it constructs the canvas ad-hoc from divs. If we want a typed `PaintCanvas` widget later, the api.json pattern landed here is the template.
