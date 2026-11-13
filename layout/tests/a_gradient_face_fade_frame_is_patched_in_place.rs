//! A gradient face's fade patches the display list in place, as a colour's
//! fade does: its frame owes a repaint, not a restyle and a rebuilt list.
//!
//! The flora theme paints a control's face as a two-stop linear gradient
//! (`themes::decl::face`): the switch's track, a button's hover face. A fade
//! between two faces of one shape interpolates stop by stop
//! (`interpolate_background_layers`), but only a SOLID background was
//! patchable, so every frame of a face fade took the restyle path: the
//! override, a compact-cache and inheritance rebuild of the whole DOM, and a
//! display list rebuilt from scratch. The same Switch therefore glided on a
//! bare repaint per frame in the flat look (AzWidgets, by default) and rebuilt
//! its window's list on every frame in flora - every Azlin settings page in
//! the user's `flora:green` (SWITCH13: 9-11 display-list rebuilds per toggle
//! on AzCalculator's settings page against 2-3 in flat; in AzWidgets 11.5 ms a
//! frame in flora against 0.6 ms in flat).
//!
//! A browser repaints the one box whose background moves. The item a
//! gradient layer paints keeps its bounds while its colours move, so the
//! frame rewrites the item in place, exactly like a `Rect`'s colour.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    events::ProcessEventResult,
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::{color::ColorOrSystem, ColorU, PixelValue},
    property::CssProperty,
    style::{
        LinearGradient, NormalizedLinearColorStopVec, StyleBackgroundContent,
        StyleBackgroundContentVec, StyleTransform, StyleTransformVec,
    },
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    overlay::ContentChange,
    solver3::display_list::DisplayListItem,
    widgets::{switch::Switch, themes::UiTheme},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const TOP: ColorU = ColorU {
    r: 0x33,
    g: 0x66,
    b: 0xff,
    a: 0xff,
};
const BOTTOM: ColorU = ColorU {
    r: 0x11,
    g: 0x33,
    b: 0x99,
    a: 0xff,
};

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

fn epoch(lw: &LayoutWindow) -> u64 {
    lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .get_css_property_cache()
        .cascade_epoch
}

/// The linear gradient the current display list paints for `node`.
fn painted_face(lw: &LayoutWindow, node: NodeId) -> Option<LinearGradient> {
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;
    dl.items.iter().enumerate().find_map(|(i, item)| {
        if dl.node_mapping.get(i).copied().flatten() != Some(node) {
            return None;
        }
        match item {
            DisplayListItem::LinearGradient { gradient, .. } => Some(gradient.clone()),
            _ => None,
        }
    })
}

/// `face` with its stops recoloured `colors`, top to bottom: the same shape,
/// other colours - a face a fade can walk to stop by stop.
fn recoloured(face: &LinearGradient, colors: [ColorU; 2]) -> LinearGradient {
    let stops = face
        .stops
        .as_ref()
        .iter()
        .zip(colors)
        .map(|(stop, color)| {
            let mut stop = *stop;
            stop.color = ColorOrSystem::color(color);
            stop
        })
        .collect::<Vec<_>>();
    LinearGradient {
        stops: NormalizedLinearColorStopVec::from_vec(stops),
        ..face.clone()
    }
}

fn top_stop(face: &LinearGradient) -> ColorOrSystem {
    face.stops.as_ref()[0].color
}

/// The work an animation frame owes, done the way the shells' frame driver
/// does it: a frame that is neither patched nor values-only rebuilds the
/// display list (`PlatformWindow::advance_css_animations_now`).
fn frame_work(lw: &mut LayoutWindow) -> ProcessEventResult {
    let work = lw.take_animation_frame_work();
    if work == ProcessEventResult::ShouldUpdateDisplayListCurrentWindow {
        lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    }
    work
}

fn background(face: LinearGradient) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
        StyleBackgroundContent::LinearGradient(face),
    ]))
}

#[test]
fn a_gradient_face_fade_frame_is_patched_in_place_and_restyles_nothing() {
    let mut lw = window_with(Dom::create_body().with_child(
        Dom::create_div().with_class("face".into()).with_css(
            "width: 80px; height: 24px; \
             background: linear-gradient(to bottom, #eeeeee, #cccccc); \
             animation: background 150ms linear;",
        ),
    ));
    let face = node_with_class(&lw, "face");
    let rest = painted_face(&lw, face).expect("harness: the face paints its gradient");
    assert_eq!(rest.stops.as_ref().len(), 2, "harness: a two-stop face");
    let lit = recoloured(&rest, [TOP, BOTTOM]);

    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: face,
        props: vec![background(lit.clone())],
        override_only: false,
    });
    assert_eq!(
        lw.css_transitions.len(),
        1,
        "harness: the face declares a tween for its background"
    );

    let rebuilds = lw.frame_report.dl_rebuilds;
    let mut tops = Vec::new();
    for frame in 0..3 {
        let before = epoch(&lw);
        let _ = lw.tick_animations(0.016);
        let work = frame_work(&mut lw);
        assert_eq!(
            work,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            "frame {frame}: the face's gradient was patched into the display list in place - \
             the frame owes a repaint, not a rebuild (pending css dirt: {:?})",
            lw.pending_css_dirty
        );
        assert_eq!(
            epoch(&lw),
            before,
            "frame {frame}: a patched face frame must not re-run the cascade"
        );
        tops.push(painted_face(&lw, face).map(|f| top_stop(&f)));
    }
    assert_eq!(
        lw.frame_report.dl_rebuilds, rebuilds,
        "the fade's frames rebuilt no display list"
    );
    assert!(
        tops.iter()
            .any(|t| t.is_some_and(|c| c != top_stop(&rest) && c != top_stop(&lit))),
        "the painted face must pass between its two gradients, top stops per frame: {tops:?}"
    );

    for _ in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let _ = lw.tick_animations(0.016);
        let _ = frame_work(&mut lw);
    }
    assert_eq!(
        painted_face(&lw, face),
        Some(lit.clone()),
        "the settled fade paints the face it faded to"
    );
    // A list built after the fade for another reason (a caret blink, a hover)
    // paints from the styles: they must hold the face the patches showed.
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert_eq!(
        painted_face(&lw, face),
        Some(lit),
        "a list rebuilt after the fade paints the face it faded to"
    );
}

/// The flora Switch, toggled the way its click handler toggles it: the knob's
/// `transform` (a GPU value) and the track's face (a gradient). Both values
/// of each frame are bound by key or patched in place, so the glide owes a
/// repaint per frame - as the flat Switch's does.
#[test]
fn the_flora_switch_glides_on_a_repaint_per_frame() {
    let mut lw = window_with(
        Dom::create_body().with_child(Switch::create(false).with_theme(UiTheme::Flora).dom()),
    );
    let track = node_with_class(&lw, "__azul-native-switch");
    let knob = node_with_class(&lw, "__azul-native-switch-knob");
    let off_face = painted_face(&lw, track).expect("harness: the flora track paints a face");
    // The face of the switched-on track, as the theme builds it: what the click
    // handler writes (`switch::track_face`).
    let on_face = {
        let on = window_with(
            Dom::create_body().with_child(Switch::create(true).with_theme(UiTheme::Flora).dom()),
        );
        let on_track = node_with_class(&on, "__azul-native-switch");
        painted_face(&on, on_track).expect("harness: the switched-on track paints a face")
    };
    assert_ne!(off_face, on_face, "harness: the two faces differ");

    for (node, prop) in [
        (track, background(on_face.clone())),
        (
            knob,
            CssProperty::const_transform(StyleTransformVec::from_vec(vec![
                StyleTransform::TranslateX(PixelValue::const_px(16)),
            ])),
        ),
    ] {
        let _ = lw.apply_content_change(ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: node,
            props: vec![prop],
            override_only: false,
        });
    }
    assert_eq!(
        lw.css_transitions.len(),
        2,
        "harness: the track's face and the knob's slide both tween"
    );

    let rebuilds = lw.frame_report.dl_rebuilds;
    let mut frames = Vec::new();
    for frame in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let before = epoch(&lw);
        let _ = lw.tick_animations(0.016);
        let work = frame_work(&mut lw);
        assert_eq!(
            work,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            "frame {frame}: the knob's matrix is a GPU value and the track's face is patched in \
             place - the frame owes a repaint, not a display-list rebuild (pending css dirt: \
             {:?})",
            lw.pending_css_dirty
        );
        assert_eq!(
            epoch(&lw),
            before,
            "frame {frame}: a glide frame must not re-run the cascade"
        );
        let knob_x = lw
            .gpu_state_manager
            .painted_transform_of(DomId::ROOT_ID, knob)
            .and_then(|m| m.transform_point2d(LogicalPosition::zero()))
            .map(|p| p.x);
        frames.push((knob_x, painted_face(&lw, track).map(|f| top_stop(&f))));
    }
    assert_eq!(
        lw.frame_report.dl_rebuilds, rebuilds,
        "the glide's frames rebuilt no display list, frames: {frames:?}"
    );
    assert!(
        frames.iter().any(
            |(_, top)| top.is_some_and(|c| c != top_stop(&off_face) && c != top_stop(&on_face))
        ),
        "the track's face must pass between its two faces mid-glide, frames: {frames:?}"
    );
    assert_eq!(
        painted_face(&lw, track),
        Some(on_face),
        "the settled track paints the switched-on face"
    );
}
