//! An `<svg>` that states a size but no `viewBox` draws 1:1.
//!
//! SVG's sizing rules say user units map straight onto the viewport when
//! `viewBox` is absent - the element behaves as if it declared
//! `viewBox="0 0 <width> <height>"`. azul recorded a user space only when the
//! attribute was literally there, so every shape in such a document fell back
//! to whatever the mask rasteriser does without one: MEASURED, half scale and
//! anchored at the box's origin instead of at the shape's own coordinates.
//!
//! Every window-control icon in a GTK theme is exactly this document - Mint-Y
//! writes `height="16" width="16"` and no viewBox - which is why a titlebar's
//! controls came out as a small illegible cluster in the corner of each
//! button while azul's OWN close glyph, the one markup with a `viewBox`, drew
//! correctly.

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SIDE: f32 = 32.0;

/// The dark pixels the markup paints, as `(x, y)`.
fn painted(markup: &str) -> Vec<(usize, usize)> {
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
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let p = &pm.data()[(y * w + x) * 4..][..4];
            p[3] > 40 && u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 400
        })
        .collect()
}

fn bounds(px: &[(usize, usize)]) -> (usize, usize, usize, usize) {
    let xs: Vec<usize> = px.iter().map(|p| p.0).collect();
    let ys: Vec<usize> = px.iter().map(|p| p.1).collect();
    (
        *xs.iter().min().unwrap(),
        *ys.iter().min().unwrap(),
        xs.iter().max().unwrap() - xs.iter().min().unwrap() + 1,
        ys.iter().max().unwrap() - ys.iter().min().unwrap() + 1,
    )
}

/// An 8x4 rectangle at user (4, 6) of a 16x16 document. The `<svg>` is a
/// replaced element sized 16x16 and centred by the UA `<body>` margin at
/// (8, 8), so the rectangle has to land at (12, 14) and measure 8x4.
const RECT: &str = r#"<path d="M4 6v4h8v-4z" style="fill:#000000"/>"#;

#[test]
fn a_stated_size_without_a_viewbox_is_a_one_to_one_user_space() {
    let with_box = painted(&format!(
        r#"<svg width="16" height="16" viewBox="0 0 16 16">{RECT}</svg>"#
    ));
    assert_eq!(
        bounds(&with_box),
        (12, 14, 8, 4),
        "premise: WITH a viewBox the shape lands at its own coordinates, at 1:1"
    );

    let without = painted(&format!(r#"<svg width="16" height="16">{RECT}</svg>"#));
    assert!(
        !without.is_empty(),
        "the shape must be painted at all without a viewBox"
    );
    assert_eq!(
        bounds(&without),
        bounds(&with_box),
        "an absent viewBox is the SAME user space as `0 0 width height`"
    );
}
