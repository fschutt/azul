//! A cached shaping is re-stamped with the run it is HIT from.
//!
//! `shape_visual_items_with_per_item_cache` caches a group's shaped clusters
//! under its text and layout style, and on a hit re-stamps the paint and the
//! node from the current items - finding each cluster's item by equality of
//! `source_content_index`. A cluster shaped where its text was run 0 and hit
//! where it is run 1 matched no current item, so nothing was re-stamped and it
//! kept `source_run` 0: every caret, selection end and hit test in that run
//! named the paragraph's FIRST run instead. Seen as typing after a Tab into
//! `Hello <b>world</b>` landing in "Hello": the caret at the end of "world"
//! read `(run 0, byte 4)`, which is the "o" of "Hello".

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, text3::cache::ShapedItem, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

/// `body(0) > [p.a(1) > b(2) > "world"(3), p.b(4) > ["Hello "(5), b(6) > "world"(7)]]`
#[test]
fn a_cached_run_takes_the_run_index_of_the_item_it_is_hit_from() {
    let class = |c: &str| vec![IdOrClass::Class(c.into())].into();
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_p()
                .with_ids_and_classes(class("a"))
                .with_child(Dom::create_b().with_child(text("world"))),
        )
        .with_child(
            Dom::create_p()
                .with_ids_and_classes(class("b"))
                .with_child(text("Hello "))
                .with_child(Dom::create_b().with_child(text("world"))),
        );
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 14px; width: 600px; }",
    );
    let styled = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();

    let target = lw
        .text_target_at_node(dnid(7))
        .expect("the second paragraph is laid out");
    // In order: "Hello " (six clusters, run 0, node 5), then "world" (five
    // clusters, which must say run 1 and node 7).
    let clusters: Vec<(u32, u32, Option<NodeId>)> = target
        .layout
        .items
        .iter()
        .filter_map(|item| match &item.item {
            ShapedItem::Cluster(c) => Some((
                c.source_cluster_id.source_run,
                c.source_content_index.run_index,
                c.source_node_id,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(clusters.len(), 11, "premise: \"Hello world\" is 11 clusters: {clusters:?}");
    assert!(
        clusters[..6]
            .iter()
            .all(|&c| c == (0, 0, Some(NodeId::new(5)))),
        "premise: \"Hello \" is run 0 of node 5: {clusters:?}"
    );
    assert_eq!(
        clusters[6..].to_vec(),
        vec![(1, 1, Some(NodeId::new(7))); 5],
        "\"world\" is the second paragraph's run 1, text node 7 (source_run, content run, node)"
    );
}
