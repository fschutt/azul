# CSS — Collapse `Css` / `Stylesheet` Double Wrapper

**Date:** 2026-05-08
**Status:** Draft — post-exploration, pre-implementation
**Scope:** Remove the unused `Stylesheet` indirection inside `Css`,
recover the layering capability via an explicit `priority` field on
`CssRuleBlock`, regenerate FFI codegen.

---

## Table of Contents

1. [Current shape and why it's a problem](#1-current-shape-and-why-its-a-problem)
2. [What the wrapper was supposed to give us](#2-what-the-wrapper-was-supposed-to-give-us)
3. [What it actually gives us today](#3-what-it-actually-gives-us-today)
4. [Proposed shape](#4-proposed-shape)
5. [Layer recovery: the priority field](#5-layer-recovery-the-priority-field)
6. [Migration scope](#6-migration-scope)
7. [ABI / FFI considerations](#7-abi--ffi-considerations)
8. [Risk register](#8-risk-register)
9. [Step-by-step implementation order](#9-step-by-step-implementation-order)

---

## 1. Current shape and why it's a problem

```rust
// css/src/css.rs
pub struct Css        { pub stylesheets: StylesheetVec }      //  Vec<Stylesheet>
pub struct Stylesheet { pub rules: CssRuleBlockVec }          //  Vec<CssRuleBlock>
pub struct CssRuleBlock {
    pub path: CssPath,
    pub declarations: CssDeclarationVec,
    pub conditions: DynamicSelectorVec,
}
```

A parsed user stylesheet is `Css → Vec<Stylesheet> → Vec<CssRuleBlock>`.
Three nested vectors to express what is, in practice, a flat list of
rules with selectors and declarations.

The cost is small but real:

- One extra type for everyone to model in their head.
- Iteration through `Css.rules()` is a doubly-nested loop
  (`RuleIterator` at `css/src/css.rs:1652`).
- Every callsite that wants "the rules" has to flatten:
  `css.stylesheets.iter().flat_map(|s| s.rules.iter())`.
- Codegen / FFI surfaces both `AzCss` and `AzStylesheet`, doubling the
  type wall in C/C++/Python bindings.

## 2. What the wrapper was supposed to give us

The doc-comment on `Css.stylesheets`
(`css/src/css.rs:25-30`) says:

> One CSS stylesheet can hold more than one sub-stylesheet:
> For example, when overriding native styles, the
> `.sort_by_specificity()` function should not mix the two stylesheets
> during sorting.

That is a real, sound use case. CSS itself eventually grew the
`@layer` feature precisely because plain specificity sort is too crude
when you need a UA layer beneath author rules. The Stylesheet boundary
was meant to act as an implicit layer separator: each Stylesheet sorts
internally by selector specificity; layers compose by their order in
the outer Vec.

## 3. What it actually gives us today

In practice nothing currently uses the layering capability.

| Site | What it does | Why the wrapper isn't load-bearing |
|------|--------------|-------------------------------------|
| `css/src/parser2.rs:673` | Returns `Css { stylesheets: vec![one_stylesheet] }` | Parser always produces exactly one Stylesheet. |
| `dll/src/desktop/shell2/{linux,macos,windows}/system_style.rs` | `css.stylesheets[0]` | Caller assumes there is exactly one. |
| `core/src/styled_dom.rs:961-976`, `core/src/xml.rs:5171` | `combined_stylesheets.extend(...)` from N sources | Merge code flattens nested stylesheets back into one big Vec, **discarding any layer boundary that might have existed**. |
| `core/src/prop_cache.rs:1026`, `core/src/xml.rs:4381`, `tests/src/css.rs:97` | `css.sort_by_specificity()` | Each call's input has exactly one stylesheet, so the per-stylesheet sort is identical to a flat sort. |
| `core/src/compact_cache_builder.rs` (UA → author → inline → runtime) | Cascade priority | Implemented at the **prop_cache layer**, not via Stylesheet boundaries. UA CSS uses `apply_ua_css_to_compact`, author `*` rules sit in `global_css_props`, inline lives in `NodeData::css_props`, runtime overrides in `user_overridden_properties`. The `Css` value passed in carries one Stylesheet. |

The layering claim is unrealised. The merge code at
`styled_dom.rs:961-976` actively *erases* any boundary by flattening,
so even if a producer wanted to use multiple Stylesheets to express
layering, the merge would discard it.

## 4. Proposed shape

```rust
// css/src/css.rs
pub struct Css {
    pub rules: CssRuleBlockVec,
}

pub struct CssRuleBlock {
    pub priority: u8,                       // NEW — see §5
    pub path: CssPath,
    pub declarations: CssDeclarationVec,
    pub conditions: DynamicSelectorVec,
}
```

`Stylesheet` and `StylesheetVec` are deleted. `Css.rules()` becomes
trivially `self.rules.iter()`. `sort_by_specificity` becomes one
stable sort over `(priority, specificity)`.

## 5. Layer recovery: the priority field

The wrapper's stated purpose was layer-aware sorting. We recover that
with an explicit `u8` `priority` on each rule block:

- Lower values are applied first; higher values win on conflict.
- `sort_by_specificity` becomes sort by `(priority, specificity)`.
- The merge code at `styled_dom.rs:961-976` keeps working: it just
  concatenates rule vectors. If a producer wants its rules at a
  different layer, it sets `priority` on those blocks before pushing.

### Priority slots

These are the concrete values to ship. They line up with the priority
levels documented in `internals/styling/cascade.md` so the
in-Css layering matches the prop_cache cascade priorities.

```rust
// css/src/css.rs

/// Layer priority for `CssRuleBlock`. Lower numbers cascade first;
/// higher numbers override earlier layers at the same specificity.
///
/// `u8` leaves 256 slots, so a new layer can be inserted between any
/// two existing slots without renumbering consumers. The gaps between
/// named slots (10 / 20 / 30 / 50) are intentional — fill them with
/// custom intermediate layers if/when `@layer` lands.
pub mod rule_priority {
    /// User-Agent / framework defaults. The lowest layer; widget code
    /// that emits its own default CSS uses this.
    pub const UA: u8 = 0;

    /// Stylesheets the host system reports (system fonts, theme CSS
    /// derived from `SystemStyle`). One step above UA so they win
    /// against framework defaults but lose against anything the app
    /// author writes.
    pub const SYSTEM: u8 = 10;

    /// Default for parser-produced rules: the app author's CSS.
    /// Everything coming out of `Css::from_string` lives here.
    pub const AUTHOR: u8 = 20;

    /// Inline `style="..."` / `NodeData::set_css(...)` rules — once
    /// the separate inline-vs-component unification (its own plan)
    /// folds inline storage into this same Vec.
    pub const INLINE: u8 = 30;

    /// Reserved for direct-rule runtime overrides. Today the
    /// prop_cache handles runtime overrides via
    /// `user_overridden_properties`; this slot is reserved so a
    /// future "push a CssRuleBlock at runtime" path stays above
    /// inline. Used only when a callback writes a full rule, not a
    /// single property.
    pub const RUNTIME: u8 = 50;
}
```

### Mapping to the existing cascade

`internals/styling/cascade.md:81-87` lists the cascade priority levels
inside `CssPropertyCache`. The `priority` field mirrors them so the
two layerings stay in sync:

| `cascade.md` priority | What it is | `rule_priority::*` |
|-----------------------|------------|---------------------|
| 1 (lowest)            | UA CSS     | `UA = 0`            |
| 2                     | author `*` rules | (still global_css_props, unchanged) |
| 3                     | author specific selectors | `AUTHOR = 20` |
| 4                     | inline `style="..."` / `NodeData::css_props` | `INLINE = 30` (after the inline collapse) |
| 5 (highest)           | runtime callback overrides | `user_overridden_properties` (unchanged); `RUNTIME = 50` reserved for full-rule overrides |

The `SYSTEM = 10` slot is new — it didn't exist in `cascade.md` because
today system-derived CSS is just author CSS that happens to come from
the OS. Giving it a dedicated slot lets app authors confidently
override system styles with a normal stylesheet.

### `!important` interaction

CSS `!important` is *not* encoded via `priority`. `!important` flips
the cascade direction within a single layer — UA important beats
author important beats author normal beats UA normal. Keep `priority`
for layer identity; encode `!important` on `CssDeclaration` as it is
today.

### Why a numeric field rather than re-introducing the boundary type

- **Cheap.** One byte per rule, padded into existing struct alignment
  (CssRuleBlock already carries a `CssPath` and two `Vec`s — adding a
  `u8` is free in practice).
- **Explicit.** `priority: 10` is more discoverable than "this rule
  lives in the author Stylesheet, which is the second one in the Css's
  Vec".
- **Mergeable.** Concatenating two `Vec<CssRuleBlock>` preserves layer
  identity per-rule. Concatenating two `Vec<Stylesheet>` doesn't tell
  you which Stylesheet was UA and which was author.
- **Fine-grained.** Mixed-priority rules within the same lexical
  source are expressible (e.g. `!important` could nudge priority by
  +1) without inventing a new Stylesheet.
- **Forward-compatible with `@layer`.** The CSS Cascade Layers spec
  treats layers as ordered groups of rules. A future `@layer
  framework, theme, app;` declaration can be implemented by mapping
  layer names to numeric priorities at parse time, then writing those
  numbers into the `priority` field.

## 6. Migration scope

| Area | Files | Change |
|------|-------|--------|
| Type definitions | `css/src/css.rs` | Delete `Stylesheet` + `StylesheetVec` and impls; add `priority` field to `CssRuleBlock`; update `Css::rules()`, `sort_by_specificity`, `RuleIterator`, `is_empty`, `new`. |
| Parser | `css/src/parser2.rs` (lines ~660-680) | Build `Css { rules: ... }` directly; rules get `priority: 10` (author default). |
| Codegen | `css/src/codegen/rust.rs` | Stop emitting `stylesheets: [...]` wrapper, emit `rules: [...]`. |
| System style readers | `dll/src/desktop/shell2/linux/system_style.rs:993-999`, `windows/system_style.rs:563`, `macos/system_style.rs:670`, `dll/src/desktop/menu_renderer.rs:644`, `css/src/system.rs:1628-1629` | Replace `stylesheets[0]` / `into_iter().next()` with the now-direct `rules` vec. Set `priority: 0` if these are framework defaults. |
| Merge sites | `core/src/styled_dom.rs:961-976`, `core/src/styled_dom.rs:1183`, `core/src/xml.rs:5170-5180`, `layout/src/xml/mod.rs:201` | Concatenate rule Vecs directly instead of flat-mapping over Stylesheets. |
| Cascade input | `core/src/prop_cache.rs:1026` | `css.sort_by_specificity()` keeps working (now sorts by `(priority, specificity)`). No structural change. |
| Tests | `tests/src/css.rs:97` and any test that constructs a `Css` literal | Drop the inner `Stylesheet { rules: ... }` wrapper. |

Estimated ~15 modified files, ~150 lines of mechanical change.
Per-callsite the change is uniform: `.stylesheets[0].rules` →
`.rules`, `.stylesheets.iter().flat_map(|s| s.rules.iter())` →
`.rules.iter()`.

## 7. ABI / FFI considerations

- `AzCss` and `AzStylesheet` are exposed in the FFI. After the change,
  `AzStylesheet` and `AzStylesheetVec` no longer exist; `AzCss`'s
  layout changes (`stylesheets: AzStylesheetVec` → `rules: AzCssRuleBlockVec`).
- C / C++ / Python bindings need a codegen regeneration. External
  code that touched `AzStylesheet` directly has to migrate to
  `AzCss.rules`.
- `api.json` entries for `Css`, `Stylesheet`, `CssVec`, `StylesheetVec`
  need updating; the patch tooling under `doc/src/autofix/` will need
  a one-shot run.
- This is a clean break — no deprecated alias kept around. The
  user has signed off on the ABI break.

## 8. Risk register

| Risk | Severity | Mitigation |
|------|----------|------------|
| External consumers of `AzStylesheet` break | High | Documented in changelog as a breaking change; codegen regenerated; migration note in the styling internals doc. |
| `sort_by_specificity` ordering changes for some real input | Medium | New sort key is `(priority, specificity)`. With all current rules at `priority: 10`, the order is identical to today's single-Stylesheet sort. Add a unit test pinning the order. |
| Merge code that relied on per-Stylesheet identity | Low | Confirmed already-flattening. No identity to lose. |
| Codegen drift between Rust and bindings | Medium | Run the full codegen pipeline after the change; rebuild C/C++/Python examples to verify. |
| @layer support added later wants real boundaries | Low | The priority field is the foundation for `@layer`. No re-architecture needed; layer parsing assigns priority numbers. |

## 9. Step-by-step implementation order

1. **Add `priority: u8` to `CssRuleBlock`**, default 0. Update
   `CssRuleBlock::new` to also accept a priority. All existing
   callsites continue to compile (the `derive(Default)` keeps working).
2. **Update `sort_by_specificity`** to sort by `(priority, get_specificity(path))`.
   Add a test that pins ordering for a mixed-priority input.
3. **Make the parser set `priority: rule_priority::AUTHOR`** (= 20) on
   every produced rule block.
4. **Migrate merge sites** to concatenate rule Vecs directly. Each
   merge site that has a clear "this is UA / framework" intent gets
   `priority: rule_priority::UA`; system-style readers get `SYSTEM`;
   everything else stays at `AUTHOR`.
5. **Replace `Css.stylesheets` with `Css.rules`**: delete
   `Stylesheet`, `StylesheetVec`, all their impls; collapse
   `RuleIterator`; update every call site in one pass.
6. **Regenerate codegen**: run the FFI generator, rebuild C/C++/Python
   examples, fix any binding-side fallout.
7. **Update docs**: `internals/styling/cascade.md` mentions
   "stylesheet" — replace with "rule layer". Add a short note in the
   styling guide about the `priority` field for advanced users.
8. **Final sweep**: search the repo for `Stylesheet`, `stylesheets[`,
   `vec![Stylesheet`, `StylesheetVec` to catch stragglers.

Each step is a separate commit so the diff is reviewable.
