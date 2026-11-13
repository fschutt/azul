//! A frame of a layout-property tween refreshes the compact cache of its own node only.
//!
//! The frame writes the tween's value as the node's override, and the layout reads that
//! node's margin, width or padding from the compact cache. The frame rebuilt the compact
//! cache of the WHOLE DOM for that one value (~2.7 ms per frame in AzWidgets). A property
//! no other node inherits is written into the node's compact entry in place; the last
//! frame, which hands the value back to the cascade, rebuilds once.

use azul_core::dom::DomId;
use azul_layout::window::LayoutWindow;

use crate::a_layout_tween_frame_reuses_the_tree_and_patches_the_list::{knob_x, relayout, toggled};

/// The compact cache's identity: a rebuild allocates new arrays.
fn compact_arrays(lw: &LayoutWindow) -> Option<usize> {
    lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .get_css_property_cache()
        .compact_cache
        .as_ref()
        .map(|c| c.tier2_dims.as_ptr() as usize)
}

#[test]
fn a_layout_tween_frame_patches_its_nodes_compact_entry() {
    let (mut lw, knob) = toggled();
    let before = compact_arrays(&lw);
    assert!(before.is_some(), "harness: the page has a compact cache");
    let x0 = knob_x(&lw, knob);
    for frame in 0..3 {
        let _ = lw.tick_animations(0.016);
        assert!(
            lw.take_transition_relayout(),
            "frame {frame}: harness - a margin-left step owes a relayout"
        );
        relayout(&mut lw);
        assert_eq!(
            compact_arrays(&lw),
            before,
            "frame {frame}: the tween rebuilt the compact cache of the whole page for one \
             node's margin"
        );
    }
    assert!(
        knob_x(&lw, knob) > x0,
        "the knob moved: its compact entry carries the tween's value"
    );
}
