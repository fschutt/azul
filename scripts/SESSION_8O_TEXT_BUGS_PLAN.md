# Session 8O: Text Editing Bugs + Word Wrap + Wayland Surrounding Text

## Bugs Found

### 1. Backspace with selection deletes too much (CRITICAL)
- **File:** `layout/src/text3/edit.rs:98-124`, `apply_edit_to_selection()`
- **Root cause:** After deleting a `Selection::Range`, the function still calls
  `delete_backward()` on the collapsed cursor, removing one extra character
- **Fix:** For `Selection::Range` + `DeleteBackward`/`DeleteForward`, return
  immediately after `delete_range()` — don't apply the edit again

### 2. Long word wraps character-by-character
- **File:** `layout/src/text3/cache.rs:7730-7756`
- **Root cause:** When `available_width` is very small or 0, the emergency
  break loop at line 7735 breaks after every character. Also: shape exclusion
  calculations can produce negative segment widths (no `.max(0.0)` guard)
- **Fix:** Ensure minimum segment width of 0.0 in `get_line_constraints()`;
  investigate `available_width: Definite(0.0)` default issue documented at
  line 863

### 3. Clipboard (Ctrl+C/V/X): ALREADY WORKING
- All handlers fully implemented with platform clipboard access on all OSes
- Multi-cursor paste with smart N-lines→N-cursors logic exists
- No changes needed

### 4. Unicode/CJK IME text in VoiceOver: MOSTLY WORKING  
- UTF-16 conversion correct for BMP and supplementary plane chars
- Byte→char index conversion correct
- Font fallback handles CJK
- **Gap:** Preedit (composition) text not exposed to a11y tree — VoiceOver
  only hears committed text, not in-progress composition

### 5. Wayland surrounding text sends empty string
- **File:** `dll/src/desktop/shell2/linux/wayland/mod.rs:3864`
- **Fix:** Read actual text from `get_text_before_textinput()` + compute
  global cursor byte offset from `cluster_id.source_run` + `start_byte_in_run`

## Execution Order

```
1. Backspace bug fix          (critical, 10 min — one function change)
2. Wayland surrounding text   (easy, 15 min)  
3. Word wrap investigation    (complex — need to understand available_width=0 root cause)
```
