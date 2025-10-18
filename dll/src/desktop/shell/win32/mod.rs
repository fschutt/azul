#![cfg(target_os = "windows")]
#![allow(non_snake_case)]

//! Win32 implementation of the window shell containing all functions
//! related to running the application

mod dpi;
mod event;
mod gl;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
};
use core::{
    cell::{BorrowError, BorrowMutError, RefCell},
    convert::TryInto,
    ffi::c_void,
    fmt, mem, ptr,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use azul_core::{
    app_resources::{
        AppConfig, DpiScaleFactor, Epoch, GlTextureCache, ImageCache, ImageMask, ImageRef,
        RendererResources, ResourceUpdate,
    },
    callbacks::{DocumentId, DomNodeId, RefAny},
    dom::NodeId,
    events::NodesToCheck,
    gl::OptionGlContextPtr,
    styled_dom::DomId,
    task::{Thread, ThreadId, Timer, TimerId},
    ui_solver::LayoutResult,
    window::{
        CallCallbacksResult, CursorPosition, FullWindowState, LayoutWindow, LogicalPosition, Menu,
        MenuCallback, MenuItem, MonitorVec, MouseCursorType, PhysicalSize, ProcessEventResult,
        RawWindowHandle, ScrollResult, WindowCreateOptions, WindowFrame, WindowId, WindowState,
        WindowsHandle,
    },
    FastBTreeSet, FastHashMap,
};
use azul_css::FloatValue;
use gl_context_loader::GenericGlContext;
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
use winapi::{
    ctypes::wchar_t,
    shared::{
        minwindef::{BOOL, HINSTANCE, LPARAM, LRESULT, TRUE, UINT, WPARAM},
        ntdef::HRESULT,
        windef::{HDC, HGLRC, HMENU, HWND, POINT, RECT},
    },
    um::{
        dwmapi::{DWM_BB_ENABLE, DWM_BLURBEHIND},
        uxtheme::MARGINS,
        winuser::WM_APP,
    },
};

use self::{
    dpi::DpiFunctions,
    gl::{ExtraWglFunctions, ExtraWglFunctionsLoadError, GlFunctions},
};
use super::{CommandMap, MenuTarget, Notifier, AZ_THREAD_TICK, AZ_TICK_REGENERATE_DOM};
use crate::desktop::{
    app::{App, LazyFcCache},
    shell::{event::*, process::*},
    wr_translate::{
        generate_frame, rebuild_display_list, wr_synchronize_updated_images, AsyncHitTester,
    },
};

const AZ_REGENERATE_DOM: u32 = WM_APP + 1;
const AZ_REGENERATE_DISPLAY_LIST: u32 = WM_APP + 2;
const AZ_REDO_HIT_TEST: u32 = WM_APP + 3;
const AZ_GPU_SCROLL_RENDER: u32 = WM_APP + 4;

type TIMERPTR = winapi::shared::basetsd::UINT_PTR;

const CLASS_NAME: &str = "AzulApplicationClass";

pub use super::WR_SHADER_CACHE;

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

#[derive(Debug)]
pub struct AppData {
    pub userdata: App,
    pub active_menus: BTreeMap<MenuTarget, CommandMap>,
    pub windows: BTreeMap<WindowId, Window>,
    // active HWNDS, tracked separately from the AppData
    pub active_hwnds: Rc<RefCell<BTreeSet<HWND>>>,
    pub dwm: Option<DwmFunctions>,
    pub dpi: DpiFunctions,
    pub hinstance: HINSTANCE,
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
            winbase::{INFINITE, WAIT_FAILED},
            wingdi::{wglMakeCurrent, CreateSolidBrush},
            winuser::{
                DispatchMessageW, GetDC, GetForegroundWindow, GetMessageW,
                MsgWaitForMultipleObjects, PeekMessageW, RegisterClassW, ReleaseDC,
                SetProcessDPIAware, TranslateMessage, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, MSG,
                PM_NOREMOVE, PM_NOYIELD, QS_ALLEVENTS, WNDCLASSW,
            },
        },
    };

    let hinstance = unsafe { GetModuleHandleW(ptr::null_mut()) };
    if hinstance.is_null() {
        return Err(WindowsStartupError::NoAppInstance(get_last_error()));
    }

    // Tell windows that this process is DPI-aware
    let dpi = self::dpi::DpiFunctions::init();
    dpi.become_dpi_aware();

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

        let app_data_inner = Rc::new(RefCell::new(AppData {
            hinstance,
            userdata: App {
                data,
                config,
                windows,
                image_cache,
                fc_cache,
            },
            active_menus: BTreeMap::new(),
            windows: BTreeMap::new(),
            active_hwnds: active_hwnds.clone(),
            dwm,
            dpi,
        }));

        let w = Window::create(
            hinstance,
            root_window,
            SharedAppData {
                inner: app_data_inner.clone(),
            },
        )?;

        active_hwnds.try_borrow_mut()?.insert(w.hwnd);
        app_data_inner
            .try_borrow_mut()?
            .windows
            .insert(w.get_id(), w);

        for opts in windows {
            if let Ok(w) = Window::create(
                hinstance,
                opts,
                SharedAppData {
                    inner: app_data_inner.clone(),
                },
            ) {
                active_hwnds.try_borrow_mut()?.insert(w.hwnd);
                app_data_inner
                    .try_borrow_mut()?
                    .windows
                    .insert(w.get_id(), w);
            }
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
            }
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
            while !hwnds
                .iter()
                .any(|hwnd| unsafe { PeekMessageW(&mut dump_msg, *hwnd, 0, 0, PM_NOREMOVE) > 0 })
            {
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

pub fn load_dll(name: &'static str) -> Option<HINSTANCE> {
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

#[derive(Debug, Clone)]
struct SharedAppData {
    inner: Rc<RefCell<AppData>>,
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
        unsafe {
            FreeLibrary(self._dwmapi_dll_handle);
        }
    }
}

pub struct Window {
    /// HWND handle of the plaform window
    pub hwnd: HWND,
    /// See azul-core, stores the entire UI (DOM, CSS styles, layout results, etc.)
    pub internal: LayoutWindow,
    /// OpenGL context handle - None if running in software mode
    pub gl_context: Option<HGLRC>,
    /// OpenGL functions for faster rendering
    pub gl_functions: GlFunctions,
    /// OpenGL context pointer with compiled SVG and FXAA shaders
    pub gl_context_ptr: OptionGlContextPtr,
    /// Main render API that can be used to register and un-register fonts and images
    pub render_api: WrRenderApi,
    /// WebRender renderer implementation (software or hardware)
    pub renderer: Option<WrRenderer>,
    /// Hit-tester, lazily initialized and updated every time the display list changes layout
    pub hit_tester: AsyncHitTester,
    /// ID -> Callback map for the window menu (default: empty map)
    pub menu_bar: Option<WindowsMenuBar>,
    /// ID -> Context menu callbacks (cleared when the context menu closes)
    pub context_menu: Option<CurrentContextMenu>,
    /// Timer ID -> Win32 timer map
    pub timers: BTreeMap<TimerId, TIMERPTR>,
    /// If threads is non-empty, the window will receive a WM_TIMER every 16ms
    pub thread_timer_running: Option<TIMERPTR>,
    /// characters are combined via two following wparam messages
    pub high_surrogate: Option<u16>,
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
        use winapi::um::wingdi::{wglDeleteContext, wglMakeCurrent};

        // drop the layout results first
        self.internal.layout_results = Vec::new();

        unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };

        if let Some(context) = self.gl_context.as_mut() {
            unsafe {
                wglDeleteContext(*context);
            }
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
    pub fn get_id(&self) -> WindowId {
        WindowId {
            id: self.hwnd as i64,
        }
    }

    // Creates a new HWND according to the options
    fn create(
        hinstance: HINSTANCE,
        mut options: WindowCreateOptions,
        mut shared_application_data: SharedAppData,
    ) -> Result<Self, WindowsWindowCreateError> {
        use azul_core::{
            callbacks::PipelineId,
            gl::GlContextPtr,
            window::{
                CursorPosition, FullHitTest, HwAcceleration, LayoutWindowInit, LogicalPosition,
                PhysicalSize, RendererType, ScrollResult, WindowFrame,
            },
        };
        use webrender::{api::ColorF as WrColorF, ProgramCache as WrProgramCache};
        use winapi::{
            shared::windef::POINT,
            um::{
                wingdi::{
                    wglDeleteContext, wglMakeCurrent, GetDeviceCaps, SwapBuffers, LOGPIXELSX,
                    LOGPIXELSY,
                },
                winuser::{
                    CreateWindowExW, DestroyWindow, GetClientRect, GetCursorPos, GetDC,
                    GetWindowRect, ReleaseDC, ScreenToClient, SetMenu, SetWindowPos, ShowWindow,
                    CW_USEDEFAULT, HWND_TOP, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOZORDER, SW_HIDE,
                    SW_MAXIMIZE, SW_MINIMIZE, SW_NORMAL, SW_SHOWNORMAL, WS_CAPTION,
                    WS_EX_ACCEPTFILES, WS_EX_APPWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
                    WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_THICKFRAME,
                },
            },
        };

        use crate::desktop::{
            compositor::Compositor,
            wr_translate::{
                translate_document_id_wr, translate_id_namespace_wr, wr_translate_document_id,
            },
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

        let data_ptr = Box::into_raw(Box::new(shared_application_data.clone()))
            as *mut SharedAppData as *mut c_void;

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
                if options.size_to_content {
                    0
                } else {
                    libm::roundf(options.state.size.dimensions.width) as i32
                }, // width
                if options.size_to_content {
                    0
                } else {
                    libm::roundf(options.state.size.dimensions.height) as i32
                }, // height
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
        let dpi = if let Ok(s) = shared_application_data.inner.try_borrow() {
            unsafe { s.dpi.hwnd_dpi(hwnd) }
        } else {
            96
        };

        let dpi_factor = self::dpi::dpi_to_scale_factor(dpi);

        options.state.size.dpi = dpi;

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

        // LayoutWindow::new() may dispatch OpenGL calls,
        // need to make context current before invoking
        let hdc = unsafe { GetDC(hwnd) };
        if let Some(hrc) = opengl_context.as_mut() {
            unsafe { wglMakeCurrent(hdc, *hrc) };
        }

        // Invoke callback to initialize UI for the first time
        let wr2 = webrender::Renderer::new(
            gl.functions.clone(),
            Box::new(Notifier {}),
            super::default_renderer_options(&options),
            super::WR_SHADER_CACHE,
        );

        let (mut renderer, sender) = match wr2 {
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

        let framebuffer_size =
            WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let pipeline_id = PipelineId::new();
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // hit tester will be empty on startup
        let hit_tester = render_api
            .request_hit_tester(wr_translate_document_id(document_id))
            .resolve();
        let hit_tester_ref = &*hit_tester;

        // lock the SharedAppData in order to
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
            let fc_cache = &mut appdata_lock.userdata.fc_cache;
            let image_cache = &appdata_lock.userdata.image_cache;
            let data = &mut appdata_lock.userdata.data;

            fc_cache.apply_closure(|fc_cache| {
                LayoutWindow::new(
                    LayoutWindowInit {
                        window_create_options: options.clone(),
                        document_id,
                        id_namespace,
                    },
                    data,
                    image_cache,
                    &gl_context_ptr,
                    &mut initial_resource_updates,
                    &crate::desktop::app::CALLBACKS,
                    fc_cache,
                    azul_layout::solver3::do_the_relayout,
                    |window_state, scroll_states, layout_results| {
                        crate::desktop::wr_translate::fullhittest_new_webrender(
                            hit_tester_ref,
                            document_id,
                            window_state.focused_node,
                            layout_results,
                            &window_state.mouse_state.cursor_position,
                            window_state.size.get_hidpi_factor(),
                        )
                    },
                    &mut None, // no debug messages
                )
            })
        };

        // Since the menu bar affects the window size, set it first,
        // before querying the window size again
        let mut menu_bar = None;
        if let Some(m) = internal.get_menu_bar() {
            let mb = WindowsMenuBar::new(m);
            unsafe {
                SetMenu(hwnd, mb._native_ptr);
            }
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
            WindowFrame::Normal => {
                sw_options |= SW_NORMAL;
                hidden_sw_options |= SW_NORMAL;
            }
            WindowFrame::Minimized => {
                sw_options |= SW_MINIMIZE;
                hidden_sw_options |= SW_MINIMIZE;
            }
            WindowFrame::Maximized => {
                sw_options |= SW_MAXIMIZE;
                hidden_sw_options |= SW_MAXIMIZE;
            }
            WindowFrame::Fullscreen => {
                sw_options |= SW_MAXIMIZE;
                hidden_sw_options |= SW_MAXIMIZE;
            }
        }

        unsafe {
            ShowWindow(hwnd, hidden_sw_options);
        }

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
            let fc_cache = &mut appdata_lock.userdata.fc_cache;
            let image_cache = &appdata_lock.userdata.image_cache;
            let size = internal.current_window_state.size.clone();
            let theme = internal.current_window_state.theme;
            let resize_result = fc_cache.apply_closure(|fc_cache| {
                internal.do_quick_resize(
                    &image_cache,
                    &crate::desktop::app::CALLBACKS,
                    azul_layout::solver3::do_the_relayout,
                    fc_cache,
                    &gl_context_ptr,
                    &size,
                    theme,
                )
            });

            wr_synchronize_updated_images(resize_result.updated_images, &mut txn);
        }

        if let Some(hrc) = opengl_context.as_ref() {
            unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
        }

        unsafe {
            ReleaseDC(hwnd, hdc);
        }

        txn.set_document_view(WrDeviceIntRect::from_size(WrDeviceIntSize::new(
            physical_size.width as i32,
            physical_size.height as i32,
        )));
        render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);

        render_api.flush_scene_builder();

        // Build the display list and send it to webrender for the first time
        rebuild_display_list(
            &mut internal,
            &mut render_api,
            &appdata_lock.userdata.image_cache,
            initial_resource_updates,
        );

        render_api.flush_scene_builder();

        generate_frame(&mut internal, &mut render_api, true);

        render_api.flush_scene_builder();

        // Get / store mouse cursor position, now that the window position is final
        let mut cursor_pos: POINT = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut cursor_pos);
        }
        unsafe { ScreenToClient(hwnd, &mut cursor_pos) };
        let cursor_pos_logical = LogicalPosition {
            x: cursor_pos.x as f32 / dpi_factor,
            y: cursor_pos.y as f32 / dpi_factor,
        };
        internal.current_window_state.mouse_state.cursor_position =
            if cursor_pos.x <= 0 || cursor_pos.y <= 0 {
                CursorPosition::Uninitialized
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
            unsafe {
                SetTimer(hwnd, AZ_TICK_REGENERATE_DOM, 200, None);
            }
        }

        use winapi::um::winuser::PostMessageW;
        unsafe {
            PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
        }

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
            let fc_cache = &mut ab.userdata.fc_cache;
            let image_cache = &mut ab.userdata.image_cache;
            let data = &mut ab.userdata.data;
            let config = &ab.userdata.config;

            let ccr = fc_cache.apply_closure(|fc_cache| {
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
                window
                    .internal
                    .current_window_state
                    .mouse_state
                    .mouse_down(),
                window.internal.current_window_state.focused_node,
            );

            let mut new_windows = Vec::new();
            let mut destroyed_windows = Vec::new();

            let ret = process_callback_results(
                ccr,
                &mut window,
                &ntc,
                image_cache,
                fc_cache,
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

            unsafe {
                ReleaseDC(hwnd, hdc);
            }
        }

        unsafe {
            ShowWindow(hwnd, sw_options);
        }

        // NOTE: The window is NOT stored yet
        Ok(window)
    }

    pub fn start_stop_timers(
        &mut self,
        added: FastHashMap<TimerId, Timer>,
        removed: FastBTreeSet<TimerId>,
    ) {
        use winapi::um::winuser::{KillTimer, SetTimer};

        for (id, timer) in added {
            let res = unsafe {
                SetTimer(
                    self.hwnd,
                    id.id,
                    timer.tick_millis().min(u32::MAX as u64) as u32,
                    None,
                )
            };
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

    pub fn start_stop_threads(
        &mut self,
        mut added: FastHashMap<ThreadId, Thread>,
        removed: FastBTreeSet<ThreadId>,
    ) {
        use winapi::um::winuser::{KillTimer, SetTimer};

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

    // Stop all timers that have a NodeId attached to them because in the next
    // frame the NodeId would be invalid, leading to crashes / panics
    pub fn stop_timers_with_node_ids(&mut self) {
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
    pub fn do_system_scroll(&mut self, scroll: ScrollResult) {
        // for scrolled_node in scroll {
        //      self.render_api.scroll_node_with_id();
        //      let scrolled_rect = LogicalRect { origin: scroll_offset, size: visible.size };
        //      if !scrolled_node.scroll_bounds.contains(&scroll_rect) {
        //
        //      }
        // }
    }

    pub fn set_cursor(&mut self, handle: &RawWindowHandle, cursor: MouseCursorType) {
        use winapi::um::winuser::{SetClassLongPtrW, GCLP_HCURSOR};

        if let RawWindowHandle::Windows(win_handle) = handle {
            let hwnd = win_handle.hwnd as HWND;
            unsafe {
                SetClassLongPtrW(
                    hwnd,
                    GCLP_HCURSOR,
                    (win32_translate_cursor(cursor) as isize)
                        .try_into()
                        .unwrap_or(0),
                );
            }
        }
    }

    pub fn set_menu_bar(
        hwnd: HWND,
        old: &mut Option<WindowsMenuBar>,
        menu_bar: Option<&Box<Menu>>,
    ) {
        use winapi::um::winuser::SetMenu;

        let hash = old.as_ref().map(|o| o.hash);

        match (hash, menu_bar) {
            (Some(_), None) => {
                unsafe {
                    SetMenu(hwnd, ptr::null_mut());
                }
                *old = None;
            }
            (None, Some(new)) => {
                let new_menu_bar = WindowsMenuBar::new(new);
                unsafe {
                    SetMenu(hwnd, new_menu_bar._native_ptr);
                }
                *old = Some(new_menu_bar);
            }
            (Some(hash), Some(new)) => {
                if hash != new.get_hash() {
                    let new_menu_bar = WindowsMenuBar::new(new);
                    unsafe {
                        SetMenu(hwnd, new_menu_bar._native_ptr);
                    }
                    *old = Some(new_menu_bar);
                }
            }
            (None, None) => {}
        }
    }

    pub fn create_and_open_context_menu(
        &self,
        context_menu: &Menu,
        hit: &azul_core::hit_test::HitTestItem,
        node_id: DomNodeId,
        active_menus: &mut BTreeMap<MenuTarget, CommandMap>,
    ) {
        use winapi::um::winuser::{
            ClientToScreen, CreatePopupMenu, GetClientRect, SetForegroundWindow, TrackPopupMenu,
            TPM_LEFTALIGN, TPM_TOPALIGN,
        };

        let mut hPopupMenu = unsafe { CreatePopupMenu() };
        let mut callbacks = BTreeMap::new();
        let hidpi_factor = self.internal.current_window_state.size.get_hidpi_factor();

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
        unsafe { GetClientRect(self.hwnd, &mut rect) };

        let mut top_left = POINT {
            x: rect.left,
            y: rect.top,
        };
        unsafe { ClientToScreen(self.hwnd, &mut top_left) };

        let pos = match context_menu.position {
            _ => hit.point_in_viewport, // TODO
        };

        // Store callbacks in the Window's context_menu
        let context_menu_data = CurrentContextMenu {
            callbacks,
            hit_dom_node: node_id,
        };

        unsafe {
            SetForegroundWindow(self.hwnd);
            TrackPopupMenu(
                hPopupMenu,
                align,
                top_left.x + (libm::roundf(pos.x * hidpi_factor) as i32),
                top_left.y + (libm::roundf(pos.y * hidpi_factor) as i32),
                0,
                self.hwnd,
                ptr::null_mut(),
            );
        }
    }

    pub fn swap_buffers(&mut self, handle: &RawWindowHandle) {
        use winapi::um::wingdi::SwapBuffers;

        if let RawWindowHandle::Windows(win_handle) = handle {
            let hwnd = win_handle.hwnd as HWND;
            let hdc = unsafe { winapi::um::winuser::GetDC(hwnd) };
            if !hdc.is_null() {
                unsafe { SwapBuffers(hdc) };
                unsafe { winapi::um::winuser::ReleaseDC(hwnd, hdc) };
            }
        }
    }

    pub fn on_cursor_change(&mut self, prev: Option<MouseCursorType>, cur: MouseCursorType) {
        // Set the cursor
        if let Some(prev) = prev {
            if prev != cur {
                use winapi::um::winuser::{SetClassLongPtrW, GCLP_HCURSOR};
                unsafe {
                    SetClassLongPtrW(
                        self.hwnd,
                        GCLP_HCURSOR,
                        (win32_translate_cursor(cur) as isize)
                            .try_into()
                            .unwrap_or(0),
                    );
                }
            }
        }
    }

    pub fn on_mouse_enter(&mut self, prev: CursorPosition, cur: CursorPosition) {
        use winapi::um::winuser::{TrackMouseEvent, HOVER_DEFAULT, TME_LEAVE, TRACKMOUSEEVENT};

        unsafe {
            TrackMouseEvent(&mut TRACKMOUSEEVENT {
                cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: HOVER_DEFAULT,
            });
        }
    }

    pub fn destroy(
        &mut self,
        userdata: &mut App,
        guard: &GlContextGuard,
        handle: &RawWindowHandle,
        gl_functions: Rc<GenericGlContext>,
    ) {
        // Deallocate any OpenGL resources if needed
    }
}

pub struct GlContextGuard {
    pub hdc: HDC,
    pub context: HGLRC,
}

impl Drop for GlContextGuard {
    fn drop(&mut self) {
        use winapi::um::wingdi::wglMakeCurrent;
        if !self.hdc.is_null() {
            unsafe { wglMakeCurrent(ptr::null_mut(), ptr::null_mut()) };
        }
    }
}

/// Creates an OpenGL 3.2 context using wglCreateContextAttribsARB
fn create_gl_context(
    hwnd: HWND,
    hinstance: HINSTANCE,
    extra: &ExtraWglFunctions,
) -> Result<HGLRC, WindowsOpenGlError> {
    use winapi::um::{
        wingdi::{
            wglCreateContext, wglDeleteContext, wglMakeCurrent, ChoosePixelFormat,
            DescribePixelFormat, SetPixelFormat,
        },
        winuser::{GetDC, ReleaseDC},
    };

    use self::WindowsOpenGlError::*;

    let wglCreateContextAttribsARB = extra
        .wglCreateContextAttribsARB
        .ok_or(OpenGLNotAvailable(get_last_error()))?;

    let wglChoosePixelFormatARB = extra
        .wglChoosePixelFormatARB
        .ok_or(OpenGLNotAvailable(get_last_error()))?;

    let opengl32_dll = load_dll("opengl32.dll").ok_or(OpenGL32DllNotFound(get_last_error()))?;

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
    const WGL_FULL_ACCELERATION_ARB: i32 = 0x2027;
    const WGL_ACCELERATION_ARB: i32 = 0x2003;

    const GL_TRUE: i32 = 1;

    let pixel_format_attribs = [
        WGL_DRAW_TO_WINDOW_ARB,
        GL_TRUE,
        WGL_SUPPORT_OPENGL_ARB,
        GL_TRUE,
        WGL_DOUBLE_BUFFER_ARB,
        GL_TRUE,
        WGL_ACCELERATION_ARB,
        WGL_FULL_ACCELERATION_ARB,
        WGL_PIXEL_TYPE_ARB,
        WGL_TYPE_RGBA_ARB,
        WGL_COLOR_BITS_ARB,
        32,
        WGL_DEPTH_BITS_ARB,
        24,
        WGL_STENCIL_BITS_ARB,
        8,
        0,
    ];

    let mut pixel_format = 0;
    let mut num_formats = 0;
    unsafe {
        (wglChoosePixelFormatARB)(
            hDC,
            pixel_format_attribs.as_ptr(),
            ptr::null_mut(),
            1,
            &mut pixel_format,
            &mut num_formats,
        )
    };
    if num_formats == 0 {
        unsafe {
            ReleaseDC(hwnd, hDC);
        }
        return Err(NoMatchingPixelFormat(get_last_error()));
    }

    let mut pfd: PIXELFORMATDESCRIPTOR = get_default_pfd();

    unsafe {
        DescribePixelFormat(
            hDC,
            pixel_format,
            mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32,
            &mut pfd,
        );
        if SetPixelFormat(hDC, pixel_format, &mut pfd) != TRUE {
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
        WGL_CONTEXT_MAJOR_VERSION_ARB,
        3,
        WGL_CONTEXT_MINOR_VERSION_ARB,
        2,
        WGL_CONTEXT_PROFILE_MASK_ARB,
        WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
        0,
    ];

    let gl32_context =
        unsafe { (wglCreateContextAttribsARB)(hDC, ptr::null_mut(), gl32_attribs.as_ptr()) };
    if gl32_context.is_null() {
        unsafe {
            ReleaseDC(hwnd, hDC);
        }
        return Err(OpenGLNotAvailable(get_last_error()));
    }

    unsafe {
        ReleaseDC(hwnd, hDC);
    }

    return Ok(gl32_context);
}

use winapi::um::wingdi::PIXELFORMATDESCRIPTOR;

fn get_default_pfd() -> PIXELFORMATDESCRIPTOR {
    use winapi::um::wingdi::{
        PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW, PFD_GENERIC_ACCELERATED, PFD_MAIN_PLANE,
        PFD_SUPPORT_COMPOSITION, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA,
    };

    PIXELFORMATDESCRIPTOR {
        nSize: mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
        nVersion: 1,
        dwFlags: {
            PFD_DRAW_TO_WINDOW |        // support window
            PFD_SUPPORT_OPENGL |        // support OpenGL
            PFD_DOUBLEBUFFER // double buffered
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

        use winapi::{
            shared::basetsd::UINT_PTR,
            um::winuser::{
                AppendMenuW, CreateMenu, MF_MENUBREAK, MF_POPUP, MF_SEPARATOR, MF_STRING,
            },
        };

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
    use winapi::um::{
        wingdi::wglMakeCurrent,
        winuser::{
            DefWindowProcW, GetWindowLongPtrW, PostMessageW, PostQuitMessage, SetWindowLongPtrW,
            CREATESTRUCTW, GWLP_USERDATA, VK_F4, WHEEL_DELTA, WM_ACTIVATE, WM_CHAR, WM_COMMAND,
            WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_HSCROLL,
            WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
            WM_MBUTTONUP, WM_MOUSELEAVE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCHITTEST,
            WM_NCMOUSELEAVE, WM_PAINT, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETFOCUS, WM_SIZE,
            WM_SIZING, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_VSCROLL,
            WM_WINDOWPOSCHANGED,
        },
    };

    use crate::desktop::wr_translate::wr_translate_document_id;

    return if msg == WM_NCCREATE {
        let createstruct: *mut CREATESTRUCTW = mem::transmute(lparam);
        let data_ptr = (*createstruct).lpCreateParams;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, mem::transmute(data_ptr));
        DefWindowProcW(hwnd, msg, wparam, lparam)
    } else {
        let shared_application_data: *mut SharedAppData =
            mem::transmute(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
        if shared_application_data == ptr::null_mut() {
            // message fired before WM_NCCREATE: ignore
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let shared_application_data: &mut SharedAppData = &mut *shared_application_data;

        let mut app_borrow = match shared_application_data.inner.try_borrow_mut() {
            Ok(b) => b,
            Err(e) => {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
        };

        let mut app_borrow = &mut *app_borrow;
        let hwnd_key = WindowId { id: hwnd as i64 };

        let msg_start = std::time::Instant::now();

        let r = match msg {
            AZ_REGENERATE_DOM => {
                use azul_core::events::NodesToCheck;
                // TODO: StyleAndLayoutChanges no longer exists - need to reimplement with new API
                // use azul_layout::window_state::{NodesToCheck, StyleAndLayoutChanges};

                let mut ret = ProcessEventResult::DoNothing;

                // borrow checker :|
                let ab = &mut *app_borrow;
                let windows = &mut ab.windows;
                let fc_cache = &mut ab.userdata.fc_cache;
                let data = &mut ab.userdata.data;
                let image_cache = &mut ab.userdata.image_cache;

                if let Some(current_window) = windows.get_mut(&hwnd_key) {
                    use winapi::um::winuser::{GetDC, ReleaseDC};

                    let hDC = GetDC(hwnd);

                    let gl_context = match current_window.gl_context {
                        Some(c) => {
                            if !hDC.is_null() {
                                wglMakeCurrent(hDC, c);
                            }
                        }
                        None => {}
                    };

                    let mut current_program = [0_i32];

                    {
                        let mut gl = &mut current_window.gl_functions.functions;
                        gl.get_integer_v(
                            gl_context_loader::gl::CURRENT_PROGRAM,
                            (&mut current_program[..]).into(),
                        );
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
                            internal.get_dpi_scale_factor(),
                            &crate::desktop::app::CALLBACKS,
                            fc_cache,
                            azul_layout::solver3::do_the_relayout,
                            |window_state, scroll_states, layout_results| {
                                crate::desktop::wr_translate::fullhittest_new_webrender(
                                    &*hit_tester.resolve(),
                                    document_id,
                                    window_state.focused_node,
                                    layout_results,
                                    &window_state.mouse_state.cursor_position,
                                    window_state.size.get_hidpi_factor(),
                                )
                            },
                            &mut None,
                        );
                    });

                    // stop timers that have a DomNodeId attached to them
                    current_window.stop_timers_with_node_ids();

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
                        current_window.internal.get_menu_bar(),
                    );

                    // rebuild the display list and send it
                    rebuild_display_list(
                        &mut current_window.internal,
                        &mut current_window.render_api,
                        image_cache,
                        resource_updates,
                    );

                    current_window.render_api.flush_scene_builder();

                    let wr_document_id =
                        wr_translate_document_id(current_window.internal.document_id);
                    current_window.hit_tester = AsyncHitTester::Requested(
                        current_window.render_api.request_hit_tester(wr_document_id),
                    );

                    let hit_test = crate::desktop::wr_translate::fullhittest_new_webrender(
                        &*current_window.hit_tester.resolve(),
                        current_window.internal.document_id,
                        current_window.internal.current_window_state.focused_node,
                        &current_window.internal.layout_results,
                        &current_window
                            .internal
                            .current_window_state
                            .mouse_state
                            .cursor_position,
                        current_window
                            .internal
                            .current_window_state
                            .size
                            .get_hidpi_factor(),
                    );

                    current_window.internal.previous_window_state = None;
                    current_window.internal.current_window_state.last_hit_test = hit_test;

                    let mut nodes_to_check = NodesToCheck::simulated_mouse_move(
                        &current_window.internal.current_window_state.last_hit_test,
                        current_window.internal.current_window_state.focused_node,
                        current_window
                            .internal
                            .current_window_state
                            .mouse_state
                            .mouse_down(),
                    );

                    // TODO: StyleAndLayoutChanges no longer exists - need to reimplement with new
                    // API let mut style_layout_changes =
                    // StyleAndLayoutChanges::new(     &nodes_to_check,
                    //     &mut current_window.internal.layout_results,
                    //     &image_cache,
                    //     &mut current_window.internal.renderer_resources,
                    //     current_window
                    //         .internal
                    //         .current_window_state
                    //         .size
                    //         .get_layout_size(),
                    //     &current_window.internal.document_id,
                    //     None,
                    //     None,
                    //     &None,
                    //     azul_layout::solver3::do_the_relayout,
                    // );

                    PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                }

                mem::drop(app_borrow);
                0
            }
            AZ_REDO_HIT_TEST => {
                let mut ret = ProcessEventResult::DoNothing;

                let cur_hwnd;

                let hinstance = app_borrow.hinstance;
                let windows = &mut app_borrow.windows;
                let userdata = &mut app_borrow.userdata;

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
                            }
                            None => {}
                        };

                        let mut current_program = [0_i32];

                        {
                            let mut gl = &mut current_window.gl_functions.functions;
                            gl.get_integer_v(
                                gl_context_loader::gl::CURRENT_PROGRAM,
                                (&mut current_program[..]).into(),
                            );
                        }

                        let guard = GlContextGuard {
                            hdc: hDC,
                            context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                        };

                        let handle = RawWindowHandle::Windows(WindowsHandle {
                            hwnd: current_window.hwnd as *mut c_void,
                            hinstance: hinstance as *mut c_void,
                        });

                        ret = crate::desktop::shell::event::az_redo_hit_test(
                            current_window,
                            userdata,
                            &guard,
                            &handle,
                        );

                        let mut gl = &mut current_window.gl_functions.functions;
                        gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                        gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                        gl.use_program(current_program[0] as u32);

                        mem::drop(guard);
                        if !hDC.is_null() {
                            ReleaseDC(cur_hwnd, hDC);
                        }
                        0
                    }
                    None => {
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    }
                };

                let hinstance = app_borrow.hinstance;
                mem::drop(app_borrow);

                create_windows(hinstance, shared_application_data, new_windows);
                let mut app_borrow = shared_application_data.inner.try_borrow_mut().unwrap();
                let mut ab = &mut *app_borrow;

                destroy_windows(ab, destroyed_windows);

                match ret {
                    ProcessEventResult::DoNothing => {}
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                        PostMessageW(cur_hwnd, AZ_REGENERATE_DOM, 0, 0);
                    }
                    ProcessEventResult::ShouldRegenerateDomAllWindows => {
                        for window in app_borrow.windows.values() {
                            PostMessageW(window.hwnd, AZ_REGENERATE_DOM, 0, 0);
                        }
                    }
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                        PostMessageW(cur_hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                    }
                    ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                        if let Some(w) = app_borrow.windows.get_mut(&hwnd_key) {
                            // TODO: submit display list, wait for new hit-tester and update
                            // hit-test results
                            w.internal.previous_window_state =
                                Some(w.internal.current_window_state.clone());
                            PostMessageW(cur_hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                            PostMessageW(cur_hwnd, AZ_REDO_HIT_TEST, 0, 0);
                        }
                    }
                    ProcessEventResult::ShouldReRenderCurrentWindow => {
                        PostMessageW(cur_hwnd, AZ_GPU_SCROLL_RENDER, 0, 0);
                    }
                }

                mem::drop(app_borrow);
                0
            }
            AZ_REGENERATE_DISPLAY_LIST => {
                use winapi::um::winuser::InvalidateRect;

                let image_cache = &app_borrow.userdata.image_cache;
                let windows = &mut app_borrow.windows;

                if let Some(current_window) = windows.get_mut(&hwnd_key) {
                    rebuild_display_list(
                        &mut current_window.internal,
                        &mut current_window.render_api,
                        image_cache,
                        Vec::new(), // no resource updates
                    );

                    let wr_document_id =
                        wr_translate_document_id(current_window.internal.document_id);
                    current_window.hit_tester = AsyncHitTester::Requested(
                        current_window.render_api.request_hit_tester(wr_document_id),
                    );

                    generate_frame(
                        &mut current_window.internal,
                        &mut current_window.render_api,
                        true,
                    );

                    PostMessageW(hwnd, WM_PAINT, 0, 0);
                    mem::drop(app_borrow);
                    0
                } else {
                    mem::drop(app_borrow);
                    -1
                }
            }
            AZ_GPU_SCROLL_RENDER => {
                match app_borrow.windows.get_mut(&hwnd_key) {
                    Some(current_window) => {
                        generate_frame(
                            &mut current_window.internal,
                            &mut current_window.render_api,
                            false,
                        );

                        PostMessageW(hwnd, WM_PAINT, 0, 0);
                    }
                    None => {}
                }

                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CREATE => {
                if let Ok(mut o) = app_borrow.active_hwnds.try_borrow_mut() {
                    o.insert(hwnd);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ACTIVATE => {
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ERASEBKGND => {
                mem::drop(app_borrow);
                1
            }
            WM_SETFOCUS => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_set_focus(current_window, userdata, &guard, &handle);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                    mem::drop(app_borrow);
                    0
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_KILLFOCUS => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_kill_focus(current_window, userdata, &guard, &handle);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                    mem::drop(app_borrow);
                    0
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_MOUSEMOVE => {
                use azul_core::geom::LogicalPosition;
                use winapi::shared::windowsx::{GET_X_LPARAM, GET_Y_LPARAM};

                let x = GET_X_LPARAM(lparam);
                let y = GET_Y_LPARAM(lparam);
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let dpi_factor = current_window
                        .internal
                        .current_window_state
                        .size
                        .get_hidpi_factor();

                    let newpos = LogicalPosition::new(x as f32 / dpi_factor, y as f32 / dpi_factor);

                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_mousemove(current_window, userdata, &guard, &handle, newpos);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                    mem::drop(app_borrow);
                    0
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if msg == WM_SYSKEYDOWN && wparam as i32 == VK_F4 {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                } else {
                    let hinstance = app_borrow.hinstance;
                    let mut userdata = &mut app_borrow.userdata;

                    if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                        if let Some((scancode, vk)) =
                            self::event::process_key_params(wparam, lparam)
                        {
                            use winapi::um::winuser::SendMessageW;

                            let guard = GlContextGuard {
                                hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                                context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                            };

                            let handle = RawWindowHandle::Windows(WindowsHandle {
                                hwnd: hwnd as *mut c_void,
                                hinstance: hinstance as *mut c_void,
                            });

                            let ret =
                                wm_keydown(current_window, userdata, &guard, &handle, scancode, vk);

                            // Now process the result without borrowing conflicts
                            let windows = &mut app_borrow.windows;
                            userdata = &mut app_borrow.userdata;

                            let r = handle_process_event_result(
                                ret, windows, hwnd_key, userdata, &guard, &handle,
                            );

                            mem::drop(guard);
                            mem::drop(app_borrow);

                            // NOTE: due to a Win32 bug, the WM_CHAR message gets sent immediately
                            // after the WM_KEYDOWN: this would mess
                            // with the event handling in the window state
                            // code (the window state code expects events to arrive in logical
                            // order)
                            //
                            // So here we use SendMessage instead of PostMessage in order to
                            // immediately call AZ_REDO_HIT_TEST
                            // (instead of posting to the windows message queue).
                            SendMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);

                            0
                        } else {
                            mem::drop(app_borrow);
                            DefWindowProcW(hwnd, msg, wparam, lparam)
                        }
                    } else {
                        mem::drop(app_borrow);
                        DefWindowProcW(hwnd, msg, wparam, lparam)
                    }
                }
            }
            WM_CHAR | WM_SYSCHAR => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

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
                            let guard = GlContextGuard {
                                hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                                context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                            };

                            let handle = RawWindowHandle::Windows(WindowsHandle {
                                hwnd: hwnd as *mut c_void,
                                hinstance: hinstance as *mut c_void,
                            });

                            let ret = wm_char(current_window, userdata, &guard, &handle, c);

                            // Now process the result without borrowing conflicts
                            let windows = &mut app_borrow.windows;
                            userdata = &mut app_borrow.userdata;

                            let r = handle_process_event_result(
                                ret, windows, hwnd_key, userdata, &guard, &handle,
                            );

                            mem::drop(guard);
                            mem::drop(app_borrow);
                            0
                        } else {
                            mem::drop(app_borrow);
                            DefWindowProcW(hwnd, msg, wparam, lparam)
                        }
                    } else {
                        mem::drop(app_borrow);
                        DefWindowProcW(hwnd, msg, wparam, lparam)
                    }
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                use self::event::process_key_params;

                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some((scancode, vk)) = process_key_params(wparam, lparam) {
                    if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                        let guard = GlContextGuard {
                            hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                            context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                        };

                        let handle = RawWindowHandle::Windows(WindowsHandle {
                            hwnd: hwnd as *mut c_void,
                            hinstance: hinstance as *mut c_void,
                        });

                        let ret = wm_keyup(current_window, userdata, &guard, &handle, scancode, vk);

                        // Now process the result without borrowing conflicts
                        let windows = &mut app_borrow.windows;
                        userdata = &mut app_borrow.userdata;

                        let r = handle_process_event_result(
                            ret, windows, hwnd_key, userdata, &guard, &handle,
                        );

                        mem::drop(guard);
                        mem::drop(app_borrow);
                        0
                    } else {
                        mem::drop(app_borrow);
                        DefWindowProcW(hwnd, msg, wparam, lparam)
                    }
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_MOUSELEAVE => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_mouseleave(current_window, userdata, &guard, &handle);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                    mem::drop(app_borrow);
                    0
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_RBUTTONDOWN => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_rbuttondown(current_window, userdata, &guard, &handle);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_RBUTTONUP => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;
                let mut active_menus = &mut app_borrow.active_menus;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_rbuttonup(current_window, userdata, &guard, &handle, active_menus);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MBUTTONDOWN => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_mbuttondown(current_window, userdata, &guard, &handle);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MBUTTONUP => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_mbuttonup(current_window, userdata, &guard, &handle);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_LBUTTONDOWN => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_lbuttondown(current_window, userdata, &guard, &handle);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_LBUTTONUP => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;
                let mut active_menus = &mut app_borrow.active_menus;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_lbuttonup(current_window, userdata, &guard, &handle, active_menus);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOUSEWHEEL => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let value = (wparam >> 16) as i16;
                    let value = value as i32;
                    let value = value as f32 / WHEEL_DELTA as f32;

                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_mousewheel(current_window, userdata, &guard, &handle, value);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                    mem::drop(app_borrow);
                    0
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_DPICHANGED => {
                use winapi::shared::minwindef::LOWORD;

                let dpi = LOWORD(wparam as u32);
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let ret = wm_dpichanged(current_window, userdata, &guard, &handle, dpi as u32);

                    // Now process the result without borrowing conflicts
                    let windows = &mut app_borrow.windows;
                    userdata = &mut app_borrow.userdata;

                    let r = handle_process_event_result(
                        ret, windows, hwnd_key, userdata, &guard, &handle,
                    );

                    mem::drop(guard);
                }

                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_SIZE => {
                use azul_core::window::{PhysicalSize, WindowFrame};
                use winapi::{
                    shared::minwindef::{HIWORD, LOWORD},
                    um::winuser::{
                        SIZE_MAXIMIZED, SIZE_MINIMIZED, SIZE_RESTORED, SWP_NOSIZE, WINDOWPOS,
                    },
                };

                let new_width = LOWORD(lparam as u32);
                let new_height = HIWORD(lparam as u32);
                let new_size = PhysicalSize {
                    width: new_width as u32,
                    height: new_height as u32,
                };

                let fc_cache = &mut app_borrow.userdata.fc_cache;
                let windows = &mut app_borrow.windows;
                let image_cache = &app_borrow.userdata.image_cache;

                if let Some(current_window) = windows.get_mut(&hwnd_key) {
                    fc_cache.apply_closure(|fc_cache| {
                        use winapi::um::winuser::{GetDC, ReleaseDC};

                        let mut new_window_state =
                            current_window.internal.current_window_state.clone();
                        new_window_state.size.dimensions =
                            new_size.to_logical(new_window_state.size.get_hidpi_factor());

                        match wparam {
                            SIZE_MAXIMIZED => {
                                new_window_state.flags.frame = WindowFrame::Maximized;
                            }
                            SIZE_MINIMIZED => {
                                new_window_state.flags.frame = WindowFrame::Minimized;
                            }
                            SIZE_RESTORED => {
                                new_window_state.flags.frame = WindowFrame::Normal;
                            }
                            _ => {}
                        }

                        let hDC = GetDC(hwnd);

                        let gl_context = match current_window.gl_context {
                            Some(c) => {
                                if !hDC.is_null() {
                                    wglMakeCurrent(hDC, c);
                                }
                            }
                            None => {}
                        };

                        let mut current_program = [0_i32];

                        {
                            let mut gl = &mut current_window.gl_functions.functions;
                            gl.get_integer_v(
                                gl_context_loader::gl::CURRENT_PROGRAM,
                                (&mut current_program[..]).into(),
                            );
                        }

                        let resize_result = current_window.internal.do_quick_resize(
                            &image_cache,
                            &crate::desktop::app::CALLBACKS,
                            azul_layout::solver3::do_the_relayout,
                            fc_cache,
                            &current_window.gl_context_ptr,
                            &new_window_state.size,
                            new_window_state.theme,
                        );

                        let mut txn = WrTransaction::new();
                        wr_synchronize_updated_images(resize_result.updated_images, &mut txn);

                        let mut gl = &mut current_window.gl_functions.functions;
                        gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
                        gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
                        gl.use_program(current_program[0] as u32);

                        wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                        if !hDC.is_null() {
                            ReleaseDC(hwnd, hDC);
                        }

                        current_window.internal.previous_window_state =
                            Some(current_window.internal.current_window_state.clone());
                        current_window.internal.current_window_state = new_window_state;

                        txn.set_document_view(WrDeviceIntRect::from_size(WrDeviceIntSize::new(
                            new_width as i32,
                            new_height as i32,
                        )));
                        current_window.render_api.send_transaction(
                            wr_translate_document_id(current_window.internal.document_id),
                            txn,
                        );

                        rebuild_display_list(
                            &mut current_window.internal,
                            &mut current_window.render_api,
                            image_cache,
                            Vec::new(),
                        );

                        let wr_document_id =
                            wr_translate_document_id(current_window.internal.document_id);
                        current_window.hit_tester = AsyncHitTester::Requested(
                            current_window.render_api.request_hit_tester(wr_document_id),
                        );

                        generate_frame(
                            &mut current_window.internal,
                            &mut current_window.render_api,
                            true,
                        );
                    });

                    mem::drop(app_borrow);
                    0
                } else {
                    mem::drop(app_borrow);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_NCHITTEST => {
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_PAINT => {
                use winapi::um::{
                    wingdi::SwapBuffers,
                    winuser::{GetClientRect, GetDC, ReleaseDC},
                };

                // Assuming that the display list has been submitted and the
                // scene on the background thread has been rebuilt, now tell
                // webrender to pain the scene

                let hDC = GetDC(hwnd);
                if hDC.is_null() {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }

                let windows = &mut app_borrow.windows;
                let mut current_window = match windows.get_mut(&hwnd_key) {
                    Some(s) => s,
                    None => {
                        // message fired before window was created: ignore
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    }
                };

                let gl_context = match current_window.gl_context {
                    Some(s) => s,
                    None => {
                        // TODO: software rendering
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    }
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
                gl.get_integer_v(
                    gl_context_loader::gl::CURRENT_PROGRAM,
                    (&mut current_program[..]).into(),
                );

                let framebuffer_size =
                    WrDeviceIntSize::new(rect.width() as i32, rect.height() as i32);

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
            }
            WM_TIMER => {
                use winapi::um::winuser::{GetDC, ReleaseDC};

                let hinstance = app_borrow.hinstance;
                let windows = &mut app_borrow.windows;
                let data = &mut app_borrow.userdata.data;
                let image_cache = &mut app_borrow.userdata.image_cache;
                let fc_cache = &mut app_borrow.userdata.fc_cache;
                let config = &app_borrow.userdata.config;

                let mut ret = ProcessEventResult::DoNothing;
                let mut new_windows = Vec::new();
                let mut destroyed_windows = Vec::new();

                let r = match wparam {
                    AZ_TICK_REGENERATE_DOM => {
                        // re-load the layout() callback
                        PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
                        mem::drop(app_borrow);
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    }
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
                                    }
                                    None => {}
                                };

                                let mut current_program = [0_i32];

                                {
                                    let mut gl = &mut current_window.gl_functions.functions;
                                    gl.get_integer_v(
                                        gl_context_loader::gl::CURRENT_PROGRAM,
                                        (&mut current_program[..]).into(),
                                    );
                                }

                                let guard = GlContextGuard {
                                    hdc: hDC,
                                    context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                                };

                                let handle = RawWindowHandle::Windows(WindowsHandle {
                                    hwnd: current_window.hwnd as *mut c_void,
                                    hinstance: hinstance as *mut c_void,
                                });

                                ret = process_threads(
                                    &handle,
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

                                mem::drop(guard);
                                if !hDC.is_null() {
                                    ReleaseDC(hwnd, hDC);
                                }
                            }
                            None => {
                                mem::drop(app_borrow);
                                return DefWindowProcW(hwnd, msg, wparam, lparam);
                            }
                        }
                    }
                    id => {
                        // run timer with ID "id"
                        match windows.get_mut(&hwnd_key) {
                            Some(current_window) => {
                                let hDC = GetDC(hwnd);

                                let gl_context = match current_window.gl_context {
                                    Some(c) => {
                                        if !hDC.is_null() {
                                            wglMakeCurrent(hDC, c);
                                        }
                                    }
                                    None => {}
                                };

                                let mut current_program = [0_i32];

                                {
                                    let mut gl = &mut current_window.gl_functions.functions;
                                    gl.get_integer_v(
                                        gl_context_loader::gl::CURRENT_PROGRAM,
                                        (&mut current_program[..]).into(),
                                    );
                                }

                                let guard = GlContextGuard {
                                    hdc: hDC,
                                    context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                                };

                                let handle = RawWindowHandle::Windows(WindowsHandle {
                                    hwnd: current_window.hwnd as *mut c_void,
                                    hinstance: hinstance as *mut c_void,
                                });

                                ret = process_timer(
                                    id,
                                    &handle,
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

                                mem::drop(guard);
                                if !hDC.is_null() {
                                    ReleaseDC(hwnd, hDC);
                                }
                            }
                            None => {
                                mem::drop(app_borrow);
                                return DefWindowProcW(hwnd, msg, wparam, lparam);
                            }
                        }
                    }
                };

                // create_windows needs to clone the SharedAppData RefCell
                // drop the borrowed variables and restore them immediately after
                let hinstance = app_borrow.hinstance;
                mem::drop(app_borrow);
                create_windows(hinstance, shared_application_data, new_windows);
                let mut app_borrow = shared_application_data.inner.try_borrow_mut().unwrap();
                destroy_windows(&mut *app_borrow, destroyed_windows);

                match ret {
                    ProcessEventResult::DoNothing => {}
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                        PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
                    }
                    ProcessEventResult::ShouldRegenerateDomAllWindows => {
                        for window in app_borrow.windows.values() {
                            PostMessageW(window.hwnd, AZ_REGENERATE_DOM, 0, 0);
                        }
                    }
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                        PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                    }
                    ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                        if let Some(w) = app_borrow.windows.get_mut(&hwnd_key) {
                            w.internal.previous_window_state =
                                Some(w.internal.current_window_state.clone());
                            // TODO: submit display list, wait for new hit-tester and update
                            // hit-test results
                            PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                            PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                        }
                    }
                    ProcessEventResult::ShouldReRenderCurrentWindow => {
                        PostMessageW(hwnd, AZ_GPU_SCROLL_RENDER, 0, 0);
                    }
                }

                mem::drop(app_borrow);
                0
            }
            WM_COMMAND => {
                use winapi::shared::minwindef::{HIWORD, LOWORD};

                let hiword = HIWORD(wparam.min(core::u32::MAX as usize) as u32);
                let loword = LOWORD(wparam.min(core::u32::MAX as usize) as u32);

                // assert that the command came from a menu
                if hiword != 0 {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }

                let hinstance = app_borrow.hinstance;
                let windows = &mut app_borrow.windows;
                let data = &mut app_borrow.userdata.data;
                let image_cache = &mut app_borrow.userdata.image_cache;
                let fc_cache = &mut app_borrow.userdata.fc_cache;
                let config = &app_borrow.userdata.config;

                // execute menu callback
                if let Some(current_window) = windows.get_mut(&hwnd_key) {
                    use azul_core::{
                        styled_dom::NodeHierarchyItemId,
                        window::{RawWindowHandle, WindowsHandle},
                    };

                    let mut ret = ProcessEventResult::DoNothing;
                    let mut new_windows = Vec::new();
                    let mut destroyed_windows = Vec::new();

                    let window_handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut _,
                        hinstance: hinstance as *mut _,
                    });

                    let ntc = NodesToCheck::empty(
                        current_window
                            .internal
                            .current_window_state
                            .mouse_state
                            .mouse_down(),
                        current_window.internal.current_window_state.focused_node,
                    );

                    let call_callback_result = {
                        let mb = &mut current_window.menu_bar;
                        let internal = &mut current_window.internal;
                        let context_menu = current_window.context_menu.as_mut();
                        let gl_context_ptr = &current_window.gl_context_ptr;

                        if let Some(menu_callback) =
                            mb.as_mut().and_then(|m| m.callbacks.get_mut(&loword))
                        {
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
                            fc_cache,
                            &mut new_windows,
                            &mut destroyed_windows,
                        );
                    };

                    // same as invoke_timers(), invoke_threads(), ...

                    mem::drop(app_borrow);
                    create_windows(hinstance, shared_application_data, new_windows);
                    let mut app_borrow = shared_application_data.inner.try_borrow_mut().unwrap();
                    destroy_windows(&mut *app_borrow, destroyed_windows);

                    match ret {
                        ProcessEventResult::DoNothing => {}
                        ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                            PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
                        }
                        ProcessEventResult::ShouldRegenerateDomAllWindows => {
                            for window in app_borrow.windows.values() {
                                PostMessageW(window.hwnd, AZ_REGENERATE_DOM, 0, 0);
                            }
                        }
                        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                            PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                        }
                        ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                            if let Some(w) = app_borrow.windows.get_mut(&hwnd_key) {
                                w.internal.previous_window_state =
                                    Some(w.internal.current_window_state.clone());
                                // TODO: submit display list, wait for new hit-tester and update
                                // hit-test results
                                PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                                PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                            }
                        }
                        ProcessEventResult::ShouldReRenderCurrentWindow => {
                            PostMessageW(hwnd, AZ_GPU_SCROLL_RENDER, 0, 0);
                        }
                    }

                    mem::drop(app_borrow);
                    return 0;
                } else {
                    mem::drop(app_borrow);
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
            }
            WM_QUIT => {
                let hinstance = app_borrow.hinstance;
                let mut userdata = &mut app_borrow.userdata;

                if let Some(current_window) = app_borrow.windows.get_mut(&hwnd_key) {
                    let guard = GlContextGuard {
                        hdc: unsafe { winapi::um::winuser::GetDC(hwnd) },
                        context: current_window.gl_context.unwrap_or(ptr::null_mut()),
                    };

                    let handle = RawWindowHandle::Windows(WindowsHandle {
                        hwnd: hwnd as *mut c_void,
                        hinstance: hinstance as *mut c_void,
                    });

                    let functions = current_window.gl_functions.functions.clone();

                    wm_quit(current_window, userdata, &guard, &handle, functions);

                    mem::drop(guard);
                }
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DESTROY => {
                use winapi::um::winuser::{GetDC, ReleaseDC};

                // make OpenGL context current in case there are
                // OpenGL objects stored in the windows' RefAny data

                let mut windows_is_empty = false;

                if let Ok(mut o) = app_borrow.active_hwnds.try_borrow_mut() {
                    o.remove(&hwnd);
                    windows_is_empty = o.is_empty();
                }

                if let Some(mut current_window) =
                    app_borrow.windows.remove(&WindowId { id: hwnd as i64 })
                {
                    let hDC = GetDC(hwnd);
                    if let Some(c) = current_window.gl_context {
                        if !hDC.is_null() {
                            wglMakeCurrent(hDC, c);
                        }
                    }

                    // destruct the window data
                    let mut window_data =
                        Box::from_raw(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SharedAppData);

                    // if this window was the last window, the RefAny data
                    // should be dropped here, while the OpenGL context
                    // is still current!
                    mem::drop(window_data);
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
                } else {
                    mem::drop(app_borrow);
                }

                if windows_is_empty {
                    PostQuitMessage(0);
                }

                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => {
                mem::drop(app_borrow);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        };

        // TODO: performance benchmark here!

        r
    };
}

fn create_windows(hinstance: HINSTANCE, app: &mut SharedAppData, new: Vec<WindowCreateOptions>) {
    for opts in new {
        if let Ok(w) = Window::create(hinstance, opts, app.clone()) {
            if let Ok(mut a) = app.inner.try_borrow_mut() {
                a.windows.insert(w.get_id(), w);
            }
        }
    }
}

fn destroy_windows(app: &mut AppData, old: Vec<WindowId>) {
    use winapi::um::winuser::{PostMessageW, WM_QUIT};
    for window in old {
        if let Some(w) = app.windows.get(&window) {
            unsafe {
                PostMessageW(w.hwnd, WM_QUIT, 0, 0);
            }
        }
    }
}

pub(crate) fn synchronize_window_state_with_os(window: &Window) {
    // TODO: window.set_title
}

fn send_resource_updates(render_api: &mut WrRenderApi, resource_updates: Vec<ResourceUpdate>) {}

// translates MouseCursorType to a builtin IDC_* value
// note: taken from https://github.com/rust-windowing/winit/blob/1c4d6e7613c3a3870cecb4cfa0eecc97409d45ff/src/platform_impl/windows/util.rs#L200
const fn win32_translate_cursor(input: MouseCursorType) -> *const wchar_t {
    use azul_core::window::MouseCursorType::*;
    use winapi::um::winuser;

    match input {
        Arrow | Default => winuser::IDC_ARROW,
        Hand => winuser::IDC_HAND,
        Crosshair => winuser::IDC_CROSS,
        Text | VerticalText => winuser::IDC_IBEAM,
        NotAllowed | NoDrop => winuser::IDC_NO,
        Grab | Grabbing | Move | AllScroll => winuser::IDC_SIZEALL,
        EResize | WResize | EwResize | ColResize => winuser::IDC_SIZEWE,
        NResize | SResize | NsResize | RowResize => winuser::IDC_SIZENS,
        NeResize | SwResize | NeswResize => winuser::IDC_SIZENESW,
        NwResize | SeResize | NwseResize => winuser::IDC_SIZENWSE,
        Wait => winuser::IDC_WAIT,
        Progress => winuser::IDC_APPSTARTING,
        Help => winuser::IDC_HELP,
        _ => winuser::IDC_ARROW,
    }
}
