//! A block's `text-decoration` decorates the text in it.
//!
//! CSS Text Decoration 3 §2.1: decorations "are propagated to all in-flow
//! children" of the box they are set on - a block container's to the inline
//! content it holds, an inline box's to everything inside it - but "not ... to
//! any out-of-flow descendants, nor to the contents of atomic inline-level
//! descendants such as inline blocks". A text run read only its own node's
//! (`text-decoration` is not inherited), so `<p style="text-decoration:
//! underline">text</p>` painted no line at all, nor did a struck-through task
//! row (`<div style="text-decoration: line-through"><p>...`), nor a coloured
//! span inside an underlined link. Only text sitting directly in the decorated
//! span had its line.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The decorations painted for `dom`: (kind, x from, x to).
fn decorations(dom: Dom) -> Vec<(&'static str, f32, f32)> {
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(500.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .clone();
    dl.items
        .iter()
        .filter_map(|item| {
            let (kind, bounds) = match item {
                DisplayListItem::Underline { bounds, .. } => ("underline", bounds),
                DisplayListItem::Strikethrough { bounds, .. } => ("line-through", bounds),
                DisplayListItem::Overline { bounds, .. } => ("overline", bounds),
                _ => return None,
            };
            let r = bounds.into_inner();
            Some((kind, r.origin.x, r.origin.x + r.size.width))
        })
        .collect()
}

fn text(t: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(t)
}

fn page(child: Dom) -> Dom {
    Dom::create_body().with_css("margin: 0px; font-size: 16px;").with_child(child)
}

#[test]
fn a_paragraphs_underline_underlines_its_text() {
    let found = decorations(page(
        Dom::create_p()
            .with_css("margin: 0px; text-decoration: underline;")
            .with_child(text("Underlined")),
    ));
    assert_eq!(found.len(), 1, "one underline under the paragraph's text: {found:?}");
    assert_eq!(found[0].0, "underline");
    assert!(found[0].2 - found[0].1 > 40.0, "under the whole word: {found:?}");
}

#[test]
fn a_struck_through_row_strikes_the_text_of_its_paragraphs() {
    let found = decorations(page(
        Dom::create_div()
            .with_css("text-decoration: line-through;")
            .with_child(Dom::create_p().with_css("margin: 0px;").with_child(text("Done"))),
    ));
    assert_eq!(found.iter().filter(|d| d.0 == "line-through").count(), 1, "{found:?}");
}

#[test]
fn an_underlined_links_coloured_span_is_underlined_too() {
    let found = decorations(page(
        Dom::create_p().with_css("margin: 0px;").with_child(
            Dom::create_span()
                .with_css("text-decoration: underline;")
                .with_child(
                    Dom::create_span()
                        .with_css("color: red;")
                        .with_child(text("nested")),
                ),
        ),
    ));
    assert_eq!(found.iter().filter(|d| d.0 == "underline").count(), 1, "{found:?}");
}

#[test]
fn a_decoration_stops_at_an_inline_block_and_an_out_of_flow_box() {
    // The inline-block's text is not underlined: the underlines end before
    // it and start again after it.
    let found = decorations(page(
        Dom::create_p()
            .with_css("margin: 0px; text-decoration: underline;")
            .with_child(text("before "))
            .with_child(
                Dom::create_span()
                    .with_css("display: inline-block; width: 100px;")
                    .with_child(text("box")),
            )
            .with_child(text(" after")),
    ));
    assert_eq!(
        found.iter().filter(|d| d.0 == "underline").count(),
        2,
        "the text before and after the box, not the box's own: {found:?}"
    );

    let found = decorations(page(
        Dom::create_div()
            .with_css("position: relative; height: 40px; text-decoration: underline;")
            .with_child(
                Dom::create_p()
                    .with_css("margin: 0px; position: absolute; top: 0px; left: 0px;")
                    .with_child(text("absolute")),
            ),
    ));
    assert!(found.is_empty(), "an absolutely positioned box is not decorated: {found:?}");
}
