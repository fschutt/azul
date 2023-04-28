use crate::{
    app::{App, LazyFcCache},
    wr_translate::{
        rebuild_display_list,
        generate_frame,
        synchronize_gpu_values,
        scroll_all_nodes,
        wr_synchronize_updated_images,
        AsyncHitTester,
    }
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc
};
use azul_core::{
    FastBTreeSet, FastHashMap,
    app_resources::{
        ImageMask, ImageRef, Epoch,
        AppConfig, ImageCache, ResourceUpdate,
        RendererResources, GlTextureCache,
    },
    callbacks::{
        RefAny, UpdateImageType,
        DomNodeId, DocumentId
    },
    gl::OptionGlContextPtr,
    task::{Thread, ThreadId, Timer, TimerId},
    ui_solver::LayoutResult,
    styled_dom::DomId,
    dom::NodeId,
    display_list::RenderCallbacks,
    window::{
        LogicalSize, Menu, MenuCallback, MenuItem,
        MonitorVec, WindowCreateOptions, WindowInternal,
        WindowState, FullWindowState, ScrollResult,
        MouseCursorType, CallCallbacksResult
    },
    window_state::NodesToCheck,
};
use core::{
    fmt,
    convert::TryInto,
    cell::{BorrowError, BorrowMutError, RefCell},
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
use gl_context_loader::GenericGlContext;
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
use core_foundation::base::{CFIndex, CFRelease};
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopSourceContext,
    CFRunLoopSourceCreate, CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
};
use objc2::foundation::is_main_thread;
use objc2::rc::{autoreleasepool, Id, Shared};
use objc2::{msg_send_id, ClassType};
use raw_window_handle::{AppKitDisplayHandle, RawDisplayHandle};

// Copied from rust-windowing/winit @ 249609889029af3e5ec19afbd02464e11a265378
// because we need to inline a lot of dependencies for macOS such as glutin / glium
pub mod winit;

use self::winit::appkit::{NSApp, NSApplicationActivationPolicy, NSEvent};

#[derive(Debug)]
pub enum CocoaWindowCreateError {
    FailedToCreateHWND(u32),
    NoHDC,
    NoGlContext,
    Renderer(WrRendererError),
    BorrowMut(BorrowMutError),
}

#[derive(Debug, Copy, Clone)]
pub enum CocoaOpenGlError {
    OpenGL32DllNotFound(u32),
    FailedToGetDC(u32),
    FailedToCreateHiddenHWND(u32),
    FailedToGetPixelFormat(u32),
    NoMatchingPixelFormat(u32),
    OpenGLNotAvailable(u32),
    FailedToStoreContext(u32),
}

#[derive(Debug)]
pub enum CocoaStartupError {
    NoAppInstance(u32),
    WindowCreationFailed,
    Borrow(BorrowError),
    BorrowMut(BorrowMutError),
    Create(CocoaWindowCreateError),
    Gl(CocoaOpenGlError),
}

pub fn get_monitors(app: &App) -> MonitorVec {
    MonitorVec::from_const_slice(&[]) // TODO
}


pub struct EventLoop<T: 'static> {
    /// The delegate is only weakly referenced by NSApplication, so we keep
    /// it around here as well.
    _delegate: Id<ApplicationDelegate, Shared>,

    window_target: Rc<RootWindowTarget<T>>,
    panic_info: Rc<PanicInfo>,

    /// We make sure that the callback closure is dropped during a panic
    /// by making the event loop own it.
    ///
    /// Every other reference should be a Weak reference which is only upgraded
    /// into a strong reference in order to call the callback but then the
    /// strong reference should be dropped as soon as possible.
    _callback: Option<Rc<Callback<T>>>,
}

/// Main function that starts when app.run() is invoked
pub fn run(app: App, root_window: WindowCreateOptions) -> Result<isize, CocoaStartupError> {

    // This must be done before `NSApp()` (equivalent to sending
    // `sharedApplication`) is called anywhere else, or we'll end up
    // with the wrong `NSApplication` class and the wrong thread could
    // be marked as main.
    let app: Id<WinitApplication, Shared> =
        unsafe { msg_send_id![WinitApplication::class(), sharedApplication] };

    use NSApplicationActivationPolicy::*;
    let activation_policy = match attributes.activation_policy {
        ActivationPolicy::Regular => NSApplicationActivationPolicyRegular,
        ActivationPolicy::Accessory => NSApplicationActivationPolicyAccessory,
        ActivationPolicy::Prohibited => NSApplicationActivationPolicyProhibited,
    };
    let delegate = ApplicationDelegate::new(
        activation_policy,
        attributes.default_menu,
        attributes.activate_ignoring_other_apps,
    );

    autoreleasepool(|_| {
        app.setDelegate(&delegate);
    });

    let panic_info: Rc<PanicInfo> = Default::default();
    setup_control_flow_observers(Rc::downgrade(&panic_info));

    let s = EventLoop {
            _delegate: delegate,
            window_target: Rc::new(RootWindowTarget {
                p: Default::default(),
                _marker: PhantomData,
            }),
            panic_info,
            _callback: None,
        };

    // This transmute is always safe, in case it was reached through `run`, since our
    // lifetime will be already 'static. In other cases caller should ensure that all data
    // they passed to callback will actually outlive it, some apps just can't move
    // everything to event loop, so this is something that they should care about.
    let callback = unsafe {
        mem::transmute::<
            Rc<RefCell<dyn FnMut(Event<'_, T>, &RootWindowTarget<T>, &mut ControlFlow)>>,
            Rc<RefCell<dyn FnMut(Event<'_, T>, &RootWindowTarget<T>, &mut ControlFlow)>>,
        >(Rc::new(RefCell::new(callback)))
    };

    s._callback = Some(Rc::clone(&callback));

    let exit_code = autoreleasepool(|_| {
        let app = NSApp();

        // A bit of juggling with the callback references to make sure
        // that `s.callback` is the only owner of the callback.
        let weak_cb: Weak<_> = Rc::downgrade(&callback);
        drop(callback);

        AppState::set_callback(weak_cb, Rc::clone(&s.window_target));
        unsafe { app.run() };

        if let Some(panic) = s.panic_info.take() {
            drop(s._callback.take());
            resume_unwind(panic);
        }
        AppState::exit()
    });
    drop(s._callback.take());

    Ok(exit_code)
}