# Session 8L: TODO Stubs Fix Plan

## Overview

15 TODO stubs found across the event/layout pipeline. Grouped by dependency
and ordered for implementation.

---

## Group 1: Click State + Selection (foundation for Groups 2-3)

### 1.1 Double/triple-click detection
- **File:** `window.rs:5716`
- **Problem:** `click_count` hardcoded to `1u32`
- **Key finding:** `SelectionManager.update_click_count(node_id, position, time_ms)`
  already exists at `managers/selection.rs:87-136` with 500ms timeout and 5px threshold.
  It returns `u8` (1/2/3). Just wire it up.
- **Fix:** Replace line 5717 `let click_count = 1u32;` with:
  ```rust
  // SelectionManager tracks multi-click state with timeout + distance
  self.selection_manager.update_click_count(dom_node_id, position, time_ms) as u32
  ```
  The word/paragraph selection code (lines 5730-5736) already handles count 2/3.

### 1.2 Selection bounding rect
- **File:** `window.rs:2790`
- **Problem:** Falls back to cursor rect instead of selection bounds
- **Fix:** `build_text_selections_map()` already provides the Range. Call `layout.get_selection_rects(&range)` and compute the bounding union of all rects.

---

## Group 2: Scrolling (depends on layout tree)

### 2.1 Find scrollable ancestor
- **File:** `window.rs:5264`
- **Problem:** Only checks if node itself is scrollable
- **Key finding:** `find_scrollable_ancestor()` already exists at `window.rs:2700`!
  It walks the layout tree and returns the scrollable ancestor's DomNodeId.
- **Fix:** Replace the stub with a call to `self.find_scrollable_ancestor(dom_node_id)`.
  Use the returned ancestor for scroll offset calculations.

### 2.2 Scroll-to-node
- **File:** `window.rs:5201`
- **Problem:** Scrolls to (0,0) instead of target
- **Fix:** Once 2.1 exists:
  1. Get target node's absolute position from `calculated_positions`
  2. Find scrollable ancestor (2.1)
  3. Get ancestor's viewport rect
  4. If target outside viewport, compute scroll offset to bring it into view
  5. Call `scroll_manager.scroll_to(ancestor_node, offset, ...)`

### 2.3 Keyboard scrolling
- **File:** `event.rs:3831`
- **Problem:** `DefaultAction::ScrollFocusedContainer` is empty
- **Fix:** Get focused node → find scrollable ancestor (2.1) → scroll by page/line amount based on key (PageUp/Down = viewport height, Arrow = line height ~20px)

---

## Group 3: Text Editing Features

### 3.1 IME cursor position
- **File:** `window.rs:4529`
- **Problem:** Hardcoded to (0.0, 0.0)
- **Fix:** Use `layout.get_cursor_rect(cursor)` which already exists (used in display_list.rs for painting cursors). Convert to screen coordinates by adding node_pos + window_pos. The `firstRectForCharacterRange` in macos/mod.rs already does this for the IME window, so the code pattern exists — just needs to be ported to the accessibility path at 4529.

### 3.2 Ctrl+D select next occurrence
- **File:** `event.rs:2319`
- **Problem:** Stub, no text search
- **Fix:**
  1. Get primary selection text (or word at cursor if no selection)
  2. Search forward in the node's text content for the next occurrence
  3. Create a new cursor at the match position via `mc.add_selection(range)`
  4. Mark dirty

### 3.3 Route switching
- **File:** `event.rs:2033`
- **Problem:** Stub prints message, doesn't switch
- **Architecture:** `apply_user_change()` on `PlatformWindowHandler` has no access to
  `AppConfig`. Need to add `Arc<RouteVec>` to `CommonWindowState`, set at window creation
  from `shell2/run.rs`. Then handler can look up route and swap `layout_callback`.
- **Risk:** Touches all 5 platform window creation sites (Win32, macOS, X11, Wayland, headless).

---

## Group 4: Accessibility (independent)

### 4.1 Context menu accessibility
- **File:** `window.rs:4226`
- **Fix:** Generate a synthetic right-click event at the node's position. Use the existing mouse event pipeline.

### 4.2 Tooltip/custom actions
- **Files:** `window.rs:4307, 4311`
- **Status:** Genuine future features, low priority.

---

## Group 5: Resource Management (independent)

### 5.1 Font/image GC
- **Files:** `window.rs:1605, 1615`
- **Fix:** Scan `styled_dom.node_data` for font-family CSS values and image source URLs. Return the set of referenced resource IDs. Callers can diff against loaded resources to find unused ones.
- **Low priority:** Only matters if apps load many fonts/images dynamically.

---

## Execution Order

```
1.1 click state  ─┐
                   ├─► 1.2 selection bounding rect
                   │
2.1 scroll ancestor ─┬─► 2.2 scroll-to-node
                     ├─► 2.3 keyboard scrolling
                     │
3.1 IME cursor pos  (independent)
3.2 Ctrl+D          (independent)
3.3 route switching  (independent, complex)
4.1 context menu    (independent, low priority)
5.1 font/image GC   (independent, low priority)
```

**Recommended order:** 1.1 → 2.1 → 2.2 → 2.3 → 3.1 → 1.2 → 3.2
