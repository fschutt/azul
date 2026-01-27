Excellent and detailed problem description. Here is a thorough analysis of the bugs and recommended fixes based on the provided information.

## Executive Summary

*   **Bug 1 (text_area.c):** The root cause is that the **inline text layout engine is not correctly handling newlines (`\n`) when calculating the total height of a text block with `white-space: pre`**. It appears to be laying out only the first line of text, resulting in a `content_size.height` that reflects a single line, not all 15. The `get_scroll_content_size` function itself is likely correct, but it's operating on incomplete layout data.

*   **Bug 3 (selection.c):** This is a classic **coordinate space mismatch bug**. The recent fix in commit `80d8ebc5` correctly transformed text rendering primitives from `Window` space to `ScrollFrame` space. However, the logic that generates the selection rectangles was not updated. It still calculates and draws the rectangles in absolute `Window` coordinates, while the text has been moved, causing the visual offset.

## Detailed Analysis

### 1. Bug 1: `text_area.c` - Incorrect `content_size` Calculation

**Why does `get_scroll_content_size()` return ~92px instead of ~400px?**

The function `get_scroll_content_size` has two main paths:
1.  Return a pre-calculated `overflow_content_size` (for block-level children).
2.  Calculate the size from `inline_layout_result` (for inline content like text).

The `text_area` test uses a single `<div>` with a single text node child. This triggers the second path. The function iterates through `text_layout.items` to find the maximum Y extent. The value of `92.79843px` is suspiciously large for a single line of 36px text (which should be closer to `36px * 1.4 line-height = 50.4px`), but it's far too small for 15 lines.

The critical clue is in the `text_area` debug data for the text primitive itself:

```json
{
  "index": 12,
  "type": "text",
  "x": 56.0,
  "y": 91.0,
  "width": 614.60156,
  "height": 200.0,
  "color": "#ffffffff",
  "font_size": 36.0,
  "glyph_count": 40,  // <-- THIS IS THE SMOKING GUN
  "clip_depth": 1,
  "scroll_depth": 1
}
```

The first line of text is `"Line 1: This is the first line of text."`. This string is exactly **40 characters long**. The `glyph_count` of 40 strongly indicates that the layout engine processed only the first line and stopped at the `\n` character, or was constrained by the initial height.

Therefore, the `inline_layout_result` passed to `get_scroll_content_size` only contains layout items for the first line of text. The loop correctly calculates the bounding box of what it was given, which is just one line, leading to the incorrect `content_size`.

**Conclusion:** The bug is not in `get_scroll_content_size` but in the upstream **inline layout solver**. It fails to correctly parse the entire text content and create line boxes for each line separated by `\n` when `white-space: pre` is active. It must lay out the *entire* content, regardless of the container's height, to determine the scrollable overflow size.

---

### 2. Difference between `text_area` and `scrollbar_drag`

This comparison confirms the analysis above.

*   **`scrollbar_drag.c` (Works):** The scrollable container has 30 block-level `<div>` children. The layout engine's block formatting context calculates the position and size of each child. The total height of these children is summed up and stored in the parent's `overflow_content_size` field. The `get_scroll_content_size` function then takes the first, correct path: `if let Some(overflow_size) = node.overflow_content_size`. This works because block layout is fundamentally simpler for this case.

*   **`text_area.c` (Broken):** The scrollable container has one inline-level text node child. The layout engine must use the inline formatting context to determine the size. As established, this context is failing to process the newlines, so it reports a size that is far too small.

The key difference is **Block Layout vs. Inline Layout** for content sizing. The block layout path is working correctly, while the inline layout path has a bug related to `white-space: pre` and newline handling.

---

### 3. Bug 3: `selection.c` - Visual Offset Bug

This bug is a direct consequence of the partial fix in commit `80d8ebc5`.

**Analysis:**
1.  **The Problem:** The commit `80d8ebc5` fixed a coordinate space mismatch for rendering primitives. It correctly notes that layout produces absolute `Window` coordinates, but scroll frames require coordinates relative to their own origin. The fix was to subtract the scroll frame's origin from child primitives.
2.  **The Oversight:** This transformation was applied to the main display list primitives (like text glyphs), but it was **missed for dynamically generated primitives like selection rectangles**.
3.  **The Result:**
    *   The text inside a scrollable area is correctly drawn at, for example, `(10, 10)` relative to the scroll frame's top-left corner.
    *   When you select that text, the selection logic queries the layout tree for the glyph positions. It gets back the original, absolute `Window` coordinates (e.g., `(110, 110)` if the scroll frame is at `(100, 100)`).
    *   The selection rendering code then creates a rectangle at `(110, 110)` in `Window` space and pushes it to the display list.
    *   This rectangle is now visually offset from the text it is supposed to be highlighting.

This bug will manifest any time selection occurs inside a transformed stacking context or a scroll frame. The `selection.c` test doesn't use a scroll frame, but the `body` is a flex container, which creates its own coordinate context for its children. The principle is the same: the selection rectangles are not being drawn in the same coordinate space as the text they highlight.

---

### 4. Recommended Fixes

#### Fix for Bug 1 (`text_area.c` content_size)

The fix needs to be applied in the **inline layout engine**, likely in a file such as `layout/src/inline.rs` or `layout/src/text.rs`.

**Location:** Find the code responsible for generating `InlineLayoutResult` for a text node.

**Logic:**
1.  When laying out a text node, check for the `white-space: pre` CSS property.
2.  If it's present, the layout algorithm must not treat the text as a single continuous string to be wrapped.
3.  Instead, it should split the string by `\n` characters.
4.  It must then create a new "line box" for each resulting substring.
5.  The vertical position of each line box must be accumulated (`y_position += line_height`).
6.  The final `content_size.height` will be the `y_position` of the bottom of the last line. This process must continue for the entire string, even if it exceeds the container's bounds.

**Pseudocode for the fix:**
```rust
// Inside the inline layout solver...

fn layout_text_node(text: &str, style: &ComputedStyle) -> InlineLayoutResult {
    let mut items = Vec::new();
    let mut current_y = 0.0;
    let mut max_x = 0.0;
    let line_height = calculate_line_height(style);

    if style.white_space == WhiteSpace::Pre {
        for line_str in text.split('\n') {
            // Layout this single line without wrapping
            let line_layout = layout_single_line(line_str, style);
            
            for item in line_layout.items {
                // Create a new positioned item with the correct Y offset
                let positioned_item = PositionedLayoutItem {
                    position: LogicalPoint::new(item.position.x, current_y + item.position.y),
                    item: item.item,
                };
                items.push(positioned_item);
            }

            max_x = max_x.max(line_layout.width);
            current_y += line_height;
        }
    } else {
        // ... existing line wrapping logic ...
    }

    // The total height is now correctly calculated from the last line's position
    let total_height = current_y; 
    
    return InlineLayoutResult {
        items,
        bounds: LogicalSize::new(max_x, total_height),
        // ... other fields
    };
}
```

#### Fix for Bug 3 (`selection.c` visual offset)

The fix needs to be applied where the selection rectangles are generated and pushed to the display list. This could be in a module like `compositor/src/selection.rs`, `display_list/src/builder.rs`, or wherever `SelectionState` is translated into drawing primitives.

**Location:** Find the function that takes a `SelectionRange` and generates `DisplayListPrimitive::Rect`.

**Logic:**
1.  When drawing a selection rectangle for a given text run, the code currently gets the glyph positions in `Window` space.
2.  Before creating the `Rect` primitive, it must get the **current transformation from the display list builder's state stack**. This stack holds the active scroll offsets and other transforms.
3.  Apply the inverse of this transformation to the rectangle's coordinates, effectively converting them from `Window` space to the correct local space (e.g., `ScrollFrame` or `Parent` space). This is the exact same principle from commit `80d8ebc5`.

**Pseudocode for the fix:**
```rust
// In the module that builds the display list for selections...

fn push_selection_rects(
    builder: &mut DisplayListBuilder, 
    selection: &Selection
) {
    for range in selection.ranges() {
        // Calculate the rectangle bounds in absolute Window coordinates
        let window_space_rect = calculate_rect_for_range(range); // Returns Rect in [CoordinateSpace::Window]

        // Get the current transform from the builder's state. This contains
        // the origin of the current scroll frame or stacking context.
        let current_transform = builder.get_current_transform(); // This is the key step

        // Transform the rect from Window space to the local coordinate space
        // This is the reverse of what the compositor does to the context itself.
        let local_space_rect = current_transform.inverse().transform_rect(window_space_rect);

        // Push the correctly transformed rectangle
        builder.push_rect(local_space_rect, selection_color);
    }
}
```
This ensures that the selection rectangles and the text they are highlighting are both transformed into the same final coordinate space before being sent to the renderer.