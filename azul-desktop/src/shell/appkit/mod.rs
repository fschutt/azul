#![cfg(target_os = "macos")]
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use azul_core::app_resources::ImageCache;
use azul_core::gl::OptionGlContextPtr;
use azul_core::task::{Thread, ThreadId, Timer, TimerId};
use azul_core::ui_solver::QuickResizeResult;
use azul_core::window_state::NodesToCheck;
use azul_core::{FastBTreeSet, FastHashMap};
use gl_context_loader::GenericGlContext;
use objc2::declare::ClassDecl;
use objc2::runtime::{AnyClass, AnyObject, Class, ClassBuilder, Object, ProtocolObject, Sel};
use objc2::*;
use azul_core::window::{MacOSHandle, MonitorVec, PhysicalSize, ScrollResult, WindowInternal, WindowInternalInit};
use azul_core::window::WindowCreateOptions;
use crate::app::{App, LazyFcCache};
use crate::wr_translate::{wr_synchronize_updated_images, AsyncHitTester};
use objc2::runtime::YES;
use objc2::rc::{autoreleasepool, AutoreleasePool, Retained};
use objc2_app_kit::{NSAppKitVersionNumber, NSAppKitVersionNumber10_12, NSView, NSWindowStyleMask, NSWindowWillCloseNotification};
use objc2_app_kit::NSApp;
use objc2_foundation::{MainThreadMarker, NSNotification, NSNotificationCenter, NSNotificationName, NSObjectProtocol};
use objc2::ffi::id;
use objc2_foundation::NSRect;
use objc2_foundation::NSString;
use objc2::ffi::nil;
use objc2_app_kit::NSWindow;
use objc2_foundation::NSSize;
use objc2_foundation::NSPoint;
use std::ffi::c_void;
use objc2_app_kit::NSBackingStoreType;
use objc2_app_kit::NSApplicationActivationPolicy;
use std::{fmt, mem, os::raw::c_char, ptr, rc::Rc};
use self::gl::GlFunctions;
use objc2::rc::Id;
use azul_core::window::{RawWindowHandle, WindowId, WindowsHandle};
use webrender::{
    api::{
        units::{
            DeviceIntPoint as WrDeviceIntPoint,
            DeviceIntRect as WrDeviceIntRect,
            DeviceIntSize as WrDeviceIntSize,
            LayoutSize as WrLayoutSize,
        },
        HitTesterRequest as WrHitTesterRequest,
        ApiHitTester as WrApiHitTester, DocumentId as WrDocumentId,
        RenderNotifier as WrRenderNotifier,
    },
    render_api::RenderApi as WrRenderApi,
    PipelineInfo as WrPipelineInfo, Renderer as WrRenderer, RendererError as WrRendererError,
    RendererOptions as WrRendererOptions, ShaderPrecacheFlags as WrShaderPrecacheFlags,
    Shaders as WrShaders, Transaction as WrTransaction,
};

mod menu;
mod gl;

/// OpenGL context guard, to be returned by window.make_current_gl()
pub(crate) struct GlContextGuard {
    glc: *mut Object,
}

pub(crate) struct MacApp {
    pub(crate) functions: Rc<GenericGlContext>,
    pub(crate) active_menus: BTreeMap<menu::MenuTarget, menu::CommandMap>,
    pub(crate) data: Arc<Mutex<AppData>>,
}

pub(crate) struct AppData {
    pub userdata: App,
    pub windows: BTreeMap<WindowId, Window>
}

pub(crate) struct Window {
    /// Internal Window ID
    pub(crate) id: WindowId,
    /// NSWindow
    pub(crate) ns_window: Option<Retained<NSWindow>>,
    /// Observer that fires a notification when the window is closed
    pub(crate) ns_close_observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    /// See azul-core, stores the entire UI (DOM, CSS styles, layout results, etc.)
    pub(crate) internal: WindowInternal,
    /// Main render API that can be used to register and un-register fonts and images
    pub(crate) render_api: WrRenderApi,
    /// WebRender renderer implementation (software or hardware)
    pub(crate) renderer: Option<WrRenderer>,
    /// Hit-tester, lazily initialized and updated every time the display list changes layout
    pub(crate) hit_tester: AsyncHitTester,
    pub(crate) gl_context_ptr: OptionGlContextPtr,
    /*
    /// ID -> Callback map for the window menu (default: empty map)
    menu_bar: Option<WindowsMenuBar>,
    /// ID -> Context menu callbacks (cleared when the context menu closes)
    context_menu: Option<CurrentContextMenu>,
    /// Timer ID -> Win32 timer map
    timers: BTreeMap<TimerId, TIMERPTR>,
    /// If threads is non-empty, the window will receive a WM_TIMER every 16ms
    thread_timer_running: Option<TIMERPTR>,
    */
}

impl Window {

    // --- functions necessary for event.rs handling

    /// Utility for making the GL context current, returning a guard that resets it afterwards
    pub(crate) fn make_current_gl(gl_context: *mut Object) -> GlContextGuard {
        unsafe {
            let _: () = msg_send![gl_context, makeCurrentContext];
            GlContextGuard { glc: gl_context }
        }
    }

    /// Function that flushes the buffer and "ends" the drawing code
    pub(crate) fn finish_gl(guard: GlContextGuard) {
        unsafe {
            let _: () = msg_send![guard.glc, flushBuffer];
        }
    }
    
    /// On macOS, we might do something akin to `[self.ns_window setTitle:new_title]`
    /// or update the menubar, etc.
    pub(crate) fn update_menus(&mut self) {
        // Called from `event::regenerate_dom` etc. after the DOM changes
    }

    /// After updating the display list, we want a new "hit tester" from webrender
    pub(crate) fn request_new_hit_tester(&mut self) {
        self.hit_tester = crate::wr_translate::AsyncHitTester::Requested(
            self.render_api.request_hit_tester(crate::wr_translate::wr_translate_document_id(
                self.internal.document_id
            ))
        );
    }

    /// Indicate that the window contents need repainting (setNeedsDisplay: YES)
    pub(crate) fn request_redraw(&mut self) {
        unsafe {
            if let Some(s) = self.ns_window.as_deref() {
                let s: *mut Object = msg_send![s, contentView];
                let _: () = msg_send![s, setNeedsDisplay: YES];
            }
        }
    }

    /// Called internally from do_resize
    /// 
    /// If needed, do `wr_synchronize_updated_images(resize_result.updated_images, ...)`
    /// and update the WebRender doc-view 
    #[must_use]
    pub(crate) fn do_resize_impl(
        &mut self,
        new_physical_size: PhysicalSize<u32>,
        image_cache: &ImageCache,
        fc_cache: &mut LazyFcCache,
        gl_context_ptr: &OptionGlContextPtr,
    ) -> QuickResizeResult {

        let new_size = new_physical_size.to_logical(self.internal.current_window_state.size.get_hidpi_factor());
        let old_state = self.internal.current_window_state.clone();
        self.internal.current_window_state.size.dimensions = new_size;

        let size = self.internal.current_window_state.size.clone();
        let theme = self.internal.current_window_state.theme.clone();

        fc_cache.apply_closure(|fc_cache| {
            self.internal.do_quick_resize(
                image_cache,
                &crate::app::CALLBACKS,
                azul_layout::do_the_relayout,
                fc_cache,
                gl_context_ptr,
                &size,
                theme,
            )
        })
    }

    // --- functions necessary for process.rs handling 

    pub(crate) fn start_stop_timers(
        &mut self,
        added: FastHashMap<TimerId, Timer>,
        removed: FastBTreeSet<TimerId>
    ) {

        /*
        use winapi::um::winuser::{SetTimer, KillTimer};

        for (id, timer) in added {
            let res = unsafe { SetTimer(self.hwnd, id.id, timer.tick_millis().min(u32::MAX as u64) as u32, None) };
            self.internal.timers.insert(id, timer);
            self.timers.insert(id, res);
        }

        for id in removed {
            if let Some(_) = self.internal.timers.remove(&id) {
                if let Some(handle) = self.timers.remove(&id) {
                    unsafe { KillTimer(self.hwnd, handle) };
                }
            }
        }
         */
    }

    pub(crate) fn start_stop_threads(
        &mut self,
        mut added: FastHashMap<ThreadId, Thread>,
        removed: FastBTreeSet<ThreadId>
    ) {

        /*
        use winapi::um::winuser::{SetTimer, KillTimer};

        self.internal.threads.append(&mut added);
        self.internal.threads.retain(|r, _| !removed.contains(r));

        if self.internal.threads.is_empty() {
            if let Some(thread_tick) = self.thread_timer_running {
                unsafe { KillTimer(self.hwnd, thread_tick) };
            }
            self.thread_timer_running = None;
        } else if !self.internal.threads.is_empty() && self.thread_timer_running.is_none() {
            let res = unsafe { SetTimer(self.hwnd, AZ_THREAD_TICK, 16, None) }; // 16ms timer
            self.thread_timer_running = Some(res);
        }
         */
    }

    // Stop all timers that have a NodeId attached to them because in the next
    // frame the NodeId would be invalid, leading to crashes / panics
    pub(crate) fn stop_timers_with_node_ids(&mut self) {
        let timers_to_remove = self.internal.timers
        .iter()
        .filter_map(|(id, timer)| timer.node_id.as_ref().map(|_| *id))
        .collect();

        self.start_stop_timers(FastHashMap::default(), timers_to_remove);
    }

    // ScrollResult contains information about what nodes need to be scrolled,
    // whether they were scrolled by the system or by the user and how far they
    // need to be scrolled
    pub(crate) fn do_system_scroll(&mut self, scroll: ScrollResult) {
        // for scrolled_node in scroll {
        //      self.render_api.scroll_node_with_id();
        //      let scrolled_rect = LogicalRect { origin: scroll_offset, size: visible.size };
        //      if !scrolled_node.scroll_bounds.contains(&scroll_rect) {
        //
        //      }
        // }
    }
}

pub fn get_monitors(app: &App) -> MonitorVec {
    azul_core::window::MonitorVec::from_const_slice(&[]) // TODO
}

pub fn run(app: App, root_window: WindowCreateOptions) -> Result<(), String> {
    
    let context = GlFunctions::initialize()?;

    let s = MacApp {
        functions: context.functions.clone(),
        active_menus: BTreeMap::new(),
        data: Arc::new(Mutex::new(AppData {
            userdata: app,
            windows: BTreeMap::new(),
        })),
    };

    autoreleasepool(|app| {

        let mtm = MainThreadMarker::new()
        .ok_or_else(|| format!("appkit::run(app, root_window) not on main thread"))?;
        
        let ns_opengl_class = ns_opengl_class();
        let any_opengl_class = ns_opengl_class.register();

        let ns_menu_class = menu::menu_handler_class();
        let any_menu_class = ns_menu_class.register();

        let app = NSApp(mtm);
        
        let (window_id, window) = create_nswindow(
            &mtm,
            root_window, 
            &any_opengl_class, 
            &any_menu_class,
            &s
        )?;

        // Show the window
        window.ns_window.as_ref().map(|s| s.makeKeyAndOrderFront(None));
    
        s.data.lock().unwrap().windows.insert(window_id, window);
        
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        app.activateIgnoringOtherApps(true);
        app.run();

        Ok(())
    })
}

// Creates an NSWindow and hooks up an NSOpenGLView
fn create_nswindow(
    mtm: &MainThreadMarker,
    mut options: WindowCreateOptions, 
    ns_opengl_view: &AnyClass,
    ns_menu_view: &AnyClass,
    megaclass: &MacApp,
) -> Result<(WindowId, Window), String> {

    use crate::{
        compositor::Compositor,
        wr_translate::{
            translate_document_id_wr,
            translate_id_namespace_wr,
            wr_translate_debug_flags,
            wr_translate_document_id,
        },
    };
    use azul_core::{
        callbacks::PipelineId,
        gl::GlContextPtr,
        window::{
            CursorPosition, HwAcceleration,
            LogicalPosition, ScrollResult,
            PhysicalSize, RendererType,
            WindowInternalInit, FullHitTest,
            WindowFrame,
        },
    };
    use webrender::api::ColorF as WrColorF;
    use webrender::ProgramCache as WrProgramCache;

    // let parent_window = options.platform_specific_options.parent_window;
    let width = options.state.size.dimensions.width as f64;
    let height = options.state.size.dimensions.height as f64;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));

    let style_mask = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::Miniaturizable;

    let mtm = MainThreadMarker::new().unwrap();
    let window = NSWindow::alloc(mtm);
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
        window,
        rect,
        style_mask,
        NSBackingStoreType::Buffered,
        false,
    ) };

    window.center();
    window.setTitle(&NSString::from_str(&options.state.title));

    let dpi_factor = window.screen().map(|s| s.backingScaleFactor()).unwrap_or(1.0);
    options.state.size.dpi = (dpi_factor * 96.0) as u32;

    let physical_size = if options.size_to_content {
        PhysicalSize {
            width: 0,
            height: 0,
        }
    } else {
        PhysicalSize {
            width: width as u32,
            height: height as u32,
        }
    };

    options.state.size.dimensions = physical_size.to_logical(dpi_factor as f32);

    let (window_id, gl_view) = create_opengl_view(
        rect, ns_opengl_view, megaclass
    );

    // Attach an observer for "will close" notifications
    let data_clone2 = megaclass.data.clone();
    let observer = unsafe {
        create_observer(
        &objc2_foundation::NSNotificationCenter::defaultCenter(),
        &NSWindowWillCloseNotification,
        move |notification| { window_will_close(Arc::clone(&data_clone2), window_id, mtm); },
    ) };

    // Make gl_view the content view of the window
    unsafe { window.setContentView(Some(&*(gl_view as *const _ as *const NSView))) };

    let rt = RendererType::Hardware;
    let gl_context_ptr: OptionGlContextPtr = Some(unsafe {
        let gl_context: *mut Object = msg_send![gl_view, openGLContext];
        let _: () = msg_send![gl_context, makeCurrentContext];
        let s = GlContextPtr::new(rt, megaclass.functions.clone());
        let _: () = msg_send![gl_context, flushBuffer];
        s
    }).into();


    // WindowInternal::new() may dispatch OpenGL calls,
    // need to make context current before invoking
    let gl_context: *mut Object = unsafe { msg_send![gl_view, openGLContext] };
    let _: () = unsafe { msg_send![gl_context, makeCurrentContext] };

    // Invoke callback to initialize UI for the first time
    let wr = WrRenderer::new(
        megaclass.functions.clone(),
        Box::new(super::Notifier {}),
        super::default_renderer_options(&options),
        super::WR_SHADER_CACHE,
    ).map_err(|e| format!("{e:?}"))?;

    let (mut renderer, sender) = wr;

    renderer.set_external_image_handler(Box::new(Compositor::default()));

    let mut render_api = sender.create_api();
    let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
    let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
    let pipeline_id = PipelineId::new();
    let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());
    let hit_tester = render_api
        .request_hit_tester(wr_translate_document_id(document_id))
        .resolve();
    let hit_tester_ref = &*hit_tester;
    
    let mut appdata_lock = match megaclass.data.lock() {
        Ok(o) => o,
        Err(e) => unsafe {
            return Err(format!("failed to lock app data on startup {e:?}"))
        },
    };


    let mut initial_resource_updates = Vec::new();
    let mut internal = {

        let appdata_lock = &mut *appdata_lock;
        let fc_cache = &mut appdata_lock.userdata.fc_cache;
        let image_cache = &appdata_lock.userdata.image_cache;
        let data = &mut appdata_lock.userdata.data;

        fc_cache.apply_closure(|fc_cache| {
            WindowInternal::new(
                WindowInternalInit {
                    window_create_options: options.clone(),
                    document_id,
                    id_namespace,
                },
                data,
                image_cache,
                &gl_context_ptr,
                &mut initial_resource_updates,
                &crate::app::CALLBACKS,
                fc_cache,
                azul_layout::do_the_relayout,
                |window_state, scroll_states, layout_results| {
                    crate::wr_translate::fullhittest_new_webrender(
                        hit_tester_ref,
                        document_id,
                        window_state.focused_node,
                        layout_results,
                        &window_state.mouse_state.cursor_position,
                        window_state.size.get_hidpi_factor(),
                    )
                },
            )
        })
    };


    /*
        // Since the menu bar affects the window size, set it first,
        // before querying the window size again
        let mut menu_bar = None;
        if let Some(m) = internal.get_menu_bar() {
            let mb = WindowsMenuBar::new(m);
            unsafe { SetMenu(hwnd, mb._native_ptr); }
            menu_bar = Some(mb);
        }
    */

    // If size_to_content is set, query the content size and adjust!
    if options.size_to_content {
        let content_size = internal.get_content_size();
        // window.setWidth(content_size.width);
        // window.setHeight(content_size.height);
        internal.current_window_state.size.dimensions = content_size;
    }

    let mut txn = WrTransaction::new();

    // re-layout the window content for the first frame
    // (since the width / height might have changed)
    let resize_result = {
        let mut uc = &mut appdata_lock.userdata;
        let fcc = &mut uc.fc_cache;
        let ic = &uc.image_cache;
        fcc.apply_closure(|mut fc_cache| {
            let size = internal.current_window_state.size.clone();
            let theme = internal.current_window_state.theme;

            internal.do_quick_resize(
                ic,
                &crate::app::CALLBACKS,
                azul_layout::do_the_relayout,
                &mut fc_cache,
                &gl_context_ptr,
                &size,
                theme,
            )
        })
    };

    wr_synchronize_updated_images(resize_result.updated_images, &mut txn);

    // glContext can be deactivated now
    let _: () = unsafe { msg_send![gl_context, flushBuffer] };

    txn.set_document_view(
        WrDeviceIntRect::from_size(
            WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32),
        )
    );
    render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);
    render_api.flush_scene_builder();

    // Build the display list and send it to webrender for the first time
    crate::wr_translate::rebuild_display_list(
        &mut internal,
        &mut render_api,
        &appdata_lock.userdata.image_cache,
        initial_resource_updates,
    );

    render_api.flush_scene_builder();

    crate::wr_translate::generate_frame(
        &mut internal,
        &mut render_api,
        true,
    );

    render_api.flush_scene_builder();

    /*
        // Get / store mouse cursor position, now that the window position is final
        let mut cursor_pos: POINT = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut cursor_pos); }
        unsafe { ScreenToClient(hwnd, &mut cursor_pos) };
        let cursor_pos_logical = LogicalPosition {
            x: cursor_pos.x as f32 / dpi_factor,
            y: cursor_pos.y as f32 / dpi_factor,
        };
        internal.current_window_state.mouse_state.cursor_position = if cursor_pos.x <= 0 || cursor_pos.y <= 0 {
            CursorPosition::Uninitialized
        } else {
            CursorPosition::InWindow(cursor_pos_logical)
        };
    */

    // Update the hit-tester to account for the new hit-testing functionality
    let hit_tester = render_api.request_hit_tester(wr_translate_document_id(document_id));

    // Done! Window is now created properly, display list has been built by
    // WebRender (window is ready to render), menu bar is visible and hit-tester
    // now contains the newest UI tree.

    if options.hot_reload {
        // SetTimer(regenerate_dom, 200ms);
    }

    // regenerate_dom (???) - necessary?
    let mut window = Window {
        id: window_id.clone(),
        ns_window: Some(window),
        internal,
        render_api,
        renderer: Some(renderer),
        hit_tester: AsyncHitTester::Requested(hit_tester),
        ns_close_observer: observer,
        gl_context_ptr: gl_context_ptr,
    };

    // invoke the window create callback, if there is any
    let (windows_to_create, windows_to_destroy) = 
    if let Some(create_callback) = options.create_callback.as_mut() {

        let _: () = unsafe { msg_send![gl_context, makeCurrentContext] };

        let uc = &mut appdata_lock.userdata;
        let fcc = &mut uc.fc_cache;
        let ud = &mut uc.data;
        let ic1 = &mut uc.image_cache;
        let sysc = &uc.config.system_callbacks;

        let ccr = fcc.apply_closure(|mut fc_cache| {

            let raw_window_ptr = window.ns_window.take().map(|s| {
                Retained::<NSWindow>::into_raw(s)
            }).unwrap_or_else(|| std::ptr::null_mut());

            let s = window.internal.invoke_single_callback(
                create_callback,
                ud,
                &RawWindowHandle::MacOS(MacOSHandle {
                    ns_view: gl_view as *mut c_void,
                    ns_window: raw_window_ptr as *mut c_void,
                }),
                &window.gl_context_ptr,
                ic1,
                &mut fc_cache,
                sysc,
            );

            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(raw_window_ptr) };

            s
        });

        let ntc = NodesToCheck::empty(
            window.internal.current_window_state.mouse_state.mouse_down(),
            window.internal.current_window_state.focused_node.clone(),
        );

        let mut new_windows = Vec::new();
        let mut destroyed_windows = Vec::new();

        let ret = {
            let mut ud = &mut appdata_lock.userdata;
            let ic = &mut ud.image_cache;
            let fc = &mut ud.fc_cache;
            super::process::process_callback_results(
                ccr,
                &mut window,
                &ntc,
                ic,
                fc,
                &mut new_windows,
                &mut destroyed_windows,
            )
        };

        let _: () = unsafe { msg_send![gl_context, flushBuffer] };

        (new_windows, destroyed_windows)
    } else {
        (Vec::new(), Vec::new())
    };
    
    mem::drop(appdata_lock);

    create_windows(
        &mtm, 
        megaclass, 
        windows_to_create, 
        ns_opengl_view, 
        ns_menu_view
    );

    destroy_windows(megaclass, windows_to_destroy);

    Ok((window_id, window))
}


fn create_windows(
    mtm: &MainThreadMarker,
    app: &MacApp,
    new: Vec<WindowCreateOptions>,
    any_opengl_class: &AnyClass,
    any_menu_class: &AnyClass,
) {
    for opts in new {
        let w = create_nswindow(
            &mtm,
            opts, 
            &any_opengl_class, 
            &any_menu_class,
            &app,
        );

        if let Ok((id, w)) = w {
            if let Ok(mut lock) = app.data.lock() {
                lock.windows.insert(id, w);
            }
        }
    }
}

fn destroy_windows(
    app: &MacApp, 
    old: Vec<WindowId>
) {
    for window in old {
        let _ = app.data
        .try_lock().ok()
        .and_then(|s| {
            s.windows.get(&window)
            .and_then(|w| w.ns_window.as_ref())
            .map(|w| w.close())
        });
    }
}

pub(crate) fn synchronize_window_state_with_os(window: &Window) {
    // TODO: window.set_title
}

fn create_observer(
    center: &NSNotificationCenter,
    name: &NSNotificationName,
    handler: impl Fn(&NSNotification) + 'static,
) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {

    let block = block2::RcBlock::new(
        move |notification: NonNull<NSNotification>| {
            handler(unsafe { notification.as_ref() });
        }
    );

    // SAFETY: Per the docs, `addObserverForName:object:queue:usingBlock:` is safe as long as
    // we keep the observer alive, which we do by storing it in our struct.
    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(name),
            None, // No sender filter
            None, // No queue => use the posting thread
            &block,
        )
    }
}


// Creates a class as a subclass of NSOpenGLView, and registers a "megastruct" ivar, which
// holds a pointer to the NSObject
fn ns_opengl_class() -> ClassBuilder {

    let superclass = class!(NSOpenGLView);
    let c = CString::new("AzulOpenGLView").unwrap();
    let mut decl = ClassDecl::new(&c.as_c_str(), superclass)
    .expect("Failed to create ClassDecl for AzulOpenGLView");

    unsafe {
        
        let i: CString = CString::new("app").unwrap();
        decl.add_ivar::<*mut core::ffi::c_void>(&i.as_c_str());
        let i: CString = CString::new("windowid").unwrap();
        decl.add_ivar::<i64>(i.as_c_str());

        decl.add_method(
            sel!(initWithFrame:pixelFormat:),
            init_with_frame_pixel_format as extern "C" fn(*mut Object, Sel, NSRect, *mut Object) -> *mut Object
        );

        decl.add_method(sel!(drawRect:), draw_rect as extern "C" fn(*mut Object, Sel, NSRect));
        decl.add_method(sel!(mouseDown:), mouse_down as extern "C" fn(*mut Object, Sel, *mut Object));
        decl.add_method(sel!(mouseUp:), mouse_up as extern "C" fn(*mut Object, Sel, *mut Object));
        decl.add_method(sel!(mouseMoved:), mouse_moved as extern "C" fn(*mut Object, Sel, *mut Object));
        decl.add_method(sel!(scrollWheel:), scroll_wheel as extern "C" fn(*mut Object, Sel, *mut Object));
        decl.add_method(sel!(keyDown:), key_down as extern "C" fn(*mut Object, Sel, *mut Object));
        decl.add_method(sel!(keyUp:), key_up as extern "C" fn(*mut Object, Sel, *mut Object));
    }

    decl
}

/// Creates a custom instance of the given `ns_opengl_view` class template,
/// storing a pointer to the `megaclass` inside the templates `app` field.
fn create_opengl_view(
    frame: NSRect,
    ns_opengl_view: &AnyClass,
    megaclass: &MacApp,
) -> (WindowId, id) { // Retained<NSOpenGLView>
    unsafe {

        let new_window_id = WindowId::new();

        // 2) Create an NSOpenGLPixelFormat
        let attrs = [
            objc2_app_kit::NSOpenGLPFAAccelerated,
            objc2_app_kit::NSOpenGLPFADoubleBuffer,
            objc2_app_kit::NSOpenGLPFAColorSize,
            24,
            objc2_app_kit::NSOpenGLPFADepthSize,
            24,
            objc2_app_kit::NSOpenGLPFAStencilSize,
            8,
            objc2_app_kit::NSOpenGLPFAOpenGLProfile,
            objc2_app_kit::NSOpenGLProfileVersion3_2Core, // <- use OpenGL 3.2 core
            0, // terminator
        ];

        let pixel_format: *mut Object = msg_send![class!(NSOpenGLPixelFormat), alloc];
        let pixel_format: *mut Object = msg_send![
            pixel_format,
            initWithAttributes: attrs.as_ptr()
        ];
        assert!(!pixel_format.is_null(), "Failed to create NSOpenGLPixelFormat");

        // 3) Allocate and init our "AzulOpenGLView" instance
        let view: *mut Object = msg_send![ns_opengl_view, alloc];
        let view: *mut Object = msg_send![view, initWithFrame: frame pixelFormat: pixel_format];
        assert!(!view.is_null(), "Failed to init AzulOpenGLView");

        *((*view).get_mut_ivar("app")) = megaclass as *const _ as *const c_void;
        *((*view).get_mut_ivar("windowid")) = new_window_id.id;

        let _: () = msg_send![view, setWantsBestResolutionOpenGLSurface: YES];

        // On Mojave, views automatically become layer-backed shortly after being added to
        // a window. Changing the layer-backedness of a view breaks the association between
        // the view and its associated OpenGL context. To work around this, on Mojave we
        // explicitly make the view layer-backed up front so that AppKit doesn't do it
        // itself and break the association with its context.
        if unsafe { NSAppKitVersionNumber }.floor() > NSAppKitVersionNumber10_12 {
            let _: () = msg_send![view, setWantsLayer: YES];
        }

        (new_window_id, view)
    }
}

/// Override of `[NSOpenGLView initWithFrame:pixelFormat:]`.
///
/// - Chains up to `[super initWithFrame:frame pixelFormat:format]`.
extern "C" fn init_with_frame_pixel_format(
    this: *mut Object,
    _sel: Sel,
    frame: NSRect,
    format: *mut Object
) -> *mut Object {
    unsafe {
        // Call [super initWithFrame:frame pixelFormat:format]
        let superclass = class!(NSOpenGLView);
        let this: *mut Object = msg_send![super(this, superclass),
            initWithFrame: frame
            pixelFormat: format
        ];

        // You could do more setup logic if `this` != null.
        this
    }
}

extern "C" 
fn window_will_close(ptr: Arc<Mutex<AppData>>, window_id: WindowId, mtm: MainThreadMarker) {
    
    let mut app_data = match ptr.try_lock().ok() {
        Some(s) => s,
        None => {
            // close notification can fire twice for the same window,
            // could lead to deadlock, hence try_lock
            return;
        },
    };

    app_data.windows.remove(&window_id);
    if app_data.windows.is_empty() {
        unsafe { objc2_app_kit::NSApplication::sharedApplication(mtm).terminate(None); }
    }
}

extern "C" 
fn mouse_down(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        if let Some(data) = ptr.data.lock().ok() {
            // shared_data.process_mouse_down();
            println!("mouse down!");
        }
    }
}

extern "C" 
fn mouse_up(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        if let Some(data) = ptr.data.lock().ok() {
            // shared_data.process_mouse_up();
            println!("mouse up!");
        }
    }
}

extern "C" 
fn mouse_moved(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        if let Some(data) = ptr.data.lock().ok() {
            // shared_data.process_mouse_move();
            println!("mouse moved!");
        }
    }
}

extern "C" 
fn scroll_wheel(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        if let Some(data) = ptr.data.lock().ok() {
            let delta_y: f64 = msg_send![event, scrollingDeltaY]; // deltaY, et.c
            // data.process_scroll(delta_y as f32);
            // [this setNeedsDisplay, YES]
            println!("scrolled {delta_y}");
        }
    }
}

extern "C" 
fn key_down(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        if let Some(data) = ptr.data.lock().ok() {
            // query [event keyCode], [event modifierFlags], etc.
            // data.process_key_down();
            println!("key down");
        }
    }
}

extern "C" 
fn key_up(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        if let Some(data) = ptr.data.lock().ok() {
            // data.process_key_up();
            println!("key up");
        }
    }
}

extern "C" 
fn draw_rect(this: *mut AnyObject, _sel: Sel, _dirty_rect: NSRect) {
    unsafe {
        // Retrieve the pointer to our Arc<Mutex<AppData>> from the ivar
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        // REDRAWING: if the width / height of the window differ from the display list w / h,
        // relayout the code here (yes, in the redrawing function)

        let glc = Window::make_current_gl(msg_send![this, openGLContext]);

        const GL_COLOR_BUFFER_BIT: u32 = 0x00004000;
        ptr.functions.clear_color(0.0, 1.0, 0.0, 1.0);
        ptr.functions.clear(GL_COLOR_BUFFER_BIT); 

        Window::finish_gl(glc);
    }
}
