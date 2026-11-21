//! A full-width rule in a table cell that spans two columns renders.
//!
//! Mail receipts draw their divider as `<tr><td colspan="2"><hr></td></tr>`
//! (an `<hr>` is `width: 100%`). Found by the AzMail exploration
//! (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, section 1): the release
//! dylib of 2026-09-30, driven headless through the E2E `mount` op, PANICS on
//! such a table - `index out of bounds: the len is 1 but the index is 1` in
//! agg's `ScanlineU8::add_cell` (agg-rust-azul 1.1.4 `scanline_u.rs:92/97`),
//! reached from `cpurender::raster::render_glyphs_lcd` while painting the
//! text of the NEXT row. The whole reading pane goes down with it.
//!
//! Bisected on that binary: the panic needs all three of
//!   * a cell with `colspan="2"`,
//!   * a PERCENTAGE-wide block inside it (`width: 100%`; `width: 50px` renders),
//!   * a following row with two cells (one cell renders).
//!
//! The document below is byte for byte what the `mount` op handed the
//! engine. Not compiled by the author (house rule); expected RED.

use azul_core::{
    dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 1100.0;

/// The minimal receipt: a spanning divider row, then a two-cell row.
const RECEIPT: &str = "<html>\n<head>\n<style>\n\n</style>\n</head>\n<body>\n\
<table><tr><td colspan=\"2\"><div style=\"width:100%;height:4px;background:red\"></div></td></tr>\
<tr><td>a</td><td>b</td></tr></table>\n</body>\n</html>";

/// The document laid out in a `WIDTH` x `HEIGHT` window: the window and the
/// display list it painted.
fn laid_out(markup: &str) -> (LayoutWindow, DisplayList) {
    let parsed = azul_layout::xml::parse_xml(markup).expect("the receipt parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the receipt lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone();
    (lw, dl)
}

#[test]
fn every_text_run_of_the_receipt_has_a_finite_position_and_clip() {
    let (_, dl) = laid_out(RECEIPT);
    let mut runs = 0;
    for item in &dl.items {
        if let DisplayListItem::Text {
            glyphs, clip_rect, ..
        } = item
        {
            runs += 1;
            let c = clip_rect.0;
            assert!(
                c.origin.x.is_finite()
                    && c.origin.y.is_finite()
                    && c.size.width.is_finite()
                    && c.size.height.is_finite()
                    && c.size.width >= 0.0
                    && c.size.height >= 0.0,
                "a text run's clip must be a real rectangle, got {c:?}"
            );
            for g in glyphs {
                assert!(
                    g.point.x.is_finite() && g.point.y.is_finite(),
                    "a glyph must sit at a finite point, got {:?}",
                    g.point
                );
            }
        }
    }
    assert!(
        runs >= 2,
        "both cells of the second row paint text ({runs} runs)"
    );
}

#[test]
fn the_receipt_renders_on_the_cpu_without_a_panic() {
    let (lw, dl) = laid_out(RECEIPT);
    let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut gc = GlyphCache::new();
        cpurender::render_with_font_manager(
            &dl,
            &RendererResources::default(),
            &lw.font_manager,
            RenderOptions {
                width: WIDTH,
                height: HEIGHT,
                dpi_factor: 1.0,
            },
            &mut gc,
        )
        .map(|_| ())
    }));
    match rendered {
        Ok(Ok(())) => {}
        Ok(Err(e)) => panic!("the receipt must render, the renderer said: {e}"),
        Err(_) => panic!(
            "the CPU renderer panicked on a two-column receipt with a full-width rule in a \
             spanning cell"
        ),
    }
}
