# Plan: BTreeMap<usize, LogicalPosition> → Vec<LogicalPosition>

## Motivation

`calculated_positions` maps layout-tree node indices (0..N contiguous) to their
absolute positions. Using `BTreeMap<usize, LogicalPosition>` means:

- **O(log N) per lookup/insert** vs O(1) for Vec indexing
- **Poor cache locality**: tree nodes scattered across heap allocations
- **Allocation overhead**: ~12k nodes = thousands of BTreeMap internal nodes
- For 12k-node DOMs (git2pdf), this is called tens of thousands of times during layout

With `Vec<LogicalPosition>`, every access is O(1) with perfect cache locality.

## Scope

### Files to change (layout crate — 21 type signatures, ~114 references)

| File | Type sigs | Access patterns | Notes |
|------|-----------|-----------------|-------|
| `solver3/cache.rs` | 7 | `.get()`, `.insert()`, `.get_mut()`, `.contains_key()`, `.clone()` | **LayoutCache.calculated_positions** field + all positioning fns |
| `solver3/fc.rs` | 2 | `LayoutOutput.positions` field, return type | Also a BTreeMap<usize, LP> — separate map, relative positions |
| `solver3/display_list.rs` | 2 | `.get()` (read-only) | `generate_display_list` + `DisplayListContext` |
| `solver3/positioning.rs` | 3 | `.get()`, `.get_mut()`, `.insert()` | `adjust_relative_positions`, `position_out_of_flow_elements` |
| `solver3/mod.rs` | 1 | `.get()`, `.clone()`, `.contains_key()`, `.insert()` | `layout_document` + helper |
| `solver3/paged_layout.rs` | 1 | `.clone()`, `.contains_key()`, `.insert()` | `compute_layout_with_fragmentation` |
| `window.rs` | 2 | `.get()`, `.clone()` | `LayoutResult.calculated_positions` + hit-testing |

### Files to change (dll crate — 6 references, read-only)

| File | References | Notes |
|------|-----------|-------|
| `dll/src/desktop/wr_translate2.rs` | 3 | `.get(&layout_index)` |
| `dll/src/desktop/shell2/common/layout_v2.rs` | 2 | `.get(&layout_index)` |
| `dll/src/desktop/shell2/common/debug_server.rs` | 1 | `.get(&layout_index)` |

### Special case: `LayoutOutput.positions` in fc.rs

This is a **separate** BTreeMap used in `layout_bfc`, `layout_ifc`, etc. to
collect child positions relative to a container. The keys are also contiguous
layout-tree indices, but they're a **subset** of all nodes (only the children of
one container). Options:

1. **Also convert to Vec** — pre-size to `tree.nodes.len()`, index directly
2. **Keep as BTreeMap** — smaller map, less hot path
3. **Use a sparse Vec with sentinel** — `Vec<Option<LogicalPosition>>`

**Decision**: Convert to Vec too. The indices are layout-tree indices (same
space), and fc.rs is called thousands of times. Pre-size to max possible index.

## Migration strategy

### Type alias approach (incremental, safe)

```rust
// In a common module (e.g. solver3/mod.rs or a new solver3/types.rs):
pub type PositionVec = Vec<LogicalPosition>;

// Wrapper functions for the transition:
#[inline(always)]
fn pos_get(positions: &PositionVec, idx: usize) -> Option<&LogicalPosition> {
    positions.get(idx).filter(|p| p.x != f32::MIN)
}

#[inline(always)]
fn pos_set(positions: &mut PositionVec, idx: usize, pos: LogicalPosition) {
    if idx >= positions.len() {
        positions.resize(idx + 1, LogicalPosition::new(f32::MIN, f32::MIN));
    }
    positions[idx] = pos;
}

#[inline(always)]
fn pos_contains(positions: &PositionVec, idx: usize) -> bool {
    positions.get(idx).map_or(false, |p| p.x != f32::MIN)
}
```

### Sentinel value

Use `LogicalPosition { x: f32::MIN, y: f32::MIN }` as "not set". This avoids
wrapping in `Option<>` and keeps the Vec tightly packed (8 bytes per entry vs
12+ with Option).

No real position will ever be f32::MIN.

### Initialization

```rust
// In cache.rs, when creating/cloning calculated_positions:
let mut calculated_positions = vec![
    LogicalPosition::new(f32::MIN, f32::MIN);
    new_tree.nodes.len()
];
```

## Step-by-step execution plan

### Phase 1: Add type alias + helper functions
- [ ] Add `PositionVec` type alias and `pos_get`/`pos_set`/`pos_contains` helpers
- [ ] Add sentinel constant `POSITION_UNSET`

### Phase 2: Change LayoutCache.calculated_positions
- [ ] Change field type in `cache.rs` `LayoutCache` struct
- [ ] Update `LayoutCache::default()` / initialization
- [ ] Update `.clone()` calls (Vec::clone is faster than BTreeMap::clone)

### Phase 3: Update cache.rs functions (7 signatures, ~40 references)
- [ ] `reposition_clean_subtrees` + `shift_subtree_position`
- [ ] `calculate_layout_for_subtree` (the main one)
- [ ] `position_bfc_child` + `position_bfc_child_descendants`
- [ ] `position_ifc_children`
- [ ] `position_flex_children` + `position_grid_children`

### Phase 4: Update fc.rs LayoutOutput.positions
- [ ] Change `LayoutOutput.positions` type
- [ ] Update `layout_bfc_children_block`, `layout_ifc`, etc.
- [ ] Update return type of `position_ifc_lines`

### Phase 5: Update positioning.rs (3 signatures)
- [ ] `adjust_relative_positions`
- [ ] `position_out_of_flow_elements`
- [ ] `resolve_sticky_positions`

### Phase 6: Update display_list.rs (2 signatures, read-only)
- [ ] `generate_display_list` parameter
- [ ] `DisplayListContext.calculated_positions` field

### Phase 7: Update mod.rs + paged_layout.rs (callers)
- [ ] `layout_document()` in mod.rs
- [ ] `compute_layout_with_fragmentation()` in paged_layout.rs
- [ ] `get_containing_block_for_node()` helpers in both files

### Phase 8: Update window.rs (external API)
- [ ] `LayoutResult.calculated_positions` field
- [ ] Hit-testing functions that read positions

### Phase 9: Update dll crate (6 read-only references)
- [ ] `wr_translate2.rs` — 3x `.get()`
- [ ] `layout_v2.rs` — 2x `.get()`
- [ ] `debug_server.rs` — 1x `.get()`

### Phase 10: Build + test
- [ ] `cargo build -p azul-layout --features text_layout`
- [ ] `cargo build` (full, including dll if possible)
- [ ] Run git2pdf benchmark to measure improvement
- [ ] Commit

## Risk assessment

- **Low risk**: All indices are contiguous 0..N from the layout tree
- **Sentinel value**: f32::MIN is safe — no real position is ever that value
- **No semantic change**: Just data structure swap, same logical behavior
- **Easy rollback**: If anything breaks, `git revert` one commit

## Expected performance gain

- ~50-100ms per layout pass (Gemini estimate for 12k nodes)
- Better cache behavior compounds with other optimizations
- Vec::clone is a single memcpy vs BTreeMap's tree-walk clone
