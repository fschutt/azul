//! A sliding box moves its sliding children ONCE - a FLIP offset is
//! published relative to the sliding frame the node is painted inside.
//!
//! The reconcile gives every node whose layout position changed a FLIP
//! slide, in ABSOLUTE terms (old place minus new place): when a section of
//! a list moves down by 13 px, the section and every one of its descendants
//! slide from 13 px up. But the display list nests reference frames - a
//! child's frame is painted inside its parent's - and every renderer
//! composes nested frames: the child was moved by its own 13 px AND its
//! parent's, a grandchild three times. Mid-slide AzTasks (Cmd+2: 87 nodes
//! sliding, 70 inside another sliding node) showed rows drawn over one
//! another and chips split from their rows (PIM6 "duplicated bold text";
//! SHEETSHOW6 "overlapping sorter thumbnails"; HEADLESS6's "two layouts at
//! once").
//!
//! A node's published offset is its own minus the one of the sliding frame
//! it is painted inside: a child that moves with its parent is published
//! still. A positioned child is painted by its stacking context, outside an
//! in-flow parent's frame: it keeps its own offset.

use azul_core::{
    animation::{AnimKey, FlipTransform, InterpolationMode},
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > outer(1) > inner(2).
const OUTER: NodeId = NodeId::new(1);
const INNER: NodeId = NodeId::new(2);

fn sliding(outer_css: &str, inner_css: &str) -> LayoutWindow {
    let page = Dom::create_body()
        .with_css("margin: 0; padding: 0;")
        .with_child(
            Dom::create_div()
                .with_css(&format!("width: 100px; height: 50px; {outer_css}"))
                .with_child(Dom::create_div().with_css(&format!(
                    "width: 50px; height: 20px; background: red; {inner_css}"
                ))),
        );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    // Both moved down by 40 px in the new layout: both slide from 40 px up,
    // as the reconcile starts them.
    let from_above = FlipTransform {
        translate_x: 0.0,
        translate_y: -40.0,
        scale_x: 1.0,
        scale_y: 1.0,
    };
    for (key, node) in [(AnimKey(1), OUTER), (AnimKey(2), INNER)] {
        lw.animations
            .start_or_retarget_move(key, from_above, InterpolationMode::default());
        lw.anim_key_to_node.insert(key, node);
    }
    let _ = lw.tick_animations(0.0);
    lw
}

/// The (x, y) offset published for `node`'s reference frame.
fn published(lw: &LayoutWindow, node: NodeId) -> (f32, f32) {
    let t = lw.gpu_state_manager.caches[&DomId::ROOT_ID]
        .anim_current_transform_values
        .get(&node)
        .copied()
        .expect("the sliding node is published");
    (t.m[3][0], t.m[3][1])
}

#[test]
fn a_child_sliding_with_its_parent_is_published_still() {
    let lw = sliding("", "");
    assert_eq!(published(&lw, OUTER), (0.0, -40.0), "the parent slides");
    assert_eq!(
        published(&lw, INNER),
        (0.0, 0.0),
        "the child's frame is painted inside its parent's, which moves it already"
    );
}

#[test]
fn a_child_of_a_stacking_context_moves_with_it() {
    let lw = sliding("opacity: 0.9;", "position: relative;");
    assert_eq!(published(&lw, OUTER), (0.0, -40.0));
    assert_eq!(
        published(&lw, INNER),
        (0.0, 0.0),
        "everything below a stacking context is painted inside its frame"
    );
}

#[test]
fn a_positioned_child_of_an_in_flow_parent_keeps_its_own_slide() {
    let lw = sliding("", "position: absolute; top: 0px; left: 0px;");
    assert_eq!(published(&lw, OUTER), (0.0, -40.0));
    assert_eq!(
        published(&lw, INNER),
        (0.0, -40.0),
        "a positioned box is painted by its stacking context, outside the in-flow parent's \
         frame: nothing else moves it"
    );
}
