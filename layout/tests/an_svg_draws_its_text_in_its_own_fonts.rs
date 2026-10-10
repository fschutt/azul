//! An SVG draws its `<text>` in the fonts its own stylesheet embeds.
//!
//! The SVG of a PDF page (printpdf's `page_to_svg`) carries each font of the
//! page as an `@font-face` with a base64 `data:` URI, named by its PDF
//! resource name (`F1`, `F2`), and its `<text>`s name them in `font-family`.
//! The loaders parse those fonts (once per distinct font) and land them on
//! the text as `StyleFontFamily::Ref` - SCOPED to their `<svg>`: every page
//! of a document has an `F1`, and two pages in one DOM are two fonts. Before,
//! the family name went to the system font lookup, which has no `F1`.

use azul_core::{
    dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use base64::Engine as _;
use rust_fontconfig::FcFontCache;

/// A test font as the `data:` URI printpdf embeds it in.
fn font_uri(file: &str) -> String {
    variant_uri(file, 0)
}

/// `file` with `n` bytes after its tables: another face to the font pool (a
/// face is known by its bytes), the same glyphs.
fn variant_uri(file: &str, n: usize) -> String {
    let path = format!("{}/tests/fonts/{file}", env!("CARGO_MANIFEST_DIR"));
    let mut bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    bytes.extend(std::iter::repeat_n(0u8, n));
    format!(
        "data:font/otf;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// A page as printpdf writes it: its fonts in a `<style>`, the text "Mi" at
/// 100 px in its `F1`.
fn page(f1: &str) -> String {
    format!(
        r#"<svg width="300" height="200" viewBox="0 0 300 200"><style>
@font-face {{ font-family: "F1"; src: url("{}"); }}
</style><text x="10" y="150" font-family="F1" font-size="100">Mi</text></svg>"#,
        font_uri(f1)
    )
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws;
    lw
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom) {
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .unwrap();
}

/// The font hash of every text run the window draws.
fn drawn_fonts(lw: &LayoutWindow) -> Vec<u64> {
    let dl = &lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list;
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { font_hash, .. } => Some(font_hash.font_hash),
            _ => None,
        })
        .collect()
}

fn laid_out(styled: StyledDom) -> Vec<Vec<f32>> {
    let mut lw = window();
    lay_out(&mut lw, styled);
    let dl = &lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list;
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => {
                Some(glyphs.iter().map(|g| g.point.x).collect())
            }
            _ => None,
        })
        .collect()
}

/// The x of each glyph of each text run, the markup loaded by the tree
/// loader (`Dom::create_from_parsed_xml`).
fn glyph_xs(body: &str) -> Vec<Vec<f32>> {
    let markup = format!("<html><body style=\"margin: 0px\">{body}</body></html>");
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    laid_out(StyledDom::create(&mut dom, azul_css::css::Css::empty()))
}

/// The same, loaded by the document loader (straight into an arena).
fn glyph_xs_streamed(body: &str) -> Vec<Vec<f32>> {
    let markup = format!("<html><body style=\"margin: 0px\">{body}</body></html>");
    laid_out(azul_layout::xml::parse_xml_to_styled_dom(&markup).expect("the markup parses"))
}

/// How far the `i` of "Mi" is from its `M`: the font's advance of `M`.
fn m_advance(runs: &[Vec<f32>]) -> f32 {
    let xs: Vec<f32> = runs.iter().flatten().copied().collect();
    assert!(xs.len() >= 2, "two glyphs are drawn: {runs:?}");
    xs[1] - xs[0]
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.75
}

#[test]
fn a_text_is_drawn_in_the_font_its_svg_embeds() {
    // azul-mock-prop: M is 900 units wide, 90 px at 100 px.
    let tree = m_advance(&glyph_xs(&page("azul-mock-prop.ttf")));
    assert!(
        close(tree, 90.0),
        "the tree loader draws it in the page's font: {tree}"
    );
    let streamed = m_advance(&glyph_xs_streamed(&page("azul-mock-prop.ttf")));
    assert!(
        close(streamed, 90.0),
        "so does the document loader: {streamed}"
    );
}

#[test]
fn two_svgs_fonts_of_one_name_are_two_fonts() {
    // azul-mock-liga's M is 500 units, azul-mock-prop's 900: each page's F1.
    let body = format!(
        "{}{}",
        page("azul-mock-liga.ttf"),
        page("azul-mock-prop.ttf")
    );
    for (loader, runs) in [
        ("tree", glyph_xs(&body)),
        ("document", glyph_xs_streamed(&body)),
    ] {
        assert_eq!(runs.len(), 2, "{loader} loader: one run per page: {runs:?}");
        let (first, second) = (m_advance(&runs[..1]), m_advance(&runs[1..]));
        assert!(
            close(first, 50.0) && close(second, 90.0),
            "{loader} loader: page one's F1 is liga (50 px), page two's prop (90 px): {first}, {second}"
        );
    }
}

#[test]
fn a_font_is_parsed_once_for_every_page_that_embeds_it() {
    let uri = font_uri("azul-mock-prop.ttf");
    let first = azul_layout::font_from_url(&uri).expect("the font parses");
    let again = azul_layout::font_from_url(&uri).expect("the font parses");
    assert_eq!(first, again, "the same font, not a second copy");
    assert!(
        azul_layout::font_from_url("data:font/otf;base64,AAAA").is_none(),
        "not a font"
    );
    assert!(
        azul_layout::font_from_url("https://example.com/f.otf").is_none(),
        "not embedded"
    );
}

/// Pages `0..count`, each in a font of its own (all named `F1`); the pages
/// not in `shown` are `display: none`.
fn pages(count: usize, shown: impl Fn(usize) -> bool) -> StyledDom {
    let body: String = (0..count)
        .map(|i| {
            format!(
                r#"<svg width="300" height="200" viewBox="0 0 300 200" style="display: {}"><style>
@font-face {{ font-family: "F1"; src: url("{}"); }}
</style><text x="10" y="150" font-family="F1" font-size="100">Mi</text></svg>"#,
                if shown(i) { "block" } else { "none" },
                variant_uri("azul-mock-prop.ttf", i + 1)
            )
        })
        .collect();
    let markup = format!("<html><body style=\"margin: 0px\">{body}</body></html>");
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

#[test]
fn the_fonts_of_pages_that_went_are_given_back() {
    let mut lw = window();
    lay_out(&mut lw, pages(12, |_| true));
    assert_eq!(drawn_fonts(&lw).len(), 12, "every page draws its text");
    assert!(
        lw.font_manager.embedded_font_count() >= 12,
        "the pool holds the pages' fonts"
    );
    // The document scrolled: one page is left.
    for _ in 0..3 {
        lay_out(&mut lw, pages(1, |_| true));
    }
    assert_eq!(
        lw.font_manager.embedded_font_count(),
        1,
        "the fonts of the pages that went are dropped; the shown page's stays"
    );
}

#[test]
fn a_page_whose_font_was_dropped_while_hidden_draws_again_when_shown() {
    let mut lw = window();
    // Twelve pages, only the first shown: the hidden pages' fonts are named
    // by the DOM but drawn by nothing, and dropped.
    for _ in 0..3 {
        lay_out(&mut lw, pages(12, |i| i == 0));
    }
    assert!(
        lw.font_manager.embedded_font_count() < 12,
        "the hidden pages' fonts are dropped"
    );
    // The same fonts, every page shown: each must be drawable again.
    lay_out(&mut lw, pages(12, |_| true));
    let drawn = drawn_fonts(&lw);
    assert_eq!(drawn.len(), 12, "every page draws its text");
    for hash in drawn {
        assert!(
            lw.font_manager.resolve_font_by_hash(hash).is_some(),
            "a drawn font is in the pool again, or the renderer cannot draw it ({hash})"
        );
    }
}
