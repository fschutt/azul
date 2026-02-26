//! Display/Monitor management for all platforms
//!
//! This module provides cross-platform display enumeration and information.
//! Used primarily for menu positioning to avoid overflow at screen edges.

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    window::{Monitor, MonitorId, MonitorVec, VideoMode, VideoModeVec},
};
use azul_css::{
    props::basic::{LayoutPoint, LayoutRect, LayoutSize},
    AzString, OptionString,
};

use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::log_debug;

/// Information about a display/monitor
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    /// Display name (e.g., "\\.\DISPLAY1" on Windows, ":0.0" on X11)
    pub name: String,
    /// Physical bounds of the display in screen coordinates
    pub bounds: LogicalRect,
    /// Work area (bounds minus taskbars/panels)
    pub work_area: LogicalRect,
    /// DPI scale factor
    pub scale_factor: f32,
    /// Whether this is the primary display
    pub is_primary: bool,
    /// Available video modes (resolution, refresh rate, bit depth)
    pub video_modes: Vec<VideoMode>,
}

/// Get all available displays
///
/// This function queries the OS for all connected displays/monitors.
/// The first display in the list is typically the primary display.
///
/// # Platform Notes
///
/// - **Windows**: Uses EnumDisplayMonitors + GetMonitorInfoW
/// - **macOS**: Uses NSScreen.screens
/// - **X11**: Uses XRandR extension (fallback to single display if unavailable)
/// - **Wayland**: Not directly available - compositor manages positioning
pub fn get_displays() -> Vec<DisplayInfo> {
    #[cfg(target_os = "windows")]
    return windows::get_displays();

    #[cfg(target_os = "macos")]
    return macos::get_displays();

    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    return linux::get_displays();

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return vec![];
}

/// Get all monitors as Monitor structs with stable MonitorId
///
/// This is the recommended API for getting monitor information.
/// Returns a MonitorVec with stable MonitorId values that persist across frames.
pub fn get_monitors() -> MonitorVec {
    get_displays()
        .into_iter()
        .enumerate()
        .map(|(i, display)| display.to_monitor(i))
        .collect::<Vec<_>>()
        .into()
}

impl DisplayInfo {
    /// Convert DisplayInfo to Monitor with index and stable hash
    pub fn to_monitor(&self, index: usize) -> Monitor {
        // Generate stable ID from monitor properties (includes index + hash)
        let monitor_id = MonitorId::from_properties(
            index,
            &self.name,
            LayoutPoint::new(self.bounds.origin.x as isize, self.bounds.origin.y as isize),
            LayoutSize::new(
                self.bounds.size.width as isize,
                self.bounds.size.height as isize,
            ),
        );

        Monitor {
            monitor_id,
            monitor_name: OptionString::Some(self.name.as_str().into()),
            size: LayoutSize::new(
                self.bounds.size.width as isize,
                self.bounds.size.height as isize,
            ),
            position: LayoutPoint::new(
                self.bounds.origin.x as isize,
                self.bounds.origin.y as isize,
            ),
            scale_factor: self.scale_factor as f64,
            work_area: LayoutRect::new(
                LayoutPoint::new(
                    self.work_area.origin.x as isize,
                    self.work_area.origin.y as isize,
                ),
                LayoutSize::new(
                    self.work_area.size.width as isize,
                    self.work_area.size.height as isize,
                ),
            ),
            video_modes: self.video_modes.clone().into(),
            is_primary_monitor: self.is_primary,
        }
    }
}

/// Get the display containing the given point
///
/// Returns None if the point is not on any display.
pub fn get_display_at_point(point: LogicalPosition) -> Option<DisplayInfo> {
    get_displays()
        .into_iter()
        .find(|display| display.bounds.contains(point))
}

/// Get the display index containing the given point
///
/// Returns 0 (primary monitor) if the point is not on any display.
pub fn get_display_index_at_point(point: LogicalPosition) -> usize {
    get_displays()
        .iter()
        .position(|display| display.bounds.contains(point))
        .unwrap_or(0)
}

/// Get the display containing the given window
///
/// Uses platform-specific APIs to determine which monitor the window is on.
/// Falls back to the display containing the window's center point.
///
/// # Platform Notes
///
/// - **Windows**: Uses MonitorFromWindow API
/// - **macOS**: Uses NSScreen.screens and window frame
/// - **X11/Wayland**: Uses window position to find containing display
pub fn get_window_display(
    window_position: LogicalPosition,
    window_size: LogicalSize,
) -> Option<DisplayInfo> {
    // Calculate window center
    let center = LogicalPosition::new(
        window_position.x + window_size.width / 2.0,
        window_position.y + window_size.height / 2.0,
    );

    get_display_at_point(center)
}

/// Get the primary display
pub fn get_primary_display() -> Option<DisplayInfo> {
    get_displays()
        .into_iter()
        .find(|display| display.is_primary)
}

#[cfg(target_os = "windows")]
mod windows {
    use std::ptr;

    use super::*;

    // Windows API structures and functions
    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MONITORINFOEXW {
        monitor_info: MONITORINFO,
        sz_device: [u16; 32], // CCHDEVICENAME = 32
    }

    #[repr(C)]
    struct MONITORINFO {
        cb_size: u32,
        rc_monitor: RECT,
        rc_work: RECT,
        dw_flags: u32,
    }

    const MONITORINFOF_PRIMARY: u32 = 0x00000001;
    const MONITOR_DEFAULTTONEAREST: u32 = 0x00000002;

    type HMONITOR = *mut std::ffi::c_void;
    type HDC = *mut std::ffi::c_void;
    type HWND = *mut std::ffi::c_void;

    #[repr(C)]
    struct DEVMODEW {
        dm_device_name: [u16; 32],
        dm_spec_version: u16,
        dm_driver_version: u16,
        dm_size: u16,
        dm_driver_extra: u16,
        dm_fields: u32,
        dm_position_x: i32,
        dm_position_y: i32,
        dm_display_orientation: u32,
        dm_display_fixed_output: u32,
        dm_color: i16,
        dm_duplex: i16,
        dm_y_resolution: i16,
        dm_tt_option: i16,
        dm_collate: i16,
        dm_form_name: [u16; 32],
        dm_log_pixels: u16,
        dm_bits_per_pel: u32,
        dm_pels_width: u32,
        dm_pels_height: u32,
        dm_display_flags: u32,
        dm_display_frequency: u32,
        dm_icm_method: u32,
        dm_icm_intent: u32,
        dm_media_type: u32,
        dm_dither_type: u32,
        dm_reserved1: u32,
        dm_reserved2: u32,
        dm_panning_width: u32,
        dm_panning_height: u32,
    }

    const DM_BITSPERPEL: u32 = 0x00040000;
    const DM_PELSWIDTH: u32 = 0x00080000;
    const DM_PELSHEIGHT: u32 = 0x00100000;
    const DM_DISPLAYFREQUENCY: u32 = 0x00400000;
    const ENUM_CURRENT_SETTINGS: u32 = 0xFFFFFFFF;

    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplayMonitors(
            hdc: HDC,
            lprc_clip: *const RECT,
            lpfn_enum: extern "system" fn(HMONITOR, HDC, *mut RECT, isize) -> i32,
            dw_data: isize,
        ) -> i32;

        fn GetMonitorInfoW(hmonitor: HMONITOR, lpmi: *mut MONITORINFO) -> i32;

        fn GetDpiForMonitor(
            hmonitor: HMONITOR,
            dpi_type: u32,
            dpi_x: *mut u32,
            dpi_y: *mut u32,
        ) -> i32;

        fn EnumDisplaySettingsW(
            lpsz_device_name: *const u16,
            i_mode_num: u32,
            lp_dev_mode: *mut DEVMODEW,
        ) -> i32;
    }

    // Callback context for EnumDisplayMonitors
    struct EnumContext {
        displays: Vec<DisplayInfo>,
        monitor_id: usize,
    }

    extern "system" fn monitor_enum_proc(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _lprc_monitor: *mut RECT,
        dw_data: isize,
    ) -> i32 {
        unsafe {
            let context = &mut *(dw_data as *mut EnumContext);

            // Get monitor info
            let mut monitor_info: MONITORINFOEXW = std::mem::zeroed();
            monitor_info.monitor_info.cb_size = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(hmonitor, &mut monitor_info.monitor_info as *mut MONITORINFO) == 0 {
                return 1; // Continue enumeration
            }

            // Extract monitor bounds
            let rc_monitor = &monitor_info.monitor_info.rc_monitor;
            let bounds = LogicalRect::new(
                LogicalPosition::new(rc_monitor.left as f32, rc_monitor.top as f32),
                LogicalSize::new(
                    (rc_monitor.right - rc_monitor.left) as f32,
                    (rc_monitor.bottom - rc_monitor.top) as f32,
                ),
            );

            // Extract work area (bounds minus taskbar)
            let rc_work = &monitor_info.monitor_info.rc_work;
            let work_area = LogicalRect::new(
                LogicalPosition::new(rc_work.left as f32, rc_work.top as f32),
                LogicalSize::new(
                    (rc_work.right - rc_work.left) as f32,
                    (rc_work.bottom - rc_work.top) as f32,
                ),
            );

            // Get DPI (Windows 8.1+)
            let mut dpi_x: u32 = 96;
            let mut dpi_y: u32 = 96;
            let _ = GetDpiForMonitor(hmonitor, 0, &mut dpi_x, &mut dpi_y); // MDT_EFFECTIVE_DPI = 0

            let scale_factor = (dpi_x as f32) / 96.0;

            // Check if primary monitor
            let is_primary = (monitor_info.monitor_info.dw_flags & MONITORINFOF_PRIMARY) != 0;

            // Convert device name from UTF-16
            let name = String::from_utf16_lossy(&monitor_info.sz_device)
                .trim_end_matches('\0')
                .to_string();

            // Get current display settings (resolution, refresh rate, bit depth)
            let mut dev_mode: DEVMODEW = std::mem::zeroed();
            dev_mode.dm_size = std::mem::size_of::<DEVMODEW>() as u16;

            let video_modes = if !monitor_info.sz_device.is_empty()
                && EnumDisplaySettingsW(
                    monitor_info.sz_device.as_ptr(),
                    ENUM_CURRENT_SETTINGS,
                    &mut dev_mode,
                ) != 0
            {
                // Check which fields are valid
                let has_refresh = (dev_mode.dm_fields & DM_DISPLAYFREQUENCY) != 0;
                let has_bits = (dev_mode.dm_fields & DM_BITSPERPEL) != 0;
                let has_resolution = (dev_mode.dm_fields & (DM_PELSWIDTH | DM_PELSHEIGHT))
                    == (DM_PELSWIDTH | DM_PELSHEIGHT);

                if has_resolution {
                    vec![VideoMode {
                        size: LayoutSize::new(
                            dev_mode.dm_pels_width as isize,
                            dev_mode.dm_pels_height as isize,
                        ),
                        bit_depth: if has_bits {
                            dev_mode.dm_bits_per_pel as u16
                        } else {
                            32
                        },
                        refresh_rate: if has_refresh {
                            dev_mode.dm_display_frequency as u16
                        } else {
                            60
                        },
                    }]
                } else {
                    // Fallback: use monitor bounds for resolution
                    vec![VideoMode {
                        size: LayoutSize::new(
                            (rc_monitor.right - rc_monitor.left) as isize,
                            (rc_monitor.bottom - rc_monitor.top) as isize,
                        ),
                        bit_depth: 32,
                        refresh_rate: 60,
                    }]
                }
            } else {
                // Fallback: use monitor bounds for resolution
                vec![VideoMode {
                    size: LayoutSize::new(
                        (rc_monitor.right - rc_monitor.left) as isize,
                        (rc_monitor.bottom - rc_monitor.top) as isize,
                    ),
                    bit_depth: 32,
                    refresh_rate: 60,
                }]
            };

            context.displays.push(DisplayInfo {
                name: if name.is_empty() {
                    format!("Monitor {}", context.monitor_id)
                } else {
                    name
                },
                bounds,
                work_area,
                scale_factor,
                is_primary,
                video_modes,
            });

            context.monitor_id += 1;
            1 // Continue enumeration
        }
    }

    pub fn get_displays() -> Vec<DisplayInfo> {
        unsafe {
            let mut context = EnumContext {
                displays: Vec::new(),
                monitor_id: 0,
            };

            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                monitor_enum_proc,
                &mut context as *mut EnumContext as isize,
            );

            // If enumeration failed or found no monitors, return fallback
            if context.displays.is_empty() {
                vec![DisplayInfo {
                    name: "Primary Monitor".to_string(),
                    bounds: LogicalRect::new(
                        LogicalPosition::zero(),
                        LogicalSize::new(1920.0, 1080.0),
                    ),
                    work_area: LogicalRect::new(
                        LogicalPosition::zero(),
                        LogicalSize::new(1920.0, 1040.0),
                    ),
                    scale_factor: 1.0,
                    is_primary: true,
                    video_modes: vec![VideoMode {
                        size: LayoutSize::new(1920, 1080),
                        bit_depth: 32,
                        refresh_rate: 60,
                    }],
                }]
            } else {
                context.displays
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::msg_send;
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;

    use super::*;

    pub fn get_displays() -> Vec<DisplayInfo> {
        log_debug!(
            LogCategory::General,
            "[get_displays] Starting monitor enumeration..."
        );
        let mtm = MainThreadMarker::new().expect("Must be called on main thread");
        log_debug!(LogCategory::General, "[get_displays] Got MainThreadMarker");

        let screens = NSScreen::screens(mtm);
        log_debug!(
            LogCategory::General,
            "[get_displays] Got {} screens",
            screens.len()
        );

        let mut displays = Vec::new();

        for (i, screen) in screens.iter().enumerate() {
            log_debug!(
                LogCategory::General,
                "[get_displays] Processing screen {}...",
                i
            );
            let frame = screen.frame();
            let visible_frame = screen.visibleFrame();
            let scale = screen.backingScaleFactor();
            log_debug!(
                LogCategory::General,
                "[get_displays] Screen {} frame: {}x{}",
                i,
                frame.size.width,
                frame.size.height
            );

            // macOS uses flipped coordinates (origin at bottom-left)
            // Convert to top-left origin
            let bounds = LogicalRect::new(
                LogicalPosition::new(frame.origin.x as f32, frame.origin.y as f32),
                LogicalSize::new(frame.size.width as f32, frame.size.height as f32),
            );

            let work_area = LogicalRect::new(
                LogicalPosition::new(visible_frame.origin.x as f32, visible_frame.origin.y as f32),
                LogicalSize::new(
                    visible_frame.size.width as f32,
                    visible_frame.size.height as f32,
                ),
            );

            // Get refresh rate from NSScreen (macOS 10.15+)
            // maximumFramesPerSecond returns refresh rate in Hz
            log_debug!(
                LogCategory::General,
                "[get_displays] Getting refresh rate for screen {}...",
                i
            );
            let refresh_rate = unsafe {
                let fps: i64 = msg_send![&**screen, maximumFramesPerSecond];
                log_debug!(
                    LogCategory::General,
                    "[get_displays] Screen {} refresh rate: {} Hz",
                    i,
                    fps
                );
                if fps > 0 {
                    fps as u16
                } else {
                    60
                }
            };

            log_debug!(
                LogCategory::General,
                "[get_displays] Getting localized name for screen {}...",
                i
            );
            let name = screen.localizedName().to_string();
            log_debug!(
                LogCategory::General,
                "[get_displays] Screen {} name: {}",
                i,
                name
            );

            displays.push(DisplayInfo {
                name,
                bounds,
                work_area,
                scale_factor: scale as f32,
                is_primary: i == 0, // First screen is primary on macOS
                video_modes: vec![VideoMode {
                    size: LayoutSize::new(bounds.size.width as isize, bounds.size.height as isize),
                    bit_depth: 32, // macOS doesn't expose bit depth, assume 32-bit
                    refresh_rate,
                }],
            });
            log_debug!(
                LogCategory::General,
                "[get_displays] Screen {} added to displays list",
                i
            );
        }

        log_debug!(
            LogCategory::General,
            "[get_displays] Returning {} displays",
            displays.len()
        );
        displays
    }
}

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
mod linux {
    use super::*;

    pub fn get_displays() -> Vec<DisplayInfo> {
        // Try X11 first, then Wayland
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            wayland::get_displays()
        } else {
            x11::get_displays()
        }
    }

    mod x11 {
        use std::ptr;

        use super::*;
        use crate::desktop::shell2::linux::x11::dlopen::Xlib;

        // XRandR types (opaque pointers)
        type XRRScreenResources = *mut c_void;
        type XRRCrtcInfo = *mut c_void;
        type RRCrtc = u64;
        type RROutput = u64;
        type Rotation = u16;
        type Time = u64;

        use std::ffi::c_void;

        // XRandR structures
        #[repr(C)]
        struct XRRModeInfo {
            id: u64,
            width: u32,
            height: u32,
            dot_clock: u64,
            h_sync_start: u32,
            h_sync_end: u32,
            h_total: u32,
            h_skew: u32,
            v_sync_start: u32,
            v_sync_end: u32,
            v_total: u32,
            name: *mut i8,
            name_length: u32,
            mode_flags: u64,
        }

        #[repr(C)]
        struct XRRScreenResourcesStruct {
            timestamp: Time,
            config_timestamp: Time,
            ncrtc: i32,
            crtcs: *mut RRCrtc,
            noutput: i32,
            outputs: *mut RROutput,
            nmode: i32,
            modes: *mut XRRModeInfo,
        }

        #[repr(C)]
        struct XRRCrtcInfoStruct {
            timestamp: Time,
            x: i32,
            y: i32,
            width: u32,
            height: u32,
            mode: u64, // RRMode
            rotation: Rotation,
            noutput: i32,
            outputs: *mut RROutput,
            rotations: Rotation,
            npossible: i32,
            possible: *mut RROutput,
        }

        // XRandR function pointers
        type XRRGetScreenResourcesCurrentFn =
            unsafe extern "C" fn(*mut c_void, u64) -> XRRScreenResources;
        type XRRFreeScreenResourcesFn = unsafe extern "C" fn(XRRScreenResources);
        type XRRGetCrtcInfoFn =
            unsafe extern "C" fn(*mut c_void, XRRScreenResources, RRCrtc) -> XRRCrtcInfo;
        type XRRFreeCrtcInfoFn = unsafe extern "C" fn(XRRCrtcInfo);

        pub fn get_displays() -> Vec<DisplayInfo> {
            // Try XRandR multi-monitor first, fallback to single display
            match try_xrandr_displays() {
                Ok(displays) if !displays.is_empty() => displays,
                _ => try_single_display(),
            }
        }

        fn try_xrandr_displays() -> Result<Vec<DisplayInfo>, ()> {
            use crate::desktop::shell2::{
                common::{dlopen::load_first_available, DynamicLibrary},
                linux::x11::dlopen::Library,
            };

            unsafe {
                // Load Xlib
                let xlib = Xlib::new().map_err(|_| ())?;
                let display = (xlib.XOpenDisplay)(ptr::null());
                if display.is_null() {
                    return Err(());
                }

                // Try to load XRandR library
                let xrandr_lib =
                    load_first_available::<Library>(&["libXrandr.so.2", "libXrandr.so"])
                        .map_err(|_| ())?;

                // Load XRandR functions
                let get_screen_resources: XRRGetScreenResourcesCurrentFn = xrandr_lib
                    .get_symbol("XRRGetScreenResourcesCurrent")
                    .map_err(|_| ())?;
                let free_screen_resources: XRRFreeScreenResourcesFn = xrandr_lib
                    .get_symbol("XRRFreeScreenResources")
                    .map_err(|_| ())?;
                let get_crtc_info: XRRGetCrtcInfoFn =
                    xrandr_lib.get_symbol("XRRGetCrtcInfo").map_err(|_| ())?;
                let free_crtc_info: XRRFreeCrtcInfoFn =
                    xrandr_lib.get_symbol("XRRFreeCrtcInfo").map_err(|_| ())?;

                let screen = (xlib.XDefaultScreen)(display);
                let root = (xlib.XRootWindow)(display, screen);

                // Get screen resources
                let resources_ptr = get_screen_resources(display, root);
                if resources_ptr.is_null() {
                    (xlib.XCloseDisplay)(display);
                    return Err(());
                }

                let resources = &*(resources_ptr as *const XRRScreenResourcesStruct);
                let mut displays = Vec::new();

                // Calculate base DPI from screen size
                let width_mm = (xlib.XDisplayWidthMM)(display, screen);
                let height_mm = (xlib.XDisplayHeightMM)(display, screen);
                let base_scale = if width_mm > 0 {
                    let screen_dpi =
                        ((xlib.XDisplayWidth)(display, screen) as f32 / width_mm as f32) * 25.4;
                    screen_dpi / 96.0
                } else {
                    1.0
                };

                // Iterate over CRTCs (monitors)
                let crtcs = std::slice::from_raw_parts(resources.crtcs, resources.ncrtc as usize);
                for (i, &crtc) in crtcs.iter().enumerate() {
                    let crtc_info_ptr = get_crtc_info(display, resources_ptr, crtc);
                    if crtc_info_ptr.is_null() {
                        continue;
                    }

                    let crtc_info = &*(crtc_info_ptr as *const XRRCrtcInfoStruct);

                    // Skip disabled CRTCs (width/height = 0)
                    if crtc_info.width == 0 || crtc_info.height == 0 {
                        free_crtc_info(crtc_info_ptr);
                        continue;
                    }

                    let bounds = LogicalRect::new(
                        LogicalPosition::new(crtc_info.x as f32, crtc_info.y as f32),
                        LogicalSize::new(crtc_info.width as f32, crtc_info.height as f32),
                    );

                    // Approximate work area (subtract 24px for panel)
                    let work_area = LogicalRect::new(
                        LogicalPosition::new(crtc_info.x as f32, crtc_info.y as f32),
                        LogicalSize::new(
                            crtc_info.width as f32,
                            (crtc_info.height.saturating_sub(24)) as f32,
                        ),
                    );

                    // Get refresh rate from mode info
                    let refresh_rate = if crtc_info.mode != 0 {
                        // Find the mode in resources
                        let modes =
                            std::slice::from_raw_parts(resources.modes, resources.nmode as usize);
                        modes
                            .iter()
                            .find(|mode| mode.id == crtc_info.mode)
                            .map(|mode| {
                                // Calculate refresh rate: (dotClock / (hTotal * vTotal))
                                let h_total = mode.h_total as u64;
                                let v_total = mode.v_total as u64;
                                if h_total > 0 && v_total > 0 {
                                    let refresh =
                                        (mode.dot_clock as u64 * 1000) / (h_total * v_total);
                                    refresh as u16
                                } else {
                                    60
                                }
                            })
                            .unwrap_or(60)
                    } else {
                        60
                    };

                    displays.push(DisplayInfo {
                        name: format!("CRTC-{}", i),
                        bounds,
                        work_area,
                        scale_factor: base_scale,
                        is_primary: i == 0, // First CRTC is typically primary
                        video_modes: vec![VideoMode {
                            size: LayoutSize::new(
                                crtc_info.width as isize,
                                crtc_info.height as isize,
                            ),
                            bit_depth: 32, // X11 doesn't easily expose this, assume 32-bit
                            refresh_rate,
                        }],
                    });

                    free_crtc_info(crtc_info_ptr);
                }

                free_screen_resources(resources_ptr);
                (xlib.XCloseDisplay)(display);

                if displays.is_empty() {
                    Err(())
                } else {
                    Ok(displays)
                }
            }
        }

        fn try_single_display() -> Vec<DisplayInfo> {
            // Fallback to single display detection
            let xlib = match Xlib::new() {
                Ok(x) => x,
                Err(_) => return fallback_display(),
            };

            unsafe {
                let display = (xlib.XOpenDisplay)(ptr::null());
                if display.is_null() {
                    return fallback_display();
                }

                let screen = (xlib.XDefaultScreen)(display);

                // Get screen dimensions in pixels
                let width_px = (xlib.XDisplayWidth)(display, screen);
                let height_px = (xlib.XDisplayHeight)(display, screen);

                // Get screen dimensions in millimeters
                let width_mm = (xlib.XDisplayWidthMM)(display, screen);
                let height_mm = (xlib.XDisplayHeightMM)(display, screen);

                // Calculate DPI
                let dpi_x = if width_mm > 0 {
                    (width_px as f32 / width_mm as f32) * 25.4
                } else {
                    96.0 // Default DPI
                };

                let dpi_y = if height_mm > 0 {
                    (height_px as f32 / height_mm as f32) * 25.4
                } else {
                    96.0
                };

                // Use average DPI for scale factor
                let avg_dpi = (dpi_x + dpi_y) / 2.0;
                let scale_factor = avg_dpi / 96.0; // 96 DPI is the standard baseline

                (xlib.XCloseDisplay)(display);

                let bounds = LogicalRect::new(
                    LogicalPosition::zero(),
                    LogicalSize::new(width_px as f32, height_px as f32),
                );

                // Approximate work area by subtracting common panel height (24px)
                let work_area = LogicalRect::new(
                    LogicalPosition::zero(),
                    LogicalSize::new(width_px as f32, (height_px - 24).max(0) as f32),
                );

                vec![DisplayInfo {
                    name: format!(":0.{}", screen),
                    bounds,
                    work_area,
                    scale_factor,
                    is_primary: true,
                    video_modes: vec![VideoMode {
                        size: LayoutSize::new(width_px as isize, height_px as isize),
                        bit_depth: 32,
                        refresh_rate: 60, // Default to 60Hz when we can't detect
                    }],
                }]
            }
        }

        fn fallback_display() -> Vec<DisplayInfo> {
            let bounds =
                LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(1920.0, 1080.0));

            let work_area = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(1920.0, 1056.0), // 1080 - 24
            );

            vec![DisplayInfo {
                name: ":0.0".to_string(),
                bounds,
                work_area,
                scale_factor: 1.0,
                is_primary: true,
                video_modes: vec![VideoMode {
                    size: LayoutSize::new(1920, 1080),
                    bit_depth: 32,
                    refresh_rate: 60,
                }],
            }]
        }
    }

    mod wayland {
        use std::process::Command;

        use super::*;

        type DisplayProvider = fn() -> Result<Vec<DisplayInfo>, ()>;

        // Chain of detection methods, ordered from most specific to most generic
        const DETECTION_CHAIN: &[DisplayProvider] =
            &[try_swaymsg, try_hyprctl, try_kscreen_doctor, try_wlr_randr];

        pub fn get_displays() -> Vec<DisplayInfo> {
            // Try each detection method in order
            for provider in DETECTION_CHAIN {
                if let Ok(displays) = provider() {
                    if !displays.is_empty() {
                        return displays;
                    }
                }
            }
            // If all providers fail, use fallback
            fallback_display()
        }

        fn fallback_display() -> Vec<DisplayInfo> {
            log_debug!(
                LogCategory::General,
                "[display] All Wayland detection methods failed. Falling back to default display."
            );

            // Try to get actual dimensions from environment or reasonable defaults
            let (width, height) = if let (Ok(w), Ok(h)) = (
                std::env::var("WAYLAND_DISPLAY_WIDTH"),
                std::env::var("WAYLAND_DISPLAY_HEIGHT"),
            ) {
                (w.parse().unwrap_or(1920), h.parse().unwrap_or(1080))
            } else {
                (1920, 1080)
            };

            let bounds = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(width as f32, height as f32),
            );

            let work_area = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(width as f32, (height - 24).max(0) as f32),
            );

            vec![DisplayInfo {
                name: "wayland-0".to_string(),
                bounds,
                work_area,
                scale_factor: 1.0,
                is_primary: true,
                video_modes: vec![VideoMode {
                    size: LayoutSize::new(width as isize, height as isize),
                    bit_depth: 32,
                    refresh_rate: 60,
                }],
            }]
        }

        // --- Sway/swaymsg Implementation ---
        fn try_swaymsg() -> Result<Vec<DisplayInfo>, ()> {
            // Only run if SWAYSOCK is set, which is specific to Sway
            if std::env::var("SWAYSOCK").is_err() {
                return Err(());
            }

            let output = Command::new("swaymsg")
                .arg("-t")
                .arg("get_outputs")
                .output()
                .map_err(|_| ())?;

            if !output.status.success() {
                return Err(());
            }

            let stdout = String::from_utf8(output.stdout).map_err(|_| ())?;

            #[cfg(feature = "desktop")]
            {
                use serde::Deserialize;

                #[derive(Deserialize)]
                struct SwayOutput {
                    name: String,
                    active: bool,
                    #[serde(default)]
                    primary: bool,
                    rect: SwayRect,
                    scale: f32,
                }

                #[derive(Deserialize)]
                struct SwayRect {
                    x: f32,
                    y: f32,
                    width: f32,
                    height: f32,
                }

                let outputs: Vec<SwayOutput> = serde_json::from_str(&stdout).map_err(|_| ())?;

                let mut displays: Vec<DisplayInfo> = outputs
                    .into_iter()
                    .filter(|o| o.active)
                    .map(|o| DisplayInfo {
                        name: o.name.clone(),
                        bounds: LogicalRect::new(
                            LogicalPosition::new(o.rect.x, o.rect.y),
                            LogicalSize::new(o.rect.width, o.rect.height),
                        ),
                        work_area: LogicalRect::new(
                            LogicalPosition::new(o.rect.x, o.rect.y),
                            LogicalSize::new(o.rect.width, (o.rect.height - 24.0).max(0.0)),
                        ),
                        scale_factor: o.scale,
                        is_primary: o.primary,
                        video_modes: vec![VideoMode {
                            size: LayoutSize::new(o.rect.width as isize, o.rect.height as isize),
                            bit_depth: 32,
                            refresh_rate: 60,
                        }],
                    })
                    .collect();

                // Ensure at least one primary display
                if !displays.is_empty() && !displays.iter().any(|d| d.is_primary) {
                    displays[0].is_primary = true;
                }

                if displays.is_empty() {
                    Err(())
                } else {
                    log_debug!(
                        LogCategory::General,
                        "[display] Detected {} display(s) using swaymsg",
                        displays.len()
                    );
                    Ok(displays)
                }
            }

            #[cfg(not(feature = "desktop"))]
            Err(())
        }

        // --- Hyprland/hyprctl Implementation ---
        fn try_hyprctl() -> Result<Vec<DisplayInfo>, ()> {
            let output = Command::new("hyprctl")
                .arg("monitors")
                .arg("-j")
                .output()
                .map_err(|_| ())?;

            if !output.status.success() {
                return Err(());
            }

            let stdout = String::from_utf8(output.stdout).map_err(|_| ())?;

            #[cfg(feature = "desktop")]
            {
                use serde::Deserialize;

                #[derive(Deserialize)]
                struct HyprlandOutput {
                    name: String,
                    x: f32,
                    y: f32,
                    width: f32,
                    height: f32,
                    scale: f32,
                    #[serde(default)]
                    focused: bool,
                }

                let outputs: Vec<HyprlandOutput> = serde_json::from_str(&stdout).map_err(|_| ())?;

                let mut displays: Vec<DisplayInfo> = outputs
                    .into_iter()
                    .map(|o| DisplayInfo {
                        name: o.name.clone(),
                        bounds: LogicalRect::new(
                            LogicalPosition::new(o.x, o.y),
                            LogicalSize::new(o.width, o.height),
                        ),
                        work_area: LogicalRect::new(
                            LogicalPosition::new(o.x, o.y),
                            LogicalSize::new(o.width, (o.height - 24.0).max(0.0)),
                        ),
                        scale_factor: o.scale,
                        is_primary: o.focused,
                        video_modes: vec![VideoMode {
                            size: LayoutSize::new(o.width as isize, o.height as isize),
                            bit_depth: 32,
                            refresh_rate: 60,
                        }],
                    })
                    .collect();

                // Ensure at least one primary display
                if !displays.is_empty() && !displays.iter().any(|d| d.is_primary) {
                    displays[0].is_primary = true;
                }

                if displays.is_empty() {
                    Err(())
                } else {
                    log_debug!(
                        LogCategory::General,
                        "[display] Detected {} display(s) using hyprctl",
                        displays.len()
                    );
                    Ok(displays)
                }
            }

            #[cfg(not(feature = "desktop"))]
            Err(())
        }

        // --- KDE/kscreen-doctor Implementation ---
        fn try_kscreen_doctor() -> Result<Vec<DisplayInfo>, ()> {
            let output = Command::new("kscreen-doctor")
                .arg("-o")
                .arg("--json")
                .output()
                .map_err(|_| ())?;

            if !output.status.success() {
                return Err(());
            }

            let stdout = String::from_utf8(output.stdout).map_err(|_| ())?;

            #[cfg(feature = "desktop")]
            {
                use serde::Deserialize;

                #[derive(Deserialize)]
                struct KdeOutputList {
                    outputs: Vec<KdeMonitor>,
                }

                #[derive(Deserialize)]
                struct KdeMonitor {
                    name: String,
                    enabled: bool,
                    #[serde(default)]
                    primary: bool,
                    geometry: KdeGeom,
                    scale: f32,
                }

                #[derive(Deserialize)]
                struct KdeGeom {
                    x: f32,
                    y: f32,
                    width: f32,
                    height: f32,
                }

                let output_list: KdeOutputList = serde_json::from_str(&stdout).map_err(|_| ())?;

                let mut displays: Vec<DisplayInfo> = output_list
                    .outputs
                    .into_iter()
                    .filter(|o| o.enabled)
                    .map(|o| DisplayInfo {
                        name: o.name.clone(),
                        bounds: LogicalRect::new(
                            LogicalPosition::new(o.geometry.x, o.geometry.y),
                            LogicalSize::new(o.geometry.width, o.geometry.height),
                        ),
                        work_area: LogicalRect::new(
                            LogicalPosition::new(o.geometry.x, o.geometry.y),
                            LogicalSize::new(o.geometry.width, (o.geometry.height - 24.0).max(0.0)),
                        ),
                        scale_factor: o.scale,
                        is_primary: o.primary,
                        video_modes: vec![VideoMode {
                            size: LayoutSize::new(
                                o.geometry.width as isize,
                                o.geometry.height as isize,
                            ),
                            bit_depth: 32,
                            refresh_rate: 60,
                        }],
                    })
                    .collect();

                // Ensure at least one primary display
                if !displays.is_empty() && !displays.iter().any(|d| d.is_primary) {
                    displays[0].is_primary = true;
                }

                if displays.is_empty() {
                    Err(())
                } else {
                    log_debug!(
                        LogCategory::General,
                        "[display] Detected {} display(s) using kscreen-doctor",
                        displays.len()
                    );
                    Ok(displays)
                }
            }

            #[cfg(not(feature = "desktop"))]
            Err(())
        }

        // --- wlroots/wlr-randr Implementation ---
        fn try_wlr_randr() -> Result<Vec<DisplayInfo>, ()> {
            let output = Command::new("wlr-randr").output().map_err(|_| ())?;

            if !output.status.success() {
                return Err(());
            }

            let stdout = String::from_utf8(output.stdout).map_err(|_| ())?;

            #[cfg(feature = "desktop")]
            {
                // Parse wlr-randr text output using string ops (no regex).
                // Header lines are non-indented, non-empty, and contain a quoted name, e.g.:
                //   HDMI-A-1 "Dell U2415" (focused)
                // Property lines are indented, e.g.:
                //   Position: 0,0
                //   1920x1200 px, 59.95 Hz (preferred, current)
                //   Scale: 1.000000

                let mut displays = Vec::new();
                let lines: Vec<&str> = stdout.lines().collect();
                let mut i = 0;

                while i < lines.len() {
                    let line = lines[i];

                    // Check if this is an output header: non-indented, non-empty, contains '"'
                    let is_header = !line.starts_with(' ')
                        && !line.is_empty()
                        && line.contains('"');

                    if is_header {
                        // Extract output name (first whitespace-delimited token)
                        let name = line
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_string();

                        let is_focused = line.contains("(focused)");

                        // Look for properties in the next few lines
                        let mut x = 0.0f32;
                        let mut y = 0.0f32;
                        let mut width = 0.0f32;
                        let mut height = 0.0f32;
                        let mut scale = 1.0f32;

                        for j in (i + 1)..((i + 10).min(lines.len())) {
                            let prop_line = lines[j];

                            // Stop if we hit another output header
                            if !prop_line.starts_with(' ') && !prop_line.is_empty() {
                                break;
                            }

                            let trimmed = prop_line.trim();

                            // Parse "Position: X,Y"
                            if let Some(rest) = trimmed.strip_prefix("Position:") {
                                if let Some((xs, ys)) = rest.trim().split_once(',') {
                                    x = xs.trim().parse().unwrap_or(0.0);
                                    y = ys.trim().parse().unwrap_or(0.0);
                                }
                            }

                            // Parse mode lines like "1920x1200 px, 59.95 Hz (preferred, current)"
                            if trimmed.contains("current") && trimmed.contains("px") {
                                if let Some(wh) = trimmed.split_whitespace().next() {
                                    if let Some((ws, hs)) = wh.split_once('x') {
                                        width = ws.parse().unwrap_or(0.0);
                                        height = hs.parse().unwrap_or(0.0);
                                    }
                                }
                            }

                            // Parse "Scale: 1.000000"
                            if let Some(rest) = trimmed.strip_prefix("Scale:") {
                                scale = rest.trim().parse().unwrap_or(1.0);
                            }
                        }

                        if width > 0.0 && height > 0.0 {
                            displays.push(DisplayInfo {
                                name,
                                bounds: LogicalRect::new(
                                    LogicalPosition::new(x, y),
                                    LogicalSize::new(width, height),
                                ),
                                work_area: LogicalRect::new(
                                    LogicalPosition::new(x, y),
                                    LogicalSize::new(width, (height - 24.0).max(0.0)),
                                ),
                                scale_factor: scale,
                                is_primary: is_focused || (x == 0.0 && y == 0.0),
                                video_modes: vec![VideoMode {
                                    size: LayoutSize::new(width as isize, height as isize),
                                    bit_depth: 32,
                                    refresh_rate: 60, /* Wayland doesn't easily expose this via
                                                       * wlr-randr */
                                }],
                            });
                        }
                    }

                    i += 1;
                }

                // Ensure at least one primary display
                if !displays.is_empty() && !displays.iter().any(|d| d.is_primary) {
                    displays[0].is_primary = true;
                }

                if displays.is_empty() {
                    Err(())
                } else {
                    log_debug!(
                        LogCategory::General,
                        "[display] Detected {} display(s) using wlr-randr",
                        displays.len()
                    );
                    Ok(displays)
                }
            }

            #[cfg(not(feature = "desktop"))]
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: These tests require the main thread on macOS and real display hardware.
    // They are marked as #[ignore] for regular unit testing but can be run manually.

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_get_displays() {
        let displays = get_displays();
        assert!(!displays.is_empty(), "Should have at least one display");

        // Should have exactly one primary display
        let primary_count = displays.iter().filter(|d| d.is_primary).count();
        assert_eq!(primary_count, 1, "Should have exactly one primary display");

        // All displays should have valid dimensions
        for display in &displays {
            assert!(display.bounds.size.width > 0.0);
            assert!(display.bounds.size.height > 0.0);
            assert!(display.scale_factor > 0.0);
        }
    }

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_get_primary_display() {
        let primary = get_primary_display();
        assert!(primary.is_some(), "Should have a primary display");

        if let Some(display) = primary {
            assert!(display.is_primary);
            assert!(display.bounds.size.width > 0.0);
            assert!(display.bounds.size.height > 0.0);
        }
    }

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_get_display_at_point() {
        let displays = get_displays();
        if displays.is_empty() {
            return;
        }

        // Test point in the middle of the first display
        let first = &displays[0];
        let center = LogicalPosition::new(
            first.bounds.origin.x + first.bounds.size.width / 2.0,
            first.bounds.origin.y + first.bounds.size.height / 2.0,
        );

        let found = get_display_at_point(center);
        assert!(found.is_some(), "Should find display at center point");
    }
}
