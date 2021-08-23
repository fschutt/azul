#![cfg(target_os = "windows")]
#![allow(non_snake_case)]

//! Win32 implementation of the window shell containing all functions
//! related to running the application

use crate::{
    app::{App, LazyFcCache},
    wr_translate::{
        rebuild_display_list,
        generate_frame,
        synchronize_gpu_values,
        scroll_all_nodes,
        wr_synchronize_resize,
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
use winapi::{
    shared::{
        minwindef::{BOOL, HINSTANCE, LPARAM, LRESULT, TRUE, UINT, WPARAM},
        ntdef::HRESULT,
        windef::{HDC, HGLRC, HMENU, HWND, RECT, POINT},
    },
    ctypes::wchar_t,
    um::dwmapi::{DWM_BB_ENABLE, DWM_BLURBEHIND},
    um::uxtheme::MARGINS,
    um::winuser::WM_APP,
};
use crate::wr_translate::AsyncHitTester;

type TIMERPTR = winapi::shared::basetsd::UINT_PTR;

// ID sent by WM_TIMER to re-generate the DOM
const AZ_TICK_REGENERATE_DOM: usize = 1;
// ID sent by WM_TIMER to check the thread results
const AZ_THREAD_TICK: usize = 2;

const AZ_REGENERATE_DOM: u32 = WM_APP + 1;
const AZ_REGENERATE_DISPLAY_LIST: u32 = WM_APP + 2;
const AZ_REDO_HIT_TEST: u32 = WM_APP + 3;
const AZ_GPU_SCROLL_RENDER: u32 = WM_APP + 4;

const CLASS_NAME: &str = "AzulApplicationClass";

// TODO: Cache compiled shaders between renderers
const WR_SHADER_CACHE: Option<&Rc<RefCell<WrShaders>>> = None;

trait RectTrait {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}

impl RectTrait for RECT {
    fn width(&self) -> u32 {
        (self.right - self.left).max(0) as u32
    }
    fn height(&self) -> u32 {
        (self.bottom - self.top).max(0) as u32
    }
}

pub fn get_monitors(app: &App) -> MonitorVec {
    MonitorVec::from_const_slice(&[]) // TODO
}

/// Main function that starts when app.run() is invoked
pub fn run(app: App, root_window: WindowCreateOptions) -> Result<isize, WindowsStartupError> {

    use winapi::{
        shared::minwindef::FALSE,
        um::{
            libloaderapi::GetModuleHandleW,
            wingdi::{wglMakeCurrent, CreateSolidBrush},
            winbase::{INFINITE, WAIT_FAILED},
            winuser::{
                DispatchMessageW, GetDC, GetMessageW,
                RegisterClassW, ReleaseDC, SetProcessDPIAware,
                TranslateMessage, MsgWaitForMultipleObjects,
                PeekMessageW, GetForegroundWindow,
                CS_HREDRAW, CS_OWNDC, QS_ALLEVENTS,
                CS_VREDRAW, MSG, WNDCLASSW, PM_NOREMOVE, PM_NOYIELD
            }
        },
    };

    let hinstance = unsafe { GetModuleHandleW(ptr::null_mut()) };
    if hinstance.is_null() {
        return Err(WindowsStartupError::NoAppInstance(get_last_error()));
    }

    // Tell windows that this process is DPI-aware
    unsafe { SetProcessDPIAware(); } // Vista
    // SetProcessDpiAwareness(); Win8.1
    // SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE); // Win10

    // Register the application class (shared between windows)
    let mut class_name = encode_wide(CLASS_NAME);
    let mut wc: WNDCLASSW = unsafe { mem::zeroed() };
    wc.style = CS_HREDRAW | CS_VREDRAW | CS_OWNDC;
    wc.hInstance = hinstance;
    wc.lpszClassName = class_name.as_mut_ptr();
    wc.lpfnWndProc = Some(WindowProc);
    wc.hCursor = ptr::null_mut();
    wc.hbrBackground = unsafe { CreateSolidBrush(0x00000000) }; // transparent black

    // RegisterClass can fail if the same class is
    // registered twice, error can be ignored
    unsafe { RegisterClassW(&wc) };

    let dwm = DwmFunctions::initialize();
    let gl = GlFunctions::initialize();

    let mut active_hwnds = Rc::new(RefCell::new(BTreeSet::new()));

    {
        let App {
            data,
            config,
            windows,
            image_cache,
            fc_cache,
        } = app;

        let app_data_inner = Rc::new(RefCell::new(ApplicationData {
            hinstance,
            data,
            config,
            image_cache,
            fc_cache,
            windows: BTreeMap::new(),
            active_hwnds: active_hwnds.clone(),
            dwm,
        }));

        for opts in windows {
            if let Ok(w) = Window::create(hinstance, opts, SharedApplicationData { inner: app_data_inner.clone() }) {
                active_hwnds.try_borrow_mut()?.insert(w.hwnd);
                app_data_inner
                    .try_borrow_mut()?
                    .windows
                    .insert(w.get_id(), w);
            }
        }

        if let Ok(w) = Window::create(hinstance, root_window, SharedApplicationData { inner: app_data_inner.clone() }) {
            active_hwnds.try_borrow_mut()?.insert(w.hwnd);
            app_data_inner
                .try_borrow_mut()?
                .windows
                .insert(w.get_id(), w);
        }
    }

    // Process the window messages one after another
    //
    // Multiple windows will process messages in sequence
    // to avoid complicated multithreading logic
    let mut msg: MSG = unsafe { mem::zeroed() };
    let mut results = Vec::new();
    let mut hwnds = Vec::new();

    'main: loop {

        match active_hwnds.try_borrow().ok() {
            Some(windows_vec) => {
                hwnds = windows_vec.clone().into_iter().collect();
            },
            None => break 'main, // borrow error
        }

        // For single-window apps, GetMessageW will block until
        // the next event comes in. For multi-window apps we have
        // to use PeekMessage in order to not block in case that
        // there are no messages for that window

        let is_multiwindow = match hwnds.len() {
            0 | 1 => false,
            _ => true,
        };

        if is_multiwindow {

            for hwnd in hwnds.iter() {
                unsafe {
                    let r = PeekMessageW(&mut msg, *hwnd, 0, 0, PM_NOREMOVE);

                    if r > 0 {
                        // new message available
                        let r = GetMessageW(&mut msg, *hwnd, 0, 0);
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                        results.push(r);
                    }
                }
            }

            // It would be great if there was a function like
            // MsgWaitForMultipleObjects([hwnd]), so that you could
            // wait on one of many input events
            //
            // The best workaround is to get the foreground window
            // (that the user is interacting with) and then
            // wait until some event happens to that foreground window
            let mut dump_msg: MSG = unsafe { mem::zeroed() };
            while !hwnds.iter().any(|hwnd| unsafe { PeekMessageW(&mut dump_msg, *hwnd, 0, 0, PM_NOREMOVE) > 0 }) {
                // reduce CPU load for multi-window apps
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        } else {
            for hwnd in hwnds.iter() {
                unsafe {
                    let r = GetMessageW(&mut msg, *hwnd, 0, 0);
                    if r > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    results.push(r);
                }
            }
        }

        for r in results.iter() {
            if !(*r > 0) {
                break 'main; // error occured
            }
        }

        if hwnds.is_empty() {
            break 'main;
        }

        hwnds.clear();
        results.clear();
    }

    Ok(msg.wParam as isize)
}

fn encode_wide(input: &str) -> Vec<u16> {
    input
        .encode_utf16()
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>()
}

fn encode_ascii(input: &str) -> Vec<i8> {
    input
        .chars()
        .filter(|c| c.is_ascii())
        .map(|c| c as i8)
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>()
}

fn get_last_error() -> u32 {
    use winapi::um::errhandlingapi::GetLastError;
    (unsafe { GetLastError() }) as u32
}

fn load_dll(name: &'static str) -> Option<HINSTANCE> {
    use winapi::um::libloaderapi::LoadLibraryW;
    let mut dll_name = encode_wide(name);
    let dll = unsafe { LoadLibraryW(dll_name.as_mut_ptr()) };
    if dll.is_null() {
        None
    } else {
        Some(dll)
    }
}

#[derive(Debug)]
pub enum WindowsWindowCreateError {
    FailedToCreateHWND(u32),
    NoHDC,
    NoGlContext,
    Extra(ExtraWglFunctionsLoadError),
    Renderer(WrRendererError),
    BorrowMut(BorrowMutError),
}

impl From<ExtraWglFunctionsLoadError> for WindowsWindowCreateError {
    fn from(e: ExtraWglFunctionsLoadError) -> Self {
        WindowsWindowCreateError::Extra(e)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum WindowsOpenGlError {
    OpenGL32DllNotFound(u32),
    FailedToGetDC(u32),
    FailedToCreateHiddenHWND(u32),
    FailedToGetPixelFormat(u32),
    NoMatchingPixelFormat(u32),
    OpenGLNotAvailable(u32),
    FailedToStoreContext(u32),
}

#[derive(Debug)]
pub enum WindowsStartupError {
    NoAppInstance(u32),
    WindowCreationFailed,
    Borrow(BorrowError),
    BorrowMut(BorrowMutError),
    Create(WindowsWindowCreateError),
    Gl(WindowsOpenGlError),
}

impl From<BorrowError> for WindowsStartupError {
    fn from(e: BorrowError) -> Self {
        WindowsStartupError::Borrow(e)
    }
}
impl From<BorrowMutError> for WindowsStartupError {
    fn from(e: BorrowMutError) -> Self {
        WindowsStartupError::BorrowMut(e)
    }
}
impl From<WindowsWindowCreateError> for WindowsStartupError {
    fn from(e: WindowsWindowCreateError) -> Self {
        WindowsStartupError::Create(e)
    }
}
impl From<WindowsOpenGlError> for WindowsStartupError {
    fn from(e: WindowsOpenGlError) -> Self {
        WindowsStartupError::Gl(e)
    }
}

struct Notifier {}

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier {})
    }
    fn wake_up(&self, composite_needed: bool) {}
    fn new_frame_ready(
        &self,
        _: WrDocumentId,
        _scrolled: bool,
        composite_needed: bool,
        _render_time: Option<u64>,
    ) {
    }
}

#[derive(Debug, Clone)]
struct SharedApplicationData {
    inner: Rc<RefCell<ApplicationData>>,
}

// ApplicationData struct that is shared across
#[derive(Debug)]
struct ApplicationData {
    hinstance: HINSTANCE,
    data: RefAny,
    config: AppConfig,
    image_cache: ImageCache,
    fc_cache: LazyFcCache,
    windows: BTreeMap<usize, Window>,
    // active HWNDS, tracked separately from the ApplicationData
    active_hwnds: Rc<RefCell<BTreeSet<HWND>>>,
    dwm: Option<DwmFunctions>,
}

// Extra functions from dwmapi.dll
struct DwmFunctions {
    _dwmapi_dll_handle: HINSTANCE,
    DwmEnableBlurBehindWindow: Option<extern "system" fn(HWND, &DWM_BLURBEHIND) -> HRESULT>,
    DwmExtendFrameIntoClientArea: Option<extern "system" fn(HWND, &MARGINS) -> HRESULT>,
    DwmDefWindowProc: Option<extern "system" fn(HWND, u32, WPARAM, LPARAM, *mut LRESULT)>,
}

impl fmt::Debug for DwmFunctions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (self._dwmapi_dll_handle as usize).fmt(f)?;
        (self.DwmEnableBlurBehindWindow.map(|f| f as usize)).fmt(f)?;
        (self.DwmExtendFrameIntoClientArea.map(|f| f as usize)).fmt(f)?;
        (self.DwmExtendFrameIntoClientArea.map(|f| f as usize)).fmt(f)?;
        Ok(())
    }
}

impl DwmFunctions {
    fn initialize() -> Option<Self> {
        use winapi::um::libloaderapi::{GetProcAddress, LoadLibraryW};

        let mut dll_name = encode_wide("dwmapi.dll");
        let hDwmAPI_DLL = unsafe { LoadLibraryW(dll_name.as_mut_ptr()) };
        if hDwmAPI_DLL.is_null() {
            return None; // dwmapi.dll not found
        }

        let mut func_name = encode_ascii("DwmEnableBlurBehindWindow");
        let DwmEnableBlurBehindWindow =
            unsafe { GetProcAddress(hDwmAPI_DLL, func_name.as_mut_ptr()) };
        let DwmEnableBlurBehindWindow = if DwmEnableBlurBehindWindow != ptr::null_mut() {
            Some(unsafe { mem::transmute(DwmEnableBlurBehindWindow) })
        } else {
            None
        };

        let mut func_name = encode_ascii("DwmExtendFrameIntoClientArea");
        let DwmExtendFrameIntoClientArea =
            unsafe { GetProcAddress(hDwmAPI_DLL, func_name.as_mut_ptr()) };
        let DwmExtendFrameIntoClientArea = if DwmExtendFrameIntoClientArea != ptr::null_mut() {
            Some(unsafe { mem::transmute(DwmExtendFrameIntoClientArea) })
        } else {
            None
        };

        let mut func_name = encode_ascii("DwmDefWindowProc");
        let DwmDefWindowProc = unsafe { GetProcAddress(hDwmAPI_DLL, func_name.as_mut_ptr()) };
        let DwmDefWindowProc = if DwmDefWindowProc != ptr::null_mut() {
            Some(unsafe { mem::transmute(DwmDefWindowProc) })
        } else {
            None
        };

        Some(Self {
            _dwmapi_dll_handle: hDwmAPI_DLL,
            DwmEnableBlurBehindWindow,
            DwmExtendFrameIntoClientArea,
            DwmDefWindowProc,
        })
    }
}

impl Drop for DwmFunctions {
    fn drop(&mut self) {
        use winapi::um::libloaderapi::FreeLibrary;
        unsafe { FreeLibrary(self._dwmapi_dll_handle); }
    }
}

// OpenGL functions from wglGetProcAddress OR loaded from opengl32.dll
struct GlFunctions {
    _opengl32_dll_handle: Option<HINSTANCE>,
    functions: Rc<GenericGlContext>, // implements Rc<dyn gleam::Gl>!
}

impl fmt::Debug for GlFunctions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self._opengl32_dll_handle.map(|f| f as usize).fmt(f)?;
        Ok(())
    }
}

impl GlFunctions {
    // Initializes the DLL, but does not load the functions yet
    fn initialize() -> Self {
        // zero-initialize all function pointers
        let context: GenericGlContext = unsafe { mem::zeroed() };

        let opengl32_dll = load_dll("opengl32.dll");

        Self {
            _opengl32_dll_handle: opengl32_dll,
            functions: Rc::new(context),
        }
    }

    // Assuming the OpenGL context is current, loads the OpenGL function pointers
    fn load(&mut self) {
        fn get_func(s: &str, opengl32_dll: Option<HINSTANCE>) -> *mut gl_context_loader::c_void {
            use winapi::um::{libloaderapi::GetProcAddress, wingdi::wglGetProcAddress};

            let mut func_name = encode_ascii(s);
            let addr1 = unsafe { wglGetProcAddress(func_name.as_mut_ptr()) };
            (if addr1 != ptr::null_mut() {
                addr1
            } else {
                if let Some(opengl32_dll) = opengl32_dll {
                    unsafe { GetProcAddress(opengl32_dll, func_name.as_mut_ptr()) }
                } else {
                    addr1
                }
            }) as *mut gl_context_loader::c_void
        }

        self.functions = Rc::new(GenericGlContext {
            glAccum: get_func("glAccum", self._opengl32_dll_handle),
            glActiveTexture: get_func("glActiveTexture", self._opengl32_dll_handle),
            glAlphaFunc: get_func("glAlphaFunc", self._opengl32_dll_handle),
            glAreTexturesResident: get_func("glAreTexturesResident", self._opengl32_dll_handle),
            glArrayElement: get_func("glArrayElement", self._opengl32_dll_handle),
            glAttachShader: get_func("glAttachShader", self._opengl32_dll_handle),
            glBegin: get_func("glBegin", self._opengl32_dll_handle),
            glBeginConditionalRender: get_func(
                "glBeginConditionalRender",
                self._opengl32_dll_handle,
            ),
            glBeginQuery: get_func("glBeginQuery", self._opengl32_dll_handle),
            glBeginTransformFeedback: get_func(
                "glBeginTransformFeedback",
                self._opengl32_dll_handle,
            ),
            glBindAttribLocation: get_func("glBindAttribLocation", self._opengl32_dll_handle),
            glBindBuffer: get_func("glBindBuffer", self._opengl32_dll_handle),
            glBindBufferBase: get_func("glBindBufferBase", self._opengl32_dll_handle),
            glBindBufferRange: get_func("glBindBufferRange", self._opengl32_dll_handle),
            glBindFragDataLocation: get_func("glBindFragDataLocation", self._opengl32_dll_handle),
            glBindFragDataLocationIndexed: get_func(
                "glBindFragDataLocationIndexed",
                self._opengl32_dll_handle,
            ),
            glBindFramebuffer: get_func("glBindFramebuffer", self._opengl32_dll_handle),
            glBindRenderbuffer: get_func("glBindRenderbuffer", self._opengl32_dll_handle),
            glBindSampler: get_func("glBindSampler", self._opengl32_dll_handle),
            glBindTexture: get_func("glBindTexture", self._opengl32_dll_handle),
            glBindVertexArray: get_func("glBindVertexArray", self._opengl32_dll_handle),
            glBindVertexArrayAPPLE: get_func("glBindVertexArrayAPPLE", self._opengl32_dll_handle),
            glBitmap: get_func("glBitmap", self._opengl32_dll_handle),
            glBlendBarrierKHR: get_func("glBlendBarrierKHR", self._opengl32_dll_handle),
            glBlendColor: get_func("glBlendColor", self._opengl32_dll_handle),
            glBlendEquation: get_func("glBlendEquation", self._opengl32_dll_handle),
            glBlendEquationSeparate: get_func("glBlendEquationSeparate", self._opengl32_dll_handle),
            glBlendFunc: get_func("glBlendFunc", self._opengl32_dll_handle),
            glBlendFuncSeparate: get_func("glBlendFuncSeparate", self._opengl32_dll_handle),
            glBlitFramebuffer: get_func("glBlitFramebuffer", self._opengl32_dll_handle),
            glBufferData: get_func("glBufferData", self._opengl32_dll_handle),
            glBufferStorage: get_func("glBufferStorage", self._opengl32_dll_handle),
            glBufferSubData: get_func("glBufferSubData", self._opengl32_dll_handle),
            glCallList: get_func("glCallList", self._opengl32_dll_handle),
            glCallLists: get_func("glCallLists", self._opengl32_dll_handle),
            glCheckFramebufferStatus: get_func(
                "glCheckFramebufferStatus",
                self._opengl32_dll_handle,
            ),
            glClampColor: get_func("glClampColor", self._opengl32_dll_handle),
            glClear: get_func("glClear", self._opengl32_dll_handle),
            glClearAccum: get_func("glClearAccum", self._opengl32_dll_handle),
            glClearBufferfi: get_func("glClearBufferfi", self._opengl32_dll_handle),
            glClearBufferfv: get_func("glClearBufferfv", self._opengl32_dll_handle),
            glClearBufferiv: get_func("glClearBufferiv", self._opengl32_dll_handle),
            glClearBufferuiv: get_func("glClearBufferuiv", self._opengl32_dll_handle),
            glClearColor: get_func("glClearColor", self._opengl32_dll_handle),
            glClearDepth: get_func("glClearDepth", self._opengl32_dll_handle),
            glClearIndex: get_func("glClearIndex", self._opengl32_dll_handle),
            glClearStencil: get_func("glClearStencil", self._opengl32_dll_handle),
            glClientActiveTexture: get_func("glClientActiveTexture", self._opengl32_dll_handle),
            glClientWaitSync: get_func("glClientWaitSync", self._opengl32_dll_handle),
            glClipPlane: get_func("glClipPlane", self._opengl32_dll_handle),
            glColor3b: get_func("glColor3b", self._opengl32_dll_handle),
            glColor3bv: get_func("glColor3bv", self._opengl32_dll_handle),
            glColor3d: get_func("glColor3d", self._opengl32_dll_handle),
            glColor3dv: get_func("glColor3dv", self._opengl32_dll_handle),
            glColor3f: get_func("glColor3f", self._opengl32_dll_handle),
            glColor3fv: get_func("glColor3fv", self._opengl32_dll_handle),
            glColor3i: get_func("glColor3i", self._opengl32_dll_handle),
            glColor3iv: get_func("glColor3iv", self._opengl32_dll_handle),
            glColor3s: get_func("glColor3s", self._opengl32_dll_handle),
            glColor3sv: get_func("glColor3sv", self._opengl32_dll_handle),
            glColor3ub: get_func("glColor3ub", self._opengl32_dll_handle),
            glColor3ubv: get_func("glColor3ubv", self._opengl32_dll_handle),
            glColor3ui: get_func("glColor3ui", self._opengl32_dll_handle),
            glColor3uiv: get_func("glColor3uiv", self._opengl32_dll_handle),
            glColor3us: get_func("glColor3us", self._opengl32_dll_handle),
            glColor3usv: get_func("glColor3usv", self._opengl32_dll_handle),
            glColor4b: get_func("glColor4b", self._opengl32_dll_handle),
            glColor4bv: get_func("glColor4bv", self._opengl32_dll_handle),
            glColor4d: get_func("glColor4d", self._opengl32_dll_handle),
            glColor4dv: get_func("glColor4dv", self._opengl32_dll_handle),
            glColor4f: get_func("glColor4f", self._opengl32_dll_handle),
            glColor4fv: get_func("glColor4fv", self._opengl32_dll_handle),
            glColor4i: get_func("glColor4i", self._opengl32_dll_handle),
            glColor4iv: get_func("glColor4iv", self._opengl32_dll_handle),
            glColor4s: get_func("glColor4s", self._opengl32_dll_handle),
            glColor4sv: get_func("glColor4sv", self._opengl32_dll_handle),
            glColor4ub: get_func("glColor4ub", self._opengl32_dll_handle),
            glColor4ubv: get_func("glColor4ubv", self._opengl32_dll_handle),
            glColor4ui: get_func("glColor4ui", self._opengl32_dll_handle),
            glColor4uiv: get_func("glColor4uiv", self._opengl32_dll_handle),
            glColor4us: get_func("glColor4us", self._opengl32_dll_handle),
            glColor4usv: get_func("glColor4usv", self._opengl32_dll_handle),
            glColorMask: get_func("glColorMask", self._opengl32_dll_handle),
            glColorMaski: get_func("glColorMaski", self._opengl32_dll_handle),
            glColorMaterial: get_func("glColorMaterial", self._opengl32_dll_handle),
            glColorP3ui: get_func("glColorP3ui", self._opengl32_dll_handle),
            glColorP3uiv: get_func("glColorP3uiv", self._opengl32_dll_handle),
            glColorP4ui: get_func("glColorP4ui", self._opengl32_dll_handle),
            glColorP4uiv: get_func("glColorP4uiv", self._opengl32_dll_handle),
            glColorPointer: get_func("glColorPointer", self._opengl32_dll_handle),
            glCompileShader: get_func("glCompileShader", self._opengl32_dll_handle),
            glCompressedTexImage1D: get_func("glCompressedTexImage1D", self._opengl32_dll_handle),
            glCompressedTexImage2D: get_func("glCompressedTexImage2D", self._opengl32_dll_handle),
            glCompressedTexImage3D: get_func("glCompressedTexImage3D", self._opengl32_dll_handle),
            glCompressedTexSubImage1D: get_func(
                "glCompressedTexSubImage1D",
                self._opengl32_dll_handle,
            ),
            glCompressedTexSubImage2D: get_func(
                "glCompressedTexSubImage2D",
                self._opengl32_dll_handle,
            ),
            glCompressedTexSubImage3D: get_func(
                "glCompressedTexSubImage3D",
                self._opengl32_dll_handle,
            ),
            glCopyBufferSubData: get_func("glCopyBufferSubData", self._opengl32_dll_handle),
            glCopyImageSubData: get_func("glCopyImageSubData", self._opengl32_dll_handle),
            glCopyPixels: get_func("glCopyPixels", self._opengl32_dll_handle),
            glCopyTexImage1D: get_func("glCopyTexImage1D", self._opengl32_dll_handle),
            glCopyTexImage2D: get_func("glCopyTexImage2D", self._opengl32_dll_handle),
            glCopyTexSubImage1D: get_func("glCopyTexSubImage1D", self._opengl32_dll_handle),
            glCopyTexSubImage2D: get_func("glCopyTexSubImage2D", self._opengl32_dll_handle),
            glCopyTexSubImage3D: get_func("glCopyTexSubImage3D", self._opengl32_dll_handle),
            glCreateProgram: get_func("glCreateProgram", self._opengl32_dll_handle),
            glCreateShader: get_func("glCreateShader", self._opengl32_dll_handle),
            glCullFace: get_func("glCullFace", self._opengl32_dll_handle),
            glDebugMessageCallback: get_func("glDebugMessageCallback", self._opengl32_dll_handle),
            glDebugMessageCallbackKHR: get_func(
                "glDebugMessageCallbackKHR",
                self._opengl32_dll_handle,
            ),
            glDebugMessageControl: get_func("glDebugMessageControl", self._opengl32_dll_handle),
            glDebugMessageControlKHR: get_func(
                "glDebugMessageControlKHR",
                self._opengl32_dll_handle,
            ),
            glDebugMessageInsert: get_func("glDebugMessageInsert", self._opengl32_dll_handle),
            glDebugMessageInsertKHR: get_func("glDebugMessageInsertKHR", self._opengl32_dll_handle),
            glDeleteBuffers: get_func("glDeleteBuffers", self._opengl32_dll_handle),
            glDeleteFencesAPPLE: get_func("glDeleteFencesAPPLE", self._opengl32_dll_handle),
            glDeleteFramebuffers: get_func("glDeleteFramebuffers", self._opengl32_dll_handle),
            glDeleteLists: get_func("glDeleteLists", self._opengl32_dll_handle),
            glDeleteProgram: get_func("glDeleteProgram", self._opengl32_dll_handle),
            glDeleteQueries: get_func("glDeleteQueries", self._opengl32_dll_handle),
            glDeleteRenderbuffers: get_func("glDeleteRenderbuffers", self._opengl32_dll_handle),
            glDeleteSamplers: get_func("glDeleteSamplers", self._opengl32_dll_handle),
            glDeleteShader: get_func("glDeleteShader", self._opengl32_dll_handle),
            glDeleteSync: get_func("glDeleteSync", self._opengl32_dll_handle),
            glDeleteTextures: get_func("glDeleteTextures", self._opengl32_dll_handle),
            glDeleteVertexArrays: get_func("glDeleteVertexArrays", self._opengl32_dll_handle),
            glDeleteVertexArraysAPPLE: get_func(
                "glDeleteVertexArraysAPPLE",
                self._opengl32_dll_handle,
            ),
            glDepthFunc: get_func("glDepthFunc", self._opengl32_dll_handle),
            glDepthMask: get_func("glDepthMask", self._opengl32_dll_handle),
            glDepthRange: get_func("glDepthRange", self._opengl32_dll_handle),
            glDetachShader: get_func("glDetachShader", self._opengl32_dll_handle),
            glDisable: get_func("glDisable", self._opengl32_dll_handle),
            glDisableClientState: get_func("glDisableClientState", self._opengl32_dll_handle),
            glDisableVertexAttribArray: get_func(
                "glDisableVertexAttribArray",
                self._opengl32_dll_handle,
            ),
            glDisablei: get_func("glDisablei", self._opengl32_dll_handle),
            glDrawArrays: get_func("glDrawArrays", self._opengl32_dll_handle),
            glDrawArraysInstanced: get_func("glDrawArraysInstanced", self._opengl32_dll_handle),
            glDrawBuffer: get_func("glDrawBuffer", self._opengl32_dll_handle),
            glDrawBuffers: get_func("glDrawBuffers", self._opengl32_dll_handle),
            glDrawElements: get_func("glDrawElements", self._opengl32_dll_handle),
            glDrawElementsBaseVertex: get_func(
                "glDrawElementsBaseVertex",
                self._opengl32_dll_handle,
            ),
            glDrawElementsInstanced: get_func("glDrawElementsInstanced", self._opengl32_dll_handle),
            glDrawElementsInstancedBaseVertex: get_func(
                "glDrawElementsInstancedBaseVertex",
                self._opengl32_dll_handle,
            ),
            glDrawPixels: get_func("glDrawPixels", self._opengl32_dll_handle),
            glDrawRangeElements: get_func("glDrawRangeElements", self._opengl32_dll_handle),
            glDrawRangeElementsBaseVertex: get_func(
                "glDrawRangeElementsBaseVertex",
                self._opengl32_dll_handle,
            ),
            glEdgeFlag: get_func("glEdgeFlag", self._opengl32_dll_handle),
            glEdgeFlagPointer: get_func("glEdgeFlagPointer", self._opengl32_dll_handle),
            glEdgeFlagv: get_func("glEdgeFlagv", self._opengl32_dll_handle),
            glEnable: get_func("glEnable", self._opengl32_dll_handle),
            glEnableClientState: get_func("glEnableClientState", self._opengl32_dll_handle),
            glEnableVertexAttribArray: get_func(
                "glEnableVertexAttribArray",
                self._opengl32_dll_handle,
            ),
            glEnablei: get_func("glEnablei", self._opengl32_dll_handle),
            glEnd: get_func("glEnd", self._opengl32_dll_handle),
            glEndConditionalRender: get_func("glEndConditionalRender", self._opengl32_dll_handle),
            glEndList: get_func("glEndList", self._opengl32_dll_handle),
            glEndQuery: get_func("glEndQuery", self._opengl32_dll_handle),
            glEndTransformFeedback: get_func("glEndTransformFeedback", self._opengl32_dll_handle),
            glEvalCoord1d: get_func("glEvalCoord1d", self._opengl32_dll_handle),
            glEvalCoord1dv: get_func("glEvalCoord1dv", self._opengl32_dll_handle),
            glEvalCoord1f: get_func("glEvalCoord1f", self._opengl32_dll_handle),
            glEvalCoord1fv: get_func("glEvalCoord1fv", self._opengl32_dll_handle),
            glEvalCoord2d: get_func("glEvalCoord2d", self._opengl32_dll_handle),
            glEvalCoord2dv: get_func("glEvalCoord2dv", self._opengl32_dll_handle),
            glEvalCoord2f: get_func("glEvalCoord2f", self._opengl32_dll_handle),
            glEvalCoord2fv: get_func("glEvalCoord2fv", self._opengl32_dll_handle),
            glEvalMesh1: get_func("glEvalMesh1", self._opengl32_dll_handle),
            glEvalMesh2: get_func("glEvalMesh2", self._opengl32_dll_handle),
            glEvalPoint1: get_func("glEvalPoint1", self._opengl32_dll_handle),
            glEvalPoint2: get_func("glEvalPoint2", self._opengl32_dll_handle),
            glFeedbackBuffer: get_func("glFeedbackBuffer", self._opengl32_dll_handle),
            glFenceSync: get_func("glFenceSync", self._opengl32_dll_handle),
            glFinish: get_func("glFinish", self._opengl32_dll_handle),
            glFinishFenceAPPLE: get_func("glFinishFenceAPPLE", self._opengl32_dll_handle),
            glFinishObjectAPPLE: get_func("glFinishObjectAPPLE", self._opengl32_dll_handle),
            glFlush: get_func("glFlush", self._opengl32_dll_handle),
            glFlushMappedBufferRange: get_func(
                "glFlushMappedBufferRange",
                self._opengl32_dll_handle,
            ),
            glFogCoordPointer: get_func("glFogCoordPointer", self._opengl32_dll_handle),
            glFogCoordd: get_func("glFogCoordd", self._opengl32_dll_handle),
            glFogCoorddv: get_func("glFogCoorddv", self._opengl32_dll_handle),
            glFogCoordf: get_func("glFogCoordf", self._opengl32_dll_handle),
            glFogCoordfv: get_func("glFogCoordfv", self._opengl32_dll_handle),
            glFogf: get_func("glFogf", self._opengl32_dll_handle),
            glFogfv: get_func("glFogfv", self._opengl32_dll_handle),
            glFogi: get_func("glFogi", self._opengl32_dll_handle),
            glFogiv: get_func("glFogiv", self._opengl32_dll_handle),
            glFramebufferRenderbuffer: get_func(
                "glFramebufferRenderbuffer",
                self._opengl32_dll_handle,
            ),
            glFramebufferTexture: get_func("glFramebufferTexture", self._opengl32_dll_handle),
            glFramebufferTexture1D: get_func("glFramebufferTexture1D", self._opengl32_dll_handle),
            glFramebufferTexture2D: get_func("glFramebufferTexture2D", self._opengl32_dll_handle),
            glFramebufferTexture3D: get_func("glFramebufferTexture3D", self._opengl32_dll_handle),
            glFramebufferTextureLayer: get_func(
                "glFramebufferTextureLayer",
                self._opengl32_dll_handle,
            ),
            glFrontFace: get_func("glFrontFace", self._opengl32_dll_handle),
            glFrustum: get_func("glFrustum", self._opengl32_dll_handle),
            glGenBuffers: get_func("glGenBuffers", self._opengl32_dll_handle),
            glGenFencesAPPLE: get_func("glGenFencesAPPLE", self._opengl32_dll_handle),
            glGenFramebuffers: get_func("glGenFramebuffers", self._opengl32_dll_handle),
            glGenLists: get_func("glGenLists", self._opengl32_dll_handle),
            glGenQueries: get_func("glGenQueries", self._opengl32_dll_handle),
            glGenRenderbuffers: get_func("glGenRenderbuffers", self._opengl32_dll_handle),
            glGenSamplers: get_func("glGenSamplers", self._opengl32_dll_handle),
            glGenTextures: get_func("glGenTextures", self._opengl32_dll_handle),
            glGenVertexArrays: get_func("glGenVertexArrays", self._opengl32_dll_handle),
            glGenVertexArraysAPPLE: get_func("glGenVertexArraysAPPLE", self._opengl32_dll_handle),
            glGenerateMipmap: get_func("glGenerateMipmap", self._opengl32_dll_handle),
            glGetActiveAttrib: get_func("glGetActiveAttrib", self._opengl32_dll_handle),
            glGetActiveUniform: get_func("glGetActiveUniform", self._opengl32_dll_handle),
            glGetActiveUniformBlockName: get_func(
                "glGetActiveUniformBlockName",
                self._opengl32_dll_handle,
            ),
            glGetActiveUniformBlockiv: get_func(
                "glGetActiveUniformBlockiv",
                self._opengl32_dll_handle,
            ),
            glGetActiveUniformName: get_func("glGetActiveUniformName", self._opengl32_dll_handle),
            glGetActiveUniformsiv: get_func("glGetActiveUniformsiv", self._opengl32_dll_handle),
            glGetAttachedShaders: get_func("glGetAttachedShaders", self._opengl32_dll_handle),
            glGetAttribLocation: get_func("glGetAttribLocation", self._opengl32_dll_handle),
            glGetBooleani_v: get_func("glGetBooleani_v", self._opengl32_dll_handle),
            glGetBooleanv: get_func("glGetBooleanv", self._opengl32_dll_handle),
            glGetBufferParameteri64v: get_func(
                "glGetBufferParameteri64v",
                self._opengl32_dll_handle,
            ),
            glGetBufferParameteriv: get_func("glGetBufferParameteriv", self._opengl32_dll_handle),
            glGetBufferPointerv: get_func("glGetBufferPointerv", self._opengl32_dll_handle),
            glGetBufferSubData: get_func("glGetBufferSubData", self._opengl32_dll_handle),
            glGetClipPlane: get_func("glGetClipPlane", self._opengl32_dll_handle),
            glGetCompressedTexImage: get_func("glGetCompressedTexImage", self._opengl32_dll_handle),
            glGetDebugMessageLog: get_func("glGetDebugMessageLog", self._opengl32_dll_handle),
            glGetDebugMessageLogKHR: get_func("glGetDebugMessageLogKHR", self._opengl32_dll_handle),
            glGetDoublev: get_func("glGetDoublev", self._opengl32_dll_handle),
            glGetError: get_func("glGetError", self._opengl32_dll_handle),
            glGetFloatv: get_func("glGetFloatv", self._opengl32_dll_handle),
            glGetFragDataIndex: get_func("glGetFragDataIndex", self._opengl32_dll_handle),
            glGetFragDataLocation: get_func("glGetFragDataLocation", self._opengl32_dll_handle),
            glGetFramebufferAttachmentParameteriv: get_func(
                "glGetFramebufferAttachmentParameteriv",
                self._opengl32_dll_handle,
            ),
            glGetInteger64i_v: get_func("glGetInteger64i_v", self._opengl32_dll_handle),
            glGetInteger64v: get_func("glGetInteger64v", self._opengl32_dll_handle),
            glGetIntegeri_v: get_func("glGetIntegeri_v", self._opengl32_dll_handle),
            glGetIntegerv: get_func("glGetIntegerv", self._opengl32_dll_handle),
            glGetLightfv: get_func("glGetLightfv", self._opengl32_dll_handle),
            glGetLightiv: get_func("glGetLightiv", self._opengl32_dll_handle),
            glGetMapdv: get_func("glGetMapdv", self._opengl32_dll_handle),
            glGetMapfv: get_func("glGetMapfv", self._opengl32_dll_handle),
            glGetMapiv: get_func("glGetMapiv", self._opengl32_dll_handle),
            glGetMaterialfv: get_func("glGetMaterialfv", self._opengl32_dll_handle),
            glGetMaterialiv: get_func("glGetMaterialiv", self._opengl32_dll_handle),
            glGetMultisamplefv: get_func("glGetMultisamplefv", self._opengl32_dll_handle),
            glGetObjectLabel: get_func("glGetObjectLabel", self._opengl32_dll_handle),
            glGetObjectLabelKHR: get_func("glGetObjectLabelKHR", self._opengl32_dll_handle),
            glGetObjectPtrLabel: get_func("glGetObjectPtrLabel", self._opengl32_dll_handle),
            glGetObjectPtrLabelKHR: get_func("glGetObjectPtrLabelKHR", self._opengl32_dll_handle),
            glGetPixelMapfv: get_func("glGetPixelMapfv", self._opengl32_dll_handle),
            glGetPixelMapuiv: get_func("glGetPixelMapuiv", self._opengl32_dll_handle),
            glGetPixelMapusv: get_func("glGetPixelMapusv", self._opengl32_dll_handle),
            glGetPointerv: get_func("glGetPointerv", self._opengl32_dll_handle),
            glGetPointervKHR: get_func("glGetPointervKHR", self._opengl32_dll_handle),
            glGetPolygonStipple: get_func("glGetPolygonStipple", self._opengl32_dll_handle),
            glGetProgramBinary: get_func("glGetProgramBinary", self._opengl32_dll_handle),
            glGetProgramInfoLog: get_func("glGetProgramInfoLog", self._opengl32_dll_handle),
            glGetProgramiv: get_func("glGetProgramiv", self._opengl32_dll_handle),
            glGetQueryObjecti64v: get_func("glGetQueryObjecti64v", self._opengl32_dll_handle),
            glGetQueryObjectiv: get_func("glGetQueryObjectiv", self._opengl32_dll_handle),
            glGetQueryObjectui64v: get_func("glGetQueryObjectui64v", self._opengl32_dll_handle),
            glGetQueryObjectuiv: get_func("glGetQueryObjectuiv", self._opengl32_dll_handle),
            glGetQueryiv: get_func("glGetQueryiv", self._opengl32_dll_handle),
            glGetRenderbufferParameteriv: get_func(
                "glGetRenderbufferParameteriv",
                self._opengl32_dll_handle,
            ),
            glGetSamplerParameterIiv: get_func(
                "glGetSamplerParameterIiv",
                self._opengl32_dll_handle,
            ),
            glGetSamplerParameterIuiv: get_func(
                "glGetSamplerParameterIuiv",
                self._opengl32_dll_handle,
            ),
            glGetSamplerParameterfv: get_func("glGetSamplerParameterfv", self._opengl32_dll_handle),
            glGetSamplerParameteriv: get_func("glGetSamplerParameteriv", self._opengl32_dll_handle),
            glGetShaderInfoLog: get_func("glGetShaderInfoLog", self._opengl32_dll_handle),
            glGetShaderSource: get_func("glGetShaderSource", self._opengl32_dll_handle),
            glGetShaderiv: get_func("glGetShaderiv", self._opengl32_dll_handle),
            glGetString: get_func("glGetString", self._opengl32_dll_handle),
            glGetStringi: get_func("glGetStringi", self._opengl32_dll_handle),
            glGetSynciv: get_func("glGetSynciv", self._opengl32_dll_handle),
            glGetTexEnvfv: get_func("glGetTexEnvfv", self._opengl32_dll_handle),
            glGetTexEnviv: get_func("glGetTexEnviv", self._opengl32_dll_handle),
            glGetTexGendv: get_func("glGetTexGendv", self._opengl32_dll_handle),
            glGetTexGenfv: get_func("glGetTexGenfv", self._opengl32_dll_handle),
            glGetTexGeniv: get_func("glGetTexGeniv", self._opengl32_dll_handle),
            glGetTexImage: get_func("glGetTexImage", self._opengl32_dll_handle),
            glGetTexLevelParameterfv: get_func(
                "glGetTexLevelParameterfv",
                self._opengl32_dll_handle,
            ),
            glGetTexLevelParameteriv: get_func(
                "glGetTexLevelParameteriv",
                self._opengl32_dll_handle,
            ),
            glGetTexParameterIiv: get_func("glGetTexParameterIiv", self._opengl32_dll_handle),
            glGetTexParameterIuiv: get_func("glGetTexParameterIuiv", self._opengl32_dll_handle),
            glGetTexParameterPointervAPPLE: get_func(
                "glGetTexParameterPointervAPPLE",
                self._opengl32_dll_handle,
            ),
            glGetTexParameterfv: get_func("glGetTexParameterfv", self._opengl32_dll_handle),
            glGetTexParameteriv: get_func("glGetTexParameteriv", self._opengl32_dll_handle),
            glGetTransformFeedbackVarying: get_func(
                "glGetTransformFeedbackVarying",
                self._opengl32_dll_handle,
            ),
            glGetUniformBlockIndex: get_func("glGetUniformBlockIndex", self._opengl32_dll_handle),
            glGetUniformIndices: get_func("glGetUniformIndices", self._opengl32_dll_handle),
            glGetUniformLocation: get_func("glGetUniformLocation", self._opengl32_dll_handle),
            glGetUniformfv: get_func("glGetUniformfv", self._opengl32_dll_handle),
            glGetUniformiv: get_func("glGetUniformiv", self._opengl32_dll_handle),
            glGetUniformuiv: get_func("glGetUniformuiv", self._opengl32_dll_handle),
            glGetVertexAttribIiv: get_func("glGetVertexAttribIiv", self._opengl32_dll_handle),
            glGetVertexAttribIuiv: get_func("glGetVertexAttribIuiv", self._opengl32_dll_handle),
            glGetVertexAttribPointerv: get_func(
                "glGetVertexAttribPointerv",
                self._opengl32_dll_handle,
            ),
            glGetVertexAttribdv: get_func("glGetVertexAttribdv", self._opengl32_dll_handle),
            glGetVertexAttribfv: get_func("glGetVertexAttribfv", self._opengl32_dll_handle),
            glGetVertexAttribiv: get_func("glGetVertexAttribiv", self._opengl32_dll_handle),
            glHint: get_func("glHint", self._opengl32_dll_handle),
            glIndexMask: get_func("glIndexMask", self._opengl32_dll_handle),
            glIndexPointer: get_func("glIndexPointer", self._opengl32_dll_handle),
            glIndexd: get_func("glIndexd", self._opengl32_dll_handle),
            glIndexdv: get_func("glIndexdv", self._opengl32_dll_handle),
            glIndexf: get_func("glIndexf", self._opengl32_dll_handle),
            glIndexfv: get_func("glIndexfv", self._opengl32_dll_handle),
            glIndexi: get_func("glIndexi", self._opengl32_dll_handle),
            glIndexiv: get_func("glIndexiv", self._opengl32_dll_handle),
            glIndexs: get_func("glIndexs", self._opengl32_dll_handle),
            glIndexsv: get_func("glIndexsv", self._opengl32_dll_handle),
            glIndexub: get_func("glIndexub", self._opengl32_dll_handle),
            glIndexubv: get_func("glIndexubv", self._opengl32_dll_handle),
            glInitNames: get_func("glInitNames", self._opengl32_dll_handle),
            glInsertEventMarkerEXT: get_func("glInsertEventMarkerEXT", self._opengl32_dll_handle),
            glInterleavedArrays: get_func("glInterleavedArrays", self._opengl32_dll_handle),
            glInvalidateBufferData: get_func("glInvalidateBufferData", self._opengl32_dll_handle),
            glInvalidateBufferSubData: get_func(
                "glInvalidateBufferSubData",
                self._opengl32_dll_handle,
            ),
            glInvalidateFramebuffer: get_func("glInvalidateFramebuffer", self._opengl32_dll_handle),
            glInvalidateSubFramebuffer: get_func(
                "glInvalidateSubFramebuffer",
                self._opengl32_dll_handle,
            ),
            glInvalidateTexImage: get_func("glInvalidateTexImage", self._opengl32_dll_handle),
            glInvalidateTexSubImage: get_func("glInvalidateTexSubImage", self._opengl32_dll_handle),
            glIsBuffer: get_func("glIsBuffer", self._opengl32_dll_handle),
            glIsEnabled: get_func("glIsEnabled", self._opengl32_dll_handle),
            glIsEnabledi: get_func("glIsEnabledi", self._opengl32_dll_handle),
            glIsFenceAPPLE: get_func("glIsFenceAPPLE", self._opengl32_dll_handle),
            glIsFramebuffer: get_func("glIsFramebuffer", self._opengl32_dll_handle),
            glIsList: get_func("glIsList", self._opengl32_dll_handle),
            glIsProgram: get_func("glIsProgram", self._opengl32_dll_handle),
            glIsQuery: get_func("glIsQuery", self._opengl32_dll_handle),
            glIsRenderbuffer: get_func("glIsRenderbuffer", self._opengl32_dll_handle),
            glIsSampler: get_func("glIsSampler", self._opengl32_dll_handle),
            glIsShader: get_func("glIsShader", self._opengl32_dll_handle),
            glIsSync: get_func("glIsSync", self._opengl32_dll_handle),
            glIsTexture: get_func("glIsTexture", self._opengl32_dll_handle),
            glIsVertexArray: get_func("glIsVertexArray", self._opengl32_dll_handle),
            glIsVertexArrayAPPLE: get_func("glIsVertexArrayAPPLE", self._opengl32_dll_handle),
            glLightModelf: get_func("glLightModelf", self._opengl32_dll_handle),
            glLightModelfv: get_func("glLightModelfv", self._opengl32_dll_handle),
            glLightModeli: get_func("glLightModeli", self._opengl32_dll_handle),
            glLightModeliv: get_func("glLightModeliv", self._opengl32_dll_handle),
            glLightf: get_func("glLightf", self._opengl32_dll_handle),
            glLightfv: get_func("glLightfv", self._opengl32_dll_handle),
            glLighti: get_func("glLighti", self._opengl32_dll_handle),
            glLightiv: get_func("glLightiv", self._opengl32_dll_handle),
            glLineStipple: get_func("glLineStipple", self._opengl32_dll_handle),
            glLineWidth: get_func("glLineWidth", self._opengl32_dll_handle),
            glLinkProgram: get_func("glLinkProgram", self._opengl32_dll_handle),
            glListBase: get_func("glListBase", self._opengl32_dll_handle),
            glLoadIdentity: get_func("glLoadIdentity", self._opengl32_dll_handle),
            glLoadMatrixd: get_func("glLoadMatrixd", self._opengl32_dll_handle),
            glLoadMatrixf: get_func("glLoadMatrixf", self._opengl32_dll_handle),
            glLoadName: get_func("glLoadName", self._opengl32_dll_handle),
            glLoadTransposeMatrixd: get_func("glLoadTransposeMatrixd", self._opengl32_dll_handle),
            glLoadTransposeMatrixf: get_func("glLoadTransposeMatrixf", self._opengl32_dll_handle),
            glLogicOp: get_func("glLogicOp", self._opengl32_dll_handle),
            glMap1d: get_func("glMap1d", self._opengl32_dll_handle),
            glMap1f: get_func("glMap1f", self._opengl32_dll_handle),
            glMap2d: get_func("glMap2d", self._opengl32_dll_handle),
            glMap2f: get_func("glMap2f", self._opengl32_dll_handle),
            glMapBuffer: get_func("glMapBuffer", self._opengl32_dll_handle),
            glMapBufferRange: get_func("glMapBufferRange", self._opengl32_dll_handle),
            glMapGrid1d: get_func("glMapGrid1d", self._opengl32_dll_handle),
            glMapGrid1f: get_func("glMapGrid1f", self._opengl32_dll_handle),
            glMapGrid2d: get_func("glMapGrid2d", self._opengl32_dll_handle),
            glMapGrid2f: get_func("glMapGrid2f", self._opengl32_dll_handle),
            glMaterialf: get_func("glMaterialf", self._opengl32_dll_handle),
            glMaterialfv: get_func("glMaterialfv", self._opengl32_dll_handle),
            glMateriali: get_func("glMateriali", self._opengl32_dll_handle),
            glMaterialiv: get_func("glMaterialiv", self._opengl32_dll_handle),
            glMatrixMode: get_func("glMatrixMode", self._opengl32_dll_handle),
            glMultMatrixd: get_func("glMultMatrixd", self._opengl32_dll_handle),
            glMultMatrixf: get_func("glMultMatrixf", self._opengl32_dll_handle),
            glMultTransposeMatrixd: get_func("glMultTransposeMatrixd", self._opengl32_dll_handle),
            glMultTransposeMatrixf: get_func("glMultTransposeMatrixf", self._opengl32_dll_handle),
            glMultiDrawArrays: get_func("glMultiDrawArrays", self._opengl32_dll_handle),
            glMultiDrawElements: get_func("glMultiDrawElements", self._opengl32_dll_handle),
            glMultiDrawElementsBaseVertex: get_func(
                "glMultiDrawElementsBaseVertex",
                self._opengl32_dll_handle,
            ),
            glMultiTexCoord1d: get_func("glMultiTexCoord1d", self._opengl32_dll_handle),
            glMultiTexCoord1dv: get_func("glMultiTexCoord1dv", self._opengl32_dll_handle),
            glMultiTexCoord1f: get_func("glMultiTexCoord1f", self._opengl32_dll_handle),
            glMultiTexCoord1fv: get_func("glMultiTexCoord1fv", self._opengl32_dll_handle),
            glMultiTexCoord1i: get_func("glMultiTexCoord1i", self._opengl32_dll_handle),
            glMultiTexCoord1iv: get_func("glMultiTexCoord1iv", self._opengl32_dll_handle),
            glMultiTexCoord1s: get_func("glMultiTexCoord1s", self._opengl32_dll_handle),
            glMultiTexCoord1sv: get_func("glMultiTexCoord1sv", self._opengl32_dll_handle),
            glMultiTexCoord2d: get_func("glMultiTexCoord2d", self._opengl32_dll_handle),
            glMultiTexCoord2dv: get_func("glMultiTexCoord2dv", self._opengl32_dll_handle),
            glMultiTexCoord2f: get_func("glMultiTexCoord2f", self._opengl32_dll_handle),
            glMultiTexCoord2fv: get_func("glMultiTexCoord2fv", self._opengl32_dll_handle),
            glMultiTexCoord2i: get_func("glMultiTexCoord2i", self._opengl32_dll_handle),
            glMultiTexCoord2iv: get_func("glMultiTexCoord2iv", self._opengl32_dll_handle),
            glMultiTexCoord2s: get_func("glMultiTexCoord2s", self._opengl32_dll_handle),
            glMultiTexCoord2sv: get_func("glMultiTexCoord2sv", self._opengl32_dll_handle),
            glMultiTexCoord3d: get_func("glMultiTexCoord3d", self._opengl32_dll_handle),
            glMultiTexCoord3dv: get_func("glMultiTexCoord3dv", self._opengl32_dll_handle),
            glMultiTexCoord3f: get_func("glMultiTexCoord3f", self._opengl32_dll_handle),
            glMultiTexCoord3fv: get_func("glMultiTexCoord3fv", self._opengl32_dll_handle),
            glMultiTexCoord3i: get_func("glMultiTexCoord3i", self._opengl32_dll_handle),
            glMultiTexCoord3iv: get_func("glMultiTexCoord3iv", self._opengl32_dll_handle),
            glMultiTexCoord3s: get_func("glMultiTexCoord3s", self._opengl32_dll_handle),
            glMultiTexCoord3sv: get_func("glMultiTexCoord3sv", self._opengl32_dll_handle),
            glMultiTexCoord4d: get_func("glMultiTexCoord4d", self._opengl32_dll_handle),
            glMultiTexCoord4dv: get_func("glMultiTexCoord4dv", self._opengl32_dll_handle),
            glMultiTexCoord4f: get_func("glMultiTexCoord4f", self._opengl32_dll_handle),
            glMultiTexCoord4fv: get_func("glMultiTexCoord4fv", self._opengl32_dll_handle),
            glMultiTexCoord4i: get_func("glMultiTexCoord4i", self._opengl32_dll_handle),
            glMultiTexCoord4iv: get_func("glMultiTexCoord4iv", self._opengl32_dll_handle),
            glMultiTexCoord4s: get_func("glMultiTexCoord4s", self._opengl32_dll_handle),
            glMultiTexCoord4sv: get_func("glMultiTexCoord4sv", self._opengl32_dll_handle),
            glMultiTexCoordP1ui: get_func("glMultiTexCoordP1ui", self._opengl32_dll_handle),
            glMultiTexCoordP1uiv: get_func("glMultiTexCoordP1uiv", self._opengl32_dll_handle),
            glMultiTexCoordP2ui: get_func("glMultiTexCoordP2ui", self._opengl32_dll_handle),
            glMultiTexCoordP2uiv: get_func("glMultiTexCoordP2uiv", self._opengl32_dll_handle),
            glMultiTexCoordP3ui: get_func("glMultiTexCoordP3ui", self._opengl32_dll_handle),
            glMultiTexCoordP3uiv: get_func("glMultiTexCoordP3uiv", self._opengl32_dll_handle),
            glMultiTexCoordP4ui: get_func("glMultiTexCoordP4ui", self._opengl32_dll_handle),
            glMultiTexCoordP4uiv: get_func("glMultiTexCoordP4uiv", self._opengl32_dll_handle),
            glNewList: get_func("glNewList", self._opengl32_dll_handle),
            glNormal3b: get_func("glNormal3b", self._opengl32_dll_handle),
            glNormal3bv: get_func("glNormal3bv", self._opengl32_dll_handle),
            glNormal3d: get_func("glNormal3d", self._opengl32_dll_handle),
            glNormal3dv: get_func("glNormal3dv", self._opengl32_dll_handle),
            glNormal3f: get_func("glNormal3f", self._opengl32_dll_handle),
            glNormal3fv: get_func("glNormal3fv", self._opengl32_dll_handle),
            glNormal3i: get_func("glNormal3i", self._opengl32_dll_handle),
            glNormal3iv: get_func("glNormal3iv", self._opengl32_dll_handle),
            glNormal3s: get_func("glNormal3s", self._opengl32_dll_handle),
            glNormal3sv: get_func("glNormal3sv", self._opengl32_dll_handle),
            glNormalP3ui: get_func("glNormalP3ui", self._opengl32_dll_handle),
            glNormalP3uiv: get_func("glNormalP3uiv", self._opengl32_dll_handle),
            glNormalPointer: get_func("glNormalPointer", self._opengl32_dll_handle),
            glObjectLabel: get_func("glObjectLabel", self._opengl32_dll_handle),
            glObjectLabelKHR: get_func("glObjectLabelKHR", self._opengl32_dll_handle),
            glObjectPtrLabel: get_func("glObjectPtrLabel", self._opengl32_dll_handle),
            glObjectPtrLabelKHR: get_func("glObjectPtrLabelKHR", self._opengl32_dll_handle),
            glOrtho: get_func("glOrtho", self._opengl32_dll_handle),
            glPassThrough: get_func("glPassThrough", self._opengl32_dll_handle),
            glPixelMapfv: get_func("glPixelMapfv", self._opengl32_dll_handle),
            glPixelMapuiv: get_func("glPixelMapuiv", self._opengl32_dll_handle),
            glPixelMapusv: get_func("glPixelMapusv", self._opengl32_dll_handle),
            glPixelStoref: get_func("glPixelStoref", self._opengl32_dll_handle),
            glPixelStorei: get_func("glPixelStorei", self._opengl32_dll_handle),
            glPixelTransferf: get_func("glPixelTransferf", self._opengl32_dll_handle),
            glPixelTransferi: get_func("glPixelTransferi", self._opengl32_dll_handle),
            glPixelZoom: get_func("glPixelZoom", self._opengl32_dll_handle),
            glPointParameterf: get_func("glPointParameterf", self._opengl32_dll_handle),
            glPointParameterfv: get_func("glPointParameterfv", self._opengl32_dll_handle),
            glPointParameteri: get_func("glPointParameteri", self._opengl32_dll_handle),
            glPointParameteriv: get_func("glPointParameteriv", self._opengl32_dll_handle),
            glPointSize: get_func("glPointSize", self._opengl32_dll_handle),
            glPolygonMode: get_func("glPolygonMode", self._opengl32_dll_handle),
            glPolygonOffset: get_func("glPolygonOffset", self._opengl32_dll_handle),
            glPolygonStipple: get_func("glPolygonStipple", self._opengl32_dll_handle),
            glPopAttrib: get_func("glPopAttrib", self._opengl32_dll_handle),
            glPopClientAttrib: get_func("glPopClientAttrib", self._opengl32_dll_handle),
            glPopDebugGroup: get_func("glPopDebugGroup", self._opengl32_dll_handle),
            glPopDebugGroupKHR: get_func("glPopDebugGroupKHR", self._opengl32_dll_handle),
            glPopGroupMarkerEXT: get_func("glPopGroupMarkerEXT", self._opengl32_dll_handle),
            glPopMatrix: get_func("glPopMatrix", self._opengl32_dll_handle),
            glPopName: get_func("glPopName", self._opengl32_dll_handle),
            glPrimitiveRestartIndex: get_func("glPrimitiveRestartIndex", self._opengl32_dll_handle),
            glPrioritizeTextures: get_func("glPrioritizeTextures", self._opengl32_dll_handle),
            glProgramBinary: get_func("glProgramBinary", self._opengl32_dll_handle),
            glProgramParameteri: get_func("glProgramParameteri", self._opengl32_dll_handle),
            glProvokingVertex: get_func("glProvokingVertex", self._opengl32_dll_handle),
            glPushAttrib: get_func("glPushAttrib", self._opengl32_dll_handle),
            glPushClientAttrib: get_func("glPushClientAttrib", self._opengl32_dll_handle),
            glPushDebugGroup: get_func("glPushDebugGroup", self._opengl32_dll_handle),
            glPushDebugGroupKHR: get_func("glPushDebugGroupKHR", self._opengl32_dll_handle),
            glPushGroupMarkerEXT: get_func("glPushGroupMarkerEXT", self._opengl32_dll_handle),
            glPushMatrix: get_func("glPushMatrix", self._opengl32_dll_handle),
            glPushName: get_func("glPushName", self._opengl32_dll_handle),
            glQueryCounter: get_func("glQueryCounter", self._opengl32_dll_handle),
            glRasterPos2d: get_func("glRasterPos2d", self._opengl32_dll_handle),
            glRasterPos2dv: get_func("glRasterPos2dv", self._opengl32_dll_handle),
            glRasterPos2f: get_func("glRasterPos2f", self._opengl32_dll_handle),
            glRasterPos2fv: get_func("glRasterPos2fv", self._opengl32_dll_handle),
            glRasterPos2i: get_func("glRasterPos2i", self._opengl32_dll_handle),
            glRasterPos2iv: get_func("glRasterPos2iv", self._opengl32_dll_handle),
            glRasterPos2s: get_func("glRasterPos2s", self._opengl32_dll_handle),
            glRasterPos2sv: get_func("glRasterPos2sv", self._opengl32_dll_handle),
            glRasterPos3d: get_func("glRasterPos3d", self._opengl32_dll_handle),
            glRasterPos3dv: get_func("glRasterPos3dv", self._opengl32_dll_handle),
            glRasterPos3f: get_func("glRasterPos3f", self._opengl32_dll_handle),
            glRasterPos3fv: get_func("glRasterPos3fv", self._opengl32_dll_handle),
            glRasterPos3i: get_func("glRasterPos3i", self._opengl32_dll_handle),
            glRasterPos3iv: get_func("glRasterPos3iv", self._opengl32_dll_handle),
            glRasterPos3s: get_func("glRasterPos3s", self._opengl32_dll_handle),
            glRasterPos3sv: get_func("glRasterPos3sv", self._opengl32_dll_handle),
            glRasterPos4d: get_func("glRasterPos4d", self._opengl32_dll_handle),
            glRasterPos4dv: get_func("glRasterPos4dv", self._opengl32_dll_handle),
            glRasterPos4f: get_func("glRasterPos4f", self._opengl32_dll_handle),
            glRasterPos4fv: get_func("glRasterPos4fv", self._opengl32_dll_handle),
            glRasterPos4i: get_func("glRasterPos4i", self._opengl32_dll_handle),
            glRasterPos4iv: get_func("glRasterPos4iv", self._opengl32_dll_handle),
            glRasterPos4s: get_func("glRasterPos4s", self._opengl32_dll_handle),
            glRasterPos4sv: get_func("glRasterPos4sv", self._opengl32_dll_handle),
            glReadBuffer: get_func("glReadBuffer", self._opengl32_dll_handle),
            glReadPixels: get_func("glReadPixels", self._opengl32_dll_handle),
            glRectd: get_func("glRectd", self._opengl32_dll_handle),
            glRectdv: get_func("glRectdv", self._opengl32_dll_handle),
            glRectf: get_func("glRectf", self._opengl32_dll_handle),
            glRectfv: get_func("glRectfv", self._opengl32_dll_handle),
            glRecti: get_func("glRecti", self._opengl32_dll_handle),
            glRectiv: get_func("glRectiv", self._opengl32_dll_handle),
            glRects: get_func("glRects", self._opengl32_dll_handle),
            glRectsv: get_func("glRectsv", self._opengl32_dll_handle),
            glRenderMode: get_func("glRenderMode", self._opengl32_dll_handle),
            glRenderbufferStorage: get_func("glRenderbufferStorage", self._opengl32_dll_handle),
            glRenderbufferStorageMultisample: get_func(
                "glRenderbufferStorageMultisample",
                self._opengl32_dll_handle,
            ),
            glRotated: get_func("glRotated", self._opengl32_dll_handle),
            glRotatef: get_func("glRotatef", self._opengl32_dll_handle),
            glSampleCoverage: get_func("glSampleCoverage", self._opengl32_dll_handle),
            glSampleMaski: get_func("glSampleMaski", self._opengl32_dll_handle),
            glSamplerParameterIiv: get_func("glSamplerParameterIiv", self._opengl32_dll_handle),
            glSamplerParameterIuiv: get_func("glSamplerParameterIuiv", self._opengl32_dll_handle),
            glSamplerParameterf: get_func("glSamplerParameterf", self._opengl32_dll_handle),
            glSamplerParameterfv: get_func("glSamplerParameterfv", self._opengl32_dll_handle),
            glSamplerParameteri: get_func("glSamplerParameteri", self._opengl32_dll_handle),
            glSamplerParameteriv: get_func("glSamplerParameteriv", self._opengl32_dll_handle),
            glScaled: get_func("glScaled", self._opengl32_dll_handle),
            glScalef: get_func("glScalef", self._opengl32_dll_handle),
            glScissor: get_func("glScissor", self._opengl32_dll_handle),
            glSecondaryColor3b: get_func("glSecondaryColor3b", self._opengl32_dll_handle),
            glSecondaryColor3bv: get_func("glSecondaryColor3bv", self._opengl32_dll_handle),
            glSecondaryColor3d: get_func("glSecondaryColor3d", self._opengl32_dll_handle),
            glSecondaryColor3dv: get_func("glSecondaryColor3dv", self._opengl32_dll_handle),
            glSecondaryColor3f: get_func("glSecondaryColor3f", self._opengl32_dll_handle),
            glSecondaryColor3fv: get_func("glSecondaryColor3fv", self._opengl32_dll_handle),
            glSecondaryColor3i: get_func("glSecondaryColor3i", self._opengl32_dll_handle),
            glSecondaryColor3iv: get_func("glSecondaryColor3iv", self._opengl32_dll_handle),
            glSecondaryColor3s: get_func("glSecondaryColor3s", self._opengl32_dll_handle),
            glSecondaryColor3sv: get_func("glSecondaryColor3sv", self._opengl32_dll_handle),
            glSecondaryColor3ub: get_func("glSecondaryColor3ub", self._opengl32_dll_handle),
            glSecondaryColor3ubv: get_func("glSecondaryColor3ubv", self._opengl32_dll_handle),
            glSecondaryColor3ui: get_func("glSecondaryColor3ui", self._opengl32_dll_handle),
            glSecondaryColor3uiv: get_func("glSecondaryColor3uiv", self._opengl32_dll_handle),
            glSecondaryColor3us: get_func("glSecondaryColor3us", self._opengl32_dll_handle),
            glSecondaryColor3usv: get_func("glSecondaryColor3usv", self._opengl32_dll_handle),
            glSecondaryColorP3ui: get_func("glSecondaryColorP3ui", self._opengl32_dll_handle),
            glSecondaryColorP3uiv: get_func("glSecondaryColorP3uiv", self._opengl32_dll_handle),
            glSecondaryColorPointer: get_func("glSecondaryColorPointer", self._opengl32_dll_handle),
            glSelectBuffer: get_func("glSelectBuffer", self._opengl32_dll_handle),
            glSetFenceAPPLE: get_func("glSetFenceAPPLE", self._opengl32_dll_handle),
            glShadeModel: get_func("glShadeModel", self._opengl32_dll_handle),
            glShaderSource: get_func("glShaderSource", self._opengl32_dll_handle),
            glShaderStorageBlockBinding: get_func(
                "glShaderStorageBlockBinding",
                self._opengl32_dll_handle,
            ),
            glStencilFunc: get_func("glStencilFunc", self._opengl32_dll_handle),
            glStencilFuncSeparate: get_func("glStencilFuncSeparate", self._opengl32_dll_handle),
            glStencilMask: get_func("glStencilMask", self._opengl32_dll_handle),
            glStencilMaskSeparate: get_func("glStencilMaskSeparate", self._opengl32_dll_handle),
            glStencilOp: get_func("glStencilOp", self._opengl32_dll_handle),
            glStencilOpSeparate: get_func("glStencilOpSeparate", self._opengl32_dll_handle),
            glTestFenceAPPLE: get_func("glTestFenceAPPLE", self._opengl32_dll_handle),
            glTestObjectAPPLE: get_func("glTestObjectAPPLE", self._opengl32_dll_handle),
            glTexBuffer: get_func("glTexBuffer", self._opengl32_dll_handle),
            glTexCoord1d: get_func("glTexCoord1d", self._opengl32_dll_handle),
            glTexCoord1dv: get_func("glTexCoord1dv", self._opengl32_dll_handle),
            glTexCoord1f: get_func("glTexCoord1f", self._opengl32_dll_handle),
            glTexCoord1fv: get_func("glTexCoord1fv", self._opengl32_dll_handle),
            glTexCoord1i: get_func("glTexCoord1i", self._opengl32_dll_handle),
            glTexCoord1iv: get_func("glTexCoord1iv", self._opengl32_dll_handle),
            glTexCoord1s: get_func("glTexCoord1s", self._opengl32_dll_handle),
            glTexCoord1sv: get_func("glTexCoord1sv", self._opengl32_dll_handle),
            glTexCoord2d: get_func("glTexCoord2d", self._opengl32_dll_handle),
            glTexCoord2dv: get_func("glTexCoord2dv", self._opengl32_dll_handle),
            glTexCoord2f: get_func("glTexCoord2f", self._opengl32_dll_handle),
            glTexCoord2fv: get_func("glTexCoord2fv", self._opengl32_dll_handle),
            glTexCoord2i: get_func("glTexCoord2i", self._opengl32_dll_handle),
            glTexCoord2iv: get_func("glTexCoord2iv", self._opengl32_dll_handle),
            glTexCoord2s: get_func("glTexCoord2s", self._opengl32_dll_handle),
            glTexCoord2sv: get_func("glTexCoord2sv", self._opengl32_dll_handle),
            glTexCoord3d: get_func("glTexCoord3d", self._opengl32_dll_handle),
            glTexCoord3dv: get_func("glTexCoord3dv", self._opengl32_dll_handle),
            glTexCoord3f: get_func("glTexCoord3f", self._opengl32_dll_handle),
            glTexCoord3fv: get_func("glTexCoord3fv", self._opengl32_dll_handle),
            glTexCoord3i: get_func("glTexCoord3i", self._opengl32_dll_handle),
            glTexCoord3iv: get_func("glTexCoord3iv", self._opengl32_dll_handle),
            glTexCoord3s: get_func("glTexCoord3s", self._opengl32_dll_handle),
            glTexCoord3sv: get_func("glTexCoord3sv", self._opengl32_dll_handle),
            glTexCoord4d: get_func("glTexCoord4d", self._opengl32_dll_handle),
            glTexCoord4dv: get_func("glTexCoord4dv", self._opengl32_dll_handle),
            glTexCoord4f: get_func("glTexCoord4f", self._opengl32_dll_handle),
            glTexCoord4fv: get_func("glTexCoord4fv", self._opengl32_dll_handle),
            glTexCoord4i: get_func("glTexCoord4i", self._opengl32_dll_handle),
            glTexCoord4iv: get_func("glTexCoord4iv", self._opengl32_dll_handle),
            glTexCoord4s: get_func("glTexCoord4s", self._opengl32_dll_handle),
            glTexCoord4sv: get_func("glTexCoord4sv", self._opengl32_dll_handle),
            glTexCoordP1ui: get_func("glTexCoordP1ui", self._opengl32_dll_handle),
            glTexCoordP1uiv: get_func("glTexCoordP1uiv", self._opengl32_dll_handle),
            glTexCoordP2ui: get_func("glTexCoordP2ui", self._opengl32_dll_handle),
            glTexCoordP2uiv: get_func("glTexCoordP2uiv", self._opengl32_dll_handle),
            glTexCoordP3ui: get_func("glTexCoordP3ui", self._opengl32_dll_handle),
            glTexCoordP3uiv: get_func("glTexCoordP3uiv", self._opengl32_dll_handle),
            glTexCoordP4ui: get_func("glTexCoordP4ui", self._opengl32_dll_handle),
            glTexCoordP4uiv: get_func("glTexCoordP4uiv", self._opengl32_dll_handle),
            glTexCoordPointer: get_func("glTexCoordPointer", self._opengl32_dll_handle),
            glTexEnvf: get_func("glTexEnvf", self._opengl32_dll_handle),
            glTexEnvfv: get_func("glTexEnvfv", self._opengl32_dll_handle),
            glTexEnvi: get_func("glTexEnvi", self._opengl32_dll_handle),
            glTexEnviv: get_func("glTexEnviv", self._opengl32_dll_handle),
            glTexGend: get_func("glTexGend", self._opengl32_dll_handle),
            glTexGendv: get_func("glTexGendv", self._opengl32_dll_handle),
            glTexGenf: get_func("glTexGenf", self._opengl32_dll_handle),
            glTexGenfv: get_func("glTexGenfv", self._opengl32_dll_handle),
            glTexGeni: get_func("glTexGeni", self._opengl32_dll_handle),
            glTexGeniv: get_func("glTexGeniv", self._opengl32_dll_handle),
            glTexImage1D: get_func("glTexImage1D", self._opengl32_dll_handle),
            glTexImage2D: get_func("glTexImage2D", self._opengl32_dll_handle),
            glTexImage2DMultisample: get_func("glTexImage2DMultisample", self._opengl32_dll_handle),
            glTexImage3D: get_func("glTexImage3D", self._opengl32_dll_handle),
            glTexImage3DMultisample: get_func("glTexImage3DMultisample", self._opengl32_dll_handle),
            glTexParameterIiv: get_func("glTexParameterIiv", self._opengl32_dll_handle),
            glTexParameterIuiv: get_func("glTexParameterIuiv", self._opengl32_dll_handle),
            glTexParameterf: get_func("glTexParameterf", self._opengl32_dll_handle),
            glTexParameterfv: get_func("glTexParameterfv", self._opengl32_dll_handle),
            glTexParameteri: get_func("glTexParameteri", self._opengl32_dll_handle),
            glTexParameteriv: get_func("glTexParameteriv", self._opengl32_dll_handle),
            glTexStorage1D: get_func("glTexStorage1D", self._opengl32_dll_handle),
            glTexStorage2D: get_func("glTexStorage2D", self._opengl32_dll_handle),
            glTexStorage3D: get_func("glTexStorage3D", self._opengl32_dll_handle),
            glTexSubImage1D: get_func("glTexSubImage1D", self._opengl32_dll_handle),
            glTexSubImage2D: get_func("glTexSubImage2D", self._opengl32_dll_handle),
            glTexSubImage3D: get_func("glTexSubImage3D", self._opengl32_dll_handle),
            glTextureRangeAPPLE: get_func("glTextureRangeAPPLE", self._opengl32_dll_handle),
            glTransformFeedbackVaryings: get_func(
                "glTransformFeedbackVaryings",
                self._opengl32_dll_handle,
            ),
            glTranslated: get_func("glTranslated", self._opengl32_dll_handle),
            glTranslatef: get_func("glTranslatef", self._opengl32_dll_handle),
            glUniform1f: get_func("glUniform1f", self._opengl32_dll_handle),
            glUniform1fv: get_func("glUniform1fv", self._opengl32_dll_handle),
            glUniform1i: get_func("glUniform1i", self._opengl32_dll_handle),
            glUniform1iv: get_func("glUniform1iv", self._opengl32_dll_handle),
            glUniform1ui: get_func("glUniform1ui", self._opengl32_dll_handle),
            glUniform1uiv: get_func("glUniform1uiv", self._opengl32_dll_handle),
            glUniform2f: get_func("glUniform2f", self._opengl32_dll_handle),
            glUniform2fv: get_func("glUniform2fv", self._opengl32_dll_handle),
            glUniform2i: get_func("glUniform2i", self._opengl32_dll_handle),
            glUniform2iv: get_func("glUniform2iv", self._opengl32_dll_handle),
            glUniform2ui: get_func("glUniform2ui", self._opengl32_dll_handle),
            glUniform2uiv: get_func("glUniform2uiv", self._opengl32_dll_handle),
            glUniform3f: get_func("glUniform3f", self._opengl32_dll_handle),
            glUniform3fv: get_func("glUniform3fv", self._opengl32_dll_handle),
            glUniform3i: get_func("glUniform3i", self._opengl32_dll_handle),
            glUniform3iv: get_func("glUniform3iv", self._opengl32_dll_handle),
            glUniform3ui: get_func("glUniform3ui", self._opengl32_dll_handle),
            glUniform3uiv: get_func("glUniform3uiv", self._opengl32_dll_handle),
            glUniform4f: get_func("glUniform4f", self._opengl32_dll_handle),
            glUniform4fv: get_func("glUniform4fv", self._opengl32_dll_handle),
            glUniform4i: get_func("glUniform4i", self._opengl32_dll_handle),
            glUniform4iv: get_func("glUniform4iv", self._opengl32_dll_handle),
            glUniform4ui: get_func("glUniform4ui", self._opengl32_dll_handle),
            glUniform4uiv: get_func("glUniform4uiv", self._opengl32_dll_handle),
            glUniformBlockBinding: get_func("glUniformBlockBinding", self._opengl32_dll_handle),
            glUniformMatrix2fv: get_func("glUniformMatrix2fv", self._opengl32_dll_handle),
            glUniformMatrix2x3fv: get_func("glUniformMatrix2x3fv", self._opengl32_dll_handle),
            glUniformMatrix2x4fv: get_func("glUniformMatrix2x4fv", self._opengl32_dll_handle),
            glUniformMatrix3fv: get_func("glUniformMatrix3fv", self._opengl32_dll_handle),
            glUniformMatrix3x2fv: get_func("glUniformMatrix3x2fv", self._opengl32_dll_handle),
            glUniformMatrix3x4fv: get_func("glUniformMatrix3x4fv", self._opengl32_dll_handle),
            glUniformMatrix4fv: get_func("glUniformMatrix4fv", self._opengl32_dll_handle),
            glUniformMatrix4x2fv: get_func("glUniformMatrix4x2fv", self._opengl32_dll_handle),
            glUniformMatrix4x3fv: get_func("glUniformMatrix4x3fv", self._opengl32_dll_handle),
            glUnmapBuffer: get_func("glUnmapBuffer", self._opengl32_dll_handle),
            glUseProgram: get_func("glUseProgram", self._opengl32_dll_handle),
            glValidateProgram: get_func("glValidateProgram", self._opengl32_dll_handle),
            glVertex2d: get_func("glVertex2d", self._opengl32_dll_handle),
            glVertex2dv: get_func("glVertex2dv", self._opengl32_dll_handle),
            glVertex2f: get_func("glVertex2f", self._opengl32_dll_handle),
            glVertex2fv: get_func("glVertex2fv", self._opengl32_dll_handle),
            glVertex2i: get_func("glVertex2i", self._opengl32_dll_handle),
            glVertex2iv: get_func("glVertex2iv", self._opengl32_dll_handle),
            glVertex2s: get_func("glVertex2s", self._opengl32_dll_handle),
            glVertex2sv: get_func("glVertex2sv", self._opengl32_dll_handle),
            glVertex3d: get_func("glVertex3d", self._opengl32_dll_handle),
            glVertex3dv: get_func("glVertex3dv", self._opengl32_dll_handle),
            glVertex3f: get_func("glVertex3f", self._opengl32_dll_handle),
            glVertex3fv: get_func("glVertex3fv", self._opengl32_dll_handle),
            glVertex3i: get_func("glVertex3i", self._opengl32_dll_handle),
            glVertex3iv: get_func("glVertex3iv", self._opengl32_dll_handle),
            glVertex3s: get_func("glVertex3s", self._opengl32_dll_handle),
            glVertex3sv: get_func("glVertex3sv", self._opengl32_dll_handle),
            glVertex4d: get_func("glVertex4d", self._opengl32_dll_handle),
            glVertex4dv: get_func("glVertex4dv", self._opengl32_dll_handle),
            glVertex4f: get_func("glVertex4f", self._opengl32_dll_handle),
            glVertex4fv: get_func("glVertex4fv", self._opengl32_dll_handle),
            glVertex4i: get_func("glVertex4i", self._opengl32_dll_handle),
            glVertex4iv: get_func("glVertex4iv", self._opengl32_dll_handle),
            glVertex4s: get_func("glVertex4s", self._opengl32_dll_handle),
            glVertex4sv: get_func("glVertex4sv", self._opengl32_dll_handle),
            glVertexAttrib1d: get_func("glVertexAttrib1d", self._opengl32_dll_handle),
            glVertexAttrib1dv: get_func("glVertexAttrib1dv", self._opengl32_dll_handle),
            glVertexAttrib1f: get_func("glVertexAttrib1f", self._opengl32_dll_handle),
            glVertexAttrib1fv: get_func("glVertexAttrib1fv", self._opengl32_dll_handle),
            glVertexAttrib1s: get_func("glVertexAttrib1s", self._opengl32_dll_handle),
            glVertexAttrib1sv: get_func("glVertexAttrib1sv", self._opengl32_dll_handle),
            glVertexAttrib2d: get_func("glVertexAttrib2d", self._opengl32_dll_handle),
            glVertexAttrib2dv: get_func("glVertexAttrib2dv", self._opengl32_dll_handle),
            glVertexAttrib2f: get_func("glVertexAttrib2f", self._opengl32_dll_handle),
            glVertexAttrib2fv: get_func("glVertexAttrib2fv", self._opengl32_dll_handle),
            glVertexAttrib2s: get_func("glVertexAttrib2s", self._opengl32_dll_handle),
            glVertexAttrib2sv: get_func("glVertexAttrib2sv", self._opengl32_dll_handle),
            glVertexAttrib3d: get_func("glVertexAttrib3d", self._opengl32_dll_handle),
            glVertexAttrib3dv: get_func("glVertexAttrib3dv", self._opengl32_dll_handle),
            glVertexAttrib3f: get_func("glVertexAttrib3f", self._opengl32_dll_handle),
            glVertexAttrib3fv: get_func("glVertexAttrib3fv", self._opengl32_dll_handle),
            glVertexAttrib3s: get_func("glVertexAttrib3s", self._opengl32_dll_handle),
            glVertexAttrib3sv: get_func("glVertexAttrib3sv", self._opengl32_dll_handle),
            glVertexAttrib4Nbv: get_func("glVertexAttrib4Nbv", self._opengl32_dll_handle),
            glVertexAttrib4Niv: get_func("glVertexAttrib4Niv", self._opengl32_dll_handle),
            glVertexAttrib4Nsv: get_func("glVertexAttrib4Nsv", self._opengl32_dll_handle),
            glVertexAttrib4Nub: get_func("glVertexAttrib4Nub", self._opengl32_dll_handle),
            glVertexAttrib4Nubv: get_func("glVertexAttrib4Nubv", self._opengl32_dll_handle),
            glVertexAttrib4Nuiv: get_func("glVertexAttrib4Nuiv", self._opengl32_dll_handle),
            glVertexAttrib4Nusv: get_func("glVertexAttrib4Nusv", self._opengl32_dll_handle),
            glVertexAttrib4bv: get_func("glVertexAttrib4bv", self._opengl32_dll_handle),
            glVertexAttrib4d: get_func("glVertexAttrib4d", self._opengl32_dll_handle),
            glVertexAttrib4dv: get_func("glVertexAttrib4dv", self._opengl32_dll_handle),
            glVertexAttrib4f: get_func("glVertexAttrib4f", self._opengl32_dll_handle),
            glVertexAttrib4fv: get_func("glVertexAttrib4fv", self._opengl32_dll_handle),
            glVertexAttrib4iv: get_func("glVertexAttrib4iv", self._opengl32_dll_handle),
            glVertexAttrib4s: get_func("glVertexAttrib4s", self._opengl32_dll_handle),
            glVertexAttrib4sv: get_func("glVertexAttrib4sv", self._opengl32_dll_handle),
            glVertexAttrib4ubv: get_func("glVertexAttrib4ubv", self._opengl32_dll_handle),
            glVertexAttrib4uiv: get_func("glVertexAttrib4uiv", self._opengl32_dll_handle),
            glVertexAttrib4usv: get_func("glVertexAttrib4usv", self._opengl32_dll_handle),
            glVertexAttribDivisor: get_func("glVertexAttribDivisor", self._opengl32_dll_handle),
            glVertexAttribI1i: get_func("glVertexAttribI1i", self._opengl32_dll_handle),
            glVertexAttribI1iv: get_func("glVertexAttribI1iv", self._opengl32_dll_handle),
            glVertexAttribI1ui: get_func("glVertexAttribI1ui", self._opengl32_dll_handle),
            glVertexAttribI1uiv: get_func("glVertexAttribI1uiv", self._opengl32_dll_handle),
            glVertexAttribI2i: get_func("glVertexAttribI2i", self._opengl32_dll_handle),
            glVertexAttribI2iv: get_func("glVertexAttribI2iv", self._opengl32_dll_handle),
            glVertexAttribI2ui: get_func("glVertexAttribI2ui", self._opengl32_dll_handle),
            glVertexAttribI2uiv: get_func("glVertexAttribI2uiv", self._opengl32_dll_handle),
            glVertexAttribI3i: get_func("glVertexAttribI3i", self._opengl32_dll_handle),
            glVertexAttribI3iv: get_func("glVertexAttribI3iv", self._opengl32_dll_handle),
            glVertexAttribI3ui: get_func("glVertexAttribI3ui", self._opengl32_dll_handle),
            glVertexAttribI3uiv: get_func("glVertexAttribI3uiv", self._opengl32_dll_handle),
            glVertexAttribI4bv: get_func("glVertexAttribI4bv", self._opengl32_dll_handle),
            glVertexAttribI4i: get_func("glVertexAttribI4i", self._opengl32_dll_handle),
            glVertexAttribI4iv: get_func("glVertexAttribI4iv", self._opengl32_dll_handle),
            glVertexAttribI4sv: get_func("glVertexAttribI4sv", self._opengl32_dll_handle),
            glVertexAttribI4ubv: get_func("glVertexAttribI4ubv", self._opengl32_dll_handle),
            glVertexAttribI4ui: get_func("glVertexAttribI4ui", self._opengl32_dll_handle),
            glVertexAttribI4uiv: get_func("glVertexAttribI4uiv", self._opengl32_dll_handle),
            glVertexAttribI4usv: get_func("glVertexAttribI4usv", self._opengl32_dll_handle),
            glVertexAttribIPointer: get_func("glVertexAttribIPointer", self._opengl32_dll_handle),
            glVertexAttribP1ui: get_func("glVertexAttribP1ui", self._opengl32_dll_handle),
            glVertexAttribP1uiv: get_func("glVertexAttribP1uiv", self._opengl32_dll_handle),
            glVertexAttribP2ui: get_func("glVertexAttribP2ui", self._opengl32_dll_handle),
            glVertexAttribP2uiv: get_func("glVertexAttribP2uiv", self._opengl32_dll_handle),
            glVertexAttribP3ui: get_func("glVertexAttribP3ui", self._opengl32_dll_handle),
            glVertexAttribP3uiv: get_func("glVertexAttribP3uiv", self._opengl32_dll_handle),
            glVertexAttribP4ui: get_func("glVertexAttribP4ui", self._opengl32_dll_handle),
            glVertexAttribP4uiv: get_func("glVertexAttribP4uiv", self._opengl32_dll_handle),
            glVertexAttribPointer: get_func("glVertexAttribPointer", self._opengl32_dll_handle),
            glVertexP2ui: get_func("glVertexP2ui", self._opengl32_dll_handle),
            glVertexP2uiv: get_func("glVertexP2uiv", self._opengl32_dll_handle),
            glVertexP3ui: get_func("glVertexP3ui", self._opengl32_dll_handle),
            glVertexP3uiv: get_func("glVertexP3uiv", self._opengl32_dll_handle),
            glVertexP4ui: get_func("glVertexP4ui", self._opengl32_dll_handle),
            glVertexP4uiv: get_func("glVertexP4uiv", self._opengl32_dll_handle),
            glVertexPointer: get_func("glVertexPointer", self._opengl32_dll_handle),
            glViewport: get_func("glViewport", self._opengl32_dll_handle),
            glWaitSync: get_func("glWaitSync", self._opengl32_dll_handle),
            glWindowPos2d: get_func("glWindowPos2d", self._opengl32_dll_handle),
            glWindowPos2dv: get_func("glWindowPos2dv", self._opengl32_dll_handle),
            glWindowPos2f: get_func("glWindowPos2f", self._opengl32_dll_handle),
            glWindowPos2fv: get_func("glWindowPos2fv", self._opengl32_dll_handle),
            glWindowPos2i: get_func("glWindowPos2i", self._opengl32_dll_handle),
            glWindowPos2iv: get_func("glWindowPos2iv", self._opengl32_dll_handle),
            glWindowPos2s: get_func("glWindowPos2s", self._opengl32_dll_handle),
            glWindowPos2sv: get_func("glWindowPos2sv", self._opengl32_dll_handle),
            glWindowPos3d: get_func("glWindowPos3d", self._opengl32_dll_handle),
            glWindowPos3dv: get_func("glWindowPos3dv", self._opengl32_dll_handle),
            glWindowPos3f: get_func("glWindowPos3f", self._opengl32_dll_handle),
            glWindowPos3fv: get_func("glWindowPos3fv", self._opengl32_dll_handle),
            glWindowPos3i: get_func("glWindowPos3i", self._opengl32_dll_handle),
            glWindowPos3iv: get_func("glWindowPos3iv", self._opengl32_dll_handle),
            glWindowPos3s: get_func("glWindowPos3s", self._opengl32_dll_handle),
            glWindowPos3sv: get_func("glWindowPos3sv", self._opengl32_dll_handle),
        });
    }
}

impl Drop for GlFunctions {
    fn drop(&mut self) {
        use winapi::um::libloaderapi::FreeLibrary;
        if let Some(opengl32) = self._opengl32_dll_handle {
            unsafe {
                FreeLibrary(opengl32);
            }
        }
    }
}

#[derive(Default)]
struct ExtraWglFunctions {
    wglCreateContextAttribsARB: Option<extern "system" fn(HDC, HGLRC, *const i32) -> HGLRC>,
    wglSwapIntervalEXT: Option<extern "system" fn(i32) -> i32>,
    wglChoosePixelFormatARB: Option<extern "system" fn(HDC, *const i32, *const f32, u32, *mut i32, *mut u32) -> BOOL>,
}

impl fmt::Debug for ExtraWglFunctions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.wglCreateContextAttribsARB.map(|f| f as usize).fmt(f)?;
        self.wglSwapIntervalEXT.map(|f| f as usize).fmt(f)?;
        self.wglChoosePixelFormatARB.map(|f| f as usize).fmt(f)?;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ExtraWglFunctionsLoadError {
    FailedToCreateDummyWindow,
    FailedToFindPixelFormat,
    FailedToSetPixelFormat,
    FailedToCreateDummyGlContext,
    FailedToActivateDummyGlContext,
}

impl ExtraWglFunctions {
    pub fn load() -> Result<Self, ExtraWglFunctionsLoadError> {

        use winapi::um::{
            libloaderapi::GetModuleHandleW,
            winuser::{
                CreateWindowExW, GetDC,
                ReleaseDC, DestroyWindow,
                CW_USEDEFAULT
            },
            wingdi::{
                wglGetProcAddress, ChoosePixelFormat,
                SetPixelFormat, wglCreateContext,
                wglMakeCurrent, wglDeleteContext,
            },
        };
        use self::ExtraWglFunctionsLoadError::*;

        unsafe {

            let mut hidden_class_name = encode_wide(CLASS_NAME);
            let mut hidden_window_title = encode_wide("Dummy Window");

            let dummy_window = CreateWindowExW(
                0,
                hidden_class_name.as_mut_ptr(),
                hidden_window_title.as_mut_ptr(),
                0,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                ptr::null_mut(),
                ptr::null_mut(),
                GetModuleHandleW(ptr::null_mut()),
                ptr::null_mut(),
            );

            if dummy_window.is_null() {
                return Err(FailedToCreateDummyWindow);
            }

            let dummy_dc = GetDC(dummy_window);

            let mut pfd = get_default_pfd();

            let pixel_format = ChoosePixelFormat(dummy_dc, &pfd);
            if pixel_format == 0 {
                return Err(FailedToFindPixelFormat);
            }

            if SetPixelFormat(dummy_dc, pixel_format, &pfd) != TRUE {
                return Err(FailedToSetPixelFormat);
            }

            let dummy_context = wglCreateContext(dummy_dc);
            if dummy_context.is_null() {
                return Err(FailedToCreateDummyGlContext);
            }

            if wglMakeCurrent(dummy_dc, dummy_context) != TRUE {
                return Err(FailedToActivateDummyGlContext);
            }

            let mut extra_functions = ExtraWglFunctions::default();

            extra_functions.wglChoosePixelFormatARB = {
                let mut func_name_1 = encode_ascii("wglChoosePixelFormatARB");
                let mut func_name_2 = encode_ascii("wglChoosePixelFormatEXT");

                let wgl1_result = unsafe { wglGetProcAddress(func_name_1.as_mut_ptr()) };
                let wgl2_result = unsafe { wglGetProcAddress(func_name_2.as_mut_ptr()) };

                if wgl1_result != ptr::null_mut() {
                    Some(unsafe { mem::transmute(wgl1_result) })
                } else if wgl2_result != ptr::null_mut() {
                    Some(unsafe { mem::transmute(wgl2_result) })
                } else {
                    None
                }
            };

            extra_functions.wglCreateContextAttribsARB = {
                let mut func_name = encode_ascii("wglCreateContextAttribsARB");
                let proc_address = unsafe { wglGetProcAddress(func_name.as_mut_ptr()) };
                if proc_address == ptr::null_mut() {
                    None
                } else {
                    Some(unsafe { mem::transmute(proc_address) })
                }
            };

            extra_functions.wglSwapIntervalEXT = {
                let mut func_name = encode_ascii("wglSwapIntervalEXT");
                let proc_address = unsafe { wglGetProcAddress(func_name.as_mut_ptr()) };
                if proc_address == ptr::null_mut() {
                    None
                } else {
                    Some(unsafe { mem::transmute(proc_address) })
                }
            };

            wglMakeCurrent(dummy_dc, ptr::null_mut());
            wglDeleteContext(dummy_context);
            ReleaseDC(dummy_window, dummy_dc);
            DestroyWindow(dummy_window);

            return Ok(extra_functions);
        }
    }
}

struct Window {
    /// HWND handle of the plaform window
    hwnd: HWND,
    /// See azul-core, stores the entire UI (DOM, CSS styles, layout results, etc.)
    internal: WindowInternal,
    /// OpenGL context handle - None if running in software mode
    gl_context: Option<HGLRC>,
    /// OpenGL functions for faster rendering
    gl_functions: GlFunctions,
    /// OpenGL context pointer with compiled SVG and FXAA shaders
    gl_context_ptr: OptionGlContextPtr,
    /// Main render API that can be used to register and un-register fonts and images
    render_api: WrRenderApi,
    /// WebRender renderer implementation (software or hardware)
    renderer: Option<WrRenderer>,
    /// Hit-tester, lazily initialized and updated every time the display list changes layout
    hit_tester: AsyncHitTester,
    /// ID -> Callback map for the window menu (default: empty map)
    menu_bar: Option<WindowsMenuBar>,
    /// ID -> Context menu callbacks (cleared when the context menu closes)
    context_menu: Option<CurrentContextMenu>,
    /// Timer ID -> Win32 timer map
    timers: BTreeMap<TimerId, TIMERPTR>,
    /// If threads is non-empty, the window will receive a WM_TIMER every 16ms
    thread_timer_running: Option<TIMERPTR>,
    /// characters are combined via two following wparam messages
    high_surrogate: Option<u16>,
}

impl fmt::Debug for Window {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.hwnd.fmt(f)?;
        self.internal.fmt(f)?;
        self.gl_context.fmt(f)?;
        self.gl_context_ptr.fmt(f)?;
        self.renderer.is_some().fmt(f)?;
        self.menu_bar.fmt(f)?;
        self.context_menu.fmt(f)?;
        self.high_surrogate.fmt(f)?;
        Ok(())
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        use winapi::um::wingdi::{wglMakeCurrent, wglDeleteContext};

        // drop the layout results first
        self.internal.layout_results = Vec::new();

        unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };

        if let Some(context) = self.gl_context.as_mut() {
            unsafe { wglDeleteContext(*context); }
        }

        if let Some(renderer) = self.renderer.take() {
            renderer.deinit();
        }
    }
}

#[derive(Debug)]
struct CurrentContextMenu {
    callbacks: BTreeMap<u16, MenuCallback>,
    hit_dom_node: DomNodeId,
}

impl Window {

    fn get_id(&self) -> usize {
        self.hwnd as usize
    }

    // Creates a new HWND according to the options
    fn create(
        hinstance: HINSTANCE,
        mut options: WindowCreateOptions,
        mut shared_application_data: SharedApplicationData,
    ) -> Result<Self, WindowsWindowCreateError> {

        use crate::{
            compositor::Compositor,
            wr_translate::{
                translate_document_id_wr, translate_id_namespace_wr, wr_translate_debug_flags,
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
        use winapi::{
            shared::windef::POINT,
            um::{
                wingdi::{
                    wglDeleteContext, wglMakeCurrent,
                    SwapBuffers, GetDeviceCaps,
                    LOGPIXELSX, LOGPIXELSY
                },
                winuser::{
                    CreateWindowExW, DestroyWindow, GetClientRect, GetCursorPos, GetDC,
                    GetWindowRect, ReleaseDC, ScreenToClient, SetMenu, CW_USEDEFAULT, WS_CAPTION,
                    WS_EX_ACCEPTFILES, WS_EX_APPWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
                    WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_THICKFRAME,
                    ShowWindow, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_NORMAL, SW_SHOWNORMAL,
                },
            },
        };
        use winapi::um::winuser::{
            SetWindowPos, HWND_TOP, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOZORDER,
        };
        let parent_window = match options
            .state
            .platform_specific_options
            .windows_options
            .parent_window
            .as_ref()
        {
            Some(hwnd) => (*hwnd) as HWND,
            None => ptr::null_mut(),
        };

        let mut class_name = encode_wide(CLASS_NAME);
        let mut window_title = encode_wide(options.state.title.as_str());

        let data_ptr = Box::into_raw(Box::new(shared_application_data.clone())) as *mut SharedApplicationData as *mut c_void;

        // Create the window
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_APPWINDOW | WS_EX_ACCEPTFILES,
                class_name.as_mut_ptr(),
                window_title.as_mut_ptr(),
                WS_OVERLAPPED
                    | WS_CAPTION
                    | WS_SYSMENU
                    | WS_THICKFRAME
                    | WS_MINIMIZEBOX
                    | WS_MAXIMIZEBOX
                    | WS_TABSTOP
                    | WS_POPUP,
                // Size and position: set later, after DPI factor has been queried
                CW_USEDEFAULT, // x
                CW_USEDEFAULT, // y
                if options.size_to_content { 0 } else { libm::roundf(options.state.size.dimensions.width) as i32 }, // width
                if options.size_to_content { 0 } else { libm::roundf(options.state.size.dimensions.height) as i32 }, // height
                parent_window,
                ptr::null_mut(), // Menu
                hinstance,
                data_ptr,
            )
        };

        if hwnd.is_null() {
            return Err(WindowsWindowCreateError::FailedToCreateHWND(
                get_last_error(),
            ));
        }

        // Get / store DPI
        // NOTE: GetDpiForWindow would be easier, but it's Win10 only
        let dpi = unsafe {
            let dc = GetDC(hwnd);
            let dpi_x = GetDeviceCaps(dc, LOGPIXELSX);
            let dpi_y = GetDeviceCaps(dc, LOGPIXELSY);
            dpi_x.max(dpi_y).max(0) as u32
        };
        let dpi_factor = dpi as f32 / 96.0;
        options.state.size.dpi = dpi;
        options.state.size.hidpi_factor = dpi_factor;
        options.state.size.system_hidpi_factor = dpi_factor;

        // Window created, now try initializing OpenGL context
        let renderer_types = match options.renderer.into_option() {
            Some(s) => match s.hw_accel {
                HwAcceleration::DontCare => vec![RendererType::Hardware, RendererType::Software],
                HwAcceleration::Enabled => vec![RendererType::Hardware],
                HwAcceleration::Disabled => vec![RendererType::Software],
            },
            None => vec![RendererType::Hardware, RendererType::Software],
        };

        let mut opengl_context: Option<HGLRC> = None;
        let mut rt = RendererType::Software;
        let mut extra = ExtraWglFunctions::load()?;
        let mut gl = GlFunctions::initialize();
        let mut gl_context_ptr: OptionGlContextPtr = None.into();

        for r in renderer_types {
            rt = r;
            match r {
                RendererType::Software => {}
                RendererType::Hardware => {
                    if let Ok(o) = create_gl_context(hwnd, hinstance, &extra) {
                        opengl_context = Some(o);
                        break;
                    }
                }
            }
        }

        gl_context_ptr = opengl_context
            .map(|hrc| unsafe {
                let hdc = GetDC(hwnd);
                unsafe { wglMakeCurrent(hdc, hrc) };
                gl.load();
                // compiles SVG and FXAA shader programs...
                let ptr = GlContextPtr::new(rt, gl.functions.clone());

                /*
                match options.renderer.as_ref().map(|v| v.vsync) {
                    Some(VSync::Enabled) => {
                        if let Some(wglSwapIntervalEXT) = extra_functions.wglSwapIntervalEXT {
                            unsafe { (wglSwapIntervalEXT)(1) };
                        }
                    },
                    Some(VSync::Disabled) => {
                        if let Some(wglSwapIntervalEXT) = extra_functions.wglSwapIntervalEXT {
                            unsafe { (wglSwapIntervalEXT)(0) };
                        }
                    },
                    _ => { },
                }
                */

                unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
                ReleaseDC(hwnd, hdc);
                ptr
            })
            .into();


        // WindowInternal::new() may dispatch OpenGL calls,
        // need to make context current before invoking
        let hdc = unsafe { GetDC(hwnd) };
        if let Some(hrc) = opengl_context.as_mut() {
            unsafe { wglMakeCurrent(hdc, *hrc) };
        }

        // Invoke callback to initialize UI for the first time
        let (mut renderer, sender) = match WrRenderer::new(
            gl.functions.clone(),
            Box::new(Notifier {}),
            WrRendererOptions {
                resource_override_path: None,
                use_optimized_shaders: true,
                enable_aa: true,
                enable_subpixel_aa: true,
                force_subpixel_aa: true,
                clear_color: WrColorF {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }, // transparent
                panic_on_gl_error: false,
                precache_flags: WrShaderPrecacheFlags::EMPTY,
                cached_programs: Some(WrProgramCache::new(None)),
                enable_multithreading: true,
                debug_flags: wr_translate_debug_flags(&options.state.debug_state),
                ..WrRendererOptions::default()
            },
            WR_SHADER_CACHE,
        ) {
            Ok(o) => o,
            Err(e) => unsafe {
                if let Some(hrc) = opengl_context.as_mut() {
                    wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                    wglDeleteContext(*hrc);
                }
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return Err(WindowsWindowCreateError::Renderer(e));
            },
        };


        renderer.set_external_image_handler(Box::new(Compositor::default()));

        let mut render_api = sender.create_api();

        // Query the current size of the window
        let physical_size = if options.size_to_content {
            PhysicalSize {
                width: 0,
                height: 0,
            }
        } else {
            let mut rect: RECT = unsafe { mem::zeroed() };
            let current_window_size = unsafe { GetClientRect(hwnd, &mut rect) }; // not DPI adjusted: physical pixels
            PhysicalSize {
                width: rect.width(),
                height: rect.height(),
            }
        };

        options.state.size.dimensions = physical_size.to_logical(dpi_factor);

        let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let pipeline_id = PipelineId::new();
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // hit tester will be empty on startup
        let hit_tester = render_api
            .request_hit_tester(wr_translate_document_id(document_id))
            .resolve();
        let hit_tester_ref = &*hit_tester;

        // lock the SharedApplicationData in order to
        // invoke the UI callback for the first time
        let mut appdata_lock = match shared_application_data.inner.try_borrow_mut() {
            Ok(o) => o,
            Err(e) => unsafe {
                if let Some(hrc) = opengl_context.as_mut() {
                    wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                    wglDeleteContext(*hrc);
                }
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return Err(WindowsWindowCreateError::BorrowMut(e));
            },
        };

        let mut initial_resource_updates = Vec::new();
        let mut internal = {

            let appdata_lock = &mut *appdata_lock;
            let fc_cache = &mut appdata_lock.fc_cache;
            let image_cache = &appdata_lock.image_cache;
            let data = &mut appdata_lock.data;

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
                            window_state.size.hidpi_factor,
                        )
                    },
                )
            })
        };

        // Since the menu bar affects the window size, set it first,
        // before querying the window size again
        let mut menu_bar = None;
        if let Some(m) = internal.get_menu_bar() {
            let mb = WindowsMenuBar::new(m);
            unsafe { SetMenu(hwnd, mb._native_ptr); }
            menu_bar = Some(mb);
        }

        // If size_to_content is set, query the content size and adjust!
        if options.size_to_content {
            let content_size = internal.get_content_size();
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOP,
                    0,
                    0,
                    libm::roundf(content_size.width) as i32,
                    libm::roundf(content_size.height) as i32,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
            }
        }

        // If the window is maximized on startup, we have to call ShowWindow here
        // before querying the client area
        let mut sw_options = SW_HIDE; // 0 = default
        let mut hidden_sw_options = SW_HIDE; // 0 = default
        if internal.current_window_state.flags.is_visible {
            sw_options |= SW_SHOWNORMAL;
        }

        match internal.current_window_state.flags.frame {
            WindowFrame::Normal => { sw_options |= SW_NORMAL; hidden_sw_options |= SW_NORMAL; },
            WindowFrame::Minimized => { sw_options |= SW_MINIMIZE; hidden_sw_options |= SW_MINIMIZE; },
            WindowFrame::Maximized => { sw_options |= SW_MAXIMIZE; hidden_sw_options |= SW_MAXIMIZE; },
            WindowFrame::Fullscreen => { sw_options |= SW_MAXIMIZE; hidden_sw_options |= SW_MAXIMIZE; },
        }

        unsafe { ShowWindow(hwnd, hidden_sw_options); }

        // Query the client area from Win32 (not DPI adjusted) and adjust framebuffer
        let mut rect: RECT = unsafe { mem::zeroed() };
        let current_window_size = unsafe { GetClientRect(hwnd, &mut rect) };
        let physical_size = PhysicalSize {
            width: rect.width(),
            height: rect.height(),
        };
        // internal.previous_window_state = Some(internal.current_window_state.clone());
        internal.current_window_state.size.dimensions = physical_size.to_logical(dpi_factor);

        let mut txn = WrTransaction::new();

        // re-layout the window content for the first frame
        // (since the width / height might have changed)
        {
            let appdata_lock = &mut *appdata_lock;
            let fc_cache = &mut appdata_lock.fc_cache;
            let image_cache = &appdata_lock.image_cache;
            let size = internal.current_window_state.size.clone();
            let theme = internal.current_window_state.theme;
            let resize_result = fc_cache.apply_closure(|fc_cache| {
                internal.do_quick_resize(
                    &image_cache,
                    &crate::app::CALLBACKS,
                    azul_layout::do_the_relayout,
                    fc_cache,
                    &gl_context_ptr,
                    &size,
                    theme,
                )
            });

            wr_synchronize_resize(resize_result, &document_id, &mut txn);
        }

        if let Some(hrc) = opengl_context.as_ref() {
            unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
        }

        unsafe { ReleaseDC(hwnd, hdc); }

        txn.set_document_view(
            WrDeviceIntRect::from_size(
                WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32),
            )
        );
        render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);

        render_api.flush_scene_builder();

        // Build the display list and send it to webrender for the first time
        rebuild_display_list(
            &mut internal,
            &mut render_api,
            &appdata_lock.image_cache,
            initial_resource_updates,
        );

        render_api.flush_scene_builder();

        generate_frame(
            &mut internal,
            &mut render_api,
            true,
        );

        render_api.flush_scene_builder();

        // Get / store mouse cursor position, now that the window position is final
        let mut cursor_pos: POINT = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut cursor_pos); }
        unsafe { ScreenToClient(hwnd, &mut cursor_pos) };
        let cursor_pos_logical = LogicalPosition {
            x: cursor_pos.x as f32 / dpi_factor,
            y: cursor_pos.y as f32 / dpi_factor,
        };
        internal.current_window_state.mouse_state.cursor_position = if cursor_pos.x <= 0 || cursor_pos.y <= 0 {
            CursorPosition::OutOfWindow
        } else {
            CursorPosition::InWindow(cursor_pos_logical)
        };

        // Update the hit-tester to account for the new hit-testing functionality
        let hit_tester = render_api.request_hit_tester(wr_translate_document_id(document_id));

        // Done! Window is now created properly, display list has been built by
        // WebRender (window is ready to render), menu bar is visible and hit-tester
        // now contains the newest UI tree.

        if options.hot_reload {
            use winapi::um::winuser::SetTimer;
            unsafe { SetTimer(hwnd, AZ_TICK_REGENERATE_DOM, 200, None); }
        }

        use winapi::um::winuser::PostMessageW;
        unsafe { PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0 ); }

        let mut window = Window {
            hwnd,
            internal,
            gl_context: opengl_context,
            gl_functions: gl,
            gl_context_ptr,
            render_api,
            renderer: Some(renderer),
            hit_tester: AsyncHitTester::Requested(hit_tester),
            menu_bar,
            context_menu: None,
            timers: BTreeMap::new(),
            thread_timer_running: None,
            high_surrogate: None,
        };

        // invoke the create callback, if there is any
        if let Some(create_callback) = options.create_callback.as_mut() {

            let hdc = unsafe { GetDC(hwnd) };
            if let Some(hrc) = opengl_context.as_mut() {
                unsafe { wglMakeCurrent(hdc, *hrc) };
            }

            let ab = &mut *appdata_lock;
            let fc_cache = &mut ab.fc_cache;
            let image_cache = &mut ab.image_cache;
            let data = &mut ab.data;
            let config = &ab.config;

            let ccr = fc_cache.apply_closure(|fc_cache| {
                use azul_core::window::{RawWindowHandle, WindowsHandle};

                window.internal.invoke_single_callback(
                    create_callback,
                    data,
                    &RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut core::ffi::c_void,
                        hinstance: hinstance as *mut core::ffi::c_void,
                    }),
                    &window.gl_context_ptr,
                    image_cache,
                    fc_cache,
                    &config.system_callbacks,
                )
            });

            let ntc = NodesToCheck::empty(
                window.internal.current_window_state.mouse_state.mouse_down(),
                window.internal.current_window_state.focused_node,
            );

            let mut new_windows = Vec::new();
            let mut destroyed_windows = Vec::new();

            let ret = process_callback_results(
                ccr,
                &mut window,
                &ntc,
                image_cache,
                &mut new_windows,
                &mut destroyed_windows,
            );

            if let Some(hrc) = opengl_context.as_mut() {
                unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
            }

            mem::drop(ab);
            mem::drop(appdata_lock);
            create_windows(hinstance, &mut shared_application_data, new_windows);
            let mut appdata_lock = shared_application_data.inner.try_borrow_mut().unwrap();
            let mut ab = &mut *appdata_lock;
            destroy_windows(ab, destroyed_windows);

            unsafe { ReleaseDC(hwnd, hdc); }
        }

        unsafe { ShowWindow(hwnd, sw_options); }

        // NOTE: The window is NOT stored yet
        Ok(window)
    }

    fn start_stop_timers(
        &mut self,
        added: FastHashMap<TimerId, Timer>,
        removed: FastBTreeSet<TimerId>
    ) {

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
    }

    fn start_stop_threads(
        &mut self,
        mut added: FastHashMap<ThreadId, Thread>,
        removed: FastBTreeSet<ThreadId>
    ) {

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
    }

    // ScrollResult contains information about what nodes need to be scrolled,
    // whether they were scrolled by the system or by the user and how far they
    // need to be scrolled
    fn do_system_scroll(&mut self, scroll: ScrollResult) {
        // for scrolled_node in scroll {
        //      self.render_api.scroll_node_with_id();
        //      let scrolled_rect = LogicalRect { origin: scroll_offset, size: visible.size };
        //      if !scrolled_node.scroll_bounds.contains(&scroll_rect) {
        //
        //      }
        // }
    }

    fn set_menu_bar(hwnd: HWND, old: &mut Option<WindowsMenuBar>, menu_bar: Option<&Box<Menu>>) {

        use winapi::um::winuser::SetMenu;

        let hash = old.as_ref().map(|o| o.hash);

        match (hash, menu_bar) {
            (Some(_), None) => {
                unsafe { SetMenu(hwnd, ptr::null_mut()); }
                *old = None;
            },
            (None, Some(new)) => {
                let new_menu_bar = WindowsMenuBar::new(new);
                unsafe { SetMenu(hwnd, new_menu_bar._native_ptr); }
                *old = Some(new_menu_bar);
            }
            (Some(hash), Some(new)) => {
                if hash != new.get_hash() {
                    let new_menu_bar = WindowsMenuBar::new(new);
                    unsafe { SetMenu(hwnd, new_menu_bar._native_ptr); }
                    *old = Some(new_menu_bar);
                }
            },
            (None, None) => { }
        }

    }
}

/// Creates an OpenGL 3.2 context using wglCreateContextAttribsARB
fn create_gl_context(hwnd: HWND, hinstance: HINSTANCE, extra: &ExtraWglFunctions)
-> Result<HGLRC, WindowsOpenGlError>
{
    use winapi::um::{
        wingdi::{
            wglCreateContext, wglDeleteContext,
            wglMakeCurrent, ChoosePixelFormat,
            DescribePixelFormat, SetPixelFormat,
        },
        winuser::{GetDC, ReleaseDC}
    };

    use self::WindowsOpenGlError::*;

    let wglCreateContextAttribsARB = extra.wglCreateContextAttribsARB
    .ok_or(OpenGLNotAvailable(get_last_error()))?;

    let wglChoosePixelFormatARB = extra.wglChoosePixelFormatARB
    .ok_or(OpenGLNotAvailable(get_last_error()))?;

    let opengl32_dll = load_dll("opengl32.dll")
    .ok_or(OpenGL32DllNotFound(get_last_error()))?;

    let hDC = unsafe { GetDC(hwnd) };
    if hDC.is_null() {
        return Err(FailedToGetDC(get_last_error()));
    }

    // https://www.khronos.org/registry/OpenGL/api/GL/wglext.h
    const WGL_DRAW_TO_WINDOW_ARB: i32 = 0x2001;
    const WGL_DOUBLE_BUFFER_ARB: i32 = 0x2011;
    const WGL_SUPPORT_OPENGL_ARB: i32 = 0x2010;
    const WGL_PIXEL_TYPE_ARB: i32 = 0x2013;
    const WGL_TYPE_RGBA_ARB: i32 = 0x202B;
    const WGL_TRANSPARENT_ARB: i32 = 0x200A;
    const WGL_COLOR_BITS_ARB: i32 = 0x2014;
    const WGL_RED_BITS_ARB: i32 = 0x2015;
    const WGL_GREEN_BITS_ARB: i32 = 0x2017;
    const WGL_BLUE_BITS_ARB: i32 = 0x2019;
    const WGL_ALPHA_BITS_ARB: i32 = 0x201B;
    const WGL_DEPTH_BITS_ARB: i32 = 0x2022;
    const WGL_STENCIL_BITS_ARB: i32 = 0x2023;
    const WGL_FULL_ACCELERATION_ARB: i32 =  0x2027;
    const WGL_ACCELERATION_ARB: i32 = 0x2003;

    const GL_TRUE: i32 = 1;

    let pixel_format_attribs = [
        WGL_DRAW_TO_WINDOW_ARB,     GL_TRUE,
        WGL_SUPPORT_OPENGL_ARB,     GL_TRUE,
        WGL_DOUBLE_BUFFER_ARB,      GL_TRUE,
        WGL_ACCELERATION_ARB,       WGL_FULL_ACCELERATION_ARB,
        WGL_PIXEL_TYPE_ARB,         WGL_TYPE_RGBA_ARB,
        WGL_COLOR_BITS_ARB,         32,
        WGL_DEPTH_BITS_ARB,         24,
        WGL_STENCIL_BITS_ARB,       8,
        0
    ];

    let mut pixel_format = 0;
    let mut num_formats = 0;
    unsafe { (wglChoosePixelFormatARB)(hDC, pixel_format_attribs.as_ptr(), ptr::null_mut(), 1, &mut pixel_format, &mut num_formats) };
    if num_formats == 0 {
        unsafe { ReleaseDC(hwnd, hDC); }
        return Err(NoMatchingPixelFormat(get_last_error()));
    }

    let mut pfd: PIXELFORMATDESCRIPTOR = get_default_pfd();

    unsafe {
        DescribePixelFormat(hDC, pixel_format, mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32, &mut pfd);
        if SetPixelFormat(hDC, pixel_format, &mut pfd) != TRUE{
            ReleaseDC(hwnd, hDC);
            return Err(NoMatchingPixelFormat(get_last_error()));
        }
    }

    // https://www.khronos.org/registry/OpenGL/extensions/ARB/WGL_ARB_create_context.txt
    const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
    const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;
    const WGL_CONTEXT_PROFILE_MASK_ARB: i32 = 0x9126;
    const WGL_CONTEXT_CORE_PROFILE_BIT_ARB: i32 = 0x00000001;

    // Create OpenGL 3.2 core context - #version 150 required by WR!
    // Specify that we want to create an OpenGL 3.3 core profile context
    let gl32_attribs = [
        WGL_CONTEXT_MAJOR_VERSION_ARB, 3,
        WGL_CONTEXT_MINOR_VERSION_ARB, 2,
        WGL_CONTEXT_PROFILE_MASK_ARB,  WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
        0,
    ];

    let gl32_context = unsafe { (wglCreateContextAttribsARB)(hDC, ptr::null_mut(), gl32_attribs.as_ptr()) };
    if gl32_context.is_null() {
        unsafe { ReleaseDC(hwnd, hDC); }
        return Err(OpenGLNotAvailable(get_last_error()));
    }

    unsafe { ReleaseDC(hwnd, hDC); }

    return Ok(gl32_context);
}


use winapi::um::wingdi::PIXELFORMATDESCRIPTOR;

fn get_default_pfd() -> PIXELFORMATDESCRIPTOR {

    use winapi::um::wingdi::{
        PFD_DRAW_TO_WINDOW,
        PFD_SUPPORT_OPENGL,
        PFD_GENERIC_ACCELERATED,
        PFD_DOUBLEBUFFER,
        PFD_MAIN_PLANE,
        PFD_TYPE_RGBA,
        PFD_SUPPORT_COMPOSITION,
    };

    PIXELFORMATDESCRIPTOR {
        nSize: mem::size_of::<PIXELFORMATDESCRIPTOR> as u16,
        nVersion: 1,
        dwFlags: {
            PFD_DRAW_TO_WINDOW |        // support window
            PFD_SUPPORT_OPENGL |        // support OpenGL
            PFD_DOUBLEBUFFER            // double buffered
        },
        iPixelType: PFD_TYPE_RGBA as u8,
        cColorBits: 32,
        cRedBits: 0,
        cRedShift: 0,
        cGreenBits: 0,
        cGreenShift: 0,
        cBlueBits: 0,
        cBlueShift: 0,
        cAlphaBits: 8, // request alpha
        cAlphaShift: 0,
        cAccumBits: 0,
        cAccumRedBits: 0,
        cAccumGreenBits: 0,
        cAccumBlueBits: 0,
        cAccumAlphaBits: 0,
        cDepthBits: 24,                   // 16-bit z-buffer
        cStencilBits: 8,                  // 8-bit stencil
        cAuxBuffers: 0,                   // no auxiliary buffer
        iLayerType: PFD_MAIN_PLANE as u8, // main layer
        bReserved: 0,
        dwLayerMask: 0,
        dwVisibleMask: 0,
        dwDamageMask: 0,
    }
}

#[derive(Debug)]
struct WindowsMenuBar {
    _native_ptr: HMENU,
    /// Map from Command -> callback to call
    callbacks: BTreeMap<u16, MenuCallback>,
    hash: u64,
}

static WINDOWS_UNIQUE_COMMAND_ID_GENERATOR: AtomicUsize = AtomicUsize::new(1); // 0 = no command

impl WindowsMenuBar {

    fn new(new: &Menu) -> Self {
        use winapi::um::winuser::CreateMenu;

        let hash = new.get_hash();
        let mut root = unsafe { CreateMenu() };
        let mut command_map = BTreeMap::new();

        Self::recursive_construct_menu(&mut root, new.items.as_ref(), &mut command_map);

        Self {
            _native_ptr: root,
            callbacks: command_map,
            hash,
        }
    }

    fn get_new_command_id() -> usize {
        WINDOWS_UNIQUE_COMMAND_ID_GENERATOR.fetch_add(1, AtomicOrdering::SeqCst)
    }

    fn recursive_construct_menu(
        menu: &mut HMENU,
        items: &[MenuItem],
        command_map: &mut BTreeMap<u16, MenuCallback>,
    ) {
        fn convert_widestring(input: &str) -> Vec<u16> {
            let mut v: Vec<u16> = input
                .chars()
                .filter_map(|s| {
                    use std::convert::TryInto;
                    (s as u32).try_into().ok()
                })
                .collect();
            v.push(0);
            v
        }

        use winapi::shared::basetsd::UINT_PTR;
        use winapi::um::winuser::{AppendMenuW, CreateMenu};
        use winapi::um::winuser::{MF_MENUBREAK, MF_POPUP, MF_SEPARATOR, MF_STRING};

        for item in items.as_ref() {
            match item {
                MenuItem::String(mi) => {
                    if mi.children.as_ref().is_empty() {
                        // no children
                        let command = match mi.callback.as_ref() {
                            None => 0,
                            Some(c) => {
                                let new_command_id =
                                    Self::get_new_command_id().min(core::u16::MAX as usize) as u16;
                                command_map.insert(new_command_id, c.clone());
                                new_command_id as usize
                            }
                        };
                        unsafe {
                            AppendMenuW(
                                *menu,
                                MF_STRING,
                                command,
                                convert_widestring(mi.label.as_str()).as_ptr(),
                            )
                        };
                    } else {
                        let mut root = unsafe { CreateMenu() };
                        Self::recursive_construct_menu(
                            &mut root,
                            mi.children.as_ref(),
                            command_map,
                        );
                        unsafe {
                            AppendMenuW(
                                *menu,
                                MF_POPUP,
                                root as UINT_PTR,
                                convert_widestring(mi.label.as_str()).as_ptr(),
                            )
                        };
                    }
                }
                MenuItem::Separator => unsafe {
                    AppendMenuW(*menu, MF_SEPARATOR, 0, core::ptr::null_mut());
                },
                MenuItem::BreakLine => unsafe {
                    AppendMenuW(*menu, MF_MENUBREAK, 0, core::ptr::null_mut());
                },
            }
        }
    }
}

unsafe extern "system" fn WindowProc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {

    use winapi::um::winuser::{
        DefWindowProcW, SetWindowLongPtrW,
        GetWindowLongPtrW, PostQuitMessage, PostMessageW,
        WM_NCCREATE, WM_TIMER, WM_COMMAND,
        WM_CREATE, WM_NCMOUSELEAVE, WM_ERASEBKGND,
        WM_MOUSEMOVE, WM_DESTROY, WM_PAINT, WM_ACTIVATE,
        WM_MOUSEWHEEL, WM_SIZE, WM_NCHITTEST,
        WM_LBUTTONDOWN, WM_DPICHANGED, WM_RBUTTONDOWN,
        WM_LBUTTONUP, WM_RBUTTONUP, WM_MOUSELEAVE,
        WM_DISPLAYCHANGE, WM_SIZING, WM_WINDOWPOSCHANGED,
        WM_QUIT, WM_HSCROLL, WM_VSCROLL,
        WM_KEYUP, WM_KEYDOWN, WM_SYSKEYUP, WM_SYSKEYDOWN,
        WM_CHAR, WM_SYSCHAR, WHEEL_DELTA, WM_SETFOCUS, WM_KILLFOCUS,

        VK_F4,
        CREATESTRUCTW, GWLP_USERDATA,
    };
    use winapi::um::wingdi::wglMakeCurrent;
    use crate::wr_translate::wr_translate_document_id;


    return if msg == WM_NCCREATE {
        let createstruct: *mut CREATESTRUCTW = mem::transmute(lparam);
        let data_ptr = (*createstruct).lpCreateParams;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, mem::transmute(data_ptr));
        DefWindowProcW(hwnd, msg, wparam, lparam)
    } else {

        let shared_application_data: *mut SharedApplicationData = mem::transmute(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
        if shared_application_data == ptr::null_mut() {
            // message fired before WM_NCCREATE: ignore
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let shared_application_data: &mut SharedApplicationData = &mut *shared_application_data;

        let mut app_borrow = match shared_application_data.inner.try_borrow_mut() {
            Ok(b) => b,
            Err(e) => { return DefWindowProcW(hwnd, msg, wparam, lparam); },
        };

        let hwnd_key = hwnd as usize;

        match msg {
            AZ_REGENERATE_DOM => {

                use azul_core::window_state::{NodesToCheck, StyleAndLayoutChanges};

                let mut ret = ProcessEventResult::DoNothing;

                // borrow checker :|
                let ab = &mut *app_borrow;
                let windows = &mut ab.windows;
                let fc_cache = &mut ab.fc_cache;
                let data = &mut ab.data;
                let image_cache = &mut ab.image_cache;

                if let Some(current_window) = windows.get_mut(&hwnd_key) {

                    use winapi::um::winuser::{GetDC, ReleaseDC};

                    let hDC = GetDC(hwnd);

                    let gl_context = match current_window.gl_context {
                        Some(c) => {
                            if !hDC.is_null() {
                                wglMakeCurrent(hDC, c);
                            }
                        },
                        None => { },
                    };

                    let mut current_program = [0_i32];

                    {
                        let mut gl = &mut current_window.gl_functions.functions;
                        gl.get_integer_v(gl_context_loader::gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
                    }

                    let document_id = current_window.internal.document_id;
                    let mut hit_tester = &mut current_window.hit_tester;
                    let internal = &mut current_window.internal;
                    let gl_context = &current_window.gl_context_ptr;

                    // unset the focus
                    internal.current_window_state.focused_node = None;

                    let mut resource_updates = Vec::new();
                    fc_cache.apply_closure(|fc_cache| {
                        internal.regenerate_styled_dom(
                            data,
                            image_cache,
                            gl_context,
                            &mut resource_updates,
                            &crate::app::CALLBACKS,
                            fc_cache,
                            azul_layout::do_the_relayout,
                            |window_state, scroll_states, layout_results| {
                                crate::wr_translate::fullhittest_new_webrender(
                                     &*hit_tester.resolve(),
                                     document_id,
                                     window_state.focused_node,
                                     layout_results,
                                     &window_state.mouse_state.cursor_position,
                                     window_state.size.hidpi_factor,
                                )
                            }
                        );
                    });

                    let mut gl = &mut current_window.gl_functions.functions;
                    gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                    gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                    gl.use_program(current_program[0] as u32);

                    wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                    if !hDC.is_null() {
                        ReleaseDC(hwnd, hDC);
                    }

                    current_window.context_menu = None;
                    Window::set_menu_bar(
                        hwnd,
                        &mut current_window.menu_bar,
                        current_window.internal.get_menu_bar()
                    );

                    // rebuild the display list and send it
                    rebuild_display_list(
                        &mut current_window.internal,
                        &mut current_window.render_api,
                        image_cache,
                        resource_updates,
                    );

                    current_window.render_api.flush_scene_builder();

                    let wr_document_id = wr_translate_document_id(current_window.internal.document_id);
                    current_window.hit_tester = AsyncHitTester::Requested(
                        current_window.render_api.request_hit_tester(wr_document_id)
                    );

                    let hit_test = crate::wr_translate::fullhittest_new_webrender(
                        &*current_window.hit_tester.resolve(),
                        current_window.internal.document_id,
                        current_window.internal.current_window_state.focused_node,
                        &current_window.internal.layout_results,
                        &current_window.internal.current_window_state.mouse_state.cursor_position,
                        current_window.internal.current_window_state.size.hidpi_factor,
                    );

                    current_window.internal.previous_window_state = None;
                    current_window.internal.current_window_state.last_hit_test = hit_test;

                    let mut nodes_to_check = NodesToCheck::simulated_mouse_move(
                        &current_window.internal.current_window_state.last_hit_test,
                        current_window.internal.current_window_state.focused_node,
                        current_window.internal.current_window_state.mouse_state.mouse_down()
                    );

                    let mut style_layout_changes = StyleAndLayoutChanges::new(
                        &nodes_to_check,
                        &mut current_window.internal.layout_results,
                        &image_cache,
                        &mut current_window.internal.renderer_resources,
                        current_window.internal.current_window_state.size.get_layout_size(),
                        &current_window.internal.document_id,
                        None,
                        None,
                        &None,
                        azul_layout::do_the_relayout,
                    );

                    PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                }

                mem::drop(app_borrow);
                return 0;
            },
            AZ_REDO_HIT_TEST => {

                let mut ret = ProcessEventResult::DoNothing;

                let cur_hwnd;

                let ab = &mut *app_borrow;
                let windows = &mut ab.windows;
                let fc_cache = &mut ab.fc_cache;
                let image_cache = &mut ab.image_cache;
                let config = &ab.config;
                let hinstance = ab.hinstance;

                let mut new_windows = Vec::new();
                let mut destroyed_windows = Vec::new();

                match windows.get_mut(&hwnd_key) {
                    Some(current_window) => {

                        use winapi::um::winuser::{GetDC, ReleaseDC};

                        cur_hwnd = current_window.hwnd;

                        let hDC = GetDC(cur_hwnd);

                        let gl_context = match current_window.gl_context {
                            Some(c) => {
                                if !hDC.is_null() {
                                    wglMakeCurrent(hDC, c);
                                }
                            },
                            None => { },
                        };

                        let mut current_program = [0_i32];

                        {
                            let mut gl = &mut current_window.gl_functions.functions;
                            gl.get_integer_v(gl_context_loader::gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
                        }

                        ret = process_event(
                            hinstance,
                            current_window,
                            fc_cache,
                            image_cache,
                            config,
                            &mut new_windows,
                            &mut destroyed_windows,
                        );

                        let mut gl = &mut current_window.gl_functions.functions;
                        gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                        gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                        gl.use_program(current_program[0] as u32);

                        wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                        if !hDC.is_null() {
                            ReleaseDC(cur_hwnd, hDC);
                        }
                    },
                    None => {
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    },
                };

                let hinstance = ab.hinstance;
                mem::drop(ab);
                mem::drop(app_borrow);
                create_windows(hinstance, shared_application_data, new_windows);
                let mut app_borrow = shared_application_data.inner.try_borrow_mut().unwrap();
                let mut ab = &mut *app_borrow;
                destroy_windows(ab, destroyed_windows);

                mem::drop(ab);

                match ret {
                    ProcessEventResult::DoNothing => { },
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                        PostMessageW(cur_hwnd, AZ_REGENERATE_DOM, 0, 0);
                    },
                    ProcessEventResult::ShouldRegenerateDomAllWindows => {
                        for window in app_borrow.windows.values() {
                            PostMessageW(window.hwnd, AZ_REGENERATE_DOM, 0, 0);
                        }
                    },
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                        PostMessageW(cur_hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                    },
                    ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                        if let Some(w) = app_borrow.windows.get_mut(&hwnd_key) {
                            // TODO: submit display list, wait for new hit-tester and update hit-test results
                            w.internal.previous_window_state = Some(w.internal.current_window_state.clone());
                            PostMessageW(cur_hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                            PostMessageW(cur_hwnd, AZ_REDO_HIT_TEST, 0, 0);
                        }
                    },
                    ProcessEventResult::ShouldReRenderCurrentWindow => {
                        PostMessageW(cur_hwnd, AZ_GPU_SCROLL_RENDER, 0, 0);
                    },
                }

                mem::drop(app_borrow);
                return 0;
            },
            AZ_REGENERATE_DISPLAY_LIST => {

                use winapi::um::winuser::InvalidateRect;

                let ab = &mut *app_borrow;
                let image_cache = &ab.image_cache;
                let windows = &mut ab.windows;

                if let Some(current_window) =  windows.get_mut(&hwnd_key) {

                    rebuild_display_list(
                        &mut current_window.internal,
                        &mut current_window.render_api,
                        image_cache,
                        Vec::new(), // no resource updates
                    );

                    let wr_document_id = wr_translate_document_id(current_window.internal.document_id);
                    current_window.hit_tester = AsyncHitTester::Requested(
                        current_window.render_api.request_hit_tester(wr_document_id)
                    );

                    generate_frame(
                        &mut current_window.internal,
                        &mut current_window.render_api,
                        true,
                    );

                    InvalidateRect(current_window.hwnd, ptr::null_mut(), 0);
                    mem::drop(app_borrow);
                    return 0;
                } else {
                    mem::drop(app_borrow);
                    return -1;
                }
            },
            AZ_GPU_SCROLL_RENDER => {

                use winapi::um::winuser::InvalidateRect;

                match app_borrow.windows.get_mut(&hwnd_key) {
                    Some(current_window) => {
                        generate_frame(
                            &mut current_window.internal,
                            &mut current_window.render_api,
                            false,
                        );

                        InvalidateRect(current_window.hwnd, ptr::null_mut(), 0);
                    },
                    None => { },
                }

                mem::drop(app_borrow);
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            },
            WM_CREATE => {
                if let Ok(mut o) = app_borrow.active_hwnds.try_borrow_mut() {
                    o.insert(hwnd);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_ACTIVATE => {
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_ERASEBKGND => {
                mem::drop(app_borrow);
                return 1;
            },
            WM_SETFOCUS => {
                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    current_window.internal.previous_window_state = Some(current_window.internal.current_window_state.clone());
                    current_window.internal.current_window_state.flags.has_focus = true;
                    PostMessageW(current_window.hwnd, AZ_REDO_HIT_TEST, 0, 0);
                    mem::drop(app_borrow);
                    return 0;
                }
                mem::drop(app_borrow);
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            },
            WM_KILLFOCUS => {
                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    current_window.internal.previous_window_state = Some(current_window.internal.current_window_state.clone());
                    current_window.internal.current_window_state.flags.has_focus = false;
                    PostMessageW(current_window.hwnd, AZ_REDO_HIT_TEST, 0, 0);
                    mem::drop(app_borrow);
                    return 0;
                }
                mem::drop(app_borrow);
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            },
            WM_MOUSEMOVE => {

                use winapi::{
                    um::winuser::{
                        SetClassLongPtrW, TrackMouseEvent,
                        TME_LEAVE, HOVER_DEFAULT, TRACKMOUSEEVENT,
                        GCLP_HCURSOR
                    },
                    shared::windowsx::{GET_X_LPARAM, GET_Y_LPARAM}
                };
                use azul_core::window::{
                    CursorTypeHitTest, LogicalPosition,
                    CursorPosition, OptionMouseCursorType,
                    FullHitTest,
                };

                let x = GET_X_LPARAM(lparam);
                let y = GET_Y_LPARAM(lparam);

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {

                    let pos = CursorPosition::InWindow(LogicalPosition::new(
                        x as f32 / current_window.internal.current_window_state.size.hidpi_factor,
                        y as f32 / current_window.internal.current_window_state.size.hidpi_factor,
                    ));

                    // call SetCapture(hwnd) so that we can capture the WM_MOUSELEAVE event
                    let cur_cursor_pos = current_window.internal.current_window_state.mouse_state.cursor_position;
                    let prev_cursor_pos = current_window.internal.previous_window_state
                        .as_ref().map(|m| m.mouse_state.cursor_position).unwrap_or_default();

                    if !prev_cursor_pos.is_inside_window() && cur_cursor_pos.is_inside_window() {
                        // cursor entered
                        TrackMouseEvent(&mut TRACKMOUSEEVENT {
                            cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: current_window.hwnd,
                            dwHoverTime: HOVER_DEFAULT,
                        });
                    }

                    let previous_state = current_window.internal.current_window_state.clone();
                    current_window.internal.previous_window_state = Some(previous_state);
                    current_window.internal.current_window_state.mouse_state.cursor_position = pos;

                    // mouse moved, so we need a new hit test
                    let hit_test = crate::wr_translate::fullhittest_new_webrender(
                        &*current_window.hit_tester.resolve(),
                        current_window.internal.document_id,
                        current_window.internal.current_window_state.focused_node,
                        &current_window.internal.layout_results,
                        &current_window.internal.current_window_state.mouse_state.cursor_position,
                        current_window.internal.current_window_state.size.hidpi_factor,
                    );
                    let cht = CursorTypeHitTest::new(&hit_test, &current_window.internal.layout_results);
                    current_window.internal.current_window_state.last_hit_test = hit_test;

                    // update the cursor if necessary
                    if current_window.internal.current_window_state.mouse_state.mouse_cursor_type != OptionMouseCursorType::Some(cht.cursor_icon) {
                        // TODO: unset previous cursor?
                        current_window.internal.current_window_state.mouse_state.mouse_cursor_type = OptionMouseCursorType::Some(cht.cursor_icon);
                        SetClassLongPtrW(current_window.hwnd, GCLP_HCURSOR, win32_translate_cursor(cht.cursor_icon) as isize);
                    }

                    PostMessageW(current_window.hwnd, AZ_REDO_HIT_TEST, 0, 0);
                };

                mem::drop(app_borrow);
                return 0;
            },
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if msg == WM_SYSKEYDOWN && wparam as i32 == VK_F4 {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                } else {
                    if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                        if let Some((scancode, vk)) = event::process_key_params(wparam, lparam) {
                            use winapi::um::winuser::SendMessageW;

                            current_window.internal.previous_window_state = Some(current_window.internal.current_window_state.clone());
                            current_window.internal.current_window_state.keyboard_state.current_char = None.into();
                            current_window.internal.current_window_state.keyboard_state.pressed_scancodes.insert_hm_item(scancode);
                            if let Some(vk) = vk {
                                current_window.internal.current_window_state.keyboard_state.current_virtual_keycode = Some(vk).into();
                                current_window.internal.current_window_state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
                            }
                            mem::drop(app_borrow);

                            // NOTE: due to a Win32 bug, the WM_CHAR message gets sent immediately after
                            // the WM_KEYDOWN: this would mess with the event handling in the window state
                            // code (the window state code expects events to arrive in logical order)
                            //
                            // So here we use SendMessage instead of PostMessage in order to immediately
                            // call AZ_REDO_HIT_TEST (instead of posting to the windows message queue).
                            SendMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                            return 0;
                        }
                    }
                }

                mem::drop(app_borrow);
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            },
            WM_CHAR | WM_SYSCHAR => {

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {

                    use std::char;

                    let is_high_surrogate = 0xD800 <= wparam && wparam <= 0xDBFF;
                    let is_low_surrogate = 0xDC00 <= wparam && wparam <= 0xDFFF;

                    let mut c = None; // character
                    if is_high_surrogate {
                        current_window.high_surrogate = Some(wparam as u16);
                    } else if is_low_surrogate {
                        if let Some(high_surrogate) = current_window.high_surrogate {
                            let pair = [high_surrogate, wparam as u16];
                            if let Some(Ok(chr)) = char::decode_utf16(pair.iter().copied()).next() {
                                c = Some(chr);
                            }
                        }
                    } else {
                        current_window.high_surrogate = None;
                        if let Some(chr) = char::from_u32(wparam as u32) {
                            c = Some(chr);
                        }
                    }

                    if let Some(c) = c {
                        if !c.is_control() {
                            current_window.internal.previous_window_state = Some(current_window.internal.current_window_state.clone());
                            current_window.internal.current_window_state.keyboard_state.current_char = Some(c as u32).into();
                            PostMessageW(current_window.hwnd, AZ_REDO_HIT_TEST, 0, 0);
                            mem::drop(app_borrow);
                            return 0;
                        }
                    }
                }

                mem::drop(app_borrow);
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            },
            WM_KEYUP | WM_SYSKEYUP => {
                use self::event::process_key_params;
                if let Some((scancode, vk)) = process_key_params(wparam, lparam) {
                    if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                        current_window.internal.previous_window_state = Some(current_window.internal.current_window_state.clone());
                        current_window.internal.current_window_state.keyboard_state.current_char = None.into();
                        current_window.internal.current_window_state.keyboard_state.pressed_scancodes.remove_hm_item(&scancode);
                        if let Some(vk) = vk {
                            current_window.internal.current_window_state.keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
                            current_window.internal.current_window_state.keyboard_state.current_virtual_keycode = None.into();
                        }
                        PostMessageW(current_window.hwnd, AZ_REDO_HIT_TEST, 0, 0);
                        mem::drop(app_borrow);
                        return 0;
                    }
                }
                mem::drop(app_borrow);
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            },
            WM_MOUSELEAVE => {

                use winapi::um::winuser::{SetClassLongPtrW, GCLP_HCURSOR};
                use azul_core::window::{
                    FullHitTest, OptionMouseCursorType, CursorPosition,
                };

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {

                    let current_focus = current_window.internal.current_window_state.focused_node;
                    let previous_state = current_window.internal.current_window_state.clone();
                    current_window.internal.previous_window_state = Some(previous_state);
                    current_window.internal.current_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow;
                    current_window.internal.current_window_state.last_hit_test = FullHitTest::empty(current_focus);
                    current_window.internal.current_window_state.mouse_state.mouse_cursor_type = OptionMouseCursorType::None;

                    SetClassLongPtrW(hwnd, GCLP_HCURSOR, win32_translate_cursor(MouseCursorType::Default) as isize);
                    PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                    mem::drop(app_borrow);
                    return 0;
                } else {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
            },
            WM_RBUTTONDOWN => {
                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let previous_state = current_window.internal.current_window_state.clone();
                    current_window.internal.previous_window_state = Some(previous_state);
                    current_window.internal.current_window_state.mouse_state.right_down = true;
                    PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_RBUTTONUP => {
                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let previous_state = current_window.internal.current_window_state.clone();
                    current_window.internal.previous_window_state = Some(previous_state);

                    // open context menu
                    if let Some((context_menu, hit, node_id)) = current_window.internal.get_context_menu() {

                        use winapi::um::winuser::{
                            CreatePopupMenu, TrackPopupMenu, SetForegroundWindow,
                            GetClientRect, ClientToScreen,
                            TPM_TOPALIGN, TPM_LEFTALIGN,
                        };

                        let mut hPopupMenu = CreatePopupMenu();
                        let mut callbacks = BTreeMap::new();

                        WindowsMenuBar::recursive_construct_menu(
                            &mut hPopupMenu,
                            &context_menu.items.as_ref(),
                            &mut callbacks,
                        );

                        let align = match context_menu.position {
                            _ => TPM_TOPALIGN | TPM_LEFTALIGN, // TODO
                        };

                        // get the current top left edge of the window rect
                        let mut rect: RECT = unsafe { mem::zeroed() };
                        GetClientRect(hwnd, &mut rect);

                        let mut top_left = POINT { x: rect.left, y: rect.top };
                        ClientToScreen(hwnd, &mut top_left);

                        let pos = match context_menu.position {
                            _ => hit.point_in_viewport, // TODO
                        };

                        current_window.context_menu = Some(CurrentContextMenu {
                            callbacks,
                            hit_dom_node: node_id,
                        });

                        SetForegroundWindow(hwnd);
                        TrackPopupMenu(
                            hPopupMenu,
                            align,
                            top_left.x + (libm::roundf(pos.x) as i32),
                            top_left.y + (libm::roundf(pos.y) as i32),
                            0,
                            hwnd,
                            ptr::null_mut()
                        );
                    }

                    current_window.internal.current_window_state.mouse_state.right_down = false;
                    PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_LBUTTONDOWN => {
                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let previous_state = current_window.internal.current_window_state.clone();
                    current_window.internal.previous_window_state = Some(previous_state);
                    current_window.internal.current_window_state.mouse_state.left_down = true;
                    PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_LBUTTONUP => {
                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let previous_state = current_window.internal.current_window_state.clone();
                    current_window.internal.previous_window_state = Some(previous_state);

                    // open context menu
                    if let Some((context_menu, hit, node_id)) = current_window.internal.get_context_menu() {

                        use winapi::um::winuser::{
                            CreatePopupMenu, TrackPopupMenu, SetForegroundWindow,
                            GetClientRect, ClientToScreen,
                            TPM_TOPALIGN, TPM_LEFTALIGN,
                        };

                        let mut hPopupMenu = CreatePopupMenu();
                        let mut callbacks = BTreeMap::new();

                        WindowsMenuBar::recursive_construct_menu(
                            &mut hPopupMenu,
                            &context_menu.items.as_ref(),
                            &mut callbacks,
                        );

                        let align = match context_menu.position {
                            _ => TPM_TOPALIGN | TPM_LEFTALIGN, // TODO
                        };

                        // get the current top left edge of the window rect
                        let mut rect: RECT = unsafe { mem::zeroed() };
                        GetClientRect(hwnd, &mut rect);

                        let mut top_left = POINT { x: rect.left, y: rect.top };
                        ClientToScreen(hwnd, &mut top_left);

                        let pos = match context_menu.position {
                            _ => hit.point_in_viewport, // TODO
                        };

                        current_window.context_menu = Some(CurrentContextMenu {
                            callbacks,
                            hit_dom_node: node_id,
                        });

                        SetForegroundWindow(hwnd);
                        TrackPopupMenu(
                            hPopupMenu,
                            align,
                            top_left.x + (libm::roundf(pos.x) as i32),
                            top_left.y + (libm::roundf(pos.y) as i32),
                            0,
                            hwnd,
                            ptr::null_mut()
                        );
                    }

                    current_window.internal.current_window_state.mouse_state.left_down = false;
                    PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_MOUSEWHEEL => {
                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let value = (wparam >> 16) as i16;
                    let value = value as i32;
                    let value = value as f32 / WHEEL_DELTA as f32;
                    let previous_state = current_window.internal.current_window_state.clone();
                    current_window.internal.previous_window_state = Some(previous_state);
                    current_window.internal.current_window_state.mouse_state.scroll_y = Some(value).into();
                    PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                    mem::drop(app_borrow);
                    return 0;
                } else {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
            },
            WM_DPICHANGED => {
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_SIZE => {
                use azul_core::window::{WindowFrame, PhysicalSize};
                use winapi::um::winuser::{
                    WINDOWPOS, SWP_NOSIZE, SIZE_MAXIMIZED,
                    SIZE_RESTORED, SIZE_MINIMIZED
                };
                use winapi::shared::minwindef::{LOWORD, HIWORD};

                let new_width = LOWORD(lparam as u32);
                let new_height = HIWORD(lparam as u32);
                let new_size = PhysicalSize {
                    width: new_width as u32,
                    height: new_height as u32
                };

                let mut ab = &mut *app_borrow;
                let fc_cache = &mut ab.fc_cache;
                let windows = &mut ab.windows;
                let image_cache = &ab.image_cache;

                if let Some(current_window) = windows.get_mut(&hwnd_key) {
                    fc_cache.apply_closure(|fc_cache| {

                        use winapi::um::winuser::{GetDC, ReleaseDC};

                        let mut new_window_state = current_window.internal.current_window_state.clone();
                        new_window_state.size.dimensions = new_size.to_logical(new_window_state.size.hidpi_factor);

                        match wparam {
                            SIZE_MAXIMIZED => {
                                new_window_state.flags.frame = WindowFrame::Maximized;
                            },
                            SIZE_MINIMIZED => {
                                new_window_state.flags.frame = WindowFrame::Minimized;
                            },
                            SIZE_RESTORED => {
                                new_window_state.flags.frame = WindowFrame::Normal;
                            },
                            _ => { }
                        }

                        let hDC = GetDC(hwnd);

                        let gl_context = match current_window.gl_context {
                            Some(c) => {
                                if !hDC.is_null() {
                                    wglMakeCurrent(hDC, c);
                                }
                            },
                            None => { },
                        };

                        let mut current_program = [0_i32];

                        {
                            let mut gl = &mut current_window.gl_functions.functions;
                            gl.get_integer_v(gl_context_loader::gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
                        }

                        let resize_result = current_window.internal.do_quick_resize(
                            &image_cache,
                            &crate::app::CALLBACKS,
                            azul_layout::do_the_relayout,
                            fc_cache,
                            &current_window.gl_context_ptr,
                            &new_window_state.size,
                            new_window_state.theme,
                        );

                        let mut txn = WrTransaction::new();
                        wr_synchronize_resize(
                            resize_result,
                            &current_window.internal.document_id,
                            &mut txn
                        );

                        let mut gl = &mut current_window.gl_functions.functions;
                        gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                        gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                        gl.use_program(current_program[0] as u32);

                        wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                        if !hDC.is_null() {
                            ReleaseDC(hwnd, hDC);
                        }

                        current_window.internal.previous_window_state = Some(current_window.internal.current_window_state.clone());
                        current_window.internal.current_window_state = new_window_state;

                        txn.set_document_view(
                            WrDeviceIntRect::from_size(
                                WrDeviceIntSize::new(new_width as i32, new_height as i32),
                            )
                        );
                        current_window.render_api.send_transaction(wr_translate_document_id(current_window.internal.document_id), txn);

                        rebuild_display_list(
                            &mut current_window.internal,
                            &mut current_window.render_api,
                            image_cache,
                            Vec::new(),
                        );

                        let wr_document_id = wr_translate_document_id(current_window.internal.document_id);
                        current_window.hit_tester = AsyncHitTester::Requested(
                            current_window.render_api.request_hit_tester(wr_document_id)
                        );

                        generate_frame(
                            &mut current_window.internal,
                            &mut current_window.render_api,
                            true,
                        );
                    });

                    mem::drop(app_borrow);
                    return 0;
                } else {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
            },
            WM_NCHITTEST => {
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_PAINT => {

                use winapi::um::{
                    wingdi::SwapBuffers,
                    winuser::{GetDC, ReleaseDC, GetClientRect},
                };

                // Assuming that the display list has been submitted and the
                // scene on the background thread has been rebuilt, now tell
                // webrender to pain the scene

                let hDC = GetDC(hwnd);
                if hDC.is_null() {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }

                let mut app = &mut *app_borrow;
                let mut current_window = match app.windows.get_mut(&hwnd_key) {
                    Some(s) => s,
                    None => {
                        // message fired before window was created: ignore
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam)
                    },
                };

                let gl_context = match current_window.gl_context {
                    Some(s) => s,
                    None => {
                        // TODO: software rendering
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    },
                };

                wglMakeCurrent(hDC, gl_context);

                let mut rect: RECT = mem::zeroed();
                GetClientRect(hwnd, &mut rect);

                // Block until all transactions (display list build)
                // have finished processing
                //
                // Usually this shouldn't take too long, since DL building
                // happens asynchronously between WM_SIZE and WM_PAINT
                current_window.render_api.flush_scene_builder();

                let mut gl = &mut current_window.gl_functions.functions;

                gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                gl.disable(gl_context_loader::gl::FRAMEBUFFER_SRGB);
                gl.disable(gl_context_loader::gl::MULTISAMPLE);
                gl.viewport(0, 0, rect.width() as i32, rect.height() as i32);

                let mut current_program = [0_i32];
                gl.get_integer_v(gl_context_loader::gl::CURRENT_PROGRAM, (&mut current_program[..]).into());

                let framebuffer_size = WrDeviceIntSize::new(
                    rect.width() as i32,
                    rect.height() as i32
                );

                // Render
                if let Some(r) = current_window.renderer.as_mut() {
                    r.update();
                    let _ = r.render(framebuffer_size, 0);
                }

                SwapBuffers(hDC);

                gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                gl.use_program(current_program[0] as u32);

                wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                ReleaseDC(hwnd, hDC);
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_TIMER => {

                use winapi::um::winuser::{GetDC, ReleaseDC};

                let mut ab = &mut *app_borrow;
                let hinstance = ab.hinstance;
                let windows = &mut ab.windows;
                let data = &mut ab.data;
                let image_cache = &mut ab.image_cache;
                let fc_cache = &mut ab.fc_cache;
                let config = &ab.config;

                let mut ret = ProcessEventResult::DoNothing;
                let mut new_windows = Vec::new();
                let mut destroyed_windows = Vec::new();

                match wparam {
                    AZ_TICK_REGENERATE_DOM => {
                        // re-load the layout() callback
                        PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    },
                    AZ_THREAD_TICK => {

                        // tick every 16ms to process new thread messages
                        match windows.get_mut(&hwnd_key) {
                            Some(current_window) => {

                                let hDC = GetDC(hwnd);

                                let gl_context = match current_window.gl_context {
                                    Some(c) => {
                                        if !hDC.is_null() {
                                            wglMakeCurrent(hDC, c);
                                        }
                                    },
                                    None => { },
                                };

                                let mut current_program = [0_i32];

                                {
                                    let mut gl = &mut current_window.gl_functions.functions;
                                    gl.get_integer_v(gl_context_loader::gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
                                }

                                ret = process_threads(
                                    hinstance,
                                    data,
                                    current_window,
                                    fc_cache,
                                    image_cache,
                                    config,
                                    &mut new_windows,
                                    &mut destroyed_windows,
                                );

                                let mut gl = &mut current_window.gl_functions.functions;
                                gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                                gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                                gl.use_program(current_program[0] as u32);

                                wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                                if !hDC.is_null() {
                                    ReleaseDC(hwnd, hDC);
                                }
                            },
                            None => {
                                mem::drop(app_borrow);
                                return DefWindowProcW(hwnd, msg, wparam, lparam);
                            },
                        }
                    },
                    id => { // run timer with ID "id"
                        match windows.get_mut(&hwnd_key) {
                            Some(current_window) => {

                                let hDC = GetDC(hwnd);

                                let gl_context = match current_window.gl_context {
                                    Some(c) => {
                                        if !hDC.is_null() {
                                            wglMakeCurrent(hDC, c);
                                        }
                                    },
                                    None => { },
                                };

                                let mut current_program = [0_i32];

                                {
                                    let mut gl = &mut current_window.gl_functions.functions;
                                    gl.get_integer_v(gl_context_loader::gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
                                }

                                ret = process_timer(
                                    id,
                                    hinstance,
                                    current_window,
                                    fc_cache,
                                    image_cache,
                                    config,
                                    &mut new_windows,
                                    &mut destroyed_windows,
                                );

                                let mut gl = &mut current_window.gl_functions.functions;
                                gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                                gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                                gl.use_program(current_program[0] as u32);

                                wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                                if !hDC.is_null() {
                                    ReleaseDC(hwnd, hDC);
                                }
                            },
                            None => {
                                mem::drop(app_borrow);
                                return DefWindowProcW(hwnd, msg, wparam, lparam);
                            },
                        }
                    }
                }

                // create_windows needs to clone the SharedApplicationData RefCell
                // drop the borrowed variables and restore them immediately after
                let hinstance = ab.hinstance;
                mem::drop(ab);
                mem::drop(app_borrow);
                create_windows(hinstance, shared_application_data, new_windows);
                let mut app_borrow = shared_application_data.inner.try_borrow_mut().unwrap();
                let mut ab = &mut *app_borrow;
                destroy_windows(ab, destroyed_windows);

                match ret {
                    ProcessEventResult::DoNothing => { },
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                        PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
                    },
                    ProcessEventResult::ShouldRegenerateDomAllWindows => {
                        for window in ab.windows.values() {
                            PostMessageW(window.hwnd, AZ_REGENERATE_DOM, 0, 0);
                        }
                    },
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                        PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                    },
                    ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                        if let Some(w) = ab.windows.get_mut(&hwnd_key) {
                            w.internal.previous_window_state = Some(w.internal.current_window_state.clone());
                            // TODO: submit display list, wait for new hit-tester and update hit-test results
                            PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                            PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                        }
                    },
                    ProcessEventResult::ShouldReRenderCurrentWindow => {
                        PostMessageW(hwnd, AZ_GPU_SCROLL_RENDER, 0, 0);
                    },
                }

                mem::drop(ab);
                mem::drop(app_borrow);
                return 0;
            },
            WM_COMMAND => {

                use winapi::shared::minwindef::{HIWORD, LOWORD};

                let hiword = HIWORD(wparam.min(core::u32::MAX as usize) as u32);
                let loword = LOWORD(wparam.min(core::u32::MAX as usize) as u32);

                // assert that the command came from a menu
                if hiword != 0 {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }

                let mut ab = &mut *app_borrow;
                let hinstance = ab.hinstance;
                let windows = &mut ab.windows;
                let data = &mut ab.data;
                let image_cache = &mut ab.image_cache;
                let fc_cache = &mut ab.fc_cache;
                let config = &ab.config;

                // execute menu callback
                if let Some(current_window) = windows.get_mut(&hwnd_key) {

                    use azul_core::window::{RawWindowHandle, WindowsHandle};
                    use azul_core::styled_dom::NodeHierarchyItemId;

                    let mut ret = ProcessEventResult::DoNothing;
                    let mut new_windows = Vec::new();
                    let mut destroyed_windows = Vec::new();

                    let window_handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut _,
                        hinstance: hinstance as *mut _,
                    });

                    let ntc = NodesToCheck::empty(
                        current_window.internal.current_window_state.mouse_state.mouse_down(),
                        current_window.internal.current_window_state.focused_node,
                    );

                    let call_callback_result = {

                        let mb = &mut current_window.menu_bar;
                        let internal = &mut current_window.internal;
                        let context_menu = current_window.context_menu.as_mut();
                        let gl_context_ptr = &current_window.gl_context_ptr;

                        if let Some(menu_callback) = mb.as_mut().and_then(|m| m.callbacks.get_mut(&loword)) {
                            Some(fc_cache.apply_closure(|fc_cache| {
                                internal.invoke_menu_callback(
                                    menu_callback,
                                    DomNodeId {
                                        dom: DomId::ROOT_ID,
                                        node: NodeHierarchyItemId::from_crate_internal(None),
                                    },
                                    &window_handle,
                                    &gl_context_ptr,
                                    image_cache,
                                    fc_cache,
                                    &config.system_callbacks,
                                )
                            }))
                        } else if let Some(context_menu) = context_menu {
                            let hit_dom_node = context_menu.hit_dom_node;
                            if let Some(menu_callback) = context_menu.callbacks.get_mut(&loword) {
                                Some(fc_cache.apply_closure(|fc_cache| {
                                    internal.invoke_menu_callback(
                                        menu_callback,
                                        hit_dom_node,
                                        &window_handle,
                                        &gl_context_ptr,
                                        image_cache,
                                        fc_cache,
                                        &config.system_callbacks,
                                    )
                                }))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    if let Some(ccr) = call_callback_result {
                        ret = process_callback_results(
                            ccr,
                            current_window,
                            &ntc,
                            image_cache,
                            &mut new_windows,
                            &mut destroyed_windows,
                        );
                    };

                    // same as invoke_timers(), invoke_threads(), ...

                    mem::drop(ab);
                    mem::drop(app_borrow);
                    create_windows(hinstance, shared_application_data, new_windows);
                    let mut app_borrow = shared_application_data.inner.try_borrow_mut().unwrap();
                    let mut ab = &mut *app_borrow;
                    destroy_windows(ab, destroyed_windows);

                    match ret {
                        ProcessEventResult::DoNothing => { },
                        ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                            PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
                        },
                        ProcessEventResult::ShouldRegenerateDomAllWindows => {
                            for window in app_borrow.windows.values() {
                                PostMessageW(window.hwnd, AZ_REGENERATE_DOM, 0, 0);
                            }
                        },
                        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                            PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                        },
                        ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                            if let Some(w) = app_borrow.windows.get_mut(&hwnd_key) {
                                w.internal.previous_window_state = Some(w.internal.current_window_state.clone());
                                // TODO: submit display list, wait for new hit-tester and update hit-test results
                                PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                                PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                            }
                        },
                        ProcessEventResult::ShouldReRenderCurrentWindow => {
                            PostMessageW(hwnd, AZ_GPU_SCROLL_RENDER, 0, 0);
                        },
                    }

                    mem::drop(app_borrow);
                    return 0;
                } else {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
            },
            WM_QUIT => {
                // TODO: execute quit callback
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            WM_DESTROY => {

                use winapi::um::winuser::{GetDC, ReleaseDC};

                // make OpenGL context current in case there are
                // OpenGL objects stored in the windows' RefAny data

                let mut ab = &mut *app_borrow;
                let mut windows_is_empty = false;

                if let Ok(mut o) = ab.active_hwnds.try_borrow_mut() {
                    o.remove(&hwnd);
                    windows_is_empty = o.is_empty();
                }

                if let Some(mut current_window) = ab.windows.remove(&(hwnd as usize)) {

                    let hDC = GetDC(hwnd);
                    if let Some(c) = current_window.gl_context {
                        if !hDC.is_null() {
                            wglMakeCurrent(hDC, c);
                        }
                    }

                    // destruct the window data
                    let mut window_data = Box::from_raw(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SharedApplicationData);

                    // if this window was the last window, the RefAny data
                    // should be dropped here, while the OpenGL context
                    // is still current!
                    mem::drop(window_data);
                    if let Some(c) = current_window.gl_context.as_mut() {
                        if !hDC.is_null() {
                            wglMakeCurrent(hDC, *c);
                        }
                    }

                    mem::drop(ab);
                    if let Some(c) = current_window.gl_context.as_mut() {
                        if !hDC.is_null() {
                            wglMakeCurrent(hDC, *c);
                        }
                    }

                    mem::drop(app_borrow);
                    if let Some(c) = current_window.gl_context.as_mut() {
                        if !hDC.is_null() {
                            wglMakeCurrent(hDC, *c);
                        }
                    }

                    mem::drop(current_window);

                    wglMakeCurrent(ptr::null_mut(), ptr::null_mut());

                    if !hDC.is_null() {
                        ReleaseDC(hwnd, hDC);
                    }
                }

                if windows_is_empty {
                    PostQuitMessage(0);
                }

                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
            _ => {
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
    };
}

mod event {
    include!("./win32-event.rs");
}

#[derive(Debug, PartialEq, Eq)]
enum ProcessEventResult {
    DoNothing,
    ShouldRegenerateDomCurrentWindow,
    ShouldRegenerateDomAllWindows,
    ShouldUpdateDisplayListCurrentWindow,
    // GPU transforms changed: do another hit-test and recurse
    // until nothing has changed anymore
    UpdateHitTesterAndProcessAgain,
    // Only refresh the display (in case of pure scroll or GPU-only events)
    ShouldReRenderCurrentWindow,
}

// Assuming that current_window_state and the previous_window_state of the window
// are set correctly and the hit-test has been performed, will call the callbacks
// and return what the application should do next
#[must_use]
fn process_event(
    hinstance: HINSTANCE,
    window: &mut Window,
    fc_cache: &mut LazyFcCache,
    image_cache: &mut ImageCache,
    config: &AppConfig,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<usize>,
) -> ProcessEventResult {

    use azul_core::window_state::{
        Events, NodesToCheck, CallbacksOfHitTest,
        StyleAndLayoutChanges,
    };
    use azul_core::window::FullWindowState;
    use azul_core::callbacks::Update;

    // TODO:
    // window.internal.current_window_state.monitor =
    // win32_translate_monitor(MonitorFromWindow(window.hwnd, MONITOR_DEFAULTTONEAREST));

    // Get events
    let events = Events::new(
        &window.internal.current_window_state,
        &window.internal.previous_window_state,
    );

    // Get nodes for events
    let nodes_to_check = NodesToCheck::new(
        &window.internal.current_window_state.last_hit_test,
        &events
    );

    // Invoke callbacks on nodes
    let callback_result = fc_cache.apply_closure(|fc_cache| {

        use azul_core::window::{RawWindowHandle, WindowsHandle};

        // Get callbacks for nodes
        let mut callbacks = CallbacksOfHitTest::new(&nodes_to_check, &events, &window.internal.layout_results);

        let window_handle = RawWindowHandle::Windows(WindowsHandle {
            hwnd: window.hwnd as *mut _,
            hinstance: hinstance as *mut _,
        });
        let current_scroll_states = window.internal.get_current_scroll_states();

        // Invoke user-defined callbacks in the UI
        callbacks.call(
            &window.internal.previous_window_state,
            &window.internal.current_window_state,
            &window_handle,
            &current_scroll_states,
            &window.gl_context_ptr,
            &mut window.internal.layout_results,
            &mut window.internal.scroll_states,
            image_cache,
            fc_cache,
            &config.system_callbacks,
            &window.internal.renderer_resources,
        )
    });

    return process_callback_results(
        callback_result,
        window,
        &nodes_to_check,
        image_cache,
        new_windows,
        destroyed_windows
    );
}

#[must_use]
fn process_timer(
    timer_id: usize,
    hinstance: HINSTANCE,
    window: &mut Window,
    fc_cache: &mut LazyFcCache,
    image_cache: &mut ImageCache,
    config: &AppConfig,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<usize>
) -> ProcessEventResult {

    use azul_core::window::{RawWindowHandle, WindowsHandle};

    let callback_result = fc_cache.apply_closure(|fc_cache| {

        let window_handle = RawWindowHandle::Windows(WindowsHandle {
            hwnd: window.hwnd as *mut _,
            hinstance: hinstance as *mut _,
        });

        let frame_start = (config.system_callbacks.get_system_time_fn.cb)();
        window.internal.run_single_timer(
            timer_id,
            frame_start,
            &window_handle,
            &window.gl_context_ptr,
            image_cache,
            fc_cache,
            &config.system_callbacks,
        )
    });

    return process_callback_results(
        callback_result,
        window,
        &NodesToCheck::empty(
            window.internal.current_window_state.mouse_state.mouse_down(),
            window.internal.current_window_state.focused_node,
        ),
        image_cache,
        new_windows,
        destroyed_windows
    );
}

#[must_use]
fn process_threads(
    hinstance: HINSTANCE,
    data: &mut RefAny,
    window: &mut Window,
    fc_cache: &mut LazyFcCache,
    image_cache: &mut ImageCache,
    config: &AppConfig,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<usize>
) -> ProcessEventResult {

    use azul_core::window::{RawWindowHandle, WindowsHandle};

    let callback_result = fc_cache.apply_closure(|fc_cache| {

        let window_handle = RawWindowHandle::Windows(WindowsHandle {
            hwnd: window.hwnd as *mut _,
            hinstance: hinstance as *mut _,
        });

        let frame_start = (config.system_callbacks.get_system_time_fn.cb)();
        window.internal.run_all_threads(
            data,
            &window_handle,
            &window.gl_context_ptr,
            image_cache,
            fc_cache,
            &config.system_callbacks,
        )
    });

    return process_callback_results(
        callback_result,
        window,
        &NodesToCheck::empty(
            window.internal.current_window_state.mouse_state.mouse_down(),
            window.internal.current_window_state.focused_node,
        ),
        image_cache,
        new_windows,
        destroyed_windows
    );
}

#[must_use]
fn process_callback_results(
    mut callback_results: CallCallbacksResult,
    window: &mut Window,
    nodes_to_check: &NodesToCheck,
    image_cache: &mut ImageCache,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<usize>,
) -> ProcessEventResult {

    use azul_core::callbacks::Update;
    use azul_core::window_state::{StyleAndLayoutChanges, NodesToCheck};

    if callback_results.images_changed.is_some() ||
       callback_results.image_masks_changed.is_some() {
        update_image_resources(
            &mut window.render_api,
            &window.internal.layout_results,
            callback_results.images_changed.unwrap_or_default(),
            callback_results.image_masks_changed.unwrap_or_default(),
            &crate::app::CALLBACKS,
            &*image_cache,
            &mut window.internal.gl_texture_cache,
            &mut window.internal.renderer_resources,
            window.internal.document_id,
            window.internal.epoch,
        );
    }

    window.start_stop_timers(
        callback_results.timers.unwrap_or_default(),
        callback_results.timers_removed.unwrap_or_default()
    );
    window.start_stop_threads(
        callback_results.threads.unwrap_or_default(),
        callback_results.threads_removed.unwrap_or_default()
    );

    for w in callback_results.windows_created {
        new_windows.push(w);
    }

    let mut result = ProcessEventResult::DoNothing;

    let scroll = window.internal.current_window_state.process_system_scroll(&window.internal.scroll_states);
    let need_scroll_render = scroll.is_some();

    if let Some(modified) = callback_results.modified_window_state.as_ref() {
        if modified.flags.is_about_to_close {
            destroyed_windows.push(window.hwnd as usize);
        }
        window.internal.current_window_state = FullWindowState::from_window_state(
            modified,
            window.internal.current_window_state.dropped_file.clone(),
            window.internal.current_window_state.hovered_file.clone(),
            window.internal.current_window_state.focused_node.clone(),
            window.internal.current_window_state.last_hit_test.clone(),
        );
        if modified.size.get_layout_size() != window.internal.current_window_state.size.get_layout_size() {
            result = ProcessEventResult::UpdateHitTesterAndProcessAgain;
        } else if !need_scroll_render {
            result = ProcessEventResult::ShouldReRenderCurrentWindow;
        }
    }

    synchronize_window_state_with_os(
        window.hwnd,
        window.internal.previous_window_state.as_ref(),
        &window.internal.current_window_state
    );

    let layout_callback_changed = window.internal.current_window_state.layout_callback_changed(
        &window.internal.previous_window_state
    );

    if layout_callback_changed {
        return ProcessEventResult::ShouldRegenerateDomCurrentWindow;
    } else {
        match callback_results.callbacks_update_screen {
            Update::RegenerateStyledDomForCurrentWindow => {
                return ProcessEventResult::ShouldRegenerateDomCurrentWindow;
            },
            Update::RegenerateStyledDomForAllWindows => {
                return ProcessEventResult::ShouldRegenerateDomAllWindows;
            },
            Update::DoNothing => { },
        }
    }

    // Re-layout and re-style the window.internal.layout_results
    let mut style_layout_changes = StyleAndLayoutChanges::new(
        &nodes_to_check,
        &mut window.internal.layout_results,
        &image_cache,
        &mut window.internal.renderer_resources,
        window.internal.current_window_state.size.get_layout_size(),
        &window.internal.document_id,
        callback_results.css_properties_changed.as_ref(),
        callback_results.words_changed.as_ref(),
        &callback_results.update_focused_node,
        azul_layout::do_the_relayout,
    );

    // FOCUS CHANGE HAPPENS HERE!
    if let Some(focus_change) = style_layout_changes.focus_change.clone() {
         window.internal.current_window_state.focused_node = focus_change.new;
    }

    // Perform a system or user scroll event: only
    // scroll nodes that were not scrolled in the current frame
    //
    // Update the scroll states of the nodes, returning what nodes were actually scrolled this frame
    if let Some(scroll) = scroll {
        // Does a system scroll and re-invokes the IFrame
        // callbacks if scrolled out of view
        window.do_system_scroll(scroll);
        window.internal.current_window_state.mouse_state.reset_scroll_to_zero();
    }

    if style_layout_changes.did_resize_nodes() {
        // at least update the hit-tester
        ProcessEventResult::UpdateHitTesterAndProcessAgain
    } else if style_layout_changes.need_regenerate_display_list() {
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
    } else if need_scroll_render || style_layout_changes.need_redraw() {
        ProcessEventResult::ShouldReRenderCurrentWindow
    } else {
        result
    }
}

// Updates images and image mask resources
// NOTE: assumes the GL context is made current
fn update_image_resources(
    render_api: &mut WrRenderApi,
    layout_results: &[LayoutResult],
    images_to_update: BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>,
    image_masks_to_update: BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>,
    callbacks: &RenderCallbacks,
    image_cache: &ImageCache,
    gl_texture_cache: &mut GlTextureCache,
    renderer_resources: &mut RendererResources,
    document_id: DocumentId,
    epoch: Epoch,
) {

    use webrender::api::units::ImageDirtyRect as WrImageDirtyRect;
    use crate::wr_translate::{
        wr_translate_image_key,
        wr_translate_image_descriptor,
        wr_translate_image_data,
        wr_translate_document_id,
    };
    use azul_core::dom::NodeType;
    use azul_core::app_resources::{
        ImageData, ExternalImageType,
        ExternalImageData, DecodedImage,
        ImageBufferKind
    };

    let mut txn = WrTransaction::new();

    // update images
    for (dom_id, image_map) in images_to_update {

        let layout_result = match layout_results.get(dom_id.inner) {
            Some(s) => s,
            None => continue,
        };

        for (node_id, (image_ref, image_type)) in image_map {

            // get the existing key + extents of the image
            let existing_image_ref_hash = match image_type {
                UpdateImageType::Content => {
                    match layout_result.styled_dom.node_data.as_container().get(node_id).map(|n| n.get_node_type()) {
                        Some(NodeType::Image(image_ref)) => image_ref.get_hash(),
                        _ => continue,
                    }
                },
                UpdateImageType::Background => {

                    let node_data = layout_result.styled_dom.node_data.as_container();
                    let node_data = match node_data.get(node_id) {
                        Some(s) => s,
                        None => continue,
                    };

                    let styled_node_states = layout_result.styled_dom.styled_nodes.as_container();
                    let node_state = match styled_node_states.get(node_id) {
                        Some(s) => s.state.clone(),
                        None => continue,
                    };

                    let default = azul_css::StyleBackgroundContentVec::from_const_slice(&[]);

                    // TODO: only updates the first image background - usually not a problem
                    let bg_hash = layout_result.styled_dom.css_property_cache.ptr
                    .get_background_content(node_data, &node_id, &node_state)
                    .and_then(|bg| bg.get_property().unwrap_or(&default).as_ref().iter().find_map(|b| match b {
                        azul_css::StyleBackgroundContent::Image(id) => {
                            let image_ref = image_cache.get_css_image_id(id)?;
                            Some(image_ref.get_hash())
                        },
                        _ => None,
                    }));

                    match bg_hash {
                        Some(h) => h,
                        None => continue,
                    }
                }
            };

            let decoded_image = match image_ref.into_inner() {
                Some(s) => s,
                None => continue,
            };

            // Try getting the existing image key either
            // from the textures or from the renderer resources
            let existing_key = gl_texture_cache.solved_textures
                .get(&dom_id)
                .and_then(|map| map.get(&node_id))
                .map(|val| val.0);

            let existing_key = match existing_key {
                Some(s) => Some(s),
                None => {
                    renderer_resources
                    .get_image(&existing_image_ref_hash)
                    .map(|resolved_image| resolved_image.key)
                },
            };

            let key = match existing_key {
                Some(s) => s,
                None => continue, // updating an image requires at
                                  // least one image to be present
            };

            let (descriptor, data) = match decoded_image {
                DecodedImage::Gl(texture) => {

                    let descriptor = texture.get_descriptor();
                    let new_external_image_id = match gl_texture_cache.update_texture(
                        dom_id,
                        node_id,
                        document_id,
                        epoch,
                        texture,
                        &crate::app::CALLBACKS,
                    ) {
                        Some(s) => s,
                        None => continue,
                    };

                    let data = ImageData::External(ExternalImageData {
                        id: new_external_image_id,
                        channel_index: 0,
                        image_type: ExternalImageType::TextureHandle(ImageBufferKind::Texture2D),
                    });

                    (descriptor, data)
                },
                DecodedImage::Raw((descriptor, data)) => {
                    // use the hash to get the existing image key
                    // TODO: may lead to problems when the same ImageRef is used more than once?
                    renderer_resources.update_image(&existing_image_ref_hash, descriptor);
                    (descriptor, data)
                },
                DecodedImage::NullImage { .. } => continue, // TODO: NULL image descriptor?
                DecodedImage::Callback(callback) => {
                    // TODO: re-render image callbacks?
                    /*
                    let (key, descriptor) = match gl_texture_cache.solved_textures.get(&dom_id).and_then(|textures| textures.get(&node_id)) {
                        Some((k, d)) => (k, d),
                        None => continue,
                    };*/

                    continue
                },
            };

            // update the image descriptor in the renderer resources

            txn.update_image(
                wr_translate_image_key(key),
                wr_translate_image_descriptor(descriptor),
                wr_translate_image_data(data),
                &WrImageDirtyRect::All,
            );
        }
    }

    // update image masks
    for (dom_id, image_mask_map) in image_masks_to_update {

        let layout_result = match layout_results.get(dom_id.inner) {
            Some(s) => s,
            None => continue,
        };

        /*

        for (node_id, image_ref) in image_mask_map {
                let key = match existing_key {
                    Some(s) => s,
                    None => continue, // updating an image requires at
                                      // least one image to be present
                };
            let decoded_image = match image_ref.into_inner() {
                Some(s) => s,
                None => continue,
            };

            txn.update_image(
                wr_translate_image_key(key),
                wr_translate_image_descriptor(descriptor),
                wr_translate_image_data(data),
                &WrImageDirtyRect::All,
            );
        }
        */

    }

    render_api.send_transaction(wr_translate_document_id(document_id), txn);
}

fn create_windows(hinstance: HINSTANCE, app: &mut SharedApplicationData, new: Vec<WindowCreateOptions>) {
    for opts in new {
        if let Ok(w) = Window::create(hinstance, opts, app.clone()) {
            if let Ok(mut a) = app.inner.try_borrow_mut() {
                a.windows.insert(w.get_id(), w);
            }
        }
    }
}

fn destroy_windows(app: &mut ApplicationData, old: Vec<usize>) {
    use winapi::um::winuser::{PostMessageW, WM_QUIT};
    for window in old {
        if let Some(w) = app.windows.get(&window) {
            unsafe { PostMessageW(w.hwnd, WM_QUIT, 0, 0); }
        }
    }
}

fn run_all_threads() {
    // TODO
}

// Initializes the OS window
fn initialize_os_window(
    hwnd: HWND,
    initial_state: &WindowState,
    internal_state: &WindowState
) {

    /*

        window.set_title(new_state.title.as_str());
        window.set_maximized(new_state.flags.is_maximized);

        if new_state.flags.is_fullscreen {
            window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
        } else {
            window.set_fullscreen(None);
        }

        window.set_decorations(new_state.flags.has_decorations);
        window.set_inner_size(translate_logical_size(new_state.size.dimensions));
        window.set_min_inner_size(new_state.size.min_dimensions.into_option().map(translate_logical_size));
        window.set_min_inner_size(new_state.size.max_dimensions.into_option().map(translate_logical_size));

        if let WindowPosition::Initialized(new_position) = new_state.position {
            let new_position: PhysicalPosition<i32> = new_position.into();
            window.set_outer_position(translate_logical_position(new_position.to_logical(new_state.size.hidpi_factor)));
        }

        if let ImePosition::Initialized(new_ime_position) = new_state.ime_position {
            window.set_ime_position(translate_logical_position(new_ime_position));
        }

        window.set_always_on_top(new_state.flags.is_always_on_top);
        window.set_resizable(new_state.flags.is_resizable);
    */
}

fn synchronize_window_state_with_os(
    window: HWND,
    previous_state: Option<&FullWindowState>,
    current_state: &FullWindowState
) {
    // TODO: window.set_title
}

fn send_resource_updates(
    render_api: &mut WrRenderApi,
    resource_updates: Vec<ResourceUpdate>,
) {

}

// translates MouseCursorType to a builtin IDC_* value
// note: taken from https://github.com/rust-windowing/winit/blob/1c4d6e7613c3a3870cecb4cfa0eecc97409d45ff/src/platform_impl/windows/util.rs#L200
const fn win32_translate_cursor(input: MouseCursorType) -> *const wchar_t {
    use azul_core::window::MouseCursorType::*;
    use winapi::um::winuser;

    match input {
        Arrow
        | Default => winuser::IDC_ARROW,
        Hand => winuser::IDC_HAND,
        Crosshair => winuser::IDC_CROSS,
        Text
        | VerticalText => winuser::IDC_IBEAM,
        NotAllowed
        | NoDrop => winuser::IDC_NO,
        Grab
        | Grabbing
        | Move
        | AllScroll => {
            winuser::IDC_SIZEALL
        }
        EResize
        | WResize
        | EwResize
        | ColResize => winuser::IDC_SIZEWE,
        NResize
        | SResize
        | NsResize
        | RowResize => winuser::IDC_SIZENS,
        NeResize
        | SwResize
        | NeswResize => {
            winuser::IDC_SIZENESW
        }
        NwResize
        | SeResize
        | NwseResize => {
            winuser::IDC_SIZENWSE
        }
        Wait => winuser::IDC_WAIT,
        Progress => winuser::IDC_APPSTARTING,
        Help => winuser::IDC_HELP,
        _ => winuser::IDC_ARROW,
    }
}