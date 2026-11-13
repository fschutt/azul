//! Flex items keep the size their container gave them.
//!
//! Found while building AzDrive's Explorer view (FB2, 2026-09-30): a drive
//! tile's capacity bar - a block stretched across a flex column inside a flex
//! row inside a block - came out 2 px wide, the column beside it 260. Probed
//! with the debug server's `mount` op on the released engine and traced with
//! `AZ_TAFFY_DEBUG=1`, three mechanisms, one per test group below:
//!
//! A. A MEASURE overwrote a laid-out item. `TaffyBridge::compute_child_layout`
//!    wrote the result of every call into the node's `used_size`, measures
//!    (`RunMode::ComputeSize`) included, even when the measure was a cache
//!    hit. The block parent measures its flex row a second time after the
//!    row's final layout (a min-content height query); the column's final
//!    layout then comes from taffy's cache, so the bar's final layout never
//!    runs again and it keeps the measured 2 px (its borders).
//!
//! B. A definite-width item was measured at min-content. taffy's own leaf
//!    algorithm resolves a node's `width: 140px` into its known width
//!    (`SizingMode::InherentSize`); the bridge's `compute_non_flex_layout`
//!    did not, so the min-content query laid the item's text out at the
//!    width of its longest word, answered "140 wide, 8 lines tall", and taffy
//!    served that entry for the item's real 140 px query. The row and the
//!    block around it were four times too tall.
//!
//! C. A second layout pass re-laid a flex item out on its own. When a pass
//!    hits the layout cache of a flex container, the cache-hit branch of
//!    `calculate_layout_for_subtree` recursed into each child through
//!    `calculate_layout_for_subtree` - a flex item as if it were a block, with
//!    an auto height - so a `flex-grow: 1` pane that is itself a flex
//!    container shrank to its content. A scroll box in the pane (the
//!    navigation tree) is enough to make the engine lay the page out twice.
//!
//! Not compiled by the author (house rule); expected RED before the fixes.

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::NodeHierarchyItemId,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body` inside the document the debug server's `mount` op builds, laid out
/// in a `width` x `height` window.
fn laid_out(body: &str, css: &str, width: f32, height: f32) -> LayoutWindow {
    let xml = format!(
        "<html>\n<head>\n<style>\n{css}\n</style>\n</head>\n<body>\n{body}\n</body>\n</html>"
    );
    let styled = azul_layout::xml::parse_xml_to_styled_dom(&xml).expect("the page parses");
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, height);
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
    lw
}

/// The laid-out border box of the node with the id `id`.
fn rect(lw: &LayoutWindow, id: &str) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|node| node.has_id(id))
        .unwrap_or_else(|| panic!("the page has no node #{id}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!("#{id} has no layout rect"))
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

const BODY_CSS: &str = "body { display: flex; flex-direction: column; height: 100%; margin: \
                        0px; font-size: 13px; }";

// ---- A: a measure never overwrites a laid-out item ----

#[test]
fn a_block_stretched_in_a_flex_column_keeps_its_width_after_its_block_parent_is_measured_again() {
    let css = format!(
        "{BODY_CSS}
         .plain {{ margin: 4px; border: 1px solid #cccccc; }}
         .row {{ display: flex; flex-direction: row; width: 300px; }}
         .col {{ display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; }}
         .bar {{ height: 12px; border: 1px solid #888888; }}"
    );
    let lw = laid_out(
        "<div class='plain'><div class='row'><div class='col' id='col'><div class='bar' \
         id='bar'></div></div></div></div>",
        &css,
        800.0,
        600.0,
    );
    let (col, bar) = (rect(&lw, "col"), rect(&lw, "bar"));
    assert!(
        close(col.size.width, 300.0),
        "the column grows to the row's 300 px: {col:?}"
    );
    assert!(
        close(bar.size.width, col.size.width),
        "the bar is stretched across its column ({} px), not left at its measured {} px",
        col.size.width,
        bar.size.width
    );
}

#[test]
fn a_capacity_bar_in_a_tile_column_is_as_wide_as_the_column_and_fills_its_share() {
    // A drive tile: icon | column [name, bar [fill 40% | rest 60%], detail],
    // in a wrapping row of tiles with a frame - AzDrive's "This PC" view.
    let css = format!(
        "{BODY_CSS}
         .tiles {{ display: flex; flex-direction: row; flex-wrap: wrap; margin: 4px; border: \
                   1px solid #cccccc; }}
         .tile {{ display: flex; flex-direction: row; width: 250px; padding: 6px; }}
         .icon {{ width: 40px; height: 40px; flex-shrink: 0; }}
         .tcol {{ display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; }}
         .bar {{ display: flex; flex-direction: row; height: 12px; border: 1px solid #888888; }}
         .fill {{ width: 40%; }}
         .rest {{ width: 60%; }}
         .m0 {{ margin: 0px; }}"
    );
    let lw = laid_out(
        "<div class='tiles'><div class='tile'><div class='icon'></div><div class='tcol' \
         id='tcol'><p class='m0'>Home</p><div class='bar' id='bar'><div class='fill' \
         id='fill'></div><div class='rest'></div></div><p class='m0'>324 GB free of 456 \
         GB</p></div></div></div>",
        &css,
        800.0,
        600.0,
    );
    let (tcol, bar, fill) = (rect(&lw, "tcol"), rect(&lw, "bar"), rect(&lw, "fill"));
    // The tile's 250 px content box less the 40 px icon.
    assert!(
        close(tcol.size.width, 210.0),
        "the tile's column takes the rest of the tile: {tcol:?}"
    );
    assert!(
        close(bar.size.width, tcol.size.width),
        "the capacity bar spans its column ({} px), not {} px",
        tcol.size.width,
        bar.size.width
    );
    let inner = bar.size.width - 2.0;
    assert!(
        close(fill.size.width, inner * 0.4) && close(fill.size.height, 12.0),
        "the fill is 40% of the bar's {inner} px track and as tall as it: {fill:?}"
    );
}

// ---- B: a definite-width item is measured at its own width ----

#[test]
fn a_fixed_width_flex_item_is_measured_at_its_own_width_so_its_block_parent_hugs_the_row() {
    let css = format!(
        "{BODY_CSS}
         .plain {{ margin: 4px; border: 1px solid #cccccc; }}
         .row {{ display: flex; flex-direction: row; width: 300px; }}
         .wrap {{ display: flex; flex-direction: row; flex-wrap: wrap; width: 300px; }}
         .item {{ width: 140px; }}
         .m0 {{ margin: 0px; }}"
    );
    let text = "one two three four five six seven eight";
    for class in ["row", "wrap"] {
        let lw = laid_out(
            &format!(
                "<div class='plain' id='plain'><div class='{class}' id='flex'><div class='item' \
                 id='item'><p class='m0' id='text'>{text}</p></div><div class='item'><p \
                 class='m0'>nine ten eleven</p></div></div></div>"
            ),
            &css,
            800.0,
            600.0,
        );
        let (plain, flex, item, para) = (
            rect(&lw, "plain"),
            rect(&lw, "flex"),
            rect(&lw, "item"),
            rect(&lw, "text"),
        );
        assert!(
            close(item.size.width, 140.0) && close(item.size.height, para.size.height),
            "{class}: the item is 140 px wide and as tall as its text at that width: item \
             {item:?}, text {para:?}"
        );
        assert!(
            close(flex.size.height, item.size.height),
            "{class}: the flex line is as tall as its tallest item ({} px), not {} px",
            item.size.height,
            flex.size.height
        );
        assert!(
            close(plain.size.height, flex.size.height + 2.0),
            "{class}: the block around the row hugs it (row {} px + 2 px border), not the \
             min-content height of the text: {} px",
            flex.size.height,
            plain.size.height
        );
    }
}

// ---- C: a second layout pass keeps what the flex container decided ----

#[test]
fn a_grown_flex_pane_keeps_its_height_when_the_page_is_laid_out_a_second_time() {
    // The Explorer frame: a navigation pane (a scroll box) beside the content,
    // in a pane that takes the rest of a full-height column.
    let css = format!(
        "{BODY_CSS}
         .main {{ display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; }}
         .tree {{ width: 200px; flex-shrink: 0; border-right: 1px solid #d9dce3; padding: 6px; \
                  overflow-y: auto; }}
         .content {{ flex-grow: 1; min-width: 0px; overflow-y: auto; padding: 8px 16px; }}"
    );
    let lw = laid_out(
        "<div class='main' id='main'><div class='tree' id='tree'><p>This PC</p><p>Home</p></div><div \
         class='content' id='content'><p>content</p></div></div>",
        &css,
        1100.0,
        720.0,
    );
    let (main, tree, content) = (rect(&lw, "main"), rect(&lw, "tree"), rect(&lw, "content"));
    assert!(
        close(main.size.height, 720.0),
        "the pane grows to the window's 720 px, not its content's {} px",
        main.size.height
    );
    assert!(
        close(tree.size.height, 720.0) && close(content.size.height, 720.0),
        "both panes are stretched to the full height: tree {tree:?}, content {content:?}"
    );
}
