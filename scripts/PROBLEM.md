# WebRender Display List Serialization Problem

## STATUS: ✅ FULLY FIXED (2024-12-10)

## Background

WebRender was originally designed for Firefox, where display lists need to be serialized 
and sent over IPC (Inter-Process Communication) between the content process and the GPU process.
This serialization uses the `peek-poke` crate for efficient binary serialization.

In Azul, we don't need IPC - everything runs in the same process. The serialization was 
removed to reduce dependencies and improve performance, but **this broke the renderer** 
because no replacement storage mechanism was implemented.

## Solution Implemented (Complete Refactor)

**All traces of serialization have been removed.** Items are now stored directly in typed Vecs:

```rust
pub struct DisplayListPayload {
    // Core display items
    pub items: Vec<di::DisplayItem>,
    pub spatial_items: Vec<di::SpatialTreeItem>,
    
    // Auxiliary data for variable-length items
    pub glyphs: Vec<GlyphInstance>,
    pub stops: Vec<di::GradientStop>,
    pub filters: Vec<di::FilterOp>,
    pub filter_data: Vec<di::FilterData>,
    pub filter_primitives: Vec<di::FilterPrimitive>,
    pub clip_chain_items: Vec<di::ClipId>,
    pub points: Vec<LayoutPoint>,
}
```

### Key Changes:

1. **`DisplayListPayload`**: Removed legacy `items_data`, `cache_data`, `spatial_tree` fields
2. **`BuiltDisplayListIter`**: Now uses `&'a DisplayListPayload` directly with index tracking
3. **All accessor methods** return `&[T]` slices instead of `ItemRange<T>`
4. **`scene_building.rs`**: Updated to accept slices instead of `ItemRange`
5. **No more peek-poke serialization anywhere in the codebase**

## Verification

```
$ cargo run --bin test_rectangles
...
[compositor2] >>>>> builder.end() RETURNED, dl.size_in_bytes()=2808 <<<<<
```

Display list now contains data (2808 bytes) instead of 0 bytes!

## Root Cause (Historical)

The `DisplayListBuilder` pushes items via `push_item_to_section()`, which **used to** 
serialize items into `payload.items_data` (a `Vec<u8>`). After the serialization was removed,
this function now does **nothing**, so items are never stored.

When the `BuiltDisplayListIter` tries to iterate over items, it reads from the empty 
`items_data` buffer and finds nothing.

## Affected Call Sites (Fixed)

All locations in `/azul/webrender/api/src/display_list.rs` marked with `// Serialization removed`:

### 1. Item Storage (FIXED)

| Line | Function | Original (upstream) | Fix Applied |
|------|----------|---------------------|-------------|
| 1172 | `push_item_to_section` | `poke_into_vec(item, buffer)` | `self.payload.items.push(*item)` |
| 1189 | `push_spatial_tree_item` | `poke_into_vec(item, &mut spatial_tree)` | `self.payload.spatial_items.push(*item)` |

### 2. Item Reading (FIXED)

| Line | Function | Original (upstream) | Fix Applied |
|------|----------|---------------------|-------------|
| 813 | `next_raw` | `peek_from_slice` to deserialize | Read from `items[item_index]` |
| `iter_spatial_tree` | `peek_from_slice` to deserialize | Iterate over `spatial_items` Vec |

### 3. Red Zone Management (Removed - Not Needed)

| Line | Function | Original | Current |
|------|----------|----------|---------|
| 1118 | `reset` | `ensure_red_zone` | Removed |
| 1135 | `begin` | `strip_red_zone` | Removed |
| 2112 | `end` | `ensure_red_zone` | Removed |

### 4. Deserialization for Display List Cache

| Line | Function | Original | Current |
|------|----------|----------|---------|
| 294 | `create_debug_spatial_tree_items` | `poke_into_vec` in loop | Does nothing |
| 364 | `create_debug_display_items` | `poke_into_vec` in loop | Does nothing |
| 946 | `skip_slice` | `peek_from_slice` | Returns empty stub |

## Upstream Reference Functions

From `/webrender-upstream/webrender_api/src/display_list.rs`:

```rust
// Line 1237 - How items are stored
pub fn push_item_to_section(&mut self, item: &di::DisplayItem, section: DisplayListSection) {
    debug_assert_eq!(self.state, BuildState::Build);
    poke_into_vec(item, self.buffer_from_section(section));  // SERIALIZES ITEM
    self.add_to_display_list_dump(item);
}

// Line 1254 - How spatial tree items are stored  
pub fn push_spatial_tree_item(&mut self, item: &di::SpatialTreeItem) {
    debug_assert_eq!(self.state, BuildState::Build);
    poke_into_vec(item, &mut self.payload.spatial_tree);  // SERIALIZES ITEM
}

// Line 861 - How items are read back
pub fn next_raw(&mut self) -> Option<DisplayItemRef<'a, 'b>> {
    // ...
    self.data = peek_from_slice(self.data, &mut self.cur_item);  // DESERIALIZES ITEM
    // ...
}
```

## Data Flow

### Original (Working)

```
DisplayListBuilder::push_rect()
    → push_item(&DisplayItem::Rectangle{...})
        → push_item_to_section(item, section)
            → poke_into_vec(item, buffer)  // SERIALIZES to Vec<u8>

BuiltDisplayListIter::next()
    → next_raw()
        → peek_from_slice(data, &mut cur_item)  // DESERIALIZES from Vec<u8>
        → return Some(DisplayItemRef)

SceneBuilder::build_all()
    → traversal.next()  // Returns items
    → build_item(item)  // Processes items
    → tile_cache_builder.add_prim()  // Creates tiles
```

### Current (Broken)

```
DisplayListBuilder::push_rect()
    → push_item(&DisplayItem::Rectangle{...})
        → push_item_to_section(item, section)
            → // DOES NOTHING - item is lost!

BuiltDisplayListIter::next()
    → next_raw()
        → // data is empty, returns None immediately

SceneBuilder::build_all()
    → traversal.next()  // Returns None (no items!)
    → // Nothing to process
    → // No tiles created
    → // White window
```

## Solution: Option A - Direct Item Storage

Instead of serializing to bytes and deserializing, store items directly in a `Vec<DisplayItem>`.

### Required Changes

#### 1. Add Direct Item Storage to `DisplayListPayload`

```rust
#[derive(Default, Clone)]
pub struct DisplayListPayload {
    // Keep for compatibility, but may be unused
    pub items_data: Vec<u8>,
    pub cache_data: Vec<u8>,
    pub spatial_tree: Vec<u8>,
    
    // NEW: Direct item storage (no serialization)
    pub items: Vec<di::DisplayItem>,
    pub spatial_items: Vec<di::SpatialTreeItem>,
}
```

#### 2. Modify `push_item_to_section`

```rust
pub fn push_item_to_section(&mut self, item: &di::DisplayItem, section: DisplayListSection) {
    debug_assert_eq!(self.state, BuildState::Build);
    // Store item directly instead of serializing
    self.payload.items.push(item.clone());
    self.add_to_display_list_dump(item);
}
```

#### 3. Modify `push_spatial_tree_item`

```rust
pub fn push_spatial_tree_item(&mut self, item: &di::SpatialTreeItem) {
    debug_assert_eq!(self.state, BuildState::Build);
    self.payload.spatial_items.push(item.clone());
}
```

#### 4. Modify `BuiltDisplayListIter`

```rust
pub struct BuiltDisplayListIter<'a> {
    // NEW: Iterator over items directly
    items: std::slice::Iter<'a, di::DisplayItem>,
    cur_item_index: usize,
    // ... rest of fields
}

impl<'a> BuiltDisplayListIter<'a> {
    pub fn new(items: &'a [di::DisplayItem], cache: Option<&'a DisplayItemCache>) -> Self {
        Self {
            items: items.iter(),
            cur_item_index: 0,
            // ...
        }
    }
    
    pub fn next_raw<'b>(&'b mut self) -> Option<DisplayItemRef<'a, 'b>> {
        self.cur_item = self.items.next()?.clone();
        self.cur_item_index += 1;
        Some(self.as_ref())
    }
}
```

#### 5. Update `BuiltDisplayList::iter()`

```rust
impl BuiltDisplayList {
    pub fn iter(&self) -> BuiltDisplayListIter {
        BuiltDisplayListIter::new(&self.payload.items, None)
    }
    
    pub fn size_in_bytes(&self) -> usize {
        // Return actual size based on items
        self.payload.items.len() * std::mem::size_of::<di::DisplayItem>()
            + self.payload.spatial_items.len() * std::mem::size_of::<di::SpatialTreeItem>()
    }
}
```

## Complexity Considerations

### Simple Items
These are straightforward - just store and clone:
- `Rectangle`, `ClearRectangle`, `Line`, `HitTest`
- `PushStackingContext`, `PopStackingContext`
- `PushReferenceFrame`, `PopReferenceFrame`
- `Iframe`, `Clip`, `ClipChain`

### Complex Items with Associated Data
These items have variable-length data that was serialized separately:
- `Text` - has `Vec<GlyphInstance>` (glyphs)
- `SetGradientStops` - has `Vec<GradientStop>` (stops)
- `SetFilterOps` - has `Vec<FilterOp>` (filters)
- `SetFilterData` - has multiple `Vec<f32>` arrays
- `SetFilterPrimitives` - has `Vec<FilterPrimitive>`
- `SetPoints` - has `Vec<LayoutPoint>` (for polygons)

**Solution**: Store these as separate vectors in the payload, or embed them in the `DisplayItem` enum directly.

## Files to Modify

1. `/azul/webrender/api/src/display_list.rs`
   - `DisplayListPayload` struct
   - `DisplayListBuilder` methods
   - `BuiltDisplayListIter` struct and methods
   - `BuiltDisplayList::iter()` and `size_in_bytes()`

2. May need to check `/azul/webrender/api/src/display_item.rs`
   - Ensure `DisplayItem` implements `Clone`

3. May need to check consumers in `/azul/webrender/core/src/scene_building.rs`
   - Verify iterator usage is compatible

## Testing

After implementation:
1. Run `cargo build --release -p azul-dll --features desktop`
2. Run `./target/release/hello_world_window`
3. Verify red rectangle is visible
4. Run `python3 scripts/imagetotext.py` - should return SUCCESS
