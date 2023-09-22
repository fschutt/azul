#ifndef AZUL_H
#define AZUL_H

namespace dll {

    #include <cstdint>
    #include <cstddef>
    
    struct RefAny;
    struct LayoutCallbackInfo;
    struct StyledDom;
    using MarshaledLayoutCallbackType = StyledDom(*)(RefAny* restrict, RefAny* restrict, LayoutCallbackInfo);
    
    using LayoutCallbackType = StyledDom(*)(RefAny* restrict, LayoutCallbackInfo* restrict);
    
    struct CallbackInfo;
    enum Update;
    using CallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict);
    
    struct IFrameCallbackInfo;
    struct IFrameCallbackReturn;
    using IFrameCallbackType = IFrameCallbackReturn(*)(RefAny* restrict, IFrameCallbackInfo* restrict);
    
    struct RenderImageCallbackInfo;
    struct ImageRef;
    using RenderImageCallbackType = ImageRef(*)(RefAny* restrict, RenderImageCallbackInfo* restrict);
    
    struct TimerCallbackInfo;
    struct TimerCallbackReturn;
    using TimerCallbackType = TimerCallbackReturn(*)(RefAny* restrict, TimerCallbackInfo* restrict);
    
    using WriteBackCallbackType = Update(*)(RefAny* restrict, RefAny* restrict, CallbackInfo* restrict);
    
    struct ThreadSender;
    struct ThreadReceiver;
    using ThreadCallbackType = void(*)(RefAny, ThreadSender, ThreadReceiver);
    
    using RefAnyDestructorType = void(*)(void* restrict);
    
    using RibbonOnTabClickedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, int32_t);
    
    struct FileInputState;
    using FileInputOnPathChangeCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, FileInputState* const);
    
    struct CheckBoxState;
    using CheckBoxOnToggleCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, CheckBoxState* const);
    
    struct ColorInputState;
    using ColorInputOnValueChangeCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, ColorInputState* const);
    
    struct TextInputState;
    struct OnTextInputReturn;
    using TextInputOnTextInputCallbackType = OnTextInputReturn(*)(RefAny* restrict, CallbackInfo* restrict, TextInputState* const);
    
    using TextInputOnVirtualKeyDownCallbackType = OnTextInputReturn(*)(RefAny* restrict, CallbackInfo* restrict, TextInputState* const);
    
    using TextInputOnFocusLostCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, TextInputState* const);
    
    struct NumberInputState;
    using NumberInputOnValueChangeCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NumberInputState* const);
    
    using NumberInputOnFocusLostCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NumberInputState* const);
    
    struct TabHeaderState;
    using TabOnClickCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, TabHeaderState* const);
    
    struct NodeTypeId;
    struct NodeGraphNodeId;
    struct NodePosition;
    using NodeGraphOnNodeAddedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NodeTypeId, NodeGraphNodeId, NodePosition);
    
    using NodeGraphOnNodeRemovedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NodeGraphNodeId);
    
    struct GraphDragAmount;
    using NodeGraphOnNodeGraphDraggedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, GraphDragAmount);
    
    struct NodeDragAmount;
    using NodeGraphOnNodeDraggedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NodeGraphNodeId, NodeDragAmount);
    
    using NodeGraphOnNodeConnectedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NodeGraphNodeId, size_t, NodeGraphNodeId, size_t);
    
    using NodeGraphOnNodeInputDisconnectedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NodeGraphNodeId, size_t);
    
    using NodeGraphOnNodeOutputDisconnectedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NodeGraphNodeId, size_t);
    
    union NodeTypeFieldValue;
    using NodeGraphOnNodeFieldEditedCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, NodeGraphNodeId, size_t, NodeTypeId, NodeTypeFieldValue);
    
    struct ListViewState;
    using ListViewOnLazyLoadScrollCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, ListViewState* const);
    
    using ListViewOnColumnClickCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, ListViewState* const, size_t);
    
    using ListViewOnRowClickCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, ListViewState* const, size_t);
    
    using DropDownOnChoiceChangeCallbackType = Update(*)(RefAny* restrict, CallbackInfo* restrict, size_t);
    
    using ParsedFontDestructorFnType = void(*)(void* restrict);
    
    struct InstantPtr;
    using InstantPtrCloneFnType = InstantPtr(*)(InstantPtr* const);
    
    using InstantPtrDestructorFnType = void(*)(InstantPtr* restrict);
    
    struct ThreadCallback;
    struct Thread;
    using CreateThreadFnType = Thread(*)(RefAny, RefAny, ThreadCallback);
    
    union Instant;
    using GetSystemTimeFnType = Instant(*)();
    
    using CheckThreadFinishedFnType = bool(*)(const void*);
    
    union ThreadSendMsg;
    using LibrarySendThreadMsgFnType = bool(*)(const void*, ThreadSendMsg);
    
    union OptionThreadReceiveMsg;
    using LibraryReceiveThreadMsgFnType = OptionThreadReceiveMsg(*)(const void*);
    
    union OptionThreadSendMsg;
    using ThreadRecvFnType = OptionThreadSendMsg(*)(const void*);
    
    union ThreadReceiveMsg;
    using ThreadSendFnType = bool(*)(const void*, ThreadReceiveMsg);
    
    using ThreadDestructorFnType = void(*)(Thread* restrict);
    
    using ThreadReceiverDestructorFnType = void(*)(ThreadReceiver* restrict);
    
    using ThreadSenderDestructorFnType = void(*)(ThreadSender* restrict);
    
    struct StyleFontFamilyVec;
    using StyleFontFamilyVecDestructorType = void(*)(StyleFontFamilyVec* restrict);
    
    struct ListViewRowVec;
    using ListViewRowVecDestructorType = void(*)(ListViewRowVec* restrict);
    
    struct StyleFilterVec;
    using StyleFilterVecDestructorType = void(*)(StyleFilterVec* restrict);
    
    struct LogicalRectVec;
    using LogicalRectVecDestructorType = void(*)(LogicalRectVec* restrict);
    
    struct NodeTypeIdInfoMapVec;
    using NodeTypeIdInfoMapVecDestructorType = void(*)(NodeTypeIdInfoMapVec* restrict);
    
    struct InputOutputTypeIdInfoMapVec;
    using InputOutputTypeIdInfoMapVecDestructorType = void(*)(InputOutputTypeIdInfoMapVec* restrict);
    
    struct NodeIdNodeMapVec;
    using NodeIdNodeMapVecDestructorType = void(*)(NodeIdNodeMapVec* restrict);
    
    struct InputOutputTypeIdVec;
    using InputOutputTypeIdVecDestructorType = void(*)(InputOutputTypeIdVec* restrict);
    
    struct NodeTypeFieldVec;
    using NodeTypeFieldVecDestructorType = void(*)(NodeTypeFieldVec* restrict);
    
    struct InputConnectionVec;
    using InputConnectionVecDestructorType = void(*)(InputConnectionVec* restrict);
    
    struct OutputNodeAndIndexVec;
    using OutputNodeAndIndexVecDestructorType = void(*)(OutputNodeAndIndexVec* restrict);
    
    struct OutputConnectionVec;
    using OutputConnectionVecDestructorType = void(*)(OutputConnectionVec* restrict);
    
    struct InputNodeAndIndexVec;
    using InputNodeAndIndexVecDestructorType = void(*)(InputNodeAndIndexVec* restrict);
    
    struct AccessibilityStateVec;
    using AccessibilityStateVecDestructorType = void(*)(AccessibilityStateVec* restrict);
    
    struct MenuItemVec;
    using MenuItemVecDestructorType = void(*)(MenuItemVec* restrict);
    
    struct TessellatedSvgNodeVec;
    using TessellatedSvgNodeVecDestructorType = void(*)(TessellatedSvgNodeVec* restrict);
    
    struct XmlNodeVec;
    using XmlNodeVecDestructorType = void(*)(XmlNodeVec* restrict);
    
    struct FmtArgVec;
    using FmtArgVecDestructorType = void(*)(FmtArgVec* restrict);
    
    struct InlineLineVec;
    using InlineLineVecDestructorType = void(*)(InlineLineVec* restrict);
    
    struct InlineWordVec;
    using InlineWordVecDestructorType = void(*)(InlineWordVec* restrict);
    
    struct InlineGlyphVec;
    using InlineGlyphVecDestructorType = void(*)(InlineGlyphVec* restrict);
    
    struct InlineTextHitVec;
    using InlineTextHitVecDestructorType = void(*)(InlineTextHitVec* restrict);
    
    struct MonitorVec;
    using MonitorVecDestructorType = void(*)(MonitorVec* restrict);
    
    struct VideoModeVec;
    using VideoModeVecDestructorType = void(*)(VideoModeVec* restrict);
    
    struct DomVec;
    using DomVecDestructorType = void(*)(DomVec* restrict);
    
    struct IdOrClassVec;
    using IdOrClassVecDestructorType = void(*)(IdOrClassVec* restrict);
    
    struct NodeDataInlineCssPropertyVec;
    using NodeDataInlineCssPropertyVecDestructorType = void(*)(NodeDataInlineCssPropertyVec* restrict);
    
    struct StyleBackgroundContentVec;
    using StyleBackgroundContentVecDestructorType = void(*)(StyleBackgroundContentVec* restrict);
    
    struct StyleBackgroundPositionVec;
    using StyleBackgroundPositionVecDestructorType = void(*)(StyleBackgroundPositionVec* restrict);
    
    struct StyleBackgroundRepeatVec;
    using StyleBackgroundRepeatVecDestructorType = void(*)(StyleBackgroundRepeatVec* restrict);
    
    struct StyleBackgroundSizeVec;
    using StyleBackgroundSizeVecDestructorType = void(*)(StyleBackgroundSizeVec* restrict);
    
    struct StyleTransformVec;
    using StyleTransformVecDestructorType = void(*)(StyleTransformVec* restrict);
    
    struct CssPropertyVec;
    using CssPropertyVecDestructorType = void(*)(CssPropertyVec* restrict);
    
    struct SvgMultiPolygonVec;
    using SvgMultiPolygonVecDestructorType = void(*)(SvgMultiPolygonVec* restrict);
    
    struct SvgSimpleNodeVec;
    using SvgSimpleNodeVecDestructorType = void(*)(SvgSimpleNodeVec* restrict);
    
    struct SvgPathVec;
    using SvgPathVecDestructorType = void(*)(SvgPathVec* restrict);
    
    struct VertexAttributeVec;
    using VertexAttributeVecDestructorType = void(*)(VertexAttributeVec* restrict);
    
    struct SvgPathElementVec;
    using SvgPathElementVecDestructorType = void(*)(SvgPathElementVec* restrict);
    
    struct SvgVertexVec;
    using SvgVertexVecDestructorType = void(*)(SvgVertexVec* restrict);
    
    struct U32Vec;
    using U32VecDestructorType = void(*)(U32Vec* restrict);
    
    struct XWindowTypeVec;
    using XWindowTypeVecDestructorType = void(*)(XWindowTypeVec* restrict);
    
    struct VirtualKeyCodeVec;
    using VirtualKeyCodeVecDestructorType = void(*)(VirtualKeyCodeVec* restrict);
    
    struct CascadeInfoVec;
    using CascadeInfoVecDestructorType = void(*)(CascadeInfoVec* restrict);
    
    struct ScanCodeVec;
    using ScanCodeVecDestructorType = void(*)(ScanCodeVec* restrict);
    
    struct CssDeclarationVec;
    using CssDeclarationVecDestructorType = void(*)(CssDeclarationVec* restrict);
    
    struct CssPathSelectorVec;
    using CssPathSelectorVecDestructorType = void(*)(CssPathSelectorVec* restrict);
    
    struct StylesheetVec;
    using StylesheetVecDestructorType = void(*)(StylesheetVec* restrict);
    
    struct CssRuleBlockVec;
    using CssRuleBlockVecDestructorType = void(*)(CssRuleBlockVec* restrict);
    
    struct F32Vec;
    using F32VecDestructorType = void(*)(F32Vec* restrict);
    
    struct U16Vec;
    using U16VecDestructorType = void(*)(U16Vec* restrict);
    
    struct U8Vec;
    using U8VecDestructorType = void(*)(U8Vec* restrict);
    
    struct CallbackDataVec;
    using CallbackDataVecDestructorType = void(*)(CallbackDataVec* restrict);
    
    struct DebugMessageVec;
    using DebugMessageVecDestructorType = void(*)(DebugMessageVec* restrict);
    
    struct GLuintVec;
    using GLuintVecDestructorType = void(*)(GLuintVec* restrict);
    
    struct GLintVec;
    using GLintVecDestructorType = void(*)(GLintVec* restrict);
    
    struct StringVec;
    using StringVecDestructorType = void(*)(StringVec* restrict);
    
    struct StringPairVec;
    using StringPairVecDestructorType = void(*)(StringPairVec* restrict);
    
    struct NormalizedLinearColorStopVec;
    using NormalizedLinearColorStopVecDestructorType = void(*)(NormalizedLinearColorStopVec* restrict);
    
    struct NormalizedRadialColorStopVec;
    using NormalizedRadialColorStopVecDestructorType = void(*)(NormalizedRadialColorStopVec* restrict);
    
    struct NodeIdVec;
    using NodeIdVecDestructorType = void(*)(NodeIdVec* restrict);
    
    struct NodeHierarchyItemVec;
    using NodeHierarchyItemVecDestructorType = void(*)(NodeHierarchyItemVec* restrict);
    
    struct StyledNodeVec;
    using StyledNodeVecDestructorType = void(*)(StyledNodeVec* restrict);
    
    struct TagIdToNodeIdMappingVec;
    using TagIdToNodeIdMappingVecDestructorType = void(*)(TagIdToNodeIdMappingVec* restrict);
    
    struct ParentWithNodeDepthVec;
    using ParentWithNodeDepthVecDestructorType = void(*)(ParentWithNodeDepthVec* restrict);
    
    struct NodeDataVec;
    using NodeDataVecDestructorType = void(*)(NodeDataVec* restrict);
    
    
    struct App {
        void* ptr;
        bool  run_destructor;
        App& operator=(const App&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        App(const App&) = delete; /* disable copy constructor, use explicit .clone() */
        App() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class AppLogLevel {
       Off,
       Error,
       Warn,
       Info,
       Debug,
       Trace,
    };
    
    enum class LayoutSolver {
       Default,
    };
    
    enum class Vsync {
       Enabled,
       Disabled,
       DontCare,
    };
    
    enum class Srgb {
       Enabled,
       Disabled,
       DontCare,
    };
    
    enum class HwAcceleration {
       Enabled,
       Disabled,
       DontCare,
    };
    
    struct LayoutPoint {
        ssize_t x;
        ssize_t y;
        LayoutPoint& operator=(const LayoutPoint&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutPoint() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutSize {
        ssize_t width;
        ssize_t height;
        LayoutSize& operator=(const LayoutSize&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutSize() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct IOSHandle {
        void* restrict ui_window;
        void* restrict ui_view;
        void* restrict ui_view_controller;
        IOSHandle& operator=(const IOSHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        IOSHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct MacOSHandle {
        void* restrict ns_window;
        void* restrict ns_view;
        MacOSHandle& operator=(const MacOSHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MacOSHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct XlibHandle {
        uint64_t window;
        void* restrict display;
        XlibHandle& operator=(const XlibHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        XlibHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct XcbHandle {
        uint32_t window;
        void* restrict connection;
        XcbHandle& operator=(const XcbHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        XcbHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WaylandHandle {
        void* restrict surface;
        void* restrict display;
        WaylandHandle& operator=(const WaylandHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WaylandHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WindowsHandle {
        void* restrict hwnd;
        void* restrict hinstance;
        WindowsHandle& operator=(const WindowsHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WindowsHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WebHandle {
        uint32_t id;
        WebHandle& operator=(const WebHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WebHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct AndroidHandle {
        void* restrict a_native_window;
        AndroidHandle& operator=(const AndroidHandle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        AndroidHandle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class XWindowType {
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
    };
    
    struct PhysicalPositionI32 {
        int32_t x;
        int32_t y;
        PhysicalPositionI32& operator=(const PhysicalPositionI32&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        PhysicalPositionI32() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct PhysicalSizeU32 {
        uint32_t width;
        uint32_t height;
        PhysicalSizeU32& operator=(const PhysicalSizeU32&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        PhysicalSizeU32() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LogicalPosition {
        float x;
        float y;
        LogicalPosition& operator=(const LogicalPosition&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LogicalPosition() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LogicalSize {
        float width;
        float height;
        LogicalSize& operator=(const LogicalSize&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LogicalSize() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct IconKey {
        size_t id;
        IconKey& operator=(const IconKey&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        IconKey() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class VirtualKeyCode {
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
    };
    
    enum class WindowFrame {
       Normal,
       Minimized,
       Maximized,
       Fullscreen,
    };
    
    struct DebugState {
        bool  profiler_dbg;
        bool  render_target_dbg;
        bool  texture_cache_dbg;
        bool  gpu_time_queries;
        bool  gpu_sample_queries;
        bool  disable_batching;
        bool  epochs;
        bool  echo_driver_messages;
        bool  show_overdraw;
        bool  gpu_cache_dbg;
        bool  texture_cache_dbg_clear_evicted;
        bool  picture_caching_dbg;
        bool  primitive_dbg;
        bool  zoom_dbg;
        bool  small_screen;
        bool  disable_opaque_pass;
        bool  disable_alpha_pass;
        bool  disable_clip_masks;
        bool  disable_text_prims;
        bool  disable_gradient_prims;
        bool  obscure_images;
        bool  glyph_flashing;
        bool  smart_profiler;
        bool  invalidation_dbg;
        bool  tile_cache_logging_dbg;
        bool  profiler_capture;
        bool  force_picture_invalidation;
        DebugState& operator=(const DebugState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DebugState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class MouseCursorType {
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
    };
    
    enum class RendererType {
       Hardware,
       Software,
    };
    
    struct MacWindowOptions {
        uint8_t _reserved;
        MacWindowOptions& operator=(const MacWindowOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MacWindowOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WasmWindowOptions {
        uint8_t _reserved;
        WasmWindowOptions& operator=(const WasmWindowOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WasmWindowOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class FullScreenMode {
       SlowFullScreen,
       FastFullScreen,
       SlowWindowed,
       FastWindowed,
    };
    
    enum class WindowTheme {
       DarkMode,
       LightMode,
    };
    
    struct TouchState {
        uint8_t unused;
        TouchState& operator=(const TouchState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TouchState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct MarshaledLayoutCallbackInner {
        MarshaledLayoutCallbackType cb;
        MarshaledLayoutCallbackInner& operator=(const MarshaledLayoutCallbackInner&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MarshaledLayoutCallbackInner() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutCallbackInner {
        LayoutCallbackType cb;
        LayoutCallbackInner& operator=(const LayoutCallbackInner&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutCallbackInner() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Callback {
        CallbackType cb;
        Callback& operator=(const Callback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Callback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class UpdateImageType {
       Background,
       Content,
    };
    
    enum class Update {
       DoNothing,
       RefreshDom,
       RefreshDomAllWindows,
    };
    
    struct NodeId {
        size_t inner;
        NodeId& operator=(const NodeId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DomId {
        size_t inner;
        DomId& operator=(const DomId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DomId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct PositionInfoInner {
        float x_offset;
        float y_offset;
        float static_x_offset;
        float static_y_offset;
        PositionInfoInner& operator=(const PositionInfoInner&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        PositionInfoInner(const PositionInfoInner&) = delete; /* disable copy constructor, use explicit .clone() */
        PositionInfoInner() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class AnimationRepeat {
       NoRepeat,
       Loop,
       PingPong,
    };
    
    enum class AnimationRepeatCountTag {
       Times,
       Infinite,
    };
    
    struct AnimationRepeatCountVariant_Times { AnimationRepeatCountTag tag; size_t payload; };
    struct AnimationRepeatCountVariant_Infinite { AnimationRepeatCountTag tag; };
    union AnimationRepeatCount {
        AnimationRepeatCountVariant_Times Times;
        AnimationRepeatCountVariant_Infinite Infinite;
    };
    
    
    struct IFrameCallback {
        IFrameCallbackType cb;
        IFrameCallback& operator=(const IFrameCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        IFrameCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RenderImageCallback {
        RenderImageCallbackType cb;
        RenderImageCallback& operator=(const RenderImageCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RenderImageCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TimerCallback {
        TimerCallbackType cb;
        TimerCallback& operator=(const TimerCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TimerCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WriteBackCallback {
        WriteBackCallbackType cb;
        WriteBackCallback& operator=(const WriteBackCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WriteBackCallback(const WriteBackCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        WriteBackCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadCallback {
        ThreadCallbackType cb;
        ThreadCallback& operator=(const ThreadCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadCallback(const ThreadCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RefCount {
        void* ptr;
        bool  run_destructor;
        RefCount& operator=(const RefCount&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RefCount(const RefCount&) = delete; /* disable copy constructor, use explicit .clone() */
        RefCount() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class On {
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
    };
    
    enum class HoverEventFilter {
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
    };
    
    enum class FocusEventFilter {
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
    };
    
    enum class WindowEventFilter {
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
    };
    
    enum class ComponentEventFilter {
       AfterMount,
       BeforeUnmount,
       NodeResized,
       DefaultAction,
       Selected,
    };
    
    enum class ApplicationEventFilter {
       DeviceConnected,
       DeviceDisconnected,
    };
    
    enum class AccessibilityRole {
       TitleBar,
       MenuBar,
       ScrollBar,
       Grip,
       Sound,
       Cursor,
       Caret,
       Alert,
       Window,
       Client,
       MenuPopup,
       MenuItem,
       Tooltip,
       Application,
       Document,
       Pane,
       Chart,
       Dialog,
       Border,
       Grouping,
       Separator,
       Toolbar,
       StatusBar,
       Table,
       ColumnHeader,
       RowHeader,
       Column,
       Row,
       Cell,
       Link,
       HelpBalloon,
       Character,
       List,
       ListItem,
       Outline,
       OutlineItem,
       Pagetab,
       PropertyPage,
       Indicator,
       Graphic,
       StaticText,
       Text,
       PushButton,
       CheckButton,
       RadioButton,
       ComboBox,
       DropList,
       ProgressBar,
       Dial,
       HotkeyField,
       Slider,
       SpinButton,
       Diagram,
       Animation,
       Equation,
       ButtonDropdown,
       ButtonMenu,
       ButtonDropdownGrid,
       Whitespace,
       PageTabList,
       Clock,
       SplitButton,
       IpAddress,
       Nothing,
    };
    
    enum class AccessibilityState {
       Unavailable,
       Selected,
       Focused,
       Checked,
       Readonly,
       Default,
       Expanded,
       Collapsed,
       Busy,
       Offscreen,
       Focusable,
       Selectable,
       Linked,
       Traversed,
       Multiselectable,
       Protected,
    };
    
    enum class TabIndexTag {
       Auto,
       OverrideInParent,
       NoKeyboardFocus,
    };
    
    struct TabIndexVariant_Auto { TabIndexTag tag; };
    struct TabIndexVariant_OverrideInParent { TabIndexTag tag; uint32_t payload; };
    struct TabIndexVariant_NoKeyboardFocus { TabIndexTag tag; };
    union TabIndex {
        TabIndexVariant_Auto Auto;
        TabIndexVariant_OverrideInParent OverrideInParent;
        TabIndexVariant_NoKeyboardFocus NoKeyboardFocus;
    };
    
    
    enum class ContextMenuMouseButton {
       Right,
       Middle,
       Left,
    };
    
    enum class MenuPopupPosition {
       BottomLeftOfCursor,
       BottomRightOfCursor,
       TopLeftOfCursor,
       TopRightOfCursor,
       BottomOfHitRect,
       LeftOfHitRect,
       TopOfHitRect,
       RightOfHitRect,
       AutoCursor,
       AutoHitRect,
    };
    
    enum class MenuItemState {
       Normal,
       Greyed,
       Disabled,
    };
    
    enum class NodeTypeKey {
       Body,
       Div,
       Br,
       P,
       Img,
       IFrame,
    };
    
    struct CssNthChildPattern {
        uint32_t repeat;
        uint32_t offset;
        CssNthChildPattern& operator=(const CssNthChildPattern&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssNthChildPattern() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class CssPropertyType {
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
       BackgroundContent,
       BackgroundPosition,
       BackgroundSize,
       BackgroundRepeat,
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
       TransformOrigin,
       PerspectiveOrigin,
       BackfaceVisibility,
       MixBlendMode,
       Filter,
       BackdropFilter,
       TextShadow,
    };
    
    struct ColorU {
        uint8_t r;
        uint8_t g;
        uint8_t b;
        uint8_t a;
        ColorU& operator=(const ColorU&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ColorU() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class SizeMetric {
       Px,
       Pt,
       Em,
       Percent,
    };
    
    struct FloatValue {
        ssize_t number;
        FloatValue& operator=(const FloatValue&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FloatValue() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class BoxShadowClipMode {
       Outset,
       Inset,
    };
    
    enum class StyleMixBlendMode {
       Normal,
       Multiply,
       Screen,
       Overlay,
       Darken,
       Lighten,
       ColorDodge,
       ColorBurn,
       HardLight,
       SoftLight,
       Difference,
       Exclusion,
       Hue,
       Saturation,
       Color,
       Luminosity,
    };
    
    enum class LayoutAlignContent {
       Stretch,
       Center,
       Start,
       End,
       SpaceBetween,
       SpaceAround,
    };
    
    enum class LayoutAlignItems {
       Stretch,
       Center,
       FlexStart,
       FlexEnd,
    };
    
    enum class LayoutBoxSizing {
       ContentBox,
       BorderBox,
    };
    
    enum class LayoutFlexDirection {
       Row,
       RowReverse,
       Column,
       ColumnReverse,
    };
    
    enum class LayoutDisplay {
       None,
       Flex,
       Block,
       InlineBlock,
    };
    
    enum class LayoutFloat {
       Left,
       Right,
    };
    
    enum class LayoutJustifyContent {
       Start,
       End,
       Center,
       SpaceBetween,
       SpaceAround,
       SpaceEvenly,
    };
    
    enum class LayoutPosition {
       Static,
       Relative,
       Absolute,
       Fixed,
    };
    
    enum class LayoutFlexWrap {
       Wrap,
       NoWrap,
    };
    
    enum class LayoutOverflow {
       Scroll,
       Auto,
       Hidden,
       Visible,
    };
    
    enum class AngleMetric {
       Degree,
       Radians,
       Grad,
       Turn,
       Percent,
    };
    
    enum class DirectionCorner {
       Right,
       Left,
       Top,
       Bottom,
       TopRight,
       TopLeft,
       BottomRight,
       BottomLeft,
    };
    
    enum class ExtendMode {
       Clamp,
       Repeat,
    };
    
    enum class Shape {
       Ellipse,
       Circle,
    };
    
    enum class RadialGradientSize {
       ClosestSide,
       ClosestCorner,
       FarthestSide,
       FarthestCorner,
    };
    
    enum class StyleBackgroundRepeat {
       NoRepeat,
       Repeat,
       RepeatX,
       RepeatY,
    };
    
    enum class BorderStyle {
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
    };
    
    enum class StyleCursor {
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
    };
    
    enum class StyleBackfaceVisibility {
       Hidden,
       Visible,
    };
    
    enum class StyleTextAlign {
       Left,
       Center,
       Right,
    };
    
    struct Ribbon {
        int32_t tab_active;
        Ribbon& operator=(const Ribbon&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Ribbon(const Ribbon&) = delete; /* disable copy constructor, use explicit .clone() */
        Ribbon() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RibbonOnTabClickedCallback {
        RibbonOnTabClickedCallbackType cb;
        RibbonOnTabClickedCallback& operator=(const RibbonOnTabClickedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RibbonOnTabClickedCallback(const RibbonOnTabClickedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        RibbonOnTabClickedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct FileInputOnPathChangeCallback {
        FileInputOnPathChangeCallbackType cb;
        FileInputOnPathChangeCallback& operator=(const FileInputOnPathChangeCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FileInputOnPathChangeCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CheckBoxOnToggleCallback {
        CheckBoxOnToggleCallbackType cb;
        CheckBoxOnToggleCallback& operator=(const CheckBoxOnToggleCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CheckBoxOnToggleCallback(const CheckBoxOnToggleCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        CheckBoxOnToggleCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CheckBoxState {
        bool  checked;
        CheckBoxState& operator=(const CheckBoxState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CheckBoxState(const CheckBoxState&) = delete; /* disable copy constructor, use explicit .clone() */
        CheckBoxState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ColorInputOnValueChangeCallback {
        ColorInputOnValueChangeCallbackType cb;
        ColorInputOnValueChangeCallback& operator=(const ColorInputOnValueChangeCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ColorInputOnValueChangeCallback(const ColorInputOnValueChangeCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        ColorInputOnValueChangeCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputSelectionRange {
        size_t from;
        size_t to;
        TextInputSelectionRange& operator=(const TextInputSelectionRange&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputSelectionRange(const TextInputSelectionRange&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputSelectionRange() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputOnTextInputCallback {
        TextInputOnTextInputCallbackType cb;
        TextInputOnTextInputCallback& operator=(const TextInputOnTextInputCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputOnTextInputCallback(const TextInputOnTextInputCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputOnTextInputCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputOnVirtualKeyDownCallback {
        TextInputOnVirtualKeyDownCallbackType cb;
        TextInputOnVirtualKeyDownCallback& operator=(const TextInputOnVirtualKeyDownCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputOnVirtualKeyDownCallback(const TextInputOnVirtualKeyDownCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputOnVirtualKeyDownCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputOnFocusLostCallback {
        TextInputOnFocusLostCallbackType cb;
        TextInputOnFocusLostCallback& operator=(const TextInputOnFocusLostCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputOnFocusLostCallback(const TextInputOnFocusLostCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputOnFocusLostCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class TextInputValid {
       Yes,
       No,
    };
    
    struct NumberInputState {
        float previous;
        float number;
        float min;
        float max;
        NumberInputState& operator=(const NumberInputState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NumberInputState(const NumberInputState&) = delete; /* disable copy constructor, use explicit .clone() */
        NumberInputState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NumberInputOnValueChangeCallback {
        NumberInputOnValueChangeCallbackType cb;
        NumberInputOnValueChangeCallback& operator=(const NumberInputOnValueChangeCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NumberInputOnValueChangeCallback(const NumberInputOnValueChangeCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NumberInputOnValueChangeCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NumberInputOnFocusLostCallback {
        NumberInputOnFocusLostCallbackType cb;
        NumberInputOnFocusLostCallback& operator=(const NumberInputOnFocusLostCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NumberInputOnFocusLostCallback(const NumberInputOnFocusLostCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NumberInputOnFocusLostCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ProgressBarState {
        float percent_done;
        bool  display_percentage;
        ProgressBarState& operator=(const ProgressBarState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ProgressBarState(const ProgressBarState&) = delete; /* disable copy constructor, use explicit .clone() */
        ProgressBarState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TabHeaderState {
        size_t active_tab;
        TabHeaderState& operator=(const TabHeaderState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TabHeaderState(const TabHeaderState&) = delete; /* disable copy constructor, use explicit .clone() */
        TabHeaderState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TabOnClickCallback {
        TabOnClickCallbackType cb;
        TabOnClickCallback& operator=(const TabOnClickCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TabOnClickCallback(const TabOnClickCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        TabOnClickCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class NodeGraphStyle {
       Default,
    };
    
    struct NodeGraphOnNodeAddedCallback {
        NodeGraphOnNodeAddedCallbackType cb;
        NodeGraphOnNodeAddedCallback& operator=(const NodeGraphOnNodeAddedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeAddedCallback(const NodeGraphOnNodeAddedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeAddedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeRemovedCallback {
        NodeGraphOnNodeRemovedCallbackType cb;
        NodeGraphOnNodeRemovedCallback& operator=(const NodeGraphOnNodeRemovedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeRemovedCallback(const NodeGraphOnNodeRemovedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeRemovedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeGraphDraggedCallback {
        NodeGraphOnNodeGraphDraggedCallbackType cb;
        NodeGraphOnNodeGraphDraggedCallback& operator=(const NodeGraphOnNodeGraphDraggedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeGraphDraggedCallback(const NodeGraphOnNodeGraphDraggedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeGraphDraggedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeDraggedCallback {
        NodeGraphOnNodeDraggedCallbackType cb;
        NodeGraphOnNodeDraggedCallback& operator=(const NodeGraphOnNodeDraggedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeDraggedCallback(const NodeGraphOnNodeDraggedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeDraggedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeConnectedCallback {
        NodeGraphOnNodeConnectedCallbackType cb;
        NodeGraphOnNodeConnectedCallback& operator=(const NodeGraphOnNodeConnectedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeConnectedCallback(const NodeGraphOnNodeConnectedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeConnectedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeInputDisconnectedCallback {
        NodeGraphOnNodeInputDisconnectedCallbackType cb;
        NodeGraphOnNodeInputDisconnectedCallback& operator=(const NodeGraphOnNodeInputDisconnectedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeInputDisconnectedCallback(const NodeGraphOnNodeInputDisconnectedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeInputDisconnectedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeOutputDisconnectedCallback {
        NodeGraphOnNodeOutputDisconnectedCallbackType cb;
        NodeGraphOnNodeOutputDisconnectedCallback& operator=(const NodeGraphOnNodeOutputDisconnectedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeOutputDisconnectedCallback(const NodeGraphOnNodeOutputDisconnectedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeOutputDisconnectedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeFieldEditedCallback {
        NodeGraphOnNodeFieldEditedCallbackType cb;
        NodeGraphOnNodeFieldEditedCallback& operator=(const NodeGraphOnNodeFieldEditedCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeFieldEditedCallback(const NodeGraphOnNodeFieldEditedCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeFieldEditedCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputOutputTypeId {
        uint64_t inner;
        InputOutputTypeId& operator=(const InputOutputTypeId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputOutputTypeId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeTypeId {
        uint64_t inner;
        NodeTypeId& operator=(const NodeTypeId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeTypeId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphNodeId {
        uint64_t inner;
        NodeGraphNodeId& operator=(const NodeGraphNodeId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphNodeId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodePosition {
        float x;
        float y;
        NodePosition& operator=(const NodePosition&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodePosition() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GraphDragAmount {
        float x;
        float y;
        GraphDragAmount& operator=(const GraphDragAmount&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GraphDragAmount() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeDragAmount {
        float x;
        float y;
        NodeDragAmount& operator=(const NodeDragAmount&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeDragAmount() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewOnLazyLoadScrollCallback {
        ListViewOnLazyLoadScrollCallbackType cb;
        ListViewOnLazyLoadScrollCallback& operator=(const ListViewOnLazyLoadScrollCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewOnLazyLoadScrollCallback(const ListViewOnLazyLoadScrollCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewOnLazyLoadScrollCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewOnColumnClickCallback {
        ListViewOnColumnClickCallbackType cb;
        ListViewOnColumnClickCallback& operator=(const ListViewOnColumnClickCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewOnColumnClickCallback(const ListViewOnColumnClickCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewOnColumnClickCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewOnRowClickCallback {
        ListViewOnRowClickCallbackType cb;
        ListViewOnRowClickCallback& operator=(const ListViewOnRowClickCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewOnRowClickCallback(const ListViewOnRowClickCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewOnRowClickCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DropDownOnChoiceChangeCallback {
        DropDownOnChoiceChangeCallbackType cb;
        DropDownOnChoiceChangeCallback& operator=(const DropDownOnChoiceChangeCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DropDownOnChoiceChangeCallback(const DropDownOnChoiceChangeCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        DropDownOnChoiceChangeCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeHierarchyItem {
        size_t parent;
        size_t previous_sibling;
        size_t next_sibling;
        size_t last_child;
        NodeHierarchyItem& operator=(const NodeHierarchyItem&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeHierarchyItem() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CascadeInfo {
        uint32_t index_in_parent;
        bool  is_last_child;
        CascadeInfo& operator=(const CascadeInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CascadeInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyledNodeState {
        bool  normal;
        bool  hover;
        bool  active;
        bool  focused;
        StyledNodeState& operator=(const StyledNodeState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyledNodeState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TagId {
        uint64_t inner;
        TagId& operator=(const TagId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TagId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CssPropertyCache {
        void* restrict ptr;
        bool  run_destructor;
        CssPropertyCache& operator=(const CssPropertyCache&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssPropertyCache(const CssPropertyCache&) = delete; /* disable copy constructor, use explicit .clone() */
        CssPropertyCache() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GlVoidPtrConst {
        void* ptr;
        bool  run_destructor;
        GlVoidPtrConst& operator=(const GlVoidPtrConst&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GlVoidPtrConst(const GlVoidPtrConst&) = delete; /* disable copy constructor, use explicit .clone() */
        GlVoidPtrConst() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GlVoidPtrMut {
        void* restrict ptr;
        GlVoidPtrMut& operator=(const GlVoidPtrMut&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GlVoidPtrMut(const GlVoidPtrMut&) = delete; /* disable copy constructor, use explicit .clone() */
        GlVoidPtrMut() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GlShaderPrecisionFormatReturn {
        int32_t _0;
        int32_t _1;
        int32_t _2;
        GlShaderPrecisionFormatReturn& operator=(const GlShaderPrecisionFormatReturn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GlShaderPrecisionFormatReturn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class VertexAttributeType {
       Float,
       Double,
       UnsignedByte,
       UnsignedShort,
       UnsignedInt,
    };
    
    enum class IndexBufferFormat {
       Points,
       Lines,
       LineStrip,
       Triangles,
       TriangleStrip,
       TriangleFan,
    };
    
    enum class GlType {
       Gl,
       Gles,
    };
    
    struct U8VecRef {
        uint8_t* ptr;
        size_t len;
        U8VecRef& operator=(const U8VecRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        U8VecRef(const U8VecRef&) = delete; /* disable copy constructor, use explicit .clone() */
        U8VecRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct U8VecRefMut {
        uint8_t* restrict ptr;
        size_t len;
        U8VecRefMut& operator=(const U8VecRefMut&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        U8VecRefMut(const U8VecRefMut&) = delete; /* disable copy constructor, use explicit .clone() */
        U8VecRefMut() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct F32VecRef {
        float* ptr;
        size_t len;
        F32VecRef& operator=(const F32VecRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        F32VecRef(const F32VecRef&) = delete; /* disable copy constructor, use explicit .clone() */
        F32VecRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct I32VecRef {
        int32_t* ptr;
        size_t len;
        I32VecRef& operator=(const I32VecRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        I32VecRef(const I32VecRef&) = delete; /* disable copy constructor, use explicit .clone() */
        I32VecRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLuintVecRef {
        uint32_t* ptr;
        size_t len;
        GLuintVecRef& operator=(const GLuintVecRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLuintVecRef(const GLuintVecRef&) = delete; /* disable copy constructor, use explicit .clone() */
        GLuintVecRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLenumVecRef {
        uint32_t* ptr;
        size_t len;
        GLenumVecRef& operator=(const GLenumVecRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLenumVecRef(const GLenumVecRef&) = delete; /* disable copy constructor, use explicit .clone() */
        GLenumVecRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLintVecRefMut {
        int32_t* restrict ptr;
        size_t len;
        GLintVecRefMut& operator=(const GLintVecRefMut&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLintVecRefMut(const GLintVecRefMut&) = delete; /* disable copy constructor, use explicit .clone() */
        GLintVecRefMut() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLint64VecRefMut {
        int64_t* restrict ptr;
        size_t len;
        GLint64VecRefMut& operator=(const GLint64VecRefMut&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLint64VecRefMut(const GLint64VecRefMut&) = delete; /* disable copy constructor, use explicit .clone() */
        GLint64VecRefMut() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLbooleanVecRefMut {
        uint8_t* restrict ptr;
        size_t len;
        GLbooleanVecRefMut& operator=(const GLbooleanVecRefMut&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLbooleanVecRefMut(const GLbooleanVecRefMut&) = delete; /* disable copy constructor, use explicit .clone() */
        GLbooleanVecRefMut() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLfloatVecRefMut {
        float* restrict ptr;
        size_t len;
        GLfloatVecRefMut& operator=(const GLfloatVecRefMut&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLfloatVecRefMut(const GLfloatVecRefMut&) = delete; /* disable copy constructor, use explicit .clone() */
        GLfloatVecRefMut() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Refstr {
        uint8_t* ptr;
        size_t len;
        Refstr& operator=(const Refstr&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Refstr(const Refstr&) = delete; /* disable copy constructor, use explicit .clone() */
        Refstr() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLsyncPtr {
        void* ptr;
        bool  run_destructor;
        GLsyncPtr& operator=(const GLsyncPtr&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLsyncPtr(const GLsyncPtr&) = delete; /* disable copy constructor, use explicit .clone() */
        GLsyncPtr() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextureFlags {
        bool  is_opaque;
        bool  is_video_texture;
        TextureFlags& operator=(const TextureFlags&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextureFlags() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ImageRef {
        void* data;
        void* copies;
        bool  run_destructor;
        ImageRef& operator=(const ImageRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ImageRef(const ImageRef&) = delete; /* disable copy constructor, use explicit .clone() */
        ImageRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class RawImageFormat {
       R8,
       RG8,
       RGB8,
       RGBA8,
       R16,
       RG16,
       RGB16,
       RGBA16,
       BGR8,
       BGRA8,
    };
    
    enum class EncodeImageError {
       EncoderNotAvailable,
       InsufficientMemory,
       DimensionError,
       InvalidData,
       Unknown,
    };
    
    enum class DecodeImageError {
       InsufficientMemory,
       DimensionError,
       UnsupportedImageFormat,
       Unknown,
    };
    
    struct FontRef {
        void* data;
        void* copies;
        bool  run_destructor;
        FontRef& operator=(const FontRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FontRef(const FontRef&) = delete; /* disable copy constructor, use explicit .clone() */
        FontRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Svg {
        void* restrict ptr;
        bool  run_destructor;
        Svg& operator=(const Svg&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Svg(const Svg&) = delete; /* disable copy constructor, use explicit .clone() */
        Svg() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgXmlNode {
        void* restrict ptr;
        bool  run_destructor;
        SvgXmlNode& operator=(const SvgXmlNode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgXmlNode(const SvgXmlNode&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgXmlNode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgCircle {
        float center_x;
        float center_y;
        float radius;
        SvgCircle& operator=(const SvgCircle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgCircle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgPoint {
        float x;
        float y;
        SvgPoint& operator=(const SvgPoint&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgPoint() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgVector {
        double x;
        double y;
        SvgVector& operator=(const SvgVector&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgVector() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgRect {
        float width;
        float height;
        float x;
        float y;
        float radius_top_left;
        float radius_top_right;
        float radius_bottom_left;
        float radius_bottom_right;
        SvgRect& operator=(const SvgRect&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgRect() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgVertex {
        float x;
        float y;
        SvgVertex& operator=(const SvgVertex&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgVertex() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class ShapeRendering {
       OptimizeSpeed,
       CrispEdges,
       GeometricPrecision,
    };
    
    enum class TextRendering {
       OptimizeSpeed,
       OptimizeLegibility,
       GeometricPrecision,
    };
    
    enum class ImageRendering {
       OptimizeQuality,
       OptimizeSpeed,
    };
    
    enum class FontDatabase {
       Empty,
       System,
    };
    
    struct SvgRenderTransform {
        float sx;
        float kx;
        float ky;
        float sy;
        float tx;
        float ty;
        SvgRenderTransform& operator=(const SvgRenderTransform&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgRenderTransform() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class IndentTag {
       None,
       Spaces,
       Tabs,
    };
    
    struct IndentVariant_None { IndentTag tag; };
    struct IndentVariant_Spaces { IndentTag tag; uint8_t payload; };
    struct IndentVariant_Tabs { IndentTag tag; };
    union Indent {
        IndentVariant_None None;
        IndentVariant_Spaces Spaces;
        IndentVariant_Tabs Tabs;
    };
    
    
    enum class SvgFitToTag {
       Original,
       Width,
       Height,
       Zoom,
    };
    
    struct SvgFitToVariant_Original { SvgFitToTag tag; };
    struct SvgFitToVariant_Width { SvgFitToTag tag; uint32_t payload; };
    struct SvgFitToVariant_Height { SvgFitToTag tag; uint32_t payload; };
    struct SvgFitToVariant_Zoom { SvgFitToTag tag; float payload; };
    union SvgFitTo {
        SvgFitToVariant_Original Original;
        SvgFitToVariant_Width Width;
        SvgFitToVariant_Height Height;
        SvgFitToVariant_Zoom Zoom;
    };
    
    
    enum class SvgFillRule {
       Winding,
       EvenOdd,
    };
    
    struct SvgTransform {
        float sx;
        float kx;
        float ky;
        float sy;
        float tx;
        float ty;
        SvgTransform& operator=(const SvgTransform&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgTransform() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class SvgLineJoin {
       Miter,
       MiterClip,
       Round,
       Bevel,
    };
    
    enum class SvgLineCap {
       Butt,
       Square,
       Round,
    };
    
    struct SvgDashPattern {
        float offset;
        float length_1;
        float gap_1;
        float length_2;
        float gap_2;
        float length_3;
        float gap_3;
        SvgDashPattern& operator=(const SvgDashPattern&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgDashPattern() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct MsgBox {
        size_t _reserved;
        MsgBox& operator=(const MsgBox&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MsgBox(const MsgBox&) = delete; /* disable copy constructor, use explicit .clone() */
        MsgBox() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class MsgBoxIcon {
       Info,
       Warning,
       Error,
       Question,
    };
    
    enum class MsgBoxYesNo {
       Yes,
       No,
    };
    
    enum class MsgBoxOkCancel {
       Ok,
       Cancel,
    };
    
    struct FileDialog {
        size_t _reserved;
        FileDialog& operator=(const FileDialog&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FileDialog(const FileDialog&) = delete; /* disable copy constructor, use explicit .clone() */
        FileDialog() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ColorPickerDialog {
        size_t _reserved;
        ColorPickerDialog& operator=(const ColorPickerDialog&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ColorPickerDialog(const ColorPickerDialog&) = delete; /* disable copy constructor, use explicit .clone() */
        ColorPickerDialog() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SystemClipboard {
        void* _native;
        bool  run_destructor;
        SystemClipboard& operator=(const SystemClipboard&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SystemClipboard(const SystemClipboard&) = delete; /* disable copy constructor, use explicit .clone() */
        SystemClipboard() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InstantPtrCloneFn {
        InstantPtrCloneFnType cb;
        InstantPtrCloneFn& operator=(const InstantPtrCloneFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InstantPtrCloneFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InstantPtrDestructorFn {
        InstantPtrDestructorFnType cb;
        InstantPtrDestructorFn& operator=(const InstantPtrDestructorFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InstantPtrDestructorFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SystemTick {
        uint64_t tick_counter;
        SystemTick& operator=(const SystemTick&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SystemTick() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SystemTimeDiff {
        uint64_t secs;
        uint32_t nanos;
        SystemTimeDiff& operator=(const SystemTimeDiff&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SystemTimeDiff() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SystemTickDiff {
        uint64_t tick_diff;
        SystemTickDiff& operator=(const SystemTickDiff&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SystemTickDiff() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TimerId {
        size_t id;
        TimerId& operator=(const TimerId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TimerId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class TerminateTimer {
       Terminate,
       Continue,
    };
    
    struct ThreadId {
        size_t id;
        ThreadId& operator=(const ThreadId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Thread {
        void* ptr;
        bool  run_destructor;
        Thread& operator=(const Thread&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Thread(const Thread&) = delete; /* disable copy constructor, use explicit .clone() */
        Thread() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadSender {
        void* ptr;
        bool  run_destructor;
        ThreadSender& operator=(const ThreadSender&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadSender(const ThreadSender&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadSender() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadReceiver {
        void* ptr;
        bool  run_destructor;
        ThreadReceiver& operator=(const ThreadReceiver&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadReceiver(const ThreadReceiver&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadReceiver() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CreateThreadFn {
        CreateThreadFnType cb;
        CreateThreadFn& operator=(const CreateThreadFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CreateThreadFn(const CreateThreadFn&) = delete; /* disable copy constructor, use explicit .clone() */
        CreateThreadFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GetSystemTimeFn {
        GetSystemTimeFnType cb;
        GetSystemTimeFn& operator=(const GetSystemTimeFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GetSystemTimeFn(const GetSystemTimeFn&) = delete; /* disable copy constructor, use explicit .clone() */
        GetSystemTimeFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CheckThreadFinishedFn {
        CheckThreadFinishedFnType cb;
        CheckThreadFinishedFn& operator=(const CheckThreadFinishedFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CheckThreadFinishedFn(const CheckThreadFinishedFn&) = delete; /* disable copy constructor, use explicit .clone() */
        CheckThreadFinishedFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LibrarySendThreadMsgFn {
        LibrarySendThreadMsgFnType cb;
        LibrarySendThreadMsgFn& operator=(const LibrarySendThreadMsgFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LibrarySendThreadMsgFn(const LibrarySendThreadMsgFn&) = delete; /* disable copy constructor, use explicit .clone() */
        LibrarySendThreadMsgFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LibraryReceiveThreadMsgFn {
        LibraryReceiveThreadMsgFnType cb;
        LibraryReceiveThreadMsgFn& operator=(const LibraryReceiveThreadMsgFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LibraryReceiveThreadMsgFn(const LibraryReceiveThreadMsgFn&) = delete; /* disable copy constructor, use explicit .clone() */
        LibraryReceiveThreadMsgFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadRecvFn {
        ThreadRecvFnType cb;
        ThreadRecvFn& operator=(const ThreadRecvFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadRecvFn(const ThreadRecvFn&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadRecvFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadSendFn {
        ThreadSendFnType cb;
        ThreadSendFn& operator=(const ThreadSendFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadSendFn(const ThreadSendFn&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadSendFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadDestructorFn {
        ThreadDestructorFnType cb;
        ThreadDestructorFn& operator=(const ThreadDestructorFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadDestructorFn(const ThreadDestructorFn&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadDestructorFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadReceiverDestructorFn {
        ThreadReceiverDestructorFnType cb;
        ThreadReceiverDestructorFn& operator=(const ThreadReceiverDestructorFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadReceiverDestructorFn(const ThreadReceiverDestructorFn&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadReceiverDestructorFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ThreadSenderDestructorFn {
        ThreadSenderDestructorFnType cb;
        ThreadSenderDestructorFn& operator=(const ThreadSenderDestructorFn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadSenderDestructorFn(const ThreadSenderDestructorFn&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadSenderDestructorFn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleFontFamilyVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyleFontFamilyVecDestructorVariant_DefaultRust { StyleFontFamilyVecDestructorTag tag; };
    struct StyleFontFamilyVecDestructorVariant_NoDestructor { StyleFontFamilyVecDestructorTag tag; };
    struct StyleFontFamilyVecDestructorVariant_External { StyleFontFamilyVecDestructorTag tag; StyleFontFamilyVecDestructorType payload; };
    union StyleFontFamilyVecDestructor {
        StyleFontFamilyVecDestructorVariant_DefaultRust DefaultRust;
        StyleFontFamilyVecDestructorVariant_NoDestructor NoDestructor;
        StyleFontFamilyVecDestructorVariant_External External;
    };
    
    
    enum class ListViewRowVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct ListViewRowVecDestructorVariant_DefaultRust { ListViewRowVecDestructorTag tag; };
    struct ListViewRowVecDestructorVariant_NoDestructor { ListViewRowVecDestructorTag tag; };
    struct ListViewRowVecDestructorVariant_External { ListViewRowVecDestructorTag tag; ListViewRowVecDestructorType payload; };
    union ListViewRowVecDestructor {
        ListViewRowVecDestructorVariant_DefaultRust DefaultRust;
        ListViewRowVecDestructorVariant_NoDestructor NoDestructor;
        ListViewRowVecDestructorVariant_External External;
    };
    
    
    enum class StyleFilterVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyleFilterVecDestructorVariant_DefaultRust { StyleFilterVecDestructorTag tag; };
    struct StyleFilterVecDestructorVariant_NoDestructor { StyleFilterVecDestructorTag tag; };
    struct StyleFilterVecDestructorVariant_External { StyleFilterVecDestructorTag tag; StyleFilterVecDestructorType payload; };
    union StyleFilterVecDestructor {
        StyleFilterVecDestructorVariant_DefaultRust DefaultRust;
        StyleFilterVecDestructorVariant_NoDestructor NoDestructor;
        StyleFilterVecDestructorVariant_External External;
    };
    
    
    enum class LogicalRectVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct LogicalRectVecDestructorVariant_DefaultRust { LogicalRectVecDestructorTag tag; };
    struct LogicalRectVecDestructorVariant_NoDestructor { LogicalRectVecDestructorTag tag; };
    struct LogicalRectVecDestructorVariant_External { LogicalRectVecDestructorTag tag; LogicalRectVecDestructorType payload; };
    union LogicalRectVecDestructor {
        LogicalRectVecDestructorVariant_DefaultRust DefaultRust;
        LogicalRectVecDestructorVariant_NoDestructor NoDestructor;
        LogicalRectVecDestructorVariant_External External;
    };
    
    
    enum class NodeTypeIdInfoMapVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NodeTypeIdInfoMapVecDestructorVariant_DefaultRust { NodeTypeIdInfoMapVecDestructorTag tag; };
    struct NodeTypeIdInfoMapVecDestructorVariant_NoDestructor { NodeTypeIdInfoMapVecDestructorTag tag; };
    struct NodeTypeIdInfoMapVecDestructorVariant_External { NodeTypeIdInfoMapVecDestructorTag tag; NodeTypeIdInfoMapVecDestructorType payload; };
    union NodeTypeIdInfoMapVecDestructor {
        NodeTypeIdInfoMapVecDestructorVariant_DefaultRust DefaultRust;
        NodeTypeIdInfoMapVecDestructorVariant_NoDestructor NoDestructor;
        NodeTypeIdInfoMapVecDestructorVariant_External External;
    };
    
    
    enum class InputOutputTypeIdInfoMapVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InputOutputTypeIdInfoMapVecDestructorVariant_DefaultRust { InputOutputTypeIdInfoMapVecDestructorTag tag; };
    struct InputOutputTypeIdInfoMapVecDestructorVariant_NoDestructor { InputOutputTypeIdInfoMapVecDestructorTag tag; };
    struct InputOutputTypeIdInfoMapVecDestructorVariant_External { InputOutputTypeIdInfoMapVecDestructorTag tag; InputOutputTypeIdInfoMapVecDestructorType payload; };
    union InputOutputTypeIdInfoMapVecDestructor {
        InputOutputTypeIdInfoMapVecDestructorVariant_DefaultRust DefaultRust;
        InputOutputTypeIdInfoMapVecDestructorVariant_NoDestructor NoDestructor;
        InputOutputTypeIdInfoMapVecDestructorVariant_External External;
    };
    
    
    enum class NodeIdNodeMapVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NodeIdNodeMapVecDestructorVariant_DefaultRust { NodeIdNodeMapVecDestructorTag tag; };
    struct NodeIdNodeMapVecDestructorVariant_NoDestructor { NodeIdNodeMapVecDestructorTag tag; };
    struct NodeIdNodeMapVecDestructorVariant_External { NodeIdNodeMapVecDestructorTag tag; NodeIdNodeMapVecDestructorType payload; };
    union NodeIdNodeMapVecDestructor {
        NodeIdNodeMapVecDestructorVariant_DefaultRust DefaultRust;
        NodeIdNodeMapVecDestructorVariant_NoDestructor NoDestructor;
        NodeIdNodeMapVecDestructorVariant_External External;
    };
    
    
    enum class InputOutputTypeIdVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InputOutputTypeIdVecDestructorVariant_DefaultRust { InputOutputTypeIdVecDestructorTag tag; };
    struct InputOutputTypeIdVecDestructorVariant_NoDestructor { InputOutputTypeIdVecDestructorTag tag; };
    struct InputOutputTypeIdVecDestructorVariant_External { InputOutputTypeIdVecDestructorTag tag; InputOutputTypeIdVecDestructorType payload; };
    union InputOutputTypeIdVecDestructor {
        InputOutputTypeIdVecDestructorVariant_DefaultRust DefaultRust;
        InputOutputTypeIdVecDestructorVariant_NoDestructor NoDestructor;
        InputOutputTypeIdVecDestructorVariant_External External;
    };
    
    
    enum class NodeTypeFieldVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NodeTypeFieldVecDestructorVariant_DefaultRust { NodeTypeFieldVecDestructorTag tag; };
    struct NodeTypeFieldVecDestructorVariant_NoDestructor { NodeTypeFieldVecDestructorTag tag; };
    struct NodeTypeFieldVecDestructorVariant_External { NodeTypeFieldVecDestructorTag tag; NodeTypeFieldVecDestructorType payload; };
    union NodeTypeFieldVecDestructor {
        NodeTypeFieldVecDestructorVariant_DefaultRust DefaultRust;
        NodeTypeFieldVecDestructorVariant_NoDestructor NoDestructor;
        NodeTypeFieldVecDestructorVariant_External External;
    };
    
    
    enum class InputConnectionVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InputConnectionVecDestructorVariant_DefaultRust { InputConnectionVecDestructorTag tag; };
    struct InputConnectionVecDestructorVariant_NoDestructor { InputConnectionVecDestructorTag tag; };
    struct InputConnectionVecDestructorVariant_External { InputConnectionVecDestructorTag tag; InputConnectionVecDestructorType payload; };
    union InputConnectionVecDestructor {
        InputConnectionVecDestructorVariant_DefaultRust DefaultRust;
        InputConnectionVecDestructorVariant_NoDestructor NoDestructor;
        InputConnectionVecDestructorVariant_External External;
    };
    
    
    enum class OutputNodeAndIndexVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct OutputNodeAndIndexVecDestructorVariant_DefaultRust { OutputNodeAndIndexVecDestructorTag tag; };
    struct OutputNodeAndIndexVecDestructorVariant_NoDestructor { OutputNodeAndIndexVecDestructorTag tag; };
    struct OutputNodeAndIndexVecDestructorVariant_External { OutputNodeAndIndexVecDestructorTag tag; OutputNodeAndIndexVecDestructorType payload; };
    union OutputNodeAndIndexVecDestructor {
        OutputNodeAndIndexVecDestructorVariant_DefaultRust DefaultRust;
        OutputNodeAndIndexVecDestructorVariant_NoDestructor NoDestructor;
        OutputNodeAndIndexVecDestructorVariant_External External;
    };
    
    
    enum class OutputConnectionVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct OutputConnectionVecDestructorVariant_DefaultRust { OutputConnectionVecDestructorTag tag; };
    struct OutputConnectionVecDestructorVariant_NoDestructor { OutputConnectionVecDestructorTag tag; };
    struct OutputConnectionVecDestructorVariant_External { OutputConnectionVecDestructorTag tag; OutputConnectionVecDestructorType payload; };
    union OutputConnectionVecDestructor {
        OutputConnectionVecDestructorVariant_DefaultRust DefaultRust;
        OutputConnectionVecDestructorVariant_NoDestructor NoDestructor;
        OutputConnectionVecDestructorVariant_External External;
    };
    
    
    enum class InputNodeAndIndexVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InputNodeAndIndexVecDestructorVariant_DefaultRust { InputNodeAndIndexVecDestructorTag tag; };
    struct InputNodeAndIndexVecDestructorVariant_NoDestructor { InputNodeAndIndexVecDestructorTag tag; };
    struct InputNodeAndIndexVecDestructorVariant_External { InputNodeAndIndexVecDestructorTag tag; InputNodeAndIndexVecDestructorType payload; };
    union InputNodeAndIndexVecDestructor {
        InputNodeAndIndexVecDestructorVariant_DefaultRust DefaultRust;
        InputNodeAndIndexVecDestructorVariant_NoDestructor NoDestructor;
        InputNodeAndIndexVecDestructorVariant_External External;
    };
    
    
    enum class AccessibilityStateVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct AccessibilityStateVecDestructorVariant_DefaultRust { AccessibilityStateVecDestructorTag tag; };
    struct AccessibilityStateVecDestructorVariant_NoDestructor { AccessibilityStateVecDestructorTag tag; };
    struct AccessibilityStateVecDestructorVariant_External { AccessibilityStateVecDestructorTag tag; AccessibilityStateVecDestructorType payload; };
    union AccessibilityStateVecDestructor {
        AccessibilityStateVecDestructorVariant_DefaultRust DefaultRust;
        AccessibilityStateVecDestructorVariant_NoDestructor NoDestructor;
        AccessibilityStateVecDestructorVariant_External External;
    };
    
    
    enum class MenuItemVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct MenuItemVecDestructorVariant_DefaultRust { MenuItemVecDestructorTag tag; };
    struct MenuItemVecDestructorVariant_NoDestructor { MenuItemVecDestructorTag tag; };
    struct MenuItemVecDestructorVariant_External { MenuItemVecDestructorTag tag; MenuItemVecDestructorType payload; };
    union MenuItemVecDestructor {
        MenuItemVecDestructorVariant_DefaultRust DefaultRust;
        MenuItemVecDestructorVariant_NoDestructor NoDestructor;
        MenuItemVecDestructorVariant_External External;
    };
    
    
    enum class TessellatedSvgNodeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct TessellatedSvgNodeVecDestructorVariant_DefaultRust { TessellatedSvgNodeVecDestructorTag tag; };
    struct TessellatedSvgNodeVecDestructorVariant_NoDestructor { TessellatedSvgNodeVecDestructorTag tag; };
    struct TessellatedSvgNodeVecDestructorVariant_External { TessellatedSvgNodeVecDestructorTag tag; TessellatedSvgNodeVecDestructorType payload; };
    union TessellatedSvgNodeVecDestructor {
        TessellatedSvgNodeVecDestructorVariant_DefaultRust DefaultRust;
        TessellatedSvgNodeVecDestructorVariant_NoDestructor NoDestructor;
        TessellatedSvgNodeVecDestructorVariant_External External;
    };
    
    
    enum class XmlNodeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct XmlNodeVecDestructorVariant_DefaultRust { XmlNodeVecDestructorTag tag; };
    struct XmlNodeVecDestructorVariant_NoDestructor { XmlNodeVecDestructorTag tag; };
    struct XmlNodeVecDestructorVariant_External { XmlNodeVecDestructorTag tag; XmlNodeVecDestructorType payload; };
    union XmlNodeVecDestructor {
        XmlNodeVecDestructorVariant_DefaultRust DefaultRust;
        XmlNodeVecDestructorVariant_NoDestructor NoDestructor;
        XmlNodeVecDestructorVariant_External External;
    };
    
    
    enum class FmtArgVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct FmtArgVecDestructorVariant_DefaultRust { FmtArgVecDestructorTag tag; };
    struct FmtArgVecDestructorVariant_NoDestructor { FmtArgVecDestructorTag tag; };
    struct FmtArgVecDestructorVariant_External { FmtArgVecDestructorTag tag; FmtArgVecDestructorType payload; };
    union FmtArgVecDestructor {
        FmtArgVecDestructorVariant_DefaultRust DefaultRust;
        FmtArgVecDestructorVariant_NoDestructor NoDestructor;
        FmtArgVecDestructorVariant_External External;
    };
    
    
    enum class InlineLineVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InlineLineVecDestructorVariant_DefaultRust { InlineLineVecDestructorTag tag; };
    struct InlineLineVecDestructorVariant_NoDestructor { InlineLineVecDestructorTag tag; };
    struct InlineLineVecDestructorVariant_External { InlineLineVecDestructorTag tag; InlineLineVecDestructorType payload; };
    union InlineLineVecDestructor {
        InlineLineVecDestructorVariant_DefaultRust DefaultRust;
        InlineLineVecDestructorVariant_NoDestructor NoDestructor;
        InlineLineVecDestructorVariant_External External;
    };
    
    
    enum class InlineWordVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InlineWordVecDestructorVariant_DefaultRust { InlineWordVecDestructorTag tag; };
    struct InlineWordVecDestructorVariant_NoDestructor { InlineWordVecDestructorTag tag; };
    struct InlineWordVecDestructorVariant_External { InlineWordVecDestructorTag tag; InlineWordVecDestructorType payload; };
    union InlineWordVecDestructor {
        InlineWordVecDestructorVariant_DefaultRust DefaultRust;
        InlineWordVecDestructorVariant_NoDestructor NoDestructor;
        InlineWordVecDestructorVariant_External External;
    };
    
    
    enum class InlineGlyphVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InlineGlyphVecDestructorVariant_DefaultRust { InlineGlyphVecDestructorTag tag; };
    struct InlineGlyphVecDestructorVariant_NoDestructor { InlineGlyphVecDestructorTag tag; };
    struct InlineGlyphVecDestructorVariant_External { InlineGlyphVecDestructorTag tag; InlineGlyphVecDestructorType payload; };
    union InlineGlyphVecDestructor {
        InlineGlyphVecDestructorVariant_DefaultRust DefaultRust;
        InlineGlyphVecDestructorVariant_NoDestructor NoDestructor;
        InlineGlyphVecDestructorVariant_External External;
    };
    
    
    enum class InlineTextHitVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct InlineTextHitVecDestructorVariant_DefaultRust { InlineTextHitVecDestructorTag tag; };
    struct InlineTextHitVecDestructorVariant_NoDestructor { InlineTextHitVecDestructorTag tag; };
    struct InlineTextHitVecDestructorVariant_External { InlineTextHitVecDestructorTag tag; InlineTextHitVecDestructorType payload; };
    union InlineTextHitVecDestructor {
        InlineTextHitVecDestructorVariant_DefaultRust DefaultRust;
        InlineTextHitVecDestructorVariant_NoDestructor NoDestructor;
        InlineTextHitVecDestructorVariant_External External;
    };
    
    
    enum class MonitorVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct MonitorVecDestructorVariant_DefaultRust { MonitorVecDestructorTag tag; };
    struct MonitorVecDestructorVariant_NoDestructor { MonitorVecDestructorTag tag; };
    struct MonitorVecDestructorVariant_External { MonitorVecDestructorTag tag; MonitorVecDestructorType payload; };
    union MonitorVecDestructor {
        MonitorVecDestructorVariant_DefaultRust DefaultRust;
        MonitorVecDestructorVariant_NoDestructor NoDestructor;
        MonitorVecDestructorVariant_External External;
    };
    
    
    enum class VideoModeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct VideoModeVecDestructorVariant_DefaultRust { VideoModeVecDestructorTag tag; };
    struct VideoModeVecDestructorVariant_NoDestructor { VideoModeVecDestructorTag tag; };
    struct VideoModeVecDestructorVariant_External { VideoModeVecDestructorTag tag; VideoModeVecDestructorType payload; };
    union VideoModeVecDestructor {
        VideoModeVecDestructorVariant_DefaultRust DefaultRust;
        VideoModeVecDestructorVariant_NoDestructor NoDestructor;
        VideoModeVecDestructorVariant_External External;
    };
    
    
    enum class DomVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct DomVecDestructorVariant_DefaultRust { DomVecDestructorTag tag; };
    struct DomVecDestructorVariant_NoDestructor { DomVecDestructorTag tag; };
    struct DomVecDestructorVariant_External { DomVecDestructorTag tag; DomVecDestructorType payload; };
    union DomVecDestructor {
        DomVecDestructorVariant_DefaultRust DefaultRust;
        DomVecDestructorVariant_NoDestructor NoDestructor;
        DomVecDestructorVariant_External External;
    };
    
    
    enum class IdOrClassVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct IdOrClassVecDestructorVariant_DefaultRust { IdOrClassVecDestructorTag tag; };
    struct IdOrClassVecDestructorVariant_NoDestructor { IdOrClassVecDestructorTag tag; };
    struct IdOrClassVecDestructorVariant_External { IdOrClassVecDestructorTag tag; IdOrClassVecDestructorType payload; };
    union IdOrClassVecDestructor {
        IdOrClassVecDestructorVariant_DefaultRust DefaultRust;
        IdOrClassVecDestructorVariant_NoDestructor NoDestructor;
        IdOrClassVecDestructorVariant_External External;
    };
    
    
    enum class NodeDataInlineCssPropertyVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NodeDataInlineCssPropertyVecDestructorVariant_DefaultRust { NodeDataInlineCssPropertyVecDestructorTag tag; };
    struct NodeDataInlineCssPropertyVecDestructorVariant_NoDestructor { NodeDataInlineCssPropertyVecDestructorTag tag; };
    struct NodeDataInlineCssPropertyVecDestructorVariant_External { NodeDataInlineCssPropertyVecDestructorTag tag; NodeDataInlineCssPropertyVecDestructorType payload; };
    union NodeDataInlineCssPropertyVecDestructor {
        NodeDataInlineCssPropertyVecDestructorVariant_DefaultRust DefaultRust;
        NodeDataInlineCssPropertyVecDestructorVariant_NoDestructor NoDestructor;
        NodeDataInlineCssPropertyVecDestructorVariant_External External;
    };
    
    
    enum class StyleBackgroundContentVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyleBackgroundContentVecDestructorVariant_DefaultRust { StyleBackgroundContentVecDestructorTag tag; };
    struct StyleBackgroundContentVecDestructorVariant_NoDestructor { StyleBackgroundContentVecDestructorTag tag; };
    struct StyleBackgroundContentVecDestructorVariant_External { StyleBackgroundContentVecDestructorTag tag; StyleBackgroundContentVecDestructorType payload; };
    union StyleBackgroundContentVecDestructor {
        StyleBackgroundContentVecDestructorVariant_DefaultRust DefaultRust;
        StyleBackgroundContentVecDestructorVariant_NoDestructor NoDestructor;
        StyleBackgroundContentVecDestructorVariant_External External;
    };
    
    
    enum class StyleBackgroundPositionVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyleBackgroundPositionVecDestructorVariant_DefaultRust { StyleBackgroundPositionVecDestructorTag tag; };
    struct StyleBackgroundPositionVecDestructorVariant_NoDestructor { StyleBackgroundPositionVecDestructorTag tag; };
    struct StyleBackgroundPositionVecDestructorVariant_External { StyleBackgroundPositionVecDestructorTag tag; StyleBackgroundPositionVecDestructorType payload; };
    union StyleBackgroundPositionVecDestructor {
        StyleBackgroundPositionVecDestructorVariant_DefaultRust DefaultRust;
        StyleBackgroundPositionVecDestructorVariant_NoDestructor NoDestructor;
        StyleBackgroundPositionVecDestructorVariant_External External;
    };
    
    
    enum class StyleBackgroundRepeatVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyleBackgroundRepeatVecDestructorVariant_DefaultRust { StyleBackgroundRepeatVecDestructorTag tag; };
    struct StyleBackgroundRepeatVecDestructorVariant_NoDestructor { StyleBackgroundRepeatVecDestructorTag tag; };
    struct StyleBackgroundRepeatVecDestructorVariant_External { StyleBackgroundRepeatVecDestructorTag tag; StyleBackgroundRepeatVecDestructorType payload; };
    union StyleBackgroundRepeatVecDestructor {
        StyleBackgroundRepeatVecDestructorVariant_DefaultRust DefaultRust;
        StyleBackgroundRepeatVecDestructorVariant_NoDestructor NoDestructor;
        StyleBackgroundRepeatVecDestructorVariant_External External;
    };
    
    
    enum class StyleBackgroundSizeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyleBackgroundSizeVecDestructorVariant_DefaultRust { StyleBackgroundSizeVecDestructorTag tag; };
    struct StyleBackgroundSizeVecDestructorVariant_NoDestructor { StyleBackgroundSizeVecDestructorTag tag; };
    struct StyleBackgroundSizeVecDestructorVariant_External { StyleBackgroundSizeVecDestructorTag tag; StyleBackgroundSizeVecDestructorType payload; };
    union StyleBackgroundSizeVecDestructor {
        StyleBackgroundSizeVecDestructorVariant_DefaultRust DefaultRust;
        StyleBackgroundSizeVecDestructorVariant_NoDestructor NoDestructor;
        StyleBackgroundSizeVecDestructorVariant_External External;
    };
    
    
    enum class StyleTransformVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyleTransformVecDestructorVariant_DefaultRust { StyleTransformVecDestructorTag tag; };
    struct StyleTransformVecDestructorVariant_NoDestructor { StyleTransformVecDestructorTag tag; };
    struct StyleTransformVecDestructorVariant_External { StyleTransformVecDestructorTag tag; StyleTransformVecDestructorType payload; };
    union StyleTransformVecDestructor {
        StyleTransformVecDestructorVariant_DefaultRust DefaultRust;
        StyleTransformVecDestructorVariant_NoDestructor NoDestructor;
        StyleTransformVecDestructorVariant_External External;
    };
    
    
    enum class CssPropertyVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct CssPropertyVecDestructorVariant_DefaultRust { CssPropertyVecDestructorTag tag; };
    struct CssPropertyVecDestructorVariant_NoDestructor { CssPropertyVecDestructorTag tag; };
    struct CssPropertyVecDestructorVariant_External { CssPropertyVecDestructorTag tag; CssPropertyVecDestructorType payload; };
    union CssPropertyVecDestructor {
        CssPropertyVecDestructorVariant_DefaultRust DefaultRust;
        CssPropertyVecDestructorVariant_NoDestructor NoDestructor;
        CssPropertyVecDestructorVariant_External External;
    };
    
    
    enum class SvgMultiPolygonVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct SvgMultiPolygonVecDestructorVariant_DefaultRust { SvgMultiPolygonVecDestructorTag tag; };
    struct SvgMultiPolygonVecDestructorVariant_NoDestructor { SvgMultiPolygonVecDestructorTag tag; };
    struct SvgMultiPolygonVecDestructorVariant_External { SvgMultiPolygonVecDestructorTag tag; SvgMultiPolygonVecDestructorType payload; };
    union SvgMultiPolygonVecDestructor {
        SvgMultiPolygonVecDestructorVariant_DefaultRust DefaultRust;
        SvgMultiPolygonVecDestructorVariant_NoDestructor NoDestructor;
        SvgMultiPolygonVecDestructorVariant_External External;
    };
    
    
    enum class SvgSimpleNodeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct SvgSimpleNodeVecDestructorVariant_DefaultRust { SvgSimpleNodeVecDestructorTag tag; };
    struct SvgSimpleNodeVecDestructorVariant_NoDestructor { SvgSimpleNodeVecDestructorTag tag; };
    struct SvgSimpleNodeVecDestructorVariant_External { SvgSimpleNodeVecDestructorTag tag; SvgSimpleNodeVecDestructorType payload; };
    union SvgSimpleNodeVecDestructor {
        SvgSimpleNodeVecDestructorVariant_DefaultRust DefaultRust;
        SvgSimpleNodeVecDestructorVariant_NoDestructor NoDestructor;
        SvgSimpleNodeVecDestructorVariant_External External;
    };
    
    
    enum class SvgPathVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct SvgPathVecDestructorVariant_DefaultRust { SvgPathVecDestructorTag tag; };
    struct SvgPathVecDestructorVariant_NoDestructor { SvgPathVecDestructorTag tag; };
    struct SvgPathVecDestructorVariant_External { SvgPathVecDestructorTag tag; SvgPathVecDestructorType payload; };
    union SvgPathVecDestructor {
        SvgPathVecDestructorVariant_DefaultRust DefaultRust;
        SvgPathVecDestructorVariant_NoDestructor NoDestructor;
        SvgPathVecDestructorVariant_External External;
    };
    
    
    enum class VertexAttributeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct VertexAttributeVecDestructorVariant_DefaultRust { VertexAttributeVecDestructorTag tag; };
    struct VertexAttributeVecDestructorVariant_NoDestructor { VertexAttributeVecDestructorTag tag; };
    struct VertexAttributeVecDestructorVariant_External { VertexAttributeVecDestructorTag tag; VertexAttributeVecDestructorType payload; };
    union VertexAttributeVecDestructor {
        VertexAttributeVecDestructorVariant_DefaultRust DefaultRust;
        VertexAttributeVecDestructorVariant_NoDestructor NoDestructor;
        VertexAttributeVecDestructorVariant_External External;
    };
    
    
    enum class SvgPathElementVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct SvgPathElementVecDestructorVariant_DefaultRust { SvgPathElementVecDestructorTag tag; };
    struct SvgPathElementVecDestructorVariant_NoDestructor { SvgPathElementVecDestructorTag tag; };
    struct SvgPathElementVecDestructorVariant_External { SvgPathElementVecDestructorTag tag; SvgPathElementVecDestructorType payload; };
    union SvgPathElementVecDestructor {
        SvgPathElementVecDestructorVariant_DefaultRust DefaultRust;
        SvgPathElementVecDestructorVariant_NoDestructor NoDestructor;
        SvgPathElementVecDestructorVariant_External External;
    };
    
    
    enum class SvgVertexVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct SvgVertexVecDestructorVariant_DefaultRust { SvgVertexVecDestructorTag tag; };
    struct SvgVertexVecDestructorVariant_NoDestructor { SvgVertexVecDestructorTag tag; };
    struct SvgVertexVecDestructorVariant_External { SvgVertexVecDestructorTag tag; SvgVertexVecDestructorType payload; };
    union SvgVertexVecDestructor {
        SvgVertexVecDestructorVariant_DefaultRust DefaultRust;
        SvgVertexVecDestructorVariant_NoDestructor NoDestructor;
        SvgVertexVecDestructorVariant_External External;
    };
    
    
    enum class U32VecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct U32VecDestructorVariant_DefaultRust { U32VecDestructorTag tag; };
    struct U32VecDestructorVariant_NoDestructor { U32VecDestructorTag tag; };
    struct U32VecDestructorVariant_External { U32VecDestructorTag tag; U32VecDestructorType payload; };
    union U32VecDestructor {
        U32VecDestructorVariant_DefaultRust DefaultRust;
        U32VecDestructorVariant_NoDestructor NoDestructor;
        U32VecDestructorVariant_External External;
    };
    
    
    enum class XWindowTypeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct XWindowTypeVecDestructorVariant_DefaultRust { XWindowTypeVecDestructorTag tag; };
    struct XWindowTypeVecDestructorVariant_NoDestructor { XWindowTypeVecDestructorTag tag; };
    struct XWindowTypeVecDestructorVariant_External { XWindowTypeVecDestructorTag tag; XWindowTypeVecDestructorType payload; };
    union XWindowTypeVecDestructor {
        XWindowTypeVecDestructorVariant_DefaultRust DefaultRust;
        XWindowTypeVecDestructorVariant_NoDestructor NoDestructor;
        XWindowTypeVecDestructorVariant_External External;
    };
    
    
    enum class VirtualKeyCodeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct VirtualKeyCodeVecDestructorVariant_DefaultRust { VirtualKeyCodeVecDestructorTag tag; };
    struct VirtualKeyCodeVecDestructorVariant_NoDestructor { VirtualKeyCodeVecDestructorTag tag; };
    struct VirtualKeyCodeVecDestructorVariant_External { VirtualKeyCodeVecDestructorTag tag; VirtualKeyCodeVecDestructorType payload; };
    union VirtualKeyCodeVecDestructor {
        VirtualKeyCodeVecDestructorVariant_DefaultRust DefaultRust;
        VirtualKeyCodeVecDestructorVariant_NoDestructor NoDestructor;
        VirtualKeyCodeVecDestructorVariant_External External;
    };
    
    
    enum class CascadeInfoVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct CascadeInfoVecDestructorVariant_DefaultRust { CascadeInfoVecDestructorTag tag; };
    struct CascadeInfoVecDestructorVariant_NoDestructor { CascadeInfoVecDestructorTag tag; };
    struct CascadeInfoVecDestructorVariant_External { CascadeInfoVecDestructorTag tag; CascadeInfoVecDestructorType payload; };
    union CascadeInfoVecDestructor {
        CascadeInfoVecDestructorVariant_DefaultRust DefaultRust;
        CascadeInfoVecDestructorVariant_NoDestructor NoDestructor;
        CascadeInfoVecDestructorVariant_External External;
    };
    
    
    enum class ScanCodeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct ScanCodeVecDestructorVariant_DefaultRust { ScanCodeVecDestructorTag tag; };
    struct ScanCodeVecDestructorVariant_NoDestructor { ScanCodeVecDestructorTag tag; };
    struct ScanCodeVecDestructorVariant_External { ScanCodeVecDestructorTag tag; ScanCodeVecDestructorType payload; };
    union ScanCodeVecDestructor {
        ScanCodeVecDestructorVariant_DefaultRust DefaultRust;
        ScanCodeVecDestructorVariant_NoDestructor NoDestructor;
        ScanCodeVecDestructorVariant_External External;
    };
    
    
    enum class CssDeclarationVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct CssDeclarationVecDestructorVariant_DefaultRust { CssDeclarationVecDestructorTag tag; };
    struct CssDeclarationVecDestructorVariant_NoDestructor { CssDeclarationVecDestructorTag tag; };
    struct CssDeclarationVecDestructorVariant_External { CssDeclarationVecDestructorTag tag; CssDeclarationVecDestructorType payload; };
    union CssDeclarationVecDestructor {
        CssDeclarationVecDestructorVariant_DefaultRust DefaultRust;
        CssDeclarationVecDestructorVariant_NoDestructor NoDestructor;
        CssDeclarationVecDestructorVariant_External External;
    };
    
    
    enum class CssPathSelectorVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct CssPathSelectorVecDestructorVariant_DefaultRust { CssPathSelectorVecDestructorTag tag; };
    struct CssPathSelectorVecDestructorVariant_NoDestructor { CssPathSelectorVecDestructorTag tag; };
    struct CssPathSelectorVecDestructorVariant_External { CssPathSelectorVecDestructorTag tag; CssPathSelectorVecDestructorType payload; };
    union CssPathSelectorVecDestructor {
        CssPathSelectorVecDestructorVariant_DefaultRust DefaultRust;
        CssPathSelectorVecDestructorVariant_NoDestructor NoDestructor;
        CssPathSelectorVecDestructorVariant_External External;
    };
    
    
    enum class StylesheetVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StylesheetVecDestructorVariant_DefaultRust { StylesheetVecDestructorTag tag; };
    struct StylesheetVecDestructorVariant_NoDestructor { StylesheetVecDestructorTag tag; };
    struct StylesheetVecDestructorVariant_External { StylesheetVecDestructorTag tag; StylesheetVecDestructorType payload; };
    union StylesheetVecDestructor {
        StylesheetVecDestructorVariant_DefaultRust DefaultRust;
        StylesheetVecDestructorVariant_NoDestructor NoDestructor;
        StylesheetVecDestructorVariant_External External;
    };
    
    
    enum class CssRuleBlockVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct CssRuleBlockVecDestructorVariant_DefaultRust { CssRuleBlockVecDestructorTag tag; };
    struct CssRuleBlockVecDestructorVariant_NoDestructor { CssRuleBlockVecDestructorTag tag; };
    struct CssRuleBlockVecDestructorVariant_External { CssRuleBlockVecDestructorTag tag; CssRuleBlockVecDestructorType payload; };
    union CssRuleBlockVecDestructor {
        CssRuleBlockVecDestructorVariant_DefaultRust DefaultRust;
        CssRuleBlockVecDestructorVariant_NoDestructor NoDestructor;
        CssRuleBlockVecDestructorVariant_External External;
    };
    
    
    enum class F32VecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct F32VecDestructorVariant_DefaultRust { F32VecDestructorTag tag; };
    struct F32VecDestructorVariant_NoDestructor { F32VecDestructorTag tag; };
    struct F32VecDestructorVariant_External { F32VecDestructorTag tag; F32VecDestructorType payload; };
    union F32VecDestructor {
        F32VecDestructorVariant_DefaultRust DefaultRust;
        F32VecDestructorVariant_NoDestructor NoDestructor;
        F32VecDestructorVariant_External External;
    };
    
    
    enum class U16VecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct U16VecDestructorVariant_DefaultRust { U16VecDestructorTag tag; };
    struct U16VecDestructorVariant_NoDestructor { U16VecDestructorTag tag; };
    struct U16VecDestructorVariant_External { U16VecDestructorTag tag; U16VecDestructorType payload; };
    union U16VecDestructor {
        U16VecDestructorVariant_DefaultRust DefaultRust;
        U16VecDestructorVariant_NoDestructor NoDestructor;
        U16VecDestructorVariant_External External;
    };
    
    
    enum class U8VecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct U8VecDestructorVariant_DefaultRust { U8VecDestructorTag tag; };
    struct U8VecDestructorVariant_NoDestructor { U8VecDestructorTag tag; };
    struct U8VecDestructorVariant_External { U8VecDestructorTag tag; U8VecDestructorType payload; };
    union U8VecDestructor {
        U8VecDestructorVariant_DefaultRust DefaultRust;
        U8VecDestructorVariant_NoDestructor NoDestructor;
        U8VecDestructorVariant_External External;
    };
    
    
    enum class CallbackDataVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct CallbackDataVecDestructorVariant_DefaultRust { CallbackDataVecDestructorTag tag; };
    struct CallbackDataVecDestructorVariant_NoDestructor { CallbackDataVecDestructorTag tag; };
    struct CallbackDataVecDestructorVariant_External { CallbackDataVecDestructorTag tag; CallbackDataVecDestructorType payload; };
    union CallbackDataVecDestructor {
        CallbackDataVecDestructorVariant_DefaultRust DefaultRust;
        CallbackDataVecDestructorVariant_NoDestructor NoDestructor;
        CallbackDataVecDestructorVariant_External External;
    };
    
    
    enum class DebugMessageVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct DebugMessageVecDestructorVariant_DefaultRust { DebugMessageVecDestructorTag tag; };
    struct DebugMessageVecDestructorVariant_NoDestructor { DebugMessageVecDestructorTag tag; };
    struct DebugMessageVecDestructorVariant_External { DebugMessageVecDestructorTag tag; DebugMessageVecDestructorType payload; };
    union DebugMessageVecDestructor {
        DebugMessageVecDestructorVariant_DefaultRust DefaultRust;
        DebugMessageVecDestructorVariant_NoDestructor NoDestructor;
        DebugMessageVecDestructorVariant_External External;
    };
    
    
    enum class GLuintVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct GLuintVecDestructorVariant_DefaultRust { GLuintVecDestructorTag tag; };
    struct GLuintVecDestructorVariant_NoDestructor { GLuintVecDestructorTag tag; };
    struct GLuintVecDestructorVariant_External { GLuintVecDestructorTag tag; GLuintVecDestructorType payload; };
    union GLuintVecDestructor {
        GLuintVecDestructorVariant_DefaultRust DefaultRust;
        GLuintVecDestructorVariant_NoDestructor NoDestructor;
        GLuintVecDestructorVariant_External External;
    };
    
    
    enum class GLintVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct GLintVecDestructorVariant_DefaultRust { GLintVecDestructorTag tag; };
    struct GLintVecDestructorVariant_NoDestructor { GLintVecDestructorTag tag; };
    struct GLintVecDestructorVariant_External { GLintVecDestructorTag tag; GLintVecDestructorType payload; };
    union GLintVecDestructor {
        GLintVecDestructorVariant_DefaultRust DefaultRust;
        GLintVecDestructorVariant_NoDestructor NoDestructor;
        GLintVecDestructorVariant_External External;
    };
    
    
    enum class StringVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StringVecDestructorVariant_DefaultRust { StringVecDestructorTag tag; };
    struct StringVecDestructorVariant_NoDestructor { StringVecDestructorTag tag; };
    struct StringVecDestructorVariant_External { StringVecDestructorTag tag; StringVecDestructorType payload; };
    union StringVecDestructor {
        StringVecDestructorVariant_DefaultRust DefaultRust;
        StringVecDestructorVariant_NoDestructor NoDestructor;
        StringVecDestructorVariant_External External;
    };
    
    
    enum class StringPairVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StringPairVecDestructorVariant_DefaultRust { StringPairVecDestructorTag tag; };
    struct StringPairVecDestructorVariant_NoDestructor { StringPairVecDestructorTag tag; };
    struct StringPairVecDestructorVariant_External { StringPairVecDestructorTag tag; StringPairVecDestructorType payload; };
    union StringPairVecDestructor {
        StringPairVecDestructorVariant_DefaultRust DefaultRust;
        StringPairVecDestructorVariant_NoDestructor NoDestructor;
        StringPairVecDestructorVariant_External External;
    };
    
    
    enum class NormalizedLinearColorStopVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NormalizedLinearColorStopVecDestructorVariant_DefaultRust { NormalizedLinearColorStopVecDestructorTag tag; };
    struct NormalizedLinearColorStopVecDestructorVariant_NoDestructor { NormalizedLinearColorStopVecDestructorTag tag; };
    struct NormalizedLinearColorStopVecDestructorVariant_External { NormalizedLinearColorStopVecDestructorTag tag; NormalizedLinearColorStopVecDestructorType payload; };
    union NormalizedLinearColorStopVecDestructor {
        NormalizedLinearColorStopVecDestructorVariant_DefaultRust DefaultRust;
        NormalizedLinearColorStopVecDestructorVariant_NoDestructor NoDestructor;
        NormalizedLinearColorStopVecDestructorVariant_External External;
    };
    
    
    enum class NormalizedRadialColorStopVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NormalizedRadialColorStopVecDestructorVariant_DefaultRust { NormalizedRadialColorStopVecDestructorTag tag; };
    struct NormalizedRadialColorStopVecDestructorVariant_NoDestructor { NormalizedRadialColorStopVecDestructorTag tag; };
    struct NormalizedRadialColorStopVecDestructorVariant_External { NormalizedRadialColorStopVecDestructorTag tag; NormalizedRadialColorStopVecDestructorType payload; };
    union NormalizedRadialColorStopVecDestructor {
        NormalizedRadialColorStopVecDestructorVariant_DefaultRust DefaultRust;
        NormalizedRadialColorStopVecDestructorVariant_NoDestructor NoDestructor;
        NormalizedRadialColorStopVecDestructorVariant_External External;
    };
    
    
    enum class NodeIdVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NodeIdVecDestructorVariant_DefaultRust { NodeIdVecDestructorTag tag; };
    struct NodeIdVecDestructorVariant_NoDestructor { NodeIdVecDestructorTag tag; };
    struct NodeIdVecDestructorVariant_External { NodeIdVecDestructorTag tag; NodeIdVecDestructorType payload; };
    union NodeIdVecDestructor {
        NodeIdVecDestructorVariant_DefaultRust DefaultRust;
        NodeIdVecDestructorVariant_NoDestructor NoDestructor;
        NodeIdVecDestructorVariant_External External;
    };
    
    
    enum class NodeHierarchyItemVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NodeHierarchyItemVecDestructorVariant_DefaultRust { NodeHierarchyItemVecDestructorTag tag; };
    struct NodeHierarchyItemVecDestructorVariant_NoDestructor { NodeHierarchyItemVecDestructorTag tag; };
    struct NodeHierarchyItemVecDestructorVariant_External { NodeHierarchyItemVecDestructorTag tag; NodeHierarchyItemVecDestructorType payload; };
    union NodeHierarchyItemVecDestructor {
        NodeHierarchyItemVecDestructorVariant_DefaultRust DefaultRust;
        NodeHierarchyItemVecDestructorVariant_NoDestructor NoDestructor;
        NodeHierarchyItemVecDestructorVariant_External External;
    };
    
    
    enum class StyledNodeVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct StyledNodeVecDestructorVariant_DefaultRust { StyledNodeVecDestructorTag tag; };
    struct StyledNodeVecDestructorVariant_NoDestructor { StyledNodeVecDestructorTag tag; };
    struct StyledNodeVecDestructorVariant_External { StyledNodeVecDestructorTag tag; StyledNodeVecDestructorType payload; };
    union StyledNodeVecDestructor {
        StyledNodeVecDestructorVariant_DefaultRust DefaultRust;
        StyledNodeVecDestructorVariant_NoDestructor NoDestructor;
        StyledNodeVecDestructorVariant_External External;
    };
    
    
    enum class TagIdToNodeIdMappingVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct TagIdToNodeIdMappingVecDestructorVariant_DefaultRust { TagIdToNodeIdMappingVecDestructorTag tag; };
    struct TagIdToNodeIdMappingVecDestructorVariant_NoDestructor { TagIdToNodeIdMappingVecDestructorTag tag; };
    struct TagIdToNodeIdMappingVecDestructorVariant_External { TagIdToNodeIdMappingVecDestructorTag tag; TagIdToNodeIdMappingVecDestructorType payload; };
    union TagIdToNodeIdMappingVecDestructor {
        TagIdToNodeIdMappingVecDestructorVariant_DefaultRust DefaultRust;
        TagIdToNodeIdMappingVecDestructorVariant_NoDestructor NoDestructor;
        TagIdToNodeIdMappingVecDestructorVariant_External External;
    };
    
    
    enum class ParentWithNodeDepthVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct ParentWithNodeDepthVecDestructorVariant_DefaultRust { ParentWithNodeDepthVecDestructorTag tag; };
    struct ParentWithNodeDepthVecDestructorVariant_NoDestructor { ParentWithNodeDepthVecDestructorTag tag; };
    struct ParentWithNodeDepthVecDestructorVariant_External { ParentWithNodeDepthVecDestructorTag tag; ParentWithNodeDepthVecDestructorType payload; };
    union ParentWithNodeDepthVecDestructor {
        ParentWithNodeDepthVecDestructorVariant_DefaultRust DefaultRust;
        ParentWithNodeDepthVecDestructorVariant_NoDestructor NoDestructor;
        ParentWithNodeDepthVecDestructorVariant_External External;
    };
    
    
    enum class NodeDataVecDestructorTag {
       DefaultRust,
       NoDestructor,
       External,
    };
    
    struct NodeDataVecDestructorVariant_DefaultRust { NodeDataVecDestructorTag tag; };
    struct NodeDataVecDestructorVariant_NoDestructor { NodeDataVecDestructorTag tag; };
    struct NodeDataVecDestructorVariant_External { NodeDataVecDestructorTag tag; NodeDataVecDestructorType payload; };
    union NodeDataVecDestructor {
        NodeDataVecDestructorVariant_DefaultRust DefaultRust;
        NodeDataVecDestructorVariant_NoDestructor NoDestructor;
        NodeDataVecDestructorVariant_External External;
    };
    
    
    enum class OptionI16Tag {
       None,
       Some,
    };
    
    struct OptionI16Variant_None { OptionI16Tag tag; };
    struct OptionI16Variant_Some { OptionI16Tag tag; int16_t payload; };
    union OptionI16 {
        OptionI16Variant_None None;
        OptionI16Variant_Some Some;
    };
    
    
    enum class OptionU16Tag {
       None,
       Some,
    };
    
    struct OptionU16Variant_None { OptionU16Tag tag; };
    struct OptionU16Variant_Some { OptionU16Tag tag; uint16_t payload; };
    union OptionU16 {
        OptionU16Variant_None None;
        OptionU16Variant_Some Some;
    };
    
    
    enum class OptionU32Tag {
       None,
       Some,
    };
    
    struct OptionU32Variant_None { OptionU32Tag tag; };
    struct OptionU32Variant_Some { OptionU32Tag tag; uint32_t payload; };
    union OptionU32 {
        OptionU32Variant_None None;
        OptionU32Variant_Some Some;
    };
    
    
    enum class OptionHwndHandleTag {
       None,
       Some,
    };
    
    struct OptionHwndHandleVariant_None { OptionHwndHandleTag tag; };
    struct OptionHwndHandleVariant_Some { OptionHwndHandleTag tag; void* restrict payload; };
    union OptionHwndHandle {
        OptionHwndHandleVariant_None None;
        OptionHwndHandleVariant_Some Some;
    };
    
    
    enum class OptionX11VisualTag {
       None,
       Some,
    };
    
    struct OptionX11VisualVariant_None { OptionX11VisualTag tag; };
    struct OptionX11VisualVariant_Some { OptionX11VisualTag tag; void* payload; };
    union OptionX11Visual {
        OptionX11VisualVariant_None None;
        OptionX11VisualVariant_Some Some;
    };
    
    
    enum class OptionI32Tag {
       None,
       Some,
    };
    
    struct OptionI32Variant_None { OptionI32Tag tag; };
    struct OptionI32Variant_Some { OptionI32Tag tag; int32_t payload; };
    union OptionI32 {
        OptionI32Variant_None None;
        OptionI32Variant_Some Some;
    };
    
    
    enum class OptionF32Tag {
       None,
       Some,
    };
    
    struct OptionF32Variant_None { OptionF32Tag tag; };
    struct OptionF32Variant_Some { OptionF32Tag tag; float payload; };
    union OptionF32 {
        OptionF32Variant_None None;
        OptionF32Variant_Some Some;
    };
    
    
    enum class OptionCharTag {
       None,
       Some,
    };
    
    struct OptionCharVariant_None { OptionCharTag tag; };
    struct OptionCharVariant_Some { OptionCharTag tag; uint32_t payload; };
    union OptionChar {
        OptionCharVariant_None None;
        OptionCharVariant_Some Some;
    };
    
    
    enum class OptionUsizeTag {
       None,
       Some,
    };
    
    struct OptionUsizeVariant_None { OptionUsizeTag tag; };
    struct OptionUsizeVariant_Some { OptionUsizeTag tag; size_t payload; };
    union OptionUsize {
        OptionUsizeVariant_None None;
        OptionUsizeVariant_Some Some;
    };
    
    
    struct SvgParseErrorPosition {
        uint32_t row;
        uint32_t col;
        SvgParseErrorPosition& operator=(const SvgParseErrorPosition&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgParseErrorPosition() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SystemCallbacks {
        CreateThreadFn create_thread_fn;
        GetSystemTimeFn get_system_time_fn;
        SystemCallbacks& operator=(const SystemCallbacks&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SystemCallbacks(const SystemCallbacks&) = delete; /* disable copy constructor, use explicit .clone() */
        SystemCallbacks() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RendererOptions {
        Vsync vsync;
        Srgb srgb;
        HwAcceleration hw_accel;
        RendererOptions& operator=(const RendererOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RendererOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutRect {
        LayoutPoint origin;
        LayoutSize size;
        LayoutRect& operator=(const LayoutRect&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutRect() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class RawWindowHandleTag {
       IOS,
       MacOS,
       Xlib,
       Xcb,
       Wayland,
       Windows,
       Web,
       Android,
       Unsupported,
    };
    
    struct RawWindowHandleVariant_IOS { RawWindowHandleTag tag; IOSHandle payload; };
    struct RawWindowHandleVariant_MacOS { RawWindowHandleTag tag; MacOSHandle payload; };
    struct RawWindowHandleVariant_Xlib { RawWindowHandleTag tag; XlibHandle payload; };
    struct RawWindowHandleVariant_Xcb { RawWindowHandleTag tag; XcbHandle payload; };
    struct RawWindowHandleVariant_Wayland { RawWindowHandleTag tag; WaylandHandle payload; };
    struct RawWindowHandleVariant_Windows { RawWindowHandleTag tag; WindowsHandle payload; };
    struct RawWindowHandleVariant_Web { RawWindowHandleTag tag; WebHandle payload; };
    struct RawWindowHandleVariant_Android { RawWindowHandleTag tag; AndroidHandle payload; };
    struct RawWindowHandleVariant_Unsupported { RawWindowHandleTag tag; };
    union RawWindowHandle {
        RawWindowHandleVariant_IOS IOS;
        RawWindowHandleVariant_MacOS MacOS;
        RawWindowHandleVariant_Xlib Xlib;
        RawWindowHandleVariant_Xcb Xcb;
        RawWindowHandleVariant_Wayland Wayland;
        RawWindowHandleVariant_Windows Windows;
        RawWindowHandleVariant_Web Web;
        RawWindowHandleVariant_Android Android;
        RawWindowHandleVariant_Unsupported Unsupported;
    };
    
    
    struct LogicalRect {
        LogicalPosition origin;
        LogicalSize size;
        LogicalRect& operator=(const LogicalRect&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LogicalRect() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class AcceleratorKeyTag {
       Ctrl,
       Alt,
       Shift,
       Key,
    };
    
    struct AcceleratorKeyVariant_Ctrl { AcceleratorKeyTag tag; };
    struct AcceleratorKeyVariant_Alt { AcceleratorKeyTag tag; };
    struct AcceleratorKeyVariant_Shift { AcceleratorKeyTag tag; };
    struct AcceleratorKeyVariant_Key { AcceleratorKeyTag tag; VirtualKeyCode payload; };
    union AcceleratorKey {
        AcceleratorKeyVariant_Ctrl Ctrl;
        AcceleratorKeyVariant_Alt Alt;
        AcceleratorKeyVariant_Shift Shift;
        AcceleratorKeyVariant_Key Key;
    };
    
    
    struct WindowFlags {
        WindowFrame frame;
        bool  is_about_to_close;
        bool  has_decorations;
        bool  is_visible;
        bool  is_always_on_top;
        bool  is_resizable;
        bool  has_focus;
        bool  has_extended_window_frame;
        bool  has_blur_behind_window;
        bool  smooth_scroll_enabled;
        bool  autotab_enabled;
        WindowFlags& operator=(const WindowFlags&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WindowFlags() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class CursorPositionTag {
       OutOfWindow,
       Uninitialized,
       InWindow,
    };
    
    struct CursorPositionVariant_OutOfWindow { CursorPositionTag tag; LogicalPosition payload; };
    struct CursorPositionVariant_Uninitialized { CursorPositionTag tag; };
    struct CursorPositionVariant_InWindow { CursorPositionTag tag; LogicalPosition payload; };
    union CursorPosition {
        CursorPositionVariant_OutOfWindow OutOfWindow;
        CursorPositionVariant_Uninitialized Uninitialized;
        CursorPositionVariant_InWindow InWindow;
    };
    
    
    enum class WindowPositionTag {
       Uninitialized,
       Initialized,
    };
    
    struct WindowPositionVariant_Uninitialized { WindowPositionTag tag; };
    struct WindowPositionVariant_Initialized { WindowPositionTag tag; PhysicalPositionI32 payload; };
    union WindowPosition {
        WindowPositionVariant_Uninitialized Uninitialized;
        WindowPositionVariant_Initialized Initialized;
    };
    
    
    enum class ImePositionTag {
       Uninitialized,
       Initialized,
    };
    
    struct ImePositionVariant_Uninitialized { ImePositionTag tag; };
    struct ImePositionVariant_Initialized { ImePositionTag tag; LogicalPosition payload; };
    union ImePosition {
        ImePositionVariant_Uninitialized Uninitialized;
        ImePositionVariant_Initialized Initialized;
    };
    
    
    struct VideoMode {
        LayoutSize size;
        uint16_t bit_depth;
        uint16_t refresh_rate;
        VideoMode& operator=(const VideoMode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VideoMode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DomNodeId {
        DomId dom;
        NodeId node;
        DomNodeId& operator=(const DomNodeId&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DomNodeId() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class PositionInfoTag {
       Static,
       Fixed,
       Absolute,
       Relative,
    };
    
    struct PositionInfoVariant_Static { PositionInfoTag tag; PositionInfoInner payload; };
    struct PositionInfoVariant_Fixed { PositionInfoTag tag; PositionInfoInner payload; };
    struct PositionInfoVariant_Absolute { PositionInfoTag tag; PositionInfoInner payload; };
    struct PositionInfoVariant_Relative { PositionInfoTag tag; PositionInfoInner payload; };
    union PositionInfo {
        PositionInfoVariant_Static Static;
        PositionInfoVariant_Fixed Fixed;
        PositionInfoVariant_Absolute Absolute;
        PositionInfoVariant_Relative Relative;
    };
    
    
    struct HidpiAdjustedBounds {
        LogicalSize logical_size;
        float hidpi_factor;
        HidpiAdjustedBounds& operator=(const HidpiAdjustedBounds&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        HidpiAdjustedBounds() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineGlyph {
        LogicalRect bounds;
        OptionChar unicode_codepoint;
        uint32_t glyph_index;
        InlineGlyph& operator=(const InlineGlyph&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineGlyph(const InlineGlyph&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineGlyph() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineTextHit {
        OptionChar unicode_codepoint;
        LogicalPosition hit_relative_to_inline_text;
        LogicalPosition hit_relative_to_line;
        LogicalPosition hit_relative_to_text_content;
        LogicalPosition hit_relative_to_glyph;
        size_t line_index_relative_to_text;
        size_t word_index_relative_to_text;
        size_t text_content_index_relative_to_text;
        size_t glyph_index_relative_to_text;
        size_t char_index_relative_to_text;
        size_t word_index_relative_to_line;
        size_t text_content_index_relative_to_line;
        size_t glyph_index_relative_to_line;
        size_t char_index_relative_to_line;
        size_t glyph_index_relative_to_word;
        size_t char_index_relative_to_word;
        InlineTextHit& operator=(const InlineTextHit&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineTextHit(const InlineTextHit&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineTextHit() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct IFrameCallbackInfo {
        void* system_fonts;
        void* image_cache;
        WindowTheme window_theme;
        HidpiAdjustedBounds bounds;
        LogicalSize scroll_size;
        LogicalPosition scroll_offset;
        LogicalSize virtual_scroll_size;
        LogicalPosition virtual_scroll_offset;
        void* _reserved_ref;
        void* restrict _reserved_mut;
        IFrameCallbackInfo& operator=(const IFrameCallbackInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        IFrameCallbackInfo(const IFrameCallbackInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        IFrameCallbackInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TimerCallbackReturn {
        Update should_update;
        TerminateTimer should_terminate;
        TimerCallbackReturn& operator=(const TimerCallbackReturn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TimerCallbackReturn(const TimerCallbackReturn&) = delete; /* disable copy constructor, use explicit .clone() */
        TimerCallbackReturn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RefAny {
        void* _internal_ptr;
        RefCount sharing_info;
        uint64_t instance_id;
        bool  run_destructor;
        RefAny& operator=(const RefAny&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RefAny(const RefAny&) = delete; /* disable copy constructor, use explicit .clone() */
        RefAny() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct IFrameNode {
        IFrameCallback callback;
        RefAny data;
        IFrameNode& operator=(const IFrameNode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        IFrameNode(const IFrameNode&) = delete; /* disable copy constructor, use explicit .clone() */
        IFrameNode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class NotEventFilterTag {
       Hover,
       Focus,
    };
    
    struct NotEventFilterVariant_Hover { NotEventFilterTag tag; HoverEventFilter payload; };
    struct NotEventFilterVariant_Focus { NotEventFilterTag tag; FocusEventFilter payload; };
    union NotEventFilter {
        NotEventFilterVariant_Hover Hover;
        NotEventFilterVariant_Focus Focus;
    };
    
    
    struct MenuCallback {
        Callback callback;
        RefAny data;
        MenuCallback& operator=(const MenuCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MenuCallback(const MenuCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        MenuCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class MenuItemIconTag {
       Checkbox,
       Image,
    };
    
    struct MenuItemIconVariant_Checkbox { MenuItemIconTag tag; bool payload; };
    struct MenuItemIconVariant_Image { MenuItemIconTag tag; ImageRef payload; };
    union MenuItemIcon {
        MenuItemIconVariant_Checkbox Checkbox;
        MenuItemIconVariant_Image Image;
    };
    
    
    enum class CssNthChildSelectorTag {
       Number,
       Even,
       Odd,
       Pattern,
    };
    
    struct CssNthChildSelectorVariant_Number { CssNthChildSelectorTag tag; uint32_t payload; };
    struct CssNthChildSelectorVariant_Even { CssNthChildSelectorTag tag; };
    struct CssNthChildSelectorVariant_Odd { CssNthChildSelectorTag tag; };
    struct CssNthChildSelectorVariant_Pattern { CssNthChildSelectorTag tag; CssNthChildPattern payload; };
    union CssNthChildSelector {
        CssNthChildSelectorVariant_Number Number;
        CssNthChildSelectorVariant_Even Even;
        CssNthChildSelectorVariant_Odd Odd;
        CssNthChildSelectorVariant_Pattern Pattern;
    };
    
    
    struct PixelValue {
        SizeMetric metric;
        FloatValue number;
        PixelValue& operator=(const PixelValue&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        PixelValue() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct PixelValueNoPercent {
        PixelValue inner;
        PixelValueNoPercent& operator=(const PixelValueNoPercent&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        PixelValueNoPercent() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBoxShadow {
        PixelValueNoPercent offset[2];
        ColorU color;
        PixelValueNoPercent blur_radius;
        PixelValueNoPercent spread_radius;
        BoxShadowClipMode clip_mode;
        StyleBoxShadow& operator=(const StyleBoxShadow&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBoxShadow() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBlur {
        PixelValue width;
        PixelValue height;
        StyleBlur& operator=(const StyleBlur&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBlur() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleColorMatrix {
        FloatValue matrix[20];
        StyleColorMatrix& operator=(const StyleColorMatrix&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleColorMatrix() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleFilterOffset {
        PixelValue x;
        PixelValue y;
        StyleFilterOffset& operator=(const StyleFilterOffset&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleFilterOffset() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleCompositeFilterTag {
       Over,
       In,
       Atop,
       Out,
       Xor,
       Lighter,
       Arithmetic,
    };
    
    struct StyleCompositeFilterVariant_Over { StyleCompositeFilterTag tag; };
    struct StyleCompositeFilterVariant_In { StyleCompositeFilterTag tag; };
    struct StyleCompositeFilterVariant_Atop { StyleCompositeFilterTag tag; };
    struct StyleCompositeFilterVariant_Out { StyleCompositeFilterTag tag; };
    struct StyleCompositeFilterVariant_Xor { StyleCompositeFilterTag tag; };
    struct StyleCompositeFilterVariant_Lighter { StyleCompositeFilterTag tag; };
    struct StyleCompositeFilterVariant_Arithmetic { StyleCompositeFilterTag tag; FloatValue payload[4]; };
    union StyleCompositeFilter {
        StyleCompositeFilterVariant_Over Over;
        StyleCompositeFilterVariant_In In;
        StyleCompositeFilterVariant_Atop Atop;
        StyleCompositeFilterVariant_Out Out;
        StyleCompositeFilterVariant_Xor Xor;
        StyleCompositeFilterVariant_Lighter Lighter;
        StyleCompositeFilterVariant_Arithmetic Arithmetic;
    };
    
    
    struct LayoutBottom {
        PixelValue inner;
        LayoutBottom& operator=(const LayoutBottom&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutBottom() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutFlexGrow {
        FloatValue inner;
        LayoutFlexGrow& operator=(const LayoutFlexGrow&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutFlexGrow() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutFlexShrink {
        FloatValue inner;
        LayoutFlexShrink& operator=(const LayoutFlexShrink&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutFlexShrink() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutHeight {
        PixelValue inner;
        LayoutHeight& operator=(const LayoutHeight&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutHeight() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutLeft {
        PixelValue inner;
        LayoutLeft& operator=(const LayoutLeft&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutLeft() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMarginBottom {
        PixelValue inner;
        LayoutMarginBottom& operator=(const LayoutMarginBottom&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMarginBottom() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMarginLeft {
        PixelValue inner;
        LayoutMarginLeft& operator=(const LayoutMarginLeft&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMarginLeft() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMarginRight {
        PixelValue inner;
        LayoutMarginRight& operator=(const LayoutMarginRight&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMarginRight() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMarginTop {
        PixelValue inner;
        LayoutMarginTop& operator=(const LayoutMarginTop&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMarginTop() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMaxHeight {
        PixelValue inner;
        LayoutMaxHeight& operator=(const LayoutMaxHeight&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMaxHeight() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMaxWidth {
        PixelValue inner;
        LayoutMaxWidth& operator=(const LayoutMaxWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMaxWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMinHeight {
        PixelValue inner;
        LayoutMinHeight& operator=(const LayoutMinHeight&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMinHeight() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutMinWidth {
        PixelValue inner;
        LayoutMinWidth& operator=(const LayoutMinWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutMinWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutPaddingBottom {
        PixelValue inner;
        LayoutPaddingBottom& operator=(const LayoutPaddingBottom&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutPaddingBottom() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutPaddingLeft {
        PixelValue inner;
        LayoutPaddingLeft& operator=(const LayoutPaddingLeft&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutPaddingLeft() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutPaddingRight {
        PixelValue inner;
        LayoutPaddingRight& operator=(const LayoutPaddingRight&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutPaddingRight() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutPaddingTop {
        PixelValue inner;
        LayoutPaddingTop& operator=(const LayoutPaddingTop&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutPaddingTop() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutRight {
        PixelValue inner;
        LayoutRight& operator=(const LayoutRight&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutRight() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutTop {
        PixelValue inner;
        LayoutTop& operator=(const LayoutTop&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutTop() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutWidth {
        PixelValue inner;
        LayoutWidth& operator=(const LayoutWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct PercentageValue {
        FloatValue number;
        PercentageValue& operator=(const PercentageValue&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        PercentageValue() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct AngleValue {
        AngleMetric metric;
        FloatValue number;
        AngleValue& operator=(const AngleValue&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        AngleValue() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NormalizedLinearColorStop {
        PercentageValue offset;
        ColorU color;
        NormalizedLinearColorStop& operator=(const NormalizedLinearColorStop&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NormalizedLinearColorStop() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NormalizedRadialColorStop {
        AngleValue offset;
        ColorU color;
        NormalizedRadialColorStop& operator=(const NormalizedRadialColorStop&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NormalizedRadialColorStop() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DirectionCorners {
        DirectionCorner from;
        DirectionCorner to;
        DirectionCorners& operator=(const DirectionCorners&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DirectionCorners() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class DirectionTag {
       Angle,
       FromTo,
    };
    
    struct DirectionVariant_Angle { DirectionTag tag; AngleValue payload; };
    struct DirectionVariant_FromTo { DirectionTag tag; DirectionCorners payload; };
    union Direction {
        DirectionVariant_Angle Angle;
        DirectionVariant_FromTo FromTo;
    };
    
    
    enum class BackgroundPositionHorizontalTag {
       Left,
       Center,
       Right,
       Exact,
    };
    
    struct BackgroundPositionHorizontalVariant_Left { BackgroundPositionHorizontalTag tag; };
    struct BackgroundPositionHorizontalVariant_Center { BackgroundPositionHorizontalTag tag; };
    struct BackgroundPositionHorizontalVariant_Right { BackgroundPositionHorizontalTag tag; };
    struct BackgroundPositionHorizontalVariant_Exact { BackgroundPositionHorizontalTag tag; PixelValue payload; };
    union BackgroundPositionHorizontal {
        BackgroundPositionHorizontalVariant_Left Left;
        BackgroundPositionHorizontalVariant_Center Center;
        BackgroundPositionHorizontalVariant_Right Right;
        BackgroundPositionHorizontalVariant_Exact Exact;
    };
    
    
    enum class BackgroundPositionVerticalTag {
       Top,
       Center,
       Bottom,
       Exact,
    };
    
    struct BackgroundPositionVerticalVariant_Top { BackgroundPositionVerticalTag tag; };
    struct BackgroundPositionVerticalVariant_Center { BackgroundPositionVerticalTag tag; };
    struct BackgroundPositionVerticalVariant_Bottom { BackgroundPositionVerticalTag tag; };
    struct BackgroundPositionVerticalVariant_Exact { BackgroundPositionVerticalTag tag; PixelValue payload; };
    union BackgroundPositionVertical {
        BackgroundPositionVerticalVariant_Top Top;
        BackgroundPositionVerticalVariant_Center Center;
        BackgroundPositionVerticalVariant_Bottom Bottom;
        BackgroundPositionVerticalVariant_Exact Exact;
    };
    
    
    struct StyleBackgroundPosition {
        BackgroundPositionHorizontal horizontal;
        BackgroundPositionVertical vertical;
        StyleBackgroundPosition& operator=(const StyleBackgroundPosition&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBackgroundPosition() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleBackgroundSizeTag {
       ExactSize,
       Contain,
       Cover,
    };
    
    struct StyleBackgroundSizeVariant_ExactSize { StyleBackgroundSizeTag tag; PixelValue payload[2]; };
    struct StyleBackgroundSizeVariant_Contain { StyleBackgroundSizeTag tag; };
    struct StyleBackgroundSizeVariant_Cover { StyleBackgroundSizeTag tag; };
    union StyleBackgroundSize {
        StyleBackgroundSizeVariant_ExactSize ExactSize;
        StyleBackgroundSizeVariant_Contain Contain;
        StyleBackgroundSizeVariant_Cover Cover;
    };
    
    
    struct StyleBorderBottomColor {
        ColorU inner;
        StyleBorderBottomColor& operator=(const StyleBorderBottomColor&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderBottomColor() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderBottomLeftRadius {
        PixelValue inner;
        StyleBorderBottomLeftRadius& operator=(const StyleBorderBottomLeftRadius&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderBottomLeftRadius() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderBottomRightRadius {
        PixelValue inner;
        StyleBorderBottomRightRadius& operator=(const StyleBorderBottomRightRadius&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderBottomRightRadius() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderBottomStyle {
        BorderStyle inner;
        StyleBorderBottomStyle& operator=(const StyleBorderBottomStyle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderBottomStyle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutBorderBottomWidth {
        PixelValue inner;
        LayoutBorderBottomWidth& operator=(const LayoutBorderBottomWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutBorderBottomWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderLeftColor {
        ColorU inner;
        StyleBorderLeftColor& operator=(const StyleBorderLeftColor&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderLeftColor() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderLeftStyle {
        BorderStyle inner;
        StyleBorderLeftStyle& operator=(const StyleBorderLeftStyle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderLeftStyle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutBorderLeftWidth {
        PixelValue inner;
        LayoutBorderLeftWidth& operator=(const LayoutBorderLeftWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutBorderLeftWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderRightColor {
        ColorU inner;
        StyleBorderRightColor& operator=(const StyleBorderRightColor&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderRightColor() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderRightStyle {
        BorderStyle inner;
        StyleBorderRightStyle& operator=(const StyleBorderRightStyle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderRightStyle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutBorderRightWidth {
        PixelValue inner;
        LayoutBorderRightWidth& operator=(const LayoutBorderRightWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutBorderRightWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderTopColor {
        ColorU inner;
        StyleBorderTopColor& operator=(const StyleBorderTopColor&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderTopColor() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderTopLeftRadius {
        PixelValue inner;
        StyleBorderTopLeftRadius& operator=(const StyleBorderTopLeftRadius&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderTopLeftRadius() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderTopRightRadius {
        PixelValue inner;
        StyleBorderTopRightRadius& operator=(const StyleBorderTopRightRadius&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderTopRightRadius() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBorderTopStyle {
        BorderStyle inner;
        StyleBorderTopStyle& operator=(const StyleBorderTopStyle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBorderTopStyle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutBorderTopWidth {
        PixelValue inner;
        LayoutBorderTopWidth& operator=(const LayoutBorderTopWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutBorderTopWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleFontSize {
        PixelValue inner;
        StyleFontSize& operator=(const StyleFontSize&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleFontSize() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleLetterSpacing {
        PixelValue inner;
        StyleLetterSpacing& operator=(const StyleLetterSpacing&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleLetterSpacing() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleLineHeight {
        PercentageValue inner;
        StyleLineHeight& operator=(const StyleLineHeight&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleLineHeight() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTabWidth {
        PercentageValue inner;
        StyleTabWidth& operator=(const StyleTabWidth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTabWidth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleOpacity {
        PercentageValue inner;
        StyleOpacity& operator=(const StyleOpacity&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleOpacity() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformOrigin {
        PixelValue x;
        PixelValue y;
        StyleTransformOrigin& operator=(const StyleTransformOrigin&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformOrigin() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StylePerspectiveOrigin {
        PixelValue x;
        PixelValue y;
        StylePerspectiveOrigin& operator=(const StylePerspectiveOrigin&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StylePerspectiveOrigin() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformMatrix2D {
        PixelValue a;
        PixelValue b;
        PixelValue c;
        PixelValue d;
        PixelValue tx;
        PixelValue ty;
        StyleTransformMatrix2D& operator=(const StyleTransformMatrix2D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformMatrix2D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformMatrix3D {
        PixelValue m11;
        PixelValue m12;
        PixelValue m13;
        PixelValue m14;
        PixelValue m21;
        PixelValue m22;
        PixelValue m23;
        PixelValue m24;
        PixelValue m31;
        PixelValue m32;
        PixelValue m33;
        PixelValue m34;
        PixelValue m41;
        PixelValue m42;
        PixelValue m43;
        PixelValue m44;
        StyleTransformMatrix3D& operator=(const StyleTransformMatrix3D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformMatrix3D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformTranslate2D {
        PixelValue x;
        PixelValue y;
        StyleTransformTranslate2D& operator=(const StyleTransformTranslate2D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformTranslate2D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformTranslate3D {
        PixelValue x;
        PixelValue y;
        PixelValue z;
        StyleTransformTranslate3D& operator=(const StyleTransformTranslate3D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformTranslate3D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformRotate3D {
        PercentageValue x;
        PercentageValue y;
        PercentageValue z;
        AngleValue angle;
        StyleTransformRotate3D& operator=(const StyleTransformRotate3D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformRotate3D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformScale2D {
        PercentageValue x;
        PercentageValue y;
        StyleTransformScale2D& operator=(const StyleTransformScale2D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformScale2D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformScale3D {
        PercentageValue x;
        PercentageValue y;
        PercentageValue z;
        StyleTransformScale3D& operator=(const StyleTransformScale3D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformScale3D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformSkew2D {
        PercentageValue x;
        PercentageValue y;
        StyleTransformSkew2D& operator=(const StyleTransformSkew2D&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformSkew2D() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTextColor {
        ColorU inner;
        StyleTextColor& operator=(const StyleTextColor&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTextColor() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleWordSpacing {
        PixelValue inner;
        StyleWordSpacing& operator=(const StyleWordSpacing&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleWordSpacing() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleBoxShadowValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBoxShadowValueVariant_Auto { StyleBoxShadowValueTag tag; };
    struct StyleBoxShadowValueVariant_None { StyleBoxShadowValueTag tag; };
    struct StyleBoxShadowValueVariant_Inherit { StyleBoxShadowValueTag tag; };
    struct StyleBoxShadowValueVariant_Initial { StyleBoxShadowValueTag tag; };
    struct StyleBoxShadowValueVariant_Exact { StyleBoxShadowValueTag tag; StyleBoxShadow payload; };
    union StyleBoxShadowValue {
        StyleBoxShadowValueVariant_Auto Auto;
        StyleBoxShadowValueVariant_None None;
        StyleBoxShadowValueVariant_Inherit Inherit;
        StyleBoxShadowValueVariant_Initial Initial;
        StyleBoxShadowValueVariant_Exact Exact;
    };
    
    
    enum class LayoutAlignContentValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutAlignContentValueVariant_Auto { LayoutAlignContentValueTag tag; };
    struct LayoutAlignContentValueVariant_None { LayoutAlignContentValueTag tag; };
    struct LayoutAlignContentValueVariant_Inherit { LayoutAlignContentValueTag tag; };
    struct LayoutAlignContentValueVariant_Initial { LayoutAlignContentValueTag tag; };
    struct LayoutAlignContentValueVariant_Exact { LayoutAlignContentValueTag tag; LayoutAlignContent payload; };
    union LayoutAlignContentValue {
        LayoutAlignContentValueVariant_Auto Auto;
        LayoutAlignContentValueVariant_None None;
        LayoutAlignContentValueVariant_Inherit Inherit;
        LayoutAlignContentValueVariant_Initial Initial;
        LayoutAlignContentValueVariant_Exact Exact;
    };
    
    
    enum class LayoutAlignItemsValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutAlignItemsValueVariant_Auto { LayoutAlignItemsValueTag tag; };
    struct LayoutAlignItemsValueVariant_None { LayoutAlignItemsValueTag tag; };
    struct LayoutAlignItemsValueVariant_Inherit { LayoutAlignItemsValueTag tag; };
    struct LayoutAlignItemsValueVariant_Initial { LayoutAlignItemsValueTag tag; };
    struct LayoutAlignItemsValueVariant_Exact { LayoutAlignItemsValueTag tag; LayoutAlignItems payload; };
    union LayoutAlignItemsValue {
        LayoutAlignItemsValueVariant_Auto Auto;
        LayoutAlignItemsValueVariant_None None;
        LayoutAlignItemsValueVariant_Inherit Inherit;
        LayoutAlignItemsValueVariant_Initial Initial;
        LayoutAlignItemsValueVariant_Exact Exact;
    };
    
    
    enum class LayoutBottomValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutBottomValueVariant_Auto { LayoutBottomValueTag tag; };
    struct LayoutBottomValueVariant_None { LayoutBottomValueTag tag; };
    struct LayoutBottomValueVariant_Inherit { LayoutBottomValueTag tag; };
    struct LayoutBottomValueVariant_Initial { LayoutBottomValueTag tag; };
    struct LayoutBottomValueVariant_Exact { LayoutBottomValueTag tag; LayoutBottom payload; };
    union LayoutBottomValue {
        LayoutBottomValueVariant_Auto Auto;
        LayoutBottomValueVariant_None None;
        LayoutBottomValueVariant_Inherit Inherit;
        LayoutBottomValueVariant_Initial Initial;
        LayoutBottomValueVariant_Exact Exact;
    };
    
    
    enum class LayoutBoxSizingValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutBoxSizingValueVariant_Auto { LayoutBoxSizingValueTag tag; };
    struct LayoutBoxSizingValueVariant_None { LayoutBoxSizingValueTag tag; };
    struct LayoutBoxSizingValueVariant_Inherit { LayoutBoxSizingValueTag tag; };
    struct LayoutBoxSizingValueVariant_Initial { LayoutBoxSizingValueTag tag; };
    struct LayoutBoxSizingValueVariant_Exact { LayoutBoxSizingValueTag tag; LayoutBoxSizing payload; };
    union LayoutBoxSizingValue {
        LayoutBoxSizingValueVariant_Auto Auto;
        LayoutBoxSizingValueVariant_None None;
        LayoutBoxSizingValueVariant_Inherit Inherit;
        LayoutBoxSizingValueVariant_Initial Initial;
        LayoutBoxSizingValueVariant_Exact Exact;
    };
    
    
    enum class LayoutFlexDirectionValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutFlexDirectionValueVariant_Auto { LayoutFlexDirectionValueTag tag; };
    struct LayoutFlexDirectionValueVariant_None { LayoutFlexDirectionValueTag tag; };
    struct LayoutFlexDirectionValueVariant_Inherit { LayoutFlexDirectionValueTag tag; };
    struct LayoutFlexDirectionValueVariant_Initial { LayoutFlexDirectionValueTag tag; };
    struct LayoutFlexDirectionValueVariant_Exact { LayoutFlexDirectionValueTag tag; LayoutFlexDirection payload; };
    union LayoutFlexDirectionValue {
        LayoutFlexDirectionValueVariant_Auto Auto;
        LayoutFlexDirectionValueVariant_None None;
        LayoutFlexDirectionValueVariant_Inherit Inherit;
        LayoutFlexDirectionValueVariant_Initial Initial;
        LayoutFlexDirectionValueVariant_Exact Exact;
    };
    
    
    enum class LayoutDisplayValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutDisplayValueVariant_Auto { LayoutDisplayValueTag tag; };
    struct LayoutDisplayValueVariant_None { LayoutDisplayValueTag tag; };
    struct LayoutDisplayValueVariant_Inherit { LayoutDisplayValueTag tag; };
    struct LayoutDisplayValueVariant_Initial { LayoutDisplayValueTag tag; };
    struct LayoutDisplayValueVariant_Exact { LayoutDisplayValueTag tag; LayoutDisplay payload; };
    union LayoutDisplayValue {
        LayoutDisplayValueVariant_Auto Auto;
        LayoutDisplayValueVariant_None None;
        LayoutDisplayValueVariant_Inherit Inherit;
        LayoutDisplayValueVariant_Initial Initial;
        LayoutDisplayValueVariant_Exact Exact;
    };
    
    
    enum class LayoutFlexGrowValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutFlexGrowValueVariant_Auto { LayoutFlexGrowValueTag tag; };
    struct LayoutFlexGrowValueVariant_None { LayoutFlexGrowValueTag tag; };
    struct LayoutFlexGrowValueVariant_Inherit { LayoutFlexGrowValueTag tag; };
    struct LayoutFlexGrowValueVariant_Initial { LayoutFlexGrowValueTag tag; };
    struct LayoutFlexGrowValueVariant_Exact { LayoutFlexGrowValueTag tag; LayoutFlexGrow payload; };
    union LayoutFlexGrowValue {
        LayoutFlexGrowValueVariant_Auto Auto;
        LayoutFlexGrowValueVariant_None None;
        LayoutFlexGrowValueVariant_Inherit Inherit;
        LayoutFlexGrowValueVariant_Initial Initial;
        LayoutFlexGrowValueVariant_Exact Exact;
    };
    
    
    enum class LayoutFlexShrinkValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutFlexShrinkValueVariant_Auto { LayoutFlexShrinkValueTag tag; };
    struct LayoutFlexShrinkValueVariant_None { LayoutFlexShrinkValueTag tag; };
    struct LayoutFlexShrinkValueVariant_Inherit { LayoutFlexShrinkValueTag tag; };
    struct LayoutFlexShrinkValueVariant_Initial { LayoutFlexShrinkValueTag tag; };
    struct LayoutFlexShrinkValueVariant_Exact { LayoutFlexShrinkValueTag tag; LayoutFlexShrink payload; };
    union LayoutFlexShrinkValue {
        LayoutFlexShrinkValueVariant_Auto Auto;
        LayoutFlexShrinkValueVariant_None None;
        LayoutFlexShrinkValueVariant_Inherit Inherit;
        LayoutFlexShrinkValueVariant_Initial Initial;
        LayoutFlexShrinkValueVariant_Exact Exact;
    };
    
    
    enum class LayoutFloatValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutFloatValueVariant_Auto { LayoutFloatValueTag tag; };
    struct LayoutFloatValueVariant_None { LayoutFloatValueTag tag; };
    struct LayoutFloatValueVariant_Inherit { LayoutFloatValueTag tag; };
    struct LayoutFloatValueVariant_Initial { LayoutFloatValueTag tag; };
    struct LayoutFloatValueVariant_Exact { LayoutFloatValueTag tag; LayoutFloat payload; };
    union LayoutFloatValue {
        LayoutFloatValueVariant_Auto Auto;
        LayoutFloatValueVariant_None None;
        LayoutFloatValueVariant_Inherit Inherit;
        LayoutFloatValueVariant_Initial Initial;
        LayoutFloatValueVariant_Exact Exact;
    };
    
    
    enum class LayoutHeightValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutHeightValueVariant_Auto { LayoutHeightValueTag tag; };
    struct LayoutHeightValueVariant_None { LayoutHeightValueTag tag; };
    struct LayoutHeightValueVariant_Inherit { LayoutHeightValueTag tag; };
    struct LayoutHeightValueVariant_Initial { LayoutHeightValueTag tag; };
    struct LayoutHeightValueVariant_Exact { LayoutHeightValueTag tag; LayoutHeight payload; };
    union LayoutHeightValue {
        LayoutHeightValueVariant_Auto Auto;
        LayoutHeightValueVariant_None None;
        LayoutHeightValueVariant_Inherit Inherit;
        LayoutHeightValueVariant_Initial Initial;
        LayoutHeightValueVariant_Exact Exact;
    };
    
    
    enum class LayoutJustifyContentValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutJustifyContentValueVariant_Auto { LayoutJustifyContentValueTag tag; };
    struct LayoutJustifyContentValueVariant_None { LayoutJustifyContentValueTag tag; };
    struct LayoutJustifyContentValueVariant_Inherit { LayoutJustifyContentValueTag tag; };
    struct LayoutJustifyContentValueVariant_Initial { LayoutJustifyContentValueTag tag; };
    struct LayoutJustifyContentValueVariant_Exact { LayoutJustifyContentValueTag tag; LayoutJustifyContent payload; };
    union LayoutJustifyContentValue {
        LayoutJustifyContentValueVariant_Auto Auto;
        LayoutJustifyContentValueVariant_None None;
        LayoutJustifyContentValueVariant_Inherit Inherit;
        LayoutJustifyContentValueVariant_Initial Initial;
        LayoutJustifyContentValueVariant_Exact Exact;
    };
    
    
    enum class LayoutLeftValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutLeftValueVariant_Auto { LayoutLeftValueTag tag; };
    struct LayoutLeftValueVariant_None { LayoutLeftValueTag tag; };
    struct LayoutLeftValueVariant_Inherit { LayoutLeftValueTag tag; };
    struct LayoutLeftValueVariant_Initial { LayoutLeftValueTag tag; };
    struct LayoutLeftValueVariant_Exact { LayoutLeftValueTag tag; LayoutLeft payload; };
    union LayoutLeftValue {
        LayoutLeftValueVariant_Auto Auto;
        LayoutLeftValueVariant_None None;
        LayoutLeftValueVariant_Inherit Inherit;
        LayoutLeftValueVariant_Initial Initial;
        LayoutLeftValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMarginBottomValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMarginBottomValueVariant_Auto { LayoutMarginBottomValueTag tag; };
    struct LayoutMarginBottomValueVariant_None { LayoutMarginBottomValueTag tag; };
    struct LayoutMarginBottomValueVariant_Inherit { LayoutMarginBottomValueTag tag; };
    struct LayoutMarginBottomValueVariant_Initial { LayoutMarginBottomValueTag tag; };
    struct LayoutMarginBottomValueVariant_Exact { LayoutMarginBottomValueTag tag; LayoutMarginBottom payload; };
    union LayoutMarginBottomValue {
        LayoutMarginBottomValueVariant_Auto Auto;
        LayoutMarginBottomValueVariant_None None;
        LayoutMarginBottomValueVariant_Inherit Inherit;
        LayoutMarginBottomValueVariant_Initial Initial;
        LayoutMarginBottomValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMarginLeftValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMarginLeftValueVariant_Auto { LayoutMarginLeftValueTag tag; };
    struct LayoutMarginLeftValueVariant_None { LayoutMarginLeftValueTag tag; };
    struct LayoutMarginLeftValueVariant_Inherit { LayoutMarginLeftValueTag tag; };
    struct LayoutMarginLeftValueVariant_Initial { LayoutMarginLeftValueTag tag; };
    struct LayoutMarginLeftValueVariant_Exact { LayoutMarginLeftValueTag tag; LayoutMarginLeft payload; };
    union LayoutMarginLeftValue {
        LayoutMarginLeftValueVariant_Auto Auto;
        LayoutMarginLeftValueVariant_None None;
        LayoutMarginLeftValueVariant_Inherit Inherit;
        LayoutMarginLeftValueVariant_Initial Initial;
        LayoutMarginLeftValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMarginRightValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMarginRightValueVariant_Auto { LayoutMarginRightValueTag tag; };
    struct LayoutMarginRightValueVariant_None { LayoutMarginRightValueTag tag; };
    struct LayoutMarginRightValueVariant_Inherit { LayoutMarginRightValueTag tag; };
    struct LayoutMarginRightValueVariant_Initial { LayoutMarginRightValueTag tag; };
    struct LayoutMarginRightValueVariant_Exact { LayoutMarginRightValueTag tag; LayoutMarginRight payload; };
    union LayoutMarginRightValue {
        LayoutMarginRightValueVariant_Auto Auto;
        LayoutMarginRightValueVariant_None None;
        LayoutMarginRightValueVariant_Inherit Inherit;
        LayoutMarginRightValueVariant_Initial Initial;
        LayoutMarginRightValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMarginTopValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMarginTopValueVariant_Auto { LayoutMarginTopValueTag tag; };
    struct LayoutMarginTopValueVariant_None { LayoutMarginTopValueTag tag; };
    struct LayoutMarginTopValueVariant_Inherit { LayoutMarginTopValueTag tag; };
    struct LayoutMarginTopValueVariant_Initial { LayoutMarginTopValueTag tag; };
    struct LayoutMarginTopValueVariant_Exact { LayoutMarginTopValueTag tag; LayoutMarginTop payload; };
    union LayoutMarginTopValue {
        LayoutMarginTopValueVariant_Auto Auto;
        LayoutMarginTopValueVariant_None None;
        LayoutMarginTopValueVariant_Inherit Inherit;
        LayoutMarginTopValueVariant_Initial Initial;
        LayoutMarginTopValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMaxHeightValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMaxHeightValueVariant_Auto { LayoutMaxHeightValueTag tag; };
    struct LayoutMaxHeightValueVariant_None { LayoutMaxHeightValueTag tag; };
    struct LayoutMaxHeightValueVariant_Inherit { LayoutMaxHeightValueTag tag; };
    struct LayoutMaxHeightValueVariant_Initial { LayoutMaxHeightValueTag tag; };
    struct LayoutMaxHeightValueVariant_Exact { LayoutMaxHeightValueTag tag; LayoutMaxHeight payload; };
    union LayoutMaxHeightValue {
        LayoutMaxHeightValueVariant_Auto Auto;
        LayoutMaxHeightValueVariant_None None;
        LayoutMaxHeightValueVariant_Inherit Inherit;
        LayoutMaxHeightValueVariant_Initial Initial;
        LayoutMaxHeightValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMaxWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMaxWidthValueVariant_Auto { LayoutMaxWidthValueTag tag; };
    struct LayoutMaxWidthValueVariant_None { LayoutMaxWidthValueTag tag; };
    struct LayoutMaxWidthValueVariant_Inherit { LayoutMaxWidthValueTag tag; };
    struct LayoutMaxWidthValueVariant_Initial { LayoutMaxWidthValueTag tag; };
    struct LayoutMaxWidthValueVariant_Exact { LayoutMaxWidthValueTag tag; LayoutMaxWidth payload; };
    union LayoutMaxWidthValue {
        LayoutMaxWidthValueVariant_Auto Auto;
        LayoutMaxWidthValueVariant_None None;
        LayoutMaxWidthValueVariant_Inherit Inherit;
        LayoutMaxWidthValueVariant_Initial Initial;
        LayoutMaxWidthValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMinHeightValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMinHeightValueVariant_Auto { LayoutMinHeightValueTag tag; };
    struct LayoutMinHeightValueVariant_None { LayoutMinHeightValueTag tag; };
    struct LayoutMinHeightValueVariant_Inherit { LayoutMinHeightValueTag tag; };
    struct LayoutMinHeightValueVariant_Initial { LayoutMinHeightValueTag tag; };
    struct LayoutMinHeightValueVariant_Exact { LayoutMinHeightValueTag tag; LayoutMinHeight payload; };
    union LayoutMinHeightValue {
        LayoutMinHeightValueVariant_Auto Auto;
        LayoutMinHeightValueVariant_None None;
        LayoutMinHeightValueVariant_Inherit Inherit;
        LayoutMinHeightValueVariant_Initial Initial;
        LayoutMinHeightValueVariant_Exact Exact;
    };
    
    
    enum class LayoutMinWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutMinWidthValueVariant_Auto { LayoutMinWidthValueTag tag; };
    struct LayoutMinWidthValueVariant_None { LayoutMinWidthValueTag tag; };
    struct LayoutMinWidthValueVariant_Inherit { LayoutMinWidthValueTag tag; };
    struct LayoutMinWidthValueVariant_Initial { LayoutMinWidthValueTag tag; };
    struct LayoutMinWidthValueVariant_Exact { LayoutMinWidthValueTag tag; LayoutMinWidth payload; };
    union LayoutMinWidthValue {
        LayoutMinWidthValueVariant_Auto Auto;
        LayoutMinWidthValueVariant_None None;
        LayoutMinWidthValueVariant_Inherit Inherit;
        LayoutMinWidthValueVariant_Initial Initial;
        LayoutMinWidthValueVariant_Exact Exact;
    };
    
    
    enum class LayoutPaddingBottomValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutPaddingBottomValueVariant_Auto { LayoutPaddingBottomValueTag tag; };
    struct LayoutPaddingBottomValueVariant_None { LayoutPaddingBottomValueTag tag; };
    struct LayoutPaddingBottomValueVariant_Inherit { LayoutPaddingBottomValueTag tag; };
    struct LayoutPaddingBottomValueVariant_Initial { LayoutPaddingBottomValueTag tag; };
    struct LayoutPaddingBottomValueVariant_Exact { LayoutPaddingBottomValueTag tag; LayoutPaddingBottom payload; };
    union LayoutPaddingBottomValue {
        LayoutPaddingBottomValueVariant_Auto Auto;
        LayoutPaddingBottomValueVariant_None None;
        LayoutPaddingBottomValueVariant_Inherit Inherit;
        LayoutPaddingBottomValueVariant_Initial Initial;
        LayoutPaddingBottomValueVariant_Exact Exact;
    };
    
    
    enum class LayoutPaddingLeftValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutPaddingLeftValueVariant_Auto { LayoutPaddingLeftValueTag tag; };
    struct LayoutPaddingLeftValueVariant_None { LayoutPaddingLeftValueTag tag; };
    struct LayoutPaddingLeftValueVariant_Inherit { LayoutPaddingLeftValueTag tag; };
    struct LayoutPaddingLeftValueVariant_Initial { LayoutPaddingLeftValueTag tag; };
    struct LayoutPaddingLeftValueVariant_Exact { LayoutPaddingLeftValueTag tag; LayoutPaddingLeft payload; };
    union LayoutPaddingLeftValue {
        LayoutPaddingLeftValueVariant_Auto Auto;
        LayoutPaddingLeftValueVariant_None None;
        LayoutPaddingLeftValueVariant_Inherit Inherit;
        LayoutPaddingLeftValueVariant_Initial Initial;
        LayoutPaddingLeftValueVariant_Exact Exact;
    };
    
    
    enum class LayoutPaddingRightValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutPaddingRightValueVariant_Auto { LayoutPaddingRightValueTag tag; };
    struct LayoutPaddingRightValueVariant_None { LayoutPaddingRightValueTag tag; };
    struct LayoutPaddingRightValueVariant_Inherit { LayoutPaddingRightValueTag tag; };
    struct LayoutPaddingRightValueVariant_Initial { LayoutPaddingRightValueTag tag; };
    struct LayoutPaddingRightValueVariant_Exact { LayoutPaddingRightValueTag tag; LayoutPaddingRight payload; };
    union LayoutPaddingRightValue {
        LayoutPaddingRightValueVariant_Auto Auto;
        LayoutPaddingRightValueVariant_None None;
        LayoutPaddingRightValueVariant_Inherit Inherit;
        LayoutPaddingRightValueVariant_Initial Initial;
        LayoutPaddingRightValueVariant_Exact Exact;
    };
    
    
    enum class LayoutPaddingTopValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutPaddingTopValueVariant_Auto { LayoutPaddingTopValueTag tag; };
    struct LayoutPaddingTopValueVariant_None { LayoutPaddingTopValueTag tag; };
    struct LayoutPaddingTopValueVariant_Inherit { LayoutPaddingTopValueTag tag; };
    struct LayoutPaddingTopValueVariant_Initial { LayoutPaddingTopValueTag tag; };
    struct LayoutPaddingTopValueVariant_Exact { LayoutPaddingTopValueTag tag; LayoutPaddingTop payload; };
    union LayoutPaddingTopValue {
        LayoutPaddingTopValueVariant_Auto Auto;
        LayoutPaddingTopValueVariant_None None;
        LayoutPaddingTopValueVariant_Inherit Inherit;
        LayoutPaddingTopValueVariant_Initial Initial;
        LayoutPaddingTopValueVariant_Exact Exact;
    };
    
    
    enum class LayoutPositionValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutPositionValueVariant_Auto { LayoutPositionValueTag tag; };
    struct LayoutPositionValueVariant_None { LayoutPositionValueTag tag; };
    struct LayoutPositionValueVariant_Inherit { LayoutPositionValueTag tag; };
    struct LayoutPositionValueVariant_Initial { LayoutPositionValueTag tag; };
    struct LayoutPositionValueVariant_Exact { LayoutPositionValueTag tag; LayoutPosition payload; };
    union LayoutPositionValue {
        LayoutPositionValueVariant_Auto Auto;
        LayoutPositionValueVariant_None None;
        LayoutPositionValueVariant_Inherit Inherit;
        LayoutPositionValueVariant_Initial Initial;
        LayoutPositionValueVariant_Exact Exact;
    };
    
    
    enum class LayoutRightValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutRightValueVariant_Auto { LayoutRightValueTag tag; };
    struct LayoutRightValueVariant_None { LayoutRightValueTag tag; };
    struct LayoutRightValueVariant_Inherit { LayoutRightValueTag tag; };
    struct LayoutRightValueVariant_Initial { LayoutRightValueTag tag; };
    struct LayoutRightValueVariant_Exact { LayoutRightValueTag tag; LayoutRight payload; };
    union LayoutRightValue {
        LayoutRightValueVariant_Auto Auto;
        LayoutRightValueVariant_None None;
        LayoutRightValueVariant_Inherit Inherit;
        LayoutRightValueVariant_Initial Initial;
        LayoutRightValueVariant_Exact Exact;
    };
    
    
    enum class LayoutTopValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutTopValueVariant_Auto { LayoutTopValueTag tag; };
    struct LayoutTopValueVariant_None { LayoutTopValueTag tag; };
    struct LayoutTopValueVariant_Inherit { LayoutTopValueTag tag; };
    struct LayoutTopValueVariant_Initial { LayoutTopValueTag tag; };
    struct LayoutTopValueVariant_Exact { LayoutTopValueTag tag; LayoutTop payload; };
    union LayoutTopValue {
        LayoutTopValueVariant_Auto Auto;
        LayoutTopValueVariant_None None;
        LayoutTopValueVariant_Inherit Inherit;
        LayoutTopValueVariant_Initial Initial;
        LayoutTopValueVariant_Exact Exact;
    };
    
    
    enum class LayoutWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutWidthValueVariant_Auto { LayoutWidthValueTag tag; };
    struct LayoutWidthValueVariant_None { LayoutWidthValueTag tag; };
    struct LayoutWidthValueVariant_Inherit { LayoutWidthValueTag tag; };
    struct LayoutWidthValueVariant_Initial { LayoutWidthValueTag tag; };
    struct LayoutWidthValueVariant_Exact { LayoutWidthValueTag tag; LayoutWidth payload; };
    union LayoutWidthValue {
        LayoutWidthValueVariant_Auto Auto;
        LayoutWidthValueVariant_None None;
        LayoutWidthValueVariant_Inherit Inherit;
        LayoutWidthValueVariant_Initial Initial;
        LayoutWidthValueVariant_Exact Exact;
    };
    
    
    enum class LayoutFlexWrapValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutFlexWrapValueVariant_Auto { LayoutFlexWrapValueTag tag; };
    struct LayoutFlexWrapValueVariant_None { LayoutFlexWrapValueTag tag; };
    struct LayoutFlexWrapValueVariant_Inherit { LayoutFlexWrapValueTag tag; };
    struct LayoutFlexWrapValueVariant_Initial { LayoutFlexWrapValueTag tag; };
    struct LayoutFlexWrapValueVariant_Exact { LayoutFlexWrapValueTag tag; LayoutFlexWrap payload; };
    union LayoutFlexWrapValue {
        LayoutFlexWrapValueVariant_Auto Auto;
        LayoutFlexWrapValueVariant_None None;
        LayoutFlexWrapValueVariant_Inherit Inherit;
        LayoutFlexWrapValueVariant_Initial Initial;
        LayoutFlexWrapValueVariant_Exact Exact;
    };
    
    
    enum class LayoutOverflowValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutOverflowValueVariant_Auto { LayoutOverflowValueTag tag; };
    struct LayoutOverflowValueVariant_None { LayoutOverflowValueTag tag; };
    struct LayoutOverflowValueVariant_Inherit { LayoutOverflowValueTag tag; };
    struct LayoutOverflowValueVariant_Initial { LayoutOverflowValueTag tag; };
    struct LayoutOverflowValueVariant_Exact { LayoutOverflowValueTag tag; LayoutOverflow payload; };
    union LayoutOverflowValue {
        LayoutOverflowValueVariant_Auto Auto;
        LayoutOverflowValueVariant_None None;
        LayoutOverflowValueVariant_Inherit Inherit;
        LayoutOverflowValueVariant_Initial Initial;
        LayoutOverflowValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderBottomColorValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderBottomColorValueVariant_Auto { StyleBorderBottomColorValueTag tag; };
    struct StyleBorderBottomColorValueVariant_None { StyleBorderBottomColorValueTag tag; };
    struct StyleBorderBottomColorValueVariant_Inherit { StyleBorderBottomColorValueTag tag; };
    struct StyleBorderBottomColorValueVariant_Initial { StyleBorderBottomColorValueTag tag; };
    struct StyleBorderBottomColorValueVariant_Exact { StyleBorderBottomColorValueTag tag; StyleBorderBottomColor payload; };
    union StyleBorderBottomColorValue {
        StyleBorderBottomColorValueVariant_Auto Auto;
        StyleBorderBottomColorValueVariant_None None;
        StyleBorderBottomColorValueVariant_Inherit Inherit;
        StyleBorderBottomColorValueVariant_Initial Initial;
        StyleBorderBottomColorValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderBottomLeftRadiusValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderBottomLeftRadiusValueVariant_Auto { StyleBorderBottomLeftRadiusValueTag tag; };
    struct StyleBorderBottomLeftRadiusValueVariant_None { StyleBorderBottomLeftRadiusValueTag tag; };
    struct StyleBorderBottomLeftRadiusValueVariant_Inherit { StyleBorderBottomLeftRadiusValueTag tag; };
    struct StyleBorderBottomLeftRadiusValueVariant_Initial { StyleBorderBottomLeftRadiusValueTag tag; };
    struct StyleBorderBottomLeftRadiusValueVariant_Exact { StyleBorderBottomLeftRadiusValueTag tag; StyleBorderBottomLeftRadius payload; };
    union StyleBorderBottomLeftRadiusValue {
        StyleBorderBottomLeftRadiusValueVariant_Auto Auto;
        StyleBorderBottomLeftRadiusValueVariant_None None;
        StyleBorderBottomLeftRadiusValueVariant_Inherit Inherit;
        StyleBorderBottomLeftRadiusValueVariant_Initial Initial;
        StyleBorderBottomLeftRadiusValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderBottomRightRadiusValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderBottomRightRadiusValueVariant_Auto { StyleBorderBottomRightRadiusValueTag tag; };
    struct StyleBorderBottomRightRadiusValueVariant_None { StyleBorderBottomRightRadiusValueTag tag; };
    struct StyleBorderBottomRightRadiusValueVariant_Inherit { StyleBorderBottomRightRadiusValueTag tag; };
    struct StyleBorderBottomRightRadiusValueVariant_Initial { StyleBorderBottomRightRadiusValueTag tag; };
    struct StyleBorderBottomRightRadiusValueVariant_Exact { StyleBorderBottomRightRadiusValueTag tag; StyleBorderBottomRightRadius payload; };
    union StyleBorderBottomRightRadiusValue {
        StyleBorderBottomRightRadiusValueVariant_Auto Auto;
        StyleBorderBottomRightRadiusValueVariant_None None;
        StyleBorderBottomRightRadiusValueVariant_Inherit Inherit;
        StyleBorderBottomRightRadiusValueVariant_Initial Initial;
        StyleBorderBottomRightRadiusValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderBottomStyleValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderBottomStyleValueVariant_Auto { StyleBorderBottomStyleValueTag tag; };
    struct StyleBorderBottomStyleValueVariant_None { StyleBorderBottomStyleValueTag tag; };
    struct StyleBorderBottomStyleValueVariant_Inherit { StyleBorderBottomStyleValueTag tag; };
    struct StyleBorderBottomStyleValueVariant_Initial { StyleBorderBottomStyleValueTag tag; };
    struct StyleBorderBottomStyleValueVariant_Exact { StyleBorderBottomStyleValueTag tag; StyleBorderBottomStyle payload; };
    union StyleBorderBottomStyleValue {
        StyleBorderBottomStyleValueVariant_Auto Auto;
        StyleBorderBottomStyleValueVariant_None None;
        StyleBorderBottomStyleValueVariant_Inherit Inherit;
        StyleBorderBottomStyleValueVariant_Initial Initial;
        StyleBorderBottomStyleValueVariant_Exact Exact;
    };
    
    
    enum class LayoutBorderBottomWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutBorderBottomWidthValueVariant_Auto { LayoutBorderBottomWidthValueTag tag; };
    struct LayoutBorderBottomWidthValueVariant_None { LayoutBorderBottomWidthValueTag tag; };
    struct LayoutBorderBottomWidthValueVariant_Inherit { LayoutBorderBottomWidthValueTag tag; };
    struct LayoutBorderBottomWidthValueVariant_Initial { LayoutBorderBottomWidthValueTag tag; };
    struct LayoutBorderBottomWidthValueVariant_Exact { LayoutBorderBottomWidthValueTag tag; LayoutBorderBottomWidth payload; };
    union LayoutBorderBottomWidthValue {
        LayoutBorderBottomWidthValueVariant_Auto Auto;
        LayoutBorderBottomWidthValueVariant_None None;
        LayoutBorderBottomWidthValueVariant_Inherit Inherit;
        LayoutBorderBottomWidthValueVariant_Initial Initial;
        LayoutBorderBottomWidthValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderLeftColorValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderLeftColorValueVariant_Auto { StyleBorderLeftColorValueTag tag; };
    struct StyleBorderLeftColorValueVariant_None { StyleBorderLeftColorValueTag tag; };
    struct StyleBorderLeftColorValueVariant_Inherit { StyleBorderLeftColorValueTag tag; };
    struct StyleBorderLeftColorValueVariant_Initial { StyleBorderLeftColorValueTag tag; };
    struct StyleBorderLeftColorValueVariant_Exact { StyleBorderLeftColorValueTag tag; StyleBorderLeftColor payload; };
    union StyleBorderLeftColorValue {
        StyleBorderLeftColorValueVariant_Auto Auto;
        StyleBorderLeftColorValueVariant_None None;
        StyleBorderLeftColorValueVariant_Inherit Inherit;
        StyleBorderLeftColorValueVariant_Initial Initial;
        StyleBorderLeftColorValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderLeftStyleValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderLeftStyleValueVariant_Auto { StyleBorderLeftStyleValueTag tag; };
    struct StyleBorderLeftStyleValueVariant_None { StyleBorderLeftStyleValueTag tag; };
    struct StyleBorderLeftStyleValueVariant_Inherit { StyleBorderLeftStyleValueTag tag; };
    struct StyleBorderLeftStyleValueVariant_Initial { StyleBorderLeftStyleValueTag tag; };
    struct StyleBorderLeftStyleValueVariant_Exact { StyleBorderLeftStyleValueTag tag; StyleBorderLeftStyle payload; };
    union StyleBorderLeftStyleValue {
        StyleBorderLeftStyleValueVariant_Auto Auto;
        StyleBorderLeftStyleValueVariant_None None;
        StyleBorderLeftStyleValueVariant_Inherit Inherit;
        StyleBorderLeftStyleValueVariant_Initial Initial;
        StyleBorderLeftStyleValueVariant_Exact Exact;
    };
    
    
    enum class LayoutBorderLeftWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutBorderLeftWidthValueVariant_Auto { LayoutBorderLeftWidthValueTag tag; };
    struct LayoutBorderLeftWidthValueVariant_None { LayoutBorderLeftWidthValueTag tag; };
    struct LayoutBorderLeftWidthValueVariant_Inherit { LayoutBorderLeftWidthValueTag tag; };
    struct LayoutBorderLeftWidthValueVariant_Initial { LayoutBorderLeftWidthValueTag tag; };
    struct LayoutBorderLeftWidthValueVariant_Exact { LayoutBorderLeftWidthValueTag tag; LayoutBorderLeftWidth payload; };
    union LayoutBorderLeftWidthValue {
        LayoutBorderLeftWidthValueVariant_Auto Auto;
        LayoutBorderLeftWidthValueVariant_None None;
        LayoutBorderLeftWidthValueVariant_Inherit Inherit;
        LayoutBorderLeftWidthValueVariant_Initial Initial;
        LayoutBorderLeftWidthValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderRightColorValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderRightColorValueVariant_Auto { StyleBorderRightColorValueTag tag; };
    struct StyleBorderRightColorValueVariant_None { StyleBorderRightColorValueTag tag; };
    struct StyleBorderRightColorValueVariant_Inherit { StyleBorderRightColorValueTag tag; };
    struct StyleBorderRightColorValueVariant_Initial { StyleBorderRightColorValueTag tag; };
    struct StyleBorderRightColorValueVariant_Exact { StyleBorderRightColorValueTag tag; StyleBorderRightColor payload; };
    union StyleBorderRightColorValue {
        StyleBorderRightColorValueVariant_Auto Auto;
        StyleBorderRightColorValueVariant_None None;
        StyleBorderRightColorValueVariant_Inherit Inherit;
        StyleBorderRightColorValueVariant_Initial Initial;
        StyleBorderRightColorValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderRightStyleValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderRightStyleValueVariant_Auto { StyleBorderRightStyleValueTag tag; };
    struct StyleBorderRightStyleValueVariant_None { StyleBorderRightStyleValueTag tag; };
    struct StyleBorderRightStyleValueVariant_Inherit { StyleBorderRightStyleValueTag tag; };
    struct StyleBorderRightStyleValueVariant_Initial { StyleBorderRightStyleValueTag tag; };
    struct StyleBorderRightStyleValueVariant_Exact { StyleBorderRightStyleValueTag tag; StyleBorderRightStyle payload; };
    union StyleBorderRightStyleValue {
        StyleBorderRightStyleValueVariant_Auto Auto;
        StyleBorderRightStyleValueVariant_None None;
        StyleBorderRightStyleValueVariant_Inherit Inherit;
        StyleBorderRightStyleValueVariant_Initial Initial;
        StyleBorderRightStyleValueVariant_Exact Exact;
    };
    
    
    enum class LayoutBorderRightWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutBorderRightWidthValueVariant_Auto { LayoutBorderRightWidthValueTag tag; };
    struct LayoutBorderRightWidthValueVariant_None { LayoutBorderRightWidthValueTag tag; };
    struct LayoutBorderRightWidthValueVariant_Inherit { LayoutBorderRightWidthValueTag tag; };
    struct LayoutBorderRightWidthValueVariant_Initial { LayoutBorderRightWidthValueTag tag; };
    struct LayoutBorderRightWidthValueVariant_Exact { LayoutBorderRightWidthValueTag tag; LayoutBorderRightWidth payload; };
    union LayoutBorderRightWidthValue {
        LayoutBorderRightWidthValueVariant_Auto Auto;
        LayoutBorderRightWidthValueVariant_None None;
        LayoutBorderRightWidthValueVariant_Inherit Inherit;
        LayoutBorderRightWidthValueVariant_Initial Initial;
        LayoutBorderRightWidthValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderTopColorValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderTopColorValueVariant_Auto { StyleBorderTopColorValueTag tag; };
    struct StyleBorderTopColorValueVariant_None { StyleBorderTopColorValueTag tag; };
    struct StyleBorderTopColorValueVariant_Inherit { StyleBorderTopColorValueTag tag; };
    struct StyleBorderTopColorValueVariant_Initial { StyleBorderTopColorValueTag tag; };
    struct StyleBorderTopColorValueVariant_Exact { StyleBorderTopColorValueTag tag; StyleBorderTopColor payload; };
    union StyleBorderTopColorValue {
        StyleBorderTopColorValueVariant_Auto Auto;
        StyleBorderTopColorValueVariant_None None;
        StyleBorderTopColorValueVariant_Inherit Inherit;
        StyleBorderTopColorValueVariant_Initial Initial;
        StyleBorderTopColorValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderTopLeftRadiusValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderTopLeftRadiusValueVariant_Auto { StyleBorderTopLeftRadiusValueTag tag; };
    struct StyleBorderTopLeftRadiusValueVariant_None { StyleBorderTopLeftRadiusValueTag tag; };
    struct StyleBorderTopLeftRadiusValueVariant_Inherit { StyleBorderTopLeftRadiusValueTag tag; };
    struct StyleBorderTopLeftRadiusValueVariant_Initial { StyleBorderTopLeftRadiusValueTag tag; };
    struct StyleBorderTopLeftRadiusValueVariant_Exact { StyleBorderTopLeftRadiusValueTag tag; StyleBorderTopLeftRadius payload; };
    union StyleBorderTopLeftRadiusValue {
        StyleBorderTopLeftRadiusValueVariant_Auto Auto;
        StyleBorderTopLeftRadiusValueVariant_None None;
        StyleBorderTopLeftRadiusValueVariant_Inherit Inherit;
        StyleBorderTopLeftRadiusValueVariant_Initial Initial;
        StyleBorderTopLeftRadiusValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderTopRightRadiusValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderTopRightRadiusValueVariant_Auto { StyleBorderTopRightRadiusValueTag tag; };
    struct StyleBorderTopRightRadiusValueVariant_None { StyleBorderTopRightRadiusValueTag tag; };
    struct StyleBorderTopRightRadiusValueVariant_Inherit { StyleBorderTopRightRadiusValueTag tag; };
    struct StyleBorderTopRightRadiusValueVariant_Initial { StyleBorderTopRightRadiusValueTag tag; };
    struct StyleBorderTopRightRadiusValueVariant_Exact { StyleBorderTopRightRadiusValueTag tag; StyleBorderTopRightRadius payload; };
    union StyleBorderTopRightRadiusValue {
        StyleBorderTopRightRadiusValueVariant_Auto Auto;
        StyleBorderTopRightRadiusValueVariant_None None;
        StyleBorderTopRightRadiusValueVariant_Inherit Inherit;
        StyleBorderTopRightRadiusValueVariant_Initial Initial;
        StyleBorderTopRightRadiusValueVariant_Exact Exact;
    };
    
    
    enum class StyleBorderTopStyleValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBorderTopStyleValueVariant_Auto { StyleBorderTopStyleValueTag tag; };
    struct StyleBorderTopStyleValueVariant_None { StyleBorderTopStyleValueTag tag; };
    struct StyleBorderTopStyleValueVariant_Inherit { StyleBorderTopStyleValueTag tag; };
    struct StyleBorderTopStyleValueVariant_Initial { StyleBorderTopStyleValueTag tag; };
    struct StyleBorderTopStyleValueVariant_Exact { StyleBorderTopStyleValueTag tag; StyleBorderTopStyle payload; };
    union StyleBorderTopStyleValue {
        StyleBorderTopStyleValueVariant_Auto Auto;
        StyleBorderTopStyleValueVariant_None None;
        StyleBorderTopStyleValueVariant_Inherit Inherit;
        StyleBorderTopStyleValueVariant_Initial Initial;
        StyleBorderTopStyleValueVariant_Exact Exact;
    };
    
    
    enum class LayoutBorderTopWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct LayoutBorderTopWidthValueVariant_Auto { LayoutBorderTopWidthValueTag tag; };
    struct LayoutBorderTopWidthValueVariant_None { LayoutBorderTopWidthValueTag tag; };
    struct LayoutBorderTopWidthValueVariant_Inherit { LayoutBorderTopWidthValueTag tag; };
    struct LayoutBorderTopWidthValueVariant_Initial { LayoutBorderTopWidthValueTag tag; };
    struct LayoutBorderTopWidthValueVariant_Exact { LayoutBorderTopWidthValueTag tag; LayoutBorderTopWidth payload; };
    union LayoutBorderTopWidthValue {
        LayoutBorderTopWidthValueVariant_Auto Auto;
        LayoutBorderTopWidthValueVariant_None None;
        LayoutBorderTopWidthValueVariant_Inherit Inherit;
        LayoutBorderTopWidthValueVariant_Initial Initial;
        LayoutBorderTopWidthValueVariant_Exact Exact;
    };
    
    
    enum class StyleCursorValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleCursorValueVariant_Auto { StyleCursorValueTag tag; };
    struct StyleCursorValueVariant_None { StyleCursorValueTag tag; };
    struct StyleCursorValueVariant_Inherit { StyleCursorValueTag tag; };
    struct StyleCursorValueVariant_Initial { StyleCursorValueTag tag; };
    struct StyleCursorValueVariant_Exact { StyleCursorValueTag tag; StyleCursor payload; };
    union StyleCursorValue {
        StyleCursorValueVariant_Auto Auto;
        StyleCursorValueVariant_None None;
        StyleCursorValueVariant_Inherit Inherit;
        StyleCursorValueVariant_Initial Initial;
        StyleCursorValueVariant_Exact Exact;
    };
    
    
    enum class StyleFontSizeValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleFontSizeValueVariant_Auto { StyleFontSizeValueTag tag; };
    struct StyleFontSizeValueVariant_None { StyleFontSizeValueTag tag; };
    struct StyleFontSizeValueVariant_Inherit { StyleFontSizeValueTag tag; };
    struct StyleFontSizeValueVariant_Initial { StyleFontSizeValueTag tag; };
    struct StyleFontSizeValueVariant_Exact { StyleFontSizeValueTag tag; StyleFontSize payload; };
    union StyleFontSizeValue {
        StyleFontSizeValueVariant_Auto Auto;
        StyleFontSizeValueVariant_None None;
        StyleFontSizeValueVariant_Inherit Inherit;
        StyleFontSizeValueVariant_Initial Initial;
        StyleFontSizeValueVariant_Exact Exact;
    };
    
    
    enum class StyleLetterSpacingValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleLetterSpacingValueVariant_Auto { StyleLetterSpacingValueTag tag; };
    struct StyleLetterSpacingValueVariant_None { StyleLetterSpacingValueTag tag; };
    struct StyleLetterSpacingValueVariant_Inherit { StyleLetterSpacingValueTag tag; };
    struct StyleLetterSpacingValueVariant_Initial { StyleLetterSpacingValueTag tag; };
    struct StyleLetterSpacingValueVariant_Exact { StyleLetterSpacingValueTag tag; StyleLetterSpacing payload; };
    union StyleLetterSpacingValue {
        StyleLetterSpacingValueVariant_Auto Auto;
        StyleLetterSpacingValueVariant_None None;
        StyleLetterSpacingValueVariant_Inherit Inherit;
        StyleLetterSpacingValueVariant_Initial Initial;
        StyleLetterSpacingValueVariant_Exact Exact;
    };
    
    
    enum class StyleLineHeightValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleLineHeightValueVariant_Auto { StyleLineHeightValueTag tag; };
    struct StyleLineHeightValueVariant_None { StyleLineHeightValueTag tag; };
    struct StyleLineHeightValueVariant_Inherit { StyleLineHeightValueTag tag; };
    struct StyleLineHeightValueVariant_Initial { StyleLineHeightValueTag tag; };
    struct StyleLineHeightValueVariant_Exact { StyleLineHeightValueTag tag; StyleLineHeight payload; };
    union StyleLineHeightValue {
        StyleLineHeightValueVariant_Auto Auto;
        StyleLineHeightValueVariant_None None;
        StyleLineHeightValueVariant_Inherit Inherit;
        StyleLineHeightValueVariant_Initial Initial;
        StyleLineHeightValueVariant_Exact Exact;
    };
    
    
    enum class StyleTabWidthValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleTabWidthValueVariant_Auto { StyleTabWidthValueTag tag; };
    struct StyleTabWidthValueVariant_None { StyleTabWidthValueTag tag; };
    struct StyleTabWidthValueVariant_Inherit { StyleTabWidthValueTag tag; };
    struct StyleTabWidthValueVariant_Initial { StyleTabWidthValueTag tag; };
    struct StyleTabWidthValueVariant_Exact { StyleTabWidthValueTag tag; StyleTabWidth payload; };
    union StyleTabWidthValue {
        StyleTabWidthValueVariant_Auto Auto;
        StyleTabWidthValueVariant_None None;
        StyleTabWidthValueVariant_Inherit Inherit;
        StyleTabWidthValueVariant_Initial Initial;
        StyleTabWidthValueVariant_Exact Exact;
    };
    
    
    enum class StyleTextAlignValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleTextAlignValueVariant_Auto { StyleTextAlignValueTag tag; };
    struct StyleTextAlignValueVariant_None { StyleTextAlignValueTag tag; };
    struct StyleTextAlignValueVariant_Inherit { StyleTextAlignValueTag tag; };
    struct StyleTextAlignValueVariant_Initial { StyleTextAlignValueTag tag; };
    struct StyleTextAlignValueVariant_Exact { StyleTextAlignValueTag tag; StyleTextAlign payload; };
    union StyleTextAlignValue {
        StyleTextAlignValueVariant_Auto Auto;
        StyleTextAlignValueVariant_None None;
        StyleTextAlignValueVariant_Inherit Inherit;
        StyleTextAlignValueVariant_Initial Initial;
        StyleTextAlignValueVariant_Exact Exact;
    };
    
    
    enum class StyleTextColorValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleTextColorValueVariant_Auto { StyleTextColorValueTag tag; };
    struct StyleTextColorValueVariant_None { StyleTextColorValueTag tag; };
    struct StyleTextColorValueVariant_Inherit { StyleTextColorValueTag tag; };
    struct StyleTextColorValueVariant_Initial { StyleTextColorValueTag tag; };
    struct StyleTextColorValueVariant_Exact { StyleTextColorValueTag tag; StyleTextColor payload; };
    union StyleTextColorValue {
        StyleTextColorValueVariant_Auto Auto;
        StyleTextColorValueVariant_None None;
        StyleTextColorValueVariant_Inherit Inherit;
        StyleTextColorValueVariant_Initial Initial;
        StyleTextColorValueVariant_Exact Exact;
    };
    
    
    enum class StyleWordSpacingValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleWordSpacingValueVariant_Auto { StyleWordSpacingValueTag tag; };
    struct StyleWordSpacingValueVariant_None { StyleWordSpacingValueTag tag; };
    struct StyleWordSpacingValueVariant_Inherit { StyleWordSpacingValueTag tag; };
    struct StyleWordSpacingValueVariant_Initial { StyleWordSpacingValueTag tag; };
    struct StyleWordSpacingValueVariant_Exact { StyleWordSpacingValueTag tag; StyleWordSpacing payload; };
    union StyleWordSpacingValue {
        StyleWordSpacingValueVariant_Auto Auto;
        StyleWordSpacingValueVariant_None None;
        StyleWordSpacingValueVariant_Inherit Inherit;
        StyleWordSpacingValueVariant_Initial Initial;
        StyleWordSpacingValueVariant_Exact Exact;
    };
    
    
    enum class StyleOpacityValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleOpacityValueVariant_Auto { StyleOpacityValueTag tag; };
    struct StyleOpacityValueVariant_None { StyleOpacityValueTag tag; };
    struct StyleOpacityValueVariant_Inherit { StyleOpacityValueTag tag; };
    struct StyleOpacityValueVariant_Initial { StyleOpacityValueTag tag; };
    struct StyleOpacityValueVariant_Exact { StyleOpacityValueTag tag; StyleOpacity payload; };
    union StyleOpacityValue {
        StyleOpacityValueVariant_Auto Auto;
        StyleOpacityValueVariant_None None;
        StyleOpacityValueVariant_Inherit Inherit;
        StyleOpacityValueVariant_Initial Initial;
        StyleOpacityValueVariant_Exact Exact;
    };
    
    
    enum class StyleTransformOriginValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleTransformOriginValueVariant_Auto { StyleTransformOriginValueTag tag; };
    struct StyleTransformOriginValueVariant_None { StyleTransformOriginValueTag tag; };
    struct StyleTransformOriginValueVariant_Inherit { StyleTransformOriginValueTag tag; };
    struct StyleTransformOriginValueVariant_Initial { StyleTransformOriginValueTag tag; };
    struct StyleTransformOriginValueVariant_Exact { StyleTransformOriginValueTag tag; StyleTransformOrigin payload; };
    union StyleTransformOriginValue {
        StyleTransformOriginValueVariant_Auto Auto;
        StyleTransformOriginValueVariant_None None;
        StyleTransformOriginValueVariant_Inherit Inherit;
        StyleTransformOriginValueVariant_Initial Initial;
        StyleTransformOriginValueVariant_Exact Exact;
    };
    
    
    enum class StylePerspectiveOriginValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StylePerspectiveOriginValueVariant_Auto { StylePerspectiveOriginValueTag tag; };
    struct StylePerspectiveOriginValueVariant_None { StylePerspectiveOriginValueTag tag; };
    struct StylePerspectiveOriginValueVariant_Inherit { StylePerspectiveOriginValueTag tag; };
    struct StylePerspectiveOriginValueVariant_Initial { StylePerspectiveOriginValueTag tag; };
    struct StylePerspectiveOriginValueVariant_Exact { StylePerspectiveOriginValueTag tag; StylePerspectiveOrigin payload; };
    union StylePerspectiveOriginValue {
        StylePerspectiveOriginValueVariant_Auto Auto;
        StylePerspectiveOriginValueVariant_None None;
        StylePerspectiveOriginValueVariant_Inherit Inherit;
        StylePerspectiveOriginValueVariant_Initial Initial;
        StylePerspectiveOriginValueVariant_Exact Exact;
    };
    
    
    enum class StyleBackfaceVisibilityValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBackfaceVisibilityValueVariant_Auto { StyleBackfaceVisibilityValueTag tag; };
    struct StyleBackfaceVisibilityValueVariant_None { StyleBackfaceVisibilityValueTag tag; };
    struct StyleBackfaceVisibilityValueVariant_Inherit { StyleBackfaceVisibilityValueTag tag; };
    struct StyleBackfaceVisibilityValueVariant_Initial { StyleBackfaceVisibilityValueTag tag; };
    struct StyleBackfaceVisibilityValueVariant_Exact { StyleBackfaceVisibilityValueTag tag; StyleBackfaceVisibility payload; };
    union StyleBackfaceVisibilityValue {
        StyleBackfaceVisibilityValueVariant_Auto Auto;
        StyleBackfaceVisibilityValueVariant_None None;
        StyleBackfaceVisibilityValueVariant_Inherit Inherit;
        StyleBackfaceVisibilityValueVariant_Initial Initial;
        StyleBackfaceVisibilityValueVariant_Exact Exact;
    };
    
    
    enum class StyleMixBlendModeValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleMixBlendModeValueVariant_Auto { StyleMixBlendModeValueTag tag; };
    struct StyleMixBlendModeValueVariant_None { StyleMixBlendModeValueTag tag; };
    struct StyleMixBlendModeValueVariant_Inherit { StyleMixBlendModeValueTag tag; };
    struct StyleMixBlendModeValueVariant_Initial { StyleMixBlendModeValueTag tag; };
    struct StyleMixBlendModeValueVariant_Exact { StyleMixBlendModeValueTag tag; StyleMixBlendMode payload; };
    union StyleMixBlendModeValue {
        StyleMixBlendModeValueVariant_Auto Auto;
        StyleMixBlendModeValueVariant_None None;
        StyleMixBlendModeValueVariant_Inherit Inherit;
        StyleMixBlendModeValueVariant_Initial Initial;
        StyleMixBlendModeValueVariant_Exact Exact;
    };
    
    
    struct ButtonOnClick {
        RefAny data;
        Callback callback;
        ButtonOnClick& operator=(const ButtonOnClick&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ButtonOnClick(const ButtonOnClick&) = delete; /* disable copy constructor, use explicit .clone() */
        ButtonOnClick() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct FileInputOnPathChange {
        RefAny data;
        FileInputOnPathChangeCallback callback;
        FileInputOnPathChange& operator=(const FileInputOnPathChange&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FileInputOnPathChange(const FileInputOnPathChange&) = delete; /* disable copy constructor, use explicit .clone() */
        FileInputOnPathChange() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CheckBoxOnToggle {
        RefAny data;
        CheckBoxOnToggleCallback callback;
        CheckBoxOnToggle& operator=(const CheckBoxOnToggle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CheckBoxOnToggle(const CheckBoxOnToggle&) = delete; /* disable copy constructor, use explicit .clone() */
        CheckBoxOnToggle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ColorInputState {
        ColorU color;
        ColorInputState& operator=(const ColorInputState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ColorInputState(const ColorInputState&) = delete; /* disable copy constructor, use explicit .clone() */
        ColorInputState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ColorInputOnValueChange {
        RefAny data;
        ColorInputOnValueChangeCallback callback;
        ColorInputOnValueChange& operator=(const ColorInputOnValueChange&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ColorInputOnValueChange(const ColorInputOnValueChange&) = delete; /* disable copy constructor, use explicit .clone() */
        ColorInputOnValueChange() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class TextInputSelectionTag {
       All,
       FromTo,
    };
    
    struct TextInputSelectionVariant_All { TextInputSelectionTag tag; };
    struct TextInputSelectionVariant_FromTo { TextInputSelectionTag tag; TextInputSelectionRange payload; };
    union TextInputSelection {
        TextInputSelectionVariant_All All;
        TextInputSelectionVariant_FromTo FromTo;
    };
    
    
    struct TextInputOnTextInput {
        RefAny data;
        TextInputOnTextInputCallback callback;
        TextInputOnTextInput& operator=(const TextInputOnTextInput&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputOnTextInput(const TextInputOnTextInput&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputOnTextInput() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputOnVirtualKeyDown {
        RefAny data;
        TextInputOnVirtualKeyDownCallback callback;
        TextInputOnVirtualKeyDown& operator=(const TextInputOnVirtualKeyDown&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputOnVirtualKeyDown(const TextInputOnVirtualKeyDown&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputOnVirtualKeyDown() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputOnFocusLost {
        RefAny data;
        TextInputOnFocusLostCallback callback;
        TextInputOnFocusLost& operator=(const TextInputOnFocusLost&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputOnFocusLost(const TextInputOnFocusLost&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputOnFocusLost() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct OnTextInputReturn {
        Update update;
        TextInputValid valid;
        OnTextInputReturn& operator=(const OnTextInputReturn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        OnTextInputReturn(const OnTextInputReturn&) = delete; /* disable copy constructor, use explicit .clone() */
        OnTextInputReturn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NumberInputOnValueChange {
        RefAny data;
        NumberInputOnValueChangeCallback callback;
        NumberInputOnValueChange& operator=(const NumberInputOnValueChange&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NumberInputOnValueChange(const NumberInputOnValueChange&) = delete; /* disable copy constructor, use explicit .clone() */
        NumberInputOnValueChange() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NumberInputOnFocusLost {
        RefAny data;
        NumberInputOnFocusLostCallback callback;
        NumberInputOnFocusLost& operator=(const NumberInputOnFocusLost&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NumberInputOnFocusLost(const NumberInputOnFocusLost&) = delete; /* disable copy constructor, use explicit .clone() */
        NumberInputOnFocusLost() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TabOnClick {
        RefAny data;
        TabOnClickCallback callback;
        TabOnClick& operator=(const TabOnClick&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TabOnClick(const TabOnClick&) = delete; /* disable copy constructor, use explicit .clone() */
        TabOnClick() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeAdded {
        RefAny data;
        NodeGraphOnNodeAddedCallback callback;
        NodeGraphOnNodeAdded& operator=(const NodeGraphOnNodeAdded&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeAdded(const NodeGraphOnNodeAdded&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeAdded() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeRemoved {
        RefAny data;
        NodeGraphOnNodeRemovedCallback callback;
        NodeGraphOnNodeRemoved& operator=(const NodeGraphOnNodeRemoved&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeRemoved(const NodeGraphOnNodeRemoved&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeRemoved() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeGraphDragged {
        RefAny data;
        NodeGraphOnNodeGraphDraggedCallback callback;
        NodeGraphOnNodeGraphDragged& operator=(const NodeGraphOnNodeGraphDragged&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeGraphDragged(const NodeGraphOnNodeGraphDragged&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeGraphDragged() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeDragged {
        RefAny data;
        NodeGraphOnNodeDraggedCallback callback;
        NodeGraphOnNodeDragged& operator=(const NodeGraphOnNodeDragged&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeDragged(const NodeGraphOnNodeDragged&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeDragged() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeConnected {
        RefAny data;
        NodeGraphOnNodeConnectedCallback callback;
        NodeGraphOnNodeConnected& operator=(const NodeGraphOnNodeConnected&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeConnected(const NodeGraphOnNodeConnected&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeConnected() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeInputDisconnected {
        RefAny data;
        NodeGraphOnNodeInputDisconnectedCallback callback;
        NodeGraphOnNodeInputDisconnected& operator=(const NodeGraphOnNodeInputDisconnected&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeInputDisconnected(const NodeGraphOnNodeInputDisconnected&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeInputDisconnected() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeOutputDisconnected {
        RefAny data;
        NodeGraphOnNodeOutputDisconnectedCallback callback;
        NodeGraphOnNodeOutputDisconnected& operator=(const NodeGraphOnNodeOutputDisconnected&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeOutputDisconnected(const NodeGraphOnNodeOutputDisconnected&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeOutputDisconnected() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphOnNodeFieldEdited {
        RefAny data;
        NodeGraphOnNodeFieldEditedCallback callback;
        NodeGraphOnNodeFieldEdited& operator=(const NodeGraphOnNodeFieldEdited&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphOnNodeFieldEdited(const NodeGraphOnNodeFieldEdited&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphOnNodeFieldEdited() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct OutputNodeAndIndex {
        NodeGraphNodeId node_id;
        size_t output_index;
        OutputNodeAndIndex& operator=(const OutputNodeAndIndex&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        OutputNodeAndIndex(const OutputNodeAndIndex&) = delete; /* disable copy constructor, use explicit .clone() */
        OutputNodeAndIndex() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputNodeAndIndex {
        NodeGraphNodeId node_id;
        size_t input_index;
        InputNodeAndIndex& operator=(const InputNodeAndIndex&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputNodeAndIndex(const InputNodeAndIndex&) = delete; /* disable copy constructor, use explicit .clone() */
        InputNodeAndIndex() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewOnLazyLoadScroll {
        RefAny data;
        ListViewOnLazyLoadScrollCallback callback;
        ListViewOnLazyLoadScroll& operator=(const ListViewOnLazyLoadScroll&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewOnLazyLoadScroll(const ListViewOnLazyLoadScroll&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewOnLazyLoadScroll() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewOnColumnClick {
        RefAny data;
        ListViewOnColumnClickCallback callback;
        ListViewOnColumnClick& operator=(const ListViewOnColumnClick&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewOnColumnClick(const ListViewOnColumnClick&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewOnColumnClick() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewOnRowClick {
        RefAny data;
        ListViewOnRowClickCallback callback;
        ListViewOnRowClick& operator=(const ListViewOnRowClick&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewOnRowClick(const ListViewOnRowClick&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewOnRowClick() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DropDownOnChoiceChange {
        RefAny data;
        DropDownOnChoiceChangeCallback callback;
        DropDownOnChoiceChange& operator=(const DropDownOnChoiceChange&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DropDownOnChoiceChange(const DropDownOnChoiceChange&) = delete; /* disable copy constructor, use explicit .clone() */
        DropDownOnChoiceChange() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ParentWithNodeDepth {
        size_t depth;
        NodeId node_id;
        ParentWithNodeDepth& operator=(const ParentWithNodeDepth&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ParentWithNodeDepth() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Gl {
        void* ptr;
        RendererType renderer_type;
        bool  run_destructor;
        Gl& operator=(const Gl&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Gl(const Gl&) = delete; /* disable copy constructor, use explicit .clone() */
        Gl() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RefstrVecRef {
        Refstr* ptr;
        size_t len;
        RefstrVecRef& operator=(const RefstrVecRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RefstrVecRef(const RefstrVecRef&) = delete; /* disable copy constructor, use explicit .clone() */
        RefstrVecRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ImageMask {
        ImageRef image;
        LogicalRect rect;
        bool  repeat;
        ImageMask& operator=(const ImageMask&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ImageMask(const ImageMask&) = delete; /* disable copy constructor, use explicit .clone() */
        ImageMask() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct FontMetrics {
        uint16_t units_per_em;
        uint16_t font_flags;
        int16_t x_min;
        int16_t y_min;
        int16_t x_max;
        int16_t y_max;
        int16_t ascender;
        int16_t descender;
        int16_t line_gap;
        uint16_t advance_width_max;
        int16_t min_left_side_bearing;
        int16_t min_right_side_bearing;
        int16_t x_max_extent;
        int16_t caret_slope_rise;
        int16_t caret_slope_run;
        int16_t caret_offset;
        uint16_t num_h_metrics;
        int16_t x_avg_char_width;
        uint16_t us_weight_class;
        uint16_t us_width_class;
        uint16_t fs_type;
        int16_t y_subscript_x_size;
        int16_t y_subscript_y_size;
        int16_t y_subscript_x_offset;
        int16_t y_subscript_y_offset;
        int16_t y_superscript_x_size;
        int16_t y_superscript_y_size;
        int16_t y_superscript_x_offset;
        int16_t y_superscript_y_offset;
        int16_t y_strikeout_size;
        int16_t y_strikeout_position;
        int16_t s_family_class;
        uint8_t panose[ 10];
        uint32_t ul_unicode_range1;
        uint32_t ul_unicode_range2;
        uint32_t ul_unicode_range3;
        uint32_t ul_unicode_range4;
        uint32_t ach_vend_id;
        uint16_t fs_selection;
        uint16_t us_first_char_index;
        uint16_t us_last_char_index;
        OptionI16 s_typo_ascender;
        OptionI16 s_typo_descender;
        OptionI16 s_typo_line_gap;
        OptionU16 us_win_ascent;
        OptionU16 us_win_descent;
        OptionU32 ul_code_page_range1;
        OptionU32 ul_code_page_range2;
        OptionI16 sx_height;
        OptionI16 s_cap_height;
        OptionU16 us_default_char;
        OptionU16 us_break_char;
        OptionU16 us_max_context;
        OptionU16 us_lower_optical_point_size;
        OptionU16 us_upper_optical_point_size;
        FontMetrics& operator=(const FontMetrics&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FontMetrics(const FontMetrics&) = delete; /* disable copy constructor, use explicit .clone() */
        FontMetrics() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgLine {
        SvgPoint start;
        SvgPoint end;
        SvgLine& operator=(const SvgLine&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgLine() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgQuadraticCurve {
        SvgPoint start;
        SvgPoint ctrl;
        SvgPoint end;
        SvgQuadraticCurve& operator=(const SvgQuadraticCurve&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgQuadraticCurve() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgCubicCurve {
        SvgPoint start;
        SvgPoint ctrl_1;
        SvgPoint ctrl_2;
        SvgPoint end;
        SvgCubicCurve& operator=(const SvgCubicCurve&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgCubicCurve() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgStringFormatOptions {
        bool  use_single_quote;
        Indent indent;
        Indent attributes_indent;
        SvgStringFormatOptions& operator=(const SvgStringFormatOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgStringFormatOptions(const SvgStringFormatOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgStringFormatOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgFillStyle {
        SvgLineJoin line_join;
        float miter_limit;
        float tolerance;
        SvgFillRule fill_rule;
        SvgTransform transform;
        bool  anti_alias;
        bool  high_quality_aa;
        SvgFillStyle& operator=(const SvgFillStyle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgFillStyle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InstantPtr {
        void* ptr;
        InstantPtrCloneFn clone_fn;
        InstantPtrDestructorFn destructor;
        bool  run_destructor;
        InstantPtr& operator=(const InstantPtr&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InstantPtr(const InstantPtr&) = delete; /* disable copy constructor, use explicit .clone() */
        InstantPtr() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class DurationTag {
       System,
       Tick,
    };
    
    struct DurationVariant_System { DurationTag tag; SystemTimeDiff payload; };
    struct DurationVariant_Tick { DurationTag tag; SystemTickDiff payload; };
    union Duration {
        DurationVariant_System System;
        DurationVariant_Tick Tick;
    };
    
    
    enum class ThreadSendMsgTag {
       TerminateThread,
       Tick,
       Custom,
    };
    
    struct ThreadSendMsgVariant_TerminateThread { ThreadSendMsgTag tag; };
    struct ThreadSendMsgVariant_Tick { ThreadSendMsgTag tag; };
    struct ThreadSendMsgVariant_Custom { ThreadSendMsgTag tag; RefAny payload; };
    union ThreadSendMsg {
        ThreadSendMsgVariant_TerminateThread TerminateThread;
        ThreadSendMsgVariant_Tick Tick;
        ThreadSendMsgVariant_Custom Custom;
    };
    
    
    struct ThreadWriteBackMsg {
        RefAny data;
        WriteBackCallback callback;
        ThreadWriteBackMsg& operator=(const ThreadWriteBackMsg&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ThreadWriteBackMsg(const ThreadWriteBackMsg&) = delete; /* disable copy constructor, use explicit .clone() */
        ThreadWriteBackMsg() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LogicalRectVec {
        LogicalRect* ptr;
        size_t len;
        size_t cap;
        LogicalRectVecDestructor destructor;
        LogicalRectVec& operator=(const LogicalRectVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LogicalRectVec(const LogicalRectVec&) = delete; /* disable copy constructor, use explicit .clone() */
        LogicalRectVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputOutputTypeIdVec {
        InputOutputTypeId* ptr;
        size_t len;
        size_t cap;
        InputOutputTypeIdVecDestructor destructor;
        InputOutputTypeIdVec& operator=(const InputOutputTypeIdVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputOutputTypeIdVec(const InputOutputTypeIdVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InputOutputTypeIdVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct OutputNodeAndIndexVec {
        OutputNodeAndIndex* ptr;
        size_t len;
        size_t cap;
        OutputNodeAndIndexVecDestructor destructor;
        OutputNodeAndIndexVec& operator=(const OutputNodeAndIndexVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        OutputNodeAndIndexVec(const OutputNodeAndIndexVec&) = delete; /* disable copy constructor, use explicit .clone() */
        OutputNodeAndIndexVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputNodeAndIndexVec {
        InputNodeAndIndex* ptr;
        size_t len;
        size_t cap;
        InputNodeAndIndexVecDestructor destructor;
        InputNodeAndIndexVec& operator=(const InputNodeAndIndexVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputNodeAndIndexVec(const InputNodeAndIndexVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InputNodeAndIndexVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct AccessibilityStateVec {
        AccessibilityState* ptr;
        size_t len;
        size_t cap;
        AccessibilityStateVecDestructor destructor;
        AccessibilityStateVec& operator=(const AccessibilityStateVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        AccessibilityStateVec(const AccessibilityStateVec&) = delete; /* disable copy constructor, use explicit .clone() */
        AccessibilityStateVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct AzMenuItem;
    struct MenuItemVec {
        MenuItem* ptr;
        size_t len;
        size_t cap;
        MenuItemVecDestructor destructor;
        MenuItemVec& operator=(const MenuItemVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MenuItemVec(const MenuItemVec&) = delete; /* disable copy constructor, use explicit .clone() */
        MenuItemVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct AzXmlNode;
    struct XmlNodeVec {
        XmlNode* ptr;
        size_t len;
        size_t cap;
        XmlNodeVecDestructor destructor;
        XmlNodeVec& operator=(const XmlNodeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        XmlNodeVec(const XmlNodeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        XmlNodeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineGlyphVec {
        InlineGlyph* ptr;
        size_t len;
        size_t cap;
        InlineGlyphVecDestructor destructor;
        InlineGlyphVec& operator=(const InlineGlyphVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineGlyphVec(const InlineGlyphVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineGlyphVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineTextHitVec {
        InlineTextHit* ptr;
        size_t len;
        size_t cap;
        InlineTextHitVecDestructor destructor;
        InlineTextHitVec& operator=(const InlineTextHitVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineTextHitVec(const InlineTextHitVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineTextHitVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct VideoModeVec {
        VideoMode* ptr;
        size_t len;
        size_t cap;
        VideoModeVecDestructor destructor;
        VideoModeVec& operator=(const VideoModeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VideoModeVec(const VideoModeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        VideoModeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct AzDom;
    struct DomVec {
        Dom* ptr;
        size_t len;
        size_t cap;
        DomVecDestructor destructor;
        DomVec& operator=(const DomVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DomVec(const DomVec&) = delete; /* disable copy constructor, use explicit .clone() */
        DomVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBackgroundPositionVec {
        StyleBackgroundPosition* ptr;
        size_t len;
        size_t cap;
        StyleBackgroundPositionVecDestructor destructor;
        StyleBackgroundPositionVec& operator=(const StyleBackgroundPositionVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBackgroundPositionVec(const StyleBackgroundPositionVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyleBackgroundPositionVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBackgroundRepeatVec {
        StyleBackgroundRepeat* ptr;
        size_t len;
        size_t cap;
        StyleBackgroundRepeatVecDestructor destructor;
        StyleBackgroundRepeatVec& operator=(const StyleBackgroundRepeatVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBackgroundRepeatVec(const StyleBackgroundRepeatVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyleBackgroundRepeatVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBackgroundSizeVec {
        StyleBackgroundSize* ptr;
        size_t len;
        size_t cap;
        StyleBackgroundSizeVecDestructor destructor;
        StyleBackgroundSizeVec& operator=(const StyleBackgroundSizeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBackgroundSizeVec(const StyleBackgroundSizeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyleBackgroundSizeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgVertexVec {
        SvgVertex* ptr;
        size_t len;
        size_t cap;
        SvgVertexVecDestructor destructor;
        SvgVertexVec& operator=(const SvgVertexVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgVertexVec(const SvgVertexVec&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgVertexVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct U32Vec {
        uint32_t* ptr;
        size_t len;
        size_t cap;
        U32VecDestructor destructor;
        U32Vec& operator=(const U32Vec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        U32Vec(const U32Vec&) = delete; /* disable copy constructor, use explicit .clone() */
        U32Vec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct XWindowTypeVec {
        XWindowType* ptr;
        size_t len;
        size_t cap;
        XWindowTypeVecDestructor destructor;
        XWindowTypeVec& operator=(const XWindowTypeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        XWindowTypeVec(const XWindowTypeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        XWindowTypeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct VirtualKeyCodeVec {
        VirtualKeyCode* ptr;
        size_t len;
        size_t cap;
        VirtualKeyCodeVecDestructor destructor;
        VirtualKeyCodeVec& operator=(const VirtualKeyCodeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VirtualKeyCodeVec(const VirtualKeyCodeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        VirtualKeyCodeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CascadeInfoVec {
        CascadeInfo* ptr;
        size_t len;
        size_t cap;
        CascadeInfoVecDestructor destructor;
        CascadeInfoVec& operator=(const CascadeInfoVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CascadeInfoVec(const CascadeInfoVec&) = delete; /* disable copy constructor, use explicit .clone() */
        CascadeInfoVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ScanCodeVec {
        uint32_t* ptr;
        size_t len;
        size_t cap;
        ScanCodeVecDestructor destructor;
        ScanCodeVec& operator=(const ScanCodeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ScanCodeVec(const ScanCodeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        ScanCodeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct U16Vec {
        uint16_t* ptr;
        size_t len;
        size_t cap;
        U16VecDestructor destructor;
        U16Vec& operator=(const U16Vec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        U16Vec(const U16Vec&) = delete; /* disable copy constructor, use explicit .clone() */
        U16Vec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct F32Vec {
        float* ptr;
        size_t len;
        size_t cap;
        F32VecDestructor destructor;
        F32Vec& operator=(const F32Vec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        F32Vec(const F32Vec&) = delete; /* disable copy constructor, use explicit .clone() */
        F32Vec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct U8Vec {
        uint8_t* ptr;
        size_t len;
        size_t cap;
        U8VecDestructor destructor;
        U8Vec& operator=(const U8Vec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        U8Vec(const U8Vec&) = delete; /* disable copy constructor, use explicit .clone() */
        U8Vec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLuintVec {
        uint32_t* ptr;
        size_t len;
        size_t cap;
        GLuintVecDestructor destructor;
        GLuintVec& operator=(const GLuintVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLuintVec(const GLuintVec&) = delete; /* disable copy constructor, use explicit .clone() */
        GLuintVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GLintVec {
        int32_t* ptr;
        size_t len;
        size_t cap;
        GLintVecDestructor destructor;
        GLintVec& operator=(const GLintVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GLintVec(const GLintVec&) = delete; /* disable copy constructor, use explicit .clone() */
        GLintVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NormalizedLinearColorStopVec {
        NormalizedLinearColorStop* ptr;
        size_t len;
        size_t cap;
        NormalizedLinearColorStopVecDestructor destructor;
        NormalizedLinearColorStopVec& operator=(const NormalizedLinearColorStopVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NormalizedLinearColorStopVec(const NormalizedLinearColorStopVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NormalizedLinearColorStopVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NormalizedRadialColorStopVec {
        NormalizedRadialColorStop* ptr;
        size_t len;
        size_t cap;
        NormalizedRadialColorStopVecDestructor destructor;
        NormalizedRadialColorStopVec& operator=(const NormalizedRadialColorStopVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NormalizedRadialColorStopVec(const NormalizedRadialColorStopVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NormalizedRadialColorStopVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeIdVec {
        NodeId* ptr;
        size_t len;
        size_t cap;
        NodeIdVecDestructor destructor;
        NodeIdVec& operator=(const NodeIdVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeIdVec(const NodeIdVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeIdVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeHierarchyItemVec {
        NodeHierarchyItem* ptr;
        size_t len;
        size_t cap;
        NodeHierarchyItemVecDestructor destructor;
        NodeHierarchyItemVec& operator=(const NodeHierarchyItemVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeHierarchyItemVec(const NodeHierarchyItemVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeHierarchyItemVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ParentWithNodeDepthVec {
        ParentWithNodeDepth* ptr;
        size_t len;
        size_t cap;
        ParentWithNodeDepthVecDestructor destructor;
        ParentWithNodeDepthVec& operator=(const ParentWithNodeDepthVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ParentWithNodeDepthVec(const ParentWithNodeDepthVec&) = delete; /* disable copy constructor, use explicit .clone() */
        ParentWithNodeDepthVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class OptionSvgPointTag {
       None,
       Some,
    };
    
    struct OptionSvgPointVariant_None { OptionSvgPointTag tag; };
    struct OptionSvgPointVariant_Some { OptionSvgPointTag tag; SvgPoint payload; };
    union OptionSvgPoint {
        OptionSvgPointVariant_None None;
        OptionSvgPointVariant_Some Some;
    };
    
    
    enum class OptionListViewOnRowClickTag {
       None,
       Some,
    };
    
    struct OptionListViewOnRowClickVariant_None { OptionListViewOnRowClickTag tag; };
    struct OptionListViewOnRowClickVariant_Some { OptionListViewOnRowClickTag tag; ListViewOnRowClick payload; };
    union OptionListViewOnRowClick {
        OptionListViewOnRowClickVariant_None None;
        OptionListViewOnRowClickVariant_Some Some;
    };
    
    
    enum class OptionListViewOnColumnClickTag {
       None,
       Some,
    };
    
    struct OptionListViewOnColumnClickVariant_None { OptionListViewOnColumnClickTag tag; };
    struct OptionListViewOnColumnClickVariant_Some { OptionListViewOnColumnClickTag tag; ListViewOnColumnClick payload; };
    union OptionListViewOnColumnClick {
        OptionListViewOnColumnClickVariant_None None;
        OptionListViewOnColumnClickVariant_Some Some;
    };
    
    
    enum class OptionListViewOnLazyLoadScrollTag {
       None,
       Some,
    };
    
    struct OptionListViewOnLazyLoadScrollVariant_None { OptionListViewOnLazyLoadScrollTag tag; };
    struct OptionListViewOnLazyLoadScrollVariant_Some { OptionListViewOnLazyLoadScrollTag tag; ListViewOnLazyLoadScroll payload; };
    union OptionListViewOnLazyLoadScroll {
        OptionListViewOnLazyLoadScrollVariant_None None;
        OptionListViewOnLazyLoadScrollVariant_Some Some;
    };
    
    
    enum class OptionPixelValueNoPercentTag {
       None,
       Some,
    };
    
    struct OptionPixelValueNoPercentVariant_None { OptionPixelValueNoPercentTag tag; };
    struct OptionPixelValueNoPercentVariant_Some { OptionPixelValueNoPercentTag tag; PixelValueNoPercent payload; };
    union OptionPixelValueNoPercent {
        OptionPixelValueNoPercentVariant_None None;
        OptionPixelValueNoPercentVariant_Some Some;
    };
    
    
    enum class OptionDropDownOnChoiceChangeTag {
       None,
       Some,
    };
    
    struct OptionDropDownOnChoiceChangeVariant_None { OptionDropDownOnChoiceChangeTag tag; };
    struct OptionDropDownOnChoiceChangeVariant_Some { OptionDropDownOnChoiceChangeTag tag; DropDownOnChoiceChange payload; };
    union OptionDropDownOnChoiceChange {
        OptionDropDownOnChoiceChangeVariant_None None;
        OptionDropDownOnChoiceChangeVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeAddedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeAddedVariant_None { OptionNodeGraphOnNodeAddedTag tag; };
    struct OptionNodeGraphOnNodeAddedVariant_Some { OptionNodeGraphOnNodeAddedTag tag; NodeGraphOnNodeAdded payload; };
    union OptionNodeGraphOnNodeAdded {
        OptionNodeGraphOnNodeAddedVariant_None None;
        OptionNodeGraphOnNodeAddedVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeRemovedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeRemovedVariant_None { OptionNodeGraphOnNodeRemovedTag tag; };
    struct OptionNodeGraphOnNodeRemovedVariant_Some { OptionNodeGraphOnNodeRemovedTag tag; NodeGraphOnNodeRemoved payload; };
    union OptionNodeGraphOnNodeRemoved {
        OptionNodeGraphOnNodeRemovedVariant_None None;
        OptionNodeGraphOnNodeRemovedVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeGraphDraggedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeGraphDraggedVariant_None { OptionNodeGraphOnNodeGraphDraggedTag tag; };
    struct OptionNodeGraphOnNodeGraphDraggedVariant_Some { OptionNodeGraphOnNodeGraphDraggedTag tag; NodeGraphOnNodeGraphDragged payload; };
    union OptionNodeGraphOnNodeGraphDragged {
        OptionNodeGraphOnNodeGraphDraggedVariant_None None;
        OptionNodeGraphOnNodeGraphDraggedVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeDraggedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeDraggedVariant_None { OptionNodeGraphOnNodeDraggedTag tag; };
    struct OptionNodeGraphOnNodeDraggedVariant_Some { OptionNodeGraphOnNodeDraggedTag tag; NodeGraphOnNodeDragged payload; };
    union OptionNodeGraphOnNodeDragged {
        OptionNodeGraphOnNodeDraggedVariant_None None;
        OptionNodeGraphOnNodeDraggedVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeConnectedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeConnectedVariant_None { OptionNodeGraphOnNodeConnectedTag tag; };
    struct OptionNodeGraphOnNodeConnectedVariant_Some { OptionNodeGraphOnNodeConnectedTag tag; NodeGraphOnNodeConnected payload; };
    union OptionNodeGraphOnNodeConnected {
        OptionNodeGraphOnNodeConnectedVariant_None None;
        OptionNodeGraphOnNodeConnectedVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeInputDisconnectedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeInputDisconnectedVariant_None { OptionNodeGraphOnNodeInputDisconnectedTag tag; };
    struct OptionNodeGraphOnNodeInputDisconnectedVariant_Some { OptionNodeGraphOnNodeInputDisconnectedTag tag; NodeGraphOnNodeInputDisconnected payload; };
    union OptionNodeGraphOnNodeInputDisconnected {
        OptionNodeGraphOnNodeInputDisconnectedVariant_None None;
        OptionNodeGraphOnNodeInputDisconnectedVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeOutputDisconnectedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeOutputDisconnectedVariant_None { OptionNodeGraphOnNodeOutputDisconnectedTag tag; };
    struct OptionNodeGraphOnNodeOutputDisconnectedVariant_Some { OptionNodeGraphOnNodeOutputDisconnectedTag tag; NodeGraphOnNodeOutputDisconnected payload; };
    union OptionNodeGraphOnNodeOutputDisconnected {
        OptionNodeGraphOnNodeOutputDisconnectedVariant_None None;
        OptionNodeGraphOnNodeOutputDisconnectedVariant_Some Some;
    };
    
    
    enum class OptionNodeGraphOnNodeFieldEditedTag {
       None,
       Some,
    };
    
    struct OptionNodeGraphOnNodeFieldEditedVariant_None { OptionNodeGraphOnNodeFieldEditedTag tag; };
    struct OptionNodeGraphOnNodeFieldEditedVariant_Some { OptionNodeGraphOnNodeFieldEditedTag tag; NodeGraphOnNodeFieldEdited payload; };
    union OptionNodeGraphOnNodeFieldEdited {
        OptionNodeGraphOnNodeFieldEditedVariant_None None;
        OptionNodeGraphOnNodeFieldEditedVariant_Some Some;
    };
    
    
    enum class OptionColorInputOnValueChangeTag {
       None,
       Some,
    };
    
    struct OptionColorInputOnValueChangeVariant_None { OptionColorInputOnValueChangeTag tag; };
    struct OptionColorInputOnValueChangeVariant_Some { OptionColorInputOnValueChangeTag tag; ColorInputOnValueChange payload; };
    union OptionColorInputOnValueChange {
        OptionColorInputOnValueChangeVariant_None None;
        OptionColorInputOnValueChangeVariant_Some Some;
    };
    
    
    enum class OptionButtonOnClickTag {
       None,
       Some,
    };
    
    struct OptionButtonOnClickVariant_None { OptionButtonOnClickTag tag; };
    struct OptionButtonOnClickVariant_Some { OptionButtonOnClickTag tag; ButtonOnClick payload; };
    union OptionButtonOnClick {
        OptionButtonOnClickVariant_None None;
        OptionButtonOnClickVariant_Some Some;
    };
    
    
    enum class OptionTabOnClickTag {
       None,
       Some,
    };
    
    struct OptionTabOnClickVariant_None { OptionTabOnClickTag tag; };
    struct OptionTabOnClickVariant_Some { OptionTabOnClickTag tag; TabOnClick payload; };
    union OptionTabOnClick {
        OptionTabOnClickVariant_None None;
        OptionTabOnClickVariant_Some Some;
    };
    
    
    enum class OptionFileInputOnPathChangeTag {
       None,
       Some,
    };
    
    struct OptionFileInputOnPathChangeVariant_None { OptionFileInputOnPathChangeTag tag; };
    struct OptionFileInputOnPathChangeVariant_Some { OptionFileInputOnPathChangeTag tag; FileInputOnPathChange payload; };
    union OptionFileInputOnPathChange {
        OptionFileInputOnPathChangeVariant_None None;
        OptionFileInputOnPathChangeVariant_Some Some;
    };
    
    
    enum class OptionCheckBoxOnToggleTag {
       None,
       Some,
    };
    
    struct OptionCheckBoxOnToggleVariant_None { OptionCheckBoxOnToggleTag tag; };
    struct OptionCheckBoxOnToggleVariant_Some { OptionCheckBoxOnToggleTag tag; CheckBoxOnToggle payload; };
    union OptionCheckBoxOnToggle {
        OptionCheckBoxOnToggleVariant_None None;
        OptionCheckBoxOnToggleVariant_Some Some;
    };
    
    
    enum class OptionTextInputOnTextInputTag {
       None,
       Some,
    };
    
    struct OptionTextInputOnTextInputVariant_None { OptionTextInputOnTextInputTag tag; };
    struct OptionTextInputOnTextInputVariant_Some { OptionTextInputOnTextInputTag tag; TextInputOnTextInput payload; };
    union OptionTextInputOnTextInput {
        OptionTextInputOnTextInputVariant_None None;
        OptionTextInputOnTextInputVariant_Some Some;
    };
    
    
    enum class OptionTextInputOnVirtualKeyDownTag {
       None,
       Some,
    };
    
    struct OptionTextInputOnVirtualKeyDownVariant_None { OptionTextInputOnVirtualKeyDownTag tag; };
    struct OptionTextInputOnVirtualKeyDownVariant_Some { OptionTextInputOnVirtualKeyDownTag tag; TextInputOnVirtualKeyDown payload; };
    union OptionTextInputOnVirtualKeyDown {
        OptionTextInputOnVirtualKeyDownVariant_None None;
        OptionTextInputOnVirtualKeyDownVariant_Some Some;
    };
    
    
    enum class OptionTextInputOnFocusLostTag {
       None,
       Some,
    };
    
    struct OptionTextInputOnFocusLostVariant_None { OptionTextInputOnFocusLostTag tag; };
    struct OptionTextInputOnFocusLostVariant_Some { OptionTextInputOnFocusLostTag tag; TextInputOnFocusLost payload; };
    union OptionTextInputOnFocusLost {
        OptionTextInputOnFocusLostVariant_None None;
        OptionTextInputOnFocusLostVariant_Some Some;
    };
    
    
    enum class OptionTextInputSelectionTag {
       None,
       Some,
    };
    
    struct OptionTextInputSelectionVariant_None { OptionTextInputSelectionTag tag; };
    struct OptionTextInputSelectionVariant_Some { OptionTextInputSelectionTag tag; TextInputSelection payload; };
    union OptionTextInputSelection {
        OptionTextInputSelectionVariant_None None;
        OptionTextInputSelectionVariant_Some Some;
    };
    
    
    enum class OptionNumberInputOnFocusLostTag {
       None,
       Some,
    };
    
    struct OptionNumberInputOnFocusLostVariant_None { OptionNumberInputOnFocusLostTag tag; };
    struct OptionNumberInputOnFocusLostVariant_Some { OptionNumberInputOnFocusLostTag tag; NumberInputOnFocusLost payload; };
    union OptionNumberInputOnFocusLost {
        OptionNumberInputOnFocusLostVariant_None None;
        OptionNumberInputOnFocusLostVariant_Some Some;
    };
    
    
    enum class OptionNumberInputOnValueChangeTag {
       None,
       Some,
    };
    
    struct OptionNumberInputOnValueChangeVariant_None { OptionNumberInputOnValueChangeTag tag; };
    struct OptionNumberInputOnValueChangeVariant_Some { OptionNumberInputOnValueChangeTag tag; NumberInputOnValueChange payload; };
    union OptionNumberInputOnValueChange {
        OptionNumberInputOnValueChangeVariant_None None;
        OptionNumberInputOnValueChangeVariant_Some Some;
    };
    
    
    enum class OptionMenuItemIconTag {
       None,
       Some,
    };
    
    struct OptionMenuItemIconVariant_None { OptionMenuItemIconTag tag; };
    struct OptionMenuItemIconVariant_Some { OptionMenuItemIconTag tag; MenuItemIcon payload; };
    union OptionMenuItemIcon {
        OptionMenuItemIconVariant_None None;
        OptionMenuItemIconVariant_Some Some;
    };
    
    
    enum class OptionMenuCallbackTag {
       None,
       Some,
    };
    
    struct OptionMenuCallbackVariant_None { OptionMenuCallbackTag tag; };
    struct OptionMenuCallbackVariant_Some { OptionMenuCallbackTag tag; MenuCallback payload; };
    union OptionMenuCallback {
        OptionMenuCallbackVariant_None None;
        OptionMenuCallbackVariant_Some Some;
    };
    
    
    enum class OptionPositionInfoTag {
       None,
       Some,
    };
    
    struct OptionPositionInfoVariant_None { OptionPositionInfoTag tag; };
    struct OptionPositionInfoVariant_Some { OptionPositionInfoTag tag; PositionInfo payload; };
    union OptionPositionInfo {
        OptionPositionInfoVariant_None None;
        OptionPositionInfoVariant_Some Some;
    };
    
    
    enum class OptionTimerIdTag {
       None,
       Some,
    };
    
    struct OptionTimerIdVariant_None { OptionTimerIdTag tag; };
    struct OptionTimerIdVariant_Some { OptionTimerIdTag tag; TimerId payload; };
    union OptionTimerId {
        OptionTimerIdVariant_None None;
        OptionTimerIdVariant_Some Some;
    };
    
    
    enum class OptionThreadIdTag {
       None,
       Some,
    };
    
    struct OptionThreadIdVariant_None { OptionThreadIdTag tag; };
    struct OptionThreadIdVariant_Some { OptionThreadIdTag tag; ThreadId payload; };
    union OptionThreadId {
        OptionThreadIdVariant_None None;
        OptionThreadIdVariant_Some Some;
    };
    
    
    enum class OptionImageRefTag {
       None,
       Some,
    };
    
    struct OptionImageRefVariant_None { OptionImageRefTag tag; };
    struct OptionImageRefVariant_Some { OptionImageRefTag tag; ImageRef payload; };
    union OptionImageRef {
        OptionImageRefVariant_None None;
        OptionImageRefVariant_Some Some;
    };
    
    
    enum class OptionFontRefTag {
       None,
       Some,
    };
    
    struct OptionFontRefVariant_None { OptionFontRefTag tag; };
    struct OptionFontRefVariant_Some { OptionFontRefTag tag; FontRef payload; };
    union OptionFontRef {
        OptionFontRefVariant_None None;
        OptionFontRefVariant_Some Some;
    };
    
    
    enum class OptionSystemClipboardTag {
       None,
       Some,
    };
    
    struct OptionSystemClipboardVariant_None { OptionSystemClipboardTag tag; };
    struct OptionSystemClipboardVariant_Some { OptionSystemClipboardTag tag; SystemClipboard payload; };
    union OptionSystemClipboard {
        OptionSystemClipboardVariant_None None;
        OptionSystemClipboardVariant_Some Some;
    };
    
    
    enum class OptionGlTag {
       None,
       Some,
    };
    
    struct OptionGlVariant_None { OptionGlTag tag; };
    struct OptionGlVariant_Some { OptionGlTag tag; Gl payload; };
    union OptionGl {
        OptionGlVariant_None None;
        OptionGlVariant_Some Some;
    };
    
    
    enum class OptionPercentageValueTag {
       None,
       Some,
    };
    
    struct OptionPercentageValueVariant_None { OptionPercentageValueTag tag; };
    struct OptionPercentageValueVariant_Some { OptionPercentageValueTag tag; PercentageValue payload; };
    union OptionPercentageValue {
        OptionPercentageValueVariant_None None;
        OptionPercentageValueVariant_Some Some;
    };
    
    
    enum class OptionAngleValueTag {
       None,
       Some,
    };
    
    struct OptionAngleValueVariant_None { OptionAngleValueTag tag; };
    struct OptionAngleValueVariant_Some { OptionAngleValueTag tag; AngleValue payload; };
    union OptionAngleValue {
        OptionAngleValueVariant_None None;
        OptionAngleValueVariant_Some Some;
    };
    
    
    enum class OptionRendererOptionsTag {
       None,
       Some,
    };
    
    struct OptionRendererOptionsVariant_None { OptionRendererOptionsTag tag; };
    struct OptionRendererOptionsVariant_Some { OptionRendererOptionsTag tag; RendererOptions payload; };
    union OptionRendererOptions {
        OptionRendererOptionsVariant_None None;
        OptionRendererOptionsVariant_Some Some;
    };
    
    
    enum class OptionCallbackTag {
       None,
       Some,
    };
    
    struct OptionCallbackVariant_None { OptionCallbackTag tag; };
    struct OptionCallbackVariant_Some { OptionCallbackTag tag; Callback payload; };
    union OptionCallback {
        OptionCallbackVariant_None None;
        OptionCallbackVariant_Some Some;
    };
    
    
    enum class OptionThreadSendMsgTag {
       None,
       Some,
    };
    
    struct OptionThreadSendMsgVariant_None { OptionThreadSendMsgTag tag; };
    struct OptionThreadSendMsgVariant_Some { OptionThreadSendMsgTag tag; ThreadSendMsg payload; };
    union OptionThreadSendMsg {
        OptionThreadSendMsgVariant_None None;
        OptionThreadSendMsgVariant_Some Some;
    };
    
    
    enum class OptionLayoutRectTag {
       None,
       Some,
    };
    
    struct OptionLayoutRectVariant_None { OptionLayoutRectTag tag; };
    struct OptionLayoutRectVariant_Some { OptionLayoutRectTag tag; LayoutRect payload; };
    union OptionLayoutRect {
        OptionLayoutRectVariant_None None;
        OptionLayoutRectVariant_Some Some;
    };
    
    
    enum class OptionRefAnyTag {
       None,
       Some,
    };
    
    struct OptionRefAnyVariant_None { OptionRefAnyTag tag; };
    struct OptionRefAnyVariant_Some { OptionRefAnyTag tag; RefAny payload; };
    union OptionRefAny {
        OptionRefAnyVariant_None None;
        OptionRefAnyVariant_Some Some;
    };
    
    
    enum class OptionLayoutPointTag {
       None,
       Some,
    };
    
    struct OptionLayoutPointVariant_None { OptionLayoutPointTag tag; };
    struct OptionLayoutPointVariant_Some { OptionLayoutPointTag tag; LayoutPoint payload; };
    union OptionLayoutPoint {
        OptionLayoutPointVariant_None None;
        OptionLayoutPointVariant_Some Some;
    };
    
    
    enum class OptionLayoutSizeTag {
       None,
       Some,
    };
    
    struct OptionLayoutSizeVariant_None { OptionLayoutSizeTag tag; };
    struct OptionLayoutSizeVariant_Some { OptionLayoutSizeTag tag; LayoutSize payload; };
    union OptionLayoutSize {
        OptionLayoutSizeVariant_None None;
        OptionLayoutSizeVariant_Some Some;
    };
    
    
    enum class OptionWindowThemeTag {
       None,
       Some,
    };
    
    struct OptionWindowThemeVariant_None { OptionWindowThemeTag tag; };
    struct OptionWindowThemeVariant_Some { OptionWindowThemeTag tag; WindowTheme payload; };
    union OptionWindowTheme {
        OptionWindowThemeVariant_None None;
        OptionWindowThemeVariant_Some Some;
    };
    
    
    enum class OptionNodeIdTag {
       None,
       Some,
    };
    
    struct OptionNodeIdVariant_None { OptionNodeIdTag tag; };
    struct OptionNodeIdVariant_Some { OptionNodeIdTag tag; NodeId payload; };
    union OptionNodeId {
        OptionNodeIdVariant_None None;
        OptionNodeIdVariant_Some Some;
    };
    
    
    enum class OptionDomNodeIdTag {
       None,
       Some,
    };
    
    struct OptionDomNodeIdVariant_None { OptionDomNodeIdTag tag; };
    struct OptionDomNodeIdVariant_Some { OptionDomNodeIdTag tag; DomNodeId payload; };
    union OptionDomNodeId {
        OptionDomNodeIdVariant_None None;
        OptionDomNodeIdVariant_Some Some;
    };
    
    
    enum class OptionColorUTag {
       None,
       Some,
    };
    
    struct OptionColorUVariant_None { OptionColorUTag tag; };
    struct OptionColorUVariant_Some { OptionColorUTag tag; ColorU payload; };
    union OptionColorU {
        OptionColorUVariant_None None;
        OptionColorUVariant_Some Some;
    };
    
    
    enum class OptionSvgDashPatternTag {
       None,
       Some,
    };
    
    struct OptionSvgDashPatternVariant_None { OptionSvgDashPatternTag tag; };
    struct OptionSvgDashPatternVariant_Some { OptionSvgDashPatternTag tag; SvgDashPattern payload; };
    union OptionSvgDashPattern {
        OptionSvgDashPatternVariant_None None;
        OptionSvgDashPatternVariant_Some Some;
    };
    
    
    enum class OptionLogicalPositionTag {
       None,
       Some,
    };
    
    struct OptionLogicalPositionVariant_None { OptionLogicalPositionTag tag; };
    struct OptionLogicalPositionVariant_Some { OptionLogicalPositionTag tag; LogicalPosition payload; };
    union OptionLogicalPosition {
        OptionLogicalPositionVariant_None None;
        OptionLogicalPositionVariant_Some Some;
    };
    
    
    enum class OptionPhysicalPositionI32Tag {
       None,
       Some,
    };
    
    struct OptionPhysicalPositionI32Variant_None { OptionPhysicalPositionI32Tag tag; };
    struct OptionPhysicalPositionI32Variant_Some { OptionPhysicalPositionI32Tag tag; PhysicalPositionI32 payload; };
    union OptionPhysicalPositionI32 {
        OptionPhysicalPositionI32Variant_None None;
        OptionPhysicalPositionI32Variant_Some Some;
    };
    
    
    enum class OptionMouseCursorTypeTag {
       None,
       Some,
    };
    
    struct OptionMouseCursorTypeVariant_None { OptionMouseCursorTypeTag tag; };
    struct OptionMouseCursorTypeVariant_Some { OptionMouseCursorTypeTag tag; MouseCursorType payload; };
    union OptionMouseCursorType {
        OptionMouseCursorTypeVariant_None None;
        OptionMouseCursorTypeVariant_Some Some;
    };
    
    
    enum class OptionLogicalSizeTag {
       None,
       Some,
    };
    
    struct OptionLogicalSizeVariant_None { OptionLogicalSizeTag tag; };
    struct OptionLogicalSizeVariant_Some { OptionLogicalSizeTag tag; LogicalSize payload; };
    union OptionLogicalSize {
        OptionLogicalSizeVariant_None None;
        OptionLogicalSizeVariant_Some Some;
    };
    
    
    enum class OptionVirtualKeyCodeTag {
       None,
       Some,
    };
    
    struct OptionVirtualKeyCodeVariant_None { OptionVirtualKeyCodeTag tag; };
    struct OptionVirtualKeyCodeVariant_Some { OptionVirtualKeyCodeTag tag; VirtualKeyCode payload; };
    union OptionVirtualKeyCode {
        OptionVirtualKeyCodeVariant_None None;
        OptionVirtualKeyCodeVariant_Some Some;
    };
    
    
    enum class OptionImageMaskTag {
       None,
       Some,
    };
    
    struct OptionImageMaskVariant_None { OptionImageMaskTag tag; };
    struct OptionImageMaskVariant_Some { OptionImageMaskTag tag; ImageMask payload; };
    union OptionImageMask {
        OptionImageMaskVariant_None None;
        OptionImageMaskVariant_Some Some;
    };
    
    
    enum class OptionTabIndexTag {
       None,
       Some,
    };
    
    struct OptionTabIndexVariant_None { OptionTabIndexTag tag; };
    struct OptionTabIndexVariant_Some { OptionTabIndexTag tag; TabIndex payload; };
    union OptionTabIndex {
        OptionTabIndexVariant_None None;
        OptionTabIndexVariant_Some Some;
    };
    
    
    enum class OptionTagIdTag {
       None,
       Some,
    };
    
    struct OptionTagIdVariant_None { OptionTagIdTag tag; };
    struct OptionTagIdVariant_Some { OptionTagIdTag tag; TagId payload; };
    union OptionTagId {
        OptionTagIdVariant_None None;
        OptionTagIdVariant_Some Some;
    };
    
    
    enum class OptionDurationTag {
       None,
       Some,
    };
    
    struct OptionDurationVariant_None { OptionDurationTag tag; };
    struct OptionDurationVariant_Some { OptionDurationTag tag; Duration payload; };
    union OptionDuration {
        OptionDurationVariant_None None;
        OptionDurationVariant_Some Some;
    };
    
    
    enum class OptionU8VecTag {
       None,
       Some,
    };
    
    struct OptionU8VecVariant_None { OptionU8VecTag tag; };
    struct OptionU8VecVariant_Some { OptionU8VecTag tag; U8Vec payload; };
    union OptionU8Vec {
        OptionU8VecVariant_None None;
        OptionU8VecVariant_Some Some;
    };
    
    
    enum class OptionU8VecRefTag {
       None,
       Some,
    };
    
    struct OptionU8VecRefVariant_None { OptionU8VecRefTag tag; };
    struct OptionU8VecRefVariant_Some { OptionU8VecRefTag tag; U8VecRef payload; };
    union OptionU8VecRef {
        OptionU8VecRefVariant_None None;
        OptionU8VecRefVariant_Some Some;
    };
    
    
    enum class ResultU8VecEncodeImageErrorTag {
       Ok,
       Err,
    };
    
    struct ResultU8VecEncodeImageErrorVariant_Ok { ResultU8VecEncodeImageErrorTag tag; U8Vec payload; };
    struct ResultU8VecEncodeImageErrorVariant_Err { ResultU8VecEncodeImageErrorTag tag; EncodeImageError payload; };
    union ResultU8VecEncodeImageError {
        ResultU8VecEncodeImageErrorVariant_Ok Ok;
        ResultU8VecEncodeImageErrorVariant_Err Err;
    };
    
    
    struct NonXmlCharError {
        uint32_t ch;
        SvgParseErrorPosition pos;
        NonXmlCharError& operator=(const NonXmlCharError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NonXmlCharError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InvalidCharError {
        uint8_t expected;
        uint8_t got;
        SvgParseErrorPosition pos;
        InvalidCharError& operator=(const InvalidCharError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InvalidCharError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InvalidCharMultipleError {
        uint8_t expected;
        U8Vec got;
        SvgParseErrorPosition pos;
        InvalidCharMultipleError& operator=(const InvalidCharMultipleError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InvalidCharMultipleError(const InvalidCharMultipleError&) = delete; /* disable copy constructor, use explicit .clone() */
        InvalidCharMultipleError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InvalidQuoteError {
        uint8_t got;
        SvgParseErrorPosition pos;
        InvalidQuoteError& operator=(const InvalidQuoteError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InvalidQuoteError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InvalidSpaceError {
        uint8_t got;
        SvgParseErrorPosition pos;
        InvalidSpaceError& operator=(const InvalidSpaceError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InvalidSpaceError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct AppConfig {
        LayoutSolver layout_solver;
        AppLogLevel log_level;
        bool  enable_visual_panic_hook;
        bool  enable_logging_on_panic;
        bool  enable_tab_navigation;
        SystemCallbacks system_callbacks;
        AppConfig& operator=(const AppConfig&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        AppConfig(const AppConfig&) = delete; /* disable copy constructor, use explicit .clone() */
        AppConfig() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SmallWindowIconBytes {
        IconKey key;
        U8Vec rgba_bytes;
        SmallWindowIconBytes& operator=(const SmallWindowIconBytes&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SmallWindowIconBytes(const SmallWindowIconBytes&) = delete; /* disable copy constructor, use explicit .clone() */
        SmallWindowIconBytes() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LargeWindowIconBytes {
        IconKey key;
        U8Vec rgba_bytes;
        LargeWindowIconBytes& operator=(const LargeWindowIconBytes&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LargeWindowIconBytes(const LargeWindowIconBytes&) = delete; /* disable copy constructor, use explicit .clone() */
        LargeWindowIconBytes() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class WindowIconTag {
       Small,
       Large,
    };
    
    struct WindowIconVariant_Small { WindowIconTag tag; SmallWindowIconBytes payload; };
    struct WindowIconVariant_Large { WindowIconTag tag; LargeWindowIconBytes payload; };
    union WindowIcon {
        WindowIconVariant_Small Small;
        WindowIconVariant_Large Large;
    };
    
    
    struct TaskBarIcon {
        IconKey key;
        U8Vec rgba_bytes;
        TaskBarIcon& operator=(const TaskBarIcon&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TaskBarIcon(const TaskBarIcon&) = delete; /* disable copy constructor, use explicit .clone() */
        TaskBarIcon() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WindowSize {
        LogicalSize dimensions;
        uint32_t dpi;
        OptionLogicalSize min_dimensions;
        OptionLogicalSize max_dimensions;
        WindowSize& operator=(const WindowSize&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WindowSize() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct KeyboardState {
        OptionChar current_char;
        OptionVirtualKeyCode current_virtual_keycode;
        VirtualKeyCodeVec pressed_virtual_keycodes;
        ScanCodeVec pressed_scancodes;
        KeyboardState& operator=(const KeyboardState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        KeyboardState(const KeyboardState&) = delete; /* disable copy constructor, use explicit .clone() */
        KeyboardState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct MouseState {
        OptionMouseCursorType mouse_cursor_type;
        CursorPosition cursor_position;
        bool  is_cursor_locked;
        bool  left_down;
        bool  right_down;
        bool  middle_down;
        OptionF32 scroll_x;
        OptionF32 scroll_y;
        MouseState& operator=(const MouseState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MouseState(const MouseState&) = delete; /* disable copy constructor, use explicit .clone() */
        MouseState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct MarshaledLayoutCallback {
        RefAny marshal_data;
        MarshaledLayoutCallbackInner cb;
        MarshaledLayoutCallback& operator=(const MarshaledLayoutCallback&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MarshaledLayoutCallback(const MarshaledLayoutCallback&) = delete; /* disable copy constructor, use explicit .clone() */
        MarshaledLayoutCallback() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineTextContents {
        InlineGlyphVec glyphs;
        LogicalRect bounds;
        InlineTextContents& operator=(const InlineTextContents&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineTextContents(const InlineTextContents&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineTextContents() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ResolvedTextLayoutOptions {
        float font_size_px;
        OptionF32 line_height;
        OptionF32 letter_spacing;
        OptionF32 word_spacing;
        OptionF32 tab_width;
        OptionF32 max_horizontal_width;
        OptionF32 leading;
        LogicalRectVec holes;
        ResolvedTextLayoutOptions& operator=(const ResolvedTextLayoutOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ResolvedTextLayoutOptions(const ResolvedTextLayoutOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        ResolvedTextLayoutOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class AnimationEasingTag {
       Ease,
       Linear,
       EaseIn,
       EaseOut,
       EaseInOut,
       CubicBezier,
    };
    
    struct AnimationEasingVariant_Ease { AnimationEasingTag tag; };
    struct AnimationEasingVariant_Linear { AnimationEasingTag tag; };
    struct AnimationEasingVariant_EaseIn { AnimationEasingTag tag; };
    struct AnimationEasingVariant_EaseOut { AnimationEasingTag tag; };
    struct AnimationEasingVariant_EaseInOut { AnimationEasingTag tag; };
    struct AnimationEasingVariant_CubicBezier { AnimationEasingTag tag; SvgCubicCurve payload; };
    union AnimationEasing {
        AnimationEasingVariant_Ease Ease;
        AnimationEasingVariant_Linear Linear;
        AnimationEasingVariant_EaseIn EaseIn;
        AnimationEasingVariant_EaseOut EaseOut;
        AnimationEasingVariant_EaseInOut EaseInOut;
        AnimationEasingVariant_CubicBezier CubicBezier;
    };
    
    
    struct RenderImageCallbackInfo {
        DomNodeId callback_node_id;
        HidpiAdjustedBounds bounds;
        OptionGl* gl_context;
        void* image_cache;
        void* system_fonts;
        NodeHierarchyItemVec* node_hierarchy;
        void* words_cache;
        void* shaped_words_cache;
        void* positioned_words_cache;
        void* positioned_rects;
        void* _reserved_ref;
        void* restrict _reserved_mut;
        RenderImageCallbackInfo& operator=(const RenderImageCallbackInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RenderImageCallbackInfo(const RenderImageCallbackInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        RenderImageCallbackInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct LayoutCallbackInfo {
        WindowSize window_size;
        WindowTheme theme;
        void* image_cache;
        OptionGl* gl_context;
        void* system_fonts;
        void* _reserved_ref;
        void* restrict _reserved_mut;
        LayoutCallbackInfo& operator=(const LayoutCallbackInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LayoutCallbackInfo(const LayoutCallbackInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        LayoutCallbackInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class EventFilterTag {
       Hover,
       Not,
       Focus,
       Window,
       Component,
       Application,
    };
    
    struct EventFilterVariant_Hover { EventFilterTag tag; HoverEventFilter payload; };
    struct EventFilterVariant_Not { EventFilterTag tag; NotEventFilter payload; };
    struct EventFilterVariant_Focus { EventFilterTag tag; FocusEventFilter payload; };
    struct EventFilterVariant_Window { EventFilterTag tag; WindowEventFilter payload; };
    struct EventFilterVariant_Component { EventFilterTag tag; ComponentEventFilter payload; };
    struct EventFilterVariant_Application { EventFilterTag tag; ApplicationEventFilter payload; };
    union EventFilter {
        EventFilterVariant_Hover Hover;
        EventFilterVariant_Not Not;
        EventFilterVariant_Focus Focus;
        EventFilterVariant_Window Window;
        EventFilterVariant_Component Component;
        EventFilterVariant_Application Application;
    };
    
    
    struct Menu {
        MenuItemVec items;
        MenuPopupPosition position;
        ContextMenuMouseButton context_mouse_btn;
        Menu& operator=(const Menu&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Menu(const Menu&) = delete; /* disable copy constructor, use explicit .clone() */
        Menu() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct VirtualKeyCodeCombo {
        VirtualKeyCodeVec keys;
        VirtualKeyCodeCombo& operator=(const VirtualKeyCodeCombo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VirtualKeyCodeCombo(const VirtualKeyCodeCombo&) = delete; /* disable copy constructor, use explicit .clone() */
        VirtualKeyCodeCombo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class CssPathPseudoSelectorTag {
       First,
       Last,
       NthChild,
       Hover,
       Active,
       Focus,
    };
    
    struct CssPathPseudoSelectorVariant_First { CssPathPseudoSelectorTag tag; };
    struct CssPathPseudoSelectorVariant_Last { CssPathPseudoSelectorTag tag; };
    struct CssPathPseudoSelectorVariant_NthChild { CssPathPseudoSelectorTag tag; CssNthChildSelector payload; };
    struct CssPathPseudoSelectorVariant_Hover { CssPathPseudoSelectorTag tag; };
    struct CssPathPseudoSelectorVariant_Active { CssPathPseudoSelectorTag tag; };
    struct CssPathPseudoSelectorVariant_Focus { CssPathPseudoSelectorTag tag; };
    union CssPathPseudoSelector {
        CssPathPseudoSelectorVariant_First First;
        CssPathPseudoSelectorVariant_Last Last;
        CssPathPseudoSelectorVariant_NthChild NthChild;
        CssPathPseudoSelectorVariant_Hover Hover;
        CssPathPseudoSelectorVariant_Active Active;
        CssPathPseudoSelectorVariant_Focus Focus;
    };
    
    
    enum class AnimationInterpolationFunctionTag {
       Ease,
       Linear,
       EaseIn,
       EaseOut,
       EaseInOut,
       CubicBezier,
    };
    
    struct AnimationInterpolationFunctionVariant_Ease { AnimationInterpolationFunctionTag tag; };
    struct AnimationInterpolationFunctionVariant_Linear { AnimationInterpolationFunctionTag tag; };
    struct AnimationInterpolationFunctionVariant_EaseIn { AnimationInterpolationFunctionTag tag; };
    struct AnimationInterpolationFunctionVariant_EaseOut { AnimationInterpolationFunctionTag tag; };
    struct AnimationInterpolationFunctionVariant_EaseInOut { AnimationInterpolationFunctionTag tag; };
    struct AnimationInterpolationFunctionVariant_CubicBezier { AnimationInterpolationFunctionTag tag; SvgCubicCurve payload; };
    union AnimationInterpolationFunction {
        AnimationInterpolationFunctionVariant_Ease Ease;
        AnimationInterpolationFunctionVariant_Linear Linear;
        AnimationInterpolationFunctionVariant_EaseIn EaseIn;
        AnimationInterpolationFunctionVariant_EaseOut EaseOut;
        AnimationInterpolationFunctionVariant_EaseInOut EaseInOut;
        AnimationInterpolationFunctionVariant_CubicBezier CubicBezier;
    };
    
    
    struct InterpolateContext {
        AnimationInterpolationFunction animation_func;
        float parent_rect_width;
        float parent_rect_height;
        float current_rect_width;
        float current_rect_height;
        InterpolateContext& operator=(const InterpolateContext&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InterpolateContext() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleFilterTag {
       Blend,
       Flood,
       Blur,
       Opacity,
       ColorMatrix,
       DropShadow,
       ComponentTransfer,
       Offset,
       Composite,
    };
    
    struct StyleFilterVariant_Blend { StyleFilterTag tag; StyleMixBlendMode payload; };
    struct StyleFilterVariant_Flood { StyleFilterTag tag; ColorU payload; };
    struct StyleFilterVariant_Blur { StyleFilterTag tag; StyleBlur payload; };
    struct StyleFilterVariant_Opacity { StyleFilterTag tag; PercentageValue payload; };
    struct StyleFilterVariant_ColorMatrix { StyleFilterTag tag; StyleColorMatrix payload; };
    struct StyleFilterVariant_DropShadow { StyleFilterTag tag; StyleBoxShadow payload; };
    struct StyleFilterVariant_ComponentTransfer { StyleFilterTag tag; };
    struct StyleFilterVariant_Offset { StyleFilterTag tag; StyleFilterOffset payload; };
    struct StyleFilterVariant_Composite { StyleFilterTag tag; StyleCompositeFilter payload; };
    union StyleFilter {
        StyleFilterVariant_Blend Blend;
        StyleFilterVariant_Flood Flood;
        StyleFilterVariant_Blur Blur;
        StyleFilterVariant_Opacity Opacity;
        StyleFilterVariant_ColorMatrix ColorMatrix;
        StyleFilterVariant_DropShadow DropShadow;
        StyleFilterVariant_ComponentTransfer ComponentTransfer;
        StyleFilterVariant_Offset Offset;
        StyleFilterVariant_Composite Composite;
    };
    
    
    struct LinearGradient {
        Direction direction;
        ExtendMode extend_mode;
        NormalizedLinearColorStopVec stops;
        LinearGradient& operator=(const LinearGradient&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LinearGradient(const LinearGradient&) = delete; /* disable copy constructor, use explicit .clone() */
        LinearGradient() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RadialGradient {
        Shape shape;
        RadialGradientSize size;
        StyleBackgroundPosition position;
        ExtendMode extend_mode;
        NormalizedLinearColorStopVec stops;
        RadialGradient& operator=(const RadialGradient&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RadialGradient(const RadialGradient&) = delete; /* disable copy constructor, use explicit .clone() */
        RadialGradient() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ConicGradient {
        ExtendMode extend_mode;
        StyleBackgroundPosition center;
        AngleValue angle;
        NormalizedRadialColorStopVec stops;
        ConicGradient& operator=(const ConicGradient&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ConicGradient(const ConicGradient&) = delete; /* disable copy constructor, use explicit .clone() */
        ConicGradient() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleTransformTag {
       Matrix,
       Matrix3D,
       Translate,
       Translate3D,
       TranslateX,
       TranslateY,
       TranslateZ,
       Rotate,
       Rotate3D,
       RotateX,
       RotateY,
       RotateZ,
       Scale,
       Scale3D,
       ScaleX,
       ScaleY,
       ScaleZ,
       Skew,
       SkewX,
       SkewY,
       Perspective,
    };
    
    struct StyleTransformVariant_Matrix { StyleTransformTag tag; StyleTransformMatrix2D payload; };
    struct StyleTransformVariant_Matrix3D { StyleTransformTag tag; StyleTransformMatrix3D payload; };
    struct StyleTransformVariant_Translate { StyleTransformTag tag; StyleTransformTranslate2D payload; };
    struct StyleTransformVariant_Translate3D { StyleTransformTag tag; StyleTransformTranslate3D payload; };
    struct StyleTransformVariant_TranslateX { StyleTransformTag tag; PixelValue payload; };
    struct StyleTransformVariant_TranslateY { StyleTransformTag tag; PixelValue payload; };
    struct StyleTransformVariant_TranslateZ { StyleTransformTag tag; PixelValue payload; };
    struct StyleTransformVariant_Rotate { StyleTransformTag tag; AngleValue payload; };
    struct StyleTransformVariant_Rotate3D { StyleTransformTag tag; StyleTransformRotate3D payload; };
    struct StyleTransformVariant_RotateX { StyleTransformTag tag; AngleValue payload; };
    struct StyleTransformVariant_RotateY { StyleTransformTag tag; AngleValue payload; };
    struct StyleTransformVariant_RotateZ { StyleTransformTag tag; AngleValue payload; };
    struct StyleTransformVariant_Scale { StyleTransformTag tag; StyleTransformScale2D payload; };
    struct StyleTransformVariant_Scale3D { StyleTransformTag tag; StyleTransformScale3D payload; };
    struct StyleTransformVariant_ScaleX { StyleTransformTag tag; PercentageValue payload; };
    struct StyleTransformVariant_ScaleY { StyleTransformTag tag; PercentageValue payload; };
    struct StyleTransformVariant_ScaleZ { StyleTransformTag tag; PercentageValue payload; };
    struct StyleTransformVariant_Skew { StyleTransformTag tag; StyleTransformSkew2D payload; };
    struct StyleTransformVariant_SkewX { StyleTransformTag tag; PercentageValue payload; };
    struct StyleTransformVariant_SkewY { StyleTransformTag tag; PercentageValue payload; };
    struct StyleTransformVariant_Perspective { StyleTransformTag tag; PixelValue payload; };
    union StyleTransform {
        StyleTransformVariant_Matrix Matrix;
        StyleTransformVariant_Matrix3D Matrix3D;
        StyleTransformVariant_Translate Translate;
        StyleTransformVariant_Translate3D Translate3D;
        StyleTransformVariant_TranslateX TranslateX;
        StyleTransformVariant_TranslateY TranslateY;
        StyleTransformVariant_TranslateZ TranslateZ;
        StyleTransformVariant_Rotate Rotate;
        StyleTransformVariant_Rotate3D Rotate3D;
        StyleTransformVariant_RotateX RotateX;
        StyleTransformVariant_RotateY RotateY;
        StyleTransformVariant_RotateZ RotateZ;
        StyleTransformVariant_Scale Scale;
        StyleTransformVariant_Scale3D Scale3D;
        StyleTransformVariant_ScaleX ScaleX;
        StyleTransformVariant_ScaleY ScaleY;
        StyleTransformVariant_ScaleZ ScaleZ;
        StyleTransformVariant_Skew Skew;
        StyleTransformVariant_SkewX SkewX;
        StyleTransformVariant_SkewY SkewY;
        StyleTransformVariant_Perspective Perspective;
    };
    
    
    enum class StyleBackgroundPositionVecValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBackgroundPositionVecValueVariant_Auto { StyleBackgroundPositionVecValueTag tag; };
    struct StyleBackgroundPositionVecValueVariant_None { StyleBackgroundPositionVecValueTag tag; };
    struct StyleBackgroundPositionVecValueVariant_Inherit { StyleBackgroundPositionVecValueTag tag; };
    struct StyleBackgroundPositionVecValueVariant_Initial { StyleBackgroundPositionVecValueTag tag; };
    struct StyleBackgroundPositionVecValueVariant_Exact { StyleBackgroundPositionVecValueTag tag; StyleBackgroundPositionVec payload; };
    union StyleBackgroundPositionVecValue {
        StyleBackgroundPositionVecValueVariant_Auto Auto;
        StyleBackgroundPositionVecValueVariant_None None;
        StyleBackgroundPositionVecValueVariant_Inherit Inherit;
        StyleBackgroundPositionVecValueVariant_Initial Initial;
        StyleBackgroundPositionVecValueVariant_Exact Exact;
    };
    
    
    enum class StyleBackgroundRepeatVecValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBackgroundRepeatVecValueVariant_Auto { StyleBackgroundRepeatVecValueTag tag; };
    struct StyleBackgroundRepeatVecValueVariant_None { StyleBackgroundRepeatVecValueTag tag; };
    struct StyleBackgroundRepeatVecValueVariant_Inherit { StyleBackgroundRepeatVecValueTag tag; };
    struct StyleBackgroundRepeatVecValueVariant_Initial { StyleBackgroundRepeatVecValueTag tag; };
    struct StyleBackgroundRepeatVecValueVariant_Exact { StyleBackgroundRepeatVecValueTag tag; StyleBackgroundRepeatVec payload; };
    union StyleBackgroundRepeatVecValue {
        StyleBackgroundRepeatVecValueVariant_Auto Auto;
        StyleBackgroundRepeatVecValueVariant_None None;
        StyleBackgroundRepeatVecValueVariant_Inherit Inherit;
        StyleBackgroundRepeatVecValueVariant_Initial Initial;
        StyleBackgroundRepeatVecValueVariant_Exact Exact;
    };
    
    
    enum class StyleBackgroundSizeVecValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBackgroundSizeVecValueVariant_Auto { StyleBackgroundSizeVecValueTag tag; };
    struct StyleBackgroundSizeVecValueVariant_None { StyleBackgroundSizeVecValueTag tag; };
    struct StyleBackgroundSizeVecValueVariant_Inherit { StyleBackgroundSizeVecValueTag tag; };
    struct StyleBackgroundSizeVecValueVariant_Initial { StyleBackgroundSizeVecValueTag tag; };
    struct StyleBackgroundSizeVecValueVariant_Exact { StyleBackgroundSizeVecValueTag tag; StyleBackgroundSizeVec payload; };
    union StyleBackgroundSizeVecValue {
        StyleBackgroundSizeVecValueVariant_Auto Auto;
        StyleBackgroundSizeVecValueVariant_None None;
        StyleBackgroundSizeVecValueVariant_Inherit Inherit;
        StyleBackgroundSizeVecValueVariant_Initial Initial;
        StyleBackgroundSizeVecValueVariant_Exact Exact;
    };
    
    
    struct CheckBoxStateWrapper {
        CheckBoxState inner;
        OptionCheckBoxOnToggle on_toggle;
        CheckBoxStateWrapper& operator=(const CheckBoxStateWrapper&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CheckBoxStateWrapper(const CheckBoxStateWrapper&) = delete; /* disable copy constructor, use explicit .clone() */
        CheckBoxStateWrapper() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NumberInputStateWrapper {
        NumberInputState inner;
        OptionNumberInputOnValueChange on_value_change;
        OptionNumberInputOnFocusLost on_focus_lost;
        NumberInputStateWrapper& operator=(const NumberInputStateWrapper&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NumberInputStateWrapper(const NumberInputStateWrapper&) = delete; /* disable copy constructor, use explicit .clone() */
        NumberInputStateWrapper() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraphCallbacks {
        OptionNodeGraphOnNodeAdded on_node_added;
        OptionNodeGraphOnNodeRemoved on_node_removed;
        OptionNodeGraphOnNodeDragged on_node_dragged;
        OptionNodeGraphOnNodeGraphDragged on_node_graph_dragged;
        OptionNodeGraphOnNodeConnected on_node_connected;
        OptionNodeGraphOnNodeInputDisconnected on_node_input_disconnected;
        OptionNodeGraphOnNodeOutputDisconnected on_node_output_disconnected;
        OptionNodeGraphOnNodeFieldEdited on_node_field_edited;
        NodeGraphCallbacks& operator=(const NodeGraphCallbacks&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraphCallbacks(const NodeGraphCallbacks&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraphCallbacks() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputConnection {
        size_t input_index;
        OutputNodeAndIndexVec connects_to;
        InputConnection& operator=(const InputConnection&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputConnection(const InputConnection&) = delete; /* disable copy constructor, use explicit .clone() */
        InputConnection() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct OutputConnection {
        size_t output_index;
        InputNodeAndIndexVec connects_to;
        OutputConnection& operator=(const OutputConnection&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        OutputConnection(const OutputConnection&) = delete; /* disable copy constructor, use explicit .clone() */
        OutputConnection() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewRow {
        DomVec cells;
        OptionPixelValueNoPercent height;
        ListViewRow& operator=(const ListViewRow&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewRow(const ListViewRow&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewRow() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyledNode {
        StyledNodeState state;
        OptionTagId tag_id;
        StyledNode& operator=(const StyledNode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyledNode(const StyledNode&) = delete; /* disable copy constructor, use explicit .clone() */
        StyledNode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TagIdToNodeIdMapping {
        TagId tag_id;
        NodeId node_id;
        OptionTabIndex tab_index;
        NodeIdVec parents;
        TagIdToNodeIdMapping& operator=(const TagIdToNodeIdMapping&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TagIdToNodeIdMapping(const TagIdToNodeIdMapping&) = delete; /* disable copy constructor, use explicit .clone() */
        TagIdToNodeIdMapping() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Texture {
        uint32_t texture_id;
        TextureFlags flags;
        PhysicalSizeU32 size;
        ColorU background_color;
        Gl gl_context;
        RawImageFormat format;
        void* refcount;
        bool  run_destructor;
        Texture& operator=(const Texture&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Texture(const Texture&) = delete; /* disable copy constructor, use explicit .clone() */
        Texture() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GetProgramBinaryReturn {
        U8Vec _0;
        uint32_t _1;
        GetProgramBinaryReturn& operator=(const GetProgramBinaryReturn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GetProgramBinaryReturn(const GetProgramBinaryReturn&) = delete; /* disable copy constructor, use explicit .clone() */
        GetProgramBinaryReturn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class RawImageDataTag {
       U8,
       U16,
       F32,
    };
    
    struct RawImageDataVariant_U8 { RawImageDataTag tag; U8Vec payload; };
    struct RawImageDataVariant_U16 { RawImageDataTag tag; U16Vec payload; };
    struct RawImageDataVariant_F32 { RawImageDataTag tag; F32Vec payload; };
    union RawImageData {
        RawImageDataVariant_U8 U8;
        RawImageDataVariant_U16 U16;
        RawImageDataVariant_F32 F32;
    };
    
    
    struct FontSource {
        U8Vec data;
        uint32_t font_index;
        bool  parse_glyph_outlines;
        FontSource& operator=(const FontSource&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FontSource(const FontSource&) = delete; /* disable copy constructor, use explicit .clone() */
        FontSource() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class SvgPathElementTag {
       Line,
       QuadraticCurve,
       CubicCurve,
    };
    
    struct SvgPathElementVariant_Line { SvgPathElementTag tag; SvgLine payload; };
    struct SvgPathElementVariant_QuadraticCurve { SvgPathElementTag tag; SvgQuadraticCurve payload; };
    struct SvgPathElementVariant_CubicCurve { SvgPathElementTag tag; SvgCubicCurve payload; };
    union SvgPathElement {
        SvgPathElementVariant_Line Line;
        SvgPathElementVariant_QuadraticCurve QuadraticCurve;
        SvgPathElementVariant_CubicCurve CubicCurve;
    };
    
    
    struct TessellatedSvgNode {
        SvgVertexVec vertices;
        U32Vec indices;
        TessellatedSvgNode& operator=(const TessellatedSvgNode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TessellatedSvgNode(const TessellatedSvgNode&) = delete; /* disable copy constructor, use explicit .clone() */
        TessellatedSvgNode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TessellatedSvgNodeVecRef {
        TessellatedSvgNode* ptr;
        size_t len;
        TessellatedSvgNodeVecRef& operator=(const TessellatedSvgNodeVecRef&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TessellatedSvgNodeVecRef(const TessellatedSvgNodeVecRef&) = delete; /* disable copy constructor, use explicit .clone() */
        TessellatedSvgNodeVecRef() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgRenderOptions {
        OptionLayoutSize target_size;
        OptionColorU background_color;
        SvgFitTo fit;
        SvgRenderTransform transform;
        SvgRenderOptions& operator=(const SvgRenderOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgRenderOptions(const SvgRenderOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgRenderOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgStrokeStyle {
        SvgLineCap start_cap;
        SvgLineCap end_cap;
        SvgLineJoin line_join;
        OptionSvgDashPattern dash_pattern;
        float line_width;
        float miter_limit;
        float tolerance;
        bool  apply_line_width;
        SvgTransform transform;
        bool  anti_alias;
        bool  high_quality_aa;
        SvgStrokeStyle& operator=(const SvgStrokeStyle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgStrokeStyle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Xml {
        XmlNodeVec root;
        Xml& operator=(const Xml&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Xml(const Xml&) = delete; /* disable copy constructor, use explicit .clone() */
        Xml() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class InstantTag {
       System,
       Tick,
    };
    
    struct InstantVariant_System { InstantTag tag; InstantPtr payload; };
    struct InstantVariant_Tick { InstantTag tag; SystemTick payload; };
    union Instant {
        InstantVariant_System System;
        InstantVariant_Tick Tick;
    };
    
    
    enum class ThreadReceiveMsgTag {
       WriteBack,
       Update,
    };
    
    struct ThreadReceiveMsgVariant_WriteBack { ThreadReceiveMsgTag tag; ThreadWriteBackMsg payload; };
    struct ThreadReceiveMsgVariant_Update { ThreadReceiveMsgTag tag; Update payload; };
    union ThreadReceiveMsg {
        ThreadReceiveMsgVariant_WriteBack WriteBack;
        ThreadReceiveMsgVariant_Update Update;
    };
    
    
    struct String {
        U8Vec vec;
        String& operator=(const String&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        String(const String&) = delete; /* disable copy constructor, use explicit .clone() */
        String() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewRowVec {
        ListViewRow* ptr;
        size_t len;
        size_t cap;
        ListViewRowVecDestructor destructor;
        ListViewRowVec& operator=(const ListViewRowVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewRowVec(const ListViewRowVec&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewRowVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleFilterVec {
        StyleFilter* ptr;
        size_t len;
        size_t cap;
        StyleFilterVecDestructor destructor;
        StyleFilterVec& operator=(const StyleFilterVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleFilterVec(const StyleFilterVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyleFilterVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputConnectionVec {
        InputConnection* ptr;
        size_t len;
        size_t cap;
        InputConnectionVecDestructor destructor;
        InputConnectionVec& operator=(const InputConnectionVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputConnectionVec(const InputConnectionVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InputConnectionVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct OutputConnectionVec {
        OutputConnection* ptr;
        size_t len;
        size_t cap;
        OutputConnectionVecDestructor destructor;
        OutputConnectionVec& operator=(const OutputConnectionVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        OutputConnectionVec(const OutputConnectionVec&) = delete; /* disable copy constructor, use explicit .clone() */
        OutputConnectionVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TessellatedSvgNodeVec {
        TessellatedSvgNode* ptr;
        size_t len;
        size_t cap;
        TessellatedSvgNodeVecDestructor destructor;
        TessellatedSvgNodeVec& operator=(const TessellatedSvgNodeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TessellatedSvgNodeVec(const TessellatedSvgNodeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        TessellatedSvgNodeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleTransformVec {
        StyleTransform* ptr;
        size_t len;
        size_t cap;
        StyleTransformVecDestructor destructor;
        StyleTransformVec& operator=(const StyleTransformVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleTransformVec(const StyleTransformVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyleTransformVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgPathElementVec {
        SvgPathElement* ptr;
        size_t len;
        size_t cap;
        SvgPathElementVecDestructor destructor;
        SvgPathElementVec& operator=(const SvgPathElementVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgPathElementVec(const SvgPathElementVec&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgPathElementVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StringVec {
        String* ptr;
        size_t len;
        size_t cap;
        StringVecDestructor destructor;
        StringVec& operator=(const StringVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StringVec(const StringVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StringVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyledNodeVec {
        StyledNode* ptr;
        size_t len;
        size_t cap;
        StyledNodeVecDestructor destructor;
        StyledNodeVec& operator=(const StyledNodeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyledNodeVec(const StyledNodeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyledNodeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TagIdToNodeIdMappingVec {
        TagIdToNodeIdMapping* ptr;
        size_t len;
        size_t cap;
        TagIdToNodeIdMappingVecDestructor destructor;
        TagIdToNodeIdMappingVec& operator=(const TagIdToNodeIdMappingVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TagIdToNodeIdMappingVec(const TagIdToNodeIdMappingVec&) = delete; /* disable copy constructor, use explicit .clone() */
        TagIdToNodeIdMappingVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class OptionMenuTag {
       None,
       Some,
    };
    
    struct OptionMenuVariant_None { OptionMenuTag tag; };
    struct OptionMenuVariant_Some { OptionMenuTag tag; Menu payload; };
    union OptionMenu {
        OptionMenuVariant_None None;
        OptionMenuVariant_Some Some;
    };
    
    
    enum class OptionResolvedTextLayoutOptionsTag {
       None,
       Some,
    };
    
    struct OptionResolvedTextLayoutOptionsVariant_None { OptionResolvedTextLayoutOptionsTag tag; };
    struct OptionResolvedTextLayoutOptionsVariant_Some { OptionResolvedTextLayoutOptionsTag tag; ResolvedTextLayoutOptions payload; };
    union OptionResolvedTextLayoutOptions {
        OptionResolvedTextLayoutOptionsVariant_None None;
        OptionResolvedTextLayoutOptionsVariant_Some Some;
    };
    
    
    enum class OptionVirtualKeyCodeComboTag {
       None,
       Some,
    };
    
    struct OptionVirtualKeyCodeComboVariant_None { OptionVirtualKeyCodeComboTag tag; };
    struct OptionVirtualKeyCodeComboVariant_Some { OptionVirtualKeyCodeComboTag tag; VirtualKeyCodeCombo payload; };
    union OptionVirtualKeyCodeCombo {
        OptionVirtualKeyCodeComboVariant_None None;
        OptionVirtualKeyCodeComboVariant_Some Some;
    };
    
    
    enum class OptionMouseStateTag {
       None,
       Some,
    };
    
    struct OptionMouseStateVariant_None { OptionMouseStateTag tag; };
    struct OptionMouseStateVariant_Some { OptionMouseStateTag tag; MouseState payload; };
    union OptionMouseState {
        OptionMouseStateVariant_None None;
        OptionMouseStateVariant_Some Some;
    };
    
    
    enum class OptionKeyboardStateTag {
       None,
       Some,
    };
    
    struct OptionKeyboardStateVariant_None { OptionKeyboardStateTag tag; };
    struct OptionKeyboardStateVariant_Some { OptionKeyboardStateTag tag; KeyboardState payload; };
    union OptionKeyboardState {
        OptionKeyboardStateVariant_None None;
        OptionKeyboardStateVariant_Some Some;
    };
    
    
    enum class OptionStringVecTag {
       None,
       Some,
    };
    
    struct OptionStringVecVariant_None { OptionStringVecTag tag; };
    struct OptionStringVecVariant_Some { OptionStringVecTag tag; StringVec payload; };
    union OptionStringVec {
        OptionStringVecVariant_None None;
        OptionStringVecVariant_Some Some;
    };
    
    
    enum class OptionThreadReceiveMsgTag {
       None,
       Some,
    };
    
    struct OptionThreadReceiveMsgVariant_None { OptionThreadReceiveMsgTag tag; };
    struct OptionThreadReceiveMsgVariant_Some { OptionThreadReceiveMsgTag tag; ThreadReceiveMsg payload; };
    union OptionThreadReceiveMsg {
        OptionThreadReceiveMsgVariant_None None;
        OptionThreadReceiveMsgVariant_Some Some;
    };
    
    
    enum class OptionTaskBarIconTag {
       None,
       Some,
    };
    
    struct OptionTaskBarIconVariant_None { OptionTaskBarIconTag tag; };
    struct OptionTaskBarIconVariant_Some { OptionTaskBarIconTag tag; TaskBarIcon payload; };
    union OptionTaskBarIcon {
        OptionTaskBarIconVariant_None None;
        OptionTaskBarIconVariant_Some Some;
    };
    
    
    enum class OptionWindowIconTag {
       None,
       Some,
    };
    
    struct OptionWindowIconVariant_None { OptionWindowIconTag tag; };
    struct OptionWindowIconVariant_Some { OptionWindowIconTag tag; WindowIcon payload; };
    union OptionWindowIcon {
        OptionWindowIconVariant_None None;
        OptionWindowIconVariant_Some Some;
    };
    
    
    enum class OptionStringTag {
       None,
       Some,
    };
    
    struct OptionStringVariant_None { OptionStringTag tag; };
    struct OptionStringVariant_Some { OptionStringTag tag; String payload; };
    union OptionString {
        OptionStringVariant_None None;
        OptionStringVariant_Some Some;
    };
    
    
    enum class OptionTextureTag {
       None,
       Some,
    };
    
    struct OptionTextureVariant_None { OptionTextureTag tag; };
    struct OptionTextureVariant_Some { OptionTextureTag tag; Texture payload; };
    union OptionTexture {
        OptionTextureVariant_None None;
        OptionTextureVariant_Some Some;
    };
    
    
    enum class OptionInstantTag {
       None,
       Some,
    };
    
    struct OptionInstantVariant_None { OptionInstantTag tag; };
    struct OptionInstantVariant_Some { OptionInstantTag tag; Instant payload; };
    union OptionInstant {
        OptionInstantVariant_None None;
        OptionInstantVariant_Some Some;
    };
    
    
    struct DuplicatedNamespaceError {
        String ns;
        SvgParseErrorPosition pos;
        DuplicatedNamespaceError& operator=(const DuplicatedNamespaceError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DuplicatedNamespaceError(const DuplicatedNamespaceError&) = delete; /* disable copy constructor, use explicit .clone() */
        DuplicatedNamespaceError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct UnknownNamespaceError {
        String ns;
        SvgParseErrorPosition pos;
        UnknownNamespaceError& operator=(const UnknownNamespaceError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        UnknownNamespaceError(const UnknownNamespaceError&) = delete; /* disable copy constructor, use explicit .clone() */
        UnknownNamespaceError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct UnexpectedCloseTagError {
        String expected;
        String actual;
        SvgParseErrorPosition pos;
        UnexpectedCloseTagError& operator=(const UnexpectedCloseTagError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        UnexpectedCloseTagError(const UnexpectedCloseTagError&) = delete; /* disable copy constructor, use explicit .clone() */
        UnexpectedCloseTagError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct UnknownEntityReferenceError {
        String entity;
        SvgParseErrorPosition pos;
        UnknownEntityReferenceError& operator=(const UnknownEntityReferenceError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        UnknownEntityReferenceError(const UnknownEntityReferenceError&) = delete; /* disable copy constructor, use explicit .clone() */
        UnknownEntityReferenceError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DuplicatedAttributeError {
        String attribute;
        SvgParseErrorPosition pos;
        DuplicatedAttributeError& operator=(const DuplicatedAttributeError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DuplicatedAttributeError(const DuplicatedAttributeError&) = delete; /* disable copy constructor, use explicit .clone() */
        DuplicatedAttributeError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InvalidStringError {
        String got;
        SvgParseErrorPosition pos;
        InvalidStringError& operator=(const InvalidStringError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InvalidStringError(const InvalidStringError&) = delete; /* disable copy constructor, use explicit .clone() */
        InvalidStringError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WindowsWindowOptions {
        bool  allow_drag_drop;
        bool  no_redirection_bitmap;
        OptionWindowIcon window_icon;
        OptionTaskBarIcon taskbar_icon;
        OptionHwndHandle parent_window;
        WindowsWindowOptions& operator=(const WindowsWindowOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WindowsWindowOptions(const WindowsWindowOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        WindowsWindowOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WaylandTheme {
        uint8_t title_bar_active_background_color[4];
        uint8_t title_bar_active_separator_color[4];
        uint8_t title_bar_active_text_color[4];
        uint8_t title_bar_inactive_background_color[4];
        uint8_t title_bar_inactive_separator_color[4];
        uint8_t title_bar_inactive_text_color[4];
        uint8_t maximize_idle_foreground_inactive_color[4];
        uint8_t minimize_idle_foreground_inactive_color[4];
        uint8_t close_idle_foreground_inactive_color[4];
        uint8_t maximize_hovered_foreground_inactive_color[4];
        uint8_t minimize_hovered_foreground_inactive_color[4];
        uint8_t close_hovered_foreground_inactive_color[4];
        uint8_t maximize_disabled_foreground_inactive_color[4];
        uint8_t minimize_disabled_foreground_inactive_color[4];
        uint8_t close_disabled_foreground_inactive_color[4];
        uint8_t maximize_idle_background_inactive_color[4];
        uint8_t minimize_idle_background_inactive_color[4];
        uint8_t close_idle_background_inactive_color[4];
        uint8_t maximize_hovered_background_inactive_color[4];
        uint8_t minimize_hovered_background_inactive_color[4];
        uint8_t close_hovered_background_inactive_color[4];
        uint8_t maximize_disabled_background_inactive_color[4];
        uint8_t minimize_disabled_background_inactive_color[4];
        uint8_t close_disabled_background_inactive_color[4];
        uint8_t maximize_idle_foreground_active_color[4];
        uint8_t minimize_idle_foreground_active_color[4];
        uint8_t close_idle_foreground_active_color[4];
        uint8_t maximize_hovered_foreground_active_color[4];
        uint8_t minimize_hovered_foreground_active_color[4];
        uint8_t close_hovered_foreground_active_color[4];
        uint8_t maximize_disabled_foreground_active_color[4];
        uint8_t minimize_disabled_foreground_active_color[4];
        uint8_t close_disabled_foreground_active_color[4];
        uint8_t maximize_idle_background_active_color[4];
        uint8_t minimize_idle_background_active_color[4];
        uint8_t close_idle_background_active_color[4];
        uint8_t maximize_hovered_background_active_color[4];
        uint8_t minimize_hovered_background_active_color[4];
        uint8_t close_hovered_background_active_color[4];
        uint8_t maximize_disabled_background_active_color[4];
        uint8_t minimize_disabled_background_active_color[4];
        uint8_t close_disabled_background_active_color[4];
        String title_bar_font;
        float title_bar_font_size;
        WaylandTheme& operator=(const WaylandTheme&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WaylandTheme(const WaylandTheme&) = delete; /* disable copy constructor, use explicit .clone() */
        WaylandTheme() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StringPair {
        String key;
        String value;
        StringPair& operator=(const StringPair&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StringPair(const StringPair&) = delete; /* disable copy constructor, use explicit .clone() */
        StringPair() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Monitor {
        size_t id;
        OptionString name;
        LayoutSize size;
        LayoutPoint position;
        double scale_factor;
        VideoModeVec video_modes;
        bool  is_primary_monitor;
        Monitor& operator=(const Monitor&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Monitor(const Monitor&) = delete; /* disable copy constructor, use explicit .clone() */
        Monitor() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class LayoutCallbackTag {
       Raw,
       Marshaled,
    };
    
    struct LayoutCallbackVariant_Raw { LayoutCallbackTag tag; LayoutCallbackInner payload; };
    struct LayoutCallbackVariant_Marshaled { LayoutCallbackTag tag; MarshaledLayoutCallback payload; };
    union LayoutCallback {
        LayoutCallbackVariant_Raw Raw;
        LayoutCallbackVariant_Marshaled Marshaled;
    };
    
    
    enum class InlineWordTag {
       Tab,
       Return,
       Space,
       Word,
    };
    
    struct InlineWordVariant_Tab { InlineWordTag tag; };
    struct InlineWordVariant_Return { InlineWordTag tag; };
    struct InlineWordVariant_Space { InlineWordTag tag; };
    struct InlineWordVariant_Word { InlineWordTag tag; InlineTextContents payload; };
    union InlineWord {
        InlineWordVariant_Tab Tab;
        InlineWordVariant_Return Return;
        InlineWordVariant_Space Space;
        InlineWordVariant_Word Word;
    };
    
    
    struct CallbackData {
        EventFilter event;
        Callback callback;
        RefAny data;
        CallbackData& operator=(const CallbackData&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CallbackData(const CallbackData&) = delete; /* disable copy constructor, use explicit .clone() */
        CallbackData() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class NodeTypeTag {
       Body,
       Div,
       Br,
       Text,
       Image,
       IFrame,
    };
    
    struct NodeTypeVariant_Body { NodeTypeTag tag; };
    struct NodeTypeVariant_Div { NodeTypeTag tag; };
    struct NodeTypeVariant_Br { NodeTypeTag tag; };
    struct NodeTypeVariant_Text { NodeTypeTag tag; String payload; };
    struct NodeTypeVariant_Image { NodeTypeTag tag; ImageRef payload; };
    struct NodeTypeVariant_IFrame { NodeTypeTag tag; IFrameNode payload; };
    union NodeType {
        NodeTypeVariant_Body Body;
        NodeTypeVariant_Div Div;
        NodeTypeVariant_Br Br;
        NodeTypeVariant_Text Text;
        NodeTypeVariant_Image Image;
        NodeTypeVariant_IFrame IFrame;
    };
    
    
    struct AccessibilityInfo {
        OptionString name;
        OptionString value;
        AccessibilityRole role;
        AccessibilityStateVec states;
        OptionVirtualKeyCodeCombo accelerator;
        OptionString default_action;
        AccessibilityInfo& operator=(const AccessibilityInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        AccessibilityInfo(const AccessibilityInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        AccessibilityInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class IdOrClassTag {
       Id,
       Class,
    };
    
    struct IdOrClassVariant_Id { IdOrClassTag tag; String payload; };
    struct IdOrClassVariant_Class { IdOrClassTag tag; String payload; };
    union IdOrClass {
        IdOrClassVariant_Id Id;
        IdOrClassVariant_Class Class;
    };
    
    
    struct StringMenuItem {
        String label;
        OptionVirtualKeyCodeCombo accelerator;
        OptionMenuCallback callback;
        MenuItemState state;
        OptionMenuItemIcon icon;
        MenuItemVec children;
        StringMenuItem& operator=(const StringMenuItem&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StringMenuItem(const StringMenuItem&) = delete; /* disable copy constructor, use explicit .clone() */
        StringMenuItem() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class CssPathSelectorTag {
       Global,
       Type,
       Class,
       Id,
       PseudoSelector,
       DirectChildren,
       Children,
    };
    
    struct CssPathSelectorVariant_Global { CssPathSelectorTag tag; };
    struct CssPathSelectorVariant_Type { CssPathSelectorTag tag; NodeTypeKey payload; };
    struct CssPathSelectorVariant_Class { CssPathSelectorTag tag; String payload; };
    struct CssPathSelectorVariant_Id { CssPathSelectorTag tag; String payload; };
    struct CssPathSelectorVariant_PseudoSelector { CssPathSelectorTag tag; CssPathPseudoSelector payload; };
    struct CssPathSelectorVariant_DirectChildren { CssPathSelectorTag tag; };
    struct CssPathSelectorVariant_Children { CssPathSelectorTag tag; };
    union CssPathSelector {
        CssPathSelectorVariant_Global Global;
        CssPathSelectorVariant_Type Type;
        CssPathSelectorVariant_Class Class;
        CssPathSelectorVariant_Id Id;
        CssPathSelectorVariant_PseudoSelector PseudoSelector;
        CssPathSelectorVariant_DirectChildren DirectChildren;
        CssPathSelectorVariant_Children Children;
    };
    
    
    enum class StyleBackgroundContentTag {
       LinearGradient,
       RadialGradient,
       ConicGradient,
       Image,
       Color,
    };
    
    struct StyleBackgroundContentVariant_LinearGradient { StyleBackgroundContentTag tag; LinearGradient payload; };
    struct StyleBackgroundContentVariant_RadialGradient { StyleBackgroundContentTag tag; RadialGradient payload; };
    struct StyleBackgroundContentVariant_ConicGradient { StyleBackgroundContentTag tag; ConicGradient payload; };
    struct StyleBackgroundContentVariant_Image { StyleBackgroundContentTag tag; String payload; };
    struct StyleBackgroundContentVariant_Color { StyleBackgroundContentTag tag; ColorU payload; };
    union StyleBackgroundContent {
        StyleBackgroundContentVariant_LinearGradient LinearGradient;
        StyleBackgroundContentVariant_RadialGradient RadialGradient;
        StyleBackgroundContentVariant_ConicGradient ConicGradient;
        StyleBackgroundContentVariant_Image Image;
        StyleBackgroundContentVariant_Color Color;
    };
    
    
    struct ScrollbarInfo {
        LayoutWidth width;
        LayoutPaddingLeft padding_left;
        LayoutPaddingRight padding_right;
        StyleBackgroundContent track;
        StyleBackgroundContent thumb;
        StyleBackgroundContent button;
        StyleBackgroundContent corner;
        StyleBackgroundContent resizer;
        ScrollbarInfo& operator=(const ScrollbarInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ScrollbarInfo(const ScrollbarInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        ScrollbarInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ScrollbarStyle {
        ScrollbarInfo horizontal;
        ScrollbarInfo vertical;
        ScrollbarStyle& operator=(const ScrollbarStyle&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ScrollbarStyle(const ScrollbarStyle&) = delete; /* disable copy constructor, use explicit .clone() */
        ScrollbarStyle() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleFontFamilyTag {
       System,
       File,
       Ref,
    };
    
    struct StyleFontFamilyVariant_System { StyleFontFamilyTag tag; String payload; };
    struct StyleFontFamilyVariant_File { StyleFontFamilyTag tag; String payload; };
    struct StyleFontFamilyVariant_Ref { StyleFontFamilyTag tag; FontRef payload; };
    union StyleFontFamily {
        StyleFontFamilyVariant_System System;
        StyleFontFamilyVariant_File File;
        StyleFontFamilyVariant_Ref Ref;
    };
    
    
    enum class ScrollbarStyleValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct ScrollbarStyleValueVariant_Auto { ScrollbarStyleValueTag tag; };
    struct ScrollbarStyleValueVariant_None { ScrollbarStyleValueTag tag; };
    struct ScrollbarStyleValueVariant_Inherit { ScrollbarStyleValueTag tag; };
    struct ScrollbarStyleValueVariant_Initial { ScrollbarStyleValueTag tag; };
    struct ScrollbarStyleValueVariant_Exact { ScrollbarStyleValueTag tag; ScrollbarStyle payload; };
    union ScrollbarStyleValue {
        ScrollbarStyleValueVariant_Auto Auto;
        ScrollbarStyleValueVariant_None None;
        ScrollbarStyleValueVariant_Inherit Inherit;
        ScrollbarStyleValueVariant_Initial Initial;
        ScrollbarStyleValueVariant_Exact Exact;
    };
    
    
    enum class StyleTransformVecValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleTransformVecValueVariant_Auto { StyleTransformVecValueTag tag; };
    struct StyleTransformVecValueVariant_None { StyleTransformVecValueTag tag; };
    struct StyleTransformVecValueVariant_Inherit { StyleTransformVecValueTag tag; };
    struct StyleTransformVecValueVariant_Initial { StyleTransformVecValueTag tag; };
    struct StyleTransformVecValueVariant_Exact { StyleTransformVecValueTag tag; StyleTransformVec payload; };
    union StyleTransformVecValue {
        StyleTransformVecValueVariant_Auto Auto;
        StyleTransformVecValueVariant_None None;
        StyleTransformVecValueVariant_Inherit Inherit;
        StyleTransformVecValueVariant_Initial Initial;
        StyleTransformVecValueVariant_Exact Exact;
    };
    
    
    enum class StyleFilterVecValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleFilterVecValueVariant_Auto { StyleFilterVecValueTag tag; };
    struct StyleFilterVecValueVariant_None { StyleFilterVecValueTag tag; };
    struct StyleFilterVecValueVariant_Inherit { StyleFilterVecValueTag tag; };
    struct StyleFilterVecValueVariant_Initial { StyleFilterVecValueTag tag; };
    struct StyleFilterVecValueVariant_Exact { StyleFilterVecValueTag tag; StyleFilterVec payload; };
    union StyleFilterVecValue {
        StyleFilterVecValueVariant_Auto Auto;
        StyleFilterVecValueVariant_None None;
        StyleFilterVecValueVariant_Inherit Inherit;
        StyleFilterVecValueVariant_Initial Initial;
        StyleFilterVecValueVariant_Exact Exact;
    };
    
    
    struct FileInputState {
        OptionString path;
        FileInputState& operator=(const FileInputState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FileInputState(const FileInputState&) = delete; /* disable copy constructor, use explicit .clone() */
        FileInputState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ColorInputStateWrapper {
        ColorInputState inner;
        String title;
        OptionColorInputOnValueChange on_value_change;
        ColorInputStateWrapper& operator=(const ColorInputStateWrapper&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ColorInputStateWrapper(const ColorInputStateWrapper&) = delete; /* disable copy constructor, use explicit .clone() */
        ColorInputStateWrapper() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputState {
        U32Vec text;
        OptionString placeholder;
        size_t max_len;
        OptionTextInputSelection selection;
        size_t cursor_pos;
        TextInputState& operator=(const TextInputState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputState(const TextInputState&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TabHeader {
        StringVec tabs;
        size_t active_tab;
        OptionTabOnClick on_click;
        TabHeader& operator=(const TabHeader&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TabHeader(const TabHeader&) = delete; /* disable copy constructor, use explicit .clone() */
        TabHeader() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class NodeTypeFieldValueTag {
       TextInput,
       NumberInput,
       CheckBox,
       ColorInput,
       FileInput,
    };
    
    struct NodeTypeFieldValueVariant_TextInput { NodeTypeFieldValueTag tag; String payload; };
    struct NodeTypeFieldValueVariant_NumberInput { NodeTypeFieldValueTag tag; float payload; };
    struct NodeTypeFieldValueVariant_CheckBox { NodeTypeFieldValueTag tag; bool payload; };
    struct NodeTypeFieldValueVariant_ColorInput { NodeTypeFieldValueTag tag; ColorU payload; };
    struct NodeTypeFieldValueVariant_FileInput { NodeTypeFieldValueTag tag; OptionString payload; };
    union NodeTypeFieldValue {
        NodeTypeFieldValueVariant_TextInput TextInput;
        NodeTypeFieldValueVariant_NumberInput NumberInput;
        NodeTypeFieldValueVariant_CheckBox CheckBox;
        NodeTypeFieldValueVariant_ColorInput ColorInput;
        NodeTypeFieldValueVariant_FileInput FileInput;
    };
    
    
    struct NodeTypeInfo {
        bool  is_root;
        String name;
        InputOutputTypeIdVec inputs;
        InputOutputTypeIdVec outputs;
        NodeTypeInfo& operator=(const NodeTypeInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeTypeInfo(const NodeTypeInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeTypeInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputOutputInfo {
        String data_type;
        ColorU color;
        InputOutputInfo& operator=(const InputOutputInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputOutputInfo(const InputOutputInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        InputOutputInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListView {
        StringVec columns;
        ListViewRowVec rows;
        OptionUsize sorted_by;
        PixelValueNoPercent scroll_offset;
        OptionPixelValueNoPercent content_height;
        OptionMenu column_context_menu;
        OptionListViewOnLazyLoadScroll on_lazy_load_scroll;
        OptionListViewOnColumnClick on_column_click;
        OptionListViewOnRowClick on_row_click;
        ListView& operator=(const ListView&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListView(const ListView&) = delete; /* disable copy constructor, use explicit .clone() */
        ListView() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ListViewState {
        StringVec columns;
        OptionUsize sorted_by;
        size_t current_row_count;
        PixelValueNoPercent scroll_offset;
        LogicalPosition current_scroll_position;
        LogicalSize current_content_height;
        ListViewState& operator=(const ListViewState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ListViewState(const ListViewState&) = delete; /* disable copy constructor, use explicit .clone() */
        ListViewState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TreeView {
        String root;
        TreeView& operator=(const TreeView&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TreeView(const TreeView&) = delete; /* disable copy constructor, use explicit .clone() */
        TreeView() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DropDown {
        StringVec choices;
        size_t selected;
        OptionDropDownOnChoiceChange on_choice_change;
        DropDown& operator=(const DropDown&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DropDown(const DropDown&) = delete; /* disable copy constructor, use explicit .clone() */
        DropDown() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct VertexAttribute {
        String name;
        OptionUsize layout_location;
        VertexAttributeType attribute_type;
        size_t item_count;
        VertexAttribute& operator=(const VertexAttribute&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VertexAttribute(const VertexAttribute&) = delete; /* disable copy constructor, use explicit .clone() */
        VertexAttribute() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DebugMessage {
        String message;
        uint32_t source;
        uint32_t ty;
        uint32_t id;
        uint32_t severity;
        DebugMessage& operator=(const DebugMessage&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DebugMessage(const DebugMessage&) = delete; /* disable copy constructor, use explicit .clone() */
        DebugMessage() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GetActiveAttribReturn {
        int32_t _0;
        uint32_t _1;
        String _2;
        GetActiveAttribReturn& operator=(const GetActiveAttribReturn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GetActiveAttribReturn(const GetActiveAttribReturn&) = delete; /* disable copy constructor, use explicit .clone() */
        GetActiveAttribReturn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct GetActiveUniformReturn {
        int32_t _0;
        uint32_t _1;
        String _2;
        GetActiveUniformReturn& operator=(const GetActiveUniformReturn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        GetActiveUniformReturn(const GetActiveUniformReturn&) = delete; /* disable copy constructor, use explicit .clone() */
        GetActiveUniformReturn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct RawImage {
        RawImageData pixels;
        size_t width;
        size_t height;
        bool  alpha_premultiplied;
        RawImageFormat data_format;
        RawImage& operator=(const RawImage&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        RawImage(const RawImage&) = delete; /* disable copy constructor, use explicit .clone() */
        RawImage() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgPath {
        SvgPathElementVec items;
        SvgPath& operator=(const SvgPath&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgPath(const SvgPath&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgPath() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgParseOptions {
        OptionString relative_image_path;
        float dpi;
        String default_font_family;
        float font_size;
        StringVec languages;
        ShapeRendering shape_rendering;
        TextRendering text_rendering;
        ImageRendering image_rendering;
        bool  keep_named_groups;
        FontDatabase fontdb;
        SvgParseOptions& operator=(const SvgParseOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgParseOptions(const SvgParseOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgParseOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class SvgStyleTag {
       Fill,
       Stroke,
    };
    
    struct SvgStyleVariant_Fill { SvgStyleTag tag; SvgFillStyle payload; };
    struct SvgStyleVariant_Stroke { SvgStyleTag tag; SvgStrokeStyle payload; };
    union SvgStyle {
        SvgStyleVariant_Fill Fill;
        SvgStyleVariant_Stroke Stroke;
    };
    
    
    struct File {
        void* ptr;
        String path;
        bool  run_destructor;
        File& operator=(const File&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        File(const File&) = delete; /* disable copy constructor, use explicit .clone() */
        File() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct FileTypeList {
        StringVec document_types;
        String document_descriptor;
        FileTypeList& operator=(const FileTypeList&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FileTypeList(const FileTypeList&) = delete; /* disable copy constructor, use explicit .clone() */
        FileTypeList() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Timer {
        RefAny data;
        OptionDomNodeId node_id;
        Instant created;
        OptionInstant last_run;
        size_t run_count;
        OptionDuration delay;
        OptionDuration interval;
        OptionDuration timeout;
        TimerCallback callback;
        Timer& operator=(const Timer&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Timer(const Timer&) = delete; /* disable copy constructor, use explicit .clone() */
        Timer() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class FmtValueTag {
       Bool,
       Uchar,
       Schar,
       Ushort,
       Sshort,
       Uint,
       Sint,
       Ulong,
       Slong,
       Isize,
       Usize,
       Float,
       Double,
       Str,
       StrVec,
    };
    
    struct FmtValueVariant_Bool { FmtValueTag tag; bool payload; };
    struct FmtValueVariant_Uchar { FmtValueTag tag; uint8_t payload; };
    struct FmtValueVariant_Schar { FmtValueTag tag; int8_t payload; };
    struct FmtValueVariant_Ushort { FmtValueTag tag; uint16_t payload; };
    struct FmtValueVariant_Sshort { FmtValueTag tag; int16_t payload; };
    struct FmtValueVariant_Uint { FmtValueTag tag; uint32_t payload; };
    struct FmtValueVariant_Sint { FmtValueTag tag; int32_t payload; };
    struct FmtValueVariant_Ulong { FmtValueTag tag; uint64_t payload; };
    struct FmtValueVariant_Slong { FmtValueTag tag; int64_t payload; };
    struct FmtValueVariant_Isize { FmtValueTag tag; ssize_t payload; };
    struct FmtValueVariant_Usize { FmtValueTag tag; size_t payload; };
    struct FmtValueVariant_Float { FmtValueTag tag; float payload; };
    struct FmtValueVariant_Double { FmtValueTag tag; double payload; };
    struct FmtValueVariant_Str { FmtValueTag tag; String payload; };
    struct FmtValueVariant_StrVec { FmtValueTag tag; StringVec payload; };
    union FmtValue {
        FmtValueVariant_Bool Bool;
        FmtValueVariant_Uchar Uchar;
        FmtValueVariant_Schar Schar;
        FmtValueVariant_Ushort Ushort;
        FmtValueVariant_Sshort Sshort;
        FmtValueVariant_Uint Uint;
        FmtValueVariant_Sint Sint;
        FmtValueVariant_Ulong Ulong;
        FmtValueVariant_Slong Slong;
        FmtValueVariant_Isize Isize;
        FmtValueVariant_Usize Usize;
        FmtValueVariant_Float Float;
        FmtValueVariant_Double Double;
        FmtValueVariant_Str Str;
        FmtValueVariant_StrVec StrVec;
    };
    
    
    struct FmtArg {
        String key;
        FmtValue value;
        FmtArg& operator=(const FmtArg&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FmtArg(const FmtArg&) = delete; /* disable copy constructor, use explicit .clone() */
        FmtArg() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleFontFamilyVec {
        StyleFontFamily* ptr;
        size_t len;
        size_t cap;
        StyleFontFamilyVecDestructor destructor;
        StyleFontFamilyVec& operator=(const StyleFontFamilyVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleFontFamilyVec(const StyleFontFamilyVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyleFontFamilyVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct FmtArgVec {
        FmtArg* ptr;
        size_t len;
        size_t cap;
        FmtArgVecDestructor destructor;
        FmtArgVec& operator=(const FmtArgVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FmtArgVec(const FmtArgVec&) = delete; /* disable copy constructor, use explicit .clone() */
        FmtArgVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineWordVec {
        InlineWord* ptr;
        size_t len;
        size_t cap;
        InlineWordVecDestructor destructor;
        InlineWordVec& operator=(const InlineWordVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineWordVec(const InlineWordVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineWordVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct MonitorVec {
        Monitor* ptr;
        size_t len;
        size_t cap;
        MonitorVecDestructor destructor;
        MonitorVec& operator=(const MonitorVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        MonitorVec(const MonitorVec&) = delete; /* disable copy constructor, use explicit .clone() */
        MonitorVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct IdOrClassVec {
        IdOrClass* ptr;
        size_t len;
        size_t cap;
        IdOrClassVecDestructor destructor;
        IdOrClassVec& operator=(const IdOrClassVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        IdOrClassVec(const IdOrClassVec&) = delete; /* disable copy constructor, use explicit .clone() */
        IdOrClassVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyleBackgroundContentVec {
        StyleBackgroundContent* ptr;
        size_t len;
        size_t cap;
        StyleBackgroundContentVecDestructor destructor;
        StyleBackgroundContentVec& operator=(const StyleBackgroundContentVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyleBackgroundContentVec(const StyleBackgroundContentVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StyleBackgroundContentVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgPathVec {
        SvgPath* ptr;
        size_t len;
        size_t cap;
        SvgPathVecDestructor destructor;
        SvgPathVec& operator=(const SvgPathVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgPathVec(const SvgPathVec&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgPathVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct VertexAttributeVec {
        VertexAttribute* ptr;
        size_t len;
        size_t cap;
        VertexAttributeVecDestructor destructor;
        VertexAttributeVec& operator=(const VertexAttributeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VertexAttributeVec(const VertexAttributeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        VertexAttributeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CssPathSelectorVec {
        CssPathSelector* ptr;
        size_t len;
        size_t cap;
        CssPathSelectorVecDestructor destructor;
        CssPathSelectorVec& operator=(const CssPathSelectorVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssPathSelectorVec(const CssPathSelectorVec&) = delete; /* disable copy constructor, use explicit .clone() */
        CssPathSelectorVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CallbackDataVec {
        CallbackData* ptr;
        size_t len;
        size_t cap;
        CallbackDataVecDestructor destructor;
        CallbackDataVec& operator=(const CallbackDataVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CallbackDataVec(const CallbackDataVec&) = delete; /* disable copy constructor, use explicit .clone() */
        CallbackDataVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct DebugMessageVec {
        DebugMessage* ptr;
        size_t len;
        size_t cap;
        DebugMessageVecDestructor destructor;
        DebugMessageVec& operator=(const DebugMessageVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DebugMessageVec(const DebugMessageVec&) = delete; /* disable copy constructor, use explicit .clone() */
        DebugMessageVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StringPairVec {
        StringPair* ptr;
        size_t len;
        size_t cap;
        StringPairVecDestructor destructor;
        StringPairVec& operator=(const StringPairVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StringPairVec(const StringPairVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StringPairVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class OptionFileTypeListTag {
       None,
       Some,
    };
    
    struct OptionFileTypeListVariant_None { OptionFileTypeListTag tag; };
    struct OptionFileTypeListVariant_Some { OptionFileTypeListTag tag; FileTypeList payload; };
    union OptionFileTypeList {
        OptionFileTypeListVariant_None None;
        OptionFileTypeListVariant_Some Some;
    };
    
    
    enum class OptionFileTag {
       None,
       Some,
    };
    
    struct OptionFileVariant_None { OptionFileTag tag; };
    struct OptionFileVariant_Some { OptionFileTag tag; File payload; };
    union OptionFile {
        OptionFileVariant_None None;
        OptionFileVariant_Some Some;
    };
    
    
    enum class OptionRawImageTag {
       None,
       Some,
    };
    
    struct OptionRawImageVariant_None { OptionRawImageTag tag; };
    struct OptionRawImageVariant_Some { OptionRawImageTag tag; RawImage payload; };
    union OptionRawImage {
        OptionRawImageVariant_None None;
        OptionRawImageVariant_Some Some;
    };
    
    
    enum class OptionWaylandThemeTag {
       None,
       Some,
    };
    
    struct OptionWaylandThemeVariant_None { OptionWaylandThemeTag tag; };
    struct OptionWaylandThemeVariant_Some { OptionWaylandThemeTag tag; WaylandTheme payload; };
    union OptionWaylandTheme {
        OptionWaylandThemeVariant_None None;
        OptionWaylandThemeVariant_Some Some;
    };
    
    
    enum class ResultRawImageDecodeImageErrorTag {
       Ok,
       Err,
    };
    
    struct ResultRawImageDecodeImageErrorVariant_Ok { ResultRawImageDecodeImageErrorTag tag; RawImage payload; };
    struct ResultRawImageDecodeImageErrorVariant_Err { ResultRawImageDecodeImageErrorTag tag; DecodeImageError payload; };
    union ResultRawImageDecodeImageError {
        ResultRawImageDecodeImageErrorVariant_Ok Ok;
        ResultRawImageDecodeImageErrorVariant_Err Err;
    };
    
    
    enum class XmlStreamErrorTag {
       UnexpectedEndOfStream,
       InvalidName,
       NonXmlChar,
       InvalidChar,
       InvalidCharMultiple,
       InvalidQuote,
       InvalidSpace,
       InvalidString,
       InvalidReference,
       InvalidExternalID,
       InvalidCommentData,
       InvalidCommentEnd,
       InvalidCharacterData,
    };
    
    struct XmlStreamErrorVariant_UnexpectedEndOfStream { XmlStreamErrorTag tag; };
    struct XmlStreamErrorVariant_InvalidName { XmlStreamErrorTag tag; };
    struct XmlStreamErrorVariant_NonXmlChar { XmlStreamErrorTag tag; NonXmlCharError payload; };
    struct XmlStreamErrorVariant_InvalidChar { XmlStreamErrorTag tag; InvalidCharError payload; };
    struct XmlStreamErrorVariant_InvalidCharMultiple { XmlStreamErrorTag tag; InvalidCharMultipleError payload; };
    struct XmlStreamErrorVariant_InvalidQuote { XmlStreamErrorTag tag; InvalidQuoteError payload; };
    struct XmlStreamErrorVariant_InvalidSpace { XmlStreamErrorTag tag; InvalidSpaceError payload; };
    struct XmlStreamErrorVariant_InvalidString { XmlStreamErrorTag tag; InvalidStringError payload; };
    struct XmlStreamErrorVariant_InvalidReference { XmlStreamErrorTag tag; };
    struct XmlStreamErrorVariant_InvalidExternalID { XmlStreamErrorTag tag; };
    struct XmlStreamErrorVariant_InvalidCommentData { XmlStreamErrorTag tag; };
    struct XmlStreamErrorVariant_InvalidCommentEnd { XmlStreamErrorTag tag; };
    struct XmlStreamErrorVariant_InvalidCharacterData { XmlStreamErrorTag tag; };
    union XmlStreamError {
        XmlStreamErrorVariant_UnexpectedEndOfStream UnexpectedEndOfStream;
        XmlStreamErrorVariant_InvalidName InvalidName;
        XmlStreamErrorVariant_NonXmlChar NonXmlChar;
        XmlStreamErrorVariant_InvalidChar InvalidChar;
        XmlStreamErrorVariant_InvalidCharMultiple InvalidCharMultiple;
        XmlStreamErrorVariant_InvalidQuote InvalidQuote;
        XmlStreamErrorVariant_InvalidSpace InvalidSpace;
        XmlStreamErrorVariant_InvalidString InvalidString;
        XmlStreamErrorVariant_InvalidReference InvalidReference;
        XmlStreamErrorVariant_InvalidExternalID InvalidExternalID;
        XmlStreamErrorVariant_InvalidCommentData InvalidCommentData;
        XmlStreamErrorVariant_InvalidCommentEnd InvalidCommentEnd;
        XmlStreamErrorVariant_InvalidCharacterData InvalidCharacterData;
    };
    
    
    struct LinuxWindowOptions {
        OptionX11Visual x11_visual;
        OptionI32 x11_screen;
        StringPairVec x11_wm_classes;
        bool  x11_override_redirect;
        XWindowTypeVec x11_window_types;
        OptionString x11_gtk_theme_variant;
        OptionLogicalSize x11_resize_increments;
        OptionLogicalSize x11_base_size;
        OptionString wayland_app_id;
        OptionWaylandTheme wayland_theme;
        bool  request_user_attention;
        OptionWindowIcon window_icon;
        LinuxWindowOptions& operator=(const LinuxWindowOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        LinuxWindowOptions(const LinuxWindowOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        LinuxWindowOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineLine {
        InlineWordVec words;
        LogicalRect bounds;
        InlineLine& operator=(const InlineLine&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineLine(const InlineLine&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineLine() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class MenuItemTag {
       String,
       Separator,
       BreakLine,
    };
    
    struct MenuItemVariant_String { MenuItemTag tag; StringMenuItem payload; };
    struct MenuItemVariant_Separator { MenuItemTag tag; };
    struct MenuItemVariant_BreakLine { MenuItemTag tag; };
    union MenuItem {
        MenuItemVariant_String String;
        MenuItemVariant_Separator Separator;
        MenuItemVariant_BreakLine BreakLine;
    };
    
    
    struct CssPath {
        CssPathSelectorVec selectors;
        CssPath& operator=(const CssPath&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssPath(const CssPath&) = delete; /* disable copy constructor, use explicit .clone() */
        CssPath() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class StyleBackgroundContentVecValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleBackgroundContentVecValueVariant_Auto { StyleBackgroundContentVecValueTag tag; };
    struct StyleBackgroundContentVecValueVariant_None { StyleBackgroundContentVecValueTag tag; };
    struct StyleBackgroundContentVecValueVariant_Inherit { StyleBackgroundContentVecValueTag tag; };
    struct StyleBackgroundContentVecValueVariant_Initial { StyleBackgroundContentVecValueTag tag; };
    struct StyleBackgroundContentVecValueVariant_Exact { StyleBackgroundContentVecValueTag tag; StyleBackgroundContentVec payload; };
    union StyleBackgroundContentVecValue {
        StyleBackgroundContentVecValueVariant_Auto Auto;
        StyleBackgroundContentVecValueVariant_None None;
        StyleBackgroundContentVecValueVariant_Inherit Inherit;
        StyleBackgroundContentVecValueVariant_Initial Initial;
        StyleBackgroundContentVecValueVariant_Exact Exact;
    };
    
    
    enum class StyleFontFamilyVecValueTag {
       Auto,
       None,
       Inherit,
       Initial,
       Exact,
    };
    
    struct StyleFontFamilyVecValueVariant_Auto { StyleFontFamilyVecValueTag tag; };
    struct StyleFontFamilyVecValueVariant_None { StyleFontFamilyVecValueTag tag; };
    struct StyleFontFamilyVecValueVariant_Inherit { StyleFontFamilyVecValueTag tag; };
    struct StyleFontFamilyVecValueVariant_Initial { StyleFontFamilyVecValueTag tag; };
    struct StyleFontFamilyVecValueVariant_Exact { StyleFontFamilyVecValueTag tag; StyleFontFamilyVec payload; };
    union StyleFontFamilyVecValue {
        StyleFontFamilyVecValueVariant_Auto Auto;
        StyleFontFamilyVecValueVariant_None None;
        StyleFontFamilyVecValueVariant_Inherit Inherit;
        StyleFontFamilyVecValueVariant_Initial Initial;
        StyleFontFamilyVecValueVariant_Exact Exact;
    };
    
    
    enum class CssPropertyTag {
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
       BackgroundContent,
       BackgroundPosition,
       BackgroundSize,
       BackgroundRepeat,
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
       TransformOrigin,
       PerspectiveOrigin,
       BackfaceVisibility,
       MixBlendMode,
       Filter,
       BackdropFilter,
       TextShadow,
    };
    
    struct CssPropertyVariant_TextColor { CssPropertyTag tag; StyleTextColorValue payload; };
    struct CssPropertyVariant_FontSize { CssPropertyTag tag; StyleFontSizeValue payload; };
    struct CssPropertyVariant_FontFamily { CssPropertyTag tag; StyleFontFamilyVecValue payload; };
    struct CssPropertyVariant_TextAlign { CssPropertyTag tag; StyleTextAlignValue payload; };
    struct CssPropertyVariant_LetterSpacing { CssPropertyTag tag; StyleLetterSpacingValue payload; };
    struct CssPropertyVariant_LineHeight { CssPropertyTag tag; StyleLineHeightValue payload; };
    struct CssPropertyVariant_WordSpacing { CssPropertyTag tag; StyleWordSpacingValue payload; };
    struct CssPropertyVariant_TabWidth { CssPropertyTag tag; StyleTabWidthValue payload; };
    struct CssPropertyVariant_Cursor { CssPropertyTag tag; StyleCursorValue payload; };
    struct CssPropertyVariant_Display { CssPropertyTag tag; LayoutDisplayValue payload; };
    struct CssPropertyVariant_Float { CssPropertyTag tag; LayoutFloatValue payload; };
    struct CssPropertyVariant_BoxSizing { CssPropertyTag tag; LayoutBoxSizingValue payload; };
    struct CssPropertyVariant_Width { CssPropertyTag tag; LayoutWidthValue payload; };
    struct CssPropertyVariant_Height { CssPropertyTag tag; LayoutHeightValue payload; };
    struct CssPropertyVariant_MinWidth { CssPropertyTag tag; LayoutMinWidthValue payload; };
    struct CssPropertyVariant_MinHeight { CssPropertyTag tag; LayoutMinHeightValue payload; };
    struct CssPropertyVariant_MaxWidth { CssPropertyTag tag; LayoutMaxWidthValue payload; };
    struct CssPropertyVariant_MaxHeight { CssPropertyTag tag; LayoutMaxHeightValue payload; };
    struct CssPropertyVariant_Position { CssPropertyTag tag; LayoutPositionValue payload; };
    struct CssPropertyVariant_Top { CssPropertyTag tag; LayoutTopValue payload; };
    struct CssPropertyVariant_Right { CssPropertyTag tag; LayoutRightValue payload; };
    struct CssPropertyVariant_Left { CssPropertyTag tag; LayoutLeftValue payload; };
    struct CssPropertyVariant_Bottom { CssPropertyTag tag; LayoutBottomValue payload; };
    struct CssPropertyVariant_FlexWrap { CssPropertyTag tag; LayoutFlexWrapValue payload; };
    struct CssPropertyVariant_FlexDirection { CssPropertyTag tag; LayoutFlexDirectionValue payload; };
    struct CssPropertyVariant_FlexGrow { CssPropertyTag tag; LayoutFlexGrowValue payload; };
    struct CssPropertyVariant_FlexShrink { CssPropertyTag tag; LayoutFlexShrinkValue payload; };
    struct CssPropertyVariant_JustifyContent { CssPropertyTag tag; LayoutJustifyContentValue payload; };
    struct CssPropertyVariant_AlignItems { CssPropertyTag tag; LayoutAlignItemsValue payload; };
    struct CssPropertyVariant_AlignContent { CssPropertyTag tag; LayoutAlignContentValue payload; };
    struct CssPropertyVariant_BackgroundContent { CssPropertyTag tag; StyleBackgroundContentVecValue payload; };
    struct CssPropertyVariant_BackgroundPosition { CssPropertyTag tag; StyleBackgroundPositionVecValue payload; };
    struct CssPropertyVariant_BackgroundSize { CssPropertyTag tag; StyleBackgroundSizeVecValue payload; };
    struct CssPropertyVariant_BackgroundRepeat { CssPropertyTag tag; StyleBackgroundRepeatVecValue payload; };
    struct CssPropertyVariant_OverflowX { CssPropertyTag tag; LayoutOverflowValue payload; };
    struct CssPropertyVariant_OverflowY { CssPropertyTag tag; LayoutOverflowValue payload; };
    struct CssPropertyVariant_PaddingTop { CssPropertyTag tag; LayoutPaddingTopValue payload; };
    struct CssPropertyVariant_PaddingLeft { CssPropertyTag tag; LayoutPaddingLeftValue payload; };
    struct CssPropertyVariant_PaddingRight { CssPropertyTag tag; LayoutPaddingRightValue payload; };
    struct CssPropertyVariant_PaddingBottom { CssPropertyTag tag; LayoutPaddingBottomValue payload; };
    struct CssPropertyVariant_MarginTop { CssPropertyTag tag; LayoutMarginTopValue payload; };
    struct CssPropertyVariant_MarginLeft { CssPropertyTag tag; LayoutMarginLeftValue payload; };
    struct CssPropertyVariant_MarginRight { CssPropertyTag tag; LayoutMarginRightValue payload; };
    struct CssPropertyVariant_MarginBottom { CssPropertyTag tag; LayoutMarginBottomValue payload; };
    struct CssPropertyVariant_BorderTopLeftRadius { CssPropertyTag tag; StyleBorderTopLeftRadiusValue payload; };
    struct CssPropertyVariant_BorderTopRightRadius { CssPropertyTag tag; StyleBorderTopRightRadiusValue payload; };
    struct CssPropertyVariant_BorderBottomLeftRadius { CssPropertyTag tag; StyleBorderBottomLeftRadiusValue payload; };
    struct CssPropertyVariant_BorderBottomRightRadius { CssPropertyTag tag; StyleBorderBottomRightRadiusValue payload; };
    struct CssPropertyVariant_BorderTopColor { CssPropertyTag tag; StyleBorderTopColorValue payload; };
    struct CssPropertyVariant_BorderRightColor { CssPropertyTag tag; StyleBorderRightColorValue payload; };
    struct CssPropertyVariant_BorderLeftColor { CssPropertyTag tag; StyleBorderLeftColorValue payload; };
    struct CssPropertyVariant_BorderBottomColor { CssPropertyTag tag; StyleBorderBottomColorValue payload; };
    struct CssPropertyVariant_BorderTopStyle { CssPropertyTag tag; StyleBorderTopStyleValue payload; };
    struct CssPropertyVariant_BorderRightStyle { CssPropertyTag tag; StyleBorderRightStyleValue payload; };
    struct CssPropertyVariant_BorderLeftStyle { CssPropertyTag tag; StyleBorderLeftStyleValue payload; };
    struct CssPropertyVariant_BorderBottomStyle { CssPropertyTag tag; StyleBorderBottomStyleValue payload; };
    struct CssPropertyVariant_BorderTopWidth { CssPropertyTag tag; LayoutBorderTopWidthValue payload; };
    struct CssPropertyVariant_BorderRightWidth { CssPropertyTag tag; LayoutBorderRightWidthValue payload; };
    struct CssPropertyVariant_BorderLeftWidth { CssPropertyTag tag; LayoutBorderLeftWidthValue payload; };
    struct CssPropertyVariant_BorderBottomWidth { CssPropertyTag tag; LayoutBorderBottomWidthValue payload; };
    struct CssPropertyVariant_BoxShadowLeft { CssPropertyTag tag; StyleBoxShadowValue payload; };
    struct CssPropertyVariant_BoxShadowRight { CssPropertyTag tag; StyleBoxShadowValue payload; };
    struct CssPropertyVariant_BoxShadowTop { CssPropertyTag tag; StyleBoxShadowValue payload; };
    struct CssPropertyVariant_BoxShadowBottom { CssPropertyTag tag; StyleBoxShadowValue payload; };
    struct CssPropertyVariant_ScrollbarStyle { CssPropertyTag tag; ScrollbarStyleValue payload; };
    struct CssPropertyVariant_Opacity { CssPropertyTag tag; StyleOpacityValue payload; };
    struct CssPropertyVariant_Transform { CssPropertyTag tag; StyleTransformVecValue payload; };
    struct CssPropertyVariant_TransformOrigin { CssPropertyTag tag; StyleTransformOriginValue payload; };
    struct CssPropertyVariant_PerspectiveOrigin { CssPropertyTag tag; StylePerspectiveOriginValue payload; };
    struct CssPropertyVariant_BackfaceVisibility { CssPropertyTag tag; StyleBackfaceVisibilityValue payload; };
    struct CssPropertyVariant_MixBlendMode { CssPropertyTag tag; StyleMixBlendModeValue payload; };
    struct CssPropertyVariant_Filter { CssPropertyTag tag; StyleFilterVecValue payload; };
    struct CssPropertyVariant_BackdropFilter { CssPropertyTag tag; StyleFilterVecValue payload; };
    struct CssPropertyVariant_TextShadow { CssPropertyTag tag; StyleBoxShadowValue payload; };
    union CssProperty {
        CssPropertyVariant_TextColor TextColor;
        CssPropertyVariant_FontSize FontSize;
        CssPropertyVariant_FontFamily FontFamily;
        CssPropertyVariant_TextAlign TextAlign;
        CssPropertyVariant_LetterSpacing LetterSpacing;
        CssPropertyVariant_LineHeight LineHeight;
        CssPropertyVariant_WordSpacing WordSpacing;
        CssPropertyVariant_TabWidth TabWidth;
        CssPropertyVariant_Cursor Cursor;
        CssPropertyVariant_Display Display;
        CssPropertyVariant_Float Float;
        CssPropertyVariant_BoxSizing BoxSizing;
        CssPropertyVariant_Width Width;
        CssPropertyVariant_Height Height;
        CssPropertyVariant_MinWidth MinWidth;
        CssPropertyVariant_MinHeight MinHeight;
        CssPropertyVariant_MaxWidth MaxWidth;
        CssPropertyVariant_MaxHeight MaxHeight;
        CssPropertyVariant_Position Position;
        CssPropertyVariant_Top Top;
        CssPropertyVariant_Right Right;
        CssPropertyVariant_Left Left;
        CssPropertyVariant_Bottom Bottom;
        CssPropertyVariant_FlexWrap FlexWrap;
        CssPropertyVariant_FlexDirection FlexDirection;
        CssPropertyVariant_FlexGrow FlexGrow;
        CssPropertyVariant_FlexShrink FlexShrink;
        CssPropertyVariant_JustifyContent JustifyContent;
        CssPropertyVariant_AlignItems AlignItems;
        CssPropertyVariant_AlignContent AlignContent;
        CssPropertyVariant_BackgroundContent BackgroundContent;
        CssPropertyVariant_BackgroundPosition BackgroundPosition;
        CssPropertyVariant_BackgroundSize BackgroundSize;
        CssPropertyVariant_BackgroundRepeat BackgroundRepeat;
        CssPropertyVariant_OverflowX OverflowX;
        CssPropertyVariant_OverflowY OverflowY;
        CssPropertyVariant_PaddingTop PaddingTop;
        CssPropertyVariant_PaddingLeft PaddingLeft;
        CssPropertyVariant_PaddingRight PaddingRight;
        CssPropertyVariant_PaddingBottom PaddingBottom;
        CssPropertyVariant_MarginTop MarginTop;
        CssPropertyVariant_MarginLeft MarginLeft;
        CssPropertyVariant_MarginRight MarginRight;
        CssPropertyVariant_MarginBottom MarginBottom;
        CssPropertyVariant_BorderTopLeftRadius BorderTopLeftRadius;
        CssPropertyVariant_BorderTopRightRadius BorderTopRightRadius;
        CssPropertyVariant_BorderBottomLeftRadius BorderBottomLeftRadius;
        CssPropertyVariant_BorderBottomRightRadius BorderBottomRightRadius;
        CssPropertyVariant_BorderTopColor BorderTopColor;
        CssPropertyVariant_BorderRightColor BorderRightColor;
        CssPropertyVariant_BorderLeftColor BorderLeftColor;
        CssPropertyVariant_BorderBottomColor BorderBottomColor;
        CssPropertyVariant_BorderTopStyle BorderTopStyle;
        CssPropertyVariant_BorderRightStyle BorderRightStyle;
        CssPropertyVariant_BorderLeftStyle BorderLeftStyle;
        CssPropertyVariant_BorderBottomStyle BorderBottomStyle;
        CssPropertyVariant_BorderTopWidth BorderTopWidth;
        CssPropertyVariant_BorderRightWidth BorderRightWidth;
        CssPropertyVariant_BorderLeftWidth BorderLeftWidth;
        CssPropertyVariant_BorderBottomWidth BorderBottomWidth;
        CssPropertyVariant_BoxShadowLeft BoxShadowLeft;
        CssPropertyVariant_BoxShadowRight BoxShadowRight;
        CssPropertyVariant_BoxShadowTop BoxShadowTop;
        CssPropertyVariant_BoxShadowBottom BoxShadowBottom;
        CssPropertyVariant_ScrollbarStyle ScrollbarStyle;
        CssPropertyVariant_Opacity Opacity;
        CssPropertyVariant_Transform Transform;
        CssPropertyVariant_TransformOrigin TransformOrigin;
        CssPropertyVariant_PerspectiveOrigin PerspectiveOrigin;
        CssPropertyVariant_BackfaceVisibility BackfaceVisibility;
        CssPropertyVariant_MixBlendMode MixBlendMode;
        CssPropertyVariant_Filter Filter;
        CssPropertyVariant_BackdropFilter BackdropFilter;
        CssPropertyVariant_TextShadow TextShadow;
    };
    
    
    struct FileInputStateWrapper {
        FileInputState inner;
        OptionFileInputOnPathChange on_file_path_change;
        String file_dialog_title;
        OptionString default_dir;
        OptionFileTypeList file_types;
        FileInputStateWrapper& operator=(const FileInputStateWrapper&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FileInputStateWrapper(const FileInputStateWrapper&) = delete; /* disable copy constructor, use explicit .clone() */
        FileInputStateWrapper() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInputStateWrapper {
        TextInputState inner;
        OptionTextInputOnTextInput on_text_input;
        OptionTextInputOnVirtualKeyDown on_virtual_key_down;
        OptionTextInputOnFocusLost on_focus_lost;
        bool  update_text_input_before_calling_focus_lost_fn;
        bool  update_text_input_before_calling_vk_down_fn;
        OptionTimerId cursor_animation;
        TextInputStateWrapper& operator=(const TextInputStateWrapper&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInputStateWrapper(const TextInputStateWrapper&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInputStateWrapper() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ProgressBar {
        ProgressBarState state;
        PixelValue height;
        StyleBackgroundContentVec bar_background;
        StyleBackgroundContentVec container_background;
        ProgressBar& operator=(const ProgressBar&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ProgressBar(const ProgressBar&) = delete; /* disable copy constructor, use explicit .clone() */
        ProgressBar() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeTypeIdInfoMap {
        NodeTypeId node_type_id;
        NodeTypeInfo node_type_info;
        NodeTypeIdInfoMap& operator=(const NodeTypeIdInfoMap&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeTypeIdInfoMap(const NodeTypeIdInfoMap&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeTypeIdInfoMap() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputOutputTypeIdInfoMap {
        InputOutputTypeId io_type_id;
        InputOutputInfo io_info;
        InputOutputTypeIdInfoMap& operator=(const InputOutputTypeIdInfoMap&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputOutputTypeIdInfoMap(const InputOutputTypeIdInfoMap&) = delete; /* disable copy constructor, use explicit .clone() */
        InputOutputTypeIdInfoMap() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeTypeField {
        String key;
        NodeTypeFieldValue value;
        NodeTypeField& operator=(const NodeTypeField&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeTypeField(const NodeTypeField&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeTypeField() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class CssPropertySourceTag {
       Css,
       Inline,
    };
    
    struct CssPropertySourceVariant_Css { CssPropertySourceTag tag; CssPath payload; };
    struct CssPropertySourceVariant_Inline { CssPropertySourceTag tag; };
    union CssPropertySource {
        CssPropertySourceVariant_Css Css;
        CssPropertySourceVariant_Inline Inline;
    };
    
    
    struct VertexLayout {
        VertexAttributeVec fields;
        VertexLayout& operator=(const VertexLayout&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VertexLayout(const VertexLayout&) = delete; /* disable copy constructor, use explicit .clone() */
        VertexLayout() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct VertexArrayObject {
        VertexLayout vertex_layout;
        uint32_t vao_id;
        Gl gl_context;
        void* refcount;
        bool  run_destructor;
        VertexArrayObject& operator=(const VertexArrayObject&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VertexArrayObject(const VertexArrayObject&) = delete; /* disable copy constructor, use explicit .clone() */
        VertexArrayObject() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct VertexBuffer {
        uint32_t vertex_buffer_id;
        size_t vertex_buffer_len;
        VertexArrayObject vao;
        uint32_t index_buffer_id;
        size_t index_buffer_len;
        IndexBufferFormat index_buffer_format;
        void* refcount;
        bool  run_destructor;
        VertexBuffer& operator=(const VertexBuffer&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        VertexBuffer(const VertexBuffer&) = delete; /* disable copy constructor, use explicit .clone() */
        VertexBuffer() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgMultiPolygon {
        SvgPathVec rings;
        SvgMultiPolygon& operator=(const SvgMultiPolygon&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgMultiPolygon(const SvgMultiPolygon&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgMultiPolygon() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class SvgSimpleNodeTag {
       Path,
       Circle,
       Rect,
       CircleHole,
       RectHole,
    };
    
    struct SvgSimpleNodeVariant_Path { SvgSimpleNodeTag tag; SvgPath payload; };
    struct SvgSimpleNodeVariant_Circle { SvgSimpleNodeTag tag; SvgCircle payload; };
    struct SvgSimpleNodeVariant_Rect { SvgSimpleNodeTag tag; SvgRect payload; };
    struct SvgSimpleNodeVariant_CircleHole { SvgSimpleNodeTag tag; SvgCircle payload; };
    struct SvgSimpleNodeVariant_RectHole { SvgSimpleNodeTag tag; SvgRect payload; };
    union SvgSimpleNode {
        SvgSimpleNodeVariant_Path Path;
        SvgSimpleNodeVariant_Circle Circle;
        SvgSimpleNodeVariant_Rect Rect;
        SvgSimpleNodeVariant_CircleHole CircleHole;
        SvgSimpleNodeVariant_RectHole RectHole;
    };
    
    
    struct TessellatedGPUSvgNode {
        VertexBuffer vertex_index_buffer;
        TessellatedGPUSvgNode& operator=(const TessellatedGPUSvgNode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TessellatedGPUSvgNode(const TessellatedGPUSvgNode&) = delete; /* disable copy constructor, use explicit .clone() */
        TessellatedGPUSvgNode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct XmlNode {
        String tag;
        StringPairVec attributes;
        XmlNodeVec children;
        OptionString text;
        XmlNode& operator=(const XmlNode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        XmlNode(const XmlNode&) = delete; /* disable copy constructor, use explicit .clone() */
        XmlNode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeTypeIdInfoMapVec {
        NodeTypeIdInfoMap* ptr;
        size_t len;
        size_t cap;
        NodeTypeIdInfoMapVecDestructor destructor;
        NodeTypeIdInfoMapVec& operator=(const NodeTypeIdInfoMapVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeTypeIdInfoMapVec(const NodeTypeIdInfoMapVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeTypeIdInfoMapVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InputOutputTypeIdInfoMapVec {
        InputOutputTypeIdInfoMap* ptr;
        size_t len;
        size_t cap;
        InputOutputTypeIdInfoMapVecDestructor destructor;
        InputOutputTypeIdInfoMapVec& operator=(const InputOutputTypeIdInfoMapVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InputOutputTypeIdInfoMapVec(const InputOutputTypeIdInfoMapVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InputOutputTypeIdInfoMapVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeTypeFieldVec {
        NodeTypeField* ptr;
        size_t len;
        size_t cap;
        NodeTypeFieldVecDestructor destructor;
        NodeTypeFieldVec& operator=(const NodeTypeFieldVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeTypeFieldVec(const NodeTypeFieldVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeTypeFieldVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineLineVec {
        InlineLine* ptr;
        size_t len;
        size_t cap;
        InlineLineVecDestructor destructor;
        InlineLineVec& operator=(const InlineLineVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineLineVec(const InlineLineVec&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineLineVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CssPropertyVec {
        CssProperty* ptr;
        size_t len;
        size_t cap;
        CssPropertyVecDestructor destructor;
        CssPropertyVec& operator=(const CssPropertyVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssPropertyVec(const CssPropertyVec&) = delete; /* disable copy constructor, use explicit .clone() */
        CssPropertyVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgMultiPolygonVec {
        SvgMultiPolygon* ptr;
        size_t len;
        size_t cap;
        SvgMultiPolygonVecDestructor destructor;
        SvgMultiPolygonVec& operator=(const SvgMultiPolygonVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgMultiPolygonVec(const SvgMultiPolygonVec&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgMultiPolygonVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct SvgSimpleNodeVec {
        SvgSimpleNode* ptr;
        size_t len;
        size_t cap;
        SvgSimpleNodeVecDestructor destructor;
        SvgSimpleNodeVec& operator=(const SvgSimpleNodeVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgSimpleNodeVec(const SvgSimpleNodeVec&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgSimpleNodeVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class OptionCssPropertyTag {
       None,
       Some,
    };
    
    struct OptionCssPropertyVariant_None { OptionCssPropertyTag tag; };
    struct OptionCssPropertyVariant_Some { OptionCssPropertyTag tag; CssProperty payload; };
    union OptionCssProperty {
        OptionCssPropertyVariant_None None;
        OptionCssPropertyVariant_Some Some;
    };
    
    
    struct XmlTextError {
        XmlStreamError stream_error;
        SvgParseErrorPosition pos;
        XmlTextError& operator=(const XmlTextError&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        XmlTextError(const XmlTextError&) = delete; /* disable copy constructor, use explicit .clone() */
        XmlTextError() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct PlatformSpecificOptions {
        WindowsWindowOptions windows_options;
        LinuxWindowOptions linux_options;
        MacWindowOptions mac_options;
        WasmWindowOptions wasm_options;
        PlatformSpecificOptions& operator=(const PlatformSpecificOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        PlatformSpecificOptions(const PlatformSpecificOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        PlatformSpecificOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct WindowState {
        String title;
        WindowTheme theme;
        WindowSize size;
        WindowPosition position;
        WindowFlags flags;
        DebugState debug_state;
        KeyboardState keyboard_state;
        MouseState mouse_state;
        TouchState touch_state;
        ImePosition ime_position;
        Monitor monitor;
        PlatformSpecificOptions platform_specific_options;
        RendererOptions renderer_options;
        ColorU background_color;
        LayoutCallback layout_callback;
        OptionCallback close_callback;
        WindowState& operator=(const WindowState&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WindowState(const WindowState&) = delete; /* disable copy constructor, use explicit .clone() */
        WindowState() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CallbackInfo {
        void* layout_results;
        size_t layout_results_count;
        void* renderer_resources;
        void* previous_window_state;
        void* current_window_state;
        WindowState* restrict modifiable_window_state;
        OptionGl* gl_context;
        void* restrict image_cache;
        void* restrict system_fonts;
        void* restrict timers;
        void* restrict threads;
        void* restrict timers_removed;
        void* restrict threads_removed;
        RawWindowHandle* current_window_handle;
        void* restrict new_windows;
        SystemCallbacks* system_callbacks;
        bool * restrict stop_propagation;
        void* restrict focus_target;
        void* restrict words_changed_in_callbacks;
        void* restrict images_changed_in_callbacks;
        void* restrict image_masks_changed_in_callbacks;
        void* restrict css_properties_changed_in_callbacks;
        void* current_scroll_states;
        void* restrict nodes_scrolled_in_callback;
        DomNodeId hit_dom_node;
        OptionLogicalPosition cursor_relative_to_item;
        OptionLogicalPosition cursor_in_viewport;
        void* _reserved_ref;
        void* restrict _reserved_mut;
        CallbackInfo& operator=(const CallbackInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CallbackInfo(const CallbackInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        CallbackInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct InlineText {
        InlineLineVec lines;
        LogicalSize content_size;
        float font_size_px;
        size_t last_word_index;
        float baseline_descender_px;
        InlineText& operator=(const InlineText&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        InlineText(const InlineText&) = delete; /* disable copy constructor, use explicit .clone() */
        InlineText() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct FocusTargetPath {
        DomId dom;
        CssPath css_path;
        FocusTargetPath& operator=(const FocusTargetPath&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FocusTargetPath(const FocusTargetPath&) = delete; /* disable copy constructor, use explicit .clone() */
        FocusTargetPath() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Animation {
        CssProperty from;
        CssProperty to;
        Duration duration;
        AnimationRepeat repeat;
        AnimationRepeatCount repeat_count;
        AnimationEasing easing;
        bool  relayout_on_finish;
        Animation& operator=(const Animation&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Animation(const Animation&) = delete; /* disable copy constructor, use explicit .clone() */
        Animation() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TimerCallbackInfo {
        CallbackInfo callback_info;
        OptionDomNodeId node_id;
        Instant frame_start;
        size_t call_count;
        bool  is_about_to_finish;
        void* _reserved_ref;
        void* restrict _reserved_mut;
        TimerCallbackInfo& operator=(const TimerCallbackInfo&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TimerCallbackInfo(const TimerCallbackInfo&) = delete; /* disable copy constructor, use explicit .clone() */
        TimerCallbackInfo() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class NodeDataInlineCssPropertyTag {
       Normal,
       Active,
       Focus,
       Hover,
    };
    
    struct NodeDataInlineCssPropertyVariant_Normal { NodeDataInlineCssPropertyTag tag; CssProperty payload; };
    struct NodeDataInlineCssPropertyVariant_Active { NodeDataInlineCssPropertyTag tag; CssProperty payload; };
    struct NodeDataInlineCssPropertyVariant_Focus { NodeDataInlineCssPropertyTag tag; CssProperty payload; };
    struct NodeDataInlineCssPropertyVariant_Hover { NodeDataInlineCssPropertyTag tag; CssProperty payload; };
    union NodeDataInlineCssProperty {
        NodeDataInlineCssPropertyVariant_Normal Normal;
        NodeDataInlineCssPropertyVariant_Active Active;
        NodeDataInlineCssPropertyVariant_Focus Focus;
        NodeDataInlineCssPropertyVariant_Hover Hover;
    };
    
    
    struct DynamicCssProperty {
        String dynamic_id;
        CssProperty default_value;
        DynamicCssProperty& operator=(const DynamicCssProperty&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        DynamicCssProperty(const DynamicCssProperty&) = delete; /* disable copy constructor, use explicit .clone() */
        DynamicCssProperty() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Node {
        NodeTypeId node_type;
        NodePosition position;
        NodeTypeFieldVec fields;
        InputConnectionVec connect_in;
        OutputConnectionVec connect_out;
        Node& operator=(const Node&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Node(const Node&) = delete; /* disable copy constructor, use explicit .clone() */
        Node() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class SvgNodeTag {
       MultiPolygonCollection,
       MultiPolygon,
       MultiShape,
       Path,
       Circle,
       Rect,
    };
    
    struct SvgNodeVariant_MultiPolygonCollection { SvgNodeTag tag; SvgMultiPolygonVec payload; };
    struct SvgNodeVariant_MultiPolygon { SvgNodeTag tag; SvgMultiPolygon payload; };
    struct SvgNodeVariant_MultiShape { SvgNodeTag tag; SvgSimpleNodeVec payload; };
    struct SvgNodeVariant_Path { SvgNodeTag tag; SvgPath payload; };
    struct SvgNodeVariant_Circle { SvgNodeTag tag; SvgCircle payload; };
    struct SvgNodeVariant_Rect { SvgNodeTag tag; SvgRect payload; };
    union SvgNode {
        SvgNodeVariant_MultiPolygonCollection MultiPolygonCollection;
        SvgNodeVariant_MultiPolygon MultiPolygon;
        SvgNodeVariant_MultiShape MultiShape;
        SvgNodeVariant_Path Path;
        SvgNodeVariant_Circle Circle;
        SvgNodeVariant_Rect Rect;
    };
    
    
    struct SvgStyledNode {
        SvgNode geometry;
        SvgStyle style;
        SvgStyledNode& operator=(const SvgStyledNode&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        SvgStyledNode(const SvgStyledNode&) = delete; /* disable copy constructor, use explicit .clone() */
        SvgStyledNode() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeDataInlineCssPropertyVec {
        NodeDataInlineCssProperty* ptr;
        size_t len;
        size_t cap;
        NodeDataInlineCssPropertyVecDestructor destructor;
        NodeDataInlineCssPropertyVec& operator=(const NodeDataInlineCssPropertyVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeDataInlineCssPropertyVec(const NodeDataInlineCssPropertyVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeDataInlineCssPropertyVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class OptionWindowStateTag {
       None,
       Some,
    };
    
    struct OptionWindowStateVariant_None { OptionWindowStateTag tag; };
    struct OptionWindowStateVariant_Some { OptionWindowStateTag tag; WindowState payload; };
    union OptionWindowState {
        OptionWindowStateVariant_None None;
        OptionWindowStateVariant_Some Some;
    };
    
    
    enum class OptionInlineTextTag {
       None,
       Some,
    };
    
    struct OptionInlineTextVariant_None { OptionInlineTextTag tag; };
    struct OptionInlineTextVariant_Some { OptionInlineTextTag tag; InlineText payload; };
    union OptionInlineText {
        OptionInlineTextVariant_None None;
        OptionInlineTextVariant_Some Some;
    };
    
    
    enum class XmlParseErrorTag {
       InvalidDeclaration,
       InvalidComment,
       InvalidPI,
       InvalidDoctype,
       InvalidEntity,
       InvalidElement,
       InvalidAttribute,
       InvalidCdata,
       InvalidCharData,
       UnknownToken,
    };
    
    struct XmlParseErrorVariant_InvalidDeclaration { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidComment { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidPI { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidDoctype { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidEntity { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidElement { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidAttribute { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidCdata { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_InvalidCharData { XmlParseErrorTag tag; XmlTextError payload; };
    struct XmlParseErrorVariant_UnknownToken { XmlParseErrorTag tag; SvgParseErrorPosition payload; };
    union XmlParseError {
        XmlParseErrorVariant_InvalidDeclaration InvalidDeclaration;
        XmlParseErrorVariant_InvalidComment InvalidComment;
        XmlParseErrorVariant_InvalidPI InvalidPI;
        XmlParseErrorVariant_InvalidDoctype InvalidDoctype;
        XmlParseErrorVariant_InvalidEntity InvalidEntity;
        XmlParseErrorVariant_InvalidElement InvalidElement;
        XmlParseErrorVariant_InvalidAttribute InvalidAttribute;
        XmlParseErrorVariant_InvalidCdata InvalidCdata;
        XmlParseErrorVariant_InvalidCharData InvalidCharData;
        XmlParseErrorVariant_UnknownToken UnknownToken;
    };
    
    
    struct WindowCreateOptions {
        WindowState state;
        bool  size_to_content;
        OptionRendererOptions renderer_type;
        OptionWindowTheme theme;
        OptionCallback create_callback;
        bool  hot_reload;
        WindowCreateOptions& operator=(const WindowCreateOptions&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        WindowCreateOptions(const WindowCreateOptions&) = delete; /* disable copy constructor, use explicit .clone() */
        WindowCreateOptions() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class FocusTargetTag {
       Id,
       Path,
       Previous,
       Next,
       First,
       Last,
       NoFocus,
    };
    
    struct FocusTargetVariant_Id { FocusTargetTag tag; DomNodeId payload; };
    struct FocusTargetVariant_Path { FocusTargetTag tag; FocusTargetPath payload; };
    struct FocusTargetVariant_Previous { FocusTargetTag tag; };
    struct FocusTargetVariant_Next { FocusTargetTag tag; };
    struct FocusTargetVariant_First { FocusTargetTag tag; };
    struct FocusTargetVariant_Last { FocusTargetTag tag; };
    struct FocusTargetVariant_NoFocus { FocusTargetTag tag; };
    union FocusTarget {
        FocusTargetVariant_Id Id;
        FocusTargetVariant_Path Path;
        FocusTargetVariant_Previous Previous;
        FocusTargetVariant_Next Next;
        FocusTargetVariant_First First;
        FocusTargetVariant_Last Last;
        FocusTargetVariant_NoFocus NoFocus;
    };
    
    
    struct NodeData {
        NodeType node_type;
        OptionRefAny dataset;
        IdOrClassVec ids_and_classes;
        CallbackDataVec callbacks;
        NodeDataInlineCssPropertyVec inline_css_props;
        OptionTabIndex tab_index;
        void* extra;
        NodeData& operator=(const NodeData&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeData(const NodeData&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeData() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class CssDeclarationTag {
       Static,
       Dynamic,
    };
    
    struct CssDeclarationVariant_Static { CssDeclarationTag tag; CssProperty payload; };
    struct CssDeclarationVariant_Dynamic { CssDeclarationTag tag; DynamicCssProperty payload; };
    union CssDeclaration {
        CssDeclarationVariant_Static Static;
        CssDeclarationVariant_Dynamic Dynamic;
    };
    
    
    struct Button {
        String label;
        OptionImageRef image;
        NodeDataInlineCssPropertyVec container_style;
        NodeDataInlineCssPropertyVec label_style;
        NodeDataInlineCssPropertyVec image_style;
        OptionButtonOnClick on_click;
        Button& operator=(const Button&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Button(const Button&) = delete; /* disable copy constructor, use explicit .clone() */
        Button() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct FileInput {
        FileInputStateWrapper state;
        String default_text;
        OptionImageRef image;
        NodeDataInlineCssPropertyVec container_style;
        NodeDataInlineCssPropertyVec label_style;
        NodeDataInlineCssPropertyVec image_style;
        FileInput& operator=(const FileInput&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        FileInput(const FileInput&) = delete; /* disable copy constructor, use explicit .clone() */
        FileInput() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CheckBox {
        CheckBoxStateWrapper state;
        NodeDataInlineCssPropertyVec container_style;
        NodeDataInlineCssPropertyVec content_style;
        CheckBox& operator=(const CheckBox&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CheckBox(const CheckBox&) = delete; /* disable copy constructor, use explicit .clone() */
        CheckBox() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Label {
        String text;
        NodeDataInlineCssPropertyVec style;
        Label& operator=(const Label&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Label(const Label&) = delete; /* disable copy constructor, use explicit .clone() */
        Label() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct ColorInput {
        ColorInputStateWrapper state;
        NodeDataInlineCssPropertyVec style;
        ColorInput& operator=(const ColorInput&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        ColorInput(const ColorInput&) = delete; /* disable copy constructor, use explicit .clone() */
        ColorInput() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TextInput {
        TextInputStateWrapper state;
        NodeDataInlineCssPropertyVec placeholder_style;
        NodeDataInlineCssPropertyVec container_style;
        NodeDataInlineCssPropertyVec label_style;
        TextInput& operator=(const TextInput&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TextInput(const TextInput&) = delete; /* disable copy constructor, use explicit .clone() */
        TextInput() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NumberInput {
        TextInput text_input;
        NumberInputStateWrapper state;
        NumberInput& operator=(const NumberInput&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NumberInput(const NumberInput&) = delete; /* disable copy constructor, use explicit .clone() */
        NumberInput() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeIdNodeMap {
        NodeGraphNodeId node_id;
        Node node;
        NodeIdNodeMap& operator=(const NodeIdNodeMap&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeIdNodeMap(const NodeIdNodeMap&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeIdNodeMap() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeIdNodeMapVec {
        NodeIdNodeMap* ptr;
        size_t len;
        size_t cap;
        NodeIdNodeMapVecDestructor destructor;
        NodeIdNodeMapVec& operator=(const NodeIdNodeMapVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeIdNodeMapVec(const NodeIdNodeMapVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeIdNodeMapVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CssDeclarationVec {
        CssDeclaration* ptr;
        size_t len;
        size_t cap;
        CssDeclarationVecDestructor destructor;
        CssDeclarationVec& operator=(const CssDeclarationVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssDeclarationVec(const CssDeclarationVec&) = delete; /* disable copy constructor, use explicit .clone() */
        CssDeclarationVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeDataVec {
        NodeData* ptr;
        size_t len;
        size_t cap;
        NodeDataVecDestructor destructor;
        NodeDataVec& operator=(const NodeDataVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeDataVec(const NodeDataVec&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeDataVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class XmlErrorTag {
       NoParserAvailable,
       InvalidXmlPrefixUri,
       UnexpectedXmlUri,
       UnexpectedXmlnsUri,
       InvalidElementNamePrefix,
       DuplicatedNamespace,
       UnknownNamespace,
       UnexpectedCloseTag,
       UnexpectedEntityCloseTag,
       UnknownEntityReference,
       MalformedEntityReference,
       EntityReferenceLoop,
       InvalidAttributeValue,
       DuplicatedAttribute,
       NoRootNode,
       SizeLimit,
       ParserError,
    };
    
    struct XmlErrorVariant_NoParserAvailable { XmlErrorTag tag; };
    struct XmlErrorVariant_InvalidXmlPrefixUri { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_UnexpectedXmlUri { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_UnexpectedXmlnsUri { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_InvalidElementNamePrefix { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_DuplicatedNamespace { XmlErrorTag tag; DuplicatedNamespaceError payload; };
    struct XmlErrorVariant_UnknownNamespace { XmlErrorTag tag; UnknownNamespaceError payload; };
    struct XmlErrorVariant_UnexpectedCloseTag { XmlErrorTag tag; UnexpectedCloseTagError payload; };
    struct XmlErrorVariant_UnexpectedEntityCloseTag { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_UnknownEntityReference { XmlErrorTag tag; UnknownEntityReferenceError payload; };
    struct XmlErrorVariant_MalformedEntityReference { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_EntityReferenceLoop { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_InvalidAttributeValue { XmlErrorTag tag; SvgParseErrorPosition payload; };
    struct XmlErrorVariant_DuplicatedAttribute { XmlErrorTag tag; DuplicatedAttributeError payload; };
    struct XmlErrorVariant_NoRootNode { XmlErrorTag tag; };
    struct XmlErrorVariant_SizeLimit { XmlErrorTag tag; };
    struct XmlErrorVariant_ParserError { XmlErrorTag tag; XmlParseError payload; };
    union XmlError {
        XmlErrorVariant_NoParserAvailable NoParserAvailable;
        XmlErrorVariant_InvalidXmlPrefixUri InvalidXmlPrefixUri;
        XmlErrorVariant_UnexpectedXmlUri UnexpectedXmlUri;
        XmlErrorVariant_UnexpectedXmlnsUri UnexpectedXmlnsUri;
        XmlErrorVariant_InvalidElementNamePrefix InvalidElementNamePrefix;
        XmlErrorVariant_DuplicatedNamespace DuplicatedNamespace;
        XmlErrorVariant_UnknownNamespace UnknownNamespace;
        XmlErrorVariant_UnexpectedCloseTag UnexpectedCloseTag;
        XmlErrorVariant_UnexpectedEntityCloseTag UnexpectedEntityCloseTag;
        XmlErrorVariant_UnknownEntityReference UnknownEntityReference;
        XmlErrorVariant_MalformedEntityReference MalformedEntityReference;
        XmlErrorVariant_EntityReferenceLoop EntityReferenceLoop;
        XmlErrorVariant_InvalidAttributeValue InvalidAttributeValue;
        XmlErrorVariant_DuplicatedAttribute DuplicatedAttribute;
        XmlErrorVariant_NoRootNode NoRootNode;
        XmlErrorVariant_SizeLimit SizeLimit;
        XmlErrorVariant_ParserError ParserError;
    };
    
    
    struct Dom {
        NodeData root;
        DomVec children;
        size_t total_children;
        Dom& operator=(const Dom&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Dom(const Dom&) = delete; /* disable copy constructor, use explicit .clone() */
        Dom() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CssRuleBlock {
        CssPath path;
        CssDeclarationVec declarations;
        CssRuleBlock& operator=(const CssRuleBlock&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssRuleBlock(const CssRuleBlock&) = delete; /* disable copy constructor, use explicit .clone() */
        CssRuleBlock() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct TabContent {
        Dom content;
        bool  has_padding;
        TabContent& operator=(const TabContent&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        TabContent(const TabContent&) = delete; /* disable copy constructor, use explicit .clone() */
        TabContent() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Frame {
        String title;
        float flex_grow;
        Dom content;
        Frame& operator=(const Frame&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Frame(const Frame&) = delete; /* disable copy constructor, use explicit .clone() */
        Frame() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct NodeGraph {
        NodeTypeIdInfoMapVec node_types;
        InputOutputTypeIdInfoMapVec input_output_types;
        NodeIdNodeMapVec nodes;
        bool  allow_multiple_root_nodes;
        LogicalPosition offset;
        NodeGraphStyle style;
        NodeGraphCallbacks callbacks;
        String add_node_str;
        float scale_factor;
        NodeGraph& operator=(const NodeGraph&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        NodeGraph(const NodeGraph&) = delete; /* disable copy constructor, use explicit .clone() */
        NodeGraph() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StyledDom {
        NodeId root;
        NodeHierarchyItemVec node_hierarchy;
        NodeDataVec node_data;
        StyledNodeVec styled_nodes;
        CascadeInfoVec cascade_info;
        NodeIdVec nodes_with_window_callbacks;
        NodeIdVec nodes_with_not_callbacks;
        NodeIdVec nodes_with_datasets_and_callbacks;
        TagIdToNodeIdMappingVec tag_ids_to_node_ids;
        ParentWithNodeDepthVec non_leaf_nodes;
        CssPropertyCache css_property_cache;
        StyledDom& operator=(const StyledDom&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StyledDom(const StyledDom&) = delete; /* disable copy constructor, use explicit .clone() */
        StyledDom() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct CssRuleBlockVec {
        CssRuleBlock* ptr;
        size_t len;
        size_t cap;
        CssRuleBlockVecDestructor destructor;
        CssRuleBlockVec& operator=(const CssRuleBlockVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        CssRuleBlockVec(const CssRuleBlockVec&) = delete; /* disable copy constructor, use explicit .clone() */
        CssRuleBlockVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class OptionDomTag {
       None,
       Some,
    };
    
    struct OptionDomVariant_None { OptionDomTag tag; };
    struct OptionDomVariant_Some { OptionDomTag tag; Dom payload; };
    union OptionDom {
        OptionDomVariant_None None;
        OptionDomVariant_Some Some;
    };
    
    
    enum class ResultXmlXmlErrorTag {
       Ok,
       Err,
    };
    
    struct ResultXmlXmlErrorVariant_Ok { ResultXmlXmlErrorTag tag; Xml payload; };
    struct ResultXmlXmlErrorVariant_Err { ResultXmlXmlErrorTag tag; XmlError payload; };
    union ResultXmlXmlError {
        ResultXmlXmlErrorVariant_Ok Ok;
        ResultXmlXmlErrorVariant_Err Err;
    };
    
    
    enum class SvgParseErrorTag {
       NoParserAvailable,
       ElementsLimitReached,
       NotAnUtf8Str,
       MalformedGZip,
       InvalidSize,
       ParsingFailed,
    };
    
    struct SvgParseErrorVariant_NoParserAvailable { SvgParseErrorTag tag; };
    struct SvgParseErrorVariant_ElementsLimitReached { SvgParseErrorTag tag; };
    struct SvgParseErrorVariant_NotAnUtf8Str { SvgParseErrorTag tag; };
    struct SvgParseErrorVariant_MalformedGZip { SvgParseErrorTag tag; };
    struct SvgParseErrorVariant_InvalidSize { SvgParseErrorTag tag; };
    struct SvgParseErrorVariant_ParsingFailed { SvgParseErrorTag tag; XmlError payload; };
    union SvgParseError {
        SvgParseErrorVariant_NoParserAvailable NoParserAvailable;
        SvgParseErrorVariant_ElementsLimitReached ElementsLimitReached;
        SvgParseErrorVariant_NotAnUtf8Str NotAnUtf8Str;
        SvgParseErrorVariant_MalformedGZip MalformedGZip;
        SvgParseErrorVariant_InvalidSize InvalidSize;
        SvgParseErrorVariant_ParsingFailed ParsingFailed;
    };
    
    
    struct IFrameCallbackReturn {
        StyledDom dom;
        LogicalSize scroll_size;
        LogicalPosition scroll_offset;
        LogicalSize virtual_scroll_size;
        LogicalPosition virtual_scroll_offset;
        IFrameCallbackReturn& operator=(const IFrameCallbackReturn&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        IFrameCallbackReturn(const IFrameCallbackReturn&) = delete; /* disable copy constructor, use explicit .clone() */
        IFrameCallbackReturn() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct Stylesheet {
        CssRuleBlockVec rules;
        Stylesheet& operator=(const Stylesheet&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Stylesheet(const Stylesheet&) = delete; /* disable copy constructor, use explicit .clone() */
        Stylesheet() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    struct StylesheetVec {
        Stylesheet* ptr;
        size_t len;
        size_t cap;
        StylesheetVecDestructor destructor;
        StylesheetVec& operator=(const StylesheetVec&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        StylesheetVec(const StylesheetVec&) = delete; /* disable copy constructor, use explicit .clone() */
        StylesheetVec() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };
    
    enum class ResultSvgXmlNodeSvgParseErrorTag {
       Ok,
       Err,
    };
    
    struct ResultSvgXmlNodeSvgParseErrorVariant_Ok { ResultSvgXmlNodeSvgParseErrorTag tag; SvgXmlNode payload; };
    struct ResultSvgXmlNodeSvgParseErrorVariant_Err { ResultSvgXmlNodeSvgParseErrorTag tag; SvgParseError payload; };
    union ResultSvgXmlNodeSvgParseError {
        ResultSvgXmlNodeSvgParseErrorVariant_Ok Ok;
        ResultSvgXmlNodeSvgParseErrorVariant_Err Err;
    };
    
    
    enum class ResultSvgSvgParseErrorTag {
       Ok,
       Err,
    };
    
    struct ResultSvgSvgParseErrorVariant_Ok { ResultSvgSvgParseErrorTag tag; Svg payload; };
    struct ResultSvgSvgParseErrorVariant_Err { ResultSvgSvgParseErrorTag tag; SvgParseError payload; };
    union ResultSvgSvgParseError {
        ResultSvgSvgParseErrorVariant_Ok Ok;
        ResultSvgSvgParseErrorVariant_Err Err;
    };
    
    
    struct Css {
        StylesheetVec stylesheets;
        Css& operator=(const Css&) = delete; /* disable assignment operator, use std::move (default) or .clone() */
        Css(const Css&) = delete; /* disable copy constructor, use explicit .clone() */
        Css() = delete; /* disable default constructor, use C++20 designated initializer instead */
    };

    extern "C" {        
        
        /* FUNCTIONS from azul.dll / libazul.so */
        App App_new(AzRefAny  data, AzAppConfig  config);
        void App_addWindow(App* restrict app, AzWindowCreateOptions  window);
        void App_addImage(App* restrict app, AzString  id, AzImageRef  image);
        MonitorVec App_getMonitors(const App* app);
        void App_run(const App* app, AzWindowCreateOptions  window);
        void App_delete(App* restrict instance);
        App App_deepCopy(App* const instance);
        AppConfig AppConfig_new(AzLayoutSolver  layout_solver);
        SystemCallbacks SystemCallbacks_libraryInternal();
        WindowCreateOptions WindowCreateOptions_new(AzLayoutCallbackType  layout_callback);
        void WindowCreateOptions_delete(WindowCreateOptions* restrict instance);
        LogicalPosition LogicalPosition_new(float x, float y);
        LogicalPosition LogicalPosition_zero();
        PhysicalSizeU32 LogicalSize_toPhysical(const LogicalSize* logicalsize, float hidpi_factor);
        void SmallWindowIconBytes_delete(SmallWindowIconBytes* restrict instance);
        void LargeWindowIconBytes_delete(LargeWindowIconBytes* restrict instance);
        void WindowIcon_delete(WindowIcon* restrict instance);
        void TaskBarIcon_delete(TaskBarIcon* restrict instance);
        float WindowSize_getHidpiFactor(const WindowSize* windowsize);
        bool  KeyboardState_shiftDown(const KeyboardState* keyboardstate);
        bool  KeyboardState_ctrlDown(const KeyboardState* keyboardstate);
        bool  KeyboardState_altDown(const KeyboardState* keyboardstate);
        bool  KeyboardState_superDown(const KeyboardState* keyboardstate);
        bool  KeyboardState_isKeyDown(const KeyboardState* keyboardstate, AzVirtualKeyCode  key);
        void KeyboardState_delete(KeyboardState* restrict instance);
        OptionLogicalPosition CursorPosition_getPosition(const CursorPosition* cursorposition);
        void PlatformSpecificOptions_delete(PlatformSpecificOptions* restrict instance);
        void WindowsWindowOptions_delete(WindowsWindowOptions* restrict instance);
        void WaylandTheme_delete(WaylandTheme* restrict instance);
        void StringPair_delete(StringPair* restrict instance);
        void LinuxWindowOptions_delete(LinuxWindowOptions* restrict instance);
        void Monitor_delete(Monitor* restrict instance);
        WindowState WindowState_new(AzLayoutCallbackType  layout_callback);
        WindowState WindowState_default();
        void WindowState_delete(WindowState* restrict instance);
        void LayoutCallback_delete(LayoutCallback* restrict instance);
        void MarshaledLayoutCallback_delete(MarshaledLayoutCallback* restrict instance);
        DomNodeId CallbackInfo_getHitNode(const CallbackInfo* callbackinfo);
        GetSystemTimeFn CallbackInfo_getSystemTimeFn(const CallbackInfo* callbackinfo);
        OptionLogicalPosition CallbackInfo_getCursorRelativeToViewport(const CallbackInfo* callbackinfo);
        OptionLogicalPosition CallbackInfo_getCursorRelativeToNode(const CallbackInfo* callbackinfo);
        WindowState CallbackInfo_getCurrentWindowState(const CallbackInfo* callbackinfo);
        KeyboardState CallbackInfo_getCurrentKeyboardState(const CallbackInfo* callbackinfo);
        MouseState CallbackInfo_getCurrentMouseState(const CallbackInfo* callbackinfo);
        OptionWindowState CallbackInfo_getPreviousWindowState(const CallbackInfo* callbackinfo);
        OptionKeyboardState CallbackInfo_getPreviousKeyboardState(const CallbackInfo* callbackinfo);
        OptionMouseState CallbackInfo_getPreviousMouseState(const CallbackInfo* callbackinfo);
        RawWindowHandle CallbackInfo_getCurrentWindowHandle(const CallbackInfo* callbackinfo);
        OptionGl CallbackInfo_getGlContext(const CallbackInfo* callbackinfo);
        OptionLogicalPosition CallbackInfo_getScrollPosition(const CallbackInfo* callbackinfo, AzDomNodeId  node_id);
        OptionRefAny CallbackInfo_getDataset(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId CallbackInfo_getNodeIdOfRootDataset(CallbackInfo* restrict callbackinfo, AzRefAny  dataset);
        OptionString CallbackInfo_getStringContents(const CallbackInfo* callbackinfo, AzDomNodeId  node_id);
        OptionInlineText CallbackInfo_getInlineText(const CallbackInfo* callbackinfo, AzDomNodeId  node_id);
        OptionFontRef CallbackInfo_getFontRef(const CallbackInfo* callbackinfo, AzDomNodeId  node_id);
        OptionResolvedTextLayoutOptions CallbackInfo_getTextLayoutOptions(const CallbackInfo* callbackinfo, AzDomNodeId  node_id);
        OptionInlineText CallbackInfo_shapeText(const CallbackInfo* callbackinfo, AzDomNodeId  node_id, AzString  text);
        size_t CallbackInfo_getIndexInParent(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId CallbackInfo_getParent(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId CallbackInfo_getPreviousSibling(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId CallbackInfo_getNextSibling(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId CallbackInfo_getFirstChild(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId CallbackInfo_getLastChild(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionPositionInfo CallbackInfo_getNodePosition(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionLogicalSize CallbackInfo_getNodeSize(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
        OptionCssProperty CallbackInfo_getComputedCssProperty(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzCssPropertyType  property_type);
        void CallbackInfo_setWindowState(CallbackInfo* restrict callbackinfo, AzWindowState  new_state);
        void CallbackInfo_setFocus(CallbackInfo* restrict callbackinfo, AzFocusTarget  target);
        void CallbackInfo_setCssProperty(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzCssProperty  new_property);
        void CallbackInfo_setScrollPosition(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzLogicalPosition  scroll_position);
        void CallbackInfo_setStringContents(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzString  string);
        void CallbackInfo_addImage(CallbackInfo* restrict callbackinfo, AzString  id, AzImageRef  image);
        bool  CallbackInfo_hasImage(const CallbackInfo* callbackinfo, AzString  id);
        OptionImageRef CallbackInfo_getImage(const CallbackInfo* callbackinfo, AzString  id);
        void CallbackInfo_updateImage(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzImageRef  new_image, AzUpdateImageType  image_type);
        void CallbackInfo_deleteImage(CallbackInfo* restrict callbackinfo, AzString  id);
        void CallbackInfo_updateImageMask(CallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzImageMask  new_mask);
        void CallbackInfo_stopPropagation(CallbackInfo* restrict callbackinfo);
        void CallbackInfo_createWindow(CallbackInfo* restrict callbackinfo, AzWindowCreateOptions  new_window);
        TimerId CallbackInfo_startTimer(CallbackInfo* restrict callbackinfo, AzTimer  timer);
        OptionTimerId CallbackInfo_startAnimation(CallbackInfo* restrict callbackinfo, AzDomNodeId  node, AzAnimation  animation);
        bool  CallbackInfo_stopTimer(CallbackInfo* restrict callbackinfo, AzTimerId  timer_id);
        OptionThreadId CallbackInfo_startThread(CallbackInfo* restrict callbackinfo, AzRefAny  thread_initialize_data, AzRefAny  writeback_data, AzThreadCallbackType  callback);
        bool  CallbackInfo_sendThreadMsg(CallbackInfo* restrict callbackinfo, AzThreadId  thread_id, AzThreadSendMsg  msg);
        bool  CallbackInfo_stopThread(CallbackInfo* restrict callbackinfo, AzThreadId  thread_id);
        void CallbackInfo_delete(CallbackInfo* restrict instance);
        bool  PositionInfo_isPositioned(const PositionInfo* positioninfo);
        LogicalPosition PositionInfo_getStaticOffset(const PositionInfo* positioninfo);
        LogicalPosition PositionInfo_getRelativeOffset(const PositionInfo* positioninfo);
        LogicalSize HidpiAdjustedBounds_getLogicalSize(const HidpiAdjustedBounds* hidpiadjustedbounds);
        PhysicalSizeU32 HidpiAdjustedBounds_getPhysicalSize(const HidpiAdjustedBounds* hidpiadjustedbounds);
        float HidpiAdjustedBounds_getHidpiFactor(const HidpiAdjustedBounds* hidpiadjustedbounds);
        InlineTextHitVec InlineText_hitTest(const InlineText* inlinetext, AzLogicalPosition  position);
        void InlineText_delete(InlineText* restrict instance);
        void InlineLine_delete(InlineLine* restrict instance);
        void InlineWord_delete(InlineWord* restrict instance);
        void InlineTextContents_delete(InlineTextContents* restrict instance);
        void FocusTarget_delete(FocusTarget* restrict instance);
        void FocusTargetPath_delete(FocusTargetPath* restrict instance);
        ResolvedTextLayoutOptions ResolvedTextLayoutOptions_default();
        void ResolvedTextLayoutOptions_delete(ResolvedTextLayoutOptions* restrict instance);
        void Animation_delete(Animation* restrict instance);
        void IFrameCallbackReturn_delete(IFrameCallbackReturn* restrict instance);
        OptionGl RenderImageCallbackInfo_getGlContext(const RenderImageCallbackInfo* renderimagecallbackinfo);
        HidpiAdjustedBounds RenderImageCallbackInfo_getBounds(const RenderImageCallbackInfo* renderimagecallbackinfo);
        DomNodeId RenderImageCallbackInfo_getCallbackNodeId(const RenderImageCallbackInfo* renderimagecallbackinfo);
        OptionInlineText RenderImageCallbackInfo_getInlineText(const RenderImageCallbackInfo* renderimagecallbackinfo, AzDomNodeId  node_id);
        size_t RenderImageCallbackInfo_getIndexInParent(RenderImageCallbackInfo* restrict renderimagecallbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId RenderImageCallbackInfo_getParent(RenderImageCallbackInfo* restrict renderimagecallbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId RenderImageCallbackInfo_getPreviousSibling(RenderImageCallbackInfo* restrict renderimagecallbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId RenderImageCallbackInfo_getNextSibling(RenderImageCallbackInfo* restrict renderimagecallbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId RenderImageCallbackInfo_getFirstChild(RenderImageCallbackInfo* restrict renderimagecallbackinfo, AzDomNodeId  node_id);
        OptionDomNodeId RenderImageCallbackInfo_getLastChild(RenderImageCallbackInfo* restrict renderimagecallbackinfo, AzDomNodeId  node_id);
        void RenderImageCallbackInfo_delete(RenderImageCallbackInfo* restrict instance);
        void TimerCallbackInfo_delete(TimerCallbackInfo* restrict instance);
        bool  RefCount_canBeShared(const RefCount* refcount);
        bool  RefCount_canBeSharedMut(const RefCount* refcount);
        void RefCount_increaseRef(RefCount* restrict refcount);
        void RefCount_decreaseRef(RefCount* restrict refcount);
        void RefCount_increaseRefmut(RefCount* restrict refcount);
        void RefCount_decreaseRefmut(RefCount* restrict refcount);
        void RefCount_delete(RefCount* restrict instance);
        RefCount RefCount_deepCopy(RefCount* const instance);
        RefAny RefAny_newC(void ptr, size_t len, uint64_t type_id, AzString  type_name, AzRefAnyDestructorType  destructor);
        uint64_t RefAny_getTypeId(const RefAny* refany);
        String RefAny_getTypeName(const RefAny* refany);
        void RefAny_delete(RefAny* restrict instance);
        RefAny RefAny_deepCopy(RefAny* const instance);
        OptionGl LayoutCallbackInfo_getGlContext(const LayoutCallbackInfo* layoutcallbackinfo);
        StringPairVec LayoutCallbackInfo_getSystemFonts(const LayoutCallbackInfo* layoutcallbackinfo);
        OptionImageRef LayoutCallbackInfo_getImage(const LayoutCallbackInfo* layoutcallbackinfo, AzString  id);
        void LayoutCallbackInfo_delete(LayoutCallbackInfo* restrict instance);
        Dom Dom_new(AzNodeType  node_type);
        Dom Dom_body();
        Dom Dom_div();
        Dom Dom_br();
        Dom Dom_text(AzString  string);
        Dom Dom_image(AzImageRef  image);
        Dom Dom_iframe(AzRefAny  data, AzIFrameCallbackType  callback);
        void Dom_setNodeType(Dom* restrict dom, AzNodeType  node_type);
        Dom Dom_withNodeType(Dom* restrict dom, AzNodeType  node_type);
        void Dom_setDataset(Dom* restrict dom, AzRefAny  dataset);
        Dom Dom_withDataset(Dom* restrict dom, AzRefAny  dataset);
        void Dom_setIdsAndClasses(Dom* restrict dom, AzIdOrClassVec  ids_and_classes);
        Dom Dom_withIdsAndClasses(Dom* restrict dom, AzIdOrClassVec  ids_and_classes);
        void Dom_setCallbacks(Dom* restrict dom, AzCallbackDataVec  callbacks);
        Dom Dom_withCallbacks(Dom* restrict dom, AzCallbackDataVec  callbacks);
        void Dom_setInlineCssProps(Dom* restrict dom, AzNodeDataInlineCssPropertyVec  css_properties);
        Dom Dom_withInlineCssProps(Dom* restrict dom, AzNodeDataInlineCssPropertyVec  css_properties);
        void Dom_addCallback(Dom* restrict dom, AzEventFilter  event, AzRefAny  data, AzCallbackType  callback);
        Dom Dom_withCallback(Dom* restrict dom, AzEventFilter  event, AzRefAny  data, AzCallbackType  callback);
        void Dom_addChild(Dom* restrict dom, AzDom  child);
        Dom Dom_withChild(Dom* restrict dom, AzDom  child);
        void Dom_setChildren(Dom* restrict dom, AzDomVec  children);
        Dom Dom_withChildren(Dom* restrict dom, AzDomVec  children);
        void Dom_addId(Dom* restrict dom, AzString  id);
        Dom Dom_withId(Dom* restrict dom, AzString  id);
        void Dom_addClass(Dom* restrict dom, AzString  class);
        Dom Dom_withClass(Dom* restrict dom, AzString  class);
        void Dom_addCssProperty(Dom* restrict dom, AzCssProperty  prop);
        Dom Dom_withCssProperty(Dom* restrict dom, AzCssProperty  prop);
        void Dom_addHoverCssProperty(Dom* restrict dom, AzCssProperty  prop);
        Dom Dom_withHoverCssProperty(Dom* restrict dom, AzCssProperty  prop);
        void Dom_addActiveCssProperty(Dom* restrict dom, AzCssProperty  prop);
        Dom Dom_withActiveCssProperty(Dom* restrict dom, AzCssProperty  prop);
        void Dom_addFocusCssProperty(Dom* restrict dom, AzCssProperty  prop);
        Dom Dom_withFocusCssProperty(Dom* restrict dom, AzCssProperty  prop);
        void Dom_setInlineStyle(Dom* restrict dom, AzString  style);
        Dom Dom_withInlineStyle(Dom* restrict dom, AzString  style);
        void Dom_setInlineHoverStyle(Dom* restrict dom, AzString  style);
        Dom Dom_withInlineHoverStyle(Dom* restrict dom, AzString  style);
        void Dom_setInlineActiveStyle(Dom* restrict dom, AzString  style);
        Dom Dom_withInlineActiveStyle(Dom* restrict dom, AzString  style);
        void Dom_setInlineFocusStyle(Dom* restrict dom, AzString  style);
        Dom Dom_withInlineFocusStyle(Dom* restrict dom, AzString  style);
        void Dom_setClipMask(Dom* restrict dom, AzImageMask  clip_mask);
        Dom Dom_withClipMask(Dom* restrict dom, AzImageMask  clip_mask);
        void Dom_setTabIndex(Dom* restrict dom, AzTabIndex  tab_index);
        Dom Dom_withTabIndex(Dom* restrict dom, AzTabIndex  tab_index);
        void Dom_setAccessibilityInfo(Dom* restrict dom, AzAccessibilityInfo  accessibility_info);
        Dom Dom_withAccessibilityInfo(Dom* restrict dom, AzAccessibilityInfo  accessibility_info);
        void Dom_setMenuBar(Dom* restrict dom, AzMenu  menu_bar);
        Dom Dom_withMenuBar(Dom* restrict dom, AzMenu  menu_bar);
        void Dom_setContextMenu(Dom* restrict dom, AzMenu  context_menu);
        Dom Dom_withContextMenu(Dom* restrict dom, AzMenu  context_menu);
        uint64_t Dom_hash(const Dom* dom);
        size_t Dom_nodeCount(const Dom* dom);
        String Dom_getHtmlString(Dom* restrict dom);
        String Dom_getHtmlStringTest(Dom* restrict dom);
        StyledDom Dom_style(Dom* restrict dom, AzCss  css);
        void Dom_delete(Dom* restrict instance);
        void IFrameNode_delete(IFrameNode* restrict instance);
        void CallbackData_delete(CallbackData* restrict instance);
        NodeData NodeData_new(AzNodeType  node_type);
        NodeData NodeData_body();
        NodeData NodeData_div();
        NodeData NodeData_br();
        NodeData NodeData_text(AzString  string);
        NodeData NodeData_image(AzImageRef  image);
        NodeData NodeData_iframe(AzRefAny  data, AzIFrameCallbackType  callback);
        void NodeData_setNodeType(NodeData* restrict nodedata, AzNodeType  node_type);
        NodeData NodeData_withNodeType(NodeData* restrict nodedata, AzNodeType  node_type);
        void NodeData_setDataset(NodeData* restrict nodedata, AzRefAny  dataset);
        NodeData NodeData_withDataset(NodeData* restrict nodedata, AzRefAny  dataset);
        void NodeData_setIdsAndClasses(NodeData* restrict nodedata, AzIdOrClassVec  ids_and_classes);
        NodeData NodeData_withIdsAndClasses(NodeData* restrict nodedata, AzIdOrClassVec  ids_and_classes);
        void NodeData_addCallback(NodeData* restrict nodedata, AzEventFilter  event, AzRefAny  data, AzCallbackType  callback);
        NodeData NodeData_withCallback(NodeData* restrict nodedata, AzEventFilter  event, AzRefAny  data, AzCallbackType  callback);
        void NodeData_setCallbacks(NodeData* restrict nodedata, AzCallbackDataVec  callbacks);
        NodeData NodeData_withCallbacks(NodeData* restrict nodedata, AzCallbackDataVec  callbacks);
        void NodeData_setInlineCssProps(NodeData* restrict nodedata, AzNodeDataInlineCssPropertyVec  css_properties);
        NodeData NodeData_withInlineCssProps(NodeData* restrict nodedata, AzNodeDataInlineCssPropertyVec  css_properties);
        void NodeData_setInlineStyle(NodeData* restrict nodedata, AzString  style);
        NodeData NodeData_withInlineStyle(NodeData* restrict nodedata, AzString  style);
        void NodeData_setInlineHoverStyle(NodeData* restrict nodedata, AzString  style);
        NodeData NodeData_withInlineHoverStyle(NodeData* restrict nodedata, AzString  style);
        void NodeData_setInlineActiveStyle(NodeData* restrict nodedata, AzString  style);
        NodeData NodeData_withInlineActiveStyle(NodeData* restrict nodedata, AzString  style);
        void NodeData_setInlineFocusStyle(NodeData* restrict nodedata, AzString  style);
        NodeData NodeData_withInlineFocusStyle(NodeData* restrict nodedata, AzString  style);
        void NodeData_setClipMask(NodeData* restrict nodedata, AzImageMask  image_mask);
        void NodeData_setTabIndex(NodeData* restrict nodedata, AzTabIndex  tab_index);
        void NodeData_setAccessibilityInfo(NodeData* restrict nodedata, AzAccessibilityInfo  accessibility_info);
        void NodeData_setMenuBar(NodeData* restrict nodedata, AzMenu  menu_bar);
        void NodeData_setContextMenu(NodeData* restrict nodedata, AzMenu  context_menu);
        uint64_t NodeData_hash(const NodeData* nodedata);
        void NodeData_delete(NodeData* restrict instance);
        void NodeType_delete(NodeType* restrict instance);
        EventFilter On_intoEventFilter(const On on);
        void AccessibilityInfo_delete(AccessibilityInfo* restrict instance);
        void IdOrClass_delete(IdOrClass* restrict instance);
        void NodeDataInlineCssProperty_delete(NodeDataInlineCssProperty* restrict instance);
        Menu Menu_new(AzMenuItemVec  items);
        void Menu_setPopupPosition(Menu* restrict menu, AzMenuPopupPosition  position);
        Menu Menu_withPopupPosition(Menu* restrict menu, AzMenuPopupPosition  position);
        void Menu_delete(Menu* restrict instance);
        void MenuItem_delete(MenuItem* restrict instance);
        StringMenuItem StringMenuItem_new(AzString  label);
        void StringMenuItem_setCallback(StringMenuItem* restrict stringmenuitem, AzRefAny  data, AzCallbackType  callback);
        StringMenuItem StringMenuItem_withCallback(StringMenuItem* restrict stringmenuitem, AzRefAny  data, AzCallbackType  callback);
        void StringMenuItem_addChild(StringMenuItem* restrict stringmenuitem, AzMenuItem  child);
        StringMenuItem StringMenuItem_withChild(StringMenuItem* restrict stringmenuitem, AzMenuItem  child);
        void StringMenuItem_setChildren(StringMenuItem* restrict stringmenuitem, AzMenuItemVec  children);
        StringMenuItem StringMenuItem_withChildren(StringMenuItem* restrict stringmenuitem, AzMenuItemVec  children);
        void StringMenuItem_delete(StringMenuItem* restrict instance);
        void VirtualKeyCodeCombo_delete(VirtualKeyCodeCombo* restrict instance);
        MenuCallback MenuCallback_new(AzRefAny  data, AzCallbackType  callback);
        void MenuCallback_delete(MenuCallback* restrict instance);
        void MenuItemIcon_delete(MenuItemIcon* restrict instance);
        void CssRuleBlock_delete(CssRuleBlock* restrict instance);
        void CssDeclaration_delete(CssDeclaration* restrict instance);
        void DynamicCssProperty_delete(DynamicCssProperty* restrict instance);
        void CssPath_delete(CssPath* restrict instance);
        void CssPathSelector_delete(CssPathSelector* restrict instance);
        void Stylesheet_delete(Stylesheet* restrict instance);
        Css Css_empty();
        Css Css_fromString(AzString  s);
        void Css_delete(Css* restrict instance);
        ColorU ColorU_fromStr(AzString  string);
        ColorU ColorU_transparent();
        ColorU ColorU_white();
        ColorU ColorU_black();
        String ColorU_toHash(const ColorU* coloru);
        float AngleValue_getDegrees(const AngleValue* anglevalue);
        void LinearGradient_delete(LinearGradient* restrict instance);
        void RadialGradient_delete(RadialGradient* restrict instance);
        void ConicGradient_delete(ConicGradient* restrict instance);
        void StyleBackgroundContent_delete(StyleBackgroundContent* restrict instance);
        void ScrollbarInfo_delete(ScrollbarInfo* restrict instance);
        void ScrollbarStyle_delete(ScrollbarStyle* restrict instance);
        void StyleFontFamily_delete(StyleFontFamily* restrict instance);
        void ScrollbarStyleValue_delete(ScrollbarStyleValue* restrict instance);
        void StyleBackgroundContentVecValue_delete(StyleBackgroundContentVecValue* restrict instance);
        void StyleBackgroundPositionVecValue_delete(StyleBackgroundPositionVecValue* restrict instance);
        void StyleBackgroundRepeatVecValue_delete(StyleBackgroundRepeatVecValue* restrict instance);
        void StyleBackgroundSizeVecValue_delete(StyleBackgroundSizeVecValue* restrict instance);
        void StyleFontFamilyVecValue_delete(StyleFontFamilyVecValue* restrict instance);
        void StyleTransformVecValue_delete(StyleTransformVecValue* restrict instance);
        void StyleFilterVecValue_delete(StyleFilterVecValue* restrict instance);
        String CssProperty_getKeyString(const CssProperty* cssproperty);
        String CssProperty_getValueString(const CssProperty* cssproperty);
        String CssProperty_getKeyValueString(const CssProperty* cssproperty);
        CssProperty CssProperty_interpolate(const CssProperty* cssproperty, AzCssProperty  other, float t, AzInterpolateContext  context);
        void CssProperty_delete(CssProperty* restrict instance);
        Dom Ribbon_dom(Ribbon* restrict ribbon, AzRibbonOnTabClickedCallback  callback, AzRefAny  data);
        Button Button_new(AzString  label);
        void Button_setOnClick(Button* restrict button, AzRefAny  data, AzCallbackType  callback);
        Button Button_withOnClick(Button* restrict button, AzRefAny  data, AzCallbackType  callback);
        Dom Button_dom(Button* restrict button);
        void Button_delete(Button* restrict instance);
        void ButtonOnClick_delete(ButtonOnClick* restrict instance);
        FileInput FileInput_new(AzOptionString  path);
        void FileInput_setDefaultText(FileInput* restrict fileinput, AzString  default_text);
        FileInput FileInput_withDefaultText(FileInput* restrict fileinput, AzString  default_text);
        void FileInput_setOnPathChange(FileInput* restrict fileinput, AzRefAny  data, AzFileInputOnPathChangeCallbackType  callback);
        FileInput FileInput_withOnPathChange(FileInput* restrict fileinput, AzRefAny  data, AzFileInputOnPathChangeCallbackType  callback);
        Dom FileInput_dom(FileInput* restrict fileinput);
        void FileInput_delete(FileInput* restrict instance);
        void FileInputStateWrapper_delete(FileInputStateWrapper* restrict instance);
        void FileInputState_delete(FileInputState* restrict instance);
        void FileInputOnPathChange_delete(FileInputOnPathChange* restrict instance);
        CheckBox CheckBox_new(bool  checked);
        void CheckBox_setOnToggle(CheckBox* restrict checkbox, AzRefAny  data, AzCheckBoxOnToggleCallbackType  callback);
        CheckBox CheckBox_withOnToggle(CheckBox* restrict checkbox, AzRefAny  data, AzCheckBoxOnToggleCallbackType  callback);
        Dom CheckBox_dom(CheckBox* restrict checkbox);
        void CheckBox_delete(CheckBox* restrict instance);
        void CheckBoxStateWrapper_delete(CheckBoxStateWrapper* restrict instance);
        void CheckBoxOnToggle_delete(CheckBoxOnToggle* restrict instance);
        Label Label_new(AzString  text);
        Dom Label_dom(Label* restrict label);
        void Label_delete(Label* restrict instance);
        ColorInput ColorInput_new(AzColorU  color);
        void ColorInput_setOnValueChange(ColorInput* restrict colorinput, AzRefAny  data, AzColorInputOnValueChangeCallbackType  callback);
        ColorInput ColorInput_withOnValueChange(ColorInput* restrict colorinput, AzRefAny  data, AzColorInputOnValueChangeCallbackType  callback);
        Dom ColorInput_dom(ColorInput* restrict colorinput);
        void ColorInput_delete(ColorInput* restrict instance);
        void ColorInputStateWrapper_delete(ColorInputStateWrapper* restrict instance);
        void ColorInputOnValueChange_delete(ColorInputOnValueChange* restrict instance);
        TextInput TextInput_new();
        void TextInput_setText(TextInput* restrict textinput, AzString  text);
        TextInput TextInput_withText(TextInput* restrict textinput, AzString  text);
        void TextInput_setPlaceholder(TextInput* restrict textinput, AzString  text);
        TextInput TextInput_withPlaceholder(TextInput* restrict textinput, AzString  text);
        void TextInput_setOnTextInput(TextInput* restrict textinput, AzRefAny  data, AzTextInputOnTextInputCallbackType  callback);
        TextInput TextInput_withOnTextInput(TextInput* restrict textinput, AzRefAny  data, AzTextInputOnTextInputCallbackType  callback);
        void TextInput_setOnVirtualKeyDown(TextInput* restrict textinput, AzRefAny  data, AzTextInputOnVirtualKeyDownCallbackType  callback);
        TextInput TextInput_withOnVirtualKeyDown(TextInput* restrict textinput, AzRefAny  data, AzTextInputOnVirtualKeyDownCallbackType  callback);
        void TextInput_setOnFocusLost(TextInput* restrict textinput, AzRefAny  data, AzTextInputOnFocusLostCallbackType  callback);
        TextInput TextInput_withOnFocusLost(TextInput* restrict textinput, AzRefAny  data, AzTextInputOnFocusLostCallbackType  callback);
        void TextInput_setPlaceholderStyle(TextInput* restrict textinput, AzNodeDataInlineCssPropertyVec  placeholder_style);
        TextInput TextInput_withPlaceholderStyle(TextInput* restrict textinput, AzNodeDataInlineCssPropertyVec  placeholder_style);
        void TextInput_setContainerStyle(TextInput* restrict textinput, AzNodeDataInlineCssPropertyVec  container_style);
        TextInput TextInput_withContainerStyle(TextInput* restrict textinput, AzNodeDataInlineCssPropertyVec  container_style);
        void TextInput_setLabelStyle(TextInput* restrict textinput, AzNodeDataInlineCssPropertyVec  label_style);
        TextInput TextInput_withLabelStyle(TextInput* restrict textinput, AzNodeDataInlineCssPropertyVec  label_style);
        Dom TextInput_dom(TextInput* restrict textinput);
        void TextInput_delete(TextInput* restrict instance);
        void TextInputStateWrapper_delete(TextInputStateWrapper* restrict instance);
        String TextInputState_getText(const TextInputState* textinputstate);
        void TextInputState_delete(TextInputState* restrict instance);
        void TextInputOnTextInput_delete(TextInputOnTextInput* restrict instance);
        void TextInputOnVirtualKeyDown_delete(TextInputOnVirtualKeyDown* restrict instance);
        void TextInputOnFocusLost_delete(TextInputOnFocusLost* restrict instance);
        NumberInput NumberInput_new(float number);
        void NumberInput_setOnTextInput(NumberInput* restrict numberinput, AzRefAny  data, AzTextInputOnTextInputCallbackType  callback);
        NumberInput NumberInput_withOnTextInput(NumberInput* restrict numberinput, AzRefAny  data, AzTextInputOnTextInputCallbackType  callback);
        void NumberInput_setOnVirtualKeyDown(NumberInput* restrict numberinput, AzRefAny  data, AzTextInputOnVirtualKeyDownCallbackType  callback);
        NumberInput NumberInput_withOnVirtualKeyDown(NumberInput* restrict numberinput, AzRefAny  data, AzTextInputOnVirtualKeyDownCallbackType  callback);
        void NumberInput_setOnFocusLost(NumberInput* restrict numberinput, AzRefAny  data, AzNumberInputOnFocusLostCallbackType  callback);
        NumberInput NumberInput_withOnFocusLost(NumberInput* restrict numberinput, AzRefAny  data, AzNumberInputOnFocusLostCallbackType  callback);
        void NumberInput_setPlaceholderStyle(NumberInput* restrict numberinput, AzNodeDataInlineCssPropertyVec  style);
        NumberInput NumberInput_withPlaceholderStyle(NumberInput* restrict numberinput, AzNodeDataInlineCssPropertyVec  style);
        void NumberInput_setContainerStyle(NumberInput* restrict numberinput, AzNodeDataInlineCssPropertyVec  style);
        NumberInput NumberInput_withContainerStyle(NumberInput* restrict numberinput, AzNodeDataInlineCssPropertyVec  style);
        void NumberInput_setLabelStyle(NumberInput* restrict numberinput, AzNodeDataInlineCssPropertyVec  style);
        NumberInput NumberInput_withLabelStyle(NumberInput* restrict numberinput, AzNodeDataInlineCssPropertyVec  style);
        void NumberInput_setOnValueChange(NumberInput* restrict numberinput, AzRefAny  data, AzNumberInputOnValueChangeCallbackType  callback);
        NumberInput NumberInput_withOnValueChange(NumberInput* restrict numberinput, AzRefAny  data, AzNumberInputOnValueChangeCallbackType  callback);
        Dom NumberInput_dom(NumberInput* restrict numberinput);
        void NumberInput_delete(NumberInput* restrict instance);
        void NumberInputStateWrapper_delete(NumberInputStateWrapper* restrict instance);
        void NumberInputOnValueChange_delete(NumberInputOnValueChange* restrict instance);
        void NumberInputOnFocusLost_delete(NumberInputOnFocusLost* restrict instance);
        ProgressBar ProgressBar_new(float percent_done);
        void ProgressBar_setHeight(ProgressBar* restrict progressbar, AzPixelValue  height);
        ProgressBar ProgressBar_withHeight(ProgressBar* restrict progressbar, AzPixelValue  height);
        void ProgressBar_setContainerBackground(ProgressBar* restrict progressbar, AzStyleBackgroundContentVec  background);
        ProgressBar ProgressBar_withContainerStyle(ProgressBar* restrict progressbar, AzStyleBackgroundContentVec  background);
        void ProgressBar_setBarBackground(ProgressBar* restrict progressbar, AzStyleBackgroundContentVec  background);
        ProgressBar ProgressBar_withBarBackground(ProgressBar* restrict progressbar, AzStyleBackgroundContentVec  background);
        Dom ProgressBar_dom(ProgressBar* restrict progressbar);
        void ProgressBar_delete(ProgressBar* restrict instance);
        TabHeader TabHeader_new(AzStringVec  tabs);
        void TabHeader_setActiveTab(TabHeader* restrict tabheader, size_t active_tab);
        TabHeader TabHeader_withActiveTab(TabHeader* restrict tabheader, size_t active_tab);
        void TabHeader_setOnClick(TabHeader* restrict tabheader, AzRefAny  data, AzTabOnClickCallbackType  callback);
        TabHeader TabHeader_withOnClick(TabHeader* restrict tabheader, AzRefAny  data, AzTabOnClickCallbackType  callback);
        Dom TabHeader_dom(TabHeader* restrict tabheader);
        void TabHeader_delete(TabHeader* restrict instance);
        TabContent TabContent_new(AzDom  content);
        void TabContent_setPadding(TabContent* restrict tabcontent, bool  has_padding);
        TabContent TabContent_withPadding(TabContent* restrict tabcontent, bool  has_padding);
        Dom TabContent_dom(TabContent* restrict tabcontent);
        void TabContent_delete(TabContent* restrict instance);
        void TabOnClick_delete(TabOnClick* restrict instance);
        Frame Frame_new(AzString  title, AzDom  dom);
        void Frame_setFlexGrow(Frame* restrict frame, float flex_grow);
        Frame Frame_withFlexGrow(Frame* restrict frame, float flex_grow);
        Dom Frame_dom(Frame* restrict frame);
        void Frame_delete(Frame* restrict instance);
        Dom NodeGraph_dom(NodeGraph* restrict nodegraph);
        void NodeGraph_delete(NodeGraph* restrict instance);
        void NodeTypeIdInfoMap_delete(NodeTypeIdInfoMap* restrict instance);
        void InputOutputTypeIdInfoMap_delete(InputOutputTypeIdInfoMap* restrict instance);
        void NodeIdNodeMap_delete(NodeIdNodeMap* restrict instance);
        void NodeGraphCallbacks_delete(NodeGraphCallbacks* restrict instance);
        void NodeGraphOnNodeAdded_delete(NodeGraphOnNodeAdded* restrict instance);
        void NodeGraphOnNodeRemoved_delete(NodeGraphOnNodeRemoved* restrict instance);
        void NodeGraphOnNodeGraphDragged_delete(NodeGraphOnNodeGraphDragged* restrict instance);
        void NodeGraphOnNodeDragged_delete(NodeGraphOnNodeDragged* restrict instance);
        void NodeGraphOnNodeConnected_delete(NodeGraphOnNodeConnected* restrict instance);
        void NodeGraphOnNodeInputDisconnected_delete(NodeGraphOnNodeInputDisconnected* restrict instance);
        void NodeGraphOnNodeOutputDisconnected_delete(NodeGraphOnNodeOutputDisconnected* restrict instance);
        void NodeGraphOnNodeFieldEdited_delete(NodeGraphOnNodeFieldEdited* restrict instance);
        void Node_delete(Node* restrict instance);
        void NodeTypeField_delete(NodeTypeField* restrict instance);
        void NodeTypeFieldValue_delete(NodeTypeFieldValue* restrict instance);
        void InputConnection_delete(InputConnection* restrict instance);
        void OutputConnection_delete(OutputConnection* restrict instance);
        void NodeTypeInfo_delete(NodeTypeInfo* restrict instance);
        void InputOutputInfo_delete(InputOutputInfo* restrict instance);
        ListView ListView_new(AzStringVec  columns);
        ListView ListView_withRows(ListView* restrict listview, AzListViewRowVec  rows);
        Dom ListView_dom(ListView* restrict listview);
        void ListView_delete(ListView* restrict instance);
        void ListViewRow_delete(ListViewRow* restrict instance);
        void ListViewState_delete(ListViewState* restrict instance);
        void ListViewOnLazyLoadScroll_delete(ListViewOnLazyLoadScroll* restrict instance);
        void ListViewOnColumnClick_delete(ListViewOnColumnClick* restrict instance);
        void ListViewOnRowClick_delete(ListViewOnRowClick* restrict instance);
        TreeView TreeView_new(AzString  root);
        Dom TreeView_dom(TreeView* restrict treeview);
        void TreeView_delete(TreeView* restrict instance);
        DropDown DropDown_new(AzStringVec  choices);
        Dom DropDown_dom(DropDown* restrict dropdown);
        void DropDown_delete(DropDown* restrict instance);
        void DropDownOnChoiceChange_delete(DropDownOnChoiceChange* restrict instance);
        void CssPropertySource_delete(CssPropertySource* restrict instance);
        void TagIdToNodeIdMapping_delete(TagIdToNodeIdMapping* restrict instance);
        void CssPropertyCache_delete(CssPropertyCache* restrict instance);
        CssPropertyCache CssPropertyCache_deepCopy(CssPropertyCache* const instance);
        StyledDom StyledDom_new(AzDom  dom, AzCss  css);
        StyledDom StyledDom_default();
        StyledDom StyledDom_fromXml(AzString  xml_string);
        StyledDom StyledDom_fromFile(AzString  xml_file_path);
        void StyledDom_appendChild(StyledDom* restrict styleddom, AzStyledDom  dom);
        StyledDom StyledDom_withChild(StyledDom* restrict styleddom, AzStyledDom  dom);
        void StyledDom_restyle(StyledDom* restrict styleddom, AzCss  css);
        size_t StyledDom_nodeCount(const StyledDom* styleddom);
        String StyledDom_getHtmlString(const StyledDom* styleddom);
        String StyledDom_getHtmlStringTest(const StyledDom* styleddom);
        void StyledDom_setMenuBar(StyledDom* restrict styleddom, AzMenu  menu);
        StyledDom StyledDom_withMenuBar(StyledDom* restrict styleddom, AzMenu  menu);
        void StyledDom_setContextMenu(StyledDom* restrict styleddom, AzMenu  menu);
        StyledDom StyledDom_withContextMenu(StyledDom* restrict styleddom, AzMenu  menu);
        void StyledDom_delete(StyledDom* restrict instance);
        Texture Texture_new(uint32_t texture_id, AzTextureFlags  flags, AzPhysicalSizeU32  size, AzColorU  background_color, AzGl  gl_context, AzRawImageFormat  format);
        Texture Texture_allocateRgba8(AzGl  gl, AzPhysicalSizeU32  size, AzColorU  background);
        Texture Texture_allocateClipMask(AzGl  gl, AzPhysicalSizeU32  size, AzColorU  background);
        void Texture_clear(Texture* restrict texture);
        bool  Texture_drawClipMask(Texture* restrict texture, AzTessellatedSvgNode  node);
        bool  Texture_drawTesselatedSvgGpuNode(Texture* restrict texture, AzTessellatedGPUSvgNode * node, AzPhysicalSizeU32  size, AzColorU  color, AzStyleTransformVec  transforms);
        bool  Texture_applyFxaa(Texture* restrict texture);
        void Texture_delete(Texture* restrict instance);
        Texture Texture_deepCopy(Texture* const instance);
        void GlVoidPtrConst_delete(GlVoidPtrConst* restrict instance);
        GlVoidPtrConst GlVoidPtrConst_deepCopy(GlVoidPtrConst* const instance);
        GlType Gl_getType(const Gl* gl);
        void Gl_bufferDataUntyped(const Gl* gl, uint32_t target, ssize_t size, AzGlVoidPtrConst  data, uint32_t usage);
        void Gl_bufferSubDataUntyped(const Gl* gl, uint32_t target, ssize_t offset, ssize_t size, AzGlVoidPtrConst  data);
        GlVoidPtrMut Gl_mapBuffer(const Gl* gl, uint32_t target, uint32_t access);
        GlVoidPtrMut Gl_mapBufferRange(const Gl* gl, uint32_t target, ssize_t offset, ssize_t length, uint32_t access);
        uint8_t Gl_unmapBuffer(const Gl* gl, uint32_t target);
        void Gl_texBuffer(const Gl* gl, uint32_t target, uint32_t internal_format, uint32_t buffer);
        void Gl_shaderSource(const Gl* gl, uint32_t shader, AzStringVec  strings);
        void Gl_readBuffer(const Gl* gl, uint32_t mode);
        void Gl_readPixelsIntoBuffer(const Gl* gl, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t pixel_type, AzU8VecRefMut  dst_buffer);
        U8Vec Gl_readPixels(const Gl* gl, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t pixel_type);
        void Gl_readPixelsIntoPbo(const Gl* gl, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t pixel_type);
        void Gl_sampleCoverage(const Gl* gl, float value, bool  invert);
        void Gl_polygonOffset(const Gl* gl, float factor, float units);
        void Gl_pixelStoreI(const Gl* gl, uint32_t name, int32_t param);
        GLuintVec Gl_genBuffers(const Gl* gl, int32_t n);
        GLuintVec Gl_genRenderbuffers(const Gl* gl, int32_t n);
        GLuintVec Gl_genFramebuffers(const Gl* gl, int32_t n);
        GLuintVec Gl_genTextures(const Gl* gl, int32_t n);
        GLuintVec Gl_genVertexArrays(const Gl* gl, int32_t n);
        GLuintVec Gl_genQueries(const Gl* gl, int32_t n);
        void Gl_beginQuery(const Gl* gl, uint32_t target, uint32_t id);
        void Gl_endQuery(const Gl* gl, uint32_t target);
        void Gl_queryCounter(const Gl* gl, uint32_t id, uint32_t target);
        int32_t Gl_getQueryObjectIv(const Gl* gl, uint32_t id, uint32_t pname);
        uint32_t Gl_getQueryObjectUiv(const Gl* gl, uint32_t id, uint32_t pname);
        int64_t Gl_getQueryObjectI64V(const Gl* gl, uint32_t id, uint32_t pname);
        uint64_t Gl_getQueryObjectUi64V(const Gl* gl, uint32_t id, uint32_t pname);
        void Gl_deleteQueries(const Gl* gl, AzGLuintVecRef  queries);
        void Gl_deleteVertexArrays(const Gl* gl, AzGLuintVecRef  vertex_arrays);
        void Gl_deleteBuffers(const Gl* gl, AzGLuintVecRef  buffers);
        void Gl_deleteRenderbuffers(const Gl* gl, AzGLuintVecRef  renderbuffers);
        void Gl_deleteFramebuffers(const Gl* gl, AzGLuintVecRef  framebuffers);
        void Gl_deleteTextures(const Gl* gl, AzGLuintVecRef  textures);
        void Gl_framebufferRenderbuffer(const Gl* gl, uint32_t target, uint32_t attachment, uint32_t renderbuffertarget, uint32_t renderbuffer);
        void Gl_renderbufferStorage(const Gl* gl, uint32_t target, uint32_t internalformat, int32_t width, int32_t height);
        void Gl_depthFunc(const Gl* gl, uint32_t func);
        void Gl_activeTexture(const Gl* gl, uint32_t texture);
        void Gl_attachShader(const Gl* gl, uint32_t program, uint32_t shader);
        void Gl_bindAttribLocation(const Gl* gl, uint32_t program, uint32_t index, AzRefstr  name);
        void Gl_getUniformIv(const Gl* gl, uint32_t program, int32_t location, AzGLintVecRefMut  result);
        void Gl_getUniformFv(const Gl* gl, uint32_t program, int32_t location, AzGLfloatVecRefMut  result);
        uint32_t Gl_getUniformBlockIndex(const Gl* gl, uint32_t program, AzRefstr  name);
        GLuintVec Gl_getUniformIndices(const Gl* gl, uint32_t program, AzRefstrVecRef  names);
        void Gl_bindBufferBase(const Gl* gl, uint32_t target, uint32_t index, uint32_t buffer);
        void Gl_bindBufferRange(const Gl* gl, uint32_t target, uint32_t index, uint32_t buffer, ssize_t offset, ssize_t size);
        void Gl_uniformBlockBinding(const Gl* gl, uint32_t program, uint32_t uniform_block_index, uint32_t uniform_block_binding);
        void Gl_bindBuffer(const Gl* gl, uint32_t target, uint32_t buffer);
        void Gl_bindVertexArray(const Gl* gl, uint32_t vao);
        void Gl_bindRenderbuffer(const Gl* gl, uint32_t target, uint32_t renderbuffer);
        void Gl_bindFramebuffer(const Gl* gl, uint32_t target, uint32_t framebuffer);
        void Gl_bindTexture(const Gl* gl, uint32_t target, uint32_t texture);
        void Gl_drawBuffers(const Gl* gl, AzGLenumVecRef  bufs);
        void Gl_texImage2D(const Gl* gl, uint32_t target, int32_t level, int32_t internal_format, int32_t width, int32_t height, int32_t border, uint32_t format, uint32_t ty, AzOptionU8VecRef  opt_data);
        void Gl_compressedTexImage2D(const Gl* gl, uint32_t target, int32_t level, uint32_t internal_format, int32_t width, int32_t height, int32_t border, AzU8VecRef  data);
        void Gl_compressedTexSubImage2D(const Gl* gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height, uint32_t format, AzU8VecRef  data);
        void Gl_texImage3D(const Gl* gl, uint32_t target, int32_t level, int32_t internal_format, int32_t width, int32_t height, int32_t depth, int32_t border, uint32_t format, uint32_t ty, AzOptionU8VecRef  opt_data);
        void Gl_copyTexImage2D(const Gl* gl, uint32_t target, int32_t level, uint32_t internal_format, int32_t x, int32_t y, int32_t width, int32_t height, int32_t border);
        void Gl_copyTexSubImage2D(const Gl* gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t x, int32_t y, int32_t width, int32_t height);
        void Gl_copyTexSubImage3D(const Gl* gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t zoffset, int32_t x, int32_t y, int32_t width, int32_t height);
        void Gl_texSubImage2D(const Gl* gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height, uint32_t format, uint32_t ty, AzU8VecRef  data);
        void Gl_texSubImage2DPbo(const Gl* gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height, uint32_t format, uint32_t ty, size_t offset);
        void Gl_texSubImage3D(const Gl* gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t zoffset, int32_t width, int32_t height, int32_t depth, uint32_t format, uint32_t ty, AzU8VecRef  data);
        void Gl_texSubImage3DPbo(const Gl* gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t zoffset, int32_t width, int32_t height, int32_t depth, uint32_t format, uint32_t ty, size_t offset);
        void Gl_texStorage2D(const Gl* gl, uint32_t target, int32_t levels, uint32_t internal_format, int32_t width, int32_t height);
        void Gl_texStorage3D(const Gl* gl, uint32_t target, int32_t levels, uint32_t internal_format, int32_t width, int32_t height, int32_t depth);
        void Gl_getTexImageIntoBuffer(const Gl* gl, uint32_t target, int32_t level, uint32_t format, uint32_t ty, AzU8VecRefMut  output);
        void Gl_copyImageSubData(const Gl* gl, uint32_t src_name, uint32_t src_target, int32_t src_level, int32_t src_x, int32_t src_y, int32_t src_z, uint32_t dst_name, uint32_t dst_target, int32_t dst_level, int32_t dst_x, int32_t dst_y, int32_t dst_z, int32_t src_width, int32_t src_height, int32_t src_depth);
        void Gl_invalidateFramebuffer(const Gl* gl, uint32_t target, AzGLenumVecRef  attachments);
        void Gl_invalidateSubFramebuffer(const Gl* gl, uint32_t target, AzGLenumVecRef  attachments, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height);
        void Gl_getIntegerV(const Gl* gl, uint32_t name, AzGLintVecRefMut  result);
        void Gl_getInteger64V(const Gl* gl, uint32_t name, AzGLint64VecRefMut  result);
        void Gl_getIntegerIv(const Gl* gl, uint32_t name, uint32_t index, AzGLintVecRefMut  result);
        void Gl_getInteger64Iv(const Gl* gl, uint32_t name, uint32_t index, AzGLint64VecRefMut  result);
        void Gl_getBooleanV(const Gl* gl, uint32_t name, AzGLbooleanVecRefMut  result);
        void Gl_getFloatV(const Gl* gl, uint32_t name, AzGLfloatVecRefMut  result);
        int32_t Gl_getFramebufferAttachmentParameterIv(const Gl* gl, uint32_t target, uint32_t attachment, uint32_t pname);
        int32_t Gl_getRenderbufferParameterIv(const Gl* gl, uint32_t target, uint32_t pname);
        int32_t Gl_getTexParameterIv(const Gl* gl, uint32_t target, uint32_t name);
        float Gl_getTexParameterFv(const Gl* gl, uint32_t target, uint32_t name);
        void Gl_texParameterI(const Gl* gl, uint32_t target, uint32_t pname, int32_t param);
        void Gl_texParameterF(const Gl* gl, uint32_t target, uint32_t pname, float param);
        void Gl_framebufferTexture2D(const Gl* gl, uint32_t target, uint32_t attachment, uint32_t textarget, uint32_t texture, int32_t level);
        void Gl_framebufferTextureLayer(const Gl* gl, uint32_t target, uint32_t attachment, uint32_t texture, int32_t level, int32_t layer);
        void Gl_blitFramebuffer(const Gl* gl, int32_t src_x0, int32_t src_y0, int32_t src_x1, int32_t src_y1, int32_t dst_x0, int32_t dst_y0, int32_t dst_x1, int32_t dst_y1, uint32_t mask, uint32_t filter);
        void Gl_vertexAttrib4F(const Gl* gl, uint32_t index, float x, float y, float z, float w);
        void Gl_vertexAttribPointerF32(const Gl* gl, uint32_t index, int32_t size, bool  normalized, int32_t stride, uint32_t offset);
        void Gl_vertexAttribPointer(const Gl* gl, uint32_t index, int32_t size, uint32_t type_, bool  normalized, int32_t stride, uint32_t offset);
        void Gl_vertexAttribIPointer(const Gl* gl, uint32_t index, int32_t size, uint32_t type_, int32_t stride, uint32_t offset);
        void Gl_vertexAttribDivisor(const Gl* gl, uint32_t index, uint32_t divisor);
        void Gl_viewport(const Gl* gl, int32_t x, int32_t y, int32_t width, int32_t height);
        void Gl_scissor(const Gl* gl, int32_t x, int32_t y, int32_t width, int32_t height);
        void Gl_lineWidth(const Gl* gl, float width);
        void Gl_useProgram(const Gl* gl, uint32_t program);
        void Gl_validateProgram(const Gl* gl, uint32_t program);
        void Gl_drawArrays(const Gl* gl, uint32_t mode, int32_t first, int32_t count);
        void Gl_drawArraysInstanced(const Gl* gl, uint32_t mode, int32_t first, int32_t count, int32_t primcount);
        void Gl_drawElements(const Gl* gl, uint32_t mode, int32_t count, uint32_t element_type, uint32_t indices_offset);
        void Gl_drawElementsInstanced(const Gl* gl, uint32_t mode, int32_t count, uint32_t element_type, uint32_t indices_offset, int32_t primcount);
        void Gl_blendColor(const Gl* gl, float r, float g, float b, float a);
        void Gl_blendFunc(const Gl* gl, uint32_t sfactor, uint32_t dfactor);
        void Gl_blendFuncSeparate(const Gl* gl, uint32_t src_rgb, uint32_t dest_rgb, uint32_t src_alpha, uint32_t dest_alpha);
        void Gl_blendEquation(const Gl* gl, uint32_t mode);
        void Gl_blendEquationSeparate(const Gl* gl, uint32_t mode_rgb, uint32_t mode_alpha);
        void Gl_colorMask(const Gl* gl, bool  r, bool  g, bool  b, bool  a);
        void Gl_cullFace(const Gl* gl, uint32_t mode);
        void Gl_frontFace(const Gl* gl, uint32_t mode);
        void Gl_enable(const Gl* gl, uint32_t cap);
        void Gl_disable(const Gl* gl, uint32_t cap);
        void Gl_hint(const Gl* gl, uint32_t param_name, uint32_t param_val);
        uint8_t Gl_isEnabled(const Gl* gl, uint32_t cap);
        uint8_t Gl_isShader(const Gl* gl, uint32_t shader);
        uint8_t Gl_isTexture(const Gl* gl, uint32_t texture);
        uint8_t Gl_isFramebuffer(const Gl* gl, uint32_t framebuffer);
        uint8_t Gl_isRenderbuffer(const Gl* gl, uint32_t renderbuffer);
        uint32_t Gl_checkFrameBufferStatus(const Gl* gl, uint32_t target);
        void Gl_enableVertexAttribArray(const Gl* gl, uint32_t index);
        void Gl_disableVertexAttribArray(const Gl* gl, uint32_t index);
        void Gl_uniform1F(const Gl* gl, int32_t location, float v0);
        void Gl_uniform1Fv(const Gl* gl, int32_t location, AzF32VecRef  values);
        void Gl_uniform1I(const Gl* gl, int32_t location, int32_t v0);
        void Gl_uniform1Iv(const Gl* gl, int32_t location, AzI32VecRef  values);
        void Gl_uniform1Ui(const Gl* gl, int32_t location, uint32_t v0);
        void Gl_uniform2F(const Gl* gl, int32_t location, float v0, float v1);
        void Gl_uniform2Fv(const Gl* gl, int32_t location, AzF32VecRef  values);
        void Gl_uniform2I(const Gl* gl, int32_t location, int32_t v0, int32_t v1);
        void Gl_uniform2Iv(const Gl* gl, int32_t location, AzI32VecRef  values);
        void Gl_uniform2Ui(const Gl* gl, int32_t location, uint32_t v0, uint32_t v1);
        void Gl_uniform3F(const Gl* gl, int32_t location, float v0, float v1, float v2);
        void Gl_uniform3Fv(const Gl* gl, int32_t location, AzF32VecRef  values);
        void Gl_uniform3I(const Gl* gl, int32_t location, int32_t v0, int32_t v1, int32_t v2);
        void Gl_uniform3Iv(const Gl* gl, int32_t location, AzI32VecRef  values);
        void Gl_uniform3Ui(const Gl* gl, int32_t location, uint32_t v0, uint32_t v1, uint32_t v2);
        void Gl_uniform4F(const Gl* gl, int32_t location, float x, float y, float z, float w);
        void Gl_uniform4I(const Gl* gl, int32_t location, int32_t x, int32_t y, int32_t z, int32_t w);
        void Gl_uniform4Iv(const Gl* gl, int32_t location, AzI32VecRef  values);
        void Gl_uniform4Ui(const Gl* gl, int32_t location, uint32_t x, uint32_t y, uint32_t z, uint32_t w);
        void Gl_uniform4Fv(const Gl* gl, int32_t location, AzF32VecRef  values);
        void Gl_uniformMatrix2Fv(const Gl* gl, int32_t location, bool  transpose, AzF32VecRef  value);
        void Gl_uniformMatrix3Fv(const Gl* gl, int32_t location, bool  transpose, AzF32VecRef  value);
        void Gl_uniformMatrix4Fv(const Gl* gl, int32_t location, bool  transpose, AzF32VecRef  value);
        void Gl_depthMask(const Gl* gl, bool  flag);
        void Gl_depthRange(const Gl* gl, double near, double far);
        GetActiveAttribReturn Gl_getActiveAttrib(const Gl* gl, uint32_t program, uint32_t index);
        GetActiveUniformReturn Gl_getActiveUniform(const Gl* gl, uint32_t program, uint32_t index);
        GLintVec Gl_getActiveUniformsIv(const Gl* gl, uint32_t program, AzGLuintVec  indices, uint32_t pname);
        int32_t Gl_getActiveUniformBlockI(const Gl* gl, uint32_t program, uint32_t index, uint32_t pname);
        GLintVec Gl_getActiveUniformBlockIv(const Gl* gl, uint32_t program, uint32_t index, uint32_t pname);
        String Gl_getActiveUniformBlockName(const Gl* gl, uint32_t program, uint32_t index);
        int32_t Gl_getAttribLocation(const Gl* gl, uint32_t program, AzRefstr  name);
        int32_t Gl_getFragDataLocation(const Gl* gl, uint32_t program, AzRefstr  name);
        int32_t Gl_getUniformLocation(const Gl* gl, uint32_t program, AzRefstr  name);
        String Gl_getProgramInfoLog(const Gl* gl, uint32_t program);
        void Gl_getProgramIv(const Gl* gl, uint32_t program, uint32_t pname, AzGLintVecRefMut  result);
        GetProgramBinaryReturn Gl_getProgramBinary(const Gl* gl, uint32_t program);
        void Gl_programBinary(const Gl* gl, uint32_t program, uint32_t format, AzU8VecRef  binary);
        void Gl_programParameterI(const Gl* gl, uint32_t program, uint32_t pname, int32_t value);
        void Gl_getVertexAttribIv(const Gl* gl, uint32_t index, uint32_t pname, AzGLintVecRefMut  result);
        void Gl_getVertexAttribFv(const Gl* gl, uint32_t index, uint32_t pname, AzGLfloatVecRefMut  result);
        ssize_t Gl_getVertexAttribPointerV(const Gl* gl, uint32_t index, uint32_t pname);
        int32_t Gl_getBufferParameterIv(const Gl* gl, uint32_t target, uint32_t pname);
        String Gl_getShaderInfoLog(const Gl* gl, uint32_t shader);
        String Gl_getString(const Gl* gl, uint32_t which);
        String Gl_getStringI(const Gl* gl, uint32_t which, uint32_t index);
        void Gl_getShaderIv(const Gl* gl, uint32_t shader, uint32_t pname, AzGLintVecRefMut  result);
        GlShaderPrecisionFormatReturn Gl_getShaderPrecisionFormat(const Gl* gl, uint32_t shader_type, uint32_t precision_type);
        void Gl_compileShader(const Gl* gl, uint32_t shader);
        uint32_t Gl_createProgram(const Gl* gl);
        void Gl_deleteProgram(const Gl* gl, uint32_t program);
        uint32_t Gl_createShader(const Gl* gl, uint32_t shader_type);
        void Gl_deleteShader(const Gl* gl, uint32_t shader);
        void Gl_detachShader(const Gl* gl, uint32_t program, uint32_t shader);
        void Gl_linkProgram(const Gl* gl, uint32_t program);
        void Gl_clearColor(const Gl* gl, float r, float g, float b, float a);
        void Gl_clear(const Gl* gl, uint32_t buffer_mask);
        void Gl_clearDepth(const Gl* gl, double depth);
        void Gl_clearStencil(const Gl* gl, int32_t s);
        void Gl_flush(const Gl* gl);
        void Gl_finish(const Gl* gl);
        uint32_t Gl_getError(const Gl* gl);
        void Gl_stencilMask(const Gl* gl, uint32_t mask);
        void Gl_stencilMaskSeparate(const Gl* gl, uint32_t face, uint32_t mask);
        void Gl_stencilFunc(const Gl* gl, uint32_t func, int32_t ref_, uint32_t mask);
        void Gl_stencilFuncSeparate(const Gl* gl, uint32_t face, uint32_t func, int32_t ref_, uint32_t mask);
        void Gl_stencilOp(const Gl* gl, uint32_t sfail, uint32_t dpfail, uint32_t dppass);
        void Gl_stencilOpSeparate(const Gl* gl, uint32_t face, uint32_t sfail, uint32_t dpfail, uint32_t dppass);
        void Gl_eglImageTargetTexture2DOes(const Gl* gl, uint32_t target, AzGlVoidPtrConst  image);
        void Gl_generateMipmap(const Gl* gl, uint32_t target);
        void Gl_insertEventMarkerExt(const Gl* gl, AzRefstr  message);
        void Gl_pushGroupMarkerExt(const Gl* gl, AzRefstr  message);
        void Gl_popGroupMarkerExt(const Gl* gl);
        void Gl_debugMessageInsertKhr(const Gl* gl, uint32_t source, uint32_t type_, uint32_t id, uint32_t severity, AzRefstr  message);
        void Gl_pushDebugGroupKhr(const Gl* gl, uint32_t source, uint32_t id, AzRefstr  message);
        void Gl_popDebugGroupKhr(const Gl* gl);
        GLsyncPtr Gl_fenceSync(const Gl* gl, uint32_t condition, uint32_t flags);
        uint32_t Gl_clientWaitSync(const Gl* gl, AzGLsyncPtr  sync, uint32_t flags, uint64_t timeout);
        void Gl_waitSync(const Gl* gl, AzGLsyncPtr  sync, uint32_t flags, uint64_t timeout);
        void Gl_deleteSync(const Gl* gl, AzGLsyncPtr  sync);
        void Gl_textureRangeApple(const Gl* gl, uint32_t target, AzU8VecRef  data);
        GLuintVec Gl_genFencesApple(const Gl* gl, int32_t n);
        void Gl_deleteFencesApple(const Gl* gl, AzGLuintVecRef  fences);
        void Gl_setFenceApple(const Gl* gl, uint32_t fence);
        void Gl_finishFenceApple(const Gl* gl, uint32_t fence);
        void Gl_testFenceApple(const Gl* gl, uint32_t fence);
        uint8_t Gl_testObjectApple(const Gl* gl, uint32_t object, uint32_t name);
        void Gl_finishObjectApple(const Gl* gl, uint32_t object, uint32_t name);
        int32_t Gl_getFragDataIndex(const Gl* gl, uint32_t program, AzRefstr  name);
        void Gl_blendBarrierKhr(const Gl* gl);
        void Gl_bindFragDataLocationIndexed(const Gl* gl, uint32_t program, uint32_t color_number, uint32_t index, AzRefstr  name);
        DebugMessageVec Gl_getDebugMessages(const Gl* gl);
        void Gl_provokingVertexAngle(const Gl* gl, uint32_t mode);
        GLuintVec Gl_genVertexArraysApple(const Gl* gl, int32_t n);
        void Gl_bindVertexArrayApple(const Gl* gl, uint32_t vao);
        void Gl_deleteVertexArraysApple(const Gl* gl, AzGLuintVecRef  vertex_arrays);
        void Gl_copyTextureChromium(const Gl* gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t internal_format, uint32_t dest_type, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
        void Gl_copySubTextureChromium(const Gl* gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t x_offset, int32_t y_offset, int32_t x, int32_t y, int32_t width, int32_t height, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
        void Gl_eglImageTargetRenderbufferStorageOes(const Gl* gl, uint32_t target, AzGlVoidPtrConst  image);
        void Gl_copyTexture3DAngle(const Gl* gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t internal_format, uint32_t dest_type, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
        void Gl_copySubTexture3DAngle(const Gl* gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t x_offset, int32_t y_offset, int32_t z_offset, int32_t x, int32_t y, int32_t z, int32_t width, int32_t height, int32_t depth, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
        void Gl_bufferStorage(const Gl* gl, uint32_t target, ssize_t size, AzGlVoidPtrConst  data, uint32_t flags);
        void Gl_flushMappedBufferRange(const Gl* gl, uint32_t target, ssize_t offset, ssize_t length);
        void Gl_delete(Gl* restrict instance);
        Gl Gl_deepCopy(Gl* const instance);
        void VertexAttribute_delete(VertexAttribute* restrict instance);
        void VertexLayout_delete(VertexLayout* restrict instance);
        VertexArrayObject VertexArrayObject_new(AzVertexLayout  vertex_layout, uint32_t vao_id, AzGl  gl_context);
        void VertexArrayObject_delete(VertexArrayObject* restrict instance);
        VertexArrayObject VertexArrayObject_deepCopy(VertexArrayObject* const instance);
        VertexBuffer VertexBuffer_new(uint32_t vertex_buffer_id, size_t vertex_buffer_len, AzVertexArrayObject  vao, uint32_t index_buffer_id, size_t index_buffer_len, AzIndexBufferFormat  index_buffer_format);
        void VertexBuffer_delete(VertexBuffer* restrict instance);
        VertexBuffer VertexBuffer_deepCopy(VertexBuffer* const instance);
        void DebugMessage_delete(DebugMessage* restrict instance);
        void GetProgramBinaryReturn_delete(GetProgramBinaryReturn* restrict instance);
        void GetActiveAttribReturn_delete(GetActiveAttribReturn* restrict instance);
        void GLsyncPtr_delete(GLsyncPtr* restrict instance);
        GLsyncPtr GLsyncPtr_deepCopy(GLsyncPtr* const instance);
        void GetActiveUniformReturn_delete(GetActiveUniformReturn* restrict instance);
        TextureFlags TextureFlags_default();
        ImageRef ImageRef_invalid(size_t width, size_t height, AzRawImageFormat  format);
        ImageRef ImageRef_rawImage(AzRawImage  data);
        ImageRef ImageRef_glTexture(AzTexture  texture);
        ImageRef ImageRef_callback(AzRefAny  data, AzRenderImageCallbackType  callback);
        ImageRef ImageRef_cloneBytes(const ImageRef* imageref);
        bool  ImageRef_isInvalid(const ImageRef* imageref);
        bool  ImageRef_isGlTexture(const ImageRef* imageref);
        bool  ImageRef_isRawImage(const ImageRef* imageref);
        bool  ImageRef_isCallback(const ImageRef* imageref);
        OptionRawImage ImageRef_getRawImage(const ImageRef* imageref);
        uint64_t ImageRef_getHash(const ImageRef* imageref);
        void ImageRef_delete(ImageRef* restrict instance);
        ImageRef ImageRef_deepCopy(ImageRef* const instance);
        RawImage RawImage_empty();
        RawImage RawImage_allocateClipMask(AzLayoutSize  size);
        RawImage RawImage_decodeImageBytesAny(AzU8VecRef  bytes);
        bool  RawImage_drawClipMask(RawImage* restrict rawimage, AzSvgNode  node, AzSvgStyle  style);
        ResultU8VecEncodeImageError RawImage_encodeBmp(const RawImage* rawimage);
        ResultU8VecEncodeImageError RawImage_encodePng(const RawImage* rawimage);
        ResultU8VecEncodeImageError RawImage_encodeJpeg(const RawImage* rawimage, uint8_t quality);
        ResultU8VecEncodeImageError RawImage_encodeTga(const RawImage* rawimage);
        ResultU8VecEncodeImageError RawImage_encodePnm(const RawImage* rawimage);
        ResultU8VecEncodeImageError RawImage_encodeGif(const RawImage* rawimage);
        ResultU8VecEncodeImageError RawImage_encodeTiff(const RawImage* rawimage);
        void RawImage_delete(RawImage* restrict instance);
        void ImageMask_delete(ImageMask* restrict instance);
        void RawImageData_delete(RawImageData* restrict instance);
        FontMetrics FontMetrics_zero();
        bool  FontMetrics_useTypoMetrics(const FontMetrics* fontmetrics);
        float FontMetrics_getAscender(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getDescender(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getLineGap(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getXMin(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYMin(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getXMax(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYMax(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getAdvanceWidthMax(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getMinLeftSideBearing(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getMinRightSideBearing(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getXMaxExtent(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getXAvgCharWidth(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSubscriptXSize(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSubscriptYSize(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSubscriptXOffset(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSubscriptYOffset(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSuperscriptXSize(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSuperscriptYSize(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSuperscriptXOffset(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYSuperscriptYOffset(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYStrikeoutSize(const FontMetrics* fontmetrics, float target_font_size);
        float FontMetrics_getYStrikeoutPosition(const FontMetrics* fontmetrics, float target_font_size);
        void FontSource_delete(FontSource* restrict instance);
        FontRef FontRef_parse(AzFontSource  source);
        U8Vec FontRef_getBytes(const FontRef* fontref);
        FontMetrics FontRef_getFontMetrics(const FontRef* fontref);
        InlineText FontRef_shapeText(const FontRef* fontref, AzRefstr  text, AzResolvedTextLayoutOptions  options);
        uint64_t FontRef_getHash(const FontRef* fontref);
        void FontRef_delete(FontRef* restrict instance);
        FontRef FontRef_deepCopy(FontRef* const instance);
        Svg Svg_fromString(AzString  svg_string, AzSvgParseOptions  parse_options);
        Svg Svg_fromBytes(AzU8VecRef  svg_bytes, AzSvgParseOptions  parse_options);
        SvgXmlNode Svg_getRoot(const Svg* svg);
        OptionRawImage Svg_render(const Svg* svg, AzSvgRenderOptions  options);
        String Svg_toString(const Svg* svg, AzSvgStringFormatOptions  options);
        void Svg_delete(Svg* restrict instance);
        Svg Svg_deepCopy(Svg* const instance);
        SvgXmlNode SvgXmlNode_parseFrom(AzU8VecRef  svg_bytes, AzSvgParseOptions  parse_options);
        void SvgXmlNode_delete(SvgXmlNode* restrict instance);
        SvgXmlNode SvgXmlNode_deepCopy(SvgXmlNode* const instance);
        SvgRect SvgMultiPolygon_getBounds(const SvgMultiPolygon* svgmultipolygon);
        bool  SvgMultiPolygon_containsPoint(const SvgMultiPolygon* svgmultipolygon, AzSvgPoint  point, AzSvgFillRule  fill_rule, float tolerance);
        SvgMultiPolygon SvgMultiPolygon_union(const SvgMultiPolygon* svgmultipolygon, AzSvgMultiPolygon  other);
        SvgMultiPolygon SvgMultiPolygon_intersection(const SvgMultiPolygon* svgmultipolygon, AzSvgMultiPolygon  other);
        SvgMultiPolygon SvgMultiPolygon_difference(const SvgMultiPolygon* svgmultipolygon, AzSvgMultiPolygon  other);
        SvgMultiPolygon SvgMultiPolygon_xor(const SvgMultiPolygon* svgmultipolygon, AzSvgMultiPolygon  other);
        TessellatedSvgNode SvgMultiPolygon_tessellateFill(const SvgMultiPolygon* svgmultipolygon, AzSvgFillStyle  fill_style);
        TessellatedSvgNode SvgMultiPolygon_tessellateStroke(const SvgMultiPolygon* svgmultipolygon, AzSvgStrokeStyle  stroke_style);
        void SvgMultiPolygon_delete(SvgMultiPolygon* restrict instance);
        TessellatedSvgNode SvgNode_tessellateFill(const SvgNode* svgnode, AzSvgFillStyle  fill_style);
        TessellatedSvgNode SvgNode_tessellateStroke(const SvgNode* svgnode, AzSvgStrokeStyle  stroke_style);
        bool  SvgNode_isClosed(const SvgNode* svgnode);
        bool  SvgNode_containsPoint(const SvgNode* svgnode, AzSvgPoint  point, AzSvgFillRule  fill_rule, float tolerance);
        SvgRect SvgNode_getBounds(const SvgNode* svgnode);
        void SvgNode_delete(SvgNode* restrict instance);
        SvgRect SvgSimpleNode_getBounds(const SvgSimpleNode* svgsimplenode);
        void SvgSimpleNode_delete(SvgSimpleNode* restrict instance);
        TessellatedSvgNode SvgStyledNode_tessellate(const SvgStyledNode* svgstylednode);
        void SvgStyledNode_delete(SvgStyledNode* restrict instance);
        TessellatedSvgNode SvgCircle_tessellateFill(const SvgCircle* svgcircle, AzSvgFillStyle  fill_style);
        TessellatedSvgNode SvgCircle_tessellateStroke(const SvgCircle* svgcircle, AzSvgStrokeStyle  stroke_style);
        bool  SvgPath_isClosed(const SvgPath* svgpath);
        void SvgPath_reverse(SvgPath* restrict svgpath);
        void SvgPath_joinWith(SvgPath* restrict svgpath, AzSvgPath  path);
        SvgPath SvgPath_offset(SvgPath* restrict svgpath, float distance, AzSvgLineJoin  join, AzSvgLineCap  cap);
        SvgPath SvgPath_bevel(SvgPath* restrict svgpath, float distance);
        TessellatedSvgNode SvgPath_tessellateFill(const SvgPath* svgpath, AzSvgFillStyle  fill_style);
        TessellatedSvgNode SvgPath_tessellateStroke(const SvgPath* svgpath, AzSvgStrokeStyle  stroke_style);
        void SvgPath_delete(SvgPath* restrict instance);
        void SvgPathElement_reverse(SvgPathElement* restrict svgpathelement);
        SvgPoint SvgPathElement_getStart(const SvgPathElement* svgpathelement);
        SvgPoint SvgPathElement_getEnd(const SvgPathElement* svgpathelement);
        SvgRect SvgPathElement_getBounds(const SvgPathElement* svgpathelement);
        double SvgPathElement_getLength(const SvgPathElement* svgpathelement);
        double SvgPathElement_getTAtOffset(const SvgPathElement* svgpathelement, double offset);
        double SvgPathElement_getXAtT(const SvgPathElement* svgpathelement, double t);
        double SvgPathElement_getYAtT(const SvgPathElement* svgpathelement, double t);
        SvgVector SvgPathElement_getTangentVectorAtT(const SvgPathElement* svgpathelement, double t);
        TessellatedSvgNode SvgPathElement_tessellateStroke(const SvgPathElement* svgpathelement, AzSvgStrokeStyle  stroke_style);
        double SvgPoint_distance(const SvgPoint* svgpoint, AzSvgPoint  other);
        double SvgVector_angleDegrees(const SvgVector* svgvector);
        SvgVector SvgVector_normalize(const SvgVector* svgvector);
        SvgVector SvgVector_rotate90DegCcw(const SvgVector* svgvector);
        void SvgLine_reverse(SvgLine* restrict svgline);
        SvgPoint SvgLine_getStart(const SvgLine* svgline);
        SvgPoint SvgLine_getEnd(const SvgLine* svgline);
        SvgRect SvgLine_getBounds(const SvgLine* svgline);
        double SvgLine_getLength(const SvgLine* svgline);
        double SvgLine_getTAtOffset(const SvgLine* svgline, double offset);
        double SvgLine_getXAtT(const SvgLine* svgline, double t);
        double SvgLine_getYAtT(const SvgLine* svgline, double t);
        SvgVector SvgLine_getTangentVectorAtT(const SvgLine* svgline, double t);
        OptionSvgPoint SvgLine_intersect(const SvgLine* svgline, AzSvgLine  other);
        TessellatedSvgNode SvgLine_tessellateStroke(const SvgLine* svgline, AzSvgStrokeStyle  stroke_style);
        void SvgQuadraticCurve_reverse(SvgQuadraticCurve* restrict svgquadraticcurve);
        SvgPoint SvgQuadraticCurve_getStart(const SvgQuadraticCurve* svgquadraticcurve);
        SvgPoint SvgQuadraticCurve_getEnd(const SvgQuadraticCurve* svgquadraticcurve);
        SvgRect SvgQuadraticCurve_getBounds(const SvgQuadraticCurve* svgquadraticcurve);
        double SvgQuadraticCurve_getLength(const SvgQuadraticCurve* svgquadraticcurve);
        double SvgQuadraticCurve_getTAtOffset(const SvgQuadraticCurve* svgquadraticcurve, double offset);
        double SvgQuadraticCurve_getXAtT(const SvgQuadraticCurve* svgquadraticcurve, double t);
        double SvgQuadraticCurve_getYAtT(const SvgQuadraticCurve* svgquadraticcurve, double t);
        SvgVector SvgQuadraticCurve_getTangentVectorAtT(const SvgQuadraticCurve* svgquadraticcurve, double t);
        TessellatedSvgNode SvgQuadraticCurve_tessellateStroke(const SvgQuadraticCurve* svgquadraticcurve, AzSvgStrokeStyle  stroke_style);
        void SvgCubicCurve_reverse(SvgCubicCurve* restrict svgcubiccurve);
        SvgPoint SvgCubicCurve_getStart(const SvgCubicCurve* svgcubiccurve);
        SvgPoint SvgCubicCurve_getEnd(const SvgCubicCurve* svgcubiccurve);
        SvgRect SvgCubicCurve_getBounds(const SvgCubicCurve* svgcubiccurve);
        double SvgCubicCurve_getLength(const SvgCubicCurve* svgcubiccurve);
        double SvgCubicCurve_getTAtOffset(const SvgCubicCurve* svgcubiccurve, double offset);
        double SvgCubicCurve_getXAtT(const SvgCubicCurve* svgcubiccurve, double t);
        double SvgCubicCurve_getYAtT(const SvgCubicCurve* svgcubiccurve, double t);
        SvgVector SvgCubicCurve_getTangentVectorAtT(const SvgCubicCurve* svgcubiccurve, double t);
        TessellatedSvgNode SvgCubicCurve_tessellateStroke(const SvgCubicCurve* svgcubiccurve, AzSvgStrokeStyle  stroke_style);
        SvgPoint SvgRect_getCenter(const SvgRect* svgrect);
        bool  SvgRect_containsPoint(const SvgRect* svgrect, AzSvgPoint  point);
        TessellatedSvgNode SvgRect_tessellateFill(const SvgRect* svgrect, AzSvgFillStyle  fill_style);
        TessellatedSvgNode SvgRect_tessellateStroke(const SvgRect* svgrect, AzSvgStrokeStyle  stroke_style);
        TessellatedSvgNode TessellatedSvgNode_empty();
        TessellatedSvgNode TessellatedSvgNode_fromNodes(AzTessellatedSvgNodeVecRef  nodes);
        void TessellatedSvgNode_delete(TessellatedSvgNode* restrict instance);
        void TessellatedSvgNodeVecRef_delete(TessellatedSvgNodeVecRef* restrict instance);
        TessellatedGPUSvgNode TessellatedGPUSvgNode_new(AzTessellatedSvgNode * tessellated_node, AzGl  gl);
        void TessellatedGPUSvgNode_delete(TessellatedGPUSvgNode* restrict instance);
        SvgParseOptions SvgParseOptions_default();
        void SvgParseOptions_delete(SvgParseOptions* restrict instance);
        SvgRenderOptions SvgRenderOptions_default();
        SvgFillStyle SvgFillStyle_default();
        SvgStrokeStyle SvgStrokeStyle_default();
        Xml Xml_fromStr(AzRefstr  xml_string);
        void Xml_delete(Xml* restrict instance);
        void XmlNode_delete(XmlNode* restrict instance);
        File File_open(AzString  path);
        File File_create(AzString  path);
        OptionString File_readToString(File* restrict file);
        OptionU8Vec File_readToBytes(File* restrict file);
        bool  File_writeString(File* restrict file, AzRefstr  bytes);
        bool  File_writeBytes(File* restrict file, AzU8VecRef  bytes);
        void File_close(File* restrict file);
        void File_delete(File* restrict instance);
        File File_deepCopy(File* const instance);
        MsgBox MsgBox_ok(AzMsgBoxIcon  icon, AzString  title, AzString  message);
        MsgBox MsgBox_info(AzString  message);
        MsgBox MsgBox_warning(AzString  message);
        MsgBox MsgBox_error(AzString  message);
        MsgBox MsgBox_question(AzString  message);
        MsgBox MsgBox_okCancel(AzMsgBoxIcon  icon, AzString  title, AzString  message, AzMsgBoxOkCancel  default_value);
        MsgBox MsgBox_yesNo(AzMsgBoxIcon  icon, AzString  title, AzString  message, AzMsgBoxYesNo  default_value);
        FileDialog FileDialog_selectFile(AzString  title, AzOptionString  default_path, AzOptionFileTypeList  filter_list);
        FileDialog FileDialog_selectMultipleFiles(AzString  title, AzOptionString  default_path, AzOptionFileTypeList  filter_list);
        FileDialog FileDialog_selectFolder(AzString  title, AzOptionString  default_path);
        FileDialog FileDialog_saveFile(AzString  title, AzOptionString  default_path);
        void FileTypeList_delete(FileTypeList* restrict instance);
        ColorPickerDialog ColorPickerDialog_open(AzString  title, AzOptionColorU  default_color);
        SystemClipboard SystemClipboard_new();
        OptionString SystemClipboard_getStringContents(const SystemClipboard* systemclipboard);
        bool  SystemClipboard_setStringContents(SystemClipboard* restrict systemclipboard, AzString  contents);
        void SystemClipboard_delete(SystemClipboard* restrict instance);
        SystemClipboard SystemClipboard_deepCopy(SystemClipboard* const instance);
        OptionDuration Instant_durationSince(const Instant* instant, AzInstant  earlier);
        Instant Instant_addDuration(Instant* restrict instant, AzDuration  duration);
        float Instant_linearInterpolate(const Instant* instant, AzInstant  start, AzInstant  end);
        void Instant_delete(Instant* restrict instance);
        void InstantPtr_delete(InstantPtr* restrict instance);
        InstantPtr InstantPtr_deepCopy(InstantPtr* const instance);
        Timer Timer_new(AzRefAny  timer_data, AzTimerCallbackType  callback, AzGetSystemTimeFn  get_system_time_fn);
        Timer Timer_withDelay(const Timer* timer, AzDuration  delay);
        Timer Timer_withInterval(const Timer* timer, AzDuration  interval);
        Timer Timer_withTimeout(const Timer* timer, AzDuration  timeout);
        void Timer_delete(Timer* restrict instance);
        void Thread_delete(Thread* restrict instance);
        Thread Thread_deepCopy(Thread* const instance);
        bool  ThreadSender_send(ThreadSender* restrict threadsender, AzThreadReceiveMsg  msg);
        void ThreadSender_delete(ThreadSender* restrict instance);
        ThreadSender ThreadSender_deepCopy(ThreadSender* const instance);
        OptionThreadSendMsg ThreadReceiver_receive(ThreadReceiver* restrict threadreceiver);
        void ThreadReceiver_delete(ThreadReceiver* restrict instance);
        ThreadReceiver ThreadReceiver_deepCopy(ThreadReceiver* const instance);
        void ThreadSendMsg_delete(ThreadSendMsg* restrict instance);
        void ThreadReceiveMsg_delete(ThreadReceiveMsg* restrict instance);
        void ThreadWriteBackMsg_delete(ThreadWriteBackMsg* restrict instance);
        void FmtValue_delete(FmtValue* restrict instance);
        void FmtArg_delete(FmtArg* restrict instance);
        String String_format(AzString  format, AzFmtArgVec  args);
        String String_copyFromBytes(uint8_t ptr, size_t start, size_t len);
        String String_trim(const String* string);
        Refstr String_asRefstr(const String* string);
        void String_delete(String* restrict instance);
        void ListViewRowVec_delete(ListViewRowVec* restrict instance);
        void StyleFilterVec_delete(StyleFilterVec* restrict instance);
        void LogicalRectVec_delete(LogicalRectVec* restrict instance);
        void NodeTypeIdInfoMapVec_delete(NodeTypeIdInfoMapVec* restrict instance);
        void InputOutputTypeIdInfoMapVec_delete(InputOutputTypeIdInfoMapVec* restrict instance);
        void NodeIdNodeMapVec_delete(NodeIdNodeMapVec* restrict instance);
        void InputOutputTypeIdVec_delete(InputOutputTypeIdVec* restrict instance);
        void NodeTypeFieldVec_delete(NodeTypeFieldVec* restrict instance);
        void InputConnectionVec_delete(InputConnectionVec* restrict instance);
        void OutputNodeAndIndexVec_delete(OutputNodeAndIndexVec* restrict instance);
        void OutputConnectionVec_delete(OutputConnectionVec* restrict instance);
        void InputNodeAndIndexVec_delete(InputNodeAndIndexVec* restrict instance);
        void AccessibilityStateVec_delete(AccessibilityStateVec* restrict instance);
        void MenuItemVec_delete(MenuItemVec* restrict instance);
        TessellatedSvgNodeVecRef TessellatedSvgNodeVec_asRefVec(const TessellatedSvgNodeVec* tessellatedsvgnodevec);
        void TessellatedSvgNodeVec_delete(TessellatedSvgNodeVec* restrict instance);
        void StyleFontFamilyVec_delete(StyleFontFamilyVec* restrict instance);
        void XmlNodeVec_delete(XmlNodeVec* restrict instance);
        void FmtArgVec_delete(FmtArgVec* restrict instance);
        void InlineLineVec_delete(InlineLineVec* restrict instance);
        void InlineWordVec_delete(InlineWordVec* restrict instance);
        void InlineGlyphVec_delete(InlineGlyphVec* restrict instance);
        void InlineTextHitVec_delete(InlineTextHitVec* restrict instance);
        void MonitorVec_delete(MonitorVec* restrict instance);
        void VideoModeVec_delete(VideoModeVec* restrict instance);
        void DomVec_delete(DomVec* restrict instance);
        void IdOrClassVec_delete(IdOrClassVec* restrict instance);
        void NodeDataInlineCssPropertyVec_delete(NodeDataInlineCssPropertyVec* restrict instance);
        void StyleBackgroundContentVec_delete(StyleBackgroundContentVec* restrict instance);
        void StyleBackgroundPositionVec_delete(StyleBackgroundPositionVec* restrict instance);
        void StyleBackgroundRepeatVec_delete(StyleBackgroundRepeatVec* restrict instance);
        void StyleBackgroundSizeVec_delete(StyleBackgroundSizeVec* restrict instance);
        void StyleTransformVec_delete(StyleTransformVec* restrict instance);
        void CssPropertyVec_delete(CssPropertyVec* restrict instance);
        void SvgMultiPolygonVec_delete(SvgMultiPolygonVec* restrict instance);
        void SvgSimpleNodeVec_delete(SvgSimpleNodeVec* restrict instance);
        void SvgPathVec_delete(SvgPathVec* restrict instance);
        void VertexAttributeVec_delete(VertexAttributeVec* restrict instance);
        void SvgPathElementVec_delete(SvgPathElementVec* restrict instance);
        void SvgVertexVec_delete(SvgVertexVec* restrict instance);
        void U32Vec_delete(U32Vec* restrict instance);
        void XWindowTypeVec_delete(XWindowTypeVec* restrict instance);
        void VirtualKeyCodeVec_delete(VirtualKeyCodeVec* restrict instance);
        void CascadeInfoVec_delete(CascadeInfoVec* restrict instance);
        void ScanCodeVec_delete(ScanCodeVec* restrict instance);
        void CssDeclarationVec_delete(CssDeclarationVec* restrict instance);
        void CssPathSelectorVec_delete(CssPathSelectorVec* restrict instance);
        void StylesheetVec_delete(StylesheetVec* restrict instance);
        void CssRuleBlockVec_delete(CssRuleBlockVec* restrict instance);
        void U16Vec_delete(U16Vec* restrict instance);
        void F32Vec_delete(F32Vec* restrict instance);
        U8Vec U8Vec_copyFromBytes(uint8_t ptr, size_t start, size_t len);
        U8VecRef U8Vec_asRefVec(const U8Vec* u8vec);
        void U8Vec_delete(U8Vec* restrict instance);
        void CallbackDataVec_delete(CallbackDataVec* restrict instance);
        void DebugMessageVec_delete(DebugMessageVec* restrict instance);
        void GLuintVec_delete(GLuintVec* restrict instance);
        void GLintVec_delete(GLintVec* restrict instance);
        void StringVec_delete(StringVec* restrict instance);
        void StringPairVec_delete(StringPairVec* restrict instance);
        void NormalizedLinearColorStopVec_delete(NormalizedLinearColorStopVec* restrict instance);
        void NormalizedRadialColorStopVec_delete(NormalizedRadialColorStopVec* restrict instance);
        void NodeIdVec_delete(NodeIdVec* restrict instance);
        void NodeHierarchyItemVec_delete(NodeHierarchyItemVec* restrict instance);
        void StyledNodeVec_delete(StyledNodeVec* restrict instance);
        void TagIdToNodeIdMappingVec_delete(TagIdToNodeIdMappingVec* restrict instance);
        void ParentWithNodeDepthVec_delete(ParentWithNodeDepthVec* restrict instance);
        void NodeDataVec_delete(NodeDataVec* restrict instance);
        void OptionListViewOnRowClick_delete(OptionListViewOnRowClick* restrict instance);
        void OptionListViewOnColumnClick_delete(OptionListViewOnColumnClick* restrict instance);
        void OptionListViewOnLazyLoadScroll_delete(OptionListViewOnLazyLoadScroll* restrict instance);
        void OptionMenu_delete(OptionMenu* restrict instance);
        void OptionDropDownOnChoiceChange_delete(OptionDropDownOnChoiceChange* restrict instance);
        void OptionResolvedTextLayoutOptions_delete(OptionResolvedTextLayoutOptions* restrict instance);
        void OptionNodeGraphOnNodeAdded_delete(OptionNodeGraphOnNodeAdded* restrict instance);
        void OptionNodeGraphOnNodeRemoved_delete(OptionNodeGraphOnNodeRemoved* restrict instance);
        void OptionNodeGraphOnNodeGraphDragged_delete(OptionNodeGraphOnNodeGraphDragged* restrict instance);
        void OptionNodeGraphOnNodeDragged_delete(OptionNodeGraphOnNodeDragged* restrict instance);
        void OptionNodeGraphOnNodeConnected_delete(OptionNodeGraphOnNodeConnected* restrict instance);
        void OptionNodeGraphOnNodeInputDisconnected_delete(OptionNodeGraphOnNodeInputDisconnected* restrict instance);
        void OptionNodeGraphOnNodeOutputDisconnected_delete(OptionNodeGraphOnNodeOutputDisconnected* restrict instance);
        void OptionNodeGraphOnNodeFieldEdited_delete(OptionNodeGraphOnNodeFieldEdited* restrict instance);
        void OptionColorInputOnValueChange_delete(OptionColorInputOnValueChange* restrict instance);
        void OptionButtonOnClick_delete(OptionButtonOnClick* restrict instance);
        void OptionTabOnClick_delete(OptionTabOnClick* restrict instance);
        void OptionFileInputOnPathChange_delete(OptionFileInputOnPathChange* restrict instance);
        void OptionCheckBoxOnToggle_delete(OptionCheckBoxOnToggle* restrict instance);
        void OptionTextInputOnTextInput_delete(OptionTextInputOnTextInput* restrict instance);
        void OptionTextInputOnVirtualKeyDown_delete(OptionTextInputOnVirtualKeyDown* restrict instance);
        void OptionTextInputOnFocusLost_delete(OptionTextInputOnFocusLost* restrict instance);
        void OptionNumberInputOnFocusLost_delete(OptionNumberInputOnFocusLost* restrict instance);
        void OptionNumberInputOnValueChange_delete(OptionNumberInputOnValueChange* restrict instance);
        void OptionMenuItemIcon_delete(OptionMenuItemIcon* restrict instance);
        void OptionMenuCallback_delete(OptionMenuCallback* restrict instance);
        void OptionVirtualKeyCodeCombo_delete(OptionVirtualKeyCodeCombo* restrict instance);
        void OptionCssProperty_delete(OptionCssProperty* restrict instance);
        void OptionImageRef_delete(OptionImageRef* restrict instance);
        void OptionFontRef_delete(OptionFontRef* restrict instance);
        void OptionSystemClipboard_delete(OptionSystemClipboard* restrict instance);
        void OptionFileTypeList_delete(OptionFileTypeList* restrict instance);
        void OptionWindowState_delete(OptionWindowState* restrict instance);
        void OptionKeyboardState_delete(OptionKeyboardState* restrict instance);
        void OptionStringVec_delete(OptionStringVec* restrict instance);
        void OptionFile_delete(OptionFile* restrict instance);
        void OptionGl_delete(OptionGl* restrict instance);
        void OptionThreadReceiveMsg_delete(OptionThreadReceiveMsg* restrict instance);
        void OptionThreadSendMsg_delete(OptionThreadSendMsg* restrict instance);
        void OptionRefAny_delete(OptionRefAny* restrict instance);
        void OptionInlineText_delete(OptionInlineText* restrict instance);
        void OptionRawImage_delete(OptionRawImage* restrict instance);
        void OptionWaylandTheme_delete(OptionWaylandTheme* restrict instance);
        void OptionTaskBarIcon_delete(OptionTaskBarIcon* restrict instance);
        void OptionWindowIcon_delete(OptionWindowIcon* restrict instance);
        void OptionString_delete(OptionString* restrict instance);
        void OptionDom_delete(OptionDom* restrict instance);
        void OptionTexture_delete(OptionTexture* restrict instance);
        void OptionImageMask_delete(OptionImageMask* restrict instance);
        void OptionInstant_delete(OptionInstant* restrict instance);
        void OptionU8Vec_delete(OptionU8Vec* restrict instance);
        void ResultXmlXmlError_delete(ResultXmlXmlError* restrict instance);
        void ResultRawImageDecodeImageError_delete(ResultRawImageDecodeImageError* restrict instance);
        void ResultU8VecEncodeImageError_delete(ResultU8VecEncodeImageError* restrict instance);
        void ResultSvgXmlNodeSvgParseError_delete(ResultSvgXmlNodeSvgParseError* restrict instance);
        void ResultSvgSvgParseError_delete(ResultSvgSvgParseError* restrict instance);
        void SvgParseError_delete(SvgParseError* restrict instance);
        void XmlError_delete(XmlError* restrict instance);
        void DuplicatedNamespaceError_delete(DuplicatedNamespaceError* restrict instance);
        void UnknownNamespaceError_delete(UnknownNamespaceError* restrict instance);
        void UnexpectedCloseTagError_delete(UnexpectedCloseTagError* restrict instance);
        void UnknownEntityReferenceError_delete(UnknownEntityReferenceError* restrict instance);
        void DuplicatedAttributeError_delete(DuplicatedAttributeError* restrict instance);
        void XmlParseError_delete(XmlParseError* restrict instance);
        void XmlTextError_delete(XmlTextError* restrict instance);
        void XmlStreamError_delete(XmlStreamError* restrict instance);
        void InvalidCharMultipleError_delete(InvalidCharMultipleError* restrict instance);
        void InvalidStringError_delete(InvalidStringError* restrict instance);

    } /* extern "C" */

} /* namespace */ 


#endif /* AZUL_H */
