# Hit-Test Tag System Analysis & Migration Plan

## Date: January 19, 2026

## Executive Summary

The Azul GUI framework has a **critical bug** where all mouse clicks are incorrectly interpreted as scrollbar hits, preventing button callbacks from firing. This document analyzes the problem, the existing (unused) solution, and proposes a migration plan.

---

## 1. Current Problem

### Symptoms
1. **Button clicks don't work**: Clicking the "Update counter" button doesn't trigger the callback
2. **Selection workaround**: Selecting text first, then releasing over the button, triggers the callback
3. **Debug output shows**: `[DEBUG handle_mouse_down] HIT SCROLLBAR, returning early`

### Root Cause

In [dll/src/desktop/wr_translate2.rs](../dll/src/desktop/wr_translate2.rs), the function `translate_item_tag_to_scrollbar_hit_id()` incorrectly identifies **all** hit-test tags as scrollbar hits:

```rust
// BUGGY CODE (before fix attempt):
let component_type = (tag_value >> 62) & 0x3;
match component_type {
    0 => Some(ScrollbarHitId::VerticalTrack(...)),  // ← ALL tags with upper bits = 0!
    1 => Some(ScrollbarHitId::VerticalThumb(...)),
    2 => Some(ScrollbarHitId::HorizontalTrack(...)),
    3 => Some(ScrollbarHitId::HorizontalThumb(...)),
    _ => None,
}
```

**Why this is wrong:**
- DOM node tags are simple sequential numbers: `1, 2, 3, ..., 673`
- These small numbers have `(tag >> 62) & 0x3 == 0`
- `0` maps to `VerticalTrack`, so **every normal click is treated as a scrollbar hit!**

### Current Fix Attempt

A bit-61 marker was added to identify scrollbar tags:

```rust
// In wr_translate_scrollbar_hit_id():
const SCROLLBAR_MARKER: u64 = 1u64 << 61;
let tag = ... | SCROLLBAR_MARKER;

// In translate_item_tag_to_scrollbar_hit_id():
if (tag_value & SCROLLBAR_MARKER) == 0 {
    return None;  // Not a scrollbar
}
```

**Status**: Partially implemented, not yet tested.

---

## 2. The Better Solution Already Exists!

There's an **unused but complete type-safe system** in [core/src/hit_test_tag.rs](../core/src/hit_test_tag.rs):

### WebRender ItemTag Format
```
WebRender ItemTag = (u64, u16)
                    ↑      ↑
                    |      └── Namespace/Type marker
                    └── Payload data
```

### Namespace Design in hit_test_tag.rs

```rust
/// Marker for DOM node tags (regular UI elements)
pub const TAG_TYPE_DOM_NODE: u16 = 0x0100;

/// Marker for scrollbar component tags
pub const TAG_TYPE_SCROLLBAR: u16 = 0x0200;

/// Reserved for future use
pub const TAG_TYPE_RESERVED: u16 = 0x0300;
```

### HitTestTag Enum

```rust
pub enum HitTestTag {
    /// Regular DOM node (button, div, text container)
    DomNode { tag_id: TagId },
    
    /// Scrollbar component (track or thumb)
    Scrollbar { dom_id: DomId, node_id: NodeId, component: ScrollbarComponent },
}

impl HitTestTag {
    pub fn to_item_tag(&self) -> (u64, u16) {
        match self {
            HitTestTag::DomNode { tag_id } => {
                (tag_id.inner, TAG_TYPE_DOM_NODE)  // Uses u16 for namespace!
            }
            HitTestTag::Scrollbar { dom_id, node_id, component } => {
                let value = (dom_id.inner << 32) | node_id.index();
                (value, TAG_TYPE_SCROLLBAR | (component as u16))
            }
        }
    }
    
    pub fn from_item_tag(tag: (u64, u16)) -> Option<Self> {
        let type_marker = tag.1 & 0xFF00;
        match type_marker {
            TAG_TYPE_DOM_NODE => Some(HitTestTag::DomNode { ... }),
            TAG_TYPE_SCROLLBAR => Some(HitTestTag::Scrollbar { ... }),
            _ => None,  // Unknown type
        }
    }
}
```

---

## 3. Current Architecture

### Tag ID Generation Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│ DOM Creation (hello-world.c)                                        │
├─────────────────────────────────────────────────────────────────────┤
│ body (node_id=0) → label (node_id=1) → button (node_id=2)          │
│                                         └→ button_text (node_id=3)  │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Styling (styled_dom.rs)                                             │
├─────────────────────────────────────────────────────────────────────┤
│ Nodes with callbacks/focus/cursor get sequential TagId:            │
│   body    → TagId { inner: 7 }  (has :hover pseudo-class)          │
│   label   → TagId { inner: 8 }  (is selectable text)               │
│   button  → TagId { inner: 9 }  (has On::Click callback)           │
│   btn_txt → TagId { inner: 10 } (is selectable text)               │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Display List Generation (display_list.rs)                           │
├─────────────────────────────────────────────────────────────────────┤
│ push_hit_test_area(bounds, tag_id: u64)                            │
│   → Currently passes TagId.inner directly!                         │
│   → WebRender stores as ItemTag = (tag_id.inner, 0)                │
│                                                 ↑                   │
│                                          u16 = 0 (no namespace!)   │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Hit Testing (wr_translate2.rs)                                      │
├─────────────────────────────────────────────────────────────────────┤
│ WebRender returns: ItemTag = (673, 0)                              │
│                                                                     │
│ translate_item_tag_to_scrollbar_hit_id((673, 0)):                  │
│   component_type = (673 >> 62) & 0x3 = 0                           │
│   0 → VerticalTrack  ← BUG! Normal tag interpreted as scrollbar!  │
└─────────────────────────────────────────────────────────────────────┘
```

### The Problem: Missing Namespace

Currently, all tags use `u16 = 0`, making them indistinguishable:

| What | tag.0 | tag.1 | Problem |
|------|-------|-------|---------|
| DOM Node (tag 673) | 673 | 0 | ← No type marker |
| Scrollbar (track) | encoded | 0 | ← Same as above! |

With the new system from `hit_test_tag.rs`:

| What | tag.0 | tag.1 | Distinguishable |
|------|-------|-------|-----------------|
| DOM Node (tag 673) | 673 | 0x0100 | ✓ |
| Scrollbar (track) | encoded | 0x0200 | ✓ |

---

## 4. Files That Need Changes

### Core Changes

| File | Current State | Needed Change |
|------|---------------|---------------|
| `core/src/hit_test_tag.rs` | ✅ Complete, tested | Export in `lib.rs` |
| `core/src/lib.rs` | Missing export | Add `pub mod hit_test_tag;` |

### Display List Generation

| File | Current State | Needed Change |
|------|---------------|---------------|
| `layout/src/solver3/display_list.rs` | Uses raw `u64` for tag | Use `HitTestTag::DomNode` |

```rust
// Current (display_list.rs:2807):
fn get_tag_id(dom: &StyledDom, id: Option<NodeId>) -> Option<DisplayListTagId> {
    Some(tag_id.inner)  // ← Raw u64
}

// Should be:
fn get_tag_id(dom: &StyledDom, id: Option<NodeId>) -> Option<HitTestTag> {
    Some(HitTestTag::DomNode { tag_id })
}
```

### WebRender Translation

| File | Current State | Needed Change |
|------|---------------|---------------|
| `dll/src/desktop/wr_translate2.rs` | Bit-hacking | Use `HitTestTag::from_item_tag()` |

```rust
// Current:
pub fn translate_item_tag_to_scrollbar_hit_id(tag: ItemTag) -> Option<ScrollbarHitId> {
    // Complex bit manipulation with potential bugs
}

// Should be:
pub fn translate_item_tag(tag: ItemTag) -> Option<HitTestTag> {
    HitTestTag::from_item_tag(tag)  // ← Type-safe!
}
```

### Event Processing

| File | Current State | Needed Change |
|------|---------------|---------------|
| `dll/src/desktop/shell2/common/event_v2.rs` | Uses `perform_scrollbar_hit_test` | Check `HitTestTag` variant |

```rust
// Current (event_v2.rs:2357):
fn perform_scrollbar_hit_test(&self, position: LogicalPosition) -> Option<ScrollbarHitId> {
    for item in hit_result.items.iter() {
        if let Some(scrollbar_id) = translate_item_tag_to_scrollbar_hit_id(item.tag) {
            return Some(scrollbar_id);
        }
    }
    None
}

// Should be:
fn classify_hit_test_tag(&self, position: LogicalPosition) -> Vec<HitTestTag> {
    hit_result.items.iter()
        .filter_map(|item| HitTestTag::from_item_tag(item.tag))
        .collect()
}
```

---

## 5. Proposed Namespace Scheme

Using the `u16` field of WebRender's ItemTag for namespaces:

```
┌─────────────────────────────────────────────────────────────────────┐
│ u16 Layout (tag.1)                                                  │
├─────────────────────────────────────────────────────────────────────┤
│ Bits 15-8: Namespace Type     Bits 7-0: Subtype/Flags              │
│ ┌─────────────────────────────────────────────────────────────────┐│
│ │ 0x01__ = DOM Node           0x00 = default                      ││
│ │ 0x02__ = Scrollbar          0x00-0x07 = component type          ││
│ │ 0x03__ = Selection          0x00 = start handle, 0x01 = end     ││
│ │ 0x04__ = Resize Handle      0x00-0x07 = position (N/NE/E/SE/...)││
│ │ 0x05__ = Reserved           (future use)                         ││
│ └─────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│ u64 Layout (tag.0) - Depends on namespace                          │
├─────────────────────────────────────────────────────────────────────┤
│ DOM Node:                                                           │
│   [63:0] = TagId.inner (sequential counter)                        │
│                                                                     │
│ Scrollbar:                                                          │
│   [63:32] = DomId.inner                                            │
│   [31:0]  = NodeId.index()                                         │
│                                                                     │
│ Selection:                                                          │
│   [63:32] = DomId.inner                                            │
│   [31:0]  = NodeId.index() of text container                       │
│                                                                     │
│ Resize Handle:                                                      │
│   [63:32] = DomId.inner                                            │
│   [31:0]  = NodeId.index() of resizable element                    │
└─────────────────────────────────────────────────────────────────────┘
```

### Extended HitTestTag Enum (Future)

```rust
pub enum HitTestTag {
    /// Regular DOM node
    DomNode { tag_id: TagId },
    
    /// Scrollbar component
    Scrollbar { dom_id: DomId, node_id: NodeId, component: ScrollbarComponent },
    
    /// Text selection handle
    SelectionHandle { dom_id: DomId, node_id: NodeId, is_end: bool },
    
    /// Resize handle for resizable elements
    ResizeHandle { dom_id: DomId, node_id: NodeId, position: ResizePosition },
}

pub enum ScrollbarComponent {
    VerticalTrack = 0,
    VerticalThumb = 1,
    HorizontalTrack = 2,
    HorizontalThumb = 3,
    // Future: scroll buttons
    VerticalUpButton = 4,
    VerticalDownButton = 5,
    HorizontalLeftButton = 6,
    HorizontalRightButton = 7,
}

pub enum ResizePosition {
    North = 0, NorthEast = 1, East = 2, SouthEast = 3,
    South = 4, SouthWest = 5, West = 6, NorthWest = 7,
}
```

---

## 6. Migration Plan

### Phase 1: Fix Immediate Bug (Today)
1. ✅ Export `hit_test_tag` module from `core/src/lib.rs`
2. Update `display_list.rs` to use `HitTestTag::DomNode` with proper `tag.1`
3. Update `wr_translate2.rs` to use `HitTestTag::from_item_tag()`
4. Test that button clicks work

### Phase 2: Clean Up Legacy Code (Next)
1. Remove old `translate_item_tag_to_scrollbar_hit_id()` function
2. Remove old `wr_translate_scrollbar_hit_id()` function
3. Update all callers to use `HitTestTag`

### Phase 3: Add New Features (Future)
1. Add `SelectionHandle` variant
2. Add `ResizeHandle` variant
3. Implement corresponding display list generation

---

## 7. Test Cases

### Critical Tests
```rust
#[test]
fn dom_node_tag_not_confused_with_scrollbar() {
    let dom_tag = HitTestTag::DomNode { tag_id: TagId { inner: 673 } };
    let item_tag = dom_tag.to_item_tag();
    
    assert_eq!(item_tag.1, TAG_TYPE_DOM_NODE);  // u16 = 0x0100, NOT 0
    
    let decoded = HitTestTag::from_item_tag(item_tag).unwrap();
    assert!(decoded.is_dom_node());
    assert!(!decoded.is_scrollbar());
}

#[test]
fn scrollbar_tag_correctly_identified() {
    let sb_tag = HitTestTag::Scrollbar { 
        dom_id: DomId { inner: 0 }, 
        node_id: NodeId::new(5),
        component: ScrollbarComponent::VerticalThumb,
    };
    let item_tag = sb_tag.to_item_tag();
    
    assert_eq!(item_tag.1 & 0xFF00, TAG_TYPE_SCROLLBAR);
    
    let decoded = HitTestTag::from_item_tag(item_tag).unwrap();
    assert!(decoded.is_scrollbar());
    assert!(!decoded.is_dom_node());
}
```

---

## 8. Current Diff Status

The following changes have been made but **not yet integrated**:

### File: `dll/src/desktop/wr_translate2.rs`
- Added `SCROLLBAR_MARKER` bit (bit 61) to scrollbar tag encoding
- Added check in decoding to reject tags without the marker
- **Status**: Partial fix, needs to be replaced with `HitTestTag` system

### File: `core/src/hit_test_tag.rs`
- Complete type-safe `HitTestTag` system
- Proper namespace markers in `tag.1` field
- Unit tests included
- **Status**: Complete but not exported/used

### Files Needing Integration:
1. `core/src/lib.rs` - Export `hit_test_tag` module
2. `layout/src/solver3/display_list.rs` - Use `HitTestTag::DomNode`
3. `dll/src/desktop/shell2/common/event_v2.rs` - Use `HitTestTag::from_item_tag()`
4. `dll/src/desktop/wr_translate2.rs` - Remove old functions, use new system

---

## 9. Additional Issue: Body Margin Triggering Scrollbar

The screenshot shows the body (green background) is correctly positioned at (8, 8) with size 624×53.5. However, the body's content doesn't extend to fill the window, creating a scrollable area.

**Cause**: The body has `margin: 8px` (UA default), which is applied correctly, but the content height (53.5px) is less than the viewport height (480px), so no scrollbar should appear.

**Investigation needed**: 
- Why does `perform_scrollbar_hit_test()` find a scrollbar when there's no visible scrollbar?
- Is there an invisible scrollbar hit area being generated?

---

## 10. Conclusion

The fix is straightforward once we understand the architecture:

1. **Use the u16 field** for namespace/type identification
2. **The solution already exists** in `hit_test_tag.rs`
3. **Integration is the remaining work**

The namespace approach using `tag.1` is cleaner than bit-hacking in `tag.0` because:
- It's immediately visible in debug output
- It leaves the full 64 bits of `tag.0` for payload
- It's extensible without changing existing code
- It's type-safe through the `HitTestTag` enum
