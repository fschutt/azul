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
