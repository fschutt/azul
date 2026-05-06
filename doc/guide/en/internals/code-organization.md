---
slug: code-organization
title: Code Organization
language: en
canonical_slug: code-organization
audience: contributor
maturity: mature
guide_order: null
topic_only: false
short_desc: Top-level crate map and where each piece lives
prerequisites: []
tracked_files:
  - core/src/lib.rs
  - css/src/lib.rs
  - layout/src/lib.rs
  - dll/src/lib.rs
  - scripts/ARCHITECTURE.md
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Code Organization

Azul is a Cargo workspace of five crates that compile bottom-up: `css` → `core` → `layout` → `dll`, with `doc` as a sibling tool crate. Each crate owns one layer of the stack and depends only on layers below it. The `Cargo.toml` workspace declaration enforces this:

```toml
# Cargo.toml
[workspace]
members = [
    "css",       # CSS parser + property types — no internal deps
    "core",      # DOM, callbacks, RefAny — depends on css
    "layout",    # solver3, text3, managers — depends on core + css
    "dll",       # platform shells, FFI, Python ext — depends on all
    "doc",       # codegen + reftest harness + autodoc
    "examples/rust",
    "examples/https-test",
]
```rust

The repository layout mirrors that order. Read the crates top-to-bottom and you read the dependency graph in reverse.

## css/ — parser and property types

Pure data definitions for the CSS engine. `no_std`-compatible, no platform code, no dependencies on other azul crates. The `corety` module re-exports the FFI-safe primitives (`AzString`, `AzVec<T>`, `OptionT`) that travel through the C API; everything else in the workspace consumes these.

Key modules declared in `css/src/lib.rs`:

- **css.** The parsed AST: `Css`, `Stylesheet`, `CssRuleBlock`, `CssDeclaration`.
  - `css/src/css.rs`
- **props.** Typed `CssProperty` enum and per-property value types.
  - `css/src/props/`
- **parser2.** Feature-gated CSS string parser.
  - `css/src/parser2.rs`
- **compact_cache.** Three-tier numeric cache for resolved styles.
  - `css/src/compact_cache.rs`
- **format_rust_code.** Const-compatible Rust source emitter for compile-time CSS.
  - `css/src/format_rust_code.rs`
- **system.** OS-native theme discovery for system colors, fonts, and DPI.
  - `css/src/system.rs`
- **shape.** Data for `shape-inside`, `shape-outside`, and `clip-path`.
  - `css/src/shape.rs`
- **codegen.** Compiles a stylesheet to a standalone project. Separate from the FFI codegen pipeline.
  - `css/src/codegen/mod.rs`

The crate sets `#![cfg_attr(not(feature = "std"), no_std)]` and depends on `alloc` only, so the parser can run in restricted environments (e.g. embedded CSS in a WASM payload).

## core/ — shared data types

Platform-independent definitions for the GUI loop: DOM construction, the CSSOM, callbacks, hit-testing, resources, OpenGL, and SVG. Foundational; depends only on `azul-css`.

The crate is large but well-partitioned. Modules are declared in `core/src/lib.rs`:

- **dom.** `Dom`, `NodeData`, `NodeType`, the CSS-in-Rust API.
  - `core/src/dom.rs`
- **styled_dom.** `StyledDom`, the DOM after the CSS cascade.
  - `core/src/styled_dom.rs`
- **callbacks.** `Callback`, `Update`, `LayoutCallback`, and `RefAny`-using closures.
  - `core/src/callbacks.rs`
- **refany.** The type-erased ref-counted pointer.
  - `core/src/refany.rs`
- **events.** `EventFilter`, `SyntheticEvent`, plus hover, focus, and touch enums.
  - `core/src/events.rs`
- **window.** `WindowState`, `WindowCreateOptions`, platform options.
  - `core/src/window.rs`
- **resources.** `ImageCache`, `RendererResources`, font and image refcounts.
  - `core/src/resources.rs`
- **hit_test.** Typed hit-test results and the compositor tag wire format.
  - `core/src/hit_test.rs`
  - `core/src/hit_test_tag.rs`
- **prop_cache.** Per-node CSS property cache.
  - `core/src/prop_cache.rs`
- **compact_cache_builder.** Converts `CssPropertyCache` into the numeric three-tier cache.
  - `core/src/compact_cache_builder.rs`
- **style.** Cascade, selector matching, specificity.
  - `core/src/style.rs`
- **gl.** OpenGL context wrappers, GL constants, FXAA shader.
  - `core/src/gl.rs`
  - `core/src/glconst.rs`
  - `core/src/gl_fxaa.rs`
- **gpu.** GPU value cache for transforms, opacity, and scrollbar fades.
  - `core/src/gpu.rs`
- **svg.** SVG types and `d=""` parser.
  - `core/src/svg.rs`
  - `core/src/svg_path_parser.rs`
- **task.** `Timer`, `Thread`, `ThreadSendMsg`, async state machinery.
  - `core/src/task.rs`
- **animation.** `AnimationData` and transition interpolation.
  - `core/src/animation.rs`
- **a11y.** Screen-reader types fed to AccessKit.
  - `core/src/a11y.rs`
- **menu.** Menu bar, context menu, and menu item types.
  - `core/src/menu.rs`
- **xml.** XHTML parser for declarative UI.
  - `core/src/xml.rs`
- **json.** C-API JSON value types with no `serde` dep.
  - `core/src/json.rs`

`core` has the same `no_std` surface as `css`, but most consumers enable the `std` feature. Type aliases `OrderedMap<K, V>` (a `BTreeMap`) and `FastBTreeSet<T>` are defined at the crate root because `HashMap` is unavailable under `no_std`.

## layout/ — solver, text, managers

The runtime layer. Owns the layout solver, text shaping, font management, hit-testing, and the per-frame state machines that the platform shells drive. Depends on `azul-core` and `azul-css`.

Five sub-systems do most of the work:

- **[`solver3`](../../../../layout/src/solver3/)** — block, inline, flex, grid, and table formatting contexts. Block/inline are azul's; flex/grid delegate to [Taffy](../../../../layout/src/solver3/taffy_bridge.rs). Entry point: `layout_document` in [`layout/src/solver3/mod.rs`](../../../../layout/src/solver3/mod.rs).
- **[`text3`](../../../../layout/src/text3/)** — third-generation text engine. Bidi via `unicode-bidi`, shaping via [allsorts](../../../../layout/src/font.rs), Knuth–Plass line breaking, hyphenation. Caches in `text3::cache::TextShapingCache` (re-exported as `TextLayoutCache` for back-compat).
- **[`managers/`](../../../../layout/src/managers/)** — stateful per-window components: `ScrollManager`, `FocusManager`, `SelectionManager`, `CursorManager`, `IFrameManager`, `GpuStateManager`, `GestureManager`. Each is a struct on `LayoutWindow`; see [`scripts/ARCHITECTURE.md`](../../../../scripts/ARCHITECTURE.md) §3 for the full table.
- **[`window`](../../../../layout/src/window.rs)** — `LayoutWindow` is the per-window aggregate; `layout_and_generate_display_list()` is the relayout entry point called by every platform shell each frame.
- **[`widgets/`](../../../../layout/src/widgets/)** — built-in widgets: button, text input, tabs, tree view, node graph. Optional via the `widgets` feature.

Smaller modules in `layout/src/lib.rs`:

- **font.** Font parsing, metrics, and subsetting via allsorts. Feature `text_layout`.
  - `layout/src/font.rs`
- **hit_test.** Maps screen coords to `DomNodeId`. Feature `text_layout`.
  - `layout/src/hit_test.rs`
- **fragmentation.** CSS fragmentation engine for paged media. Always compiled.
  - `layout/src/fragmentation.rs`
- **paged.** Infinite-canvas paged layout. Always compiled.
  - `layout/src/paged.rs`
- **event_determination.** Maps raw input to DOM callbacks. Feature `text_layout`.
  - `layout/src/event_determination.rs`
- **callbacks.** Callback invocation and result processing. Feature `text_layout`.
  - `layout/src/callbacks.rs`
- **default_actions.** Copy, paste, select-all, and undo defaults. Feature `text_layout`.
  - `layout/src/default_actions.rs`
- **cpurender.** CPU-only software rendering. Feature `cpurender`.
  - `layout/src/cpurender.rs`
- **headless.** Headless backend (`AZUL_HEADLESS=1`) for E2E and screenshots. Feature `text_layout`.
  - `layout/src/headless.rs`
- **scroll_timer.** Momentum-scroll physics timer. Feature `text_layout`.
  - `layout/src/scroll_timer.rs`
- **thread, timer.** C-API wrappers around `core::task`. Feature `text_layout`.
  - `layout/src/thread.rs`
  - `layout/src/timer.rs`
- **xml.** XML/XHTML to `StyledDom`. Feature `xml`.
  - `layout/src/xml/`
- **fluent.** Project Fluent localization. Feature `fluent`.
  - `layout/src/fluent.rs`
- **icu.** Platform-specific ICU bindings. Features `icu` / `icu_macos` / `icu_windows`.
  - `layout/src/icu.rs`
- **http, url.** HTTP client and URL parser. Feature `http`.
  - `layout/src/http.rs`
  - `layout/src/url.rs`
- **zip.** ZIP I/O. Feature `zip_support`.
  - `layout/src/zip.rs`
- **json.** C-API JSON. Feature `json`.
  - `layout/src/json.rs`
- **file.** Filesystem ops, C-compatible. Always compiled.
  - `layout/src/file.rs`
- **icon.** Icon resolver for Material Icons font, image, and ZIP packs. Always compiled.
  - `layout/src/icon.rs`
- **probe.** `AZ_PROFILE` instrumentation. Feature `probe`, no-op when off.
  - `layout/src/probe.rs`

Everything large is feature-gated so a minimal build (e.g. WASM target, headless CI) only compiles what it needs.

## dll/ — FFI, platform shells, web, Python

The library entry point. `dll/src/lib.rs` is mostly a feature-gated dispatch hub: it pulls in the codegen output (`include!(...dll_api_internal.rs)` / `dll_api_external.rs` / `reexports.rs` / `python_api.rs` / `memtest.rs`) and conditionally compiles the desktop and web modules.

```
dll/src/
├── lib.rs            ← FFI gate, allocator selection, codegen include!()s
├── desktop/          ← native shells (cabi_internal only)
│   ├── app.rs        ← App::new(), App::run()
│   ├── compositor2.rs ← display list → WebRender translation
│   ├── csd.rs        ← client-side decorations
│   ├── menu.rs       ← desktop menu bar / context menu rendering
│   ├── menu_renderer.rs
│   ├── gl_texture_cache.rs
│   ├── gl_texture_integration.rs
│   ├── shader_cache.rs
│   ├── wr_translate2.rs
│   ├── native_screenshot.rs
│   ├── shell2/       ← per-platform window + event-loop backends
│   │   ├── common/   ← shared event_v2 pipeline + debug server
│   │   ├── windows/  ← WndProc, DWM, IME via ImmGetContext
│   │   ├── macos/    ← NSApplication, NSTextInputClient
│   │   ├── linux/    ← X11 + Wayland with shared GL/DBus
│   │   ├── ios/
│   │   └── headless/
│   └── ...
└── web/              ← AZ_BACKEND=web HTML server (feature = "web")
```rust

The `desktop/` tree is only compiled when `cabi_internal` is on (i.e. `build-dll` or `link-static`). For `link-dynamic`, only the `extern "C"` declarations in `dll_api_external.rs` are pulled in.

`dll/src/lib.rs:46-101` enables one of three mutually exclusive global allocators (mimalloc, jemalloc, system) and exposes `az_purge_allocator()` so live apps can hint memory release after large transient allocations.

`dll/Cargo.toml` is the source of truth for which features compose which build modes — see [`build-and-codegen`](build-and-codegen.md) for the matrix.

## doc/ — codegen, autodoc, reftests, deploy

A multi-purpose tool crate. Run as a CLI (`cargo run --release -p azul-doc -- <subcommand>`).

```
doc/src/
├── api.rs        ← api.json schema + parser
├── codegen/      ← FFI codegen v2 (Rust/C/C++/Python emitters)
│   └── v2/
│       ├── ir.rs
│       ├── ir_builder.rs
│       ├── generator.rs
│       ├── lang_rust.rs / lang_c.rs / lang_cpp/ / lang_python.rs
│       ├── lang_reexports.rs
│       └── rust/{static,dynamic}_binding.rs
├── dllgen/       ← release builder (dllgen/build.rs, deploy/, license/)
├── docgen/       ← guide HTML renderer (comrak + azul-render expansion)
├── autofix/      ← autoreview + small-fixes + autodoc subcommands
├── reftest/      ← visual regression harness (HeadlessWindow + PNG diff)
├── patch/        ← api.json patches and dedup
├── print.rs      ← human-readable api.json dumper
└── main.rs       ← argv dispatch
```

Subcommands worth knowing:

- **`print`.** Dumps api.json by module, class, or function.
  - `doc/src/print.rs`
- **`normalize`.** Rewrites api.json to canonical form.
  - `doc/src/main.rs`
  - `doc/src/patch/`
- **`codegen all`.** Produces every binding (`target/codegen/*.rs`, `azul.h`, `azul*.hpp`).
  - `doc/src/codegen/v2/generator.rs::generate_all_v2`
- **`codegen <rust|c|cpp|python>`.** Produces one target.
  - `doc/src/codegen/v2/mod.rs`
- **`reftest`.** Runs the visual regression suite.
  - `doc/src/reftest/`
- **`deploy`.** Builds release artifacts and the website.
  - `doc/src/dllgen/deploy/`
- **`autoreview autodoc`.** Parallel doc-generation agents. This pipeline.
  - `doc/src/autofix/autodoc.rs`
- **`autoreview autodoc-screenshots`.** Renders `azul-render` fences.
  - `doc/src/autofix/autodoc.rs`
- **`autoreview autodoc-check`.** Pre-deploy staleness gate.
  - `doc/src/autofix/autodoc.rs`

`scripts/ARCHITECTURE.md` is the authoritative high-level map; cite it before adding cross-crate refactors.

## Adding a new module to an existing crate

1. Create `<crate>/src/<module>.rs`.
2. Declare it in the crate root with the right feature gate, e.g. `#[cfg(feature = "text_layout")] pub mod managers;` in `layout/src/lib.rs:54-55`.
3. Add a one-line `///` doc comment above the `pub mod` line — the comment becomes the module summary in rustdoc.
4. If the module exports types that should appear in the public API surface, add them to `api.json` (see [`build-and-codegen`](build-and-codegen.md)) and re-run `cargo run --release -p azul-doc -- codegen all`.

## Adding a new crate

The default answer is: don't. The five-crate split is deliberate, and adding a sixth crate adds coupling, build-time, and a new dependency edge to maintain. If you have a new external dep and want to avoid pulling it into `core`/`layout`, gate it behind a feature flag instead.

If a new crate is justified (e.g. an optional decoder that has a heavy build dep nobody else needs), the steps are:

1. Add the directory and a `Cargo.toml` with `version = "0.0.7"` and the workspace's MPL-2.0 license.
2. Add it to `Cargo.toml`'s `[workspace] members`.
3. Wire it as an optional dependency in the consuming crate; do not add it to `core` unconditionally.

## Where to put platform code

Platform-specific code lives in `dll/src/desktop/shell2/<platform>/`. Don't add `cfg(target_os = "...")` blocks in `core` or `layout` — those crates must compile for every target including WASM. The shell2 layer is the integration boundary.

The exception is platform-specific *type definitions* in `core::window` (`WindowsWindowOptions`, `MacOsWindowOptions`, etc.) — these are POD and need to round-trip through the C ABI regardless of host platform, so they live in `core` but are only consumed by `dll/src/desktop/shell2/<platform>/`.

## Coming Up Next

- [FFI Codegen](build-and-codegen.md) — How `cargo build` cascades and the codegen pass
- [DOM Internals](dom.md) — How the public `Dom` type is built and stored
- [Layout Solver (Flex/Grid)](layout-solver.md) — Architecture of `solver3/` and how the engines share state
- [Rendering Pipeline](rendering-pipeline.md) — From `StyledDom` to pixels
