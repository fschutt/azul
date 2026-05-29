---
slug: mobile
title: Mobile (iOS & Android) in Rust
language: en
canonical_slug: mobile
audience: external
maturity: beta
guide_order: 285
topic_only: false
short_desc: What you actually do to ship your Azul app as an .apk / .ipa — all Rust, no Xcode or Android Studio
prerequisites: [hello-world]
tracked_files:
  - dll/build.rs
  - dll/src/desktop/shell2/android/mod.rs
  - dll/src/desktop/shell2/run.rs
last_generated_rev: ae991d834f665b88609d19ecf25dc88ea50e98a0
generated_at: 2026-05-29T00:00:00Z
default-search-keys:
  - App
  - WindowCreateOptions
  - link-static
  - aarch64-linux-android
  - aarch64-apple-ios
---

# Mobile (iOS & Android) in Rust

Your Azul app **is** the mobile app. The same `App::create(...).run(...)` you
wrote for desktop ships as an `.apk` or `.ipa` — there is no Kotlin/Swift app
layer to write, and you need **neither Xcode nor Android Studio**. This page is
the short "what do I do" path; [Mobile Deployment](mobile-deployment.md) is the
full reference (minimal-toolchain table, packaging internals, signing).

## Do I recompile azul? No — drop in the prebuilt library

Exactly like desktop: you ship the **prebuilt `libazul`** in the app bundle and
link your (small) app against it dynamically — you do **not** rebuild the
framework, and your app can be C, Rust, or any binding. A C `hello-world.c` runs
on Android and iOS the same way it runs on desktop: it calls `AzApp_create` /
`AzApp_run` from `azul.h` and links `libazul`.

- **Android:** put `libazul.so` in the APK under `lib/<abi>/` (e.g.
  `lib/arm64-v8a/`); your thin app `.so` links against it. Both are just files in
  the APK ZIP. (`build.rs` prints "place libazul.so in jniLibs/" for
  link-dynamic builds.)
- **iOS:** embed `libazul.dylib` in `MyApp.app/Frameworks/`, set the app
  binary's rpath to `@executable_path/Frameworks`. Embedded dylibs are allowed in
  the bundle (same mechanism Swift dynamic frameworks use).

Two real caveats, neither of which means recompiling azul:

1. **Per-architecture, not one file.** A mobile `libazul` is per-ABI/arch
   (`aarch64` for devices, `x86_64` for the emulator/simulator) — you bundle the
   slice(s) you target, not a single `.dll`. CI publishes a `libazul` per mobile
   target for download (alongside the desktop ones on the
   [release page](/releases)).
2. **The entry point still differs** (see [Step 1](#step-1--make-your-app-build-for-both-platforms)).
   On iOS your app's `main()` runs and calls `App::run`, so dynamic linking just
   works. On Android the OS calls `android_main` *inside `libazul`*, which needs
   your window options set first — so your thin app lib runs a few lines
   (`JNI_OnLoad` or an `android_main` shim) to call `AzApp_run` before handing
   off. That shim is the only Android-specific code; it does not rebuild azul.
3. **iOS signs both.** The app binary *and* the embedded `libazul.dylib` must be
   code-signed (see [Signing](#signing)).

This is the recommended, lowest-friction path. If you'd rather ship a **single**
artifact, use static linking instead (`link-static`, below) — your app + azul
compile into one `.so`/binary.

## What you need (the short list)

1. **The Rust target** — `rustup target add aarch64-linux-android` /
   `aarch64-apple-ios`.
2. **A tiny stub sysroot + `rust-lld`** — *not* the full NDK / iOS SDK. Azul
   renders on the CPU on mobile, so the entire link surface is `libandroid` +
   `liblog` + libc/libm/libdl on Android, and `Foundation` / `UIKit` /
   `CoreGraphics` + `libSystem` on iOS — all link *stubs* (see the
   [minimal-toolchain table](mobile-deployment.md#minimal-toolchain-only-the-stubs-you-need)).
   Rust's bundled `rust-lld` does the link; no external `clang`/`ld`/`xcrun`.
3. **A packager** to zip + sign the result (an `.apk`/`.ipa` is just a ZIP). See
   [Packaging](#packaging-rust-native).

## Step 1 — make your app build for both platforms

Factor your startup into one function. iOS keeps a normal `main()` (it runs
`UIApplicationMain` for you via `App::run`). Android has no `main()`: the OS
loads your cdylib and calls `android_main` *inside libazul*, which reads the
window options `App::run` stashed — so `start()` must run **before** that, from
a load-time constructor (the [`ctor`](https://crates.io/crates/ctor) crate):

```rust
use azul::prelude::*;

pub fn start() {
    let data = RefAny::new(DataModel { counter: 0 });
    App::create(data, AppConfig::create())
        .run(WindowCreateOptions::create(my_layout));   // Android: stashes + returns
}

#[cfg(not(target_os = "android"))]
fn main() { start(); }                       // desktop + iOS: main() runs start()

// Android: fires at dlopen / System.loadLibrary, before ANativeActivity_onCreate
// → libazul's android_main then picks up the stashed window options.
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() { start(); }
```

(That's the whole platform difference — `azul-maps` / `azul-paint` in the repo
are set up exactly like this.) For Android your crate also builds as a shared
library, and links the android-activity glue + `ctor` only on Android:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

# Android only: pull android-activity into libazul (it provides android_main /
# ANativeActivity_onCreate) and ctor for the load-time constructor above.
[target.'cfg(target_os = "android")'.dependencies]
azul = { package = "azul-dll", version = "0.2", default-features = false, features = ["link-static", "android-activity"] }
ctor = "0.2"
```

That is the *entire* difference from a desktop app.

## Step 2 — let `build.rs` do the platform wiring

Prefer Rust over shell. When you depend on `azul-dll`, its `build.rs` already
does the platform-specific setup for you on a mobile target: it links
`libandroid` + `liblog` (Android), wires the iOS linker, and warns you if the
target tools aren't found. You don't write a `build.gradle` or an Xcode project.

If you assemble a minimal cross-toolchain, set the linker once in
`.cargo/config.toml` (still no shell):

```toml
[target.aarch64-linux-android]
linker = "rust-lld"            # or the NDK clang if you have it
rustflags = ["-Clink-arg=--sysroot=/path/to/min-android-sysroot"]
```

## Step 3 — build

```sh
# Android (cdylib)
cargo build --release --target aarch64-linux-android -p my-app \
    --no-default-features --features "std,logging,link-static,a11y,android-activity"

# iOS (binary)
cargo build --release --target aarch64-apple-ios -p my-app \
    --no-default-features --features "std,logging,link-static,a11y"
```

## Packaging (Rust-native)

`cargo build` gives you the `.so` (Android) or executable (iOS). Turning that
into an installable bundle is a **post-build** step — and this is the one place
`build.rs` can't help, because it runs *before* your crate is compiled and never
sees the output binary. So packaging lives outside the build:

- **Recommended (Rust):** a workspace
  [`xtask`](https://github.com/matklad/cargo-xtask) — a plain Rust binary you run
  with `cargo xtask android` / `cargo xtask ios`. It lays out the bundle tree,
  copies the lib + a generated `AndroidManifest.xml` / `Info.plist`, and zips +
  signs. No shell, no IDE — just `cargo run`. This is the direction the project
  is moving toward so the whole flow is `cargo`-only.
- **Today:** the repo ships
  [`scripts/build-android.sh`](https://github.com/fschutt/azul/blob/master/scripts/build-android.sh)
  and [`scripts/build-ios.sh`](https://github.com/fschutt/azul/blob/master/scripts/build-ios.sh),
  which do exactly that (cross-compile → bundle → sign) using only CLI tools
  (`aapt2`/`zipalign`/`apksigner`; `zip`). Run e.g.
  `bash scripts/build-android.sh aarch64-linux-android MyApp com.me.myapp`.

Why a script at all, if we prefer Rust? Zipping and signing happen *after*
`cargo build`, so they can't be a `build.rs` step — they're either a separate
Rust tool (`xtask`) or a shell one-liner. The bundle formats are plain ZIPs, so
the "script" is genuinely just *copy a few files + zip + sign*.

## Signing

- **Android:** `apksigner` with a keystore (a generated debug keystore is fine
  for sideloading / testing). CLI only.
- **iOS:** signing is the only Apple-specific step and it needs **no Mac** —
  [`rcodesign`](https://github.com/indygreg/apple-platform-rs) (the Rust
  `apple-codesign` crate) signs and submits to Apple's notarization web API from
  Linux or Windows. You still need an Apple Developer ID certificate
  (a file), but no Xcode. The iOS *simulator* needs no signing.

## See also

- [Mobile Deployment](mobile-deployment.md) — full reference: the minimal-stub
  table, packaging internals, cross-compiling iOS from Linux, permissions.
- [Realtime Media and Devices](realtime-media.md) — camera/mic/sensors/gamepad
  on mobile.
- [hello-world](hello-world.md) — the app you're shipping.
