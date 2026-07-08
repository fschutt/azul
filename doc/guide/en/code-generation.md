---
slug: code-generation
title: Code Generation
language: en
canonical_slug: code-generation
audience: external
maturity: wip
guide_order: 300
topic_only: false
short_desc: How `azul-doc` regenerates bindings from `api.json`
prerequisites: [hello-world, architecture]
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

# Code Generation

## Introduction

*WIP.* The per-language defaults are stable, but pin to a specific commit if you embed the generator in your own build.

`api.json` at the repository root is the single source of truth for the public surface of azul. Every binding (Rust, C, C++03–23, Python) is a deterministic transform of that file. The transform is exposed through the `azul-doc` binary's `codegen` subcommand.

## Running the generator

```sh
cargo run --release -p azul-doc -- codegen all
```

- `codegen` or `codegen rust`. Writes `target/codegen/azul.rs`.
- `codegen c`. Writes `target/codegen/azul.h`.
- `codegen cpp`. Writes `target/codegen/azul11.hpp`.
- `codegen python`. Writes `target/codegen/python_api.rs`.
- `codegen all`. Every output above, plus internal C-ABI files, all six `azulNN.hpp` headers, and brotli-compressed asset blobs.

`codegen all` is what `dll/build.rs` expects. Run it after editing `api.json` or after a fresh checkout, then build the DLL.

The generated files land under `target/codegen/`. Nothing in that directory is committed. The build script re-runs the generator if any required file is missing.

## Anatomy of api.json

The file is a top-level map of version strings (`"$VERSION"`, …) to version data. Within a version the relevant keys are:

- `apiversion` — integer, bumped when the FFI ABI breaks.
- `git` — short SHA pinned to that release.
- `installation.languages` — per-language install commands shown in the docs.
- `examples` — list of `{name, code, screenshot, description}` entries. `code` carries one path per supported language. Files live under `examples/`.
- `classes` (one per module) — the type list. Each class declares its layout, derived traits, and method set.

Adding a type or method to `api.json` and re-running `codegen all` is the only step needed to expose it through every binding.

## Build-time integration

The DLL crate's build script gates each generated file behind a Cargo feature. If a feature is enabled and the matching file is missing, the build aborts with:

```text
Missing generated file: dll_api_internal.rs
Run: cargo run --release -p azul-doc -- codegen all
```

Editing `api.json` and re-running `codegen all` triggers a downstream rebuild without `cargo clean`.

## Three link modes

Generated outputs pair with three Cargo feature combinations:

- `build-dll`. The shared library itself (`libazul.{so,dylib,dll}`) with exported symbols.
- `link-static`. Static linking from a Rust binary. The full crate stack is compiled in.
- `link-dynamic`. Extern declarations only. Expects `libazul.{so,dylib,dll}` at runtime.

The C, C++, and Python bindings are downstream of `build-dll`: they consume the produced shared library plus the matching header. The Rust binding can use any of the three modes.

## Locating the library at link time

`dll/build.rs` searches for `libazul.{so,dylib,dll}` in this order when dynamic linking is active:

1. Each path in the comma-separated `AZ_DLL_PATH` environment variable.
2. `target/release/`, then `target/debug/` of the workspace root.
3. System paths — `/opt/homebrew/lib`, `/usr/local/lib`, `/usr/lib` (skipped on Windows).

If a local match is found, the dylib is copied next to the build artifacts so the binary loads it without `DYLD_LIBRARY_PATH`.

## Determinism

A given `(api.json + azul-doc + cargo lockfile)` triple regenerates byte-identical headers and Rust bindings. Binary artifacts depend on the target triple and host toolchain; pin both in CI to keep release archives reproducible.


## Coming Up Next

- [Deploying to the web](deploying-web.md) — Building for the browser via WASM
- [Headless Rendering](headless-rendering.md) — Running the pipeline without a window
- [Debugging](debugging.md) — Debug overlays, the inspector, and structured logging
