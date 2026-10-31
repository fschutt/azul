//! An inline-block's paint belongs to its own node.
//!
//! The display list maps every item to the DOM node that painted it
//! (`DisplayList::node_mapping`): damage, the in-place patches of a fade or an
//! image swap and the tests find a node's paint through it. An inline-block's
//! background, border and shadows are painted by its line (`paint_inline_shape`,
//! as the line lays it out) - while the builder still named the line's block,
//! so they were mapped to the block: a flora button (an inline-block in its
//! row) had no face of its own in the list, its hover fade could not be found
//! to patch, and its damage was the block's. Inline images were already
//! mapped to themselves; the boxes now are too.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

#[test]
fn an_inline_blocks_background_border_and_shadow_belong_to_its_own_node() {
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_p().with_css("margin: 0px;").with_child(
            Dom::create_span()
                .with_id("box".into())
                .with_css(
                    "display: inline-block; width: 50px; height: 20px; background: #ff0000; \
                     border: 2px solid #0000ff; box-shadow: 0px 2px 4px #000000;",
                ),
        ),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let span = NodeId::new(
        result
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|n| n.has_id("box"))
            .expect("the inline-block"),
    );
    let dl = &result.display_list;
    let owner = |want: fn(&DisplayListItem) -> bool| -> Vec<Option<NodeId>> {
        dl.items
            .iter()
            .enumerate()
            .filter(|(_, item)| want(item))
            .map(|(i, _)| dl.node_mapping.get(i).copied().flatten())
            .collect()
    };
    let backgrounds = owner(|i| {
        matches!(i, DisplayListItem::Rect { color, .. } if color.r == 255 && color.g == 0 && color.b == 0)
    });
    let borders = owner(|i| matches!(i, DisplayListItem::Border { .. }));
    let shadows = owner(|i| matches!(i, DisplayListItem::BoxShadow { .. }));
    for (what, found) in [("background", backgrounds), ("border", borders), ("shadow", shadows)] {
        assert!(!found.is_empty(), "the inline-block paints its {what}");
        assert!(
            found.iter().all(|n| *n == Some(span)),
            "its {what} belongs to the inline-block {span:?}, not to the line's block: {found:?}"
        );
    }
}
