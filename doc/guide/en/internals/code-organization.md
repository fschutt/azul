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

## Overview

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
```

The repository layout mirrors that order. Read the crates top-to-bottom and you read the dependency graph in reverse. This page is the contributor entry point: each section below covers one crate and points at the dedicated subtree that explains its internals in depth.

## css — parser and property types

The `css` crate holds pure data definitions for the CSS engine. It is `no_std`-compatible, has no platform code, and depends on no other azul crates. The `corety` module re-exports the FFI-safe primitives — `AzString`, `AzVec<T>`, `OptionT` — that travel through the C API; everything else in the workspace consumes those types from here.

The parsed AST lives in the `css` module: `Css`, `Stylesheet`, `CssRuleBlock`, and `CssDeclaration`. The typed `CssProperty` enum and per-property value types live in `props`. The string parser sits behind a feature gate in `parser2`, while `compact_cache` provides the three-tier numeric cache for resolved styles. `format_rust_code` is the const-compatible Rust source emitter used for compile-time CSS, `system` performs OS-native theme discovery for system colors, fonts, and DPI, and `shape` carries the data for `shape-inside`, `shape-outside`, and `clip-path`. A `codegen` module compiles a stylesheet to a standalone project; this is separate from the FFI codegen pipeline in `doc`.

The crate sets `#![cfg_attr(not(feature = "std"), no_std)]` and depends on `alloc` only, so the parser can run in restricted environments such as embedded CSS in a WASM payload. The matching contributor docs live in [styling/](styling/cascade.md), and the parser specifically in [styling/css-parser.md](styling/css-parser.md).

## core — shared data types

`core` is the platform-independent backbone of the GUI loop: DOM construction, the CSSOM, callbacks, hit-testing, resources, OpenGL, and SVG. It is foundational and depends only on `azul-css`.

The crate is large but well-partitioned. `dom` defines `Dom`, `NodeData`, `NodeType`, and the CSS-in-Rust API; `styled_dom` wraps the DOM after the CSS cascade. Callback machinery — `Callback`, `Update`, `LayoutCallback`, and `RefAny`-using closures — lives in `callbacks`, and the type-erased ref-counted pointer itself in `refany`. Event types (`EventFilter`, `SyntheticEvent`, plus hover, focus, and touch enums) live in `events`, and window state types (`WindowState`, `WindowCreateOptions`, platform options) in `window`. Resources — the `ImageCache`, `RendererResources`, font and image refcounts — sit in `resources`. Hit-testing splits between `hit_test` (typed results) and `hit_test_tag` (the compositor wire format).

The styling pipeline has three core modules: `prop_cache` for the per-node CSS property cache, `compact_cache_builder` for converting that cache into the numeric three-tier representation, and `style` for the cascade itself, selector matching, and specificity. GPU support is in `gl` (OpenGL context wrappers, the GL constants table in `glconst`, and the FXAA shader in `gl_fxaa`) and `gpu` (the GPU value cache for transforms, opacity, and scrollbar fades). SVG support comes from `svg` and the `d=""` path parser in `svg_path_parser`. Async machinery lives in `task` (`Timer`, `Thread`, `ThreadSendMsg`); see [Async, Timers, Threading](async.md). Animations are in `animation`, accessibility types fed to AccessKit are in `a11y`, menus are in `menu`, the XHTML parser for declarative UI is in `xml`, and a no-`serde` C-API JSON value type is in `json`.

`core` exposes the same `no_std` surface as `css`, but most consumers enable the `std` feature. The type aliases `OrderedMap<K, V>` (a `BTreeMap`) and `FastBTreeSet<T>` are defined at the crate root because `HashMap` is unavailable under `no_std`. Contributor deep-dives split across [DOM Internals](dom.md), [Styling](styling.md), [Events](events.md), and [Async, Timers, Threading](async.md).

## layout — solver, text, managers

`layout` is the runtime layer. It owns the layout solver, text shaping, font management, hit-testing, and the per-frame state machines that the platform shells drive. It depends on `azul-core` and `azul-css`.

Five sub-systems do most of the work. `solver3` implements block, inline, flex, grid, and table formatting contexts; block and inline are azul's, while flex and grid delegate to Taffy. The entry point `layout_document` produces a layout tree from a `StyledDom`. `text3` is the third-generation text engine, handling bidi via `unicode-bidi`, shaping via allsorts, Knuth–Plass line breaking, and hyphenation; its caches live in `text3::cache::TextShapingCache` (re-exported as `TextLayoutCache` for back-compat). The `managers` directory holds the stateful per-window components — `ScrollManager`, `FocusManager`, `SelectionManager`, `CursorManager`, `IFrameManager`, `GpuStateManager`, `GestureManager` — each a struct on `LayoutWindow`. The `window` module is where `LayoutWindow` itself lives, plus `layout_and_generate_display_list()`, the relayout entry point called by every platform shell each frame. Optional built-in widgets — button, text input, tabs, tree view, node graph — live in `widgets`, gated by the `widgets` feature.

Smaller modules cover specific concerns. Font parsing, metrics, and subsetting via allsorts are in `font` (feature `text_layout`). Mapping screen coordinates to a `DomNodeId` happens in `hit_test` (feature `text_layout`). The CSS fragmentation engine for paged media (`fragmentation`) and infinite-canvas paged layout (`paged`) are always compiled. Raw input is mapped to DOM callbacks in `event_determination`, callback invocation and result processing in `callbacks`, and copy/paste/select-all/undo defaults in `default_actions`. CPU-only software rendering is in `cpurender` (feature `cpurender`). The headless backend used for end-to-end testing and screenshots lives in `headless`, gated on `text_layout`. Momentum-scroll physics live in `scroll_timer`; C-API wrappers around `core::task` are in `thread` and `timer`. XML/XHTML to `StyledDom` conversion is in `xml` (feature `xml`). Project Fluent localization is in `fluent`. Platform-specific ICU bindings sit in `icu` (features `icu`, `icu_macos`, `icu_windows`). HTTP client and URL parser are in `http` and `url`, ZIP I/O is in `zip`, and a C-API JSON variant is in `json`. Filesystem ops sit in `file`, the icon resolver for Material Icons font / image / ZIP packs in `icon`, and `AZ_PROFILE` instrumentation in `probe` (no-op when the feature is off).

Everything large is feature-gated so a minimal build (such as a WASM target or headless CI) only compiles what it needs. Deep dives into the layout side are at [Layout](layout.md), [Rendering](rendering.md), [Events](events.md), and [Async, Timers, Threading](async.md).

## dll — FFI, platform shells, web, Python

`dll` is the library entry point. Its `lib.rs` is mostly a feature-gated dispatch hub: it pulls in the codegen output (`dll_api_internal.rs`, `dll_api_external.rs`, `reexports.rs`, `python_api.rs`, `memtest.rs`) via `include!()` and conditionally compiles the desktop and web modules.

```text
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
```

The `desktop/` tree is only compiled when `cabi_internal` is on (i.e. `build-dll` or `link-static`). For `link-dynamic`, only the `extern "C"` declarations in `dll_api_external.rs` are pulled in. The crate root also enables one of three mutually exclusive global allocators (mimalloc, jemalloc, system) and exposes `az_purge_allocator()` so live apps can hint memory release after large transient allocations.

`dll/Cargo.toml` is the source of truth for which features compose which build modes — see [build-and-codegen](build-and-codegen.md) for the matrix. Platform-shell internals live under [Windowing](windowing.md), the rendering bridge under [Rendering](rendering.md), and the HTTP server under [Web](web.md).

## doc — codegen, autodoc, reftests, deploy

`doc` is a multi-purpose tool crate. Run it as a CLI: `cargo run --release -p azul-doc -- <subcommand>`.

```text
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
├── dllgen/       ← release builder (build orchestration, deploy, license)
├── docgen/       ← guide HTML renderer (comrak + azul-render expansion)
├── autofix/      ← autoreview + small-fixes + autodoc subcommands
├── reftest/      ← visual regression harness (HeadlessWindow + PNG diff)
├── patch/        ← api.json patches and dedup
├── print.rs      ← human-readable api.json dumper
└── main.rs       ← argv dispatch
```

The subcommands worth knowing are `print` (dumps `api.json` by module, class, or function), `normalize` (rewrites `api.json` to canonical form), `codegen all` (produces every binding into `target/codegen/`), `codegen <rust|c|cpp|python>` (one target at a time), `reftest` (runs the visual regression suite), `deploy` (builds release artifacts and the website), and the three autoreview pipelines: `autoreview autodoc` for parallel doc-generation agents, `autoreview autodoc-screenshots` for rendering `azul-render` fences, and `autoreview autodoc-check` as the pre-deploy staleness gate.

`scripts/ARCHITECTURE.md` is the authoritative high-level map; cite it before adding cross-crate refactors.

## Adding a new module to an existing crate

1. Create `<crate>/src/<module>.rs`.
2. Declare it in the crate root with the right feature gate, e.g. `#[cfg(feature = "text_layout")] pub mod managers;` in `layout/src/lib.rs`.
3. Add a one-line `///` doc comment above the `pub mod` line — the comment becomes the module summary in rustdoc.
4. If the module exports types that should appear in the public API surface, add them to `api.json` (see [build-and-codegen](build-and-codegen.md)) and re-run `cargo run --release -p azul-doc -- codegen all`.

## Adding a new crate

The default answer is: don't. The five-crate split is deliberate, and adding a sixth crate adds coupling, build-time, and a new dependency edge to maintain. If you have a new external dep and want to avoid pulling it into `core` or `layout`, gate it behind a feature flag instead.

If a new crate is justified — for example, an optional decoder with a heavy build dep that nobody else needs — the steps are:

1. Add the directory and a `Cargo.toml` with `version = "0.0.7"` and the workspace's MPL-2.0 license.
2. Add it to `Cargo.toml`'s `[workspace] members`.
3. Wire it as an optional dependency in the consuming crate; do not add it to `core` unconditionally.

## Where to put platform code

Platform-specific code lives in `dll/src/desktop/shell2/<platform>/`. Don't add `cfg(target_os = "...")` blocks in `core` or `layout` — those crates must compile for every target including WASM. The shell2 layer is the integration boundary.

The exception is platform-specific *type definitions* in `core::window` (`WindowsWindowOptions`, `MacOsWindowOptions`, etc.) — these are POD and need to round-trip through the C ABI regardless of host platform, so they live in `core` but are only consumed by `dll/src/desktop/shell2/<platform>/`.

## Coming Up Next

- [FFI Codegen](build-and-codegen.md) — How `cargo build` cascades and the codegen pass
- [DOM Internals](dom.md) — How the public `Dom` type is built and stored
- [Layout](layout.md) — Architecture of `solver3/` and how the engines share state
- [Rendering](rendering.md) — From `StyledDom` to pixels
