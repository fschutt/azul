//! The switch knob slides on EVERY toggle.
//!
//! The switch styles itself imperatively (`set_css_property` on the knob's
//! `margin-left` and the track's background) and declares an `animation` for
//! both. Four defects froze it:
//!
//! - the transition's start value was read after the new value was already written inline, so a
//!   first write seeded nothing and snapped;
//! - nothing ticked CSS transitions on the CPU renderer, so a seeded transition held its start
//!   value for good;
//! - a timer-driven layout change was flagged for a relayout that never ran, so the knob's box
//!   stayed put while its `margin-left` tweened;
//! - the tick stamp outlived the glide, so the next toggle's first frame measured the whole idle
//!   gap and landed at once.
//!
//! This pins the layout half; the shell's driver and its relayout are verified
//! live (AzWidgets, `AZ_E2E`).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    layout::LayoutMarginLeft,
    property::{CssProperty, CssPropertyType},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange, widgets::switch::Switch,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn switch_window() -> (LayoutWindow, NodeId) {
    let mut dom = Dom::create_body().with_child(Switch::create(false).dom());
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
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
    .unwrap();
    let knob = {
        let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let node_data = sd.node_data.as_container();
        (0..node_data.len())
            .map(NodeId::new)
            .find(|n| {
                node_data[*n]
                    .get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c.as_class(), Some(s) if s == "__azul-native-switch-knob"))
            })
            .expect("the switch has a knob")
    };
    (lw, knob)
}

fn margin_left(lw: &LayoutWindow, node: NodeId) -> Option<f32> {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let nd = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    match sd
        .css_property_cache
        .ptr
        .get_property(nd, &node, state, &CssPropertyType::MarginLeft)
    {
        Some(CssProperty::MarginLeft(v)) => v
            .get_property()
            .map(|m| m.inner.to_pixels_internal(0.0, 16.0, 16.0)),
        _ => None,
    }
}

#[test]
fn every_switch_toggle_seeds_a_knob_transition_that_lands_on_its_target() {
    let (mut lw, knob) = switch_window();
    for (toggle, target) in [16isize, 0, 16, 0].into_iter().enumerate() {
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: knob,
            props: vec![CssProperty::const_margin_left(LayoutMarginLeft::const_px(
                target,
            ))],
            override_only: false,
        });
        assert!(
            lw.css_transitions
                .iter()
                .any(|t| t.node == knob && t.prop_type == CssPropertyType::MarginLeft),
            "toggle {toggle}: the knob's margin-left change must seed a transition (the declared \
             animation), got {:?}",
            lw.css_transitions
        );
        for _ in 0..200 {
            if lw.css_transitions.is_empty() {
                break;
            }
            lw.tick_animations(0.016);
        }
        assert!(
            lw.css_transitions.is_empty(),
            "toggle {toggle}: the transition settles"
        );
        let resolved = margin_left(&lw, knob).expect("the knob resolves a margin-left");
        assert!(
            (resolved - target as f32).abs() < 0.5,
            "toggle {toggle}: the knob must end at {target}px, resolves {resolved}px"
        );
    }
}

fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
}

fn knob_x(lw: &LayoutWindow, knob: NodeId) -> f32 {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(knob).into(),
    })
    .expect("the knob has a box")
    .origin
    .x
}

/// The knob's BOX glides, not just its computed `margin-left`.
///
/// Live in AzWidgets the computed value tweened 16 -> 10.6 -> 3.9 -> 0 while
/// the knob's laid-out x never left 318, not even once the tween settled: the
/// transition staged its layout dirt, and the relayout that consumed it laid
/// the knob out where it already was. Every frame of the tween must move the
/// box toward the target, and the settled box must sit AT the target.
#[test]
fn the_knob_box_follows_its_animated_margin_on_every_toggle() {
    let (mut lw, knob) = switch_window();
    let rest = knob_x(&lw, knob);
    for (toggle, target) in [16isize, 0, 16, 0].into_iter().enumerate() {
        let start = knob_x(&lw, knob);
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: knob,
            props: vec![CssProperty::const_margin_left(LayoutMarginLeft::const_px(
                target,
            ))],
            override_only: false,
        });
        relayout(&mut lw);
        let goal = rest + target as f32;
        let mut xs = vec![knob_x(&lw, knob)];
        for _ in 0..200 {
            if lw.css_transitions.is_empty() {
                break;
            }
            lw.tick_animations(0.016);
            if lw.take_transition_relayout() {
                relayout(&mut lw);
            }
            xs.push(knob_x(&lw, knob));
        }
        assert!(
            xs.iter()
                .any(|x| (x - start).abs() > 0.5 && (x - goal).abs() > 0.5),
            "toggle {toggle}: the knob box must pass between {start} and {goal} mid-glide, \
             frames: {xs:?}"
        );
        let end = knob_x(&lw, knob);
        assert!(
            (end - goal).abs() < 0.5,
            "toggle {toggle}: the settled knob box must sit at {goal}, sits at {end} \
             (frames: {xs:?})"
        );
    }
}

/// The second toggle glides too, however long the switch sat idle.
///
/// `tick_animations_now` derives `dt` from the previous tick's stamp, and a
/// first tick after an idle period must count as one frame. The stamp was only
/// cleared by a tick that found nothing to move, and once the last transition
/// settled nothing called one: the CPU driver disarmed itself and the GPU path
/// stopped owing frames. The next toggle's first tick then measured the whole
/// idle gap as its `dt`, and a 150 ms glide landed in one frame — live, every
/// toggle after the first snapped.
#[test]
fn a_glide_after_an_idle_period_starts_from_a_fresh_frame() {
    let (mut lw, knob) = switch_window();
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let toggle = |lw: &mut LayoutWindow, target: isize| {
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: knob,
            props: vec![CssProperty::const_margin_left(LayoutMarginLeft::const_px(
                target,
            ))],
            override_only: false,
        });
    };

    toggle(&mut lw, 16);
    for _ in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let _ = azul_core::task::advance_test_clock_ms(16);
        lw.tick_animations_now();
    }
    assert!(
        lw.css_transitions.is_empty(),
        "harness: the first glide settles"
    );

    // The switch sits untouched for two seconds, then flips back.
    let _ = azul_core::task::advance_test_clock_ms(2000);
    toggle(&mut lw, 0);
    let _ = azul_core::task::advance_test_clock_ms(16);
    lw.tick_animations_now();

    let shown = margin_left(&lw, knob).expect("the knob resolves a margin-left");
    azul_core::task::reset_test_clock();
    assert!(
        !lw.css_transitions.is_empty() && shown > 0.5 && shown < 15.5,
        "one frame into the second glide the knob must be on its way from 16px to 0px, but it \
         shows {shown}px with {} transition(s) left",
        lw.css_transitions.len()
    );
}

/// A tick that moves no transition owes no layout.
///
/// The X11 and Wayland loops service the CSS driver on every pass, not once
/// per timer period, and a pass in the same instant as the last tick steps by
/// zero. The knob's transition then shows exactly what the last tick wrote,
/// yet the tick restyled it and flagged a relayout, so the shell laid the
/// whole window out again for a frame that could not differ from the one on
/// screen.
#[test]
fn a_zero_length_tick_owes_no_relayout() {
    let (mut lw, knob) = switch_window();
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: knob,
        props: vec![CssProperty::const_margin_left(LayoutMarginLeft::const_px(
            16,
        ))],
        override_only: false,
    });
    relayout(&mut lw);

    lw.tick_animations(0.016);
    assert!(
        lw.take_transition_relayout(),
        "harness: a real step of the knob's margin-left owes a relayout"
    );
    relayout(&mut lw);

    let before = margin_left(&lw, knob);
    lw.tick_animations(0.0);
    let after = margin_left(&lw, knob);
    assert!(
        !lw.take_transition_relayout(),
        "a zero-length tick moved nothing (margin-left {before:?} -> {after:?}), so it must not \
         owe a relayout"
    );
}

/// A frame shorter than a millisecond advances a glide by its real length.
///
/// `tick_animations_now` measured its step in WHOLE milliseconds and then
/// moved its stamp to now regardless. A pass 0.6 ms after the previous one
/// stepped by zero and its 0.6 ms were lost for good; a 1.5 ms pass stepped by
/// one. The X11 and Wayland loops tick on every pass, so there the knob lost an
/// uneven share of every frame and dragged through its 150 ms glide.
#[test]
fn sub_millisecond_frames_advance_a_glide_by_their_real_length() {
    let (mut lw, knob) = switch_window();
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: knob,
        props: vec![CssProperty::const_margin_left(LayoutMarginLeft::const_px(
            16,
        ))],
        override_only: false,
    });

    // Ten frames of 1.5 ms each. The test clock only moves in whole
    // milliseconds, so it stays frozen and each tick's previous stamp is put
    // 1.5 ms before `now` by hand.
    let now = azul_core::task::Instant::now().into_std_instant();
    let frame = std::time::Duration::from_micros(1500);
    for _ in 0..10 {
        lw.last_anim_tick = Some(azul_core::task::Instant::from(now - frame));
        lw.tick_animations_now();
    }
    let t = lw
        .css_transitions
        .iter()
        .find(|tr| tr.node == knob && tr.prop_type == CssPropertyType::MarginLeft)
        .map(|tr| tr.t);
    azul_core::task::reset_test_clock();

    let t = t.expect("15 ms into a 150 ms glide the knob is still moving");
    assert!(
        (t - 0.1).abs() < 0.002,
        "ten 1.5 ms frames are 15 ms of the knob's 150 ms glide, so its progress must be 0.1; it \
         is {t}"
    );
}
