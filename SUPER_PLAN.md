# SUPER_PLAN — iOS + Android headless-window backends

**Branch:** `mobile-ios-android` (worktree at `/Users/fschutt/Development/azul-mobile`)
**Author:** autonomous Claude session, 2026-05-19
**Parent agent guidance:** "iOS first, then Android, install first what you need, we are on macOS, goal is to make it cross compile and depend not much on Apple / Java tooling."
**Coexists with:** another agent in `/Users/fschutt/Development/azul` on dll/web/remill. Don't touch that working tree; on cargo lock contention wait 60s.

---

## 0. Why a "headless window"

Both iOS and Android UI backends ultimately need to draw pixels into a native surface (UIView / ANativeWindow). The cheapest path that lets us **build and verify on macOS without an iPhone, Android device, full Xcode, or Android Studio** is the same CPU rendering pipeline the desktop `headless` backend already uses:

```
LayoutWindow → DisplayList → cpurender::render() → AzulPixmap (RGBA8) → {PNG | UIView | ANativeWindow}
```

`shell2::run::run()` already routes to `run_headless` when `AZ_BACKEND=headless` is set, even on iOS / Android targets. That means **we can run mobile-target binaries headlessly on macOS-host CI and verify pixel output**, without simulators. The native windowed path is the production target; the headless path is the regression harness.

This plan delivers both, in order:

1. Make the codepath compile for both targets (no warnings, no Apple/Java SDK dependency at `cargo check` time).
2. Wire the CPU render pipeline so a real iOS / Android device can blit AzulPixmap to screen.
3. Wire the events: touch, key, lifecycle.
4. Build a CLI bundler that produces a `.app` (iOS) / `.apk` (Android) without Xcode / Gradle.
5. Headless regression: golden-PNG snapshots produced by the iOS- and Android-target binaries on macOS host.

---

## 1. Worktree + Cargo target isolation

```
/Users/fschutt/Development/azul/          ← OTHER agent, dll/web/remill, do not touch
/Users/fschutt/Development/azul-mobile/   ← THIS worktree (branch mobile-ios-android)
  └── target/                             ← isolated, never collides with the main repo
```

Each git worktree gets its own `target/` automatically. No `CARGO_TARGET_DIR` needed. If the main agent rebases `master` ahead, we just `git fetch && git rebase origin/master` later.

**Conflict policy:** if `cargo build` fails with `Blocking waiting for file lock on package cache` or similar, sleep 60 s and retry once before raising the error.

---

## 2. Host-tool inventory (2026-05-19)

| Tool                    | Status                              | Action                                       |
|-------------------------|-------------------------------------|----------------------------------------------|
| `rustup` ios targets    | ✅ aarch64-apple-ios, sim, x86_64    | none                                         |
| `rustup` android targets| ❌ none installed                    | `rustup target add aarch64-linux-android x86_64-linux-android` |
| Xcode CLT               | ✅ `/Library/Developer/CommandLineTools` (macOS SDKs only) | — |
| Full Xcode + iOS SDK    | ❌ not installed                     | **prereq for `cargo build --target *-ios`**; document, do not block plan |
| `xcrun simctl`          | ❌ requires full Xcode               | iOS sim deployment gated until Xcode present |
| Android NDK             | ❌ not installed                     | install via `sdkmanager 'ndk;27.0.12077973'` |
| Android cmdline-tools   | 🟡 installing via brew cask          | `brew install --cask android-commandlinetools` |
| Android platform-tools  | 🟡 installing via brew cask          | `brew install --cask android-platform-tools` (gives `adb`) |
| JDK                     | ✅ `openjdk@17` via brew             | wire `JAVA_HOME=$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home` |
| `cargo-ndk`             | ✅ 3.5.4                             | use as default Android build wrapper         |

**iOS SDK gap is the only hard blocker for actually building iOS code.** The plan is staged so all the *code* lands and is gated only by `cfg(target_os="ios")` — when the user installs Xcode, `cargo build --target aarch64-apple-ios-sim -p azul-dll` will compile.

Where we cannot link, we substitute `cargo check --target aarch64-apple-darwin --cfg ios_offline_check` (a manual sanity-check) to keep working.

---

## 3. Sprint sequence

Each sprint has: **GOAL**, **FILES**, **GATE** (verifiable check), **REFERENCE** (line numbers in existing plans).

### Sprint A — Foundation & install (~30 min)

* **GOAL:** Worktree set up; SUPER_PLAN.md committed; Android tooling staged; `.cargo/config.toml` mobile linker entries added.
* **FILES:** `.cargo/config.toml`, `SUPER_PLAN.md`, `scripts/install-android-sdk.sh`, `scripts/check-prereqs-mobile.sh`.
* **GATE:** `scripts/check-prereqs-mobile.sh` exits 0 for Android (warns for iOS until Xcode is installed).

### Sprint B — iOS-1: compilation (~1 hr)

* **GOAL:** `cargo check --target aarch64-apple-ios-sim -p azul-dll` succeeds with iOS SDK present. **Without SDK**, the only acceptable failure is "iphonesimulator SDK not found", proving the code itself is correct.
* **FILES:**
  * `dll/src/desktop/shell2/mod.rs` — already declares `pub mod ios;` correctly (verified).
  * `dll/src/desktop/shell2/ios/mod.rs` — fix `INITIAL_OPTIONS` 5-tuple destructure to match `(RefAny, AppConfig, Arc<FcFontCache>, Option<Arc<FcFontRegistry>>, WindowCreateOptions)`. Currently `IOSWindow::new()` is called with the wrong arity.
  * `dll/build.rs:574` — replace `panic!()` from `check_tool("ios-deploy"...)` with a `println!("cargo:warning=...")`. ios-deploy is for device deploy only; simulator doesn't need it.
  * `dll/Cargo.toml` — verify `objc`, `objc-foundation`, `objc_id`, `objc_exception` are all gated under `cfg(target_os = "ios")`.
* **GATE:** `cargo check --target aarch64-apple-ios-sim -p azul-dll` outputs only the SDK-missing error (or succeeds if SDK present).

### Sprint C — iOS-2: CPU render pipeline (~2 hr)

* **GOAL:** `drawRect:` blits an `AzulPixmap` to the layer; `headless` backend behaviour fully reusable.
* **FILES:**
  * `dll/src/desktop/shell2/ios/mod.rs` — wire `CpuBackend` (mirroring `headless/mod.rs:130`).
  * `dll/src/desktop/shell2/ios/coregraphics.rs` (new) — `create_cgimage_from_rgba()` (see IOS_IMPLEMENTATION_PLAN.md §Phase 2 step 3).
  * `dll/src/desktop/shell2/ios/display_link.rs` (new) — `CADisplayLink` wrapper, ~30 lines.
* **GATE:** Code compiles. Manual: when an iOS device is available, a window opens and shows a non-blank pixmap.

### Sprint D — iOS-3: touch events (~1 hr)

* **GOAL:** First touch maps to mouse left button + hit test + `process_window_events(0)`.
* **FILES:**
  * `ios/mod.rs::touches_began/Moved/Ended/Cancelled` — translate `UITouch` → `FullWindowState::mouse_state` + `cpu_hit_tester.hit_test()`.
* **GATE:** Compiles; in a future iPhone-on-desk session, tapping highlights a button.

### Sprint E — iOS-4: build pipeline (CLI only) (~1 hr)

* **GOAL:** `scripts/build-ios.sh aarch64-apple-ios-sim` produces a valid `.app` bundle ready for `xcrun simctl install`.
* **FILES:**
  * `scripts/build-ios.sh` (new) — see IOS_IMPLEMENTATION_PLAN.md §Phase 7 step 1.
  * `scripts/ios/Info.plist` (new) — see IOS_IMPLEMENTATION_PLAN.md §Phase 1 minimal Info.plist.
  * `scripts/ios/entitlements.xcent` (new) — sandbox entitlements stub.
* **GATE:** `bash scripts/build-ios.sh aarch64-apple-ios-sim` (with `AZ_IOS_DRYRUN=1` for now) produces `MyAzulApp.app/MyAzulApp + Info.plist` without contacting `simctl`.

### Sprint F — Android-1: skeleton + compile (~1 hr)

* **GOAL:** `cargo build --target aarch64-linux-android -p azul-dll` compiles and links a `.so`.
* **FILES:**
  * `dll/src/desktop/shell2/android/mod.rs` (new) — skeleton from ANDROID_IMPLEMENTATION_PLAN.md §Phase 1 step "Minimal skeleton" (~150 lines).
  * `dll/src/desktop/shell2/mod.rs` — `#[cfg(target_os="android")] pub mod android;` + cfg_if branch.
  * `dll/src/desktop/shell2/run.rs` — `#[cfg(target_os="android")]` run() impl with `ANDROID_INITIAL_OPTIONS` static.
  * `dll/Cargo.toml` — `[target.'cfg(target_os="android")'.dependencies] android-activity = { version = "0.6", features = ["native-activity"] }, ndk = "0.9", jni = "0.21"`.
  * `dll/build.rs` — Android branch: link `-landroid -llog`, warn if `ANDROID_NDK_HOME` missing.
* **GATE:** `cargo ndk -t arm64-v8a build -p azul-dll` produces `target/aarch64-linux-android/debug/libazul.so` (or equivalent) — non-empty file.

### Sprint G — Android-2: CPU rendering via ANativeWindow_lock (~2 hr)

* **GOAL:** `render_frame()` copies AzulPixmap pixels into the lock-buffer of an `ANativeWindow`.
* **FILES:**
  * `android/mod.rs::render_frame()` — wire `cpurender::render` → `pixmap.data()` → `native_window.lock()` → row-by-row memcpy → `drop(buffer)`.
* **GATE:** Compiles. Manual: on emulator, a window appears and shows the rendered DOM.

### Sprint H — Android-3: event loop + touch + key (~1.5 hr)

* **GOAL:** `android_main` polls events; touch → mouse mapping; basic `KeyEvent::unicode_char()` → text input.
* **FILES:**
  * `android/mod.rs::android_main()` — full event loop from ANDROID_IMPLEMENTATION_PLAN.md §Phase 3.
  * `android/mod.rs::handle_motion_event()` / `handle_key_event()` — §Phase 4.
* **GATE:** Compiles. Manual: tap on emulator → click handler fires.

### Sprint I — Android-4: APK build pipeline (~1 hr)

* **GOAL:** `scripts/build-android.sh aarch64-linux-android` produces a signed `aligned.apk` ready for `adb install -r`.
* **FILES:**
  * `scripts/build-android.sh` (new) — full pipeline from ANDROID_IMPLEMENTATION_PLAN.md §Phase 7.
  * `scripts/android/AndroidManifest.xml` (new) — minimal, `android:hasCode="false"`, `NativeActivity` entry.
  * `scripts/android/NativeInputConnection.java` (new, but optional in MVP) — soft-keyboard bridge.
* **GATE:** `bash scripts/build-android.sh aarch64-linux-android` (with `AZ_ANDROID_NO_DEPLOY=1`) emits `aligned.apk` signed with the debug keystore.

### Sprint J — Headless snapshot harness (~1 hr)

* **GOAL:** Cross-target proof. A binary built for `aarch64-apple-ios-sim` or `aarch64-linux-android`, run on macOS host with `AZ_BACKEND=headless`, produces a deterministic PNG.
* **FILES:**
  * `scripts/mobile-headless-snapshot.sh` (new) — builds for mobile target, runs binary under macOS rosetta / emulator, asserts PNG matches.
  * `scripts/mobile/golden/*.png` — committed snapshots.
* **GATE:** Snapshot script exits 0 on a freshly built mobile binary.

### Sprint K — Cross-compile docs & CI (~30 min)

* **GOAL:** A single document explains how to take this branch and ship to a phone.
* **FILES:**
  * `scripts/CROSS_COMPILE_MOBILE.md` (new) — distilled from this plan, with one-paragraph quickstarts.
  * `.github/workflows/mobile.yml` — CI matrix for {ios-sim, android-arm64} + cargo check + golden snapshot.
* **GATE:** Doc renders cleanly; CI YAML is valid (`actionlint`).

### Sprint L — Stretch goals (no GATE required)

* iOS Phase 5: orientation + safe-area-insets + DPI.
* Android Phase 6: ConfigChanged / DPI / lifecycle.
* GPU on iOS via EAGLContext + CAEAGLLayer (optional).
* Vulkan / OpenGL ES on Android (optional, only if profiling shows blit cost matters).

---

## 4. Architectural decisions

1. **CPU-only render path on day 1.** The headless `CpuBackend` is reused verbatim; there is no EGL/GL/Metal context creation. Performance is good enough for menu bars / UIs; profiling can drive a GPU pass later.

2. **No Xcode project, no Gradle, no Android Studio.** Build pipeline = `cargo build` + `aapt2` + `zipalign` + `apksigner` (Android) or `cargo build` + `Info.plist` template + optional `codesign` (iOS). The whole pipeline runs from `scripts/build-{ios,android}.sh`.

3. **No Java code on Android until Phase 5.** `NativeActivity` (built into Android) covers everything except soft-keyboard IME. Hardware/`unicode_char` keys deliver ASCII for free.

4. **`android-activity` crate** for the entry point + event loop. It is maintained, used by winit/Bevy/wgpu, and abstracts the JNI dance for `NativeActivity`.

5. **`ANativeWindow_lock` for blit** — simplest, no surface/context management. If profiling later shows the memcpy is hot, upgrade to an EGL texture blit.

6. **Headless backend as both regression harness and "screen-less window".** The same `cpurender::render()` call drives both the on-device pixmap and the CI golden-PNG snapshot. One implementation, two consumers.

7. **`config.toml` linker entries committed** so contributors don't need to manually configure cargo for cross-compile.

8. **All Apple-tooling-dependent steps are runtime-gated.** `cargo check` works the moment the iOS SDK is installed; nothing in this plan needs full Xcode in the source tree.

---

## 5. Reference index

| Topic                    | File                                                  |
|--------------------------|-------------------------------------------------------|
| iOS step-by-step phases  | `scripts/IOS_IMPLEMENTATION_PLAN.md`                  |
| Android step-by-step     | `scripts/ANDROID_IMPLEMENTATION_PLAN.md`              |
| Cross-platform event     | `dll/src/desktop/shell2/common/event.rs`              |
| Headless CPU rendering   | `dll/src/desktop/shell2/headless/mod.rs:130`          |
| macOS golden standard    | `dll/src/desktop/shell2/macos/mod.rs`                 |
| Current iOS skeleton     | `dll/src/desktop/shell2/ios/mod.rs` (326 lines)       |
| Run-entry dispatch       | `dll/src/desktop/shell2/run.rs:622-652` (iOS branch)  |
| RawWindowHandle structs  | `core/src/window.rs` (AndroidHandle, IOSHandle)       |

---

## 6. Autonomous cron / loop policy

* Recurring schedule: every ~13 min during work hours, fires `/loop` re-entry to keep advancing sprints.
* Each fire: read `TaskList`, pick the next `pending` task in ID order, mark `in_progress`, do the work, mark `completed`, then exit.
* Skip + sleep 60 s if `cargo build` reports a file-lock conflict.
* Auto-expire after 7 days (CronCreate default).
* Plan & sprint completion summaries are appended to `scripts/MOBILE_SESSION_LOG.md` so progress is auditable across cron fires.
