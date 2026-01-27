Here's a detailed analysis of the 4 bugs observed in the `selection.c` test, along with root causes, investigation points, priorities, and proposed solutions.

## Text Selection Bug Report - 4 Issues

### Test Setup Recap

The `selection.c` test creates a window with three `div` paragraphs:
1.  **Paragraph 1 (Green):** Selectable text.
2.  **Paragraph 2 (Red/Pink):** `user-select: none` - *should NOT be selectable*.
3.  **Paragraph 3 (Blue/Purple):** Selectable text.

All paragraphs have `font-size: 28px; padding: 15px; background-color: #color; margin: 8px;`.

### Screenshot Analysis

The screenshot clearly shows a selection spanning across all three paragraphs, including the red one. Blue selection rectangles are visible, and a blinking text cursor (caret) is also present within the selected region.

---

### Bug 1: Selection Rectangles at Wrong Position

**Symptom:** The blue selection rectangles appear offset from the actual text. They extend into the padding area and seem to be misaligned with the text glyphs.

**Root Cause Analysis:**
This issue typically arises from a mismatch in coordinate systems or box model interpretation between the text layout engine and the selection rendering engine.
1.  **Layout Engine:** Calculates the precise position and bounding boxes of text glyphs, usually relative to the *content box* of their parent element.
2.  **Selection Rendering:** When drawing the selection rectangles, it might be using coordinates that are relative to a different box model (e.g., the *padding box* or *border box*) without accounting for the element's padding.
    *   If the text layout provides glyph coordinates relative to the content box, and the selection renderer draws them relative to the padding box, the selection will appear shifted by the padding amount.
    *   Alternatively, the selection rectangles might be calculated based on the element's full bounding box (including padding) but then drawn *within* the content box, leading to an offset.

**File and Line Number Locations to Investigate (Hypothetical):**
*   **`layout/text_layout.c` / `layout/box_model.c`:** Functions that calculate the bounding boxes of text lines and glyphs. Verify what coordinate space these bounds are in (e.g., relative to element's content box, padding box, or document root).
*   **`rendering/selection_renderer.c` / `rendering/paint_text.c`:** The code responsible for iterating through selected text ranges and drawing the blue highlight rectangles. Check how the coordinates provided by the selection manager are transformed before drawing.
*   **`dom/element.c` / `css/computed_style.c`:** How padding values are stored and retrieved.

**Priority:** **High** - This is a fundamental visual correctness issue that makes text selection appear broken and unprofessional.

**Specific Code Changes (Conceptual):**
Ensure consistency in coordinate systems.
*   **Option A (Preferred):** If text layout provides glyph bounds relative to the content box, the selection renderer must add the element's `padding-left` and `padding-top` to these coordinates before drawing the selection rectangles, effectively shifting them into the correct position relative to the element's padding box.
    ```c
    // In rendering/selection_renderer.c (or similar)
    void draw_selection_highlight(GraphicsContext* gc, TextSelectionRect* rect, Element* parent_element) {
        // Assume rect->x, rect->y are relative to parent_element's content box
        float adjusted_x = rect->x + parent_element->computed_style.padding_left;
        float adjusted_y = rect->y + parent_element->computed_style.padding_top;
        gc->draw_filled_rectangle(adjusted_x, adjusted_y, rect->width, rect->height, SELECTION_COLOR);
    }
    ```
*   **Option B:** Adjust the text layout engine to provide glyph bounds already offset by padding, if that aligns better with the overall rendering pipeline.

---

### Bug 2: Cursor Visible on Non-Editable Content

**Symptom:** A blinking text cursor (caret) is visible even though none of the paragraphs are `contenteditable` or input fields.

**Root Cause Analysis:**
The logic that determines when to display a text cursor is too permissive. A cursor should only be shown in contexts where text can be *inserted* or *edited*.
*   The system might be showing a cursor whenever a text node receives focus (e.g., after a click or selection), without checking if the element is actually editable.
*   There might be a default behavior to show a cursor if a selection exists, which is incorrect for non-editable content.

**File and Line Number Locations to Investigate (Hypothetical):**
*   **`rendering/caret_renderer.c` / `rendering/paint_caret.c`:** The function responsible for drawing the blinking caret.
*   **`input/selection_manager.c` / `input/focus_manager.c`:** The component that manages focus and selection state. Check where the decision to "show caret" is made.
*   **`dom/element.c` / `dom/node.c`:** How `contenteditable` or `is_text_input` properties are stored and queried.

**Priority:** **Medium** - While not breaking functionality, it's a UI inconsistency that suggests an editable context where none exists, potentially confusing the user.

**Specific Code Changes (Conceptual):**
Modify the caret rendering logic to explicitly check for editability.
```c
// In rendering/caret_renderer.c (or similar)
void draw_caret(GraphicsContext* gc, SelectionState* selection_state) {
    if (selection_state->has_focus && selection_state->is_collapsed) { // Only draw if collapsed (no range selection)
        Element* focused_element = selection_state->focused_node->parent_element;
        // Check if the focused element is actually editable
        if (focused_element->is_contenteditable || focused_element->is_text_input) {
            // Calculate caret position based on selection_state->caret_position
            // ...
            gc->draw_line(caret_x, caret_y, caret_x, caret_y + caret_height, CARET_COLOR);
        }
    }
}
```
This also ties into Bug 3, as a caret should ideally only be shown when the selection is collapsed (an insertion point).

---

### Bug 3: Cursor AND Selection Visible Simultaneously

**Symptom:** Both the blinking text cursor (caret) and the blue selection rectangles are visible at the same time.

**Root Cause Analysis:**
This indicates a flawed selection state model. In standard UI/UX:
*   If there's a **range selection** (text highlighted), the cursor is typically *not* shown as a blinking caret, or it's implicitly at one end of the selection.
*   If there's a **collapsed selection** (just an insertion point, no text highlighted), then a blinking caret is shown.
The system likely has independent flags or states for "has selection" and "has caret" that are not mutually exclusive, or the rendering logic for the caret doesn't check if a range selection is active.

**File and Line Number Locations to Investigate (Hypothetical):**
*   **`input/selection_manager.c`:** The central component managing the selection state. How is `is_collapsed` (selection is an insertion point) or `has_range_selection` tracked?
*   **`rendering/caret_renderer.c` / `rendering/selection_renderer.c`:** The rendering functions for both the caret and the selection. They need to coordinate.

**Priority:** **Medium-High** - This is a UI state inconsistency that can be confusing. It's less critical than `user-select: none` not working, but it's a clear visual bug.

**Specific Code Changes (Conceptual):**
Modify the caret rendering logic to only draw if the selection is collapsed (an insertion point).
```c
// In rendering/caret_renderer.c (or similar)
void draw_caret(GraphicsContext* gc, SelectionState* selection_state) {
    // Only draw caret if there's focus, and the selection is collapsed (no range)
    if (selection_state->has_focus && selection_state->is_collapsed) {
        // Add the contenteditable check from Bug 2 fix
        Element* focused_element = selection_state->focused_node->parent_element;
        if (focused_element->is_contenteditable || focused_element->is_text_input) {
            // ... calculate and draw caret ...
        }
    }
}
```
The `selection_state` should have a clear `is_collapsed` flag.

---

### Bug 4: `user-select: none` Not Working

**Symptom:** The second paragraph (red background) has `user-select: none` but selection still appears to include it, with blue highlight covering its text.

**Root Cause Analysis:**
The selection algorithm, which determines which text nodes are part of a user's drag selection, is not respecting the `user-select` CSS property.
1.  **CSS Parsing:** The `user-select: none` property is likely parsed correctly and stored in the computed style of the paragraph.
2.  **Selection Algorithm:** When the user drags the mouse, the selection manager calculates the start and end points of the selection. It then traverses the DOM between these points to identify all text nodes that fall within the selection range. This traversal logic is probably *not* checking the `user-select` property of the text nodes or their parent elements. It simply includes all text it finds.

**File and Line Number Locations to Investigate (Hypothetical):**
*   **`input/selection_manager.c` / `input/mouse_event_handler.c`:** The core logic that processes mouse drag events to update the selection range. This is where the DOM traversal happens.
*   **`css/style_resolver.c` / `dom/element.c`:** Where `user-select` is parsed and stored in the `ComputedStyle` for an element.
*   **`dom/node.c` / `dom/text_node.c`:** Functions to get the parent element and its computed style for a given text node.

**Priority:** **Critical** - This is a functional bug that violates a standard CSS property and user expectation. It can break UI interactions where certain text should explicitly not be selectable.

**Specific Code Changes (Conceptual):**
Modify the selection algorithm to check `user-select` during DOM traversal.
```c
// In input/selection_manager.c (or similar)
void update_selection_from_mouse_drag(Point start_screen_pos, Point end_screen_pos) {
    // ... (existing logic to find start_node, start_offset, end_node, end_offset) ...

    // Iterate through the text nodes between start_node and end_node
    // This loop determines the actual selected range(s)
    SelectionRange current_range = { .start_node = start_node, .start_offset = start_offset };
    TextNode* node = start_node;
    while (node != NULL && node != end_node->next_sibling_text_node) { // Iterate until after end_node
        Element* parent_element = get_parent_element(node);
        ComputedStyle* style = get_computed_style(parent_element);

        if (style->user_select == USER_SELECT_NONE) {
            // This node (or its parent) is not selectable.
            // If current_range has accumulated selectable text, finalize it.
            if (current_range.end_node != NULL) {
                add_selection_range_to_state(current_range);
            }
            // Reset current_range to start after this non-selectable node
            current_range.start_node = get_next_selectable_text_node(node); // Find next selectable node
            current_range.start_offset = 0;
            current_range.end_node = NULL; // Mark as empty range
        } else {
            // This node is selectable. Extend the current range.
            current_range.end_node = node;
            current_range.end_offset = node->text_length; // Or specific offset if end_node is partial
            // If this is the actual end_node, set its specific offset
            if (node == end_node) {
                current_range.end_offset = end_offset;
            }
        }
        node = node->next_sibling_text_node;
    }
    // Add the final range if any
    if (current_range.end_node != NULL) {
        add_selection_range_to_state(current_range);
    }
    // Update the global selection_state with the list of valid ranges
}
```
This might require the selection model to support multiple, non-contiguous ranges if the user selects across a `user-select: none` element. A simpler initial fix might be to just prevent the highlight, but the correct behavior is to exclude the text from the actual selection range.

---

### Code Structure Overview (General Browser Engine)

Based on the `azul.h` API and common browser engine architectures, the selection logic would typically be distributed across these components:

1.  **DOM (Document Object Model):**
    *   `AzDom` structures represent elements and text nodes.
    *   `AzDom_setInlineStyle`, `AzDom_addClass` modify the DOM.

2.  **CSS / Style System:**
    *   `AzCss` handles stylesheets.
    *   A style resolver (not explicitly shown in `selection.c` but implied by `AzDom_style`) would parse CSS properties like `user-select` and compute the final style for each DOM node. This computed style would be stored (e.g., in a `ComputedStyle` struct associated with each `AzDom` node).

3.  **Layout Engine:**
    *   `AzLayoutCallbackInfo` suggests a layout pass.
    *   This engine takes the DOM and computed styles to calculate the size and position of every element and text run on the page. It determines line breaks, glyph positions, and the overall box model for each element (content box, padding box, border box).

4.  **Input Handling / Event Loop:**
    *   `AzEventFilter`, `AzCallbackInfo` indicate event processing.
    *   A mouse event handler would capture `mouseDown`, `mouseMove` (for dragging), and `mouseUp` events.
    *   These events are then passed to a **Selection Manager**.

5.  **Selection Manager (Core Logic):**
    *   This is the central component for text selection.
    *   It receives mouse events, performs hit-testing to identify the text node and offset under the cursor.
    *   It maintains the current `SelectionState` (start node/offset, end node/offset, whether it's collapsed, whether it has focus).
    *   It implements the logic for extending selection during drags, handling double/triple clicks for word/paragraph selection, and respecting CSS properties like `user-select`.
    *   It notifies the rendering engine when the selection state changes.

6.  **Rendering Engine:**
    *   Takes the layout information and selection state.
    *   **Text Renderer:** Draws the actual text glyphs.
    *   **Selection Renderer:** Draws the blue highlight rectangles based on the `SelectionState` provided by the Selection Manager, using the glyph bounding boxes from the Layout Engine.
    *   **Caret Renderer:** Draws the blinking text cursor (caret) based on the `SelectionState` (specifically, when the selection is collapsed and focused on an editable element).

---

By addressing these bugs in the proposed order and locations, the text selection functionality in Azul should become much more robust and compliant with standard web behavior.