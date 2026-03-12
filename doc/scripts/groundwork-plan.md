# Groundwork Plan: Reduce NOT_APPLICABLE to Near-Zero

## Goal

After the first `claude-exec` run, 509/1316 paragraphs (39%) were NOT_APPLICABLE.
Of those, ~213 are potentially implementable if we add missing CSS properties,
expand the allowed file scope, and wire up existing but unused infrastructure.

This plan covers ALL groundwork needed so a second `claude-exec` run can
implement every remaining paragraph. Target: <50 NOT_APPLICABLE (spec metadata only).

## Problem: Agent Prompt Too Restrictive

The current prompt in `executor.rs` line 2689-2690 says:

```
Only modify files in `layout/src/solver3/`, `layout/src/text3/`, and `css/src/`.
Do NOT modify `display_list.rs`, rendering code, or any other files.
```

This blocked agents from implementing:
- Stacking context fixes (display_list.rs)
- Scroll handling (window.rs, scrollbar.rs)
- Painting order / clipping (display_list.rs)
- Core type additions (core/src/)
- Pagination/fragmentation (paged_layout.rs, pagination.rs)

**Fix**: Expand allowed files to the full layout crate + css crate + core types.

---

## Phase 1: Fix Agent Prompt (executor.rs)

### 1a. Remove file scope restriction

Replace lines 2689-2690 in `doc/src/spec/executor.rs`:

```
OLD:
- Only modify files in `layout/src/solver3/`, `layout/src/text3/`, and `css/src/`.
  Do NOT modify `display_list.rs`, rendering code, or any other files.

NEW:
- You may modify any file in `layout/src/` and `css/src/`.
  This includes `solver3/`, `text3/`, `display_list.rs`, `window.rs`,
  `scrollbar.rs`, `paged_layout.rs`, `pagination.rs`, etc.
- You may also add new types in `core/src/` if needed for new CSS properties.
- Do NOT modify `taffy_bridge.rs` (flex/grid is handled by Taffy).
- Do NOT modify files outside `layout/`, `css/`, and `core/`.
```

### 1b. Remove NOT_APPLICABLE for rendering

Replace lines 2626-2629:

```
OLD:
**If a spec paragraph does NOT apply to this codebase** (e.g., it only
applies to flex/grid which is handled by Taffy, or it describes user agent
behavior we don't implement, or it is purely about rendering/painting):
- Do NOT commit for that paragraph. Output `NOT_APPLICABLE` and move on.

NEW:
**If a spec paragraph does NOT apply to this codebase** (e.g., it only
applies to flex/grid which is handled by Taffy, or it describes user agent
default stylesheets we don't control):
- Do NOT commit for that paragraph. Output `NOT_APPLICABLE` and move on.
- NOTE: Rendering, painting, clipping, stacking contexts, and scroll
  handling ARE in scope. Implement them in `display_list.rs`, `window.rs`,
  or the appropriate file. Do NOT mark rendering paragraphs as NOT_APPLICABLE.
- Only use NOT_APPLICABLE for: spec metadata, flex/grid (Taffy), UA defaults,
  and SVG/MathML features.
```

---

## Phase 2: Missing CSS Properties

These CSS properties need to be added to `css/src/props/` so agents can use them.
Each needs: enum definition, parser, CssPropertyType variant, and api.json entry.

### 2a. `unicode-bidi` (unlocks ~42 bidi paragraphs)

File: `css/src/props/style/text.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleUnicodeBidi {
    Normal,
    Embed,
    Isolate,
    BidiOverride,
    IsolateOverride,
    Plaintext,
}
```

Parser values: `normal`, `embed`, `isolate`, `bidi-override`, `isolate-override`, `plaintext`

Wire into: `layout/src/text3/cache.rs` line ~5217 where the TODO says
"unicode-bidi != normal (not yet implemented)".

### 2b. `text-box-trim` + `text-box-edge` (unlocks ~17 paragraphs)

File: `css/src/props/style/text.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextBoxTrim {
    None,
    TrimStart,
    TrimEnd,
    TrimBoth,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextBoxEdge {
    Auto,
    TextEdge,      // text-over + text-under
    CapHeight,     // cap + alphabetic
    ExHeight,      // ex + alphabetic
}
```

Wire into: `layout/src/text3/cache.rs` — `UnifiedConstraints` struct + `position_one_line()`
for trimming half-leading on first/last lines.

### 2c. `dominant-baseline` + `alignment-baseline` (unlocks ~7 paragraphs)

File: `css/src/props/style/text.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleDominantBaseline {
    Auto,
    TextBottom,
    Alphabetic,
    Ideographic,
    Middle,
    Central,
    Mathematical,
    Hanging,
    TextTop,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleAlignmentBaseline {
    Baseline, // = auto
    TextBottom,
    Alphabetic,
    Ideographic,
    Middle,
    Central,
    Mathematical,
    TextTop,
}
```

Wire into: `layout/src/text3/cache.rs` vertical-align / baseline alignment code.

### 2d. `text-combine-upright` (unlocks ~10 paragraphs)

File: `css/src/props/style/text.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextCombineUpright {
    None,
    All,
    // Digits(u8) removed — not in latest spec
}
```

Wire into: `layout/src/text3/cache.rs` for horizontal-in-vertical composition.

### 2e. `text-orientation` (unlocks ~4 paragraphs)

File: `css/src/props/style/text.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleTextOrientation {
    Mixed,
    Upright,
    Sideways,
}
```

### 2f. `visibility` (unlocks ~2 paragraphs)

Already exists? Check. If not:

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleVisibility {
    Visible,
    Hidden,
    Collapse,
}
```

Wire into: `display_list.rs` — skip painting when hidden, collapse table rows/columns.

### 2g. `initial-letter-align` + `initial-letter-wrap` (unlocks ~47 paragraphs)

File: `css/src/props/style/text.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleInitialLetterAlign {
    Auto,
    Alphabetic,
    Hanging,
    Ideographic,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleInitialLetterWrap {
    None,
    First,
    All,
    Grid,
}
```

Wire into: `layout/src/solver3/fc.rs` where initial_letter is already read but unused,
and `layout/src/text3/cache.rs` for drop-cap line exclusion.

### 2h. `scrollbar-gutter` (unlocks ~7 scroll overflow paragraphs)

File: `css/src/props/layout/overflow.rs`

```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleScrollbarGutter {
    Auto,
    Stable,
    StableBothEdges,
}
```

Wire into: `layout/src/solver3/scrollbar.rs`.

### 2i. `overflow-clip-margin` (unlocks a few overflow paragraphs)

File: `css/src/props/layout/overflow.rs`

```rust
// Simple length value — clip region extends beyond the box by this amount
// when overflow: clip is used
pub type StyleOverflowClipMargin = LayoutLength;
```

Wire into: `display_list.rs` clip rect calculation.

---

## Phase 3: Layout Infrastructure

### 3a. Initial-letter layout (fc.rs + cache.rs)

The `initial_letter` field in `UnifiedConstraints` is populated but never used.
Implement basic drop-cap:

1. In `fc.rs` IFC layout: detect `initial_letter.size > 0`
2. Size the initial letter box to span N lines (size parameter)
3. Sink it by M lines (sink parameter)
4. Create a float-like exclusion for surrounding text to wrap around
5. Apply `initial-letter-align` for baseline positioning

### 3b. Sticky positioning (positioning.rs + display_list.rs)

1. In `positioning.rs`: store resolved sticky constraints (top/right/bottom/left)
   in layout output alongside the normal-flow position
2. In `display_list.rs`: apply scroll-dependent offset at paint time
3. In `display_list.rs`: sticky always establishes stacking context

### 3c. Stacking contexts (display_list.rs)

Fix `node_establishes_stacking_context()` to also check:
- `position: sticky` (always establishes)
- `opacity < 1.0` (if not already checked)
- `transform` is set (if not already checked)
- `filter` is set (if not already checked)
- `will-change` with layout/paint properties

### 3d. Painting order (display_list.rs)

Implement CSS 2.2 Appendix E painting order:
1. Background/borders of block boxes in tree order
2. Non-positioned floats in tree order
3. In-flow non-inline-level content
4. Non-positioned inline-level content
5. Positioned descendants with z-index: auto or 0
6. Positioned descendants with positive z-index

### 3e. Visibility property (display_list.rs)

Wire `visibility: hidden` to skip painting the element's own content
while still allocating space and painting children.

### 3f. Text-box-trim (cache.rs)

In `position_one_line()`:
- If first line and trim_start: reduce line_ascent by half-leading
- If last line and trim_end: reduce line_descent by half-leading
- Amount based on text-box-edge metric (cap-height, x-height, etc.)

### 3g. Unicode-bidi integration (cache.rs)

At line ~5217, implement unicode-bidi values:
- `embed`: Push directional embedding (LRE/RLE)
- `isolate`: Push directional isolate (LRI/RLI)
- `bidi-override`: Push directional override (LRO/RLO)
- `isolate-override`: Push FSI + override
- `plaintext`: Use paragraph direction from content

### 3h. Fragmentation / pagination (paged_layout.rs, pagination.rs)

Basic fragmentation support for:
- `break-before` / `break-after` (already parsed)
- `orphans` / `widows` (line count constraints)
- Page break avoidance (`break-inside: avoid`)

### 3i. Table collapsing borders (fc.rs + display_list.rs)

Currently only separated border model. Add:
1. Border conflict resolution algorithm (CSS 2.2 §17.6.2)
2. `border-collapse: collapse` support
3. Drawing collapsed borders in display_list.rs

---

## Phase 4: Run Autofix + Codegen

After all CSS property additions:

```bash
# 1. Run autofix until 0 patches
cargo run --release --manifest-path doc/Cargo.toml -- autofix
# Apply patches, repeat until clean

# 2. Run codegen
cargo run --release --manifest-path doc/Cargo.toml -- codegen all

# 3. Verify DLL builds
cargo check -p azul-dll --features build-dll

# 4. Delete old .done/.result files for NOT_APPLICABLE prompts
# (so they get re-run in the next claude-exec)
```

---

## Phase 5: Reset NOT_APPLICABLE Results + Re-run

```bash
# Delete .done and .result for prompts that were NOT_APPLICABLE
# (keep ANNOTATED/FIXED/IMPLEMENTED results)
python3 scripts/reset_na_results.py

# Re-run claude-exec
cargo run --release --manifest-path doc/Cargo.toml -- spec claude-exec --agents=50
```

---

## Implementation Order (by dependency)

1. **Fix agent prompt** (executor.rs) — no deps
2. **Add CSS properties** (2a-2i) — no deps, can parallelize
3. **Run autofix + codegen** — depends on step 2
4. **Implement layout infrastructure** (3a-3i) — depends on step 2-3
5. **Reset N/A results** — depends on step 1-4
6. **Re-run claude-exec** — depends on step 5

Steps 2 and 4 are the bulk of the work. CSS property additions (step 2) are
mechanical and can be done by an implementation agent. Layout infrastructure
(step 4) requires careful implementation but each sub-task is independent.

## Estimated Total Effort

| Phase | Items | Effort |
|-------|-------|--------|
| Fix prompt | 1 | 10 min |
| CSS properties | 9 properties | 2-4 hours |
| Autofix + codegen | 1 | 30 min |
| Layout infra | 9 tasks | 3-5 days |
| Reset + re-run | 1 | 1 hour |

Total: ~4-6 days of implementation work, most of which the agents will do
in the second claude-exec run once the groundwork is in place.
