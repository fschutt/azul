# DOM Change Report Architecture

## Goal

Replace the binary "changed / unchanged" layout decision with a **granular
per-node change report** that tells each downstream stage (CSS restyle, layout,
text shaping, display list) exactly what changed, so it can do the **minimum
possible work**.

### User-facing benefit

| User action | Today | After this work |
|---|---|---|
| GL texture animation (timer) | Full DOM rebuild + relayout every frame | Image-only WR transaction |
| Typing a character | Full relayout of entire page | Reshape one word + relayout one IFC |
| Changing CSS color via toggle | Full relayout | Paint-only, skip layout |
| Hovering a button | Full relayout | Restyle + repaint, no layout if only color |
| Unchanged timer tick | `is_layout_equivalent` short-circuit | Same, but now with proper `is_clean()` |

---

## Current Architecture (before this work)

```
User layout_callback
       │
       ▼
   StyledDom (new)
       │
       ├──▶ diff::reconcile_dom()  ──▶  DiffResult { events, node_moves }
       │                                   (only lifecycle events + state migration)
       │
       ├──▶ is_layout_equivalent()  ──▶  bool  (binary: skip or full layout)
       │
       └──▶ layout_document()
              │
              ├── reconcile_and_invalidate()
              │       compares hash_styled_node_data() per node
              │       produces ReconciliationResult { intrinsic_dirty, layout_roots }
              │       (binary per node: dirty or clean)
              │
              ├── Early exit if is_clean()
              │
              ├── calculate_intrinsic_sizes() for dirty nodes
              ├── calculate_layout_for_subtree() for layout_roots
              │
              └── generate_display_list() (always regenerated)
```

### Problems

1. `hash_styled_node_data()` hashes `node_type + styled_node_state` into ONE u64.
   If the hash differs → the node is "dirty" → full intrinsic + layout + display
   list rebuild for its subtree. No distinction between text change vs style change
   vs structural change.

2. `is_layout_equivalent()` is a separate, all-or-nothing check that runs *before*
   `layout_document()`. If it returns false, we fall through to full layout. There
   is no way to say "only this one node changed".

3. `reconcile_dom()` returns `DiffResult { events, node_moves }` but does NOT
   tell us **what** changed per node. It only identifies which old nodes map to
   which new nodes.

4. The display list is always fully regenerated (no incremental display list).

---

## Proposed Architecture

```
User layout_callback
       │
       ▼
   StyledDom (new)
       │
       ├──▶ diff::reconcile_dom_with_changes()          ◄── NEW
       │       produces ExtendedDiffResult {
       │           events, node_moves,
       │           per_node_changes: Vec<(NodeId, NodeId, NodeChangeSet)>  ◄── NEW
       │       }
       │
       └──▶ layout_document_with_changes()               ◄── MODIFIED
              │
              ├── reconcile_and_invalidate_with_changes() ◄── MODIFIED
              │       uses NodeChangeSet to set DirtyFlag::None / Paint / Layout
              │       per node (instead of binary dirty/clean)
              │
              ├── Early exit if is_clean()
              │
              ├── calculate_intrinsic_sizes() ONLY for Layout-dirty nodes
              ├── calculate_layout_for_subtree() ONLY for Layout-dirty subtrees
              │
              ├── For Paint-only nodes: skip layout, only update display list entries
              │
              └── generate_display_list()
                    (future: incremental display list for Paint-only changes)
```

---

## Data Structures

### `NodeChangeSet` (new, in `core/src/diff.rs`)

```rust
use bitflags::bitflags;

bitflags! {
    /// Bit flags describing what changed about a node between old and new DOM.
    /// Multiple flags can be set simultaneously.
    ///
    /// The flags are ordered by "severity" — downstream code can check:
    /// - `intersects(AFFECTS_LAYOUT)` → need relayout
    /// - `intersects(AFFECTS_PAINT)` → need repaint  
    /// - `is_empty()` → nothing changed
    pub struct NodeChangeSet: u32 {
        // --- Changes that affect LAYOUT (need relayout + repaint) ---

        /// Node type changed entirely (e.g., Text → Image).
        /// Always requires full relayout.
        const NODE_TYPE_CHANGED     = 0b0000_0000_0000_0001;

        /// Text content changed (for Text nodes).
        /// Requires text reshaping + IFC relayout.
        const TEXT_CONTENT          = 0b0000_0000_0000_0010;

        /// CSS IDs or classes changed.
        /// May cause restyle → relayout if affected properties include layout props.
        const IDS_AND_CLASSES       = 0b0000_0000_0000_0100;

        /// Inline CSS properties changed that affect layout
        /// (width, height, margin, padding, display, position, flex-*, etc.)
        const INLINE_STYLE_LAYOUT   = 0b0000_0000_0000_1000;

        /// Children added, removed, or reordered.
        const CHILDREN_CHANGED      = 0b0000_0000_0001_0000;

        /// Image source changed (may affect intrinsic size).
        const IMAGE_CHANGED         = 0b0000_0000_0010_0000;

        /// Contenteditable flag changed.
        const CONTENTEDITABLE       = 0b0000_0000_0100_0000;

        /// Tab index changed.
        const TAB_INDEX             = 0b0000_0000_1000_0000;

        // --- Changes that affect PAINT only (no relayout needed) ---

        /// Inline CSS properties changed that affect paint only
        /// (color, background-color, border-color, opacity, etc.)
        const INLINE_STYLE_PAINT    = 0b0000_0001_0000_0000;

        /// Styled node state changed (hover, active, focus, etc.)
        /// This may affect paint (e.g., :hover color) but typically not layout.
        const STYLED_STATE          = 0b0000_0010_0000_0000;

        // --- Changes that affect NEITHER layout nor paint ---

        /// Callbacks changed (new RefAny, different event handlers).
        /// Does not affect visual output at all.
        const CALLBACKS             = 0b0000_0100_0000_0000;

        /// Dataset changed. Does not affect visual output.
        const DATASET               = 0b0000_1000_0000_0000;

        /// Accessibility info changed. Does not affect visual output.
        const ACCESSIBILITY         = 0b0001_0000_0000_0000;

        // --- Composite masks for quick checks ---

        /// Any change that requires a layout pass.
        const AFFECTS_LAYOUT = Self::NODE_TYPE_CHANGED.bits()
                             | Self::TEXT_CONTENT.bits()
                             | Self::IDS_AND_CLASSES.bits()
                             | Self::INLINE_STYLE_LAYOUT.bits()
                             | Self::CHILDREN_CHANGED.bits()
                             | Self::IMAGE_CHANGED.bits()
                             | Self::CONTENTEDITABLE.bits();

        /// Any change that requires a paint/display-list update (but not layout).
        const AFFECTS_PAINT = Self::INLINE_STYLE_PAINT.bits()
                            | Self::STYLED_STATE.bits();
    }
}

impl NodeChangeSet {
    /// Returns the appropriate DirtyFlag for this change set.
    pub fn to_dirty_flag(&self) -> DirtyFlag {
        if self.intersects(Self::AFFECTS_LAYOUT) {
            DirtyFlag::Layout
        } else if self.intersects(Self::AFFECTS_PAINT) {
            DirtyFlag::Paint
        } else {
            DirtyFlag::None
        }
    }

    /// Returns true if no visual change occurred (only callbacks/dataset/a11y).
    pub fn is_visually_unchanged(&self) -> bool {
        !self.intersects(Self::AFFECTS_LAYOUT | Self::AFFECTS_PAINT)
    }
}
```

### `ExtendedDiffResult` (new, in `core/src/diff.rs`)

```rust
/// Extended diff result that includes per-node change information.
#[derive(Debug, Clone)]
pub struct ExtendedDiffResult {
    /// Original diff result (lifecycle events + node moves).
    pub diff: DiffResult,

    /// Per-node change report for matched nodes.
    /// Each entry: (old_node_id, new_node_id, what_changed).
    /// Only contains entries for nodes that were matched (moved),
    /// not for newly mounted or unmounted nodes.
    pub node_changes: Vec<(NodeId, NodeId, NodeChangeSet)>,
}
```

---

## Implementation Plan

### Phase 1: Per-field hashing in `diff.rs` (core change)

**File: `core/src/diff.rs`**

Add a function that compares two matched `NodeData` instances field-by-field
and returns a `NodeChangeSet`:

```rust
/// Compare two matched NodeData instances and determine what changed.
pub fn compute_node_changes(
    old_node: &NodeData,
    new_node: &NodeData,
    old_styled_state: Option<&StyledNodeState>,
    new_styled_state: Option<&StyledNodeState>,
) -> NodeChangeSet {
    let mut changes = NodeChangeSet::empty();

    // 1. Node type discriminant
    if core::mem::discriminant(&old_node.node_type)
        != core::mem::discriminant(&new_node.node_type)
    {
        changes |= NodeChangeSet::NODE_TYPE_CHANGED;
        return changes; // If node type changed, everything changed
    }

    // 2. Text content (only for Text nodes)
    match (&old_node.node_type, &new_node.node_type) {
        (NodeType::Text(old_text), NodeType::Text(new_text)) => {
            if old_text.as_str() != new_text.as_str() {
                changes |= NodeChangeSet::TEXT_CONTENT;
            }
        }
        (NodeType::Image(old_img), NodeType::Image(new_img)) => {
            if !old_img.is_same_image(new_img) {
                changes |= NodeChangeSet::IMAGE_CHANGED;
            }
        }
        _ => {} // Same discriminant, handle other types as needed
    }

    // 3. IDs and classes
    if old_node.ids_and_classes.as_ref() != new_node.ids_and_classes.as_ref() {
        changes |= NodeChangeSet::IDS_AND_CLASSES;
    }

    // 4. Inline CSS props (distinguish layout vs paint)
    let (layout_changed, paint_changed) = compare_css_props(
        old_node.css_props.as_ref(),
        new_node.css_props.as_ref(),
    );
    if layout_changed {
        changes |= NodeChangeSet::INLINE_STYLE_LAYOUT;
    }
    if paint_changed {
        changes |= NodeChangeSet::INLINE_STYLE_PAINT;
    }

    // 5. Callbacks
    if !callbacks_equal(&old_node.callbacks, &new_node.callbacks) {
        changes |= NodeChangeSet::CALLBACKS;
    }

    // 6. Dataset
    if old_node.dataset != new_node.dataset {
        changes |= NodeChangeSet::DATASET;
    }

    // 7. Contenteditable
    if old_node.contenteditable != new_node.contenteditable {
        changes |= NodeChangeSet::CONTENTEDITABLE;
    }

    // 8. Tab index
    if old_node.tab_index != new_node.tab_index {
        changes |= NodeChangeSet::TAB_INDEX;
    }

    // 9. Styled node state (hover, active, focused, etc.)
    if old_styled_state != new_styled_state {
        changes |= NodeChangeSet::STYLED_STATE;
    }

    changes
}
```

### Phase 2: CSS property classification

**File: `css/src/props/property.rs`** (or new helper module)

Add a method to classify CSS properties as layout-affecting vs paint-only:

```rust
impl CssPropertyType {
    /// Returns true if changing this property requires a layout recalculation.
    /// Returns false if the change only affects painting (color, opacity, etc.)
    pub fn affects_layout(&self) -> bool {
        match self {
            // Paint-only properties
            CssPropertyType::TextColor
            | CssPropertyType::CaretColor
            | CssPropertyType::SelectionBackgroundColor
            | CssPropertyType::SelectionColor
            | CssPropertyType::TextDecoration
            | CssPropertyType::Cursor
            | CssPropertyType::UserSelect
            | CssPropertyType::Opacity
            | CssPropertyType::BackgroundContent
            | CssPropertyType::BackgroundPosition
            | CssPropertyType::BackgroundSize
            | CssPropertyType::BackgroundRepeat
            | CssPropertyType::BorderTopColor
            | CssPropertyType::BorderRightColor
            | CssPropertyType::BorderBottomColor
            | CssPropertyType::BorderLeftColor
            | CssPropertyType::BoxShadowLeft
            | CssPropertyType::BoxShadowRight
            | CssPropertyType::BoxShadowTop
            | CssPropertyType::BoxShadowBottom
            | CssPropertyType::MixBlendMode
            | CssPropertyType::Filter
            | CssPropertyType::BackdropFilter
            | CssPropertyType::Transform
            | CssPropertyType::TransformOrigin
            => false,

            // Everything else affects layout
            _ => true,
        }
    }
}
```

### Phase 3: Integrate into `reconcile_dom()`

**File: `core/src/diff.rs`**

Modify `reconcile_dom()` to also produce `ExtendedDiffResult`:

```rust
pub fn reconcile_dom_with_changes(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_styled_nodes: Option<&[StyledNode]>,  // NEW
    new_styled_nodes: Option<&[StyledNode]>,  // NEW
    old_layout_map: &FastHashMap<DomNodeId, LogicalRect>,
    new_layout_map: &FastHashMap<DomNodeId, LogicalRect>,
    dom_id: DomId,
    now: Instant,
) -> ExtendedDiffResult {
    // ... existing reconciliation logic (steps 1-4) ...

    // After step 3 (matching), for each NodeMove, compute changes
    let mut node_changes = Vec::new();
    for node_move in &diff_result.node_moves {
        let old_nd = &old_node_data[node_move.old_node_id.index()];
        let new_nd = &new_node_data[node_move.new_node_id.index()];

        let old_state = old_styled_nodes.and_then(|s| s.get(node_move.old_node_id.index()))
            .map(|s| &s.styled_node_state);
        let new_state = new_styled_nodes.and_then(|s| s.get(node_move.new_node_id.index()))
            .map(|s| &s.styled_node_state);

        let changes = compute_node_changes(old_nd, new_nd, old_state, new_state);
        if !changes.is_empty() {
            node_changes.push((
                node_move.old_node_id,
                node_move.new_node_id,
                changes,
            ));
        }
    }

    ExtendedDiffResult {
        diff: diff_result,
        node_changes,
    }
}
```

### Phase 4: Pass changes through to layout

**File: `dll/src/desktop/shell2/common/layout_v2.rs`**

In `regenerate_layout()`, replace `is_layout_equivalent()` with the granular
change report:

```rust
// Instead of:
//   if is_layout_equivalent(&old.styled_dom, &styled_dom) { ... }
// Do:
let extended_diff = reconcile_dom_with_changes(
    &old_node_data, &new_node_data,
    Some(old_styled_nodes_ref), Some(new_styled_nodes_ref),
    &old_layout_map, &new_layout_map,
    dom_id, now,
);

// Quick check: if ALL matched nodes have empty change sets, skip layout entirely
if extended_diff.node_changes.is_empty()
    && extended_diff.diff.events.iter().all(|e| !e.is_mount_or_unmount())
{
    // Equivalent to is_layout_equivalent() → LayoutUnchanged
    // ... transfer image callbacks, return LayoutUnchanged ...
}

// Otherwise, pass change report into layout_document
let display_list = layout_document_with_changes(
    &mut cache,
    &mut text_cache,
    styled_dom,
    viewport,
    &font_manager,
    &scroll_offsets,
    &selections,
    &text_selections,
    debug_messages,
    gpu_value_cache,
    renderer_resources,
    id_namespace,
    dom_id,
    cursor_is_visible,
    cursor_location,
    system_style,
    get_system_time_fn,
    &extended_diff,  // NEW PARAMETER
)?;
```

### Phase 5: Use changes in `reconcile_and_invalidate()`

**File: `layout/src/solver3/cache.rs`**

Modify `reconcile_recursive()` to use `NodeChangeSet` instead of binary hash comparison:

```rust
// Before:
let is_dirty = old_node.map_or(true, |n| new_node_data_hash != n.node_data_hash);

// After:
let (is_dirty, dirty_flag) = if let Some(old) = old_node {
    // Check if we have a pre-computed NodeChangeSet from the DOM-level diff
    if let Some(change_set) = node_changes_map.get(&new_dom_id) {
        let flag = change_set.to_dirty_flag();
        (flag != DirtyFlag::None, flag)
    } else {
        // Fallback: hash comparison
        let dirty = new_node_data_hash != old.node_data_hash;
        (dirty, if dirty { DirtyFlag::Layout } else { DirtyFlag::None })
    }
} else {
    (true, DirtyFlag::Layout) // New node → always dirty
};
```

When `dirty_flag == DirtyFlag::Paint`, the node is added to a new
`paint_dirty` set (not `intrinsic_dirty` or `layout_roots`), which
means only the display list for that node gets regenerated — no
intrinsic size recalculation or layout pass.

### Phase 6: Text-level granularity (future)

For `TEXT_CONTENT` changes, we can extract the edit range by comparing
old and new text strings:

```rust
fn compute_text_edit_range(old: &str, new: &str) -> (usize, usize, usize) {
    // Find common prefix
    let prefix_len = old.bytes().zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();

    // Find common suffix (from the non-prefix part)
    let old_rest = &old[prefix_len..];
    let new_rest = &new[prefix_len..];
    let suffix_len = old_rest.bytes().rev().zip(new_rest.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();

    (prefix_len, old.len() - suffix_len, new.len() - suffix_len)
}
```

This edit range can be passed into the text shaping pipeline so only
the affected word(s) are reshaped. The `LayoutCache` already caches
per-stage results keyed by content hash — if we only invalidate the
cache entry for the changed word, the other words' shaped glyphs are
reused automatically.

---

## Integration with Existing Infrastructure

### What already works (no changes needed)

| Component | Why it works |
|---|---|
| `LayoutCache` 4-stage text pipeline | Already caches by content hash — unchanged text runs are reused |
| `LayoutNode.taffy_cache` | Already per-node — clean nodes keep their cache |
| `ReconciliationResult.is_clean()` early exit | Still works — if no changes, return cached display list |
| `reposition_clean_subtrees()` | Still works — shifts clean siblings without relayout |
| `SubtreeHash` propagation | Still works — only dirty subtrees get new hashes |

### What needs modification

| Component | Change | Effort |
|---|---|---|
| `diff::reconcile_dom()` | Add `compute_node_changes()` per matched pair | Medium |
| `CssPropertyType` | Add `affects_layout()` method | Small |
| `regenerate_layout()` | Replace `is_layout_equivalent()` with `ExtendedDiffResult` | Medium |
| `reconcile_and_invalidate()` | Accept `NodeChangeSet` map, set `DirtyFlag::Paint` vs `Layout` | Medium |
| `ReconciliationResult` | Add `paint_dirty: BTreeSet<usize>` | Small |
| `layout_document()` | Handle `paint_dirty` nodes (skip layout, regenerate display list entries) | Medium |

### What is deferred (future phases)

| Component | Change |
|---|---|
| Incremental display list | Only regenerate entries for paint-dirty nodes |
| Text edit range extraction | Use `compute_text_edit_range()` for partial reshaping |
| IFC incremental relayout | Use `InlineItemMetrics` for per-word dirty checking |
| GPU value cache | Map `NodeChangeSet` to selective GPU cache invalidation |

---

## Files Modified (Phase 1-5)

```
core/src/diff.rs                          — NodeChangeSet bitflags, compute_node_changes(),
                                            ExtendedDiffResult, reconcile_dom_with_changes()
css/src/props/property.rs                 — CssPropertyType::affects_layout()
layout/src/solver3/cache.rs               — reconcile_and_invalidate() accepts change map,
                                            ReconciliationResult gets paint_dirty set
layout/src/solver3/mod.rs                 — layout_document() handles paint_dirty nodes
dll/src/desktop/shell2/common/layout_v2.rs — regenerate_layout() uses ExtendedDiffResult
core/src/styled_dom.rs                    — Remove is_layout_equivalent() (superseded)
```

---

## Correctness Guarantees

1. **Fallback safety**: If the `NodeChangeSet` is not available for a node
   (new node, no match), it defaults to `DirtyFlag::Layout` — same as today.

2. **Conservative classification**: `IDS_AND_CLASSES` is classified as
   `AFFECTS_LAYOUT` even though some class changes might be paint-only.
   This is correct because class changes trigger `restyle()` which may
   add layout-affecting properties. A future optimization could run a
   quick restyle check to see if the actual affected properties are
   paint-only.

3. **Hash as verification**: Even with `NodeChangeSet`, the layout
   `hash_styled_node_data()` can serve as a double-check in debug builds.

4. **No display list regression**: Until incremental display list is
   implemented, `generate_display_list()` is always called for the full
   tree. The optimization comes from skipping layout passes for
   paint-only or unchanged nodes. The display list generator reads
   from `calculated_positions` which are correct since only dirty
   subtrees were relaid out.

---

## Performance Impact

| Scenario | Nodes | Today | After Phase 5 |
|---|---|---|---|
| GL callback animation (100 nodes) | 100 | Full layout | 0 layout (IMAGE_CHANGED → update WR image only) |
| Single char typed (1000 node page) | 1 dirty | Full layout of 1000 | Reshape 1 word + relayout 1 IFC |
| CSS color hover (500 nodes) | 1 dirty | Full layout of 500 | Paint-only for 1 node |
| No change (timer tick) | 0 dirty | `is_clean()` early exit | Same |
| Structural change (add 10 nodes) | 10 new | Full layout | Full layout (no change) |
