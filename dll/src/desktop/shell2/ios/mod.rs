//! iOS backend.
//!
//! Structurally mirrors `shell2/android/mod.rs`: an `IOSWindow` carries the
//! cross-platform [`CommonWindowState`] + a [`CpuBackend`], plus the native
//! `UIWindow` / `UIViewController` / `UIView` handles. The render path is
//! identical in spirit to Android — CPU rendering to an `AzulPixmap`, blitted
//! to the layer via `CGImage` + `CALayer.contents` (Sprint C wires the blit;
//! this module currently lands the type surface + entry point so the iOS
//! target compiles end-to-end).
//!
//! No iOS SDK is required to *type-check* this file — every UIKit/Foundation
//! symbol is referenced through the `objc` crate's `class!` / `msg_send!` /
//! `sel!` macros (which compile-check against the `objc` crate, not the
//! UIKit SDK). The SDK is only needed at link time, which lives in
//! `dll/build.rs::configure_ios`.

use crate::impl_platform_window_getters;
use std::{
    cell::RefCell,
    ffi::c_void,
    ptr,
    sync::{Arc, Once},
};

use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Protocol, Sel};
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};
use objc_id::Id;

use azul_core::{
    callbacks::RelayoutReason,
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::{AppConfig, IdNamespace, ImageCache, RendererResources},
    window::{IOSHandle, RawWindowHandle},
};
use azul_layout::{
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::{registry::FcFontRegistry, FcFontCache};

use crate::desktop::shell2::common::{
    debug_server::LogCategory,
    event::{self, CommonWindowState, HitTestNode, PlatformWindow},
    WindowError,
};
use crate::desktop::shell2::headless::CpuBackend;
use crate::desktop::wr_translate2::{AsyncHitTester, WrRenderApi};
use crate::{log_debug, log_error, log_info};

// ─── Core Graphics geometry types (FFI-safe; `Encode` impls let them
//     traverse `msg_send!` without depending on `core_graphics_sys`) ────

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

// Objective-C type encodings for the Core Graphics geometry structs.
// `{CGPoint=dd}` etc. matches the encoding `[UIScreen mainScreen].bounds`
// returns, which `msg_send!` walks to lay out the call. objc 0.2's
// `Encode` trait uses `fn encode() -> Encoding`, not the `const ENCODING`
// surface from objc2.
unsafe impl Encode for CGPoint {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGPoint=dd}") }
    }
}
unsafe impl Encode for CGSize {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGSize=dd}") }
    }
}
unsafe impl Encode for CGRect {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

// ─── FFI bindings ─────────────────────────────────────────────────────

#[link(name = "Foundation", kind = "framework")]
extern "C" {
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "UIKit", kind = "framework")]
extern "C" {
    fn UIApplicationMain(
        argc: i32,
        argv: *mut *mut u8,
        principalClassName: *mut Object,
        delegateClassName: *mut Object,
    ) -> i32;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(cs: *mut c_void);
    fn CGDataProviderCreateWithData(
        info: *mut c_void,
        data: *const u8,
        size: usize,
        release: Option<extern "C" fn(*mut c_void, *const u8, usize)>,
    ) -> *mut c_void;
    fn CGDataProviderRelease(p: *mut c_void);
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
        provider: *mut c_void,
        decode: *const f64,
        should_interpolate: bool,
        intent: u32,
    ) -> *mut c_void;
    fn CGImageRelease(img: *mut c_void);
}

const K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST: u32 = 1;
const K_CG_BITMAP_BYTE_ORDER_DEFAULT: u32 = 0;
const K_CG_RENDERING_INTENT_DEFAULT: u32 = 0;

// ─── Global window pointer ────────────────────────────────────────────
//
// `extern "C"` Objective-C callbacks are static functions, so they reach
// back into Rust state via this singleton. Set in
// `application:didFinishLaunchingWithOptions:`, cleared by
// `applicationWillTerminate:` (TODO once we wire lifecycle methods).

static mut AZUL_IOS_WINDOW: *mut IOSWindow = ptr::null_mut();

/// Borrow the singleton AndroidWindow-style. None until `did_finish_launching`.
#[inline]
unsafe fn azul_ios_window<'a>() -> Option<&'a mut IOSWindow> {
    AZUL_IOS_WINDOW.as_mut()
}

// ─── AzulView (UIView subclass) ───────────────────────────────────────

extern "C" fn draw_rect(this: &Object, _cmd: Sel, _rect: CGRect) {
    // Sprint C iOS blit. Mirrors the Android render_frame() path:
    // regenerate layout if needed -> read cpu_backend.last_frame -> wrap
    // the AzulPixmap bytes in a CGImage -> assign to `view.layer.contents`.
    let window = match unsafe { azul_ios_window() } {
        Some(w) => w,
        None => return,
    };

    if window.common.frame_needs_regeneration {
        if let Err(e) = window.regenerate_layout() {
            log_error!(LogCategory::Layout, "[iOS] regenerate_layout: {}", e);
        }
    }

    #[cfg(feature = "cpurender")]
    {
        let pixmap = match window.cpu_backend.last_frame.as_ref() {
            Some(p) => p,
            None => return,
        };
        let (pw, ph) = (pixmap.width() as usize, pixmap.height() as usize);
        if pw == 0 || ph == 0 {
            return;
        }
        let bytes = pixmap.data();
        unsafe {
            let cs = CGColorSpaceCreateDeviceRGB();
            let provider = CGDataProviderCreateWithData(
                core::ptr::null_mut(),
                bytes.as_ptr(),
                bytes.len(),
                None, // pixmap outlives this draw_rect call
            );
            let image = CGImageCreate(
                pw,
                ph,
                8,
                32,
                pw * 4,
                cs,
                K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST | K_CG_BITMAP_BYTE_ORDER_DEFAULT,
                provider,
                core::ptr::null(),
                false,
                K_CG_RENDERING_INTENT_DEFAULT,
            );
            let layer: *mut Object = msg_send![this, layer];
            let _: () = msg_send![layer, setContents: image];
            CGImageRelease(image);
            CGDataProviderRelease(provider);
            CGColorSpaceRelease(cs);
        }
    }
}

/// Shared body for the four UITouch responder selectors. `phase`
/// follows UIKit semantics:
///   0 = began    (left_down=true, update cursor)
///   1 = moved    (update cursor only)
///   2 = ended    (left_down=false)
///   3 = cancelled
fn handle_touch(this: &Object, touches: *mut Object, phase: u8) {
    use azul_core::events::ProcessEventResult;
    use azul_core::geom::LogicalPosition;
    use azul_core::window::CursorPosition;

    let window = match unsafe { azul_ios_window() } {
        Some(w) => w,
        None => return,
    };

    // Read the first UITouch's location-in-view. Empty set means no
    // touches (theoretically can't happen for these selectors).
    let pos: Option<LogicalPosition> = unsafe {
        let any: *mut Object = msg_send![touches, anyObject];
        if any.is_null() {
            None
        } else {
            let this_ptr = this as *const Object as *mut Object;
            let p: CGPoint = msg_send![any, locationInView: this_ptr];
            Some(LogicalPosition::new(p.x as f32, p.y as f32))
        }
    };

    // Snapshot previous state for the diff pipeline; mirrors Android.
    window.common.previous_window_state =
        Some(window.common.current_window_state.clone());

    {
        let ms = &mut window.common.current_window_state.mouse_state;
        if let Some(p) = pos {
            ms.cursor_position = CursorPosition::InWindow(p);
        }
        match phase {
            0 => ms.left_down = true,
            2 | 3 => ms.left_down = false,
            _ => {}
        }
    }

    if let Some(p) = pos {
        window.update_hit_test_at(p);
    }
    let r = window.process_window_events(0);
    if !matches!(r, ProcessEventResult::DoNothing) {
        window.common.frame_needs_regeneration = true;
    }
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.gesture_drag_manager.clear_native_gesture();
    }

    // Ask the view to redraw — drawRect: will pick up the new layout.
    let view = this as *const Object as *mut Object;
    let _: () = unsafe { msg_send![view, setNeedsDisplay] };
}

extern "C" fn touches_began(
    this: &Object,
    _cmd: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    handle_touch(this, touches, 0);
}
extern "C" fn touches_moved(
    this: &Object,
    _cmd: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    handle_touch(this, touches, 1);
}
extern "C" fn touches_ended(
    this: &Object,
    _cmd: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    handle_touch(this, touches, 2);
}
extern "C" fn touches_cancelled(
    this: &Object,
    _cmd: Sel,
    touches: *mut Object,
    _event: *mut Object,
) {
    handle_touch(this, touches, 3);
}

// ─── UIKit gesture-recognizer handlers (Sprint M iOS side) ───────────
//
// Each handler is attached as the `action:` selector of a
// `UI*GestureRecognizer` instance constructed in `IOSWindow::new`. UIKit
// fires the action with `(sender: UIGestureRecognizer*)`; we read the
// recognizer's `state` to decide whether to inject. For tap/long-press
// we only inject on `UIGestureRecognizerStateRecognized` (== 3) /
// `Began` (== 1); for continuous recognizers (pinch / rotation) we'd
// inject on `Changed` (== 2). Action-selector signatures: the Rust
// function takes `(target: &Object, _cmd: Sel, sender: *mut Object)`.

const UI_GESTURE_RECOGNIZER_STATE_RECOGNIZED: i64 = 3;
const UI_GESTURE_RECOGNIZER_STATE_BEGAN: i64 = 1;
const UI_GESTURE_RECOGNIZER_STATE_CHANGED: i64 = 2;

fn inject(window: &mut IOSWindow, gesture: azul_layout::managers::gesture::NativeGestureEvent) {
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.gesture_drag_manager.inject_native_gesture(gesture);
        window.common.frame_needs_regeneration = true;
    }
}

extern "C" fn on_double_tap(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_layout::managers::gesture::NativeGestureEvent;
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_RECOGNIZED {
        return;
    }
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::DoubleClick);
    }
}

extern "C" fn on_long_press(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_core::geom::LogicalPosition;
    use azul_layout::managers::gesture::{DetectedLongPress, NativeGestureEvent};
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_BEGAN {
        return;
    }
    let p: CGPoint = unsafe { msg_send![sender, locationInView: ptr::null_mut::<Object>()] };
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(
            w,
            NativeGestureEvent::LongPress(DetectedLongPress {
                position: LogicalPosition { x: p.x as f32, y: p.y as f32 },
                duration_ms: 0,
                callback_invoked: false,
                session_id: 0,
            }),
        );
    }
}

extern "C" fn on_swipe_left(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Left));
    }
}
extern "C" fn on_swipe_right(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Right));
    }
}
extern "C" fn on_swipe_up(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Up));
    }
}
extern "C" fn on_swipe_down(_t: &Object, _c: Sel, _s: *mut Object) {
    use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(w, NativeGestureEvent::Swipe(GestureDirection::Down));
    }
}

extern "C" fn on_pinch(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_core::geom::LogicalPosition;
    use azul_layout::managers::gesture::{DetectedPinch, NativeGestureEvent};
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_CHANGED {
        return;
    }
    let scale: f64 = unsafe { msg_send![sender, scale] };
    let p: CGPoint = unsafe { msg_send![sender, locationInView: ptr::null_mut::<Object>()] };
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(
            w,
            NativeGestureEvent::Pinch(DetectedPinch {
                scale: scale as f32,
                center: LogicalPosition { x: p.x as f32, y: p.y as f32 },
                initial_distance: 0.0,
                current_distance: 0.0,
                duration_ms: 0,
            }),
        );
    }
}

extern "C" fn on_rotation(_this: &Object, _cmd: Sel, sender: *mut Object) {
    use azul_core::geom::LogicalPosition;
    use azul_layout::managers::gesture::{DetectedRotation, NativeGestureEvent};
    let state: i64 = unsafe { msg_send![sender, state] };
    if state != UI_GESTURE_RECOGNIZER_STATE_CHANGED {
        return;
    }
    let rotation: f64 = unsafe { msg_send![sender, rotation] };
    let p: CGPoint = unsafe { msg_send![sender, locationInView: ptr::null_mut::<Object>()] };
    if let Some(w) = unsafe { azul_ios_window() } {
        inject(
            w,
            NativeGestureEvent::Rotation(DetectedRotation {
                angle_radians: rotation as f32,
                center: LogicalPosition { x: p.x as f32, y: p.y as f32 },
                duration_ms: 0,
            }),
        );
    }
}

/// Dynamically register an empty NSObject subclass whose only purpose
/// is to be the `target:` of every gesture recognizer. UIKit expects an
/// Objective-C object; an empty subclass is the cheapest legal one.
fn get_or_create_gesture_target_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("AzulGestureTarget", superclass).unwrap();
        decl.add_method(
            sel!(onDoubleTap:),
            on_double_tap as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onLongPress:),
            on_long_press as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeLeft:),
            on_swipe_left as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeRight:),
            on_swipe_right as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeUp:),
            on_swipe_up as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onSwipeDown:),
            on_swipe_down as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onPinch:),
            on_pinch as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(onRotation:),
            on_rotation as extern "C" fn(&Object, Sel, *mut Object),
        );
        CLS = decl.register();
    });
    unsafe { &*CLS }
}

/// Attach UITap / UILongPress / UISwipe(×4) / UIPinch / UIRotation
/// recognizers to `view`. The shared `target` object is leaked — its
/// lifetime is tied to the application.
unsafe fn install_gesture_recognizers(view: *mut Object) {
    use objc::sel;
    let target_class = get_or_create_gesture_target_class();
    let target_alloc: *mut Object = msg_send![target_class, alloc];
    let target: *mut Object = msg_send![target_alloc, init];

    // Helper closure to alloc + init + addGestureRecognizer:
    let attach_basic = |class_name: &Class, action: Sel| {
        let r_alloc: *mut Object = msg_send![class_name, alloc];
        let r: *mut Object = msg_send![r_alloc, initWithTarget: target action: action];
        let _: () = msg_send![view, addGestureRecognizer: r];
        r
    };

    // Double-tap (UITapGestureRecognizer with numberOfTapsRequired = 2)
    let tap = attach_basic(class!(UITapGestureRecognizer), sel!(onDoubleTap:));
    let _: () = msg_send![tap, setNumberOfTapsRequired: 2i64];

    let _ = attach_basic(class!(UILongPressGestureRecognizer), sel!(onLongPress:));
    let _ = attach_basic(class!(UIPinchGestureRecognizer), sel!(onPinch:));
    let _ = attach_basic(class!(UIRotationGestureRecognizer), sel!(onRotation:));

    // Swipe recognizers need one instance per direction (UISwipeGestureRecognizer's
    // `direction` is a bitmask but UIKit fires the action once per direction).
    // direction enum values: Right=1, Left=2, Up=4, Down=8.
    let attach_swipe = |dir: u64, action: Sel| {
        let r_alloc: *mut Object = msg_send![class!(UISwipeGestureRecognizer), alloc];
        let r: *mut Object = msg_send![r_alloc, initWithTarget: target action: action];
        let _: () = msg_send![r, setDirection: dir];
        let _: () = msg_send![view, addGestureRecognizer: r];
    };
    attach_swipe(1, sel!(onSwipeRight:));
    attach_swipe(2, sel!(onSwipeLeft:));
    attach_swipe(4, sel!(onSwipeUp:));
    attach_swipe(8, sel!(onSwipeDown:));
}

fn get_or_create_view_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut AZUL_VIEW_CLASS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(UIView);
        let mut decl = ClassDecl::new("AzulView", superclass).unwrap();

        decl.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(&Object, Sel, CGRect),
        );
        decl.add_method(
            sel!(touchesBegan:withEvent:),
            touches_began as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(touchesMoved:withEvent:),
            touches_moved as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(touchesEnded:withEvent:),
            touches_ended as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(touchesCancelled:withEvent:),
            touches_cancelled as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );

        AZUL_VIEW_CLASS = decl.register();
    });
    unsafe { &*AZUL_VIEW_CLASS }
}

// ─── AppDelegate ──────────────────────────────────────────────────────

extern "C" fn did_finish_launching(
    _this: &Object,
    _cmd: Sel,
    _app: *mut Object,
    _opts: *mut Object,
) -> bool {
    unsafe {
        let (app_data, config, fc_cache, font_registry, root_window) =
            match super::run::INITIAL_OPTIONS.take() {
                Some(opts) => opts,
                None => {
                    log_error!(
                        LogCategory::EventLoop,
                        "[iOS] did_finish_launching: INITIAL_OPTIONS unset — \
                         azul_run() must run before UIApplicationMain"
                    );
                    return false;
                }
            };

        let window =
            match IOSWindow::new(root_window, fc_cache, config, app_data, font_registry) {
                Ok(w) => w,
                Err(e) => {
                    log_error!(LogCategory::EventLoop, "[iOS] IOSWindow::new: {:?}", e);
                    return false;
                }
            };
        AZUL_IOS_WINDOW = Box::into_raw(Box::new(window));
        log_info!(LogCategory::EventLoop, "[iOS] application:didFinishLaunching: ok");
    }
    true
}

fn get_or_create_app_delegate_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut APP_DELEGATE_CLASS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("AppDelegate", superclass).unwrap();

        decl.add_protocol(Protocol::get("UIApplicationDelegate").unwrap());

        decl.add_method(
            sel!(application:didFinishLaunchingWithOptions:),
            did_finish_launching
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> bool,
        );

        APP_DELEGATE_CLASS = decl.register();
    });
    unsafe { &*APP_DELEGATE_CLASS }
}

/// Bootstraps the UIKit run-loop. Never returns.
pub unsafe fn launch_app() {
    let pool = objc_autoreleasePoolPush();
    let _ = get_or_create_app_delegate_class();

    // NSString*: the principal class + delegate class names UIApplicationMain
    // uses to instantiate the application + delegate. `obj_alloc_init` is
    // simpler than constructing an NSString.
    let principal_cstr = b"UIApplication\0".as_ptr() as *const i8;
    let delegate_cstr = b"AppDelegate\0".as_ptr() as *const i8;
    let principal_name: *mut Object =
        msg_send![class!(NSString), stringWithUTF8String: principal_cstr];
    let delegate_name: *mut Object =
        msg_send![class!(NSString), stringWithUTF8String: delegate_cstr];

    UIApplicationMain(0, ptr::null_mut(), principal_name, delegate_name);

    objc_autoreleasePoolPop(pool);
}

// ─── IOSWindow ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBackend {
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IOSEvent {
    Close,
}

pub struct IOSWindow {
    /// Cross-platform window state.
    pub common: CommonWindowState,
    /// CPU rendering backend (replaces WebRender).
    pub cpu_backend: CpuBackend,
    /// Native UIWindow.
    ui_window: Id<Object>,
    /// Custom UIView (AzulView subclass).
    ui_view: Id<Object>,
    /// UIViewController.
    ui_view_controller: Id<Object>,
    /// Rendering backend selector (CPU only until Sprint M-iOS-GPU).
    pub backend: RenderBackend,
    /// `false` after `applicationWillTerminate:`.
    pub is_open: bool,
    /// Shared icon provider — needed by `regenerate_layout()`.
    pub icon_provider: SharedIconProvider,
    /// Optional shared font registry for async font discovery.
    pub font_registry: Option<Arc<FcFontRegistry>>,
}

impl IOSWindow {
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        mut config: AppConfig,
        app_data: RefAny,
        font_registry: Option<Arc<FcFontRegistry>>,
    ) -> Result<Self, WindowError> {
        let full_window_state = options.window_state;

        let icon_provider_handle = core::mem::take(&mut config.icon_provider);
        let icon_provider = SharedIconProvider::from_handle(icon_provider_handle);

        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone())
            .map_err(|e| WindowError::PlatformError(format!("Layout init failed: {:?}", e)))?;
        layout_window.current_window_state = full_window_state.clone();
        layout_window.routes = config.routes.clone();

        // Build the native UI tree. Bounds come from `[[UIScreen mainScreen] bounds]`.
        let (ui_window, ui_view_controller, ui_view) = unsafe {
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let bounds: CGRect = msg_send![screen, bounds];

            let window_alloc: *mut Object = msg_send![class!(UIWindow), alloc];
            let window: *mut Object = msg_send![window_alloc, initWithFrame: bounds];

            let vc_alloc: *mut Object = msg_send![class!(UIViewController), alloc];
            let vc: *mut Object = msg_send![vc_alloc, init];

            let view_class = get_or_create_view_class();
            let view_alloc: *mut Object = msg_send![view_class, alloc];
            let view: *mut Object = msg_send![view_alloc, initWithFrame: bounds];

            let _: () = msg_send![vc, setView: view];
            let _: () = msg_send![window, setRootViewController: vc];
            let _: () = msg_send![window, makeKeyAndVisible];

            // Attach UIKit gesture recognizers (Sprint M iOS side).
            // Each recognizer forwards to AZUL_IOS_WINDOW.common
            //  .layout_window.gesture_drag_manager.inject_native_gesture
            // so CallbackInfo::get_swipe_direction etc. observe a result.
            install_gesture_recognizers(view);

            // `Id::from_ptr` retains the object; balanced by Drop.
            (
                Id::from_ptr(window),
                Id::from_ptr(vc),
                Id::from_ptr(view),
            )
        };

        Ok(Self {
            common: CommonWindowState {
                layout_window: Some(layout_window),
                current_window_state: full_window_state,
                previous_window_state: None,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                fc_cache,
                gl_context_ptr: OptionGlContextPtr::None,
                system_style: Arc::new(azul_css::system::SystemStyle::default()),
                app_data: Arc::new(RefCell::new(app_data)),
                scrollbar_drag_state: None,
                hit_tester: None,
                cpu_hit_tester: Some(azul_layout::headless::CpuHitTester::new()),
                last_hovered_node: None,
                document_id: None,
                id_namespace: None,
                render_api: None,
                renderer: None,
                frame_needs_regeneration: true,
                next_relayout_reason: RelayoutReason::Initial,
                display_list_initialized: false,
                display_list_dirty: false,
                a11y_dirty: true,
            },
            cpu_backend: CpuBackend::new(),
            ui_window,
            ui_view,
            ui_view_controller,
            backend: RenderBackend::Cpu,
            is_open: true,
            icon_provider,
            font_registry,
        })
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }
    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn poll_event(&mut self) -> Option<IOSEvent> {
        None
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        let view = &*self.ui_view as *const Object as *mut Object;
        unsafe {
            let _: () = msg_send![view, setNeedsDisplay];
        }
        Ok(())
    }
    pub fn request_redraw(&mut self) {
        let _ = self.present();
    }

    /// Run a full layout regeneration pass and CPU-render the resulting
    /// display list. Mirrors `AndroidWindow::regenerate_layout()`. Called
    /// from the `drawRect:` handler when `frame_needs_regeneration` is
    /// true (Sprint C-iOS wires that).
    pub fn regenerate_layout(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        let layout_window = self
            .common
            .layout_window
            .as_mut()
            .ok_or("No layout window")?;

        let debug_enabled =
            crate::desktop::shell2::common::debug_server::is_debug_enabled();
        let mut debug_messages = if debug_enabled { Some(Vec::new()) } else { None };

        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            &self.common.app_data,
            &self.common.current_window_state,
            &mut self.common.renderer_resources,
            &self.common.image_cache,
            &self.common.gl_context_ptr,
            &self.common.fc_cache,
            &self.font_registry,
            &self.common.system_style,
            &self.icon_provider,
            &mut debug_messages,
            self.common.next_relayout_reason,
        )?;
        self.common.next_relayout_reason =
            azul_core::callbacks::RelayoutReason::RefreshDom;

        if let Some(msgs) = debug_messages {
            for msg in msgs {
                crate::desktop::shell2::common::debug_server::log(
                    crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                    crate::desktop::shell2::common::debug_server::LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        if let Some(lw) = self.common.layout_window.as_ref() {
            self.cpu_backend.hit_tester.rebuild_from_layout(&lw.layout_results);
        }

        // CPU-render the frame — populates `self.cpu_backend.last_frame`,
        // ready for `drawRect:` to blit into the layer (Sprint C-iOS).
        #[cfg(feature = "cpurender")]
        {
            let ws = &self.common.current_window_state;
            let width = ws.size.dimensions.width;
            let height = ws.size.dimensions.height;
            let dpi = ws.size.dpi as f32 / 96.0;
            if let Some(lw) = self.common.layout_window.as_ref() {
                self.cpu_backend.render_frame(
                    lw,
                    &self.common.renderer_resources,
                    width,
                    height,
                    dpi,
                );
            }
        }

        self.common.frame_needs_regeneration = false;
        Ok(result)
    }
}

impl PlatformWindow for IOSWindow {
    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::IOS(IOSHandle {
            ui_window: (&*self.ui_window as *const Object) as *mut c_void,
            ui_view: (&*self.ui_view as *const Object) as *mut c_void,
            ui_view_controller: (&*self.ui_view_controller as *const Object) as *mut c_void,
        })
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows<'_> {
        let layout_window = self
            .common
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");
        event::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::IOS(IOSHandle {
                ui_window: std::ptr::null_mut(),
                ui_view: std::ptr::null_mut(),
                ui_view_controller: std::ptr::null_mut(),
            }),
            gl_context_ptr: &self.common.gl_context_ptr,
            image_cache: &mut self.common.image_cache,
            fc_cache_clone: (*self.common.fc_cache).clone(),
            system_style: self.common.system_style.clone(),
            previous_window_state: &self.common.previous_window_state,
            current_window_state: &self.common.current_window_state,
            renderer_resources: &mut self.common.renderer_resources,
        }
    }

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
    }
    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }
    fn start_thread_poll_timer(&mut self) {}
    fn stop_thread_poll_timer(&mut self) {}

    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<
            azul_core::task::ThreadId,
            azul_layout::thread::Thread,
        >,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for (id, thread) in threads {
                lw.threads.insert(id, thread);
            }
        }
    }
    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for id in thread_ids {
                lw.threads.remove(id);
            }
        }
    }

    fn queue_window_create(&mut self, _options: WindowCreateOptions) {
        // No popup windows on iOS — sub-windows would require a
        // separate UIWindow or modal UIViewController.
    }

    fn show_menu_from_callback(
        &mut self,
        _menu: &azul_core::menu::Menu,
        _position: azul_core::geom::LogicalPosition,
    ) {
    }

    fn show_tooltip_from_callback(
        &mut self,
        _text: &str,
        _position: azul_core::geom::LogicalPosition,
    ) {
    }

    fn hide_tooltip_from_callback(&mut self) {}

    fn sync_window_state(&mut self) {}
}
