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

> **WIP** — `azul-doc codegen` is the daily-driver pipeline; the per-language defaults documented here are stable, but the IR layer (`doc/src/codegen/v2/`) is still being reorganized. Pin to a specific commit if you embed the generator in your own build.

`api.json` at the repository root is the single source of truth for the public surface of azul. Every binding (Rust, C, C++03–23, Python) is a deterministic transform of that file. The transform is implemented in the `azul-doc` crate and exposed as the `codegen` subcommand.

## The pipeline

```
api.json
   │
   ▼
┌──────────────────────────────────────────┐
│ IR (doc/src/codegen/v2/ir.rs)            │
│   structs · enums · functions · traits   │
└──────────────────────────────────────────┘
   │
   ▼
┌──────────────────────────────────────────┐
│ CodegenConfig                            │
│   target_lang · cabi mode · struct mode  │
└──────────────────────────────────────────┘
   │
   ├─► target/codegen/dll_api_internal.rs   (C-ABI bodies, #[no_mangle] gated)
   ├─► target/codegen/dll_api_external.rs   (extern "C" {…} declarations)
   ├─► target/codegen/reexports.rs          (public Rust API without Az prefix)
   ├─► target/codegen/python_api.rs         (PyO3 #[pyclass] wrappers)
   ├─► target/codegen/memtest.rs            (size/alignment assertions)
   ├─► target/codegen/azul.h                (C header)
   └─► target/codegen/azul{03,11,14,17,20,23}.hpp
```

The IR is built once per run (`build_ir_from_api` in `doc/src/codegen/v2/mod.rs`); each output is then a string formatter that walks the IR with a different `CodegenConfig`. Files land under `target/codegen/` because they are not committed — `dll/build.rs` re-runs the generator if any required file is missing.

## Running the generator

```sh
cargo run --release -p azul-doc -- codegen all
```

| subcommand | output |
|---|---|
| `codegen` or `codegen rust` | `target/codegen/azul.rs` |
| `codegen c` | `target/codegen/azul.h` |
| `codegen cpp` | `target/codegen/azul11.hpp` |
| `codegen python` | `target/codegen/python_api.rs` |
| `codegen all` | every output above plus `dll_api_internal.rs`, `dll_api_external.rs`, `reexports.rs`, `memtest.rs`, all six `azulNN.hpp` headers, brotli-compressed `api.json.br`, and `material_icons.ttf.br` |

`codegen all` is what `dll/build.rs` expects. Run it after editing `api.json` or after a fresh checkout, then build the DLL.

## Anatomy of `api.json`

The file is a top-level map of version strings (`"1.0.0-alpha1"`, …) to version data. Within a version the relevant keys for binding generation are:

- `apiversion` — integer; bumped when the FFI ABI breaks.
- `git` — short SHA pinned to that release.
- `installation.languages` — per-language install commands shown in the docs.
- `examples` — list of `{name, code, screenshot, description}` entries; `code` carries one path per supported language (`c`, `rust`, `python`, `cpp03`…`cpp23`). Files live under `examples/`.
- `classes` (one per module) — the type list itself. Each class declares its layout, derived traits, and method set; the IR builder turns this into `StructDef` / `EnumDef` / `FunctionDef` records.

Adding a type or method to `api.json` and re-running `codegen all` is the only step needed to expose it through every binding.

## Using the generator on your own FFI project

The codegen v2 architecture (`doc/src/codegen/v2/`) does not assume azul-specific types in its core layer. The IR is plain data; the language emitters (`lang_rust.rs`, `lang_c.rs`, `lang_cpp/`, `lang_python.rs`) read from it. To repurpose the pipeline:

1. Vendor `doc/src/codegen/v2/` into your project (one module).
2. Replace `api.json` with your own description following the same shape (`versions → modules → classes → methods`). The schema is documented inline in `doc/src/api.rs`.
3. Build an `IRBuilder` over your `VersionData` and call `CodeGenerator::generate(&ir, &config)` with one of the prebuilt configs (`CodegenConfig::dll_internal()`, `c_header()`, `cpp_header(CppStandard::Cpp17)`, `rust_public_api()`, `PythonConfig::python_extension()`).
4. Write the resulting `String` wherever your build expects it.

The `dllgen` module (`doc/src/dllgen/`) is the reference *consumer* of the generator — it builds the actual release artifacts. Its three submodules are:

- `dllgen::build` — `build_all_configs(version, output_dir, &Config)` iterates the requested platforms and feature combinations, invokes `cargo build --release --target …` for each, and copies the resulting `.dll`/`.so`/`.dylib` into `output_dir` under the public artifact name.
- `dllgen::license` — `format_license_authors(&[License])` renders a flat license report from `cargo_license::get_dependencies_from_cargo_lock` output.
- `dllgen::deploy` — collects assets per platform (`ReleaseAssets::collect`), validates that nothing is missing in `--strict` mode, and emits the release-page HTML plus an `nfpm.yaml` for `.deb`/`.rpm`/`.apk` packaging.

## Build-time integration in `dll/build.rs`

The DLL crate's build script (`dll/build.rs`) gates each generated file behind a Cargo feature:

| feature | required file |
|---|---|
| `cabi_internal` | `target/codegen/dll_api_internal.rs` |
| `cabi_external` | `target/codegen/dll_api_external.rs` |
| `python-extension` | `target/codegen/python_api.rs` |
| `rust_api` | `target/codegen/reexports.rs` |

If a feature is enabled and the matching file is missing, the build aborts with:

```
Missing generated file: dll_api_internal.rs
Run: cargo run --release -p azul-doc -- codegen all
```

The build script also emits `cargo:rerun-if-changed=` for each file, so editing `api.json` and re-running `codegen all` triggers a downstream rebuild without `cargo clean`.

## Three link modes

Generated outputs are paired with three Cargo feature combinations the consumer picks between:

| mode | feature | what gets compiled in |
|---|---|---|
| `build-dll` | `cabi_export` + `rust_api` + full backend | the shared library itself (`libazul.{so,dylib,dll}`) — exports `#[no_mangle]` symbols. |
| `link-static` | `cabi_export` + `rust_api` (no `cabi_external`) | static linking from a Rust binary; full crate stack compiled in. |
| `link-dynamic` | `cabi_external` + `rust_api` only | extern declarations only; expects `libazul.{so,dylib,dll}` at runtime. The internal crates (`azul-core`, `azul-css`, `azul-layout`) are not compiled. |

The C, C++, and Python bindings are downstream of `build-dll`: they consume the produced shared library plus the matching header. The Rust binding can use any of the three modes (covered on [Rust Bindings](bindings/rust.md)).

## Locating the library at link time

`dll/build.rs` searches for `libazul.{so,dylib,dll}` in this order when `cabi_external` is active:

1. Each path in the comma-separated `AZUL_DLL_PATH` environment variable.
2. `target/release/`, then `target/debug/` of the workspace root.
3. System paths — `/opt/homebrew/lib`, `/usr/local/lib`, `/usr/lib` (skipped on Windows).

If a local (non-system) match is found, the dylib is copied into `OUT_DIR` and into `target/{debug,release}/`, `target/{debug,release}/examples/`, and `target/{debug,release}/deps/`. On macOS, `install_name_tool -id @executable_path/libazul.dylib` is run on the copy so the binary finds it at runtime without `DYLD_LIBRARY_PATH`. If only `azul.lib` / `libazul.a` is present, the build falls back to `link-static` against it.

## Release artifacts produced by `dllgen`

`build_all_configs` produces these files under the configured output directory:

```
azul.dll          azul.dll.lib      azul.lib       azul.pyd
libazul.so        libazul.linux.a   azul.cpython.so
libazul.dylib     libazul.macos.a   azul.so
LICENSE-WINDOWS.txt   LICENSE-LINUX.txt   LICENSE-MACOS.txt
```

Each file is the result of one `cargo build --release --target <triple>` with the matching feature flags (`desktop-cdylib`, `desktop-staticlib`, or `python-extension`). The release pipeline additionally emits `azul.h`, `azul{03,11,14,17,20,23}.hpp`, `api.json`, and an `examples.zip` containing the per-language source files referenced from `api.json`'s `examples[].code` map.

## Determinism and what to commit

Nothing under `target/codegen/` is committed. The only things that matter for reproducible builds are:

- `api.json` (committed, hand-curated).
- The `azul-doc` source itself (`doc/src/codegen/v2/`).
- The Cargo lock file of the consuming binary.

A given `(api.json + azul-doc + cargo lockfile)` triple regenerates byte-identical headers and Rust bindings. The binary artifacts depend on the target triple and the host toolchain; pin both in CI to keep release archives reproducible.

## Next

- [Rust Bindings](bindings/rust.md) — link the DLL into a Cargo project via `link-static` or `link-dynamic`.
- [C Bindings](bindings/c.md) — `azul.h` and the dylib.
- [C++ Bindings](bindings/cpp.md) — pick a `azulNN.hpp` matching your C++ standard.
- [Python Bindings](bindings/python.md) — drop `azul.so` / `azul.pyd` next to your script.
