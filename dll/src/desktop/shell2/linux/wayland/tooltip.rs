//! Wayland tooltip implementation using wl_subsurface
//!
//! This module provides tooltip display for Wayland windows using subsurfaces.
//! A subsurface is a child surface that can be positioned relative to its parent
//! and is composited together with the parent by the compositor.

use std::rc::Rc;

use super::{defines::*, dlopen::Wayland};

use super::super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

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

    /// Show the tooltip at the given position with text
    pub fn show(&mut self, text: String, x: i32, y: i32) {
        unsafe {
            // Calculate tooltip size (simple estimation)
            let char_width = 7;
            let char_height = 14;
            let padding = 4;

            let text_width = text.len() as i32 * char_width;
            let width = text_width + padding * 2;
            let height = char_height + padding * 2;

            // Create or recreate buffer if size changed
            if self.width != width || self.height != height || self.buffer.is_none() {
                self.cleanup_buffer();

                let stride = width * 4; // ARGB8888
                let size = stride * height;

                // Create shared memory file
                let shm_name =
                    std::ffi::CString::new(format!("/azul-tooltip-{}", std::process::id()))
                        .unwrap();
                let fd = libc::shm_open(
                    shm_name.as_ptr(),
                    libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                    0o600,
                );

                if fd < 0 {
                    log_error!(
                        LogCategory::Resources,
                        "[Wayland] Failed to create shared memory"
                    );
                    return;
                }

                libc::shm_unlink(shm_name.as_ptr());

                if libc::ftruncate(fd, size as i64) < 0 {
                    libc::close(fd);
                    log_error!(
                        LogCategory::Resources,
                        "[Wayland] Failed to resize shared memory"
                    );
                    return;
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
                    log_error!(
                        LogCategory::Resources,
                        "[Wayland] Failed to mmap shared memory"
                    );
                    return;
                }

                // Create wl_shm_pool
                let pool = (self.wayland.wl_shm_create_pool)(self.shm, fd, size);
                libc::close(fd);

                if pool.is_null() {
                    libc::munmap(data as *mut libc::c_void, size as usize);
                    log_error!(
                        LogCategory::Resources,
                        "[Wayland] Failed to create shm pool"
                    );
                    return;
                }

                // Create wl_buffer
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
                    log_error!(LogCategory::Resources, "[Wayland] Failed to create buffer");
                    return;
                }

                // Store the mapped size for cleanup
                self.mapped_size = size as usize;
                self.pool = Some(pool);
                self.buffer = Some(buffer);
                self.data = Some(data);
                self.width = width;
                self.height = height;
            }

            // Render tooltip (simple software rendering)
            if let Some(data) = self.data {
                let stride = self.width * 4;

                // Fill background (light yellow: 0xFFFFF0)
                for y in 0..self.height {
                    for x in 0..self.width {
                        let offset = (y * stride + x * 4) as isize;
                        *data.offset(offset + 0) = 0xF0; // Blue
                        *data.offset(offset + 1) = 0xFF; // Green
                        *data.offset(offset + 2) = 0xFF; // Red
                        *data.offset(offset + 3) = 0xFF; // Alpha
                    }
                }

                // Draw text (very simple - just black pixels)
                // In a real implementation, you'd use a proper font rendering library
                let text_x = padding;
                let text_y = padding;

                for (i, _ch) in text.chars().enumerate() {
                    let char_x = text_x + i as i32 * char_width;

                    // Draw a simple rectangle as placeholder for each character
                    for dy in 0..char_height {
                        for dx in 0..char_width - 1 {
                            let px = char_x + dx;
                            let py = text_y + dy;

                            if px >= 0 && px < self.width && py >= 0 && py < self.height {
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
