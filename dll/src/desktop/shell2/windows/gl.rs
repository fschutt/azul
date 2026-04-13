//! Windows OpenGL function loading via `wglGetProcAddress` and `opengl32.dll`,
//! plus WGL extension function bootstrapping via a dummy context.

use alloc::rc::Rc;
use core::{fmt, mem, ptr};

use gl_context_loader::GenericGlContext;
use crate::desktop::shell2::common::gl_loader::load_gl_context;
use winapi::shared::{
    minwindef::{BOOL, HINSTANCE, LOWORD, TRUE},
    windef::{HDC, HGLRC},
};

use super::{
    dlopen::{encode_wide, Win32Libraries, HWND, POINT, RECT, WNDCLASSW},
    wcreate::CLASS_NAME,
};

/// OpenGL functions from `wglGetProcAddress` OR loaded from `opengl32.dll`.
pub struct GlFunctions {
    pub _opengl32_dll_handle: Option<HINSTANCE>,
    pub functions: Rc<GenericGlContext>, // implements Rc<dyn gleam::Gl>!
}

impl fmt::Debug for GlFunctions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self._opengl32_dll_handle.map(|s| s as *const () as usize).fmt(f)?;
        Ok(())
    }
}

impl GlFunctions {
    /// Initializes the DLL, but does not load the functions yet.
    pub fn initialize() -> Self {
        // zero-initialize all function pointers
        let context: GenericGlContext = unsafe { mem::zeroed() };

        let opengl32_dll = super::load_dll("opengl32.dll").map(|h| h as HINSTANCE);

        Self {
            _opengl32_dll_handle: opengl32_dll,
            functions: Rc::new(context),
        }
    }

    /// Assuming the OpenGL context is current, loads the OpenGL function pointers.
    pub fn load(&mut self) {
        let opengl32_dll = self._opengl32_dll_handle;
        self.functions = Rc::new(load_gl_context(|s| {
            use winapi::um::{libloaderapi::GetProcAddress, wingdi::wglGetProcAddress};

            let mut func_name = super::encode_ascii(s);
            let addr1 = unsafe { wglGetProcAddress(func_name.as_mut_ptr() as *const i8) };
            (if addr1 != ptr::null_mut() {
                addr1
            } else {
                if let Some(opengl32_dll) = opengl32_dll {
                    unsafe { GetProcAddress(opengl32_dll, func_name.as_mut_ptr() as *const i8) }
                } else {
                    addr1
                }
            }) as *mut gl_context_loader::c_void
        }));
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

/// Extra WGL extension functions loaded via a dummy OpenGL context.
#[derive(Default)]
pub struct ExtraWglFunctions {
    pub wglCreateContextAttribsARB: Option<extern "system" fn(HDC, HGLRC, *const i32) -> HGLRC>,
    pub wglSwapIntervalEXT: Option<extern "system" fn(i32) -> i32>,
    pub wglChoosePixelFormatARB:
        Option<extern "system" fn(HDC, *const i32, *const f32, u32, *mut i32, *mut u32) -> BOOL>,
}

impl fmt::Debug for ExtraWglFunctions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.wglCreateContextAttribsARB.map(|p| p as *const () as usize).fmt(f)?;
        self.wglSwapIntervalEXT.map(|p| p as *const () as usize).fmt(f)?;
        self.wglChoosePixelFormatARB.map(|p| p as *const () as usize).fmt(f)?;
        Ok(())
    }
}

/// Errors that can occur when loading WGL extension functions.
#[derive(Debug, Copy, Clone)]
pub(crate) enum ExtraWglFunctionsLoadError {
    FailedToCreateDummyWindow,
    FailedToFindPixelFormat,
    FailedToSetPixelFormat,
    FailedToCreateDummyGlContext,
    FailedToActivateDummyGlContext,
}

impl ExtraWglFunctions {
    /// Creates a dummy OpenGL context to load WGL extension function pointers.
    pub fn load() -> Result<Self, ExtraWglFunctionsLoadError> {
        use winapi::um::{
            libloaderapi::GetModuleHandleW,
            wingdi::{
                wglCreateContext, wglDeleteContext, wglGetProcAddress, wglMakeCurrent,
                ChoosePixelFormat, SetPixelFormat,
            },
            winuser::{CreateWindowExW, DestroyWindow, GetDC, ReleaseDC, CW_USEDEFAULT},
        };

        use self::ExtraWglFunctionsLoadError::*;

        unsafe {
            let mut hidden_class_name = dlopen::encode_wide(CLASS_NAME);
            let mut hidden_window_title = dlopen::encode_wide("Dummy Window");

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

            let mut pfd = super::get_default_pfd();

            let pixel_format = ChoosePixelFormat(dummy_dc, &pfd);
            if pixel_format == 0 {
                ReleaseDC(dummy_window, dummy_dc);
                DestroyWindow(dummy_window);
                return Err(FailedToFindPixelFormat);
            }

            if SetPixelFormat(dummy_dc, pixel_format, &pfd) != TRUE {
                ReleaseDC(dummy_window, dummy_dc);
                DestroyWindow(dummy_window);
                return Err(FailedToSetPixelFormat);
            }

            let dummy_context = wglCreateContext(dummy_dc);
            if dummy_context.is_null() {
                ReleaseDC(dummy_window, dummy_dc);
                DestroyWindow(dummy_window);
                return Err(FailedToCreateDummyGlContext);
            }

            if wglMakeCurrent(dummy_dc, dummy_context) != TRUE {
                wglDeleteContext(dummy_context);
                ReleaseDC(dummy_window, dummy_dc);
                DestroyWindow(dummy_window);
                return Err(FailedToActivateDummyGlContext);
            }

            let mut extra_functions = ExtraWglFunctions::default();

            extra_functions.wglChoosePixelFormatARB = {
                let mut func_name_1 = super::encode_ascii("wglChoosePixelFormatARB");
                let mut func_name_2 = super::encode_ascii("wglChoosePixelFormatEXT");

                let wgl1_result =
                    unsafe { wglGetProcAddress(func_name_1.as_mut_ptr() as *const i8) };
                let wgl2_result =
                    unsafe { wglGetProcAddress(func_name_2.as_mut_ptr() as *const i8) };

                if wgl1_result != ptr::null_mut() {
                    Some(unsafe { mem::transmute(wgl1_result) })
                } else if wgl2_result != ptr::null_mut() {
                    Some(unsafe { mem::transmute(wgl2_result) })
                } else {
                    None
                }
            };

            extra_functions.wglCreateContextAttribsARB = {
                let mut func_name = super::encode_ascii("wglCreateContextAttribsARB");
                let proc_address =
                    unsafe { wglGetProcAddress(func_name.as_mut_ptr() as *const i8) };
                if proc_address == ptr::null_mut() {
                    None
                } else {
                    Some(unsafe { mem::transmute(proc_address) })
                }
            };

            extra_functions.wglSwapIntervalEXT = {
                let mut func_name = super::encode_ascii("wglSwapIntervalEXT");
                let proc_address =
                    unsafe { wglGetProcAddress(func_name.as_mut_ptr() as *const i8) };
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
