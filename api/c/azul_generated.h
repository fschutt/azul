#ifndef AZUL_H
#define AZUL_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

/* C89 port for "restrict" keyword from C99 */

#if __STDC__ != 1
#    define restrict __restrict
#else
#    ifndef __STDC_VERSION__
#        define restrict __restrict
#    else
#        if __STDC_VERSION__ < 199901L
#            define restrict __restrict
#        endif
#    endif
#endif

/* cross-platform define for ssize_t (signed size_t) */
#ifdef _WIN32
#include <windows.h>
#ifdef _MSC_VER
typedef SSIZE_T ssize_t;
#endif
#else
#include <sys/types.h>
#include <sys/socket.h>
#include <arpa/inet.h>
#include <unistd.h>
#endif


struct AzRefAny;
typedef struct AzRefAny AzRefAny;
struct AzLayoutInfo;
typedef struct AzLayoutInfo AzLayoutInfo;
struct AzStyledDom;
typedef struct AzStyledDom AzStyledDom;
typedef AzStyledDom (*AzLayoutCallbackType)(AzRefAny* restrict A, AzLayoutInfo B);

struct AzCallbackInfo;
typedef struct AzCallbackInfo AzCallbackInfo;
enum AzUpdateScreen;
typedef enum AzUpdateScreen AzUpdateScreen;
typedef AzUpdateScreen (*AzCallbackType)(AzRefAny* restrict A, AzCallbackInfo B);

struct AzIFrameCallbackInfo;
typedef struct AzIFrameCallbackInfo AzIFrameCallbackInfo;
struct AzIFrameCallbackReturn;
typedef struct AzIFrameCallbackReturn AzIFrameCallbackReturn;
typedef AzIFrameCallbackReturn (*AzIFrameCallbackType)(AzRefAny* restrict A, AzIFrameCallbackInfo B);

struct AzGlCallbackInfo;
typedef struct AzGlCallbackInfo AzGlCallbackInfo;
struct AzGlCallbackReturn;
typedef struct AzGlCallbackReturn AzGlCallbackReturn;
typedef AzGlCallbackReturn (*AzGlCallbackType)(AzRefAny* restrict A, AzGlCallbackInfo B);

struct AzTimerCallbackInfo;
typedef struct AzTimerCallbackInfo AzTimerCallbackInfo;
struct AzTimerCallbackReturn;
typedef struct AzTimerCallbackReturn AzTimerCallbackReturn;
typedef AzTimerCallbackReturn (*AzTimerCallbackType)(AzRefAny* restrict A, AzRefAny* restrict B, AzTimerCallbackInfo C);

typedef AzUpdateScreen (*AzWriteBackCallbackType)(AzRefAny* restrict A, AzRefAny B, AzCallbackInfo C);

struct AzThreadSender;
typedef struct AzThreadSender AzThreadSender;
struct AzThreadReceiver;
typedef struct AzThreadReceiver AzThreadReceiver;
typedef void (*AzThreadCallbackType)(AzRefAny A, AzThreadSender B, AzThreadReceiver C);

typedef void (*AzRefAnyDestructorType)(void* restrict A);

struct AzThreadCallback;
typedef struct AzThreadCallback AzThreadCallback;
struct AzThread;
typedef struct AzThread AzThread;
typedef AzThread (*AzCreateThreadFnType)(AzRefAny A, AzRefAny B, AzThreadCallback C);

union AzInstant;
typedef union AzInstant AzInstant;
typedef AzInstant (*AzGetSystemTimeFnType)();

typedef bool (*AzCheckThreadFinishedFnType)(void* const A);

enum AzThreadSendMsg;
typedef enum AzThreadSendMsg AzThreadSendMsg;
typedef bool (*AzLibrarySendThreadMsgFnType)(void* restrict A, AzThreadSendMsg B);

union AzOptionThreadReceiveMsg;
typedef union AzOptionThreadReceiveMsg AzOptionThreadReceiveMsg;
typedef AzOptionThreadReceiveMsg (*AzLibraryReceiveThreadMsgFnType)(void* restrict A);

union AzOptionThreadSendMsg;
typedef union AzOptionThreadSendMsg AzOptionThreadSendMsg;
typedef AzOptionThreadSendMsg (*AzThreadRecvFnType)(void* restrict A);

union AzThreadReceiveMsg;
typedef union AzThreadReceiveMsg AzThreadReceiveMsg;
typedef bool (*AzThreadSendFnType)(void* restrict A, AzThreadReceiveMsg B);

typedef void (*AzThreadDestructorFnType)(void* restrict A, void* restrict B, void* restrict C, void* restrict D);

typedef void (*AzThreadReceiverDestructorFnType)(AzThreadReceiver* restrict A);

typedef void (*AzThreadSenderDestructorFnType)(AzThreadSender* restrict A);

struct AzMonitorVec;
typedef struct AzMonitorVec AzMonitorVec;
typedef void (*AzMonitorVecDestructorType)(AzMonitorVec* restrict A);

struct AzVideoModeVec;
typedef struct AzVideoModeVec AzVideoModeVec;
typedef void (*AzVideoModeVecDestructorType)(AzVideoModeVec* restrict A);

struct AzDomVec;
typedef struct AzDomVec AzDomVec;
typedef void (*AzDomVecDestructorType)(AzDomVec* restrict A);

struct AzIdOrClassVec;
typedef struct AzIdOrClassVec AzIdOrClassVec;
typedef void (*AzIdOrClassVecDestructorType)(AzIdOrClassVec* restrict A);

struct AzNodeDataInlineCssPropertyVec;
typedef struct AzNodeDataInlineCssPropertyVec AzNodeDataInlineCssPropertyVec;
typedef void (*AzNodeDataInlineCssPropertyVecDestructorType)(AzNodeDataInlineCssPropertyVec* restrict A);

struct AzStyleBackgroundContentVec;
typedef struct AzStyleBackgroundContentVec AzStyleBackgroundContentVec;
typedef void (*AzStyleBackgroundContentVecDestructorType)(AzStyleBackgroundContentVec* restrict A);

struct AzStyleBackgroundPositionVec;
typedef struct AzStyleBackgroundPositionVec AzStyleBackgroundPositionVec;
typedef void (*AzStyleBackgroundPositionVecDestructorType)(AzStyleBackgroundPositionVec* restrict A);

struct AzStyleBackgroundRepeatVec;
typedef struct AzStyleBackgroundRepeatVec AzStyleBackgroundRepeatVec;
typedef void (*AzStyleBackgroundRepeatVecDestructorType)(AzStyleBackgroundRepeatVec* restrict A);

struct AzStyleBackgroundSizeVec;
typedef struct AzStyleBackgroundSizeVec AzStyleBackgroundSizeVec;
typedef void (*AzStyleBackgroundSizeVecDestructorType)(AzStyleBackgroundSizeVec* restrict A);

struct AzStyleTransformVec;
typedef struct AzStyleTransformVec AzStyleTransformVec;
typedef void (*AzStyleTransformVecDestructorType)(AzStyleTransformVec* restrict A);

struct AzCssPropertyVec;
typedef struct AzCssPropertyVec AzCssPropertyVec;
typedef void (*AzCssPropertyVecDestructorType)(AzCssPropertyVec* restrict A);

struct AzSvgMultiPolygonVec;
typedef struct AzSvgMultiPolygonVec AzSvgMultiPolygonVec;
typedef void (*AzSvgMultiPolygonVecDestructorType)(AzSvgMultiPolygonVec* restrict A);

struct AzSvgPathVec;
typedef struct AzSvgPathVec AzSvgPathVec;
typedef void (*AzSvgPathVecDestructorType)(AzSvgPathVec* restrict A);

struct AzVertexAttributeVec;
typedef struct AzVertexAttributeVec AzVertexAttributeVec;
typedef void (*AzVertexAttributeVecDestructorType)(AzVertexAttributeVec* restrict A);

struct AzSvgPathElementVec;
typedef struct AzSvgPathElementVec AzSvgPathElementVec;
typedef void (*AzSvgPathElementVecDestructorType)(AzSvgPathElementVec* restrict A);

struct AzSvgVertexVec;
typedef struct AzSvgVertexVec AzSvgVertexVec;
typedef void (*AzSvgVertexVecDestructorType)(AzSvgVertexVec* restrict A);

struct AzU32Vec;
typedef struct AzU32Vec AzU32Vec;
typedef void (*AzU32VecDestructorType)(AzU32Vec* restrict A);

struct AzXWindowTypeVec;
typedef struct AzXWindowTypeVec AzXWindowTypeVec;
typedef void (*AzXWindowTypeVecDestructorType)(AzXWindowTypeVec* restrict A);

struct AzVirtualKeyCodeVec;
typedef struct AzVirtualKeyCodeVec AzVirtualKeyCodeVec;
typedef void (*AzVirtualKeyCodeVecDestructorType)(AzVirtualKeyCodeVec* restrict A);

struct AzCascadeInfoVec;
typedef struct AzCascadeInfoVec AzCascadeInfoVec;
typedef void (*AzCascadeInfoVecDestructorType)(AzCascadeInfoVec* restrict A);

struct AzScanCodeVec;
typedef struct AzScanCodeVec AzScanCodeVec;
typedef void (*AzScanCodeVecDestructorType)(AzScanCodeVec* restrict A);

struct AzCssDeclarationVec;
typedef struct AzCssDeclarationVec AzCssDeclarationVec;
typedef void (*AzCssDeclarationVecDestructorType)(AzCssDeclarationVec* restrict A);

struct AzCssPathSelectorVec;
typedef struct AzCssPathSelectorVec AzCssPathSelectorVec;
typedef void (*AzCssPathSelectorVecDestructorType)(AzCssPathSelectorVec* restrict A);

struct AzStylesheetVec;
typedef struct AzStylesheetVec AzStylesheetVec;
typedef void (*AzStylesheetVecDestructorType)(AzStylesheetVec* restrict A);

struct AzCssRuleBlockVec;
typedef struct AzCssRuleBlockVec AzCssRuleBlockVec;
typedef void (*AzCssRuleBlockVecDestructorType)(AzCssRuleBlockVec* restrict A);

struct AzU8Vec;
typedef struct AzU8Vec AzU8Vec;
typedef void (*AzU8VecDestructorType)(AzU8Vec* restrict A);

struct AzCallbackDataVec;
typedef struct AzCallbackDataVec AzCallbackDataVec;
typedef void (*AzCallbackDataVecDestructorType)(AzCallbackDataVec* restrict A);

struct AzDebugMessageVec;
typedef struct AzDebugMessageVec AzDebugMessageVec;
typedef void (*AzDebugMessageVecDestructorType)(AzDebugMessageVec* restrict A);

struct AzGLuintVec;
typedef struct AzGLuintVec AzGLuintVec;
typedef void (*AzGLuintVecDestructorType)(AzGLuintVec* restrict A);

struct AzGLintVec;
typedef struct AzGLintVec AzGLintVec;
typedef void (*AzGLintVecDestructorType)(AzGLintVec* restrict A);

struct AzStringVec;
typedef struct AzStringVec AzStringVec;
typedef void (*AzStringVecDestructorType)(AzStringVec* restrict A);

struct AzStringPairVec;
typedef struct AzStringPairVec AzStringPairVec;
typedef void (*AzStringPairVecDestructorType)(AzStringPairVec* restrict A);

struct AzLinearColorStopVec;
typedef struct AzLinearColorStopVec AzLinearColorStopVec;
typedef void (*AzLinearColorStopVecDestructorType)(AzLinearColorStopVec* restrict A);

struct AzRadialColorStopVec;
typedef struct AzRadialColorStopVec AzRadialColorStopVec;
typedef void (*AzRadialColorStopVecDestructorType)(AzRadialColorStopVec* restrict A);

struct AzNodeIdVec;
typedef struct AzNodeIdVec AzNodeIdVec;
typedef void (*AzNodeIdVecDestructorType)(AzNodeIdVec* restrict A);

struct AzNodeVec;
typedef struct AzNodeVec AzNodeVec;
typedef void (*AzNodeVecDestructorType)(AzNodeVec* restrict A);

struct AzStyledNodeVec;
typedef struct AzStyledNodeVec AzStyledNodeVec;
typedef void (*AzStyledNodeVecDestructorType)(AzStyledNodeVec* restrict A);

struct AzTagIdsToNodeIdsMappingVec;
typedef struct AzTagIdsToNodeIdsMappingVec AzTagIdsToNodeIdsMappingVec;
typedef void (*AzTagIdsToNodeIdsMappingVecDestructorType)(AzTagIdsToNodeIdsMappingVec* restrict A);

struct AzParentWithNodeDepthVec;
typedef struct AzParentWithNodeDepthVec AzParentWithNodeDepthVec;
typedef void (*AzParentWithNodeDepthVecDestructorType)(AzParentWithNodeDepthVec* restrict A);

struct AzNodeDataVec;
typedef struct AzNodeDataVec AzNodeDataVec;
typedef void (*AzNodeDataVecDestructorType)(AzNodeDataVec* restrict A);

struct AzInstantPtr;
typedef struct AzInstantPtr AzInstantPtr;
typedef AzInstantPtr (*AzInstantPtrCloneFnType)(void* const A);

typedef void (*AzInstantPtrDestructorFnType)(void* restrict A);


struct AzApp {
    void* const ptr;
};
typedef struct AzApp AzApp;

enum AzAppLogLevel {
   AzAppLogLevel_Off,
   AzAppLogLevel_Error,
   AzAppLogLevel_Warn,
   AzAppLogLevel_Info,
   AzAppLogLevel_Debug,
   AzAppLogLevel_Trace,
};
typedef enum AzAppLogLevel AzAppLogLevel;

enum AzVsync {
   AzVsync_Enabled,
   AzVsync_Disabled,
};
typedef enum AzVsync AzVsync;

enum AzSrgb {
   AzSrgb_Enabled,
   AzSrgb_Disabled,
};
typedef enum AzSrgb AzSrgb;

enum AzHwAcceleration {
   AzHwAcceleration_Enabled,
   AzHwAcceleration_Disabled,
};
typedef enum AzHwAcceleration AzHwAcceleration;

struct AzLayoutPoint {
    ssize_t x;
    ssize_t y;
};
typedef struct AzLayoutPoint AzLayoutPoint;

struct AzLayoutSize {
    ssize_t width;
    ssize_t height;
};
typedef struct AzLayoutSize AzLayoutSize;

struct AzIOSHandle {
    void* restrict ui_window;
    void* restrict ui_view;
    void* restrict ui_view_controller;
};
typedef struct AzIOSHandle AzIOSHandle;

struct AzMacOSHandle {
    void* restrict ns_window;
    void* restrict ns_view;
};
typedef struct AzMacOSHandle AzMacOSHandle;

struct AzXlibHandle {
    uint64_t window;
    void* restrict display;
};
typedef struct AzXlibHandle AzXlibHandle;

struct AzXcbHandle {
    uint32_t window;
    void* restrict connection;
};
typedef struct AzXcbHandle AzXcbHandle;

struct AzWaylandHandle {
    void* restrict surface;
    void* restrict display;
};
typedef struct AzWaylandHandle AzWaylandHandle;

struct AzWindowsHandle {
    void* restrict hwnd;
    void* restrict hinstance;
};
typedef struct AzWindowsHandle AzWindowsHandle;

struct AzWebHandle {
    uint32_t id;
};
typedef struct AzWebHandle AzWebHandle;

struct AzAndroidHandle {
    void* restrict a_native_window;
};
typedef struct AzAndroidHandle AzAndroidHandle;

enum AzXWindowType {
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
};
typedef enum AzXWindowType AzXWindowType;

struct AzPhysicalPositionI32 {
    int32_t x;
    int32_t y;
};
typedef struct AzPhysicalPositionI32 AzPhysicalPositionI32;

struct AzPhysicalSizeU32 {
    uint32_t width;
    uint32_t height;
};
typedef struct AzPhysicalSizeU32 AzPhysicalSizeU32;

struct AzLogicalPosition {
    float x;
    float y;
};
typedef struct AzLogicalPosition AzLogicalPosition;

struct AzLogicalSize {
    float width;
    float height;
};
typedef struct AzLogicalSize AzLogicalSize;

struct AzIconKey {
    size_t id;
};
typedef struct AzIconKey AzIconKey;

enum AzVirtualKeyCode {
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
};
typedef enum AzVirtualKeyCode AzVirtualKeyCode;

struct AzWindowFlags {
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
};
typedef struct AzWindowFlags AzWindowFlags;

struct AzDebugState {
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
};
typedef struct AzDebugState AzDebugState;

enum AzMouseCursorType {
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
};
typedef enum AzMouseCursorType AzMouseCursorType;

enum AzRendererType {
   AzRendererType_Hardware,
   AzRendererType_Software,
};
typedef enum AzRendererType AzRendererType;

struct AzMacWindowOptions {
    uint8_t _reserved;
};
typedef struct AzMacWindowOptions AzMacWindowOptions;

struct AzWasmWindowOptions {
    uint8_t _reserved;
};
typedef struct AzWasmWindowOptions AzWasmWindowOptions;

enum AzFullScreenMode {
   AzFullScreenMode_SlowFullScreen,
   AzFullScreenMode_FastFullScreen,
   AzFullScreenMode_SlowWindowed,
   AzFullScreenMode_FastWindowed,
};
typedef enum AzFullScreenMode AzFullScreenMode;

enum AzWindowTheme {
   AzWindowTheme_DarkMode,
   AzWindowTheme_LightMode,
};
typedef enum AzWindowTheme AzWindowTheme;

struct AzTouchState {
    uint8_t unused;
};
typedef struct AzTouchState AzTouchState;

struct AzMonitorHandle {
    void* restrict ptr;
};
typedef struct AzMonitorHandle AzMonitorHandle;

struct AzLayoutCallback {
    AzLayoutCallbackType cb;
};
typedef struct AzLayoutCallback AzLayoutCallback;

struct AzCallback {
    AzCallbackType cb;
};
typedef struct AzCallback AzCallback;

enum AzUpdateScreen {
   AzUpdateScreen_DoNothing,
   AzUpdateScreen_RegenerateStyledDomForCurrentWindow,
   AzUpdateScreen_RegenerateStyledDomForAllWindows,
};
typedef enum AzUpdateScreen AzUpdateScreen;

struct AzNodeId {
    size_t inner;
};
typedef struct AzNodeId AzNodeId;

struct AzDomId {
    size_t inner;
};
typedef struct AzDomId AzDomId;

struct AzIFrameCallback {
    AzIFrameCallbackType cb;
};
typedef struct AzIFrameCallback AzIFrameCallback;

struct AzGlCallback {
    AzGlCallbackType cb;
};
typedef struct AzGlCallback AzGlCallback;

struct AzTimerCallback {
    AzTimerCallbackType cb;
};
typedef struct AzTimerCallback AzTimerCallback;

struct AzWriteBackCallback {
    AzWriteBackCallbackType cb;
};
typedef struct AzWriteBackCallback AzWriteBackCallback;

struct AzThreadCallback {
    AzThreadCallbackType cb;
};
typedef struct AzThreadCallback AzThreadCallback;

enum AzOn {
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
};
typedef enum AzOn AzOn;

enum AzHoverEventFilter {
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
};
typedef enum AzHoverEventFilter AzHoverEventFilter;

enum AzFocusEventFilter {
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
};
typedef enum AzFocusEventFilter AzFocusEventFilter;

enum AzWindowEventFilter {
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
};
typedef enum AzWindowEventFilter AzWindowEventFilter;

enum AzComponentEventFilter {
   AzComponentEventFilter_AfterMount,
   AzComponentEventFilter_BeforeUnmount,
   AzComponentEventFilter_NodeResized,
};
typedef enum AzComponentEventFilter AzComponentEventFilter;

enum AzApplicationEventFilter {
   AzApplicationEventFilter_DeviceConnected,
   AzApplicationEventFilter_DeviceDisconnected,
};
typedef enum AzApplicationEventFilter AzApplicationEventFilter;

enum AzTabIndexTag {
   AzTabIndexTag_Auto,
   AzTabIndexTag_OverrideInParent,
   AzTabIndexTag_NoKeyboardFocus,
};
typedef enum AzTabIndexTag AzTabIndexTag;

struct AzTabIndexVariant_Auto { AzTabIndexTag tag; };
typedef struct AzTabIndexVariant_Auto AzTabIndexVariant_Auto;

struct AzTabIndexVariant_OverrideInParent { AzTabIndexTag tag; uint32_t payload; };
typedef struct AzTabIndexVariant_OverrideInParent AzTabIndexVariant_OverrideInParent;

struct AzTabIndexVariant_NoKeyboardFocus { AzTabIndexTag tag; };
typedef struct AzTabIndexVariant_NoKeyboardFocus AzTabIndexVariant_NoKeyboardFocus;


union AzTabIndex {
    AzTabIndexVariant_Auto Auto;
    AzTabIndexVariant_OverrideInParent OverrideInParent;
    AzTabIndexVariant_NoKeyboardFocus NoKeyboardFocus;
};
typedef union AzTabIndex AzTabIndex;

enum AzNodeTypePath {
   AzNodeTypePath_Body,
   AzNodeTypePath_Div,
   AzNodeTypePath_Br,
   AzNodeTypePath_P,
   AzNodeTypePath_Img,
   AzNodeTypePath_Texture,
   AzNodeTypePath_IFrame,
};
typedef enum AzNodeTypePath AzNodeTypePath;

struct AzCssNthChildPattern {
    uint32_t repeat;
    uint32_t offset;
};
typedef struct AzCssNthChildPattern AzCssNthChildPattern;

enum AzCssPropertyType {
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
};
typedef enum AzCssPropertyType AzCssPropertyType;

struct AzColorU {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
};
typedef struct AzColorU AzColorU;

enum AzSizeMetric {
   AzSizeMetric_Px,
   AzSizeMetric_Pt,
   AzSizeMetric_Em,
   AzSizeMetric_Percent,
};
typedef enum AzSizeMetric AzSizeMetric;

struct AzFloatValue {
    ssize_t number;
};
typedef struct AzFloatValue AzFloatValue;

enum AzBoxShadowClipMode {
   AzBoxShadowClipMode_Outset,
   AzBoxShadowClipMode_Inset,
};
typedef enum AzBoxShadowClipMode AzBoxShadowClipMode;

enum AzLayoutAlignContent {
   AzLayoutAlignContent_Stretch,
   AzLayoutAlignContent_Center,
   AzLayoutAlignContent_Start,
   AzLayoutAlignContent_End,
   AzLayoutAlignContent_SpaceBetween,
   AzLayoutAlignContent_SpaceAround,
};
typedef enum AzLayoutAlignContent AzLayoutAlignContent;

enum AzLayoutAlignItems {
   AzLayoutAlignItems_Stretch,
   AzLayoutAlignItems_Center,
   AzLayoutAlignItems_FlexStart,
   AzLayoutAlignItems_FlexEnd,
};
typedef enum AzLayoutAlignItems AzLayoutAlignItems;

enum AzLayoutBoxSizing {
   AzLayoutBoxSizing_ContentBox,
   AzLayoutBoxSizing_BorderBox,
};
typedef enum AzLayoutBoxSizing AzLayoutBoxSizing;

enum AzLayoutFlexDirection {
   AzLayoutFlexDirection_Row,
   AzLayoutFlexDirection_RowReverse,
   AzLayoutFlexDirection_Column,
   AzLayoutFlexDirection_ColumnReverse,
};
typedef enum AzLayoutFlexDirection AzLayoutFlexDirection;

enum AzLayoutDisplay {
   AzLayoutDisplay_Flex,
   AzLayoutDisplay_Block,
   AzLayoutDisplay_InlineBlock,
};
typedef enum AzLayoutDisplay AzLayoutDisplay;

enum AzLayoutFloat {
   AzLayoutFloat_Left,
   AzLayoutFloat_Right,
};
typedef enum AzLayoutFloat AzLayoutFloat;

enum AzLayoutJustifyContent {
   AzLayoutJustifyContent_Start,
   AzLayoutJustifyContent_End,
   AzLayoutJustifyContent_Center,
   AzLayoutJustifyContent_SpaceBetween,
   AzLayoutJustifyContent_SpaceAround,
   AzLayoutJustifyContent_SpaceEvenly,
};
typedef enum AzLayoutJustifyContent AzLayoutJustifyContent;

enum AzLayoutPosition {
   AzLayoutPosition_Static,
   AzLayoutPosition_Relative,
   AzLayoutPosition_Absolute,
   AzLayoutPosition_Fixed,
};
typedef enum AzLayoutPosition AzLayoutPosition;

enum AzLayoutFlexWrap {
   AzLayoutFlexWrap_Wrap,
   AzLayoutFlexWrap_NoWrap,
};
typedef enum AzLayoutFlexWrap AzLayoutFlexWrap;

enum AzLayoutOverflow {
   AzLayoutOverflow_Scroll,
   AzLayoutOverflow_Auto,
   AzLayoutOverflow_Hidden,
   AzLayoutOverflow_Visible,
};
typedef enum AzLayoutOverflow AzLayoutOverflow;

enum AzAngleMetric {
   AzAngleMetric_Degree,
   AzAngleMetric_Radians,
   AzAngleMetric_Grad,
   AzAngleMetric_Turn,
   AzAngleMetric_Percent,
};
typedef enum AzAngleMetric AzAngleMetric;

enum AzDirectionCorner {
   AzDirectionCorner_Right,
   AzDirectionCorner_Left,
   AzDirectionCorner_Top,
   AzDirectionCorner_Bottom,
   AzDirectionCorner_TopRight,
   AzDirectionCorner_TopLeft,
   AzDirectionCorner_BottomRight,
   AzDirectionCorner_BottomLeft,
};
typedef enum AzDirectionCorner AzDirectionCorner;

enum AzExtendMode {
   AzExtendMode_Clamp,
   AzExtendMode_Repeat,
};
typedef enum AzExtendMode AzExtendMode;

enum AzShape {
   AzShape_Ellipse,
   AzShape_Circle,
};
typedef enum AzShape AzShape;

enum AzRadialGradientSize {
   AzRadialGradientSize_ClosestSide,
   AzRadialGradientSize_ClosestCorner,
   AzRadialGradientSize_FarthestSide,
   AzRadialGradientSize_FarthestCorner,
};
typedef enum AzRadialGradientSize AzRadialGradientSize;

enum AzStyleBackgroundRepeat {
   AzStyleBackgroundRepeat_NoRepeat,
   AzStyleBackgroundRepeat_Repeat,
   AzStyleBackgroundRepeat_RepeatX,
   AzStyleBackgroundRepeat_RepeatY,
};
typedef enum AzStyleBackgroundRepeat AzStyleBackgroundRepeat;

enum AzBorderStyle {
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
};
typedef enum AzBorderStyle AzBorderStyle;

enum AzStyleCursor {
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
};
typedef enum AzStyleCursor AzStyleCursor;

enum AzStyleBackfaceVisibility {
   AzStyleBackfaceVisibility_Hidden,
   AzStyleBackfaceVisibility_Visible,
};
typedef enum AzStyleBackfaceVisibility AzStyleBackfaceVisibility;

enum AzStyleTextAlignmentHorz {
   AzStyleTextAlignmentHorz_Left,
   AzStyleTextAlignmentHorz_Center,
   AzStyleTextAlignmentHorz_Right,
};
typedef enum AzStyleTextAlignmentHorz AzStyleTextAlignmentHorz;

struct AzNode {
    size_t parent;
    size_t previous_sibling;
    size_t next_sibling;
    size_t last_child;
};
typedef struct AzNode AzNode;

struct AzCascadeInfo {
    uint32_t index_in_parent;
    bool  is_last_child;
};
typedef struct AzCascadeInfo AzCascadeInfo;

struct AzStyledNodeState {
    bool  normal;
    bool  hover;
    bool  active;
    bool  focused;
};
typedef struct AzStyledNodeState AzStyledNodeState;

struct AzTagId {
    uint64_t inner;
};
typedef struct AzTagId AzTagId;

struct AzCssPropertyCache {
    void* restrict ptr;
};
typedef struct AzCssPropertyCache AzCssPropertyCache;

struct AzGlShaderPrecisionFormatReturn {
    int32_t _0;
    int32_t _1;
    int32_t _2;
};
typedef struct AzGlShaderPrecisionFormatReturn AzGlShaderPrecisionFormatReturn;

enum AzVertexAttributeType {
   AzVertexAttributeType_Float,
   AzVertexAttributeType_Double,
   AzVertexAttributeType_UnsignedByte,
   AzVertexAttributeType_UnsignedShort,
   AzVertexAttributeType_UnsignedInt,
};
typedef enum AzVertexAttributeType AzVertexAttributeType;

enum AzIndexBufferFormat {
   AzIndexBufferFormat_Points,
   AzIndexBufferFormat_Lines,
   AzIndexBufferFormat_LineStrip,
   AzIndexBufferFormat_Triangles,
   AzIndexBufferFormat_TriangleStrip,
   AzIndexBufferFormat_TriangleFan,
};
typedef enum AzIndexBufferFormat AzIndexBufferFormat;

enum AzGlType {
   AzGlType_Gl,
   AzGlType_Gles,
};
typedef enum AzGlType AzGlType;

struct AzU8VecRef {
    uint8_t* const ptr;
    size_t len;
};
typedef struct AzU8VecRef AzU8VecRef;

struct AzU8VecRefMut {
    uint8_t* restrict ptr;
    size_t len;
};
typedef struct AzU8VecRefMut AzU8VecRefMut;

struct AzF32VecRef {
    float* const ptr;
    size_t len;
};
typedef struct AzF32VecRef AzF32VecRef;

struct AzI32VecRef {
    int32_t* const ptr;
    size_t len;
};
typedef struct AzI32VecRef AzI32VecRef;

struct AzGLuintVecRef {
    uint32_t* const ptr;
    size_t len;
};
typedef struct AzGLuintVecRef AzGLuintVecRef;

struct AzGLenumVecRef {
    uint32_t* const ptr;
    size_t len;
};
typedef struct AzGLenumVecRef AzGLenumVecRef;

struct AzGLintVecRefMut {
    int32_t* restrict ptr;
    size_t len;
};
typedef struct AzGLintVecRefMut AzGLintVecRefMut;

struct AzGLint64VecRefMut {
    int64_t* restrict ptr;
    size_t len;
};
typedef struct AzGLint64VecRefMut AzGLint64VecRefMut;

struct AzGLbooleanVecRefMut {
    uint8_t* restrict ptr;
    size_t len;
};
typedef struct AzGLbooleanVecRefMut AzGLbooleanVecRefMut;

struct AzGLfloatVecRefMut {
    float* restrict ptr;
    size_t len;
};
typedef struct AzGLfloatVecRefMut AzGLfloatVecRefMut;

struct AzRefstr {
    uint8_t* const ptr;
    size_t len;
};
typedef struct AzRefstr AzRefstr;

struct AzGLsyncPtr {
    void* const ptr;
};
typedef struct AzGLsyncPtr AzGLsyncPtr;

struct AzTextureFlags {
    bool  is_opaque;
    bool  is_video_texture;
};
typedef struct AzTextureFlags AzTextureFlags;

enum AzRawImageFormat {
   AzRawImageFormat_R8,
   AzRawImageFormat_R16,
   AzRawImageFormat_RG16,
   AzRawImageFormat_BGRA8,
   AzRawImageFormat_RGBAF32,
   AzRawImageFormat_RG8,
   AzRawImageFormat_RGBAI32,
   AzRawImageFormat_RGBA8,
};
typedef enum AzRawImageFormat AzRawImageFormat;

struct AzImageId {
    size_t id;
};
typedef struct AzImageId AzImageId;

struct AzFontId {
    size_t id;
};
typedef struct AzFontId AzFontId;

struct AzSvgCircle {
    float center_x;
    float center_y;
    float radius;
};
typedef struct AzSvgCircle AzSvgCircle;

struct AzSvgPoint {
    float x;
    float y;
};
typedef struct AzSvgPoint AzSvgPoint;

struct AzSvgVertex {
    float x;
    float y;
};
typedef struct AzSvgVertex AzSvgVertex;

struct AzSvgRect {
    float width;
    float height;
    float x;
    float y;
    float radius_top_left;
    float radius_top_right;
    float radius_bottom_left;
    float radius_bottom_right;
};
typedef struct AzSvgRect AzSvgRect;

enum AzSvgLineCap {
   AzSvgLineCap_Butt,
   AzSvgLineCap_Square,
   AzSvgLineCap_Round,
};
typedef enum AzSvgLineCap AzSvgLineCap;

enum AzShapeRendering {
   AzShapeRendering_OptimizeSpeed,
   AzShapeRendering_CrispEdges,
   AzShapeRendering_GeometricPrecision,
};
typedef enum AzShapeRendering AzShapeRendering;

enum AzTextRendering {
   AzTextRendering_OptimizeSpeed,
   AzTextRendering_OptimizeLegibility,
   AzTextRendering_GeometricPrecision,
};
typedef enum AzTextRendering AzTextRendering;

enum AzImageRendering {
   AzImageRendering_OptimizeQuality,
   AzImageRendering_OptimizeSpeed,
};
typedef enum AzImageRendering AzImageRendering;

enum AzFontDatabase {
   AzFontDatabase_Empty,
   AzFontDatabase_System,
};
typedef enum AzFontDatabase AzFontDatabase;

enum AzSvgFitToTag {
   AzSvgFitToTag_Original,
   AzSvgFitToTag_Width,
   AzSvgFitToTag_Height,
   AzSvgFitToTag_Zoom,
};
typedef enum AzSvgFitToTag AzSvgFitToTag;

struct AzSvgFitToVariant_Original { AzSvgFitToTag tag; };
typedef struct AzSvgFitToVariant_Original AzSvgFitToVariant_Original;

struct AzSvgFitToVariant_Width { AzSvgFitToTag tag; uint32_t payload; };
typedef struct AzSvgFitToVariant_Width AzSvgFitToVariant_Width;

struct AzSvgFitToVariant_Height { AzSvgFitToTag tag; uint32_t payload; };
typedef struct AzSvgFitToVariant_Height AzSvgFitToVariant_Height;

struct AzSvgFitToVariant_Zoom { AzSvgFitToTag tag; float payload; };
typedef struct AzSvgFitToVariant_Zoom AzSvgFitToVariant_Zoom;


union AzSvgFitTo {
    AzSvgFitToVariant_Original Original;
    AzSvgFitToVariant_Width Width;
    AzSvgFitToVariant_Height Height;
    AzSvgFitToVariant_Zoom Zoom;
};
typedef union AzSvgFitTo AzSvgFitTo;

struct AzSvg {
    void* restrict ptr;
};
typedef struct AzSvg AzSvg;

struct AzSvgXmlNode {
    void* restrict ptr;
};
typedef struct AzSvgXmlNode AzSvgXmlNode;

enum AzSvgLineJoin {
   AzSvgLineJoin_Miter,
   AzSvgLineJoin_MiterClip,
   AzSvgLineJoin_Round,
   AzSvgLineJoin_Bevel,
};
typedef enum AzSvgLineJoin AzSvgLineJoin;

struct AzSvgDashPattern {
    size_t offset;
    size_t length_1;
    size_t gap_1;
    size_t length_2;
    size_t gap_2;
    size_t length_3;
    size_t gap_3;
};
typedef struct AzSvgDashPattern AzSvgDashPattern;

struct AzTimerId {
    size_t id;
};
typedef struct AzTimerId AzTimerId;

enum AzTerminateTimer {
   AzTerminateTimer_Terminate,
   AzTerminateTimer_Continue,
};
typedef enum AzTerminateTimer AzTerminateTimer;

struct AzThreadId {
    size_t id;
};
typedef struct AzThreadId AzThreadId;

enum AzThreadSendMsg {
   AzThreadSendMsg_TerminateThread,
   AzThreadSendMsg_Tick,
};
typedef enum AzThreadSendMsg AzThreadSendMsg;

struct AzCreateThreadFn {
    AzCreateThreadFnType cb;
};
typedef struct AzCreateThreadFn AzCreateThreadFn;

struct AzGetSystemTimeFn {
    AzGetSystemTimeFnType cb;
};
typedef struct AzGetSystemTimeFn AzGetSystemTimeFn;

struct AzCheckThreadFinishedFn {
    AzCheckThreadFinishedFnType cb;
};
typedef struct AzCheckThreadFinishedFn AzCheckThreadFinishedFn;

struct AzLibrarySendThreadMsgFn {
    AzLibrarySendThreadMsgFnType cb;
};
typedef struct AzLibrarySendThreadMsgFn AzLibrarySendThreadMsgFn;

struct AzLibraryReceiveThreadMsgFn {
    AzLibraryReceiveThreadMsgFnType cb;
};
typedef struct AzLibraryReceiveThreadMsgFn AzLibraryReceiveThreadMsgFn;

struct AzThreadRecvFn {
    AzThreadRecvFnType cb;
};
typedef struct AzThreadRecvFn AzThreadRecvFn;

struct AzThreadSendFn {
    AzThreadSendFnType cb;
};
typedef struct AzThreadSendFn AzThreadSendFn;

struct AzThreadDestructorFn {
    AzThreadDestructorFnType cb;
};
typedef struct AzThreadDestructorFn AzThreadDestructorFn;

struct AzThreadReceiverDestructorFn {
    AzThreadReceiverDestructorFnType cb;
};
typedef struct AzThreadReceiverDestructorFn AzThreadReceiverDestructorFn;

struct AzThreadSenderDestructorFn {
    AzThreadSenderDestructorFnType cb;
};
typedef struct AzThreadSenderDestructorFn AzThreadSenderDestructorFn;

enum AzMonitorVecDestructorTag {
   AzMonitorVecDestructorTag_DefaultRust,
   AzMonitorVecDestructorTag_NoDestructor,
   AzMonitorVecDestructorTag_External,
};
typedef enum AzMonitorVecDestructorTag AzMonitorVecDestructorTag;

struct AzMonitorVecDestructorVariant_DefaultRust { AzMonitorVecDestructorTag tag; };
typedef struct AzMonitorVecDestructorVariant_DefaultRust AzMonitorVecDestructorVariant_DefaultRust;

struct AzMonitorVecDestructorVariant_NoDestructor { AzMonitorVecDestructorTag tag; };
typedef struct AzMonitorVecDestructorVariant_NoDestructor AzMonitorVecDestructorVariant_NoDestructor;

struct AzMonitorVecDestructorVariant_External { AzMonitorVecDestructorTag tag; AzMonitorVecDestructorType payload; };
typedef struct AzMonitorVecDestructorVariant_External AzMonitorVecDestructorVariant_External;


union AzMonitorVecDestructor {
    AzMonitorVecDestructorVariant_DefaultRust DefaultRust;
    AzMonitorVecDestructorVariant_NoDestructor NoDestructor;
    AzMonitorVecDestructorVariant_External External;
};
typedef union AzMonitorVecDestructor AzMonitorVecDestructor;

enum AzVideoModeVecDestructorTag {
   AzVideoModeVecDestructorTag_DefaultRust,
   AzVideoModeVecDestructorTag_NoDestructor,
   AzVideoModeVecDestructorTag_External,
};
typedef enum AzVideoModeVecDestructorTag AzVideoModeVecDestructorTag;

struct AzVideoModeVecDestructorVariant_DefaultRust { AzVideoModeVecDestructorTag tag; };
typedef struct AzVideoModeVecDestructorVariant_DefaultRust AzVideoModeVecDestructorVariant_DefaultRust;

struct AzVideoModeVecDestructorVariant_NoDestructor { AzVideoModeVecDestructorTag tag; };
typedef struct AzVideoModeVecDestructorVariant_NoDestructor AzVideoModeVecDestructorVariant_NoDestructor;

struct AzVideoModeVecDestructorVariant_External { AzVideoModeVecDestructorTag tag; AzVideoModeVecDestructorType payload; };
typedef struct AzVideoModeVecDestructorVariant_External AzVideoModeVecDestructorVariant_External;


union AzVideoModeVecDestructor {
    AzVideoModeVecDestructorVariant_DefaultRust DefaultRust;
    AzVideoModeVecDestructorVariant_NoDestructor NoDestructor;
    AzVideoModeVecDestructorVariant_External External;
};
typedef union AzVideoModeVecDestructor AzVideoModeVecDestructor;

enum AzDomVecDestructorTag {
   AzDomVecDestructorTag_DefaultRust,
   AzDomVecDestructorTag_NoDestructor,
   AzDomVecDestructorTag_External,
};
typedef enum AzDomVecDestructorTag AzDomVecDestructorTag;

struct AzDomVecDestructorVariant_DefaultRust { AzDomVecDestructorTag tag; };
typedef struct AzDomVecDestructorVariant_DefaultRust AzDomVecDestructorVariant_DefaultRust;

struct AzDomVecDestructorVariant_NoDestructor { AzDomVecDestructorTag tag; };
typedef struct AzDomVecDestructorVariant_NoDestructor AzDomVecDestructorVariant_NoDestructor;

struct AzDomVecDestructorVariant_External { AzDomVecDestructorTag tag; AzDomVecDestructorType payload; };
typedef struct AzDomVecDestructorVariant_External AzDomVecDestructorVariant_External;


union AzDomVecDestructor {
    AzDomVecDestructorVariant_DefaultRust DefaultRust;
    AzDomVecDestructorVariant_NoDestructor NoDestructor;
    AzDomVecDestructorVariant_External External;
};
typedef union AzDomVecDestructor AzDomVecDestructor;

enum AzIdOrClassVecDestructorTag {
   AzIdOrClassVecDestructorTag_DefaultRust,
   AzIdOrClassVecDestructorTag_NoDestructor,
   AzIdOrClassVecDestructorTag_External,
};
typedef enum AzIdOrClassVecDestructorTag AzIdOrClassVecDestructorTag;

struct AzIdOrClassVecDestructorVariant_DefaultRust { AzIdOrClassVecDestructorTag tag; };
typedef struct AzIdOrClassVecDestructorVariant_DefaultRust AzIdOrClassVecDestructorVariant_DefaultRust;

struct AzIdOrClassVecDestructorVariant_NoDestructor { AzIdOrClassVecDestructorTag tag; };
typedef struct AzIdOrClassVecDestructorVariant_NoDestructor AzIdOrClassVecDestructorVariant_NoDestructor;

struct AzIdOrClassVecDestructorVariant_External { AzIdOrClassVecDestructorTag tag; AzIdOrClassVecDestructorType payload; };
typedef struct AzIdOrClassVecDestructorVariant_External AzIdOrClassVecDestructorVariant_External;


union AzIdOrClassVecDestructor {
    AzIdOrClassVecDestructorVariant_DefaultRust DefaultRust;
    AzIdOrClassVecDestructorVariant_NoDestructor NoDestructor;
    AzIdOrClassVecDestructorVariant_External External;
};
typedef union AzIdOrClassVecDestructor AzIdOrClassVecDestructor;

enum AzNodeDataInlineCssPropertyVecDestructorTag {
   AzNodeDataInlineCssPropertyVecDestructorTag_DefaultRust,
   AzNodeDataInlineCssPropertyVecDestructorTag_NoDestructor,
   AzNodeDataInlineCssPropertyVecDestructorTag_External,
};
typedef enum AzNodeDataInlineCssPropertyVecDestructorTag AzNodeDataInlineCssPropertyVecDestructorTag;

struct AzNodeDataInlineCssPropertyVecDestructorVariant_DefaultRust { AzNodeDataInlineCssPropertyVecDestructorTag tag; };
typedef struct AzNodeDataInlineCssPropertyVecDestructorVariant_DefaultRust AzNodeDataInlineCssPropertyVecDestructorVariant_DefaultRust;

struct AzNodeDataInlineCssPropertyVecDestructorVariant_NoDestructor { AzNodeDataInlineCssPropertyVecDestructorTag tag; };
typedef struct AzNodeDataInlineCssPropertyVecDestructorVariant_NoDestructor AzNodeDataInlineCssPropertyVecDestructorVariant_NoDestructor;

struct AzNodeDataInlineCssPropertyVecDestructorVariant_External { AzNodeDataInlineCssPropertyVecDestructorTag tag; AzNodeDataInlineCssPropertyVecDestructorType payload; };
typedef struct AzNodeDataInlineCssPropertyVecDestructorVariant_External AzNodeDataInlineCssPropertyVecDestructorVariant_External;


union AzNodeDataInlineCssPropertyVecDestructor {
    AzNodeDataInlineCssPropertyVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeDataInlineCssPropertyVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeDataInlineCssPropertyVecDestructorVariant_External External;
};
typedef union AzNodeDataInlineCssPropertyVecDestructor AzNodeDataInlineCssPropertyVecDestructor;

enum AzStyleBackgroundContentVecDestructorTag {
   AzStyleBackgroundContentVecDestructorTag_DefaultRust,
   AzStyleBackgroundContentVecDestructorTag_NoDestructor,
   AzStyleBackgroundContentVecDestructorTag_External,
};
typedef enum AzStyleBackgroundContentVecDestructorTag AzStyleBackgroundContentVecDestructorTag;

struct AzStyleBackgroundContentVecDestructorVariant_DefaultRust { AzStyleBackgroundContentVecDestructorTag tag; };
typedef struct AzStyleBackgroundContentVecDestructorVariant_DefaultRust AzStyleBackgroundContentVecDestructorVariant_DefaultRust;

struct AzStyleBackgroundContentVecDestructorVariant_NoDestructor { AzStyleBackgroundContentVecDestructorTag tag; };
typedef struct AzStyleBackgroundContentVecDestructorVariant_NoDestructor AzStyleBackgroundContentVecDestructorVariant_NoDestructor;

struct AzStyleBackgroundContentVecDestructorVariant_External { AzStyleBackgroundContentVecDestructorTag tag; AzStyleBackgroundContentVecDestructorType payload; };
typedef struct AzStyleBackgroundContentVecDestructorVariant_External AzStyleBackgroundContentVecDestructorVariant_External;


union AzStyleBackgroundContentVecDestructor {
    AzStyleBackgroundContentVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundContentVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundContentVecDestructorVariant_External External;
};
typedef union AzStyleBackgroundContentVecDestructor AzStyleBackgroundContentVecDestructor;

enum AzStyleBackgroundPositionVecDestructorTag {
   AzStyleBackgroundPositionVecDestructorTag_DefaultRust,
   AzStyleBackgroundPositionVecDestructorTag_NoDestructor,
   AzStyleBackgroundPositionVecDestructorTag_External,
};
typedef enum AzStyleBackgroundPositionVecDestructorTag AzStyleBackgroundPositionVecDestructorTag;

struct AzStyleBackgroundPositionVecDestructorVariant_DefaultRust { AzStyleBackgroundPositionVecDestructorTag tag; };
typedef struct AzStyleBackgroundPositionVecDestructorVariant_DefaultRust AzStyleBackgroundPositionVecDestructorVariant_DefaultRust;

struct AzStyleBackgroundPositionVecDestructorVariant_NoDestructor { AzStyleBackgroundPositionVecDestructorTag tag; };
typedef struct AzStyleBackgroundPositionVecDestructorVariant_NoDestructor AzStyleBackgroundPositionVecDestructorVariant_NoDestructor;

struct AzStyleBackgroundPositionVecDestructorVariant_External { AzStyleBackgroundPositionVecDestructorTag tag; AzStyleBackgroundPositionVecDestructorType payload; };
typedef struct AzStyleBackgroundPositionVecDestructorVariant_External AzStyleBackgroundPositionVecDestructorVariant_External;


union AzStyleBackgroundPositionVecDestructor {
    AzStyleBackgroundPositionVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundPositionVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundPositionVecDestructorVariant_External External;
};
typedef union AzStyleBackgroundPositionVecDestructor AzStyleBackgroundPositionVecDestructor;

enum AzStyleBackgroundRepeatVecDestructorTag {
   AzStyleBackgroundRepeatVecDestructorTag_DefaultRust,
   AzStyleBackgroundRepeatVecDestructorTag_NoDestructor,
   AzStyleBackgroundRepeatVecDestructorTag_External,
};
typedef enum AzStyleBackgroundRepeatVecDestructorTag AzStyleBackgroundRepeatVecDestructorTag;

struct AzStyleBackgroundRepeatVecDestructorVariant_DefaultRust { AzStyleBackgroundRepeatVecDestructorTag tag; };
typedef struct AzStyleBackgroundRepeatVecDestructorVariant_DefaultRust AzStyleBackgroundRepeatVecDestructorVariant_DefaultRust;

struct AzStyleBackgroundRepeatVecDestructorVariant_NoDestructor { AzStyleBackgroundRepeatVecDestructorTag tag; };
typedef struct AzStyleBackgroundRepeatVecDestructorVariant_NoDestructor AzStyleBackgroundRepeatVecDestructorVariant_NoDestructor;

struct AzStyleBackgroundRepeatVecDestructorVariant_External { AzStyleBackgroundRepeatVecDestructorTag tag; AzStyleBackgroundRepeatVecDestructorType payload; };
typedef struct AzStyleBackgroundRepeatVecDestructorVariant_External AzStyleBackgroundRepeatVecDestructorVariant_External;


union AzStyleBackgroundRepeatVecDestructor {
    AzStyleBackgroundRepeatVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundRepeatVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundRepeatVecDestructorVariant_External External;
};
typedef union AzStyleBackgroundRepeatVecDestructor AzStyleBackgroundRepeatVecDestructor;

enum AzStyleBackgroundSizeVecDestructorTag {
   AzStyleBackgroundSizeVecDestructorTag_DefaultRust,
   AzStyleBackgroundSizeVecDestructorTag_NoDestructor,
   AzStyleBackgroundSizeVecDestructorTag_External,
};
typedef enum AzStyleBackgroundSizeVecDestructorTag AzStyleBackgroundSizeVecDestructorTag;

struct AzStyleBackgroundSizeVecDestructorVariant_DefaultRust { AzStyleBackgroundSizeVecDestructorTag tag; };
typedef struct AzStyleBackgroundSizeVecDestructorVariant_DefaultRust AzStyleBackgroundSizeVecDestructorVariant_DefaultRust;

struct AzStyleBackgroundSizeVecDestructorVariant_NoDestructor { AzStyleBackgroundSizeVecDestructorTag tag; };
typedef struct AzStyleBackgroundSizeVecDestructorVariant_NoDestructor AzStyleBackgroundSizeVecDestructorVariant_NoDestructor;

struct AzStyleBackgroundSizeVecDestructorVariant_External { AzStyleBackgroundSizeVecDestructorTag tag; AzStyleBackgroundSizeVecDestructorType payload; };
typedef struct AzStyleBackgroundSizeVecDestructorVariant_External AzStyleBackgroundSizeVecDestructorVariant_External;


union AzStyleBackgroundSizeVecDestructor {
    AzStyleBackgroundSizeVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleBackgroundSizeVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleBackgroundSizeVecDestructorVariant_External External;
};
typedef union AzStyleBackgroundSizeVecDestructor AzStyleBackgroundSizeVecDestructor;

enum AzStyleTransformVecDestructorTag {
   AzStyleTransformVecDestructorTag_DefaultRust,
   AzStyleTransformVecDestructorTag_NoDestructor,
   AzStyleTransformVecDestructorTag_External,
};
typedef enum AzStyleTransformVecDestructorTag AzStyleTransformVecDestructorTag;

struct AzStyleTransformVecDestructorVariant_DefaultRust { AzStyleTransformVecDestructorTag tag; };
typedef struct AzStyleTransformVecDestructorVariant_DefaultRust AzStyleTransformVecDestructorVariant_DefaultRust;

struct AzStyleTransformVecDestructorVariant_NoDestructor { AzStyleTransformVecDestructorTag tag; };
typedef struct AzStyleTransformVecDestructorVariant_NoDestructor AzStyleTransformVecDestructorVariant_NoDestructor;

struct AzStyleTransformVecDestructorVariant_External { AzStyleTransformVecDestructorTag tag; AzStyleTransformVecDestructorType payload; };
typedef struct AzStyleTransformVecDestructorVariant_External AzStyleTransformVecDestructorVariant_External;


union AzStyleTransformVecDestructor {
    AzStyleTransformVecDestructorVariant_DefaultRust DefaultRust;
    AzStyleTransformVecDestructorVariant_NoDestructor NoDestructor;
    AzStyleTransformVecDestructorVariant_External External;
};
typedef union AzStyleTransformVecDestructor AzStyleTransformVecDestructor;

enum AzCssPropertyVecDestructorTag {
   AzCssPropertyVecDestructorTag_DefaultRust,
   AzCssPropertyVecDestructorTag_NoDestructor,
   AzCssPropertyVecDestructorTag_External,
};
typedef enum AzCssPropertyVecDestructorTag AzCssPropertyVecDestructorTag;

struct AzCssPropertyVecDestructorVariant_DefaultRust { AzCssPropertyVecDestructorTag tag; };
typedef struct AzCssPropertyVecDestructorVariant_DefaultRust AzCssPropertyVecDestructorVariant_DefaultRust;

struct AzCssPropertyVecDestructorVariant_NoDestructor { AzCssPropertyVecDestructorTag tag; };
typedef struct AzCssPropertyVecDestructorVariant_NoDestructor AzCssPropertyVecDestructorVariant_NoDestructor;

struct AzCssPropertyVecDestructorVariant_External { AzCssPropertyVecDestructorTag tag; AzCssPropertyVecDestructorType payload; };
typedef struct AzCssPropertyVecDestructorVariant_External AzCssPropertyVecDestructorVariant_External;


union AzCssPropertyVecDestructor {
    AzCssPropertyVecDestructorVariant_DefaultRust DefaultRust;
    AzCssPropertyVecDestructorVariant_NoDestructor NoDestructor;
    AzCssPropertyVecDestructorVariant_External External;
};
typedef union AzCssPropertyVecDestructor AzCssPropertyVecDestructor;

enum AzSvgMultiPolygonVecDestructorTag {
   AzSvgMultiPolygonVecDestructorTag_DefaultRust,
   AzSvgMultiPolygonVecDestructorTag_NoDestructor,
   AzSvgMultiPolygonVecDestructorTag_External,
};
typedef enum AzSvgMultiPolygonVecDestructorTag AzSvgMultiPolygonVecDestructorTag;

struct AzSvgMultiPolygonVecDestructorVariant_DefaultRust { AzSvgMultiPolygonVecDestructorTag tag; };
typedef struct AzSvgMultiPolygonVecDestructorVariant_DefaultRust AzSvgMultiPolygonVecDestructorVariant_DefaultRust;

struct AzSvgMultiPolygonVecDestructorVariant_NoDestructor { AzSvgMultiPolygonVecDestructorTag tag; };
typedef struct AzSvgMultiPolygonVecDestructorVariant_NoDestructor AzSvgMultiPolygonVecDestructorVariant_NoDestructor;

struct AzSvgMultiPolygonVecDestructorVariant_External { AzSvgMultiPolygonVecDestructorTag tag; AzSvgMultiPolygonVecDestructorType payload; };
typedef struct AzSvgMultiPolygonVecDestructorVariant_External AzSvgMultiPolygonVecDestructorVariant_External;


union AzSvgMultiPolygonVecDestructor {
    AzSvgMultiPolygonVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgMultiPolygonVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgMultiPolygonVecDestructorVariant_External External;
};
typedef union AzSvgMultiPolygonVecDestructor AzSvgMultiPolygonVecDestructor;

enum AzSvgPathVecDestructorTag {
   AzSvgPathVecDestructorTag_DefaultRust,
   AzSvgPathVecDestructorTag_NoDestructor,
   AzSvgPathVecDestructorTag_External,
};
typedef enum AzSvgPathVecDestructorTag AzSvgPathVecDestructorTag;

struct AzSvgPathVecDestructorVariant_DefaultRust { AzSvgPathVecDestructorTag tag; };
typedef struct AzSvgPathVecDestructorVariant_DefaultRust AzSvgPathVecDestructorVariant_DefaultRust;

struct AzSvgPathVecDestructorVariant_NoDestructor { AzSvgPathVecDestructorTag tag; };
typedef struct AzSvgPathVecDestructorVariant_NoDestructor AzSvgPathVecDestructorVariant_NoDestructor;

struct AzSvgPathVecDestructorVariant_External { AzSvgPathVecDestructorTag tag; AzSvgPathVecDestructorType payload; };
typedef struct AzSvgPathVecDestructorVariant_External AzSvgPathVecDestructorVariant_External;


union AzSvgPathVecDestructor {
    AzSvgPathVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgPathVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgPathVecDestructorVariant_External External;
};
typedef union AzSvgPathVecDestructor AzSvgPathVecDestructor;

enum AzVertexAttributeVecDestructorTag {
   AzVertexAttributeVecDestructorTag_DefaultRust,
   AzVertexAttributeVecDestructorTag_NoDestructor,
   AzVertexAttributeVecDestructorTag_External,
};
typedef enum AzVertexAttributeVecDestructorTag AzVertexAttributeVecDestructorTag;

struct AzVertexAttributeVecDestructorVariant_DefaultRust { AzVertexAttributeVecDestructorTag tag; };
typedef struct AzVertexAttributeVecDestructorVariant_DefaultRust AzVertexAttributeVecDestructorVariant_DefaultRust;

struct AzVertexAttributeVecDestructorVariant_NoDestructor { AzVertexAttributeVecDestructorTag tag; };
typedef struct AzVertexAttributeVecDestructorVariant_NoDestructor AzVertexAttributeVecDestructorVariant_NoDestructor;

struct AzVertexAttributeVecDestructorVariant_External { AzVertexAttributeVecDestructorTag tag; AzVertexAttributeVecDestructorType payload; };
typedef struct AzVertexAttributeVecDestructorVariant_External AzVertexAttributeVecDestructorVariant_External;


union AzVertexAttributeVecDestructor {
    AzVertexAttributeVecDestructorVariant_DefaultRust DefaultRust;
    AzVertexAttributeVecDestructorVariant_NoDestructor NoDestructor;
    AzVertexAttributeVecDestructorVariant_External External;
};
typedef union AzVertexAttributeVecDestructor AzVertexAttributeVecDestructor;

enum AzSvgPathElementVecDestructorTag {
   AzSvgPathElementVecDestructorTag_DefaultRust,
   AzSvgPathElementVecDestructorTag_NoDestructor,
   AzSvgPathElementVecDestructorTag_External,
};
typedef enum AzSvgPathElementVecDestructorTag AzSvgPathElementVecDestructorTag;

struct AzSvgPathElementVecDestructorVariant_DefaultRust { AzSvgPathElementVecDestructorTag tag; };
typedef struct AzSvgPathElementVecDestructorVariant_DefaultRust AzSvgPathElementVecDestructorVariant_DefaultRust;

struct AzSvgPathElementVecDestructorVariant_NoDestructor { AzSvgPathElementVecDestructorTag tag; };
typedef struct AzSvgPathElementVecDestructorVariant_NoDestructor AzSvgPathElementVecDestructorVariant_NoDestructor;

struct AzSvgPathElementVecDestructorVariant_External { AzSvgPathElementVecDestructorTag tag; AzSvgPathElementVecDestructorType payload; };
typedef struct AzSvgPathElementVecDestructorVariant_External AzSvgPathElementVecDestructorVariant_External;


union AzSvgPathElementVecDestructor {
    AzSvgPathElementVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgPathElementVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgPathElementVecDestructorVariant_External External;
};
typedef union AzSvgPathElementVecDestructor AzSvgPathElementVecDestructor;

enum AzSvgVertexVecDestructorTag {
   AzSvgVertexVecDestructorTag_DefaultRust,
   AzSvgVertexVecDestructorTag_NoDestructor,
   AzSvgVertexVecDestructorTag_External,
};
typedef enum AzSvgVertexVecDestructorTag AzSvgVertexVecDestructorTag;

struct AzSvgVertexVecDestructorVariant_DefaultRust { AzSvgVertexVecDestructorTag tag; };
typedef struct AzSvgVertexVecDestructorVariant_DefaultRust AzSvgVertexVecDestructorVariant_DefaultRust;

struct AzSvgVertexVecDestructorVariant_NoDestructor { AzSvgVertexVecDestructorTag tag; };
typedef struct AzSvgVertexVecDestructorVariant_NoDestructor AzSvgVertexVecDestructorVariant_NoDestructor;

struct AzSvgVertexVecDestructorVariant_External { AzSvgVertexVecDestructorTag tag; AzSvgVertexVecDestructorType payload; };
typedef struct AzSvgVertexVecDestructorVariant_External AzSvgVertexVecDestructorVariant_External;


union AzSvgVertexVecDestructor {
    AzSvgVertexVecDestructorVariant_DefaultRust DefaultRust;
    AzSvgVertexVecDestructorVariant_NoDestructor NoDestructor;
    AzSvgVertexVecDestructorVariant_External External;
};
typedef union AzSvgVertexVecDestructor AzSvgVertexVecDestructor;

enum AzU32VecDestructorTag {
   AzU32VecDestructorTag_DefaultRust,
   AzU32VecDestructorTag_NoDestructor,
   AzU32VecDestructorTag_External,
};
typedef enum AzU32VecDestructorTag AzU32VecDestructorTag;

struct AzU32VecDestructorVariant_DefaultRust { AzU32VecDestructorTag tag; };
typedef struct AzU32VecDestructorVariant_DefaultRust AzU32VecDestructorVariant_DefaultRust;

struct AzU32VecDestructorVariant_NoDestructor { AzU32VecDestructorTag tag; };
typedef struct AzU32VecDestructorVariant_NoDestructor AzU32VecDestructorVariant_NoDestructor;

struct AzU32VecDestructorVariant_External { AzU32VecDestructorTag tag; AzU32VecDestructorType payload; };
typedef struct AzU32VecDestructorVariant_External AzU32VecDestructorVariant_External;


union AzU32VecDestructor {
    AzU32VecDestructorVariant_DefaultRust DefaultRust;
    AzU32VecDestructorVariant_NoDestructor NoDestructor;
    AzU32VecDestructorVariant_External External;
};
typedef union AzU32VecDestructor AzU32VecDestructor;

enum AzXWindowTypeVecDestructorTag {
   AzXWindowTypeVecDestructorTag_DefaultRust,
   AzXWindowTypeVecDestructorTag_NoDestructor,
   AzXWindowTypeVecDestructorTag_External,
};
typedef enum AzXWindowTypeVecDestructorTag AzXWindowTypeVecDestructorTag;

struct AzXWindowTypeVecDestructorVariant_DefaultRust { AzXWindowTypeVecDestructorTag tag; };
typedef struct AzXWindowTypeVecDestructorVariant_DefaultRust AzXWindowTypeVecDestructorVariant_DefaultRust;

struct AzXWindowTypeVecDestructorVariant_NoDestructor { AzXWindowTypeVecDestructorTag tag; };
typedef struct AzXWindowTypeVecDestructorVariant_NoDestructor AzXWindowTypeVecDestructorVariant_NoDestructor;

struct AzXWindowTypeVecDestructorVariant_External { AzXWindowTypeVecDestructorTag tag; AzXWindowTypeVecDestructorType payload; };
typedef struct AzXWindowTypeVecDestructorVariant_External AzXWindowTypeVecDestructorVariant_External;


union AzXWindowTypeVecDestructor {
    AzXWindowTypeVecDestructorVariant_DefaultRust DefaultRust;
    AzXWindowTypeVecDestructorVariant_NoDestructor NoDestructor;
    AzXWindowTypeVecDestructorVariant_External External;
};
typedef union AzXWindowTypeVecDestructor AzXWindowTypeVecDestructor;

enum AzVirtualKeyCodeVecDestructorTag {
   AzVirtualKeyCodeVecDestructorTag_DefaultRust,
   AzVirtualKeyCodeVecDestructorTag_NoDestructor,
   AzVirtualKeyCodeVecDestructorTag_External,
};
typedef enum AzVirtualKeyCodeVecDestructorTag AzVirtualKeyCodeVecDestructorTag;

struct AzVirtualKeyCodeVecDestructorVariant_DefaultRust { AzVirtualKeyCodeVecDestructorTag tag; };
typedef struct AzVirtualKeyCodeVecDestructorVariant_DefaultRust AzVirtualKeyCodeVecDestructorVariant_DefaultRust;

struct AzVirtualKeyCodeVecDestructorVariant_NoDestructor { AzVirtualKeyCodeVecDestructorTag tag; };
typedef struct AzVirtualKeyCodeVecDestructorVariant_NoDestructor AzVirtualKeyCodeVecDestructorVariant_NoDestructor;

struct AzVirtualKeyCodeVecDestructorVariant_External { AzVirtualKeyCodeVecDestructorTag tag; AzVirtualKeyCodeVecDestructorType payload; };
typedef struct AzVirtualKeyCodeVecDestructorVariant_External AzVirtualKeyCodeVecDestructorVariant_External;


union AzVirtualKeyCodeVecDestructor {
    AzVirtualKeyCodeVecDestructorVariant_DefaultRust DefaultRust;
    AzVirtualKeyCodeVecDestructorVariant_NoDestructor NoDestructor;
    AzVirtualKeyCodeVecDestructorVariant_External External;
};
typedef union AzVirtualKeyCodeVecDestructor AzVirtualKeyCodeVecDestructor;

enum AzCascadeInfoVecDestructorTag {
   AzCascadeInfoVecDestructorTag_DefaultRust,
   AzCascadeInfoVecDestructorTag_NoDestructor,
   AzCascadeInfoVecDestructorTag_External,
};
typedef enum AzCascadeInfoVecDestructorTag AzCascadeInfoVecDestructorTag;

struct AzCascadeInfoVecDestructorVariant_DefaultRust { AzCascadeInfoVecDestructorTag tag; };
typedef struct AzCascadeInfoVecDestructorVariant_DefaultRust AzCascadeInfoVecDestructorVariant_DefaultRust;

struct AzCascadeInfoVecDestructorVariant_NoDestructor { AzCascadeInfoVecDestructorTag tag; };
typedef struct AzCascadeInfoVecDestructorVariant_NoDestructor AzCascadeInfoVecDestructorVariant_NoDestructor;

struct AzCascadeInfoVecDestructorVariant_External { AzCascadeInfoVecDestructorTag tag; AzCascadeInfoVecDestructorType payload; };
typedef struct AzCascadeInfoVecDestructorVariant_External AzCascadeInfoVecDestructorVariant_External;


union AzCascadeInfoVecDestructor {
    AzCascadeInfoVecDestructorVariant_DefaultRust DefaultRust;
    AzCascadeInfoVecDestructorVariant_NoDestructor NoDestructor;
    AzCascadeInfoVecDestructorVariant_External External;
};
typedef union AzCascadeInfoVecDestructor AzCascadeInfoVecDestructor;

enum AzScanCodeVecDestructorTag {
   AzScanCodeVecDestructorTag_DefaultRust,
   AzScanCodeVecDestructorTag_NoDestructor,
   AzScanCodeVecDestructorTag_External,
};
typedef enum AzScanCodeVecDestructorTag AzScanCodeVecDestructorTag;

struct AzScanCodeVecDestructorVariant_DefaultRust { AzScanCodeVecDestructorTag tag; };
typedef struct AzScanCodeVecDestructorVariant_DefaultRust AzScanCodeVecDestructorVariant_DefaultRust;

struct AzScanCodeVecDestructorVariant_NoDestructor { AzScanCodeVecDestructorTag tag; };
typedef struct AzScanCodeVecDestructorVariant_NoDestructor AzScanCodeVecDestructorVariant_NoDestructor;

struct AzScanCodeVecDestructorVariant_External { AzScanCodeVecDestructorTag tag; AzScanCodeVecDestructorType payload; };
typedef struct AzScanCodeVecDestructorVariant_External AzScanCodeVecDestructorVariant_External;


union AzScanCodeVecDestructor {
    AzScanCodeVecDestructorVariant_DefaultRust DefaultRust;
    AzScanCodeVecDestructorVariant_NoDestructor NoDestructor;
    AzScanCodeVecDestructorVariant_External External;
};
typedef union AzScanCodeVecDestructor AzScanCodeVecDestructor;

enum AzCssDeclarationVecDestructorTag {
   AzCssDeclarationVecDestructorTag_DefaultRust,
   AzCssDeclarationVecDestructorTag_NoDestructor,
   AzCssDeclarationVecDestructorTag_External,
};
typedef enum AzCssDeclarationVecDestructorTag AzCssDeclarationVecDestructorTag;

struct AzCssDeclarationVecDestructorVariant_DefaultRust { AzCssDeclarationVecDestructorTag tag; };
typedef struct AzCssDeclarationVecDestructorVariant_DefaultRust AzCssDeclarationVecDestructorVariant_DefaultRust;

struct AzCssDeclarationVecDestructorVariant_NoDestructor { AzCssDeclarationVecDestructorTag tag; };
typedef struct AzCssDeclarationVecDestructorVariant_NoDestructor AzCssDeclarationVecDestructorVariant_NoDestructor;

struct AzCssDeclarationVecDestructorVariant_External { AzCssDeclarationVecDestructorTag tag; AzCssDeclarationVecDestructorType payload; };
typedef struct AzCssDeclarationVecDestructorVariant_External AzCssDeclarationVecDestructorVariant_External;


union AzCssDeclarationVecDestructor {
    AzCssDeclarationVecDestructorVariant_DefaultRust DefaultRust;
    AzCssDeclarationVecDestructorVariant_NoDestructor NoDestructor;
    AzCssDeclarationVecDestructorVariant_External External;
};
typedef union AzCssDeclarationVecDestructor AzCssDeclarationVecDestructor;

enum AzCssPathSelectorVecDestructorTag {
   AzCssPathSelectorVecDestructorTag_DefaultRust,
   AzCssPathSelectorVecDestructorTag_NoDestructor,
   AzCssPathSelectorVecDestructorTag_External,
};
typedef enum AzCssPathSelectorVecDestructorTag AzCssPathSelectorVecDestructorTag;

struct AzCssPathSelectorVecDestructorVariant_DefaultRust { AzCssPathSelectorVecDestructorTag tag; };
typedef struct AzCssPathSelectorVecDestructorVariant_DefaultRust AzCssPathSelectorVecDestructorVariant_DefaultRust;

struct AzCssPathSelectorVecDestructorVariant_NoDestructor { AzCssPathSelectorVecDestructorTag tag; };
typedef struct AzCssPathSelectorVecDestructorVariant_NoDestructor AzCssPathSelectorVecDestructorVariant_NoDestructor;

struct AzCssPathSelectorVecDestructorVariant_External { AzCssPathSelectorVecDestructorTag tag; AzCssPathSelectorVecDestructorType payload; };
typedef struct AzCssPathSelectorVecDestructorVariant_External AzCssPathSelectorVecDestructorVariant_External;


union AzCssPathSelectorVecDestructor {
    AzCssPathSelectorVecDestructorVariant_DefaultRust DefaultRust;
    AzCssPathSelectorVecDestructorVariant_NoDestructor NoDestructor;
    AzCssPathSelectorVecDestructorVariant_External External;
};
typedef union AzCssPathSelectorVecDestructor AzCssPathSelectorVecDestructor;

enum AzStylesheetVecDestructorTag {
   AzStylesheetVecDestructorTag_DefaultRust,
   AzStylesheetVecDestructorTag_NoDestructor,
   AzStylesheetVecDestructorTag_External,
};
typedef enum AzStylesheetVecDestructorTag AzStylesheetVecDestructorTag;

struct AzStylesheetVecDestructorVariant_DefaultRust { AzStylesheetVecDestructorTag tag; };
typedef struct AzStylesheetVecDestructorVariant_DefaultRust AzStylesheetVecDestructorVariant_DefaultRust;

struct AzStylesheetVecDestructorVariant_NoDestructor { AzStylesheetVecDestructorTag tag; };
typedef struct AzStylesheetVecDestructorVariant_NoDestructor AzStylesheetVecDestructorVariant_NoDestructor;

struct AzStylesheetVecDestructorVariant_External { AzStylesheetVecDestructorTag tag; AzStylesheetVecDestructorType payload; };
typedef struct AzStylesheetVecDestructorVariant_External AzStylesheetVecDestructorVariant_External;


union AzStylesheetVecDestructor {
    AzStylesheetVecDestructorVariant_DefaultRust DefaultRust;
    AzStylesheetVecDestructorVariant_NoDestructor NoDestructor;
    AzStylesheetVecDestructorVariant_External External;
};
typedef union AzStylesheetVecDestructor AzStylesheetVecDestructor;

enum AzCssRuleBlockVecDestructorTag {
   AzCssRuleBlockVecDestructorTag_DefaultRust,
   AzCssRuleBlockVecDestructorTag_NoDestructor,
   AzCssRuleBlockVecDestructorTag_External,
};
typedef enum AzCssRuleBlockVecDestructorTag AzCssRuleBlockVecDestructorTag;

struct AzCssRuleBlockVecDestructorVariant_DefaultRust { AzCssRuleBlockVecDestructorTag tag; };
typedef struct AzCssRuleBlockVecDestructorVariant_DefaultRust AzCssRuleBlockVecDestructorVariant_DefaultRust;

struct AzCssRuleBlockVecDestructorVariant_NoDestructor { AzCssRuleBlockVecDestructorTag tag; };
typedef struct AzCssRuleBlockVecDestructorVariant_NoDestructor AzCssRuleBlockVecDestructorVariant_NoDestructor;

struct AzCssRuleBlockVecDestructorVariant_External { AzCssRuleBlockVecDestructorTag tag; AzCssRuleBlockVecDestructorType payload; };
typedef struct AzCssRuleBlockVecDestructorVariant_External AzCssRuleBlockVecDestructorVariant_External;


union AzCssRuleBlockVecDestructor {
    AzCssRuleBlockVecDestructorVariant_DefaultRust DefaultRust;
    AzCssRuleBlockVecDestructorVariant_NoDestructor NoDestructor;
    AzCssRuleBlockVecDestructorVariant_External External;
};
typedef union AzCssRuleBlockVecDestructor AzCssRuleBlockVecDestructor;

enum AzU8VecDestructorTag {
   AzU8VecDestructorTag_DefaultRust,
   AzU8VecDestructorTag_NoDestructor,
   AzU8VecDestructorTag_External,
};
typedef enum AzU8VecDestructorTag AzU8VecDestructorTag;

struct AzU8VecDestructorVariant_DefaultRust { AzU8VecDestructorTag tag; };
typedef struct AzU8VecDestructorVariant_DefaultRust AzU8VecDestructorVariant_DefaultRust;

struct AzU8VecDestructorVariant_NoDestructor { AzU8VecDestructorTag tag; };
typedef struct AzU8VecDestructorVariant_NoDestructor AzU8VecDestructorVariant_NoDestructor;

struct AzU8VecDestructorVariant_External { AzU8VecDestructorTag tag; AzU8VecDestructorType payload; };
typedef struct AzU8VecDestructorVariant_External AzU8VecDestructorVariant_External;


union AzU8VecDestructor {
    AzU8VecDestructorVariant_DefaultRust DefaultRust;
    AzU8VecDestructorVariant_NoDestructor NoDestructor;
    AzU8VecDestructorVariant_External External;
};
typedef union AzU8VecDestructor AzU8VecDestructor;

enum AzCallbackDataVecDestructorTag {
   AzCallbackDataVecDestructorTag_DefaultRust,
   AzCallbackDataVecDestructorTag_NoDestructor,
   AzCallbackDataVecDestructorTag_External,
};
typedef enum AzCallbackDataVecDestructorTag AzCallbackDataVecDestructorTag;

struct AzCallbackDataVecDestructorVariant_DefaultRust { AzCallbackDataVecDestructorTag tag; };
typedef struct AzCallbackDataVecDestructorVariant_DefaultRust AzCallbackDataVecDestructorVariant_DefaultRust;

struct AzCallbackDataVecDestructorVariant_NoDestructor { AzCallbackDataVecDestructorTag tag; };
typedef struct AzCallbackDataVecDestructorVariant_NoDestructor AzCallbackDataVecDestructorVariant_NoDestructor;

struct AzCallbackDataVecDestructorVariant_External { AzCallbackDataVecDestructorTag tag; AzCallbackDataVecDestructorType payload; };
typedef struct AzCallbackDataVecDestructorVariant_External AzCallbackDataVecDestructorVariant_External;


union AzCallbackDataVecDestructor {
    AzCallbackDataVecDestructorVariant_DefaultRust DefaultRust;
    AzCallbackDataVecDestructorVariant_NoDestructor NoDestructor;
    AzCallbackDataVecDestructorVariant_External External;
};
typedef union AzCallbackDataVecDestructor AzCallbackDataVecDestructor;

enum AzDebugMessageVecDestructorTag {
   AzDebugMessageVecDestructorTag_DefaultRust,
   AzDebugMessageVecDestructorTag_NoDestructor,
   AzDebugMessageVecDestructorTag_External,
};
typedef enum AzDebugMessageVecDestructorTag AzDebugMessageVecDestructorTag;

struct AzDebugMessageVecDestructorVariant_DefaultRust { AzDebugMessageVecDestructorTag tag; };
typedef struct AzDebugMessageVecDestructorVariant_DefaultRust AzDebugMessageVecDestructorVariant_DefaultRust;

struct AzDebugMessageVecDestructorVariant_NoDestructor { AzDebugMessageVecDestructorTag tag; };
typedef struct AzDebugMessageVecDestructorVariant_NoDestructor AzDebugMessageVecDestructorVariant_NoDestructor;

struct AzDebugMessageVecDestructorVariant_External { AzDebugMessageVecDestructorTag tag; AzDebugMessageVecDestructorType payload; };
typedef struct AzDebugMessageVecDestructorVariant_External AzDebugMessageVecDestructorVariant_External;


union AzDebugMessageVecDestructor {
    AzDebugMessageVecDestructorVariant_DefaultRust DefaultRust;
    AzDebugMessageVecDestructorVariant_NoDestructor NoDestructor;
    AzDebugMessageVecDestructorVariant_External External;
};
typedef union AzDebugMessageVecDestructor AzDebugMessageVecDestructor;

enum AzGLuintVecDestructorTag {
   AzGLuintVecDestructorTag_DefaultRust,
   AzGLuintVecDestructorTag_NoDestructor,
   AzGLuintVecDestructorTag_External,
};
typedef enum AzGLuintVecDestructorTag AzGLuintVecDestructorTag;

struct AzGLuintVecDestructorVariant_DefaultRust { AzGLuintVecDestructorTag tag; };
typedef struct AzGLuintVecDestructorVariant_DefaultRust AzGLuintVecDestructorVariant_DefaultRust;

struct AzGLuintVecDestructorVariant_NoDestructor { AzGLuintVecDestructorTag tag; };
typedef struct AzGLuintVecDestructorVariant_NoDestructor AzGLuintVecDestructorVariant_NoDestructor;

struct AzGLuintVecDestructorVariant_External { AzGLuintVecDestructorTag tag; AzGLuintVecDestructorType payload; };
typedef struct AzGLuintVecDestructorVariant_External AzGLuintVecDestructorVariant_External;


union AzGLuintVecDestructor {
    AzGLuintVecDestructorVariant_DefaultRust DefaultRust;
    AzGLuintVecDestructorVariant_NoDestructor NoDestructor;
    AzGLuintVecDestructorVariant_External External;
};
typedef union AzGLuintVecDestructor AzGLuintVecDestructor;

enum AzGLintVecDestructorTag {
   AzGLintVecDestructorTag_DefaultRust,
   AzGLintVecDestructorTag_NoDestructor,
   AzGLintVecDestructorTag_External,
};
typedef enum AzGLintVecDestructorTag AzGLintVecDestructorTag;

struct AzGLintVecDestructorVariant_DefaultRust { AzGLintVecDestructorTag tag; };
typedef struct AzGLintVecDestructorVariant_DefaultRust AzGLintVecDestructorVariant_DefaultRust;

struct AzGLintVecDestructorVariant_NoDestructor { AzGLintVecDestructorTag tag; };
typedef struct AzGLintVecDestructorVariant_NoDestructor AzGLintVecDestructorVariant_NoDestructor;

struct AzGLintVecDestructorVariant_External { AzGLintVecDestructorTag tag; AzGLintVecDestructorType payload; };
typedef struct AzGLintVecDestructorVariant_External AzGLintVecDestructorVariant_External;


union AzGLintVecDestructor {
    AzGLintVecDestructorVariant_DefaultRust DefaultRust;
    AzGLintVecDestructorVariant_NoDestructor NoDestructor;
    AzGLintVecDestructorVariant_External External;
};
typedef union AzGLintVecDestructor AzGLintVecDestructor;

enum AzStringVecDestructorTag {
   AzStringVecDestructorTag_DefaultRust,
   AzStringVecDestructorTag_NoDestructor,
   AzStringVecDestructorTag_External,
};
typedef enum AzStringVecDestructorTag AzStringVecDestructorTag;

struct AzStringVecDestructorVariant_DefaultRust { AzStringVecDestructorTag tag; };
typedef struct AzStringVecDestructorVariant_DefaultRust AzStringVecDestructorVariant_DefaultRust;

struct AzStringVecDestructorVariant_NoDestructor { AzStringVecDestructorTag tag; };
typedef struct AzStringVecDestructorVariant_NoDestructor AzStringVecDestructorVariant_NoDestructor;

struct AzStringVecDestructorVariant_External { AzStringVecDestructorTag tag; AzStringVecDestructorType payload; };
typedef struct AzStringVecDestructorVariant_External AzStringVecDestructorVariant_External;


union AzStringVecDestructor {
    AzStringVecDestructorVariant_DefaultRust DefaultRust;
    AzStringVecDestructorVariant_NoDestructor NoDestructor;
    AzStringVecDestructorVariant_External External;
};
typedef union AzStringVecDestructor AzStringVecDestructor;

enum AzStringPairVecDestructorTag {
   AzStringPairVecDestructorTag_DefaultRust,
   AzStringPairVecDestructorTag_NoDestructor,
   AzStringPairVecDestructorTag_External,
};
typedef enum AzStringPairVecDestructorTag AzStringPairVecDestructorTag;

struct AzStringPairVecDestructorVariant_DefaultRust { AzStringPairVecDestructorTag tag; };
typedef struct AzStringPairVecDestructorVariant_DefaultRust AzStringPairVecDestructorVariant_DefaultRust;

struct AzStringPairVecDestructorVariant_NoDestructor { AzStringPairVecDestructorTag tag; };
typedef struct AzStringPairVecDestructorVariant_NoDestructor AzStringPairVecDestructorVariant_NoDestructor;

struct AzStringPairVecDestructorVariant_External { AzStringPairVecDestructorTag tag; AzStringPairVecDestructorType payload; };
typedef struct AzStringPairVecDestructorVariant_External AzStringPairVecDestructorVariant_External;


union AzStringPairVecDestructor {
    AzStringPairVecDestructorVariant_DefaultRust DefaultRust;
    AzStringPairVecDestructorVariant_NoDestructor NoDestructor;
    AzStringPairVecDestructorVariant_External External;
};
typedef union AzStringPairVecDestructor AzStringPairVecDestructor;

enum AzLinearColorStopVecDestructorTag {
   AzLinearColorStopVecDestructorTag_DefaultRust,
   AzLinearColorStopVecDestructorTag_NoDestructor,
   AzLinearColorStopVecDestructorTag_External,
};
typedef enum AzLinearColorStopVecDestructorTag AzLinearColorStopVecDestructorTag;

struct AzLinearColorStopVecDestructorVariant_DefaultRust { AzLinearColorStopVecDestructorTag tag; };
typedef struct AzLinearColorStopVecDestructorVariant_DefaultRust AzLinearColorStopVecDestructorVariant_DefaultRust;

struct AzLinearColorStopVecDestructorVariant_NoDestructor { AzLinearColorStopVecDestructorTag tag; };
typedef struct AzLinearColorStopVecDestructorVariant_NoDestructor AzLinearColorStopVecDestructorVariant_NoDestructor;

struct AzLinearColorStopVecDestructorVariant_External { AzLinearColorStopVecDestructorTag tag; AzLinearColorStopVecDestructorType payload; };
typedef struct AzLinearColorStopVecDestructorVariant_External AzLinearColorStopVecDestructorVariant_External;


union AzLinearColorStopVecDestructor {
    AzLinearColorStopVecDestructorVariant_DefaultRust DefaultRust;
    AzLinearColorStopVecDestructorVariant_NoDestructor NoDestructor;
    AzLinearColorStopVecDestructorVariant_External External;
};
typedef union AzLinearColorStopVecDestructor AzLinearColorStopVecDestructor;

enum AzRadialColorStopVecDestructorTag {
   AzRadialColorStopVecDestructorTag_DefaultRust,
   AzRadialColorStopVecDestructorTag_NoDestructor,
   AzRadialColorStopVecDestructorTag_External,
};
typedef enum AzRadialColorStopVecDestructorTag AzRadialColorStopVecDestructorTag;

struct AzRadialColorStopVecDestructorVariant_DefaultRust { AzRadialColorStopVecDestructorTag tag; };
typedef struct AzRadialColorStopVecDestructorVariant_DefaultRust AzRadialColorStopVecDestructorVariant_DefaultRust;

struct AzRadialColorStopVecDestructorVariant_NoDestructor { AzRadialColorStopVecDestructorTag tag; };
typedef struct AzRadialColorStopVecDestructorVariant_NoDestructor AzRadialColorStopVecDestructorVariant_NoDestructor;

struct AzRadialColorStopVecDestructorVariant_External { AzRadialColorStopVecDestructorTag tag; AzRadialColorStopVecDestructorType payload; };
typedef struct AzRadialColorStopVecDestructorVariant_External AzRadialColorStopVecDestructorVariant_External;


union AzRadialColorStopVecDestructor {
    AzRadialColorStopVecDestructorVariant_DefaultRust DefaultRust;
    AzRadialColorStopVecDestructorVariant_NoDestructor NoDestructor;
    AzRadialColorStopVecDestructorVariant_External External;
};
typedef union AzRadialColorStopVecDestructor AzRadialColorStopVecDestructor;

enum AzNodeIdVecDestructorTag {
   AzNodeIdVecDestructorTag_DefaultRust,
   AzNodeIdVecDestructorTag_NoDestructor,
   AzNodeIdVecDestructorTag_External,
};
typedef enum AzNodeIdVecDestructorTag AzNodeIdVecDestructorTag;

struct AzNodeIdVecDestructorVariant_DefaultRust { AzNodeIdVecDestructorTag tag; };
typedef struct AzNodeIdVecDestructorVariant_DefaultRust AzNodeIdVecDestructorVariant_DefaultRust;

struct AzNodeIdVecDestructorVariant_NoDestructor { AzNodeIdVecDestructorTag tag; };
typedef struct AzNodeIdVecDestructorVariant_NoDestructor AzNodeIdVecDestructorVariant_NoDestructor;

struct AzNodeIdVecDestructorVariant_External { AzNodeIdVecDestructorTag tag; AzNodeIdVecDestructorType payload; };
typedef struct AzNodeIdVecDestructorVariant_External AzNodeIdVecDestructorVariant_External;


union AzNodeIdVecDestructor {
    AzNodeIdVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeIdVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeIdVecDestructorVariant_External External;
};
typedef union AzNodeIdVecDestructor AzNodeIdVecDestructor;

enum AzNodeVecDestructorTag {
   AzNodeVecDestructorTag_DefaultRust,
   AzNodeVecDestructorTag_NoDestructor,
   AzNodeVecDestructorTag_External,
};
typedef enum AzNodeVecDestructorTag AzNodeVecDestructorTag;

struct AzNodeVecDestructorVariant_DefaultRust { AzNodeVecDestructorTag tag; };
typedef struct AzNodeVecDestructorVariant_DefaultRust AzNodeVecDestructorVariant_DefaultRust;

struct AzNodeVecDestructorVariant_NoDestructor { AzNodeVecDestructorTag tag; };
typedef struct AzNodeVecDestructorVariant_NoDestructor AzNodeVecDestructorVariant_NoDestructor;

struct AzNodeVecDestructorVariant_External { AzNodeVecDestructorTag tag; AzNodeVecDestructorType payload; };
typedef struct AzNodeVecDestructorVariant_External AzNodeVecDestructorVariant_External;


union AzNodeVecDestructor {
    AzNodeVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeVecDestructorVariant_External External;
};
typedef union AzNodeVecDestructor AzNodeVecDestructor;

enum AzStyledNodeVecDestructorTag {
   AzStyledNodeVecDestructorTag_DefaultRust,
   AzStyledNodeVecDestructorTag_NoDestructor,
   AzStyledNodeVecDestructorTag_External,
};
typedef enum AzStyledNodeVecDestructorTag AzStyledNodeVecDestructorTag;

struct AzStyledNodeVecDestructorVariant_DefaultRust { AzStyledNodeVecDestructorTag tag; };
typedef struct AzStyledNodeVecDestructorVariant_DefaultRust AzStyledNodeVecDestructorVariant_DefaultRust;

struct AzStyledNodeVecDestructorVariant_NoDestructor { AzStyledNodeVecDestructorTag tag; };
typedef struct AzStyledNodeVecDestructorVariant_NoDestructor AzStyledNodeVecDestructorVariant_NoDestructor;

struct AzStyledNodeVecDestructorVariant_External { AzStyledNodeVecDestructorTag tag; AzStyledNodeVecDestructorType payload; };
typedef struct AzStyledNodeVecDestructorVariant_External AzStyledNodeVecDestructorVariant_External;


union AzStyledNodeVecDestructor {
    AzStyledNodeVecDestructorVariant_DefaultRust DefaultRust;
    AzStyledNodeVecDestructorVariant_NoDestructor NoDestructor;
    AzStyledNodeVecDestructorVariant_External External;
};
typedef union AzStyledNodeVecDestructor AzStyledNodeVecDestructor;

enum AzTagIdsToNodeIdsMappingVecDestructorTag {
   AzTagIdsToNodeIdsMappingVecDestructorTag_DefaultRust,
   AzTagIdsToNodeIdsMappingVecDestructorTag_NoDestructor,
   AzTagIdsToNodeIdsMappingVecDestructorTag_External,
};
typedef enum AzTagIdsToNodeIdsMappingVecDestructorTag AzTagIdsToNodeIdsMappingVecDestructorTag;

struct AzTagIdsToNodeIdsMappingVecDestructorVariant_DefaultRust { AzTagIdsToNodeIdsMappingVecDestructorTag tag; };
typedef struct AzTagIdsToNodeIdsMappingVecDestructorVariant_DefaultRust AzTagIdsToNodeIdsMappingVecDestructorVariant_DefaultRust;

struct AzTagIdsToNodeIdsMappingVecDestructorVariant_NoDestructor { AzTagIdsToNodeIdsMappingVecDestructorTag tag; };
typedef struct AzTagIdsToNodeIdsMappingVecDestructorVariant_NoDestructor AzTagIdsToNodeIdsMappingVecDestructorVariant_NoDestructor;

struct AzTagIdsToNodeIdsMappingVecDestructorVariant_External { AzTagIdsToNodeIdsMappingVecDestructorTag tag; AzTagIdsToNodeIdsMappingVecDestructorType payload; };
typedef struct AzTagIdsToNodeIdsMappingVecDestructorVariant_External AzTagIdsToNodeIdsMappingVecDestructorVariant_External;


union AzTagIdsToNodeIdsMappingVecDestructor {
    AzTagIdsToNodeIdsMappingVecDestructorVariant_DefaultRust DefaultRust;
    AzTagIdsToNodeIdsMappingVecDestructorVariant_NoDestructor NoDestructor;
    AzTagIdsToNodeIdsMappingVecDestructorVariant_External External;
};
typedef union AzTagIdsToNodeIdsMappingVecDestructor AzTagIdsToNodeIdsMappingVecDestructor;

enum AzParentWithNodeDepthVecDestructorTag {
   AzParentWithNodeDepthVecDestructorTag_DefaultRust,
   AzParentWithNodeDepthVecDestructorTag_NoDestructor,
   AzParentWithNodeDepthVecDestructorTag_External,
};
typedef enum AzParentWithNodeDepthVecDestructorTag AzParentWithNodeDepthVecDestructorTag;

struct AzParentWithNodeDepthVecDestructorVariant_DefaultRust { AzParentWithNodeDepthVecDestructorTag tag; };
typedef struct AzParentWithNodeDepthVecDestructorVariant_DefaultRust AzParentWithNodeDepthVecDestructorVariant_DefaultRust;

struct AzParentWithNodeDepthVecDestructorVariant_NoDestructor { AzParentWithNodeDepthVecDestructorTag tag; };
typedef struct AzParentWithNodeDepthVecDestructorVariant_NoDestructor AzParentWithNodeDepthVecDestructorVariant_NoDestructor;

struct AzParentWithNodeDepthVecDestructorVariant_External { AzParentWithNodeDepthVecDestructorTag tag; AzParentWithNodeDepthVecDestructorType payload; };
typedef struct AzParentWithNodeDepthVecDestructorVariant_External AzParentWithNodeDepthVecDestructorVariant_External;


union AzParentWithNodeDepthVecDestructor {
    AzParentWithNodeDepthVecDestructorVariant_DefaultRust DefaultRust;
    AzParentWithNodeDepthVecDestructorVariant_NoDestructor NoDestructor;
    AzParentWithNodeDepthVecDestructorVariant_External External;
};
typedef union AzParentWithNodeDepthVecDestructor AzParentWithNodeDepthVecDestructor;

enum AzNodeDataVecDestructorTag {
   AzNodeDataVecDestructorTag_DefaultRust,
   AzNodeDataVecDestructorTag_NoDestructor,
   AzNodeDataVecDestructorTag_External,
};
typedef enum AzNodeDataVecDestructorTag AzNodeDataVecDestructorTag;

struct AzNodeDataVecDestructorVariant_DefaultRust { AzNodeDataVecDestructorTag tag; };
typedef struct AzNodeDataVecDestructorVariant_DefaultRust AzNodeDataVecDestructorVariant_DefaultRust;

struct AzNodeDataVecDestructorVariant_NoDestructor { AzNodeDataVecDestructorTag tag; };
typedef struct AzNodeDataVecDestructorVariant_NoDestructor AzNodeDataVecDestructorVariant_NoDestructor;

struct AzNodeDataVecDestructorVariant_External { AzNodeDataVecDestructorTag tag; AzNodeDataVecDestructorType payload; };
typedef struct AzNodeDataVecDestructorVariant_External AzNodeDataVecDestructorVariant_External;


union AzNodeDataVecDestructor {
    AzNodeDataVecDestructorVariant_DefaultRust DefaultRust;
    AzNodeDataVecDestructorVariant_NoDestructor NoDestructor;
    AzNodeDataVecDestructorVariant_External External;
};
typedef union AzNodeDataVecDestructor AzNodeDataVecDestructor;

enum AzOptionHwndHandleTag {
   AzOptionHwndHandleTag_None,
   AzOptionHwndHandleTag_Some,
};
typedef enum AzOptionHwndHandleTag AzOptionHwndHandleTag;

struct AzOptionHwndHandleVariant_None { AzOptionHwndHandleTag tag; };
typedef struct AzOptionHwndHandleVariant_None AzOptionHwndHandleVariant_None;

struct AzOptionHwndHandleVariant_Some { AzOptionHwndHandleTag tag; void* restrict payload; };
typedef struct AzOptionHwndHandleVariant_Some AzOptionHwndHandleVariant_Some;


union AzOptionHwndHandle {
    AzOptionHwndHandleVariant_None None;
    AzOptionHwndHandleVariant_Some Some;
};
typedef union AzOptionHwndHandle AzOptionHwndHandle;

enum AzOptionX11VisualTag {
   AzOptionX11VisualTag_None,
   AzOptionX11VisualTag_Some,
};
typedef enum AzOptionX11VisualTag AzOptionX11VisualTag;

struct AzOptionX11VisualVariant_None { AzOptionX11VisualTag tag; };
typedef struct AzOptionX11VisualVariant_None AzOptionX11VisualVariant_None;

struct AzOptionX11VisualVariant_Some { AzOptionX11VisualTag tag; void* const payload; };
typedef struct AzOptionX11VisualVariant_Some AzOptionX11VisualVariant_Some;


union AzOptionX11Visual {
    AzOptionX11VisualVariant_None None;
    AzOptionX11VisualVariant_Some Some;
};
typedef union AzOptionX11Visual AzOptionX11Visual;

enum AzOptionI32Tag {
   AzOptionI32Tag_None,
   AzOptionI32Tag_Some,
};
typedef enum AzOptionI32Tag AzOptionI32Tag;

struct AzOptionI32Variant_None { AzOptionI32Tag tag; };
typedef struct AzOptionI32Variant_None AzOptionI32Variant_None;

struct AzOptionI32Variant_Some { AzOptionI32Tag tag; int32_t payload; };
typedef struct AzOptionI32Variant_Some AzOptionI32Variant_Some;


union AzOptionI32 {
    AzOptionI32Variant_None None;
    AzOptionI32Variant_Some Some;
};
typedef union AzOptionI32 AzOptionI32;

enum AzOptionF32Tag {
   AzOptionF32Tag_None,
   AzOptionF32Tag_Some,
};
typedef enum AzOptionF32Tag AzOptionF32Tag;

struct AzOptionF32Variant_None { AzOptionF32Tag tag; };
typedef struct AzOptionF32Variant_None AzOptionF32Variant_None;

struct AzOptionF32Variant_Some { AzOptionF32Tag tag; float payload; };
typedef struct AzOptionF32Variant_Some AzOptionF32Variant_Some;


union AzOptionF32 {
    AzOptionF32Variant_None None;
    AzOptionF32Variant_Some Some;
};
typedef union AzOptionF32 AzOptionF32;

enum AzOptionCharTag {
   AzOptionCharTag_None,
   AzOptionCharTag_Some,
};
typedef enum AzOptionCharTag AzOptionCharTag;

struct AzOptionCharVariant_None { AzOptionCharTag tag; };
typedef struct AzOptionCharVariant_None AzOptionCharVariant_None;

struct AzOptionCharVariant_Some { AzOptionCharTag tag; uint32_t payload; };
typedef struct AzOptionCharVariant_Some AzOptionCharVariant_Some;


union AzOptionChar {
    AzOptionCharVariant_None None;
    AzOptionCharVariant_Some Some;
};
typedef union AzOptionChar AzOptionChar;

enum AzOptionUsizeTag {
   AzOptionUsizeTag_None,
   AzOptionUsizeTag_Some,
};
typedef enum AzOptionUsizeTag AzOptionUsizeTag;

struct AzOptionUsizeVariant_None { AzOptionUsizeTag tag; };
typedef struct AzOptionUsizeVariant_None AzOptionUsizeVariant_None;

struct AzOptionUsizeVariant_Some { AzOptionUsizeTag tag; size_t payload; };
typedef struct AzOptionUsizeVariant_Some AzOptionUsizeVariant_Some;


union AzOptionUsize {
    AzOptionUsizeVariant_None None;
    AzOptionUsizeVariant_Some Some;
};
typedef union AzOptionUsize AzOptionUsize;

struct AzSvgParseErrorPosition {
    uint32_t row;
    uint32_t col;
};
typedef struct AzSvgParseErrorPosition AzSvgParseErrorPosition;

struct AzInstantPtrCloneFn {
    AzInstantPtrCloneFnType cb;
};
typedef struct AzInstantPtrCloneFn AzInstantPtrCloneFn;

struct AzInstantPtrDestructorFn {
    AzInstantPtrDestructorFnType cb;
};
typedef struct AzInstantPtrDestructorFn AzInstantPtrDestructorFn;

struct AzSystemTick {
    uint64_t tick_counter;
};
typedef struct AzSystemTick AzSystemTick;

struct AzSystemTimeDiff {
    uint64_t secs;
    uint32_t nanos;
};
typedef struct AzSystemTimeDiff AzSystemTimeDiff;

struct AzSystemTickDiff {
    uint64_t tick_diff;
};
typedef struct AzSystemTickDiff AzSystemTickDiff;

struct AzRendererOptions {
    AzVsync vsync;
    AzSrgb srgb;
    AzHwAcceleration hw_accel;
};
typedef struct AzRendererOptions AzRendererOptions;

struct AzLayoutRect {
    AzLayoutPoint origin;
    AzLayoutSize size;
};
typedef struct AzLayoutRect AzLayoutRect;

enum AzRawWindowHandleTag {
   AzRawWindowHandleTag_IOS,
   AzRawWindowHandleTag_MacOS,
   AzRawWindowHandleTag_Xlib,
   AzRawWindowHandleTag_Xcb,
   AzRawWindowHandleTag_Wayland,
   AzRawWindowHandleTag_Windows,
   AzRawWindowHandleTag_Web,
   AzRawWindowHandleTag_Android,
   AzRawWindowHandleTag_Unsupported,
};
typedef enum AzRawWindowHandleTag AzRawWindowHandleTag;

struct AzRawWindowHandleVariant_IOS { AzRawWindowHandleTag tag; AzIOSHandle payload; };
typedef struct AzRawWindowHandleVariant_IOS AzRawWindowHandleVariant_IOS;

struct AzRawWindowHandleVariant_MacOS { AzRawWindowHandleTag tag; AzMacOSHandle payload; };
typedef struct AzRawWindowHandleVariant_MacOS AzRawWindowHandleVariant_MacOS;

struct AzRawWindowHandleVariant_Xlib { AzRawWindowHandleTag tag; AzXlibHandle payload; };
typedef struct AzRawWindowHandleVariant_Xlib AzRawWindowHandleVariant_Xlib;

struct AzRawWindowHandleVariant_Xcb { AzRawWindowHandleTag tag; AzXcbHandle payload; };
typedef struct AzRawWindowHandleVariant_Xcb AzRawWindowHandleVariant_Xcb;

struct AzRawWindowHandleVariant_Wayland { AzRawWindowHandleTag tag; AzWaylandHandle payload; };
typedef struct AzRawWindowHandleVariant_Wayland AzRawWindowHandleVariant_Wayland;

struct AzRawWindowHandleVariant_Windows { AzRawWindowHandleTag tag; AzWindowsHandle payload; };
typedef struct AzRawWindowHandleVariant_Windows AzRawWindowHandleVariant_Windows;

struct AzRawWindowHandleVariant_Web { AzRawWindowHandleTag tag; AzWebHandle payload; };
typedef struct AzRawWindowHandleVariant_Web AzRawWindowHandleVariant_Web;

struct AzRawWindowHandleVariant_Android { AzRawWindowHandleTag tag; AzAndroidHandle payload; };
typedef struct AzRawWindowHandleVariant_Android AzRawWindowHandleVariant_Android;

struct AzRawWindowHandleVariant_Unsupported { AzRawWindowHandleTag tag; };
typedef struct AzRawWindowHandleVariant_Unsupported AzRawWindowHandleVariant_Unsupported;


union AzRawWindowHandle {
    AzRawWindowHandleVariant_IOS IOS;
    AzRawWindowHandleVariant_MacOS MacOS;
    AzRawWindowHandleVariant_Xlib Xlib;
    AzRawWindowHandleVariant_Xcb Xcb;
    AzRawWindowHandleVariant_Wayland Wayland;
    AzRawWindowHandleVariant_Windows Windows;
    AzRawWindowHandleVariant_Web Web;
    AzRawWindowHandleVariant_Android Android;
    AzRawWindowHandleVariant_Unsupported Unsupported;
};
typedef union AzRawWindowHandle AzRawWindowHandle;

struct AzLogicalRect {
    AzLogicalPosition origin;
    AzLogicalSize size;
};
typedef struct AzLogicalRect AzLogicalRect;

enum AzAcceleratorKeyTag {
   AzAcceleratorKeyTag_Ctrl,
   AzAcceleratorKeyTag_Alt,
   AzAcceleratorKeyTag_Shift,
   AzAcceleratorKeyTag_Key,
};
typedef enum AzAcceleratorKeyTag AzAcceleratorKeyTag;

struct AzAcceleratorKeyVariant_Ctrl { AzAcceleratorKeyTag tag; };
typedef struct AzAcceleratorKeyVariant_Ctrl AzAcceleratorKeyVariant_Ctrl;

struct AzAcceleratorKeyVariant_Alt { AzAcceleratorKeyTag tag; };
typedef struct AzAcceleratorKeyVariant_Alt AzAcceleratorKeyVariant_Alt;

struct AzAcceleratorKeyVariant_Shift { AzAcceleratorKeyTag tag; };
typedef struct AzAcceleratorKeyVariant_Shift AzAcceleratorKeyVariant_Shift;

struct AzAcceleratorKeyVariant_Key { AzAcceleratorKeyTag tag; AzVirtualKeyCode payload; };
typedef struct AzAcceleratorKeyVariant_Key AzAcceleratorKeyVariant_Key;


union AzAcceleratorKey {
    AzAcceleratorKeyVariant_Ctrl Ctrl;
    AzAcceleratorKeyVariant_Alt Alt;
    AzAcceleratorKeyVariant_Shift Shift;
    AzAcceleratorKeyVariant_Key Key;
};
typedef union AzAcceleratorKey AzAcceleratorKey;

enum AzCursorPositionTag {
   AzCursorPositionTag_OutOfWindow,
   AzCursorPositionTag_Uninitialized,
   AzCursorPositionTag_InWindow,
};
typedef enum AzCursorPositionTag AzCursorPositionTag;

struct AzCursorPositionVariant_OutOfWindow { AzCursorPositionTag tag; };
typedef struct AzCursorPositionVariant_OutOfWindow AzCursorPositionVariant_OutOfWindow;

struct AzCursorPositionVariant_Uninitialized { AzCursorPositionTag tag; };
typedef struct AzCursorPositionVariant_Uninitialized AzCursorPositionVariant_Uninitialized;

struct AzCursorPositionVariant_InWindow { AzCursorPositionTag tag; AzLogicalPosition payload; };
typedef struct AzCursorPositionVariant_InWindow AzCursorPositionVariant_InWindow;


union AzCursorPosition {
    AzCursorPositionVariant_OutOfWindow OutOfWindow;
    AzCursorPositionVariant_Uninitialized Uninitialized;
    AzCursorPositionVariant_InWindow InWindow;
};
typedef union AzCursorPosition AzCursorPosition;

enum AzWindowPositionTag {
   AzWindowPositionTag_Uninitialized,
   AzWindowPositionTag_Initialized,
};
typedef enum AzWindowPositionTag AzWindowPositionTag;

struct AzWindowPositionVariant_Uninitialized { AzWindowPositionTag tag; };
typedef struct AzWindowPositionVariant_Uninitialized AzWindowPositionVariant_Uninitialized;

struct AzWindowPositionVariant_Initialized { AzWindowPositionTag tag; AzPhysicalPositionI32 payload; };
typedef struct AzWindowPositionVariant_Initialized AzWindowPositionVariant_Initialized;


union AzWindowPosition {
    AzWindowPositionVariant_Uninitialized Uninitialized;
    AzWindowPositionVariant_Initialized Initialized;
};
typedef union AzWindowPosition AzWindowPosition;

enum AzImePositionTag {
   AzImePositionTag_Uninitialized,
   AzImePositionTag_Initialized,
};
typedef enum AzImePositionTag AzImePositionTag;

struct AzImePositionVariant_Uninitialized { AzImePositionTag tag; };
typedef struct AzImePositionVariant_Uninitialized AzImePositionVariant_Uninitialized;

struct AzImePositionVariant_Initialized { AzImePositionTag tag; AzLogicalPosition payload; };
typedef struct AzImePositionVariant_Initialized AzImePositionVariant_Initialized;


union AzImePosition {
    AzImePositionVariant_Uninitialized Uninitialized;
    AzImePositionVariant_Initialized Initialized;
};
typedef union AzImePosition AzImePosition;

struct AzVideoMode {
    AzLayoutSize size;
    uint16_t bit_depth;
    uint16_t refresh_rate;
};
typedef struct AzVideoMode AzVideoMode;

struct AzDomNodeId {
    AzDomId dom;
    AzNodeId node;
};
typedef struct AzDomNodeId AzDomNodeId;

struct AzHidpiAdjustedBounds {
    AzLogicalSize logical_size;
    float hidpi_factor;
};
typedef struct AzHidpiAdjustedBounds AzHidpiAdjustedBounds;

struct AzIFrameCallbackInfo {
    void* const resources;
    AzHidpiAdjustedBounds bounds;
};
typedef struct AzIFrameCallbackInfo AzIFrameCallbackInfo;

struct AzTimerCallbackReturn {
    AzUpdateScreen should_update;
    AzTerminateTimer should_terminate;
};
typedef struct AzTimerCallbackReturn AzTimerCallbackReturn;

struct AzSystemCallbacks {
    AzCreateThreadFn create_thread_fn;
    AzGetSystemTimeFn get_system_time_fn;
};
typedef struct AzSystemCallbacks AzSystemCallbacks;

enum AzNotEventFilterTag {
   AzNotEventFilterTag_Hover,
   AzNotEventFilterTag_Focus,
};
typedef enum AzNotEventFilterTag AzNotEventFilterTag;

struct AzNotEventFilterVariant_Hover { AzNotEventFilterTag tag; AzHoverEventFilter payload; };
typedef struct AzNotEventFilterVariant_Hover AzNotEventFilterVariant_Hover;

struct AzNotEventFilterVariant_Focus { AzNotEventFilterTag tag; AzFocusEventFilter payload; };
typedef struct AzNotEventFilterVariant_Focus AzNotEventFilterVariant_Focus;


union AzNotEventFilter {
    AzNotEventFilterVariant_Hover Hover;
    AzNotEventFilterVariant_Focus Focus;
};
typedef union AzNotEventFilter AzNotEventFilter;

enum AzCssNthChildSelectorTag {
   AzCssNthChildSelectorTag_Number,
   AzCssNthChildSelectorTag_Even,
   AzCssNthChildSelectorTag_Odd,
   AzCssNthChildSelectorTag_Pattern,
};
typedef enum AzCssNthChildSelectorTag AzCssNthChildSelectorTag;

struct AzCssNthChildSelectorVariant_Number { AzCssNthChildSelectorTag tag; uint32_t payload; };
typedef struct AzCssNthChildSelectorVariant_Number AzCssNthChildSelectorVariant_Number;

struct AzCssNthChildSelectorVariant_Even { AzCssNthChildSelectorTag tag; };
typedef struct AzCssNthChildSelectorVariant_Even AzCssNthChildSelectorVariant_Even;

struct AzCssNthChildSelectorVariant_Odd { AzCssNthChildSelectorTag tag; };
typedef struct AzCssNthChildSelectorVariant_Odd AzCssNthChildSelectorVariant_Odd;

struct AzCssNthChildSelectorVariant_Pattern { AzCssNthChildSelectorTag tag; AzCssNthChildPattern payload; };
typedef struct AzCssNthChildSelectorVariant_Pattern AzCssNthChildSelectorVariant_Pattern;


union AzCssNthChildSelector {
    AzCssNthChildSelectorVariant_Number Number;
    AzCssNthChildSelectorVariant_Even Even;
    AzCssNthChildSelectorVariant_Odd Odd;
    AzCssNthChildSelectorVariant_Pattern Pattern;
};
typedef union AzCssNthChildSelector AzCssNthChildSelector;

struct AzPixelValue {
    AzSizeMetric metric;
    AzFloatValue number;
};
typedef struct AzPixelValue AzPixelValue;

struct AzPixelValueNoPercent {
    AzPixelValue inner;
};
typedef struct AzPixelValueNoPercent AzPixelValueNoPercent;

struct AzStyleBoxShadow {
    AzPixelValueNoPercent offset[2];
    AzColorU color;
    AzPixelValueNoPercent blur_radius;
    AzPixelValueNoPercent spread_radius;
    AzBoxShadowClipMode clip_mode;
};
typedef struct AzStyleBoxShadow AzStyleBoxShadow;

struct AzLayoutBottom {
    AzPixelValue inner;
};
typedef struct AzLayoutBottom AzLayoutBottom;

struct AzLayoutFlexGrow {
    AzFloatValue inner;
};
typedef struct AzLayoutFlexGrow AzLayoutFlexGrow;

struct AzLayoutFlexShrink {
    AzFloatValue inner;
};
typedef struct AzLayoutFlexShrink AzLayoutFlexShrink;

struct AzLayoutHeight {
    AzPixelValue inner;
};
typedef struct AzLayoutHeight AzLayoutHeight;

struct AzLayoutLeft {
    AzPixelValue inner;
};
typedef struct AzLayoutLeft AzLayoutLeft;

struct AzLayoutMarginBottom {
    AzPixelValue inner;
};
typedef struct AzLayoutMarginBottom AzLayoutMarginBottom;

struct AzLayoutMarginLeft {
    AzPixelValue inner;
};
typedef struct AzLayoutMarginLeft AzLayoutMarginLeft;

struct AzLayoutMarginRight {
    AzPixelValue inner;
};
typedef struct AzLayoutMarginRight AzLayoutMarginRight;

struct AzLayoutMarginTop {
    AzPixelValue inner;
};
typedef struct AzLayoutMarginTop AzLayoutMarginTop;

struct AzLayoutMaxHeight {
    AzPixelValue inner;
};
typedef struct AzLayoutMaxHeight AzLayoutMaxHeight;

struct AzLayoutMaxWidth {
    AzPixelValue inner;
};
typedef struct AzLayoutMaxWidth AzLayoutMaxWidth;

struct AzLayoutMinHeight {
    AzPixelValue inner;
};
typedef struct AzLayoutMinHeight AzLayoutMinHeight;

struct AzLayoutMinWidth {
    AzPixelValue inner;
};
typedef struct AzLayoutMinWidth AzLayoutMinWidth;

struct AzLayoutPaddingBottom {
    AzPixelValue inner;
};
typedef struct AzLayoutPaddingBottom AzLayoutPaddingBottom;

struct AzLayoutPaddingLeft {
    AzPixelValue inner;
};
typedef struct AzLayoutPaddingLeft AzLayoutPaddingLeft;

struct AzLayoutPaddingRight {
    AzPixelValue inner;
};
typedef struct AzLayoutPaddingRight AzLayoutPaddingRight;

struct AzLayoutPaddingTop {
    AzPixelValue inner;
};
typedef struct AzLayoutPaddingTop AzLayoutPaddingTop;

struct AzLayoutRight {
    AzPixelValue inner;
};
typedef struct AzLayoutRight AzLayoutRight;

struct AzLayoutTop {
    AzPixelValue inner;
};
typedef struct AzLayoutTop AzLayoutTop;

struct AzLayoutWidth {
    AzPixelValue inner;
};
typedef struct AzLayoutWidth AzLayoutWidth;

struct AzPercentageValue {
    AzFloatValue number;
};
typedef struct AzPercentageValue AzPercentageValue;

struct AzAngleValue {
    AzAngleMetric metric;
    AzFloatValue number;
};
typedef struct AzAngleValue AzAngleValue;

struct AzDirectionCorners {
    AzDirectionCorner from;
    AzDirectionCorner to;
};
typedef struct AzDirectionCorners AzDirectionCorners;

enum AzDirectionTag {
   AzDirectionTag_Angle,
   AzDirectionTag_FromTo,
};
typedef enum AzDirectionTag AzDirectionTag;

struct AzDirectionVariant_Angle { AzDirectionTag tag; AzAngleValue payload; };
typedef struct AzDirectionVariant_Angle AzDirectionVariant_Angle;

struct AzDirectionVariant_FromTo { AzDirectionTag tag; AzDirectionCorners payload; };
typedef struct AzDirectionVariant_FromTo AzDirectionVariant_FromTo;


union AzDirection {
    AzDirectionVariant_Angle Angle;
    AzDirectionVariant_FromTo FromTo;
};
typedef union AzDirection AzDirection;

enum AzBackgroundPositionHorizontalTag {
   AzBackgroundPositionHorizontalTag_Left,
   AzBackgroundPositionHorizontalTag_Center,
   AzBackgroundPositionHorizontalTag_Right,
   AzBackgroundPositionHorizontalTag_Exact,
};
typedef enum AzBackgroundPositionHorizontalTag AzBackgroundPositionHorizontalTag;

struct AzBackgroundPositionHorizontalVariant_Left { AzBackgroundPositionHorizontalTag tag; };
typedef struct AzBackgroundPositionHorizontalVariant_Left AzBackgroundPositionHorizontalVariant_Left;

struct AzBackgroundPositionHorizontalVariant_Center { AzBackgroundPositionHorizontalTag tag; };
typedef struct AzBackgroundPositionHorizontalVariant_Center AzBackgroundPositionHorizontalVariant_Center;

struct AzBackgroundPositionHorizontalVariant_Right { AzBackgroundPositionHorizontalTag tag; };
typedef struct AzBackgroundPositionHorizontalVariant_Right AzBackgroundPositionHorizontalVariant_Right;

struct AzBackgroundPositionHorizontalVariant_Exact { AzBackgroundPositionHorizontalTag tag; AzPixelValue payload; };
typedef struct AzBackgroundPositionHorizontalVariant_Exact AzBackgroundPositionHorizontalVariant_Exact;


union AzBackgroundPositionHorizontal {
    AzBackgroundPositionHorizontalVariant_Left Left;
    AzBackgroundPositionHorizontalVariant_Center Center;
    AzBackgroundPositionHorizontalVariant_Right Right;
    AzBackgroundPositionHorizontalVariant_Exact Exact;
};
typedef union AzBackgroundPositionHorizontal AzBackgroundPositionHorizontal;

enum AzBackgroundPositionVerticalTag {
   AzBackgroundPositionVerticalTag_Top,
   AzBackgroundPositionVerticalTag_Center,
   AzBackgroundPositionVerticalTag_Bottom,
   AzBackgroundPositionVerticalTag_Exact,
};
typedef enum AzBackgroundPositionVerticalTag AzBackgroundPositionVerticalTag;

struct AzBackgroundPositionVerticalVariant_Top { AzBackgroundPositionVerticalTag tag; };
typedef struct AzBackgroundPositionVerticalVariant_Top AzBackgroundPositionVerticalVariant_Top;

struct AzBackgroundPositionVerticalVariant_Center { AzBackgroundPositionVerticalTag tag; };
typedef struct AzBackgroundPositionVerticalVariant_Center AzBackgroundPositionVerticalVariant_Center;

struct AzBackgroundPositionVerticalVariant_Bottom { AzBackgroundPositionVerticalTag tag; };
typedef struct AzBackgroundPositionVerticalVariant_Bottom AzBackgroundPositionVerticalVariant_Bottom;

struct AzBackgroundPositionVerticalVariant_Exact { AzBackgroundPositionVerticalTag tag; AzPixelValue payload; };
typedef struct AzBackgroundPositionVerticalVariant_Exact AzBackgroundPositionVerticalVariant_Exact;


union AzBackgroundPositionVertical {
    AzBackgroundPositionVerticalVariant_Top Top;
    AzBackgroundPositionVerticalVariant_Center Center;
    AzBackgroundPositionVerticalVariant_Bottom Bottom;
    AzBackgroundPositionVerticalVariant_Exact Exact;
};
typedef union AzBackgroundPositionVertical AzBackgroundPositionVertical;

struct AzStyleBackgroundPosition {
    AzBackgroundPositionHorizontal horizontal;
    AzBackgroundPositionVertical vertical;
};
typedef struct AzStyleBackgroundPosition AzStyleBackgroundPosition;

enum AzStyleBackgroundSizeTag {
   AzStyleBackgroundSizeTag_ExactSize,
   AzStyleBackgroundSizeTag_Contain,
   AzStyleBackgroundSizeTag_Cover,
};
typedef enum AzStyleBackgroundSizeTag AzStyleBackgroundSizeTag;

struct AzStyleBackgroundSizeVariant_ExactSize { AzStyleBackgroundSizeTag tag; AzPixelValue payload[2]; };
typedef struct AzStyleBackgroundSizeVariant_ExactSize AzStyleBackgroundSizeVariant_ExactSize;

struct AzStyleBackgroundSizeVariant_Contain { AzStyleBackgroundSizeTag tag; };
typedef struct AzStyleBackgroundSizeVariant_Contain AzStyleBackgroundSizeVariant_Contain;

struct AzStyleBackgroundSizeVariant_Cover { AzStyleBackgroundSizeTag tag; };
typedef struct AzStyleBackgroundSizeVariant_Cover AzStyleBackgroundSizeVariant_Cover;


union AzStyleBackgroundSize {
    AzStyleBackgroundSizeVariant_ExactSize ExactSize;
    AzStyleBackgroundSizeVariant_Contain Contain;
    AzStyleBackgroundSizeVariant_Cover Cover;
};
typedef union AzStyleBackgroundSize AzStyleBackgroundSize;

struct AzStyleBorderBottomColor {
    AzColorU inner;
};
typedef struct AzStyleBorderBottomColor AzStyleBorderBottomColor;

struct AzStyleBorderBottomLeftRadius {
    AzPixelValue inner;
};
typedef struct AzStyleBorderBottomLeftRadius AzStyleBorderBottomLeftRadius;

struct AzStyleBorderBottomRightRadius {
    AzPixelValue inner;
};
typedef struct AzStyleBorderBottomRightRadius AzStyleBorderBottomRightRadius;

struct AzStyleBorderBottomStyle {
    AzBorderStyle inner;
};
typedef struct AzStyleBorderBottomStyle AzStyleBorderBottomStyle;

struct AzLayoutBorderBottomWidth {
    AzPixelValue inner;
};
typedef struct AzLayoutBorderBottomWidth AzLayoutBorderBottomWidth;

struct AzStyleBorderLeftColor {
    AzColorU inner;
};
typedef struct AzStyleBorderLeftColor AzStyleBorderLeftColor;

struct AzStyleBorderLeftStyle {
    AzBorderStyle inner;
};
typedef struct AzStyleBorderLeftStyle AzStyleBorderLeftStyle;

struct AzLayoutBorderLeftWidth {
    AzPixelValue inner;
};
typedef struct AzLayoutBorderLeftWidth AzLayoutBorderLeftWidth;

struct AzStyleBorderRightColor {
    AzColorU inner;
};
typedef struct AzStyleBorderRightColor AzStyleBorderRightColor;

struct AzStyleBorderRightStyle {
    AzBorderStyle inner;
};
typedef struct AzStyleBorderRightStyle AzStyleBorderRightStyle;

struct AzLayoutBorderRightWidth {
    AzPixelValue inner;
};
typedef struct AzLayoutBorderRightWidth AzLayoutBorderRightWidth;

struct AzStyleBorderTopColor {
    AzColorU inner;
};
typedef struct AzStyleBorderTopColor AzStyleBorderTopColor;

struct AzStyleBorderTopLeftRadius {
    AzPixelValue inner;
};
typedef struct AzStyleBorderTopLeftRadius AzStyleBorderTopLeftRadius;

struct AzStyleBorderTopRightRadius {
    AzPixelValue inner;
};
typedef struct AzStyleBorderTopRightRadius AzStyleBorderTopRightRadius;

struct AzStyleBorderTopStyle {
    AzBorderStyle inner;
};
typedef struct AzStyleBorderTopStyle AzStyleBorderTopStyle;

struct AzLayoutBorderTopWidth {
    AzPixelValue inner;
};
typedef struct AzLayoutBorderTopWidth AzLayoutBorderTopWidth;

struct AzStyleFontSize {
    AzPixelValue inner;
};
typedef struct AzStyleFontSize AzStyleFontSize;

struct AzStyleLetterSpacing {
    AzPixelValue inner;
};
typedef struct AzStyleLetterSpacing AzStyleLetterSpacing;

struct AzStyleLineHeight {
    AzPercentageValue inner;
};
typedef struct AzStyleLineHeight AzStyleLineHeight;

struct AzStyleTabWidth {
    AzPercentageValue inner;
};
typedef struct AzStyleTabWidth AzStyleTabWidth;

struct AzStyleOpacity {
    AzFloatValue inner;
};
typedef struct AzStyleOpacity AzStyleOpacity;

struct AzStyleTransformOrigin {
    AzPixelValue x;
    AzPixelValue y;
};
typedef struct AzStyleTransformOrigin AzStyleTransformOrigin;

struct AzStylePerspectiveOrigin {
    AzPixelValue x;
    AzPixelValue y;
};
typedef struct AzStylePerspectiveOrigin AzStylePerspectiveOrigin;

struct AzStyleTransformMatrix2D {
    AzPixelValue a;
    AzPixelValue b;
    AzPixelValue c;
    AzPixelValue d;
    AzPixelValue tx;
    AzPixelValue ty;
};
typedef struct AzStyleTransformMatrix2D AzStyleTransformMatrix2D;

struct AzStyleTransformMatrix3D {
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
};
typedef struct AzStyleTransformMatrix3D AzStyleTransformMatrix3D;

struct AzStyleTransformTranslate2D {
    AzPixelValue x;
    AzPixelValue y;
};
typedef struct AzStyleTransformTranslate2D AzStyleTransformTranslate2D;

struct AzStyleTransformTranslate3D {
    AzPixelValue x;
    AzPixelValue y;
    AzPixelValue z;
};
typedef struct AzStyleTransformTranslate3D AzStyleTransformTranslate3D;

struct AzStyleTransformRotate3D {
    AzPercentageValue x;
    AzPercentageValue y;
    AzPercentageValue z;
    AzAngleValue angle;
};
typedef struct AzStyleTransformRotate3D AzStyleTransformRotate3D;

struct AzStyleTransformScale2D {
    AzPercentageValue x;
    AzPercentageValue y;
};
typedef struct AzStyleTransformScale2D AzStyleTransformScale2D;

struct AzStyleTransformScale3D {
    AzPercentageValue x;
    AzPercentageValue y;
    AzPercentageValue z;
};
typedef struct AzStyleTransformScale3D AzStyleTransformScale3D;

struct AzStyleTransformSkew2D {
    AzPercentageValue x;
    AzPercentageValue y;
};
typedef struct AzStyleTransformSkew2D AzStyleTransformSkew2D;

struct AzStyleTextColor {
    AzColorU inner;
};
typedef struct AzStyleTextColor AzStyleTextColor;

struct AzStyleWordSpacing {
    AzPixelValue inner;
};
typedef struct AzStyleWordSpacing AzStyleWordSpacing;

enum AzStyleBoxShadowValueTag {
   AzStyleBoxShadowValueTag_Auto,
   AzStyleBoxShadowValueTag_None,
   AzStyleBoxShadowValueTag_Inherit,
   AzStyleBoxShadowValueTag_Initial,
   AzStyleBoxShadowValueTag_Exact,
};
typedef enum AzStyleBoxShadowValueTag AzStyleBoxShadowValueTag;

struct AzStyleBoxShadowValueVariant_Auto { AzStyleBoxShadowValueTag tag; };
typedef struct AzStyleBoxShadowValueVariant_Auto AzStyleBoxShadowValueVariant_Auto;

struct AzStyleBoxShadowValueVariant_None { AzStyleBoxShadowValueTag tag; };
typedef struct AzStyleBoxShadowValueVariant_None AzStyleBoxShadowValueVariant_None;

struct AzStyleBoxShadowValueVariant_Inherit { AzStyleBoxShadowValueTag tag; };
typedef struct AzStyleBoxShadowValueVariant_Inherit AzStyleBoxShadowValueVariant_Inherit;

struct AzStyleBoxShadowValueVariant_Initial { AzStyleBoxShadowValueTag tag; };
typedef struct AzStyleBoxShadowValueVariant_Initial AzStyleBoxShadowValueVariant_Initial;

struct AzStyleBoxShadowValueVariant_Exact { AzStyleBoxShadowValueTag tag; AzStyleBoxShadow payload; };
typedef struct AzStyleBoxShadowValueVariant_Exact AzStyleBoxShadowValueVariant_Exact;


union AzStyleBoxShadowValue {
    AzStyleBoxShadowValueVariant_Auto Auto;
    AzStyleBoxShadowValueVariant_None None;
    AzStyleBoxShadowValueVariant_Inherit Inherit;
    AzStyleBoxShadowValueVariant_Initial Initial;
    AzStyleBoxShadowValueVariant_Exact Exact;
};
typedef union AzStyleBoxShadowValue AzStyleBoxShadowValue;

enum AzLayoutAlignContentValueTag {
   AzLayoutAlignContentValueTag_Auto,
   AzLayoutAlignContentValueTag_None,
   AzLayoutAlignContentValueTag_Inherit,
   AzLayoutAlignContentValueTag_Initial,
   AzLayoutAlignContentValueTag_Exact,
};
typedef enum AzLayoutAlignContentValueTag AzLayoutAlignContentValueTag;

struct AzLayoutAlignContentValueVariant_Auto { AzLayoutAlignContentValueTag tag; };
typedef struct AzLayoutAlignContentValueVariant_Auto AzLayoutAlignContentValueVariant_Auto;

struct AzLayoutAlignContentValueVariant_None { AzLayoutAlignContentValueTag tag; };
typedef struct AzLayoutAlignContentValueVariant_None AzLayoutAlignContentValueVariant_None;

struct AzLayoutAlignContentValueVariant_Inherit { AzLayoutAlignContentValueTag tag; };
typedef struct AzLayoutAlignContentValueVariant_Inherit AzLayoutAlignContentValueVariant_Inherit;

struct AzLayoutAlignContentValueVariant_Initial { AzLayoutAlignContentValueTag tag; };
typedef struct AzLayoutAlignContentValueVariant_Initial AzLayoutAlignContentValueVariant_Initial;

struct AzLayoutAlignContentValueVariant_Exact { AzLayoutAlignContentValueTag tag; AzLayoutAlignContent payload; };
typedef struct AzLayoutAlignContentValueVariant_Exact AzLayoutAlignContentValueVariant_Exact;


union AzLayoutAlignContentValue {
    AzLayoutAlignContentValueVariant_Auto Auto;
    AzLayoutAlignContentValueVariant_None None;
    AzLayoutAlignContentValueVariant_Inherit Inherit;
    AzLayoutAlignContentValueVariant_Initial Initial;
    AzLayoutAlignContentValueVariant_Exact Exact;
};
typedef union AzLayoutAlignContentValue AzLayoutAlignContentValue;

enum AzLayoutAlignItemsValueTag {
   AzLayoutAlignItemsValueTag_Auto,
   AzLayoutAlignItemsValueTag_None,
   AzLayoutAlignItemsValueTag_Inherit,
   AzLayoutAlignItemsValueTag_Initial,
   AzLayoutAlignItemsValueTag_Exact,
};
typedef enum AzLayoutAlignItemsValueTag AzLayoutAlignItemsValueTag;

struct AzLayoutAlignItemsValueVariant_Auto { AzLayoutAlignItemsValueTag tag; };
typedef struct AzLayoutAlignItemsValueVariant_Auto AzLayoutAlignItemsValueVariant_Auto;

struct AzLayoutAlignItemsValueVariant_None { AzLayoutAlignItemsValueTag tag; };
typedef struct AzLayoutAlignItemsValueVariant_None AzLayoutAlignItemsValueVariant_None;

struct AzLayoutAlignItemsValueVariant_Inherit { AzLayoutAlignItemsValueTag tag; };
typedef struct AzLayoutAlignItemsValueVariant_Inherit AzLayoutAlignItemsValueVariant_Inherit;

struct AzLayoutAlignItemsValueVariant_Initial { AzLayoutAlignItemsValueTag tag; };
typedef struct AzLayoutAlignItemsValueVariant_Initial AzLayoutAlignItemsValueVariant_Initial;

struct AzLayoutAlignItemsValueVariant_Exact { AzLayoutAlignItemsValueTag tag; AzLayoutAlignItems payload; };
typedef struct AzLayoutAlignItemsValueVariant_Exact AzLayoutAlignItemsValueVariant_Exact;


union AzLayoutAlignItemsValue {
    AzLayoutAlignItemsValueVariant_Auto Auto;
    AzLayoutAlignItemsValueVariant_None None;
    AzLayoutAlignItemsValueVariant_Inherit Inherit;
    AzLayoutAlignItemsValueVariant_Initial Initial;
    AzLayoutAlignItemsValueVariant_Exact Exact;
};
typedef union AzLayoutAlignItemsValue AzLayoutAlignItemsValue;

enum AzLayoutBottomValueTag {
   AzLayoutBottomValueTag_Auto,
   AzLayoutBottomValueTag_None,
   AzLayoutBottomValueTag_Inherit,
   AzLayoutBottomValueTag_Initial,
   AzLayoutBottomValueTag_Exact,
};
typedef enum AzLayoutBottomValueTag AzLayoutBottomValueTag;

struct AzLayoutBottomValueVariant_Auto { AzLayoutBottomValueTag tag; };
typedef struct AzLayoutBottomValueVariant_Auto AzLayoutBottomValueVariant_Auto;

struct AzLayoutBottomValueVariant_None { AzLayoutBottomValueTag tag; };
typedef struct AzLayoutBottomValueVariant_None AzLayoutBottomValueVariant_None;

struct AzLayoutBottomValueVariant_Inherit { AzLayoutBottomValueTag tag; };
typedef struct AzLayoutBottomValueVariant_Inherit AzLayoutBottomValueVariant_Inherit;

struct AzLayoutBottomValueVariant_Initial { AzLayoutBottomValueTag tag; };
typedef struct AzLayoutBottomValueVariant_Initial AzLayoutBottomValueVariant_Initial;

struct AzLayoutBottomValueVariant_Exact { AzLayoutBottomValueTag tag; AzLayoutBottom payload; };
typedef struct AzLayoutBottomValueVariant_Exact AzLayoutBottomValueVariant_Exact;


union AzLayoutBottomValue {
    AzLayoutBottomValueVariant_Auto Auto;
    AzLayoutBottomValueVariant_None None;
    AzLayoutBottomValueVariant_Inherit Inherit;
    AzLayoutBottomValueVariant_Initial Initial;
    AzLayoutBottomValueVariant_Exact Exact;
};
typedef union AzLayoutBottomValue AzLayoutBottomValue;

enum AzLayoutBoxSizingValueTag {
   AzLayoutBoxSizingValueTag_Auto,
   AzLayoutBoxSizingValueTag_None,
   AzLayoutBoxSizingValueTag_Inherit,
   AzLayoutBoxSizingValueTag_Initial,
   AzLayoutBoxSizingValueTag_Exact,
};
typedef enum AzLayoutBoxSizingValueTag AzLayoutBoxSizingValueTag;

struct AzLayoutBoxSizingValueVariant_Auto { AzLayoutBoxSizingValueTag tag; };
typedef struct AzLayoutBoxSizingValueVariant_Auto AzLayoutBoxSizingValueVariant_Auto;

struct AzLayoutBoxSizingValueVariant_None { AzLayoutBoxSizingValueTag tag; };
typedef struct AzLayoutBoxSizingValueVariant_None AzLayoutBoxSizingValueVariant_None;

struct AzLayoutBoxSizingValueVariant_Inherit { AzLayoutBoxSizingValueTag tag; };
typedef struct AzLayoutBoxSizingValueVariant_Inherit AzLayoutBoxSizingValueVariant_Inherit;

struct AzLayoutBoxSizingValueVariant_Initial { AzLayoutBoxSizingValueTag tag; };
typedef struct AzLayoutBoxSizingValueVariant_Initial AzLayoutBoxSizingValueVariant_Initial;

struct AzLayoutBoxSizingValueVariant_Exact { AzLayoutBoxSizingValueTag tag; AzLayoutBoxSizing payload; };
typedef struct AzLayoutBoxSizingValueVariant_Exact AzLayoutBoxSizingValueVariant_Exact;


union AzLayoutBoxSizingValue {
    AzLayoutBoxSizingValueVariant_Auto Auto;
    AzLayoutBoxSizingValueVariant_None None;
    AzLayoutBoxSizingValueVariant_Inherit Inherit;
    AzLayoutBoxSizingValueVariant_Initial Initial;
    AzLayoutBoxSizingValueVariant_Exact Exact;
};
typedef union AzLayoutBoxSizingValue AzLayoutBoxSizingValue;

enum AzLayoutFlexDirectionValueTag {
   AzLayoutFlexDirectionValueTag_Auto,
   AzLayoutFlexDirectionValueTag_None,
   AzLayoutFlexDirectionValueTag_Inherit,
   AzLayoutFlexDirectionValueTag_Initial,
   AzLayoutFlexDirectionValueTag_Exact,
};
typedef enum AzLayoutFlexDirectionValueTag AzLayoutFlexDirectionValueTag;

struct AzLayoutFlexDirectionValueVariant_Auto { AzLayoutFlexDirectionValueTag tag; };
typedef struct AzLayoutFlexDirectionValueVariant_Auto AzLayoutFlexDirectionValueVariant_Auto;

struct AzLayoutFlexDirectionValueVariant_None { AzLayoutFlexDirectionValueTag tag; };
typedef struct AzLayoutFlexDirectionValueVariant_None AzLayoutFlexDirectionValueVariant_None;

struct AzLayoutFlexDirectionValueVariant_Inherit { AzLayoutFlexDirectionValueTag tag; };
typedef struct AzLayoutFlexDirectionValueVariant_Inherit AzLayoutFlexDirectionValueVariant_Inherit;

struct AzLayoutFlexDirectionValueVariant_Initial { AzLayoutFlexDirectionValueTag tag; };
typedef struct AzLayoutFlexDirectionValueVariant_Initial AzLayoutFlexDirectionValueVariant_Initial;

struct AzLayoutFlexDirectionValueVariant_Exact { AzLayoutFlexDirectionValueTag tag; AzLayoutFlexDirection payload; };
typedef struct AzLayoutFlexDirectionValueVariant_Exact AzLayoutFlexDirectionValueVariant_Exact;


union AzLayoutFlexDirectionValue {
    AzLayoutFlexDirectionValueVariant_Auto Auto;
    AzLayoutFlexDirectionValueVariant_None None;
    AzLayoutFlexDirectionValueVariant_Inherit Inherit;
    AzLayoutFlexDirectionValueVariant_Initial Initial;
    AzLayoutFlexDirectionValueVariant_Exact Exact;
};
typedef union AzLayoutFlexDirectionValue AzLayoutFlexDirectionValue;

enum AzLayoutDisplayValueTag {
   AzLayoutDisplayValueTag_Auto,
   AzLayoutDisplayValueTag_None,
   AzLayoutDisplayValueTag_Inherit,
   AzLayoutDisplayValueTag_Initial,
   AzLayoutDisplayValueTag_Exact,
};
typedef enum AzLayoutDisplayValueTag AzLayoutDisplayValueTag;

struct AzLayoutDisplayValueVariant_Auto { AzLayoutDisplayValueTag tag; };
typedef struct AzLayoutDisplayValueVariant_Auto AzLayoutDisplayValueVariant_Auto;

struct AzLayoutDisplayValueVariant_None { AzLayoutDisplayValueTag tag; };
typedef struct AzLayoutDisplayValueVariant_None AzLayoutDisplayValueVariant_None;

struct AzLayoutDisplayValueVariant_Inherit { AzLayoutDisplayValueTag tag; };
typedef struct AzLayoutDisplayValueVariant_Inherit AzLayoutDisplayValueVariant_Inherit;

struct AzLayoutDisplayValueVariant_Initial { AzLayoutDisplayValueTag tag; };
typedef struct AzLayoutDisplayValueVariant_Initial AzLayoutDisplayValueVariant_Initial;

struct AzLayoutDisplayValueVariant_Exact { AzLayoutDisplayValueTag tag; AzLayoutDisplay payload; };
typedef struct AzLayoutDisplayValueVariant_Exact AzLayoutDisplayValueVariant_Exact;


union AzLayoutDisplayValue {
    AzLayoutDisplayValueVariant_Auto Auto;
    AzLayoutDisplayValueVariant_None None;
    AzLayoutDisplayValueVariant_Inherit Inherit;
    AzLayoutDisplayValueVariant_Initial Initial;
    AzLayoutDisplayValueVariant_Exact Exact;
};
typedef union AzLayoutDisplayValue AzLayoutDisplayValue;

enum AzLayoutFlexGrowValueTag {
   AzLayoutFlexGrowValueTag_Auto,
   AzLayoutFlexGrowValueTag_None,
   AzLayoutFlexGrowValueTag_Inherit,
   AzLayoutFlexGrowValueTag_Initial,
   AzLayoutFlexGrowValueTag_Exact,
};
typedef enum AzLayoutFlexGrowValueTag AzLayoutFlexGrowValueTag;

struct AzLayoutFlexGrowValueVariant_Auto { AzLayoutFlexGrowValueTag tag; };
typedef struct AzLayoutFlexGrowValueVariant_Auto AzLayoutFlexGrowValueVariant_Auto;

struct AzLayoutFlexGrowValueVariant_None { AzLayoutFlexGrowValueTag tag; };
typedef struct AzLayoutFlexGrowValueVariant_None AzLayoutFlexGrowValueVariant_None;

struct AzLayoutFlexGrowValueVariant_Inherit { AzLayoutFlexGrowValueTag tag; };
typedef struct AzLayoutFlexGrowValueVariant_Inherit AzLayoutFlexGrowValueVariant_Inherit;

struct AzLayoutFlexGrowValueVariant_Initial { AzLayoutFlexGrowValueTag tag; };
typedef struct AzLayoutFlexGrowValueVariant_Initial AzLayoutFlexGrowValueVariant_Initial;

struct AzLayoutFlexGrowValueVariant_Exact { AzLayoutFlexGrowValueTag tag; AzLayoutFlexGrow payload; };
typedef struct AzLayoutFlexGrowValueVariant_Exact AzLayoutFlexGrowValueVariant_Exact;


union AzLayoutFlexGrowValue {
    AzLayoutFlexGrowValueVariant_Auto Auto;
    AzLayoutFlexGrowValueVariant_None None;
    AzLayoutFlexGrowValueVariant_Inherit Inherit;
    AzLayoutFlexGrowValueVariant_Initial Initial;
    AzLayoutFlexGrowValueVariant_Exact Exact;
};
typedef union AzLayoutFlexGrowValue AzLayoutFlexGrowValue;

enum AzLayoutFlexShrinkValueTag {
   AzLayoutFlexShrinkValueTag_Auto,
   AzLayoutFlexShrinkValueTag_None,
   AzLayoutFlexShrinkValueTag_Inherit,
   AzLayoutFlexShrinkValueTag_Initial,
   AzLayoutFlexShrinkValueTag_Exact,
};
typedef enum AzLayoutFlexShrinkValueTag AzLayoutFlexShrinkValueTag;

struct AzLayoutFlexShrinkValueVariant_Auto { AzLayoutFlexShrinkValueTag tag; };
typedef struct AzLayoutFlexShrinkValueVariant_Auto AzLayoutFlexShrinkValueVariant_Auto;

struct AzLayoutFlexShrinkValueVariant_None { AzLayoutFlexShrinkValueTag tag; };
typedef struct AzLayoutFlexShrinkValueVariant_None AzLayoutFlexShrinkValueVariant_None;

struct AzLayoutFlexShrinkValueVariant_Inherit { AzLayoutFlexShrinkValueTag tag; };
typedef struct AzLayoutFlexShrinkValueVariant_Inherit AzLayoutFlexShrinkValueVariant_Inherit;

struct AzLayoutFlexShrinkValueVariant_Initial { AzLayoutFlexShrinkValueTag tag; };
typedef struct AzLayoutFlexShrinkValueVariant_Initial AzLayoutFlexShrinkValueVariant_Initial;

struct AzLayoutFlexShrinkValueVariant_Exact { AzLayoutFlexShrinkValueTag tag; AzLayoutFlexShrink payload; };
typedef struct AzLayoutFlexShrinkValueVariant_Exact AzLayoutFlexShrinkValueVariant_Exact;


union AzLayoutFlexShrinkValue {
    AzLayoutFlexShrinkValueVariant_Auto Auto;
    AzLayoutFlexShrinkValueVariant_None None;
    AzLayoutFlexShrinkValueVariant_Inherit Inherit;
    AzLayoutFlexShrinkValueVariant_Initial Initial;
    AzLayoutFlexShrinkValueVariant_Exact Exact;
};
typedef union AzLayoutFlexShrinkValue AzLayoutFlexShrinkValue;

enum AzLayoutFloatValueTag {
   AzLayoutFloatValueTag_Auto,
   AzLayoutFloatValueTag_None,
   AzLayoutFloatValueTag_Inherit,
   AzLayoutFloatValueTag_Initial,
   AzLayoutFloatValueTag_Exact,
};
typedef enum AzLayoutFloatValueTag AzLayoutFloatValueTag;

struct AzLayoutFloatValueVariant_Auto { AzLayoutFloatValueTag tag; };
typedef struct AzLayoutFloatValueVariant_Auto AzLayoutFloatValueVariant_Auto;

struct AzLayoutFloatValueVariant_None { AzLayoutFloatValueTag tag; };
typedef struct AzLayoutFloatValueVariant_None AzLayoutFloatValueVariant_None;

struct AzLayoutFloatValueVariant_Inherit { AzLayoutFloatValueTag tag; };
typedef struct AzLayoutFloatValueVariant_Inherit AzLayoutFloatValueVariant_Inherit;

struct AzLayoutFloatValueVariant_Initial { AzLayoutFloatValueTag tag; };
typedef struct AzLayoutFloatValueVariant_Initial AzLayoutFloatValueVariant_Initial;

struct AzLayoutFloatValueVariant_Exact { AzLayoutFloatValueTag tag; AzLayoutFloat payload; };
typedef struct AzLayoutFloatValueVariant_Exact AzLayoutFloatValueVariant_Exact;


union AzLayoutFloatValue {
    AzLayoutFloatValueVariant_Auto Auto;
    AzLayoutFloatValueVariant_None None;
    AzLayoutFloatValueVariant_Inherit Inherit;
    AzLayoutFloatValueVariant_Initial Initial;
    AzLayoutFloatValueVariant_Exact Exact;
};
typedef union AzLayoutFloatValue AzLayoutFloatValue;

enum AzLayoutHeightValueTag {
   AzLayoutHeightValueTag_Auto,
   AzLayoutHeightValueTag_None,
   AzLayoutHeightValueTag_Inherit,
   AzLayoutHeightValueTag_Initial,
   AzLayoutHeightValueTag_Exact,
};
typedef enum AzLayoutHeightValueTag AzLayoutHeightValueTag;

struct AzLayoutHeightValueVariant_Auto { AzLayoutHeightValueTag tag; };
typedef struct AzLayoutHeightValueVariant_Auto AzLayoutHeightValueVariant_Auto;

struct AzLayoutHeightValueVariant_None { AzLayoutHeightValueTag tag; };
typedef struct AzLayoutHeightValueVariant_None AzLayoutHeightValueVariant_None;

struct AzLayoutHeightValueVariant_Inherit { AzLayoutHeightValueTag tag; };
typedef struct AzLayoutHeightValueVariant_Inherit AzLayoutHeightValueVariant_Inherit;

struct AzLayoutHeightValueVariant_Initial { AzLayoutHeightValueTag tag; };
typedef struct AzLayoutHeightValueVariant_Initial AzLayoutHeightValueVariant_Initial;

struct AzLayoutHeightValueVariant_Exact { AzLayoutHeightValueTag tag; AzLayoutHeight payload; };
typedef struct AzLayoutHeightValueVariant_Exact AzLayoutHeightValueVariant_Exact;


union AzLayoutHeightValue {
    AzLayoutHeightValueVariant_Auto Auto;
    AzLayoutHeightValueVariant_None None;
    AzLayoutHeightValueVariant_Inherit Inherit;
    AzLayoutHeightValueVariant_Initial Initial;
    AzLayoutHeightValueVariant_Exact Exact;
};
typedef union AzLayoutHeightValue AzLayoutHeightValue;

enum AzLayoutJustifyContentValueTag {
   AzLayoutJustifyContentValueTag_Auto,
   AzLayoutJustifyContentValueTag_None,
   AzLayoutJustifyContentValueTag_Inherit,
   AzLayoutJustifyContentValueTag_Initial,
   AzLayoutJustifyContentValueTag_Exact,
};
typedef enum AzLayoutJustifyContentValueTag AzLayoutJustifyContentValueTag;

struct AzLayoutJustifyContentValueVariant_Auto { AzLayoutJustifyContentValueTag tag; };
typedef struct AzLayoutJustifyContentValueVariant_Auto AzLayoutJustifyContentValueVariant_Auto;

struct AzLayoutJustifyContentValueVariant_None { AzLayoutJustifyContentValueTag tag; };
typedef struct AzLayoutJustifyContentValueVariant_None AzLayoutJustifyContentValueVariant_None;

struct AzLayoutJustifyContentValueVariant_Inherit { AzLayoutJustifyContentValueTag tag; };
typedef struct AzLayoutJustifyContentValueVariant_Inherit AzLayoutJustifyContentValueVariant_Inherit;

struct AzLayoutJustifyContentValueVariant_Initial { AzLayoutJustifyContentValueTag tag; };
typedef struct AzLayoutJustifyContentValueVariant_Initial AzLayoutJustifyContentValueVariant_Initial;

struct AzLayoutJustifyContentValueVariant_Exact { AzLayoutJustifyContentValueTag tag; AzLayoutJustifyContent payload; };
typedef struct AzLayoutJustifyContentValueVariant_Exact AzLayoutJustifyContentValueVariant_Exact;


union AzLayoutJustifyContentValue {
    AzLayoutJustifyContentValueVariant_Auto Auto;
    AzLayoutJustifyContentValueVariant_None None;
    AzLayoutJustifyContentValueVariant_Inherit Inherit;
    AzLayoutJustifyContentValueVariant_Initial Initial;
    AzLayoutJustifyContentValueVariant_Exact Exact;
};
typedef union AzLayoutJustifyContentValue AzLayoutJustifyContentValue;

enum AzLayoutLeftValueTag {
   AzLayoutLeftValueTag_Auto,
   AzLayoutLeftValueTag_None,
   AzLayoutLeftValueTag_Inherit,
   AzLayoutLeftValueTag_Initial,
   AzLayoutLeftValueTag_Exact,
};
typedef enum AzLayoutLeftValueTag AzLayoutLeftValueTag;

struct AzLayoutLeftValueVariant_Auto { AzLayoutLeftValueTag tag; };
typedef struct AzLayoutLeftValueVariant_Auto AzLayoutLeftValueVariant_Auto;

struct AzLayoutLeftValueVariant_None { AzLayoutLeftValueTag tag; };
typedef struct AzLayoutLeftValueVariant_None AzLayoutLeftValueVariant_None;

struct AzLayoutLeftValueVariant_Inherit { AzLayoutLeftValueTag tag; };
typedef struct AzLayoutLeftValueVariant_Inherit AzLayoutLeftValueVariant_Inherit;

struct AzLayoutLeftValueVariant_Initial { AzLayoutLeftValueTag tag; };
typedef struct AzLayoutLeftValueVariant_Initial AzLayoutLeftValueVariant_Initial;

struct AzLayoutLeftValueVariant_Exact { AzLayoutLeftValueTag tag; AzLayoutLeft payload; };
typedef struct AzLayoutLeftValueVariant_Exact AzLayoutLeftValueVariant_Exact;


union AzLayoutLeftValue {
    AzLayoutLeftValueVariant_Auto Auto;
    AzLayoutLeftValueVariant_None None;
    AzLayoutLeftValueVariant_Inherit Inherit;
    AzLayoutLeftValueVariant_Initial Initial;
    AzLayoutLeftValueVariant_Exact Exact;
};
typedef union AzLayoutLeftValue AzLayoutLeftValue;

enum AzLayoutMarginBottomValueTag {
   AzLayoutMarginBottomValueTag_Auto,
   AzLayoutMarginBottomValueTag_None,
   AzLayoutMarginBottomValueTag_Inherit,
   AzLayoutMarginBottomValueTag_Initial,
   AzLayoutMarginBottomValueTag_Exact,
};
typedef enum AzLayoutMarginBottomValueTag AzLayoutMarginBottomValueTag;

struct AzLayoutMarginBottomValueVariant_Auto { AzLayoutMarginBottomValueTag tag; };
typedef struct AzLayoutMarginBottomValueVariant_Auto AzLayoutMarginBottomValueVariant_Auto;

struct AzLayoutMarginBottomValueVariant_None { AzLayoutMarginBottomValueTag tag; };
typedef struct AzLayoutMarginBottomValueVariant_None AzLayoutMarginBottomValueVariant_None;

struct AzLayoutMarginBottomValueVariant_Inherit { AzLayoutMarginBottomValueTag tag; };
typedef struct AzLayoutMarginBottomValueVariant_Inherit AzLayoutMarginBottomValueVariant_Inherit;

struct AzLayoutMarginBottomValueVariant_Initial { AzLayoutMarginBottomValueTag tag; };
typedef struct AzLayoutMarginBottomValueVariant_Initial AzLayoutMarginBottomValueVariant_Initial;

struct AzLayoutMarginBottomValueVariant_Exact { AzLayoutMarginBottomValueTag tag; AzLayoutMarginBottom payload; };
typedef struct AzLayoutMarginBottomValueVariant_Exact AzLayoutMarginBottomValueVariant_Exact;


union AzLayoutMarginBottomValue {
    AzLayoutMarginBottomValueVariant_Auto Auto;
    AzLayoutMarginBottomValueVariant_None None;
    AzLayoutMarginBottomValueVariant_Inherit Inherit;
    AzLayoutMarginBottomValueVariant_Initial Initial;
    AzLayoutMarginBottomValueVariant_Exact Exact;
};
typedef union AzLayoutMarginBottomValue AzLayoutMarginBottomValue;

enum AzLayoutMarginLeftValueTag {
   AzLayoutMarginLeftValueTag_Auto,
   AzLayoutMarginLeftValueTag_None,
   AzLayoutMarginLeftValueTag_Inherit,
   AzLayoutMarginLeftValueTag_Initial,
   AzLayoutMarginLeftValueTag_Exact,
};
typedef enum AzLayoutMarginLeftValueTag AzLayoutMarginLeftValueTag;

struct AzLayoutMarginLeftValueVariant_Auto { AzLayoutMarginLeftValueTag tag; };
typedef struct AzLayoutMarginLeftValueVariant_Auto AzLayoutMarginLeftValueVariant_Auto;

struct AzLayoutMarginLeftValueVariant_None { AzLayoutMarginLeftValueTag tag; };
typedef struct AzLayoutMarginLeftValueVariant_None AzLayoutMarginLeftValueVariant_None;

struct AzLayoutMarginLeftValueVariant_Inherit { AzLayoutMarginLeftValueTag tag; };
typedef struct AzLayoutMarginLeftValueVariant_Inherit AzLayoutMarginLeftValueVariant_Inherit;

struct AzLayoutMarginLeftValueVariant_Initial { AzLayoutMarginLeftValueTag tag; };
typedef struct AzLayoutMarginLeftValueVariant_Initial AzLayoutMarginLeftValueVariant_Initial;

struct AzLayoutMarginLeftValueVariant_Exact { AzLayoutMarginLeftValueTag tag; AzLayoutMarginLeft payload; };
typedef struct AzLayoutMarginLeftValueVariant_Exact AzLayoutMarginLeftValueVariant_Exact;


union AzLayoutMarginLeftValue {
    AzLayoutMarginLeftValueVariant_Auto Auto;
    AzLayoutMarginLeftValueVariant_None None;
    AzLayoutMarginLeftValueVariant_Inherit Inherit;
    AzLayoutMarginLeftValueVariant_Initial Initial;
    AzLayoutMarginLeftValueVariant_Exact Exact;
};
typedef union AzLayoutMarginLeftValue AzLayoutMarginLeftValue;

enum AzLayoutMarginRightValueTag {
   AzLayoutMarginRightValueTag_Auto,
   AzLayoutMarginRightValueTag_None,
   AzLayoutMarginRightValueTag_Inherit,
   AzLayoutMarginRightValueTag_Initial,
   AzLayoutMarginRightValueTag_Exact,
};
typedef enum AzLayoutMarginRightValueTag AzLayoutMarginRightValueTag;

struct AzLayoutMarginRightValueVariant_Auto { AzLayoutMarginRightValueTag tag; };
typedef struct AzLayoutMarginRightValueVariant_Auto AzLayoutMarginRightValueVariant_Auto;

struct AzLayoutMarginRightValueVariant_None { AzLayoutMarginRightValueTag tag; };
typedef struct AzLayoutMarginRightValueVariant_None AzLayoutMarginRightValueVariant_None;

struct AzLayoutMarginRightValueVariant_Inherit { AzLayoutMarginRightValueTag tag; };
typedef struct AzLayoutMarginRightValueVariant_Inherit AzLayoutMarginRightValueVariant_Inherit;

struct AzLayoutMarginRightValueVariant_Initial { AzLayoutMarginRightValueTag tag; };
typedef struct AzLayoutMarginRightValueVariant_Initial AzLayoutMarginRightValueVariant_Initial;

struct AzLayoutMarginRightValueVariant_Exact { AzLayoutMarginRightValueTag tag; AzLayoutMarginRight payload; };
typedef struct AzLayoutMarginRightValueVariant_Exact AzLayoutMarginRightValueVariant_Exact;


union AzLayoutMarginRightValue {
    AzLayoutMarginRightValueVariant_Auto Auto;
    AzLayoutMarginRightValueVariant_None None;
    AzLayoutMarginRightValueVariant_Inherit Inherit;
    AzLayoutMarginRightValueVariant_Initial Initial;
    AzLayoutMarginRightValueVariant_Exact Exact;
};
typedef union AzLayoutMarginRightValue AzLayoutMarginRightValue;

enum AzLayoutMarginTopValueTag {
   AzLayoutMarginTopValueTag_Auto,
   AzLayoutMarginTopValueTag_None,
   AzLayoutMarginTopValueTag_Inherit,
   AzLayoutMarginTopValueTag_Initial,
   AzLayoutMarginTopValueTag_Exact,
};
typedef enum AzLayoutMarginTopValueTag AzLayoutMarginTopValueTag;

struct AzLayoutMarginTopValueVariant_Auto { AzLayoutMarginTopValueTag tag; };
typedef struct AzLayoutMarginTopValueVariant_Auto AzLayoutMarginTopValueVariant_Auto;

struct AzLayoutMarginTopValueVariant_None { AzLayoutMarginTopValueTag tag; };
typedef struct AzLayoutMarginTopValueVariant_None AzLayoutMarginTopValueVariant_None;

struct AzLayoutMarginTopValueVariant_Inherit { AzLayoutMarginTopValueTag tag; };
typedef struct AzLayoutMarginTopValueVariant_Inherit AzLayoutMarginTopValueVariant_Inherit;

struct AzLayoutMarginTopValueVariant_Initial { AzLayoutMarginTopValueTag tag; };
typedef struct AzLayoutMarginTopValueVariant_Initial AzLayoutMarginTopValueVariant_Initial;

struct AzLayoutMarginTopValueVariant_Exact { AzLayoutMarginTopValueTag tag; AzLayoutMarginTop payload; };
typedef struct AzLayoutMarginTopValueVariant_Exact AzLayoutMarginTopValueVariant_Exact;


union AzLayoutMarginTopValue {
    AzLayoutMarginTopValueVariant_Auto Auto;
    AzLayoutMarginTopValueVariant_None None;
    AzLayoutMarginTopValueVariant_Inherit Inherit;
    AzLayoutMarginTopValueVariant_Initial Initial;
    AzLayoutMarginTopValueVariant_Exact Exact;
};
typedef union AzLayoutMarginTopValue AzLayoutMarginTopValue;

enum AzLayoutMaxHeightValueTag {
   AzLayoutMaxHeightValueTag_Auto,
   AzLayoutMaxHeightValueTag_None,
   AzLayoutMaxHeightValueTag_Inherit,
   AzLayoutMaxHeightValueTag_Initial,
   AzLayoutMaxHeightValueTag_Exact,
};
typedef enum AzLayoutMaxHeightValueTag AzLayoutMaxHeightValueTag;

struct AzLayoutMaxHeightValueVariant_Auto { AzLayoutMaxHeightValueTag tag; };
typedef struct AzLayoutMaxHeightValueVariant_Auto AzLayoutMaxHeightValueVariant_Auto;

struct AzLayoutMaxHeightValueVariant_None { AzLayoutMaxHeightValueTag tag; };
typedef struct AzLayoutMaxHeightValueVariant_None AzLayoutMaxHeightValueVariant_None;

struct AzLayoutMaxHeightValueVariant_Inherit { AzLayoutMaxHeightValueTag tag; };
typedef struct AzLayoutMaxHeightValueVariant_Inherit AzLayoutMaxHeightValueVariant_Inherit;

struct AzLayoutMaxHeightValueVariant_Initial { AzLayoutMaxHeightValueTag tag; };
typedef struct AzLayoutMaxHeightValueVariant_Initial AzLayoutMaxHeightValueVariant_Initial;

struct AzLayoutMaxHeightValueVariant_Exact { AzLayoutMaxHeightValueTag tag; AzLayoutMaxHeight payload; };
typedef struct AzLayoutMaxHeightValueVariant_Exact AzLayoutMaxHeightValueVariant_Exact;


union AzLayoutMaxHeightValue {
    AzLayoutMaxHeightValueVariant_Auto Auto;
    AzLayoutMaxHeightValueVariant_None None;
    AzLayoutMaxHeightValueVariant_Inherit Inherit;
    AzLayoutMaxHeightValueVariant_Initial Initial;
    AzLayoutMaxHeightValueVariant_Exact Exact;
};
typedef union AzLayoutMaxHeightValue AzLayoutMaxHeightValue;

enum AzLayoutMaxWidthValueTag {
   AzLayoutMaxWidthValueTag_Auto,
   AzLayoutMaxWidthValueTag_None,
   AzLayoutMaxWidthValueTag_Inherit,
   AzLayoutMaxWidthValueTag_Initial,
   AzLayoutMaxWidthValueTag_Exact,
};
typedef enum AzLayoutMaxWidthValueTag AzLayoutMaxWidthValueTag;

struct AzLayoutMaxWidthValueVariant_Auto { AzLayoutMaxWidthValueTag tag; };
typedef struct AzLayoutMaxWidthValueVariant_Auto AzLayoutMaxWidthValueVariant_Auto;

struct AzLayoutMaxWidthValueVariant_None { AzLayoutMaxWidthValueTag tag; };
typedef struct AzLayoutMaxWidthValueVariant_None AzLayoutMaxWidthValueVariant_None;

struct AzLayoutMaxWidthValueVariant_Inherit { AzLayoutMaxWidthValueTag tag; };
typedef struct AzLayoutMaxWidthValueVariant_Inherit AzLayoutMaxWidthValueVariant_Inherit;

struct AzLayoutMaxWidthValueVariant_Initial { AzLayoutMaxWidthValueTag tag; };
typedef struct AzLayoutMaxWidthValueVariant_Initial AzLayoutMaxWidthValueVariant_Initial;

struct AzLayoutMaxWidthValueVariant_Exact { AzLayoutMaxWidthValueTag tag; AzLayoutMaxWidth payload; };
typedef struct AzLayoutMaxWidthValueVariant_Exact AzLayoutMaxWidthValueVariant_Exact;


union AzLayoutMaxWidthValue {
    AzLayoutMaxWidthValueVariant_Auto Auto;
    AzLayoutMaxWidthValueVariant_None None;
    AzLayoutMaxWidthValueVariant_Inherit Inherit;
    AzLayoutMaxWidthValueVariant_Initial Initial;
    AzLayoutMaxWidthValueVariant_Exact Exact;
};
typedef union AzLayoutMaxWidthValue AzLayoutMaxWidthValue;

enum AzLayoutMinHeightValueTag {
   AzLayoutMinHeightValueTag_Auto,
   AzLayoutMinHeightValueTag_None,
   AzLayoutMinHeightValueTag_Inherit,
   AzLayoutMinHeightValueTag_Initial,
   AzLayoutMinHeightValueTag_Exact,
};
typedef enum AzLayoutMinHeightValueTag AzLayoutMinHeightValueTag;

struct AzLayoutMinHeightValueVariant_Auto { AzLayoutMinHeightValueTag tag; };
typedef struct AzLayoutMinHeightValueVariant_Auto AzLayoutMinHeightValueVariant_Auto;

struct AzLayoutMinHeightValueVariant_None { AzLayoutMinHeightValueTag tag; };
typedef struct AzLayoutMinHeightValueVariant_None AzLayoutMinHeightValueVariant_None;

struct AzLayoutMinHeightValueVariant_Inherit { AzLayoutMinHeightValueTag tag; };
typedef struct AzLayoutMinHeightValueVariant_Inherit AzLayoutMinHeightValueVariant_Inherit;

struct AzLayoutMinHeightValueVariant_Initial { AzLayoutMinHeightValueTag tag; };
typedef struct AzLayoutMinHeightValueVariant_Initial AzLayoutMinHeightValueVariant_Initial;

struct AzLayoutMinHeightValueVariant_Exact { AzLayoutMinHeightValueTag tag; AzLayoutMinHeight payload; };
typedef struct AzLayoutMinHeightValueVariant_Exact AzLayoutMinHeightValueVariant_Exact;


union AzLayoutMinHeightValue {
    AzLayoutMinHeightValueVariant_Auto Auto;
    AzLayoutMinHeightValueVariant_None None;
    AzLayoutMinHeightValueVariant_Inherit Inherit;
    AzLayoutMinHeightValueVariant_Initial Initial;
    AzLayoutMinHeightValueVariant_Exact Exact;
};
typedef union AzLayoutMinHeightValue AzLayoutMinHeightValue;

enum AzLayoutMinWidthValueTag {
   AzLayoutMinWidthValueTag_Auto,
   AzLayoutMinWidthValueTag_None,
   AzLayoutMinWidthValueTag_Inherit,
   AzLayoutMinWidthValueTag_Initial,
   AzLayoutMinWidthValueTag_Exact,
};
typedef enum AzLayoutMinWidthValueTag AzLayoutMinWidthValueTag;

struct AzLayoutMinWidthValueVariant_Auto { AzLayoutMinWidthValueTag tag; };
typedef struct AzLayoutMinWidthValueVariant_Auto AzLayoutMinWidthValueVariant_Auto;

struct AzLayoutMinWidthValueVariant_None { AzLayoutMinWidthValueTag tag; };
typedef struct AzLayoutMinWidthValueVariant_None AzLayoutMinWidthValueVariant_None;

struct AzLayoutMinWidthValueVariant_Inherit { AzLayoutMinWidthValueTag tag; };
typedef struct AzLayoutMinWidthValueVariant_Inherit AzLayoutMinWidthValueVariant_Inherit;

struct AzLayoutMinWidthValueVariant_Initial { AzLayoutMinWidthValueTag tag; };
typedef struct AzLayoutMinWidthValueVariant_Initial AzLayoutMinWidthValueVariant_Initial;

struct AzLayoutMinWidthValueVariant_Exact { AzLayoutMinWidthValueTag tag; AzLayoutMinWidth payload; };
typedef struct AzLayoutMinWidthValueVariant_Exact AzLayoutMinWidthValueVariant_Exact;


union AzLayoutMinWidthValue {
    AzLayoutMinWidthValueVariant_Auto Auto;
    AzLayoutMinWidthValueVariant_None None;
    AzLayoutMinWidthValueVariant_Inherit Inherit;
    AzLayoutMinWidthValueVariant_Initial Initial;
    AzLayoutMinWidthValueVariant_Exact Exact;
};
typedef union AzLayoutMinWidthValue AzLayoutMinWidthValue;

enum AzLayoutPaddingBottomValueTag {
   AzLayoutPaddingBottomValueTag_Auto,
   AzLayoutPaddingBottomValueTag_None,
   AzLayoutPaddingBottomValueTag_Inherit,
   AzLayoutPaddingBottomValueTag_Initial,
   AzLayoutPaddingBottomValueTag_Exact,
};
typedef enum AzLayoutPaddingBottomValueTag AzLayoutPaddingBottomValueTag;

struct AzLayoutPaddingBottomValueVariant_Auto { AzLayoutPaddingBottomValueTag tag; };
typedef struct AzLayoutPaddingBottomValueVariant_Auto AzLayoutPaddingBottomValueVariant_Auto;

struct AzLayoutPaddingBottomValueVariant_None { AzLayoutPaddingBottomValueTag tag; };
typedef struct AzLayoutPaddingBottomValueVariant_None AzLayoutPaddingBottomValueVariant_None;

struct AzLayoutPaddingBottomValueVariant_Inherit { AzLayoutPaddingBottomValueTag tag; };
typedef struct AzLayoutPaddingBottomValueVariant_Inherit AzLayoutPaddingBottomValueVariant_Inherit;

struct AzLayoutPaddingBottomValueVariant_Initial { AzLayoutPaddingBottomValueTag tag; };
typedef struct AzLayoutPaddingBottomValueVariant_Initial AzLayoutPaddingBottomValueVariant_Initial;

struct AzLayoutPaddingBottomValueVariant_Exact { AzLayoutPaddingBottomValueTag tag; AzLayoutPaddingBottom payload; };
typedef struct AzLayoutPaddingBottomValueVariant_Exact AzLayoutPaddingBottomValueVariant_Exact;


union AzLayoutPaddingBottomValue {
    AzLayoutPaddingBottomValueVariant_Auto Auto;
    AzLayoutPaddingBottomValueVariant_None None;
    AzLayoutPaddingBottomValueVariant_Inherit Inherit;
    AzLayoutPaddingBottomValueVariant_Initial Initial;
    AzLayoutPaddingBottomValueVariant_Exact Exact;
};
typedef union AzLayoutPaddingBottomValue AzLayoutPaddingBottomValue;

enum AzLayoutPaddingLeftValueTag {
   AzLayoutPaddingLeftValueTag_Auto,
   AzLayoutPaddingLeftValueTag_None,
   AzLayoutPaddingLeftValueTag_Inherit,
   AzLayoutPaddingLeftValueTag_Initial,
   AzLayoutPaddingLeftValueTag_Exact,
};
typedef enum AzLayoutPaddingLeftValueTag AzLayoutPaddingLeftValueTag;

struct AzLayoutPaddingLeftValueVariant_Auto { AzLayoutPaddingLeftValueTag tag; };
typedef struct AzLayoutPaddingLeftValueVariant_Auto AzLayoutPaddingLeftValueVariant_Auto;

struct AzLayoutPaddingLeftValueVariant_None { AzLayoutPaddingLeftValueTag tag; };
typedef struct AzLayoutPaddingLeftValueVariant_None AzLayoutPaddingLeftValueVariant_None;

struct AzLayoutPaddingLeftValueVariant_Inherit { AzLayoutPaddingLeftValueTag tag; };
typedef struct AzLayoutPaddingLeftValueVariant_Inherit AzLayoutPaddingLeftValueVariant_Inherit;

struct AzLayoutPaddingLeftValueVariant_Initial { AzLayoutPaddingLeftValueTag tag; };
typedef struct AzLayoutPaddingLeftValueVariant_Initial AzLayoutPaddingLeftValueVariant_Initial;

struct AzLayoutPaddingLeftValueVariant_Exact { AzLayoutPaddingLeftValueTag tag; AzLayoutPaddingLeft payload; };
typedef struct AzLayoutPaddingLeftValueVariant_Exact AzLayoutPaddingLeftValueVariant_Exact;


union AzLayoutPaddingLeftValue {
    AzLayoutPaddingLeftValueVariant_Auto Auto;
    AzLayoutPaddingLeftValueVariant_None None;
    AzLayoutPaddingLeftValueVariant_Inherit Inherit;
    AzLayoutPaddingLeftValueVariant_Initial Initial;
    AzLayoutPaddingLeftValueVariant_Exact Exact;
};
typedef union AzLayoutPaddingLeftValue AzLayoutPaddingLeftValue;

enum AzLayoutPaddingRightValueTag {
   AzLayoutPaddingRightValueTag_Auto,
   AzLayoutPaddingRightValueTag_None,
   AzLayoutPaddingRightValueTag_Inherit,
   AzLayoutPaddingRightValueTag_Initial,
   AzLayoutPaddingRightValueTag_Exact,
};
typedef enum AzLayoutPaddingRightValueTag AzLayoutPaddingRightValueTag;

struct AzLayoutPaddingRightValueVariant_Auto { AzLayoutPaddingRightValueTag tag; };
typedef struct AzLayoutPaddingRightValueVariant_Auto AzLayoutPaddingRightValueVariant_Auto;

struct AzLayoutPaddingRightValueVariant_None { AzLayoutPaddingRightValueTag tag; };
typedef struct AzLayoutPaddingRightValueVariant_None AzLayoutPaddingRightValueVariant_None;

struct AzLayoutPaddingRightValueVariant_Inherit { AzLayoutPaddingRightValueTag tag; };
typedef struct AzLayoutPaddingRightValueVariant_Inherit AzLayoutPaddingRightValueVariant_Inherit;

struct AzLayoutPaddingRightValueVariant_Initial { AzLayoutPaddingRightValueTag tag; };
typedef struct AzLayoutPaddingRightValueVariant_Initial AzLayoutPaddingRightValueVariant_Initial;

struct AzLayoutPaddingRightValueVariant_Exact { AzLayoutPaddingRightValueTag tag; AzLayoutPaddingRight payload; };
typedef struct AzLayoutPaddingRightValueVariant_Exact AzLayoutPaddingRightValueVariant_Exact;


union AzLayoutPaddingRightValue {
    AzLayoutPaddingRightValueVariant_Auto Auto;
    AzLayoutPaddingRightValueVariant_None None;
    AzLayoutPaddingRightValueVariant_Inherit Inherit;
    AzLayoutPaddingRightValueVariant_Initial Initial;
    AzLayoutPaddingRightValueVariant_Exact Exact;
};
typedef union AzLayoutPaddingRightValue AzLayoutPaddingRightValue;

enum AzLayoutPaddingTopValueTag {
   AzLayoutPaddingTopValueTag_Auto,
   AzLayoutPaddingTopValueTag_None,
   AzLayoutPaddingTopValueTag_Inherit,
   AzLayoutPaddingTopValueTag_Initial,
   AzLayoutPaddingTopValueTag_Exact,
};
typedef enum AzLayoutPaddingTopValueTag AzLayoutPaddingTopValueTag;

struct AzLayoutPaddingTopValueVariant_Auto { AzLayoutPaddingTopValueTag tag; };
typedef struct AzLayoutPaddingTopValueVariant_Auto AzLayoutPaddingTopValueVariant_Auto;

struct AzLayoutPaddingTopValueVariant_None { AzLayoutPaddingTopValueTag tag; };
typedef struct AzLayoutPaddingTopValueVariant_None AzLayoutPaddingTopValueVariant_None;

struct AzLayoutPaddingTopValueVariant_Inherit { AzLayoutPaddingTopValueTag tag; };
typedef struct AzLayoutPaddingTopValueVariant_Inherit AzLayoutPaddingTopValueVariant_Inherit;

struct AzLayoutPaddingTopValueVariant_Initial { AzLayoutPaddingTopValueTag tag; };
typedef struct AzLayoutPaddingTopValueVariant_Initial AzLayoutPaddingTopValueVariant_Initial;

struct AzLayoutPaddingTopValueVariant_Exact { AzLayoutPaddingTopValueTag tag; AzLayoutPaddingTop payload; };
typedef struct AzLayoutPaddingTopValueVariant_Exact AzLayoutPaddingTopValueVariant_Exact;


union AzLayoutPaddingTopValue {
    AzLayoutPaddingTopValueVariant_Auto Auto;
    AzLayoutPaddingTopValueVariant_None None;
    AzLayoutPaddingTopValueVariant_Inherit Inherit;
    AzLayoutPaddingTopValueVariant_Initial Initial;
    AzLayoutPaddingTopValueVariant_Exact Exact;
};
typedef union AzLayoutPaddingTopValue AzLayoutPaddingTopValue;

enum AzLayoutPositionValueTag {
   AzLayoutPositionValueTag_Auto,
   AzLayoutPositionValueTag_None,
   AzLayoutPositionValueTag_Inherit,
   AzLayoutPositionValueTag_Initial,
   AzLayoutPositionValueTag_Exact,
};
typedef enum AzLayoutPositionValueTag AzLayoutPositionValueTag;

struct AzLayoutPositionValueVariant_Auto { AzLayoutPositionValueTag tag; };
typedef struct AzLayoutPositionValueVariant_Auto AzLayoutPositionValueVariant_Auto;

struct AzLayoutPositionValueVariant_None { AzLayoutPositionValueTag tag; };
typedef struct AzLayoutPositionValueVariant_None AzLayoutPositionValueVariant_None;

struct AzLayoutPositionValueVariant_Inherit { AzLayoutPositionValueTag tag; };
typedef struct AzLayoutPositionValueVariant_Inherit AzLayoutPositionValueVariant_Inherit;

struct AzLayoutPositionValueVariant_Initial { AzLayoutPositionValueTag tag; };
typedef struct AzLayoutPositionValueVariant_Initial AzLayoutPositionValueVariant_Initial;

struct AzLayoutPositionValueVariant_Exact { AzLayoutPositionValueTag tag; AzLayoutPosition payload; };
typedef struct AzLayoutPositionValueVariant_Exact AzLayoutPositionValueVariant_Exact;


union AzLayoutPositionValue {
    AzLayoutPositionValueVariant_Auto Auto;
    AzLayoutPositionValueVariant_None None;
    AzLayoutPositionValueVariant_Inherit Inherit;
    AzLayoutPositionValueVariant_Initial Initial;
    AzLayoutPositionValueVariant_Exact Exact;
};
typedef union AzLayoutPositionValue AzLayoutPositionValue;

enum AzLayoutRightValueTag {
   AzLayoutRightValueTag_Auto,
   AzLayoutRightValueTag_None,
   AzLayoutRightValueTag_Inherit,
   AzLayoutRightValueTag_Initial,
   AzLayoutRightValueTag_Exact,
};
typedef enum AzLayoutRightValueTag AzLayoutRightValueTag;

struct AzLayoutRightValueVariant_Auto { AzLayoutRightValueTag tag; };
typedef struct AzLayoutRightValueVariant_Auto AzLayoutRightValueVariant_Auto;

struct AzLayoutRightValueVariant_None { AzLayoutRightValueTag tag; };
typedef struct AzLayoutRightValueVariant_None AzLayoutRightValueVariant_None;

struct AzLayoutRightValueVariant_Inherit { AzLayoutRightValueTag tag; };
typedef struct AzLayoutRightValueVariant_Inherit AzLayoutRightValueVariant_Inherit;

struct AzLayoutRightValueVariant_Initial { AzLayoutRightValueTag tag; };
typedef struct AzLayoutRightValueVariant_Initial AzLayoutRightValueVariant_Initial;

struct AzLayoutRightValueVariant_Exact { AzLayoutRightValueTag tag; AzLayoutRight payload; };
typedef struct AzLayoutRightValueVariant_Exact AzLayoutRightValueVariant_Exact;


union AzLayoutRightValue {
    AzLayoutRightValueVariant_Auto Auto;
    AzLayoutRightValueVariant_None None;
    AzLayoutRightValueVariant_Inherit Inherit;
    AzLayoutRightValueVariant_Initial Initial;
    AzLayoutRightValueVariant_Exact Exact;
};
typedef union AzLayoutRightValue AzLayoutRightValue;

enum AzLayoutTopValueTag {
   AzLayoutTopValueTag_Auto,
   AzLayoutTopValueTag_None,
   AzLayoutTopValueTag_Inherit,
   AzLayoutTopValueTag_Initial,
   AzLayoutTopValueTag_Exact,
};
typedef enum AzLayoutTopValueTag AzLayoutTopValueTag;

struct AzLayoutTopValueVariant_Auto { AzLayoutTopValueTag tag; };
typedef struct AzLayoutTopValueVariant_Auto AzLayoutTopValueVariant_Auto;

struct AzLayoutTopValueVariant_None { AzLayoutTopValueTag tag; };
typedef struct AzLayoutTopValueVariant_None AzLayoutTopValueVariant_None;

struct AzLayoutTopValueVariant_Inherit { AzLayoutTopValueTag tag; };
typedef struct AzLayoutTopValueVariant_Inherit AzLayoutTopValueVariant_Inherit;

struct AzLayoutTopValueVariant_Initial { AzLayoutTopValueTag tag; };
typedef struct AzLayoutTopValueVariant_Initial AzLayoutTopValueVariant_Initial;

struct AzLayoutTopValueVariant_Exact { AzLayoutTopValueTag tag; AzLayoutTop payload; };
typedef struct AzLayoutTopValueVariant_Exact AzLayoutTopValueVariant_Exact;


union AzLayoutTopValue {
    AzLayoutTopValueVariant_Auto Auto;
    AzLayoutTopValueVariant_None None;
    AzLayoutTopValueVariant_Inherit Inherit;
    AzLayoutTopValueVariant_Initial Initial;
    AzLayoutTopValueVariant_Exact Exact;
};
typedef union AzLayoutTopValue AzLayoutTopValue;

enum AzLayoutWidthValueTag {
   AzLayoutWidthValueTag_Auto,
   AzLayoutWidthValueTag_None,
   AzLayoutWidthValueTag_Inherit,
   AzLayoutWidthValueTag_Initial,
   AzLayoutWidthValueTag_Exact,
};
typedef enum AzLayoutWidthValueTag AzLayoutWidthValueTag;

struct AzLayoutWidthValueVariant_Auto { AzLayoutWidthValueTag tag; };
typedef struct AzLayoutWidthValueVariant_Auto AzLayoutWidthValueVariant_Auto;

struct AzLayoutWidthValueVariant_None { AzLayoutWidthValueTag tag; };
typedef struct AzLayoutWidthValueVariant_None AzLayoutWidthValueVariant_None;

struct AzLayoutWidthValueVariant_Inherit { AzLayoutWidthValueTag tag; };
typedef struct AzLayoutWidthValueVariant_Inherit AzLayoutWidthValueVariant_Inherit;

struct AzLayoutWidthValueVariant_Initial { AzLayoutWidthValueTag tag; };
typedef struct AzLayoutWidthValueVariant_Initial AzLayoutWidthValueVariant_Initial;

struct AzLayoutWidthValueVariant_Exact { AzLayoutWidthValueTag tag; AzLayoutWidth payload; };
typedef struct AzLayoutWidthValueVariant_Exact AzLayoutWidthValueVariant_Exact;


union AzLayoutWidthValue {
    AzLayoutWidthValueVariant_Auto Auto;
    AzLayoutWidthValueVariant_None None;
    AzLayoutWidthValueVariant_Inherit Inherit;
    AzLayoutWidthValueVariant_Initial Initial;
    AzLayoutWidthValueVariant_Exact Exact;
};
typedef union AzLayoutWidthValue AzLayoutWidthValue;

enum AzLayoutFlexWrapValueTag {
   AzLayoutFlexWrapValueTag_Auto,
   AzLayoutFlexWrapValueTag_None,
   AzLayoutFlexWrapValueTag_Inherit,
   AzLayoutFlexWrapValueTag_Initial,
   AzLayoutFlexWrapValueTag_Exact,
};
typedef enum AzLayoutFlexWrapValueTag AzLayoutFlexWrapValueTag;

struct AzLayoutFlexWrapValueVariant_Auto { AzLayoutFlexWrapValueTag tag; };
typedef struct AzLayoutFlexWrapValueVariant_Auto AzLayoutFlexWrapValueVariant_Auto;

struct AzLayoutFlexWrapValueVariant_None { AzLayoutFlexWrapValueTag tag; };
typedef struct AzLayoutFlexWrapValueVariant_None AzLayoutFlexWrapValueVariant_None;

struct AzLayoutFlexWrapValueVariant_Inherit { AzLayoutFlexWrapValueTag tag; };
typedef struct AzLayoutFlexWrapValueVariant_Inherit AzLayoutFlexWrapValueVariant_Inherit;

struct AzLayoutFlexWrapValueVariant_Initial { AzLayoutFlexWrapValueTag tag; };
typedef struct AzLayoutFlexWrapValueVariant_Initial AzLayoutFlexWrapValueVariant_Initial;

struct AzLayoutFlexWrapValueVariant_Exact { AzLayoutFlexWrapValueTag tag; AzLayoutFlexWrap payload; };
typedef struct AzLayoutFlexWrapValueVariant_Exact AzLayoutFlexWrapValueVariant_Exact;


union AzLayoutFlexWrapValue {
    AzLayoutFlexWrapValueVariant_Auto Auto;
    AzLayoutFlexWrapValueVariant_None None;
    AzLayoutFlexWrapValueVariant_Inherit Inherit;
    AzLayoutFlexWrapValueVariant_Initial Initial;
    AzLayoutFlexWrapValueVariant_Exact Exact;
};
typedef union AzLayoutFlexWrapValue AzLayoutFlexWrapValue;

enum AzLayoutOverflowValueTag {
   AzLayoutOverflowValueTag_Auto,
   AzLayoutOverflowValueTag_None,
   AzLayoutOverflowValueTag_Inherit,
   AzLayoutOverflowValueTag_Initial,
   AzLayoutOverflowValueTag_Exact,
};
typedef enum AzLayoutOverflowValueTag AzLayoutOverflowValueTag;

struct AzLayoutOverflowValueVariant_Auto { AzLayoutOverflowValueTag tag; };
typedef struct AzLayoutOverflowValueVariant_Auto AzLayoutOverflowValueVariant_Auto;

struct AzLayoutOverflowValueVariant_None { AzLayoutOverflowValueTag tag; };
typedef struct AzLayoutOverflowValueVariant_None AzLayoutOverflowValueVariant_None;

struct AzLayoutOverflowValueVariant_Inherit { AzLayoutOverflowValueTag tag; };
typedef struct AzLayoutOverflowValueVariant_Inherit AzLayoutOverflowValueVariant_Inherit;

struct AzLayoutOverflowValueVariant_Initial { AzLayoutOverflowValueTag tag; };
typedef struct AzLayoutOverflowValueVariant_Initial AzLayoutOverflowValueVariant_Initial;

struct AzLayoutOverflowValueVariant_Exact { AzLayoutOverflowValueTag tag; AzLayoutOverflow payload; };
typedef struct AzLayoutOverflowValueVariant_Exact AzLayoutOverflowValueVariant_Exact;


union AzLayoutOverflowValue {
    AzLayoutOverflowValueVariant_Auto Auto;
    AzLayoutOverflowValueVariant_None None;
    AzLayoutOverflowValueVariant_Inherit Inherit;
    AzLayoutOverflowValueVariant_Initial Initial;
    AzLayoutOverflowValueVariant_Exact Exact;
};
typedef union AzLayoutOverflowValue AzLayoutOverflowValue;

enum AzStyleBorderBottomColorValueTag {
   AzStyleBorderBottomColorValueTag_Auto,
   AzStyleBorderBottomColorValueTag_None,
   AzStyleBorderBottomColorValueTag_Inherit,
   AzStyleBorderBottomColorValueTag_Initial,
   AzStyleBorderBottomColorValueTag_Exact,
};
typedef enum AzStyleBorderBottomColorValueTag AzStyleBorderBottomColorValueTag;

struct AzStyleBorderBottomColorValueVariant_Auto { AzStyleBorderBottomColorValueTag tag; };
typedef struct AzStyleBorderBottomColorValueVariant_Auto AzStyleBorderBottomColorValueVariant_Auto;

struct AzStyleBorderBottomColorValueVariant_None { AzStyleBorderBottomColorValueTag tag; };
typedef struct AzStyleBorderBottomColorValueVariant_None AzStyleBorderBottomColorValueVariant_None;

struct AzStyleBorderBottomColorValueVariant_Inherit { AzStyleBorderBottomColorValueTag tag; };
typedef struct AzStyleBorderBottomColorValueVariant_Inherit AzStyleBorderBottomColorValueVariant_Inherit;

struct AzStyleBorderBottomColorValueVariant_Initial { AzStyleBorderBottomColorValueTag tag; };
typedef struct AzStyleBorderBottomColorValueVariant_Initial AzStyleBorderBottomColorValueVariant_Initial;

struct AzStyleBorderBottomColorValueVariant_Exact { AzStyleBorderBottomColorValueTag tag; AzStyleBorderBottomColor payload; };
typedef struct AzStyleBorderBottomColorValueVariant_Exact AzStyleBorderBottomColorValueVariant_Exact;


union AzStyleBorderBottomColorValue {
    AzStyleBorderBottomColorValueVariant_Auto Auto;
    AzStyleBorderBottomColorValueVariant_None None;
    AzStyleBorderBottomColorValueVariant_Inherit Inherit;
    AzStyleBorderBottomColorValueVariant_Initial Initial;
    AzStyleBorderBottomColorValueVariant_Exact Exact;
};
typedef union AzStyleBorderBottomColorValue AzStyleBorderBottomColorValue;

enum AzStyleBorderBottomLeftRadiusValueTag {
   AzStyleBorderBottomLeftRadiusValueTag_Auto,
   AzStyleBorderBottomLeftRadiusValueTag_None,
   AzStyleBorderBottomLeftRadiusValueTag_Inherit,
   AzStyleBorderBottomLeftRadiusValueTag_Initial,
   AzStyleBorderBottomLeftRadiusValueTag_Exact,
};
typedef enum AzStyleBorderBottomLeftRadiusValueTag AzStyleBorderBottomLeftRadiusValueTag;

struct AzStyleBorderBottomLeftRadiusValueVariant_Auto { AzStyleBorderBottomLeftRadiusValueTag tag; };
typedef struct AzStyleBorderBottomLeftRadiusValueVariant_Auto AzStyleBorderBottomLeftRadiusValueVariant_Auto;

struct AzStyleBorderBottomLeftRadiusValueVariant_None { AzStyleBorderBottomLeftRadiusValueTag tag; };
typedef struct AzStyleBorderBottomLeftRadiusValueVariant_None AzStyleBorderBottomLeftRadiusValueVariant_None;

struct AzStyleBorderBottomLeftRadiusValueVariant_Inherit { AzStyleBorderBottomLeftRadiusValueTag tag; };
typedef struct AzStyleBorderBottomLeftRadiusValueVariant_Inherit AzStyleBorderBottomLeftRadiusValueVariant_Inherit;

struct AzStyleBorderBottomLeftRadiusValueVariant_Initial { AzStyleBorderBottomLeftRadiusValueTag tag; };
typedef struct AzStyleBorderBottomLeftRadiusValueVariant_Initial AzStyleBorderBottomLeftRadiusValueVariant_Initial;

struct AzStyleBorderBottomLeftRadiusValueVariant_Exact { AzStyleBorderBottomLeftRadiusValueTag tag; AzStyleBorderBottomLeftRadius payload; };
typedef struct AzStyleBorderBottomLeftRadiusValueVariant_Exact AzStyleBorderBottomLeftRadiusValueVariant_Exact;


union AzStyleBorderBottomLeftRadiusValue {
    AzStyleBorderBottomLeftRadiusValueVariant_Auto Auto;
    AzStyleBorderBottomLeftRadiusValueVariant_None None;
    AzStyleBorderBottomLeftRadiusValueVariant_Inherit Inherit;
    AzStyleBorderBottomLeftRadiusValueVariant_Initial Initial;
    AzStyleBorderBottomLeftRadiusValueVariant_Exact Exact;
};
typedef union AzStyleBorderBottomLeftRadiusValue AzStyleBorderBottomLeftRadiusValue;

enum AzStyleBorderBottomRightRadiusValueTag {
   AzStyleBorderBottomRightRadiusValueTag_Auto,
   AzStyleBorderBottomRightRadiusValueTag_None,
   AzStyleBorderBottomRightRadiusValueTag_Inherit,
   AzStyleBorderBottomRightRadiusValueTag_Initial,
   AzStyleBorderBottomRightRadiusValueTag_Exact,
};
typedef enum AzStyleBorderBottomRightRadiusValueTag AzStyleBorderBottomRightRadiusValueTag;

struct AzStyleBorderBottomRightRadiusValueVariant_Auto { AzStyleBorderBottomRightRadiusValueTag tag; };
typedef struct AzStyleBorderBottomRightRadiusValueVariant_Auto AzStyleBorderBottomRightRadiusValueVariant_Auto;

struct AzStyleBorderBottomRightRadiusValueVariant_None { AzStyleBorderBottomRightRadiusValueTag tag; };
typedef struct AzStyleBorderBottomRightRadiusValueVariant_None AzStyleBorderBottomRightRadiusValueVariant_None;

struct AzStyleBorderBottomRightRadiusValueVariant_Inherit { AzStyleBorderBottomRightRadiusValueTag tag; };
typedef struct AzStyleBorderBottomRightRadiusValueVariant_Inherit AzStyleBorderBottomRightRadiusValueVariant_Inherit;

struct AzStyleBorderBottomRightRadiusValueVariant_Initial { AzStyleBorderBottomRightRadiusValueTag tag; };
typedef struct AzStyleBorderBottomRightRadiusValueVariant_Initial AzStyleBorderBottomRightRadiusValueVariant_Initial;

struct AzStyleBorderBottomRightRadiusValueVariant_Exact { AzStyleBorderBottomRightRadiusValueTag tag; AzStyleBorderBottomRightRadius payload; };
typedef struct AzStyleBorderBottomRightRadiusValueVariant_Exact AzStyleBorderBottomRightRadiusValueVariant_Exact;


union AzStyleBorderBottomRightRadiusValue {
    AzStyleBorderBottomRightRadiusValueVariant_Auto Auto;
    AzStyleBorderBottomRightRadiusValueVariant_None None;
    AzStyleBorderBottomRightRadiusValueVariant_Inherit Inherit;
    AzStyleBorderBottomRightRadiusValueVariant_Initial Initial;
    AzStyleBorderBottomRightRadiusValueVariant_Exact Exact;
};
typedef union AzStyleBorderBottomRightRadiusValue AzStyleBorderBottomRightRadiusValue;

enum AzStyleBorderBottomStyleValueTag {
   AzStyleBorderBottomStyleValueTag_Auto,
   AzStyleBorderBottomStyleValueTag_None,
   AzStyleBorderBottomStyleValueTag_Inherit,
   AzStyleBorderBottomStyleValueTag_Initial,
   AzStyleBorderBottomStyleValueTag_Exact,
};
typedef enum AzStyleBorderBottomStyleValueTag AzStyleBorderBottomStyleValueTag;

struct AzStyleBorderBottomStyleValueVariant_Auto { AzStyleBorderBottomStyleValueTag tag; };
typedef struct AzStyleBorderBottomStyleValueVariant_Auto AzStyleBorderBottomStyleValueVariant_Auto;

struct AzStyleBorderBottomStyleValueVariant_None { AzStyleBorderBottomStyleValueTag tag; };
typedef struct AzStyleBorderBottomStyleValueVariant_None AzStyleBorderBottomStyleValueVariant_None;

struct AzStyleBorderBottomStyleValueVariant_Inherit { AzStyleBorderBottomStyleValueTag tag; };
typedef struct AzStyleBorderBottomStyleValueVariant_Inherit AzStyleBorderBottomStyleValueVariant_Inherit;

struct AzStyleBorderBottomStyleValueVariant_Initial { AzStyleBorderBottomStyleValueTag tag; };
typedef struct AzStyleBorderBottomStyleValueVariant_Initial AzStyleBorderBottomStyleValueVariant_Initial;

struct AzStyleBorderBottomStyleValueVariant_Exact { AzStyleBorderBottomStyleValueTag tag; AzStyleBorderBottomStyle payload; };
typedef struct AzStyleBorderBottomStyleValueVariant_Exact AzStyleBorderBottomStyleValueVariant_Exact;


union AzStyleBorderBottomStyleValue {
    AzStyleBorderBottomStyleValueVariant_Auto Auto;
    AzStyleBorderBottomStyleValueVariant_None None;
    AzStyleBorderBottomStyleValueVariant_Inherit Inherit;
    AzStyleBorderBottomStyleValueVariant_Initial Initial;
    AzStyleBorderBottomStyleValueVariant_Exact Exact;
};
typedef union AzStyleBorderBottomStyleValue AzStyleBorderBottomStyleValue;

enum AzLayoutBorderBottomWidthValueTag {
   AzLayoutBorderBottomWidthValueTag_Auto,
   AzLayoutBorderBottomWidthValueTag_None,
   AzLayoutBorderBottomWidthValueTag_Inherit,
   AzLayoutBorderBottomWidthValueTag_Initial,
   AzLayoutBorderBottomWidthValueTag_Exact,
};
typedef enum AzLayoutBorderBottomWidthValueTag AzLayoutBorderBottomWidthValueTag;

struct AzLayoutBorderBottomWidthValueVariant_Auto { AzLayoutBorderBottomWidthValueTag tag; };
typedef struct AzLayoutBorderBottomWidthValueVariant_Auto AzLayoutBorderBottomWidthValueVariant_Auto;

struct AzLayoutBorderBottomWidthValueVariant_None { AzLayoutBorderBottomWidthValueTag tag; };
typedef struct AzLayoutBorderBottomWidthValueVariant_None AzLayoutBorderBottomWidthValueVariant_None;

struct AzLayoutBorderBottomWidthValueVariant_Inherit { AzLayoutBorderBottomWidthValueTag tag; };
typedef struct AzLayoutBorderBottomWidthValueVariant_Inherit AzLayoutBorderBottomWidthValueVariant_Inherit;

struct AzLayoutBorderBottomWidthValueVariant_Initial { AzLayoutBorderBottomWidthValueTag tag; };
typedef struct AzLayoutBorderBottomWidthValueVariant_Initial AzLayoutBorderBottomWidthValueVariant_Initial;

struct AzLayoutBorderBottomWidthValueVariant_Exact { AzLayoutBorderBottomWidthValueTag tag; AzLayoutBorderBottomWidth payload; };
typedef struct AzLayoutBorderBottomWidthValueVariant_Exact AzLayoutBorderBottomWidthValueVariant_Exact;


union AzLayoutBorderBottomWidthValue {
    AzLayoutBorderBottomWidthValueVariant_Auto Auto;
    AzLayoutBorderBottomWidthValueVariant_None None;
    AzLayoutBorderBottomWidthValueVariant_Inherit Inherit;
    AzLayoutBorderBottomWidthValueVariant_Initial Initial;
    AzLayoutBorderBottomWidthValueVariant_Exact Exact;
};
typedef union AzLayoutBorderBottomWidthValue AzLayoutBorderBottomWidthValue;

enum AzStyleBorderLeftColorValueTag {
   AzStyleBorderLeftColorValueTag_Auto,
   AzStyleBorderLeftColorValueTag_None,
   AzStyleBorderLeftColorValueTag_Inherit,
   AzStyleBorderLeftColorValueTag_Initial,
   AzStyleBorderLeftColorValueTag_Exact,
};
typedef enum AzStyleBorderLeftColorValueTag AzStyleBorderLeftColorValueTag;

struct AzStyleBorderLeftColorValueVariant_Auto { AzStyleBorderLeftColorValueTag tag; };
typedef struct AzStyleBorderLeftColorValueVariant_Auto AzStyleBorderLeftColorValueVariant_Auto;

struct AzStyleBorderLeftColorValueVariant_None { AzStyleBorderLeftColorValueTag tag; };
typedef struct AzStyleBorderLeftColorValueVariant_None AzStyleBorderLeftColorValueVariant_None;

struct AzStyleBorderLeftColorValueVariant_Inherit { AzStyleBorderLeftColorValueTag tag; };
typedef struct AzStyleBorderLeftColorValueVariant_Inherit AzStyleBorderLeftColorValueVariant_Inherit;

struct AzStyleBorderLeftColorValueVariant_Initial { AzStyleBorderLeftColorValueTag tag; };
typedef struct AzStyleBorderLeftColorValueVariant_Initial AzStyleBorderLeftColorValueVariant_Initial;

struct AzStyleBorderLeftColorValueVariant_Exact { AzStyleBorderLeftColorValueTag tag; AzStyleBorderLeftColor payload; };
typedef struct AzStyleBorderLeftColorValueVariant_Exact AzStyleBorderLeftColorValueVariant_Exact;


union AzStyleBorderLeftColorValue {
    AzStyleBorderLeftColorValueVariant_Auto Auto;
    AzStyleBorderLeftColorValueVariant_None None;
    AzStyleBorderLeftColorValueVariant_Inherit Inherit;
    AzStyleBorderLeftColorValueVariant_Initial Initial;
    AzStyleBorderLeftColorValueVariant_Exact Exact;
};
typedef union AzStyleBorderLeftColorValue AzStyleBorderLeftColorValue;

enum AzStyleBorderLeftStyleValueTag {
   AzStyleBorderLeftStyleValueTag_Auto,
   AzStyleBorderLeftStyleValueTag_None,
   AzStyleBorderLeftStyleValueTag_Inherit,
   AzStyleBorderLeftStyleValueTag_Initial,
   AzStyleBorderLeftStyleValueTag_Exact,
};
typedef enum AzStyleBorderLeftStyleValueTag AzStyleBorderLeftStyleValueTag;

struct AzStyleBorderLeftStyleValueVariant_Auto { AzStyleBorderLeftStyleValueTag tag; };
typedef struct AzStyleBorderLeftStyleValueVariant_Auto AzStyleBorderLeftStyleValueVariant_Auto;

struct AzStyleBorderLeftStyleValueVariant_None { AzStyleBorderLeftStyleValueTag tag; };
typedef struct AzStyleBorderLeftStyleValueVariant_None AzStyleBorderLeftStyleValueVariant_None;

struct AzStyleBorderLeftStyleValueVariant_Inherit { AzStyleBorderLeftStyleValueTag tag; };
typedef struct AzStyleBorderLeftStyleValueVariant_Inherit AzStyleBorderLeftStyleValueVariant_Inherit;

struct AzStyleBorderLeftStyleValueVariant_Initial { AzStyleBorderLeftStyleValueTag tag; };
typedef struct AzStyleBorderLeftStyleValueVariant_Initial AzStyleBorderLeftStyleValueVariant_Initial;

struct AzStyleBorderLeftStyleValueVariant_Exact { AzStyleBorderLeftStyleValueTag tag; AzStyleBorderLeftStyle payload; };
typedef struct AzStyleBorderLeftStyleValueVariant_Exact AzStyleBorderLeftStyleValueVariant_Exact;


union AzStyleBorderLeftStyleValue {
    AzStyleBorderLeftStyleValueVariant_Auto Auto;
    AzStyleBorderLeftStyleValueVariant_None None;
    AzStyleBorderLeftStyleValueVariant_Inherit Inherit;
    AzStyleBorderLeftStyleValueVariant_Initial Initial;
    AzStyleBorderLeftStyleValueVariant_Exact Exact;
};
typedef union AzStyleBorderLeftStyleValue AzStyleBorderLeftStyleValue;

enum AzLayoutBorderLeftWidthValueTag {
   AzLayoutBorderLeftWidthValueTag_Auto,
   AzLayoutBorderLeftWidthValueTag_None,
   AzLayoutBorderLeftWidthValueTag_Inherit,
   AzLayoutBorderLeftWidthValueTag_Initial,
   AzLayoutBorderLeftWidthValueTag_Exact,
};
typedef enum AzLayoutBorderLeftWidthValueTag AzLayoutBorderLeftWidthValueTag;

struct AzLayoutBorderLeftWidthValueVariant_Auto { AzLayoutBorderLeftWidthValueTag tag; };
typedef struct AzLayoutBorderLeftWidthValueVariant_Auto AzLayoutBorderLeftWidthValueVariant_Auto;

struct AzLayoutBorderLeftWidthValueVariant_None { AzLayoutBorderLeftWidthValueTag tag; };
typedef struct AzLayoutBorderLeftWidthValueVariant_None AzLayoutBorderLeftWidthValueVariant_None;

struct AzLayoutBorderLeftWidthValueVariant_Inherit { AzLayoutBorderLeftWidthValueTag tag; };
typedef struct AzLayoutBorderLeftWidthValueVariant_Inherit AzLayoutBorderLeftWidthValueVariant_Inherit;

struct AzLayoutBorderLeftWidthValueVariant_Initial { AzLayoutBorderLeftWidthValueTag tag; };
typedef struct AzLayoutBorderLeftWidthValueVariant_Initial AzLayoutBorderLeftWidthValueVariant_Initial;

struct AzLayoutBorderLeftWidthValueVariant_Exact { AzLayoutBorderLeftWidthValueTag tag; AzLayoutBorderLeftWidth payload; };
typedef struct AzLayoutBorderLeftWidthValueVariant_Exact AzLayoutBorderLeftWidthValueVariant_Exact;


union AzLayoutBorderLeftWidthValue {
    AzLayoutBorderLeftWidthValueVariant_Auto Auto;
    AzLayoutBorderLeftWidthValueVariant_None None;
    AzLayoutBorderLeftWidthValueVariant_Inherit Inherit;
    AzLayoutBorderLeftWidthValueVariant_Initial Initial;
    AzLayoutBorderLeftWidthValueVariant_Exact Exact;
};
typedef union AzLayoutBorderLeftWidthValue AzLayoutBorderLeftWidthValue;

enum AzStyleBorderRightColorValueTag {
   AzStyleBorderRightColorValueTag_Auto,
   AzStyleBorderRightColorValueTag_None,
   AzStyleBorderRightColorValueTag_Inherit,
   AzStyleBorderRightColorValueTag_Initial,
   AzStyleBorderRightColorValueTag_Exact,
};
typedef enum AzStyleBorderRightColorValueTag AzStyleBorderRightColorValueTag;

struct AzStyleBorderRightColorValueVariant_Auto { AzStyleBorderRightColorValueTag tag; };
typedef struct AzStyleBorderRightColorValueVariant_Auto AzStyleBorderRightColorValueVariant_Auto;

struct AzStyleBorderRightColorValueVariant_None { AzStyleBorderRightColorValueTag tag; };
typedef struct AzStyleBorderRightColorValueVariant_None AzStyleBorderRightColorValueVariant_None;

struct AzStyleBorderRightColorValueVariant_Inherit { AzStyleBorderRightColorValueTag tag; };
typedef struct AzStyleBorderRightColorValueVariant_Inherit AzStyleBorderRightColorValueVariant_Inherit;

struct AzStyleBorderRightColorValueVariant_Initial { AzStyleBorderRightColorValueTag tag; };
typedef struct AzStyleBorderRightColorValueVariant_Initial AzStyleBorderRightColorValueVariant_Initial;

struct AzStyleBorderRightColorValueVariant_Exact { AzStyleBorderRightColorValueTag tag; AzStyleBorderRightColor payload; };
typedef struct AzStyleBorderRightColorValueVariant_Exact AzStyleBorderRightColorValueVariant_Exact;


union AzStyleBorderRightColorValue {
    AzStyleBorderRightColorValueVariant_Auto Auto;
    AzStyleBorderRightColorValueVariant_None None;
    AzStyleBorderRightColorValueVariant_Inherit Inherit;
    AzStyleBorderRightColorValueVariant_Initial Initial;
    AzStyleBorderRightColorValueVariant_Exact Exact;
};
typedef union AzStyleBorderRightColorValue AzStyleBorderRightColorValue;

enum AzStyleBorderRightStyleValueTag {
   AzStyleBorderRightStyleValueTag_Auto,
   AzStyleBorderRightStyleValueTag_None,
   AzStyleBorderRightStyleValueTag_Inherit,
   AzStyleBorderRightStyleValueTag_Initial,
   AzStyleBorderRightStyleValueTag_Exact,
};
typedef enum AzStyleBorderRightStyleValueTag AzStyleBorderRightStyleValueTag;

struct AzStyleBorderRightStyleValueVariant_Auto { AzStyleBorderRightStyleValueTag tag; };
typedef struct AzStyleBorderRightStyleValueVariant_Auto AzStyleBorderRightStyleValueVariant_Auto;

struct AzStyleBorderRightStyleValueVariant_None { AzStyleBorderRightStyleValueTag tag; };
typedef struct AzStyleBorderRightStyleValueVariant_None AzStyleBorderRightStyleValueVariant_None;

struct AzStyleBorderRightStyleValueVariant_Inherit { AzStyleBorderRightStyleValueTag tag; };
typedef struct AzStyleBorderRightStyleValueVariant_Inherit AzStyleBorderRightStyleValueVariant_Inherit;

struct AzStyleBorderRightStyleValueVariant_Initial { AzStyleBorderRightStyleValueTag tag; };
typedef struct AzStyleBorderRightStyleValueVariant_Initial AzStyleBorderRightStyleValueVariant_Initial;

struct AzStyleBorderRightStyleValueVariant_Exact { AzStyleBorderRightStyleValueTag tag; AzStyleBorderRightStyle payload; };
typedef struct AzStyleBorderRightStyleValueVariant_Exact AzStyleBorderRightStyleValueVariant_Exact;


union AzStyleBorderRightStyleValue {
    AzStyleBorderRightStyleValueVariant_Auto Auto;
    AzStyleBorderRightStyleValueVariant_None None;
    AzStyleBorderRightStyleValueVariant_Inherit Inherit;
    AzStyleBorderRightStyleValueVariant_Initial Initial;
    AzStyleBorderRightStyleValueVariant_Exact Exact;
};
typedef union AzStyleBorderRightStyleValue AzStyleBorderRightStyleValue;

enum AzLayoutBorderRightWidthValueTag {
   AzLayoutBorderRightWidthValueTag_Auto,
   AzLayoutBorderRightWidthValueTag_None,
   AzLayoutBorderRightWidthValueTag_Inherit,
   AzLayoutBorderRightWidthValueTag_Initial,
   AzLayoutBorderRightWidthValueTag_Exact,
};
typedef enum AzLayoutBorderRightWidthValueTag AzLayoutBorderRightWidthValueTag;

struct AzLayoutBorderRightWidthValueVariant_Auto { AzLayoutBorderRightWidthValueTag tag; };
typedef struct AzLayoutBorderRightWidthValueVariant_Auto AzLayoutBorderRightWidthValueVariant_Auto;

struct AzLayoutBorderRightWidthValueVariant_None { AzLayoutBorderRightWidthValueTag tag; };
typedef struct AzLayoutBorderRightWidthValueVariant_None AzLayoutBorderRightWidthValueVariant_None;

struct AzLayoutBorderRightWidthValueVariant_Inherit { AzLayoutBorderRightWidthValueTag tag; };
typedef struct AzLayoutBorderRightWidthValueVariant_Inherit AzLayoutBorderRightWidthValueVariant_Inherit;

struct AzLayoutBorderRightWidthValueVariant_Initial { AzLayoutBorderRightWidthValueTag tag; };
typedef struct AzLayoutBorderRightWidthValueVariant_Initial AzLayoutBorderRightWidthValueVariant_Initial;

struct AzLayoutBorderRightWidthValueVariant_Exact { AzLayoutBorderRightWidthValueTag tag; AzLayoutBorderRightWidth payload; };
typedef struct AzLayoutBorderRightWidthValueVariant_Exact AzLayoutBorderRightWidthValueVariant_Exact;


union AzLayoutBorderRightWidthValue {
    AzLayoutBorderRightWidthValueVariant_Auto Auto;
    AzLayoutBorderRightWidthValueVariant_None None;
    AzLayoutBorderRightWidthValueVariant_Inherit Inherit;
    AzLayoutBorderRightWidthValueVariant_Initial Initial;
    AzLayoutBorderRightWidthValueVariant_Exact Exact;
};
typedef union AzLayoutBorderRightWidthValue AzLayoutBorderRightWidthValue;

enum AzStyleBorderTopColorValueTag {
   AzStyleBorderTopColorValueTag_Auto,
   AzStyleBorderTopColorValueTag_None,
   AzStyleBorderTopColorValueTag_Inherit,
   AzStyleBorderTopColorValueTag_Initial,
   AzStyleBorderTopColorValueTag_Exact,
};
typedef enum AzStyleBorderTopColorValueTag AzStyleBorderTopColorValueTag;

struct AzStyleBorderTopColorValueVariant_Auto { AzStyleBorderTopColorValueTag tag; };
typedef struct AzStyleBorderTopColorValueVariant_Auto AzStyleBorderTopColorValueVariant_Auto;

struct AzStyleBorderTopColorValueVariant_None { AzStyleBorderTopColorValueTag tag; };
typedef struct AzStyleBorderTopColorValueVariant_None AzStyleBorderTopColorValueVariant_None;

struct AzStyleBorderTopColorValueVariant_Inherit { AzStyleBorderTopColorValueTag tag; };
typedef struct AzStyleBorderTopColorValueVariant_Inherit AzStyleBorderTopColorValueVariant_Inherit;

struct AzStyleBorderTopColorValueVariant_Initial { AzStyleBorderTopColorValueTag tag; };
typedef struct AzStyleBorderTopColorValueVariant_Initial AzStyleBorderTopColorValueVariant_Initial;

struct AzStyleBorderTopColorValueVariant_Exact { AzStyleBorderTopColorValueTag tag; AzStyleBorderTopColor payload; };
typedef struct AzStyleBorderTopColorValueVariant_Exact AzStyleBorderTopColorValueVariant_Exact;


union AzStyleBorderTopColorValue {
    AzStyleBorderTopColorValueVariant_Auto Auto;
    AzStyleBorderTopColorValueVariant_None None;
    AzStyleBorderTopColorValueVariant_Inherit Inherit;
    AzStyleBorderTopColorValueVariant_Initial Initial;
    AzStyleBorderTopColorValueVariant_Exact Exact;
};
typedef union AzStyleBorderTopColorValue AzStyleBorderTopColorValue;

enum AzStyleBorderTopLeftRadiusValueTag {
   AzStyleBorderTopLeftRadiusValueTag_Auto,
   AzStyleBorderTopLeftRadiusValueTag_None,
   AzStyleBorderTopLeftRadiusValueTag_Inherit,
   AzStyleBorderTopLeftRadiusValueTag_Initial,
   AzStyleBorderTopLeftRadiusValueTag_Exact,
};
typedef enum AzStyleBorderTopLeftRadiusValueTag AzStyleBorderTopLeftRadiusValueTag;

struct AzStyleBorderTopLeftRadiusValueVariant_Auto { AzStyleBorderTopLeftRadiusValueTag tag; };
typedef struct AzStyleBorderTopLeftRadiusValueVariant_Auto AzStyleBorderTopLeftRadiusValueVariant_Auto;

struct AzStyleBorderTopLeftRadiusValueVariant_None { AzStyleBorderTopLeftRadiusValueTag tag; };
typedef struct AzStyleBorderTopLeftRadiusValueVariant_None AzStyleBorderTopLeftRadiusValueVariant_None;

struct AzStyleBorderTopLeftRadiusValueVariant_Inherit { AzStyleBorderTopLeftRadiusValueTag tag; };
typedef struct AzStyleBorderTopLeftRadiusValueVariant_Inherit AzStyleBorderTopLeftRadiusValueVariant_Inherit;

struct AzStyleBorderTopLeftRadiusValueVariant_Initial { AzStyleBorderTopLeftRadiusValueTag tag; };
typedef struct AzStyleBorderTopLeftRadiusValueVariant_Initial AzStyleBorderTopLeftRadiusValueVariant_Initial;

struct AzStyleBorderTopLeftRadiusValueVariant_Exact { AzStyleBorderTopLeftRadiusValueTag tag; AzStyleBorderTopLeftRadius payload; };
typedef struct AzStyleBorderTopLeftRadiusValueVariant_Exact AzStyleBorderTopLeftRadiusValueVariant_Exact;


union AzStyleBorderTopLeftRadiusValue {
    AzStyleBorderTopLeftRadiusValueVariant_Auto Auto;
    AzStyleBorderTopLeftRadiusValueVariant_None None;
    AzStyleBorderTopLeftRadiusValueVariant_Inherit Inherit;
    AzStyleBorderTopLeftRadiusValueVariant_Initial Initial;
    AzStyleBorderTopLeftRadiusValueVariant_Exact Exact;
};
typedef union AzStyleBorderTopLeftRadiusValue AzStyleBorderTopLeftRadiusValue;

enum AzStyleBorderTopRightRadiusValueTag {
   AzStyleBorderTopRightRadiusValueTag_Auto,
   AzStyleBorderTopRightRadiusValueTag_None,
   AzStyleBorderTopRightRadiusValueTag_Inherit,
   AzStyleBorderTopRightRadiusValueTag_Initial,
   AzStyleBorderTopRightRadiusValueTag_Exact,
};
typedef enum AzStyleBorderTopRightRadiusValueTag AzStyleBorderTopRightRadiusValueTag;

struct AzStyleBorderTopRightRadiusValueVariant_Auto { AzStyleBorderTopRightRadiusValueTag tag; };
typedef struct AzStyleBorderTopRightRadiusValueVariant_Auto AzStyleBorderTopRightRadiusValueVariant_Auto;

struct AzStyleBorderTopRightRadiusValueVariant_None { AzStyleBorderTopRightRadiusValueTag tag; };
typedef struct AzStyleBorderTopRightRadiusValueVariant_None AzStyleBorderTopRightRadiusValueVariant_None;

struct AzStyleBorderTopRightRadiusValueVariant_Inherit { AzStyleBorderTopRightRadiusValueTag tag; };
typedef struct AzStyleBorderTopRightRadiusValueVariant_Inherit AzStyleBorderTopRightRadiusValueVariant_Inherit;

struct AzStyleBorderTopRightRadiusValueVariant_Initial { AzStyleBorderTopRightRadiusValueTag tag; };
typedef struct AzStyleBorderTopRightRadiusValueVariant_Initial AzStyleBorderTopRightRadiusValueVariant_Initial;

struct AzStyleBorderTopRightRadiusValueVariant_Exact { AzStyleBorderTopRightRadiusValueTag tag; AzStyleBorderTopRightRadius payload; };
typedef struct AzStyleBorderTopRightRadiusValueVariant_Exact AzStyleBorderTopRightRadiusValueVariant_Exact;


union AzStyleBorderTopRightRadiusValue {
    AzStyleBorderTopRightRadiusValueVariant_Auto Auto;
    AzStyleBorderTopRightRadiusValueVariant_None None;
    AzStyleBorderTopRightRadiusValueVariant_Inherit Inherit;
    AzStyleBorderTopRightRadiusValueVariant_Initial Initial;
    AzStyleBorderTopRightRadiusValueVariant_Exact Exact;
};
typedef union AzStyleBorderTopRightRadiusValue AzStyleBorderTopRightRadiusValue;

enum AzStyleBorderTopStyleValueTag {
   AzStyleBorderTopStyleValueTag_Auto,
   AzStyleBorderTopStyleValueTag_None,
   AzStyleBorderTopStyleValueTag_Inherit,
   AzStyleBorderTopStyleValueTag_Initial,
   AzStyleBorderTopStyleValueTag_Exact,
};
typedef enum AzStyleBorderTopStyleValueTag AzStyleBorderTopStyleValueTag;

struct AzStyleBorderTopStyleValueVariant_Auto { AzStyleBorderTopStyleValueTag tag; };
typedef struct AzStyleBorderTopStyleValueVariant_Auto AzStyleBorderTopStyleValueVariant_Auto;

struct AzStyleBorderTopStyleValueVariant_None { AzStyleBorderTopStyleValueTag tag; };
typedef struct AzStyleBorderTopStyleValueVariant_None AzStyleBorderTopStyleValueVariant_None;

struct AzStyleBorderTopStyleValueVariant_Inherit { AzStyleBorderTopStyleValueTag tag; };
typedef struct AzStyleBorderTopStyleValueVariant_Inherit AzStyleBorderTopStyleValueVariant_Inherit;

struct AzStyleBorderTopStyleValueVariant_Initial { AzStyleBorderTopStyleValueTag tag; };
typedef struct AzStyleBorderTopStyleValueVariant_Initial AzStyleBorderTopStyleValueVariant_Initial;

struct AzStyleBorderTopStyleValueVariant_Exact { AzStyleBorderTopStyleValueTag tag; AzStyleBorderTopStyle payload; };
typedef struct AzStyleBorderTopStyleValueVariant_Exact AzStyleBorderTopStyleValueVariant_Exact;


union AzStyleBorderTopStyleValue {
    AzStyleBorderTopStyleValueVariant_Auto Auto;
    AzStyleBorderTopStyleValueVariant_None None;
    AzStyleBorderTopStyleValueVariant_Inherit Inherit;
    AzStyleBorderTopStyleValueVariant_Initial Initial;
    AzStyleBorderTopStyleValueVariant_Exact Exact;
};
typedef union AzStyleBorderTopStyleValue AzStyleBorderTopStyleValue;

enum AzLayoutBorderTopWidthValueTag {
   AzLayoutBorderTopWidthValueTag_Auto,
   AzLayoutBorderTopWidthValueTag_None,
   AzLayoutBorderTopWidthValueTag_Inherit,
   AzLayoutBorderTopWidthValueTag_Initial,
   AzLayoutBorderTopWidthValueTag_Exact,
};
typedef enum AzLayoutBorderTopWidthValueTag AzLayoutBorderTopWidthValueTag;

struct AzLayoutBorderTopWidthValueVariant_Auto { AzLayoutBorderTopWidthValueTag tag; };
typedef struct AzLayoutBorderTopWidthValueVariant_Auto AzLayoutBorderTopWidthValueVariant_Auto;

struct AzLayoutBorderTopWidthValueVariant_None { AzLayoutBorderTopWidthValueTag tag; };
typedef struct AzLayoutBorderTopWidthValueVariant_None AzLayoutBorderTopWidthValueVariant_None;

struct AzLayoutBorderTopWidthValueVariant_Inherit { AzLayoutBorderTopWidthValueTag tag; };
typedef struct AzLayoutBorderTopWidthValueVariant_Inherit AzLayoutBorderTopWidthValueVariant_Inherit;

struct AzLayoutBorderTopWidthValueVariant_Initial { AzLayoutBorderTopWidthValueTag tag; };
typedef struct AzLayoutBorderTopWidthValueVariant_Initial AzLayoutBorderTopWidthValueVariant_Initial;

struct AzLayoutBorderTopWidthValueVariant_Exact { AzLayoutBorderTopWidthValueTag tag; AzLayoutBorderTopWidth payload; };
typedef struct AzLayoutBorderTopWidthValueVariant_Exact AzLayoutBorderTopWidthValueVariant_Exact;


union AzLayoutBorderTopWidthValue {
    AzLayoutBorderTopWidthValueVariant_Auto Auto;
    AzLayoutBorderTopWidthValueVariant_None None;
    AzLayoutBorderTopWidthValueVariant_Inherit Inherit;
    AzLayoutBorderTopWidthValueVariant_Initial Initial;
    AzLayoutBorderTopWidthValueVariant_Exact Exact;
};
typedef union AzLayoutBorderTopWidthValue AzLayoutBorderTopWidthValue;

enum AzStyleCursorValueTag {
   AzStyleCursorValueTag_Auto,
   AzStyleCursorValueTag_None,
   AzStyleCursorValueTag_Inherit,
   AzStyleCursorValueTag_Initial,
   AzStyleCursorValueTag_Exact,
};
typedef enum AzStyleCursorValueTag AzStyleCursorValueTag;

struct AzStyleCursorValueVariant_Auto { AzStyleCursorValueTag tag; };
typedef struct AzStyleCursorValueVariant_Auto AzStyleCursorValueVariant_Auto;

struct AzStyleCursorValueVariant_None { AzStyleCursorValueTag tag; };
typedef struct AzStyleCursorValueVariant_None AzStyleCursorValueVariant_None;

struct AzStyleCursorValueVariant_Inherit { AzStyleCursorValueTag tag; };
typedef struct AzStyleCursorValueVariant_Inherit AzStyleCursorValueVariant_Inherit;

struct AzStyleCursorValueVariant_Initial { AzStyleCursorValueTag tag; };
typedef struct AzStyleCursorValueVariant_Initial AzStyleCursorValueVariant_Initial;

struct AzStyleCursorValueVariant_Exact { AzStyleCursorValueTag tag; AzStyleCursor payload; };
typedef struct AzStyleCursorValueVariant_Exact AzStyleCursorValueVariant_Exact;


union AzStyleCursorValue {
    AzStyleCursorValueVariant_Auto Auto;
    AzStyleCursorValueVariant_None None;
    AzStyleCursorValueVariant_Inherit Inherit;
    AzStyleCursorValueVariant_Initial Initial;
    AzStyleCursorValueVariant_Exact Exact;
};
typedef union AzStyleCursorValue AzStyleCursorValue;

enum AzStyleFontSizeValueTag {
   AzStyleFontSizeValueTag_Auto,
   AzStyleFontSizeValueTag_None,
   AzStyleFontSizeValueTag_Inherit,
   AzStyleFontSizeValueTag_Initial,
   AzStyleFontSizeValueTag_Exact,
};
typedef enum AzStyleFontSizeValueTag AzStyleFontSizeValueTag;

struct AzStyleFontSizeValueVariant_Auto { AzStyleFontSizeValueTag tag; };
typedef struct AzStyleFontSizeValueVariant_Auto AzStyleFontSizeValueVariant_Auto;

struct AzStyleFontSizeValueVariant_None { AzStyleFontSizeValueTag tag; };
typedef struct AzStyleFontSizeValueVariant_None AzStyleFontSizeValueVariant_None;

struct AzStyleFontSizeValueVariant_Inherit { AzStyleFontSizeValueTag tag; };
typedef struct AzStyleFontSizeValueVariant_Inherit AzStyleFontSizeValueVariant_Inherit;

struct AzStyleFontSizeValueVariant_Initial { AzStyleFontSizeValueTag tag; };
typedef struct AzStyleFontSizeValueVariant_Initial AzStyleFontSizeValueVariant_Initial;

struct AzStyleFontSizeValueVariant_Exact { AzStyleFontSizeValueTag tag; AzStyleFontSize payload; };
typedef struct AzStyleFontSizeValueVariant_Exact AzStyleFontSizeValueVariant_Exact;


union AzStyleFontSizeValue {
    AzStyleFontSizeValueVariant_Auto Auto;
    AzStyleFontSizeValueVariant_None None;
    AzStyleFontSizeValueVariant_Inherit Inherit;
    AzStyleFontSizeValueVariant_Initial Initial;
    AzStyleFontSizeValueVariant_Exact Exact;
};
typedef union AzStyleFontSizeValue AzStyleFontSizeValue;

enum AzStyleLetterSpacingValueTag {
   AzStyleLetterSpacingValueTag_Auto,
   AzStyleLetterSpacingValueTag_None,
   AzStyleLetterSpacingValueTag_Inherit,
   AzStyleLetterSpacingValueTag_Initial,
   AzStyleLetterSpacingValueTag_Exact,
};
typedef enum AzStyleLetterSpacingValueTag AzStyleLetterSpacingValueTag;

struct AzStyleLetterSpacingValueVariant_Auto { AzStyleLetterSpacingValueTag tag; };
typedef struct AzStyleLetterSpacingValueVariant_Auto AzStyleLetterSpacingValueVariant_Auto;

struct AzStyleLetterSpacingValueVariant_None { AzStyleLetterSpacingValueTag tag; };
typedef struct AzStyleLetterSpacingValueVariant_None AzStyleLetterSpacingValueVariant_None;

struct AzStyleLetterSpacingValueVariant_Inherit { AzStyleLetterSpacingValueTag tag; };
typedef struct AzStyleLetterSpacingValueVariant_Inherit AzStyleLetterSpacingValueVariant_Inherit;

struct AzStyleLetterSpacingValueVariant_Initial { AzStyleLetterSpacingValueTag tag; };
typedef struct AzStyleLetterSpacingValueVariant_Initial AzStyleLetterSpacingValueVariant_Initial;

struct AzStyleLetterSpacingValueVariant_Exact { AzStyleLetterSpacingValueTag tag; AzStyleLetterSpacing payload; };
typedef struct AzStyleLetterSpacingValueVariant_Exact AzStyleLetterSpacingValueVariant_Exact;


union AzStyleLetterSpacingValue {
    AzStyleLetterSpacingValueVariant_Auto Auto;
    AzStyleLetterSpacingValueVariant_None None;
    AzStyleLetterSpacingValueVariant_Inherit Inherit;
    AzStyleLetterSpacingValueVariant_Initial Initial;
    AzStyleLetterSpacingValueVariant_Exact Exact;
};
typedef union AzStyleLetterSpacingValue AzStyleLetterSpacingValue;

enum AzStyleLineHeightValueTag {
   AzStyleLineHeightValueTag_Auto,
   AzStyleLineHeightValueTag_None,
   AzStyleLineHeightValueTag_Inherit,
   AzStyleLineHeightValueTag_Initial,
   AzStyleLineHeightValueTag_Exact,
};
typedef enum AzStyleLineHeightValueTag AzStyleLineHeightValueTag;

struct AzStyleLineHeightValueVariant_Auto { AzStyleLineHeightValueTag tag; };
typedef struct AzStyleLineHeightValueVariant_Auto AzStyleLineHeightValueVariant_Auto;

struct AzStyleLineHeightValueVariant_None { AzStyleLineHeightValueTag tag; };
typedef struct AzStyleLineHeightValueVariant_None AzStyleLineHeightValueVariant_None;

struct AzStyleLineHeightValueVariant_Inherit { AzStyleLineHeightValueTag tag; };
typedef struct AzStyleLineHeightValueVariant_Inherit AzStyleLineHeightValueVariant_Inherit;

struct AzStyleLineHeightValueVariant_Initial { AzStyleLineHeightValueTag tag; };
typedef struct AzStyleLineHeightValueVariant_Initial AzStyleLineHeightValueVariant_Initial;

struct AzStyleLineHeightValueVariant_Exact { AzStyleLineHeightValueTag tag; AzStyleLineHeight payload; };
typedef struct AzStyleLineHeightValueVariant_Exact AzStyleLineHeightValueVariant_Exact;


union AzStyleLineHeightValue {
    AzStyleLineHeightValueVariant_Auto Auto;
    AzStyleLineHeightValueVariant_None None;
    AzStyleLineHeightValueVariant_Inherit Inherit;
    AzStyleLineHeightValueVariant_Initial Initial;
    AzStyleLineHeightValueVariant_Exact Exact;
};
typedef union AzStyleLineHeightValue AzStyleLineHeightValue;

enum AzStyleTabWidthValueTag {
   AzStyleTabWidthValueTag_Auto,
   AzStyleTabWidthValueTag_None,
   AzStyleTabWidthValueTag_Inherit,
   AzStyleTabWidthValueTag_Initial,
   AzStyleTabWidthValueTag_Exact,
};
typedef enum AzStyleTabWidthValueTag AzStyleTabWidthValueTag;

struct AzStyleTabWidthValueVariant_Auto { AzStyleTabWidthValueTag tag; };
typedef struct AzStyleTabWidthValueVariant_Auto AzStyleTabWidthValueVariant_Auto;

struct AzStyleTabWidthValueVariant_None { AzStyleTabWidthValueTag tag; };
typedef struct AzStyleTabWidthValueVariant_None AzStyleTabWidthValueVariant_None;

struct AzStyleTabWidthValueVariant_Inherit { AzStyleTabWidthValueTag tag; };
typedef struct AzStyleTabWidthValueVariant_Inherit AzStyleTabWidthValueVariant_Inherit;

struct AzStyleTabWidthValueVariant_Initial { AzStyleTabWidthValueTag tag; };
typedef struct AzStyleTabWidthValueVariant_Initial AzStyleTabWidthValueVariant_Initial;

struct AzStyleTabWidthValueVariant_Exact { AzStyleTabWidthValueTag tag; AzStyleTabWidth payload; };
typedef struct AzStyleTabWidthValueVariant_Exact AzStyleTabWidthValueVariant_Exact;


union AzStyleTabWidthValue {
    AzStyleTabWidthValueVariant_Auto Auto;
    AzStyleTabWidthValueVariant_None None;
    AzStyleTabWidthValueVariant_Inherit Inherit;
    AzStyleTabWidthValueVariant_Initial Initial;
    AzStyleTabWidthValueVariant_Exact Exact;
};
typedef union AzStyleTabWidthValue AzStyleTabWidthValue;

enum AzStyleTextAlignmentHorzValueTag {
   AzStyleTextAlignmentHorzValueTag_Auto,
   AzStyleTextAlignmentHorzValueTag_None,
   AzStyleTextAlignmentHorzValueTag_Inherit,
   AzStyleTextAlignmentHorzValueTag_Initial,
   AzStyleTextAlignmentHorzValueTag_Exact,
};
typedef enum AzStyleTextAlignmentHorzValueTag AzStyleTextAlignmentHorzValueTag;

struct AzStyleTextAlignmentHorzValueVariant_Auto { AzStyleTextAlignmentHorzValueTag tag; };
typedef struct AzStyleTextAlignmentHorzValueVariant_Auto AzStyleTextAlignmentHorzValueVariant_Auto;

struct AzStyleTextAlignmentHorzValueVariant_None { AzStyleTextAlignmentHorzValueTag tag; };
typedef struct AzStyleTextAlignmentHorzValueVariant_None AzStyleTextAlignmentHorzValueVariant_None;

struct AzStyleTextAlignmentHorzValueVariant_Inherit { AzStyleTextAlignmentHorzValueTag tag; };
typedef struct AzStyleTextAlignmentHorzValueVariant_Inherit AzStyleTextAlignmentHorzValueVariant_Inherit;

struct AzStyleTextAlignmentHorzValueVariant_Initial { AzStyleTextAlignmentHorzValueTag tag; };
typedef struct AzStyleTextAlignmentHorzValueVariant_Initial AzStyleTextAlignmentHorzValueVariant_Initial;

struct AzStyleTextAlignmentHorzValueVariant_Exact { AzStyleTextAlignmentHorzValueTag tag; AzStyleTextAlignmentHorz payload; };
typedef struct AzStyleTextAlignmentHorzValueVariant_Exact AzStyleTextAlignmentHorzValueVariant_Exact;


union AzStyleTextAlignmentHorzValue {
    AzStyleTextAlignmentHorzValueVariant_Auto Auto;
    AzStyleTextAlignmentHorzValueVariant_None None;
    AzStyleTextAlignmentHorzValueVariant_Inherit Inherit;
    AzStyleTextAlignmentHorzValueVariant_Initial Initial;
    AzStyleTextAlignmentHorzValueVariant_Exact Exact;
};
typedef union AzStyleTextAlignmentHorzValue AzStyleTextAlignmentHorzValue;

enum AzStyleTextColorValueTag {
   AzStyleTextColorValueTag_Auto,
   AzStyleTextColorValueTag_None,
   AzStyleTextColorValueTag_Inherit,
   AzStyleTextColorValueTag_Initial,
   AzStyleTextColorValueTag_Exact,
};
typedef enum AzStyleTextColorValueTag AzStyleTextColorValueTag;

struct AzStyleTextColorValueVariant_Auto { AzStyleTextColorValueTag tag; };
typedef struct AzStyleTextColorValueVariant_Auto AzStyleTextColorValueVariant_Auto;

struct AzStyleTextColorValueVariant_None { AzStyleTextColorValueTag tag; };
typedef struct AzStyleTextColorValueVariant_None AzStyleTextColorValueVariant_None;

struct AzStyleTextColorValueVariant_Inherit { AzStyleTextColorValueTag tag; };
typedef struct AzStyleTextColorValueVariant_Inherit AzStyleTextColorValueVariant_Inherit;

struct AzStyleTextColorValueVariant_Initial { AzStyleTextColorValueTag tag; };
typedef struct AzStyleTextColorValueVariant_Initial AzStyleTextColorValueVariant_Initial;

struct AzStyleTextColorValueVariant_Exact { AzStyleTextColorValueTag tag; AzStyleTextColor payload; };
typedef struct AzStyleTextColorValueVariant_Exact AzStyleTextColorValueVariant_Exact;


union AzStyleTextColorValue {
    AzStyleTextColorValueVariant_Auto Auto;
    AzStyleTextColorValueVariant_None None;
    AzStyleTextColorValueVariant_Inherit Inherit;
    AzStyleTextColorValueVariant_Initial Initial;
    AzStyleTextColorValueVariant_Exact Exact;
};
typedef union AzStyleTextColorValue AzStyleTextColorValue;

enum AzStyleWordSpacingValueTag {
   AzStyleWordSpacingValueTag_Auto,
   AzStyleWordSpacingValueTag_None,
   AzStyleWordSpacingValueTag_Inherit,
   AzStyleWordSpacingValueTag_Initial,
   AzStyleWordSpacingValueTag_Exact,
};
typedef enum AzStyleWordSpacingValueTag AzStyleWordSpacingValueTag;

struct AzStyleWordSpacingValueVariant_Auto { AzStyleWordSpacingValueTag tag; };
typedef struct AzStyleWordSpacingValueVariant_Auto AzStyleWordSpacingValueVariant_Auto;

struct AzStyleWordSpacingValueVariant_None { AzStyleWordSpacingValueTag tag; };
typedef struct AzStyleWordSpacingValueVariant_None AzStyleWordSpacingValueVariant_None;

struct AzStyleWordSpacingValueVariant_Inherit { AzStyleWordSpacingValueTag tag; };
typedef struct AzStyleWordSpacingValueVariant_Inherit AzStyleWordSpacingValueVariant_Inherit;

struct AzStyleWordSpacingValueVariant_Initial { AzStyleWordSpacingValueTag tag; };
typedef struct AzStyleWordSpacingValueVariant_Initial AzStyleWordSpacingValueVariant_Initial;

struct AzStyleWordSpacingValueVariant_Exact { AzStyleWordSpacingValueTag tag; AzStyleWordSpacing payload; };
typedef struct AzStyleWordSpacingValueVariant_Exact AzStyleWordSpacingValueVariant_Exact;


union AzStyleWordSpacingValue {
    AzStyleWordSpacingValueVariant_Auto Auto;
    AzStyleWordSpacingValueVariant_None None;
    AzStyleWordSpacingValueVariant_Inherit Inherit;
    AzStyleWordSpacingValueVariant_Initial Initial;
    AzStyleWordSpacingValueVariant_Exact Exact;
};
typedef union AzStyleWordSpacingValue AzStyleWordSpacingValue;

enum AzStyleOpacityValueTag {
   AzStyleOpacityValueTag_Auto,
   AzStyleOpacityValueTag_None,
   AzStyleOpacityValueTag_Inherit,
   AzStyleOpacityValueTag_Initial,
   AzStyleOpacityValueTag_Exact,
};
typedef enum AzStyleOpacityValueTag AzStyleOpacityValueTag;

struct AzStyleOpacityValueVariant_Auto { AzStyleOpacityValueTag tag; };
typedef struct AzStyleOpacityValueVariant_Auto AzStyleOpacityValueVariant_Auto;

struct AzStyleOpacityValueVariant_None { AzStyleOpacityValueTag tag; };
typedef struct AzStyleOpacityValueVariant_None AzStyleOpacityValueVariant_None;

struct AzStyleOpacityValueVariant_Inherit { AzStyleOpacityValueTag tag; };
typedef struct AzStyleOpacityValueVariant_Inherit AzStyleOpacityValueVariant_Inherit;

struct AzStyleOpacityValueVariant_Initial { AzStyleOpacityValueTag tag; };
typedef struct AzStyleOpacityValueVariant_Initial AzStyleOpacityValueVariant_Initial;

struct AzStyleOpacityValueVariant_Exact { AzStyleOpacityValueTag tag; AzStyleOpacity payload; };
typedef struct AzStyleOpacityValueVariant_Exact AzStyleOpacityValueVariant_Exact;


union AzStyleOpacityValue {
    AzStyleOpacityValueVariant_Auto Auto;
    AzStyleOpacityValueVariant_None None;
    AzStyleOpacityValueVariant_Inherit Inherit;
    AzStyleOpacityValueVariant_Initial Initial;
    AzStyleOpacityValueVariant_Exact Exact;
};
typedef union AzStyleOpacityValue AzStyleOpacityValue;

enum AzStyleTransformOriginValueTag {
   AzStyleTransformOriginValueTag_Auto,
   AzStyleTransformOriginValueTag_None,
   AzStyleTransformOriginValueTag_Inherit,
   AzStyleTransformOriginValueTag_Initial,
   AzStyleTransformOriginValueTag_Exact,
};
typedef enum AzStyleTransformOriginValueTag AzStyleTransformOriginValueTag;

struct AzStyleTransformOriginValueVariant_Auto { AzStyleTransformOriginValueTag tag; };
typedef struct AzStyleTransformOriginValueVariant_Auto AzStyleTransformOriginValueVariant_Auto;

struct AzStyleTransformOriginValueVariant_None { AzStyleTransformOriginValueTag tag; };
typedef struct AzStyleTransformOriginValueVariant_None AzStyleTransformOriginValueVariant_None;

struct AzStyleTransformOriginValueVariant_Inherit { AzStyleTransformOriginValueTag tag; };
typedef struct AzStyleTransformOriginValueVariant_Inherit AzStyleTransformOriginValueVariant_Inherit;

struct AzStyleTransformOriginValueVariant_Initial { AzStyleTransformOriginValueTag tag; };
typedef struct AzStyleTransformOriginValueVariant_Initial AzStyleTransformOriginValueVariant_Initial;

struct AzStyleTransformOriginValueVariant_Exact { AzStyleTransformOriginValueTag tag; AzStyleTransformOrigin payload; };
typedef struct AzStyleTransformOriginValueVariant_Exact AzStyleTransformOriginValueVariant_Exact;


union AzStyleTransformOriginValue {
    AzStyleTransformOriginValueVariant_Auto Auto;
    AzStyleTransformOriginValueVariant_None None;
    AzStyleTransformOriginValueVariant_Inherit Inherit;
    AzStyleTransformOriginValueVariant_Initial Initial;
    AzStyleTransformOriginValueVariant_Exact Exact;
};
typedef union AzStyleTransformOriginValue AzStyleTransformOriginValue;

enum AzStylePerspectiveOriginValueTag {
   AzStylePerspectiveOriginValueTag_Auto,
   AzStylePerspectiveOriginValueTag_None,
   AzStylePerspectiveOriginValueTag_Inherit,
   AzStylePerspectiveOriginValueTag_Initial,
   AzStylePerspectiveOriginValueTag_Exact,
};
typedef enum AzStylePerspectiveOriginValueTag AzStylePerspectiveOriginValueTag;

struct AzStylePerspectiveOriginValueVariant_Auto { AzStylePerspectiveOriginValueTag tag; };
typedef struct AzStylePerspectiveOriginValueVariant_Auto AzStylePerspectiveOriginValueVariant_Auto;

struct AzStylePerspectiveOriginValueVariant_None { AzStylePerspectiveOriginValueTag tag; };
typedef struct AzStylePerspectiveOriginValueVariant_None AzStylePerspectiveOriginValueVariant_None;

struct AzStylePerspectiveOriginValueVariant_Inherit { AzStylePerspectiveOriginValueTag tag; };
typedef struct AzStylePerspectiveOriginValueVariant_Inherit AzStylePerspectiveOriginValueVariant_Inherit;

struct AzStylePerspectiveOriginValueVariant_Initial { AzStylePerspectiveOriginValueTag tag; };
typedef struct AzStylePerspectiveOriginValueVariant_Initial AzStylePerspectiveOriginValueVariant_Initial;

struct AzStylePerspectiveOriginValueVariant_Exact { AzStylePerspectiveOriginValueTag tag; AzStylePerspectiveOrigin payload; };
typedef struct AzStylePerspectiveOriginValueVariant_Exact AzStylePerspectiveOriginValueVariant_Exact;


union AzStylePerspectiveOriginValue {
    AzStylePerspectiveOriginValueVariant_Auto Auto;
    AzStylePerspectiveOriginValueVariant_None None;
    AzStylePerspectiveOriginValueVariant_Inherit Inherit;
    AzStylePerspectiveOriginValueVariant_Initial Initial;
    AzStylePerspectiveOriginValueVariant_Exact Exact;
};
typedef union AzStylePerspectiveOriginValue AzStylePerspectiveOriginValue;

enum AzStyleBackfaceVisibilityValueTag {
   AzStyleBackfaceVisibilityValueTag_Auto,
   AzStyleBackfaceVisibilityValueTag_None,
   AzStyleBackfaceVisibilityValueTag_Inherit,
   AzStyleBackfaceVisibilityValueTag_Initial,
   AzStyleBackfaceVisibilityValueTag_Exact,
};
typedef enum AzStyleBackfaceVisibilityValueTag AzStyleBackfaceVisibilityValueTag;

struct AzStyleBackfaceVisibilityValueVariant_Auto { AzStyleBackfaceVisibilityValueTag tag; };
typedef struct AzStyleBackfaceVisibilityValueVariant_Auto AzStyleBackfaceVisibilityValueVariant_Auto;

struct AzStyleBackfaceVisibilityValueVariant_None { AzStyleBackfaceVisibilityValueTag tag; };
typedef struct AzStyleBackfaceVisibilityValueVariant_None AzStyleBackfaceVisibilityValueVariant_None;

struct AzStyleBackfaceVisibilityValueVariant_Inherit { AzStyleBackfaceVisibilityValueTag tag; };
typedef struct AzStyleBackfaceVisibilityValueVariant_Inherit AzStyleBackfaceVisibilityValueVariant_Inherit;

struct AzStyleBackfaceVisibilityValueVariant_Initial { AzStyleBackfaceVisibilityValueTag tag; };
typedef struct AzStyleBackfaceVisibilityValueVariant_Initial AzStyleBackfaceVisibilityValueVariant_Initial;

struct AzStyleBackfaceVisibilityValueVariant_Exact { AzStyleBackfaceVisibilityValueTag tag; AzStyleBackfaceVisibility payload; };
typedef struct AzStyleBackfaceVisibilityValueVariant_Exact AzStyleBackfaceVisibilityValueVariant_Exact;


union AzStyleBackfaceVisibilityValue {
    AzStyleBackfaceVisibilityValueVariant_Auto Auto;
    AzStyleBackfaceVisibilityValueVariant_None None;
    AzStyleBackfaceVisibilityValueVariant_Inherit Inherit;
    AzStyleBackfaceVisibilityValueVariant_Initial Initial;
    AzStyleBackfaceVisibilityValueVariant_Exact Exact;
};
typedef union AzStyleBackfaceVisibilityValue AzStyleBackfaceVisibilityValue;

struct AzParentWithNodeDepth {
    size_t depth;
    AzNodeId node_id;
};
typedef struct AzParentWithNodeDepth AzParentWithNodeDepth;

struct AzGlContextPtr {
    void* const ptr;
    AzRendererType renderer_type;
};
typedef struct AzGlContextPtr AzGlContextPtr;

struct AzTexture {
    uint32_t texture_id;
    AzRawImageFormat format;
    AzTextureFlags flags;
    AzPhysicalSizeU32 size;
    AzGlContextPtr gl_context;
};
typedef struct AzTexture AzTexture;

struct AzRefstrVecRef {
    AzRefstr* const ptr;
    size_t len;
};
typedef struct AzRefstrVecRef AzRefstrVecRef;

struct AzImageMask {
    AzImageId image;
    AzLogicalRect rect;
    bool  repeat;
};
typedef struct AzImageMask AzImageMask;

struct AzSvgLine {
    AzSvgPoint start;
    AzSvgPoint end;
};
typedef struct AzSvgLine AzSvgLine;

struct AzSvgQuadraticCurve {
    AzSvgPoint start;
    AzSvgPoint ctrl;
    AzSvgPoint end;
};
typedef struct AzSvgQuadraticCurve AzSvgQuadraticCurve;

struct AzSvgCubicCurve {
    AzSvgPoint start;
    AzSvgPoint ctrl_1;
    AzSvgPoint ctrl_2;
    AzSvgPoint end;
};
typedef struct AzSvgCubicCurve AzSvgCubicCurve;

struct AzSvgFillStyle {
    AzSvgLineJoin line_join;
    size_t miter_limit;
    size_t tolerance;
};
typedef struct AzSvgFillStyle AzSvgFillStyle;

struct AzThreadSender {
    void* restrict ptr;
    AzThreadSendFn send_fn;
    AzThreadSenderDestructorFn destructor;
};
typedef struct AzThreadSender AzThreadSender;

struct AzThreadReceiver {
    void* restrict ptr;
    AzThreadRecvFn recv_fn;
    AzThreadReceiverDestructorFn destructor;
};
typedef struct AzThreadReceiver AzThreadReceiver;

struct AzVideoModeVec {
    AzVideoMode* const ptr;
    size_t len;
    size_t cap;
    AzVideoModeVecDestructor destructor;
};
typedef struct AzVideoModeVec AzVideoModeVec;

struct AzDom;
typedef struct AzDom AzDom;
struct AzDomVec {
    AzDom* const ptr;
    size_t len;
    size_t cap;
    AzDomVecDestructor destructor;
};
typedef struct AzDomVec AzDomVec;

struct AzStyleBackgroundPositionVec {
    AzStyleBackgroundPosition* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundPositionVecDestructor destructor;
};
typedef struct AzStyleBackgroundPositionVec AzStyleBackgroundPositionVec;

struct AzStyleBackgroundRepeatVec {
    AzStyleBackgroundRepeat* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundRepeatVecDestructor destructor;
};
typedef struct AzStyleBackgroundRepeatVec AzStyleBackgroundRepeatVec;

struct AzStyleBackgroundSizeVec {
    AzStyleBackgroundSize* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundSizeVecDestructor destructor;
};
typedef struct AzStyleBackgroundSizeVec AzStyleBackgroundSizeVec;

struct AzSvgVertexVec {
    AzSvgVertex* const ptr;
    size_t len;
    size_t cap;
    AzSvgVertexVecDestructor destructor;
};
typedef struct AzSvgVertexVec AzSvgVertexVec;

struct AzU32Vec {
    uint32_t* const ptr;
    size_t len;
    size_t cap;
    AzU32VecDestructor destructor;
};
typedef struct AzU32Vec AzU32Vec;

struct AzXWindowTypeVec {
    AzXWindowType* const ptr;
    size_t len;
    size_t cap;
    AzXWindowTypeVecDestructor destructor;
};
typedef struct AzXWindowTypeVec AzXWindowTypeVec;

struct AzVirtualKeyCodeVec {
    AzVirtualKeyCode* const ptr;
    size_t len;
    size_t cap;
    AzVirtualKeyCodeVecDestructor destructor;
};
typedef struct AzVirtualKeyCodeVec AzVirtualKeyCodeVec;

struct AzCascadeInfoVec {
    AzCascadeInfo* const ptr;
    size_t len;
    size_t cap;
    AzCascadeInfoVecDestructor destructor;
};
typedef struct AzCascadeInfoVec AzCascadeInfoVec;

struct AzScanCodeVec {
    uint32_t* const ptr;
    size_t len;
    size_t cap;
    AzScanCodeVecDestructor destructor;
};
typedef struct AzScanCodeVec AzScanCodeVec;

struct AzU8Vec {
    uint8_t* const ptr;
    size_t len;
    size_t cap;
    AzU8VecDestructor destructor;
};
typedef struct AzU8Vec AzU8Vec;

struct AzGLuintVec {
    uint32_t* const ptr;
    size_t len;
    size_t cap;
    AzGLuintVecDestructor destructor;
};
typedef struct AzGLuintVec AzGLuintVec;

struct AzGLintVec {
    int32_t* const ptr;
    size_t len;
    size_t cap;
    AzGLintVecDestructor destructor;
};
typedef struct AzGLintVec AzGLintVec;

struct AzNodeIdVec {
    AzNodeId* const ptr;
    size_t len;
    size_t cap;
    AzNodeIdVecDestructor destructor;
};
typedef struct AzNodeIdVec AzNodeIdVec;

struct AzNodeVec {
    AzNode* const ptr;
    size_t len;
    size_t cap;
    AzNodeVecDestructor destructor;
};
typedef struct AzNodeVec AzNodeVec;

struct AzParentWithNodeDepthVec {
    AzParentWithNodeDepth* const ptr;
    size_t len;
    size_t cap;
    AzParentWithNodeDepthVecDestructor destructor;
};
typedef struct AzParentWithNodeDepthVec AzParentWithNodeDepthVec;

enum AzOptionGlContextPtrTag {
   AzOptionGlContextPtrTag_None,
   AzOptionGlContextPtrTag_Some,
};
typedef enum AzOptionGlContextPtrTag AzOptionGlContextPtrTag;

struct AzOptionGlContextPtrVariant_None { AzOptionGlContextPtrTag tag; };
typedef struct AzOptionGlContextPtrVariant_None AzOptionGlContextPtrVariant_None;

struct AzOptionGlContextPtrVariant_Some { AzOptionGlContextPtrTag tag; AzGlContextPtr payload; };
typedef struct AzOptionGlContextPtrVariant_Some AzOptionGlContextPtrVariant_Some;


union AzOptionGlContextPtr {
    AzOptionGlContextPtrVariant_None None;
    AzOptionGlContextPtrVariant_Some Some;
};
typedef union AzOptionGlContextPtr AzOptionGlContextPtr;

enum AzOptionPercentageValueTag {
   AzOptionPercentageValueTag_None,
   AzOptionPercentageValueTag_Some,
};
typedef enum AzOptionPercentageValueTag AzOptionPercentageValueTag;

struct AzOptionPercentageValueVariant_None { AzOptionPercentageValueTag tag; };
typedef struct AzOptionPercentageValueVariant_None AzOptionPercentageValueVariant_None;

struct AzOptionPercentageValueVariant_Some { AzOptionPercentageValueTag tag; AzPercentageValue payload; };
typedef struct AzOptionPercentageValueVariant_Some AzOptionPercentageValueVariant_Some;


union AzOptionPercentageValue {
    AzOptionPercentageValueVariant_None None;
    AzOptionPercentageValueVariant_Some Some;
};
typedef union AzOptionPercentageValue AzOptionPercentageValue;

enum AzOptionAngleValueTag {
   AzOptionAngleValueTag_None,
   AzOptionAngleValueTag_Some,
};
typedef enum AzOptionAngleValueTag AzOptionAngleValueTag;

struct AzOptionAngleValueVariant_None { AzOptionAngleValueTag tag; };
typedef struct AzOptionAngleValueVariant_None AzOptionAngleValueVariant_None;

struct AzOptionAngleValueVariant_Some { AzOptionAngleValueTag tag; AzAngleValue payload; };
typedef struct AzOptionAngleValueVariant_Some AzOptionAngleValueVariant_Some;


union AzOptionAngleValue {
    AzOptionAngleValueVariant_None None;
    AzOptionAngleValueVariant_Some Some;
};
typedef union AzOptionAngleValue AzOptionAngleValue;

enum AzOptionRendererOptionsTag {
   AzOptionRendererOptionsTag_None,
   AzOptionRendererOptionsTag_Some,
};
typedef enum AzOptionRendererOptionsTag AzOptionRendererOptionsTag;

struct AzOptionRendererOptionsVariant_None { AzOptionRendererOptionsTag tag; };
typedef struct AzOptionRendererOptionsVariant_None AzOptionRendererOptionsVariant_None;

struct AzOptionRendererOptionsVariant_Some { AzOptionRendererOptionsTag tag; AzRendererOptions payload; };
typedef struct AzOptionRendererOptionsVariant_Some AzOptionRendererOptionsVariant_Some;


union AzOptionRendererOptions {
    AzOptionRendererOptionsVariant_None None;
    AzOptionRendererOptionsVariant_Some Some;
};
typedef union AzOptionRendererOptions AzOptionRendererOptions;

enum AzOptionCallbackTag {
   AzOptionCallbackTag_None,
   AzOptionCallbackTag_Some,
};
typedef enum AzOptionCallbackTag AzOptionCallbackTag;

struct AzOptionCallbackVariant_None { AzOptionCallbackTag tag; };
typedef struct AzOptionCallbackVariant_None AzOptionCallbackVariant_None;

struct AzOptionCallbackVariant_Some { AzOptionCallbackTag tag; AzCallback payload; };
typedef struct AzOptionCallbackVariant_Some AzOptionCallbackVariant_Some;


union AzOptionCallback {
    AzOptionCallbackVariant_None None;
    AzOptionCallbackVariant_Some Some;
};
typedef union AzOptionCallback AzOptionCallback;

enum AzOptionThreadSendMsgTag {
   AzOptionThreadSendMsgTag_None,
   AzOptionThreadSendMsgTag_Some,
};
typedef enum AzOptionThreadSendMsgTag AzOptionThreadSendMsgTag;

struct AzOptionThreadSendMsgVariant_None { AzOptionThreadSendMsgTag tag; };
typedef struct AzOptionThreadSendMsgVariant_None AzOptionThreadSendMsgVariant_None;

struct AzOptionThreadSendMsgVariant_Some { AzOptionThreadSendMsgTag tag; AzThreadSendMsg payload; };
typedef struct AzOptionThreadSendMsgVariant_Some AzOptionThreadSendMsgVariant_Some;


union AzOptionThreadSendMsg {
    AzOptionThreadSendMsgVariant_None None;
    AzOptionThreadSendMsgVariant_Some Some;
};
typedef union AzOptionThreadSendMsg AzOptionThreadSendMsg;

enum AzOptionLayoutRectTag {
   AzOptionLayoutRectTag_None,
   AzOptionLayoutRectTag_Some,
};
typedef enum AzOptionLayoutRectTag AzOptionLayoutRectTag;

struct AzOptionLayoutRectVariant_None { AzOptionLayoutRectTag tag; };
typedef struct AzOptionLayoutRectVariant_None AzOptionLayoutRectVariant_None;

struct AzOptionLayoutRectVariant_Some { AzOptionLayoutRectTag tag; AzLayoutRect payload; };
typedef struct AzOptionLayoutRectVariant_Some AzOptionLayoutRectVariant_Some;


union AzOptionLayoutRect {
    AzOptionLayoutRectVariant_None None;
    AzOptionLayoutRectVariant_Some Some;
};
typedef union AzOptionLayoutRect AzOptionLayoutRect;

enum AzOptionLayoutPointTag {
   AzOptionLayoutPointTag_None,
   AzOptionLayoutPointTag_Some,
};
typedef enum AzOptionLayoutPointTag AzOptionLayoutPointTag;

struct AzOptionLayoutPointVariant_None { AzOptionLayoutPointTag tag; };
typedef struct AzOptionLayoutPointVariant_None AzOptionLayoutPointVariant_None;

struct AzOptionLayoutPointVariant_Some { AzOptionLayoutPointTag tag; AzLayoutPoint payload; };
typedef struct AzOptionLayoutPointVariant_Some AzOptionLayoutPointVariant_Some;


union AzOptionLayoutPoint {
    AzOptionLayoutPointVariant_None None;
    AzOptionLayoutPointVariant_Some Some;
};
typedef union AzOptionLayoutPoint AzOptionLayoutPoint;

enum AzOptionWindowThemeTag {
   AzOptionWindowThemeTag_None,
   AzOptionWindowThemeTag_Some,
};
typedef enum AzOptionWindowThemeTag AzOptionWindowThemeTag;

struct AzOptionWindowThemeVariant_None { AzOptionWindowThemeTag tag; };
typedef struct AzOptionWindowThemeVariant_None AzOptionWindowThemeVariant_None;

struct AzOptionWindowThemeVariant_Some { AzOptionWindowThemeTag tag; AzWindowTheme payload; };
typedef struct AzOptionWindowThemeVariant_Some AzOptionWindowThemeVariant_Some;


union AzOptionWindowTheme {
    AzOptionWindowThemeVariant_None None;
    AzOptionWindowThemeVariant_Some Some;
};
typedef union AzOptionWindowTheme AzOptionWindowTheme;

enum AzOptionNodeIdTag {
   AzOptionNodeIdTag_None,
   AzOptionNodeIdTag_Some,
};
typedef enum AzOptionNodeIdTag AzOptionNodeIdTag;

struct AzOptionNodeIdVariant_None { AzOptionNodeIdTag tag; };
typedef struct AzOptionNodeIdVariant_None AzOptionNodeIdVariant_None;

struct AzOptionNodeIdVariant_Some { AzOptionNodeIdTag tag; AzNodeId payload; };
typedef struct AzOptionNodeIdVariant_Some AzOptionNodeIdVariant_Some;


union AzOptionNodeId {
    AzOptionNodeIdVariant_None None;
    AzOptionNodeIdVariant_Some Some;
};
typedef union AzOptionNodeId AzOptionNodeId;

enum AzOptionDomNodeIdTag {
   AzOptionDomNodeIdTag_None,
   AzOptionDomNodeIdTag_Some,
};
typedef enum AzOptionDomNodeIdTag AzOptionDomNodeIdTag;

struct AzOptionDomNodeIdVariant_None { AzOptionDomNodeIdTag tag; };
typedef struct AzOptionDomNodeIdVariant_None AzOptionDomNodeIdVariant_None;

struct AzOptionDomNodeIdVariant_Some { AzOptionDomNodeIdTag tag; AzDomNodeId payload; };
typedef struct AzOptionDomNodeIdVariant_Some AzOptionDomNodeIdVariant_Some;


union AzOptionDomNodeId {
    AzOptionDomNodeIdVariant_None None;
    AzOptionDomNodeIdVariant_Some Some;
};
typedef union AzOptionDomNodeId AzOptionDomNodeId;

enum AzOptionColorUTag {
   AzOptionColorUTag_None,
   AzOptionColorUTag_Some,
};
typedef enum AzOptionColorUTag AzOptionColorUTag;

struct AzOptionColorUVariant_None { AzOptionColorUTag tag; };
typedef struct AzOptionColorUVariant_None AzOptionColorUVariant_None;

struct AzOptionColorUVariant_Some { AzOptionColorUTag tag; AzColorU payload; };
typedef struct AzOptionColorUVariant_Some AzOptionColorUVariant_Some;


union AzOptionColorU {
    AzOptionColorUVariant_None None;
    AzOptionColorUVariant_Some Some;
};
typedef union AzOptionColorU AzOptionColorU;

enum AzOptionSvgDashPatternTag {
   AzOptionSvgDashPatternTag_None,
   AzOptionSvgDashPatternTag_Some,
};
typedef enum AzOptionSvgDashPatternTag AzOptionSvgDashPatternTag;

struct AzOptionSvgDashPatternVariant_None { AzOptionSvgDashPatternTag tag; };
typedef struct AzOptionSvgDashPatternVariant_None AzOptionSvgDashPatternVariant_None;

struct AzOptionSvgDashPatternVariant_Some { AzOptionSvgDashPatternTag tag; AzSvgDashPattern payload; };
typedef struct AzOptionSvgDashPatternVariant_Some AzOptionSvgDashPatternVariant_Some;


union AzOptionSvgDashPattern {
    AzOptionSvgDashPatternVariant_None None;
    AzOptionSvgDashPatternVariant_Some Some;
};
typedef union AzOptionSvgDashPattern AzOptionSvgDashPattern;

enum AzOptionLogicalPositionTag {
   AzOptionLogicalPositionTag_None,
   AzOptionLogicalPositionTag_Some,
};
typedef enum AzOptionLogicalPositionTag AzOptionLogicalPositionTag;

struct AzOptionLogicalPositionVariant_None { AzOptionLogicalPositionTag tag; };
typedef struct AzOptionLogicalPositionVariant_None AzOptionLogicalPositionVariant_None;

struct AzOptionLogicalPositionVariant_Some { AzOptionLogicalPositionTag tag; AzLogicalPosition payload; };
typedef struct AzOptionLogicalPositionVariant_Some AzOptionLogicalPositionVariant_Some;


union AzOptionLogicalPosition {
    AzOptionLogicalPositionVariant_None None;
    AzOptionLogicalPositionVariant_Some Some;
};
typedef union AzOptionLogicalPosition AzOptionLogicalPosition;

enum AzOptionPhysicalPositionI32Tag {
   AzOptionPhysicalPositionI32Tag_None,
   AzOptionPhysicalPositionI32Tag_Some,
};
typedef enum AzOptionPhysicalPositionI32Tag AzOptionPhysicalPositionI32Tag;

struct AzOptionPhysicalPositionI32Variant_None { AzOptionPhysicalPositionI32Tag tag; };
typedef struct AzOptionPhysicalPositionI32Variant_None AzOptionPhysicalPositionI32Variant_None;

struct AzOptionPhysicalPositionI32Variant_Some { AzOptionPhysicalPositionI32Tag tag; AzPhysicalPositionI32 payload; };
typedef struct AzOptionPhysicalPositionI32Variant_Some AzOptionPhysicalPositionI32Variant_Some;


union AzOptionPhysicalPositionI32 {
    AzOptionPhysicalPositionI32Variant_None None;
    AzOptionPhysicalPositionI32Variant_Some Some;
};
typedef union AzOptionPhysicalPositionI32 AzOptionPhysicalPositionI32;

enum AzOptionMouseCursorTypeTag {
   AzOptionMouseCursorTypeTag_None,
   AzOptionMouseCursorTypeTag_Some,
};
typedef enum AzOptionMouseCursorTypeTag AzOptionMouseCursorTypeTag;

struct AzOptionMouseCursorTypeVariant_None { AzOptionMouseCursorTypeTag tag; };
typedef struct AzOptionMouseCursorTypeVariant_None AzOptionMouseCursorTypeVariant_None;

struct AzOptionMouseCursorTypeVariant_Some { AzOptionMouseCursorTypeTag tag; AzMouseCursorType payload; };
typedef struct AzOptionMouseCursorTypeVariant_Some AzOptionMouseCursorTypeVariant_Some;


union AzOptionMouseCursorType {
    AzOptionMouseCursorTypeVariant_None None;
    AzOptionMouseCursorTypeVariant_Some Some;
};
typedef union AzOptionMouseCursorType AzOptionMouseCursorType;

enum AzOptionLogicalSizeTag {
   AzOptionLogicalSizeTag_None,
   AzOptionLogicalSizeTag_Some,
};
typedef enum AzOptionLogicalSizeTag AzOptionLogicalSizeTag;

struct AzOptionLogicalSizeVariant_None { AzOptionLogicalSizeTag tag; };
typedef struct AzOptionLogicalSizeVariant_None AzOptionLogicalSizeVariant_None;

struct AzOptionLogicalSizeVariant_Some { AzOptionLogicalSizeTag tag; AzLogicalSize payload; };
typedef struct AzOptionLogicalSizeVariant_Some AzOptionLogicalSizeVariant_Some;


union AzOptionLogicalSize {
    AzOptionLogicalSizeVariant_None None;
    AzOptionLogicalSizeVariant_Some Some;
};
typedef union AzOptionLogicalSize AzOptionLogicalSize;

enum AzOptionVirtualKeyCodeTag {
   AzOptionVirtualKeyCodeTag_None,
   AzOptionVirtualKeyCodeTag_Some,
};
typedef enum AzOptionVirtualKeyCodeTag AzOptionVirtualKeyCodeTag;

struct AzOptionVirtualKeyCodeVariant_None { AzOptionVirtualKeyCodeTag tag; };
typedef struct AzOptionVirtualKeyCodeVariant_None AzOptionVirtualKeyCodeVariant_None;

struct AzOptionVirtualKeyCodeVariant_Some { AzOptionVirtualKeyCodeTag tag; AzVirtualKeyCode payload; };
typedef struct AzOptionVirtualKeyCodeVariant_Some AzOptionVirtualKeyCodeVariant_Some;


union AzOptionVirtualKeyCode {
    AzOptionVirtualKeyCodeVariant_None None;
    AzOptionVirtualKeyCodeVariant_Some Some;
};
typedef union AzOptionVirtualKeyCode AzOptionVirtualKeyCode;

enum AzOptionTextureTag {
   AzOptionTextureTag_None,
   AzOptionTextureTag_Some,
};
typedef enum AzOptionTextureTag AzOptionTextureTag;

struct AzOptionTextureVariant_None { AzOptionTextureTag tag; };
typedef struct AzOptionTextureVariant_None AzOptionTextureVariant_None;

struct AzOptionTextureVariant_Some { AzOptionTextureTag tag; AzTexture payload; };
typedef struct AzOptionTextureVariant_Some AzOptionTextureVariant_Some;


union AzOptionTexture {
    AzOptionTextureVariant_None None;
    AzOptionTextureVariant_Some Some;
};
typedef union AzOptionTexture AzOptionTexture;

enum AzOptionImageMaskTag {
   AzOptionImageMaskTag_None,
   AzOptionImageMaskTag_Some,
};
typedef enum AzOptionImageMaskTag AzOptionImageMaskTag;

struct AzOptionImageMaskVariant_None { AzOptionImageMaskTag tag; };
typedef struct AzOptionImageMaskVariant_None AzOptionImageMaskVariant_None;

struct AzOptionImageMaskVariant_Some { AzOptionImageMaskTag tag; AzImageMask payload; };
typedef struct AzOptionImageMaskVariant_Some AzOptionImageMaskVariant_Some;


union AzOptionImageMask {
    AzOptionImageMaskVariant_None None;
    AzOptionImageMaskVariant_Some Some;
};
typedef union AzOptionImageMask AzOptionImageMask;

enum AzOptionTabIndexTag {
   AzOptionTabIndexTag_None,
   AzOptionTabIndexTag_Some,
};
typedef enum AzOptionTabIndexTag AzOptionTabIndexTag;

struct AzOptionTabIndexVariant_None { AzOptionTabIndexTag tag; };
typedef struct AzOptionTabIndexVariant_None AzOptionTabIndexVariant_None;

struct AzOptionTabIndexVariant_Some { AzOptionTabIndexTag tag; AzTabIndex payload; };
typedef struct AzOptionTabIndexVariant_Some AzOptionTabIndexVariant_Some;


union AzOptionTabIndex {
    AzOptionTabIndexVariant_None None;
    AzOptionTabIndexVariant_Some Some;
};
typedef union AzOptionTabIndex AzOptionTabIndex;

enum AzOptionTagIdTag {
   AzOptionTagIdTag_None,
   AzOptionTagIdTag_Some,
};
typedef enum AzOptionTagIdTag AzOptionTagIdTag;

struct AzOptionTagIdVariant_None { AzOptionTagIdTag tag; };
typedef struct AzOptionTagIdVariant_None AzOptionTagIdVariant_None;

struct AzOptionTagIdVariant_Some { AzOptionTagIdTag tag; AzTagId payload; };
typedef struct AzOptionTagIdVariant_Some AzOptionTagIdVariant_Some;


union AzOptionTagId {
    AzOptionTagIdVariant_None None;
    AzOptionTagIdVariant_Some Some;
};
typedef union AzOptionTagId AzOptionTagId;

enum AzOptionU8VecRefTag {
   AzOptionU8VecRefTag_None,
   AzOptionU8VecRefTag_Some,
};
typedef enum AzOptionU8VecRefTag AzOptionU8VecRefTag;

struct AzOptionU8VecRefVariant_None { AzOptionU8VecRefTag tag; };
typedef struct AzOptionU8VecRefVariant_None AzOptionU8VecRefVariant_None;

struct AzOptionU8VecRefVariant_Some { AzOptionU8VecRefTag tag; AzU8VecRef payload; };
typedef struct AzOptionU8VecRefVariant_Some AzOptionU8VecRefVariant_Some;


union AzOptionU8VecRef {
    AzOptionU8VecRefVariant_None None;
    AzOptionU8VecRefVariant_Some Some;
};
typedef union AzOptionU8VecRef AzOptionU8VecRef;

struct AzNonXmlCharError {
    uint32_t ch;
    AzSvgParseErrorPosition pos;
};
typedef struct AzNonXmlCharError AzNonXmlCharError;

struct AzInvalidCharError {
    uint8_t expected;
    uint8_t got;
    AzSvgParseErrorPosition pos;
};
typedef struct AzInvalidCharError AzInvalidCharError;

struct AzInvalidCharMultipleError {
    uint8_t expected;
    AzU8Vec got;
    AzSvgParseErrorPosition pos;
};
typedef struct AzInvalidCharMultipleError AzInvalidCharMultipleError;

struct AzInvalidQuoteError {
    uint8_t got;
    AzSvgParseErrorPosition pos;
};
typedef struct AzInvalidQuoteError AzInvalidQuoteError;

struct AzInvalidSpaceError {
    uint8_t got;
    AzSvgParseErrorPosition pos;
};
typedef struct AzInvalidSpaceError AzInvalidSpaceError;

struct AzInstantPtr {
    void* const ptr;
    AzInstantPtrCloneFn clone_fn;
    AzInstantPtrDestructorFn destructor;
};
typedef struct AzInstantPtr AzInstantPtr;

enum AzDurationTag {
   AzDurationTag_System,
   AzDurationTag_Tick,
};
typedef enum AzDurationTag AzDurationTag;

struct AzDurationVariant_System { AzDurationTag tag; AzSystemTimeDiff payload; };
typedef struct AzDurationVariant_System AzDurationVariant_System;

struct AzDurationVariant_Tick { AzDurationTag tag; AzSystemTickDiff payload; };
typedef struct AzDurationVariant_Tick AzDurationVariant_Tick;


union AzDuration {
    AzDurationVariant_System System;
    AzDurationVariant_Tick Tick;
};
typedef union AzDuration AzDuration;

struct AzAppConfig {
    AzAppLogLevel log_level;
    bool  enable_visual_panic_hook;
    bool  enable_logging_on_panic;
    bool  enable_tab_navigation;
    AzSystemCallbacks system_callbacks;
};
typedef struct AzAppConfig AzAppConfig;

struct AzSmallWindowIconBytes {
    AzIconKey key;
    AzU8Vec rgba_bytes;
};
typedef struct AzSmallWindowIconBytes AzSmallWindowIconBytes;

struct AzLargeWindowIconBytes {
    AzIconKey key;
    AzU8Vec rgba_bytes;
};
typedef struct AzLargeWindowIconBytes AzLargeWindowIconBytes;

enum AzWindowIconTag {
   AzWindowIconTag_Small,
   AzWindowIconTag_Large,
};
typedef enum AzWindowIconTag AzWindowIconTag;

struct AzWindowIconVariant_Small { AzWindowIconTag tag; AzSmallWindowIconBytes payload; };
typedef struct AzWindowIconVariant_Small AzWindowIconVariant_Small;

struct AzWindowIconVariant_Large { AzWindowIconTag tag; AzLargeWindowIconBytes payload; };
typedef struct AzWindowIconVariant_Large AzWindowIconVariant_Large;


union AzWindowIcon {
    AzWindowIconVariant_Small Small;
    AzWindowIconVariant_Large Large;
};
typedef union AzWindowIcon AzWindowIcon;

struct AzTaskBarIcon {
    AzIconKey key;
    AzU8Vec rgba_bytes;
};
typedef struct AzTaskBarIcon AzTaskBarIcon;

struct AzWindowSize {
    AzLogicalSize dimensions;
    float hidpi_factor;
    float system_hidpi_factor;
    AzOptionLogicalSize min_dimensions;
    AzOptionLogicalSize max_dimensions;
};
typedef struct AzWindowSize AzWindowSize;

struct AzKeyboardState {
    bool  shift_down;
    bool  ctrl_down;
    bool  alt_down;
    bool  super_down;
    AzOptionChar current_char;
    AzOptionVirtualKeyCode current_virtual_keycode;
    AzVirtualKeyCodeVec pressed_virtual_keycodes;
    AzScanCodeVec pressed_scancodes;
};
typedef struct AzKeyboardState AzKeyboardState;

struct AzMouseState {
    AzOptionMouseCursorType mouse_cursor_type;
    AzCursorPosition cursor_position;
    bool  is_cursor_locked;
    bool  left_down;
    bool  right_down;
    bool  middle_down;
    AzOptionF32 scroll_x;
    AzOptionF32 scroll_y;
};
typedef struct AzMouseState AzMouseState;

struct AzGlCallbackInfo {
    AzDomNodeId callback_node_id;
    AzHidpiAdjustedBounds bounds;
    AzGlContextPtr* const gl_context;
    void* const resources;
    AzNodeVec* const node_hierarchy;
    void* const words_cache;
    void* const shaped_words_cache;
    void* const positioned_words_cache;
    void* const positioned_rects;
};
typedef struct AzGlCallbackInfo AzGlCallbackInfo;

struct AzGlCallbackReturn {
    AzOptionTexture texture;
};
typedef struct AzGlCallbackReturn AzGlCallbackReturn;

struct AzLayoutInfo {
    AzWindowSize* const window_size;
    void* restrict window_size_width_stops;
    void* restrict window_size_height_stops;
    void* const resources;
};
typedef struct AzLayoutInfo AzLayoutInfo;

enum AzEventFilterTag {
   AzEventFilterTag_Hover,
   AzEventFilterTag_Not,
   AzEventFilterTag_Focus,
   AzEventFilterTag_Window,
   AzEventFilterTag_Component,
   AzEventFilterTag_Application,
};
typedef enum AzEventFilterTag AzEventFilterTag;

struct AzEventFilterVariant_Hover { AzEventFilterTag tag; AzHoverEventFilter payload; };
typedef struct AzEventFilterVariant_Hover AzEventFilterVariant_Hover;

struct AzEventFilterVariant_Not { AzEventFilterTag tag; AzNotEventFilter payload; };
typedef struct AzEventFilterVariant_Not AzEventFilterVariant_Not;

struct AzEventFilterVariant_Focus { AzEventFilterTag tag; AzFocusEventFilter payload; };
typedef struct AzEventFilterVariant_Focus AzEventFilterVariant_Focus;

struct AzEventFilterVariant_Window { AzEventFilterTag tag; AzWindowEventFilter payload; };
typedef struct AzEventFilterVariant_Window AzEventFilterVariant_Window;

struct AzEventFilterVariant_Component { AzEventFilterTag tag; AzComponentEventFilter payload; };
typedef struct AzEventFilterVariant_Component AzEventFilterVariant_Component;

struct AzEventFilterVariant_Application { AzEventFilterTag tag; AzApplicationEventFilter payload; };
typedef struct AzEventFilterVariant_Application AzEventFilterVariant_Application;


union AzEventFilter {
    AzEventFilterVariant_Hover Hover;
    AzEventFilterVariant_Not Not;
    AzEventFilterVariant_Focus Focus;
    AzEventFilterVariant_Window Window;
    AzEventFilterVariant_Component Component;
    AzEventFilterVariant_Application Application;
};
typedef union AzEventFilter AzEventFilter;

enum AzCssPathPseudoSelectorTag {
   AzCssPathPseudoSelectorTag_First,
   AzCssPathPseudoSelectorTag_Last,
   AzCssPathPseudoSelectorTag_NthChild,
   AzCssPathPseudoSelectorTag_Hover,
   AzCssPathPseudoSelectorTag_Active,
   AzCssPathPseudoSelectorTag_Focus,
};
typedef enum AzCssPathPseudoSelectorTag AzCssPathPseudoSelectorTag;

struct AzCssPathPseudoSelectorVariant_First { AzCssPathPseudoSelectorTag tag; };
typedef struct AzCssPathPseudoSelectorVariant_First AzCssPathPseudoSelectorVariant_First;

struct AzCssPathPseudoSelectorVariant_Last { AzCssPathPseudoSelectorTag tag; };
typedef struct AzCssPathPseudoSelectorVariant_Last AzCssPathPseudoSelectorVariant_Last;

struct AzCssPathPseudoSelectorVariant_NthChild { AzCssPathPseudoSelectorTag tag; AzCssNthChildSelector payload; };
typedef struct AzCssPathPseudoSelectorVariant_NthChild AzCssPathPseudoSelectorVariant_NthChild;

struct AzCssPathPseudoSelectorVariant_Hover { AzCssPathPseudoSelectorTag tag; };
typedef struct AzCssPathPseudoSelectorVariant_Hover AzCssPathPseudoSelectorVariant_Hover;

struct AzCssPathPseudoSelectorVariant_Active { AzCssPathPseudoSelectorTag tag; };
typedef struct AzCssPathPseudoSelectorVariant_Active AzCssPathPseudoSelectorVariant_Active;

struct AzCssPathPseudoSelectorVariant_Focus { AzCssPathPseudoSelectorTag tag; };
typedef struct AzCssPathPseudoSelectorVariant_Focus AzCssPathPseudoSelectorVariant_Focus;


union AzCssPathPseudoSelector {
    AzCssPathPseudoSelectorVariant_First First;
    AzCssPathPseudoSelectorVariant_Last Last;
    AzCssPathPseudoSelectorVariant_NthChild NthChild;
    AzCssPathPseudoSelectorVariant_Hover Hover;
    AzCssPathPseudoSelectorVariant_Active Active;
    AzCssPathPseudoSelectorVariant_Focus Focus;
};
typedef union AzCssPathPseudoSelector AzCssPathPseudoSelector;

struct AzLinearColorStop {
    AzOptionPercentageValue offset;
    AzColorU color;
};
typedef struct AzLinearColorStop AzLinearColorStop;

struct AzRadialColorStop {
    AzOptionAngleValue offset;
    AzColorU color;
};
typedef struct AzRadialColorStop AzRadialColorStop;

enum AzStyleTransformTag {
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
};
typedef enum AzStyleTransformTag AzStyleTransformTag;

struct AzStyleTransformVariant_Matrix { AzStyleTransformTag tag; AzStyleTransformMatrix2D payload; };
typedef struct AzStyleTransformVariant_Matrix AzStyleTransformVariant_Matrix;

struct AzStyleTransformVariant_Matrix3D { AzStyleTransformTag tag; AzStyleTransformMatrix3D payload; };
typedef struct AzStyleTransformVariant_Matrix3D AzStyleTransformVariant_Matrix3D;

struct AzStyleTransformVariant_Translate { AzStyleTransformTag tag; AzStyleTransformTranslate2D payload; };
typedef struct AzStyleTransformVariant_Translate AzStyleTransformVariant_Translate;

struct AzStyleTransformVariant_Translate3D { AzStyleTransformTag tag; AzStyleTransformTranslate3D payload; };
typedef struct AzStyleTransformVariant_Translate3D AzStyleTransformVariant_Translate3D;

struct AzStyleTransformVariant_TranslateX { AzStyleTransformTag tag; AzPixelValue payload; };
typedef struct AzStyleTransformVariant_TranslateX AzStyleTransformVariant_TranslateX;

struct AzStyleTransformVariant_TranslateY { AzStyleTransformTag tag; AzPixelValue payload; };
typedef struct AzStyleTransformVariant_TranslateY AzStyleTransformVariant_TranslateY;

struct AzStyleTransformVariant_TranslateZ { AzStyleTransformTag tag; AzPixelValue payload; };
typedef struct AzStyleTransformVariant_TranslateZ AzStyleTransformVariant_TranslateZ;

struct AzStyleTransformVariant_Rotate { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_Rotate AzStyleTransformVariant_Rotate;

struct AzStyleTransformVariant_Rotate3D { AzStyleTransformTag tag; AzStyleTransformRotate3D payload; };
typedef struct AzStyleTransformVariant_Rotate3D AzStyleTransformVariant_Rotate3D;

struct AzStyleTransformVariant_RotateX { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_RotateX AzStyleTransformVariant_RotateX;

struct AzStyleTransformVariant_RotateY { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_RotateY AzStyleTransformVariant_RotateY;

struct AzStyleTransformVariant_RotateZ { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_RotateZ AzStyleTransformVariant_RotateZ;

struct AzStyleTransformVariant_Scale { AzStyleTransformTag tag; AzStyleTransformScale2D payload; };
typedef struct AzStyleTransformVariant_Scale AzStyleTransformVariant_Scale;

struct AzStyleTransformVariant_Scale3D { AzStyleTransformTag tag; AzStyleTransformScale3D payload; };
typedef struct AzStyleTransformVariant_Scale3D AzStyleTransformVariant_Scale3D;

struct AzStyleTransformVariant_ScaleX { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_ScaleX AzStyleTransformVariant_ScaleX;

struct AzStyleTransformVariant_ScaleY { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_ScaleY AzStyleTransformVariant_ScaleY;

struct AzStyleTransformVariant_ScaleZ { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_ScaleZ AzStyleTransformVariant_ScaleZ;

struct AzStyleTransformVariant_Skew { AzStyleTransformTag tag; AzStyleTransformSkew2D payload; };
typedef struct AzStyleTransformVariant_Skew AzStyleTransformVariant_Skew;

struct AzStyleTransformVariant_SkewX { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_SkewX AzStyleTransformVariant_SkewX;

struct AzStyleTransformVariant_SkewY { AzStyleTransformTag tag; AzPercentageValue payload; };
typedef struct AzStyleTransformVariant_SkewY AzStyleTransformVariant_SkewY;

struct AzStyleTransformVariant_Perspective { AzStyleTransformTag tag; AzPixelValue payload; };
typedef struct AzStyleTransformVariant_Perspective AzStyleTransformVariant_Perspective;


union AzStyleTransform {
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
};
typedef union AzStyleTransform AzStyleTransform;

enum AzStyleBackgroundPositionVecValueTag {
   AzStyleBackgroundPositionVecValueTag_Auto,
   AzStyleBackgroundPositionVecValueTag_None,
   AzStyleBackgroundPositionVecValueTag_Inherit,
   AzStyleBackgroundPositionVecValueTag_Initial,
   AzStyleBackgroundPositionVecValueTag_Exact,
};
typedef enum AzStyleBackgroundPositionVecValueTag AzStyleBackgroundPositionVecValueTag;

struct AzStyleBackgroundPositionVecValueVariant_Auto { AzStyleBackgroundPositionVecValueTag tag; };
typedef struct AzStyleBackgroundPositionVecValueVariant_Auto AzStyleBackgroundPositionVecValueVariant_Auto;

struct AzStyleBackgroundPositionVecValueVariant_None { AzStyleBackgroundPositionVecValueTag tag; };
typedef struct AzStyleBackgroundPositionVecValueVariant_None AzStyleBackgroundPositionVecValueVariant_None;

struct AzStyleBackgroundPositionVecValueVariant_Inherit { AzStyleBackgroundPositionVecValueTag tag; };
typedef struct AzStyleBackgroundPositionVecValueVariant_Inherit AzStyleBackgroundPositionVecValueVariant_Inherit;

struct AzStyleBackgroundPositionVecValueVariant_Initial { AzStyleBackgroundPositionVecValueTag tag; };
typedef struct AzStyleBackgroundPositionVecValueVariant_Initial AzStyleBackgroundPositionVecValueVariant_Initial;

struct AzStyleBackgroundPositionVecValueVariant_Exact { AzStyleBackgroundPositionVecValueTag tag; AzStyleBackgroundPositionVec payload; };
typedef struct AzStyleBackgroundPositionVecValueVariant_Exact AzStyleBackgroundPositionVecValueVariant_Exact;


union AzStyleBackgroundPositionVecValue {
    AzStyleBackgroundPositionVecValueVariant_Auto Auto;
    AzStyleBackgroundPositionVecValueVariant_None None;
    AzStyleBackgroundPositionVecValueVariant_Inherit Inherit;
    AzStyleBackgroundPositionVecValueVariant_Initial Initial;
    AzStyleBackgroundPositionVecValueVariant_Exact Exact;
};
typedef union AzStyleBackgroundPositionVecValue AzStyleBackgroundPositionVecValue;

enum AzStyleBackgroundRepeatVecValueTag {
   AzStyleBackgroundRepeatVecValueTag_Auto,
   AzStyleBackgroundRepeatVecValueTag_None,
   AzStyleBackgroundRepeatVecValueTag_Inherit,
   AzStyleBackgroundRepeatVecValueTag_Initial,
   AzStyleBackgroundRepeatVecValueTag_Exact,
};
typedef enum AzStyleBackgroundRepeatVecValueTag AzStyleBackgroundRepeatVecValueTag;

struct AzStyleBackgroundRepeatVecValueVariant_Auto { AzStyleBackgroundRepeatVecValueTag tag; };
typedef struct AzStyleBackgroundRepeatVecValueVariant_Auto AzStyleBackgroundRepeatVecValueVariant_Auto;

struct AzStyleBackgroundRepeatVecValueVariant_None { AzStyleBackgroundRepeatVecValueTag tag; };
typedef struct AzStyleBackgroundRepeatVecValueVariant_None AzStyleBackgroundRepeatVecValueVariant_None;

struct AzStyleBackgroundRepeatVecValueVariant_Inherit { AzStyleBackgroundRepeatVecValueTag tag; };
typedef struct AzStyleBackgroundRepeatVecValueVariant_Inherit AzStyleBackgroundRepeatVecValueVariant_Inherit;

struct AzStyleBackgroundRepeatVecValueVariant_Initial { AzStyleBackgroundRepeatVecValueTag tag; };
typedef struct AzStyleBackgroundRepeatVecValueVariant_Initial AzStyleBackgroundRepeatVecValueVariant_Initial;

struct AzStyleBackgroundRepeatVecValueVariant_Exact { AzStyleBackgroundRepeatVecValueTag tag; AzStyleBackgroundRepeatVec payload; };
typedef struct AzStyleBackgroundRepeatVecValueVariant_Exact AzStyleBackgroundRepeatVecValueVariant_Exact;


union AzStyleBackgroundRepeatVecValue {
    AzStyleBackgroundRepeatVecValueVariant_Auto Auto;
    AzStyleBackgroundRepeatVecValueVariant_None None;
    AzStyleBackgroundRepeatVecValueVariant_Inherit Inherit;
    AzStyleBackgroundRepeatVecValueVariant_Initial Initial;
    AzStyleBackgroundRepeatVecValueVariant_Exact Exact;
};
typedef union AzStyleBackgroundRepeatVecValue AzStyleBackgroundRepeatVecValue;

enum AzStyleBackgroundSizeVecValueTag {
   AzStyleBackgroundSizeVecValueTag_Auto,
   AzStyleBackgroundSizeVecValueTag_None,
   AzStyleBackgroundSizeVecValueTag_Inherit,
   AzStyleBackgroundSizeVecValueTag_Initial,
   AzStyleBackgroundSizeVecValueTag_Exact,
};
typedef enum AzStyleBackgroundSizeVecValueTag AzStyleBackgroundSizeVecValueTag;

struct AzStyleBackgroundSizeVecValueVariant_Auto { AzStyleBackgroundSizeVecValueTag tag; };
typedef struct AzStyleBackgroundSizeVecValueVariant_Auto AzStyleBackgroundSizeVecValueVariant_Auto;

struct AzStyleBackgroundSizeVecValueVariant_None { AzStyleBackgroundSizeVecValueTag tag; };
typedef struct AzStyleBackgroundSizeVecValueVariant_None AzStyleBackgroundSizeVecValueVariant_None;

struct AzStyleBackgroundSizeVecValueVariant_Inherit { AzStyleBackgroundSizeVecValueTag tag; };
typedef struct AzStyleBackgroundSizeVecValueVariant_Inherit AzStyleBackgroundSizeVecValueVariant_Inherit;

struct AzStyleBackgroundSizeVecValueVariant_Initial { AzStyleBackgroundSizeVecValueTag tag; };
typedef struct AzStyleBackgroundSizeVecValueVariant_Initial AzStyleBackgroundSizeVecValueVariant_Initial;

struct AzStyleBackgroundSizeVecValueVariant_Exact { AzStyleBackgroundSizeVecValueTag tag; AzStyleBackgroundSizeVec payload; };
typedef struct AzStyleBackgroundSizeVecValueVariant_Exact AzStyleBackgroundSizeVecValueVariant_Exact;


union AzStyleBackgroundSizeVecValue {
    AzStyleBackgroundSizeVecValueVariant_Auto Auto;
    AzStyleBackgroundSizeVecValueVariant_None None;
    AzStyleBackgroundSizeVecValueVariant_Inherit Inherit;
    AzStyleBackgroundSizeVecValueVariant_Initial Initial;
    AzStyleBackgroundSizeVecValueVariant_Exact Exact;
};
typedef union AzStyleBackgroundSizeVecValue AzStyleBackgroundSizeVecValue;

struct AzStyledNode {
    AzStyledNodeState state;
    AzOptionTagId tag_id;
};
typedef struct AzStyledNode AzStyledNode;

struct AzTagIdToNodeIdMapping {
    AzTagId tag_id;
    AzNodeId node_id;
    AzOptionTabIndex tab_index;
};
typedef struct AzTagIdToNodeIdMapping AzTagIdToNodeIdMapping;

struct AzGetProgramBinaryReturn {
    AzU8Vec _0;
    uint32_t _1;
};
typedef struct AzGetProgramBinaryReturn AzGetProgramBinaryReturn;

struct AzRawImage {
    AzU8Vec pixels;
    size_t width;
    size_t height;
    AzRawImageFormat data_format;
};
typedef struct AzRawImage AzRawImage;

enum AzSvgPathElementTag {
   AzSvgPathElementTag_Line,
   AzSvgPathElementTag_QuadraticCurve,
   AzSvgPathElementTag_CubicCurve,
};
typedef enum AzSvgPathElementTag AzSvgPathElementTag;

struct AzSvgPathElementVariant_Line { AzSvgPathElementTag tag; AzSvgLine payload; };
typedef struct AzSvgPathElementVariant_Line AzSvgPathElementVariant_Line;

struct AzSvgPathElementVariant_QuadraticCurve { AzSvgPathElementTag tag; AzSvgQuadraticCurve payload; };
typedef struct AzSvgPathElementVariant_QuadraticCurve AzSvgPathElementVariant_QuadraticCurve;

struct AzSvgPathElementVariant_CubicCurve { AzSvgPathElementTag tag; AzSvgCubicCurve payload; };
typedef struct AzSvgPathElementVariant_CubicCurve AzSvgPathElementVariant_CubicCurve;


union AzSvgPathElement {
    AzSvgPathElementVariant_Line Line;
    AzSvgPathElementVariant_QuadraticCurve QuadraticCurve;
    AzSvgPathElementVariant_CubicCurve CubicCurve;
};
typedef union AzSvgPathElement AzSvgPathElement;

struct AzTesselatedCPUSvgNode {
    AzSvgVertexVec vertices;
    AzU32Vec indices;
};
typedef struct AzTesselatedCPUSvgNode AzTesselatedCPUSvgNode;

struct AzSvgRenderOptions {
    AzOptionColorU background_color;
    AzSvgFitTo fit;
};
typedef struct AzSvgRenderOptions AzSvgRenderOptions;

struct AzSvgStrokeStyle {
    AzSvgLineCap start_cap;
    AzSvgLineCap end_cap;
    AzSvgLineJoin line_join;
    AzOptionSvgDashPattern dash_pattern;
    size_t line_width;
    size_t miter_limit;
    size_t tolerance;
    bool  apply_line_width;
};
typedef struct AzSvgStrokeStyle AzSvgStrokeStyle;

struct AzString {
    AzU8Vec vec;
};
typedef struct AzString AzString;

struct AzStyleTransformVec {
    AzStyleTransform* const ptr;
    size_t len;
    size_t cap;
    AzStyleTransformVecDestructor destructor;
};
typedef struct AzStyleTransformVec AzStyleTransformVec;

struct AzSvgPathElementVec {
    AzSvgPathElement* const ptr;
    size_t len;
    size_t cap;
    AzSvgPathElementVecDestructor destructor;
};
typedef struct AzSvgPathElementVec AzSvgPathElementVec;

struct AzStringVec {
    AzString* const ptr;
    size_t len;
    size_t cap;
    AzStringVecDestructor destructor;
};
typedef struct AzStringVec AzStringVec;

struct AzLinearColorStopVec {
    AzLinearColorStop* const ptr;
    size_t len;
    size_t cap;
    AzLinearColorStopVecDestructor destructor;
};
typedef struct AzLinearColorStopVec AzLinearColorStopVec;

struct AzRadialColorStopVec {
    AzRadialColorStop* const ptr;
    size_t len;
    size_t cap;
    AzRadialColorStopVecDestructor destructor;
};
typedef struct AzRadialColorStopVec AzRadialColorStopVec;

struct AzStyledNodeVec {
    AzStyledNode* const ptr;
    size_t len;
    size_t cap;
    AzStyledNodeVecDestructor destructor;
};
typedef struct AzStyledNodeVec AzStyledNodeVec;

struct AzTagIdsToNodeIdsMappingVec {
    AzTagIdToNodeIdMapping* const ptr;
    size_t len;
    size_t cap;
    AzTagIdsToNodeIdsMappingVecDestructor destructor;
};
typedef struct AzTagIdsToNodeIdsMappingVec AzTagIdsToNodeIdsMappingVec;

enum AzOptionRawImageTag {
   AzOptionRawImageTag_None,
   AzOptionRawImageTag_Some,
};
typedef enum AzOptionRawImageTag AzOptionRawImageTag;

struct AzOptionRawImageVariant_None { AzOptionRawImageTag tag; };
typedef struct AzOptionRawImageVariant_None AzOptionRawImageVariant_None;

struct AzOptionRawImageVariant_Some { AzOptionRawImageTag tag; AzRawImage payload; };
typedef struct AzOptionRawImageVariant_Some AzOptionRawImageVariant_Some;


union AzOptionRawImage {
    AzOptionRawImageVariant_None None;
    AzOptionRawImageVariant_Some Some;
};
typedef union AzOptionRawImage AzOptionRawImage;

enum AzOptionTaskBarIconTag {
   AzOptionTaskBarIconTag_None,
   AzOptionTaskBarIconTag_Some,
};
typedef enum AzOptionTaskBarIconTag AzOptionTaskBarIconTag;

struct AzOptionTaskBarIconVariant_None { AzOptionTaskBarIconTag tag; };
typedef struct AzOptionTaskBarIconVariant_None AzOptionTaskBarIconVariant_None;

struct AzOptionTaskBarIconVariant_Some { AzOptionTaskBarIconTag tag; AzTaskBarIcon payload; };
typedef struct AzOptionTaskBarIconVariant_Some AzOptionTaskBarIconVariant_Some;


union AzOptionTaskBarIcon {
    AzOptionTaskBarIconVariant_None None;
    AzOptionTaskBarIconVariant_Some Some;
};
typedef union AzOptionTaskBarIcon AzOptionTaskBarIcon;

enum AzOptionWindowIconTag {
   AzOptionWindowIconTag_None,
   AzOptionWindowIconTag_Some,
};
typedef enum AzOptionWindowIconTag AzOptionWindowIconTag;

struct AzOptionWindowIconVariant_None { AzOptionWindowIconTag tag; };
typedef struct AzOptionWindowIconVariant_None AzOptionWindowIconVariant_None;

struct AzOptionWindowIconVariant_Some { AzOptionWindowIconTag tag; AzWindowIcon payload; };
typedef struct AzOptionWindowIconVariant_Some AzOptionWindowIconVariant_Some;


union AzOptionWindowIcon {
    AzOptionWindowIconVariant_None None;
    AzOptionWindowIconVariant_Some Some;
};
typedef union AzOptionWindowIcon AzOptionWindowIcon;

enum AzOptionStringTag {
   AzOptionStringTag_None,
   AzOptionStringTag_Some,
};
typedef enum AzOptionStringTag AzOptionStringTag;

struct AzOptionStringVariant_None { AzOptionStringTag tag; };
typedef struct AzOptionStringVariant_None AzOptionStringVariant_None;

struct AzOptionStringVariant_Some { AzOptionStringTag tag; AzString payload; };
typedef struct AzOptionStringVariant_Some AzOptionStringVariant_Some;


union AzOptionString {
    AzOptionStringVariant_None None;
    AzOptionStringVariant_Some Some;
};
typedef union AzOptionString AzOptionString;

enum AzOptionDurationTag {
   AzOptionDurationTag_None,
   AzOptionDurationTag_Some,
};
typedef enum AzOptionDurationTag AzOptionDurationTag;

struct AzOptionDurationVariant_None { AzOptionDurationTag tag; };
typedef struct AzOptionDurationVariant_None AzOptionDurationVariant_None;

struct AzOptionDurationVariant_Some { AzOptionDurationTag tag; AzDuration payload; };
typedef struct AzOptionDurationVariant_Some AzOptionDurationVariant_Some;


union AzOptionDuration {
    AzOptionDurationVariant_None None;
    AzOptionDurationVariant_Some Some;
};
typedef union AzOptionDuration AzOptionDuration;

struct AzDuplicatedNamespaceError {
    AzString ns;
    AzSvgParseErrorPosition pos;
};
typedef struct AzDuplicatedNamespaceError AzDuplicatedNamespaceError;

struct AzUnknownNamespaceError {
    AzString ns;
    AzSvgParseErrorPosition pos;
};
typedef struct AzUnknownNamespaceError AzUnknownNamespaceError;

struct AzUnexpectedCloseTagError {
    AzString expected;
    AzString actual;
    AzSvgParseErrorPosition pos;
};
typedef struct AzUnexpectedCloseTagError AzUnexpectedCloseTagError;

struct AzUnknownEntityReferenceError {
    AzString entity;
    AzSvgParseErrorPosition pos;
};
typedef struct AzUnknownEntityReferenceError AzUnknownEntityReferenceError;

struct AzDuplicatedAttributeError {
    AzString attribute;
    AzSvgParseErrorPosition pos;
};
typedef struct AzDuplicatedAttributeError AzDuplicatedAttributeError;

struct AzInvalidStringError {
    AzString got;
    AzSvgParseErrorPosition pos;
};
typedef struct AzInvalidStringError AzInvalidStringError;

enum AzInstantTag {
   AzInstantTag_System,
   AzInstantTag_Tick,
};
typedef enum AzInstantTag AzInstantTag;

struct AzInstantVariant_System { AzInstantTag tag; AzInstantPtr payload; };
typedef struct AzInstantVariant_System AzInstantVariant_System;

struct AzInstantVariant_Tick { AzInstantTag tag; AzSystemTick payload; };
typedef struct AzInstantVariant_Tick AzInstantVariant_Tick;


union AzInstant {
    AzInstantVariant_System System;
    AzInstantVariant_Tick Tick;
};
typedef union AzInstant AzInstant;

struct AzWindowsWindowOptions {
    bool  allow_drag_drop;
    bool  no_redirection_bitmap;
    AzOptionWindowIcon window_icon;
    AzOptionTaskBarIcon taskbar_icon;
    AzOptionHwndHandle parent_window;
};
typedef struct AzWindowsWindowOptions AzWindowsWindowOptions;

struct AzWaylandTheme {
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
};
typedef struct AzWaylandTheme AzWaylandTheme;

struct AzStringPair {
    AzString key;
    AzString value;
};
typedef struct AzStringPair AzStringPair;

struct AzMonitor {
    AzMonitorHandle handle;
    AzOptionString name;
    AzLayoutSize size;
    AzLayoutPoint position;
    double scale_factor;
    AzVideoModeVec video_modes;
    bool  is_primary_monitor;
};
typedef struct AzMonitor AzMonitor;

struct AzRefCountInner {
    size_t num_copies;
    size_t num_refs;
    size_t num_mutable_refs;
    size_t _internal_len;
    size_t _internal_layout_size;
    size_t _internal_layout_align;
    uint64_t type_id;
    AzString type_name;
    AzRefAnyDestructorType custom_destructor;
};
typedef struct AzRefCountInner AzRefCountInner;

struct AzRefCount {
    AzRefCountInner* const ptr;
};
typedef struct AzRefCount AzRefCount;

struct AzRefAny {
    void* const _internal_ptr;
    bool  is_dead;
    AzRefCount sharing_info;
};
typedef struct AzRefAny AzRefAny;

struct AzGlTextureNode {
    AzGlCallback callback;
    AzRefAny data;
};
typedef struct AzGlTextureNode AzGlTextureNode;

struct AzIFrameNode {
    AzIFrameCallback callback;
    AzRefAny data;
};
typedef struct AzIFrameNode AzIFrameNode;

struct AzCallbackData {
    AzEventFilter event;
    AzCallback callback;
    AzRefAny data;
};
typedef struct AzCallbackData AzCallbackData;

enum AzNodeTypeTag {
   AzNodeTypeTag_Div,
   AzNodeTypeTag_Body,
   AzNodeTypeTag_Br,
   AzNodeTypeTag_Label,
   AzNodeTypeTag_Image,
   AzNodeTypeTag_IFrame,
   AzNodeTypeTag_GlTexture,
};
typedef enum AzNodeTypeTag AzNodeTypeTag;

struct AzNodeTypeVariant_Div { AzNodeTypeTag tag; };
typedef struct AzNodeTypeVariant_Div AzNodeTypeVariant_Div;

struct AzNodeTypeVariant_Body { AzNodeTypeTag tag; };
typedef struct AzNodeTypeVariant_Body AzNodeTypeVariant_Body;

struct AzNodeTypeVariant_Br { AzNodeTypeTag tag; };
typedef struct AzNodeTypeVariant_Br AzNodeTypeVariant_Br;

struct AzNodeTypeVariant_Label { AzNodeTypeTag tag; AzString payload; };
typedef struct AzNodeTypeVariant_Label AzNodeTypeVariant_Label;

struct AzNodeTypeVariant_Image { AzNodeTypeTag tag; AzImageId payload; };
typedef struct AzNodeTypeVariant_Image AzNodeTypeVariant_Image;

struct AzNodeTypeVariant_IFrame { AzNodeTypeTag tag; AzIFrameNode payload; };
typedef struct AzNodeTypeVariant_IFrame AzNodeTypeVariant_IFrame;

struct AzNodeTypeVariant_GlTexture { AzNodeTypeTag tag; AzGlTextureNode payload; };
typedef struct AzNodeTypeVariant_GlTexture AzNodeTypeVariant_GlTexture;


union AzNodeType {
    AzNodeTypeVariant_Div Div;
    AzNodeTypeVariant_Body Body;
    AzNodeTypeVariant_Br Br;
    AzNodeTypeVariant_Label Label;
    AzNodeTypeVariant_Image Image;
    AzNodeTypeVariant_IFrame IFrame;
    AzNodeTypeVariant_GlTexture GlTexture;
};
typedef union AzNodeType AzNodeType;

enum AzIdOrClassTag {
   AzIdOrClassTag_Id,
   AzIdOrClassTag_Class,
};
typedef enum AzIdOrClassTag AzIdOrClassTag;

struct AzIdOrClassVariant_Id { AzIdOrClassTag tag; AzString payload; };
typedef struct AzIdOrClassVariant_Id AzIdOrClassVariant_Id;

struct AzIdOrClassVariant_Class { AzIdOrClassTag tag; AzString payload; };
typedef struct AzIdOrClassVariant_Class AzIdOrClassVariant_Class;


union AzIdOrClass {
    AzIdOrClassVariant_Id Id;
    AzIdOrClassVariant_Class Class;
};
typedef union AzIdOrClass AzIdOrClass;

enum AzCssPathSelectorTag {
   AzCssPathSelectorTag_Global,
   AzCssPathSelectorTag_Type,
   AzCssPathSelectorTag_Class,
   AzCssPathSelectorTag_Id,
   AzCssPathSelectorTag_PseudoSelector,
   AzCssPathSelectorTag_DirectChildren,
   AzCssPathSelectorTag_Children,
};
typedef enum AzCssPathSelectorTag AzCssPathSelectorTag;

struct AzCssPathSelectorVariant_Global { AzCssPathSelectorTag tag; };
typedef struct AzCssPathSelectorVariant_Global AzCssPathSelectorVariant_Global;

struct AzCssPathSelectorVariant_Type { AzCssPathSelectorTag tag; AzNodeTypePath payload; };
typedef struct AzCssPathSelectorVariant_Type AzCssPathSelectorVariant_Type;

struct AzCssPathSelectorVariant_Class { AzCssPathSelectorTag tag; AzString payload; };
typedef struct AzCssPathSelectorVariant_Class AzCssPathSelectorVariant_Class;

struct AzCssPathSelectorVariant_Id { AzCssPathSelectorTag tag; AzString payload; };
typedef struct AzCssPathSelectorVariant_Id AzCssPathSelectorVariant_Id;

struct AzCssPathSelectorVariant_PseudoSelector { AzCssPathSelectorTag tag; AzCssPathPseudoSelector payload; };
typedef struct AzCssPathSelectorVariant_PseudoSelector AzCssPathSelectorVariant_PseudoSelector;

struct AzCssPathSelectorVariant_DirectChildren { AzCssPathSelectorTag tag; };
typedef struct AzCssPathSelectorVariant_DirectChildren AzCssPathSelectorVariant_DirectChildren;

struct AzCssPathSelectorVariant_Children { AzCssPathSelectorTag tag; };
typedef struct AzCssPathSelectorVariant_Children AzCssPathSelectorVariant_Children;


union AzCssPathSelector {
    AzCssPathSelectorVariant_Global Global;
    AzCssPathSelectorVariant_Type Type;
    AzCssPathSelectorVariant_Class Class;
    AzCssPathSelectorVariant_Id Id;
    AzCssPathSelectorVariant_PseudoSelector PseudoSelector;
    AzCssPathSelectorVariant_DirectChildren DirectChildren;
    AzCssPathSelectorVariant_Children Children;
};
typedef union AzCssPathSelector AzCssPathSelector;

struct AzLinearGradient {
    AzDirection direction;
    AzExtendMode extend_mode;
    AzLinearColorStopVec stops;
};
typedef struct AzLinearGradient AzLinearGradient;

struct AzRadialGradient {
    AzShape shape;
    AzRadialGradientSize size;
    AzStyleBackgroundPosition position;
    AzExtendMode extend_mode;
    AzLinearColorStopVec stops;
};
typedef struct AzRadialGradient AzRadialGradient;

struct AzConicGradient {
    AzExtendMode extend_mode;
    AzStyleBackgroundPosition center;
    AzAngleValue angle;
    AzRadialColorStopVec stops;
};
typedef struct AzConicGradient AzConicGradient;

struct AzCssImageId {
    AzString inner;
};
typedef struct AzCssImageId AzCssImageId;

enum AzStyleBackgroundContentTag {
   AzStyleBackgroundContentTag_LinearGradient,
   AzStyleBackgroundContentTag_RadialGradient,
   AzStyleBackgroundContentTag_ConicGradient,
   AzStyleBackgroundContentTag_Image,
   AzStyleBackgroundContentTag_Color,
};
typedef enum AzStyleBackgroundContentTag AzStyleBackgroundContentTag;

struct AzStyleBackgroundContentVariant_LinearGradient { AzStyleBackgroundContentTag tag; AzLinearGradient payload; };
typedef struct AzStyleBackgroundContentVariant_LinearGradient AzStyleBackgroundContentVariant_LinearGradient;

struct AzStyleBackgroundContentVariant_RadialGradient { AzStyleBackgroundContentTag tag; AzRadialGradient payload; };
typedef struct AzStyleBackgroundContentVariant_RadialGradient AzStyleBackgroundContentVariant_RadialGradient;

struct AzStyleBackgroundContentVariant_ConicGradient { AzStyleBackgroundContentTag tag; AzConicGradient payload; };
typedef struct AzStyleBackgroundContentVariant_ConicGradient AzStyleBackgroundContentVariant_ConicGradient;

struct AzStyleBackgroundContentVariant_Image { AzStyleBackgroundContentTag tag; AzCssImageId payload; };
typedef struct AzStyleBackgroundContentVariant_Image AzStyleBackgroundContentVariant_Image;

struct AzStyleBackgroundContentVariant_Color { AzStyleBackgroundContentTag tag; AzColorU payload; };
typedef struct AzStyleBackgroundContentVariant_Color AzStyleBackgroundContentVariant_Color;


union AzStyleBackgroundContent {
    AzStyleBackgroundContentVariant_LinearGradient LinearGradient;
    AzStyleBackgroundContentVariant_RadialGradient RadialGradient;
    AzStyleBackgroundContentVariant_ConicGradient ConicGradient;
    AzStyleBackgroundContentVariant_Image Image;
    AzStyleBackgroundContentVariant_Color Color;
};
typedef union AzStyleBackgroundContent AzStyleBackgroundContent;

struct AzScrollbarInfo {
    AzLayoutWidth width;
    AzLayoutPaddingLeft padding_left;
    AzLayoutPaddingRight padding_right;
    AzStyleBackgroundContent track;
    AzStyleBackgroundContent thumb;
    AzStyleBackgroundContent button;
    AzStyleBackgroundContent corner;
    AzStyleBackgroundContent resizer;
};
typedef struct AzScrollbarInfo AzScrollbarInfo;

struct AzScrollbarStyle {
    AzScrollbarInfo horizontal;
    AzScrollbarInfo vertical;
};
typedef struct AzScrollbarStyle AzScrollbarStyle;

struct AzStyleFontFamily {
    AzStringVec fonts;
};
typedef struct AzStyleFontFamily AzStyleFontFamily;

enum AzScrollbarStyleValueTag {
   AzScrollbarStyleValueTag_Auto,
   AzScrollbarStyleValueTag_None,
   AzScrollbarStyleValueTag_Inherit,
   AzScrollbarStyleValueTag_Initial,
   AzScrollbarStyleValueTag_Exact,
};
typedef enum AzScrollbarStyleValueTag AzScrollbarStyleValueTag;

struct AzScrollbarStyleValueVariant_Auto { AzScrollbarStyleValueTag tag; };
typedef struct AzScrollbarStyleValueVariant_Auto AzScrollbarStyleValueVariant_Auto;

struct AzScrollbarStyleValueVariant_None { AzScrollbarStyleValueTag tag; };
typedef struct AzScrollbarStyleValueVariant_None AzScrollbarStyleValueVariant_None;

struct AzScrollbarStyleValueVariant_Inherit { AzScrollbarStyleValueTag tag; };
typedef struct AzScrollbarStyleValueVariant_Inherit AzScrollbarStyleValueVariant_Inherit;

struct AzScrollbarStyleValueVariant_Initial { AzScrollbarStyleValueTag tag; };
typedef struct AzScrollbarStyleValueVariant_Initial AzScrollbarStyleValueVariant_Initial;

struct AzScrollbarStyleValueVariant_Exact { AzScrollbarStyleValueTag tag; AzScrollbarStyle payload; };
typedef struct AzScrollbarStyleValueVariant_Exact AzScrollbarStyleValueVariant_Exact;


union AzScrollbarStyleValue {
    AzScrollbarStyleValueVariant_Auto Auto;
    AzScrollbarStyleValueVariant_None None;
    AzScrollbarStyleValueVariant_Inherit Inherit;
    AzScrollbarStyleValueVariant_Initial Initial;
    AzScrollbarStyleValueVariant_Exact Exact;
};
typedef union AzScrollbarStyleValue AzScrollbarStyleValue;

enum AzStyleFontFamilyValueTag {
   AzStyleFontFamilyValueTag_Auto,
   AzStyleFontFamilyValueTag_None,
   AzStyleFontFamilyValueTag_Inherit,
   AzStyleFontFamilyValueTag_Initial,
   AzStyleFontFamilyValueTag_Exact,
};
typedef enum AzStyleFontFamilyValueTag AzStyleFontFamilyValueTag;

struct AzStyleFontFamilyValueVariant_Auto { AzStyleFontFamilyValueTag tag; };
typedef struct AzStyleFontFamilyValueVariant_Auto AzStyleFontFamilyValueVariant_Auto;

struct AzStyleFontFamilyValueVariant_None { AzStyleFontFamilyValueTag tag; };
typedef struct AzStyleFontFamilyValueVariant_None AzStyleFontFamilyValueVariant_None;

struct AzStyleFontFamilyValueVariant_Inherit { AzStyleFontFamilyValueTag tag; };
typedef struct AzStyleFontFamilyValueVariant_Inherit AzStyleFontFamilyValueVariant_Inherit;

struct AzStyleFontFamilyValueVariant_Initial { AzStyleFontFamilyValueTag tag; };
typedef struct AzStyleFontFamilyValueVariant_Initial AzStyleFontFamilyValueVariant_Initial;

struct AzStyleFontFamilyValueVariant_Exact { AzStyleFontFamilyValueTag tag; AzStyleFontFamily payload; };
typedef struct AzStyleFontFamilyValueVariant_Exact AzStyleFontFamilyValueVariant_Exact;


union AzStyleFontFamilyValue {
    AzStyleFontFamilyValueVariant_Auto Auto;
    AzStyleFontFamilyValueVariant_None None;
    AzStyleFontFamilyValueVariant_Inherit Inherit;
    AzStyleFontFamilyValueVariant_Initial Initial;
    AzStyleFontFamilyValueVariant_Exact Exact;
};
typedef union AzStyleFontFamilyValue AzStyleFontFamilyValue;

enum AzStyleTransformVecValueTag {
   AzStyleTransformVecValueTag_Auto,
   AzStyleTransformVecValueTag_None,
   AzStyleTransformVecValueTag_Inherit,
   AzStyleTransformVecValueTag_Initial,
   AzStyleTransformVecValueTag_Exact,
};
typedef enum AzStyleTransformVecValueTag AzStyleTransformVecValueTag;

struct AzStyleTransformVecValueVariant_Auto { AzStyleTransformVecValueTag tag; };
typedef struct AzStyleTransformVecValueVariant_Auto AzStyleTransformVecValueVariant_Auto;

struct AzStyleTransformVecValueVariant_None { AzStyleTransformVecValueTag tag; };
typedef struct AzStyleTransformVecValueVariant_None AzStyleTransformVecValueVariant_None;

struct AzStyleTransformVecValueVariant_Inherit { AzStyleTransformVecValueTag tag; };
typedef struct AzStyleTransformVecValueVariant_Inherit AzStyleTransformVecValueVariant_Inherit;

struct AzStyleTransformVecValueVariant_Initial { AzStyleTransformVecValueTag tag; };
typedef struct AzStyleTransformVecValueVariant_Initial AzStyleTransformVecValueVariant_Initial;

struct AzStyleTransformVecValueVariant_Exact { AzStyleTransformVecValueTag tag; AzStyleTransformVec payload; };
typedef struct AzStyleTransformVecValueVariant_Exact AzStyleTransformVecValueVariant_Exact;


union AzStyleTransformVecValue {
    AzStyleTransformVecValueVariant_Auto Auto;
    AzStyleTransformVecValueVariant_None None;
    AzStyleTransformVecValueVariant_Inherit Inherit;
    AzStyleTransformVecValueVariant_Initial Initial;
    AzStyleTransformVecValueVariant_Exact Exact;
};
typedef union AzStyleTransformVecValue AzStyleTransformVecValue;

struct AzVertexAttribute {
    AzString name;
    AzOptionUsize layout_location;
    AzVertexAttributeType attribute_type;
    size_t item_count;
};
typedef struct AzVertexAttribute AzVertexAttribute;

struct AzDebugMessage {
    AzString message;
    uint32_t source;
    uint32_t ty;
    uint32_t id;
    uint32_t severity;
};
typedef struct AzDebugMessage AzDebugMessage;

struct AzGetActiveAttribReturn {
    int32_t _0;
    uint32_t _1;
    AzString _2;
};
typedef struct AzGetActiveAttribReturn AzGetActiveAttribReturn;

struct AzGetActiveUniformReturn {
    int32_t _0;
    uint32_t _1;
    AzString _2;
};
typedef struct AzGetActiveUniformReturn AzGetActiveUniformReturn;

enum AzImageSourceTag {
   AzImageSourceTag_Embedded,
   AzImageSourceTag_File,
   AzImageSourceTag_Raw,
};
typedef enum AzImageSourceTag AzImageSourceTag;

struct AzImageSourceVariant_Embedded { AzImageSourceTag tag; AzU8Vec payload; };
typedef struct AzImageSourceVariant_Embedded AzImageSourceVariant_Embedded;

struct AzImageSourceVariant_File { AzImageSourceTag tag; AzString payload; };
typedef struct AzImageSourceVariant_File AzImageSourceVariant_File;

struct AzImageSourceVariant_Raw { AzImageSourceTag tag; AzRawImage payload; };
typedef struct AzImageSourceVariant_Raw AzImageSourceVariant_Raw;


union AzImageSource {
    AzImageSourceVariant_Embedded Embedded;
    AzImageSourceVariant_File File;
    AzImageSourceVariant_Raw Raw;
};
typedef union AzImageSource AzImageSource;

struct AzEmbeddedFontSource {
    AzString postscript_id;
    AzU8Vec font_data;
    bool  load_glyph_outlines;
};
typedef struct AzEmbeddedFontSource AzEmbeddedFontSource;

struct AzFileFontSource {
    AzString postscript_id;
    AzString file_path;
    bool  load_glyph_outlines;
};
typedef struct AzFileFontSource AzFileFontSource;

struct AzSystemFontSource {
    AzString postscript_id;
    bool  load_glyph_outlines;
};
typedef struct AzSystemFontSource AzSystemFontSource;

struct AzSvgPath {
    AzSvgPathElementVec items;
};
typedef struct AzSvgPath AzSvgPath;

struct AzSvgParseOptions {
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
};
typedef struct AzSvgParseOptions AzSvgParseOptions;

enum AzSvgStyleTag {
   AzSvgStyleTag_Fill,
   AzSvgStyleTag_Stroke,
};
typedef enum AzSvgStyleTag AzSvgStyleTag;

struct AzSvgStyleVariant_Fill { AzSvgStyleTag tag; AzSvgFillStyle payload; };
typedef struct AzSvgStyleVariant_Fill AzSvgStyleVariant_Fill;

struct AzSvgStyleVariant_Stroke { AzSvgStyleTag tag; AzSvgStrokeStyle payload; };
typedef struct AzSvgStyleVariant_Stroke AzSvgStyleVariant_Stroke;


union AzSvgStyle {
    AzSvgStyleVariant_Fill Fill;
    AzSvgStyleVariant_Stroke Stroke;
};
typedef union AzSvgStyle AzSvgStyle;

struct AzThread {
    void* restrict thread_handle;
    void* restrict sender;
    void* restrict receiver;
    AzRefAny writeback_data;
    void* restrict dropcheck;
    AzCheckThreadFinishedFn check_thread_finished_fn;
    AzLibrarySendThreadMsgFn send_thread_msg_fn;
    AzLibraryReceiveThreadMsgFn receive_thread_msg_fn;
    AzThreadDestructorFn thread_destructor_fn;
};
typedef struct AzThread AzThread;

struct AzThreadWriteBackMsg {
    AzRefAny data;
    AzWriteBackCallback callback;
};
typedef struct AzThreadWriteBackMsg AzThreadWriteBackMsg;

struct AzMonitorVec {
    AzMonitor* const ptr;
    size_t len;
    size_t cap;
    AzMonitorVecDestructor destructor;
};
typedef struct AzMonitorVec AzMonitorVec;

struct AzIdOrClassVec {
    AzIdOrClass* const ptr;
    size_t len;
    size_t cap;
    AzIdOrClassVecDestructor destructor;
};
typedef struct AzIdOrClassVec AzIdOrClassVec;

struct AzStyleBackgroundContentVec {
    AzStyleBackgroundContent* const ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundContentVecDestructor destructor;
};
typedef struct AzStyleBackgroundContentVec AzStyleBackgroundContentVec;

struct AzSvgPathVec {
    AzSvgPath* const ptr;
    size_t len;
    size_t cap;
    AzSvgPathVecDestructor destructor;
};
typedef struct AzSvgPathVec AzSvgPathVec;

struct AzVertexAttributeVec {
    AzVertexAttribute* const ptr;
    size_t len;
    size_t cap;
    AzVertexAttributeVecDestructor destructor;
};
typedef struct AzVertexAttributeVec AzVertexAttributeVec;

struct AzCssPathSelectorVec {
    AzCssPathSelector* const ptr;
    size_t len;
    size_t cap;
    AzCssPathSelectorVecDestructor destructor;
};
typedef struct AzCssPathSelectorVec AzCssPathSelectorVec;

struct AzCallbackDataVec {
    AzCallbackData* const ptr;
    size_t len;
    size_t cap;
    AzCallbackDataVecDestructor destructor;
};
typedef struct AzCallbackDataVec AzCallbackDataVec;

struct AzDebugMessageVec {
    AzDebugMessage* const ptr;
    size_t len;
    size_t cap;
    AzDebugMessageVecDestructor destructor;
};
typedef struct AzDebugMessageVec AzDebugMessageVec;

struct AzStringPairVec {
    AzStringPair* const ptr;
    size_t len;
    size_t cap;
    AzStringPairVecDestructor destructor;
};
typedef struct AzStringPairVec AzStringPairVec;

enum AzOptionRefAnyTag {
   AzOptionRefAnyTag_None,
   AzOptionRefAnyTag_Some,
};
typedef enum AzOptionRefAnyTag AzOptionRefAnyTag;

struct AzOptionRefAnyVariant_None { AzOptionRefAnyTag tag; };
typedef struct AzOptionRefAnyVariant_None AzOptionRefAnyVariant_None;

struct AzOptionRefAnyVariant_Some { AzOptionRefAnyTag tag; AzRefAny payload; };
typedef struct AzOptionRefAnyVariant_Some AzOptionRefAnyVariant_Some;


union AzOptionRefAny {
    AzOptionRefAnyVariant_None None;
    AzOptionRefAnyVariant_Some Some;
};
typedef union AzOptionRefAny AzOptionRefAny;

enum AzOptionWaylandThemeTag {
   AzOptionWaylandThemeTag_None,
   AzOptionWaylandThemeTag_Some,
};
typedef enum AzOptionWaylandThemeTag AzOptionWaylandThemeTag;

struct AzOptionWaylandThemeVariant_None { AzOptionWaylandThemeTag tag; };
typedef struct AzOptionWaylandThemeVariant_None AzOptionWaylandThemeVariant_None;

struct AzOptionWaylandThemeVariant_Some { AzOptionWaylandThemeTag tag; AzWaylandTheme payload; };
typedef struct AzOptionWaylandThemeVariant_Some AzOptionWaylandThemeVariant_Some;


union AzOptionWaylandTheme {
    AzOptionWaylandThemeVariant_None None;
    AzOptionWaylandThemeVariant_Some Some;
};
typedef union AzOptionWaylandTheme AzOptionWaylandTheme;

enum AzOptionInstantTag {
   AzOptionInstantTag_None,
   AzOptionInstantTag_Some,
};
typedef enum AzOptionInstantTag AzOptionInstantTag;

struct AzOptionInstantVariant_None { AzOptionInstantTag tag; };
typedef struct AzOptionInstantVariant_None AzOptionInstantVariant_None;

struct AzOptionInstantVariant_Some { AzOptionInstantTag tag; AzInstant payload; };
typedef struct AzOptionInstantVariant_Some AzOptionInstantVariant_Some;


union AzOptionInstant {
    AzOptionInstantVariant_None None;
    AzOptionInstantVariant_Some Some;
};
typedef union AzOptionInstant AzOptionInstant;

enum AzXmlStreamErrorTag {
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
};
typedef enum AzXmlStreamErrorTag AzXmlStreamErrorTag;

struct AzXmlStreamErrorVariant_UnexpectedEndOfStream { AzXmlStreamErrorTag tag; };
typedef struct AzXmlStreamErrorVariant_UnexpectedEndOfStream AzXmlStreamErrorVariant_UnexpectedEndOfStream;

struct AzXmlStreamErrorVariant_InvalidName { AzXmlStreamErrorTag tag; };
typedef struct AzXmlStreamErrorVariant_InvalidName AzXmlStreamErrorVariant_InvalidName;

struct AzXmlStreamErrorVariant_NonXmlChar { AzXmlStreamErrorTag tag; AzNonXmlCharError payload; };
typedef struct AzXmlStreamErrorVariant_NonXmlChar AzXmlStreamErrorVariant_NonXmlChar;

struct AzXmlStreamErrorVariant_InvalidChar { AzXmlStreamErrorTag tag; AzInvalidCharError payload; };
typedef struct AzXmlStreamErrorVariant_InvalidChar AzXmlStreamErrorVariant_InvalidChar;

struct AzXmlStreamErrorVariant_InvalidCharMultiple { AzXmlStreamErrorTag tag; AzInvalidCharMultipleError payload; };
typedef struct AzXmlStreamErrorVariant_InvalidCharMultiple AzXmlStreamErrorVariant_InvalidCharMultiple;

struct AzXmlStreamErrorVariant_InvalidQuote { AzXmlStreamErrorTag tag; AzInvalidQuoteError payload; };
typedef struct AzXmlStreamErrorVariant_InvalidQuote AzXmlStreamErrorVariant_InvalidQuote;

struct AzXmlStreamErrorVariant_InvalidSpace { AzXmlStreamErrorTag tag; AzInvalidSpaceError payload; };
typedef struct AzXmlStreamErrorVariant_InvalidSpace AzXmlStreamErrorVariant_InvalidSpace;

struct AzXmlStreamErrorVariant_InvalidString { AzXmlStreamErrorTag tag; AzInvalidStringError payload; };
typedef struct AzXmlStreamErrorVariant_InvalidString AzXmlStreamErrorVariant_InvalidString;

struct AzXmlStreamErrorVariant_InvalidReference { AzXmlStreamErrorTag tag; };
typedef struct AzXmlStreamErrorVariant_InvalidReference AzXmlStreamErrorVariant_InvalidReference;

struct AzXmlStreamErrorVariant_InvalidExternalID { AzXmlStreamErrorTag tag; };
typedef struct AzXmlStreamErrorVariant_InvalidExternalID AzXmlStreamErrorVariant_InvalidExternalID;

struct AzXmlStreamErrorVariant_InvalidCommentData { AzXmlStreamErrorTag tag; };
typedef struct AzXmlStreamErrorVariant_InvalidCommentData AzXmlStreamErrorVariant_InvalidCommentData;

struct AzXmlStreamErrorVariant_InvalidCommentEnd { AzXmlStreamErrorTag tag; };
typedef struct AzXmlStreamErrorVariant_InvalidCommentEnd AzXmlStreamErrorVariant_InvalidCommentEnd;

struct AzXmlStreamErrorVariant_InvalidCharacterData { AzXmlStreamErrorTag tag; };
typedef struct AzXmlStreamErrorVariant_InvalidCharacterData AzXmlStreamErrorVariant_InvalidCharacterData;


union AzXmlStreamError {
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
};
typedef union AzXmlStreamError AzXmlStreamError;

struct AzLinuxWindowOptions {
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
};
typedef struct AzLinuxWindowOptions AzLinuxWindowOptions;

struct AzCssPath {
    AzCssPathSelectorVec selectors;
};
typedef struct AzCssPath AzCssPath;

enum AzStyleBackgroundContentVecValueTag {
   AzStyleBackgroundContentVecValueTag_Auto,
   AzStyleBackgroundContentVecValueTag_None,
   AzStyleBackgroundContentVecValueTag_Inherit,
   AzStyleBackgroundContentVecValueTag_Initial,
   AzStyleBackgroundContentVecValueTag_Exact,
};
typedef enum AzStyleBackgroundContentVecValueTag AzStyleBackgroundContentVecValueTag;

struct AzStyleBackgroundContentVecValueVariant_Auto { AzStyleBackgroundContentVecValueTag tag; };
typedef struct AzStyleBackgroundContentVecValueVariant_Auto AzStyleBackgroundContentVecValueVariant_Auto;

struct AzStyleBackgroundContentVecValueVariant_None { AzStyleBackgroundContentVecValueTag tag; };
typedef struct AzStyleBackgroundContentVecValueVariant_None AzStyleBackgroundContentVecValueVariant_None;

struct AzStyleBackgroundContentVecValueVariant_Inherit { AzStyleBackgroundContentVecValueTag tag; };
typedef struct AzStyleBackgroundContentVecValueVariant_Inherit AzStyleBackgroundContentVecValueVariant_Inherit;

struct AzStyleBackgroundContentVecValueVariant_Initial { AzStyleBackgroundContentVecValueTag tag; };
typedef struct AzStyleBackgroundContentVecValueVariant_Initial AzStyleBackgroundContentVecValueVariant_Initial;

struct AzStyleBackgroundContentVecValueVariant_Exact { AzStyleBackgroundContentVecValueTag tag; AzStyleBackgroundContentVec payload; };
typedef struct AzStyleBackgroundContentVecValueVariant_Exact AzStyleBackgroundContentVecValueVariant_Exact;


union AzStyleBackgroundContentVecValue {
    AzStyleBackgroundContentVecValueVariant_Auto Auto;
    AzStyleBackgroundContentVecValueVariant_None None;
    AzStyleBackgroundContentVecValueVariant_Inherit Inherit;
    AzStyleBackgroundContentVecValueVariant_Initial Initial;
    AzStyleBackgroundContentVecValueVariant_Exact Exact;
};
typedef union AzStyleBackgroundContentVecValue AzStyleBackgroundContentVecValue;

enum AzCssPropertyTag {
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
};
typedef enum AzCssPropertyTag AzCssPropertyTag;

struct AzCssPropertyVariant_TextColor { AzCssPropertyTag tag; AzStyleTextColorValue payload; };
typedef struct AzCssPropertyVariant_TextColor AzCssPropertyVariant_TextColor;

struct AzCssPropertyVariant_FontSize { AzCssPropertyTag tag; AzStyleFontSizeValue payload; };
typedef struct AzCssPropertyVariant_FontSize AzCssPropertyVariant_FontSize;

struct AzCssPropertyVariant_FontFamily { AzCssPropertyTag tag; AzStyleFontFamilyValue payload; };
typedef struct AzCssPropertyVariant_FontFamily AzCssPropertyVariant_FontFamily;

struct AzCssPropertyVariant_TextAlign { AzCssPropertyTag tag; AzStyleTextAlignmentHorzValue payload; };
typedef struct AzCssPropertyVariant_TextAlign AzCssPropertyVariant_TextAlign;

struct AzCssPropertyVariant_LetterSpacing { AzCssPropertyTag tag; AzStyleLetterSpacingValue payload; };
typedef struct AzCssPropertyVariant_LetterSpacing AzCssPropertyVariant_LetterSpacing;

struct AzCssPropertyVariant_LineHeight { AzCssPropertyTag tag; AzStyleLineHeightValue payload; };
typedef struct AzCssPropertyVariant_LineHeight AzCssPropertyVariant_LineHeight;

struct AzCssPropertyVariant_WordSpacing { AzCssPropertyTag tag; AzStyleWordSpacingValue payload; };
typedef struct AzCssPropertyVariant_WordSpacing AzCssPropertyVariant_WordSpacing;

struct AzCssPropertyVariant_TabWidth { AzCssPropertyTag tag; AzStyleTabWidthValue payload; };
typedef struct AzCssPropertyVariant_TabWidth AzCssPropertyVariant_TabWidth;

struct AzCssPropertyVariant_Cursor { AzCssPropertyTag tag; AzStyleCursorValue payload; };
typedef struct AzCssPropertyVariant_Cursor AzCssPropertyVariant_Cursor;

struct AzCssPropertyVariant_Display { AzCssPropertyTag tag; AzLayoutDisplayValue payload; };
typedef struct AzCssPropertyVariant_Display AzCssPropertyVariant_Display;

struct AzCssPropertyVariant_Float { AzCssPropertyTag tag; AzLayoutFloatValue payload; };
typedef struct AzCssPropertyVariant_Float AzCssPropertyVariant_Float;

struct AzCssPropertyVariant_BoxSizing { AzCssPropertyTag tag; AzLayoutBoxSizingValue payload; };
typedef struct AzCssPropertyVariant_BoxSizing AzCssPropertyVariant_BoxSizing;

struct AzCssPropertyVariant_Width { AzCssPropertyTag tag; AzLayoutWidthValue payload; };
typedef struct AzCssPropertyVariant_Width AzCssPropertyVariant_Width;

struct AzCssPropertyVariant_Height { AzCssPropertyTag tag; AzLayoutHeightValue payload; };
typedef struct AzCssPropertyVariant_Height AzCssPropertyVariant_Height;

struct AzCssPropertyVariant_MinWidth { AzCssPropertyTag tag; AzLayoutMinWidthValue payload; };
typedef struct AzCssPropertyVariant_MinWidth AzCssPropertyVariant_MinWidth;

struct AzCssPropertyVariant_MinHeight { AzCssPropertyTag tag; AzLayoutMinHeightValue payload; };
typedef struct AzCssPropertyVariant_MinHeight AzCssPropertyVariant_MinHeight;

struct AzCssPropertyVariant_MaxWidth { AzCssPropertyTag tag; AzLayoutMaxWidthValue payload; };
typedef struct AzCssPropertyVariant_MaxWidth AzCssPropertyVariant_MaxWidth;

struct AzCssPropertyVariant_MaxHeight { AzCssPropertyTag tag; AzLayoutMaxHeightValue payload; };
typedef struct AzCssPropertyVariant_MaxHeight AzCssPropertyVariant_MaxHeight;

struct AzCssPropertyVariant_Position { AzCssPropertyTag tag; AzLayoutPositionValue payload; };
typedef struct AzCssPropertyVariant_Position AzCssPropertyVariant_Position;

struct AzCssPropertyVariant_Top { AzCssPropertyTag tag; AzLayoutTopValue payload; };
typedef struct AzCssPropertyVariant_Top AzCssPropertyVariant_Top;

struct AzCssPropertyVariant_Right { AzCssPropertyTag tag; AzLayoutRightValue payload; };
typedef struct AzCssPropertyVariant_Right AzCssPropertyVariant_Right;

struct AzCssPropertyVariant_Left { AzCssPropertyTag tag; AzLayoutLeftValue payload; };
typedef struct AzCssPropertyVariant_Left AzCssPropertyVariant_Left;

struct AzCssPropertyVariant_Bottom { AzCssPropertyTag tag; AzLayoutBottomValue payload; };
typedef struct AzCssPropertyVariant_Bottom AzCssPropertyVariant_Bottom;

struct AzCssPropertyVariant_FlexWrap { AzCssPropertyTag tag; AzLayoutFlexWrapValue payload; };
typedef struct AzCssPropertyVariant_FlexWrap AzCssPropertyVariant_FlexWrap;

struct AzCssPropertyVariant_FlexDirection { AzCssPropertyTag tag; AzLayoutFlexDirectionValue payload; };
typedef struct AzCssPropertyVariant_FlexDirection AzCssPropertyVariant_FlexDirection;

struct AzCssPropertyVariant_FlexGrow { AzCssPropertyTag tag; AzLayoutFlexGrowValue payload; };
typedef struct AzCssPropertyVariant_FlexGrow AzCssPropertyVariant_FlexGrow;

struct AzCssPropertyVariant_FlexShrink { AzCssPropertyTag tag; AzLayoutFlexShrinkValue payload; };
typedef struct AzCssPropertyVariant_FlexShrink AzCssPropertyVariant_FlexShrink;

struct AzCssPropertyVariant_JustifyContent { AzCssPropertyTag tag; AzLayoutJustifyContentValue payload; };
typedef struct AzCssPropertyVariant_JustifyContent AzCssPropertyVariant_JustifyContent;

struct AzCssPropertyVariant_AlignItems { AzCssPropertyTag tag; AzLayoutAlignItemsValue payload; };
typedef struct AzCssPropertyVariant_AlignItems AzCssPropertyVariant_AlignItems;

struct AzCssPropertyVariant_AlignContent { AzCssPropertyTag tag; AzLayoutAlignContentValue payload; };
typedef struct AzCssPropertyVariant_AlignContent AzCssPropertyVariant_AlignContent;

struct AzCssPropertyVariant_BackgroundContent { AzCssPropertyTag tag; AzStyleBackgroundContentVecValue payload; };
typedef struct AzCssPropertyVariant_BackgroundContent AzCssPropertyVariant_BackgroundContent;

struct AzCssPropertyVariant_BackgroundPosition { AzCssPropertyTag tag; AzStyleBackgroundPositionVecValue payload; };
typedef struct AzCssPropertyVariant_BackgroundPosition AzCssPropertyVariant_BackgroundPosition;

struct AzCssPropertyVariant_BackgroundSize { AzCssPropertyTag tag; AzStyleBackgroundSizeVecValue payload; };
typedef struct AzCssPropertyVariant_BackgroundSize AzCssPropertyVariant_BackgroundSize;

struct AzCssPropertyVariant_BackgroundRepeat { AzCssPropertyTag tag; AzStyleBackgroundRepeatVecValue payload; };
typedef struct AzCssPropertyVariant_BackgroundRepeat AzCssPropertyVariant_BackgroundRepeat;

struct AzCssPropertyVariant_OverflowX { AzCssPropertyTag tag; AzLayoutOverflowValue payload; };
typedef struct AzCssPropertyVariant_OverflowX AzCssPropertyVariant_OverflowX;

struct AzCssPropertyVariant_OverflowY { AzCssPropertyTag tag; AzLayoutOverflowValue payload; };
typedef struct AzCssPropertyVariant_OverflowY AzCssPropertyVariant_OverflowY;

struct AzCssPropertyVariant_PaddingTop { AzCssPropertyTag tag; AzLayoutPaddingTopValue payload; };
typedef struct AzCssPropertyVariant_PaddingTop AzCssPropertyVariant_PaddingTop;

struct AzCssPropertyVariant_PaddingLeft { AzCssPropertyTag tag; AzLayoutPaddingLeftValue payload; };
typedef struct AzCssPropertyVariant_PaddingLeft AzCssPropertyVariant_PaddingLeft;

struct AzCssPropertyVariant_PaddingRight { AzCssPropertyTag tag; AzLayoutPaddingRightValue payload; };
typedef struct AzCssPropertyVariant_PaddingRight AzCssPropertyVariant_PaddingRight;

struct AzCssPropertyVariant_PaddingBottom { AzCssPropertyTag tag; AzLayoutPaddingBottomValue payload; };
typedef struct AzCssPropertyVariant_PaddingBottom AzCssPropertyVariant_PaddingBottom;

struct AzCssPropertyVariant_MarginTop { AzCssPropertyTag tag; AzLayoutMarginTopValue payload; };
typedef struct AzCssPropertyVariant_MarginTop AzCssPropertyVariant_MarginTop;

struct AzCssPropertyVariant_MarginLeft { AzCssPropertyTag tag; AzLayoutMarginLeftValue payload; };
typedef struct AzCssPropertyVariant_MarginLeft AzCssPropertyVariant_MarginLeft;

struct AzCssPropertyVariant_MarginRight { AzCssPropertyTag tag; AzLayoutMarginRightValue payload; };
typedef struct AzCssPropertyVariant_MarginRight AzCssPropertyVariant_MarginRight;

struct AzCssPropertyVariant_MarginBottom { AzCssPropertyTag tag; AzLayoutMarginBottomValue payload; };
typedef struct AzCssPropertyVariant_MarginBottom AzCssPropertyVariant_MarginBottom;

struct AzCssPropertyVariant_BorderTopLeftRadius { AzCssPropertyTag tag; AzStyleBorderTopLeftRadiusValue payload; };
typedef struct AzCssPropertyVariant_BorderTopLeftRadius AzCssPropertyVariant_BorderTopLeftRadius;

struct AzCssPropertyVariant_BorderTopRightRadius { AzCssPropertyTag tag; AzStyleBorderTopRightRadiusValue payload; };
typedef struct AzCssPropertyVariant_BorderTopRightRadius AzCssPropertyVariant_BorderTopRightRadius;

struct AzCssPropertyVariant_BorderBottomLeftRadius { AzCssPropertyTag tag; AzStyleBorderBottomLeftRadiusValue payload; };
typedef struct AzCssPropertyVariant_BorderBottomLeftRadius AzCssPropertyVariant_BorderBottomLeftRadius;

struct AzCssPropertyVariant_BorderBottomRightRadius { AzCssPropertyTag tag; AzStyleBorderBottomRightRadiusValue payload; };
typedef struct AzCssPropertyVariant_BorderBottomRightRadius AzCssPropertyVariant_BorderBottomRightRadius;

struct AzCssPropertyVariant_BorderTopColor { AzCssPropertyTag tag; AzStyleBorderTopColorValue payload; };
typedef struct AzCssPropertyVariant_BorderTopColor AzCssPropertyVariant_BorderTopColor;

struct AzCssPropertyVariant_BorderRightColor { AzCssPropertyTag tag; AzStyleBorderRightColorValue payload; };
typedef struct AzCssPropertyVariant_BorderRightColor AzCssPropertyVariant_BorderRightColor;

struct AzCssPropertyVariant_BorderLeftColor { AzCssPropertyTag tag; AzStyleBorderLeftColorValue payload; };
typedef struct AzCssPropertyVariant_BorderLeftColor AzCssPropertyVariant_BorderLeftColor;

struct AzCssPropertyVariant_BorderBottomColor { AzCssPropertyTag tag; AzStyleBorderBottomColorValue payload; };
typedef struct AzCssPropertyVariant_BorderBottomColor AzCssPropertyVariant_BorderBottomColor;

struct AzCssPropertyVariant_BorderTopStyle { AzCssPropertyTag tag; AzStyleBorderTopStyleValue payload; };
typedef struct AzCssPropertyVariant_BorderTopStyle AzCssPropertyVariant_BorderTopStyle;

struct AzCssPropertyVariant_BorderRightStyle { AzCssPropertyTag tag; AzStyleBorderRightStyleValue payload; };
typedef struct AzCssPropertyVariant_BorderRightStyle AzCssPropertyVariant_BorderRightStyle;

struct AzCssPropertyVariant_BorderLeftStyle { AzCssPropertyTag tag; AzStyleBorderLeftStyleValue payload; };
typedef struct AzCssPropertyVariant_BorderLeftStyle AzCssPropertyVariant_BorderLeftStyle;

struct AzCssPropertyVariant_BorderBottomStyle { AzCssPropertyTag tag; AzStyleBorderBottomStyleValue payload; };
typedef struct AzCssPropertyVariant_BorderBottomStyle AzCssPropertyVariant_BorderBottomStyle;

struct AzCssPropertyVariant_BorderTopWidth { AzCssPropertyTag tag; AzLayoutBorderTopWidthValue payload; };
typedef struct AzCssPropertyVariant_BorderTopWidth AzCssPropertyVariant_BorderTopWidth;

struct AzCssPropertyVariant_BorderRightWidth { AzCssPropertyTag tag; AzLayoutBorderRightWidthValue payload; };
typedef struct AzCssPropertyVariant_BorderRightWidth AzCssPropertyVariant_BorderRightWidth;

struct AzCssPropertyVariant_BorderLeftWidth { AzCssPropertyTag tag; AzLayoutBorderLeftWidthValue payload; };
typedef struct AzCssPropertyVariant_BorderLeftWidth AzCssPropertyVariant_BorderLeftWidth;

struct AzCssPropertyVariant_BorderBottomWidth { AzCssPropertyTag tag; AzLayoutBorderBottomWidthValue payload; };
typedef struct AzCssPropertyVariant_BorderBottomWidth AzCssPropertyVariant_BorderBottomWidth;

struct AzCssPropertyVariant_BoxShadowLeft { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; };
typedef struct AzCssPropertyVariant_BoxShadowLeft AzCssPropertyVariant_BoxShadowLeft;

struct AzCssPropertyVariant_BoxShadowRight { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; };
typedef struct AzCssPropertyVariant_BoxShadowRight AzCssPropertyVariant_BoxShadowRight;

struct AzCssPropertyVariant_BoxShadowTop { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; };
typedef struct AzCssPropertyVariant_BoxShadowTop AzCssPropertyVariant_BoxShadowTop;

struct AzCssPropertyVariant_BoxShadowBottom { AzCssPropertyTag tag; AzStyleBoxShadowValue payload; };
typedef struct AzCssPropertyVariant_BoxShadowBottom AzCssPropertyVariant_BoxShadowBottom;

struct AzCssPropertyVariant_ScrollbarStyle { AzCssPropertyTag tag; AzScrollbarStyleValue payload; };
typedef struct AzCssPropertyVariant_ScrollbarStyle AzCssPropertyVariant_ScrollbarStyle;

struct AzCssPropertyVariant_Opacity { AzCssPropertyTag tag; AzStyleOpacityValue payload; };
typedef struct AzCssPropertyVariant_Opacity AzCssPropertyVariant_Opacity;

struct AzCssPropertyVariant_Transform { AzCssPropertyTag tag; AzStyleTransformVecValue payload; };
typedef struct AzCssPropertyVariant_Transform AzCssPropertyVariant_Transform;

struct AzCssPropertyVariant_TransformOrigin { AzCssPropertyTag tag; AzStyleTransformOriginValue payload; };
typedef struct AzCssPropertyVariant_TransformOrigin AzCssPropertyVariant_TransformOrigin;

struct AzCssPropertyVariant_PerspectiveOrigin { AzCssPropertyTag tag; AzStylePerspectiveOriginValue payload; };
typedef struct AzCssPropertyVariant_PerspectiveOrigin AzCssPropertyVariant_PerspectiveOrigin;

struct AzCssPropertyVariant_BackfaceVisibility { AzCssPropertyTag tag; AzStyleBackfaceVisibilityValue payload; };
typedef struct AzCssPropertyVariant_BackfaceVisibility AzCssPropertyVariant_BackfaceVisibility;


union AzCssProperty {
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
};
typedef union AzCssProperty AzCssProperty;

enum AzCssPropertySourceTag {
   AzCssPropertySourceTag_Css,
   AzCssPropertySourceTag_Inline,
};
typedef enum AzCssPropertySourceTag AzCssPropertySourceTag;

struct AzCssPropertySourceVariant_Css { AzCssPropertySourceTag tag; AzCssPath payload; };
typedef struct AzCssPropertySourceVariant_Css AzCssPropertySourceVariant_Css;

struct AzCssPropertySourceVariant_Inline { AzCssPropertySourceTag tag; };
typedef struct AzCssPropertySourceVariant_Inline AzCssPropertySourceVariant_Inline;


union AzCssPropertySource {
    AzCssPropertySourceVariant_Css Css;
    AzCssPropertySourceVariant_Inline Inline;
};
typedef union AzCssPropertySource AzCssPropertySource;

struct AzVertexLayout {
    AzVertexAttributeVec fields;
};
typedef struct AzVertexLayout AzVertexLayout;

struct AzVertexArrayObject {
    AzVertexLayout vertex_layout;
    uint32_t vao_id;
    AzGlContextPtr gl_context;
};
typedef struct AzVertexArrayObject AzVertexArrayObject;

struct AzVertexBuffer {
    uint32_t vertex_buffer_id;
    size_t vertex_buffer_len;
    AzVertexArrayObject vao;
    uint32_t index_buffer_id;
    size_t index_buffer_len;
    AzIndexBufferFormat index_buffer_format;
};
typedef struct AzVertexBuffer AzVertexBuffer;

enum AzFontSourceTag {
   AzFontSourceTag_Embedded,
   AzFontSourceTag_File,
   AzFontSourceTag_System,
};
typedef enum AzFontSourceTag AzFontSourceTag;

struct AzFontSourceVariant_Embedded { AzFontSourceTag tag; AzEmbeddedFontSource payload; };
typedef struct AzFontSourceVariant_Embedded AzFontSourceVariant_Embedded;

struct AzFontSourceVariant_File { AzFontSourceTag tag; AzFileFontSource payload; };
typedef struct AzFontSourceVariant_File AzFontSourceVariant_File;

struct AzFontSourceVariant_System { AzFontSourceTag tag; AzSystemFontSource payload; };
typedef struct AzFontSourceVariant_System AzFontSourceVariant_System;


union AzFontSource {
    AzFontSourceVariant_Embedded Embedded;
    AzFontSourceVariant_File File;
    AzFontSourceVariant_System System;
};
typedef union AzFontSource AzFontSource;

struct AzSvgMultiPolygon {
    AzSvgPathVec rings;
};
typedef struct AzSvgMultiPolygon AzSvgMultiPolygon;

struct AzTimer {
    AzRefAny data;
    AzInstant created;
    AzOptionInstant last_run;
    size_t run_count;
    AzOptionDuration delay;
    AzOptionDuration interval;
    AzOptionDuration timeout;
    AzTimerCallback callback;
};
typedef struct AzTimer AzTimer;

enum AzThreadReceiveMsgTag {
   AzThreadReceiveMsgTag_WriteBack,
   AzThreadReceiveMsgTag_Update,
};
typedef enum AzThreadReceiveMsgTag AzThreadReceiveMsgTag;

struct AzThreadReceiveMsgVariant_WriteBack { AzThreadReceiveMsgTag tag; AzThreadWriteBackMsg payload; };
typedef struct AzThreadReceiveMsgVariant_WriteBack AzThreadReceiveMsgVariant_WriteBack;

struct AzThreadReceiveMsgVariant_Update { AzThreadReceiveMsgTag tag; AzUpdateScreen payload; };
typedef struct AzThreadReceiveMsgVariant_Update AzThreadReceiveMsgVariant_Update;


union AzThreadReceiveMsg {
    AzThreadReceiveMsgVariant_WriteBack WriteBack;
    AzThreadReceiveMsgVariant_Update Update;
};
typedef union AzThreadReceiveMsg AzThreadReceiveMsg;

struct AzCssPropertyVec {
    AzCssProperty* const ptr;
    size_t len;
    size_t cap;
    AzCssPropertyVecDestructor destructor;
};
typedef struct AzCssPropertyVec AzCssPropertyVec;

struct AzSvgMultiPolygonVec {
    AzSvgMultiPolygon* const ptr;
    size_t len;
    size_t cap;
    AzSvgMultiPolygonVecDestructor destructor;
};
typedef struct AzSvgMultiPolygonVec AzSvgMultiPolygonVec;

enum AzOptionThreadReceiveMsgTag {
   AzOptionThreadReceiveMsgTag_None,
   AzOptionThreadReceiveMsgTag_Some,
};
typedef enum AzOptionThreadReceiveMsgTag AzOptionThreadReceiveMsgTag;

struct AzOptionThreadReceiveMsgVariant_None { AzOptionThreadReceiveMsgTag tag; };
typedef struct AzOptionThreadReceiveMsgVariant_None AzOptionThreadReceiveMsgVariant_None;

struct AzOptionThreadReceiveMsgVariant_Some { AzOptionThreadReceiveMsgTag tag; AzThreadReceiveMsg payload; };
typedef struct AzOptionThreadReceiveMsgVariant_Some AzOptionThreadReceiveMsgVariant_Some;


union AzOptionThreadReceiveMsg {
    AzOptionThreadReceiveMsgVariant_None None;
    AzOptionThreadReceiveMsgVariant_Some Some;
};
typedef union AzOptionThreadReceiveMsg AzOptionThreadReceiveMsg;

struct AzXmlTextError {
    AzXmlStreamError stream_error;
    AzSvgParseErrorPosition pos;
};
typedef struct AzXmlTextError AzXmlTextError;

struct AzPlatformSpecificOptions {
    AzWindowsWindowOptions windows_options;
    AzLinuxWindowOptions linux_options;
    AzMacWindowOptions mac_options;
    AzWasmWindowOptions wasm_options;
};
typedef struct AzPlatformSpecificOptions AzPlatformSpecificOptions;

struct AzWindowState {
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
};
typedef struct AzWindowState AzWindowState;

struct AzCallbackInfo {
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
};
typedef struct AzCallbackInfo AzCallbackInfo;

struct AzFocusTargetPath {
    AzDomId dom;
    AzCssPath css_path;
};
typedef struct AzFocusTargetPath AzFocusTargetPath;

struct AzTimerCallbackInfo {
    AzCallbackInfo callback_info;
    AzInstant frame_start;
    size_t call_count;
    bool  is_about_to_finish;
};
typedef struct AzTimerCallbackInfo AzTimerCallbackInfo;

enum AzNodeDataInlineCssPropertyTag {
   AzNodeDataInlineCssPropertyTag_Normal,
   AzNodeDataInlineCssPropertyTag_Active,
   AzNodeDataInlineCssPropertyTag_Focus,
   AzNodeDataInlineCssPropertyTag_Hover,
};
typedef enum AzNodeDataInlineCssPropertyTag AzNodeDataInlineCssPropertyTag;

struct AzNodeDataInlineCssPropertyVariant_Normal { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; };
typedef struct AzNodeDataInlineCssPropertyVariant_Normal AzNodeDataInlineCssPropertyVariant_Normal;

struct AzNodeDataInlineCssPropertyVariant_Active { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; };
typedef struct AzNodeDataInlineCssPropertyVariant_Active AzNodeDataInlineCssPropertyVariant_Active;

struct AzNodeDataInlineCssPropertyVariant_Focus { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; };
typedef struct AzNodeDataInlineCssPropertyVariant_Focus AzNodeDataInlineCssPropertyVariant_Focus;

struct AzNodeDataInlineCssPropertyVariant_Hover { AzNodeDataInlineCssPropertyTag tag; AzCssProperty payload; };
typedef struct AzNodeDataInlineCssPropertyVariant_Hover AzNodeDataInlineCssPropertyVariant_Hover;


union AzNodeDataInlineCssProperty {
    AzNodeDataInlineCssPropertyVariant_Normal Normal;
    AzNodeDataInlineCssPropertyVariant_Active Active;
    AzNodeDataInlineCssPropertyVariant_Focus Focus;
    AzNodeDataInlineCssPropertyVariant_Hover Hover;
};
typedef union AzNodeDataInlineCssProperty AzNodeDataInlineCssProperty;

struct AzDynamicCssProperty {
    AzString dynamic_id;
    AzCssProperty default_value;
};
typedef struct AzDynamicCssProperty AzDynamicCssProperty;

enum AzSvgNodeTag {
   AzSvgNodeTag_MultiPolygonCollection,
   AzSvgNodeTag_MultiPolygon,
   AzSvgNodeTag_Path,
   AzSvgNodeTag_Circle,
   AzSvgNodeTag_Rect,
};
typedef enum AzSvgNodeTag AzSvgNodeTag;

struct AzSvgNodeVariant_MultiPolygonCollection { AzSvgNodeTag tag; AzSvgMultiPolygonVec payload; };
typedef struct AzSvgNodeVariant_MultiPolygonCollection AzSvgNodeVariant_MultiPolygonCollection;

struct AzSvgNodeVariant_MultiPolygon { AzSvgNodeTag tag; AzSvgMultiPolygon payload; };
typedef struct AzSvgNodeVariant_MultiPolygon AzSvgNodeVariant_MultiPolygon;

struct AzSvgNodeVariant_Path { AzSvgNodeTag tag; AzSvgPath payload; };
typedef struct AzSvgNodeVariant_Path AzSvgNodeVariant_Path;

struct AzSvgNodeVariant_Circle { AzSvgNodeTag tag; AzSvgCircle payload; };
typedef struct AzSvgNodeVariant_Circle AzSvgNodeVariant_Circle;

struct AzSvgNodeVariant_Rect { AzSvgNodeTag tag; AzSvgRect payload; };
typedef struct AzSvgNodeVariant_Rect AzSvgNodeVariant_Rect;


union AzSvgNode {
    AzSvgNodeVariant_MultiPolygonCollection MultiPolygonCollection;
    AzSvgNodeVariant_MultiPolygon MultiPolygon;
    AzSvgNodeVariant_Path Path;
    AzSvgNodeVariant_Circle Circle;
    AzSvgNodeVariant_Rect Rect;
};
typedef union AzSvgNode AzSvgNode;

struct AzSvgStyledNode {
    AzSvgNode geometry;
    AzSvgStyle style;
};
typedef struct AzSvgStyledNode AzSvgStyledNode;

struct AzNodeDataInlineCssPropertyVec {
    AzNodeDataInlineCssProperty* const ptr;
    size_t len;
    size_t cap;
    AzNodeDataInlineCssPropertyVecDestructor destructor;
};
typedef struct AzNodeDataInlineCssPropertyVec AzNodeDataInlineCssPropertyVec;

enum AzXmlParseErrorTag {
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
};
typedef enum AzXmlParseErrorTag AzXmlParseErrorTag;

struct AzXmlParseErrorVariant_InvalidDeclaration { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidDeclaration AzXmlParseErrorVariant_InvalidDeclaration;

struct AzXmlParseErrorVariant_InvalidComment { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidComment AzXmlParseErrorVariant_InvalidComment;

struct AzXmlParseErrorVariant_InvalidPI { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidPI AzXmlParseErrorVariant_InvalidPI;

struct AzXmlParseErrorVariant_InvalidDoctype { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidDoctype AzXmlParseErrorVariant_InvalidDoctype;

struct AzXmlParseErrorVariant_InvalidEntity { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidEntity AzXmlParseErrorVariant_InvalidEntity;

struct AzXmlParseErrorVariant_InvalidElement { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidElement AzXmlParseErrorVariant_InvalidElement;

struct AzXmlParseErrorVariant_InvalidAttribute { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidAttribute AzXmlParseErrorVariant_InvalidAttribute;

struct AzXmlParseErrorVariant_InvalidCdata { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidCdata AzXmlParseErrorVariant_InvalidCdata;

struct AzXmlParseErrorVariant_InvalidCharData { AzXmlParseErrorTag tag; AzXmlTextError payload; };
typedef struct AzXmlParseErrorVariant_InvalidCharData AzXmlParseErrorVariant_InvalidCharData;

struct AzXmlParseErrorVariant_UnknownToken { AzXmlParseErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlParseErrorVariant_UnknownToken AzXmlParseErrorVariant_UnknownToken;


union AzXmlParseError {
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
};
typedef union AzXmlParseError AzXmlParseError;

struct AzWindowCreateOptions {
    AzWindowState state;
    AzOptionRendererOptions renderer_type;
    AzOptionWindowTheme theme;
    AzOptionCallback create_callback;
};
typedef struct AzWindowCreateOptions AzWindowCreateOptions;

enum AzFocusTargetTag {
   AzFocusTargetTag_Id,
   AzFocusTargetTag_Path,
   AzFocusTargetTag_Previous,
   AzFocusTargetTag_Next,
   AzFocusTargetTag_First,
   AzFocusTargetTag_Last,
   AzFocusTargetTag_NoFocus,
};
typedef enum AzFocusTargetTag AzFocusTargetTag;

struct AzFocusTargetVariant_Id { AzFocusTargetTag tag; AzDomNodeId payload; };
typedef struct AzFocusTargetVariant_Id AzFocusTargetVariant_Id;

struct AzFocusTargetVariant_Path { AzFocusTargetTag tag; AzFocusTargetPath payload; };
typedef struct AzFocusTargetVariant_Path AzFocusTargetVariant_Path;

struct AzFocusTargetVariant_Previous { AzFocusTargetTag tag; };
typedef struct AzFocusTargetVariant_Previous AzFocusTargetVariant_Previous;

struct AzFocusTargetVariant_Next { AzFocusTargetTag tag; };
typedef struct AzFocusTargetVariant_Next AzFocusTargetVariant_Next;

struct AzFocusTargetVariant_First { AzFocusTargetTag tag; };
typedef struct AzFocusTargetVariant_First AzFocusTargetVariant_First;

struct AzFocusTargetVariant_Last { AzFocusTargetTag tag; };
typedef struct AzFocusTargetVariant_Last AzFocusTargetVariant_Last;

struct AzFocusTargetVariant_NoFocus { AzFocusTargetTag tag; };
typedef struct AzFocusTargetVariant_NoFocus AzFocusTargetVariant_NoFocus;


union AzFocusTarget {
    AzFocusTargetVariant_Id Id;
    AzFocusTargetVariant_Path Path;
    AzFocusTargetVariant_Previous Previous;
    AzFocusTargetVariant_Next Next;
    AzFocusTargetVariant_First First;
    AzFocusTargetVariant_Last Last;
    AzFocusTargetVariant_NoFocus NoFocus;
};
typedef union AzFocusTarget AzFocusTarget;

struct AzNodeData {
    AzNodeType node_type;
    AzOptionRefAny dataset;
    AzIdOrClassVec ids_and_classes;
    AzCallbackDataVec callbacks;
    AzNodeDataInlineCssPropertyVec inline_css_props;
    AzOptionImageMask clip_mask;
    AzOptionTabIndex tab_index;
};
typedef struct AzNodeData AzNodeData;

enum AzCssDeclarationTag {
   AzCssDeclarationTag_Static,
   AzCssDeclarationTag_Dynamic,
};
typedef enum AzCssDeclarationTag AzCssDeclarationTag;

struct AzCssDeclarationVariant_Static { AzCssDeclarationTag tag; AzCssProperty payload; };
typedef struct AzCssDeclarationVariant_Static AzCssDeclarationVariant_Static;

struct AzCssDeclarationVariant_Dynamic { AzCssDeclarationTag tag; AzDynamicCssProperty payload; };
typedef struct AzCssDeclarationVariant_Dynamic AzCssDeclarationVariant_Dynamic;


union AzCssDeclaration {
    AzCssDeclarationVariant_Static Static;
    AzCssDeclarationVariant_Dynamic Dynamic;
};
typedef union AzCssDeclaration AzCssDeclaration;

struct AzCssDeclarationVec {
    AzCssDeclaration* const ptr;
    size_t len;
    size_t cap;
    AzCssDeclarationVecDestructor destructor;
};
typedef struct AzCssDeclarationVec AzCssDeclarationVec;

struct AzNodeDataVec {
    AzNodeData* const ptr;
    size_t len;
    size_t cap;
    AzNodeDataVecDestructor destructor;
};
typedef struct AzNodeDataVec AzNodeDataVec;

enum AzXmlErrorTag {
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
};
typedef enum AzXmlErrorTag AzXmlErrorTag;

struct AzXmlErrorVariant_InvalidXmlPrefixUri { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_InvalidXmlPrefixUri AzXmlErrorVariant_InvalidXmlPrefixUri;

struct AzXmlErrorVariant_UnexpectedXmlUri { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_UnexpectedXmlUri AzXmlErrorVariant_UnexpectedXmlUri;

struct AzXmlErrorVariant_UnexpectedXmlnsUri { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_UnexpectedXmlnsUri AzXmlErrorVariant_UnexpectedXmlnsUri;

struct AzXmlErrorVariant_InvalidElementNamePrefix { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_InvalidElementNamePrefix AzXmlErrorVariant_InvalidElementNamePrefix;

struct AzXmlErrorVariant_DuplicatedNamespace { AzXmlErrorTag tag; AzDuplicatedNamespaceError payload; };
typedef struct AzXmlErrorVariant_DuplicatedNamespace AzXmlErrorVariant_DuplicatedNamespace;

struct AzXmlErrorVariant_UnknownNamespace { AzXmlErrorTag tag; AzUnknownNamespaceError payload; };
typedef struct AzXmlErrorVariant_UnknownNamespace AzXmlErrorVariant_UnknownNamespace;

struct AzXmlErrorVariant_UnexpectedCloseTag { AzXmlErrorTag tag; AzUnexpectedCloseTagError payload; };
typedef struct AzXmlErrorVariant_UnexpectedCloseTag AzXmlErrorVariant_UnexpectedCloseTag;

struct AzXmlErrorVariant_UnexpectedEntityCloseTag { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_UnexpectedEntityCloseTag AzXmlErrorVariant_UnexpectedEntityCloseTag;

struct AzXmlErrorVariant_UnknownEntityReference { AzXmlErrorTag tag; AzUnknownEntityReferenceError payload; };
typedef struct AzXmlErrorVariant_UnknownEntityReference AzXmlErrorVariant_UnknownEntityReference;

struct AzXmlErrorVariant_MalformedEntityReference { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_MalformedEntityReference AzXmlErrorVariant_MalformedEntityReference;

struct AzXmlErrorVariant_EntityReferenceLoop { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_EntityReferenceLoop AzXmlErrorVariant_EntityReferenceLoop;

struct AzXmlErrorVariant_InvalidAttributeValue { AzXmlErrorTag tag; AzSvgParseErrorPosition payload; };
typedef struct AzXmlErrorVariant_InvalidAttributeValue AzXmlErrorVariant_InvalidAttributeValue;

struct AzXmlErrorVariant_DuplicatedAttribute { AzXmlErrorTag tag; AzDuplicatedAttributeError payload; };
typedef struct AzXmlErrorVariant_DuplicatedAttribute AzXmlErrorVariant_DuplicatedAttribute;

struct AzXmlErrorVariant_NoRootNode { AzXmlErrorTag tag; };
typedef struct AzXmlErrorVariant_NoRootNode AzXmlErrorVariant_NoRootNode;

struct AzXmlErrorVariant_SizeLimit { AzXmlErrorTag tag; };
typedef struct AzXmlErrorVariant_SizeLimit AzXmlErrorVariant_SizeLimit;

struct AzXmlErrorVariant_ParserError { AzXmlErrorTag tag; AzXmlParseError payload; };
typedef struct AzXmlErrorVariant_ParserError AzXmlErrorVariant_ParserError;


union AzXmlError {
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
};
typedef union AzXmlError AzXmlError;

struct AzDom {
    AzNodeData root;
    AzDomVec children;
    size_t estimated_total_children;
};
typedef struct AzDom AzDom;

struct AzCssRuleBlock {
    AzCssPath path;
    AzCssDeclarationVec declarations;
};
typedef struct AzCssRuleBlock AzCssRuleBlock;

struct AzStyledDom {
    AzNodeId root;
    AzNodeVec node_hierarchy;
    AzNodeDataVec node_data;
    AzStyledNodeVec styled_nodes;
    AzCascadeInfoVec cascade_info;
    AzTagIdsToNodeIdsMappingVec tag_ids_to_node_ids;
    AzParentWithNodeDepthVec non_leaf_nodes;
    AzCssPropertyCache css_property_cache;
};
typedef struct AzStyledDom AzStyledDom;

struct AzCssRuleBlockVec {
    AzCssRuleBlock* const ptr;
    size_t len;
    size_t cap;
    AzCssRuleBlockVecDestructor destructor;
};
typedef struct AzCssRuleBlockVec AzCssRuleBlockVec;

enum AzOptionDomTag {
   AzOptionDomTag_None,
   AzOptionDomTag_Some,
};
typedef enum AzOptionDomTag AzOptionDomTag;

struct AzOptionDomVariant_None { AzOptionDomTag tag; };
typedef struct AzOptionDomVariant_None AzOptionDomVariant_None;

struct AzOptionDomVariant_Some { AzOptionDomTag tag; AzDom payload; };
typedef struct AzOptionDomVariant_Some AzOptionDomVariant_Some;


union AzOptionDom {
    AzOptionDomVariant_None None;
    AzOptionDomVariant_Some Some;
};
typedef union AzOptionDom AzOptionDom;

enum AzSvgParseErrorTag {
   AzSvgParseErrorTag_InvalidFileSuffix,
   AzSvgParseErrorTag_FileOpenFailed,
   AzSvgParseErrorTag_NotAnUtf8Str,
   AzSvgParseErrorTag_MalformedGZip,
   AzSvgParseErrorTag_InvalidSize,
   AzSvgParseErrorTag_ParsingFailed,
};
typedef enum AzSvgParseErrorTag AzSvgParseErrorTag;

struct AzSvgParseErrorVariant_InvalidFileSuffix { AzSvgParseErrorTag tag; };
typedef struct AzSvgParseErrorVariant_InvalidFileSuffix AzSvgParseErrorVariant_InvalidFileSuffix;

struct AzSvgParseErrorVariant_FileOpenFailed { AzSvgParseErrorTag tag; };
typedef struct AzSvgParseErrorVariant_FileOpenFailed AzSvgParseErrorVariant_FileOpenFailed;

struct AzSvgParseErrorVariant_NotAnUtf8Str { AzSvgParseErrorTag tag; };
typedef struct AzSvgParseErrorVariant_NotAnUtf8Str AzSvgParseErrorVariant_NotAnUtf8Str;

struct AzSvgParseErrorVariant_MalformedGZip { AzSvgParseErrorTag tag; };
typedef struct AzSvgParseErrorVariant_MalformedGZip AzSvgParseErrorVariant_MalformedGZip;

struct AzSvgParseErrorVariant_InvalidSize { AzSvgParseErrorTag tag; };
typedef struct AzSvgParseErrorVariant_InvalidSize AzSvgParseErrorVariant_InvalidSize;

struct AzSvgParseErrorVariant_ParsingFailed { AzSvgParseErrorTag tag; AzXmlError payload; };
typedef struct AzSvgParseErrorVariant_ParsingFailed AzSvgParseErrorVariant_ParsingFailed;


union AzSvgParseError {
    AzSvgParseErrorVariant_InvalidFileSuffix InvalidFileSuffix;
    AzSvgParseErrorVariant_FileOpenFailed FileOpenFailed;
    AzSvgParseErrorVariant_NotAnUtf8Str NotAnUtf8Str;
    AzSvgParseErrorVariant_MalformedGZip MalformedGZip;
    AzSvgParseErrorVariant_InvalidSize InvalidSize;
    AzSvgParseErrorVariant_ParsingFailed ParsingFailed;
};
typedef union AzSvgParseError AzSvgParseError;

struct AzIFrameCallbackReturn {
    AzStyledDom dom;
    AzLayoutRect size;
    AzOptionLayoutRect virtual_size;
};
typedef struct AzIFrameCallbackReturn AzIFrameCallbackReturn;

struct AzStylesheet {
    AzCssRuleBlockVec rules;
};
typedef struct AzStylesheet AzStylesheet;

struct AzStylesheetVec {
    AzStylesheet* const ptr;
    size_t len;
    size_t cap;
    AzStylesheetVecDestructor destructor;
};
typedef struct AzStylesheetVec AzStylesheetVec;

enum AzResultSvgSvgParseErrorTag {
   AzResultSvgSvgParseErrorTag_Ok,
   AzResultSvgSvgParseErrorTag_Err,
};
typedef enum AzResultSvgSvgParseErrorTag AzResultSvgSvgParseErrorTag;

struct AzResultSvgSvgParseErrorVariant_Ok { AzResultSvgSvgParseErrorTag tag; AzSvg payload; };
typedef struct AzResultSvgSvgParseErrorVariant_Ok AzResultSvgSvgParseErrorVariant_Ok;

struct AzResultSvgSvgParseErrorVariant_Err { AzResultSvgSvgParseErrorTag tag; AzSvgParseError payload; };
typedef struct AzResultSvgSvgParseErrorVariant_Err AzResultSvgSvgParseErrorVariant_Err;


union AzResultSvgSvgParseError {
    AzResultSvgSvgParseErrorVariant_Ok Ok;
    AzResultSvgSvgParseErrorVariant_Err Err;
};
typedef union AzResultSvgSvgParseError AzResultSvgSvgParseError;

struct AzCss {
    AzStylesheetVec stylesheets;
};
typedef struct AzCss AzCss;

#endif /* AZUL_H */
