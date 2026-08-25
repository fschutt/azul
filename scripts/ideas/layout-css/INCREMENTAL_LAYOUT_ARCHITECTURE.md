# Incremental Layout Architecture

## Goal

Connect the existing granular change-detection infrastructure—`RelayoutScope`,
`RestyleResult`, `DirtyFlag`, `mark_dirty()`—to the layout pipeline so the
engine does the **minimum possible work** on each frame.

Today these systems are disconnected: the restyle path computes per-node,
per-property change info, then throws it away; the layout path uses binary
hash comparison (dirty vs clean). This plan bridges that gap.

### User-facing benefit

| Scenario | Today | After |
|---|---|---|
| GL texture animation (timer) | Full DOM rebuild + relayout | Image-only WR transaction (no layout) |
| Typing a character | Full DOM rebuild + relayout | Reshape one IFC, relayout one subtree |
| Changing CSS color via hover | Full DOM rebuild + relayout | Restyle + repaint, skip layout |
| Hovering a button (no CSS change) | Full DOM rebuild | `is_clean()` fast path |
| Unchanged timer tick | `is_layout_equivalent()` short-circuit | Same |
| Runtime `set_css_property()` of color | Ignored (dead field) | Paint-only, no layout |
| Runtime `set_node_text()` | Ignored (dead field) | Reshape IFC, relayout locally |

---

## Inventory of Existing Infrastructure

### Already built — just not wired together

| Component | Location | What it provides |
|---|---|---|
| `RelayoutScope` (4-level) | [property.rs:695](css/src/props/property.rs#L695) | `None` / `IfcOnly` / `SizingOnly` / `Full` per CSS property |
| `CssPropertyType::relayout_scope()` | [property.rs:1195](css/src/props/property.rs#L1195) | Classifies every CSS property into one of 4 scopes |
| `CssPropertyType::is_gpu_only_property()` | [property.rs:1175](css/src/props/property.rs#L1175) | `opacity` and `transform` → GPU fast path |
| `RestyleResult` | [styled_dom.rs:129](core/src/styled_dom.rs#L129) | `changed_nodes`, `max_relayout_scope`, `needs_layout`, `gpu_only_changes` |
| `restyle_on_state_change()` | [styled_dom.rs:1770](core/src/styled_dom.rs#L1770) | Computes per-node `ChangedCssProperty` for hover/focus/active |
| `DirtyFlag` (3-level) | [layout_tree.rs:136](layout/src/solver3/layout_tree.rs#L136) | `None` / `Paint` / `Layout` on each `LayoutNode` |
| `mark_dirty(flag)` | [layout_tree.rs:637](layout/src/solver3/layout_tree.rs#L637) | Marks node + ancestors, severity-based early stop |
| `mark_subtree_dirty(flag)` | [layout_tree.rs:665](layout/src/solver3/layout_tree.rs#L665) | Marks entire subtree (for inherited CSS) |
| `ReconciliationResult` | [cache.rs:369](layout/src/solver3/cache.rs#L369) | `intrinsic_dirty: BTreeSet<usize>`, `layout_roots: BTreeSet<usize>` |
| `reconcile_and_invalidate()` | [cache.rs:649](layout/src/solver3/cache.rs#L649) | Builds new `LayoutTree`, marks dirty nodes via hash comparison |
| `reconcile_recursive()` | [cache.rs:761](layout/src/solver3/cache.rs#L761) | Per-node: `is_dirty = old_hash != new_hash` (binary) |
| `hash_styled_node_data()` | [cache.rs:2082](layout/src/solver3/cache.rs#L2082) | Hashes `styled_node_state + node_type` → single u64 |
| `CachedInlineLayout` with `InlineItemMetrics` | [layout_tree.rs:199](layout/src/solver3/layout_tree.rs#L199) | Per-IFC shaped text cache, supports incremental reshaping |
| `reposition_clean_subtrees()` | [cache.rs:395](layout/src/solver3/cache.rs#L395) | Shifts clean siblings without relayout |
| `ProcessEventResult` (6-level) | [events.rs:61](core/src/events.rs#L61) | `DoNothing` → `ReRender` → `UpdateDisplayList` → `RegenerateDom` |
| `CallbackChangeResult` | [window.rs:305](layout/src/window.rs#L305) | `words_changed`, `images_changed`, `css_properties_changed` per callback |
| `reconcile_dom()` (4-step) | [diff.rs:430](core/src/diff.rs#L430) | Key/hash/structural matching, produces `DiffResult { events, node_moves }` |
| `NodeChangeSet` (already added) | [diff.rs:37](core/src/diff.rs#L37) | Bitflags for per-node change classification |
| `compute_node_changes()` (already added) | [diff.rs:168](core/src/diff.rs#L168) | Field-by-field comparison of two `NodeData` |
| `transfer_states()` | [diff.rs:757](core/src/diff.rs#L757) | Migrates heavy resources (GPU handles) via merge callbacks |
| `create_migration_map()` | [diff.rs:727](core/src/diff.rs#L727) | `BTreeMap<OldNodeId, NewNodeId>` from `node_moves` |

### State preserved across DOM rebuilds

| State | Manager | File | Remap method |
|---|---|---|---|
| Focus | `FocusManager` | [focus_cursor.rs:55](layout/src/managers/focus_cursor.rs#L55) | `remap_pending_focus_node_ids()` |
| Text cursor | `CursorManager` | [cursor.rs:68](layout/src/managers/cursor.rs#L68) | `remap_node_ids()` + stable `contenteditable_key` |
| Text selection | `SelectionManager` | [selection.rs:50](layout/src/managers/selection.rs#L50) | `remap_node_ids()` |
| Scroll offset + animation | `ScrollManager` | [scroll_state.rs:290](layout/src/managers/scroll_state.rs#L290) | `remap_node_ids()` — offsets survive rebuilds |
| Hover/active | `HoverManager` | various | `remap_node_ids()` |
| Drag | `GestureAndDragManager` | various | `remap_node_ids()` |

All managers are remapped via `update_managers_with_node_moves()` in
[layout_v2.rs:645](dll/src/desktop/shell2/common/layout_v2.rs#L645).

### Dead fields (computed but never consumed)

| Field | Where it's set | Problem |
|---|---|---|
| `CallbackChangeResult.words_changed` | [window.rs:1937](layout/src/window.rs#L1937) | Set by `CallbackChange::ChangeNodeText`, never consumed in shell |
| `CallbackChangeResult.css_properties_changed` | [window.rs:1985](layout/src/window.rs#L1985) | Set by `CallbackChange::ChangeNodeCssProperties`, never consumed |
| `RestyleResult.changed_nodes` | [styled_dom.rs:1770](core/src/styled_dom.rs#L1770) | Computed, then mapped to binary `ProcessEventResult` |
| `RestyleResult.max_relayout_scope` | same | Computed but unused by layout engine |

---

## The Critical Gap

### Two parallel systems that don't talk:

```
RESTYLE PATH (event_v2)                    LAYOUT PATH (solver3)
─────────────────────                      ────────────────────
hover/focus/active change                  User's layout_callback() → new StyledDom
       │                                          │
       ▼                                          ▼
restyle_on_state_change()                  reconcile_and_invalidate()
       │                                          │
       ▼                                          ▼
RestyleResult {                            ReconciliationResult {
  changed_nodes: per-node props              intrinsic_dirty: BTreeSet
  max_relayout_scope: 4-level                layout_roots: BTreeSet
  gpu_only_changes: bool                   }  ← binary dirty/clean only
}                                                 │
       │                                          ▼
       ▼                                   layout_document()
ProcessEventResult::                              │
  ShouldRegenerateDom                      DirtyFlag::Layout only
       │                                   (::Paint never set)
       ▼
FULL DOM REBUILD
(calls user's layout_callback again!)
(restyle info discarded)
```

**Result**: The engine has 4-level granularity (`RelayoutScope`) and 3-level
dirty flags (`DirtyFlag::None/Paint/Layout`) but always falls back to binary
"everything dirty" or "nothing dirty".

---

## Proposed Architecture

### Three change input paths, unified into one pipeline:

```
 ┌─────────────────────┐  ┌──────────────────────┐  ┌────────────────────────┐
 │ PATH A: DOM Rebuild │  │ PATH B: Restyle      │  │ PATH C: Runtime Edits  │
 │ (layout_callback)   │  │ (hover/focus/active)  │  │ (set_text/set_css)     │
 └──────────┬──────────┘  └──────────┬───────────┘  └──────────┬─────────────┘
            │                        │                          │
            ▼                        ▼                          ▼
    reconcile_dom()          restyle_on_state_change()   CallbackChangeResult
    + compute_node_changes() → RestyleResult               .words_changed
    → ExtendedDiffResult       .changed_nodes              .css_properties_changed
      .node_changes            .max_relayout_scope         .images_changed
            │                        │                          │
            └────────────────────────┼──────────────────────────┘
                                     │
                                     ▼
                          ┌─────────────────────┐
                          │  ChangeAccumulator   │   ← NEW: unifies all 3 paths
                          │  per_node: Map<      │
                          │    NodeId →           │
                          │    (NodeChangeSet,    │
                          │     RelayoutScope)    │
                          │  >                    │
                          │  max_scope:           │   
                          │    RelayoutScope      │
                          │  damage_rects: Vec<>  │   ← for future compositor
                          └──────────┬────────────┘
                                     │
                          ┌──────────▼────────────┐
                          │ Decision Engine        │
                          │                        │
                          │ match max_scope {      │
                          │   None →               │
                          │     repaint only       │
                          │   IfcOnly →            │
                          │     reshape IFCs,      │
                          │     local relayout     │
                          │   SizingOnly →         │
                          │     intrinsic +        │
                          │     local relayout     │
                          │   Full →               │
                          │     full subtree       │
                          │     relayout           │
                          │ }                      │
                          └──────────┬────────────┘
                                     │
                                     ▼
                          ┌─────────────────────────────────┐
                          │ reconcile_and_invalidate()       │
                          │  - uses NodeChangeSet to set     │
                          │    DirtyFlag::Paint vs ::Layout  │
                          │  - adds paint_dirty set          │
                          │    (separate from intrinsic_     │
                          │     dirty / layout_roots)        │
                          └──────────┬──────────────────────┘
                                     │
                          ┌──────────▼──────────────────────┐
                          │ layout_document()                │
                          │  - Layout-dirty → full intrinsic │
                          │    + taffy + position pass       │
                          │  - Paint-dirty → skip to display │
                          │    list regeneration             │
                          │  - generate_display_list()       │
                          │    (future: damage_rects for     │
                          │     partial DL regeneration)     │
                          └─────────────────────────────────┘
```

---

## Data Structures

### ChangeAccumulator (new, in `core/src/diff.rs`)

```rust
/// Unified change report that merges information from all three change paths:
/// 1. DOM reconciliation (compute_node_changes)
/// 2. CSS restyle (restyle_on_state_change)
/// 3. Runtime edits (words_changed, css_properties_changed, images_changed)
///
/// This is the single source of truth for "what needs to be done this frame".
#[derive(Debug, Clone, Default)]
pub struct ChangeAccumulator {
    /// Per-node change info. Key is the new-DOM NodeId.
    /// The NodeChangeSet bitflags tell us WHAT changed;
    /// the RelayoutScope tells us HOW MUCH layout work is needed.
    pub per_node: BTreeMap<NodeId, NodeChangeReport>,

    /// Maximum RelayoutScope across all changed nodes.
    /// Quick check: if this is None, we can skip layout entirely.
    pub max_scope: RelayoutScope,

    /// Damage rectangles for compositor-level partial repaint (Phase 3+).
    /// Each rect is in logical coordinates relative to the window.
    pub damage_rects: Vec<LogicalRect>,

    /// Nodes that are newly mounted (no old counterpart).
    /// These always get DirtyFlag::Layout.
    pub mounted_nodes: Vec<NodeId>,

    /// Nodes that were unmounted (no new counterpart).
    /// Used for cleanup (remove from scroll/focus/cursor managers).
    pub unmounted_nodes: Vec<NodeId>,
}

/// Per-node change report combining multiple information sources.
#[derive(Debug, Clone, Default)]
pub struct NodeChangeReport {
    /// Bitflags from DOM-level field comparison.
    pub change_set: NodeChangeSet,

    /// Highest RelayoutScope from any CSS property that changed on this node.
    /// This is more granular than NodeChangeSet's binary LAYOUT/PAINT split.
    ///
    /// - `None` → repaint only (color, opacity, transform)
    /// - `IfcOnly` → reshape text in the containing IFC
    /// - `SizingOnly` → recompute this node's intrinsic size
    /// - `Full` → full subtree relayout (display, position, float, etc.)
    pub relayout_scope: RelayoutScope,

    /// Individual CSS properties that changed (for fine-grained cache invalidation).
    /// Empty if the change was structural (text content, node type, etc.)
    pub changed_css_properties: Vec<CssPropertyType>,

    /// If text content changed, the old and new text for cursor reconciliation.
    pub text_change: Option<TextChange>,
}

/// Text change info for cursor/selection reconciliation.
#[derive(Debug, Clone)]
pub struct TextChange {
    pub old_text: String,
    pub new_text: String,
}
```

### Updated NodeChangeSet (fix `affects_layout()` → `relayout_scope()`)

The existing `compute_node_changes()` in [diff.rs:168](core/src/diff.rs#L168) calls
`prop_type.affects_layout()` which was reverted. Replace with `relayout_scope()`:

```rust
// In compute_node_changes(), replace the CSS property comparison block:

// 4. Inline CSS properties — classify using RelayoutScope
let old_props = old_node.css_props.as_ref();
let new_props = new_node.css_props.as_ref();
if old_props != new_props {
    let mut has_layout = false;
    let mut has_paint = false;
    let mut max_scope = RelayoutScope::None;

    // Build old property map
    let mut old_map = FastHashMap::default();
    for prop in old_props.iter() {
        old_map.insert(prop.property.get_type(), prop);
    }

    // Check new vs old
    let mut seen_types = FastHashMap::default();
    for prop in new_props.iter() {
        let prop_type = prop.property.get_type();
        seen_types.insert(prop_type, ());
        match old_map.get(&prop_type) {
            Some(old_prop) if **old_prop == *prop => {} // unchanged
            _ => {
                let scope = prop_type.relayout_scope(true); // conservative
                if scope > max_scope { max_scope = scope; }
                if scope != RelayoutScope::None {
                    has_layout = true;
                } else {
                    has_paint = true;
                }
            }
        }
    }

    // Check removed properties
    for (prop_type, _) in old_map.iter() {
        if !seen_types.contains_key(prop_type) {
            let scope = prop_type.relayout_scope(true);
            if scope > max_scope { max_scope = scope; }
            if scope != RelayoutScope::None {
                has_layout = true;
            } else {
                has_paint = true;
            }
        }
    }

    if has_layout {
        changes.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
    }
    if has_paint {
        changes.insert(NodeChangeSet::INLINE_STYLE_PAINT);
    }
}
```

### Extended ReconciliationResult

```rust
/// The result of a reconciliation pass (modified).
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    /// Nodes needing intrinsic size recalculation (bottom-up).
    pub intrinsic_dirty: BTreeSet<usize>,
    /// Subtree roots needing top-down layout pass.
    pub layout_roots: BTreeSet<usize>,
    /// Nodes needing display-list regeneration only (no layout).  ← NEW
    pub paint_dirty: BTreeSet<usize>,
}

impl ReconciliationResult {
    pub fn is_clean(&self) -> bool {
        self.intrinsic_dirty.is_empty()
            && self.layout_roots.is_empty()
            && self.paint_dirty.is_empty()
    }

    pub fn needs_layout(&self) -> bool {
        !self.intrinsic_dirty.is_empty() || !self.layout_roots.is_empty()
    }

    pub fn needs_paint_only(&self) -> bool {
        !self.paint_dirty.is_empty() && !self.needs_layout()
    }
}
```

---

## Mapping RelayoutScope → DirtyFlag

```
RelayoutScope       DirtyFlag           Layout work needed
────────────        ─────────           ──────────────────
None                None (or Paint)     Regenerate display list entry only
IfcOnly             Layout              Reshape text in containing IFC + local relayout
SizingOnly          Layout              Recompute intrinsic sizes, local sibling repositioning
Full                Layout              Full subtree relayout

With NodeChangeSet:
NodeChangeSet::TEXT_CONTENT     → IfcOnly (reshape text, may cascade to SizingOnly if line count changes)
NodeChangeSet::IMAGE_CHANGED   → SizingOnly (image may have different intrinsic size) or None (same size)
NodeChangeSet::IDS_AND_CLASSES  → Full (conservative: class change may add any CSS property)
NodeChangeSet::INLINE_STYLE_*  → per-property RelayoutScope
NodeChangeSet::CHILDREN_CHANGED → Full
NodeChangeSet::NODE_TYPE_CHANGED → Full
NodeChangeSet::CALLBACKS        → None (no visual change)
NodeChangeSet::DATASET          → None (no visual change)
NodeChangeSet::STYLED_STATE     → depends on affected CSS properties (via RestyleResult)
```

---

## Integration with Dual-Path Text Editing

The dual-path text editing system (from `TEXT_INPUT_IMPLEMENTATION_PLAN_V3.md`)
creates two distinct change paths for text:

### Path A: Full DOM rebuild with text change

User's `layout_callback()` returns a new `StyledDom` with different text content.

```
layout_callback() → StyledDom(new)
       │
       ▼
reconcile_dom() matches old Text("Hello") ↔ new Text("Hello World")
       │        (via structural hash match)
       ▼
compute_node_changes() sets TEXT_CONTENT flag
       │
       ▼
ChangeAccumulator adds TextChange {
    old_text: "Hello",
    new_text: "Hello World",
}
       │
       ▼
reconcile_cursor_position() maps cursor byte offset
       │        (already exists in diff.rs:936)
       ▼
reconcile_and_invalidate() sets DirtyFlag::Layout
       │        (IfcOnly scope → only reshape the containing IFC)
       ▼
layout_document() reshapes IFC, local relayout
```

### Path B: Runtime text edit (optimistic, no DOM rebuild)

User types a character → `apply_text_changeset()` modifies the layout cache
directly without calling the user's `layout_callback()`.

```
keystroke → TextInputManager.record_input()
       │
       ▼
apply_text_changeset() [in LayoutWindow]
       │
       ├── update text in dirty_text_nodes cache
       ├── CursorManager.set_cursor()
       │
       ▼
ChangeAccumulator adds:
  - NodeChangeSet::TEXT_CONTENT for the IFC root
  - RelayoutScope::IfcOnly
  - TextChange { old, new }
       │
       ▼
relayout_dirty_nodes() [fast path, no reconciliation]
  - Get cached UnifiedConstraints for this IFC
  - Reshape text (stages 1-4 of text pipeline)
  - Check if IFC height changed → may upgrade to SizingOnly
  - Update calculated_positions locally
  - Regenerate display list
```

### Cursor position preservation across paths

Both paths use the same cursor reconciliation:

1. **`CursorManager.cursor`** stores `TextCursor { cluster_id: GraphemeClusterId, affinity }`
2. **`CursorLocation`** has `contenteditable_key: u64` — a stable key calculated
   via `calculate_contenteditable_key()` using the reconciliation key hierarchy
3. **`reconcile_cursor_position(old_text, new_text, old_byte)`** maps byte offsets
   through prefix/suffix matching (already implemented in [diff.rs:936](core/src/diff.rs#L936))
4. **`CursorManager.remap_node_ids()`** updates `cursor_location.node_id` via the
   migration map (already implemented in [cursor.rs:361](layout/src/managers/cursor.rs#L361))

The `ChangeAccumulator.per_node[id].text_change` provides the old/new text pair
needed by `reconcile_cursor_position()`, eliminating the need to re-access the
old DOM's node data after reconciliation.

### Scroll position preservation

`ScrollManager` already preserves `AnimatedScrollState.current_offset` and
`ScrollAnimation` across DOM rebuilds via `remap_node_ids()`. The
`ChangeAccumulator` does NOT need to handle scroll — scroll positions are
orthogonal to DOM/style changes.

However, scroll velocity (momentum) could be interrupted by a full DOM
rebuild today (because `regenerate_layout()` rebuilds the LayoutTree).
The incremental path avoids this: if `max_scope <= SizingOnly`, the
`AnimatedScrollState.animation` is preserved without interruption.

---

## Integration with Restyle Path

### Current problem (restyle → full DOM rebuild)

```
apply_focus_restyle() → restyle_on_state_change()
   → RestyleResult { needs_layout: true, max_relayout_scope: IfcOnly, ... }
   → ProcessEventResult::ShouldRegenerateDomCurrentWindow  ← BUG: throws away granular info
   → regenerate_layout() calls user's layout_callback()     ← unnecessary DOM rebuild
   → apply_runtime_states_before_layout() re-applies states ← restyle info recomputed from scratch
```

### Proposed fix: new ProcessEventResult level

```rust
pub enum ProcessEventResult {
    DoNothing,
    ShouldReRenderCurrentWindow,            // compositor repaint only
    ShouldUpdateDisplayListCurrentWindow,   // rebuild display list, no layout
    ShouldIncrementalRelayout,              // ← NEW: restyle-driven, no DOM rebuild
    UpdateHitTesterCurrentWindow,           // rebuild hit tester
    ShouldRegenerateDomCurrentWindow,       // full DOM rebuild (user's layout_callback)
    ShouldRegenerateDomAllWindows,          // all windows
}
```

### Updated `apply_focus_restyle()`:

```rust
fn apply_focus_restyle(
    layout_window: &mut LayoutWindow,
    old_focus: Option<NodeId>,
    new_focus: Option<NodeId>,
) -> ProcessEventResult {
    let restyle = layout_result.styled_dom.restyle_on_state_change(...);

    if restyle.changed_nodes.is_empty() {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    }

    if restyle.gpu_only_changes {
        return ProcessEventResult::ShouldReRenderCurrentWindow; // GPU handles transform/opacity
    }

    if restyle.needs_layout {
        // NEW: Instead of ShouldRegenerateDomCurrentWindow, use incremental path
        // Feed RestyleResult into ChangeAccumulator
        let mut accumulator = ChangeAccumulator::default();
        accumulator.merge_restyle_result(&restyle);

        // Apply directly to the layout tree's dirty flags
        for (node_id, report) in &accumulator.per_node {
            let layout_idx = layout_result.node_id_to_layout_idx(*node_id);
            match report.relayout_scope {
                RelayoutScope::None => {
                    layout_tree.mark_dirty(layout_idx, DirtyFlag::Paint);
                }
                RelayoutScope::IfcOnly => {
                    // Find IFC root and mark it for reshape
                    let ifc_root = layout_tree.find_ifc_root(layout_idx);
                    layout_tree.mark_dirty(ifc_root, DirtyFlag::Layout);
                }
                RelayoutScope::SizingOnly | RelayoutScope::Full => {
                    layout_tree.mark_dirty(layout_idx, DirtyFlag::Layout);
                }
            }
        }

        return ProcessEventResult::ShouldIncrementalRelayout;
    }

    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
}
```

### Shell-layer handling of `ShouldIncrementalRelayout`:

```rust
// In macOS shell (and equivalent for other platforms):
ProcessEventResult::ShouldIncrementalRelayout => {
    // Don't call user's layout_callback() — DOM is unchanged
    // Just re-run layout_document() with the existing StyledDom
    // The ChangeAccumulator has already set dirty flags on the LayoutTree
    let display_list = layout_document(
        &mut cache,
        // ... same StyledDom, just with dirty flags updated ...
    );
    // Build WR transaction with new display list
    build_webrender_transaction(&display_list);
}
```

---

## Integration with Runtime Edits (Path C)

### Activating dead fields

The `words_changed` and `css_properties_changed` fields in
`CallbackChangeResult` are currently computed but never consumed.
They should be fed into `ChangeAccumulator`:

```rust
// In process_callback_result_v2(), after collecting CallCallbacksResult:

if let Some(words) = &result.words_changed {
    for (dom_id, nodes) in words {
        for (node_id, new_text) in nodes {
            accumulator.add_text_change(*node_id, RelayoutScope::IfcOnly);
        }
    }
}

if let Some(css) = &result.css_properties_changed {
    for (dom_id, nodes) in css {
        for (node_id, properties) in nodes {
            for prop in properties.iter() {
                let scope = prop.get_type().relayout_scope(true);
                accumulator.add_css_change(*node_id, prop.get_type(), scope);
            }
        }
    }
}

if let Some(images) = &result.images_changed {
    for (dom_id, nodes) in images {
        for (node_id, (image_ref, update_type)) in nodes {
            // Check if intrinsic size changed
            let scope = if size_changed { RelayoutScope::SizingOnly } else { RelayoutScope::None };
            accumulator.add_image_change(*node_id, scope);
        }
    }
}

// Then determine ProcessEventResult from accumulator:
match accumulator.max_scope {
    RelayoutScope::None => ProcessEventResult::ShouldUpdateDisplayListCurrentWindow,
    _ => ProcessEventResult::ShouldIncrementalRelayout,
}
```

---

## Adjusting Node Data Hashing

### Current: single hash, loses all granularity

```rust
fn hash_styled_node_data(dom: &StyledDom, node_id: NodeId) -> u64 {
    let mut hasher = DefaultHasher::new();
    styled_node.styled_node_state.hash(&mut hasher);
    node_data.get_node_type().hash(&mut hasher);
    hasher.finish()
}
```

**Problem**: This hash combines `styled_node_state` + `node_type` into one value.
If ANYTHING differs, the node is fully dirty. It doesn't distinguish:
- text content change vs node type change
- state change (hover) vs structural change
- CSS property change (needs `relayout_scope()` classification)

### Proposed: multi-field hash for fast change detection

Replace the single hash with a struct of per-field hashes:

```rust
/// Per-node hash broken into independent fields.
/// Each field can be compared separately to determine what changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeDataFingerprint {
    /// Hash of node_type (Text content, Image ref, Div, etc.)
    pub content_hash: u64,
    /// Hash of styled_node_state (hover, focus, active bits)
    pub state_hash: u64,
    /// Hash of inline CSS properties
    pub inline_css_hash: u64,
    /// Hash of ids_and_classes
    pub ids_classes_hash: u64,
    /// Hash of callbacks (event types + function pointers)
    pub callbacks_hash: u64,
    /// Hash of other attributes (contenteditable, tab_index, dataset, a11y)
    pub attrs_hash: u64,
}

impl NodeDataFingerprint {
    pub fn compute(dom: &StyledDom, node_id: NodeId) -> Self {
        // ... hash each field independently ...
    }

    /// Returns a NodeChangeSet by comparing two fingerprints.
    /// This is O(1) — just comparing 6 u64s instead of walking all fields.
    pub fn diff(&self, other: &NodeDataFingerprint) -> NodeChangeSet {
        let mut changes = NodeChangeSet::empty();
        if self.content_hash != other.content_hash {
            // Could be TEXT_CONTENT, IMAGE_CHANGED, or NODE_TYPE_CHANGED
            // We set a generic flag; detailed classification needs NodeData access
            changes.insert(NodeChangeSet::TEXT_CONTENT | NodeChangeSet::IMAGE_CHANGED);
        }
        if self.state_hash != other.state_hash {
            changes.insert(NodeChangeSet::STYLED_STATE);
        }
        if self.inline_css_hash != other.inline_css_hash {
            // Conservative: mark as layout. Detailed classification done by
            // compute_node_changes() which checks relayout_scope() per property.
            changes.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
        }
        if self.ids_classes_hash != other.ids_classes_hash {
            changes.insert(NodeChangeSet::IDS_AND_CLASSES);
        }
        if self.callbacks_hash != other.callbacks_hash {
            changes.insert(NodeChangeSet::CALLBACKS);
        }
        if self.attrs_hash != other.attrs_hash {
            changes.insert(NodeChangeSet::TAB_INDEX | NodeChangeSet::CONTENTEDITABLE);
        }
        changes
    }

    /// Returns true if all fields are identical.
    pub fn is_identical(&self, other: &NodeDataFingerprint) -> bool {
        self == other
    }

    /// Quick check: does this change affect layout?
    /// Only does the expensive field-by-field check if quick hash check says "maybe".
    pub fn might_affect_layout(&self, other: &NodeDataFingerprint) -> bool {
        self.content_hash != other.content_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
            || self.attrs_hash != other.attrs_hash
    }

    /// Quick check: is this a visual change at all?
    pub fn might_affect_visuals(&self, other: &NodeDataFingerprint) -> bool {
        self.content_hash != other.content_hash
            || self.state_hash != other.state_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
    }
}
```

### Two-tier change detection

```
Tier 1 (O(1) per node, in reconcile_recursive):
    Compare NodeDataFingerprint fields.
    If all match → DirtyFlag::None (skip entirely).
    If only state/callbacks/attrs differ → quick classification.
    If content/css/ids differ → go to Tier 2.

Tier 2 (O(n) per changed field, only for changed nodes):
    compute_node_changes() does field-by-field NodeData comparison.
    For CSS props: uses relayout_scope() per property.
    Returns precise NodeChangeSet + RelayoutScope.
```

This avoids the expensive `compute_node_changes()` for unchanged nodes (vast
majority) while still getting granular info for the few that changed.

---

## Damage Tracking (Compositor Hints)

For future work, `ChangeAccumulator.damage_rects` collects the bounding boxes
of all changed nodes. These can be passed to WebRender as
`Transaction::set_root_pipeline_dirty_rects()` or equivalent, telling the
compositor to only re-rasterize affected tiles.

```rust
impl ChangeAccumulator {
    /// After layout, compute damage rects from the changed nodes' positions.
    pub fn compute_damage_rects(
        &mut self,
        calculated_positions: &PositionVec,
        used_sizes: &[LogicalSize],
    ) {
        for (node_id, report) in &self.per_node {
            if report.change_set.is_visually_unchanged() {
                continue;
            }
            let idx = node_id.index();
            if let Some(pos) = calculated_positions.get(idx) {
                let size = used_sizes.get(idx).copied().unwrap_or_default();
                self.damage_rects.push(LogicalRect {
                    origin: *pos,
                    size,
                });
            }
        }
    }
}
```

This is deferred to Phase 3+ but the data structure is ready.

---

## Implementation Phases

### Phase 1: Fix existing code + ChangeAccumulator skeleton

**Goal**: Fix the broken `compute_node_changes()` (calls reverted `affects_layout()`),
add `ChangeAccumulator`, wire it through `reconcile_and_invalidate()`.

| File | Change |
|---|---|
| [core/src/diff.rs](core/src/diff.rs) | Fix `compute_node_changes()` to use `relayout_scope()` instead of `affects_layout()` |
| [core/src/diff.rs](core/src/diff.rs) | Add `ChangeAccumulator`, `NodeChangeReport`, `TextChange` |
| [layout/src/solver3/cache.rs](layout/src/solver3/cache.rs) | Add `paint_dirty: BTreeSet<usize>` to `ReconciliationResult` |
| [layout/src/solver3/cache.rs](layout/src/solver3/cache.rs) | Store `NodeDataFingerprint` in `LayoutNode` instead of single `node_data_hash: u64` |
| [layout/src/solver3/cache.rs](layout/src/solver3/cache.rs) | Update `reconcile_recursive()` to use fingerprint diff → `DirtyFlag::Paint` vs `::Layout` |

### Phase 2: Connect restyle path

**Goal**: `ShouldIncrementalRelayout` replaces `ShouldRegenerateDomCurrentWindow`
for restyle-only changes (hover color doesn't rebuild DOM).

| File | Change |
|---|---|
| [core/src/events.rs](core/src/events.rs) | Add `ShouldIncrementalRelayout` to `ProcessEventResult` |
| [dll/src/desktop/shell2/common/event_v2.rs](dll/src/desktop/shell2/common/event_v2.rs) | Update `apply_focus_restyle()` to feed `RestyleResult` → `ChangeAccumulator` → incremental path |
| [dll/src/desktop/shell2/macos/mod.rs](dll/src/desktop/shell2/macos/mod.rs) | Handle `ShouldIncrementalRelayout` in render loop (relayout without user callback) |
| [dll/src/desktop/shell2/common/layout_v2.rs](dll/src/desktop/shell2/common/layout_v2.rs) | Add `incremental_relayout()` that re-runs `layout_document()` on existing StyledDom with dirty flags |

### Phase 3: Activate dead fields (runtime edits)

**Goal**: `words_changed`, `css_properties_changed`, `images_changed` from
callbacks trigger incremental layout without full DOM rebuild.

| File | Change |
|---|---|
| [dll/src/desktop/shell2/common/event_v2.rs](dll/src/desktop/shell2/common/event_v2.rs) | In `process_callback_result_v2()`, feed `CallCallbacksResult` → `ChangeAccumulator` |
| [layout/src/window.rs](layout/src/window.rs) | Apply `words_changed`/`css_properties_changed` directly to cached `StyledDom` before relayout |

### Phase 4: Text-specific optimizations

**Goal**: `TEXT_CONTENT` changes only reshape the affected IFC, not the whole page.

| File | Change |
|---|---|
| [layout/src/solver3/cache.rs](layout/src/solver3/cache.rs) | When `TEXT_CONTENT` flag set + scope is `IfcOnly`: only invalidate the IFC's `CachedInlineLayout` |
| [layout/src/text3/cache.rs](layout/src/text3/cache.rs) | Incremental reshape: reuse `InlineItemMetrics` for unchanged runs |
| [core/src/diff.rs](core/src/diff.rs) | `compute_text_edit_range()` for finding affected byte range |

### Phase 5: Damage tracking

**Goal**: Tell compositor which screen regions changed (partial re-rasterization).

| File | Change |
|---|---|
| [layout/src/solver3/mod.rs](layout/src/solver3/mod.rs) | After layout, call `accumulator.compute_damage_rects()` |
| [dll/src/desktop/shell2/common/layout_v2.rs](dll/src/desktop/shell2/common/layout_v2.rs) | Pass `damage_rects` to WebRender transaction |

---

## Correctness Guarantees

1. **Fallback safety**: Any node without a fingerprint match defaults to
   `DirtyFlag::Layout` — same behavior as today.

2. **Conservative classification**: `IDS_AND_CLASSES` → `Full` relayout
   (class changes may add layout-affecting properties). Future: quick
   restyle check to see if actual affected properties are paint-only.

3. **IFC height cascade**: If an `IfcOnly` change causes the IFC to grow
   taller (e.g., text wraps to a new line), automatic upgrade to
   `SizingOnly` → parent repositioning.

4. **Restyle consistency**: The `ShouldIncrementalRelayout` path re-runs
   layout on the SAME `StyledDom` with updated `styled_node_state` —
   identical result to a full DOM rebuild where the user returns the
   same DOM with same states.

5. **State preservation**: Scroll offsets, cursor positions, selections,
   scroll animations, and drag state are all preserved across incremental
   relayouts (no `remap_node_ids()` needed since NodeIds don't change).

6. **Hash as verification**: In debug builds, `compute_node_changes()` can
   be run as a cross-check against the fingerprint-based classification.

---

## Performance Impact

| Scenario | Nodes | Today | After Phase 2 |
|---|---|---|---|
| GL callback animation (100 nodes) | 100 | Full layout | `is_clean()` → 0 layout |
| Hover color change (500 nodes) | 1 | Full DOM rebuild + layout | Paint-only for 1 node |
| Typing in text input (1000 nodes) | 1 IFC | Full DOM rebuild + layout | Reshape 1 IFC |
| Structural change (add 10 nodes) | 10 | Full layout | Full layout (no change) |
| CSS font-size via toggle | 1 | Full DOM rebuild + layout | IFC reshape + local relayout |
| `set_node_text()` from callback | 1 | **Ignored** (dead field) | IFC reshape + local relayout |
| `set_css_property()` of color | 1 | **Ignored** (dead field) | Paint-only |
| Unchanged timer tick | 0 | `is_layout_equivalent()` | `is_clean()` early exit |

---

## Files Modified (All Phases)

```
core/src/diff.rs                            — Fix compute_node_changes(), add ChangeAccumulator,
                                              NodeChangeReport, TextChange, NodeDataFingerprint
core/src/events.rs                          — Add ShouldIncrementalRelayout to ProcessEventResult
core/src/styled_dom.rs                      — (unchanged, RestyleResult already correct)
css/src/props/property.rs                   — (unchanged, relayout_scope() already exists)
layout/src/solver3/cache.rs                 — ReconciliationResult.paint_dirty, NodeDataFingerprint
                                              in LayoutNode, update reconcile_recursive()
layout/src/solver3/layout_tree.rs           — Store NodeDataFingerprint instead of u64 hash
layout/src/solver3/mod.rs                   — Handle paint_dirty in layout_document()
layout/src/window.rs                        — Apply words_changed/css_properties_changed
dll/src/desktop/shell2/common/event_v2.rs   — Feed RestyleResult + CallCallbacksResult → ChangeAccumulator,
                                              apply_focus_restyle() → incremental path
dll/src/desktop/shell2/common/layout_v2.rs  — Add incremental_relayout(), update regenerate_layout()
dll/src/desktop/shell2/macos/mod.rs         — Handle ShouldIncrementalRelayout in render loop
dll/src/desktop/shell2/macos/events.rs      — Map ShouldIncrementalRelayout to RegenerateDisplayList
```
