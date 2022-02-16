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
use std::ffi::CString;

#[derive(Debug)]
pub enum LinuxWindowCreateError {
    X(String),
    NoGlContext,
    Renderer(WrRendererError),
    BorrowMut(BorrowMutError),
}

#[derive(Debug, Copy, Clone)]
pub enum LinuxOpenGlError {
    OpenGL32DllNotFound(u32),
    FailedToGetDC(u32),
    FailedToCreateHiddenHWND(u32),
    FailedToGetPixelFormat(u32),
    NoMatchingPixelFormat(u32),
    OpenGLNotAvailable(u32),
    FailedToStoreContext(u32),
}

#[derive(Debug)]
pub enum LinuxStartupError {
    NoAppInstance(u32),
    WindowCreationFailed,
    Borrow(BorrowError),
    BorrowMut(BorrowMutError),
    Create(LinuxWindowCreateError),
    Gl(LinuxOpenGlError),
}

impl From<LinuxWindowCreateError> for LinuxStartupError {
    fn from(e: LinuxWindowCreateError) -> LinuxStartupError {
        LinuxStartupError::Create(e)
    }
}

pub fn get_monitors(app: &App) -> MonitorVec {
    MonitorVec::from_const_slice(&[]) // TODO
}


extern crate x11_dl;

const GL_TRUE: i32 = 1;
const GL_FALSE: i32 = 0;

const GL_DEPTH_TEST: GLenum = 0x0B71;

type GLenum = u32;
type GLboolean = u8;
type GLbitfield =   u32;
type GLbyte =       i8;
type GLshort =      i16;
type GLint =        i32;
type GLsizei =      i32;
type GLubyte =      u8;
type GLushort =     u16;
type GLuint =       u8;
type GLfloat =      f32;
type GLclampf =     f32;
type GLdouble =     f64;
type GLclampd =     f64;
type GLvoid =       ();


#[link(kind = "dylib", name = "GL")]
extern {
    fn glEnable(cap: GLenum) -> ();
    fn glViewport(x: GLint, y: GLint, width: GLsizei, height: GLsizei) -> ();
}

/// Main function that starts when app.run() is invoked
pub fn run(app: App, root_window: WindowCreateOptions) -> Result<isize, LinuxStartupError> {

    use self::LinuxWindowCreateError::*;
    use x11_dl::xlib::{self, *};
    use x11_dl::glx::{self, Glx};

    let xlib = Xlib::open()
        .map_err(|e| X(format!("{}", e.detail())))?;

    let display_int = 0_i8;
    let dpy = unsafe { (xlib.XOpenDisplay)(&display_int) };

    let mut display = {
        if dpy.is_null() {
            return Err(X(format!("X11: No display found")).into());
        } else {
            unsafe { &mut*dpy }
        }
    };

    let root = unsafe { (xlib.XDefaultRootWindow)(display) };

    let glx_ext = Glx::open()
    .map_err(|e| X(format!("GLX: {}", e.detail())))?;

    let mut att = [
        glx::GLX_RGBA,
        glx::GLX_DEPTH_SIZE,
        24,
        glx::GLX_DOUBLEBUFFER,
        glx::GLX_NONE
    ];

    let vi = unsafe { (glx_ext.glXChooseVisual)(dpy, 0, &mut att[0]) };

    let mut visual_info = if vi.is_null() {
        return Err(X(format!("X11: No display found")).into());
    } else {
        unsafe { &mut*vi }
    };

    let cmap = unsafe { (xlib.XCreateColormap)(display, root, visual_info.visual, AllocNone) };

    let mut window_attributes: XSetWindowAttributes = unsafe { std::mem::zeroed() };
    window_attributes.event_mask = ExposureMask | KeyPressMask;
    window_attributes.colormap = cmap;

    // construct window
    let window = unsafe { (xlib.XCreateWindow)(display, root, 0, 0, 600, 600, 0, visual_info.depth,
                                            1 /* InputOutput */, visual_info.visual,
                                            CWColormap | CWEventMask,
                                            &mut window_attributes) };

    let window_title = CString::new("Hello, world!").unwrap();

    // show window
    unsafe { (xlib.XMapWindow)(display, window) };
    unsafe { (xlib.XStoreName)(display, window, window_title.as_ptr()) };

    let glc = unsafe { (glx_ext.glXCreateContext)(display, &mut *visual_info, ptr::null_mut(), GL_TRUE) };
    unsafe { (glx_ext.glXMakeCurrent)(display, window, glc) };

    unsafe { glEnable(GL_DEPTH_TEST) }; /* todo */

    let mut cur_xevent = XEvent { pad: [0;24] };
    let mut cur_window_attributes: XWindowAttributes = unsafe { mem::zeroed() };

    loop {

        unsafe { (xlib.XNextEvent)(display, &mut cur_xevent) };

        let cur_event_type = cur_xevent.get_type();

        match cur_event_type {
            xlib::Expose => {
                unsafe { (xlib.XGetWindowAttributes)(display, window, &mut cur_window_attributes) };
                unsafe { glViewport(
                    0, 0,
                    cur_window_attributes.width,
                    cur_window_attributes.height
                    );
                };
                /* do drawing here */
                unsafe { (glx_ext.glXSwapBuffers)(display, window) };
            },
            xlib::KeyPress => {
                unsafe { (glx_ext.glXMakeCurrent)(display, 0 /* None ? */, ptr::null_mut()) };
                unsafe { (glx_ext.glXDestroyContext)(display, glc) };
                unsafe { (xlib.XDestroyWindow)(display, window) };
                unsafe { (xlib.XCloseDisplay)(display) };
                break;
            },
            _ => { },
        }
    }

    Ok(0)
}