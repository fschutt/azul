# Azul-Core API Mapping Reference

This document provides a comprehensive mapping of all public azul-core types, functions, and modules.
Use this as a reference when fixing import errors in azul-dll and other crates.

## Module Structure

```
azul_core::
├── animation       - Animation system
├── callbacks       - Callback types, focus handling  
├── dom             - DOM construction, NodeData
├── events          - Event handling (mouse, keyboard, window)
├── geom            - Geometry (LogicalRect, LogicalSize, LogicalPosition, etc.)
├── gl              - OpenGL helper functions
├── glyph           - Glyph type definitions
├── gpu             - GPU value synchronization
├── hit_test        - Hit-testing module
├── id              - Internal node storage (Node, NodeId, NodeHierarchy)
├── macros          - Useful macros
├── menu            - Menu handling (context menu, menubar)
├── prop_cache      - CSS property cache
├── refany          - Type-erased reference wrapper
├── resources       - Font/image management, resource GC
├── selection       - Cursor and text selection
├── style           - CSS cascading
├── styled_dom      - StyledDom (CSSOM)
├── svg             - SVG module
├── task            - Async (task, thread, timer) helpers
├── transform       - CSS transform computation
├── ui_solver       - UI layout solver
├── window          - Window creation/OS windowing API
└── xml             - XML structures
```

## Type Aliases

```rust
pub type FastHashMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;
```

## 1. Geometry Module (`azul_core::geom`)

**All window geometry types moved here from `azul_core::window`**

### Types
```rust
pub struct LogicalRect { pub origin: LogicalPosition, pub size: LogicalSize }
pub struct LogicalPosition { pub x: f32, pub y: f32 }
pub struct LogicalSize { pub width: f32, pub height: f32 }
pub struct PhysicalPosition<T> { pub x: T, pub y: T }
pub type PhysicalPositionI32 = PhysicalPosition<i32>;
pub struct PhysicalSize<T> { pub width: T, pub height: T }
pub type PhysicalSizeU32 = PhysicalSize<u32>;
pub type PhysicalSizeF32 = PhysicalSize<f32>;
```

### Common Import Fixes
- `azul_core::window::LogicalRect` → `azul_core::geom::LogicalRect`
- `azul_core::window::LogicalSize` → `azul_core::geom::LogicalSize`
- `azul_core::window::LogicalPosition` → `azul_core::geom::LogicalPosition`
- `azul_core::window::PhysicalPosition*` → `azul_core::geom::PhysicalPosition*`
- `azul_core::window::PhysicalSize*` → `azul_core::geom::PhysicalSize*`

## 2. Window Module (`azul_core::window`)

**Reduced module - LayoutWindow and many window state types moved to `azul_layout`**

### Remaining Window Types
```rust
// Window identification
pub struct WindowId { pub id: usize }
pub struct IconKey { pub id: usize }

// Renderer configuration
pub struct RendererOptions { ... }
pub enum Vsync { Enabled, Disabled }
pub enum Srgb { Enabled, Disabled }
pub enum HwAcceleration { Enabled, Disabled }
pub enum RendererType { Hardware, Software, HardwareWithSoftwareFallback }

// Window handles (platform-specific)
pub enum RawWindowHandle { IOSHandle, MacOSHandle, XlibHandle, ... }
pub struct IOSHandle { ... }
pub struct MacOSHandle { ... }
pub struct XlibHandle { ... }
pub struct XcbHandle { ... }
pub struct WaylandHandle { ... }
pub struct WindowsHandle { ... }
pub struct WebHandle { ... }
pub struct AndroidHandle { ... }

// Mouse and keyboard
pub enum MouseCursorType { Default, Arrow, Hand, Text, ... }
pub type ScanCode = u32;
pub struct KeyboardState { ... }
pub struct MouseState { ... }
pub struct VirtualKeyCodeCombo { ... }
pub enum ContextMenuMouseButton { ... }

// Input state
pub struct ScrollResult {}
pub enum CursorPosition { OutOfWindow, InWindow(LogicalPosition), Uninitialized }
pub struct DebugState { ... }
pub struct TouchState { ... }

// Window configuration
pub enum WindowTheme { Light, Dark, Unknown }
pub struct Monitor { ... }
pub struct VideoMode { ... }
pub enum WindowPosition { Initialized(LogicalPosition), Uninitialized }
pub enum ImePosition { ... }
pub struct WindowFlags { ... }
pub enum WindowFrame { Normal, Maximized, Fullscreen(FullScreenMode) }
pub enum FullScreenMode { SlowExclusive, FastVideoMode(VideoMode) }

// Platform-specific options
pub struct PlatformSpecificOptions { ... }
pub struct WindowsWindowOptions { ... }
pub enum XWindowType { ... }
pub enum UserAttentionType { ... }
pub struct LinuxWindowOptions { ... }
pub struct MacWindowOptions { ... }
pub struct WasmWindowOptions { ... }
pub struct WaylandTheme { ... }

// Window sizing
pub struct WindowSize { ... }

// Focus and keyboard
pub enum UpdateFocusWarning { ... }
pub enum AcceleratorKey { ... }
pub enum VirtualKeyCode { Key1, Key2, ..., A, B, C, ... }

// Window icons
pub struct SmallWindowIconBytes { ... }
pub struct LargeWindowIconBytes { ... }
pub enum WindowIcon { Small(SmallWindowIconBytes), Large(LargeWindowIconBytes) }
pub struct TaskBarIcon { ... }

// Constants
pub const DEFAULT_TITLE: &str = "Azul App";
```

### Types MOVED to `azul_layout::window_state`
- `FullWindowState` → `azul_layout::window_state::FullWindowState`
- `WindowState` → `azul_layout::window_state::WindowState`

### Types MOVED to `azul_layout::window`
- `WindowInternal` → `azul_layout::window::LayoutWindow` (renamed!)

## 3. Callbacks Module (`azul_core::callbacks`)

**Most callback infrastructure in core, but CallbackInfo moved to layout**

### Core Callback Types (in azul_core)
```rust
pub struct Dummy {}
pub enum Update { DoNothing, RefreshDom, RegenerateStyledDom, RegenerateCssom, ... }

// Layout callbacks
pub type LayoutCallbackType = extern "C" fn(&mut RefAny, &mut LayoutCallbackInfo) -> StyledDom;
pub struct LayoutCallbackInner { ... }
pub type MarshaledLayoutCallbackType = extern "C" fn(...) -> ...;
pub enum LayoutCallback { Raw(LayoutCallbackInner), Marshaled(MarshaledLayoutCallback) }
pub struct MarshaledLayoutCallback { ... }
pub struct MarshaledLayoutCallbackInner { ... }
pub struct InlineGlyph { ... }

// IFrame callbacks
pub type IFrameCallbackType = extern "C" fn(&mut RefAny, &mut IFrameCallbackInfo) -> IFrameCallbackReturn;
pub struct IFrameCallback { ... }
pub struct IFrameCallbackInfo { ... }
pub struct IFrameCallbackReturn { ... }

// Timer callbacks  
pub struct TimerCallbackReturn { ... }

// Layout callback info (partially in core)
pub struct LayoutCallbackInfo { ... }
pub struct HidpiAdjustedBounds { ... }

// Focus handling
pub enum FocusTarget { First, Last, Next, Previous, Parent, ... }
pub struct FocusTargetPath { ... }

// Core callbacks (for FFI)
pub type CoreCallbackType = usize;
pub struct CoreCallback { ... }
pub struct CoreCallbackData { ... }
pub type CoreRenderImageCallbackType = usize;
pub struct CoreRenderImageCallback { ... }
pub struct CoreImageCallback { ... }
```

### Types MOVED to `azul_layout::callbacks`
- `CallbackInfo` → `azul_layout::callbacks::CallbackInfo` (the full one used in layout)
- `Callback` → `azul_layout::callbacks::Callback`
- `MenuCallback` → `azul_layout::callbacks::MenuCallback`
- `ExternalSystemCallbacks` → `azul_layout::callbacks::ExternalSystemCallbacks`

## 4. DOM Module (`azul_core::dom`)

### All DOM Types (in azul_core)
```rust
// Node identification
pub struct TagId(pub u64);
pub struct ScrollTagId(pub TagId);
pub struct DomNodeHash(pub u64);

// Node types
pub enum NodeType { Body, Div, Text(String), Image(ImageRef), ... }

// Event system
pub enum On { MouseOver, MouseDown, KeyPress, ... }
pub enum EventFilter { Hover(HoverEventFilter), Not(NotEventFilter), ... }
pub enum HoverEventFilter { MouseOver, MouseDown, ... }
pub enum NotEventFilter { Hover(HoverEventFilter), Focus(FocusEventFilter), ... }
pub enum FocusEventFilter { FocusReceived, FocusLost, ... }
pub enum WindowEventFilter { WindowResize, WindowScroll, ... }
pub enum ComponentEventFilter { OnComponentUpdate }
pub enum ApplicationEventFilter { }

// IFrame
pub struct IFrameNode { ... }

// CSS classes and IDs
pub enum IdOrClass { Id(String), Class(String) }

// Inline styles
pub enum NodeDataInlineCssProperty { ... }

// Node data
pub struct NodeData { ... }
pub struct NodeDataExt { ... }

// Accessibility
pub struct AccessibilityInfo { ... }
pub enum AccessibilityRole { Button, Checkbox, Link, ... }
pub enum AccessibilityState { Focused, Selected, ... }
pub enum TabIndex { Auto, OverrideInParent(u32), NoKeyboardFocus }

// DOM structure
pub struct DomId { pub inner: usize }
pub struct DomNodeId { pub dom: DomId, pub node: NodeHierarchyItemId }
pub struct Dom { pub root: NodeHierarchy, pub total_nodes: usize, ... }
pub struct CompactDom { ... }

// Functions
pub fn convert_dom_into_compact_dom(mut dom: Dom) -> CompactDom { ... }
```

### Constants
```rust
impl DomId {
    pub const ROOT_ID: DomId = DomId { inner: 0 };
}
```

## 5. Hit Test Module (`azul_core::hit_test`)

### All Hit Test Types
```rust
pub struct HitTest { ... }
pub struct ExternalScrollId(pub u64, pub PipelineId);
pub struct ScrolledNodes { ... }
pub struct OverflowingScrollNode { ... }
pub type PipelineSourceId = u32;

// Scroll position (CHANGED from scroll_x/scroll_y to parent_rect/children_rect)
pub struct ScrollPosition {
    pub parent_rect: LogicalRect,
    pub children_rect: LogicalRect,
}

pub struct DocumentId { pub namespace_id: IdNamespace, pub id: u32 }
pub struct PipelineId(pub PipelineSourceId, pub u32);
pub struct HitTestItem { ... }
pub struct ScrollHitTestItem { ... }
pub struct ScrollStates(pub FastHashMap<ExternalScrollId, ScrollState>);
pub struct ScrollState { ... }

// NEW: Full hit testing
pub struct FullHitTest { ... }
pub struct CursorTypeHitTest { ... }
```

## 6. Resources Module (`azul_core::resources`)

**Major refactoring: `app_resources` → `resources`**

### Key Types
```rust
// DPI and configuration
pub struct DpiScaleFactor { pub inner: f32 }
pub struct AppConfig { ... }
pub enum AppLogLevel { Off, Error, Warn, Info, Debug, Trace }

// Text layout types
pub type WordIndex = usize;
pub type GlyphIndex = usize;
pub type LineLength = f32;
pub type IndexOfLineBreak = usize;
pub type RemainingSpaceToRight = f32;
pub type LineBreaks = Vec<(GlyphIndex, RemainingSpaceToRight)>;

// Primitive flags and image descriptors
pub struct PrimitiveFlags { ... }
pub struct ImageDescriptor { ... }
pub struct ImageDescriptorFlags { ... }

// Resource keys
pub struct IdNamespace(pub u32);
pub enum RawImageFormat { R8, R8G8, R8G8B8, R8G8B8A8, ... }
pub struct ImageKey { pub namespace_id: IdNamespace, pub key: u32 }
pub struct FontKey { pub namespace_id: IdNamespace, pub key: u32 }
pub struct FontInstanceKey { pub namespace_id: IdNamespace, pub key: u32 }

// Images
pub enum DecodedImage { Gl(Texture), Raw((ImageData, ImageDescriptor))) }
pub struct ImageRef { ... }
pub struct ImageRefHash(pub usize);
pub struct ImageCache { ... }
pub enum ImageType { Texture, Cached(ImageRef) }
pub struct ResolvedImage { ... }

// Text exclusion
pub struct TextExclusionArea { ... }
pub enum ExclusionSide { Left, Right }

// Renderer resources trait and struct
pub trait RendererResourcesTrait: core::fmt::Debug { ... }
pub struct RendererResources { ... }

// Image updates and GL texture cache
pub struct UpdateImageResult { ... }
pub struct GlTextureCache { ... }

// Image masks
pub struct ImageMask { ... }

// Font loading
pub enum ImmediateFontId { Resolved(FontKey), Unresolved(String) }

// Raw image data
pub enum RawImageData { U8(Vec<u8>), U16(Vec<u16>), F32(Vec<f32>), ... }
pub struct RawImage { ... }

// Font instance options
pub type FontInstanceFlags = u32;
pub const FONT_INSTANCE_FLAG_SYNTHETIC_BOLD: u32 = 1 << 1;
pub const FONT_INSTANCE_FLAG_EMBEDDED_BITMAPS: u32 = 1 << 2;
// ... (many more font flags)

pub struct GlyphOptions { ... }
pub enum FontRenderMode { Mono, Alpha, Subpixel }
pub struct FontInstancePlatformOptions { ... }
pub enum FontHinting { None, Slight, Normal, Full }
pub enum FontLCDFilter { None, Default, Light, Legacy }
pub struct FontInstanceOptions { ... }
pub struct SyntheticItalics { ... }

// Image data
pub enum ImageData { Raw(Vec<u8>), External(ExternalImageData) }
pub enum ExternalImageType { TextureHandle, Buffer }
pub struct ExternalImageId { ... }

// Glyph outline
pub enum GlyphOutlineOperation { MoveTo(OutlineMoveTo), LineTo(OutlineLineTo), ... }
pub struct OutlineMoveTo { ... }
pub struct OutlineLineTo { ... }
pub struct OutlineQuadTo { ... }
pub struct OutlineCubicTo { ... }
pub struct GlyphOutline { ... }
pub struct OwnedGlyphBoundingBox { ... }

// External image data
pub enum ImageBufferKind { Texture2D, TextureRect, TextureExternal, ... }
pub struct ExternalImageData { ... }
pub type TileSize = u16;

// Resource updates
pub enum ImageDirtyRect { Partial(LogicalRect), All }
pub enum ResourceUpdate { AddImage(AddImage), UpdateImage(UpdateImage), ... }
pub struct AddImage { ... }
pub struct UpdateImage { ... }
pub struct AddFont { ... }
pub struct AddFontInstance { ... }
pub struct FontVariation { ... }

// Epoch (frame counter)
pub struct Epoch { pub inner: u32 }

// Au (app unit)
pub struct Au(pub i32);
pub const AU_PER_PX: i32 = 60;
pub const MAX_AU: i32 = (1 << 30) - 1;
pub const MIN_AU: i32 = -(1 << 30) - 1;

// Resource messages
pub enum AddFontMsg { Font(AddFont), Instance(AddFontInstance, Au) }
pub enum DeleteFontMsg { Font(FontKey), Instance(FontInstanceKey) }
pub struct AddImageMsg(pub AddImage);
pub struct DeleteImageMsg(ImageKey);

// Font loading callbacks
pub struct LoadedFontSource { ... }
pub type LoadFontFn = fn(&StyleFontFamily, &FcFontCache) -> Option<LoadedFontSource>;
pub type ParseFontFn = fn(LoadedFontSource) -> Option<FontRef>;
pub type GlStoreImageFn = fn(DocumentId, Epoch, Texture) -> ExternalImageId;
```

### Key Functions
```rust
pub fn image_ref_get_hash(ir: &ImageRef) -> ImageRefHash { ... }
pub fn font_ref_get_hash(fr: &FontRef) -> u64 { ... }
pub fn add_fonts_and_images(...) { ... }
pub fn font_size_to_au(font_size: StyleFontSize) -> Au { ... }
pub fn build_add_font_resource_updates(...) { ... }
pub fn build_add_image_resource_updates(...) { ... }
pub fn add_resources(...) { ... }
```

### Import Fix
- `azul_core::app_resources` → `azul_core::resources`

## 7. Task Module (`azul_core::task`)

**Timers and Threads moved to `azul_layout`, only base types remain**

### Types Remaining in azul_core::task
```rust
// Timer control
pub enum TerminateTimer { Continue, Terminate }

// IDs
pub struct TimerId { pub id: usize }
pub struct ThreadId { pub id: usize, pub seed: u64 }

// Time measurement
pub enum Instant { System(SystemTick), Tick(SystemTick) }
pub struct SystemTick { pub tick_counter: usize }
pub struct AzInstantPtr { ... }
pub type InstantPtrCloneCallbackType = extern "C" fn(*const AzInstantPtr) -> AzInstantPtr;
pub struct InstantPtrCloneCallback { ... }
pub type InstantPtrDestructorCallbackType = extern "C" fn(*mut AzInstantPtr);
pub struct InstantPtrDestructorCallback { ... }

// Duration
pub enum Duration { System(SystemTimeDiff), Tick(SystemTickDiff) }
pub struct SystemTickDiff { ... }
pub struct SystemTimeDiff { ... }

// Thread messaging
pub enum ThreadSendMsg { Tick, TerminateThread, Custom(RefAny) }
pub struct ThreadReceiver { ... }
pub struct ThreadReceiverInner { ... }

// Callbacks
pub type GetSystemTimeCallbackType = extern "C" fn() -> Instant;
pub struct GetSystemTimeCallback { ... }
pub type CheckThreadFinishedCallbackType = ...;
pub struct CheckThreadFinishedCallback { ... }
pub type LibrarySendThreadMsgCallbackType = ...;
pub struct LibrarySendThreadMsgCallback { ... }
pub type ThreadRecvCallbackType = ...;
pub struct ThreadRecvCallback { ... }
pub type ThreadReceiverDestructorCallbackType = extern "C" fn(*mut ThreadReceiverInner);
pub struct ThreadReceiverDestructorCallback { ... }
```

### Types MOVED to azul_layout
- `Timer` → `azul_layout::timer::Timer`
- `Thread` → `azul_layout::thread::Thread`
- All Timer/Thread management → `azul_layout::{timer,thread}::{Timer,Thread}`

## 8. ID Module (`azul_core::id`)

### Node Hierarchy Types
```rust
pub use self::node_id::NodeId;
pub type NodeDepths = Vec<(usize, NodeId)>;

pub struct Node { ... }
pub const ROOT_NODE: Node = Node { ... };

// Node hierarchy
pub struct NodeHierarchy { ... }
pub struct NodeHierarchyRef<'a> { ... }
pub struct NodeHierarchyRefMut<'a> { ... }

// Node data containers
pub struct NodeDataContainer<T> { ... }
pub struct NodeDataContainerRef<'a, T> { ... }
pub struct NodeDataContainerRefMut<'a, T> { ... }

// Iterators
pub struct LinearIterator { ... }
pub struct Ancestors<'a> { ... }
pub struct PrecedingSiblings<'a> { ... }
pub struct FollowingSiblings<'a> { ... }
pub struct AzChildren<'a> { ... }
pub struct AzReverseChildren<'a> { ... }
pub struct Children<'a> { ... }
pub struct ReverseChildren<'a> { ... }
pub struct Descendants<'a>(Traverse<'a>);
pub enum NodeEdge<T> { Start(T), End(T) }
pub struct Traverse<'a> { ... }
pub struct ReverseTraverse<'a> { ... }
```

## 9. Selection Module (`azul_core::selection`)

### Selection Types
```rust
pub struct ContentIndex { ... }
pub struct GraphemeClusterId { ... }
pub enum CursorAffinity { Upstream, Downstream }
pub struct TextCursor { ... }
pub struct SelectionRange { ... }
pub enum Selection { All, RangeFrom(usize), RangeTo(usize), ... }
pub struct SelectionState { ... }
```

## 10. Menu Module (`azul_core::menu`)

### Menu Types
```rust
pub struct Menu { ... }
pub enum MenuPopupPosition { ... }
pub enum MenuItemState { ... }
pub enum MenuItem { String(StringMenuItem), Label(String), Separator }
pub struct StringMenuItem { ... }
pub enum MenuItemIcon { None, Checkbox, Image(ImageRef) }
pub struct CoreMenuCallback { ... }
```

## 11. Animation Module (`azul_core::animation`)

### Animation Types
```rust
pub enum UpdateImageType { ... }
pub struct AnimationData { ... }
pub struct Animation { ... }
pub enum AnimationRepeat { Loop, PingPong, NoRepeat }
pub enum AnimationRepeatCount { Times(usize), Infinite }
```

## 12. SVG Module (`azul_core::svg`)

### SVG Types
```rust
pub struct SvgSize { ... }
pub struct SvgLine { ... }
pub enum SvgPathElement { ... }
pub struct SvgPath { ... }
pub struct SvgMultiPolygon { ... }
pub enum SvgNode { ... }
pub enum SvgSimpleNode { ... }
pub struct SvgStyledNode { ... }
pub struct SvgVertex { ... }
pub struct SvgColoredVertex { ... }
pub struct SvgCircle { ... }

// Tessellated SVG
pub struct TessellatedSvgNode { ... }
pub struct TessellatedSvgNodeVecRef { ... }
pub struct TessellatedColoredSvgNode { ... }
pub struct TessellatedColoredSvgNodeVecRef { ... }
pub struct TessellatedGPUSvgNode { ... }
pub struct TessellatedColoredGPUSvgNode { ... }

// SVG styling
pub enum SvgStyle { ... }
pub enum SvgFillRule { ... }
pub struct SvgTransform { ... }
pub struct SvgFillStyle { ... }
pub struct SvgStrokeStyle { ... }
pub struct SvgDashPattern { ... }
pub enum SvgLineCap { ... }
pub enum SvgLineJoin { ... }

// SVG parsing and rendering
pub enum c_void {}
pub type GlyphId = u16;
pub struct SvgXmlNode { ... }
pub struct Svg { ... }
pub enum ShapeRendering { ... }
pub enum ImageRendering { ... }
pub enum TextRendering { ... }
pub enum FontDatabase { ... }
pub struct SvgRenderOptions { ... }
pub struct SvgRenderTransform { ... }
pub enum SvgFitTo { ... }
pub struct SvgParseOptions { ... }
pub struct SvgXmlOptions { ... }
pub enum SvgParseError { ... }
pub enum Indent { ... }
```

## 13. Transform Module (`azul_core::transform`)

### Transform Types
```rust
pub enum RotationMode { ... }
pub struct ComputedTransform3D { ... }
```

## 14. UI Solver Module (`azul_core::ui_solver`)

**Most layout logic moved to `azul_layout::solver3`, some constants remain**

### Constants and Basic Types
```rust
pub const DEFAULT_FONT_SIZE_PX: isize = 16;
pub const DEFAULT_FONT_SIZE: StyleFontSize = ...;
pub const DEFAULT_FONT_ID: &str = "serif";
pub const DEFAULT_TEXT_COLOR: StyleTextColor = ...;
pub const DEFAULT_LINE_HEIGHT: f32 = 1.0;
pub const DEFAULT_WORD_SPACING: f32 = 1.0;
pub const DEFAULT_LETTER_SPACING: f32 = 0.0;
pub const DEFAULT_TAB_WIDTH: f32 = 4.0;

pub struct ResolvedOffsets { ... }
pub enum FormattingContext { ... }
pub type GlyphIndex = u32;
pub struct GlyphInstance { ... }
pub struct QuickResizeResult { ... }
pub struct OverflowInfo { ... }
pub enum DirectionalOverflowInfo { ... }
pub enum PositionInfo { ... }
pub struct PositionInfoInner { ... }
pub struct StyleBoxShadowOffsets { ... }
```

### Types MOVED to azul_layout
- `LayoutResult` → `azul_layout::solver3::LayoutResult`
- All layout solver logic → `azul_layout::solver3`

## 15. Prop Cache Module (`azul_core::prop_cache`)

### CSS Property Cache
```rust
pub struct CssPropertyCache { ... }
pub struct CssPropertyCachePtr { ... }
```

## 16. Style Module (`azul_core::style`)

(Check azul_core/src/style.rs for public types - typically CSS cascade logic)

## 17. Styled DOM Module (`azul_core::styled_dom`)

(Check azul_core/src/styled_dom.rs for StyledDom structure)

## 18. GPU Module (`azul_core::gpu`)

```rust
pub struct GpuValueCache { ... }
```

## 19. GL Module (`azul_core::gl`)

OpenGL helper functions and VirtualGlDriver for testing

## 20. Glyph Module (`azul_core::glyph`)

Glyph type definitions

## 21. Events Module (`azul_core::events`)

Event handling (mouse, keyboard, window events)

## 22. XML Module (`azul_core::xml`)

XML structures

## 23. RefAny Module (`azul_core::refany`)

```rust
pub struct RefAny { ... }
```

---

## AZUL-LAYOUT Module Structure

```
azul_layout::
├── callbacks       - Full CallbackInfo, Callback, MenuCallback, ExternalSystemCallbacks
├── display_list    - Display list generation (moved from azul_core)
├── focus           - Focus target resolution
├── font            - Font management
├── hit_test        - Hit test computation (complements azul_core::hit_test)
├── scroll          - ScrollStates manager
├── solver3         - Layout solver (flex, grid, block, inline, tables)
├── text3           - Text shaping and layout
├── thread          - Thread management (moved from azul_core::task)
├── timer           - Timer management (moved from azul_core::task)
├── window          - LayoutWindow (replaces WindowInternal)
└── window_state    - FullWindowState, WindowState (moved from azul_core::window)
```

---

## AZUL-CSS Important Migrations

### Parser Module
- `azul_css::parser` → `azul_css::parser2` (NEW PARSER)

### Color Types  
- `azul_css::ColorU` → `azul_css::props::basic::color::ColorU`

### Layout Types
- All layout dimension types in `azul_css::props::layout::dimensions`
- LayoutDisplay, LayoutPosition remain in azul_css

---

## COMMON IMPORT FIXES SUMMARY

### Window Types
```rust
// OLD → NEW
azul_core::window::LogicalRect → azul_core::geom::LogicalRect
azul_core::window::LogicalSize → azul_core::geom::LogicalSize
azul_core::window::LogicalPosition → azul_core::geom::LogicalPosition
azul_core::window::WindowInternal → azul_layout::window::LayoutWindow
azul_core::window::FullWindowState → azul_layout::window_state::FullWindowState
azul_core::window::WindowState → azul_layout::window_state::WindowState
```

### Callbacks
```rust
// OLD → NEW
azul_core::callbacks::CallbackInfo → azul_layout::callbacks::CallbackInfo
azul_core::callbacks::Callback → azul_layout::callbacks::Callback
azul_core::callbacks::MenuCallback → azul_layout::callbacks::MenuCallback
```

### Resources
```rust
// OLD → NEW
azul_core::app_resources → azul_core::resources
```

### Task System
```rust
// OLD → NEW
azul_core::task::Timer → azul_layout::timer::Timer
azul_core::task::Thread → azul_layout::thread::Thread
```

### Layout Solver
```rust
// OLD → NEW
azul_core::ui_solver::LayoutResult → azul_layout::solver3::LayoutResult
```

### Display List
```rust
// OLD → NEW
azul_core::display_list → azul_layout::display_list
```

### CSS
```rust
// OLD → NEW
azul_css::parser → azul_css::parser2
azul_css::ColorU → azul_css::props::basic::color::ColorU
```

---

## NOTES

1. **DocumentId and IdNamespace creation**: Use atomic counters, not `::new()` methods
2. **ScrollPosition structure**: Changed from `{scroll_x, scroll_y}` to `{parent_rect, children_rect}`
3. **Thread feature flag**: azul-layout needs `std = ["azul-core/std"]` for Thread support
4. **DomId::ROOT_ID**: Use this constant instead of `DomId { inner: 0 }`
5. **LayoutWindow**: This is the main entry point for layout, replaces WindowInternal

---

## SEARCH STRATEGY FOR FIXING IMPORTS

1. **Grep for unresolved imports**: `cargo check 2>&1 | grep "error\[E0432\]"`
2. **Check this document** for correct module path
3. **Common patterns**:
   - Window geometry → `azul_core::geom`
   - Window state → `azul_layout::window_state`
   - Callbacks → `azul_layout::callbacks`
   - Resources → `azul_core::resources` (not app_resources)
   - Timers/Threads → `azul_layout::{timer,thread}`
4. **Verify**: Run `cargo check` after each batch of fixes

---

## VERSION INFO

This mapping is for the current refactoring state as of the conversation.
Main changes:
- azul-core: Geometry split out, window types reduced, resources renamed
- azul-layout: New crate with solver3, text3, callbacks, window state
- azul-css: New parser2, color paths changed
