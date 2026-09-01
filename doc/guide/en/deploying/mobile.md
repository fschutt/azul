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

Everything — layout, rendering (CPU), callbacks, the
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

## Minimal toolchain (only the stubs you need)

You do **not** need the full NDK or the iOS SDK. Both are mostly *link stubs*
(empty `.so` API stubs / `.tbd` text stubs) plus headers, and Rust already ships
its own linker (`rust-lld`). Azul renders on the **CPU** on mobile
(`gl_context_ptr = None`), so there are no OpenGL ES / Metal libraries to link
either. The entire system-library surface is:

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

(`azul-maps` / `azul-paint` in the repo are set up exactly like this.) Your crate
must build as a `cdylib` for Android, and pulls the android-activity glue + `ctor`
only on Android:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_os = "android")'.dependencies]
azul = { package = "azul-dll", version = "0.2", default-features = false, features = ["link-static", "android-activity"] }
ctor = "0.2"
```

That is the *entire* difference from a desktop app.

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

The `Info.plist` template + entitlements are in
[`scripts/ios/`](https://github.com/fschutt/azul/tree/master/scripts/ios).
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

## Installing & debugging the built app

You don't have to build anything to try the demos — every example is published
per-OS on the [release page](https://azul.rs/ui/release/$VERSION) (Demos section): a `.apk` for Android, a
device `.app` and a Simulator `.app` for iOS. To install a build (yours or a
downloaded one):

### Android (`.apk`)

The APKs are debug-signed, so they sideload directly.

```sh
# Over USB (enable Settings → Developer options → USB debugging first):
adb install azul-maps-android.apk
# Replace an existing install: adb install -r …; uninstall: adb uninstall com.azul.azul_maps
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

The Simulator slice (`<demo>-ios-sim.app.zip`) runs **unsigned** — easiest to try
on a Mac:

```sh
unzip azul-maps-ios-sim.app.zip
open -a Simulator                              # boot a simulator
xcrun simctl install booted azul-maps.app
xcrun simctl launch --console booted <bundle-id>   # --console streams stdout/stderr
```

### iOS device (`.app` → signed)

A physical iPhone needs the binary code-signed (a **free** Apple ID / personal
team works for a 7-day sideload). On a Mac:

```sh
codesign --force --sign "Apple Development: you@example.com (TEAMID)" \
  --entitlements scripts/ios/entitlements.plist azul-maps.app
xcrun devicectl device install app --device <udid> azul-maps.app
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

## Local testing

There are four rungs here, and each proves something the one below it cannot.
Start at the top — it is free and catches most of what actually breaks.

| Rung | Cost | Proves |
|---|---|---|
| 1. `cargo check` per target | seconds, **any OS**, no SDK | the platform code *compiles* |
| 2. Synthetic-event harness | seconds, no hardware | the event pipeline dispatches |
| 3. Emulator / simulator | one-time SDK install | the app *boots and draws* |
| 4. Real device | a cable | timing, sensors, the actual digitizer |

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

Everything comes from Homebrew and the headless `sdkmanager`:

```sh
brew install --cask android-commandlinetools android-platform-tools
brew install openjdk@17                       # sdkmanager/avdmanager need a JDK

export JAVA_HOME=/opt/homebrew/opt/openjdk@17
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools

# The emulator + one system image. Match the ABI to your host: arm64-v8a on
# Apple silicon, x86_64 on an Intel/AMD box — a mismatched image runs under
# full emulation and is unusably slow.
sdkmanager "emulator" "system-images;android-34;google_apis;arm64-v8a"
avdmanager create avd -n azul -k "system-images;android-34;google_apis;arm64-v8a"

"$ANDROID_HOME/emulator/emulator" -avd azul &
adb wait-for-device
adb install -r target/azul-maps.apk
adb logcat -s azul:V '*:S'
```

To *build* the APK you additionally need the NDK
(`sdkmanager "ndk;27.0.12077973"`) — that is the one large download, and it is
required for linking, not for checking.

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
