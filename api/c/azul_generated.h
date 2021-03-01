#ifndef AZUL_H
#define AZUL_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

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
   AzAcceleratorKey_Ctrl,
   AzAcceleratorKey_Alt,
   AzAcceleratorKey_Shift,
        Key(AzVirtualKeyCode),
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
   AzCursorPosition_OutOfWindow,
   AzCursorPosition_Uninitialized,
        InWindow(AzLogicalPosition),
} AzCursorPosition;

typedef struct {
    uint8_t _reserved;
} AzMacWindowOptions;

typedef struct {
    uint8_t _reserved;
} AzWasmWindowOptions;

typedef enum {
   AzWindowPosition_Uninitialized,
        Initialized(AzPhysicalPositionI32),
} AzWindowPosition;

typedef enum {
   AzImePosition_Uninitialized,
        Initialized(AzLogicalPosition),
} AzImePosition;

typedef struct {
    uint8_t unused;
} AzTouchState;

typedef struct {
    AzLayoutSize size;
    uint16_t bit_depth;
    uint16_t refresh_rate;
} AzVideoMode;
typedef AzLayoutCallbackType;
typedef struct {
    AzLayoutCallbackType cb;
} AzLayoutCallback;
typedef AzCallbackType;
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
typedef AzIFrameCallbackType;
typedef struct {
    AzIFrameCallbackType cb;
} AzIFrameCallback;

typedef struct {
    void* const resources;
    AzHidpiAdjustedBounds bounds;
} AzIFrameCallbackInfo;
typedef AzGlCallbackType;
typedef struct {
    AzGlCallbackType cb;
} AzGlCallback;
typedef AzTimerCallbackType;
typedef struct {
    AzTimerCallbackType cb;
} AzTimerCallback;

typedef struct {
    AzUpdateScreen should_update;
    AzTerminateTimer should_terminate;
} AzTimerCallbackReturn;
typedef AzWriteBackCallbackType;
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
        Hover(AzHoverEventFilter),
        Focus(AzFocusEventFilter),
} AzNotEventFilter;

typedef enum {
   AzTabIndex_Auto,
    OverrideInParent(u32),
   AzTabIndex_NoKeyboardFocus,
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
    AzPixelValueNoPercent[ ;2]offset;
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
        Angle(AzAngleValue),
        FromTo(AzDirectionCorners),
} AzDirection;

typedef enum {
   AzBackgroundPositionHorizontal_Left,
   AzBackgroundPositionHorizontal_Center,
   AzBackgroundPositionHorizontal_Right,
        Exact(AzPixelValue),
} AzBackgroundPositionHorizontal;

typedef enum {
   AzBackgroundPositionVertical_Top,
   AzBackgroundPositionVertical_Center,
   AzBackgroundPositionVertical_Bottom,
        Exact(AzPixelValue),
} AzBackgroundPositionVertical;

typedef struct {
    AzBackgroundPositionHorizontal horizontal;
    AzBackgroundPositionVertical vertical;
} AzStyleBackgroundPosition;

typedef enum {
        ExactSize([AzPixelValue;2]),
   AzStyleBackgroundSize_Contain,
   AzStyleBackgroundSize_Cover,
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
   AzStyleBoxShadowValue_Auto,
   AzStyleBoxShadowValue_None,
   AzStyleBoxShadowValue_Inherit,
   AzStyleBoxShadowValue_Initial,
        Exact(AzStyleBoxShadow),
} AzStyleBoxShadowValue;

typedef enum {
   AzLayoutAlignContentValue_Auto,
   AzLayoutAlignContentValue_None,
   AzLayoutAlignContentValue_Inherit,
   AzLayoutAlignContentValue_Initial,
        Exact(AzLayoutAlignContent),
} AzLayoutAlignContentValue;

typedef enum {
   AzLayoutAlignItemsValue_Auto,
   AzLayoutAlignItemsValue_None,
   AzLayoutAlignItemsValue_Inherit,
   AzLayoutAlignItemsValue_Initial,
        Exact(AzLayoutAlignItems),
} AzLayoutAlignItemsValue;

typedef enum {
   AzLayoutBottomValue_Auto,
   AzLayoutBottomValue_None,
   AzLayoutBottomValue_Inherit,
   AzLayoutBottomValue_Initial,
        Exact(AzLayoutBottom),
} AzLayoutBottomValue;

typedef enum {
   AzLayoutBoxSizingValue_Auto,
   AzLayoutBoxSizingValue_None,
   AzLayoutBoxSizingValue_Inherit,
   AzLayoutBoxSizingValue_Initial,
        Exact(AzLayoutBoxSizing),
} AzLayoutBoxSizingValue;

typedef enum {
   AzLayoutFlexDirectionValue_Auto,
   AzLayoutFlexDirectionValue_None,
   AzLayoutFlexDirectionValue_Inherit,
   AzLayoutFlexDirectionValue_Initial,
        Exact(AzLayoutFlexDirection),
} AzLayoutFlexDirectionValue;

typedef enum {
   AzLayoutDisplayValue_Auto,
   AzLayoutDisplayValue_None,
   AzLayoutDisplayValue_Inherit,
   AzLayoutDisplayValue_Initial,
        Exact(AzLayoutDisplay),
} AzLayoutDisplayValue;

typedef enum {
   AzLayoutFlexGrowValue_Auto,
   AzLayoutFlexGrowValue_None,
   AzLayoutFlexGrowValue_Inherit,
   AzLayoutFlexGrowValue_Initial,
        Exact(AzLayoutFlexGrow),
} AzLayoutFlexGrowValue;

typedef enum {
   AzLayoutFlexShrinkValue_Auto,
   AzLayoutFlexShrinkValue_None,
   AzLayoutFlexShrinkValue_Inherit,
   AzLayoutFlexShrinkValue_Initial,
        Exact(AzLayoutFlexShrink),
} AzLayoutFlexShrinkValue;

typedef enum {
   AzLayoutFloatValue_Auto,
   AzLayoutFloatValue_None,
   AzLayoutFloatValue_Inherit,
   AzLayoutFloatValue_Initial,
        Exact(AzLayoutFloat),
} AzLayoutFloatValue;

typedef enum {
   AzLayoutHeightValue_Auto,
   AzLayoutHeightValue_None,
   AzLayoutHeightValue_Inherit,
   AzLayoutHeightValue_Initial,
        Exact(AzLayoutHeight),
} AzLayoutHeightValue;

typedef enum {
   AzLayoutJustifyContentValue_Auto,
   AzLayoutJustifyContentValue_None,
   AzLayoutJustifyContentValue_Inherit,
   AzLayoutJustifyContentValue_Initial,
        Exact(AzLayoutJustifyContent),
} AzLayoutJustifyContentValue;

typedef enum {
   AzLayoutLeftValue_Auto,
   AzLayoutLeftValue_None,
   AzLayoutLeftValue_Inherit,
   AzLayoutLeftValue_Initial,
        Exact(AzLayoutLeft),
} AzLayoutLeftValue;

typedef enum {
   AzLayoutMarginBottomValue_Auto,
   AzLayoutMarginBottomValue_None,
   AzLayoutMarginBottomValue_Inherit,
   AzLayoutMarginBottomValue_Initial,
        Exact(AzLayoutMarginBottom),
} AzLayoutMarginBottomValue;

typedef enum {
   AzLayoutMarginLeftValue_Auto,
   AzLayoutMarginLeftValue_None,
   AzLayoutMarginLeftValue_Inherit,
   AzLayoutMarginLeftValue_Initial,
        Exact(AzLayoutMarginLeft),
} AzLayoutMarginLeftValue;

typedef enum {
   AzLayoutMarginRightValue_Auto,
   AzLayoutMarginRightValue_None,
   AzLayoutMarginRightValue_Inherit,
   AzLayoutMarginRightValue_Initial,
        Exact(AzLayoutMarginRight),
} AzLayoutMarginRightValue;

typedef enum {
   AzLayoutMarginTopValue_Auto,
   AzLayoutMarginTopValue_None,
   AzLayoutMarginTopValue_Inherit,
   AzLayoutMarginTopValue_Initial,
        Exact(AzLayoutMarginTop),
} AzLayoutMarginTopValue;

typedef enum {
   AzLayoutMaxHeightValue_Auto,
   AzLayoutMaxHeightValue_None,
   AzLayoutMaxHeightValue_Inherit,
   AzLayoutMaxHeightValue_Initial,
        Exact(AzLayoutMaxHeight),
} AzLayoutMaxHeightValue;

typedef enum {
   AzLayoutMaxWidthValue_Auto,
   AzLayoutMaxWidthValue_None,
   AzLayoutMaxWidthValue_Inherit,
   AzLayoutMaxWidthValue_Initial,
        Exact(AzLayoutMaxWidth),
} AzLayoutMaxWidthValue;

typedef enum {
   AzLayoutMinHeightValue_Auto,
   AzLayoutMinHeightValue_None,
   AzLayoutMinHeightValue_Inherit,
   AzLayoutMinHeightValue_Initial,
        Exact(AzLayoutMinHeight),
} AzLayoutMinHeightValue;

typedef enum {
   AzLayoutMinWidthValue_Auto,
   AzLayoutMinWidthValue_None,
   AzLayoutMinWidthValue_Inherit,
   AzLayoutMinWidthValue_Initial,
        Exact(AzLayoutMinWidth),
} AzLayoutMinWidthValue;

typedef enum {
   AzLayoutPaddingBottomValue_Auto,
   AzLayoutPaddingBottomValue_None,
   AzLayoutPaddingBottomValue_Inherit,
   AzLayoutPaddingBottomValue_Initial,
        Exact(AzLayoutPaddingBottom),
} AzLayoutPaddingBottomValue;

typedef enum {
   AzLayoutPaddingLeftValue_Auto,
   AzLayoutPaddingLeftValue_None,
   AzLayoutPaddingLeftValue_Inherit,
   AzLayoutPaddingLeftValue_Initial,
        Exact(AzLayoutPaddingLeft),
} AzLayoutPaddingLeftValue;

typedef enum {
   AzLayoutPaddingRightValue_Auto,
   AzLayoutPaddingRightValue_None,
   AzLayoutPaddingRightValue_Inherit,
   AzLayoutPaddingRightValue_Initial,
        Exact(AzLayoutPaddingRight),
} AzLayoutPaddingRightValue;

typedef enum {
   AzLayoutPaddingTopValue_Auto,
   AzLayoutPaddingTopValue_None,
   AzLayoutPaddingTopValue_Inherit,
   AzLayoutPaddingTopValue_Initial,
        Exact(AzLayoutPaddingTop),
} AzLayoutPaddingTopValue;

typedef enum {
   AzLayoutPositionValue_Auto,
   AzLayoutPositionValue_None,
   AzLayoutPositionValue_Inherit,
   AzLayoutPositionValue_Initial,
        Exact(AzLayoutPosition),
} AzLayoutPositionValue;

typedef enum {
   AzLayoutRightValue_Auto,
   AzLayoutRightValue_None,
   AzLayoutRightValue_Inherit,
   AzLayoutRightValue_Initial,
        Exact(AzLayoutRight),
} AzLayoutRightValue;

typedef enum {
   AzLayoutTopValue_Auto,
   AzLayoutTopValue_None,
   AzLayoutTopValue_Inherit,
   AzLayoutTopValue_Initial,
        Exact(AzLayoutTop),
} AzLayoutTopValue;

typedef enum {
   AzLayoutWidthValue_Auto,
   AzLayoutWidthValue_None,
   AzLayoutWidthValue_Inherit,
   AzLayoutWidthValue_Initial,
        Exact(AzLayoutWidth),
} AzLayoutWidthValue;

typedef enum {
   AzLayoutFlexWrapValue_Auto,
   AzLayoutFlexWrapValue_None,
   AzLayoutFlexWrapValue_Inherit,
   AzLayoutFlexWrapValue_Initial,
        Exact(AzLayoutFlexWrap),
} AzLayoutFlexWrapValue;

typedef enum {
   AzLayoutOverflowValue_Auto,
   AzLayoutOverflowValue_None,
   AzLayoutOverflowValue_Inherit,
   AzLayoutOverflowValue_Initial,
        Exact(AzLayoutOverflow),
} AzLayoutOverflowValue;

typedef enum {
   AzStyleBorderBottomColorValue_Auto,
   AzStyleBorderBottomColorValue_None,
   AzStyleBorderBottomColorValue_Inherit,
   AzStyleBorderBottomColorValue_Initial,
        Exact(AzStyleBorderBottomColor),
} AzStyleBorderBottomColorValue;

typedef enum {
   AzStyleBorderBottomLeftRadiusValue_Auto,
   AzStyleBorderBottomLeftRadiusValue_None,
   AzStyleBorderBottomLeftRadiusValue_Inherit,
   AzStyleBorderBottomLeftRadiusValue_Initial,
        Exact(AzStyleBorderBottomLeftRadius),
} AzStyleBorderBottomLeftRadiusValue;

typedef enum {
   AzStyleBorderBottomRightRadiusValue_Auto,
   AzStyleBorderBottomRightRadiusValue_None,
   AzStyleBorderBottomRightRadiusValue_Inherit,
   AzStyleBorderBottomRightRadiusValue_Initial,
        Exact(AzStyleBorderBottomRightRadius),
} AzStyleBorderBottomRightRadiusValue;

typedef enum {
   AzStyleBorderBottomStyleValue_Auto,
   AzStyleBorderBottomStyleValue_None,
   AzStyleBorderBottomStyleValue_Inherit,
   AzStyleBorderBottomStyleValue_Initial,
        Exact(AzStyleBorderBottomStyle),
} AzStyleBorderBottomStyleValue;

typedef enum {
   AzLayoutBorderBottomWidthValue_Auto,
   AzLayoutBorderBottomWidthValue_None,
   AzLayoutBorderBottomWidthValue_Inherit,
   AzLayoutBorderBottomWidthValue_Initial,
        Exact(AzLayoutBorderBottomWidth),
} AzLayoutBorderBottomWidthValue;

typedef enum {
   AzStyleBorderLeftColorValue_Auto,
   AzStyleBorderLeftColorValue_None,
   AzStyleBorderLeftColorValue_Inherit,
   AzStyleBorderLeftColorValue_Initial,
        Exact(AzStyleBorderLeftColor),
} AzStyleBorderLeftColorValue;

typedef enum {
   AzStyleBorderLeftStyleValue_Auto,
   AzStyleBorderLeftStyleValue_None,
   AzStyleBorderLeftStyleValue_Inherit,
   AzStyleBorderLeftStyleValue_Initial,
        Exact(AzStyleBorderLeftStyle),
} AzStyleBorderLeftStyleValue;

typedef enum {
   AzLayoutBorderLeftWidthValue_Auto,
   AzLayoutBorderLeftWidthValue_None,
   AzLayoutBorderLeftWidthValue_Inherit,
   AzLayoutBorderLeftWidthValue_Initial,
        Exact(AzLayoutBorderLeftWidth),
} AzLayoutBorderLeftWidthValue;

typedef enum {
   AzStyleBorderRightColorValue_Auto,
   AzStyleBorderRightColorValue_None,
   AzStyleBorderRightColorValue_Inherit,
   AzStyleBorderRightColorValue_Initial,
        Exact(AzStyleBorderRightColor),
} AzStyleBorderRightColorValue;

typedef enum {
   AzStyleBorderRightStyleValue_Auto,
   AzStyleBorderRightStyleValue_None,
   AzStyleBorderRightStyleValue_Inherit,
   AzStyleBorderRightStyleValue_Initial,
        Exact(AzStyleBorderRightStyle),
} AzStyleBorderRightStyleValue;

typedef enum {
   AzLayoutBorderRightWidthValue_Auto,
   AzLayoutBorderRightWidthValue_None,
   AzLayoutBorderRightWidthValue_Inherit,
   AzLayoutBorderRightWidthValue_Initial,
        Exact(AzLayoutBorderRightWidth),
} AzLayoutBorderRightWidthValue;

typedef enum {
   AzStyleBorderTopColorValue_Auto,
   AzStyleBorderTopColorValue_None,
   AzStyleBorderTopColorValue_Inherit,
   AzStyleBorderTopColorValue_Initial,
        Exact(AzStyleBorderTopColor),
} AzStyleBorderTopColorValue;

typedef enum {
   AzStyleBorderTopLeftRadiusValue_Auto,
   AzStyleBorderTopLeftRadiusValue_None,
   AzStyleBorderTopLeftRadiusValue_Inherit,
   AzStyleBorderTopLeftRadiusValue_Initial,
        Exact(AzStyleBorderTopLeftRadius),
} AzStyleBorderTopLeftRadiusValue;

typedef enum {
   AzStyleBorderTopRightRadiusValue_Auto,
   AzStyleBorderTopRightRadiusValue_None,
   AzStyleBorderTopRightRadiusValue_Inherit,
   AzStyleBorderTopRightRadiusValue_Initial,
        Exact(AzStyleBorderTopRightRadius),
} AzStyleBorderTopRightRadiusValue;

typedef enum {
   AzStyleBorderTopStyleValue_Auto,
   AzStyleBorderTopStyleValue_None,
   AzStyleBorderTopStyleValue_Inherit,
   AzStyleBorderTopStyleValue_Initial,
        Exact(AzStyleBorderTopStyle),
} AzStyleBorderTopStyleValue;

typedef enum {
   AzLayoutBorderTopWidthValue_Auto,
   AzLayoutBorderTopWidthValue_None,
   AzLayoutBorderTopWidthValue_Inherit,
   AzLayoutBorderTopWidthValue_Initial,
        Exact(AzLayoutBorderTopWidth),
} AzLayoutBorderTopWidthValue;

typedef enum {
   AzStyleCursorValue_Auto,
   AzStyleCursorValue_None,
   AzStyleCursorValue_Inherit,
   AzStyleCursorValue_Initial,
        Exact(AzStyleCursor),
} AzStyleCursorValue;

typedef enum {
   AzStyleFontSizeValue_Auto,
   AzStyleFontSizeValue_None,
   AzStyleFontSizeValue_Inherit,
   AzStyleFontSizeValue_Initial,
        Exact(AzStyleFontSize),
} AzStyleFontSizeValue;

typedef enum {
   AzStyleLetterSpacingValue_Auto,
   AzStyleLetterSpacingValue_None,
   AzStyleLetterSpacingValue_Inherit,
   AzStyleLetterSpacingValue_Initial,
        Exact(AzStyleLetterSpacing),
} AzStyleLetterSpacingValue;

typedef enum {
   AzStyleLineHeightValue_Auto,
   AzStyleLineHeightValue_None,
   AzStyleLineHeightValue_Inherit,
   AzStyleLineHeightValue_Initial,
        Exact(AzStyleLineHeight),
} AzStyleLineHeightValue;

typedef enum {
   AzStyleTabWidthValue_Auto,
   AzStyleTabWidthValue_None,
   AzStyleTabWidthValue_Inherit,
   AzStyleTabWidthValue_Initial,
        Exact(AzStyleTabWidth),
} AzStyleTabWidthValue;

typedef enum {
   AzStyleTextAlignmentHorzValue_Auto,
   AzStyleTextAlignmentHorzValue_None,
   AzStyleTextAlignmentHorzValue_Inherit,
   AzStyleTextAlignmentHorzValue_Initial,
        Exact(AzStyleTextAlignmentHorz),
} AzStyleTextAlignmentHorzValue;

typedef enum {
   AzStyleTextColorValue_Auto,
   AzStyleTextColorValue_None,
   AzStyleTextColorValue_Inherit,
   AzStyleTextColorValue_Initial,
        Exact(AzStyleTextColor),
} AzStyleTextColorValue;

typedef enum {
   AzStyleWordSpacingValue_Auto,
   AzStyleWordSpacingValue_None,
   AzStyleWordSpacingValue_Inherit,
   AzStyleWordSpacingValue_Initial,
        Exact(AzStyleWordSpacing),
} AzStyleWordSpacingValue;

typedef enum {
   AzStyleOpacityValue_Auto,
   AzStyleOpacityValue_None,
   AzStyleOpacityValue_Inherit,
   AzStyleOpacityValue_Initial,
        Exact(AzStyleOpacity),
} AzStyleOpacityValue;

typedef enum {
   AzStyleTransformOriginValue_Auto,
   AzStyleTransformOriginValue_None,
   AzStyleTransformOriginValue_Inherit,
   AzStyleTransformOriginValue_Initial,
        Exact(AzStyleTransformOrigin),
} AzStyleTransformOriginValue;

typedef enum {
   AzStylePerspectiveOriginValue_Auto,
   AzStylePerspectiveOriginValue_None,
   AzStylePerspectiveOriginValue_Inherit,
   AzStylePerspectiveOriginValue_Initial,
        Exact(AzStylePerspectiveOrigin),
} AzStylePerspectiveOriginValue;

typedef enum {
   AzStyleBackfaceVisibilityValue_Auto,
   AzStyleBackfaceVisibilityValue_None,
   AzStyleBackfaceVisibilityValue_Inherit,
   AzStyleBackfaceVisibilityValue_Initial,
        Exact(AzStyleBackfaceVisibility),
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
   AzSvgFitTo_Original,
    Width(u32),
    Height(u32),
    Zoom(f32),
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
typedef AzCreateThreadFnType;
typedef struct {
    AzCreateThreadFnType cb;
} AzCreateThreadFn;
typedef AzGetSystemTimeFnType;
typedef struct {
    AzGetSystemTimeFnType cb;
} AzGetSystemTimeFn;
typedef AzCheckThreadFinishedFnType;
typedef struct {
    AzCheckThreadFinishedFnType cb;
} AzCheckThreadFinishedFn;
typedef AzLibrarySendThreadMsgFnType;
typedef struct {
    AzLibrarySendThreadMsgFnType cb;
} AzLibrarySendThreadMsgFn;
typedef AzLibraryReceiveThreadMsgFnType;
typedef struct {
    AzLibraryReceiveThreadMsgFnType cb;
} AzLibraryReceiveThreadMsgFn;
typedef AzThreadRecvFnType;
typedef struct {
    AzThreadRecvFnType cb;
} AzThreadRecvFn;
typedef AzThreadSendFnType;
typedef struct {
    AzThreadSendFnType cb;
} AzThreadSendFn;
typedef AzThreadDestructorFnType;
typedef struct {
    AzThreadDestructorFnType cb;
} AzThreadDestructorFn;
typedef AzThreadReceiverDestructorFnType;
typedef struct {
    AzThreadReceiverDestructorFnType cb;
} AzThreadReceiverDestructorFn;
typedef AzThreadSenderDestructorFnType;
typedef struct {
    AzThreadSenderDestructorFnType cb;
} AzThreadSenderDestructorFn;

typedef enum {
   AzMonitorVecDestructor_DefaultRust,
   AzMonitorVecDestructor_NoDestructor,
        External(AzMonitorVecDestructorType),
} AzMonitorVecDestructor;

typedef enum {
   AzVideoModeVecDestructor_DefaultRust,
   AzVideoModeVecDestructor_NoDestructor,
        External(AzVideoModeVecDestructorType),
} AzVideoModeVecDestructor;

typedef enum {
   AzDomVecDestructor_DefaultRust,
   AzDomVecDestructor_NoDestructor,
        External(AzDomVecDestructorType),
} AzDomVecDestructor;

typedef enum {
   AzIdOrClassVecDestructor_DefaultRust,
   AzIdOrClassVecDestructor_NoDestructor,
        External(AzIdOrClassVecDestructorType),
} AzIdOrClassVecDestructor;

typedef enum {
   AzNodeDataInlineCssPropertyVecDestructor_DefaultRust,
   AzNodeDataInlineCssPropertyVecDestructor_NoDestructor,
        External(AzNodeDataInlineCssPropertyVecDestructorType),
} AzNodeDataInlineCssPropertyVecDestructor;

typedef enum {
   AzStyleBackgroundContentVecDestructor_DefaultRust,
   AzStyleBackgroundContentVecDestructor_NoDestructor,
        External(AzStyleBackgroundContentVecDestructorType),
} AzStyleBackgroundContentVecDestructor;

typedef enum {
   AzStyleBackgroundPositionVecDestructor_DefaultRust,
   AzStyleBackgroundPositionVecDestructor_NoDestructor,
        External(AzStyleBackgroundPositionVecDestructorType),
} AzStyleBackgroundPositionVecDestructor;

typedef enum {
   AzStyleBackgroundRepeatVecDestructor_DefaultRust,
   AzStyleBackgroundRepeatVecDestructor_NoDestructor,
        External(AzStyleBackgroundRepeatVecDestructorType),
} AzStyleBackgroundRepeatVecDestructor;

typedef enum {
   AzStyleBackgroundSizeVecDestructor_DefaultRust,
   AzStyleBackgroundSizeVecDestructor_NoDestructor,
        External(AzStyleBackgroundSizeVecDestructorType),
} AzStyleBackgroundSizeVecDestructor;

typedef enum {
   AzStyleTransformVecDestructor_DefaultRust,
   AzStyleTransformVecDestructor_NoDestructor,
        External(AzStyleTransformVecDestructorType),
} AzStyleTransformVecDestructor;

typedef enum {
   AzCssPropertyVecDestructor_DefaultRust,
   AzCssPropertyVecDestructor_NoDestructor,
        External(AzCssPropertyVecDestructorType),
} AzCssPropertyVecDestructor;

typedef enum {
   AzSvgMultiPolygonVecDestructor_DefaultRust,
   AzSvgMultiPolygonVecDestructor_NoDestructor,
        External(AzSvgMultiPolygonVecDestructorType),
} AzSvgMultiPolygonVecDestructor;

typedef enum {
   AzSvgPathVecDestructor_DefaultRust,
   AzSvgPathVecDestructor_NoDestructor,
        External(AzSvgPathVecDestructorType),
} AzSvgPathVecDestructor;

typedef enum {
   AzVertexAttributeVecDestructor_DefaultRust,
   AzVertexAttributeVecDestructor_NoDestructor,
        External(AzVertexAttributeVecDestructorType),
} AzVertexAttributeVecDestructor;

typedef enum {
   AzSvgPathElementVecDestructor_DefaultRust,
   AzSvgPathElementVecDestructor_NoDestructor,
        External(AzSvgPathElementVecDestructorType),
} AzSvgPathElementVecDestructor;

typedef enum {
   AzSvgVertexVecDestructor_DefaultRust,
   AzSvgVertexVecDestructor_NoDestructor,
        External(AzSvgVertexVecDestructorType),
} AzSvgVertexVecDestructor;

typedef enum {
   AzU32VecDestructor_DefaultRust,
   AzU32VecDestructor_NoDestructor,
        External(AzU32VecDestructorType),
} AzU32VecDestructor;

typedef enum {
   AzXWindowTypeVecDestructor_DefaultRust,
   AzXWindowTypeVecDestructor_NoDestructor,
        External(AzXWindowTypeVecDestructorType),
} AzXWindowTypeVecDestructor;

typedef enum {
   AzVirtualKeyCodeVecDestructor_DefaultRust,
   AzVirtualKeyCodeVecDestructor_NoDestructor,
        External(AzVirtualKeyCodeVecDestructorType),
} AzVirtualKeyCodeVecDestructor;

typedef enum {
   AzCascadeInfoVecDestructor_DefaultRust,
   AzCascadeInfoVecDestructor_NoDestructor,
        External(AzCascadeInfoVecDestructorType),
} AzCascadeInfoVecDestructor;

typedef enum {
   AzScanCodeVecDestructor_DefaultRust,
   AzScanCodeVecDestructor_NoDestructor,
        External(AzScanCodeVecDestructorType),
} AzScanCodeVecDestructor;

typedef enum {
   AzCssDeclarationVecDestructor_DefaultRust,
   AzCssDeclarationVecDestructor_NoDestructor,
        External(AzCssDeclarationVecDestructorType),
} AzCssDeclarationVecDestructor;

typedef enum {
   AzCssPathSelectorVecDestructor_DefaultRust,
   AzCssPathSelectorVecDestructor_NoDestructor,
        External(AzCssPathSelectorVecDestructorType),
} AzCssPathSelectorVecDestructor;

typedef enum {
   AzStylesheetVecDestructor_DefaultRust,
   AzStylesheetVecDestructor_NoDestructor,
        External(AzStylesheetVecDestructorType),
} AzStylesheetVecDestructor;

typedef enum {
   AzCssRuleBlockVecDestructor_DefaultRust,
   AzCssRuleBlockVecDestructor_NoDestructor,
        External(AzCssRuleBlockVecDestructorType),
} AzCssRuleBlockVecDestructor;

typedef enum {
   AzU8VecDestructor_DefaultRust,
   AzU8VecDestructor_NoDestructor,
        External(AzU8VecDestructorType),
} AzU8VecDestructor;

typedef enum {
   AzCallbackDataVecDestructor_DefaultRust,
   AzCallbackDataVecDestructor_NoDestructor,
        External(AzCallbackDataVecDestructorType),
} AzCallbackDataVecDestructor;

typedef enum {
   AzDebugMessageVecDestructor_DefaultRust,
   AzDebugMessageVecDestructor_NoDestructor,
        External(AzDebugMessageVecDestructorType),
} AzDebugMessageVecDestructor;

typedef enum {
   AzGLuintVecDestructor_DefaultRust,
   AzGLuintVecDestructor_NoDestructor,
        External(AzGLuintVecDestructorType),
} AzGLuintVecDestructor;

typedef enum {
   AzGLintVecDestructor_DefaultRust,
   AzGLintVecDestructor_NoDestructor,
        External(AzGLintVecDestructorType),
} AzGLintVecDestructor;

typedef enum {
   AzStringVecDestructor_DefaultRust,
   AzStringVecDestructor_NoDestructor,
        External(AzStringVecDestructorType),
} AzStringVecDestructor;

typedef enum {
   AzStringPairVecDestructor_DefaultRust,
   AzStringPairVecDestructor_NoDestructor,
        External(AzStringPairVecDestructorType),
} AzStringPairVecDestructor;

typedef enum {
   AzLinearColorStopVecDestructor_DefaultRust,
   AzLinearColorStopVecDestructor_NoDestructor,
        External(AzLinearColorStopVecDestructorType),
} AzLinearColorStopVecDestructor;

typedef enum {
   AzRadialColorStopVecDestructor_DefaultRust,
   AzRadialColorStopVecDestructor_NoDestructor,
        External(AzRadialColorStopVecDestructorType),
} AzRadialColorStopVecDestructor;

typedef enum {
   AzNodeIdVecDestructor_DefaultRust,
   AzNodeIdVecDestructor_NoDestructor,
        External(AzNodeIdVecDestructorType),
} AzNodeIdVecDestructor;

typedef enum {
   AzNodeVecDestructor_DefaultRust,
   AzNodeVecDestructor_NoDestructor,
        External(AzNodeVecDestructorType),
} AzNodeVecDestructor;

typedef enum {
   AzStyledNodeVecDestructor_DefaultRust,
   AzStyledNodeVecDestructor_NoDestructor,
        External(AzStyledNodeVecDestructorType),
} AzStyledNodeVecDestructor;

typedef enum {
   AzTagIdsToNodeIdsMappingVecDestructor_DefaultRust,
   AzTagIdsToNodeIdsMappingVecDestructor_NoDestructor,
        External(AzTagIdsToNodeIdsMappingVecDestructorType),
} AzTagIdsToNodeIdsMappingVecDestructor;

typedef enum {
   AzParentWithNodeDepthVecDestructor_DefaultRust,
   AzParentWithNodeDepthVecDestructor_NoDestructor,
        External(AzParentWithNodeDepthVecDestructorType),
} AzParentWithNodeDepthVecDestructor;

typedef enum {
   AzNodeDataVecDestructor_DefaultRust,
   AzNodeDataVecDestructor_NoDestructor,
        External(AzNodeDataVecDestructorType),
} AzNodeDataVecDestructor;

typedef enum {
   AzOptionGlContextPtr_None,
        Some(AzGlContextPtr),
} AzOptionGlContextPtr;

typedef enum {
   AzOptionPercentageValue_None,
        Some(AzPercentageValue),
} AzOptionPercentageValue;

typedef enum {
   AzOptionAngleValue_None,
        Some(AzAngleValue),
} AzOptionAngleValue;

typedef enum {
   AzOptionRendererOptions_None,
        Some(AzRendererOptions),
} AzOptionRendererOptions;

typedef enum {
   AzOptionCallback_None,
        Some(AzCallback),
} AzOptionCallback;

typedef enum {
   AzOptionThreadSendMsg_None,
        Some(AzThreadSendMsg),
} AzOptionThreadSendMsg;

typedef enum {
   AzOptionLayoutRect_None,
        Some(AzLayoutRect),
} AzOptionLayoutRect;

typedef enum {
   AzOptionRefAny_None,
        Some(AzRefAny),
} AzOptionRefAny;

typedef enum {
   AzOptionLayoutPoint_None,
        Some(AzLayoutPoint),
} AzOptionLayoutPoint;

typedef enum {
   AzOptionWindowTheme_None,
        Some(AzWindowTheme),
} AzOptionWindowTheme;

typedef enum {
   AzOptionNodeId_None,
        Some(AzNodeId),
} AzOptionNodeId;

typedef enum {
   AzOptionDomNodeId_None,
        Some(AzDomNodeId),
} AzOptionDomNodeId;

typedef enum {
   AzOptionColorU_None,
        Some(AzColorU),
} AzOptionColorU;

typedef enum {
   AzOptionSvgDashPattern_None,
        Some(AzSvgDashPattern),
} AzOptionSvgDashPattern;

typedef enum {
   AzOptionHwndHandle_None,
    Some(*mut c_void),
} AzOptionHwndHandle;

typedef enum {
   AzOptionLogicalPosition_None,
        Some(AzLogicalPosition),
} AzOptionLogicalPosition;

typedef enum {
   AzOptionPhysicalPositionI32_None,
        Some(AzPhysicalPositionI32),
} AzOptionPhysicalPositionI32;

typedef enum {
   AzOptionX11Visual_None,
    Some(*const c_void),
} AzOptionX11Visual;

typedef enum {
   AzOptionI32_None,
    Some(i32),
} AzOptionI32;

typedef enum {
   AzOptionF32_None,
    Some(f32),
} AzOptionF32;

typedef enum {
   AzOptionMouseCursorType_None,
        Some(AzMouseCursorType),
} AzOptionMouseCursorType;

typedef enum {
   AzOptionLogicalSize_None,
        Some(AzLogicalSize),
} AzOptionLogicalSize;

typedef enum {
   AzOptionChar_None,
    Some(u32),
} AzOptionChar;

typedef enum {
   AzOptionVirtualKeyCode_None,
        Some(AzVirtualKeyCode),
} AzOptionVirtualKeyCode;

typedef enum {
   AzOptionTexture_None,
        Some(AzTexture),
} AzOptionTexture;

typedef enum {
   AzOptionTabIndex_None,
        Some(AzTabIndex),
} AzOptionTabIndex;

typedef enum {
   AzOptionTagId_None,
        Some(AzTagId),
} AzOptionTagId;

typedef enum {
   AzOptionUsize_None,
    Some(usize),
} AzOptionUsize;

typedef enum {
   AzOptionU8VecRef_None,
        Some(AzU8VecRef),
} AzOptionU8VecRef;

typedef struct {
    uint32_t row;
    uint32_t col;
} AzSvgParseErrorPosition;
typedef AzInstantPtrCloneFnType;
typedef struct {
    AzInstantPtrCloneFnType cb;
} AzInstantPtrCloneFn;
typedef AzInstantPtrDestructorFnType;
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
        IOS(AzIOSHandle),
        MacOS(AzMacOSHandle),
        Xlib(AzXlibHandle),
        Xcb(AzXcbHandle),
        Wayland(AzWaylandHandle),
        Windows(AzWindowsHandle),
        Web(AzWebHandle),
        Android(AzAndroidHandle),
   AzRawWindowHandle_Unsupported,
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
        Hover(AzHoverEventFilter),
        Not(AzNotEventFilter),
        Focus(AzFocusEventFilter),
        Window(AzWindowEventFilter),
        Component(AzComponentEventFilter),
        Application(AzApplicationEventFilter),
} AzEventFilter;

typedef enum {
    Number(u32),
   AzCssNthChildSelector_Even,
   AzCssNthChildSelector_Odd,
        Pattern(AzCssNthChildPattern),
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
        Matrix(AzStyleTransformMatrix2D),
        Matrix3D(AzStyleTransformMatrix3D),
        Translate(AzStyleTransformTranslate2D),
        Translate3D(AzStyleTransformTranslate3D),
        TranslateX(AzPixelValue),
        TranslateY(AzPixelValue),
        TranslateZ(AzPixelValue),
        Rotate(AzPercentageValue),
        Rotate3D(AzStyleTransformRotate3D),
        RotateX(AzPercentageValue),
        RotateY(AzPercentageValue),
        RotateZ(AzPercentageValue),
        Scale(AzStyleTransformScale2D),
        Scale3D(AzStyleTransformScale3D),
        ScaleX(AzPercentageValue),
        ScaleY(AzPercentageValue),
        ScaleZ(AzPercentageValue),
        Skew(AzStyleTransformSkew2D),
        SkewX(AzPercentageValue),
        SkewY(AzPercentageValue),
        Perspective(AzPixelValue),
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
        WriteBack(AzThreadWriteBackMsg),
        Update(AzUpdateScreen),
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
   AzOptionThreadReceiveMsg_None,
        Some(AzThreadReceiveMsg),
} AzOptionThreadReceiveMsg;

typedef enum {
   AzOptionImageMask_None,
        Some(AzImageMask),
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
        System(AzInstantPtr),
        Tick(AzSystemTick),
} AzInstant;

typedef enum {
        System(AzSystemTimeDiff),
        Tick(AzSystemTickDiff),
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
        Small(AzSmallWindowIconBytes),
        Large(AzLargeWindowIconBytes),
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
   AzCssPathPseudoSelector_First,
   AzCssPathPseudoSelector_Last,
        NthChild(AzCssNthChildSelector),
   AzCssPathPseudoSelector_Hover,
   AzCssPathPseudoSelector_Active,
   AzCssPathPseudoSelector_Focus,
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
   AzStyleBackgroundPositionVecValue_Auto,
   AzStyleBackgroundPositionVecValue_None,
   AzStyleBackgroundPositionVecValue_Inherit,
   AzStyleBackgroundPositionVecValue_Initial,
        Exact(AzStyleBackgroundPositionVec),
} AzStyleBackgroundPositionVecValue;

typedef enum {
   AzStyleBackgroundRepeatVecValue_Auto,
   AzStyleBackgroundRepeatVecValue_None,
   AzStyleBackgroundRepeatVecValue_Inherit,
   AzStyleBackgroundRepeatVecValue_Initial,
        Exact(AzStyleBackgroundRepeatVec),
} AzStyleBackgroundRepeatVecValue;

typedef enum {
   AzStyleBackgroundSizeVecValue_Auto,
   AzStyleBackgroundSizeVecValue_None,
   AzStyleBackgroundSizeVecValue_Inherit,
   AzStyleBackgroundSizeVecValue_Initial,
        Exact(AzStyleBackgroundSizeVec),
} AzStyleBackgroundSizeVecValue;

typedef enum {
   AzStyleTransformVecValue_Auto,
   AzStyleTransformVecValue_None,
   AzStyleTransformVecValue_Inherit,
   AzStyleTransformVecValue_Initial,
        Exact(AzStyleTransformVec),
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
        Line(AzSvgLine),
        QuadraticCurve(AzSvgQuadraticCurve),
        CubicCurve(AzSvgCubicCurve),
} AzSvgPathElement;

typedef struct {
    AzSvgVertexVec vertices;
    AzU32Vec indices;
} AzTesselatedCPUSvgNode;

typedef enum {
        Fill(AzSvgFillStyle),
        Stroke(AzSvgStrokeStyle),
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
   AzOptionRawImage_None,
        Some(AzRawImage),
} AzOptionRawImage;

typedef enum {
   AzOptionTaskBarIcon_None,
        Some(AzTaskBarIcon),
} AzOptionTaskBarIcon;

typedef enum {
   AzOptionWindowIcon_None,
        Some(AzWindowIcon),
} AzOptionWindowIcon;

typedef enum {
   AzOptionString_None,
        Some(AzString),
} AzOptionString;

typedef enum {
   AzOptionDuration_None,
        Some(AzDuration),
} AzOptionDuration;

typedef enum {
   AzOptionInstant_None,
        Some(AzInstant),
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
    uint8_t[ ;4]title_bar_active_background_color;
    uint8_t[ ;4]title_bar_active_separator_color;
    uint8_t[ ;4]title_bar_active_text_color;
    uint8_t[ ;4]title_bar_inactive_background_color;
    uint8_t[ ;4]title_bar_inactive_separator_color;
    uint8_t[ ;4]title_bar_inactive_text_color;
    uint8_t[ ;4]maximize_idle_foreground_inactive_color;
    uint8_t[ ;4]minimize_idle_foreground_inactive_color;
    uint8_t[ ;4]close_idle_foreground_inactive_color;
    uint8_t[ ;4]maximize_hovered_foreground_inactive_color;
    uint8_t[ ;4]minimize_hovered_foreground_inactive_color;
    uint8_t[ ;4]close_hovered_foreground_inactive_color;
    uint8_t[ ;4]maximize_disabled_foreground_inactive_color;
    uint8_t[ ;4]minimize_disabled_foreground_inactive_color;
    uint8_t[ ;4]close_disabled_foreground_inactive_color;
    uint8_t[ ;4]maximize_idle_background_inactive_color;
    uint8_t[ ;4]minimize_idle_background_inactive_color;
    uint8_t[ ;4]close_idle_background_inactive_color;
    uint8_t[ ;4]maximize_hovered_background_inactive_color;
    uint8_t[ ;4]minimize_hovered_background_inactive_color;
    uint8_t[ ;4]close_hovered_background_inactive_color;
    uint8_t[ ;4]maximize_disabled_background_inactive_color;
    uint8_t[ ;4]minimize_disabled_background_inactive_color;
    uint8_t[ ;4]close_disabled_background_inactive_color;
    uint8_t[ ;4]maximize_idle_foreground_active_color;
    uint8_t[ ;4]minimize_idle_foreground_active_color;
    uint8_t[ ;4]close_idle_foreground_active_color;
    uint8_t[ ;4]maximize_hovered_foreground_active_color;
    uint8_t[ ;4]minimize_hovered_foreground_active_color;
    uint8_t[ ;4]close_hovered_foreground_active_color;
    uint8_t[ ;4]maximize_disabled_foreground_active_color;
    uint8_t[ ;4]minimize_disabled_foreground_active_color;
    uint8_t[ ;4]close_disabled_foreground_active_color;
    uint8_t[ ;4]maximize_idle_background_active_color;
    uint8_t[ ;4]minimize_idle_background_active_color;
    uint8_t[ ;4]close_idle_background_active_color;
    uint8_t[ ;4]maximize_hovered_background_active_color;
    uint8_t[ ;4]minimize_hovered_background_active_color;
    uint8_t[ ;4]close_hovered_background_active_color;
    uint8_t[ ;4]maximize_disabled_background_active_color;
    uint8_t[ ;4]minimize_disabled_background_active_color;
    uint8_t[ ;4]close_disabled_background_active_color;
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
typedef AzRefAnyDestructorType;
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
   AzNodeType_Div,
   AzNodeType_Body,
   AzNodeType_Br,
        Label(AzString),
        Image(AzImageId),
        IFrame(AzIFrameNode),
        GlTexture(AzGlTextureNode),
} AzNodeType;

typedef enum {
        Id(AzString),
        Class(AzString),
} AzIdOrClass;

typedef enum {
   AzCssPathSelector_Global,
        Type(AzNodeTypePath),
        Class(AzString),
        Id(AzString),
        PseudoSelector(AzCssPathPseudoSelector),
   AzCssPathSelector_DirectChildren,
   AzCssPathSelector_Children,
} AzCssPathSelector;

typedef struct {
    AzString inner;
} AzCssImageId;

typedef enum {
        LinearGradient(AzLinearGradient),
        RadialGradient(AzRadialGradient),
        ConicGradient(AzConicGradient),
        Image(AzCssImageId),
        Color(AzColorU),
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
   AzScrollbarStyleValue_Auto,
   AzScrollbarStyleValue_None,
   AzScrollbarStyleValue_Inherit,
   AzScrollbarStyleValue_Initial,
        Exact(AzScrollbarStyle),
} AzScrollbarStyleValue;

typedef enum {
   AzStyleFontFamilyValue_Auto,
   AzStyleFontFamilyValue_None,
   AzStyleFontFamilyValue_Inherit,
   AzStyleFontFamilyValue_Initial,
        Exact(AzStyleFontFamily),
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
        Embedded(AzU8Vec),
        File(AzString),
        Raw(AzRawImage),
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
   AzOptionWaylandTheme_None,
        Some(AzWaylandTheme),
} AzOptionWaylandTheme;

typedef enum {
   AzXmlStreamError_UnexpectedEndOfStream,
   AzXmlStreamError_InvalidName,
        NonXmlChar(AzNonXmlCharError),
        InvalidChar(AzInvalidCharError),
        InvalidCharMultiple(AzInvalidCharMultipleError),
        InvalidQuote(AzInvalidQuoteError),
        InvalidSpace(AzInvalidSpaceError),
        InvalidString(AzInvalidStringError),
   AzXmlStreamError_InvalidReference,
   AzXmlStreamError_InvalidExternalID,
   AzXmlStreamError_InvalidCommentData,
   AzXmlStreamError_InvalidCommentEnd,
   AzXmlStreamError_InvalidCharacterData,
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
   AzStyleBackgroundContentVecValue_Auto,
   AzStyleBackgroundContentVecValue_None,
   AzStyleBackgroundContentVecValue_Inherit,
   AzStyleBackgroundContentVecValue_Initial,
        Exact(AzStyleBackgroundContentVec),
} AzStyleBackgroundContentVecValue;

typedef enum {
        TextColor(AzStyleTextColorValue),
        FontSize(AzStyleFontSizeValue),
        FontFamily(AzStyleFontFamilyValue),
        TextAlign(AzStyleTextAlignmentHorzValue),
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
} AzCssProperty;

typedef enum {
        Css(AzCssPath),
   AzCssPropertySource_Inline,
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
        Embedded(AzEmbeddedFontSource),
        File(AzFileFontSource),
        System(AzSystemFontSource),
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
        Normal(AzCssProperty),
        Active(AzCssProperty),
        Focus(AzCssProperty),
        Hover(AzCssProperty),
} AzNodeDataInlineCssProperty;

typedef struct {
    AzString dynamic_id;
    AzCssProperty default_value;
} AzDynamicCssProperty;

typedef enum {
        MultiPolygonCollection(AzSvgMultiPolygonVec),
        MultiPolygon(AzSvgMultiPolygon),
        Path(AzSvgPath),
        Circle(AzSvgCircle),
        Rect(AzSvgRect),
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
} AzXmlParseError;

typedef struct {
    AzWindowState state;
    AzOptionRendererOptions renderer_type;
    AzOptionWindowTheme theme;
    AzOptionCallback create_callback;
} AzWindowCreateOptions;

typedef enum {
        Id(AzDomNodeId),
        Path(AzFocusTargetPath),
   AzFocusTarget_Previous,
   AzFocusTarget_Next,
   AzFocusTarget_First,
   AzFocusTarget_Last,
   AzFocusTarget_NoFocus,
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
        Static(AzCssProperty),
        Dynamic(AzDynamicCssProperty),
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
   AzXmlError_NoRootNode,
   AzXmlError_SizeLimit,
        ParserError(AzXmlParseError),
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
   AzOptionDom_None,
        Some(AzDom),
} AzOptionDom;

typedef enum {
   AzSvgParseError_InvalidFileSuffix,
   AzSvgParseError_FileOpenFailed,
   AzSvgParseError_NotAnUtf8Str,
   AzSvgParseError_MalformedGZip,
   AzSvgParseError_InvalidSize,
        ParsingFailed(AzXmlError),
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
        Ok(AzSvg),
        Err(AzSvgParseError),
} AzResultSvgSvgParseError;

typedef struct {
    AzStylesheetVec stylesheets;
} AzCss;


#endif // AZUL_H
