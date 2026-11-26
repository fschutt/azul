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
//! * `the_display_list_paints_the_page_under_the_whole_window_after_a_rotation` - the engine's screenshot clears to WHITE
//!   (`CallbackInfo::render_screenshot`), so a page rect that stopped at the old width is invisible in it: the list itself is read.
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
    dom::{Dom, DomId},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
};
use azul_layout::{solver3::display_list::DisplayListItem, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

/// 252 dpi is the engine's dpi of a 420 dpi phone (`dpi_of_density(420)`): a frame factor of 2.625, so 400 x 800 logical px is
/// 1050 x 2100 device px and 800 x 400 is 2100 x 1050.
const PHONE_DPI: u32 = 252;
const PORTRAIT: (f32, f32) = (400.0, 800.0);
const LANDSCAPE: (f32, f32) = (800.0, 400.0);

/// AzWidgets' skeleton: a body in the page colour that fills the window (`height: 100%`, a column), a bar, and a scrolling pane
/// with padding, a card and a field. Boxes only: no text, so no font is needed. The colours are the system's, as AzWidgets'.
extern "C" fn page_in_system_colours(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    page_of("system:background", "system:window-background")
}

/// The same page in plain colours (a page background that is no system colour).
extern "C" fn page_in_plain_colours(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    page_of("#ffffff", "#ececec")
}

/// The layouts the tests run, by name.
const PAGES: [(&str, LayoutCallbackType); 2] = [
    ("system colours", page_in_system_colours),
    ("plain colours", page_in_plain_colours),
];

fn page_of(page: &str, card: &str) -> Dom {
    Dom::create_body()
        .with_css(&format!(
            "margin: 0; background-color: {page}; display: flex; flex-direction: column; height: 100%;"
        ))
        .with_child(Dom::create_div().with_css(&format!(
            "height: 36px; flex-shrink: 0; background-color: {card};"
        )))
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; min-height: 0; padding: 24px;",
                )
                .with_child(Dom::create_div().with_css(&format!(
                    "height: 60px; margin-bottom: 12px; background-color: {card}; border-radius: 10px;"
                )))
                .with_child(Dom::create_div().with_css(
                    "height: 40px; background-color: #ffffff; border: 1px solid #888888;",
                )),
        )
}

fn light_desktop() -> azul_css::system::SystemStyle {
    azul_css::system::defaults::macos_modern_light()
}

/// The canvas colour of the Android window of the emulator run: the system's window background of the light mode.
const CANVAS: azul_css::props::basic::color::ColorU = azul_css::props::basic::color::ColorU {
    r: 236,
    g: 236,
    b: 236,
    a: 255,
};

/// A window as a DESKTOP shell opens it (`mode_headless.rs`): a canvas that is not the page's, as the CPU canvas of every real
/// window is (the system's window background).
fn open(size: (f32, f32), layout: LayoutCallbackType) -> HeadlessWindow {
    let mut config = AppConfig::default();
    config.system_style = light_desktop();
    let app = Arc::new(AppShared::new(&config));
    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout,
        ctx: OptionRefAny::None,
    };
    options.window_state.size.dimensions = azul_core::geom::LogicalSize {
        width: size.0,
        height: size.1,
    };
    options.window_state.size.dpi = PHONE_DPI;
    options.window_state.background_color = azul_css::props::basic::OptionColorU::Some(CANVAS);
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

/// The first `Rect` of the window's display list - the canvas background (CSS 2.2 14.2) - as `(x, y, width, height)` in logical px.
fn canvas_rect_of(window: &HeadlessWindow) -> (f32, f32, f32, f32) {
    let lw = window.common.layout_window.as_ref().expect("a layout window");
    let list = &lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list;
    list.items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Rect { bounds, .. } => {
                let b = bounds.inner();
                Some((b.origin.x, b.origin.y, b.size.width, b.size.height))
            }
            _ => None,
        })
        .expect("the page paints a background")
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
fn portrait_painted(layout: LayoutCallbackType) -> (HeadlessWindow, Frame) {
    let mut window = open(PORTRAIT, layout);
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

/// Runs `check` for every page of [`PAGES`] and fails with ALL the findings, so one run says which pages are wrong.
fn for_every_page(check: impl Fn(LayoutCallbackType) -> Result<(), String>) {
    let findings: Vec<String> = PAGES
        .iter()
        .filter_map(|(name, layout)| check(*layout).err().map(|why| format!("[{name}] {why}")))
        .collect();
    assert!(findings.is_empty(), "\n{}", findings.join("\n"));
}

#[test]
fn the_canvas_colour_is_not_the_page_colour() {
    for_every_page(|layout| {
        let (window, portrait) = portrait_painted(layout);
        let (clear, page) = (clear_colour(&window), page_colour(&portrait));
        if clear == page {
            return Err(format!(
                "the premise of the other tests: a strip nobody painted shows the canvas colour {clear:?}, the page's is {page:?}"
            ));
        }
        Ok(())
    });
}

/// The display list the rotation made paints the page under the whole window: its canvas rect is the window's, not the one the
/// portrait window had. (The screenshot of the engine clears to white and would not show it if it were not.)
#[test]
fn the_display_list_paints_the_page_under_the_whole_window_after_a_rotation() {
    for_every_page(|layout| {
        let (mut window, _portrait) = portrait_painted(layout);
        let before = canvas_rect_of(&window);
        rotate(&mut window);
        let after = canvas_rect_of(&window);
        if after != (0.0, 0.0, LANDSCAPE.0, LANDSCAPE.1) {
            return Err(format!(
                "the canvas background of the list is {after:?} after the rotation: it should cover the landscape window {LANDSCAPE:?} (it was {before:?} in portrait)"
            ));
        }
        Ok(())
    });
}

/// The backend's own repaint of the mixed resize: the strip the rotation exposed is the page, and the frame is the one a repaint of
/// everything gives.
///
/// Mutation: none to make; this is the observation of the emulator run (the strip right of the old width was the canvas colour).
#[test]
fn a_rotation_to_landscape_is_painted_like_a_full_repaint() {
    for_every_page(|layout| {
        let (mut window, portrait) = portrait_painted(layout);
        let page = page_colour(&portrait);
        rotate(&mut window);
        let incremental = paint(&mut window);
        let how = format!(
            "{:?}; it painted {}; the list's canvas rect is {:?}",
            window.cpu_backend.last_frame_damage,
            incremental.painted,
            canvas_rect_of(&window)
        );
        if (incremental.width, incremental.height) != (2100, 1050) {
            return Err(format!("the frame after the rotation is {} x {} px", incremental.width, incremental.height));
        }
        // What the page looks like where the window did not exist before: right of the old width, next to the right edge.
        let strip = incremental.pixel(incremental.width - 4, incremental.height / 2);
        if strip != page {
            return Err(format!(
                "the strip a rotation exposes shows {strip:?}, the page's colour is {page:?} (the canvas colour is {:?}); the frame was repainted as: {how}",
                clear_colour(&window)
            ));
        }
        window.cpu_backend.force_full_repaint = true;
        let full = paint(&mut window);
        if let Some((x, y, a, b, count)) = first_difference(&incremental, &full) {
            return Err(format!(
                "the repaint of what changed ({how}) and a repaint of everything differ in {count} px, the first at ({x}, {y}): {a:?} against {b:?}"
            ));
        }
        Ok(())
    });
}

/// The frame the Android shell asks for after a change of the surface's size: a full repaint, which is the page and equals the
/// frame of a window that never was portrait.
#[test]
fn a_full_repaint_after_the_rotation_is_the_page() {
    for_every_page(|layout| {
        let (mut window, portrait) = portrait_painted(layout);
        let page = page_colour(&portrait);
        window.cpu_backend.force_full_repaint = true;
        rotate(&mut window);
        let after = paint(&mut window);
        let kind = format!("{:?}", window.cpu_backend.last_frame_damage);
        if kind != "Full" {
            return Err(format!("force_full_repaint paints the whole frame, the damage was {kind}"));
        }
        let strip = after.pixel(after.width - 4, after.height / 2);
        if strip != page {
            return Err(format!(
                "after a full repaint the strip is {strip:?}, the page's colour is {page:?} (the list's canvas rect is {:?})",
                canvas_rect_of(&window)
            ));
        }
        let mut from_the_start = open(LANDSCAPE, layout);
        from_the_start.regenerate_layout().expect("layout");
        let reference = paint(&mut from_the_start);
        if let Some((x, y, a, b, count)) = first_difference(&after, &reference) {
            return Err(format!(
                "the frame after the rotation and the frame of a window that opened in landscape differ in {count} px, the first at ({x}, {y}): {a:?} against {b:?}"
            ));
        }
        Ok(())
    });
}
