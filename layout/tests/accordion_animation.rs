//! An accordion section opens and closes with a height tween, the way the
//! switch's knob slides.
//!
//! The user test of 2026-09-28: "the Accordion expansion isn't animated (the
//! Switch toggle is)". The switch writes its knob's `margin-left` through
//! `set_css_property` and declares an `animation` for it, so the write seeds
//! a CSS transition the frame driver walks. The accordion toggled its body
//! between `display: none` and `display: block` - a discrete value, nothing
//! to walk - so a section snapped open and shut.
//!
//! Two halves are pinned here:
//!
//! - the engine: a `height` tween whose start value is `auto` (the height of an open section,
//!   which its content decides) starts at the box's LAID-OUT height. `auto` does not interpolate
//!   against a length - it held its value until the midpoint and then jumped;
//! - the widget: a click on a header walks the body's height through the frames in between,
//!   opening and closing.
//!
//! The shell's frame driver (`advance_css_animations_now` / the WebRender
//! frame path) is replaced by the loop the switch test uses: tick, and
//! relayout when a layout property moved.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::RawWindowHandle,
};
use azul_css::{
    props::{layout::LayoutHeight, property::CssProperty},
    system::SystemStyle,
};
use azul_layout::{
    callbacks::{Callback, CallbackChange, ExternalSystemCallbacks},
    overlay::ContentChange,
    widgets::accordion::{Accordion, AccordionSection, AccordionSectionVec},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The accordion body's height when open: an 80 px content block inside the
/// body's 12 px top and bottom padding.
const OPEN_HEIGHT: f32 = 80.0 + 12.0 + 12.0;

fn window(dom: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the page lays out");
    lw
}

/// Relayout over the window's own styled DOM - what the frame driver does
/// when a transition moved a layout property.
fn relayout(lw: &mut LayoutWindow) {
    let result = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("harness: laid out");
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the page lays out again");
}

/// The node's laid-out height; 0 for a node without a box (`display: none`).
fn height(lw: &LayoutWindow, node: NodeId) -> f32 {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
    .map_or(0.0, |rect| rect.size.height)
}

/// Every height the node has on the way to where its transitions settle,
/// frame by frame, starting with the frame the change itself produced.
fn frames(lw: &mut LayoutWindow, node: NodeId) -> Vec<f32> {
    let mut heights = vec![height(lw, node)];
    for _ in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        lw.tick_animations(0.016);
        if lw.take_transition_relayout() {
            relayout(lw);
        }
        heights.push(height(lw, node));
    }
    heights
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
        .expect("harness: the accordion has the node")
}

/// A one-section accordion (no `on_toggle`: the widget owns its open state)
/// with an 80 px content block, laid out; returns the window, the header
/// and the body.
fn accordion(open: bool) -> (LayoutWindow, NodeId, NodeId) {
    let section = AccordionSection::new("Section", Dom::create_div().with_css("height: 80px;"))
        .with_open(open);
    let lw = window(
        Dom::create_body().with_css("margin: 0;").with_child(
            Accordion::new(AccordionSectionVec::from_vec(vec![section])).dom(),
        ),
    );
    let header = node_with_class(&lw, "__azul-native-accordion-header");
    let body = node_with_class(&lw, "__azul-native-accordion-body");
    (lw, header, body)
}

/// A click on `header`: its click callback runs the way the shell runs it,
/// and the CSS writes it made land through the content chokepoint the shell
/// applies them through.
fn click(lw: &mut LayoutWindow, header: NodeId) {
    let (core_callback, mut data) = {
        let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let node_data = sd.node_data.as_container();
        let registered = node_data[header]
            .get_callbacks()
            .as_ref()
            .iter()
            .find(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click))
            .expect("harness: the header takes clicks");
        (registered.callback.clone(), registered.refany.clone())
    };
    let mut callback = Callback::from_core(core_callback);
    let state = lw.current_window_state.clone();
    let hit = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(header)),
    };
    let (changes, _update) = lw.invoke_single_callback_at(
        hit,
        &mut callback,
        &mut data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
    for change in changes {
        let (dom_id, node_id, properties, override_only) = match change {
            CallbackChange::ChangeNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => (dom_id, node_id, properties, false),
            CallbackChange::OverrideNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => (dom_id, node_id, properties, true),
            _ => continue,
        };
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id,
            node_id,
            props: properties.as_ref().to_vec(),
            override_only,
        });
    }
    // The shell relays the window out for the relayout tier the writes
    // returned; the override channel does not do it itself.
    relayout(lw);
}

/// `height: auto -> 0px` over 200 ms, linear: a quarter of the way in, the
/// box is a quarter of the way down from its laid-out 100 px.
#[test]
fn a_height_tween_from_auto_starts_at_the_boxs_laid_out_height() {
    let mut lw = window(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("animation: height 200ms linear; overflow: clip;")
                .with_child(Dom::create_div().with_css("height: 100px;")),
        ),
    );
    let tweened = NodeId::new(1);
    assert!(
        (height(&lw, tweened) - 100.0).abs() < 0.5,
        "harness: the box starts at its content's 100 px, is {}",
        height(&lw, tweened)
    );

    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: tweened,
        props: vec![CssProperty::const_height(LayoutHeight::const_px(0))],
        override_only: false,
    });
    relayout(&mut lw);
    lw.tick_animations(0.05);
    if lw.take_transition_relayout() {
        relayout(&mut lw);
    }
    let quarter = height(&lw, tweened);
    assert!(
        (quarter - 75.0).abs() < 1.0,
        "a quarter into `height: auto -> 0px` the 100 px box must be 75 px tall, is {quarter} \
         px - `auto` does not interpolate, so the tween must start at the laid-out height"
    );
}

#[test]
fn opening_an_accordion_section_grows_its_body_over_several_frames() {
    let (mut lw, header, body) = accordion(false);
    assert!(
        height(&lw, body) < 0.5,
        "harness: a closed body takes no height, takes {}",
        height(&lw, body)
    );

    click(&mut lw, header);
    let heights = frames(&mut lw, body);
    let end = heights.last().copied().unwrap_or(0.0);
    assert!(
        (end - OPEN_HEIGHT).abs() < 0.5,
        "the opened body must settle at {OPEN_HEIGHT} px, settles at {end} (frames: {heights:?})"
    );
    assert!(
        heights
            .iter()
            .any(|h| *h > 1.0 && *h < OPEN_HEIGHT - 1.0),
        "the body must grow through the heights between 0 and {OPEN_HEIGHT} px, not snap: \
         frames {heights:?}"
    );
}

#[test]
fn closing_an_accordion_section_shrinks_its_body_over_several_frames() {
    let (mut lw, header, body) = accordion(true);
    assert!(
        (height(&lw, body) - OPEN_HEIGHT).abs() < 0.5,
        "harness: an open body is {OPEN_HEIGHT} px, is {}",
        height(&lw, body)
    );

    click(&mut lw, header);
    let heights = frames(&mut lw, body);
    let end = heights.last().copied().unwrap_or(OPEN_HEIGHT);
    assert!(
        end < 0.5,
        "the closed body must settle at 0 px, settles at {end} (frames: {heights:?})"
    );
    assert!(
        heights
            .iter()
            .any(|h| *h > 1.0 && *h < OPEN_HEIGHT - 1.0),
        "the body must shrink through the heights between {OPEN_HEIGHT} and 0 px, not snap: \
         frames {heights:?}"
    );
}
