//! Wayland tooltip implementation using wl_subsurface
//!
//! This module provides tooltip display for Wayland windows using subsurfaces.
//! A subsurface is a child surface that can be positioned relative to its parent
//! and is composited together with the parent by the compositor.
//!
//! Note: Text rendering is currently a placeholder (solid rectangles per character).
//!
//! TODO(superplan): two follow-ups, both currently blocked on files this module
//! cannot reach from here:
//!   1. Wire `render_tooltip_content` into Azul's real text-shaping pipeline
//!      instead of drawing black-bar placeholders. Unlike X11/macOS/Windows —
//!      which delegate tooltip text to native server-side/UI text drawing
//!      (`XDrawString` / `NSTextField` / GDI) — Wayland has no native text path,
//!      so this needs an `FcFontCache` + shaped glyphs (`ParsedFont`) threaded in
//!      via `new()`/`show()` and rasterized into the ARGB8888 `wl_shm` buffer.
//!      Needs runtime verification on a real compositor.
//!   2. Align `show`/`hide` with the other backends' signatures
//!      (`show(text, position: LogicalPosition, dpi: DpiScaleFactor) -> Result<…>`,
//!      `hide() -> Result<…>`, plus `is_visible()`). That changes the call sites
//!      in `linux/wayland/mod.rs` (~:5095, a Group 7 file), so it must land
//!      together with the per-backend wiring.

use std::ffi::CString;
use std::rc::Rc;

use super::{defines::*, dlopen::Wayland};

use super::super::super::common::debug_server::LogCategory;
use crate::log_error;

/// Approximate width of a single character in pixels (placeholder font metrics)
const TOOLTIP_CHAR_WIDTH_PX: i32 = 7;
/// Approximate height of a single character in pixels (placeholder font metrics)
const TOOLTIP_CHAR_HEIGHT_PX: i32 = 14;
/// Padding around tooltip text in pixels
const TOOLTIP_PADDING_PX: i32 = 4;

/// Tooltip window using Wayland wl_subsurface
///
/// Wayland doesn't have a direct "tooltip" protocol, so we implement tooltips
/// using a subsurface that:
/// - Is positioned relative to the parent surface
/// - Uses wl_shm for software rendering (simple text rendering)
/// - Is shown/hidden by attaching/detaching the buffer
pub struct TooltipWindow {
    wayland: Rc<Wayland>,
    display: *mut wl_display,
    parent_surface: *mut wl_surface,
    compositor: *mut wl_compositor,
    shm: *mut wl_shm,
    subcompositor: *mut wl_subcompositor,

    // Subsurface for tooltip
    surface: *mut wl_surface,
    subsurface: *mut wl_subsurface,

    // Shared memory buffer for rendering
    pool: Option<*mut wl_shm_pool>,
    buffer: Option<*mut wl_buffer>,
    data: Option<*mut u8>,
    mapped_size: usize, // Size of the mmap'd region for proper cleanup
    width: i32,
    height: i32,
}

impl TooltipWindow {
    /// Create a new tooltip subsurface
    pub fn new(
        wayland: Rc<Wayland>,
        display: *mut wl_display,
        parent_surface: *mut wl_surface,
        compositor: *mut wl_compositor,
        shm: *mut wl_shm,
        subcompositor: *mut wl_subcompositor,
    ) -> Result<Self, String> {
        if compositor.is_null() {
            return Err("Compositor not available".to_string());
        }

        if subcompositor.is_null() {
            return Err("Subcompositor not available".to_string());
        }

        if shm.is_null() {
            return Err("Shared memory not available".to_string());
        }

        unsafe {
            // Create subsurface for tooltip
            let surface = (wayland.wl_compositor_create_surface)(compositor);
            if surface.is_null() {
                return Err("Failed to create tooltip surface".to_string());
            }

            // Create subsurface from parent
            let subsurface =
                (wayland.wl_subcompositor_get_subsurface)(subcompositor, surface, parent_surface);
            if subsurface.is_null() {
                (wayland.wl_surface_destroy)(surface);
                return Err("Failed to create subsurface".to_string());
            }

            // Set subsurface to desynchronized mode for immediate updates
            (wayland.wl_subsurface_set_desync)(subsurface);

            Ok(Self {
                wayland,
                display,
                parent_surface,
                compositor,
                shm,
                subcompositor,
                surface,
                subsurface,
                pool: None,
                buffer: None,
                data: None,
                mapped_size: 0,
                width: 0,
                height: 0,
            })
        }
    }

    /// Show the tooltip at the given position with text.
    ///
    /// TODO(superplan): align signature with X11/macOS/Windows
    /// (`text, position: LogicalPosition, dpi: DpiScaleFactor) -> Result<(), String>`);
    /// blocked on the `linux/wayland/mod.rs` caller (see module-level note).
    pub fn show(&mut self, text: &str, x: i32, y: i32) {
        unsafe {
            let text_width = text.len() as i32 * TOOLTIP_CHAR_WIDTH_PX;
            let width = text_width + TOOLTIP_PADDING_PX * 2;
            let height = TOOLTIP_CHAR_HEIGHT_PX + TOOLTIP_PADDING_PX * 2;

            if self.width != width || self.height != height || self.buffer.is_none() {
                if let Err(e) = self.allocate_shm_buffer(width, height) {
                    log_error!(LogCategory::Resources, "[Wayland] {}", e);
                    return;
                }
            }

            if let Some(data) = self.data {
                Self::render_tooltip_content(data, self.width, self.height, text);
            }

            // Position subsurface
            (self.wayland.wl_subsurface_set_position)(self.subsurface, x, y);

            // Attach buffer and commit
            if let Some(buffer) = self.buffer {
                (self.wayland.wl_surface_attach)(self.surface, buffer, 0, 0);
                (self.wayland.wl_surface_damage)(self.surface, 0, 0, self.width, self.height);
                (self.wayland.wl_surface_commit)(self.surface);
            }

            (self.wayland.wl_display_flush)(self.display);
        }
    }

    /// Hide the tooltip
    pub fn hide(&mut self) {
        unsafe {
            // Detach buffer to hide the surface
            (self.wayland.wl_surface_attach)(self.surface, std::ptr::null_mut(), 0, 0);
            (self.wayland.wl_surface_commit)(self.surface);
            (self.wayland.wl_display_flush)(self.display);
        }
    }

    /// Allocate a shared memory buffer for tooltip rendering.
    /// Uses `memfd_create` with `shm_open` fallback (matching mod.rs pattern).
    fn allocate_shm_buffer(&mut self, width: i32, height: i32) -> Result<(), String> {
        self.cleanup_buffer();

        let stride = width * 4; // ARGB8888
        let size = stride * height;

        // Try memfd_create first (Linux 3.17+, glibc 2.27+)
        // Fall back to shm_open for older systems
        let fd = unsafe {
            #[cfg(target_os = "linux")]
            {
                let result = libc::syscall(
                    libc::SYS_memfd_create,
                    CString::new("azul-tooltip").unwrap().as_ptr(),
                    1 as libc::c_int, // MFD_CLOEXEC
                );

                if result != -1 {
                    result as libc::c_int
                } else {
                    let name = CString::new(format!(
                        "/azul-tooltip-{}",
                        std::process::id()
                    )).unwrap();
                    let fd = libc::shm_open(
                        name.as_ptr(),
                        libc::O_CREAT | libc::O_RDWR | libc::O_EXCL,
                        0o600,
                    );
                    if fd != -1 {
                        libc::shm_unlink(name.as_ptr());
                    }
                    fd
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                -1
            }
        };

        if fd < 0 {
            return Err("Failed to create shared memory".to_string());
        }

        unsafe {
            if libc::ftruncate(fd, size as libc::off_t) < 0 {
                libc::close(fd);
                return Err("Failed to resize shared memory".to_string());
            }

            let data = libc::mmap(
                std::ptr::null_mut(),
                size as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            ) as *mut u8;

            if data == libc::MAP_FAILED as *mut u8 {
                libc::close(fd);
                return Err("Failed to mmap shared memory".to_string());
            }

            let pool = (self.wayland.wl_shm_create_pool)(self.shm, fd, size);
            libc::close(fd);

            if pool.is_null() {
                libc::munmap(data as *mut libc::c_void, size as usize);
                return Err("Failed to create shm pool".to_string());
            }

            let buffer = (self.wayland.wl_shm_pool_create_buffer)(
                pool,
                0,
                width,
                height,
                stride,
                WL_SHM_FORMAT_ARGB8888,
            );

            if buffer.is_null() {
                (self.wayland.wl_shm_pool_destroy)(pool);
                libc::munmap(data as *mut libc::c_void, size as usize);
                return Err("Failed to create buffer".to_string());
            }

            self.mapped_size = size as usize;
            self.pool = Some(pool);
            self.buffer = Some(buffer);
            self.data = Some(data);
            self.width = width;
            self.height = height;
        }

        Ok(())
    }

    /// Render tooltip background and placeholder text into the pixel buffer.
    ///
    /// TODO(superplan): replace the per-character black-bar placeholder below
    /// with glyphs shaped through Azul's text pipeline (needs an `FcFontCache` +
    /// `ParsedFont` threaded in and rasterized into this ARGB8888 buffer). See
    /// the module-level note. Needs runtime verification on a real compositor.
    fn render_tooltip_content(data: *mut u8, width: i32, height: i32, text: &str) {
        let stride = width * 4;

        unsafe {
            // Fill background (light yellow: 0xFFFFF0)
            for y in 0..height {
                for x in 0..width {
                    let offset = (y * stride + x * 4) as isize;
                    *data.offset(offset + 0) = 0xF0; // Blue
                    *data.offset(offset + 1) = 0xFF; // Green
                    *data.offset(offset + 2) = 0xFF; // Red
                    *data.offset(offset + 3) = 0xFF; // Alpha
                }
            }

            // Draw text (very simple - just black pixels)
            // In a real implementation, you'd use a proper font rendering library
            let text_x = TOOLTIP_PADDING_PX;
            let text_y = TOOLTIP_PADDING_PX;

            for (i, _ch) in text.chars().enumerate() {
                let char_x = text_x + i as i32 * TOOLTIP_CHAR_WIDTH_PX;

                // Draw a simple rectangle as placeholder for each character
                for dy in 0..TOOLTIP_CHAR_HEIGHT_PX {
                    for dx in 0..TOOLTIP_CHAR_WIDTH_PX - 1 {
                        let px = char_x + dx;
                        let py = text_y + dy;

                        if px >= 0 && px < width && py >= 0 && py < height {
                            let offset = (py * stride + px * 4) as isize;
                            *data.offset(offset + 0) = 0x00; // Blue
                            *data.offset(offset + 1) = 0x00; // Green
                            *data.offset(offset + 2) = 0x00; // Red
                            *data.offset(offset + 3) = 0xFF; // Alpha
                        }
                    }
                }
            }
        }
    }

    /// Cleanup buffer resources
    fn cleanup_buffer(&mut self) {
        unsafe {
            if let Some(buffer) = self.buffer.take() {
                (self.wayland.wl_buffer_destroy)(buffer);
            }

            if let Some(pool) = self.pool.take() {
                (self.wayland.wl_shm_pool_destroy)(pool);
            }

            if let Some(data) = self.data.take() {
                // Use the stored mapped_size instead of calculating from current width/height
                // This ensures we unmap the exact size that was originally mapped
                libc::munmap(data as *mut libc::c_void, self.mapped_size);
                self.mapped_size = 0;
            }
        }
    }
}

impl Drop for TooltipWindow {
    fn drop(&mut self) {
        unsafe {
            self.cleanup_buffer();

            if !self.subsurface.is_null() {
                (self.wayland.wl_subsurface_destroy)(self.subsurface);
            }

            if !self.surface.is_null() {
                (self.wayland.wl_surface_destroy)(self.surface);
            }
        }
    }
}
