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
```

The repository layout mirrors that order. Read the crates top-to-bottom and you read the dependency graph in reverse.

## `css/` — parser and property types

Pure data definitions for the CSS engine. `no_std`-compatible, no platform code, no dependencies on other azul crates. The `corety` module re-exports the FFI-safe primitives (`AzString`, `AzVec<T>`, `OptionT`) that travel through the C API; everything else in the workspace consumes these.

Key modules (declared in `css/src/lib.rs`):

| module | purpose |
|---|---|
| [`css`](../../../../css/src/css.rs) | `Css`, `Stylesheet`, `CssRuleBlock`, `CssDeclaration` — the parsed AST |
| [`props`](../../../../css/src/props/) | typed `CssProperty` enum and per-property value types |
| [`parser2`](../../../../css/src/parser2.rs) | feature-gated CSS string parser |
| [`compact_cache`](../../../../css/src/compact_cache.rs) | three-tier numeric cache for resolved styles |
| [`format_rust_code`](../../../../css/src/format_rust_code.rs) | const-compatible Rust source emitter for compile-time CSS |
| [`system`](../../../../css/src/system.rs) | OS-native theme discovery (system colors, fonts, DPI) |
| [`shape`](../../../../css/src/shape.rs) | `shape-inside`, `shape-outside`, `clip-path` data |
| [`codegen`](../../../../css/src/codegen/mod.rs) | "compile a stylesheet to a standalone project" — separate from the FFI codegen pipeline |

The crate sets `#![cfg_attr(not(feature = "std"), no_std)]` and depends on `alloc` only, so the parser can run in restricted environments (e.g. embedded CSS in a WASM payload).

## `core/` — shared data types

Platform-independent definitions for the GUI loop: DOM construction, the CSSOM, callbacks, hit-testing, resources, OpenGL, and SVG. Foundational; depends only on `azul-css`.

The crate is large but well-partitioned (modules declared in `core/src/lib.rs:47-134`):

| module | what it owns |
|---|---|
| [`dom`](../../../../core/src/dom.rs) | `Dom`, `NodeData`, `NodeType`, the CSS-in-Rust API |
| [`styled_dom`](../../../../core/src/styled_dom.rs) | `StyledDom` — DOM after the CSS cascade |
| [`callbacks`](../../../../core/src/callbacks.rs) | `Callback`, `Update`, `LayoutCallback`, `RefAny`-using closures |
| [`refany`](../../../../core/src/refany.rs) | the type-erased ref-counted pointer |
| [`events`](../../../../core/src/events.rs) | `EventFilter`, `SyntheticEvent`, hover/focus/touch enums |
| [`window`](../../../../core/src/window.rs) | `WindowState`, `WindowCreateOptions`, platform options |
| [`resources`](../../../../core/src/resources.rs) | `ImageCache`, `RendererResources`, font/image refcounts |
| [`hit_test`](../../../../core/src/hit_test.rs), [`hit_test_tag`](../../../../core/src/hit_test_tag.rs) | typed hit-test results and the compositor tag wire format |
| [`prop_cache`](../../../../core/src/prop_cache.rs) | per-node CSS property cache |
| [`compact_cache_builder`](../../../../core/src/compact_cache_builder.rs) | converts `CssPropertyCache` into the numeric three-tier cache |
| [`style`](../../../../core/src/style.rs) | cascade, selector matching, specificity |
| [`gl`](../../../../core/src/gl.rs), [`glconst`](../../../../core/src/glconst.rs), [`gl_fxaa`](../../../../core/src/gl_fxaa.rs) | OpenGL context wrappers, GL constants, FXAA shader |
| [`gpu`](../../../../core/src/gpu.rs) | GPU value cache (transforms, opacity, scrollbar fades) |
| [`svg`](../../../../core/src/svg.rs), [`svg_path_parser`](../../../../core/src/svg_path_parser.rs) | SVG types and `d=""` parser |
| [`task`](../../../../core/src/task.rs) | `Timer`, `Thread`, `ThreadSendMsg`, async state machinery |
| [`animation`](../../../../core/src/animation.rs) | `AnimationData`, transition interpolation |
| [`a11y`](../../../../core/src/a11y.rs) | screen-reader types fed to AccessKit |
| [`menu`](../../../../core/src/menu.rs) | menu bar, context menu, menu item types |
| [`xml`](../../../../core/src/xml.rs) | XHTML parser (declarative UI) |
| [`json`](../../../../core/src/json.rs) | C-API JSON value types (no `serde` dep) |

`core` has the same `no_std` surface as `css`, but most consumers enable the `std` feature. Type aliases `OrderedMap<K, V>` (a `BTreeMap`) and `FastBTreeSet<T>` are defined at the crate root because `HashMap` is unavailable under `no_std`.

## `layout/` — solver, text, managers

The runtime layer. Owns the layout solver, text shaping, font management, hit-testing, and the per-frame state machines that the platform shells drive. Depends on `azul-core` and `azul-css`.

Five sub-systems do most of the work:

- **[`solver3`](../../../../layout/src/solver3/)** — block, inline, flex, grid, and table formatting contexts. Block/inline are azul's; flex/grid delegate to [Taffy](../../../../layout/src/solver3/taffy_bridge.rs). Entry point: `layout_document` in [`layout/src/solver3/mod.rs`](../../../../layout/src/solver3/mod.rs).
- **[`text3`](../../../../layout/src/text3/)** — third-generation text engine. Bidi via `unicode-bidi`, shaping via [allsorts](../../../../layout/src/font.rs), Knuth–Plass line breaking, hyphenation. Caches in `text3::cache::TextShapingCache` (re-exported as `TextLayoutCache` for back-compat).
- **[`managers/`](../../../../layout/src/managers/)** — stateful per-window components: `ScrollManager`, `FocusManager`, `SelectionManager`, `CursorManager`, `IFrameManager`, `GpuStateManager`, `GestureManager`. Each is a struct on `LayoutWindow`; see [`scripts/ARCHITECTURE.md`](../../../../scripts/ARCHITECTURE.md) §3 for the full table.
- **[`window`](../../../../layout/src/window.rs)** — `LayoutWindow` is the per-window aggregate; `layout_and_generate_display_list()` is the relayout entry point called by every platform shell each frame.
- **[`widgets/`](../../../../layout/src/widgets/)** — built-in widgets: button, text input, tabs, tree view, node graph. Optional via the `widgets` feature.

Smaller modules in `layout/src/lib.rs:46-235`:

| module | feature gate | purpose |
|---|---|---|
| [`font`](../../../../layout/src/font.rs) | `text_layout` | font parsing, metrics, subsetting (allsorts) |
| [`hit_test`](../../../../layout/src/hit_test.rs) | `text_layout` | maps screen coords to `DomNodeId` |
| [`fragmentation`](../../../../layout/src/fragmentation.rs) | always | CSS fragmentation engine for paged media |
| [`paged`](../../../../layout/src/paged.rs) | always | infinite-canvas paged layout |
| [`event_determination`](../../../../layout/src/event_determination.rs) | `text_layout` | maps raw input to DOM callbacks |
| [`callbacks`](../../../../layout/src/callbacks.rs) | `text_layout` | callback invocation + result processing |
| [`default_actions`](../../../../layout/src/default_actions.rs) | `text_layout` | copy/paste/select-all/undo defaults |
| [`cpurender`](../../../../layout/src/cpurender.rs) | `cpurender` | CPU-only software rendering |
| [`headless`](../../../../layout/src/headless.rs) | `text_layout` | headless backend (`AZUL_HEADLESS=1`) for E2E + screenshots |
| [`scroll_timer`](../../../../layout/src/scroll_timer.rs) | `text_layout` | momentum-scroll physics timer |
| [`thread`](../../../../layout/src/thread.rs), [`timer`](../../../../layout/src/timer.rs) | `text_layout` | C-API wrappers around `core::task` |
| [`xml`](../../../../layout/src/xml/) | `xml` | XML/XHTML → `StyledDom` |
| [`fluent`](../../../../layout/src/fluent.rs) | `fluent` | Project Fluent localization |
| [`icu`](../../../../layout/src/icu.rs) | `icu` / `icu_macos` / `icu_windows` | platform-specific ICU bindings |
| [`http`](../../../../layout/src/http.rs), [`url`](../../../../layout/src/url.rs) | `http` | HTTP client + URL parser |
| [`zip`](../../../../layout/src/zip.rs) | `zip_support` | ZIP I/O |
| [`json`](../../../../layout/src/json.rs) | `json` | C-API JSON |
| [`file`](../../../../layout/src/file.rs) | always | filesystem ops (C-compatible) |
| [`icon`](../../../../layout/src/icon.rs) | always | icon resolver (Material Icons font, image, ZIP packs) |
| [`probe`](../../../../layout/src/probe.rs) | `probe` (no-op when off) | `AZ_PROFILE` instrumentation |

Everything large is feature-gated so a minimal build (e.g. WASM target, headless CI) only compiles what it needs.

## `dll/` — FFI, platform shells, web, Python

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
```

The `desktop/` tree is only compiled when `cabi_internal` is on (i.e. `build-dll` or `link-static`). For `link-dynamic`, only the `extern "C"` declarations in `dll_api_external.rs` are pulled in.

`dll/src/lib.rs:46-101` enables one of three mutually exclusive global allocators (mimalloc, jemalloc, system) and exposes `az_purge_allocator()` so live apps can hint memory release after large transient allocations.

`dll/Cargo.toml` is the source of truth for which features compose which build modes — see [`build-and-codegen`](build-and-codegen.md) for the matrix.

## `doc/` — codegen, autodoc, reftests, deploy

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

| subcommand | code path | purpose |
|---|---|---|
| `print` | `print.rs` | dump api.json by module/class/function |
| `normalize` | `main.rs` + `patch/` | rewrite api.json to canonical form |
| `codegen all` | `codegen/v2/generator.rs` `generate_all_v2` | produce every binding (`target/codegen/*.rs`, `azul.h`, `azul*.hpp`) |
| `codegen <rust\|c\|cpp\|python>` | `codegen/v2/mod.rs` | one target |
| `reftest` | `reftest/` | run visual regression suite |
| `deploy` | `dllgen/deploy/` | build release artifacts + website |
| `autoreview autodoc` | `autofix/autodoc.rs` | parallel doc-generation agents (this pipeline) |
| `autoreview autodoc-screenshots` | same | render `azul-render` fences |
| `autoreview autodoc-check` | same | pre-deploy staleness gate |

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
