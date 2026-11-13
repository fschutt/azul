//! A `transform` tween is a GPU property: each frame writes the node's matrix
//! into the GPU value cache, and nothing is laid out or rebuilt.
//!
//! The AzWidgets switch knob slid by `margin-left`, a LAYOUT property, so every
//! frame of its 150 ms glide was a whole-window incremental relayout: a
//! reconcile of all 2234 layout nodes, a full display list, a whole-DOM
//! compact-cache rebuild for the one override, every VirtualView re-invoked,
//! the accessibility tree rebuilt - 20 ms per frame for a 16 px move, of which
//! the layout itself was 0.4 ms (ANIMFRAME8, measured on the wave-8 build).
//!
//! A knob that slides is a MOVE, and a move travels on the compositor. The
//! knob now carries `transform: translateX(..)` in both states (so its
//! reference frame exists from the first layout), declares its tween on
//! `transform`, and a tick of that tween owes a repaint and nothing else.
//!
//! Two defects of the display-list-only frame (the path a paint-scope tween
//! takes when it is not a solid colour) are pinned here as well:
//!
//! - it rebuilt the list against the OLD GPU values: `GpuValueCache::synchronize` ran only inside
//!   a layout pass, so a `transform` written without a layout never reached the screen until
//!   some unrelated relayout;
//! - it never consumed the css dirt the tick staged for it, so the dirt grew by one entry per
//!   tick and every later tick was refused the values-only frame.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::{PercentageValue, PixelValue},
    property::{CssProperty, CssPropertyType},
    style::{StyleOpacity, StyleTransform, StyleTransformVec},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange, widgets::switch::Switch,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn window_with(dom: Dom) -> LayoutWindow {
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the page lays out");
    lw
}

fn node_with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == class))
        })
        .unwrap_or_else(|| panic!("no node with class {class}"))
}

fn translate_x(px: isize) -> CssProperty {
    CssProperty::const_transform(StyleTransformVec::from_vec(vec![
        StyleTransform::TranslateX(PixelValue::const_px(px)),
    ]))
}

/// The x offset the node is PAINTED at (the matrix of the reference frame the
/// display list opens for it), `None` when it is painted untransformed.
fn painted_x(lw: &LayoutWindow, node: NodeId) -> Option<f32> {
    lw.gpu_state_manager
        .painted_transform_of(DomId::ROOT_ID, node)
        .and_then(|m| m.transform_point2d(LogicalPosition::zero()))
        .map(|p| p.x)
}

fn laid_out_x(lw: &LayoutWindow, node: NodeId) -> f32 {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(node).into(),
    })
    .expect("the node has a box")
    .origin
    .x
}

#[test]
fn the_switch_knob_slides_on_the_gpu_and_owes_no_layout() {
    let mut lw = window_with(Dom::create_body().with_child(Switch::create(false).dom()));
    let knob = node_with_class(&lw, "__azul-native-switch-knob");
    let box_x = laid_out_x(&lw, knob);
    assert_eq!(
        painted_x(&lw, knob).map(f32::round),
        Some(0.0),
        "the switched-off knob must carry its transform (translateX(0)) from the first layout, so \
         its reference frame exists before it ever moves"
    );

    // The click handler's write.
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: knob,
        props: vec![translate_x(16)],
        override_only: false,
    });
    assert!(
        lw.css_transitions
            .iter()
            .any(|t| t.node == knob && t.prop_type == CssPropertyType::Transform),
        "the knob declares its tween on `transform`, so the write seeds one, got {:?}",
        lw.css_transitions
    );
    let layout_passes = lw.frame_report.layout_passes;
    let dl_rebuilds = lw.frame_report.dl_rebuilds;

    let mut xs = Vec::new();
    for frame in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let _ = lw.tick_animations(0.016);
        assert!(
            !lw.take_transition_relayout(),
            "frame {frame}: a transform tween moves no box - it must not owe a relayout"
        );
        let repaint_only = lw.take_transition_patched() || lw.animation_tick_is_values_only();
        assert!(
            repaint_only,
            "frame {frame}: the tick only moved a GPU value (and patched the track's colour in \
             place) - it owes a repaint, not a display-list rebuild (pending css dirt: {:?})",
            lw.pending_css_dirty
        );
        xs.push(painted_x(&lw, knob).unwrap_or(f32::NAN));
    }
    assert!(
        xs.iter().any(|x| *x > 0.5 && *x < 15.5),
        "the knob must be PAINTED between 0 and 16 px mid-glide, frames: {xs:?}"
    );
    let end = painted_x(&lw, knob).unwrap_or(f32::NAN);
    assert!(
        (end - 16.0).abs() < 0.5,
        "the settled knob must be painted 16 px to the right, it is at {end} (frames: {xs:?})"
    );
    assert_eq!(
        laid_out_x(&lw, knob),
        box_x,
        "the knob's laid-out box never moves - the slide is a transform"
    );
    assert_eq!(
        (lw.frame_report.layout_passes, lw.frame_report.dl_rebuilds),
        (layout_passes, dl_rebuilds),
        "the glide's frames ran no layout pass and rebuilt no display list"
    );
}

#[test]
fn a_transform_written_without_a_layout_reaches_the_painted_matrix() {
    let mut lw = window_with(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_class("moved".into())
                .with_css("width: 20px; height: 20px; transform: translateX(0px);"),
        ),
    );
    let node = node_with_class(&lw, "moved");
    assert_eq!(
        painted_x(&lw, node).map(f32::round),
        Some(0.0),
        "harness: the node opens a reference frame at 0"
    );
    // No `animation` declared: the write is the new value at once, and a
    // paint-scope write rebuilds the display list from the existing layout.
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        props: vec![translate_x(30)],
        override_only: false,
    });
    let x = painted_x(&lw, node).unwrap_or(f32::NAN);
    assert!(
        (x - 30.0).abs() < 0.5,
        "the display list rebuilt for the write must bind the NEW matrix (30 px), it binds {x}"
    );
}

#[test]
fn a_display_list_frame_consumes_the_dirt_its_tween_staged() {
    let mut lw = window_with(Dom::create_body().with_child(
        Dom::create_div().with_class("faded".into()).with_css(
            "width: 20px; height: 20px; background: red; animation: opacity 150ms linear;",
        ),
    ));
    let node = node_with_class(&lw, "faded");
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        props: vec![CssProperty::const_opacity(StyleOpacity {
            inner: PercentageValue::const_new(50),
        })],
        override_only: false,
    });
    assert!(
        lw.css_transitions.iter().any(|t| t.node == node),
        "harness: the opacity write seeds a tween"
    );
    for frame in 0..3 {
        let _ = lw.tick_animations(0.016);
        assert!(
            !lw.take_transition_relayout(),
            "frame {frame}: opacity moves no box"
        );
        // What the shells do for a tick that is neither patched nor values-only.
        if !lw.take_transition_patched() {
            lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
        }
        assert!(
            lw.pending_css_dirty.is_none(),
            "frame {frame}: the display list was rebuilt from the styles the tick wrote - the \
             paint dirt it staged is served and must not linger, got {:?}",
            lw.pending_css_dirty
        );
    }
}
