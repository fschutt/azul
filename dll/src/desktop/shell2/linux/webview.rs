//! `<webview>` on Linux (X11 and Wayland): WPE `WebKit`, composited - every
//! library loaded with `dlopen` at the first web view (an app without one
//! loads nothing, a machine without WPE `WebKit` runs the app all the same
//! and its web views say what to install).
//!
//! # Why WPE `WebKit`, not `WebKitGTK`
//!
//! An azul window is an X11 window or a Wayland surface of its own, not a
//! GTK one. `WebKitGTK`'s view is a `GtkWidget`: it needs a GTK window tree
//! and the GTK main loop around it. Putting one into an azul window means
//! XEmbed (`GtkPlug`/`GtkSocket` - X11 only, and gone in GTK 4) or a GTK
//! toplevel kept over the view's rectangle (Wayland lets no client position
//! a toplevel, and on X11 it fights the window manager and cannot be clipped
//! by a scroll container or stacked under an azul popup).
//!
//! WPE `WebKit` is the same engine without a toolkit: it renders into
//! buffers handed to the embedder (`WPEBackend-fdo`'s exportable view
//! backend, shared memory here) and takes input as plain events. The window
//! draws each frame as an image at the view's content box
//! (`LayoutWindow::set_webview_frame`), so clipping, scrolling, stacking and
//! transforms are the engine's own, and X11 and Wayland are one code path;
//! the window routes the page's pointer and keyboard to it
//! (`WebViewOp::Input`).
//!
//! # How
//!
//! [`platform`] makes a Linux window composite its web views. Its backend
//! ([`WpeWebViews`]) hands the engine's ops to the `azul-webview` thread
//! (`super::wpe_thread`), which owns every `WebKit` object and runs GLib's
//! main context, and takes back its reports and frames. The libraries and
//! their functions are `super::wpe`.
//!
//! Not here (yet): the GPU path's EGL export (frames are copied from shared
//! memory), an input method inside the page (keys go in as keysyms), and a
//! page's own cursor shape.

use azul_core::{
    events::{KeyModifiers, MouseButton},
    geom::LogicalPosition,
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat},
    webview::WebViewConfig,
};
use azul_css::AzString;
use azul_layout::managers::webview::{
    WebViewId, WebViewInput, WebViewOp, WebViewPlacement, WebViewPlatform, WebViewReport,
    WebViewTransform,
};

pub use super::wpe::WPE_WEBKIT;
use super::{
    wpe,
    wpe_thread::{self, Command, Mail, Mailbox},
};
use crate::desktop::shell2::common::{webview::WebViewBackend, DlError};

/// `wl_shm` format `ARGB8888`: B, G, R, A in memory, premultiplied.
pub const SHM_FORMAT_ARGB8888: u32 = 0;
/// `wl_shm` format `XRGB8888`: B, G, R and an undefined fourth byte.
pub const SHM_FORMAT_XRGB8888: u32 = 1;

/// What a Linux window (X11 or Wayland) does with a `<webview>`: composite
/// it - WPE `WebKit` is loaded at the first one.
#[must_use]
pub const fn platform() -> WebViewPlatform {
    WebViewPlatform::Composited
}

/// What a web view shows (and `WebViewLoadFailed` says) when WPE `WebKit`
/// did not load: one line, the libraries or the function that were missing
/// and what to install - not the loader's own error text.
#[must_use]
pub fn unavailable_reason(error: &DlError) -> String {
    match error {
        DlError::LibraryNotFound { tried, .. } => format!(
            "This web view needs WPE WebKit, which did not load (tried {}). Install it: \
             libwpewebkit-2.0-1 and libwpebackend-fdo-1.0-1 on Debian and Ubuntu, wpewebkit \
             and wpebackend-fdo on Fedora and Arch.",
            tried.join(", ")
        ),
        DlError::SymbolNotFound {
            symbol, library, ..
        } => format!(
            "This web view needs WPE WebKit, but {library} has no {symbol}: it is older than \
             azul supports (WPE WebKit 2.22 or newer)."
        ),
        other => format!(
            "This web view needs WPE WebKit, which did not load: {}",
            other.to_string().replace('\n', " ")
        ),
    }
}

/// A shared-memory frame of `width` x `height` pixels, rows `stride` bytes
/// apart, as tightly packed BGRA (premultiplied, as `WebKit` draws it):
/// `ARGB8888` is that already, `XRGB8888` gets an opaque alpha. `None` for
/// another format or a buffer shorter than its rows.
#[must_use]
pub fn bgra_from_shm(
    width: usize,
    height: usize,
    stride: usize,
    format: u32,
    data: &[u8],
) -> Option<Vec<u8>> {
    let row = width.checked_mul(4)?;
    let opaque = match format {
        SHM_FORMAT_ARGB8888 => false,
        SHM_FORMAT_XRGB8888 => true,
        _ => return None,
    };
    if stride < row || data.len() < stride.checked_mul(height.checked_sub(1)?)? + row {
        return None;
    }
    let mut out = Vec::with_capacity(row * height);
    for y in 0..height {
        out.extend_from_slice(&data[y * stride..y * stride + row]);
    }
    if opaque {
        for pixel in out.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }
    Some(out)
}

/// A page point (CSS px) in the device pixels WPE's input events carry.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn device_point(at: LogicalPosition, scale: f32) -> (i32, i32) {
    ((at.x * scale).floor() as i32, (at.y * scale).floor() as i32)
}

/// WPE's number for a button: 1 left, 2 right, 3 middle (the evdev order).
#[must_use]
pub const fn wpe_button(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 3,
        MouseButton::Other(n) => n as u32,
    }
}

/// WPE's modifier mask (`enum wpe_input_modifier`): the keys held and the
/// buttons held (button N is bit 19 + N).
#[must_use]
pub fn wpe_modifiers(keys: KeyModifiers, buttons: &[MouseButton]) -> u32 {
    let mut mask = 0;
    if keys.ctrl {
        mask |= 1 << 0;
    }
    if keys.shift {
        mask |= 1 << 1;
    }
    if keys.alt {
        mask |= 1 << 2;
    }
    if keys.meta {
        mask |= 1 << 3;
    }
    for button in buttons {
        let n = wpe_button(*button);
        if (1..=5).contains(&n) {
            mask |= 1 << (19 + n);
        }
    }
    mask
}

/// A wheel `delta` (logical px, positive `y` scrolls the content up - the
/// page down) as WPE's smooth 2D axes: device pixels, positive towards the
/// page's top-left.
#[must_use]
pub fn wheel_axes(delta: LogicalPosition, scale: f32) -> (f64, f64) {
    (-f64::from(delta.x * scale), -f64::from(delta.y * scale))
}

/// Whether a `load-failed` error is a navigation somebody cancelled - the
/// app (`prevent_default`), a policy answer, a new load over an old one -
/// rather than a failure: `WEBKIT_NETWORK_ERROR_CANCELLED` and
/// `WEBKIT_POLICY_ERROR_FRAME_LOAD_INTERRUPTED_BY_POLICY_CHANGE`.
#[must_use]
pub fn is_quiet_failure(domain: &str, code: i32) -> bool {
    matches!((domain, code), ("WebKitNetworkError", 302) | ("WebKitPolicyError", 102))
}

/// The web views of one Linux window: the ops go to the `azul-webview`
/// thread, its reports and frames come back through the window's mail.
pub struct WpeWebViews {
    mailbox: Mailbox,
    scale: f32,
    /// The thread knows this window's scale.
    scale_sent: bool,
}

impl WpeWebViews {
    /// The backend of a window drawn at `scale` device pixels per logical
    /// one. Loads nothing: WPE `WebKit` is loaded by the first `create`.
    #[must_use]
    pub fn new(scale: f32) -> Self {
        Self {
            mailbox: Mailbox::default(),
            scale,
            scale_sent: false,
        }
    }

    /// The window's scale changed (it moved to another monitor).
    pub fn set_scale(&mut self, scale: f32) {
        if (scale - self.scale).abs() > f32::EPSILON {
            self.scale = scale;
            if self.scale_sent {
                wpe_thread::send(&self.mailbox, Command::Scale(scale));
            }
        }
    }

    fn op(&self, op: WebViewOp) {
        wpe_thread::send(&self.mailbox, Command::Op(op));
    }

    fn take<T>(&self, part: impl FnOnce(&mut Mail) -> T) -> Option<T> {
        self.mailbox.lock().ok().map(|mut mail| part(&mut mail))
    }
}

impl WebViewBackend for WpeWebViews {
    fn create(&mut self, id: WebViewId, config: WebViewConfig, src: &str) -> Result<(), String> {
        wpe::wpe().map_err(|e| unavailable_reason(&e))?;
        wpe_thread::start()?;
        if !self.scale_sent {
            self.scale_sent = true;
            wpe_thread::send(&self.mailbox, Command::Scale(self.scale));
        }
        self.op(WebViewOp::Create {
            id,
            config,
            src: AzString::from(src),
        });
        Ok(())
    }
    fn place(&mut self, id: WebViewId, placement: &WebViewPlacement) {
        self.op(WebViewOp::Place {
            id,
            placement: *placement,
        });
    }
    fn transform(&mut self, id: WebViewId, transform: &WebViewTransform) {
        self.op(WebViewOp::Transform {
            id,
            transform: *transform,
        });
    }
    fn input(&mut self, id: WebViewId, input: &WebViewInput) {
        self.op(WebViewOp::Input {
            id,
            input: input.clone(),
        });
    }
    fn navigate(&mut self, id: WebViewId, url: &str) {
        self.op(WebViewOp::Navigate {
            id,
            url: AzString::from(url),
        });
    }
    fn reload(&mut self, id: WebViewId) {
        self.op(WebViewOp::Reload { id });
    }
    fn go_back(&mut self, id: WebViewId) {
        self.op(WebViewOp::GoBack { id });
    }
    fn decide_navigation(&mut self, id: WebViewId, request: u64, allow: bool) {
        wpe_thread::send(&self.mailbox, Command::Decide { id, request, allow });
    }
    fn destroy(&mut self, id: WebViewId) {
        self.op(WebViewOp::Destroy { id });
    }
    fn poll_reports(&mut self) -> Vec<WebViewReport> {
        self.take(|mail| core::mem::take(&mut mail.reports)).unwrap_or_default()
    }
    fn poll_frames(&mut self) -> Vec<(WebViewId, ImageRef)> {
        let frames = self
            .take(|mail| core::mem::take(&mut mail.frames))
            .unwrap_or_default();
        frames
            .into_iter()
            .filter_map(|(id, frame)| {
                ImageRef::new_rawimage(RawImage {
                    pixels: RawImageData::U8(frame.pixels.into()),
                    width: frame.width,
                    height: frame.height,
                    premultiplied_alpha: true,
                    data_format: RawImageFormat::BGRA8,
                    tag: Vec::new().into(),
                })
                .map(|image| (id, image))
            })
            .collect()
    }
}

impl Drop for WpeWebViews {
    fn drop(&mut self) {
        // The window's views go with it, on the thread that owns them.
        wpe_thread::send(&self.mailbox, Command::Forget);
    }
}
