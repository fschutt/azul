//! What the tests of the text properties share: a window that knows the mock faces by family
//! name, a page of one block of text set under the declarations being tested, and what the layout
//! of that page hands the renderers - the glyphs of the display list, line by line, and the glyph
//! runs the PDF writer is given.
//!
//! The faces (see `tests/fonts/README.md`): `Azul Mock Mono` is built into every font manager and
//! advances half an em for every ASCII glyph, so at 20 px a glyph is 10 px wide and a line 20 px
//! tall; the others are registered here by family name, as an app registers a face.
//!
//! Every function takes the window it works in: no test sees the faces or the caches of another.

#![allow(dead_code)] // shared test helper: not every test uses every function

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::basic::FontRef;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::{
        display_list::{DisplayList, DisplayListItem},
        layout_tree::TextPayload,
    },
    text3::{
        cache::UnifiedLayout,
        glyphs::{get_glyph_runs_pdf, PdfGlyphRun},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Half an em for every ASCII glyph, an ascent of 0.8 em and a descent of 0.2: built into every
/// font manager.
pub const MONO: &str = "Azul Mock Mono";
/// Mono's twin with a whole em of advance: built into every font manager.
pub const WIDE: &str = "Azul Mock Wide";
/// `tests/fonts/azul-mock-features.ttf`: one OpenType feature for each CSS font feature.
pub const FEATURES: &str = "Azul Mock Features";
/// `tests/fonts/azul-mock-kern.ttf`: `A V` kerned by -200 units.
pub const KERN: &str = "Azul Mock Kern";
/// `tests/fonts/azul-mock-liga.ttf`: the ligatures `fi`, `ff` and `ffi`.
pub const LIGA: &str = "Azul Mock Liga";
/// `tests/fonts/azul-mock-prop.ttf`: advances of 200, 250, 500 and 900 units.
pub const PROP: &str = "Azul Mock Prop";
/// `tests/fonts/azul-mock-vertical.ttf`: sixteen ideographs with vertical metrics.
pub const VERTICAL: &str = "Azul Mock Vertical";

/// The faces of `tests/fonts` this rig registers, by family name.
static FACES: [(&str, &str); 5] = [
    (FEATURES, "azul-mock-features.ttf"),
    (KERN, "azul-mock-kern.ttf"),
    (LIGA, "azul-mock-liga.ttf"),
    (PROP, "azul-mock-prop.ttf"),
    (VERTICAL, "azul-mock-vertical.ttf"),
];

/// The bytes of the face file `file` of `tests/fonts`.
pub fn face_bytes(file: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fonts")
        .join(file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A window that knows every face of the rig by its family name.
pub fn window() -> LayoutWindow {
    let lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    for (family, file) in &FACES {
        assert!(
            lw.font_manager
                .registered_fonts
                .register_font(family, &face_bytes(file)),
            "premise: {file} is a font"
        );
    }
    lw
}

/// A page of one block set in `family` at 20 px with the declarations `css`, holding `text`.
///
/// The block has no margin, so the first glyph of the first line starts at x = 0.
pub fn page(family: &str, css: &str, text: &str) -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&format!(
                "margin: 0px; font-family: \"{family}\"; font-size: 20px; color: #000000; {css}"
            ))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
    )
}

/// A page of one block set in `family` at 20 px with the declarations `css`, holding `children`:
/// text ([`text`]) and inline boxes ([`span`]) in order.
pub fn block(family: &str, css: &str, children: Vec<Dom>) -> Dom {
    let mut div = Dom::create_div().with_css(&format!(
        "margin: 0px; font-family: \"{family}\"; font-size: 20px; color: #000000; {css}"
    ));
    for child in children {
        div = div.with_child(child);
    }
    Dom::create_body().with_css("margin: 0px;").with_child(div)
}

/// A piece of text inside a block.
pub fn text(content: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(content)
}

/// An inline box with the declarations `css`, holding `content`.
pub fn span(css: &str, content: &str) -> Dom {
    Dom::create_span_with_text(content).with_css(css)
}

/// `dom` laid out in `lw` in a window `width` px wide: what the layout hands the renderers.
pub fn lay_out(lw: &mut LayoutWindow, dom: Dom, width: f32) -> DisplayList {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone()
}

/// One glyph the display list paints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Painted {
    /// The glyph id in its face.
    pub id: u32,
    /// The pen: where the glyph's origin sits on its baseline, in px.
    pub x: f32,
    pub y: f32,
    /// The size the run is painted at.
    pub size: f32,
    /// The face (`ParsedFont::hash`).
    pub face: u64,
    /// The run is drawn thickened (the face has no bold cut).
    pub synthetic_bold: bool,
}

/// Every glyph the display list paints, in painting order.
pub fn painted(dl: &DisplayList) -> Vec<Painted> {
    dl.items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text {
                glyphs,
                font_hash,
                font_size_px,
                ..
            } => glyphs
                .iter()
                .map(|g| Painted {
                    id: g.index,
                    x: g.point.x,
                    y: g.point.y,
                    size: *font_size_px,
                    face: font_hash.font_hash,
                    synthetic_bold: font_hash.synthetic_bold,
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// The glyph ids the display list paints, in painting order.
pub fn ids(dl: &DisplayList) -> Vec<u32> {
    painted(dl).iter().map(|g| g.id).collect()
}

/// The painted glyphs grouped into lines: glyphs on one baseline (within half a px) are one line,
/// the lines from the top.
pub fn lines(dl: &DisplayList) -> Vec<Vec<Painted>> {
    let mut lines: Vec<Vec<Painted>> = Vec::new();
    for glyph in painted(dl) {
        match lines.iter_mut().find(|line| (line[0].y - glyph.y).abs() < 0.5) {
            Some(line) => line.push(glyph),
            None => lines.push(vec![glyph]),
        }
    }
    lines.sort_by(|a, b| a[0].y.total_cmp(&b[0].y));
    lines
}

/// The glyph ids of each line, from the top.
pub fn line_ids(dl: &DisplayList) -> Vec<Vec<u32>> {
    lines(dl)
        .into_iter()
        .map(|line| line.into_iter().map(|g| g.id).collect())
        .collect()
}

/// The width of a line of the page: from the pen of its first glyph to the pen after its last,
/// given the advance of the last glyph.
pub fn extent(line: &[Painted], last_advance: f32) -> f32 {
    let first = line.iter().map(|g| g.x).fold(f32::MAX, f32::min);
    let last = line.iter().map(|g| g.x).fold(f32::MIN, f32::max);
    last + last_advance - first
}

/// Every layout of text the display list holds, as the PDF writer reads it: the shaped items of
/// each block (a bare layout, or the payload of the production path).
pub fn text_layouts(dl: &DisplayList) -> Vec<UnifiedLayout> {
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::TextLayout { layout, .. } => {
                if let Some(bare) = layout.downcast_ref::<UnifiedLayout>() {
                    return Some(bare.clone());
                }
                let payload = layout.downcast_ref::<TextPayload>()?;
                Some(if payload.sparse.items.is_empty() {
                    UnifiedLayout {
                        items: payload.dense.to_unified_items(),
                        overflow: payload.sparse.overflow.clone(),
                    }
                } else {
                    (*payload.sparse).clone()
                })
            }
            _ => None,
        })
        .collect()
}

/// The glyph runs the PDF writer is handed for the page, in order: what `printpdf` draws its text
/// from.
pub fn pdf_runs(lw: &LayoutWindow, dl: &DisplayList) -> Vec<PdfGlyphRun<FontRef>> {
    let fonts = lw.font_manager.get_loaded_fonts();
    text_layouts(dl)
        .iter()
        .flat_map(|layout| get_glyph_runs_pdf(layout, &fonts))
        .collect()
}

/// A fresh window, a page of `text` set in `family` under `css` laid out in it, and the display
/// list.
pub fn laid_out(family: &str, css: &str, text: &str, width: f32) -> (LayoutWindow, DisplayList) {
    let mut lw = window();
    let dl = lay_out(&mut lw, page(family, css, text), width);
    (lw, dl)
}

/// The glyph ids of `text` set in `family` under `css` in a window 800 px wide.
pub fn ids_of(family: &str, css: &str, text: &str) -> Vec<u32> {
    ids(&laid_out(family, css, text, 800.0).1)
}
