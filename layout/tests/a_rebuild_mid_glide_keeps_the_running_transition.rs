//! A rebuild mid-glide keeps the running transition (CSS Transitions 1, s3:
//! a running transition whose end value is the property's value in the
//! after-change style is not modified).
//!
//! The Switch styles itself on the click (`set_css_property` on the knob's
//! `transform` and the track's background, each starting its declared tween)
//! and the app answers `RefreshDom`, which rebuilds the switch in its new
//! state. The rebuild's CSS diff compared the old tree's value - the tween's
//! override, the knob mid-way - with the new tree's target, found them
//! different and started a NEW transition from where the knob stood, at
//! t = 0: every rebuild that landed during the glide (the X11 / Wayland
//! loops tick the driver ahead of the rebuild on the click's own frame; an
//! app timer; a thread's write-back) restarted the 150 ms tween from rest,
//! so the knob slowed down, waited and set off again. The new tree changed
//! nothing about where the knob is going: its tween runs on.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
};
use azul_css::props::property::{CssProperty, CssPropertyType};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    overlay::ContentChange,
    widgets::switch::{build_knob_style, build_track_style, Switch},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body > a Switch in the state `checked`.
fn page(checked: bool) -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Switch::create(checked).dom()),
    )
}

fn state() -> FullWindowState {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    ws
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

/// The value the Switch's style for `checked` gives property `ty` - what its
/// click handler writes.
fn styled(
    style: &azul_css::dynamic_selector::CssPropertyWithConditionsVec,
    ty: CssPropertyType,
) -> CssProperty {
    style
        .as_ref()
        .iter()
        .rev()
        .map(|p| p.property.clone())
        .find(|p| p.get_type() == ty)
        .unwrap_or_else(|| panic!("the switch's style sets no {ty:?}"))
}

/// The progress of the knob's slide, `None` once it settled.
fn knob_glide(lw: &LayoutWindow, knob: NodeId) -> Option<f32> {
    lw.css_transitions
        .iter()
        .find(|t| t.node == knob && t.prop_type == CssPropertyType::Transform)
        .map(|t| t.t)
}

/// The switch laid out off, then clicked: the handler's two writes, which
/// start the knob's slide and the track's fade.
fn clicked_switch() -> (LayoutWindow, NodeId) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let ws = state();
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        page(false),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let track = node_with_class(&lw, "__azul-native-switch");
    let knob = node_with_class(&lw, "__azul-native-switch-knob");
    for (node, prop) in [
        (
            track,
            styled(&build_track_style(true), CssPropertyType::BackgroundContent),
        ),
        (
            knob,
            styled(&build_knob_style(true), CssPropertyType::Transform),
        ),
    ] {
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: node,
            props: vec![prop],
            override_only: false,
        });
    }
    assert!(
        knob_glide(&lw, knob).is_some(),
        "harness: the click starts the knob's slide"
    );
    (lw, knob)
}

/// The app's rebuild, installed as the shells install one: the
/// reconciliation, the layout, its completion. Returns the knob's new id.
fn rebuild(lw: &mut LayoutWindow, checked: bool) -> NodeId {
    let ws = state();
    let mut next = page(checked);
    let pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, Instant::now());
    lw.layout_new_generation(
        next,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the next DOM lays out");
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
    node_with_class(lw, "__azul-native-switch-knob")
}

#[test]
fn a_rebuild_mid_glide_keeps_the_running_transition() {
    let (mut lw, knob) = clicked_switch();
    // Three frames of the 150 ms slide (one frame is a ninth of it).
    for _ in 0..3 {
        let _ = lw.tick_animations(1.0 / 60.0);
    }
    let before = knob_glide(&lw, knob).expect("harness: the slide is under way");
    assert!(
        before > 0.25,
        "harness: three frames moved the slide on, t = {before}"
    );

    // The app's RefreshDom lands: the switch rebuilt in the state the click
    // gave it.
    let knob = rebuild(&mut lw, true);
    let after = knob_glide(&lw, knob).expect("the rebuild must not drop the knob's slide");
    assert!(
        after >= before,
        "the rebuilt switch is going where the knob was going already: its slide runs on, it \
         must not start over (t {before} -> {after})"
    );

    // ...and it lands on time: the slide's 150 ms are nine frames, three of
    // them were before the rebuild (one more is slack for the float sum).
    let mut frames = 0;
    while knob_glide(&lw, knob).is_some() && frames < 20 {
        let _ = lw.tick_animations(1.0 / 60.0);
        frames += 1;
    }
    assert!(
        frames <= 7,
        "the slide must land in the six frames it had left, it took {frames} more"
    );
}

#[test]
fn a_rebuild_that_moves_the_target_turns_the_glide_around() {
    let (mut lw, _) = clicked_switch();
    for _ in 0..3 {
        let _ = lw.tick_animations(1.0 / 60.0);
    }
    // The app keeps the switch off (it refused the change): the rebuild's
    // target is the knob's old place, so the knob turns back from where it
    // stands - the retarget a rebuild has always made.
    let knob = rebuild(&mut lw, false);
    let back = lw
        .css_transitions
        .iter()
        .find(|t| t.node == knob && t.prop_type == CssPropertyType::Transform)
        .expect("the knob slides back to where the rebuilt switch has it");
    assert_eq!(
        back.to,
        styled(&build_knob_style(false), CssPropertyType::Transform),
        "the slide's new end is the rebuilt switch's knob"
    );
    assert!(
        back.t < 0.01,
        "a turned-around slide starts its tween afresh from where the knob stands, t = {}",
        back.t
    );
}
