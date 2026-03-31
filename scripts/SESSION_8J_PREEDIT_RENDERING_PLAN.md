# Session 8J: Preedit Text Inline Rendering Plan

## Problem
IME composition (Japanese, Chinese, dead keys) stores preedit text in
`TextEditManager.preedit_text`, but only a 2px underline is drawn.
The actual preedit characters (にほんご, 日本語) are not rendered inline.

## Architecture: Inject via dirty_text_overrides

The text layout pipeline already has a "dirty text" override system
(`dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>`) that
lets edited text bypass the DOM. Preedit text should use the same
mechanism — it's just another temporary text override.

### Flow

```
1. setMarkedText("にほ") called by macOS IME
2. TextEditManager.set_preedit("にほ", 2, 2)
3. Window sets display_list_dirty = true, request_redraw()
4. drawRect → regenerate_display_list_for_dom(dom_id)
5. Layout reads preedit text and injects it at cursor position
6. Text is shaped through normal font fallback pipeline
7. Glyphs rendered with underline decoration
```

### Step 1: Inject preedit into inline content at cursor position

In `layout/src/window.rs`, function `get_text_before_textinput()` or
the equivalent content-gathering function:

```rust
fn get_inline_content_with_preedit(
    &self, dom_id: DomId, node_id: NodeId
) -> Vec<InlineContent> {
    let mut content = self.get_text_before_textinput(dom_id, node_id);

    if let Some(ref preedit) = self.text_edit_manager.preedit_text {
        if let Some(cursor) = self.text_edit_manager.get_primary_cursor() {
            // Insert preedit text at cursor position in the StyledRun
            let run_idx = cursor.cluster_id.source_run as usize;
            let byte_pos = cursor.cluster_id.start_byte_in_run as usize;
            if let Some(InlineContent::Text(run)) = content.get_mut(run_idx) {
                // Insert preedit text at cursor byte position
                run.text.insert_str(byte_pos, preedit);
                // Mark the range as preedit (for underline rendering)
            }
        }
    }
    content
}
```

This means the preedit text becomes part of the normal text run.
The font fallback chain automatically handles CJK characters because
`font_chain.resolve_char()` picks the right font per character.

### Step 2: Shape normally — font fallback handles CJK

No changes needed in the shaping pipeline. The `shape_visual_items()`
function already:
1. Splits text by script changes (Latin → CJK boundary)
2. Calls `font_chain.resolve_char(first_char)` per segment
3. Shapes each segment with the correct font

If the user's font stack includes a CJK fallback (which macOS system
fonts do by default via fontconfig), Japanese/Chinese characters will
be shaped with Hiragino Sans / PingFang / etc.

### Step 3: Render with underline decoration

In the display list, the preedit range needs an underline. Options:

**Option A (simple):** Track preedit byte range and add underline
in `paint_cursor`. The shaped glyphs are already positioned correctly.
Just draw a 2px underline rect spanning the preedit glyph positions.

**Option B (CSS):** Apply `text-decoration: underline` to the preedit
range via a CSS pseudo-class. More complex but follows web standards.

Recommend Option A — it's 10 lines of code.

### Step 4: Update cursor position after preedit

After injecting preedit text, the cursor should move to the end of
the preedit range (so typing continues after the preedit). The
`selectedRange` from `setMarkedText` tells us where the cursor should
be within the preedit.

### Step 5: Remove preedit on commit/cancel

When `insertText` is called (commit), preedit is cleared and the
final text is inserted via `handle_text_input()`.
When `unmarkText` is called (cancel), preedit is cleared.
Both already work — the preedit text is simply removed from the
inline content on the next layout pass.

## What DOESN'T need to change

- Font loading / fontconfig — already handles CJK via fallback chain
- WebRender font registration — fonts used in display list are auto-registered
- Text shaping — `shape_visual_items()` already handles mixed scripts
- CPU rendering — glyph rasterization is font-agnostic
- Display list builder — `push_text_run()` emits any shaped glyphs

## What DOES need to change

1. `get_text_before_textinput()` or a wrapper — inject preedit at cursor
2. `apply_text_changeset()` — skip changeset when preedit is active
3. `paint_cursor()` in display_list.rs — underline the preedit range
   (replace current approximate underline with precise glyph-based rect)

## Estimated complexity: ~50 lines of code

The key insight: preedit text IS regular text, just temporary.
Inject it into the StyledRun at the cursor position, let the existing
font fallback + shaping pipeline handle it, then remove it on commit.
