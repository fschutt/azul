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

The Rust binding is the `azul` crate (the published wrapper around `azul-dll`). The same crate compiles in three modes, selected at the consumer's Cargo feature flags. This page covers a fresh project that depends on azul; for the program itself, see [Hello, World — Rust](../hello-world/rust.md).

## Cargo.toml — link-static (default)

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"

[dependencies]
azul = { git = "https://github.com/maps4print/azul" }
```

`link-static` is the default feature set. Cargo compiles `azul-dll`, `azul-core`, `azul-css`, `azul-layout`, and the bundled WebRender fork into your binary. First build runs ten minutes on a recent laptop; subsequent builds are incremental.

## Cargo.toml — link-dynamic

Use this when the dylib already exists on the build machine (CI cache, vendored release archive) and you do not want to recompile the framework.

```toml
[dependencies]
azul = { git = "https://github.com/maps4print/azul", default-features = false, features = ["link-dynamic"] }
```

The `cabi_external` feature compiles in only `extern "C" { … }` declarations from `target/codegen/dll_api_external.rs`. None of the internal crates are pulled into the dependency graph.

## Pointing the linker at the dylib

`dll/build.rs` searches the following directories, in order, for `libazul.dylib` / `libazul.so` / `azul.dll`:

1. Each entry in `AZUL_DLL_PATH` (comma-separated, absolute or relative to the workspace root).
2. `target/release/`, then `target/debug/` of the workspace root.
3. On macOS: `/opt/homebrew/lib`, then `/usr/local/lib`. On Linux: `/usr/local/lib`, then `/usr/lib`.

```sh
export AZUL_DLL_PATH=/opt/azul/lib
cargo build --release
```

A local (non-system) match is copied into `OUT_DIR`, `target/{debug,release}/`, `target/{debug,release}/examples/`, and `target/{debug,release}/deps/`. On macOS, `install_name_tool -id @executable_path/libazul.dylib` rewrites the install name so the binary finds the dylib at runtime without `DYLD_LIBRARY_PATH`. On Linux, copying the dylib next to the binary yields the same effect via the default loader search path.

If only the static archive (`azul.lib` / `libazul.a`) is found, the build falls back to static linking against it.

## Building the dylib once

```sh
cargo build -p azul-dll --release --no-default-features --features build-dll
```

The output is `target/release/libazul.{so,dylib}` or `target/release/azul.dll`. The matching import library on Windows is `target/release/azul.dll.lib`. Copy these to a stable directory and point `AZUL_DLL_PATH` at it.

## Workspace setup with both modes

A workspace can expose two binary targets — one statically linked, one dynamically linked — by feature-gating the `azul` dependency:

```toml
[features]
default = ["static"]
static = ["azul/link-static"]
dynamic = ["azul/link-dynamic"]

[dependencies]
azul = { git = "https://github.com/maps4print/azul", default-features = false }
```

Then build with `cargo build --features static` or `cargo build --features dynamic`. The chosen feature flips which generated file (`dll_api_internal.rs` vs `dll_api_external.rs`) is included by `dll/src/lib.rs`.

## Verifying the link

`AZUL_DLL_PATH` and the chosen mode are echoed by `dll/build.rs` as `cargo:warning=` lines:

```
warning: Linking against libazul.dylib [local]: /Users/me/azul/target/release
```

If the build prints `Linking against libazul.a [static fallback]`, the dylib was not found and the build downgraded to static archive linking. Set `AZUL_DLL_PATH` and re-run.

If the build aborts with `Missing generated file: dll_api_external.rs`, `target/codegen/` is empty. Run:

```sh
cargo run --release -p azul-doc -- codegen all
```

once, then rebuild. See [Code Generation](../code-generation.md) for what this command produces.

## Required `extern "C"` on callbacks

All callbacks crossing the C-ABI must be `extern "C"`, including in pure-Rust applications using `link-static`. The framework holds raw function pointers internally and dispatches them through the same C-ABI surface that the C and Python bindings use.

```rust,no_run
use azul::prelude::*;

extern "C" fn my_layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

extern "C" fn my_on_click(_: RefAny, _: CallbackInfo) -> Update {
    Update::DoNothing
}
```

Forgetting `extern "C"` produces a type-mismatch error at the `App::create` / `set_on_click` call site, not at the callback definition.

## Cross-compiling

`build-dll` works for any target the underlying dependencies support. The release pipeline (`doc/src/dllgen/build.rs`) drives:

| target triple | output |
|---|---|
| `x86_64-pc-windows-gnu` | `azul.dll`, `azul.dll.lib`, `azul.lib`, `azul.pyd` |
| `x86_64-unknown-linux-musl` | `libazul.so`, `libazul.linux.a`, `azul.cpython.so` |
| `x86_64-apple-darwin` | `libazul.dylib`, `libazul.macos.a`, `azul.so` |

Add the target with `rustup target add <triple>` first; the build script does this automatically when invoked through `azul-doc deploy --build=all`.

## iOS

Setting target to `aarch64-apple-ios` is supported but requires Xcode and `ios-deploy`:

```sh
brew install ios-deploy
xcode-select --install
cargo build -p azul-dll --target aarch64-apple-ios --features build-dll
```

`dll/build.rs` writes `.cargo/config.toml` and `scripts/ios-runner.sh` on the first iOS build so `cargo run --target aarch64-apple-ios` can deploy to a connected device. Set `AZUL_IOS_SETUP=disable` to suppress this auto-configuration.

## Choosing between the modes

| use case | mode |
|---|---|
| Greenfield Rust binary that ships azul as part of itself | `link-static` |
| CI matrix where you want short build times after the first job | `link-dynamic` with the dylib cached |
| Plug-in host that already ships `libazul.dylib` | `link-dynamic` |
| Building the dylib for distribution | `build-dll` |

Pick `link-static` if you have no other constraints. Switch to `link-dynamic` only when the binary size or build-time penalty of compiling the framework is unacceptable.

## Next

- [C Bindings](c.md) — link the same dylib from a C compiler.
- [C++ Bindings](cpp.md) — header-only wrapper over the C ABI, one header per C++ standard.
- [Python Bindings](python.md) — `azul.so` / `azul.pyd` as a CPython extension.
