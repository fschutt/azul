//! The screen eyedropper behind `CallbackInfo::pick_screen_color`.
//!
//! Two ways to pick a pixel, chosen per platform by what the OS allows:
//!
//! - **The system sampler** where one exists: macOS's `NSColorSampler`
//!   shows the familiar magnifier loupe and needs no screen-recording
//!   permission (`macos.rs`). The OS owns the UI; we only get the answer.
//!
//! - **A screenshot shown in a fullscreen loupe window** everywhere else
//!   ([`loupe_window`]). The screen is read ONCE - freely on X11 (`XGetImage`
//!   of the root) and Windows (`BitBlt` of the screen DC); on Wayland through
//!   the desktop portal's `Screenshot` call, which is where the user is asked
//!   for permission (the compositor shows its dialog; a refusal cancels the
//!   pick). The frozen frame fills a borderless fullscreen window that
//!   therefore receives every pointer move and the click - the one way to
//!   track the pointer on a display server that never reports it outside
//!   your own surfaces. A magnifier (`MAGNIFY`x, `LOUPE_CELLS`x`LOUPE_CELLS`
//!   source pixels) follows the pointer with the hex value; a click picks the
//!   pixel under it, Escape / right-click cancels.
//!
//! The answer travels back through `azul_layout::managers::eyedropper`
//! (routed by request id to the window that asked) and surfaces there as
//! `EventType::ScreenColorPicked`.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod x11;

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, Update},
    dom::{Dom, NodeData, NodeType},
    events::{EventFilter, HoverEventFilter, WindowEventFilter},
    geom::{LogicalPosition, LogicalSize, PhysicalPosition},
    refany::{OptionRefAny, RefAny},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat},
    window::{
        CursorPosition, VirtualKeyCode, WindowDecorations, WindowFrame, WindowPosition, WindowType,
    },
};
use azul_css::props::basic::color::ColorU;
use azul_layout::{
    callbacks::{Callback, CallbackInfo},
    managers::eyedropper::{push_result, EyedropperResult},
    window_state::{FullWindowState, WindowCreateOptions},
};

/// One frozen frame of the screen, as the loupe window shows it.
#[derive(Debug, Clone)]
pub struct Screenshot {
    /// Physical pixels.
    pub width: u32,
    pub height: u32,
    /// Straight (un-premultiplied) RGBA, row-major, `width * height * 4`.
    pub rgba: Vec<u8>,
    /// Where the captured area's top-left sits on screen, in logical px
    /// (the loupe window is placed there).
    pub origin: LogicalPosition,
    /// Physical pixels per logical pixel of the captured display.
    pub scale: f32,
}

impl Screenshot {
    /// The colour of the physical pixel at (`x`, `y`), or `None` off-image.
    #[must_use]
    pub fn pixel(&self, x: i64, y: i64) -> Option<ColorU> {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return None;
        }
        #[allow(clippy::cast_sign_loss)] // checked non-negative above
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = self.rgba.get(i..i + 4)?;
        Some(ColorU {
            r: p[0],
            g: p[1],
            b: p[2],
            a: 255,
        })
    }

    /// The logical size of the captured area.
    #[must_use]
    pub fn logical_size(&self) -> LogicalSize {
        #[allow(clippy::cast_precision_loss)] // pixel counts
        LogicalSize::new(
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        )
    }
}

/// Source pixels per side under the magnifier (odd, so there is a centre).
const LOUPE_CELLS: u32 = 15;
/// Screen pixels per source pixel in the magnifier.
const MAGNIFY: u32 = 8;
/// The magnifier's edge in logical px.
const LOUPE_PX: u32 = LOUPE_CELLS * MAGNIFY;
/// Gap between the pointer and the magnifier.
const LOUPE_GAP: f32 = 18.0;

/// The loupe window's state, in its layout-callback ctx.
struct LoupeData {
    request_id: u64,
    shot: Screenshot,
    /// The screenshot as an image, built once.
    frame: ImageRef,
    /// Last pointer position in the loupe window (logical).
    cursor: Option<LogicalPosition>,
    /// The answer was sent; ignore further input (a second click while the
    /// window closes must not answer twice).
    done: bool,
}

/// Build the fullscreen loupe window for a finished screenshot.
#[must_use]
pub fn loupe_window(shot: Screenshot, request_id: u64, dpi: u32) -> Option<WindowCreateOptions> {
    let size = shot.logical_size();
    let frame = ImageRef::new_rawimage(RawImage {
        width: shot.width as usize,
        height: shot.height as usize,
        pixels: RawImageData::U8(shot.rgba.clone().into()),
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })?;
    let data = RefAny::new(LoupeData {
        request_id,
        shot,
        frame,
        cursor: None,
        done: false,
    });

    let mut ws = FullWindowState::default();
    ws.title = "Pick a colour".into();
    ws.window_id = "azul-eyedropper".into();
    ws.flags.window_type = WindowType::Normal;
    ws.flags.decorations = WindowDecorations::None;
    ws.flags.frame = WindowFrame::Fullscreen;
    ws.flags.is_always_on_top = true;
    ws.flags.is_resizable = false;
    ws.flags.is_visible = true;
    ws.size.dimensions = size;
    ws.size.dpi = dpi;
    #[allow(clippy::cast_possible_truncation)] // whole pixels
    {
        ws.position = WindowPosition::Initialized(PhysicalPosition::new(
            data_origin(&data).x.round() as i32,
            data_origin(&data).y.round() as i32,
        ));
    }
    ws.layout_callback = LayoutCallback {
        cb: loupe_layout,
        ctx: OptionRefAny::Some(data),
    };
    Some(WindowCreateOptions {
        window_state: ws,
        size_to_content: false,
        renderer: None.into(),
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false,
        parent_window_id: 0,
    })
}

fn data_origin(data: &RefAny) -> LogicalPosition {
    let mut d = data.clone();
    d.downcast_ref::<LoupeData>()
        .map_or(LogicalPosition::zero(), |d| d.shot.origin)
}

/// Answer a pick (or a cancellation) for `request_id`.
pub fn finish(request_id: u64, color: Option<ColorU>) {
    push_result(EyedropperResult { request_id, color });
}

/// The magnifier's content: `LOUPE_CELLS`² source pixels around the pointer,
/// each blown up to `MAGNIFY`² with a one-pixel grid, the centre cell
/// outlined. Nearest-neighbour on purpose - a loupe shows PIXELS.
fn magnifier_image(shot: &Screenshot, cursor: LogicalPosition) -> Option<ImageRef> {
    let half = i64::from(LOUPE_CELLS / 2);
    #[allow(clippy::cast_possible_truncation)] // pixel coordinates
    let (cx, cy) = (
        (cursor.x * shot.scale).floor() as i64,
        (cursor.y * shot.scale).floor() as i64,
    );
    let side = LOUPE_PX as usize;
    let mut px = vec![0u8; side * side * 4];
    for cell_y in 0..LOUPE_CELLS {
        for cell_x in 0..LOUPE_CELLS {
            let sx = cx - half + i64::from(cell_x);
            let sy = cy - half + i64::from(cell_y);
            let c = shot.pixel(sx, sy).unwrap_or(ColorU {
                r: 32,
                g: 32,
                b: 32,
                a: 255,
            });
            let is_centre = cell_x == LOUPE_CELLS / 2 && cell_y == LOUPE_CELLS / 2;
            for dy in 0..MAGNIFY {
                for dx in 0..MAGNIFY {
                    let x = (cell_x * MAGNIFY + dx) as usize;
                    let y = (cell_y * MAGNIFY + dy) as usize;
                    let on_grid = dx == 0 || dy == 0;
                    let on_ring =
                        is_centre && (dx == 0 || dy == 0 || dx == MAGNIFY - 1 || dy == MAGNIFY - 1);
                    let (r, g, b) = if on_ring {
                        // White ring with a dark inner edge reads on any colour.
                        (255, 255, 255)
                    } else if on_grid {
                        // A faint grid: the cell's colour darkened a little.
                        (
                            c.r.saturating_sub(c.r / 6),
                            c.g.saturating_sub(c.g / 6),
                            c.b.saturating_sub(c.b / 6),
                        )
                    } else {
                        (c.r, c.g, c.b)
                    };
                    let i = (y * side + x) * 4;
                    px[i] = r;
                    px[i + 1] = g;
                    px[i + 2] = b;
                    px[i + 3] = 255;
                }
            }
        }
    }
    ImageRef::new_rawimage(RawImage {
        width: side,
        height: side,
        pixels: RawImageData::U8(px.into()),
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
}

fn hex(c: ColorU) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Where the magnifier box goes for a pointer at `cursor` in a window of
/// `size`: below-right of the pointer, flipped to the other side near edges.
fn magnifier_origin(cursor: LogicalPosition, size: LogicalSize) -> LogicalPosition {
    #[allow(clippy::cast_precision_loss)] // small constants
    let box_w = LOUPE_PX as f32 + 2.0;
    #[allow(clippy::cast_precision_loss)]
    let box_h = LOUPE_PX as f32 + 2.0 + 24.0; // + the hex strip
    let mut x = cursor.x + LOUPE_GAP;
    let mut y = cursor.y + LOUPE_GAP;
    if x + box_w > size.width {
        x = cursor.x - LOUPE_GAP - box_w;
    }
    if y + box_h > size.height {
        y = cursor.y - LOUPE_GAP - box_h;
    }
    LogicalPosition::new(x.max(0.0), y.max(0.0))
}

extern "C" fn loupe_layout(_app: RefAny, info: LayoutCallbackInfo) -> Dom {
    let OptionRefAny::Some(ctx) = info.get_ctx() else {
        return Dom::create_body();
    };
    let mut probe = ctx.clone();
    let Some(d) = probe.downcast_ref::<LoupeData>() else {
        return Dom::create_body();
    };
    let size = d.shot.logical_size();

    let mk = |event: EventFilter, cb: extern "C" fn(RefAny, CallbackInfo) -> Update| {
        (event, ctx.clone(), Callback::from_ptr(cb).to_core())
    };

    // The frozen frame, filling the window. Every pointer move over it
    // re-lays the magnifier out (a tiny DOM; RefreshDom is cheap here).
    let mut frame = NodeData::create_node(NodeType::Image(azul_css::css::BoxOrStatic::heap(
        d.frame.clone(),
    )));
    frame.set_css("position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;");
    let frame = Dom::create_from_data(frame);

    let mut body = Dom::create_body().with_css(&format!(
        "position: relative; width: {}px; height: {}px; margin: 0px; padding: 0px; \
         cursor: crosshair; overflow: hidden; background: #000000;",
        size.width, size.height
    ));
    for (event, data, cb) in [
        mk(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            on_loupe_move,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            on_loupe_pick,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::RightMouseUp),
            on_loupe_cancel,
        ),
        mk(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            on_loupe_key,
        ),
        mk(
            EventFilter::Window(WindowEventFilter::WindowFocusLost),
            on_loupe_cancel,
        ),
    ] {
        body.root.add_callback(event, data, cb);
    }
    body = body.with_child(frame);

    if let Some(cursor) = d.cursor {
        if let Some(zoomed) = magnifier_image(&d.shot, cursor) {
            let at = magnifier_origin(cursor, size);
            #[allow(clippy::cast_possible_truncation)]
            let (px, py) = (
                (cursor.x * d.shot.scale).floor() as i64,
                (cursor.y * d.shot.scale).floor() as i64,
            );
            let colour = d.shot.pixel(px, py).unwrap_or(ColorU::BLACK);
            let mut img =
                NodeData::create_node(NodeType::Image(azul_css::css::BoxOrStatic::heap(zoomed)));
            img.set_css(&format!(
                "width: {LOUPE_PX}px; height: {LOUPE_PX}px; display: block;"
            ));
            let label = Dom::create_div()
                .with_css(&format!(
                    "display: flex; flex-direction: row; align-items: center; gap: 6px; height: 24px; \
                     padding: 0px 6px; background: #202020; color: #ffffff; font-size: 12px; \
                     font-family: monospace;"
                ))
                .with_child(Dom::create_div().with_css(&format!(
                    "width: 12px; height: 12px; border: 1px solid #ffffff; background: {};",
                    hex(colour)
                )))
                .with_child(Dom::create_span_with_text(hex(colour)));
            let magnifier = Dom::create_div()
                .with_css(&format!(
                    "position: absolute; left: {}px; top: {}px; width: {LOUPE_PX}px; \
                     border: 1px solid #ffffff; box-shadow: 0px 2px 12px rgba(0, 0, 0, 0.6); \
                     background: #202020;",
                    at.x, at.y
                ))
                .with_child(Dom::create_from_data(img))
                .with_child(label);
            body = body.with_child(magnifier);
        }
    }
    body
}

fn with_loupe(data: &mut RefAny, f: impl FnOnce(&mut LoupeData)) {
    if let Some(mut d) = data.downcast_mut::<LoupeData>() {
        f(&mut d);
    }
}

extern "C" fn on_loupe_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let CursorPosition::InWindow(pos) = info.get_current_window_state().mouse_state.cursor_position
    else {
        return Update::DoNothing;
    };
    let mut changed = false;
    with_loupe(&mut data, |d| {
        if !d.done && d.cursor != Some(pos) {
            d.cursor = Some(pos);
            changed = true;
        }
    });
    if changed {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

fn close_loupe(info: &mut CallbackInfo) {
    let mut state = info.get_current_window_state().clone();
    state.flags.close_requested = true;
    info.modify_window_state(state);
}

extern "C" fn on_loupe_pick(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let CursorPosition::InWindow(pos) = info.get_current_window_state().mouse_state.cursor_position
    else {
        return Update::DoNothing;
    };
    let mut answered = false;
    with_loupe(&mut data, |d| {
        if d.done {
            return;
        }
        d.done = true;
        #[allow(clippy::cast_possible_truncation)]
        let (px, py) = (
            (pos.x * d.shot.scale).floor() as i64,
            (pos.y * d.shot.scale).floor() as i64,
        );
        finish(d.request_id, d.shot.pixel(px, py));
        answered = true;
    });
    if answered {
        close_loupe(&mut info);
        // The window that asked is another window: wake it to read the answer.
        Update::RefreshDomAllWindows
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_loupe_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut answered = false;
    with_loupe(&mut data, |d| {
        if !d.done {
            d.done = true;
            finish(d.request_id, None);
            answered = true;
        }
    });
    if answered {
        close_loupe(&mut info);
        Update::RefreshDomAllWindows
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_loupe_key(data: RefAny, info: CallbackInfo) -> Update {
    let escape = info
        .get_current_window_state()
        .keyboard_state
        .pressed_virtual_keycodes
        .as_ref()
        .contains(&VirtualKeyCode::Escape);
    if escape {
        on_loupe_cancel(data, info)
    } else {
        Update::DoNothing
    }
}

/// A decoded image (the portal's PNG) as straight, opaque RGBA. `None` for
/// pixel formats a screenshot never has (16-bit, float).
#[must_use]
pub fn raw_image_to_rgba(img: &RawImage) -> Option<Vec<u8>> {
    let RawImageData::U8(px) = &img.pixels else {
        return None;
    };
    let px = px.as_ref();
    let n = img.width * img.height;
    let mut out = Vec::with_capacity(n * 4);
    match img.data_format {
        RawImageFormat::RGBA8 => {
            for p in px.chunks_exact(4).take(n) {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        RawImageFormat::RGB8 => {
            for p in px.chunks_exact(3).take(n) {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        RawImageFormat::BGRA8 => out = bgra_to_rgba(&px[..n * 4]),
        RawImageFormat::BGR8 => {
            for p in px.chunks_exact(3).take(n) {
                out.extend_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        RawImageFormat::R8 => {
            for p in px.iter().take(n) {
                out.extend_from_slice(&[*p, *p, *p, 255]);
            }
        }
        RawImageFormat::RG8 => {
            for p in px.chunks_exact(2).take(n) {
                out.extend_from_slice(&[p[0], p[0], p[0], 255]);
            }
        }
        _ => return None,
    }
    (out.len() == n * 4).then_some(out)
}

/// BGRX / BGRA (little-endian 0x00RRGGBB words, what X11 and GDI hand out)
/// to straight RGBA with alpha forced opaque.
#[must_use]
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bgra.len());
    for p in bgra.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot() -> Screenshot {
        // 4x3, scale 2: a red pixel at (3, 2).
        let mut rgba = vec![0u8; 4 * 3 * 4];
        let i = (2 * 4 + 3) * 4;
        rgba[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        Screenshot {
            width: 4,
            height: 3,
            rgba,
            origin: LogicalPosition::new(10.0, 20.0),
            scale: 2.0,
        }
    }

    #[test]
    fn pixel_reads_are_bounds_checked_and_opaque() {
        let s = shot();
        assert_eq!(
            s.pixel(3, 2),
            Some(ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            })
        );
        assert_eq!(
            s.pixel(0, 0),
            Some(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            })
        );
        assert_eq!(s.pixel(-1, 0), None);
        assert_eq!(s.pixel(4, 0), None);
        assert_eq!(s.pixel(0, 3), None);
        assert_eq!(s.logical_size(), LogicalSize::new(2.0, 1.5));
    }

    #[test]
    fn the_magnifier_is_the_right_size_and_shows_the_centre_pixel() {
        let s = shot();
        // Logical (1.5, 1.0) -> physical (3, 2): the red pixel is the centre cell.
        let img = magnifier_image(&s, LogicalPosition::new(1.5, 1.0)).expect("image");
        let raw = img.get_rawimage().expect("raw");
        assert_eq!(
            (raw.width, raw.height),
            (LOUPE_PX as usize, LOUPE_PX as usize)
        );
        let RawImageData::U8(px) = &raw.pixels else {
            panic!("u8")
        };
        // Centre cell interior (off its ring and grid lines): the source red.
        // `ImageRef::new_rawimage` ENCODES to BGRA8 (its documented storage
        // format — that is what the GPU wants), so the RGBA red source
        // `[255,0,0,255]` reads back through `get_rawimage` as BGRA
        // `[0,0,255,255]`. The loupe still DISPLAYS red; only the raw byte order
        // is B,G,R,A here.
        let cell = (LOUPE_CELLS / 2) * MAGNIFY;
        let (x, y) = ((cell + MAGNIFY / 2) as usize, (cell + MAGNIFY / 2) as usize);
        let i = (y * LOUPE_PX as usize + x) * 4;
        assert_eq!(&px.as_ref()[i..i + 4], &[0, 0, 255, 255]);
        // Its ring: white (symmetric under the RGBA↔BGRA channel swap).
        let i = (cell as usize * LOUPE_PX as usize + cell as usize) * 4;
        assert_eq!(&px.as_ref()[i..i + 3], &[255, 255, 255]);
    }

    #[test]
    fn the_magnifier_flips_away_from_the_edges() {
        let size = LogicalSize::new(800.0, 600.0);
        let near_origin = magnifier_origin(LogicalPosition::new(10.0, 10.0), size);
        assert!(
            near_origin.x > 10.0 && near_origin.y > 10.0,
            "below-right by default"
        );
        let near_corner = magnifier_origin(LogicalPosition::new(790.0, 590.0), size);
        assert!(
            near_corner.x < 790.0 && near_corner.y < 590.0,
            "flipped above-left"
        );
        assert!(near_corner.x >= 0.0 && near_corner.y >= 0.0);
    }

    #[test]
    fn bgra_becomes_opaque_rgba() {
        assert_eq!(
            bgra_to_rgba(&[1, 2, 3, 0, 4, 5, 6, 7]),
            vec![3, 2, 1, 255, 6, 5, 4, 255]
        );
    }

    #[test]
    fn a_loupe_window_is_fullscreen_at_the_capture_origin() {
        let opts = loupe_window(shot(), 7, 96).expect("window");
        assert_eq!(opts.window_state.flags.frame, WindowFrame::Fullscreen);
        assert_eq!(opts.window_state.flags.decorations, WindowDecorations::None);
        assert_eq!(
            opts.window_state.size.dimensions,
            LogicalSize::new(2.0, 1.5)
        );
        assert_eq!(
            opts.window_state.position,
            WindowPosition::Initialized(PhysicalPosition::new(10, 20))
        );
    }
}
