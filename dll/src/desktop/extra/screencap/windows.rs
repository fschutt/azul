//! Windows screen capture: DXGI desktop duplication of one monitor, read back through a staging
//! texture. `dxgi.dll` and `d3d11.dll` are loaded at runtime. A window capture crops that window's
//! frame out of its monitor; this process's own windows are hidden from the capture with
//! `WDA_EXCLUDEFROMCAPTURE` while it runs.

use core::ffi::c_void;
use std::time::{Duration, Instant};

use azul_layout::widgets::capture_common::{CaptureRead, CaptureRequest};
use windows::{
    core::{Interface, HRESULT},
    Win32::{
        Foundation::{E_ACCESSDENIED, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::{
                ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::{
                IDXGIAdapter1, IDXGIFactory1, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication,
                IDXGIResource, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT,
                DXGI_OUTDUPL_FRAME_INFO,
            },
        },
    },
};

use crate::desktop::shell2::windows::dlopen::{Win32Libraries, HWND, LPARAM};

const WDA_NONE: u32 = 0x0;
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

struct DxgiApi {
    create_factory: unsafe extern "system" fn(*const windows::core::GUID, *mut *mut c_void) -> HRESULT,
    create_device: unsafe extern "system" fn(
        *mut c_void,
        i32,
        *mut c_void,
        u32,
        *const i32,
        u32,
        u32,
        *mut *mut c_void,
        *mut i32,
        *mut *mut c_void,
    ) -> HRESULT,
}

fn api() -> Result<&'static DxgiApi, String> {
    static API: std::sync::OnceLock<Result<DxgiApi, String>> = std::sync::OnceLock::new();
    API.get_or_init(|| unsafe {
        let load = |name: &str| -> Result<&'static libloading::Library, String> {
            libloading::Library::new(name)
                .map(|lib| &*Box::leak(Box::new(lib)))
                .map_err(|e| format!("{name}: {e}"))
        };
        let sym = |lib: &'static libloading::Library, name: &str| -> Result<*const c_void, String> {
            let mut z = name.as_bytes().to_vec();
            z.push(0);
            lib.get::<*const c_void>(&z)
                .map(|s| *s)
                .map_err(|e| format!("{name}: {e}"))
        };
        let (dxgi, d3d11) = (load("dxgi.dll")?, load("d3d11.dll")?);
        Ok(DxgiApi {
            create_factory: core::mem::transmute(sym(dxgi, "CreateDXGIFactory1")?),
            create_device: core::mem::transmute(sym(d3d11, "D3D11CreateDevice")?),
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

fn describe(hr: HRESULT) -> String {
    if hr == E_ACCESSDENIED {
        "access denied (a secure desktop such as UAC or the lock screen is showing)".to_string()
    } else {
        format!("{:#010x} {}", hr.0 as u32, hr.message())
    }
}

struct DxgiCapture {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    output: IDXGIOutput1,
    duplication: IDXGIOutputDuplication,
    staging: Option<(ID3D11Texture2D, u32, u32)>,
    /// The monitor in desktop coordinates, for cropping a window out of it.
    monitor: RECT,
    window: u64,
    frame_interval: Duration,
    last_frame: Option<Instant>,
    excluded: Vec<HWND>,
}

impl Drop for DxgiCapture {
    fn drop(&mut self) {
        set_affinity(&self.excluded, WDA_NONE);
    }
}

/// Monitors of every adapter, in adapter then output order.
unsafe fn outputs(api: &DxgiApi) -> Result<Vec<(IDXGIAdapter1, IDXGIOutput)>, String> {
    unsafe {
        let mut raw = core::ptr::null_mut();
        (api.create_factory)(&IDXGIFactory1::IID, &mut raw)
            .ok()
            .map_err(|e| describe(e.code()))?;
        let factory = IDXGIFactory1::from_raw(raw);
        let mut list = Vec::new();
        for a in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(a) else {
                break;
            };
            for o in 0.. {
                let Ok(output) = adapter.EnumOutputs(o) else {
                    break;
                };
                list.push((adapter.clone(), output));
            }
        }
        Ok(list)
    }
}

/// Every top-level window of this process.
fn own_windows() -> Vec<HWND> {
    let Some(win32) = Win32Libraries::shared() else {
        return Vec::new();
    };
    unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> i32 {
        unsafe {
            let list = &mut *(list as *mut (u32, Vec<HWND>));
            let Some(win32) = Win32Libraries::shared() else {
                return 0;
            };
            let mut pid = 0u32;
            (win32.user32.GetWindowThreadProcessId)(hwnd, &mut pid);
            if pid == list.0 {
                list.1.push(hwnd);
            }
            1
        }
    }
    let mut list = (std::process::id(), Vec::new());
    unsafe {
        (win32.user32.EnumWindows)(Some(collect), &mut list as *mut (u32, Vec<HWND>) as LPARAM);
    }
    list.1
}

fn set_affinity(windows: &[HWND], affinity: u32) {
    let Some(set) = Win32Libraries::shared().and_then(|w| w.user32.SetWindowDisplayAffinity) else {
        return;
    };
    let Some(win32) = Win32Libraries::shared() else {
        return;
    };
    for &hwnd in windows {
        unsafe {
            if (win32.user32.IsWindow)(hwnd) != 0 {
                set(hwnd, affinity);
            }
        }
    }
}

/// The window's visible frame in desktop coordinates.
fn window_rect(window: u64) -> Option<RECT> {
    let win32 = Win32Libraries::shared()?;
    let hwnd = window as usize as HWND;
    let mut rect = crate::desktop::shell2::windows::dlopen::RECT::default();
    unsafe {
        if (win32.user32.IsWindow)(hwnd) == 0 {
            return None;
        }
        let from_dwm = win32.dwmapi_funcs.is_some_and(|dwm| {
            (dwm.DwmGetWindowAttribute)(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut rect as *mut crate::desktop::shell2::windows::dlopen::RECT).cast(),
                core::mem::size_of::<crate::desktop::shell2::windows::dlopen::RECT>() as u32,
            ) == 0
        });
        if !from_dwm && (win32.user32.GetWindowRect)(hwnd, &mut rect) == 0 {
            return None;
        }
    }
    Some(RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

impl DxgiCapture {
    unsafe fn open(request: &CaptureRequest) -> Result<(Self, u32, u32), String> {
        unsafe {
            let api = api()?;
            let outputs = outputs(api)?;
            let (adapter, output) = outputs
                .get(request.index as usize)
                .or(outputs.first())
                .ok_or("no monitor attached to a DXGI adapter")?;
            let mut device = core::ptr::null_mut();
            let mut context = core::ptr::null_mut();
            (api.create_device)(
                adapter.as_raw(),
                D3D_DRIVER_TYPE_UNKNOWN.0,
                core::ptr::null_mut(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT.0,
                core::ptr::null(),
                0,
                D3D11_SDK_VERSION,
                &mut device,
                core::ptr::null_mut(),
                &mut context,
            )
            .ok()
            .map_err(|e| describe(e.code()))?;
            let device = ID3D11Device::from_raw(device);
            let context = ID3D11DeviceContext::from_raw(context);
            let output: IDXGIOutput1 = output.cast().map_err(|e| describe(e.code()))?;
            let duplication = output
                .DuplicateOutput(&device)
                .map_err(|e| describe(e.code()))?;
            let desc = output.GetDesc().map_err(|e| describe(e.code()))?;
            let mode = duplication.GetDesc().ModeDesc;
            let excluded = if request.exclude_self {
                let windows = own_windows();
                set_affinity(&windows, WDA_EXCLUDEFROMCAPTURE);
                windows
            } else {
                Vec::new()
            };
            Ok((
                DxgiCapture {
                    device,
                    context,
                    output,
                    duplication,
                    staging: None,
                    monitor: desc.DesktopCoordinates,
                    window: request.window,
                    frame_interval: Duration::from_millis(1000 / u64::from(request.fps_or(30).max(1))),
                    last_frame: None,
                    excluded,
                },
                mode.Width,
                mode.Height,
            ))
        }
    }

    unsafe fn staging_for(&mut self, width: u32, height: u32, template: &D3D11_TEXTURE2D_DESC) -> Option<ID3D11Texture2D> {
        unsafe {
            if let Some((tex, w, h)) = &self.staging {
                if *w == width && *h == height {
                    return Some(tex.clone());
                }
            }
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: template.Format,
                SampleDesc: template.SampleDesc,
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut tex = None;
            self.device.CreateTexture2D(&desc, None, Some(&mut tex)).ok()?;
            let tex = tex?;
            self.staging = Some((tex.clone(), width, height));
            Some(tex)
        }
    }

    unsafe fn read(&mut self, out: &mut Vec<u8>) -> CaptureRead {
        unsafe {
            if let Some(last) = self.last_frame {
                let due = last + self.frame_interval;
                let now = Instant::now();
                if due > now {
                    std::thread::sleep(due - now);
                }
            }
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            match self.duplication.AcquireNextFrame(500, &mut info, &mut resource) {
                Ok(()) => {}
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return CaptureRead::Idle,
                Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                    // Mode change, fullscreen app or secure desktop: duplicate again.
                    match self.output.DuplicateOutput(&self.device) {
                        Ok(d) => self.duplication = d,
                        Err(_) => std::thread::sleep(Duration::from_millis(250)),
                    }
                    return CaptureRead::Idle;
                }
                Err(_) => return CaptureRead::Ended,
            }
            // Only the pointer moved: the desktop image is unchanged.
            let result = if info.LastPresentTime == 0 {
                None
            } else {
                resource.and_then(|r| r.cast::<ID3D11Texture2D>().ok()).and_then(|frame| {
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    frame.GetDesc(&mut desc);
                    let staging = self.staging_for(desc.Width, desc.Height, &desc)?;
                    self.context.CopyResource(&staging, &frame);
                    Some((staging, desc.Width, desc.Height))
                })
            };
            self.duplication.ReleaseFrame().ok();
            let Some((staging, width, height)) = result else {
                return CaptureRead::Idle;
            };
            self.last_frame = Some(Instant::now());

            // The part of the monitor to deliver: all of it, or the window's frame inside it.
            let (x0, y0, x1, y1) = if self.window == 0 {
                (0, 0, width as i32, height as i32)
            } else {
                let Some(r) = window_rect(self.window) else {
                    return CaptureRead::Ended;
                };
                let x0 = (r.left - self.monitor.left).clamp(0, width as i32);
                let y0 = (r.top - self.monitor.top).clamp(0, height as i32);
                let x1 = (r.right - self.monitor.left).clamp(0, width as i32);
                let y1 = (r.bottom - self.monitor.top).clamp(0, height as i32);
                if x1 <= x0 || y1 <= y0 {
                    return CaptureRead::Idle;
                }
                (x0, y0, x1, y1)
            };
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if self
                .context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .is_err()
            {
                return CaptureRead::Idle;
            }
            let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
            out.resize(w * h * 4, 0);
            let pitch = mapped.RowPitch as usize;
            let base = mapped.pData as *const u8;
            for y in 0..h {
                let row = base.add((y0 as usize + y) * pitch + x0 as usize * 4);
                let src = core::slice::from_raw_parts(row, w * 4);
                let dst = &mut out[y * w * 4..(y + 1) * w * 4];
                for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }
            self.context.Unmap(&staging, 0);
            CaptureRead::Frame {
                width: w as u32,
                height: h as u32,
            }
        }
    }
}

pub fn open(request: &CaptureRequest) -> u64 {
    match unsafe { DxgiCapture::open(request) } {
        Ok((capture, width, height)) => {
            crate::plog_info!(
                "[screencap] DXGI desktop duplication of monitor {} ({width}x{height}){}{}",
                request.index,
                if request.window != 0 { ", cropped to a window" } else { "" },
                if capture.excluded.is_empty() { "" } else { ", own windows excluded" },
            );
            Box::into_raw(Box::new(capture)) as u64
        }
        Err(e) => {
            crate::plog_warn!("[screencap] DXGI desktop duplication unavailable: {e}");
            0
        }
    }
}

pub fn read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
    match unsafe { (handle as *mut DxgiCapture).as_mut() } {
        Some(capture) => unsafe { capture.read(out) },
        None => CaptureRead::Ended,
    }
}

pub fn close(handle: u64) {
    if handle != 0 {
        unsafe { drop(Box::from_raw(handle as *mut DxgiCapture)) };
    }
}
