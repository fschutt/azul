# WebRender Clipping Analysis Report

## Executive Summary

**Problem**: Content inside a scroll container is not being visually clipped, even though the clip infrastructure appears to be set up correctly.

**Key Finding**: The debug output shows that:
1. Clips ARE being defined correctly in WebRender
2. The `local_clip_rect` is being set properly
3. The clip chain is being built with proper parent relationships
4. **BUT**: The primitives are not being rejected or masked by the clips

## Current Debug Output Analysis

### Clip Definition (Correct ✅)
```
[WR SCENE] RectClip: id=ClipId(1), spatial_id=SpatialId(1), clip_rect=Box2D((68,68), (836,636))
[WR SCENE] ClipChain: id=ClipChainId(0), parent=None
[WR SCENE] RectClip: id=ClipId(2), spatial_id=SpatialId(2), clip_rect=Box2D((0,0), (768,568))
[WR SCENE] ClipChain: id=ClipChainId(1), parent=Some(ClipChainId(0))
```

### Primitive Placement (Correct ✅)
```
[WR SCENE] Rectangle: bounds=Box2D((20,1640), (780,1800)), clip_chain_id=ClipChainId(1), spatial_id=SpatialId(2)
```
- Rectangle at y=1640-1800 is outside the clip rect (0,0)-(768,568)
- It should be clipped!

### Clip Chain Building (Suspicious ⚠️)
```
[WR CLIP] build_clip_chain_instance: local_prim_rect=Box2D((20,1640), (780,1800)), local_clip_rect=Box2D((20,1640), (780,1800))
[WR CLIP] local_bounding_rect (intersection)=Box2D((20,1640), (780,1800))
```

**PROBLEM IDENTIFIED**: The `local_clip_rect` equals the `local_prim_rect`! 

This means the clip is NOT being applied. The clip rect should be `(0,0)-(768,568)` but it's `(20,1640)-(780,1800)`.

## Root Cause Analysis

### ClipTree / ClipLeaf System

The `local_clip_rect` comes from `clip_leaf.local_clip_rect` in `set_active_clips()`:

```rust
pub fn set_active_clips(...) {
    let clip_leaf = clip_tree.get_leaf(clip_leaf_id);
    let mut local_clip_rect = clip_leaf.local_clip_rect;  // <-- THIS IS THE PROBLEM
    ...
}
```

The `clip_leaf.local_clip_rect` is initialized with the primitive's own clip_rect from `CommonItemProperties`, NOT from the parent clip chain!

### Where the ClipLeaf is Created

In `scene_building.rs`, when processing `DisplayItem::Rectangle`:

```rust
let current_clip_chain_id = self.get_clip_chain(item.clip_chain_id());
...
let prim_info = CommonItemProperties {
    clip_rect: info.clip_rect,  // <-- This is the primitive's own clip_rect
    clip_chain_id: current_clip_chain_id,
    ...
};
```

The `info.clip_rect` is set to the primitive's bounds in our compositor code:
```rust
let info = CommonItemProperties {
    clip_rect: rect,  // <-- We set clip_rect = rect (the full primitive bounds)
    clip_chain_id: current_clip_chain_id,
    ...
};
```

## The Fix

The issue is that `CommonItemProperties.clip_rect` should be the INTERSECTION of the primitive bounds with the active clip region, not just the primitive bounds.

### Option 1: Fix in compositor2.rs (Recommended)

For items inside a scroll frame, the `clip_rect` in `CommonItemProperties` should be clipped to the scroll frame's viewport:

```rust
// Instead of:
let info = CommonItemProperties {
    clip_rect: rect,  // Full primitive rect
    ...
};

// Should be:
let clipped_rect = rect.intersection(&current_clip_bounds).unwrap_or(LayoutRect::zero());
let info = CommonItemProperties {
    clip_rect: clipped_rect,  // Primitive rect clipped to viewport
    ...
};
```

### Option 2: Let WebRender Handle It

WebRender's clip system should handle this through `ClipChainId`, but the `clip_rect` field is used as an early-out optimization. If `clip_rect` is larger than the actual clip region, WebRender may skip clipping.

## WebRender Clipping Pipeline

### 1. Scene Building (`scene_building.rs`)
- Parses display list items
- Creates clip nodes via `define_clip_rect()`
- Creates clip chains via `define_clip_chain()`
- Associates primitives with clip chains

### 2. Clip Store (`clip.rs`)
- `set_active_clips()` - Walks the clip tree to build active clip list
- `build_clip_chain_instance()` - Creates optimized clip chain for rendering
- `get_clip_result()` - Determines Accept/Reject/Partial for each clip

### 3. Visibility (`visibility.rs`)
- Calls `build_clip_chain_instance()` for each primitive
- Rejects primitives that are completely clipped
- Creates clip masks for partially clipped primitives

### 4. GPU Rendering
- Clip masks are texture-based alpha masks
- Simple rect clips can use vertex shader clipping
- Complex clips require mask rendering

## Key Data Structures

```
ClipTree
├── ClipNodeId (0) - Root
│   └── ClipNodeId (1)
│       ├── handle: ClipDataHandle -> ClipNode with clip_rect
│       └── parent: ClipNodeId (0)
│
└── ClipLeafId (0)
    ├── node_id: ClipNodeId (1)
    └── local_clip_rect: LayoutRect  <-- THE PROBLEM SOURCE
```

## Debug Trace Points

1. **Scene Building** (`scene_building.rs`):
   - `[WR SCENE] RectClip:` - When a clip rect is defined
   - `[WR SCENE] ClipChain:` - When a clip chain is defined
   - `[WR SCENE] Rectangle:` - When a rectangle primitive is added

2. **Clip Store** (`clip.rs`):
   - `[WR SET_ACTIVE_CLIPS]` - When clips are activated for a primitive
   - `[WR CLIP CHAIN]` - When a clip node is added to the chain
   - `[WR CLIP] build_clip_chain_instance:` - When building the final clip instance
   - `[WR CLIP] REJECTED:` - When a primitive is fully clipped

3. **Compositor** (`compositor2.rs`):
   - `[CLIP DEBUG] Rect:` - Raw and adjusted coordinates
   - `[CLIP DEBUG] PushScrollFrame:` - Scroll frame definition

## Next Steps

1. **Add logging to `set_active_clips()`** to see the clip tree traversal
2. **Check `clip_leaf.local_clip_rect`** - This might be set incorrectly
3. **Trace `CommonItemProperties.clip_rect`** - Ensure it's being intersected with clip bounds
4. **Check spatial node coordinate transforms** - The clip might be in wrong coordinate space

## Files to Investigate

1. `/webrender/core/src/clip.rs` - ClipStore, set_active_clips, build_clip_chain_instance
2. `/webrender/core/src/scene_building.rs` - How clips are defined and associated
3. `/webrender/core/src/visibility.rs` - How primitives are culled
4. `/dll/src/desktop/compositor2.rs` - How we build the display list

## Hypothesis

The `ClipLeafId` assigned to each primitive determines which clip chain applies. The issue is that:

1. We define a scroll frame clip at `(0,0)-(768,568)` in scroll space
2. We push rectangles with `clip_chain_id` pointing to this clip
3. **BUT** the `ClipLeaf.local_clip_rect` is set to the primitive's own bounds
4. WebRender uses `local_clip_rect` as the primary clip source
5. The clip chain clips are only used for masking, not rejection

The fix should ensure that the `clip_rect` in `CommonItemProperties` is properly constrained to the scroll frame viewport.
