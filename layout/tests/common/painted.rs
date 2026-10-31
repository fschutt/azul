//! Shared helper for the paint tests of the WPT sweep (WPT8): lay a whole
//! document out the way azul loads one - `parse_xml_to_styled_dom`, the
//! document loader, which keeps the `<html>` element and its attributes -,
//! paint it with the CPU renderer and read pixels back.
//!
//! The subjects are the web-platform-tests pages of `tests/wpt/` reduced to
//! their essence: coloured boxes whose pixels have one right answer (what
//! Chrome paints), so the assertions never depend on the machine's fonts.

#![allow(dead_code)] // shared: each test file uses part of it

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A painted window: RGBA rows of `width` pixels.
pub struct Painted {
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl Painted {
    /// `(r, g, b)` of the pixel at `(x, y)`.
    pub fn rgb(&self, x: usize, y: usize) -> (u8, u8, u8) {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) lies outside the {}x{} window",
            self.width,
            self.height
        );
        let i = (y * self.width + x) * 4;
        (self.pixels[i], self.pixels[i + 1], self.pixels[i + 2])
    }

    /// Whether the pixel at `(x, y)` is `expected`, each channel within
    /// `tolerance`.
    pub fn is(&self, x: usize, y: usize, expected: (u8, u8, u8), tolerance: u8) -> bool {
        close(self.rgb(x, y), expected, tolerance)
    }

    /// How many pixels of the rectangle `x0..x1` x `y0..y1` are `expected`
    /// (each channel within `tolerance`).
    pub fn count(
        &self,
        (x0, y0, x1, y1): (usize, usize, usize, usize),
        expected: (u8, u8, u8),
        tolerance: u8,
    ) -> usize {
        let mut n = 0;
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                if self.is(x, y, expected, tolerance) {
                    n += 1;
                }
            }
        }
        n
    }
}

impl Painted {
    /// The bounding box `(x0, y0, x1, y1)` (exclusive ends) of every pixel
    /// that is `color` (each channel within `tolerance`), or `None`.
    pub fn bounds_of(
        &self,
        color: (u8, u8, u8),
        tolerance: u8,
    ) -> Option<(usize, usize, usize, usize)> {
        let mut found: Option<(usize, usize, usize, usize)> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.is(x, y, color, tolerance) {
                    found = Some(match found {
                        None => (x, y, x + 1, y + 1),
                        Some((x0, y0, x1, y1)) => {
                            (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1))
                        }
                    });
                }
            }
        }
        found
    }
}

/// Whether `a` is `b`, each channel within `tolerance`.
pub fn close(a: (u8, u8, u8), b: (u8, u8, u8), tolerance: u8) -> bool {
    a.0.abs_diff(b.0) <= tolerance
        && a.1.abs_diff(b.1) <= tolerance
        && a.2.abs_diff(b.2) <= tolerance
}

pub const WHITE: (u8, u8, u8) = (255, 255, 255);
pub const GREEN: (u8, u8, u8) = (0, 128, 0);
pub const RED: (u8, u8, u8) = (255, 0, 0);
pub const BLUE: (u8, u8, u8) = (0, 0, 255);

/// `document` (a whole `<html>` document, strict XHTML) laid out and painted
/// in a `width` x `height` window.
pub fn painted(document: &str, width: u32, height: u32) -> Painted {
    let styled = azul_layout::xml::parse_xml_to_styled_dom(document).expect("the document parses");
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width as f32, height as f32);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the document lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone();
    let mut gc = GlyphCache::new();
    let pm = cpurender::render_with_font_manager(
        &dl,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: width as f32,
            height: height as f32,
            dpi_factor: 1.0,
        },
        &mut gc,
    )
    .expect("the document paints");
    Painted {
        pixels: pm.data().to_vec(),
        width: pm.width() as usize,
        height: pm.height as usize,
    }
}
