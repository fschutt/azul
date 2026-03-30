# Text Input Architecture: Dual Update Path Analysis (V4)

**Date**: 2026-03-30

---

## The Two Update Paths

### Path 1: Full DOM Regeneration (`frame_needs_regeneration = true`)

```
User callback returns Update::RefreshDom
  → ShouldRegenerateDomCurrentWindow
  → mark_frame_needs_regeneration()
  → regenerate_layout() calls user's layout_callback
  → Full cascade, layout (solver3), display list generation
  → WebRender transaction with resources
```

**When used**: User explicitly requests DOM rebuild via `Update::RefreshDom`, or
on initial window creation.

### Path 2: Incremental Display List (`display_list_dirty = true`)

```
Text edit → apply_text_changeset()
  → update_text_cache_after_edit() → regenerate_display_list_for_dom()
  → Display list already regenerated from existing layout tree
  → ShouldUpdateDisplayListCurrentWindow
  → mark_display_list_dirty()
  → build_atomic_txn() sends display list to WebRender (no layout callback)
```

**When used**: Text edits that don't change layout dimensions.

### Path 2b: Incremental Relayout (`ShouldIncrementalRelayout`)

```
Text edit where text size changed → needs_relayout = true
  → ShouldIncrementalRelayout
  → incremental_relayout() re-runs solver3 on EXISTING StyledDom
  → No layout callback, but full layout recomputation
```

---

## The Bug: `handle_text_input` on macOS

### Problem

In `events.rs:handle_text_input()`, after `apply_text_changeset()` returns with
dirty nodes, the code correctly computes `ShouldUpdateDisplayListCurrentWindow`.

But then `convert_process_result()` maps `ShouldUpdateDisplayListCurrentWindow` →
`RegenerateDisplayList`, and the caller in `mod.rs` sets
`frame_needs_regeneration = true`. This triggers a full DOM rebuild, calling the
user's layout callback which returns a fresh DOM with the ORIGINAL text,
overwriting the edit.

### Evidence from AZ_RECORD log

```
[17979862us] handle_text_input text='a', record_text_input returned 1 affected nodes
[17981773us] request_redraw Marking view as needing display
[17988562us] regenerate_layout START               ← FULL REBUILD
[17999227us] Calling layout_callback               ← USER CALLBACK CALLED
[17999617us] State migration: 8 node moves         ← ORIGINAL DOM REPLACES EDIT
[18012383us] COMPLETE
```

The layout callback is called with the C data model which has the original text.
State migration matches nodes by structure — the text node gets the old text back.

### Root Cause

`convert_process_result()` in `events.rs:76-85`:

```rust
PER::ShouldUpdateDisplayListCurrentWindow => EventProcessResult::RegenerateDisplayList
```

This incorrectly escalates a display-list-only update to a full DOM rebuild.

---

## The Correct Architecture

### `EventProcessResult` should have 3 levels (not 2):

| Level | Name | What it does |
|-------|------|-------------|
| 1 | `RequestRedraw` | Just repaint (scroll offsets, GPU transforms) |
| 2 | `UpdateDisplayList` | Send already-regenerated display list to renderer |
| 3 | `RegenerateDisplayList` | Full DOM rebuild via layout callback |

### Mapping from `ProcessEventResult`:

| ProcessEventResult | EventProcessResult | frame_needs_regeneration | display_list_dirty |
|---|---|---|---|
| DoNothing | DoNothing | false | false |
| ShouldReRenderCurrentWindow | RequestRedraw | false | false |
| ShouldUpdateDisplayListCurrentWindow | UpdateDisplayList | false | true |
| ShouldIncrementalRelayout | UpdateDisplayList | false | true |
| UpdateHitTesterAndProcessAgain | RegenerateDisplayList | true | - |
| ShouldRegenerateDomCurrentWindow | RegenerateDisplayList | true | - |
| ShouldRegenerateDomAllWindows | RegenerateDisplayList | true | - |

### `ShouldIncrementalRelayout` → `UpdateDisplayList`

This works because `apply_text_changeset()` already calls
`update_text_cache_after_edit()` → `regenerate_display_list_for_dom()`.
The display list is regenerated INSIDE the changeset application. By the time
the event result propagates, the display list is already up-to-date. We just
need to mark `display_list_dirty` so `build_atomic_txn()` sends it to
WebRender.

### Handler code in `mod.rs` keyDown/mouseDown/etc:

```rust
match result {
    EventProcessResult::RegenerateDisplayList => {
        macos_window.common.frame_needs_regeneration = true;
        macos_window.request_redraw();
    }
    EventProcessResult::UpdateDisplayList => {
        macos_window.common.display_list_dirty = true;
        macos_window.request_redraw();
    }
    EventProcessResult::RequestRedraw => {
        macos_window.request_redraw();
    }
    _ => {}
}
```

### `handle_text_input` already does the right thing

After `apply_text_changeset`, the display list is already regenerated internally.
The only thing `handle_text_input` needs to do is:

1. Mark `display_list_dirty = true` (so build_atomic_txn sends it)
2. Call `request_redraw()` (so the view repaints)

It must NOT set `frame_needs_regeneration = true`.

---

## Files to Change

1. **`dll/src/desktop/shell2/macos/events.rs`**:
   - Add `UpdateDisplayList` variant to `EventProcessResult`
   - Fix `convert_process_result` mapping
   - In `handle_text_input`: replace the `convert_process_result` + conditionals
     at the end with direct `display_list_dirty = true` + `request_redraw()`

2. **`dll/src/desktop/shell2/macos/mod.rs`**:
   - All 16+ event handlers (mouseDown, mouseUp, keyDown, etc.) — handle
     `UpdateDisplayList` correctly: set `display_list_dirty`, NOT
     `frame_needs_regeneration`

---

## Additional Bugs Found

### Cursor Not Blinking

`cursor_blink_timer_callback` is started when a contenteditable gains focus. If
focus triggers a full rebuild and the timer is re-registered during state
migration, the timer should survive. But if the full rebuild changes the timer ID
mapping, the blink timer might be lost. Verify timers survive DOM reconciliation.

### Cursor at Start Instead of Click Position

The cursor is initialized at end of text via `initialize_cursor_at_end()` in
`finalize_pending_focus_changes()`. The mouse click position should position the
cursor at the click point via hit-testing. The `ProcessTextSelectionClick` system
change handles this — but it may fire BEFORE focus is set on the first click,
so the text selection click targets the wrong node.

### interpretKeyEvents / NSTextInputClient

The deep audit found that `NSTextInputClient` protocol conformance is NEVER
declared on GLView/CPUView:

```rust
// MISSING from both class definitions:
unsafe impl NSTextInputClient for GLView {}
unsafe impl NSTextInputClient for CPUView {}
```

Without this declaration, macOS doesn't call `insertText:replacementRange:` from
`interpretKeyEvents`. The workaround (direct text input in `handle_key_down`)
works for ASCII but breaks IME (Ctrl+Space, CJK input). For now we skip
`interpretKeyEvents` entirely and handle text directly. Adding the protocol
conformance declaration is the proper fix for IME support.
