#ifndef AZUL_H
#define AZUL_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

// ssize_t and size_t have the same size
// but ssize_t is signed
#define ssize_t size_t

struct AzRefAny;
struct AzLayoutInfo;
struct AzStyledDom;
typedef AzStyledDom (*AzLayoutCallbackType)(AzRefAny* restrict, AzLayoutInfo);

struct AzCallbackInfo;
struct AzUpdateScreen;
typedef AzUpdateScreen (*AzCallbackType)(AzRefAny* restrict, AzCallbackInfo);

struct AzIFrameCallbackInfo;
struct AzIFrameCallbackReturn;
typedef AzIFrameCallbackReturn (*AzIFrameCallbackType)(AzRefAny* restrict, AzIFrameCallbackInfo);

struct AzGlCallbackInfo;
struct AzGlCallbackReturn;
typedef AzGlCallbackReturn (*AzGlCallbackType)(AzRefAny* restrict, AzGlCallbackInfo);

struct AzTimerCallbackInfo;
struct AzTimerCallbackReturn;
typedef AzTimerCallbackReturn (*AzTimerCallbackType)(AzRefAny* restrict, AzRefAny* restrict, AzTimerCallbackInfo);

typedef AzUpdateScreen (*AzWriteBackCallbackType)(AzRefAny* restrict, AzRefAny, AzCallbackInfo);

struct AzThreadSender;
struct AzThreadReceiver;
typedef void (*AzThreadCallbackType)(AzRefAny, AzThreadSender, AzThreadReceiver);

typedef void (*AzRefAnyDestructorType)(void* restrict);

struct AzThreadCallbackType;
struct AzThread;
typedef AzThread (*AzCreateThreadFnType)(AzRefAny, AzRefAny, AzThreadCallbackType);

struct AzInstant;
typedef AzInstant (*AzGetSystemTimeFnType)();

typedef bool (*AzCheckThreadFinishedFnType)(void* const);

struct AzThreadSendMsg;
typedef bool (*AzLibrarySendThreadMsgFnType)(void* restrict, AzThreadSendMsg);

struct AzOptionThreadReceiveMsg;
typedef AzOptionThreadReceiveMsg (*AzLibraryReceiveThreadMsgFnType)(void* restrict);

struct AzOptionThreadSendMsg;
typedef AzOptionThreadSendMsg (*AzThreadRecvFnType)(void* restrict);

struct AzThreadReceiveMsg;
typedef bool (*AzThreadSendFnType)(void* restrict, AzThreadReceiveMsg);

typedef void (*AzThreadDestructorFnType)(void* restrict, void* restrict, void* restrict, void* restrict);

typedef void (*AzThreadReceiverDestructorFnType)(AzThreadReceiver* restrict);

typedef void (*AzThreadSenderDestructorFnType)(AzThreadSender* restrict);

struct AzMonitorVec;
typedef void (*AzMonitorVecDestructorType)(AzMonitorVec* restrict);

struct AzVideoModeVec;
typedef void (*AzVideoModeVecDestructorType)(AzVideoModeVec* restrict);

struct AzDomVec;
typedef void (*AzDomVecDestructorType)(AzDomVec* restrict);

struct AzIdOrClassVec;
typedef void (*AzIdOrClassVecDestructorType)(AzIdOrClassVec* restrict);

struct AzNodeDataInlineCssPropertyVec;
typedef void (*AzNodeDataInlineCssPropertyVecDestructorType)(AzNodeDataInlineCssPropertyVec* restrict);

struct AzStyleBackgroundContentVec;
typedef void (*AzStyleBackgroundContentVecDestructorType)(AzStyleBackgroundContentVec* restrict);

struct AzStyleBackgroundPositionVec;
typedef void (*AzStyleBackgroundPositionVecDestructorType)(AzStyleBackgroundPositionVec* restrict);

struct AzStyleBackgroundRepeatVec;
typedef void (*AzStyleBackgroundRepeatVecDestructorType)(AzStyleBackgroundRepeatVec* restrict);

struct AzStyleBackgroundSizeVec;
typedef void (*AzStyleBackgroundSizeVecDestructorType)(AzStyleBackgroundSizeVec* restrict);

struct AzStyleTransformVec;
typedef void (*AzStyleTransformVecDestructorType)(AzStyleTransformVec* restrict);

struct AzCssPropertyVec;
typedef void (*AzCssPropertyVecDestructorType)(AzCssPropertyVec* restrict);

struct AzSvgMultiPolygonVec;
typedef void (*AzSvgMultiPolygonVecDestructorType)(AzSvgMultiPolygonVec* restrict);

struct AzSvgPathVec;
typedef void (*AzSvgPathVecDestructorType)(AzSvgPathVec* restrict);

struct AzVertexAttributeVec;
typedef void (*AzVertexAttributeVecDestructorType)(AzVertexAttributeVec* restrict);

struct AzSvgPathElementVec;
typedef void (*AzSvgPathElementVecDestructorType)(AzSvgPathElementVec* restrict);

struct AzSvgVertexVec;
typedef void (*AzSvgVertexVecDestructorType)(AzSvgVertexVec* restrict);

struct AzU32Vec;
typedef void (*AzU32VecDestructorType)(AzU32Vec* restrict);

struct AzXWindowTypeVec;
typedef void (*AzXWindowTypeVecDestructorType)(AzXWindowTypeVec* restrict);

struct AzVirtualKeyCodeVec;
typedef void (*AzVirtualKeyCodeVecDestructorType)(AzVirtualKeyCodeVec* restrict);

struct AzCascadeInfoVec;
typedef void (*AzCascadeInfoVecDestructorType)(AzCascadeInfoVec* restrict);

struct AzScanCodeVec;
typedef void (*AzScanCodeVecDestructorType)(AzScanCodeVec* restrict);

struct AzCssDeclarationVec;
typedef void (*AzCssDeclarationVecDestructorType)(AzCssDeclarationVec* restrict);

struct AzCssPathSelectorVec;
typedef void (*AzCssPathSelectorVecDestructorType)(AzCssPathSelectorVec* restrict);

struct AzStylesheetVec;
typedef void (*AzStylesheetVecDestructorType)(AzStylesheetVec* restrict);

struct AzCssRuleBlockVec;
typedef void (*AzCssRuleBlockVecDestructorType)(AzCssRuleBlockVec* restrict);

struct AzU8Vec;
typedef void (*AzU8VecDestructorType)(AzU8Vec* restrict);

struct AzCallbackDataVec;
typedef void (*AzCallbackDataVecDestructorType)(AzCallbackDataVec* restrict);

struct AzDebugMessageVec;
typedef void (*AzDebugMessageVecDestructorType)(AzDebugMessageVec* restrict);

struct AzGLuintVec;
typedef void (*AzGLuintVecDestructorType)(AzGLuintVec* restrict);

struct AzGLintVec;
typedef void (*AzGLintVecDestructorType)(AzGLintVec* restrict);

struct AzStringVec;
typedef void (*AzStringVecDestructorType)(AzStringVec* restrict);

struct AzStringPairVec;
typedef void (*AzStringPairVecDestructorType)(AzStringPairVec* restrict);

struct AzLinearColorStopVec;
typedef void (*AzLinearColorStopVecDestructorType)(AzLinearColorStopVec* restrict);

struct AzRadialColorStopVec;
typedef void (*AzRadialColorStopVecDestructorType)(AzRadialColorStopVec* restrict);

struct AzNodeIdVec;
typedef void (*AzNodeIdVecDestructorType)(AzNodeIdVec* restrict);

struct AzNodeVec;
typedef void (*AzNodeVecDestructorType)(AzNodeVec* restrict);

struct AzStyledNodeVec;
typedef void (*AzStyledNodeVecDestructorType)(AzStyledNodeVec* restrict);

struct AzTagIdsToNodeIdsMappingVec;
typedef void (*AzTagIdsToNodeIdsMappingVecDestructorType)(AzTagIdsToNodeIdsMappingVec* restrict);

struct AzParentWithNodeDepthVec;
typedef void (*AzParentWithNodeDepthVecDestructorType)(AzParentWithNodeDepthVec* restrict);

struct AzNodeDataVec;
typedef void (*AzNodeDataVecDestructorType)(AzNodeDataVec* restrict);

struct AzInstantPtr;
typedef AzInstantPtr (*AzInstantPtrCloneFnType)(void* const);

typedef void (*AzInstantPtrDestructorFnType)(void* restrict);


typedef struct {
    void* const ptr;
} AzApp;

typedef enum {
   AzAppLogLevel_Off,
   AzAppLogLevel_Error,
   AzAppLogLevel_Warn,
   AzAppLogLevel_Info,
   AzAppLogLevel_Debug,
   AzAppLogLevel_Trace,
} AzAppLogLevel;

typedef enum {
   AzVsync_Enabled,
   AzVsync_Disabled,
} AzVsync;

typedef enum {
   AzSrgb_Enabled,
   AzSrgb_Disabled,
} AzSrgb;

typedef enum {
   AzHwAcceleration_Enabled,
   AzHwAcceleration_Disabled,
} AzHwAcceleration;

typedef enum {
   AzXWindowType_Desktop,
   AzXWindowType_Dock,
   AzXWindowType_Toolbar,
   AzXWindowType_Menu,
   AzXWindowType_Utility,
   AzXWindowType_Splash,
   AzXWindowType_Dialog,
   AzXWindowType_DropdownMenu,
   AzXWindowType_PopupMenu,
   AzXWindowType_Tooltip,
   AzXWindowType_Notification,
   AzXWindowType_Combo,
   AzXWindowType_Dnd,
   AzXWindowType_Normal,
} AzXWindowType;

typedef enum {
   AzVirtualKeyCode_Key1,
   AzVirtualKeyCode_Key2,
   AzVirtualKeyCode_Key3,
   AzVirtualKeyCode_Key4,
   AzVirtualKeyCode_Key5,
   AzVirtualKeyCode_Key6,
   AzVirtualKeyCode_Key7,
   AzVirtualKeyCode_Key8,
   AzVirtualKeyCode_Key9,
   AzVirtualKeyCode_Key0,
   AzVirtualKeyCode_A,
   AzVirtualKeyCode_B,
   AzVirtualKeyCode_C,
   AzVirtualKeyCode_D,
   AzVirtualKeyCode_E,
   AzVirtualKeyCode_F,
   AzVirtualKeyCode_G,
   AzVirtualKeyCode_H,
   AzVirtualKeyCode_I,
   AzVirtualKeyCode_J,
   AzVirtualKeyCode_K,
   AzVirtualKeyCode_L,
   AzVirtualKeyCode_M,
   AzVirtualKeyCode_N,
   AzVirtualKeyCode_O,
   AzVirtualKeyCode_P,
   AzVirtualKeyCode_Q,
   AzVirtualKeyCode_R,
   AzVirtualKeyCode_S,
   AzVirtualKeyCode_T,
   AzVirtualKeyCode_U,
   AzVirtualKeyCode_V,
   AzVirtualKeyCode_W,
   AzVirtualKeyCode_X,
   AzVirtualKeyCode_Y,
   AzVirtualKeyCode_Z,
   AzVirtualKeyCode_Escape,
   AzVirtualKeyCode_F1,
   AzVirtualKeyCode_F2,
   AzVirtualKeyCode_F3,
   AzVirtualKeyCode_F4,
   AzVirtualKeyCode_F5,
   AzVirtualKeyCode_F6,
   AzVirtualKeyCode_F7,
   AzVirtualKeyCode_F8,
   AzVirtualKeyCode_F9,
   AzVirtualKeyCode_F10,
   AzVirtualKeyCode_F11,
   AzVirtualKeyCode_F12,
   AzVirtualKeyCode_F13,
   AzVirtualKeyCode_F14,
   AzVirtualKeyCode_F15,
   AzVirtualKeyCode_F16,
   AzVirtualKeyCode_F17,
   AzVirtualKeyCode_F18,
   AzVirtualKeyCode_F19,
   AzVirtualKeyCode_F20,
   AzVirtualKeyCode_F21,
   AzVirtualKeyCode_F22,
   AzVirtualKeyCode_F23,
   AzVirtualKeyCode_F24,
   AzVirtualKeyCode_Snapshot,
   AzVirtualKeyCode_Scroll,
   AzVirtualKeyCode_Pause,
   AzVirtualKeyCode_Insert,
   AzVirtualKeyCode_Home,
   AzVirtualKeyCode_Delete,
   AzVirtualKeyCode_End,
   AzVirtualKeyCode_PageDown,
   AzVirtualKeyCode_PageUp,
   AzVirtualKeyCode_Left,
   AzVirtualKeyCode_Up,
   AzVirtualKeyCode_Right,
   AzVirtualKeyCode_Down,
   AzVirtualKeyCode_Back,
   AzVirtualKeyCode_Return,
   AzVirtualKeyCode_Space,
   AzVirtualKeyCode_Compose,
   AzVirtualKeyCode_Caret,
   AzVirtualKeyCode_Numlock,
   AzVirtualKeyCode_Numpad0,
   AzVirtualKeyCode_Numpad1,
   AzVirtualKeyCode_Numpad2,
   AzVirtualKeyCode_Numpad3,
   AzVirtualKeyCode_Numpad4,
   AzVirtualKeyCode_Numpad5,
   AzVirtualKeyCode_Numpad6,
   AzVirtualKeyCode_Numpad7,
   AzVirtualKeyCode_Numpad8,
   AzVirtualKeyCode_Numpad9,
   AzVirtualKeyCode_NumpadAdd,
   AzVirtualKeyCode_NumpadDivide,
   AzVirtualKeyCode_NumpadDecimal,
   AzVirtualKeyCode_NumpadComma,
   AzVirtualKeyCode_NumpadEnter,
   AzVirtualKeyCode_NumpadEquals,
   AzVirtualKeyCode_NumpadMultiply,
   AzVirtualKeyCode_NumpadSubtract,
   AzVirtualKeyCode_AbntC1,
   AzVirtualKeyCode_AbntC2,
   AzVirtualKeyCode_Apostrophe,
   AzVirtualKeyCode_Apps,
   AzVirtualKeyCode_Asterisk,
   AzVirtualKeyCode_At,
   AzVirtualKeyCode_Ax,
   AzVirtualKeyCode_Backslash,
   AzVirtualKeyCode_Calculator,
   AzVirtualKeyCode_Capital,
   AzVirtualKeyCode_Colon,
   AzVirtualKeyCode_Comma,
   AzVirtualKeyCode_Convert,
   AzVirtualKeyCode_Equals,
   AzVirtualKeyCode_Grave,
   AzVirtualKeyCode_Kana,
   AzVirtualKeyCode_Kanji,
   AzVirtualKeyCode_LAlt,
   AzVirtualKeyCode_LBracket,
   AzVirtualKeyCode_LControl,
   AzVirtualKeyCode_LShift,
   AzVirtualKeyCode_LWin,
   AzVirtualKeyCode_Mail,
   AzVirtualKeyCode_MediaSelect,
   AzVirtualKeyCode_MediaStop,
   AzVirtualKeyCode_Minus,
   AzVirtualKeyCode_Mute,
   AzVirtualKeyCode_MyComputer,
   AzVirtualKeyCode_NavigateForward,
   AzVirtualKeyCode_NavigateBackward,
   AzVirtualKeyCode_NextTrack,
   AzVirtualKeyCode_NoConvert,
   AzVirtualKeyCode_OEM102,
   AzVirtualKeyCode_Period,
   AzVirtualKeyCode_PlayPause,
   AzVirtualKeyCode_Plus,
   AzVirtualKeyCode_Power,
   AzVirtualKeyCode_PrevTrack,
   AzVirtualKeyCode_RAlt,
   AzVirtualKeyCode_RBracket,
   AzVirtualKeyCode_RControl,
   AzVirtualKeyCode_RShift,
   AzVirtualKeyCode_RWin,
   AzVirtualKeyCode_Semicolon,
   AzVirtualKeyCode_Slash,
   AzVirtualKeyCode_Sleep,
   AzVirtualKeyCode_Stop,
   AzVirtualKeyCode_Sysrq,
   AzVirtualKeyCode_Tab,
   AzVirtualKeyCode_Underline,
   AzVirtualKeyCode_Unlabeled,
   AzVirtualKeyCode_VolumeDown,
   AzVirtualKeyCode_VolumeUp,
   AzVirtualKeyCode_Wake,
   AzVirtualKeyCode_WebBack,
   AzVirtualKeyCode_WebFavorites,
   AzVirtualKeyCode_WebForward,
   AzVirtualKeyCode_WebHome,
   AzVirtualKeyCode_WebRefresh,
   AzVirtualKeyCode_WebSearch,
   AzVirtualKeyCode_WebStop,
   AzVirtualKeyCode_Yen,
   AzVirtualKeyCode_Copy,
   AzVirtualKeyCode_Paste,
   AzVirtualKeyCode_Cut,
} AzVirtualKeyCode;

typedef enum {
   AzMouseCursorType_Default,
   AzMouseCursorType_Crosshair,
   AzMouseCursorType_Hand,
   AzMouseCursorType_Arrow,
   AzMouseCursorType_Move,
   AzMouseCursorType_Text,
   AzMouseCursorType_Wait,
   AzMouseCursorType_Help,
   AzMouseCursorType_Progress,
   AzMouseCursorType_NotAllowed,
   AzMouseCursorType_ContextMenu,
   AzMouseCursorType_Cell,
   AzMouseCursorType_VerticalText,
   AzMouseCursorType_Alias,
   AzMouseCursorType_Copy,
   AzMouseCursorType_NoDrop,
   AzMouseCursorType_Grab,
   AzMouseCursorType_Grabbing,
   AzMouseCursorType_AllScroll,
   AzMouseCursorType_ZoomIn,
   AzMouseCursorType_ZoomOut,
   AzMouseCursorType_EResize,
   AzMouseCursorType_NResize,
   AzMouseCursorType_NeResize,
   AzMouseCursorType_NwResize,
   AzMouseCursorType_SResize,
   AzMouseCursorType_SeResize,
   AzMouseCursorType_SwResize,
   AzMouseCursorType_WResize,
   AzMouseCursorType_EwResize,
   AzMouseCursorType_NsResize,
   AzMouseCursorType_NeswResize,
   AzMouseCursorType_NwseResize,
   AzMouseCursorType_ColResize,
   AzMouseCursorType_RowResize,
} AzMouseCursorType;

typedef enum {
   AzRendererType_Hardware,
   AzRendererType_Software,
} AzRendererType;

typedef enum {
   AzFullScreenMode_SlowFullScreen,
   AzFullScreenMode_FastFullScreen,
   AzFullScreenMode_SlowWindowed,
   AzFullScreenMode_FastWindowed,
} AzFullScreenMode;

typedef enum {
   AzWindowTheme_DarkMode,
   AzWindowTheme_LightMode,
} AzWindowTheme;

typedef struct {
    void* restrict ptr;
} AzMonitorHandle;

typedef enum {
   AzUpdateScreen_DoNothing,
   AzUpdateScreen_RegenerateStyledDomForCurrentWindow,
   AzUpdateScreen_RegenerateStyledDomForAllWindows,
} AzUpdateScreen;

typedef struct {
    AzRefCountInner* const ptr;
} AzRefCount;

typedef struct {
    void* const _internal_ptr;
    bool  is_dead;
    AzRefCount sharing_info;
} AzRefAny;

typedef enum {
   AzOn_MouseOver,
   AzOn_MouseDown,
   AzOn_LeftMouseDown,
   AzOn_MiddleMouseDown,
   AzOn_RightMouseDown,
   AzOn_MouseUp,
   AzOn_LeftMouseUp,
   AzOn_MiddleMouseUp,
   AzOn_RightMouseUp,
   AzOn_MouseEnter,
   AzOn_MouseLeave,
   AzOn_Scroll,
   AzOn_TextInput,
   AzOn_VirtualKeyDown,
   AzOn_VirtualKeyUp,
   AzOn_HoveredFile,
   AzOn_DroppedFile,
   AzOn_HoveredFileCancelled,
   AzOn_FocusReceived,
   AzOn_FocusLost,
} AzOn;

typedef enum {
   AzHoverEventFilter_MouseOver,
   AzHoverEventFilter_MouseDown,
   AzHoverEventFilter_LeftMouseDown,
   AzHoverEventFilter_RightMouseDown,
   AzHoverEventFilter_MiddleMouseDown,
   AzHoverEventFilter_MouseUp,
   AzHoverEventFilter_LeftMouseUp,
   AzHoverEventFilter_RightMouseUp,
   AzHoverEventFilter_MiddleMouseUp,
   AzHoverEventFilter_MouseEnter,
   AzHoverEventFilter_MouseLeave,
   AzHoverEventFilter_Scroll,
   AzHoverEventFilter_ScrollStart,
   AzHoverEventFilter_ScrollEnd,
   AzHoverEventFilter_TextInput,
   AzHoverEventFilter_VirtualKeyDown,
   AzHoverEventFilter_VirtualKeyUp,
   AzHoverEventFilter_HoveredFile,
   AzHoverEventFilter_DroppedFile,
   AzHoverEventFilter_HoveredFileCancelled,
   AzHoverEventFilter_TouchStart,
   AzHoverEventFilter_TouchMove,
   AzHoverEventFilter_TouchEnd,
   AzHoverEventFilter_TouchCancel,
} AzHoverEventFilter;

typedef enum {
   AzFocusEventFilter_MouseOver,
   AzFocusEventFilter_MouseDown,
   AzFocusEventFilter_LeftMouseDown,
   AzFocusEventFilter_RightMouseDown,
   AzFocusEventFilter_MiddleMouseDown,
   AzFocusEventFilter_MouseUp,
   AzFocusEventFilter_LeftMouseUp,
   AzFocusEventFilter_RightMouseUp,
   AzFocusEventFilter_MiddleMouseUp,
   AzFocusEventFilter_MouseEnter,
   AzFocusEventFilter_MouseLeave,
   AzFocusEventFilter_Scroll,
   AzFocusEventFilter_ScrollStart,
   AzFocusEventFilter_ScrollEnd,
   AzFocusEventFilter_TextInput,
   AzFocusEventFilter_VirtualKeyDown,
   AzFocusEventFilter_VirtualKeyUp,
   AzFocusEventFilter_FocusReceived,
   AzFocusEventFilter_FocusLost,
} AzFocusEventFilter;

typedef enum {
   AzWindowEventFilter_MouseOver,
   AzWindowEventFilter_MouseDown,
   AzWindowEventFilter_LeftMouseDown,
   AzWindowEventFilter_RightMouseDown,
   AzWindowEventFilter_MiddleMouseDown,
   AzWindowEventFilter_MouseUp,
   AzWindowEventFilter_LeftMouseUp,
   AzWindowEventFilter_RightMouseUp,
   AzWindowEventFilter_MiddleMouseUp,
   AzWindowEventFilter_MouseEnter,
   AzWindowEventFilter_MouseLeave,
   AzWindowEventFilter_Scroll,
   AzWindowEventFilter_ScrollStart,
   AzWindowEventFilter_ScrollEnd,
   AzWindowEventFilter_TextInput,
   AzWindowEventFilter_VirtualKeyDown,
   AzWindowEventFilter_VirtualKeyUp,
   AzWindowEventFilter_HoveredFile,
   AzWindowEventFilter_DroppedFile,
   AzWindowEventFilter_HoveredFileCancelled,
   AzWindowEventFilter_Resized,
   AzWindowEventFilter_Moved,
   AzWindowEventFilter_TouchStart,
   AzWindowEventFilter_TouchMove,
   AzWindowEventFilter_TouchEnd,
   AzWindowEventFilter_TouchCancel,
   AzWindowEventFilter_FocusReceived,
   AzWindowEventFilter_FocusLost,
   AzWindowEventFilter_CloseRequested,
   AzWindowEventFilter_ThemeChanged,
} AzWindowEventFilter;

typedef enum {
   AzComponentEventFilter_AfterMount,
   AzComponentEventFilter_BeforeUnmount,
   AzComponentEventFilter_NodeResized,
} AzComponentEventFilter;

typedef enum {
   AzApplicationEventFilter_DeviceConnected,
   AzApplicationEventFilter_DeviceDisconnected,
} AzApplicationEventFilter;

typedef enum {
   AzNodeTypePath_Body,
   AzNodeTypePath_Div,
   AzNodeTypePath_Br,
   AzNodeTypePath_P,
   AzNodeTypePath_Img,
   AzNodeTypePath_Texture,
   AzNodeTypePath_IFrame,
} AzNodeTypePath;

typedef enum {
   AzCssPropertyType_TextColor,
   AzCssPropertyType_FontSize,
   AzCssPropertyType_FontFamily,
   AzCssPropertyType_TextAlign,
   AzCssPropertyType_LetterSpacing,
   AzCssPropertyType_LineHeight,
   AzCssPropertyType_WordSpacing,
   AzCssPropertyType_TabWidth,
   AzCssPropertyType_Cursor,
   AzCssPropertyType_Display,
   AzCssPropertyType_Float,
   AzCssPropertyType_BoxSizing,
   AzCssPropertyType_Width,
   AzCssPropertyType_Height,
   AzCssPropertyType_MinWidth,
   AzCssPropertyType_MinHeight,
   AzCssPropertyType_MaxWidth,
   AzCssPropertyType_MaxHeight,
   AzCssPropertyType_Position,
   AzCssPropertyType_Top,
   AzCssPropertyType_Right,
   AzCssPropertyType_Left,
   AzCssPropertyType_Bottom,
   AzCssPropertyType_FlexWrap,
   AzCssPropertyType_FlexDirection,
   AzCssPropertyType_FlexGrow,
   AzCssPropertyType_FlexShrink,
   AzCssPropertyType_JustifyContent,
   AzCssPropertyType_AlignItems,
   AzCssPropertyType_AlignContent,
   AzCssPropertyType_OverflowX,
   AzCssPropertyType_OverflowY,
   AzCssPropertyType_PaddingTop,
   AzCssPropertyType_PaddingLeft,
   AzCssPropertyType_PaddingRight,
   AzCssPropertyType_PaddingBottom,
   AzCssPropertyType_MarginTop,
   AzCssPropertyType_MarginLeft,
   AzCssPropertyType_MarginRight,
   AzCssPropertyType_MarginBottom,
   AzCssPropertyType_Background,
   AzCssPropertyType_BackgroundImage,
   AzCssPropertyType_BackgroundColor,
   AzCssPropertyType_BackgroundPosition,
   AzCssPropertyType_BackgroundSize,
   AzCssPropertyType_BackgroundRepeat,
   AzCssPropertyType_BorderTopLeftRadius,
   AzCssPropertyType_BorderTopRightRadius,
   AzCssPropertyType_BorderBottomLeftRadius,
   AzCssPropertyType_BorderBottomRightRadius,
   AzCssPropertyType_BorderTopColor,
   AzCssPropertyType_BorderRightColor,
   AzCssPropertyType_BorderLeftColor,
   AzCssPropertyType_BorderBottomColor,
   AzCssPropertyType_BorderTopStyle,
   AzCssPropertyType_BorderRightStyle,
   AzCssPropertyType_BorderLeftStyle,
   AzCssPropertyType_BorderBottomStyle,
   AzCssPropertyType_BorderTopWidth,
   AzCssPropertyType_BorderRightWidth,
   AzCssPropertyType_BorderLeftWidth,
   AzCssPropertyType_BorderBottomWidth,
   AzCssPropertyType_BoxShadowLeft,
   AzCssPropertyType_BoxShadowRight,
   AzCssPropertyType_BoxShadowTop,
   AzCssPropertyType_BoxShadowBottom,
   AzCssPropertyType_ScrollbarStyle,
   AzCssPropertyType_Opacity,
   AzCssPropertyType_Transform,
   AzCssPropertyType_PerspectiveOrigin,
   AzCssPropertyType_TransformOrigin,
   AzCssPropertyType_BackfaceVisibility,
} AzCssPropertyType;

typedef enum {
   AzSizeMetric_Px,
   AzSizeMetric_Pt,
   AzSizeMetric_Em,
   AzSizeMetric_Percent,
} AzSizeMetric;

typedef enum {
   AzBoxShadowClipMode_Outset,
   AzBoxShadowClipMode_Inset,
} AzBoxShadowClipMode;

typedef enum {
   AzLayoutAlignContent_Stretch,
   AzLayoutAlignContent_Center,
   AzLayoutAlignContent_Start,
   AzLayoutAlignContent_End,
   AzLayoutAlignContent_SpaceBetween,
   AzLayoutAlignContent_SpaceAround,
} AzLayoutAlignContent;

typedef enum {
   AzLayoutAlignItems_Stretch,
   AzLayoutAlignItems_Center,
   AzLayoutAlignItems_FlexStart,
   AzLayoutAlignItems_FlexEnd,
} AzLayoutAlignItems;

typedef enum {
   AzLayoutBoxSizing_ContentBox,
   AzLayoutBoxSizing_BorderBox,
} AzLayoutBoxSizing;

typedef enum {
   AzLayoutFlexDirection_Row,
   AzLayoutFlexDirection_RowReverse,
   AzLayoutFlexDirection_Column,
   AzLayoutFlexDirection_ColumnReverse,
} AzLayoutFlexDirection;

typedef enum {
   AzLayoutDisplay_Flex,
   AzLayoutDisplay_Block,
   AzLayoutDisplay_InlineBlock,
} AzLayoutDisplay;

typedef enum {
   AzLayoutFloat_Left,
   AzLayoutFloat_Right,
} AzLayoutFloat;

typedef enum {
   AzLayoutJustifyContent_Start,
   AzLayoutJustifyContent_End,
   AzLayoutJustifyContent_Center,
   AzLayoutJustifyContent_SpaceBetween,
   AzLayoutJustifyContent_SpaceAround,
   AzLayoutJustifyContent_SpaceEvenly,
} AzLayoutJustifyContent;

typedef enum {
   AzLayoutPosition_Static,
   AzLayoutPosition_Relative,
   AzLayoutPosition_Absolute,
   AzLayoutPosition_Fixed,
} AzLayoutPosition;

typedef enum {
   AzLayoutFlexWrap_Wrap,
   AzLayoutFlexWrap_NoWrap,
} AzLayoutFlexWrap;

typedef enum {
   AzLayoutOverflow_Scroll,
   AzLayoutOverflow_Auto,
   AzLayoutOverflow_Hidden,
   AzLayoutOverflow_Visible,
} AzLayoutOverflow;

typedef enum {
   AzAngleMetric_Degree,
   AzAngleMetric_Radians,
   AzAngleMetric_Grad,
   AzAngleMetric_Turn,
   AzAngleMetric_Percent,
} AzAngleMetric;

typedef enum {
   AzDirectionCorner_Right,
   AzDirectionCorner_Left,
   AzDirectionCorner_Top,
   AzDirectionCorner_Bottom,
   AzDirectionCorner_TopRight,
   AzDirectionCorner_TopLeft,
   AzDirectionCorner_BottomRight,
   AzDirectionCorner_BottomLeft,
} AzDirectionCorner;

typedef enum {
   AzExtendMode_Clamp,
   AzExtendMode_Repeat,
} AzExtendMode;

typedef enum {
   AzShape_Ellipse,
   AzShape_Circle,
} AzShape;

typedef enum {
   AzRadialGradientSize_ClosestSide,
   AzRadialGradientSize_ClosestCorner,
   AzRadialGradientSize_FarthestSide,
   AzRadialGradientSize_FarthestCorner,
} AzRadialGradientSize;

typedef enum {
   AzStyleBackgroundRepeat_NoRepeat,
   AzStyleBackgroundRepeat_Repeat,
   AzStyleBackgroundRepeat_RepeatX,
   AzStyleBackgroundRepeat_RepeatY,
} AzStyleBackgroundRepeat;

typedef enum {
   AzBorderStyle_None,
   AzBorderStyle_Solid,
   AzBorderStyle_Double,
   AzBorderStyle_Dotted,
   AzBorderStyle_Dashed,
   AzBorderStyle_Hidden,
   AzBorderStyle_Groove,
   AzBorderStyle_Ridge,
   AzBorderStyle_Inset,
   AzBorderStyle_Outset,
} AzBorderStyle;

typedef enum {
   AzStyleCursor_Alias,
   AzStyleCursor_AllScroll,
   AzStyleCursor_Cell,
   AzStyleCursor_ColResize,
   AzStyleCursor_ContextMenu,
   AzStyleCursor_Copy,
   AzStyleCursor_Crosshair,
   AzStyleCursor_Default,
   AzStyleCursor_EResize,
   AzStyleCursor_EwResize,
   AzStyleCursor_Grab,
   AzStyleCursor_Grabbing,
   AzStyleCursor_Help,
   AzStyleCursor_Move,
   AzStyleCursor_NResize,
   AzStyleCursor_NsResize,
   AzStyleCursor_NeswResize,
   AzStyleCursor_NwseResize,
   AzStyleCursor_Pointer,
   AzStyleCursor_Progress,
   AzStyleCursor_RowResize,
   AzStyleCursor_SResize,
   AzStyleCursor_SeResize,
   AzStyleCursor_Text,
   AzStyleCursor_Unset,
   AzStyleCursor_VerticalText,
   AzStyleCursor_WResize,
   AzStyleCursor_Wait,
   AzStyleCursor_ZoomIn,
   AzStyleCursor_ZoomOut,
} AzStyleCursor;

typedef enum {
   AzStyleBackfaceVisibility_Hidden,
   AzStyleBackfaceVisibility_Visible,
} AzStyleBackfaceVisibility;

typedef enum {
   AzStyleTextAlignmentHorz_Left,
   AzStyleTextAlignmentHorz_Center,
   AzStyleTextAlignmentHorz_Right,
} AzStyleTextAlignmentHorz;

typedef struct {
    void* restrict ptr;
} AzCssPropertyCache;

typedef struct {
    void* const ptr;
    AzRendererType renderer_type;
} AzGlContextPtr;

typedef struct {
    uint32_t texture_id;
    AzRawImageFormat format;
    AzTextureFlags flags;
    AzPhysicalSizeU32 size;
    AzGlContextPtr gl_context;
} AzTexture;

typedef enum {
   AzVertexAttributeType_Float,
   AzVertexAttributeType_Double,
   AzVertexAttributeType_UnsignedByte,
   AzVertexAttributeType_UnsignedShort,
   AzVertexAttributeType_UnsignedInt,
} AzVertexAttributeType;

typedef enum {
   AzIndexBufferFormat_Points,
   AzIndexBufferFormat_Lines,
   AzIndexBufferFormat_LineStrip,
   AzIndexBufferFormat_Triangles,
   AzIndexBufferFormat_TriangleStrip,
   AzIndexBufferFormat_TriangleFan,
} AzIndexBufferFormat;

typedef enum {
   AzGlType_Gl,
   AzGlType_Gles,
} AzGlType;

typedef struct {
    void* const ptr;
} AzGLsyncPtr;

typedef enum {
   AzRawImageFormat_R8,
   AzRawImageFormat_R16,
   AzRawImageFormat_RG16,
   AzRawImageFormat_BGRA8,
   AzRawImageFormat_RGBAF32,
   AzRawImageFormat_RG8,
   AzRawImageFormat_RGBAI32,
   AzRawImageFormat_RGBA8,
} AzRawImageFormat;

typedef enum {
   AzSvgLineCap_Butt,
   AzSvgLineCap_Square,
   AzSvgLineCap_Round,
} AzSvgLineCap;

typedef enum {
   AzShapeRendering_OptimizeSpeed,
   AzShapeRendering_CrispEdges,
   AzShapeRendering_GeometricPrecision,
} AzShapeRendering;

typedef enum {
   AzTextRendering_OptimizeSpeed,
   AzTextRendering_OptimizeLegibility,
   AzTextRendering_GeometricPrecision,
} AzTextRendering;

typedef enum {
   AzImageRendering_OptimizeQuality,
   AzImageRendering_OptimizeSpeed,
} AzImageRendering;

typedef enum {
   AzFontDatabase_Empty,
   AzFontDatabase_System,
} AzFontDatabase;

typedef struct {
    void* restrict ptr;
} AzSvg;

typedef struct {
    void* restrict ptr;
} AzSvgXmlNode;

typedef enum {
   AzSvgLineJoin_Miter,
   AzSvgLineJoin_MiterClip,
   AzSvgLineJoin_Round,
   AzSvgLineJoin_Bevel,
} AzSvgLineJoin;

typedef enum {
   AzTerminateTimer_Terminate,
   AzTerminateTimer_Continue,
} AzTerminateTimer;

typedef struct {
    void* restrict ptr;
    AzThreadSendFn send_fn;
    AzThreadSenderDestructorFn destructor;
} AzThreadSender;

typedef struct {
    void* restrict ptr;
    AzThreadRecvFn recv_fn;
    AzThreadReceiverDestructorFn destructor;
} AzThreadReceiver;

typedef enum {
   AzThreadSendMsg_TerminateThread,
   AzThreadSendMsg_Tick,
} AzThreadSendMsg;

typedef struct {
    void* const ptr;
    AzInstantPtrCloneFn clone_fn;
    AzInstantPtrDestructorFn destructor;
} AzInstantPtr;

typedef struct {
    AzVsync vsync;
    AzSrgb srgb;
    AzHwAcceleration hw_accel;
} AzRendererOptions;

typedef struct {
    ssize_t x;
    ssize_t y;
} AzLayoutPoint;

typedef struct {
    ssize_t width;
    ssize_t height;
} AzLayoutSize;

typedef struct {
    AzLayoutPoint origin;
    AzLayoutSize size;
} AzLayoutRect;

typedef struct {
    void* restrict ui_window;
    void* restrict ui_view;
    void* restrict ui_view_controller;
} AzIOSHandle;

typedef struct {
    void* restrict ns_window;
    void* restrict ns_view;
} AzMacOSHandle;

typedef struct {
    uint64_t window;
    void* restrict display;
} AzXlibHandle;

typedef struct {
    uint32_t window;
    void* restrict connection;
} AzXcbHandle;

typedef struct {
    void* restrict surface;
    void* restrict display;
} AzWaylandHandle;

typedef struct {
    void* restrict hwnd;
    void* restrict hinstance;
} AzWindowsHandle;

typedef struct {
    uint32_t id;
} AzWebHandle;

typedef struct {
    void* restrict a_native_window;
} AzAndroidHandle;

typedef struct {
    int32_t x;
    int32_t y;
} AzPhysicalPositionI32;

typedef struct {
    uint32_t width;
    uint32_t height;
} AzPhysicalSizeU32;

typedef struct {
    float x;
    float y;
} AzLogicalPosition;

typedef struct {
    float width;
    float height;
} AzLogicalSize;

typedef struct {
    size_t id;
} AzIconKey;

typedef enum {
   AzAcceleratorKeyTag_Ctrl,
   AzAcceleratorKeyTag_Alt,
   AzAcceleratorKeyTag_Shift,
   AzAcceleratorKeyTag_Key,
} AzAcceleratorKeyTag;

typedef struct { AzAcceleratorKeyTag tag; } AzAcceleratorKeyVariant_Ctrl;
typedef struct { AzAcceleratorKeyTag tag; } AzAcceleratorKeyVariant_Alt;
typedef struct { AzAcceleratorKeyTag tag; } AzAcceleratorKeyVariant_Shift;
typedef struct { AzAcceleratorKeyTag tag; AzVirtualKeyCode payload; } AzAcceleratorKeyVariant_Key;

typedef union {
    AzAcceleratorKeyVariant_Ctrl Ctrl;
    AzAcceleratorKeyVariant_Alt Alt;
    AzAcceleratorKeyVariant_Shift Shift;
    AzAcceleratorKeyVariant_Key Key;
} AzAcceleratorKey;

typedef struct {
    bool  is_maximized;
    bool  is_minimized;
    bool  is_about_to_close;
    bool  is_fullscreen;
    bool  has_decorations;
    bool  is_visible;
    bool  is_always_on_top;
    bool  is_resizable;
    bool  has_focus;
    bool  has_extended_frame;
    bool  has_blur_behind_window;
} AzWindowFlags;

typedef struct {
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
} AzDebugState;

typedef enum {
   AzCursorPositionTag_OutOfWindow,
   AzCursorPositionTag_Uninitialized,
   AzCursorPositionTag_InWindow,
} AzCursorPositionTag;

typedef struct { AzCursorPositionTag tag; } AzCursorPositionVariant_OutOfWindow;
typedef struct { AzCursorPositionTag tag; } AzCursorPositionVariant_Uninitialized;
typedef struct { AzCursorPositionTag tag; AzLogicalPosition payload; } AzCursorPositionVariant_InWindow;

typedef union {
    AzCursorPositionVariant_OutOfWindow OutOfWindow;
    AzCursorPositionVariant_Uninitialized Uninitialized;
    AzCursorPositionVariant_InWindow InWindow;
} AzCursorPosition;

typedef struct {
    uint8_t _reserved;
} AzMacWindowOptions;

typedef struct {
    uint8_t _reserved;
} AzWasmWindowOptions;

typedef enum {
   AzWindowPositionTag_Uninitialized,
   AzWindowPositionTag_Initialized,
} AzWindowPositionTag;

typedef struct { AzWindowPositionTag tag; } AzWindowPositionVariant_Uninitialized;
typedef struct { AzWindowPositionTag tag; AzPhysicalPositionI32 payload; } AzWindowPositionVariant_Initialized;

typedef union {
    AzWindowPositionVariant_Uninitialized Uninitialized;
    AzWindowPositionVariant_Initialized Initialized;
} AzWindowPosition;

typedef enum {
   AzImePositionTag_Uninitialized,
   AzImePositionTag_Initialized,
} AzImePositionTag;

typedef struct { AzImePositionTag tag; } AzImePositionVariant_Uninitialized;
typedef struct { AzImePositionTag tag; AzLogicalPosition payload; } AzImePositionVariant_Initialized;

typedef union {
    AzImePositionVariant_Uninitialized Uninitialized;
    AzImePositionVariant_Initialized Initialized;
} AzImePosition;

typedef struct {
    uint8_t unused;
} AzTouchState;

typedef struct {
    AzLayoutSize size;
    uint16_t bit_depth;
    uint16_t refresh_rate;
} AzVideoMode;

typedef struct {
    AzLayoutCallbackType cb;
} AzLayoutCallback;

typedef struct {
    AzCallbackType cb;
} AzCallback;

typedef struct {
    size_t inner;
} AzNodeId;

typedef struct {
    size_t inner;
} AzDomId;

typedef struct {
    AzDomId dom;
    AzNodeId node;
} AzDomNodeId;

typedef struct {
    AzLogicalSize logical_size;
    float hidpi_factor;
} AzHidpiAdjustedBounds;

typedef struct {
    AzIFrameCallbackType cb;
} AzIFrameCallback;

typedef struct {
    void* const resources;
    AzHidpiAdjustedBounds bounds;
} AzIFrameCallbackInfo;

typedef struct {
    AzGlCallbackType cb;
} AzGlCallback;

typedef struct {
    AzTimerCallbackType cb;
} AzTimerCallback;

typedef struct {
    AzUpdateScreen should_update;
    AzTerminateTimer should_terminate;
} AzTimerCallbackReturn;

typedef struct {
    AzWriteBackCallbackType cb;
} AzWriteBackCallback;

typedef struct {
    AzGlCallback callback;
    AzRefAny data;
} AzGlTextureNode;

typedef struct {
    AzIFrameCallback callback;
    AzRefAny data;
} AzIFrameNode;

typedef enum {
   AzNotEventFilterTag_Hover,
   AzNotEventFilterTag_Focus,
} AzNotEventFilterTag;

typedef struct { AzNotEventFilterTag tag; AzHoverEventFilter payload; } AzNotEventFilterVariant_Hover;
typedef struct { AzNotEventFilterTag tag; AzFocusEventFilter payload; } AzNotEventFilterVariant_Focus;

typedef union {
    AzNotEventFilterVariant_Hover Hover;
    AzNotEventFilterVariant_Focus Focus;
} AzNotEventFilter;

typedef enum {
   AzTabIndexTag_Auto,
   AzTabIndexTag_OverrideInParent,
   AzTabIndexTag_NoKeyboardFocus,
} AzTabIndexTag;

typedef struct { AzTabIndexTag tag; } AzTabIndexVariant_Auto;
typedef struct { AzTabIndexTag tag; uint32_t payload; } AzTabIndexVariant_OverrideInParent;
typedef struct { AzTabIndexTag tag; } AzTabIndexVariant_NoKeyboardFocus;

typedef union {
    AzTabIndexVariant_Auto Auto;
    AzTabIndexVariant_OverrideInParent OverrideInParent;
    AzTabIndexVariant_NoKeyboardFocus NoKeyboardFocus;
} AzTabIndex;

typedef struct {
    uint32_t repeat;
    uint32_t offset;
} AzCssNthChildPattern;

typedef struct {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} AzColorU;

typedef struct {
    ssize_t number;
} AzFloatValue;

typedef struct {
    AzSizeMetric metric;
    AzFloatValue number;
} AzPixelValue;

typedef struct {
    AzPixelValue inner;
} AzPixelValueNoPercent;

typedef struct {
    AzPixelValueNoPercent offset[2];
    AzColorU color;
    AzPixelValueNoPercent blur_radius;
    AzPixelValueNoPercent spread_radius;
    AzBoxShadowClipMode clip_mode;
} AzStyleBoxShadow;

typedef struct {
    AzPixelValue inner;
} AzLayoutBottom;

typedef struct {
    AzFloatValue inner;
} AzLayoutFlexGrow;

typedef struct {
    AzFloatValue inner;
} AzLayoutFlexShrink;

typedef struct {
    AzPixelValue inner;
} AzLayoutHeight;

typedef struct {
    AzPixelValue inner;
} AzLayoutLeft;

typedef struct {
    AzPixelValue inner;
} AzLayoutMarginBottom;

typedef struct {
    AzPixelValue inner;
} AzLayoutMarginLeft;

typedef struct {
    AzPixelValue inner;
} AzLayoutMarginRight;

typedef struct {
    AzPixelValue inner;
} AzLayoutMarginTop;

typedef struct {
    AzPixelValue inner;
} AzLayoutMaxHeight;

typedef struct {
    AzPixelValue inner;
} AzLayoutMaxWidth;

typedef struct {
    AzPixelValue inner;
} AzLayoutMinHeight;

typedef struct {
    AzPixelValue inner;
} AzLayoutMinWidth;

typedef struct {
    AzPixelValue inner;
} AzLayoutPaddingBottom;

typedef struct {
    AzPixelValue inner;
} AzLayoutPaddingLeft;

typedef struct {
    AzPixelValue inner;
} AzLayoutPaddingRight;

typedef struct {
    AzPixelValue inner;
} AzLayoutPaddingTop;

typedef struct {
    AzPixelValue inner;
} AzLayoutRight;

typedef struct {
    AzPixelValue inner;
} AzLayoutTop;

typedef struct {
    AzPixelValue inner;
} AzLayoutWidth;

typedef struct {
    AzFloatValue number;
} AzPercentageValue;

typedef struct {
    AzAngleMetric metric;
    AzFloatValue number;
} AzAngleValue;

typedef struct {
    AzDirectionCorner from;
    AzDirectionCorner to;
} AzDirectionCorners;

typedef enum {
   AzDirectionTag_Angle,
   AzDirectionTag_FromTo,
} AzDirectionTag;

typedef struct { AzDirectionTag tag; AzAngleValue payload; } AzDirectionVariant_Angle;
typedef struct { AzDirectionTag tag; AzDirectionCorners payload; } AzDirectionVariant_FromTo;

typedef union {
    AzDirectionVariant_Angle Angle;
    AzDirectionVariant_FromTo FromTo;
} AzDirection;

typedef enum {
   AzBackgroundPositionHorizontalTag_Left,
   AzBackgroundPositionHorizontalTag_Center,
   AzBackgroundPositionHorizontalTag_Right,
   AzBackgroundPositionHorizontalTag_Exact,
} AzBackgroundPositionHorizontalTag;

typedef struct { AzBackgroundPositionHorizontalTag tag; } AzBackgroundPositionHorizontalVariant_Left;
typedef struct { AzBackgroundPositionHorizontalTag tag; } AzBackgroundPositionHorizontalVariant_Center;
typedef struct { AzBackgroundPositionHorizontalTag tag; } AzBackgroundPositionHorizontalVariant_Right;
typedef struct { AzBackgroundPositionHorizontalTag tag; AzPixelValue payload; } AzBackgroundPositionHorizontalVariant_Exact;

typedef union {
    AzBackgroundPositionHorizontalVariant_Left Left;
    AzBackgroundPositionHorizontalVariant_Center Center;
    AzBackgroundPositionHorizontalVariant_Right Right;
    AzBackgroundPositionHorizontalVariant_Exact Exact;
} AzBackgroundPositionHorizontal;

typedef enum {
   AzBackgroundPositionVerticalTag_Top,
   AzBackgroundPositionVerticalTag_Center,
   AzBackgroundPositionVerticalTag_Bottom,
   AzBackgroundPositionVerticalTag_Exact,
} AzBackgroundPositionVerticalTag;

typedef struct { AzBackgroundPositionVerticalTag tag; } AzBackgroundPositionVerticalVariant_Top;
typedef struct { AzBackgroundPositionVerticalTag tag; } AzBackgroundPositionVerticalVariant_Center;
typedef struct { AzBackgroundPositionVerticalTag tag; } AzBackgroundPositionVerticalVariant_Bottom;
typedef struct { AzBackgroundPositionVerticalTag tag; AzPixelValue payload; } AzBackgroundPositionVerticalVariant_Exact;

typedef union {
    AzBackgroundPositionVerticalVariant_Top Top;
    AzBackgroundPositionVerticalVariant_Center Center;
    AzBackgroundPositionVerticalVariant_Bottom Bottom;
    AzBackgroundPositionVerticalVariant_Exact Exact;
} AzBackgroundPositionVertical;

typedef struct {
    AzBackgroundPositionHorizontal horizontal;
    AzBackgroundPositionVertical vertical;
} AzStyleBackgroundPosition;

typedef enum {
   AzStyleBackgroundSizeTag_ExactSize,
   AzStyleBackgroundSizeTag_Contain,
   AzStyleBackgroundSizeTag_Cover,
} AzStyleBackgroundSizeTag;

typedef struct { AzStyleBackgroundSizeTag tag; AzPixelValue payload[2]; } AzStyleBackgroundSizeVariant_ExactSize;
typedef struct { AzStyleBackgroundSizeTag tag; } AzStyleBackgroundSizeVariant_Contain;
typedef struct { AzStyleBackgroundSizeTag tag; } AzStyleBackgroundSizeVariant_Cover;

typedef union {
    AzStyleBackgroundSizeVariant_ExactSize ExactSize;
    AzStyleBackgroundSizeVariant_Contain Contain;
    AzStyleBackgroundSizeVariant_Cover Cover;
} AzStyleBackgroundSize;

typedef struct {
    AzColorU inner;
} AzStyleBorderBottomColor;

typedef struct {
    AzPixelValue inner;
} AzStyleBorderBottomLeftRadius;

typedef struct {
    AzPixelValue inner;
} AzStyleBorderBottomRightRadius;

typedef struct {
    AzBorderStyle inner;
} AzStyleBorderBottomStyle;

typedef struct {
    AzPixelValue inner;
} AzLayoutBorderBottomWidth;

typedef struct {
    AzColorU inner;
} AzStyleBorderLeftColor;

typedef struct {
    AzBorderStyle inner;
} AzStyleBorderLeftStyle;

typedef struct {
    AzPixelValue inner;
} AzLayoutBorderLeftWidth;

typedef struct {
    AzColorU inner;
} AzStyleBorderRightColor;

typedef struct {
    AzBorderStyle inner;
} AzStyleBorderRightStyle;

typedef struct {
    AzPixelValue inner;
} AzLayoutBorderRightWidth;

typedef struct {
    AzColorU inner;
} AzStyleBorderTopColor;

typedef struct {
    AzPixelValue inner;
} AzStyleBorderTopLeftRadius;

typedef struct {
    AzPixelValue inner;
} AzStyleBorderTopRightRadius;

typedef struct {
    AzBorderStyle inner;
} AzStyleBorderTopStyle;

typedef struct {
    AzPixelValue inner;
} AzLayoutBorderTopWidth;

typedef struct {
    AzPixelValue inner;
} AzStyleFontSize;

typedef struct {
    AzPixelValue inner;
} AzStyleLetterSpacing;

typedef struct {
    AzPercentageValue inner;
} AzStyleLineHeight;

typedef struct {
    AzPercentageValue inner;
} AzStyleTabWidth;

typedef struct {
    AzFloatValue inner;
} AzStyleOpacity;

typedef struct {
    AzPixelValue x;
    AzPixelValue y;
} AzStyleTransformOrigin;

typedef struct {
    AzPixelValue x;
    AzPixelValue y;
} AzStylePerspectiveOrigin;

typedef struct {
    AzPixelValue a;
    AzPixelValue b;
    AzPixelValue c;
    AzPixelValue d;
    AzPixelValue tx;
    AzPixelValue ty;
} AzStyleTransformMatrix2D;

typedef struct {
    AzPixelValue m11;
    AzPixelValue m12;
    AzPixelValue m13;
    AzPixelValue m14;
    AzPixelValue m21;
    AzPixelValue m22;
    AzPixelValue m23;
    AzPixelValue m24;
    AzPixelValue m31;
    AzPixelValue m32;
    AzPixelValue m33;
    AzPixelValue m34;
    AzPixelValue m41;
    AzPixelValue m42;
    AzPixelValue m43;
    AzPixelValue m44;
} AzStyleTransformMatrix3D;

typedef struct {
    AzPixelValue x;
    AzPixelValue y;
} AzStyleTransformTranslate2D;

typedef struct {
    AzPixelValue x;
    AzPixelValue y;
    AzPixelValue z;
} AzStyleTransformTranslate3D;

typedef struct {
    AzPercentageValue x;
    AzPercentageValue y;
    AzPercentageValue z;
    AzAngleValue angle;
} AzStyleTransformRotate3D;

typedef struct {
    AzPercentageValue x;
    AzPercentageValue y;
} AzStyleTransformScale2D;

typedef struct {
    AzPercentageValue x;
    AzPercentageValue y;
    AzPercentageValue z;
} AzStyleTransformScale3D;

typedef struct {
    AzPercentageValue x;
    AzPercentageValue y;
} AzStyleTransformSkew2D;

typedef struct {
    AzColorU inner;
} AzStyleTextColor;

typedef struct {
    AzPixelValue inner;
} AzStyleWordSpacing;

typedef enum {
   AzStyleBoxShadowValueTag_Auto,
   AzStyleBoxShadowValueTag_None,
   AzStyleBoxShadowValueTag_Inherit,
   AzStyleBoxShadowValueTag_Initial,
   AzStyleBoxShadowValueTag_Exact,
} AzStyleBoxShadowValueTag;

typedef struct { AzStyleBoxShadowValueTag tag; } AzStyleBoxShadowValueVariant_Auto;
typedef struct { AzStyleBoxShadowValueTag tag; } AzStyleBoxShadowValueVariant_None;
typedef struct { AzStyleBoxShadowValueTag tag; } AzStyleBoxShadowValueVariant_Inherit;
typedef struct { AzStyleBoxShadowValueTag tag; } AzStyleBoxShadowValueVariant_Initial;
typedef struct { AzStyleBoxShadowValueTag tag; AzStyleBoxShadow payload; } AzStyleBoxShadowValueVariant_Exact;

typedef union {
    AzStyleBoxShadowValueVariant_Auto Auto;
    AzStyleBoxShadowValueVariant_None None;
    AzStyleBoxShadowValueVariant_Inherit Inherit;
    AzStyleBoxShadowValueVariant_Initial Initial;
    AzStyleBoxShadowValueVariant_Exact Exact;
} AzStyleBoxShadowValue;

typedef enum {
   AzLayoutAlignContentValueTag_Auto,
   AzLayoutAlignContentValueTag_None,
   AzLayoutAlignContentValueTag_Inherit,
   AzLayoutAlignContentValueTag_Initial,
   AzLayoutAlignContentValueTag_Exact,
} AzLayoutAlignContentValueTag;

typedef struct { AzLayoutAlignContentValueTag tag; } AzLayoutAlignContentValueVariant_Auto;
typedef struct { AzLayoutAlignContentValueTag tag; } AzLayoutAlignContentValueVariant_None;
typedef struct { AzLayoutAlignContentValueTag tag; } AzLayoutAlignContentValueVariant_Inherit;
typedef struct { AzLayoutAlignContentValueTag tag; } AzLayoutAlignContentValueVariant_Initial;
typedef struct { AzLayoutAlignContentValueTag tag; AzLayoutAlignContent payload; } AzLayoutAlignContentValueVariant_Exact;

typedef union {
    AzLayoutAlignContentValueVariant_Auto Auto;
    AzLayoutAlignContentValueVariant_None None;
    AzLayoutAlignContentValueVariant_Inherit Inherit;
    AzLayoutAlignContentValueVariant_Initial Initial;
    AzLayoutAlignContentValueVariant_Exact Exact;
} AzLayoutAlignContentValue;

typedef enum {
   AzLayoutAlignItemsValueTag_Auto,
   AzLayoutAlignItemsValueTag_None,
   AzLayoutAlignItemsValueTag_Inherit,
   AzLayoutAlignItemsValueTag_Initial,
   AzLayoutAlignItemsValueTag_Exact,
} AzLayoutAlignItemsValueTag;

typedef struct { AzLayoutAlignItemsValueTag tag; } AzLayoutAlignItemsValueVariant_Auto;
typedef struct { AzLayoutAlignItemsValueTag tag; } AzLayoutAlignItemsValueVariant_None;
typedef struct { AzLayoutAlignItemsValueTag tag; } AzLayoutAlignItemsValueVariant_Inherit;
typedef struct { AzLayoutAlignItemsValueTag tag; } AzLayoutAlignItemsValueVariant_Initial;
typedef struct { AzLayoutAlignItemsValueTag tag; AzLayoutAlignItems payload; } AzLayoutAlignItemsValueVariant_Exact;

typedef union {
    AzLayoutAlignItemsValueVariant_Auto Auto;
    AzLayoutAlignItemsValueVariant_None None;
    AzLayoutAlignItemsValueVariant_Inherit Inherit;
    AzLayoutAlignItemsValueVariant_Initial Initial;
    AzLayoutAlignItemsValueVariant_Exact Exact;
} AzLayoutAlignItemsValue;

typedef enum {
   AzLayoutBottomValueTag_Auto,
   AzLayoutBottomValueTag_None,
   AzLayoutBottomValueTag_Inherit,
   AzLayoutBottomValueTag_Initial,
   AzLayoutBottomValueTag_Exact,
} AzLayoutBottomValueTag;

typedef struct { AzLayoutBottomValueTag tag; } AzLayoutBottomValueVariant_Auto;
typedef struct { AzLayoutBottomValueTag tag; } AzLayoutBottomValueVariant_None;
typedef struct { AzLayoutBottomValueTag tag; } AzLayoutBottomValueVariant_Inherit;
typedef struct { AzLayoutBottomValueTag tag; } AzLayoutBottomValueVariant_Initial;
typedef struct { AzLayoutBottomValueTag tag; AzLayoutBottom payload; } AzLayoutBottomValueVariant_Exact;

typedef union {
    AzLayoutBottomValueVariant_Auto Auto;
    AzLayoutBottomValueVariant_None None;
    AzLayoutBottomValueVariant_Inherit Inherit;
    AzLayoutBottomValueVariant_Initial Initial;
    AzLayoutBottomValueVariant_Exact Exact;
} AzLayoutBottomValue;

typedef enum {
   AzLayoutBoxSizingValueTag_Auto,
   AzLayoutBoxSizingValueTag_None,
   AzLayoutBoxSizingValueTag_Inherit,
   AzLayoutBoxSizingValueTag_Initial,
   AzLayoutBoxSizingValueTag_Exact,
} AzLayoutBoxSizingValueTag;

typedef struct { AzLayoutBoxSizingValueTag tag; } AzLayoutBoxSizingValueVariant_Auto;
typedef struct { AzLayoutBoxSizingValueTag tag; } AzLayoutBoxSizingValueVariant_None;
typedef struct { AzLayoutBoxSizingValueTag tag; } AzLayoutBoxSizingValueVariant_Inherit;
typedef struct { AzLayoutBoxSizingValueTag tag; } AzLayoutBoxSizingValueVariant_Initial;
typedef struct { AzLayoutBoxSizingValueTag tag; AzLayoutBoxSizing payload; } AzLayoutBoxSizingValueVariant_Exact;

typedef union {
    AzLayoutBoxSizingValueVariant_Auto Auto;
    AzLayoutBoxSizingValueVariant_None None;
    AzLayoutBoxSizingValueVariant_Inherit Inherit;
    AzLayoutBoxSizingValueVariant_Initial Initial;
    AzLayoutBoxSizingValueVariant_Exact Exact;
} AzLayoutBoxSizingValue;

typedef enum {
   AzLayoutFlexDirectionValueTag_Auto,
   AzLayoutFlexDirectionValueTag_None,
   AzLayoutFlexDirectionValueTag_Inherit,
   AzLayoutFlexDirectionValueTag_Initial,
   AzLayoutFlexDirectionValueTag_Exact,
} AzLayoutFlexDirectionValueTag;

typedef struct { AzLayoutFlexDirectionValueTag tag; } AzLayoutFlexDirectionValueVariant_Auto;
typedef struct { AzLayoutFlexDirectionValueTag tag; } AzLayoutFlexDirectionValueVariant_None;
typedef struct { AzLayoutFlexDirectionValueTag tag; } AzLayoutFlexDirectionValueVariant_Inherit;
typedef struct { AzLayoutFlexDirectionValueTag tag; } AzLayoutFlexDirectionValueVariant_Initial;
typedef struct { AzLayoutFlexDirectionValueTag tag; AzLayoutFlexDirection payload; } AzLayoutFlexDirectionValueVariant_Exact;

typedef union {
    AzLayoutFlexDirectionValueVariant_Auto Auto;
    AzLayoutFlexDirectionValueVariant_None None;
    AzLayoutFlexDirectionValueVariant_Inherit Inherit;
    AzLayoutFlexDirectionValueVariant_Initial Initial;
    AzLayoutFlexDirectionValueVariant_Exact Exact;
} AzLayoutFlexDirectionValue;

typedef enum {
   AzLayoutDisplayValueTag_Auto,
   AzLayoutDisplayValueTag_None,
   AzLayoutDisplayValueTag_Inherit,
   AzLayoutDisplayValueTag_Initial,
   AzLayoutDisplayValueTag_Exact,
} AzLayoutDisplayValueTag;

typedef struct { AzLayoutDisplayValueTag tag; } AzLayoutDisplayValueVariant_Auto;
typedef struct { AzLayoutDisplayValueTag tag; } AzLayoutDisplayValueVariant_None;
typedef struct { AzLayoutDisplayValueTag tag; } AzLayoutDisplayValueVariant_Inherit;
typedef struct { AzLayoutDisplayValueTag tag; } AzLayoutDisplayValueVariant_Initial;
typedef struct { AzLayoutDisplayValueTag tag; AzLayoutDisplay payload; } AzLayoutDisplayValueVariant_Exact;

typedef union {
    AzLayoutDisplayValueVariant_Auto Auto;
    AzLayoutDisplayValueVariant_None None;
    AzLayoutDisplayValueVariant_Inherit Inherit;
    AzLayoutDisplayValueVariant_Initial Initial;
    AzLayoutDisplayValueVariant_Exact Exact;
} AzLayoutDisplayValue;

typedef enum {
   AzLayoutFlexGrowValueTag_Auto,
   AzLayoutFlexGrowValueTag_None,
   AzLayoutFlexGrowValueTag_Inherit,
   AzLayoutFlexGrowValueTag_Initial,
   AzLayoutFlexGrowValueTag_Exact,
} AzLayoutFlexGrowValueTag;

typedef struct { AzLayoutFlexGrowValueTag tag; } AzLayoutFlexGrowValueVariant_Auto;
typedef struct { AzLayoutFlexGrowValueTag tag; } AzLayoutFlexGrowValueVariant_None;
typedef struct { AzLayoutFlexGrowValueTag tag; } AzLayoutFlexGrowValueVariant_Inherit;
typedef struct { AzLayoutFlexGrowValueTag tag; } AzLayoutFlexGrowValueVariant_Initial;
typedef struct { AzLayoutFlexGrowValueTag tag; AzLayoutFlexGrow payload; } AzLayoutFlexGrowValueVariant_Exact;

typedef union {
    AzLayoutFlexGrowValueVariant_Auto Auto;
    AzLayoutFlexGrowValueVariant_None None;
    AzLayoutFlexGrowValueVariant_Inherit Inherit;
    AzLayoutFlexGrowValueVariant_Initial Initial;
    AzLayoutFlexGrowValueVariant_Exact Exact;
} AzLayoutFlexGrowValue;

typedef enum {
   AzLayoutFlexShrinkValueTag_Auto,
   AzLayoutFlexShrinkValueTag_None,
   AzLayoutFlexShrinkValueTag_Inherit,
   AzLayoutFlexShrinkValueTag_Initial,
   AzLayoutFlexShrinkValueTag_Exact,
} AzLayoutFlexShrinkValueTag;

typedef struct { AzLayoutFlexShrinkValueTag tag; } AzLayoutFlexShrinkValueVariant_Auto;
typedef struct { AzLayoutFlexShrinkValueTag tag; } AzLayoutFlexShrinkValueVariant_None;
typedef struct { AzLayoutFlexShrinkValueTag tag; } AzLayoutFlexShrinkValueVariant_Inherit;
typedef struct { AzLayoutFlexShrinkValueTag tag; } AzLayoutFlexShrinkValueVariant_Initial;
typedef struct { AzLayoutFlexShrinkValueTag tag; AzLayoutFlexShrink payload; } AzLayoutFlexShrinkValueVariant_Exact;

typedef union {
    AzLayoutFlexShrinkValueVariant_Auto Auto;
    AzLayoutFlexShrinkValueVariant_None None;
    AzLayoutFlexShrinkValueVariant_Inherit Inherit;
    AzLayoutFlexShrinkValueVariant_Initial Initial;
    AzLayoutFlexShrinkValueVariant_Exact Exact;
} AzLayoutFlexShrinkValue;

typedef enum {
   AzLayoutFloatValueTag_Auto,
   AzLayoutFloatValueTag_None,
   AzLayoutFloatValueTag_Inherit,
   AzLayoutFloatValueTag_Initial,
   AzLayoutFloatValueTag_Exact,
} AzLayoutFloatValueTag;

typedef struct { AzLayoutFloatValueTag tag; } AzLayoutFloatValueVariant_Auto;
typedef struct { AzLayoutFloatValueTag tag; } AzLayoutFloatValueVariant_None;
typedef struct { AzLayoutFloatValueTag tag; } AzLayoutFloatValueVariant_Inherit;
typedef struct { AzLayoutFloatValueTag tag; } AzLayoutFloatValueVariant_Initial;
typedef struct { AzLayoutFloatValueTag tag; AzLayoutFloat payload; } AzLayoutFloatValueVariant_Exact;

typedef union {
    AzLayoutFloatValueVariant_Auto Auto;
    AzLayoutFloatValueVariant_None None;
    AzLayoutFloatValueVariant_Inherit Inherit;
    AzLayoutFloatValueVariant_Initial Initial;
    AzLayoutFloatValueVariant_Exact Exact;
} AzLayoutFloatValue;

typedef enum {
   AzLayoutHeightValueTag_Auto,
   AzLayoutHeightValueTag_None,
   AzLayoutHeightValueTag_Inherit,
   AzLayoutHeightValueTag_Initial,
   AzLayoutHeightValueTag_Exact,
} AzLayoutHeightValueTag;

typedef struct { AzLayoutHeightValueTag tag; } AzLayoutHeightValueVariant_Auto;
typedef struct { AzLayoutHeightValueTag tag; } AzLayoutHeightValueVariant_None;
typedef struct { AzLayoutHeightValueTag tag; } AzLayoutHeightValueVariant_Inherit;
typedef struct { AzLayoutHeightValueTag tag; } AzLayoutHeightValueVariant_Initial;
typedef struct { AzLayoutHeightValueTag tag; AzLayoutHeight payload; } AzLayoutHeightValueVariant_Exact;

typedef union {
    AzLayoutHeightValueVariant_Auto Auto;
    AzLayoutHeightValueVariant_None None;
    AzLayoutHeightValueVariant_Inherit Inherit;
    AzLayoutHeightValueVariant_Initial Initial;
    AzLayoutHeightValueVariant_Exact Exact;
} AzLayoutHeightValue;

typedef enum {
   AzLayoutJustifyContentValueTag_Auto,
   AzLayoutJustifyContentValueTag_None,
   AzLayoutJustifyContentValueTag_Inherit,
   AzLayoutJustifyContentValueTag_Initial,
   AzLayoutJustifyContentValueTag_Exact,
} AzLayoutJustifyContentValueTag;

typedef struct { AzLayoutJustifyContentValueTag tag; } AzLayoutJustifyContentValueVariant_Auto;
typedef struct { AzLayoutJustifyContentValueTag tag; } AzLayoutJustifyContentValueVariant_None;
typedef struct { AzLayoutJustifyContentValueTag tag; } AzLayoutJustifyContentValueVariant_Inherit;
typedef struct { AzLayoutJustifyContentValueTag tag; } AzLayoutJustifyContentValueVariant_Initial;
typedef struct { AzLayoutJustifyContentValueTag tag; AzLayoutJustifyContent payload; } AzLayoutJustifyContentValueVariant_Exact;

typedef union {
    AzLayoutJustifyContentValueVariant_Auto Auto;
    AzLayoutJustifyContentValueVariant_None None;
    AzLayoutJustifyContentValueVariant_Inherit Inherit;
    AzLayoutJustifyContentValueVariant_Initial Initial;
    AzLayoutJustifyContentValueVariant_Exact Exact;
} AzLayoutJustifyContentValue;

typedef enum {
   AzLayoutLeftValueTag_Auto,
   AzLayoutLeftValueTag_None,
   AzLayoutLeftValueTag_Inherit,
   AzLayoutLeftValueTag_Initial,
   AzLayoutLeftValueTag_Exact,
} AzLayoutLeftValueTag;

typedef struct { AzLayoutLeftValueTag tag; } AzLayoutLeftValueVariant_Auto;
typedef struct { AzLayoutLeftValueTag tag; } AzLayoutLeftValueVariant_None;
typedef struct { AzLayoutLeftValueTag tag; } AzLayoutLeftValueVariant_Inherit;
typedef struct { AzLayoutLeftValueTag tag; } AzLayoutLeftValueVariant_Initial;
typedef struct { AzLayoutLeftValueTag tag; AzLayoutLeft payload; } AzLayoutLeftValueVariant_Exact;

typedef union {
    AzLayoutLeftValueVariant_Auto Auto;
    AzLayoutLeftValueVariant_None None;
    AzLayoutLeftValueVariant_Inherit Inherit;
    AzLayoutLeftValueVariant_Initial Initial;
    AzLayoutLeftValueVariant_Exact Exact;
} AzLayoutLeftValue;

typedef enum {
   AzLayoutMarginBottomValueTag_Auto,
   AzLayoutMarginBottomValueTag_None,
   AzLayoutMarginBottomValueTag_Inherit,
   AzLayoutMarginBottomValueTag_Initial,
   AzLayoutMarginBottomValueTag_Exact,
} AzLayoutMarginBottomValueTag;

typedef struct { AzLayoutMarginBottomValueTag tag; } AzLayoutMarginBottomValueVariant_Auto;
typedef struct { AzLayoutMarginBottomValueTag tag; } AzLayoutMarginBottomValueVariant_None;
typedef struct { AzLayoutMarginBottomValueTag tag; } AzLayoutMarginBottomValueVariant_Inherit;
typedef struct { AzLayoutMarginBottomValueTag tag; } AzLayoutMarginBottomValueVariant_Initial;
typedef struct { AzLayoutMarginBottomValueTag tag; AzLayoutMarginBottom payload; } AzLayoutMarginBottomValueVariant_Exact;

typedef union {
    AzLayoutMarginBottomValueVariant_Auto Auto;
    AzLayoutMarginBottomValueVariant_None None;
    AzLayoutMarginBottomValueVariant_Inherit Inherit;
    AzLayoutMarginBottomValueVariant_Initial Initial;
    AzLayoutMarginBottomValueVariant_Exact Exact;
} AzLayoutMarginBottomValue;

typedef enum {
   AzLayoutMarginLeftValueTag_Auto,
   AzLayoutMarginLeftValueTag_None,
   AzLayoutMarginLeftValueTag_Inherit,
   AzLayoutMarginLeftValueTag_Initial,
   AzLayoutMarginLeftValueTag_Exact,
} AzLayoutMarginLeftValueTag;

typedef struct { AzLayoutMarginLeftValueTag tag; } AzLayoutMarginLeftValueVariant_Auto;
typedef struct { AzLayoutMarginLeftValueTag tag; } AzLayoutMarginLeftValueVariant_None;
typedef struct { AzLayoutMarginLeftValueTag tag; } AzLayoutMarginLeftValueVariant_Inherit;
typedef struct { AzLayoutMarginLeftValueTag tag; } AzLayoutMarginLeftValueVariant_Initial;
typedef struct { AzLayoutMarginLeftValueTag tag; AzLayoutMarginLeft payload; } AzLayoutMarginLeftValueVariant_Exact;

typedef union {
    AzLayoutMarginLeftValueVariant_Auto Auto;
    AzLayoutMarginLeftValueVariant_None None;
    AzLayoutMarginLeftValueVariant_Inherit Inherit;
    AzLayoutMarginLeftValueVariant_Initial Initial;
    AzLayoutMarginLeftValueVariant_Exact Exact;
} AzLayoutMarginLeftValue;

typedef enum {
   AzLayoutMarginRightValueTag_Auto,
   AzLayoutMarginRightValueTag_None,
   AzLayoutMarginRightValueTag_Inherit,
   AzLayoutMarginRightValueTag_Initial,
   AzLayoutMarginRightValueTag_Exact,
} AzLayoutMarginRightValueTag;

typedef struct { AzLayoutMarginRightValueTag tag; } AzLayoutMarginRightValueVariant_Auto;
typedef struct { AzLayoutMarginRightValueTag tag; } AzLayoutMarginRightValueVariant_None;
typedef struct { AzLayoutMarginRightValueTag tag; } AzLayoutMarginRightValueVariant_Inherit;
typedef struct { AzLayoutMarginRightValueTag tag; } AzLayoutMarginRightValueVariant_Initial;
typedef struct { AzLayoutMarginRightValueTag tag; AzLayoutMarginRight payload; } AzLayoutMarginRightValueVariant_Exact;

typedef union {
    AzLayoutMarginRightValueVariant_Auto Auto;
    AzLayoutMarginRightValueVariant_None None;
    AzLayoutMarginRightValueVariant_Inherit Inherit;
    AzLayoutMarginRightValueVariant_Initial Initial;
    AzLayoutMarginRightValueVariant_Exact Exact;
} AzLayoutMarginRightValue;

typedef enum {
   AzLayoutMarginTopValueTag_Auto,
   AzLayoutMarginTopValueTag_None,
   AzLayoutMarginTopValueTag_Inherit,
   AzLayoutMarginTopValueTag_Initial,
   AzLayoutMarginTopValueTag_Exact,
} AzLayoutMarginTopValueTag;

typedef struct { AzLayoutMarginTopValueTag tag; } AzLayoutMarginTopValueVariant_Auto;
typedef struct { AzLayoutMarginTopValueTag tag; } AzLayoutMarginTopValueVariant_None;
typedef struct { AzLayoutMarginTopValueTag tag; } AzLayoutMarginTopValueVariant_Inherit;
typedef struct { AzLayoutMarginTopValueTag tag; } AzLayoutMarginTopValueVariant_Initial;
typedef struct { AzLayoutMarginTopValueTag tag; AzLayoutMarginTop payload; } AzLayoutMarginTopValueVariant_Exact;

typedef union {
    AzLayoutMarginTopValueVariant_Auto Auto;
    AzLayoutMarginTopValueVariant_None None;
    AzLayoutMarginTopValueVariant_Inherit Inherit;
    AzLayoutMarginTopValueVariant_Initial Initial;
    AzLayoutMarginTopValueVariant_Exact Exact;
} AzLayoutMarginTopValue;

typedef enum {
   AzLayoutMaxHeightValueTag_Auto,
   AzLayoutMaxHeightValueTag_None,
   AzLayoutMaxHeightValueTag_Inherit,
   AzLayoutMaxHeightValueTag_Initial,
   AzLayoutMaxHeightValueTag_Exact,
} AzLayoutMaxHeightValueTag;

typedef struct { AzLayoutMaxHeightValueTag tag; } AzLayoutMaxHeightValueVariant_Auto;
typedef struct { AzLayoutMaxHeightValueTag tag; } AzLayoutMaxHeightValueVariant_None;
typedef struct { AzLayoutMaxHeightValueTag tag; } AzLayoutMaxHeightValueVariant_Inherit;
typedef struct { AzLayoutMaxHeightValueTag tag; } AzLayoutMaxHeightValueVariant_Initial;
typedef struct { AzLayoutMaxHeightValueTag tag; AzLayoutMaxHeight payload; } AzLayoutMaxHeightValueVariant_Exact;

typedef union {
    AzLayoutMaxHeightValueVariant_Auto Auto;
    AzLayoutMaxHeightValueVariant_None None;
    AzLayoutMaxHeightValueVariant_Inherit Inherit;
    AzLayoutMaxHeightValueVariant_Initial Initial;
    AzLayoutMaxHeightValueVariant_Exact Exact;
} AzLayoutMaxHeightValue;

typedef enum {
   AzLayoutMaxWidthValueTag_Auto,
   AzLayoutMaxWidthValueTag_None,
   AzLayoutMaxWidthValueTag_Inherit,
   AzLayoutMaxWidthValueTag_Initial,
   AzLayoutMaxWidthValueTag_Exact,
} AzLayoutMaxWidthValueTag;

typedef struct { AzLayoutMaxWidthValueTag tag; } AzLayoutMaxWidthValueVariant_Auto;
typedef struct { AzLayoutMaxWidthValueTag tag; } AzLayoutMaxWidthValueVariant_None;
typedef struct { AzLayoutMaxWidthValueTag tag; } AzLayoutMaxWidthValueVariant_Inherit;
typedef struct { AzLayoutMaxWidthValueTag tag; } AzLayoutMaxWidthValueVariant_Initial;
typedef struct { AzLayoutMaxWidthValueTag tag; AzLayoutMaxWidth payload; } AzLayoutMaxWidthValueVariant_Exact;

typedef union {
    AzLayoutMaxWidthValueVariant_Auto Auto;
    AzLayoutMaxWidthValueVariant_None None;
    AzLayoutMaxWidthValueVariant_Inherit Inherit;
    AzLayoutMaxWidthValueVariant_Initial Initial;
    AzLayoutMaxWidthValueVariant_Exact Exact;
} AzLayoutMaxWidthValue;

typedef enum {
   AzLayoutMinHeightValueTag_Auto,
   AzLayoutMinHeightValueTag_None,
   AzLayoutMinHeightValueTag_Inherit,
   AzLayoutMinHeightValueTag_Initial,
   AzLayoutMinHeightValueTag_Exact,
} AzLayoutMinHeightValueTag;

typedef struct { AzLayoutMinHeightValueTag tag; } AzLayoutMinHeightValueVariant_Auto;
typedef struct { AzLayoutMinHeightValueTag tag; } AzLayoutMinHeightValueVariant_None;
typedef struct { AzLayoutMinHeightValueTag tag; } AzLayoutMinHeightValueVariant_Inherit;
typedef struct { AzLayoutMinHeightValueTag tag; } AzLayoutMinHeightValueVariant_Initial;
typedef struct { AzLayoutMinHeightValueTag tag; AzLayoutMinHeight payload; } AzLayoutMinHeightValueVariant_Exact;

typedef union {
    AzLayoutMinHeightValueVariant_Auto Auto;
    AzLayoutMinHeightValueVariant_None None;
    AzLayoutMinHeightValueVariant_Inherit Inherit;
    AzLayoutMinHeightValueVariant_Initial Initial;
    AzLayoutMinHeightValueVariant_Exact Exact;
} AzLayoutMinHeightValue;

typedef enum {
   AzLayoutMinWidthValueTag_Auto,
   AzLayoutMinWidthValueTag_None,
   AzLayoutMinWidthValueTag_Inherit,
   AzLayoutMinWidthValueTag_Initial,
   AzLayoutMinWidthValueTag_Exact,
} AzLayoutMinWidthValueTag;

typedef struct { AzLayoutMinWidthValueTag tag; } AzLayoutMinWidthValueVariant_Auto;
typedef struct { AzLayoutMinWidthValueTag tag; } AzLayoutMinWidthValueVariant_None;
typedef struct { AzLayoutMinWidthValueTag tag; } AzLayoutMinWidthValueVariant_Inherit;
typedef struct { AzLayoutMinWidthValueTag tag; } AzLayoutMinWidthValueVariant_Initial;
typedef struct { AzLayoutMinWidthValueTag tag; AzLayoutMinWidth payload; } AzLayoutMinWidthValueVariant_Exact;

typedef union {
    AzLayoutMinWidthValueVariant_Auto Auto;
    AzLayoutMinWidthValueVariant_None None;
    AzLayoutMinWidthValueVariant_Inherit Inherit;
    AzLayoutMinWidthValueVariant_Initial Initial;
    AzLayoutMinWidthValueVariant_Exact Exact;
} AzLayoutMinWidthValue;

typedef enum {
   AzLayoutPaddingBottomValueTag_Auto,
   AzLayoutPaddingBottomValueTag_None,
   AzLayoutPaddingBottomValueTag_Inherit,
   AzLayoutPaddingBottomValueTag_Initial,
   AzLayoutPaddingBottomValueTag_Exact,
} AzLayoutPaddingBottomValueTag;

typedef struct { AzLayoutPaddingBottomValueTag tag; } AzLayoutPaddingBottomValueVariant_Auto;
typedef struct { AzLayoutPaddingBottomValueTag tag; } AzLayoutPaddingBottomValueVariant_None;
typedef struct { AzLayoutPaddingBottomValueTag tag; } AzLayoutPaddingBottomValueVariant_Inherit;
typedef struct { AzLayoutPaddingBottomValueTag tag; } AzLayoutPaddingBottomValueVariant_Initial;
typedef struct { AzLayoutPaddingBottomValueTag tag; AzLayoutPaddingBottom payload; } AzLayoutPaddingBottomValueVariant_Exact;

typedef union {
    AzLayoutPaddingBottomValueVariant_Auto Auto;
    AzLayoutPaddingBottomValueVariant_None None;
    AzLayoutPaddingBottomValueVariant_Inherit Inherit;
    AzLayoutPaddingBottomValueVariant_Initial Initial;
    AzLayoutPaddingBottomValueVariant_Exact Exact;
} AzLayoutPaddingBottomValue;

typedef enum {
   AzLayoutPaddingLeftValueTag_Auto,
   AzLayoutPaddingLeftValueTag_None,
   AzLayoutPaddingLeftValueTag_Inherit,
   AzLayoutPaddingLeftValueTag_Initial,
   AzLayoutPaddingLeftValueTag_Exact,
} AzLayoutPaddingLeftValueTag;

typedef struct { AzLayoutPaddingLeftValueTag tag; } AzLayoutPaddingLeftValueVariant_Auto;
typedef struct { AzLayoutPaddingLeftValueTag tag; } AzLayoutPaddingLeftValueVariant_None;
typedef struct { AzLayoutPaddingLeftValueTag tag; } AzLayoutPaddingLeftValueVariant_Inherit;
typedef struct { AzLayoutPaddingLeftValueTag tag; } AzLayoutPaddingLeftValueVariant_Initial;
typedef struct { AzLayoutPaddingLeftValueTag tag; AzLayoutPaddingLeft payload; } AzLayoutPaddingLeftValueVariant_Exact;

typedef union {
    AzLayoutPaddingLeftValueVariant_Auto Auto;
    AzLayoutPaddingLeftValueVariant_None None;
    AzLayoutPaddingLeftValueVariant_Inherit Inherit;
    AzLayoutPaddingLeftValueVariant_Initial Initial;
    AzLayoutPaddingLeftValueVariant_Exact Exact;
} AzLayoutPaddingLeftValue;

typedef enum {
   AzLayoutPaddingRightValueTag_Auto,
   AzLayoutPaddingRightValueTag_None,
   AzLayoutPaddingRightValueTag_Inherit,
   AzLayoutPaddingRightValueTag_Initial,
   AzLayoutPaddingRightValueTag_Exact,
} AzLayoutPaddingRightValueTag;

typedef struct { AzLayoutPaddingRightValueTag tag; } AzLayoutPaddingRightValueVariant_Auto;
typedef struct { AzLayoutPaddingRightValueTag tag; } AzLayoutPaddingRightValueVariant_None;
typedef struct { AzLayoutPaddingRightValueTag tag; } AzLayoutPaddingRightValueVariant_Inherit;
typedef struct { AzLayoutPaddingRightValueTag tag; } AzLayoutPaddingRightValueVariant_Initial;
typedef struct { AzLayoutPaddingRightValueTag tag; AzLayoutPaddingRight payload; } AzLayoutPaddingRightValueVariant_Exact;

typedef union {
    AzLayoutPaddingRightValueVariant_Auto Auto;
    AzLayoutPaddingRightValueVariant_None None;
    AzLayoutPaddingRightValueVariant_Inherit Inherit;
    AzLayoutPaddingRightValueVariant_Initial Initial;
    AzLayoutPaddingRightValueVariant_Exact Exact;
} AzLayoutPaddingRightValue;

typedef enum {
   AzLayoutPaddingTopValueTag_Auto,
   AzLayoutPaddingTopValueTag_None,
   AzLayoutPaddingTopValueTag_Inherit,
   AzLayoutPaddingTopValueTag_Initial,
   AzLayoutPaddingTopValueTag_Exact,
} AzLayoutPaddingTopValueTag;

typedef struct { AzLayoutPaddingTopValueTag tag; } AzLayoutPaddingTopValueVariant_Auto;
typedef struct { AzLayoutPaddingTopValueTag tag; } AzLayoutPaddingTopValueVariant_None;
typedef struct { AzLayoutPaddingTopValueTag tag; } AzLayoutPaddingTopValueVariant_Inherit;
typedef struct { AzLayoutPaddingTopValueTag tag; } AzLayoutPaddingTopValueVariant_Initial;
typedef struct { AzLayoutPaddingTopValueTag tag; AzLayoutPaddingTop payload; } AzLayoutPaddingTopValueVariant_Exact;

typedef union {
    AzLayoutPaddingTopValueVariant_Auto Auto;
    AzLayoutPaddingTopValueVariant_None None;
    AzLayoutPaddingTopValueVariant_Inherit Inherit;
    AzLayoutPaddingTopValueVariant_Initial Initial;
    AzLayoutPaddingTopValueVariant_Exact Exact;
} AzLayoutPaddingTopValue;

typedef enum {
   AzLayoutPositionValueTag_Auto,
   AzLayoutPositionValueTag_None,
   AzLayoutPositionValueTag_Inherit,
   AzLayoutPositionValueTag_Initial,
   AzLayoutPositionValueTag_Exact,
} AzLayoutPositionValueTag;

typedef struct { AzLayoutPositionValueTag tag; } AzLayoutPositionValueVariant_Auto;
typedef struct { AzLayoutPositionValueTag tag; } AzLayoutPositionValueVariant_None;
typedef struct { AzLayoutPositionValueTag tag; } AzLayoutPositionValueVariant_Inherit;
typedef struct { AzLayoutPositionValueTag tag; } AzLayoutPositionValueVariant_Initial;
typedef struct { AzLayoutPositionValueTag tag; AzLayoutPosition payload; } AzLayoutPositionValueVariant_Exact;

typedef union {
    AzLayoutPositionValueVariant_Auto Auto;
    AzLayoutPositionValueVariant_None None;
    AzLayoutPositionValueVariant_Inherit Inherit;
    AzLayoutPositionValueVariant_Initial Initial;
    AzLayoutPositionValueVariant_Exact Exact;
} AzLayoutPositionValue;

typedef enum {
   AzLayoutRightValueTag_Auto,
   AzLayoutRightValueTag_None,
   AzLayoutRightValueTag_Inherit,
   AzLayoutRightValueTag_Initial,
   AzLayoutRightValueTag_Exact,
} AzLayoutRightValueTag;

typedef struct { AzLayoutRightValueTag tag; } AzLayoutRightValueVariant_Auto;
typedef struct { AzLayoutRightValueTag tag; } AzLayoutRightValueVariant_None;
typedef struct { AzLayoutRightValueTag tag; } AzLayoutRightValueVariant_Inherit;
typedef struct { AzLayoutRightValueTag tag; } AzLayoutRightValueVariant_Initial;
typedef struct { AzLayoutRightValueTag tag; AzLayoutRight payload; } AzLayoutRightValueVariant_Exact;

typedef union {
    AzLayoutRightValueVariant_Auto Auto;
    AzLayoutRightValueVariant_None None;
    AzLayoutRightValueVariant_Inherit Inherit;
    AzLayoutRightValueVariant_Initial Initial;
    AzLayoutRightValueVariant_Exact Exact;
} AzLayoutRightValue;

typedef enum {
   AzLayoutTopValueTag_Auto,
   AzLayoutTopValueTag_None,
   AzLayoutTopValueTag_Inherit,
   AzLayoutTopValueTag_Initial,
   AzLayoutTopValueTag_Exact,
} AzLayoutTopValueTag;

typedef struct { AzLayoutTopValueTag tag; } AzLayoutTopValueVariant_Auto;
typedef struct { AzLayoutTopValueTag tag; } AzLayoutTopValueVariant_None;
typedef struct { AzLayoutTopValueTag tag; } AzLayoutTopValueVariant_Inherit;
typedef struct { AzLayoutTopValueTag tag; } AzLayoutTopValueVariant_Initial;
typedef struct { AzLayoutTopValueTag tag; AzLayoutTop payload; } AzLayoutTopValueVariant_Exact;

typedef union {
    AzLayoutTopValueVariant_Auto Auto;
    AzLayoutTopValueVariant_None None;
    AzLayoutTopValueVariant_Inherit Inherit;
    AzLayoutTopValueVariant_Initial Initial;
    AzLayoutTopValueVariant_Exact Exact;
} AzLayoutTopValue;

typedef enum {
   AzLayoutWidthValueTag_Auto,
   AzLayoutWidthValueTag_None,
   AzLayoutWidthValueTag_Inherit,
   AzLayoutWidthValueTag_Initial,
   AzLayoutWidthValueTag_Exact,
} AzLayoutWidthValueTag;

typedef struct { AzLayoutWidthValueTag tag; } AzLayoutWidthValueVariant_Auto;
typedef struct { AzLayoutWidthValueTag tag; } AzLayoutWidthValueVariant_None;
typedef struct { AzLayoutWidthValueTag tag; } AzLayoutWidthValueVariant_Inherit;
typedef struct { AzLayoutWidthValueTag tag; } AzLayoutWidthValueVariant_Initial;
typedef struct { AzLayoutWidthValueTag tag; AzLayoutWidth payload; } AzLayoutWidthValueVariant_Exact;

typedef union {
    AzLayoutWidthValueVariant_Auto Auto;
    AzLayoutWidthValueVariant_None None;
    AzLayoutWidthValueVariant_Inherit Inherit;
    AzLayoutWidthValueVariant_Initial Initial;
    AzLayoutWidthValueVariant_Exact Exact;
} AzLayoutWidthValue;

typedef enum {
   AzLayoutFlexWrapValueTag_Auto,
   AzLayoutFlexWrapValueTag_None,
   AzLayoutFlexWrapValueTag_Inherit,
   AzLayoutFlexWrapValueTag_Initial,
   AzLayoutFlexWrapValueTag_Exact,
} AzLayoutFlexWrapValueTag;

typedef struct { AzLayoutFlexWrapValueTag tag; } AzLayoutFlexWrapValueVariant_Auto;
typedef struct { AzLayoutFlexWrapValueTag tag; } AzLayoutFlexWrapValueVariant_None;
typedef struct { AzLayoutFlexWrapValueTag tag; } AzLayoutFlexWrapValueVariant_Inherit;
typedef struct { AzLayoutFlexWrapValueTag tag; } AzLayoutFlexWrapValueVariant_Initial;
typedef struct { AzLayoutFlexWrapValueTag tag; AzLayoutFlexWrap payload; } AzLayoutFlexWrapValueVariant_Exact;

typedef union {
    AzLayoutFlexWrapValueVariant_Auto Auto;
    AzLayoutFlexWrapValueVariant_None None;
    AzLayoutFlexWrapValueVariant_Inherit Inherit;
    AzLayoutFlexWrapValueVariant_Initial Initial;
    AzLayoutFlexWrapValueVariant_Exact Exact;
} AzLayoutFlexWrapValue;

typedef enum {
   AzLayoutOverflowValueTag_Auto,
   AzLayoutOverflowValueTag_None,
   AzLayoutOverflowValueTag_Inherit,
   AzLayoutOverflowValueTag_Initial,
   AzLayoutOverflowValueTag_Exact,
} AzLayoutOverflowValueTag;

typedef struct { AzLayoutOverflowValueTag tag; } AzLayoutOverflowValueVariant_Auto;
typedef struct { AzLayoutOverflowValueTag tag; } AzLayoutOverflowValueVariant_None;
typedef struct { AzLayoutOverflowValueTag tag; } AzLayoutOverflowValueVariant_Inherit;
typedef struct { AzLayoutOverflowValueTag tag; } AzLayoutOverflowValueVariant_Initial;
typedef struct { AzLayoutOverflowValueTag tag; AzLayoutOverflow payload; } AzLayoutOverflowValueVariant_Exact;

typedef union {
    AzLayoutOverflowValueVariant_Auto Auto;
    AzLayoutOverflowValueVariant_None None;
    AzLayoutOverflowValueVariant_Inherit Inherit;
    AzLayoutOverflowValueVariant_Initial Initial;
    AzLayoutOverflowValueVariant_Exact Exact;
} AzLayoutOverflowValue;

typedef enum {
   AzStyleBorderBottomColorValueTag_Auto,
   AzStyleBorderBottomColorValueTag_None,
   AzStyleBorderBottomColorValueTag_Inherit,
   AzStyleBorderBottomColorValueTag_Initial,
   AzStyleBorderBottomColorValueTag_Exact,
} AzStyleBorderBottomColorValueTag;

typedef struct { AzStyleBorderBottomColorValueTag tag; } AzStyleBorderBottomColorValueVariant_Auto;
typedef struct { AzStyleBorderBottomColorValueTag tag; } AzStyleBorderBottomColorValueVariant_None;
typedef struct { AzStyleBorderBottomColorValueTag tag; } AzStyleBorderBottomColorValueVariant_Inherit;
typedef struct { AzStyleBorderBottomColorValueTag tag; } AzStyleBorderBottomColorValueVariant_Initial;
typedef struct { AzStyleBorderBottomColorValueTag tag; AzStyleBorderBottomColor payload; } AzStyleBorderBottomColorValueVariant_Exact;

typedef union {
    AzStyleBorderBottomColorValueVariant_Auto Auto;
    AzStyleBorderBottomColorValueVariant_None None;
    AzStyleBorderBottomColorValueVariant_Inherit Inherit;
    AzStyleBorderBottomColorValueVariant_Initial Initial;
    AzStyleBorderBottomColorValueVariant_Exact Exact;
} AzStyleBorderBottomColorValue;

typedef enum {
   AzStyleBorderBottomLeftRadiusValueTag_Auto,
   AzStyleBorderBottomLeftRadiusValueTag_None,
   AzStyleBorderBottomLeftRadiusValueTag_Inherit,
   AzStyleBorderBottomLeftRadiusValueTag_Initial,
   AzStyleBorderBottomLeftRadiusValueTag_Exact,
} AzStyleBorderBottomLeftRadiusValueTag;

typedef struct { AzStyleBorderBottomLeftRadiusValueTag tag; } AzStyleBorderBottomLeftRadiusValueVariant_Auto;
typedef struct { AzStyleBorderBottomLeftRadiusValueTag tag; } AzStyleBorderBottomLeftRadiusValueVariant_None;
typedef struct { AzStyleBorderBottomLeftRadiusValueTag tag; } AzStyleBorderBottomLeftRadiusValueVariant_Inherit;
typedef struct { AzStyleBorderBottomLeftRadiusValueTag tag; } AzStyleBorderBottomLeftRadiusValueVariant_Initial;
typedef struct { AzStyleBorderBottomLeftRadiusValueTag tag; AzStyleBorderBottomLeftRadius payload; } AzStyleBorderBottomLeftRadiusValueVariant_Exact;

typedef union {
    AzStyleBorderBottomLeftRadiusValueVariant_Auto Auto;
    AzStyleBorderBottomLeftRadiusValueVariant_None None;
    AzStyleBorderBottomLeftRadiusValueVariant_Inherit Inherit;
    AzStyleBorderBottomLeftRadiusValueVariant_Initial Initial;
    AzStyleBorderBottomLeftRadiusValueVariant_Exact Exact;
} AzStyleBorderBottomLeftRadiusValue;

typedef enum {
   AzStyleBorderBottomRightRadiusValueTag_Auto,
   AzStyleBorderBottomRightRadiusValueTag_None,
   AzStyleBorderBottomRightRadiusValueTag_Inherit,
   AzStyleBorderBottomRightRadiusValueTag_Initial,
   AzStyleBorderBottomRightRadiusValueTag_Exact,
} AzStyleBorderBottomRightRadiusValueTag;

typedef struct { AzStyleBorderBottomRightRadiusValueTag tag; } AzStyleBorderBottomRightRadiusValueVariant_Auto;
typedef struct { AzStyleBorderBottomRightRadiusValueTag tag; } AzStyleBorderBottomRightRadiusValueVariant_None;
typedef struct { AzStyleBorderBottomRightRadiusValueTag tag; } AzStyleBorderBottomRightRadiusValueVariant_Inherit;
typedef struct { AzStyleBorderBottomRightRadiusValueTag tag; } AzStyleBorderBottomRightRadiusValueVariant_Initial;
typedef struct { AzStyleBorderBottomRightRadiusValueTag tag; AzStyleBorderBottomRightRadius payload; } AzStyleBorderBottomRightRadiusValueVariant_Exact;

typedef union {
    AzStyleBorderBottomRightRadiusValueVariant_Auto Auto;
    AzStyleBorderBottomRightRadiusValueVariant_None None;
    AzStyleBorderBottomRightRadiusValueVariant_Inherit Inherit;
    AzStyleBorderBottomRightRadiusValueVariant_Initial Initial;
    AzStyleBorderBottomRightRadiusValueVariant_Exact Exact;
} AzStyleBorderBottomRightRadiusValue;

typedef enum {
   AzStyleBorderBottomStyleValueTag_Auto,
   AzStyleBorderBottomStyleValueTag_None,
   AzStyleBorderBottomStyleValueTag_Inherit,
   AzStyleBorderBottomStyleValueTag_Initial,
   AzStyleBorderBottomStyleValueTag_Exact,
} AzStyleBorderBottomStyleValueTag;

typedef struct { AzStyleBorderBottomStyleValueTag tag; } AzStyleBorderBottomStyleValueVariant_Auto;
typedef struct { AzStyleBorderBottomStyleValueTag tag; } AzStyleBorderBottomStyleValueVariant_None;
typedef struct { AzStyleBorderBottomStyleValueTag tag; } AzStyleBorderBottomStyleValueVariant_Inherit;
typedef struct { AzStyleBorderBottomStyleValueTag tag; } AzStyleBorderBottomStyleValueVariant_Initial;
typedef struct { AzStyleBorderBottomStyleValueTag tag; AzStyleBorderBottomStyle payload; } AzStyleBorderBottomStyleValueVariant_Exact;

typedef union {
    AzStyleBorderBottomStyleValueVariant_Auto Auto;
    AzStyleBorderBottomStyleValueVariant_None None;
    AzStyleBorderBottomStyleValueVariant_Inherit Inherit;
    AzStyleBorderBottomStyleValueVariant_Initial Initial;
    AzStyleBorderBottomStyleValueVariant_Exact Exact;
} AzStyleBorderBottomStyleValue;

typedef enum {
   AzLayoutBorderBottomWidthValueTag_Auto,
   AzLayoutBorderBottomWidthValueTag_None,
   AzLayoutBorderBottomWidthValueTag_Inherit,
   AzLayoutBorderBottomWidthValueTag_Initial,
   AzLayoutBorderBottomWidthValueTag_Exact,
} AzLayoutBorderBottomWidthValueTag;

typedef struct { AzLayoutBorderBottomWidthValueTag tag; } AzLayoutBorderBottomWidthValueVariant_Auto;
typedef struct { AzLayoutBorderBottomWidthValueTag tag; } AzLayoutBorderBottomWidthValueVariant_None;
typedef struct { AzLayoutBorderBottomWidthValueTag tag; } AzLayoutBorderBottomWidthValueVariant_Inherit;
typedef struct { AzLayoutBorderBottomWidthValueTag tag; } AzLayoutBorderBottomWidthValueVariant_Initial;
typedef struct { AzLayoutBorderBottomWidthValueTag tag; AzLayoutBorderBottomWidth payload; } AzLayoutBorderBottomWidthValueVariant_Exact;

typedef union {
    AzLayoutBorderBottomWidthValueVariant_Auto Auto;
    AzLayoutBorderBottomWidthValueVariant_None None;
    AzLayoutBorderBottomWidthValueVariant_Inherit Inherit;
    AzLayoutBorderBottomWidthValueVariant_Initial Initial;
    AzLayoutBorderBottomWidthValueVariant_Exact Exact;
} AzLayoutBorderBottomWidthValue;

typedef enum {
   AzStyleBorderLeftColorValueTag_Auto,
   AzStyleBorderLeftColorValueTag_None,
   AzStyleBorderLeftColorValueTag_Inherit,
   AzStyleBorderLeftColorValueTag_Initial,
   AzStyleBorderLeftColorValueTag_Exact,
} AzStyleBorderLeftColorValueTag;

typedef struct { AzStyleBorderLeftColorValueTag tag; } AzStyleBorderLeftColorValueVariant_Auto;
typedef struct { AzStyleBorderLeftColorValueTag tag; } AzStyleBorderLeftColorValueVariant_None;
typedef struct { AzStyleBorderLeftColorValueTag tag; } AzStyleBorderLeftColorValueVariant_Inherit;
typedef struct { AzStyleBorderLeftColorValueTag tag; } AzStyleBorderLeftColorValueVariant_Initial;
typedef struct { AzStyleBorderLeftColorValueTag tag; AzStyleBorderLeftColor payload; } AzStyleBorderLeftColorValueVariant_Exact;

typedef union {
    AzStyleBorderLeftColorValueVariant_Auto Auto;
    AzStyleBorderLeftColorValueVariant_None None;
    AzStyleBorderLeftColorValueVariant_Inherit Inherit;
    AzStyleBorderLeftColorValueVariant_Initial Initial;
    AzStyleBorderLeftColorValueVariant_Exact Exact;
} AzStyleBorderLeftColorValue;

typedef enum {
   AzStyleBorderLeftStyleValueTag_Auto,
   AzStyleBorderLeftStyleValueTag_None,
   AzStyleBorderLeftStyleValueTag_Inherit,
   AzStyleBorderLeftStyleValueTag_Initial,
   AzStyleBorderLeftStyleValueTag_Exact,
} AzStyleBorderLeftStyleValueTag;

typedef struct { AzStyleBorderLeftStyleValueTag tag; } AzStyleBorderLeftStyleValueVariant_Auto;
typedef struct { AzStyleBorderLeftStyleValueTag tag; } AzStyleBorderLeftStyleValueVariant_None;
typedef struct { AzStyleBorderLeftStyleValueTag tag; } AzStyleBorderLeftStyleValueVariant_Inherit;
typedef struct { AzStyleBorderLeftStyleValueTag tag; } AzStyleBorderLeftStyleValueVariant_Initial;
typedef struct { AzStyleBorderLeftStyleValueTag tag; AzStyleBorderLeftStyle payload; } AzStyleBorderLeftStyleValueVariant_Exact;

typedef union {
    AzStyleBorderLeftStyleValueVariant_Auto Auto;
    AzStyleBorderLeftStyleValueVariant_None None;
    AzStyleBorderLeftStyleValueVariant_Inherit Inherit;
    AzStyleBorderLeftStyleValueVariant_Initial Initial;
    AzStyleBorderLeftStyleValueVariant_Exact Exact;
} AzStyleBorderLeftStyleValue;

typedef enum {
   AzLayoutBorderLeftWidthValueTag_Auto,
   AzLayoutBorderLeftWidthValueTag_None,
   AzLayoutBorderLeftWidthValueTag_Inherit,
   AzLayoutBorderLeftWidthValueTag_Initial,
   AzLayoutBorderLeftWidthValueTag_Exact,
} AzLayoutBorderLeftWidthValueTag;

typedef struct { AzLayoutBorderLeftWidthValueTag tag; } AzLayoutBorderLeftWidthValueVariant_Auto;
typedef struct { AzLayoutBorderLeftWidthValueTag tag; } AzLayoutBorderLeftWidthValueVariant_None;
typedef struct { AzLayoutBorderLeftWidthValueTag tag; } AzLayoutBorderLeftWidthValueVariant_Inherit;
typedef struct { AzLayoutBorderLeftWidthValueTag tag; } AzLayoutBorderLeftWidthValueVariant_Initial;
typedef struct { AzLayoutBorderLeftWidthValueTag tag; AzLayoutBorderLeftWidth payload; } AzLayoutBorderLeftWidthValueVariant_Exact;

typedef union {
    AzLayoutBorderLeftWidthValueVariant_Auto Auto;
    AzLayoutBorderLeftWidthValueVariant_None None;
    AzLayoutBorderLeftWidthValueVariant_Inherit Inherit;
    AzLayoutBorderLeftWidthValueVariant_Initial Initial;
    AzLayoutBorderLeftWidthValueVariant_Exact Exact;
} AzLayoutBorderLeftWidthValue;

typedef enum {
   AzStyleBorderRightColorValueTag_Auto,
   AzStyleBorderRightColorValueTag_None,
   AzStyleBorderRightColorValueTag_Inherit,
   AzStyleBorderRightColorValueTag_Initial,
   AzStyleBorderRightColorValueTag_Exact,
} AzStyleBorderRightColorValueTag;

typedef struct { AzStyleBorderRightColorValueTag tag; } AzStyleBorderRightColorValueVariant_Auto;
typedef struct { AzStyleBorderRightColorValueTag tag; } AzStyleBorderRightColorValueVariant_None;
typedef struct { AzStyleBorderRightColorValueTag tag; } AzStyleBorderRightColorValueVariant_Inherit;
typedef struct { AzStyleBorderRightColorValueTag tag; } AzStyleBorderRightColorValueVariant_Initial;
typedef struct { AzStyleBorderRightColorValueTag tag; AzStyleBorderRightColor payload; } AzStyleBorderRightColorValueVariant_Exact;

typedef union {
    AzStyleBorderRightColorValueVariant_Auto Auto;
    AzStyleBorderRightColorValueVariant_None None;
    AzStyleBorderRightColorValueVariant_Inherit Inherit;
    AzStyleBorderRightColorValueVariant_Initial Initial;
    AzStyleBorderRightColorValueVariant_Exact Exact;
} AzStyleBorderRightColorValue;

typedef enum {
   AzStyleBorderRightStyleValueTag_Auto,
   AzStyleBorderRightStyleValueTag_None,
   AzStyleBorderRightStyleValueTag_Inherit,
   AzStyleBorderRightStyleValueTag_Initial,
   AzStyleBorderRightStyleValueTag_Exact,
} AzStyleBorderRightStyleValueTag;

typedef struct { AzStyleBorderRightStyleValueTag tag; } AzStyleBorderRightStyleValueVariant_Auto;
typedef struct { AzStyleBorderRightStyleValueTag tag; } AzStyleBorderRightStyleValueVariant_None;
typedef struct { AzStyleBorderRightStyleValueTag tag; } AzStyleBorderRightStyleValueVariant_Inherit;
typedef struct { AzStyleBorderRightStyleValueTag tag; } AzStyleBorderRightStyleValueVariant_Initial;
typedef struct { AzStyleBorderRightStyleValueTag tag; AzStyleBorderRightStyle payload; } AzStyleBorderRightStyleValueVariant_Exact;

typedef union {
    AzStyleBorderRightStyleValueVariant_Auto Auto;
    AzStyleBorderRightStyleValueVariant_None None;
    AzStyleBorderRightStyleValueVariant_Inherit Inherit;
    AzStyleBorderRightStyleValueVariant_Initial Initial;
    AzStyleBorderRightStyleValueVariant_Exact Exact;
} AzStyleBorderRightStyleValue;

typedef enum {
   AzLayoutBorderRightWidthValueTag_Auto,
   AzLayoutBorderRightWidthValueTag_None,
   AzLayoutBorderRightWidthValueTag_Inherit,
   AzLayoutBorderRightWidthValueTag_Initial,
   AzLayoutBorderRightWidthValueTag_Exact,
} AzLayoutBorderRightWidthValueTag;

typedef struct { AzLayoutBorderRightWidthValueTag tag; } AzLayoutBorderRightWidthValueVariant_Auto;
typedef struct { AzLayoutBorderRightWidthValueTag tag; } AzLayoutBorderRightWidthValueVariant_None;
typedef struct { AzLayoutBorderRightWidthValueTag tag; } AzLayoutBorderRightWidthValueVariant_Inherit;
typedef struct { AzLayoutBorderRightWidthValueTag tag; } AzLayoutBorderRightWidthValueVariant_Initial;
typedef struct { AzLayoutBorderRightWidthValueTag tag; AzLayoutBorderRightWidth payload; } AzLayoutBorderRightWidthValueVariant_Exact;

typedef union {
    AzLayoutBorderRightWidthValueVariant_Auto Auto;
    AzLayoutBorderRightWidthValueVariant_None None;
    AzLayoutBorderRightWidthValueVariant_Inherit Inherit;
    AzLayoutBorderRightWidthValueVariant_Initial Initial;
    AzLayoutBorderRightWidthValueVariant_Exact Exact;
} AzLayoutBorderRightWidthValue;

typedef enum {
   AzStyleBorderTopColorValueTag_Auto,
   AzStyleBorderTopColorValueTag_None,
   AzStyleBorderTopColorValueTag_Inherit,
   AzStyleBorderTopColorValueTag_Initial,
   AzStyleBorderTopColorValueTag_Exact,
} AzStyleBorderTopColorValueTag;

typedef struct { AzStyleBorderTopColorValueTag tag; } AzStyleBorderTopColorValueVariant_Auto;
typedef struct { AzStyleBorderTopColorValueTag tag; } AzStyleBorderTopColorValueVariant_None;
typedef struct { AzStyleBorderTopColorValueTag tag; } AzStyleBorderTopColorValueVariant_Inherit;
typedef struct { AzStyleBorderTopColorValueTag tag; } AzStyleBorderTopColorValueVariant_Initial;
typedef struct { AzStyleBorderTopColorValueTag tag; AzStyleBorderTopColor payload; } AzStyleBorderTopColorValueVariant_Exact;

typedef union {
    AzStyleBorderTopColorValueVariant_Auto Auto;
    AzStyleBorderTopColorValueVariant_None None;
    AzStyleBorderTopColorValueVariant_Inherit Inherit;
    AzStyleBorderTopColorValueVariant_Initial Initial;
    AzStyleBorderTopColorValueVariant_Exact Exact;
} AzStyleBorderTopColorValue;

typedef enum {
   AzStyleBorderTopLeftRadiusValueTag_Auto,
   AzStyleBorderTopLeftRadiusValueTag_None,
   AzStyleBorderTopLeftRadiusValueTag_Inherit,
   AzStyleBorderTopLeftRadiusValueTag_Initial,
   AzStyleBorderTopLeftRadiusValueTag_Exact,
} AzStyleBorderTopLeftRadiusValueTag;

typedef struct { AzStyleBorderTopLeftRadiusValueTag tag; } AzStyleBorderTopLeftRadiusValueVariant_Auto;
typedef struct { AzStyleBorderTopLeftRadiusValueTag tag; } AzStyleBorderTopLeftRadiusValueVariant_None;
typedef struct { AzStyleBorderTopLeftRadiusValueTag tag; } AzStyleBorderTopLeftRadiusValueVariant_Inherit;
typedef struct { AzStyleBorderTopLeftRadiusValueTag tag; } AzStyleBorderTopLeftRadiusValueVariant_Initial;
typedef struct { AzStyleBorderTopLeftRadiusValueTag tag; AzStyleBorderTopLeftRadius payload; } AzStyleBorderTopLeftRadiusValueVariant_Exact;

typedef union {
    AzStyleBorderTopLeftRadiusValueVariant_Auto Auto;
    AzStyleBorderTopLeftRadiusValueVariant_None None;
    AzStyleBorderTopLeftRadiusValueVariant_Inherit Inherit;
    AzStyleBorderTopLeftRadiusValueVariant_Initial Initial;
    AzStyleBorderTopLeftRadiusValueVariant_Exact Exact;
} AzStyleBorderTopLeftRadiusValue;

typedef enum {
   AzStyleBorderTopRightRadiusValueTag_Auto,
   AzStyleBorderTopRightRadiusValueTag_None,
   AzStyleBorderTopRightRadiusValueTag_Inherit,
   AzStyleBorderTopRightRadiusValueTag_Initial,
   AzStyleBorderTopRightRadiusValueTag_Exact,
} AzStyleBorderTopRightRadiusValueTag;

typedef struct { AzStyleBorderTopRightRadiusValueTag tag; } AzStyleBorderTopRightRadiusValueVariant_Auto;
typedef struct { AzStyleBorderTopRightRadiusValueTag tag; } AzStyleBorderTopRightRadiusValueVariant_None;
typedef struct { AzStyleBorderTopRightRadiusValueTag tag; } AzStyleBorderTopRightRadiusValueVariant_Inherit;
typedef struct { AzStyleBorderTopRightRadiusValueTag tag; } AzStyleBorderTopRightRadiusValueVariant_Initial;
typedef struct { AzStyleBorderTopRightRadiusValueTag tag; AzStyleBorderTopRightRadius payload; } AzStyleBorderTopRightRadiusValueVariant_Exact;

typedef union {
    AzStyleBorderTopRightRadiusValueVariant_Auto Auto;
    AzStyleBorderTopRightRadiusValueVariant_None None;
    AzStyleBorderTopRightRadiusValueVariant_Inherit Inherit;
    AzStyleBorderTopRightRadiusValueVariant_Initial Initial;
    AzStyleBorderTopRightRadiusValueVariant_Exact Exact;
} AzStyleBorderTopRightRadiusValue;

typedef enum {
   AzStyleBorderTopStyleValueTag_Auto,
   AzStyleBorderTopStyleValueTag_None,
   AzStyleBorderTopStyleValueTag_Inherit,
   AzStyleBorderTopStyleValueTag_Initial,
   AzStyleBorderTopStyleValueTag_Exact,
} AzStyleBorderTopStyleValueTag;

typedef struct { AzStyleBorderTopStyleValueTag tag; } AzStyleBorderTopStyleValueVariant_Auto;
typedef struct { AzStyleBorderTopStyleValueTag tag; } AzStyleBorderTopStyleValueVariant_None;
typedef struct { AzStyleBorderTopStyleValueTag tag; } AzStyleBorderTopStyleValueVariant_Inherit;
typedef struct { AzStyleBorderTopStyleValueTag tag; } AzStyleBorderTopStyleValueVariant_Initial;
typedef struct { AzStyleBorderTopStyleValueTag tag; AzStyleBorderTopStyle payload; } AzStyleBorderTopStyleValueVariant_Exact;

typedef union {
    AzStyleBorderTopStyleValueVariant_Auto Auto;
    AzStyleBorderTopStyleValueVariant_None None;
    AzStyleBorderTopStyleValueVariant_Inherit Inherit;
    AzStyleBorderTopStyleValueVariant_Initial Initial;
    AzStyleBorderTopStyleValueVariant_Exact Exact;
} AzStyleBorderTopStyleValue;

typedef enum {
   AzLayoutBorderTopWidthValueTag_Auto,
   AzLayoutBorderTopWidthValueTag_None,
   AzLayoutBorderTopWidthValueTag_Inherit,
   AzLayoutBorderTopWidthValueTag_Initial,
   AzLayoutBorderTopWidthValueTag_Exact,
} AzLayoutBorderTopWidthValueTag;

typedef struct { AzLayoutBorderTopWidthValueTag tag; } AzLayoutBorderTopWidthValueVariant_Auto;
typedef struct { AzLayoutBorderTopWidthValueTag tag; } AzLayoutBorderTopWidthValueVariant_None;
typedef struct { AzLayoutBorderTopWidthValueTag tag; } AzLayoutBorderTopWidthValueVariant_Inherit;
typedef struct { AzLayoutBorderTopWidthValueTag tag; } AzLayoutBorderTopWidthValueVariant_Initial;
typedef struct { AzLayoutBorderTopWidthValueTag tag; AzLayoutBorderTopWidth payload; } AzLayoutBorderTopWidthValueVariant_Exact;

typedef union {
    AzLayoutBorderTopWidthValueVariant_Auto Auto;
    AzLayoutBorderTopWidthValueVariant_None None;
    AzLayoutBorderTopWidthValueVariant_Inherit Inherit;
    AzLayoutBorderTopWidthValueVariant_Initial Initial;
    AzLayoutBorderTopWidthValueVariant_Exact Exact;
} AzLayoutBorderTopWidthValue;

typedef enum {
   AzStyleCursorValueTag_Auto,
   AzStyleCursorValueTag_None,
   AzStyleCursorValueTag_Inherit,
   AzStyleCursorValueTag_Initial,
   AzStyleCursorValueTag_Exact,
} AzStyleCursorValueTag;

typedef struct { AzStyleCursorValueTag tag; } AzStyleCursorValueVariant_Auto;
typedef struct { AzStyleCursorValueTag tag; } AzStyleCursorValueVariant_None;
typedef struct { AzStyleCursorValueTag tag; } AzStyleCursorValueVariant_Inherit;
typedef struct { AzStyleCursorValueTag tag; } AzStyleCursorValueVariant_Initial;
typedef struct { AzStyleCursorValueTag tag; AzStyleCursor payload; } AzStyleCursorValueVariant_Exact;

typedef union {
    AzStyleCursorValueVariant_Auto Auto;
    AzStyleCursorValueVariant_None None;
    AzStyleCursorValueVariant_Inherit Inherit;
    AzStyleCursorValueVariant_Initial Initial;
    AzStyleCursorValueVariant_Exact Exact;
} AzStyleCursorValue;

typedef enum {
   AzStyleFontSizeValueTag_Auto,
   AzStyleFontSizeValueTag_None,
   AzStyleFontSizeValueTag_Inherit,
   AzStyleFontSizeValueTag_Initial,
   AzStyleFontSizeValueTag_Exact,
} AzStyleFontSizeValueTag;

typedef struct { AzStyleFontSizeValueTag tag; } AzStyleFontSizeValueVariant_Auto;
typedef struct { AzStyleFontSizeValueTag tag; } AzStyleFontSizeValueVariant_None;
typedef struct { AzStyleFontSizeValueTag tag; } AzStyleFontSizeValueVariant_Inherit;
typedef struct { AzStyleFontSizeValueTag tag; } AzStyleFontSizeValueVariant_Initial;
typedef struct { AzStyleFontSizeValueTag tag; AzStyleFontSize payload; } AzStyleFontSizeValueVariant_Exact;

typedef union {
    AzStyleFontSizeValueVariant_Auto Auto;
    AzStyleFontSizeValueVariant_None None;
    AzStyleFontSizeValueVariant_Inherit Inherit;
    AzStyleFontSizeValueVariant_Initial Initial;
    AzStyleFontSizeValueVariant_Exact Exact;
} AzStyleFontSizeValue;

typedef enum {
   AzStyleLetterSpacingValueTag_Auto,
   AzStyleLetterSpacingValueTag_None,
   AzStyleLetterSpacingValueTag_Inherit,
   AzStyleLetterSpacingValueTag_Initial,
   AzStyleLetterSpacingValueTag_Exact,
} AzStyleLetterSpacingValueTag;

typedef struct { AzStyleLetterSpacingValueTag tag; } AzStyleLetterSpacingValueVariant_Auto;
typedef struct { AzStyleLetterSpacingValueTag tag; } AzStyleLetterSpacingValueVariant_None;
typedef struct { AzStyleLetterSpacingValueTag tag; } AzStyleLetterSpacingValueVariant_Inherit;
typedef struct { AzStyleLetterSpacingValueTag tag; } AzStyleLetterSpacingValueVariant_Initial;
typedef struct { AzStyleLetterSpacingValueTag tag; AzStyleLetterSpacing payload; } AzStyleLetterSpacingValueVariant_Exact;

typedef union {
    AzStyleLetterSpacingValueVariant_Auto Auto;
    AzStyleLetterSpacingValueVariant_None None;
    AzStyleLetterSpacingValueVariant_Inherit Inherit;
    AzStyleLetterSpacingValueVariant_Initial Initial;
    AzStyleLetterSpacingValueVariant_Exact Exact;
} AzStyleLetterSpacingValue;

typedef enum {
   AzStyleLineHeightValueTag_Auto,
   AzStyleLineHeightValueTag_None,
   AzStyleLineHeightValueTag_Inherit,
   AzStyleLineHeightValueTag_Initial,
   AzStyleLineHeightValueTag_Exact,
} AzStyleLineHeightValueTag;

typedef struct { AzStyleLineHeightValueTag tag; } AzStyleLineHeightValueVariant_Auto;
typedef struct { AzStyleLineHeightValueTag tag; } AzStyleLineHeightValueVariant_None;
typedef struct { AzStyleLineHeightValueTag tag; } AzStyleLineHeightValueVariant_Inherit;
typedef struct { AzStyleLineHeightValueTag tag; } AzStyleLineHeightValueVariant_Initial;
typedef struct { AzStyleLineHeightValueTag tag; AzStyleLineHeight payload; } AzStyleLineHeightValueVariant_Exact;

typedef union {
    AzStyleLineHeightValueVariant_Auto Auto;
    AzStyleLineHeightValueVariant_None None;
    AzStyleLineHeightValueVariant_Inherit Inherit;
    AzStyleLineHeightValueVariant_Initial Initial;
    AzStyleLineHeightValueVariant_Exact Exact;
} AzStyleLineHeightValue;

typedef enum {
   AzStyleTabWidthValueTag_Auto,
   AzStyleTabWidthValueTag_None,
   AzStyleTabWidthValueTag_Inherit,
   AzStyleTabWidthValueTag_Initial,
   AzStyleTabWidthValueTag_Exact,
} AzStyleTabWidthValueTag;

typedef struct { AzStyleTabWidthValueTag tag; } AzStyleTabWidthValueVariant_Auto;
typedef struct { AzStyleTabWidthValueTag tag; } AzStyleTabWidthValueVariant_None;
typedef struct { AzStyleTabWidthValueTag tag; } AzStyleTabWidthValueVariant_Inherit;
typedef struct { AzStyleTabWidthValueTag tag; } AzStyleTabWidthValueVariant_Initial;
typedef struct { AzStyleTabWidthValueTag tag; AzStyleTabWidth payload; } AzStyleTabWidthValueVariant_Exact;

typedef union {
    AzStyleTabWidthValueVariant_Auto Auto;
    AzStyleTabWidthValueVariant_None None;
    AzStyleTabWidthValueVariant_Inherit Inherit;
    AzStyleTabWidthValueVariant_Initial Initial;
    AzStyleTabWidthValueVariant_Exact Exact;
} AzStyleTabWidthValue;

typedef enum {
   AzStyleTextAlignmentHorzValueTag_Auto,
   AzStyleTextAlignmentHorzValueTag_None,
   AzStyleTextAlignmentHorzValueTag_Inherit,
   AzStyleTextAlignmentHorzValueTag_Initial,
   AzStyleTextAlignmentHorzValueTag_Exact,
} AzStyleTextAlignmentHorzValueTag;

typedef struct { AzStyleTextAlignmentHorzValueTag tag; } AzStyleTextAlignmentHorzValueVariant_Auto;
typedef struct { AzStyleTextAlignmentHorzValueTag tag; } AzStyleTextAlignmentHorzValueVariant_None;
typedef struct { AzStyleTextAlignmentHorzValueTag tag; } AzStyleTextAlignmentHorzValueVariant_Inherit;
typedef struct { AzStyleTextAlignmentHorzValueTag tag; } AzStyleTextAlignmentHorzValueVariant_Initial;
typedef struct { AzStyleTextAlignmentHorzValueTag tag; AzStyleTextAlignmentHorz payload; } AzStyleTextAlignmentHorzValueVariant_Exact;

typedef union {
    AzStyleTextAlignmentHorzValueVariant_Auto Auto;
    AzStyleTextAlignmentHorzValueVariant_None None;
    AzStyleTextAlignmentHorzValueVariant_Inherit Inherit;
    AzStyleTextAlignmentHorzValueVariant_Initial Initial;
    AzStyleTextAlignmentHorzValueVariant_Exact Exact;
} AzStyleTextAlignmentHorzValue;

typedef enum {
   AzStyleTextColorValueTag_Auto,
   AzStyleTextColorValueTag_None,
   AzStyleTextColorValueTag_Inherit,
   AzStyleTextColorValueTag_Initial,
   AzStyleTextColorValueTag_Exact,
} AzStyleTextColorValueTag;

typedef struct { AzStyleTextColorValueTag tag; } AzStyleTextColorValueVariant_Auto;
typedef struct { AzStyleTextColorValueTag tag; } AzStyleTextColorValueVariant_None;
typedef struct { AzStyleTextColorValueTag tag; } AzStyleTextColorValueVariant_Inherit;
typedef struct { AzStyleTextColorValueTag tag; } AzStyleTextColorValueVariant_Initial;
typedef struct { AzStyleTextColorValueTag tag; AzStyleTextColor payload; } AzStyleTextColorValueVariant_Exact;

typedef union {
    AzStyleTextColorValueVariant_Auto Auto;
    AzStyleTextColorValueVariant_None None;
    AzStyleTextColorValueVariant_Inherit Inherit;
    AzStyleTextColorValueVariant_Initial Initial;
    AzStyleTextColorValueVariant_Exact Exact;
} AzStyleTextColorValue;

typedef enum {
   AzStyleWordSpacingValueTag_Auto,
   AzStyleWordSpacingValueTag_None,
   AzStyleWordSpacingValueTag_Inherit,
   AzStyleWordSpacingValueTag_Initial,
   AzStyleWordSpacingValueTag_Exact,
} AzStyleWordSpacingValueTag;

typedef struct { AzStyleWordSpacingValueTag tag; } AzStyleWordSpacingValueVariant_Auto;
typedef struct { AzStyleWordSpacingValueTag tag; } AzStyleWordSpacingValueVariant_None;
typedef struct { AzStyleWordSpacingValueTag tag; } AzStyleWordSpacingValueVariant_Inherit;
typedef struct { AzStyleWordSpacingValueTag tag; } AzStyleWordSpacingValueVariant_Initial;
typedef struct { AzStyleWordSpacingValueTag tag; AzStyleWordSpacing payload; } AzStyleWordSpacingValueVariant_Exact;

typedef union {
    AzStyleWordSpacingValueVariant_Auto Auto;
    AzStyleWordSpacingValueVariant_None None;
    AzStyleWordSpacingValueVariant_Inherit Inherit;
    AzStyleWordSpacingValueVariant_Initial Initial;
    AzStyleWordSpacingValueVariant_Exact Exact;
} AzStyleWordSpacingValue;

typedef enum {
   AzStyleOpacityValueTag_Auto,
   AzStyleOpacityValueTag_None,
   AzStyleOpacityValueTag_Inherit,
   AzStyleOpacityValueTag_Initial,
   AzStyleOpacityValueTag_Exact,
} AzStyleOpacityValueTag;

typedef struct { AzStyleOpacityValueTag tag; } AzStyleOpacityValueVariant_Auto;
typedef struct { AzStyleOpacityValueTag tag; } AzStyleOpacityValueVariant_None;
typedef struct { AzStyleOpacityValueTag tag; } AzStyleOpacityValueVariant_Inherit;
typedef struct { AzStyleOpacityValueTag tag; } AzStyleOpacityValueVariant_Initial;
typedef struct { AzStyleOpacityValueTag tag; AzStyleOpacity payload; } AzStyleOpacityValueVariant_Exact;

typedef union {
    AzStyleOpacityValueVariant_Auto Auto;
    AzStyleOpacityValueVariant_None None;
    AzStyleOpacityValueVariant_Inherit Inherit;
    AzStyleOpacityValueVariant_Initial Initial;
    AzStyleOpacityValueVariant_Exact Exact;
} AzStyleOpacityValue;

typedef enum {
   AzStyleTransformOriginValueTag_Auto,
   AzStyleTransformOriginValueTag_None,
   AzStyleTransformOriginValueTag_Inherit,
   AzStyleTransformOriginValueTag_Initial,
   AzStyleTransformOriginValueTag_Exact,
} AzStyleTransformOriginValueTag;

typedef struct { AzStyleTransformOriginValueTag tag; } AzStyleTransformOriginValueVariant_Auto;
typedef struct { AzStyleTransformOriginValueTag tag; } AzStyleTransformOriginValueVariant_None;
typedef struct { AzStyleTransformOriginValueTag tag; } AzStyleTransformOriginValueVariant_Inherit;
typedef struct { AzStyleTransformOriginValueTag tag; } AzStyleTransformOriginValueVariant_Initial;
typedef struct { AzStyleTransformOriginValueTag tag; AzStyleTransformOrigin payload; } AzStyleTransformOriginValueVariant_Exact;

typedef union {
    AzStyleTransformOriginValueVariant_Auto Auto;
    AzStyleTransformOriginValueVariant_None None;
    AzStyleTransformOriginValueVariant_Inherit Inherit;
    AzStyleTransformOriginValueVariant_Initial Initial;
    AzStyleTransformOriginValueVariant_Exact Exact;
} AzStyleTransformOriginValue;

typedef enum {
   AzStylePerspectiveOriginValueTag_Auto,
   AzStylePerspectiveOriginValueTag_None,
   AzStylePerspectiveOriginValueTag_Inherit,
   AzStylePerspectiveOriginValueTag_Initial,
   AzStylePerspectiveOriginValueTag_Exact,
} AzStylePerspectiveOriginValueTag;

typedef struct { AzStylePerspectiveOriginValueTag tag; } AzStylePerspectiveOriginValueVariant_Auto;
typedef struct { AzStylePerspectiveOriginValueTag tag; } AzStylePerspectiveOriginValueVariant_None;
typedef struct { AzStylePerspectiveOriginValueTag tag; } AzStylePerspectiveOriginValueVariant_Inherit;
typedef struct { AzStylePerspectiveOriginValueTag tag; } AzStylePerspectiveOriginValueVariant_Initial;
typedef struct { AzStylePerspectiveOriginValueTag tag; AzStylePerspectiveOrigin payload; } AzStylePerspectiveOriginValueVariant_Exact;

typedef union {
    AzStylePerspectiveOriginValueVariant_Auto Auto;
    AzStylePerspectiveOriginValueVariant_None None;
    AzStylePerspectiveOriginValueVariant_Inherit Inherit;
    AzStylePerspectiveOriginValueVariant_Initial Initial;
    AzStylePerspectiveOriginValueVariant_Exact Exact;
} AzStylePerspectiveOriginValue;

typedef enum {
   AzStyleBackfaceVisibilityValueTag_Auto,
   AzStyleBackfaceVisibilityValueTag_None,
   AzStyleBackfaceVisibilityValueTag_Inherit,
   AzStyleBackfaceVisibilityValueTag_Initial,
   AzStyleBackfaceVisibilityValueTag_Exact,
} AzStyleBackfaceVisibilityValueTag;

typedef struct { AzStyleBackfaceVisibilityValueTag tag; } AzStyleBackfaceVisibilityValueVariant_Auto;
typedef struct { AzStyleBackfaceVisibilityValueTag tag; } AzStyleBackfaceVisibilityValueVariant_None;
typedef struct { AzStyleBackfaceVisibilityValueTag tag; } AzStyleBackfaceVisibilityValueVariant_Inherit;
typedef struct { AzStyleBackfaceVisibilityValueTag tag; } AzStyleBackfaceVisibilityValueVariant_Initial;
typedef struct { AzStyleBackfaceVisibilityValueTag tag; AzStyleBackfaceVisibility payload; } AzStyleBackfaceVisibilityValueVariant_Exact;

typedef union {
    AzStyleBackfaceVisibilityValueVariant_Auto Auto;
    AzStyleBackfaceVisibilityValueVariant_None None;
    AzStyleBackfaceVisibilityValueVariant_Inherit Inherit;
    AzStyleBackfaceVisibilityValueVariant_Initial Initial;
    AzStyleBackfaceVisibilityValueVariant_Exact Exact;
} AzStyleBackfaceVisibilityValue;

typedef struct {
    size_t parent;
    size_t previous_sibling;
    size_t next_sibling;
    size_t last_child;
} AzNode;

typedef struct {
    uint32_t index_in_parent;
    bool  is_last_child;
} AzCascadeInfo;

typedef struct {
    bool  normal;
    bool  hover;
    bool  active;
    bool  focused;
} AzStyledNodeState;

typedef struct {
    uint64_t inner;
} AzTagId;

typedef struct {
    size_t depth;
    AzNodeId node_id;
} AzParentWithNodeDepth;

typedef struct {
    int32_t _0;
    int32_t _1;
    int32_t _2;
} AzGlShaderPrecisionFormatReturn;

typedef struct {
    uint8_t* const ptr;
    size_t len;
} AzU8VecRef;

typedef struct {
    uint8_t* restrict ptr;
    size_t len;
} AzU8VecRefMut;

typedef struct {
    float* const ptr;
    size_t len;
} AzF32VecRef;

typedef struct {
    int32_t* const ptr;
    size_t len;
} AzI32VecRef;

typedef struct {
    uint32_t* const ptr;
    size_t len;
} AzGLuintVecRef;

typedef struct {
    uint32_t* const ptr;
    size_t len;
} AzGLenumVecRef;

typedef struct {
    int32_t* restrict ptr;
    size_t len;
} AzGLintVecRefMut;

typedef struct {
    int64_t* restrict ptr;
    size_t len;
} AzGLint64VecRefMut;

typedef struct {
    uint8_t* restrict ptr;
    size_t len;
} AzGLbooleanVecRefMut;

typedef struct {
    float* restrict ptr;
    size_t len;
} AzGLfloatVecRefMut;

typedef struct {
    uint8_t* const ptr;
    size_t len;
} AzRefstr;

typedef struct {
    bool  is_opaque;
    bool  is_video_texture;
} AzTextureFlags;

typedef struct {
    size_t id;
} AzImageId;

typedef struct {
    size_t id;
} AzFontId;

typedef struct {
    float center_x;
    float center_y;
    float radius;
} AzSvgCircle;

typedef struct {
    float x;
    float y;
} AzSvgPoint;

typedef struct {
    float x;
    float y;
} AzSvgVertex;

typedef struct {
    AzSvgPoint start;
    AzSvgPoint ctrl;
    AzSvgPoint end;
} AzSvgQuadraticCurve;

typedef struct {
    AzSvgPoint start;
    AzSvgPoint ctrl_1;
    AzSvgPoint ctrl_2;
    AzSvgPoint end;
} AzSvgCubicCurve;

typedef struct {
    float width;
    float height;
    float x;
    float y;
    float radius_top_left;
    float radius_top_right;
    float radius_bottom_left;
    float radius_bottom_right;
} AzSvgRect;

typedef enum {
   AzSvgFitToTag_Original,
   AzSvgFitToTag_Width,
   AzSvgFitToTag_Height,
   AzSvgFitToTag_Zoom,
} AzSvgFitToTag;

typedef struct { AzSvgFitToTag tag; } AzSvgFitToVariant_Original;
typedef struct { AzSvgFitToTag tag; uint32_t payload; } AzSvgFitToVariant_Width;
typedef struct { AzSvgFitToTag tag; uint32_t payload; } AzSvgFitToVariant_Height;
typedef struct { AzSvgFitToTag tag; float payload; } AzSvgFitToVariant_Zoom;

typedef union {
    AzSvgFitToVariant_Original Original;
    AzSvgFitToVariant_Width Width;
    AzSvgFitToVariant_Height Height;
    AzSvgFitToVariant_Zoom Zoom;
} AzSvgFitTo;

typedef struct {
    size_t offset;
    size_t length_1;
    size_t gap_1;
    size_t length_2;
    size_t gap_2;
    size_t length_3;
    size_t gap_3;
} AzSvgDashPattern;

typedef struct {
    AzSvgLineJoin line_join;
    size_t miter_limit;
    size_t tolerance;
} AzSvgFillStyle;

typedef struct {
    size_t id;
} AzTimerId;

typedef struct {
    size_t id;
} AzThreadId;

typedef struct {
    AzRefAny data;
    AzWriteBackCallback callback;
} AzThreadWriteBackMsg;

typedef struct {
    AzCreateThreadFnType cb;
} AzCreateThreadFn;

typedef struct {
    AzGetSystemTimeFnType cb;
} AzGetSystemTimeFn;

typedef struct {
    AzCheckThreadFinishedFnType cb;
} AzCheckThreadFinishedFn;

typedef struct {
    AzLibrarySendThreadMsgFnType cb;
} AzLibrarySendThreadMsgFn;

typedef struct {
    AzLibraryReceiveThreadMsgFnType cb;
} AzLibraryReceiveThreadMsgFn;

typedef struct {
    AzThreadRecvFnType cb;
} AzThreadRecvFn;

typedef struct {
    AzThreadSendFnType cb;
} AzThreadSendFn;

typedef struct {
    AzThreadDestructorFnType cb;
} AzThreadDestructorFn;

typedef struct {
    AzThreadReceiverDestructorFnType cb;
} AzThreadReceiverDestructorFn;

typedef struct {
    AzThreadSenderDestructorFnType cb;
} AzThreadSenderDestructorFn;

typedef enum {
   AzMonitorVecDestructorTag_DefaultRust,
   AzMonitorVecDestructorTag_NoDestructor,
   AzMonitorVecDestructorTag_External,
} AzMonitorVecDestructorTag;

typedef struct { AzMonitorVecDestructorTag tag; } AzMonitorVecDestructorVariant_DefaultRust;
typedef struct { AzMonitorVecDestructorTag tag; } AzMonitorVecDestructorVariant_NoDestructor;
typedef struct { AzMonitorVecDestructorTag tag; AzMonitorVecDestructorType payload; } AzMonitorVecDestructorVariant_External;

typedef union {
    AzMonitorVecDestructorVariant_DefaultRust DefaultRust;
    AzMonitorVecDestructorVariant_NoDestructor NoDestructor;
    AzMonitorVecDestructorVariant_External External;
} AzMonitorVecDestructor;

typedef enum {
   AzVideoModeVecDestructorTag_DefaultRust,
   AzVideoModeVecDestructorTag_NoDestructor,
   AzVideoModeVecDestructorTag_External,
} AzVideoModeVecDestructorTag;

typedef struct { AzVideoModeVecDestructorTag tag; } AzVideoModeVecDestructorVariant_DefaultRust;
typedef struct { AzVideoModeVecDestructorTag tag; } AzVideoModeVecDestructorVariant_NoDestructor;
typedef struct { AzVideoModeVecDestructorTag tag; AzVideoModeVecDestructorType payload; } AzVideoModeVecDestructorVariant_External;

typedef union {
    AzVideoModeVecDestructorVariant_DefaultRust DefaultRust;
    AzVideoModeVecDestructorVariant_NoDestructor NoDestructor;
    AzVideoModeVecDestructorVariant_External External;
} AzVideoModeVecDestructor;

typedef enum {
   AzDomVecDestructorTag_DefaultRust,
   AzDomVecDestructorTag_NoDestructor,
   AzDomVecDestructorTag_External,
} AzDomVecDestructorTag;

typedef struct { AzDomVecDestructorTag tag; } AzDomVecDestructorVariant_DefaultRust;
typedef struct { AzDomVecDestructorTag tag; } AzDomVecDestructorVariant_NoDestructor;
typedef struct { AzDomVecDestructorTag tag; AzDomVecDestructorType payload; } AzDomVecDestructorVariant_External;

typedef union {
    AzDomVecDestructorVariant_DefaultRust DefaultRust;
    AzDomVecDestructorVariant_NoDestructor NoDestructor;
    AzDomVecDestructorVariant_External External;
} AzDomVecDestructor;

typedef enum {
   AzIdOrClassVecDestructorTag_DefaultRust,
   AzIdOrClassVecDestructorTag_NoDestructor,
   AzIdOrClassVecDestructorTag_External,
} AzIdOrClassVecDestructorTag;

typedef struct { AzIdOrClassVecDestructorTag tag; } AzIdOrClassVecDestructorVariant_DefaultRust;
typedef struct { AzIdOrClassVecDestructorTag tag; } AzIdOrClassVecDestructorVariant_NoDestructor;
typedef struct { AzIdOrClassVecDestructorTag tag; AzIdOrClassVecDestructorType payload; } AzIdOrClassVecDestructorVariant_External;

typedef union {
    AzIdOrClassVecDestructorVariant_DefaultRust DefaultRust;
    AzIdOrClassVecDestructorVariant_NoDestructor NoDestructor;
    AzIdOrClassVecDestructorVariant_External External;
} AzIdOrClassVecDestructor;

typedef enum {
   AzNodeDataInlineCssPropertyVecDestructorTag_DefaultRust,
   AzNodeDataInlineCssPropertyVecDestructorTag_NoDestructor,
   AzNodeDataInlineCssPropertyVecDestructorTag_External,
} AzNodeDataInlineCssPropertyVecDestructorTag;

typedef struct { AzNodeDataInlineCssPropertyVecDestructorTag tag; } AzNodeDataInlineCssPropertyVecDestructorVariant_DefaultRust;
typedef struct { AzNodeDataInlineCssPropertyVecDestructorTag tag; } AzNodeDataInlineCssPropertyVecDestructorVariant_NoDestructor;
typedef struct { AzNodeDataInlineCssPropertyVecDestructorTag tag; AzNodeDataInlineCssPropertyVecDestructorType payload; } AzNodeDataInlineCssPropertyVecDestructorVariant_External;

typedef union {
    AzNodeDataInlineCssPropertyVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeDataInlineCssPropertyVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeDataInlineCssPropertyVecDestructorVariant_External External;
} AzNodeDataInlineCssPropertyVecDestructor;

typedef enum {
   AzStyleBackgroundContentVecDestructorTag_DefaultRust,
   AzStyleBackgroundContentVecDestructorTag_NoDestructor,
   AzStyleBackgroundContentVecDestructorTag_External,
} AzStyleBackgroundContentVecDestructorTag;

typedef struct { AzStyleBackgroundContentVecDestructorTag tag; } AzStyleBackgroundContentVecDestructorVariant_DefaultRust;
typedef struct { AzStyleBackgroundContentVecDestructorTag tag; } AzStyleBackgroundContentVecDestructorVariant_NoDestructor;
typedef struct { AzStyleBackgroundContentVecDestructorTag tag; AzStyleBackgroundContentVecDestructorType payload; } AzStyleBackgroundContentVecDestructorVariant_External;

typedef union {
    AzStyleBackgroundContentVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundContentVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundContentVecDestructorVariant_External External;
} AzStyleBackgroundContentVecDestructor;

typedef enum {
   AzStyleBackgroundPositionVecDestructorTag_DefaultRust,
   AzStyleBackgroundPositionVecDestructorTag_NoDestructor,
   AzStyleBackgroundPositionVecDestructorTag_External,
} AzStyleBackgroundPositionVecDestructorTag;

typedef struct { AzStyleBackgroundPositionVecDestructorTag tag; } AzStyleBackgroundPositionVecDestructorVariant_DefaultRust;
typedef struct { AzStyleBackgroundPositionVecDestructorTag tag; } AzStyleBackgroundPositionVecDestructorVariant_NoDestructor;
typedef struct { AzStyleBackgroundPositionVecDestructorTag tag; AzStyleBackgroundPositionVecDestructorType payload; } AzStyleBackgroundPositionVecDestructorVariant_External;

typedef union {
    AzStyleBackgroundPositionVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundPositionVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundPositionVecDestructorVariant_External External;
} AzStyleBackgroundPositionVecDestructor;

typedef enum {
   AzStyleBackgroundRepeatVecDestructorTag_DefaultRust,
   AzStyleBackgroundRepeatVecDestructorTag_NoDestructor,
   AzStyleBackgroundRepeatVecDestructorTag_External,
} AzStyleBackgroundRepeatVecDestructorTag;

typedef struct { AzStyleBackgroundRepeatVecDestructorTag tag; } AzStyleBackgroundRepeatVecDestructorVariant_DefaultRust;
typedef struct { AzStyleBackgroundRepeatVecDestructorTag tag; } AzStyleBackgroundRepeatVecDestructorVariant_NoDestructor;
typedef struct { AzStyleBackgroundRepeatVecDestructorTag tag; AzStyleBackgroundRepeatVecDestructorType payload; } AzStyleBackgroundRepeatVecDestructorVariant_External;

typedef union {
    AzStyleBackgroundRepeatVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundRepeatVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundRepeatVecDestructorVariant_External External;
} AzStyleBackgroundRepeatVecDestructor;

typedef enum {
   AzStyleBackgroundSizeVecDestructorTag_DefaultRust,
   AzStyleBackgroundSizeVecDestructorTag_NoDestructor,
   AzStyleBackgroundSizeVecDestructorTag_External,
} AzStyleBackgroundSizeVecDestructorTag;

typedef struct { AzStyleBackgroundSizeVecDestructorTag tag; } AzStyleBackgroundSizeVecDestructorVariant_DefaultRust;
typedef struct { AzStyleBackgroundSizeVecDestructorTag tag; } AzStyleBackgroundSizeVecDestructorVariant_NoDestructor;
typedef struct { AzStyleBackgroundSizeVecDestructorTag tag; AzStyleBackgroundSizeVecDestructorType payload; } AzStyleBackgroundSizeVecDestructorVariant_External;

typedef union {
    AzStyleBackgroundSizeVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundSizeVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundSizeVecDestructorVariant_External External;
} AzStyleBackgroundSizeVecDestructor;

typedef enum {
   AzStyleTransformVecDestructorTag_DefaultRust,
   AzStyleTransformVecDestructorTag_NoDestructor,
   AzStyleTransformVecDestructorTag_External,
} AzStyleTransformVecDestructorTag;

typedef struct { AzStyleTransformVecDestructorTag tag; } AzStyleTransformVecDestructorVariant_DefaultRust;
typedef struct { AzStyleTransformVecDestructorTag tag; } AzStyleTransformVecDestructorVariant_NoDestructor;
typedef struct { AzStyleTransformVecDestructorTag tag; AzStyleTransformVecDestructorType payload; } AzStyleTransformVecDestructorVariant_External;

typedef union {
    AzStyleTransformVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleTransformVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleTransformVecDestructorVariant_External External;
} AzStyleTransformVecDestructor;

typedef enum {
   AzCssPropertyVecDestructorTag_DefaultRust,
   AzCssPropertyVecDestructorTag_NoDestructor,
   AzCssPropertyVecDestructorTag_External,
} AzCssPropertyVecDestructorTag;

typedef struct { AzCssPropertyVecDestructorTag tag; } AzCssPropertyVecDestructorVariant_DefaultRust;
typedef struct { AzCssPropertyVecDestructorTag tag; } AzCssPropertyVecDestructorVariant_NoDestructor;
typedef struct { AzCssPropertyVecDestructorTag tag; AzCssPropertyVecDestructorType payload; } AzCssPropertyVecDestructorVariant_External;

typedef union {
    AzCssPropertyVecDestructorVariant_DefaultRust DefaultRust;
    AzCssPropertyVecDestructorVariant_NoDestructor NoDestructor;
    AzCssPropertyVecDestructorVariant_External External;
} AzCssPropertyVecDestructor;

typedef enum {
   AzSvgMultiPolygonVecDestructorTag_DefaultRust,
   AzSvgMultiPolygonVecDestructorTag_NoDestructor,
   AzSvgMultiPolygonVecDestructorTag_External,
} AzSvgMultiPolygonVecDestructorTag;

typedef struct { AzSvgMultiPolygonVecDestructorTag tag; } AzSvgMultiPolygonVecDestructorVariant_DefaultRust;
typedef struct { AzSvgMultiPolygonVecDestructorTag tag; } AzSvgMultiPolygonVecDestructorVariant_NoDestructor;
typedef struct { AzSvgMultiPolygonVecDestructorTag tag; AzSvgMultiPolygonVecDestructorType payload; } AzSvgMultiPolygonVecDestructorVariant_External;

typedef union {
    AzSvgMultiPolygonVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgMultiPolygonVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgMultiPolygonVecDestructorVariant_External External;
} AzSvgMultiPolygonVecDestructor;

typedef enum {
   AzSvgPathVecDestructorTag_DefaultRust,
   AzSvgPathVecDestructorTag_NoDestructor,
   AzSvgPathVecDestructorTag_External,
} AzSvgPathVecDestructorTag;

typedef struct { AzSvgPathVecDestructorTag tag; } AzSvgPathVecDestructorVariant_DefaultRust;
typedef struct { AzSvgPathVecDestructorTag tag; } AzSvgPathVecDestructorVariant_NoDestructor;
typedef struct { AzSvgPathVecDestructorTag tag; AzSvgPathVecDestructorType payload; } AzSvgPathVecDestructorVariant_External;

typedef union {
    AzSvgPathVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgPathVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgPathVecDestructorVariant_External External;
} AzSvgPathVecDestructor;

typedef enum {
   AzVertexAttributeVecDestructorTag_DefaultRust,
   AzVertexAttributeVecDestructorTag_NoDestructor,
   AzVertexAttributeVecDestructorTag_External,
} AzVertexAttributeVecDestructorTag;

typedef struct { AzVertexAttributeVecDestructorTag tag; } AzVertexAttributeVecDestructorVariant_DefaultRust;
typedef struct { AzVertexAttributeVecDestructorTag tag; } AzVertexAttributeVecDestructorVariant_NoDestructor;
typedef struct { AzVertexAttributeVecDestructorTag tag; AzVertexAttributeVecDestructorType payload; } AzVertexAttributeVecDestructorVariant_External;

typedef union {
    AzVertexAttributeVecDestructorVariant_DefaultRust DefaultRust;
    AzVertexAttributeVecDestructorVariant_NoDestructor NoDestructor;
    AzVertexAttributeVecDestructorVariant_External External;
} AzVertexAttributeVecDestructor;

typedef enum {
   AzSvgPathElementVecDestructorTag_DefaultRust,
   AzSvgPathElementVecDestructorTag_NoDestructor,
   AzSvgPathElementVecDestructorTag_External,
} AzSvgPathElementVecDestructorTag;

typedef struct { AzSvgPathElementVecDestructorTag tag; } AzSvgPathElementVecDestructorVariant_DefaultRust;
typedef struct { AzSvgPathElementVecDestructorTag tag; } AzSvgPathElementVecDestructorVariant_NoDestructor;
typedef struct { AzSvgPathElementVecDestructorTag tag; AzSvgPathElementVecDestructorType payload; } AzSvgPathElementVecDestructorVariant_External;

typedef union {
    AzSvgPathElementVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgPathElementVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgPathElementVecDestructorVariant_External External;
} AzSvgPathElementVecDestructor;

typedef enum {
   AzSvgVertexVecDestructorTag_DefaultRust,
   AzSvgVertexVecDestructorTag_NoDestructor,
   AzSvgVertexVecDestructorTag_External,
} AzSvgVertexVecDestructorTag;

typedef struct { AzSvgVertexVecDestructorTag tag; } AzSvgVertexVecDestructorVariant_DefaultRust;
typedef struct { AzSvgVertexVecDestructorTag tag; } AzSvgVertexVecDestructorVariant_NoDestructor;
typedef struct { AzSvgVertexVecDestructorTag tag; AzSvgVertexVecDestructorType payload; } AzSvgVertexVecDestructorVariant_External;

typedef union {
    AzSvgVertexVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgVertexVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgVertexVecDestructorVariant_External External;
} AzSvgVertexVecDestructor;

typedef enum {
   AzU32VecDestructorTag_DefaultRust,
   AzU32VecDestructorTag_NoDestructor,
   AzU32VecDestructorTag_External,
} AzU32VecDestructorTag;

typedef struct { AzU32VecDestructorTag tag; } AzU32VecDestructorVariant_DefaultRust;
typedef struct { AzU32VecDestructorTag tag; } AzU32VecDestructorVariant_NoDestructor;
typedef struct { AzU32VecDestructorTag tag; AzU32VecDestructorType payload; } AzU32VecDestructorVariant_External;

typedef union {
    AzU32VecDestructorVariant_DefaultRust DefaultRust;
    AzU32VecDestructorVariant_NoDestructor NoDestructor;
    AzU32VecDestructorVariant_External External;
} AzU32VecDestructor;

typedef enum {
   AzXWindowTypeVecDestructorTag_DefaultRust,
   AzXWindowTypeVecDestructorTag_NoDestructor,
   AzXWindowTypeVecDestructorTag_External,
} AzXWindowTypeVecDestructorTag;

typedef struct { AzXWindowTypeVecDestructorTag tag; } AzXWindowTypeVecDestructorVariant_DefaultRust;
typedef struct { AzXWindowTypeVecDestructorTag tag; } AzXWindowTypeVecDestructorVariant_NoDestructor;
typedef struct { AzXWindowTypeVecDestructorTag tag; AzXWindowTypeVecDestructorType payload; } AzXWindowTypeVecDestructorVariant_External;

typedef union {
    AzXWindowTypeVecDestructorVariant_DefaultRust DefaultRust;
    AzXWindowTypeVecDestructorVariant_NoDestructor NoDestructor;
    AzXWindowTypeVecDestructorVariant_External External;
} AzXWindowTypeVecDestructor;

typedef enum {
   AzVirtualKeyCodeVecDestructorTag_DefaultRust,
   AzVirtualKeyCodeVecDestructorTag_NoDestructor,
   AzVirtualKeyCodeVecDestructorTag_External,
} AzVirtualKeyCodeVecDestructorTag;

typedef struct { AzVirtualKeyCodeVecDestructorTag tag; } AzVirtualKeyCodeVecDestructorVariant_DefaultRust;
typedef struct { AzVirtualKeyCodeVecDestructorTag tag; } AzVirtualKeyCodeVecDestructorVariant_NoDestructor;
typedef struct { AzVirtualKeyCodeVecDestructorTag tag; AzVirtualKeyCodeVecDestructorType payload; } AzVirtualKeyCodeVecDestructorVariant_External;

typedef union {
    AzVirtualKeyCodeVecDestructorVariant_DefaultRust DefaultRust;
    AzVirtualKeyCodeVecDestructorVariant_NoDestructor NoDestructor;
    AzVirtualKeyCodeVecDestructorVariant_External External;
} AzVirtualKeyCodeVecDestructor;

typedef enum {
   AzCascadeInfoVecDestructorTag_DefaultRust,
   AzCascadeInfoVecDestructorTag_NoDestructor,
   AzCascadeInfoVecDestructorTag_External,
} AzCascadeInfoVecDestructorTag;

typedef struct { AzCascadeInfoVecDestructorTag tag; } AzCascadeInfoVecDestructorVariant_DefaultRust;
typedef struct { AzCascadeInfoVecDestructorTag tag; } AzCascadeInfoVecDestructorVariant_NoDestructor;
typedef struct { AzCascadeInfoVecDestructorTag tag; AzCascadeInfoVecDestructorType payload; } AzCascadeInfoVecDestructorVariant_External;

typedef union {
    AzCascadeInfoVecDestructorVariant_DefaultRust DefaultRust;
    AzCascadeInfoVecDestructorVariant_NoDestructor NoDestructor;
    AzCascadeInfoVecDestructorVariant_External External;
} AzCascadeInfoVecDestructor;

typedef enum {
   AzScanCodeVecDestructorTag_DefaultRust,
   AzScanCodeVecDestructorTag_NoDestructor,
   AzScanCodeVecDestructorTag_External,
} AzScanCodeVecDestructorTag;

typedef struct { AzScanCodeVecDestructorTag tag; } AzScanCodeVecDestructorVariant_DefaultRust;
typedef struct { AzScanCodeVecDestructorTag tag; } AzScanCodeVecDestructorVariant_NoDestructor;
typedef struct { AzScanCodeVecDestructorTag tag; AzScanCodeVecDestructorType payload; } AzScanCodeVecDestructorVariant_External;

typedef union {
    AzScanCodeVecDestructorVariant_DefaultRust DefaultRust;
    AzScanCodeVecDestructorVariant_NoDestructor NoDestructor;
    AzScanCodeVecDestructorVariant_External External;
} AzScanCodeVecDestructor;

typedef enum {
   AzCssDeclarationVecDestructorTag_DefaultRust,
   AzCssDeclarationVecDestructorTag_NoDestructor,
   AzCssDeclarationVecDestructorTag_External,
} AzCssDeclarationVecDestructorTag;

typedef struct { AzCssDeclarationVecDestructorTag tag; } AzCssDeclarationVecDestructorVariant_DefaultRust;
typedef struct { AzCssDeclarationVecDestructorTag tag; } AzCssDeclarationVecDestructorVariant_NoDestructor;
typedef struct { AzCssDeclarationVecDestructorTag tag; AzCssDeclarationVecDestructorType payload; } AzCssDeclarationVecDestructorVariant_External;

typedef union {
    AzCssDeclarationVecDestructorVariant_DefaultRust DefaultRust;
    AzCssDeclarationVecDestructorVariant_NoDestructor NoDestructor;
    AzCssDeclarationVecDestructorVariant_External External;
} AzCssDeclarationVecDestructor;

typedef enum {
   AzCssPathSelectorVecDestructorTag_DefaultRust,
   AzCssPathSelectorVecDestructorTag_NoDestructor,
   AzCssPathSelectorVecDestructorTag_External,
} AzCssPathSelectorVecDestructorTag;

typedef struct { AzCssPathSelectorVecDestructorTag tag; } AzCssPathSelectorVecDestructorVariant_DefaultRust;
typedef struct { AzCssPathSelectorVecDestructorTag tag; } AzCssPathSelectorVecDestructorVariant_NoDestructor;
typedef struct { AzCssPathSelectorVecDestructorTag tag; AzCssPathSelectorVecDestructorType payload; } AzCssPathSelectorVecDestructorVariant_External;

typedef union {
    AzCssPathSelectorVecDestructorVariant_DefaultRust DefaultRust;
    AzCssPathSelectorVecDestructorVariant_NoDestructor NoDestructor;
    AzCssPathSelectorVecDestructorVariant_External External;
} AzCssPathSelectorVecDestructor;

typedef enum {
   AzStylesheetVecDestructorTag_DefaultRust,
   AzStylesheetVecDestructorTag_NoDestructor,
   AzStylesheetVecDestructorTag_External,
} AzStylesheetVecDestructorTag;

typedef struct { AzStylesheetVecDestructorTag tag; } AzStylesheetVecDestructorVariant_DefaultRust;
typedef struct { AzStylesheetVecDestructorTag tag; } AzStylesheetVecDestructorVariant_NoDestructor;
typedef struct { AzStylesheetVecDestructorTag tag; AzStylesheetVecDestructorType payload; } AzStylesheetVecDestructorVariant_External;

typedef union {
    AzStylesheetVecDestructorVariant_DefaultRust DefaultRust;
    AzStylesheetVecDestructorVariant_NoDestructor NoDestructor;
    AzStylesheetVecDestructorVariant_External External;
} AzStylesheetVecDestructor;

typedef enum {
   AzCssRuleBlockVecDestructorTag_DefaultRust,
   AzCssRuleBlockVecDestructorTag_NoDestructor,
   AzCssRuleBlockVecDestructorTag_External,
} AzCssRuleBlockVecDestructorTag;

typedef struct { AzCssRuleBlockVecDestructorTag tag; } AzCssRuleBlockVecDestructorVariant_DefaultRust;
typedef struct { AzCssRuleBlockVecDestructorTag tag; } AzCssRuleBlockVecDestructorVariant_NoDestructor;
typedef struct { AzCssRuleBlockVecDestructorTag tag; AzCssRuleBlockVecDestructorType payload; } AzCssRuleBlockVecDestructorVariant_External;

typedef union {
    AzCssRuleBlockVecDestructorVariant_DefaultRust DefaultRust;
    AzCssRuleBlockVecDestructorVariant_NoDestructor NoDestructor;
    AzCssRuleBlockVecDestructorVariant_External External;
} AzCssRuleBlockVecDestructor;

typedef enum {
   AzU8VecDestructorTag_DefaultRust,
   AzU8VecDestructorTag_NoDestructor,
   AzU8VecDestructorTag_External,
} AzU8VecDestructorTag;

typedef struct { AzU8VecDestructorTag tag; } AzU8VecDestructorVariant_DefaultRust;
typedef struct { AzU8VecDestructorTag tag; } AzU8VecDestructorVariant_NoDestructor;
typedef struct { AzU8VecDestructorTag tag; AzU8VecDestructorType payload; } AzU8VecDestructorVariant_External;

typedef union {
    AzU8VecDestructorVariant_DefaultRust DefaultRust;
    AzU8VecDestructorVariant_NoDestructor NoDestructor;
    AzU8VecDestructorVariant_External External;
} AzU8VecDestructor;

typedef enum {
   AzCallbackDataVecDestructorTag_DefaultRust,
   AzCallbackDataVecDestructorTag_NoDestructor,
   AzCallbackDataVecDestructorTag_External,
} AzCallbackDataVecDestructorTag;

typedef struct { AzCallbackDataVecDestructorTag tag; } AzCallbackDataVecDestructorVariant_DefaultRust;
typedef struct { AzCallbackDataVecDestructorTag tag; } AzCallbackDataVecDestructorVariant_NoDestructor;
typedef struct { AzCallbackDataVecDestructorTag tag; AzCallbackDataVecDestructorType payload; } AzCallbackDataVecDestructorVariant_External;

typedef union {
    AzCallbackDataVecDestructorVariant_DefaultRust DefaultRust;
    AzCallbackDataVecDestructorVariant_NoDestructor NoDestructor;
    AzCallbackDataVecDestructorVariant_External External;
} AzCallbackDataVecDestructor;

typedef enum {
   AzDebugMessageVecDestructorTag_DefaultRust,
   AzDebugMessageVecDestructorTag_NoDestructor,
   AzDebugMessageVecDestructorTag_External,
} AzDebugMessageVecDestructorTag;

typedef struct { AzDebugMessageVecDestructorTag tag; } AzDebugMessageVecDestructorVariant_DefaultRust;
typedef struct { AzDebugMessageVecDestructorTag tag; } AzDebugMessageVecDestructorVariant_NoDestructor;
typedef struct { AzDebugMessageVecDestructorTag tag; AzDebugMessageVecDestructorType payload; } AzDebugMessageVecDestructorVariant_External;

typedef union {
    AzDebugMessageVecDestructorVariant_DefaultRust DefaultRust;
    AzDebugMessageVecDestructorVariant_NoDestructor NoDestructor;
    AzDebugMessageVecDestructorVariant_External External;
} AzDebugMessageVecDestructor;

typedef enum {
   AzGLuintVecDestructorTag_DefaultRust,
   AzGLuintVecDestructorTag_NoDestructor,
   AzGLuintVecDestructorTag_External,
} AzGLuintVecDestructorTag;

typedef struct { AzGLuintVecDestructorTag tag; } AzGLuintVecDestructorVariant_DefaultRust;
typedef struct { AzGLuintVecDestructorTag tag; } AzGLuintVecDestructorVariant_NoDestructor;
typedef struct { AzGLuintVecDestructorTag tag; AzGLuintVecDestructorType payload; } AzGLuintVecDestructorVariant_External;

typedef union {
    AzGLuintVecDestructorVariant_DefaultRust DefaultRust;
    AzGLuintVecDestructorVariant_NoDestructor NoDestructor;
    AzGLuintVecDestructorVariant_External External;
} AzGLuintVecDestructor;

typedef enum {
   AzGLintVecDestructorTag_DefaultRust,
   AzGLintVecDestructorTag_NoDestructor,
   AzGLintVecDestructorTag_External,
} AzGLintVecDestructorTag;

typedef struct { AzGLintVecDestructorTag tag; } AzGLintVecDestructorVariant_DefaultRust;
typedef struct { AzGLintVecDestructorTag tag; } AzGLintVecDestructorVariant_NoDestructor;
typedef struct { AzGLintVecDestructorTag tag; AzGLintVecDestructorType payload; } AzGLintVecDestructorVariant_External;

typedef union {
    AzGLintVecDestructorVariant_DefaultRust DefaultRust;
    AzGLintVecDestructorVariant_NoDestructor NoDestructor;
    AzGLintVecDestructorVariant_External External;
} AzGLintVecDestructor;

typedef enum {
   AzStringVecDestructorTag_DefaultRust,
   AzStringVecDestructorTag_NoDestructor,
   AzStringVecDestructorTag_External,
} AzStringVecDestructorTag;

typedef struct { AzStringVecDestructorTag tag; } AzStringVecDestructorVariant_DefaultRust;
typedef struct { AzStringVecDestructorTag tag; } AzStringVecDestructorVariant_NoDestructor;
typedef struct { AzStringVecDestructorTag tag; AzStringVecDestructorType payload; } AzStringVecDestructorVariant_External;

typedef union {
    AzStringVecDestructorVariant_DefaultRust DefaultRust;
    AzStringVecDestructorVariant_NoDestructor NoDestructor;
    AzStringVecDestructorVariant_External External;
} AzStringVecDestructor;

typedef enum {
   AzStringPairVecDestructorTag_DefaultRust,
   AzStringPairVecDestructorTag_NoDestructor,
   AzStringPairVecDestructorTag_External,
} AzStringPairVecDestructorTag;

typedef struct { AzStringPairVecDestructorTag tag; } AzStringPairVecDestructorVariant_DefaultRust;
typedef struct { AzStringPairVecDestructorTag tag; } AzStringPairVecDestructorVariant_NoDestructor;
typedef struct { AzStringPairVecDestructorTag tag; AzStringPairVecDestructorType payload; } AzStringPairVecDestructorVariant_External;

typedef union {
    AzStringPairVecDestructorVariant_DefaultRust DefaultRust;
    AzStringPairVecDestructorVariant_NoDestructor NoDestructor;
    AzStringPairVecDestructorVariant_External External;
} AzStringPairVecDestructor;

typedef enum {
   AzLinearColorStopVecDestructorTag_DefaultRust,
   AzLinearColorStopVecDestructorTag_NoDestructor,
   AzLinearColorStopVecDestructorTag_External,
} AzLinearColorStopVecDestructorTag;

typedef struct { AzLinearColorStopVecDestructorTag tag; } AzLinearColorStopVecDestructorVariant_DefaultRust;
typedef struct { AzLinearColorStopVecDestructorTag tag; } AzLinearColorStopVecDestructorVariant_NoDestructor;
typedef struct { AzLinearColorStopVecDestructorTag tag; AzLinearColorStopVecDestructorType payload; } AzLinearColorStopVecDestructorVariant_External;

typedef union {
    AzLinearColorStopVecDestructorVariant_DefaultRust DefaultRust;
    AzLinearColorStopVecDestructorVariant_NoDestructor NoDestructor;
    AzLinearColorStopVecDestructorVariant_External External;
} AzLinearColorStopVecDestructor;

typedef enum {
   AzRadialColorStopVecDestructorTag_DefaultRust,
   AzRadialColorStopVecDestructorTag_NoDestructor,
   AzRadialColorStopVecDestructorTag_External,
} AzRadialColorStopVecDestructorTag;

typedef struct { AzRadialColorStopVecDestructorTag tag; } AzRadialColorStopVecDestructorVariant_DefaultRust;
typedef struct { AzRadialColorStopVecDestructorTag tag; } AzRadialColorStopVecDestructorVariant_NoDestructor;
typedef struct { AzRadialColorStopVecDestructorTag tag; AzRadialColorStopVecDestructorType payload; } AzRadialColorStopVecDestructorVariant_External;

typedef union {
    AzRadialColorStopVecDestructorVariant_DefaultRust DefaultRust;
    AzRadialColorStopVecDestructorVariant_NoDestructor NoDestructor;
    AzRadialColorStopVecDestructorVariant_External External;
} AzRadialColorStopVecDestructor;

typedef enum {
   AzNodeIdVecDestructorTag_DefaultRust,
   AzNodeIdVecDestructorTag_NoDestructor,
   AzNodeIdVecDestructorTag_External,
} AzNodeIdVecDestructorTag;

typedef struct { AzNodeIdVecDestructorTag tag; } AzNodeIdVecDestructorVariant_DefaultRust;
typedef struct { AzNodeIdVecDestructorTag tag; } AzNodeIdVecDestructorVariant_NoDestructor;
typedef struct { AzNodeIdVecDestructorTag tag; AzNodeIdVecDestructorType payload; } AzNodeIdVecDestructorVariant_External;

typedef union {
    AzNodeIdVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeIdVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeIdVecDestructorVariant_External External;
} AzNodeIdVecDestructor;

typedef enum {
   AzNodeVecDestructorTag_DefaultRust,
   AzNodeVecDestructorTag_NoDestructor,
   AzNodeVecDestructorTag_External,
} AzNodeVecDestructorTag;

typedef struct { AzNodeVecDestructorTag tag; } AzNodeVecDestructorVariant_DefaultRust;
typedef struct { AzNodeVecDestructorTag tag; } AzNodeVecDestructorVariant_NoDestructor;
typedef struct { AzNodeVecDestructorTag tag; AzNodeVecDestructorType payload; } AzNodeVecDestructorVariant_External;

typedef union {
    AzNodeVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeVecDestructorVariant_External External;
} AzNodeVecDestructor;

typedef enum {
   AzStyledNodeVecDestructorTag_DefaultRust,
   AzStyledNodeVecDestructorTag_NoDestructor,
   AzStyledNodeVecDestructorTag_External,
} AzStyledNodeVecDestructorTag;

typedef struct { AzStyledNodeVecDestructorTag tag; } AzStyledNodeVecDestructorVariant_DefaultRust;
typedef struct { AzStyledNodeVecDestructorTag tag; } AzStyledNodeVecDestructorVariant_NoDestructor;
typedef struct { AzStyledNodeVecDestructorTag tag; AzStyledNodeVecDestructorType payload; } AzStyledNodeVecDestructorVariant_External;

typedef union {
    AzStyledNodeVecDestructorVariant_DefaultRust DefaultRust;
    AzStyledNodeVecDestructorVariant_NoDestructor NoDestructor;
    AzStyledNodeVecDestructorVariant_External External;
} AzStyledNodeVecDestructor;

typedef enum {
   AzTagIdsToNodeIdsMappingVecDestructorTag_DefaultRust,
   AzTagIdsToNodeIdsMappingVecDestructorTag_NoDestructor,
   AzTagIdsToNodeIdsMappingVecDestructorTag_External,
} AzTagIdsToNodeIdsMappingVecDestructorTag;

typedef struct { AzTagIdsToNodeIdsMappingVecDestructorTag tag; } AzTagIdsToNodeIdsMappingVecDestructorVariant_DefaultRust;
typedef struct { AzTagIdsToNodeIdsMappingVecDestructorTag tag; } AzTagIdsToNodeIdsMappingVecDestructorVariant_NoDestructor;
typedef struct { AzTagIdsToNodeIdsMappingVecDestructorTag tag; AzTagIdsToNodeIdsMappingVecDestructorType payload; } AzTagIdsToNodeIdsMappingVecDestructorVariant_External;

typedef union {
    AzTagIdsToNodeIdsMappingVecDestructorVariant_DefaultRust DefaultRust;
    AzTagIdsToNodeIdsMappingVecDestructorVariant_NoDestructor NoDestructor;
    AzTagIdsToNodeIdsMappingVecDestructorVariant_External External;
} AzTagIdsToNodeIdsMappingVecDestructor;

typedef enum {
   AzParentWithNodeDepthVecDestructorTag_DefaultRust,
   AzParentWithNodeDepthVecDestructorTag_NoDestructor,
   AzParentWithNodeDepthVecDestructorTag_External,
} AzParentWithNodeDepthVecDestructorTag;

typedef struct { AzParentWithNodeDepthVecDestructorTag tag; } AzParentWithNodeDepthVecDestructorVariant_DefaultRust;
typedef struct { AzParentWithNodeDepthVecDestructorTag tag; } AzParentWithNodeDepthVecDestructorVariant_NoDestructor;
typedef struct { AzParentWithNodeDepthVecDestructorTag tag; AzParentWithNodeDepthVecDestructorType payload; } AzParentWithNodeDepthVecDestructorVariant_External;

typedef union {
    AzParentWithNodeDepthVecDestructorVariant_DefaultRust DefaultRust;
    AzParentWithNodeDepthVecDestructorVariant_NoDestructor NoDestructor;
    AzParentWithNodeDepthVecDestructorVariant_External External;
} AzParentWithNodeDepthVecDestructor;

typedef enum {
   AzNodeDataVecDestructorTag_DefaultRust,
   AzNodeDataVecDestructorTag_NoDestructor,
   AzNodeDataVecDestructorTag_External,
} AzNodeDataVecDestructorTag;

typedef struct { AzNodeDataVecDestructorTag tag; } AzNodeDataVecDestructorVariant_DefaultRust;
typedef struct { AzNodeDataVecDestructorTag tag; } AzNodeDataVecDestructorVariant_NoDestructor;
typedef struct { AzNodeDataVecDestructorTag tag; AzNodeDataVecDestructorType payload; } AzNodeDataVecDestructorVariant_External;

typedef union {
    AzNodeDataVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeDataVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeDataVecDestructorVariant_External External;
} AzNodeDataVecDestructor;

typedef enum {
   AzOptionGlContextPtrTag_None,
   AzOptionGlContextPtrTag_Some,
} AzOptionGlContextPtrTag;

typedef struct { AzOptionGlContextPtrTag tag; } AzOptionGlContextPtrVariant_None;
typedef struct { AzOptionGlContextPtrTag tag; AzGlContextPtr payload; } AzOptionGlContextPtrVariant_Some;

typedef union {
    AzOptionGlContextPtrVariant_None None;
    AzOptionGlContextPtrVariant_Some Some;
} AzOptionGlContextPtr;

typedef enum {
   AzOptionPercentageValueTag_None,
   AzOptionPercentageValueTag_Some,
} AzOptionPercentageValueTag;

typedef struct { AzOptionPercentageValueTag tag; } AzOptionPercentageValueVariant_None;
typedef struct { AzOptionPercentageValueTag tag; AzPercentageValue payload; } AzOptionPercentageValueVariant_Some;

typedef union {
    AzOptionPercentageValueVariant_None None;
    AzOptionPercentageValueVariant_Some Some;
} AzOptionPercentageValue;

typedef enum {
   AzOptionAngleValueTag_None,
   AzOptionAngleValueTag_Some,
} AzOptionAngleValueTag;

typedef struct { AzOptionAngleValueTag tag; } AzOptionAngleValueVariant_None;
typedef struct { AzOptionAngleValueTag tag; AzAngleValue payload; } AzOptionAngleValueVariant_Some;

typedef union {
    AzOptionAngleValueVariant_None None;
    AzOptionAngleValueVariant_Some Some;
} AzOptionAngleValue;

typedef enum {
   AzOptionRendererOptionsTag_None,
   AzOptionRendererOptionsTag_Some,
} AzOptionRendererOptionsTag;

typedef struct { AzOptionRendererOptionsTag tag; } AzOptionRendererOptionsVariant_None;
typedef struct { AzOptionRendererOptionsTag tag; AzRendererOptions payload; } AzOptionRendererOptionsVariant_Some;

typedef union {
    AzOptionRendererOptionsVariant_None None;
    AzOptionRendererOptionsVariant_Some Some;
} AzOptionRendererOptions;

typedef enum {
   AzOptionCallbackTag_None,
   AzOptionCallbackTag_Some,
} AzOptionCallbackTag;

typedef struct { AzOptionCallbackTag tag; } AzOptionCallbackVariant_None;
typedef struct { AzOptionCallbackTag tag; AzCallback payload; } AzOptionCallbackVariant_Some;

typedef union {
    AzOptionCallbackVariant_None None;
    AzOptionCallbackVariant_Some Some;
} AzOptionCallback;

typedef enum {
   AzOptionThreadSendMsgTag_None,
   AzOptionThreadSendMsgTag_Some,
} AzOptionThreadSendMsgTag;

typedef struct { AzOptionThreadSendMsgTag tag; } AzOptionThreadSendMsgVariant_None;
typedef struct { AzOptionThreadSendMsgTag tag; AzThreadSendMsg payload; } AzOptionThreadSendMsgVariant_Some;

typedef union {
    AzOptionThreadSendMsgVariant_None None;
    AzOptionThreadSendMsgVariant_Some Some;
} AzOptionThreadSendMsg;

typedef enum {
   AzOptionLayoutRectTag_None,
   AzOptionLayoutRectTag_Some,
} AzOptionLayoutRectTag;

typedef struct { AzOptionLayoutRectTag tag; } AzOptionLayoutRectVariant_None;
typedef struct { AzOptionLayoutRectTag tag; AzLayoutRect payload; } AzOptionLayoutRectVariant_Some;

typedef union {
    AzOptionLayoutRectVariant_None None;
    AzOptionLayoutRectVariant_Some Some;
} AzOptionLayoutRect;

typedef enum {
   AzOptionRefAnyTag_None,
   AzOptionRefAnyTag_Some,
} AzOptionRefAnyTag;

typedef struct { AzOptionRefAnyTag tag; } AzOptionRefAnyVariant_None;
typedef struct { AzOptionRefAnyTag tag; AzRefAny payload; } AzOptionRefAnyVariant_Some;

typedef union {
    AzOptionRefAnyVariant_None None;
    AzOptionRefAnyVariant_Some Some;
} AzOptionRefAny;

typedef enum {
   AzOptionLayoutPointTag_None,
   AzOptionLayoutPointTag_Some,
} AzOptionLayoutPointTag;

typedef struct { AzOptionLayoutPointTag tag; } AzOptionLayoutPointVariant_None;
typedef struct { AzOptionLayoutPointTag tag; AzLayoutPoint payload; } AzOptionLayoutPointVariant_Some;

typedef union {
    AzOptionLayoutPointVariant_None None;
    AzOptionLayoutPointVariant_Some Some;
} AzOptionLayoutPoint;

typedef enum {
   AzOptionWindowThemeTag_None,
   AzOptionWindowThemeTag_Some,
} AzOptionWindowThemeTag;

typedef struct { AzOptionWindowThemeTag tag; } AzOptionWindowThemeVariant_None;
typedef struct { AzOptionWindowThemeTag tag; AzWindowTheme payload; } AzOptionWindowThemeVariant_Some;

typedef union {
    AzOptionWindowThemeVariant_None None;
    AzOptionWindowThemeVariant_Some Some;
} AzOptionWindowTheme;

typedef enum {
   AzOptionNodeIdTag_None,
   AzOptionNodeIdTag_Some,
} AzOptionNodeIdTag;

typedef struct { AzOptionNodeIdTag tag; } AzOptionNodeIdVariant_None;
typedef struct { AzOptionNodeIdTag tag; AzNodeId payload; } AzOptionNodeIdVariant_Some;

typedef union {
    AzOptionNodeIdVariant_None None;
    AzOptionNodeIdVariant_Some Some;
} AzOptionNodeId;

typedef enum {
   AzOptionDomNodeIdTag_None,
   AzOptionDomNodeIdTag_Some,
} AzOptionDomNodeIdTag;

typedef struct { AzOptionDomNodeIdTag tag; } AzOptionDomNodeIdVariant_None;
typedef struct { AzOptionDomNodeIdTag tag; AzDomNodeId payload; } AzOptionDomNodeIdVariant_Some;

typedef union {
    AzOptionDomNodeIdVariant_None None;
    AzOptionDomNodeIdVariant_Some Some;
} AzOptionDomNodeId;

typedef enum {
   AzOptionColorUTag_None,
   AzOptionColorUTag_Some,
} AzOptionColorUTag;

typedef struct { AzOptionColorUTag tag; } AzOptionColorUVariant_None;
typedef struct { AzOptionColorUTag tag; AzColorU payload; } AzOptionColorUVariant_Some;

typedef union {
    AzOptionColorUVariant_None None;
    AzOptionColorUVariant_Some Some;
} AzOptionColorU;

typedef enum {
   AzOptionSvgDashPatternTag_None,
   AzOptionSvgDashPatternTag_Some,
} AzOptionSvgDashPatternTag;

typedef struct { AzOptionSvgDashPatternTag tag; } AzOptionSvgDashPatternVariant_None;
typedef struct { AzOptionSvgDashPatternTag tag; AzSvgDashPattern payload; } AzOptionSvgDashPatternVariant_Some;

typedef union {
    AzOptionSvgDashPatternVariant_None None;
    AzOptionSvgDashPatternVariant_Some Some;
} AzOptionSvgDashPattern;

typedef enum {
   AzOptionHwndHandleTag_None,
   AzOptionHwndHandleTag_Some,
} AzOptionHwndHandleTag;

typedef struct { AzOptionHwndHandleTag tag; } AzOptionHwndHandleVariant_None;
typedef struct { AzOptionHwndHandleTag tag; void* restrict payload; } AzOptionHwndHandleVariant_Some;

typedef union {
    AzOptionHwndHandleVariant_None None;
    AzOptionHwndHandleVariant_Some Some;
} AzOptionHwndHandle;

typedef enum {
   AzOptionLogicalPositionTag_None,
   AzOptionLogicalPositionTag_Some,
} AzOptionLogicalPositionTag;

typedef struct { AzOptionLogicalPositionTag tag; } AzOptionLogicalPositionVariant_None;
typedef struct { AzOptionLogicalPositionTag tag; AzLogicalPosition payload; } AzOptionLogicalPositionVariant_Some;

typedef union {
    AzOptionLogicalPositionVariant_None None;
    AzOptionLogicalPositionVariant_Some Some;
} AzOptionLogicalPosition;

typedef enum {
   AzOptionPhysicalPositionI32Tag_None,
   AzOptionPhysicalPositionI32Tag_Some,
} AzOptionPhysicalPositionI32Tag;

typedef struct { AzOptionPhysicalPositionI32Tag tag; } AzOptionPhysicalPositionI32Variant_None;
typedef struct { AzOptionPhysicalPositionI32Tag tag; AzPhysicalPositionI32 payload; } AzOptionPhysicalPositionI32Variant_Some;

typedef union {
    AzOptionPhysicalPositionI32Variant_None None;
    AzOptionPhysicalPositionI32Variant_Some Some;
} AzOptionPhysicalPositionI32;

typedef enum {
   AzOptionX11VisualTag_None,
   AzOptionX11VisualTag_Some,
} AzOptionX11VisualTag;

typedef struct { AzOptionX11VisualTag tag; } AzOptionX11VisualVariant_None;
typedef struct { AzOptionX11VisualTag tag; void* const payload; } AzOptionX11VisualVariant_Some;

typedef union {
    AzOptionX11VisualVariant_None None;
    AzOptionX11VisualVariant_Some Some;
} AzOptionX11Visual;

typedef enum {
   AzOptionI32Tag_None,
   AzOptionI32Tag_Some,
} AzOptionI32Tag;

typedef struct { AzOptionI32Tag tag; } AzOptionI32Variant_None;
typedef struct { AzOptionI32Tag tag; int32_t payload; } AzOptionI32Variant_Some;

typedef union {
    AzOptionI32Variant_None None;
    AzOptionI32Variant_Some Some;
} AzOptionI32;

typedef enum {
   AzOptionF32Tag_None,
   AzOptionF32Tag_Some,
} AzOptionF32Tag;

typedef struct { AzOptionF32Tag tag; } AzOptionF32Variant_None;
typedef struct { AzOptionF32Tag tag; float payload; } AzOptionF32Variant_Some;

typedef union {
    AzOptionF32Variant_None None;
    AzOptionF32Variant_Some Some;
} AzOptionF32;

typedef enum {
   AzOptionMouseCursorTypeTag_None,
   AzOptionMouseCursorTypeTag_Some,
} AzOptionMouseCursorTypeTag;

typedef struct { AzOptionMouseCursorTypeTag tag; } AzOptionMouseCursorTypeVariant_None;
typedef struct { AzOptionMouseCursorTypeTag tag; AzMouseCursorType payload; } AzOptionMouseCursorTypeVariant_Some;

typedef union {
    AzOptionMouseCursorTypeVariant_None None;
    AzOptionMouseCursorTypeVariant_Some Some;
} AzOptionMouseCursorType;

typedef enum {
   AzOptionLogicalSizeTag_None,
   AzOptionLogicalSizeTag_Some,
} AzOptionLogicalSizeTag;

typedef struct { AzOptionLogicalSizeTag tag; } AzOptionLogicalSizeVariant_None;
typedef struct { AzOptionLogicalSizeTag tag; AzLogicalSize payload; } AzOptionLogicalSizeVariant_Some;

typedef union {
    AzOptionLogicalSizeVariant_None None;
    AzOptionLogicalSizeVariant_Some Some;
} AzOptionLogicalSize;

typedef enum {
   AzOptionCharTag_None,
   AzOptionCharTag_Some,
} AzOptionCharTag;

typedef struct { AzOptionCharTag tag; } AzOptionCharVariant_None;
typedef struct { AzOptionCharTag tag; uint32_t payload; } AzOptionCharVariant_Some;

typedef union {
    AzOptionCharVariant_None None;
    AzOptionCharVariant_Some Some;
} AzOptionChar;

typedef enum {
   AzOptionVirtualKeyCodeTag_None,
   AzOptionVirtualKeyCodeTag_Some,
} AzOptionVirtualKeyCodeTag;

typedef struct { AzOptionVirtualKeyCodeTag tag; } AzOptionVirtualKeyCodeVariant_None;
typedef struct { AzOptionVirtualKeyCodeTag tag; AzVirtualKeyCode payload; } AzOptionVirtualKeyCodeVariant_Some;

typedef union {
    AzOptionVirtualKeyCodeVariant_None None;
    AzOptionVirtualKeyCodeVariant_Some Some;
} AzOptionVirtualKeyCode;

typedef enum {
   AzOptionTextureTag_None,
   AzOptionTextureTag_Some,
} AzOptionTextureTag;

typedef struct { AzOptionTextureTag tag; } AzOptionTextureVariant_None;
typedef struct { AzOptionTextureTag tag; AzTexture payload; } AzOptionTextureVariant_Some;

typedef union {
    AzOptionTextureVariant_None None;
    AzOptionTextureVariant_Some Some;
} AzOptionTexture;

typedef enum {
   AzOptionTabIndexTag_None,
   AzOptionTabIndexTag_Some,
} AzOptionTabIndexTag;

typedef struct { AzOptionTabIndexTag tag; } AzOptionTabIndexVariant_None;
typedef struct { AzOptionTabIndexTag tag; AzTabIndex payload; } AzOptionTabIndexVariant_Some;

typedef union {
    AzOptionTabIndexVariant_None None;
    AzOptionTabIndexVariant_Some Some;
} AzOptionTabIndex;

typedef enum {
   AzOptionTagIdTag_None,
   AzOptionTagIdTag_Some,
} AzOptionTagIdTag;

typedef struct { AzOptionTagIdTag tag; } AzOptionTagIdVariant_None;
typedef struct { AzOptionTagIdTag tag; AzTagId payload; } AzOptionTagIdVariant_Some;

typedef union {
    AzOptionTagIdVariant_None None;
    AzOptionTagIdVariant_Some Some;
} AzOptionTagId;

typedef enum {
   AzOptionUsizeTag_None,
   AzOptionUsizeTag_Some,
} AzOptionUsizeTag;

typedef struct { AzOptionUsizeTag tag; } AzOptionUsizeVariant_None;
typedef struct { AzOptionUsizeTag tag; size_t payload; } AzOptionUsizeVariant_Some;

typedef union {
    AzOptionUsizeVariant_None None;
    AzOptionUsizeVariant_Some Some;
} AzOptionUsize;

typedef enum {
   AzOptionU8VecRefTag_None,
   AzOptionU8VecRefTag_Some,
} AzOptionU8VecRefTag;

typedef struct { AzOptionU8VecRefTag tag; } AzOptionU8VecRefVariant_None;
typedef struct { AzOptionU8VecRefTag tag; AzU8VecRef payload; } AzOptionU8VecRefVariant_Some;

typedef union {
    AzOptionU8VecRefVariant_None None;
    AzOptionU8VecRefVariant_Some Some;
} AzOptionU8VecRef;

typedef struct {
    uint32_t row;
    uint32_t col;
} AzSvgParseErrorPosition;

typedef struct {
    AzInstantPtrCloneFnType cb;
} AzInstantPtrCloneFn;

typedef struct {
    AzInstantPtrDestructorFnType cb;
} AzInstantPtrDestructorFn;

typedef struct {
    uint64_t tick_counter;
} AzSystemTick;

typedef struct {
    uint64_t secs;
    uint32_t nanos;
} AzSystemTimeDiff;

typedef struct {
    uint64_t tick_diff;
} AzSystemTickDiff;

typedef enum {
   AzRawWindowHandleTag_IOS,
   AzRawWindowHandleTag_MacOS,
   AzRawWindowHandleTag_Xlib,
   AzRawWindowHandleTag_Xcb,
   AzRawWindowHandleTag_Wayland,
   AzRawWindowHandleTag_Windows,
   AzRawWindowHandleTag_Web,
   AzRawWindowHandleTag_Android,
   AzRawWindowHandleTag_Unsupported,
} AzRawWindowHandleTag;

typedef struct { AzRawWindowHandleTag tag; AzIOSHandle payload; } AzRawWindowHandleVariant_IOS;
typedef struct { AzRawWindowHandleTag tag; AzMacOSHandle payload; } AzRawWindowHandleVariant_MacOS;
typedef struct { AzRawWindowHandleTag tag; AzXlibHandle payload; } AzRawWindowHandleVariant_Xlib;
typedef struct { AzRawWindowHandleTag tag; AzXcbHandle payload; } AzRawWindowHandleVariant_Xcb;
typedef struct { AzRawWindowHandleTag tag; AzWaylandHandle payload; } AzRawWindowHandleVariant_Wayland;
typedef struct { AzRawWindowHandleTag tag; AzWindowsHandle payload; } AzRawWindowHandleVariant_Windows;
typedef struct { AzRawWindowHandleTag tag; AzWebHandle payload; } AzRawWindowHandleVariant_Web;
typedef struct { AzRawWindowHandleTag tag; AzAndroidHandle payload; } AzRawWindowHandleVariant_Android;
typedef struct { AzRawWindowHandleTag tag; } AzRawWindowHandleVariant_Unsupported;

typedef union {
    AzRawWindowHandleVariant_IOS IOS;
    AzRawWindowHandleVariant_MacOS MacOS;
    AzRawWindowHandleVariant_Xlib Xlib;
    AzRawWindowHandleVariant_Xcb Xcb;
    AzRawWindowHandleVariant_Wayland Wayland;
    AzRawWindowHandleVariant_Windows Windows;
    AzRawWindowHandleVariant_Web Web;
    AzRawWindowHandleVariant_Android Android;
    AzRawWindowHandleVariant_Unsupported Unsupported;
} AzRawWindowHandle;

typedef struct {
    AzLogicalPosition origin;
    AzLogicalSize size;
} AzLogicalRect;

typedef struct {
    AzLogicalSize dimensions;
    float hidpi_factor;
    float system_hidpi_factor;
    AzOptionLogicalSize min_dimensions;
    AzOptionLogicalSize max_dimensions;
} AzWindowSize;

typedef struct {
    AzOptionMouseCursorType mouse_cursor_type;
    AzCursorPosition cursor_position;
    bool  is_cursor_locked;
    bool  left_down;
    bool  right_down;
    bool  middle_down;
    AzOptionF32 scroll_x;
    AzOptionF32 scroll_y;
} AzMouseState;

typedef struct {
    AzOptionTexture texture;
} AzGlCallbackReturn;

typedef struct {
    AzWindowSize* const window_size;
    void* restrict window_size_width_stops;
    void* restrict window_size_height_stops;
    void* const resources;
} AzLayoutInfo;

typedef struct {
    AzCreateThreadFn create_thread_fn;
    AzGetSystemTimeFn get_system_time_fn;
} AzSystemCallbacks;

typedef enum {
   AzEventFilterTag_Hover,
   AzEventFilterTag_Not,
   AzEventFilterTag_Focus,
   AzEventFilterTag_Window,
   AzEventFilterTag_Component,
   AzEventFilterTag_Application,
} AzEventFilterTag;

typedef struct { AzEventFilterTag tag; AzHoverEventFilter payload; } AzEventFilterVariant_Hover;
typedef struct { AzEventFilterTag tag; AzNotEventFilter payload; } AzEventFilterVariant_Not;
typedef struct { AzEventFilterTag tag; AzFocusEventFilter payload; } AzEventFilterVariant_Focus;
typedef struct { AzEventFilterTag tag; AzWindowEventFilter payload; } AzEventFilterVariant_Window;
typedef struct { AzEventFilterTag tag; AzComponentEventFilter payload; } AzEventFilterVariant_Component;
typedef struct { AzEventFilterTag tag; AzApplicationEventFilter payload; } AzEventFilterVariant_Application;

typedef union {
    AzEventFilterVariant_Hover Hover;
    AzEventFilterVariant_Not Not;
    AzEventFilterVariant_Focus Focus;
    AzEventFilterVariant_Window Window;
    AzEventFilterVariant_Component Component;
    AzEventFilterVariant_Application Application;
} AzEventFilter;

typedef enum {
   AzCssNthChildSelectorTag_Number,
   AzCssNthChildSelectorTag_Even,
   AzCssNthChildSelectorTag_Odd,
   AzCssNthChildSelectorTag_Pattern,
} AzCssNthChildSelectorTag;

typedef struct { AzCssNthChildSelectorTag tag; uint32_t payload; } AzCssNthChildSelectorVariant_Number;
typedef struct { AzCssNthChildSelectorTag tag; } AzCssNthChildSelectorVariant_Even;
typedef struct { AzCssNthChildSelectorTag tag; } AzCssNthChildSelectorVariant_Odd;
typedef struct { AzCssNthChildSelectorTag tag; AzCssNthChildPattern payload; } AzCssNthChildSelectorVariant_Pattern;

typedef union {
    AzCssNthChildSelectorVariant_Number Number;
    AzCssNthChildSelectorVariant_Even Even;
    AzCssNthChildSelectorVariant_Odd Odd;
    AzCssNthChildSelectorVariant_Pattern Pattern;
} AzCssNthChildSelector;

typedef struct {
    AzOptionPercentageValue offset;
    AzColorU color;
} AzLinearColorStop;

typedef struct {
    AzOptionAngleValue offset;
    AzColorU color;
} AzRadialColorStop;

typedef enum {
   AzStyleTransformTag_Matrix,
   AzStyleTransformTag_Matrix3D,
   AzStyleTransformTag_Translate,
   AzStyleTransformTag_Translate3D,
   AzStyleTransformTag_TranslateX,
   AzStyleTransformTag_TranslateY,
   AzStyleTransformTag_TranslateZ,
   AzStyleTransformTag_Rotate,
   AzStyleTransformTag_Rotate3D,
   AzStyleTransformTag_RotateX,
   AzStyleTransformTag_RotateY,
   AzStyleTransformTag_RotateZ,
   AzStyleTransformTag_Scale,
   AzStyleTransformTag_Scale3D,
   AzStyleTransformTag_ScaleX,
   AzStyleTransformTag_ScaleY,
   AzStyleTransformTag_ScaleZ,
   AzStyleTransformTag_Skew,
   AzStyleTransformTag_SkewX,
   AzStyleTransformTag_SkewY,
   AzStyleTransformTag_Perspective,
} AzStyleTransformTag;

typedef struct { AzStyleTransformTag tag; AzStyleTransformMatrix2D payload; } AzStyleTransformVariant_Matrix;
typedef struct { AzStyleTransformTag tag; AzStyleTransformMatrix3D payload; } AzStyleTransformVariant_Matrix3D;
typedef struct { AzStyleTransformTag tag; AzStyleTransformTranslate2D payload; } AzStyleTransformVariant_Translate;
typedef struct { AzStyleTransformTag tag; AzStyleTransformTranslate3D payload; } AzStyleTransformVariant_Translate3D;
typedef struct { AzStyleTransformTag tag; AzPixelValue payload; } AzStyleTransformVariant_TranslateX;
typedef struct { AzStyleTransformTag tag; AzPixelValue payload; } AzStyleTransformVariant_TranslateY;
typedef struct { AzStyleTransformTag tag; AzPixelValue payload; } AzStyleTransformVariant_TranslateZ;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_Rotate;
typedef struct { AzStyleTransformTag tag; AzStyleTransformRotate3D payload; } AzStyleTransformVariant_Rotate3D;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_RotateX;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_RotateY;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_RotateZ;
typedef struct { AzStyleTransformTag tag; AzStyleTransformScale2D payload; } AzStyleTransformVariant_Scale;
typedef struct { AzStyleTransformTag tag; AzStyleTransformScale3D payload; } AzStyleTransformVariant_Scale3D;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_ScaleX;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_ScaleY;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_ScaleZ;
typedef struct { AzStyleTransformTag tag; AzStyleTransformSkew2D payload; } AzStyleTransformVariant_Skew;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_SkewX;
typedef struct { AzStyleTransformTag tag; AzPercentageValue payload; } AzStyleTransformVariant_SkewY;
typedef struct { AzStyleTransformTag tag; AzPixelValue payload; } AzStyleTransformVariant_Perspective;

typedef union {
    AzStyleTransformVariant_Matrix Matrix;
    AzStyleTransformVariant_Matrix3D Matrix3D;
    AzStyleTransformVariant_Translate Translate;
    AzStyleTransformVariant_Translate3D Translate3D;
    AzStyleTransformVariant_TranslateX TranslateX;
    AzStyleTransformVariant_TranslateY TranslateY;
    AzStyleTransformVariant_TranslateZ TranslateZ;
    AzStyleTransformVariant_Rotate Rotate;
    AzStyleTransformVariant_Rotate3D Rotate3D;
    AzStyleTransformVariant_RotateX RotateX;
    AzStyleTransformVariant_RotateY RotateY;
    AzStyleTransformVariant_RotateZ RotateZ;
    AzStyleTransformVariant_Scale Scale;
    AzStyleTransformVariant_Scale3D Scale3D;
    AzStyleTransformVariant_ScaleX ScaleX;
    AzStyleTransformVariant_ScaleY ScaleY;
    AzStyleTransformVariant_ScaleZ ScaleZ;
    AzStyleTransformVariant_Skew Skew;
    AzStyleTransformVariant_SkewX SkewX;
    AzStyleTransformVariant_SkewY SkewY;
    AzStyleTransformVariant_Perspective Perspective;
} AzStyleTransform;

typedef struct {
    AzStyledNodeState state;
    AzOptionTagId tag_id;
} AzStyledNode;

typedef struct {
    AzTagId tag_id;
    AzNodeId node_id;
    AzOptionTabIndex tab_index;
} AzTagIdToNodeIdMapping;

typedef struct {
    AzRefstr* const ptr;
    size_t len;
} AzRefstrVecRef;

typedef struct {
    AzImageId image;
    AzLogicalRect rect;
    bool  repeat;
} AzImageMask;

typedef struct {
    AzSvgPoint start;
    AzSvgPoint end;
} AzSvgLine;

typedef struct {
    AzOptionColorU background_color;
    AzSvgFitTo fit;
} AzSvgRenderOptions;

typedef struct {
    AzSvgLineCap start_cap;
    AzSvgLineCap end_cap;
    AzSvgLineJoin line_join;
    AzOptionSvgDashPattern dash_pattern;
    size_t line_width;
    size_t miter_limit;
    size_t tolerance;
    bool  apply_line_width;
} AzSvgStrokeStyle;

typedef struct {
    void* restrict thread_handle;
    void* restrict sender;
    void* restrict receiver;
    AzRefAny writeback_data;
    void* restrict dropcheck;
    AzCheckThreadFinishedFn check_thread_finished_fn;
    AzLibrarySendThreadMsgFn send_thread_msg_fn;
    AzLibraryReceiveThreadMsgFn receive_thread_msg_fn;
    AzThreadDestructorFn thread_destructor_fn;
} AzThread;

typedef enum {
   AzThreadReceiveMsgTag_WriteBack,
   AzThreadReceiveMsgTag_Update,
} AzThreadReceiveMsgTag;

typedef struct { AzThreadReceiveMsgTag tag; AzThreadWriteBackMsg payload; } AzThreadReceiveMsgVariant_WriteBack;
typedef struct { AzThreadReceiveMsgTag tag; AzUpdateScreen payload; } AzThreadReceiveMsgVariant_Update;

typedef union {
    AzThreadReceiveMsgVariant_WriteBack WriteBack;
    AzThreadReceiveMsgVariant_Update Update;
} AzThreadReceiveMsg;

typedef struct {
    AzVideoMode* const ptr;
    size_t len;
    size_t cap;
    AzVideoModeVecDestructor destructor;
} AzVideoModeVec;

typedef struct {
    AzStyleBackgroundPosition* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundPositionVecDestructor destructor;
} AzStyleBackgroundPositionVec;

typedef struct {
    AzStyleBackgroundRepeat* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundRepeatVecDestructor destructor;
} AzStyleBackgroundRepeatVec;

typedef struct {
    AzStyleBackgroundSize* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundSizeVecDestructor destructor;
} AzStyleBackgroundSizeVec;

typedef struct {
    AzStyleTransform* const ptr;
    size_t len;
    size_t cap;
    AzStyleTransformVecDestructor destructor;
} AzStyleTransformVec;

typedef struct {
    AzSvgVertex* const ptr;
    size_t len;
    size_t cap;
    AzSvgVertexVecDestructor destructor;
} AzSvgVertexVec;

typedef struct {
    uint32_t* const ptr;
    size_t len;
    size_t cap;
    AzU32VecDestructor destructor;
} AzU32Vec;

typedef struct {
    AzXWindowType* const ptr;
    size_t len;
    size_t cap;
    AzXWindowTypeVecDestructor destructor;
} AzXWindowTypeVec;

typedef struct {
    AzVirtualKeyCode* const ptr;
    size_t len;
    size_t cap;
    AzVirtualKeyCodeVecDestructor destructor;
} AzVirtualKeyCodeVec;

typedef struct {
    AzCascadeInfo* const ptr;
    size_t len;
    size_t cap;
    AzCascadeInfoVecDestructor destructor;
} AzCascadeInfoVec;

typedef struct {
    uint32_t* const ptr;
    size_t len;
    size_t cap;
    AzScanCodeVecDestructor destructor;
} AzScanCodeVec;

typedef struct {
    uint8_t* const ptr;
    size_t len;
    size_t cap;
    AzU8VecDestructor destructor;
} AzU8Vec;

typedef struct {
    uint32_t* const ptr;
    size_t len;
    size_t cap;
    AzGLuintVecDestructor destructor;
} AzGLuintVec;

typedef struct {
    int32_t* const ptr;
    size_t len;
    size_t cap;
    AzGLintVecDestructor destructor;
} AzGLintVec;

typedef struct {
    AzLinearColorStop* const ptr;
    size_t len;
    size_t cap;
    AzLinearColorStopVecDestructor destructor;
} AzLinearColorStopVec;

typedef struct {
    AzRadialColorStop* const ptr;
    size_t len;
    size_t cap;
    AzRadialColorStopVecDestructor destructor;
} AzRadialColorStopVec;

typedef struct {
    AzNodeId* const ptr;
    size_t len;
    size_t cap;
    AzNodeIdVecDestructor destructor;
} AzNodeIdVec;

typedef struct {
    AzNode* const ptr;
    size_t len;
    size_t cap;
    AzNodeVecDestructor destructor;
} AzNodeVec;

typedef struct {
    AzStyledNode* const ptr;
    size_t len;
    size_t cap;
    AzStyledNodeVecDestructor destructor;
} AzStyledNodeVec;

typedef struct {
    AzTagIdToNodeIdMapping* const ptr;
    size_t len;
    size_t cap;
    AzTagIdsToNodeIdsMappingVecDestructor destructor;
} AzTagIdsToNodeIdsMappingVec;

typedef struct {
    AzParentWithNodeDepth* const ptr;
    size_t len;
    size_t cap;
    AzParentWithNodeDepthVecDestructor destructor;
} AzParentWithNodeDepthVec;

typedef enum {
   AzOptionThreadReceiveMsgTag_None,
   AzOptionThreadReceiveMsgTag_Some,
} AzOptionThreadReceiveMsgTag;

typedef struct { AzOptionThreadReceiveMsgTag tag; } AzOptionThreadReceiveMsgVariant_None;
typedef struct { AzOptionThreadReceiveMsgTag tag; AzThreadReceiveMsg payload; } AzOptionThreadReceiveMsgVariant_Some;

typedef union {
    AzOptionThreadReceiveMsgVariant_None None;
    AzOptionThreadReceiveMsgVariant_Some Some;
} AzOptionThreadReceiveMsg;

typedef enum {
   AzOptionImageMaskTag_None,
   AzOptionImageMaskTag_Some,
} AzOptionImageMaskTag;

typedef struct { AzOptionImageMaskTag tag; } AzOptionImageMaskVariant_None;
typedef struct { AzOptionImageMaskTag tag; AzImageMask payload; } AzOptionImageMaskVariant_Some;

typedef union {
    AzOptionImageMaskVariant_None None;
    AzOptionImageMaskVariant_Some Some;
} AzOptionImageMask;

typedef struct {
    uint32_t ch;
    AzSvgParseErrorPosition pos;
} AzNonXmlCharError;

typedef struct {
    uint8_t expected;
    uint8_t got;
    AzSvgParseErrorPosition pos;
} AzInvalidCharError;

typedef struct {
    uint8_t expected;
    AzU8Vec got;
    AzSvgParseErrorPosition pos;
} AzInvalidCharMultipleError;

typedef struct {
    uint8_t got;
    AzSvgParseErrorPosition pos;
} AzInvalidQuoteError;

typedef struct {
    uint8_t got;
    AzSvgParseErrorPosition pos;
} AzInvalidSpaceError;

typedef enum {
   AzInstantTag_System,
   AzInstantTag_Tick,
} AzInstantTag;

typedef struct { AzInstantTag tag; AzInstantPtr payload; } AzInstantVariant_System;
typedef struct { AzInstantTag tag; AzSystemTick payload; } AzInstantVariant_Tick;

typedef union {
    AzInstantVariant_System System;
    AzInstantVariant_Tick Tick;
} AzInstant;

typedef enum {
   AzDurationTag_System,
   AzDurationTag_Tick,
} AzDurationTag;

typedef struct { AzDurationTag tag; AzSystemTimeDiff payload; } AzDurationVariant_System;
typedef struct { AzDurationTag tag; AzSystemTickDiff payload; } AzDurationVariant_Tick;

typedef union {
    AzDurationVariant_System System;
    AzDurationVariant_Tick Tick;
} AzDuration;

typedef struct {
    AzAppLogLevel log_level;
    bool  enable_visual_panic_hook;
    bool  enable_logging_on_panic;
    bool  enable_tab_navigation;
    AzSystemCallbacks system_callbacks;
} AzAppConfig;

typedef struct {
    AzIconKey key;
    AzU8Vec rgba_bytes;
} AzSmallWindowIconBytes;

typedef struct {
    AzIconKey key;
    AzU8Vec rgba_bytes;
} AzLargeWindowIconBytes;

typedef enum {
   AzWindowIconTag_Small,
   AzWindowIconTag_Large,
} AzWindowIconTag;

typedef struct { AzWindowIconTag tag; AzSmallWindowIconBytes payload; } AzWindowIconVariant_Small;
typedef struct { AzWindowIconTag tag; AzLargeWindowIconBytes payload; } AzWindowIconVariant_Large;

typedef union {
    AzWindowIconVariant_Small Small;
    AzWindowIconVariant_Large Large;
} AzWindowIcon;

typedef struct {
    AzIconKey key;
    AzU8Vec rgba_bytes;
} AzTaskBarIcon;

typedef struct {
    bool  shift_down;
    bool  ctrl_down;
    bool  alt_down;
    bool  super_down;
    AzOptionChar current_char;
    AzOptionVirtualKeyCode current_virtual_keycode;
    AzVirtualKeyCodeVec pressed_virtual_keycodes;
    AzScanCodeVec pressed_scancodes;
} AzKeyboardState;

typedef struct {
    AzDomNodeId callback_node_id;
    AzHidpiAdjustedBounds bounds;
    AzGlContextPtr* const gl_context;
    void* const resources;
    AzNodeVec* const node_hierarchy;
    void* const words_cache;
    void* const shaped_words_cache;
    void* const positioned_words_cache;
    void* const positioned_rects;
} AzGlCallbackInfo;

typedef struct {
    AzEventFilter event;
    AzCallback callback;
    AzRefAny data;
} AzCallbackData;

typedef enum {
   AzCssPathPseudoSelectorTag_First,
   AzCssPathPseudoSelectorTag_Last,
   AzCssPathPseudoSelectorTag_NthChild,
   AzCssPathPseudoSelectorTag_Hover,
   AzCssPathPseudoSelectorTag_Active,
   AzCssPathPseudoSelectorTag_Focus,
} AzCssPathPseudoSelectorTag;

typedef struct { AzCssPathPseudoSelectorTag tag; } AzCssPathPseudoSelectorVariant_First;
typedef struct { AzCssPathPseudoSelectorTag tag; } AzCssPathPseudoSelectorVariant_Last;
typedef struct { AzCssPathPseudoSelectorTag tag; AzCssNthChildSelector payload; } AzCssPathPseudoSelectorVariant_NthChild;
typedef struct { AzCssPathPseudoSelectorTag tag; } AzCssPathPseudoSelectorVariant_Hover;
typedef struct { AzCssPathPseudoSelectorTag tag; } AzCssPathPseudoSelectorVariant_Active;
typedef struct { AzCssPathPseudoSelectorTag tag; } AzCssPathPseudoSelectorVariant_Focus;

typedef union {
    AzCssPathPseudoSelectorVariant_First First;
    AzCssPathPseudoSelectorVariant_Last Last;
    AzCssPathPseudoSelectorVariant_NthChild NthChild;
    AzCssPathPseudoSelectorVariant_Hover Hover;
    AzCssPathPseudoSelectorVariant_Active Active;
    AzCssPathPseudoSelectorVariant_Focus Focus;
} AzCssPathPseudoSelector;

typedef struct {
    AzDirection direction;
    AzExtendMode extend_mode;
    AzLinearColorStopVec stops;
} AzLinearGradient;

typedef struct {
    AzShape shape;
    AzRadialGradientSize size;
    AzStyleBackgroundPosition position;
    AzExtendMode extend_mode;
    AzLinearColorStopVec stops;
} AzRadialGradient;

typedef struct {
    AzExtendMode extend_mode;
    AzStyleBackgroundPosition center;
    AzAngleValue angle;
    AzRadialColorStopVec stops;
} AzConicGradient;

typedef enum {
   AzStyleBackgroundPositionVecValueTag_Auto,
   AzStyleBackgroundPositionVecValueTag_None,
   AzStyleBackgroundPositionVecValueTag_Inherit,
   AzStyleBackgroundPositionVecValueTag_Initial,
   AzStyleBackgroundPositionVecValueTag_Exact,
} AzStyleBackgroundPositionVecValueTag;

typedef struct { AzStyleBackgroundPositionVecValueTag tag; } AzStyleBackgroundPositionVecValueVariant_Auto;
typedef struct { AzStyleBackgroundPositionVecValueTag tag; } AzStyleBackgroundPositionVecValueVariant_None;
typedef struct { AzStyleBackgroundPositionVecValueTag tag; } AzStyleBackgroundPositionVecValueVariant_Inherit;
typedef struct { AzStyleBackgroundPositionVecValueTag tag; } AzStyleBackgroundPositionVecValueVariant_Initial;
typedef struct { AzStyleBackgroundPositionVecValueTag tag; AzStyleBackgroundPositionVec payload; } AzStyleBackgroundPositionVecValueVariant_Exact;

typedef union {
    AzStyleBackgroundPositionVecValueVariant_Auto Auto;
    AzStyleBackgroundPositionVecValueVariant_None None;
    AzStyleBackgroundPositionVecValueVariant_Inherit Inherit;
    AzStyleBackgroundPositionVecValueVariant_Initial Initial;
    AzStyleBackgroundPositionVecValueVariant_Exact Exact;
} AzStyleBackgroundPositionVecValue;

typedef enum {
   AzStyleBackgroundRepeatVecValueTag_Auto,
   AzStyleBackgroundRepeatVecValueTag_None,
   AzStyleBackgroundRepeatVecValueTag_Inherit,
   AzStyleBackgroundRepeatVecValueTag_Initial,
   AzStyleBackgroundRepeatVecValueTag_Exact,
} AzStyleBackgroundRepeatVecValueTag;

typedef struct { AzStyleBackgroundRepeatVecValueTag tag; } AzStyleBackgroundRepeatVecValueVariant_Auto;
typedef struct { AzStyleBackgroundRepeatVecValueTag tag; } AzStyleBackgroundRepeatVecValueVariant_None;
typedef struct { AzStyleBackgroundRepeatVecValueTag tag; } AzStyleBackgroundRepeatVecValueVariant_Inherit;
typedef struct { AzStyleBackgroundRepeatVecValueTag tag; } AzStyleBackgroundRepeatVecValueVariant_Initial;
typedef struct { AzStyleBackgroundRepeatVecValueTag tag; AzStyleBackgroundRepeatVec payload; } AzStyleBackgroundRepeatVecValueVariant_Exact;

typedef union {
    AzStyleBackgroundRepeatVecValueVariant_Auto Auto;
    AzStyleBackgroundRepeatVecValueVariant_None None;
    AzStyleBackgroundRepeatVecValueVariant_Inherit Inherit;
    AzStyleBackgroundRepeatVecValueVariant_Initial Initial;
    AzStyleBackgroundRepeatVecValueVariant_Exact Exact;
} AzStyleBackgroundRepeatVecValue;

typedef enum {
   AzStyleBackgroundSizeVecValueTag_Auto,
   AzStyleBackgroundSizeVecValueTag_None,
   AzStyleBackgroundSizeVecValueTag_Inherit,
   AzStyleBackgroundSizeVecValueTag_Initial,
   AzStyleBackgroundSizeVecValueTag_Exact,
} AzStyleBackgroundSizeVecValueTag;

typedef struct { AzStyleBackgroundSizeVecValueTag tag; } AzStyleBackgroundSizeVecValueVariant_Auto;
typedef struct { AzStyleBackgroundSizeVecValueTag tag; } AzStyleBackgroundSizeVecValueVariant_None;
typedef struct { AzStyleBackgroundSizeVecValueTag tag; } AzStyleBackgroundSizeVecValueVariant_Inherit;
typedef struct { AzStyleBackgroundSizeVecValueTag tag; } AzStyleBackgroundSizeVecValueVariant_Initial;
typedef struct { AzStyleBackgroundSizeVecValueTag tag; AzStyleBackgroundSizeVec payload; } AzStyleBackgroundSizeVecValueVariant_Exact;

typedef union {
    AzStyleBackgroundSizeVecValueVariant_Auto Auto;
    AzStyleBackgroundSizeVecValueVariant_None None;
    AzStyleBackgroundSizeVecValueVariant_Inherit Inherit;
    AzStyleBackgroundSizeVecValueVariant_Initial Initial;
    AzStyleBackgroundSizeVecValueVariant_Exact Exact;
} AzStyleBackgroundSizeVecValue;

typedef enum {
   AzStyleTransformVecValueTag_Auto,
   AzStyleTransformVecValueTag_None,
   AzStyleTransformVecValueTag_Inherit,
   AzStyleTransformVecValueTag_Initial,
   AzStyleTransformVecValueTag_Exact,
} AzStyleTransformVecValueTag;

typedef struct { AzStyleTransformVecValueTag tag; } AzStyleTransformVecValueVariant_Auto;
typedef struct { AzStyleTransformVecValueTag tag; } AzStyleTransformVecValueVariant_None;
typedef struct { AzStyleTransformVecValueTag tag; } AzStyleTransformVecValueVariant_Inherit;
typedef struct { AzStyleTransformVecValueTag tag; } AzStyleTransformVecValueVariant_Initial;
typedef struct { AzStyleTransformVecValueTag tag; AzStyleTransformVec payload; } AzStyleTransformVecValueVariant_Exact;

typedef union {
    AzStyleTransformVecValueVariant_Auto Auto;
    AzStyleTransformVecValueVariant_None None;
    AzStyleTransformVecValueVariant_Inherit Inherit;
    AzStyleTransformVecValueVariant_Initial Initial;
    AzStyleTransformVecValueVariant_Exact Exact;
} AzStyleTransformVecValue;

typedef struct {
    AzU8Vec _0;
    uint32_t _1;
} AzGetProgramBinaryReturn;

typedef struct {
    AzU8Vec pixels;
    size_t width;
    size_t height;
    AzRawImageFormat data_format;
} AzRawImage;

typedef enum {
   AzSvgPathElementTag_Line,
   AzSvgPathElementTag_QuadraticCurve,
   AzSvgPathElementTag_CubicCurve,
} AzSvgPathElementTag;

typedef struct { AzSvgPathElementTag tag; AzSvgLine payload; } AzSvgPathElementVariant_Line;
typedef struct { AzSvgPathElementTag tag; AzSvgQuadraticCurve payload; } AzSvgPathElementVariant_QuadraticCurve;
typedef struct { AzSvgPathElementTag tag; AzSvgCubicCurve payload; } AzSvgPathElementVariant_CubicCurve;

typedef union {
    AzSvgPathElementVariant_Line Line;
    AzSvgPathElementVariant_QuadraticCurve QuadraticCurve;
    AzSvgPathElementVariant_CubicCurve CubicCurve;
} AzSvgPathElement;

typedef struct {
    AzSvgVertexVec vertices;
    AzU32Vec indices;
} AzTesselatedCPUSvgNode;

typedef enum {
   AzSvgStyleTag_Fill,
   AzSvgStyleTag_Stroke,
} AzSvgStyleTag;

typedef struct { AzSvgStyleTag tag; AzSvgFillStyle payload; } AzSvgStyleVariant_Fill;
typedef struct { AzSvgStyleTag tag; AzSvgStrokeStyle payload; } AzSvgStyleVariant_Stroke;

typedef union {
    AzSvgStyleVariant_Fill Fill;
    AzSvgStyleVariant_Stroke Stroke;
} AzSvgStyle;

typedef struct {
    AzU8Vec vec;
} AzString;

typedef struct {
    AzSvgPathElement* const ptr;
    size_t len;
    size_t cap;
    AzSvgPathElementVecDestructor destructor;
} AzSvgPathElementVec;

typedef struct {
    AzCallbackData* const ptr;
    size_t len;
    size_t cap;
    AzCallbackDataVecDestructor destructor;
} AzCallbackDataVec;

typedef struct {
    AzString* const ptr;
    size_t len;
    size_t cap;
    AzStringVecDestructor destructor;
} AzStringVec;

typedef enum {
   AzOptionRawImageTag_None,
   AzOptionRawImageTag_Some,
} AzOptionRawImageTag;

typedef struct { AzOptionRawImageTag tag; } AzOptionRawImageVariant_None;
typedef struct { AzOptionRawImageTag tag; AzRawImage payload; } AzOptionRawImageVariant_Some;

typedef union {
    AzOptionRawImageVariant_None None;
    AzOptionRawImageVariant_Some Some;
} AzOptionRawImage;

typedef enum {
   AzOptionTaskBarIconTag_None,
   AzOptionTaskBarIconTag_Some,
} AzOptionTaskBarIconTag;

typedef struct { AzOptionTaskBarIconTag tag; } AzOptionTaskBarIconVariant_None;
typedef struct { AzOptionTaskBarIconTag tag; AzTaskBarIcon payload; } AzOptionTaskBarIconVariant_Some;

typedef union {
    AzOptionTaskBarIconVariant_None None;
    AzOptionTaskBarIconVariant_Some Some;
} AzOptionTaskBarIcon;

typedef enum {
   AzOptionWindowIconTag_None,
   AzOptionWindowIconTag_Some,
} AzOptionWindowIconTag;

typedef struct { AzOptionWindowIconTag tag; } AzOptionWindowIconVariant_None;
typedef struct { AzOptionWindowIconTag tag; AzWindowIcon payload; } AzOptionWindowIconVariant_Some;

typedef union {
    AzOptionWindowIconVariant_None None;
    AzOptionWindowIconVariant_Some Some;
} AzOptionWindowIcon;

typedef enum {
   AzOptionStringTag_None,
   AzOptionStringTag_Some,
} AzOptionStringTag;

typedef struct { AzOptionStringTag tag; } AzOptionStringVariant_None;
typedef struct { AzOptionStringTag tag; AzString payload; } AzOptionStringVariant_Some;

typedef union {
    AzOptionStringVariant_None None;
    AzOptionStringVariant_Some Some;
} AzOptionString;

typedef enum {
   AzOptionDurationTag_None,
   AzOptionDurationTag_Some,
} AzOptionDurationTag;

typedef struct { AzOptionDurationTag tag; } AzOptionDurationVariant_None;
typedef struct { AzOptionDurationTag tag; AzDuration payload; } AzOptionDurationVariant_Some;

typedef union {
    AzOptionDurationVariant_None None;
    AzOptionDurationVariant_Some Some;
} AzOptionDuration;

typedef enum {
   AzOptionInstantTag_None,
   AzOptionInstantTag_Some,
} AzOptionInstantTag;

typedef struct { AzOptionInstantTag tag; } AzOptionInstantVariant_None;
typedef struct { AzOptionInstantTag tag; AzInstant payload; } AzOptionInstantVariant_Some;

typedef union {
    AzOptionInstantVariant_None None;
    AzOptionInstantVariant_Some Some;
} AzOptionInstant;

typedef struct {
    AzString ns;
    AzSvgParseErrorPosition pos;
} AzDuplicatedNamespaceError;

typedef struct {
    AzString ns;
    AzSvgParseErrorPosition pos;
} AzUnknownNamespaceError;

typedef struct {
    AzString expected;
    AzString actual;
    AzSvgParseErrorPosition pos;
} AzUnexpectedCloseTagError;

typedef struct {
    AzString entity;
    AzSvgParseErrorPosition pos;
} AzUnknownEntityReferenceError;

typedef struct {
    AzString attribute;
    AzSvgParseErrorPosition pos;
} AzDuplicatedAttributeError;

typedef struct {
    AzString got;
    AzSvgParseErrorPosition pos;
} AzInvalidStringError;

typedef struct {
    bool  allow_drag_drop;
    bool  no_redirection_bitmap;
    AzOptionWindowIcon window_icon;
    AzOptionTaskBarIcon taskbar_icon;
    AzOptionHwndHandle parent_window;
} AzWindowsWindowOptions;

typedef struct {
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
    AzString title_bar_font;
    float title_bar_font_size;
} AzWaylandTheme;

typedef struct {
    AzString key;
    AzString value;
} AzStringPair;

typedef struct {
    AzMonitorHandle handle;
    AzOptionString name;
    AzLayoutSize size;
    AzLayoutPoint position;
    double scale_factor;
    AzVideoModeVec video_modes;
    bool  is_primary_monitor;
} AzMonitor;

typedef struct {
    size_t num_copies;
    size_t num_refs;
    size_t num_mutable_refs;
    size_t _internal_len;
    size_t _internal_layout_size;
    size_t _internal_layout_align;
    uint64_t type_id;
    AzString type_name;
    AzRefAnyDestructorType custom_destructor;
} AzRefCountInner;

typedef enum {
   AzNodeTypeTag_Div,
   AzNodeTypeTag_Body,
   AzNodeTypeTag_Br,
   AzNodeTypeTag_Label,
   AzNodeTypeTag_Image,
   AzNodeTypeTag_IFrame,
   AzNodeTypeTag_GlTexture,
} AzNodeTypeTag;

typedef struct { AzNodeTypeTag tag; } AzNodeTypeVariant_Div;
typedef struct { AzNodeTypeTag tag; } AzNodeTypeVariant_Body;
typedef struct { AzNodeTypeTag tag; } AzNodeTypeVariant_Br;
typedef struct { AzNodeTypeTag tag; AzString payload; } AzNodeTypeVariant_Label;
typedef struct { AzNodeTypeTag tag; AzImageId payload; } AzNodeTypeVariant_Image;
typedef struct { AzNodeTypeTag tag; AzIFrameNode payload; } AzNodeTypeVariant_IFrame;
typedef struct { AzNodeTypeTag tag; AzGlTextureNode payload; } AzNodeTypeVariant_GlTexture;

typedef union {
    AzNodeTypeVariant_Div Div;
    AzNodeTypeVariant_Body Body;
    AzNodeTypeVariant_Br Br;
    AzNodeTypeVariant_Label Label;
    AzNodeTypeVariant_Image Image;
    AzNodeTypeVariant_IFrame IFrame;
    AzNodeTypeVariant_GlTexture GlTexture;
} AzNodeType;

typedef enum {
   AzIdOrClassTag_Id,
   AzIdOrClassTag_Class,
} AzIdOrClassTag;

typedef struct { AzIdOrClassTag tag; AzString payload; } AzIdOrClassVariant_Id;
typedef struct { AzIdOrClassTag tag; AzString payload; } AzIdOrClassVariant_Class;

typedef union {
    AzIdOrClassVariant_Id Id;
    AzIdOrClassVariant_Class Class;
} AzIdOrClass;

typedef enum {
   AzCssPathSelectorTag_Global,
   AzCssPathSelectorTag_Type,
   AzCssPathSelectorTag_Class,
   AzCssPathSelectorTag_Id,
   AzCssPathSelectorTag_PseudoSelector,
   AzCssPathSelectorTag_DirectChildren,
   AzCssPathSelectorTag_Children,
} AzCssPathSelectorTag;

typedef struct { AzCssPathSelectorTag tag; } AzCssPathSelectorVariant_Global;
typedef struct { AzCssPathSelectorTag tag; AzNodeTypePath payload; } AzCssPathSelectorVariant_Type;
typedef struct { AzCssPathSelectorTag tag; AzString payload; } AzCssPathSelectorVariant_Class;
typedef struct { AzCssPathSelectorTag tag; AzString payload; } AzCssPathSelectorVariant_Id;
typedef struct { AzCssPathSelectorTag tag; AzCssPathPseudoSelector payload; } AzCssPathSelectorVariant_PseudoSelector;
typedef struct { AzCssPathSelectorTag tag; } AzCssPathSelectorVariant_DirectChildren;
typedef struct { AzCssPathSelectorTag tag; } AzCssPathSelectorVariant_Children;

typedef union {
    AzCssPathSelectorVariant_Global Global;
    AzCssPathSelectorVariant_Type Type;
    AzCssPathSelectorVariant_Class Class;
    AzCssPathSelectorVariant_Id Id;
    AzCssPathSelectorVariant_PseudoSelector PseudoSelector;
    AzCssPathSelectorVariant_DirectChildren DirectChildren;
    AzCssPathSelectorVariant_Children Children;
} AzCssPathSelector;

typedef struct {
    AzString inner;
} AzCssImageId;

typedef enum {
   AzStyleBackgroundContentTag_LinearGradient,
   AzStyleBackgroundContentTag_RadialGradient,
   AzStyleBackgroundContentTag_ConicGradient,
   AzStyleBackgroundContentTag_Image,
   AzStyleBackgroundContentTag_Color,
} AzStyleBackgroundContentTag;

typedef struct { AzStyleBackgroundContentTag tag; AzLinearGradient payload; } AzStyleBackgroundContentVariant_LinearGradient;
typedef struct { AzStyleBackgroundContentTag tag; AzRadialGradient payload; } AzStyleBackgroundContentVariant_RadialGradient;
typedef struct { AzStyleBackgroundContentTag tag; AzConicGradient payload; } AzStyleBackgroundContentVariant_ConicGradient;
typedef struct { AzStyleBackgroundContentTag tag; AzCssImageId payload; } AzStyleBackgroundContentVariant_Image;
typedef struct { AzStyleBackgroundContentTag tag; AzColorU payload; } AzStyleBackgroundContentVariant_Color;

typedef union {
    AzStyleBackgroundContentVariant_LinearGradient LinearGradient;
    AzStyleBackgroundContentVariant_RadialGradient RadialGradient;
    AzStyleBackgroundContentVariant_ConicGradient ConicGradient;
    AzStyleBackgroundContentVariant_Image Image;
    AzStyleBackgroundContentVariant_Color Color;
} AzStyleBackgroundContent;

typedef struct {
    AzLayoutWidth width;
    AzLayoutPaddingLeft padding_left;
    AzLayoutPaddingRight padding_right;
    AzStyleBackgroundContent track;
    AzStyleBackgroundContent thumb;
    AzStyleBackgroundContent button;
    AzStyleBackgroundContent corner;
    AzStyleBackgroundContent resizer;
} AzScrollbarInfo;

typedef struct {
    AzScrollbarInfo horizontal;
    AzScrollbarInfo vertical;
} AzScrollbarStyle;

typedef struct {
    AzStringVec fonts;
} AzStyleFontFamily;

typedef enum {
   AzScrollbarStyleValueTag_Auto,
   AzScrollbarStyleValueTag_None,
   AzScrollbarStyleValueTag_Inherit,
   AzScrollbarStyleValueTag_Initial,
   AzScrollbarStyleValueTag_Exact,
} AzScrollbarStyleValueTag;

typedef struct { AzScrollbarStyleValueTag tag; } AzScrollbarStyleValueVariant_Auto;
typedef struct { AzScrollbarStyleValueTag tag; } AzScrollbarStyleValueVariant_None;
typedef struct { AzScrollbarStyleValueTag tag; } AzScrollbarStyleValueVariant_Inherit;
typedef struct { AzScrollbarStyleValueTag tag; } AzScrollbarStyleValueVariant_Initial;
typedef struct { AzScrollbarStyleValueTag tag; AzScrollbarStyle payload; } AzScrollbarStyleValueVariant_Exact;

typedef union {
    AzScrollbarStyleValueVariant_Auto Auto;
    AzScrollbarStyleValueVariant_None None;
    AzScrollbarStyleValueVariant_Inherit Inherit;
    AzScrollbarStyleValueVariant_Initial Initial;
    AzScrollbarStyleValueVariant_Exact Exact;
} AzScrollbarStyleValue;

typedef enum {
   AzStyleFontFamilyValueTag_Auto,
   AzStyleFontFamilyValueTag_None,
   AzStyleFontFamilyValueTag_Inherit,
   AzStyleFontFamilyValueTag_Initial,
   AzStyleFontFamilyValueTag_Exact,
} AzStyleFontFamilyValueTag;

typedef struct { AzStyleFontFamilyValueTag tag; } AzStyleFontFamilyValueVariant_Auto;
typedef struct { AzStyleFontFamilyValueTag tag; } AzStyleFontFamilyValueVariant_None;
typedef struct { AzStyleFontFamilyValueTag tag; } AzStyleFontFamilyValueVariant_Inherit;
typedef struct { AzStyleFontFamilyValueTag tag; } AzStyleFontFamilyValueVariant_Initial;
typedef struct { AzStyleFontFamilyValueTag tag; AzStyleFontFamily payload; } AzStyleFontFamilyValueVariant_Exact;

typedef union {
    AzStyleFontFamilyValueVariant_Auto Auto;
    AzStyleFontFamilyValueVariant_None None;
    AzStyleFontFamilyValueVariant_Inherit Inherit;
    AzStyleFontFamilyValueVariant_Initial Initial;
    AzStyleFontFamilyValueVariant_Exact Exact;
} AzStyleFontFamilyValue;

typedef struct {
    AzString name;
    AzOptionUsize layout_location;
    AzVertexAttributeType attribute_type;
    size_t item_count;
} AzVertexAttribute;

typedef struct {
    AzString message;
    uint32_t source;
    uint32_t ty;
    uint32_t id;
    uint32_t severity;
} AzDebugMessage;

typedef struct {
    int32_t _0;
    uint32_t _1;
    AzString _2;
} AzGetActiveAttribReturn;

typedef struct {
    int32_t _0;
    uint32_t _1;
    AzString _2;
} AzGetActiveUniformReturn;

typedef enum {
   AzImageSourceTag_Embedded,
   AzImageSourceTag_File,
   AzImageSourceTag_Raw,
} AzImageSourceTag;

typedef struct { AzImageSourceTag tag; AzU8Vec payload; } AzImageSourceVariant_Embedded;
typedef struct { AzImageSourceTag tag; AzString payload; } AzImageSourceVariant_File;
typedef struct { AzImageSourceTag tag; AzRawImage payload; } AzImageSourceVariant_Raw;

typedef union {
    AzImageSourceVariant_Embedded Embedded;
    AzImageSourceVariant_File File;
    AzImageSourceVariant_Raw Raw;
} AzImageSource;

typedef struct {
    AzString postscript_id;
    AzU8Vec font_data;
    bool  load_glyph_outlines;
} AzEmbeddedFontSource;

typedef struct {
    AzString postscript_id;
    AzString file_path;
    bool  load_glyph_outlines;
} AzFileFontSource;

typedef struct {
    AzString postscript_id;
    bool  load_glyph_outlines;
} AzSystemFontSource;

typedef struct {
    AzSvgPathElementVec items;
} AzSvgPath;

typedef struct {
    AzOptionString relative_image_path;
    float dpi;
    AzString default_font_family;
    float font_size;
    AzStringVec languages;
    AzShapeRendering shape_rendering;
    AzTextRendering text_rendering;
    AzImageRendering image_rendering;
    bool  keep_named_groups;
    AzFontDatabase fontdb;
} AzSvgParseOptions;

typedef struct {
    AzRefAny data;
    AzInstant created;
    AzOptionInstant last_run;
    size_t run_count;
    AzOptionDuration delay;
    AzOptionDuration interval;
    AzOptionDuration timeout;
    AzTimerCallback callback;
} AzTimer;

typedef struct {
    AzMonitor* const ptr;
    size_t len;
    size_t cap;
    AzMonitorVecDestructor destructor;
} AzMonitorVec;

typedef struct {
    AzIdOrClass* const ptr;
    size_t len;
    size_t cap;
    AzIdOrClassVecDestructor destructor;
} AzIdOrClassVec;

typedef struct {
    AzStyleBackgroundContent* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundContentVecDestructor destructor;
} AzStyleBackgroundContentVec;

typedef struct {
    AzSvgPath* const ptr;
    size_t len;
    size_t cap;
    AzSvgPathVecDestructor destructor;
} AzSvgPathVec;

typedef struct {
    AzVertexAttribute* const ptr;
    size_t len;
    size_t cap;
    AzVertexAttributeVecDestructor destructor;
} AzVertexAttributeVec;

typedef struct {
    AzCssPathSelector* const ptr;
    size_t len;
    size_t cap;
    AzCssPathSelectorVecDestructor destructor;
} AzCssPathSelectorVec;

typedef struct {
    AzDebugMessage* const ptr;
    size_t len;
    size_t cap;
    AzDebugMessageVecDestructor destructor;
} AzDebugMessageVec;

typedef struct {
    AzStringPair* const ptr;
    size_t len;
    size_t cap;
    AzStringPairVecDestructor destructor;
} AzStringPairVec;

typedef enum {
   AzOptionWaylandThemeTag_None,
   AzOptionWaylandThemeTag_Some,
} AzOptionWaylandThemeTag;

typedef struct { AzOptionWaylandThemeTag tag; } AzOptionWaylandThemeVariant_None;
typedef struct { AzOptionWaylandThemeTag tag; AzWaylandTheme payload; } AzOptionWaylandThemeVariant_Some;

typedef union {
    AzOptionWaylandThemeVariant_None None;
    AzOptionWaylandThemeVariant_Some Some;
} AzOptionWaylandTheme;

typedef enum {
   AzXmlStreamErrorTag_UnexpectedEndOfStream,
   AzXmlStreamErrorTag_InvalidName,
   AzXmlStreamErrorTag_NonXmlChar,
   AzXmlStreamErrorTag_InvalidChar,
   AzXmlStreamErrorTag_InvalidCharMultiple,
   AzXmlStreamErrorTag_InvalidQuote,
   AzXmlStreamErrorTag_InvalidSpace,
   AzXmlStreamErrorTag_InvalidString,
   AzXmlStreamErrorTag_InvalidReference,
   AzXmlStreamErrorTag_InvalidExternalID,
   AzXmlStreamErrorTag_InvalidCommentData,
   AzXmlStreamErrorTag_InvalidCommentEnd,
   AzXmlStreamErrorTag_InvalidCharacterData,
} AzXmlStreamErrorTag;

typedef struct { AzXmlStreamErrorTag tag; } AzXmlStreamErrorVariant_UnexpectedEndOfStream;
typedef struct { AzXmlStreamErrorTag tag; } AzXmlStreamErrorVariant_InvalidName;
typedef struct { AzXmlStreamErrorTag tag; AzNonXmlCharError payload; } AzXmlStreamErrorVariant_NonXmlChar;
typedef struct { AzXmlStreamErrorTag tag; AzInvalidCharError payload; } AzXmlStreamErrorVariant_InvalidChar;
typedef struct { AzXmlStreamErrorTag tag; AzInvalidCharMultipleError payload; } AzXmlStreamErrorVariant_InvalidCharMultiple;
typedef struct { AzXmlStreamErrorTag tag; AzInvalidQuoteError payload; } AzXmlStreamErrorVariant_InvalidQuote;
typedef struct { AzXmlStreamErrorTag tag; AzInvalidSpaceError payload; } AzXmlStreamErrorVariant_InvalidSpace;
typedef struct { AzXmlStreamErrorTag tag; AzInvalidStringError payload; } AzXmlStreamErrorVariant_InvalidString;
typedef struct { AzXmlStreamErrorTag tag; } AzXmlStreamErrorVariant_InvalidReference;
typedef struct { AzXmlStreamErrorTag tag; } AzXmlStreamErrorVariant_InvalidExternalID;
typedef struct { AzXmlStreamErrorTag tag; } AzXmlStreamErrorVariant_InvalidCommentData;
typedef struct { AzXmlStreamErrorTag tag; } AzXmlStreamErrorVariant_InvalidCommentEnd;
typedef struct { AzXmlStreamErrorTag tag; } AzXmlStreamErrorVariant_InvalidCharacterData;

typedef union {
    AzXmlStreamErrorVariant_UnexpectedEndOfStream UnexpectedEndOfStream;
    AzXmlStreamErrorVariant_InvalidName InvalidName;
    AzXmlStreamErrorVariant_NonXmlChar NonXmlChar;
    AzXmlStreamErrorVariant_InvalidChar InvalidChar;
    AzXmlStreamErrorVariant_InvalidCharMultiple InvalidCharMultiple;
    AzXmlStreamErrorVariant_InvalidQuote InvalidQuote;
    AzXmlStreamErrorVariant_InvalidSpace InvalidSpace;
    AzXmlStreamErrorVariant_InvalidString InvalidString;
    AzXmlStreamErrorVariant_InvalidReference InvalidReference;
    AzXmlStreamErrorVariant_InvalidExternalID InvalidExternalID;
    AzXmlStreamErrorVariant_InvalidCommentData InvalidCommentData;
    AzXmlStreamErrorVariant_InvalidCommentEnd InvalidCommentEnd;
    AzXmlStreamErrorVariant_InvalidCharacterData InvalidCharacterData;
} AzXmlStreamError;

typedef struct {
    AzOptionX11Visual x11_visual;
    AzOptionI32 x11_screen;
    AzStringPairVec x11_wm_classes;
    bool  x11_override_redirect;
    AzXWindowTypeVec x11_window_types;
    AzOptionString x11_gtk_theme_variant;
    AzOptionLogicalSize x11_resize_increments;
    AzOptionLogicalSize x11_base_size;
    AzOptionString wayland_app_id;
    AzOptionWaylandTheme wayland_theme;
    bool  request_user_attention;
    AzOptionWindowIcon window_icon;
} AzLinuxWindowOptions;

typedef struct {
    AzCssPathSelectorVec selectors;
} AzCssPath;

typedef enum {
   AzStyleBackgroundContentVecValueTag_Auto,
   AzStyleBackgroundContentVecValueTag_None,
   AzStyleBackgroundContentVecValueTag_Inherit,
   AzStyleBackgroundContentVecValueTag_Initial,
   AzStyleBackgroundContentVecValueTag_Exact,
} AzStyleBackgroundContentVecValueTag;

typedef struct { AzStyleBackgroundContentVecValueTag tag; } AzStyleBackgroundContentVecValueVariant_Auto;
typedef struct { AzStyleBackgroundContentVecValueTag tag; } AzStyleBackgroundContentVecValueVariant_None;
typedef struct { AzStyleBackgroundContentVecValueTag tag; } AzStyleBackgroundContentVecValueVariant_Inherit;
typedef struct { AzStyleBackgroundContentVecValueTag tag; } AzStyleBackgroundContentVecValueVariant_Initial;
typedef struct { AzStyleBackgroundContentVecValueTag tag; AzStyleBackgroundContentVec payload; } AzStyleBackgroundContentVecValueVariant_Exact;

typedef union {
    AzStyleBackgroundContentVecValueVariant_Auto Auto;
    AzStyleBackgroundContentVecValueVariant_None None;
    AzStyleBackgroundContentVecValueVariant_Inherit Inherit;
    AzStyleBackgroundContentVecValueVariant_Initial Initial;
    AzStyleBackgroundContentVecValueVariant_Exact Exact;
} AzStyleBackgroundContentVecValue;

typedef enum {
   AzCssPropertyTag_TextColor,
   AzCssPropertyTag_FontSize,
   AzCssPropertyTag_FontFamily,
   AzCssPropertyTag_TextAlign,
   AzCssPropertyTag_LetterSpacing,
   AzCssPropertyTag_LineHeight,
   AzCssPropertyTag_WordSpacing,
   AzCssPropertyTag_TabWidth,
   AzCssPropertyTag_Cursor,
   AzCssPropertyTag_Display,
   AzCssPropertyTag_Float,
   AzCssPropertyTag_BoxSizing,
   AzCssPropertyTag_Width,
   AzCssPropertyTag_Height,
   AzCssPropertyTag_MinWidth,
   AzCssPropertyTag_MinHeight,
   AzCssPropertyTag_MaxWidth,
   AzCssPropertyTag_MaxHeight,
   AzCssPropertyTag_Position,
   AzCssPropertyTag_Top,
   AzCssPropertyTag_Right,
   AzCssPropertyTag_Left,
   AzCssPropertyTag_Bottom,
   AzCssPropertyTag_FlexWrap,
   AzCssPropertyTag_FlexDirection,
   AzCssPropertyTag_FlexGrow,
   AzCssPropertyTag_FlexShrink,
   AzCssPropertyTag_JustifyContent,
   AzCssPropertyTag_AlignItems,
   AzCssPropertyTag_AlignContent,
   AzCssPropertyTag_BackgroundContent,
   AzCssPropertyTag_BackgroundPosition,
   AzCssPropertyTag_BackgroundSize,
   AzCssPropertyTag_BackgroundRepeat,
   AzCssPropertyTag_OverflowX,
   AzCssPropertyTag_OverflowY,
   AzCssPropertyTag_PaddingTop,
   AzCssPropertyTag_PaddingLeft,
   AzCssPropertyTag_PaddingRight,
   AzCssPropertyTag_PaddingBottom,
   AzCssPropertyTag_MarginTop,
   AzCssPropertyTag_MarginLeft,
   AzCssPropertyTag_MarginRight,
   AzCssPropertyTag_MarginBottom,
   AzCssPropertyTag_BorderTopLeftRadius,
   AzCssPropertyTag_BorderTopRightRadius,
   AzCssPropertyTag_BorderBottomLeftRadius,
   AzCssPropertyTag_BorderBottomRightRadius,
   AzCssPropertyTag_BorderTopColor,
   AzCssPropertyTag_BorderRightColor,
   AzCssPropertyTag_BorderLeftColor,
   AzCssPropertyTag_BorderBottomColor,
   AzCssPropertyTag_BorderTopStyle,
   AzCssPropertyTag_BorderRightStyle,
   AzCssPropertyTag_BorderLeftStyle,
   AzCssPropertyTag_BorderBottomStyle,
   AzCssPropertyTag_BorderTopWidth,
   AzCssPropertyTag_BorderRightWidth,
   AzCssPropertyTag_BorderLeftWidth,
   AzCssPropertyTag_BorderBottomWidth,
   AzCssPropertyTag_BoxShadowLeft,
   AzCssPropertyTag_BoxShadowRight,
   AzCssPropertyTag_BoxShadowTop,
   AzCssPropertyTag_BoxShadowBottom,
   AzCssPropertyTag_ScrollbarStyle,
   AzCssPropertyTag_Opacity,
   AzCssPropertyTag_Transform,
   AzCssPropertyTag_TransformOrigin,
   AzCssPropertyTag_PerspectiveOrigin,
   AzCssPropertyTag_BackfaceVisibility,
} AzCssPropertyTag;

typedef struct { AzCssPropertyTag tag; AzStyleTextColorValue payload; } AzCssPropertyVariant_TextColor;
typedef struct { AzCssPropertyTag tag; AzStyleFontSizeValue payload; } AzCssPropertyVariant_FontSize;
typedef struct { AzCssPropertyTag tag; AzStyleFontFamilyValue payload; } AzCssPropertyVariant_FontFamily;
typedef struct { AzCssPropertyTag tag; AzStyleTextAlignmentHorzValue payload; } AzCssPropertyVariant_TextAlign;
typedef struct { AzCssPropertyTag tag; AzStyleLetterSpacingValue payload; } AzCssPropertyVariant_LetterSpacing;
typedef struct { AzCssPropertyTag tag; AzStyleLineHeightValue payload; } AzCssPropertyVariant_LineHeight;
typedef struct { AzCssPropertyTag tag; AzStyleWordSpacingValue payload; } AzCssPropertyVariant_WordSpacing;
typedef struct { AzCssPropertyTag tag; AzStyleTabWidthValue payload; } AzCssPropertyVariant_TabWidth;
typedef struct { AzCssPropertyTag tag; AzStyleCursorValue payload; } AzCssPropertyVariant_Cursor;
typedef struct { AzCssPropertyTag tag; AzLayoutDisplayValue payload; } AzCssPropertyVariant_Display;
typedef struct { AzCssPropertyTag tag; AzLayoutFloatValue payload; } AzCssPropertyVariant_Float;
typedef struct { AzCssPropertyTag tag; AzLayoutBoxSizingValue payload; } AzCssPropertyVariant_BoxSizing;
typedef struct { AzCssPropertyTag tag; AzLayoutWidthValue payload; } AzCssPropertyVariant_Width;
typedef struct { AzCssPropertyTag tag; AzLayoutHeightValue payload; } AzCssPropertyVariant_Height;
typedef struct { AzCssPropertyTag tag; AzLayoutMinWidthValue payload; } AzCssPropertyVariant_MinWidth;
typedef struct { AzCssPropertyTag tag; AzLayoutMinHeightValue payload; } AzCssPropertyVariant_MinHeight;
typedef struct { AzCssPropertyTag tag; AzLayoutMaxWidthValue payload; } AzCssPropertyVariant_MaxWidth;
typedef struct { AzCssPropertyTag tag; AzLayoutMaxHeightValue payload; } AzCssPropertyVariant_MaxHeight;
typedef struct { AzCssPropertyTag tag; AzLayoutPositionValue payload; } AzCssPropertyVariant_Position;
typedef struct { AzCssPropertyTag tag; AzLayoutTopValue payload; } AzCssPropertyVariant_Top;
typedef struct { AzCssPropertyTag tag; AzLayoutRightValue payload; } AzCssPropertyVariant_Right;
typedef struct { AzCssPropertyTag tag; AzLayoutLeftValue payload; } AzCssPropertyVariant_Left;
typedef struct { AzCssPropertyTag tag; AzLayoutBottomValue payload; } AzCssPropertyVariant_Bottom;
typedef struct { AzCssPropertyTag tag; AzLayoutFlexWrapValue payload; } AzCssPropertyVariant_FlexWrap;
typedef struct { AzCssPropertyTag tag; AzLayoutFlexDirectionValue payload; } AzCssPropertyVariant_FlexDirection;
typedef struct { AzCssPropertyTag tag; AzLayoutFlexGrowValue payload; } AzCssPropertyVariant_FlexGrow;
typedef struct { AzCssPropertyTag tag; AzLayoutFlexShrinkValue payload; } AzCssPropertyVariant_FlexShrink;
typedef struct { AzCssPropertyTag tag; AzLayoutJustifyContentValue payload; } AzCssPropertyVariant_JustifyContent;
typedef struct { AzCssPropertyTag tag; AzLayoutAlignItemsValue payload; } AzCssPropertyVariant_AlignItems;
typedef struct { AzCssPropertyTag tag; AzLayoutAlignContentValue payload; } AzCssPropertyVariant_AlignContent;
typedef struct { AzCssPropertyTag tag; AzStyleBackgroundContentVecValue payload; } AzCssPropertyVariant_BackgroundContent;
typedef struct { AzCssPropertyTag tag; AzStyleBackgroundPositionVecValue payload; } AzCssPropertyVariant_BackgroundPosition;
typedef struct { AzCssPropertyTag tag; AzStyleBackgroundSizeVecValue payload; } AzCssPropertyVariant_BackgroundSize;
typedef struct { AzCssPropertyTag tag; AzStyleBackgroundRepeatVecValue payload; } AzCssPropertyVariant_BackgroundRepeat;
typedef struct { AzCssPropertyTag tag; AzLayoutOverflowValue payload; } AzCssPropertyVariant_OverflowX;
typedef struct { AzCssPropertyTag tag; AzLayoutOverflowValue payload; } AzCssPropertyVariant_OverflowY;
typedef struct { AzCssPropertyTag tag; AzLayoutPaddingTopValue payload; } AzCssPropertyVariant_PaddingTop;
typedef struct { AzCssPropertyTag tag; AzLayoutPaddingLeftValue payload; } AzCssPropertyVariant_PaddingLeft;
typedef struct { AzCssPropertyTag tag; AzLayoutPaddingRightValue payload; } AzCssPropertyVariant_PaddingRight;
typedef struct { AzCssPropertyTag tag; AzLayoutPaddingBottomValue payload; } AzCssPropertyVariant_PaddingBottom;
typedef struct { AzCssPropertyTag tag; AzLayoutMarginTopValue payload; } AzCssPropertyVariant_MarginTop;
typedef struct { AzCssPropertyTag tag; AzLayoutMarginLeftValue payload; } AzCssPropertyVariant_MarginLeft;
typedef struct { AzCssPropertyTag tag; AzLayoutMarginRightValue payload; } AzCssPropertyVariant_MarginRight;
typedef struct { AzCssPropertyTag tag; AzLayoutMarginBottomValue payload; } AzCssPropertyVariant_MarginBottom;
typedef struct { AzCssPropertyTag tag; AzStyleBorderTopLeftRadiusValue payload; } AzCssPropertyVariant_BorderTopLeftRadius;
typedef struct { AzCssPropertyTag tag; AzStyleBorderTopRightRadiusValue payload; } AzCssPropertyVariant_BorderTopRightRadius;
typedef struct { AzCssPropertyTag tag; AzStyleBorderBottomLeftRadiusValue payload; } AzCssPropertyVariant_BorderBottomLeftRadius;
typedef struct { AzCssPropertyTag tag; AzStyleBorderBottomRightRadiusValue payload; } AzCssPropertyVariant_BorderBottomRightRadius;
typedef struct { AzCssPropertyTag tag; AzStyleBorderTopColorValue payload; } AzCssPropertyVariant_BorderTopColor;
typedef struct { AzCssPropertyTag tag; AzStyleBorderRightColorValue payload; } AzCssPropertyVariant_BorderRightColor;
typedef struct { AzCssPropertyTag tag; AzStyleBorderLeftColorValue payload; } AzCssPropertyVariant_BorderLeftColor;
typedef struct { AzCssPropertyTag tag; AzStyleBorderBottomColorValue payload; } AzCssPropertyVariant_BorderBottomColor;
typedef struct { AzCssPropertyTag tag; AzStyleBorderTopStyleValue payload; } AzCssPropertyVariant_BorderTopStyle;
typedef struct { AzCssPropertyTag tag; AzStyleBorderRightStyleValue payload; } AzCssPropertyVariant_BorderRightStyle;
typedef struct { AzCssPropertyTag tag; AzStyleBorderLeftStyleValue payload; } AzCssPropertyVariant_BorderLeftStyle;
typedef struct { AzCssPropertyTag tag; AzStyleBorderBottomStyleValue payload; } AzCssPropertyVariant_BorderBottomStyle;
typedef struct { AzCssPropertyTag tag; AzLayoutBorderTopWidthValue payload; } AzCssPropertyVariant_BorderTopWidth;
typedef struct { AzCssPropertyTag tag; AzLayoutBorderRightWidthValue payload; } AzCssPropertyVariant_BorderRightWidth;
typedef struct { AzCssPropertyTag tag; AzLayoutBorderLeftWidthValue payload; } AzCssPropertyVariant_BorderLeftWidth;
typedef struct { AzCssPropertyTag tag; AzLayoutBorderBottomWidthValue payload; } AzCssPropertyVariant_BorderBottomWidth;
typedef struct { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; } AzCssPropertyVariant_BoxShadowLeft;
typedef struct { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; } AzCssPropertyVariant_BoxShadowRight;
typedef struct { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; } AzCssPropertyVariant_BoxShadowTop;
typedef struct { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; } AzCssPropertyVariant_BoxShadowBottom;
typedef struct { AzCssPropertyTag tag; AzScrollbarStyleValue payload; } AzCssPropertyVariant_ScrollbarStyle;
typedef struct { AzCssPropertyTag tag; AzStyleOpacityValue payload; } AzCssPropertyVariant_Opacity;
typedef struct { AzCssPropertyTag tag; AzStyleTransformVecValue payload; } AzCssPropertyVariant_Transform;
typedef struct { AzCssPropertyTag tag; AzStyleTransformOriginValue payload; } AzCssPropertyVariant_TransformOrigin;
typedef struct { AzCssPropertyTag tag; AzStylePerspectiveOriginValue payload; } AzCssPropertyVariant_PerspectiveOrigin;
typedef struct { AzCssPropertyTag tag; AzStyleBackfaceVisibilityValue payload; } AzCssPropertyVariant_BackfaceVisibility;

typedef union {
    AzCssPropertyVariant_TextColor TextColor;
    AzCssPropertyVariant_FontSize FontSize;
    AzCssPropertyVariant_FontFamily FontFamily;
    AzCssPropertyVariant_TextAlign TextAlign;
    AzCssPropertyVariant_LetterSpacing LetterSpacing;
    AzCssPropertyVariant_LineHeight LineHeight;
    AzCssPropertyVariant_WordSpacing WordSpacing;
    AzCssPropertyVariant_TabWidth TabWidth;
    AzCssPropertyVariant_Cursor Cursor;
    AzCssPropertyVariant_Display Display;
    AzCssPropertyVariant_Float Float;
    AzCssPropertyVariant_BoxSizing BoxSizing;
    AzCssPropertyVariant_Width Width;
    AzCssPropertyVariant_Height Height;
    AzCssPropertyVariant_MinWidth MinWidth;
    AzCssPropertyVariant_MinHeight MinHeight;
    AzCssPropertyVariant_MaxWidth MaxWidth;
    AzCssPropertyVariant_MaxHeight MaxHeight;
    AzCssPropertyVariant_Position Position;
    AzCssPropertyVariant_Top Top;
    AzCssPropertyVariant_Right Right;
    AzCssPropertyVariant_Left Left;
    AzCssPropertyVariant_Bottom Bottom;
    AzCssPropertyVariant_FlexWrap FlexWrap;
    AzCssPropertyVariant_FlexDirection FlexDirection;
    AzCssPropertyVariant_FlexGrow FlexGrow;
    AzCssPropertyVariant_FlexShrink FlexShrink;
    AzCssPropertyVariant_JustifyContent JustifyContent;
    AzCssPropertyVariant_AlignItems AlignItems;
    AzCssPropertyVariant_AlignContent AlignContent;
    AzCssPropertyVariant_BackgroundContent BackgroundContent;
    AzCssPropertyVariant_BackgroundPosition BackgroundPosition;
    AzCssPropertyVariant_BackgroundSize BackgroundSize;
    AzCssPropertyVariant_BackgroundRepeat BackgroundRepeat;
    AzCssPropertyVariant_OverflowX OverflowX;
    AzCssPropertyVariant_OverflowY OverflowY;
    AzCssPropertyVariant_PaddingTop PaddingTop;
    AzCssPropertyVariant_PaddingLeft PaddingLeft;
    AzCssPropertyVariant_PaddingRight PaddingRight;
    AzCssPropertyVariant_PaddingBottom PaddingBottom;
    AzCssPropertyVariant_MarginTop MarginTop;
    AzCssPropertyVariant_MarginLeft MarginLeft;
    AzCssPropertyVariant_MarginRight MarginRight;
    AzCssPropertyVariant_MarginBottom MarginBottom;
    AzCssPropertyVariant_BorderTopLeftRadius BorderTopLeftRadius;
    AzCssPropertyVariant_BorderTopRightRadius BorderTopRightRadius;
    AzCssPropertyVariant_BorderBottomLeftRadius BorderBottomLeftRadius;
    AzCssPropertyVariant_BorderBottomRightRadius BorderBottomRightRadius;
    AzCssPropertyVariant_BorderTopColor BorderTopColor;
    AzCssPropertyVariant_BorderRightColor BorderRightColor;
    AzCssPropertyVariant_BorderLeftColor BorderLeftColor;
    AzCssPropertyVariant_BorderBottomColor BorderBottomColor;
    AzCssPropertyVariant_BorderTopStyle BorderTopStyle;
    AzCssPropertyVariant_BorderRightStyle BorderRightStyle;
    AzCssPropertyVariant_BorderLeftStyle BorderLeftStyle;
    AzCssPropertyVariant_BorderBottomStyle BorderBottomStyle;
    AzCssPropertyVariant_BorderTopWidth BorderTopWidth;
    AzCssPropertyVariant_BorderRightWidth BorderRightWidth;
    AzCssPropertyVariant_BorderLeftWidth BorderLeftWidth;
    AzCssPropertyVariant_BorderBottomWidth BorderBottomWidth;
    AzCssPropertyVariant_BoxShadowLeft BoxShadowLeft;
    AzCssPropertyVariant_BoxShadowRight BoxShadowRight;
    AzCssPropertyVariant_BoxShadowTop BoxShadowTop;
    AzCssPropertyVariant_BoxShadowBottom BoxShadowBottom;
    AzCssPropertyVariant_ScrollbarStyle ScrollbarStyle;
    AzCssPropertyVariant_Opacity Opacity;
    AzCssPropertyVariant_Transform Transform;
    AzCssPropertyVariant_TransformOrigin TransformOrigin;
    AzCssPropertyVariant_PerspectiveOrigin PerspectiveOrigin;
    AzCssPropertyVariant_BackfaceVisibility BackfaceVisibility;
} AzCssProperty;

typedef enum {
   AzCssPropertySourceTag_Css,
   AzCssPropertySourceTag_Inline,
} AzCssPropertySourceTag;

typedef struct { AzCssPropertySourceTag tag; AzCssPath payload; } AzCssPropertySourceVariant_Css;
typedef struct { AzCssPropertySourceTag tag; } AzCssPropertySourceVariant_Inline;

typedef union {
    AzCssPropertySourceVariant_Css Css;
    AzCssPropertySourceVariant_Inline Inline;
} AzCssPropertySource;

typedef struct {
    AzVertexAttributeVec fields;
} AzVertexLayout;

typedef struct {
    AzVertexLayout vertex_layout;
    uint32_t vao_id;
    AzGlContextPtr gl_context;
} AzVertexArrayObject;

typedef struct {
    uint32_t vertex_buffer_id;
    size_t vertex_buffer_len;
    AzVertexArrayObject vao;
    uint32_t index_buffer_id;
    size_t index_buffer_len;
    AzIndexBufferFormat index_buffer_format;
} AzVertexBuffer;

typedef enum {
   AzFontSourceTag_Embedded,
   AzFontSourceTag_File,
   AzFontSourceTag_System,
} AzFontSourceTag;

typedef struct { AzFontSourceTag tag; AzEmbeddedFontSource payload; } AzFontSourceVariant_Embedded;
typedef struct { AzFontSourceTag tag; AzFileFontSource payload; } AzFontSourceVariant_File;
typedef struct { AzFontSourceTag tag; AzSystemFontSource payload; } AzFontSourceVariant_System;

typedef union {
    AzFontSourceVariant_Embedded Embedded;
    AzFontSourceVariant_File File;
    AzFontSourceVariant_System System;
} AzFontSource;

typedef struct {
    AzSvgPathVec rings;
} AzSvgMultiPolygon;

typedef struct {
    AzCssProperty* const ptr;
    size_t len;
    size_t cap;
    AzCssPropertyVecDestructor destructor;
} AzCssPropertyVec;

typedef struct {
    AzSvgMultiPolygon* const ptr;
    size_t len;
    size_t cap;
    AzSvgMultiPolygonVecDestructor destructor;
} AzSvgMultiPolygonVec;

typedef struct {
    AzXmlStreamError stream_error;
    AzSvgParseErrorPosition pos;
} AzXmlTextError;

typedef struct {
    AzWindowsWindowOptions windows_options;
    AzLinuxWindowOptions linux_options;
    AzMacWindowOptions mac_options;
    AzWasmWindowOptions wasm_options;
} AzPlatformSpecificOptions;

typedef struct {
    AzString title;
    AzWindowTheme theme;
    AzWindowSize size;
    AzWindowPosition position;
    AzWindowFlags flags;
    AzDebugState debug_state;
    AzKeyboardState keyboard_state;
    AzMouseState mouse_state;
    AzTouchState touch_state;
    AzImePosition ime_position;
    AzMonitor monitor;
    AzPlatformSpecificOptions platform_specific_options;
    AzRendererOptions renderer_options;
    AzColorU background_color;
    AzLayoutCallback layout_callback;
    AzOptionCallback close_callback;
} AzWindowState;

typedef struct {
    void* const current_window_state;
    AzWindowState* restrict modifiable_window_state;
    AzGlContextPtr* const gl_context;
    void* restrict resources;
    void* restrict timers;
    void* restrict threads;
    void* restrict new_windows;
    AzRawWindowHandle* const current_window_handle;
    void* const node_hierarchy;
    AzSystemCallbacks* const system_callbacks;
    void* restrict datasets;
    bool * restrict stop_propagation;
    void* restrict focus_target;
    void* const words_cache;
    void* const shaped_words_cache;
    void* const positioned_words_cache;
    void* const positioned_rects;
    void* restrict words_changed_in_callbacks;
    void* restrict images_changed_in_callbacks;
    void* restrict image_masks_changed_in_callbacks;
    void* restrict css_properties_changed_in_callbacks;
    void* const current_scroll_states;
    void* restrict nodes_scrolled_in_callback;
    AzDomNodeId hit_dom_node;
    AzOptionLayoutPoint cursor_relative_to_item;
    AzOptionLayoutPoint cursor_in_viewport;
} AzCallbackInfo;

typedef struct {
    AzDomId dom;
    AzCssPath css_path;
} AzFocusTargetPath;

typedef struct {
    AzCallbackInfo callback_info;
    AzInstant frame_start;
    size_t call_count;
    bool  is_about_to_finish;
} AzTimerCallbackInfo;

typedef enum {
   AzNodeDataInlineCssPropertyTag_Normal,
   AzNodeDataInlineCssPropertyTag_Active,
   AzNodeDataInlineCssPropertyTag_Focus,
   AzNodeDataInlineCssPropertyTag_Hover,
} AzNodeDataInlineCssPropertyTag;

typedef struct { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; } AzNodeDataInlineCssPropertyVariant_Normal;
typedef struct { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; } AzNodeDataInlineCssPropertyVariant_Active;
typedef struct { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; } AzNodeDataInlineCssPropertyVariant_Focus;
typedef struct { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; } AzNodeDataInlineCssPropertyVariant_Hover;

typedef union {
    AzNodeDataInlineCssPropertyVariant_Normal Normal;
    AzNodeDataInlineCssPropertyVariant_Active Active;
    AzNodeDataInlineCssPropertyVariant_Focus Focus;
    AzNodeDataInlineCssPropertyVariant_Hover Hover;
} AzNodeDataInlineCssProperty;

typedef struct {
    AzString dynamic_id;
    AzCssProperty default_value;
} AzDynamicCssProperty;

typedef enum {
   AzSvgNodeTag_MultiPolygonCollection,
   AzSvgNodeTag_MultiPolygon,
   AzSvgNodeTag_Path,
   AzSvgNodeTag_Circle,
   AzSvgNodeTag_Rect,
} AzSvgNodeTag;

typedef struct { AzSvgNodeTag tag; AzSvgMultiPolygonVec payload; } AzSvgNodeVariant_MultiPolygonCollection;
typedef struct { AzSvgNodeTag tag; AzSvgMultiPolygon payload; } AzSvgNodeVariant_MultiPolygon;
typedef struct { AzSvgNodeTag tag; AzSvgPath payload; } AzSvgNodeVariant_Path;
typedef struct { AzSvgNodeTag tag; AzSvgCircle payload; } AzSvgNodeVariant_Circle;
typedef struct { AzSvgNodeTag tag; AzSvgRect payload; } AzSvgNodeVariant_Rect;

typedef union {
    AzSvgNodeVariant_MultiPolygonCollection MultiPolygonCollection;
    AzSvgNodeVariant_MultiPolygon MultiPolygon;
    AzSvgNodeVariant_Path Path;
    AzSvgNodeVariant_Circle Circle;
    AzSvgNodeVariant_Rect Rect;
} AzSvgNode;

typedef struct {
    AzSvgNode geometry;
    AzSvgStyle style;
} AzSvgStyledNode;

typedef struct {
    AzNodeDataInlineCssProperty* const ptr;
    size_t len;
    size_t cap;
    AzNodeDataInlineCssPropertyVecDestructor destructor;
} AzNodeDataInlineCssPropertyVec;

typedef enum {
   AzXmlParseErrorTag_InvalidDeclaration,
   AzXmlParseErrorTag_InvalidComment,
   AzXmlParseErrorTag_InvalidPI,
   AzXmlParseErrorTag_InvalidDoctype,
   AzXmlParseErrorTag_InvalidEntity,
   AzXmlParseErrorTag_InvalidElement,
   AzXmlParseErrorTag_InvalidAttribute,
   AzXmlParseErrorTag_InvalidCdata,
   AzXmlParseErrorTag_InvalidCharData,
   AzXmlParseErrorTag_UnknownToken,
} AzXmlParseErrorTag;

typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidDeclaration;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidComment;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidPI;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidDoctype;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidEntity;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidElement;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidAttribute;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidCdata;
typedef struct { AzXmlParseErrorTag tag; AzXmlTextError payload; } AzXmlParseErrorVariant_InvalidCharData;
typedef struct { AzXmlParseErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlParseErrorVariant_UnknownToken;

typedef union {
    AzXmlParseErrorVariant_InvalidDeclaration InvalidDeclaration;
    AzXmlParseErrorVariant_InvalidComment InvalidComment;
    AzXmlParseErrorVariant_InvalidPI InvalidPI;
    AzXmlParseErrorVariant_InvalidDoctype InvalidDoctype;
    AzXmlParseErrorVariant_InvalidEntity InvalidEntity;
    AzXmlParseErrorVariant_InvalidElement InvalidElement;
    AzXmlParseErrorVariant_InvalidAttribute InvalidAttribute;
    AzXmlParseErrorVariant_InvalidCdata InvalidCdata;
    AzXmlParseErrorVariant_InvalidCharData InvalidCharData;
    AzXmlParseErrorVariant_UnknownToken UnknownToken;
} AzXmlParseError;

typedef struct {
    AzWindowState state;
    AzOptionRendererOptions renderer_type;
    AzOptionWindowTheme theme;
    AzOptionCallback create_callback;
} AzWindowCreateOptions;

typedef enum {
   AzFocusTargetTag_Id,
   AzFocusTargetTag_Path,
   AzFocusTargetTag_Previous,
   AzFocusTargetTag_Next,
   AzFocusTargetTag_First,
   AzFocusTargetTag_Last,
   AzFocusTargetTag_NoFocus,
} AzFocusTargetTag;

typedef struct { AzFocusTargetTag tag; AzDomNodeId payload; } AzFocusTargetVariant_Id;
typedef struct { AzFocusTargetTag tag; AzFocusTargetPath payload; } AzFocusTargetVariant_Path;
typedef struct { AzFocusTargetTag tag; } AzFocusTargetVariant_Previous;
typedef struct { AzFocusTargetTag tag; } AzFocusTargetVariant_Next;
typedef struct { AzFocusTargetTag tag; } AzFocusTargetVariant_First;
typedef struct { AzFocusTargetTag tag; } AzFocusTargetVariant_Last;
typedef struct { AzFocusTargetTag tag; } AzFocusTargetVariant_NoFocus;

typedef union {
    AzFocusTargetVariant_Id Id;
    AzFocusTargetVariant_Path Path;
    AzFocusTargetVariant_Previous Previous;
    AzFocusTargetVariant_Next Next;
    AzFocusTargetVariant_First First;
    AzFocusTargetVariant_Last Last;
    AzFocusTargetVariant_NoFocus NoFocus;
} AzFocusTarget;

typedef struct {
    AzNodeType node_type;
    AzOptionRefAny dataset;
    AzIdOrClassVec ids_and_classes;
    AzCallbackDataVec callbacks;
    AzNodeDataInlineCssPropertyVec inline_css_props;
    AzOptionImageMask clip_mask;
    AzOptionTabIndex tab_index;
} AzNodeData;

typedef enum {
   AzCssDeclarationTag_Static,
   AzCssDeclarationTag_Dynamic,
} AzCssDeclarationTag;

typedef struct { AzCssDeclarationTag tag; AzCssProperty payload; } AzCssDeclarationVariant_Static;
typedef struct { AzCssDeclarationTag tag; AzDynamicCssProperty payload; } AzCssDeclarationVariant_Dynamic;

typedef union {
    AzCssDeclarationVariant_Static Static;
    AzCssDeclarationVariant_Dynamic Dynamic;
} AzCssDeclaration;

typedef struct {
    AzCssDeclaration* const ptr;
    size_t len;
    size_t cap;
    AzCssDeclarationVecDestructor destructor;
} AzCssDeclarationVec;

typedef struct {
    AzNodeData* const ptr;
    size_t len;
    size_t cap;
    AzNodeDataVecDestructor destructor;
} AzNodeDataVec;

typedef enum {
   AzXmlErrorTag_InvalidXmlPrefixUri,
   AzXmlErrorTag_UnexpectedXmlUri,
   AzXmlErrorTag_UnexpectedXmlnsUri,
   AzXmlErrorTag_InvalidElementNamePrefix,
   AzXmlErrorTag_DuplicatedNamespace,
   AzXmlErrorTag_UnknownNamespace,
   AzXmlErrorTag_UnexpectedCloseTag,
   AzXmlErrorTag_UnexpectedEntityCloseTag,
   AzXmlErrorTag_UnknownEntityReference,
   AzXmlErrorTag_MalformedEntityReference,
   AzXmlErrorTag_EntityReferenceLoop,
   AzXmlErrorTag_InvalidAttributeValue,
   AzXmlErrorTag_DuplicatedAttribute,
   AzXmlErrorTag_NoRootNode,
   AzXmlErrorTag_SizeLimit,
   AzXmlErrorTag_ParserError,
} AzXmlErrorTag;

typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_InvalidXmlPrefixUri;
typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_UnexpectedXmlUri;
typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_UnexpectedXmlnsUri;
typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_InvalidElementNamePrefix;
typedef struct { AzXmlErrorTag tag; AzDuplicatedNamespaceError payload; } AzXmlErrorVariant_DuplicatedNamespace;
typedef struct { AzXmlErrorTag tag; AzUnknownNamespaceError payload; } AzXmlErrorVariant_UnknownNamespace;
typedef struct { AzXmlErrorTag tag; AzUnexpectedCloseTagError payload; } AzXmlErrorVariant_UnexpectedCloseTag;
typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_UnexpectedEntityCloseTag;
typedef struct { AzXmlErrorTag tag; AzUnknownEntityReferenceError payload; } AzXmlErrorVariant_UnknownEntityReference;
typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_MalformedEntityReference;
typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_EntityReferenceLoop;
typedef struct { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; } AzXmlErrorVariant_InvalidAttributeValue;
typedef struct { AzXmlErrorTag tag; AzDuplicatedAttributeError payload; } AzXmlErrorVariant_DuplicatedAttribute;
typedef struct { AzXmlErrorTag tag; } AzXmlErrorVariant_NoRootNode;
typedef struct { AzXmlErrorTag tag; } AzXmlErrorVariant_SizeLimit;
typedef struct { AzXmlErrorTag tag; AzXmlParseError payload; } AzXmlErrorVariant_ParserError;

typedef union {
    AzXmlErrorVariant_InvalidXmlPrefixUri InvalidXmlPrefixUri;
    AzXmlErrorVariant_UnexpectedXmlUri UnexpectedXmlUri;
    AzXmlErrorVariant_UnexpectedXmlnsUri UnexpectedXmlnsUri;
    AzXmlErrorVariant_InvalidElementNamePrefix InvalidElementNamePrefix;
    AzXmlErrorVariant_DuplicatedNamespace DuplicatedNamespace;
    AzXmlErrorVariant_UnknownNamespace UnknownNamespace;
    AzXmlErrorVariant_UnexpectedCloseTag UnexpectedCloseTag;
    AzXmlErrorVariant_UnexpectedEntityCloseTag UnexpectedEntityCloseTag;
    AzXmlErrorVariant_UnknownEntityReference UnknownEntityReference;
    AzXmlErrorVariant_MalformedEntityReference MalformedEntityReference;
    AzXmlErrorVariant_EntityReferenceLoop EntityReferenceLoop;
    AzXmlErrorVariant_InvalidAttributeValue InvalidAttributeValue;
    AzXmlErrorVariant_DuplicatedAttribute DuplicatedAttribute;
    AzXmlErrorVariant_NoRootNode NoRootNode;
    AzXmlErrorVariant_SizeLimit SizeLimit;
    AzXmlErrorVariant_ParserError ParserError;
} AzXmlError;

typedef struct {
    AzNodeData root;
    AzDomVec children;
    size_t estimated_total_children;
} AzDom;

typedef struct {
    AzCssPath path;
    AzCssDeclarationVec declarations;
} AzCssRuleBlock;

typedef struct {
    AzNodeId root;
    AzNodeVec node_hierarchy;
    AzNodeDataVec node_data;
    AzStyledNodeVec styled_nodes;
    AzCascadeInfoVec cascade_info;
    AzTagIdsToNodeIdsMappingVec tag_ids_to_node_ids;
    AzParentWithNodeDepthVec non_leaf_nodes;
    AzCssPropertyCache css_property_cache;
} AzStyledDom;

typedef struct {
    AzDom* const ptr;
    size_t len;
    size_t cap;
    AzDomVecDestructor destructor;
} AzDomVec;

typedef struct {
    AzCssRuleBlock* const ptr;
    size_t len;
    size_t cap;
    AzCssRuleBlockVecDestructor destructor;
} AzCssRuleBlockVec;

typedef enum {
   AzOptionDomTag_None,
   AzOptionDomTag_Some,
} AzOptionDomTag;

typedef struct { AzOptionDomTag tag; } AzOptionDomVariant_None;
typedef struct { AzOptionDomTag tag; AzDom payload; } AzOptionDomVariant_Some;

typedef union {
    AzOptionDomVariant_None None;
    AzOptionDomVariant_Some Some;
} AzOptionDom;

typedef enum {
   AzSvgParseErrorTag_InvalidFileSuffix,
   AzSvgParseErrorTag_FileOpenFailed,
   AzSvgParseErrorTag_NotAnUtf8Str,
   AzSvgParseErrorTag_MalformedGZip,
   AzSvgParseErrorTag_InvalidSize,
   AzSvgParseErrorTag_ParsingFailed,
} AzSvgParseErrorTag;

typedef struct { AzSvgParseErrorTag tag; } AzSvgParseErrorVariant_InvalidFileSuffix;
typedef struct { AzSvgParseErrorTag tag; } AzSvgParseErrorVariant_FileOpenFailed;
typedef struct { AzSvgParseErrorTag tag; } AzSvgParseErrorVariant_NotAnUtf8Str;
typedef struct { AzSvgParseErrorTag tag; } AzSvgParseErrorVariant_MalformedGZip;
typedef struct { AzSvgParseErrorTag tag; } AzSvgParseErrorVariant_InvalidSize;
typedef struct { AzSvgParseErrorTag tag; AzXmlError payload; } AzSvgParseErrorVariant_ParsingFailed;

typedef union {
    AzSvgParseErrorVariant_InvalidFileSuffix InvalidFileSuffix;
    AzSvgParseErrorVariant_FileOpenFailed FileOpenFailed;
    AzSvgParseErrorVariant_NotAnUtf8Str NotAnUtf8Str;
    AzSvgParseErrorVariant_MalformedGZip MalformedGZip;
    AzSvgParseErrorVariant_InvalidSize InvalidSize;
    AzSvgParseErrorVariant_ParsingFailed ParsingFailed;
} AzSvgParseError;

typedef struct {
    AzStyledDom dom;
    AzLayoutRect size;
    AzOptionLayoutRect virtual_size;
} AzIFrameCallbackReturn;

typedef struct {
    AzCssRuleBlockVec rules;
} AzStylesheet;

typedef struct {
    AzStylesheet* const ptr;
    size_t len;
    size_t cap;
    AzStylesheetVecDestructor destructor;
} AzStylesheetVec;

typedef enum {
   AzResultSvgSvgParseErrorTag_Ok,
   AzResultSvgSvgParseErrorTag_Err,
} AzResultSvgSvgParseErrorTag;

typedef struct { AzResultSvgSvgParseErrorTag tag; AzSvg payload; } AzResultSvgSvgParseErrorVariant_Ok;
typedef struct { AzResultSvgSvgParseErrorTag tag; AzSvgParseError payload; } AzResultSvgSvgParseErrorVariant_Err;

typedef union {
    AzResultSvgSvgParseErrorVariant_Ok Ok;
    AzResultSvgSvgParseErrorVariant_Err Err;
} AzResultSvgSvgParseError;

typedef struct {
    AzStylesheetVec stylesheets;
} AzCss;


#undef ssize_t

#endif // AZUL_H
