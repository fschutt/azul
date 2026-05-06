---
slug: build-and-codegen
title: FFI Codegen
language: en
canonical_slug: build-and-codegen
audience: contributor
maturity: mature
guide_order: null
topic_only: false
short_desc: How `cargo build` cascades and the codegen pass
prerequisites: [code-organization]
tracked_files:
  - api.json
  - dll/build.rs
  - dll/src/lib.rs
  - doc/src/dllgen/mod.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Build System and FFI Codegen

Azul's public surface is generated from a single source of truth: [`api.json`](../../../../api.json) at the workspace root. A tool crate (`azul-doc`) reads it, builds an intermediate representation, and emits Rust/C/C++/Python bindings into `target/codegen/`. The `azul-dll` crate then `include!()`s those generated files behind feature flags. Every binding for every language stays in lockstep because they all derive from the same JSON.

```rust
api.json ──► azul-doc codegen all ──► target/codegen/
                                       ├── dll_api_internal.rs    (C-ABI bodies)
                                       ├── dll_api_external.rs    (extern "C" decls)
                                       ├── reexports.rs           (public Rust API)
                                       ├── azul.h                 (C header)
                                       ├── azul{03,11,14,17,20,23}.hpp  (C++ headers)
                                       ├── azul.rs                (legacy Rust API)
                                       ├── python_api.rs          (PyO3 module)
                                       ├── memtest.rs             (size/align tests)
                                       └── api.json.br            (compressed for web backend)
                                              │
                              ┌───────────────┼─────────────────────┐
                              ▼               ▼                     ▼
                         dll/build.rs   dll/src/lib.rs        external consumers
                       (sanity checks +  include!()-s the      (C / C++ / Python)
                        dynamic linking) generated .rs files
```

## Regenerating bindings

Whenever you edit `api.json` (or any generator), run:

```bash
cd doc && cargo run --release -- codegen all
```

This walks every standard target. See [`GenerationTargets::generate_all`](../../../../doc/src/codegen/v2/generator.rs) in `doc/src/codegen/v2/generator.rs`. Granular targets exist if you want to iterate quickly:

```bash
cargo run --release -p azul-doc -- codegen rust    # → target/codegen/azul.rs
cargo run --release -p azul-doc -- codegen c       # → target/codegen/azul.h
cargo run --release -p azul-doc -- codegen cpp     # → target/codegen/azul11.hpp
cargo run --release -p azul-doc -- codegen python  # → target/codegen/python_api.rs
```rust

`check_generated_files()` in `dll/build.rs` refuses to compile when a feature is enabled but the matching generated file is missing. The panic message tells you exactly which command to run.

## api.json schema

Top-level shape: `{ "<version>": { "api": { "<module>": { "classes": { "<TypeName>": { ... } }, "functions": { ... } }, ... }, ... } }`. The current version is keyed `"1.0.0-alpha1"` in `api.json`.

Each class entry carries:

- **`external`** — fully-qualified Rust path (e.g. `"azul_core::dom::Dom"`). The internal binding `transmute`s between the prefixed C-ABI struct and this internal type.
- **`derive`** — derives to apply (`Debug`, `Clone`, `PartialEq`, …). Used by both the generated public API and trait codegen.
- **`struct_fields`** or **`enum_fields`** — POD fields or variant list.
- **`repr`** — `"C"`, `"C, u8"`, etc. Drives the layout the codegen emits.
- **`functions`** — methods. Each has `fn_args` and `returns` plus optional doc strings.
- **`callback_typedef`** — for function pointer types like `LayoutCallbackType`.

Module-level `doc:` arrays propagate as rustdoc on the generated module.

Because `api.json` is hand-curated and large (~85 K lines), `doc/src/main.rs` exposes a `normalize` subcommand that rewrites array types, type aliases, and enum variants to a canonical shape; run it after any edit:

```bash
cargo run --release -p azul-doc -- normalize
```

## Adding a new type to the API

1. **Pick a module.** Open `api.json` and find the closest `"<module>"` block (e.g. `window`, `dom`, `css`, `callbacks`).
2. **Add the class.** Inside `"classes"`, add an entry. Minimum fields:

   ```json
   "MyType": {
       "external": "azul_core::my_module::MyType",
       "derive": ["Debug", "Clone", "PartialEq"],
       "struct_fields": [
           { "field_a": { "type": "u32" } },
           { "field_b": { "type": "AzString" } }
       ],
       "repr": "C"
   }
   ```rust

3. **Define the type in Rust.** It must live at the path declared in `external`, be `#[repr(C)]`, and match the field layout exactly. Field name and order must match `api.json`.
4. **Run `normalize`** to canonicalize the new entry: `cargo run -p azul-doc -- normalize`.
5. **Run `codegen all`**: `cargo run --release -p azul-doc -- codegen all`.
6. **Verify size and alignment.** `cargo test -p azul-dll` runs the generated `memtest.rs` which asserts `mem::size_of` and `mem::align_of` match between the generated prefixed type and the internal type. A mismatch means the field list in `api.json` doesn't agree with the Rust struct.

The `Az`-prefixed type appears automatically in C/C++/Python bindings, with all derived traits routed through C-ABI functions (`AzMyType_deepCopy`, `AzMyType_eq`, `AzMyType_delete`, etc.). The unprefixed Rust API is generated into `target/codegen/reexports.rs`, exposing it as `azul::my_module::MyType`.

## Adding a new function

Inside the same module entry in `api.json`:

```json
"functions": {
    "do_thing": {
        "doc": "One-line summary used as rustdoc.",
        "fn_args": [
            { "type": "AzMyType", "ref": "ref" },
            { "type": "u32" }
        ],
        "returns": { "type": "AzString" }
    }
}
```rust

Implement the function in the appropriate crate (`azul-core`, `azul-layout`, or `azul-dll`). The codegen emits `extern "C" fn AzMyType_do_thing(...)` whose body `transmute`s arguments to internal types and calls your Rust function. See `doc/src/codegen/v2/rust/static_binding.rs` for the exact emission rules.

## Codegen v2 internals

`doc/src/codegen/v2/mod.rs` is the entry point and documents the architecture. It has three pieces:

- **IR** ([`ir.rs`](../../../../doc/src/codegen/v2/ir.rs), [`ir_builder.rs`](../../../../doc/src/codegen/v2/ir_builder.rs)) — `CodegenIR` holds `Vec<StructDef>`, `Vec<EnumDef>`, `Vec<FunctionDef>`, derives, type-to-module map, and module docs. Built once from `ApiData`.
- **Config** ([`config.rs`](../../../../doc/src/codegen/v2/config.rs)) — `CodegenConfig` selects target language and which blocks to emit:
  - `CodegenConfig::dll_internal()` — types + transmute-bodied C-ABI functions; emitted to `dll_api_internal.rs`.
  - `CodegenConfig::dll_dynamic()` — types + `extern "C" { ... }` declarations only; emitted to `dll_api_external.rs`.
  - `CodegenConfig::c_header()`, `cpp_header(standard)` — emitted to `azul.h` / `azul{NN}.hpp`.
  - `CodegenConfig::rust_public_api()` — re-exports without the `Az` prefix; emitted to `azul.rs` (legacy; `reexports.rs` is the live one).
  - `CodegenConfig::memtest()` — `assert_eq!(mem::size_of::<Az…>(), mem::size_of::<…>())`; emitted to `memtest.rs`.
- **Emitters** ([`lang_rust.rs`](../../../../doc/src/codegen/v2/lang_rust.rs), [`lang_c.rs`](../../../../doc/src/codegen/v2/lang_c.rs), [`lang_cpp/`](../../../../doc/src/codegen/v2/lang_cpp/), [`lang_python.rs`](../../../../doc/src/codegen/v2/lang_python.rs), [`lang_reexports.rs`](../../../../doc/src/codegen/v2/lang_reexports.rs)) — language-specific. Python is generated through its own `PythonConfig` because PyO3 needs `#[pyclass]` attributes and different trait machinery. See the design note in `doc/src/codegen/v2/mod.rs`.

Adding a new emission target is a config and emitter change. Nothing else in the pipeline touches the IR.

## Three link modes

`dll/Cargo.toml` defines the feature compositions. They differ in which generated file is included and which platform code is compiled.

- **`build-dll`.** Builds the shared library itself (`libazul.dylib` / `azul.dll` / `libazul.so`). Gates `cabi_export` + `rust_api` + `_internal_deps`. Binding source is `dll_api_internal.rs` with `#[no_mangle]`.
- **`link-static`.** Rust apps statically linking the entire azul stack. Gates `cabi_export` + `rust_api` + `_internal_deps`. Binding source is `dll_api_internal.rs` with `#[no_mangle]`.
- **`link-dynamic`.** Apps loading a prebuilt `libazul` at runtime. Gates `cabi_external` + `rust_api`. Binding source is `dll_api_external.rs` (`extern "C" { ... }`).

The granular building blocks:

- **`cabi_internal`** — pulls in `azul-core`, `azul-css`, `azul-layout`. Compiles the C-ABI function *bodies* (transmute-based). Used by `build-dll` and `link-static`.
- **`cabi_export`** — adds `#[no_mangle]` to those bodies so dlsym / C / C++ / Python can find them. Implies `cabi_internal`. Both `build-dll` and `link-static` enable this so the web backend's `remill` lifter can dlsym function names.
- **`cabi_external`** — emits `extern "C" { fn ... }` declarations only. No bodies, no internal crates. The cdylib must be on the link path at compile time and at runtime.
- **`rust_api`** — pulls in `target/codegen/reexports.rs`, exposing `azul::dom::Dom`, `azul::app::App`, etc.

`dll/src/lib.rs` shows how the feature gates choose which `include!()` to take.

```rust,ignore
#[cfg(feature = "cabi_internal")]
mod __ffi_internal {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/codegen/dll_api_internal.rs"
    ));
}

#[cfg(all(feature = "cabi_external", not(feature = "cabi_internal")))]
mod __ffi_external {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/codegen/dll_api_external.rs"
    ));
}

#[cfg(feature = "rust_api")]
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/codegen/reexports.rs"
));
```

The two `cabi_*` features are wired so `cabi_internal` wins if both are on (see the `not(feature = "cabi_internal")` guard on the external import). `link-dynamic` therefore deliberately omits `cabi_internal`.

## How dll/build.rs resolves a dynamic library

`configure_dynamic_linking` in `dll/build.rs` only fires when `cabi_external` is on and `cabi_internal` is off. Search order, top to bottom:

1. **`AZUL_DLL_PATH`** — comma-separated, absolute or workspace-relative. Per-entry, `printf cargo:warning=Linking against ...`.
2. **`target/release/`**, **`target/debug/`** — local builds. `target/debug/` triggers an extra warning so contributors don't accidentally link against an unoptimized library.
3. **System paths** — `/opt/homebrew/lib`, `/usr/local/lib`, `/usr/lib`. No copy, no rpath.

For local hits, the build script:

- Copies the dylib into `OUT_DIR` (avoids the cdylib self-link error: "can't link a dylib with itself").
- On Apple, runs `install_name_tool -id @executable_path/libazul.dylib` so the binary resolves the dylib next to itself at runtime — no `DYLD_LIBRARY_PATH` required.
- Copies the dylib into `target/<profile>/`, `target/<profile>/examples/`, and `target/<profile>/deps/` so `cargo run --example`, plain binaries, and dep tests all find it.

If only a static library (`libazul.a` / `azul.lib`) is found, the script falls back to `cargo:rustc-link-lib=static=azul`. If nothing is found, the build still proceeds, but the linker errors at link time with `-lazul` unresolved; the build script prints the search list as `cargo:warning` so you can tell `AZUL_DLL_PATH` what to point at.

## Allocator selection

`dll/src/lib.rs` picks one global allocator at compile time:

- **`allocator_mimalloc`.** Uses `mimalloc::MiMalloc`. Page release via `mi_collect(true)`.
- **`allocator_jemalloc`.** Uses `tikv_jemallocator::Jemalloc`. Page release via `mallctl("arena.0.purge")`.
- **(default).** System allocator. Page release via `malloc_zone_pressure_relief` on macOS, no-op elsewhere.

These are mutually exclusive — enabling both is a compile error in `Cargo.toml`'s feature graph. Because azul exposes a C ABI, the host application keeps its own allocator unchanged. Only azul's internal allocations route through the chosen one.

`az_purge_allocator()` in `dll/src/lib.rs`, gated on `cabi_export`, is the one-shot pressure-relief hook. Call it after large transient allocations are freed (e.g. after a layout pass). The desktop event loop wires this in as part of frame-end cleanup.

## Compressed asset embedding

`compress_debugger_assets()` in `dll/build.rs` brotli-compresses three debugger UI files at build time:

- `dll/src/desktop/shell2/common/debugger/debugger.{css,js,html}` → `OUT_DIR/{name}.br`

These are then `include_bytes!`ed and served with `Content-Encoding: br`. Quality is hard-coded at 11 (max), which is slow but only runs when the source changes (`cargo:rerun-if-changed=...`).

`generate_compressed_api_json` and `compress_material_icons_font` in [`doc/src/codegen/v2/mod.rs`](../../../../doc/src/codegen/v2/mod.rs) do the same for two larger payloads during `codegen all`:

- `api.json` → `target/codegen/api.json.br` (~3.7 MB → ~150 KB). Embedded into the web backend so it can classify functions at runtime without shipping the full JSON.
- `MaterialIcons-Regular.ttf` → `target/codegen/material_icons.ttf.br` (~348 KB → ~80 KB). The compressed font replaces the raw `material_icons::FONT` constant; the linker dead-code-eliminates the uncompressed copy because nothing references it directly.

## iOS automation

`configure_ios()` in `dll/build.rs` runs only on iOS targets and only when `AZUL_IOS_SETUP` isn't `"disable"`. It checks for `xcode-select` and `ios-deploy`, then writes a default `.cargo/config.toml` and `scripts/ios-runner.sh` so `cargo run --target aarch64-apple-ios` deploys to a connected device. Existing files are preserved.

## Python extension

`python-extension` is a meta-feature that enables `build-dll` + `pyo3` + `use_pyo3_logger` + `link-static`. The build emits a cdylib whose `PyInit_azul` is generated from `target/codegen/python_api.rs` in `dll/src/lib.rs`:

```rust,ignore
#[cfg(feature = "python-extension")]
mod python {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/codegen/python_api.rs"
    ));
}

#[cfg(feature = "python-extension")]
pub use python::azul;
```rust

Build with `cargo build --release -p azul-dll --features python-extension`. On macOS `dll/build.rs` adds `-Wl,-undefined,dynamic_lookup` so the symbol references into the Python interpreter resolve at load time.

The Python codegen lives in [`doc/src/codegen/v2/lang_python.rs`](../../../../doc/src/codegen/v2/lang_python.rs) and uses its own `PythonConfig::python_extension()` because PyO3 needs different attributes and trait routing. See the design note in `doc/src/codegen/v2/mod.rs`.

## Memtest

Every release build of `dll/` runs `cargo test`, which compiles the auto-generated `memtest.rs`:

```rust,ignore
// excerpt from target/codegen/memtest.rs
#[test]
fn assert_size_align_AzDom() {
    assert_eq!(mem::size_of::<AzDom>(), mem::size_of::<azul_core::dom::Dom>());
    assert_eq!(mem::align_of::<AzDom>(), mem::align_of::<azul_core::dom::Dom>());
}
```

A test failure here means `api.json` and the internal type drifted apart, and a transmute would corrupt memory. Fix by updating `api.json` (or the internal type) and re-running `codegen all`.

## Release-binary builder

`doc/src/dllgen/` is a separate concern: it drives `cargo build` for every link-mode × platform × language combination, signs binaries, generates `.deb` / `.rpm` packages via `nfpm`, and stages everything for the website. Entry point: `cargo run --release -p azul-doc -- deploy`.

```rust,ignore
// doc/src/dllgen/mod.rs
pub mod build;     // cargo build orchestration per platform
pub mod deploy;    // nfpm config, releases index, asset copies
pub mod license;   // license file generation per release
```

`build_all_configs` in `doc/src/dllgen/build.rs` enumerates the build matrix. Each entry is `(target_triple, cargo_features, source_artifact, dest_filename)`. The deploy step then assembles a downloadable bundle per language with the right header/binary pairs.

The deploy command is invoked by CI. Locally you typically don't run it. `azul-doc deploy debug` skips minification and is useful when iterating on website templates.

## Common build problems

**"Missing generated file: dll_api_internal.rs".** You enabled `cabi_internal` (or any of `build-dll` / `link-static`) but haven't run codegen. Fix: `cd doc && cargo run --release -- codegen all`.

**"can't link a dylib with itself".** Happens on `link-dynamic` when the build script's dylib copy step didn't fire. Check that `OUT_DIR` is writable and that `AZUL_DLL_PATH` (or `target/release/`) actually contains a valid `libazul.{dylib,so,dll}`.

**Memtest failure on `assert_size_align_AzFoo`.** `api.json`'s field list for `Foo` no longer matches the Rust struct. Update one or the other, run `azul-doc normalize`, then `azul-doc codegen all`.

**`PyInit_azul` missing on macOS.** The `-undefined dynamic_lookup` link arg only fires under `target_os = "macos"` and `feature = "pyo3"`. The cdylib must be built with `cargo build --features python-extension`, not just `pyo3`.

**Linking against the debug build by accident.** The build script prints `Linking against libazul.dylib [local (debug)]` and a warning. Build the release dylib (`cargo build --release -p azul-dll --features build-dll`) before linking your downstream crate.

## Coming Up Next

- [Code Organization](code-organization.md) — Top-level crate map and where each piece lives
- [Web Backend Internals](web-backend.md) — WASM target - DOM-attachment and OffscreenCanvas
- [DOM Internals](dom.md) — How the public `Dom` type is built and stored
