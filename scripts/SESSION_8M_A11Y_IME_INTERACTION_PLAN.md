# Session 8M: Accessibility + IME Interaction Plan

## Overview

After fixing the TODO stubs in Session 8L, several accessibility gaps remained.
This session addresses: a11y context menus, a11y scrolling, cursor/selection
exposure, stale contenteditable bounds, and a11y tree update triggers.

---

## Current Architecture

```
Screen Reader / Voice Control
         |
  accesskit ActionRequest (platform-specific adapter)
         |
  poll_accessibility_actions()          [platform event loop]
         |
  LayoutWindow::process_accessibility_action()   [window.rs ~line 4095]
         |  returns BTreeMap<DomNodeId, (Vec<EventFilter>, bool)>
         v
  Event dispatcher fires synthetic events + marks nodes dirty
         |
  After layout/display-list: update_a11y_tree() rebuilds a11y tree
         |
  Platform adapter pushes TreeUpdate to native a11y APIs
```

**Key files:**
- `layout/src/managers/a11y.rs` — tree construction, action decoding
- `layout/src/window.rs` — action processing, update_a11y_tree()
- `dll/src/desktop/shell2/{macos,windows,linux}/accessibility.rs` — platform adapters

---

## Completed Fixes

### 1. A11y Context Menu (was stub 4.1) -- DONE

**Problem:** `AccessibilityAction::ShowContextMenu` detected context menus but
couldn't trigger them.

**Fix:** Returns synthetic `RightMouseDown` in `affected_nodes` so the event
dispatcher triggers the normal platform context-menu path.

### 2. A11y Directional Scrolling -- DONE

**Problem:** `ScrollUp/Down/Left/Right` used hardcoded 100px and scrolled the
target node instead of its scrollable ancestor.

**Fix:** Uses `find_scrollable_ancestor()` + viewport-relative amounts (75%).

### 3. Cursor/Selection Exposure to Screen Readers -- DONE

**Problem:** `cursor_offset` was captured but `set_caret_index()` was never
called.  Screen readers couldn't track cursor position.

**Fix:** Added `set_text_selection()` with degenerate selection (anchor==focus)
in `update_tree()`.  Converts byte offset to UTF-16 character index per
accesskit spec.

### 4. Contenteditable Stale Bounds After Resize -- DONE

**Problem:** `update_text_cache_after_edit()` read cached `UnifiedConstraints`
from `CachedInlineLayout` which had stale `available_width` if the container
was resized since the last full layout.

**Fix:** Before re-layout, reads the parent node's current `used_size` from
the layout tree and updates `constraints.available_width` to reflect the
current content-box width (accounting for padding + border).

### 5. A11y Tree Update on Display-List-Only Changes -- DONE

**Problem:** `update_a11y_tree()` was only called inside
`layout_and_generate_display_list()`.  Display-list-only updates (text edits,
cursor moves) skipped a11y, so screen readers saw stale state.

**Fix:** Extracted `update_a11y_tree()` as a public method on `LayoutWindow`.
Now called from both `layout_and_generate_display_list()` (after full layout)
and `regenerate_display_list_for_dom()` (after display-list-only changes).

---

## Remaining Work (Future Sessions)

### Wayland IME Activation

`zwp_text_input_v3` is bound but `enable()` is never called when
contenteditable gains focus.  Needs:
- `enable()` + `set_content_type()` + `set_cursor_rectangle()` + `commit()`
  on contenteditable focus
- `disable()` + `commit()` on blur
- `set_cursor_rectangle()` + `commit()` on cursor move

**Files:** `dll/src/desktop/shell2/linux/wayland/{mod,events}.rs`
**Risk:** Medium. Needs Wayland compositor with IME support to test.

### Incremental A11y Updates

Currently `update_a11y_tree()` rebuilds the entire tree.  For performance on
large DOMs, use incremental `TreeUpdate` with only changed nodes.  accesskit
supports this natively.

### A11y Text Input Audit

`SetTextSelection`, `ReplaceSelectedText`, `SetValue` actions need audit for:
- Range selection support in `SetTextSelection`
- Undo/redo recording for a11y-driven edits
- Proper `needs_relayout` flags

### Route Switching (stub 3.3)

`CallbackChange::SwitchRoute` requires `Arc<RouteVec>` on `CommonWindowState`
and modifying 5 platform window creation sites.  Architectural change, track
separately.
