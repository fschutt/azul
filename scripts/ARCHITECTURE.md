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