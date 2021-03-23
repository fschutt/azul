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
#endif

/* cross-platform define for __declspec(dllimport) */
#ifdef _WIN32
    #define DLLIMPORT __declspec(dllimport)
#else
    #define DLLIMPORT
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

struct AzTesselatedSvgNodeVec;
typedef struct AzTesselatedSvgNodeVec AzTesselatedSvgNodeVec;
typedef void (*AzTesselatedSvgNodeVecDestructorType)(AzTesselatedSvgNodeVec* restrict A);

struct AzXmlNodeVec;
typedef struct AzXmlNodeVec AzXmlNodeVec;
typedef void (*AzXmlNodeVecDestructorType)(AzXmlNodeVec* restrict A);

struct AzFmtArgVec;
typedef struct AzFmtArgVec AzFmtArgVec;
typedef void (*AzFmtArgVecDestructorType)(AzFmtArgVec* restrict A);

struct AzInlineLineVec;
typedef struct AzInlineLineVec AzInlineLineVec;
typedef void (*AzInlineLineVecDestructorType)(AzInlineLineVec* restrict A);

struct AzInlineWordVec;
typedef struct AzInlineWordVec AzInlineWordVec;
typedef void (*AzInlineWordVecDestructorType)(AzInlineWordVec* restrict A);

struct AzInlineGlyphVec;
typedef struct AzInlineGlyphVec AzInlineGlyphVec;
typedef void (*AzInlineGlyphVecDestructorType)(AzInlineGlyphVec* restrict A);

struct AzInlineTextHitVec;
typedef struct AzInlineTextHitVec AzInlineTextHitVec;
typedef void (*AzInlineTextHitVecDestructorType)(AzInlineTextHitVec* restrict A);

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

struct AzF32Vec;
typedef struct AzF32Vec AzF32Vec;
typedef void (*AzF32VecDestructorType)(AzF32Vec* restrict A);

struct AzU16Vec;
typedef struct AzU16Vec AzU16Vec;
typedef void (*AzU16VecDestructorType)(AzU16Vec* restrict A);

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
    void* ptr;
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

enum AzLayoutSolverVersion {
   AzLayoutSolverVersion_March2021,
};
typedef enum AzLayoutSolverVersion AzLayoutSolverVersion;

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
    bool  has_extended_window_frame;
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

struct AzLayoutInfo {
    void* window_size;
    void* theme;
    void* restrict window_size_width_stops;
    void* restrict window_size_height_stops;
    void* restrict is_theme_dependent;
    void* resources;
};
typedef struct AzLayoutInfo AzLayoutInfo;

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
#define AzTabIndex_Auto { .Auto = { .tag = AzTabIndexTag_Auto } }
#define AzTabIndex_OverrideInParent(v) { .OverrideInParent = { .tag = AzTabIndexTag_OverrideInParent, .payload = v } }
#define AzTabIndex_NoKeyboardFocus { .NoKeyboardFocus = { .tag = AzTabIndexTag_NoKeyboardFocus } }

enum AzNodeTypeKey {
   AzNodeTypeKey_Body,
   AzNodeTypeKey_Div,
   AzNodeTypeKey_Br,
   AzNodeTypeKey_P,
   AzNodeTypeKey_Img,
   AzNodeTypeKey_Texture,
   AzNodeTypeKey_IFrame,
};
typedef enum AzNodeTypeKey AzNodeTypeKey;

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
    uint8_t* ptr;
    size_t len;
};
typedef struct AzU8VecRef AzU8VecRef;

struct AzU8VecRefMut {
    uint8_t* restrict ptr;
    size_t len;
};
typedef struct AzU8VecRefMut AzU8VecRefMut;

struct AzF32VecRef {
    float* ptr;
    size_t len;
};
typedef struct AzF32VecRef AzF32VecRef;

struct AzI32VecRef {
    int32_t* ptr;
    size_t len;
};
typedef struct AzI32VecRef AzI32VecRef;

struct AzGLuintVecRef {
    uint32_t* ptr;
    size_t len;
};
typedef struct AzGLuintVecRef AzGLuintVecRef;

struct AzGLenumVecRef {
    uint32_t* ptr;
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
    uint8_t* ptr;
    size_t len;
};
typedef struct AzRefstr AzRefstr;

struct AzGLsyncPtr {
    void* ptr;
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

enum AzEncodeImageError {
   AzEncodeImageError_InsufficientMemory,
   AzEncodeImageError_DimensionError,
   AzEncodeImageError_InvalidData,
   AzEncodeImageError_Unknown,
};
typedef enum AzEncodeImageError AzEncodeImageError;

enum AzDecodeImageError {
   AzDecodeImageError_InsufficientMemory,
   AzDecodeImageError_DimensionError,
   AzDecodeImageError_UnsupportedImageFormat,
   AzDecodeImageError_Unknown,
};
typedef enum AzDecodeImageError AzDecodeImageError;

struct AzFontId {
    size_t id;
};
typedef struct AzFontId AzFontId;

struct AzSvg {
    void* restrict ptr;
};
typedef struct AzSvg AzSvg;

struct AzSvgXmlNode {
    void* restrict ptr;
};
typedef struct AzSvgXmlNode AzSvgXmlNode;

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

struct AzSvgVertex {
    float x;
    float y;
};
typedef struct AzSvgVertex AzSvgVertex;

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

enum AzIndentTag {
   AzIndentTag_None,
   AzIndentTag_Spaces,
   AzIndentTag_Tabs,
};
typedef enum AzIndentTag AzIndentTag;

struct AzIndentVariant_None { AzIndentTag tag; };
typedef struct AzIndentVariant_None AzIndentVariant_None;
struct AzIndentVariant_Spaces { AzIndentTag tag; uint8_t payload; };
typedef struct AzIndentVariant_Spaces AzIndentVariant_Spaces;
struct AzIndentVariant_Tabs { AzIndentTag tag; };
typedef struct AzIndentVariant_Tabs AzIndentVariant_Tabs;
union AzIndent {
    AzIndentVariant_None None;
    AzIndentVariant_Spaces Spaces;
    AzIndentVariant_Tabs Tabs;
};
typedef union AzIndent AzIndent;
#define AzIndent_None { .None = { .tag = AzIndentTag_None } }
#define AzIndent_Spaces(v) { .Spaces = { .tag = AzIndentTag_Spaces, .payload = v } }
#define AzIndent_Tabs { .Tabs = { .tag = AzIndentTag_Tabs } }

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
#define AzSvgFitTo_Original { .Original = { .tag = AzSvgFitToTag_Original } }
#define AzSvgFitTo_Width(v) { .Width = { .tag = AzSvgFitToTag_Width, .payload = v } }
#define AzSvgFitTo_Height(v) { .Height = { .tag = AzSvgFitToTag_Height, .payload = v } }
#define AzSvgFitTo_Zoom(v) { .Zoom = { .tag = AzSvgFitToTag_Zoom, .payload = v } }

enum AzSvgFillRule {
   AzSvgFillRule_Winding,
   AzSvgFillRule_EvenOdd,
};
typedef enum AzSvgFillRule AzSvgFillRule;

struct AzSvgTransform {
    float sx;
    float kx;
    float ky;
    float sy;
    float tx;
    float ty;
};
typedef struct AzSvgTransform AzSvgTransform;

enum AzSvgLineJoin {
   AzSvgLineJoin_Miter,
   AzSvgLineJoin_MiterClip,
   AzSvgLineJoin_Round,
   AzSvgLineJoin_Bevel,
};
typedef enum AzSvgLineJoin AzSvgLineJoin;

enum AzSvgLineCap {
   AzSvgLineCap_Butt,
   AzSvgLineCap_Square,
   AzSvgLineCap_Round,
};
typedef enum AzSvgLineCap AzSvgLineCap;

struct AzSvgDashPattern {
    float offset;
    float length_1;
    float gap_1;
    float length_2;
    float gap_2;
    float length_3;
    float gap_3;
};
typedef struct AzSvgDashPattern AzSvgDashPattern;

struct AzFile {
    void* ptr;
};
typedef struct AzFile AzFile;

struct AzMsgBox {
    void* restrict _reserved;
};
typedef struct AzMsgBox AzMsgBox;

enum AzMsgBoxIcon {
   AzMsgBoxIcon_Info,
   AzMsgBoxIcon_Warning,
   AzMsgBoxIcon_Error,
   AzMsgBoxIcon_Question,
};
typedef enum AzMsgBoxIcon AzMsgBoxIcon;

enum AzMsgBoxYesNo {
   AzMsgBoxYesNo_Yes,
   AzMsgBoxYesNo_No,
};
typedef enum AzMsgBoxYesNo AzMsgBoxYesNo;

enum AzMsgBoxOkCancel {
   AzMsgBoxOkCancel_Ok,
   AzMsgBoxOkCancel_Cancel,
};
typedef enum AzMsgBoxOkCancel AzMsgBoxOkCancel;

struct AzFileDialog {
    void* restrict _reserved;
};
typedef struct AzFileDialog AzFileDialog;

struct AzColorPickerDialog {
    void* restrict _reserved;
};
typedef struct AzColorPickerDialog AzColorPickerDialog;

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

enum AzTesselatedSvgNodeVecDestructorTag {
   AzTesselatedSvgNodeVecDestructorTag_DefaultRust,
   AzTesselatedSvgNodeVecDestructorTag_NoDestructor,
   AzTesselatedSvgNodeVecDestructorTag_External,
};
typedef enum AzTesselatedSvgNodeVecDestructorTag AzTesselatedSvgNodeVecDestructorTag;

struct AzTesselatedSvgNodeVecDestructorVariant_DefaultRust { AzTesselatedSvgNodeVecDestructorTag tag; };
typedef struct AzTesselatedSvgNodeVecDestructorVariant_DefaultRust AzTesselatedSvgNodeVecDestructorVariant_DefaultRust;
struct AzTesselatedSvgNodeVecDestructorVariant_NoDestructor { AzTesselatedSvgNodeVecDestructorTag tag; };
typedef struct AzTesselatedSvgNodeVecDestructorVariant_NoDestructor AzTesselatedSvgNodeVecDestructorVariant_NoDestructor;
struct AzTesselatedSvgNodeVecDestructorVariant_External { AzTesselatedSvgNodeVecDestructorTag tag; AzTesselatedSvgNodeVecDestructorType payload; };
typedef struct AzTesselatedSvgNodeVecDestructorVariant_External AzTesselatedSvgNodeVecDestructorVariant_External;
union AzTesselatedSvgNodeVecDestructor {
    AzTesselatedSvgNodeVecDestructorVariant_DefaultRust DefaultRust;
    AzTesselatedSvgNodeVecDestructorVariant_NoDestructor NoDestructor;
    AzTesselatedSvgNodeVecDestructorVariant_External External;
};
typedef union AzTesselatedSvgNodeVecDestructor AzTesselatedSvgNodeVecDestructor;
#define AzTesselatedSvgNodeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzTesselatedSvgNodeVecDestructorTag_DefaultRust } }
#define AzTesselatedSvgNodeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzTesselatedSvgNodeVecDestructorTag_NoDestructor } }
#define AzTesselatedSvgNodeVecDestructor_External(v) { .External = { .tag = AzTesselatedSvgNodeVecDestructorTag_External, .payload = v } }

enum AzXmlNodeVecDestructorTag {
   AzXmlNodeVecDestructorTag_DefaultRust,
   AzXmlNodeVecDestructorTag_NoDestructor,
   AzXmlNodeVecDestructorTag_External,
};
typedef enum AzXmlNodeVecDestructorTag AzXmlNodeVecDestructorTag;

struct AzXmlNodeVecDestructorVariant_DefaultRust { AzXmlNodeVecDestructorTag tag; };
typedef struct AzXmlNodeVecDestructorVariant_DefaultRust AzXmlNodeVecDestructorVariant_DefaultRust;
struct AzXmlNodeVecDestructorVariant_NoDestructor { AzXmlNodeVecDestructorTag tag; };
typedef struct AzXmlNodeVecDestructorVariant_NoDestructor AzXmlNodeVecDestructorVariant_NoDestructor;
struct AzXmlNodeVecDestructorVariant_External { AzXmlNodeVecDestructorTag tag; AzXmlNodeVecDestructorType payload; };
typedef struct AzXmlNodeVecDestructorVariant_External AzXmlNodeVecDestructorVariant_External;
union AzXmlNodeVecDestructor {
    AzXmlNodeVecDestructorVariant_DefaultRust DefaultRust;
    AzXmlNodeVecDestructorVariant_NoDestructor NoDestructor;
    AzXmlNodeVecDestructorVariant_External External;
};
typedef union AzXmlNodeVecDestructor AzXmlNodeVecDestructor;
#define AzXmlNodeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzXmlNodeVecDestructorTag_DefaultRust } }
#define AzXmlNodeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzXmlNodeVecDestructorTag_NoDestructor } }
#define AzXmlNodeVecDestructor_External(v) { .External = { .tag = AzXmlNodeVecDestructorTag_External, .payload = v } }

enum AzFmtArgVecDestructorTag {
   AzFmtArgVecDestructorTag_DefaultRust,
   AzFmtArgVecDestructorTag_NoDestructor,
   AzFmtArgVecDestructorTag_External,
};
typedef enum AzFmtArgVecDestructorTag AzFmtArgVecDestructorTag;

struct AzFmtArgVecDestructorVariant_DefaultRust { AzFmtArgVecDestructorTag tag; };
typedef struct AzFmtArgVecDestructorVariant_DefaultRust AzFmtArgVecDestructorVariant_DefaultRust;
struct AzFmtArgVecDestructorVariant_NoDestructor { AzFmtArgVecDestructorTag tag; };
typedef struct AzFmtArgVecDestructorVariant_NoDestructor AzFmtArgVecDestructorVariant_NoDestructor;
struct AzFmtArgVecDestructorVariant_External { AzFmtArgVecDestructorTag tag; AzFmtArgVecDestructorType payload; };
typedef struct AzFmtArgVecDestructorVariant_External AzFmtArgVecDestructorVariant_External;
union AzFmtArgVecDestructor {
    AzFmtArgVecDestructorVariant_DefaultRust DefaultRust;
    AzFmtArgVecDestructorVariant_NoDestructor NoDestructor;
    AzFmtArgVecDestructorVariant_External External;
};
typedef union AzFmtArgVecDestructor AzFmtArgVecDestructor;
#define AzFmtArgVecDestructor_DefaultRust { .DefaultRust = { .tag = AzFmtArgVecDestructorTag_DefaultRust } }
#define AzFmtArgVecDestructor_NoDestructor { .NoDestructor = { .tag = AzFmtArgVecDestructorTag_NoDestructor } }
#define AzFmtArgVecDestructor_External(v) { .External = { .tag = AzFmtArgVecDestructorTag_External, .payload = v } }

enum AzInlineLineVecDestructorTag {
   AzInlineLineVecDestructorTag_DefaultRust,
   AzInlineLineVecDestructorTag_NoDestructor,
   AzInlineLineVecDestructorTag_External,
};
typedef enum AzInlineLineVecDestructorTag AzInlineLineVecDestructorTag;

struct AzInlineLineVecDestructorVariant_DefaultRust { AzInlineLineVecDestructorTag tag; };
typedef struct AzInlineLineVecDestructorVariant_DefaultRust AzInlineLineVecDestructorVariant_DefaultRust;
struct AzInlineLineVecDestructorVariant_NoDestructor { AzInlineLineVecDestructorTag tag; };
typedef struct AzInlineLineVecDestructorVariant_NoDestructor AzInlineLineVecDestructorVariant_NoDestructor;
struct AzInlineLineVecDestructorVariant_External { AzInlineLineVecDestructorTag tag; AzInlineLineVecDestructorType payload; };
typedef struct AzInlineLineVecDestructorVariant_External AzInlineLineVecDestructorVariant_External;
union AzInlineLineVecDestructor {
    AzInlineLineVecDestructorVariant_DefaultRust DefaultRust;
    AzInlineLineVecDestructorVariant_NoDestructor NoDestructor;
    AzInlineLineVecDestructorVariant_External External;
};
typedef union AzInlineLineVecDestructor AzInlineLineVecDestructor;
#define AzInlineLineVecDestructor_DefaultRust { .DefaultRust = { .tag = AzInlineLineVecDestructorTag_DefaultRust } }
#define AzInlineLineVecDestructor_NoDestructor { .NoDestructor = { .tag = AzInlineLineVecDestructorTag_NoDestructor } }
#define AzInlineLineVecDestructor_External(v) { .External = { .tag = AzInlineLineVecDestructorTag_External, .payload = v } }

enum AzInlineWordVecDestructorTag {
   AzInlineWordVecDestructorTag_DefaultRust,
   AzInlineWordVecDestructorTag_NoDestructor,
   AzInlineWordVecDestructorTag_External,
};
typedef enum AzInlineWordVecDestructorTag AzInlineWordVecDestructorTag;

struct AzInlineWordVecDestructorVariant_DefaultRust { AzInlineWordVecDestructorTag tag; };
typedef struct AzInlineWordVecDestructorVariant_DefaultRust AzInlineWordVecDestructorVariant_DefaultRust;
struct AzInlineWordVecDestructorVariant_NoDestructor { AzInlineWordVecDestructorTag tag; };
typedef struct AzInlineWordVecDestructorVariant_NoDestructor AzInlineWordVecDestructorVariant_NoDestructor;
struct AzInlineWordVecDestructorVariant_External { AzInlineWordVecDestructorTag tag; AzInlineWordVecDestructorType payload; };
typedef struct AzInlineWordVecDestructorVariant_External AzInlineWordVecDestructorVariant_External;
union AzInlineWordVecDestructor {
    AzInlineWordVecDestructorVariant_DefaultRust DefaultRust;
    AzInlineWordVecDestructorVariant_NoDestructor NoDestructor;
    AzInlineWordVecDestructorVariant_External External;
};
typedef union AzInlineWordVecDestructor AzInlineWordVecDestructor;
#define AzInlineWordVecDestructor_DefaultRust { .DefaultRust = { .tag = AzInlineWordVecDestructorTag_DefaultRust } }
#define AzInlineWordVecDestructor_NoDestructor { .NoDestructor = { .tag = AzInlineWordVecDestructorTag_NoDestructor } }
#define AzInlineWordVecDestructor_External(v) { .External = { .tag = AzInlineWordVecDestructorTag_External, .payload = v } }

enum AzInlineGlyphVecDestructorTag {
   AzInlineGlyphVecDestructorTag_DefaultRust,
   AzInlineGlyphVecDestructorTag_NoDestructor,
   AzInlineGlyphVecDestructorTag_External,
};
typedef enum AzInlineGlyphVecDestructorTag AzInlineGlyphVecDestructorTag;

struct AzInlineGlyphVecDestructorVariant_DefaultRust { AzInlineGlyphVecDestructorTag tag; };
typedef struct AzInlineGlyphVecDestructorVariant_DefaultRust AzInlineGlyphVecDestructorVariant_DefaultRust;
struct AzInlineGlyphVecDestructorVariant_NoDestructor { AzInlineGlyphVecDestructorTag tag; };
typedef struct AzInlineGlyphVecDestructorVariant_NoDestructor AzInlineGlyphVecDestructorVariant_NoDestructor;
struct AzInlineGlyphVecDestructorVariant_External { AzInlineGlyphVecDestructorTag tag; AzInlineGlyphVecDestructorType payload; };
typedef struct AzInlineGlyphVecDestructorVariant_External AzInlineGlyphVecDestructorVariant_External;
union AzInlineGlyphVecDestructor {
    AzInlineGlyphVecDestructorVariant_DefaultRust DefaultRust;
    AzInlineGlyphVecDestructorVariant_NoDestructor NoDestructor;
    AzInlineGlyphVecDestructorVariant_External External;
};
typedef union AzInlineGlyphVecDestructor AzInlineGlyphVecDestructor;
#define AzInlineGlyphVecDestructor_DefaultRust { .DefaultRust = { .tag = AzInlineGlyphVecDestructorTag_DefaultRust } }
#define AzInlineGlyphVecDestructor_NoDestructor { .NoDestructor = { .tag = AzInlineGlyphVecDestructorTag_NoDestructor } }
#define AzInlineGlyphVecDestructor_External(v) { .External = { .tag = AzInlineGlyphVecDestructorTag_External, .payload = v } }

enum AzInlineTextHitVecDestructorTag {
   AzInlineTextHitVecDestructorTag_DefaultRust,
   AzInlineTextHitVecDestructorTag_NoDestructor,
   AzInlineTextHitVecDestructorTag_External,
};
typedef enum AzInlineTextHitVecDestructorTag AzInlineTextHitVecDestructorTag;

struct AzInlineTextHitVecDestructorVariant_DefaultRust { AzInlineTextHitVecDestructorTag tag; };
typedef struct AzInlineTextHitVecDestructorVariant_DefaultRust AzInlineTextHitVecDestructorVariant_DefaultRust;
struct AzInlineTextHitVecDestructorVariant_NoDestructor { AzInlineTextHitVecDestructorTag tag; };
typedef struct AzInlineTextHitVecDestructorVariant_NoDestructor AzInlineTextHitVecDestructorVariant_NoDestructor;
struct AzInlineTextHitVecDestructorVariant_External { AzInlineTextHitVecDestructorTag tag; AzInlineTextHitVecDestructorType payload; };
typedef struct AzInlineTextHitVecDestructorVariant_External AzInlineTextHitVecDestructorVariant_External;
union AzInlineTextHitVecDestructor {
    AzInlineTextHitVecDestructorVariant_DefaultRust DefaultRust;
    AzInlineTextHitVecDestructorVariant_NoDestructor NoDestructor;
    AzInlineTextHitVecDestructorVariant_External External;
};
typedef union AzInlineTextHitVecDestructor AzInlineTextHitVecDestructor;
#define AzInlineTextHitVecDestructor_DefaultRust { .DefaultRust = { .tag = AzInlineTextHitVecDestructorTag_DefaultRust } }
#define AzInlineTextHitVecDestructor_NoDestructor { .NoDestructor = { .tag = AzInlineTextHitVecDestructorTag_NoDestructor } }
#define AzInlineTextHitVecDestructor_External(v) { .External = { .tag = AzInlineTextHitVecDestructorTag_External, .payload = v } }

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
#define AzMonitorVecDestructor_DefaultRust { .DefaultRust = { .tag = AzMonitorVecDestructorTag_DefaultRust } }
#define AzMonitorVecDestructor_NoDestructor { .NoDestructor = { .tag = AzMonitorVecDestructorTag_NoDestructor } }
#define AzMonitorVecDestructor_External(v) { .External = { .tag = AzMonitorVecDestructorTag_External, .payload = v } }

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
#define AzVideoModeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzVideoModeVecDestructorTag_DefaultRust } }
#define AzVideoModeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzVideoModeVecDestructorTag_NoDestructor } }
#define AzVideoModeVecDestructor_External(v) { .External = { .tag = AzVideoModeVecDestructorTag_External, .payload = v } }

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
#define AzDomVecDestructor_DefaultRust { .DefaultRust = { .tag = AzDomVecDestructorTag_DefaultRust } }
#define AzDomVecDestructor_NoDestructor { .NoDestructor = { .tag = AzDomVecDestructorTag_NoDestructor } }
#define AzDomVecDestructor_External(v) { .External = { .tag = AzDomVecDestructorTag_External, .payload = v } }

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
#define AzIdOrClassVecDestructor_DefaultRust { .DefaultRust = { .tag = AzIdOrClassVecDestructorTag_DefaultRust } }
#define AzIdOrClassVecDestructor_NoDestructor { .NoDestructor = { .tag = AzIdOrClassVecDestructorTag_NoDestructor } }
#define AzIdOrClassVecDestructor_External(v) { .External = { .tag = AzIdOrClassVecDestructorTag_External, .payload = v } }

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
#define AzNodeDataInlineCssPropertyVecDestructor_DefaultRust { .DefaultRust = { .tag = AzNodeDataInlineCssPropertyVecDestructorTag_DefaultRust } }
#define AzNodeDataInlineCssPropertyVecDestructor_NoDestructor { .NoDestructor = { .tag = AzNodeDataInlineCssPropertyVecDestructorTag_NoDestructor } }
#define AzNodeDataInlineCssPropertyVecDestructor_External(v) { .External = { .tag = AzNodeDataInlineCssPropertyVecDestructorTag_External, .payload = v } }

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
#define AzStyleBackgroundContentVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStyleBackgroundContentVecDestructorTag_DefaultRust } }
#define AzStyleBackgroundContentVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStyleBackgroundContentVecDestructorTag_NoDestructor } }
#define AzStyleBackgroundContentVecDestructor_External(v) { .External = { .tag = AzStyleBackgroundContentVecDestructorTag_External, .payload = v } }

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
#define AzStyleBackgroundPositionVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStyleBackgroundPositionVecDestructorTag_DefaultRust } }
#define AzStyleBackgroundPositionVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStyleBackgroundPositionVecDestructorTag_NoDestructor } }
#define AzStyleBackgroundPositionVecDestructor_External(v) { .External = { .tag = AzStyleBackgroundPositionVecDestructorTag_External, .payload = v } }

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
#define AzStyleBackgroundRepeatVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStyleBackgroundRepeatVecDestructorTag_DefaultRust } }
#define AzStyleBackgroundRepeatVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStyleBackgroundRepeatVecDestructorTag_NoDestructor } }
#define AzStyleBackgroundRepeatVecDestructor_External(v) { .External = { .tag = AzStyleBackgroundRepeatVecDestructorTag_External, .payload = v } }

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
#define AzStyleBackgroundSizeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStyleBackgroundSizeVecDestructorTag_DefaultRust } }
#define AzStyleBackgroundSizeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStyleBackgroundSizeVecDestructorTag_NoDestructor } }
#define AzStyleBackgroundSizeVecDestructor_External(v) { .External = { .tag = AzStyleBackgroundSizeVecDestructorTag_External, .payload = v } }

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
#define AzStyleTransformVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStyleTransformVecDestructorTag_DefaultRust } }
#define AzStyleTransformVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStyleTransformVecDestructorTag_NoDestructor } }
#define AzStyleTransformVecDestructor_External(v) { .External = { .tag = AzStyleTransformVecDestructorTag_External, .payload = v } }

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
#define AzCssPropertyVecDestructor_DefaultRust { .DefaultRust = { .tag = AzCssPropertyVecDestructorTag_DefaultRust } }
#define AzCssPropertyVecDestructor_NoDestructor { .NoDestructor = { .tag = AzCssPropertyVecDestructorTag_NoDestructor } }
#define AzCssPropertyVecDestructor_External(v) { .External = { .tag = AzCssPropertyVecDestructorTag_External, .payload = v } }

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
#define AzSvgMultiPolygonVecDestructor_DefaultRust { .DefaultRust = { .tag = AzSvgMultiPolygonVecDestructorTag_DefaultRust } }
#define AzSvgMultiPolygonVecDestructor_NoDestructor { .NoDestructor = { .tag = AzSvgMultiPolygonVecDestructorTag_NoDestructor } }
#define AzSvgMultiPolygonVecDestructor_External(v) { .External = { .tag = AzSvgMultiPolygonVecDestructorTag_External, .payload = v } }

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
#define AzSvgPathVecDestructor_DefaultRust { .DefaultRust = { .tag = AzSvgPathVecDestructorTag_DefaultRust } }
#define AzSvgPathVecDestructor_NoDestructor { .NoDestructor = { .tag = AzSvgPathVecDestructorTag_NoDestructor } }
#define AzSvgPathVecDestructor_External(v) { .External = { .tag = AzSvgPathVecDestructorTag_External, .payload = v } }

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
#define AzVertexAttributeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzVertexAttributeVecDestructorTag_DefaultRust } }
#define AzVertexAttributeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzVertexAttributeVecDestructorTag_NoDestructor } }
#define AzVertexAttributeVecDestructor_External(v) { .External = { .tag = AzVertexAttributeVecDestructorTag_External, .payload = v } }

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
#define AzSvgPathElementVecDestructor_DefaultRust { .DefaultRust = { .tag = AzSvgPathElementVecDestructorTag_DefaultRust } }
#define AzSvgPathElementVecDestructor_NoDestructor { .NoDestructor = { .tag = AzSvgPathElementVecDestructorTag_NoDestructor } }
#define AzSvgPathElementVecDestructor_External(v) { .External = { .tag = AzSvgPathElementVecDestructorTag_External, .payload = v } }

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
#define AzSvgVertexVecDestructor_DefaultRust { .DefaultRust = { .tag = AzSvgVertexVecDestructorTag_DefaultRust } }
#define AzSvgVertexVecDestructor_NoDestructor { .NoDestructor = { .tag = AzSvgVertexVecDestructorTag_NoDestructor } }
#define AzSvgVertexVecDestructor_External(v) { .External = { .tag = AzSvgVertexVecDestructorTag_External, .payload = v } }

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
#define AzU32VecDestructor_DefaultRust { .DefaultRust = { .tag = AzU32VecDestructorTag_DefaultRust } }
#define AzU32VecDestructor_NoDestructor { .NoDestructor = { .tag = AzU32VecDestructorTag_NoDestructor } }
#define AzU32VecDestructor_External(v) { .External = { .tag = AzU32VecDestructorTag_External, .payload = v } }

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
#define AzXWindowTypeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzXWindowTypeVecDestructorTag_DefaultRust } }
#define AzXWindowTypeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzXWindowTypeVecDestructorTag_NoDestructor } }
#define AzXWindowTypeVecDestructor_External(v) { .External = { .tag = AzXWindowTypeVecDestructorTag_External, .payload = v } }

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
#define AzVirtualKeyCodeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzVirtualKeyCodeVecDestructorTag_DefaultRust } }
#define AzVirtualKeyCodeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzVirtualKeyCodeVecDestructorTag_NoDestructor } }
#define AzVirtualKeyCodeVecDestructor_External(v) { .External = { .tag = AzVirtualKeyCodeVecDestructorTag_External, .payload = v } }

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
#define AzCascadeInfoVecDestructor_DefaultRust { .DefaultRust = { .tag = AzCascadeInfoVecDestructorTag_DefaultRust } }
#define AzCascadeInfoVecDestructor_NoDestructor { .NoDestructor = { .tag = AzCascadeInfoVecDestructorTag_NoDestructor } }
#define AzCascadeInfoVecDestructor_External(v) { .External = { .tag = AzCascadeInfoVecDestructorTag_External, .payload = v } }

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
#define AzScanCodeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzScanCodeVecDestructorTag_DefaultRust } }
#define AzScanCodeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzScanCodeVecDestructorTag_NoDestructor } }
#define AzScanCodeVecDestructor_External(v) { .External = { .tag = AzScanCodeVecDestructorTag_External, .payload = v } }

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
#define AzCssDeclarationVecDestructor_DefaultRust { .DefaultRust = { .tag = AzCssDeclarationVecDestructorTag_DefaultRust } }
#define AzCssDeclarationVecDestructor_NoDestructor { .NoDestructor = { .tag = AzCssDeclarationVecDestructorTag_NoDestructor } }
#define AzCssDeclarationVecDestructor_External(v) { .External = { .tag = AzCssDeclarationVecDestructorTag_External, .payload = v } }

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
#define AzCssPathSelectorVecDestructor_DefaultRust { .DefaultRust = { .tag = AzCssPathSelectorVecDestructorTag_DefaultRust } }
#define AzCssPathSelectorVecDestructor_NoDestructor { .NoDestructor = { .tag = AzCssPathSelectorVecDestructorTag_NoDestructor } }
#define AzCssPathSelectorVecDestructor_External(v) { .External = { .tag = AzCssPathSelectorVecDestructorTag_External, .payload = v } }

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
#define AzStylesheetVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStylesheetVecDestructorTag_DefaultRust } }
#define AzStylesheetVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStylesheetVecDestructorTag_NoDestructor } }
#define AzStylesheetVecDestructor_External(v) { .External = { .tag = AzStylesheetVecDestructorTag_External, .payload = v } }

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
#define AzCssRuleBlockVecDestructor_DefaultRust { .DefaultRust = { .tag = AzCssRuleBlockVecDestructorTag_DefaultRust } }
#define AzCssRuleBlockVecDestructor_NoDestructor { .NoDestructor = { .tag = AzCssRuleBlockVecDestructorTag_NoDestructor } }
#define AzCssRuleBlockVecDestructor_External(v) { .External = { .tag = AzCssRuleBlockVecDestructorTag_External, .payload = v } }

enum AzF32VecDestructorTag {
   AzF32VecDestructorTag_DefaultRust,
   AzF32VecDestructorTag_NoDestructor,
   AzF32VecDestructorTag_External,
};
typedef enum AzF32VecDestructorTag AzF32VecDestructorTag;

struct AzF32VecDestructorVariant_DefaultRust { AzF32VecDestructorTag tag; };
typedef struct AzF32VecDestructorVariant_DefaultRust AzF32VecDestructorVariant_DefaultRust;
struct AzF32VecDestructorVariant_NoDestructor { AzF32VecDestructorTag tag; };
typedef struct AzF32VecDestructorVariant_NoDestructor AzF32VecDestructorVariant_NoDestructor;
struct AzF32VecDestructorVariant_External { AzF32VecDestructorTag tag; AzF32VecDestructorType payload; };
typedef struct AzF32VecDestructorVariant_External AzF32VecDestructorVariant_External;
union AzF32VecDestructor {
    AzF32VecDestructorVariant_DefaultRust DefaultRust;
    AzF32VecDestructorVariant_NoDestructor NoDestructor;
    AzF32VecDestructorVariant_External External;
};
typedef union AzF32VecDestructor AzF32VecDestructor;
#define AzF32VecDestructor_DefaultRust { .DefaultRust = { .tag = AzF32VecDestructorTag_DefaultRust } }
#define AzF32VecDestructor_NoDestructor { .NoDestructor = { .tag = AzF32VecDestructorTag_NoDestructor } }
#define AzF32VecDestructor_External(v) { .External = { .tag = AzF32VecDestructorTag_External, .payload = v } }

enum AzU16VecDestructorTag {
   AzU16VecDestructorTag_DefaultRust,
   AzU16VecDestructorTag_NoDestructor,
   AzU16VecDestructorTag_External,
};
typedef enum AzU16VecDestructorTag AzU16VecDestructorTag;

struct AzU16VecDestructorVariant_DefaultRust { AzU16VecDestructorTag tag; };
typedef struct AzU16VecDestructorVariant_DefaultRust AzU16VecDestructorVariant_DefaultRust;
struct AzU16VecDestructorVariant_NoDestructor { AzU16VecDestructorTag tag; };
typedef struct AzU16VecDestructorVariant_NoDestructor AzU16VecDestructorVariant_NoDestructor;
struct AzU16VecDestructorVariant_External { AzU16VecDestructorTag tag; AzU16VecDestructorType payload; };
typedef struct AzU16VecDestructorVariant_External AzU16VecDestructorVariant_External;
union AzU16VecDestructor {
    AzU16VecDestructorVariant_DefaultRust DefaultRust;
    AzU16VecDestructorVariant_NoDestructor NoDestructor;
    AzU16VecDestructorVariant_External External;
};
typedef union AzU16VecDestructor AzU16VecDestructor;
#define AzU16VecDestructor_DefaultRust { .DefaultRust = { .tag = AzU16VecDestructorTag_DefaultRust } }
#define AzU16VecDestructor_NoDestructor { .NoDestructor = { .tag = AzU16VecDestructorTag_NoDestructor } }
#define AzU16VecDestructor_External(v) { .External = { .tag = AzU16VecDestructorTag_External, .payload = v } }

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
#define AzU8VecDestructor_DefaultRust { .DefaultRust = { .tag = AzU8VecDestructorTag_DefaultRust } }
#define AzU8VecDestructor_NoDestructor { .NoDestructor = { .tag = AzU8VecDestructorTag_NoDestructor } }
#define AzU8VecDestructor_External(v) { .External = { .tag = AzU8VecDestructorTag_External, .payload = v } }

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
#define AzCallbackDataVecDestructor_DefaultRust { .DefaultRust = { .tag = AzCallbackDataVecDestructorTag_DefaultRust } }
#define AzCallbackDataVecDestructor_NoDestructor { .NoDestructor = { .tag = AzCallbackDataVecDestructorTag_NoDestructor } }
#define AzCallbackDataVecDestructor_External(v) { .External = { .tag = AzCallbackDataVecDestructorTag_External, .payload = v } }

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
#define AzDebugMessageVecDestructor_DefaultRust { .DefaultRust = { .tag = AzDebugMessageVecDestructorTag_DefaultRust } }
#define AzDebugMessageVecDestructor_NoDestructor { .NoDestructor = { .tag = AzDebugMessageVecDestructorTag_NoDestructor } }
#define AzDebugMessageVecDestructor_External(v) { .External = { .tag = AzDebugMessageVecDestructorTag_External, .payload = v } }

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
#define AzGLuintVecDestructor_DefaultRust { .DefaultRust = { .tag = AzGLuintVecDestructorTag_DefaultRust } }
#define AzGLuintVecDestructor_NoDestructor { .NoDestructor = { .tag = AzGLuintVecDestructorTag_NoDestructor } }
#define AzGLuintVecDestructor_External(v) { .External = { .tag = AzGLuintVecDestructorTag_External, .payload = v } }

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
#define AzGLintVecDestructor_DefaultRust { .DefaultRust = { .tag = AzGLintVecDestructorTag_DefaultRust } }
#define AzGLintVecDestructor_NoDestructor { .NoDestructor = { .tag = AzGLintVecDestructorTag_NoDestructor } }
#define AzGLintVecDestructor_External(v) { .External = { .tag = AzGLintVecDestructorTag_External, .payload = v } }

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
#define AzStringVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStringVecDestructorTag_DefaultRust } }
#define AzStringVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStringVecDestructorTag_NoDestructor } }
#define AzStringVecDestructor_External(v) { .External = { .tag = AzStringVecDestructorTag_External, .payload = v } }

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
#define AzStringPairVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStringPairVecDestructorTag_DefaultRust } }
#define AzStringPairVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStringPairVecDestructorTag_NoDestructor } }
#define AzStringPairVecDestructor_External(v) { .External = { .tag = AzStringPairVecDestructorTag_External, .payload = v } }

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
#define AzLinearColorStopVecDestructor_DefaultRust { .DefaultRust = { .tag = AzLinearColorStopVecDestructorTag_DefaultRust } }
#define AzLinearColorStopVecDestructor_NoDestructor { .NoDestructor = { .tag = AzLinearColorStopVecDestructorTag_NoDestructor } }
#define AzLinearColorStopVecDestructor_External(v) { .External = { .tag = AzLinearColorStopVecDestructorTag_External, .payload = v } }

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
#define AzRadialColorStopVecDestructor_DefaultRust { .DefaultRust = { .tag = AzRadialColorStopVecDestructorTag_DefaultRust } }
#define AzRadialColorStopVecDestructor_NoDestructor { .NoDestructor = { .tag = AzRadialColorStopVecDestructorTag_NoDestructor } }
#define AzRadialColorStopVecDestructor_External(v) { .External = { .tag = AzRadialColorStopVecDestructorTag_External, .payload = v } }

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
#define AzNodeIdVecDestructor_DefaultRust { .DefaultRust = { .tag = AzNodeIdVecDestructorTag_DefaultRust } }
#define AzNodeIdVecDestructor_NoDestructor { .NoDestructor = { .tag = AzNodeIdVecDestructorTag_NoDestructor } }
#define AzNodeIdVecDestructor_External(v) { .External = { .tag = AzNodeIdVecDestructorTag_External, .payload = v } }

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
#define AzNodeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzNodeVecDestructorTag_DefaultRust } }
#define AzNodeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzNodeVecDestructorTag_NoDestructor } }
#define AzNodeVecDestructor_External(v) { .External = { .tag = AzNodeVecDestructorTag_External, .payload = v } }

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
#define AzStyledNodeVecDestructor_DefaultRust { .DefaultRust = { .tag = AzStyledNodeVecDestructorTag_DefaultRust } }
#define AzStyledNodeVecDestructor_NoDestructor { .NoDestructor = { .tag = AzStyledNodeVecDestructorTag_NoDestructor } }
#define AzStyledNodeVecDestructor_External(v) { .External = { .tag = AzStyledNodeVecDestructorTag_External, .payload = v } }

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
#define AzTagIdsToNodeIdsMappingVecDestructor_DefaultRust { .DefaultRust = { .tag = AzTagIdsToNodeIdsMappingVecDestructorTag_DefaultRust } }
#define AzTagIdsToNodeIdsMappingVecDestructor_NoDestructor { .NoDestructor = { .tag = AzTagIdsToNodeIdsMappingVecDestructorTag_NoDestructor } }
#define AzTagIdsToNodeIdsMappingVecDestructor_External(v) { .External = { .tag = AzTagIdsToNodeIdsMappingVecDestructorTag_External, .payload = v } }

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
#define AzParentWithNodeDepthVecDestructor_DefaultRust { .DefaultRust = { .tag = AzParentWithNodeDepthVecDestructorTag_DefaultRust } }
#define AzParentWithNodeDepthVecDestructor_NoDestructor { .NoDestructor = { .tag = AzParentWithNodeDepthVecDestructorTag_NoDestructor } }
#define AzParentWithNodeDepthVecDestructor_External(v) { .External = { .tag = AzParentWithNodeDepthVecDestructorTag_External, .payload = v } }

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
#define AzNodeDataVecDestructor_DefaultRust { .DefaultRust = { .tag = AzNodeDataVecDestructorTag_DefaultRust } }
#define AzNodeDataVecDestructor_NoDestructor { .NoDestructor = { .tag = AzNodeDataVecDestructorTag_NoDestructor } }
#define AzNodeDataVecDestructor_External(v) { .External = { .tag = AzNodeDataVecDestructorTag_External, .payload = v } }

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
#define AzOptionHwndHandle_None { .None = { .tag = AzOptionHwndHandleTag_None } }
#define AzOptionHwndHandle_Some(v) { .Some = { .tag = AzOptionHwndHandleTag_Some, .payload = v } }

enum AzOptionX11VisualTag {
   AzOptionX11VisualTag_None,
   AzOptionX11VisualTag_Some,
};
typedef enum AzOptionX11VisualTag AzOptionX11VisualTag;

struct AzOptionX11VisualVariant_None { AzOptionX11VisualTag tag; };
typedef struct AzOptionX11VisualVariant_None AzOptionX11VisualVariant_None;
struct AzOptionX11VisualVariant_Some { AzOptionX11VisualTag tag; void* payload; };
typedef struct AzOptionX11VisualVariant_Some AzOptionX11VisualVariant_Some;
union AzOptionX11Visual {
    AzOptionX11VisualVariant_None None;
    AzOptionX11VisualVariant_Some Some;
};
typedef union AzOptionX11Visual AzOptionX11Visual;
#define AzOptionX11Visual_None { .None = { .tag = AzOptionX11VisualTag_None } }
#define AzOptionX11Visual_Some(v) { .Some = { .tag = AzOptionX11VisualTag_Some, .payload = v } }

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
#define AzOptionI32_None { .None = { .tag = AzOptionI32Tag_None } }
#define AzOptionI32_Some(v) { .Some = { .tag = AzOptionI32Tag_Some, .payload = v } }

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
#define AzOptionF32_None { .None = { .tag = AzOptionF32Tag_None } }
#define AzOptionF32_Some(v) { .Some = { .tag = AzOptionF32Tag_Some, .payload = v } }

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
#define AzOptionChar_None { .None = { .tag = AzOptionCharTag_None } }
#define AzOptionChar_Some(v) { .Some = { .tag = AzOptionCharTag_Some, .payload = v } }

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
#define AzOptionUsize_None { .None = { .tag = AzOptionUsizeTag_None } }
#define AzOptionUsize_Some(v) { .Some = { .tag = AzOptionUsizeTag_Some, .payload = v } }

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

struct AzSystemCallbacks {
    AzCreateThreadFn create_thread_fn;
    AzGetSystemTimeFn get_system_time_fn;
};
typedef struct AzSystemCallbacks AzSystemCallbacks;

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
#define AzRawWindowHandle_IOS(v) { .IOS = { .tag = AzRawWindowHandleTag_IOS, .payload = v } }
#define AzRawWindowHandle_MacOS(v) { .MacOS = { .tag = AzRawWindowHandleTag_MacOS, .payload = v } }
#define AzRawWindowHandle_Xlib(v) { .Xlib = { .tag = AzRawWindowHandleTag_Xlib, .payload = v } }
#define AzRawWindowHandle_Xcb(v) { .Xcb = { .tag = AzRawWindowHandleTag_Xcb, .payload = v } }
#define AzRawWindowHandle_Wayland(v) { .Wayland = { .tag = AzRawWindowHandleTag_Wayland, .payload = v } }
#define AzRawWindowHandle_Windows(v) { .Windows = { .tag = AzRawWindowHandleTag_Windows, .payload = v } }
#define AzRawWindowHandle_Web(v) { .Web = { .tag = AzRawWindowHandleTag_Web, .payload = v } }
#define AzRawWindowHandle_Android(v) { .Android = { .tag = AzRawWindowHandleTag_Android, .payload = v } }
#define AzRawWindowHandle_Unsupported { .Unsupported = { .tag = AzRawWindowHandleTag_Unsupported } }

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
#define AzAcceleratorKey_Ctrl { .Ctrl = { .tag = AzAcceleratorKeyTag_Ctrl } }
#define AzAcceleratorKey_Alt { .Alt = { .tag = AzAcceleratorKeyTag_Alt } }
#define AzAcceleratorKey_Shift { .Shift = { .tag = AzAcceleratorKeyTag_Shift } }
#define AzAcceleratorKey_Key(v) { .Key = { .tag = AzAcceleratorKeyTag_Key, .payload = v } }

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
#define AzCursorPosition_OutOfWindow { .OutOfWindow = { .tag = AzCursorPositionTag_OutOfWindow } }
#define AzCursorPosition_Uninitialized { .Uninitialized = { .tag = AzCursorPositionTag_Uninitialized } }
#define AzCursorPosition_InWindow(v) { .InWindow = { .tag = AzCursorPositionTag_InWindow, .payload = v } }

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
#define AzWindowPosition_Uninitialized { .Uninitialized = { .tag = AzWindowPositionTag_Uninitialized } }
#define AzWindowPosition_Initialized(v) { .Initialized = { .tag = AzWindowPositionTag_Initialized, .payload = v } }

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
#define AzImePosition_Uninitialized { .Uninitialized = { .tag = AzImePositionTag_Uninitialized } }
#define AzImePosition_Initialized(v) { .Initialized = { .tag = AzImePositionTag_Initialized, .payload = v } }

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

struct AzInlineGlyph {
    AzLogicalRect bounds;
    AzOptionChar unicode_codepoint;
    uint32_t glyph_index;
};
typedef struct AzInlineGlyph AzInlineGlyph;

struct AzInlineTextHit {
    AzOptionChar unicode_codepoint;
    AzLogicalPosition hit_relative_to_inline_text;
    AzLogicalPosition hit_relative_to_line;
    AzLogicalPosition hit_relative_to_text_content;
    AzLogicalPosition hit_relative_to_glyph;
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
};
typedef struct AzInlineTextHit AzInlineTextHit;

struct AzIFrameCallbackInfo {
    void* resources;
    AzHidpiAdjustedBounds bounds;
    AzLogicalSize scroll_size;
    AzLogicalPosition scroll_offset;
    AzLogicalSize virtual_scroll_size;
    AzLogicalPosition virtual_scroll_offset;
};
typedef struct AzIFrameCallbackInfo AzIFrameCallbackInfo;

struct AzTimerCallbackReturn {
    AzUpdateScreen should_update;
    AzTerminateTimer should_terminate;
};
typedef struct AzTimerCallbackReturn AzTimerCallbackReturn;

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
#define AzNotEventFilter_Hover(v) { .Hover = { .tag = AzNotEventFilterTag_Hover, .payload = v } }
#define AzNotEventFilter_Focus(v) { .Focus = { .tag = AzNotEventFilterTag_Focus, .payload = v } }

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
#define AzCssNthChildSelector_Number(v) { .Number = { .tag = AzCssNthChildSelectorTag_Number, .payload = v } }
#define AzCssNthChildSelector_Even { .Even = { .tag = AzCssNthChildSelectorTag_Even } }
#define AzCssNthChildSelector_Odd { .Odd = { .tag = AzCssNthChildSelectorTag_Odd } }
#define AzCssNthChildSelector_Pattern(v) { .Pattern = { .tag = AzCssNthChildSelectorTag_Pattern, .payload = v } }

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
#define AzDirection_Angle(v) { .Angle = { .tag = AzDirectionTag_Angle, .payload = v } }
#define AzDirection_FromTo(v) { .FromTo = { .tag = AzDirectionTag_FromTo, .payload = v } }

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
#define AzBackgroundPositionHorizontal_Left { .Left = { .tag = AzBackgroundPositionHorizontalTag_Left } }
#define AzBackgroundPositionHorizontal_Center { .Center = { .tag = AzBackgroundPositionHorizontalTag_Center } }
#define AzBackgroundPositionHorizontal_Right { .Right = { .tag = AzBackgroundPositionHorizontalTag_Right } }
#define AzBackgroundPositionHorizontal_Exact(v) { .Exact = { .tag = AzBackgroundPositionHorizontalTag_Exact, .payload = v } }

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
#define AzBackgroundPositionVertical_Top { .Top = { .tag = AzBackgroundPositionVerticalTag_Top } }
#define AzBackgroundPositionVertical_Center { .Center = { .tag = AzBackgroundPositionVerticalTag_Center } }
#define AzBackgroundPositionVertical_Bottom { .Bottom = { .tag = AzBackgroundPositionVerticalTag_Bottom } }
#define AzBackgroundPositionVertical_Exact(v) { .Exact = { .tag = AzBackgroundPositionVerticalTag_Exact, .payload = v } }

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
#define AzStyleBackgroundSize_ExactSize(v) { .ExactSize = { .tag = AzStyleBackgroundSizeTag_ExactSize, .payload = v } }
#define AzStyleBackgroundSize_Contain { .Contain = { .tag = AzStyleBackgroundSizeTag_Contain } }
#define AzStyleBackgroundSize_Cover { .Cover = { .tag = AzStyleBackgroundSizeTag_Cover } }

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
#define AzStyleBoxShadowValue_Auto { .Auto = { .tag = AzStyleBoxShadowValueTag_Auto } }
#define AzStyleBoxShadowValue_None { .None = { .tag = AzStyleBoxShadowValueTag_None } }
#define AzStyleBoxShadowValue_Inherit { .Inherit = { .tag = AzStyleBoxShadowValueTag_Inherit } }
#define AzStyleBoxShadowValue_Initial { .Initial = { .tag = AzStyleBoxShadowValueTag_Initial } }
#define AzStyleBoxShadowValue_Exact(v) { .Exact = { .tag = AzStyleBoxShadowValueTag_Exact, .payload = v } }

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
#define AzLayoutAlignContentValue_Auto { .Auto = { .tag = AzLayoutAlignContentValueTag_Auto } }
#define AzLayoutAlignContentValue_None { .None = { .tag = AzLayoutAlignContentValueTag_None } }
#define AzLayoutAlignContentValue_Inherit { .Inherit = { .tag = AzLayoutAlignContentValueTag_Inherit } }
#define AzLayoutAlignContentValue_Initial { .Initial = { .tag = AzLayoutAlignContentValueTag_Initial } }
#define AzLayoutAlignContentValue_Exact(v) { .Exact = { .tag = AzLayoutAlignContentValueTag_Exact, .payload = v } }

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
#define AzLayoutAlignItemsValue_Auto { .Auto = { .tag = AzLayoutAlignItemsValueTag_Auto } }
#define AzLayoutAlignItemsValue_None { .None = { .tag = AzLayoutAlignItemsValueTag_None } }
#define AzLayoutAlignItemsValue_Inherit { .Inherit = { .tag = AzLayoutAlignItemsValueTag_Inherit } }
#define AzLayoutAlignItemsValue_Initial { .Initial = { .tag = AzLayoutAlignItemsValueTag_Initial } }
#define AzLayoutAlignItemsValue_Exact(v) { .Exact = { .tag = AzLayoutAlignItemsValueTag_Exact, .payload = v } }

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
#define AzLayoutBottomValue_Auto { .Auto = { .tag = AzLayoutBottomValueTag_Auto } }
#define AzLayoutBottomValue_None { .None = { .tag = AzLayoutBottomValueTag_None } }
#define AzLayoutBottomValue_Inherit { .Inherit = { .tag = AzLayoutBottomValueTag_Inherit } }
#define AzLayoutBottomValue_Initial { .Initial = { .tag = AzLayoutBottomValueTag_Initial } }
#define AzLayoutBottomValue_Exact(v) { .Exact = { .tag = AzLayoutBottomValueTag_Exact, .payload = v } }

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
#define AzLayoutBoxSizingValue_Auto { .Auto = { .tag = AzLayoutBoxSizingValueTag_Auto } }
#define AzLayoutBoxSizingValue_None { .None = { .tag = AzLayoutBoxSizingValueTag_None } }
#define AzLayoutBoxSizingValue_Inherit { .Inherit = { .tag = AzLayoutBoxSizingValueTag_Inherit } }
#define AzLayoutBoxSizingValue_Initial { .Initial = { .tag = AzLayoutBoxSizingValueTag_Initial } }
#define AzLayoutBoxSizingValue_Exact(v) { .Exact = { .tag = AzLayoutBoxSizingValueTag_Exact, .payload = v } }

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
#define AzLayoutFlexDirectionValue_Auto { .Auto = { .tag = AzLayoutFlexDirectionValueTag_Auto } }
#define AzLayoutFlexDirectionValue_None { .None = { .tag = AzLayoutFlexDirectionValueTag_None } }
#define AzLayoutFlexDirectionValue_Inherit { .Inherit = { .tag = AzLayoutFlexDirectionValueTag_Inherit } }
#define AzLayoutFlexDirectionValue_Initial { .Initial = { .tag = AzLayoutFlexDirectionValueTag_Initial } }
#define AzLayoutFlexDirectionValue_Exact(v) { .Exact = { .tag = AzLayoutFlexDirectionValueTag_Exact, .payload = v } }

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
#define AzLayoutDisplayValue_Auto { .Auto = { .tag = AzLayoutDisplayValueTag_Auto } }
#define AzLayoutDisplayValue_None { .None = { .tag = AzLayoutDisplayValueTag_None } }
#define AzLayoutDisplayValue_Inherit { .Inherit = { .tag = AzLayoutDisplayValueTag_Inherit } }
#define AzLayoutDisplayValue_Initial { .Initial = { .tag = AzLayoutDisplayValueTag_Initial } }
#define AzLayoutDisplayValue_Exact(v) { .Exact = { .tag = AzLayoutDisplayValueTag_Exact, .payload = v } }

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
#define AzLayoutFlexGrowValue_Auto { .Auto = { .tag = AzLayoutFlexGrowValueTag_Auto } }
#define AzLayoutFlexGrowValue_None { .None = { .tag = AzLayoutFlexGrowValueTag_None } }
#define AzLayoutFlexGrowValue_Inherit { .Inherit = { .tag = AzLayoutFlexGrowValueTag_Inherit } }
#define AzLayoutFlexGrowValue_Initial { .Initial = { .tag = AzLayoutFlexGrowValueTag_Initial } }
#define AzLayoutFlexGrowValue_Exact(v) { .Exact = { .tag = AzLayoutFlexGrowValueTag_Exact, .payload = v } }

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
#define AzLayoutFlexShrinkValue_Auto { .Auto = { .tag = AzLayoutFlexShrinkValueTag_Auto } }
#define AzLayoutFlexShrinkValue_None { .None = { .tag = AzLayoutFlexShrinkValueTag_None } }
#define AzLayoutFlexShrinkValue_Inherit { .Inherit = { .tag = AzLayoutFlexShrinkValueTag_Inherit } }
#define AzLayoutFlexShrinkValue_Initial { .Initial = { .tag = AzLayoutFlexShrinkValueTag_Initial } }
#define AzLayoutFlexShrinkValue_Exact(v) { .Exact = { .tag = AzLayoutFlexShrinkValueTag_Exact, .payload = v } }

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
#define AzLayoutFloatValue_Auto { .Auto = { .tag = AzLayoutFloatValueTag_Auto } }
#define AzLayoutFloatValue_None { .None = { .tag = AzLayoutFloatValueTag_None } }
#define AzLayoutFloatValue_Inherit { .Inherit = { .tag = AzLayoutFloatValueTag_Inherit } }
#define AzLayoutFloatValue_Initial { .Initial = { .tag = AzLayoutFloatValueTag_Initial } }
#define AzLayoutFloatValue_Exact(v) { .Exact = { .tag = AzLayoutFloatValueTag_Exact, .payload = v } }

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
#define AzLayoutHeightValue_Auto { .Auto = { .tag = AzLayoutHeightValueTag_Auto } }
#define AzLayoutHeightValue_None { .None = { .tag = AzLayoutHeightValueTag_None } }
#define AzLayoutHeightValue_Inherit { .Inherit = { .tag = AzLayoutHeightValueTag_Inherit } }
#define AzLayoutHeightValue_Initial { .Initial = { .tag = AzLayoutHeightValueTag_Initial } }
#define AzLayoutHeightValue_Exact(v) { .Exact = { .tag = AzLayoutHeightValueTag_Exact, .payload = v } }

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
#define AzLayoutJustifyContentValue_Auto { .Auto = { .tag = AzLayoutJustifyContentValueTag_Auto } }
#define AzLayoutJustifyContentValue_None { .None = { .tag = AzLayoutJustifyContentValueTag_None } }
#define AzLayoutJustifyContentValue_Inherit { .Inherit = { .tag = AzLayoutJustifyContentValueTag_Inherit } }
#define AzLayoutJustifyContentValue_Initial { .Initial = { .tag = AzLayoutJustifyContentValueTag_Initial } }
#define AzLayoutJustifyContentValue_Exact(v) { .Exact = { .tag = AzLayoutJustifyContentValueTag_Exact, .payload = v } }

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
#define AzLayoutLeftValue_Auto { .Auto = { .tag = AzLayoutLeftValueTag_Auto } }
#define AzLayoutLeftValue_None { .None = { .tag = AzLayoutLeftValueTag_None } }
#define AzLayoutLeftValue_Inherit { .Inherit = { .tag = AzLayoutLeftValueTag_Inherit } }
#define AzLayoutLeftValue_Initial { .Initial = { .tag = AzLayoutLeftValueTag_Initial } }
#define AzLayoutLeftValue_Exact(v) { .Exact = { .tag = AzLayoutLeftValueTag_Exact, .payload = v } }

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
#define AzLayoutMarginBottomValue_Auto { .Auto = { .tag = AzLayoutMarginBottomValueTag_Auto } }
#define AzLayoutMarginBottomValue_None { .None = { .tag = AzLayoutMarginBottomValueTag_None } }
#define AzLayoutMarginBottomValue_Inherit { .Inherit = { .tag = AzLayoutMarginBottomValueTag_Inherit } }
#define AzLayoutMarginBottomValue_Initial { .Initial = { .tag = AzLayoutMarginBottomValueTag_Initial } }
#define AzLayoutMarginBottomValue_Exact(v) { .Exact = { .tag = AzLayoutMarginBottomValueTag_Exact, .payload = v } }

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
#define AzLayoutMarginLeftValue_Auto { .Auto = { .tag = AzLayoutMarginLeftValueTag_Auto } }
#define AzLayoutMarginLeftValue_None { .None = { .tag = AzLayoutMarginLeftValueTag_None } }
#define AzLayoutMarginLeftValue_Inherit { .Inherit = { .tag = AzLayoutMarginLeftValueTag_Inherit } }
#define AzLayoutMarginLeftValue_Initial { .Initial = { .tag = AzLayoutMarginLeftValueTag_Initial } }
#define AzLayoutMarginLeftValue_Exact(v) { .Exact = { .tag = AzLayoutMarginLeftValueTag_Exact, .payload = v } }

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
#define AzLayoutMarginRightValue_Auto { .Auto = { .tag = AzLayoutMarginRightValueTag_Auto } }
#define AzLayoutMarginRightValue_None { .None = { .tag = AzLayoutMarginRightValueTag_None } }
#define AzLayoutMarginRightValue_Inherit { .Inherit = { .tag = AzLayoutMarginRightValueTag_Inherit } }
#define AzLayoutMarginRightValue_Initial { .Initial = { .tag = AzLayoutMarginRightValueTag_Initial } }
#define AzLayoutMarginRightValue_Exact(v) { .Exact = { .tag = AzLayoutMarginRightValueTag_Exact, .payload = v } }

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
#define AzLayoutMarginTopValue_Auto { .Auto = { .tag = AzLayoutMarginTopValueTag_Auto } }
#define AzLayoutMarginTopValue_None { .None = { .tag = AzLayoutMarginTopValueTag_None } }
#define AzLayoutMarginTopValue_Inherit { .Inherit = { .tag = AzLayoutMarginTopValueTag_Inherit } }
#define AzLayoutMarginTopValue_Initial { .Initial = { .tag = AzLayoutMarginTopValueTag_Initial } }
#define AzLayoutMarginTopValue_Exact(v) { .Exact = { .tag = AzLayoutMarginTopValueTag_Exact, .payload = v } }

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
#define AzLayoutMaxHeightValue_Auto { .Auto = { .tag = AzLayoutMaxHeightValueTag_Auto } }
#define AzLayoutMaxHeightValue_None { .None = { .tag = AzLayoutMaxHeightValueTag_None } }
#define AzLayoutMaxHeightValue_Inherit { .Inherit = { .tag = AzLayoutMaxHeightValueTag_Inherit } }
#define AzLayoutMaxHeightValue_Initial { .Initial = { .tag = AzLayoutMaxHeightValueTag_Initial } }
#define AzLayoutMaxHeightValue_Exact(v) { .Exact = { .tag = AzLayoutMaxHeightValueTag_Exact, .payload = v } }

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
#define AzLayoutMaxWidthValue_Auto { .Auto = { .tag = AzLayoutMaxWidthValueTag_Auto } }
#define AzLayoutMaxWidthValue_None { .None = { .tag = AzLayoutMaxWidthValueTag_None } }
#define AzLayoutMaxWidthValue_Inherit { .Inherit = { .tag = AzLayoutMaxWidthValueTag_Inherit } }
#define AzLayoutMaxWidthValue_Initial { .Initial = { .tag = AzLayoutMaxWidthValueTag_Initial } }
#define AzLayoutMaxWidthValue_Exact(v) { .Exact = { .tag = AzLayoutMaxWidthValueTag_Exact, .payload = v } }

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
#define AzLayoutMinHeightValue_Auto { .Auto = { .tag = AzLayoutMinHeightValueTag_Auto } }
#define AzLayoutMinHeightValue_None { .None = { .tag = AzLayoutMinHeightValueTag_None } }
#define AzLayoutMinHeightValue_Inherit { .Inherit = { .tag = AzLayoutMinHeightValueTag_Inherit } }
#define AzLayoutMinHeightValue_Initial { .Initial = { .tag = AzLayoutMinHeightValueTag_Initial } }
#define AzLayoutMinHeightValue_Exact(v) { .Exact = { .tag = AzLayoutMinHeightValueTag_Exact, .payload = v } }

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
#define AzLayoutMinWidthValue_Auto { .Auto = { .tag = AzLayoutMinWidthValueTag_Auto } }
#define AzLayoutMinWidthValue_None { .None = { .tag = AzLayoutMinWidthValueTag_None } }
#define AzLayoutMinWidthValue_Inherit { .Inherit = { .tag = AzLayoutMinWidthValueTag_Inherit } }
#define AzLayoutMinWidthValue_Initial { .Initial = { .tag = AzLayoutMinWidthValueTag_Initial } }
#define AzLayoutMinWidthValue_Exact(v) { .Exact = { .tag = AzLayoutMinWidthValueTag_Exact, .payload = v } }

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
#define AzLayoutPaddingBottomValue_Auto { .Auto = { .tag = AzLayoutPaddingBottomValueTag_Auto } }
#define AzLayoutPaddingBottomValue_None { .None = { .tag = AzLayoutPaddingBottomValueTag_None } }
#define AzLayoutPaddingBottomValue_Inherit { .Inherit = { .tag = AzLayoutPaddingBottomValueTag_Inherit } }
#define AzLayoutPaddingBottomValue_Initial { .Initial = { .tag = AzLayoutPaddingBottomValueTag_Initial } }
#define AzLayoutPaddingBottomValue_Exact(v) { .Exact = { .tag = AzLayoutPaddingBottomValueTag_Exact, .payload = v } }

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
#define AzLayoutPaddingLeftValue_Auto { .Auto = { .tag = AzLayoutPaddingLeftValueTag_Auto } }
#define AzLayoutPaddingLeftValue_None { .None = { .tag = AzLayoutPaddingLeftValueTag_None } }
#define AzLayoutPaddingLeftValue_Inherit { .Inherit = { .tag = AzLayoutPaddingLeftValueTag_Inherit } }
#define AzLayoutPaddingLeftValue_Initial { .Initial = { .tag = AzLayoutPaddingLeftValueTag_Initial } }
#define AzLayoutPaddingLeftValue_Exact(v) { .Exact = { .tag = AzLayoutPaddingLeftValueTag_Exact, .payload = v } }

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
#define AzLayoutPaddingRightValue_Auto { .Auto = { .tag = AzLayoutPaddingRightValueTag_Auto } }
#define AzLayoutPaddingRightValue_None { .None = { .tag = AzLayoutPaddingRightValueTag_None } }
#define AzLayoutPaddingRightValue_Inherit { .Inherit = { .tag = AzLayoutPaddingRightValueTag_Inherit } }
#define AzLayoutPaddingRightValue_Initial { .Initial = { .tag = AzLayoutPaddingRightValueTag_Initial } }
#define AzLayoutPaddingRightValue_Exact(v) { .Exact = { .tag = AzLayoutPaddingRightValueTag_Exact, .payload = v } }

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
#define AzLayoutPaddingTopValue_Auto { .Auto = { .tag = AzLayoutPaddingTopValueTag_Auto } }
#define AzLayoutPaddingTopValue_None { .None = { .tag = AzLayoutPaddingTopValueTag_None } }
#define AzLayoutPaddingTopValue_Inherit { .Inherit = { .tag = AzLayoutPaddingTopValueTag_Inherit } }
#define AzLayoutPaddingTopValue_Initial { .Initial = { .tag = AzLayoutPaddingTopValueTag_Initial } }
#define AzLayoutPaddingTopValue_Exact(v) { .Exact = { .tag = AzLayoutPaddingTopValueTag_Exact, .payload = v } }

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
#define AzLayoutPositionValue_Auto { .Auto = { .tag = AzLayoutPositionValueTag_Auto } }
#define AzLayoutPositionValue_None { .None = { .tag = AzLayoutPositionValueTag_None } }
#define AzLayoutPositionValue_Inherit { .Inherit = { .tag = AzLayoutPositionValueTag_Inherit } }
#define AzLayoutPositionValue_Initial { .Initial = { .tag = AzLayoutPositionValueTag_Initial } }
#define AzLayoutPositionValue_Exact(v) { .Exact = { .tag = AzLayoutPositionValueTag_Exact, .payload = v } }

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
#define AzLayoutRightValue_Auto { .Auto = { .tag = AzLayoutRightValueTag_Auto } }
#define AzLayoutRightValue_None { .None = { .tag = AzLayoutRightValueTag_None } }
#define AzLayoutRightValue_Inherit { .Inherit = { .tag = AzLayoutRightValueTag_Inherit } }
#define AzLayoutRightValue_Initial { .Initial = { .tag = AzLayoutRightValueTag_Initial } }
#define AzLayoutRightValue_Exact(v) { .Exact = { .tag = AzLayoutRightValueTag_Exact, .payload = v } }

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
#define AzLayoutTopValue_Auto { .Auto = { .tag = AzLayoutTopValueTag_Auto } }
#define AzLayoutTopValue_None { .None = { .tag = AzLayoutTopValueTag_None } }
#define AzLayoutTopValue_Inherit { .Inherit = { .tag = AzLayoutTopValueTag_Inherit } }
#define AzLayoutTopValue_Initial { .Initial = { .tag = AzLayoutTopValueTag_Initial } }
#define AzLayoutTopValue_Exact(v) { .Exact = { .tag = AzLayoutTopValueTag_Exact, .payload = v } }

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
#define AzLayoutWidthValue_Auto { .Auto = { .tag = AzLayoutWidthValueTag_Auto } }
#define AzLayoutWidthValue_None { .None = { .tag = AzLayoutWidthValueTag_None } }
#define AzLayoutWidthValue_Inherit { .Inherit = { .tag = AzLayoutWidthValueTag_Inherit } }
#define AzLayoutWidthValue_Initial { .Initial = { .tag = AzLayoutWidthValueTag_Initial } }
#define AzLayoutWidthValue_Exact(v) { .Exact = { .tag = AzLayoutWidthValueTag_Exact, .payload = v } }

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
#define AzLayoutFlexWrapValue_Auto { .Auto = { .tag = AzLayoutFlexWrapValueTag_Auto } }
#define AzLayoutFlexWrapValue_None { .None = { .tag = AzLayoutFlexWrapValueTag_None } }
#define AzLayoutFlexWrapValue_Inherit { .Inherit = { .tag = AzLayoutFlexWrapValueTag_Inherit } }
#define AzLayoutFlexWrapValue_Initial { .Initial = { .tag = AzLayoutFlexWrapValueTag_Initial } }
#define AzLayoutFlexWrapValue_Exact(v) { .Exact = { .tag = AzLayoutFlexWrapValueTag_Exact, .payload = v } }

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
#define AzLayoutOverflowValue_Auto { .Auto = { .tag = AzLayoutOverflowValueTag_Auto } }
#define AzLayoutOverflowValue_None { .None = { .tag = AzLayoutOverflowValueTag_None } }
#define AzLayoutOverflowValue_Inherit { .Inherit = { .tag = AzLayoutOverflowValueTag_Inherit } }
#define AzLayoutOverflowValue_Initial { .Initial = { .tag = AzLayoutOverflowValueTag_Initial } }
#define AzLayoutOverflowValue_Exact(v) { .Exact = { .tag = AzLayoutOverflowValueTag_Exact, .payload = v } }

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
#define AzStyleBorderBottomColorValue_Auto { .Auto = { .tag = AzStyleBorderBottomColorValueTag_Auto } }
#define AzStyleBorderBottomColorValue_None { .None = { .tag = AzStyleBorderBottomColorValueTag_None } }
#define AzStyleBorderBottomColorValue_Inherit { .Inherit = { .tag = AzStyleBorderBottomColorValueTag_Inherit } }
#define AzStyleBorderBottomColorValue_Initial { .Initial = { .tag = AzStyleBorderBottomColorValueTag_Initial } }
#define AzStyleBorderBottomColorValue_Exact(v) { .Exact = { .tag = AzStyleBorderBottomColorValueTag_Exact, .payload = v } }

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
#define AzStyleBorderBottomLeftRadiusValue_Auto { .Auto = { .tag = AzStyleBorderBottomLeftRadiusValueTag_Auto } }
#define AzStyleBorderBottomLeftRadiusValue_None { .None = { .tag = AzStyleBorderBottomLeftRadiusValueTag_None } }
#define AzStyleBorderBottomLeftRadiusValue_Inherit { .Inherit = { .tag = AzStyleBorderBottomLeftRadiusValueTag_Inherit } }
#define AzStyleBorderBottomLeftRadiusValue_Initial { .Initial = { .tag = AzStyleBorderBottomLeftRadiusValueTag_Initial } }
#define AzStyleBorderBottomLeftRadiusValue_Exact(v) { .Exact = { .tag = AzStyleBorderBottomLeftRadiusValueTag_Exact, .payload = v } }

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
#define AzStyleBorderBottomRightRadiusValue_Auto { .Auto = { .tag = AzStyleBorderBottomRightRadiusValueTag_Auto } }
#define AzStyleBorderBottomRightRadiusValue_None { .None = { .tag = AzStyleBorderBottomRightRadiusValueTag_None } }
#define AzStyleBorderBottomRightRadiusValue_Inherit { .Inherit = { .tag = AzStyleBorderBottomRightRadiusValueTag_Inherit } }
#define AzStyleBorderBottomRightRadiusValue_Initial { .Initial = { .tag = AzStyleBorderBottomRightRadiusValueTag_Initial } }
#define AzStyleBorderBottomRightRadiusValue_Exact(v) { .Exact = { .tag = AzStyleBorderBottomRightRadiusValueTag_Exact, .payload = v } }

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
#define AzStyleBorderBottomStyleValue_Auto { .Auto = { .tag = AzStyleBorderBottomStyleValueTag_Auto } }
#define AzStyleBorderBottomStyleValue_None { .None = { .tag = AzStyleBorderBottomStyleValueTag_None } }
#define AzStyleBorderBottomStyleValue_Inherit { .Inherit = { .tag = AzStyleBorderBottomStyleValueTag_Inherit } }
#define AzStyleBorderBottomStyleValue_Initial { .Initial = { .tag = AzStyleBorderBottomStyleValueTag_Initial } }
#define AzStyleBorderBottomStyleValue_Exact(v) { .Exact = { .tag = AzStyleBorderBottomStyleValueTag_Exact, .payload = v } }

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
#define AzLayoutBorderBottomWidthValue_Auto { .Auto = { .tag = AzLayoutBorderBottomWidthValueTag_Auto } }
#define AzLayoutBorderBottomWidthValue_None { .None = { .tag = AzLayoutBorderBottomWidthValueTag_None } }
#define AzLayoutBorderBottomWidthValue_Inherit { .Inherit = { .tag = AzLayoutBorderBottomWidthValueTag_Inherit } }
#define AzLayoutBorderBottomWidthValue_Initial { .Initial = { .tag = AzLayoutBorderBottomWidthValueTag_Initial } }
#define AzLayoutBorderBottomWidthValue_Exact(v) { .Exact = { .tag = AzLayoutBorderBottomWidthValueTag_Exact, .payload = v } }

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
#define AzStyleBorderLeftColorValue_Auto { .Auto = { .tag = AzStyleBorderLeftColorValueTag_Auto } }
#define AzStyleBorderLeftColorValue_None { .None = { .tag = AzStyleBorderLeftColorValueTag_None } }
#define AzStyleBorderLeftColorValue_Inherit { .Inherit = { .tag = AzStyleBorderLeftColorValueTag_Inherit } }
#define AzStyleBorderLeftColorValue_Initial { .Initial = { .tag = AzStyleBorderLeftColorValueTag_Initial } }
#define AzStyleBorderLeftColorValue_Exact(v) { .Exact = { .tag = AzStyleBorderLeftColorValueTag_Exact, .payload = v } }

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
#define AzStyleBorderLeftStyleValue_Auto { .Auto = { .tag = AzStyleBorderLeftStyleValueTag_Auto } }
#define AzStyleBorderLeftStyleValue_None { .None = { .tag = AzStyleBorderLeftStyleValueTag_None } }
#define AzStyleBorderLeftStyleValue_Inherit { .Inherit = { .tag = AzStyleBorderLeftStyleValueTag_Inherit } }
#define AzStyleBorderLeftStyleValue_Initial { .Initial = { .tag = AzStyleBorderLeftStyleValueTag_Initial } }
#define AzStyleBorderLeftStyleValue_Exact(v) { .Exact = { .tag = AzStyleBorderLeftStyleValueTag_Exact, .payload = v } }

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
#define AzLayoutBorderLeftWidthValue_Auto { .Auto = { .tag = AzLayoutBorderLeftWidthValueTag_Auto } }
#define AzLayoutBorderLeftWidthValue_None { .None = { .tag = AzLayoutBorderLeftWidthValueTag_None } }
#define AzLayoutBorderLeftWidthValue_Inherit { .Inherit = { .tag = AzLayoutBorderLeftWidthValueTag_Inherit } }
#define AzLayoutBorderLeftWidthValue_Initial { .Initial = { .tag = AzLayoutBorderLeftWidthValueTag_Initial } }
#define AzLayoutBorderLeftWidthValue_Exact(v) { .Exact = { .tag = AzLayoutBorderLeftWidthValueTag_Exact, .payload = v } }

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
#define AzStyleBorderRightColorValue_Auto { .Auto = { .tag = AzStyleBorderRightColorValueTag_Auto } }
#define AzStyleBorderRightColorValue_None { .None = { .tag = AzStyleBorderRightColorValueTag_None } }
#define AzStyleBorderRightColorValue_Inherit { .Inherit = { .tag = AzStyleBorderRightColorValueTag_Inherit } }
#define AzStyleBorderRightColorValue_Initial { .Initial = { .tag = AzStyleBorderRightColorValueTag_Initial } }
#define AzStyleBorderRightColorValue_Exact(v) { .Exact = { .tag = AzStyleBorderRightColorValueTag_Exact, .payload = v } }

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
#define AzStyleBorderRightStyleValue_Auto { .Auto = { .tag = AzStyleBorderRightStyleValueTag_Auto } }
#define AzStyleBorderRightStyleValue_None { .None = { .tag = AzStyleBorderRightStyleValueTag_None } }
#define AzStyleBorderRightStyleValue_Inherit { .Inherit = { .tag = AzStyleBorderRightStyleValueTag_Inherit } }
#define AzStyleBorderRightStyleValue_Initial { .Initial = { .tag = AzStyleBorderRightStyleValueTag_Initial } }
#define AzStyleBorderRightStyleValue_Exact(v) { .Exact = { .tag = AzStyleBorderRightStyleValueTag_Exact, .payload = v } }

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
#define AzLayoutBorderRightWidthValue_Auto { .Auto = { .tag = AzLayoutBorderRightWidthValueTag_Auto } }
#define AzLayoutBorderRightWidthValue_None { .None = { .tag = AzLayoutBorderRightWidthValueTag_None } }
#define AzLayoutBorderRightWidthValue_Inherit { .Inherit = { .tag = AzLayoutBorderRightWidthValueTag_Inherit } }
#define AzLayoutBorderRightWidthValue_Initial { .Initial = { .tag = AzLayoutBorderRightWidthValueTag_Initial } }
#define AzLayoutBorderRightWidthValue_Exact(v) { .Exact = { .tag = AzLayoutBorderRightWidthValueTag_Exact, .payload = v } }

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
#define AzStyleBorderTopColorValue_Auto { .Auto = { .tag = AzStyleBorderTopColorValueTag_Auto } }
#define AzStyleBorderTopColorValue_None { .None = { .tag = AzStyleBorderTopColorValueTag_None } }
#define AzStyleBorderTopColorValue_Inherit { .Inherit = { .tag = AzStyleBorderTopColorValueTag_Inherit } }
#define AzStyleBorderTopColorValue_Initial { .Initial = { .tag = AzStyleBorderTopColorValueTag_Initial } }
#define AzStyleBorderTopColorValue_Exact(v) { .Exact = { .tag = AzStyleBorderTopColorValueTag_Exact, .payload = v } }

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
#define AzStyleBorderTopLeftRadiusValue_Auto { .Auto = { .tag = AzStyleBorderTopLeftRadiusValueTag_Auto } }
#define AzStyleBorderTopLeftRadiusValue_None { .None = { .tag = AzStyleBorderTopLeftRadiusValueTag_None } }
#define AzStyleBorderTopLeftRadiusValue_Inherit { .Inherit = { .tag = AzStyleBorderTopLeftRadiusValueTag_Inherit } }
#define AzStyleBorderTopLeftRadiusValue_Initial { .Initial = { .tag = AzStyleBorderTopLeftRadiusValueTag_Initial } }
#define AzStyleBorderTopLeftRadiusValue_Exact(v) { .Exact = { .tag = AzStyleBorderTopLeftRadiusValueTag_Exact, .payload = v } }

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
#define AzStyleBorderTopRightRadiusValue_Auto { .Auto = { .tag = AzStyleBorderTopRightRadiusValueTag_Auto } }
#define AzStyleBorderTopRightRadiusValue_None { .None = { .tag = AzStyleBorderTopRightRadiusValueTag_None } }
#define AzStyleBorderTopRightRadiusValue_Inherit { .Inherit = { .tag = AzStyleBorderTopRightRadiusValueTag_Inherit } }
#define AzStyleBorderTopRightRadiusValue_Initial { .Initial = { .tag = AzStyleBorderTopRightRadiusValueTag_Initial } }
#define AzStyleBorderTopRightRadiusValue_Exact(v) { .Exact = { .tag = AzStyleBorderTopRightRadiusValueTag_Exact, .payload = v } }

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
#define AzStyleBorderTopStyleValue_Auto { .Auto = { .tag = AzStyleBorderTopStyleValueTag_Auto } }
#define AzStyleBorderTopStyleValue_None { .None = { .tag = AzStyleBorderTopStyleValueTag_None } }
#define AzStyleBorderTopStyleValue_Inherit { .Inherit = { .tag = AzStyleBorderTopStyleValueTag_Inherit } }
#define AzStyleBorderTopStyleValue_Initial { .Initial = { .tag = AzStyleBorderTopStyleValueTag_Initial } }
#define AzStyleBorderTopStyleValue_Exact(v) { .Exact = { .tag = AzStyleBorderTopStyleValueTag_Exact, .payload = v } }

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
#define AzLayoutBorderTopWidthValue_Auto { .Auto = { .tag = AzLayoutBorderTopWidthValueTag_Auto } }
#define AzLayoutBorderTopWidthValue_None { .None = { .tag = AzLayoutBorderTopWidthValueTag_None } }
#define AzLayoutBorderTopWidthValue_Inherit { .Inherit = { .tag = AzLayoutBorderTopWidthValueTag_Inherit } }
#define AzLayoutBorderTopWidthValue_Initial { .Initial = { .tag = AzLayoutBorderTopWidthValueTag_Initial } }
#define AzLayoutBorderTopWidthValue_Exact(v) { .Exact = { .tag = AzLayoutBorderTopWidthValueTag_Exact, .payload = v } }

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
#define AzStyleCursorValue_Auto { .Auto = { .tag = AzStyleCursorValueTag_Auto } }
#define AzStyleCursorValue_None { .None = { .tag = AzStyleCursorValueTag_None } }
#define AzStyleCursorValue_Inherit { .Inherit = { .tag = AzStyleCursorValueTag_Inherit } }
#define AzStyleCursorValue_Initial { .Initial = { .tag = AzStyleCursorValueTag_Initial } }
#define AzStyleCursorValue_Exact(v) { .Exact = { .tag = AzStyleCursorValueTag_Exact, .payload = v } }

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
#define AzStyleFontSizeValue_Auto { .Auto = { .tag = AzStyleFontSizeValueTag_Auto } }
#define AzStyleFontSizeValue_None { .None = { .tag = AzStyleFontSizeValueTag_None } }
#define AzStyleFontSizeValue_Inherit { .Inherit = { .tag = AzStyleFontSizeValueTag_Inherit } }
#define AzStyleFontSizeValue_Initial { .Initial = { .tag = AzStyleFontSizeValueTag_Initial } }
#define AzStyleFontSizeValue_Exact(v) { .Exact = { .tag = AzStyleFontSizeValueTag_Exact, .payload = v } }

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
#define AzStyleLetterSpacingValue_Auto { .Auto = { .tag = AzStyleLetterSpacingValueTag_Auto } }
#define AzStyleLetterSpacingValue_None { .None = { .tag = AzStyleLetterSpacingValueTag_None } }
#define AzStyleLetterSpacingValue_Inherit { .Inherit = { .tag = AzStyleLetterSpacingValueTag_Inherit } }
#define AzStyleLetterSpacingValue_Initial { .Initial = { .tag = AzStyleLetterSpacingValueTag_Initial } }
#define AzStyleLetterSpacingValue_Exact(v) { .Exact = { .tag = AzStyleLetterSpacingValueTag_Exact, .payload = v } }

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
#define AzStyleLineHeightValue_Auto { .Auto = { .tag = AzStyleLineHeightValueTag_Auto } }
#define AzStyleLineHeightValue_None { .None = { .tag = AzStyleLineHeightValueTag_None } }
#define AzStyleLineHeightValue_Inherit { .Inherit = { .tag = AzStyleLineHeightValueTag_Inherit } }
#define AzStyleLineHeightValue_Initial { .Initial = { .tag = AzStyleLineHeightValueTag_Initial } }
#define AzStyleLineHeightValue_Exact(v) { .Exact = { .tag = AzStyleLineHeightValueTag_Exact, .payload = v } }

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
#define AzStyleTabWidthValue_Auto { .Auto = { .tag = AzStyleTabWidthValueTag_Auto } }
#define AzStyleTabWidthValue_None { .None = { .tag = AzStyleTabWidthValueTag_None } }
#define AzStyleTabWidthValue_Inherit { .Inherit = { .tag = AzStyleTabWidthValueTag_Inherit } }
#define AzStyleTabWidthValue_Initial { .Initial = { .tag = AzStyleTabWidthValueTag_Initial } }
#define AzStyleTabWidthValue_Exact(v) { .Exact = { .tag = AzStyleTabWidthValueTag_Exact, .payload = v } }

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
#define AzStyleTextAlignmentHorzValue_Auto { .Auto = { .tag = AzStyleTextAlignmentHorzValueTag_Auto } }
#define AzStyleTextAlignmentHorzValue_None { .None = { .tag = AzStyleTextAlignmentHorzValueTag_None } }
#define AzStyleTextAlignmentHorzValue_Inherit { .Inherit = { .tag = AzStyleTextAlignmentHorzValueTag_Inherit } }
#define AzStyleTextAlignmentHorzValue_Initial { .Initial = { .tag = AzStyleTextAlignmentHorzValueTag_Initial } }
#define AzStyleTextAlignmentHorzValue_Exact(v) { .Exact = { .tag = AzStyleTextAlignmentHorzValueTag_Exact, .payload = v } }

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
#define AzStyleTextColorValue_Auto { .Auto = { .tag = AzStyleTextColorValueTag_Auto } }
#define AzStyleTextColorValue_None { .None = { .tag = AzStyleTextColorValueTag_None } }
#define AzStyleTextColorValue_Inherit { .Inherit = { .tag = AzStyleTextColorValueTag_Inherit } }
#define AzStyleTextColorValue_Initial { .Initial = { .tag = AzStyleTextColorValueTag_Initial } }
#define AzStyleTextColorValue_Exact(v) { .Exact = { .tag = AzStyleTextColorValueTag_Exact, .payload = v } }

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
#define AzStyleWordSpacingValue_Auto { .Auto = { .tag = AzStyleWordSpacingValueTag_Auto } }
#define AzStyleWordSpacingValue_None { .None = { .tag = AzStyleWordSpacingValueTag_None } }
#define AzStyleWordSpacingValue_Inherit { .Inherit = { .tag = AzStyleWordSpacingValueTag_Inherit } }
#define AzStyleWordSpacingValue_Initial { .Initial = { .tag = AzStyleWordSpacingValueTag_Initial } }
#define AzStyleWordSpacingValue_Exact(v) { .Exact = { .tag = AzStyleWordSpacingValueTag_Exact, .payload = v } }

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
#define AzStyleOpacityValue_Auto { .Auto = { .tag = AzStyleOpacityValueTag_Auto } }
#define AzStyleOpacityValue_None { .None = { .tag = AzStyleOpacityValueTag_None } }
#define AzStyleOpacityValue_Inherit { .Inherit = { .tag = AzStyleOpacityValueTag_Inherit } }
#define AzStyleOpacityValue_Initial { .Initial = { .tag = AzStyleOpacityValueTag_Initial } }
#define AzStyleOpacityValue_Exact(v) { .Exact = { .tag = AzStyleOpacityValueTag_Exact, .payload = v } }

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
#define AzStyleTransformOriginValue_Auto { .Auto = { .tag = AzStyleTransformOriginValueTag_Auto } }
#define AzStyleTransformOriginValue_None { .None = { .tag = AzStyleTransformOriginValueTag_None } }
#define AzStyleTransformOriginValue_Inherit { .Inherit = { .tag = AzStyleTransformOriginValueTag_Inherit } }
#define AzStyleTransformOriginValue_Initial { .Initial = { .tag = AzStyleTransformOriginValueTag_Initial } }
#define AzStyleTransformOriginValue_Exact(v) { .Exact = { .tag = AzStyleTransformOriginValueTag_Exact, .payload = v } }

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
#define AzStylePerspectiveOriginValue_Auto { .Auto = { .tag = AzStylePerspectiveOriginValueTag_Auto } }
#define AzStylePerspectiveOriginValue_None { .None = { .tag = AzStylePerspectiveOriginValueTag_None } }
#define AzStylePerspectiveOriginValue_Inherit { .Inherit = { .tag = AzStylePerspectiveOriginValueTag_Inherit } }
#define AzStylePerspectiveOriginValue_Initial { .Initial = { .tag = AzStylePerspectiveOriginValueTag_Initial } }
#define AzStylePerspectiveOriginValue_Exact(v) { .Exact = { .tag = AzStylePerspectiveOriginValueTag_Exact, .payload = v } }

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
#define AzStyleBackfaceVisibilityValue_Auto { .Auto = { .tag = AzStyleBackfaceVisibilityValueTag_Auto } }
#define AzStyleBackfaceVisibilityValue_None { .None = { .tag = AzStyleBackfaceVisibilityValueTag_None } }
#define AzStyleBackfaceVisibilityValue_Inherit { .Inherit = { .tag = AzStyleBackfaceVisibilityValueTag_Inherit } }
#define AzStyleBackfaceVisibilityValue_Initial { .Initial = { .tag = AzStyleBackfaceVisibilityValueTag_Initial } }
#define AzStyleBackfaceVisibilityValue_Exact(v) { .Exact = { .tag = AzStyleBackfaceVisibilityValueTag_Exact, .payload = v } }

struct AzParentWithNodeDepth {
    size_t depth;
    AzNodeId node_id;
};
typedef struct AzParentWithNodeDepth AzParentWithNodeDepth;

struct AzGl {
    void* ptr;
    uint32_t svg_shader;
    uint32_t fxaa_shader;
    AzRendererType renderer_type;
};
typedef struct AzGl AzGl;

struct AzRefstrVecRef {
    AzRefstr* ptr;
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

struct AzSvgStringFormatOptions {
    bool  use_single_quote;
    AzIndent indent;
    AzIndent attributes_indent;
};
typedef struct AzSvgStringFormatOptions AzSvgStringFormatOptions;

struct AzSvgFillStyle {
    AzSvgLineJoin line_join;
    float miter_limit;
    float tolerance;
    AzSvgFillRule fill_rule;
    AzSvgTransform transform;
    bool  anti_alias;
    bool  high_quality_aa;
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

struct AzXmlNode;
typedef struct AzXmlNode AzXmlNode;
struct AzXmlNodeVec {
    AzXmlNode* ptr;
    size_t len;
    size_t cap;
    AzXmlNodeVecDestructor destructor;
};
typedef struct AzXmlNodeVec AzXmlNodeVec;

struct AzInlineGlyphVec {
    AzInlineGlyph* ptr;
    size_t len;
    size_t cap;
    AzInlineGlyphVecDestructor destructor;
};
typedef struct AzInlineGlyphVec AzInlineGlyphVec;

struct AzInlineTextHitVec {
    AzInlineTextHit* ptr;
    size_t len;
    size_t cap;
    AzInlineTextHitVecDestructor destructor;
};
typedef struct AzInlineTextHitVec AzInlineTextHitVec;

struct AzVideoModeVec {
    AzVideoMode* ptr;
    size_t len;
    size_t cap;
    AzVideoModeVecDestructor destructor;
};
typedef struct AzVideoModeVec AzVideoModeVec;

struct AzDom;
typedef struct AzDom AzDom;
struct AzDomVec {
    AzDom* ptr;
    size_t len;
    size_t cap;
    AzDomVecDestructor destructor;
};
typedef struct AzDomVec AzDomVec;

struct AzStyleBackgroundPositionVec {
    AzStyleBackgroundPosition* ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundPositionVecDestructor destructor;
};
typedef struct AzStyleBackgroundPositionVec AzStyleBackgroundPositionVec;

struct AzStyleBackgroundRepeatVec {
    AzStyleBackgroundRepeat* ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundRepeatVecDestructor destructor;
};
typedef struct AzStyleBackgroundRepeatVec AzStyleBackgroundRepeatVec;

struct AzStyleBackgroundSizeVec {
    AzStyleBackgroundSize* ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundSizeVecDestructor destructor;
};
typedef struct AzStyleBackgroundSizeVec AzStyleBackgroundSizeVec;

struct AzSvgVertexVec {
    AzSvgVertex* ptr;
    size_t len;
    size_t cap;
    AzSvgVertexVecDestructor destructor;
};
typedef struct AzSvgVertexVec AzSvgVertexVec;

struct AzU32Vec {
    uint32_t* ptr;
    size_t len;
    size_t cap;
    AzU32VecDestructor destructor;
};
typedef struct AzU32Vec AzU32Vec;

struct AzXWindowTypeVec {
    AzXWindowType* ptr;
    size_t len;
    size_t cap;
    AzXWindowTypeVecDestructor destructor;
};
typedef struct AzXWindowTypeVec AzXWindowTypeVec;

struct AzVirtualKeyCodeVec {
    AzVirtualKeyCode* ptr;
    size_t len;
    size_t cap;
    AzVirtualKeyCodeVecDestructor destructor;
};
typedef struct AzVirtualKeyCodeVec AzVirtualKeyCodeVec;

struct AzCascadeInfoVec {
    AzCascadeInfo* ptr;
    size_t len;
    size_t cap;
    AzCascadeInfoVecDestructor destructor;
};
typedef struct AzCascadeInfoVec AzCascadeInfoVec;

struct AzScanCodeVec {
    uint32_t* ptr;
    size_t len;
    size_t cap;
    AzScanCodeVecDestructor destructor;
};
typedef struct AzScanCodeVec AzScanCodeVec;

struct AzU16Vec {
    uint16_t* ptr;
    size_t len;
    size_t cap;
    AzU16VecDestructor destructor;
};
typedef struct AzU16Vec AzU16Vec;

struct AzF32Vec {
    float* ptr;
    size_t len;
    size_t cap;
    AzF32VecDestructor destructor;
};
typedef struct AzF32Vec AzF32Vec;

struct AzU8Vec {
    uint8_t* ptr;
    size_t len;
    size_t cap;
    AzU8VecDestructor destructor;
};
typedef struct AzU8Vec AzU8Vec;

struct AzGLuintVec {
    uint32_t* ptr;
    size_t len;
    size_t cap;
    AzGLuintVecDestructor destructor;
};
typedef struct AzGLuintVec AzGLuintVec;

struct AzGLintVec {
    int32_t* ptr;
    size_t len;
    size_t cap;
    AzGLintVecDestructor destructor;
};
typedef struct AzGLintVec AzGLintVec;

struct AzNodeIdVec {
    AzNodeId* ptr;
    size_t len;
    size_t cap;
    AzNodeIdVecDestructor destructor;
};
typedef struct AzNodeIdVec AzNodeIdVec;

struct AzNodeVec {
    AzNode* ptr;
    size_t len;
    size_t cap;
    AzNodeVecDestructor destructor;
};
typedef struct AzNodeVec AzNodeVec;

struct AzParentWithNodeDepthVec {
    AzParentWithNodeDepth* ptr;
    size_t len;
    size_t cap;
    AzParentWithNodeDepthVecDestructor destructor;
};
typedef struct AzParentWithNodeDepthVec AzParentWithNodeDepthVec;

enum AzOptionFileTag {
   AzOptionFileTag_None,
   AzOptionFileTag_Some,
};
typedef enum AzOptionFileTag AzOptionFileTag;

struct AzOptionFileVariant_None { AzOptionFileTag tag; };
typedef struct AzOptionFileVariant_None AzOptionFileVariant_None;
struct AzOptionFileVariant_Some { AzOptionFileTag tag; AzFile payload; };
typedef struct AzOptionFileVariant_Some AzOptionFileVariant_Some;
union AzOptionFile {
    AzOptionFileVariant_None None;
    AzOptionFileVariant_Some Some;
};
typedef union AzOptionFile AzOptionFile;
#define AzOptionFile_None { .None = { .tag = AzOptionFileTag_None } }
#define AzOptionFile_Some(v) { .Some = { .tag = AzOptionFileTag_Some, .payload = v } }

enum AzOptionGlTag {
   AzOptionGlTag_None,
   AzOptionGlTag_Some,
};
typedef enum AzOptionGlTag AzOptionGlTag;

struct AzOptionGlVariant_None { AzOptionGlTag tag; };
typedef struct AzOptionGlVariant_None AzOptionGlVariant_None;
struct AzOptionGlVariant_Some { AzOptionGlTag tag; AzGl payload; };
typedef struct AzOptionGlVariant_Some AzOptionGlVariant_Some;
union AzOptionGl {
    AzOptionGlVariant_None None;
    AzOptionGlVariant_Some Some;
};
typedef union AzOptionGl AzOptionGl;
#define AzOptionGl_None { .None = { .tag = AzOptionGlTag_None } }
#define AzOptionGl_Some(v) { .Some = { .tag = AzOptionGlTag_Some, .payload = v } }

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
#define AzOptionPercentageValue_None { .None = { .tag = AzOptionPercentageValueTag_None } }
#define AzOptionPercentageValue_Some(v) { .Some = { .tag = AzOptionPercentageValueTag_Some, .payload = v } }

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
#define AzOptionAngleValue_None { .None = { .tag = AzOptionAngleValueTag_None } }
#define AzOptionAngleValue_Some(v) { .Some = { .tag = AzOptionAngleValueTag_Some, .payload = v } }

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
#define AzOptionRendererOptions_None { .None = { .tag = AzOptionRendererOptionsTag_None } }
#define AzOptionRendererOptions_Some(v) { .Some = { .tag = AzOptionRendererOptionsTag_Some, .payload = v } }

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
#define AzOptionCallback_None { .None = { .tag = AzOptionCallbackTag_None } }
#define AzOptionCallback_Some(v) { .Some = { .tag = AzOptionCallbackTag_Some, .payload = v } }

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
#define AzOptionThreadSendMsg_None { .None = { .tag = AzOptionThreadSendMsgTag_None } }
#define AzOptionThreadSendMsg_Some(v) { .Some = { .tag = AzOptionThreadSendMsgTag_Some, .payload = v } }

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
#define AzOptionLayoutRect_None { .None = { .tag = AzOptionLayoutRectTag_None } }
#define AzOptionLayoutRect_Some(v) { .Some = { .tag = AzOptionLayoutRectTag_Some, .payload = v } }

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
#define AzOptionLayoutPoint_None { .None = { .tag = AzOptionLayoutPointTag_None } }
#define AzOptionLayoutPoint_Some(v) { .Some = { .tag = AzOptionLayoutPointTag_Some, .payload = v } }

enum AzOptionLayoutSizeTag {
   AzOptionLayoutSizeTag_None,
   AzOptionLayoutSizeTag_Some,
};
typedef enum AzOptionLayoutSizeTag AzOptionLayoutSizeTag;

struct AzOptionLayoutSizeVariant_None { AzOptionLayoutSizeTag tag; };
typedef struct AzOptionLayoutSizeVariant_None AzOptionLayoutSizeVariant_None;
struct AzOptionLayoutSizeVariant_Some { AzOptionLayoutSizeTag tag; AzLayoutSize payload; };
typedef struct AzOptionLayoutSizeVariant_Some AzOptionLayoutSizeVariant_Some;
union AzOptionLayoutSize {
    AzOptionLayoutSizeVariant_None None;
    AzOptionLayoutSizeVariant_Some Some;
};
typedef union AzOptionLayoutSize AzOptionLayoutSize;
#define AzOptionLayoutSize_None { .None = { .tag = AzOptionLayoutSizeTag_None } }
#define AzOptionLayoutSize_Some(v) { .Some = { .tag = AzOptionLayoutSizeTag_Some, .payload = v } }

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
#define AzOptionWindowTheme_None { .None = { .tag = AzOptionWindowThemeTag_None } }
#define AzOptionWindowTheme_Some(v) { .Some = { .tag = AzOptionWindowThemeTag_Some, .payload = v } }

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
#define AzOptionNodeId_None { .None = { .tag = AzOptionNodeIdTag_None } }
#define AzOptionNodeId_Some(v) { .Some = { .tag = AzOptionNodeIdTag_Some, .payload = v } }

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
#define AzOptionDomNodeId_None { .None = { .tag = AzOptionDomNodeIdTag_None } }
#define AzOptionDomNodeId_Some(v) { .Some = { .tag = AzOptionDomNodeIdTag_Some, .payload = v } }

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
#define AzOptionColorU_None { .None = { .tag = AzOptionColorUTag_None } }
#define AzOptionColorU_Some(v) { .Some = { .tag = AzOptionColorUTag_Some, .payload = v } }

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
#define AzOptionSvgDashPattern_None { .None = { .tag = AzOptionSvgDashPatternTag_None } }
#define AzOptionSvgDashPattern_Some(v) { .Some = { .tag = AzOptionSvgDashPatternTag_Some, .payload = v } }

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
#define AzOptionLogicalPosition_None { .None = { .tag = AzOptionLogicalPositionTag_None } }
#define AzOptionLogicalPosition_Some(v) { .Some = { .tag = AzOptionLogicalPositionTag_Some, .payload = v } }

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
#define AzOptionPhysicalPositionI32_None { .None = { .tag = AzOptionPhysicalPositionI32Tag_None } }
#define AzOptionPhysicalPositionI32_Some(v) { .Some = { .tag = AzOptionPhysicalPositionI32Tag_Some, .payload = v } }

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
#define AzOptionMouseCursorType_None { .None = { .tag = AzOptionMouseCursorTypeTag_None } }
#define AzOptionMouseCursorType_Some(v) { .Some = { .tag = AzOptionMouseCursorTypeTag_Some, .payload = v } }

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
#define AzOptionLogicalSize_None { .None = { .tag = AzOptionLogicalSizeTag_None } }
#define AzOptionLogicalSize_Some(v) { .Some = { .tag = AzOptionLogicalSizeTag_Some, .payload = v } }

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
#define AzOptionVirtualKeyCode_None { .None = { .tag = AzOptionVirtualKeyCodeTag_None } }
#define AzOptionVirtualKeyCode_Some(v) { .Some = { .tag = AzOptionVirtualKeyCodeTag_Some, .payload = v } }

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
#define AzOptionImageMask_None { .None = { .tag = AzOptionImageMaskTag_None } }
#define AzOptionImageMask_Some(v) { .Some = { .tag = AzOptionImageMaskTag_Some, .payload = v } }

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
#define AzOptionTabIndex_None { .None = { .tag = AzOptionTabIndexTag_None } }
#define AzOptionTabIndex_Some(v) { .Some = { .tag = AzOptionTabIndexTag_Some, .payload = v } }

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
#define AzOptionTagId_None { .None = { .tag = AzOptionTagIdTag_None } }
#define AzOptionTagId_Some(v) { .Some = { .tag = AzOptionTagIdTag_Some, .payload = v } }

enum AzOptionU8VecTag {
   AzOptionU8VecTag_None,
   AzOptionU8VecTag_Some,
};
typedef enum AzOptionU8VecTag AzOptionU8VecTag;

struct AzOptionU8VecVariant_None { AzOptionU8VecTag tag; };
typedef struct AzOptionU8VecVariant_None AzOptionU8VecVariant_None;
struct AzOptionU8VecVariant_Some { AzOptionU8VecTag tag; AzU8Vec payload; };
typedef struct AzOptionU8VecVariant_Some AzOptionU8VecVariant_Some;
union AzOptionU8Vec {
    AzOptionU8VecVariant_None None;
    AzOptionU8VecVariant_Some Some;
};
typedef union AzOptionU8Vec AzOptionU8Vec;
#define AzOptionU8Vec_None { .None = { .tag = AzOptionU8VecTag_None } }
#define AzOptionU8Vec_Some(v) { .Some = { .tag = AzOptionU8VecTag_Some, .payload = v } }

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
#define AzOptionU8VecRef_None { .None = { .tag = AzOptionU8VecRefTag_None } }
#define AzOptionU8VecRef_Some(v) { .Some = { .tag = AzOptionU8VecRefTag_Some, .payload = v } }

enum AzResultU8VecEncodeImageErrorTag {
   AzResultU8VecEncodeImageErrorTag_Ok,
   AzResultU8VecEncodeImageErrorTag_Err,
};
typedef enum AzResultU8VecEncodeImageErrorTag AzResultU8VecEncodeImageErrorTag;

struct AzResultU8VecEncodeImageErrorVariant_Ok { AzResultU8VecEncodeImageErrorTag tag; AzU8Vec payload; };
typedef struct AzResultU8VecEncodeImageErrorVariant_Ok AzResultU8VecEncodeImageErrorVariant_Ok;
struct AzResultU8VecEncodeImageErrorVariant_Err { AzResultU8VecEncodeImageErrorTag tag; AzEncodeImageError payload; };
typedef struct AzResultU8VecEncodeImageErrorVariant_Err AzResultU8VecEncodeImageErrorVariant_Err;
union AzResultU8VecEncodeImageError {
    AzResultU8VecEncodeImageErrorVariant_Ok Ok;
    AzResultU8VecEncodeImageErrorVariant_Err Err;
};
typedef union AzResultU8VecEncodeImageError AzResultU8VecEncodeImageError;
#define AzResultU8VecEncodeImageError_Ok(v) { .Ok = { .tag = AzResultU8VecEncodeImageErrorTag_Ok, .payload = v } }
#define AzResultU8VecEncodeImageError_Err(v) { .Err = { .tag = AzResultU8VecEncodeImageErrorTag_Err, .payload = v } }

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
    void* ptr;
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
#define AzDuration_System(v) { .System = { .tag = AzDurationTag_System, .payload = v } }
#define AzDuration_Tick(v) { .Tick = { .tag = AzDurationTag_Tick, .payload = v } }

struct AzAppConfig {
    AzLayoutSolverVersion layout_solver;
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
#define AzWindowIcon_Small(v) { .Small = { .tag = AzWindowIconTag_Small, .payload = v } }
#define AzWindowIcon_Large(v) { .Large = { .tag = AzWindowIconTag_Large, .payload = v } }

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

struct AzInlineTextContents {
    AzInlineGlyphVec glyphs;
    AzLogicalRect bounds;
};
typedef struct AzInlineTextContents AzInlineTextContents;

struct AzGlCallbackInfo {
    AzDomNodeId callback_node_id;
    AzHidpiAdjustedBounds bounds;
    AzOptionGl* gl_context;
    void* resources;
    AzNodeVec* node_hierarchy;
    void* words_cache;
    void* shaped_words_cache;
    void* positioned_words_cache;
    void* positioned_rects;
};
typedef struct AzGlCallbackInfo AzGlCallbackInfo;

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
#define AzEventFilter_Hover(v) { .Hover = { .tag = AzEventFilterTag_Hover, .payload = v } }
#define AzEventFilter_Not(v) { .Not = { .tag = AzEventFilterTag_Not, .payload = v } }
#define AzEventFilter_Focus(v) { .Focus = { .tag = AzEventFilterTag_Focus, .payload = v } }
#define AzEventFilter_Window(v) { .Window = { .tag = AzEventFilterTag_Window, .payload = v } }
#define AzEventFilter_Component(v) { .Component = { .tag = AzEventFilterTag_Component, .payload = v } }
#define AzEventFilter_Application(v) { .Application = { .tag = AzEventFilterTag_Application, .payload = v } }

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
#define AzCssPathPseudoSelector_First { .First = { .tag = AzCssPathPseudoSelectorTag_First } }
#define AzCssPathPseudoSelector_Last { .Last = { .tag = AzCssPathPseudoSelectorTag_Last } }
#define AzCssPathPseudoSelector_NthChild(v) { .NthChild = { .tag = AzCssPathPseudoSelectorTag_NthChild, .payload = v } }
#define AzCssPathPseudoSelector_Hover { .Hover = { .tag = AzCssPathPseudoSelectorTag_Hover } }
#define AzCssPathPseudoSelector_Active { .Active = { .tag = AzCssPathPseudoSelectorTag_Active } }
#define AzCssPathPseudoSelector_Focus { .Focus = { .tag = AzCssPathPseudoSelectorTag_Focus } }

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
struct AzStyleTransformVariant_Rotate { AzStyleTransformTag tag; AzAngleValue payload; };
typedef struct AzStyleTransformVariant_Rotate AzStyleTransformVariant_Rotate;
struct AzStyleTransformVariant_Rotate3D { AzStyleTransformTag tag; AzStyleTransformRotate3D payload; };
typedef struct AzStyleTransformVariant_Rotate3D AzStyleTransformVariant_Rotate3D;
struct AzStyleTransformVariant_RotateX { AzStyleTransformTag tag; AzAngleValue payload; };
typedef struct AzStyleTransformVariant_RotateX AzStyleTransformVariant_RotateX;
struct AzStyleTransformVariant_RotateY { AzStyleTransformTag tag; AzAngleValue payload; };
typedef struct AzStyleTransformVariant_RotateY AzStyleTransformVariant_RotateY;
struct AzStyleTransformVariant_RotateZ { AzStyleTransformTag tag; AzAngleValue payload; };
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
#define AzStyleTransform_Matrix(v) { .Matrix = { .tag = AzStyleTransformTag_Matrix, .payload = v } }
#define AzStyleTransform_Matrix3D(v) { .Matrix3D = { .tag = AzStyleTransformTag_Matrix3D, .payload = v } }
#define AzStyleTransform_Translate(v) { .Translate = { .tag = AzStyleTransformTag_Translate, .payload = v } }
#define AzStyleTransform_Translate3D(v) { .Translate3D = { .tag = AzStyleTransformTag_Translate3D, .payload = v } }
#define AzStyleTransform_TranslateX(v) { .TranslateX = { .tag = AzStyleTransformTag_TranslateX, .payload = v } }
#define AzStyleTransform_TranslateY(v) { .TranslateY = { .tag = AzStyleTransformTag_TranslateY, .payload = v } }
#define AzStyleTransform_TranslateZ(v) { .TranslateZ = { .tag = AzStyleTransformTag_TranslateZ, .payload = v } }
#define AzStyleTransform_Rotate(v) { .Rotate = { .tag = AzStyleTransformTag_Rotate, .payload = v } }
#define AzStyleTransform_Rotate3D(v) { .Rotate3D = { .tag = AzStyleTransformTag_Rotate3D, .payload = v } }
#define AzStyleTransform_RotateX(v) { .RotateX = { .tag = AzStyleTransformTag_RotateX, .payload = v } }
#define AzStyleTransform_RotateY(v) { .RotateY = { .tag = AzStyleTransformTag_RotateY, .payload = v } }
#define AzStyleTransform_RotateZ(v) { .RotateZ = { .tag = AzStyleTransformTag_RotateZ, .payload = v } }
#define AzStyleTransform_Scale(v) { .Scale = { .tag = AzStyleTransformTag_Scale, .payload = v } }
#define AzStyleTransform_Scale3D(v) { .Scale3D = { .tag = AzStyleTransformTag_Scale3D, .payload = v } }
#define AzStyleTransform_ScaleX(v) { .ScaleX = { .tag = AzStyleTransformTag_ScaleX, .payload = v } }
#define AzStyleTransform_ScaleY(v) { .ScaleY = { .tag = AzStyleTransformTag_ScaleY, .payload = v } }
#define AzStyleTransform_ScaleZ(v) { .ScaleZ = { .tag = AzStyleTransformTag_ScaleZ, .payload = v } }
#define AzStyleTransform_Skew(v) { .Skew = { .tag = AzStyleTransformTag_Skew, .payload = v } }
#define AzStyleTransform_SkewX(v) { .SkewX = { .tag = AzStyleTransformTag_SkewX, .payload = v } }
#define AzStyleTransform_SkewY(v) { .SkewY = { .tag = AzStyleTransformTag_SkewY, .payload = v } }
#define AzStyleTransform_Perspective(v) { .Perspective = { .tag = AzStyleTransformTag_Perspective, .payload = v } }

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
#define AzStyleBackgroundPositionVecValue_Auto { .Auto = { .tag = AzStyleBackgroundPositionVecValueTag_Auto } }
#define AzStyleBackgroundPositionVecValue_None { .None = { .tag = AzStyleBackgroundPositionVecValueTag_None } }
#define AzStyleBackgroundPositionVecValue_Inherit { .Inherit = { .tag = AzStyleBackgroundPositionVecValueTag_Inherit } }
#define AzStyleBackgroundPositionVecValue_Initial { .Initial = { .tag = AzStyleBackgroundPositionVecValueTag_Initial } }
#define AzStyleBackgroundPositionVecValue_Exact(v) { .Exact = { .tag = AzStyleBackgroundPositionVecValueTag_Exact, .payload = v } }

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
#define AzStyleBackgroundRepeatVecValue_Auto { .Auto = { .tag = AzStyleBackgroundRepeatVecValueTag_Auto } }
#define AzStyleBackgroundRepeatVecValue_None { .None = { .tag = AzStyleBackgroundRepeatVecValueTag_None } }
#define AzStyleBackgroundRepeatVecValue_Inherit { .Inherit = { .tag = AzStyleBackgroundRepeatVecValueTag_Inherit } }
#define AzStyleBackgroundRepeatVecValue_Initial { .Initial = { .tag = AzStyleBackgroundRepeatVecValueTag_Initial } }
#define AzStyleBackgroundRepeatVecValue_Exact(v) { .Exact = { .tag = AzStyleBackgroundRepeatVecValueTag_Exact, .payload = v } }

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
#define AzStyleBackgroundSizeVecValue_Auto { .Auto = { .tag = AzStyleBackgroundSizeVecValueTag_Auto } }
#define AzStyleBackgroundSizeVecValue_None { .None = { .tag = AzStyleBackgroundSizeVecValueTag_None } }
#define AzStyleBackgroundSizeVecValue_Inherit { .Inherit = { .tag = AzStyleBackgroundSizeVecValueTag_Inherit } }
#define AzStyleBackgroundSizeVecValue_Initial { .Initial = { .tag = AzStyleBackgroundSizeVecValueTag_Initial } }
#define AzStyleBackgroundSizeVecValue_Exact(v) { .Exact = { .tag = AzStyleBackgroundSizeVecValueTag_Exact, .payload = v } }

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

struct AzTexture {
    uint32_t texture_id;
    AzRawImageFormat format;
    AzTextureFlags flags;
    AzPhysicalSizeU32 size;
    AzGl gl_context;
};
typedef struct AzTexture AzTexture;

struct AzGetProgramBinaryReturn {
    AzU8Vec _0;
    uint32_t _1;
};
typedef struct AzGetProgramBinaryReturn AzGetProgramBinaryReturn;

enum AzRawImageDataTag {
   AzRawImageDataTag_U8,
   AzRawImageDataTag_U16,
   AzRawImageDataTag_F32,
};
typedef enum AzRawImageDataTag AzRawImageDataTag;

struct AzRawImageDataVariant_U8 { AzRawImageDataTag tag; AzU8Vec payload; };
typedef struct AzRawImageDataVariant_U8 AzRawImageDataVariant_U8;
struct AzRawImageDataVariant_U16 { AzRawImageDataTag tag; AzU16Vec payload; };
typedef struct AzRawImageDataVariant_U16 AzRawImageDataVariant_U16;
struct AzRawImageDataVariant_F32 { AzRawImageDataTag tag; AzF32Vec payload; };
typedef struct AzRawImageDataVariant_F32 AzRawImageDataVariant_F32;
union AzRawImageData {
    AzRawImageDataVariant_U8 U8;
    AzRawImageDataVariant_U16 U16;
    AzRawImageDataVariant_F32 F32;
};
typedef union AzRawImageData AzRawImageData;
#define AzRawImageData_U8(v) { .U8 = { .tag = AzRawImageDataTag_U8, .payload = v } }
#define AzRawImageData_U16(v) { .U16 = { .tag = AzRawImageDataTag_U16, .payload = v } }
#define AzRawImageData_F32(v) { .F32 = { .tag = AzRawImageDataTag_F32, .payload = v } }

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
#define AzSvgPathElement_Line(v) { .Line = { .tag = AzSvgPathElementTag_Line, .payload = v } }
#define AzSvgPathElement_QuadraticCurve(v) { .QuadraticCurve = { .tag = AzSvgPathElementTag_QuadraticCurve, .payload = v } }
#define AzSvgPathElement_CubicCurve(v) { .CubicCurve = { .tag = AzSvgPathElementTag_CubicCurve, .payload = v } }

struct AzTesselatedSvgNode {
    AzSvgVertexVec vertices;
    AzU32Vec indices;
};
typedef struct AzTesselatedSvgNode AzTesselatedSvgNode;

struct AzTesselatedSvgNodeVecRef {
    AzTesselatedSvgNode* ptr;
    size_t len;
};
typedef struct AzTesselatedSvgNodeVecRef AzTesselatedSvgNodeVecRef;

struct AzSvgRenderOptions {
    AzOptionLayoutSize target_size;
    AzOptionColorU background_color;
    AzSvgFitTo fit;
};
typedef struct AzSvgRenderOptions AzSvgRenderOptions;

struct AzSvgStrokeStyle {
    AzSvgLineCap start_cap;
    AzSvgLineCap end_cap;
    AzSvgLineJoin line_join;
    AzOptionSvgDashPattern dash_pattern;
    float line_width;
    float miter_limit;
    float tolerance;
    bool  apply_line_width;
    AzSvgTransform transform;
    bool  anti_alias;
    bool  high_quality_aa;
};
typedef struct AzSvgStrokeStyle AzSvgStrokeStyle;

struct AzXml {
    AzXmlNodeVec root;
};
typedef struct AzXml AzXml;

struct AzString {
    AzU8Vec vec;
};
typedef struct AzString AzString;

struct AzTesselatedSvgNodeVec {
    AzTesselatedSvgNode* ptr;
    size_t len;
    size_t cap;
    AzTesselatedSvgNodeVecDestructor destructor;
};
typedef struct AzTesselatedSvgNodeVec AzTesselatedSvgNodeVec;

struct AzStyleTransformVec {
    AzStyleTransform* ptr;
    size_t len;
    size_t cap;
    AzStyleTransformVecDestructor destructor;
};
typedef struct AzStyleTransformVec AzStyleTransformVec;

struct AzSvgPathElementVec {
    AzSvgPathElement* ptr;
    size_t len;
    size_t cap;
    AzSvgPathElementVecDestructor destructor;
};
typedef struct AzSvgPathElementVec AzSvgPathElementVec;

struct AzStringVec {
    AzString* ptr;
    size_t len;
    size_t cap;
    AzStringVecDestructor destructor;
};
typedef struct AzStringVec AzStringVec;

struct AzLinearColorStopVec {
    AzLinearColorStop* ptr;
    size_t len;
    size_t cap;
    AzLinearColorStopVecDestructor destructor;
};
typedef struct AzLinearColorStopVec AzLinearColorStopVec;

struct AzRadialColorStopVec {
    AzRadialColorStop* ptr;
    size_t len;
    size_t cap;
    AzRadialColorStopVecDestructor destructor;
};
typedef struct AzRadialColorStopVec AzRadialColorStopVec;

struct AzStyledNodeVec {
    AzStyledNode* ptr;
    size_t len;
    size_t cap;
    AzStyledNodeVecDestructor destructor;
};
typedef struct AzStyledNodeVec AzStyledNodeVec;

struct AzTagIdsToNodeIdsMappingVec {
    AzTagIdToNodeIdMapping* ptr;
    size_t len;
    size_t cap;
    AzTagIdsToNodeIdsMappingVecDestructor destructor;
};
typedef struct AzTagIdsToNodeIdsMappingVec AzTagIdsToNodeIdsMappingVec;

enum AzOptionStringVecTag {
   AzOptionStringVecTag_None,
   AzOptionStringVecTag_Some,
};
typedef enum AzOptionStringVecTag AzOptionStringVecTag;

struct AzOptionStringVecVariant_None { AzOptionStringVecTag tag; };
typedef struct AzOptionStringVecVariant_None AzOptionStringVecVariant_None;
struct AzOptionStringVecVariant_Some { AzOptionStringVecTag tag; AzStringVec payload; };
typedef struct AzOptionStringVecVariant_Some AzOptionStringVecVariant_Some;
union AzOptionStringVec {
    AzOptionStringVecVariant_None None;
    AzOptionStringVecVariant_Some Some;
};
typedef union AzOptionStringVec AzOptionStringVec;
#define AzOptionStringVec_None { .None = { .tag = AzOptionStringVecTag_None } }
#define AzOptionStringVec_Some(v) { .Some = { .tag = AzOptionStringVecTag_Some, .payload = v } }

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
#define AzOptionTaskBarIcon_None { .None = { .tag = AzOptionTaskBarIconTag_None } }
#define AzOptionTaskBarIcon_Some(v) { .Some = { .tag = AzOptionTaskBarIconTag_Some, .payload = v } }

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
#define AzOptionWindowIcon_None { .None = { .tag = AzOptionWindowIconTag_None } }
#define AzOptionWindowIcon_Some(v) { .Some = { .tag = AzOptionWindowIconTag_Some, .payload = v } }

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
#define AzOptionString_None { .None = { .tag = AzOptionStringTag_None } }
#define AzOptionString_Some(v) { .Some = { .tag = AzOptionStringTag_Some, .payload = v } }

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
#define AzOptionTexture_None { .None = { .tag = AzOptionTextureTag_None } }
#define AzOptionTexture_Some(v) { .Some = { .tag = AzOptionTextureTag_Some, .payload = v } }

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
#define AzOptionDuration_None { .None = { .tag = AzOptionDurationTag_None } }
#define AzOptionDuration_Some(v) { .Some = { .tag = AzOptionDurationTag_Some, .payload = v } }

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
#define AzInstant_System(v) { .System = { .tag = AzInstantTag_System, .payload = v } }
#define AzInstant_Tick(v) { .Tick = { .tag = AzInstantTag_Tick, .payload = v } }

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
    size_t id;
    AzOptionString name;
    AzLayoutSize size;
    AzLayoutPoint position;
    double scale_factor;
    AzVideoModeVec video_modes;
    bool  is_primary_monitor;
};
typedef struct AzMonitor AzMonitor;

enum AzInlineWordTag {
   AzInlineWordTag_Tab,
   AzInlineWordTag_Return,
   AzInlineWordTag_Space,
   AzInlineWordTag_Word,
};
typedef enum AzInlineWordTag AzInlineWordTag;

struct AzInlineWordVariant_Tab { AzInlineWordTag tag; };
typedef struct AzInlineWordVariant_Tab AzInlineWordVariant_Tab;
struct AzInlineWordVariant_Return { AzInlineWordTag tag; };
typedef struct AzInlineWordVariant_Return AzInlineWordVariant_Return;
struct AzInlineWordVariant_Space { AzInlineWordTag tag; };
typedef struct AzInlineWordVariant_Space AzInlineWordVariant_Space;
struct AzInlineWordVariant_Word { AzInlineWordTag tag; AzInlineTextContents payload; };
typedef struct AzInlineWordVariant_Word AzInlineWordVariant_Word;
union AzInlineWord {
    AzInlineWordVariant_Tab Tab;
    AzInlineWordVariant_Return Return;
    AzInlineWordVariant_Space Space;
    AzInlineWordVariant_Word Word;
};
typedef union AzInlineWord AzInlineWord;
#define AzInlineWord_Tab { .Tab = { .tag = AzInlineWordTag_Tab } }
#define AzInlineWord_Return { .Return = { .tag = AzInlineWordTag_Return } }
#define AzInlineWord_Space { .Space = { .tag = AzInlineWordTag_Space } }
#define AzInlineWord_Word(v) { .Word = { .tag = AzInlineWordTag_Word, .payload = v } }

struct AzGlCallbackReturn {
    AzOptionTexture texture;
};
typedef struct AzGlCallbackReturn AzGlCallbackReturn;

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
    AzRefCountInner* ptr;
};
typedef struct AzRefCount AzRefCount;

struct AzRefAny {
    void* _internal_ptr;
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
#define AzNodeType_Div { .Div = { .tag = AzNodeTypeTag_Div } }
#define AzNodeType_Body { .Body = { .tag = AzNodeTypeTag_Body } }
#define AzNodeType_Br { .Br = { .tag = AzNodeTypeTag_Br } }
#define AzNodeType_Label(v) { .Label = { .tag = AzNodeTypeTag_Label, .payload = v } }
#define AzNodeType_Image(v) { .Image = { .tag = AzNodeTypeTag_Image, .payload = v } }
#define AzNodeType_IFrame(v) { .IFrame = { .tag = AzNodeTypeTag_IFrame, .payload = v } }
#define AzNodeType_GlTexture(v) { .GlTexture = { .tag = AzNodeTypeTag_GlTexture, .payload = v } }

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
#define AzIdOrClass_Id(v) { .Id = { .tag = AzIdOrClassTag_Id, .payload = v } }
#define AzIdOrClass_Class(v) { .Class = { .tag = AzIdOrClassTag_Class, .payload = v } }

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
struct AzCssPathSelectorVariant_Type { AzCssPathSelectorTag tag; AzNodeTypeKey payload; };
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
#define AzCssPathSelector_Global { .Global = { .tag = AzCssPathSelectorTag_Global } }
#define AzCssPathSelector_Type(v) { .Type = { .tag = AzCssPathSelectorTag_Type, .payload = v } }
#define AzCssPathSelector_Class(v) { .Class = { .tag = AzCssPathSelectorTag_Class, .payload = v } }
#define AzCssPathSelector_Id(v) { .Id = { .tag = AzCssPathSelectorTag_Id, .payload = v } }
#define AzCssPathSelector_PseudoSelector(v) { .PseudoSelector = { .tag = AzCssPathSelectorTag_PseudoSelector, .payload = v } }
#define AzCssPathSelector_DirectChildren { .DirectChildren = { .tag = AzCssPathSelectorTag_DirectChildren } }
#define AzCssPathSelector_Children { .Children = { .tag = AzCssPathSelectorTag_Children } }

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
#define AzStyleBackgroundContent_LinearGradient(v) { .LinearGradient = { .tag = AzStyleBackgroundContentTag_LinearGradient, .payload = v } }
#define AzStyleBackgroundContent_RadialGradient(v) { .RadialGradient = { .tag = AzStyleBackgroundContentTag_RadialGradient, .payload = v } }
#define AzStyleBackgroundContent_ConicGradient(v) { .ConicGradient = { .tag = AzStyleBackgroundContentTag_ConicGradient, .payload = v } }
#define AzStyleBackgroundContent_Image(v) { .Image = { .tag = AzStyleBackgroundContentTag_Image, .payload = v } }
#define AzStyleBackgroundContent_Color(v) { .Color = { .tag = AzStyleBackgroundContentTag_Color, .payload = v } }

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
#define AzScrollbarStyleValue_Auto { .Auto = { .tag = AzScrollbarStyleValueTag_Auto } }
#define AzScrollbarStyleValue_None { .None = { .tag = AzScrollbarStyleValueTag_None } }
#define AzScrollbarStyleValue_Inherit { .Inherit = { .tag = AzScrollbarStyleValueTag_Inherit } }
#define AzScrollbarStyleValue_Initial { .Initial = { .tag = AzScrollbarStyleValueTag_Initial } }
#define AzScrollbarStyleValue_Exact(v) { .Exact = { .tag = AzScrollbarStyleValueTag_Exact, .payload = v } }

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
#define AzStyleFontFamilyValue_Auto { .Auto = { .tag = AzStyleFontFamilyValueTag_Auto } }
#define AzStyleFontFamilyValue_None { .None = { .tag = AzStyleFontFamilyValueTag_None } }
#define AzStyleFontFamilyValue_Inherit { .Inherit = { .tag = AzStyleFontFamilyValueTag_Inherit } }
#define AzStyleFontFamilyValue_Initial { .Initial = { .tag = AzStyleFontFamilyValueTag_Initial } }
#define AzStyleFontFamilyValue_Exact(v) { .Exact = { .tag = AzStyleFontFamilyValueTag_Exact, .payload = v } }

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
#define AzStyleTransformVecValue_Auto { .Auto = { .tag = AzStyleTransformVecValueTag_Auto } }
#define AzStyleTransformVecValue_None { .None = { .tag = AzStyleTransformVecValueTag_None } }
#define AzStyleTransformVecValue_Inherit { .Inherit = { .tag = AzStyleTransformVecValueTag_Inherit } }
#define AzStyleTransformVecValue_Initial { .Initial = { .tag = AzStyleTransformVecValueTag_Initial } }
#define AzStyleTransformVecValue_Exact(v) { .Exact = { .tag = AzStyleTransformVecValueTag_Exact, .payload = v } }

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

struct AzRawImage {
    AzRawImageData pixels;
    size_t width;
    size_t height;
    bool  alpha_premultiplied;
    AzRawImageFormat data_format;
};
typedef struct AzRawImage AzRawImage;

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
#define AzImageSource_Embedded(v) { .Embedded = { .tag = AzImageSourceTag_Embedded, .payload = v } }
#define AzImageSource_File(v) { .File = { .tag = AzImageSourceTag_File, .payload = v } }
#define AzImageSource_Raw(v) { .Raw = { .tag = AzImageSourceTag_Raw, .payload = v } }

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
#define AzSvgStyle_Fill(v) { .Fill = { .tag = AzSvgStyleTag_Fill, .payload = v } }
#define AzSvgStyle_Stroke(v) { .Stroke = { .tag = AzSvgStyleTag_Stroke, .payload = v } }

struct AzFileTypeList {
    AzStringVec document_types;
    AzString document_descriptor;
};
typedef struct AzFileTypeList AzFileTypeList;

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

enum AzFmtValueTag {
   AzFmtValueTag_Bool,
   AzFmtValueTag_Uchar,
   AzFmtValueTag_Schar,
   AzFmtValueTag_Ushort,
   AzFmtValueTag_Sshort,
   AzFmtValueTag_Uint,
   AzFmtValueTag_Sint,
   AzFmtValueTag_Ulong,
   AzFmtValueTag_Slong,
   AzFmtValueTag_Isize,
   AzFmtValueTag_Usize,
   AzFmtValueTag_Float,
   AzFmtValueTag_Double,
   AzFmtValueTag_Str,
   AzFmtValueTag_StrVec,
};
typedef enum AzFmtValueTag AzFmtValueTag;

struct AzFmtValueVariant_Bool { AzFmtValueTag tag; bool payload; };
typedef struct AzFmtValueVariant_Bool AzFmtValueVariant_Bool;
struct AzFmtValueVariant_Uchar { AzFmtValueTag tag; uint8_t payload; };
typedef struct AzFmtValueVariant_Uchar AzFmtValueVariant_Uchar;
struct AzFmtValueVariant_Schar { AzFmtValueTag tag; int8_t payload; };
typedef struct AzFmtValueVariant_Schar AzFmtValueVariant_Schar;
struct AzFmtValueVariant_Ushort { AzFmtValueTag tag; uint16_t payload; };
typedef struct AzFmtValueVariant_Ushort AzFmtValueVariant_Ushort;
struct AzFmtValueVariant_Sshort { AzFmtValueTag tag; int16_t payload; };
typedef struct AzFmtValueVariant_Sshort AzFmtValueVariant_Sshort;
struct AzFmtValueVariant_Uint { AzFmtValueTag tag; uint32_t payload; };
typedef struct AzFmtValueVariant_Uint AzFmtValueVariant_Uint;
struct AzFmtValueVariant_Sint { AzFmtValueTag tag; int32_t payload; };
typedef struct AzFmtValueVariant_Sint AzFmtValueVariant_Sint;
struct AzFmtValueVariant_Ulong { AzFmtValueTag tag; uint64_t payload; };
typedef struct AzFmtValueVariant_Ulong AzFmtValueVariant_Ulong;
struct AzFmtValueVariant_Slong { AzFmtValueTag tag; int64_t payload; };
typedef struct AzFmtValueVariant_Slong AzFmtValueVariant_Slong;
struct AzFmtValueVariant_Isize { AzFmtValueTag tag; ssize_t payload; };
typedef struct AzFmtValueVariant_Isize AzFmtValueVariant_Isize;
struct AzFmtValueVariant_Usize { AzFmtValueTag tag; size_t payload; };
typedef struct AzFmtValueVariant_Usize AzFmtValueVariant_Usize;
struct AzFmtValueVariant_Float { AzFmtValueTag tag; float payload; };
typedef struct AzFmtValueVariant_Float AzFmtValueVariant_Float;
struct AzFmtValueVariant_Double { AzFmtValueTag tag; double payload; };
typedef struct AzFmtValueVariant_Double AzFmtValueVariant_Double;
struct AzFmtValueVariant_Str { AzFmtValueTag tag; AzString payload; };
typedef struct AzFmtValueVariant_Str AzFmtValueVariant_Str;
struct AzFmtValueVariant_StrVec { AzFmtValueTag tag; AzStringVec payload; };
typedef struct AzFmtValueVariant_StrVec AzFmtValueVariant_StrVec;
union AzFmtValue {
    AzFmtValueVariant_Bool Bool;
    AzFmtValueVariant_Uchar Uchar;
    AzFmtValueVariant_Schar Schar;
    AzFmtValueVariant_Ushort Ushort;
    AzFmtValueVariant_Sshort Sshort;
    AzFmtValueVariant_Uint Uint;
    AzFmtValueVariant_Sint Sint;
    AzFmtValueVariant_Ulong Ulong;
    AzFmtValueVariant_Slong Slong;
    AzFmtValueVariant_Isize Isize;
    AzFmtValueVariant_Usize Usize;
    AzFmtValueVariant_Float Float;
    AzFmtValueVariant_Double Double;
    AzFmtValueVariant_Str Str;
    AzFmtValueVariant_StrVec StrVec;
};
typedef union AzFmtValue AzFmtValue;
#define AzFmtValue_Bool(v) { .Bool = { .tag = AzFmtValueTag_Bool, .payload = v } }
#define AzFmtValue_Uchar(v) { .Uchar = { .tag = AzFmtValueTag_Uchar, .payload = v } }
#define AzFmtValue_Schar(v) { .Schar = { .tag = AzFmtValueTag_Schar, .payload = v } }
#define AzFmtValue_Ushort(v) { .Ushort = { .tag = AzFmtValueTag_Ushort, .payload = v } }
#define AzFmtValue_Sshort(v) { .Sshort = { .tag = AzFmtValueTag_Sshort, .payload = v } }
#define AzFmtValue_Uint(v) { .Uint = { .tag = AzFmtValueTag_Uint, .payload = v } }
#define AzFmtValue_Sint(v) { .Sint = { .tag = AzFmtValueTag_Sint, .payload = v } }
#define AzFmtValue_Ulong(v) { .Ulong = { .tag = AzFmtValueTag_Ulong, .payload = v } }
#define AzFmtValue_Slong(v) { .Slong = { .tag = AzFmtValueTag_Slong, .payload = v } }
#define AzFmtValue_Isize(v) { .Isize = { .tag = AzFmtValueTag_Isize, .payload = v } }
#define AzFmtValue_Usize(v) { .Usize = { .tag = AzFmtValueTag_Usize, .payload = v } }
#define AzFmtValue_Float(v) { .Float = { .tag = AzFmtValueTag_Float, .payload = v } }
#define AzFmtValue_Double(v) { .Double = { .tag = AzFmtValueTag_Double, .payload = v } }
#define AzFmtValue_Str(v) { .Str = { .tag = AzFmtValueTag_Str, .payload = v } }
#define AzFmtValue_StrVec(v) { .StrVec = { .tag = AzFmtValueTag_StrVec, .payload = v } }

struct AzFmtArg {
    AzString key;
    AzFmtValue value;
};
typedef struct AzFmtArg AzFmtArg;

struct AzFmtArgVec {
    AzFmtArg* ptr;
    size_t len;
    size_t cap;
    AzFmtArgVecDestructor destructor;
};
typedef struct AzFmtArgVec AzFmtArgVec;

struct AzInlineWordVec {
    AzInlineWord* ptr;
    size_t len;
    size_t cap;
    AzInlineWordVecDestructor destructor;
};
typedef struct AzInlineWordVec AzInlineWordVec;

struct AzMonitorVec {
    AzMonitor* ptr;
    size_t len;
    size_t cap;
    AzMonitorVecDestructor destructor;
};
typedef struct AzMonitorVec AzMonitorVec;

struct AzIdOrClassVec {
    AzIdOrClass* ptr;
    size_t len;
    size_t cap;
    AzIdOrClassVecDestructor destructor;
};
typedef struct AzIdOrClassVec AzIdOrClassVec;

struct AzStyleBackgroundContentVec {
    AzStyleBackgroundContent* ptr;
    size_t len;
    size_t cap;
    AzStyleBackgroundContentVecDestructor destructor;
};
typedef struct AzStyleBackgroundContentVec AzStyleBackgroundContentVec;

struct AzSvgPathVec {
    AzSvgPath* ptr;
    size_t len;
    size_t cap;
    AzSvgPathVecDestructor destructor;
};
typedef struct AzSvgPathVec AzSvgPathVec;

struct AzVertexAttributeVec {
    AzVertexAttribute* ptr;
    size_t len;
    size_t cap;
    AzVertexAttributeVecDestructor destructor;
};
typedef struct AzVertexAttributeVec AzVertexAttributeVec;

struct AzCssPathSelectorVec {
    AzCssPathSelector* ptr;
    size_t len;
    size_t cap;
    AzCssPathSelectorVecDestructor destructor;
};
typedef struct AzCssPathSelectorVec AzCssPathSelectorVec;

struct AzCallbackDataVec {
    AzCallbackData* ptr;
    size_t len;
    size_t cap;
    AzCallbackDataVecDestructor destructor;
};
typedef struct AzCallbackDataVec AzCallbackDataVec;

struct AzDebugMessageVec {
    AzDebugMessage* ptr;
    size_t len;
    size_t cap;
    AzDebugMessageVecDestructor destructor;
};
typedef struct AzDebugMessageVec AzDebugMessageVec;

struct AzStringPairVec {
    AzStringPair* ptr;
    size_t len;
    size_t cap;
    AzStringPairVecDestructor destructor;
};
typedef struct AzStringPairVec AzStringPairVec;

enum AzOptionFileTypeListTag {
   AzOptionFileTypeListTag_None,
   AzOptionFileTypeListTag_Some,
};
typedef enum AzOptionFileTypeListTag AzOptionFileTypeListTag;

struct AzOptionFileTypeListVariant_None { AzOptionFileTypeListTag tag; };
typedef struct AzOptionFileTypeListVariant_None AzOptionFileTypeListVariant_None;
struct AzOptionFileTypeListVariant_Some { AzOptionFileTypeListTag tag; AzFileTypeList payload; };
typedef struct AzOptionFileTypeListVariant_Some AzOptionFileTypeListVariant_Some;
union AzOptionFileTypeList {
    AzOptionFileTypeListVariant_None None;
    AzOptionFileTypeListVariant_Some Some;
};
typedef union AzOptionFileTypeList AzOptionFileTypeList;
#define AzOptionFileTypeList_None { .None = { .tag = AzOptionFileTypeListTag_None } }
#define AzOptionFileTypeList_Some(v) { .Some = { .tag = AzOptionFileTypeListTag_Some, .payload = v } }

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
#define AzOptionRefAny_None { .None = { .tag = AzOptionRefAnyTag_None } }
#define AzOptionRefAny_Some(v) { .Some = { .tag = AzOptionRefAnyTag_Some, .payload = v } }

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
#define AzOptionRawImage_None { .None = { .tag = AzOptionRawImageTag_None } }
#define AzOptionRawImage_Some(v) { .Some = { .tag = AzOptionRawImageTag_Some, .payload = v } }

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
#define AzOptionWaylandTheme_None { .None = { .tag = AzOptionWaylandThemeTag_None } }
#define AzOptionWaylandTheme_Some(v) { .Some = { .tag = AzOptionWaylandThemeTag_Some, .payload = v } }

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
#define AzOptionInstant_None { .None = { .tag = AzOptionInstantTag_None } }
#define AzOptionInstant_Some(v) { .Some = { .tag = AzOptionInstantTag_Some, .payload = v } }

enum AzResultRawImageDecodeImageErrorTag {
   AzResultRawImageDecodeImageErrorTag_Ok,
   AzResultRawImageDecodeImageErrorTag_Err,
};
typedef enum AzResultRawImageDecodeImageErrorTag AzResultRawImageDecodeImageErrorTag;

struct AzResultRawImageDecodeImageErrorVariant_Ok { AzResultRawImageDecodeImageErrorTag tag; AzRawImage payload; };
typedef struct AzResultRawImageDecodeImageErrorVariant_Ok AzResultRawImageDecodeImageErrorVariant_Ok;
struct AzResultRawImageDecodeImageErrorVariant_Err { AzResultRawImageDecodeImageErrorTag tag; AzDecodeImageError payload; };
typedef struct AzResultRawImageDecodeImageErrorVariant_Err AzResultRawImageDecodeImageErrorVariant_Err;
union AzResultRawImageDecodeImageError {
    AzResultRawImageDecodeImageErrorVariant_Ok Ok;
    AzResultRawImageDecodeImageErrorVariant_Err Err;
};
typedef union AzResultRawImageDecodeImageError AzResultRawImageDecodeImageError;
#define AzResultRawImageDecodeImageError_Ok(v) { .Ok = { .tag = AzResultRawImageDecodeImageErrorTag_Ok, .payload = v } }
#define AzResultRawImageDecodeImageError_Err(v) { .Err = { .tag = AzResultRawImageDecodeImageErrorTag_Err, .payload = v } }

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
#define AzXmlStreamError_UnexpectedEndOfStream { .UnexpectedEndOfStream = { .tag = AzXmlStreamErrorTag_UnexpectedEndOfStream } }
#define AzXmlStreamError_InvalidName { .InvalidName = { .tag = AzXmlStreamErrorTag_InvalidName } }
#define AzXmlStreamError_NonXmlChar(v) { .NonXmlChar = { .tag = AzXmlStreamErrorTag_NonXmlChar, .payload = v } }
#define AzXmlStreamError_InvalidChar(v) { .InvalidChar = { .tag = AzXmlStreamErrorTag_InvalidChar, .payload = v } }
#define AzXmlStreamError_InvalidCharMultiple(v) { .InvalidCharMultiple = { .tag = AzXmlStreamErrorTag_InvalidCharMultiple, .payload = v } }
#define AzXmlStreamError_InvalidQuote(v) { .InvalidQuote = { .tag = AzXmlStreamErrorTag_InvalidQuote, .payload = v } }
#define AzXmlStreamError_InvalidSpace(v) { .InvalidSpace = { .tag = AzXmlStreamErrorTag_InvalidSpace, .payload = v } }
#define AzXmlStreamError_InvalidString(v) { .InvalidString = { .tag = AzXmlStreamErrorTag_InvalidString, .payload = v } }
#define AzXmlStreamError_InvalidReference { .InvalidReference = { .tag = AzXmlStreamErrorTag_InvalidReference } }
#define AzXmlStreamError_InvalidExternalID { .InvalidExternalID = { .tag = AzXmlStreamErrorTag_InvalidExternalID } }
#define AzXmlStreamError_InvalidCommentData { .InvalidCommentData = { .tag = AzXmlStreamErrorTag_InvalidCommentData } }
#define AzXmlStreamError_InvalidCommentEnd { .InvalidCommentEnd = { .tag = AzXmlStreamErrorTag_InvalidCommentEnd } }
#define AzXmlStreamError_InvalidCharacterData { .InvalidCharacterData = { .tag = AzXmlStreamErrorTag_InvalidCharacterData } }

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

struct AzInlineLine {
    AzInlineWordVec words;
    AzLogicalRect bounds;
};
typedef struct AzInlineLine AzInlineLine;

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
#define AzStyleBackgroundContentVecValue_Auto { .Auto = { .tag = AzStyleBackgroundContentVecValueTag_Auto } }
#define AzStyleBackgroundContentVecValue_None { .None = { .tag = AzStyleBackgroundContentVecValueTag_None } }
#define AzStyleBackgroundContentVecValue_Inherit { .Inherit = { .tag = AzStyleBackgroundContentVecValueTag_Inherit } }
#define AzStyleBackgroundContentVecValue_Initial { .Initial = { .tag = AzStyleBackgroundContentVecValueTag_Initial } }
#define AzStyleBackgroundContentVecValue_Exact(v) { .Exact = { .tag = AzStyleBackgroundContentVecValueTag_Exact, .payload = v } }

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
#define AzCssProperty_TextColor(v) { .TextColor = { .tag = AzCssPropertyTag_TextColor, .payload = v } }
#define AzCssProperty_FontSize(v) { .FontSize = { .tag = AzCssPropertyTag_FontSize, .payload = v } }
#define AzCssProperty_FontFamily(v) { .FontFamily = { .tag = AzCssPropertyTag_FontFamily, .payload = v } }
#define AzCssProperty_TextAlign(v) { .TextAlign = { .tag = AzCssPropertyTag_TextAlign, .payload = v } }
#define AzCssProperty_LetterSpacing(v) { .LetterSpacing = { .tag = AzCssPropertyTag_LetterSpacing, .payload = v } }
#define AzCssProperty_LineHeight(v) { .LineHeight = { .tag = AzCssPropertyTag_LineHeight, .payload = v } }
#define AzCssProperty_WordSpacing(v) { .WordSpacing = { .tag = AzCssPropertyTag_WordSpacing, .payload = v } }
#define AzCssProperty_TabWidth(v) { .TabWidth = { .tag = AzCssPropertyTag_TabWidth, .payload = v } }
#define AzCssProperty_Cursor(v) { .Cursor = { .tag = AzCssPropertyTag_Cursor, .payload = v } }
#define AzCssProperty_Display(v) { .Display = { .tag = AzCssPropertyTag_Display, .payload = v } }
#define AzCssProperty_Float(v) { .Float = { .tag = AzCssPropertyTag_Float, .payload = v } }
#define AzCssProperty_BoxSizing(v) { .BoxSizing = { .tag = AzCssPropertyTag_BoxSizing, .payload = v } }
#define AzCssProperty_Width(v) { .Width = { .tag = AzCssPropertyTag_Width, .payload = v } }
#define AzCssProperty_Height(v) { .Height = { .tag = AzCssPropertyTag_Height, .payload = v } }
#define AzCssProperty_MinWidth(v) { .MinWidth = { .tag = AzCssPropertyTag_MinWidth, .payload = v } }
#define AzCssProperty_MinHeight(v) { .MinHeight = { .tag = AzCssPropertyTag_MinHeight, .payload = v } }
#define AzCssProperty_MaxWidth(v) { .MaxWidth = { .tag = AzCssPropertyTag_MaxWidth, .payload = v } }
#define AzCssProperty_MaxHeight(v) { .MaxHeight = { .tag = AzCssPropertyTag_MaxHeight, .payload = v } }
#define AzCssProperty_Position(v) { .Position = { .tag = AzCssPropertyTag_Position, .payload = v } }
#define AzCssProperty_Top(v) { .Top = { .tag = AzCssPropertyTag_Top, .payload = v } }
#define AzCssProperty_Right(v) { .Right = { .tag = AzCssPropertyTag_Right, .payload = v } }
#define AzCssProperty_Left(v) { .Left = { .tag = AzCssPropertyTag_Left, .payload = v } }
#define AzCssProperty_Bottom(v) { .Bottom = { .tag = AzCssPropertyTag_Bottom, .payload = v } }
#define AzCssProperty_FlexWrap(v) { .FlexWrap = { .tag = AzCssPropertyTag_FlexWrap, .payload = v } }
#define AzCssProperty_FlexDirection(v) { .FlexDirection = { .tag = AzCssPropertyTag_FlexDirection, .payload = v } }
#define AzCssProperty_FlexGrow(v) { .FlexGrow = { .tag = AzCssPropertyTag_FlexGrow, .payload = v } }
#define AzCssProperty_FlexShrink(v) { .FlexShrink = { .tag = AzCssPropertyTag_FlexShrink, .payload = v } }
#define AzCssProperty_JustifyContent(v) { .JustifyContent = { .tag = AzCssPropertyTag_JustifyContent, .payload = v } }
#define AzCssProperty_AlignItems(v) { .AlignItems = { .tag = AzCssPropertyTag_AlignItems, .payload = v } }
#define AzCssProperty_AlignContent(v) { .AlignContent = { .tag = AzCssPropertyTag_AlignContent, .payload = v } }
#define AzCssProperty_BackgroundContent(v) { .BackgroundContent = { .tag = AzCssPropertyTag_BackgroundContent, .payload = v } }
#define AzCssProperty_BackgroundPosition(v) { .BackgroundPosition = { .tag = AzCssPropertyTag_BackgroundPosition, .payload = v } }
#define AzCssProperty_BackgroundSize(v) { .BackgroundSize = { .tag = AzCssPropertyTag_BackgroundSize, .payload = v } }
#define AzCssProperty_BackgroundRepeat(v) { .BackgroundRepeat = { .tag = AzCssPropertyTag_BackgroundRepeat, .payload = v } }
#define AzCssProperty_OverflowX(v) { .OverflowX = { .tag = AzCssPropertyTag_OverflowX, .payload = v } }
#define AzCssProperty_OverflowY(v) { .OverflowY = { .tag = AzCssPropertyTag_OverflowY, .payload = v } }
#define AzCssProperty_PaddingTop(v) { .PaddingTop = { .tag = AzCssPropertyTag_PaddingTop, .payload = v } }
#define AzCssProperty_PaddingLeft(v) { .PaddingLeft = { .tag = AzCssPropertyTag_PaddingLeft, .payload = v } }
#define AzCssProperty_PaddingRight(v) { .PaddingRight = { .tag = AzCssPropertyTag_PaddingRight, .payload = v } }
#define AzCssProperty_PaddingBottom(v) { .PaddingBottom = { .tag = AzCssPropertyTag_PaddingBottom, .payload = v } }
#define AzCssProperty_MarginTop(v) { .MarginTop = { .tag = AzCssPropertyTag_MarginTop, .payload = v } }
#define AzCssProperty_MarginLeft(v) { .MarginLeft = { .tag = AzCssPropertyTag_MarginLeft, .payload = v } }
#define AzCssProperty_MarginRight(v) { .MarginRight = { .tag = AzCssPropertyTag_MarginRight, .payload = v } }
#define AzCssProperty_MarginBottom(v) { .MarginBottom = { .tag = AzCssPropertyTag_MarginBottom, .payload = v } }
#define AzCssProperty_BorderTopLeftRadius(v) { .BorderTopLeftRadius = { .tag = AzCssPropertyTag_BorderTopLeftRadius, .payload = v } }
#define AzCssProperty_BorderTopRightRadius(v) { .BorderTopRightRadius = { .tag = AzCssPropertyTag_BorderTopRightRadius, .payload = v } }
#define AzCssProperty_BorderBottomLeftRadius(v) { .BorderBottomLeftRadius = { .tag = AzCssPropertyTag_BorderBottomLeftRadius, .payload = v } }
#define AzCssProperty_BorderBottomRightRadius(v) { .BorderBottomRightRadius = { .tag = AzCssPropertyTag_BorderBottomRightRadius, .payload = v } }
#define AzCssProperty_BorderTopColor(v) { .BorderTopColor = { .tag = AzCssPropertyTag_BorderTopColor, .payload = v } }
#define AzCssProperty_BorderRightColor(v) { .BorderRightColor = { .tag = AzCssPropertyTag_BorderRightColor, .payload = v } }
#define AzCssProperty_BorderLeftColor(v) { .BorderLeftColor = { .tag = AzCssPropertyTag_BorderLeftColor, .payload = v } }
#define AzCssProperty_BorderBottomColor(v) { .BorderBottomColor = { .tag = AzCssPropertyTag_BorderBottomColor, .payload = v } }
#define AzCssProperty_BorderTopStyle(v) { .BorderTopStyle = { .tag = AzCssPropertyTag_BorderTopStyle, .payload = v } }
#define AzCssProperty_BorderRightStyle(v) { .BorderRightStyle = { .tag = AzCssPropertyTag_BorderRightStyle, .payload = v } }
#define AzCssProperty_BorderLeftStyle(v) { .BorderLeftStyle = { .tag = AzCssPropertyTag_BorderLeftStyle, .payload = v } }
#define AzCssProperty_BorderBottomStyle(v) { .BorderBottomStyle = { .tag = AzCssPropertyTag_BorderBottomStyle, .payload = v } }
#define AzCssProperty_BorderTopWidth(v) { .BorderTopWidth = { .tag = AzCssPropertyTag_BorderTopWidth, .payload = v } }
#define AzCssProperty_BorderRightWidth(v) { .BorderRightWidth = { .tag = AzCssPropertyTag_BorderRightWidth, .payload = v } }
#define AzCssProperty_BorderLeftWidth(v) { .BorderLeftWidth = { .tag = AzCssPropertyTag_BorderLeftWidth, .payload = v } }
#define AzCssProperty_BorderBottomWidth(v) { .BorderBottomWidth = { .tag = AzCssPropertyTag_BorderBottomWidth, .payload = v } }
#define AzCssProperty_BoxShadowLeft(v) { .BoxShadowLeft = { .tag = AzCssPropertyTag_BoxShadowLeft, .payload = v } }
#define AzCssProperty_BoxShadowRight(v) { .BoxShadowRight = { .tag = AzCssPropertyTag_BoxShadowRight, .payload = v } }
#define AzCssProperty_BoxShadowTop(v) { .BoxShadowTop = { .tag = AzCssPropertyTag_BoxShadowTop, .payload = v } }
#define AzCssProperty_BoxShadowBottom(v) { .BoxShadowBottom = { .tag = AzCssPropertyTag_BoxShadowBottom, .payload = v } }
#define AzCssProperty_ScrollbarStyle(v) { .ScrollbarStyle = { .tag = AzCssPropertyTag_ScrollbarStyle, .payload = v } }
#define AzCssProperty_Opacity(v) { .Opacity = { .tag = AzCssPropertyTag_Opacity, .payload = v } }
#define AzCssProperty_Transform(v) { .Transform = { .tag = AzCssPropertyTag_Transform, .payload = v } }
#define AzCssProperty_TransformOrigin(v) { .TransformOrigin = { .tag = AzCssPropertyTag_TransformOrigin, .payload = v } }
#define AzCssProperty_PerspectiveOrigin(v) { .PerspectiveOrigin = { .tag = AzCssPropertyTag_PerspectiveOrigin, .payload = v } }
#define AzCssProperty_BackfaceVisibility(v) { .BackfaceVisibility = { .tag = AzCssPropertyTag_BackfaceVisibility, .payload = v } }

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
#define AzCssPropertySource_Css(v) { .Css = { .tag = AzCssPropertySourceTag_Css, .payload = v } }
#define AzCssPropertySource_Inline { .Inline = { .tag = AzCssPropertySourceTag_Inline } }

struct AzVertexLayout {
    AzVertexAttributeVec fields;
};
typedef struct AzVertexLayout AzVertexLayout;

struct AzVertexArrayObject {
    AzVertexLayout vertex_layout;
    uint32_t vao_id;
    AzGl gl_context;
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
#define AzFontSource_Embedded(v) { .Embedded = { .tag = AzFontSourceTag_Embedded, .payload = v } }
#define AzFontSource_File(v) { .File = { .tag = AzFontSourceTag_File, .payload = v } }
#define AzFontSource_System(v) { .System = { .tag = AzFontSourceTag_System, .payload = v } }

struct AzSvgMultiPolygon {
    AzSvgPathVec rings;
};
typedef struct AzSvgMultiPolygon AzSvgMultiPolygon;

struct AzXmlNode {
    AzString tag;
    AzStringPairVec attributes;
    AzXmlNodeVec children;
    AzOptionString text;
};
typedef struct AzXmlNode AzXmlNode;

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
#define AzThreadReceiveMsg_WriteBack(v) { .WriteBack = { .tag = AzThreadReceiveMsgTag_WriteBack, .payload = v } }
#define AzThreadReceiveMsg_Update(v) { .Update = { .tag = AzThreadReceiveMsgTag_Update, .payload = v } }

struct AzInlineLineVec {
    AzInlineLine* ptr;
    size_t len;
    size_t cap;
    AzInlineLineVecDestructor destructor;
};
typedef struct AzInlineLineVec AzInlineLineVec;

struct AzCssPropertyVec {
    AzCssProperty* ptr;
    size_t len;
    size_t cap;
    AzCssPropertyVecDestructor destructor;
};
typedef struct AzCssPropertyVec AzCssPropertyVec;

struct AzSvgMultiPolygonVec {
    AzSvgMultiPolygon* ptr;
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
#define AzOptionThreadReceiveMsg_None { .None = { .tag = AzOptionThreadReceiveMsgTag_None } }
#define AzOptionThreadReceiveMsg_Some(v) { .Some = { .tag = AzOptionThreadReceiveMsgTag_Some, .payload = v } }

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
    void* current_window_state;
    AzWindowState* restrict modifiable_window_state;
    AzOptionGl* gl_context;
    void* restrict resources;
    void* restrict timers;
    void* restrict threads;
    void* restrict new_windows;
    AzRawWindowHandle* current_window_handle;
    void* node_hierarchy;
    AzSystemCallbacks* system_callbacks;
    void* restrict datasets;
    bool * restrict stop_propagation;
    void* restrict focus_target;
    void* words_cache;
    void* shaped_words_cache;
    void* positioned_words_cache;
    void* positioned_rects;
    void* restrict words_changed_in_callbacks;
    void* restrict images_changed_in_callbacks;
    void* restrict image_masks_changed_in_callbacks;
    void* restrict css_properties_changed_in_callbacks;
    void* current_scroll_states;
    void* restrict nodes_scrolled_in_callback;
    AzDomNodeId hit_dom_node;
    AzOptionLayoutPoint cursor_relative_to_item;
    AzOptionLayoutPoint cursor_in_viewport;
};
typedef struct AzCallbackInfo AzCallbackInfo;

struct AzInlineText {
    AzInlineLineVec lines;
    AzLogicalRect bounds;
    float font_size_px;
    size_t last_word_index;
    float baseline_descender_px;
};
typedef struct AzInlineText AzInlineText;

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
#define AzNodeDataInlineCssProperty_Normal(v) { .Normal = { .tag = AzNodeDataInlineCssPropertyTag_Normal, .payload = v } }
#define AzNodeDataInlineCssProperty_Active(v) { .Active = { .tag = AzNodeDataInlineCssPropertyTag_Active, .payload = v } }
#define AzNodeDataInlineCssProperty_Focus(v) { .Focus = { .tag = AzNodeDataInlineCssPropertyTag_Focus, .payload = v } }
#define AzNodeDataInlineCssProperty_Hover(v) { .Hover = { .tag = AzNodeDataInlineCssPropertyTag_Hover, .payload = v } }

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
#define AzSvgNode_MultiPolygonCollection(v) { .MultiPolygonCollection = { .tag = AzSvgNodeTag_MultiPolygonCollection, .payload = v } }
#define AzSvgNode_MultiPolygon(v) { .MultiPolygon = { .tag = AzSvgNodeTag_MultiPolygon, .payload = v } }
#define AzSvgNode_Path(v) { .Path = { .tag = AzSvgNodeTag_Path, .payload = v } }
#define AzSvgNode_Circle(v) { .Circle = { .tag = AzSvgNodeTag_Circle, .payload = v } }
#define AzSvgNode_Rect(v) { .Rect = { .tag = AzSvgNodeTag_Rect, .payload = v } }

struct AzSvgStyledNode {
    AzSvgNode geometry;
    AzSvgStyle style;
};
typedef struct AzSvgStyledNode AzSvgStyledNode;

struct AzNodeDataInlineCssPropertyVec {
    AzNodeDataInlineCssProperty* ptr;
    size_t len;
    size_t cap;
    AzNodeDataInlineCssPropertyVecDestructor destructor;
};
typedef struct AzNodeDataInlineCssPropertyVec AzNodeDataInlineCssPropertyVec;

enum AzOptionInlineTextTag {
   AzOptionInlineTextTag_None,
   AzOptionInlineTextTag_Some,
};
typedef enum AzOptionInlineTextTag AzOptionInlineTextTag;

struct AzOptionInlineTextVariant_None { AzOptionInlineTextTag tag; };
typedef struct AzOptionInlineTextVariant_None AzOptionInlineTextVariant_None;
struct AzOptionInlineTextVariant_Some { AzOptionInlineTextTag tag; AzInlineText payload; };
typedef struct AzOptionInlineTextVariant_Some AzOptionInlineTextVariant_Some;
union AzOptionInlineText {
    AzOptionInlineTextVariant_None None;
    AzOptionInlineTextVariant_Some Some;
};
typedef union AzOptionInlineText AzOptionInlineText;
#define AzOptionInlineText_None { .None = { .tag = AzOptionInlineTextTag_None } }
#define AzOptionInlineText_Some(v) { .Some = { .tag = AzOptionInlineTextTag_Some, .payload = v } }

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
#define AzXmlParseError_InvalidDeclaration(v) { .InvalidDeclaration = { .tag = AzXmlParseErrorTag_InvalidDeclaration, .payload = v } }
#define AzXmlParseError_InvalidComment(v) { .InvalidComment = { .tag = AzXmlParseErrorTag_InvalidComment, .payload = v } }
#define AzXmlParseError_InvalidPI(v) { .InvalidPI = { .tag = AzXmlParseErrorTag_InvalidPI, .payload = v } }
#define AzXmlParseError_InvalidDoctype(v) { .InvalidDoctype = { .tag = AzXmlParseErrorTag_InvalidDoctype, .payload = v } }
#define AzXmlParseError_InvalidEntity(v) { .InvalidEntity = { .tag = AzXmlParseErrorTag_InvalidEntity, .payload = v } }
#define AzXmlParseError_InvalidElement(v) { .InvalidElement = { .tag = AzXmlParseErrorTag_InvalidElement, .payload = v } }
#define AzXmlParseError_InvalidAttribute(v) { .InvalidAttribute = { .tag = AzXmlParseErrorTag_InvalidAttribute, .payload = v } }
#define AzXmlParseError_InvalidCdata(v) { .InvalidCdata = { .tag = AzXmlParseErrorTag_InvalidCdata, .payload = v } }
#define AzXmlParseError_InvalidCharData(v) { .InvalidCharData = { .tag = AzXmlParseErrorTag_InvalidCharData, .payload = v } }
#define AzXmlParseError_UnknownToken(v) { .UnknownToken = { .tag = AzXmlParseErrorTag_UnknownToken, .payload = v } }

struct AzWindowCreateOptions {
    AzWindowState state;
    AzOptionRendererOptions renderer_type;
    AzOptionWindowTheme theme;
    AzOptionCallback create_callback;
    bool  hot_reload;
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
#define AzFocusTarget_Id(v) { .Id = { .tag = AzFocusTargetTag_Id, .payload = v } }
#define AzFocusTarget_Path(v) { .Path = { .tag = AzFocusTargetTag_Path, .payload = v } }
#define AzFocusTarget_Previous { .Previous = { .tag = AzFocusTargetTag_Previous } }
#define AzFocusTarget_Next { .Next = { .tag = AzFocusTargetTag_Next } }
#define AzFocusTarget_First { .First = { .tag = AzFocusTargetTag_First } }
#define AzFocusTarget_Last { .Last = { .tag = AzFocusTargetTag_Last } }
#define AzFocusTarget_NoFocus { .NoFocus = { .tag = AzFocusTargetTag_NoFocus } }

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
#define AzCssDeclaration_Static(v) { .Static = { .tag = AzCssDeclarationTag_Static, .payload = v } }
#define AzCssDeclaration_Dynamic(v) { .Dynamic = { .tag = AzCssDeclarationTag_Dynamic, .payload = v } }

struct AzCssDeclarationVec {
    AzCssDeclaration* ptr;
    size_t len;
    size_t cap;
    AzCssDeclarationVecDestructor destructor;
};
typedef struct AzCssDeclarationVec AzCssDeclarationVec;

struct AzNodeDataVec {
    AzNodeData* ptr;
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
#define AzXmlError_InvalidXmlPrefixUri(v) { .InvalidXmlPrefixUri = { .tag = AzXmlErrorTag_InvalidXmlPrefixUri, .payload = v } }
#define AzXmlError_UnexpectedXmlUri(v) { .UnexpectedXmlUri = { .tag = AzXmlErrorTag_UnexpectedXmlUri, .payload = v } }
#define AzXmlError_UnexpectedXmlnsUri(v) { .UnexpectedXmlnsUri = { .tag = AzXmlErrorTag_UnexpectedXmlnsUri, .payload = v } }
#define AzXmlError_InvalidElementNamePrefix(v) { .InvalidElementNamePrefix = { .tag = AzXmlErrorTag_InvalidElementNamePrefix, .payload = v } }
#define AzXmlError_DuplicatedNamespace(v) { .DuplicatedNamespace = { .tag = AzXmlErrorTag_DuplicatedNamespace, .payload = v } }
#define AzXmlError_UnknownNamespace(v) { .UnknownNamespace = { .tag = AzXmlErrorTag_UnknownNamespace, .payload = v } }
#define AzXmlError_UnexpectedCloseTag(v) { .UnexpectedCloseTag = { .tag = AzXmlErrorTag_UnexpectedCloseTag, .payload = v } }
#define AzXmlError_UnexpectedEntityCloseTag(v) { .UnexpectedEntityCloseTag = { .tag = AzXmlErrorTag_UnexpectedEntityCloseTag, .payload = v } }
#define AzXmlError_UnknownEntityReference(v) { .UnknownEntityReference = { .tag = AzXmlErrorTag_UnknownEntityReference, .payload = v } }
#define AzXmlError_MalformedEntityReference(v) { .MalformedEntityReference = { .tag = AzXmlErrorTag_MalformedEntityReference, .payload = v } }
#define AzXmlError_EntityReferenceLoop(v) { .EntityReferenceLoop = { .tag = AzXmlErrorTag_EntityReferenceLoop, .payload = v } }
#define AzXmlError_InvalidAttributeValue(v) { .InvalidAttributeValue = { .tag = AzXmlErrorTag_InvalidAttributeValue, .payload = v } }
#define AzXmlError_DuplicatedAttribute(v) { .DuplicatedAttribute = { .tag = AzXmlErrorTag_DuplicatedAttribute, .payload = v } }
#define AzXmlError_NoRootNode { .NoRootNode = { .tag = AzXmlErrorTag_NoRootNode } }
#define AzXmlError_SizeLimit { .SizeLimit = { .tag = AzXmlErrorTag_SizeLimit } }
#define AzXmlError_ParserError(v) { .ParserError = { .tag = AzXmlErrorTag_ParserError, .payload = v } }

struct AzDom {
    AzNodeData root;
    AzDomVec children;
    size_t total_children;
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
    AzCssRuleBlock* ptr;
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
#define AzOptionDom_None { .None = { .tag = AzOptionDomTag_None } }
#define AzOptionDom_Some(v) { .Some = { .tag = AzOptionDomTag_Some, .payload = v } }

enum AzResultXmlXmlErrorTag {
   AzResultXmlXmlErrorTag_Ok,
   AzResultXmlXmlErrorTag_Err,
};
typedef enum AzResultXmlXmlErrorTag AzResultXmlXmlErrorTag;

struct AzResultXmlXmlErrorVariant_Ok { AzResultXmlXmlErrorTag tag; AzXml payload; };
typedef struct AzResultXmlXmlErrorVariant_Ok AzResultXmlXmlErrorVariant_Ok;
struct AzResultXmlXmlErrorVariant_Err { AzResultXmlXmlErrorTag tag; AzXmlError payload; };
typedef struct AzResultXmlXmlErrorVariant_Err AzResultXmlXmlErrorVariant_Err;
union AzResultXmlXmlError {
    AzResultXmlXmlErrorVariant_Ok Ok;
    AzResultXmlXmlErrorVariant_Err Err;
};
typedef union AzResultXmlXmlError AzResultXmlXmlError;
#define AzResultXmlXmlError_Ok(v) { .Ok = { .tag = AzResultXmlXmlErrorTag_Ok, .payload = v } }
#define AzResultXmlXmlError_Err(v) { .Err = { .tag = AzResultXmlXmlErrorTag_Err, .payload = v } }

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
#define AzSvgParseError_InvalidFileSuffix { .InvalidFileSuffix = { .tag = AzSvgParseErrorTag_InvalidFileSuffix } }
#define AzSvgParseError_FileOpenFailed { .FileOpenFailed = { .tag = AzSvgParseErrorTag_FileOpenFailed } }
#define AzSvgParseError_NotAnUtf8Str { .NotAnUtf8Str = { .tag = AzSvgParseErrorTag_NotAnUtf8Str } }
#define AzSvgParseError_MalformedGZip { .MalformedGZip = { .tag = AzSvgParseErrorTag_MalformedGZip } }
#define AzSvgParseError_InvalidSize { .InvalidSize = { .tag = AzSvgParseErrorTag_InvalidSize } }
#define AzSvgParseError_ParsingFailed(v) { .ParsingFailed = { .tag = AzSvgParseErrorTag_ParsingFailed, .payload = v } }

struct AzIFrameCallbackReturn {
    AzStyledDom dom;
    AzLogicalSize scroll_size;
    AzLogicalPosition scroll_offset;
    AzLogicalSize virtual_scroll_size;
    AzLogicalPosition virtual_scroll_offset;
};
typedef struct AzIFrameCallbackReturn AzIFrameCallbackReturn;

struct AzStylesheet {
    AzCssRuleBlockVec rules;
};
typedef struct AzStylesheet AzStylesheet;

struct AzStylesheetVec {
    AzStylesheet* ptr;
    size_t len;
    size_t cap;
    AzStylesheetVecDestructor destructor;
};
typedef struct AzStylesheetVec AzStylesheetVec;

enum AzResultSvgXmlNodeSvgParseErrorTag {
   AzResultSvgXmlNodeSvgParseErrorTag_Ok,
   AzResultSvgXmlNodeSvgParseErrorTag_Err,
};
typedef enum AzResultSvgXmlNodeSvgParseErrorTag AzResultSvgXmlNodeSvgParseErrorTag;

struct AzResultSvgXmlNodeSvgParseErrorVariant_Ok { AzResultSvgXmlNodeSvgParseErrorTag tag; AzSvgXmlNode payload; };
typedef struct AzResultSvgXmlNodeSvgParseErrorVariant_Ok AzResultSvgXmlNodeSvgParseErrorVariant_Ok;
struct AzResultSvgXmlNodeSvgParseErrorVariant_Err { AzResultSvgXmlNodeSvgParseErrorTag tag; AzSvgParseError payload; };
typedef struct AzResultSvgXmlNodeSvgParseErrorVariant_Err AzResultSvgXmlNodeSvgParseErrorVariant_Err;
union AzResultSvgXmlNodeSvgParseError {
    AzResultSvgXmlNodeSvgParseErrorVariant_Ok Ok;
    AzResultSvgXmlNodeSvgParseErrorVariant_Err Err;
};
typedef union AzResultSvgXmlNodeSvgParseError AzResultSvgXmlNodeSvgParseError;
#define AzResultSvgXmlNodeSvgParseError_Ok(v) { .Ok = { .tag = AzResultSvgXmlNodeSvgParseErrorTag_Ok, .payload = v } }
#define AzResultSvgXmlNodeSvgParseError_Err(v) { .Err = { .tag = AzResultSvgXmlNodeSvgParseErrorTag_Err, .payload = v } }

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
#define AzResultSvgSvgParseError_Ok(v) { .Ok = { .tag = AzResultSvgSvgParseErrorTag_Ok, .payload = v } }
#define AzResultSvgSvgParseError_Err(v) { .Err = { .tag = AzResultSvgSvgParseErrorTag_Err, .payload = v } }

struct AzCss {
    AzStylesheetVec stylesheets;
};
typedef struct AzCss AzCss;

AzTesselatedSvgNode AzTesselatedSvgNodeVecArray[] = {};
#define AzTesselatedSvgNodeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzTesselatedSvgNode), .cap = sizeof(v) / sizeof(AzTesselatedSvgNode), .destructor = { .NoDestructor = { .tag = AzTesselatedSvgNodeVecDestructorTag_NoDestructor, }, }, }
#define AzTesselatedSvgNodeVec_empty { .ptr = &AzTesselatedSvgNodeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzTesselatedSvgNodeVecDestructorTag_NoDestructor, }, }, }

AzXmlNode AzXmlNodeVecArray[] = {};
#define AzXmlNodeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzXmlNode), .cap = sizeof(v) / sizeof(AzXmlNode), .destructor = { .NoDestructor = { .tag = AzXmlNodeVecDestructorTag_NoDestructor, }, }, }
#define AzXmlNodeVec_empty { .ptr = &AzXmlNodeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzXmlNodeVecDestructorTag_NoDestructor, }, }, }

AzFmtArg AzFmtArgVecArray[] = {};
#define AzFmtArgVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzFmtArg), .cap = sizeof(v) / sizeof(AzFmtArg), .destructor = { .NoDestructor = { .tag = AzFmtArgVecDestructorTag_NoDestructor, }, }, }
#define AzFmtArgVec_empty { .ptr = &AzFmtArgVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzFmtArgVecDestructorTag_NoDestructor, }, }, }

AzInlineLine AzInlineLineVecArray[] = {};
#define AzInlineLineVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzInlineLine), .cap = sizeof(v) / sizeof(AzInlineLine), .destructor = { .NoDestructor = { .tag = AzInlineLineVecDestructorTag_NoDestructor, }, }, }
#define AzInlineLineVec_empty { .ptr = &AzInlineLineVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzInlineLineVecDestructorTag_NoDestructor, }, }, }

AzInlineWord AzInlineWordVecArray[] = {};
#define AzInlineWordVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzInlineWord), .cap = sizeof(v) / sizeof(AzInlineWord), .destructor = { .NoDestructor = { .tag = AzInlineWordVecDestructorTag_NoDestructor, }, }, }
#define AzInlineWordVec_empty { .ptr = &AzInlineWordVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzInlineWordVecDestructorTag_NoDestructor, }, }, }

AzInlineGlyph AzInlineGlyphVecArray[] = {};
#define AzInlineGlyphVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzInlineGlyph), .cap = sizeof(v) / sizeof(AzInlineGlyph), .destructor = { .NoDestructor = { .tag = AzInlineGlyphVecDestructorTag_NoDestructor, }, }, }
#define AzInlineGlyphVec_empty { .ptr = &AzInlineGlyphVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzInlineGlyphVecDestructorTag_NoDestructor, }, }, }

AzInlineTextHit AzInlineTextHitVecArray[] = {};
#define AzInlineTextHitVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzInlineTextHit), .cap = sizeof(v) / sizeof(AzInlineTextHit), .destructor = { .NoDestructor = { .tag = AzInlineTextHitVecDestructorTag_NoDestructor, }, }, }
#define AzInlineTextHitVec_empty { .ptr = &AzInlineTextHitVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzInlineTextHitVecDestructorTag_NoDestructor, }, }, }

AzMonitor AzMonitorVecArray[] = {};
#define AzMonitorVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzMonitor), .cap = sizeof(v) / sizeof(AzMonitor), .destructor = { .NoDestructor = { .tag = AzMonitorVecDestructorTag_NoDestructor, }, }, }
#define AzMonitorVec_empty { .ptr = &AzMonitorVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzMonitorVecDestructorTag_NoDestructor, }, }, }

AzVideoMode AzVideoModeVecArray[] = {};
#define AzVideoModeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzVideoMode), .cap = sizeof(v) / sizeof(AzVideoMode), .destructor = { .NoDestructor = { .tag = AzVideoModeVecDestructorTag_NoDestructor, }, }, }
#define AzVideoModeVec_empty { .ptr = &AzVideoModeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzVideoModeVecDestructorTag_NoDestructor, }, }, }

AzDom AzDomVecArray[] = {};
#define AzDomVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzDom), .cap = sizeof(v) / sizeof(AzDom), .destructor = { .NoDestructor = { .tag = AzDomVecDestructorTag_NoDestructor, }, }, }
#define AzDomVec_empty { .ptr = &AzDomVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzDomVecDestructorTag_NoDestructor, }, }, }

AzIdOrClass AzIdOrClassVecArray[] = {};
#define AzIdOrClassVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzIdOrClass), .cap = sizeof(v) / sizeof(AzIdOrClass), .destructor = { .NoDestructor = { .tag = AzIdOrClassVecDestructorTag_NoDestructor, }, }, }
#define AzIdOrClassVec_empty { .ptr = &AzIdOrClassVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzIdOrClassVecDestructorTag_NoDestructor, }, }, }

AzNodeDataInlineCssProperty AzNodeDataInlineCssPropertyVecArray[] = {};
#define AzNodeDataInlineCssPropertyVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzNodeDataInlineCssProperty), .cap = sizeof(v) / sizeof(AzNodeDataInlineCssProperty), .destructor = { .NoDestructor = { .tag = AzNodeDataInlineCssPropertyVecDestructorTag_NoDestructor, }, }, }
#define AzNodeDataInlineCssPropertyVec_empty { .ptr = &AzNodeDataInlineCssPropertyVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzNodeDataInlineCssPropertyVecDestructorTag_NoDestructor, }, }, }

AzStyleBackgroundContent AzStyleBackgroundContentVecArray[] = {};
#define AzStyleBackgroundContentVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStyleBackgroundContent), .cap = sizeof(v) / sizeof(AzStyleBackgroundContent), .destructor = { .NoDestructor = { .tag = AzStyleBackgroundContentVecDestructorTag_NoDestructor, }, }, }
#define AzStyleBackgroundContentVec_empty { .ptr = &AzStyleBackgroundContentVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStyleBackgroundContentVecDestructorTag_NoDestructor, }, }, }

AzStyleBackgroundPosition AzStyleBackgroundPositionVecArray[] = {};
#define AzStyleBackgroundPositionVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStyleBackgroundPosition), .cap = sizeof(v) / sizeof(AzStyleBackgroundPosition), .destructor = { .NoDestructor = { .tag = AzStyleBackgroundPositionVecDestructorTag_NoDestructor, }, }, }
#define AzStyleBackgroundPositionVec_empty { .ptr = &AzStyleBackgroundPositionVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStyleBackgroundPositionVecDestructorTag_NoDestructor, }, }, }

AzStyleBackgroundRepeat AzStyleBackgroundRepeatVecArray[] = {};
#define AzStyleBackgroundRepeatVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStyleBackgroundRepeat), .cap = sizeof(v) / sizeof(AzStyleBackgroundRepeat), .destructor = { .NoDestructor = { .tag = AzStyleBackgroundRepeatVecDestructorTag_NoDestructor, }, }, }
#define AzStyleBackgroundRepeatVec_empty { .ptr = &AzStyleBackgroundRepeatVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStyleBackgroundRepeatVecDestructorTag_NoDestructor, }, }, }

AzStyleBackgroundSize AzStyleBackgroundSizeVecArray[] = {};
#define AzStyleBackgroundSizeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStyleBackgroundSize), .cap = sizeof(v) / sizeof(AzStyleBackgroundSize), .destructor = { .NoDestructor = { .tag = AzStyleBackgroundSizeVecDestructorTag_NoDestructor, }, }, }
#define AzStyleBackgroundSizeVec_empty { .ptr = &AzStyleBackgroundSizeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStyleBackgroundSizeVecDestructorTag_NoDestructor, }, }, }

AzStyleTransform AzStyleTransformVecArray[] = {};
#define AzStyleTransformVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStyleTransform), .cap = sizeof(v) / sizeof(AzStyleTransform), .destructor = { .NoDestructor = { .tag = AzStyleTransformVecDestructorTag_NoDestructor, }, }, }
#define AzStyleTransformVec_empty { .ptr = &AzStyleTransformVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStyleTransformVecDestructorTag_NoDestructor, }, }, }

AzCssProperty AzCssPropertyVecArray[] = {};
#define AzCssPropertyVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzCssProperty), .cap = sizeof(v) / sizeof(AzCssProperty), .destructor = { .NoDestructor = { .tag = AzCssPropertyVecDestructorTag_NoDestructor, }, }, }
#define AzCssPropertyVec_empty { .ptr = &AzCssPropertyVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzCssPropertyVecDestructorTag_NoDestructor, }, }, }

AzSvgMultiPolygon AzSvgMultiPolygonVecArray[] = {};
#define AzSvgMultiPolygonVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzSvgMultiPolygon), .cap = sizeof(v) / sizeof(AzSvgMultiPolygon), .destructor = { .NoDestructor = { .tag = AzSvgMultiPolygonVecDestructorTag_NoDestructor, }, }, }
#define AzSvgMultiPolygonVec_empty { .ptr = &AzSvgMultiPolygonVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzSvgMultiPolygonVecDestructorTag_NoDestructor, }, }, }

AzSvgPath AzSvgPathVecArray[] = {};
#define AzSvgPathVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzSvgPath), .cap = sizeof(v) / sizeof(AzSvgPath), .destructor = { .NoDestructor = { .tag = AzSvgPathVecDestructorTag_NoDestructor, }, }, }
#define AzSvgPathVec_empty { .ptr = &AzSvgPathVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzSvgPathVecDestructorTag_NoDestructor, }, }, }

AzVertexAttribute AzVertexAttributeVecArray[] = {};
#define AzVertexAttributeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzVertexAttribute), .cap = sizeof(v) / sizeof(AzVertexAttribute), .destructor = { .NoDestructor = { .tag = AzVertexAttributeVecDestructorTag_NoDestructor, }, }, }
#define AzVertexAttributeVec_empty { .ptr = &AzVertexAttributeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzVertexAttributeVecDestructorTag_NoDestructor, }, }, }

AzSvgPathElement AzSvgPathElementVecArray[] = {};
#define AzSvgPathElementVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzSvgPathElement), .cap = sizeof(v) / sizeof(AzSvgPathElement), .destructor = { .NoDestructor = { .tag = AzSvgPathElementVecDestructorTag_NoDestructor, }, }, }
#define AzSvgPathElementVec_empty { .ptr = &AzSvgPathElementVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzSvgPathElementVecDestructorTag_NoDestructor, }, }, }

AzSvgVertex AzSvgVertexVecArray[] = {};
#define AzSvgVertexVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzSvgVertex), .cap = sizeof(v) / sizeof(AzSvgVertex), .destructor = { .NoDestructor = { .tag = AzSvgVertexVecDestructorTag_NoDestructor, }, }, }
#define AzSvgVertexVec_empty { .ptr = &AzSvgVertexVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzSvgVertexVecDestructorTag_NoDestructor, }, }, }

uint32_t AzU32VecArray[] = {};
#define AzU32Vec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(uint32_t), .cap = sizeof(v) / sizeof(uint32_t), .destructor = { .NoDestructor = { .tag = AzU32VecDestructorTag_NoDestructor, }, }, }
#define AzU32Vec_empty { .ptr = &AzU32VecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzU32VecDestructorTag_NoDestructor, }, }, }

AzXWindowType AzXWindowTypeVecArray[] = {};
#define AzXWindowTypeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzXWindowType), .cap = sizeof(v) / sizeof(AzXWindowType), .destructor = { .NoDestructor = { .tag = AzXWindowTypeVecDestructorTag_NoDestructor, }, }, }
#define AzXWindowTypeVec_empty { .ptr = &AzXWindowTypeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzXWindowTypeVecDestructorTag_NoDestructor, }, }, }

AzVirtualKeyCode AzVirtualKeyCodeVecArray[] = {};
#define AzVirtualKeyCodeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzVirtualKeyCode), .cap = sizeof(v) / sizeof(AzVirtualKeyCode), .destructor = { .NoDestructor = { .tag = AzVirtualKeyCodeVecDestructorTag_NoDestructor, }, }, }
#define AzVirtualKeyCodeVec_empty { .ptr = &AzVirtualKeyCodeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzVirtualKeyCodeVecDestructorTag_NoDestructor, }, }, }

AzCascadeInfo AzCascadeInfoVecArray[] = {};
#define AzCascadeInfoVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzCascadeInfo), .cap = sizeof(v) / sizeof(AzCascadeInfo), .destructor = { .NoDestructor = { .tag = AzCascadeInfoVecDestructorTag_NoDestructor, }, }, }
#define AzCascadeInfoVec_empty { .ptr = &AzCascadeInfoVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzCascadeInfoVecDestructorTag_NoDestructor, }, }, }

uint32_t AzScanCodeVecArray[] = {};
#define AzScanCodeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(uint32_t), .cap = sizeof(v) / sizeof(uint32_t), .destructor = { .NoDestructor = { .tag = AzScanCodeVecDestructorTag_NoDestructor, }, }, }
#define AzScanCodeVec_empty { .ptr = &AzScanCodeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzScanCodeVecDestructorTag_NoDestructor, }, }, }

AzCssDeclaration AzCssDeclarationVecArray[] = {};
#define AzCssDeclarationVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzCssDeclaration), .cap = sizeof(v) / sizeof(AzCssDeclaration), .destructor = { .NoDestructor = { .tag = AzCssDeclarationVecDestructorTag_NoDestructor, }, }, }
#define AzCssDeclarationVec_empty { .ptr = &AzCssDeclarationVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzCssDeclarationVecDestructorTag_NoDestructor, }, }, }

AzCssPathSelector AzCssPathSelectorVecArray[] = {};
#define AzCssPathSelectorVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzCssPathSelector), .cap = sizeof(v) / sizeof(AzCssPathSelector), .destructor = { .NoDestructor = { .tag = AzCssPathSelectorVecDestructorTag_NoDestructor, }, }, }
#define AzCssPathSelectorVec_empty { .ptr = &AzCssPathSelectorVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzCssPathSelectorVecDestructorTag_NoDestructor, }, }, }

AzStylesheet AzStylesheetVecArray[] = {};
#define AzStylesheetVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStylesheet), .cap = sizeof(v) / sizeof(AzStylesheet), .destructor = { .NoDestructor = { .tag = AzStylesheetVecDestructorTag_NoDestructor, }, }, }
#define AzStylesheetVec_empty { .ptr = &AzStylesheetVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStylesheetVecDestructorTag_NoDestructor, }, }, }

AzCssRuleBlock AzCssRuleBlockVecArray[] = {};
#define AzCssRuleBlockVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzCssRuleBlock), .cap = sizeof(v) / sizeof(AzCssRuleBlock), .destructor = { .NoDestructor = { .tag = AzCssRuleBlockVecDestructorTag_NoDestructor, }, }, }
#define AzCssRuleBlockVec_empty { .ptr = &AzCssRuleBlockVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzCssRuleBlockVecDestructorTag_NoDestructor, }, }, }

uint16_t AzU16VecArray[] = {};
#define AzU16Vec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(uint16_t), .cap = sizeof(v) / sizeof(uint16_t), .destructor = { .NoDestructor = { .tag = AzU16VecDestructorTag_NoDestructor, }, }, }
#define AzU16Vec_empty { .ptr = &AzU16VecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzU16VecDestructorTag_NoDestructor, }, }, }

float AzF32VecArray[] = {};
#define AzF32Vec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(float), .cap = sizeof(v) / sizeof(float), .destructor = { .NoDestructor = { .tag = AzF32VecDestructorTag_NoDestructor, }, }, }
#define AzF32Vec_empty { .ptr = &AzF32VecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzF32VecDestructorTag_NoDestructor, }, }, }

uint8_t AzU8VecArray[] = {};
#define AzU8Vec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(uint8_t), .cap = sizeof(v) / sizeof(uint8_t), .destructor = { .NoDestructor = { .tag = AzU8VecDestructorTag_NoDestructor, }, }, }
#define AzU8Vec_empty { .ptr = &AzU8VecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzU8VecDestructorTag_NoDestructor, }, }, }

AzCallbackData AzCallbackDataVecArray[] = {};
#define AzCallbackDataVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzCallbackData), .cap = sizeof(v) / sizeof(AzCallbackData), .destructor = { .NoDestructor = { .tag = AzCallbackDataVecDestructorTag_NoDestructor, }, }, }
#define AzCallbackDataVec_empty { .ptr = &AzCallbackDataVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzCallbackDataVecDestructorTag_NoDestructor, }, }, }

AzDebugMessage AzDebugMessageVecArray[] = {};
#define AzDebugMessageVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzDebugMessage), .cap = sizeof(v) / sizeof(AzDebugMessage), .destructor = { .NoDestructor = { .tag = AzDebugMessageVecDestructorTag_NoDestructor, }, }, }
#define AzDebugMessageVec_empty { .ptr = &AzDebugMessageVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzDebugMessageVecDestructorTag_NoDestructor, }, }, }

uint32_t AzGLuintVecArray[] = {};
#define AzGLuintVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(uint32_t), .cap = sizeof(v) / sizeof(uint32_t), .destructor = { .NoDestructor = { .tag = AzGLuintVecDestructorTag_NoDestructor, }, }, }
#define AzGLuintVec_empty { .ptr = &AzGLuintVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzGLuintVecDestructorTag_NoDestructor, }, }, }

int32_t AzGLintVecArray[] = {};
#define AzGLintVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(int32_t), .cap = sizeof(v) / sizeof(int32_t), .destructor = { .NoDestructor = { .tag = AzGLintVecDestructorTag_NoDestructor, }, }, }
#define AzGLintVec_empty { .ptr = &AzGLintVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzGLintVecDestructorTag_NoDestructor, }, }, }

AzString AzStringVecArray[] = {};
#define AzStringVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzString), .cap = sizeof(v) / sizeof(AzString), .destructor = { .NoDestructor = { .tag = AzStringVecDestructorTag_NoDestructor, }, }, }
#define AzStringVec_empty { .ptr = &AzStringVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStringVecDestructorTag_NoDestructor, }, }, }

AzStringPair AzStringPairVecArray[] = {};
#define AzStringPairVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStringPair), .cap = sizeof(v) / sizeof(AzStringPair), .destructor = { .NoDestructor = { .tag = AzStringPairVecDestructorTag_NoDestructor, }, }, }
#define AzStringPairVec_empty { .ptr = &AzStringPairVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStringPairVecDestructorTag_NoDestructor, }, }, }

AzLinearColorStop AzLinearColorStopVecArray[] = {};
#define AzLinearColorStopVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzLinearColorStop), .cap = sizeof(v) / sizeof(AzLinearColorStop), .destructor = { .NoDestructor = { .tag = AzLinearColorStopVecDestructorTag_NoDestructor, }, }, }
#define AzLinearColorStopVec_empty { .ptr = &AzLinearColorStopVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzLinearColorStopVecDestructorTag_NoDestructor, }, }, }

AzRadialColorStop AzRadialColorStopVecArray[] = {};
#define AzRadialColorStopVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzRadialColorStop), .cap = sizeof(v) / sizeof(AzRadialColorStop), .destructor = { .NoDestructor = { .tag = AzRadialColorStopVecDestructorTag_NoDestructor, }, }, }
#define AzRadialColorStopVec_empty { .ptr = &AzRadialColorStopVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzRadialColorStopVecDestructorTag_NoDestructor, }, }, }

AzNodeId AzNodeIdVecArray[] = {};
#define AzNodeIdVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzNodeId), .cap = sizeof(v) / sizeof(AzNodeId), .destructor = { .NoDestructor = { .tag = AzNodeIdVecDestructorTag_NoDestructor, }, }, }
#define AzNodeIdVec_empty { .ptr = &AzNodeIdVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzNodeIdVecDestructorTag_NoDestructor, }, }, }

AzNode AzNodeVecArray[] = {};
#define AzNodeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzNode), .cap = sizeof(v) / sizeof(AzNode), .destructor = { .NoDestructor = { .tag = AzNodeVecDestructorTag_NoDestructor, }, }, }
#define AzNodeVec_empty { .ptr = &AzNodeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzNodeVecDestructorTag_NoDestructor, }, }, }

AzStyledNode AzStyledNodeVecArray[] = {};
#define AzStyledNodeVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzStyledNode), .cap = sizeof(v) / sizeof(AzStyledNode), .destructor = { .NoDestructor = { .tag = AzStyledNodeVecDestructorTag_NoDestructor, }, }, }
#define AzStyledNodeVec_empty { .ptr = &AzStyledNodeVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzStyledNodeVecDestructorTag_NoDestructor, }, }, }

AzTagIdToNodeIdMapping AzTagIdsToNodeIdsMappingVecArray[] = {};
#define AzTagIdsToNodeIdsMappingVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzTagIdsToNodeIdsMapping), .cap = sizeof(v) / sizeof(AzTagIdsToNodeIdsMapping), .destructor = { .NoDestructor = { .tag = AzTagIdsToNodeIdsMappingVecDestructorTag_NoDestructor, }, }, }
#define AzTagIdsToNodeIdsMappingVec_empty { .ptr = &AzTagIdsToNodeIdsMappingVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzTagIdsToNodeIdsMappingVecDestructorTag_NoDestructor, }, }, }

AzParentWithNodeDepth AzParentWithNodeDepthVecArray[] = {};
#define AzParentWithNodeDepthVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzParentWithNodeDepth), .cap = sizeof(v) / sizeof(AzParentWithNodeDepth), .destructor = { .NoDestructor = { .tag = AzParentWithNodeDepthVecDestructorTag_NoDestructor, }, }, }
#define AzParentWithNodeDepthVec_empty { .ptr = &AzParentWithNodeDepthVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzParentWithNodeDepthVecDestructorTag_NoDestructor, }, }, }

AzNodeData AzNodeDataVecArray[] = {};
#define AzNodeDataVec_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(AzNodeData), .cap = sizeof(v) / sizeof(AzNodeData), .destructor = { .NoDestructor = { .tag = AzNodeDataVecDestructorTag_NoDestructor, }, }, }
#define AzNodeDataVec_empty { .ptr = &AzNodeDataVecArray, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = AzNodeDataVecDestructorTag_NoDestructor, }, }, }


/* FUNCTIONS from azul.dll / libazul.so */
extern DLLIMPORT AzApp AzApp_new(AzRefAny  data, AzAppConfig  config);
extern DLLIMPORT void AzApp_addWindow(AzApp* restrict app, AzWindowCreateOptions  window);
extern DLLIMPORT AzMonitorVec AzApp_getMonitors(AzApp* const app);
extern DLLIMPORT void AzApp_run(const AzApp app, AzWindowCreateOptions  window);
extern DLLIMPORT void AzApp_delete(AzApp* restrict instance);
extern DLLIMPORT AzSystemCallbacks AzSystemCallbacks_libraryInternal();
extern DLLIMPORT AzWindowCreateOptions AzWindowCreateOptions_new(AzLayoutCallbackType  layout_callback);
extern DLLIMPORT AzWindowState AzWindowState_new(AzLayoutCallbackType  layout_callback);
extern DLLIMPORT AzWindowState AzWindowState_default();
extern DLLIMPORT AzDomNodeId AzCallbackInfo_getHitNode(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzOptionLayoutPoint AzCallbackInfo_getCursorRelativeToViewport(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzOptionLayoutPoint AzCallbackInfo_getCursorRelativeToNode(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzWindowState AzCallbackInfo_getWindowState(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzKeyboardState AzCallbackInfo_getKeyboardState(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzMouseState AzCallbackInfo_getMouseState(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzRawWindowHandle AzCallbackInfo_getCurrentWindowHandle(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzOptionGl AzCallbackInfo_getGlContext(AzCallbackInfo* const callbackinfo);
extern DLLIMPORT AzOptionLogicalPosition AzCallbackInfo_getScrollPosition(AzCallbackInfo* const callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionRefAny AzCallbackInfo_getDataset(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionString AzCallbackInfo_getStringContents(AzCallbackInfo* const callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionInlineText AzCallbackInfo_getInlineText(AzCallbackInfo* const callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzCallbackInfo_getParent(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzCallbackInfo_getPreviousSibling(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzCallbackInfo_getNextSibling(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzCallbackInfo_getFirstChild(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzCallbackInfo_getLastChild(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT void AzCallbackInfo_setWindowState(AzCallbackInfo* restrict callbackinfo, AzWindowState  new_state);
extern DLLIMPORT void AzCallbackInfo_setFocus(AzCallbackInfo* restrict callbackinfo, AzFocusTarget  target);
extern DLLIMPORT void AzCallbackInfo_setCssProperty(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzCssProperty  new_property);
extern DLLIMPORT void AzCallbackInfo_setScrollPosition(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzLogicalPosition  scroll_position);
extern DLLIMPORT void AzCallbackInfo_setStringContents(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzString  string);
extern DLLIMPORT void AzCallbackInfo_exchangeImage(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzImageSource  new_image);
extern DLLIMPORT void AzCallbackInfo_exchangeImageMask(AzCallbackInfo* restrict callbackinfo, AzDomNodeId  node_id, AzImageMask  new_mask);
extern DLLIMPORT void AzCallbackInfo_stopPropagation(AzCallbackInfo* restrict callbackinfo);
extern DLLIMPORT void AzCallbackInfo_createWindow(AzCallbackInfo* restrict callbackinfo, AzWindowCreateOptions  new_window);
extern DLLIMPORT void AzCallbackInfo_startThread(AzCallbackInfo* restrict callbackinfo, AzThreadId  id, AzRefAny  thread_initialize_data, AzRefAny  writeback_data, AzThreadCallback  callback);
extern DLLIMPORT void AzCallbackInfo_startTimer(AzCallbackInfo* restrict callbackinfo, AzTimerId  id, AzTimer  timer);
extern DLLIMPORT AzLogicalSize AzHidpiAdjustedBounds_getLogicalSize(AzHidpiAdjustedBounds* const hidpiadjustedbounds);
extern DLLIMPORT AzPhysicalSizeU32 AzHidpiAdjustedBounds_getPhysicalSize(AzHidpiAdjustedBounds* const hidpiadjustedbounds);
extern DLLIMPORT float AzHidpiAdjustedBounds_getHidpiFactor(AzHidpiAdjustedBounds* const hidpiadjustedbounds);
extern DLLIMPORT AzInlineTextHitVec AzInlineText_hitTest(AzInlineText* const inlinetext, AzLogicalPosition  position);
extern DLLIMPORT AzHidpiAdjustedBounds AzIFrameCallbackInfo_getBounds(AzIFrameCallbackInfo* const iframecallbackinfo);
extern DLLIMPORT AzOptionGl AzGlCallbackInfo_getGlContext(AzGlCallbackInfo* const glcallbackinfo);
extern DLLIMPORT AzHidpiAdjustedBounds AzGlCallbackInfo_getBounds(AzGlCallbackInfo* const glcallbackinfo);
extern DLLIMPORT AzDomNodeId AzGlCallbackInfo_getCallbackNodeId(AzGlCallbackInfo* const glcallbackinfo);
extern DLLIMPORT AzOptionInlineText AzGlCallbackInfo_getInlineText(AzGlCallbackInfo* const glcallbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzGlCallbackInfo_getParent(AzGlCallbackInfo* restrict glcallbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzGlCallbackInfo_getPreviousSibling(AzGlCallbackInfo* restrict glcallbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzGlCallbackInfo_getNextSibling(AzGlCallbackInfo* restrict glcallbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzGlCallbackInfo_getFirstChild(AzGlCallbackInfo* restrict glcallbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT AzOptionDomNodeId AzGlCallbackInfo_getLastChild(AzGlCallbackInfo* restrict glcallbackinfo, AzDomNodeId  node_id);
extern DLLIMPORT bool  AzRefCount_canBeShared(AzRefCount* const refcount);
extern DLLIMPORT bool  AzRefCount_canBeSharedMut(AzRefCount* const refcount);
extern DLLIMPORT void AzRefCount_increaseRef(AzRefCount* restrict refcount);
extern DLLIMPORT void AzRefCount_decreaseRef(AzRefCount* restrict refcount);
extern DLLIMPORT void AzRefCount_increaseRefmut(AzRefCount* restrict refcount);
extern DLLIMPORT void AzRefCount_decreaseRefmut(AzRefCount* restrict refcount);
extern DLLIMPORT void AzRefCount_delete(AzRefCount* restrict instance);
extern DLLIMPORT AzRefCount AzRefCount_deepCopy(AzRefCount* const instance);
extern DLLIMPORT AzRefAny AzRefAny_newC(void* ptr, size_t len, uint64_t type_id, AzString  type_name, AzRefAnyDestructorType  destructor);
extern DLLIMPORT bool  AzRefAny_isType(AzRefAny* const refany, uint64_t type_id);
extern DLLIMPORT AzString AzRefAny_getTypeName(AzRefAny* const refany);
extern DLLIMPORT AzRefAny AzRefAny_clone(AzRefAny* restrict refany);
extern DLLIMPORT void AzRefAny_delete(AzRefAny* restrict instance);
extern DLLIMPORT bool  AzLayoutInfo_windowWidthLargerThan(AzLayoutInfo* restrict layoutinfo, float width);
extern DLLIMPORT bool  AzLayoutInfo_windowWidthSmallerThan(AzLayoutInfo* restrict layoutinfo, float width);
extern DLLIMPORT bool  AzLayoutInfo_windowHeightLargerThan(AzLayoutInfo* restrict layoutinfo, float width);
extern DLLIMPORT bool  AzLayoutInfo_windowHeightSmallerThan(AzLayoutInfo* restrict layoutinfo, float width);
extern DLLIMPORT bool  AzLayoutInfo_usesDarkTheme(AzLayoutInfo* restrict layoutinfo);
extern DLLIMPORT size_t AzDom_nodeCount(AzDom* const dom);
extern DLLIMPORT AzStyledDom AzDom_style(const AzDom dom, AzCss  css);
extern DLLIMPORT AzEventFilter AzOn_intoEventFilter(const AzOn on);
extern DLLIMPORT AzCss AzCss_empty();
extern DLLIMPORT AzCss AzCss_fromString(AzString  s);
extern DLLIMPORT AzColorU AzColorU_fromStr(AzString  string);
extern DLLIMPORT AzString AzColorU_toHash(AzColorU* const coloru);
extern DLLIMPORT void AzCssPropertyCache_delete(AzCssPropertyCache* restrict instance);
extern DLLIMPORT AzCssPropertyCache AzCssPropertyCache_deepCopy(AzCssPropertyCache* const instance);
extern DLLIMPORT AzStyledDom AzStyledDom_new(AzDom  dom, AzCss  css);
extern DLLIMPORT AzStyledDom AzStyledDom_fromXml(AzString  xml_string);
extern DLLIMPORT AzStyledDom AzStyledDom_fromFile(AzString  xml_file_path);
extern DLLIMPORT void AzStyledDom_append(AzStyledDom* restrict styleddom, AzStyledDom  dom);
extern DLLIMPORT void AzStyledDom_restyle(AzStyledDom* restrict styleddom, AzCss  css);
extern DLLIMPORT size_t AzStyledDom_nodeCount(AzStyledDom* const styleddom);
extern DLLIMPORT AzString AzStyledDom_getHtmlString(AzStyledDom* const styleddom);
extern DLLIMPORT AzTexture AzTexture_allocateClipMask(AzGl  gl, AzLayoutSize  size);
extern DLLIMPORT bool  AzTexture_drawClipMask(AzTexture* restrict texture, AzTesselatedSvgNode * node);
extern DLLIMPORT bool  AzTexture_applyFxaa(AzTexture* restrict texture);
extern DLLIMPORT void AzTexture_delete(AzTexture* restrict instance);
extern DLLIMPORT AzGlType AzGl_getType(AzGl* const gl);
extern DLLIMPORT void AzGl_bufferDataUntyped(AzGl* const gl, uint32_t target, ssize_t size, void* data, uint32_t usage);
extern DLLIMPORT void AzGl_bufferSubDataUntyped(AzGl* const gl, uint32_t target, ssize_t offset, ssize_t size, void* data);
extern DLLIMPORT void AzGl_mapBuffer(AzGl* const gl, uint32_t target, uint32_t access);
extern DLLIMPORT void AzGl_mapBufferRange(AzGl* const gl, uint32_t target, ssize_t offset, ssize_t length, uint32_t access);
extern DLLIMPORT uint8_t AzGl_unmapBuffer(AzGl* const gl, uint32_t target);
extern DLLIMPORT void AzGl_texBuffer(AzGl* const gl, uint32_t target, uint32_t internal_format, uint32_t buffer);
extern DLLIMPORT void AzGl_shaderSource(AzGl* const gl, uint32_t shader, AzStringVec  strings);
extern DLLIMPORT void AzGl_readBuffer(AzGl* const gl, uint32_t mode);
extern DLLIMPORT void AzGl_readPixelsIntoBuffer(AzGl* const gl, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t pixel_type, AzU8VecRefMut  dst_buffer);
extern DLLIMPORT AzU8Vec AzGl_readPixels(AzGl* const gl, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t pixel_type);
extern DLLIMPORT void AzGl_readPixelsIntoPbo(AzGl* const gl, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t format, uint32_t pixel_type);
extern DLLIMPORT void AzGl_sampleCoverage(AzGl* const gl, float value, bool  invert);
extern DLLIMPORT void AzGl_polygonOffset(AzGl* const gl, float factor, float units);
extern DLLIMPORT void AzGl_pixelStoreI(AzGl* const gl, uint32_t name, int32_t param);
extern DLLIMPORT AzGLuintVec AzGl_genBuffers(AzGl* const gl, int32_t n);
extern DLLIMPORT AzGLuintVec AzGl_genRenderbuffers(AzGl* const gl, int32_t n);
extern DLLIMPORT AzGLuintVec AzGl_genFramebuffers(AzGl* const gl, int32_t n);
extern DLLIMPORT AzGLuintVec AzGl_genTextures(AzGl* const gl, int32_t n);
extern DLLIMPORT AzGLuintVec AzGl_genVertexArrays(AzGl* const gl, int32_t n);
extern DLLIMPORT AzGLuintVec AzGl_genQueries(AzGl* const gl, int32_t n);
extern DLLIMPORT void AzGl_beginQuery(AzGl* const gl, uint32_t target, uint32_t id);
extern DLLIMPORT void AzGl_endQuery(AzGl* const gl, uint32_t target);
extern DLLIMPORT void AzGl_queryCounter(AzGl* const gl, uint32_t id, uint32_t target);
extern DLLIMPORT int32_t AzGl_getQueryObjectIv(AzGl* const gl, uint32_t id, uint32_t pname);
extern DLLIMPORT uint32_t AzGl_getQueryObjectUiv(AzGl* const gl, uint32_t id, uint32_t pname);
extern DLLIMPORT int64_t AzGl_getQueryObjectI64V(AzGl* const gl, uint32_t id, uint32_t pname);
extern DLLIMPORT uint64_t AzGl_getQueryObjectUi64V(AzGl* const gl, uint32_t id, uint32_t pname);
extern DLLIMPORT void AzGl_deleteQueries(AzGl* const gl, AzGLuintVecRef  queries);
extern DLLIMPORT void AzGl_deleteVertexArrays(AzGl* const gl, AzGLuintVecRef  vertex_arrays);
extern DLLIMPORT void AzGl_deleteBuffers(AzGl* const gl, AzGLuintVecRef  buffers);
extern DLLIMPORT void AzGl_deleteRenderbuffers(AzGl* const gl, AzGLuintVecRef  renderbuffers);
extern DLLIMPORT void AzGl_deleteFramebuffers(AzGl* const gl, AzGLuintVecRef  framebuffers);
extern DLLIMPORT void AzGl_deleteTextures(AzGl* const gl, AzGLuintVecRef  textures);
extern DLLIMPORT void AzGl_framebufferRenderbuffer(AzGl* const gl, uint32_t target, uint32_t attachment, uint32_t renderbuffertarget, uint32_t renderbuffer);
extern DLLIMPORT void AzGl_renderbufferStorage(AzGl* const gl, uint32_t target, uint32_t internalformat, int32_t width, int32_t height);
extern DLLIMPORT void AzGl_depthFunc(AzGl* const gl, uint32_t func);
extern DLLIMPORT void AzGl_activeTexture(AzGl* const gl, uint32_t texture);
extern DLLIMPORT void AzGl_attachShader(AzGl* const gl, uint32_t program, uint32_t shader);
extern DLLIMPORT void AzGl_bindAttribLocation(AzGl* const gl, uint32_t program, uint32_t index, AzRefstr  name);
extern DLLIMPORT void AzGl_getUniformIv(AzGl* const gl, uint32_t program, int32_t location, AzGLintVecRefMut  result);
extern DLLIMPORT void AzGl_getUniformFv(AzGl* const gl, uint32_t program, int32_t location, AzGLfloatVecRefMut  result);
extern DLLIMPORT uint32_t AzGl_getUniformBlockIndex(AzGl* const gl, uint32_t program, AzRefstr  name);
extern DLLIMPORT AzGLuintVec AzGl_getUniformIndices(AzGl* const gl, uint32_t program, AzRefstrVecRef  names);
extern DLLIMPORT void AzGl_bindBufferBase(AzGl* const gl, uint32_t target, uint32_t index, uint32_t buffer);
extern DLLIMPORT void AzGl_bindBufferRange(AzGl* const gl, uint32_t target, uint32_t index, uint32_t buffer, ssize_t offset, ssize_t size);
extern DLLIMPORT void AzGl_uniformBlockBinding(AzGl* const gl, uint32_t program, uint32_t uniform_block_index, uint32_t uniform_block_binding);
extern DLLIMPORT void AzGl_bindBuffer(AzGl* const gl, uint32_t target, uint32_t buffer);
extern DLLIMPORT void AzGl_bindVertexArray(AzGl* const gl, uint32_t vao);
extern DLLIMPORT void AzGl_bindRenderbuffer(AzGl* const gl, uint32_t target, uint32_t renderbuffer);
extern DLLIMPORT void AzGl_bindFramebuffer(AzGl* const gl, uint32_t target, uint32_t framebuffer);
extern DLLIMPORT void AzGl_bindTexture(AzGl* const gl, uint32_t target, uint32_t texture);
extern DLLIMPORT void AzGl_drawBuffers(AzGl* const gl, AzGLenumVecRef  bufs);
extern DLLIMPORT void AzGl_texImage2D(AzGl* const gl, uint32_t target, int32_t level, int32_t internal_format, int32_t width, int32_t height, int32_t border, uint32_t format, uint32_t ty, AzOptionU8VecRef  opt_data);
extern DLLIMPORT void AzGl_compressedTexImage2D(AzGl* const gl, uint32_t target, int32_t level, uint32_t internal_format, int32_t width, int32_t height, int32_t border, AzU8VecRef  data);
extern DLLIMPORT void AzGl_compressedTexSubImage2D(AzGl* const gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height, uint32_t format, AzU8VecRef  data);
extern DLLIMPORT void AzGl_texImage3D(AzGl* const gl, uint32_t target, int32_t level, int32_t internal_format, int32_t width, int32_t height, int32_t depth, int32_t border, uint32_t format, uint32_t ty, AzOptionU8VecRef  opt_data);
extern DLLIMPORT void AzGl_copyTexImage2D(AzGl* const gl, uint32_t target, int32_t level, uint32_t internal_format, int32_t x, int32_t y, int32_t width, int32_t height, int32_t border);
extern DLLIMPORT void AzGl_copyTexSubImage2D(AzGl* const gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t x, int32_t y, int32_t width, int32_t height);
extern DLLIMPORT void AzGl_copyTexSubImage3D(AzGl* const gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t zoffset, int32_t x, int32_t y, int32_t width, int32_t height);
extern DLLIMPORT void AzGl_texSubImage2D(AzGl* const gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height, uint32_t format, uint32_t ty, AzU8VecRef  data);
extern DLLIMPORT void AzGl_texSubImage2DPbo(AzGl* const gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height, uint32_t format, uint32_t ty, size_t offset);
extern DLLIMPORT void AzGl_texSubImage3D(AzGl* const gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t zoffset, int32_t width, int32_t height, int32_t depth, uint32_t format, uint32_t ty, AzU8VecRef  data);
extern DLLIMPORT void AzGl_texSubImage3DPbo(AzGl* const gl, uint32_t target, int32_t level, int32_t xoffset, int32_t yoffset, int32_t zoffset, int32_t width, int32_t height, int32_t depth, uint32_t format, uint32_t ty, size_t offset);
extern DLLIMPORT void AzGl_texStorage2D(AzGl* const gl, uint32_t target, int32_t levels, uint32_t internal_format, int32_t width, int32_t height);
extern DLLIMPORT void AzGl_texStorage3D(AzGl* const gl, uint32_t target, int32_t levels, uint32_t internal_format, int32_t width, int32_t height, int32_t depth);
extern DLLIMPORT void AzGl_getTexImageIntoBuffer(AzGl* const gl, uint32_t target, int32_t level, uint32_t format, uint32_t ty, AzU8VecRefMut  output);
extern DLLIMPORT void AzGl_copyImageSubData(AzGl* const gl, uint32_t src_name, uint32_t src_target, int32_t src_level, int32_t src_x, int32_t src_y, int32_t src_z, uint32_t dst_name, uint32_t dst_target, int32_t dst_level, int32_t dst_x, int32_t dst_y, int32_t dst_z, int32_t src_width, int32_t src_height, int32_t src_depth);
extern DLLIMPORT void AzGl_invalidateFramebuffer(AzGl* const gl, uint32_t target, AzGLenumVecRef  attachments);
extern DLLIMPORT void AzGl_invalidateSubFramebuffer(AzGl* const gl, uint32_t target, AzGLenumVecRef  attachments, int32_t xoffset, int32_t yoffset, int32_t width, int32_t height);
extern DLLIMPORT void AzGl_getIntegerV(AzGl* const gl, uint32_t name, AzGLintVecRefMut  result);
extern DLLIMPORT void AzGl_getInteger64V(AzGl* const gl, uint32_t name, AzGLint64VecRefMut  result);
extern DLLIMPORT void AzGl_getIntegerIv(AzGl* const gl, uint32_t name, uint32_t index, AzGLintVecRefMut  result);
extern DLLIMPORT void AzGl_getInteger64Iv(AzGl* const gl, uint32_t name, uint32_t index, AzGLint64VecRefMut  result);
extern DLLIMPORT void AzGl_getBooleanV(AzGl* const gl, uint32_t name, AzGLbooleanVecRefMut  result);
extern DLLIMPORT void AzGl_getFloatV(AzGl* const gl, uint32_t name, AzGLfloatVecRefMut  result);
extern DLLIMPORT int32_t AzGl_getFramebufferAttachmentParameterIv(AzGl* const gl, uint32_t target, uint32_t attachment, uint32_t pname);
extern DLLIMPORT int32_t AzGl_getRenderbufferParameterIv(AzGl* const gl, uint32_t target, uint32_t pname);
extern DLLIMPORT int32_t AzGl_getTexParameterIv(AzGl* const gl, uint32_t target, uint32_t name);
extern DLLIMPORT float AzGl_getTexParameterFv(AzGl* const gl, uint32_t target, uint32_t name);
extern DLLIMPORT void AzGl_texParameterI(AzGl* const gl, uint32_t target, uint32_t pname, int32_t param);
extern DLLIMPORT void AzGl_texParameterF(AzGl* const gl, uint32_t target, uint32_t pname, float param);
extern DLLIMPORT void AzGl_framebufferTexture2D(AzGl* const gl, uint32_t target, uint32_t attachment, uint32_t textarget, uint32_t texture, int32_t level);
extern DLLIMPORT void AzGl_framebufferTextureLayer(AzGl* const gl, uint32_t target, uint32_t attachment, uint32_t texture, int32_t level, int32_t layer);
extern DLLIMPORT void AzGl_blitFramebuffer(AzGl* const gl, int32_t src_x0, int32_t src_y0, int32_t src_x1, int32_t src_y1, int32_t dst_x0, int32_t dst_y0, int32_t dst_x1, int32_t dst_y1, uint32_t mask, uint32_t filter);
extern DLLIMPORT void AzGl_vertexAttrib4F(AzGl* const gl, uint32_t index, float x, float y, float z, float w);
extern DLLIMPORT void AzGl_vertexAttribPointerF32(AzGl* const gl, uint32_t index, int32_t size, bool  normalized, int32_t stride, uint32_t offset);
extern DLLIMPORT void AzGl_vertexAttribPointer(AzGl* const gl, uint32_t index, int32_t size, uint32_t type_, bool  normalized, int32_t stride, uint32_t offset);
extern DLLIMPORT void AzGl_vertexAttribIPointer(AzGl* const gl, uint32_t index, int32_t size, uint32_t type_, int32_t stride, uint32_t offset);
extern DLLIMPORT void AzGl_vertexAttribDivisor(AzGl* const gl, uint32_t index, uint32_t divisor);
extern DLLIMPORT void AzGl_viewport(AzGl* const gl, int32_t x, int32_t y, int32_t width, int32_t height);
extern DLLIMPORT void AzGl_scissor(AzGl* const gl, int32_t x, int32_t y, int32_t width, int32_t height);
extern DLLIMPORT void AzGl_lineWidth(AzGl* const gl, float width);
extern DLLIMPORT void AzGl_useProgram(AzGl* const gl, uint32_t program);
extern DLLIMPORT void AzGl_validateProgram(AzGl* const gl, uint32_t program);
extern DLLIMPORT void AzGl_drawArrays(AzGl* const gl, uint32_t mode, int32_t first, int32_t count);
extern DLLIMPORT void AzGl_drawArraysInstanced(AzGl* const gl, uint32_t mode, int32_t first, int32_t count, int32_t primcount);
extern DLLIMPORT void AzGl_drawElements(AzGl* const gl, uint32_t mode, int32_t count, uint32_t element_type, uint32_t indices_offset);
extern DLLIMPORT void AzGl_drawElementsInstanced(AzGl* const gl, uint32_t mode, int32_t count, uint32_t element_type, uint32_t indices_offset, int32_t primcount);
extern DLLIMPORT void AzGl_blendColor(AzGl* const gl, float r, float g, float b, float a);
extern DLLIMPORT void AzGl_blendFunc(AzGl* const gl, uint32_t sfactor, uint32_t dfactor);
extern DLLIMPORT void AzGl_blendFuncSeparate(AzGl* const gl, uint32_t src_rgb, uint32_t dest_rgb, uint32_t src_alpha, uint32_t dest_alpha);
extern DLLIMPORT void AzGl_blendEquation(AzGl* const gl, uint32_t mode);
extern DLLIMPORT void AzGl_blendEquationSeparate(AzGl* const gl, uint32_t mode_rgb, uint32_t mode_alpha);
extern DLLIMPORT void AzGl_colorMask(AzGl* const gl, bool  r, bool  g, bool  b, bool  a);
extern DLLIMPORT void AzGl_cullFace(AzGl* const gl, uint32_t mode);
extern DLLIMPORT void AzGl_frontFace(AzGl* const gl, uint32_t mode);
extern DLLIMPORT void AzGl_enable(AzGl* const gl, uint32_t cap);
extern DLLIMPORT void AzGl_disable(AzGl* const gl, uint32_t cap);
extern DLLIMPORT void AzGl_hint(AzGl* const gl, uint32_t param_name, uint32_t param_val);
extern DLLIMPORT uint8_t AzGl_isEnabled(AzGl* const gl, uint32_t cap);
extern DLLIMPORT uint8_t AzGl_isShader(AzGl* const gl, uint32_t shader);
extern DLLIMPORT uint8_t AzGl_isTexture(AzGl* const gl, uint32_t texture);
extern DLLIMPORT uint8_t AzGl_isFramebuffer(AzGl* const gl, uint32_t framebuffer);
extern DLLIMPORT uint8_t AzGl_isRenderbuffer(AzGl* const gl, uint32_t renderbuffer);
extern DLLIMPORT uint32_t AzGl_checkFrameBufferStatus(AzGl* const gl, uint32_t target);
extern DLLIMPORT void AzGl_enableVertexAttribArray(AzGl* const gl, uint32_t index);
extern DLLIMPORT void AzGl_disableVertexAttribArray(AzGl* const gl, uint32_t index);
extern DLLIMPORT void AzGl_uniform1F(AzGl* const gl, int32_t location, float v0);
extern DLLIMPORT void AzGl_uniform1Fv(AzGl* const gl, int32_t location, AzF32VecRef  values);
extern DLLIMPORT void AzGl_uniform1I(AzGl* const gl, int32_t location, int32_t v0);
extern DLLIMPORT void AzGl_uniform1Iv(AzGl* const gl, int32_t location, AzI32VecRef  values);
extern DLLIMPORT void AzGl_uniform1Ui(AzGl* const gl, int32_t location, uint32_t v0);
extern DLLIMPORT void AzGl_uniform2F(AzGl* const gl, int32_t location, float v0, float v1);
extern DLLIMPORT void AzGl_uniform2Fv(AzGl* const gl, int32_t location, AzF32VecRef  values);
extern DLLIMPORT void AzGl_uniform2I(AzGl* const gl, int32_t location, int32_t v0, int32_t v1);
extern DLLIMPORT void AzGl_uniform2Iv(AzGl* const gl, int32_t location, AzI32VecRef  values);
extern DLLIMPORT void AzGl_uniform2Ui(AzGl* const gl, int32_t location, uint32_t v0, uint32_t v1);
extern DLLIMPORT void AzGl_uniform3F(AzGl* const gl, int32_t location, float v0, float v1, float v2);
extern DLLIMPORT void AzGl_uniform3Fv(AzGl* const gl, int32_t location, AzF32VecRef  values);
extern DLLIMPORT void AzGl_uniform3I(AzGl* const gl, int32_t location, int32_t v0, int32_t v1, int32_t v2);
extern DLLIMPORT void AzGl_uniform3Iv(AzGl* const gl, int32_t location, AzI32VecRef  values);
extern DLLIMPORT void AzGl_uniform3Ui(AzGl* const gl, int32_t location, uint32_t v0, uint32_t v1, uint32_t v2);
extern DLLIMPORT void AzGl_uniform4F(AzGl* const gl, int32_t location, float x, float y, float z, float w);
extern DLLIMPORT void AzGl_uniform4I(AzGl* const gl, int32_t location, int32_t x, int32_t y, int32_t z, int32_t w);
extern DLLIMPORT void AzGl_uniform4Iv(AzGl* const gl, int32_t location, AzI32VecRef  values);
extern DLLIMPORT void AzGl_uniform4Ui(AzGl* const gl, int32_t location, uint32_t x, uint32_t y, uint32_t z, uint32_t w);
extern DLLIMPORT void AzGl_uniform4Fv(AzGl* const gl, int32_t location, AzF32VecRef  values);
extern DLLIMPORT void AzGl_uniformMatrix2Fv(AzGl* const gl, int32_t location, bool  transpose, AzF32VecRef  value);
extern DLLIMPORT void AzGl_uniformMatrix3Fv(AzGl* const gl, int32_t location, bool  transpose, AzF32VecRef  value);
extern DLLIMPORT void AzGl_uniformMatrix4Fv(AzGl* const gl, int32_t location, bool  transpose, AzF32VecRef  value);
extern DLLIMPORT void AzGl_depthMask(AzGl* const gl, bool  flag);
extern DLLIMPORT void AzGl_depthRange(AzGl* const gl, double near, double far);
extern DLLIMPORT AzGetActiveAttribReturn AzGl_getActiveAttrib(AzGl* const gl, uint32_t program, uint32_t index);
extern DLLIMPORT AzGetActiveUniformReturn AzGl_getActiveUniform(AzGl* const gl, uint32_t program, uint32_t index);
extern DLLIMPORT AzGLintVec AzGl_getActiveUniformsIv(AzGl* const gl, uint32_t program, AzGLuintVec  indices, uint32_t pname);
extern DLLIMPORT int32_t AzGl_getActiveUniformBlockI(AzGl* const gl, uint32_t program, uint32_t index, uint32_t pname);
extern DLLIMPORT AzGLintVec AzGl_getActiveUniformBlockIv(AzGl* const gl, uint32_t program, uint32_t index, uint32_t pname);
extern DLLIMPORT AzString AzGl_getActiveUniformBlockName(AzGl* const gl, uint32_t program, uint32_t index);
extern DLLIMPORT int32_t AzGl_getAttribLocation(AzGl* const gl, uint32_t program, AzRefstr  name);
extern DLLIMPORT int32_t AzGl_getFragDataLocation(AzGl* const gl, uint32_t program, AzRefstr  name);
extern DLLIMPORT int32_t AzGl_getUniformLocation(AzGl* const gl, uint32_t program, AzRefstr  name);
extern DLLIMPORT AzString AzGl_getProgramInfoLog(AzGl* const gl, uint32_t program);
extern DLLIMPORT void AzGl_getProgramIv(AzGl* const gl, uint32_t program, uint32_t pname, AzGLintVecRefMut  result);
extern DLLIMPORT AzGetProgramBinaryReturn AzGl_getProgramBinary(AzGl* const gl, uint32_t program);
extern DLLIMPORT void AzGl_programBinary(AzGl* const gl, uint32_t program, uint32_t format, AzU8VecRef  binary);
extern DLLIMPORT void AzGl_programParameterI(AzGl* const gl, uint32_t program, uint32_t pname, int32_t value);
extern DLLIMPORT void AzGl_getVertexAttribIv(AzGl* const gl, uint32_t index, uint32_t pname, AzGLintVecRefMut  result);
extern DLLIMPORT void AzGl_getVertexAttribFv(AzGl* const gl, uint32_t index, uint32_t pname, AzGLfloatVecRefMut  result);
extern DLLIMPORT ssize_t AzGl_getVertexAttribPointerV(AzGl* const gl, uint32_t index, uint32_t pname);
extern DLLIMPORT int32_t AzGl_getBufferParameterIv(AzGl* const gl, uint32_t target, uint32_t pname);
extern DLLIMPORT AzString AzGl_getShaderInfoLog(AzGl* const gl, uint32_t shader);
extern DLLIMPORT AzString AzGl_getString(AzGl* const gl, uint32_t which);
extern DLLIMPORT AzString AzGl_getStringI(AzGl* const gl, uint32_t which, uint32_t index);
extern DLLIMPORT void AzGl_getShaderIv(AzGl* const gl, uint32_t shader, uint32_t pname, AzGLintVecRefMut  result);
extern DLLIMPORT AzGlShaderPrecisionFormatReturn AzGl_getShaderPrecisionFormat(AzGl* const gl, uint32_t shader_type, uint32_t precision_type);
extern DLLIMPORT void AzGl_compileShader(AzGl* const gl, uint32_t shader);
extern DLLIMPORT uint32_t AzGl_createProgram(AzGl* const gl);
extern DLLIMPORT void AzGl_deleteProgram(AzGl* const gl, uint32_t program);
extern DLLIMPORT uint32_t AzGl_createShader(AzGl* const gl, uint32_t shader_type);
extern DLLIMPORT void AzGl_deleteShader(AzGl* const gl, uint32_t shader);
extern DLLIMPORT void AzGl_detachShader(AzGl* const gl, uint32_t program, uint32_t shader);
extern DLLIMPORT void AzGl_linkProgram(AzGl* const gl, uint32_t program);
extern DLLIMPORT void AzGl_clearColor(AzGl* const gl, float r, float g, float b, float a);
extern DLLIMPORT void AzGl_clear(AzGl* const gl, uint32_t buffer_mask);
extern DLLIMPORT void AzGl_clearDepth(AzGl* const gl, double depth);
extern DLLIMPORT void AzGl_clearStencil(AzGl* const gl, int32_t s);
extern DLLIMPORT void AzGl_flush(AzGl* const gl);
extern DLLIMPORT void AzGl_finish(AzGl* const gl);
extern DLLIMPORT uint32_t AzGl_getError(AzGl* const gl);
extern DLLIMPORT void AzGl_stencilMask(AzGl* const gl, uint32_t mask);
extern DLLIMPORT void AzGl_stencilMaskSeparate(AzGl* const gl, uint32_t face, uint32_t mask);
extern DLLIMPORT void AzGl_stencilFunc(AzGl* const gl, uint32_t func, int32_t ref_, uint32_t mask);
extern DLLIMPORT void AzGl_stencilFuncSeparate(AzGl* const gl, uint32_t face, uint32_t func, int32_t ref_, uint32_t mask);
extern DLLIMPORT void AzGl_stencilOp(AzGl* const gl, uint32_t sfail, uint32_t dpfail, uint32_t dppass);
extern DLLIMPORT void AzGl_stencilOpSeparate(AzGl* const gl, uint32_t face, uint32_t sfail, uint32_t dpfail, uint32_t dppass);
extern DLLIMPORT void AzGl_eglImageTargetTexture2DOes(AzGl* const gl, uint32_t target, void* image);
extern DLLIMPORT void AzGl_generateMipmap(AzGl* const gl, uint32_t target);
extern DLLIMPORT void AzGl_insertEventMarkerExt(AzGl* const gl, AzRefstr  message);
extern DLLIMPORT void AzGl_pushGroupMarkerExt(AzGl* const gl, AzRefstr  message);
extern DLLIMPORT void AzGl_popGroupMarkerExt(AzGl* const gl);
extern DLLIMPORT void AzGl_debugMessageInsertKhr(AzGl* const gl, uint32_t source, uint32_t type_, uint32_t id, uint32_t severity, AzRefstr  message);
extern DLLIMPORT void AzGl_pushDebugGroupKhr(AzGl* const gl, uint32_t source, uint32_t id, AzRefstr  message);
extern DLLIMPORT void AzGl_popDebugGroupKhr(AzGl* const gl);
extern DLLIMPORT AzGLsyncPtr AzGl_fenceSync(AzGl* const gl, uint32_t condition, uint32_t flags);
extern DLLIMPORT uint32_t AzGl_clientWaitSync(AzGl* const gl, AzGLsyncPtr  sync, uint32_t flags, uint64_t timeout);
extern DLLIMPORT void AzGl_waitSync(AzGl* const gl, AzGLsyncPtr  sync, uint32_t flags, uint64_t timeout);
extern DLLIMPORT void AzGl_deleteSync(AzGl* const gl, AzGLsyncPtr  sync);
extern DLLIMPORT void AzGl_textureRangeApple(AzGl* const gl, uint32_t target, AzU8VecRef  data);
extern DLLIMPORT AzGLuintVec AzGl_genFencesApple(AzGl* const gl, int32_t n);
extern DLLIMPORT void AzGl_deleteFencesApple(AzGl* const gl, AzGLuintVecRef  fences);
extern DLLIMPORT void AzGl_setFenceApple(AzGl* const gl, uint32_t fence);
extern DLLIMPORT void AzGl_finishFenceApple(AzGl* const gl, uint32_t fence);
extern DLLIMPORT void AzGl_testFenceApple(AzGl* const gl, uint32_t fence);
extern DLLIMPORT uint8_t AzGl_testObjectApple(AzGl* const gl, uint32_t object, uint32_t name);
extern DLLIMPORT void AzGl_finishObjectApple(AzGl* const gl, uint32_t object, uint32_t name);
extern DLLIMPORT int32_t AzGl_getFragDataIndex(AzGl* const gl, uint32_t program, AzRefstr  name);
extern DLLIMPORT void AzGl_blendBarrierKhr(AzGl* const gl);
extern DLLIMPORT void AzGl_bindFragDataLocationIndexed(AzGl* const gl, uint32_t program, uint32_t color_number, uint32_t index, AzRefstr  name);
extern DLLIMPORT AzDebugMessageVec AzGl_getDebugMessages(AzGl* const gl);
extern DLLIMPORT void AzGl_provokingVertexAngle(AzGl* const gl, uint32_t mode);
extern DLLIMPORT AzGLuintVec AzGl_genVertexArraysApple(AzGl* const gl, int32_t n);
extern DLLIMPORT void AzGl_bindVertexArrayApple(AzGl* const gl, uint32_t vao);
extern DLLIMPORT void AzGl_deleteVertexArraysApple(AzGl* const gl, AzGLuintVecRef  vertex_arrays);
extern DLLIMPORT void AzGl_copyTextureChromium(AzGl* const gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t internal_format, uint32_t dest_type, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
extern DLLIMPORT void AzGl_copySubTextureChromium(AzGl* const gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t x_offset, int32_t y_offset, int32_t x, int32_t y, int32_t width, int32_t height, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
extern DLLIMPORT void AzGl_eglImageTargetRenderbufferStorageOes(AzGl* const gl, uint32_t target, void* image);
extern DLLIMPORT void AzGl_copyTexture3DAngle(AzGl* const gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t internal_format, uint32_t dest_type, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
extern DLLIMPORT void AzGl_copySubTexture3DAngle(AzGl* const gl, uint32_t source_id, int32_t source_level, uint32_t dest_target, uint32_t dest_id, int32_t dest_level, int32_t x_offset, int32_t y_offset, int32_t z_offset, int32_t x, int32_t y, int32_t z, int32_t width, int32_t height, int32_t depth, uint8_t unpack_flip_y, uint8_t unpack_premultiply_alpha, uint8_t unpack_unmultiply_alpha);
extern DLLIMPORT void AzGl_bufferStorage(AzGl* const gl, uint32_t target, ssize_t size, void* data, uint32_t flags);
extern DLLIMPORT void AzGl_flushMappedBufferRange(AzGl* const gl, uint32_t target, ssize_t offset, ssize_t length);
extern DLLIMPORT void AzGl_delete(AzGl* restrict instance);
extern DLLIMPORT AzGl AzGl_deepCopy(AzGl* const instance);
extern DLLIMPORT void AzGLsyncPtr_delete(AzGLsyncPtr* restrict instance);
extern DLLIMPORT AzTextureFlags AzTextureFlags_default();
extern DLLIMPORT AzRawImage AzRawImage_empty();
extern DLLIMPORT AzRawImage AzRawImage_allocateClipMask(AzLayoutSize  size);
extern DLLIMPORT AzRawImage AzRawImage_decodeImageBytesAny(AzU8VecRef  bytes);
extern DLLIMPORT bool  AzRawImage_drawClipMask(AzRawImage* restrict rawimage, AzSvgNode * node, AzSvgStyle  style);
extern DLLIMPORT AzResultU8VecEncodeImageError AzRawImage_encodeBmp(AzRawImage* const rawimage);
extern DLLIMPORT AzResultU8VecEncodeImageError AzRawImage_encodePng(AzRawImage* const rawimage);
extern DLLIMPORT AzResultU8VecEncodeImageError AzRawImage_encodeJpeg(AzRawImage* const rawimage);
extern DLLIMPORT AzResultU8VecEncodeImageError AzRawImage_encodeTga(AzRawImage* const rawimage);
extern DLLIMPORT AzResultU8VecEncodeImageError AzRawImage_encodePnm(AzRawImage* const rawimage);
extern DLLIMPORT AzResultU8VecEncodeImageError AzRawImage_encodeGif(AzRawImage* const rawimage);
extern DLLIMPORT AzResultU8VecEncodeImageError AzRawImage_encodeTiff(AzRawImage* const rawimage);
extern DLLIMPORT AzSvg AzSvg_fromString(AzString  svg_string, AzSvgParseOptions  parse_options);
extern DLLIMPORT AzSvg AzSvg_fromBytes(AzU8VecRef  svg_bytes, AzSvgParseOptions  parse_options);
extern DLLIMPORT AzSvgXmlNode AzSvg_getRoot(AzSvg* const svg);
extern DLLIMPORT AzOptionRawImage AzSvg_render(AzSvg* const svg, AzSvgRenderOptions  options);
extern DLLIMPORT AzString AzSvg_toString(AzSvg* const svg, AzSvgStringFormatOptions  options);
extern DLLIMPORT void AzSvg_delete(AzSvg* restrict instance);
extern DLLIMPORT AzSvg AzSvg_deepCopy(AzSvg* const instance);
extern DLLIMPORT AzSvgXmlNode AzSvgXmlNode_parseFrom(AzU8VecRef  svg_bytes, AzSvgParseOptions  parse_options);
extern DLLIMPORT AzOptionRawImage AzSvgXmlNode_render(AzSvgXmlNode* const svgxmlnode, AzSvgRenderOptions  options);
extern DLLIMPORT AzString AzSvgXmlNode_toString(AzSvgXmlNode* const svgxmlnode, AzSvgStringFormatOptions  options);
extern DLLIMPORT void AzSvgXmlNode_delete(AzSvgXmlNode* restrict instance);
extern DLLIMPORT AzSvgXmlNode AzSvgXmlNode_deepCopy(AzSvgXmlNode* const instance);
extern DLLIMPORT AzTesselatedSvgNode AzSvgMultiPolygon_tesselateFill(AzSvgMultiPolygon* const svgmultipolygon, AzSvgFillStyle  fill_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgMultiPolygon_tesselateStroke(AzSvgMultiPolygon* const svgmultipolygon, AzSvgStrokeStyle  stroke_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgNode_tesselateFill(AzSvgNode* const svgnode, AzSvgFillStyle  fill_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgNode_tesselateStroke(AzSvgNode* const svgnode, AzSvgStrokeStyle  stroke_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgStyledNode_tesselate(AzSvgStyledNode* const svgstylednode);
extern DLLIMPORT AzTesselatedSvgNode AzSvgCircle_tesselateFill(AzSvgCircle* const svgcircle, AzSvgFillStyle  fill_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgCircle_tesselateStroke(AzSvgCircle* const svgcircle, AzSvgStrokeStyle  stroke_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgPath_tesselateFill(AzSvgPath* const svgpath, AzSvgFillStyle  fill_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgPath_tesselateStroke(AzSvgPath* const svgpath, AzSvgStrokeStyle  stroke_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgRect_tesselateFill(AzSvgRect* const svgrect, AzSvgFillStyle  fill_style);
extern DLLIMPORT AzTesselatedSvgNode AzSvgRect_tesselateStroke(AzSvgRect* const svgrect, AzSvgStrokeStyle  stroke_style);
extern DLLIMPORT AzTesselatedSvgNode AzTesselatedSvgNode_empty();
extern DLLIMPORT AzTesselatedSvgNode AzTesselatedSvgNode_fromNodes(AzTesselatedSvgNodeVecRef  nodes);
extern DLLIMPORT AzSvgParseOptions AzSvgParseOptions_default();
extern DLLIMPORT AzSvgRenderOptions AzSvgRenderOptions_default();
extern DLLIMPORT AzXml AzXml_fromStr(AzRefstr  xml_string);
extern DLLIMPORT AzFile AzFile_open(AzString  path);
extern DLLIMPORT AzFile AzFile_create(AzString  path);
extern DLLIMPORT AzOptionString AzFile_readToString(AzFile* restrict file);
extern DLLIMPORT AzOptionU8Vec AzFile_readToBytes(AzFile* restrict file);
extern DLLIMPORT bool  AzFile_writeString(AzFile* restrict file, AzRefstr  bytes);
extern DLLIMPORT bool  AzFile_writeBytes(AzFile* restrict file, AzU8VecRef  bytes);
extern DLLIMPORT void AzFile_close(const AzFile file);
extern DLLIMPORT void AzFile_delete(AzFile* restrict instance);
extern DLLIMPORT AzMsgBox AzMsgBox_ok(AzMsgBoxIcon  icon, AzString  title, AzString  message);
extern DLLIMPORT AzMsgBox AzMsgBox_okCancel(AzMsgBoxIcon  icon, AzString  title, AzString  message, AzMsgBoxOkCancel  default_value);
extern DLLIMPORT AzMsgBox AzMsgBox_yesNo(AzMsgBoxIcon  icon, AzString  title, AzString  message, AzMsgBoxYesNo  default_value);
extern DLLIMPORT AzFileDialog AzFileDialog_selectFile(AzString  title, AzOptionString  default_path, AzOptionFileTypeList  filter_list);
extern DLLIMPORT AzFileDialog AzFileDialog_selectMultipleFiles(AzString  title, AzOptionString  default_path, AzOptionFileTypeList  filter_list);
extern DLLIMPORT AzFileDialog AzFileDialog_selectFolder(AzString  title, AzOptionString  default_path);
extern DLLIMPORT AzFileDialog AzFileDialog_saveFile(AzString  title, AzOptionString  default_path);
extern DLLIMPORT AzColorPickerDialog AzColorPickerDialog_open(AzString  title, AzOptionColorU  default_color);
extern DLLIMPORT AzTimerId AzTimerId_unique();
extern DLLIMPORT AzTimer AzTimer_new(AzRefAny  timer_data, AzTimerCallbackType  callback, AzGetSystemTimeFn  get_system_time_fn);
extern DLLIMPORT AzTimer AzTimer_withDelay(const AzTimer timer, AzDuration  delay);
extern DLLIMPORT AzTimer AzTimer_withInterval(const AzTimer timer, AzDuration  interval);
extern DLLIMPORT AzTimer AzTimer_withTimeout(const AzTimer timer, AzDuration  timeout);
extern DLLIMPORT bool  AzThreadSender_send(AzThreadSender* restrict threadsender, AzThreadReceiveMsg  msg);
extern DLLIMPORT void AzThreadSender_delete(AzThreadSender* restrict instance);
extern DLLIMPORT AzOptionThreadSendMsg AzThreadReceiver_receive(AzThreadReceiver* restrict threadreceiver);
extern DLLIMPORT void AzThreadReceiver_delete(AzThreadReceiver* restrict instance);
extern DLLIMPORT AzString AzString_format(AzString  format, AzFmtArgVec  args);
extern DLLIMPORT AzString AzString_trim(AzString* const string);
extern DLLIMPORT AzRefstr AzString_asRefstr(AzString* const string);
extern DLLIMPORT AzTesselatedSvgNodeVecRef AzTesselatedSvgNodeVec_asRefVec(AzTesselatedSvgNodeVec* const tesselatedsvgnodevec);
extern DLLIMPORT void AzTesselatedSvgNodeVec_delete(AzTesselatedSvgNodeVec* restrict instance);
extern DLLIMPORT void AzXmlNodeVec_delete(AzXmlNodeVec* restrict instance);
extern DLLIMPORT void AzFmtArgVec_delete(AzFmtArgVec* restrict instance);
extern DLLIMPORT void AzInlineLineVec_delete(AzInlineLineVec* restrict instance);
extern DLLIMPORT void AzInlineWordVec_delete(AzInlineWordVec* restrict instance);
extern DLLIMPORT void AzInlineGlyphVec_delete(AzInlineGlyphVec* restrict instance);
extern DLLIMPORT void AzInlineTextHitVec_delete(AzInlineTextHitVec* restrict instance);
extern DLLIMPORT void AzMonitorVec_delete(AzMonitorVec* restrict instance);
extern DLLIMPORT void AzVideoModeVec_delete(AzVideoModeVec* restrict instance);
extern DLLIMPORT void AzDomVec_delete(AzDomVec* restrict instance);
extern DLLIMPORT void AzIdOrClassVec_delete(AzIdOrClassVec* restrict instance);
extern DLLIMPORT void AzNodeDataInlineCssPropertyVec_delete(AzNodeDataInlineCssPropertyVec* restrict instance);
extern DLLIMPORT void AzStyleBackgroundContentVec_delete(AzStyleBackgroundContentVec* restrict instance);
extern DLLIMPORT void AzStyleBackgroundPositionVec_delete(AzStyleBackgroundPositionVec* restrict instance);
extern DLLIMPORT void AzStyleBackgroundRepeatVec_delete(AzStyleBackgroundRepeatVec* restrict instance);
extern DLLIMPORT void AzStyleBackgroundSizeVec_delete(AzStyleBackgroundSizeVec* restrict instance);
extern DLLIMPORT void AzStyleTransformVec_delete(AzStyleTransformVec* restrict instance);
extern DLLIMPORT void AzCssPropertyVec_delete(AzCssPropertyVec* restrict instance);
extern DLLIMPORT void AzSvgMultiPolygonVec_delete(AzSvgMultiPolygonVec* restrict instance);
extern DLLIMPORT void AzSvgPathVec_delete(AzSvgPathVec* restrict instance);
extern DLLIMPORT void AzVertexAttributeVec_delete(AzVertexAttributeVec* restrict instance);
extern DLLIMPORT void AzSvgPathElementVec_delete(AzSvgPathElementVec* restrict instance);
extern DLLIMPORT void AzSvgVertexVec_delete(AzSvgVertexVec* restrict instance);
extern DLLIMPORT void AzU32Vec_delete(AzU32Vec* restrict instance);
extern DLLIMPORT void AzXWindowTypeVec_delete(AzXWindowTypeVec* restrict instance);
extern DLLIMPORT void AzVirtualKeyCodeVec_delete(AzVirtualKeyCodeVec* restrict instance);
extern DLLIMPORT void AzCascadeInfoVec_delete(AzCascadeInfoVec* restrict instance);
extern DLLIMPORT void AzScanCodeVec_delete(AzScanCodeVec* restrict instance);
extern DLLIMPORT void AzCssDeclarationVec_delete(AzCssDeclarationVec* restrict instance);
extern DLLIMPORT void AzCssPathSelectorVec_delete(AzCssPathSelectorVec* restrict instance);
extern DLLIMPORT void AzStylesheetVec_delete(AzStylesheetVec* restrict instance);
extern DLLIMPORT void AzCssRuleBlockVec_delete(AzCssRuleBlockVec* restrict instance);
extern DLLIMPORT void AzU16Vec_delete(AzU16Vec* restrict instance);
extern DLLIMPORT void AzF32Vec_delete(AzF32Vec* restrict instance);
extern DLLIMPORT AzU8VecRef AzU8Vec_asRefVec(AzU8Vec* const u8vec);
extern DLLIMPORT void AzU8Vec_delete(AzU8Vec* restrict instance);
extern DLLIMPORT void AzCallbackDataVec_delete(AzCallbackDataVec* restrict instance);
extern DLLIMPORT void AzDebugMessageVec_delete(AzDebugMessageVec* restrict instance);
extern DLLIMPORT void AzGLuintVec_delete(AzGLuintVec* restrict instance);
extern DLLIMPORT void AzGLintVec_delete(AzGLintVec* restrict instance);
extern DLLIMPORT void AzStringVec_delete(AzStringVec* restrict instance);
extern DLLIMPORT void AzStringPairVec_delete(AzStringPairVec* restrict instance);
extern DLLIMPORT void AzLinearColorStopVec_delete(AzLinearColorStopVec* restrict instance);
extern DLLIMPORT void AzRadialColorStopVec_delete(AzRadialColorStopVec* restrict instance);
extern DLLIMPORT void AzNodeIdVec_delete(AzNodeIdVec* restrict instance);
extern DLLIMPORT void AzNodeVec_delete(AzNodeVec* restrict instance);
extern DLLIMPORT void AzStyledNodeVec_delete(AzStyledNodeVec* restrict instance);
extern DLLIMPORT void AzTagIdsToNodeIdsMappingVec_delete(AzTagIdsToNodeIdsMappingVec* restrict instance);
extern DLLIMPORT void AzParentWithNodeDepthVec_delete(AzParentWithNodeDepthVec* restrict instance);
extern DLLIMPORT void AzNodeDataVec_delete(AzNodeDataVec* restrict instance);
extern DLLIMPORT void AzInstantPtr_delete(AzInstantPtr* restrict instance);
extern DLLIMPORT AzInstantPtr AzInstantPtr_deepCopy(AzInstantPtr* const instance);

/* Macro to turn a compile-time string into a compile-time AzString
 *
 * static AzString foo = AzString_fromConstStr(\"MyString\");
 */
#define AzString_fromConstStr(s) { \
    .vec = { \
        .ptr = s, \
        .len = sizeof(s) - 1, \
        .cap = sizeof(s) - 1, \
        .destructor = AzU8VecDestructor_NoDestructor, \
    } \
}

/* Macro to initialize a compile-time AzNodeData struct
 *
 * static AzNodeData foo = AzNodeData_new(AzNodeType_Div);
 */
#define AzNodeData_new(nt) { \
    .node_type = nt, \
    .dataset = AzOptionRefAny_None, \
    .ids_and_classes = AzIdOrClassVec_empty, \
    .callbacks = AzCallbackDataVec_empty, \
    .inline_css_props = AzNodeDataInlineCssPropertyVec_empty, \
    .clip_mask = AzOptionImageMask_None, \
    .tab_index = AzOptionTabIndex_None, \
}

/* Macro to initialize a compile-time AzDom struct
 *
 * static AzDom foo = AzDom_new(AzNodeType_Div);
 */
#define AzDom_new(nt) { \
    .root = AzNodeData_new(nt),\
    .children = AzDomVec_empty, \
    .total_children = 0, \
}

/* Macro to initialize the default AppConfig struct, must be in a header file
 * so that the LayoutSolverVersion is defined by the binary, not the library -
 * this way upgrading the library won't break the application layout
 *
 * AzAppConfig foo = AzAppConfig_default();
 */
#define AzAppConfig_default(...) { \
    .layout_solver = AzLayoutSolverVersion_March2021, \
    .log_level = AzAppLogLevel_Error, \
    .enable_visual_panic_hook = true, \
    .enable_logging_on_panic = true, \
    .enable_tab_navigation = true, \
    .system_callbacks = AzSystemCallbacks_libraryInternal(), \
}

/* Macro to generate reflection metadata for a given struct - for a "structName" of "foo", generates:
 *
 * constants:
 * - a foo_RttiTypeId, which serves as the "type ID" for that struct
 * - a foo_RttiString, a compile-time string that identifies the class
 *
 * structs:
 * - struct fooRef(): immutable reference to a RefAny<foo>
 * - struct fooRefMut(): mutable reference to a RefAny<foo>
 *
 * functions:
 * - AzRefAny foo_upcast(myStructInstance): upcasts a #structName to a RefAny
 *
 * - fooRef_create(AzRefAny): creates a new fooRef, but does not yet downcast it (.ptr is set to nullptr)
 * - fooRefMut_create(AzRefAny): creates a new fooRefMut, but does not yet downcast it (.ptr is set to nullptr)
 *
 * - bool foo_downcastRef(AzRefAny, fooRef* restrict): downcasts the RefAny immutably, if true is returned then the fooRef is properly initialized
 * - bool foo_downcastMut(AzRefAny, fooRefMut* restrict): downcasts the RefAny mutably, if true is returned then the fooRef is properly initialized
 *
 * - void fooRef_delete(fooRef): disposes of the fooRef and decreases the immutable reference count
 * - void fooRefMut_delete(fooRefMut): disposes of the fooRefMut and decreases the mutable reference count
 * - bool fooRefAny_delete(AzRefAny): disposes of the AzRefAny type, returns false if the AzRefAny is not of type RefAny<foo>
 *
 * USAGE:
 *
 *     typedef struct { } foo;
 *
 *     // -- destructor of foo, azul will call this function once the refcount hits 0
 *     // note: the function expects a void*, but you can just use a foo*
 *     void fooDestructor(foo* restrict destructorPtr) { }
 *
 *     AZ_REFLECT(foo, fooDestructor)
*/
#define AZ_REFLECT(structName, destructor) \
    /* in C all statics are guaranteed to have a unique address, use that address as a TypeId */ \
    static uint64_t const structName##_RttiTypePtrId = 0; \
    static uint64_t const structName##_RttiTypeId = (uint64_t)(&structName##_RttiTypePtrId); \
    static AzString const structName##_Type_RttiString = AzString_fromConstStr(#structName); \
    \
    AzRefAny structName##_upcast(structName const s) { \
        return AzRefAny_newC(&s, sizeof(structName), structName##_RttiTypeId, structName##_Type_RttiString, destructor); \
    } \
    \
    /* generate structNameRef and structNameRefMut structs*/ \
    typedef struct { const structName* ptr; AzRefCount sharing_info; } structName##Ref; \
    typedef struct { structName* restrict ptr; AzRefCount sharing_info; } structName##RefMut; \
    \
    structName##Ref structName##Ref_create(AzRefAny* const refany) { \
        structName##Ref val = { .ptr = 0, .sharing_info = AzRefCount_deepCopy(&refany->sharing_info) };    \
        return val;    \
    } \
    \
    structName##RefMut structName##RefMut_create(AzRefAny* const refany) { \
        structName##RefMut val = { .ptr = 0, .sharing_info = AzRefCount_deepCopy(&refany->sharing_info), };    \
        return val;    \
    } \
    \
    /* if downcastRef returns true, the downcast worked */ \
    bool structName##_downcastRef(AzRefAny* restrict refany, structName##Ref * restrict result) { \
        if (!AzRefAny_isType(refany, structName##_RttiTypeId)) { return false; } else { \
            if (!AzRefCount_canBeShared(&refany->sharing_info)) { return false; } else {\
                AzRefCount_increaseRef(&refany->sharing_info); \
                result->ptr = (structName* const)(refany->_internal_ptr); \
                return true; \
            } \
        } \
    } \
    \
    /* if downcastRefMut returns true, the mutable downcast worked */ \
    bool structName##_downcastMut(AzRefAny* restrict refany, structName##RefMut * restrict result) { \
        if (!AzRefAny_isType(refany, structName##_RttiTypeId)) { return false; } else { \
            if (!AzRefCount_canBeSharedMut(&refany->sharing_info)) { return false; }  else {\
                AzRefCount_increaseRefmut(&refany->sharing_info); \
                result->ptr = (structName* restrict)(refany->_internal_ptr); \
                return true; \
            } \
        } \
    } \
    \
    /* releases a structNameRef (decreases the RefCount) */ \
    void structName##Ref_delete(structName##Ref* restrict value) { \
        AzRefCount_decreaseRef(&value->sharing_info); \
    }\
    \
    /* releases a structNameRefMut (decreases the mutable RefCount) */ \
    void structName##RefMut_delete(structName##RefMut* restrict value) { \
        AzRefCount_decreaseRefmut(&value->sharing_info); \
    }\
    /* releases a structNameRefAny (checks if the RefCount is 0 and calls the destructor) */ \
    bool structName##RefAny_delete(AzRefAny* restrict refany) { \
        if (!AzRefAny_isType(refany, structName##_RttiTypeId)) { return false; } \
        AzRefAny_delete(refany); \
        return true; \
    }

#endif /* AZUL_H */
