//! A button FADES into its hover face and back, and shows its pressed face at
//! once (ANIM8, 2026-10-03; the user: "the hover animation over buttons, it
//! immediately transitions").
//!
//! The engine half is `LayoutWindow::seed_state_change_transitions`: a
//! `:hover` / `:active` restyle starts the transitions a node DECLARES. This
//! is the widget half: flat's and flora's Button declare a short fade of their
//! face (background and border colours) and declare `:active` at 0 ms, so a
//! press shows immediately - the way a native button reacts - and its release
//! fades back. A link has no face to fade (it underlines) and declares none.
//!
//! The check, per theme: lay a Button out, hover it the way the shell does
//! (`restyle_on_state_change` with the states snapshot before the flip), and
//! read which transitions the engine started.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{ActiveChange, HoverChange, StyledDom},
    window::{DarkLightMode, OptionDarkLightMode},
};
use azul_css::{props::property::CssPropertyType, system::defaults, AzString};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        button::{Button, ButtonType},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `button` alone in a 640x480 light window.
fn window(button: Button) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(defaults::macos_modern_light()));
    lw.mode = OptionDarkLightMode::None;
    let mut ws = FullWindowState {
        mode: DarkLightMode::Light,
        ..Default::default()
    };
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0;")
        .with_child(button.dom());
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    lw
}

/// The button's own node (the one carrying `__azul-native-button`).
fn button_node(lw: &LayoutWindow) -> NodeId {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    (0..styled.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            styled.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == "__azul-native-button"))
        })
        .expect("harness: a button node")
}

/// The pointer enters `node` (and its ancestors), as the shell applies it.
fn hover(lw: &mut LayoutWindow, node: NodeId) {
    let before = lw.node_states(DomId::ROOT_ID, [node]);
    let _ = lw
        .layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: Vec::new(),
                entered_nodes: vec![node],
            }),
            None,
        );
    let _ = lw.seed_state_change_transitions(DomId::ROOT_ID, &before);
}

/// The primary button goes down on `node`, as the shell applies it.
fn press(lw: &mut LayoutWindow, node: NodeId) {
    let before = lw.node_states(DomId::ROOT_ID, [node]);
    let _ = lw
        .layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .restyle_on_state_change(
            None,
            None,
            Some(ActiveChange {
                deactivated: Vec::new(),
                activated: vec![node],
            }),
        );
    let _ = lw.seed_state_change_transitions(DomId::ROOT_ID, &before);
}

fn fades(lw: &LayoutWindow, node: NodeId, ty: CssPropertyType) -> bool {
    lw.css_transitions
        .iter()
        .any(|t| t.node == node && t.prop_type == ty)
}

#[test]
fn a_button_fades_into_its_hover_face_in_both_themes() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        for kind in [ButtonType::Default, ButtonType::Primary] {
            let mut lw = window(Button::with_type(AzString::from("Save"), kind).with_theme(theme));
            let node = button_node(&lw);
            assert!(
                lw.css_transitions.is_empty(),
                "{theme:?} {kind:?}: harness - nothing moves before the pointer comes"
            );
            hover(&mut lw, node);
            assert!(
                fades(&lw, node, CssPropertyType::BackgroundContent),
                "{theme:?} {kind:?}: the pointer entering the button starts a fade of its face, \
                 not a snap (transitions: {:?})",
                lw.css_transitions
                    .iter()
                    .map(|t| t.prop_type.to_str())
                    .collect::<Vec<_>>()
            );
            // Half-way through, the face is still moving; at the end it rests
            // (flora's stone the slowest: light moves across it over
            // `--fl-dur-slow`, 1.2s).
            assert!(lw.tick_animations(0.05), "{theme:?} {kind:?}: the fade is in flight");
            let _ = lw.tick_animations(2.0);
            assert!(
                !fades(&lw, node, CssPropertyType::BackgroundContent),
                "{theme:?} {kind:?}: the fade ends"
            );
        }
    }
}

#[test]
fn a_pressed_button_shows_its_pressed_face_at_once() {
    // Flat: on the next frame. Flora: a press is its ONE fast movement
    // (flora.css's `--fl-dur-fast`, 0.14s) - never the hover's slow fade.
    const FLORA_PRESS_S: f32 = 0.14;
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        let mut lw = window(Button::create(AzString::from("Save")).with_theme(theme));
        let node = button_node(&lw);
        hover(&mut lw, node);
        let _ = lw.tick_animations(2.0);
        press(&mut lw, node);
        let press_fades: Vec<f32> = lw
            .css_transitions
            .iter()
            .filter(|t| t.node == node && t.prop_type == CssPropertyType::BackgroundContent)
            .map(|t| t.duration_s)
            .collect();
        match theme {
            UiTheme::Flat => assert!(
                press_fades.is_empty(),
                "{theme:?}: a press shows the pressed face on the next frame, it does not fade in"
            ),
            UiTheme::Flora => assert!(
                press_fades.iter().all(|d| *d <= FLORA_PRESS_S + 1e-3),
                "{theme:?}: a press is the fast movement, not the hover's fade: {press_fades:?}"
            ),
        }
    }
}

#[test]
fn a_link_declares_no_face_fade() {
    // Flat's link is text: it underlines on hover. Flora's (the QUIET
    // command, flora.css's `.btn-quiet`) is paper with a face, and fades it
    // like every flora command.
    let theme = UiTheme::Flat;
    let mut lw =
        window(Button::with_type(AzString::from("More"), ButtonType::Link).with_theme(theme));
    let node = button_node(&lw);
    hover(&mut lw, node);
    assert!(
        lw.css_transitions.iter().all(|t| t.node != node),
        "{theme:?}: a link underlines on hover - a discrete change, nothing to fade"
    );
}
