#![allow(non_snake_case, unused_unsafe)]

use std::{ffi::c_void, mem};

use winapi::{
    shared::{
        minwindef::{BOOL, HINSTANCE},
        ntdef::HRESULT,
        windef::{
            DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, HMONITOR, HWND, RECT,
        },
        winerror::S_OK,
    },
    um::{
        wingdi::{GetDeviceCaps, LOGPIXELSX},
        winuser::{GetDC, IsProcessDPIAware, MonitorFromWindow, MONITOR_DEFAULTTONEAREST},
    },
};

#[repr(C)]
pub enum ProcessDpiAwareness {
    PROCESS_DPI_UNAWARE = 0,
    PROCESS_SYSTEM_DPI_AWARE = 1,
    PROCESS_PER_MONITOR_DPI_AWARE = 2,
}

#[repr(C)]
pub enum MonitorDpiType {
    MDT_EFFECTIVE_DPI = 0,
    MDT_ANGULAR_DPI = 1,
    MDT_RAW_DPI = 2,
    MDT_DEFAULT,
}

pub type SetProcessDPIAware = unsafe extern "system" fn() -> BOOL;
pub type SetProcessDpiAwareness = unsafe extern "system" fn(value: ProcessDpiAwareness) -> HRESULT;
pub type SetProcessDpiAwarenessContext =
    unsafe extern "system" fn(value: DPI_AWARENESS_CONTEXT) -> BOOL;
pub type GetDpiForWindow = unsafe extern "system" fn(hwnd: HWND) -> u32;
pub type GetDpiForMonitor = unsafe extern "system" fn(
    hmonitor: HMONITOR,
    dpi_type: MonitorDpiType,
    dpi_x: *mut u32,
    dpi_y: *mut u32,
) -> HRESULT;
pub type EnableNonClientDpiScaling = unsafe extern "system" fn(hwnd: HWND) -> BOOL;
pub type AdjustWindowRectExForDpi = unsafe extern "system" fn(
    rect: *mut RECT,
    dwStyle: u32,
    bMenu: BOOL,
    dwExStyle: u32,
    dpi: u32,
) -> BOOL;

#[derive(Default, Debug)]
pub struct DpiFunctions {
    user32_dll_handle: Option<HINSTANCE>,
    get_dpi_for_window: Option<GetDpiForWindow>,
    adjust_window_rect_ex_for_dpi: Option<AdjustWindowRectExForDpi>,
    get_dpi_for_monitor: Option<GetDpiForMonitor>,
    enable_non_client_dpi_scaling: Option<EnableNonClientDpiScaling>,
    set_process_dpi_awareness_context: Option<SetProcessDpiAwarenessContext>,
    set_process_dpi_awareness: Option<SetProcessDpiAwareness>,
    set_process_dpi_aware: Option<SetProcessDPIAware>,
}

impl Drop for DpiFunctions {
    fn drop(&mut self) {
        use winapi::um::libloaderapi::FreeLibrary;
        if let Some(opengl32) = self.user32_dll_handle {
            unsafe {
                FreeLibrary(opengl32);
            }
        }
    }
}

impl DpiFunctions {
    pub fn init() -> Self {
        let user32_dll = super::load_dll("user32.dll");

        unsafe {
            Self {
                user32_dll_handle: user32_dll,
                get_dpi_for_window: Self::get_func(user32_dll, "GetDpiForWindow")
                    .map(|e| unsafe { mem::transmute(e) }),
                adjust_window_rect_ex_for_dpi: Self::get_func(
                    user32_dll,
                    "AdjustWindowRectExForDpi",
                )
                .map(|e| unsafe { mem::transmute(e) }),
                get_dpi_for_monitor: Self::get_func(user32_dll, "GetDpiForMonitor")
                    .map(|e| unsafe { mem::transmute(e) }),
                enable_non_client_dpi_scaling: Self::get_func(
                    user32_dll,
                    "EnableNonClientDpiScaling",
                )
                .map(|e| unsafe { mem::transmute(e) }),
                set_process_dpi_awareness_context: Self::get_func(
                    user32_dll,
                    "SetProcessDpiAwarenessContext",
                )
                .map(|e| unsafe { mem::transmute(e) }),
                set_process_dpi_awareness: Self::get_func(user32_dll, "SetProcessDpiAwareness")
                    .map(|e| unsafe { mem::transmute(e) }),
                set_process_dpi_aware: Self::get_func(user32_dll, "SetProcessDPIAware")
                    .map(|e| unsafe { mem::transmute(e) }),
            }
        }
    }

    fn get_func(dll: Option<HINSTANCE>, s: &str) -> Option<*mut c_void> {
        use winapi::um::libloaderapi::GetProcAddress;
        let mut func_name = super::encode_ascii(s);
        dll.and_then(|s| unsafe {
            let q = GetProcAddress(s, func_name.as_mut_ptr());
            if q.is_null() {
                None
            } else {
                Some(q as *mut c_void)
            }
        })
    }

    pub fn become_dpi_aware(&self) {
        unsafe {
            if let Some(SetProcessDpiAwarenessContext) =
                self.set_process_dpi_awareness_context.clone()
            {
                // We are on Windows 10 Anniversary Update (1607) or later.
                if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
                    == false.into()
                {
                    // V2 only works with Windows 10 Creators Update (1703). Try using the older
                    // V1 if we can't set V2.
                    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);
                }
            } else if let Some(SetProcessDPIAwareness) = self.set_process_dpi_awareness.clone() {
                // We are on Windows 8.1 or later.
                SetProcessDPIAwareness(ProcessDpiAwareness::PROCESS_PER_MONITOR_DPI_AWARE);
            } else if let Some(SetProcessDPIAware) = self.set_process_dpi_aware.clone() {
                // We are on Vista or later.
                SetProcessDPIAware();
            }
        }
    }

    pub fn enable_non_client_dpi_scaling(&self, hwnd: HWND) {
        unsafe {
            if let Some(EnableNonClientDpiScaling) = self.enable_non_client_dpi_scaling.clone() {
                EnableNonClientDpiScaling(hwnd);
            }
        }
    }

    pub fn get_monitor_dpi(&self, hmonitor: HMONITOR) -> Option<u32> {
        unsafe {
            if let Some(GetDpiForMonitor) = self.get_dpi_for_monitor.clone() {
                // We are on Windows 8.1 or later.
                let mut dpi_x = 0;
                let mut dpi_y = 0;
                if GetDpiForMonitor(
                    hmonitor,
                    MonitorDpiType::MDT_EFFECTIVE_DPI,
                    &mut dpi_x,
                    &mut dpi_y,
                ) == S_OK
                {
                    // MSDN says that "the values of *dpiX and *dpiY are identical. You only need to
                    // record one of the values to determine the DPI and respond appropriately".
                    // https://msdn.microsoft.com/en-us/library/windows/desktop/dn280510(v=vs.85).aspx
                    return Some(dpi_x);
                }
            }
        }
        None
    }

    pub unsafe fn hwnd_dpi(&self, hwnd: HWND) -> u32 {
        let hdc = GetDC(hwnd);
        if hdc.is_null() {
            return BASE_DPI;
        }
        if let Some(GetDpiForWindow) = self.get_dpi_for_window.clone() {
            // We are on Windows 10 Anniversary Update (1607) or later.
            match GetDpiForWindow(hwnd) {
                0 => BASE_DPI, // 0 is returned if hwnd is invalid
                dpi => dpi,
            }
        } else if let Some(GetDpiForMonitor) = self.get_dpi_for_monitor.clone() {
            // We are on Windows 8.1 or later.
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return BASE_DPI;
            }

            let mut dpi_x = 0;
            let mut dpi_y = 0;
            if GetDpiForMonitor(
                monitor,
                MonitorDpiType::MDT_EFFECTIVE_DPI,
                &mut dpi_x,
                &mut dpi_y,
            ) == S_OK
            {
                dpi_x
            } else {
                BASE_DPI
            }
        } else {
            // We are on Vista or later.
            if IsProcessDPIAware() != false.into() {
                // If the process is DPI aware, then scaling must be handled by the application
                // using this DPI value.
                GetDeviceCaps(hdc, LOGPIXELSX) as u32
            } else {
                // If the process is DPI unaware, then scaling is performed by the OS; we thus
                // return 96 (scale factor 1.0) to prevent the window from being
                // re-scaled by both the application and the WM.
                BASE_DPI
            }
        }
    }
}

pub const BASE_DPI: u32 = 96;

pub fn dpi_to_scale_factor(dpi: u32) -> f32 {
    dpi as f32 / BASE_DPI as f32
}
