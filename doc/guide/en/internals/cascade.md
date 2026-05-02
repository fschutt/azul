---
slug: cascade
title: Cascade, Inheritance, Restyle
language: en
canonical_slug: cascade
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - core/src/compact_cache_builder.rs
  - core/src/prop_cache.rs
  - core/src/styled_dom.rs
  - core/src/ua_css.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:43:38Z
---

> **WIP** — the cascade pipeline still has two parallel build paths (`build_compact_cache` and `build_compact_cache_with_inheritance`) pending consolidation. The shape described below reflects the inheritance variant, which is the production path called from `StyledDom::restyle`.

The cascade owns four data structures in a strict order: `CssPropertyCache` (the slow-path resolver), the per-node `css_props` from `NodeData`, the global `*` rules, and the `CompactLayoutCache` (the fast-path resolved values). Building the cache is one pre-order arena walk per restyle.

## File map

| File | Role |
|---|---|
| `core/src/prop_cache.rs:667` | `CssPropertyCache` — the cascade's main state |
| `core/src/compact_cache_builder.rs:35` | `build_compact_cache` — non-inheritance variant (legacy) |
| `core/src/compact_cache_builder.rs:433` | `build_compact_cache_with_inheritance` — production path |
| `core/src/compact_cache_builder.rs:443` | `build_compact_cache_with_inheritance_debug` — same with debug logging |
| `core/src/styled_dom.rs:1375` | `StyledDom::restyle` — orchestrates restyle + UA + inherit + compact build |
| `core/src/styled_dom.rs:1604` | `restyle_on_state_change` — incremental restyle for hover/focus/active |
| `core/src/ua_css.rs` | `apply_ua_css_to_compact` — per-NodeType default property table |

## The full restyle

`StyledDom::restyle(css)` (`core/src/styled_dom.rs:1375`) runs four phases in order:

```rust,ignore
pub fn restyle(&mut self, mut css: Css) {
    // 1. Match selectors → fill cascaded_props / css_props in CssPropertyCache
    self.css_property_cache.restyle(&mut css, ...);

    // 2. Apply UA-CSS defaults per node_type
    self.css_property_cache.apply_ua_css(node_data);

    // 3. Resolve em / rem / inherited values
    self.css_property_cache.compute_inherited_values(hierarchy, node_data);

    // 4. (Caller responsibility) build_compact_cache_with_inheritance
}
```

Step 4 is in a separate function because the layout pipeline calls it explicitly with `prev_font_hashes` from the previous frame to compute `font_dirty_nodes`.

## `CssPropertyCache`

```rust,ignore
pub struct CssPropertyCache {
    pub node_count: usize,

    pub user_overridden_properties: Vec<Vec<(CssPropertyType, CssProperty)>>,
    pub cascaded_props: FlatVecVec<StatefulCssProperty>,
    pub css_props: FlatVecVec<StatefulCssProperty>,
    pub computed_values: Vec<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    pub compact_cache: Option<azul_css::compact_cache::CompactLayoutCache>,
    pub global_css_props: Vec<CssProperty>,
    pub resolved_font_sizes_px: std::sync::OnceLock<Vec<f32>>,
}
```

(`core/src/prop_cache.rs:667`)

The cache layers properties from five priority sources, lowest to highest:

| Priority | Source | Storage |
|---|---|---|
| 1 (lowest) | UA CSS | `apply_ua_css_to_compact` writes directly to compact arrays |
| 2 | author `*` rules | `global_css_props: Vec<CssProperty>` |
| 3 | author specific selectors | `cascaded_props` (parents) + `css_props` (per-node from stylesheets) |
| 4 | inline `style="..."` / `NodeData::css_props` | `css_props` |
| 5 (highest) | runtime callback overrides | `user_overridden_properties` |

`StatefulCssProperty` carries a `CssProperty` plus the pseudo-state mask it applies in (Normal / Hover / Active / Focus / Dragging / DragOver). The cascade unifies all states into one entry per property — the getter picks the right value at lookup time.

`computed_values` holds the post-inheritance resolved values for *inheritable* properties. `font-size` resolves `em`/`%` here; the resolved px is then re-cached in `resolved_font_sizes_px` because `get_font_size` is called ~730× per node per layout pass and the recursive parent walk would dominate.

## Pre-order is load-bearing

The flat arena indexes nodes in pre-order: parent index < all child indices. Every cascade pass walks `0..node_count` forwards and trusts that any value it reads from a parent is already resolved. Inheritance, font-size resolution, and `compute_inherited_values` all rely on this.

If you ever reorder the arena or build it bottom-up, the cascade breaks silently — values from later siblings can leak into earlier ones.

## `build_compact_cache_with_inheritance`

The production cascade build is one loop over the arena (`core/src/compact_cache_builder.rs:443`). Per-node steps:

```text
for i in 0..node_count:
    Step 1: inherit from parent (only INHERITABLE_TIER1_MASK fields + font_size + text props)
    Step 2: apply_ua_css_to_compact(node_type)
    Step 2.5: apply global *-rule properties (skipped on text nodes)
    Step 3: cascade-walk this node's properties via CssPropertyCache getters
    Step 4: encode results into tier1_enums[i] / tier2_dims[i] / tier2_cold[i] / tier2b_text[i]
    Step 5: compare font_family_hash against prev_font_hashes; record dirty
```

Step 1 uses a static mask to copy *only* the inheritable tier-1 enum bits from the parent's `tier1_enums[parent_idx]`:

```rust,ignore
const INHERITABLE_TIER1_MASK: u64 =
    (FONT_WEIGHT_MASK   << FONT_WEIGHT_SHIFT)
  | (FONT_STYLE_MASK    << FONT_STYLE_SHIFT)
  | (TEXT_ALIGN_MASK    << TEXT_ALIGN_SHIFT)
  | (VISIBILITY_MASK    << VISIBILITY_SHIFT)
  | (WHITE_SPACE_MASK   << WHITE_SPACE_SHIFT)
  | (DIRECTION_MASK     << DIRECTION_SHIFT)
  | (BORDER_COLLAPSE_MASK << BORDER_COLLAPSE_SHIFT);
```

(`core/src/compact_cache_builder.rs:553`)

Non-inheritable enum fields (display, position, float, overflow, box-sizing, flex-*, clear, vertical-align, writing-mode) stay at 0 — the CSS initial value. They get filled in by UA CSS in Step 2 and author CSS in Step 3.

For tier 2, the inheritable fields are `font_size` (dims), `border_spacing_h/v` and `tab_size` (cold), and *all* of `tier2b_text` (text-color, font-family-hash, line-height, letter-spacing, word-spacing, text-indent).

## UA CSS application

`apply_ua_css_to_compact(node_type, &mut tier1, &mut dims, &mut cold, &mut text, &mut font_hash_to_families)` (`core/src/ua_css.rs`) hard-codes per-`NodeType` defaults in compact form. For example, `<h1>` writes `font_size = 2em` (resolved later via the parent chain) and `font_weight = Bold`; `<button>` writes `display = InlineBlock`, padding, and a border style.

Hard-coding the UA defaults in compact form skips a full cascade walk per node for the common case where author CSS doesn't override. The cost is that adding a new `NodeType` requires a matching arm in `apply_ua_css_to_compact`.

## Global `*` rules

A `*` rule applies to all *elements* — but per CSS spec, text nodes are not elements. The cascade enforces this:

```rust,ignore
if !nd.is_text_node() {
    for prop in self.global_css_props.iter() {
        apply_css_property_to_compact(prop, ...);
    }
}
```

(`core/src/compact_cache_builder.rs:614`)

Without that check, `* { color: red }` would overwrite the inherited `color` on every `Text` child of a `<p>` even though the `<p>` itself correctly cascaded from `p { color: blue }`.

`global_css_props` is hoisted out of `cascaded_props` during `restyle()` so it doesn't get cloned into every node's per-node prop list — that saved ~50K clones on real pages.

## The four-level `RelayoutScope`

Every `CssPropertyType` is classified by `relayout_scope(conservative: bool) -> RelayoutScope`:

```rust,ignore
pub enum RelayoutScope {
    None,         // repaint only (color, opacity, transform, filter, …)
    IfcOnly,      // re-shape the inline-formatting-context (text-content, font-size)
    SizingOnly,   // recompute this node's sizing (width, height, padding, …)
    Full,         // full subtree relayout (display, position, float, …)
}
```

(`css/src/props/property.rs:784`)

This is the cascade's contribution to incremental layout. Taffy uses a binary clean/dirty flag; the four-level classification lets the layout solver skip subtree walks when only an IFC's text needs reshaping, or skip a parent's reflow when only a child's color changed. `ChangeAccumulator::max_scope` (see [DOM Internals](dom.md)) propagates the worst case.

## `restyle_on_state_change`: hover, focus, active

Pseudo-state changes don't re-run `restyle()`. Instead, `restyle_on_state_change` (`core/src/styled_dom.rs:1604`) walks only the affected nodes and produces a `RestyleResult { changed_nodes: Vec<(NodeId, Vec<ChangedCssProperty>)> }` with the deltas. The result feeds back through `ChangeAccumulator::merge_restyle_result` so the rest of the pipeline doesn't care whether a change came from a DOM diff or a hover toggle.

The `changed_nodes` list always classifies via `relayout_scope(true)` — conservative — because the property-specific scope can depend on the new value and the safe choice is the higher relayout level.

## Font dirty tracking

The compact cache stores a `font_family_hash: u64` per node (`tier2b_text`). The builder takes the previous frame's hashes:

```rust,ignore
pub fn build_compact_cache_with_inheritance(
    &self,
    node_data: &[NodeData],
    node_hierarchy: &[NodeHierarchyItem],
    prev_font_hashes: &[u64],
) -> CompactLayoutCache;
```

After encoding, it compares each `tier2b_text[i].font_family_hash` against `prev_font_hashes[i]` and pushes mismatching indices into `font_dirty_nodes`. The text shaper consumes that list to re-resolve only the affected font chains, instead of the global all-or-nothing `font_stacks_hash` XOR.

On the very first build (`prev_font_hashes` is empty), every text node is marked dirty.

## Slow path: `get_property_slow`

The compact cache covers ~50 hot properties. Anything else (background, box-shadow, transform, filter, content, transitions, …) falls through to `CssPropertyCache::get_property_slow(node, node_id, prop_type, state)`. The slow path:

1. Checks `user_overridden_properties[i]` for a runtime override.
2. Walks `css_props[i]` for an inline / per-node author rule whose state-mask matches.
3. Walks `cascaded_props[i]` for an inherited cascaded match.
4. Falls back to `computed_values[i]` for the resolved inherited value.
5. Returns `None` (the property's `initial` value) if nothing matched.

Each step short-circuits on first match. The cost is ~5 walks per call vs O(1) array access for compact-cached properties — quoted at ~700× per node per layout pass before the compact cache existed.

## Two parallel build paths (legacy)

`build_compact_cache` (no inheritance, `core/src/compact_cache_builder.rs:35`) and `build_compact_cache_with_inheritance` (`:433`) duplicate ~400 lines of encoding logic. The non-inheritance variant uses `CssPropertyCache` getters that internally cascade — slower but doesn't require the parent-already-resolved invariant.

The non-inheritance variant is currently called from `core/src/styled_dom.rs:1353` (a fallback path); the production path is the inheritance variant called from `core/src/styled_dom.rs:960`. Consolidating them is on the cleanup list — until then, the inheritance variant is the source of truth, and changes must be mirrored in both files or the fallback path will silently diverge.

## Adding a property to the cascade

If your property *should* be in the compact cache (frequently set, layout-relevant):

1. Decide which tier — see [Compact Property Cache](compact-cache.md).
2. Add encode/decode helpers in `css/src/compact_cache.rs`.
3. Add a Step-3 cascade-walk arm in `compact_cache_builder.rs` that calls the `CssPropertyCache` getter and writes the encoded value.
4. If inheritable, add it to the `INHERITABLE_TIER1_MASK` and the Step-1 inheritance copy.
5. Add UA defaults in `core/src/ua_css.rs` if the property has any.
6. Update `RelayoutScope` for the property's `CssPropertyType`.

If your property stays on the slow path:

1. Implement `parse_*` (see [CSS Parser](css-parser.md)).
2. Add an arm in `CssPropertyCache::get_property_slow` if it needs special resolution; otherwise the generic walker handles it.
3. Update `RelayoutScope`.

## See also

- [DOM Internals](dom.md) — `NodeData::css_props` is one of the cascade's input sources.
- [CSS Parser](css-parser.md) — produces the `CssProperty` values the cascade routes.
- [Compact Property Cache](compact-cache.md) — the encoded output of `build_compact_cache_with_inheritance`.
