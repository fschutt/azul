# Documentation Reference Backlog

Generated from `doc/target/autoreview/reports/` — one section per source file.
This is a working document: it captures the *current* documentation gaps
identified by autoreview. It will be consumed by `azul-doc autoreview
autodoc`, which will replace these unstructured notes with grouped guides.

---

## core/src/a11y.rs

- System identified: yes — Accessibility system
- Existing doc: none (no `doc/guide/accessibility.md` exists)
- Doc needed: An `accessibility.md` guide covering the accessibility type hierarchy (`AccessibilityInfo`, `SmallAriaInfo`, roles, states, actions), how they flow from `core` types through `layout/src/managers/a11y.rs` to platform backends (`dll/src/desktop/shell2/*/accessibility.rs`), and the current state of platform support.

## core/src/animation.rs

- System identified: yes — Animation system
- Existing doc: none (no animation guide in `doc/guide/`)
- Doc needed: An animation system guide would be warranted once the animation types are actually wired into the runtime. Currently the system is essentially stub definitions with no runtime integration, so documentation would be premature.

## core/src/callbacks.rs

- System identified: yes — **Callback / Event System**
- Existing doc: `doc/guide/lifecycle.md` covers the app lifecycle and callback usage from a user perspective, but does not detail the callback infrastructure internals.
- Doc needed: A dedicated `doc/guide/callbacks.md` or section in `architecture.md` explaining:
  - The Core vs Layout callback split (usize function pointer pattern to avoid circular deps)
  - The FFI callable pattern (`ctx: OptionRefAny` + extern "C" trampoline)
  - The `LayoutCallbackInfo` / `VirtualViewCallbackInfo` / `CallbackInfo` hierarchy
  - How callbacks flow through the event loop
  - The VirtualView callback re-invocation lifecycle

## core/src/compact_cache_builder.rs

- System identified: yes — CSS cascade / compact layout cache (part of the styling and layout pipeline)
- Existing doc: `doc/guide/styling-system.md` exists
- Doc needed: The compact cache encoding scheme (tier1/tier2/tier2b/tier2_cold structure, sentinel values, encoding conventions) is complex and undocumented in any guide. Multiple planning docs exist in `scripts/` (`COMPACT_CACHE_PLAN.md`, `COMPACT_CACHE_STATUS.md`, `COMPACT_CACHE_STATUS_V2.md`) but no user-facing guide. A section in `styling-system.md` or a dedicated `compact-cache.md` explaining the tiered binary encoding, how properties flow from CSS cascade into compact arrays, and the sentinel value system would be valuable.

## core/src/debug.rs

- System identified: yes — Debug / Diagnostics system (overlaps with E2E test infrastructure in debug_server.rs)
- Existing doc: none (no debug system guide in `doc/guide/`)
- Doc needed: A guide explaining the debug infrastructure — how `AZUL_DEBUG` env var works, the HTTP debug server in `debug_server.rs`, the logging macros from `core/debug.rs`, and how E2E testing integrates. This would help clarify the relationship between the two parallel logging systems and could drive their consolidation.

## core/src/diff.rs

- System identified: yes — DOM Reconciliation / Incremental Update system
- Existing doc: `scripts/DOM_CHANGE_REPORT_ARCHITECTURE.md`, `scripts/INCREMENTAL_LAYOUT_ARCHITECTURE.md` (design docs in scripts/, not user-facing guide)
- Doc needed: A `doc/guide/reconciliation.md` covering the DOM diff/reconciliation pipeline, how `reconcile_dom` → `ChangeAccumulator` → layout invalidation works, and how state migration (`transfer_states`) fits in. Currently no user-facing guide exists for this system.

## core/src/dom.rs

- System identified: **DOM / Document Model** system
- Existing doc: None specific. `doc/guide/lifecycle.md` covers the rendering lifecycle but not DOM construction or the node type model.
- Doc needed: A `doc/guide/dom.md` guide covering: the `Dom` / `NodeData` / `NodeType` model, tree vs arena (FastDom) construction, the builder pattern API, how DOM nodes map to HTML elements, CSS property attachment, and how the DOM feeds into the styling/layout pipeline.

## core/src/drag.rs

- System identified: yes — Drag & Drop / Input Handling system
- Existing doc: none (no `doc/guide/` file for drag-and-drop or input handling)
- Doc needed: A `doc/guide/drag-and-drop.md` guide explaining the unified drag system, how `DragContext` flows through the event loop, the relationship between `core/src/drag.rs` (types), `layout/src/managers/gesture.rs` (gesture detection), and `layout/src/managers/drag_drop.rs` (drag-drop handling).

## core/src/events.rs

- System identified: **Event System** (event types, propagation, filtering, input interpretation)
- Existing doc: `doc/guide/lifecycle.md` covers lifecycle but not event propagation/filtering
- Doc needed: A `doc/guide/events.md` documenting the event system architecture:
  event types, filter categories (Hover/Focus/Window/Not/Component/Application),
  capture/target/bubble propagation, input interpreter pipeline, and SystemChange flow.

## core/src/geom.rs

- System identified: yes — Geometry/Coordinate System (used by layout engine, rendering pipeline, windowing, hit testing)
- Existing doc: none (no `doc/guide/geometry.md` or `doc/guide/coordinates.md`)
- Doc needed: A guide document covering the coordinate space model (logical vs physical, window vs scroll-frame vs parent), DPI scaling, and how geometry types flow through layout → display list → rendering. The `CoordinateSpace` enum comments (lines 538-575) contain good material that could seed this document.

## core/src/gl_fxaa.rs

- System identified: **Rendering pipeline** (OpenGL post-processing / anti-aliasing subsystem)
- Existing doc: none (no rendering pipeline guide in `doc/guide/`)
- Doc needed: A rendering pipeline guide covering the GL context setup (`core/src/gl.rs`), shader compilation, SVG rendering, and post-processing (FXAA). This file is part of a small subsystem: `gl_fxaa.rs` defines config + shaders, `gl.rs` compiles them at startup, and `layout/src/xml/svg.rs` executes the FXAA pass.

## core/src/gl.rs

- System identified: yes — OpenGL / rendering pipeline (GL context management, texture cache, shader compilation, vertex buffer management)
- Existing doc: none (no `doc/guide/rendering.md` or `doc/guide/opengl.md` exists)
- Doc needed: A guide document for the OpenGL/rendering subsystem covering the texture cache lifecycle, shader compilation, and how `gl.rs` integrates with WebRender and the SVG rendering pipeline.

## core/src/glconst.rs

- System identified: yes — OpenGL rendering pipeline
- Existing doc: none (no `doc/guide/` file covers OpenGL/rendering)
- Doc needed: A `doc/guide/rendering.md` covering the GL abstraction layer
  (`core/src/gl.rs`, `core/src/glconst.rs`, `gl_context_loader`, and
  how webrender integrates) would be valuable.

## core/src/glyph.rs

- System identified: yes — **Text Shaping / Glyph System**
- Existing doc: none (no text-shaping guide in `doc/guide/`)
- Doc needed: A `doc/guide/text-shaping.md` explaining the text shaping pipeline: font loading, glyph lookup, shaping (GSUB/GPOS), advance calculation, and how glyphs flow into layout and rendering. This would clarify the roles of `core/src/glyph.rs`, `layout/src/text3/`, and related modules.

## core/src/gpu.rs

- System identified: yes — GPU rendering / WebRender integration pipeline
- Existing doc: none (no `doc/guide/` file for rendering or GPU)
- Doc needed: A `doc/guide/rendering.md` covering the GPU value cache, WebRender key management, scrollbar opacity fading, and how `GpuStateManager` + `GpuValueCache` + display list generation fit together.

## core/src/hit_test_tag.rs

- System identified: yes — hit-testing / event dispatch system
- Existing doc: none (no `doc/guide/hit-testing.md` or similar)
- Doc needed: A guide covering the hit-test pipeline: how WebRender tags are pushed during display list building, how hit-test results are processed (`wr_translate2.rs`), how they feed into event dispatch, scrollbar handling, cursor resolution, and text selection. The tag namespace system (0x0100–0x0600) should be documented as it spans multiple files.

## core/src/hit_test.rs

- System identified: yes — Hit Testing / Event Dispatch system
- Existing doc: none (no hit-testing guide in `doc/guide/`)
- Doc needed: A guide explaining the hit-testing pipeline — how mouse/touch coordinates are mapped to DOM nodes, how scroll hit testing works, how cursor type is determined, and how `FullHitTest` feeds into the event system. Related files: `core/src/hit_test.rs`, `layout/src/hit_test.rs`, `dll/src/desktop/wr_translate2.rs`, `layout/src/managers/hover.rs`, `layout/src/managers/scroll_state.rs`.

## core/src/icon.rs

- System identified: yes — Icon System (icon resolution pipeline, spans `core/src/icon.rs`, `layout/src/icon.rs`, shell integration in `dll/src/desktop/shell2/`)
- Existing doc: none (no `doc/guide/icons.md` or similar)
- Doc needed: A `doc/guide/icons.md` covering the icon provider architecture, registration flow (packs, resolvers), resolution pipeline (`NodeType::Icon` -> `resolve_icons_in_styled_dom` -> `StyledDom` replacement), and how to create custom icon resolvers from C/Rust. The `scripts/ICON_SYSTEM_ANALYSIS.md` contains design analysis that could serve as a starting point.

## core/src/id.rs

- System identified: yes — DOM tree / node hierarchy system
- Existing doc: none (no `doc/guide/` document covers the node hierarchy or DOM tree internals)
- Doc needed: A guide document covering the DOM tree data structures (`NodeId`, `Node`, `NodeHierarchy`, `NodeDataContainer`), the FFI encoding scheme for `Option<NodeId>`, and how the hierarchy relates to `StyledDom` and layout. This is a core data structure that many other systems depend on.

## core/src/json.rs

- System identified: yes — JSON / data serialization system for C FFI
- Existing doc: none (no guide doc covers the JSON/serialization subsystem)
- Doc needed: A guide document explaining the JSON data model, how it bridges Rust/C/Python, and the design trade-off of string-serialized compound values for FFI safety. Could be part of a broader "FFI data types" guide.

## core/src/lib.rs

- System identified: yes — this is the **core types / shared data definitions**
  crate, foundational to the entire Azul toolkit.
- Existing doc: `doc/guide/architecture.md` covers the high-level architecture.
- Doc needed: n/a (architecture guide already exists; the crate root doc comment
  should be expanded per the MEDIUM finding above, but no new guide document is
  required).

## core/src/macros.rs

- System identified: yes — macro/utility infrastructure (cross-cutting, used by callback and type systems)
- Existing doc: none (no specific guide for internal macro infrastructure)
- Doc needed: n/a — macros are an implementation detail, not a user-facing system. A module-level doc comment suffices.

## core/src/menu.rs

- System identified: yes — Menu / Context Menu system (windowing subsystem)
- Existing doc: none (no `doc/guide/menus.md` or similar)
- Doc needed: A guide document covering the menu system — how menus are constructed (`Menu`, `MenuItem`, `StringMenuItem`), the core/layout callback split (`CoreMenuCallback` vs real function pointers), popup positioning (`MenuPopupPosition`), and platform-specific rendering (Windows native menus in `dll/src/desktop/shell2/windows/menu.rs`, macOS in `dll/src/desktop/shell2/macos/menu.rs`, Linux GNOME in `dll/src/desktop/shell2/linux/gnome_menu/`, custom renderer in `dll/src/desktop/menu_renderer.rs`).

## core/src/prop_cache.rs

- System identified: yes — CSS Styling / Cascade Resolution system
- Existing doc: `doc/guide/styling-system.md`, `doc/guide/css-styling.md`
- Doc needed: n/a (existing guides cover the styling system)

## core/src/selection.rs

- System identified: yes — Text Selection / Text Editing system
- Existing doc: none (no guide for text selection or text editing)
- Doc needed: A `doc/guide/text-editing.md` covering the text selection model (ContentIndex, GraphemeClusterId, cursor positioning, multi-node selection via anchor/focus), the relationship between `SelectionState`/`MultiCursorState`/`TextSelection`, and how selection integrates with the rendering pipeline and input handling. The `scripts/report-selection.md` file contains useful architectural notes about IFC roots and hit-testing that could inform this guide.

## core/src/style.rs

- System identified: **CSS styling / cascade system** — this file implements CSS selector matching, a core part of the styling pipeline.
- Existing doc: `doc/guide/styling-system.md`, `doc/guide/css-styling.md`, `doc/guide/css-properties.md`
- Doc needed: n/a (existing guides cover the styling system)

## core/src/svg_path_parser.rs

- System identified: SVG rendering / geometry pipeline
- Existing doc: none (no SVG or rendering guide in `doc/guide/`)
- Doc needed: A `doc/guide/svg-rendering.md` covering SVG parsing, geometry types (`SvgMultiPolygon`, `SvgPath`, `SvgPathElement`), the path parser, and how SVG elements are rendered via `cpurender.rs`. Multiple SVG-related source files would benefit from this.

## core/src/task.rs

- System identified: yes — Timer/Thread async task system
- Existing doc: `doc/guide/lifecycle.md` covers the event loop; no dedicated threading/timer guide exists.
- Doc needed: A `doc/guide/async-tasks.md` covering the timer system (TimerId, reserved IDs, timer callbacks), background thread system (ThreadId, ThreadReceiver, ThreadSendMsg), and how they integrate with the event loop (timer ticking, thread completion checking).

## core/src/transform.rs

- System identified: yes — CSS Transform / Rendering Pipeline
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md` cover CSS but not transforms specifically
- Doc needed: A section on CSS transforms (how `StyleTransform` -> `ComputedTransform3D` -> WebRender matrix pipeline works, SIMD acceleration, coordinate systems, rotation modes) would be valuable as part of a rendering pipeline guide.

## core/src/ua_css.rs

- System identified: yes — **CSS Styling System** (user-agent stylesheet / default CSS cascade)
- Existing doc: `doc/guide/styling-system.md`, `doc/guide/css-styling.md`, `doc/guide/css-properties.md`
- Doc needed: n/a — existing guides cover the styling system

## core/src/ui_solver.rs

- System identified: yes — layout solver / CSS resolution system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md` (related but not specific to layout solving)
- Doc needed: A `doc/guide/layout-solver.md` covering the layout pipeline (`solver3/`, `ui_solver.rs`, `prop_cache.rs`, CSS property resolution, box model computation). Many files contribute to this system.

## core/src/window.rs

- System identified: **Windowing system** — defines core window state types, input state (keyboard, mouse, touch), platform-specific options, and window configuration used across all platform backends (`dll/src/desktop/shell2/{windows,macos,linux}/`).
- Existing doc: none (no `doc/guide/windowing.md` or similar)
- Doc needed: A windowing system guide explaining the relationship between `core/src/window.rs` (type definitions), `layout/src/window_state.rs` (state management), and the platform backends in `dll/src/desktop/shell2/`. Should cover window lifecycle, input state flow, and platform option mapping.

## css/src/compact_cache.rs

- System identified: yes — CSS property caching / layout solver optimization
- Existing doc: `doc/guide/styling-system.md` (partial — covers CSS styling but not the compact cache specifically)
- Doc needed: A guide section on the compact property cache architecture (three-tier encoding, how it integrates with the layout solver, when the slow cascade path is used). This would benefit users working on layout performance.

## css/src/lib.rs

- System identified: yes — CSS styling / theming system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`
- Doc needed: n/a (already covered by existing guides)

## css/src/props/basic/angle.rs

- System identified: CSS styling system (angle values for transforms, gradients, filters)
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/animation.rs

- System identified: yes — SVG geometry / CSS animation interpolation system
- Existing doc: `doc/guide/css-styling.md` and `doc/guide/css-properties.md` cover CSS but not animation/SVG geometry specifically
- Doc needed: A guide covering the animation interpolation pipeline (how `AnimationInterpolationFunction` drives CSS transitions) and how SVG geometry primitives flow from `css/` definitions through `core/src/svg.rs` and `layout/src/xml/svg.rs`. Could be `doc/guide/svg-animation.md`.

## css/src/props/basic/color.rs

- System identified: yes — CSS styling/parsing system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/direction.rs

- System identified: CSS styling / gradient direction parsing
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (existing guides cover this system area)

## css/src/props/basic/error.rs

- System identified: CSS parsing / FFI error types (part of the CSS styling system)
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/font.rs

- System identified: yes — CSS styling system / font subsystem
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/geometry.rs

- System identified: CSS / layout geometry primitives (part of the layout solver system)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md` (related but not specific to geometry primitives)
- Doc needed: n/a (these are basic data types, not a standalone system requiring a guide)

## css/src/props/basic/image.rs

- System identified: CSS parsing system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/length.rs

- System identified: CSS property type system (basic numeric types)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/mod.rs

- System identified: yes — CSS property system (basic/primitive CSS types)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/parse.rs

- System identified: CSS parsing / styling system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/pixel.rs

- System identified: yes — CSS value resolution / styling system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/basic/time.rs

- System identified: CSS property parsing system (`css/src/props/basic/`)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/formatter.rs

- System identified: CSS styling system (CSS property formatting/serialization)
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/layout/column.rs

- System identified: CSS property system (specifically multi-column layout properties)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/layout/dimensions.rs

- System identified: CSS property system / layout dimensions
- Existing doc: `doc/guide/css-styling.md` (covers box model and styling, but not internal dimension type system or calc() AST)
- Doc needed: n/a (covered by existing css-styling.md guide, though it could be expanded with calc() expression details)

## css/src/props/layout/display.rs

- System identified: CSS property definitions (part of the CSS/styling system)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (existing guides cover the CSS property system)

## css/src/props/layout/flex.rs

- System identified: yes — CSS property parsing / layout styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/layout/flow.rs

- System identified: CSS property parsing / styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (existing guides cover the CSS property system)

## css/src/props/layout/fragmentation.rs

- System identified: CSS property type definitions (fragmentation subset of the CSS styling system)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides; the runtime fragmentation engine in `layout/src/fragmentation.rs` may warrant its own layout/pagination guide, but that's outside this file's scope)

## css/src/props/layout/grid.rs

- System identified: yes — CSS property types / layout system (specifically CSS Grid)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a — covered by existing guides. However, the `Fr` scaling convention (100x multiplier) should be documented in-code.

## css/src/props/layout/mod.rs

- System identified: yes — CSS layout property definitions (part of the CSS / styling system)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a

## css/src/props/layout/overflow.rs

- System identified: yes — CSS property parsing / styling system (overflow subset)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (existing guides cover this system)

## css/src/props/layout/position.rs

- System identified: CSS property / layout system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md` (partial coverage)
- Doc needed: A dedicated layout system guide covering how CSS position/offset/z-index properties flow from parsing through the layout solver (`layout/src/solver3/positioning.rs`) would be valuable. No `doc/guide/layout.md` exists.

## css/src/props/layout/shape.rs

- System identified: CSS styling / property system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (existing guides cover the CSS property system)

## css/src/props/layout/spacing.rs

- System identified: CSS property definitions / styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/layout/table.rs

- System identified: CSS property system / styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (existing docs cover the CSS property system)

## css/src/props/layout/text.rs

- System identified: yes — CSS property parsing / styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (system is already documented)

## css/src/props/layout/wrapping.rs

- System identified: CSS property parsing / styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (already covered by existing guides)

## css/src/props/macros.rs

- System identified: CSS property/styling system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (already documented)

## css/src/props/mod.rs

- System identified: CSS property system (property definitions, parsing, formatting)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a — existing guides cover the CSS property system

## css/src/props/property.rs

- System identified: CSS property system (parsing, typing, categorization, interpolation, formatting)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/style/azul_exclusion.rs

- System identified: yes — CSS styling system (custom Azul CSS properties for exclusion/hyphenation)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/style/border_radius.rs

- System identified: CSS styling system (property parsing and representation)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (already covered by existing guides)

## css/src/props/style/box_shadow.rs

- System identified: CSS styling / property parsing system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/props/style/content.rs

- System identified: CSS property / styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (existing docs cover this system)

## css/src/props/style/effects.rs

- System identified: yes — CSS property definitions / styling system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a

## css/src/props/style/filter.rs

- System identified: CSS styling / filter effects system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: The existing CSS docs may cover filters, but a section specifically documenting which CSS filter functions are supported and their rendering backend coverage (WebRender vs CPU) would be valuable. No new guide file needed — extend existing `css-properties.md`.

## css/src/props/style/lists.rs

- System identified: CSS styling/property system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a

## css/src/props/style/mod.rs

- System identified: yes — CSS styling system
- Existing doc: `doc/guide/styling-system.md`
- Doc needed: n/a

## css/src/props/style/scrollbar.rs

- System identified: yes — CSS Styling System (scrollbar subsystem)
- Existing doc: `doc/guide/styling-system.md`, `doc/guide/css-properties.md`
- Doc needed: The existing styling-system doc likely covers general CSS; a dedicated section or guide on **scrollbar styling and scroll physics** would be valuable given the complexity (standard vs webkit vs Azul-specific properties, platform presets, resolution logic). Consider adding to `css-properties.md` if not already covered there.

## css/src/props/style/selection.rs

- System identified: yes — CSS styling system (text selection subsystem)
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing styling guides; the `scripts/report-selection.md` architecture document already provides detailed design context for the selection subsystem)

## css/src/props/style/text.rs

- System identified: yes - CSS styling system (text properties subsystem)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing styling system docs)

## css/src/props/style/transform.rs

- System identified: yes — CSS styling / transform property system
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/shape_parser.rs

- System identified: CSS parsing / styling system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)

## css/src/shape.rs

- System identified: yes — CSS shape / text-shaping / clip-path system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md` (partially covers CSS properties)
- Doc needed: A guide document covering the shape system (shape-inside, shape-outside, clip-path) — how shapes flow from CSS parsing through `css/src/shape.rs` and `css/src/shape_parser.rs` into layout (`layout/src/text3/cache.rs` ShapeBoundary) and display list generation (`layout/src/solver3/display_list.rs` clip-path). This would help clarify the relationship between the duplicate types and logic.

## css/src/system_native_linux.rs

- System identified: yes — System Style / Theme Discovery (part of the `css` crate's system styling subsystem)
- Existing doc: `doc/guide/styling-system.md` exists and likely covers this area
- Doc needed: n/a (covered by existing guide)

## css/src/system_native_macos.rs

- System identified: yes — System Style / Theme Discovery system
- Existing doc: none (no `doc/guide/system-style.md` or similar)
- Doc needed: A guide covering the system style discovery pipeline — how
  `SystemStyle::detect()` dispatches to native (dlopen) vs CLI vs defaults,
  what each platform queries, and how the result feeds into CSS resolution
  and dynamic selectors (`prefers-color-scheme`, `prefers-reduced-motion`, etc.).

## css/src/system_native_windows.rs

- System identified: yes — System Style / Platform Discovery (part of the CSS/styling system)
- Existing doc: `doc/guide/styling-system.md` covers SystemStyle at a high level (lines 57-92)
- Doc needed: n/a — existing coverage is adequate; the planning docs in `scripts/SYSTEMSTYLE_INTEGRATION_PLAN.md` and `scripts/SYSTEMSTYLE.md` provide additional design context

## css/src/system.rs

- System identified: yes — **System styling / theme detection system**
- Existing doc: `doc/guide/styling-system.md` covers CSS styling but not system-native style detection
- Doc needed: A guide document covering the system style detection pipeline would be valuable. It should explain the priority order (native FFI > CLI discovery > compile-time defaults), the `io`/`system` feature flags, the app-specific ricing mechanism, and how `SystemStyle` feeds into the CSS cascade. This file is central to the system — used by 20+ files across layout, dll, and core crates.

## dll/src/desktop/app.rs

- System identified: yes — Application lifecycle / event loop entry point
- Existing doc: `doc/guide/lifecycle.md` exists
- Doc needed: n/a (lifecycle.md covers this area)

## dll/src/desktop/clipboard_error.rs

- System identified: yes — clipboard / desktop integration system
- Existing doc: none (no clipboard guide in `doc/guide/`)
- Doc needed: A clipboard system guide covering the platform-specific clipboard implementations (X11, Wayland, macOS, Windows), how they integrate with `ClipboardManager`, and the error handling strategy would be useful given the current fragmentation.

## dll/src/desktop/compositor2.rs

- System identified: yes — Rendering Pipeline / WebRender compositor integration
- Existing doc: `scripts/SCROLL_COORDINATE_ARCHITECTURE.md` (partial, scroll-specific)
- Doc needed: A `doc/guide/rendering-pipeline.md` explaining the full rendering pipeline from `DisplayList` generation through `compositor2.rs` translation to WebRender display list submission. No existing guide covers this system.

## dll/src/desktop/csd.rs

- System identified: yes — Windowing / Client-Side Decorations system
- Existing doc: none (no `doc/guide/windowing.md` or `doc/guide/csd.md` exists)
- Doc needed: A guide document covering the windowing system, including CSD injection, decoration modes (`WindowDecorations` enum), and how the titlebar/menu bar are integrated into the DOM would be valuable.

## dll/src/desktop/css.rs

- System identified: CSS styling system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (already covered by existing guides)

## dll/src/desktop/dialogs.rs

- System identified: yes — native OS dialog / windowing system
- Existing doc: none (no `doc/guide/dialogs.md` or similar)
- Doc needed: A brief guide covering the dialog subsystem (message boxes, file dialogs, color picker) and how it integrates with the windowing layer would be useful, though this is a small enough module that inline docs may suffice.

## dll/src/desktop/display.rs

- System identified: yes — **Windowing / Display Management** system
- Existing doc: none (no `windowing.md` or `display.md` in `doc/guide/`)
- Doc needed: A `windowing.md` guide covering the display management, window creation, DPI handling, and platform abstraction layer (`shell2/` + `display.rs`). This would document how monitor enumeration works cross-platform, how DPI scaling is handled, and the relationship between `DisplayInfo`, `Monitor`, and the platform shell modules.

## dll/src/desktop/gl_texture_cache.rs

- System identified: yes — rendering pipeline / OpenGL texture management
- Existing doc: `doc/guide/lifecycle.md`, `doc/guide/architecture.md` (general)
- Doc needed: A rendering pipeline guide covering WebRender integration, texture management, display list building, and the external image API would be useful. Multiple files in `dll/src/desktop/` (gl_texture_cache, gl_texture_integration, wr_translate2) form this system.

## dll/src/desktop/gl_texture_integration.rs

- System identified: yes — OpenGL texture management / rendering pipeline
- Existing doc: none (no `doc/guide/rendering.md` or `doc/guide/gl-textures.md`)
- Doc needed: A guide covering the GL texture lifecycle — how textures are created, cached (`gl_texture_cache`), integrated (`gl_texture_integration`), and consumed by WebRender (`wr_translate2`). Should clarify the two parallel storage systems (`ACTIVE_GL_TEXTURES` in `core/src/gl.rs` vs `TEXTURE_CACHE` in `gl_texture_cache.rs`).

## dll/src/desktop/logging.rs

- System identified: yes — logging / crash reporting subsystem (part of the desktop application lifecycle)
- Existing doc: `doc/guide/lifecycle.md` covers application lifecycle but no dedicated logging/crash-reporting guide exists
- Doc needed: n/a — the file is small and self-contained; a section in the lifecycle guide would suffice if expanded

## dll/src/desktop/menu_renderer.rs

- System identified: yes — Menu / Context Menu rendering system (part of the desktop windowing subsystem)
- Existing doc: none (no `doc/guide/menus.md` or similar)
- Doc needed: A guide document covering the menu system — how menus are created (`Menu` struct), rendered (`menu_renderer.rs`), positioned and shown (`menu.rs`), and how submenus work. Related files: `dll/src/desktop/menu.rs`, `dll/src/desktop/menu_renderer.rs`, `core/src/menu.rs`, platform-specific `menu.rs` files.

## dll/src/desktop/menu.rs

- System identified: yes — **Menu / Popup Window system** (spanning `dll/src/desktop/menu.rs`, `menu_renderer.rs`, `csd.rs`, and platform-specific `shell2/*/menu.rs`)
- Existing doc: none (no `doc/guide/menu*.md` or similar)
- Doc needed: A guide document covering the unified menu system architecture: how menus are implemented as Azul windows, the positioning algorithm, how platform shells integrate with `show_menu`, and the relationship between `menu.rs`, `menu_renderer.rs`, and platform-specific menu modules.

## dll/src/desktop/mod.rs

- System identified: **Desktop Windowing System** — this is the top-level module for the desktop windowing backend, coordinating shell2 (platform backends), compositor, CSD, menus, and resource management.
- Existing doc: `doc/guide/lifecycle.md` covers the event lifecycle; `doc/guide/architecture.md` covers overall architecture.
- Doc needed: A dedicated **Windowing / Desktop Backend** guide would be valuable, covering: platform abstraction (shell2), compositor integration (compositor2/wr_translate2), CSD, menu system, and how `app.rs` ties them together. Currently this information is spread across module docs and must be pieced together.

## dll/src/desktop/native_screenshot.rs

- System identified: yes — Desktop/Windowing system (screenshot subsystem)
- Existing doc: none (no `doc/guide/` document covers screenshots or the debug server)
- Doc needed: A guide document covering the debug server and its screenshot capabilities would be useful, since `debug_server.rs` is the primary consumer and the screenshot scripts in `scripts/` describe a 10-step workflow for automated screenshot capture.

## dll/src/desktop/shader_cache.rs

- System identified: yes — rendering pipeline (WebRender integration / shader caching)
- Existing doc: none (no guide covers the rendering pipeline or WebRender integration)
- Doc needed: A `doc/guide/rendering-pipeline.md` covering WebRender integration, shader compilation, the shader disk cache, and how the rendering pipeline is initialized.

## dll/src/desktop/shell2/common/compositor.rs

- System identified: **Rendering / Compositor Pipeline** — this file defines the backend selection, GPU blacklist, and compositor trait that the rendering pipeline depends on.
- Existing doc: `doc/guide/envs.md` documents the `AZ_BACKEND` env var. No dedicated compositor/rendering pipeline guide exists.
- Doc needed: A `doc/guide/rendering.md` covering the compositor pipeline: how `AzBackend` is resolved, how `CompositorMode` maps to concrete implementations (`CpuCompositor`, future GPU compositor), the GPU blacklist system, and how `run.rs` orchestrates backend selection.

## dll/src/desktop/shell2/common/cpu_compositor.rs

- System identified: yes — rendering pipeline / compositor system
- Existing doc: none (no `doc/guide/` file covers the compositor/rendering pipeline)
- Doc needed: A guide document covering the compositor abstraction (`Compositor` trait), available backends (CPU, GPU), how compositor selection works (`select_compositor_mode`), and how the compositor integrates with the window event loop and display list rendering.

## dll/src/desktop/shell2/common/debug_server.rs

- System identified: yes — Debug Server / E2E Testing / Remote Debugging system
- Existing doc: `doc/guide/envs.md` likely covers `AZ_DEBUG` env var, but no dedicated debug server guide exists
- Doc needed: A `doc/guide/debug-server.md` guide covering: how to start the debug server, available HTTP endpoints, the E2E testing API, JSON command format, screenshot capabilities, and the timer-based architecture. The module doc at the top of the file is excellent and could serve as a starting point.

## dll/src/desktop/shell2/common/dlopen.rs

- System identified: yes — windowing / platform abstraction layer (`shell2`)
- Existing doc: none (no `doc/guide/windowing.md` or `shell2` guide)
- Doc needed: A guide covering the `shell2` windowing abstraction — platform detection, dynamic library loading strategy, how X11/Wayland/Win32/macOS backends are structured, and the common traits they share.

## dll/src/desktop/shell2/common/error.rs

- System identified: yes — windowing/shell2 system
- Existing doc: none (no `doc/guide/windowing.md` or similar)
- Doc needed: A windowing system guide covering the shell2 architecture (platform backends, common abstractions, error handling, compositor integration)

## dll/src/desktop/shell2/common/mod.rs

- System identified: yes — Shell2 windowing/platform abstraction layer
- Existing doc: `doc/guide/architecture.md` covers high-level architecture; `doc/guide/lifecycle.md` covers the event loop lifecycle
- Doc needed: A dedicated `doc/guide/windowing.md` or `doc/guide/shell2.md` guide explaining the shell2 platform abstraction layer, the role of the `common/` module vs platform-specific backends, and the compositor/layout/event pipeline would be valuable. Currently no guide specifically covers this system.

## dll/src/desktop/shell2/headless/mod.rs

- System identified: yes — Headless/Testing backend (part of the windowing/shell system)
- Existing doc: `doc/guide/envs.md` mentions `AZ_BACKEND=headless`; `doc/guide/lifecycle.md` covers the event loop generally. No dedicated headless/testing guide exists.
- Doc needed: A `doc/guide/headless-testing.md` covering the headless backend architecture, CpuBackend, event injection API, and how to write E2E tests using the headless window.

## dll/src/desktop/shell2/linux/common/gl.rs

- System identified: yes — OpenGL/rendering pipeline (specifically GL function loading for Linux)
- Existing doc: none specific to GL loading; `doc/guide/` has no rendering-pipeline guide
- Doc needed: A rendering pipeline / GL initialization guide covering how GL contexts are created and function pointers are loaded across platforms (Linux EGL, macOS dlopen, Windows wglGetProcAddress). This file is part of that system along with `x11/gl.rs`, `wayland/gl.rs`, `macos/gl.rs`, and `windows/gl.rs`.

## dll/src/desktop/shell2/linux/common/mod.rs

- System identified: yes — Linux windowing / platform backends (X11 + Wayland)
- Existing doc: none specific to the Linux windowing system in `doc/guide/`
- Doc needed: A guide document covering the Linux windowing backend architecture (how X11, Wayland, and common code interact) would be useful. This is likely shared with reviews of other files in `shell2/linux/`.

## dll/src/desktop/shell2/linux/dbus/dlopen.rs

- System identified: yes — DBus / GNOME menu integration subsystem (part of the Linux windowing/desktop integration layer)
- Existing doc: none (no `dbus.md` or `gnome-menu.md` in `doc/guide/`)
- Doc needed: A guide document covering the DBus integration system would be useful. It should explain how `dbus/dlopen.rs` provides runtime-loaded FFI bindings, how `gnome_menu/` uses them for GNOME global menu integration, and how both X11 and Wayland backends wire up DBus for features like inhibit-idle and screen-saver control.

## dll/src/desktop/shell2/linux/dbus/mod.rs

- System identified: yes — Linux DBus integration / GNOME menu system
- Existing doc: none (no guide for the Linux windowing/desktop integration system)
- Doc needed: A guide covering the Linux desktop integration system (DBus, GNOME menus, X11/Wayland windowing) would help explain how `dbus/`, `gnome_menu/`, `x11/`, and `wayland/` modules interact within `shell2/linux/`.

## dll/src/desktop/shell2/linux/gnome_menu/actions_protocol.rs

- System identified: yes — GNOME native menu integration (DBus `org.gtk.Actions` protocol)
- Existing doc: none (no `doc/guide/` file for menu/DBus/GNOME integration)
- Doc needed: A guide document covering the GNOME native menu system — how `org.gtk.Actions` and `org.gtk.Menus` DBus interfaces work together, the callback queue architecture (`PendingMenuCallback`), and how it integrates with X11/Wayland event loops.

## dll/src/desktop/shell2/linux/gnome_menu/dbus_connection.rs

- System identified: yes — GNOME native menu integration via DBus (Linux desktop integration / windowing system)
- Existing doc: none (no guide for the GNOME menu / DBus integration system)
- Doc needed: A guide explaining the GNOME Shell menu integration protocol (org.gtk.Menus, org.gtk.Actions), how the dlopen-based DBus approach works, and how it connects to the windowing/shell layer

## dll/src/desktop/shell2/linux/gnome_menu/manager.rs

- System identified: yes — GNOME/GTK menu integration via DBus (part of the Linux windowing/desktop integration system)
- Existing doc: none (no guide document covers GNOME menu integration, DBus, or Linux desktop integration)
- Doc needed: A `doc/guide/linux-desktop-integration.md` covering the GNOME menu system (V1 vs V2 implementations), DBus service registration, X11 property setting, and Wayland app_id matching. Multiple files in `gnome_menu/` would benefit from this system-level overview.

## dll/src/desktop/shell2/linux/gnome_menu/menu_protocol.rs

- System identified: yes — GNOME/GTK application menu integration (windowing / desktop integration subsystem)
- Existing doc: none (no `doc/guide/` file covers GNOME menu, desktop integration, or DBus)
- Doc needed: A guide covering Linux desktop integration — GNOME menus, DBus protocol, how `gnome_menu/` module fits into the shell2 windowing layer

## dll/src/desktop/shell2/linux/gnome_menu/mod.rs

- System identified: yes — Linux Desktop / GNOME Shell Integration (windowing subsystem)
- Existing doc: none (no `doc/guide/` file covers Linux windowing or GNOME menu integration)
- Doc needed: A guide covering Linux windowing integration (X11, Wayland, GNOME menus) would help explain how `shell2/linux/` modules interact. This could be `doc/guide/linux-windowing.md` covering the X11, Wayland, and GNOME menu subsystems.

## dll/src/desktop/shell2/linux/gnome_menu/protocol_impl.rs

- System identified: yes — GNOME desktop menu integration (DBus-based application menu for GNOME Shell)
- Existing doc: none (no guide doc covers the GNOME menu / DBus integration system)
- Doc needed: A guide document covering the GNOME menu integration system, explaining how `manager.rs` orchestrates `protocol_impl.rs`, `actions_protocol.rs`, `menu_protocol.rs`, `menu_conversion.rs`, and `dbus_connection.rs` to expose application menus via the `org.gtk.Menus` and `org.gtk.Actions` DBus interfaces.

## dll/src/desktop/shell2/linux/gnome_menu/shared_dbus.rs

- System identified: yes — Linux desktop shell / DBus integration (windowing system)
- Existing doc: none (no `doc/guide/` file covers the Linux windowing/DBus subsystem)
- Doc needed: A guide covering the Linux desktop shell architecture (`shell2/linux/`), including how DBus, Wayland, and X11 backends are structured and how shared resources like `DBusLib` are managed across windows.

## dll/src/desktop/shell2/linux/gnome_menu/x11_properties.rs

- System identified: yes — GNOME native menu integration (windowing / desktop integration subsystem)
- Existing doc: none (no guide for GNOME menu / DBus menu integration)
- Doc needed: A guide covering the GNOME menu integration system — how DBus menu protocol works, the X11 property advertisement mechanism, how GnomeMenuManager orchestrates the components, and how it integrates with the windowing backend.

## dll/src/desktop/shell2/linux/mod.rs

- System identified: yes — Linux windowing / platform backend (part of the shell2 windowing system)
- Existing doc: `doc/guide/architecture.md` covers high-level architecture; no dedicated windowing/shell guide exists.
- Doc needed: A `doc/guide/windowing.md` or `doc/guide/shell.md` explaining the platform backend abstraction (shell2), how X11/Wayland/macOS/Windows backends are selected and structured, and the event loop lifecycle. Multiple files across `shell2/` would reference this guide.

## dll/src/desktop/shell2/linux/registry.rs

- System identified: yes — **windowing / multi-window management** subsystem
- Existing doc: none (no `doc/guide/windowing.md` or similar)
- Doc needed: A guide covering the multi-window architecture, the per-platform registry pattern (`windows/registry.rs`, `macos/registry.rs`, `linux/registry.rs`), window lifecycle (creation → registration → event routing → unregistration → drop), and the event loop in `run.rs`.

## dll/src/desktop/shell2/linux/resources.rs

- System identified: yes — Linux windowing / shell2 system
- Existing doc: none (no `doc/guide/windowing.md` or `doc/guide/linux.md`)
- Doc needed: A `doc/guide/windowing.md` covering the shell2 windowing abstraction, platform backends (Linux/Wayland/X11, macOS, Windows), shared resources (`AppResources`), and window lifecycle.

## dll/src/desktop/shell2/linux/timer.rs

- System identified: yes — Linux timer subsystem (part of the windowing/event-loop system)
- Existing doc: `doc/guide/lifecycle.md` covers the event loop at a high level
- Doc needed: n/a — this is a small utility module; the event-loop / windowing system would benefit from a dedicated guide, but that is a broader concern not specific to this file.

## dll/src/desktop/shell2/linux/wayland/clipboard.rs

- System identified: yes — Windowing / Platform Clipboard subsystem
- Existing doc: none (no `doc/guide/` file covers windowing, clipboard, or platform integration)
- Doc needed: A `doc/guide/windowing.md` or `doc/guide/platform-integration.md` covering the clipboard subsystem, platform dispatch in `event.rs`, and how each platform backend (Windows, macOS, X11, Wayland) implements clipboard operations.

## dll/src/desktop/shell2/linux/wayland/defines.rs

- System identified: yes — Wayland windowing subsystem (part of the Linux platform shell)
- Existing doc: none (no `doc/guide/wayland.md` or `doc/guide/windowing.md`)
- Doc needed: A windowing system guide covering the platform abstraction layer, including Wayland, X11, macOS, and Windows backends. This file specifically belongs to the Wayland backend's FFI definitions layer.

## dll/src/desktop/shell2/linux/wayland/dlopen.rs

- System identified: yes — Wayland windowing / platform integration
- Existing doc: none (no `doc/guide/wayland.md` or `doc/guide/windowing.md`)
- Doc needed: A guide document covering the Wayland windowing backend (and potentially the windowing system in general, covering X11, Wayland, Win32, Cocoa backends and how they share code via the `shell2` module structure).

## dll/src/desktop/shell2/linux/wayland/events.rs

- System identified: yes — Wayland windowing / event handling subsystem
- Existing doc: none (no `doc/guide/windowing.md` or `doc/guide/wayland.md`)
- Doc needed: A windowing system guide covering the event loop, platform backends (X11, Wayland, macOS, Windows), callback registration patterns, and how events flow from OS to the common `process_window_events()` pipeline.

## dll/src/desktop/shell2/linux/wayland/gl.rs

- System identified: yes — Wayland windowing / OpenGL rendering subsystem
- Existing doc: none (no `doc/guide/wayland.md`, `opengl.md`, or `rendering.md`)
- Doc needed: A guide covering the rendering pipeline (EGL context creation, GL function loading, swap chain) across both X11 and Wayland backends, and how `RenderMode::Gpu` vs CPU compositor is selected.

## dll/src/desktop/shell2/linux/wayland/menu.rs

- System identified: yes — Wayland windowing / menu popup system
- Existing doc: none (no `wayland.md`, `menu.md`, or `windowing.md` in `doc/guide/`)
- Doc needed: A windowing system guide covering how shell2 manages platform windows (X11, Wayland, macOS, Windows), including popup/menu creation and the shared menu_renderer pipeline.

## dll/src/desktop/shell2/linux/x11/accessibility.rs

- System identified: yes — Accessibility (AT-SPI / accesskit integration for Linux)
- Existing doc: none (no `doc/guide/accessibility.md`)
- Doc needed: Accessibility system guide covering the accesskit bridge architecture, per-platform adapters, tree update flow, and action handling

## dll/src/desktop/shell2/linux/x11/clipboard.rs

- System identified: yes — Clipboard / Windowing system (platform abstraction layer)
- Existing doc: none (no clipboard or windowing guide in `doc/guide/`)
- Doc needed: A `doc/guide/clipboard.md` or broader `doc/guide/platform-abstraction.md` explaining the clipboard flow (ClipboardManager in layout -> platform sync_clipboard/get_clipboard_content/write_to_clipboard) would help. The architecture is well-structured across 4 platform modules but undocumented.

## dll/src/desktop/shell2/linux/x11/defines.rs

- System identified: X11 windowing backend (part of the platform shell/windowing system)
- Existing doc: none (no windowing/shell guide in `doc/guide/`)
- Doc needed: A guide covering the windowing/shell system architecture — how X11, Wayland, Win32, and Cocoa backends are structured, the `shell2` module layout, and how `dlopen`-based dynamic loading works. This file (`defines.rs`) would be referenced as the FFI type definitions layer for X11.

## dll/src/desktop/shell2/linux/x11/dlopen.rs

- System identified: yes — X11/Linux windowing and dynamic library loading subsystem
- Existing doc: none (no windowing or platform-backend guide in `doc/guide/`)
- Doc needed: A `doc/guide/windowing.md` or `doc/guide/platform-backends.md` covering the dynamic loading strategy (`DynamicLibraryTrait`, `load_first_available`, per-library structs), the platform-specific backend structure (X11, Wayland, Win32, Cocoa), and how `shell2/` modules are organized.

## dll/src/desktop/shell2/linux/x11/events.rs

- System identified: yes — X11 windowing / event handling (part of the cross-platform Shell2 event system)
- Existing doc: none (no `doc/guide/` document covers the windowing/event architecture)
- Doc needed: A guide covering the Shell2 event processing architecture — the state-diffing pattern shared across Windows/macOS/X11/Wayland, the `PlatformWindow` trait, and how events flow from OS → state update → `create_events_from_states()` → `dispatch_events()` → callbacks.

## dll/src/desktop/shell2/linux/x11/gl.rs

- System identified: yes — X11/Linux windowing and OpenGL rendering pipeline
- Existing doc: none (no guide covers the Linux windowing/GL subsystem)
- Doc needed: A `doc/guide/windowing.md` or `doc/guide/platform-backends.md` covering how the platform-specific shell2 backends (X11, Wayland, macOS, Windows) are structured, how EGL/GL contexts are managed, and how they integrate with the rendering pipeline.

## dll/src/desktop/shell2/linux/x11/menu.rs

- System identified: yes — Windowing / Menu system (Linux X11 platform layer)
- Existing doc: none (no `doc/guide/` file for the menu or windowing system)
- Doc needed: A guide document for the menu system explaining the unified approach (`dll/src/desktop/menu.rs`) vs platform-specific implementations, and how menu popups integrate with the window creation pipeline.

## dll/src/desktop/shell2/macos/accessibility.rs

- System identified: yes — Accessibility system (macOS platform adapter)
- Existing doc: none (no `doc/guide/accessibility.md` or `doc/guide/a11y.md`)
- Doc needed: A guide document covering the accessibility system architecture — how `azul_core::a11y` builds the tree, how platform adapters (macOS/Windows/X11) bridge to OS APIs, and how actions flow back. Files involved: `core/src/a11y.rs`, `layout/src/managers/a11y.rs`, `dll/src/desktop/shell2/macos/accessibility.rs`, `dll/src/desktop/shell2/windows/accessibility.rs`, `dll/src/desktop/shell2/linux/x11/accessibility.rs`.

## dll/src/desktop/shell2/macos/menu.rs

- System identified: yes — macOS windowing / menu system (part of the `shell2` platform abstraction)
- Existing doc: none (no guide for the windowing/shell system)
- Doc needed: A `doc/guide/windowing.md` or `doc/guide/shell.md` covering the platform abstraction layer (`shell2`), including how menus, events, and window lifecycle are handled across macOS/Windows/Linux.

## dll/src/desktop/shell2/macos/registry.rs

- System identified: yes — **windowing / multi-window management** subsystem
- Existing doc: none (no `windowing.md` in `doc/guide/`)
- Doc needed: A `windowing.md` guide explaining the window registry pattern, multi-window lifecycle (creation, event routing, destruction), and how the per-platform registries integrate with the shared event loop in `run.rs`.

## dll/src/desktop/shell2/macos/tooltip.rs

- System identified: yes — macOS windowing/shell system (`shell2/macos/`)
- Existing doc: none (no `doc/guide/windowing.md` or similar)
- Doc needed: A windowing system guide covering the shell2 architecture across platforms (macos, x11, wayland, windows), tooltip lifecycle, and the platform-specific patterns used. Multiple tooltip/shell files would benefit from this shared documentation.

## dll/src/desktop/shell2/windows/tooltip.rs

- System identified: yes — Windows windowing/shell system (`dll/src/desktop/shell2/windows/`)
- Existing doc: none (no windowing guide in `doc/guide/`)
- Doc needed: A `doc/guide/windowing.md` covering the platform windowing abstraction (`shell2`), per-platform tooltip/menu/IME subsystems, and the callback trait (`show_tooltip_from_callback`, `hide_tooltip_from_callback`, etc.)

## dll/src/lib.rs

- System identified: yes — C-API / FFI binding & code generation system
- Existing doc: none (getting-started guides cover usage, not internals)
- Doc needed: `doc/guide/ffi-codegen.md` — document the code generation pipeline
  (`api.json` → `azul-doc codegen all` → `target/codegen/*.rs`), the three link
  modes (build-dll, link-static, link-dynamic), Python extension support, and how
  the `dll` crate wires everything together.

## dll/src/web/cb_gen.rs

- System identified: yes — Web Backend (`AZ_BACKEND=web`)
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a (guide already exists, though it describes the aspirational design rather than current stub state)

## dll/src/web/classify.rs

- System identified: yes — Web backend / WASM compilation pipeline
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a (guide already exists)

## dll/src/web/config.rs

- System identified: yes — Web backend system
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a (guide already exists)

## dll/src/web/loader_js.rs

- System identified: yes — Web target / SSR rendering pipeline (`dll/src/web/`)
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a (guide already exists)

## dll/src/web/mini_gen.rs

- System identified: yes — Web/WASM transpilation pipeline (`dll/src/web/`)
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a (existing guide covers the web system including this module's role)

## dll/src/web/mod.rs

- System identified: **Web backend** (`AZ_BACKEND=web://`)
- Existing doc: `doc/guide/web.md` (comprehensive implementation plan, ~1000+ lines)
- Doc needed: n/a — well-documented

## dll/src/web/server.rs

- System identified: yes — Web backend / server-side rendering system
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a (guide already exists)

## dll/src/web/transpiler.rs

- System identified: Web backend (`dll/src/web/`)
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a (guide already exists)

## layout/src/default_actions.rs

- System identified: yes — Event System / Default Actions (keyboard event → default behavior pipeline)
- Existing doc: none (`doc/guide/` has no event-system or keyboard-actions guide)
- Doc needed: An "event-system.md" guide covering the event dispatch pipeline (capture → target → bubble → default action), how `default_input_interpreter` in `core/src/events.rs` relates to `determine_keyboard_default_action` in this file, and the overall keyboard/mouse/focus event flow.

## layout/src/extra.rs

- System identified: XML / DOM construction system (partially); this file is a small utility, not a core system component
- Existing doc: none specifically for XML DOM construction; `widgets.md` covers widget system
- Doc needed: n/a — the file is too small and peripheral to warrant its own guide document

## layout/src/fmt.rs

- System identified: yes — string formatting / internationalization (fmt + fluent + ICU modules in layout crate)
- Existing doc: none (no guide for i18n/formatting)
- Doc needed: A guide covering the string formatting and internationalization system (fmt.rs for basic strfmt, fluent.rs for Fluent/ICU message formatting, icu.rs for locale-aware number/date formatting). Should explain how C/Python/C++ users pass format arguments via `FmtArgVec`.

## layout/src/glyph_cache.rs

- System identified: **CPU text rendering / glyph rasterization pipeline**
- Existing doc: none — no guide covers CPU rendering, text shaping, or glyph caching
- Doc needed: A guide document covering the CPU rendering pipeline (how glyphs go from font data through hinting, path construction, rasterization cells, and final scanline rendering) would help. This file is one piece; `cpurender.rs` and `font/parsed.rs` are others.

## layout/src/hit_test.rs

- System identified: yes — hit-testing / event handling system
- Existing doc: none (no `doc/guide/hit-testing.md` or `doc/guide/event-handling.md`)
- Doc needed: A guide covering the hit-testing pipeline: how mouse events flow from the windowing shell through `FullHitTest` to cursor resolution (`CursorTypeHitTest`) and event determination. This file is one small piece of a larger event-handling system spanning `core/src/hit_test.rs`, `core/src/events.rs`, `layout/src/hit_test.rs`, `layout/src/event_determination.rs`, and the shell event handlers.

## layout/src/managers/mod.rs

- System identified: yes — "managers / input & state management"
- Existing doc: none (no dedicated guide for the managers subsystem)
- Doc needed: A guide explaining the managers subsystem, the role of each manager, and how they interact with the event loop and window system.

## layout/src/managers/virtual_view.rs

- System identified: yes — VirtualView / lazy-loading layout system
- Existing doc: none (no `doc/guide/` file covers virtual views or lazy loading)
- Doc needed: A guide covering the VirtualView lifecycle (initial render, edge scroll, bounds expansion), how `VirtualViewManager` coordinates with `ScrollManager` and the layout pass, and the callback flow (`VirtualViewCallbackReason` variants). This would fit as `doc/guide/virtual-views.md`.

## layout/src/text3/mod.rs

- System identified: **Text layout and shaping system** (text3)
- Existing doc: None in `doc/guide/`. No `text-layout.md`, `text-shaping.md`, or similar guide exists.
- Doc needed: A `doc/guide/text-layout.md` guide covering the text3 subsystem — shaping pipeline, inline layout, text editing, selection, font fallback/caching. This is a core system used by the layout solver (`solver3`), window event handling, and CPU rendering.

## layout/src/url.rs

- System identified: HTTP/networking utilities (URL parsing for the C API layer)
- Existing doc: none — no guide doc covers HTTP or networking
- Doc needed: A `doc/guide/networking.md` or `doc/guide/http.md` covering URL parsing, HTTP client usage, and how these types flow through the C FFI boundary. (Low priority — small surface area.)

## layout/src/widgets/list_view.rs

- System identified: yes — Widget system (layout/src/widgets/)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (existing guide covers widgets)

## layout/src/widgets/mod.rs

- System identified: Widget system
- Existing doc: `doc/guide/widgets.md` (exists)
- Doc needed: n/a

## layout/src/widgets/node_graph.rs

- System identified: yes — Widget system (node graph editor widget)
- Existing doc: `doc/guide/widgets.md` exists but does not mention NodeGraph
- Doc needed: The NodeGraph widget should be documented in `doc/guide/widgets.md` alongside other widgets. A brief section explaining the widget's purpose, data model (`NodeGraph`, `Node`, `NodeTypeInfo`, `InputOutputInfo`), callback system, and current limitations (connection rendering is non-functional) would be valuable.

## layout/src/widgets/number_input.rs

- System identified: yes — Widget system (input widgets)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide already exists)

## layout/src/widgets/progressbar.rs

- System identified: yes — Widget system (native widget implementations)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide already exists)

## layout/src/widgets/ribbon.rs

- System identified: yes — Widget system (built-in widgets)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: The existing `widgets.md` should document the Ribbon widget once it has call sites. n/a for new guide.

## layout/src/widgets/tabs.rs

- System identified: yes — Widget system (native-styled tab controls)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide already exists)

## layout/src/widgets/text_input.rs

- System identified: yes — Widget system (text input widget)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: The existing `widgets.md` covers widget usage at a high level. There is also `layout/src/managers/text_input.rs` which is a separate text input manager. The relationship between the widget (`widgets/text_input.rs`) and the manager (`managers/text_input.rs`) is not documented anywhere. A brief note in `widgets.md` or the module docs explaining this split would be helpful.

## layout/src/widgets/titlebar.rs

- System identified: yes — **Widgets / CSD (Client-Side Decorations)**
- Existing doc: `doc/guide/widgets.md` exists
- Doc needed: The widgets guide likely covers widget usage. CSD-specific documentation (how titlebar, window decorations, and drag/resize interact with the shell) may warrant a dedicated section in `widgets.md` or a new `doc/guide/csd.md`. The `dll/src/desktop/csd.rs` module doc already provides good inline documentation of the CSD pipeline.

## layout/src/widgets/tree_view.rs

- System identified: yes — Widget system (tree view widget)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (widgets guide exists; tree view should be documented there once the widget is wired up and tested)

## layout/src/window_state.rs

- System identified: **Windowing** — window state management, window creation options
- Existing doc: `doc/guide/lifecycle.md` covers the app lifecycle; `doc/guide/architecture.md` mentions windows. No dedicated windowing guide exists.
- Doc needed: A `doc/guide/windowing.md` covering window creation, `FullWindowState` fields, window lifecycle (create → configure → run → close), and multi-window patterns would be valuable. Multiple files contribute to this system (`layout/src/window_state.rs`, `layout/src/window.rs`, `core/src/window.rs`, platform shell modules).

## layout/src/window.rs

- System identified: **Windowing / Layout Window Management** (part of the layout engine, event loop integration, and rendering pipeline)
- Existing doc: `doc/guide/lifecycle.md` covers the frame lifecycle; `doc/guide/architecture.md` covers high-level architecture. Neither specifically documents `LayoutWindow` or how it coordinates layout, text editing, accessibility, and rendering.
- Doc needed: A **"LayoutWindow Internals"** guide explaining:
  - How `LayoutWindow` orchestrates layout passes, display list generation, and incremental updates
  - The text editing pipeline (record_text_input -> apply_text_changeset -> update_text_cache_after_edit -> regenerate_display_list)
  - The VirtualView lifecycle (scan -> check_reinvoke -> callback -> recursive layout)
  - Timer/thread callback invocation
  - How accessibility tree updates work (full vs incremental)

## layout/src/xml/mod.rs

- System identified: **XML/HTML Parsing System** — responsible for parsing XML/HTML strings into Azul's DOM tree representations (`XmlNode` tree, `FastDom` arena, `StyledDom`).
- Existing doc: none (no XML guide in `doc/guide/`)
- Doc needed: A guide explaining the XML parsing pipeline: input formats accepted (XML, HTML5-lite), the two parsing paths (XmlNode tree via `parse_xml_string` vs. FastDom via `parse_xml_to_fast_dom_with_css`), CSS extraction from `<style>` tags, the component system (`ComponentMap`), and how parsed DOMs feed into layout. The `scripts/XML_COMPONENT_REFACTORING_PLAN.md` describes an incomplete migration from old `XmlComponentTrait` to new `ComponentMap` that would also be relevant context.

## layout/src/xml/svg.rs

- System identified: yes — SVG rendering / tessellation pipeline
- Existing doc: none (no SVG-specific guide in `doc/guide/`)
- Doc needed: A guide covering the SVG subsystem: tessellation via lyon, CPU rendering via agg-rust, clip mask generation, FXAA post-processing, boolean polygon operations, and how `ParsedSvg` / `svg_render` fit into the rendering pipeline. Related scripts: `scripts/SVG_CLIP_MASKS_AGENT_PROMPT.md`.

## layout/src/zip.rs

- System identified: yes — file I/O / resource utilities (ZIP, HTTP, fluent localization)
- Existing doc: none (no guide for the resource/utility subsystem)
- Doc needed: A guide covering the `layout` crate's resource utilities (ZIP, HTTP, fluent, icons) — how they compose, how to load/bundle translations, and how the C API wraps them.

