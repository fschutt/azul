---
slug: deploying/mobile
title: Mobile (iOS & Android)
language: en
canonical_slug: deploying/mobile
audience: external
maturity: beta
guide_order: 231
topic_only: false
short_desc: Build, package, sign and install an Azul app as an .apk / .ipa — all Rust, from any OS, no Xcode or Android Studio
prerequisites: [hello-world]
tracked_files:
  - dll/build.rs
  - dll/Cargo.toml
  - dll/src/desktop/shell2/android/mod.rs
  - dll/src/desktop/shell2/ios/gl.rs
  - dll/src/desktop/shell2/ios/gpu.rs
  - dll/src/desktop/shell2/common/gpu_select.rs
  - dll/src/desktop/shell2/common/gpu_lifecycle.rs
  - dll/src/desktop/shell2/run.rs
  - scripts/build-android.sh
  - scripts/build-ios.sh
  - scripts/mobile-check-all.sh
last_generated_rev: ae991d834f665b88609d19ecf25dc88ea50e98a0
generated_at: 2026-05-30T00:00:00Z
default-search-keys:
  - App
  - WindowCreateOptions
  - link-static
  - aarch64-linux-android
  - aarch64-apple-ios
---

# Mobile (iOS & Android)

Your Azul app **is** the mobile app. The same `App::create(...).run(...)` you
wrote for desktop ships as an `.apk` or `.ipa` — there is no Java/Kotlin or
Swift/Objective-C app layer to write, and you need **neither Xcode nor Android
Studio**. An `.apk` and an `.ipa`/`.app` are just ZIP archives with a known
layout, so you cross-compile the native library with the Rust compiler and pack
it with a few command-line tools. You can build for *both* platforms from Linux;
only the final iOS code-signing touches an Apple-specific tool, and even that has
a cross-platform option (see [iOS signing](#ios-code-signing-no-xcode)).

The two ready-made scripts —
[`scripts/build-android.sh`](https://github.com/fschutt/azul/blob/master/scripts/build-android.sh)
and [`scripts/build-ios.sh`](https://github.com/fschutt/azul/blob/master/scripts/build-ios.sh)
— do the whole thing (cross-compile → bundle → sign → optionally deploy) with no
IDE; CI runs exactly these to produce the release artifacts. The sections below
explain what they do so you can reproduce or adapt them. Every example in the
repo (AzulMaps, azul-paint, azul-meet, …) is packaged this way.

## Supported targets

| Target | Use |
|---|---|
| `aarch64-apple-ios` | iOS device (arm64) |
| `aarch64-apple-ios-sim` | iOS simulator on Apple silicon |
| `x86_64-apple-ios` | iOS simulator on Intel |
| `aarch64-linux-android` | Android device (arm64-v8a) |
| `x86_64-linux-android` | Android emulator |

`rustup target add <triple>` installs each. `bash scripts/mobile-check-all.sh`
runs `cargo check` across all five **plus** the two desktop cross targets
(`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-gnu`) — seven in total, and it
needs no SDK for any of them (see [Local testing](#local-testing)).

## How an Azul app maps onto each platform

Everything — layout, rendering (CPU or [GPU](#rendering-cpu-or-gpu-opengl-es)), callbacks, the
[realtime-media](../system/realtime-media.md) / sensor / gamepad / geolocation device APIs
— is identical to desktop. The one structural difference is the **entry point**:

- **iOS** keeps a normal `fn main()`. Your `main` calls `App::run(...)`, which on
  iOS hands control to `UIApplicationMain` and drives the UIKit run loop. So an
  iOS app is just your example compiled as a **binary** for an iOS target — the
  exact same source as desktop.
- **Android** has *no* `main()`: the OS loads your `.so` and calls
  `ANativeActivity_onCreate` (provided by the bundled
  [`android-activity`](https://crates.io/crates/android-activity) glue), which
  invokes `android_main` inside `libazul`. So an Android app is your example
  compiled as a **`cdylib`** with a tiny load-time shim. See
  [Android entry point](#android-entry-point).

## Two ways to ship libazul

Just like desktop, you don't rebuild the framework — you decide how your app
links against it:

- **Dynamic (drop-in prebuilt).** Ship the prebuilt `libazul` in the bundle and
  link your small app against it. On Android, put `libazul.so` in the APK under
  `lib/<abi>/` (e.g. `lib/arm64-v8a/`); on iOS, embed `libazul.dylib` in
  `MyApp.app/Frameworks/` and set the app binary's rpath to
  `@executable_path/Frameworks`. CI publishes a `libazul` per mobile target on
  the [release page](https://azul.rs/ui/release/$VERSION) for download. Your app can be C, Rust, or any
  binding — a C `hello-world.c` links `libazul` and calls `AzApp_create` /
  `AzApp_run` exactly as on desktop. On iOS the app binary **and** the embedded
  `libazul.dylib` must both be code-signed.
- **Static (single artifact).** Build with the `link-static` feature so your app
  and azul compile into one `.so` (Android) or binary (iOS). This is what the
  build scripts and every repo example use, and what the rest of this page shows.

A mobile `libazul` is per-ABI/arch (`aarch64` for devices, `x86_64` for the
emulator/simulator) — you bundle the slice(s) you target, not a single file.

**How the demo apps are built.** The demos on the
[release page](https://azul.rs/ui/release/$VERSION) do not compile the engine each:
CI builds ONE `libazul` per phone target (`libazul.so` for Android, `libazul.a` for
iOS), and every app is `cargo build -p <App>` against it with `azul-dll`'s
`link-dynamic` feature (the Rust facade, the C ABI as `extern` declarations; no
`azul-core`, `azul-layout` or `azul-css` compiles in an app build, and CI fails an app
that does). On Android the APK carries `libazul.so` next to the app's own library
(`lib/<abi>/`); NativeActivity finds `ANativeActivity_onCreate` and the JNI entry
points through the app library's dependency. On iOS the app links `libazul.a`, and the
system frameworks it needs are the ones `rustc --print native-static-libs` printed
when the archive was built (kept in `libazul.a.native-libs` next to it, which
`AZ_LINK_PATH` consumers read). Your own app can do the same: depend on `azul-dll` with
`features = ["link-dynamic"]` and point `AZ_LINK_PATH` at the library file.

## Minimal toolchain (only the stubs you need)

You do **not** need the full NDK or the iOS SDK. Both are mostly *link stubs*
(empty `.so` API stubs / `.tbd` text stubs) plus headers, and Rust already ships
its own linker (`rust-lld`). Azul renders on the **CPU** on mobile unless you ask for
the GPU, and the GPU path links no OpenGL ES library either: the app opens
`OpenGLES.framework` (iOS) or `libEGL` / `libGLESv2` (Android) by name the first time a window
asks for it (see [Rendering: CPU or GPU](#rendering-cpu-or-gpu-opengl-es)). The entire
system-library surface is:

| Platform | Links against | Notes |
|---|---|---|
| **Android** | `libandroid` (NativeActivity, `ANativeWindow`, `ALooper`), `liblog` (`__android_log_print`), and `libc` / `libm` / `libdl` | Tiny NDK *stub* `.so`s — extract just those five, no full NDK needed at link time. (Set via `cargo:rustc-link-lib=android,log` in `dll/build.rs`.) |
| **iOS** | `Foundation`, `UIKit`, `CoreGraphics`, `libSystem` — plus one framework per device API you use (`AVFoundation`, `CoreMotion`, `CoreLocation`, `GameController`) | `.tbd` text stubs in the iOS SDK; copy only the ones you reference. |

So a from-scratch minimal setup is:

1. `rustup target add <triple>` — brings the Rust `std` for the target.
2. A small **stub sysroot**: the five Android stub `.so`s (from the NDK's
   `platforms/android-<api>/.../usr/lib/`) **or** the iOS framework `.tbd` stubs
   (from the SDK's `System/Library/Frameworks/`). A few hundred KB, not the
   multi-GB toolchain.
3. **`rust-lld`** as the linker — no external `ld`, `clang`, or `xcrun`.

For **packaging** you then need only small CLI tools — `aapt2` / `zipalign` /
`apksigner` for Android (head-less `sdkmanager` install, no Studio), nothing for
iOS beyond `zip` and a signer. A pure-`NativeActivity` Android app needs **no
Java** (so no JDK / `d8`) — the custom `AzulActivity.java` is only for the
optional gesture bridge.

> The ready-made `build-android.sh` / `build-ios.sh` currently lean on a normal
> NDK / iOS-SDK install for convenience; the table above is the irreducible set
> if you want to assemble a minimal cross-toolchain (e.g. to build iOS apps on
> Linux). Extracting a minimal stub sysroot is a one-time step.

## Rendering: CPU or GPU (OpenGL ES)

A phone window draws with one of two renderers, chosen when the window is made:

| `AZ_BACKEND` | What runs |
|---|---|
| `cpu` (the default, also when nothing is set) | The CPU renderer. No GL context is made; the frame is a pixmap handed to the system. |
| `gpu` | WebRender on an **OpenGL ES 3.0** context, whatever the renderer is — a software one (the iOS Simulator's, the Android emulator's SwiftShader) included, which is how the GPU path is tested without a GPU. |
| `auto` | As `gpu`, but only on a hardware renderer: a software OpenGL ES is slower than the CPU renderer, which `auto` then keeps. |

An app can ask for the same from code (`WindowCreateOptions` →
`window_state.renderer_options.hw_accel`: `Enabled` is `gpu`, `Disabled` is `cpu`); the
environment variable wins over it. A phone app started from the home screen has no environment
to set: on iOS the `Info.plist` key `AZBackend` (`gpu`, `auto` or `cpu`) says the same when
`AZ_BACKEND` is not set — `/usr/libexec/PlistBuddy -c 'Add :AZBackend string gpu' MyApp.app/Info.plist`,
then sign and install as usual. One libazul
holds both renderers and chooses at run time: there is no second build and no cargo feature
for the GPU path.

**It never costs you the window.** If the GPU path cannot come up — no OpenGL ES 3.0, a context
or a surface the system refuses, a shader the driver does not compile, a context lost twice
without a frame in between — the window is drawn by the CPU renderer instead, and the log says
so in one line that names the stage and the cause:

```text
[render] gpu unavailable at surface: -[EAGLContext renderbufferStorage:fromDrawable:] returned NO; drawing on the CPU
```

Every window writes, once, the line that says which renderer draws it (`AZ_LOG=info` shows it on
the iOS console and in logcat; the phone QA of the repository reads it):

```text
[render] backend=gpu requested=gpu device=Apple Software Renderer
[render] backend=cpu requested=auto device=software
```

The stages that can fail are `library`, `context`, `make_current`, `functions`, `probe`,
`surface`, `renderer`, `document`, `shaders`, `frame` and `lost`. A **test** build (the
`test-hooks` cargo feature; never a shipped app) can make one fail on purpose to see the fall back:
`AZ_GPU_FAIL=surface` (every time), `AZ_GPU_FAIL=context:1` (once), `AZ_GPU_FAIL=lost:2` (the
context is lost twice), a comma separated list for several.

### iOS: OpenGL ES, and why not Metal

The engine's only GPU renderer is WebRender, and the WebRender in this repository drives
OpenGL (ES) and nothing else, so on iOS the GPU path is OpenGL ES 3.0 through `EAGLContext` and
a `CAEAGLLayer` under the app's view. OpenGL ES has been **deprecated since iOS 12** (every
symbol of `EAGL.h` is marked `OPENGLES_DEPRECATED(ios(2.0, 12.0))`), Apple says apps built with it
continue to run, and the framework is still part of iOS 26's Simulator runtime. A Metal path
needs either ANGLE (GLES on Metal) built for iOS or a Metal back end for WebRender; neither
exists in the tree, so it is a tracked item and not a plan. `OpenGLES.framework` is opened by
its path the first time a window asks for the GPU: nothing links it, an app that draws on the CPU never
loads it, and the build scripts need no extra flag.

What the iOS path does with the app's life, from Apple's rules for OpenGL ES: when the app is
about to go inactive (`applicationWillResignActive:`) the display link is paused and every
submitted command is finished (`glFinish`); in `applicationDidEnterBackground:` the last GL the
app may issue runs, and from then on **no GL command is issued at all** (the system ends an app that
does), whatever the app does; when the app is active again the frames resume with one full frame.
A memory warning releases WebRender's caches (at once when the app is active, when it is back
when it is not). A rotation or a new scale reallocates the layer's renderbuffer once.

Requirements: OpenGL ES 3.0 (every iPhone since the 5s, which is every device iOS 16 runs on).
Not verified without a phone: the speed of a real GPU, thermal behaviour, and the system's own
termination of a background app that issues GL (the Simulator does not enforce it).

To see it on your own phone, with the app installed (signed with your own profile) and the phone
connected and trusted, one command on the Mac:

```sh
python3 scripts/ci/ios_phone_gpu.py "<the phone's name or id from `xcrun devicectl list devices`>" --bundle <your bundle id>
```

It starts the app three times (`cpu`, `gpu`, `auto`), keeps the console of each for 25 seconds (scroll
the page with your finger meanwhile), and prints which renderer ran on which `GL_RENDERER`, why not
the GPU if it fell back (`gpu unavailable at <stage>`), the first GPU frame and the frame times of what
you scrolled. The two lines that say which renderer drew (and why not the GPU) are also in the unified
log (Console.app) of an app started from the home screen with the `AZBackend` key set.

## Building the native library

Mobile builds use `link-static` with **no default features** (the desktop
windowing/renderer defaults pull in things mobile doesn't want):

```sh
# iOS device — produces the binary (its main() runs UIApplicationMain via App::run)
cargo build --release --target aarch64-apple-ios -p my-app \
    --no-default-features --features "std,logging,link-static,a11y"

# Android arm64 — produces the cdylib the APK ships
cargo build --release --target aarch64-linux-android -p my-app \
    --no-default-features --features "std,logging,link-static,a11y,android-activity"
```

From Rust, depend on `azul-dll` directly with those features; from C, use the
generated `azul.h` (`cargo run -r -p azul-doc -- codegen c`) and the same
`AzApp_create` / `AzApp_run` entry points every binding uses.

## Android

### Android entry point

Because there is no `main()`, run your setup from a **load-time constructor**.
`libazul` already provides `android_main` (via the android-activity glue);
`App::run` on Android just stashes the window options for it to read — and it
must run *before* `ANativeActivity_onCreate`, which is exactly what a
[`ctor`](https://crates.io/crates/ctor) gives you. Factor the setup into one
function and wire both entry points:

```rust
use azul::prelude::*;

pub fn start() {
    let data = RefAny::new(DataModel { counter: 0 });
    let app = App::create(data, AppConfig::create());
    // Android: run() stashes the window options + returns; desktop/iOS: blocks.
    app.run(WindowCreateOptions::create(my_layout));
}

// Desktop / iOS — main() runs and App::run drives UIApplicationMain on iOS.
#[cfg(not(target_os = "android"))]
fn main() { start(); }

// Android — fires at dlopen, before libazul's android_main reads the options.
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() { start(); }
```

(`azul-maps` / `azul-paint` in the repo have the same entry point; they link the
prebuilt `libazul` instead of compiling it, see "How the demo apps are built" above.)
Your crate must build as a `cdylib` for Android, and pulls the android-activity glue +
`ctor` only on Android:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_os = "android")'.dependencies]
azul = { package = "azul-dll", version = "0.2", default-features = false, features = ["link-static", "android-activity"] }
ctor = "0.2"
```

That is the *entire* difference from a desktop app.

### One process per activity, and the app's folders

- `libazul` takes your window options once per process (the constructor above runs at `dlopen`), so it cannot be
  started a second time in the same process. `AzulActivity` therefore **ends the process when the activity is
  destroyed**: BACK, then the launcher icon, is a cold start. The manifest lists every configuration change
  (rotation, screen size, font size, bold text, locale, layout direction, dark mode, ...), so none of them recreates
  the activity; the app sees them in its window state (size, dpi, the system style) at its next layout.
- An Android app process has no `HOME` and no `XDG_*` variables. `AzulActivity` sets `HOME` and `XDG_DATA_HOME` to
  `getFilesDir()`, `XDG_CONFIG_HOME` to `files/config`, and `XDG_CACHE_HOME` and `TMPDIR` to `getCacheDir()` before the
  library loads (what the caller set is kept), so `FilePath::get_data_dir()`, `get_config_dir()` and
  `get_cache_dir()` answer, and `std::env::temp_dir()` is writable. `libazul` logs one line
  `[Android] app dirs: home=... (writable) data=... cache=... temp=...` at `AZ_LOG=info` (an error line when one is missing or not writable).
- `WindowCreateOptions::create_callback` runs once per window, when the window's surface first exists (Android's first
  `InitWindow`: the real size and density are in the window state) and before its first layout, as on the desktop
  shells. Start the timers and workers that live as long as the window there.
- A finger is a *tap candidate* until it has moved more than the touch slop (8 dp, the engine's logical pixel) from where
  it landed: inside the slop nothing scrolls, a release is a click at the landing point and a long press stays a long
  press; the move that crosses it makes the contact a pan for good (the page does not jump by the slop) and its release
  clicks nothing. A mouse, a stylus and an eraser have no slop. A widget a finger drags (a slider, a map, a paint canvas)
  therefore starts to move 8 dp after the touch under a finger and at once under a stylus.

### Package the APK (no Android Studio)

You need **NDK 27**, **build-tools 34** (`aapt2`, `zipalign`, `apksigner`) and a
**JDK 17** — all installable head-less via `sdkmanager`, no IDE. An `.apk` is a
ZIP, assembled like this (what `build-android.sh` does):

```sh
# 1. cross-compile the cdylib (cargo-ndk sets the NDK linker for you).
#    NB: the API level is --platform (cargo-ndk forwards a bare -p to cargo as
#    --package), and it goes BEFORE the `build` subcommand.
cargo ndk -t arm64-v8a --platform 24 -o ./jniLibs build --release \
    -p my-app --no-default-features --features "std,logging,link-static,a11y,android-activity"

# 2. lay out the APK tree, compile the manifest, add the lib + Java glue
aapt2 link --manifest AndroidManifest.xml -I "$ANDROID_HOME/platforms/android-34/android.jar" -o base.apk
zip -r base.apk lib/arm64-v8a/libmy_app.so classes.dex   # classes.dex = dexed scripts/android/*.java

# 3. align + sign with a (debug) keystore — apksigner is a CLI tool
zipalign -f 4 base.apk aligned.apk
apksigner sign --ks debug.keystore --ks-pass pass:android aligned.apk
```

**Files to copy** into your project (Java glue + manifest template) live in
[`scripts/android/`](https://github.com/fschutt/azul/tree/master/scripts/android):
`AndroidManifest.xml` (sets `android.app.lib_name` to your lib), `AzulActivity.java`
(the `NativeActivity` subclass), `NativeGestureBridge.java`, `AzulFilePicker.java`.
The manifest's `lib_name` must match your `cdylib` name. Simplest path:
`bash scripts/build-android.sh aarch64-linux-android <AppName> <com.pkg>`.

Declare permissions in `AndroidManifest.xml` (`CAMERA`, `RECORD_AUDIO`,
`ACCESS_FINE_LOCATION`, `INTERNET`, …) and request the dangerous ones at runtime.

**Minimum Android 7.0 (API 24)** — the camera backend links the NDK Camera2 stubs
(API 24); AAudio (API 26) is loaded at runtime, so on 7.0/7.1 the app runs and
audio just reports unavailable.

**16 KiB memory pages.** Android 15 and later run on devices whose kernel uses 16 KiB pages,
and Google requires apps that target Android 15 (targetSdk 35) and later to support them from
2027-02-01. A native library whose segments are aligned to 4 KiB (the default of NDK 27's
linker; NDK 28 aligns to 16 KiB) cannot be loaded there. Link every library of your app, yours
and `libazul.so`, with 16 KiB alignment by naming the flags once in the `.cargo/config.toml` of
your project (this repository does it for the demo apps):

```toml
[target.aarch64-linux-android]
rustflags = ["-Clink-arg=-Wl,-z,max-page-size=16384", "-Clink-arg=-Wl,-z,common-page-size=16384"]
[target.x86_64-linux-android]
rustflags = ["-Clink-arg=-Wl,-z,max-page-size=16384", "-Clink-arg=-Wl,-z,common-page-size=16384"]
```

`python3 scripts/android_elf_check.py app.apk` reads the program headers of every library of an
APK and names each one whose `PT_LOAD` alignment is below 16 KiB (`llvm-readelf -lW lib.so` shows
the same column, `Align`). A `RUSTFLAGS` environment variable replaces the flags of the config
file, so name them there too when you set one.

## iOS

### Build + bundle the .app/.ipa (no Xcode project)

A `.app` is a directory; an `.ipa` is `Payload/<App>.app` zipped. You do **not**
need an Xcode project — just the iOS SDK (for the linker sysroot) and the
cross-linker. `build-ios.sh` does:

```sh
# 1. build your example as an iOS binary (main() runs UIApplicationMain via App::run)
cargo build --release --target aarch64-apple-ios -p my-app \
    --no-default-features --features "std,logging,link-static,a11y"

# 2. assemble the bundle: the executable + an Info.plist
mkdir -p MyApp.app
cp target/aarch64-apple-ios/release/my-app MyApp.app/MyApp
cp scripts/ios/Info.plist MyApp.app/Info.plist          # template to copy/edit

# 3. (device) sign, then zip into an .ipa
mkdir -p Payload && cp -r MyApp.app Payload/ && zip -r MyApp.ipa Payload
```

The `Info.plist` template + entitlements are standard iOS files.
Add the usage strings for the device APIs you use: `NSCameraUsageDescription`,
`NSMicrophoneUsageDescription`, `NSLocationWhenInUseUsageDescription`,
`NSMotionUsageDescription`.

> **Cross-compiling iOS from Linux:** install the iOS SDK sysroot (extractable
> from the Xcode toolchain, no GUI) and point Rust's linker at it. The simulator
> needs no signing and runs unsigned `.app`s directly.

### iOS code signing (no Xcode)

Code signing is the *only* Apple-specific step, and it does **not** require Xcode
or even a Mac:

- [`rcodesign`](https://github.com/indygreg/apple-platform-rs) (the Rust
  `apple-codesign` crate) signs `.app`/`.ipa` bundles **on Linux or Windows** and
  can submit to **Apple's notarization web service** (`rcodesign notary-submit`),
  a REST API — no `xcrun`/`notarytool` needed.
- You still need an Apple Developer ID certificate + provisioning profile (the
  paid program), but those are files, not tools.
- Simulator builds and personal-team device installs over a debug bridge need no
  signing at all.

## Local network on a phone

An app that uses an `IrohEndpoint` finds the computer it controls by its endpoint id on the local
network ([Networking](../data/networking.md#finding-a-peer-on-the-local-network)); a phone needs
two things the engine cannot grant itself.

**Android.** `AndroidManifest.xml` declares `android.permission.CHANGE_WIFI_MULTICAST_STATE` (the
template in `scripts/android/` already does; it is a normal permission, granted at install, no
prompt). The engine takes a `WifiManager.MulticastLock` for as long as an endpoint with local
discovery is bound: the Wi-Fi chip drops multicast without it. Without the permission the endpoint
works and `local_discovery_status()` says `unavailable`.

**iOS.** The Info.plist needs `NSLocalNetworkUsageDescription` (the text of the Local Network
prompt) and `NSBonjourServices` with `_irohv1._udp`. `build-ios.sh` writes both for
`AZ_IOS_LOCAL_NETWORK="Find the computer you control on your network"`; `AZ_IOS_USAGE` adds other
usage descriptions (one `NSKey=text` per line) and `AZ_IOS_BONJOUR_SERVICES` other service types.
An iPhone does not open multicast sockets (the entitlement for that is restricted to teams Apple
approves, and a personal team cannot hold it): it asks the system's Bonjour, which needs the
prompt's permission and the listed service type and nothing else. The first lookup shows the prompt;
the Simulator does not implement it, test on a device. If the user denies it, or the keys are
missing, the lookup answers `kDNSServiceErr_PolicyDenied` and the dial's error says so in words.

```sh
AZ_IOS_LOCAL_NETWORK="Find the computer you control on your network" \
  bash scripts/build-ios.sh aarch64-apple-ios my-app
```

## Updates on a phone

The App Store and Google Play deliver the updates of an app that is installed from them, and both
forbid an app to download or install code of its own (App Store Review Guideline 2.5.2, Google
Play's Device and Network Abuse policy). A libazul built for iOS or Android therefore has no code
that fetches, stages or installs a program: the updater is still there (its types and functions
are part of the C API on every target), `InstallKind::detect()` answers `AppStore`, the mode is
clamped to notify-only, a check can still read your manifest and tell the user that a newer
version exists, and `stage_program_update`, the automatic staging and the update dialog answer
that the store delivers the updates. The HTTP client itself (map tiles, `http_get`, your own
requests) is part of every build except wasm32. A desktop app that is sold through the Mac App
Store or the Microsoft Store gets the same build with azul-layout's `store_app` feature.

A store build downloads data only. The one thing azul fetches on its own is the model of the
built-in speech recognizer (a file of 16.9 MB that an engine already in the app reads): it comes
into the app's cache folder the first time the recognizer needs it, only after the app has asked
its user (the default is `SpeechModelDownload::Ask`; `SpeechSession.set_model_download` sets the
policy to `Never` or `Automatic`, and `model_state` and `allow_model_download` are the state, the
progress and the user's yes), over https, with the size and the SHA-256 of the file checked against
the pin before it is used. Nothing that was downloaded is executed or loaded as code, and an app
that is offline (`set_offline`) fetches nothing.

## Installing & debugging the built app

You don't have to build anything to try the demos — every example is published
per-OS on the [release page](https://azul.rs/ui/release/$VERSION) (Demos section): a `.apk` for Android, a
device `.app` and a Simulator `.app` for iOS. To install a build (yours or a
downloaded one):

### Android (`.apk`)

The APKs are debug-signed, so they sideload directly.

```sh
# Over USB (enable Settings → Developer options → USB debugging first):
adb install AzMaps-android-arm64.apk
# Replace an existing install: adb install -r …; uninstall: adb uninstall com.azul.AzMaps
# (every APK of every release is signed with the same debug key, scripts/android/debug.keystore,
#  so an update installs over an older version of the same app)
# On an emulator use the x86_64 file: adb install AzMaps-android-x86_64.apk
```

Or copy the `.apk` to the phone and tap it (allow "install unknown apps" for the
browser/file manager).

**Debug logs:** azul's platform layer logs through the `log` facade to logcat
(via `liblog`):

```sh
adb logcat -s azul:V '*:S'        # azul lines only
adb logcat | grep -E '\[camera\]|\[gamepad\]|\[sensors\]|\[cap\]'
```

A native crash prints a tombstone — `adb logcat` shows the `backtrace:` with the
faulting library; pull `/data/tombstones/` for the full dump.

### iOS Simulator (`.app`, no signing)

The Simulator slice (`<App>-ios-sim.app.zip`) runs **unsigned** (ad-hoc signed by the
build) — easiest to try on a Mac:

```sh
unzip AzMaps-ios-sim.app.zip                   # -> AzMaps.app
open -a Simulator                              # boot a simulator
xcrun simctl install booted AzMaps.app
xcrun simctl launch --console booted com.azul.AzMaps   # --console streams stdout/stderr
```

### iOS device (`.app` → signed)

A physical iPhone needs the binary code-signed (a **free** Apple ID / personal
team works for a 7-day sideload). The release's device slice (`<App>-ios-device.app.zip`)
is **unsigned on purpose**: CI holds no signing identity, and the identity has to be
yours (AzReview signs and installs the AzReview Companion for the phone it is attached
to, with your own Apple ID). On a Mac:

```sh
unzip AzMaps-ios-device.app.zip
codesign --force --sign "Apple Development: you@example.com (TEAMID)" \
  --entitlements entitlements.plist AzMaps.app
xcrun devicectl device install app --device <udid> AzMaps.app
```

No Mac? `rcodesign` signs an `.app`/`.ipa` from Linux/Windows with a Developer ID
`.p12`. **Device logs:** `xcrun devicectl device console --device <udid>`, or
Console.app filtered by the app name, or `idevicesyslog` (libimobiledevice).

## From Rust to a final .apk / .ipa — cross-platform

The whole pipeline is `cargo` + small CLI tools, no IDE, and the same on Linux or
macOS (only the final iOS *signing* prefers a Mac, and even that has the
`rcodesign` escape hatch):

| Step | Android | iOS |
|------|---------|-----|
| 1. Compile | `cargo ndk -t arm64-v8a --platform 24 build` (the cdylib NativeActivity loads) | `cargo build --target aarch64-apple-ios[-sim]` (the binary; `main()` runs `UIApplicationMain`) |
| 2. Bundle | `aapt2 link` + `zip` the `.so` + `classes.dex` → `.apk` | lay out `MyApp.app/` (binary + `Info.plist`); `.ipa` = `Payload/MyApp.app` zipped |
| 3. Sign | `zipalign` + `apksigner` (debug keystore is fine for sideloading) | `codesign` / `rcodesign` (device only; Simulator needs none) |
| 4. Install | `adb install` | `xcrun simctl install` (sim) / `devicectl` (device) |

`build-android.sh` and `build-ios.sh` run steps 1–4 end to end.

**The app's id.** `azul-doc mobile build` / `mobile run` name the iOS bundle id and the Android
package after `--package`, else the crate's `[package.metadata.bundle] identifier`, else
`com.azul.<crate>`. On a phone that id wins over the app's `AppConfig::app_id`; set both to the
same string (a different `app_id` is logged as a warning). See
[App identity](../system/windowing.md#app-identity).

## Local testing

There are four rungs here, and each proves something the one below it cannot.
Start at the top — it is free and catches most of what actually breaks.

| Rung | Cost | Proves |
|---|---|---|
| 1. `cargo check` per target | seconds, **any OS**, no SDK | the platform code *compiles* |
| 2. Synthetic-event harness | seconds, no hardware | the event pipeline dispatches |
| 3. Emulator / simulator | one-time SDK install | the app *boots and draws* |
| 4. Real device | a cable | timing, sensors, the actual digitizer |

Everything below is also available as a command that probes before it acts:

```sh
cargo run -p azul-doc -- mobile doctor            # what is installed, what is missing
cargo run -p azul-doc -- mobile install android   # plan, confirm, install
cargo run -p azul-doc -- mobile check             # rung 1, every target
cargo run -p azul-doc -- mobile emulator          # rung 3, headless
cargo run -p azul-doc -- mobile run android examples/azul-writer
```

`mobile install` prints a plan and waits for a `y` before running anything:
each step reports whether it is already satisfied, exactly what would run, or
why a human has to do it (`--yes` to skip the prompt in CI, `--dry-run` to only
ever print). It is idempotent, so a re-run after a failed download resumes
instead of starting over.

### 1. Cross-compiling is cheaper than it looks

**`cargo check` does not link.** That is the whole trick: it needs the Rust
`std` for the target and nothing else — no NDK, no iOS SDK, no cross C
toolchain, no `JAVA_HOME`. One `rustup target add` per triple and every mobile
backend compiles on whatever machine you happen to have:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android
bash scripts/mobile-check-all.sh          # or, per target:
cargo check -p azul-dll --lib --target aarch64-linux-android
```

This matters more than it sounds. Platform shells are `#[cfg]`-gated, so
**nothing in `shell2/android/`, `shell2/ios/`, `shell2/windows/` or
`shell2/linux/` is compiled by a normal `cargo build` on your machine** — an
editor will happily show you green squiggles over code the compiler has never
seen. A `use super::foo` that resolves on one platform and not another, a
struct literal missing a field added last week, a helper that exists only in
the desktop build: all of it is invisible until you check the target.

Run it before you commit platform code, not after.

### 2. Without any device at all

The synthetic-event harness drives the full input → dispatch → callback path
with no hardware, including pen, touch, gestures, scroll phases, IME
composition and gamepad state — see
[e2e-testing](../debugging/e2e-testing.md). This is the right place to pin
*behaviour*; the rungs below are for proving the platform glue underneath it
is real.

### 3a. Android emulator — no Android Studio

Everything comes from Homebrew and the headless `sdkmanager` — no Android
Studio, no Gradle:

```sh
cargo run -p azul-doc -- mobile install android
cargo run -p azul-doc -- mobile emulator
```

That is the whole thing. What it does, if you would rather type it:

```sh
brew install --cask android-commandlinetools android-platform-tools
brew install openjdk@17                       # sdkmanager/avdmanager need a JDK

# NB: the Homebrew shim at opt/openjdk@17 has bin/java but no lib/, so it is
# not a valid JAVA_HOME — some tools limp along on it and others fail oddly.
# The real home is inside the bundle:
export JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools

# The emulator + one system image. Match the ABI to your host: arm64-v8a on
# Apple silicon, x86_64 on an Intel/AMD box — a mismatched image runs under
# full emulation and is unusably slow.
sdkmanager "emulator" "system-images;android-34;google_apis;arm64-v8a"
avdmanager create avd -n azul -k "system-images;android-34;google_apis;arm64-v8a"

# Run the emulator by its absolute path from inside its own directory: it
# locates its bundled libraries relative to argv[0], so a PATH symlink breaks
# it. -no-window still produces a framebuffer, so screencap and
# `uiautomator dump` both keep working headlessly.
cd "$ANDROID_HOME/emulator" && ./emulator -avd azul \
    -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect &

adb wait-for-device
# `device` in `adb devices` only means adbd answered — the framework can still
# be a minute away. This is the property the platform itself waits on:
until [ "$(adb shell getprop sys.boot_completed | tr -d '\r')" = "1" ]; do sleep 1; done

adb install -r target/android-bundle/AzWriter-arm64-v8a/aligned.apk
adb shell am start -n com.azul.azwriter/com.azul.app.AzulActivity
adb logcat -d | grep RustStdoutStderr
```

Note the activity: the manifest declares `com.azul.app.AzulActivity`, a
`NativeActivity` subclass that constructs the gesture and accessibility bridges
in `onCreate`. Launching `android.app.NativeActivity` instead fails with
*"Activity class does not exist"*.

To *build* the APK you additionally need the NDK
(`sdkmanager "ndk;27.0.12077973"`) — that is the one large download, and it is
required for linking, not for checking. `mobile install android --no-ndk` skips
it when you only want to run a prebuilt APK.

> **What an emulator cannot tell you.** Azul's Android input bridges
> (`NativeTextBridge` for soft-keyboard text/IME/insets, `AzulGamepad` for
> controllers) are JNI entry points, and JNI is a *callee*. If the Java side
> that calls them is not in your APK, the emulator boots, draws, and exercises
> none of those paths — you will see a working window and conclude the input
> layer works. Check the APK actually ships the Java glue before drawing that
> conclusion.

### 3b. iOS Simulator

The simulator needs **Xcode** — not an Xcode *project* (you never need one of
those, see [above](#build--bundle-the-appipa-no-xcode-project)), but the app
itself, because `simctl` and the iOS SDK ship inside it. Command Line Tools
alone are not enough: `xcode-select -p` pointing at
`/Library/Developer/CommandLineTools` gives you `MacOSX.sdk` and no
`iphonesimulator` sysroot, so iOS can be `cargo check`ed but **not linked**.

```sh
cargo run -p azul-doc -- mobile install ios   # checks the above, installs what it can
cargo run -p azul-doc -- mobile simulator     # boots one and prints its UDID
```

`mobile install ios` cannot install Xcode for you — it is an App Store download
tied to an Apple ID — so that step is reported as yours, with the exact commands
(including `xcodes install --latest` if you want a pinned version). Everything
around it, the Rust targets and the simulator runtime, it does install.

By hand:

```sh
xcode-select -p                                   # must NOT be CommandLineTools
xcrun --sdk iphonesimulator --show-sdk-path       # must resolve

open -a Simulator
xcrun simctl install booted azul-maps.app
xcrun simctl launch --console booted <bundle-id>
```

Budget the disk: since Xcode 14 the simulator **runtime is a separate
download** from Xcode itself, so it is roughly 10 GB + 7–10 GB, not one number.

### 3c. Headless simulator automation with baguette

[`baguette`](https://github.com/tddworks/baguette) drives booted simulators
from a CLI — no Simulator.app window, no Xcode UI — which makes the iOS
simulator scriptable in CI the way `adb` already makes Android scriptable.

```sh
brew install baguette        # needs Xcode 26 (⇒ macOS 15.6+), Apple silicon
```

It is not a `simctl` wrapper: it links `SimulatorKit` / `CoreSimulator`
directly, which is what buys real gesture dispatch rather than synthesized
taps. The commands that matter for testing an azul app:

```sh
UDID=$(baguette list --json | jq -r '.[0].udid')
baguette boot --udid $UDID

# Input injection — the real thing, not a synthetic event
baguette tap    --udid $UDID --x 120 --y 300 --width 393 --height 852
baguette swipe  --udid $UDID --startX 40 --startY 400 --endX 350 --endY 400 \
                --width 393 --height 852
baguette pinch  --udid $UDID --cx 196 --cy 400 --startSpread 40 --endSpread 220 \
                --width 393 --height 852
baguette type   --udid $UDID --text "hello"
baguette key    --udid $UDID --code ArrowLeft --modifiers shift

# Assertions
baguette describe-ui --udid $UDID --output tree.json    # accessibility tree
baguette screenshot  --udid $UDID --output shot.png
baguette clipboard get --udid $UDID
baguette logs --udid $UDID --style ndjson --bundle-id <id>
```

Three of those are worth calling out because they map onto things azul
otherwise has no way to check on a device:

* **`describe-ui`** dumps the accessibility tree as JSON. That is a *structural
  assertion target*: azul builds that tree itself (see
  [accessibility](../system/accessibility.md)), so a diff against a golden file
  catches a11y regressions that a screenshot cannot see.
* **`clipboard get` / `sync`** reads the simulator's `UIPasteboard`, which is
  exactly what azul's iOS clipboard transport writes to. A copy in the app and
  a `clipboard get` outside it is a true end-to-end check of the typed-payload
  path — flavors and all.
* **`tap` / `swipe` / `pinch` / `type`** enter through UIKit, so they exercise
  the real `touchesBegan:`, `UIPencilInteraction`, `UIKeyInput` and gesture
  recognizers — not azul's own synthetic-event injection. Rung 2 proves the
  dispatch logic; this proves the *platform glue feeding it*.

For a scripted loop, `baguette serve --port 8421` exposes the same surface over
HTTP + WebSocket (`POST /simulators/:udid/input`,
`GET /simulators/:udid/describe-ui.json`, `WS …/stream`, `WS …/logs`), which is
usually easier to drive from a test harness than shelling out per gesture.

### 3d. One command: build, boot, install, launch, assert

`mobile run` is rungs 3a/3b end to end. It resolves the crate, builds it,
boots a device if one is not already up, installs, launches, waits for the
first frame, screenshots, and reports whether the engine actually started:

```sh
cargo run -p azul-doc -- mobile run android examples/azul-writer
cargo run -p azul-doc -- mobile run ios AzWriter --windowed
```

It exits non-zero when nothing in the device log came from the Rust side, or
when the log contains a native crash — so it works as a CI gate, not just as a
convenience.

`--e2e <scenario.json>` additionally replays a scenario against the device,
through `adb shell input` on Android and through baguette on iOS:

```sh
cargo run -p azul-doc -- mobile run android examples/azul-writer \
    --e2e e2e/op-scroll-to-position.json
```

**This is deliberately a subset, and it tells you which subset.** The e2e op
vocabulary is engine-internal: `mount` installs a DOM, `assert_response`
inspects the debug dispatcher's last reply, `snapshot_frame` reaches into the
frame cache. None of that is expressible from outside the process. What a host
driver *can* replay is the input half — taps, swipes, keys, text, screenshots —
plus structural assertions against the platform accessibility tree.

So every op is classified, and the ones the driver cannot honour are counted
and named:

```
==> host replay: bug_scroll_offsets_hit_test
  11 of 19 ops executed on the device
  7 op(s) a host driver cannot express:
    - assert_node_count x1
    - assert_response x2
    - mount x1
    …
  verdict: INCOMPLETE
```

A replay that dropped ops is never reported green. That matters more than it
looks: a passing test from a harness that silently did nothing is a false
statement about the device, and it is exactly how a platform-glue bug survives
a "green" suite. Run the engine-internal half with `azul-doc e2e`, which drives
the same dispatcher in-process; run this half to prove the `touchesBegan:` /
`GestureDetector` / `InputConnection` path that the in-process harness bypasses.

### 4. Real devices

An emulator will not tell you about digitizer sampling rate, pen tilt, Force
Touch stages, real sensor noise or thermal throttling. For anything timing- or
hardware-shaped, the device is the only answer — see the install steps
[above](#installing--debugging-the-built-app).

## See also

- [Realtime Media and Devices](../system/realtime-media.md) — camera/mic/sensors/gamepad/
  geolocation on mobile, and the permissions you declare.
- [Deploying to the web](web.md) — the WASM target, by comparison.
- [hello-world](../hello-world.md) — the app you're shipping.
