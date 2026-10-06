//! An SVG shape is drawn through the `transform`s of its groups and its own.
//!
//! The builtin SVG renderers keep a `transform` attribute on its node; the
//! display list composes them from the shape up to its `<svg>` and maps the
//! shape's geometry (its clip mask, its stroke) through them, before the
//! viewBox takes over. printpdf's page SVG puts nearly every shape under one
//! (`matrix(...)` from the PDF's `cm`); every such shape was drawn as if the
//! attribute were not there.

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SIDE: f32 = 64.0;

/// The bounds `(x, y, width, height)` of the dark pixels `svg` paints at the
/// window's origin (a 64x64 `<svg>`, user space 1:1, no body margin).
fn painted(svg_content: &str) -> Option<(usize, usize, usize, usize)> {
    let markup = format!(
        "<html><body style=\"margin: 0px\"><svg width=\"64\" height=\"64\" viewBox=\"0 0 64 \
         64\">{svg_content}</svg></body></html>"
    );
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(SIDE, SIDE);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut None)
        .unwrap();
    let dl = lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list.clone();
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
    let dark: Vec<(usize, usize)> = (0..pm.height() as usize)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let p = &pm.data()[(y * w + x) * 4..][..4];
            p[3] > 128 && u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 200
        })
        .collect();
    let xs = dark.iter().map(|p| p.0);
    let ys = dark.iter().map(|p| p.1);
    let (x0, x1) = (xs.clone().min()?, xs.max()?);
    let (y0, y1) = (ys.clone().min()?, ys.max()?);
    Some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

const SQUARE: &str = r##"<path d="M0 0 L10 0 L10 10 L0 10 Z" fill="#000000"/>"##;

#[test]
fn a_shape_in_a_translated_group_is_drawn_where_the_group_moves_it() {
    assert_eq!(painted(SQUARE), Some((0, 0, 10, 10)), "premise: the square at the origin");
    assert_eq!(
        painted(&format!(r#"<g transform="translate(20 10)">{SQUARE}</g>"#)),
        Some((20, 10, 10, 10))
    );
}

#[test]
fn a_shapes_own_matrix_scales_and_moves_it_as_printpdf_writes_it() {
    // printpdf's `matrix(a b c d e f)`, space separated.
    assert_eq!(
        painted(r##"<path transform="matrix(2 0 0 2 4 6)" d="M0 0 L10 0 L10 10 L0 10 Z" fill="#000000"/>"##),
        Some((4, 6, 20, 20))
    );
}

#[test]
fn nested_groups_compose_inner_first() {
    // scale(2) applies first, then the outer translate(10 0).
    assert_eq!(
        painted(&format!(
            r#"<g transform="translate(10 0)"><g transform="scale(2)">{SQUARE}</g></g>"#
        )),
        Some((10, 0, 20, 20))
    );
}

#[test]
fn a_stroke_scales_with_its_transform() {
    // A 1-unit stroke under scale(4) is 4 px wide.
    let bounds = painted(
        r##"<path transform="scale(4)" d="M2 1 L2 10" fill="none" stroke="#000000" stroke-width="1"/>"##,
    )
    .expect("the stroke paints");
    assert!(
        (3..=5).contains(&bounds.2),
        "the stroke is about 4 px wide: {bounds:?}"
    );
}
