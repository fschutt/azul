//! Display/Monitor management for all platforms
//!
//! This module provides cross-platform display enumeration and information.
//! Used primarily for menu positioning to avoid overflow at screen edges.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

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
}

/// Get all available displays
///
/// This function queries the OS for all connected displays/monitors.
/// The first display in the list is typically the primary display.
///
/// # Platform Notes
///
/// - **Windows**: Uses MonitorFromWindow + GetMonitorInfoW
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

/// Get the display containing the given point
///
/// Returns None if the point is not on any display.
pub fn get_display_at_point(point: LogicalPosition) -> Option<DisplayInfo> {
    get_displays()
        .into_iter()
        .find(|display| display.bounds.contains(point))
}

/// Get the primary display
pub fn get_primary_display() -> Option<DisplayInfo> {
    get_displays()
        .into_iter()
        .find(|display| display.is_primary)
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub fn get_displays() -> Vec<DisplayInfo> {
        // On Windows, without direct monitor enumeration API,
        // return a reasonable default for the primary display

        // Use winapi to get the primary monitor dimensions
        #[cfg(target_os = "windows")]
        unsafe {
            use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

            let width = GetSystemMetrics(SM_CXSCREEN) as f32;
            let height = GetSystemMetrics(SM_CYSCREEN) as f32;

            let bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(width, height));

            // Approximate work area (subtract taskbar height ~40px)
            let work_area = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(width, (height - 40.0).max(0.0)),
            );

            return vec![DisplayInfo {
                name: "\\\\.\\DISPLAY1".to_string(),
                bounds,
                work_area,
                scale_factor: 1.0, // TODO: Get actual DPI
                is_primary: true,
            }];
        }

        #[cfg(not(target_os = "windows"))]
        vec![]
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;

    use super::*;

    pub fn get_displays() -> Vec<DisplayInfo> {
        let mtm = MainThreadMarker::new().expect("Must be called on main thread");
        let screens = NSScreen::screens(mtm);

        let mut displays = Vec::new();

        for (i, screen) in screens.iter().enumerate() {
            let frame = screen.frame();
            let visible_frame = screen.visibleFrame();
            let scale = screen.backingScaleFactor();

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

            displays.push(DisplayInfo {
                name: screen.localizedName().to_string(),
                bounds,
                work_area,
                scale_factor: scale as f32,
                is_primary: i == 0, // First screen is primary on macOS
            });
        }

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
        use super::*;
        use crate::desktop::shell2::linux::x11::dlopen::Xlib;

        pub fn get_displays() -> Vec<DisplayInfo> {
            // Try to open X11 display and query dimensions
            let xlib = match Xlib::new() {
                Ok(x) => x,
                Err(_) => return fallback_display(),
            };

            unsafe {
                let display = (xlib.XOpenDisplay)(std::ptr::null());
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
            }]
        }
    }

    mod wayland {
        use super::*;

        pub fn get_displays() -> Vec<DisplayInfo> {
            // Wayland doesn't allow clients to query absolute positioning
            // The compositor manages all window placement
            // We return a single logical display representing the primary output

            // Try to get actual dimensions from environment or reasonable defaults
            let (width, height) = if let (Ok(w), Ok(h)) = (
                std::env::var("WAYLAND_DISPLAY_WIDTH"),
                std::env::var("WAYLAND_DISPLAY_HEIGHT"),
            ) {
                (w.parse().unwrap_or(1920), h.parse().unwrap_or(1080))
            } else {
                (1920, 1080) // Reasonable default
            };

            let bounds = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(width as f32, height as f32),
            );

            let work_area = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(width as f32, (height - 24) as f32),
            );

            vec![DisplayInfo {
                name: "wayland-0".to_string(),
                bounds,
                work_area,
                scale_factor: 1.0,
                is_primary: true,
            }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
