//! A window that turns from portrait to landscape - wider AND shorter, the "mixed" resize - shows the same picture whether the
//! CPU backend repaints what changed or repaints everything.
//!
//! The Android emulator run 45797870060 (AzWidgets at 2.625 x, 1080 x 2400 px to 2272 x 1080): after the rotation the page's
//! white ended at the width the window had in portrait and the canvas colour lay beyond it, while a render of the same display
//! list from scratch was right and the shell's three presents each wrote every row of the frame the backend held. The e2e
//! harness cannot see this: it clears its canvas to white (`follow_system_background: false`), like the page of most of its
//! scenarios. The headless shell here opens a window as a desktop shell does (the system's window background is the canvas),
//! which is the clear colour of every real window.
//!
//! * `the_canvas_colour_is_not_the_page_colour` - the premise: a stale strip shows as the canvas colour, not as the page's.
//! * `a_rotation_to_landscape_is_painted_like_a_full_repaint` - the backend's own repaint of a mixed resize.
//! * `a_full_repaint_after_the_rotation_is_the_page` - what the Android shell asks for since `apply_surface_size` (A-36): the
//!   backend's `force_full_repaint` before the frame of the new size.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{
    common::{app_shared::AppShared, event::SharedUndoManager, PlatformWindow},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, LayoutCallbackType},
    dom::Dom,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

/// 252 dpi is the engine's dpi of a 420 dpi phone (`dpi_of_density(420)`): a frame factor of 2.625, so 400 x 800 logical px is
/// 1050 x 2100 device px and 800 x 400 is 2100 x 1050.
const PHONE_DPI: u32 = 252;
const PORTRAIT: (f32, f32) = (400.0, 800.0);
const LANDSCAPE: (f32, f32) = (800.0, 400.0);

/// AzWidgets' skeleton: a body in the page colour that fills the window (`height: 100%`, a column), a bar, and a scrolling pane
/// with padding, a card and a field. Boxes only: no text, so no font is needed.
extern "C" fn page(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
        .with_css(
            "margin: 0; background-color: system:background; display: flex; flex-direction: column; height: 100%;",
        )
        .with_child(Dom::create_div().with_css(
            "height: 36px; flex-shrink: 0; background-color: system:window-background;",
        ))
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; min-height: 0; padding: 24px;",
                )
                .with_child(Dom::create_div().with_css(
                    "height: 60px; margin-bottom: 12px; background-color: system:window-background; border-radius: 10px;",
                ))
                .with_child(Dom::create_div().with_css(
                    "height: 40px; background-color: #ffffff; border: 1px solid #888888;",
                )),
        )
}

fn light_desktop() -> azul_css::system::SystemStyle {
    azul_css::system::defaults::macos_modern_light()
}

/// A window as a DESKTOP shell opens it (`mode_headless.rs`): the canvas seeded from the mode at creation, and the CPU canvas
/// following the system background, as every real window's backend does.
fn open(size: (f32, f32)) -> HeadlessWindow {
    let mut config = AppConfig::default();
    config.system_style = light_desktop();
    let app = Arc::new(AppShared::new(&config));
    let mut options = WindowCreateOptions::default();
    let layout: LayoutCallbackType = page;
    options.window_state.layout_callback = LayoutCallback {
        cb: layout,
        ctx: OptionRefAny::None,
    };
    options.window_state.size.dimensions = azul_core::geom::LogicalSize {
        width: size.0,
        height: size.1,
    };
    options.window_state.size.dpi = PHONE_DPI;
    azul::desktop::shell2::common::resolve_initial_background_color(&mut options, &light_desktop(), app.mode());
    let mut window = HeadlessWindow::with_app_shared(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        SharedUndoManager::new(),
        Arc::clone(&app),
        config,
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed");
    window.cpu_backend.follow_system_background = true;
    window
}

/// A frame as the shell holds it after a `render_frame`: its size, its RGBA bytes and the rects the backend says it painted.
struct Frame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    painted: String,
}

impl Frame {
    fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.width + x) * 4) as usize;
        [self.rgba[at], self.rgba[at + 1], self.rgba[at + 2], self.rgba[at + 3]]
    }
}

/// One frame of the window's laid out page on the CPU backend, as the shells call it.
fn paint(window: &mut HeadlessWindow) -> Frame {
    let (width, height, factor) = {
        let ws = window.common.current_window_state();
        (ws.size.dimensions.width, ws.size.dimensions.height, ws.size.dpi as f32 / 96.0)
    };
    let lw = window.common.layout_window.as_ref().expect("a layout window");
    let painted = window
        .cpu_backend
        .render_frame(lw, &window.common.renderer_resources, width, height, factor);
    let frame = window.cpu_backend.last_frame.as_ref().expect("a painted frame");
    Frame {
        width: frame.width(),
        height: frame.height(),
        rgba: frame.data().to_vec(),
        painted: format!("{painted:?}"),
    }
}

fn clear_colour(window: &HeadlessWindow) -> [u8; 4] {
    window.cpu_backend.last_clear_color.expect("a painted frame records its clear colour")
}

/// The page's own colour: a pixel of the body's padding, right of everything else, in a frame that was painted whole.
fn page_colour(frame: &Frame) -> [u8; 4] {
    frame.pixel(frame.width - 4, frame.height / 2)
}

/// The first pixel where two frames of one size differ, as `(x, y, a, b)`, and how many differ.
fn first_difference(a: &Frame, b: &Frame) -> Option<(u32, u32, [u8; 4], [u8; 4], usize)> {
    assert_eq!((a.width, a.height), (b.width, b.height), "frames of one size");
    let mut first = None;
    let mut count = 0usize;
    for y in 0..a.height {
        for x in 0..a.width {
            let (pa, pb) = (a.pixel(x, y), b.pixel(x, y));
            if pa != pb {
                count += 1;
                first.get_or_insert((x, y, pa, pb));
            }
        }
    }
    first.map(|(x, y, pa, pb)| (x, y, pa, pb, count))
}

/// The portrait window, laid out and painted once.
fn portrait_painted() -> (HeadlessWindow, Frame) {
    let mut window = open(PORTRAIT);
    window.regenerate_layout().expect("first layout");
    let frame = paint(&mut window);
    assert_eq!((frame.width, frame.height), (1050, 2100), "premise: 400 x 800 logical px at 2.625 x");
    (window, frame)
}

/// The rotation itself, as the Android shell does it (`apply_surface_size`, then a `Resize` regeneration).
fn rotate(window: &mut HeadlessWindow) {
    window.simulate_resize(LANDSCAPE.0, LANDSCAPE.1);
    window.regenerate_layout().expect("layout after the rotation");
}

#[test]
fn the_canvas_colour_is_not_the_page_colour() {
    let (window, portrait) = portrait_painted();
    let (clear, page) = (clear_colour(&window), page_colour(&portrait));
    assert_ne!(
        clear, page,
        "the premise of the other tests: a strip nobody painted shows the canvas colour {clear:?}, the page's is {page:?}"
    );
}

/// The backend's own repaint of the mixed resize: the strip the rotation exposed is the page.
///
/// Mutation: none to make; this is the observation of the emulator run (the strip right of the old width was the canvas
/// colour).
#[test]
fn a_rotation_to_landscape_is_painted_like_a_full_repaint() {
    let (mut window, portrait) = portrait_painted();
    let page = page_colour(&portrait);
    rotate(&mut window);
    let incremental = paint(&mut window);
    let how = format!("{:?}; it painted {}", window.cpu_backend.last_frame_damage, incremental.painted);
    assert_eq!((incremental.width, incremental.height), (2100, 1050));

    // What the page looks like where the window did not exist before: right of the old width, next to the right edge.
    let strip = incremental.pixel(incremental.width - 4, incremental.height / 2);
    assert_eq!(
        strip, page,
        "the strip a rotation exposes shows the page's colour {page:?}, not the canvas colour {:?} (the frame was repainted as: {how})",
        clear_colour(&window)
    );

    window.cpu_backend.force_full_repaint = true;
    let full = paint(&mut window);
    if let Some((x, y, a, b, count)) = first_difference(&incremental, &full) {
        panic!(
            "the repaint of what changed ({how}) and a repaint of everything differ in {count} px, the first at ({x}, {y}): {a:?} against {b:?}"
        );
    }
}

/// The frame the Android shell asks for after a change of the surface's size: a full repaint, which is the page and equals the
/// frame of a window that never was portrait.
#[test]
fn a_full_repaint_after_the_rotation_is_the_page() {
    let (mut window, portrait) = portrait_painted();
    let page = page_colour(&portrait);
    window.cpu_backend.force_full_repaint = true;
    rotate(&mut window);
    let after = paint(&mut window);
    assert_eq!(
        format!("{:?}", window.cpu_backend.last_frame_damage),
        "Full",
        "force_full_repaint paints the whole frame"
    );
    assert_eq!(after.pixel(after.width - 4, after.height / 2), page, "the strip is the page");

    let mut landscape_from_the_start = open(LANDSCAPE);
    landscape_from_the_start.regenerate_layout().expect("layout");
    let reference = paint(&mut landscape_from_the_start);
    if let Some((x, y, a, b, count)) = first_difference(&after, &reference) {
        panic!(
            "the frame after the rotation and the frame of a window that opened in landscape differ in {count} px, the first at ({x}, {y}): {a:?} against {b:?}"
        );
    }
}
