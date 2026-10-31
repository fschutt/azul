//! Wayland tooltip: an `xdg_popup` without a grab, placed by the compositor.
//!
//! A Wayland client never learns where its window is on the screen, so it
//! cannot keep a tooltip on the screen itself. An `xdg_popup` can: its
//! positioner is handed the tooltip's anchor box on the parent surface and the
//! rule every floating surface follows (`azul_layout::placement`), and the
//! compositor flips it above the anchor or slides it back on where the screen
//! has no room. Without a grab the popup takes neither the pointer nor the
//! keyboard - and its input region is empty, so the pointer passes through it.
//! (A `wl_subsurface` is placed by the client in parent coordinates and is
//! never constrained to the screen at all.)
//!
//! Text rendering: Wayland has no text drawing of its own, so the text is
//! shaped, wrapped at the tooltip width and rasterized by Azul's CPU text path
//! (`azul_layout::cpurender::render_wrapped_text_to_pixmap`) into an ARGB8888
//! `wl_shm` buffer at device resolution; a `wp_viewport` maps it back to its
//! logical size. Runtime verification needs a real Wayland compositor.

use std::{cell::Cell, ffi::c_void, rc::Rc, sync::Arc};

use azul_core::geom::{LogicalRect, LogicalSize};
use azul_css::props::basic::ColorU;
use azul_layout::placement::{tooltip_text_width, FloatingPlacement};
use rust_fontconfig::FcFontCache;

use super::{super::super::common::debug_server::LogCategory, defines::*, dlopen::Wayland};
use crate::log_error;

/// Tooltip font size in logical pixels.
const TOOLTIP_FONT_SIZE_PX: f32 = 12.0;
/// Padding around tooltip text in logical pixels.
const TOOLTIP_PADDING_PX: f32 = 4.0;
/// The tallest a tooltip's text grows when no output's size is known, in
/// logical pixels.
const FALLBACK_MAX_TEXT_HEIGHT_PX: f32 = 1000.0;
/// Tooltip background colour (light yellow), matching the X11 backend.
const TOOLTIP_BG_COLOR: ColorU = ColorU {
    r: 0xFF,
    g: 0xFF,
    b: 0xF0,
    a: 0xFF,
};
/// Tooltip text colour (black), matching the X11 backend.
const TOOLTIP_TEXT_COLOR: ColorU = ColorU {
    r: 0x00,
    g: 0x00,
    b: 0x00,
    a: 0xFF,
};

/// What the tooltip popup's listeners need: the buffer to show on the surface
/// once the compositor has configured (and so placed) the popup.
struct PopupState {
    wayland: Rc<Wayland>,
    surface: *mut wl_surface,
    buffer: *mut wl_buffer,
    viewport: Option<*mut wp_viewport>,
    /// The surface's size in logical pixels (the viewport destination).
    logical_size: (i32, i32),
    /// The buffer's size in device pixels.
    device_size: (i32, i32),
    /// The compositor dismissed the popup (`popup_done`).
    dismissed: Cell<bool>,
}

/// One shown tooltip: its popup and the objects behind it.
struct TooltipPopup {
    surface: *mut wl_surface,
    xdg_surface: *mut xdg_surface,
    xdg_popup: *mut xdg_popup,
    viewport: Option<*mut wp_viewport>,
    /// Owned; freed after the popup's proxies are gone.
    state: *mut PopupState,
}

/// Tooltip window using an `xdg_popup`
pub struct TooltipWindow {
    wayland: Rc<Wayland>,
    display: *mut wl_display,
    compositor: *mut wl_compositor,
    shm: *mut wl_shm,
    xdg_wm_base: *mut xdg_wm_base,
    /// The parent window's `xdg_surface`, the popup's parent.
    parent_xdg_surface: *mut xdg_surface,
    viewporter: Option<*mut wp_viewporter>,

    /// The popup currently shown, if any.
    popup: Option<TooltipPopup>,

    // Shared memory buffer for rendering
    pool: Option<*mut wl_shm_pool>,
    buffer: Option<*mut wl_buffer>,
    data: Option<*mut u8>,
    mapped_size: usize, // Size of the mmap'd region for proper cleanup
    width: i32,
    height: i32,
    /// Row pitch in bytes of the mapped buffer (`shm::pool_layout`: padded to
    /// 256 bytes) - never `width * 4`.
    stride: i32,

    /// Font cache used to resolve + shape the tooltip text (Wayland has no
    /// native server-side text drawing, so glyphs are rasterized client-side).
    fc_cache: Arc<FcFontCache>,
    /// Whether the tooltip is currently mapped/visible.
    is_visible: bool,
}

impl TooltipWindow {
    /// Create the tooltip for the window whose `xdg_surface` is
    /// `parent_xdg_surface`; nothing is shown until [`Self::show`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wayland: Rc<Wayland>,
        display: *mut wl_display,
        compositor: *mut wl_compositor,
        shm: *mut wl_shm,
        xdg_wm_base: *mut xdg_wm_base,
        parent_xdg_surface: *mut xdg_surface,
        viewporter: Option<*mut wp_viewporter>,
        fc_cache: Arc<FcFontCache>,
    ) -> Result<Self, String> {
        if compositor.is_null() {
            return Err("Compositor not available".to_string());
        }
        if shm.is_null() {
            return Err("Shared memory not available".to_string());
        }
        if xdg_wm_base.is_null() || parent_xdg_surface.is_null() {
            return Err("xdg_wm_base or the parent's xdg_surface not available".to_string());
        }
        Ok(Self {
            wayland,
            display,
            compositor,
            shm,
            xdg_wm_base,
            parent_xdg_surface,
            viewporter,
            popup: None,
            pool: None,
            buffer: None,
            data: None,
            mapped_size: 0,
            width: 0,
            height: 0,
            stride: 0,
            fc_cache,
            is_visible: false,
        })
    }

    /// Show the tooltip with `text` for `anchor`, the box it explains on the
    /// parent surface (logical coordinates), at `dpi_factor`. The popup is
    /// recreated for every show: an `xdg_popup` is placed once, when it is
    /// created.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss
    )] // buffer pixels
    pub fn show(&mut self, text: &str, anchor: LogicalRect, dpi_factor: f32) -> Result<(), String> {
        let dpi = dpi_factor;
        self.destroy_popup();

        // The compositor keeps the popup on the screen; the text only has to
        // wrap at the tooltip width and stay within an output's height.
        let max_height = crate::desktop::display::desktop_screens()
            .iter()
            .map(|screen| screen.work_area.size.height)
            .fold(0.0_f32, f32::max);
        let max_height = if max_height > 0.0 {
            max_height
        } else {
            FALLBACK_MAX_TEXT_HEIGHT_PX
        };
        let max_text = LogicalSize::new(
            tooltip_text_width(f32::INFINITY, TOOLTIP_PADDING_PX),
            (max_height - TOOLTIP_PADDING_PX - TOOLTIP_PADDING_PX).max(1.0),
        );
        let pixmap = azul_layout::cpurender::render_wrapped_text_to_pixmap(
            &self.fc_cache,
            text,
            TOOLTIP_FONT_SIZE_PX,
            TOOLTIP_TEXT_COLOR,
            TOOLTIP_BG_COLOR,
            TOOLTIP_PADDING_PX,
            dpi,
            max_text,
        )
        .ok_or_else(|| "no font to draw the tooltip's text with".to_string())?;

        let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
        if self.width != width || self.height != height || self.buffer.is_none() {
            self.allocate_shm_buffer(width, height)?;
        }
        if let Some(data) = self.data {
            Self::blit_pixmap(data, self.mapped_size, self.stride, self.width, self.height, &pixmap);
        }
        let Some(buffer) = self.buffer else {
            return Err("no tooltip buffer".to_string());
        };
        let logical_size = (
            ((width as f32 / dpi).ceil() as i32).max(1),
            ((height as f32 / dpi).ceil() as i32).max(1),
        );

        self.popup = Some(self.create_popup(anchor, buffer, logical_size, (width, height))?);
        unsafe {
            (self.wayland.wl_display_flush)(self.display);
        }
        self.is_visible = true;
        Ok(())
    }

    /// Create the popup for a surface of `logical_size` showing `buffer`,
    /// placed by the tooltip rule against `anchor`. The buffer is attached
    /// when the compositor configures the popup.
    fn create_popup(
        &self,
        anchor: LogicalRect,
        buffer: *mut wl_buffer,
        logical_size: (i32, i32),
        device_size: (i32, i32),
    ) -> Result<TooltipPopup, String> {
        let wayland = &self.wayland;
        unsafe {
            let positioner = (wayland.xdg_wm_base_create_positioner)(self.xdg_wm_base);
            if positioner.is_null() {
                return Err("Failed to create the tooltip's xdg_positioner".to_string());
            }
            #[allow(clippy::cast_precision_loss)] // logical pixels
            let size = LogicalSize::new(logical_size.0 as f32, logical_size.1 as f32);
            super::configure_positioner(
                wayland,
                positioner,
                &FloatingPlacement::tooltip(anchor),
                size,
            );

            let surface = (wayland.wl_compositor_create_surface)(self.compositor);
            if surface.is_null() {
                (wayland.xdg_positioner_destroy)(positioner);
                return Err("Failed to create tooltip surface".to_string());
            }
            // The pointer passes through a tooltip: an empty input region.
            let region = (wayland.wl_compositor_create_region)(self.compositor);
            if !region.is_null() {
                (wayland.wl_surface_set_input_region)(surface, region);
                (wayland.wl_region_destroy)(region);
            }
            let viewport = self
                .viewporter
                .and_then(|vpr| super::wp_viewporter_get_viewport(wayland, vpr, surface));

            let xdg_surface = (wayland.xdg_wm_base_get_xdg_surface)(self.xdg_wm_base, surface);
            if xdg_surface.is_null() {
                if let Some(vp) = viewport {
                    super::wp_viewport_destroy(wayland, vp);
                }
                (wayland.wl_surface_destroy)(surface);
                (wayland.xdg_positioner_destroy)(positioner);
                return Err("Failed to create the tooltip's xdg_surface".to_string());
            }
            let xdg_popup =
                (wayland.xdg_surface_get_popup)(xdg_surface, self.parent_xdg_surface, positioner);
            (wayland.xdg_positioner_destroy)(positioner);
            if xdg_popup.is_null() {
                destroy_xdg_surface(wayland, xdg_surface);
                if let Some(vp) = viewport {
                    super::wp_viewport_destroy(wayland, vp);
                }
                (wayland.wl_surface_destroy)(surface);
                return Err("Failed to create the tooltip's xdg_popup".to_string());
            }

            let state = Box::into_raw(Box::new(PopupState {
                wayland: wayland.clone(),
                surface,
                buffer,
                viewport,
                logical_size,
                device_size,
                dismissed: Cell::new(false),
            }));
            // 'static: the proxies keep the listener pointers.
            static TOOLTIP_SURFACE_LISTENER: xdg_surface_listener = xdg_surface_listener {
                configure: tooltip_surface_configure,
            };
            static TOOLTIP_POPUP_LISTENER: xdg_popup_listener = xdg_popup_listener {
                configure: tooltip_popup_configure,
                popup_done: tooltip_popup_done,
            };
            (wayland.xdg_surface_add_listener)(
                xdg_surface,
                &TOOLTIP_SURFACE_LISTENER,
                state.cast::<c_void>(),
            );
            (wayland.xdg_popup_add_listener)(
                xdg_popup,
                &TOOLTIP_POPUP_LISTENER,
                state.cast::<c_void>(),
            );

            // No grab: a tooltip takes neither the pointer nor the keyboard.
            // The first commit carries no buffer; the compositor answers with
            // the configure that shows it.
            (wayland.wl_surface_commit)(surface);

            Ok(TooltipPopup {
                surface,
                xdg_surface,
                xdg_popup,
                viewport,
                state,
            })
        }
    }

    /// Unmap and destroy the shown popup, if any.
    fn destroy_popup(&mut self) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        let wayland = &self.wayland;
        unsafe {
            // Role object, then xdg_surface, then the viewport and the
            // wl_surface, as the protocol orders it.
            (wayland.xdg_popup_destroy)(popup.xdg_popup);
            destroy_xdg_surface(wayland, popup.xdg_surface);
            if let Some(vp) = popup.viewport {
                super::wp_viewport_destroy(wayland, vp);
            }
            (wayland.wl_surface_destroy)(popup.surface);
            // No listener can fire for the destroyed proxies any more.
            drop(Box::from_raw(popup.state));
        }
    }

    /// Hide the tooltip
    pub fn hide(&mut self) -> Result<(), String> {
        self.destroy_popup();
        unsafe {
            (self.wayland.wl_display_flush)(self.display);
        }
        self.is_visible = false;
        Ok(())
    }

    /// Whether the tooltip is currently visible.
    pub fn is_visible(&self) -> bool {
        let dismissed = self
            .popup
            .as_ref()
            .is_some_and(|popup| unsafe { (*popup.state).dismissed.get() });
        self.is_visible && !dismissed
    }

    /// Allocate a shared memory buffer for tooltip rendering, through the one
    /// shm allocator (`shm.rs`: sealed memfd, page-rounded size).
    fn allocate_shm_buffer(&mut self, width: i32, height: i32) -> Result<(), String> {
        self.cleanup_buffer();

        let layout = super::shm::pool_layout(width, height, 1, super::shm::page_size())
            .ok_or_else(|| "tooltip buffer too large".to_string())?;
        let stride = layout.stride; // ARGB8888, tight rows
        let size = layout.pool_bytes as i32;

        let fd = super::shm::create_shm_file("azul-tooltip", layout.pool_bytes)
            .map_err(|e| e.to_string())?;

        unsafe {
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
            self.stride = stride;
        }

        Ok(())
    }

    /// Copy a shaped, rasterized [`AzulPixmap`] (RGBA8) into the `wl_shm` buffer
    /// (ARGB8888 little-endian = BGRA byte order) at the buffer's own row pitch,
    /// through the one pitched upload. The pixmap already contains the tooltip
    /// background + shaped glyphs at device resolution.
    fn blit_pixmap(
        data: *mut u8,
        mapped_size: usize,
        stride: i32,
        width: i32,
        height: i32,
        pixmap: &azul_layout::cpurender::AzulPixmap,
    ) {
        // SAFETY: `data` is the live mapping of `mapped_size` bytes owned by
        // this tooltip (allocate_shm_buffer), not aliased while we write.
        let dst = unsafe { core::slice::from_raw_parts_mut(data, mapped_size) };
        let (sw, sh) = (pixmap.width(), pixmap.height());
        crate::desktop::shell2::headless::copy_rgba_rects_into(
            dst,
            stride.max(0) as usize,
            pixmap.data(),
            sw as usize * 4,
            width.max(0) as usize,
            height.max(0) as usize,
            &[(0, 0, sw, sh)],
            true,
        );
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
        // The popup goes before the buffer it shows.
        self.destroy_popup();
        self.cleanup_buffer();
    }
}

/// `xdg_surface.destroy` (opcode 0) + proxy teardown.
unsafe fn destroy_xdg_surface(wayland: &Wayland, xdg_surface: *mut xdg_surface) {
    if xdg_surface.is_null() {
        return;
    }
    type DestroyFn = unsafe extern "C" fn(*mut wl_proxy, u32);
    unsafe {
        let f: DestroyFn = std::mem::transmute(wayland.wl_proxy_marshal);
        f(xdg_surface.cast::<wl_proxy>(), 0);
        (wayland.wl_proxy_destroy)(xdg_surface.cast());
    }
}

/// The compositor configured (placed) the popup: acknowledge it and show the
/// buffer at its logical size.
extern "C" fn tooltip_surface_configure(
    data: *mut c_void,
    xdg_surface: *mut xdg_surface,
    serial: u32,
) {
    if data.is_null() {
        log_error!(LogCategory::Platform, "[tooltip] configure: null data pointer");
        return;
    }
    unsafe {
        let state = &*(data as *const PopupState);
        (state.wayland.xdg_surface_ack_configure)(xdg_surface, serial);
        (state.wayland.wl_surface_attach)(state.surface, state.buffer, 0, 0);
        // The buffer is at DEVICE resolution; the viewport maps it to the
        // logical size so scaled outputs (integer and fractional) do not show
        // it dpi times oversized. Without viewporter the 1:1 behaviour is kept.
        if let Some(vp) = state.viewport {
            super::wp_viewport_set_destination(
                &state.wayland,
                vp,
                state.logical_size.0,
                state.logical_size.1,
            );
        }
        (state.wayland.wl_surface_damage)(
            state.surface,
            0,
            0,
            state.device_size.0,
            state.device_size.1,
        );
        (state.wayland.wl_surface_commit)(state.surface);
    }
}

/// The compositor's placement of the popup; the buffer is attached in the
/// `xdg_surface` configure that follows it.
extern "C" fn tooltip_popup_configure(
    _data: *mut c_void,
    _xdg_popup: *mut xdg_popup,
    _x: i32,
    _y: i32,
    _width: i32,
    _height: i32,
) {
}

/// The compositor dismissed the popup.
extern "C" fn tooltip_popup_done(data: *mut c_void, _xdg_popup: *mut xdg_popup) {
    if data.is_null() {
        return;
    }
    unsafe {
        (*(data as *const PopupState)).dismissed.set(true);
    }
}
