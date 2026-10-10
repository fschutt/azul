//! A Pagination's page buttons are as wide as their own min-width says.
//!
//! The button style declares `min-width: 36px` to keep single-digit pages
//! from collapsing, next to `padding: 6px 12px` and a 1px border. With the
//! default content-box sizing that minimum applies to the CONTENT box, so a
//! "3" button came out 36 + 24 + 1 = 61px. Measured live in AzWidgets on
//! macOS: `Pagination::create(0, 10)` (Prev, ten pages, Next) was 733px wide
//! inside a 576px card and ran 157px past the page's right edge.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::pagination::Pagination, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn width_of_class(lw: &LayoutWindow, class: &str) -> Vec<f32> {
    let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("root layout");
    let container = lr.styled_dom.node_data.as_container();
    (0..container.len())
        .map(NodeId::new)
        .filter(|nid| {
            container[*nid].attributes().as_ref().iter().any(|a| {
                a.as_class().is_some_and(|c| {
                    let s: &str = c;
                    s == class
                })
            })
        })
        .filter_map(|nid| {
            let idx = *lr.layout_tree.dom_to_layout.get(&nid)?.first()?;
            Some(lr.layout_tree.nodes.get(idx.index())?.used_size?.width)
        })
        .collect()
}

#[test]
fn a_single_digit_page_button_is_as_wide_as_its_min_width() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = window_state.clone();

    // The demo's `labelled()` wrapper: a column flex container, in which the
    // row's `align-self: start` hugs its content instead of stretching.
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column;")
            .with_child(Pagination::create(0, 10).dom()),
    );
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled = StyledDom::create(&mut dom, css);
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();

    let pages = width_of_class(&lw, "__azul-native-pagination-page");
    assert_eq!(pages.len(), 10, "ten page buttons");
    // Pages 1..=9 have one digit; "10" is allowed to grow past the minimum.
    for w in &pages[..9] {
        assert!(
            (*w - 36.0).abs() < 0.5,
            "a single-digit page button is its 36px min-width wide, got {w} (all: {pages:?})"
        );
    }
    let row = width_of_class(&lw, "__azul-native-pagination");
    assert_eq!(row.len(), 1, "one pagination row");
    assert!(
        row[0] <= 576.0,
        "Prev + ten pages + Next fit the AzWidgets card's 576px, got {}",
        row[0]
    );
}
