---
slug: internals/styling
title: Styling Subsystem
language: en
canonical_slug: internals/styling
audience: contributor
maturity: mature
guide_order: null
topic_only: false
short_desc: How parsed CSS becomes per-node resolved layout values
prerequisites: []
tracked_files:
  - css/src/parser2.rs
  - css/src/props/property.rs
  - css/src/compact_cache.rs
  - css/src/system.rs
  - core/src/prop_cache.rs
  - core/src/compact_cache_builder.rs
  - core/src/styled_dom.rs
  - core/src/ua_css.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - Css
  - CssProperty
  - CssPropertyType
  - CssPropertyValue
  - StyledDom
  - SystemStyle
  - NodeData
---

# Styling Subsystem

## Overview

Azul's styling subsystem turns CSS source — author stylesheets, inline `style="..."` strings, runtime `with_css(...)` overrides, the user-agent default sheet, and OS-derived system colours — into per-node resolved values that the layout solver and renderer can read in O(1). The pipeline runs end to end in three stages: a hand-written parser produces typed `CssProperty` values from `&str`, a cascade pass resolves those values per node into a `CssPropertyCache`, and a final encoder packs the layout-hot subset into a four-array `CompactLayoutCache` keyed by node index.

Each stage has its own dedicated page. The parser is documented at [CSS Parser](styling/css-parser.md). The cascade — selector matching, specificity, inheritance, the four-level `RelayoutScope` classification — is at [Cascade, Inheritance, Restyle](styling/cascade.md). The fast-path encoding for the ~50 layout-critical properties is at [Compact Property Cache](styling/compact-cache.md). OS theme discovery, which feeds the user-agent default colour, font, and metric values into every cascade, is at [System Style Discovery](styling/system-style.md).

This page is the orientation. It describes how the four pieces interact, what flows through each stage, and what to read first depending on the task.

## The five priority sources

Every CSS property the renderer asks for is the resolution of five priority sources, lowest to highest:

1. **User-agent CSS.** Per-`NodeType` defaults (`<h1>` is `font-size: 2em font-weight: bold`, `<button>` is `display: inline-block` with padding and a border). Hard-coded into `apply_ua_css_to_compact` so the common case bypasses the cascade walk entirely.
2. **Author `*` rules.** Stylesheet rules with the universal selector. Hoisted into a single `global_css_props: Vec<CssProperty>` so they aren't cloned into every node's per-node prop list.
3. **Author specific selectors.** Stylesheet rules whose selector matched the node. Stored in `cascaded_props` (parents) and `css_props` (per-node).
4. **Inline `style="..."` and `NodeData::style`.** Each node carries an inline `Css` whose rules are tagged `rule_priority::INLINE`; the cascade walks them via `Css::iter_inline_properties()`.
5. **Runtime callback overrides.** Properties set by callbacks via `CallbackInfo::set_css_property(...)`. Stored in `user_overridden_properties`. The highest priority, intended for stateful UI (focus rings, drag highlights, animation overrides).

The cascade reads in priority order top to bottom, short-circuiting at the first match. Per-node, the resolution is one O(1) array lookup if the property is in the compact cache, or a walk through the five sources for slow-path properties.

## Stage 1: Parser

The parser turns CSS source into a typed AST. The entry point is `parser2::new_from_str(css_string) -> (Css, Vec<CssParseWarnMsg>)`, which is non-fatal: a syntax error becomes a warning and the rest of the stylesheet survives. Internally, it tokenises via `azul_simplecss::Tokenizer`, handles `@media` / `@lang` / `@theme` / `@supports` blocks, and routes each `(key, value)` declaration through `parse_css_property` to a per-property parser in `css/src/props/layout/` or `css/src/props/style/`.

The output `Css` value is a flat `Vec<CssRuleBlock>` — each pairing a `CssPath` selector with a `Vec<CssDeclaration>` plus an optional list of `@-rule` conditions and a `priority: u8` layer label (UA / SYSTEM / AUTHOR / INLINE / RUNTIME — see `rule_priority`). Selectors are separately parseable via `parse_css_path` for runtime `StyledDom::with_css(...)` overrides and for the codegen pipeline that compiles HTML+CSS to `const` Rust.

The 180-variant `CssProperty` enum is the type-safe currency that flows out of the parser into every other stage. Property modules are split by their effect on the layout pipeline (`props/layout/` for box-geometry, `props/style/` for paint-only, `props/basic/` for primitive value types). The split is what enables the `RelayoutScope` classification on `CssPropertyType`.

For details, see [CSS Parser](styling/css-parser.md).

## Stage 2: Cascade

The cascade owns the `CssPropertyCache`. Its job is to take the parsed `Css`, the per-node inline `style` (a `Css`) from `NodeData`, the global `*` rules, and the UA defaults — and produce, per node, a fully resolved property set ready for the compact-cache encoder.

`StyledDom::restyle(css)` is the orchestration entry point. It runs four phases in order:

1. Match selectors and fill `cascaded_props` / `css_props`.
2. Apply UA-CSS defaults per `node_type`.
3. Resolve `em` / `rem` / inherited values.
4. (Caller responsibility) build the compact cache, passing the previous frame's font hashes so dirty-tracking can fire.

The flat arena's pre-order index — parent index < child index — is load-bearing. Every cascade pass walks `0..node_count` forwards and trusts that any value read from a parent is already resolved. Inheritance, font-size resolution, and the inheritable-tier-1 mask copy all rely on this.

Pseudo-state changes (hover, focus, active) don't re-run a full restyle. `restyle_on_state_change` walks only the affected nodes and produces a `RestyleResult` of deltas, which feeds back through `ChangeAccumulator::merge_restyle_result` so the rest of the pipeline doesn't care whether a change came from a DOM diff or a hover toggle.

Every `CssPropertyType` is classified by `relayout_scope(conservative: bool) -> RelayoutScope`:

```rust,ignore
pub enum RelayoutScope {
    None,         // repaint only (color, opacity, transform, filter, …)
    IfcOnly,      // re-shape the inline-formatting-context (text-content, font-size)
    SizingOnly,   // recompute this node's sizing (width, height, padding, …)
    Full,         // full subtree relayout (display, position, float, …)
}
```

Taffy uses a binary clean / dirty flag; the four-level classification lets the layout solver skip subtree walks when only an IFC's text needs reshaping or skip a parent's reflow when only a child's color changed.

For the cascade walk, the inheritable-property mask, the slow-path resolver, and the legacy two-build-paths consolidation, see [Cascade, Inheritance, Restyle](styling/cascade.md).

## Stage 3: Compact cache

The compact cache is a four-array, fixed-layout encoding of the ~50 layout-hot CSS properties. Layout reads them by node index in O(1) — no `BTreeMap` lookups, no cascade walks. Built once per restyle by `build_compact_cache_with_inheritance`; read on every layout pass.

The four arrays are:

- `tier1_enums: Vec<u64>` — 21 enum-valued layout properties packed into a single `u64` per node (display, position, float, overflow, flex-*, justify-*, align-*, white-space, direction, ...). 8 B per node.
- `tier2_dims: Vec<CompactNodeProps>` — width, height, min/max-*, padding, margin, border-width, top/right/bottom/left, flex-grow/shrink, gap. 68 B per node.
- `tier2_cold: Vec<CompactNodePropsCold>` — paint-only and rare-but-typed: border colors, border radii, z-index, border-styles, grid placement, opacity, plus two `u8` "has-X" flag bytes for fast-skip negative paths. 28 B per node.
- `tier2b_text: Vec<CompactTextProps>` — text-color, font-family-hash, line-height, letter-spacing, word-spacing, text-indent. The whole struct is inheritable as a unit. 24 B per node.

Total: 128 B per node, 128 KB for a 1000-node DOM.

Properties that don't fit (background, box-shadow, transform, filter, content, transitions, ...) live on the slow `CssPropertyCache::get_property_slow` path and are not duplicated here. The `HOT_FLAG_HAS_*` and `DOM_HAS_*` bits are the negative fast paths: when the bit is clear, the renderer can skip the slow walk for that property entirely.

Sentinel encodings let the same `u32` field carry typed data and the `auto` / `none` / `inherit` / `initial` keywords. The `FloatValue` ×1000 representation is what makes the encoders work in `const` context — encoding/decoding uses no floats.

For the bit layouts, the encoder's per-node steps, the sentinel tables, the font-dirty-tracking mechanism, and the procedure for adding a property to a tier, see [Compact Property Cache](styling/compact-cache.md).

## System style: where UA defaults come from

User-agent defaults aren't a single hard-coded sheet. They're a `SystemStyle` value populated at app start from the operating system: theme (light / dark), accent colour, semantic UI colours, system fonts, scrollbar look, double-click time, reduced-motion preference, and more. Every callback's `CallbackInfo` exposes `get_system_style() -> Arc<SystemStyle>` so widgets and CSD code can consult it live.

Discovery is per-platform and lives in `dll/src/desktop/shell2/<platform>/system_style.rs`. macOS uses dlopen + Objective-C, Windows uses LoadLibrary + GetProcAddress for `user32.dll` and `dwmapi.dll`, Linux first tries the XDG Desktop Portal over raw D-Bus and then falls back to per-DE CLI helpers (`gsettings`, `kreadconfig5`, Hyprland config, pywal cache). Every backend starts by cloning a hard-coded default and mutates fields based on what the OS actually returned, so a query failure for a single value leaves the rest of the style intact.

Beyond UA defaults, `SystemStyle` carries two CSS-emitting methods (`create_csd_stylesheet`, `create_menu_stylesheet`) that build per-app stylesheets at runtime, and an `app_specific_stylesheet` slot for the user's `~/.config/azul/styles/<exe>.css` ricing file.

For the compile-time defaults table, the discovery order, the per-platform priority chains, and the cross-cutting checklist when adding a field to `SystemStyle`, see [System Style Discovery](styling/system-style.md).

## End-to-end flow

```text
        user CSS              NodeData.css_props        SystemStyle           runtime overrides
            |                        |                       |                       |
            v                        v                       v                       v
+-----------+                +-------+-----------------------+--------+      +-------+
|  parser2  |  Css   ----->  |          CssPropertyCache              |  +-> | comp  |
|           |  ----->        |   (cascaded + global * + per-node)     |  |   | cache |
+-----------+                +----------+-----------------------------+  |   | build |
                                        |                                |   +---+---+
                                        v                                |       |
                       compute_inherited_values + apply_ua_css_to_compact|       v
                                        |                                |   +---------+
                                        v                                |   | tier1   |
                            build_compact_cache_with_inheritance --------+   | tier2   |
                                                                             | tier2_c |
                                                                             | tier2b  |
                                                                             +----+----+
                                                                                  |
                                                                                  v
                                                                     layout / paint reads
```

A frame's restyle is one pre-order arena walk. A pseudo-state toggle is `restyle_on_state_change`, which only touches the affected nodes. A new font-family hash on a text node fires only when `tier2b_text[i].font_family_hash` differs from `prev_font_hashes[i]`, so font resolution is incremental.

## Where to start

- Adding a CSS property: read [CSS Parser](styling/css-parser.md) first to define the typed value and parser, then [Cascade](styling/cascade.md) to wire it through `parse_css_property` and `RelayoutScope`, then [Compact Property Cache](styling/compact-cache.md) if it should be on the fast path.
- Debugging a "why does my style not apply" issue: read [Cascade](styling/cascade.md) to understand the priority order and the slow-path walk in `get_property_slow`.
- Adding a system colour or scrollbar metric: read [System Style Discovery](styling/system-style.md), then update the per-platform `discover()` functions and the `defaults::*` constructors.
- Reading the encoded value at runtime: the getters on `CompactLayoutCache` are documented in [Compact Property Cache](styling/compact-cache.md).

## See also

- [DOM Internals](dom.md) — `NodeData::css_props` is one of the cascade's input sources.
- [Layout Solver](layout.md) — `RelayoutScope` classifies what work the layout solver actually does.
- [Rendering Pipeline](rendering.md) — `tier2_cold` flags determine which slow-path renderer code paths run.

## Coming Up Next

- [CSS Parser](styling/css-parser.md) — The hand-written CSS parser
- [Cascade, Inheritance, Restyle](styling/cascade.md) — Selector matching, specificity, computed values
- [Compact Property Cache](styling/compact-cache.md) — Layout-hot encoding for the solver
- [System Style Discovery](styling/system-style.md) — OS theme, accent, fonts, a11y settings
