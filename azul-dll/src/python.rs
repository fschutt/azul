
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
    #[pyo3(get, set)]
    pub x: isize,
    #[pyo3(get, set)]
    pub y: isize,
}

/// Size in physical pixels (integer units)
#[repr(C)]
#[pyclass(name = "LayoutSize")]
pub struct AzLayoutSize {
    #[pyo3(get, set)]
    pub width: isize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub window: u64,
    pub display: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `XcbHandle` struct
#[repr(C)]
#[pyclass(name = "XcbHandle")]
pub struct AzXcbHandle {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub x: i32,
    #[pyo3(get, set)]
    pub y: i32,
}

/// Same as `LayoutPoint`, but uses `u32` instead of `isize`
#[repr(C)]
#[pyclass(name = "PhysicalSizeU32")]
pub struct AzPhysicalSizeU32 {
    #[pyo3(get, set)]
    pub width: u32,
    #[pyo3(get, set)]
    pub height: u32,
}

/// Logical position (can differ based on HiDPI settings). Usually this is what you'd want for hit-testing and positioning elements.
#[repr(C)]
#[pyclass(name = "LogicalPosition")]
pub struct AzLogicalPosition {
    #[pyo3(get, set)]
    pub x: f32,
    #[pyo3(get, set)]
    pub y: f32,
}

/// A size in "logical" (non-HiDPI-adjusted) pixels in floating-point units
#[repr(C)]
#[pyclass(name = "LogicalSize")]
pub struct AzLogicalSize {
    #[pyo3(get, set)]
    pub width: f32,
    #[pyo3(get, set)]
    pub height: f32,
}

/// Unique hash of a window icon, so that azul does not have to compare the actual bytes to see wether the window icon has changed.
#[repr(C)]
#[pyclass(name = "IconKey")]
pub struct AzIconKey {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub is_maximized: bool,
    #[pyo3(get, set)]
    pub is_minimized: bool,
    #[pyo3(get, set)]
    pub is_about_to_close: bool,
    #[pyo3(get, set)]
    pub is_fullscreen: bool,
    #[pyo3(get, set)]
    pub has_decorations: bool,
    #[pyo3(get, set)]
    pub is_visible: bool,
    #[pyo3(get, set)]
    pub is_always_on_top: bool,
    #[pyo3(get, set)]
    pub is_resizable: bool,
    #[pyo3(get, set)]
    pub has_focus: bool,
    #[pyo3(get, set)]
    pub has_extended_window_frame: bool,
    #[pyo3(get, set)]
    pub has_blur_behind_window: bool,
}

/// Debugging information, will be rendered as an overlay on top of the UI
#[repr(C)]
#[pyclass(name = "DebugState")]
pub struct AzDebugState {
    #[pyo3(get, set)]
    pub profiler_dbg: bool,
    #[pyo3(get, set)]
    pub render_target_dbg: bool,
    #[pyo3(get, set)]
    pub texture_cache_dbg: bool,
    #[pyo3(get, set)]
    pub gpu_time_queries: bool,
    #[pyo3(get, set)]
    pub gpu_sample_queries: bool,
    #[pyo3(get, set)]
    pub disable_batching: bool,
    #[pyo3(get, set)]
    pub epochs: bool,
    #[pyo3(get, set)]
    pub echo_driver_messages: bool,
    #[pyo3(get, set)]
    pub show_overdraw: bool,
    #[pyo3(get, set)]
    pub gpu_cache_dbg: bool,
    #[pyo3(get, set)]
    pub texture_cache_dbg_clear_evicted: bool,
    #[pyo3(get, set)]
    pub picture_caching_dbg: bool,
    #[pyo3(get, set)]
    pub primitive_dbg: bool,
    #[pyo3(get, set)]
    pub zoom_dbg: bool,
    #[pyo3(get, set)]
    pub small_screen: bool,
    #[pyo3(get, set)]
    pub disable_opaque_pass: bool,
    #[pyo3(get, set)]
    pub disable_alpha_pass: bool,
    #[pyo3(get, set)]
    pub disable_clip_masks: bool,
    #[pyo3(get, set)]
    pub disable_text_prims: bool,
    #[pyo3(get, set)]
    pub disable_gradient_prims: bool,
    #[pyo3(get, set)]
    pub obscure_images: bool,
    #[pyo3(get, set)]
    pub glyph_flashing: bool,
    #[pyo3(get, set)]
    pub smart_profiler: bool,
    #[pyo3(get, set)]
    pub invalidation_dbg: bool,
    #[pyo3(get, set)]
    pub tile_cache_logging_dbg: bool,
    #[pyo3(get, set)]
    pub profiler_capture: bool,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub _reserved: u8,
}

/// Re-export of rust-allocated (stack based) `WasmWindowOptions` struct
#[repr(C)]
#[pyclass(name = "WasmWindowOptions")]
pub struct AzWasmWindowOptions {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub unused: u8,
}

/// C-ABI stable wrapper over a `LayoutCallbackType`
#[repr(C)]
#[pyclass(name = "LayoutCallback")]
pub struct AzLayoutCallback {
    #[pyo3(get, set)]
    pub cb: AzLayoutCallbackType,
}

/// `AzLayoutCallbackType` struct
pub type AzLayoutCallbackType = extern "C" fn(&mut AzRefAny, AzLayoutCallbackInfo) -> AzStyledDom;

/// C-ABI stable wrapper over a `CallbackType`
#[repr(C)]
#[pyclass(name = "Callback")]
pub struct AzCallback {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub inner: usize,
}

/// ID of a DOM - one window can contain multiple, nested DOMs (such as iframes)
#[repr(C)]
#[pyclass(name = "DomId")]
pub struct AzDomId {
    #[pyo3(get, set)]
    pub inner: usize,
}

/// Re-export of rust-allocated (stack based) `PositionInfoInner` struct
#[repr(C)]
#[pyclass(name = "PositionInfoInner")]
pub struct AzPositionInfoInner {
    #[pyo3(get, set)]
    pub x_offset: f32,
    #[pyo3(get, set)]
    pub y_offset: f32,
    #[pyo3(get, set)]
    pub static_x_offset: f32,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub cb: AzIFrameCallbackType,
}

/// `AzIFrameCallbackType` struct
pub type AzIFrameCallbackType = extern "C" fn(&mut AzRefAny, AzIFrameCallbackInfo) -> AzIFrameCallbackReturn;

/// Re-export of rust-allocated (stack based) `RenderImageCallback` struct
#[repr(C)]
#[pyclass(name = "RenderImageCallback")]
pub struct AzRenderImageCallback {
    #[pyo3(get, set)]
    pub cb: AzRenderImageCallbackType,
}

/// `AzRenderImageCallbackType` struct
pub type AzRenderImageCallbackType = extern "C" fn(&mut AzRefAny, AzRenderImageCallbackInfo) -> AzImageRef;

/// Re-export of rust-allocated (stack based) `TimerCallback` struct
#[repr(C)]
#[pyclass(name = "TimerCallback")]
pub struct AzTimerCallback {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub cb: AzWriteBackCallbackType,
}

/// Re-export of rust-allocated (stack based) `ThreadCallback` struct
#[repr(C)]
#[pyclass(name = "ThreadCallback")]
pub struct AzThreadCallback {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub repeat: u32,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub r: u8,
    #[pyo3(get, set)]
    pub g: u8,
    #[pyo3(get, set)]
    pub b: u8,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub parent: usize,
    #[pyo3(get, set)]
    pub previous_sibling: usize,
    #[pyo3(get, set)]
    pub next_sibling: usize,
    #[pyo3(get, set)]
    pub last_child: usize,
}

/// Re-export of rust-allocated (stack based) `CascadeInfo` struct
#[repr(C)]
#[pyclass(name = "CascadeInfo")]
pub struct AzCascadeInfo {
    #[pyo3(get, set)]
    pub index_in_parent: u32,
    #[pyo3(get, set)]
    pub is_last_child: bool,
}

/// Re-export of rust-allocated (stack based) `StyledNodeState` struct
#[repr(C)]
#[pyclass(name = "StyledNodeState")]
pub struct AzStyledNodeState {
    #[pyo3(get, set)]
    pub normal: bool,
    #[pyo3(get, set)]
    pub hover: bool,
    #[pyo3(get, set)]
    pub active: bool,
    #[pyo3(get, set)]
    pub focused: bool,
}

/// Re-export of rust-allocated (stack based) `TagId` struct
#[repr(C)]
#[pyclass(name = "TagId")]
pub struct AzTagId {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub _0: i32,
    #[pyo3(get, set)]
    pub _1: i32,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [u8]`
#[repr(C)]
#[pyclass(name = "U8VecRefMut")]
pub struct AzU8VecRefMut {
    pub ptr: *mut u8,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&[f32]`
#[repr(C)]
#[pyclass(name = "F32VecRef")]
pub struct AzF32VecRef {
    pub ptr: *const f32,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&[i32]`
#[repr(C)]
#[pyclass(name = "I32VecRef")]
pub struct AzI32VecRef {
    pub ptr: *const i32,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
#[repr(C)]
#[pyclass(name = "GLuintVecRef")]
pub struct AzGLuintVecRef {
    pub ptr: *const u32,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
#[repr(C)]
#[pyclass(name = "GLenumVecRef")]
pub struct AzGLenumVecRef {
    pub ptr: *const u32,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
#[repr(C)]
#[pyclass(name = "GLintVecRefMut")]
pub struct AzGLintVecRefMut {
    pub ptr: *mut i32,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
#[repr(C)]
#[pyclass(name = "GLint64VecRefMut")]
pub struct AzGLint64VecRefMut {
    pub ptr: *mut i64,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
#[repr(C)]
#[pyclass(name = "GLbooleanVecRefMut")]
pub struct AzGLbooleanVecRefMut {
    pub ptr: *mut u8,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
#[repr(C)]
#[pyclass(name = "GLfloatVecRefMut")]
pub struct AzGLfloatVecRefMut {
    pub ptr: *mut f32,
    #[pyo3(get, set)]
    pub len: usize,
}

/// C-ABI stable reexport of `&str`
#[repr(C)]
#[pyclass(name = "Refstr")]
pub struct AzRefstr {
    pub ptr: *const u8,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub is_opaque: bool,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub center_x: f32,
    #[pyo3(get, set)]
    pub center_y: f32,
    #[pyo3(get, set)]
    pub radius: f32,
}

/// Re-export of rust-allocated (stack based) `SvgPoint` struct
#[repr(C)]
#[pyclass(name = "SvgPoint")]
pub struct AzSvgPoint {
    #[pyo3(get, set)]
    pub x: f32,
    #[pyo3(get, set)]
    pub y: f32,
}

/// Re-export of rust-allocated (stack based) `SvgRect` struct
#[repr(C)]
#[pyclass(name = "SvgRect")]
pub struct AzSvgRect {
    #[pyo3(get, set)]
    pub width: f32,
    #[pyo3(get, set)]
    pub height: f32,
    #[pyo3(get, set)]
    pub x: f32,
    #[pyo3(get, set)]
    pub y: f32,
    #[pyo3(get, set)]
    pub radius_top_left: f32,
    #[pyo3(get, set)]
    pub radius_top_right: f32,
    #[pyo3(get, set)]
    pub radius_bottom_left: f32,
    #[pyo3(get, set)]
    pub radius_bottom_right: f32,
}

/// Re-export of rust-allocated (stack based) `SvgVertex` struct
#[repr(C)]
#[pyclass(name = "SvgVertex")]
pub struct AzSvgVertex {
    #[pyo3(get, set)]
    pub x: f32,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub sx: f32,
    #[pyo3(get, set)]
    pub kx: f32,
    #[pyo3(get, set)]
    pub ky: f32,
    #[pyo3(get, set)]
    pub sy: f32,
    #[pyo3(get, set)]
    pub tx: f32,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub offset: f32,
    #[pyo3(get, set)]
    pub length_1: f32,
    #[pyo3(get, set)]
    pub gap_1: f32,
    #[pyo3(get, set)]
    pub length_2: f32,
    #[pyo3(get, set)]
    pub gap_2: f32,
    #[pyo3(get, set)]
    pub length_3: f32,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub cb: AzInstantPtrCloneFnType,
}

/// `AzInstantPtrDestructorFnType` struct
pub type AzInstantPtrDestructorFnType = extern "C" fn(&mut AzInstantPtr);

/// Re-export of rust-allocated (stack based) `InstantPtrDestructorFn` struct
#[repr(C)]
#[pyclass(name = "InstantPtrDestructorFn")]
pub struct AzInstantPtrDestructorFn {
    #[pyo3(get, set)]
    pub cb: AzInstantPtrDestructorFnType,
}

/// Re-export of rust-allocated (stack based) `SystemTick` struct
#[repr(C)]
#[pyclass(name = "SystemTick")]
pub struct AzSystemTick {
    #[pyo3(get, set)]
    pub tick_counter: u64,
}

/// Re-export of rust-allocated (stack based) `SystemTimeDiff` struct
#[repr(C)]
#[pyclass(name = "SystemTimeDiff")]
pub struct AzSystemTimeDiff {
    #[pyo3(get, set)]
    pub secs: u64,
    #[pyo3(get, set)]
    pub nanos: u32,
}

/// Re-export of rust-allocated (stack based) `SystemTickDiff` struct
#[repr(C)]
#[pyclass(name = "SystemTickDiff")]
pub struct AzSystemTickDiff {
    #[pyo3(get, set)]
    pub tick_diff: u64,
}

/// Re-export of rust-allocated (stack based) `TimerId` struct
#[repr(C)]
#[pyclass(name = "TimerId")]
pub struct AzTimerId {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub id: usize,
}

/// `AzCreateThreadFnType` struct
pub type AzCreateThreadFnType = extern "C" fn(AzRefAny, AzRefAny, AzThreadCallback) -> AzThread;

/// Re-export of rust-allocated (stack based) `CreateThreadFn` struct
#[repr(C)]
#[pyclass(name = "CreateThreadFn")]
pub struct AzCreateThreadFn {
    #[pyo3(get, set)]
    pub cb: AzCreateThreadFnType,
}

/// `AzGetSystemTimeFnType` struct
pub type AzGetSystemTimeFnType = extern "C" fn() -> AzInstant;

/// Get the current system time, equivalent to `std::time::Instant::now()`, except it also works on systems that work with "ticks" instead of timers
#[repr(C)]
#[pyclass(name = "GetSystemTimeFn")]
pub struct AzGetSystemTimeFn {
    #[pyo3(get, set)]
    pub cb: AzGetSystemTimeFnType,
}

/// `AzCheckThreadFinishedFnType` struct
pub type AzCheckThreadFinishedFnType = extern "C" fn(&c_void) -> bool;

/// Function called to check if the thread has finished
#[repr(C)]
#[pyclass(name = "CheckThreadFinishedFn")]
pub struct AzCheckThreadFinishedFn {
    #[pyo3(get, set)]
    pub cb: AzCheckThreadFinishedFnType,
}

/// `AzLibrarySendThreadMsgFnType` struct
pub type AzLibrarySendThreadMsgFnType = extern "C" fn(&c_void, AzThreadSendMsg) -> bool;

/// Function to send a message to the thread
#[repr(C)]
#[pyclass(name = "LibrarySendThreadMsgFn")]
pub struct AzLibrarySendThreadMsgFn {
    #[pyo3(get, set)]
    pub cb: AzLibrarySendThreadMsgFnType,
}

/// `AzLibraryReceiveThreadMsgFnType` struct
pub type AzLibraryReceiveThreadMsgFnType = extern "C" fn(&c_void) -> AzOptionThreadReceiveMsg;

/// Function to receive a message from the thread
#[repr(C)]
#[pyclass(name = "LibraryReceiveThreadMsgFn")]
pub struct AzLibraryReceiveThreadMsgFn {
    #[pyo3(get, set)]
    pub cb: AzLibraryReceiveThreadMsgFnType,
}

/// `AzThreadRecvFnType` struct
pub type AzThreadRecvFnType = extern "C" fn(&c_void) -> AzOptionThreadSendMsg;

/// Function that the running `Thread` can call to receive messages from the main UI thread
#[repr(C)]
#[pyclass(name = "ThreadRecvFn")]
pub struct AzThreadRecvFn {
    #[pyo3(get, set)]
    pub cb: AzThreadRecvFnType,
}

/// `AzThreadSendFnType` struct
pub type AzThreadSendFnType = extern "C" fn(&c_void, AzThreadReceiveMsg) -> bool;

/// Function that the running `Thread` can call to receive messages from the main UI thread
#[repr(C)]
#[pyclass(name = "ThreadSendFn")]
pub struct AzThreadSendFn {
    #[pyo3(get, set)]
    pub cb: AzThreadSendFnType,
}

/// `AzThreadDestructorFnType` struct
pub type AzThreadDestructorFnType = extern "C" fn(&mut AzThread);

/// Destructor of the `Thread`
#[repr(C)]
#[pyclass(name = "ThreadDestructorFn")]
pub struct AzThreadDestructorFn {
    #[pyo3(get, set)]
    pub cb: AzThreadDestructorFnType,
}

/// `AzThreadReceiverDestructorFnType` struct
pub type AzThreadReceiverDestructorFnType = extern "C" fn(&mut AzThreadReceiver);

/// Destructor of the `ThreadReceiver`
#[repr(C)]
#[pyclass(name = "ThreadReceiverDestructorFn")]
pub struct AzThreadReceiverDestructorFn {
    #[pyo3(get, set)]
    pub cb: AzThreadReceiverDestructorFnType,
}

/// `AzThreadSenderDestructorFnType` struct
pub type AzThreadSenderDestructorFnType = extern "C" fn(&mut AzThreadSender);

/// Destructor of the `ThreadSender`
#[repr(C)]
#[pyclass(name = "ThreadSenderDestructorFn")]
pub struct AzThreadSenderDestructorFn {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub row: u32,
    #[pyo3(get, set)]
    pub col: u32,
}

/// External system callbacks to get the system time or create / manage threads
#[repr(C)]
#[pyclass(name = "SystemCallbacks")]
pub struct AzSystemCallbacks {
    #[pyo3(get, set)]
    pub create_thread_fn: AzCreateThreadFn,
    #[pyo3(get, set)]
    pub get_system_time_fn: AzGetSystemTimeFn,
}

/// Force a specific renderer: note that azul will **crash** on startup if the `RendererOptions` are not satisfied.
#[repr(C)]
#[pyclass(name = "RendererOptions")]
pub struct AzRendererOptions {
    #[pyo3(get, set)]
    pub vsync: AzVsync,
    #[pyo3(get, set)]
    pub srgb: AzSrgb,
    #[pyo3(get, set)]
    pub hw_accel: AzHwAcceleration,
}

/// Represents a rectangle in physical pixels (integer units)
#[repr(C)]
#[pyclass(name = "LayoutRect")]
pub struct AzLayoutRect {
    #[pyo3(get, set)]
    pub origin: AzLayoutPoint,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub origin: AzLogicalPosition,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub size: AzLayoutSize,
    #[pyo3(get, set)]
    pub bit_depth: u16,
    #[pyo3(get, set)]
    pub refresh_rate: u16,
}

/// Combination of node ID + DOM ID, both together can identify a node
#[repr(C)]
#[pyclass(name = "DomNodeId")]
pub struct AzDomNodeId {
    #[pyo3(get, set)]
    pub dom: AzDomId,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub logical_size: AzLogicalSize,
    #[pyo3(get, set)]
    pub hidpi_factor: f32,
}

/// Re-export of rust-allocated (stack based) `InlineGlyph` struct
#[repr(C)]
#[pyclass(name = "InlineGlyph")]
pub struct AzInlineGlyph {
    #[pyo3(get, set)]
    pub bounds: AzLogicalRect,
    #[pyo3(get, set)]
    pub unicode_codepoint: AzOptionChar,
    #[pyo3(get, set)]
    pub glyph_index: u32,
}

/// Re-export of rust-allocated (stack based) `InlineTextHit` struct
#[repr(C)]
#[pyclass(name = "InlineTextHit")]
pub struct AzInlineTextHit {
    #[pyo3(get, set)]
    pub unicode_codepoint: AzOptionChar,
    #[pyo3(get, set)]
    pub hit_relative_to_inline_text: AzLogicalPosition,
    #[pyo3(get, set)]
    pub hit_relative_to_line: AzLogicalPosition,
    #[pyo3(get, set)]
    pub hit_relative_to_text_content: AzLogicalPosition,
    #[pyo3(get, set)]
    pub hit_relative_to_glyph: AzLogicalPosition,
    #[pyo3(get, set)]
    pub line_index_relative_to_text: usize,
    #[pyo3(get, set)]
    pub word_index_relative_to_text: usize,
    #[pyo3(get, set)]
    pub text_content_index_relative_to_text: usize,
    #[pyo3(get, set)]
    pub glyph_index_relative_to_text: usize,
    #[pyo3(get, set)]
    pub char_index_relative_to_text: usize,
    #[pyo3(get, set)]
    pub word_index_relative_to_line: usize,
    #[pyo3(get, set)]
    pub text_content_index_relative_to_line: usize,
    #[pyo3(get, set)]
    pub glyph_index_relative_to_line: usize,
    #[pyo3(get, set)]
    pub char_index_relative_to_line: usize,
    #[pyo3(get, set)]
    pub glyph_index_relative_to_word: usize,
    #[pyo3(get, set)]
    pub char_index_relative_to_word: usize,
}

/// Re-export of rust-allocated (stack based) `IFrameCallbackInfo` struct
#[repr(C)]
#[pyclass(name = "IFrameCallbackInfo")]
pub struct AzIFrameCallbackInfo {
    pub system_fonts: *const c_void,
    pub image_cache: *const c_void,
    #[pyo3(get, set)]
    pub window_theme: AzWindowTheme,
    #[pyo3(get, set)]
    pub bounds: AzHidpiAdjustedBounds,
    #[pyo3(get, set)]
    pub scroll_size: AzLogicalSize,
    #[pyo3(get, set)]
    pub scroll_offset: AzLogicalPosition,
    #[pyo3(get, set)]
    pub virtual_scroll_size: AzLogicalSize,
    #[pyo3(get, set)]
    pub virtual_scroll_offset: AzLogicalPosition,
    pub _reserved_ref: *const c_void,
    pub _reserved_mut: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `TimerCallbackReturn` struct
#[repr(C)]
#[pyclass(name = "TimerCallbackReturn")]
pub struct AzTimerCallbackReturn {
    #[pyo3(get, set)]
    pub should_update: AzUpdateScreen,
    #[pyo3(get, set)]
    pub should_terminate: AzTerminateTimer,
}

/// RefAny is a reference-counted, opaque pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
#[repr(C)]
#[pyclass(name = "RefAny")]
pub struct AzRefAny {
    pub _internal_ptr: *const c_void,
    #[pyo3(get, set)]
    pub sharing_info: AzRefCount,
}

/// Re-export of rust-allocated (stack based) `IFrameNode` struct
#[repr(C)]
#[pyclass(name = "IFrameNode")]
pub struct AzIFrameNode {
    #[pyo3(get, set)]
    pub callback: AzIFrameCallback,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub metric: AzSizeMetric,
    #[pyo3(get, set)]
    pub number: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `PixelValueNoPercent` struct
#[repr(C)]
#[pyclass(name = "PixelValueNoPercent")]
pub struct AzPixelValueNoPercent {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBoxShadow` struct
#[repr(C)]
#[pyclass(name = "StyleBoxShadow")]
pub struct AzStyleBoxShadow {
    pub offset: [AzPixelValueNoPercent;2],
    #[pyo3(get, set)]
    pub color: AzColorU,
    #[pyo3(get, set)]
    pub blur_radius: AzPixelValueNoPercent,
    #[pyo3(get, set)]
    pub spread_radius: AzPixelValueNoPercent,
    #[pyo3(get, set)]
    pub clip_mode: AzBoxShadowClipMode,
}

/// Re-export of rust-allocated (stack based) `LayoutBottom` struct
#[repr(C)]
#[pyclass(name = "LayoutBottom")]
pub struct AzLayoutBottom {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutFlexGrow` struct
#[repr(C)]
#[pyclass(name = "LayoutFlexGrow")]
pub struct AzLayoutFlexGrow {
    #[pyo3(get, set)]
    pub inner: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `LayoutFlexShrink` struct
#[repr(C)]
#[pyclass(name = "LayoutFlexShrink")]
pub struct AzLayoutFlexShrink {
    #[pyo3(get, set)]
    pub inner: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `LayoutHeight` struct
#[repr(C)]
#[pyclass(name = "LayoutHeight")]
pub struct AzLayoutHeight {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutLeft` struct
#[repr(C)]
#[pyclass(name = "LayoutLeft")]
pub struct AzLayoutLeft {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginBottom` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginBottom")]
pub struct AzLayoutMarginBottom {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginLeft` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginLeft")]
pub struct AzLayoutMarginLeft {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginRight` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginRight")]
pub struct AzLayoutMarginRight {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMarginTop` struct
#[repr(C)]
#[pyclass(name = "LayoutMarginTop")]
pub struct AzLayoutMarginTop {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMaxHeight` struct
#[repr(C)]
#[pyclass(name = "LayoutMaxHeight")]
pub struct AzLayoutMaxHeight {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMaxWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutMaxWidth")]
pub struct AzLayoutMaxWidth {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMinHeight` struct
#[repr(C)]
#[pyclass(name = "LayoutMinHeight")]
pub struct AzLayoutMinHeight {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutMinWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutMinWidth")]
pub struct AzLayoutMinWidth {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingBottom` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingBottom")]
pub struct AzLayoutPaddingBottom {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingLeft` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingLeft")]
pub struct AzLayoutPaddingLeft {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingRight` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingRight")]
pub struct AzLayoutPaddingRight {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutPaddingTop` struct
#[repr(C)]
#[pyclass(name = "LayoutPaddingTop")]
pub struct AzLayoutPaddingTop {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutRight` struct
#[repr(C)]
#[pyclass(name = "LayoutRight")]
pub struct AzLayoutRight {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutTop` struct
#[repr(C)]
#[pyclass(name = "LayoutTop")]
pub struct AzLayoutTop {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `LayoutWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutWidth")]
pub struct AzLayoutWidth {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `PercentageValue` struct
#[repr(C)]
#[pyclass(name = "PercentageValue")]
pub struct AzPercentageValue {
    #[pyo3(get, set)]
    pub number: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `AngleValue` struct
#[repr(C)]
#[pyclass(name = "AngleValue")]
pub struct AzAngleValue {
    #[pyo3(get, set)]
    pub metric: AzAngleMetric,
    #[pyo3(get, set)]
    pub number: AzFloatValue,
}

/// Re-export of rust-allocated (stack based) `NormalizedLinearColorStop` struct
#[repr(C)]
#[pyclass(name = "NormalizedLinearColorStop")]
pub struct AzNormalizedLinearColorStop {
    #[pyo3(get, set)]
    pub offset: AzPercentageValue,
    #[pyo3(get, set)]
    pub color: AzColorU,
}

/// Re-export of rust-allocated (stack based) `NormalizedRadialColorStop` struct
#[repr(C)]
#[pyclass(name = "NormalizedRadialColorStop")]
pub struct AzNormalizedRadialColorStop {
    #[pyo3(get, set)]
    pub offset: AzAngleValue,
    #[pyo3(get, set)]
    pub color: AzColorU,
}

/// Re-export of rust-allocated (stack based) `DirectionCorners` struct
#[repr(C)]
#[pyclass(name = "DirectionCorners")]
pub struct AzDirectionCorners {
    #[pyo3(get, set)]
    pub from: AzDirectionCorner,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub horizontal: AzBackgroundPositionHorizontal,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderBottomLeftRadius")]
pub struct AzStyleBorderBottomLeftRadius {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderBottomRightRadius")]
pub struct AzStyleBorderBottomRightRadius {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderBottomStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderBottomStyle")]
pub struct AzStyleBorderBottomStyle {
    #[pyo3(get, set)]
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderBottomWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderBottomWidth")]
pub struct AzLayoutBorderBottomWidth {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderLeftColor` struct
#[repr(C)]
#[pyclass(name = "StyleBorderLeftColor")]
pub struct AzStyleBorderLeftColor {
    #[pyo3(get, set)]
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderLeftStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderLeftStyle")]
pub struct AzStyleBorderLeftStyle {
    #[pyo3(get, set)]
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderLeftWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderLeftWidth")]
pub struct AzLayoutBorderLeftWidth {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderRightColor` struct
#[repr(C)]
#[pyclass(name = "StyleBorderRightColor")]
pub struct AzStyleBorderRightColor {
    #[pyo3(get, set)]
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderRightStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderRightStyle")]
pub struct AzStyleBorderRightStyle {
    #[pyo3(get, set)]
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderRightWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderRightWidth")]
pub struct AzLayoutBorderRightWidth {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopColor` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopColor")]
pub struct AzStyleBorderTopColor {
    #[pyo3(get, set)]
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopLeftRadius")]
pub struct AzStyleBorderTopLeftRadius {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadius` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopRightRadius")]
pub struct AzStyleBorderTopRightRadius {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleBorderTopStyle` struct
#[repr(C)]
#[pyclass(name = "StyleBorderTopStyle")]
pub struct AzStyleBorderTopStyle {
    #[pyo3(get, set)]
    pub inner: AzBorderStyle,
}

/// Re-export of rust-allocated (stack based) `LayoutBorderTopWidth` struct
#[repr(C)]
#[pyclass(name = "LayoutBorderTopWidth")]
pub struct AzLayoutBorderTopWidth {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleFontSize` struct
#[repr(C)]
#[pyclass(name = "StyleFontSize")]
pub struct AzStyleFontSize {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleLetterSpacing` struct
#[repr(C)]
#[pyclass(name = "StyleLetterSpacing")]
pub struct AzStyleLetterSpacing {
    #[pyo3(get, set)]
    pub inner: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleLineHeight` struct
#[repr(C)]
#[pyclass(name = "StyleLineHeight")]
pub struct AzStyleLineHeight {
    #[pyo3(get, set)]
    pub inner: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTabWidth` struct
#[repr(C)]
#[pyclass(name = "StyleTabWidth")]
pub struct AzStyleTabWidth {
    #[pyo3(get, set)]
    pub inner: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleOpacity` struct
#[repr(C)]
#[pyclass(name = "StyleOpacity")]
pub struct AzStyleOpacity {
    #[pyo3(get, set)]
    pub inner: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformOrigin` struct
#[repr(C)]
#[pyclass(name = "StyleTransformOrigin")]
pub struct AzStyleTransformOrigin {
    #[pyo3(get, set)]
    pub x: AzPixelValue,
    #[pyo3(get, set)]
    pub y: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StylePerspectiveOrigin` struct
#[repr(C)]
#[pyclass(name = "StylePerspectiveOrigin")]
pub struct AzStylePerspectiveOrigin {
    #[pyo3(get, set)]
    pub x: AzPixelValue,
    #[pyo3(get, set)]
    pub y: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformMatrix2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformMatrix2D")]
pub struct AzStyleTransformMatrix2D {
    #[pyo3(get, set)]
    pub a: AzPixelValue,
    #[pyo3(get, set)]
    pub b: AzPixelValue,
    #[pyo3(get, set)]
    pub c: AzPixelValue,
    #[pyo3(get, set)]
    pub d: AzPixelValue,
    #[pyo3(get, set)]
    pub tx: AzPixelValue,
    #[pyo3(get, set)]
    pub ty: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformMatrix3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformMatrix3D")]
pub struct AzStyleTransformMatrix3D {
    #[pyo3(get, set)]
    pub m11: AzPixelValue,
    #[pyo3(get, set)]
    pub m12: AzPixelValue,
    #[pyo3(get, set)]
    pub m13: AzPixelValue,
    #[pyo3(get, set)]
    pub m14: AzPixelValue,
    #[pyo3(get, set)]
    pub m21: AzPixelValue,
    #[pyo3(get, set)]
    pub m22: AzPixelValue,
    #[pyo3(get, set)]
    pub m23: AzPixelValue,
    #[pyo3(get, set)]
    pub m24: AzPixelValue,
    #[pyo3(get, set)]
    pub m31: AzPixelValue,
    #[pyo3(get, set)]
    pub m32: AzPixelValue,
    #[pyo3(get, set)]
    pub m33: AzPixelValue,
    #[pyo3(get, set)]
    pub m34: AzPixelValue,
    #[pyo3(get, set)]
    pub m41: AzPixelValue,
    #[pyo3(get, set)]
    pub m42: AzPixelValue,
    #[pyo3(get, set)]
    pub m43: AzPixelValue,
    #[pyo3(get, set)]
    pub m44: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformTranslate2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformTranslate2D")]
pub struct AzStyleTransformTranslate2D {
    #[pyo3(get, set)]
    pub x: AzPixelValue,
    #[pyo3(get, set)]
    pub y: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformTranslate3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformTranslate3D")]
pub struct AzStyleTransformTranslate3D {
    #[pyo3(get, set)]
    pub x: AzPixelValue,
    #[pyo3(get, set)]
    pub y: AzPixelValue,
    #[pyo3(get, set)]
    pub z: AzPixelValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformRotate3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformRotate3D")]
pub struct AzStyleTransformRotate3D {
    #[pyo3(get, set)]
    pub x: AzPercentageValue,
    #[pyo3(get, set)]
    pub y: AzPercentageValue,
    #[pyo3(get, set)]
    pub z: AzPercentageValue,
    #[pyo3(get, set)]
    pub angle: AzAngleValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformScale2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformScale2D")]
pub struct AzStyleTransformScale2D {
    #[pyo3(get, set)]
    pub x: AzPercentageValue,
    #[pyo3(get, set)]
    pub y: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformScale3D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformScale3D")]
pub struct AzStyleTransformScale3D {
    #[pyo3(get, set)]
    pub x: AzPercentageValue,
    #[pyo3(get, set)]
    pub y: AzPercentageValue,
    #[pyo3(get, set)]
    pub z: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTransformSkew2D` struct
#[repr(C)]
#[pyclass(name = "StyleTransformSkew2D")]
pub struct AzStyleTransformSkew2D {
    #[pyo3(get, set)]
    pub x: AzPercentageValue,
    #[pyo3(get, set)]
    pub y: AzPercentageValue,
}

/// Re-export of rust-allocated (stack based) `StyleTextColor` struct
#[repr(C)]
#[pyclass(name = "StyleTextColor")]
pub struct AzStyleTextColor {
    #[pyo3(get, set)]
    pub inner: AzColorU,
}

/// Re-export of rust-allocated (stack based) `StyleWordSpacing` struct
#[repr(C)]
#[pyclass(name = "StyleWordSpacing")]
pub struct AzStyleWordSpacing {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub depth: usize,
    #[pyo3(get, set)]
    pub node_id: AzNodeId,
}

/// Re-export of rust-allocated (stack based) `Gl` struct
#[repr(C)]
#[pyclass(name = "Gl")]
pub struct AzGl {
    pub ptr: *const c_void,
    #[pyo3(get, set)]
    pub svg_shader: u32,
    #[pyo3(get, set)]
    pub fxaa_shader: u32,
    #[pyo3(get, set)]
    pub renderer_type: AzRendererType,
}

/// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
#[repr(C)]
#[pyclass(name = "RefstrVecRef")]
pub struct AzRefstrVecRef {
    pub(crate) ptr: *const AzRefstr,
    #[pyo3(get, set)]
    pub len: usize,
}

/// Re-export of rust-allocated (stack based) `ImageMask` struct
#[repr(C)]
#[pyclass(name = "ImageMask")]
pub struct AzImageMask {
    #[pyo3(get, set)]
    pub image: AzImageRef,
    #[pyo3(get, set)]
    pub rect: AzLogicalRect,
    #[pyo3(get, set)]
    pub repeat: bool,
}

/// Re-export of rust-allocated (stack based) `FontMetrics` struct
#[repr(C)]
#[pyclass(name = "FontMetrics")]
pub struct AzFontMetrics {
    #[pyo3(get, set)]
    pub units_per_em: u16,
    #[pyo3(get, set)]
    pub font_flags: u16,
    #[pyo3(get, set)]
    pub x_min: i16,
    #[pyo3(get, set)]
    pub y_min: i16,
    #[pyo3(get, set)]
    pub x_max: i16,
    #[pyo3(get, set)]
    pub y_max: i16,
    #[pyo3(get, set)]
    pub ascender: i16,
    #[pyo3(get, set)]
    pub descender: i16,
    #[pyo3(get, set)]
    pub line_gap: i16,
    #[pyo3(get, set)]
    pub advance_width_max: u16,
    #[pyo3(get, set)]
    pub min_left_side_bearing: i16,
    #[pyo3(get, set)]
    pub min_right_side_bearing: i16,
    #[pyo3(get, set)]
    pub x_max_extent: i16,
    #[pyo3(get, set)]
    pub caret_slope_rise: i16,
    #[pyo3(get, set)]
    pub caret_slope_run: i16,
    #[pyo3(get, set)]
    pub caret_offset: i16,
    #[pyo3(get, set)]
    pub num_h_metrics: u16,
    #[pyo3(get, set)]
    pub x_avg_char_width: i16,
    #[pyo3(get, set)]
    pub us_weight_class: u16,
    #[pyo3(get, set)]
    pub us_width_class: u16,
    #[pyo3(get, set)]
    pub fs_type: u16,
    #[pyo3(get, set)]
    pub y_subscript_x_size: i16,
    #[pyo3(get, set)]
    pub y_subscript_y_size: i16,
    #[pyo3(get, set)]
    pub y_subscript_x_offset: i16,
    #[pyo3(get, set)]
    pub y_subscript_y_offset: i16,
    #[pyo3(get, set)]
    pub y_superscript_x_size: i16,
    #[pyo3(get, set)]
    pub y_superscript_y_size: i16,
    #[pyo3(get, set)]
    pub y_superscript_x_offset: i16,
    #[pyo3(get, set)]
    pub y_superscript_y_offset: i16,
    #[pyo3(get, set)]
    pub y_strikeout_size: i16,
    #[pyo3(get, set)]
    pub y_strikeout_position: i16,
    #[pyo3(get, set)]
    pub s_family_class: i16,
    pub panose: [u8; 10],
    #[pyo3(get, set)]
    pub ul_unicode_range1: u32,
    #[pyo3(get, set)]
    pub ul_unicode_range2: u32,
    #[pyo3(get, set)]
    pub ul_unicode_range3: u32,
    #[pyo3(get, set)]
    pub ul_unicode_range4: u32,
    #[pyo3(get, set)]
    pub ach_vend_id: u32,
    #[pyo3(get, set)]
    pub fs_selection: u16,
    #[pyo3(get, set)]
    pub us_first_char_index: u16,
    #[pyo3(get, set)]
    pub us_last_char_index: u16,
    #[pyo3(get, set)]
    pub s_typo_ascender: AzOptionI16,
    #[pyo3(get, set)]
    pub s_typo_descender: AzOptionI16,
    #[pyo3(get, set)]
    pub s_typo_line_gap: AzOptionI16,
    #[pyo3(get, set)]
    pub us_win_ascent: AzOptionU16,
    #[pyo3(get, set)]
    pub us_win_descent: AzOptionU16,
    #[pyo3(get, set)]
    pub ul_code_page_range1: AzOptionU32,
    #[pyo3(get, set)]
    pub ul_code_page_range2: AzOptionU32,
    #[pyo3(get, set)]
    pub sx_height: AzOptionI16,
    #[pyo3(get, set)]
    pub s_cap_height: AzOptionI16,
    #[pyo3(get, set)]
    pub us_default_char: AzOptionU16,
    #[pyo3(get, set)]
    pub us_break_char: AzOptionU16,
    #[pyo3(get, set)]
    pub us_max_context: AzOptionU16,
    #[pyo3(get, set)]
    pub us_lower_optical_point_size: AzOptionU16,
    #[pyo3(get, set)]
    pub us_upper_optical_point_size: AzOptionU16,
}

/// Re-export of rust-allocated (stack based) `SvgLine` struct
#[repr(C)]
#[pyclass(name = "SvgLine")]
pub struct AzSvgLine {
    #[pyo3(get, set)]
    pub start: AzSvgPoint,
    #[pyo3(get, set)]
    pub end: AzSvgPoint,
}

/// Re-export of rust-allocated (stack based) `SvgQuadraticCurve` struct
#[repr(C)]
#[pyclass(name = "SvgQuadraticCurve")]
pub struct AzSvgQuadraticCurve {
    #[pyo3(get, set)]
    pub start: AzSvgPoint,
    #[pyo3(get, set)]
    pub ctrl: AzSvgPoint,
    #[pyo3(get, set)]
    pub end: AzSvgPoint,
}

/// Re-export of rust-allocated (stack based) `SvgCubicCurve` struct
#[repr(C)]
#[pyclass(name = "SvgCubicCurve")]
pub struct AzSvgCubicCurve {
    #[pyo3(get, set)]
    pub start: AzSvgPoint,
    #[pyo3(get, set)]
    pub ctrl_1: AzSvgPoint,
    #[pyo3(get, set)]
    pub ctrl_2: AzSvgPoint,
    #[pyo3(get, set)]
    pub end: AzSvgPoint,
}

/// Re-export of rust-allocated (stack based) `SvgStringFormatOptions` struct
#[repr(C)]
#[pyclass(name = "SvgStringFormatOptions")]
pub struct AzSvgStringFormatOptions {
    #[pyo3(get, set)]
    pub use_single_quote: bool,
    #[pyo3(get, set)]
    pub indent: AzIndent,
    #[pyo3(get, set)]
    pub attributes_indent: AzIndent,
}

/// Re-export of rust-allocated (stack based) `SvgFillStyle` struct
#[repr(C)]
#[pyclass(name = "SvgFillStyle")]
pub struct AzSvgFillStyle {
    #[pyo3(get, set)]
    pub line_join: AzSvgLineJoin,
    #[pyo3(get, set)]
    pub miter_limit: f32,
    #[pyo3(get, set)]
    pub tolerance: f32,
    #[pyo3(get, set)]
    pub fill_rule: AzSvgFillRule,
    #[pyo3(get, set)]
    pub transform: AzSvgTransform,
    #[pyo3(get, set)]
    pub anti_alias: bool,
    #[pyo3(get, set)]
    pub high_quality_aa: bool,
}

/// Re-export of rust-allocated (stack based) `InstantPtr` struct
#[repr(C)]
#[pyclass(name = "InstantPtr")]
pub struct AzInstantPtr {
    pub ptr: *const c_void,
    #[pyo3(get, set)]
    pub clone_fn: AzInstantPtrCloneFn,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub writeback_data: AzRefAny,
    #[pyo3(get, set)]
    pub check_thread_finished_fn: AzCheckThreadFinishedFn,
    #[pyo3(get, set)]
    pub send_thread_msg_fn: AzLibrarySendThreadMsgFn,
    #[pyo3(get, set)]
    pub receive_thread_msg_fn: AzLibraryReceiveThreadMsgFn,
    #[pyo3(get, set)]
    pub thread_destructor_fn: AzThreadDestructorFn,
}

/// Re-export of rust-allocated (stack based) `ThreadSender` struct
#[repr(C)]
#[pyclass(name = "ThreadSender")]
pub struct AzThreadSender {
    pub ptr: *const c_void,
    #[pyo3(get, set)]
    pub send_fn: AzThreadSendFn,
    #[pyo3(get, set)]
    pub destructor: AzThreadSenderDestructorFn,
}

/// Re-export of rust-allocated (stack based) `ThreadReceiver` struct
#[repr(C)]
#[pyclass(name = "ThreadReceiver")]
pub struct AzThreadReceiver {
    pub ptr: *const c_void,
    #[pyo3(get, set)]
    pub recv_fn: AzThreadRecvFn,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub data: AzRefAny,
    #[pyo3(get, set)]
    pub callback: AzWriteBackCallback,
}

/// Wrapper over a Rust-allocated `Vec<XmlNode>`
#[repr(C)]
#[pyclass(name = "XmlNodeVec")]
pub struct AzXmlNodeVec {
    pub(crate) ptr: *const AzXmlNode,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzXmlNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<InlineGlyph>`
#[repr(C)]
#[pyclass(name = "InlineGlyphVec")]
pub struct AzInlineGlyphVec {
    pub(crate) ptr: *const AzInlineGlyph,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzInlineGlyphVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<InlineTextHit>`
#[repr(C)]
#[pyclass(name = "InlineTextHitVec")]
pub struct AzInlineTextHitVec {
    pub(crate) ptr: *const AzInlineTextHit,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzInlineTextHitVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<VideoMode>`
#[repr(C)]
#[pyclass(name = "VideoModeVec")]
pub struct AzVideoModeVec {
    pub(crate) ptr: *const AzVideoMode,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzVideoModeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<Dom>`
#[repr(C)]
#[pyclass(name = "DomVec")]
pub struct AzDomVec {
    pub(crate) ptr: *const AzDom,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzDomVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundPosition>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundPositionVec")]
pub struct AzStyleBackgroundPositionVec {
    pub(crate) ptr: *const AzStyleBackgroundPosition,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStyleBackgroundPositionVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundRepeat>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundRepeatVec")]
pub struct AzStyleBackgroundRepeatVec {
    pub(crate) ptr: *const AzStyleBackgroundRepeat,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStyleBackgroundRepeatVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundSize>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundSizeVec")]
pub struct AzStyleBackgroundSizeVec {
    pub(crate) ptr: *const AzStyleBackgroundSize,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStyleBackgroundSizeVecDestructor,
}

/// Wrapper over a Rust-allocated `SvgVertex`
#[repr(C)]
#[pyclass(name = "SvgVertexVec")]
pub struct AzSvgVertexVec {
    pub(crate) ptr: *const AzSvgVertex,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzSvgVertexVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<u32>`
#[repr(C)]
#[pyclass(name = "U32Vec")]
pub struct AzU32Vec {
    pub ptr: *const u32,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzU32VecDestructor,
}

/// Wrapper over a Rust-allocated `XWindowType`
#[repr(C)]
#[pyclass(name = "XWindowTypeVec")]
pub struct AzXWindowTypeVec {
    pub(crate) ptr: *const AzXWindowType,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzXWindowTypeVecDestructor,
}

/// Wrapper over a Rust-allocated `VirtualKeyCode`
#[repr(C)]
#[pyclass(name = "VirtualKeyCodeVec")]
pub struct AzVirtualKeyCodeVec {
    pub(crate) ptr: *const AzVirtualKeyCode,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzVirtualKeyCodeVecDestructor,
}

/// Wrapper over a Rust-allocated `CascadeInfo`
#[repr(C)]
#[pyclass(name = "CascadeInfoVec")]
pub struct AzCascadeInfoVec {
    pub(crate) ptr: *const AzCascadeInfo,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzCascadeInfoVecDestructor,
}

/// Wrapper over a Rust-allocated `ScanCode`
#[repr(C)]
#[pyclass(name = "ScanCodeVec")]
pub struct AzScanCodeVec {
    pub ptr: *const u32,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzScanCodeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<u16>`
#[repr(C)]
#[pyclass(name = "U16Vec")]
pub struct AzU16Vec {
    pub ptr: *const u16,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzU16VecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<f32>`
#[repr(C)]
#[pyclass(name = "F32Vec")]
pub struct AzF32Vec {
    pub ptr: *const f32,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzF32VecDestructor,
}

/// Wrapper over a Rust-allocated `U8Vec`
#[repr(C)]
#[pyclass(name = "U8Vec")]
pub struct AzU8Vec {
    pub ptr: *const u8,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzU8VecDestructor,
}

/// Wrapper over a Rust-allocated `U32Vec`
#[repr(C)]
#[pyclass(name = "GLuintVec")]
pub struct AzGLuintVec {
    pub ptr: *const u32,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzGLuintVecDestructor,
}

/// Wrapper over a Rust-allocated `GLintVec`
#[repr(C)]
#[pyclass(name = "GLintVec")]
pub struct AzGLintVec {
    pub ptr: *const i32,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzGLintVecDestructor,
}

/// Wrapper over a Rust-allocated `NormalizedLinearColorStopVec`
#[repr(C)]
#[pyclass(name = "NormalizedLinearColorStopVec")]
pub struct AzNormalizedLinearColorStopVec {
    pub(crate) ptr: *const AzNormalizedLinearColorStop,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzNormalizedLinearColorStopVecDestructor,
}

/// Wrapper over a Rust-allocated `NormalizedRadialColorStopVec`
#[repr(C)]
#[pyclass(name = "NormalizedRadialColorStopVec")]
pub struct AzNormalizedRadialColorStopVec {
    pub(crate) ptr: *const AzNormalizedRadialColorStop,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzNormalizedRadialColorStopVecDestructor,
}

/// Wrapper over a Rust-allocated `NodeIdVec`
#[repr(C)]
#[pyclass(name = "NodeIdVec")]
pub struct AzNodeIdVec {
    pub(crate) ptr: *const AzNodeId,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzNodeIdVecDestructor,
}

/// Wrapper over a Rust-allocated `NodeVec`
#[repr(C)]
#[pyclass(name = "NodeVec")]
pub struct AzNodeVec {
    pub(crate) ptr: *const AzNode,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `ParentWithNodeDepthVec`
#[repr(C)]
#[pyclass(name = "ParentWithNodeDepthVec")]
pub struct AzParentWithNodeDepthVec {
    pub(crate) ptr: *const AzParentWithNodeDepth,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub ch: u32,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidCharError` struct
#[repr(C)]
#[pyclass(name = "InvalidCharError")]
pub struct AzInvalidCharError {
    #[pyo3(get, set)]
    pub expected: u8,
    #[pyo3(get, set)]
    pub got: u8,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidCharMultipleError` struct
#[repr(C)]
#[pyclass(name = "InvalidCharMultipleError")]
pub struct AzInvalidCharMultipleError {
    #[pyo3(get, set)]
    pub expected: u8,
    #[pyo3(get, set)]
    pub got: AzU8Vec,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidQuoteError` struct
#[repr(C)]
#[pyclass(name = "InvalidQuoteError")]
pub struct AzInvalidQuoteError {
    #[pyo3(get, set)]
    pub got: u8,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidSpaceError` struct
#[repr(C)]
#[pyclass(name = "InvalidSpaceError")]
pub struct AzInvalidSpaceError {
    #[pyo3(get, set)]
    pub got: u8,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Configuration for optional features, such as whether to enable logging or panic hooks
#[repr(C)]
#[pyclass(name = "AppConfig")]
pub struct AzAppConfig {
    #[pyo3(get, set)]
    pub layout_solver: AzLayoutSolverVersion,
    #[pyo3(get, set)]
    pub log_level: AzAppLogLevel,
    #[pyo3(get, set)]
    pub enable_visual_panic_hook: bool,
    #[pyo3(get, set)]
    pub enable_logging_on_panic: bool,
    #[pyo3(get, set)]
    pub enable_tab_navigation: bool,
    #[pyo3(get, set)]
    pub system_callbacks: AzSystemCallbacks,
}

/// Small (16x16x4) window icon, usually shown in the window titlebar
#[repr(C)]
#[pyclass(name = "SmallWindowIconBytes")]
pub struct AzSmallWindowIconBytes {
    #[pyo3(get, set)]
    pub key: AzIconKey,
    #[pyo3(get, set)]
    pub rgba_bytes: AzU8Vec,
}

/// Large (32x32x4) window icon, usually used on high-resolution displays (instead of `SmallWindowIcon`)
#[repr(C)]
#[pyclass(name = "LargeWindowIconBytes")]
pub struct AzLargeWindowIconBytes {
    #[pyo3(get, set)]
    pub key: AzIconKey,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub key: AzIconKey,
    #[pyo3(get, set)]
    pub rgba_bytes: AzU8Vec,
}

/// Minimum / maximum / current size of the window in logical dimensions
#[repr(C)]
#[pyclass(name = "WindowSize")]
pub struct AzWindowSize {
    #[pyo3(get, set)]
    pub dimensions: AzLogicalSize,
    #[pyo3(get, set)]
    pub hidpi_factor: f32,
    #[pyo3(get, set)]
    pub system_hidpi_factor: f32,
    #[pyo3(get, set)]
    pub min_dimensions: AzOptionLogicalSize,
    #[pyo3(get, set)]
    pub max_dimensions: AzOptionLogicalSize,
}

/// Current keyboard state, stores what keys / characters have been pressed
#[repr(C)]
#[pyclass(name = "KeyboardState")]
pub struct AzKeyboardState {
    #[pyo3(get, set)]
    pub shift_down: bool,
    #[pyo3(get, set)]
    pub ctrl_down: bool,
    #[pyo3(get, set)]
    pub alt_down: bool,
    #[pyo3(get, set)]
    pub super_down: bool,
    #[pyo3(get, set)]
    pub current_char: AzOptionChar,
    #[pyo3(get, set)]
    pub current_virtual_keycode: AzOptionVirtualKeyCode,
    #[pyo3(get, set)]
    pub pressed_virtual_keycodes: AzVirtualKeyCodeVec,
    #[pyo3(get, set)]
    pub pressed_scancodes: AzScanCodeVec,
}

/// Current mouse / cursor state
#[repr(C)]
#[pyclass(name = "MouseState")]
pub struct AzMouseState {
    #[pyo3(get, set)]
    pub mouse_cursor_type: AzOptionMouseCursorType,
    #[pyo3(get, set)]
    pub cursor_position: AzCursorPosition,
    #[pyo3(get, set)]
    pub is_cursor_locked: bool,
    #[pyo3(get, set)]
    pub left_down: bool,
    #[pyo3(get, set)]
    pub right_down: bool,
    #[pyo3(get, set)]
    pub middle_down: bool,
    #[pyo3(get, set)]
    pub scroll_x: AzOptionF32,
    #[pyo3(get, set)]
    pub scroll_y: AzOptionF32,
}

/// Re-export of rust-allocated (stack based) `InlineTextContents` struct
#[repr(C)]
#[pyclass(name = "InlineTextContents")]
pub struct AzInlineTextContents {
    #[pyo3(get, set)]
    pub glyphs: AzInlineGlyphVec,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub callback_node_id: AzDomNodeId,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub window_size: AzWindowSize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub animation_func: AzAnimationInterpolationFunction,
    #[pyo3(get, set)]
    pub parent_rect_width: f32,
    #[pyo3(get, set)]
    pub parent_rect_height: f32,
    #[pyo3(get, set)]
    pub current_rect_width: f32,
    #[pyo3(get, set)]
    pub current_rect_height: f32,
}

/// Re-export of rust-allocated (stack based) `LinearGradient` struct
#[repr(C)]
#[pyclass(name = "LinearGradient")]
pub struct AzLinearGradient {
    #[pyo3(get, set)]
    pub direction: AzDirection,
    #[pyo3(get, set)]
    pub extend_mode: AzExtendMode,
    #[pyo3(get, set)]
    pub stops: AzNormalizedLinearColorStopVec,
}

/// Re-export of rust-allocated (stack based) `RadialGradient` struct
#[repr(C)]
#[pyclass(name = "RadialGradient")]
pub struct AzRadialGradient {
    #[pyo3(get, set)]
    pub shape: AzShape,
    #[pyo3(get, set)]
    pub size: AzRadialGradientSize,
    #[pyo3(get, set)]
    pub position: AzStyleBackgroundPosition,
    #[pyo3(get, set)]
    pub extend_mode: AzExtendMode,
    #[pyo3(get, set)]
    pub stops: AzNormalizedLinearColorStopVec,
}

/// Re-export of rust-allocated (stack based) `ConicGradient` struct
#[repr(C)]
#[pyclass(name = "ConicGradient")]
pub struct AzConicGradient {
    #[pyo3(get, set)]
    pub extend_mode: AzExtendMode,
    #[pyo3(get, set)]
    pub center: AzStyleBackgroundPosition,
    #[pyo3(get, set)]
    pub angle: AzAngleValue,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub state: AzStyledNodeState,
    #[pyo3(get, set)]
    pub tag_id: AzOptionTagId,
}

/// Re-export of rust-allocated (stack based) `TagIdToNodeIdMapping` struct
#[repr(C)]
#[pyclass(name = "TagIdToNodeIdMapping")]
pub struct AzTagIdToNodeIdMapping {
    #[pyo3(get, set)]
    pub tag_id: AzTagId,
    #[pyo3(get, set)]
    pub node_id: AzNodeId,
    #[pyo3(get, set)]
    pub tab_index: AzOptionTabIndex,
    #[pyo3(get, set)]
    pub parents: AzNodeIdVec,
}

/// Re-export of rust-allocated (stack based) `Texture` struct
#[repr(C)]
#[pyclass(name = "Texture")]
pub struct AzTexture {
    #[pyo3(get, set)]
    pub texture_id: u32,
    #[pyo3(get, set)]
    pub format: AzRawImageFormat,
    #[pyo3(get, set)]
    pub flags: AzTextureFlags,
    #[pyo3(get, set)]
    pub size: AzPhysicalSizeU32,
    #[pyo3(get, set)]
    pub gl_context: AzGl,
}

/// C-ABI stable reexport of `(U8Vec, u32)`
#[repr(C)]
#[pyclass(name = "GetProgramBinaryReturn")]
pub struct AzGetProgramBinaryReturn {
    #[pyo3(get, set)]
    pub _0: AzU8Vec,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub data: AzU8Vec,
    #[pyo3(get, set)]
    pub font_index: u32,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub vertices: AzSvgVertexVec,
    #[pyo3(get, set)]
    pub indices: AzU32Vec,
}

/// Rust wrapper over a `&[TesselatedSvgNode]` or `&Vec<TesselatedSvgNode>`
#[repr(C)]
#[pyclass(name = "TesselatedSvgNodeVecRef")]
pub struct AzTesselatedSvgNodeVecRef {
    pub(crate) ptr: *const AzTesselatedSvgNode,
    #[pyo3(get, set)]
    pub len: usize,
}

/// Re-export of rust-allocated (stack based) `SvgRenderOptions` struct
#[repr(C)]
#[pyclass(name = "SvgRenderOptions")]
pub struct AzSvgRenderOptions {
    #[pyo3(get, set)]
    pub target_size: AzOptionLayoutSize,
    #[pyo3(get, set)]
    pub background_color: AzOptionColorU,
    #[pyo3(get, set)]
    pub fit: AzSvgFitTo,
}

/// Re-export of rust-allocated (stack based) `SvgStrokeStyle` struct
#[repr(C)]
#[pyclass(name = "SvgStrokeStyle")]
pub struct AzSvgStrokeStyle {
    #[pyo3(get, set)]
    pub start_cap: AzSvgLineCap,
    #[pyo3(get, set)]
    pub end_cap: AzSvgLineCap,
    #[pyo3(get, set)]
    pub line_join: AzSvgLineJoin,
    #[pyo3(get, set)]
    pub dash_pattern: AzOptionSvgDashPattern,
    #[pyo3(get, set)]
    pub line_width: f32,
    #[pyo3(get, set)]
    pub miter_limit: f32,
    #[pyo3(get, set)]
    pub tolerance: f32,
    #[pyo3(get, set)]
    pub apply_line_width: bool,
    #[pyo3(get, set)]
    pub transform: AzSvgTransform,
    #[pyo3(get, set)]
    pub anti_alias: bool,
    #[pyo3(get, set)]
    pub high_quality_aa: bool,
}

/// Re-export of rust-allocated (stack based) `Xml` struct
#[repr(C)]
#[pyclass(name = "Xml")]
pub struct AzXml {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub vec: AzU8Vec,
}

/// Wrapper over a Rust-allocated `Vec<TesselatedSvgNode>`
#[repr(C)]
#[pyclass(name = "TesselatedSvgNodeVec")]
pub struct AzTesselatedSvgNodeVec {
    pub(crate) ptr: *const AzTesselatedSvgNode,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzTesselatedSvgNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleTransform>`
#[repr(C)]
#[pyclass(name = "StyleTransformVec")]
pub struct AzStyleTransformVec {
    pub(crate) ptr: *const AzStyleTransform,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStyleTransformVecDestructor,
}

/// Wrapper over a Rust-allocated `VertexAttribute`
#[repr(C)]
#[pyclass(name = "SvgPathElementVec")]
pub struct AzSvgPathElementVec {
    pub(crate) ptr: *const AzSvgPathElement,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzSvgPathElementVecDestructor,
}

/// Wrapper over a Rust-allocated `StringVec`
#[repr(C)]
#[pyclass(name = "StringVec")]
pub struct AzStringVec {
    pub(crate) ptr: *const AzString,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStringVecDestructor,
}

/// Wrapper over a Rust-allocated `StyledNodeVec`
#[repr(C)]
#[pyclass(name = "StyledNodeVec")]
pub struct AzStyledNodeVec {
    pub(crate) ptr: *const AzStyledNode,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStyledNodeVecDestructor,
}

/// Wrapper over a Rust-allocated `TagIdsToNodeIdsMappingVec`
#[repr(C)]
#[pyclass(name = "TagIdsToNodeIdsMappingVec")]
pub struct AzTagIdsToNodeIdsMappingVec {
    pub(crate) ptr: *const AzTagIdToNodeIdMapping,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub ns: AzString,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `UnknownNamespaceError` struct
#[repr(C)]
#[pyclass(name = "UnknownNamespaceError")]
pub struct AzUnknownNamespaceError {
    #[pyo3(get, set)]
    pub ns: AzString,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `UnexpectedCloseTagError` struct
#[repr(C)]
#[pyclass(name = "UnexpectedCloseTagError")]
pub struct AzUnexpectedCloseTagError {
    #[pyo3(get, set)]
    pub expected: AzString,
    #[pyo3(get, set)]
    pub actual: AzString,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `UnknownEntityReferenceError` struct
#[repr(C)]
#[pyclass(name = "UnknownEntityReferenceError")]
pub struct AzUnknownEntityReferenceError {
    #[pyo3(get, set)]
    pub entity: AzString,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `DuplicatedAttributeError` struct
#[repr(C)]
#[pyclass(name = "DuplicatedAttributeError")]
pub struct AzDuplicatedAttributeError {
    #[pyo3(get, set)]
    pub attribute: AzString,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Re-export of rust-allocated (stack based) `InvalidStringError` struct
#[repr(C)]
#[pyclass(name = "InvalidStringError")]
pub struct AzInvalidStringError {
    #[pyo3(get, set)]
    pub got: AzString,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Window configuration specific to Win32
#[repr(C)]
#[pyclass(name = "WindowsWindowOptions")]
pub struct AzWindowsWindowOptions {
    #[pyo3(get, set)]
    pub allow_drag_drop: bool,
    #[pyo3(get, set)]
    pub no_redirection_bitmap: bool,
    #[pyo3(get, set)]
    pub window_icon: AzOptionWindowIcon,
    #[pyo3(get, set)]
    pub taskbar_icon: AzOptionTaskBarIcon,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub title_bar_font: AzString,
    #[pyo3(get, set)]
    pub title_bar_font_size: f32,
}

/// Key-value pair, used for setting WM hints values specific to GNOME
#[repr(C)]
#[pyclass(name = "StringPair")]
pub struct AzStringPair {
    #[pyo3(get, set)]
    pub key: AzString,
    #[pyo3(get, set)]
    pub value: AzString,
}

/// Information about a single (or many) monitors, useful for dock widgets
#[repr(C)]
#[pyclass(name = "Monitor")]
pub struct AzMonitor {
    #[pyo3(get, set)]
    pub id: usize,
    #[pyo3(get, set)]
    pub name: AzOptionString,
    #[pyo3(get, set)]
    pub size: AzLayoutSize,
    #[pyo3(get, set)]
    pub position: AzLayoutPoint,
    #[pyo3(get, set)]
    pub scale_factor: f64,
    #[pyo3(get, set)]
    pub video_modes: AzVideoModeVec,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub event: AzEventFilter,
    #[pyo3(get, set)]
    pub callback: AzCallback,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub width: AzLayoutWidth,
    #[pyo3(get, set)]
    pub padding_left: AzLayoutPaddingLeft,
    #[pyo3(get, set)]
    pub padding_right: AzLayoutPaddingRight,
    #[pyo3(get, set)]
    pub track: AzStyleBackgroundContent,
    #[pyo3(get, set)]
    pub thumb: AzStyleBackgroundContent,
    #[pyo3(get, set)]
    pub button: AzStyleBackgroundContent,
    #[pyo3(get, set)]
    pub corner: AzStyleBackgroundContent,
    #[pyo3(get, set)]
    pub resizer: AzStyleBackgroundContent,
}

/// Re-export of rust-allocated (stack based) `ScrollbarStyle` struct
#[repr(C)]
#[pyclass(name = "ScrollbarStyle")]
pub struct AzScrollbarStyle {
    #[pyo3(get, set)]
    pub horizontal: AzScrollbarInfo,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub name: AzString,
    #[pyo3(get, set)]
    pub layout_location: AzOptionUsize,
    #[pyo3(get, set)]
    pub attribute_type: AzVertexAttributeType,
    #[pyo3(get, set)]
    pub item_count: usize,
}

/// Re-export of rust-allocated (stack based) `DebugMessage` struct
#[repr(C)]
#[pyclass(name = "DebugMessage")]
pub struct AzDebugMessage {
    #[pyo3(get, set)]
    pub message: AzString,
    #[pyo3(get, set)]
    pub source: u32,
    #[pyo3(get, set)]
    pub ty: u32,
    #[pyo3(get, set)]
    pub id: u32,
    #[pyo3(get, set)]
    pub severity: u32,
}

/// C-ABI stable reexport of `(i32, u32, AzString)`
#[repr(C)]
#[pyclass(name = "GetActiveAttribReturn")]
pub struct AzGetActiveAttribReturn {
    #[pyo3(get, set)]
    pub _0: i32,
    #[pyo3(get, set)]
    pub _1: u32,
    #[pyo3(get, set)]
    pub _2: AzString,
}

/// C-ABI stable reexport of `(i32, u32, AzString)`
#[repr(C)]
#[pyclass(name = "GetActiveUniformReturn")]
pub struct AzGetActiveUniformReturn {
    #[pyo3(get, set)]
    pub _0: i32,
    #[pyo3(get, set)]
    pub _1: u32,
    #[pyo3(get, set)]
    pub _2: AzString,
}

/// Re-export of rust-allocated (stack based) `RawImage` struct
#[repr(C)]
#[pyclass(name = "RawImage")]
pub struct AzRawImage {
    #[pyo3(get, set)]
    pub pixels: AzRawImageData,
    #[pyo3(get, set)]
    pub width: usize,
    #[pyo3(get, set)]
    pub height: usize,
    #[pyo3(get, set)]
    pub alpha_premultiplied: bool,
    #[pyo3(get, set)]
    pub data_format: AzRawImageFormat,
}

/// Re-export of rust-allocated (stack based) `SvgPath` struct
#[repr(C)]
#[pyclass(name = "SvgPath")]
pub struct AzSvgPath {
    #[pyo3(get, set)]
    pub items: AzSvgPathElementVec,
}

/// Re-export of rust-allocated (stack based) `SvgParseOptions` struct
#[repr(C)]
#[pyclass(name = "SvgParseOptions")]
pub struct AzSvgParseOptions {
    #[pyo3(get, set)]
    pub relative_image_path: AzOptionString,
    #[pyo3(get, set)]
    pub dpi: f32,
    #[pyo3(get, set)]
    pub default_font_family: AzString,
    #[pyo3(get, set)]
    pub font_size: f32,
    #[pyo3(get, set)]
    pub languages: AzStringVec,
    #[pyo3(get, set)]
    pub shape_rendering: AzShapeRendering,
    #[pyo3(get, set)]
    pub text_rendering: AzTextRendering,
    #[pyo3(get, set)]
    pub image_rendering: AzImageRendering,
    #[pyo3(get, set)]
    pub keep_named_groups: bool,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub document_types: AzStringVec,
    #[pyo3(get, set)]
    pub document_descriptor: AzString,
}

/// Re-export of rust-allocated (stack based) `Timer` struct
#[repr(C)]
#[pyclass(name = "Timer")]
pub struct AzTimer {
    #[pyo3(get, set)]
    pub data: AzRefAny,
    #[pyo3(get, set)]
    pub node_id: AzOptionDomNodeId,
    #[pyo3(get, set)]
    pub created: AzInstant,
    #[pyo3(get, set)]
    pub last_run: AzOptionInstant,
    #[pyo3(get, set)]
    pub run_count: usize,
    #[pyo3(get, set)]
    pub delay: AzOptionDuration,
    #[pyo3(get, set)]
    pub interval: AzOptionDuration,
    #[pyo3(get, set)]
    pub timeout: AzOptionDuration,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub key: AzString,
    #[pyo3(get, set)]
    pub value: AzFmtValue,
}

/// Wrapper over a Rust-allocated `Vec<StyleFontFamily>`
#[repr(C)]
#[pyclass(name = "StyleFontFamilyVec")]
pub struct AzStyleFontFamilyVec {
    pub(crate) ptr: *const AzStyleFontFamily,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStyleFontFamilyVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<FmtArg>`
#[repr(C)]
#[pyclass(name = "FmtArgVec")]
pub struct AzFmtArgVec {
    pub(crate) ptr: *const AzFmtArg,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzFmtArgVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<InlineWord>`
#[repr(C)]
#[pyclass(name = "InlineWordVec")]
pub struct AzInlineWordVec {
    pub(crate) ptr: *const AzInlineWord,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzInlineWordVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<Monitor>`
#[repr(C)]
#[pyclass(name = "MonitorVec")]
pub struct AzMonitorVec {
    pub(crate) ptr: *const AzMonitor,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzMonitorVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<IdOrClass>`
#[repr(C)]
#[pyclass(name = "IdOrClassVec")]
pub struct AzIdOrClassVec {
    pub(crate) ptr: *const AzIdOrClass,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzIdOrClassVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<StyleBackgroundContent>`
#[repr(C)]
#[pyclass(name = "StyleBackgroundContentVec")]
pub struct AzStyleBackgroundContentVec {
    pub(crate) ptr: *const AzStyleBackgroundContent,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzStyleBackgroundContentVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<SvgPath>`
#[repr(C)]
#[pyclass(name = "SvgPathVec")]
pub struct AzSvgPathVec {
    pub(crate) ptr: *const AzSvgPath,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzSvgPathVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<VertexAttribute>`
#[repr(C)]
#[pyclass(name = "VertexAttributeVec")]
pub struct AzVertexAttributeVec {
    pub(crate) ptr: *const AzVertexAttribute,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzVertexAttributeVecDestructor,
}

/// Wrapper over a Rust-allocated `CssPathSelector`
#[repr(C)]
#[pyclass(name = "CssPathSelectorVec")]
pub struct AzCssPathSelectorVec {
    pub(crate) ptr: *const AzCssPathSelector,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzCssPathSelectorVecDestructor,
}

/// Wrapper over a Rust-allocated `CallbackData`
#[repr(C)]
#[pyclass(name = "CallbackDataVec")]
pub struct AzCallbackDataVec {
    pub(crate) ptr: *const AzCallbackData,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzCallbackDataVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<DebugMessage>`
#[repr(C)]
#[pyclass(name = "DebugMessageVec")]
pub struct AzDebugMessageVec {
    pub(crate) ptr: *const AzDebugMessage,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzDebugMessageVecDestructor,
}

/// Wrapper over a Rust-allocated `StringPairVec`
#[repr(C)]
#[pyclass(name = "StringPairVec")]
pub struct AzStringPairVec {
    pub(crate) ptr: *const AzStringPair,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub x11_visual: AzOptionX11Visual,
    #[pyo3(get, set)]
    pub x11_screen: AzOptionI32,
    #[pyo3(get, set)]
    pub x11_wm_classes: AzStringPairVec,
    #[pyo3(get, set)]
    pub x11_override_redirect: bool,
    #[pyo3(get, set)]
    pub x11_window_types: AzXWindowTypeVec,
    #[pyo3(get, set)]
    pub x11_gtk_theme_variant: AzOptionString,
    #[pyo3(get, set)]
    pub x11_resize_increments: AzOptionLogicalSize,
    #[pyo3(get, set)]
    pub x11_base_size: AzOptionLogicalSize,
    #[pyo3(get, set)]
    pub wayland_app_id: AzOptionString,
    #[pyo3(get, set)]
    pub wayland_theme: AzOptionWaylandTheme,
    #[pyo3(get, set)]
    pub request_user_attention: bool,
    #[pyo3(get, set)]
    pub window_icon: AzOptionWindowIcon,
}

/// Re-export of rust-allocated (stack based) `InlineLine` struct
#[repr(C)]
#[pyclass(name = "InlineLine")]
pub struct AzInlineLine {
    #[pyo3(get, set)]
    pub words: AzInlineWordVec,
    #[pyo3(get, set)]
    pub bounds: AzLogicalRect,
}

/// Re-export of rust-allocated (stack based) `CssPath` struct
#[repr(C)]
#[pyclass(name = "CssPath")]
pub struct AzCssPath {
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub fields: AzVertexAttributeVec,
}

/// Re-export of rust-allocated (stack based) `VertexArrayObject` struct
#[repr(C)]
#[pyclass(name = "VertexArrayObject")]
pub struct AzVertexArrayObject {
    #[pyo3(get, set)]
    pub vertex_layout: AzVertexLayout,
    #[pyo3(get, set)]
    pub vao_id: u32,
    #[pyo3(get, set)]
    pub gl_context: AzGl,
}

/// Re-export of rust-allocated (stack based) `VertexBuffer` struct
#[repr(C)]
#[pyclass(name = "VertexBuffer")]
pub struct AzVertexBuffer {
    #[pyo3(get, set)]
    pub vertex_buffer_id: u32,
    #[pyo3(get, set)]
    pub vertex_buffer_len: usize,
    #[pyo3(get, set)]
    pub vao: AzVertexArrayObject,
    #[pyo3(get, set)]
    pub index_buffer_id: u32,
    #[pyo3(get, set)]
    pub index_buffer_len: usize,
    #[pyo3(get, set)]
    pub index_buffer_format: AzIndexBufferFormat,
}

/// Re-export of rust-allocated (stack based) `SvgMultiPolygon` struct
#[repr(C)]
#[pyclass(name = "SvgMultiPolygon")]
pub struct AzSvgMultiPolygon {
    #[pyo3(get, set)]
    pub rings: AzSvgPathVec,
}

/// Re-export of rust-allocated (stack based) `XmlNode` struct
#[repr(C)]
#[pyclass(name = "XmlNode")]
pub struct AzXmlNode {
    #[pyo3(get, set)]
    pub tag: AzString,
    #[pyo3(get, set)]
    pub attributes: AzStringPairVec,
    #[pyo3(get, set)]
    pub children: AzXmlNodeVec,
    #[pyo3(get, set)]
    pub text: AzOptionString,
}

/// Wrapper over a Rust-allocated `Vec<InlineLine>`
#[repr(C)]
#[pyclass(name = "InlineLineVec")]
pub struct AzInlineLineVec {
    pub(crate) ptr: *const AzInlineLine,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzInlineLineVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<CssProperty>`
#[repr(C)]
#[pyclass(name = "CssPropertyVec")]
pub struct AzCssPropertyVec {
    pub(crate) ptr: *const AzCssProperty,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzCssPropertyVecDestructor,
}

/// Wrapper over a Rust-allocated `Vec<SvgMultiPolygon>`
#[repr(C)]
#[pyclass(name = "SvgMultiPolygonVec")]
pub struct AzSvgMultiPolygonVec {
    pub(crate) ptr: *const AzSvgMultiPolygon,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub stream_error: AzXmlStreamError,
    #[pyo3(get, set)]
    pub pos: AzSvgParseErrorPosition,
}

/// Platform-specific window configuration, i.e. WM options that are not cross-platform
#[repr(C)]
#[pyclass(name = "PlatformSpecificOptions")]
pub struct AzPlatformSpecificOptions {
    #[pyo3(get, set)]
    pub windows_options: AzWindowsWindowOptions,
    #[pyo3(get, set)]
    pub linux_options: AzLinuxWindowOptions,
    #[pyo3(get, set)]
    pub mac_options: AzMacWindowOptions,
    #[pyo3(get, set)]
    pub wasm_options: AzWasmWindowOptions,
}

/// Re-export of rust-allocated (stack based) `WindowState` struct
#[repr(C)]
#[pyclass(name = "WindowState")]
pub struct AzWindowState {
    #[pyo3(get, set)]
    pub title: AzString,
    #[pyo3(get, set)]
    pub theme: AzWindowTheme,
    #[pyo3(get, set)]
    pub size: AzWindowSize,
    #[pyo3(get, set)]
    pub position: AzWindowPosition,
    #[pyo3(get, set)]
    pub flags: AzWindowFlags,
    #[pyo3(get, set)]
    pub debug_state: AzDebugState,
    #[pyo3(get, set)]
    pub keyboard_state: AzKeyboardState,
    #[pyo3(get, set)]
    pub mouse_state: AzMouseState,
    #[pyo3(get, set)]
    pub touch_state: AzTouchState,
    #[pyo3(get, set)]
    pub ime_position: AzImePosition,
    #[pyo3(get, set)]
    pub monitor: AzMonitor,
    #[pyo3(get, set)]
    pub platform_specific_options: AzPlatformSpecificOptions,
    #[pyo3(get, set)]
    pub renderer_options: AzRendererOptions,
    #[pyo3(get, set)]
    pub background_color: AzColorU,
    #[pyo3(get, set)]
    pub layout_callback: AzLayoutCallback,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub hit_dom_node: AzDomNodeId,
    #[pyo3(get, set)]
    pub cursor_relative_to_item: AzOptionLogicalPosition,
    #[pyo3(get, set)]
    pub cursor_in_viewport: AzOptionLogicalPosition,
    pub _reserved_ref: *const c_void,
    pub _reserved_mut: *mut c_void,
}

/// Re-export of rust-allocated (stack based) `InlineText` struct
#[repr(C)]
#[pyclass(name = "InlineText")]
pub struct AzInlineText {
    #[pyo3(get, set)]
    pub lines: AzInlineLineVec,
    #[pyo3(get, set)]
    pub content_size: AzLogicalSize,
    #[pyo3(get, set)]
    pub font_size_px: f32,
    #[pyo3(get, set)]
    pub last_word_index: usize,
    #[pyo3(get, set)]
    pub baseline_descender_px: f32,
}

/// CSS path to set the keyboard input focus
#[repr(C)]
#[pyclass(name = "FocusTargetPath")]
pub struct AzFocusTargetPath {
    #[pyo3(get, set)]
    pub dom: AzDomId,
    #[pyo3(get, set)]
    pub css_path: AzCssPath,
}

/// Animation struct to start a new animation
#[repr(C)]
#[pyclass(name = "Animation")]
pub struct AzAnimation {
    #[pyo3(get, set)]
    pub from: AzCssProperty,
    #[pyo3(get, set)]
    pub to: AzCssProperty,
    #[pyo3(get, set)]
    pub duration: AzDuration,
    #[pyo3(get, set)]
    pub repeat: AzAnimationRepeat,
    #[pyo3(get, set)]
    pub repeat_count: AzAnimationRepeatCount,
    #[pyo3(get, set)]
    pub easing: AzAnimationEasing,
    #[pyo3(get, set)]
    pub relayout_on_finish: bool,
}

/// Re-export of rust-allocated (stack based) `TimerCallbackInfo` struct
#[repr(C)]
#[pyclass(name = "TimerCallbackInfo")]
pub struct AzTimerCallbackInfo {
    #[pyo3(get, set)]
    pub callback_info: AzCallbackInfo,
    #[pyo3(get, set)]
    pub node_id: AzOptionDomNodeId,
    #[pyo3(get, set)]
    pub frame_start: AzInstant,
    #[pyo3(get, set)]
    pub call_count: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub dynamic_id: AzString,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub geometry: AzSvgNode,
    #[pyo3(get, set)]
    pub style: AzSvgStyle,
}

/// Wrapper over a Rust-allocated `Vec<NodeDataInlineCssProperty>`
#[repr(C)]
#[pyclass(name = "NodeDataInlineCssPropertyVec")]
pub struct AzNodeDataInlineCssPropertyVec {
    pub(crate) ptr: *const AzNodeDataInlineCssProperty,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub state: AzWindowState,
    #[pyo3(get, set)]
    pub renderer_type: AzOptionRendererOptions,
    #[pyo3(get, set)]
    pub theme: AzOptionWindowTheme,
    #[pyo3(get, set)]
    pub create_callback: AzOptionCallback,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub node_type: AzNodeType,
    #[pyo3(get, set)]
    pub dataset: AzOptionRefAny,
    #[pyo3(get, set)]
    pub ids_and_classes: AzIdOrClassVec,
    #[pyo3(get, set)]
    pub callbacks: AzCallbackDataVec,
    #[pyo3(get, set)]
    pub inline_css_props: AzNodeDataInlineCssPropertyVec,
    #[pyo3(get, set)]
    pub clip_mask: AzOptionImageMask,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
    pub destructor: AzCssDeclarationVecDestructor,
}

/// Wrapper over a Rust-allocated `NodeDataVec`
#[repr(C)]
#[pyclass(name = "NodeDataVec")]
pub struct AzNodeDataVec {
    pub(crate) ptr: *const AzNodeData,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub root: AzNodeData,
    #[pyo3(get, set)]
    pub children: AzDomVec,
    #[pyo3(get, set)]
    pub total_children: usize,
}

/// Re-export of rust-allocated (stack based) `CssRuleBlock` struct
#[repr(C)]
#[pyclass(name = "CssRuleBlock")]
pub struct AzCssRuleBlock {
    #[pyo3(get, set)]
    pub path: AzCssPath,
    #[pyo3(get, set)]
    pub declarations: AzCssDeclarationVec,
}

/// Re-export of rust-allocated (stack based) `StyledDom` struct
#[repr(C)]
#[pyclass(name = "StyledDom")]
pub struct AzStyledDom {
    #[pyo3(get, set)]
    pub root: AzNodeId,
    #[pyo3(get, set)]
    pub node_hierarchy: AzNodeVec,
    #[pyo3(get, set)]
    pub node_data: AzNodeDataVec,
    #[pyo3(get, set)]
    pub styled_nodes: AzStyledNodeVec,
    #[pyo3(get, set)]
    pub cascade_info: AzCascadeInfoVec,
    #[pyo3(get, set)]
    pub nodes_with_window_callbacks: AzNodeIdVec,
    #[pyo3(get, set)]
    pub nodes_with_not_callbacks: AzNodeIdVec,
    #[pyo3(get, set)]
    pub nodes_with_datasets_and_callbacks: AzNodeIdVec,
    #[pyo3(get, set)]
    pub tag_ids_to_node_ids: AzTagIdsToNodeIdsMappingVec,
    #[pyo3(get, set)]
    pub non_leaf_nodes: AzParentWithNodeDepthVec,
    #[pyo3(get, set)]
    pub css_property_cache: AzCssPropertyCache,
}

/// Wrapper over a Rust-allocated `CssRuleBlock`
#[repr(C)]
#[pyclass(name = "CssRuleBlockVec")]
pub struct AzCssRuleBlockVec {
    pub(crate) ptr: *const AzCssRuleBlock,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
    pub dom: AzStyledDom,
    #[pyo3(get, set)]
    pub scroll_size: AzLogicalSize,
    #[pyo3(get, set)]
    pub scroll_offset: AzLogicalPosition,
    #[pyo3(get, set)]
    pub virtual_scroll_size: AzLogicalSize,
    #[pyo3(get, set)]
    pub virtual_scroll_offset: AzLogicalPosition,
}

/// Re-export of rust-allocated (stack based) `Stylesheet` struct
#[repr(C)]
#[pyclass(name = "Stylesheet")]
pub struct AzStylesheet {
    #[pyo3(get, set)]
    pub rules: AzCssRuleBlockVec,
}

/// Wrapper over a Rust-allocated `Stylesheet`
#[repr(C)]
#[pyclass(name = "StylesheetVec")]
pub struct AzStylesheetVec {
    pub(crate) ptr: *const AzStylesheet,
    #[pyo3(get, set)]
    pub len: usize,
    #[pyo3(get, set)]
    pub cap: usize,
    #[pyo3(get, set)]
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
    #[pyo3(get, set)]
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
    fn add_window(&mut self, window: AzWindowCreateOptions) -> () {
        ()
    }
    fn add_image(&mut self, id: String, image: AzImageRef) -> () {
        ()
    }
    fn get_monitors(&self) -> AzMonitorVec {
        ()
    }
    fn run(self, window: AzWindowCreateOptions) -> () {
        ()
    }
}

#[pymethods]
impl AzAppLogLevelEnumWrapper {
    #[classattr]
    fn Off() -> AzAppLogLevelEnumWrapper { AzAppLogLevelEnumWrapper::Off }
    #[classattr]
    fn Error() -> AzAppLogLevelEnumWrapper { AzAppLogLevelEnumWrapper::Error }
    #[classattr]
    fn Warn() -> AzAppLogLevelEnumWrapper { AzAppLogLevelEnumWrapper::Warn }
    #[classattr]
    fn Info() -> AzAppLogLevelEnumWrapper { AzAppLogLevelEnumWrapper::Info }
    #[classattr]
    fn Debug() -> AzAppLogLevelEnumWrapper { AzAppLogLevelEnumWrapper::Debug }
    #[classattr]
    fn Trace() -> AzAppLogLevelEnumWrapper { AzAppLogLevelEnumWrapper::Trace }
}

#[pymethods]
impl AzLayoutSolverVersionEnumWrapper {
    #[classattr]
    fn March2021() -> AzLayoutSolverVersionEnumWrapper { AzLayoutSolverVersionEnumWrapper::March2021 }
}

#[pymethods]
impl AzSystemCallbacks {
    #[staticmethod]
    fn library_internal() -> AzSystemCallbacks {
        unsafe { mem::transmute(crate::AzSystemCallbacks_libraryInternal()) }
    }
}

#[pymethods]
impl AzWindowCreateOptions {
}

#[pymethods]
impl AzVsyncEnumWrapper {
    #[classattr]
    fn Enabled() -> AzVsyncEnumWrapper { AzVsyncEnumWrapper::Enabled }
    #[classattr]
    fn Disabled() -> AzVsyncEnumWrapper { AzVsyncEnumWrapper::Disabled }
}

#[pymethods]
impl AzSrgbEnumWrapper {
    #[classattr]
    fn Enabled() -> AzSrgbEnumWrapper { AzSrgbEnumWrapper::Enabled }
    #[classattr]
    fn Disabled() -> AzSrgbEnumWrapper { AzSrgbEnumWrapper::Disabled }
}

#[pymethods]
impl AzHwAccelerationEnumWrapper {
    #[classattr]
    fn Enabled() -> AzHwAccelerationEnumWrapper { AzHwAccelerationEnumWrapper::Enabled }
    #[classattr]
    fn Disabled() -> AzHwAccelerationEnumWrapper { AzHwAccelerationEnumWrapper::Disabled }
}

#[pymethods]
impl AzRawWindowHandleEnumWrapper {
    #[staticmethod]
    fn IOS(v: IOSHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::IOS(v) }
    #[staticmethod]
    fn MacOS(v: MacOSHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::MacOS(v) }
    #[staticmethod]
    fn Xlib(v: XlibHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::Xlib(v) }
    #[staticmethod]
    fn Xcb(v: XcbHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::Xcb(v) }
    #[staticmethod]
    fn Wayland(v: WaylandHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::Wayland(v) }
    #[staticmethod]
    fn Windows(v: WindowsHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::Windows(v) }
    #[staticmethod]
    fn Web(v: WebHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::Web(v) }
    #[staticmethod]
    fn Android(v: AndroidHandle) -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::Android(v) }
    #[classattr]
    fn Unsupported() -> AzRawWindowHandleEnumWrapper { AzRawWindowHandleEnumWrapper::Unsupported }
}

#[pymethods]
impl AzXWindowTypeEnumWrapper {
    #[classattr]
    fn Desktop() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Desktop }
    #[classattr]
    fn Dock() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Dock }
    #[classattr]
    fn Toolbar() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Toolbar }
    #[classattr]
    fn Menu() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Menu }
    #[classattr]
    fn Utility() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Utility }
    #[classattr]
    fn Splash() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Splash }
    #[classattr]
    fn Dialog() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Dialog }
    #[classattr]
    fn DropdownMenu() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::DropdownMenu }
    #[classattr]
    fn PopupMenu() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::PopupMenu }
    #[classattr]
    fn Tooltip() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Tooltip }
    #[classattr]
    fn Notification() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Notification }
    #[classattr]
    fn Combo() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Combo }
    #[classattr]
    fn Dnd() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Dnd }
    #[classattr]
    fn Normal() -> AzXWindowTypeEnumWrapper { AzXWindowTypeEnumWrapper::Normal }
}

#[pymethods]
impl AzWindowIconEnumWrapper {
    #[staticmethod]
    fn Small(v: SmallWindowIconBytes) -> AzWindowIconEnumWrapper { AzWindowIconEnumWrapper::Small(v) }
    #[staticmethod]
    fn Large(v: LargeWindowIconBytes) -> AzWindowIconEnumWrapper { AzWindowIconEnumWrapper::Large(v) }
}

#[pymethods]
impl AzVirtualKeyCodeEnumWrapper {
    #[classattr]
    fn Key1() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key1 }
    #[classattr]
    fn Key2() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key2 }
    #[classattr]
    fn Key3() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key3 }
    #[classattr]
    fn Key4() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key4 }
    #[classattr]
    fn Key5() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key5 }
    #[classattr]
    fn Key6() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key6 }
    #[classattr]
    fn Key7() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key7 }
    #[classattr]
    fn Key8() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key8 }
    #[classattr]
    fn Key9() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key9 }
    #[classattr]
    fn Key0() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Key0 }
    #[classattr]
    fn A() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::A }
    #[classattr]
    fn B() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::B }
    #[classattr]
    fn C() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::C }
    #[classattr]
    fn D() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::D }
    #[classattr]
    fn E() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::E }
    #[classattr]
    fn F() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F }
    #[classattr]
    fn G() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::G }
    #[classattr]
    fn H() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::H }
    #[classattr]
    fn I() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::I }
    #[classattr]
    fn J() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::J }
    #[classattr]
    fn K() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::K }
    #[classattr]
    fn L() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::L }
    #[classattr]
    fn M() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::M }
    #[classattr]
    fn N() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::N }
    #[classattr]
    fn O() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::O }
    #[classattr]
    fn P() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::P }
    #[classattr]
    fn Q() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Q }
    #[classattr]
    fn R() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::R }
    #[classattr]
    fn S() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::S }
    #[classattr]
    fn T() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::T }
    #[classattr]
    fn U() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::U }
    #[classattr]
    fn V() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::V }
    #[classattr]
    fn W() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::W }
    #[classattr]
    fn X() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::X }
    #[classattr]
    fn Y() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Y }
    #[classattr]
    fn Z() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Z }
    #[classattr]
    fn Escape() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Escape }
    #[classattr]
    fn F1() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F1 }
    #[classattr]
    fn F2() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F2 }
    #[classattr]
    fn F3() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F3 }
    #[classattr]
    fn F4() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F4 }
    #[classattr]
    fn F5() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F5 }
    #[classattr]
    fn F6() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F6 }
    #[classattr]
    fn F7() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F7 }
    #[classattr]
    fn F8() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F8 }
    #[classattr]
    fn F9() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F9 }
    #[classattr]
    fn F10() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F10 }
    #[classattr]
    fn F11() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F11 }
    #[classattr]
    fn F12() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F12 }
    #[classattr]
    fn F13() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F13 }
    #[classattr]
    fn F14() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F14 }
    #[classattr]
    fn F15() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F15 }
    #[classattr]
    fn F16() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F16 }
    #[classattr]
    fn F17() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F17 }
    #[classattr]
    fn F18() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F18 }
    #[classattr]
    fn F19() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F19 }
    #[classattr]
    fn F20() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F20 }
    #[classattr]
    fn F21() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F21 }
    #[classattr]
    fn F22() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F22 }
    #[classattr]
    fn F23() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F23 }
    #[classattr]
    fn F24() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::F24 }
    #[classattr]
    fn Snapshot() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Snapshot }
    #[classattr]
    fn Scroll() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Scroll }
    #[classattr]
    fn Pause() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Pause }
    #[classattr]
    fn Insert() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Insert }
    #[classattr]
    fn Home() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Home }
    #[classattr]
    fn Delete() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Delete }
    #[classattr]
    fn End() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::End }
    #[classattr]
    fn PageDown() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::PageDown }
    #[classattr]
    fn PageUp() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::PageUp }
    #[classattr]
    fn Left() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Left }
    #[classattr]
    fn Up() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Up }
    #[classattr]
    fn Right() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Right }
    #[classattr]
    fn Down() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Down }
    #[classattr]
    fn Back() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Back }
    #[classattr]
    fn Return() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Return }
    #[classattr]
    fn Space() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Space }
    #[classattr]
    fn Compose() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Compose }
    #[classattr]
    fn Caret() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Caret }
    #[classattr]
    fn Numlock() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numlock }
    #[classattr]
    fn Numpad0() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad0 }
    #[classattr]
    fn Numpad1() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad1 }
    #[classattr]
    fn Numpad2() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad2 }
    #[classattr]
    fn Numpad3() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad3 }
    #[classattr]
    fn Numpad4() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad4 }
    #[classattr]
    fn Numpad5() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad5 }
    #[classattr]
    fn Numpad6() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad6 }
    #[classattr]
    fn Numpad7() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad7 }
    #[classattr]
    fn Numpad8() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad8 }
    #[classattr]
    fn Numpad9() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Numpad9 }
    #[classattr]
    fn NumpadAdd() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadAdd }
    #[classattr]
    fn NumpadDivide() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadDivide }
    #[classattr]
    fn NumpadDecimal() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadDecimal }
    #[classattr]
    fn NumpadComma() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadComma }
    #[classattr]
    fn NumpadEnter() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadEnter }
    #[classattr]
    fn NumpadEquals() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadEquals }
    #[classattr]
    fn NumpadMultiply() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadMultiply }
    #[classattr]
    fn NumpadSubtract() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NumpadSubtract }
    #[classattr]
    fn AbntC1() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::AbntC1 }
    #[classattr]
    fn AbntC2() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::AbntC2 }
    #[classattr]
    fn Apostrophe() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Apostrophe }
    #[classattr]
    fn Apps() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Apps }
    #[classattr]
    fn Asterisk() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Asterisk }
    #[classattr]
    fn At() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::At }
    #[classattr]
    fn Ax() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Ax }
    #[classattr]
    fn Backslash() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Backslash }
    #[classattr]
    fn Calculator() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Calculator }
    #[classattr]
    fn Capital() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Capital }
    #[classattr]
    fn Colon() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Colon }
    #[classattr]
    fn Comma() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Comma }
    #[classattr]
    fn Convert() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Convert }
    #[classattr]
    fn Equals() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Equals }
    #[classattr]
    fn Grave() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Grave }
    #[classattr]
    fn Kana() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Kana }
    #[classattr]
    fn Kanji() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Kanji }
    #[classattr]
    fn LAlt() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::LAlt }
    #[classattr]
    fn LBracket() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::LBracket }
    #[classattr]
    fn LControl() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::LControl }
    #[classattr]
    fn LShift() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::LShift }
    #[classattr]
    fn LWin() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::LWin }
    #[classattr]
    fn Mail() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Mail }
    #[classattr]
    fn MediaSelect() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::MediaSelect }
    #[classattr]
    fn MediaStop() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::MediaStop }
    #[classattr]
    fn Minus() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Minus }
    #[classattr]
    fn Mute() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Mute }
    #[classattr]
    fn MyComputer() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::MyComputer }
    #[classattr]
    fn NavigateForward() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NavigateForward }
    #[classattr]
    fn NavigateBackward() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NavigateBackward }
    #[classattr]
    fn NextTrack() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NextTrack }
    #[classattr]
    fn NoConvert() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::NoConvert }
    #[classattr]
    fn OEM102() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::OEM102 }
    #[classattr]
    fn Period() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Period }
    #[classattr]
    fn PlayPause() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::PlayPause }
    #[classattr]
    fn Plus() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Plus }
    #[classattr]
    fn Power() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Power }
    #[classattr]
    fn PrevTrack() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::PrevTrack }
    #[classattr]
    fn RAlt() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::RAlt }
    #[classattr]
    fn RBracket() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::RBracket }
    #[classattr]
    fn RControl() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::RControl }
    #[classattr]
    fn RShift() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::RShift }
    #[classattr]
    fn RWin() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::RWin }
    #[classattr]
    fn Semicolon() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Semicolon }
    #[classattr]
    fn Slash() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Slash }
    #[classattr]
    fn Sleep() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Sleep }
    #[classattr]
    fn Stop() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Stop }
    #[classattr]
    fn Sysrq() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Sysrq }
    #[classattr]
    fn Tab() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Tab }
    #[classattr]
    fn Underline() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Underline }
    #[classattr]
    fn Unlabeled() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Unlabeled }
    #[classattr]
    fn VolumeDown() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::VolumeDown }
    #[classattr]
    fn VolumeUp() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::VolumeUp }
    #[classattr]
    fn Wake() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Wake }
    #[classattr]
    fn WebBack() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::WebBack }
    #[classattr]
    fn WebFavorites() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::WebFavorites }
    #[classattr]
    fn WebForward() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::WebForward }
    #[classattr]
    fn WebHome() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::WebHome }
    #[classattr]
    fn WebRefresh() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::WebRefresh }
    #[classattr]
    fn WebSearch() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::WebSearch }
    #[classattr]
    fn WebStop() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::WebStop }
    #[classattr]
    fn Yen() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Yen }
    #[classattr]
    fn Copy() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Copy }
    #[classattr]
    fn Paste() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Paste }
    #[classattr]
    fn Cut() -> AzVirtualKeyCodeEnumWrapper { AzVirtualKeyCodeEnumWrapper::Cut }
}

#[pymethods]
impl AzAcceleratorKeyEnumWrapper {
    #[classattr]
    fn Ctrl() -> AzAcceleratorKeyEnumWrapper { AzAcceleratorKeyEnumWrapper::Ctrl }
    #[classattr]
    fn Alt() -> AzAcceleratorKeyEnumWrapper { AzAcceleratorKeyEnumWrapper::Alt }
    #[classattr]
    fn Shift() -> AzAcceleratorKeyEnumWrapper { AzAcceleratorKeyEnumWrapper::Shift }
    #[staticmethod]
    fn Key(v: VirtualKeyCode) -> AzAcceleratorKeyEnumWrapper { AzAcceleratorKeyEnumWrapper::Key(v) }
}

#[pymethods]
impl AzMouseCursorTypeEnumWrapper {
    #[classattr]
    fn Default() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Default }
    #[classattr]
    fn Crosshair() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Crosshair }
    #[classattr]
    fn Hand() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Hand }
    #[classattr]
    fn Arrow() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Arrow }
    #[classattr]
    fn Move() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Move }
    #[classattr]
    fn Text() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Text }
    #[classattr]
    fn Wait() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Wait }
    #[classattr]
    fn Help() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Help }
    #[classattr]
    fn Progress() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Progress }
    #[classattr]
    fn NotAllowed() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NotAllowed }
    #[classattr]
    fn ContextMenu() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::ContextMenu }
    #[classattr]
    fn Cell() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Cell }
    #[classattr]
    fn VerticalText() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::VerticalText }
    #[classattr]
    fn Alias() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Alias }
    #[classattr]
    fn Copy() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Copy }
    #[classattr]
    fn NoDrop() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NoDrop }
    #[classattr]
    fn Grab() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Grab }
    #[classattr]
    fn Grabbing() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::Grabbing }
    #[classattr]
    fn AllScroll() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::AllScroll }
    #[classattr]
    fn ZoomIn() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::ZoomIn }
    #[classattr]
    fn ZoomOut() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::ZoomOut }
    #[classattr]
    fn EResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::EResize }
    #[classattr]
    fn NResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NResize }
    #[classattr]
    fn NeResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NeResize }
    #[classattr]
    fn NwResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NwResize }
    #[classattr]
    fn SResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::SResize }
    #[classattr]
    fn SeResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::SeResize }
    #[classattr]
    fn SwResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::SwResize }
    #[classattr]
    fn WResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::WResize }
    #[classattr]
    fn EwResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::EwResize }
    #[classattr]
    fn NsResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NsResize }
    #[classattr]
    fn NeswResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NeswResize }
    #[classattr]
    fn NwseResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::NwseResize }
    #[classattr]
    fn ColResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::ColResize }
    #[classattr]
    fn RowResize() -> AzMouseCursorTypeEnumWrapper { AzMouseCursorTypeEnumWrapper::RowResize }
}

#[pymethods]
impl AzCursorPositionEnumWrapper {
    #[classattr]
    fn OutOfWindow() -> AzCursorPositionEnumWrapper { AzCursorPositionEnumWrapper::OutOfWindow }
    #[classattr]
    fn Uninitialized() -> AzCursorPositionEnumWrapper { AzCursorPositionEnumWrapper::Uninitialized }
    #[staticmethod]
    fn InWindow(v: LogicalPosition) -> AzCursorPositionEnumWrapper { AzCursorPositionEnumWrapper::InWindow(v) }
}

#[pymethods]
impl AzRendererTypeEnumWrapper {
    #[classattr]
    fn Hardware() -> AzRendererTypeEnumWrapper { AzRendererTypeEnumWrapper::Hardware }
    #[classattr]
    fn Software() -> AzRendererTypeEnumWrapper { AzRendererTypeEnumWrapper::Software }
}

#[pymethods]
impl AzFullScreenModeEnumWrapper {
    #[classattr]
    fn SlowFullScreen() -> AzFullScreenModeEnumWrapper { AzFullScreenModeEnumWrapper::SlowFullScreen }
    #[classattr]
    fn FastFullScreen() -> AzFullScreenModeEnumWrapper { AzFullScreenModeEnumWrapper::FastFullScreen }
    #[classattr]
    fn SlowWindowed() -> AzFullScreenModeEnumWrapper { AzFullScreenModeEnumWrapper::SlowWindowed }
    #[classattr]
    fn FastWindowed() -> AzFullScreenModeEnumWrapper { AzFullScreenModeEnumWrapper::FastWindowed }
}

#[pymethods]
impl AzWindowThemeEnumWrapper {
    #[classattr]
    fn DarkMode() -> AzWindowThemeEnumWrapper { AzWindowThemeEnumWrapper::DarkMode }
    #[classattr]
    fn LightMode() -> AzWindowThemeEnumWrapper { AzWindowThemeEnumWrapper::LightMode }
}

#[pymethods]
impl AzWindowPositionEnumWrapper {
    #[classattr]
    fn Uninitialized() -> AzWindowPositionEnumWrapper { AzWindowPositionEnumWrapper::Uninitialized }
    #[staticmethod]
    fn Initialized(v: PhysicalPositionI32) -> AzWindowPositionEnumWrapper { AzWindowPositionEnumWrapper::Initialized(v) }
}

#[pymethods]
impl AzImePositionEnumWrapper {
    #[classattr]
    fn Uninitialized() -> AzImePositionEnumWrapper { AzImePositionEnumWrapper::Uninitialized }
    #[staticmethod]
    fn Initialized(v: LogicalPosition) -> AzImePositionEnumWrapper { AzImePositionEnumWrapper::Initialized(v) }
}

#[pymethods]
impl AzWindowState {
    #[staticmethod]
    fn default() -> AzWindowState {
        unsafe { mem::transmute(crate::AzWindowState_default()) }
    }
}

#[pymethods]
impl AzCallbackInfo {
    fn get_hit_node(&self) -> AzDomNodeId {
        ()
    }
    fn get_system_time_fn(&self) -> AzGetSystemTimeFn {
        ()
    }
    fn get_cursor_relative_to_viewport(&self) -> Option<AzLogicalPosition> {
        ()
    }
    fn get_cursor_relative_to_node(&self) -> Option<AzLogicalPosition> {
        ()
    }
    fn get_current_window_state(&self) -> AzWindowState {
        ()
    }
    fn get_current_keyboard_state(&self) -> AzKeyboardState {
        ()
    }
    fn get_current_mouse_state(&self) -> AzMouseState {
        ()
    }
    fn get_previous_window_state(&self) -> Option<AzWindowState> {
        ()
    }
    fn get_previous_keyboard_state(&self) -> Option<AzKeyboardState> {
        ()
    }
    fn get_previous_mouse_state(&self) -> Option<AzMouseState> {
        ()
    }
    fn get_current_window_handle(&self) -> AzRawWindowHandle {
        ()
    }
    fn get_gl_context(&self) -> Option<AzGl> {
        ()
    }
    fn get_scroll_position(&self, node_id: AzDomNodeId) -> Option<AzLogicalPosition> {
        ()
    }
    fn get_dataset(&mut self, node_id: AzDomNodeId) -> Option<AzRefAny> {
        ()
    }
    fn get_string_contents(&self, node_id: AzDomNodeId) -> Option<AzString> {
        ()
    }
    fn get_inline_text(&self, node_id: AzDomNodeId) -> Option<AzInlineText> {
        ()
    }
    fn get_index_in_parent(&mut self, node_id: AzDomNodeId) -> usize {
        ()
    }
    fn get_parent(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_previous_sibling(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_next_sibling(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_first_child(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_last_child(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_node_position(&mut self, node_id: AzDomNodeId) -> Option<AzPositionInfo> {
        ()
    }
    fn get_node_size(&mut self, node_id: AzDomNodeId) -> Option<AzLogicalSize> {
        ()
    }
    fn get_computed_css_property(&mut self, node_id: AzDomNodeId, property_type: AzCssPropertyTypeEnumWrapper) -> Option<AzCssProperty> {
        ()
    }
    fn set_window_state(&mut self, new_state: AzWindowState) -> () {
        ()
    }
    fn set_focus(&mut self, target: AzFocusTargetEnumWrapper) -> () {
        ()
    }
    fn set_css_property(&mut self, node_id: AzDomNodeId, new_property: AzCssPropertyEnumWrapper) -> () {
        ()
    }
    fn set_scroll_position(&mut self, node_id: AzDomNodeId, scroll_position: AzLogicalPosition) -> () {
        ()
    }
    fn set_string_contents(&mut self, node_id: AzDomNodeId, string: String) -> () {
        ()
    }
    fn add_image(&mut self, id: String, image: AzImageRef) -> () {
        ()
    }
    fn has_image(&self, id: String) -> bool {
        ()
    }
    fn get_image(&self, id: String) -> Option<AzImageRef> {
        ()
    }
    fn update_image(&mut self, node_id: AzDomNodeId, new_image: AzImageRef) -> () {
        ()
    }
    fn delete_image(&mut self, id: String) -> () {
        ()
    }
    fn update_image_mask(&mut self, node_id: AzDomNodeId, new_mask: AzImageMask) -> () {
        ()
    }
    fn stop_propagation(&mut self) -> () {
        ()
    }
    fn create_window(&mut self, new_window: AzWindowCreateOptions) -> () {
        ()
    }
    fn start_timer(&mut self, timer: AzTimer) -> Option<AzTimerId> {
        ()
    }
    fn start_animation(&mut self, node: AzDomNodeId, animation: AzAnimation) -> Option<AzTimerId> {
        ()
    }
    fn stop_timer(&mut self, timer_id: AzTimerId) -> bool {
        ()
    }
    fn start_thread(&mut self, thread_initialize_data: AzRefAny, writeback_data: AzRefAny, callback: AzThreadCallback) -> Option<AzThreadId> {
        ()
    }
    fn send_thread_msg(&mut self, thread_id: AzThreadId, msg: AzThreadSendMsgEnumWrapper) -> bool {
        ()
    }
    fn stop_thread(&mut self, thread_id: AzThreadId) -> bool {
        ()
    }
}

#[pymethods]
impl AzUpdateScreenEnumWrapper {
    #[classattr]
    fn DoNothing() -> AzUpdateScreenEnumWrapper { AzUpdateScreenEnumWrapper::DoNothing }
    #[classattr]
    fn RegenerateStyledDomForCurrentWindow() -> AzUpdateScreenEnumWrapper { AzUpdateScreenEnumWrapper::RegenerateStyledDomForCurrentWindow }
    #[classattr]
    fn RegenerateStyledDomForAllWindows() -> AzUpdateScreenEnumWrapper { AzUpdateScreenEnumWrapper::RegenerateStyledDomForAllWindows }
}

#[pymethods]
impl AzPositionInfoEnumWrapper {
    #[staticmethod]
    fn Static(v: PositionInfoInner) -> AzPositionInfoEnumWrapper { AzPositionInfoEnumWrapper::Static(v) }
    #[staticmethod]
    fn Fixed(v: PositionInfoInner) -> AzPositionInfoEnumWrapper { AzPositionInfoEnumWrapper::Fixed(v) }
    #[staticmethod]
    fn Absolute(v: PositionInfoInner) -> AzPositionInfoEnumWrapper { AzPositionInfoEnumWrapper::Absolute(v) }
    #[staticmethod]
    fn Relative(v: PositionInfoInner) -> AzPositionInfoEnumWrapper { AzPositionInfoEnumWrapper::Relative(v) }
}

#[pymethods]
impl AzHidpiAdjustedBounds {
    fn get_logical_size(&self) -> AzLogicalSize {
        ()
    }
    fn get_physical_size(&self) -> AzPhysicalSizeU32 {
        ()
    }
    fn get_hidpi_factor(&self) -> f32 {
        ()
    }
}

#[pymethods]
impl AzInlineText {
    fn hit_test(&self, position: AzLogicalPosition) -> AzInlineTextHitVec {
        ()
    }
}

#[pymethods]
impl AzInlineWordEnumWrapper {
    #[classattr]
    fn Tab() -> AzInlineWordEnumWrapper { AzInlineWordEnumWrapper::Tab }
    #[classattr]
    fn Return() -> AzInlineWordEnumWrapper { AzInlineWordEnumWrapper::Return }
    #[classattr]
    fn Space() -> AzInlineWordEnumWrapper { AzInlineWordEnumWrapper::Space }
    #[staticmethod]
    fn Word(v: InlineTextContents) -> AzInlineWordEnumWrapper { AzInlineWordEnumWrapper::Word(v) }
}

#[pymethods]
impl AzFocusTargetEnumWrapper {
    #[staticmethod]
    fn Id(v: DomNodeId) -> AzFocusTargetEnumWrapper { AzFocusTargetEnumWrapper::Id(v) }
    #[staticmethod]
    fn Path(v: FocusTargetPath) -> AzFocusTargetEnumWrapper { AzFocusTargetEnumWrapper::Path(v) }
    #[classattr]
    fn Previous() -> AzFocusTargetEnumWrapper { AzFocusTargetEnumWrapper::Previous }
    #[classattr]
    fn Next() -> AzFocusTargetEnumWrapper { AzFocusTargetEnumWrapper::Next }
    #[classattr]
    fn First() -> AzFocusTargetEnumWrapper { AzFocusTargetEnumWrapper::First }
    #[classattr]
    fn Last() -> AzFocusTargetEnumWrapper { AzFocusTargetEnumWrapper::Last }
    #[classattr]
    fn NoFocus() -> AzFocusTargetEnumWrapper { AzFocusTargetEnumWrapper::NoFocus }
}

#[pymethods]
impl AzAnimationRepeatEnumWrapper {
    #[classattr]
    fn NoRepeat() -> AzAnimationRepeatEnumWrapper { AzAnimationRepeatEnumWrapper::NoRepeat }
    #[classattr]
    fn Loop() -> AzAnimationRepeatEnumWrapper { AzAnimationRepeatEnumWrapper::Loop }
    #[classattr]
    fn PingPong() -> AzAnimationRepeatEnumWrapper { AzAnimationRepeatEnumWrapper::PingPong }
}

#[pymethods]
impl AzAnimationRepeatCountEnumWrapper {
    #[staticmethod]
    fn Times(v: usize) -> AzAnimationRepeatCountEnumWrapper { AzAnimationRepeatCountEnumWrapper::Times(v) }
    #[classattr]
    fn Infinite() -> AzAnimationRepeatCountEnumWrapper { AzAnimationRepeatCountEnumWrapper::Infinite }
}

#[pymethods]
impl AzAnimationEasingEnumWrapper {
    #[classattr]
    fn Ease() -> AzAnimationEasingEnumWrapper { AzAnimationEasingEnumWrapper::Ease }
    #[classattr]
    fn Linear() -> AzAnimationEasingEnumWrapper { AzAnimationEasingEnumWrapper::Linear }
    #[classattr]
    fn EaseIn() -> AzAnimationEasingEnumWrapper { AzAnimationEasingEnumWrapper::EaseIn }
    #[classattr]
    fn EaseOut() -> AzAnimationEasingEnumWrapper { AzAnimationEasingEnumWrapper::EaseOut }
    #[classattr]
    fn EaseInOut() -> AzAnimationEasingEnumWrapper { AzAnimationEasingEnumWrapper::EaseInOut }
    #[staticmethod]
    fn CubicBezier(v: SvgCubicCurve) -> AzAnimationEasingEnumWrapper { AzAnimationEasingEnumWrapper::CubicBezier(v) }
}

#[pymethods]
impl AzRenderImageCallbackInfo {
    fn get_gl_context(&self) -> Option<AzGl> {
        ()
    }
    fn get_bounds(&self) -> AzHidpiAdjustedBounds {
        ()
    }
    fn get_callback_node_id(&self) -> AzDomNodeId {
        ()
    }
    fn get_inline_text(&self, node_id: AzDomNodeId) -> Option<AzInlineText> {
        ()
    }
    fn get_index_in_parent(&mut self, node_id: AzDomNodeId) -> usize {
        ()
    }
    fn get_parent(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_previous_sibling(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_next_sibling(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_first_child(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
    fn get_last_child(&mut self, node_id: AzDomNodeId) -> Option<AzDomNodeId> {
        ()
    }
}

#[pymethods]
impl AzRefCount {
    fn can_be_shared(&self) -> bool {
        ()
    }
    fn can_be_shared_mut(&self) -> bool {
        ()
    }
    fn increase_ref(&mut self) -> () {
        ()
    }
    fn decrease_ref(&mut self) -> () {
        ()
    }
    fn increase_refmut(&mut self) -> () {
        ()
    }
    fn decrease_refmut(&mut self) -> () {
        ()
    }
}

#[pymethods]
impl AzRefAny {
    fn get_type_id(&self) -> u64 {
        ()
    }
    fn get_type_name(&self) -> String {
        ()
    }
}

#[pymethods]
impl AzLayoutCallbackInfo {
    fn get_gl_context(&self) -> Option<AzGl> {
        ()
    }
    fn get_system_fonts(&self) -> AzStringPairVec {
        ()
    }
    fn get_image(&self, id: String) -> Option<AzImageRef> {
        ()
    }
}

#[pymethods]
impl AzDom {
    fn node_count(&self) -> usize {
        ()
    }
    fn style(self, css: AzCss) -> AzStyledDom {
        ()
    }
}

#[pymethods]
impl AzNodeTypeEnumWrapper {
    #[classattr]
    fn Body() -> AzNodeTypeEnumWrapper { AzNodeTypeEnumWrapper::Body }
    #[classattr]
    fn Div() -> AzNodeTypeEnumWrapper { AzNodeTypeEnumWrapper::Div }
    #[classattr]
    fn Br() -> AzNodeTypeEnumWrapper { AzNodeTypeEnumWrapper::Br }
    #[staticmethod]
    fn Text(v: String) -> AzNodeTypeEnumWrapper { AzNodeTypeEnumWrapper::Text(v) }
    #[staticmethod]
    fn Image(v: ImageRef) -> AzNodeTypeEnumWrapper { AzNodeTypeEnumWrapper::Image(v) }
    #[staticmethod]
    fn IFrame(v: IFrameNode) -> AzNodeTypeEnumWrapper { AzNodeTypeEnumWrapper::IFrame(v) }
}

#[pymethods]
impl AzOnEnumWrapper {
    #[classattr]
    fn MouseOver() -> AzOnEnumWrapper { AzOnEnumWrapper::MouseOver }
    #[classattr]
    fn MouseDown() -> AzOnEnumWrapper { AzOnEnumWrapper::MouseDown }
    #[classattr]
    fn LeftMouseDown() -> AzOnEnumWrapper { AzOnEnumWrapper::LeftMouseDown }
    #[classattr]
    fn MiddleMouseDown() -> AzOnEnumWrapper { AzOnEnumWrapper::MiddleMouseDown }
    #[classattr]
    fn RightMouseDown() -> AzOnEnumWrapper { AzOnEnumWrapper::RightMouseDown }
    #[classattr]
    fn MouseUp() -> AzOnEnumWrapper { AzOnEnumWrapper::MouseUp }
    #[classattr]
    fn LeftMouseUp() -> AzOnEnumWrapper { AzOnEnumWrapper::LeftMouseUp }
    #[classattr]
    fn MiddleMouseUp() -> AzOnEnumWrapper { AzOnEnumWrapper::MiddleMouseUp }
    #[classattr]
    fn RightMouseUp() -> AzOnEnumWrapper { AzOnEnumWrapper::RightMouseUp }
    #[classattr]
    fn MouseEnter() -> AzOnEnumWrapper { AzOnEnumWrapper::MouseEnter }
    #[classattr]
    fn MouseLeave() -> AzOnEnumWrapper { AzOnEnumWrapper::MouseLeave }
    #[classattr]
    fn Scroll() -> AzOnEnumWrapper { AzOnEnumWrapper::Scroll }
    #[classattr]
    fn TextInput() -> AzOnEnumWrapper { AzOnEnumWrapper::TextInput }
    #[classattr]
    fn VirtualKeyDown() -> AzOnEnumWrapper { AzOnEnumWrapper::VirtualKeyDown }
    #[classattr]
    fn VirtualKeyUp() -> AzOnEnumWrapper { AzOnEnumWrapper::VirtualKeyUp }
    #[classattr]
    fn HoveredFile() -> AzOnEnumWrapper { AzOnEnumWrapper::HoveredFile }
    #[classattr]
    fn DroppedFile() -> AzOnEnumWrapper { AzOnEnumWrapper::DroppedFile }
    #[classattr]
    fn HoveredFileCancelled() -> AzOnEnumWrapper { AzOnEnumWrapper::HoveredFileCancelled }
    #[classattr]
    fn FocusReceived() -> AzOnEnumWrapper { AzOnEnumWrapper::FocusReceived }
    #[classattr]
    fn FocusLost() -> AzOnEnumWrapper { AzOnEnumWrapper::FocusLost }
}

#[pymethods]
impl AzEventFilterEnumWrapper {
    #[staticmethod]
    fn Hover(v: HoverEventFilter) -> AzEventFilterEnumWrapper { AzEventFilterEnumWrapper::Hover(v) }
    #[staticmethod]
    fn Not(v: NotEventFilter) -> AzEventFilterEnumWrapper { AzEventFilterEnumWrapper::Not(v) }
    #[staticmethod]
    fn Focus(v: FocusEventFilter) -> AzEventFilterEnumWrapper { AzEventFilterEnumWrapper::Focus(v) }
    #[staticmethod]
    fn Window(v: WindowEventFilter) -> AzEventFilterEnumWrapper { AzEventFilterEnumWrapper::Window(v) }
    #[staticmethod]
    fn Component(v: ComponentEventFilter) -> AzEventFilterEnumWrapper { AzEventFilterEnumWrapper::Component(v) }
    #[staticmethod]
    fn Application(v: ApplicationEventFilter) -> AzEventFilterEnumWrapper { AzEventFilterEnumWrapper::Application(v) }
}

#[pymethods]
impl AzHoverEventFilterEnumWrapper {
    #[classattr]
    fn MouseOver() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::MouseOver }
    #[classattr]
    fn MouseDown() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::MouseDown }
    #[classattr]
    fn LeftMouseDown() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::LeftMouseDown }
    #[classattr]
    fn RightMouseDown() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::RightMouseDown }
    #[classattr]
    fn MiddleMouseDown() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::MiddleMouseDown }
    #[classattr]
    fn MouseUp() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::MouseUp }
    #[classattr]
    fn LeftMouseUp() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::LeftMouseUp }
    #[classattr]
    fn RightMouseUp() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::RightMouseUp }
    #[classattr]
    fn MiddleMouseUp() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::MiddleMouseUp }
    #[classattr]
    fn MouseEnter() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::MouseEnter }
    #[classattr]
    fn MouseLeave() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::MouseLeave }
    #[classattr]
    fn Scroll() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::Scroll }
    #[classattr]
    fn ScrollStart() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::ScrollStart }
    #[classattr]
    fn ScrollEnd() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::ScrollEnd }
    #[classattr]
    fn TextInput() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::TextInput }
    #[classattr]
    fn VirtualKeyDown() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::VirtualKeyDown }
    #[classattr]
    fn VirtualKeyUp() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::VirtualKeyUp }
    #[classattr]
    fn HoveredFile() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::HoveredFile }
    #[classattr]
    fn DroppedFile() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::DroppedFile }
    #[classattr]
    fn HoveredFileCancelled() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::HoveredFileCancelled }
    #[classattr]
    fn TouchStart() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::TouchStart }
    #[classattr]
    fn TouchMove() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::TouchMove }
    #[classattr]
    fn TouchEnd() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::TouchEnd }
    #[classattr]
    fn TouchCancel() -> AzHoverEventFilterEnumWrapper { AzHoverEventFilterEnumWrapper::TouchCancel }
}

#[pymethods]
impl AzFocusEventFilterEnumWrapper {
    #[classattr]
    fn MouseOver() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::MouseOver }
    #[classattr]
    fn MouseDown() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::MouseDown }
    #[classattr]
    fn LeftMouseDown() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::LeftMouseDown }
    #[classattr]
    fn RightMouseDown() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::RightMouseDown }
    #[classattr]
    fn MiddleMouseDown() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::MiddleMouseDown }
    #[classattr]
    fn MouseUp() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::MouseUp }
    #[classattr]
    fn LeftMouseUp() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::LeftMouseUp }
    #[classattr]
    fn RightMouseUp() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::RightMouseUp }
    #[classattr]
    fn MiddleMouseUp() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::MiddleMouseUp }
    #[classattr]
    fn MouseEnter() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::MouseEnter }
    #[classattr]
    fn MouseLeave() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::MouseLeave }
    #[classattr]
    fn Scroll() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::Scroll }
    #[classattr]
    fn ScrollStart() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::ScrollStart }
    #[classattr]
    fn ScrollEnd() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::ScrollEnd }
    #[classattr]
    fn TextInput() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::TextInput }
    #[classattr]
    fn VirtualKeyDown() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::VirtualKeyDown }
    #[classattr]
    fn VirtualKeyUp() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::VirtualKeyUp }
    #[classattr]
    fn FocusReceived() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::FocusReceived }
    #[classattr]
    fn FocusLost() -> AzFocusEventFilterEnumWrapper { AzFocusEventFilterEnumWrapper::FocusLost }
}

#[pymethods]
impl AzNotEventFilterEnumWrapper {
    #[staticmethod]
    fn Hover(v: HoverEventFilter) -> AzNotEventFilterEnumWrapper { AzNotEventFilterEnumWrapper::Hover(v) }
    #[staticmethod]
    fn Focus(v: FocusEventFilter) -> AzNotEventFilterEnumWrapper { AzNotEventFilterEnumWrapper::Focus(v) }
}

#[pymethods]
impl AzWindowEventFilterEnumWrapper {
    #[classattr]
    fn MouseOver() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::MouseOver }
    #[classattr]
    fn MouseDown() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::MouseDown }
    #[classattr]
    fn LeftMouseDown() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::LeftMouseDown }
    #[classattr]
    fn RightMouseDown() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::RightMouseDown }
    #[classattr]
    fn MiddleMouseDown() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::MiddleMouseDown }
    #[classattr]
    fn MouseUp() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::MouseUp }
    #[classattr]
    fn LeftMouseUp() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::LeftMouseUp }
    #[classattr]
    fn RightMouseUp() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::RightMouseUp }
    #[classattr]
    fn MiddleMouseUp() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::MiddleMouseUp }
    #[classattr]
    fn MouseEnter() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::MouseEnter }
    #[classattr]
    fn MouseLeave() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::MouseLeave }
    #[classattr]
    fn Scroll() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::Scroll }
    #[classattr]
    fn ScrollStart() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::ScrollStart }
    #[classattr]
    fn ScrollEnd() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::ScrollEnd }
    #[classattr]
    fn TextInput() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::TextInput }
    #[classattr]
    fn VirtualKeyDown() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::VirtualKeyDown }
    #[classattr]
    fn VirtualKeyUp() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::VirtualKeyUp }
    #[classattr]
    fn HoveredFile() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::HoveredFile }
    #[classattr]
    fn DroppedFile() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::DroppedFile }
    #[classattr]
    fn HoveredFileCancelled() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::HoveredFileCancelled }
    #[classattr]
    fn Resized() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::Resized }
    #[classattr]
    fn Moved() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::Moved }
    #[classattr]
    fn TouchStart() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::TouchStart }
    #[classattr]
    fn TouchMove() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::TouchMove }
    #[classattr]
    fn TouchEnd() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::TouchEnd }
    #[classattr]
    fn TouchCancel() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::TouchCancel }
    #[classattr]
    fn FocusReceived() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::FocusReceived }
    #[classattr]
    fn FocusLost() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::FocusLost }
    #[classattr]
    fn CloseRequested() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::CloseRequested }
    #[classattr]
    fn ThemeChanged() -> AzWindowEventFilterEnumWrapper { AzWindowEventFilterEnumWrapper::ThemeChanged }
}

#[pymethods]
impl AzComponentEventFilterEnumWrapper {
    #[classattr]
    fn AfterMount() -> AzComponentEventFilterEnumWrapper { AzComponentEventFilterEnumWrapper::AfterMount }
    #[classattr]
    fn BeforeUnmount() -> AzComponentEventFilterEnumWrapper { AzComponentEventFilterEnumWrapper::BeforeUnmount }
    #[classattr]
    fn NodeResized() -> AzComponentEventFilterEnumWrapper { AzComponentEventFilterEnumWrapper::NodeResized }
}

#[pymethods]
impl AzApplicationEventFilterEnumWrapper {
    #[classattr]
    fn DeviceConnected() -> AzApplicationEventFilterEnumWrapper { AzApplicationEventFilterEnumWrapper::DeviceConnected }
    #[classattr]
    fn DeviceDisconnected() -> AzApplicationEventFilterEnumWrapper { AzApplicationEventFilterEnumWrapper::DeviceDisconnected }
}

#[pymethods]
impl AzTabIndexEnumWrapper {
    #[classattr]
    fn Auto() -> AzTabIndexEnumWrapper { AzTabIndexEnumWrapper::Auto }
    #[staticmethod]
    fn OverrideInParent(v: u32) -> AzTabIndexEnumWrapper { AzTabIndexEnumWrapper::OverrideInParent(v) }
    #[classattr]
    fn NoKeyboardFocus() -> AzTabIndexEnumWrapper { AzTabIndexEnumWrapper::NoKeyboardFocus }
}

#[pymethods]
impl AzIdOrClassEnumWrapper {
    #[staticmethod]
    fn Id(v: String) -> AzIdOrClassEnumWrapper { AzIdOrClassEnumWrapper::Id(v) }
    #[staticmethod]
    fn Class(v: String) -> AzIdOrClassEnumWrapper { AzIdOrClassEnumWrapper::Class(v) }
}

#[pymethods]
impl AzNodeDataInlineCssPropertyEnumWrapper {
    #[staticmethod]
    fn Normal(v: CssProperty) -> AzNodeDataInlineCssPropertyEnumWrapper { AzNodeDataInlineCssPropertyEnumWrapper::Normal(v) }
    #[staticmethod]
    fn Active(v: CssProperty) -> AzNodeDataInlineCssPropertyEnumWrapper { AzNodeDataInlineCssPropertyEnumWrapper::Active(v) }
    #[staticmethod]
    fn Focus(v: CssProperty) -> AzNodeDataInlineCssPropertyEnumWrapper { AzNodeDataInlineCssPropertyEnumWrapper::Focus(v) }
    #[staticmethod]
    fn Hover(v: CssProperty) -> AzNodeDataInlineCssPropertyEnumWrapper { AzNodeDataInlineCssPropertyEnumWrapper::Hover(v) }
}

#[pymethods]
impl AzCssDeclarationEnumWrapper {
    #[staticmethod]
    fn Static(v: CssProperty) -> AzCssDeclarationEnumWrapper { AzCssDeclarationEnumWrapper::Static(v) }
    #[staticmethod]
    fn Dynamic(v: DynamicCssProperty) -> AzCssDeclarationEnumWrapper { AzCssDeclarationEnumWrapper::Dynamic(v) }
}

#[pymethods]
impl AzCssPathSelectorEnumWrapper {
    #[classattr]
    fn Global() -> AzCssPathSelectorEnumWrapper { AzCssPathSelectorEnumWrapper::Global }
    #[staticmethod]
    fn Type(v: NodeTypeKey) -> AzCssPathSelectorEnumWrapper { AzCssPathSelectorEnumWrapper::Type(v) }
    #[staticmethod]
    fn Class(v: String) -> AzCssPathSelectorEnumWrapper { AzCssPathSelectorEnumWrapper::Class(v) }
    #[staticmethod]
    fn Id(v: String) -> AzCssPathSelectorEnumWrapper { AzCssPathSelectorEnumWrapper::Id(v) }
    #[staticmethod]
    fn PseudoSelector(v: CssPathPseudoSelector) -> AzCssPathSelectorEnumWrapper { AzCssPathSelectorEnumWrapper::PseudoSelector(v) }
    #[classattr]
    fn DirectChildren() -> AzCssPathSelectorEnumWrapper { AzCssPathSelectorEnumWrapper::DirectChildren }
    #[classattr]
    fn Children() -> AzCssPathSelectorEnumWrapper { AzCssPathSelectorEnumWrapper::Children }
}

#[pymethods]
impl AzNodeTypeKeyEnumWrapper {
    #[classattr]
    fn Body() -> AzNodeTypeKeyEnumWrapper { AzNodeTypeKeyEnumWrapper::Body }
    #[classattr]
    fn Div() -> AzNodeTypeKeyEnumWrapper { AzNodeTypeKeyEnumWrapper::Div }
    #[classattr]
    fn Br() -> AzNodeTypeKeyEnumWrapper { AzNodeTypeKeyEnumWrapper::Br }
    #[classattr]
    fn P() -> AzNodeTypeKeyEnumWrapper { AzNodeTypeKeyEnumWrapper::P }
    #[classattr]
    fn Img() -> AzNodeTypeKeyEnumWrapper { AzNodeTypeKeyEnumWrapper::Img }
    #[classattr]
    fn IFrame() -> AzNodeTypeKeyEnumWrapper { AzNodeTypeKeyEnumWrapper::IFrame }
}

#[pymethods]
impl AzCssPathPseudoSelectorEnumWrapper {
    #[classattr]
    fn First() -> AzCssPathPseudoSelectorEnumWrapper { AzCssPathPseudoSelectorEnumWrapper::First }
    #[classattr]
    fn Last() -> AzCssPathPseudoSelectorEnumWrapper { AzCssPathPseudoSelectorEnumWrapper::Last }
    #[staticmethod]
    fn NthChild(v: CssNthChildSelector) -> AzCssPathPseudoSelectorEnumWrapper { AzCssPathPseudoSelectorEnumWrapper::NthChild(v) }
    #[classattr]
    fn Hover() -> AzCssPathPseudoSelectorEnumWrapper { AzCssPathPseudoSelectorEnumWrapper::Hover }
    #[classattr]
    fn Active() -> AzCssPathPseudoSelectorEnumWrapper { AzCssPathPseudoSelectorEnumWrapper::Active }
    #[classattr]
    fn Focus() -> AzCssPathPseudoSelectorEnumWrapper { AzCssPathPseudoSelectorEnumWrapper::Focus }
}

#[pymethods]
impl AzCssNthChildSelectorEnumWrapper {
    #[staticmethod]
    fn Number(v: u32) -> AzCssNthChildSelectorEnumWrapper { AzCssNthChildSelectorEnumWrapper::Number(v) }
    #[classattr]
    fn Even() -> AzCssNthChildSelectorEnumWrapper { AzCssNthChildSelectorEnumWrapper::Even }
    #[classattr]
    fn Odd() -> AzCssNthChildSelectorEnumWrapper { AzCssNthChildSelectorEnumWrapper::Odd }
    #[staticmethod]
    fn Pattern(v: CssNthChildPattern) -> AzCssNthChildSelectorEnumWrapper { AzCssNthChildSelectorEnumWrapper::Pattern(v) }
}

#[pymethods]
impl AzCss {
    #[staticmethod]
    fn empty() -> AzCss {
        unsafe { mem::transmute(crate::AzCss_empty()) }
    }
    #[staticmethod]
    fn from_string(s: String) -> AzCss {
        let s: AzString = s.into();
unsafe { mem::transmute(crate::AzCss_fromString(
            mem::transmute(s),
        )) }
    }
}

#[pymethods]
impl AzCssPropertyTypeEnumWrapper {
    #[classattr]
    fn TextColor() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::TextColor }
    #[classattr]
    fn FontSize() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::FontSize }
    #[classattr]
    fn FontFamily() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::FontFamily }
    #[classattr]
    fn TextAlign() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::TextAlign }
    #[classattr]
    fn LetterSpacing() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::LetterSpacing }
    #[classattr]
    fn LineHeight() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::LineHeight }
    #[classattr]
    fn WordSpacing() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::WordSpacing }
    #[classattr]
    fn TabWidth() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::TabWidth }
    #[classattr]
    fn Cursor() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Cursor }
    #[classattr]
    fn Display() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Display }
    #[classattr]
    fn Float() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Float }
    #[classattr]
    fn BoxSizing() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BoxSizing }
    #[classattr]
    fn Width() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Width }
    #[classattr]
    fn Height() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Height }
    #[classattr]
    fn MinWidth() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MinWidth }
    #[classattr]
    fn MinHeight() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MinHeight }
    #[classattr]
    fn MaxWidth() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MaxWidth }
    #[classattr]
    fn MaxHeight() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MaxHeight }
    #[classattr]
    fn Position() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Position }
    #[classattr]
    fn Top() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Top }
    #[classattr]
    fn Right() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Right }
    #[classattr]
    fn Left() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Left }
    #[classattr]
    fn Bottom() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Bottom }
    #[classattr]
    fn FlexWrap() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::FlexWrap }
    #[classattr]
    fn FlexDirection() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::FlexDirection }
    #[classattr]
    fn FlexGrow() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::FlexGrow }
    #[classattr]
    fn FlexShrink() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::FlexShrink }
    #[classattr]
    fn JustifyContent() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::JustifyContent }
    #[classattr]
    fn AlignItems() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::AlignItems }
    #[classattr]
    fn AlignContent() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::AlignContent }
    #[classattr]
    fn OverflowX() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::OverflowX }
    #[classattr]
    fn OverflowY() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::OverflowY }
    #[classattr]
    fn PaddingTop() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::PaddingTop }
    #[classattr]
    fn PaddingLeft() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::PaddingLeft }
    #[classattr]
    fn PaddingRight() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::PaddingRight }
    #[classattr]
    fn PaddingBottom() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::PaddingBottom }
    #[classattr]
    fn MarginTop() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MarginTop }
    #[classattr]
    fn MarginLeft() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MarginLeft }
    #[classattr]
    fn MarginRight() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MarginRight }
    #[classattr]
    fn MarginBottom() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::MarginBottom }
    #[classattr]
    fn Background() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Background }
    #[classattr]
    fn BackgroundImage() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BackgroundImage }
    #[classattr]
    fn BackgroundColor() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BackgroundColor }
    #[classattr]
    fn BackgroundPosition() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BackgroundPosition }
    #[classattr]
    fn BackgroundSize() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BackgroundSize }
    #[classattr]
    fn BackgroundRepeat() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BackgroundRepeat }
    #[classattr]
    fn BorderTopLeftRadius() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderTopLeftRadius }
    #[classattr]
    fn BorderTopRightRadius() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderTopRightRadius }
    #[classattr]
    fn BorderBottomLeftRadius() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderBottomLeftRadius }
    #[classattr]
    fn BorderBottomRightRadius() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderBottomRightRadius }
    #[classattr]
    fn BorderTopColor() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderTopColor }
    #[classattr]
    fn BorderRightColor() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderRightColor }
    #[classattr]
    fn BorderLeftColor() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderLeftColor }
    #[classattr]
    fn BorderBottomColor() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderBottomColor }
    #[classattr]
    fn BorderTopStyle() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderTopStyle }
    #[classattr]
    fn BorderRightStyle() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderRightStyle }
    #[classattr]
    fn BorderLeftStyle() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderLeftStyle }
    #[classattr]
    fn BorderBottomStyle() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderBottomStyle }
    #[classattr]
    fn BorderTopWidth() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderTopWidth }
    #[classattr]
    fn BorderRightWidth() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderRightWidth }
    #[classattr]
    fn BorderLeftWidth() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderLeftWidth }
    #[classattr]
    fn BorderBottomWidth() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BorderBottomWidth }
    #[classattr]
    fn BoxShadowLeft() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BoxShadowLeft }
    #[classattr]
    fn BoxShadowRight() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BoxShadowRight }
    #[classattr]
    fn BoxShadowTop() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BoxShadowTop }
    #[classattr]
    fn BoxShadowBottom() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BoxShadowBottom }
    #[classattr]
    fn ScrollbarStyle() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::ScrollbarStyle }
    #[classattr]
    fn Opacity() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Opacity }
    #[classattr]
    fn Transform() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::Transform }
    #[classattr]
    fn PerspectiveOrigin() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::PerspectiveOrigin }
    #[classattr]
    fn TransformOrigin() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::TransformOrigin }
    #[classattr]
    fn BackfaceVisibility() -> AzCssPropertyTypeEnumWrapper { AzCssPropertyTypeEnumWrapper::BackfaceVisibility }
}

#[pymethods]
impl AzAnimationInterpolationFunctionEnumWrapper {
    #[classattr]
    fn Ease() -> AzAnimationInterpolationFunctionEnumWrapper { AzAnimationInterpolationFunctionEnumWrapper::Ease }
    #[classattr]
    fn Linear() -> AzAnimationInterpolationFunctionEnumWrapper { AzAnimationInterpolationFunctionEnumWrapper::Linear }
    #[classattr]
    fn EaseIn() -> AzAnimationInterpolationFunctionEnumWrapper { AzAnimationInterpolationFunctionEnumWrapper::EaseIn }
    #[classattr]
    fn EaseOut() -> AzAnimationInterpolationFunctionEnumWrapper { AzAnimationInterpolationFunctionEnumWrapper::EaseOut }
    #[classattr]
    fn EaseInOut() -> AzAnimationInterpolationFunctionEnumWrapper { AzAnimationInterpolationFunctionEnumWrapper::EaseInOut }
    #[staticmethod]
    fn CubicBezier(v: SvgCubicCurve) -> AzAnimationInterpolationFunctionEnumWrapper { AzAnimationInterpolationFunctionEnumWrapper::CubicBezier(v) }
}

#[pymethods]
impl AzColorU {
    #[staticmethod]
    fn from_str(string: String) -> AzColorU {
        let string: AzString = string.into();
unsafe { mem::transmute(crate::AzColorU_fromStr(
            mem::transmute(string),
        )) }
    }
    fn to_hash(&self) -> String {
        ()
    }
}

#[pymethods]
impl AzSizeMetricEnumWrapper {
    #[classattr]
    fn Px() -> AzSizeMetricEnumWrapper { AzSizeMetricEnumWrapper::Px }
    #[classattr]
    fn Pt() -> AzSizeMetricEnumWrapper { AzSizeMetricEnumWrapper::Pt }
    #[classattr]
    fn Em() -> AzSizeMetricEnumWrapper { AzSizeMetricEnumWrapper::Em }
    #[classattr]
    fn Percent() -> AzSizeMetricEnumWrapper { AzSizeMetricEnumWrapper::Percent }
}

#[pymethods]
impl AzBoxShadowClipModeEnumWrapper {
    #[classattr]
    fn Outset() -> AzBoxShadowClipModeEnumWrapper { AzBoxShadowClipModeEnumWrapper::Outset }
    #[classattr]
    fn Inset() -> AzBoxShadowClipModeEnumWrapper { AzBoxShadowClipModeEnumWrapper::Inset }
}

#[pymethods]
impl AzLayoutAlignContentEnumWrapper {
    #[classattr]
    fn Stretch() -> AzLayoutAlignContentEnumWrapper { AzLayoutAlignContentEnumWrapper::Stretch }
    #[classattr]
    fn Center() -> AzLayoutAlignContentEnumWrapper { AzLayoutAlignContentEnumWrapper::Center }
    #[classattr]
    fn Start() -> AzLayoutAlignContentEnumWrapper { AzLayoutAlignContentEnumWrapper::Start }
    #[classattr]
    fn End() -> AzLayoutAlignContentEnumWrapper { AzLayoutAlignContentEnumWrapper::End }
    #[classattr]
    fn SpaceBetween() -> AzLayoutAlignContentEnumWrapper { AzLayoutAlignContentEnumWrapper::SpaceBetween }
    #[classattr]
    fn SpaceAround() -> AzLayoutAlignContentEnumWrapper { AzLayoutAlignContentEnumWrapper::SpaceAround }
}

#[pymethods]
impl AzLayoutAlignItemsEnumWrapper {
    #[classattr]
    fn Stretch() -> AzLayoutAlignItemsEnumWrapper { AzLayoutAlignItemsEnumWrapper::Stretch }
    #[classattr]
    fn Center() -> AzLayoutAlignItemsEnumWrapper { AzLayoutAlignItemsEnumWrapper::Center }
    #[classattr]
    fn FlexStart() -> AzLayoutAlignItemsEnumWrapper { AzLayoutAlignItemsEnumWrapper::FlexStart }
    #[classattr]
    fn FlexEnd() -> AzLayoutAlignItemsEnumWrapper { AzLayoutAlignItemsEnumWrapper::FlexEnd }
}

#[pymethods]
impl AzLayoutBoxSizingEnumWrapper {
    #[classattr]
    fn ContentBox() -> AzLayoutBoxSizingEnumWrapper { AzLayoutBoxSizingEnumWrapper::ContentBox }
    #[classattr]
    fn BorderBox() -> AzLayoutBoxSizingEnumWrapper { AzLayoutBoxSizingEnumWrapper::BorderBox }
}

#[pymethods]
impl AzLayoutFlexDirectionEnumWrapper {
    #[classattr]
    fn Row() -> AzLayoutFlexDirectionEnumWrapper { AzLayoutFlexDirectionEnumWrapper::Row }
    #[classattr]
    fn RowReverse() -> AzLayoutFlexDirectionEnumWrapper { AzLayoutFlexDirectionEnumWrapper::RowReverse }
    #[classattr]
    fn Column() -> AzLayoutFlexDirectionEnumWrapper { AzLayoutFlexDirectionEnumWrapper::Column }
    #[classattr]
    fn ColumnReverse() -> AzLayoutFlexDirectionEnumWrapper { AzLayoutFlexDirectionEnumWrapper::ColumnReverse }
}

#[pymethods]
impl AzLayoutDisplayEnumWrapper {
    #[classattr]
    fn None() -> AzLayoutDisplayEnumWrapper { AzLayoutDisplayEnumWrapper::None }
    #[classattr]
    fn Flex() -> AzLayoutDisplayEnumWrapper { AzLayoutDisplayEnumWrapper::Flex }
    #[classattr]
    fn Block() -> AzLayoutDisplayEnumWrapper { AzLayoutDisplayEnumWrapper::Block }
    #[classattr]
    fn InlineBlock() -> AzLayoutDisplayEnumWrapper { AzLayoutDisplayEnumWrapper::InlineBlock }
}

#[pymethods]
impl AzLayoutFloatEnumWrapper {
    #[classattr]
    fn Left() -> AzLayoutFloatEnumWrapper { AzLayoutFloatEnumWrapper::Left }
    #[classattr]
    fn Right() -> AzLayoutFloatEnumWrapper { AzLayoutFloatEnumWrapper::Right }
}

#[pymethods]
impl AzLayoutJustifyContentEnumWrapper {
    #[classattr]
    fn Start() -> AzLayoutJustifyContentEnumWrapper { AzLayoutJustifyContentEnumWrapper::Start }
    #[classattr]
    fn End() -> AzLayoutJustifyContentEnumWrapper { AzLayoutJustifyContentEnumWrapper::End }
    #[classattr]
    fn Center() -> AzLayoutJustifyContentEnumWrapper { AzLayoutJustifyContentEnumWrapper::Center }
    #[classattr]
    fn SpaceBetween() -> AzLayoutJustifyContentEnumWrapper { AzLayoutJustifyContentEnumWrapper::SpaceBetween }
    #[classattr]
    fn SpaceAround() -> AzLayoutJustifyContentEnumWrapper { AzLayoutJustifyContentEnumWrapper::SpaceAround }
    #[classattr]
    fn SpaceEvenly() -> AzLayoutJustifyContentEnumWrapper { AzLayoutJustifyContentEnumWrapper::SpaceEvenly }
}

#[pymethods]
impl AzLayoutPositionEnumWrapper {
    #[classattr]
    fn Static() -> AzLayoutPositionEnumWrapper { AzLayoutPositionEnumWrapper::Static }
    #[classattr]
    fn Relative() -> AzLayoutPositionEnumWrapper { AzLayoutPositionEnumWrapper::Relative }
    #[classattr]
    fn Absolute() -> AzLayoutPositionEnumWrapper { AzLayoutPositionEnumWrapper::Absolute }
    #[classattr]
    fn Fixed() -> AzLayoutPositionEnumWrapper { AzLayoutPositionEnumWrapper::Fixed }
}

#[pymethods]
impl AzLayoutFlexWrapEnumWrapper {
    #[classattr]
    fn Wrap() -> AzLayoutFlexWrapEnumWrapper { AzLayoutFlexWrapEnumWrapper::Wrap }
    #[classattr]
    fn NoWrap() -> AzLayoutFlexWrapEnumWrapper { AzLayoutFlexWrapEnumWrapper::NoWrap }
}

#[pymethods]
impl AzLayoutOverflowEnumWrapper {
    #[classattr]
    fn Scroll() -> AzLayoutOverflowEnumWrapper { AzLayoutOverflowEnumWrapper::Scroll }
    #[classattr]
    fn Auto() -> AzLayoutOverflowEnumWrapper { AzLayoutOverflowEnumWrapper::Auto }
    #[classattr]
    fn Hidden() -> AzLayoutOverflowEnumWrapper { AzLayoutOverflowEnumWrapper::Hidden }
    #[classattr]
    fn Visible() -> AzLayoutOverflowEnumWrapper { AzLayoutOverflowEnumWrapper::Visible }
}

#[pymethods]
impl AzAngleMetricEnumWrapper {
    #[classattr]
    fn Degree() -> AzAngleMetricEnumWrapper { AzAngleMetricEnumWrapper::Degree }
    #[classattr]
    fn Radians() -> AzAngleMetricEnumWrapper { AzAngleMetricEnumWrapper::Radians }
    #[classattr]
    fn Grad() -> AzAngleMetricEnumWrapper { AzAngleMetricEnumWrapper::Grad }
    #[classattr]
    fn Turn() -> AzAngleMetricEnumWrapper { AzAngleMetricEnumWrapper::Turn }
    #[classattr]
    fn Percent() -> AzAngleMetricEnumWrapper { AzAngleMetricEnumWrapper::Percent }
}

#[pymethods]
impl AzDirectionCornerEnumWrapper {
    #[classattr]
    fn Right() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::Right }
    #[classattr]
    fn Left() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::Left }
    #[classattr]
    fn Top() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::Top }
    #[classattr]
    fn Bottom() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::Bottom }
    #[classattr]
    fn TopRight() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::TopRight }
    #[classattr]
    fn TopLeft() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::TopLeft }
    #[classattr]
    fn BottomRight() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::BottomRight }
    #[classattr]
    fn BottomLeft() -> AzDirectionCornerEnumWrapper { AzDirectionCornerEnumWrapper::BottomLeft }
}

#[pymethods]
impl AzDirectionEnumWrapper {
    #[staticmethod]
    fn Angle(v: AngleValue) -> AzDirectionEnumWrapper { AzDirectionEnumWrapper::Angle(v) }
    #[staticmethod]
    fn FromTo(v: DirectionCorners) -> AzDirectionEnumWrapper { AzDirectionEnumWrapper::FromTo(v) }
}

#[pymethods]
impl AzExtendModeEnumWrapper {
    #[classattr]
    fn Clamp() -> AzExtendModeEnumWrapper { AzExtendModeEnumWrapper::Clamp }
    #[classattr]
    fn Repeat() -> AzExtendModeEnumWrapper { AzExtendModeEnumWrapper::Repeat }
}

#[pymethods]
impl AzShapeEnumWrapper {
    #[classattr]
    fn Ellipse() -> AzShapeEnumWrapper { AzShapeEnumWrapper::Ellipse }
    #[classattr]
    fn Circle() -> AzShapeEnumWrapper { AzShapeEnumWrapper::Circle }
}

#[pymethods]
impl AzRadialGradientSizeEnumWrapper {
    #[classattr]
    fn ClosestSide() -> AzRadialGradientSizeEnumWrapper { AzRadialGradientSizeEnumWrapper::ClosestSide }
    #[classattr]
    fn ClosestCorner() -> AzRadialGradientSizeEnumWrapper { AzRadialGradientSizeEnumWrapper::ClosestCorner }
    #[classattr]
    fn FarthestSide() -> AzRadialGradientSizeEnumWrapper { AzRadialGradientSizeEnumWrapper::FarthestSide }
    #[classattr]
    fn FarthestCorner() -> AzRadialGradientSizeEnumWrapper { AzRadialGradientSizeEnumWrapper::FarthestCorner }
}

#[pymethods]
impl AzStyleBackgroundContentEnumWrapper {
    #[staticmethod]
    fn LinearGradient(v: LinearGradient) -> AzStyleBackgroundContentEnumWrapper { AzStyleBackgroundContentEnumWrapper::LinearGradient(v) }
    #[staticmethod]
    fn RadialGradient(v: RadialGradient) -> AzStyleBackgroundContentEnumWrapper { AzStyleBackgroundContentEnumWrapper::RadialGradient(v) }
    #[staticmethod]
    fn ConicGradient(v: ConicGradient) -> AzStyleBackgroundContentEnumWrapper { AzStyleBackgroundContentEnumWrapper::ConicGradient(v) }
    #[staticmethod]
    fn Image(v: String) -> AzStyleBackgroundContentEnumWrapper { AzStyleBackgroundContentEnumWrapper::Image(v) }
    #[staticmethod]
    fn Color(v: ColorU) -> AzStyleBackgroundContentEnumWrapper { AzStyleBackgroundContentEnumWrapper::Color(v) }
}

#[pymethods]
impl AzBackgroundPositionHorizontalEnumWrapper {
    #[classattr]
    fn Left() -> AzBackgroundPositionHorizontalEnumWrapper { AzBackgroundPositionHorizontalEnumWrapper::Left }
    #[classattr]
    fn Center() -> AzBackgroundPositionHorizontalEnumWrapper { AzBackgroundPositionHorizontalEnumWrapper::Center }
    #[classattr]
    fn Right() -> AzBackgroundPositionHorizontalEnumWrapper { AzBackgroundPositionHorizontalEnumWrapper::Right }
    #[staticmethod]
    fn Exact(v: PixelValue) -> AzBackgroundPositionHorizontalEnumWrapper { AzBackgroundPositionHorizontalEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzBackgroundPositionVerticalEnumWrapper {
    #[classattr]
    fn Top() -> AzBackgroundPositionVerticalEnumWrapper { AzBackgroundPositionVerticalEnumWrapper::Top }
    #[classattr]
    fn Center() -> AzBackgroundPositionVerticalEnumWrapper { AzBackgroundPositionVerticalEnumWrapper::Center }
    #[classattr]
    fn Bottom() -> AzBackgroundPositionVerticalEnumWrapper { AzBackgroundPositionVerticalEnumWrapper::Bottom }
    #[staticmethod]
    fn Exact(v: PixelValue) -> AzBackgroundPositionVerticalEnumWrapper { AzBackgroundPositionVerticalEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBackgroundRepeatEnumWrapper {
    #[classattr]
    fn NoRepeat() -> AzStyleBackgroundRepeatEnumWrapper { AzStyleBackgroundRepeatEnumWrapper::NoRepeat }
    #[classattr]
    fn Repeat() -> AzStyleBackgroundRepeatEnumWrapper { AzStyleBackgroundRepeatEnumWrapper::Repeat }
    #[classattr]
    fn RepeatX() -> AzStyleBackgroundRepeatEnumWrapper { AzStyleBackgroundRepeatEnumWrapper::RepeatX }
    #[classattr]
    fn RepeatY() -> AzStyleBackgroundRepeatEnumWrapper { AzStyleBackgroundRepeatEnumWrapper::RepeatY }
}

#[pymethods]
impl AzStyleBackgroundSizeEnumWrapper {
    #[staticmethod]
    fn ExactSize(v: [PixelValue;2]) -> AzStyleBackgroundSizeEnumWrapper { AzStyleBackgroundSizeEnumWrapper::ExactSize(v) }
    #[classattr]
    fn Contain() -> AzStyleBackgroundSizeEnumWrapper { AzStyleBackgroundSizeEnumWrapper::Contain }
    #[classattr]
    fn Cover() -> AzStyleBackgroundSizeEnumWrapper { AzStyleBackgroundSizeEnumWrapper::Cover }
}

#[pymethods]
impl AzBorderStyleEnumWrapper {
    #[classattr]
    fn None() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::None }
    #[classattr]
    fn Solid() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Solid }
    #[classattr]
    fn Double() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Double }
    #[classattr]
    fn Dotted() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Dotted }
    #[classattr]
    fn Dashed() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Dashed }
    #[classattr]
    fn Hidden() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Hidden }
    #[classattr]
    fn Groove() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Groove }
    #[classattr]
    fn Ridge() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Ridge }
    #[classattr]
    fn Inset() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Inset }
    #[classattr]
    fn Outset() -> AzBorderStyleEnumWrapper { AzBorderStyleEnumWrapper::Outset }
}

#[pymethods]
impl AzStyleCursorEnumWrapper {
    #[classattr]
    fn Alias() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Alias }
    #[classattr]
    fn AllScroll() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::AllScroll }
    #[classattr]
    fn Cell() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Cell }
    #[classattr]
    fn ColResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::ColResize }
    #[classattr]
    fn ContextMenu() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::ContextMenu }
    #[classattr]
    fn Copy() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Copy }
    #[classattr]
    fn Crosshair() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Crosshair }
    #[classattr]
    fn Default() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Default }
    #[classattr]
    fn EResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::EResize }
    #[classattr]
    fn EwResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::EwResize }
    #[classattr]
    fn Grab() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Grab }
    #[classattr]
    fn Grabbing() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Grabbing }
    #[classattr]
    fn Help() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Help }
    #[classattr]
    fn Move() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Move }
    #[classattr]
    fn NResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::NResize }
    #[classattr]
    fn NsResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::NsResize }
    #[classattr]
    fn NeswResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::NeswResize }
    #[classattr]
    fn NwseResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::NwseResize }
    #[classattr]
    fn Pointer() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Pointer }
    #[classattr]
    fn Progress() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Progress }
    #[classattr]
    fn RowResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::RowResize }
    #[classattr]
    fn SResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::SResize }
    #[classattr]
    fn SeResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::SeResize }
    #[classattr]
    fn Text() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Text }
    #[classattr]
    fn Unset() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Unset }
    #[classattr]
    fn VerticalText() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::VerticalText }
    #[classattr]
    fn WResize() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::WResize }
    #[classattr]
    fn Wait() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::Wait }
    #[classattr]
    fn ZoomIn() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::ZoomIn }
    #[classattr]
    fn ZoomOut() -> AzStyleCursorEnumWrapper { AzStyleCursorEnumWrapper::ZoomOut }
}

#[pymethods]
impl AzStyleFontFamilyEnumWrapper {
    #[staticmethod]
    fn System(v: String) -> AzStyleFontFamilyEnumWrapper { AzStyleFontFamilyEnumWrapper::System(v) }
    #[staticmethod]
    fn File(v: String) -> AzStyleFontFamilyEnumWrapper { AzStyleFontFamilyEnumWrapper::File(v) }
    #[staticmethod]
    fn Ref(v: FontRef) -> AzStyleFontFamilyEnumWrapper { AzStyleFontFamilyEnumWrapper::Ref(v) }
}

#[pymethods]
impl AzStyleBackfaceVisibilityEnumWrapper {
    #[classattr]
    fn Hidden() -> AzStyleBackfaceVisibilityEnumWrapper { AzStyleBackfaceVisibilityEnumWrapper::Hidden }
    #[classattr]
    fn Visible() -> AzStyleBackfaceVisibilityEnumWrapper { AzStyleBackfaceVisibilityEnumWrapper::Visible }
}

#[pymethods]
impl AzStyleTransformEnumWrapper {
    #[staticmethod]
    fn Matrix(v: StyleTransformMatrix2D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Matrix(v) }
    #[staticmethod]
    fn Matrix3D(v: StyleTransformMatrix3D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Matrix3D(v) }
    #[staticmethod]
    fn Translate(v: StyleTransformTranslate2D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Translate(v) }
    #[staticmethod]
    fn Translate3D(v: StyleTransformTranslate3D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Translate3D(v) }
    #[staticmethod]
    fn TranslateX(v: PixelValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::TranslateX(v) }
    #[staticmethod]
    fn TranslateY(v: PixelValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::TranslateY(v) }
    #[staticmethod]
    fn TranslateZ(v: PixelValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::TranslateZ(v) }
    #[staticmethod]
    fn Rotate(v: AngleValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Rotate(v) }
    #[staticmethod]
    fn Rotate3D(v: StyleTransformRotate3D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Rotate3D(v) }
    #[staticmethod]
    fn RotateX(v: AngleValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::RotateX(v) }
    #[staticmethod]
    fn RotateY(v: AngleValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::RotateY(v) }
    #[staticmethod]
    fn RotateZ(v: AngleValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::RotateZ(v) }
    #[staticmethod]
    fn Scale(v: StyleTransformScale2D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Scale(v) }
    #[staticmethod]
    fn Scale3D(v: StyleTransformScale3D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Scale3D(v) }
    #[staticmethod]
    fn ScaleX(v: PercentageValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::ScaleX(v) }
    #[staticmethod]
    fn ScaleY(v: PercentageValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::ScaleY(v) }
    #[staticmethod]
    fn ScaleZ(v: PercentageValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::ScaleZ(v) }
    #[staticmethod]
    fn Skew(v: StyleTransformSkew2D) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Skew(v) }
    #[staticmethod]
    fn SkewX(v: PercentageValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::SkewX(v) }
    #[staticmethod]
    fn SkewY(v: PercentageValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::SkewY(v) }
    #[staticmethod]
    fn Perspective(v: PixelValue) -> AzStyleTransformEnumWrapper { AzStyleTransformEnumWrapper::Perspective(v) }
}

#[pymethods]
impl AzStyleTextAlignEnumWrapper {
    #[classattr]
    fn Left() -> AzStyleTextAlignEnumWrapper { AzStyleTextAlignEnumWrapper::Left }
    #[classattr]
    fn Center() -> AzStyleTextAlignEnumWrapper { AzStyleTextAlignEnumWrapper::Center }
    #[classattr]
    fn Right() -> AzStyleTextAlignEnumWrapper { AzStyleTextAlignEnumWrapper::Right }
}

#[pymethods]
impl AzStyleBoxShadowValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBoxShadowValueEnumWrapper { AzStyleBoxShadowValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBoxShadowValueEnumWrapper { AzStyleBoxShadowValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBoxShadowValueEnumWrapper { AzStyleBoxShadowValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBoxShadowValueEnumWrapper { AzStyleBoxShadowValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBoxShadow) -> AzStyleBoxShadowValueEnumWrapper { AzStyleBoxShadowValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutAlignContentValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutAlignContentValueEnumWrapper { AzLayoutAlignContentValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutAlignContentValueEnumWrapper { AzLayoutAlignContentValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutAlignContentValueEnumWrapper { AzLayoutAlignContentValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutAlignContentValueEnumWrapper { AzLayoutAlignContentValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutAlignContent) -> AzLayoutAlignContentValueEnumWrapper { AzLayoutAlignContentValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutAlignItemsValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutAlignItemsValueEnumWrapper { AzLayoutAlignItemsValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutAlignItemsValueEnumWrapper { AzLayoutAlignItemsValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutAlignItemsValueEnumWrapper { AzLayoutAlignItemsValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutAlignItemsValueEnumWrapper { AzLayoutAlignItemsValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutAlignItems) -> AzLayoutAlignItemsValueEnumWrapper { AzLayoutAlignItemsValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutBottomValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutBottomValueEnumWrapper { AzLayoutBottomValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutBottomValueEnumWrapper { AzLayoutBottomValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutBottomValueEnumWrapper { AzLayoutBottomValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutBottomValueEnumWrapper { AzLayoutBottomValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutBottom) -> AzLayoutBottomValueEnumWrapper { AzLayoutBottomValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutBoxSizingValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutBoxSizingValueEnumWrapper { AzLayoutBoxSizingValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutBoxSizingValueEnumWrapper { AzLayoutBoxSizingValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutBoxSizingValueEnumWrapper { AzLayoutBoxSizingValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutBoxSizingValueEnumWrapper { AzLayoutBoxSizingValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutBoxSizing) -> AzLayoutBoxSizingValueEnumWrapper { AzLayoutBoxSizingValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutFlexDirectionValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutFlexDirectionValueEnumWrapper { AzLayoutFlexDirectionValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutFlexDirectionValueEnumWrapper { AzLayoutFlexDirectionValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutFlexDirectionValueEnumWrapper { AzLayoutFlexDirectionValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutFlexDirectionValueEnumWrapper { AzLayoutFlexDirectionValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutFlexDirection) -> AzLayoutFlexDirectionValueEnumWrapper { AzLayoutFlexDirectionValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutDisplayValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutDisplayValueEnumWrapper { AzLayoutDisplayValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutDisplayValueEnumWrapper { AzLayoutDisplayValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutDisplayValueEnumWrapper { AzLayoutDisplayValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutDisplayValueEnumWrapper { AzLayoutDisplayValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutDisplay) -> AzLayoutDisplayValueEnumWrapper { AzLayoutDisplayValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutFlexGrowValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutFlexGrowValueEnumWrapper { AzLayoutFlexGrowValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutFlexGrowValueEnumWrapper { AzLayoutFlexGrowValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutFlexGrowValueEnumWrapper { AzLayoutFlexGrowValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutFlexGrowValueEnumWrapper { AzLayoutFlexGrowValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutFlexGrow) -> AzLayoutFlexGrowValueEnumWrapper { AzLayoutFlexGrowValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutFlexShrinkValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutFlexShrinkValueEnumWrapper { AzLayoutFlexShrinkValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutFlexShrinkValueEnumWrapper { AzLayoutFlexShrinkValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutFlexShrinkValueEnumWrapper { AzLayoutFlexShrinkValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutFlexShrinkValueEnumWrapper { AzLayoutFlexShrinkValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutFlexShrink) -> AzLayoutFlexShrinkValueEnumWrapper { AzLayoutFlexShrinkValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutFloatValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutFloatValueEnumWrapper { AzLayoutFloatValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutFloatValueEnumWrapper { AzLayoutFloatValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutFloatValueEnumWrapper { AzLayoutFloatValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutFloatValueEnumWrapper { AzLayoutFloatValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutFloat) -> AzLayoutFloatValueEnumWrapper { AzLayoutFloatValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutHeightValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutHeightValueEnumWrapper { AzLayoutHeightValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutHeightValueEnumWrapper { AzLayoutHeightValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutHeightValueEnumWrapper { AzLayoutHeightValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutHeightValueEnumWrapper { AzLayoutHeightValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutHeight) -> AzLayoutHeightValueEnumWrapper { AzLayoutHeightValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutJustifyContentValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutJustifyContentValueEnumWrapper { AzLayoutJustifyContentValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutJustifyContentValueEnumWrapper { AzLayoutJustifyContentValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutJustifyContentValueEnumWrapper { AzLayoutJustifyContentValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutJustifyContentValueEnumWrapper { AzLayoutJustifyContentValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutJustifyContent) -> AzLayoutJustifyContentValueEnumWrapper { AzLayoutJustifyContentValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutLeftValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutLeftValueEnumWrapper { AzLayoutLeftValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutLeftValueEnumWrapper { AzLayoutLeftValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutLeftValueEnumWrapper { AzLayoutLeftValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutLeftValueEnumWrapper { AzLayoutLeftValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutLeft) -> AzLayoutLeftValueEnumWrapper { AzLayoutLeftValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMarginBottomValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMarginBottomValueEnumWrapper { AzLayoutMarginBottomValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMarginBottomValueEnumWrapper { AzLayoutMarginBottomValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMarginBottomValueEnumWrapper { AzLayoutMarginBottomValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMarginBottomValueEnumWrapper { AzLayoutMarginBottomValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMarginBottom) -> AzLayoutMarginBottomValueEnumWrapper { AzLayoutMarginBottomValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMarginLeftValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMarginLeftValueEnumWrapper { AzLayoutMarginLeftValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMarginLeftValueEnumWrapper { AzLayoutMarginLeftValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMarginLeftValueEnumWrapper { AzLayoutMarginLeftValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMarginLeftValueEnumWrapper { AzLayoutMarginLeftValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMarginLeft) -> AzLayoutMarginLeftValueEnumWrapper { AzLayoutMarginLeftValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMarginRightValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMarginRightValueEnumWrapper { AzLayoutMarginRightValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMarginRightValueEnumWrapper { AzLayoutMarginRightValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMarginRightValueEnumWrapper { AzLayoutMarginRightValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMarginRightValueEnumWrapper { AzLayoutMarginRightValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMarginRight) -> AzLayoutMarginRightValueEnumWrapper { AzLayoutMarginRightValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMarginTopValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMarginTopValueEnumWrapper { AzLayoutMarginTopValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMarginTopValueEnumWrapper { AzLayoutMarginTopValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMarginTopValueEnumWrapper { AzLayoutMarginTopValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMarginTopValueEnumWrapper { AzLayoutMarginTopValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMarginTop) -> AzLayoutMarginTopValueEnumWrapper { AzLayoutMarginTopValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMaxHeightValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMaxHeightValueEnumWrapper { AzLayoutMaxHeightValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMaxHeightValueEnumWrapper { AzLayoutMaxHeightValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMaxHeightValueEnumWrapper { AzLayoutMaxHeightValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMaxHeightValueEnumWrapper { AzLayoutMaxHeightValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMaxHeight) -> AzLayoutMaxHeightValueEnumWrapper { AzLayoutMaxHeightValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMaxWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMaxWidthValueEnumWrapper { AzLayoutMaxWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMaxWidthValueEnumWrapper { AzLayoutMaxWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMaxWidthValueEnumWrapper { AzLayoutMaxWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMaxWidthValueEnumWrapper { AzLayoutMaxWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMaxWidth) -> AzLayoutMaxWidthValueEnumWrapper { AzLayoutMaxWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMinHeightValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMinHeightValueEnumWrapper { AzLayoutMinHeightValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMinHeightValueEnumWrapper { AzLayoutMinHeightValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMinHeightValueEnumWrapper { AzLayoutMinHeightValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMinHeightValueEnumWrapper { AzLayoutMinHeightValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMinHeight) -> AzLayoutMinHeightValueEnumWrapper { AzLayoutMinHeightValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutMinWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutMinWidthValueEnumWrapper { AzLayoutMinWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutMinWidthValueEnumWrapper { AzLayoutMinWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutMinWidthValueEnumWrapper { AzLayoutMinWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutMinWidthValueEnumWrapper { AzLayoutMinWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutMinWidth) -> AzLayoutMinWidthValueEnumWrapper { AzLayoutMinWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutPaddingBottomValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutPaddingBottomValueEnumWrapper { AzLayoutPaddingBottomValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutPaddingBottomValueEnumWrapper { AzLayoutPaddingBottomValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutPaddingBottomValueEnumWrapper { AzLayoutPaddingBottomValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutPaddingBottomValueEnumWrapper { AzLayoutPaddingBottomValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutPaddingBottom) -> AzLayoutPaddingBottomValueEnumWrapper { AzLayoutPaddingBottomValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutPaddingLeftValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutPaddingLeftValueEnumWrapper { AzLayoutPaddingLeftValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutPaddingLeftValueEnumWrapper { AzLayoutPaddingLeftValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutPaddingLeftValueEnumWrapper { AzLayoutPaddingLeftValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutPaddingLeftValueEnumWrapper { AzLayoutPaddingLeftValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutPaddingLeft) -> AzLayoutPaddingLeftValueEnumWrapper { AzLayoutPaddingLeftValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutPaddingRightValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutPaddingRightValueEnumWrapper { AzLayoutPaddingRightValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutPaddingRightValueEnumWrapper { AzLayoutPaddingRightValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutPaddingRightValueEnumWrapper { AzLayoutPaddingRightValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutPaddingRightValueEnumWrapper { AzLayoutPaddingRightValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutPaddingRight) -> AzLayoutPaddingRightValueEnumWrapper { AzLayoutPaddingRightValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutPaddingTopValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutPaddingTopValueEnumWrapper { AzLayoutPaddingTopValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutPaddingTopValueEnumWrapper { AzLayoutPaddingTopValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutPaddingTopValueEnumWrapper { AzLayoutPaddingTopValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutPaddingTopValueEnumWrapper { AzLayoutPaddingTopValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutPaddingTop) -> AzLayoutPaddingTopValueEnumWrapper { AzLayoutPaddingTopValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutPositionValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutPositionValueEnumWrapper { AzLayoutPositionValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutPositionValueEnumWrapper { AzLayoutPositionValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutPositionValueEnumWrapper { AzLayoutPositionValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutPositionValueEnumWrapper { AzLayoutPositionValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutPosition) -> AzLayoutPositionValueEnumWrapper { AzLayoutPositionValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutRightValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutRightValueEnumWrapper { AzLayoutRightValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutRightValueEnumWrapper { AzLayoutRightValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutRightValueEnumWrapper { AzLayoutRightValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutRightValueEnumWrapper { AzLayoutRightValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutRight) -> AzLayoutRightValueEnumWrapper { AzLayoutRightValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutTopValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutTopValueEnumWrapper { AzLayoutTopValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutTopValueEnumWrapper { AzLayoutTopValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutTopValueEnumWrapper { AzLayoutTopValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutTopValueEnumWrapper { AzLayoutTopValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutTop) -> AzLayoutTopValueEnumWrapper { AzLayoutTopValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutWidthValueEnumWrapper { AzLayoutWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutWidthValueEnumWrapper { AzLayoutWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutWidthValueEnumWrapper { AzLayoutWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutWidthValueEnumWrapper { AzLayoutWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutWidth) -> AzLayoutWidthValueEnumWrapper { AzLayoutWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutFlexWrapValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutFlexWrapValueEnumWrapper { AzLayoutFlexWrapValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutFlexWrapValueEnumWrapper { AzLayoutFlexWrapValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutFlexWrapValueEnumWrapper { AzLayoutFlexWrapValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutFlexWrapValueEnumWrapper { AzLayoutFlexWrapValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutFlexWrap) -> AzLayoutFlexWrapValueEnumWrapper { AzLayoutFlexWrapValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutOverflowValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutOverflowValueEnumWrapper { AzLayoutOverflowValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutOverflowValueEnumWrapper { AzLayoutOverflowValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutOverflowValueEnumWrapper { AzLayoutOverflowValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutOverflowValueEnumWrapper { AzLayoutOverflowValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutOverflow) -> AzLayoutOverflowValueEnumWrapper { AzLayoutOverflowValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzScrollbarStyleValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzScrollbarStyleValueEnumWrapper { AzScrollbarStyleValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzScrollbarStyleValueEnumWrapper { AzScrollbarStyleValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzScrollbarStyleValueEnumWrapper { AzScrollbarStyleValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzScrollbarStyleValueEnumWrapper { AzScrollbarStyleValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: ScrollbarStyle) -> AzScrollbarStyleValueEnumWrapper { AzScrollbarStyleValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBackgroundContentVecValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBackgroundContentVecValueEnumWrapper { AzStyleBackgroundContentVecValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBackgroundContentVecValueEnumWrapper { AzStyleBackgroundContentVecValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBackgroundContentVecValueEnumWrapper { AzStyleBackgroundContentVecValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBackgroundContentVecValueEnumWrapper { AzStyleBackgroundContentVecValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBackgroundContentVec) -> AzStyleBackgroundContentVecValueEnumWrapper { AzStyleBackgroundContentVecValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBackgroundPositionVecValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBackgroundPositionVecValueEnumWrapper { AzStyleBackgroundPositionVecValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBackgroundPositionVecValueEnumWrapper { AzStyleBackgroundPositionVecValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBackgroundPositionVecValueEnumWrapper { AzStyleBackgroundPositionVecValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBackgroundPositionVecValueEnumWrapper { AzStyleBackgroundPositionVecValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBackgroundPositionVec) -> AzStyleBackgroundPositionVecValueEnumWrapper { AzStyleBackgroundPositionVecValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBackgroundRepeatVecValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBackgroundRepeatVecValueEnumWrapper { AzStyleBackgroundRepeatVecValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBackgroundRepeatVecValueEnumWrapper { AzStyleBackgroundRepeatVecValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBackgroundRepeatVecValueEnumWrapper { AzStyleBackgroundRepeatVecValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBackgroundRepeatVecValueEnumWrapper { AzStyleBackgroundRepeatVecValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBackgroundRepeatVec) -> AzStyleBackgroundRepeatVecValueEnumWrapper { AzStyleBackgroundRepeatVecValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBackgroundSizeVecValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBackgroundSizeVecValueEnumWrapper { AzStyleBackgroundSizeVecValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBackgroundSizeVecValueEnumWrapper { AzStyleBackgroundSizeVecValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBackgroundSizeVecValueEnumWrapper { AzStyleBackgroundSizeVecValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBackgroundSizeVecValueEnumWrapper { AzStyleBackgroundSizeVecValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBackgroundSizeVec) -> AzStyleBackgroundSizeVecValueEnumWrapper { AzStyleBackgroundSizeVecValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderBottomColorValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderBottomColorValueEnumWrapper { AzStyleBorderBottomColorValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderBottomColorValueEnumWrapper { AzStyleBorderBottomColorValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderBottomColorValueEnumWrapper { AzStyleBorderBottomColorValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderBottomColorValueEnumWrapper { AzStyleBorderBottomColorValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderBottomColor) -> AzStyleBorderBottomColorValueEnumWrapper { AzStyleBorderBottomColorValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderBottomLeftRadiusValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderBottomLeftRadiusValueEnumWrapper { AzStyleBorderBottomLeftRadiusValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderBottomLeftRadiusValueEnumWrapper { AzStyleBorderBottomLeftRadiusValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderBottomLeftRadiusValueEnumWrapper { AzStyleBorderBottomLeftRadiusValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderBottomLeftRadiusValueEnumWrapper { AzStyleBorderBottomLeftRadiusValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadiusValueEnumWrapper { AzStyleBorderBottomLeftRadiusValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderBottomRightRadiusValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderBottomRightRadiusValueEnumWrapper { AzStyleBorderBottomRightRadiusValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderBottomRightRadiusValueEnumWrapper { AzStyleBorderBottomRightRadiusValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderBottomRightRadiusValueEnumWrapper { AzStyleBorderBottomRightRadiusValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderBottomRightRadiusValueEnumWrapper { AzStyleBorderBottomRightRadiusValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadiusValueEnumWrapper { AzStyleBorderBottomRightRadiusValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderBottomStyleValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderBottomStyleValueEnumWrapper { AzStyleBorderBottomStyleValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderBottomStyleValueEnumWrapper { AzStyleBorderBottomStyleValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderBottomStyleValueEnumWrapper { AzStyleBorderBottomStyleValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderBottomStyleValueEnumWrapper { AzStyleBorderBottomStyleValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderBottomStyle) -> AzStyleBorderBottomStyleValueEnumWrapper { AzStyleBorderBottomStyleValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutBorderBottomWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutBorderBottomWidthValueEnumWrapper { AzLayoutBorderBottomWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutBorderBottomWidthValueEnumWrapper { AzLayoutBorderBottomWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutBorderBottomWidthValueEnumWrapper { AzLayoutBorderBottomWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutBorderBottomWidthValueEnumWrapper { AzLayoutBorderBottomWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutBorderBottomWidth) -> AzLayoutBorderBottomWidthValueEnumWrapper { AzLayoutBorderBottomWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderLeftColorValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderLeftColorValueEnumWrapper { AzStyleBorderLeftColorValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderLeftColorValueEnumWrapper { AzStyleBorderLeftColorValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderLeftColorValueEnumWrapper { AzStyleBorderLeftColorValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderLeftColorValueEnumWrapper { AzStyleBorderLeftColorValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderLeftColor) -> AzStyleBorderLeftColorValueEnumWrapper { AzStyleBorderLeftColorValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderLeftStyleValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderLeftStyleValueEnumWrapper { AzStyleBorderLeftStyleValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderLeftStyleValueEnumWrapper { AzStyleBorderLeftStyleValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderLeftStyleValueEnumWrapper { AzStyleBorderLeftStyleValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderLeftStyleValueEnumWrapper { AzStyleBorderLeftStyleValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderLeftStyle) -> AzStyleBorderLeftStyleValueEnumWrapper { AzStyleBorderLeftStyleValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutBorderLeftWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutBorderLeftWidthValueEnumWrapper { AzLayoutBorderLeftWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutBorderLeftWidthValueEnumWrapper { AzLayoutBorderLeftWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutBorderLeftWidthValueEnumWrapper { AzLayoutBorderLeftWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutBorderLeftWidthValueEnumWrapper { AzLayoutBorderLeftWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutBorderLeftWidth) -> AzLayoutBorderLeftWidthValueEnumWrapper { AzLayoutBorderLeftWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderRightColorValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderRightColorValueEnumWrapper { AzStyleBorderRightColorValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderRightColorValueEnumWrapper { AzStyleBorderRightColorValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderRightColorValueEnumWrapper { AzStyleBorderRightColorValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderRightColorValueEnumWrapper { AzStyleBorderRightColorValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderRightColor) -> AzStyleBorderRightColorValueEnumWrapper { AzStyleBorderRightColorValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderRightStyleValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderRightStyleValueEnumWrapper { AzStyleBorderRightStyleValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderRightStyleValueEnumWrapper { AzStyleBorderRightStyleValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderRightStyleValueEnumWrapper { AzStyleBorderRightStyleValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderRightStyleValueEnumWrapper { AzStyleBorderRightStyleValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderRightStyle) -> AzStyleBorderRightStyleValueEnumWrapper { AzStyleBorderRightStyleValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutBorderRightWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutBorderRightWidthValueEnumWrapper { AzLayoutBorderRightWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutBorderRightWidthValueEnumWrapper { AzLayoutBorderRightWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutBorderRightWidthValueEnumWrapper { AzLayoutBorderRightWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutBorderRightWidthValueEnumWrapper { AzLayoutBorderRightWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutBorderRightWidth) -> AzLayoutBorderRightWidthValueEnumWrapper { AzLayoutBorderRightWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderTopColorValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderTopColorValueEnumWrapper { AzStyleBorderTopColorValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderTopColorValueEnumWrapper { AzStyleBorderTopColorValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderTopColorValueEnumWrapper { AzStyleBorderTopColorValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderTopColorValueEnumWrapper { AzStyleBorderTopColorValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderTopColor) -> AzStyleBorderTopColorValueEnumWrapper { AzStyleBorderTopColorValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderTopLeftRadiusValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderTopLeftRadiusValueEnumWrapper { AzStyleBorderTopLeftRadiusValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderTopLeftRadiusValueEnumWrapper { AzStyleBorderTopLeftRadiusValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderTopLeftRadiusValueEnumWrapper { AzStyleBorderTopLeftRadiusValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderTopLeftRadiusValueEnumWrapper { AzStyleBorderTopLeftRadiusValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadiusValueEnumWrapper { AzStyleBorderTopLeftRadiusValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderTopRightRadiusValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderTopRightRadiusValueEnumWrapper { AzStyleBorderTopRightRadiusValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderTopRightRadiusValueEnumWrapper { AzStyleBorderTopRightRadiusValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderTopRightRadiusValueEnumWrapper { AzStyleBorderTopRightRadiusValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderTopRightRadiusValueEnumWrapper { AzStyleBorderTopRightRadiusValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderTopRightRadius) -> AzStyleBorderTopRightRadiusValueEnumWrapper { AzStyleBorderTopRightRadiusValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBorderTopStyleValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBorderTopStyleValueEnumWrapper { AzStyleBorderTopStyleValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBorderTopStyleValueEnumWrapper { AzStyleBorderTopStyleValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBorderTopStyleValueEnumWrapper { AzStyleBorderTopStyleValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBorderTopStyleValueEnumWrapper { AzStyleBorderTopStyleValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBorderTopStyle) -> AzStyleBorderTopStyleValueEnumWrapper { AzStyleBorderTopStyleValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzLayoutBorderTopWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzLayoutBorderTopWidthValueEnumWrapper { AzLayoutBorderTopWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzLayoutBorderTopWidthValueEnumWrapper { AzLayoutBorderTopWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzLayoutBorderTopWidthValueEnumWrapper { AzLayoutBorderTopWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzLayoutBorderTopWidthValueEnumWrapper { AzLayoutBorderTopWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: LayoutBorderTopWidth) -> AzLayoutBorderTopWidthValueEnumWrapper { AzLayoutBorderTopWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleCursorValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleCursorValueEnumWrapper { AzStyleCursorValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleCursorValueEnumWrapper { AzStyleCursorValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleCursorValueEnumWrapper { AzStyleCursorValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleCursorValueEnumWrapper { AzStyleCursorValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleCursor) -> AzStyleCursorValueEnumWrapper { AzStyleCursorValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleFontFamilyVecValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleFontFamilyVecValueEnumWrapper { AzStyleFontFamilyVecValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleFontFamilyVecValueEnumWrapper { AzStyleFontFamilyVecValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleFontFamilyVecValueEnumWrapper { AzStyleFontFamilyVecValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleFontFamilyVecValueEnumWrapper { AzStyleFontFamilyVecValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleFontFamilyVec) -> AzStyleFontFamilyVecValueEnumWrapper { AzStyleFontFamilyVecValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleFontSizeValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleFontSizeValueEnumWrapper { AzStyleFontSizeValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleFontSizeValueEnumWrapper { AzStyleFontSizeValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleFontSizeValueEnumWrapper { AzStyleFontSizeValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleFontSizeValueEnumWrapper { AzStyleFontSizeValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleFontSize) -> AzStyleFontSizeValueEnumWrapper { AzStyleFontSizeValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleLetterSpacingValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleLetterSpacingValueEnumWrapper { AzStyleLetterSpacingValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleLetterSpacingValueEnumWrapper { AzStyleLetterSpacingValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleLetterSpacingValueEnumWrapper { AzStyleLetterSpacingValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleLetterSpacingValueEnumWrapper { AzStyleLetterSpacingValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleLetterSpacing) -> AzStyleLetterSpacingValueEnumWrapper { AzStyleLetterSpacingValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleLineHeightValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleLineHeightValueEnumWrapper { AzStyleLineHeightValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleLineHeightValueEnumWrapper { AzStyleLineHeightValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleLineHeightValueEnumWrapper { AzStyleLineHeightValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleLineHeightValueEnumWrapper { AzStyleLineHeightValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleLineHeight) -> AzStyleLineHeightValueEnumWrapper { AzStyleLineHeightValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleTabWidthValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleTabWidthValueEnumWrapper { AzStyleTabWidthValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleTabWidthValueEnumWrapper { AzStyleTabWidthValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleTabWidthValueEnumWrapper { AzStyleTabWidthValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleTabWidthValueEnumWrapper { AzStyleTabWidthValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleTabWidth) -> AzStyleTabWidthValueEnumWrapper { AzStyleTabWidthValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleTextAlignValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleTextAlignValueEnumWrapper { AzStyleTextAlignValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleTextAlignValueEnumWrapper { AzStyleTextAlignValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleTextAlignValueEnumWrapper { AzStyleTextAlignValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleTextAlignValueEnumWrapper { AzStyleTextAlignValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleTextAlign) -> AzStyleTextAlignValueEnumWrapper { AzStyleTextAlignValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleTextColorValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleTextColorValueEnumWrapper { AzStyleTextColorValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleTextColorValueEnumWrapper { AzStyleTextColorValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleTextColorValueEnumWrapper { AzStyleTextColorValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleTextColorValueEnumWrapper { AzStyleTextColorValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleTextColor) -> AzStyleTextColorValueEnumWrapper { AzStyleTextColorValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleWordSpacingValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleWordSpacingValueEnumWrapper { AzStyleWordSpacingValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleWordSpacingValueEnumWrapper { AzStyleWordSpacingValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleWordSpacingValueEnumWrapper { AzStyleWordSpacingValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleWordSpacingValueEnumWrapper { AzStyleWordSpacingValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleWordSpacing) -> AzStyleWordSpacingValueEnumWrapper { AzStyleWordSpacingValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleOpacityValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleOpacityValueEnumWrapper { AzStyleOpacityValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleOpacityValueEnumWrapper { AzStyleOpacityValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleOpacityValueEnumWrapper { AzStyleOpacityValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleOpacityValueEnumWrapper { AzStyleOpacityValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleOpacity) -> AzStyleOpacityValueEnumWrapper { AzStyleOpacityValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleTransformVecValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleTransformVecValueEnumWrapper { AzStyleTransformVecValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleTransformVecValueEnumWrapper { AzStyleTransformVecValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleTransformVecValueEnumWrapper { AzStyleTransformVecValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleTransformVecValueEnumWrapper { AzStyleTransformVecValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleTransformVec) -> AzStyleTransformVecValueEnumWrapper { AzStyleTransformVecValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleTransformOriginValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleTransformOriginValueEnumWrapper { AzStyleTransformOriginValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleTransformOriginValueEnumWrapper { AzStyleTransformOriginValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleTransformOriginValueEnumWrapper { AzStyleTransformOriginValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleTransformOriginValueEnumWrapper { AzStyleTransformOriginValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleTransformOrigin) -> AzStyleTransformOriginValueEnumWrapper { AzStyleTransformOriginValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStylePerspectiveOriginValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStylePerspectiveOriginValueEnumWrapper { AzStylePerspectiveOriginValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStylePerspectiveOriginValueEnumWrapper { AzStylePerspectiveOriginValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStylePerspectiveOriginValueEnumWrapper { AzStylePerspectiveOriginValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStylePerspectiveOriginValueEnumWrapper { AzStylePerspectiveOriginValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StylePerspectiveOrigin) -> AzStylePerspectiveOriginValueEnumWrapper { AzStylePerspectiveOriginValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzStyleBackfaceVisibilityValueEnumWrapper {
    #[classattr]
    fn Auto() -> AzStyleBackfaceVisibilityValueEnumWrapper { AzStyleBackfaceVisibilityValueEnumWrapper::Auto }
    #[classattr]
    fn None() -> AzStyleBackfaceVisibilityValueEnumWrapper { AzStyleBackfaceVisibilityValueEnumWrapper::None }
    #[classattr]
    fn Inherit() -> AzStyleBackfaceVisibilityValueEnumWrapper { AzStyleBackfaceVisibilityValueEnumWrapper::Inherit }
    #[classattr]
    fn Initial() -> AzStyleBackfaceVisibilityValueEnumWrapper { AzStyleBackfaceVisibilityValueEnumWrapper::Initial }
    #[staticmethod]
    fn Exact(v: StyleBackfaceVisibility) -> AzStyleBackfaceVisibilityValueEnumWrapper { AzStyleBackfaceVisibilityValueEnumWrapper::Exact(v) }
}

#[pymethods]
impl AzCssPropertyEnumWrapper {
    #[staticmethod]
    fn TextColor(v: StyleTextColorValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::TextColor(v) }
    #[staticmethod]
    fn FontSize(v: StyleFontSizeValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::FontSize(v) }
    #[staticmethod]
    fn FontFamily(v: StyleFontFamilyVecValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::FontFamily(v) }
    #[staticmethod]
    fn TextAlign(v: StyleTextAlignValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::TextAlign(v) }
    #[staticmethod]
    fn LetterSpacing(v: StyleLetterSpacingValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::LetterSpacing(v) }
    #[staticmethod]
    fn LineHeight(v: StyleLineHeightValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::LineHeight(v) }
    #[staticmethod]
    fn WordSpacing(v: StyleWordSpacingValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::WordSpacing(v) }
    #[staticmethod]
    fn TabWidth(v: StyleTabWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::TabWidth(v) }
    #[staticmethod]
    fn Cursor(v: StyleCursorValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Cursor(v) }
    #[staticmethod]
    fn Display(v: LayoutDisplayValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Display(v) }
    #[staticmethod]
    fn Float(v: LayoutFloatValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Float(v) }
    #[staticmethod]
    fn BoxSizing(v: LayoutBoxSizingValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BoxSizing(v) }
    #[staticmethod]
    fn Width(v: LayoutWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Width(v) }
    #[staticmethod]
    fn Height(v: LayoutHeightValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Height(v) }
    #[staticmethod]
    fn MinWidth(v: LayoutMinWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MinWidth(v) }
    #[staticmethod]
    fn MinHeight(v: LayoutMinHeightValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MinHeight(v) }
    #[staticmethod]
    fn MaxWidth(v: LayoutMaxWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MaxWidth(v) }
    #[staticmethod]
    fn MaxHeight(v: LayoutMaxHeightValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MaxHeight(v) }
    #[staticmethod]
    fn Position(v: LayoutPositionValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Position(v) }
    #[staticmethod]
    fn Top(v: LayoutTopValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Top(v) }
    #[staticmethod]
    fn Right(v: LayoutRightValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Right(v) }
    #[staticmethod]
    fn Left(v: LayoutLeftValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Left(v) }
    #[staticmethod]
    fn Bottom(v: LayoutBottomValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Bottom(v) }
    #[staticmethod]
    fn FlexWrap(v: LayoutFlexWrapValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::FlexWrap(v) }
    #[staticmethod]
    fn FlexDirection(v: LayoutFlexDirectionValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::FlexDirection(v) }
    #[staticmethod]
    fn FlexGrow(v: LayoutFlexGrowValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::FlexGrow(v) }
    #[staticmethod]
    fn FlexShrink(v: LayoutFlexShrinkValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::FlexShrink(v) }
    #[staticmethod]
    fn JustifyContent(v: LayoutJustifyContentValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::JustifyContent(v) }
    #[staticmethod]
    fn AlignItems(v: LayoutAlignItemsValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::AlignItems(v) }
    #[staticmethod]
    fn AlignContent(v: LayoutAlignContentValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::AlignContent(v) }
    #[staticmethod]
    fn BackgroundContent(v: StyleBackgroundContentVecValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BackgroundContent(v) }
    #[staticmethod]
    fn BackgroundPosition(v: StyleBackgroundPositionVecValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BackgroundPosition(v) }
    #[staticmethod]
    fn BackgroundSize(v: StyleBackgroundSizeVecValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BackgroundSize(v) }
    #[staticmethod]
    fn BackgroundRepeat(v: StyleBackgroundRepeatVecValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BackgroundRepeat(v) }
    #[staticmethod]
    fn OverflowX(v: LayoutOverflowValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::OverflowX(v) }
    #[staticmethod]
    fn OverflowY(v: LayoutOverflowValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::OverflowY(v) }
    #[staticmethod]
    fn PaddingTop(v: LayoutPaddingTopValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::PaddingTop(v) }
    #[staticmethod]
    fn PaddingLeft(v: LayoutPaddingLeftValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::PaddingLeft(v) }
    #[staticmethod]
    fn PaddingRight(v: LayoutPaddingRightValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::PaddingRight(v) }
    #[staticmethod]
    fn PaddingBottom(v: LayoutPaddingBottomValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::PaddingBottom(v) }
    #[staticmethod]
    fn MarginTop(v: LayoutMarginTopValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MarginTop(v) }
    #[staticmethod]
    fn MarginLeft(v: LayoutMarginLeftValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MarginLeft(v) }
    #[staticmethod]
    fn MarginRight(v: LayoutMarginRightValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MarginRight(v) }
    #[staticmethod]
    fn MarginBottom(v: LayoutMarginBottomValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::MarginBottom(v) }
    #[staticmethod]
    fn BorderTopLeftRadius(v: StyleBorderTopLeftRadiusValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderTopLeftRadius(v) }
    #[staticmethod]
    fn BorderTopRightRadius(v: StyleBorderTopRightRadiusValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderTopRightRadius(v) }
    #[staticmethod]
    fn BorderBottomLeftRadius(v: StyleBorderBottomLeftRadiusValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderBottomLeftRadius(v) }
    #[staticmethod]
    fn BorderBottomRightRadius(v: StyleBorderBottomRightRadiusValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderBottomRightRadius(v) }
    #[staticmethod]
    fn BorderTopColor(v: StyleBorderTopColorValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderTopColor(v) }
    #[staticmethod]
    fn BorderRightColor(v: StyleBorderRightColorValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderRightColor(v) }
    #[staticmethod]
    fn BorderLeftColor(v: StyleBorderLeftColorValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderLeftColor(v) }
    #[staticmethod]
    fn BorderBottomColor(v: StyleBorderBottomColorValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderBottomColor(v) }
    #[staticmethod]
    fn BorderTopStyle(v: StyleBorderTopStyleValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderTopStyle(v) }
    #[staticmethod]
    fn BorderRightStyle(v: StyleBorderRightStyleValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderRightStyle(v) }
    #[staticmethod]
    fn BorderLeftStyle(v: StyleBorderLeftStyleValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderLeftStyle(v) }
    #[staticmethod]
    fn BorderBottomStyle(v: StyleBorderBottomStyleValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderBottomStyle(v) }
    #[staticmethod]
    fn BorderTopWidth(v: LayoutBorderTopWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderTopWidth(v) }
    #[staticmethod]
    fn BorderRightWidth(v: LayoutBorderRightWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderRightWidth(v) }
    #[staticmethod]
    fn BorderLeftWidth(v: LayoutBorderLeftWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderLeftWidth(v) }
    #[staticmethod]
    fn BorderBottomWidth(v: LayoutBorderBottomWidthValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BorderBottomWidth(v) }
    #[staticmethod]
    fn BoxShadowLeft(v: StyleBoxShadowValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BoxShadowLeft(v) }
    #[staticmethod]
    fn BoxShadowRight(v: StyleBoxShadowValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BoxShadowRight(v) }
    #[staticmethod]
    fn BoxShadowTop(v: StyleBoxShadowValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BoxShadowTop(v) }
    #[staticmethod]
    fn BoxShadowBottom(v: StyleBoxShadowValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BoxShadowBottom(v) }
    #[staticmethod]
    fn ScrollbarStyle(v: ScrollbarStyleValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::ScrollbarStyle(v) }
    #[staticmethod]
    fn Opacity(v: StyleOpacityValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Opacity(v) }
    #[staticmethod]
    fn Transform(v: StyleTransformVecValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::Transform(v) }
    #[staticmethod]
    fn TransformOrigin(v: StyleTransformOriginValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::TransformOrigin(v) }
    #[staticmethod]
    fn PerspectiveOrigin(v: StylePerspectiveOriginValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::PerspectiveOrigin(v) }
    #[staticmethod]
    fn BackfaceVisibility(v: StyleBackfaceVisibilityValue) -> AzCssPropertyEnumWrapper { AzCssPropertyEnumWrapper::BackfaceVisibility(v) }
}

#[pymethods]
impl AzCssPropertySourceEnumWrapper {
    #[staticmethod]
    fn Css(v: CssPath) -> AzCssPropertySourceEnumWrapper { AzCssPropertySourceEnumWrapper::Css(v) }
    #[classattr]
    fn Inline() -> AzCssPropertySourceEnumWrapper { AzCssPropertySourceEnumWrapper::Inline }
}

#[pymethods]
impl AzStyledDom {
    #[staticmethod]
    fn new(dom: AzDom, css: AzCss) -> AzStyledDom {
        unsafe { mem::transmute(crate::AzStyledDom_new(
            mem::transmute(dom),
            mem::transmute(css),
        )) }
    }
    #[staticmethod]
    fn from_xml(xml_string: String) -> AzStyledDom {
        let xml_string: AzString = xml_string.into();
unsafe { mem::transmute(crate::AzStyledDom_fromXml(
            mem::transmute(xml_string),
        )) }
    }
    #[staticmethod]
    fn from_file(xml_file_path: String) -> AzStyledDom {
        let xml_file_path: AzString = xml_file_path.into();
unsafe { mem::transmute(crate::AzStyledDom_fromFile(
            mem::transmute(xml_file_path),
        )) }
    }
    fn append_child(&mut self, dom: AzStyledDom) -> () {
        ()
    }
    fn restyle(&mut self, css: AzCss) -> () {
        ()
    }
    fn node_count(&self) -> usize {
        ()
    }
    fn get_html_string(&self) -> String {
        ()
    }
}

#[pymethods]
impl AzTexture {
    #[staticmethod]
    fn allocate_clip_mask(gl: AzGl, size: AzLayoutSize) -> AzTexture {
        unsafe { mem::transmute(crate::AzTexture_allocateClipMask(
            mem::transmute(gl),
            mem::transmute(size),
        )) }
    }
    fn draw_clip_mask(&mut self, node: AzTesselatedSvgNode) -> bool {
        ()
    }
    fn apply_fxaa(&mut self) -> bool {
        ()
    }
}

#[pymethods]
impl AzGl {
    fn get_type(&self) -> AzGlType {
        ()
    }
    fn buffer_data_untyped(&self, target: u32, size: isize, data: c_void, usage: u32) -> () {
        ()
    }
    fn buffer_sub_data_untyped(&self, target: u32, offset: isize, size: isize, data: c_void) -> () {
        ()
    }
    fn map_buffer(&self, target: u32, access: u32) -> *mut c_void {
        ()
    }
    fn map_buffer_range(&self, target: u32, offset: isize, length: isize, access: u32) -> *mut c_void {
        ()
    }
    fn unmap_buffer(&self, target: u32) -> u8 {
        ()
    }
    fn tex_buffer(&self, target: u32, internal_format: u32, buffer: u32) -> () {
        ()
    }
    fn shader_source(&self, shader: u32, strings: AzStringVec) -> () {
        ()
    }
    fn read_buffer(&self, mode: u32) -> () {
        ()
    }
    fn read_pixels_into_buffer(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32, dst_buffer: AzU8VecRefMut) -> () {
        ()
    }
    fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32) -> AzU8Vec {
        ()
    }
    fn read_pixels_into_pbo(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32) -> () {
        ()
    }
    fn sample_coverage(&self, value: f32, invert: bool) -> () {
        ()
    }
    fn polygon_offset(&self, factor: f32, units: f32) -> () {
        ()
    }
    fn pixel_store_i(&self, name: u32, param: i32) -> () {
        ()
    }
    fn gen_buffers(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn gen_renderbuffers(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn gen_framebuffers(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn gen_textures(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn gen_vertex_arrays(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn gen_queries(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn begin_query(&self, target: u32, id: u32) -> () {
        ()
    }
    fn end_query(&self, target: u32) -> () {
        ()
    }
    fn query_counter(&self, id: u32, target: u32) -> () {
        ()
    }
    fn get_query_object_iv(&self, id: u32, pname: u32) -> i32 {
        ()
    }
    fn get_query_object_uiv(&self, id: u32, pname: u32) -> u32 {
        ()
    }
    fn get_query_object_i64v(&self, id: u32, pname: u32) -> i64 {
        ()
    }
    fn get_query_object_ui64v(&self, id: u32, pname: u32) -> u64 {
        ()
    }
    fn delete_queries(&self, queries: AzGLuintVecRef) -> () {
        ()
    }
    fn delete_vertex_arrays(&self, vertex_arrays: AzGLuintVecRef) -> () {
        ()
    }
    fn delete_buffers(&self, buffers: AzGLuintVecRef) -> () {
        ()
    }
    fn delete_renderbuffers(&self, renderbuffers: AzGLuintVecRef) -> () {
        ()
    }
    fn delete_framebuffers(&self, framebuffers: AzGLuintVecRef) -> () {
        ()
    }
    fn delete_textures(&self, textures: AzGLuintVecRef) -> () {
        ()
    }
    fn framebuffer_renderbuffer(&self, target: u32, attachment: u32, renderbuffertarget: u32, renderbuffer: u32) -> () {
        ()
    }
    fn renderbuffer_storage(&self, target: u32, internalformat: u32, width: i32, height: i32) -> () {
        ()
    }
    fn depth_func(&self, func: u32) -> () {
        ()
    }
    fn active_texture(&self, texture: u32) -> () {
        ()
    }
    fn attach_shader(&self, program: u32, shader: u32) -> () {
        ()
    }
    fn bind_attrib_location(&self, program: u32, index: u32, name: AzRefstr) -> () {
        ()
    }
    fn get_uniform_iv(&self, program: u32, location: i32, result: AzGLintVecRefMut) -> () {
        ()
    }
    fn get_uniform_fv(&self, program: u32, location: i32, result: AzGLfloatVecRefMut) -> () {
        ()
    }
    fn get_uniform_block_index(&self, program: u32, name: AzRefstr) -> u32 {
        ()
    }
    fn get_uniform_indices(&self, program: u32, names: AzRefstrVecRef) -> AzGLuintVec {
        ()
    }
    fn bind_buffer_base(&self, target: u32, index: u32, buffer: u32) -> () {
        ()
    }
    fn bind_buffer_range(&self, target: u32, index: u32, buffer: u32, offset: isize, size: isize) -> () {
        ()
    }
    fn uniform_block_binding(&self, program: u32, uniform_block_index: u32, uniform_block_binding: u32) -> () {
        ()
    }
    fn bind_buffer(&self, target: u32, buffer: u32) -> () {
        ()
    }
    fn bind_vertex_array(&self, vao: u32) -> () {
        ()
    }
    fn bind_renderbuffer(&self, target: u32, renderbuffer: u32) -> () {
        ()
    }
    fn bind_framebuffer(&self, target: u32, framebuffer: u32) -> () {
        ()
    }
    fn bind_texture(&self, target: u32, texture: u32) -> () {
        ()
    }
    fn draw_buffers(&self, bufs: AzGLenumVecRef) -> () {
        ()
    }
    fn tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32, format: u32, ty: u32, opt_data: AzOptionU8VecRefEnumWrapper) -> () {
        ()
    }
    fn compressed_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, width: i32, height: i32, border: i32, data: AzU8VecRef) -> () {
        ()
    }
    fn compressed_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, data: AzU8VecRef) -> () {
        ()
    }
    fn tex_image_3d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, opt_data: AzOptionU8VecRefEnumWrapper) -> () {
        ()
    }
    fn copy_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, x: i32, y: i32, width: i32, height: i32, border: i32) -> () {
        ()
    }
    fn copy_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, x: i32, y: i32, width: i32, height: i32) -> () {
        ()
    }
    fn copy_tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, x: i32, y: i32, width: i32, height: i32) -> () {
        ()
    }
    fn tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, data: AzU8VecRef) -> () {
        ()
    }
    fn tex_sub_image_2d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, offset: usize) -> () {
        ()
    }
    fn tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, data: AzU8VecRef) -> () {
        ()
    }
    fn tex_sub_image_3d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, offset: usize) -> () {
        ()
    }
    fn tex_storage_2d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32) -> () {
        ()
    }
    fn tex_storage_3d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32, depth: i32) -> () {
        ()
    }
    fn get_tex_image_into_buffer(&self, target: u32, level: i32, format: u32, ty: u32, output: AzU8VecRefMut) -> () {
        ()
    }
    fn copy_image_sub_data(&self, src_name: u32, src_target: u32, src_level: i32, src_x: i32, src_y: i32, src_z: i32, dst_name: u32, dst_target: u32, dst_level: i32, dst_x: i32, dst_y: i32, dst_z: i32, src_width: i32, src_height: i32, src_depth: i32) -> () {
        ()
    }
    fn invalidate_framebuffer(&self, target: u32, attachments: AzGLenumVecRef) -> () {
        ()
    }
    fn invalidate_sub_framebuffer(&self, target: u32, attachments: AzGLenumVecRef, xoffset: i32, yoffset: i32, width: i32, height: i32) -> () {
        ()
    }
    fn get_integer_v(&self, name: u32, result: AzGLintVecRefMut) -> () {
        ()
    }
    fn get_integer_64v(&self, name: u32, result: AzGLint64VecRefMut) -> () {
        ()
    }
    fn get_integer_iv(&self, name: u32, index: u32, result: AzGLintVecRefMut) -> () {
        ()
    }
    fn get_integer_64iv(&self, name: u32, index: u32, result: AzGLint64VecRefMut) -> () {
        ()
    }
    fn get_boolean_v(&self, name: u32, result: AzGLbooleanVecRefMut) -> () {
        ()
    }
    fn get_float_v(&self, name: u32, result: AzGLfloatVecRefMut) -> () {
        ()
    }
    fn get_framebuffer_attachment_parameter_iv(&self, target: u32, attachment: u32, pname: u32) -> i32 {
        ()
    }
    fn get_renderbuffer_parameter_iv(&self, target: u32, pname: u32) -> i32 {
        ()
    }
    fn get_tex_parameter_iv(&self, target: u32, name: u32) -> i32 {
        ()
    }
    fn get_tex_parameter_fv(&self, target: u32, name: u32) -> f32 {
        ()
    }
    fn tex_parameter_i(&self, target: u32, pname: u32, param: i32) -> () {
        ()
    }
    fn tex_parameter_f(&self, target: u32, pname: u32, param: f32) -> () {
        ()
    }
    fn framebuffer_texture_2d(&self, target: u32, attachment: u32, textarget: u32, texture: u32, level: i32) -> () {
        ()
    }
    fn framebuffer_texture_layer(&self, target: u32, attachment: u32, texture: u32, level: i32, layer: i32) -> () {
        ()
    }
    fn blit_framebuffer(&self, src_x0: i32, src_y0: i32, src_x1: i32, src_y1: i32, dst_x0: i32, dst_y0: i32, dst_x1: i32, dst_y1: i32, mask: u32, filter: u32) -> () {
        ()
    }
    fn vertex_attrib_4f(&self, index: u32, x: f32, y: f32, z: f32, w: f32) -> () {
        ()
    }
    fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, normalized: bool, stride: i32, offset: u32) -> () {
        ()
    }
    fn vertex_attrib_pointer(&self, index: u32, size: i32, type_: u32, normalized: bool, stride: i32, offset: u32) -> () {
        ()
    }
    fn vertex_attrib_i_pointer(&self, index: u32, size: i32, type_: u32, stride: i32, offset: u32) -> () {
        ()
    }
    fn vertex_attrib_divisor(&self, index: u32, divisor: u32) -> () {
        ()
    }
    fn viewport(&self, x: i32, y: i32, width: i32, height: i32) -> () {
        ()
    }
    fn scissor(&self, x: i32, y: i32, width: i32, height: i32) -> () {
        ()
    }
    fn line_width(&self, width: f32) -> () {
        ()
    }
    fn use_program(&self, program: u32) -> () {
        ()
    }
    fn validate_program(&self, program: u32) -> () {
        ()
    }
    fn draw_arrays(&self, mode: u32, first: i32, count: i32) -> () {
        ()
    }
    fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, primcount: i32) -> () {
        ()
    }
    fn draw_elements(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32) -> () {
        ()
    }
    fn draw_elements_instanced(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32, primcount: i32) -> () {
        ()
    }
    fn blend_color(&self, r: f32, g: f32, b: f32, a: f32) -> () {
        ()
    }
    fn blend_func(&self, sfactor: u32, dfactor: u32) -> () {
        ()
    }
    fn blend_func_separate(&self, src_rgb: u32, dest_rgb: u32, src_alpha: u32, dest_alpha: u32) -> () {
        ()
    }
    fn blend_equation(&self, mode: u32) -> () {
        ()
    }
    fn blend_equation_separate(&self, mode_rgb: u32, mode_alpha: u32) -> () {
        ()
    }
    fn color_mask(&self, r: bool, g: bool, b: bool, a: bool) -> () {
        ()
    }
    fn cull_face(&self, mode: u32) -> () {
        ()
    }
    fn front_face(&self, mode: u32) -> () {
        ()
    }
    fn enable(&self, cap: u32) -> () {
        ()
    }
    fn disable(&self, cap: u32) -> () {
        ()
    }
    fn hint(&self, param_name: u32, param_val: u32) -> () {
        ()
    }
    fn is_enabled(&self, cap: u32) -> u8 {
        ()
    }
    fn is_shader(&self, shader: u32) -> u8 {
        ()
    }
    fn is_texture(&self, texture: u32) -> u8 {
        ()
    }
    fn is_framebuffer(&self, framebuffer: u32) -> u8 {
        ()
    }
    fn is_renderbuffer(&self, renderbuffer: u32) -> u8 {
        ()
    }
    fn check_frame_buffer_status(&self, target: u32) -> u32 {
        ()
    }
    fn enable_vertex_attrib_array(&self, index: u32) -> () {
        ()
    }
    fn disable_vertex_attrib_array(&self, index: u32) -> () {
        ()
    }
    fn uniform_1f(&self, location: i32, v0: f32) -> () {
        ()
    }
    fn uniform_1fv(&self, location: i32, values: AzF32VecRef) -> () {
        ()
    }
    fn uniform_1i(&self, location: i32, v0: i32) -> () {
        ()
    }
    fn uniform_1iv(&self, location: i32, values: AzI32VecRef) -> () {
        ()
    }
    fn uniform_1ui(&self, location: i32, v0: u32) -> () {
        ()
    }
    fn uniform_2f(&self, location: i32, v0: f32, v1: f32) -> () {
        ()
    }
    fn uniform_2fv(&self, location: i32, values: AzF32VecRef) -> () {
        ()
    }
    fn uniform_2i(&self, location: i32, v0: i32, v1: i32) -> () {
        ()
    }
    fn uniform_2iv(&self, location: i32, values: AzI32VecRef) -> () {
        ()
    }
    fn uniform_2ui(&self, location: i32, v0: u32, v1: u32) -> () {
        ()
    }
    fn uniform_3f(&self, location: i32, v0: f32, v1: f32, v2: f32) -> () {
        ()
    }
    fn uniform_3fv(&self, location: i32, values: AzF32VecRef) -> () {
        ()
    }
    fn uniform_3i(&self, location: i32, v0: i32, v1: i32, v2: i32) -> () {
        ()
    }
    fn uniform_3iv(&self, location: i32, values: AzI32VecRef) -> () {
        ()
    }
    fn uniform_3ui(&self, location: i32, v0: u32, v1: u32, v2: u32) -> () {
        ()
    }
    fn uniform_4f(&self, location: i32, x: f32, y: f32, z: f32, w: f32) -> () {
        ()
    }
    fn uniform_4i(&self, location: i32, x: i32, y: i32, z: i32, w: i32) -> () {
        ()
    }
    fn uniform_4iv(&self, location: i32, values: AzI32VecRef) -> () {
        ()
    }
    fn uniform_4ui(&self, location: i32, x: u32, y: u32, z: u32, w: u32) -> () {
        ()
    }
    fn uniform_4fv(&self, location: i32, values: AzF32VecRef) -> () {
        ()
    }
    fn uniform_matrix_2fv(&self, location: i32, transpose: bool, value: AzF32VecRef) -> () {
        ()
    }
    fn uniform_matrix_3fv(&self, location: i32, transpose: bool, value: AzF32VecRef) -> () {
        ()
    }
    fn uniform_matrix_4fv(&self, location: i32, transpose: bool, value: AzF32VecRef) -> () {
        ()
    }
    fn depth_mask(&self, flag: bool) -> () {
        ()
    }
    fn depth_range(&self, near: f64, far: f64) -> () {
        ()
    }
    fn get_active_attrib(&self, program: u32, index: u32) -> AzGetActiveAttribReturn {
        ()
    }
    fn get_active_uniform(&self, program: u32, index: u32) -> AzGetActiveUniformReturn {
        ()
    }
    fn get_active_uniforms_iv(&self, program: u32, indices: AzGLuintVec, pname: u32) -> AzGLintVec {
        ()
    }
    fn get_active_uniform_block_i(&self, program: u32, index: u32, pname: u32) -> i32 {
        ()
    }
    fn get_active_uniform_block_iv(&self, program: u32, index: u32, pname: u32) -> AzGLintVec {
        ()
    }
    fn get_active_uniform_block_name(&self, program: u32, index: u32) -> String {
        ()
    }
    fn get_attrib_location(&self, program: u32, name: AzRefstr) -> i32 {
        ()
    }
    fn get_frag_data_location(&self, program: u32, name: AzRefstr) -> i32 {
        ()
    }
    fn get_uniform_location(&self, program: u32, name: AzRefstr) -> i32 {
        ()
    }
    fn get_program_info_log(&self, program: u32) -> String {
        ()
    }
    fn get_program_iv(&self, program: u32, pname: u32, result: AzGLintVecRefMut) -> () {
        ()
    }
    fn get_program_binary(&self, program: u32) -> AzGetProgramBinaryReturn {
        ()
    }
    fn program_binary(&self, program: u32, format: u32, binary: AzU8VecRef) -> () {
        ()
    }
    fn program_parameter_i(&self, program: u32, pname: u32, value: i32) -> () {
        ()
    }
    fn get_vertex_attrib_iv(&self, index: u32, pname: u32, result: AzGLintVecRefMut) -> () {
        ()
    }
    fn get_vertex_attrib_fv(&self, index: u32, pname: u32, result: AzGLfloatVecRefMut) -> () {
        ()
    }
    fn get_vertex_attrib_pointer_v(&self, index: u32, pname: u32) -> isize {
        ()
    }
    fn get_buffer_parameter_iv(&self, target: u32, pname: u32) -> i32 {
        ()
    }
    fn get_shader_info_log(&self, shader: u32) -> String {
        ()
    }
    fn get_string(&self, which: u32) -> String {
        ()
    }
    fn get_string_i(&self, which: u32, index: u32) -> String {
        ()
    }
    fn get_shader_iv(&self, shader: u32, pname: u32, result: AzGLintVecRefMut) -> () {
        ()
    }
    fn get_shader_precision_format(&self, shader_type: u32, precision_type: u32) -> AzGlShaderPrecisionFormatReturn {
        ()
    }
    fn compile_shader(&self, shader: u32) -> () {
        ()
    }
    fn create_program(&self) -> u32 {
        ()
    }
    fn delete_program(&self, program: u32) -> () {
        ()
    }
    fn create_shader(&self, shader_type: u32) -> u32 {
        ()
    }
    fn delete_shader(&self, shader: u32) -> () {
        ()
    }
    fn detach_shader(&self, program: u32, shader: u32) -> () {
        ()
    }
    fn link_program(&self, program: u32) -> () {
        ()
    }
    fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) -> () {
        ()
    }
    fn clear(&self, buffer_mask: u32) -> () {
        ()
    }
    fn clear_depth(&self, depth: f64) -> () {
        ()
    }
    fn clear_stencil(&self, s: i32) -> () {
        ()
    }
    fn flush(&self) -> () {
        ()
    }
    fn finish(&self) -> () {
        ()
    }
    fn get_error(&self) -> u32 {
        ()
    }
    fn stencil_mask(&self, mask: u32) -> () {
        ()
    }
    fn stencil_mask_separate(&self, face: u32, mask: u32) -> () {
        ()
    }
    fn stencil_func(&self, func: u32, ref_: i32, mask: u32) -> () {
        ()
    }
    fn stencil_func_separate(&self, face: u32, func: u32, ref_: i32, mask: u32) -> () {
        ()
    }
    fn stencil_op(&self, sfail: u32, dpfail: u32, dppass: u32) -> () {
        ()
    }
    fn stencil_op_separate(&self, face: u32, sfail: u32, dpfail: u32, dppass: u32) -> () {
        ()
    }
    fn egl_image_target_texture2d_oes(&self, target: u32, image: c_void) -> () {
        ()
    }
    fn generate_mipmap(&self, target: u32) -> () {
        ()
    }
    fn insert_event_marker_ext(&self, message: AzRefstr) -> () {
        ()
    }
    fn push_group_marker_ext(&self, message: AzRefstr) -> () {
        ()
    }
    fn pop_group_marker_ext(&self) -> () {
        ()
    }
    fn debug_message_insert_khr(&self, source: u32, type_: u32, id: u32, severity: u32, message: AzRefstr) -> () {
        ()
    }
    fn push_debug_group_khr(&self, source: u32, id: u32, message: AzRefstr) -> () {
        ()
    }
    fn pop_debug_group_khr(&self) -> () {
        ()
    }
    fn fence_sync(&self, condition: u32, flags: u32) -> AzGLsyncPtr {
        ()
    }
    fn client_wait_sync(&self, sync: AzGLsyncPtr, flags: u32, timeout: u64) -> u32 {
        ()
    }
    fn wait_sync(&self, sync: AzGLsyncPtr, flags: u32, timeout: u64) -> () {
        ()
    }
    fn delete_sync(&self, sync: AzGLsyncPtr) -> () {
        ()
    }
    fn texture_range_apple(&self, target: u32, data: AzU8VecRef) -> () {
        ()
    }
    fn gen_fences_apple(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn delete_fences_apple(&self, fences: AzGLuintVecRef) -> () {
        ()
    }
    fn set_fence_apple(&self, fence: u32) -> () {
        ()
    }
    fn finish_fence_apple(&self, fence: u32) -> () {
        ()
    }
    fn test_fence_apple(&self, fence: u32) -> () {
        ()
    }
    fn test_object_apple(&self, object: u32, name: u32) -> u8 {
        ()
    }
    fn finish_object_apple(&self, object: u32, name: u32) -> () {
        ()
    }
    fn get_frag_data_index(&self, program: u32, name: AzRefstr) -> i32 {
        ()
    }
    fn blend_barrier_khr(&self) -> () {
        ()
    }
    fn bind_frag_data_location_indexed(&self, program: u32, color_number: u32, index: u32, name: AzRefstr) -> () {
        ()
    }
    fn get_debug_messages(&self) -> AzDebugMessageVec {
        ()
    }
    fn provoking_vertex_angle(&self, mode: u32) -> () {
        ()
    }
    fn gen_vertex_arrays_apple(&self, n: i32) -> AzGLuintVec {
        ()
    }
    fn bind_vertex_array_apple(&self, vao: u32) -> () {
        ()
    }
    fn delete_vertex_arrays_apple(&self, vertex_arrays: AzGLuintVecRef) -> () {
        ()
    }
    fn copy_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8) -> () {
        ()
    }
    fn copy_sub_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, x: i32, y: i32, width: i32, height: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8) -> () {
        ()
    }
    fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: c_void) -> () {
        ()
    }
    fn copy_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8) -> () {
        ()
    }
    fn copy_sub_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, z_offset: i32, x: i32, y: i32, z: i32, width: i32, height: i32, depth: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8) -> () {
        ()
    }
    fn buffer_storage(&self, target: u32, size: isize, data: c_void, flags: u32) -> () {
        ()
    }
    fn flush_mapped_buffer_range(&self, target: u32, offset: isize, length: isize) -> () {
        ()
    }
}

#[pymethods]
impl AzVertexAttributeTypeEnumWrapper {
    #[classattr]
    fn Float() -> AzVertexAttributeTypeEnumWrapper { AzVertexAttributeTypeEnumWrapper::Float }
    #[classattr]
    fn Double() -> AzVertexAttributeTypeEnumWrapper { AzVertexAttributeTypeEnumWrapper::Double }
    #[classattr]
    fn UnsignedByte() -> AzVertexAttributeTypeEnumWrapper { AzVertexAttributeTypeEnumWrapper::UnsignedByte }
    #[classattr]
    fn UnsignedShort() -> AzVertexAttributeTypeEnumWrapper { AzVertexAttributeTypeEnumWrapper::UnsignedShort }
    #[classattr]
    fn UnsignedInt() -> AzVertexAttributeTypeEnumWrapper { AzVertexAttributeTypeEnumWrapper::UnsignedInt }
}

#[pymethods]
impl AzIndexBufferFormatEnumWrapper {
    #[classattr]
    fn Points() -> AzIndexBufferFormatEnumWrapper { AzIndexBufferFormatEnumWrapper::Points }
    #[classattr]
    fn Lines() -> AzIndexBufferFormatEnumWrapper { AzIndexBufferFormatEnumWrapper::Lines }
    #[classattr]
    fn LineStrip() -> AzIndexBufferFormatEnumWrapper { AzIndexBufferFormatEnumWrapper::LineStrip }
    #[classattr]
    fn Triangles() -> AzIndexBufferFormatEnumWrapper { AzIndexBufferFormatEnumWrapper::Triangles }
    #[classattr]
    fn TriangleStrip() -> AzIndexBufferFormatEnumWrapper { AzIndexBufferFormatEnumWrapper::TriangleStrip }
    #[classattr]
    fn TriangleFan() -> AzIndexBufferFormatEnumWrapper { AzIndexBufferFormatEnumWrapper::TriangleFan }
}

#[pymethods]
impl AzGlTypeEnumWrapper {
    #[classattr]
    fn Gl() -> AzGlTypeEnumWrapper { AzGlTypeEnumWrapper::Gl }
    #[classattr]
    fn Gles() -> AzGlTypeEnumWrapper { AzGlTypeEnumWrapper::Gles }
}

#[pymethods]
impl AzTextureFlags {
    #[staticmethod]
    fn default() -> AzTextureFlags {
        unsafe { mem::transmute(crate::AzTextureFlags_default()) }
    }
}

#[pymethods]
impl AzImageRef {
    #[staticmethod]
    fn invalid(width: usize, height: usize, format: AzRawImageFormatEnumWrapper) -> AzImageRef {
        unsafe { mem::transmute(crate::AzImageRef_invalid(
            mem::transmute(width),
            mem::transmute(height),
            mem::transmute(format),
        )) }
    }
    #[staticmethod]
    fn raw_image(data: AzRawImage) -> AzGLuintVec {
        unsafe { mem::transmute(crate::AzImageRef_rawImage(
            mem::transmute(data),
        )) }
    }
    #[staticmethod]
    fn gl_texture(texture: AzTexture) -> AzImageRef {
        unsafe { mem::transmute(crate::AzImageRef_glTexture(
            mem::transmute(texture),
        )) }
    }
    #[staticmethod]
    fn callback(callback: AzRenderImageCallback, data: AzRefAny) -> AzImageRef {
        unsafe { mem::transmute(crate::AzImageRef_callback(
            mem::transmute(callback),
            mem::transmute(data),
        )) }
    }
    fn clone_bytes(&self) -> AzImageRef {
        ()
    }
    fn is_invalid(&self) -> bool {
        ()
    }
    fn is_gl_texture(&self) -> bool {
        ()
    }
    fn is_raw_image(&self) -> bool {
        ()
    }
    fn is_callback(&self) -> bool {
        ()
    }
}

#[pymethods]
impl AzRawImage {
    #[staticmethod]
    fn empty() -> AzRawImage {
        unsafe { mem::transmute(crate::AzRawImage_empty()) }
    }
    #[staticmethod]
    fn allocate_clip_mask(size: AzLayoutSize) -> AzRawImage {
        unsafe { mem::transmute(crate::AzRawImage_allocateClipMask(
            mem::transmute(size),
        )) }
    }
    #[staticmethod]
    fn decode_image_bytes_any(bytes: AzU8VecRef) -> bool {
        unsafe { mem::transmute(crate::AzRawImage_decodeImageBytesAny(
            mem::transmute(bytes),
        )) }
    }
    fn draw_clip_mask(&mut self, node: AzSvgNodeEnumWrapper, style: AzSvgStyleEnumWrapper) -> bool {
        ()
    }
    fn encode_bmp(&self) -> Result<AzU8Vec, PyErr> {
        ()
    }
    fn encode_png(&self) -> Result<AzU8Vec, PyErr> {
        ()
    }
    fn encode_jpeg(&self) -> Result<AzU8Vec, PyErr> {
        ()
    }
    fn encode_tga(&self) -> Result<AzU8Vec, PyErr> {
        ()
    }
    fn encode_pnm(&self) -> Result<AzU8Vec, PyErr> {
        ()
    }
    fn encode_gif(&self) -> Result<AzU8Vec, PyErr> {
        ()
    }
    fn encode_tiff(&self) -> Result<AzU8Vec, PyErr> {
        ()
    }
}

#[pymethods]
impl AzRawImageFormatEnumWrapper {
    #[classattr]
    fn R8() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::R8 }
    #[classattr]
    fn R16() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::R16 }
    #[classattr]
    fn RG16() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::RG16 }
    #[classattr]
    fn BGRA8() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::BGRA8 }
    #[classattr]
    fn RGBAF32() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::RGBAF32 }
    #[classattr]
    fn RG8() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::RG8 }
    #[classattr]
    fn RGBAI32() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::RGBAI32 }
    #[classattr]
    fn RGBA8() -> AzRawImageFormatEnumWrapper { AzRawImageFormatEnumWrapper::RGBA8 }
}

#[pymethods]
impl AzEncodeImageErrorEnumWrapper {
    #[classattr]
    fn InsufficientMemory() -> AzEncodeImageErrorEnumWrapper { AzEncodeImageErrorEnumWrapper::InsufficientMemory }
    #[classattr]
    fn DimensionError() -> AzEncodeImageErrorEnumWrapper { AzEncodeImageErrorEnumWrapper::DimensionError }
    #[classattr]
    fn InvalidData() -> AzEncodeImageErrorEnumWrapper { AzEncodeImageErrorEnumWrapper::InvalidData }
    #[classattr]
    fn Unknown() -> AzEncodeImageErrorEnumWrapper { AzEncodeImageErrorEnumWrapper::Unknown }
}

#[pymethods]
impl AzDecodeImageErrorEnumWrapper {
    #[classattr]
    fn InsufficientMemory() -> AzDecodeImageErrorEnumWrapper { AzDecodeImageErrorEnumWrapper::InsufficientMemory }
    #[classattr]
    fn DimensionError() -> AzDecodeImageErrorEnumWrapper { AzDecodeImageErrorEnumWrapper::DimensionError }
    #[classattr]
    fn UnsupportedImageFormat() -> AzDecodeImageErrorEnumWrapper { AzDecodeImageErrorEnumWrapper::UnsupportedImageFormat }
    #[classattr]
    fn Unknown() -> AzDecodeImageErrorEnumWrapper { AzDecodeImageErrorEnumWrapper::Unknown }
}

#[pymethods]
impl AzRawImageDataEnumWrapper {
    #[staticmethod]
    fn U8(v: U8Vec) -> AzRawImageDataEnumWrapper { AzRawImageDataEnumWrapper::U8(v) }
    #[staticmethod]
    fn U16(v: U16Vec) -> AzRawImageDataEnumWrapper { AzRawImageDataEnumWrapper::U16(v) }
    #[staticmethod]
    fn F32(v: F32Vec) -> AzRawImageDataEnumWrapper { AzRawImageDataEnumWrapper::F32(v) }
}

#[pymethods]
impl AzFontRef {
    #[staticmethod]
    fn parse(source: AzFontSource) -> Result<AzU8Vec, PyErr> {
        let m: AzEncodeImageError = unsafe { mem::transmute(crate::AzFontRef_parse(
            mem::transmute(source),
        )) };
        match m {
            AzEncodeImageError::Ok(o) => Ok(o),
            AzEncodeImageError::Err(e) => Err(e.into()),
        }

    }
    fn get_font_metrics(&self) -> AzFontMetrics {
        ()
    }
}

#[pymethods]
impl AzSvg {
    #[staticmethod]
    fn from_string(svg_string: String, parse_options: AzSvgParseOptions) -> AzFontMetrics {
        let svg_string: AzString = svg_string.into();
unsafe { mem::transmute(crate::AzSvg_fromString(
            mem::transmute(svg_string),
            mem::transmute(parse_options),
        )) }
    }
    #[staticmethod]
    fn from_bytes(svg_bytes: AzU8VecRef, parse_options: AzSvgParseOptions) -> AzFontMetrics {
        unsafe { mem::transmute(crate::AzSvg_fromBytes(
            mem::transmute(svg_bytes),
            mem::transmute(parse_options),
        )) }
    }
    fn get_root(&self) -> AzSvgXmlNode {
        ()
    }
    fn render(&self, options: AzSvgRenderOptions) -> Option<AzRawImage> {
        ()
    }
    fn to_string(&self, options: AzSvgStringFormatOptions) -> String {
        ()
    }
}

#[pymethods]
impl AzSvgXmlNode {
    #[staticmethod]
    fn parse_from(svg_bytes: AzU8VecRef, parse_options: AzSvgParseOptions) -> String {
        unsafe { mem::transmute(crate::AzSvgXmlNode_parseFrom(
            mem::transmute(svg_bytes),
            mem::transmute(parse_options),
        )) }
    }
    fn render(&self, options: AzSvgRenderOptions) -> Option<AzRawImage> {
        ()
    }
    fn to_string(&self, options: AzSvgStringFormatOptions) -> String {
        ()
    }
}

#[pymethods]
impl AzSvgMultiPolygon {
    fn tesselate_fill(&self, fill_style: AzSvgFillStyle) -> AzTesselatedSvgNode {
        ()
    }
    fn tesselate_stroke(&self, stroke_style: AzSvgStrokeStyle) -> AzTesselatedSvgNode {
        ()
    }
}

#[pymethods]
impl AzSvgNodeEnumWrapper {
    #[staticmethod]
    fn MultiPolygonCollection(v: SvgMultiPolygonVec) -> AzSvgNodeEnumWrapper { AzSvgNodeEnumWrapper::MultiPolygonCollection(v) }
    #[staticmethod]
    fn MultiPolygon(v: SvgMultiPolygon) -> AzSvgNodeEnumWrapper { AzSvgNodeEnumWrapper::MultiPolygon(v) }
    #[staticmethod]
    fn Path(v: SvgPath) -> AzSvgNodeEnumWrapper { AzSvgNodeEnumWrapper::Path(v) }
    #[staticmethod]
    fn Circle(v: SvgCircle) -> AzSvgNodeEnumWrapper { AzSvgNodeEnumWrapper::Circle(v) }
    #[staticmethod]
    fn Rect(v: SvgRect) -> AzSvgNodeEnumWrapper { AzSvgNodeEnumWrapper::Rect(v) }
}

#[pymethods]
impl AzSvgStyledNode {
    fn tesselate(&self) -> AzTesselatedSvgNode {
        ()
    }
}

#[pymethods]
impl AzSvgCircle {
    fn tesselate_fill(&self, fill_style: AzSvgFillStyle) -> AzTesselatedSvgNode {
        ()
    }
    fn tesselate_stroke(&self, stroke_style: AzSvgStrokeStyle) -> AzTesselatedSvgNode {
        ()
    }
}

#[pymethods]
impl AzSvgPath {
    fn tesselate_fill(&self, fill_style: AzSvgFillStyle) -> AzTesselatedSvgNode {
        ()
    }
    fn tesselate_stroke(&self, stroke_style: AzSvgStrokeStyle) -> AzTesselatedSvgNode {
        ()
    }
}

#[pymethods]
impl AzSvgPathElementEnumWrapper {
    #[staticmethod]
    fn Line(v: SvgLine) -> AzSvgPathElementEnumWrapper { AzSvgPathElementEnumWrapper::Line(v) }
    #[staticmethod]
    fn QuadraticCurve(v: SvgQuadraticCurve) -> AzSvgPathElementEnumWrapper { AzSvgPathElementEnumWrapper::QuadraticCurve(v) }
    #[staticmethod]
    fn CubicCurve(v: SvgCubicCurve) -> AzSvgPathElementEnumWrapper { AzSvgPathElementEnumWrapper::CubicCurve(v) }
}

#[pymethods]
impl AzSvgRect {
    fn tesselate_fill(&self, fill_style: AzSvgFillStyle) -> AzTesselatedSvgNode {
        ()
    }
    fn tesselate_stroke(&self, stroke_style: AzSvgStrokeStyle) -> AzTesselatedSvgNode {
        ()
    }
}

#[pymethods]
impl AzTesselatedSvgNode {
    #[staticmethod]
    fn empty() -> AzTesselatedSvgNode {
        unsafe { mem::transmute(crate::AzTesselatedSvgNode_empty()) }
    }
    #[staticmethod]
    fn from_nodes(nodes: AzTesselatedSvgNodeVecRef) -> AzTesselatedSvgNode {
        unsafe { mem::transmute(crate::AzTesselatedSvgNode_fromNodes(
            mem::transmute(nodes),
        )) }
    }
}

#[pymethods]
impl AzSvgParseOptions {
    #[staticmethod]
    fn default() -> AzSvgParseOptions {
        unsafe { mem::transmute(crate::AzSvgParseOptions_default()) }
    }
}

#[pymethods]
impl AzShapeRenderingEnumWrapper {
    #[classattr]
    fn OptimizeSpeed() -> AzShapeRenderingEnumWrapper { AzShapeRenderingEnumWrapper::OptimizeSpeed }
    #[classattr]
    fn CrispEdges() -> AzShapeRenderingEnumWrapper { AzShapeRenderingEnumWrapper::CrispEdges }
    #[classattr]
    fn GeometricPrecision() -> AzShapeRenderingEnumWrapper { AzShapeRenderingEnumWrapper::GeometricPrecision }
}

#[pymethods]
impl AzTextRenderingEnumWrapper {
    #[classattr]
    fn OptimizeSpeed() -> AzTextRenderingEnumWrapper { AzTextRenderingEnumWrapper::OptimizeSpeed }
    #[classattr]
    fn OptimizeLegibility() -> AzTextRenderingEnumWrapper { AzTextRenderingEnumWrapper::OptimizeLegibility }
    #[classattr]
    fn GeometricPrecision() -> AzTextRenderingEnumWrapper { AzTextRenderingEnumWrapper::GeometricPrecision }
}

#[pymethods]
impl AzImageRenderingEnumWrapper {
    #[classattr]
    fn OptimizeQuality() -> AzImageRenderingEnumWrapper { AzImageRenderingEnumWrapper::OptimizeQuality }
    #[classattr]
    fn OptimizeSpeed() -> AzImageRenderingEnumWrapper { AzImageRenderingEnumWrapper::OptimizeSpeed }
}

#[pymethods]
impl AzFontDatabaseEnumWrapper {
    #[classattr]
    fn Empty() -> AzFontDatabaseEnumWrapper { AzFontDatabaseEnumWrapper::Empty }
    #[classattr]
    fn System() -> AzFontDatabaseEnumWrapper { AzFontDatabaseEnumWrapper::System }
}

#[pymethods]
impl AzSvgRenderOptions {
    #[staticmethod]
    fn default() -> AzSvgRenderOptions {
        unsafe { mem::transmute(crate::AzSvgRenderOptions_default()) }
    }
}

#[pymethods]
impl AzIndentEnumWrapper {
    #[classattr]
    fn None() -> AzIndentEnumWrapper { AzIndentEnumWrapper::None }
    #[staticmethod]
    fn Spaces(v: u8) -> AzIndentEnumWrapper { AzIndentEnumWrapper::Spaces(v) }
    #[classattr]
    fn Tabs() -> AzIndentEnumWrapper { AzIndentEnumWrapper::Tabs }
}

#[pymethods]
impl AzSvgFitToEnumWrapper {
    #[classattr]
    fn Original() -> AzSvgFitToEnumWrapper { AzSvgFitToEnumWrapper::Original }
    #[staticmethod]
    fn Width(v: u32) -> AzSvgFitToEnumWrapper { AzSvgFitToEnumWrapper::Width(v) }
    #[staticmethod]
    fn Height(v: u32) -> AzSvgFitToEnumWrapper { AzSvgFitToEnumWrapper::Height(v) }
    #[staticmethod]
    fn Zoom(v: f32) -> AzSvgFitToEnumWrapper { AzSvgFitToEnumWrapper::Zoom(v) }
}

#[pymethods]
impl AzSvgStyleEnumWrapper {
    #[staticmethod]
    fn Fill(v: SvgFillStyle) -> AzSvgStyleEnumWrapper { AzSvgStyleEnumWrapper::Fill(v) }
    #[staticmethod]
    fn Stroke(v: SvgStrokeStyle) -> AzSvgStyleEnumWrapper { AzSvgStyleEnumWrapper::Stroke(v) }
}

#[pymethods]
impl AzSvgFillRuleEnumWrapper {
    #[classattr]
    fn Winding() -> AzSvgFillRuleEnumWrapper { AzSvgFillRuleEnumWrapper::Winding }
    #[classattr]
    fn EvenOdd() -> AzSvgFillRuleEnumWrapper { AzSvgFillRuleEnumWrapper::EvenOdd }
}

#[pymethods]
impl AzSvgLineJoinEnumWrapper {
    #[classattr]
    fn Miter() -> AzSvgLineJoinEnumWrapper { AzSvgLineJoinEnumWrapper::Miter }
    #[classattr]
    fn MiterClip() -> AzSvgLineJoinEnumWrapper { AzSvgLineJoinEnumWrapper::MiterClip }
    #[classattr]
    fn Round() -> AzSvgLineJoinEnumWrapper { AzSvgLineJoinEnumWrapper::Round }
    #[classattr]
    fn Bevel() -> AzSvgLineJoinEnumWrapper { AzSvgLineJoinEnumWrapper::Bevel }
}

#[pymethods]
impl AzSvgLineCapEnumWrapper {
    #[classattr]
    fn Butt() -> AzSvgLineCapEnumWrapper { AzSvgLineCapEnumWrapper::Butt }
    #[classattr]
    fn Square() -> AzSvgLineCapEnumWrapper { AzSvgLineCapEnumWrapper::Square }
    #[classattr]
    fn Round() -> AzSvgLineCapEnumWrapper { AzSvgLineCapEnumWrapper::Round }
}

#[pymethods]
impl AzXml {
    #[staticmethod]
    fn from_str(xml_string: AzRefstr) -> AzTesselatedSvgNode {
        unsafe { mem::transmute(crate::AzXml_fromStr(
            mem::transmute(xml_string),
        )) }
    }
}

#[pymethods]
impl AzFile {
    #[staticmethod]
    fn open(path: String) -> AzTesselatedSvgNode {
        let path: AzString = path.into();
unsafe { mem::transmute(crate::AzFile_open(
            mem::transmute(path),
        )) }
    }
    #[staticmethod]
    fn create(path: String) -> AzTesselatedSvgNode {
        let path: AzString = path.into();
unsafe { mem::transmute(crate::AzFile_create(
            mem::transmute(path),
        )) }
    }
    fn read_to_string(&mut self) -> Option<AzString> {
        ()
    }
    fn read_to_bytes(&mut self) -> Option<AzU8Vec> {
        ()
    }
    fn write_string(&mut self, bytes: AzRefstr) -> bool {
        ()
    }
    fn write_bytes(&mut self, bytes: AzU8VecRef) -> bool {
        ()
    }
    fn close(self) -> () {
        ()
    }
}

#[pymethods]
impl AzMsgBox {
    #[staticmethod]
    fn ok(icon: AzMsgBoxIconEnumWrapper, title: String, message: String) -> bool {
        let title: AzString = title.into();
let message: AzString = message.into();
unsafe { mem::transmute(crate::AzMsgBox_ok(
            mem::transmute(icon),
            mem::transmute(title),
            mem::transmute(message),
        )) }
    }
    #[staticmethod]
    fn ok_cancel(icon: AzMsgBoxIconEnumWrapper, title: String, message: String, default_value: AzMsgBoxOkCancelEnumWrapper) -> bool {
        let title: AzString = title.into();
let message: AzString = message.into();
unsafe { mem::transmute(crate::AzMsgBox_okCancel(
            mem::transmute(icon),
            mem::transmute(title),
            mem::transmute(message),
            mem::transmute(default_value),
        )) }
    }
    #[staticmethod]
    fn yes_no(icon: AzMsgBoxIconEnumWrapper, title: String, message: String, default_value: AzMsgBoxYesNoEnumWrapper) -> bool {
        let title: AzString = title.into();
let message: AzString = message.into();
unsafe { mem::transmute(crate::AzMsgBox_yesNo(
            mem::transmute(icon),
            mem::transmute(title),
            mem::transmute(message),
            mem::transmute(default_value),
        )) }
    }
}

#[pymethods]
impl AzMsgBoxIconEnumWrapper {
    #[classattr]
    fn Info() -> AzMsgBoxIconEnumWrapper { AzMsgBoxIconEnumWrapper::Info }
    #[classattr]
    fn Warning() -> AzMsgBoxIconEnumWrapper { AzMsgBoxIconEnumWrapper::Warning }
    #[classattr]
    fn Error() -> AzMsgBoxIconEnumWrapper { AzMsgBoxIconEnumWrapper::Error }
    #[classattr]
    fn Question() -> AzMsgBoxIconEnumWrapper { AzMsgBoxIconEnumWrapper::Question }
}

#[pymethods]
impl AzMsgBoxYesNoEnumWrapper {
    #[classattr]
    fn Yes() -> AzMsgBoxYesNoEnumWrapper { AzMsgBoxYesNoEnumWrapper::Yes }
    #[classattr]
    fn No() -> AzMsgBoxYesNoEnumWrapper { AzMsgBoxYesNoEnumWrapper::No }
}

#[pymethods]
impl AzMsgBoxOkCancelEnumWrapper {
    #[classattr]
    fn Ok() -> AzMsgBoxOkCancelEnumWrapper { AzMsgBoxOkCancelEnumWrapper::Ok }
    #[classattr]
    fn Cancel() -> AzMsgBoxOkCancelEnumWrapper { AzMsgBoxOkCancelEnumWrapper::Cancel }
}

#[pymethods]
impl AzFileDialog {
    #[staticmethod]
    fn select_file(title: String, default_path: AzOptionStringEnumWrapper, filter_list: AzOptionFileTypeListEnumWrapper) -> bool {
        let title: AzString = title.into();
unsafe { mem::transmute(crate::AzFileDialog_selectFile(
            mem::transmute(title),
            mem::transmute(default_path),
            mem::transmute(filter_list),
        )) }
    }
    #[staticmethod]
    fn select_multiple_files(title: String, default_path: AzOptionStringEnumWrapper, filter_list: AzOptionFileTypeListEnumWrapper) -> bool {
        let title: AzString = title.into();
unsafe { mem::transmute(crate::AzFileDialog_selectMultipleFiles(
            mem::transmute(title),
            mem::transmute(default_path),
            mem::transmute(filter_list),
        )) }
    }
    #[staticmethod]
    fn select_folder(title: String, default_path: AzOptionStringEnumWrapper) -> bool {
        let title: AzString = title.into();
unsafe { mem::transmute(crate::AzFileDialog_selectFolder(
            mem::transmute(title),
            mem::transmute(default_path),
        )) }
    }
    #[staticmethod]
    fn save_file(title: String, default_path: AzOptionStringEnumWrapper) -> bool {
        let title: AzString = title.into();
unsafe { mem::transmute(crate::AzFileDialog_saveFile(
            mem::transmute(title),
            mem::transmute(default_path),
        )) }
    }
}

#[pymethods]
impl AzColorPickerDialog {
    #[staticmethod]
    fn open(title: String, default_color: AzOptionColorUEnumWrapper) -> bool {
        let title: AzString = title.into();
unsafe { mem::transmute(crate::AzColorPickerDialog_open(
            mem::transmute(title),
            mem::transmute(default_color),
        )) }
    }
}

#[pymethods]
impl AzSystemClipboard {
    #[staticmethod]
    fn new() -> bool {
        unsafe { mem::transmute(crate::AzSystemClipboard_new()) }
    }
    fn get_string_contents(&self) -> Option<AzString> {
        ()
    }
    fn set_string_contents(&mut self, contents: String) -> bool {
        ()
    }
}

#[pymethods]
impl AzInstantEnumWrapper {
    #[staticmethod]
    fn System(v: InstantPtr) -> AzInstantEnumWrapper { AzInstantEnumWrapper::System(v) }
    #[staticmethod]
    fn Tick(v: SystemTick) -> AzInstantEnumWrapper { AzInstantEnumWrapper::Tick(v) }
}

#[pymethods]
impl AzDurationEnumWrapper {
    #[staticmethod]
    fn System(v: SystemTimeDiff) -> AzDurationEnumWrapper { AzDurationEnumWrapper::System(v) }
    #[staticmethod]
    fn Tick(v: SystemTickDiff) -> AzDurationEnumWrapper { AzDurationEnumWrapper::Tick(v) }
}

#[pymethods]
impl AzTimer {
    fn with_delay(self, delay: AzDurationEnumWrapper) -> AzTimer {
        ()
    }
    fn with_interval(self, interval: AzDurationEnumWrapper) -> AzTimer {
        ()
    }
    fn with_timeout(self, timeout: AzDurationEnumWrapper) -> AzTimer {
        ()
    }
}

#[pymethods]
impl AzTerminateTimerEnumWrapper {
    #[classattr]
    fn Terminate() -> AzTerminateTimerEnumWrapper { AzTerminateTimerEnumWrapper::Terminate }
    #[classattr]
    fn Continue() -> AzTerminateTimerEnumWrapper { AzTerminateTimerEnumWrapper::Continue }
}

#[pymethods]
impl AzThreadSender {
    fn send(&mut self, msg: AzThreadReceiveMsgEnumWrapper) -> bool {
        ()
    }
}

#[pymethods]
impl AzThreadReceiver {
    fn receive(&mut self) -> Option<AzThreadSendMsg> {
        ()
    }
}

#[pymethods]
impl AzThreadSendMsgEnumWrapper {
    #[classattr]
    fn TerminateThread() -> AzThreadSendMsgEnumWrapper { AzThreadSendMsgEnumWrapper::TerminateThread }
    #[classattr]
    fn Tick() -> AzThreadSendMsgEnumWrapper { AzThreadSendMsgEnumWrapper::Tick }
    #[staticmethod]
    fn Custom(v: RefAny) -> AzThreadSendMsgEnumWrapper { AzThreadSendMsgEnumWrapper::Custom(v) }
}

#[pymethods]
impl AzThreadReceiveMsgEnumWrapper {
    #[staticmethod]
    fn WriteBack(v: ThreadWriteBackMsg) -> AzThreadReceiveMsgEnumWrapper { AzThreadReceiveMsgEnumWrapper::WriteBack(v) }
    #[staticmethod]
    fn Update(v: UpdateScreen) -> AzThreadReceiveMsgEnumWrapper { AzThreadReceiveMsgEnumWrapper::Update(v) }
}

#[pymethods]
impl AzFmtValueEnumWrapper {
    #[staticmethod]
    fn Bool(v: bool) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Bool(v) }
    #[staticmethod]
    fn Uchar(v: u8) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Uchar(v) }
    #[staticmethod]
    fn Schar(v: i8) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Schar(v) }
    #[staticmethod]
    fn Ushort(v: u16) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Ushort(v) }
    #[staticmethod]
    fn Sshort(v: i16) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Sshort(v) }
    #[staticmethod]
    fn Uint(v: u32) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Uint(v) }
    #[staticmethod]
    fn Sint(v: i32) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Sint(v) }
    #[staticmethod]
    fn Ulong(v: u64) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Ulong(v) }
    #[staticmethod]
    fn Slong(v: i64) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Slong(v) }
    #[staticmethod]
    fn Isize(v: isize) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Isize(v) }
    #[staticmethod]
    fn Usize(v: usize) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Usize(v) }
    #[staticmethod]
    fn Float(v: f32) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Float(v) }
    #[staticmethod]
    fn Double(v: f64) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Double(v) }
    #[staticmethod]
    fn Str(v: String) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::Str(v) }
    #[staticmethod]
    fn StrVec(v: StringVec) -> AzFmtValueEnumWrapper { AzFmtValueEnumWrapper::StrVec(v) }
}

#[pymethods]
impl AzString {
    #[staticmethod]
    fn format(format: String, args: AzFmtArgVec) -> AzString {
        let format: AzString = format.into();
unsafe { mem::transmute(crate::AzString_format(
            mem::transmute(format),
            mem::transmute(args),
        )) }
    }
    fn trim(&self) -> String {
        ()
    }
    fn as_refstr(&self) -> AzRefstr {
        ()
    }
}

#[pymethods]
impl AzTesselatedSvgNodeVec {
    fn as_ref_vec(&self) -> AzTesselatedSvgNodeVecRef {
        ()
    }
}

#[pymethods]
impl AzU8Vec {
    fn as_ref_vec(&self) -> AzU8VecRef {
        ()
    }
}

#[pymethods]
impl AzStyleFontFamilyVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStyleFontFamilyVecDestructorEnumWrapper { AzStyleFontFamilyVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStyleFontFamilyVecDestructorEnumWrapper { AzStyleFontFamilyVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StyleFontFamilyVecDestructorType) -> AzStyleFontFamilyVecDestructorEnumWrapper { AzStyleFontFamilyVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzTesselatedSvgNodeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzTesselatedSvgNodeVecDestructorEnumWrapper { AzTesselatedSvgNodeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzTesselatedSvgNodeVecDestructorEnumWrapper { AzTesselatedSvgNodeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: TesselatedSvgNodeVecDestructorType) -> AzTesselatedSvgNodeVecDestructorEnumWrapper { AzTesselatedSvgNodeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzXmlNodeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzXmlNodeVecDestructorEnumWrapper { AzXmlNodeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzXmlNodeVecDestructorEnumWrapper { AzXmlNodeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: XmlNodeVecDestructorType) -> AzXmlNodeVecDestructorEnumWrapper { AzXmlNodeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzFmtArgVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzFmtArgVecDestructorEnumWrapper { AzFmtArgVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzFmtArgVecDestructorEnumWrapper { AzFmtArgVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: FmtArgVecDestructorType) -> AzFmtArgVecDestructorEnumWrapper { AzFmtArgVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzInlineLineVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzInlineLineVecDestructorEnumWrapper { AzInlineLineVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzInlineLineVecDestructorEnumWrapper { AzInlineLineVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: InlineLineVecDestructorType) -> AzInlineLineVecDestructorEnumWrapper { AzInlineLineVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzInlineWordVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzInlineWordVecDestructorEnumWrapper { AzInlineWordVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzInlineWordVecDestructorEnumWrapper { AzInlineWordVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: InlineWordVecDestructorType) -> AzInlineWordVecDestructorEnumWrapper { AzInlineWordVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzInlineGlyphVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzInlineGlyphVecDestructorEnumWrapper { AzInlineGlyphVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzInlineGlyphVecDestructorEnumWrapper { AzInlineGlyphVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: InlineGlyphVecDestructorType) -> AzInlineGlyphVecDestructorEnumWrapper { AzInlineGlyphVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzInlineTextHitVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzInlineTextHitVecDestructorEnumWrapper { AzInlineTextHitVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzInlineTextHitVecDestructorEnumWrapper { AzInlineTextHitVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: InlineTextHitVecDestructorType) -> AzInlineTextHitVecDestructorEnumWrapper { AzInlineTextHitVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzMonitorVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzMonitorVecDestructorEnumWrapper { AzMonitorVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzMonitorVecDestructorEnumWrapper { AzMonitorVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: MonitorVecDestructorType) -> AzMonitorVecDestructorEnumWrapper { AzMonitorVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzVideoModeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzVideoModeVecDestructorEnumWrapper { AzVideoModeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzVideoModeVecDestructorEnumWrapper { AzVideoModeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: VideoModeVecDestructorType) -> AzVideoModeVecDestructorEnumWrapper { AzVideoModeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzDomVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzDomVecDestructorEnumWrapper { AzDomVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzDomVecDestructorEnumWrapper { AzDomVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: DomVecDestructorType) -> AzDomVecDestructorEnumWrapper { AzDomVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzIdOrClassVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzIdOrClassVecDestructorEnumWrapper { AzIdOrClassVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzIdOrClassVecDestructorEnumWrapper { AzIdOrClassVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: IdOrClassVecDestructorType) -> AzIdOrClassVecDestructorEnumWrapper { AzIdOrClassVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzNodeDataInlineCssPropertyVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzNodeDataInlineCssPropertyVecDestructorEnumWrapper { AzNodeDataInlineCssPropertyVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzNodeDataInlineCssPropertyVecDestructorEnumWrapper { AzNodeDataInlineCssPropertyVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: NodeDataInlineCssPropertyVecDestructorType) -> AzNodeDataInlineCssPropertyVecDestructorEnumWrapper { AzNodeDataInlineCssPropertyVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStyleBackgroundContentVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStyleBackgroundContentVecDestructorEnumWrapper { AzStyleBackgroundContentVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStyleBackgroundContentVecDestructorEnumWrapper { AzStyleBackgroundContentVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StyleBackgroundContentVecDestructorType) -> AzStyleBackgroundContentVecDestructorEnumWrapper { AzStyleBackgroundContentVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStyleBackgroundPositionVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStyleBackgroundPositionVecDestructorEnumWrapper { AzStyleBackgroundPositionVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStyleBackgroundPositionVecDestructorEnumWrapper { AzStyleBackgroundPositionVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StyleBackgroundPositionVecDestructorType) -> AzStyleBackgroundPositionVecDestructorEnumWrapper { AzStyleBackgroundPositionVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStyleBackgroundRepeatVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStyleBackgroundRepeatVecDestructorEnumWrapper { AzStyleBackgroundRepeatVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStyleBackgroundRepeatVecDestructorEnumWrapper { AzStyleBackgroundRepeatVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StyleBackgroundRepeatVecDestructorType) -> AzStyleBackgroundRepeatVecDestructorEnumWrapper { AzStyleBackgroundRepeatVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStyleBackgroundSizeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStyleBackgroundSizeVecDestructorEnumWrapper { AzStyleBackgroundSizeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStyleBackgroundSizeVecDestructorEnumWrapper { AzStyleBackgroundSizeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StyleBackgroundSizeVecDestructorType) -> AzStyleBackgroundSizeVecDestructorEnumWrapper { AzStyleBackgroundSizeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStyleTransformVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStyleTransformVecDestructorEnumWrapper { AzStyleTransformVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStyleTransformVecDestructorEnumWrapper { AzStyleTransformVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StyleTransformVecDestructorType) -> AzStyleTransformVecDestructorEnumWrapper { AzStyleTransformVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzCssPropertyVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzCssPropertyVecDestructorEnumWrapper { AzCssPropertyVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzCssPropertyVecDestructorEnumWrapper { AzCssPropertyVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: CssPropertyVecDestructorType) -> AzCssPropertyVecDestructorEnumWrapper { AzCssPropertyVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzSvgMultiPolygonVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzSvgMultiPolygonVecDestructorEnumWrapper { AzSvgMultiPolygonVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzSvgMultiPolygonVecDestructorEnumWrapper { AzSvgMultiPolygonVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: SvgMultiPolygonVecDestructorType) -> AzSvgMultiPolygonVecDestructorEnumWrapper { AzSvgMultiPolygonVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzSvgPathVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzSvgPathVecDestructorEnumWrapper { AzSvgPathVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzSvgPathVecDestructorEnumWrapper { AzSvgPathVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: SvgPathVecDestructorType) -> AzSvgPathVecDestructorEnumWrapper { AzSvgPathVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzVertexAttributeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzVertexAttributeVecDestructorEnumWrapper { AzVertexAttributeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzVertexAttributeVecDestructorEnumWrapper { AzVertexAttributeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: VertexAttributeVecDestructorType) -> AzVertexAttributeVecDestructorEnumWrapper { AzVertexAttributeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzSvgPathElementVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzSvgPathElementVecDestructorEnumWrapper { AzSvgPathElementVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzSvgPathElementVecDestructorEnumWrapper { AzSvgPathElementVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: SvgPathElementVecDestructorType) -> AzSvgPathElementVecDestructorEnumWrapper { AzSvgPathElementVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzSvgVertexVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzSvgVertexVecDestructorEnumWrapper { AzSvgVertexVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzSvgVertexVecDestructorEnumWrapper { AzSvgVertexVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: SvgVertexVecDestructorType) -> AzSvgVertexVecDestructorEnumWrapper { AzSvgVertexVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzU32VecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzU32VecDestructorEnumWrapper { AzU32VecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzU32VecDestructorEnumWrapper { AzU32VecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: U32VecDestructorType) -> AzU32VecDestructorEnumWrapper { AzU32VecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzXWindowTypeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzXWindowTypeVecDestructorEnumWrapper { AzXWindowTypeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzXWindowTypeVecDestructorEnumWrapper { AzXWindowTypeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: XWindowTypeVecDestructorType) -> AzXWindowTypeVecDestructorEnumWrapper { AzXWindowTypeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzVirtualKeyCodeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzVirtualKeyCodeVecDestructorEnumWrapper { AzVirtualKeyCodeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzVirtualKeyCodeVecDestructorEnumWrapper { AzVirtualKeyCodeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: VirtualKeyCodeVecDestructorType) -> AzVirtualKeyCodeVecDestructorEnumWrapper { AzVirtualKeyCodeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzCascadeInfoVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzCascadeInfoVecDestructorEnumWrapper { AzCascadeInfoVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzCascadeInfoVecDestructorEnumWrapper { AzCascadeInfoVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: CascadeInfoVecDestructorType) -> AzCascadeInfoVecDestructorEnumWrapper { AzCascadeInfoVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzScanCodeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzScanCodeVecDestructorEnumWrapper { AzScanCodeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzScanCodeVecDestructorEnumWrapper { AzScanCodeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: ScanCodeVecDestructorType) -> AzScanCodeVecDestructorEnumWrapper { AzScanCodeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzCssDeclarationVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzCssDeclarationVecDestructorEnumWrapper { AzCssDeclarationVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzCssDeclarationVecDestructorEnumWrapper { AzCssDeclarationVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: CssDeclarationVecDestructorType) -> AzCssDeclarationVecDestructorEnumWrapper { AzCssDeclarationVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzCssPathSelectorVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzCssPathSelectorVecDestructorEnumWrapper { AzCssPathSelectorVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzCssPathSelectorVecDestructorEnumWrapper { AzCssPathSelectorVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: CssPathSelectorVecDestructorType) -> AzCssPathSelectorVecDestructorEnumWrapper { AzCssPathSelectorVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStylesheetVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStylesheetVecDestructorEnumWrapper { AzStylesheetVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStylesheetVecDestructorEnumWrapper { AzStylesheetVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StylesheetVecDestructorType) -> AzStylesheetVecDestructorEnumWrapper { AzStylesheetVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzCssRuleBlockVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzCssRuleBlockVecDestructorEnumWrapper { AzCssRuleBlockVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzCssRuleBlockVecDestructorEnumWrapper { AzCssRuleBlockVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: CssRuleBlockVecDestructorType) -> AzCssRuleBlockVecDestructorEnumWrapper { AzCssRuleBlockVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzF32VecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzF32VecDestructorEnumWrapper { AzF32VecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzF32VecDestructorEnumWrapper { AzF32VecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: F32VecDestructorType) -> AzF32VecDestructorEnumWrapper { AzF32VecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzU16VecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzU16VecDestructorEnumWrapper { AzU16VecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzU16VecDestructorEnumWrapper { AzU16VecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: U16VecDestructorType) -> AzU16VecDestructorEnumWrapper { AzU16VecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzU8VecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzU8VecDestructorEnumWrapper { AzU8VecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzU8VecDestructorEnumWrapper { AzU8VecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: U8VecDestructorType) -> AzU8VecDestructorEnumWrapper { AzU8VecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzCallbackDataVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzCallbackDataVecDestructorEnumWrapper { AzCallbackDataVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzCallbackDataVecDestructorEnumWrapper { AzCallbackDataVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: CallbackDataVecDestructorType) -> AzCallbackDataVecDestructorEnumWrapper { AzCallbackDataVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzDebugMessageVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzDebugMessageVecDestructorEnumWrapper { AzDebugMessageVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzDebugMessageVecDestructorEnumWrapper { AzDebugMessageVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: DebugMessageVecDestructorType) -> AzDebugMessageVecDestructorEnumWrapper { AzDebugMessageVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzGLuintVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzGLuintVecDestructorEnumWrapper { AzGLuintVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzGLuintVecDestructorEnumWrapper { AzGLuintVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: GLuintVecDestructorType) -> AzGLuintVecDestructorEnumWrapper { AzGLuintVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzGLintVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzGLintVecDestructorEnumWrapper { AzGLintVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzGLintVecDestructorEnumWrapper { AzGLintVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: GLintVecDestructorType) -> AzGLintVecDestructorEnumWrapper { AzGLintVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStringVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStringVecDestructorEnumWrapper { AzStringVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStringVecDestructorEnumWrapper { AzStringVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StringVecDestructorType) -> AzStringVecDestructorEnumWrapper { AzStringVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStringPairVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStringPairVecDestructorEnumWrapper { AzStringPairVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStringPairVecDestructorEnumWrapper { AzStringPairVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StringPairVecDestructorType) -> AzStringPairVecDestructorEnumWrapper { AzStringPairVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzNormalizedLinearColorStopVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzNormalizedLinearColorStopVecDestructorEnumWrapper { AzNormalizedLinearColorStopVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzNormalizedLinearColorStopVecDestructorEnumWrapper { AzNormalizedLinearColorStopVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: NormalizedLinearColorStopVecDestructorType) -> AzNormalizedLinearColorStopVecDestructorEnumWrapper { AzNormalizedLinearColorStopVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzNormalizedRadialColorStopVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzNormalizedRadialColorStopVecDestructorEnumWrapper { AzNormalizedRadialColorStopVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzNormalizedRadialColorStopVecDestructorEnumWrapper { AzNormalizedRadialColorStopVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: NormalizedRadialColorStopVecDestructorType) -> AzNormalizedRadialColorStopVecDestructorEnumWrapper { AzNormalizedRadialColorStopVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzNodeIdVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzNodeIdVecDestructorEnumWrapper { AzNodeIdVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzNodeIdVecDestructorEnumWrapper { AzNodeIdVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: NodeIdVecDestructorType) -> AzNodeIdVecDestructorEnumWrapper { AzNodeIdVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzNodeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzNodeVecDestructorEnumWrapper { AzNodeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzNodeVecDestructorEnumWrapper { AzNodeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: NodeVecDestructorType) -> AzNodeVecDestructorEnumWrapper { AzNodeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzStyledNodeVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzStyledNodeVecDestructorEnumWrapper { AzStyledNodeVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzStyledNodeVecDestructorEnumWrapper { AzStyledNodeVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: StyledNodeVecDestructorType) -> AzStyledNodeVecDestructorEnumWrapper { AzStyledNodeVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper { AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper { AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: TagIdsToNodeIdsMappingVecDestructorType) -> AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper { AzTagIdsToNodeIdsMappingVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzParentWithNodeDepthVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzParentWithNodeDepthVecDestructorEnumWrapper { AzParentWithNodeDepthVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzParentWithNodeDepthVecDestructorEnumWrapper { AzParentWithNodeDepthVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: ParentWithNodeDepthVecDestructorType) -> AzParentWithNodeDepthVecDestructorEnumWrapper { AzParentWithNodeDepthVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzNodeDataVecDestructorEnumWrapper {
    #[classattr]
    fn DefaultRust() -> AzNodeDataVecDestructorEnumWrapper { AzNodeDataVecDestructorEnumWrapper::DefaultRust }
    #[classattr]
    fn NoDestructor() -> AzNodeDataVecDestructorEnumWrapper { AzNodeDataVecDestructorEnumWrapper::NoDestructor }
    #[staticmethod]
    fn External(v: NodeDataVecDestructorType) -> AzNodeDataVecDestructorEnumWrapper { AzNodeDataVecDestructorEnumWrapper::External(v) }
}

#[pymethods]
impl AzOptionCssPropertyEnumWrapper {
    #[classattr]
    fn None() -> AzOptionCssPropertyEnumWrapper { AzOptionCssPropertyEnumWrapper::None }
    #[staticmethod]
    fn Some(v: CssProperty) -> AzOptionCssPropertyEnumWrapper { AzOptionCssPropertyEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionPositionInfoEnumWrapper {
    #[classattr]
    fn None() -> AzOptionPositionInfoEnumWrapper { AzOptionPositionInfoEnumWrapper::None }
    #[staticmethod]
    fn Some(v: PositionInfo) -> AzOptionPositionInfoEnumWrapper { AzOptionPositionInfoEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionTimerIdEnumWrapper {
    #[classattr]
    fn None() -> AzOptionTimerIdEnumWrapper { AzOptionTimerIdEnumWrapper::None }
    #[staticmethod]
    fn Some(v: TimerId) -> AzOptionTimerIdEnumWrapper { AzOptionTimerIdEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionThreadIdEnumWrapper {
    #[classattr]
    fn None() -> AzOptionThreadIdEnumWrapper { AzOptionThreadIdEnumWrapper::None }
    #[staticmethod]
    fn Some(v: ThreadId) -> AzOptionThreadIdEnumWrapper { AzOptionThreadIdEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionI16EnumWrapper {
    #[classattr]
    fn None() -> AzOptionI16EnumWrapper { AzOptionI16EnumWrapper::None }
    #[staticmethod]
    fn Some(v: i16) -> AzOptionI16EnumWrapper { AzOptionI16EnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionU16EnumWrapper {
    #[classattr]
    fn None() -> AzOptionU16EnumWrapper { AzOptionU16EnumWrapper::None }
    #[staticmethod]
    fn Some(v: u16) -> AzOptionU16EnumWrapper { AzOptionU16EnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionU32EnumWrapper {
    #[classattr]
    fn None() -> AzOptionU32EnumWrapper { AzOptionU32EnumWrapper::None }
    #[staticmethod]
    fn Some(v: u32) -> AzOptionU32EnumWrapper { AzOptionU32EnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionImageRefEnumWrapper {
    #[classattr]
    fn None() -> AzOptionImageRefEnumWrapper { AzOptionImageRefEnumWrapper::None }
    #[staticmethod]
    fn Some(v: ImageRef) -> AzOptionImageRefEnumWrapper { AzOptionImageRefEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionFontRefEnumWrapper {
    #[classattr]
    fn None() -> AzOptionFontRefEnumWrapper { AzOptionFontRefEnumWrapper::None }
    #[staticmethod]
    fn Some(v: FontRef) -> AzOptionFontRefEnumWrapper { AzOptionFontRefEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionSystemClipboardEnumWrapper {
    #[classattr]
    fn None() -> AzOptionSystemClipboardEnumWrapper { AzOptionSystemClipboardEnumWrapper::None }
    #[staticmethod]
    fn Some(v: SystemClipboard) -> AzOptionSystemClipboardEnumWrapper { AzOptionSystemClipboardEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionFileTypeListEnumWrapper {
    #[classattr]
    fn None() -> AzOptionFileTypeListEnumWrapper { AzOptionFileTypeListEnumWrapper::None }
    #[staticmethod]
    fn Some(v: FileTypeList) -> AzOptionFileTypeListEnumWrapper { AzOptionFileTypeListEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionWindowStateEnumWrapper {
    #[classattr]
    fn None() -> AzOptionWindowStateEnumWrapper { AzOptionWindowStateEnumWrapper::None }
    #[staticmethod]
    fn Some(v: WindowState) -> AzOptionWindowStateEnumWrapper { AzOptionWindowStateEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionMouseStateEnumWrapper {
    #[classattr]
    fn None() -> AzOptionMouseStateEnumWrapper { AzOptionMouseStateEnumWrapper::None }
    #[staticmethod]
    fn Some(v: MouseState) -> AzOptionMouseStateEnumWrapper { AzOptionMouseStateEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionKeyboardStateEnumWrapper {
    #[classattr]
    fn None() -> AzOptionKeyboardStateEnumWrapper { AzOptionKeyboardStateEnumWrapper::None }
    #[staticmethod]
    fn Some(v: KeyboardState) -> AzOptionKeyboardStateEnumWrapper { AzOptionKeyboardStateEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionStringVecEnumWrapper {
    #[classattr]
    fn None() -> AzOptionStringVecEnumWrapper { AzOptionStringVecEnumWrapper::None }
    #[staticmethod]
    fn Some(v: StringVec) -> AzOptionStringVecEnumWrapper { AzOptionStringVecEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionFileEnumWrapper {
    #[classattr]
    fn None() -> AzOptionFileEnumWrapper { AzOptionFileEnumWrapper::None }
    #[staticmethod]
    fn Some(v: File) -> AzOptionFileEnumWrapper { AzOptionFileEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionGlEnumWrapper {
    #[classattr]
    fn None() -> AzOptionGlEnumWrapper { AzOptionGlEnumWrapper::None }
    #[staticmethod]
    fn Some(v: Gl) -> AzOptionGlEnumWrapper { AzOptionGlEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionThreadReceiveMsgEnumWrapper {
    #[classattr]
    fn None() -> AzOptionThreadReceiveMsgEnumWrapper { AzOptionThreadReceiveMsgEnumWrapper::None }
    #[staticmethod]
    fn Some(v: ThreadReceiveMsg) -> AzOptionThreadReceiveMsgEnumWrapper { AzOptionThreadReceiveMsgEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionPercentageValueEnumWrapper {
    #[classattr]
    fn None() -> AzOptionPercentageValueEnumWrapper { AzOptionPercentageValueEnumWrapper::None }
    #[staticmethod]
    fn Some(v: PercentageValue) -> AzOptionPercentageValueEnumWrapper { AzOptionPercentageValueEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionAngleValueEnumWrapper {
    #[classattr]
    fn None() -> AzOptionAngleValueEnumWrapper { AzOptionAngleValueEnumWrapper::None }
    #[staticmethod]
    fn Some(v: AngleValue) -> AzOptionAngleValueEnumWrapper { AzOptionAngleValueEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionRendererOptionsEnumWrapper {
    #[classattr]
    fn None() -> AzOptionRendererOptionsEnumWrapper { AzOptionRendererOptionsEnumWrapper::None }
    #[staticmethod]
    fn Some(v: RendererOptions) -> AzOptionRendererOptionsEnumWrapper { AzOptionRendererOptionsEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionCallbackEnumWrapper {
    #[classattr]
    fn None() -> AzOptionCallbackEnumWrapper { AzOptionCallbackEnumWrapper::None }
    #[staticmethod]
    fn Some(v: Callback) -> AzOptionCallbackEnumWrapper { AzOptionCallbackEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionThreadSendMsgEnumWrapper {
    #[classattr]
    fn None() -> AzOptionThreadSendMsgEnumWrapper { AzOptionThreadSendMsgEnumWrapper::None }
    #[staticmethod]
    fn Some(v: ThreadSendMsg) -> AzOptionThreadSendMsgEnumWrapper { AzOptionThreadSendMsgEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionLayoutRectEnumWrapper {
    #[classattr]
    fn None() -> AzOptionLayoutRectEnumWrapper { AzOptionLayoutRectEnumWrapper::None }
    #[staticmethod]
    fn Some(v: LayoutRect) -> AzOptionLayoutRectEnumWrapper { AzOptionLayoutRectEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionRefAnyEnumWrapper {
    #[classattr]
    fn None() -> AzOptionRefAnyEnumWrapper { AzOptionRefAnyEnumWrapper::None }
    #[staticmethod]
    fn Some(v: RefAny) -> AzOptionRefAnyEnumWrapper { AzOptionRefAnyEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionInlineTextEnumWrapper {
    #[classattr]
    fn None() -> AzOptionInlineTextEnumWrapper { AzOptionInlineTextEnumWrapper::None }
    #[staticmethod]
    fn Some(v: InlineText) -> AzOptionInlineTextEnumWrapper { AzOptionInlineTextEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionLayoutPointEnumWrapper {
    #[classattr]
    fn None() -> AzOptionLayoutPointEnumWrapper { AzOptionLayoutPointEnumWrapper::None }
    #[staticmethod]
    fn Some(v: LayoutPoint) -> AzOptionLayoutPointEnumWrapper { AzOptionLayoutPointEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionLayoutSizeEnumWrapper {
    #[classattr]
    fn None() -> AzOptionLayoutSizeEnumWrapper { AzOptionLayoutSizeEnumWrapper::None }
    #[staticmethod]
    fn Some(v: LayoutSize) -> AzOptionLayoutSizeEnumWrapper { AzOptionLayoutSizeEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionWindowThemeEnumWrapper {
    #[classattr]
    fn None() -> AzOptionWindowThemeEnumWrapper { AzOptionWindowThemeEnumWrapper::None }
    #[staticmethod]
    fn Some(v: WindowTheme) -> AzOptionWindowThemeEnumWrapper { AzOptionWindowThemeEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionNodeIdEnumWrapper {
    #[classattr]
    fn None() -> AzOptionNodeIdEnumWrapper { AzOptionNodeIdEnumWrapper::None }
    #[staticmethod]
    fn Some(v: NodeId) -> AzOptionNodeIdEnumWrapper { AzOptionNodeIdEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionDomNodeIdEnumWrapper {
    #[classattr]
    fn None() -> AzOptionDomNodeIdEnumWrapper { AzOptionDomNodeIdEnumWrapper::None }
    #[staticmethod]
    fn Some(v: DomNodeId) -> AzOptionDomNodeIdEnumWrapper { AzOptionDomNodeIdEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionColorUEnumWrapper {
    #[classattr]
    fn None() -> AzOptionColorUEnumWrapper { AzOptionColorUEnumWrapper::None }
    #[staticmethod]
    fn Some(v: ColorU) -> AzOptionColorUEnumWrapper { AzOptionColorUEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionRawImageEnumWrapper {
    #[classattr]
    fn None() -> AzOptionRawImageEnumWrapper { AzOptionRawImageEnumWrapper::None }
    #[staticmethod]
    fn Some(v: RawImage) -> AzOptionRawImageEnumWrapper { AzOptionRawImageEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionSvgDashPatternEnumWrapper {
    #[classattr]
    fn None() -> AzOptionSvgDashPatternEnumWrapper { AzOptionSvgDashPatternEnumWrapper::None }
    #[staticmethod]
    fn Some(v: SvgDashPattern) -> AzOptionSvgDashPatternEnumWrapper { AzOptionSvgDashPatternEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionWaylandThemeEnumWrapper {
    #[classattr]
    fn None() -> AzOptionWaylandThemeEnumWrapper { AzOptionWaylandThemeEnumWrapper::None }
    #[staticmethod]
    fn Some(v: WaylandTheme) -> AzOptionWaylandThemeEnumWrapper { AzOptionWaylandThemeEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionTaskBarIconEnumWrapper {
    #[classattr]
    fn None() -> AzOptionTaskBarIconEnumWrapper { AzOptionTaskBarIconEnumWrapper::None }
    #[staticmethod]
    fn Some(v: TaskBarIcon) -> AzOptionTaskBarIconEnumWrapper { AzOptionTaskBarIconEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionHwndHandleEnumWrapper {
    #[classattr]
    fn None() -> AzOptionHwndHandleEnumWrapper { AzOptionHwndHandleEnumWrapper::None }
    #[staticmethod]
    fn Some(v: *mut c_void) -> AzOptionHwndHandleEnumWrapper { AzOptionHwndHandleEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionLogicalPositionEnumWrapper {
    #[classattr]
    fn None() -> AzOptionLogicalPositionEnumWrapper { AzOptionLogicalPositionEnumWrapper::None }
    #[staticmethod]
    fn Some(v: LogicalPosition) -> AzOptionLogicalPositionEnumWrapper { AzOptionLogicalPositionEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionPhysicalPositionI32EnumWrapper {
    #[classattr]
    fn None() -> AzOptionPhysicalPositionI32EnumWrapper { AzOptionPhysicalPositionI32EnumWrapper::None }
    #[staticmethod]
    fn Some(v: PhysicalPositionI32) -> AzOptionPhysicalPositionI32EnumWrapper { AzOptionPhysicalPositionI32EnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionWindowIconEnumWrapper {
    #[classattr]
    fn None() -> AzOptionWindowIconEnumWrapper { AzOptionWindowIconEnumWrapper::None }
    #[staticmethod]
    fn Some(v: WindowIcon) -> AzOptionWindowIconEnumWrapper { AzOptionWindowIconEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionStringEnumWrapper {
    #[classattr]
    fn None() -> AzOptionStringEnumWrapper { AzOptionStringEnumWrapper::None }
    #[staticmethod]
    fn Some(v: String) -> AzOptionStringEnumWrapper { AzOptionStringEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionX11VisualEnumWrapper {
    #[classattr]
    fn None() -> AzOptionX11VisualEnumWrapper { AzOptionX11VisualEnumWrapper::None }
    #[staticmethod]
    fn Some(v: *const c_void) -> AzOptionX11VisualEnumWrapper { AzOptionX11VisualEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionI32EnumWrapper {
    #[classattr]
    fn None() -> AzOptionI32EnumWrapper { AzOptionI32EnumWrapper::None }
    #[staticmethod]
    fn Some(v: i32) -> AzOptionI32EnumWrapper { AzOptionI32EnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionF32EnumWrapper {
    #[classattr]
    fn None() -> AzOptionF32EnumWrapper { AzOptionF32EnumWrapper::None }
    #[staticmethod]
    fn Some(v: f32) -> AzOptionF32EnumWrapper { AzOptionF32EnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionMouseCursorTypeEnumWrapper {
    #[classattr]
    fn None() -> AzOptionMouseCursorTypeEnumWrapper { AzOptionMouseCursorTypeEnumWrapper::None }
    #[staticmethod]
    fn Some(v: MouseCursorType) -> AzOptionMouseCursorTypeEnumWrapper { AzOptionMouseCursorTypeEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionLogicalSizeEnumWrapper {
    #[classattr]
    fn None() -> AzOptionLogicalSizeEnumWrapper { AzOptionLogicalSizeEnumWrapper::None }
    #[staticmethod]
    fn Some(v: LogicalSize) -> AzOptionLogicalSizeEnumWrapper { AzOptionLogicalSizeEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionCharEnumWrapper {
    #[classattr]
    fn None() -> AzOptionCharEnumWrapper { AzOptionCharEnumWrapper::None }
    #[staticmethod]
    fn Some(v: u32) -> AzOptionCharEnumWrapper { AzOptionCharEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionVirtualKeyCodeEnumWrapper {
    #[classattr]
    fn None() -> AzOptionVirtualKeyCodeEnumWrapper { AzOptionVirtualKeyCodeEnumWrapper::None }
    #[staticmethod]
    fn Some(v: VirtualKeyCode) -> AzOptionVirtualKeyCodeEnumWrapper { AzOptionVirtualKeyCodeEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionDomEnumWrapper {
    #[classattr]
    fn None() -> AzOptionDomEnumWrapper { AzOptionDomEnumWrapper::None }
    #[staticmethod]
    fn Some(v: Dom) -> AzOptionDomEnumWrapper { AzOptionDomEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionTextureEnumWrapper {
    #[classattr]
    fn None() -> AzOptionTextureEnumWrapper { AzOptionTextureEnumWrapper::None }
    #[staticmethod]
    fn Some(v: Texture) -> AzOptionTextureEnumWrapper { AzOptionTextureEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionImageMaskEnumWrapper {
    #[classattr]
    fn None() -> AzOptionImageMaskEnumWrapper { AzOptionImageMaskEnumWrapper::None }
    #[staticmethod]
    fn Some(v: ImageMask) -> AzOptionImageMaskEnumWrapper { AzOptionImageMaskEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionTabIndexEnumWrapper {
    #[classattr]
    fn None() -> AzOptionTabIndexEnumWrapper { AzOptionTabIndexEnumWrapper::None }
    #[staticmethod]
    fn Some(v: TabIndex) -> AzOptionTabIndexEnumWrapper { AzOptionTabIndexEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionTagIdEnumWrapper {
    #[classattr]
    fn None() -> AzOptionTagIdEnumWrapper { AzOptionTagIdEnumWrapper::None }
    #[staticmethod]
    fn Some(v: TagId) -> AzOptionTagIdEnumWrapper { AzOptionTagIdEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionDurationEnumWrapper {
    #[classattr]
    fn None() -> AzOptionDurationEnumWrapper { AzOptionDurationEnumWrapper::None }
    #[staticmethod]
    fn Some(v: Duration) -> AzOptionDurationEnumWrapper { AzOptionDurationEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionInstantEnumWrapper {
    #[classattr]
    fn None() -> AzOptionInstantEnumWrapper { AzOptionInstantEnumWrapper::None }
    #[staticmethod]
    fn Some(v: Instant) -> AzOptionInstantEnumWrapper { AzOptionInstantEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionUsizeEnumWrapper {
    #[classattr]
    fn None() -> AzOptionUsizeEnumWrapper { AzOptionUsizeEnumWrapper::None }
    #[staticmethod]
    fn Some(v: usize) -> AzOptionUsizeEnumWrapper { AzOptionUsizeEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionU8VecEnumWrapper {
    #[classattr]
    fn None() -> AzOptionU8VecEnumWrapper { AzOptionU8VecEnumWrapper::None }
    #[staticmethod]
    fn Some(v: U8Vec) -> AzOptionU8VecEnumWrapper { AzOptionU8VecEnumWrapper::Some(v) }
}

#[pymethods]
impl AzOptionU8VecRefEnumWrapper {
    #[classattr]
    fn None() -> AzOptionU8VecRefEnumWrapper { AzOptionU8VecRefEnumWrapper::None }
    #[staticmethod]
    fn Some(v: U8VecRef) -> AzOptionU8VecRefEnumWrapper { AzOptionU8VecRefEnumWrapper::Some(v) }
}

#[pymethods]
impl AzResultXmlXmlErrorEnumWrapper {
    #[staticmethod]
    fn Ok(v: Xml) -> AzResultXmlXmlErrorEnumWrapper { AzResultXmlXmlErrorEnumWrapper::Ok(v) }
    #[staticmethod]
    fn Err(v: XmlError) -> AzResultXmlXmlErrorEnumWrapper { AzResultXmlXmlErrorEnumWrapper::Err(v) }
}

#[pymethods]
impl AzResultRawImageDecodeImageErrorEnumWrapper {
    #[staticmethod]
    fn Ok(v: RawImage) -> AzResultRawImageDecodeImageErrorEnumWrapper { AzResultRawImageDecodeImageErrorEnumWrapper::Ok(v) }
    #[staticmethod]
    fn Err(v: DecodeImageError) -> AzResultRawImageDecodeImageErrorEnumWrapper { AzResultRawImageDecodeImageErrorEnumWrapper::Err(v) }
}

#[pymethods]
impl AzResultU8VecEncodeImageErrorEnumWrapper {
    #[staticmethod]
    fn Ok(v: U8Vec) -> AzResultU8VecEncodeImageErrorEnumWrapper { AzResultU8VecEncodeImageErrorEnumWrapper::Ok(v) }
    #[staticmethod]
    fn Err(v: EncodeImageError) -> AzResultU8VecEncodeImageErrorEnumWrapper { AzResultU8VecEncodeImageErrorEnumWrapper::Err(v) }
}

#[pymethods]
impl AzResultSvgXmlNodeSvgParseErrorEnumWrapper {
    #[staticmethod]
    fn Ok(v: SvgXmlNode) -> AzResultSvgXmlNodeSvgParseErrorEnumWrapper { AzResultSvgXmlNodeSvgParseErrorEnumWrapper::Ok(v) }
    #[staticmethod]
    fn Err(v: SvgParseError) -> AzResultSvgXmlNodeSvgParseErrorEnumWrapper { AzResultSvgXmlNodeSvgParseErrorEnumWrapper::Err(v) }
}

#[pymethods]
impl AzResultSvgSvgParseErrorEnumWrapper {
    #[staticmethod]
    fn Ok(v: Svg) -> AzResultSvgSvgParseErrorEnumWrapper { AzResultSvgSvgParseErrorEnumWrapper::Ok(v) }
    #[staticmethod]
    fn Err(v: SvgParseError) -> AzResultSvgSvgParseErrorEnumWrapper { AzResultSvgSvgParseErrorEnumWrapper::Err(v) }
}

#[pymethods]
impl AzSvgParseErrorEnumWrapper {
    #[classattr]
    fn InvalidFileSuffix() -> AzSvgParseErrorEnumWrapper { AzSvgParseErrorEnumWrapper::InvalidFileSuffix }
    #[classattr]
    fn FileOpenFailed() -> AzSvgParseErrorEnumWrapper { AzSvgParseErrorEnumWrapper::FileOpenFailed }
    #[classattr]
    fn NotAnUtf8Str() -> AzSvgParseErrorEnumWrapper { AzSvgParseErrorEnumWrapper::NotAnUtf8Str }
    #[classattr]
    fn MalformedGZip() -> AzSvgParseErrorEnumWrapper { AzSvgParseErrorEnumWrapper::MalformedGZip }
    #[classattr]
    fn InvalidSize() -> AzSvgParseErrorEnumWrapper { AzSvgParseErrorEnumWrapper::InvalidSize }
    #[staticmethod]
    fn ParsingFailed(v: XmlError) -> AzSvgParseErrorEnumWrapper { AzSvgParseErrorEnumWrapper::ParsingFailed(v) }
}

#[pymethods]
impl AzXmlErrorEnumWrapper {
    #[staticmethod]
    fn InvalidXmlPrefixUri(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::InvalidXmlPrefixUri(v) }
    #[staticmethod]
    fn UnexpectedXmlUri(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::UnexpectedXmlUri(v) }
    #[staticmethod]
    fn UnexpectedXmlnsUri(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::UnexpectedXmlnsUri(v) }
    #[staticmethod]
    fn InvalidElementNamePrefix(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::InvalidElementNamePrefix(v) }
    #[staticmethod]
    fn DuplicatedNamespace(v: DuplicatedNamespaceError) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::DuplicatedNamespace(v) }
    #[staticmethod]
    fn UnknownNamespace(v: UnknownNamespaceError) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::UnknownNamespace(v) }
    #[staticmethod]
    fn UnexpectedCloseTag(v: UnexpectedCloseTagError) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::UnexpectedCloseTag(v) }
    #[staticmethod]
    fn UnexpectedEntityCloseTag(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::UnexpectedEntityCloseTag(v) }
    #[staticmethod]
    fn UnknownEntityReference(v: UnknownEntityReferenceError) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::UnknownEntityReference(v) }
    #[staticmethod]
    fn MalformedEntityReference(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::MalformedEntityReference(v) }
    #[staticmethod]
    fn EntityReferenceLoop(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::EntityReferenceLoop(v) }
    #[staticmethod]
    fn InvalidAttributeValue(v: SvgParseErrorPosition) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::InvalidAttributeValue(v) }
    #[staticmethod]
    fn DuplicatedAttribute(v: DuplicatedAttributeError) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::DuplicatedAttribute(v) }
    #[classattr]
    fn NoRootNode() -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::NoRootNode }
    #[classattr]
    fn SizeLimit() -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::SizeLimit }
    #[staticmethod]
    fn ParserError(v: XmlParseError) -> AzXmlErrorEnumWrapper { AzXmlErrorEnumWrapper::ParserError(v) }
}

#[pymethods]
impl AzXmlParseErrorEnumWrapper {
    #[staticmethod]
    fn InvalidDeclaration(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidDeclaration(v) }
    #[staticmethod]
    fn InvalidComment(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidComment(v) }
    #[staticmethod]
    fn InvalidPI(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidPI(v) }
    #[staticmethod]
    fn InvalidDoctype(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidDoctype(v) }
    #[staticmethod]
    fn InvalidEntity(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidEntity(v) }
    #[staticmethod]
    fn InvalidElement(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidElement(v) }
    #[staticmethod]
    fn InvalidAttribute(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidAttribute(v) }
    #[staticmethod]
    fn InvalidCdata(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidCdata(v) }
    #[staticmethod]
    fn InvalidCharData(v: XmlTextError) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::InvalidCharData(v) }
    #[staticmethod]
    fn UnknownToken(v: SvgParseErrorPosition) -> AzXmlParseErrorEnumWrapper { AzXmlParseErrorEnumWrapper::UnknownToken(v) }
}

#[pymethods]
impl AzXmlStreamErrorEnumWrapper {
    #[classattr]
    fn UnexpectedEndOfStream() -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::UnexpectedEndOfStream }
    #[classattr]
    fn InvalidName() -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidName }
    #[staticmethod]
    fn NonXmlChar(v: NonXmlCharError) -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::NonXmlChar(v) }
    #[staticmethod]
    fn InvalidChar(v: InvalidCharError) -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidChar(v) }
    #[staticmethod]
    fn InvalidCharMultiple(v: InvalidCharMultipleError) -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidCharMultiple(v) }
    #[staticmethod]
    fn InvalidQuote(v: InvalidQuoteError) -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidQuote(v) }
    #[staticmethod]
    fn InvalidSpace(v: InvalidSpaceError) -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidSpace(v) }
    #[staticmethod]
    fn InvalidString(v: InvalidStringError) -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidString(v) }
    #[classattr]
    fn InvalidReference() -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidReference }
    #[classattr]
    fn InvalidExternalID() -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidExternalID }
    #[classattr]
    fn InvalidCommentData() -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidCommentData }
    #[classattr]
    fn InvalidCommentEnd() -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidCommentEnd }
    #[classattr]
    fn InvalidCharacterData() -> AzXmlStreamErrorEnumWrapper { AzXmlStreamErrorEnumWrapper::InvalidCharacterData }
}


impl core::convert::From<AzDecodeImageError> for PyErr {
    fn from(err: AzDecodeImageError) -> PyErr {
        PyOSError::new_err(format!("{}", err))
    }
}

impl core::convert::From<AzEncodeImageError> for PyErr {
    fn from(err: AzEncodeImageError) -> PyErr {
        PyOSError::new_err(format!("{}", err))
    }
}

impl core::convert::From<AzSvgParseError> for PyErr {
    fn from(err: AzSvgParseError) -> PyErr {
        PyOSError::new_err(format!("{}", err))
    }
}

impl core::convert::From<AzXmlError> for PyErr {
    fn from(err: AzXmlError) -> PyErr {
        PyOSError::new_err(format!("{}", err))
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

