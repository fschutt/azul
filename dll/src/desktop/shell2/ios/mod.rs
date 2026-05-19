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

extern "C" fn draw_rect(_this: &Object, _cmd: Sel, _rect: CGRect) {
    // Sprint C wires the actual blit: regenerate_layout → cpu_backend
    // .render_frame → CGImage from AzulPixmap → CALayer.contents.
    // Until then, the view just clears to the system background.
}

extern "C" fn touches_began(
    _this: &Object,
    _cmd: Sel,
    _touches: *mut Object,
    _event: *mut Object,
) {
    if let Some(_window) = unsafe { azul_ios_window() } {
        log_debug!(LogCategory::Input, "[AzulView] touchesBegan:");
    }
}

extern "C" fn touches_moved(
    _this: &Object,
    _cmd: Sel,
    _touches: *mut Object,
    _event: *mut Object,
) {
    if let Some(_window) = unsafe { azul_ios_window() } {
        log_debug!(LogCategory::Input, "[AzulView] touchesMoved:");
    }
}

extern "C" fn touches_ended(
    _this: &Object,
    _cmd: Sel,
    _touches: *mut Object,
    _event: *mut Object,
) {
    if let Some(_window) = unsafe { azul_ios_window() } {
        log_debug!(LogCategory::Input, "[AzulView] touchesEnded:");
    }
}

extern "C" fn touches_cancelled(
    _this: &Object,
    _cmd: Sel,
    _touches: *mut Object,
    _event: *mut Object,
) {
    if let Some(_window) = unsafe { azul_ios_window() } {
        log_debug!(LogCategory::Input, "[AzulView] touchesCancelled:");
    }
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
