---
slug: bindings/rust
title: Rust Bindings
language: en
canonical_slug: bindings/rust
audience: external
maturity: mature
guide_order: 310
topic_only: false
short_desc: Using the Rust API without going through the C ABI
prerequisites: [hello-world, code-generation]
tracked_files:
  - api.json
  - dll/build.rs
  - doc/src/dllgen/build.rs
  - doc/src/dllgen/deploy.rs
  - doc/src/dllgen/license.rs
  - doc/src/dllgen/mod.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:50:43Z
---

# Rust Bindings

The Rust binding is the `azul` crate. It compiles in two modes selected by Cargo features. This page covers a fresh project that depends on azul; for the program itself, see [Hello, World — Rust](../hello-world/rust.md).

## Adding the dependency

```sh
cargo add azul
```

Or in `Cargo.toml`:

```toml
[dependencies]
azul = "0.3"
```

The default mode is `link-dynamic`. The crate links against a prebuilt `libazul` shared library on the system.

## link-dynamic (default)

Use this when the dylib is installed on the build machine. Install via the system package manager:

```sh
# Windows
choco install libazul
# Debian
apt install libazul
# Arch
yum install libazul
# macOS
brew install libazul
```

Or download the prebuilt artifact from `azul.rs/release/<version>/` and point `AZUL_DLL_PATH` at the directory:

```sh
export AZUL_DLL_PATH=/opt/azul/lib
cargo build --release
```

`AZUL_DLL_PATH` is a comma-separated list. Entries are absolute or relative to the workspace root. If unset, the build looks under `target/release` and `target/debug`, then falls back to system locations (`/opt/homebrew/lib`, `/usr/local/lib` on macOS; `/usr/local/lib`, `/usr/lib` on Linux).

A local match is copied next to your binary so the dylib is found at run time without `LD_LIBRARY_PATH` or `DYLD_LIBRARY_PATH`. If only the static archive is found (`libazul.a` or `azul.lib`), the build falls back to static linking.

## link-static

Compiles the framework into your binary. Slower first build, larger output, no external dylib to ship.

```toml
[dependencies]
azul = { version = "0.3", default-features = false, features = ["link-static"] }
```

## The prelude

```rust
use azul::prelude::*;
```

The prelude pulls in `App`, `AppConfig`, `Dom`, `RefAny`, `Update`, `LayoutCallbackInfo`, `CallbackInfo`, and `WindowCreateOptions`. Widgets are imported separately:

```rust
use azul::widgets::Button;
```

## Required `extern "C"` on callbacks

All callbacks must be `extern "C"`, including in pure-Rust applications using `link-static`. The framework holds raw function pointers and dispatches them through the same C ABI used by the C and Python bindings.

```rust
use azul::prelude::*;

extern "C" fn my_layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

extern "C" fn my_on_click(_: RefAny, _: CallbackInfo) -> Update {
    Update::DoNothing
}
```

Forgetting `extern "C"` produces a type-mismatch error at the `App::create` or `Button::set_on_click` call site, not at the callback definition.

## Choosing between the modes

- Greenfield Rust binary that ships azul as part of itself: `link-static`.
- CI matrix where you want short build times after the first job: `link-dynamic` with the dylib cached.
- Plug-in host that already ships `libazul.dylib`: `link-dynamic`.

Pick `link-dynamic` if you have no other constraints. Switch to `link-static` only when shipping a single self-contained binary outweighs the build-time and binary-size cost.

## Cross-compiling

Cross-compilation works for any target the underlying dependencies support. Add the target with `rustup target add <triple>` first.

| target triple | output |
|---|---|
| `x86_64-pc-windows-gnu` | `azul.dll`, `azul.dll.lib`, `azul.lib`, `azul.pyd` |
| `x86_64-unknown-linux-musl` | `libazul.so`, `libazul.linux.a`, `azul.cpython.so` |
| `x86_64-apple-darwin` | `libazul.dylib`, `libazul.macos.a`, `azul.so` |

## iOS

`aarch64-apple-ios` is supported but requires Xcode and `ios-deploy`:

```sh
brew install ios-deploy
xcode-select --install
cargo build --target aarch64-apple-ios
```

## Next

- [C Bindings](c.md) — link the same dylib from a C compiler.
- [C++ Bindings](cpp.md) — header-only wrapper over the C ABI.
- [Python Bindings](python.md) — `azul.so` / `azul.pyd` as a CPython extension.
