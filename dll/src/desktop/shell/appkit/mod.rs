#![cfg(target_os = "macos")]
use std::{
    collections::BTreeMap,
    ffi::{c_void, CStr, CString},
    fmt, mem,
    os::raw::c_char,
    ptr,
    ptr::NonNull,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use azul_core::{
    dom::DomNodeId,
    events::NodesToCheck,
    geom::{LogicalPosition, PhysicalPositionI32, PhysicalSize},
    gl::OptionGlContextPtr,
    hit_test::HitTestItem,
    menu::Menu,
    resources::ImageCache,
    task::{ThreadId, TimerId},
    window::{
        CursorPosition, HwAcceleration, MacOSHandle, MonitorVec, MouseCursorType, RawWindowHandle,
        RendererType, ScrollResult, VirtualKeyCode, WindowFrame, WindowId, WindowPosition,
        WindowsHandle,
    },
    FastBTreeSet, FastHashMap,
};
use azul_layout::{
    callbacks::MenuCallback, thread::Thread, timer::Timer, window::LayoutWindow,
    window_state::WindowCreateOptions,
};
use gl_context_loader::GenericGlContext;
use objc2::{
    declare::ClassDecl,
    ffi::{id, nil},
    rc::{autoreleasepool, AutoreleasePool, Id, Retained},
    runtime::{AnyClass, AnyObject, Class, ClassBuilder, Object, ProtocolObject, Sel, YES},
    *,
};
use objc2_app_kit::{
    NSApp, NSAppKitVersionNumber, NSAppKitVersionNumber10_12, NSApplicationActivationPolicy,
    NSBackingStoreType, NSView, NSWindow, NSWindowDidBecomeKeyNotification,
    NSWindowDidChangeBackingPropertiesNotification, NSWindowDidResignKeyNotification,
    NSWindowDidResizeNotification, NSWindowStyleMask, NSWindowWillCloseNotification,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSNotificationCenter, NSNotificationName, NSObjectProtocol,
    NSPoint, NSRect, NSSize, NSString,
};
use webrender::{
    api::{
        units::{
            DeviceIntPoint as WrDeviceIntPoint, DeviceIntRect as WrDeviceIntRect,
            DeviceIntSize as WrDeviceIntSize, LayoutSize as WrLayoutSize,
        },
        ApiHitTester as WrApiHitTester, DocumentId as WrDocumentId,
        HitTesterRequest as WrHitTesterRequest, RenderNotifier as WrRenderNotifier,
    },
    render_api::RenderApi as WrRenderApi,
    PipelineInfo as WrPipelineInfo, Renderer as WrRenderer, RendererError as WrRendererError,
    RendererOptions as WrRendererOptions, ShaderPrecacheFlags as WrShaderPrecacheFlags,
    Shaders as WrShaders, Transaction as WrTransaction,
};

use self::gl::GlFunctions;
use super::{CommandMap, MenuTarget};
use crate::desktop::{
    app::{self, App, LazyFcCache},
    compositor::Compositor,
    wr_translate::{
        generate_frame, rebuild_display_list, translate_document_id_wr, translate_id_namespace_wr,
        wr_synchronize_updated_images, wr_translate_document_id, AsyncHitTester,
    },
};

mod gl;
mod menu;

/// OpenGL context guard, to be returned by window.make_current_gl()
pub(crate) struct GlContextGuard {
    glc: *mut Object,
}

pub(crate) struct MacApp {
    pub(crate) functions: Rc<GenericGlContext>,
    pub(crate) data: Arc<Mutex<AppData>>,
}

pub(crate) struct AppData {
    pub userdata: App,
    pub active_menus: BTreeMap<MenuTarget, CommandMap>,
    pub windows: BTreeMap<WindowId, Window>,
}

pub(crate) struct Window {
    /// Internal Window ID
    pub(crate) id: WindowId,
    /// NSWindow
    pub(crate) ns_window: Option<Retained<NSWindow>>,
    /// Observer that fires a notification when the window is closed
    pub(crate) observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    /// See azul-core, stores the entire UI (DOM, CSS styles, layout results, etc.)
    pub(crate) internal: LayoutWindow,
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
    // Creates an NSWindow and hooks up an NSOpenGLView
    fn create(
        mtm: &MainThreadMarker,
        mut options: WindowCreateOptions,
        ns_opengl_view: &AnyClass,
        ns_menu_view: &AnyClass,
        megaclass: &MacApp,
    ) -> Result<(WindowId, Window), String> {
        use webrender::ProgramCache as WrProgramCache;

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
                objc2_app_kit::NSBackingStoreType::Buffered,
                false,
            )
        };

        window.center();
        window.setTitle(&NSString::from_str(&options.state.title));

        let dpi_factor = window
            .screen()
            .map(|s| s.backingScaleFactor())
            .unwrap_or(1.0);
        options.state.size.dpi = (dpi_factor * 96.0) as u32;

        let physical_size = if options.size_to_content {
            PhysicalSize::zero()
        } else {
            PhysicalSize {
                width: width as u32,
                height: height as u32,
            }
        };

        options.state.size.dimensions = physical_size.to_logical(dpi_factor as f32);

        // Create custom NSOpenGLView
        let (window_id, gl_view) = create_opengl_view(rect, ns_opengl_view, megaclass);

        // Attach an observer for "will close", "will resize", etc. notifications
        let observers = create_observers(&megaclass, window_id, mtm);

        unsafe {
            // Make gl_view the content view
            window.setContentView(Some(&*(gl_view as *const _ as *const NSView)));
        }

        // For hardware acceleration, we do an NSOpenGLView with core profile
        // Or fallback to software if you like
        let rt = match options.renderer.as_ref() {
            Some(r) if r.hw_accel == HwAcceleration::Disabled => RendererType::Software,
            // otherwise try hardware first
            _ => RendererType::Hardware,
        };

        // Grab the “OptionGlContextPtr” by “making current” once
        let gl_context_ptr: OptionGlContextPtr = Some(unsafe {
            let gl_context: *mut Object = msg_send![gl_view, openGLContext];
            let _: () = msg_send![gl_context, makeCurrentContext];
            let ptr = azul_core::gl::GlContextPtr::new(rt, megaclass.functions.clone());
            let _: () = msg_send![gl_context, flushBuffer];
            ptr
        })
        .into();

        // Build a WR renderer
        let wr = webrender::Renderer::new(
            megaclass.functions.clone(),
            Box::new(super::Notifier {}),
            super::default_renderer_options(&options),
            super::WR_SHADER_CACHE,
        )
        .map_err(|e| format!("{e:?}"))?;

        let (mut renderer, sender) = wr;

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        let mut render_api = sender.create_api();

        // Our “initial” device size
        let framebuffer_size =
            WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

        let doc_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let pipeline_id = azul_core::hit_test::PipelineId::new();
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // Force a flush on creation
        render_api.flush_scene_builder();

        // Next, we set up the window’s internal:
        let mut appdata_lock = megaclass
            .data
            .lock()
            .map_err(|e| format!("Failed to lock app data: {e}"))?;

        let uc = &mut appdata_lock.userdata;
        let fcc = &mut uc.fc_cache;
        let ud = &mut uc.data;
        let ic1 = &mut uc.image_cache;
        let sysc = &uc.config.system_callbacks;

        let mut initial_resource_updates = Vec::new();
        let mut internal = fcc.apply_closure(|fc_cache| {
            LayoutWindow::new(
                LayoutWindowInit {
                    window_create_options: options.clone(),
                    document_id: doc_id,
                    id_namespace,
                },
                ud,
                ic1,
                &gl_context_ptr,
                &mut initial_resource_updates,
                &crate::desktop::app::CALLBACKS,
                fc_cache,
                azul_layout::solver2::do_the_relayout,
                |window_state, scroll_states, layout_results| {
                    crate::desktop::wr_translate::fullhittest_new_webrender(
                        &*render_api
                            .request_hit_tester(wr_translate_document_id(doc_id))
                            .resolve(),
                        doc_id,
                        window_state.focused_node,
                        layout_results,
                        &window_state.mouse_state.cursor_position,
                        window_state.size.get_hidpi_factor(),
                    )
                },
                &mut None, // no debug messages
            )
        });

        if options.size_to_content {
            let content_size = internal.get_content_size();
            let current_frame = window.frame();
            internal.current_window_state.size.dimensions = content_size;

            let resized_frame = NSRect::new(
                NSPoint::new(current_frame.origin.x, current_frame.origin.y),
                NSSize::new(content_size.width as f64, content_size.height as f64),
            );

            window.setFrame_display(resized_frame, true);
        }

        // sync LayoutWindow with reality
        let current_frame = window.frame();
        internal.current_window_state.size.dimensions.width =
            current_frame.size.width.round() as f32;
        internal.current_window_state.size.dimensions.height =
            current_frame.size.height.round() as f32;
        internal.current_window_state.position = WindowPosition::Initialized(PhysicalPositionI32 {
            x: current_frame.origin.x.round() as i32,
            y: current_frame.origin.y.round() as i32,
        });

        // Now do a quick resize
        let mut txn = WrTransaction::new();
        let resize_result = fcc.apply_closure(|mut fc_cache| {
            let size = internal.current_window_state.size.clone();
            let theme = internal.current_window_state.theme;
            internal.do_quick_resize(
                ic1,
                &crate::desktop::app::CALLBACKS,
                azul_layout::solver2::do_the_relayout,
                &mut fc_cache,
                &gl_context_ptr,
                &size,
                theme,
            )
        });
        wr_synchronize_updated_images(resize_result.updated_images, &mut txn);

        txn.set_document_view(WrDeviceIntRect::from_size(framebuffer_size));
        render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);
        render_api.flush_scene_builder();

        // Rebuild display list for the first time
        rebuild_display_list(
            &mut internal,
            &mut render_api,
            ic1,
            initial_resource_updates,
        );
        render_api.flush_scene_builder();
        generate_frame(&mut internal, &mut render_api, true);
        render_api.flush_scene_builder();

        let hit_tester = render_api.request_hit_tester(wr_translate_document_id(doc_id));

        // Put all that into our Window struct
        let mut window = Window {
            id: window_id,
            ns_window: Some(window),
            observers,
            internal,
            render_api,
            renderer: Some(renderer),
            hit_tester: AsyncHitTester::Requested(hit_tester),
            gl_context_ptr,
        };

        // invoke the window create callback, if there is any
        // If you have “hot_reload” => set up a 200ms timer, etc. (omitted for brevity)
        // If you have a create_callback => call it now
        if let Some(create_callback) = options.create_callback.as_mut() {
            let gl_context: *mut Object = unsafe { msg_send![gl_view, openGLContext] };
            unsafe {
                let _: () = msg_send![gl_context, makeCurrentContext];
            }

            let ccr = fcc.apply_closure(|mut fc_cache| {
                let raw_window_ptr = window
                    .ns_window
                    .take()
                    .map(|s| Retained::<NSWindow>::into_raw(s))
                    .unwrap_or_else(|| std::ptr::null_mut());

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
                window
                    .internal
                    .current_window_state
                    .mouse_state
                    .mouse_down(),
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

            unsafe {
                let _: () = msg_send![gl_context, flushBuffer];
            }

            mem::drop(appdata_lock);

            create_windows(&mtm, megaclass, new_windows, ns_opengl_view, ns_menu_view);

            destroy_windows(megaclass, destroyed_windows);
        } else {
            mem::drop(appdata_lock);
        }

        Ok((window_id, window))
    }

    // functions necessary for the az_ event handling

    pub fn get_id(&self) -> WindowId {
        self.id.clone()
    }

    /// Utility for making the GL context current, returning a guard that resets it afterwards
    pub(crate) fn make_current_gl(gl_context: *mut Object) -> GlContextGuard {
        unsafe {
            let _: () = msg_send![gl_context, makeCurrentContext];
            GlContextGuard { glc: gl_context }
        }
    }

    pub fn swap_buffers(&mut self, handle: &RawWindowHandle) {
        // TODO
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

    /// Mousse has entered the window
    pub(crate) fn on_mouse_enter(&mut self, prev: CursorPosition, cur: CursorPosition) {
        // TODO
    }

    pub(crate) fn on_cursor_change(&mut self, prev: Option<MouseCursorType>, cur: MouseCursorType) {
        // TODO
    }

    pub(crate) fn set_cursor(&mut self, handle: &RawWindowHandle, cursor: MouseCursorType) {
        // TODO
    }

    pub(crate) fn create_and_open_context_menu(
        &self,
        context_menu: &Menu,
        hit: &HitTestItem,
        node_id: DomNodeId,
        active_menus: &mut BTreeMap<MenuTarget, CommandMap>,
    ) {
        // TODO: on Windows, this creates a menu and calls TrackCursorPosition
    }

    pub(crate) fn destroy(
        &mut self,
        userdata: &mut App,
        guard: &GlContextGuard,
        handle: &RawWindowHandle,
        gl_functions: Rc<GenericGlContext>,
    ) {
        // TODO: if necessary, deallocation of GL objects, called on window closing
    }

    // --- functions necessary for process.rs handling

    pub(crate) fn start_stop_timers(
        &mut self,
        added: FastHashMap<TimerId, Timer>,
        removed: FastBTreeSet<TimerId>,
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
        removed: FastBTreeSet<ThreadId>,
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
        let timers_to_remove = self
            .internal
            .timers
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
        data: Arc::new(Mutex::new(AppData {
            userdata: app,
            windows: BTreeMap::new(),
            active_menus: BTreeMap::new(),
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

        let (window_id, window) =
            Window::create(&mtm, root_window, &any_opengl_class, &any_menu_class, &s)?;

        // Show the window
        window
            .ns_window
            .as_ref()
            .map(|s| s.makeKeyAndOrderFront(None));

        s.data.lock().unwrap().windows.insert(window_id, window);

        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        app.activateIgnoringOtherApps(true);
        app.run();

        Ok(())
    })
}

fn create_windows(
    mtm: &MainThreadMarker,
    app: &MacApp,
    new: Vec<WindowCreateOptions>,
    any_opengl_class: &AnyClass,
    any_menu_class: &AnyClass,
) {
    for opts in new {
        let w = Window::create(&mtm, opts, &any_opengl_class, &any_menu_class, &app);

        if let Ok((id, w)) = w {
            if let Ok(mut lock) = app.data.lock() {
                lock.windows.insert(id, w);
            }
        }
    }
}

fn destroy_windows(app: &MacApp, old: Vec<WindowId>) {
    for window in old {
        let _ = app.data.try_lock().ok().and_then(|s| {
            s.windows
                .get(&window)
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
    let block = block2::RcBlock::new(move |notification: NonNull<NSNotification>| {
        handler(unsafe { notification.as_ref() });
    });

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

fn create_observers(
    megaclass: &MacApp,
    window_id: WindowId,
    mtm: MainThreadMarker,
) -> Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>> {
    let center = unsafe { objc2_foundation::NSNotificationCenter::defaultCenter() };

    // Add a “will close” observer
    let data_clone2 = megaclass.data.clone();
    let close_observer = unsafe {
        create_observer(
            &center,
            &NSWindowWillCloseNotification,
            move |notification| {
                window_will_close(Arc::clone(&data_clone2), window_id, mtm);
            },
        )
    };

    // ADDED: Observer for “becomeKey” → triggers wm_set_focus
    let data_clone_b = megaclass.data.clone();
    let focus_observer = unsafe {
        create_observer(
            &center,
            &NSWindowDidBecomeKeyNotification,
            move |notification| {
                window_did_become_key(Arc::clone(&data_clone_b), window_id, mtm);
            },
        )
    };

    // ADDED: Observer for “resignKey” → triggers wm_kill_focus
    let data_clone_r = megaclass.data.clone();
    let resign_observer = unsafe {
        create_observer(
            &center,
            &NSWindowDidResignKeyNotification,
            move |notification| {
                window_did_resign_key(Arc::clone(&data_clone_r), window_id, mtm);
            },
        )
    };

    // ADDED: Observer for “didResize” → triggers wm_size
    let data_clone_z = megaclass.data.clone();
    let resize_observer = unsafe {
        create_observer(
            &center,
            &NSWindowDidResizeNotification,
            move |notification| {
                window_did_resize(Arc::clone(&data_clone_z), window_id, mtm);
            },
        )
    };

    // ADDED: Observer for “didChangeScreenBackingProperties” → triggers “dpichange”
    let data_clone_dpi = megaclass.data.clone();
    let dpi_observer = unsafe {
        create_observer(
            &center,
            &NSWindowDidChangeBackingPropertiesNotification,
            move |notification| {
                window_did_change_backing(Arc::clone(&data_clone_dpi), window_id, mtm);
            },
        )
    };

    vec![
        close_observer,
        focus_observer,
        resign_observer,
        resize_observer,
        dpi_observer,
    ]
}

// Creates a class as a subclass of NSOpenGLView, and registers a "megastruct" ivar, which
// holds a pointer to the NSObject
fn ns_opengl_class() -> ClassBuilder {
    use std::ffi::CString;

    use objc2::{
        declare::ClassDecl,
        runtime::{Class, Object, Sel},
    };
    use objc2_foundation::NSRect;

    let superclass = class!(NSOpenGLView);
    let c = CString::new("AzulOpenGLView").unwrap();

    let mut decl =
        ClassDecl::new(&c, superclass).expect("Failed to create ClassDecl for AzulOpenGLView");

    unsafe {
        // Register the ivars "app" (pointer to MacApp) and "windowid" (WindowId)
        let i = CString::new("app").unwrap();
        decl.add_ivar::<*mut core::ffi::c_void>(i.as_c_str());

        let i = CString::new("windowid").unwrap();
        decl.add_ivar::<i64>(i.as_c_str());

        // Register the custom initializer
        decl.add_method(
            sel!(initWithFrame:pixelFormat:),
            init_with_frame_pixel_format
                as extern "C" fn(*mut Object, Sel, NSRect, *mut Object) -> *mut Object,
        );

        // Register the drawRect: override
        decl.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(*mut Object, Sel, NSRect),
        );

        // Left mouse
        decl.add_method(
            sel!(mouseDown:),
            mouse_down as extern "C" fn(*mut Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(mouseUp:),
            mouse_up as extern "C" fn(*mut Object, Sel, *mut Object),
        );

        // Right mouse
        decl.add_method(
            sel!(rightMouseDown:),
            rightMouseDown as extern "C" fn(*mut Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(rightMouseUp:),
            rightMouseUp as extern "C" fn(*mut Object, Sel, *mut Object),
        );

        // Middle / extra mouse
        decl.add_method(
            sel!(otherMouseDown:),
            otherMouseDown as extern "C" fn(*mut Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(otherMouseUp:),
            otherMouseUp as extern "C" fn(*mut Object, Sel, *mut Object),
        );

        // Mouse movement & scroll
        decl.add_method(
            sel!(mouseMoved:),
            mouseMoved as extern "C" fn(*mut Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(scrollWheel:),
            scrollWheel as extern "C" fn(*mut Object, Sel, *mut Object),
        );

        // Keyboard
        decl.add_method(
            sel!(keyDown:),
            keyDown as extern "C" fn(*mut Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(keyUp:),
            keyUp as extern "C" fn(*mut Object, Sel, *mut Object),
        );
    }

    decl
}

/// Creates a custom instance of the given `ns_opengl_view` class template,
/// storing a pointer to the `megaclass` inside the templates `app` field.
fn create_opengl_view(
    frame: NSRect,
    ns_opengl_view: &AnyClass,
    megaclass: &MacApp,
) -> (WindowId, id) {
    // Retained<NSOpenGLView>
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
            0,                                            // terminator
        ];

        let pixel_format: *mut Object = msg_send![class!(NSOpenGLPixelFormat), alloc];
        let pixel_format: *mut Object = msg_send![
            pixel_format,
            initWithAttributes: attrs.as_ptr()
        ];
        assert!(
            !pixel_format.is_null(),
            "Failed to create NSOpenGLPixelFormat"
        );

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
    format: *mut Object,
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

fn window_will_close(ptr: Arc<Mutex<AppData>>, window_id: WindowId, mtm: MainThreadMarker) {
    let mut app_data = match ptr.try_lock().ok() {
        Some(s) => s,
        None => {
            // close notification can fire twice for the same window,
            // could lead to deadlock, hence try_lock
            return;
        }
    };

    app_data.windows.remove(&window_id);
    if app_data.windows.is_empty() {
        unsafe {
            objc2_app_kit::NSApplication::sharedApplication(mtm).terminate(None);
        }
    }
}

fn window_did_resize(ptr: Arc<Mutex<AppData>>, window_id: WindowId, _mtm: MainThreadMarker) {
    let mut data = match ptr.try_lock().ok() {
        Some(s) => s,
        None => return,
    };

    let mut data = &mut *data;
    let dw = &mut data.windows;
    let ud = &mut data.userdata;

    let mut window = match dw.get_mut(&window_id) {
        Some(s) => s,
        None => return,
    };

    let mut nsw = match window.ns_window.take() {
        Some(s) => s,
        None => return,
    };

    let mut cv = match nsw.contentView() {
        Some(s) => s,
        None => {
            // Put the NSWindow back to avoid losing it
            window.ns_window = Some(nsw);
            return;
        }
    };

    // Get the new window bounds + DPI
    let bounds: NSRect = unsafe { msg_send![&*cv, bounds] };
    let bsf: CGFloat = unsafe { msg_send![&*cv, backingScaleFactor] };
    let w = bounds.size.width.round() as u32;
    let h = bounds.size.height.round() as u32;
    let new_dpi = (bsf * 96.0).round() as u32;

    let guard = Window::make_current_gl(unsafe { msg_send![&*cv, openGLContext] });

    let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
    let raw = RawWindowHandle::MacOS(MacOSHandle {
        ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
        ns_window: nswr,
    });

    let r = crate::desktop::shell::event::wm_size(
        window,
        &mut data.userdata,
        &guard,
        &raw,
        PhysicalSize::new(w, h),
        new_dpi,
        WindowFrame::Normal, // Or detect Minimized / Fullscreen if needed
    );

    mem::drop(window);

    if let Some(window) = dw.get_mut(&window_id) {
        window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
    }

    Window::finish_gl(guard);
}

fn window_did_become_key(ptr: Arc<Mutex<AppData>>, window_id: WindowId, _mtm: MainThreadMarker) {
    let mut data = match ptr.try_lock().ok() {
        Some(s) => s,
        None => return,
    };

    let mut data = &mut *data;
    let dw = &mut data.windows;
    let ud = &mut data.userdata;

    let mut window = match dw.get_mut(&window_id) {
        Some(s) => s,
        None => return,
    };

    let mut nsw = match window.ns_window.take() {
        Some(s) => s,
        None => return,
    };

    let mut cv = match nsw.contentView() {
        Some(s) => s,
        None => {
            window.ns_window = Some(nsw);
            return;
        }
    };

    let guard = Window::make_current_gl(unsafe { msg_send![&*cv, openGLContext] });

    let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
    let raw = RawWindowHandle::MacOS(MacOSHandle {
        ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
        ns_window: nswr,
    });

    let r = crate::desktop::shell::event::wm_set_focus(window, ud, &guard, &raw);

    mem::drop(window);

    let _ = crate::desktop::shell::event::handle_process_event_result(
        r, dw, window_id, ud, &guard, &raw,
    );

    if let Some(window) = dw.get_mut(&window_id) {
        window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
    }

    Window::finish_gl(guard);
}

fn window_did_resign_key(ptr: Arc<Mutex<AppData>>, window_id: WindowId, _mtm: MainThreadMarker) {
    let mut data = match ptr.try_lock().ok() {
        Some(s) => s,
        None => return,
    };

    let mut data = &mut *data;
    let dw = &mut data.windows;
    let ud = &mut data.userdata;

    let mut window = match dw.get_mut(&window_id) {
        Some(s) => s,
        None => return,
    };

    let mut nsw = match window.ns_window.take() {
        Some(s) => s,
        None => return,
    };

    let mut cv = match nsw.contentView() {
        Some(s) => s,
        None => {
            window.ns_window = Some(nsw);
            return;
        }
    };

    let guard = Window::make_current_gl(unsafe { msg_send![&*cv, openGLContext] });

    let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
    let raw = RawWindowHandle::MacOS(MacOSHandle {
        ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
        ns_window: nswr,
    });

    let r = crate::desktop::shell::event::wm_kill_focus(window, ud, &guard, &raw);

    mem::drop(window);

    let _ = crate::desktop::shell::event::handle_process_event_result(
        r, dw, window_id, ud, &guard, &raw,
    );

    if let Some(window) = dw.get_mut(&window_id) {
        window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
    }

    Window::finish_gl(guard);
}

// ADDED: Called when the screen’s backing scale changes => “dpichange”
fn window_did_change_backing(
    ptr: Arc<Mutex<AppData>>,
    window_id: WindowId,
    _mtm: MainThreadMarker,
) {
    let mut data = match ptr.try_lock().ok() {
        Some(s) => s,
        None => return,
    };

    let mut data = &mut *data;
    let dw = &mut data.windows;
    let ud = &mut data.userdata;

    let mut window = match dw.get_mut(&window_id) {
        Some(s) => s,
        None => return,
    };

    let mut nsw = match window.ns_window.take() {
        Some(s) => s,
        None => return,
    };

    let mut cv = match nsw.contentView() {
        Some(s) => s,
        None => {
            window.ns_window = Some(nsw);
            return;
        }
    };

    let new_dpi: CGFloat = unsafe { msg_send![&*cv, backingScaleFactor] };
    let new_dpi = (new_dpi * 96.0).round() as u32;

    let guard = Window::make_current_gl(unsafe { msg_send![&*cv, openGLContext] });

    let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
    let raw = RawWindowHandle::MacOS(MacOSHandle {
        ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
        ns_window: nswr,
    });

    let r = crate::desktop::shell::event::wm_dpichanged(window, ud, &guard, &raw, new_dpi);

    mem::drop(window);

    let _ = crate::desktop::shell::event::handle_process_event_result(
        r, dw, window_id, ud, &guard, &raw,
    );

    if let Some(window) = dw.get_mut(&window_id) {
        window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
    }

    Window::finish_gl(guard);
}

//
// Left-click down => wm_lbuttondown
//
extern "C" fn mouse_down(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        // Retrieve the Window
        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        // Take ns_window, early return if none
        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        // get the NSView
        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        // Make the GL context current
        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        // Create a raw handle
        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        // Call your "wm_lbuttondown" event
        let r = crate::desktop::shell::event::wm_lbuttondown(window, ud, &guard, &raw);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Left-click up => wm_lbuttonup
//
extern "C" fn mouse_up(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        let r = crate::desktop::shell::event::wm_lbuttonup(
            window,
            ud,
            &guard,
            &raw,
            &mut data.active_menus,
        );

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Right-click down => wm_rbuttondown
//
#[no_mangle]
extern "C" fn rightMouseDown(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        let r = crate::desktop::shell::event::wm_rbuttondown(window, ud, &guard, &raw);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Right-click up => wm_rbuttonup
//
#[no_mangle]
extern "C" fn rightMouseUp(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        let r = crate::desktop::shell::event::wm_rbuttonup(
            window,
            ud,
            &guard,
            &raw,
            &mut data.active_menus,
        );

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Middle-click down => wm_mbuttondown
//
#[no_mangle]
extern "C" fn otherMouseDown(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        let r = crate::desktop::shell::event::wm_mbuttondown(window, ud, &guard, &raw);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Middle-click up => wm_mbuttonup
//
#[no_mangle]
extern "C" fn otherMouseUp(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&WindowId { id: windowid }) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        let r = crate::desktop::shell::event::wm_mbuttonup(window, ud, &guard, &raw);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Mouse moved => wm_mousemove
//
#[no_mangle]
extern "C" fn mouseMoved(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let location: NSPoint = msg_send![event, locationInWindow];
        let newpos = LogicalPosition::new(location.x as f32, location.y as f32);

        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&WindowId { id: windowid }) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        let r = crate::desktop::shell::event::wm_mousemove(window, ud, &guard, &raw, newpos);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Scroll wheel => wm_mousewheel
//
#[no_mangle]
extern "C" fn scrollWheel(this: *mut Object, _sel: Sel, event: *mut Object) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        // If you want the scroll amount:
        let delta_y: f64 = msg_send![event, scrollingDeltaY];
        let r =
            crate::desktop::shell::event::wm_mousewheel(window, ud, &guard, &raw, delta_y as f32);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Key down => wm_keydown
//
#[no_mangle]
extern "C" fn keyDown(this: *mut Object, _sel: Sel, event: *mut Object) {
    println!("keyDown");
    unsafe {
        let keycode: u16 = msg_send![event, keyCode];
        let scancode = keycode as u32;
        let vk: Option<VirtualKeyCode> = cocoa_keycode_to_vkc(keycode);

        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        // Possibly parse [event keyCode], etc., then call wm_keydown
        let r = crate::desktop::shell::event::wm_keydown(window, ud, &guard, &raw, scancode, vk);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

//
// Key up => wm_keyup
//
#[no_mangle]
extern "C" fn keyUp(this: *mut Object, _sel: Sel, event: *mut Object) {
    println!("keyUp");
    unsafe {
        let keycode: u16 = msg_send![event, keyCode];
        let scancode = keycode as u32;
        let vk: Option<VirtualKeyCode> = cocoa_keycode_to_vkc(keycode);

        let ptr = (*this).get_ivar::<*const c_void>("app");
        let mac_app = &*(*ptr as *const MacApp);

        let windowid = *(*this).get_ivar::<i64>("windowid");
        let wid = WindowId { id: windowid };
        let mut data = match mac_app.data.lock().ok() {
            Some(d) => d,
            None => return,
        };

        let data = &mut *data;
        let mut dw = &mut data.windows;
        let mut ud = &mut data.userdata;

        let mut window = match dw.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let guard = Window::make_current_gl(msg_send![&*cv, openGLContext]);

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        // Possibly parse [event keyCode], then call wm_keyup
        let r = crate::desktop::shell::event::wm_keyup(window, ud, &guard, &raw, scancode, vk);

        mem::drop(window);

        let _ =
            crate::desktop::shell::event::handle_process_event_result(r, dw, wid, ud, &guard, &raw);

        if let Some(window) = dw.get_mut(&wid) {
            window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };
        }

        Window::finish_gl(guard);
    }
}

/// The main "drawRect:" which we override from NSOpenGLView, hooking up the calls
/// to do_resize / rebuild_display_list / gpu_scroll_render depending on the size.
extern "C" fn draw_rect(this: *mut AnyObject, _sel: Sel, dirty_rect: NSRect) {
    unsafe {
        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let mac_app = &*ptr;
        let functions = mac_app.functions.clone();
        let windowid = *(*this).get_ivar::<i64>("windowid");

        // Grab the lock to get the Window
        let mut app_data = match mac_app.data.lock() {
            Ok(o) => o,
            Err(e) => return,
        };

        let mut app_data = &mut *app_data;
        let w = &mut app_data.windows;
        let ud = &mut app_data.userdata;
        let wid = WindowId { id: windowid };
        let mut window = match w.get_mut(&wid) {
            Some(s) => s,
            None => return,
        };

        let bounds: NSRect = msg_send![this, bounds];
        let bsf: CGFloat = msg_send![this, backingScaleFactor];
        let bdpi = (bsf * 96.0).round() as u32;
        let bw = bounds.size.width.round() as u32;
        let bh = bounds.size.height.round() as u32;

        let stored_size = window.internal.current_window_state.size.dimensions;
        let ssw = stored_size.width.round() as u32;
        let ssh = stored_size.height.round() as u32;
        let sdpi = window.internal.current_window_state.size.dpi;

        let glc = Window::make_current_gl(msg_send![this, openGLContext]);

        const GL_COLOR_BUFFER_BIT: u32 = 0x00004000;
        mac_app.functions.clear_color(1.0, 1.0, 1.0, 1.0);
        mac_app.functions.clear(GL_COLOR_BUFFER_BIT);

        let mut nsw = match window.ns_window.take() {
            Some(s) => s,
            None => return,
        };

        let mut cv = match nsw.contentView() {
            Some(s) => s,
            None => {
                window.ns_window = Some(nsw);
                return;
            }
        };

        let nswr = Retained::<NSWindow>::into_raw(nsw) as *mut _;
        let raw = RawWindowHandle::MacOS(MacOSHandle {
            ns_view: Retained::<NSView>::into_raw(cv) as *mut _,
            ns_window: nswr,
        });

        crate::desktop::shell::event::wm_paint(window, ud, &glc, &raw, functions);

        window.ns_window = unsafe { Retained::<NSWindow>::from_raw(nswr as *mut _) };

        Window::finish_gl(glc);
    }
}

/// Converts a macOS (Cocoa) key code (`[NSEvent keyCode]`) into an `Option<VirtualKeyCode>`.
/// Note: This is incomplete; add more branches as needed for your layout.
pub fn cocoa_keycode_to_vkc(keycode: u16) -> Option<VirtualKeyCode> {
    match keycode {
        // Letters
        0 => Some(VirtualKeyCode::A),
        1 => Some(VirtualKeyCode::S),
        2 => Some(VirtualKeyCode::D),
        3 => Some(VirtualKeyCode::F),
        4 => Some(VirtualKeyCode::H),
        5 => Some(VirtualKeyCode::G),
        6 => Some(VirtualKeyCode::Z),
        7 => Some(VirtualKeyCode::X),
        8 => Some(VirtualKeyCode::C),
        9 => Some(VirtualKeyCode::V),
        11 => Some(VirtualKeyCode::B),
        12 => Some(VirtualKeyCode::Q),
        13 => Some(VirtualKeyCode::W),
        14 => Some(VirtualKeyCode::E),
        15 => Some(VirtualKeyCode::R),
        16 => Some(VirtualKeyCode::Y),
        17 => Some(VirtualKeyCode::T),

        // Number row
        18 => Some(VirtualKeyCode::Key1),
        19 => Some(VirtualKeyCode::Key2),
        20 => Some(VirtualKeyCode::Key3),
        21 => Some(VirtualKeyCode::Key4),
        22 => Some(VirtualKeyCode::Key6),
        23 => Some(VirtualKeyCode::Key5),
        24 => Some(VirtualKeyCode::Equals), // '=' on ANSI layout
        25 => Some(VirtualKeyCode::Key9),
        26 => Some(VirtualKeyCode::Key7),
        27 => Some(VirtualKeyCode::Minus),
        28 => Some(VirtualKeyCode::Key8),
        29 => Some(VirtualKeyCode::Key0),
        30 => Some(VirtualKeyCode::RBracket),
        31 => Some(VirtualKeyCode::O),
        32 => Some(VirtualKeyCode::U),
        33 => Some(VirtualKeyCode::LBracket),
        34 => Some(VirtualKeyCode::I),
        35 => Some(VirtualKeyCode::P),

        // Punctuation / special
        36 => Some(VirtualKeyCode::Return),
        37 => Some(VirtualKeyCode::L),
        38 => Some(VirtualKeyCode::J),
        39 => Some(VirtualKeyCode::Apostrophe), // "'"
        40 => Some(VirtualKeyCode::K),
        41 => Some(VirtualKeyCode::Semicolon),
        42 => Some(VirtualKeyCode::Backslash),
        43 => Some(VirtualKeyCode::Comma),
        44 => Some(VirtualKeyCode::Slash),
        45 => Some(VirtualKeyCode::N),
        46 => Some(VirtualKeyCode::M),
        47 => Some(VirtualKeyCode::Period),
        48 => Some(VirtualKeyCode::Tab),
        49 => Some(VirtualKeyCode::Space),
        50 => Some(VirtualKeyCode::Grave), // '`' / '~'
        51 => Some(VirtualKeyCode::Back),  // Backspace
        53 => Some(VirtualKeyCode::Escape),

        // Numpad & function keys
        71 => Some(VirtualKeyCode::NumpadDecimal),
        76 => Some(VirtualKeyCode::NumpadEnter),
        78 => Some(VirtualKeyCode::NumpadSubtract),
        81 => Some(VirtualKeyCode::NumpadDivide),
        82 => Some(VirtualKeyCode::Numpad0),
        83 => Some(VirtualKeyCode::Numpad1),
        84 => Some(VirtualKeyCode::Numpad2),
        85 => Some(VirtualKeyCode::Numpad3),
        86 => Some(VirtualKeyCode::Numpad4),
        87 => Some(VirtualKeyCode::Numpad5),
        88 => Some(VirtualKeyCode::Numpad6),
        89 => Some(VirtualKeyCode::Numpad7),
        91 => Some(VirtualKeyCode::Numpad8),
        92 => Some(VirtualKeyCode::Numpad9),
        96 => Some(VirtualKeyCode::F5),
        97 => Some(VirtualKeyCode::F6),
        98 => Some(VirtualKeyCode::F7),
        99 => Some(VirtualKeyCode::F3),
        100 => Some(VirtualKeyCode::F8),
        101 => Some(VirtualKeyCode::F9),
        103 => Some(VirtualKeyCode::F11),
        105 => Some(VirtualKeyCode::F13),
        106 => Some(VirtualKeyCode::F14),
        107 => Some(VirtualKeyCode::F10),
        109 => Some(VirtualKeyCode::F12),
        111 => Some(VirtualKeyCode::F15),
        113 => Some(VirtualKeyCode::F16),
        114 => Some(VirtualKeyCode::Home), // 'Help' on some keyboards
        115 => Some(VirtualKeyCode::PageUp),
        116 => Some(VirtualKeyCode::Up),
        117 => Some(VirtualKeyCode::Delete), // 'Fn+Backspace'?
        118 => Some(VirtualKeyCode::F4),
        119 => Some(VirtualKeyCode::End),
        120 => Some(VirtualKeyCode::F2),
        121 => Some(VirtualKeyCode::PageDown),
        122 => Some(VirtualKeyCode::F1),
        123 => Some(VirtualKeyCode::Left),
        124 => Some(VirtualKeyCode::Right),
        125 => Some(VirtualKeyCode::Down),
        126 => Some(VirtualKeyCode::Up),

        // etc... (Add any missing codes you need)
        // e.g. 127 => Some(VirtualKeyCode::F17), etc.
        _ => None,
    }
}
