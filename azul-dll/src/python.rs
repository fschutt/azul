
use pyo3::prelude::*;
use core::ffi::c_void;

/// Main application class
#[repr(C)]
#[pyclass(name = "App")]
pub struct AzApp {
    pub ptr: *const c_void,
}

/// Configuration to set which messages should be logged.
#[repr(C)]
pub enum AzAppLogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Version of the layout solver to use - future binary versions of azul may have more fields here, necessary so that old compiled applications don't break with newer releases of azul. Newer layout versions are opt-in only.
#[repr(C)]
pub enum AzLayoutSolverVersion {
    March2021,
}

/// Whether the renderer has VSync enabled
#[repr(C)]
pub enum AzVsync {
    Enabled,
    Disabled,
}

/// Does the renderer render in SRGB color space? By default, azul tries to set it to `Enabled` and falls back to `Disabled` if the OpenGL context can't be initialized properly
#[repr(C)]
pub enum AzSrgb {
    Enabled,
    Disabled,
}

/// Does the renderer render using hardware acceleration? By default, azul tries to set it to `Enabled` and falls back to `Disabled` if the OpenGL context can't be initialized properly
#[repr(C)]
pub enum AzHwAcceleration {
    Enabled,
    Disabled,
}

/// Offset in physical pixels (integer units)
#[repr(C)]
#[pyclass(name = "LayoutPoint")]
pub struct AzLayoutPoint {
    pub x: isize,
    pub y: isize,
}

/// Size in physical pixels (integer units)
#[repr(C)]
#[pyclass(name = "LayoutSize")]
pub struct AzLayoutSize {
    pub width: isize,
    pub height: isize,
}

/// Re-export of rust-allocated (stack based) `IOSHandle` struct
#[repr(C)]
#[pyclass(name = "IOSHandle")]
pub struct AzIOSHandle {
    pub ui_window: *mut c_void,
    pub ui_view: *mut c_void,
    pub ui_view_controller: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `MacOSHandle` struct
#[repr(C)]
#[pyclass(name = "MacOSHandle")]
pub struct AzMacOSHandle {
    pub ns_window: *mut c_void,
    pub ns_view: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `XlibHandle` struct
#[repr(C)]
#[pyclass(name = "XlibHandle")]
pub struct AzXlibHandle {
    pub window: u64,
    pub display: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `XcbHandle` struct
#[repr(C)]
#[pyclass(name = "XcbHandle")]
pub struct AzXcbHandle {
    pub window: u32,
    pub connection: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `WaylandHandle` struct
#[repr(C)]
#[pyclass(name = "WaylandHandle")]
pub struct AzWaylandHandle {
    pub surface: *mut c_void,
    pub display: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `WindowsHandle` struct
#[repr(C)]
#[pyclass(name = "WindowsHandle")]
pub struct AzWindowsHandle {
    pub hwnd: *mut c_void,
    pub hinstance: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `WebHandle` struct
#[repr(C)]
#[pyclass(name = "WebHandle")]
pub struct AzWebHandle {
    pub id: u32,
}

/// Re-export of rust-allocated (stack based) `AndroidHandle` struct
#[repr(C)]
#[pyclass(name = "AndroidHandle")]
pub struct AzAndroidHandle {
    pub a_native_window: *mut c_void,
}

/// X11 window hint: Type of window
#[repr(C)]
pub enum AzXWindowType {
    Desktop,
    Dock,
    Toolbar,
    Menu,
    Utility,
    Splash,
    Dialog,
    DropdownMenu,
    PopupMenu,
    Tooltip,
    Notification,
    Combo,
    Dnd,
    Normal,
}

/// Same as `LayoutPoint`, but uses `i32` instead of `isize`
#[repr(C)]
#[pyclass(name = "PhysicalPositionI32")]
pub struct AzPhysicalPositionI32 {
    pub x: i32,
    pub y: i32,
}

/// Same as `LayoutPoint`, but uses `u32` instead of `isize`
#[repr(C)]
#[pyclass(name = "PhysicalSizeU32")]
pub struct AzPhysicalSizeU32 {
    pub width: u32,
    pub height: u32,
}

/// Logical position (can differ based on HiDPI settings). Usually this is what you'd want for hit-testing and positioning elements.
#[repr(C)]
#[pyclass(name = "LogicalPosition")]
pub struct AzLogicalPosition {
    pub x: f32,
    pub y: f32,
}

/// A size in "logical" (non-HiDPI-adjusted) pixels in floating-point units
#[repr(C)]
#[pyclass(name = "LogicalSize")]
pub struct AzLogicalSize {
    pub width: f32,
    pub height: f32,
}

/// Unique hash of a window icon, so that azul does not have to compare the actual bytes to see wether the window icon has changed.
#[repr(C)]
#[pyclass(name = "IconKey")]
pub struct AzIconKey {
    pub id: usize,
}

/// Symbolic name for a keyboard key, does **not** take the keyboard locale into account
#[repr(C)]
pub enum AzVirtualKeyCode {
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,
    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Caret,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadDivide,
    NumpadDecimal,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    NumpadMultiply,
    NumpadSubtract,
    AbntC1,
    AbntC2,
    Apostrophe,
    Apps,
    Asterisk,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    OEM102,
    Period,
    PlayPause,
    Plus,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Copy,
    Paste,
    Cut,
}

/// Boolean flags relating to the current window state
#[repr(C)]
#[pyclass(name = "WindowFlags")]
pub struct AzWindowFlags {
    pub is_maximized: bool,
    pub is_minimized: bool,
    pub is_about_to_close: bool,
    pub is_fullscreen: bool,
    pub has_decorations: bool,
    pub is_visible: bool,
    pub is_always_on_top: bool,
    pub is_resizable: bool,
    pub has_focus: bool,
    pub has_extended_window_frame: bool,
    pub has_blur_behind_window: bool,
}

/// Debugging information, will be rendered as an overlay on top of the UI
#[repr(C)]
#[pyclass(name = "DebugState")]
pub struct AzDebugState {
    pub profiler_dbg: bool,
    pub render_target_dbg: bool,
    pub texture_cache_dbg: bool,
    pub gpu_time_queries: bool,
    pub gpu_sample_queries: bool,
    pub disable_batching: bool,
    pub epochs: bool,
    pub echo_driver_messages: bool,
    pub show_overdraw: bool,
    pub gpu_cache_dbg: bool,
    pub texture_cache_dbg_clear_evicted: bool,
    pub picture_caching_dbg: bool,
    pub primitive_dbg: bool,
    pub zoom_dbg: bool,
    pub small_screen: bool,
    pub disable_opaque_pass: bool,
    pub disable_alpha_pass: bool,
    pub disable_clip_masks: bool,
    pub disable_text_prims: bool,
    pub disable_gradient_prims: bool,
    pub obscure_images: bool,
    pub glyph_flashing: bool,
    pub smart_profiler: bool,
    pub invalidation_dbg: bool,
    pub tile_cache_logging_dbg: bool,
    pub profiler_capture: bool,
    pub force_picture_invalidation: bool,
}

/// Current icon of the mouse cursor
#[repr(C)]
pub enum AzMouseCursorType {
    Default,
    Crosshair,
    Hand,
    Arrow,
    Move,
    Text,
    Wait,
    Help,
    Progress,
    NotAllowed,
    ContextMenu,
    Cell,
    VerticalText,
    Alias,
    Copy,
    NoDrop,
    Grab,
    Grabbing,
    AllScroll,
    ZoomIn,
    ZoomOut,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
}

/// Renderer type of the current windows OpenGL context
#[repr(C)]
pub enum AzRendererType {
    Hardware,
    Software,
}

/// Re-export of rust-allocated (stack based) `MacWindowOptions` struct
#[repr(C)]
#[pyclass(name = "MacWindowOptions")]
pub struct AzMacWindowOptions {
    pub _reserved: u8,
}

/// Re-export of rust-allocated (stack based) `WasmWindowOptions` struct
#[repr(C)]
#[pyclass(name = "WasmWindowOptions")]
pub struct AzWasmWindowOptions {
    pub _reserved: u8,
}

/// Re-export of rust-allocated (stack based) `FullScreenMode` struct
#[repr(C)]
pub enum AzFullScreenMode {
    SlowFullScreen,
    FastFullScreen,
    SlowWindowed,
    FastWindowed,
}

/// Window theme, set by the operating system or `WindowCreateOptions.theme` on startup
#[repr(C)]
pub enum AzWindowTheme {
    DarkMode,
    LightMode,
}

/// Current state of touch devices / touch inputs
#[repr(C)]
#[pyclass(name = "TouchState")]
pub struct AzTouchState {
    pub unused: u8,
}

/// C-ABI stable wrapper over a `LayoutCallbackType`
#[repr(C)]
#[pyclass(name = "LayoutCallback")]
pub struct AzLayoutCallback {
    pub cb: AzLayoutCallbackType,
}

/// `AzLayoutCallbackType` struct
pub type AzLayoutCallbackType = extern "C" fn(&mut AzRefAny, AzLayoutCallbackInfo) -> AzStyledDom;

/// C-ABI stable wrapper over a `CallbackType`
#[repr(C)]
#[pyclass(name = "Callback")]
pub struct AzCallback {
    pub cb: AzCallbackType,
}

/// `AzCallbackType` struct
pub type AzCallbackType = extern "C" fn(&mut AzRefAny, AzCallbackInfo) -> AzUpdateScreen;

/// Specifies if the screen should be updated after the callback function has returned
#[repr(C)]
pub enum AzUpdateScreen {
    DoNothing,
    RegenerateStyledDomForCurrentWindow,
    RegenerateStyledDomForAllWindows,
}

/// Index of a Node in the internal `NodeDataContainer`
#[repr(C)]
#[pyclass(name = "NodeId")]
pub struct AzNodeId {
    pub inner: usize,
}

/// ID of a DOM - one window can contain multiple, nested DOMs (such as iframes)
#[repr(C)]
#[pyclass(name = "DomId")]
pub struct AzDomId {
    pub inner: usize,
}

/// Re-export of rust-allocated (stack based) `PositionInfoInner` struct
#[repr(C)]
#[pyclass(name = "PositionInfoInner")]
pub struct AzPositionInfoInner {
    pub x_offset: f32,
    pub y_offset: f32,
    pub static_x_offset: f32,
    pub static_y_offset: f32,
}

/// How should an animation repeat (loop, ping-pong, etc.)
#[repr(C)]
pub enum AzAnimationRepeat {
    NoRepeat,
    Loop,
    PingPong,
}

/// How many times should an animation repeat
#[repr(C, u8)]
pub enum AzAnimationRepeatCount {
    Times(usize),
    Infinite,
}

/// C-ABI wrapper over an `IFrameCallbackType`
#[repr(C)]
#[pyclass(name = "IFrameCallback")]
pub struct AzIFrameCallback {
    pub cb: AzIFrameCallbackType,
}

/// `AzIFrameCallbackType` struct
pub type AzIFrameCallbackType = extern "C" fn(&mut AzRefAny, AzIFrameCallbackInfo) -> AzIFrameCallbackReturn;

/// Re-export of rust-allocated (stack based) `RenderImageCallback` struct
#[repr(C)]
#[pyclass(name = "RenderImageCallback")]
pub struct AzRenderImageCallback {
    pub cb: AzRenderImageCallbackType,
}

/// `AzRenderImageCallbackType` struct
pub type AzRenderImageCallbackType = extern "C" fn(&mut AzRefAny, AzRenderImageCallbackInfo) -> AzImageRef;

/// Re-export of rust-allocated (stack based) `TimerCallback` struct
#[repr(C)]
#[pyclass(name = "TimerCallback")]
pub struct AzTimerCallback {
    pub cb: AzTimerCallbackType,
}

/// `AzTimerCallbackType` struct
pub type AzTimerCallbackType = extern "C" fn(&mut AzRefAny, &mut AzRefAny, AzTimerCallbackInfo) -> AzTimerCallbackReturn;

/// `AzWriteBackCallbackType` struct
pub type AzWriteBackCallbackType = extern "C" fn(&mut AzRefAny, AzRefAny, AzCallbackInfo) -> AzUpdateScreen;

/// Re-export of rust-allocated (stack based) `WriteBackCallback` struct
#[repr(C)]
#[pyclass(name = "WriteBackCallback")]
pub struct AzWriteBackCallback {
    pub cb: AzWriteBackCallbackType,
}

/// Re-export of rust-allocated (stack based) `ThreadCallback` struct
#[repr(C)]
#[pyclass(name = "ThreadCallback")]
pub struct AzThreadCallback {
    pub cb: AzThreadCallbackType,
}

/// `AzThreadCallbackType` struct
pub type AzThreadCallbackType = extern "C" fn(AzRefAny, AzThreadSender, AzThreadReceiver);

/// `AzRefAnyDestructorType` struct
pub type AzRefAnyDestructorType = extern "C" fn(&mut c_void);

/// Re-export of rust-allocated (stack based) `RefCount` struct
#[repr(C)]
#[pyclass(name = "RefCount")]
pub struct AzRefCount {
    pub ptr: *const c_void,
}

/// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
#[repr(C)]
pub enum AzOn {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    MiddleMouseDown,
    RightMouseDown,
    MouseUp,
    LeftMouseUp,
    MiddleMouseUp,
    RightMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    FocusReceived,
    FocusLost,
}

/// Re-export of rust-allocated (stack based) `HoverEventFilter` struct
#[repr(C)]
pub enum AzHoverEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
}

/// Re-export of rust-allocated (stack based) `FocusEventFilter` struct
#[repr(C)]
pub enum AzFocusEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    FocusReceived,
    FocusLost,
}

/// Re-export of rust-allocated (stack based) `WindowEventFilter` struct
#[repr(C)]
pub enum AzWindowEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    Resized,
    Moved,
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
    FocusReceived,
    FocusLost,
    CloseRequested,
    ThemeChanged,
}

/// Re-export of rust-allocated (stack based) `ComponentEventFilter` struct
#[repr(C)]
pub enum AzComponentEventFilter {
    AfterMount,
    BeforeUnmount,
    NodeResized,
}

/// Re-export of rust-allocated (stack based) `ApplicationEventFilter` struct
#[repr(C)]
pub enum AzApplicationEventFilter {
    DeviceConnected,
    DeviceDisconnected,
}

/// Re-export of rust-allocated (stack based) `TabIndex` struct
#[repr(C, u8)]
pub enum AzTabIndex {
    Auto,
    OverrideInParent(u32),
    NoKeyboardFocus,
}

/// Re-export of rust-allocated (stack based) `NodeTypeKey` struct
#[repr(C)]
pub enum AzNodeTypeKey {
    Body,
    Div,
    Br,
    P,
    Img,
    IFrame,
}

/// Re-export of rust-allocated (stack based) `CssNthChildPattern` struct
#[repr(C)]
#[pyclass(name = "CssNthChildPattern")]
pub struct AzCssNthChildPattern {
    pub repeat: u32,
    pub offset: u32,
}

/// Re-export of rust-allocated (stack based) `CssPropertyType` struct
#[repr(C)]
pub enum AzCssPropertyType {
    TextColor,
    FontSize,
    FontFamily,
    TextAlign,
    LetterSpacing,
    LineHeight,
    WordSpacing,
    TabWidth,
    Cursor,
    Display,
    Float,
    BoxSizing,
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    Position,
    Top,
    Right,
    Left,
    Bottom,
    FlexWrap,
    FlexDirection,
    FlexGrow,
    FlexShrink,
    JustifyContent,
    AlignItems,
    AlignContent,
    OverflowX,
    OverflowY,
    PaddingTop,
    PaddingLeft,
    PaddingRight,
    PaddingBottom,
    MarginTop,
    MarginLeft,
    MarginRight,
    MarginBottom,
    Background,
    BackgroundImage,
    BackgroundColor,
    BackgroundPosition,
    BackgroundSize,
    BackgroundRepeat,
    BorderTopLeftRadius,
    BorderTopRightRadius,
    BorderBottomLeftRadius,
    BorderBottomRightRadius,
    BorderTopColor,
    BorderRightColor,
    BorderLeftColor,
    BorderBottomColor,
    BorderTopStyle,
    BorderRightStyle,
    BorderLeftStyle,
    BorderBottomStyle,
    BorderTopWidth,
    BorderRightWidth,
    BorderLeftWidth,
    BorderBottomWidth,
    BoxShadowLeft,
    BoxShadowRight,
    BoxShadowTop,
    BoxShadowBottom,
    ScrollbarStyle,
    Opacity,
    Transform,
    PerspectiveOrigin,
    TransformOrigin,
    BackfaceVisibility,
}

/// Re-export of rust-allocated (stack based) `ColorU` struct
#[repr(C)]
#[pyclass(name = "ColorU")]
pub struct AzColorU {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Re-export of rust-allocated (stack based) `SizeMetric` struct
#[repr(C)]
pub enum AzSizeMetric {
    Px,
    Pt,
    Em,
    Percent,
}

/// Re-export of rust-allocated (stack based) `FloatValue` struct
#[repr(C)]
#[pyclass(name = "FloatValue")]
pub struct AzFloatValue {
    pub number: isize,
}

/// Re-export of rust-allocated (stack based) `BoxShadowClipMode` struct
#[repr(C)]
pub enum AzBoxShadowClipMode {
    Outset,
    Inset,
}

/// Re-export of rust-allocated (stack based) `LayoutAlignContent` struct
#[repr(C)]
pub enum AzLayoutAlignContent {
    Stretch,
    Center,
    Start,
    End,
    SpaceBetween,
    SpaceAround,
}

/// Re-export of rust-allocated (stack based) `LayoutAlignItems` struct
#[repr(C)]
pub enum AzLayoutAlignItems {
    Stretch,
    Center,
    FlexStart,
    FlexEnd,
}

/// Re-export of rust-allocated (stack based) `LayoutBoxSizing` struct
#[repr(C)]
pub enum AzLayoutBoxSizing {
    ContentBox,
    BorderBox,
}

/// Re-export of rust-allocated (stack based) `LayoutFlexDirection` struct
#[repr(C)]
pub enum AzLayoutFlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Re-export of rust-allocated (stack based) `LayoutDisplay` struct
#[repr(C)]
pub enum AzLayoutDisplay {
    None,
    Flex,
    Block,
    InlineBlock,
}

/// Re-export of rust-allocated (stack based) `LayoutFloat` struct
#[repr(C)]
pub enum AzLayoutFloat {
    Left,
    Right,
}

/// Re-export of rust-allocated (stack based) `LayoutJustifyContent` struct
#[repr(C)]
pub enum AzLayoutJustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Re-export of rust-allocated (stack based) `LayoutPosition` struct
#[repr(C)]
pub enum AzLayoutPosition {
    Static,
    Relative,
    Absolute,
    Fixed,
}

/// Re-export of rust-allocated (stack based) `LayoutFlexWrap` struct
#[repr(C)]
pub enum AzLayoutFlexWrap {
    Wrap,
    NoWrap,
}

/// Re-export of rust-allocated (stack based) `LayoutOverflow` struct
#[repr(C)]
pub enum AzLayoutOverflow {
    Scroll,
    Auto,
    Hidden,
    Visible,
}

/// Re-export of rust-allocated (stack based) `AngleMetric` struct
#[repr(C)]
pub enum AzAngleMetric {
    Degree,
    Radians,
    Grad,
    Turn,
    Percent,
}

/// Re-export of rust-allocated (stack based) `DirectionCorner` struct
#[repr(C)]
pub enum AzDirectionCorner {
    Right,
    Left,
    Top,
    Bottom,
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

/// Re-export of rust-allocated (stack based) `ExtendMode` struct
#[repr(C)]
pub enum AzExtendMode {
    Clamp,
    Repeat,
}

/// Re-export of rust-allocated (stack based) `Shape` struct
#[repr(C)]
pub enum AzShape {
    Ellipse,
    Circle,
}

/// Re-export of rust-allocated (stack based) `RadialGradientSize` struct
#[repr(C)]
pub enum AzRadialGradientSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    FarthestCorner,
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundRepeat` struct
#[repr(C)]
pub enum AzStyleBackgroundRepeat {
    NoRepeat,
    Repeat,
    RepeatX,
    RepeatY,
}

/// Re-export of rust-allocated (stack based) `BorderStyle` struct
#[repr(C)]
pub enum AzBorderStyle {
    None,
    Solid,
    Double,
    Dotted,
    Dashed,
    Hidden,
    Groove,
    Ridge,
    Inset,
    Outset,
}

/// Re-export of rust-allocated (stack based) `StyleCursor` struct
#[repr(C)]
pub enum AzStyleCursor {
    Alias,
    AllScroll,
    Cell,
    ColResize,
    ContextMenu,
    Copy,
    Crosshair,
    Default,
    EResize,
    EwResize,
    Grab,
    Grabbing,
    Help,
    Move,
    NResize,
    NsResize,
    NeswResize,
    NwseResize,
    Pointer,
    Progress,
    RowResize,
    SResize,
    SeResize,
    Text,
    Unset,
    VerticalText,
    WResize,
    Wait,
    ZoomIn,
    ZoomOut,
}

/// Re-export of rust-allocated (stack based) `StyleBackfaceVisibility` struct
#[repr(C)]
pub enum AzStyleBackfaceVisibility {
    Hidden,
    Visible,
}

/// Re-export of rust-allocated (stack based) `StyleTextAlign` struct
#[repr(C)]
pub enum AzStyleTextAlign {
    Left,
    Center,
    Right,
}

/// Re-export of rust-allocated (stack based) `Node` struct
#[repr(C)]
#[pyclass(name = "Node")]
pub struct AzNode {
    pub parent: usize,
    pub previous_sibling: usize,
    pub next_sibling: usize,
    pub last_child: usize,
}

/// Re-export of rust-allocated (stack based) `CascadeInfo` struct
#[repr(C)]
#[pyclass(name = "CascadeInfo")]
pub struct AzCascadeInfo {
    pub index_in_parent: u32,
    pub is_last_child: bool,
}

/// Re-export of rust-allocated (stack based) `StyledNodeState` struct
#[repr(C)]
#[pyclass(name = "StyledNodeState")]
pub struct AzStyledNodeState {
    pub normal: bool,
    pub hover: bool,
    pub active: bool,
    pub focused: bool,
}

/// Re-export of rust-allocated (stack based) `TagId` struct
#[repr(C)]
#[pyclass(name = "TagId")]
pub struct AzTagId {
    pub inner: u64,
}

/// Re-export of rust-allocated (stack based) `CssPropertyCache` struct
#[repr(C)]
#[pyclass(name = "CssPropertyCache")]
pub struct AzCssPropertyCache {
    pub ptr: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `GlShaderPrecisionFormatReturn` struct
#[repr(C)]
#[pyclass(name = "GlShaderPrecisionFormatReturn")]
pub struct AzGlShaderPrecisionFormatReturn {
    pub _0: i32,
    pub _1: i32,
    pub _2: i32,
}

/// Re-export of rust-allocated (stack based) `VertexAttributeType` struct
#[repr(C)]
pub enum AzVertexAttributeType {
    Float,
    Double,
    UnsignedByte,
    UnsignedShort,
    UnsignedInt,
}

/// Re-export of rust-allocated (stack based) `IndexBufferFormat` struct
#[repr(C)]
pub enum AzIndexBufferFormat {
    Points,
    Lines,
    LineStrip,
    Triangles,
    TriangleStrip,
    TriangleFan,
}

/// Re-export of rust-allocated (stack based) `GlType` struct
#[repr(C)]
pub enum AzGlType {
    Gl,
    Gles,
}

/// C-ABI stable reexport of `&[u8]`
#[repr(C)]
#[pyclass(name = "U8VecRef")]
pub struct AzU8VecRef {
    pub ptr: *const u8,
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [u8]`
#[repr(C)]
#[pyclass(name = "U8VecRefMut")]
pub struct AzU8VecRefMut {
    pub ptr: *mut u8,
    pub len: usize,
}

/// C-ABI stable reexport of `&[f32]`
#[repr(C)]
#[pyclass(name = "F32VecRef")]
pub struct AzF32VecRef {
    pub ptr: *const f32,
    pub len: usize,
}

/// C-ABI stable reexport of `&[i32]`
#[repr(C)]
#[pyclass(name = "I32VecRef")]
pub struct AzI32VecRef {
    pub ptr: *const i32,
    pub len: usize,
}

/// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
#[repr(C)]
#[pyclass(name = "GLuintVecRef")]
pub struct AzGLuintVecRef {
    pub ptr: *const u32,
    pub len: usize,
}

/// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
#[repr(C)]
#[pyclass(name = "GLenumVecRef")]
pub struct AzGLenumVecRef {
    pub ptr: *const u32,
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
#[repr(C)]
#[pyclass(name = "GLintVecRefMut")]
pub struct AzGLintVecRefMut {
    pub ptr: *mut i32,
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
#[repr(C)]
#[pyclass(name = "GLint64VecRefMut")]
pub struct AzGLint64VecRefMut {
    pub ptr: *mut i64,
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
#[repr(C)]
#[pyclass(name = "GLbooleanVecRefMut")]
pub struct AzGLbooleanVecRefMut {
    pub ptr: *mut u8,
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
#[repr(C)]
#[pyclass(name = "GLfloatVecRefMut")]
pub struct AzGLfloatVecRefMut {
    pub ptr: *mut f32,
    pub len: usize,
}

/// C-ABI stable reexport of `&str`
#[repr(C)]
#[pyclass(name = "Refstr")]
pub struct AzRefstr {
    pub ptr: *const u8,
    pub len: usize,
}

/// C-ABI stable reexport of `*const gleam::gl::GLsync`
#[repr(C)]
#[pyclass(name = "GLsyncPtr")]
pub struct AzGLsyncPtr {
    pub ptr: *const c_void,
}

/// Re-export of rust-allocated (stack based) `TextureFlags` struct
#[repr(C)]
#[pyclass(name = "TextureFlags")]
pub struct AzTextureFlags {
    pub is_opaque: bool,
    pub is_video_texture: bool,
}

/// Re-export of rust-allocated (stack based) `ImageRef` struct
#[repr(C)]
#[pyclass(name = "ImageRef")]
pub struct AzImageRef {
    pub data: *const c_void,
    pub copies: *const c_void,
}

/// Re-export of rust-allocated (stack based) `RawImageFormat` struct
#[repr(C)]
pub enum AzRawImageFormat {
    R8,
    R16,
    RG16,
    BGRA8,
    RGBAF32,
    RG8,
    RGBAI32,
    RGBA8,
}

/// Re-export of rust-allocated (stack based) `EncodeImageError` struct
#[repr(C)]
pub enum AzEncodeImageError {
    InsufficientMemory,
    DimensionError,
    InvalidData,
    Unknown,
}

/// Re-export of rust-allocated (stack based) `DecodeImageError` struct
#[repr(C)]
pub enum AzDecodeImageError {
    InsufficientMemory,
    DimensionError,
    UnsupportedImageFormat,
    Unknown,
}

/// `AzParsedFontDestructorFnType` struct
pub type AzParsedFontDestructorFnType = extern "C" fn(&mut c_void);

/// Atomically reference-counted parsed font data
#[repr(C)]
#[pyclass(name = "FontRef")]
pub struct AzFontRef {
    pub data: *const c_void,
    pub copies: *const c_void,
}

/// Re-export of rust-allocated (stack based) `Svg` struct
#[repr(C)]
#[pyclass(name = "Svg")]
pub struct AzSvg {
    pub ptr: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `SvgXmlNode` struct
#[repr(C)]
#[pyclass(name = "SvgXmlNode")]
pub struct AzSvgXmlNode {
    pub ptr: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `SvgCircle` struct
#[repr(C)]
#[pyclass(name = "SvgCircle")]
pub struct AzSvgCircle {
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

/// Re-export of rust-allocated (stack based) `SvgPoint` struct
#[repr(C)]
#[pyclass(name = "SvgPoint")]
pub struct AzSvgPoint {
    pub x: f32,
    pub y: f32,
}

/// Re-export of rust-allocated (stack based) `SvgRect` struct
#[repr(C)]
#[pyclass(name = "SvgRect")]
pub struct AzSvgRect {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub radius_top_left: f32,
    pub radius_top_right: f32,
    pub radius_bottom_left: f32,
    pub radius_bottom_right: f32,
}

/// Re-export of rust-allocated (stack based) `SvgVertex` struct
#[repr(C)]
#[pyclass(name = "SvgVertex")]
pub struct AzSvgVertex {
    pub x: f32,
    pub y: f32,
}

/// Re-export of rust-allocated (stack based) `ShapeRendering` struct
#[repr(C)]
pub enum AzShapeRendering {
    OptimizeSpeed,
    CrispEdges,
    GeometricPrecision,
}

/// Re-export of rust-allocated (stack based) `TextRendering` struct
#[repr(C)]
pub enum AzTextRendering {
    OptimizeSpeed,
    OptimizeLegibility,
    GeometricPrecision,
}

/// Re-export of rust-allocated (stack based) `ImageRendering` struct
#[repr(C)]
pub enum AzImageRendering {
    OptimizeQuality,
    OptimizeSpeed,
}

/// Re-export of rust-allocated (stack based) `FontDatabase` struct
#[repr(C)]
pub enum AzFontDatabase {
    Empty,
    System,
}

/// Re-export of rust-allocated (stack based) `Indent` struct
#[repr(C, u8)]
pub enum AzIndent {
    None,
    Spaces(u8),
    Tabs,
}

/// Re-export of rust-allocated (stack based) `SvgFitTo` struct
#[repr(C, u8)]
pub enum AzSvgFitTo {
    Original,
    Width(u32),
    Height(u32),
    Zoom(f32),
}

/// Re-export of rust-allocated (stack based) `SvgFillRule` struct
#[repr(C)]
pub enum AzSvgFillRule {
    Winding,
    EvenOdd,
}

/// Re-export of rust-allocated (stack based) `SvgTransform` struct
#[repr(C)]
#[pyclass(name = "SvgTransform")]
pub struct AzSvgTransform {
    pub sx: f32,
    pub kx: f32,
    pub ky: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

/// Re-export of rust-allocated (stack based) `SvgLineJoin` struct
#[repr(C)]
pub enum AzSvgLineJoin {
    Miter,
    MiterClip,
    Round,
    Bevel,
}

/// Re-export of rust-allocated (stack based) `SvgLineCap` struct
#[repr(C)]
pub enum AzSvgLineCap {
    Butt,
    Square,
    Round,
}

/// Re-export of rust-allocated (stack based) `SvgDashPattern` struct
#[repr(C)]
#[pyclass(name = "SvgDashPattern")]
pub struct AzSvgDashPattern {
    pub offset: f32,
    pub length_1: f32,
    pub gap_1: f32,
    pub length_2: f32,
    pub gap_2: f32,
    pub length_3: f32,
    pub gap_3: f32,
}

/// Re-export of rust-allocated (stack based) `File` struct
#[repr(C)]
#[pyclass(name = "File")]
pub struct AzFile {
    pub ptr: *const c_void,
}

/// Re-export of rust-allocated (stack based) `MsgBox` struct
#[repr(C)]
#[pyclass(name = "MsgBox")]
pub struct AzMsgBox {
    pub _reserved: *mut c_void,
}

/// Type of message box icon
#[repr(C)]
pub enum AzMsgBoxIcon {
    Info,
    Warning,
    Error,
    Question,
}

/// Value returned from a yes / no message box
#[repr(C)]
pub enum AzMsgBoxYesNo {
    Yes,
    No,
}

/// Value returned from an ok / cancel message box
#[repr(C)]
pub enum AzMsgBoxOkCancel {
    Ok,
    Cancel,
}

/// File picker dialog
#[repr(C)]
#[pyclass(name = "FileDialog")]
pub struct AzFileDialog {
    pub _reserved: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `ColorPickerDialog` struct
#[repr(C)]
#[pyclass(name = "ColorPickerDialog")]
pub struct AzColorPickerDialog {
    pub _reserved: *mut c_void,
}

/// Connection to the system clipboard, on some systems this connection can be cached
#[repr(C)]
#[pyclass(name = "SystemClipboard")]
pub struct AzSystemClipboard {
    pub _native: *const c_void,
}

/// `AzInstantPtrCloneFnType` struct
pub type AzInstantPtrCloneFnType = extern "C" fn(&AzInstantPtr) -> AzInstantPtr;

/// Re-export of rust-allocated (stack based) `InstantPtrCloneFn` struct
#[repr(C)]
#[pyclass(name = "InstantPtrCloneFn")]
pub struct AzInstantPtrCloneFn {
    pub cb: AzInstantPtrCloneFnType,
}

/// `AzInstantPtrDestructorFnType` struct
pub type AzInstantPtrDestructorFnType = extern "C" fn(&mut AzInstantPtr);

/// Re-export of rust-allocated (stack based) `InstantPtrDestructorFn` struct
#[repr(C)]
#[pyclass(name = "InstantPtrDestructorFn")]
pub struct AzInstantPtrDestructorFn {
    pub cb: AzInstantPtrDestructorFnType,
}

/// Re-export of rust-allocated (stack based) `SystemTick` struct
#[repr(C)]
#[pyclass(name = "SystemTick")]
pub struct AzSystemTick {
    pub tick_counter: u64,
}

/// Re-export of rust-allocated (stack based) `SystemTimeDiff` struct
#[repr(C)]
#[pyclass(name = "SystemTimeDiff")]
pub struct AzSystemTimeDiff {
    pub secs: u64,
    pub nanos: u32,
}

/// Re-export of rust-allocated (stack based) `SystemTickDiff` struct
#[repr(C)]
#[pyclass(name = "SystemTickDiff")]
pub struct AzSystemTickDiff {
    pub tick_diff: u64,
}

/// Re-export of rust-allocated (stack based) `TimerId` struct
#[repr(C)]
#[pyclass(name = "TimerId")]
pub struct AzTimerId {
    pub id: usize,
}

/// Should a timer terminate or not - used to remove active timers
#[repr(C)]
pub enum AzTerminateTimer {
    Terminate,
    Continue,
}

/// Re-export of rust-allocated (stack based) `ThreadId` struct
#[repr(C)]
#[pyclass(name = "ThreadId")]
pub struct AzThreadId {
    pub id: usize,
}

/// `AzCreateThreadFnType` struct
pub type AzCreateThreadFnType = extern "C" fn(AzRefAny, AzRefAny, AzThreadCallback) -> AzThread;

/// Re-export of rust-allocated (stack based) `CreateThreadFn` struct
#[repr(C)]
#[pyclass(name = "CreateThreadFn")]
pub struct AzCreateThreadFn {
    pub cb: AzCreateThreadFnType,
}

/// `AzGetSystemTimeFnType` struct
pub type AzGetSystemTimeFnType = extern "C" fn() -> AzInstant;

/// Get the current system time, equivalent to `std::time::Instant::now()`, except it also works on systems that work with "ticks" instead of timers
#[repr(C)]
#[pyclass(name = "GetSystemTimeFn")]
pub struct AzGetSystemTimeFn {
    pub cb: AzGetSystemTimeFnType,
}

/// `AzCheckThreadFinishedFnType` struct
pub type AzCheckThreadFinishedFnType = extern "C" fn(&c_void) -> bool;

/// Function called to check if the thread has finished
#[repr(C)]
#[pyclass(name = "CheckThreadFinishedFn")]
pub struct AzCheckThreadFinishedFn {
    pub cb: AzCheckThreadFinishedFnType,
}

/// `AzLibrarySendThreadMsgFnType` struct
pub type AzLibrarySendThreadMsgFnType = extern "C" fn(&c_void, AzThreadSendMsg) -> bool;

/// Function to send a message to the thread
#[repr(C)]
#[pyclass(name = "LibrarySendThreadMsgFn")]
pub struct AzLibrarySendThreadMsgFn {
    pub cb: AzLibrarySendThreadMsgFnType,
}

/// `AzLibraryReceiveThreadMsgFnType` struct
pub type AzLibraryReceiveThreadMsgFnType = extern "C" fn(&c_void) -> AzOptionThreadReceiveMsg;

/// Function to receive a message from the thread
#[repr(C)]
#[pyclass(name = "LibraryReceiveThreadMsgFn")]
pub struct AzLibraryReceiveThreadMsgFn {
    pub cb: AzLibraryReceiveThreadMsgFnType,
}

/// `AzThreadRecvFnType` struct
pub type AzThreadRecvFnType = extern "C" fn(&c_void) -> AzOptionThreadSendMsg;

/// Function that the running `Thread` can call to receive messages from the main UI thread
#[repr(C)]
#[pyclass(name = "ThreadRecvFn")]
pub struct AzThreadRecvFn {
    pub cb: AzThreadRecvFnType,
}

/// `AzThreadSendFnType` struct
pub type AzThreadSendFnType = extern "C" fn(&c_void, AzThreadReceiveMsg) -> bool;

/// Function that the running `Thread` can call to receive messages from the main UI thread
#[repr(C)]
#[pyclass(name = "ThreadSendFn")]
pub struct AzThreadSendFn {
    pub cb: AzThreadSendFnType,
}

/// `AzThreadDestructorFnType` struct
pub type AzThreadDestructorFnType = extern "C" fn(&mut AzThread);

/// Destructor of the `Thread`
#[repr(C)]
#[pyclass(name = "ThreadDestructorFn")]
pub struct AzThreadDestructorFn {
    pub cb: AzThreadDestructorFnType,
}

/// `AzThreadReceiverDestructorFnType` struct
pub type AzThreadReceiverDestructorFnType = extern "C" fn(&mut AzThreadReceiver);

/// Destructor of the `ThreadReceiver`
#[repr(C)]
#[pyclass(name = "ThreadReceiverDestructorFn")]
pub struct AzThreadReceiverDestructorFn {
    pub cb: AzThreadReceiverDestructorFnType,
}

/// `AzThreadSenderDestructorFnType` struct
pub type AzThreadSenderDestructorFnType = extern "C" fn(&mut AzThreadSender);

/// Destructor of the `ThreadSender`
#[repr(C)]
#[pyclass(name = "ThreadSenderDestructorFn")]
pub struct AzThreadSenderDestructorFn {
    pub cb: AzThreadSenderDestructorFnType,
}

/// Re-export of rust-allocated (stack based) `StyleFontFamilyVecDestructor` struct
#[repr(C, u8)]
pub enum AzStyleFontFamilyVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStyleFontFamilyVecDestructorType),
}

/// `AzStyleFontFamilyVecDestructorType` struct
pub type AzStyleFontFamilyVecDestructorType = extern "C" fn(&mut AzStyleFontFamilyVec);

/// Re-export of rust-allocated (stack based) `TesselatedSvgNodeVecDestructor` struct
#[repr(C, u8)]
pub enum AzTesselatedSvgNodeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzTesselatedSvgNodeVecDestructorType),
}

/// `AzTesselatedSvgNodeVecDestructorType` struct
pub type AzTesselatedSvgNodeVecDestructorType = extern "C" fn(&mut AzTesselatedSvgNodeVec);

/// Re-export of rust-allocated (stack based) `XmlNodeVecDestructor` struct
#[repr(C, u8)]
pub enum AzXmlNodeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzXmlNodeVecDestructorType),
}

/// `AzXmlNodeVecDestructorType` struct
pub type AzXmlNodeVecDestructorType = extern "C" fn(&mut AzXmlNodeVec);

/// Re-export of rust-allocated (stack based) `FmtArgVecDestructor` struct
#[repr(C, u8)]
pub enum AzFmtArgVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzFmtArgVecDestructorType),
}

/// `AzFmtArgVecDestructorType` struct
pub type AzFmtArgVecDestructorType = extern "C" fn(&mut AzFmtArgVec);

/// Re-export of rust-allocated (stack based) `InlineLineVecDestructor` struct
#[repr(C, u8)]
pub enum AzInlineLineVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzInlineLineVecDestructorType),
}

/// `AzInlineLineVecDestructorType` struct
pub type AzInlineLineVecDestructorType = extern "C" fn(&mut AzInlineLineVec);

/// Re-export of rust-allocated (stack based) `InlineWordVecDestructor` struct
#[repr(C, u8)]
pub enum AzInlineWordVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzInlineWordVecDestructorType),
}

/// `AzInlineWordVecDestructorType` struct
pub type AzInlineWordVecDestructorType = extern "C" fn(&mut AzInlineWordVec);

/// Re-export of rust-allocated (stack based) `InlineGlyphVecDestructor` struct
#[repr(C, u8)]
pub enum AzInlineGlyphVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzInlineGlyphVecDestructorType),
}

/// `AzInlineGlyphVecDestructorType` struct
pub type AzInlineGlyphVecDestructorType = extern "C" fn(&mut AzInlineGlyphVec);

/// Re-export of rust-allocated (stack based) `InlineTextHitVecDestructor` struct
#[repr(C, u8)]
pub enum AzInlineTextHitVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzInlineTextHitVecDestructorType),
}

/// `AzInlineTextHitVecDestructorType` struct
pub type AzInlineTextHitVecDestructorType = extern "C" fn(&mut AzInlineTextHitVec);

/// Re-export of rust-allocated (stack based) `MonitorVecDestructor` struct
#[repr(C, u8)]
pub enum AzMonitorVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzMonitorVecDestructorType),
}

/// `AzMonitorVecDestructorType` struct
pub type AzMonitorVecDestructorType = extern "C" fn(&mut AzMonitorVec);

/// Re-export of rust-allocated (stack based) `VideoModeVecDestructor` struct
#[repr(C, u8)]
pub enum AzVideoModeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzVideoModeVecDestructorType),
}

/// `AzVideoModeVecDestructorType` struct
pub type AzVideoModeVecDestructorType = extern "C" fn(&mut AzVideoModeVec);

/// Re-export of rust-allocated (stack based) `DomVecDestructor` struct
#[repr(C, u8)]
pub enum AzDomVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzDomVecDestructorType),
}

/// `AzDomVecDestructorType` struct
pub type AzDomVecDestructorType = extern "C" fn(&mut AzDomVec);

/// Re-export of rust-allocated (stack based) `IdOrClassVecDestructor` struct
#[repr(C, u8)]
pub enum AzIdOrClassVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzIdOrClassVecDestructorType),
}

/// `AzIdOrClassVecDestructorType` struct
pub type AzIdOrClassVecDestructorType = extern "C" fn(&mut AzIdOrClassVec);

/// Re-export of rust-allocated (stack based) `NodeDataInlineCssPropertyVecDestructor` struct
#[repr(C, u8)]
pub enum AzNodeDataInlineCssPropertyVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzNodeDataInlineCssPropertyVecDestructorType),
}

/// `AzNodeDataInlineCssPropertyVecDestructorType` struct
pub type AzNodeDataInlineCssPropertyVecDestructorType = extern "C" fn(&mut AzNodeDataInlineCssPropertyVec);

/// Re-export of rust-allocated (stack based) `StyleBackgroundContentVecDestructor` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundContentVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStyleBackgroundContentVecDestructorType),
}

/// `AzStyleBackgroundContentVecDestructorType` struct
pub type AzStyleBackgroundContentVecDestructorType = extern "C" fn(&mut AzStyleBackgroundContentVec);

/// Re-export of rust-allocated (stack based) `StyleBackgroundPositionVecDestructor` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundPositionVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStyleBackgroundPositionVecDestructorType),
}

/// `AzStyleBackgroundPositionVecDestructorType` struct
pub type AzStyleBackgroundPositionVecDestructorType = extern "C" fn(&mut AzStyleBackgroundPositionVec);

/// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatVecDestructor` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundRepeatVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStyleBackgroundRepeatVecDestructorType),
}

/// `AzStyleBackgroundRepeatVecDestructorType` struct
pub type AzStyleBackgroundRepeatVecDestructorType = extern "C" fn(&mut AzStyleBackgroundRepeatVec);

/// Re-export of rust-allocated (stack based) `StyleBackgroundSizeVecDestructor` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundSizeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStyleBackgroundSizeVecDestructorType),
}

/// `AzStyleBackgroundSizeVecDestructorType` struct
pub type AzStyleBackgroundSizeVecDestructorType = extern "C" fn(&mut AzStyleBackgroundSizeVec);

/// Re-export of rust-allocated (stack based) `StyleTransformVecDestructor` struct
#[repr(C, u8)]
pub enum AzStyleTransformVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStyleTransformVecDestructorType),
}

/// `AzStyleTransformVecDestructorType` struct
pub type AzStyleTransformVecDestructorType = extern "C" fn(&mut AzStyleTransformVec);

/// Re-export of rust-allocated (stack based) `CssPropertyVecDestructor` struct
#[repr(C, u8)]
pub enum AzCssPropertyVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzCssPropertyVecDestructorType),
}

/// `AzCssPropertyVecDestructorType` struct
pub type AzCssPropertyVecDestructorType = extern "C" fn(&mut AzCssPropertyVec);

/// Re-export of rust-allocated (stack based) `SvgMultiPolygonVecDestructor` struct
#[repr(C, u8)]
pub enum AzSvgMultiPolygonVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzSvgMultiPolygonVecDestructorType),
}

/// `AzSvgMultiPolygonVecDestructorType` struct
pub type AzSvgMultiPolygonVecDestructorType = extern "C" fn(&mut AzSvgMultiPolygonVec);

/// Re-export of rust-allocated (stack based) `SvgPathVecDestructor` struct
#[repr(C, u8)]
pub enum AzSvgPathVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzSvgPathVecDestructorType),
}

/// `AzSvgPathVecDestructorType` struct
pub type AzSvgPathVecDestructorType = extern "C" fn(&mut AzSvgPathVec);

/// Re-export of rust-allocated (stack based) `VertexAttributeVecDestructor` struct
#[repr(C, u8)]
pub enum AzVertexAttributeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzVertexAttributeVecDestructorType),
}

/// `AzVertexAttributeVecDestructorType` struct
pub type AzVertexAttributeVecDestructorType = extern "C" fn(&mut AzVertexAttributeVec);

/// Re-export of rust-allocated (stack based) `SvgPathElementVecDestructor` struct
#[repr(C, u8)]
pub enum AzSvgPathElementVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzSvgPathElementVecDestructorType),
}

/// `AzSvgPathElementVecDestructorType` struct
pub type AzSvgPathElementVecDestructorType = extern "C" fn(&mut AzSvgPathElementVec);

/// Re-export of rust-allocated (stack based) `SvgVertexVecDestructor` struct
#[repr(C, u8)]
pub enum AzSvgVertexVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzSvgVertexVecDestructorType),
}

/// `AzSvgVertexVecDestructorType` struct
pub type AzSvgVertexVecDestructorType = extern "C" fn(&mut AzSvgVertexVec);

/// Re-export of rust-allocated (stack based) `U32VecDestructor` struct
#[repr(C, u8)]
pub enum AzU32VecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzU32VecDestructorType),
}

/// `AzU32VecDestructorType` struct
pub type AzU32VecDestructorType = extern "C" fn(&mut AzU32Vec);

/// Re-export of rust-allocated (stack based) `XWindowTypeVecDestructor` struct
#[repr(C, u8)]
pub enum AzXWindowTypeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzXWindowTypeVecDestructorType),
}

/// `AzXWindowTypeVecDestructorType` struct
pub type AzXWindowTypeVecDestructorType = extern "C" fn(&mut AzXWindowTypeVec);

/// Re-export of rust-allocated (stack based) `VirtualKeyCodeVecDestructor` struct
#[repr(C, u8)]
pub enum AzVirtualKeyCodeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzVirtualKeyCodeVecDestructorType),
}

/// `AzVirtualKeyCodeVecDestructorType` struct
pub type AzVirtualKeyCodeVecDestructorType = extern "C" fn(&mut AzVirtualKeyCodeVec);

/// Re-export of rust-allocated (stack based) `CascadeInfoVecDestructor` struct
#[repr(C, u8)]
pub enum AzCascadeInfoVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzCascadeInfoVecDestructorType),
}

/// `AzCascadeInfoVecDestructorType` struct
pub type AzCascadeInfoVecDestructorType = extern "C" fn(&mut AzCascadeInfoVec);

/// Re-export of rust-allocated (stack based) `ScanCodeVecDestructor` struct
#[repr(C, u8)]
pub enum AzScanCodeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzScanCodeVecDestructorType),
}

/// `AzScanCodeVecDestructorType` struct
pub type AzScanCodeVecDestructorType = extern "C" fn(&mut AzScanCodeVec);

/// Re-export of rust-allocated (stack based) `CssDeclarationVecDestructor` struct
#[repr(C, u8)]
pub enum AzCssDeclarationVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzCssDeclarationVecDestructorType),
}

/// `AzCssDeclarationVecDestructorType` struct
pub type AzCssDeclarationVecDestructorType = extern "C" fn(&mut AzCssDeclarationVec);

/// Re-export of rust-allocated (stack based) `CssPathSelectorVecDestructor` struct
#[repr(C, u8)]
pub enum AzCssPathSelectorVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzCssPathSelectorVecDestructorType),
}

/// `AzCssPathSelectorVecDestructorType` struct
pub type AzCssPathSelectorVecDestructorType = extern "C" fn(&mut AzCssPathSelectorVec);

/// Re-export of rust-allocated (stack based) `StylesheetVecDestructor` struct
#[repr(C, u8)]
pub enum AzStylesheetVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStylesheetVecDestructorType),
}

/// `AzStylesheetVecDestructorType` struct
pub type AzStylesheetVecDestructorType = extern "C" fn(&mut AzStylesheetVec);

/// Re-export of rust-allocated (stack based) `CssRuleBlockVecDestructor` struct
#[repr(C, u8)]
pub enum AzCssRuleBlockVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzCssRuleBlockVecDestructorType),
}

/// `AzCssRuleBlockVecDestructorType` struct
pub type AzCssRuleBlockVecDestructorType = extern "C" fn(&mut AzCssRuleBlockVec);

/// Re-export of rust-allocated (stack based) `F32VecDestructor` struct
#[repr(C, u8)]
pub enum AzF32VecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzF32VecDestructorType),
}

/// `AzF32VecDestructorType` struct
pub type AzF32VecDestructorType = extern "C" fn(&mut AzF32Vec);

/// Re-export of rust-allocated (stack based) `U16VecDestructor` struct
#[repr(C, u8)]
pub enum AzU16VecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzU16VecDestructorType),
}

/// `AzU16VecDestructorType` struct
pub type AzU16VecDestructorType = extern "C" fn(&mut AzU16Vec);

/// Re-export of rust-allocated (stack based) `U8VecDestructor` struct
#[repr(C, u8)]
pub enum AzU8VecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzU8VecDestructorType),
}

/// `AzU8VecDestructorType` struct
pub type AzU8VecDestructorType = extern "C" fn(&mut AzU8Vec);

/// Re-export of rust-allocated (stack based) `CallbackDataVecDestructor` struct
#[repr(C, u8)]
pub enum AzCallbackDataVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzCallbackDataVecDestructorType),
}

/// `AzCallbackDataVecDestructorType` struct
pub type AzCallbackDataVecDestructorType = extern "C" fn(&mut AzCallbackDataVec);

/// Re-export of rust-allocated (stack based) `DebugMessageVecDestructor` struct
#[repr(C, u8)]
pub enum AzDebugMessageVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzDebugMessageVecDestructorType),
}

/// `AzDebugMessageVecDestructorType` struct
pub type AzDebugMessageVecDestructorType = extern "C" fn(&mut AzDebugMessageVec);

/// Re-export of rust-allocated (stack based) `GLuintVecDestructor` struct
#[repr(C, u8)]
pub enum AzGLuintVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzGLuintVecDestructorType),
}

/// `AzGLuintVecDestructorType` struct
pub type AzGLuintVecDestructorType = extern "C" fn(&mut AzGLuintVec);

/// Re-export of rust-allocated (stack based) `GLintVecDestructor` struct
#[repr(C, u8)]
pub enum AzGLintVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzGLintVecDestructorType),
}

/// `AzGLintVecDestructorType` struct
pub type AzGLintVecDestructorType = extern "C" fn(&mut AzGLintVec);

/// Re-export of rust-allocated (stack based) `StringVecDestructor` struct
#[repr(C, u8)]
pub enum AzStringVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStringVecDestructorType),
}

/// `AzStringVecDestructorType` struct
pub type AzStringVecDestructorType = extern "C" fn(&mut AzStringVec);

/// Re-export of rust-allocated (stack based) `StringPairVecDestructor` struct
#[repr(C, u8)]
pub enum AzStringPairVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStringPairVecDestructorType),
}

/// `AzStringPairVecDestructorType` struct
pub type AzStringPairVecDestructorType = extern "C" fn(&mut AzStringPairVec);

/// Re-export of rust-allocated (stack based) `NormalizedLinearColorStopVecDestructor` struct
#[repr(C, u8)]
pub enum AzNormalizedLinearColorStopVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzNormalizedLinearColorStopVecDestructorType),
}

/// `AzNormalizedLinearColorStopVecDestructorType` struct
pub type AzNormalizedLinearColorStopVecDestructorType = extern "C" fn(&mut AzNormalizedLinearColorStopVec);

/// Re-export of rust-allocated (stack based) `NormalizedRadialColorStopVecDestructor` struct
#[repr(C, u8)]
pub enum AzNormalizedRadialColorStopVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzNormalizedRadialColorStopVecDestructorType),
}

/// `AzNormalizedRadialColorStopVecDestructorType` struct
pub type AzNormalizedRadialColorStopVecDestructorType = extern "C" fn(&mut AzNormalizedRadialColorStopVec);

/// Re-export of rust-allocated (stack based) `NodeIdVecDestructor` struct
#[repr(C, u8)]
pub enum AzNodeIdVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzNodeIdVecDestructorType),
}

/// `AzNodeIdVecDestructorType` struct
pub type AzNodeIdVecDestructorType = extern "C" fn(&mut AzNodeIdVec);

/// Re-export of rust-allocated (stack based) `NodeVecDestructor` struct
#[repr(C, u8)]
pub enum AzNodeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzNodeVecDestructorType),
}

/// `AzNodeVecDestructorType` struct
pub type AzNodeVecDestructorType = extern "C" fn(&mut AzNodeVec);

/// Re-export of rust-allocated (stack based) `StyledNodeVecDestructor` struct
#[repr(C, u8)]
pub enum AzStyledNodeVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzStyledNodeVecDestructorType),
}

/// `AzStyledNodeVecDestructorType` struct
pub type AzStyledNodeVecDestructorType = extern "C" fn(&mut AzStyledNodeVec);

/// Re-export of rust-allocated (stack based) `TagIdsToNodeIdsMappingVecDestructor` struct
#[repr(C, u8)]
pub enum AzTagIdsToNodeIdsMappingVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzTagIdsToNodeIdsMappingVecDestructorType),
}

/// `AzTagIdsToNodeIdsMappingVecDestructorType` struct
pub type AzTagIdsToNodeIdsMappingVecDestructorType = extern "C" fn(&mut AzTagIdsToNodeIdsMappingVec);

/// Re-export of rust-allocated (stack based) `ParentWithNodeDepthVecDestructor` struct
#[repr(C, u8)]
pub enum AzParentWithNodeDepthVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzParentWithNodeDepthVecDestructorType),
}

/// `AzParentWithNodeDepthVecDestructorType` struct
pub type AzParentWithNodeDepthVecDestructorType = extern "C" fn(&mut AzParentWithNodeDepthVec);

/// Re-export of rust-allocated (stack based) `NodeDataVecDestructor` struct
#[repr(C, u8)]
pub enum AzNodeDataVecDestructor {
    DefaultRust,
    NoDestructor,
    External(AzNodeDataVecDestructorType),
}

/// `AzNodeDataVecDestructorType` struct
pub type AzNodeDataVecDestructorType = extern "C" fn(&mut AzNodeDataVec);

/// Re-export of rust-allocated (stack based) `OptionI16` struct
#[repr(C, u8)]
pub enum AzOptionI16 {
    None,
    Some(i16),
}

/// Re-export of rust-allocated (stack based) `OptionU16` struct
#[repr(C, u8)]
pub enum AzOptionU16 {
    None,
    Some(u16),
}

/// Re-export of rust-allocated (stack based) `OptionU32` struct
#[repr(C, u8)]
pub enum AzOptionU32 {
    None,
    Some(u32),
}

/// Re-export of rust-allocated (stack based) `OptionHwndHandle` struct
#[repr(C, u8)]
pub enum AzOptionHwndHandle {
    None,
    Some(*mut c_void),
}

/// Re-export of rust-allocated (stack based) `OptionX11Visual` struct
#[repr(C, u8)]
pub enum AzOptionX11Visual {
    None,
    Some(*const c_void),
}

/// Re-export of rust-allocated (stack based) `OptionI32` struct
#[repr(C, u8)]
pub enum AzOptionI32 {
    None,
    Some(i32),
}

/// Re-export of rust-allocated (stack based) `OptionF32` struct
#[repr(C, u8)]
pub enum AzOptionF32 {
    None,
    Some(f32),
}

/// Option<char> but the char is a u32, for C FFI stability reasons
#[repr(C, u8)]
pub enum AzOptionChar {
    None,
    Some(u32),
}

/// Re-export of rust-allocated (stack based) `OptionUsize` struct
#[repr(C, u8)]
pub enum AzOptionUsize {
    None,
    Some(usize),
}

/// Re-export of rust-allocated (stack based) `SvgParseErrorPosition` struct
#[repr(C)]
#[pyclass(name = "SvgParseErrorPosition")]
pub struct AzSvgParseErrorPosition {
    pub row: u32,
    pub col: u32,
}

/// External system callbacks to get the system time or create / manage threads
#[repr(C)]
#[pyclass(name = "SystemCallbacks")]
pub struct AzSystemCallbacks {
    pub create_thread_fn: AzCreateThreadFn,
    pub get_system_time_fn: AzGetSystemTimeFn,
}

/// Force a specific renderer: note that azul will **crash** on startup if the `RendererOptions` are not satisfied.
#[repr(C)]
#[pyclass(name = "RendererOptions")]
pub struct AzRendererOptions {
    pub vsync: AzVsync,
    pub srgb: AzSrgb,
    pub hw_accel: AzHwAcceleration,
}

/// Represents a rectangle in physical pixels (integer units)
#[repr(C)]
#[pyclass(name = "LayoutRect")]
pub struct AzLayoutRect {
    pub origin: AzLayoutPoint,
    pub size: AzLayoutSize,
}

/// Raw platform handle, for integration in / with other toolkits and custom non-azul window extensions
#[repr(C, u8)]
pub enum AzRawWindowHandle {
    IOS(AzIOSHandle),
    MacOS(AzMacOSHandle),
    Xlib(AzXlibHandle),
    Xcb(AzXcbHandle),
    Wayland(AzWaylandHandle),
    Windows(AzWindowsHandle),
    Web(AzWebHandle),
    Android(AzAndroidHandle),
    Unsupported,
}

/// Logical rectangle area (can differ based on HiDPI settings). Usually this is what you'd want for hit-testing and positioning elements.
#[repr(C)]
#[pyclass(name = "LogicalRect")]
pub struct AzLogicalRect {
    pub origin: AzLogicalPosition,
    pub size: AzLogicalSize,
}

/// Symbolic accelerator key (ctrl, alt, shift)
#[repr(C, u8)]
pub enum AzAcceleratorKey {
    Ctrl,
    Alt,
    Shift,
    Key(AzVirtualKeyCode),
}

/// Current position of the mouse cursor, relative to the window. Set to `Uninitialized` on startup (gets initialized on the first frame).
#[repr(C, u8)]
pub enum AzCursorPosition {
    OutOfWindow,
    Uninitialized,
    InWindow(AzLogicalPosition),
}

/// Position of the top left corner of the window relative to the top left of the monitor
#[repr(C, u8)]
pub enum AzWindowPosition {
    Uninitialized,
    Initialized(AzPhysicalPositionI32),
}

/// Position of the virtual keyboard necessary to insert CJK characters
#[repr(C, u8)]
pub enum AzImePosition {
    Uninitialized,
    Initialized(AzLogicalPosition),
}

/// Describes a rendering configuration for a monitor
#[repr(C)]
#[pyclass(name = "VideoMode")]
pub struct AzVideoMode {
    pub size: AzLayoutSize,
    pub bit_depth: u16,
    pub refresh_rate: u16,
}

/// Combination of node ID + DOM ID, both together can identify a node
#[repr(C)]
#[pyclass(name = "DomNodeId")]
pub struct AzDomNodeId {
    pub dom: AzDomId,
    pub node: AzNodeId,
}

/// Re-export of rust-allocated (stack based) `PositionInfo` struct
#[repr(C, u8)]
pub enum AzPositionInfo {
    Static(AzPositionInfoInner),
    Fixed(AzPositionInfoInner),
    Absolute(AzPositionInfoInner),
    Relative(AzPositionInfoInner),
}

/// Re-export of rust-allocated (stack based) `HidpiAdjustedBounds` struct
#[repr(C)]
#[pyclass(name = "HidpiAdjustedBounds")]
pub struct AzHidpiAdjustedBounds {
    pub logical_size: AzLogicalSize,
    pub hidpi_factor: f32,
}

/// Re-export of rust-allocated (stack based) `InlineGlyph` struct
#[repr(C)]
#[pyclass(name = "InlineGlyph")]
pub struct AzInlineGlyph {
    pub bounds: AzLogicalRect,
    pub unicode_codepoint: AzOptionChar,
    pub glyph_index: u32,
}

/// Re-export of rust-allocated (stack based) `InlineTextHit` struct
#[repr(C)]
#[pyclass(name = "InlineTextHit")]
pub struct AzInlineTextHit {
    pub unicode_codepoint: AzOptionChar,
    pub hit_relative_to_inline_text: AzLogicalPosition,
    pub hit_relative_to_line: AzLogicalPosition,
    pub hit_relative_to_text_content: AzLogicalPosition,
    pub hit_relative_to_glyph: AzLogicalPosition,
    pub line_index_relative_to_text: usize,
    pub word_index_relative_to_text: usize,
    pub text_content_index_relative_to_text: usize,
    pub glyph_index_relative_to_text: usize,
    pub char_index_relative_to_text: usize,
    pub word_index_relative_to_line: usize,
    pub text_content_index_relative_to_line: usize,
    pub glyph_index_relative_to_line: usize,
    pub char_index_relative_to_line: usize,
    pub glyph_index_relative_to_word: usize,
    pub char_index_relative_to_word: usize,
}

/// Re-export of rust-allocated (stack based) `IFrameCallbackInfo` struct
#[repr(C)]
#[pyclass(name = "IFrameCallbackInfo")]
pub struct AzIFrameCallbackInfo {
    pub system_fonts: *const c_void,
    pub image_cache: *const c_void,
    pub window_theme: AzWindowTheme,
    pub bounds: AzHidpiAdjustedBounds,
    pub scroll_size: AzLogicalSize,
    pub scroll_offset: AzLogicalPosition,
    pub virtual_scroll_size: AzLogicalSize,
    pub virtual_scroll_offset: AzLogicalPosition,
    pub _reserved_ref: *const c_void,
    pub _reserved_mut: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `TimerCallbackReturn` struct
#[repr(C)]
#[pyclass(name = "TimerCallbackReturn")]
pub struct AzTimerCallbackReturn {
    pub should_update: AzUpdateScreen,
    pub should_terminate: AzTerminateTimer,
}

/// RefAny is a reference-counted, opaque pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
#[repr(C)]
#[pyclass(name = "RefAny")]
pub struct AzRefAny {
    pub _internal_ptr: *const c_void,
    pub sharing_info: AzRefCount,
}

/// Re-export of rust-allocated (stack based) `IFrameNode` struct
#[repr(C)]
#[pyclass(name = "IFrameNode")]
pub struct AzIFrameNode {
    pub callback: AzIFrameCallback,
    pub data: AzRefAny,
}

/// Re-export of rust-allocated (stack based) `NotEventFilter` struct
#[repr(C, u8)]
pub enum AzNotEventFilter {
    Hover(AzHoverEventFilter),
    Focus(AzFocusEventFilter),
}

/// Re-export of rust-allocated (stack based) `CssNthChildSelector` struct
#[repr(C, u8)]
pub enum AzCssNthChildSelector {
    Number(u32),
    Even,
    Odd,
    Pattern(AzCssNthChildPattern),
}

/// Re-export of rust-allocated (stack based) `PixelValue` struct
#[repr(C)]
#[pyclass(name = "PixelValue")]
pub struct AzPixelValue {
    pub metric: AzSizeMetric,
    pub number: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `PixelValueNoPercent` struct
#[repr(C)]
#[pyclass(name = "PixelValueNoPercent")]
pub struct AzPixelValueNoPercent {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBoxShadow` struct
#[repr(C)]
#[pyclass(name = "StyleBoxShadow")]
pub struct AzStyleBoxShadow {
    pub offset: [AzPixelValueNoPercent;2],
    pub color: AzColorU,
    pub blur_radius: AzPixelValueNoPercent,
    pub spread_radius: AzPixelValueNoPercent,
    pub clip_mode: AzBoxShadowClipMode,
}

/// Re-export of rust-allocated (stack based) `LayoutBottom` struct
#[repr(C)]
#[pyclass(name = "LayoutBottom")]
pub struct AzLayoutBottom {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutFlexGrow` struct
#[repr(C)]
#[pyclass(name = "LayoutFlexGrow")]
pub struct AzLayoutFlexGrow {
    pub inner: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `LayoutFlexShrink` struct
#[repr(C)]
#[pyclass(name = "LayoutFlexShrink")]
pub struct AzLayoutFlexShrink {
    pub inner: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `LayoutHeight` struct
#[repr(C)]
#[pyclass(name = "LayoutHeight")]
pub struct AzLayoutHeight {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutLeft` struct
#[repr(C)]
#[pyclass(name = "LayoutLeft")]
pub struct AzLayoutLeft {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginBottom` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginBottom")]
pub struct AzLayoutMarginBottom {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginLeft` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginLeft")]
pub struct AzLayoutMarginLeft {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginRight` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginRight")]
pub struct AzLayoutMarginRight {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginTop` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginTop")]
pub struct AzLayoutMarginTop {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMaxHeight` struct
#[repr(C)]
#[pyclass(name = "LayoutMaxHeight")]
pub struct AzLayoutMaxHeight {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMaxWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutMaxWidth")]
pub struct AzLayoutMaxWidth {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMinHeight` struct
#[repr(C)]
#[pyclass(name = "LayoutMinHeight")]
pub struct AzLayoutMinHeight {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMinWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutMinWidth")]
pub struct AzLayoutMinWidth {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingBottom` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingBottom")]
pub struct AzLayoutPaddingBottom {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingLeft` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingLeft")]
pub struct AzLayoutPaddingLeft {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingRight` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingRight")]
pub struct AzLayoutPaddingRight {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingTop` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingTop")]
pub struct AzLayoutPaddingTop {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutRight` struct
#[repr(C)]
#[pyclass(name = "LayoutRight")]
pub struct AzLayoutRight {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutTop` struct
#[repr(C)]
#[pyclass(name = "LayoutTop")]
pub struct AzLayoutTop {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutWidth")]
pub struct AzLayoutWidth {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `PercentageValue` struct
#[repr(C)]
#[pyclass(name = "PercentageValue")]
pub struct AzPercentageValue {
    pub number: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `AngleValue` struct
#[repr(C)]
#[pyclass(name = "AngleValue")]
pub struct AzAngleValue {
    pub metric: AzAngleMetric,
    pub number: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `NormalizedLinearColorStop` struct
#[repr(C)]
#[pyclass(name = "NormalizedLinearColorStop")]
pub struct AzNormalizedLinearColorStop {
    pub offset: AzPercentageValue,
    pub color: AzColorU,
}

/// Re-export of rust-allocated (stack based) `NormalizedRadialColorStop` struct
#[repr(C)]
#[pyclass(name = "NormalizedRadialColorStop")]
pub struct AzNormalizedRadialColorStop {
    pub offset: AzAngleValue,
    pub color: AzColorU,
}

/// Re-export of rust-allocated (stack based) `DirectionCorners` struct
#[repr(C)]
#[pyclass(name = "DirectionCorners")]
pub struct AzDirectionCorners {
    pub from: AzDirectionCorner,
    pub to: AzDirectionCorner,
}

/// Re-export of rust-allocated (stack based) `Direction` struct
#[repr(C, u8)]
pub enum AzDirection {
    Angle(AzAngleValue),
    FromTo(AzDirectionCorners),
}

/// Re-export of rust-allocated (stack based) `BackgroundPositionHorizontal` struct
#[repr(C, u8)]
pub enum AzBackgroundPositionHorizontal {
    Left,
    Center,
    Right,
    Exact(AzPixelValue),
}

/// Re-export of rust-allocated (stack based) `BackgroundPositionVertical` struct
#[repr(C, u8)]
pub enum AzBackgroundPositionVertical {
    Top,
    Center,
    Bottom,
    Exact(AzPixelValue),
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundPosition` struct
#[repr(C)]
#[pyclass(name = "StyleBackgroundPosition")]
pub struct AzStyleBackgroundPosition {
    pub horizontal: AzBackgroundPositionHorizontal,
    pub vertical: AzBackgroundPositionVertical,
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundSize` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundSize {
    ExactSize([AzPixelValue;2]),
    Contain,
    Cover,
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomColor` struct
#[repr(C)]
#[pyclass(name = "StyleBorderBottomColor")]
pub struct AzStyleBorderBottomColor {
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderBottomLeftRadius")]
pub struct AzStyleBorderBottomLeftRadius {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderBottomRightRadius")]
pub struct AzStyleBorderBottomRightRadius {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderBottomStyle")]
pub struct AzStyleBorderBottomStyle {
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderBottomWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderBottomWidth")]
pub struct AzLayoutBorderBottomWidth {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderLeftColor` struct
#[repr(C)]
#[pyclass(name = "StyleBorderLeftColor")]
pub struct AzStyleBorderLeftColor {
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderLeftStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderLeftStyle")]
pub struct AzStyleBorderLeftStyle {
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderLeftWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderLeftWidth")]
pub struct AzLayoutBorderLeftWidth {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderRightColor` struct
#[repr(C)]
#[pyclass(name = "StyleBorderRightColor")]
pub struct AzStyleBorderRightColor {
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderRightStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderRightStyle")]
pub struct AzStyleBorderRightStyle {
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderRightWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderRightWidth")]
pub struct AzLayoutBorderRightWidth {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopColor` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopColor")]
pub struct AzStyleBorderTopColor {
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopLeftRadius")]
pub struct AzStyleBorderTopLeftRadius {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopRightRadius")]
pub struct AzStyleBorderTopRightRadius {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopStyle")]
pub struct AzStyleBorderTopStyle {
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderTopWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderTopWidth")]
pub struct AzLayoutBorderTopWidth {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleFontSize` struct
#[repr(C)]
#[pyclass(name = "StyleFontSize")]
pub struct AzStyleFontSize {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleLetterSpacing` struct
#[repr(C)]
#[pyclass(name = "StyleLetterSpacing")]
pub struct AzStyleLetterSpacing {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleLineHeight` struct
#[repr(C)]
#[pyclass(name = "StyleLineHeight")]
pub struct AzStyleLineHeight {
    pub inner: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTabWidth` struct
#[repr(C)]
#[pyclass(name = "StyleTabWidth")]
pub struct AzStyleTabWidth {
    pub inner: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleOpacity` struct
#[repr(C)]
#[pyclass(name = "StyleOpacity")]
pub struct AzStyleOpacity {
    pub inner: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformOrigin` struct
#[repr(C)]
#[pyclass(name = "StyleTransformOrigin")]
pub struct AzStyleTransformOrigin {
    pub x: AzPixelValue,
    pub y: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StylePerspectiveOrigin` struct
#[repr(C)]
#[pyclass(name = "StylePerspectiveOrigin")]
pub struct AzStylePerspectiveOrigin {
    pub x: AzPixelValue,
    pub y: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformMatrix2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformMatrix2D")]
pub struct AzStyleTransformMatrix2D {
    pub a: AzPixelValue,
    pub b: AzPixelValue,
    pub c: AzPixelValue,
    pub d: AzPixelValue,
    pub tx: AzPixelValue,
    pub ty: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformMatrix3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformMatrix3D")]
pub struct AzStyleTransformMatrix3D {
    pub m11: AzPixelValue,
    pub m12: AzPixelValue,
    pub m13: AzPixelValue,
    pub m14: AzPixelValue,
    pub m21: AzPixelValue,
    pub m22: AzPixelValue,
    pub m23: AzPixelValue,
    pub m24: AzPixelValue,
    pub m31: AzPixelValue,
    pub m32: AzPixelValue,
    pub m33: AzPixelValue,
    pub m34: AzPixelValue,
    pub m41: AzPixelValue,
    pub m42: AzPixelValue,
    pub m43: AzPixelValue,
    pub m44: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformTranslate2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformTranslate2D")]
pub struct AzStyleTransformTranslate2D {
    pub x: AzPixelValue,
    pub y: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformTranslate3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformTranslate3D")]
pub struct AzStyleTransformTranslate3D {
    pub x: AzPixelValue,
    pub y: AzPixelValue,
    pub z: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformRotate3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformRotate3D")]
pub struct AzStyleTransformRotate3D {
    pub x: AzPercentageValue,
    pub y: AzPercentageValue,
    pub z: AzPercentageValue,
    pub angle: AzAngleValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformScale2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformScale2D")]
pub struct AzStyleTransformScale2D {
    pub x: AzPercentageValue,
    pub y: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformScale3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformScale3D")]
pub struct AzStyleTransformScale3D {
    pub x: AzPercentageValue,
    pub y: AzPercentageValue,
    pub z: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformSkew2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformSkew2D")]
pub struct AzStyleTransformSkew2D {
    pub x: AzPercentageValue,
    pub y: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTextColor` struct
#[repr(C)]
#[pyclass(name = "StyleTextColor")]
pub struct AzStyleTextColor {
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleWordSpacing` struct
#[repr(C)]
#[pyclass(name = "StyleWordSpacing")]
pub struct AzStyleWordSpacing {
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBoxShadowValue` struct
#[repr(C, u8)]
pub enum AzStyleBoxShadowValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBoxShadow),
}

/// Re-export of rust-allocated (stack based) `LayoutAlignContentValue` struct
#[repr(C, u8)]
pub enum AzLayoutAlignContentValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutAlignContent),
}

/// Re-export of rust-allocated (stack based) `LayoutAlignItemsValue` struct
#[repr(C, u8)]
pub enum AzLayoutAlignItemsValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutAlignItems),
}

/// Re-export of rust-allocated (stack based) `LayoutBottomValue` struct
#[repr(C, u8)]
pub enum AzLayoutBottomValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutBottom),
}

/// Re-export of rust-allocated (stack based) `LayoutBoxSizingValue` struct
#[repr(C, u8)]
pub enum AzLayoutBoxSizingValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutBoxSizing),
}

/// Re-export of rust-allocated (stack based) `LayoutFlexDirectionValue` struct
#[repr(C, u8)]
pub enum AzLayoutFlexDirectionValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutFlexDirection),
}

/// Re-export of rust-allocated (stack based) `LayoutDisplayValue` struct
#[repr(C, u8)]
pub enum AzLayoutDisplayValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutDisplay),
}

/// Re-export of rust-allocated (stack based) `LayoutFlexGrowValue` struct
#[repr(C, u8)]
pub enum AzLayoutFlexGrowValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutFlexGrow),
}

/// Re-export of rust-allocated (stack based) `LayoutFlexShrinkValue` struct
#[repr(C, u8)]
pub enum AzLayoutFlexShrinkValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutFlexShrink),
}

/// Re-export of rust-allocated (stack based) `LayoutFloatValue` struct
#[repr(C, u8)]
pub enum AzLayoutFloatValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutFloat),
}

/// Re-export of rust-allocated (stack based) `LayoutHeightValue` struct
#[repr(C, u8)]
pub enum AzLayoutHeightValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutHeight),
}

/// Re-export of rust-allocated (stack based) `LayoutJustifyContentValue` struct
#[repr(C, u8)]
pub enum AzLayoutJustifyContentValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutJustifyContent),
}

/// Re-export of rust-allocated (stack based) `LayoutLeftValue` struct
#[repr(C, u8)]
pub enum AzLayoutLeftValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutLeft),
}

/// Re-export of rust-allocated (stack based) `LayoutMarginBottomValue` struct
#[repr(C, u8)]
pub enum AzLayoutMarginBottomValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMarginBottom),
}

/// Re-export of rust-allocated (stack based) `LayoutMarginLeftValue` struct
#[repr(C, u8)]
pub enum AzLayoutMarginLeftValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMarginLeft),
}

/// Re-export of rust-allocated (stack based) `LayoutMarginRightValue` struct
#[repr(C, u8)]
pub enum AzLayoutMarginRightValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMarginRight),
}

/// Re-export of rust-allocated (stack based) `LayoutMarginTopValue` struct
#[repr(C, u8)]
pub enum AzLayoutMarginTopValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMarginTop),
}

/// Re-export of rust-allocated (stack based) `LayoutMaxHeightValue` struct
#[repr(C, u8)]
pub enum AzLayoutMaxHeightValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMaxHeight),
}

/// Re-export of rust-allocated (stack based) `LayoutMaxWidthValue` struct
#[repr(C, u8)]
pub enum AzLayoutMaxWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMaxWidth),
}

/// Re-export of rust-allocated (stack based) `LayoutMinHeightValue` struct
#[repr(C, u8)]
pub enum AzLayoutMinHeightValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMinHeight),
}

/// Re-export of rust-allocated (stack based) `LayoutMinWidthValue` struct
#[repr(C, u8)]
pub enum AzLayoutMinWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutMinWidth),
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingBottomValue` struct
#[repr(C, u8)]
pub enum AzLayoutPaddingBottomValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutPaddingBottom),
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingLeftValue` struct
#[repr(C, u8)]
pub enum AzLayoutPaddingLeftValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutPaddingLeft),
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingRightValue` struct
#[repr(C, u8)]
pub enum AzLayoutPaddingRightValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutPaddingRight),
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingTopValue` struct
#[repr(C, u8)]
pub enum AzLayoutPaddingTopValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutPaddingTop),
}

/// Re-export of rust-allocated (stack based) `LayoutPositionValue` struct
#[repr(C, u8)]
pub enum AzLayoutPositionValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutPosition),
}

/// Re-export of rust-allocated (stack based) `LayoutRightValue` struct
#[repr(C, u8)]
pub enum AzLayoutRightValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutRight),
}

/// Re-export of rust-allocated (stack based) `LayoutTopValue` struct
#[repr(C, u8)]
pub enum AzLayoutTopValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutTop),
}

/// Re-export of rust-allocated (stack based) `LayoutWidthValue` struct
#[repr(C, u8)]
pub enum AzLayoutWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutWidth),
}

/// Re-export of rust-allocated (stack based) `LayoutFlexWrapValue` struct
#[repr(C, u8)]
pub enum AzLayoutFlexWrapValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutFlexWrap),
}

/// Re-export of rust-allocated (stack based) `LayoutOverflowValue` struct
#[repr(C, u8)]
pub enum AzLayoutOverflowValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutOverflow),
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomColorValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderBottomColorValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderBottomColor),
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadiusValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderBottomLeftRadiusValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderBottomLeftRadius),
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadiusValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderBottomRightRadiusValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderBottomRightRadius),
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomStyleValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderBottomStyleValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderBottomStyle),
}

/// Re-export of rust-allocated (stack based) `LayoutBorderBottomWidthValue` struct
#[repr(C, u8)]
pub enum AzLayoutBorderBottomWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutBorderBottomWidth),
}

/// Re-export of rust-allocated (stack based) `StyleBorderLeftColorValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderLeftColorValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderLeftColor),
}

/// Re-export of rust-allocated (stack based) `StyleBorderLeftStyleValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderLeftStyleValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderLeftStyle),
}

/// Re-export of rust-allocated (stack based) `LayoutBorderLeftWidthValue` struct
#[repr(C, u8)]
pub enum AzLayoutBorderLeftWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutBorderLeftWidth),
}

/// Re-export of rust-allocated (stack based) `StyleBorderRightColorValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderRightColorValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderRightColor),
}

/// Re-export of rust-allocated (stack based) `StyleBorderRightStyleValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderRightStyleValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderRightStyle),
}

/// Re-export of rust-allocated (stack based) `LayoutBorderRightWidthValue` struct
#[repr(C, u8)]
pub enum AzLayoutBorderRightWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutBorderRightWidth),
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopColorValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderTopColorValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderTopColor),
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadiusValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderTopLeftRadiusValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderTopLeftRadius),
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadiusValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderTopRightRadiusValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderTopRightRadius),
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopStyleValue` struct
#[repr(C, u8)]
pub enum AzStyleBorderTopStyleValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBorderTopStyle),
}

/// Re-export of rust-allocated (stack based) `LayoutBorderTopWidthValue` struct
#[repr(C, u8)]
pub enum AzLayoutBorderTopWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzLayoutBorderTopWidth),
}

/// Re-export of rust-allocated (stack based) `StyleCursorValue` struct
#[repr(C, u8)]
pub enum AzStyleCursorValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleCursor),
}

/// Re-export of rust-allocated (stack based) `StyleFontSizeValue` struct
#[repr(C, u8)]
pub enum AzStyleFontSizeValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleFontSize),
}

/// Re-export of rust-allocated (stack based) `StyleLetterSpacingValue` struct
#[repr(C, u8)]
pub enum AzStyleLetterSpacingValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleLetterSpacing),
}

/// Re-export of rust-allocated (stack based) `StyleLineHeightValue` struct
#[repr(C, u8)]
pub enum AzStyleLineHeightValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleLineHeight),
}

/// Re-export of rust-allocated (stack based) `StyleTabWidthValue` struct
#[repr(C, u8)]
pub enum AzStyleTabWidthValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleTabWidth),
}

/// Re-export of rust-allocated (stack based) `StyleTextAlignValue` struct
#[repr(C, u8)]
pub enum AzStyleTextAlignValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleTextAlign),
}

/// Re-export of rust-allocated (stack based) `StyleTextColorValue` struct
#[repr(C, u8)]
pub enum AzStyleTextColorValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleTextColor),
}

/// Re-export of rust-allocated (stack based) `StyleWordSpacingValue` struct
#[repr(C, u8)]
pub enum AzStyleWordSpacingValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleWordSpacing),
}

/// Re-export of rust-allocated (stack based) `StyleOpacityValue` struct
#[repr(C, u8)]
pub enum AzStyleOpacityValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleOpacity),
}

/// Re-export of rust-allocated (stack based) `StyleTransformOriginValue` struct
#[repr(C, u8)]
pub enum AzStyleTransformOriginValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleTransformOrigin),
}

/// Re-export of rust-allocated (stack based) `StylePerspectiveOriginValue` struct
#[repr(C, u8)]
pub enum AzStylePerspectiveOriginValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStylePerspectiveOrigin),
}

/// Re-export of rust-allocated (stack based) `StyleBackfaceVisibilityValue` struct
#[repr(C, u8)]
pub enum AzStyleBackfaceVisibilityValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBackfaceVisibility),
}

/// Re-export of rust-allocated (stack based) `ParentWithNodeDepth` struct
#[repr(C)]
#[pyclass(name = "ParentWithNodeDepth")]
pub struct AzParentWithNodeDepth {
    pub depth: usize,
    pub node_id: AzNodeId,
}

/// Re-export of rust-allocated (stack based) `Gl` struct
#[repr(C)]
#[pyclass(name = "Gl")]
pub struct AzGl {
    pub ptr: *const c_void,
    pub svg_shader: u32,
    pub fxaa_shader: u32,
    pub renderer_type: AzRendererType,
}

/// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
#[repr(C)]
#[pyclass(name = "RefstrVecRef")]
pub struct AzRefstrVecRef {
    pub(crate) ptr: *const AzRefstr,
    pub len: usize,
}

/// Re-export of rust-allocated (stack based) `ImageMask` struct
#[repr(C)]
#[pyclass(name = "ImageMask")]
pub struct AzImageMask {
    pub image: AzImageRef,
    pub rect: AzLogicalRect,
    pub repeat: bool,
}

/// Re-export of rust-allocated (stack based) `FontMetrics` struct
#[repr(C)]
#[pyclass(name = "FontMetrics")]
pub struct AzFontMetrics {
    pub units_per_em: u16,
    pub font_flags: u16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
    pub min_left_side_bearing: i16,
    pub min_right_side_bearing: i16,
    pub x_max_extent: i16,
    pub caret_slope_rise: i16,
    pub caret_slope_run: i16,
    pub caret_offset: i16,
    pub num_h_metrics: u16,
    pub x_avg_char_width: i16,
    pub us_weight_class: u16,
    pub us_width_class: u16,
    pub fs_type: u16,
    pub y_subscript_x_size: i16,
    pub y_subscript_y_size: i16,
    pub y_subscript_x_offset: i16,
    pub y_subscript_y_offset: i16,
    pub y_superscript_x_size: i16,
    pub y_superscript_y_size: i16,
    pub y_superscript_x_offset: i16,
    pub y_superscript_y_offset: i16,
    pub y_strikeout_size: i16,
    pub y_strikeout_position: i16,
    pub s_family_class: i16,
    pub panose: [u8; 10],
    pub ul_unicode_range1: u32,
    pub ul_unicode_range2: u32,
    pub ul_unicode_range3: u32,
    pub ul_unicode_range4: u32,
    pub ach_vend_id: u32,
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,
    pub s_typo_ascender: AzOptionI16,
    pub s_typo_descender: AzOptionI16,
    pub s_typo_line_gap: AzOptionI16,
    pub us_win_ascent: AzOptionU16,
    pub us_win_descent: AzOptionU16,
    pub ul_code_page_range1: AzOptionU32,
    pub ul_code_page_range2: AzOptionU32,
    pub sx_height: AzOptionI16,
    pub s_cap_height: AzOptionI16,
    pub us_default_char: AzOptionU16,
    pub us_break_char: AzOptionU16,
    pub us_max_context: AzOptionU16,
    pub us_lower_optical_point_size: AzOptionU16,
    pub us_upper_optical_point_size: AzOptionU16,
}

/// Re-export of rust-allocated (stack based) `SvgLine` struct
#[repr(C)]
#[pyclass(name = "SvgLine")]
pub struct AzSvgLine {
    pub start: AzSvgPoint,
    pub end: AzSvgPoint,
}

/// Re-export of rust-allocated (stack based) `SvgQuadraticCurve` struct
#[repr(C)]
#[pyclass(name = "SvgQuadraticCurve")]
pub struct AzSvgQuadraticCurve {
    pub start: AzSvgPoint,
    pub ctrl: AzSvgPoint,
    pub end: AzSvgPoint,
}

/// Re-export of rust-allocated (stack based) `SvgCubicCurve` struct
#[repr(C)]
#[pyclass(name = "SvgCubicCurve")]
pub struct AzSvgCubicCurve {
    pub start: AzSvgPoint,
    pub ctrl_1: AzSvgPoint,
    pub ctrl_2: AzSvgPoint,
    pub end: AzSvgPoint,
}

/// Re-export of rust-allocated (stack based) `SvgStringFormatOptions` struct
#[repr(C)]
#[pyclass(name = "SvgStringFormatOptions")]
pub struct AzSvgStringFormatOptions {
    pub use_single_quote: bool,
    pub indent: AzIndent,
    pub attributes_indent: AzIndent,
}

/// Re-export of rust-allocated (stack based) `SvgFillStyle` struct
#[repr(C)]
#[pyclass(name = "SvgFillStyle")]
pub struct AzSvgFillStyle {
    pub line_join: AzSvgLineJoin,
    pub miter_limit: f32,
    pub tolerance: f32,
    pub fill_rule: AzSvgFillRule,
    pub transform: AzSvgTransform,
    pub anti_alias: bool,
    pub high_quality_aa: bool,
}

/// Re-export of rust-allocated (stack based) `InstantPtr` struct
#[repr(C)]
#[pyclass(name = "InstantPtr")]
pub struct AzInstantPtr {
    pub ptr: *const c_void,
    pub clone_fn: AzInstantPtrCloneFn,
    pub destructor: AzInstantPtrDestructorFn,
}

/// Re-export of rust-allocated (stack based) `Duration` struct
#[repr(C, u8)]
pub enum AzDuration {
    System(AzSystemTimeDiff),
    Tick(AzSystemTickDiff),
}

/// Re-export of rust-allocated (stack based) `Thread` struct
#[repr(C)]
#[pyclass(name = "Thread")]
pub struct AzThread {
    pub thread_handle: *const c_void,
    pub sender: *const c_void,
    pub receiver: *const c_void,
    pub dropcheck: *const c_void,
    pub writeback_data: AzRefAny,
    pub check_thread_finished_fn: AzCheckThreadFinishedFn,
    pub send_thread_msg_fn: AzLibrarySendThreadMsgFn,
    pub receive_thread_msg_fn: AzLibraryReceiveThreadMsgFn,
    pub thread_destructor_fn: AzThreadDestructorFn,
}

/// Re-export of rust-allocated (stack based) `ThreadSender` struct
#[repr(C)]
#[pyclass(name = "ThreadSender")]
pub struct AzThreadSender {
    pub ptr: *const c_void,
    pub send_fn: AzThreadSendFn,
    pub destructor: AzThreadSenderDestructorFn,
}

/// Re-export of rust-allocated (stack based) `ThreadReceiver` struct
#[repr(C)]
#[pyclass(name = "ThreadReceiver")]
pub struct AzThreadReceiver {
    pub ptr: *const c_void,
    pub recv_fn: AzThreadRecvFn,
    pub destructor: AzThreadReceiverDestructorFn,
}

/// Re-export of rust-allocated (stack based) `ThreadSendMsg` struct
#[repr(C, u8)]
pub enum AzThreadSendMsg {
    TerminateThread,
    Tick,
    Custom(AzRefAny),
}

/// Re-export of rust-allocated (stack based) `ThreadWriteBackMsg` struct
#[repr(C)]
#[pyclass(name = "ThreadWriteBackMsg")]
pub struct AzThreadWriteBackMsg {
    pub data: AzRefAny,
    pub callback: AzWriteBackCallback,
}

/// Wrapper over a Rust-allocated `Vec<XmlNode>`
#[repr(C)]
#[pyclass(name = "XmlNodeVec")]
pub struct AzXmlNodeVec {
    pub(crate) ptr: *const AzXmlNode,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzXmlNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<InlineGlyph>`
#[repr(C)]
#[pyclass(name = "InlineGlyphVec")]
pub struct AzInlineGlyphVec {
    pub(crate) ptr: *const AzInlineGlyph,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzInlineGlyphVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<InlineTextHit>`
#[repr(C)]
#[pyclass(name = "InlineTextHitVec")]
pub struct AzInlineTextHitVec {
    pub(crate) ptr: *const AzInlineTextHit,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzInlineTextHitVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<VideoMode>`
#[repr(C)]
#[pyclass(name = "VideoModeVec")]
pub struct AzVideoModeVec {
    pub(crate) ptr: *const AzVideoMode,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzVideoModeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<Dom>`
#[repr(C)]
#[pyclass(name = "DomVec")]
pub struct AzDomVec {
    pub(crate) ptr: *const AzDom,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzDomVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundPosition>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundPositionVec")]
pub struct AzStyleBackgroundPositionVec {
    pub(crate) ptr: *const AzStyleBackgroundPosition,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStyleBackgroundPositionVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundRepeat>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundRepeatVec")]
pub struct AzStyleBackgroundRepeatVec {
    pub(crate) ptr: *const AzStyleBackgroundRepeat,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStyleBackgroundRepeatVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundSize>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundSizeVec")]
pub struct AzStyleBackgroundSizeVec {
    pub(crate) ptr: *const AzStyleBackgroundSize,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStyleBackgroundSizeVecDestructor,
}

/// Wrapper over a Rust-allocated `SvgVertex`
#[repr(C)]
#[pyclass(name = "SvgVertexVec")]
pub struct AzSvgVertexVec {
    pub(crate) ptr: *const AzSvgVertex,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzSvgVertexVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<u32>`
#[repr(C)]
#[pyclass(name = "U32Vec")]
pub struct AzU32Vec {
    pub ptr: *const u32,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzU32VecDestructor,
}

/// Wrapper over a Rust-allocated `XWindowType`
#[repr(C)]
#[pyclass(name = "XWindowTypeVec")]
pub struct AzXWindowTypeVec {
    pub(crate) ptr: *const AzXWindowType,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzXWindowTypeVecDestructor,
}

/// Wrapper over a Rust-allocated `VirtualKeyCode`
#[repr(C)]
#[pyclass(name = "VirtualKeyCodeVec")]
pub struct AzVirtualKeyCodeVec {
    pub(crate) ptr: *const AzVirtualKeyCode,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzVirtualKeyCodeVecDestructor,
}

/// Wrapper over a Rust-allocated `CascadeInfo`
#[repr(C)]
#[pyclass(name = "CascadeInfoVec")]
pub struct AzCascadeInfoVec {
    pub(crate) ptr: *const AzCascadeInfo,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzCascadeInfoVecDestructor,
}

/// Wrapper over a Rust-allocated `ScanCode`
#[repr(C)]
#[pyclass(name = "ScanCodeVec")]
pub struct AzScanCodeVec {
    pub ptr: *const u32,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzScanCodeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<u16>`
#[repr(C)]
#[pyclass(name = "U16Vec")]
pub struct AzU16Vec {
    pub ptr: *const u16,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzU16VecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<f32>`
#[repr(C)]
#[pyclass(name = "F32Vec")]
pub struct AzF32Vec {
    pub ptr: *const f32,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzF32VecDestructor,
}

/// Wrapper over a Rust-allocated `U8Vec`
#[repr(C)]
#[pyclass(name = "U8Vec")]
pub struct AzU8Vec {
    pub ptr: *const u8,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzU8VecDestructor,
}

/// Wrapper over a Rust-allocated `U32Vec`
#[repr(C)]
#[pyclass(name = "GLuintVec")]
pub struct AzGLuintVec {
    pub ptr: *const u32,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzGLuintVecDestructor,
}

/// Wrapper over a Rust-allocated `GLintVec`
#[repr(C)]
#[pyclass(name = "GLintVec")]
pub struct AzGLintVec {
    pub ptr: *const i32,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzGLintVecDestructor,
}

/// Wrapper over a Rust-allocated `NormalizedLinearColorStopVec`
#[repr(C)]
#[pyclass(name = "NormalizedLinearColorStopVec")]
pub struct AzNormalizedLinearColorStopVec {
    pub(crate) ptr: *const AzNormalizedLinearColorStop,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzNormalizedLinearColorStopVecDestructor,
}

/// Wrapper over a Rust-allocated `NormalizedRadialColorStopVec`
#[repr(C)]
#[pyclass(name = "NormalizedRadialColorStopVec")]
pub struct AzNormalizedRadialColorStopVec {
    pub(crate) ptr: *const AzNormalizedRadialColorStop,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzNormalizedRadialColorStopVecDestructor,
}

/// Wrapper over a Rust-allocated `NodeIdVec`
#[repr(C)]
#[pyclass(name = "NodeIdVec")]
pub struct AzNodeIdVec {
    pub(crate) ptr: *const AzNodeId,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzNodeIdVecDestructor,
}

/// Wrapper over a Rust-allocated `NodeVec`
#[repr(C)]
#[pyclass(name = "NodeVec")]
pub struct AzNodeVec {
    pub(crate) ptr: *const AzNode,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `ParentWithNodeDepthVec`
#[repr(C)]
#[pyclass(name = "ParentWithNodeDepthVec")]
pub struct AzParentWithNodeDepthVec {
    pub(crate) ptr: *const AzParentWithNodeDepth,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzParentWithNodeDepthVecDestructor,
}

/// Re-export of rust-allocated (stack based) `OptionPositionInfo` struct
#[repr(C, u8)]
pub enum AzOptionPositionInfo {
    None,
    Some(AzPositionInfo),
}

/// Re-export of rust-allocated (stack based) `OptionTimerId` struct
#[repr(C, u8)]
pub enum AzOptionTimerId {
    None,
    Some(AzTimerId),
}

/// Re-export of rust-allocated (stack based) `OptionThreadId` struct
#[repr(C, u8)]
pub enum AzOptionThreadId {
    None,
    Some(AzThreadId),
}

/// Re-export of rust-allocated (stack based) `OptionImageRef` struct
#[repr(C, u8)]
pub enum AzOptionImageRef {
    None,
    Some(AzImageRef),
}

/// Re-export of rust-allocated (stack based) `OptionFontRef` struct
#[repr(C, u8)]
pub enum AzOptionFontRef {
    None,
    Some(AzFontRef),
}

/// Re-export of rust-allocated (stack based) `OptionSystemClipboard` struct
#[repr(C, u8)]
pub enum AzOptionSystemClipboard {
    None,
    Some(AzSystemClipboard),
}

/// Re-export of rust-allocated (stack based) `OptionFile` struct
#[repr(C, u8)]
pub enum AzOptionFile {
    None,
    Some(AzFile),
}

/// Re-export of rust-allocated (stack based) `OptionGl` struct
#[repr(C, u8)]
pub enum AzOptionGl {
    None,
    Some(AzGl),
}

/// Re-export of rust-allocated (stack based) `OptionPercentageValue` struct
#[repr(C, u8)]
pub enum AzOptionPercentageValue {
    None,
    Some(AzPercentageValue),
}

/// Re-export of rust-allocated (stack based) `OptionAngleValue` struct
#[repr(C, u8)]
pub enum AzOptionAngleValue {
    None,
    Some(AzAngleValue),
}

/// Re-export of rust-allocated (stack based) `OptionRendererOptions` struct
#[repr(C, u8)]
pub enum AzOptionRendererOptions {
    None,
    Some(AzRendererOptions),
}

/// Re-export of rust-allocated (stack based) `OptionCallback` struct
#[repr(C, u8)]
pub enum AzOptionCallback {
    None,
    Some(AzCallback),
}

/// Re-export of rust-allocated (stack based) `OptionThreadSendMsg` struct
#[repr(C, u8)]
pub enum AzOptionThreadSendMsg {
    None,
    Some(AzThreadSendMsg),
}

/// Re-export of rust-allocated (stack based) `OptionLayoutRect` struct
#[repr(C, u8)]
pub enum AzOptionLayoutRect {
    None,
    Some(AzLayoutRect),
}

/// Re-export of rust-allocated (stack based) `OptionRefAny` struct
#[repr(C, u8)]
pub enum AzOptionRefAny {
    None,
    Some(AzRefAny),
}

/// Re-export of rust-allocated (stack based) `OptionLayoutPoint` struct
#[repr(C, u8)]
pub enum AzOptionLayoutPoint {
    None,
    Some(AzLayoutPoint),
}

/// Re-export of rust-allocated (stack based) `OptionLayoutSize` struct
#[repr(C, u8)]
pub enum AzOptionLayoutSize {
    None,
    Some(AzLayoutSize),
}

/// Re-export of rust-allocated (stack based) `OptionWindowTheme` struct
#[repr(C, u8)]
pub enum AzOptionWindowTheme {
    None,
    Some(AzWindowTheme),
}

/// Re-export of rust-allocated (stack based) `OptionNodeId` struct
#[repr(C, u8)]
pub enum AzOptionNodeId {
    None,
    Some(AzNodeId),
}

/// Re-export of rust-allocated (stack based) `OptionDomNodeId` struct
#[repr(C, u8)]
pub enum AzOptionDomNodeId {
    None,
    Some(AzDomNodeId),
}

/// Re-export of rust-allocated (stack based) `OptionColorU` struct
#[repr(C, u8)]
pub enum AzOptionColorU {
    None,
    Some(AzColorU),
}

/// Re-export of rust-allocated (stack based) `OptionSvgDashPattern` struct
#[repr(C, u8)]
pub enum AzOptionSvgDashPattern {
    None,
    Some(AzSvgDashPattern),
}

/// Re-export of rust-allocated (stack based) `OptionLogicalPosition` struct
#[repr(C, u8)]
pub enum AzOptionLogicalPosition {
    None,
    Some(AzLogicalPosition),
}

/// Re-export of rust-allocated (stack based) `OptionPhysicalPositionI32` struct
#[repr(C, u8)]
pub enum AzOptionPhysicalPositionI32 {
    None,
    Some(AzPhysicalPositionI32),
}

/// Re-export of rust-allocated (stack based) `OptionMouseCursorType` struct
#[repr(C, u8)]
pub enum AzOptionMouseCursorType {
    None,
    Some(AzMouseCursorType),
}

/// Re-export of rust-allocated (stack based) `OptionLogicalSize` struct
#[repr(C, u8)]
pub enum AzOptionLogicalSize {
    None,
    Some(AzLogicalSize),
}

/// Re-export of rust-allocated (stack based) `OptionVirtualKeyCode` struct
#[repr(C, u8)]
pub enum AzOptionVirtualKeyCode {
    None,
    Some(AzVirtualKeyCode),
}

/// Re-export of rust-allocated (stack based) `OptionImageMask` struct
#[repr(C, u8)]
pub enum AzOptionImageMask {
    None,
    Some(AzImageMask),
}

/// Re-export of rust-allocated (stack based) `OptionTabIndex` struct
#[repr(C, u8)]
pub enum AzOptionTabIndex {
    None,
    Some(AzTabIndex),
}

/// Re-export of rust-allocated (stack based) `OptionTagId` struct
#[repr(C, u8)]
pub enum AzOptionTagId {
    None,
    Some(AzTagId),
}

/// Re-export of rust-allocated (stack based) `OptionDuration` struct
#[repr(C, u8)]
pub enum AzOptionDuration {
    None,
    Some(AzDuration),
}

/// Re-export of rust-allocated (stack based) `OptionU8Vec` struct
#[repr(C, u8)]
pub enum AzOptionU8Vec {
    None,
    Some(AzU8Vec),
}

/// Re-export of rust-allocated (stack based) `OptionU8VecRef` struct
#[repr(C, u8)]
pub enum AzOptionU8VecRef {
    None,
    Some(AzU8VecRef),
}

/// Re-export of rust-allocated (stack based) `ResultU8VecEncodeImageError` struct
#[repr(C, u8)]
pub enum AzResultU8VecEncodeImageError {
    Ok(AzU8Vec),
    Err(AzEncodeImageError),
}

/// Re-export of rust-allocated (stack based) `NonXmlCharError` struct
#[repr(C)]
#[pyclass(name = "NonXmlCharError")]
pub struct AzNonXmlCharError {
    pub ch: u32,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidCharError` struct
#[repr(C)]
#[pyclass(name = "InvalidCharError")]
pub struct AzInvalidCharError {
    pub expected: u8,
    pub got: u8,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidCharMultipleError` struct
#[repr(C)]
#[pyclass(name = "InvalidCharMultipleError")]
pub struct AzInvalidCharMultipleError {
    pub expected: u8,
    pub got: AzU8Vec,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidQuoteError` struct
#[repr(C)]
#[pyclass(name = "InvalidQuoteError")]
pub struct AzInvalidQuoteError {
    pub got: u8,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidSpaceError` struct
#[repr(C)]
#[pyclass(name = "InvalidSpaceError")]
pub struct AzInvalidSpaceError {
    pub got: u8,
    pub pos: AzSvgParseErrorPosition,
}

/// Configuration for optional features, such as whether to enable logging or panic hooks
#[repr(C)]
#[pyclass(name = "AppConfig")]
pub struct AzAppConfig {
    pub layout_solver: AzLayoutSolverVersion,
    pub log_level: AzAppLogLevel,
    pub enable_visual_panic_hook: bool,
    pub enable_logging_on_panic: bool,
    pub enable_tab_navigation: bool,
    pub system_callbacks: AzSystemCallbacks,
}

/// Small (16x16x4) window icon, usually shown in the window titlebar
#[repr(C)]
#[pyclass(name = "SmallWindowIconBytes")]
pub struct AzSmallWindowIconBytes {
    pub key: AzIconKey,
    pub rgba_bytes: AzU8Vec,
}

/// Large (32x32x4) window icon, usually used on high-resolution displays (instead of `SmallWindowIcon`)
#[repr(C)]
#[pyclass(name = "LargeWindowIconBytes")]
pub struct AzLargeWindowIconBytes {
    pub key: AzIconKey,
    pub rgba_bytes: AzU8Vec,
}

/// Window "favicon", usually shown in the top left of the window on Windows
#[repr(C, u8)]
pub enum AzWindowIcon {
    Small(AzSmallWindowIconBytes),
    Large(AzLargeWindowIconBytes),
}

/// Application taskbar icon, 256x256x4 bytes in size
#[repr(C)]
#[pyclass(name = "TaskBarIcon")]
pub struct AzTaskBarIcon {
    pub key: AzIconKey,
    pub rgba_bytes: AzU8Vec,
}

/// Minimum / maximum / current size of the window in logical dimensions
#[repr(C)]
#[pyclass(name = "WindowSize")]
pub struct AzWindowSize {
    pub dimensions: AzLogicalSize,
    pub hidpi_factor: f32,
    pub system_hidpi_factor: f32,
    pub min_dimensions: AzOptionLogicalSize,
    pub max_dimensions: AzOptionLogicalSize,
}

/// Current keyboard state, stores what keys / characters have been pressed
#[repr(C)]
#[pyclass(name = "KeyboardState")]
pub struct AzKeyboardState {
    pub shift_down: bool,
    pub ctrl_down: bool,
    pub alt_down: bool,
    pub super_down: bool,
    pub current_char: AzOptionChar,
    pub current_virtual_keycode: AzOptionVirtualKeyCode,
    pub pressed_virtual_keycodes: AzVirtualKeyCodeVec,
    pub pressed_scancodes: AzScanCodeVec,
}

/// Current mouse / cursor state
#[repr(C)]
#[pyclass(name = "MouseState")]
pub struct AzMouseState {
    pub mouse_cursor_type: AzOptionMouseCursorType,
    pub cursor_position: AzCursorPosition,
    pub is_cursor_locked: bool,
    pub left_down: bool,
    pub right_down: bool,
    pub middle_down: bool,
    pub scroll_x: AzOptionF32,
    pub scroll_y: AzOptionF32,
}

/// Re-export of rust-allocated (stack based) `InlineTextContents` struct
#[repr(C)]
#[pyclass(name = "InlineTextContents")]
pub struct AzInlineTextContents {
    pub glyphs: AzInlineGlyphVec,
    pub bounds: AzLogicalRect,
}

/// Easing function of the animation (ease-in, ease-out, ease-in-out, custom)
#[repr(C, u8)]
pub enum AzAnimationEasing {
    Ease,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(AzSvgCubicCurve),
}

/// Re-export of rust-allocated (stack based) `RenderImageCallbackInfo` struct
#[repr(C)]
#[pyclass(name = "RenderImageCallbackInfo")]
pub struct AzRenderImageCallbackInfo {
    pub callback_node_id: AzDomNodeId,
    pub bounds: AzHidpiAdjustedBounds,
    pub gl_context: *const AzOptionGl,
    pub image_cache: *const c_void,
    pub system_fonts: *const c_void,
    pub node_hierarchy: *const AzNodeVec,
    pub words_cache: *const c_void,
    pub shaped_words_cache: *const c_void,
    pub positioned_words_cache: *const c_void,
    pub positioned_rects: *const c_void,
    pub _reserved_ref: *const c_void,
    pub _reserved_mut: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `LayoutCallbackInfo` struct
#[repr(C)]
#[pyclass(name = "LayoutCallbackInfo")]
pub struct AzLayoutCallbackInfo {
    pub window_size: AzWindowSize,
    pub theme: AzWindowTheme,
    pub image_cache: *const c_void,
    pub gl_context: *const AzOptionGl,
    pub system_fonts: *const c_void,
    pub _reserved_ref: *const c_void,
    pub _reserved_mut: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `EventFilter` struct
#[repr(C, u8)]
pub enum AzEventFilter {
    Hover(AzHoverEventFilter),
    Not(AzNotEventFilter),
    Focus(AzFocusEventFilter),
    Window(AzWindowEventFilter),
    Component(AzComponentEventFilter),
    Application(AzApplicationEventFilter),
}

/// Re-export of rust-allocated (stack based) `CssPathPseudoSelector` struct
#[repr(C, u8)]
pub enum AzCssPathPseudoSelector {
    First,
    Last,
    NthChild(AzCssNthChildSelector),
    Hover,
    Active,
    Focus,
}

/// Re-export of rust-allocated (stack based) `AnimationInterpolationFunction` struct
#[repr(C, u8)]
pub enum AzAnimationInterpolationFunction {
    Ease,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(AzSvgCubicCurve),
}

/// Re-export of rust-allocated (stack based) `InterpolateContext` struct
#[repr(C)]
#[pyclass(name = "InterpolateContext")]
pub struct AzInterpolateContext {
    pub animation_func: AzAnimationInterpolationFunction,
    pub parent_rect_width: f32,
    pub parent_rect_height: f32,
    pub current_rect_width: f32,
    pub current_rect_height: f32,
}

/// Re-export of rust-allocated (stack based) `LinearGradient` struct
#[repr(C)]
#[pyclass(name = "LinearGradient")]
pub struct AzLinearGradient {
    pub direction: AzDirection,
    pub extend_mode: AzExtendMode,
    pub stops: AzNormalizedLinearColorStopVec,
}

/// Re-export of rust-allocated (stack based) `RadialGradient` struct
#[repr(C)]
#[pyclass(name = "RadialGradient")]
pub struct AzRadialGradient {
    pub shape: AzShape,
    pub size: AzRadialGradientSize,
    pub position: AzStyleBackgroundPosition,
    pub extend_mode: AzExtendMode,
    pub stops: AzNormalizedLinearColorStopVec,
}

/// Re-export of rust-allocated (stack based) `ConicGradient` struct
#[repr(C)]
#[pyclass(name = "ConicGradient")]
pub struct AzConicGradient {
    pub extend_mode: AzExtendMode,
    pub center: AzStyleBackgroundPosition,
    pub angle: AzAngleValue,
    pub stops: AzNormalizedRadialColorStopVec,
}

/// Re-export of rust-allocated (stack based) `StyleTransform` struct
#[repr(C, u8)]
pub enum AzStyleTransform {
    Matrix(AzStyleTransformMatrix2D),
    Matrix3D(AzStyleTransformMatrix3D),
    Translate(AzStyleTransformTranslate2D),
    Translate3D(AzStyleTransformTranslate3D),
    TranslateX(AzPixelValue),
    TranslateY(AzPixelValue),
    TranslateZ(AzPixelValue),
    Rotate(AzAngleValue),
    Rotate3D(AzStyleTransformRotate3D),
    RotateX(AzAngleValue),
    RotateY(AzAngleValue),
    RotateZ(AzAngleValue),
    Scale(AzStyleTransformScale2D),
    Scale3D(AzStyleTransformScale3D),
    ScaleX(AzPercentageValue),
    ScaleY(AzPercentageValue),
    ScaleZ(AzPercentageValue),
    Skew(AzStyleTransformSkew2D),
    SkewX(AzPercentageValue),
    SkewY(AzPercentageValue),
    Perspective(AzPixelValue),
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundPositionVecValue` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundPositionVecValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBackgroundPositionVec),
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatVecValue` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundRepeatVecValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBackgroundRepeatVec),
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundSizeVecValue` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundSizeVecValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBackgroundSizeVec),
}

/// Re-export of rust-allocated (stack based) `StyledNode` struct
#[repr(C)]
#[pyclass(name = "StyledNode")]
pub struct AzStyledNode {
    pub state: AzStyledNodeState,
    pub tag_id: AzOptionTagId,
}

/// Re-export of rust-allocated (stack based) `TagIdToNodeIdMapping` struct
#[repr(C)]
#[pyclass(name = "TagIdToNodeIdMapping")]
pub struct AzTagIdToNodeIdMapping {
    pub tag_id: AzTagId,
    pub node_id: AzNodeId,
    pub tab_index: AzOptionTabIndex,
    pub parents: AzNodeIdVec,
}

/// Re-export of rust-allocated (stack based) `Texture` struct
#[repr(C)]
#[pyclass(name = "Texture")]
pub struct AzTexture {
    pub texture_id: u32,
    pub format: AzRawImageFormat,
    pub flags: AzTextureFlags,
    pub size: AzPhysicalSizeU32,
    pub gl_context: AzGl,
}

/// C-ABI stable reexport of `(U8Vec, u32)`
#[repr(C)]
#[pyclass(name = "GetProgramBinaryReturn")]
pub struct AzGetProgramBinaryReturn {
    pub _0: AzU8Vec,
    pub _1: u32,
}

/// Re-export of rust-allocated (stack based) `RawImageData` struct
#[repr(C, u8)]
pub enum AzRawImageData {
    U8(AzU8Vec),
    U16(AzU16Vec),
    F32(AzF32Vec),
}

/// Source data of a font file (bytes)
#[repr(C)]
#[pyclass(name = "FontSource")]
pub struct AzFontSource {
    pub data: AzU8Vec,
    pub font_index: u32,
    pub parse_glyph_outlines: bool,
}

/// Re-export of rust-allocated (stack based) `SvgPathElement` struct
#[repr(C, u8)]
pub enum AzSvgPathElement {
    Line(AzSvgLine),
    QuadraticCurve(AzSvgQuadraticCurve),
    CubicCurve(AzSvgCubicCurve),
}

/// Re-export of rust-allocated (stack based) `TesselatedSvgNode` struct
#[repr(C)]
#[pyclass(name = "TesselatedSvgNode")]
pub struct AzTesselatedSvgNode {
    pub vertices: AzSvgVertexVec,
    pub indices: AzU32Vec,
}

/// Rust wrapper over a `&[TesselatedSvgNode]` or `&Vec<TesselatedSvgNode>`
#[repr(C)]
#[pyclass(name = "TesselatedSvgNodeVecRef")]
pub struct AzTesselatedSvgNodeVecRef {
    pub(crate) ptr: *const AzTesselatedSvgNode,
    pub len: usize,
}

/// Re-export of rust-allocated (stack based) `SvgRenderOptions` struct
#[repr(C)]
#[pyclass(name = "SvgRenderOptions")]
pub struct AzSvgRenderOptions {
    pub target_size: AzOptionLayoutSize,
    pub background_color: AzOptionColorU,
    pub fit: AzSvgFitTo,
}

/// Re-export of rust-allocated (stack based) `SvgStrokeStyle` struct
#[repr(C)]
#[pyclass(name = "SvgStrokeStyle")]
pub struct AzSvgStrokeStyle {
    pub start_cap: AzSvgLineCap,
    pub end_cap: AzSvgLineCap,
    pub line_join: AzSvgLineJoin,
    pub dash_pattern: AzOptionSvgDashPattern,
    pub line_width: f32,
    pub miter_limit: f32,
    pub tolerance: f32,
    pub apply_line_width: bool,
    pub transform: AzSvgTransform,
    pub anti_alias: bool,
    pub high_quality_aa: bool,
}

/// Re-export of rust-allocated (stack based) `Xml` struct
#[repr(C)]
#[pyclass(name = "Xml")]
pub struct AzXml {
    pub root: AzXmlNodeVec,
}

/// Re-export of rust-allocated (stack based) `Instant` struct
#[repr(C, u8)]
pub enum AzInstant {
    System(AzInstantPtr),
    Tick(AzSystemTick),
}

/// Re-export of rust-allocated (stack based) `ThreadReceiveMsg` struct
#[repr(C, u8)]
pub enum AzThreadReceiveMsg {
    WriteBack(AzThreadWriteBackMsg),
    Update(AzUpdateScreen),
}

/// Re-export of rust-allocated (stack based) `String` struct
#[repr(C)]
#[pyclass(name = "String")]
pub struct AzString {
    pub vec: AzU8Vec,
}

/// Wrapper over a Rust-allocated `Vec<TesselatedSvgNode>`
#[repr(C)]
#[pyclass(name = "TesselatedSvgNodeVec")]
pub struct AzTesselatedSvgNodeVec {
    pub(crate) ptr: *const AzTesselatedSvgNode,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzTesselatedSvgNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleTransform>`
#[repr(C)]
#[pyclass(name = "StyleTransformVec")]
pub struct AzStyleTransformVec {
    pub(crate) ptr: *const AzStyleTransform,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStyleTransformVecDestructor,
}

/// Wrapper over a Rust-allocated `VertexAttribute`
#[repr(C)]
#[pyclass(name = "SvgPathElementVec")]
pub struct AzSvgPathElementVec {
    pub(crate) ptr: *const AzSvgPathElement,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzSvgPathElementVecDestructor,
}

/// Wrapper over a Rust-allocated `StringVec`
#[repr(C)]
#[pyclass(name = "StringVec")]
pub struct AzStringVec {
    pub(crate) ptr: *const AzString,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStringVecDestructor,
}

/// Wrapper over a Rust-allocated `StyledNodeVec`
#[repr(C)]
#[pyclass(name = "StyledNodeVec")]
pub struct AzStyledNodeVec {
    pub(crate) ptr: *const AzStyledNode,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStyledNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `TagIdsToNodeIdsMappingVec`
#[repr(C)]
#[pyclass(name = "TagIdsToNodeIdsMappingVec")]
pub struct AzTagIdsToNodeIdsMappingVec {
    pub(crate) ptr: *const AzTagIdToNodeIdMapping,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzTagIdsToNodeIdsMappingVecDestructor,
}

/// Re-export of rust-allocated (stack based) `OptionMouseState` struct
#[repr(C, u8)]
pub enum AzOptionMouseState {
    None,
    Some(AzMouseState),
}

/// Re-export of rust-allocated (stack based) `OptionKeyboardState` struct
#[repr(C, u8)]
pub enum AzOptionKeyboardState {
    None,
    Some(AzKeyboardState),
}

/// Re-export of rust-allocated (stack based) `OptionStringVec` struct
#[repr(C, u8)]
pub enum AzOptionStringVec {
    None,
    Some(AzStringVec),
}

/// Re-export of rust-allocated (stack based) `OptionThreadReceiveMsg` struct
#[repr(C, u8)]
pub enum AzOptionThreadReceiveMsg {
    None,
    Some(AzThreadReceiveMsg),
}

/// Re-export of rust-allocated (stack based) `OptionTaskBarIcon` struct
#[repr(C, u8)]
pub enum AzOptionTaskBarIcon {
    None,
    Some(AzTaskBarIcon),
}

/// Re-export of rust-allocated (stack based) `OptionWindowIcon` struct
#[repr(C, u8)]
pub enum AzOptionWindowIcon {
    None,
    Some(AzWindowIcon),
}

/// Re-export of rust-allocated (stack based) `OptionString` struct
#[repr(C, u8)]
pub enum AzOptionString {
    None,
    Some(AzString),
}

/// Re-export of rust-allocated (stack based) `OptionTexture` struct
#[repr(C, u8)]
pub enum AzOptionTexture {
    None,
    Some(AzTexture),
}

/// Re-export of rust-allocated (stack based) `OptionInstant` struct
#[repr(C, u8)]
pub enum AzOptionInstant {
    None,
    Some(AzInstant),
}

/// Re-export of rust-allocated (stack based) `DuplicatedNamespaceError` struct
#[repr(C)]
#[pyclass(name = "DuplicatedNamespaceError")]
pub struct AzDuplicatedNamespaceError {
    pub ns: AzString,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `UnknownNamespaceError` struct
#[repr(C)]
#[pyclass(name = "UnknownNamespaceError")]
pub struct AzUnknownNamespaceError {
    pub ns: AzString,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `UnexpectedCloseTagError` struct
#[repr(C)]
#[pyclass(name = "UnexpectedCloseTagError")]
pub struct AzUnexpectedCloseTagError {
    pub expected: AzString,
    pub actual: AzString,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `UnknownEntityReferenceError` struct
#[repr(C)]
#[pyclass(name = "UnknownEntityReferenceError")]
pub struct AzUnknownEntityReferenceError {
    pub entity: AzString,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `DuplicatedAttributeError` struct
#[repr(C)]
#[pyclass(name = "DuplicatedAttributeError")]
pub struct AzDuplicatedAttributeError {
    pub attribute: AzString,
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidStringError` struct
#[repr(C)]
#[pyclass(name = "InvalidStringError")]
pub struct AzInvalidStringError {
    pub got: AzString,
    pub pos: AzSvgParseErrorPosition,
}

/// Window configuration specific to Win32
#[repr(C)]
#[pyclass(name = "WindowsWindowOptions")]
pub struct AzWindowsWindowOptions {
    pub allow_drag_drop: bool,
    pub no_redirection_bitmap: bool,
    pub window_icon: AzOptionWindowIcon,
    pub taskbar_icon: AzOptionTaskBarIcon,
    pub parent_window: AzOptionHwndHandle,
}

/// CSD theme of the window title / button controls
#[repr(C)]
#[pyclass(name = "WaylandTheme")]
pub struct AzWaylandTheme {
    pub title_bar_active_background_color: [u8;4],
    pub title_bar_active_separator_color: [u8;4],
    pub title_bar_active_text_color: [u8;4],
    pub title_bar_inactive_background_color: [u8;4],
    pub title_bar_inactive_separator_color: [u8;4],
    pub title_bar_inactive_text_color: [u8;4],
    pub maximize_idle_foreground_inactive_color: [u8;4],
    pub minimize_idle_foreground_inactive_color: [u8;4],
    pub close_idle_foreground_inactive_color: [u8;4],
    pub maximize_hovered_foreground_inactive_color: [u8;4],
    pub minimize_hovered_foreground_inactive_color: [u8;4],
    pub close_hovered_foreground_inactive_color: [u8;4],
    pub maximize_disabled_foreground_inactive_color: [u8;4],
    pub minimize_disabled_foreground_inactive_color: [u8;4],
    pub close_disabled_foreground_inactive_color: [u8;4],
    pub maximize_idle_background_inactive_color: [u8;4],
    pub minimize_idle_background_inactive_color: [u8;4],
    pub close_idle_background_inactive_color: [u8;4],
    pub maximize_hovered_background_inactive_color: [u8;4],
    pub minimize_hovered_background_inactive_color: [u8;4],
    pub close_hovered_background_inactive_color: [u8;4],
    pub maximize_disabled_background_inactive_color: [u8;4],
    pub minimize_disabled_background_inactive_color: [u8;4],
    pub close_disabled_background_inactive_color: [u8;4],
    pub maximize_idle_foreground_active_color: [u8;4],
    pub minimize_idle_foreground_active_color: [u8;4],
    pub close_idle_foreground_active_color: [u8;4],
    pub maximize_hovered_foreground_active_color: [u8;4],
    pub minimize_hovered_foreground_active_color: [u8;4],
    pub close_hovered_foreground_active_color: [u8;4],
    pub maximize_disabled_foreground_active_color: [u8;4],
    pub minimize_disabled_foreground_active_color: [u8;4],
    pub close_disabled_foreground_active_color: [u8;4],
    pub maximize_idle_background_active_color: [u8;4],
    pub minimize_idle_background_active_color: [u8;4],
    pub close_idle_background_active_color: [u8;4],
    pub maximize_hovered_background_active_color: [u8;4],
    pub minimize_hovered_background_active_color: [u8;4],
    pub close_hovered_background_active_color: [u8;4],
    pub maximize_disabled_background_active_color: [u8;4],
    pub minimize_disabled_background_active_color: [u8;4],
    pub close_disabled_background_active_color: [u8;4],
    pub title_bar_font: AzString,
    pub title_bar_font_size: f32,
}

/// Key-value pair, used for setting WM hints values specific to GNOME
#[repr(C)]
#[pyclass(name = "StringPair")]
pub struct AzStringPair {
    pub key: AzString,
    pub value: AzString,
}

/// Information about a single (or many) monitors, useful for dock widgets
#[repr(C)]
#[pyclass(name = "Monitor")]
pub struct AzMonitor {
    pub id: usize,
    pub name: AzOptionString,
    pub size: AzLayoutSize,
    pub position: AzLayoutPoint,
    pub scale_factor: f64,
    pub video_modes: AzVideoModeVec,
    pub is_primary_monitor: bool,
}

/// Re-export of rust-allocated (stack based) `InlineWord` struct
#[repr(C, u8)]
pub enum AzInlineWord {
    Tab,
    Return,
    Space,
    Word(AzInlineTextContents),
}

/// Re-export of rust-allocated (stack based) `CallbackData` struct
#[repr(C)]
#[pyclass(name = "CallbackData")]
pub struct AzCallbackData {
    pub event: AzEventFilter,
    pub callback: AzCallback,
    pub data: AzRefAny,
}

/// List of core DOM node types built-into by `azul`
#[repr(C, u8)]
pub enum AzNodeType {
    Body,
    Div,
    Br,
    Text(AzString),
    Image(AzImageRef),
    IFrame(AzIFrameNode),
}

/// Re-export of rust-allocated (stack based) `IdOrClass` struct
#[repr(C, u8)]
pub enum AzIdOrClass {
    Id(AzString),
    Class(AzString),
}

/// Re-export of rust-allocated (stack based) `CssPathSelector` struct
#[repr(C, u8)]
pub enum AzCssPathSelector {
    Global,
    Type(AzNodeTypeKey),
    Class(AzString),
    Id(AzString),
    PseudoSelector(AzCssPathPseudoSelector),
    DirectChildren,
    Children,
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundContent` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundContent {
    LinearGradient(AzLinearGradient),
    RadialGradient(AzRadialGradient),
    ConicGradient(AzConicGradient),
    Image(AzString),
    Color(AzColorU),
}

/// Re-export of rust-allocated (stack based) `ScrollbarInfo` struct
#[repr(C)]
#[pyclass(name = "ScrollbarInfo")]
pub struct AzScrollbarInfo {
    pub width: AzLayoutWidth,
    pub padding_left: AzLayoutPaddingLeft,
    pub padding_right: AzLayoutPaddingRight,
    pub track: AzStyleBackgroundContent,
    pub thumb: AzStyleBackgroundContent,
    pub button: AzStyleBackgroundContent,
    pub corner: AzStyleBackgroundContent,
    pub resizer: AzStyleBackgroundContent,
}

/// Re-export of rust-allocated (stack based) `ScrollbarStyle` struct
#[repr(C)]
#[pyclass(name = "ScrollbarStyle")]
pub struct AzScrollbarStyle {
    pub horizontal: AzScrollbarInfo,
    pub vertical: AzScrollbarInfo,
}

/// Re-export of rust-allocated (stack based) `StyleFontFamily` struct
#[repr(C, u8)]
pub enum AzStyleFontFamily {
    System(AzString),
    File(AzString),
    Ref(AzFontRef),
}

/// Re-export of rust-allocated (stack based) `ScrollbarStyleValue` struct
#[repr(C, u8)]
pub enum AzScrollbarStyleValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzScrollbarStyle),
}

/// Re-export of rust-allocated (stack based) `StyleTransformVecValue` struct
#[repr(C, u8)]
pub enum AzStyleTransformVecValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleTransformVec),
}

/// Re-export of rust-allocated (stack based) `VertexAttribute` struct
#[repr(C)]
#[pyclass(name = "VertexAttribute")]
pub struct AzVertexAttribute {
    pub name: AzString,
    pub layout_location: AzOptionUsize,
    pub attribute_type: AzVertexAttributeType,
    pub item_count: usize,
}

/// Re-export of rust-allocated (stack based) `DebugMessage` struct
#[repr(C)]
#[pyclass(name = "DebugMessage")]
pub struct AzDebugMessage {
    pub message: AzString,
    pub source: u32,
    pub ty: u32,
    pub id: u32,
    pub severity: u32,
}

/// C-ABI stable reexport of `(i32, u32, AzString)`
#[repr(C)]
#[pyclass(name = "GetActiveAttribReturn")]
pub struct AzGetActiveAttribReturn {
    pub _0: i32,
    pub _1: u32,
    pub _2: AzString,
}

/// C-ABI stable reexport of `(i32, u32, AzString)`
#[repr(C)]
#[pyclass(name = "GetActiveUniformReturn")]
pub struct AzGetActiveUniformReturn {
    pub _0: i32,
    pub _1: u32,
    pub _2: AzString,
}

/// Re-export of rust-allocated (stack based) `RawImage` struct
#[repr(C)]
#[pyclass(name = "RawImage")]
pub struct AzRawImage {
    pub pixels: AzRawImageData,
    pub width: usize,
    pub height: usize,
    pub alpha_premultiplied: bool,
    pub data_format: AzRawImageFormat,
}

/// Re-export of rust-allocated (stack based) `SvgPath` struct
#[repr(C)]
#[pyclass(name = "SvgPath")]
pub struct AzSvgPath {
    pub items: AzSvgPathElementVec,
}

/// Re-export of rust-allocated (stack based) `SvgParseOptions` struct
#[repr(C)]
#[pyclass(name = "SvgParseOptions")]
pub struct AzSvgParseOptions {
    pub relative_image_path: AzOptionString,
    pub dpi: f32,
    pub default_font_family: AzString,
    pub font_size: f32,
    pub languages: AzStringVec,
    pub shape_rendering: AzShapeRendering,
    pub text_rendering: AzTextRendering,
    pub image_rendering: AzImageRendering,
    pub keep_named_groups: bool,
    pub fontdb: AzFontDatabase,
}

/// Re-export of rust-allocated (stack based) `SvgStyle` struct
#[repr(C, u8)]
pub enum AzSvgStyle {
    Fill(AzSvgFillStyle),
    Stroke(AzSvgStrokeStyle),
}

/// Re-export of rust-allocated (stack based) `FileTypeList` struct
#[repr(C)]
#[pyclass(name = "FileTypeList")]
pub struct AzFileTypeList {
    pub document_types: AzStringVec,
    pub document_descriptor: AzString,
}

/// Re-export of rust-allocated (stack based) `Timer` struct
#[repr(C)]
#[pyclass(name = "Timer")]
pub struct AzTimer {
    pub data: AzRefAny,
    pub node_id: AzOptionDomNodeId,
    pub created: AzInstant,
    pub last_run: AzOptionInstant,
    pub run_count: usize,
    pub delay: AzOptionDuration,
    pub interval: AzOptionDuration,
    pub timeout: AzOptionDuration,
    pub callback: AzTimerCallback,
}

/// Re-export of rust-allocated (stack based) `FmtValue` struct
#[repr(C, u8)]
pub enum AzFmtValue {
    Bool(bool),
    Uchar(u8),
    Schar(i8),
    Ushort(u16),
    Sshort(i16),
    Uint(u32),
    Sint(i32),
    Ulong(u64),
    Slong(i64),
    Isize(isize),
    Usize(usize),
    Float(f32),
    Double(f64),
    Str(AzString),
    StrVec(AzStringVec),
}

/// Re-export of rust-allocated (stack based) `FmtArg` struct
#[repr(C)]
#[pyclass(name = "FmtArg")]
pub struct AzFmtArg {
    pub key: AzString,
    pub value: AzFmtValue,
}

/// Wrapper over a Rust-allocated `Vec<StyleFontFamily>`
#[repr(C)]
#[pyclass(name = "StyleFontFamilyVec")]
pub struct AzStyleFontFamilyVec {
    pub(crate) ptr: *const AzStyleFontFamily,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStyleFontFamilyVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<FmtArg>`
#[repr(C)]
#[pyclass(name = "FmtArgVec")]
pub struct AzFmtArgVec {
    pub(crate) ptr: *const AzFmtArg,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzFmtArgVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<InlineWord>`
#[repr(C)]
#[pyclass(name = "InlineWordVec")]
pub struct AzInlineWordVec {
    pub(crate) ptr: *const AzInlineWord,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzInlineWordVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<Monitor>`
#[repr(C)]
#[pyclass(name = "MonitorVec")]
pub struct AzMonitorVec {
    pub(crate) ptr: *const AzMonitor,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzMonitorVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<IdOrClass>`
#[repr(C)]
#[pyclass(name = "IdOrClassVec")]
pub struct AzIdOrClassVec {
    pub(crate) ptr: *const AzIdOrClass,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzIdOrClassVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundContent>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundContentVec")]
pub struct AzStyleBackgroundContentVec {
    pub(crate) ptr: *const AzStyleBackgroundContent,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStyleBackgroundContentVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<SvgPath>`
#[repr(C)]
#[pyclass(name = "SvgPathVec")]
pub struct AzSvgPathVec {
    pub(crate) ptr: *const AzSvgPath,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzSvgPathVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<VertexAttribute>`
#[repr(C)]
#[pyclass(name = "VertexAttributeVec")]
pub struct AzVertexAttributeVec {
    pub(crate) ptr: *const AzVertexAttribute,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzVertexAttributeVecDestructor,
}

/// Wrapper over a Rust-allocated `CssPathSelector`
#[repr(C)]
#[pyclass(name = "CssPathSelectorVec")]
pub struct AzCssPathSelectorVec {
    pub(crate) ptr: *const AzCssPathSelector,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzCssPathSelectorVecDestructor,
}

/// Wrapper over a Rust-allocated `CallbackData`
#[repr(C)]
#[pyclass(name = "CallbackDataVec")]
pub struct AzCallbackDataVec {
    pub(crate) ptr: *const AzCallbackData,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzCallbackDataVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<DebugMessage>`
#[repr(C)]
#[pyclass(name = "DebugMessageVec")]
pub struct AzDebugMessageVec {
    pub(crate) ptr: *const AzDebugMessage,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzDebugMessageVecDestructor,
}

/// Wrapper over a Rust-allocated `StringPairVec`
#[repr(C)]
#[pyclass(name = "StringPairVec")]
pub struct AzStringPairVec {
    pub(crate) ptr: *const AzStringPair,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStringPairVecDestructor,
}

/// Re-export of rust-allocated (stack based) `OptionFileTypeList` struct
#[repr(C, u8)]
pub enum AzOptionFileTypeList {
    None,
    Some(AzFileTypeList),
}

/// Re-export of rust-allocated (stack based) `OptionRawImage` struct
#[repr(C, u8)]
pub enum AzOptionRawImage {
    None,
    Some(AzRawImage),
}

/// Re-export of rust-allocated (stack based) `OptionWaylandTheme` struct
#[repr(C, u8)]
pub enum AzOptionWaylandTheme {
    None,
    Some(AzWaylandTheme),
}

/// Re-export of rust-allocated (stack based) `ResultRawImageDecodeImageError` struct
#[repr(C, u8)]
pub enum AzResultRawImageDecodeImageError {
    Ok(AzRawImage),
    Err(AzDecodeImageError),
}

/// Re-export of rust-allocated (stack based) `XmlStreamError` struct
#[repr(C, u8)]
pub enum AzXmlStreamError {
    UnexpectedEndOfStream,
    InvalidName,
    NonXmlChar(AzNonXmlCharError),
    InvalidChar(AzInvalidCharError),
    InvalidCharMultiple(AzInvalidCharMultipleError),
    InvalidQuote(AzInvalidQuoteError),
    InvalidSpace(AzInvalidSpaceError),
    InvalidString(AzInvalidStringError),
    InvalidReference,
    InvalidExternalID,
    InvalidCommentData,
    InvalidCommentEnd,
    InvalidCharacterData,
}

/// Re-export of rust-allocated (stack based) `LinuxWindowOptions` struct
#[repr(C)]
#[pyclass(name = "LinuxWindowOptions")]
pub struct AzLinuxWindowOptions {
    pub x11_visual: AzOptionX11Visual,
    pub x11_screen: AzOptionI32,
    pub x11_wm_classes: AzStringPairVec,
    pub x11_override_redirect: bool,
    pub x11_window_types: AzXWindowTypeVec,
    pub x11_gtk_theme_variant: AzOptionString,
    pub x11_resize_increments: AzOptionLogicalSize,
    pub x11_base_size: AzOptionLogicalSize,
    pub wayland_app_id: AzOptionString,
    pub wayland_theme: AzOptionWaylandTheme,
    pub request_user_attention: bool,
    pub window_icon: AzOptionWindowIcon,
}

/// Re-export of rust-allocated (stack based) `InlineLine` struct
#[repr(C)]
#[pyclass(name = "InlineLine")]
pub struct AzInlineLine {
    pub words: AzInlineWordVec,
    pub bounds: AzLogicalRect,
}

/// Re-export of rust-allocated (stack based) `CssPath` struct
#[repr(C)]
#[pyclass(name = "CssPath")]
pub struct AzCssPath {
    pub selectors: AzCssPathSelectorVec,
}

/// Re-export of rust-allocated (stack based) `StyleBackgroundContentVecValue` struct
#[repr(C, u8)]
pub enum AzStyleBackgroundContentVecValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleBackgroundContentVec),
}

/// Re-export of rust-allocated (stack based) `StyleFontFamilyVecValue` struct
#[repr(C, u8)]
pub enum AzStyleFontFamilyVecValue {
    Auto,
    None,
    Inherit,
    Initial,
    Exact(AzStyleFontFamilyVec),
}

/// Parsed CSS key-value pair
#[repr(C, u8)]
pub enum AzCssProperty {
    TextColor(AzStyleTextColorValue),
    FontSize(AzStyleFontSizeValue),
    FontFamily(AzStyleFontFamilyVecValue),
    TextAlign(AzStyleTextAlignValue),
    LetterSpacing(AzStyleLetterSpacingValue),
    LineHeight(AzStyleLineHeightValue),
    WordSpacing(AzStyleWordSpacingValue),
    TabWidth(AzStyleTabWidthValue),
    Cursor(AzStyleCursorValue),
    Display(AzLayoutDisplayValue),
    Float(AzLayoutFloatValue),
    BoxSizing(AzLayoutBoxSizingValue),
    Width(AzLayoutWidthValue),
    Height(AzLayoutHeightValue),
    MinWidth(AzLayoutMinWidthValue),
    MinHeight(AzLayoutMinHeightValue),
    MaxWidth(AzLayoutMaxWidthValue),
    MaxHeight(AzLayoutMaxHeightValue),
    Position(AzLayoutPositionValue),
    Top(AzLayoutTopValue),
    Right(AzLayoutRightValue),
    Left(AzLayoutLeftValue),
    Bottom(AzLayoutBottomValue),
    FlexWrap(AzLayoutFlexWrapValue),
    FlexDirection(AzLayoutFlexDirectionValue),
    FlexGrow(AzLayoutFlexGrowValue),
    FlexShrink(AzLayoutFlexShrinkValue),
    JustifyContent(AzLayoutJustifyContentValue),
    AlignItems(AzLayoutAlignItemsValue),
    AlignContent(AzLayoutAlignContentValue),
    BackgroundContent(AzStyleBackgroundContentVecValue),
    BackgroundPosition(AzStyleBackgroundPositionVecValue),
    BackgroundSize(AzStyleBackgroundSizeVecValue),
    BackgroundRepeat(AzStyleBackgroundRepeatVecValue),
    OverflowX(AzLayoutOverflowValue),
    OverflowY(AzLayoutOverflowValue),
    PaddingTop(AzLayoutPaddingTopValue),
    PaddingLeft(AzLayoutPaddingLeftValue),
    PaddingRight(AzLayoutPaddingRightValue),
    PaddingBottom(AzLayoutPaddingBottomValue),
    MarginTop(AzLayoutMarginTopValue),
    MarginLeft(AzLayoutMarginLeftValue),
    MarginRight(AzLayoutMarginRightValue),
    MarginBottom(AzLayoutMarginBottomValue),
    BorderTopLeftRadius(AzStyleBorderTopLeftRadiusValue),
    BorderTopRightRadius(AzStyleBorderTopRightRadiusValue),
    BorderBottomLeftRadius(AzStyleBorderBottomLeftRadiusValue),
    BorderBottomRightRadius(AzStyleBorderBottomRightRadiusValue),
    BorderTopColor(AzStyleBorderTopColorValue),
    BorderRightColor(AzStyleBorderRightColorValue),
    BorderLeftColor(AzStyleBorderLeftColorValue),
    BorderBottomColor(AzStyleBorderBottomColorValue),
    BorderTopStyle(AzStyleBorderTopStyleValue),
    BorderRightStyle(AzStyleBorderRightStyleValue),
    BorderLeftStyle(AzStyleBorderLeftStyleValue),
    BorderBottomStyle(AzStyleBorderBottomStyleValue),
    BorderTopWidth(AzLayoutBorderTopWidthValue),
    BorderRightWidth(AzLayoutBorderRightWidthValue),
    BorderLeftWidth(AzLayoutBorderLeftWidthValue),
    BorderBottomWidth(AzLayoutBorderBottomWidthValue),
    BoxShadowLeft(AzStyleBoxShadowValue),
    BoxShadowRight(AzStyleBoxShadowValue),
    BoxShadowTop(AzStyleBoxShadowValue),
    BoxShadowBottom(AzStyleBoxShadowValue),
    ScrollbarStyle(AzScrollbarStyleValue),
    Opacity(AzStyleOpacityValue),
    Transform(AzStyleTransformVecValue),
    TransformOrigin(AzStyleTransformOriginValue),
    PerspectiveOrigin(AzStylePerspectiveOriginValue),
    BackfaceVisibility(AzStyleBackfaceVisibilityValue),
}

/// Re-export of rust-allocated (stack based) `CssPropertySource` struct
#[repr(C, u8)]
pub enum AzCssPropertySource {
    Css(AzCssPath),
    Inline,
}

/// Re-export of rust-allocated (stack based) `VertexLayout` struct
#[repr(C)]
#[pyclass(name = "VertexLayout")]
pub struct AzVertexLayout {
    pub fields: AzVertexAttributeVec,
}

/// Re-export of rust-allocated (stack based) `VertexArrayObject` struct
#[repr(C)]
#[pyclass(name = "VertexArrayObject")]
pub struct AzVertexArrayObject {
    pub vertex_layout: AzVertexLayout,
    pub vao_id: u32,
    pub gl_context: AzGl,
}

/// Re-export of rust-allocated (stack based) `VertexBuffer` struct
#[repr(C)]
#[pyclass(name = "VertexBuffer")]
pub struct AzVertexBuffer {
    pub vertex_buffer_id: u32,
    pub vertex_buffer_len: usize,
    pub vao: AzVertexArrayObject,
    pub index_buffer_id: u32,
    pub index_buffer_len: usize,
    pub index_buffer_format: AzIndexBufferFormat,
}

/// Re-export of rust-allocated (stack based) `SvgMultiPolygon` struct
#[repr(C)]
#[pyclass(name = "SvgMultiPolygon")]
pub struct AzSvgMultiPolygon {
    pub rings: AzSvgPathVec,
}

/// Re-export of rust-allocated (stack based) `XmlNode` struct
#[repr(C)]
#[pyclass(name = "XmlNode")]
pub struct AzXmlNode {
    pub tag: AzString,
    pub attributes: AzStringPairVec,
    pub children: AzXmlNodeVec,
    pub text: AzOptionString,
}

/// Wrapper over a Rust-allocated `Vec<InlineLine>`
#[repr(C)]
#[pyclass(name = "InlineLineVec")]
pub struct AzInlineLineVec {
    pub(crate) ptr: *const AzInlineLine,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzInlineLineVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<CssProperty>`
#[repr(C)]
#[pyclass(name = "CssPropertyVec")]
pub struct AzCssPropertyVec {
    pub(crate) ptr: *const AzCssProperty,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzCssPropertyVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<SvgMultiPolygon>`
#[repr(C)]
#[pyclass(name = "SvgMultiPolygonVec")]
pub struct AzSvgMultiPolygonVec {
    pub(crate) ptr: *const AzSvgMultiPolygon,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzSvgMultiPolygonVecDestructor,
}

/// Re-export of rust-allocated (stack based) `OptionCssProperty` struct
#[repr(C, u8)]
pub enum AzOptionCssProperty {
    None,
    Some(AzCssProperty),
}

/// Re-export of rust-allocated (stack based) `XmlTextError` struct
#[repr(C)]
#[pyclass(name = "XmlTextError")]
pub struct AzXmlTextError {
    pub stream_error: AzXmlStreamError,
    pub pos: AzSvgParseErrorPosition,
}

/// Platform-specific window configuration, i.e. WM options that are not cross-platform
#[repr(C)]
#[pyclass(name = "PlatformSpecificOptions")]
pub struct AzPlatformSpecificOptions {
    pub windows_options: AzWindowsWindowOptions,
    pub linux_options: AzLinuxWindowOptions,
    pub mac_options: AzMacWindowOptions,
    pub wasm_options: AzWasmWindowOptions,
}

/// Re-export of rust-allocated (stack based) `WindowState` struct
#[repr(C)]
#[pyclass(name = "WindowState")]
pub struct AzWindowState {
    pub title: AzString,
    pub theme: AzWindowTheme,
    pub size: AzWindowSize,
    pub position: AzWindowPosition,
    pub flags: AzWindowFlags,
    pub debug_state: AzDebugState,
    pub keyboard_state: AzKeyboardState,
    pub mouse_state: AzMouseState,
    pub touch_state: AzTouchState,
    pub ime_position: AzImePosition,
    pub monitor: AzMonitor,
    pub platform_specific_options: AzPlatformSpecificOptions,
    pub renderer_options: AzRendererOptions,
    pub background_color: AzColorU,
    pub layout_callback: AzLayoutCallback,
    pub close_callback: AzOptionCallback,
}

/// Re-export of rust-allocated (stack based) `CallbackInfo` struct
#[repr(C)]
#[pyclass(name = "CallbackInfo")]
pub struct AzCallbackInfo {
    pub css_property_cache: *const c_void,
    pub styled_node_states: *const c_void,
    pub previous_window_state: *const c_void,
    pub current_window_state: *const c_void,
    pub modifiable_window_state: *mut AzWindowState,
    pub gl_context: *const AzOptionGl,
    pub image_cache: *mut c_void,
    pub system_fonts: *mut c_void,
    pub timers: *mut c_void,
    pub threads: *mut c_void,
    pub new_windows: *mut c_void,
    pub current_window_handle: *const AzRawWindowHandle,
    pub node_hierarchy: *const c_void,
    pub system_callbacks: *const AzSystemCallbacks,
    pub datasets: *mut c_void,
    pub stop_propagation: *mut bool,
    pub focus_target: *mut c_void,
    pub words_cache: *const c_void,
    pub shaped_words_cache: *const c_void,
    pub positioned_words_cache: *const c_void,
    pub positioned_rects: *const c_void,
    pub words_changed_in_callbacks: *mut c_void,
    pub images_changed_in_callbacks: *mut c_void,
    pub image_masks_changed_in_callbacks: *mut c_void,
    pub css_properties_changed_in_callbacks: *mut c_void,
    pub current_scroll_states: *const c_void,
    pub nodes_scrolled_in_callback: *mut c_void,
    pub hit_dom_node: AzDomNodeId,
    pub cursor_relative_to_item: AzOptionLogicalPosition,
    pub cursor_in_viewport: AzOptionLogicalPosition,
    pub _reserved_ref: *const c_void,
    pub _reserved_mut: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `InlineText` struct
#[repr(C)]
#[pyclass(name = "InlineText")]
pub struct AzInlineText {
    pub lines: AzInlineLineVec,
    pub content_size: AzLogicalSize,
    pub font_size_px: f32,
    pub last_word_index: usize,
    pub baseline_descender_px: f32,
}

/// CSS path to set the keyboard input focus
#[repr(C)]
#[pyclass(name = "FocusTargetPath")]
pub struct AzFocusTargetPath {
    pub dom: AzDomId,
    pub css_path: AzCssPath,
}

/// Animation struct to start a new animation
#[repr(C)]
#[pyclass(name = "Animation")]
pub struct AzAnimation {
    pub from: AzCssProperty,
    pub to: AzCssProperty,
    pub duration: AzDuration,
    pub repeat: AzAnimationRepeat,
    pub repeat_count: AzAnimationRepeatCount,
    pub easing: AzAnimationEasing,
    pub relayout_on_finish: bool,
}

/// Re-export of rust-allocated (stack based) `TimerCallbackInfo` struct
#[repr(C)]
#[pyclass(name = "TimerCallbackInfo")]
pub struct AzTimerCallbackInfo {
    pub callback_info: AzCallbackInfo,
    pub node_id: AzOptionDomNodeId,
    pub frame_start: AzInstant,
    pub call_count: usize,
    pub is_about_to_finish: bool,
    pub _reserved_ref: *const c_void,
    pub _reserved_mut: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `NodeDataInlineCssProperty` struct
#[repr(C, u8)]
pub enum AzNodeDataInlineCssProperty {
    Normal(AzCssProperty),
    Active(AzCssProperty),
    Focus(AzCssProperty),
    Hover(AzCssProperty),
}

/// Re-export of rust-allocated (stack based) `DynamicCssProperty` struct
#[repr(C)]
#[pyclass(name = "DynamicCssProperty")]
pub struct AzDynamicCssProperty {
    pub dynamic_id: AzString,
    pub default_value: AzCssProperty,
}

/// Re-export of rust-allocated (stack based) `SvgNode` struct
#[repr(C, u8)]
pub enum AzSvgNode {
    MultiPolygonCollection(AzSvgMultiPolygonVec),
    MultiPolygon(AzSvgMultiPolygon),
    Path(AzSvgPath),
    Circle(AzSvgCircle),
    Rect(AzSvgRect),
}

/// Re-export of rust-allocated (stack based) `SvgStyledNode` struct
#[repr(C)]
#[pyclass(name = "SvgStyledNode")]
pub struct AzSvgStyledNode {
    pub geometry: AzSvgNode,
    pub style: AzSvgStyle,
}

/// Wrapper over a Rust-allocated `Vec<NodeDataInlineCssProperty>`
#[repr(C)]
#[pyclass(name = "NodeDataInlineCssPropertyVec")]
pub struct AzNodeDataInlineCssPropertyVec {
    pub(crate) ptr: *const AzNodeDataInlineCssProperty,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzNodeDataInlineCssPropertyVecDestructor,
}

/// Re-export of rust-allocated (stack based) `OptionWindowState` struct
#[repr(C, u8)]
pub enum AzOptionWindowState {
    None,
    Some(AzWindowState),
}

/// Re-export of rust-allocated (stack based) `OptionInlineText` struct
#[repr(C, u8)]
pub enum AzOptionInlineText {
    None,
    Some(AzInlineText),
}

/// Re-export of rust-allocated (stack based) `XmlParseError` struct
#[repr(C, u8)]
pub enum AzXmlParseError {
    InvalidDeclaration(AzXmlTextError),
    InvalidComment(AzXmlTextError),
    InvalidPI(AzXmlTextError),
    InvalidDoctype(AzXmlTextError),
    InvalidEntity(AzXmlTextError),
    InvalidElement(AzXmlTextError),
    InvalidAttribute(AzXmlTextError),
    InvalidCdata(AzXmlTextError),
    InvalidCharData(AzXmlTextError),
    UnknownToken(AzSvgParseErrorPosition),
}

/// Options on how to initially create the window
#[repr(C)]
#[pyclass(name = "WindowCreateOptions")]
pub struct AzWindowCreateOptions {
    pub state: AzWindowState,
    pub renderer_type: AzOptionRendererOptions,
    pub theme: AzOptionWindowTheme,
    pub create_callback: AzOptionCallback,
    pub hot_reload: bool,
}

/// Defines the keyboard input focus target
#[repr(C, u8)]
pub enum AzFocusTarget {
    Id(AzDomNodeId),
    Path(AzFocusTargetPath),
    Previous,
    Next,
    First,
    Last,
    NoFocus,
}

/// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
#[repr(C)]
#[pyclass(name = "NodeData")]
pub struct AzNodeData {
    pub node_type: AzNodeType,
    pub dataset: AzOptionRefAny,
    pub ids_and_classes: AzIdOrClassVec,
    pub callbacks: AzCallbackDataVec,
    pub inline_css_props: AzNodeDataInlineCssPropertyVec,
    pub clip_mask: AzOptionImageMask,
    pub tab_index: AzOptionTabIndex,
}

/// Re-export of rust-allocated (stack based) `CssDeclaration` struct
#[repr(C, u8)]
pub enum AzCssDeclaration {
    Static(AzCssProperty),
    Dynamic(AzDynamicCssProperty),
}

/// Wrapper over a Rust-allocated `CssDeclaration`
#[repr(C)]
#[pyclass(name = "CssDeclarationVec")]
pub struct AzCssDeclarationVec {
    pub(crate) ptr: *const AzCssDeclaration,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzCssDeclarationVecDestructor,
}

/// Wrapper over a Rust-allocated `NodeDataVec`
#[repr(C)]
#[pyclass(name = "NodeDataVec")]
pub struct AzNodeDataVec {
    pub(crate) ptr: *const AzNodeData,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzNodeDataVecDestructor,
}

/// Re-export of rust-allocated (stack based) `XmlError` struct
#[repr(C, u8)]
pub enum AzXmlError {
    InvalidXmlPrefixUri(AzSvgParseErrorPosition),
    UnexpectedXmlUri(AzSvgParseErrorPosition),
    UnexpectedXmlnsUri(AzSvgParseErrorPosition),
    InvalidElementNamePrefix(AzSvgParseErrorPosition),
    DuplicatedNamespace(AzDuplicatedNamespaceError),
    UnknownNamespace(AzUnknownNamespaceError),
    UnexpectedCloseTag(AzUnexpectedCloseTagError),
    UnexpectedEntityCloseTag(AzSvgParseErrorPosition),
    UnknownEntityReference(AzUnknownEntityReferenceError),
    MalformedEntityReference(AzSvgParseErrorPosition),
    EntityReferenceLoop(AzSvgParseErrorPosition),
    InvalidAttributeValue(AzSvgParseErrorPosition),
    DuplicatedAttribute(AzDuplicatedAttributeError),
    NoRootNode,
    SizeLimit,
    ParserError(AzXmlParseError),
}

/// Re-export of rust-allocated (stack based) `Dom` struct
#[repr(C)]
#[pyclass(name = "Dom")]
pub struct AzDom {
    pub root: AzNodeData,
    pub children: AzDomVec,
    pub total_children: usize,
}

/// Re-export of rust-allocated (stack based) `CssRuleBlock` struct
#[repr(C)]
#[pyclass(name = "CssRuleBlock")]
pub struct AzCssRuleBlock {
    pub path: AzCssPath,
    pub declarations: AzCssDeclarationVec,
}

/// Re-export of rust-allocated (stack based) `StyledDom` struct
#[repr(C)]
#[pyclass(name = "StyledDom")]
pub struct AzStyledDom {
    pub root: AzNodeId,
    pub node_hierarchy: AzNodeVec,
    pub node_data: AzNodeDataVec,
    pub styled_nodes: AzStyledNodeVec,
    pub cascade_info: AzCascadeInfoVec,
    pub nodes_with_window_callbacks: AzNodeIdVec,
    pub nodes_with_not_callbacks: AzNodeIdVec,
    pub nodes_with_datasets_and_callbacks: AzNodeIdVec,
    pub tag_ids_to_node_ids: AzTagIdsToNodeIdsMappingVec,
    pub non_leaf_nodes: AzParentWithNodeDepthVec,
    pub css_property_cache: AzCssPropertyCache,
}

/// Wrapper over a Rust-allocated `CssRuleBlock`
#[repr(C)]
#[pyclass(name = "CssRuleBlockVec")]
pub struct AzCssRuleBlockVec {
    pub(crate) ptr: *const AzCssRuleBlock,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzCssRuleBlockVecDestructor,
}

/// Re-export of rust-allocated (stack based) `OptionDom` struct
#[repr(C, u8)]
pub enum AzOptionDom {
    None,
    Some(AzDom),
}

/// Re-export of rust-allocated (stack based) `ResultXmlXmlError` struct
#[repr(C, u8)]
pub enum AzResultXmlXmlError {
    Ok(AzXml),
    Err(AzXmlError),
}

/// Re-export of rust-allocated (stack based) `SvgParseError` struct
#[repr(C, u8)]
pub enum AzSvgParseError {
    InvalidFileSuffix,
    FileOpenFailed,
    NotAnUtf8Str,
    MalformedGZip,
    InvalidSize,
    ParsingFailed(AzXmlError),
}

/// <img src="../images/scrollbounds.png"/>
#[repr(C)]
#[pyclass(name = "IFrameCallbackReturn")]
pub struct AzIFrameCallbackReturn {
    pub dom: AzStyledDom,
    pub scroll_size: AzLogicalSize,
    pub scroll_offset: AzLogicalPosition,
    pub virtual_scroll_size: AzLogicalSize,
    pub virtual_scroll_offset: AzLogicalPosition,
}

/// Re-export of rust-allocated (stack based) `Stylesheet` struct
#[repr(C)]
#[pyclass(name = "Stylesheet")]
pub struct AzStylesheet {
    pub rules: AzCssRuleBlockVec,
}

/// Wrapper over a Rust-allocated `Stylesheet`
#[repr(C)]
#[pyclass(name = "StylesheetVec")]
pub struct AzStylesheetVec {
    pub(crate) ptr: *const AzStylesheet,
    pub len: usize,
    pub cap: usize,
    pub destructor: AzStylesheetVecDestructor,
}

/// Re-export of rust-allocated (stack based) `ResultSvgXmlNodeSvgParseError` struct
#[repr(C, u8)]
pub enum AzResultSvgXmlNodeSvgParseError {
    Ok(AzSvgXmlNode),
    Err(AzSvgParseError),
}

/// Re-export of rust-allocated (stack based) `ResultSvgSvgParseError` struct
#[repr(C, u8)]
pub enum AzResultSvgSvgParseError {
    Ok(AzSvg),
    Err(AzSvgParseError),
}

/// Re-export of rust-allocated (stack based) `Css` struct
#[repr(C)]
#[pyclass(name = "Css")]
pub struct AzCss {
    pub stylesheets: AzStylesheetVec,
}

/// `AzAppLogLevelEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "AppLogLevel")]
pub struct AzAppLogLevelEnumWrapper {
    pub inner: AzAppLogLevel,
}

/// `AzLayoutSolverVersionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutSolverVersion")]
pub struct AzLayoutSolverVersionEnumWrapper {
    pub inner: AzLayoutSolverVersion,
}

/// `AzVsyncEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "Vsync")]
pub struct AzVsyncEnumWrapper {
    pub inner: AzVsync,
}

/// `AzSrgbEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "Srgb")]
pub struct AzSrgbEnumWrapper {
    pub inner: AzSrgb,
}

/// `AzHwAccelerationEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "HwAcceleration")]
pub struct AzHwAccelerationEnumWrapper {
    pub inner: AzHwAcceleration,
}

/// `AzXWindowTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "XWindowType")]
pub struct AzXWindowTypeEnumWrapper {
    pub inner: AzXWindowType,
}

/// `AzVirtualKeyCodeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "VirtualKeyCode")]
pub struct AzVirtualKeyCodeEnumWrapper {
    pub inner: AzVirtualKeyCode,
}

/// `AzMouseCursorTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "MouseCursorType")]
pub struct AzMouseCursorTypeEnumWrapper {
    pub inner: AzMouseCursorType,
}

/// `AzRendererTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "RendererType")]
pub struct AzRendererTypeEnumWrapper {
    pub inner: AzRendererType,
}

/// `AzFullScreenModeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "FullScreenMode")]
pub struct AzFullScreenModeEnumWrapper {
    pub inner: AzFullScreenMode,
}

/// `AzWindowThemeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "WindowTheme")]
pub struct AzWindowThemeEnumWrapper {
    pub inner: AzWindowTheme,
}

/// `AzUpdateScreenEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "UpdateScreen")]
pub struct AzUpdateScreenEnumWrapper {
    pub inner: AzUpdateScreen,
}

/// `AzAnimationRepeatEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "AnimationRepeat")]
pub struct AzAnimationRepeatEnumWrapper {
    pub inner: AzAnimationRepeat,
}

/// `AzAnimationRepeatCountEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "AnimationRepeatCount")]
pub struct AzAnimationRepeatCountEnumWrapper {
    pub inner: AzAnimationRepeatCount,
}

/// `AzOnEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "On")]
pub struct AzOnEnumWrapper {
    pub inner: AzOn,
}

/// `AzHoverEventFilterEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "HoverEventFilter")]
pub struct AzHoverEventFilterEnumWrapper {
    pub inner: AzHoverEventFilter,
}

/// `AzFocusEventFilterEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "FocusEventFilter")]
pub struct AzFocusEventFilterEnumWrapper {
    pub inner: AzFocusEventFilter,
}

/// `AzWindowEventFilterEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "WindowEventFilter")]
pub struct AzWindowEventFilterEnumWrapper {
    pub inner: AzWindowEventFilter,
}

/// `AzComponentEventFilterEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ComponentEventFilter")]
pub struct AzComponentEventFilterEnumWrapper {
    pub inner: AzComponentEventFilter,
}

/// `AzApplicationEventFilterEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ApplicationEventFilter")]
pub struct AzApplicationEventFilterEnumWrapper {
    pub inner: AzApplicationEventFilter,
}

/// `AzTabIndexEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "TabIndex")]
pub struct AzTabIndexEnumWrapper {
    pub inner: AzTabIndex,
}

/// `AzNodeTypeKeyEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NodeTypeKey")]
pub struct AzNodeTypeKeyEnumWrapper {
    pub inner: AzNodeTypeKey,
}

/// `AzCssPropertyTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssPropertyType")]
pub struct AzCssPropertyTypeEnumWrapper {
    pub inner: AzCssPropertyType,
}

/// `AzSizeMetricEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SizeMetric")]
pub struct AzSizeMetricEnumWrapper {
    pub inner: AzSizeMetric,
}

/// `AzBoxShadowClipModeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "BoxShadowClipMode")]
pub struct AzBoxShadowClipModeEnumWrapper {
    pub inner: AzBoxShadowClipMode,
}

/// `AzLayoutAlignContentEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutAlignContent")]
pub struct AzLayoutAlignContentEnumWrapper {
    pub inner: AzLayoutAlignContent,
}

/// `AzLayoutAlignItemsEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutAlignItems")]
pub struct AzLayoutAlignItemsEnumWrapper {
    pub inner: AzLayoutAlignItems,
}

/// `AzLayoutBoxSizingEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutBoxSizing")]
pub struct AzLayoutBoxSizingEnumWrapper {
    pub inner: AzLayoutBoxSizing,
}

/// `AzLayoutFlexDirectionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFlexDirection")]
pub struct AzLayoutFlexDirectionEnumWrapper {
    pub inner: AzLayoutFlexDirection,
}

/// `AzLayoutDisplayEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutDisplay")]
pub struct AzLayoutDisplayEnumWrapper {
    pub inner: AzLayoutDisplay,
}

/// `AzLayoutFloatEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFloat")]
pub struct AzLayoutFloatEnumWrapper {
    pub inner: AzLayoutFloat,
}

/// `AzLayoutJustifyContentEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutJustifyContent")]
pub struct AzLayoutJustifyContentEnumWrapper {
    pub inner: AzLayoutJustifyContent,
}

/// `AzLayoutPositionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutPosition")]
pub struct AzLayoutPositionEnumWrapper {
    pub inner: AzLayoutPosition,
}

/// `AzLayoutFlexWrapEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFlexWrap")]
pub struct AzLayoutFlexWrapEnumWrapper {
    pub inner: AzLayoutFlexWrap,
}

/// `AzLayoutOverflowEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutOverflow")]
pub struct AzLayoutOverflowEnumWrapper {
    pub inner: AzLayoutOverflow,
}

/// `AzAngleMetricEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "AngleMetric")]
pub struct AzAngleMetricEnumWrapper {
    pub inner: AzAngleMetric,
}

/// `AzDirectionCornerEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "DirectionCorner")]
pub struct AzDirectionCornerEnumWrapper {
    pub inner: AzDirectionCorner,
}

/// `AzExtendModeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ExtendMode")]
pub struct AzExtendModeEnumWrapper {
    pub inner: AzExtendMode,
}

/// `AzShapeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "Shape")]
pub struct AzShapeEnumWrapper {
    pub inner: AzShape,
}

/// `AzRadialGradientSizeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "RadialGradientSize")]
pub struct AzRadialGradientSizeEnumWrapper {
    pub inner: AzRadialGradientSize,
}

/// `AzStyleBackgroundRepeatEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundRepeat")]
pub struct AzStyleBackgroundRepeatEnumWrapper {
    pub inner: AzStyleBackgroundRepeat,
}

/// `AzBorderStyleEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "BorderStyle")]
pub struct AzBorderStyleEnumWrapper {
    pub inner: AzBorderStyle,
}

/// `AzStyleCursorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleCursor")]
pub struct AzStyleCursorEnumWrapper {
    pub inner: AzStyleCursor,
}

/// `AzStyleBackfaceVisibilityEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackfaceVisibility")]
pub struct AzStyleBackfaceVisibilityEnumWrapper {
    pub inner: AzStyleBackfaceVisibility,
}

/// `AzStyleTextAlignEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTextAlign")]
pub struct AzStyleTextAlignEnumWrapper {
    pub inner: AzStyleTextAlign,
}

/// `AzVertexAttributeTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "VertexAttributeType")]
pub struct AzVertexAttributeTypeEnumWrapper {
    pub inner: AzVertexAttributeType,
}

/// `AzIndexBufferFormatEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "IndexBufferFormat")]
pub struct AzIndexBufferFormatEnumWrapper {
    pub inner: AzIndexBufferFormat,
}

/// `AzGlTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "GlType")]
pub struct AzGlTypeEnumWrapper {
    pub inner: AzGlType,
}

/// `AzRawImageFormatEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "RawImageFormat")]
pub struct AzRawImageFormatEnumWrapper {
    pub inner: AzRawImageFormat,
}

/// `AzEncodeImageErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "EncodeImageError")]
pub struct AzEncodeImageErrorEnumWrapper {
    pub inner: AzEncodeImageError,
}

/// `AzDecodeImageErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "DecodeImageError")]
pub struct AzDecodeImageErrorEnumWrapper {
    pub inner: AzDecodeImageError,
}

/// `AzShapeRenderingEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ShapeRendering")]
pub struct AzShapeRenderingEnumWrapper {
    pub inner: AzShapeRendering,
}

/// `AzTextRenderingEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "TextRendering")]
pub struct AzTextRenderingEnumWrapper {
    pub inner: AzTextRendering,
}

/// `AzImageRenderingEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ImageRendering")]
pub struct AzImageRenderingEnumWrapper {
    pub inner: AzImageRendering,
}

/// `AzFontDatabaseEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "FontDatabase")]
pub struct AzFontDatabaseEnumWrapper {
    pub inner: AzFontDatabase,
}

/// `AzIndentEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "Indent")]
pub struct AzIndentEnumWrapper {
    pub inner: AzIndent,
}

/// `AzSvgFitToEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgFitTo")]
pub struct AzSvgFitToEnumWrapper {
    pub inner: AzSvgFitTo,
}

/// `AzSvgFillRuleEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgFillRule")]
pub struct AzSvgFillRuleEnumWrapper {
    pub inner: AzSvgFillRule,
}

/// `AzSvgLineJoinEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgLineJoin")]
pub struct AzSvgLineJoinEnumWrapper {
    pub inner: AzSvgLineJoin,
}

/// `AzSvgLineCapEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgLineCap")]
pub struct AzSvgLineCapEnumWrapper {
    pub inner: AzSvgLineCap,
}

/// `AzMsgBoxIconEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "MsgBoxIcon")]
pub struct AzMsgBoxIconEnumWrapper {
    pub inner: AzMsgBoxIcon,
}

/// `AzMsgBoxYesNoEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "MsgBoxYesNo")]
pub struct AzMsgBoxYesNoEnumWrapper {
    pub inner: AzMsgBoxYesNo,
}

/// `AzMsgBoxOkCancelEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "MsgBoxOkCancel")]
pub struct AzMsgBoxOkCancelEnumWrapper {
    pub inner: AzMsgBoxOkCancel,
}

/// `AzTerminateTimerEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "TerminateTimer")]
pub struct AzTerminateTimerEnumWrapper {
    pub inner: AzTerminateTimer,
}

/// `AzStyleFontFamilyVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleFontFamilyVecDestructor")]
pub struct AzStyleFontFamilyVecDestructorEnumWrapper {
    pub inner: AzStyleFontFamilyVecDestructor,
}

/// `AzTesselatedSvgNodeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "TesselatedSvgNodeVecDestructor")]
pub struct AzTesselatedSvgNodeVecDestructorEnumWrapper {
    pub inner: AzTesselatedSvgNodeVecDestructor,
}

/// `AzXmlNodeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "XmlNodeVecDestructor")]
pub struct AzXmlNodeVecDestructorEnumWrapper {
    pub inner: AzXmlNodeVecDestructor,
}

/// `AzFmtArgVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "FmtArgVecDestructor")]
pub struct AzFmtArgVecDestructorEnumWrapper {
    pub inner: AzFmtArgVecDestructor,
}

/// `AzInlineLineVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "InlineLineVecDestructor")]
pub struct AzInlineLineVecDestructorEnumWrapper {
    pub inner: AzInlineLineVecDestructor,
}

/// `AzInlineWordVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "InlineWordVecDestructor")]
pub struct AzInlineWordVecDestructorEnumWrapper {
    pub inner: AzInlineWordVecDestructor,
}

/// `AzInlineGlyphVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "InlineGlyphVecDestructor")]
pub struct AzInlineGlyphVecDestructorEnumWrapper {
    pub inner: AzInlineGlyphVecDestructor,
}

/// `AzInlineTextHitVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "InlineTextHitVecDestructor")]
pub struct AzInlineTextHitVecDestructorEnumWrapper {
    pub inner: AzInlineTextHitVecDestructor,
}

/// `AzMonitorVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "MonitorVecDestructor")]
pub struct AzMonitorVecDestructorEnumWrapper {
    pub inner: AzMonitorVecDestructor,
}

/// `AzVideoModeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "VideoModeVecDestructor")]
pub struct AzVideoModeVecDestructorEnumWrapper {
    pub inner: AzVideoModeVecDestructor,
}

/// `AzDomVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "DomVecDestructor")]
pub struct AzDomVecDestructorEnumWrapper {
    pub inner: AzDomVecDestructor,
}

/// `AzIdOrClassVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "IdOrClassVecDestructor")]
pub struct AzIdOrClassVecDestructorEnumWrapper {
    pub inner: AzIdOrClassVecDestructor,
}

/// `AzNodeDataInlineCssPropertyVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NodeDataInlineCssPropertyVecDestructor")]
pub struct AzNodeDataInlineCssPropertyVecDestructorEnumWrapper {
    pub inner: AzNodeDataInlineCssPropertyVecDestructor,
}

/// `AzStyleBackgroundContentVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundContentVecDestructor")]
pub struct AzStyleBackgroundContentVecDestructorEnumWrapper {
    pub inner: AzStyleBackgroundContentVecDestructor,
}

/// `AzStyleBackgroundPositionVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundPositionVecDestructor")]
pub struct AzStyleBackgroundPositionVecDestructorEnumWrapper {
    pub inner: AzStyleBackgroundPositionVecDestructor,
}

/// `AzStyleBackgroundRepeatVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundRepeatVecDestructor")]
pub struct AzStyleBackgroundRepeatVecDestructorEnumWrapper {
    pub inner: AzStyleBackgroundRepeatVecDestructor,
}

/// `AzStyleBackgroundSizeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundSizeVecDestructor")]
pub struct AzStyleBackgroundSizeVecDestructorEnumWrapper {
    pub inner: AzStyleBackgroundSizeVecDestructor,
}

/// `AzStyleTransformVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTransformVecDestructor")]
pub struct AzStyleTransformVecDestructorEnumWrapper {
    pub inner: AzStyleTransformVecDestructor,
}

/// `AzCssPropertyVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssPropertyVecDestructor")]
pub struct AzCssPropertyVecDestructorEnumWrapper {
    pub inner: AzCssPropertyVecDestructor,
}

/// `AzSvgMultiPolygonVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgMultiPolygonVecDestructor")]
pub struct AzSvgMultiPolygonVecDestructorEnumWrapper {
    pub inner: AzSvgMultiPolygonVecDestructor,
}

/// `AzSvgPathVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgPathVecDestructor")]
pub struct AzSvgPathVecDestructorEnumWrapper {
    pub inner: AzSvgPathVecDestructor,
}

/// `AzVertexAttributeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "VertexAttributeVecDestructor")]
pub struct AzVertexAttributeVecDestructorEnumWrapper {
    pub inner: AzVertexAttributeVecDestructor,
}

/// `AzSvgPathElementVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgPathElementVecDestructor")]
pub struct AzSvgPathElementVecDestructorEnumWrapper {
    pub inner: AzSvgPathElementVecDestructor,
}

/// `AzSvgVertexVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgVertexVecDestructor")]
pub struct AzSvgVertexVecDestructorEnumWrapper {
    pub inner: AzSvgVertexVecDestructor,
}

/// `AzU32VecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "U32VecDestructor")]
pub struct AzU32VecDestructorEnumWrapper {
    pub inner: AzU32VecDestructor,
}

/// `AzXWindowTypeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "XWindowTypeVecDestructor")]
pub struct AzXWindowTypeVecDestructorEnumWrapper {
    pub inner: AzXWindowTypeVecDestructor,
}

/// `AzVirtualKeyCodeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "VirtualKeyCodeVecDestructor")]
pub struct AzVirtualKeyCodeVecDestructorEnumWrapper {
    pub inner: AzVirtualKeyCodeVecDestructor,
}

/// `AzCascadeInfoVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CascadeInfoVecDestructor")]
pub struct AzCascadeInfoVecDestructorEnumWrapper {
    pub inner: AzCascadeInfoVecDestructor,
}

/// `AzScanCodeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ScanCodeVecDestructor")]
pub struct AzScanCodeVecDestructorEnumWrapper {
    pub inner: AzScanCodeVecDestructor,
}

/// `AzCssDeclarationVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssDeclarationVecDestructor")]
pub struct AzCssDeclarationVecDestructorEnumWrapper {
    pub inner: AzCssDeclarationVecDestructor,
}

/// `AzCssPathSelectorVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssPathSelectorVecDestructor")]
pub struct AzCssPathSelectorVecDestructorEnumWrapper {
    pub inner: AzCssPathSelectorVecDestructor,
}

/// `AzStylesheetVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StylesheetVecDestructor")]
pub struct AzStylesheetVecDestructorEnumWrapper {
    pub inner: AzStylesheetVecDestructor,
}

/// `AzCssRuleBlockVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssRuleBlockVecDestructor")]
pub struct AzCssRuleBlockVecDestructorEnumWrapper {
    pub inner: AzCssRuleBlockVecDestructor,
}

/// `AzF32VecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "F32VecDestructor")]
pub struct AzF32VecDestructorEnumWrapper {
    pub inner: AzF32VecDestructor,
}

/// `AzU16VecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "U16VecDestructor")]
pub struct AzU16VecDestructorEnumWrapper {
    pub inner: AzU16VecDestructor,
}

/// `AzU8VecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "U8VecDestructor")]
pub struct AzU8VecDestructorEnumWrapper {
    pub inner: AzU8VecDestructor,
}

/// `AzCallbackDataVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CallbackDataVecDestructor")]
pub struct AzCallbackDataVecDestructorEnumWrapper {
    pub inner: AzCallbackDataVecDestructor,
}

/// `AzDebugMessageVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "DebugMessageVecDestructor")]
pub struct AzDebugMessageVecDestructorEnumWrapper {
    pub inner: AzDebugMessageVecDestructor,
}

/// `AzGLuintVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "GLuintVecDestructor")]
pub struct AzGLuintVecDestructorEnumWrapper {
    pub inner: AzGLuintVecDestructor,
}

/// `AzGLintVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "GLintVecDestructor")]
pub struct AzGLintVecDestructorEnumWrapper {
    pub inner: AzGLintVecDestructor,
}

/// `AzStringVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StringVecDestructor")]
pub struct AzStringVecDestructorEnumWrapper {
    pub inner: AzStringVecDestructor,
}

/// `AzStringPairVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StringPairVecDestructor")]
pub struct AzStringPairVecDestructorEnumWrapper {
    pub inner: AzStringPairVecDestructor,
}

/// `AzNormalizedLinearColorStopVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NormalizedLinearColorStopVecDestructor")]
pub struct AzNormalizedLinearColorStopVecDestructorEnumWrapper {
    pub inner: AzNormalizedLinearColorStopVecDestructor,
}

/// `AzNormalizedRadialColorStopVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NormalizedRadialColorStopVecDestructor")]
pub struct AzNormalizedRadialColorStopVecDestructorEnumWrapper {
    pub inner: AzNormalizedRadialColorStopVecDestructor,
}

/// `AzNodeIdVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NodeIdVecDestructor")]
pub struct AzNodeIdVecDestructorEnumWrapper {
    pub inner: AzNodeIdVecDestructor,
}

/// `AzNodeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NodeVecDestructor")]
pub struct AzNodeVecDestructorEnumWrapper {
    pub inner: AzNodeVecDestructor,
}

/// `AzStyledNodeVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyledNodeVecDestructor")]
pub struct AzStyledNodeVecDestructorEnumWrapper {
    pub inner: AzStyledNodeVecDestructor,
}

/// `AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "TagIdsToNodeIdsMappingVecDestructor")]
pub struct AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper {
    pub inner: AzTagIdsToNodeIdsMappingVecDestructor,
}

/// `AzParentWithNodeDepthVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ParentWithNodeDepthVecDestructor")]
pub struct AzParentWithNodeDepthVecDestructorEnumWrapper {
    pub inner: AzParentWithNodeDepthVecDestructor,
}

/// `AzNodeDataVecDestructorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NodeDataVecDestructor")]
pub struct AzNodeDataVecDestructorEnumWrapper {
    pub inner: AzNodeDataVecDestructor,
}

/// `AzOptionI16EnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionI16")]
pub struct AzOptionI16EnumWrapper {
    pub inner: AzOptionI16,
}

/// `AzOptionU16EnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionU16")]
pub struct AzOptionU16EnumWrapper {
    pub inner: AzOptionU16,
}

/// `AzOptionU32EnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionU32")]
pub struct AzOptionU32EnumWrapper {
    pub inner: AzOptionU32,
}

/// `AzOptionHwndHandleEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionHwndHandle")]
pub struct AzOptionHwndHandleEnumWrapper {
    pub inner: AzOptionHwndHandle,
}

/// `AzOptionX11VisualEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionX11Visual")]
pub struct AzOptionX11VisualEnumWrapper {
    pub inner: AzOptionX11Visual,
}

/// `AzOptionI32EnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionI32")]
pub struct AzOptionI32EnumWrapper {
    pub inner: AzOptionI32,
}

/// `AzOptionF32EnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionF32")]
pub struct AzOptionF32EnumWrapper {
    pub inner: AzOptionF32,
}

/// `AzOptionCharEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionChar")]
pub struct AzOptionCharEnumWrapper {
    pub inner: AzOptionChar,
}

/// `AzOptionUsizeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionUsize")]
pub struct AzOptionUsizeEnumWrapper {
    pub inner: AzOptionUsize,
}

/// `AzRawWindowHandleEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "RawWindowHandle")]
pub struct AzRawWindowHandleEnumWrapper {
    pub inner: AzRawWindowHandle,
}

/// `AzAcceleratorKeyEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "AcceleratorKey")]
pub struct AzAcceleratorKeyEnumWrapper {
    pub inner: AzAcceleratorKey,
}

/// `AzCursorPositionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CursorPosition")]
pub struct AzCursorPositionEnumWrapper {
    pub inner: AzCursorPosition,
}

/// `AzWindowPositionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "WindowPosition")]
pub struct AzWindowPositionEnumWrapper {
    pub inner: AzWindowPosition,
}

/// `AzImePositionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ImePosition")]
pub struct AzImePositionEnumWrapper {
    pub inner: AzImePosition,
}

/// `AzPositionInfoEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "PositionInfo")]
pub struct AzPositionInfoEnumWrapper {
    pub inner: AzPositionInfo,
}

/// `AzNotEventFilterEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NotEventFilter")]
pub struct AzNotEventFilterEnumWrapper {
    pub inner: AzNotEventFilter,
}

/// `AzCssNthChildSelectorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssNthChildSelector")]
pub struct AzCssNthChildSelectorEnumWrapper {
    pub inner: AzCssNthChildSelector,
}

/// `AzDirectionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "Direction")]
pub struct AzDirectionEnumWrapper {
    pub inner: AzDirection,
}

/// `AzBackgroundPositionHorizontalEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "BackgroundPositionHorizontal")]
pub struct AzBackgroundPositionHorizontalEnumWrapper {
    pub inner: AzBackgroundPositionHorizontal,
}

/// `AzBackgroundPositionVerticalEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "BackgroundPositionVertical")]
pub struct AzBackgroundPositionVerticalEnumWrapper {
    pub inner: AzBackgroundPositionVertical,
}

/// `AzStyleBackgroundSizeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundSize")]
pub struct AzStyleBackgroundSizeEnumWrapper {
    pub inner: AzStyleBackgroundSize,
}

/// `AzStyleBoxShadowValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBoxShadowValue")]
pub struct AzStyleBoxShadowValueEnumWrapper {
    pub inner: AzStyleBoxShadowValue,
}

/// `AzLayoutAlignContentValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutAlignContentValue")]
pub struct AzLayoutAlignContentValueEnumWrapper {
    pub inner: AzLayoutAlignContentValue,
}

/// `AzLayoutAlignItemsValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutAlignItemsValue")]
pub struct AzLayoutAlignItemsValueEnumWrapper {
    pub inner: AzLayoutAlignItemsValue,
}

/// `AzLayoutBottomValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutBottomValue")]
pub struct AzLayoutBottomValueEnumWrapper {
    pub inner: AzLayoutBottomValue,
}

/// `AzLayoutBoxSizingValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutBoxSizingValue")]
pub struct AzLayoutBoxSizingValueEnumWrapper {
    pub inner: AzLayoutBoxSizingValue,
}

/// `AzLayoutFlexDirectionValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFlexDirectionValue")]
pub struct AzLayoutFlexDirectionValueEnumWrapper {
    pub inner: AzLayoutFlexDirectionValue,
}

/// `AzLayoutDisplayValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutDisplayValue")]
pub struct AzLayoutDisplayValueEnumWrapper {
    pub inner: AzLayoutDisplayValue,
}

/// `AzLayoutFlexGrowValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFlexGrowValue")]
pub struct AzLayoutFlexGrowValueEnumWrapper {
    pub inner: AzLayoutFlexGrowValue,
}

/// `AzLayoutFlexShrinkValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFlexShrinkValue")]
pub struct AzLayoutFlexShrinkValueEnumWrapper {
    pub inner: AzLayoutFlexShrinkValue,
}

/// `AzLayoutFloatValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFloatValue")]
pub struct AzLayoutFloatValueEnumWrapper {
    pub inner: AzLayoutFloatValue,
}

/// `AzLayoutHeightValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutHeightValue")]
pub struct AzLayoutHeightValueEnumWrapper {
    pub inner: AzLayoutHeightValue,
}

/// `AzLayoutJustifyContentValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutJustifyContentValue")]
pub struct AzLayoutJustifyContentValueEnumWrapper {
    pub inner: AzLayoutJustifyContentValue,
}

/// `AzLayoutLeftValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutLeftValue")]
pub struct AzLayoutLeftValueEnumWrapper {
    pub inner: AzLayoutLeftValue,
}

/// `AzLayoutMarginBottomValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMarginBottomValue")]
pub struct AzLayoutMarginBottomValueEnumWrapper {
    pub inner: AzLayoutMarginBottomValue,
}

/// `AzLayoutMarginLeftValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMarginLeftValue")]
pub struct AzLayoutMarginLeftValueEnumWrapper {
    pub inner: AzLayoutMarginLeftValue,
}

/// `AzLayoutMarginRightValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMarginRightValue")]
pub struct AzLayoutMarginRightValueEnumWrapper {
    pub inner: AzLayoutMarginRightValue,
}

/// `AzLayoutMarginTopValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMarginTopValue")]
pub struct AzLayoutMarginTopValueEnumWrapper {
    pub inner: AzLayoutMarginTopValue,
}

/// `AzLayoutMaxHeightValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMaxHeightValue")]
pub struct AzLayoutMaxHeightValueEnumWrapper {
    pub inner: AzLayoutMaxHeightValue,
}

/// `AzLayoutMaxWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMaxWidthValue")]
pub struct AzLayoutMaxWidthValueEnumWrapper {
    pub inner: AzLayoutMaxWidthValue,
}

/// `AzLayoutMinHeightValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMinHeightValue")]
pub struct AzLayoutMinHeightValueEnumWrapper {
    pub inner: AzLayoutMinHeightValue,
}

/// `AzLayoutMinWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutMinWidthValue")]
pub struct AzLayoutMinWidthValueEnumWrapper {
    pub inner: AzLayoutMinWidthValue,
}

/// `AzLayoutPaddingBottomValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutPaddingBottomValue")]
pub struct AzLayoutPaddingBottomValueEnumWrapper {
    pub inner: AzLayoutPaddingBottomValue,
}

/// `AzLayoutPaddingLeftValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutPaddingLeftValue")]
pub struct AzLayoutPaddingLeftValueEnumWrapper {
    pub inner: AzLayoutPaddingLeftValue,
}

/// `AzLayoutPaddingRightValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutPaddingRightValue")]
pub struct AzLayoutPaddingRightValueEnumWrapper {
    pub inner: AzLayoutPaddingRightValue,
}

/// `AzLayoutPaddingTopValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutPaddingTopValue")]
pub struct AzLayoutPaddingTopValueEnumWrapper {
    pub inner: AzLayoutPaddingTopValue,
}

/// `AzLayoutPositionValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutPositionValue")]
pub struct AzLayoutPositionValueEnumWrapper {
    pub inner: AzLayoutPositionValue,
}

/// `AzLayoutRightValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutRightValue")]
pub struct AzLayoutRightValueEnumWrapper {
    pub inner: AzLayoutRightValue,
}

/// `AzLayoutTopValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutTopValue")]
pub struct AzLayoutTopValueEnumWrapper {
    pub inner: AzLayoutTopValue,
}

/// `AzLayoutWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutWidthValue")]
pub struct AzLayoutWidthValueEnumWrapper {
    pub inner: AzLayoutWidthValue,
}

/// `AzLayoutFlexWrapValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutFlexWrapValue")]
pub struct AzLayoutFlexWrapValueEnumWrapper {
    pub inner: AzLayoutFlexWrapValue,
}

/// `AzLayoutOverflowValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutOverflowValue")]
pub struct AzLayoutOverflowValueEnumWrapper {
    pub inner: AzLayoutOverflowValue,
}

/// `AzStyleBorderBottomColorValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderBottomColorValue")]
pub struct AzStyleBorderBottomColorValueEnumWrapper {
    pub inner: AzStyleBorderBottomColorValue,
}

/// `AzStyleBorderBottomLeftRadiusValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderBottomLeftRadiusValue")]
pub struct AzStyleBorderBottomLeftRadiusValueEnumWrapper {
    pub inner: AzStyleBorderBottomLeftRadiusValue,
}

/// `AzStyleBorderBottomRightRadiusValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderBottomRightRadiusValue")]
pub struct AzStyleBorderBottomRightRadiusValueEnumWrapper {
    pub inner: AzStyleBorderBottomRightRadiusValue,
}

/// `AzStyleBorderBottomStyleValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderBottomStyleValue")]
pub struct AzStyleBorderBottomStyleValueEnumWrapper {
    pub inner: AzStyleBorderBottomStyleValue,
}

/// `AzLayoutBorderBottomWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutBorderBottomWidthValue")]
pub struct AzLayoutBorderBottomWidthValueEnumWrapper {
    pub inner: AzLayoutBorderBottomWidthValue,
}

/// `AzStyleBorderLeftColorValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderLeftColorValue")]
pub struct AzStyleBorderLeftColorValueEnumWrapper {
    pub inner: AzStyleBorderLeftColorValue,
}

/// `AzStyleBorderLeftStyleValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderLeftStyleValue")]
pub struct AzStyleBorderLeftStyleValueEnumWrapper {
    pub inner: AzStyleBorderLeftStyleValue,
}

/// `AzLayoutBorderLeftWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutBorderLeftWidthValue")]
pub struct AzLayoutBorderLeftWidthValueEnumWrapper {
    pub inner: AzLayoutBorderLeftWidthValue,
}

/// `AzStyleBorderRightColorValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderRightColorValue")]
pub struct AzStyleBorderRightColorValueEnumWrapper {
    pub inner: AzStyleBorderRightColorValue,
}

/// `AzStyleBorderRightStyleValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderRightStyleValue")]
pub struct AzStyleBorderRightStyleValueEnumWrapper {
    pub inner: AzStyleBorderRightStyleValue,
}

/// `AzLayoutBorderRightWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutBorderRightWidthValue")]
pub struct AzLayoutBorderRightWidthValueEnumWrapper {
    pub inner: AzLayoutBorderRightWidthValue,
}

/// `AzStyleBorderTopColorValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderTopColorValue")]
pub struct AzStyleBorderTopColorValueEnumWrapper {
    pub inner: AzStyleBorderTopColorValue,
}

/// `AzStyleBorderTopLeftRadiusValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderTopLeftRadiusValue")]
pub struct AzStyleBorderTopLeftRadiusValueEnumWrapper {
    pub inner: AzStyleBorderTopLeftRadiusValue,
}

/// `AzStyleBorderTopRightRadiusValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderTopRightRadiusValue")]
pub struct AzStyleBorderTopRightRadiusValueEnumWrapper {
    pub inner: AzStyleBorderTopRightRadiusValue,
}

/// `AzStyleBorderTopStyleValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBorderTopStyleValue")]
pub struct AzStyleBorderTopStyleValueEnumWrapper {
    pub inner: AzStyleBorderTopStyleValue,
}

/// `AzLayoutBorderTopWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "LayoutBorderTopWidthValue")]
pub struct AzLayoutBorderTopWidthValueEnumWrapper {
    pub inner: AzLayoutBorderTopWidthValue,
}

/// `AzStyleCursorValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleCursorValue")]
pub struct AzStyleCursorValueEnumWrapper {
    pub inner: AzStyleCursorValue,
}

/// `AzStyleFontSizeValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleFontSizeValue")]
pub struct AzStyleFontSizeValueEnumWrapper {
    pub inner: AzStyleFontSizeValue,
}

/// `AzStyleLetterSpacingValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleLetterSpacingValue")]
pub struct AzStyleLetterSpacingValueEnumWrapper {
    pub inner: AzStyleLetterSpacingValue,
}

/// `AzStyleLineHeightValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleLineHeightValue")]
pub struct AzStyleLineHeightValueEnumWrapper {
    pub inner: AzStyleLineHeightValue,
}

/// `AzStyleTabWidthValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTabWidthValue")]
pub struct AzStyleTabWidthValueEnumWrapper {
    pub inner: AzStyleTabWidthValue,
}

/// `AzStyleTextAlignValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTextAlignValue")]
pub struct AzStyleTextAlignValueEnumWrapper {
    pub inner: AzStyleTextAlignValue,
}

/// `AzStyleTextColorValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTextColorValue")]
pub struct AzStyleTextColorValueEnumWrapper {
    pub inner: AzStyleTextColorValue,
}

/// `AzStyleWordSpacingValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleWordSpacingValue")]
pub struct AzStyleWordSpacingValueEnumWrapper {
    pub inner: AzStyleWordSpacingValue,
}

/// `AzStyleOpacityValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleOpacityValue")]
pub struct AzStyleOpacityValueEnumWrapper {
    pub inner: AzStyleOpacityValue,
}

/// `AzStyleTransformOriginValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTransformOriginValue")]
pub struct AzStyleTransformOriginValueEnumWrapper {
    pub inner: AzStyleTransformOriginValue,
}

/// `AzStylePerspectiveOriginValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StylePerspectiveOriginValue")]
pub struct AzStylePerspectiveOriginValueEnumWrapper {
    pub inner: AzStylePerspectiveOriginValue,
}

/// `AzStyleBackfaceVisibilityValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackfaceVisibilityValue")]
pub struct AzStyleBackfaceVisibilityValueEnumWrapper {
    pub inner: AzStyleBackfaceVisibilityValue,
}

/// `AzDurationEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "Duration")]
pub struct AzDurationEnumWrapper {
    pub inner: AzDuration,
}

/// `AzThreadSendMsgEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ThreadSendMsg")]
pub struct AzThreadSendMsgEnumWrapper {
    pub inner: AzThreadSendMsg,
}

/// `AzOptionPositionInfoEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionPositionInfo")]
pub struct AzOptionPositionInfoEnumWrapper {
    pub inner: AzOptionPositionInfo,
}

/// `AzOptionTimerIdEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionTimerId")]
pub struct AzOptionTimerIdEnumWrapper {
    pub inner: AzOptionTimerId,
}

/// `AzOptionThreadIdEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionThreadId")]
pub struct AzOptionThreadIdEnumWrapper {
    pub inner: AzOptionThreadId,
}

/// `AzOptionImageRefEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionImageRef")]
pub struct AzOptionImageRefEnumWrapper {
    pub inner: AzOptionImageRef,
}

/// `AzOptionFontRefEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionFontRef")]
pub struct AzOptionFontRefEnumWrapper {
    pub inner: AzOptionFontRef,
}

/// `AzOptionSystemClipboardEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionSystemClipboard")]
pub struct AzOptionSystemClipboardEnumWrapper {
    pub inner: AzOptionSystemClipboard,
}

/// `AzOptionFileEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionFile")]
pub struct AzOptionFileEnumWrapper {
    pub inner: AzOptionFile,
}

/// `AzOptionGlEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionGl")]
pub struct AzOptionGlEnumWrapper {
    pub inner: AzOptionGl,
}

/// `AzOptionPercentageValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionPercentageValue")]
pub struct AzOptionPercentageValueEnumWrapper {
    pub inner: AzOptionPercentageValue,
}

/// `AzOptionAngleValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionAngleValue")]
pub struct AzOptionAngleValueEnumWrapper {
    pub inner: AzOptionAngleValue,
}

/// `AzOptionRendererOptionsEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionRendererOptions")]
pub struct AzOptionRendererOptionsEnumWrapper {
    pub inner: AzOptionRendererOptions,
}

/// `AzOptionCallbackEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionCallback")]
pub struct AzOptionCallbackEnumWrapper {
    pub inner: AzOptionCallback,
}

/// `AzOptionThreadSendMsgEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionThreadSendMsg")]
pub struct AzOptionThreadSendMsgEnumWrapper {
    pub inner: AzOptionThreadSendMsg,
}

/// `AzOptionLayoutRectEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionLayoutRect")]
pub struct AzOptionLayoutRectEnumWrapper {
    pub inner: AzOptionLayoutRect,
}

/// `AzOptionRefAnyEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionRefAny")]
pub struct AzOptionRefAnyEnumWrapper {
    pub inner: AzOptionRefAny,
}

/// `AzOptionLayoutPointEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionLayoutPoint")]
pub struct AzOptionLayoutPointEnumWrapper {
    pub inner: AzOptionLayoutPoint,
}

/// `AzOptionLayoutSizeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionLayoutSize")]
pub struct AzOptionLayoutSizeEnumWrapper {
    pub inner: AzOptionLayoutSize,
}

/// `AzOptionWindowThemeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionWindowTheme")]
pub struct AzOptionWindowThemeEnumWrapper {
    pub inner: AzOptionWindowTheme,
}

/// `AzOptionNodeIdEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionNodeId")]
pub struct AzOptionNodeIdEnumWrapper {
    pub inner: AzOptionNodeId,
}

/// `AzOptionDomNodeIdEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionDomNodeId")]
pub struct AzOptionDomNodeIdEnumWrapper {
    pub inner: AzOptionDomNodeId,
}

/// `AzOptionColorUEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionColorU")]
pub struct AzOptionColorUEnumWrapper {
    pub inner: AzOptionColorU,
}

/// `AzOptionSvgDashPatternEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionSvgDashPattern")]
pub struct AzOptionSvgDashPatternEnumWrapper {
    pub inner: AzOptionSvgDashPattern,
}

/// `AzOptionLogicalPositionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionLogicalPosition")]
pub struct AzOptionLogicalPositionEnumWrapper {
    pub inner: AzOptionLogicalPosition,
}

/// `AzOptionPhysicalPositionI32EnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionPhysicalPositionI32")]
pub struct AzOptionPhysicalPositionI32EnumWrapper {
    pub inner: AzOptionPhysicalPositionI32,
}

/// `AzOptionMouseCursorTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionMouseCursorType")]
pub struct AzOptionMouseCursorTypeEnumWrapper {
    pub inner: AzOptionMouseCursorType,
}

/// `AzOptionLogicalSizeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionLogicalSize")]
pub struct AzOptionLogicalSizeEnumWrapper {
    pub inner: AzOptionLogicalSize,
}

/// `AzOptionVirtualKeyCodeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionVirtualKeyCode")]
pub struct AzOptionVirtualKeyCodeEnumWrapper {
    pub inner: AzOptionVirtualKeyCode,
}

/// `AzOptionImageMaskEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionImageMask")]
pub struct AzOptionImageMaskEnumWrapper {
    pub inner: AzOptionImageMask,
}

/// `AzOptionTabIndexEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionTabIndex")]
pub struct AzOptionTabIndexEnumWrapper {
    pub inner: AzOptionTabIndex,
}

/// `AzOptionTagIdEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionTagId")]
pub struct AzOptionTagIdEnumWrapper {
    pub inner: AzOptionTagId,
}

/// `AzOptionDurationEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionDuration")]
pub struct AzOptionDurationEnumWrapper {
    pub inner: AzOptionDuration,
}

/// `AzOptionU8VecEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionU8Vec")]
pub struct AzOptionU8VecEnumWrapper {
    pub inner: AzOptionU8Vec,
}

/// `AzOptionU8VecRefEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionU8VecRef")]
pub struct AzOptionU8VecRefEnumWrapper {
    pub inner: AzOptionU8VecRef,
}

/// `AzResultU8VecEncodeImageErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ResultU8VecEncodeImageError")]
pub struct AzResultU8VecEncodeImageErrorEnumWrapper {
    pub inner: AzResultU8VecEncodeImageError,
}

/// `AzWindowIconEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "WindowIcon")]
pub struct AzWindowIconEnumWrapper {
    pub inner: AzWindowIcon,
}

/// `AzAnimationEasingEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "AnimationEasing")]
pub struct AzAnimationEasingEnumWrapper {
    pub inner: AzAnimationEasing,
}

/// `AzEventFilterEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "EventFilter")]
pub struct AzEventFilterEnumWrapper {
    pub inner: AzEventFilter,
}

/// `AzCssPathPseudoSelectorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssPathPseudoSelector")]
pub struct AzCssPathPseudoSelectorEnumWrapper {
    pub inner: AzCssPathPseudoSelector,
}

/// `AzAnimationInterpolationFunctionEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "AnimationInterpolationFunction")]
pub struct AzAnimationInterpolationFunctionEnumWrapper {
    pub inner: AzAnimationInterpolationFunction,
}

/// `AzStyleTransformEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTransform")]
pub struct AzStyleTransformEnumWrapper {
    pub inner: AzStyleTransform,
}

/// `AzStyleBackgroundPositionVecValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundPositionVecValue")]
pub struct AzStyleBackgroundPositionVecValueEnumWrapper {
    pub inner: AzStyleBackgroundPositionVecValue,
}

/// `AzStyleBackgroundRepeatVecValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundRepeatVecValue")]
pub struct AzStyleBackgroundRepeatVecValueEnumWrapper {
    pub inner: AzStyleBackgroundRepeatVecValue,
}

/// `AzStyleBackgroundSizeVecValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundSizeVecValue")]
pub struct AzStyleBackgroundSizeVecValueEnumWrapper {
    pub inner: AzStyleBackgroundSizeVecValue,
}

/// `AzRawImageDataEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "RawImageData")]
pub struct AzRawImageDataEnumWrapper {
    pub inner: AzRawImageData,
}

/// `AzSvgPathElementEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgPathElement")]
pub struct AzSvgPathElementEnumWrapper {
    pub inner: AzSvgPathElement,
}

/// `AzInstantEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "Instant")]
pub struct AzInstantEnumWrapper {
    pub inner: AzInstant,
}

/// `AzThreadReceiveMsgEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ThreadReceiveMsg")]
pub struct AzThreadReceiveMsgEnumWrapper {
    pub inner: AzThreadReceiveMsg,
}

/// `AzOptionMouseStateEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionMouseState")]
pub struct AzOptionMouseStateEnumWrapper {
    pub inner: AzOptionMouseState,
}

/// `AzOptionKeyboardStateEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionKeyboardState")]
pub struct AzOptionKeyboardStateEnumWrapper {
    pub inner: AzOptionKeyboardState,
}

/// `AzOptionStringVecEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionStringVec")]
pub struct AzOptionStringVecEnumWrapper {
    pub inner: AzOptionStringVec,
}

/// `AzOptionThreadReceiveMsgEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionThreadReceiveMsg")]
pub struct AzOptionThreadReceiveMsgEnumWrapper {
    pub inner: AzOptionThreadReceiveMsg,
}

/// `AzOptionTaskBarIconEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionTaskBarIcon")]
pub struct AzOptionTaskBarIconEnumWrapper {
    pub inner: AzOptionTaskBarIcon,
}

/// `AzOptionWindowIconEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionWindowIcon")]
pub struct AzOptionWindowIconEnumWrapper {
    pub inner: AzOptionWindowIcon,
}

/// `AzOptionStringEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionString")]
pub struct AzOptionStringEnumWrapper {
    pub inner: AzOptionString,
}

/// `AzOptionTextureEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionTexture")]
pub struct AzOptionTextureEnumWrapper {
    pub inner: AzOptionTexture,
}

/// `AzOptionInstantEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionInstant")]
pub struct AzOptionInstantEnumWrapper {
    pub inner: AzOptionInstant,
}

/// `AzInlineWordEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "InlineWord")]
pub struct AzInlineWordEnumWrapper {
    pub inner: AzInlineWord,
}

/// `AzNodeTypeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NodeType")]
pub struct AzNodeTypeEnumWrapper {
    pub inner: AzNodeType,
}

/// `AzIdOrClassEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "IdOrClass")]
pub struct AzIdOrClassEnumWrapper {
    pub inner: AzIdOrClass,
}

/// `AzCssPathSelectorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssPathSelector")]
pub struct AzCssPathSelectorEnumWrapper {
    pub inner: AzCssPathSelector,
}

/// `AzStyleBackgroundContentEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundContent")]
pub struct AzStyleBackgroundContentEnumWrapper {
    pub inner: AzStyleBackgroundContent,
}

/// `AzStyleFontFamilyEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleFontFamily")]
pub struct AzStyleFontFamilyEnumWrapper {
    pub inner: AzStyleFontFamily,
}

/// `AzScrollbarStyleValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ScrollbarStyleValue")]
pub struct AzScrollbarStyleValueEnumWrapper {
    pub inner: AzScrollbarStyleValue,
}

/// `AzStyleTransformVecValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleTransformVecValue")]
pub struct AzStyleTransformVecValueEnumWrapper {
    pub inner: AzStyleTransformVecValue,
}

/// `AzSvgStyleEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgStyle")]
pub struct AzSvgStyleEnumWrapper {
    pub inner: AzSvgStyle,
}

/// `AzFmtValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "FmtValue")]
pub struct AzFmtValueEnumWrapper {
    pub inner: AzFmtValue,
}

/// `AzOptionFileTypeListEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionFileTypeList")]
pub struct AzOptionFileTypeListEnumWrapper {
    pub inner: AzOptionFileTypeList,
}

/// `AzOptionRawImageEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionRawImage")]
pub struct AzOptionRawImageEnumWrapper {
    pub inner: AzOptionRawImage,
}

/// `AzOptionWaylandThemeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionWaylandTheme")]
pub struct AzOptionWaylandThemeEnumWrapper {
    pub inner: AzOptionWaylandTheme,
}

/// `AzResultRawImageDecodeImageErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ResultRawImageDecodeImageError")]
pub struct AzResultRawImageDecodeImageErrorEnumWrapper {
    pub inner: AzResultRawImageDecodeImageError,
}

/// `AzXmlStreamErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "XmlStreamError")]
pub struct AzXmlStreamErrorEnumWrapper {
    pub inner: AzXmlStreamError,
}

/// `AzStyleBackgroundContentVecValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleBackgroundContentVecValue")]
pub struct AzStyleBackgroundContentVecValueEnumWrapper {
    pub inner: AzStyleBackgroundContentVecValue,
}

/// `AzStyleFontFamilyVecValueEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "StyleFontFamilyVecValue")]
pub struct AzStyleFontFamilyVecValueEnumWrapper {
    pub inner: AzStyleFontFamilyVecValue,
}

/// `AzCssPropertyEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssProperty")]
pub struct AzCssPropertyEnumWrapper {
    pub inner: AzCssProperty,
}

/// `AzCssPropertySourceEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssPropertySource")]
pub struct AzCssPropertySourceEnumWrapper {
    pub inner: AzCssPropertySource,
}

/// `AzOptionCssPropertyEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionCssProperty")]
pub struct AzOptionCssPropertyEnumWrapper {
    pub inner: AzOptionCssProperty,
}

/// `AzNodeDataInlineCssPropertyEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "NodeDataInlineCssProperty")]
pub struct AzNodeDataInlineCssPropertyEnumWrapper {
    pub inner: AzNodeDataInlineCssProperty,
}

/// `AzSvgNodeEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgNode")]
pub struct AzSvgNodeEnumWrapper {
    pub inner: AzSvgNode,
}

/// `AzOptionWindowStateEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionWindowState")]
pub struct AzOptionWindowStateEnumWrapper {
    pub inner: AzOptionWindowState,
}

/// `AzOptionInlineTextEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionInlineText")]
pub struct AzOptionInlineTextEnumWrapper {
    pub inner: AzOptionInlineText,
}

/// `AzXmlParseErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "XmlParseError")]
pub struct AzXmlParseErrorEnumWrapper {
    pub inner: AzXmlParseError,
}

/// `AzFocusTargetEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "FocusTarget")]
pub struct AzFocusTargetEnumWrapper {
    pub inner: AzFocusTarget,
}

/// `AzCssDeclarationEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "CssDeclaration")]
pub struct AzCssDeclarationEnumWrapper {
    pub inner: AzCssDeclaration,
}

/// `AzXmlErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "XmlError")]
pub struct AzXmlErrorEnumWrapper {
    pub inner: AzXmlError,
}

/// `AzOptionDomEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "OptionDom")]
pub struct AzOptionDomEnumWrapper {
    pub inner: AzOptionDom,
}

/// `AzResultXmlXmlErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ResultXmlXmlError")]
pub struct AzResultXmlXmlErrorEnumWrapper {
    pub inner: AzResultXmlXmlError,
}

/// `AzSvgParseErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "SvgParseError")]
pub struct AzSvgParseErrorEnumWrapper {
    pub inner: AzSvgParseError,
}

/// `AzResultSvgXmlNodeSvgParseErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ResultSvgXmlNodeSvgParseError")]
pub struct AzResultSvgXmlNodeSvgParseErrorEnumWrapper {
    pub inner: AzResultSvgXmlNodeSvgParseError,
}

/// `AzResultSvgSvgParseErrorEnumWrapper` struct
#[repr(transparent)]
#[pyclass(name = "ResultSvgSvgParseError")]
pub struct AzResultSvgSvgParseErrorEnumWrapper {
    pub inner: AzResultSvgSvgParseError,
}


// Necessary because the Python interpreter may send structs across different threads
unsafe impl Send for AzApp { }
unsafe impl Send for AzIOSHandle { }
unsafe impl Send for AzMacOSHandle { }
unsafe impl Send for AzXlibHandle { }
unsafe impl Send for AzXcbHandle { }
unsafe impl Send for AzWaylandHandle { }
unsafe impl Send for AzWindowsHandle { }
unsafe impl Send for AzAndroidHandle { }
unsafe impl Send for AzRefCount { }
unsafe impl Send for AzCssPropertyCache { }
unsafe impl Send for AzU8VecRef { }
unsafe impl Send for AzU8VecRefMut { }
unsafe impl Send for AzF32VecRef { }
unsafe impl Send for AzI32VecRef { }
unsafe impl Send for AzGLuintVecRef { }
unsafe impl Send for AzGLenumVecRef { }
unsafe impl Send for AzGLintVecRefMut { }
unsafe impl Send for AzGLint64VecRefMut { }
unsafe impl Send for AzGLbooleanVecRefMut { }
unsafe impl Send for AzGLfloatVecRefMut { }
unsafe impl Send for AzRefstr { }
unsafe impl Send for AzGLsyncPtr { }
unsafe impl Send for AzImageRef { }
unsafe impl Send for AzFontRef { }
unsafe impl Send for AzSvg { }
unsafe impl Send for AzSvgXmlNode { }
unsafe impl Send for AzFile { }
unsafe impl Send for AzMsgBox { }
unsafe impl Send for AzFileDialog { }
unsafe impl Send for AzColorPickerDialog { }
unsafe impl Send for AzSystemClipboard { }
unsafe impl Send for AzOptionHwndHandle { }
unsafe impl Send for AzOptionX11Visual { }
unsafe impl Send for AzIFrameCallbackInfo { }
unsafe impl Send for AzRefAny { }
unsafe impl Send for AzStyleBoxShadow { }
unsafe impl Send for AzStyleBackgroundSize { }
unsafe impl Send for AzGl { }
unsafe impl Send for AzRefstrVecRef { }
unsafe impl Send for AzFontMetrics { }
unsafe impl Send for AzInstantPtr { }
unsafe impl Send for AzThread { }
unsafe impl Send for AzThreadSender { }
unsafe impl Send for AzThreadReceiver { }
unsafe impl Send for AzXmlNodeVec { }
unsafe impl Send for AzInlineGlyphVec { }
unsafe impl Send for AzInlineTextHitVec { }
unsafe impl Send for AzVideoModeVec { }
unsafe impl Send for AzDomVec { }
unsafe impl Send for AzStyleBackgroundPositionVec { }
unsafe impl Send for AzStyleBackgroundRepeatVec { }
unsafe impl Send for AzStyleBackgroundSizeVec { }
unsafe impl Send for AzSvgVertexVec { }
unsafe impl Send for AzU32Vec { }
unsafe impl Send for AzXWindowTypeVec { }
unsafe impl Send for AzVirtualKeyCodeVec { }
unsafe impl Send for AzCascadeInfoVec { }
unsafe impl Send for AzScanCodeVec { }
unsafe impl Send for AzU16Vec { }
unsafe impl Send for AzF32Vec { }
unsafe impl Send for AzU8Vec { }
unsafe impl Send for AzGLuintVec { }
unsafe impl Send for AzGLintVec { }
unsafe impl Send for AzNormalizedLinearColorStopVec { }
unsafe impl Send for AzNormalizedRadialColorStopVec { }
unsafe impl Send for AzNodeIdVec { }
unsafe impl Send for AzNodeVec { }
unsafe impl Send for AzParentWithNodeDepthVec { }
unsafe impl Send for AzRenderImageCallbackInfo { }
unsafe impl Send for AzLayoutCallbackInfo { }
unsafe impl Send for AzTesselatedSvgNodeVecRef { }
unsafe impl Send for AzTesselatedSvgNodeVec { }
unsafe impl Send for AzStyleTransformVec { }
unsafe impl Send for AzSvgPathElementVec { }
unsafe impl Send for AzStringVec { }
unsafe impl Send for AzStyledNodeVec { }
unsafe impl Send for AzTagIdsToNodeIdsMappingVec { }
unsafe impl Send for AzWaylandTheme { }
unsafe impl Send for AzStyleFontFamilyVec { }
unsafe impl Send for AzFmtArgVec { }
unsafe impl Send for AzInlineWordVec { }
unsafe impl Send for AzMonitorVec { }
unsafe impl Send for AzIdOrClassVec { }
unsafe impl Send for AzStyleBackgroundContentVec { }
unsafe impl Send for AzSvgPathVec { }
unsafe impl Send for AzVertexAttributeVec { }
unsafe impl Send for AzCssPathSelectorVec { }
unsafe impl Send for AzCallbackDataVec { }
unsafe impl Send for AzDebugMessageVec { }
unsafe impl Send for AzStringPairVec { }
unsafe impl Send for AzInlineLineVec { }
unsafe impl Send for AzCssPropertyVec { }
unsafe impl Send for AzSvgMultiPolygonVec { }
unsafe impl Send for AzCallbackInfo { }
unsafe impl Send for AzTimerCallbackInfo { }
unsafe impl Send for AzNodeDataInlineCssPropertyVec { }
unsafe impl Send for AzCssDeclarationVec { }
unsafe impl Send for AzNodeDataVec { }
unsafe impl Send for AzCssRuleBlockVec { }
unsafe impl Send for AzStylesheetVec { }


#[pymethods]
impl AzApp {
    #[staticmethod]
    fn new() -> PyResult<()> {
        Ok(())
    }
    fn add_window(&self) -> PyResult<()> {
        Ok(())
    }
    fn add_image(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_monitors(&self) -> PyResult<()> {
        Ok(())
    }
    fn run(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSystemCallbacks {
    #[staticmethod]
    fn library_internal() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzWindowCreateOptions {
    #[staticmethod]
    fn new() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzWindowState {
    #[staticmethod]
    fn new() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn default() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzCallbackInfo {
    fn get_hit_node(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_system_time_fn(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_cursor_relative_to_viewport(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_cursor_relative_to_node(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_current_window_state(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_current_keyboard_state(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_current_mouse_state(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_previous_window_state(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_previous_keyboard_state(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_previous_mouse_state(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_current_window_handle(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_gl_context(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_scroll_position(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_dataset(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_string_contents(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_inline_text(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_index_in_parent(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_parent(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_previous_sibling(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_next_sibling(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_first_child(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_last_child(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_node_position(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_node_size(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_computed_css_property(&self) -> PyResult<()> {
        Ok(())
    }
    fn set_window_state(&self) -> PyResult<()> {
        Ok(())
    }
    fn set_focus(&self) -> PyResult<()> {
        Ok(())
    }
    fn set_css_property(&self) -> PyResult<()> {
        Ok(())
    }
    fn set_scroll_position(&self) -> PyResult<()> {
        Ok(())
    }
    fn set_string_contents(&self) -> PyResult<()> {
        Ok(())
    }
    fn add_image(&self) -> PyResult<()> {
        Ok(())
    }
    fn has_image(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_image(&self) -> PyResult<()> {
        Ok(())
    }
    fn update_image(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_image(&self) -> PyResult<()> {
        Ok(())
    }
    fn update_image_mask(&self) -> PyResult<()> {
        Ok(())
    }
    fn stop_propagation(&self) -> PyResult<()> {
        Ok(())
    }
    fn create_window(&self) -> PyResult<()> {
        Ok(())
    }
    fn start_timer(&self) -> PyResult<()> {
        Ok(())
    }
    fn start_animation(&self) -> PyResult<()> {
        Ok(())
    }
    fn stop_timer(&self) -> PyResult<()> {
        Ok(())
    }
    fn start_thread(&self) -> PyResult<()> {
        Ok(())
    }
    fn send_thread_msg(&self) -> PyResult<()> {
        Ok(())
    }
    fn stop_thread(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzHidpiAdjustedBounds {
    fn get_logical_size(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_physical_size(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_hidpi_factor(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzInlineText {
    fn hit_test(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzRenderImageCallbackInfo {
    fn get_gl_context(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_bounds(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_callback_node_id(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_inline_text(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_index_in_parent(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_parent(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_previous_sibling(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_next_sibling(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_first_child(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_last_child(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzRefCount {
    fn can_be_shared(&self) -> PyResult<()> {
        Ok(())
    }
    fn can_be_shared_mut(&self) -> PyResult<()> {
        Ok(())
    }
    fn increase_ref(&self) -> PyResult<()> {
        Ok(())
    }
    fn decrease_ref(&self) -> PyResult<()> {
        Ok(())
    }
    fn increase_refmut(&self) -> PyResult<()> {
        Ok(())
    }
    fn decrease_refmut(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzRefAny {
    #[staticmethod]
    fn new_c() -> PyResult<()> {
        Ok(())
    }
    fn get_type_id(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_type_name(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzLayoutCallbackInfo {
    fn get_gl_context(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_system_fonts(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_image(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzDom {
    fn node_count(&self) -> PyResult<()> {
        Ok(())
    }
    fn style(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzCss {
    #[staticmethod]
    fn empty() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn from_string() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzColorU {
    #[staticmethod]
    fn from_str() -> PyResult<()> {
        Ok(())
    }
    fn to_hash(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzStyledDom {
    #[staticmethod]
    fn new() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn from_xml() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn from_file() -> PyResult<()> {
        Ok(())
    }
    fn append_child(&self) -> PyResult<()> {
        Ok(())
    }
    fn restyle(&self) -> PyResult<()> {
        Ok(())
    }
    fn node_count(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_html_string(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzTexture {
    #[staticmethod]
    fn allocate_clip_mask() -> PyResult<()> {
        Ok(())
    }
    fn draw_clip_mask(&self) -> PyResult<()> {
        Ok(())
    }
    fn apply_fxaa(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzGl {
    fn get_type(&self) -> PyResult<()> {
        Ok(())
    }
    fn buffer_data_untyped(&self) -> PyResult<()> {
        Ok(())
    }
    fn buffer_sub_data_untyped(&self) -> PyResult<()> {
        Ok(())
    }
    fn map_buffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn map_buffer_range(&self) -> PyResult<()> {
        Ok(())
    }
    fn unmap_buffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_buffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn shader_source(&self) -> PyResult<()> {
        Ok(())
    }
    fn read_buffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn read_pixels_into_buffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn read_pixels(&self) -> PyResult<()> {
        Ok(())
    }
    fn read_pixels_into_pbo(&self) -> PyResult<()> {
        Ok(())
    }
    fn sample_coverage(&self) -> PyResult<()> {
        Ok(())
    }
    fn polygon_offset(&self) -> PyResult<()> {
        Ok(())
    }
    fn pixel_store_i(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_buffers(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_renderbuffers(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_framebuffers(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_textures(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_vertex_arrays(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_queries(&self) -> PyResult<()> {
        Ok(())
    }
    fn begin_query(&self) -> PyResult<()> {
        Ok(())
    }
    fn end_query(&self) -> PyResult<()> {
        Ok(())
    }
    fn query_counter(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_query_object_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_query_object_uiv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_query_object_i64v(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_query_object_ui64v(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_queries(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_vertex_arrays(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_buffers(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_renderbuffers(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_framebuffers(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_textures(&self) -> PyResult<()> {
        Ok(())
    }
    fn framebuffer_renderbuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn renderbuffer_storage(&self) -> PyResult<()> {
        Ok(())
    }
    fn depth_func(&self) -> PyResult<()> {
        Ok(())
    }
    fn active_texture(&self) -> PyResult<()> {
        Ok(())
    }
    fn attach_shader(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_attrib_location(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_uniform_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_uniform_fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_uniform_block_index(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_uniform_indices(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_buffer_base(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_buffer_range(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_block_binding(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_buffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_vertex_array(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_renderbuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_framebuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_texture(&self) -> PyResult<()> {
        Ok(())
    }
    fn draw_buffers(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_image_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn compressed_tex_image_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn compressed_tex_sub_image_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_image_3d(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_tex_image_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_tex_sub_image_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_tex_sub_image_3d(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_sub_image_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_sub_image_2d_pbo(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_sub_image_3d(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_sub_image_3d_pbo(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_storage_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_storage_3d(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_tex_image_into_buffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_image_sub_data(&self) -> PyResult<()> {
        Ok(())
    }
    fn invalidate_framebuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn invalidate_sub_framebuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_integer_v(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_integer_64v(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_integer_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_integer_64iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_boolean_v(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_float_v(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_framebuffer_attachment_parameter_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_renderbuffer_parameter_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_tex_parameter_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_tex_parameter_fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_parameter_i(&self) -> PyResult<()> {
        Ok(())
    }
    fn tex_parameter_f(&self) -> PyResult<()> {
        Ok(())
    }
    fn framebuffer_texture_2d(&self) -> PyResult<()> {
        Ok(())
    }
    fn framebuffer_texture_layer(&self) -> PyResult<()> {
        Ok(())
    }
    fn blit_framebuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn vertex_attrib_4f(&self) -> PyResult<()> {
        Ok(())
    }
    fn vertex_attrib_pointer_f32(&self) -> PyResult<()> {
        Ok(())
    }
    fn vertex_attrib_pointer(&self) -> PyResult<()> {
        Ok(())
    }
    fn vertex_attrib_i_pointer(&self) -> PyResult<()> {
        Ok(())
    }
    fn vertex_attrib_divisor(&self) -> PyResult<()> {
        Ok(())
    }
    fn viewport(&self) -> PyResult<()> {
        Ok(())
    }
    fn scissor(&self) -> PyResult<()> {
        Ok(())
    }
    fn line_width(&self) -> PyResult<()> {
        Ok(())
    }
    fn use_program(&self) -> PyResult<()> {
        Ok(())
    }
    fn validate_program(&self) -> PyResult<()> {
        Ok(())
    }
    fn draw_arrays(&self) -> PyResult<()> {
        Ok(())
    }
    fn draw_arrays_instanced(&self) -> PyResult<()> {
        Ok(())
    }
    fn draw_elements(&self) -> PyResult<()> {
        Ok(())
    }
    fn draw_elements_instanced(&self) -> PyResult<()> {
        Ok(())
    }
    fn blend_color(&self) -> PyResult<()> {
        Ok(())
    }
    fn blend_func(&self) -> PyResult<()> {
        Ok(())
    }
    fn blend_func_separate(&self) -> PyResult<()> {
        Ok(())
    }
    fn blend_equation(&self) -> PyResult<()> {
        Ok(())
    }
    fn blend_equation_separate(&self) -> PyResult<()> {
        Ok(())
    }
    fn color_mask(&self) -> PyResult<()> {
        Ok(())
    }
    fn cull_face(&self) -> PyResult<()> {
        Ok(())
    }
    fn front_face(&self) -> PyResult<()> {
        Ok(())
    }
    fn enable(&self) -> PyResult<()> {
        Ok(())
    }
    fn disable(&self) -> PyResult<()> {
        Ok(())
    }
    fn hint(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_enabled(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_shader(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_texture(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_framebuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_renderbuffer(&self) -> PyResult<()> {
        Ok(())
    }
    fn check_frame_buffer_status(&self) -> PyResult<()> {
        Ok(())
    }
    fn enable_vertex_attrib_array(&self) -> PyResult<()> {
        Ok(())
    }
    fn disable_vertex_attrib_array(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_1f(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_1fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_1i(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_1iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_1ui(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_2f(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_2fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_2i(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_2iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_2ui(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_3f(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_3fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_3i(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_3iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_3ui(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_4f(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_4i(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_4iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_4ui(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_4fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_matrix_2fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_matrix_3fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn uniform_matrix_4fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn depth_mask(&self) -> PyResult<()> {
        Ok(())
    }
    fn depth_range(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_active_attrib(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_active_uniform(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_active_uniforms_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_active_uniform_block_i(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_active_uniform_block_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_active_uniform_block_name(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_attrib_location(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_frag_data_location(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_uniform_location(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_program_info_log(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_program_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_program_binary(&self) -> PyResult<()> {
        Ok(())
    }
    fn program_binary(&self) -> PyResult<()> {
        Ok(())
    }
    fn program_parameter_i(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_vertex_attrib_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_vertex_attrib_fv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_vertex_attrib_pointer_v(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_buffer_parameter_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_shader_info_log(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_string(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_string_i(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_shader_iv(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_shader_precision_format(&self) -> PyResult<()> {
        Ok(())
    }
    fn compile_shader(&self) -> PyResult<()> {
        Ok(())
    }
    fn create_program(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_program(&self) -> PyResult<()> {
        Ok(())
    }
    fn create_shader(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_shader(&self) -> PyResult<()> {
        Ok(())
    }
    fn detach_shader(&self) -> PyResult<()> {
        Ok(())
    }
    fn link_program(&self) -> PyResult<()> {
        Ok(())
    }
    fn clear_color(&self) -> PyResult<()> {
        Ok(())
    }
    fn clear(&self) -> PyResult<()> {
        Ok(())
    }
    fn clear_depth(&self) -> PyResult<()> {
        Ok(())
    }
    fn clear_stencil(&self) -> PyResult<()> {
        Ok(())
    }
    fn flush(&self) -> PyResult<()> {
        Ok(())
    }
    fn finish(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_error(&self) -> PyResult<()> {
        Ok(())
    }
    fn stencil_mask(&self) -> PyResult<()> {
        Ok(())
    }
    fn stencil_mask_separate(&self) -> PyResult<()> {
        Ok(())
    }
    fn stencil_func(&self) -> PyResult<()> {
        Ok(())
    }
    fn stencil_func_separate(&self) -> PyResult<()> {
        Ok(())
    }
    fn stencil_op(&self) -> PyResult<()> {
        Ok(())
    }
    fn stencil_op_separate(&self) -> PyResult<()> {
        Ok(())
    }
    fn egl_image_target_texture2d_oes(&self) -> PyResult<()> {
        Ok(())
    }
    fn generate_mipmap(&self) -> PyResult<()> {
        Ok(())
    }
    fn insert_event_marker_ext(&self) -> PyResult<()> {
        Ok(())
    }
    fn push_group_marker_ext(&self) -> PyResult<()> {
        Ok(())
    }
    fn pop_group_marker_ext(&self) -> PyResult<()> {
        Ok(())
    }
    fn debug_message_insert_khr(&self) -> PyResult<()> {
        Ok(())
    }
    fn push_debug_group_khr(&self) -> PyResult<()> {
        Ok(())
    }
    fn pop_debug_group_khr(&self) -> PyResult<()> {
        Ok(())
    }
    fn fence_sync(&self) -> PyResult<()> {
        Ok(())
    }
    fn client_wait_sync(&self) -> PyResult<()> {
        Ok(())
    }
    fn wait_sync(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_sync(&self) -> PyResult<()> {
        Ok(())
    }
    fn texture_range_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_fences_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_fences_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn set_fence_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn finish_fence_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn test_fence_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn test_object_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn finish_object_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_frag_data_index(&self) -> PyResult<()> {
        Ok(())
    }
    fn blend_barrier_khr(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_frag_data_location_indexed(&self) -> PyResult<()> {
        Ok(())
    }
    fn get_debug_messages(&self) -> PyResult<()> {
        Ok(())
    }
    fn provoking_vertex_angle(&self) -> PyResult<()> {
        Ok(())
    }
    fn gen_vertex_arrays_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn bind_vertex_array_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn delete_vertex_arrays_apple(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_texture_chromium(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_sub_texture_chromium(&self) -> PyResult<()> {
        Ok(())
    }
    fn egl_image_target_renderbuffer_storage_oes(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_texture_3d_angle(&self) -> PyResult<()> {
        Ok(())
    }
    fn copy_sub_texture_3d_angle(&self) -> PyResult<()> {
        Ok(())
    }
    fn buffer_storage(&self) -> PyResult<()> {
        Ok(())
    }
    fn flush_mapped_buffer_range(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzTextureFlags {
    #[staticmethod]
    fn default() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzImageRef {
    #[staticmethod]
    fn invalid() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn raw_image() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn gl_texture() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn callback() -> PyResult<()> {
        Ok(())
    }
    fn clone_bytes(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_invalid(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_gl_texture(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_raw_image(&self) -> PyResult<()> {
        Ok(())
    }
    fn is_callback(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzRawImage {
    #[staticmethod]
    fn empty() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn allocate_clip_mask() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn decode_image_bytes_any() -> PyResult<()> {
        Ok(())
    }
    fn draw_clip_mask(&self) -> PyResult<()> {
        Ok(())
    }
    fn encode_bmp(&self) -> PyResult<()> {
        Ok(())
    }
    fn encode_png(&self) -> PyResult<()> {
        Ok(())
    }
    fn encode_jpeg(&self) -> PyResult<()> {
        Ok(())
    }
    fn encode_tga(&self) -> PyResult<()> {
        Ok(())
    }
    fn encode_pnm(&self) -> PyResult<()> {
        Ok(())
    }
    fn encode_gif(&self) -> PyResult<()> {
        Ok(())
    }
    fn encode_tiff(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzFontRef {
    #[staticmethod]
    fn parse() -> PyResult<()> {
        Ok(())
    }
    fn get_font_metrics(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvg {
    #[staticmethod]
    fn from_string() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn from_bytes() -> PyResult<()> {
        Ok(())
    }
    fn get_root(&self) -> PyResult<()> {
        Ok(())
    }
    fn render(&self) -> PyResult<()> {
        Ok(())
    }
    fn to_string(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgXmlNode {
    #[staticmethod]
    fn parse_from() -> PyResult<()> {
        Ok(())
    }
    fn render(&self) -> PyResult<()> {
        Ok(())
    }
    fn to_string(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgMultiPolygon {
    fn tesselate_fill(&self) -> PyResult<()> {
        Ok(())
    }
    fn tesselate_stroke(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgStyledNode {
    fn tesselate(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgCircle {
    fn tesselate_fill(&self) -> PyResult<()> {
        Ok(())
    }
    fn tesselate_stroke(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgPath {
    fn tesselate_fill(&self) -> PyResult<()> {
        Ok(())
    }
    fn tesselate_stroke(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgRect {
    fn tesselate_fill(&self) -> PyResult<()> {
        Ok(())
    }
    fn tesselate_stroke(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzTesselatedSvgNode {
    #[staticmethod]
    fn empty() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn from_nodes() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgParseOptions {
    #[staticmethod]
    fn default() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSvgRenderOptions {
    #[staticmethod]
    fn default() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzXml {
    #[staticmethod]
    fn from_str() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzFile {
    #[staticmethod]
    fn open() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn create() -> PyResult<()> {
        Ok(())
    }
    fn read_to_string(&self) -> PyResult<()> {
        Ok(())
    }
    fn read_to_bytes(&self) -> PyResult<()> {
        Ok(())
    }
    fn write_string(&self) -> PyResult<()> {
        Ok(())
    }
    fn write_bytes(&self) -> PyResult<()> {
        Ok(())
    }
    fn close(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzMsgBox {
    #[staticmethod]
    fn ok() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn ok_cancel() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn yes_no() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzFileDialog {
    #[staticmethod]
    fn select_file() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn select_multiple_files() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn select_folder() -> PyResult<()> {
        Ok(())
    }
    #[staticmethod]
    fn save_file() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzColorPickerDialog {
    #[staticmethod]
    fn open() -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzSystemClipboard {
    #[staticmethod]
    fn new() -> PyResult<()> {
        Ok(())
    }
    fn get_string_contents(&self) -> PyResult<()> {
        Ok(())
    }
    fn set_string_contents(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzTimer {
    #[staticmethod]
    fn new() -> PyResult<()> {
        Ok(())
    }
    fn with_delay(&self) -> PyResult<()> {
        Ok(())
    }
    fn with_interval(&self) -> PyResult<()> {
        Ok(())
    }
    fn with_timeout(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzThreadSender {
    fn send(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzThreadReceiver {
    fn receive(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzString {
    #[staticmethod]
    fn format() -> PyResult<()> {
        Ok(())
    }
    fn trim(&self) -> PyResult<()> {
        Ok(())
    }
    fn as_refstr(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzTesselatedSvgNodeVec {
    fn as_ref_vec(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymethods]
impl AzU8Vec {
    fn as_ref_vec(&self) -> PyResult<()> {
        Ok(())
    }
}

#[pymodule]
fn azul(py: Python, m: &PyModule) -> PyResult<()> {

    m.add_class::<AzApp>()?;
    m.add_class::<AzAppConfig>()?;
    m.add_class::<AzAppLogLevelEnumWrapper>()?;
    m.add_class::<AzLayoutSolverVersionEnumWrapper>()?;
    m.add_class::<AzSystemCallbacks>()?;

    m.add_class::<AzWindowCreateOptions>()?;
    m.add_class::<AzRendererOptions>()?;
    m.add_class::<AzVsyncEnumWrapper>()?;
    m.add_class::<AzSrgbEnumWrapper>()?;
    m.add_class::<AzHwAccelerationEnumWrapper>()?;
    m.add_class::<AzLayoutPoint>()?;
    m.add_class::<AzLayoutSize>()?;
    m.add_class::<AzLayoutRect>()?;
    m.add_class::<AzRawWindowHandleEnumWrapper>()?;
    m.add_class::<AzIOSHandle>()?;
    m.add_class::<AzMacOSHandle>()?;
    m.add_class::<AzXlibHandle>()?;
    m.add_class::<AzXcbHandle>()?;
    m.add_class::<AzWaylandHandle>()?;
    m.add_class::<AzWindowsHandle>()?;
    m.add_class::<AzWebHandle>()?;
    m.add_class::<AzAndroidHandle>()?;
    m.add_class::<AzXWindowTypeEnumWrapper>()?;
    m.add_class::<AzPhysicalPositionI32>()?;
    m.add_class::<AzPhysicalSizeU32>()?;
    m.add_class::<AzLogicalRect>()?;
    m.add_class::<AzLogicalPosition>()?;
    m.add_class::<AzLogicalSize>()?;
    m.add_class::<AzIconKey>()?;
    m.add_class::<AzSmallWindowIconBytes>()?;
    m.add_class::<AzLargeWindowIconBytes>()?;
    m.add_class::<AzWindowIconEnumWrapper>()?;
    m.add_class::<AzTaskBarIcon>()?;
    m.add_class::<AzVirtualKeyCodeEnumWrapper>()?;
    m.add_class::<AzAcceleratorKeyEnumWrapper>()?;
    m.add_class::<AzWindowSize>()?;
    m.add_class::<AzWindowFlags>()?;
    m.add_class::<AzDebugState>()?;
    m.add_class::<AzKeyboardState>()?;
    m.add_class::<AzMouseCursorTypeEnumWrapper>()?;
    m.add_class::<AzCursorPositionEnumWrapper>()?;
    m.add_class::<AzMouseState>()?;
    m.add_class::<AzPlatformSpecificOptions>()?;
    m.add_class::<AzWindowsWindowOptions>()?;
    m.add_class::<AzWaylandTheme>()?;
    m.add_class::<AzRendererTypeEnumWrapper>()?;
    m.add_class::<AzStringPair>()?;
    m.add_class::<AzLinuxWindowOptions>()?;
    m.add_class::<AzMacWindowOptions>()?;
    m.add_class::<AzWasmWindowOptions>()?;
    m.add_class::<AzFullScreenModeEnumWrapper>()?;
    m.add_class::<AzWindowThemeEnumWrapper>()?;
    m.add_class::<AzWindowPositionEnumWrapper>()?;
    m.add_class::<AzImePositionEnumWrapper>()?;
    m.add_class::<AzTouchState>()?;
    m.add_class::<AzMonitor>()?;
    m.add_class::<AzVideoMode>()?;
    m.add_class::<AzWindowState>()?;

    m.add_class::<AzLayoutCallback>()?;
    m.add_class::<AzCallback>()?;
    m.add_class::<AzCallbackInfo>()?;
    m.add_class::<AzUpdateScreenEnumWrapper>()?;
    m.add_class::<AzNodeId>()?;
    m.add_class::<AzDomId>()?;
    m.add_class::<AzDomNodeId>()?;
    m.add_class::<AzPositionInfoEnumWrapper>()?;
    m.add_class::<AzPositionInfoInner>()?;
    m.add_class::<AzHidpiAdjustedBounds>()?;
    m.add_class::<AzInlineText>()?;
    m.add_class::<AzInlineLine>()?;
    m.add_class::<AzInlineWordEnumWrapper>()?;
    m.add_class::<AzInlineTextContents>()?;
    m.add_class::<AzInlineGlyph>()?;
    m.add_class::<AzInlineTextHit>()?;
    m.add_class::<AzFocusTargetEnumWrapper>()?;
    m.add_class::<AzFocusTargetPath>()?;
    m.add_class::<AzAnimation>()?;
    m.add_class::<AzAnimationRepeatEnumWrapper>()?;
    m.add_class::<AzAnimationRepeatCountEnumWrapper>()?;
    m.add_class::<AzAnimationEasingEnumWrapper>()?;
    m.add_class::<AzIFrameCallback>()?;
    m.add_class::<AzIFrameCallbackInfo>()?;
    m.add_class::<AzIFrameCallbackReturn>()?;
    m.add_class::<AzRenderImageCallback>()?;
    m.add_class::<AzRenderImageCallbackInfo>()?;
    m.add_class::<AzTimerCallback>()?;
    m.add_class::<AzTimerCallbackInfo>()?;
    m.add_class::<AzTimerCallbackReturn>()?;
    m.add_class::<AzWriteBackCallback>()?;
    m.add_class::<AzThreadCallback>()?;
    m.add_class::<AzRefCount>()?;
    m.add_class::<AzRefAny>()?;
    m.add_class::<AzLayoutCallbackInfo>()?;

    m.add_class::<AzDom>()?;
    m.add_class::<AzIFrameNode>()?;
    m.add_class::<AzCallbackData>()?;
    m.add_class::<AzNodeData>()?;
    m.add_class::<AzNodeTypeEnumWrapper>()?;
    m.add_class::<AzOnEnumWrapper>()?;
    m.add_class::<AzEventFilterEnumWrapper>()?;
    m.add_class::<AzHoverEventFilterEnumWrapper>()?;
    m.add_class::<AzFocusEventFilterEnumWrapper>()?;
    m.add_class::<AzNotEventFilterEnumWrapper>()?;
    m.add_class::<AzWindowEventFilterEnumWrapper>()?;
    m.add_class::<AzComponentEventFilterEnumWrapper>()?;
    m.add_class::<AzApplicationEventFilterEnumWrapper>()?;
    m.add_class::<AzTabIndexEnumWrapper>()?;
    m.add_class::<AzIdOrClassEnumWrapper>()?;
    m.add_class::<AzNodeDataInlineCssPropertyEnumWrapper>()?;

    m.add_class::<AzCssRuleBlock>()?;
    m.add_class::<AzCssDeclarationEnumWrapper>()?;
    m.add_class::<AzDynamicCssProperty>()?;
    m.add_class::<AzCssPath>()?;
    m.add_class::<AzCssPathSelectorEnumWrapper>()?;
    m.add_class::<AzNodeTypeKeyEnumWrapper>()?;
    m.add_class::<AzCssPathPseudoSelectorEnumWrapper>()?;
    m.add_class::<AzCssNthChildSelectorEnumWrapper>()?;
    m.add_class::<AzCssNthChildPattern>()?;
    m.add_class::<AzStylesheet>()?;
    m.add_class::<AzCss>()?;
    m.add_class::<AzCssPropertyTypeEnumWrapper>()?;
    m.add_class::<AzAnimationInterpolationFunctionEnumWrapper>()?;
    m.add_class::<AzInterpolateContext>()?;
    m.add_class::<AzColorU>()?;
    m.add_class::<AzSizeMetricEnumWrapper>()?;
    m.add_class::<AzFloatValue>()?;
    m.add_class::<AzPixelValue>()?;
    m.add_class::<AzPixelValueNoPercent>()?;
    m.add_class::<AzBoxShadowClipModeEnumWrapper>()?;
    m.add_class::<AzStyleBoxShadow>()?;
    m.add_class::<AzLayoutAlignContentEnumWrapper>()?;
    m.add_class::<AzLayoutAlignItemsEnumWrapper>()?;
    m.add_class::<AzLayoutBottom>()?;
    m.add_class::<AzLayoutBoxSizingEnumWrapper>()?;
    m.add_class::<AzLayoutFlexDirectionEnumWrapper>()?;
    m.add_class::<AzLayoutDisplayEnumWrapper>()?;
    m.add_class::<AzLayoutFlexGrow>()?;
    m.add_class::<AzLayoutFlexShrink>()?;
    m.add_class::<AzLayoutFloatEnumWrapper>()?;
    m.add_class::<AzLayoutHeight>()?;
    m.add_class::<AzLayoutJustifyContentEnumWrapper>()?;
    m.add_class::<AzLayoutLeft>()?;
    m.add_class::<AzLayoutMarginBottom>()?;
    m.add_class::<AzLayoutMarginLeft>()?;
    m.add_class::<AzLayoutMarginRight>()?;
    m.add_class::<AzLayoutMarginTop>()?;
    m.add_class::<AzLayoutMaxHeight>()?;
    m.add_class::<AzLayoutMaxWidth>()?;
    m.add_class::<AzLayoutMinHeight>()?;
    m.add_class::<AzLayoutMinWidth>()?;
    m.add_class::<AzLayoutPaddingBottom>()?;
    m.add_class::<AzLayoutPaddingLeft>()?;
    m.add_class::<AzLayoutPaddingRight>()?;
    m.add_class::<AzLayoutPaddingTop>()?;
    m.add_class::<AzLayoutPositionEnumWrapper>()?;
    m.add_class::<AzLayoutRight>()?;
    m.add_class::<AzLayoutTop>()?;
    m.add_class::<AzLayoutWidth>()?;
    m.add_class::<AzLayoutFlexWrapEnumWrapper>()?;
    m.add_class::<AzLayoutOverflowEnumWrapper>()?;
    m.add_class::<AzPercentageValue>()?;
    m.add_class::<AzAngleMetricEnumWrapper>()?;
    m.add_class::<AzAngleValue>()?;
    m.add_class::<AzNormalizedLinearColorStop>()?;
    m.add_class::<AzNormalizedRadialColorStop>()?;
    m.add_class::<AzDirectionCornerEnumWrapper>()?;
    m.add_class::<AzDirectionCorners>()?;
    m.add_class::<AzDirectionEnumWrapper>()?;
    m.add_class::<AzExtendModeEnumWrapper>()?;
    m.add_class::<AzLinearGradient>()?;
    m.add_class::<AzShapeEnumWrapper>()?;
    m.add_class::<AzRadialGradientSizeEnumWrapper>()?;
    m.add_class::<AzRadialGradient>()?;
    m.add_class::<AzConicGradient>()?;
    m.add_class::<AzStyleBackgroundContentEnumWrapper>()?;
    m.add_class::<AzBackgroundPositionHorizontalEnumWrapper>()?;
    m.add_class::<AzBackgroundPositionVerticalEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundPosition>()?;
    m.add_class::<AzStyleBackgroundRepeatEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundSizeEnumWrapper>()?;
    m.add_class::<AzStyleBorderBottomColor>()?;
    m.add_class::<AzStyleBorderBottomLeftRadius>()?;
    m.add_class::<AzStyleBorderBottomRightRadius>()?;
    m.add_class::<AzBorderStyleEnumWrapper>()?;
    m.add_class::<AzStyleBorderBottomStyle>()?;
    m.add_class::<AzLayoutBorderBottomWidth>()?;
    m.add_class::<AzStyleBorderLeftColor>()?;
    m.add_class::<AzStyleBorderLeftStyle>()?;
    m.add_class::<AzLayoutBorderLeftWidth>()?;
    m.add_class::<AzStyleBorderRightColor>()?;
    m.add_class::<AzStyleBorderRightStyle>()?;
    m.add_class::<AzLayoutBorderRightWidth>()?;
    m.add_class::<AzStyleBorderTopColor>()?;
    m.add_class::<AzStyleBorderTopLeftRadius>()?;
    m.add_class::<AzStyleBorderTopRightRadius>()?;
    m.add_class::<AzStyleBorderTopStyle>()?;
    m.add_class::<AzLayoutBorderTopWidth>()?;
    m.add_class::<AzScrollbarInfo>()?;
    m.add_class::<AzScrollbarStyle>()?;
    m.add_class::<AzStyleCursorEnumWrapper>()?;
    m.add_class::<AzStyleFontFamilyEnumWrapper>()?;
    m.add_class::<AzStyleFontSize>()?;
    m.add_class::<AzStyleLetterSpacing>()?;
    m.add_class::<AzStyleLineHeight>()?;
    m.add_class::<AzStyleTabWidth>()?;
    m.add_class::<AzStyleOpacity>()?;
    m.add_class::<AzStyleTransformOrigin>()?;
    m.add_class::<AzStylePerspectiveOrigin>()?;
    m.add_class::<AzStyleBackfaceVisibilityEnumWrapper>()?;
    m.add_class::<AzStyleTransformEnumWrapper>()?;
    m.add_class::<AzStyleTransformMatrix2D>()?;
    m.add_class::<AzStyleTransformMatrix3D>()?;
    m.add_class::<AzStyleTransformTranslate2D>()?;
    m.add_class::<AzStyleTransformTranslate3D>()?;
    m.add_class::<AzStyleTransformRotate3D>()?;
    m.add_class::<AzStyleTransformScale2D>()?;
    m.add_class::<AzStyleTransformScale3D>()?;
    m.add_class::<AzStyleTransformSkew2D>()?;
    m.add_class::<AzStyleTextAlignEnumWrapper>()?;
    m.add_class::<AzStyleTextColor>()?;
    m.add_class::<AzStyleWordSpacing>()?;
    m.add_class::<AzStyleBoxShadowValueEnumWrapper>()?;
    m.add_class::<AzLayoutAlignContentValueEnumWrapper>()?;
    m.add_class::<AzLayoutAlignItemsValueEnumWrapper>()?;
    m.add_class::<AzLayoutBottomValueEnumWrapper>()?;
    m.add_class::<AzLayoutBoxSizingValueEnumWrapper>()?;
    m.add_class::<AzLayoutFlexDirectionValueEnumWrapper>()?;
    m.add_class::<AzLayoutDisplayValueEnumWrapper>()?;
    m.add_class::<AzLayoutFlexGrowValueEnumWrapper>()?;
    m.add_class::<AzLayoutFlexShrinkValueEnumWrapper>()?;
    m.add_class::<AzLayoutFloatValueEnumWrapper>()?;
    m.add_class::<AzLayoutHeightValueEnumWrapper>()?;
    m.add_class::<AzLayoutJustifyContentValueEnumWrapper>()?;
    m.add_class::<AzLayoutLeftValueEnumWrapper>()?;
    m.add_class::<AzLayoutMarginBottomValueEnumWrapper>()?;
    m.add_class::<AzLayoutMarginLeftValueEnumWrapper>()?;
    m.add_class::<AzLayoutMarginRightValueEnumWrapper>()?;
    m.add_class::<AzLayoutMarginTopValueEnumWrapper>()?;
    m.add_class::<AzLayoutMaxHeightValueEnumWrapper>()?;
    m.add_class::<AzLayoutMaxWidthValueEnumWrapper>()?;
    m.add_class::<AzLayoutMinHeightValueEnumWrapper>()?;
    m.add_class::<AzLayoutMinWidthValueEnumWrapper>()?;
    m.add_class::<AzLayoutPaddingBottomValueEnumWrapper>()?;
    m.add_class::<AzLayoutPaddingLeftValueEnumWrapper>()?;
    m.add_class::<AzLayoutPaddingRightValueEnumWrapper>()?;
    m.add_class::<AzLayoutPaddingTopValueEnumWrapper>()?;
    m.add_class::<AzLayoutPositionValueEnumWrapper>()?;
    m.add_class::<AzLayoutRightValueEnumWrapper>()?;
    m.add_class::<AzLayoutTopValueEnumWrapper>()?;
    m.add_class::<AzLayoutWidthValueEnumWrapper>()?;
    m.add_class::<AzLayoutFlexWrapValueEnumWrapper>()?;
    m.add_class::<AzLayoutOverflowValueEnumWrapper>()?;
    m.add_class::<AzScrollbarStyleValueEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundContentVecValueEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundPositionVecValueEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundRepeatVecValueEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundSizeVecValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderBottomColorValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderBottomLeftRadiusValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderBottomRightRadiusValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderBottomStyleValueEnumWrapper>()?;
    m.add_class::<AzLayoutBorderBottomWidthValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderLeftColorValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderLeftStyleValueEnumWrapper>()?;
    m.add_class::<AzLayoutBorderLeftWidthValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderRightColorValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderRightStyleValueEnumWrapper>()?;
    m.add_class::<AzLayoutBorderRightWidthValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderTopColorValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderTopLeftRadiusValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderTopRightRadiusValueEnumWrapper>()?;
    m.add_class::<AzStyleBorderTopStyleValueEnumWrapper>()?;
    m.add_class::<AzLayoutBorderTopWidthValueEnumWrapper>()?;
    m.add_class::<AzStyleCursorValueEnumWrapper>()?;
    m.add_class::<AzStyleFontFamilyVecValueEnumWrapper>()?;
    m.add_class::<AzStyleFontSizeValueEnumWrapper>()?;
    m.add_class::<AzStyleLetterSpacingValueEnumWrapper>()?;
    m.add_class::<AzStyleLineHeightValueEnumWrapper>()?;
    m.add_class::<AzStyleTabWidthValueEnumWrapper>()?;
    m.add_class::<AzStyleTextAlignValueEnumWrapper>()?;
    m.add_class::<AzStyleTextColorValueEnumWrapper>()?;
    m.add_class::<AzStyleWordSpacingValueEnumWrapper>()?;
    m.add_class::<AzStyleOpacityValueEnumWrapper>()?;
    m.add_class::<AzStyleTransformVecValueEnumWrapper>()?;
    m.add_class::<AzStyleTransformOriginValueEnumWrapper>()?;
    m.add_class::<AzStylePerspectiveOriginValueEnumWrapper>()?;
    m.add_class::<AzStyleBackfaceVisibilityValueEnumWrapper>()?;
    m.add_class::<AzCssPropertyEnumWrapper>()?;

    m.add_class::<AzNode>()?;
    m.add_class::<AzCascadeInfo>()?;
    m.add_class::<AzCssPropertySourceEnumWrapper>()?;
    m.add_class::<AzStyledNodeState>()?;
    m.add_class::<AzStyledNode>()?;
    m.add_class::<AzTagId>()?;
    m.add_class::<AzTagIdToNodeIdMapping>()?;
    m.add_class::<AzParentWithNodeDepth>()?;
    m.add_class::<AzCssPropertyCache>()?;
    m.add_class::<AzStyledDom>()?;

    m.add_class::<AzTexture>()?;
    m.add_class::<AzGl>()?;
    m.add_class::<AzGlShaderPrecisionFormatReturn>()?;
    m.add_class::<AzVertexAttributeTypeEnumWrapper>()?;
    m.add_class::<AzVertexAttribute>()?;
    m.add_class::<AzVertexLayout>()?;
    m.add_class::<AzVertexArrayObject>()?;
    m.add_class::<AzIndexBufferFormatEnumWrapper>()?;
    m.add_class::<AzVertexBuffer>()?;
    m.add_class::<AzGlTypeEnumWrapper>()?;
    m.add_class::<AzDebugMessage>()?;
    m.add_class::<AzU8VecRef>()?;
    m.add_class::<AzU8VecRefMut>()?;
    m.add_class::<AzF32VecRef>()?;
    m.add_class::<AzI32VecRef>()?;
    m.add_class::<AzGLuintVecRef>()?;
    m.add_class::<AzGLenumVecRef>()?;
    m.add_class::<AzGLintVecRefMut>()?;
    m.add_class::<AzGLint64VecRefMut>()?;
    m.add_class::<AzGLbooleanVecRefMut>()?;
    m.add_class::<AzGLfloatVecRefMut>()?;
    m.add_class::<AzRefstrVecRef>()?;
    m.add_class::<AzRefstr>()?;
    m.add_class::<AzGetProgramBinaryReturn>()?;
    m.add_class::<AzGetActiveAttribReturn>()?;
    m.add_class::<AzGLsyncPtr>()?;
    m.add_class::<AzGetActiveUniformReturn>()?;
    m.add_class::<AzTextureFlags>()?;

    m.add_class::<AzImageRef>()?;
    m.add_class::<AzRawImage>()?;
    m.add_class::<AzImageMask>()?;
    m.add_class::<AzRawImageFormatEnumWrapper>()?;
    m.add_class::<AzEncodeImageErrorEnumWrapper>()?;
    m.add_class::<AzDecodeImageErrorEnumWrapper>()?;
    m.add_class::<AzRawImageDataEnumWrapper>()?;

    m.add_class::<AzFontMetrics>()?;
    m.add_class::<AzFontSource>()?;
    m.add_class::<AzFontRef>()?;

    m.add_class::<AzSvg>()?;
    m.add_class::<AzSvgXmlNode>()?;
    m.add_class::<AzSvgMultiPolygon>()?;
    m.add_class::<AzSvgNodeEnumWrapper>()?;
    m.add_class::<AzSvgStyledNode>()?;
    m.add_class::<AzSvgCircle>()?;
    m.add_class::<AzSvgPath>()?;
    m.add_class::<AzSvgPathElementEnumWrapper>()?;
    m.add_class::<AzSvgLine>()?;
    m.add_class::<AzSvgPoint>()?;
    m.add_class::<AzSvgQuadraticCurve>()?;
    m.add_class::<AzSvgCubicCurve>()?;
    m.add_class::<AzSvgRect>()?;
    m.add_class::<AzSvgVertex>()?;
    m.add_class::<AzTesselatedSvgNode>()?;
    m.add_class::<AzTesselatedSvgNodeVecRef>()?;
    m.add_class::<AzSvgParseOptions>()?;
    m.add_class::<AzShapeRenderingEnumWrapper>()?;
    m.add_class::<AzTextRenderingEnumWrapper>()?;
    m.add_class::<AzImageRenderingEnumWrapper>()?;
    m.add_class::<AzFontDatabaseEnumWrapper>()?;
    m.add_class::<AzSvgRenderOptions>()?;
    m.add_class::<AzSvgStringFormatOptions>()?;
    m.add_class::<AzIndentEnumWrapper>()?;
    m.add_class::<AzSvgFitToEnumWrapper>()?;
    m.add_class::<AzSvgStyleEnumWrapper>()?;
    m.add_class::<AzSvgFillRuleEnumWrapper>()?;
    m.add_class::<AzSvgTransform>()?;
    m.add_class::<AzSvgFillStyle>()?;
    m.add_class::<AzSvgStrokeStyle>()?;
    m.add_class::<AzSvgLineJoinEnumWrapper>()?;
    m.add_class::<AzSvgLineCapEnumWrapper>()?;
    m.add_class::<AzSvgDashPattern>()?;

    m.add_class::<AzXml>()?;
    m.add_class::<AzXmlNode>()?;

    m.add_class::<AzFile>()?;

    m.add_class::<AzMsgBox>()?;
    m.add_class::<AzMsgBoxIconEnumWrapper>()?;
    m.add_class::<AzMsgBoxYesNoEnumWrapper>()?;
    m.add_class::<AzMsgBoxOkCancelEnumWrapper>()?;
    m.add_class::<AzFileDialog>()?;
    m.add_class::<AzFileTypeList>()?;
    m.add_class::<AzColorPickerDialog>()?;

    m.add_class::<AzSystemClipboard>()?;

    m.add_class::<AzInstantEnumWrapper>()?;
    m.add_class::<AzInstantPtr>()?;
    m.add_class::<AzInstantPtrCloneFn>()?;
    m.add_class::<AzInstantPtrDestructorFn>()?;
    m.add_class::<AzSystemTick>()?;
    m.add_class::<AzDurationEnumWrapper>()?;
    m.add_class::<AzSystemTimeDiff>()?;
    m.add_class::<AzSystemTickDiff>()?;

    m.add_class::<AzTimerId>()?;
    m.add_class::<AzTimer>()?;
    m.add_class::<AzTerminateTimerEnumWrapper>()?;
    m.add_class::<AzThreadId>()?;
    m.add_class::<AzThread>()?;
    m.add_class::<AzThreadSender>()?;
    m.add_class::<AzThreadReceiver>()?;
    m.add_class::<AzThreadSendMsgEnumWrapper>()?;
    m.add_class::<AzThreadReceiveMsgEnumWrapper>()?;
    m.add_class::<AzThreadWriteBackMsg>()?;
    m.add_class::<AzCreateThreadFn>()?;
    m.add_class::<AzGetSystemTimeFn>()?;
    m.add_class::<AzCheckThreadFinishedFn>()?;
    m.add_class::<AzLibrarySendThreadMsgFn>()?;
    m.add_class::<AzLibraryReceiveThreadMsgFn>()?;
    m.add_class::<AzThreadRecvFn>()?;
    m.add_class::<AzThreadSendFn>()?;
    m.add_class::<AzThreadDestructorFn>()?;
    m.add_class::<AzThreadReceiverDestructorFn>()?;
    m.add_class::<AzThreadSenderDestructorFn>()?;

    m.add_class::<AzFmtValueEnumWrapper>()?;
    m.add_class::<AzFmtArg>()?;
    m.add_class::<AzString>()?;

    m.add_class::<AzTesselatedSvgNodeVec>()?;
    m.add_class::<AzStyleFontFamilyVec>()?;
    m.add_class::<AzXmlNodeVec>()?;
    m.add_class::<AzFmtArgVec>()?;
    m.add_class::<AzInlineLineVec>()?;
    m.add_class::<AzInlineWordVec>()?;
    m.add_class::<AzInlineGlyphVec>()?;
    m.add_class::<AzInlineTextHitVec>()?;
    m.add_class::<AzMonitorVec>()?;
    m.add_class::<AzVideoModeVec>()?;
    m.add_class::<AzDomVec>()?;
    m.add_class::<AzIdOrClassVec>()?;
    m.add_class::<AzNodeDataInlineCssPropertyVec>()?;
    m.add_class::<AzStyleBackgroundContentVec>()?;
    m.add_class::<AzStyleBackgroundPositionVec>()?;
    m.add_class::<AzStyleBackgroundRepeatVec>()?;
    m.add_class::<AzStyleBackgroundSizeVec>()?;
    m.add_class::<AzStyleTransformVec>()?;
    m.add_class::<AzCssPropertyVec>()?;
    m.add_class::<AzSvgMultiPolygonVec>()?;
    m.add_class::<AzSvgPathVec>()?;
    m.add_class::<AzVertexAttributeVec>()?;
    m.add_class::<AzSvgPathElementVec>()?;
    m.add_class::<AzSvgVertexVec>()?;
    m.add_class::<AzU32Vec>()?;
    m.add_class::<AzXWindowTypeVec>()?;
    m.add_class::<AzVirtualKeyCodeVec>()?;
    m.add_class::<AzCascadeInfoVec>()?;
    m.add_class::<AzScanCodeVec>()?;
    m.add_class::<AzCssDeclarationVec>()?;
    m.add_class::<AzCssPathSelectorVec>()?;
    m.add_class::<AzStylesheetVec>()?;
    m.add_class::<AzCssRuleBlockVec>()?;
    m.add_class::<AzU16Vec>()?;
    m.add_class::<AzF32Vec>()?;
    m.add_class::<AzU8Vec>()?;
    m.add_class::<AzCallbackDataVec>()?;
    m.add_class::<AzDebugMessageVec>()?;
    m.add_class::<AzGLuintVec>()?;
    m.add_class::<AzGLintVec>()?;
    m.add_class::<AzStringVec>()?;
    m.add_class::<AzStringPairVec>()?;
    m.add_class::<AzNormalizedLinearColorStopVec>()?;
    m.add_class::<AzNormalizedRadialColorStopVec>()?;
    m.add_class::<AzNodeIdVec>()?;
    m.add_class::<AzNodeVec>()?;
    m.add_class::<AzStyledNodeVec>()?;
    m.add_class::<AzTagIdsToNodeIdsMappingVec>()?;
    m.add_class::<AzParentWithNodeDepthVec>()?;
    m.add_class::<AzNodeDataVec>()?;
    m.add_class::<AzStyleFontFamilyVecDestructorEnumWrapper>()?;
    m.add_class::<AzTesselatedSvgNodeVecDestructorEnumWrapper>()?;
    m.add_class::<AzXmlNodeVecDestructorEnumWrapper>()?;
    m.add_class::<AzFmtArgVecDestructorEnumWrapper>()?;
    m.add_class::<AzInlineLineVecDestructorEnumWrapper>()?;
    m.add_class::<AzInlineWordVecDestructorEnumWrapper>()?;
    m.add_class::<AzInlineGlyphVecDestructorEnumWrapper>()?;
    m.add_class::<AzInlineTextHitVecDestructorEnumWrapper>()?;
    m.add_class::<AzMonitorVecDestructorEnumWrapper>()?;
    m.add_class::<AzVideoModeVecDestructorEnumWrapper>()?;
    m.add_class::<AzDomVecDestructorEnumWrapper>()?;
    m.add_class::<AzIdOrClassVecDestructorEnumWrapper>()?;
    m.add_class::<AzNodeDataInlineCssPropertyVecDestructorEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundContentVecDestructorEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundPositionVecDestructorEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundRepeatVecDestructorEnumWrapper>()?;
    m.add_class::<AzStyleBackgroundSizeVecDestructorEnumWrapper>()?;
    m.add_class::<AzStyleTransformVecDestructorEnumWrapper>()?;
    m.add_class::<AzCssPropertyVecDestructorEnumWrapper>()?;
    m.add_class::<AzSvgMultiPolygonVecDestructorEnumWrapper>()?;
    m.add_class::<AzSvgPathVecDestructorEnumWrapper>()?;
    m.add_class::<AzVertexAttributeVecDestructorEnumWrapper>()?;
    m.add_class::<AzSvgPathElementVecDestructorEnumWrapper>()?;
    m.add_class::<AzSvgVertexVecDestructorEnumWrapper>()?;
    m.add_class::<AzU32VecDestructorEnumWrapper>()?;
    m.add_class::<AzXWindowTypeVecDestructorEnumWrapper>()?;
    m.add_class::<AzVirtualKeyCodeVecDestructorEnumWrapper>()?;
    m.add_class::<AzCascadeInfoVecDestructorEnumWrapper>()?;
    m.add_class::<AzScanCodeVecDestructorEnumWrapper>()?;
    m.add_class::<AzCssDeclarationVecDestructorEnumWrapper>()?;
    m.add_class::<AzCssPathSelectorVecDestructorEnumWrapper>()?;
    m.add_class::<AzStylesheetVecDestructorEnumWrapper>()?;
    m.add_class::<AzCssRuleBlockVecDestructorEnumWrapper>()?;
    m.add_class::<AzF32VecDestructorEnumWrapper>()?;
    m.add_class::<AzU16VecDestructorEnumWrapper>()?;
    m.add_class::<AzU8VecDestructorEnumWrapper>()?;
    m.add_class::<AzCallbackDataVecDestructorEnumWrapper>()?;
    m.add_class::<AzDebugMessageVecDestructorEnumWrapper>()?;
    m.add_class::<AzGLuintVecDestructorEnumWrapper>()?;
    m.add_class::<AzGLintVecDestructorEnumWrapper>()?;
    m.add_class::<AzStringVecDestructorEnumWrapper>()?;
    m.add_class::<AzStringPairVecDestructorEnumWrapper>()?;
    m.add_class::<AzNormalizedLinearColorStopVecDestructorEnumWrapper>()?;
    m.add_class::<AzNormalizedRadialColorStopVecDestructorEnumWrapper>()?;
    m.add_class::<AzNodeIdVecDestructorEnumWrapper>()?;
    m.add_class::<AzNodeVecDestructorEnumWrapper>()?;
    m.add_class::<AzStyledNodeVecDestructorEnumWrapper>()?;
    m.add_class::<AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper>()?;
    m.add_class::<AzParentWithNodeDepthVecDestructorEnumWrapper>()?;
    m.add_class::<AzNodeDataVecDestructorEnumWrapper>()?;

    m.add_class::<AzOptionCssPropertyEnumWrapper>()?;
    m.add_class::<AzOptionPositionInfoEnumWrapper>()?;
    m.add_class::<AzOptionTimerIdEnumWrapper>()?;
    m.add_class::<AzOptionThreadIdEnumWrapper>()?;
    m.add_class::<AzOptionI16EnumWrapper>()?;
    m.add_class::<AzOptionU16EnumWrapper>()?;
    m.add_class::<AzOptionU32EnumWrapper>()?;
    m.add_class::<AzOptionImageRefEnumWrapper>()?;
    m.add_class::<AzOptionFontRefEnumWrapper>()?;
    m.add_class::<AzOptionSystemClipboardEnumWrapper>()?;
    m.add_class::<AzOptionFileTypeListEnumWrapper>()?;
    m.add_class::<AzOptionWindowStateEnumWrapper>()?;
    m.add_class::<AzOptionMouseStateEnumWrapper>()?;
    m.add_class::<AzOptionKeyboardStateEnumWrapper>()?;
    m.add_class::<AzOptionStringVecEnumWrapper>()?;
    m.add_class::<AzOptionFileEnumWrapper>()?;
    m.add_class::<AzOptionGlEnumWrapper>()?;
    m.add_class::<AzOptionThreadReceiveMsgEnumWrapper>()?;
    m.add_class::<AzOptionPercentageValueEnumWrapper>()?;
    m.add_class::<AzOptionAngleValueEnumWrapper>()?;
    m.add_class::<AzOptionRendererOptionsEnumWrapper>()?;
    m.add_class::<AzOptionCallbackEnumWrapper>()?;
    m.add_class::<AzOptionThreadSendMsgEnumWrapper>()?;
    m.add_class::<AzOptionLayoutRectEnumWrapper>()?;
    m.add_class::<AzOptionRefAnyEnumWrapper>()?;
    m.add_class::<AzOptionInlineTextEnumWrapper>()?;
    m.add_class::<AzOptionLayoutPointEnumWrapper>()?;
    m.add_class::<AzOptionLayoutSizeEnumWrapper>()?;
    m.add_class::<AzOptionWindowThemeEnumWrapper>()?;
    m.add_class::<AzOptionNodeIdEnumWrapper>()?;
    m.add_class::<AzOptionDomNodeIdEnumWrapper>()?;
    m.add_class::<AzOptionColorUEnumWrapper>()?;
    m.add_class::<AzOptionRawImageEnumWrapper>()?;
    m.add_class::<AzOptionSvgDashPatternEnumWrapper>()?;
    m.add_class::<AzOptionWaylandThemeEnumWrapper>()?;
    m.add_class::<AzOptionTaskBarIconEnumWrapper>()?;
    m.add_class::<AzOptionHwndHandleEnumWrapper>()?;
    m.add_class::<AzOptionLogicalPositionEnumWrapper>()?;
    m.add_class::<AzOptionPhysicalPositionI32EnumWrapper>()?;
    m.add_class::<AzOptionWindowIconEnumWrapper>()?;
    m.add_class::<AzOptionStringEnumWrapper>()?;
    m.add_class::<AzOptionX11VisualEnumWrapper>()?;
    m.add_class::<AzOptionI32EnumWrapper>()?;
    m.add_class::<AzOptionF32EnumWrapper>()?;
    m.add_class::<AzOptionMouseCursorTypeEnumWrapper>()?;
    m.add_class::<AzOptionLogicalSizeEnumWrapper>()?;
    m.add_class::<AzOptionCharEnumWrapper>()?;
    m.add_class::<AzOptionVirtualKeyCodeEnumWrapper>()?;
    m.add_class::<AzOptionDomEnumWrapper>()?;
    m.add_class::<AzOptionTextureEnumWrapper>()?;
    m.add_class::<AzOptionImageMaskEnumWrapper>()?;
    m.add_class::<AzOptionTabIndexEnumWrapper>()?;
    m.add_class::<AzOptionTagIdEnumWrapper>()?;
    m.add_class::<AzOptionDurationEnumWrapper>()?;
    m.add_class::<AzOptionInstantEnumWrapper>()?;
    m.add_class::<AzOptionUsizeEnumWrapper>()?;
    m.add_class::<AzOptionU8VecEnumWrapper>()?;
    m.add_class::<AzOptionU8VecRefEnumWrapper>()?;

    m.add_class::<AzResultXmlXmlErrorEnumWrapper>()?;
    m.add_class::<AzResultRawImageDecodeImageErrorEnumWrapper>()?;
    m.add_class::<AzResultU8VecEncodeImageErrorEnumWrapper>()?;
    m.add_class::<AzResultSvgXmlNodeSvgParseErrorEnumWrapper>()?;
    m.add_class::<AzResultSvgSvgParseErrorEnumWrapper>()?;
    m.add_class::<AzSvgParseErrorEnumWrapper>()?;
    m.add_class::<AzXmlErrorEnumWrapper>()?;
    m.add_class::<AzDuplicatedNamespaceError>()?;
    m.add_class::<AzUnknownNamespaceError>()?;
    m.add_class::<AzUnexpectedCloseTagError>()?;
    m.add_class::<AzUnknownEntityReferenceError>()?;
    m.add_class::<AzDuplicatedAttributeError>()?;
    m.add_class::<AzXmlParseErrorEnumWrapper>()?;
    m.add_class::<AzXmlTextError>()?;
    m.add_class::<AzXmlStreamErrorEnumWrapper>()?;
    m.add_class::<AzNonXmlCharError>()?;
    m.add_class::<AzInvalidCharError>()?;
    m.add_class::<AzInvalidCharMultipleError>()?;
    m.add_class::<AzInvalidQuoteError>()?;
    m.add_class::<AzInvalidSpaceError>()?;
    m.add_class::<AzInvalidStringError>()?;
    m.add_class::<AzSvgParseErrorPosition>()?;

    Ok(())
}

