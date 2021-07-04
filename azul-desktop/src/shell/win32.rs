#![cfg(target_os = "windows")]

//! Win32 implementation of the window shell containing all functions
//! related to running the application

use core::{
    ptr, mem,
    ffi::c_void,
    cell::{RefCell, BorrowError, BorrowMutError},
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
use alloc::{
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};
use azul_core::{
    callbacks::RefAny,
    window::{
        Menu, MenuItem, MenuCallback, WindowInternal, MonitorVec,
        WindowCreateOptions, WindowState, LogicalSize,
    },
    task::{TimerId, Timer, ThreadId, Thread},
    app_resources::{AppConfig, ResourceUpdate, ImageCache},
    gl::OptionGlContextPtr,
};
use winapi::{
    shared::{
        windef::{HWND, RECT, HGLRC, HDC, HMENU},
        ntdef::HRESULT,
        minwindef::{LPARAM, WPARAM, LRESULT, BOOL, HINSTANCE, TRUE, UINT},
    },
    um::uxtheme::MARGINS,
    um::dwmapi::{DWM_BLURBEHIND, DWM_BB_ENABLE},
};
use gl_context_loader::GenericGlContext;
use crate::app::{App, LazyFcCache};
use webrender::{
    render_api::{
        RenderApi as WrRenderApi,
    },
    api::{
        ApiHitTester as WrApiHitTester,
        DocumentId as WrDocumentId,
        units::{
            LayoutSize as WrLayoutSize,
            DeviceIntRect as WrDeviceIntRect,
            DeviceIntPoint as WrDeviceIntPoint,
            DeviceIntSize as WrDeviceIntSize,
        },
        RenderNotifier as WrRenderNotifier,
    },
    Transaction as WrTransaction,
    PipelineInfo as WrPipelineInfo,
    RendererOptions as WrRendererOptions,
    Renderer as WrRenderer,
    ShaderPrecacheFlags as WrShaderPrecacheFlags,
    Shaders as WrShaders,
    RendererError as WrRendererError,
};

const CLASS_NAME: &str = "AzulApplicationClass";

// TODO: Cache compiled shaders between renderers
const WR_SHADER_CACHE: Option<&Rc<RefCell<WrShaders>>> = None;

trait RectTrait {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}

impl RectTrait for RECT {
    fn width(&self) -> u32 { (self.right - self.left).max(0) as u32 }
    fn height(&self) -> u32 { (self.bottom - self.top).max(0) as u32 }
}
pub fn get_monitors(app: &App) -> MonitorVec {
    MonitorVec::from_const_slice(&[]) // TODO
}

/// Main function that starts when app.run() is invoked
pub fn run(mut app: App, root_window: WindowCreateOptions) -> Result<isize, WindowsStartupError> {
    use winapi::{
        shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
        um::{
            wingdi::wglMakeCurrent,
            libloaderapi::GetModuleHandleW,
            winuser::{
                RegisterClassW, GetDC, ReleaseDC,
                GetMessageW, DispatchMessageW, TranslateMessage,
                SetProcessDpiAwarenessContext,
                MSG, WNDCLASSW, CS_HREDRAW, CS_VREDRAW, CS_OWNDC
            }
        }
    };

    let hinstance = unsafe { GetModuleHandleW(ptr::null_mut()) };
    if hinstance.is_null() {
        return Err(WindowsStartupError::NoAppInstance(get_last_error()));
    }

    // Tell windows that this process is DPI-aware
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE); }

    // Register the application class (shared between windows)
    let mut class_name = encode_wide(CLASS_NAME);
    let mut wc: WNDCLASSW = unsafe { mem::zeroed() };
    wc.style = CS_HREDRAW | CS_VREDRAW | CS_OWNDC;
    wc.hInstance = hinstance;
    wc.lpszClassName = class_name.as_mut_ptr();
    wc.lpfnWndProc = Some(WindowProc);

    // RegisterClass can fail if the same class is
    // registered twice, error can be ignored
    unsafe { RegisterClassW(&wc) };

    let dwm = DwmFunctions::initialize();
    let gl = GlFunctions::initialize();

    let App { data, config, windows, image_cache, fc_cache} = app;
    let app_data_inner = Rc::new(RefCell::new(ApplicationData {
        hinstance,
        data,
        config,
        image_cache,
        fc_cache,
        windows: BTreeMap::new(),
        threads: BTreeMap::new(),
        timers: BTreeMap::new(),
        dwm,
    }));
    let application_data = SharedApplicationData { inner: app_data_inner.clone() };

    for opts in windows {
        if let Ok(w) = Window::create(hinstance, opts, application_data.clone()) {
            app_data_inner.try_borrow_mut()?.windows.insert(w.get_id(), w);
        }
    }

    if let Ok(w) = Window::create(hinstance, root_window, application_data.clone()) {
        app_data_inner.try_borrow_mut()?.windows.insert(w.get_id(), w);
    }

    for window in app_data_inner.try_borrow_mut()?.windows.values_mut() {
        window.show();
    }

    // Process the window messages one after another
    //
    // Multiple windows will process messages in sequence
    // to avoid complicated multithreading logic
    let mut msg: MSG = unsafe { mem::zeroed() };
    let mut results = Vec::new();
    let mut hwnds = Vec::new();

    'main: loop {

        {
            let app = match app_data_inner.try_borrow().ok() {
                Some(s) => s,
                None => break 'main, // borrow error
            };

            for win in app.windows.values() {
                hwnds.push(win.hwnd);
            }
        }

        for hwnd in hwnds {
            unsafe {
                results.push(GetMessageW(&mut msg, hwnd, 0, 0));
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        for r in results.iter() {
            if !(*r > 0) {
                break 'main; // error occured
            }
        }

        if results.is_empty() || hwnds.is_empty() {
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
    if dll.is_null() { None } else { Some(dll) }
}

pub enum WindowsWindowCreateError {
    FailedToCreateHWND(u32),
    Renderer(WrRendererError),
    BorrowMut(BorrowMutError),
}

pub enum WindowsOpenGlError {
    OpenGL32DllNotFound(u32),
    FailedToGetDC(u32),
    FailedToGetPixelFormat(u32),
    NoMatchingPixelFormat(u32),
    OpenGLNotAvailable(u32),
    FailedToStoreContext(u32),
}

pub enum WindowsStartupError {
    NoAppInstance(u32),
    WindowCreationFailed,
    Borrow(BorrowError),
    BorrowMut(BorrowMutError),
    Create(WindowsWindowCreateError),
    Gl(WindowsOpenGlError),
}

impl From<BorrowError> for WindowsStartupError {
    fn from(e: BorrowError) -> Self { WindowsStartupError::Borrow(e) }
}
impl From<BorrowMutError> for WindowsStartupError {
    fn from(e: BorrowMutError) -> Self { WindowsStartupError::BorrowMut(e) }
}
impl From<WindowsWindowCreateError> for WindowsStartupError {
    fn from(e: WindowsWindowCreateError) -> Self { WindowsStartupError::Create(e) }
}
impl From<WindowsOpenGlError> for WindowsStartupError {
    fn from(e: WindowsOpenGlError) -> Self { WindowsStartupError::Gl(e) }
}

struct Notifier { }

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> { Box::new(Notifier { }) }
    fn wake_up(&self, composite_needed: bool) { }
    fn new_frame_ready(&self,
        _: WrDocumentId,
        _scrolled: bool,
        composite_needed: bool,
        _render_time: Option<u64>) { }
}

#[derive(Clone)]
struct SharedApplicationData {
    inner: Rc<RefCell<ApplicationData>>,
}

// ApplicationData struct that is shared across
struct ApplicationData {
    hinstance: HINSTANCE,
    data: RefAny,
    config: AppConfig,
    image_cache: ImageCache,
    fc_cache: LazyFcCache,
    windows: BTreeMap<usize, Window>,
    threads: BTreeMap<ThreadId, Thread>,
    timers: BTreeMap<TimerId, Timer>,
    dwm: Option<DwmFunctions>,
}

// Extra functions from dwmapi.dll
struct DwmFunctions {
    _dwmapi_dll_handle: HINSTANCE,
    DwmEnableBlurBehindWindow: Option<extern "system" fn(HWND, &DWM_BLURBEHIND) -> HRESULT>,
    DwmExtendFrameIntoClientArea: Option<extern "system" fn(HWND, &MARGINS) -> HRESULT>,
    DwmDefWindowProc: Option<extern "system" fn(HWND, UINT, WPARAM, LPARAM, *mut LRESULT)>,
}

impl DwmFunctions {

    fn initialize() -> Option<Self> {
        use winapi::um::libloaderapi::{LoadLibraryW, GetProcAddress};

        let mut dll_name = encode_wide("dwmapi.dll");
        let hDwmAPI_DLL = LoadLibraryW(dll_name.as_mut_ptr());
        if hDwmAPI_DLL.is_null() {
            return None; // dwmapi.dll not found
        }

        let mut func_name = encode_ascii("DwmEnableBlurBehindWindow");
        let DwmEnableBlurBehindWindow = GetProcAddress(hDwmAPI_DLL, func_name.as_mut_ptr());
        let DwmEnableBlurBehindWindow = if DwmEnableBlurBehindWindow != ptr::null_mut() {
            Some(mem::transmute(DwmEnableBlurBehindWindow))
        } else {
            None
        };

        let mut func_name = encode_ascii("DwmExtendFrameIntoClientArea");
        let DwmExtendFrameIntoClientArea = GetProcAddress(hDwmAPI_DLL, func_name.as_mut_ptr());
        let DwmExtendFrameIntoClientArea = if DwmExtendFrameIntoClientArea != ptr::null_mut() {
            Some(mem::transmute(DwmExtendFrameIntoClientArea))
        } else {
            None
        };

        let mut func_name = encode_ascii("DwmDefWindowProc");
        let DwmDefWindowProc = GetProcAddress(hDwmAPI_DLL, func_name.as_mut_ptr());
        let DwmDefWindowProc = if DwmDefWindowProc != ptr::null_mut() {
            Some(mem::transmute(DwmDefWindowProc))
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

impl GlFunctions {

    // Initializes the DLL, but does not load the functions yet
    fn initialize() -> Self {

        use winapi::um::libloaderapi::LoadLibraryW;

        let opengl32_dll = load_dll("opengl32.dll");

        // zero-initialize all function pointers
        let context: GenericGlContext = unsafe { mem::zeroed() };

        Self {
            _opengl32_dll_handle: opengl32_dll,
            functions: Rc::new(context)
        }
    }

    // Assuming the OpenGL context is current, loads the OpenGL function pointers
    fn load(&mut self) {

        fn get_func(s: &str, opengl32_dll: Option<HINSTANCE>) -> *mut gl_context_loader::c_void {
            use winapi::um::{
                wingdi::wglGetProcAddress,
                libloaderapi::GetProcAddress,
            };

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
            glBeginConditionalRender: get_func("glBeginConditionalRender", self._opengl32_dll_handle),
            glBeginQuery: get_func("glBeginQuery", self._opengl32_dll_handle),
            glBeginTransformFeedback: get_func("glBeginTransformFeedback", self._opengl32_dll_handle),
            glBindAttribLocation: get_func("glBindAttribLocation", self._opengl32_dll_handle),
            glBindBuffer: get_func("glBindBuffer", self._opengl32_dll_handle),
            glBindBufferBase: get_func("glBindBufferBase", self._opengl32_dll_handle),
            glBindBufferRange: get_func("glBindBufferRange", self._opengl32_dll_handle),
            glBindFragDataLocation: get_func("glBindFragDataLocation", self._opengl32_dll_handle),
            glBindFragDataLocationIndexed: get_func("glBindFragDataLocationIndexed", self._opengl32_dll_handle),
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
            glCheckFramebufferStatus: get_func("glCheckFramebufferStatus", self._opengl32_dll_handle),
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
            glCompressedTexSubImage1D: get_func("glCompressedTexSubImage1D", self._opengl32_dll_handle),
            glCompressedTexSubImage2D: get_func("glCompressedTexSubImage2D", self._opengl32_dll_handle),
            glCompressedTexSubImage3D: get_func("glCompressedTexSubImage3D", self._opengl32_dll_handle),
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
            glDebugMessageCallbackKHR: get_func("glDebugMessageCallbackKHR", self._opengl32_dll_handle),
            glDebugMessageControl: get_func("glDebugMessageControl", self._opengl32_dll_handle),
            glDebugMessageControlKHR: get_func("glDebugMessageControlKHR", self._opengl32_dll_handle),
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
            glDeleteVertexArraysAPPLE: get_func("glDeleteVertexArraysAPPLE", self._opengl32_dll_handle),
            glDepthFunc: get_func("glDepthFunc", self._opengl32_dll_handle),
            glDepthMask: get_func("glDepthMask", self._opengl32_dll_handle),
            glDepthRange: get_func("glDepthRange", self._opengl32_dll_handle),
            glDetachShader: get_func("glDetachShader", self._opengl32_dll_handle),
            glDisable: get_func("glDisable", self._opengl32_dll_handle),
            glDisableClientState: get_func("glDisableClientState", self._opengl32_dll_handle),
            glDisableVertexAttribArray: get_func("glDisableVertexAttribArray", self._opengl32_dll_handle),
            glDisablei: get_func("glDisablei", self._opengl32_dll_handle),
            glDrawArrays: get_func("glDrawArrays", self._opengl32_dll_handle),
            glDrawArraysInstanced: get_func("glDrawArraysInstanced", self._opengl32_dll_handle),
            glDrawBuffer: get_func("glDrawBuffer", self._opengl32_dll_handle),
            glDrawBuffers: get_func("glDrawBuffers", self._opengl32_dll_handle),
            glDrawElements: get_func("glDrawElements", self._opengl32_dll_handle),
            glDrawElementsBaseVertex: get_func("glDrawElementsBaseVertex", self._opengl32_dll_handle),
            glDrawElementsInstanced: get_func("glDrawElementsInstanced", self._opengl32_dll_handle),
            glDrawElementsInstancedBaseVertex: get_func("glDrawElementsInstancedBaseVertex", self._opengl32_dll_handle),
            glDrawPixels: get_func("glDrawPixels", self._opengl32_dll_handle),
            glDrawRangeElements: get_func("glDrawRangeElements", self._opengl32_dll_handle),
            glDrawRangeElementsBaseVertex: get_func("glDrawRangeElementsBaseVertex", self._opengl32_dll_handle),
            glEdgeFlag: get_func("glEdgeFlag", self._opengl32_dll_handle),
            glEdgeFlagPointer: get_func("glEdgeFlagPointer", self._opengl32_dll_handle),
            glEdgeFlagv: get_func("glEdgeFlagv", self._opengl32_dll_handle),
            glEnable: get_func("glEnable", self._opengl32_dll_handle),
            glEnableClientState: get_func("glEnableClientState", self._opengl32_dll_handle),
            glEnableVertexAttribArray: get_func("glEnableVertexAttribArray", self._opengl32_dll_handle),
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
            glFlushMappedBufferRange: get_func("glFlushMappedBufferRange", self._opengl32_dll_handle),
            glFogCoordPointer: get_func("glFogCoordPointer", self._opengl32_dll_handle),
            glFogCoordd: get_func("glFogCoordd", self._opengl32_dll_handle),
            glFogCoorddv: get_func("glFogCoorddv", self._opengl32_dll_handle),
            glFogCoordf: get_func("glFogCoordf", self._opengl32_dll_handle),
            glFogCoordfv: get_func("glFogCoordfv", self._opengl32_dll_handle),
            glFogf: get_func("glFogf", self._opengl32_dll_handle),
            glFogfv: get_func("glFogfv", self._opengl32_dll_handle),
            glFogi: get_func("glFogi", self._opengl32_dll_handle),
            glFogiv: get_func("glFogiv", self._opengl32_dll_handle),
            glFramebufferRenderbuffer: get_func("glFramebufferRenderbuffer", self._opengl32_dll_handle),
            glFramebufferTexture: get_func("glFramebufferTexture", self._opengl32_dll_handle),
            glFramebufferTexture1D: get_func("glFramebufferTexture1D", self._opengl32_dll_handle),
            glFramebufferTexture2D: get_func("glFramebufferTexture2D", self._opengl32_dll_handle),
            glFramebufferTexture3D: get_func("glFramebufferTexture3D", self._opengl32_dll_handle),
            glFramebufferTextureLayer: get_func("glFramebufferTextureLayer", self._opengl32_dll_handle),
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
            glGetActiveUniformBlockName: get_func("glGetActiveUniformBlockName", self._opengl32_dll_handle),
            glGetActiveUniformBlockiv: get_func("glGetActiveUniformBlockiv", self._opengl32_dll_handle),
            glGetActiveUniformName: get_func("glGetActiveUniformName", self._opengl32_dll_handle),
            glGetActiveUniformsiv: get_func("glGetActiveUniformsiv", self._opengl32_dll_handle),
            glGetAttachedShaders: get_func("glGetAttachedShaders", self._opengl32_dll_handle),
            glGetAttribLocation: get_func("glGetAttribLocation", self._opengl32_dll_handle),
            glGetBooleani_v: get_func("glGetBooleani_v", self._opengl32_dll_handle),
            glGetBooleanv: get_func("glGetBooleanv", self._opengl32_dll_handle),
            glGetBufferParameteri64v: get_func("glGetBufferParameteri64v", self._opengl32_dll_handle),
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
            glGetFramebufferAttachmentParameteriv: get_func("glGetFramebufferAttachmentParameteriv", self._opengl32_dll_handle),
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
            glGetRenderbufferParameteriv: get_func("glGetRenderbufferParameteriv", self._opengl32_dll_handle),
            glGetSamplerParameterIiv: get_func("glGetSamplerParameterIiv", self._opengl32_dll_handle),
            glGetSamplerParameterIuiv: get_func("glGetSamplerParameterIuiv", self._opengl32_dll_handle),
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
            glGetTexLevelParameterfv: get_func("glGetTexLevelParameterfv", self._opengl32_dll_handle),
            glGetTexLevelParameteriv: get_func("glGetTexLevelParameteriv", self._opengl32_dll_handle),
            glGetTexParameterIiv: get_func("glGetTexParameterIiv", self._opengl32_dll_handle),
            glGetTexParameterIuiv: get_func("glGetTexParameterIuiv", self._opengl32_dll_handle),
            glGetTexParameterPointervAPPLE: get_func("glGetTexParameterPointervAPPLE", self._opengl32_dll_handle),
            glGetTexParameterfv: get_func("glGetTexParameterfv", self._opengl32_dll_handle),
            glGetTexParameteriv: get_func("glGetTexParameteriv", self._opengl32_dll_handle),
            glGetTransformFeedbackVarying: get_func("glGetTransformFeedbackVarying", self._opengl32_dll_handle),
            glGetUniformBlockIndex: get_func("glGetUniformBlockIndex", self._opengl32_dll_handle),
            glGetUniformIndices: get_func("glGetUniformIndices", self._opengl32_dll_handle),
            glGetUniformLocation: get_func("glGetUniformLocation", self._opengl32_dll_handle),
            glGetUniformfv: get_func("glGetUniformfv", self._opengl32_dll_handle),
            glGetUniformiv: get_func("glGetUniformiv", self._opengl32_dll_handle),
            glGetUniformuiv: get_func("glGetUniformuiv", self._opengl32_dll_handle),
            glGetVertexAttribIiv: get_func("glGetVertexAttribIiv", self._opengl32_dll_handle),
            glGetVertexAttribIuiv: get_func("glGetVertexAttribIuiv", self._opengl32_dll_handle),
            glGetVertexAttribPointerv: get_func("glGetVertexAttribPointerv", self._opengl32_dll_handle),
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
            glInvalidateBufferSubData: get_func("glInvalidateBufferSubData", self._opengl32_dll_handle),
            glInvalidateFramebuffer: get_func("glInvalidateFramebuffer", self._opengl32_dll_handle),
            glInvalidateSubFramebuffer: get_func("glInvalidateSubFramebuffer", self._opengl32_dll_handle),
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
            glMultiDrawElementsBaseVertex: get_func("glMultiDrawElementsBaseVertex", self._opengl32_dll_handle),
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
            glRenderbufferStorageMultisample: get_func("glRenderbufferStorageMultisample", self._opengl32_dll_handle),
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
            glShaderStorageBlockBinding: get_func("glShaderStorageBlockBinding", self._opengl32_dll_handle),
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
            glTransformFeedbackVaryings: get_func("glTransformFeedbackVaryings", self._opengl32_dll_handle),
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
            unsafe { FreeLibrary(opengl32); }
        }
    }
}

#[derive(Default)]
struct ExtraWglFunctions {
    wglCreateContextAttribsARB: Option<extern "system" fn(HDC, HGLRC, *const [i32]) -> HGLRC>,
    wglSwapIntervalEXT: Option<extern "system" fn(i32) -> i32>,
    wglChoosePixelFormatARB: Option<extern "system" fn(HDC, *const [i32], *const f32, u32, *mut i32, *mut u32) -> BOOL>,
}

impl ExtraWglFunctions {
    // Assumes that at least one (dummy) OpenGL is current
    pub fn load() -> Self {

        use winapi::um::wingdi::wglGetProcAddress;

        let mut extra = ExtraWglFunctions {
            .. Default::default()
        };

        let mut func_name_1 = encode_ascii("wglChoosePixelFormatARB");
        let mut func_name_2 = encode_ascii("wglChoosePixelFormatEXT");

        let wgl1_result = unsafe { wglGetProcAddress(func_name_1.as_mut_ptr()) };
        let wgl2_result = unsafe { wglGetProcAddress(func_name_2.as_mut_ptr()) };

        let wglarb_ChoosePixelFormatARB = if wgl1_result != ptr::null_mut() {
            Some(unsafe { mem::transmute(wgl1_result) })
        } else if wgl2_result != ptr::null_mut() {
            Some(unsafe { mem::transmute(wgl2_result) })
        } else {
            None
        };

        extra.wglChoosePixelFormatARB = wglarb_ChoosePixelFormatARB;

        let mut func_name = encode_ascii("wglCreateContextAttribsARB");
        let proc_address = unsafe { wglGetProcAddress(func_name.as_mut_ptr()) };
        extra.wglCreateContextAttribsARB = if proc_address == ptr::null_mut() {
            None
        } else {
            Some(unsafe { mem::transmute(proc_address) })
        };

        let mut func_name = encode_ascii("wglSwapIntervalEXT");
        let proc_address = unsafe { wglGetProcAddress(func_name.as_mut_ptr()) };
        extra.wglSwapIntervalEXT = if proc_address == ptr::null_mut() {
            None
        } else {
            Some(unsafe { mem::transmute(proc_address) })
        };

        extra
    }
}

struct Window {
    /// HWND handle of the plaform window
    hwnd: HWND,
    /// Current window state
    state: WindowState,
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
    hit_tester: Arc<dyn WrApiHitTester>,
    /// ID -> Callback map for the window menu (default: empty map)
    menu_callbacks: BTreeMap<u16, MenuCallback>,
}

impl Window {

    fn get_id(&self) -> usize {
        self.hwnd as usize
    }

    // Creates a new HWND according to the options
    fn create(
        hinstance: HINSTANCE,
        mut options: WindowCreateOptions,
        data: SharedApplicationData
    ) -> Result<Self, WindowsWindowCreateError> {

        use winapi::{
            shared::windef::POINT,
            um::{
                wingdi::{wglMakeCurrent, wglDeleteContext},
                winuser::{
                    CreateWindowExW, GetDpiForWindow, GetDC, ReleaseDC,
                    GetWindowRect, SetMenu, GetClientRect, GetCursorPos,
                    ScreenToClient, DestroyWindow,
                    WS_EX_APPWINDOW, WS_EX_ACCEPTFILES,
                    WS_OVERLAPPED, WS_CAPTION, WS_SYSMENU, WS_THICKFRAME,
                    WS_MINIMIZEBOX, WS_MAXIMIZEBOX, WS_TABSTOP,
                    WS_POPUP, CW_USEDEFAULT
                }
            }
        };
        use azul_core::{
            gl::GlContextPtr,
            callbacks::PipelineId,
            window::{
                CursorPosition, WindowInternalInit,
                HwAcceleration, PhysicalSize, RendererType,
                LogicalPosition,
            }
        };
        use crate::{
            compositor::Compositor,
            wr_translate::{
                translate_document_id_wr,
                wr_translate_debug_flags,
                translate_id_namespace_wr,
                wr_translate_document_id,
            }
        };
        use webrender::ProgramCache as WrProgramCache;
        use webrender::api::ColorF as WrColorF;

        let window_data = Box::new(data.clone());
        let parent_window = match options.state.platform_specific_options.windows_options.parent_window.as_ref() {
            Some(hwnd) => (*hwnd) as HWND,
            None => ptr::null_mut(),
        };

        let mut class_name = encode_wide(CLASS_NAME);
        let mut window_title = encode_wide(options.state.title.as_str());
        let physical_size = options.state.size.get_physical_size();

        // Create the window
        let hwnd = unsafe { CreateWindowExW(
            WS_EX_APPWINDOW | WS_EX_ACCEPTFILES,
            class_name.as_mut_ptr(),
            window_title.as_mut_ptr(),

            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_TABSTOP | WS_POPUP,

            // Size and position: set later, after DPI factor has been queried
            CW_USEDEFAULT, // x
            CW_USEDEFAULT, // y
            CW_USEDEFAULT, // width
            CW_USEDEFAULT, // height

            parent_window,
            ptr::null_mut(),            // Menu
            hinstance,
            Box::leak(window_data) as *mut SharedApplicationData as *mut c_void,
        ) };

        if hwnd.is_null() {
            return Err(WindowsWindowCreateError::FailedToCreateHWND(get_last_error()));
        }

        // Get / store DPI
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let dpi_factor = dpi as f32 / 96.0;
        options.state.size.dpi = dpi;
        options.state.size.hidpi_factor = dpi_factor;
        options.state.size.system_hidpi_factor = dpi_factor;

        // Window created, now try initializing OpenGL context
        let renderer_types = match options.renderer.into_option() {
            Some(s) => match s.hw_accel {
                HwAcceleration::Enabled => vec![RendererType::Hardware],
                HwAcceleration::Disabled => vec![RendererType::Software],
            },
            None => vec![
                RendererType::Hardware,
                RendererType::Software,
            ]
        };

        let mut opengl_context: Option<HGLRC> = None;
        let mut rt = RendererType::Software;
        let mut extra = ExtraWglFunctions::default();
        let mut gl = GlFunctions::initialize();
        let mut gl_context_ptr: OptionGlContextPtr = None.into();

        for r in renderer_types {
            rt = r;
            match r {
                RendererType::Software => { },
                RendererType::Hardware => {
                    if let Ok((o, extra_funcs)) = create_gl_context(hwnd) {
                        opengl_context = Some(o);
                        extra = extra_funcs;
                        break;
                    }
                }
            }
        }

        gl_context_ptr = opengl_context.map(|hrc| unsafe {
            let hdc = GetDC(hwnd);
            unsafe { wglMakeCurrent(hdc, hrc) };
            gl.load();
            // compiles SVG and FXAA shader programs...
            let ptr = GlContextPtr::new(rt, gl.functions.clone());
            unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
            ReleaseDC(hwnd, hdc);
            ptr
        }).into();

        // WindowInternal::new() may dispatch OpenGL calls,
        // need to make context current before invoking
        let hdc = unsafe { GetDC(hwnd) };
        if let Some(hrc) = opengl_context.as_mut() {
            unsafe { wglMakeCurrent(hdc, *hrc) };
        }

        // Invoke callback to initialize UI for the first time
        let mut initial_resource_updates = Vec::new();

        let (mut renderer, sender) = match WrRenderer::new(
            gl.functions.clone(),
            Box::new(Notifier { }),
            WrRendererOptions {
                resource_override_path: None,
                precache_flags: WrShaderPrecacheFlags::EMPTY,
                device_pixel_ratio: dpi_factor,
                enable_subpixel_aa: true,
                enable_aa: true,
                cached_programs: Some(WrProgramCache::new(None)),
                clear_color: Some(WrColorF { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }), // transparent
                enable_multithreading: true,
                debug_flags: wr_translate_debug_flags(&options.state.debug_state),
                .. WrRendererOptions::default()
            },
            WR_SHADER_CACHE
        ) {
            Ok(o) => o,
            Err(e) => unsafe {
                if let Some(hrc) = opengl_context.as_mut() {
                    unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
                    unsafe { wglDeleteContext(*hrc) };
                }
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return Err(WindowsWindowCreateError::Renderer(e));
            },
        };

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        let mut render_api = sender.create_api();

        // Query the current size of the window
        let mut rect: RECT = unsafe { mem::zeroed() };
        let current_window_size = unsafe { GetWindowRect(hwnd, &mut rect) }; // not DPI adjusted: physical pixels
        let physical_size = PhysicalSize {
            width: rect.width(),
            height: rect.height(),
        };
        options.state.size.dimensions = physical_size.to_logical(dpi_factor);
        let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let pipeline_id = PipelineId::new();
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // hit tester will be empty on startup
        let hit_tester = render_api.request_hit_tester(wr_translate_document_id(document_id)).resolve();
        let hit_tester_ref = &*hit_tester;

        // lock the SharedApplicationData in order to
        // invoke the UI callback for the first time
        let appdata_lock = match data.inner.try_borrow_mut() {
            Ok(o) => o,
            Err(e) => unsafe {
                if let Some(hrc) = opengl_context.as_mut() {
                    unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
                    unsafe { wglDeleteContext(*hrc) };
                }
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return Err(WindowsWindowCreateError::BorrowMut(e));
            }
        };
        let internal = appdata_lock.fc_cache.apply_closure(|fc_cache| {
            WindowInternal::new(
                WindowInternalInit {
                    window_create_options: options,
                    document_id,
                    id_namespace,
                },
                &mut appdata_lock.data,
                &appdata_lock.image_cache,
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
                }
            )
        });

        if let Some(hrc) = opengl_context.as_ref() {
            unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
        }
        unsafe { ReleaseDC(hwnd, hdc); }

        // Since the menu bar affects the window size, set it first,
        // before querying the window size again
        let mut menu_callbacks = BTreeMap::new();
        if let Some(menu_bar) = internal.get_menu_bar() {
            let WindowsMenuBar { _native_ptr, callbacks } = WindowsMenuBar::new(menu_bar);
            unsafe { SetMenu(hwnd, _native_ptr); }
            menu_callbacks = callbacks;
        }

        // Now get the size of the [0] UI in order to
        // create the window with the proper size
        if options.size_to_content {
            let content_size = internal.get_content_size();

            // SetWindowPos(hWnd, HWND_TOP, 0, 0, Rect.right - Rect.left, Rect.bottom - Rect.top, SWP_NOZORDER | SWP_NOMOVE | SWP_SHOWWINDOW);
            // setInnerSize(content_size);
        }

        // not DPI adjusted: physical pixels
        let mut rect: RECT = unsafe { mem::zeroed() };
        let current_window_size = unsafe { GetClientRect(hwnd, &mut rect) };
        let physical_size = PhysicalSize {
            width: rect.width(),
            height: rect.height(),
        };
        options.state.size.dimensions = physical_size.to_logical(dpi_factor);

        // Adjust webrender for the new framebuffer size
        let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let mut txn = WrTransaction::new();
        txn.set_document_view(
            WrDeviceIntRect::new(WrDeviceIntPoint::zero(), framebuffer_size),
            dpi_factor,
        );
        render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);

        // re-layout the window content for the first frame
        // (since the width / height might have changed)
        internal.relayout(options.state.size.dimensions);

        // Build the display list
        let display_list_size = rebuild_display_list(
            &mut internal,
            &mut render_api,
            &appdata_lock.image_cache,
            initial_resource_updates,
        );

        // Unlock the SharedApplicationData
        mem::drop(appdata_lock);

        // Get / store mouse cursor position, now that the window position is final
        let mut cursor_pos: POINT = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut cursor_pos); }
        unsafe { ScreenToClient(hwnd, &mut cursor_pos) };
        let mut cursor_pos_logical = LogicalPosition {
            x: cursor_pos.x as f32 / dpi_factor,
            y: cursor_pos.y as f32 / dpi_factor,
        };
        options.state.mouse_state.cursor_position = if cursor_pos.x <= 0 || cursor_pos.y <= 0 {
            CursorPosition::OutOfWindow
        } else {
            CursorPosition::InWindow(cursor_pos_logical)
        };

        // Update the hit-tester to account for the new hit-testing functionality
        let hit_tester = render_api.request_hit_tester(wr_translate_document_id(document_id)).resolve();

        // Done! Window is now created properly, display list has been built by
        // WebRender (window is ready to render), menu bar is visible and hit-tester
        // now contains the newest UI tree.

        // NOTE: The window is NOT stored yet
        Ok(Window {
            hwnd,
            state: options.state,
            internal,
            gl_context: opengl_context,
            gl_functions: gl,
            gl_context_ptr,
            render_api,
            renderer: Some(renderer),
            hit_tester,
            menu_callbacks,
        })
    }

    // Calls ShowWindow to show the window on the screen
    fn show(&mut self) {

        use winapi::um::winuser::{
            ShowWindow, SW_HIDE,
            SW_SHOWDEFAULT, SW_MINIMIZE,
            SW_NORMAL, SW_MAXIMIZE,
        };

        let mut sw_options = SW_HIDE; // 0 = default
        if self.state.flags.is_visible {
            sw_options |= SW_SHOWDEFAULT;
        }

        match self.state.flags.frame {
            Normal => sw_options |= SW_NORMAL,
            Minimized => sw_options |= SW_MINIMIZE,
            Maximized => sw_options |= SW_MAXIMIZE,
            Fullscreen => sw_options |= SW_MAXIMIZE,
        }

        unsafe { ShowWindow(self.hwnd, sw_options); }
    }
}

/// Returns the size fo the built display list
#[cfg(not(test))]
pub fn rebuild_display_list(
    internal: &mut WindowInternal,
    render_api: &mut WrRenderApi,
    image_cache: &ImageCache,
    resources: Vec<ResourceUpdate>,
) -> LogicalSize {

    use crate::wr_translate::{
        wr_translate_pipeline_id,
        wr_translate_document_id,
        wr_translate_display_list,
        wr_translate_epoch,
        wr_translate_resource_update,
    };
    use azul_core::styled_dom::DomId;
    use azul_core::ui_solver::LayoutResult;
    use azul_core::callbacks::PipelineId;

    let mut txn = WrTransaction::new();

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    let root_id = DomId { inner: 0 };
    let cached_display_list = LayoutResult::get_cached_display_list(
         &internal.document_id,
         root_id,
         internal.epoch,
         &internal.layout_results,
         &internal.current_window_state,
         &internal.gl_texture_cache,
         &internal.renderer_resources,
         image_cache,
    );

    let display_list_size = cached_display_list.root_size;

    let root_pipeline_id = PipelineId(0, internal.document_id.id);
    let display_list = wr_translate_display_list(
        internal.document_id,
        &mut render_api,
        cached_display_list,
        root_pipeline_id,
        internal.current_window_state.size.hidpi_factor
    );

    let logical_size = WrLayoutSize::new(
        internal.current_window_state.size.dimensions.width,
        internal.current_window_state.size.dimensions.height
    );

    txn.update_resources(resources.into_iter().map(wr_translate_resource_update).collect());
    txn.set_display_list(
        wr_translate_epoch(internal.epoch),
        None,
        logical_size.clone(),
        (wr_translate_pipeline_id(root_pipeline_id), display_list),
        true,
    );

    render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);

    display_list_size
}

// function can fail: creates an OpenGL context on the HWND, stores the context on the window-associated data
fn create_gl_context(hwnd: HWND) -> Result<(HGLRC, ExtraWglFunctions), WindowsOpenGlError> {

    use winapi::um::{
        wingdi::{
            ChoosePixelFormat, SetPixelFormat, DescribePixelFormat,
            wglCreateContext, wglDeleteContext, wglMakeCurrent,
            PFD_DRAW_TO_WINDOW, PFD_SUPPORT_OPENGL, PFD_DOUBLEBUFFER,
            PFD_TYPE_RGBA, PFD_MAIN_PLANE,
            PIXELFORMATDESCRIPTOR
        },
        winuser::{GetDC, ReleaseDC},
    };

    use self::WindowsOpenGlError::*;

    // -- window created, now create OpenGL context

    let opengl32_dll = load_dll("opengl32.dll")
    .ok_or(OpenGL32DllNotFound(get_last_error()))?;

    // Get DC
    let hDC = unsafe { GetDC(hwnd) };
    if hDC.is_null()  {
        // unsafe { DestroyWindow(hwnd) };
        return Err(FailedToGetDC(get_last_error()));
    }

    // now this is a kludge; we need to pass something in the PIXELFORMATDESCRIPTOR
    // to SetPixelFormat; it will be ignored, mostly. OTOH we want to send something
    // sane, we're nice people after all - it doesn't hurt if this fails.
    let mut pfd = PIXELFORMATDESCRIPTOR {
        nSize: mem::size_of::<PIXELFORMATDESCRIPTOR> as u16,
        nVersion: 1,
        dwFlags: {
            PFD_DRAW_TO_WINDOW |   // support window
            PFD_SUPPORT_OPENGL |   // support OpenGL
            PFD_DOUBLEBUFFER       // double buffered
        },
        iPixelType: PFD_TYPE_RGBA as u8,
        cColorBits: 24,
        cRedBits: 0,
        cRedShift: 0,
        cGreenBits: 0,
        cGreenShift: 0,
        cBlueBits: 0,
        cBlueShift: 0,
        cAlphaBits: 0,
        cAlphaShift: 0,
        cAccumBits: 0,
        cAccumRedBits: 0,
        cAccumGreenBits: 0,
        cAccumBlueBits: 0,
        cAccumAlphaBits: 0,
        cDepthBits: 32, // 32-bit z-buffer
        cStencilBits: 0, // no stencil buffer
        cAuxBuffers: 0, // no auxiliary buffer
        iLayerType: PFD_MAIN_PLANE as u8, // main layer
        bReserved: 0,
        dwLayerMask: 0,
        dwVisibleMask: 0,
        dwDamageMask: 0,
    };

    let default_pixel_format = unsafe { ChoosePixelFormat(hDC, &pfd) };
    unsafe {
        DescribePixelFormat(hDC, default_pixel_format, mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32, &mut pfd);
        if !SetPixelFormat(hDC, default_pixel_format, &pfd) == TRUE {
            // can't even set the default fallback pixel format: no OpenGL possible
            ReleaseDC(hwnd, hDC);
            // DestroyWindow(hwnd);
            return Err(NoMatchingPixelFormat(get_last_error()));
        }
    }

    // wglGetProcAddress will fail if there is no context being current,
    // create a dummy context and activate it
    let dummy_context = unsafe {
        let dc = wglCreateContext(hDC);
        wglMakeCurrent(hDC, dc);
        dc
    };

    let extra_functions = ExtraWglFunctions::load();

    fn get_transparent_pixel_format_index(hDC: HDC, extra_functions: &ExtraWglFunctions) -> Option<i32> {

        use winapi::um::{
            winuser::ReleaseDC,
            wingdi::{
                wglMakeCurrent, wglDeleteContext,
                DescribePixelFormat, SetPixelFormat
            }
        };

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

        let attribs = [
            WGL_DRAW_TO_WINDOW_ARB, TRUE,
            WGL_DOUBLE_BUFFER_ARB, TRUE,
            WGL_SUPPORT_OPENGL_ARB, TRUE,
            WGL_PIXEL_TYPE_ARB, WGL_TYPE_RGBA_ARB,
            WGL_TRANSPARENT_ARB, TRUE,
            WGL_COLOR_BITS_ARB, 32,
            WGL_RED_BITS_ARB, 8,
            WGL_GREEN_BITS_ARB, 8,
            WGL_BLUE_BITS_ARB, 8,
            WGL_ALPHA_BITS_ARB, 8,
            WGL_DEPTH_BITS_ARB, 24,
            WGL_STENCIL_BITS_ARB, 8,
            0, 0
        ];

        let mut pixel_format = 0;
        let mut num_pixel_formats = 0;

        let wglarb_ChoosePixelFormatARB = extra_functions.wglChoosePixelFormatARB?;

        let choose_pixel_format_result = unsafe { (wglarb_ChoosePixelFormatARB)(
            hDC, &attribs[..], ptr::null(), 1, &mut pixel_format, &mut num_pixel_formats
        ) };

        if choose_pixel_format_result != TRUE {
            return None; // wglarb_ChoosePixelFormatARB failed
        }

        // pixel format is now the index of the PIXELFORMATDESCRIPTOR
        // that can handle a transparent OpenGL context
        if num_pixel_formats == 0 { None } else { Some(pixel_format) }
    }

    let mut b_transparent_succeeded = false;
    let transparent_opengl_pixelformat_index = match get_transparent_pixel_format_index(hDC, &extra_functions) {
        Some(i) => { b_transparent_succeeded = true; i },
        None => default_pixel_format,
    };

    // destroy the dummy context
    unsafe {
        wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
        wglDeleteContext(dummy_context);
    }

    // set the new pixel format. if transparency is not available, this will fallback to the default PFD
    unsafe {
        DescribePixelFormat(hDC, transparent_opengl_pixelformat_index, mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32, &mut pfd);
        if SetPixelFormat(hDC, transparent_opengl_pixelformat_index, &pfd) != TRUE {
            ReleaseDC(hwnd, hDC);
            return Err(NoMatchingPixelFormat(get_last_error()));
        }
    }

    // https://www.khronos.org/registry/OpenGL/extensions/ARB/WGL_ARB_create_context.txt
    const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
    const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;

    // Create OpenGL 3.1 context
    let context_attribs = [
        WGL_CONTEXT_MAJOR_VERSION_ARB, 3,
        WGL_CONTEXT_MINOR_VERSION_ARB, 1,
        0, 0
    ];

    let CreateContextAttribsARB = if b_transparent_succeeded {
        extra_functions.wglCreateContextAttribsARB
    } else {
        None
    };

    let wglSwapIntervalEXT = extra_functions.wglSwapIntervalEXT;

    let hRC = match extra_functions.wglCreateContextAttribsARB {
        Some(func) => unsafe { (func)(hDC, ptr::null_mut(), &context_attribs[..]) },
        None => unsafe { wglCreateContext(hDC) },
    };

    if hRC.is_null() {
        unsafe { ReleaseDC(hwnd, hDC); }
        return Err(OpenGLNotAvailable(get_last_error()));
    }

    // return final context
    unsafe { ReleaseDC(hwnd, hDC); }
    return Ok((hRC, extra_functions));
}

// Initializes the OS window
fn initialize_os_window(hwnd: HWND, initial_state: &WindowState, internal_state: &WindowState) {

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

struct WindowsMenuBar {
    _native_ptr: HMENU,
    /// Map from Command -> callback to call
    callbacks: BTreeMap<u16, MenuCallback>,
}

static WINDOWS_UNIQUE_COMMAND_ID_GENERATOR: AtomicUsize = AtomicUsize::new(1); // 0 = no command

impl WindowsMenuBar {

    fn new(new: &Menu) -> Self {

        use winapi::um::winuser::CreateMenu;

        let mut root = unsafe { CreateMenu() };
        let mut command_map = BTreeMap::new();

        Self::recursive_construct_menu(&mut root, new.items.as_ref(), &mut command_map);

        Self {
            _native_ptr: root,
            callbacks: command_map,
        }
    }

    fn get_new_command_id() -> usize {
        WINDOWS_UNIQUE_COMMAND_ID_GENERATOR.fetch_add(1, AtomicOrdering::SeqCst)
    }

    fn recursive_construct_menu(menu: &mut HMENU, items: &[MenuItem], command_map: &mut BTreeMap<u16, MenuCallback>) {

        fn convert_widestring(input: &str) -> Vec<u16> {
            let mut v: Vec<u16> = input.chars().filter_map(|s| {
                use std::convert::TryInto;
                (s as u32).try_into().ok()
            }).collect();
            v.push(0);
            v
        }

        use winapi::um::winuser::{MF_STRING, MF_SEPARATOR, MF_POPUP, MF_MENUBREAK};
        use winapi::shared::basetsd::UINT_PTR;
        use winapi::um::winuser::{CreateMenu, AppendMenuW};

        for item in items.as_ref() {
            match item {
                MenuItem::String(mi) => {
                    if mi.children.as_ref().is_empty() {
                        // no children
                        let command = match mi.callback.as_ref() {
                            None => {
                                0
                            },
                            Some(c) => {
                                let new_command_id = Self::get_new_command_id().min(core::u16::MAX as usize) as u16;
                                command_map.insert(new_command_id, c.clone());
                                new_command_id as usize
                            }
                        };
                        unsafe { AppendMenuW(*menu, MF_STRING, command, convert_widestring(mi.label.as_str()).as_ptr()) };
                    } else {
                        let mut root = unsafe { CreateMenu() };
                        Self::recursive_construct_menu(&mut root, mi.children.as_ref(), command_map);
                        unsafe { AppendMenuW(*menu, MF_POPUP, root as UINT_PTR, convert_widestring(mi.label.as_str()).as_ptr()) };
                    }
                },
                MenuItem::Separator => {
                    unsafe { AppendMenuW(*menu, MF_SEPARATOR, 0, core::ptr::null_mut()); }
                }
                MenuItem::BreakLine => {
                    unsafe { AppendMenuW(*menu, MF_MENUBREAK, 0, core::ptr::null_mut()); }
                }
            }
        }
    }
}

unsafe extern "system" fn WindowProc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    use winapi::um::winuser::DefWindowProcW;

    /*
        WM_CREATE => {
            window.rebuild_display_list(&mut txn, image_cache, initial_resource_updates);
            window.force_synchronize_hit_tester_initial(image_cache);
        },
        WM_DPICHANGED => {

        },
        WM_PAINT => {
            window.render_async(txn, true);
        },
        WM_DPICHANGED => {

        },
        WM_COMMAND => {

        },
    */

    DefWindowProcW(hwnd, msg, wparam, lparam)
}