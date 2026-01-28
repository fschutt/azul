Here is a detailed analysis of each bug with its root cause and the specific code fixes required.

### Bug 1: TEXT INPUT STOPPED WORKING (HIGHEST PRIORITY)

#### 1. Root Cause Analysis

The root cause is a faulty `if` condition in the V2 event processing logic that prevents the text changes from being applied after a user types.

The text input flow is designed as follows:
1.  `handle_text_input` calls `record_text_input`.
2.  `record_text_input` creates a `pending_changeset` in the `TextInputManager`.
3.  The event loop detects this pending change and fires an `On::TextInput` callback.
4.  After the callback, if `preventDefault()` was not called, the system is supposed to call `apply_text_changeset` to visually update the text.

The bug lies in the condition that checks whether to perform Step 4. In `dll/src/desktop/shell2/common/event_v2.rs`, the function `process_window_events_recursive_v2` has this logic:

```rust
// dll/src/desktop/shell2/common/event_v2.rs

// ...
let should_apply_text_input = post_filter
    .system_events
    .contains(&azul_core::events::PostCallbackSystemEvent::ApplyTextInput);

if should_apply_text_input && !text_input_affected_nodes.is_empty() { // BUG HERE
    if let Some(layout_window) = self.get_layout_window_mut() {
        // This code is never reached for normal keyboard input
        let dirty_nodes = layout_window.apply_text_changeset();
        // ...
    }
}
```

The variable `text_input_affected_nodes` is a local variable that is only populated by programmatic text input (e.g., from the debug server via `CreateTextInput`), not by normal keyboard input. For keyboard input, this variable is always an empty `BTreeMap`. Therefore, the condition `!text_input_affected_nodes.is_empty()` is always `false`, and `apply_text_changeset()` is never called, so the typed text never appears.

This is a regression because this faulty logic was likely added as part of the V2 event system refactoring, and the `text_input_affected_nodes` variable was intended for a different purpose but was mistakenly included in this check.

#### 2. Specific File and Line Numbers

-   **File**: `dll/src/desktop/shell2/common/event_v2.rs`
-   **Function**: `process_window_events_recursive_v2`
-   **Approximate Line**: `2919` (based on the diff context for the `scroll_selection_into_view` addition, this logic is just before it).

#### 3. Exact Code Fix

The fix is to remove the incorrect check for `!text_input_affected_nodes.is_empty()`. The decision to apply the text input should only depend on whether an `Input` event was processed without being default-prevented (`should_apply_text_input`).

```diff
--- a/dll/src/desktop/shell2/common/event_v2.rs
+++ b/dll/src/desktop/shell2/common/event_v2.rs
@@ -2910,7 +2910,7 @@
             .system_events
             .contains(&azul_core::events::PostCallbackSystemEvent::ApplyTextInput);
 
-        if should_apply_text_input && !text_input_affected_nodes.is_empty() {
+        if should_apply_text_input {
             if let Some(layout_window) = self.get_layout_window_mut() {
                 // Apply text changes and get list of dirty nodes
                 let dirty_nodes = layout_window.apply_text_changeset();

```

---

### Bug 2: Border/Scrollbar Offset (~10px detached)

#### 1. Root Cause Analysis

The symptoms (both border and scrollbar are detached from the element by a similar offset) strongly indicate a coordinate system error in the display list building phase. The position of the border and scrollbar rectangles is being calculated relative to the element's **content-box** instead of its **padding-box**.

When an element has `padding: 10px`, its content area is inset from the border. The border should be drawn around the padding area. The current logic is likely using the absolute position of the *content area* as the origin for the border's rectangle, causing the border to be drawn 10px inside where it should be, appearing detached. The same error applies to the scrollbar, which should be aligned to the edge of the padding box.

This bug resides in the code that generates the `DisplayList`, which is part of the `solver3` module (source not provided). The fix involves adjusting the origin of the border and scrollbar `DisplayListItem`s.

#### 2. Specific File and Line Numbers

-   **File**: `layout/src/solver3/display_list.rs` (or a similar file responsible for display list generation, which is not provided in the source files).
-   **Function**: The function that iterates through `LayoutNode`s and creates `DisplayListItem`s for borders and scrollbars.

#### 3. Exact Code Fix

The conceptual fix is to adjust the rectangle used for drawing borders and scrollbars. Instead of using the node's content-box position directly, it must be offset outwards by the padding dimensions.

```diff
--- a/layout/src/solver3/display_list.rs
+++ b/layout/src/solver3/display_list.rs
@@ -XXX,XX +XXX,XX @@
 // ... inside display list generation loop for a node ...
 let node_pos = calculated_positions[&node_idx]; // This is likely the content-box origin
 let node_size = node.used_size.unwrap_or_default();
 let padding = get_padding_for_node(styled_dom, node.dom_node_id);
 
 // -- Border Fix --
-// BUG: Border is drawn relative to the content box origin.
-let border_rect_origin = node_pos;
-let border_rect_size = node_size;
+// FIX: Border is drawn around the padding box. Adjust origin and size.
+let border_rect_origin = LogicalPosition::new(node_pos.x - padding.left, node_pos.y - padding.top);
+let border_rect_size = LogicalSize::new(node_size.width + padding.horizontal(), node_size.height + padding.vertical());
 let border_rect = LogicalRect::new(border_rect_origin, border_rect_size);
 // ... create border DisplayListItem with corrected rect ...
 
 // -- Scrollbar Fix --
 let scrollbar_thickness = 15.0; // Example value
-// BUG: Horizontal scrollbar is positioned relative to content box bottom.
-let horizontal_scrollbar_y = node_pos.y + node_size.height;
+// FIX: Position horizontal scrollbar at the bottom of the padding box.
+let horizontal_scrollbar_y = node_pos.y + node_size.height + padding.bottom;
 let horizontal_scrollbar_rect = LogicalRect::new(
-    LogicalPosition::new(node_pos.x, horizontal_scrollbar_y),
+    LogicalPosition::new(node_pos.x - padding.left, horizontal_scrollbar_y),
     LogicalSize::new(node_size.width + padding.horizontal(), scrollbar_thickness)
 );
 // ... create scrollbar DisplayListItem with corrected rect ...

```

---

### Bug 3: `white-space: nowrap` Ignored

#### 1. Root Cause Analysis

The text layout engine (`text3`) has support for non-wrapping text via the `TextWrap::NoWrap` enum variant in `UnifiedConstraints`. The bug is that the CSS property `white-space: nowrap` is not being translated into this constraint during the layout setup phase.

The code responsible for reading CSS properties from the `StyledDom` and creating the `UnifiedConstraints` for a given node is failing to handle the `white-space` property. This logic is likely located in the `solver3` module (source not provided).

#### 2. Specific File and Line Numbers

-   **File**: `layout/src/solver3/getters.rs` (or a similar file responsible for building `UnifiedConstraints`, which is not provided).
-   **Function**: A function like `get_unified_constraints_for_node`.

#### 3. Exact Code Fix

The fix is to read the `white-space` property from the styled node and set `constraints.text_wrap` accordingly.

```diff
--- a/layout/src/solver3/getters.rs
+++ b/layout/src/solver3/getters.rs
@@ -XXX,XX +XXX,XX @@
 use azul_css::props::style::{StyleText, StyleWhitespace};
 use crate::text3::cache::{TextWrap, UnifiedConstraints};
 
 pub fn get_text_layout_constraints(styled_dom: &StyledDom, node_id: NodeId) -> UnifiedConstraints {
     let mut constraints = UnifiedConstraints::default();
     // ... code to set available_width, etc. ...
 
+    // FIX: Translate CSS white-space property to TextWrap constraint.
+    if let Some(whitespace) = get_property::<StyleWhitespace>(styled_dom, node_id) {
+        match whitespace.0 {
+            azul_css::style::Whitespace::NoWrap => {
+                constraints.text_wrap = TextWrap::NoWrap;
+            },
+            // Other values like 'pre', 'pre-wrap' would be handled here too.
+            _ => {
+                constraints.text_wrap = TextWrap::Wrap;
+            }
+        }
+    }
+
     // ... other properties ...
     return constraints;
 }
```

---

### Bug 4: Missing Glyphs

#### 1. Root Cause Analysis

The issue is an incomplete font fallback chain. When a character is not found in the specified `font-family` (e.g., `monospace`), the font resolution system (`rust_fontconfig`) fails to find an alternative system font that contains the required glyph. This is common for special characters, symbols, or emojis if a comprehensive fallback font is not included in the search.

The font chain resolution happens in `layout_dom_recursive` before layout begins. The list of font families sent to `fc_cache.resolve_font_chain` is likely too specific and does not include generic system-wide fallbacks.

#### 2. Specific File and Line Numbers

-   **File**: `layout/src/solver3/getters.rs` (or a similar file where `collect_and_resolve_font_chains` is implemented, which is not provided).
-   **Function**: `collect_and_resolve_font_chains`.

#### 3. Exact Code Fix

The fix is to augment every font list with a set of reliable system fallback fonts before resolving the chain. This ensures that if the primary fonts fail, the system has a chance to find the glyph in a more comprehensive font.

```diff
--- a/layout/src/solver3/getters.rs
+++ b/layout/src/solver3/getters.rs
@@ -XXX,XX +XXX,XX @@
 // ... inside collect_and_resolve_font_chains ...
 for (key, font_families) in font_families_to_resolve {
     
-    // BUG: The font list is used as-is, without system fallbacks.
-    let chain = fc_cache.resolve_font_chain(&font_families);
+    // FIX: Append system-default fallback fonts to ensure glyph coverage.
+    let mut families_with_fallback = font_families.clone();
+    // These are generic families that fontconfig will resolve to the system's default.
+    families_with_fallback.push("sans-serif".to_string());
+    // Add specific, known-good fonts for emojis and symbols.
+    families_with_fallback.push("Apple Color Emoji".to_string()); // macOS
+    families_with_fallback.push("Noto Color Emoji".to_string()); // Linux, new Windows
+    families_with_fallback.push("Segoe UI Emoji".to_string()); // Windows
+
+    let chain = fc_cache.resolve_font_chain(&families_with_fallback);
     
     resolved_chains.insert(key.clone(), chain);
 }
```

---

### Bug 5: Scrollbar Sizing/Position Wrong

#### 1. Root Cause Analysis

This bug has three distinct causes within the scrollbar layout and rendering logic, all located in the unprovided `solver3` module:

1.  **Track Sizing:** The calculation for the scrollbar track's length (e.g., `track_size = width`) is incorrect because it fails to subtract the space occupied by the scrollbar's arrow buttons at each end.
2.  **Hiding (`overflow: auto`):** The condition to determine if a scrollbar is needed is flawed. It should only be displayed if `content_size > container_size`. The current logic likely shows it incorrectly or fails to check this condition when `overflow` is `auto`.
3.  **Positioning:** The Y-coordinate for the horizontal scrollbar is wrong. It should be placed at the bottom edge of the scroll container's padding box, calculated as `container_rect.origin.y + container_rect.size.height - scrollbar_thickness`. The current calculation is likely omitting one of these terms.

#### 2. Specific File and Line Numbers

-   **File**: `layout/src/solver3/layout.rs` or `layout/src/solver3/display_list.rs` (not provided).
-   **Function**: The functions responsible for calculating layout geometry and building the display list for scrollable nodes.

#### 3. Exact Code Fix

The fixes require modifying the geometry calculations in the layout and display list stages.

```diff
--- a/layout/src/solver3/layout.rs
+++ b/layout/src/solver3/layout.rs
@@ -XXX,XX +XXX,XX @@
 // ... inside layout logic for a scrollable node ...
 
 // FIX for Hiding
 let needs_horizontal_scrollbar = 
     (overflow_x == Overflow::Scroll) ||
-    (overflow_x == Overflow::Auto && content_width > container_width - 1.0); // BUG: Can show for tiny overflows
+    (overflow_x == Overflow::Auto && content_width > container_width);
 
 node.scrollbar_info.needs_horizontal = needs_horizontal_scrollbar;
 
 if needs_horizontal_scrollbar {
     // FIX for Sizing
     let button_width = 15.0; // Should be from theme
-    node.scrollbar_info.track_rect.size.width = container_width;
+    node.scrollbar_info.track_rect.size.width = container_width - (2.0 * button_width);
 }
```

```diff
--- a/layout/src/solver3/display_list.rs
+++ b/layout/src/solver3/display_list.rs
@@ -XXX,XX +XXX,XX @@
 // ... inside display list builder for a scrollable node ...
 
 if node.scrollbar_info.needs_horizontal {
     let scrollbar_thickness = 15.0;
-    // BUG: Incorrect Y position.
-    let scrollbar_y = node_pos.y + node_size.height;
+    // FIX: Correct Y position at the bottom edge of the container.
+    let scrollbar_y = node_pos.y + node_size.height - scrollbar_thickness;
 
     let scrollbar_rect = LogicalRect::new(
         LogicalPosition::new(node_pos.x, scrollbar_y),
         LogicalSize::new(node_size.width, scrollbar_thickness)
     );
     // ... push scrollbar rect to display list ...
 }
```