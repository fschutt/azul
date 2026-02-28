# Deferred Cascade Architecture — Design Document

## 1. Problem Statement

The current layout callback returns `StyledDom`:

```rust
pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutCallbackInfo) -> StyledDom;
```

Each component calls `dom.style(css)` independently, producing a fully styled
subtree which is then appended via `append_child()` into the parent `StyledDom`.
This per-component styling causes several problems:

1. **Redundant work**: Each `StyledDom::create()` runs 4 full-tree passes
   (restyle → UA-CSS → inherit → compact-cache), even though the component's
   CSS will later be overridden by parent rules after `append_child()`.
2. **Broken inheritance**: After `append_child()`, no re-inheritance pass runs.
   Inherited properties (`color`, `font-size`, `direction`) from the parent DOM
   do not flow into the appended component subtree.
3. **Stale compact cache**: `append_child()` merges the CSS property caches
   with `get_css_property_cache_mut().append()` but does NOT rebuild the
   compact cache. Tier 1/2/2b entries from the child remain based on the
   child's isolated cascade — they don't reflect parent overrides.
4. **UA CSS pointer-chasing**: UA defaults are resolved at two points —
   during `apply_ua_css()` in `StyledDom::create()` AND as a runtime fallback
   in `get_property_slow()`. This fallback adds a function-call + pattern-match
   per property lookup for any node that didn't hit the compact cache.
5. **Anonymous node recomputation**: `generate_anonymous_table_elements()`
   runs at the end of `StyledDom::create()`, inserting wrapper nodes. If a
   parent DOM also generates anonymous elements, the child's anonymous nodes
   may need adjustment — but no such adjustment happens after `append_child()`.

### 1.1 What is a Component? (Terminology)

A **component** is a CSS scoping boundary. When a component renders its DOM
subtree, it calls `dom.style(component_css)` to scope its CSS rules. Those
rules are scoped to the subtree — they cannot leak upward to parent or sibling
nodes. Parent CSS *can* cascade into the component subtree (intentional
asymmetric scoping).

Components are tracked via `ComponentOrigin` on `NodeDataExt` for the debug
server's component tree visualization and code generation system. They are
NOT the same as VirtualizedViews.

A **VirtualizedView** is a lazy-loading mechanism for
DOM subtrees. VirtualizedView callbacks are invoked during layout when the viewport needs
content (scroll, resize). Each VirtualizedView gets its own `DomId` and its own
`StyledDom` instance — it participates in layout as a separate document, not
as a merged subtree. VirtualizedViews are orthogonal to the cascade discussion here.

---

## 2. Current Architecture

### 2.1 Layout Callback Flow

```
User LayoutCallback
  ├── component_a.render()
  │   ├── build Dom (NodeData tree)
  │   ├── dom.style(component_a_css)    ← StyledDom::create()
  │   │   ├── restyle()                  (CSS selector matching)
  │   │   ├── apply_ua_css()             (fill UA defaults)
  │   │   ├── compute_inherited_values() (depth-first inheritance)
  │   │   ├── build_compact_cache()      (tier 1/2/2b encoding)
  │   │   └── generate_anonymous_table_elements()
  │   └── return StyledDom_A
  │
  ├── component_b.render()
  │   └── ... same 5-pass pipeline ...  ← StyledDom_B
  │
  ├── root_dom.style(app_css)            ← StyledDom_root
  │   └── ... same 5-pass pipeline ...
  │
  ├── root_styled.append_child(StyledDom_A)  ← reindex + merge arrays
  ├── root_styled.append_child(StyledDom_B)  ← reindex + merge arrays
  └── return StyledDom (fully composed)

regenerate_layout()
  ├── call LayoutCallback → StyledDom
  ├── resolve_icons()
  ├── wrap_with_CSD()                    ← another append_child
  ├── reconcile_dom()
  ├── apply_runtime_states()
  └── layout_and_generate_display_list()
      └── layout_dom_recursive()
          ├── font chain resolution      ← NO caching (Fix 5 addresses this)
          ├── solver3::layout_document()  ← fingerprint diff + incremental layout
          └── scan_for_virtualized_views() + recurse  ← separate DomIds
```

### 2.2 Per-Node Memory (Current)

| Structure           | Size     | Notes                              |
|---------------------|----------|------------------------------------|
| NodeData            | ~192 B   | After our optimization commits     |
| NodeHierarchyItem   | 32 B     | Parent/sibling/child links         |
| StyledNode          | 10 B     | State flags                        |
| CascadeInfo         | 8 B      | index_in_parent, is_last_child     |
| Compact Tier 1      | 8 B      | u64 bit-packed enums               |
| Compact Tier 2      | 96 B     | CompactNodeProps (dimensions)      |
| Compact Tier 2b     | 24 B     | CompactTextProps                   |
| CssPropertyCache overhead | 96 B | 4 layers × 24 B Vec header       |
| **Subtotal**        | **~466 B** | Excluding inline CSS              |

Plus inline CSS: 7 properties × ~56 B (after Fix 1) = ~392 B/node,
or 7 × ~24 B (after Fix 1+2) = ~168 B/node.

### 2.3 Property Lookup (Current)

The `get_css_property_pixel!` / `get_css_property!` macros in `getters.rs` use:

```
FAST PATH (normal state + compact cache exists):
    cc.get_xxx(node_id.index())  →  O(1) array + bitshift/sentinel

SLOW PATH (hover/focus/active OR non-compact property):
    6 pseudo-states × 3 layers (inline → stylesheet → cascaded) = 18 lookups
    + computed_values binary search
    + ua_css::get_ua_property() function call
```

---

## 3. Proposed Architecture: Deferred Cascade

### 3.1 Core Idea

Change the layout callback to return `Dom` instead of `StyledDom`. `Dom` is
a recursive tree structure:

```rust
pub struct Dom {
    pub root: NodeData,
    pub children: Vec<Dom>,
    pub css: Vec<Css>,        // NEW: ordered list of CSS stylesheets
}
```

CSS is accumulated via chainable `.style()` calls that push into `css`:

```rust
impl Dom {
    pub fn style(mut self, css: Css) -> Self {
        self.css.push(css);
        self
    }
}
```

This means `Dom::div().style(Css::a()).style(Css::b())` does what
`Dom::div().style(Css::a()).restyle(Css::b())` does today — stylesheets are
applied in push order during the single deferred cascade pass. No cascade
runs at `.style()` time; the CSS objects are just stored.

```rust
// NEW signature:
pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom;
```

Child composition is a simple `Vec::push`:

```rust
impl Dom {
    pub fn append(mut self, child: Dom) -> Self {
        self.children.push(child);
        self
    }
}
```

No reindexing, no array merging — just a push into the recursive tree.
Flattening into contiguous arrays happens once, during the `Dom → StyledDom`
conversion in `regenerate_layout()`.

### 3.2 New Flow

```
User LayoutCallback
  ├── component_a.render()
  │   ├── Dom::div()
  │   │     .style(component_a_base_css)   ← push Css, no cascade
  │   │     .style(component_a_theme_css)  ← push Css, no cascade
  │   │     .append(child_1)
  │   │     .append(child_2)
  │   └── return Dom_A
  │
  ├── component_b.render()
  │   └── Dom::div().style(component_b_css).append(...)  → Dom_B
  │
  ├── root_dom
  │     .style(app_css)                    ← push Css, no cascade
  │     .append(Dom_A)                     ← Vec::push (O(1))
  │     .append(Dom_B)                     ← Vec::push (O(1))
  └── return Dom (recursive tree, unstyled)

regenerate_layout()
  ├── call LayoutCallback → Dom
  ├── resolve_icons()
  ├── Dom → StyledDom (flatten + SINGLE cascade pass):  ← NEW
  │   ├── flatten recursive Dom into contiguous NodeData arrays
  │   ├── for each node, apply its css[] in order (restyle_subtree)
  │   │   ← inner CSS first (lowest priority), outer CSS last (highest)
  │   ├── apply_ua_css()
  │   ├── compute_inherited_values()       (single depth-first pass)
  │   ├── build_compact_cache()            (single pass, complete)
  │   └── generate_anonymous_table_elements()
  ├── wrap_with_CSD()
  ├── reconcile_dom()
  ├── apply_runtime_states()
  └── layout_and_generate_display_list()
```

### 3.3 What Changes

| Aspect | Current | Proposed |
|--------|---------|----------|
| Callback return type | `StyledDom` | `Dom` |
| Dom structure | Flat arrays (NodeData, hierarchy, etc.) | Recursive tree (`root` + `children: Vec<Dom>`) |
| CSS storage | Immediate cascade via `dom.style(css)` | Deferred: `css: Vec<Css>` per Dom node, push order = priority |
| Number of cascade passes | N (one per component) | 1 (after flatten + composition) |
| Inheritance across components | Broken (no re-inherit after append) | Correct (single depth-first pass) |
| Compact cache validity | Stale after append_child | Always fresh (built once, after cascade) |
| UA CSS in compact cache | Not baked in; runtime fallback needed | Baked in; O(1) access |
| `append` / `append_child` cost | O(n) reindex + sort non_leaf_nodes + CSS cache merge | O(1) `Vec::push` into parent's children |
| Flatten cost | N/A (already flat) | O(n) single pass to build contiguous arrays |
| Anonymous elements | Per-component (may need adjustment) | Single pass on final tree |
| Component CSS scoping | By order of operations (.style → append) | Explicit via `css[]` on each Dom node + subtree range |

---

## 4. Analysis: perf-fixes Branch in Light of This Proposal

The `origin/perf-fixes` branch implements Fixes 3, 4, and 5 from the
performance report. How do they interact with the deferred cascade proposal?

### 4.1 Fix 3: Remove tier3_overflow (commit 4f71f3f5)

**Verdict: KEEP. Orthogonal to both architectures.**

Removing `tier3_overflow` and falling back to `get_property_slow()` for
non-compact properties (background, transform, box-shadow) is correct in both
architectures. The compact tiers 1/2/2b cover all layout-hot properties.

In the deferred cascade architecture, this becomes even simpler: since UA CSS
is baked into the compact cache, the slow-path fallback never needs to check
ua_css separately — it only checks the 5 cascade layers.

### 4.2 Fix 4 Part A: CompactInlineProps (commit 6caeb744)

**Verdict: RECONSIDER. The value proposition changes significantly.**

The `CompactInlineProps` struct compresses per-node inline CSS into a compact
binary format with per-pseudo-state sorted Vecs for O(log n) binary search.
The motivation was to avoid storing 7 × 1520 B = 10,976 B of `CssProperty`
enums per node.

**After Fix 1 (Scrollbar removal — already done on our branch):**
- `CssProperty` is now ~56 B (not 1520 B)
- 7 inline properties = 7 × ~104 B (`CssPropertyWithConditions`) = ~728 B/node
- For 500 nodes: 364 KB (not 5.35 MB)
- The **27× reduction** in enum size already eliminates most of the memory waste

**After Fix 1+2 (Scrollbar + BoxOrStatic):**
- `CssProperty` ≈ 24 B → 7 × ~72 B = ~504 B/node
- For 500 nodes: 252 KB

**CompactInlineProps adds complexity:**
- New `CompactInlineProps` struct with tier1/tier2/tier2b/overflow per pseudo-state
- `InlineStyleTable` dedup table with HashMap
- `build_inline_style_table()` pass during `StyledDom::create()`
- `check_state!` macro reworking all 6 pseudo-state lookups
- New `inline_style_keys: Vec<u32>` on CssPropertyCache

**With the deferred cascade, CompactInlineProps becomes unnecessary because:**
- Component CSS (formerly "inline CSS from .style()") is stored as `Css` on 
  the Dom node's `css[]` — a single shared allocation for all nodes in that
  component. There's no per-node duplication of CSS properties before cascade.
- The cascade pass resolves component CSS into the same cascade layers
  (css_props/cascaded_props/computed_values) as stylesheet CSS.
- The compact cache is built ONCE after all cascade layers are filled,
  giving O(1) access.
- Actual per-node inline styles (`set_inline_style("height:30px")`) remain,
  but these are typically few and small. After Fix 1, they're ~56 B each.
  The dedup table adds more complexity than it's worth for <1 KB/node.

**Recommendation:** Drop CompactInlineProps (Fix 4 Part A). The combination
of Fix 1 (already done) + deferred cascade eliminates the problem at the root.

### 4.3 Fix 4 Part B: Source-Based Deduplication (commit 6caeb744)

**Verdict: PARTIALLY SUPERSEDED.**

Source-based dedup hashes the inline style string to share `CompactInlineProps`
across nodes with identical styles. With the deferred cascade:

- Component CSS is naturally shared: all nodes in a component share the same
  `Css` object (stored once in the Dom node's `css[]`, not per-node).
- Per-node inline styles from `set_inline_style()` could still benefit from
  dedup, but the savings after Fix 1 are small: 500 nodes × 728 B = 364 KB
  total, and the HashMap + table management overhead may exceed the savings.

**Recommendation:** Drop source dedup. If needed later, it's a leaf
optimization that can be added without architectural changes.

### 4.4 Fix 4 Fingerprinting: O(1) inline_css_hash (commit fca1e8d2)

**Verdict: SUPERSEDED by deferred cascade.**

The `compute_with_inline_key()` optimization on `NodeDataFingerprint` uses
the dedup table key as an O(1) hash instead of hashing all CssProperty content.
This is only needed because `inline_css_hash` currently hashes N × 1520 B of
property data per node.

With deferred cascade:
- The fingerprint `inline_css_hash` computes over per-node inline styles only
  (from `set_inline_style()`), which are small after Fix 1.
- Component CSS is not per-node — it's in the parent Dom's `css[]` — so it doesn't
  enter the per-node fingerprint at all.
- Hashing 7 × 56 B = 392 B per node is fast (single cache line territory).
  The O(1) key optimization adds complexity with minimal gain.

**Recommendation:** Drop the inline_key fingerprint optimization. Standard
hashing of small inline style Vecs is sufficient.

### 4.5 Fix 5: Font Chain Caching (commit 6caeb744)

**Verdict: KEEP. Independent of cascade architecture.**

The `font_stacks_hash: u64` on `LayoutWindow`, XOR of `tier2b.font_family_hash`
values, that skips all 5 font resolution steps when unchanged — this is a pure
optimization in the layout phase, completely orthogonal to how/when the cascade
runs. It works equally well in both architectures.

### 4.6 Summary Table

| Fix | perf-fixes Status | Deferred Cascade Impact | Recommendation |
|-----|-------------------|------------------------|----------------|
| Fix 3 (tier3_overflow) | ✅ Implemented | Simpler (no UA fallback in slow path) | **Keep** |
| Fix 4A (CompactInlineProps) | ✅ Implemented | Unnecessary (component CSS not per-node) | **Drop** |
| Fix 4B (source dedup) | ✅ Implemented | Superseded (CSS shared via `css: Vec<Css>` on Dom) | **Drop** |
| Fix 4 fingerprint (O(1) hash) | ✅ Implemented | Superseded (small inline styles → fast hash) | **Drop** |
| Fix 5 (font chain cache) | ✅ Implemented | Orthogonal | **Keep** |

---

## 5. Efficiency Analysis

### 5.1 CPU: Cascade Cost

**Current (N components):**
```
Per component:
  restyle()                    O(n_i × R)    — n_i nodes, R CSS rules
  apply_ua_css()               O(n_i × P)    — P = ~30 UA properties
  sort_cascaded_props()        O(n_i × k log k)
  compute_inherited_values()   O(n_i × I)    — I = inheritable properties
  build_compact_cache()        O(n_i × C)    — C = compact properties
Total: Σ_i O(n_i × (R + P + k log k + I + C))
     = O(N_total × (R + P + k log k + I + C))   — same asymptotic
```

The constant factor is actually identical: each node is processed once per pass
regardless of how many components there are, since each node belongs to exactly
one component. The issue is NOT asymptotic complexity but:

- **Cache locality**: N separate `StyledDom::create()` calls process N small
  separate arrays. One unified pass processes one large contiguous array.
  For L1/L2 cache (32-64kb/256kb-1MB), processing 500 nodes contiguously
  is better than 50 components × 10 nodes each with separate allocations.

- **Inheritance correctness**: The current architecture silently drops
  cross-component inheritance. This is a correctness bug, not just perf.

- **Redundant UA CSS**: `apply_ua_css()` runs N times (once per component).
  With deferred cascade, it runs once. For 500 nodes with ~30 UA properties
  each, that's 15,000 bitset checks regardless — but the function call
  overhead and instruction-cache pressure of N invocations is avoided.

**Estimated CPU savings:** ~10-20% of cascade time from better cache locality
and eliminated redundant setup/teardown of per-component cascade. Not a
fundamental algorithmic improvement, but a meaningful constant-factor gain.

### 5.2 Memory: Inline CSS Deduplication

**Current (500 nodes, 7 inline props each, after Fix 1):**
```
Per-node inline CSS:  7 × CssPropertyWithConditions(~104 B) = 728 B
500 nodes:            364 KB
With CompactInlineProps + dedup (Fix 4): ~4 KB
```

**Deferred cascade (500 nodes, same scenario):**
```
Component CSS (Css object):  ~2 KB for a small stylesheet (shared, not per-node)
Per-node inline CSS:         Only from set_inline_style(), typically 0-3 properties
                             3 × 104 B = 312 B (for nodes that have it)
No per-node duplication of component CSS rules.
```

In `scrolling.c`, the 7 "inline" properties actually come from `.style(css)` —
they're component-level CSS rules that matched via selectors. With deferred
cascade, these stay in the `Css` object and are resolved during the single
cascade pass into `css_props`/`cascaded_props` layers of `CssPropertyCache`.
No per-node storage of the matched rules is needed before the cascade.

**Net memory saving vs current + Fix 4:** Roughly equivalent. Both approaches
eliminate the per-node duplication. But deferred cascade does it without the
CompactInlineProps/InlineStyleTable machinery.

### 5.3 Memory: UA CSS Baked into Compact Cache

**Current:** UA CSS is checked at runtime via `get_ua_property()` pattern-match.
The compact cache does NOT include UA defaults — it's built from resolved
cascade layers, and UA properties that weren't explicitly set are missing.

**Deferred cascade:** `apply_ua_css()` fills missing properties into the cascade
layers BEFORE `build_compact_cache()`. Since there's only one cascade pass, the
compact cache includes UA defaults. Every property lookup for normal-state nodes
hits the compact cache O(1) — no UA fallback needed.

This eliminates the most common slow-path entry: a normal-state node querying
a property that was never explicitly set (e.g., `display` on a div defaults to
`Block` via UA CSS). Currently this falls through the compact cache (which
returns `None`), then through all cascade layers (empty), then to
`get_ua_property()`. With UA baked in, it's a single array lookup.

**Wait — is this already the case?** Let me verify. In the current code,
`build_compact_cache()` is called AFTER `apply_ua_css()` and
`compute_inherited_values()`. So `computed_values` DOES contain UA defaults.
The compact cache builder reads from `computed_values`:

```
build_compact_cache() reads computed_values[node_id]
  → which was built by compute_inherited_values()
    → which was built after apply_ua_css()
```

**So UA CSS IS baked into the compact cache already in the current architecture.**
The runtime `get_ua_property()` fallback in `get_property_slow()` is a *safety
net* for cases where the compact cache doesn't exist or the property isn't in
tier 1/2/2b. The deferred cascade doesn't change this behavior.

**Correction to initial assumption:** UA CSS baking is NOT a benefit of
deferred cascade — it's already done. The UA fallback in getters is for
non-compact properties only, and Fix 3 (tier3_overflow removal) already
addresses this by having those properties use `get_property_slow()`.

### 5.4 Revised Benefit Assessment

| Benefit | Real? | Magnitude |
|---------|-------|-----------|
| Correct cross-component inheritance | **Yes** | Correctness fix |
| Fresh compact cache (no stale after append) | **Yes** | Correctness fix |
| Better cache locality (one pass vs N) | **Yes** | ~10-20% cascade speedup |
| Eliminate CompactInlineProps complexity | **Yes** | Code simplification |
| UA CSS in compact cache | **No** | Already the case |
| Fewer cascade passes | **Marginal** | Same total work, fewer invocations |
| Simpler `append_child` (no CSS merge) | **Yes** | Simpler code, O(n) → O(n) but less work |

The strongest arguments for the refactoring are **correctness** (inheritance,
compact cache freshness) rather than raw performance.

---

## 6. C API Impact

### 6.1 What Breaks

```c
// CURRENT (breaks):
AzStyledDom layout_callback(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom dom = build_ui(data);
    AzCss css = AzCss_from_string("...");
    return AzDom_style(&dom, css);   // ← cascades immediately, returns StyledDom
}

// NEW:
AzDom layout_callback(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom dom = build_ui(data);
    AzCss css = AzCss_from_string("...");
    AzDom_style(&dom, css);   // ← same function name, but now just pushes into dom.css[]
    return dom;               // ← returns Dom, not StyledDom
}
```

**Breaking changes:**
1. `LayoutCallbackType` return type: `StyledDom` → `Dom`
2. `Dom::style()` semantics change: no longer cascades, just stores CSS
3. `StyledDom::append_child()` for user-facing composition is replaced by
   `Dom::append()` — which is now just `children.push(child)` on the
   recursive Dom tree
4. All user layout callbacks must update return type
5. Multiple `.style()` calls accumulate (replaces `.restyle()`)

**Non-breaking:**
- `Dom` API (building nodes, setting properties) — unchanged
- `Css` parsing API — unchanged
- Inline style API (`set_inline_style`) — unchanged
- `.style()` call sites — same name, just deferred semantics
- VirtualizedView callbacks — these already return `StyledDom` and are invoked by the
  framework, not by user layout callbacks. They would continue to return
  `StyledDom` (or switch to `Dom` too — see Section 7.2)

### 6.2 Migration Effort

For a typical C user:
```diff
-AzStyledDom layout_callback(AzRefAny data, AzLayoutCallbackInfo info) {
+AzDom layout_callback(AzRefAny data, AzLayoutCallbackInfo info) {
     AzDom dom = build_ui(data);
     AzCss css = AzCss_from_string("...");
-    return AzDom_style(&dom, css);
+    AzDom_style(&dom, css);   // same call, but now just stores CSS
+    return dom;
 }
```

This is a 2-line change per layout callback (return type + return value).
The `.style()` call itself stays the same — only its semantics change from
"cascade now" to "store for later". Each component's `render()` changes
similarly: return `Dom` instead of `StyledDom`.

Multiple stylesheets chain naturally:
```c
AzDom component_render(...) {
    AzDom dom = build_component_ui();
    AzDom_style(&dom, base_css);    // push first
    AzDom_style(&dom, theme_css);   // push second (higher priority)
    return dom;
}
```

**Since there are no stability guarantees on the C API**, this is acceptable.

---

## 7. Implementation Plan

### 7.1 Phase 1: Internal Refactoring (no API change yet)

Keep the current `LayoutCallbackType → StyledDom` signature but fix the
internal cascade to be correct:

1. After all `append_child()` calls in `regenerate_layout()`, re-run
   `compute_inherited_values()` and `build_compact_cache()` on the
   composed tree.
2. This fixes the inheritance and stale-cache bugs without breaking the API.
3. Cost: one extra inheritance + compact-cache pass. Acceptable for correctness.

**This can be done independently and immediately.** It's a bugfix, not a
refactoring.

### 7.2 Phase 2: API Change (`Dom` return type + recursive structure)

1. Change `Dom` from flat arrays to recursive tree:
   ```rust
   pub struct Dom {
       pub root: NodeData,
       pub children: Vec<Dom>,
       pub css: Vec<Css>,  // ordered list of stylesheets
   }
   ```
2. Change `Dom::style()` from "cascade now → StyledDom" to "push CSS":
   ```rust
   pub fn style(mut self, css: Css) -> Self {
       self.css.push(css);
       self
   }
   ```
3. Change `Dom::append()` to push child into recursive tree:
   ```rust
   pub fn append(mut self, child: Dom) -> Self {
       self.children.push(child);
       self
   }
   ```
4. Change `LayoutCallbackType` return type to `Dom`
5. In `regenerate_layout()`, after receiving `Dom`:
   - Flatten recursive `Dom` tree into contiguous arrays:
     depth-first walk, assign NodeIds, build `NodeHierarchyItem` links,
     collect `NodeData` into flat Vec, record each node's `css[]` with
     its subtree range
   - Build cascade info (index_in_parent, is_last_child)
   - For each node that has `css[]`, apply stylesheets in order via
     `restyle_subtree(css[i], subtree_range)` — later entries override
     earlier ones (higher cascade priority)
   - For nested components: parent CSS has higher priority than child CSS
     (applied after child CSS in the cascade)
   - `apply_ua_css()` → `compute_inherited_values()` → `build_compact_cache()`
   - `generate_anonymous_table_elements()`
6. Remove `Dom::style() -> StyledDom` signature (the new `.style()` returns `Dom`)
7. Remove `.restyle()` — multiple `.style()` calls replace it
8. Update all examples (C, Rust, Python)
9. Update VirtualizedView callbacks: switch the callback to `Dom`

### 7.3 Phase 3: Cherry-pick from perf-fixes

From the `origin/perf-fixes` branch, cherry-pick:
- **Fix 3** (commit `4f71f3f5`): tier3_overflow removal — applies cleanly
- **Fix 5** (commit `6caeb744`): font chain caching — extract the
  `font_stacks_hash` changes from the combined Fix 4+5 commit

Do NOT cherry-pick:
- Fix 4A (CompactInlineProps) — superseded
- Fix 4B (source dedup) — superseded
- O(1) inline_css_hash fingerprint — superseded

### 7.4 Phase 4: Component CSS Scoping Semantics

Define clear scoping rules for CSS on the recursive `Dom` tree:

1. **Per-node CSS scope**: Each `Dom` node's `css: Vec<Css>` applies only
   to that node's subtree (the node itself + all `children` recursively).
2. **Push order = priority**: Within a single node's `css[]`, later entries
   have higher cascade priority. `dom.style(a).style(b)` means `b` overrides
   `a`, matching today's `.style(a).restyle(b)` semantics.
3. **Parent override**: When flattening the recursive tree, a parent Dom's
   CSS has higher cascade priority than a child Dom's CSS. Priority order:
   - Root Dom's `css[]` (highest)
   - Outer component Dom's `css[]`
   - Inner component Dom's `css[]` (lowest, before UA)
   
   This mirrors the current behavior where the outer `.style()` runs last.

4. **Implementation**: During the flatten pass (recursive Dom → flat arrays),
   maintain a CSS stack. When entering a Dom node that has `css[]`, push
   all its stylesheets onto the stack. When leaving the subtree, pop them.
   During restyle, match each node against all CSS objects on the stack,
   with priority based on stack depth (innermost = lowest priority) and
   push order within the same depth.

---

## 8. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| VirtualizedView callbacks need updating | Medium | Medium | VirtualizedViews are framework-internal; update is mechanical |
| Debug server component system breaks | Low | Low | ComponentOrigin is metadata-only, not affected |
| Reconciliation (diff) changes needed | Medium | High | NodeDataFingerprint needs to include css[] hash |
| Performance regression from single large cascade | Low | Medium | Same total work; benchmark before/after |
| Subtle CSS priority changes | Medium | Medium | Add reftests for cross-component cascade scenarios |

---

## 9. Recommendation

**Proceed with the deferred cascade refactoring (Phases 1-4).**

The primary motivations are correctness (inheritance, compact cache freshness)
rather than performance. The performance gains are real but modest (~10-20%
cascade speedup from cache locality, elimination of CompactInlineProps
complexity). The code simplification is significant: changing `Dom::style()` from
immediate cascade to deferred push, replacing `append_child()` array
reindexing with recursive `Vec::push`, and not needing the
CompactInlineProps/InlineStyleTable machinery from the perf-fixes branch.

**Phase 1 (re-inherit + rebuild compact cache after append) should be done
immediately** as it fixes real correctness bugs regardless of whether the
full API change proceeds.

**The C API break is acceptable** given the lack of stability guarantees and
the small migration effort (3-line change per callback).

From perf-fixes, **cherry-pick Fix 3 and Fix 5** only.
