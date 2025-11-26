# Cursor Movement Inspection API - Direct Access Optimization

## Overview
Refactored the `inspect_move_cursor_*` methods in `CallbackInfo` to use direct O(1) access to text layouts instead of O(n) iteration through all cache IDs.

## Problem
The initial implementation of cursor movement inspection methods had a critical performance issue:

```rust
// OLD APPROACH - O(n) iteration
pub fn inspect_move_cursor_left(&self, target: DomNodeId) -> Option<TextCursor> {
    let layout_window = self.internal_get_layout_window();
    let cursor = layout_window.cursor_manager.get_cursor()?;
    
    // PROBLEM: Iterates through ALL cached text layouts in the window
    for cache_id in layout_window.text_cache.get_all_layout_ids() {
        if let Some(layout) = layout_window.text_cache.get_layout(&cache_id) {
            let new_cursor = layout.move_cursor_left(*cursor, &mut None);
            if new_cursor != *cursor {
                return Some(new_cursor);
            }
        }
    }
    None
}
```

**Issues with this approach:**
1. **O(n) complexity** - Iterates through every text layout in the window
2. **Wasteful** - Only needs to check ONE specific node's layout
3. **Doesn't scale** - Performance degrades as more text nodes are added
4. **Wrong CacheId assumption** - CacheId is a content hash, not a stable node identifier

## Solution: Direct Access via Layout Tree

### Architecture Understanding

The key insight is that `LayoutNode` stores a **direct reference** to its text layout:

```rust
// From layout/src/solver3/layout_tree.rs
pub struct LayoutNode {
    pub inline_layout_result: Option<Arc<UnifiedLayout>>, // ← Direct reference!
    // ... other fields
}
```

This is populated during layout:
```rust
// From layout/src/solver3/fc.rs line 534
node.inline_layout_result = Some(main_frag.clone());
```

### Access Chain

`CallbackInfo` has full read access to `LayoutWindow`, which contains everything needed:

```
CallbackInfo
  ↓ internal_get_layout_window()
LayoutWindow
  ↓ layout_results: BTreeMap<DomId, DomLayoutResult>
DomLayoutResult
  ↓ layout_tree: LayoutTree
LayoutTree
  ├─ nodes: Vec<LayoutNode>                   (all layout nodes)
  └─ dom_to_layout: BTreeMap<NodeId, Vec<usize>>      (NodeId → layout indices)
       ↓
LayoutNode
  └─ inline_layout_result: Option<Arc<UnifiedLayout>>  ← Direct access!
```

### Implementation

Added a helper method that follows this chain efficiently:

```rust
fn get_inline_layout_for_node(&self, node_id: &DomNodeId) 
    -> Option<&Arc<UnifiedLayout<FontRef>>> 
{
    let layout_window = self.internal_get_layout_window();
    
    // 1. Get the layout result for this DOM - O(log n) lookup
    let layout_result = layout_window.layout_results.get(&node_id.dom)?;
    
    // 2. Convert NodeHierarchyItemId to NodeId
    let dom_node_id = node_id.node.into_crate_internal()?;
    
    // 3. Look up layout node indices - O(log n) lookup
    let layout_indices = layout_result.layout_tree.dom_to_layout.get(&dom_node_id)?;
    
    // 4. Get first layout index - O(1) access
    let layout_index = *layout_indices.first()?;
    
    // 5. Get layout node - O(1) access
    let layout_node = layout_result.layout_tree.nodes.get(layout_index)?;
    
    // 6. Return direct reference to text layout - O(1) access
    layout_node.inline_layout_result.as_ref()
}
```

**Total complexity: O(log n)** - Two BTreeMap lookups instead of O(n) iteration!

### Refactored Methods

All cursor movement inspection methods now use this pattern:

```rust
// NEW APPROACH - O(log n) direct access
pub fn inspect_move_cursor_left(&self, target: DomNodeId) -> Option<TextCursor> {
    let layout_window = self.internal_get_layout_window();
    let cursor = layout_window.cursor_manager.get_cursor()?;
    
    // Direct O(log n) access to the specific node's layout
    let layout = self.get_inline_layout_for_node(&target)?;
    
    // Use text3::cache cursor movement logic
    let new_cursor = layout.move_cursor_left(*cursor, &mut None);
    
    if new_cursor != *cursor {
        Some(new_cursor)
    } else {
        None
    }
}
```

**Refactored methods:**
- `inspect_move_cursor_left()` - O(log n) instead of O(n)
- `inspect_move_cursor_right()` - O(log n) instead of O(n)
- `inspect_move_cursor_up()` - O(log n) instead of O(n)
- `inspect_move_cursor_down()` - O(log n) instead of O(n)
- `inspect_move_cursor_to_line_start()` - O(log n) instead of O(n)
- `inspect_move_cursor_to_line_end()` - O(log n) instead of O(n)

## Why CacheId Iteration Was Wrong

### CacheId is Ephemeral
```rust
// From layout/src/text3/cache.rs
pub type CacheId = u64;

fn calculate_id<T: Hashable>(content: &T) -> u64 {
    let mut hasher = FxHasher::default();
    content.hash(&mut hasher);
    hasher.finish()
}
```

**Key issue:** CacheId is a **content hash** that changes when text changes. It's not a stable node identifier!

### Reference Implementation

The display list generation code (`layout/src/solver3/display_list.rs`) already uses the correct pattern:

```rust
// From display_list.rs line 570
let Some(layout) = &node.inline_layout_result else {
    return Ok(());
};

// ... use layout directly
```

It **never** iterates through cache IDs - it accesses `inline_layout_result` directly!

## Performance Impact

### Before (O(n) iteration)
- **10 text nodes:** ~10 cache lookups per cursor movement
- **100 text nodes:** ~100 cache lookups per cursor movement
- **1000 text nodes:** ~1000 cache lookups per cursor movement

### After (O(log n) direct access)
- **10 text nodes:** ~3 BTreeMap lookups
- **100 text nodes:** ~7 BTreeMap lookups
- **1000 text nodes:** ~10 BTreeMap lookups

**Speedup factor:** Up to 100x improvement for documents with many text nodes!

## Files Modified

- **layout/src/callbacks.rs**
  - Added `get_inline_layout_for_node()` helper method (lines 1002-1030)
  - Refactored all 6 cursor movement inspection methods (lines 2250-2390)
  - Removed O(n) cache iteration loops
  - Added proper documentation

## Testing

Compiled successfully with `cargo check -p azul-layout` - no errors.

## Next Steps

This same optimization pattern should be applied to other code in `window.rs` that currently iterates through `text_cache.get_all_layout_ids()`. The grep search revealed:

```
layout/src/window.rs:2354:            for cache_id in self.text_cache.get_all_layout_ids() {
layout/src/window.rs:2597:            for cache_id in self.text_cache.get_all_layout_ids() {
layout/src/window.rs:2777:            for cache_id in self.text_cache.get_all_layout_ids() {
layout/src/window.rs:2953:            for cache_id in self.text_cache.get_all_layout_ids() {
```

These should be investigated and potentially refactored to use direct access via `inline_layout_result`.

## Lessons Learned

1. **CacheId is NOT a node identifier** - It's a content hash that changes
2. **LayoutNode stores direct references** - No need for cache lookups
3. **display_list.rs shows the correct pattern** - Always check existing code for reference implementations
4. **CallbackInfo has full read access** - Don't assume limited access without verification
5. **Performance matters** - O(n) → O(log n) is a significant improvement

## References

- `layout/src/callbacks.rs` - CallbackInfo implementation
- `layout/src/window.rs` - LayoutWindow structure
- `layout/src/solver3/layout_tree.rs` - LayoutNode and LayoutTree definitions
- `layout/src/solver3/fc.rs` - Where inline_layout_result is populated
- `layout/src/solver3/display_list.rs` - Reference implementation of direct access
- `layout/src/text3/cache.rs` - CacheId generation and UnifiedLayout methods
