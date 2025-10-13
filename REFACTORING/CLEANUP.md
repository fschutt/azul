# Module Cleanup Plan

**Created:** 13. Oktober 2025  
**Status:** Planning Phase  
**Focus:** Mixed responsibilities, NOT file size

## Overview

This document outlines the plan to reorganize azul-core modules for better discoverability and separation of concerns. Analysis is based on **mixed responsibilities within modules**, not just file length. A 3000-line file with one clear purpose is fine; a 400-line file mixing unrelated concepts needs splitting.

---

## Analysis Criteria

✅ **Good Module:** Single responsibility, cohesive types, clear purpose  
⚠️ **Mixed Responsibilities:** Multiple unrelated concerns in one file  
📝 **Missing Documentation:** No `//!` module-level documentation  
🔧 **Long but OK:** Large file size but single, clear responsibility

---

## Module Inventory

### ✅ Well-Organized Modules (NO CHANGES NEEDED)

| Module | Lines | Types | Status | Reason |
|--------|-------|-------|--------|--------|
| **prop_cache.rs** | 2965 | 2 | ✅ OK | Single responsibility: CSS property caching. Has docs. |
| **svg.rs** | 1384 | 39 | ✅ OK | Single responsibility: SVG rendering. All types related. Has docs. |
| **id.rs** | 1003 | 17 | ✅ OK | Single responsibility: Node tree data structures. Has docs. |
| **transform.rs** | 859 | 2 | ✅ OK | Single responsibility: 3D transforms. Has docs. |
| **task.rs** | 777 | 19 | ✅ OK | Single responsibility: Threading and timers. No docs needed (simple). |
| **menu.rs** | 335 | 7 | ✅ OK | Single responsibility: Menu system. Has docs. |
| **gpu.rs** | 284 | 3 | ✅ OK | Single responsibility: GPU value caching. Has docs. |
| **window_state.rs** | 235 | 4 | ✅ OK | Single responsibility: Event filtering. Has docs. |
| **glyph.rs** | 189 | 10 | ✅ OK | Single responsibility: Glyph placement. No docs (simple). |
| **selection.rs** | 127 | 7 | ✅ OK | Single responsibility: Text selection. Has docs. |
| **style.rs** | 368 | 4 | ✅ OK | Single responsibility: CSS cascading. Has docs. |

---

### ⚠️ PROBLEM: Mixed Responsibilities

## 🔴 Priority 1: xml.rs - 3 DIFFERENT CONCERNS

**Lines:** 3022 | **Types:** 40+ | **Functions:** 64  
**Problem:** Mixes XML parsing + Component system + Rust code generation  
**Has Documentation:** ✅ Yes (but covers all 3 concerns together)

### Current Responsibilities:

1. **XML Parsing Infrastructure** (Low-level)
   - Types: `Xml`, `XmlNodeType`, `XmlError`, `XmlParseError`, etc.
   - Functions: `find_node_by_type`, `find_attribute`, `normalize_casing`
   
2. **Component System** (Domain Logic)
   - Types: `XmlComponent`, `XmlComponentMap`, `ComponentArguments`, `XmlNode`, `DomXml`
   - Functions: `str_to_dom`, `render_dom_from_body_node`, `render_component_inner`
   
3. **Rust Code Generation** (Compiler)
   - Types: `CompileError`, `CssMatcher`, `DynamicItem`, Renderers
   - Functions: `str_to_rust_code`, `compile_components`, `compile_body_node_to_rust_code`

### Split Plan:

```
core/src/
  xml.rs         - XML parsing infrastructure ONLY (12 types, 8 functions, ~800 lines)
  component.rs   - Component system + DOM rendering (15 types, 25 functions, ~600 lines)
  codegen.rs     - Rust code generation (8 types, 31 functions, ~1600 lines)
```

### Migration Details:

**xml.rs** - Keep XML parsing:
- **Types (12):**
  - `Xml`, `XmlNodeType`, `XmlQualifiedName`
  - `XmlError`, `XmlParseError`, `XmlStreamError`
  - `XmlTextPos`, `XmlTextError`
  - `NonXmlCharError`, `InvalidCharError`, `InvalidQuoteError`, `InvalidSpaceError`
  - `DuplicatedNamespaceError`, `UnknownNamespaceError`, `UnexpectedCloseTagError`, `DuplicatedAttributeError`
- **Functions (8):**
  - `find_node_by_type()`
  - `find_attribute()`
  - `get_item()`
  - `normalize_casing()`
  - `parse_bool()`
  - XML stream parsing helpers

**component.rs** - Move component system:
- **Types (15):**
  - `XmlComponent`, `XmlComponentMap`
  - `ComponentArguments`, `FilteredComponentArguments`
  - `DynamicXmlComponent`
  - `XmlNode`, `DomXml`
  - `ComponentParseError`, `ComponentError`, `RenderDomError`, `DomXmlParseError`
  - `DivRenderer`, `BodyRenderer`, `BrRenderer`, `TextRenderer`
- **Functions (25):**
  - `parse_component_arguments()`
  - `validate_and_filter_component_args()`
  - `get_html_node()`, `get_body_node()`
  - `str_to_dom()`
  - `render_dom_from_body_node()`
  - `render_dom_from_body_node_inner()`
  - `render_component_inner()`
  - `set_attributes()`, `set_stringified_attributes()`
  - All other rendering functions

**codegen.rs** - Move code generation:
- **Types (5):**
  - `CompileError`
  - `CssMatcher`
  - `DynamicItem`
  - Internal helper types
- **Functions (31):**
  - `str_to_rust_code()` ← Main entry point
  - `compile_components()`
  - `compile_component()`
  - `compile_components_to_rust_code()`
  - `compile_body_node_to_rust_code()`
  - `compile_node_to_rust_code_inner()`
  - `format_component_args()`
  - `format_args_dynamic()`
  - `format_args_for_rust_code()`
  - `split_dynamic_string()`
  - `combine_and_replace_dynamic_items()`
  - `compile_and_format_dynamic_items()`
  - `prepare_string()`
  - `get_css_blocks()`
  - `group_matches()`
  - All other compilation/formatting functions

---

## 🔴 Priority 1: ui_solver.rs - MULTIPLE UNRELATED TYPES

**Lines:** 412 | **Types:** 13 | **Functions:** 0  
**Problem:** Mixes scrolling + layout contexts + glyphs + hit testing + overflow + positioning  
**Has Documentation:** 📝 NO (needs documentation)

### Current Responsibilities:

1. **Scrolling** - `ExternalScrollId`, `ScrolledNodes`, `OverflowingScrollNode`
2. **Layout Contexts** - `FormattingContext` (Block, Inline, Flex, Table, Grid, etc.)
3. **Text Layout** - `GlyphInstance`, `QuickResizeResult`
4. **Hit Testing** - `HitTest`
5. **Overflow** - `OverflowInfo`, `DirectionalOverflowInfo`
6. **Positioning** - `PositionInfo`, `PositionInfoInner`
7. **GPU Events** - `GpuOpacityKeyEvent`
8. **Box Shadow** - `StyleBoxShadowOffsets`

### Split Plan:

```
core/src/
  scroll.rs          - Scrolling infrastructure (3 types, ~100 lines)
  layout_context.rs  - FormattingContext enum (1 type, ~80 lines)
  text_layout.rs     - GlyphInstance, QuickResizeResult (2 types, ~50 lines)
  hit_test.rs        - HitTest type (1 type, ~30 lines)
  overflow.rs        - OverflowInfo, DirectionalOverflowInfo (2 types, ~80 lines)
  position.rs        - PositionInfo, PositionInfoInner (2 types, ~100 lines)
  box_shadow.rs      - StyleBoxShadowOffsets (1 type, ~20 lines)
```

### Migration Details:

**scroll.rs** - NEW FILE:
- **Types (3):**
  - `ExternalScrollId` - Identifies scrollable regions
  - `ScrolledNodes` - Map of overflowing nodes + clip nodes
  - `OverflowingScrollNode` - Data for a single overflowing node
- **Constants:**
  - None
- **Use Case:** Managing scrollable regions and overflow tracking

**layout_context.rs** - NEW FILE:
- **Types (1):**
  - `FormattingContext` - CSS formatting context enum (Block, Inline, Flex, Table, Grid, Float, OutOfFlow, etc.)
- **Use Case:** Determining layout algorithm based on CSS display/position

**text_layout.rs** - NEW FILE (or merge into glyph.rs?):
- **Types (2):**
  - `GlyphInstance` - Single glyph with position and size
  - `QuickResizeResult` - Result of quick resize operation
- **Use Case:** Text shaping and layout results

**hit_test.rs** - NEW FILE:
- **Types (1):**
  - `HitTest` - Hit test result with node and local position
- **Use Case:** Mouse/touch hit testing

**overflow.rs** - NEW FILE:
- **Types (2):**
  - `OverflowInfo` - Overflow amounts in both directions
  - `DirectionalOverflowInfo` - Overflow in one direction with scrollbar calculation
- **Use Case:** Calculating overflow and determining scrollbar necessity

**position.rs** - NEW FILE:
- **Types (2):**
  - `PositionInfo` - Static or Dynamic positioning information
  - `PositionInfoInner` - Inner positioning data with offsets
- **Use Case:** Storing final computed positions for layout nodes

**box_shadow.rs** - NEW FILE (or merge elsewhere?):
- **Types (1):**
  - `StyleBoxShadowOffsets` - Offsets for box shadow rendering
- **Use Case:** Box shadow layout calculations

---

## 🟡 Priority 2: resources.rs - MIXED IMAGE/FONT/GPU/UPDATES

**Lines:** 2396 | **Types:** 73 | **Functions:** 5  
**Problem:** Mixes images + fonts + GPU resources + update messages + app config  
**Has Documentation:** 📝 NO (needs documentation)

### Current Responsibilities:

1. **App Configuration** - `AppConfig`, `AppLogLevel`, `DpiScaleFactor`
2. **Image Types** - `ImageKey`, `ImageDescriptor`, `RawImage`, `ImageCache`
3. **Font Types** - `FontKey`, `FontInstanceKey`, `GlyphOptions`, `FontRenderMode`
4. **Glyph Outlines** - `GlyphOutline`, `GlyphOutlineOperation`, outline operations
5. **GPU Resources** - `ExternalImageId`, `ExternalImageData`, `ImageBufferKind`
6. **Resource Cache** - `RendererResources`, `GlTextureCache`
7. **Update Messages** - `ResourceUpdate`, `AddImage`, `UpdateImage`, `AddFont`

### Split Plan:

```
core/src/
  app_config.rs      - App configuration (3 types, ~150 lines)
  image.rs           - Image types and cache (15 types, 2 functions, ~600 lines)
  font.rs            - Font types and options (18 types, 2 functions, ~500 lines)
  glyph_outline.rs   - Glyph outline rendering (10 types, ~300 lines)
  gpu_image.rs       - GPU image resources (5 types, ~200 lines)
  resource_cache.rs  - Resource caching (3 types, ~300 lines)
  resource_updates.rs - Update messages (12 types, 3 functions, ~400 lines)
```

### Migration Details:

**app_config.rs** - NEW FILE:
- **Types (3):**
  - `AppConfig`
  - `AppLogLevel`
  - `DpiScaleFactor`
- **Stub types:** `ExternalSystemCallbacks`, `FullWindowState`, `LayoutResult` (legacy stubs)

**image.rs** - NEW FILE:
- **Types (15):**
  - `ImageKey`, `ImageRef`, `ImageRefHash`
  - `ImageDescriptor`, `ImageDescriptorFlags`
  - `RawImageFormat`, `DecodedImage`
  - `RawImage`, `RawImageData`
  - `ImageCache`
  - `ImageType`, `ResolvedImage`
  - `ImageMask`
  - `TextExclusionArea`, `ExclusionSide`
- **Functions (2):**
  - `image_ref_get_hash()`
  - `build_add_image_resource_updates()`

**font.rs** - NEW FILE:
- **Types (18):**
  - `FontKey`, `FontInstanceKey`, `FontVariation`
  - `GlyphOptions`, `FontRenderMode`
  - `FontInstancePlatformOptions` (4 platform variants)
  - `FontHinting`, `FontLCDFilter`
  - `FontInstanceOptions`
  - `SyntheticItalics`
  - `ImmediateFontId`
  - `LoadedFontSource`
  - `Au` (font size unit)
- **Functions (2):**
  - `font_ref_get_hash()`
  - `font_size_to_au()`
  - `build_add_font_resource_updates()`

**glyph_outline.rs** - NEW FILE:
- **Types (10):**
  - `GlyphOutline`
  - `GlyphOutlineOperation`
  - `OutlineMoveTo`, `OutlineLineTo`, `OutlineQuadTo`, `OutlineCubicTo`
  - `OwnedGlyphBoundingBox`

**gpu_image.rs** - NEW FILE:
- **Types (5):**
  - `ExternalImageId`
  - `ExternalImageData`
  - `ExternalImageType`
  - `ImageBufferKind`
  - `ImageData`
  - `ImageDirtyRect`

**resource_cache.rs** - NEW FILE:
- **Types (3):**
  - `RendererResources`
  - `GlTextureCache`
  - `UpdateImageResult`
  - `PrimitiveFlags`
  - `IdNamespace`
  - `Epoch`

**resource_updates.rs** - NEW FILE:
- **Types (12):**
  - `ResourceUpdate`
  - `AddImage`, `UpdateImage`
  - `AddFont`, `AddFontInstance`
  - `AddFontMsg`, `DeleteFontMsg`
  - `AddImageMsg`, `DeleteImageMsg`
- **Functions (3):**
  - `add_fonts_and_images()`
  - `add_resources()`

---

## 🟡 Priority 2: window.rs - CORE + PLATFORM MIXED

**Lines:** 2240 | **Types:** 61 | **Functions:** 0  
**Problem:** Platform-specific types mixed with core window types  
**Has Documentation:** 📝 NO (needs documentation)

### Current Responsibilities:

1. **Core Window** - WindowId, WindowSize, WindowFlags, WindowPosition
2. **Platform Handles** - RawWindowHandle, IOSHandle, MacOSHandle, etc. (9 platform handles)
3. **Platform Options** - PlatformSpecificOptions, WindowsWindowOptions, LinuxWindowOptions, etc.
4. **Display** - Monitor, VideoMode, WindowTheme
5. **Input** - MouseCursorType, KeyboardState, MouseState, VirtualKeyCode
6. **Rendering** - RendererOptions, RendererType, Vsync, Srgb
7. **Geometry** - LogicalRect, LogicalPosition, PhysicalPosition, PhysicalSize

### Split Plan:

```
core/src/
  window.rs   - Core window types (35 types, ~1300 lines)
  platform.rs - Platform-specific handles and options (26 types, ~900 lines)
```

### Migration Details:

**window.rs** - KEEP core types (35 types):
- Core: `WindowId`, `IconKey`, `WindowSize`, `WindowFlags`, `WindowPosition`, `WindowFrame`, `FullScreenMode`
- Display: `Monitor`, `VideoMode`, `WindowTheme`
- Input: `MouseCursorType`, `KeyboardState`, `MouseState`, `VirtualKeyCode`, `VirtualKeyCodeCombo`, `AcceleratorKey`
- Icons: `WindowIcon`, `TaskBarIcon`, `SmallWindowIconBytes`, `LargeWindowIconBytes`
- Rendering: `RendererOptions`, `RendererType`, `Vsync`, `Srgb`, `HwAcceleration`, `ProcessEventResult`
- Geometry: `LogicalRect`, `LogicalPosition`, `LogicalSize`, `PhysicalPosition`, `PhysicalSize`
- State: `ScrollStates`, `ScrollState`, `FullHitTest`, `CursorTypeHitTest`, `TouchState`, `DebugState`
- Misc: `ImePosition`, `WritingMode`, `UpdateFocusWarning`, `ContextMenuMouseButton`, `ScrollResult`, `CursorPosition`

**platform.rs** - MOVE platform-specific (26 types):
- Handles: `RawWindowHandle`, `IOSHandle`, `MacOSHandle`, `XlibHandle`, `XcbHandle`, `WaylandHandle`, `WindowsHandle`, `WebHandle`, `AndroidHandle`
- Options: `PlatformSpecificOptions`, `WindowsWindowOptions`, `LinuxWindowOptions`, `MacWindowOptions`, `WasmWindowOptions`
- Platform-Specific: `XWindowType`, `UserAttentionType`, `WaylandTheme`, `AzStringPair`

---

## 🟡 Priority 3: callbacks.rs - MIXED CONCERNS

**Lines:** 1243 | **Types:** 36 | **Functions:** 0  
**Problem:** Mixes reference counting + callbacks + animations + IDs + hit testing + focus  
**Has Documentation:** 📝 NO (needs documentation)

### Current Responsibilities:

1. **Reference Counting** - `RefAny`, `RefCount`, `Ref`, `RefMut` (8 types)
2. **Core Callbacks** - `CoreCallback`, `CoreCallbackData`, `CoreImageCallback`
3. **Layout Callbacks** - `LayoutCallback`, `LayoutCallbackInfo`, `MarshaledLayoutCallback`
4. **IFrame Callbacks** - `IFrameCallback`, `IFrameCallbackInfo`, `IFrameCallbackReturn`
5. **Timer** - `TimerCallbackReturn`
6. **Updates/Animations** - `Update`, `AnimationData`, `Animation`, `AnimationRepeat`
7. **Hit Testing** - `HitTestItem`, `ScrollHitTestItem`, `ScrollPosition`
8. **Focus** - `FocusTarget`, `FocusTargetPath`
9. **IDs** - `DocumentId`, `PipelineId`, `DomNodeId`

**Decision:** Keep as-is for now. While it has many different types, they're all related to the callback system and application lifecycle. Splitting might make it harder to understand the callback flow.

---

## 🟢 Priority 4: gl.rs - LONG BUT COHESIVE

**Lines:** 3711 | **Types:** 46 | **Functions:** 6  
**Problem:** **NONE** - All OpenGL-related types  
**Has Documentation:** 📝 NO (needs documentation)

**Assessment:** While gl.rs is the largest file (3711 lines), it has a **single responsibility**: OpenGL FFI and abstractions. All 46 types are GL-related (textures, shaders, buffers, context). The file is long because OpenGL has many types, not because responsibilities are mixed.

**Recommendation:** Add module-level documentation, but **DO NOT SPLIT**. Splitting by "texture vs shader vs context" would fragment the GL API unnecessarily.

---

## 📝 Priority 5: dom.rs - NEEDS CLARIFICATION

**Lines:** 2174 | **Types:** 24 | **Functions:** 1  
**Problem:** Mixes DOM + Events?  
**Has Documentation:** 📝 NO (needs documentation)

**Types:**
- Core DOM: `Dom`, `CompactDom`, `NodeType`, `NodeData` (5 types)
- Events: `On`, `EventFilter`, `HoverEventFilter`, `FocusEventFilter`, etc. (8 types)
- IDs: `TagId`, `ScrollTagId`, `DomNodeHash` (3 types)
- Accessibility: `AccessibilityInfo`, `AccessibilityRole`, `AccessibilityState`, `TabIndex` (4 types)
- Other: `IFrameNode`, `IdOrClass`, `NodeDataInlineCssProperty` (3 types)

**Question:** Should events be in a separate `events.rs` module, or are they core to DOM definition?

**Recommendation:** 
- **Option A:** Keep as-is (events are part of DOM node definition)
- **Option B:** Extract to `events.rs` if events are used independently

---

## 📝 Priority 6: styled_dom.rs - TIGHTLY COUPLED WITH dom.rs

**Lines:** 1774 | **Types:** 14 | **Functions:** 0  
**Problem:** None - clear boundary between unstyled and styled DOM  
**Has Documentation:** 📝 NO (needs documentation)

**Recommendation:** Add module-level documentation explaining relationship to dom.rs. Keep separate.

---

## 📝 Priority 7: task.rs - MISSING DOCS

**Lines:** 777 | **Types:** 19 | **Functions:** 0  
**Problem:** None - all threading/timer related  
**Has Documentation:** 📝 NO

**Recommendation:** Add brief module-level documentation.

---

## 📝 Priority 8: glyph.rs - MISSING DOCS

**Lines:** 189 | **Types:** 10 | **Functions:** 0  
**Problem:** None - all glyph placement related  
**Has Documentation:** 📝 NO

**Recommendation:** Add brief module-level documentation.

---

## Summary of Required Work

### Phase 1: Critical Mixed Responsibilities (DO FIRST)
1. ✅ Split **xml.rs** → xml.rs + component.rs + codegen.rs (2-3h)
2. ✅ Split **ui_solver.rs** → 7 files (scroll.rs, layout_context.rs, etc.) (2-3h)

### Phase 2: Large Mixed Files
3. ✅ Split **resources.rs** → 7 files (app_config.rs, image.rs, font.rs, etc.) (3-4h)
4. ✅ Split **window.rs** → window.rs + platform.rs (1-2h)

### Phase 3: Add Documentation
5. 📝 Add module docs to **gl.rs** (15min)
6. 📝 Add module docs to **dom.rs** (15min)
7. 📝 Add module docs to **styled_dom.rs** (15min)
8. 📝 Add module docs to **callbacks.rs** (15min)
9. 📝 Add module docs to **task.rs** (10min)
10. 📝 Add module docs to **glyph.rs** (10min)

### Total Effort Estimate
- **Phase 1:** 4-6 hours (critical)
- **Phase 2:** 4-6 hours (recommended)
- **Phase 3:** 1.5 hours (documentation)
- **Total:** 9.5-13.5 hours

---

## Breaking Changes

**Phase 1 & 2:** YES - Import paths will change

Users must update imports:
```rust
// xml split:
// Before: use azul_core::xml::{XmlComponent, str_to_rust_code};
// After:
use azul_core::component::XmlComponent;
use azul_core::codegen::str_to_rust_code;

// ui_solver split:
// Before: use azul_core::ui_solver::{FormattingContext, OverflowInfo};
// After:
use azul_core::layout_context::FormattingContext;
use azul_core::overflow::OverflowInfo;

// resources split:
// Before: use azul_core::resources::{ImageKey, FontKey};
// After:
use azul_core::image::ImageKey;
use azul_core::font::FontKey;

// window split:
// Before: use azul_core::window::RawWindowHandle;
// After:
use azul_core::platform::RawWindowHandle;
```

**Phase 3:** NO - Only adds documentation

---

## Next Steps

1. **Review this plan** - Confirm approach
2. **Start with Phase 1** - xml.rs and ui_solver.rs splits (highest impact)
3. **Test and commit** after each split
4. **Then Phase 2** - resources.rs and window.rs
5. **Add documentation** - Phase 3
6. **Continue with** REFACTORING/portedfromcore.rs integration

**Important:** Do refactoring BEFORE portedfromcore.rs integration to avoid moving code twice.

---

## Success Criteria

- ✅ Each module has single, clear responsibility
- ✅ Related types grouped together
- ✅ Clear separation of concerns
- ✅ All modules have documentation
- ✅ Types easily discoverable by category
- ✅ All code compiles successfully
- ✅ No functionality broken
