# Session 8D: ContentEditable Bugs + Widget E2E Tests

**Date**: 2026-03-30
**Branch**: `layout-debug-clean`

---

## Bugs Found

### BUG 1: Text not clipped outside contenteditable container (HIGH)

**Symptom**: When typing in the single-line text input, text overflows beyond
the input's boundary and is visible outside the container.

**Root cause**: In `layout/src/solver3/display_list.rs`, the text clip_rect
is set to the EXPANDED content_box_rect (which includes full scroll content
size), not the viewport-visible portion. Specifically:

1. Line ~3531: `content_box_rect` is expanded to full scroll content size
2. Line ~4078: `clip_rect = container_rect` uses the expanded rect
3. Even though `push_node_clips()` correctly pushes overflow clips,
   the text rendering uses the unclipped rectangle

**Fix**: In `paint_inline_content()`, if the node has `overflow: hidden` or
`overflow: scroll`, clip the text render rect to the node's padding-box
boundary (not the expanded scroll content size).

**Files**: `layout/src/solver3/display_list.rs`

---

### BUG 2: Scroll only updates after scroll finishes (HIGH)

**Symptom**: When scrolling the multi-line textarea with trackpad, the
content doesn't visually update during the scroll gesture — it jumps to the
final position only after the scroll stops.

**Root cause**: The scroll physics timer callback in
`layout/src/scroll_timer.rs:409-413` returns `Update::DoNothing`:

```rust
TimerCallbackReturn {
    should_update: Update::DoNothing,
    should_terminate: TerminateTimer::Continue,
}
```

This means the platform code never requests a redraw during the scroll
animation. The display only updates when an external event triggers a
repaint.

**Fix**: The scroll timer should return `Update::RefreshDom` (or a new
`Update::RedrawOnly` variant) when scroll offsets change. Alternatively,
the platform event loop should detect scroll offset changes from the timer
and call `request_redraw()`.

**Files**: `layout/src/scroll_timer.rs`, platform event loops

---

### BUG 3: Text input moves cursor but text doesn't appear + macOS error sound (HIGH)

**Symptom**: When typing in the first text input, the cursor blinks and
moves, but the typed characters don't appear. macOS plays the system error
"bonk" sound on each keystroke.

**Root cause**: Dual text input path conflict on macOS:

1. `keyDown:` calls `interpretKeyEvents:` (line 333-336 in mod.rs)
2. `interpretKeyEvents:` triggers the NSTextInputClient protocol
3. ALSO: `handle_key_down()` in events.rs:424-443 handles printable
   characters directly as a "workaround" for objc2 protocol issues
4. The text gets inserted through the direct path but the view
   doesn't properly acknowledge the input to macOS
5. macOS plays NSBeep because the event wasn't "consumed" properly

**Fix**: Choose ONE text input path:
- Option A: Fix NSTextInputClient protocol conformance so
  `interpretKeyEvents:` → `insertText:` works correctly. Remove the
  direct text input workaround.
- Option B: Don't call `interpretKeyEvents:` at all. Handle all text
  input directly in `handle_key_down()`. This breaks IME support.

Option A is correct. The issue is likely that `insertText:replacementRange:`
is called but the event is also processed in `handle_key_down`, causing
double-processing.

**Files**: `dll/src/desktop/shell2/macos/mod.rs` (keyDown),
          `dll/src/desktop/shell2/macos/events.rs` (handle_key_down)

---

### BUG 4: Ctrl+Space doesn't activate IME (MEDIUM)

**Symptom**: Pressing Ctrl+Space should bring up the macOS input source
selector or IME candidate window, but nothing happens.

**Root cause**: Related to Bug 3. `interpretKeyEvents:` is called but the
NSTextInputClient protocol implementation is incomplete. macOS needs:
- Proper `selectedRange` (currently returns fixed location=0)
- Proper `firstRectForCharacterRange:` in screen coordinates
- Complete protocol conformance for the IME activation flow

When Ctrl+Space hits `handle_key_down`, the `has_ctrl` check (line 431)
prevents text insertion but doesn't prevent the event from being consumed.

**Fix**: Same as Bug 3 Option A — fix NSTextInputClient protocol conformance.

**Files**: Same as Bug 3

---

### BUG 5: Text selection doesn't work in contenteditable (MEDIUM)

**Symptom**: Shift+Arrow or mouse drag selection doesn't visually highlight
text in contenteditable elements.

**Root cause (suspected)**: The selection state may be stored on the wrong
DOM node — the wrapper div instead of the text node. The selection system
uses DomNodeId to track which node has the selection, and if there's an ID
mismatch between the node that receives the mouse/keyboard event and the
node that contains the text layout, the selection painting won't find any
text to highlight.

Key suspects:
- `SelectionManager.text_selections` maps `DomId → TextSelection`
- The hit-test may return the parent div, but text layout lives on the
  IFC root or text child node
- `paint_selections()` in display_list.rs checks if the node matches the
  selection's node ID — if these don't align, nothing is painted

**Diagnostic**: Use AZ_RECORD to log:
1. Which node receives the mouse down event
2. Which node the SelectionManager stores the selection on
3. Which node `paint_selections()` checks
4. Whether they match

**Files**: `layout/src/managers/selection.rs`,
          `layout/src/solver3/display_list.rs` (paint_selections),
          `dll/src/desktop/shell2/common/event.rs` (text selection click)

---

## E2E Test Files Created

| File | Mode | Tests |
|------|------|-------|
| `tests/e2e/contenteditable_overflow_test.json` | Headless | 8 tests: focus, typing, overflow clipping, scroll, backspace |
| `tests/e2e/widgets_headless_test.json` | Headless | 7 tests: render, focus, click, typing, checkbox, resize |
| `tests/e2e/widgets_native_test.json` | Native | 7 tests: render, hover, click, typing, checkbox, dropdown |

---

## Fix Priority

| # | Bug | Severity | Effort | Blocks |
|---|-----|----------|--------|--------|
| 1 | Text overflow clipping | HIGH | 2-3 hrs | Visual correctness |
| 2 | Scroll redraw timing | HIGH | 1-2 hrs | Scroll UX |
| 3 | Text input + error sound | HIGH | 2-3 hrs | Text input UX |
| 4 | Ctrl+Space IME | MEDIUM | Part of #3 | IME |
| 5 | Text selection painting | MEDIUM | 2-3 hrs | Selection UX |

---

## Key Files

| Component | File |
|-----------|------|
| Text clipping | `layout/src/solver3/display_list.rs` |
| Scroll timer | `layout/src/scroll_timer.rs` |
| macOS keyDown | `dll/src/desktop/shell2/macos/mod.rs` |
| macOS key handling | `dll/src/desktop/shell2/macos/events.rs` |
| Selection manager | `layout/src/managers/selection.rs` |
| Selection painting | `layout/src/solver3/display_list.rs` (paint_selections) |
| Text selection click | `dll/src/desktop/shell2/common/event.rs` |
