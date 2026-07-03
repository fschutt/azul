//! Wayland tooltip implementation using wl_subsurface
//!
//! This module provides tooltip display for Wayland windows using subsurfaces.
//! A subsurface is a child surface that can be positioned relative to its parent
//! and is composited together with the parent by the compositor.
//!
//! Text rendering: shaped glyphs are rasterized through Azul's CPU text pipeline
//! (`azul_layout::cpurender::render_text_run_to_pixmap`) into the ARGB8888
//! `wl_shm` buffer. Unlike X11/macOS/Windows — which delegate tooltip text to
//! native server-side/UI text drawing (`XDrawString` / `NSTextField` / GDI) —
//! Wayland has no native text path, so the client must rasterize the glyphs
//! itself. The `FcFontCache` used to resolve the font is threaded in via
//! `new()`. Runtime verification needs a real Wayland compositor.

use std::ffi::CString;
use std::rc::Rc;
use std::sync::Arc;

use azul_core::geom::LogicalPosition;
use azul_core::resources::DpiScaleFactor;
use azul_css::props::basic::ColorU;
use rust_fontconfig::FcFontCache;

use super::{defines::*, dlopen::Wayland};

use super::super::super::common::debug_server::LogCategory;
use crate::log_error;

/// Tooltip font size in logical pixels.
const TOOLTIP_FONT_SIZE_PX: f32 = 12.0;
/// Padding around tooltip text in logical pixels.
const TOOLTIP_PADDING_PX: f32 = 4.0;
/// Fallback per-character width (logical px) used only when no system font can
/// be resolved (so an empty-but-positioned tooltip box still appears).
const TOOLTIP_FALLBACK_CHAR_WIDTH_PX: f32 = 7.0;
/// Fallback line height (logical px) for the no-font path.
const TOOLTIP_FALLBACK_LINE_HEIGHT_PX: f32 = 14.0;
/// Tooltip background colour (light yellow), matching the X11 backend.
const TOOLTIP_BG_COLOR: ColorU = ColorU { r: 0xFF, g: 0xFF, b: 0xF0, a: 0xFF };
/// Tooltip text colour (black), matching the X11 backend.
const TOOLTIP_TEXT_COLOR: ColorU = ColorU { r: 0x00, g: 0x00, b: 0x00, a: 0xFF };

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

    /// wp_viewport for the tooltip surface (present when the compositor has
    /// wp_viewporter). The tooltip buffer is rendered at DEVICE resolution;
    /// the viewport maps it back to the logical size so it isn't displayed
    /// dpi× oversized on scaled outputs (works for fractional scales too).
    viewport: Option<*mut wp_viewport>,

    // Shared memory buffer for rendering
    pool: Option<*mut wl_shm_pool>,
    buffer: Option<*mut wl_buffer>,
    data: Option<*mut u8>,
    mapped_size: usize, // Size of the mmap'd region for proper cleanup
    width: i32,
    height: i32,

    /// Font cache used to resolve + shape the tooltip text (Wayland has no
    /// native server-side text drawing, so glyphs are rasterized client-side).
    fc_cache: Arc<FcFontCache>,
    /// Whether the tooltip is currently mapped/visible.
    is_visible: bool,
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
        viewporter: Option<*mut wp_viewporter>,
        fc_cache: Arc<FcFontCache>,
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

            // One wp_viewport per surface (when the compositor supports
            // viewporter): maps the device-resolution buffer to logical size.
            let viewport = viewporter
                .and_then(|vpr| super::wp_viewporter_get_viewport(&wayland, vpr, surface));

            Ok(Self {
                wayland,
                display,
                parent_surface,
                compositor,
                shm,
                subcompositor,
                surface,
                subsurface,
                viewport,
                pool: None,
                buffer: None,
                data: None,
                mapped_size: 0,
                width: 0,
                height: 0,
                fc_cache,
                is_visible: false,
            })
        }
    }

    /// Show the tooltip with `text` at `position`.
    ///
    /// Signature aligned with the X11/macOS/Windows backends
    /// (`text`, `position: LogicalPosition`, `dpi: DpiScaleFactor`) -> `Result`.
    /// The text is shaped + rasterized through Azul's CPU text pipeline into the
    /// `wl_shm` buffer (no native server-side text path exists on Wayland).
    pub fn show(
        &mut self,
        text: &str,
        position: LogicalPosition,
        dpi_factor: DpiScaleFactor,
    ) -> Result<(), String> {
        let dpi = dpi_factor.inner.get();

        // Shape + rasterize the tooltip text at device resolution. The pixmap is
        // RGBA8 (`AzulPixmap::data`); it is converted to ARGB8888 (BGRA byte
        // order) when copied into the `wl_shm` buffer below.
        let pixmap = azul_layout::cpurender::render_text_run_to_pixmap(
            &self.fc_cache,
            text,
            TOOLTIP_FONT_SIZE_PX,
            TOOLTIP_TEXT_COLOR,
            TOOLTIP_BG_COLOR,
            TOOLTIP_PADDING_PX,
            dpi,
        );

        // Determine the device-pixel buffer size: from the shaped pixmap when a
        // font was available, otherwise a fallback box sized by char count so a
        // (text-less) tooltip still appears.
        let (width, height) = match &pixmap {
            Some(p) => (p.width() as i32, p.height() as i32),
            None => {
                let logical_w =
                    text.chars().count() as f32 * TOOLTIP_FALLBACK_CHAR_WIDTH_PX
                        + TOOLTIP_PADDING_PX * 2.0;
                let logical_h = TOOLTIP_FALLBACK_LINE_HEIGHT_PX + TOOLTIP_PADDING_PX * 2.0;
                (
                    ((logical_w * dpi).ceil() as i32).max(1),
                    ((logical_h * dpi).ceil() as i32).max(1),
                )
            }
        };

        if self.width != width || self.height != height || self.buffer.is_none() {
            self.allocate_shm_buffer(width, height)?;
        }

        if let Some(data) = self.data {
            match &pixmap {
                Some(p) => Self::blit_pixmap(data, self.width, self.height, p),
                None => Self::render_fallback_background(data, self.width, self.height),
            }
        }

        unsafe {
            // Position subsurface (surface-local coordinates, relative to parent).
            (self.wayland.wl_subsurface_set_position)(
                self.subsurface,
                position.x as i32,
                position.y as i32,
            );

            // Attach buffer and commit.
            if let Some(buffer) = self.buffer {
                (self.wayland.wl_surface_attach)(self.surface, buffer, 0, 0);
                // The buffer is at DEVICE resolution; map it to the logical
                // size via the viewport so scaled outputs (integer AND
                // fractional) don't display it dpi× oversized. Without
                // viewporter the legacy 1:1 behavior is kept.
                if let Some(vp) = self.viewport {
                    super::wp_viewport_set_destination(
                        &self.wayland,
                        vp,
                        ((self.width as f32 / dpi).ceil() as i32).max(1),
                        ((self.height as f32 / dpi).ceil() as i32).max(1),
                    );
                }
                (self.wayland.wl_surface_damage)(self.surface, 0, 0, self.width, self.height);
                (self.wayland.wl_surface_commit)(self.surface);
            }

            (self.wayland.wl_display_flush)(self.display);
        }

        self.is_visible = true;
        Ok(())
    }

    /// Hide the tooltip
    pub fn hide(&mut self) -> Result<(), String> {
        unsafe {
            // Detach buffer to hide the surface
            (self.wayland.wl_surface_attach)(self.surface, std::ptr::null_mut(), 0, 0);
            (self.wayland.wl_surface_commit)(self.surface);
            (self.wayland.wl_display_flush)(self.display);
        }
        self.is_visible = false;
        Ok(())
    }

    /// Whether the tooltip is currently visible.
    pub fn is_visible(&self) -> bool {
        self.is_visible
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

    /// Copy a shaped, rasterized [`AzulPixmap`] (RGBA8) into the `wl_shm` buffer
    /// (ARGB8888 little-endian = BGRA byte order). The pixmap already contains
    /// the tooltip background + shaped glyphs at device resolution, so this is a
    /// straight per-pixel channel swap.
    fn blit_pixmap(
        data: *mut u8,
        width: i32,
        height: i32,
        pixmap: &azul_layout::cpurender::AzulPixmap,
    ) {
        let stride = width * 4;
        let src = pixmap.data();
        let src_w = pixmap.width() as i32;
        let src_h = pixmap.height() as i32;
        let copy_w = width.min(src_w);
        let copy_h = height.min(src_h);
        let src_stride = src_w * 4;

        unsafe {
            for y in 0..copy_h {
                for x in 0..copy_w {
                    let s = (y * src_stride + x * 4) as usize;
                    let d = (y * stride + x * 4) as isize;
                    let r = src[s];
                    let g = src[s + 1];
                    let b = src[s + 2];
                    let a = src[s + 3];
                    *data.offset(d) = b; // Blue
                    *data.offset(d + 1) = g; // Green
                    *data.offset(d + 2) = r; // Red
                    *data.offset(d + 3) = a; // Alpha
                }
            }
        }
    }

    /// Fallback for when no system font could be resolved: fill the buffer with
    /// the tooltip background colour so a positioned (text-less) box still shows.
    fn render_fallback_background(data: *mut u8, width: i32, height: i32) {
        let stride = width * 4;
        unsafe {
            for y in 0..height {
                for x in 0..width {
                    let offset = (y * stride + x * 4) as isize;
                    *data.offset(offset) = TOOLTIP_BG_COLOR.b; // Blue
                    *data.offset(offset + 1) = TOOLTIP_BG_COLOR.g; // Green
                    *data.offset(offset + 2) = TOOLTIP_BG_COLOR.r; // Red
                    *data.offset(offset + 3) = TOOLTIP_BG_COLOR.a; // Alpha
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

            // The viewport must go before its wl_surface (protocol).
            if let Some(vp) = self.viewport.take() {
                super::wp_viewport_destroy(&self.wayland, vp);
            }

            if !self.subsurface.is_null() {
                (self.wayland.wl_subsurface_destroy)(self.subsurface);
            }

            if !self.surface.is_null() {
                (self.wayland.wl_surface_destroy)(self.surface);
            }
        }
    }
}
