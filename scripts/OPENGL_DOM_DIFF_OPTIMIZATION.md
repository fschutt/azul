# OpenGL Optimization: DOM Diff Short-Circuit

## Problem Statement

The OpenGL demo (`examples/c/opengl.c`) runs a 16ms animation timer that
returns `Update::RefreshDom`. This triggers a full pipeline every frame:

```
layout() → CSS cascade → flexbox → display list → image callbacks → composite
```

The DOM hasn't actually changed — only `rotation_deg` in the app state
changed. The `layout()` function produces a structurally identical DOM
every frame. We should detect this and skip the expensive steps.

## Root Cause: Why the Diff Doesn't Catch This Today

The reconciliation (`core/src/diff.rs::reconcile_dom`) already runs during
`regenerate_layout()` (in `layout_v2.rs` line ~200). BUT it runs **after**
the layout callback has already been called, and its result is only used
for state migration (transferring datasets, updating managers). The
reconciliation result is **never checked** to see if the DOM is unchanged.

Even if we did check it, there's a deeper problem:

### Image Callback Nodes Cannot Be Matched Across Frames

`ImageRef::hash()` hashes the **heap pointer** (`self.data as usize`).
Every call to `ImageRef::callback(...)` in the `layout()` function
allocates a new `Box<DecodedImage>`, so the pointer is different each
frame. This means:

- `calculate_node_data_hash()` → **different** for the same callback
- `calculate_structural_hash()` → also hashes `img_ref` → **different**
- The reconciliation sees every Image(callback) as:
  "unmount old node + mount new node" (never matched)

Even for non-image nodes (Div, Button, etc.), every field is re-created
via `layout()` and compared by content hash. If those hashes match, the
reconciliation DOES match them. But because Image callback nodes never
match, the diff always shows "something changed."

## Proposed Solution: Structural DOM Comparison After layout()

### Core Insight

The `layout()` callback is a pure function of the app state. If the DOM
structure (node types, hierarchy, classes, IDs, callbacks, styles) hasn't
changed, then the only things that could be different are:
1. Image callback pointers (new allocation, same function)
2. Text content
3. Embedded data (RefAny datasets)

If **only** image callback pointers changed (structurally same callback),
we can skip layout+styling entirely and just re-invoke the image callbacks.

### Approach: "Semantic-equal" DOM comparison

After calling `layout()` and getting the new `StyledDom`, compare it to the
previous frame's `StyledDom` using a **semantic comparison** that treats
image callbacks as opaque/equal if their function pointer and structure
match (ignoring the heap allocation pointer).

```
Timer fires → animate() → returns Update::RefreshDom
  → shell calls layout() → new StyledDom
  → compare new StyledDom vs old StyledDom (semantic equality)
  → IF structurally identical (modulo image callback pointers):
      → SKIP: CSS cascade, flexbox, display list rebuild
      → ONLY: re-invoke image callbacks + composite
  → ELSE:
      → full pipeline (as today)
```

## Detailed Architecture

### Step 1: Semantic DOM Equality Check

Add a function that compares two DOMs while treating Image(Callback)
nodes as "equal if same callback function pointer + same RefAny type":

```rust
// core/src/diff.rs or core/src/styled_dom.rs

/// Check if two StyledDoms are semantically equivalent for layout purposes.
///
/// This compares:
/// - Node count and hierarchy (parent/child/sibling structure)
/// - Node types (treating Image(Callback) nodes as equal if
///   same callback function pointer)
/// - IDs, classes, attributes
/// - Inline CSS properties
/// - Callback event registrations
///
/// This does NOT compare:
/// - Image callback heap pointers (they differ every frame)
/// - Text content (compared separately for incremental text updates)
/// - RefAny datasets (opaque state, not layout-relevant)
///
/// Returns true if the DOMs are structurally identical for layout purposes.
pub fn is_layout_equivalent(old: &StyledDom, new: &StyledDom) -> bool
```

**What to compare:**

| Field | Compare? | Why |
|-------|----------|-----|
| `node_hierarchy` | YES | Structure must match exactly |
| `node_data[].node_type` discriminant | YES | Div must stay Div |
| `node_data[].node_type` content for Text | YES | Text change = possible layout change |
| `node_data[].node_type` content for Image(Raw) | YES | Different image = different intrinsic size |
| `node_data[].node_type` content for Image(Callback) | SKIP | Always differs (heap ptr), never affects layout |
| `node_data[].node_type` content for Image(Gl) | SKIP | Same as callback |
| `node_data[].ids_and_classes` | YES | Affects CSS cascade |
| `node_data[].attributes` | YES | Some affect layout (e.g. colspan, rowspan) |
| `node_data[].callbacks` events | YES | Event registration affects hit-test tags |
| `node_data[].callbacks` function ptrs | NO | Don't affect layout |
| `node_data[].css_props` | YES | Inline styles = direct layout input |
| `node_data[].tab_index` | NO | Doesn't affect layout |
| `node_data[].contenteditable` | NO | Doesn't affect layout |
| `node_data[].dataset` | NO | Opaque user data, not layout-relevant |
| `styled_nodes` | YES | Contains resolved CSS property indices |
| `cascade_info` | YES | CSS cascade order |
| `css_property_cache` | YES | The actual resolved CSS values |

### Step 2: Integrate Into regenerate_layout()

In `dll/src/desktop/shell2/common/layout_v2.rs::regenerate_layout()`:

```rust
// After calling layout() and CSD injection, BEFORE state migration:

// 1. Call layout() → get new_styled_dom (existing code)
let new_styled_dom = ...;

// 2. OPTIMIZATION: Check if DOM is structurally unchanged
if let Some(old_layout_result) = layout_window.layout_results.get(&DomId::ROOT_ID) {
    let old_styled_dom = &old_layout_result.styled_dom;
    
    if is_layout_equivalent(old_styled_dom, &new_styled_dom) {
        // DOM hasn't changed! Skip expensive layout pipeline.
        // But we still need to re-invoke image callbacks since
        // those are the things that actually produce new content.
        
        log_debug!("[regenerate_layout] DOM unchanged - skipping layout, \
                    will only refresh image callbacks");
        
        // Transfer image callback RefAny from new DOM to old DOM's nodes
        // (the old layout_result keeps its positions/sizes/display list)
        transfer_image_callbacks(old_layout_result, &new_styled_dom);
        
        // Mark that image callbacks need re-invocation but layout is cached
        return Ok(LayoutUnchanged);
    }
}

// 3. If DOM changed, proceed with full layout (existing code)
```

### Step 3: Short-Circuit in the Rendering Path

In `macOS/mod.rs::render_and_present_in_draw_rect()`:

Currently:
```rust
if self.frame_needs_regeneration {
    self.regenerate_layout()?;        // full layout
    self.frame_needs_regeneration = false;
}
// ... always builds full WR transaction with display lists
build_webrender_transaction(...);     // rebuilds display lists
```

After optimization:
```rust
if self.frame_needs_regeneration {
    let layout_result = self.regenerate_layout()?;
    self.frame_needs_regeneration = false;
    
    match layout_result {
        LayoutChanged => {
            // Full rebuild path (existing code)
            build_webrender_transaction(...);
        }
        LayoutUnchanged => {
            // Lightweight path: only re-invoke image callbacks + composite
            // - Skip display list rebuild (reuse from previous frame)
            // - Skip font/image resource collection
            // - Only call process_image_callback_updates()
            // - Then generate_frame with display_list_was_rebuilt=false
            let mut txn = WrTransaction::new();
            process_image_callback_updates(layout_window, gl_context, &mut txn);
            txn.skip_scene_builder();
            scroll_all_nodes(layout_window, &mut txn);
            synchronize_gpu_values(layout_window, &mut txn);
            txn.generate_frame(0, WrRenderReasons::empty());
            render_api.send_transaction(doc_id, txn);
        }
    }
}
```

### Step 4: Fix Image Callback Hashing for Better Reconciliation

Currently `calculate_structural_hash()` hashes the `ImageRef` (pointer),
which makes Image(Callback) nodes unmatchable. Fix this by using the
callback's **function pointer** (`cb` field, which is a `usize`) and the
**RefAny type ID** instead:

```rust
// In calculate_structural_hash():
// BEFORE:
if let NodeType::Image(ref img_ref) = self.node_type {
    img_ref.hash(&mut hasher);  // hashes heap pointer - BAD
}

// AFTER:
if let NodeType::Image(ref img_ref) = self.node_type {
    match img_ref.get_data() {
        DecodedImage::Callback(cb) => {
            // Hash function pointer (stable across frames)
            cb.callback.cb.hash(&mut hasher);
            // Hash RefAny type ID (not instance pointer)
            cb.refany.get_type_id().hash(&mut hasher);
        }
        _ => {
            // Raw images / GL textures: hash normally
            img_ref.hash(&mut hasher);
        }
    }
}
```

This allows `reconcile_dom` to correctly match Image(Callback) nodes
across frames, which also fixes state migration for such nodes.

## What Gets Compared in `is_layout_equivalent`

The function needs to be fast — it runs every frame. But it only runs when
`RefreshDom` is returned, so it replaces the expensive layout+styling
pipeline (which is much slower).

**Comparison strategy** (early-exit on first difference):

1. **Node count**: `old.node_data.len() == new.node_data.len()`
   → O(1), immediately eliminates structural changes

2. **Hierarchy**: `old.node_hierarchy == new.node_hierarchy`  
   → O(n) memcmp of parent/child/sibling arrays

3. **Per-node type + structure**: For each node pair:
   - `discriminant(old_type) == discriminant(new_type)`
   - For Text nodes: `old_text == new_text` (text change = possible size change)
   - For Image(Callback): compare function pointer + type_id (not heap ptr)
   - For all other types: discriminant match is sufficient

4. **CSS properties**: Compare the inline css_props arrays
   - Either via hash comparison or direct content comparison
   - These are the primary layout inputs

5. **IDs and classes**: `old.ids_and_classes == new.ids_and_classes`
   → Affects CSS selector matching

6. **StyledNodes + CssPropertyCache**: If the above all match and CSS
   hasn't changed, the styled nodes will be identical. Could optionally
   compare just the `css_property_cache` pointer or hash.

**Expected performance**: For a DOM with ~50 nodes (typical OpenGL demo),
this is a few hundred nanoseconds of comparison vs. ~5ms of full layout.
Even for 1000-node DOMs, the comparison is O(n) by node count, while full
layout is O(n log n) or worse with flexbox.

## File Change Map

| File | Change | Effort |
|------|--------|--------|
| `core/src/styled_dom.rs` or `core/src/diff.rs` | Add `is_layout_equivalent()` | Medium |
| `core/src/dom.rs` | Fix `calculate_structural_hash()` for Image(Callback) | Small |
| `dll/src/desktop/shell2/common/layout_v2.rs` | Short-circuit in `regenerate_layout()` | Medium |
| `dll/src/desktop/shell2/macos/mod.rs` | Handle `LayoutUnchanged` in render path | Medium |
| `dll/src/desktop/shell2/windows/mod.rs` | Same as macOS | Medium |
| `dll/src/desktop/shell2/linux/x11/mod.rs` | Same as macOS | Medium |
| `dll/src/desktop/shell2/linux/wayland/mod.rs` | Same as macOS | Medium |
| `dll/src/desktop/wr_translate2.rs` | Already has `display_list_was_rebuilt=false` path | None |
| `layout/src/window.rs` | Return enum from `layout_and_generate_display_list` | Small |

## What Goes Through the Pipeline in Each Case

### Case 1: DOM Changed (full pipeline, as today)

```
layout() → CSD → reconcile → state migration
  → manager updates → runtime states → font loading
  → flexbox layout → display list → build WR txn
  → image callbacks → scroll → GPU sync
  → generate_frame → send txn → composite
```
**Cost**: ~5–15ms depending on DOM complexity

### Case 2: DOM Unchanged (optimized path)

```
layout() → CSD → is_layout_equivalent() → YES
  → transfer image callback RefAnys to old DOM
  → image callbacks → scroll → GPU sync
  → generate_frame (skip scene builder) → send txn → composite
```
**Cost**: ~0.1–0.5ms (comparison + image callback + composite)

### Case 3: Only text changed (future optimization)

```
layout() → CSD → is_layout_equivalent() → NO (text differs)
  → but could do incremental text relayout only
  → (not in scope for this change)
```

## CSS Property Cache Comparison

The `css_property_cache` is the resolved set of CSS property values after
the cascade. If the DOM structure, classes, IDs, and inline styles are all
the same, the cascade result will be identical. We have two options:

**Option A: Skip CSS comparison entirely**  
If hierarchy + node types + ids/classes + inline css_props all match,
the cascade MUST produce the same result (CSS is deterministic). So we
can skip comparing `css_property_cache` and `styled_nodes`. This is the
most efficient approach.

**Option B: Hash the CSS property cache**  
Compute a hash of the old and new CSS property caches and compare. More
defensive but adds overhead.

**Recommendation: Option A** — if all inputs to the cascade match, the
output must match. The CSS engine is a pure function of its inputs.

## Edge Cases and Safety

1. **Window resize**: Resize events go through a different path (not
   `RefreshDom`). They directly trigger relayout. The optimization only
   applies when the timer returns `RefreshDom`.

2. **Hover/focus/active states**: `apply_runtime_states_before_layout()`
   runs BEFORE layout. If hover state changed but DOM structure didn't,
   the structural comparison will still pass (hover doesn't change node
   structure). But the CSS property cache might differ (`:hover` styles).
   
   **Solution**: The comparison runs AFTER `apply_runtime_states_before_layout`
   succeeds, and compares styled_nodes states (hover/focus/active flags).
   If those differ → fall through to full layout.
   
   **Alternative**: Since the timer callback is the one returning
   `RefreshDom` (not a mouse event), hover/focus/active states won't
   have changed between timer ticks. Only `process_callback_result_v2`
   changes those, and that runs separately from the timer path.

3. **IFrame content**: If an IFrame's content changed but the parent DOM
   didn't, `is_layout_equivalent` compares only the parent DOM. IFrame
   layout is separate. This is correct — IFrame relayout is independent.

4. **First frame**: No previous DOM to compare against → always full layout.
   The `layout_results.get(&ROOT_ID)` check handles this.

5. **CSD injection**: Both old and new DOMs go through CSD injection
   before comparison. This ensures structural parity.

## Future: `Update` as Bitflags

The current `Update` enum will later be changed to bitflags for composability:

```rust
bitflags! {
    pub struct Update: u32 {
        const NOTHING            = 0b0000_0000;
        const REFRESH_DOM        = 0b0000_0001;
        const REFRESH_ALL        = 0b0000_0010;
        const REFRESH_IMAGES     = 0b0000_0100;  // future: explicit image-only
        const REFRESH_SCROLL     = 0b0000_1000;  // future: scroll-only
        const REFRESH_GPU_VALUES = 0b0001_0000;  // future: opacity/transform
    }
}
```

The DOM-diff optimization proposed here is **orthogonal** to the bitflags
change — it makes the "worst case" (`REFRESH_DOM`) efficient by detecting
that the DOM didn't actually change. The bitflags would allow users to
explicitly request lighter updates, while the diff catches the case where
the user requests a heavy update but it turns out to be unnecessary.

## Testing Strategy

1. **OpenGL demo**: Run `opengl.c`, monitor CPU. Should drop from ~50% to ~5%.
2. **Button click while animating**: Click the button during animation.
   DOM changes (hover state) → full layout path should trigger.
3. **Window resize during animation**: Resize should work correctly
   (goes through separate relayout path).
4. **Multiple image callbacks**: Test with two OpenGL viewports — both
   should re-render.
5. **Text input while animating**: If a text input is present and the
   user types, DOM changes → full layout. Image callbacks still run.
6. **Performance test**: Measure time in `regenerate_layout()` before
   and after. The comparison should take <1ms even for large DOMs.

## Implementation Priority

1. **Fix `calculate_structural_hash()` for Image(Callback)** — smallest
   change, improves reconciliation accuracy independent of the optimization.
2. **Add `is_layout_equivalent()`** — the core comparison function.
3. **Short-circuit in `regenerate_layout()`** — skip layout when DOM unchanged.
4. **Handle `LayoutUnchanged` in shell render paths** — skip display list rebuild.
5. **Test on all platforms** — macOS, Windows, X11, Wayland.
