//! A mask clip whose box lands on a half pixel still masks every pixel it
//! covers.
//!
//! An SVG shape is drawn as a solid `Rect` under a `PushImageMaskClip`: the
//! path becomes a rasterised R8 mask and the rect is blended back against a
//! snapshot through it. The renderer sized that mask region by TRUNCATING the
//! origin and CEILING the size, so a box at y 9.5..25.5 got a mask covering
//! rows 9..25 - and row 25, which the rect does paint, was outside the region
//! `apply_mask` blends. The rect's own solid fill survived there: one
//! fully-opaque row, the full width of the box, under every icon.
//!
//! That is what a client-side titlebar's window controls looked like. Their
//! buttons are 24px tall and their glyphs 16px, so flex centring puts every
//! one of them at exactly `+0.5`.

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SIDE: f32 = 48.0;

/// The painted rows, as `(y, width_in_px)`.
fn painted_rows(markup: &str) -> Vec<(usize, usize)> {
    let parsed = azul_layout::xml::parse_xml(markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(SIDE, SIDE);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .clone();

    let mut gc = GlyphCache::new();
    let pm = cpurender::render_with_font_manager(
        &dl,
        &rr,
        &lw.font_manager,
        RenderOptions {
            width: SIDE,
            height: SIDE,
            dpi_factor: 1.0,
        },
        &mut gc,
    )
    .unwrap();

    let w = pm.width() as usize;
    (0..pm.height() as usize)
        .filter_map(|y| {
            let n = (0..w)
                .filter(|x| {
                    let p = &pm.data()[(y * w + x) * 4..][..4];
                    p[3] > 40 && u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 400
                })
                .count();
            (n > 0).then_some((y, n))
        })
        .collect()
}

/// A bar 8 units wide and 1 tall, in a 16x16 document - GTK's minimize glyph.
/// Nothing this document draws may ever be wider than 8px.
const BAR: &str =
    r#"<svg width="16" height="16"><path d="M4 10v1h8v-1z" style="fill:#000000"/></svg>"#;

/// `height` differs by one so the 16px glyph, centred, lands on a whole pixel
/// in one case and on a half pixel in the other. Margins are zeroed so the
/// only thing that moves is the centring.
fn box_of(height: u32) -> String {
    let style = format!(
        "display:flex;align-items:center;justify-content:center;width:32px;height:{height}px;\
         margin:0;"
    );
    format!("<div style=\"{style}\">{BAR}</div>")
}

#[test]
fn a_mask_clip_on_a_half_pixel_does_not_leak_its_rect() {
    let whole = painted_rows(&box_of(24));
    assert_eq!(
        whole.len(),
        1,
        "premise: on a whole pixel the glyph is its own one row, got {whole:?}"
    );
    assert_eq!(whole[0].1, 8, "premise: and it is 8px wide, got {whole:?}");

    let half = painted_rows(&box_of(25));
    let widest = half.iter().map(|(_, n)| *n).max().unwrap_or(0);
    assert!(
        widest <= 8,
        "a half-pixel origin must not leak the masked rect: rows {half:?}"
    );
}
