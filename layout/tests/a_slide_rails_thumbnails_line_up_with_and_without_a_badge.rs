//! A slide rail's thumbnails line up, with and without a badge.
//!
//! The LOOK of AzShow on the wave-6 build (OFFICE7, 2026-10-03): in the rail, the thumbnails of
//! slides 3 and 5 (a build badge, a transition badge under their numbers) stood 9 px further
//! right than the others. The number column of a rail item has no width of its own, so the
//! badge icon under the number widens it and pushes that item's thumbnail right. PowerPoint's
//! rail keeps one column for the numbers and the badges: every thumbnail starts at the same x.
//!
//! Owner: WIDGETS7 (layout/src/widgets/thumbnail_strip.rs). Not compiled by the author (house
//! rule); RED.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    styled_dom::NodeHierarchyItemId,
};
use azul_css::AzString;
use azul_layout::{
    widgets::thumbnail_strip::{ThumbnailItem, ThumbnailItemVec, ThumbnailStrip},
    window::LayoutWindow,
};

use crate::editing_harness::lay_out;

/// The thumbnail boxes' x, in item order.
fn thumb_xs(lw: &LayoutWindow) -> Vec<f32> {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .enumerate()
        .filter(|(_, node)| node.has_class("__azul-native-thumbnail-strip-thumb"))
        .filter_map(|(i, _)| {
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
            })
            .map(|r| r.origin.x)
        })
        .collect()
}

#[test]
fn a_slide_rails_thumbnails_line_up_with_and_without_a_badge() {
    let item = |n: usize| {
        ThumbnailItem::create(
            Dom::create_div(),
            AzString::from(format!("{}", n + 1)),
            AzString::from(format!("Slide {}", n + 1)),
        )
    };
    let strip = ThumbnailStrip::create(ThumbnailItemVec::from_vec(vec![
        item(0),
        item(1).with_badge(AzString::from("auto_awesome")),
        item(2),
        item(3).with_badge(AzString::from("animation")),
    ]))
    .with_thumb_size(150.0, 84.0);
    let lw = lay_out(
        Dom::create_body()
            .with_css("margin: 0px; width: 220px;")
            .with_child(strip.dom()),
    );
    let xs = thumb_xs(&lw);
    assert_eq!(xs.len(), 4, "four thumbnails: {xs:?}");
    for x in &xs {
        assert!(
            (x - xs[0]).abs() < 0.5,
            "every thumbnail starts where the first does: {xs:?}"
        );
    }
}
