---
slug: mobile-deployment
title: Mobile Deployment (iOS and Android)
language: en
canonical_slug: mobile-deployment
audience: external
maturity: beta
guide_order: 290
topic_only: false
short_desc: How to cross-compile, package, and deploy an Azul app on iOS and Android
prerequisites: [hello-world, web-deployment]
tracked_files:
  - dll/Cargo.toml
  - scripts/mobile-check-all.sh
last_generated_rev: 754b7f00e088960c14db598f64fa200dacc28bf1
generated_at: 2026-05-21T00:00:00Z
default-search-keys:
  - link-static
  - aarch64-apple-ios
  - aarch64-linux-android
  - staticlib
  - cargo-ndk
  - XCFramework
---

# Mobile Deployment (iOS and Android)

## Introduction

An Azul application is a Rust library (`azul-dll`) plus your app code. On
desktop you link it into an executable; on mobile you cross-compile it to a
**static library** and link that into a thin platform shell (an Xcode app or an
Android `Activity`). The shell owns the OS surface + lifecycle and hands it to
Azul; everything else - layout, rendering, callbacks, the capture/audio/UDP
APIs from [Realtime Media](realtime-media.md) - is the same code as desktop.

This guide covers cross-compilation, packaging, and the per-platform glue.

## Supported targets

The mobile build is checked against five Rust targets (see
`scripts/mobile-check-all.sh`):

| Target | Use |
|---|---|
| `aarch64-apple-ios` | iOS device (arm64) |
| `aarch64-apple-ios-sim` | iOS simulator on Apple silicon |
| `x86_64-apple-ios` | iOS simulator on Intel Macs |
| `aarch64-linux-android` | Android device (arm64-v8a) |
| `x86_64-linux-android` | Android emulator |

Install them with `rustup target add <triple>`.

## Building the static library

Mobile builds use the `link-static` feature and **no default features** (the
desktop windowing/renderer defaults pull in things mobile doesn't want). The
canonical build line, per target, is:

```sh
cargo build --release --target aarch64-apple-ios \
    -p azul-dll --no-default-features \
    --features "std,logging,link-static,a11y"
```

This produces a static library (`libazul.a`) under
`target/<triple>/release/`. To verify all five targets compile without building
artifacts, run `bash scripts/mobile-check-all.sh` (it runs `cargo check` across
the matrix; this is the gate kept green as the mobile port lands).

Your app calls into the library through the generated C API (`azul.h`,
emitted by `cargo run --release --bin azul-doc -- codegen c`) - the same
`AzApp_create` / `AzApp_run` / `Az*` entry points the C and other-language
bindings use. From Rust you can instead depend on `azul-dll` directly with the
same features.

## iOS

1. **Build for device + simulator.** Build `aarch64-apple-ios` (device) and the
   simulator slice(s) you need (`aarch64-apple-ios-sim` on Apple silicon).
2. **Make an XCFramework.** Bundle the per-slice `libazul.a` into an
   `.xcframework` so Xcode picks the right slice automatically:
   ```sh
   xcodebuild -create-xcframework \
       -library target/aarch64-apple-ios/release/libazul.a \
       -library target/aarch64-apple-ios-sim/release/libazul.a \
       -output Azul.xcframework
   ```
   (Use `lipo` only to merge slices of the *same* platform, e.g. two simulator
   archs; device + simulator must stay separate xcframework entries.)
3. **Link it.** Add `Azul.xcframework` to your Xcode target, add `azul.h` to the
   bridging header, and link the system frameworks the on-device backends need
   (Metal, AVFoundation, CoreMotion, CoreLocation, GameController).
4. **Entry + surface.** Your `UIViewController` creates the drawable surface and
   hands it to Azul, then drives the run loop. (The mobile windowing entry is
   the integration seam between UIKit and Azul's event loop.)
5. **Permissions.** Add the usage strings to `Info.plist` for whatever device
   APIs you use: `NSCameraUsageDescription`, `NSMicrophoneUsageDescription`,
   `NSLocationWhenInUseUsageDescription`, `NSMotionUsageDescription`.

## Android

1. **Install the NDK** and the Android Rust targets. The simplest build path is
   [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk), which sets the NDK
   linker + sysroot for you:
   ```sh
   cargo ndk -t arm64-v8a -t x86_64 -o ./jniLibs build --release \
       -p azul-dll --no-default-features --features "std,logging,link-static,a11y"
   ```
   Without `cargo-ndk`, point each target's linker at the NDK clang in
   `.cargo/config.toml` and build per target as in the iOS section.
2. **Bridge.** Call the C API from a `NativeActivity`, or from Kotlin/Java via a
   small JNI shim that forwards to `AzApp_*`. The native side renders into the
   `ANativeWindow` / `Surface` the Activity provides.
3. **Package.** Put the per-ABI libraries under `app/src/main/jniLibs/<abi>/`
   and let Gradle pack them into the APK/AAB.
4. **Permissions.** Declare them in `AndroidManifest.xml`: `CAMERA`,
   `RECORD_AUDIO`, `ACCESS_FINE_LOCATION`, `INTERNET` (for UDP), and request the
   dangerous ones at runtime.

## On-device backends

The cross-platform widget/handle surfaces ([Realtime Media](realtime-media.md),
sensors, gamepad, geolocation) bind to native APIs at runtime:
CoreMotion/AVFoundation/CoreLocation/GameController on iOS, the
`SensorManager`/Camera2/`LocationManager`/`InputDevice` stack on Android. The
permission prompts are driven by the manifest entries above plus the
permission-as-DOM probes (e.g. `Dom::create_geolocation_probe`).

## Testing without a device

Cross-compilation correctness is covered by `scripts/mobile-check-all.sh` (all
five targets must `cargo check` clean). Event/runtime paths are covered by the
synthetic-event harness without hardware - see [e2e-testing](e2e-testing.md) and
`layout/tests/synthetic_events.rs`.

## See also

- [Web Deployment](web-deployment.md) - the WASM target, by comparison.
- [Realtime Media and Devices](realtime-media.md) - the device APIs you'll be
  requesting permissions for.
- [hello-world](hello-world.md) - the app you're packaging.
