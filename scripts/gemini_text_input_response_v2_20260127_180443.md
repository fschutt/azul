# Gemini Text Input Architecture Response V2

Excellent. Thank you for the detailed rebuttal and clarification. Your explanation of the desired architecture is crystal clear and corrects the fundamental misunderstanding in the previous response. The "optimistic update to a visual cache" model is a powerful and well-established pattern, and my apologies for misinterpreting the framework's design.

This comprehensive review is based on your corrected architectural understanding.

## Executive Summary

The core of this architecture is the separation of the **committed state** (`StyledDom`) from the **visual state** (`LayoutCache`). The previous approach failed by attempting to merge these two, leading to incorrect behavior with complex, multi-node edits. The correct approach, as you've outlined, treats the `StyledDom` as read-only during an edit cycle and directs all immediate, "optimistic" visual updates to the `LayoutCache`. This is analogous to a controlled component in React, where the UI provides an immediate visual response while the application's data model remains the single source of truth.

This plan provides a complete, step-by-step guide to implementing this architecture correctly.
1.  **Implement Optimistic Updates:** We will correctly implement `update_text_cache_after_edit` to perform a lightweight, partial relayout of the edited content and update the `LayoutCache` directly, without touching the `StyledDom`. This provides the instant visual feedback users expect.
2.  **Refine Callback and Event Flow:** We will detail how `oninput` callbacks receive the changeset, how `preventDefault()` works within this model, and how contenteditable elements should follow the same controlled component pattern.
3.  **Implement Correct Hit-Testing:** We will provide a hit-testing function that operates on the final visual layout (`UnifiedLayout`) to accurately map mouse clicks to text cursor positions.
4.  **Fix Focus and Selection:** We will outline the precise interaction between the `FocusManager`, `CursorManager`, and `SelectionManager` to ensure seamless focus transfer and selection updates that are consistent with the optimistic update model.
5.  **Integrate Rich Content:** We will show how inline images and other rich content naturally fit into this architecture by treating them as `ShapedItem::Object`s inserted directly into the `LayoutCache`.

By following this plan, the text input system will be robust, performant, and correctly aligned with the framework's reactive, state-driven design principles.

---

*The following `ARCHITECTURE.MD` is included as provided, as it forms the basis for the architectural understanding in this review.*

# Azul GUI Framework Architecture

This document provides an architectural overview of the Azul GUI framework, detailing the flow from user input to final display, intended for new maintainers seeking a high-level understanding of the system components.

## Crate Overview

Azul is organized into several key crates:

| Crate | Path | Purpose |
|-------|------|---------|
| **core** | [`core/`](core/) | Core data structures: [`Dom`](core/src/dom.rs#L2945), [`RefAny`](core/src/refany.rs#L401), [`StyledDom`](core/src/styled_dom.rs#L585), events, resources |
| **css** | [`css/`](css/) | CSS parsing, properties ([`Css`](css/src/css.rs#L14), [`CssProperty`](css/src/props/property.rs#L486)), and styling |
| **layout** | [`layout/`](layout/) | Layout engine ([`solver3`](layout/src/solver3/)), text shaping ([`text3`](layout/src/text3/)), managers, and [`LayoutWindow`](layout/src/window.rs#L237) |
| **dll** | [`dll/`](dll/) | Platform shells ([Windows](dll/src/desktop/shell2/windows/), [macOS](dll/src/desktop/shell2/macos/), [X11](dll/src/desktop/shell2/linux/x11/), [Wayland](dll/src/desktop/shell2/linux/wayland/)), WebRender integration |
| **webrender** | [`webrender/`](webrender/) | Fork of Mozilla's WebRender GPU rendering engine |

### Entry Points

- **Desktop App Entry**: [`dll/src/desktop/app.rs`](dll/src/desktop/app.rs) - `App::new()` and `App::run()`
- **Window Creation**: Platform-specific in `dll/src/desktop/shell2/{platform}/mod.rs`
- **Layout Entry**: [`layout/src/window.rs`](layout/src/window.rs) - `LayoutWindow::layout_and_generate_display_list()`
- **Event Processing**: [`dll/src/desktop/shell2/common/event_v2.rs`](dll/src/desktop/shell2/common/event_v2.rs)

---

## 1. Architecture of the Application: The State Graph

Azul employs a reactive, declarative paradigm where the User Interface (UI) is a pure function of the application state (`UI = f(data)`). This architecture fundamentally separates the application's data structure from its visual manifestation, addressing the inherent conflict between the **Visual Tree** and the **State Graph**.

### Data Access and State Management

1. **Application Data ([`RefAny`](core/src/refany.rs#L401))**: The core application state (data models, database connections, application configuration) is encapsulated in an implementation-agnostic, **reference-counted pointer** called `RefAny`. This design choice enables C-compatibility.

2. **Visual Structure ([`Dom`](core/src/dom.rs#L2945))**: The user defines the UI structure using a Document Object Model (`Dom`), which is highly similar to valid XHTML. The `Dom` is constructed in a pure function (`layout` callback) that takes the application state (`RefAny`) and returns a structured, unstyled hierarchy.

3. **Synchronization**: Unlike immediate mode GUIs (IMGUI), the UI is only regenerated when a callback explicitly returns [`Update::RefreshDom`](core/src/callbacks.rs#L57). This **explicit update control** makes the render cycle efficient and predictable.

4. **Inter-Component Communication (Backreferences)**: Complex interactions between logically related but visually distant components are managed using **backreferences**. A lower-level component holds a direct `RefAny` reference and a callback to its specific higher-level logical controller (the *State Graph*), bypassing the visual hierarchy when passing information back up.

---

## 2. Event Handling: Two Complementary Systems

Azul uses a **unified V2 event processing pipeline** across all supported platforms (Windows, macOS, X11, Wayland). Event handling is split into **two complementary systems**:

1. **Window State Diffing** – For simple, user-modifiable state (mouse position, button status, window size). The system diffs `current_window_state` vs `previous_window_state` each frame.

2. **Manager-Based Accumulation** – For complex external events that require temporal context (multitouch gestures, drag operations, text input composition). Managers accumulate events over time until they can determine the correct high-level action.

The goal is **compartmentalization**: isolating the inherently messy interaction logic into dedicated managers rather than trying to eliminate complexity entirely.

### Event Processing Flow

1. **OS Input & State Capture**: The platform's event loop (e.g., `WndProc` on Windows, NSApplication on macOS) receives raw input and updates the internal [`FullWindowState`](layout/src/window_state.rs#L76) (including mouse position, button status, keyboard state, etc.).

2. **State Diffing (Simple Events)**: For user-modifiable state, the system compares `current_window_state` against `previous_window_state` to detect changes (mouse moved, button flipped, window resized). These generate [`SyntheticEvent`](core/src/events.rs#L497)s immediately.

3. **Manager Processing (Complex Events)**: For events requiring temporal context, specialized managers accumulate and interpret input:
   - [`GestureManager`](layout/src/managers/gesture.rs) – Tracks touch events with timestamps until a gesture pattern (pinch, swipe, etc.) is recognized
   - [`ScrollManager`](layout/src/managers/scroll_state.rs#L148) – Handles scroll momentum, inertia, and programmatic scrolling
   - [`CursorManager`](layout/src/managers/cursor.rs#L54) – Manages text cursor state across frames during editing

4. **Event Dispatch**: The `SyntheticEvent`s are filtered and routed via the [event processing pipeline](dll/src/desktop/shell2/common/event_v2.rs), using hit-testing to determine which [`DomNodeId`](core/src/dom.rs#L2921) receives each event.

5. **Callback Invocation**: The associated `RefAny` data and the callback function ([`CallbackType`](core/src/callbacks.rs) / [`Update`](core/src/callbacks.rs#L57) return type) attached to the targeted node are invoked.

6. **Result Processing**: The `Update` result from the callback determines the next steps: re-render, update display list, modify window properties, or perform recursive event re-entry if `Update::RefreshDom` was returned.

### OS-Specific Input Mechanisms

The platform layer's primary job is to keep `FullWindowState` synchronized:

| OS | Event Model | Key Input Mechanism | Window Decoration |
|:---|:---|:---|:---|
| **Windows** | Message Pump (`WndProc`, `WM_LBUTTONDOWN`) | Direct Windows messages (`WM_CHAR`, `WM_IME_COMPOSITION`) | Native (DWM) or custom (CSD) |
| **macOS** | NSApplication event loop (`NSEvent`) | NSTextInputClient protocol (IME) | Native (AppKit) or custom (CSD) |
| **X11** | Direct XNextEvent polling | XIM (X Input Method) for complex characters | Native (WM-managed) or custom (CSD) |
| **Wayland** | Protocol-based listeners (asynchronous, compositor-driven) | Text-Input protocol (or GTK fallback) | **Mandatory CSD** (Wayland has no native decoration protocol) |

---

## 3. Manager Interaction

The core runtime environment, encapsulated within the window's [`LayoutWindow`](layout/src/window.rs#L237) structure, holds specialized managers responsible for specific UI concerns.

| Manager | Location | Responsibility |
|:---|:---|:---|
| [`LayoutCache`](layout/src/solver3/cache.rs#L59) | `solver3/cache.rs` | Stores the entire layout tree and absolute node positions across frames for incremental layout |
| [`FontManager`](layout/src/text3/cache.rs#L254) | `text3/cache.rs` | Loads, caches, and resolves font definitions and fallback chains |
| [`ScrollManager`](layout/src/managers/scroll_state.rs#L148) | `managers/scroll_state.rs` | Tracks scroll positions and offsets for all scrollable nodes |
| [`FocusManager`](layout/src/managers/focus_cursor.rs#L27) | `managers/focus_cursor.rs` | Manages keyboard focus (which node has focus) |
| [`SelectionManager`](layout/src/managers/selection.rs#L43) | `managers/selection.rs` | Manages text selection ranges across different DOMs |
| [`CursorManager`](layout/src/managers/cursor.rs#L54) | `managers/cursor.rs` | Manages cursor position and text editing state |
| [`IFrameManager`](layout/src/managers/iframe.rs#L31) | `managers/iframe.rs` | Tracks nested DOMs (IFrames) and manages lazy loading/virtualization |
| [`GpuStateManager`](layout/src/managers/gpu_state.rs#L39) | `managers/gpu_state.rs` | Manages GPU-accelerated state (`transform`, `opacity`, scrollbar fades) |

---

## 4. How the Layout Engine Works (Roughly)

The core logic resides in the [`layout/src/solver3/`](layout/src/solver3/) directory and implements a modern CSS layout pipeline.

1. **Input**: The [`layout_document`](layout/src/solver3/mod.rs#L345) function takes a complete, fully styled DOM tree ([`StyledDom`](core/src/styled_dom.rs#L585)) and the current viewport boundaries.

2. **Preparation**: Fonts are resolved and loaded by the `FontManager` based on the requested `font-family` chains.

3. **Layout Pass**: The engine traverses the DOM, establishing **Formatting Contexts (FC)** for each node:
   - **Block/Inline Contexts**: Handled by the custom CSS engine ([BFC and IFC implementations](layout/src/solver3/fc.rs)). IFC delegates complex text handling to `text3`.
   - **Flex/Grid Contexts**: Delegated to the underlying [**Taffy**](layout/src/solver3/taffy_bridge.rs) layout library for robust modern layout calculation.

4. **Text Layout ([`text3`](layout/src/text3/))**: For text nodes (within an IFC), the specialized `text3` engine performs sophisticated typesetting: bidirectional (BIDI) analysis, font shaping via [allsorts](layout/src/font.rs), line breaking (including hyphenation/Knuth-Plass algorithm), and glyph positioning.

5. **Output**: The process generates a **Layout Tree** (storing box metrics, size, format context) and a map of final, absolute screen positions for all nodes.

---

## 5. How the CSS System Works (Key Structs)

The CSS system handles parsing, resolving cascading conflicts, and property storage.

| Struct | Location | Purpose |
|:---|:---|:---|
| [`Css`](css/src/css.rs#L14) | `css/src/css.rs` | Top-level container for all parsed stylesheets in a document |
| [`Stylesheet`](css/src/css.rs#L62) | `css/src/css.rs` | Represents one complete stylesheet (e.g., loaded from a file) |
| [`CssRuleBlock`](css/src/css.rs#L346) | `css/src/css.rs` | A single block of rules (CSS ruleset) associated with a specific selector path |
| [`CssDeclaration`](css/src/css.rs#L93) | `css/src/css.rs` | Represents a `key: value` pair, classified as either `Static` or `Dynamic` |
| [`CssProperty`](css/src/props/property.rs#L486) | `css/src/props/property.rs` | An enum representing a specific CSS property (e.g., `LayoutWidth`, `StyleTextColor`) |
| [`CssPropertyCachePtr`](core/src/prop_cache.rs#L1118) | `core/src/prop_cache.rs` | Pointer to the centralized cache holding final computed values after inheritance and cascading |

---

## 6. Font and Image Loading

Resource loading is tightly integrated with caching and cross-platform handling.

### Font Loading

1. **Definition**: Fonts are represented by [`FontRef`](css/src/props/basic/font.rs#L196), a reference-counted structure pointing to the parsed font data ([`ParsedFont`](layout/src/font.rs#L161)).

2. **Resolution**: The `FontManager` uses the operating system's Fontconfig cache (`FcFontCache` from `rust_fontconfig`) to resolve CSS `font-family` stacks into concrete font fallback chains.

3. **Loading**: The bytes for required font IDs are loaded from disk and parsed using [allsorts](layout/src/font.rs) to extract font metrics ([`FontMetrics`](css/src/props/basic/font.rs#L691)), glyph outlines, and shaping tables (GSUB/GPOS).

4. **Caching**: Loaded fonts are inserted into the `FontManager`'s internal cache for sharing and quick lookup by hash.

### Image and SVG Loading

- Azul provides Rust utilities for decoding and encoding images and handling SVG assets.
- SVG files undergo parsing via [resvg](layout/src/xml/svg.rs), simplification (`usvg`), and GPU-optimized vector triangulation (`lyon`) during the load process, with the resulting data stored in the [`ImageCache`](core/src/resources.rs#L623).
- Images are referenced using unique keys ([`ImageKey`](core/src/resources.rs#L225)) and managed by the [`RendererResources`](core/src/resources.rs#L762) structure, which tracks GPU texture usage.

---

## 7. Rendering Pipeline: Display List to Screen

The actual rendering is handled by the high-performance, GPU-accelerated Mozilla **WebRender** engine.

1. **Display List Generation**: After layout is complete, the `LayoutWindow` calls [`layout_and_generate_display_list()`](layout/src/window.rs#L484) to produce a highly optimized, linear array of drawing primitives ([`DisplayList`](layout/src/solver3/display_list.rs#L176)). This list contains items like `Rect`, `Border`, `Text`, `Image`, and `PushStackingContext`.

2. **WebRender Translation**: This `DisplayList` is passed to the [`compositor2`](dll/src/desktop/compositor2.rs) module, which translates Azul's custom primitives into the formats understood by WebRender (e.g., converting text/font hashes into WebRender `FontKey` and `FontInstanceKey`s).

3. **Resource Synchronization**: The translation process generates `ResourceUpdate` messages (`AddFont`, `AddImage`, etc.) required by WebRender's internal resource cache. These inform the GPU thread about new assets that need uploading.

4. **Transaction Submission**: All display commands, resource updates, and scroll offsets are packaged into a WebRender `Transaction`. This transaction is sent from the application thread to the WebRender **Render Backend** thread.

5. **Scene Building & GPU Work**: The Render Backend thread processes the transaction, builds the complex stacking context tree and scene primitives, and eventually signals the main thread via a notifier when the frame is ready in the backbuffer.

6. **Presentation**: The main thread (typically triggered by a native OS event like `WM_PAINT` or `drawRect:`) commands the GPU to present the finalized frame by performing a buffer swap, making the computed scene visible on the screen. GPU-accelerated properties (`opacity`, `transform`) can update the frame efficiently without requiring a full layout pass.

---

## 8. Open Questions for New Maintainers

These are common questions that may require further exploration:

1. **How do I add a new CSS property?** – The `CssProperty` enum and parsing logic need to be extended.

2. **Where are the platform entry points?** – Look in `dll/src/desktop/shell2/{platform}/mod.rs` for each platform.

3. **How do IFrames and virtualization work?** – See `IFrameManager` and the `IFrameCallback` system.

4. **When does `solver3` vs `taffy_bridge` handle layout?** – Block/Inline contexts use the custom engine; Flex/Grid containers delegate to Taffy.

5. **How are animations handled?** – GPU-accelerated properties go through `GpuStateManager`; see also `core/src/animation.rs`.

6. **What is `text3`?** – The third iteration of the text layout engine, supporting advanced typography features.

7. **How do I debug layout issues?** – Check `LayoutDebugMessage` and the debug server in `shell2/common/debug_server.rs`.

8. **What's the testing strategy?** – See `tests/` directories in each crate and `doc/src/reftest/` for visual regression tests.

---

## Q1: How should `update_text_cache_after_edit()` work with the CORRECT architecture?

The correct approach is to perform an "optimistic update" directly on the visual representation stored in the `LayoutCache`, leaving the `StyledDom` untouched. This function's purpose is to quickly regenerate the visual layout for the edited text block and swap it into the cache, triggering a repaint but not a full, expensive relayout of the entire window.

**Implementation Strategy:**

The function will perform a miniature, self-contained layout pass for the edited content.

1.  **Find Cache Entry:** The visual layout for a text block (an Inline Formatting Context) is stored in the `LayoutTree` within the `DomLayoutResult`. We need to find the `LayoutNode` corresponding to the `node_id` (which must be the IFC root) and get mutable access to its `inline_layout_result`.
2.  **Get Constraints:** The original layout was performed under a specific set of `UnifiedConstraints` (available width, text alignment, etc.). We must reuse these exact constraints to ensure the optimistic update is visually consistent. We can cache these constraints in `LayoutWindow` for this purpose.
3.  **Re-run Lightweight Layout:** We will re-run the text layout pipeline (`create_logical_items` -> `reorder_logical_items` -> `shape_visual_items` -> `perform_fragment_layout`) on the `new_inline_content` using the cached constraints. This produces a new `UnifiedLayout`.
4.  **Update Cache:** We will use `Arc::make_mut` to get a mutable reference to the `CachedLayout` within the `LayoutNode` and replace its internal `UnifiedLayout` with our newly generated one.
5.  **Trigger Repaint:** The caller (`apply_text_changeset`) is responsible for marking the window as needing a repaint, which will cause the display list to be regenerated from the now-updated `LayoutCache`.

Here is the implementation:

```rust
// In layout/src/window.rs

pub fn update_text_cache_after_edit(
    &mut self,
    dom_id: DomId,
    node_id: NodeId, // This must be the IFC root node
    new_inline_content: Vec<InlineContent>,
) {
    // --- Step 1: Get mutable access to the layout results for this DOM ---
    let Some(layout_result) = self.layout_results.get_mut(&dom_id) else {
        // This DOM isn't laid out, can't update it.
        return;
    };

    // --- Step 2: Find the LayoutNode and its cached constraints ---
    // The `node_id` here is the IFC root, which is what we need.
    let Some(layout_indices) = layout_result.layout_tree.dom_to_layout.get(&node_id) else {
        return;
    };
    let Some(&layout_index) = layout_indices.first() else {
        return;
    };
    let Some(layout_node) = layout_result.layout_tree.nodes.get_mut(layout_index) else {
        return;
    };

    // Retrieve the constraints used for the original layout.
    // We must cache these during the initial layout pass.
    let Some(constraints) = self.text_constraints_cache.constraints.get(&(dom_id, node_id)).cloned() else {
        // Without constraints, we can't perform a consistent optimistic update.
        return;
    };

    // --- Step 3: Re-run a lightweight layout pass on the new content ---
    // This re-uses the text3 pipeline functions to produce a new UnifiedLayout.
    let new_layout = {
        use crate::text3::cache::{create_logical_items, reorder_logical_items, shape_visual_items, perform_fragment_layout, BreakCursor};

        // Stages 1-3: Logical analysis, Bidi, Shaping
        let logical_items = create_logical_items(&new_inline_content, &[], &mut None);
        let base_direction = constraints.direction.unwrap_or(BidiDirection::Ltr);
        let visual_items = reorder_logical_items(&logical_items, base_direction, &mut None).unwrap_or_default();
        let loaded_fonts = self.font_manager.get_loaded_fonts();
        let shaped_items = shape_visual_items(&visual_items, self.font_manager.get_font_chain_cache(), &self.font_manager.fc_cache, &loaded_fonts, &mut None).unwrap_or_default();

        // Stage 4: Positioning (using a simplified fragment layout)
        let mut cursor = BreakCursor::new(&shaped_items);
        perform_fragment_layout(&mut cursor, &logical_items, &constraints, &mut None, &loaded_fonts).ok()
    };

    let Some(new_unified_layout) = new_layout else {
        return;
    };

    // --- Step 4: Update the cache with the new layout ---
    if let Some(cached_layout_arc) = &mut layout_node.inline_layout_result {
        // Get a mutable reference to the CachedLayout. This will create a new Arc
        // if the layout is shared, ensuring we don't mutate a shared resource.
        let cached_layout = Arc::make_mut(cached_layout_arc);
        // Replace the old layout with the new one.
        cached_layout.layout = new_unified_layout;
    }

    // The caller is responsible for triggering a repaint.
}
```

## Q2: How do onchange/oninput callbacks work?

The `oninput` and `onchange` callbacks are central to the "controlled component" architecture. They are the mechanism by which the UI informs the application's data model of a proposed change.

-   **`get_text_changeset()` Connection:** The `CallbackInfo` struct, passed to every callback, contains a pointer to the `LayoutWindow`. The `get_text_changeset()` method on `CallbackInfo` simply accesses `layout_window.text_input_manager.get_pending_changeset()` to retrieve the information about the edit that triggered the `Input` event.

-   **`Input` vs. `Change` Event:**
    -   **`Input` (`On::TextInput`):** This event fires *immediately* for any modification to the value of a contenteditable element (typing, pasting, deleting). It represents a proposed, uncommitted change. This is where the user's callback should update their data model.
    -   **`Change`:** This event is not explicitly shown in the provided code but follows standard web behavior. It should fire when the user *commits* a change, which typically happens when the element loses focus (`On::FocusLost`) and its value has been modified since it gained focus.

-   **`preventDefault()`:** When a callback calls `info.prevent_default()`, it pushes a `CallbackChange::PreventDefault` variant into a shared list of changes. After all callbacks for an event have run, the event loop in `event_v2.rs` checks this list. If `PreventDefault` is present, it skips the "apply" phase of the edit. For text input, this means `apply_text_changeset()` is never called, the optimistic visual update is discarded (on the next full layout), and the `TextInputManager`'s pending changeset is cleared.

-   **Contenteditable Pattern:** A `contenteditable` element should behave identically to a custom `TextInput` widget.
    1.  The element is marked as `contenteditable="true"`.
    2.  An `On::TextInput` callback is attached to it.
    3.  When the user types, the framework's `TextInputManager` records the edit and generates a synthetic `Input` event.
    4.  The event is dispatched to the `contenteditable` element.
    5.  The `On::TextInput` callback fires, receiving `CallbackInfo`.
    6.  The callback calls `info.get_text_changeset()` to see what changed.
    7.  The callback updates the application's data model (`RefAny`).
    8.  The framework performs the optimistic visual update via `update_text_cache_after_edit`.
    9.  If the user's callback returns `Update::RefreshDom`, a full layout pass is triggered, rebuilding the `StyledDom` from the (now updated) data model and overwriting the optimistic update with the committed state.

## Q3: How should cursor click positioning work with LayoutCache architecture?

Cursor positioning on click must operate on the final visual representation in the `LayoutCache`, which is the `UnifiedLayout` containing `PositionedItem`s. The key is to map a viewport coordinate to a logical `TextCursor`.

The function `UnifiedLayout::hittest_cursor` in `layout/src/text3/cache.rs` already provides a good starting point. We will expose this through a public helper function.

-   **Function Signature:** `pub fn hit_test_text_at_point(layout: &UnifiedLayout, point: LogicalPosition) -> Option<TextCursor>` is appropriate. The `point` must be relative to the origin of the `UnifiedLayout`'s container.

-   **Affinity (Between Clusters):** The affinity is determined by which half of a grapheme cluster's bounding box is clicked. If the click is on the leading half (left side in LTR), affinity is `Leading`. If on the trailing half, it's `Trailing`.

-   **Empty Lines:** An empty line will have no `ShapedCluster` items. The hit-testing logic should handle this by finding the closest line vertically. If that line is empty, it should find the last cluster of the previous line and return a `Trailing` cursor, or if there's no previous line, the first cluster of the next line with a `Leading` cursor. The existing `hittest_cursor` implementation in `text3/cache.rs` already has logic for this.

**Implementation:**

We can add a public function in `layout/src/text3/selection.rs` that wraps the logic from `UnifiedLayout::hittest_cursor`.

```rust
// In layout/src/text3/selection.rs

use azul_core::geom::LogicalPosition;
use azul_core::selection::{TextCursor, CursorAffinity};
use crate::text3::cache::{UnifiedLayout, PositionedItem, ShapedItem};
use std::collections::BTreeMap;

/// Takes a point relative to the layout's origin and returns the closest
/// logical cursor position.
pub fn hit_test_text_at_point(
    layout: &UnifiedLayout,
    point: LogicalPosition,
) -> Option<TextCursor> {
    if layout.items.is_empty() {
        // For a completely empty contenteditable, return a cursor at the beginning.
        return Some(TextCursor {
            cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: 0 },
            affinity: CursorAffinity::Leading,
        });
    }

    // --- Step 1: Find the line closest to the click's Y coordinate ---
    let mut closest_line_idx = 0;
    let mut min_vertical_dist = f32::MAX;

    // Group items by line to find the vertical center of each line
    let mut line_bounds: BTreeMap<usize, (f32, f32)> = BTreeMap::new(); // (min_y, max_y)
    for item in &layout.items {
        let item_bounds = item.item.bounds();
        let (min_y, max_y) = line_bounds.entry(item.line_index).or_insert((f32::MAX, f32::MIN));
        *min_y = min_y.min(item.position.y);
        *max_y = max_y.max(item.position.y + item_bounds.height);
    }

    for (line_idx, (min_y, max_y)) in &line_bounds {
        let line_center_y = min_y + (max_y - min_y) / 2.0;
        let dist = (point.y - line_center_y).abs();
        if dist < min_vertical_dist {
            min_vertical_dist = dist;
            closest_line_idx = *line_idx;
        }
    }

    // --- Step 2: Find the horizontally closest cluster on that line ---
    let mut closest_cluster_item: Option<&PositionedItem> = None;
    let mut min_horizontal_dist = f32::MAX;

    for item in layout.items.iter().filter(|i| i.line_index == closest_line_idx) {
        if let ShapedItem::Cluster(_) = &item.item {
            let item_bounds = item.item.bounds();
            let dist = if point.x < item.position.x {
                item.position.x - point.x
            } else if point.x > item.position.x + item_bounds.width {
                point.x - (item.position.x + item_bounds.width)
            } else {
                0.0 // Inside the cluster horizontally
            };

            if dist < min_horizontal_dist {
                min_horizontal_dist = dist;
                closest_cluster_item = Some(item);
            }
        }
    }
    
    // If no cluster is found on the line (e.g., an empty line), find the last cluster
    // on the previous line or the first on the next to handle clicks in empty space.
    let target_item = closest_cluster_item.or_else(|| {
        layout.items.iter().rev().find(|i| i.line_index < closest_line_idx && i.item.as_cluster().is_some())
    })?;

    let cluster = target_item.item.as_cluster()?;

    // --- Step 3: Determine affinity based on which half of the cluster was clicked ---
    let cluster_mid_x = target_item.position.x + cluster.advance / 2.0;
    let affinity = if point.x < cluster_mid_x {
        CursorAffinity::Leading
    } else {
        CursorAffinity::Trailing
    };

    Some(TextCursor {
        cluster_id: cluster.source_cluster_id,
        affinity,
    })
}
```

## Q4: How should focus transfer work?

Focus transfer requires careful coordination between the `FocusManager` and `CursorManager`, respecting the "flag and defer" pattern for non-click focus events.

-   **Clearing A's Visual State:** When contenteditable A loses focus, a `FocusLost` event is generated. The event handler in `window.rs` (`handle_focus_change_for_cursor_blink`) should:
    1.  Call `self.cursor_manager.clear()`. This sets `cursor` to `None`, `is_visible` to `false`, and stops the blink timer logic for that cursor.
    2.  Return `CursorBlinkTimerAction::Stop`, which tells the platform shell to stop the native timer.
    3.  The next repaint will not draw a cursor for A.

-   **Setting B's Cursor on Click:**
    1.  A `MouseDown` event is generated for B.
    2.  The event handler in `event_v2.rs` calls the hit-testing function from Q3 to get the precise `TextCursor` at the click location.
    3.  It then calls `self.focus_manager.set_focused_node(Some(B))`. This is crucial.
    4.  This focus change immediately generates `FocusLost` for A and `FocusReceived` for B.
    5.  The `FocusReceived` handler for B will set the `cursor_needs_initialization` flag and return `CursorBlinkTimerAction::Start`.
    6.  **Crucially, the `MouseDown` handler continues.** It now calls `self.cursor_manager.set_cursor_with_time(new_cursor, location_B, now)`. This *overrides* the deferred initialization, placing the cursor at the precise click location immediately and resetting the blink timer. This is the correct and desired behavior.

-   **Tab Navigation (No Click):**
    1.  A `KeyDown` event for Tab triggers a `FocusTarget::Next` request.
    2.  The event loop resolves this to node B and calls `self.focus_manager.set_focused_node(Some(B))`.
    3.  This generates `FocusLost` on A (clearing its cursor) and `FocusReceived` on B.
    4.  The `FocusReceived` handler for B, seeing it's contenteditable, calls `self.focus_manager.set_pending_contenteditable_focus(...)` and starts the blink timer.
    5.  The `MouseDown` handler is never called.
    6.  After the next layout pass, `LayoutWindow::finalize_pending_focus_changes()` is called. It consumes the pending focus flag, gets the now-valid `UnifiedLayout` for B, and calls `self.cursor_manager.initialize_cursor_at_end()` to place the cursor at the end of the text.

## Q5: How should inline images from clipboard paste work?

The `LayoutCache` architecture makes this straightforward. An inline image is just another piece of content to be rendered, represented as a `ShapedItem::Object`.

-   **Changeset Representation:** The `Vec<InlineContent>` structure is perfect for this. The `InlineContent` enum already has an `Image(InlineImage)` variant. When an image is pasted, the platform layer creates an `ImageRef` (after decoding the image data and adding it to the `ImageCache`) and wraps it in an `InlineContent::Image`. This item is then inserted into the `Vec<InlineContent>` at the cursor position.

-   **User Callback Handling:** The `oninput` callback receives the `new_inline_content` vector. The user's application code should iterate through this vector.
    -   When it encounters `InlineContent::Text`, it appends the text to its data model.
    -   When it encounters `InlineContent::Image(inline_image)`, it should inspect the `inline_image.source`. This source contains the `ImageRef`, which has a unique hash. The application should store this hash or a unique key in its data model to represent the image. When the `layout()` function is next called, it should read this key from the data model and create a `Dom::image(image_ref)` node.

The optimistic update in `update_text_cache_after_edit` will take the `Vec<InlineContent>` (containing the `InlineContent::Image`), run it through the lightweight layout pipeline, which will produce a `ShapedItem::Object`, and place that directly into the `UnifiedLayout` for immediate rendering.

## Q6: Complete Implementation Plan

### Step 1: Implement Optimistic Updates in `update_text_cache_after_edit`

This is the foundational fix for **Problem 1 (Text Input Doesn't Update Display)**.

1.  **Add a cache for text constraints:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** Add a `TextConstraintsCache` to `LayoutWindow` to store the `UnifiedConstraints` used for each text block during the main layout pass. This is essential for performing a consistent optimistic update.
    ```rust
    // In layout/src/window.rs, struct LayoutWindow
    pub struct LayoutWindow {
        // ... existing fields
        text_constraints_cache: TextConstraintsCache,
    }

    // In layout/src/window.rs, struct TextConstraintsCache
    #[derive(Debug, Clone, Default)]
    pub struct TextConstraintsCache {
        pub constraints: BTreeMap<(DomId, NodeId), UnifiedConstraints>,
    }
    ```
    *   **Action:** In `layout/src/solver3/fc.rs`, inside `layout_inline_formatting_context`, cache the constraints before calling `text_cache.layout_text`.
    ```rust
    // In layout/src/solver3/fc.rs, inside layout_inline_formatting_context
    // ... after creating `constraints` ...
    ctx.text_constraints_cache.constraints.insert((ctx.styled_dom.dom_id, dom_node_id), constraints.clone());
    ```

2.  **Implement `update_text_cache_after_edit`:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** Replace the stub with the full implementation from the **Q1 answer** above.

3.  **Test Cases for Step 1:**
    *   Focus a contenteditable div.
    *   Type a character. **Expected:** The character appears instantly. The `StyledDom` remains unchanged.
    *   Type several more characters. **Expected:** Text updates visually.
    *   Trigger a full `layout()` call (e.g., by resizing the window slightly). **Expected:** The visual text reverts to the original `StyledDom` content, because the application's data model hasn't been updated yet.

### Step 2: Implement Cursor Positioning on Click

This addresses **Problem 2 (Cursor Doesn't Reposition on Click)**.

1.  **Add `hit_test_text_at_point` function:**
    *   **File:** `layout/src/text3/selection.rs`
    *   **Action:** Add the full implementation from the **Q3 answer** above.

2.  **Integrate Hit-Testing into `MouseDown` handler:**
    *   **File:** `dll/src/desktop/shell2/common/event_v2.rs`
    *   **Action:** Modify the `handle_mouse_down` logic within the `PlatformWindowV2` trait (or a function it calls).
    ```rust
    // Inside a function like process_mouse_click_for_selection or similar
    // called from the MouseDown handler in event_v2.rs

    if let Some(layout_window) = self.get_layout_window_mut() {
        // Use the hover manager's hit test results
        use azul_layout::managers::hover::InputPointId;
        if let Some(hit_test) = layout_window.hover_manager.get_current(&InputPointId::Mouse) {
            for (dom_id, hit_data) in &hit_test.hovered_nodes {
                // Find the deepest hit node that is contenteditable
                if let Some((node_id, hit_item)) = hit_data.get_deepest_contenteditable_node(layout_window, *dom_id) {
                    
                    // Find the IFC root for this text node
                    let ifc_root_id = layout_window.find_contenteditable_ancestor(*dom_id, node_id)
                        .unwrap_or(node_id);

                    if let Some(layout) = layout_window.get_inline_layout_for_node(*dom_id, ifc_root_id) {
                        
                        // Get node's absolute position to calculate local click position
                        let node_pos = layout_window.get_node_position(DomNodeId { dom: *dom_id, node: ifc_root_id.into() }).unwrap_or_default();
                        let local_click_pos = LogicalPosition {
                            x: position.x - node_pos.x,
                            y: position.y - node_pos.y,
                        };

                        // Perform hit test
                        if let Some(new_cursor) = azul_layout::text3::selection::hit_test_text_at_point(&layout, local_click_pos) {
                            
                            // Set focus to the contenteditable container
                            layout_window.focus_manager.set_focused_node(Some(DomNodeId { dom: *dom_id, node: ifc_root_id.into() }));

                            // Set cursor position and reset blink timer
                            let now = azul_core::task::Instant::now();
                            layout_window.cursor_manager.set_cursor_with_time(Some(new_cursor), Some(CursorLocation { dom_id: *dom_id, node_id: ifc_root_id }), now);

                            // Clear any existing selection
                            layout_window.selection_manager.clear_text_selection(dom_id);
                            
                            // Break after handling the first valid hit
                            break;
                        }
                    }
                }
            }
        }
    }
    ```

3.  **Test Cases for Step 2:**
    *   Click at the beginning, middle, and end of a line of text. **Expected:** Cursor appears at the correct position.
    *   Click on a different line in a multi-line input. **Expected:** Cursor moves to the clicked line.
    *   Click in an empty contenteditable div. **Expected:** Cursor appears at the start.

### Step 3: Fix Focus Transfer and Inline Image Pasting

This addresses **Problem 3 (Focus Transfer)** and **Q5 (Inline Images)**.

1.  **Review Focus Handlers:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** Ensure `handle_focus_change_for_cursor_blink` and `finalize_pending_focus_changes` are implemented as described in the **Q4 answer**. The logic in the source code seems mostly correct, but needs to be verified in the context of the new architecture.

2.  **Implement Image Pasting:**
    *   **File:** Platform-specific clipboard handling (e.g., `dll/src/desktop/shell2/windows/clipboard.rs`)
    *   **Action:** When pasting, check the clipboard for image data. If found, decode it, add it to the `ImageCache` to get an `ImageRef`.
    *   **File:** `dll/src/desktop/shell2/common/event_v2.rs`
    *   **Action:** In the paste handler (e.g., for `Ctrl+V`), create an `InlineContent::Image` and insert it into a `Vec<InlineContent>`. Then, call `layout_window.record_text_input` with this rich content represented as a special string (e.g., `\u{FFFC}` object replacement character) and store the `InlineContent` in a temporary field for `apply_text_changeset` to use.

3.  **Test Cases for Step 3:**
    *   Click from one text input to another. **Expected:** Cursor disappears from the first and appears at the click location in the second.
    *   Use the Tab key to switch between inputs. **Expected:** Focus moves, and a cursor appears at the end of the text in the newly focused input.
    *   Copy an image to the clipboard and paste it into a contenteditable. **Expected:** The image appears inline with the text. The `oninput` callback should receive a changeset containing an `InlineContent::Image`.

### Step 4: Enable Multi-Node Selections and Edits

This is the final step, ensuring the architecture correctly handles complex edits like deleting styled text.

1.  **Enhance `get_text_before_textinput`:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** This function needs to be aware of multi-node selections. It should take a `SelectionRange` that can span multiple nodes. It will need to traverse the DOM between the start and end of the selection, collecting `InlineContent` from all text and image nodes it encounters.

2.  **Verify `text3::edit.rs`:**
    *   **File:** `layout/src/text3/edit.rs`
    *   **Action:** The functions in this module (`edit_text`, `delete_range`, etc.) already operate on `&[InlineContent]`. Their logic for handling edits that span multiple `StyledRun`s needs to be completed and thoroughly tested. This is the core of multi-node editing logic.

3.  **User `onchange` Callback Responsibility:**
    *   **Action:** This is a documentation/API contract point. The user must understand that when their `onchange` callback receives a `Vec<InlineContent>` that resulted from a multi-node edit, it is their responsibility to update their data model in a way that reflects the new structure. For example, deleting `<b>bold</b>` from `normal <b>bold</b> text` might result in a single `InlineContent::Text` run. The user's callback would need to update their data model to produce a single text node on the next `layout()` call.

4.  **Test Cases for Step 4:**
    *   Select text that includes a `<b>` tag (e.g., `normal [<b>bold</b>] text`).
    *   Press Backspace. **Expected:** The bold text is deleted visually. The `oninput` callback receives a changeset reflecting the deletion. The user's data model is updated. The next full `layout()` call renders the correct, committed DOM structure.
    *   Select text across two separate `<p>` tags. Press Backspace. **Expected:** The text is deleted, and the two paragraphs are merged into one (this is complex and depends on the `edit_text` implementation). The optimistic update shows the merged text. The `onchange` callback receives the new content, and the user's data model is updated to reflect the single paragraph.